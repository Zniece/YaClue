const { invoke } = window.__TAURI__.core;
const $ = (selector) => document.querySelector(selector);
const exprEl = $("#expr");
const errorEl = $("#error");
const summaryEl = $("#summary");
const stepsEl = $("#steps-list");
const plotEl = $("#plot");
const rawBoxEl = $("#raw-box");
const rawEl = $("#raw");
const emptyEl = $("#empty-result");
const stateEl = $("#engine-state");
const timingEl = $("#timing");
const semanticEl = $("#semantic-summary");
const assumptions = new Map();
let calculating = false;
let inputHelpTimer;

const TEMPLATES = [
  ["求导", "D(x)|", "D(变量[,阶数])表达式"],
  ["不定积分", "Integrate(x)|", "Integrate(变量)表达式"],
  ["定积分", "Integrate(x,0,Pi)|", "Integrate(变量,下限,上限)表达式"],
  ["极限", "Limit(x,0)|", "Limit(变量,趋近值[,方向])表达式"],
  ["Taylor 展开", "Taylor(x,0,6)|", "Taylor(变量,展开点,次数)表达式"],
  ["二重积分", "DoubleIntegral(|x+y,y,0,x,x,0,1)", "被积式,内层变量与上下限,外层变量与上下限"],
  ["极坐标积分", "PolarIntegral(|x^2+y^2,x,y,r,t,0,1,0,2*Pi)", "被积式,直角变量,极坐标变量及边界"],
  ["代数方程", "|x^2-5*x+6==0", "使用 == 表示等号"],
  ["方程组", "Solve({|x+y==3,x-y==1},{x,y})", "Solve({方程...},{变量...})"],
  ["常微分方程", "OdeSolve(|y'==y)", "当前标准形式使用自变量 x、因变量 y"],
  ["ODE 数值解", "OdeSolveNumeric(|y'==y,x,y,0,1,2)", "方程,自变量,因变量,起点,初值,终点"],
  ["因式分解", "Factor(|)", "Factor(表达式)"],
  ["展开", "Expand(|)", "Expand(表达式)"],
  ["化简", "Simplify(|)", "Simplify(表达式)"],
  ["整理", "Tidy(|)", "Tidy(表达式)"],
  ["部分分式", "Apart(|,x)", "Apart(表达式,变量)"],
  ["无约束极值", "Extrema(|x^2+y^2,x,y)", "Extrema(表达式,x变量,y变量)"],
  ["约束极值", "Lagrange(|x+y,x^2+y^2==1,x,y)", "Lagrange(目标,约束,x变量,y变量)"],
  ["矩阵乘法", "|{{1,2},{3,4}}*{{5,6},{7,8}}", "直接使用 + 或 *"],
  ["行列式", "Determinant(|{{1,2},{3,4}})", "Determinant(矩阵)"],
  ["逆矩阵", "Inverse(|{{1,2},{3,4}})", "Inverse(矩阵)"],
  ["转置", "Transpose(|{{1,2,3},{4,5,6}})", "Transpose(矩阵)"],
  ["特征值", "EigenValues(|{{2,1},{1,2}})", "EigenValues(矩阵)"],
  ["线性方程组", "MatrixSolve(|{{2,1},{1,-1}},{5,1})", "MatrixSolve(系数矩阵,常数向量)"],
  ["高精度近似", "N(|Pi,30)", "N(表达式,精度)"],
  ["数值求根", "FindRoot(|Cos(x)-x,x,1)", "FindRoot(表达式,变量,初值)"],
  ["函数绘图", "Plot(|Sin(x),x,-6.28,6.28)", "Plot(表达式,变量,下界,上界)"],
];

function hideInputHelp() {
  clearTimeout(inputHelpTimer);
  inputHelpTimer = undefined;
  $("#input-help").hidden = true;
}

function showInputHelp(message) {
  const helpEl = $("#input-help");
  clearTimeout(inputHelpTimer);
  helpEl.textContent = message;
  helpEl.hidden = false;
  inputHelpTimer = setTimeout(hideInputHelp, 4000);
}

function insertTemplate(textarea, markedTemplate) {
  const marker = markedTemplate.indexOf("|");
  const template = markedTemplate.replace("|", "");
  const start = textarea.selectionStart ?? textarea.value.length;
  const end = textarea.selectionEnd ?? start;
  textarea.setRangeText(template, start, end, "end");
  const caret = start + (marker === -1 ? template.length : marker);
  textarea.setSelectionRange(caret, caret);
  textarea.dispatchEvent(new Event("input", { bubbles: true }));
}

function renderTemplates() {
  const target = $("#template-list");
  target.innerHTML = "";
  TEMPLATES.forEach(([label, expression, help]) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "shortcut";
    button.textContent = label;
    button.title = help;
    button.addEventListener("click", () => {
      insertTemplate(exprEl, expression);
      showInputHelp(help);
      exprEl.focus();
    });
    target.appendChild(button);
  });
}

function setBusy(busy) {
  $("#go").disabled = busy;
  stateEl.textContent = busy ? "正在计算" : "引擎就绪";
  stateEl.classList.toggle("busy", busy);
}

function resetOutput() {
  errorEl.hidden = true;
  errorEl.textContent = "";
  summaryEl.hidden = true;
  summaryEl.innerHTML = "";
  stepsEl.innerHTML = "";
  plotEl.hidden = true;
  rawBoxEl.hidden = true;
  rawEl.textContent = "";
  emptyEl.hidden = true;
  semanticEl.hidden = true;
  semanticEl.innerHTML = "";
  $("#result-kind").textContent = "";
}

function showError(error) {
  const message = error && typeof error === "object" && error.message
    ? error.message
    : typeof error === "string" ? error : JSON.stringify(error);
  const retry = error?.code === "timeout" || error?.retryable ? "（可以重试）" : "";
  errorEl.textContent = `${message}${retry}`;
  errorEl.hidden = false;
}

function renderMath(tex, target, displayMode = true) {
  window.katex.render(tex || "", target, { throwOnError: false, displayMode });
}

function renderSummary(result) {
  summaryEl.hidden = false;
  const domain = result.data?.result || result.data;
  const representations = domain?.representations || [];
  if (representations.length > 1) {
    const control = document.createElement("label");
    control.className = "representation-switch";
    control.textContent = "解的形式";
    const select = document.createElement("select");
    const names = {
      real_basis: "实数形式",
      complex_exponential: "复指数形式",
    };
    representations.forEach((representation) => {
      const option = document.createElement("option");
      option.value = representation.kind;
      option.textContent = names[representation.kind] || representation.kind;
      option.selected = representation.kind === domain.preferred_representation;
      select.appendChild(option);
    });
    control.appendChild(select);
    summaryEl.appendChild(control);
    select.addEventListener("change", () => {
      const selected = representations.find((item) => item.kind === select.value);
      if (selected) renderMath(selected.tex, math);
    });
  }
  const math = document.createElement("div");
  math.className = "summary-math";
  summaryEl.appendChild(math);
  if (result.tex) renderMath(result.tex, math);
  else math.textContent = result.expression || "计算完成";
}

function renderSemantic(semantic, outcome) {
  if (!semantic) return;
  const kindNames = {
    scalar: "标量",
    expression: "符号表达式",
    equation: "方程",
    matrix: "矩阵",
    solution_set: "解集",
    function_family: "函数族",
    unevaluated: "未求值对象",
  };
  const exactnessNames = {
    exact: "精确",
    symbolic: "符号",
    approximate: "近似",
    unknown: "精确性未知",
  };
  const items = [kindNames[semantic.kind] || semantic.kind];
  if (semantic.shape) items.push(`${semantic.shape.rows} × ${semantic.shape.columns}`);
  items.push(exactnessNames[semantic.exactness] || semantic.exactness);
  if (semantic.symbols?.length) items.push(`符号：${semantic.symbols.join(", ")}`);
  if (semantic.bound_symbols?.length) items.push(`绑定：${semantic.bound_symbols.join(", ")}`);
  if (semantic.constants?.length) items.push(`常量：${semantic.constants.join(", ")}`);
  const reasonNames = {
    condition_insufficient: "条件不足",
    algorithm_uncovered: "算法未覆盖",
    mathematical_absence: "数学上不存在",
    divergent: "发散",
    unsupported_operation: "不支持的运算",
  };
  if (outcome?.conditionality === "conditional") items.push("条件化结果");
  if (outcome?.completeness === "representative") items.push("代表解");
  if (outcome?.reason) items.push(reasonNames[outcome.reason] || outcome.reason);
  items.forEach((text) => {
    const chip = document.createElement("span");
    chip.textContent = text;
    semanticEl.appendChild(chip);
  });
  semanticEl.hidden = false;
}

function renderSteps(steps) {
  steps.forEach((step, index) => {
    const item = document.createElement("article");
    item.className = `step importance-${step.importance}`;
    const heading = document.createElement("div");
    heading.className = "step-heading";
    heading.innerHTML = `<span>${index + 1}</span><div><strong></strong><small></small></div>`;
    heading.querySelector("strong").textContent = step.why || "计算";
    heading.querySelector("small").textContent = step.rule;
    const math = document.createElement("div");
    math.className = "math";
    item.append(heading, math);
    stepsEl.appendChild(item);
    renderMath(step.tex, math);
  });
}

function renderPlot(data) {
  const points = data.kind === "numeric_ode"
    ? data.data.points.map((point) => ({ x: point.independent, y: point.state[0] }))
    : data.data.points;
  plotEl.hidden = false;
  const ratio = window.devicePixelRatio || 1;
  const width = plotEl.clientWidth || 800;
  const height = Math.min(390, Math.max(280, width * 0.48));
  plotEl.width = width * ratio;
  plotEl.height = height * ratio;
  const ctx = plotEl.getContext("2d");
  ctx.scale(ratio, ratio);
  const finite = points.filter((point) => Number.isFinite(point.y));
  if (!finite.length) throw new Error("采样结果中没有有限点");
  const xs = points.map((point) => point.x);
  const ys = finite.map((point) => point.y).sort((a, b) => a - b);
  const xMin = Math.min(...xs), xMax = Math.max(...xs);
  const low = ys[Math.floor(ys.length * 0.02)], high = ys[Math.min(ys.length - 1, Math.ceil(ys.length * 0.98))];
  const span = Math.max(high - low, 1e-9), yMin = low - span * 0.08, yMax = high + span * 0.08;
  const xSpan = Math.max(xMax - xMin, 1e-9);
  const px = (x) => 34 + ((x - xMin) / xSpan) * (width - 52);
  const py = (y) => height - 24 - ((y - yMin) / (yMax - yMin)) * (height - 48);
  ctx.clearRect(0, 0, width, height);
  ctx.strokeStyle = "#d7deea";
  ctx.beginPath();
  if (xMin <= 0 && xMax >= 0) { ctx.moveTo(px(0), 12); ctx.lineTo(px(0), height - 18); }
  if (yMin <= 0 && yMax >= 0) { ctx.moveTo(26, py(0)); ctx.lineTo(width - 10, py(0)); }
  ctx.stroke();
  ctx.strokeStyle = "#315fc5";
  ctx.lineWidth = 2;
  ctx.beginPath();
  let drawing = false;
  points.forEach((point) => {
    if (!Number.isFinite(point.y) || point.y < yMin || point.y > yMax) drawing = false;
    else if (drawing) ctx.lineTo(px(point.x), py(point.y));
    else { ctx.moveTo(px(point.x), py(point.y)); drawing = true; }
  });
  ctx.stroke();
}

async function calculate() {
  hideInputHelp();
  if (calculating) return;
  const expression = exprEl.value.trim();
  if (!expression) return;
  calculating = true;
  resetOutput();
  setBusy(true);
  const started = performance.now();
  try {
    const result = await invoke("process_expression", {
      request: {
        expression,
        steps: $("#output-mode").value === "steps",
        verbosity: $("#verbosity").value,
      },
    });
    $("#result-title").textContent = result.title;
    $("#result-kind").textContent = result.kind.replaceAll("_", " ");
    renderSemantic(result.semantic, result.outcome);
    if (result.kind === "plot" || result.kind === "numeric_ode") renderPlot(result);
    else renderSummary(result);
    renderSteps(result.steps || []);
    rawEl.textContent = JSON.stringify(result, null, 2);
    rawBoxEl.hidden = false;
  } catch (error) {
    showError(error);
  } finally {
    timingEl.textContent = `${Math.round(performance.now() - started)} ms`;
    setBusy(false);
    calculating = false;
  }
}

function renderAssumptions() {
  const list = $("#assumption-list");
  list.innerHTML = "";
  if (!assumptions.size) list.innerHTML = '<span class="empty">暂无假设</span>';
  assumptions.forEach(({ fact, symbol }) => {
    const chip = document.createElement("span");
    chip.className = "chip";
    chip.textContent = `${symbol}: ${fact}`;
    list.appendChild(chip);
  });
  $("#assumption-summary").textContent = assumptions.size ? `${assumptions.size} 项假设` : "无假设";
}

async function refreshAssumptions() {
  const states = await invoke("get_assumptions");
  assumptions.clear();
  states.forEach((state) => assumptions.set(`${state.symbol}:${state.fact}`, state));
  renderAssumptions();
}

async function addAssumption() {
  try {
    await invoke("set_assumption", { symbol: $("#assumption-symbol").value.trim(), fact: $("#assumption-fact").value });
    await refreshAssumptions();
  } catch (error) { resetOutput(); showError(error); }
}

async function clearAssumptions() {
  try { await invoke("clear_assumptions"); await refreshAssumptions(); }
  catch (error) { resetOutput(); showError(error); }
}

$("#output-mode").addEventListener("change", () => { $("#verbosity-field").hidden = $("#output-mode").value !== "steps"; });
$("#clear-expression").addEventListener("click", () => { exprEl.value = ""; hideInputHelp(); exprEl.focus(); });
$("#go").addEventListener("click", calculate);
$("#add-assumption").addEventListener("click", addAssumption);
$("#clear-assumptions").addEventListener("click", clearAssumptions);
exprEl.addEventListener("keydown", (event) => { if ((event.ctrlKey || event.metaKey) && event.key === "Enter") calculate(); });
exprEl.addEventListener("input", hideInputHelp);
renderTemplates();
refreshAssumptions().catch(showError);
