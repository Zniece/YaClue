const { invoke } = window.__TAURI__.core;

const $ = (selector) => document.querySelector(selector);
const modeEl = $("#mode");
const exprEl = $("#expr");
const variableEl = $("#variable");
const errorEl = $("#error");
const summaryEl = $("#summary");
const stepsEl = $("#steps-list");
const plotEl = $("#plot");
const rawBoxEl = $("#raw-box");
const rawEl = $("#raw");
const emptyEl = $("#empty-result");
const stateEl = $("#engine-state");
const timingEl = $("#timing");
const assumptions = new Map();

const MODES = {
  derivative: { title: "被求导表达式", help: "支持高阶导数和三种步骤粒度。", example: "Sin(x)^2" },
  integral: { title: "被积表达式", help: "展示换元、分部积分、部分分式和三角积分等已有步骤。", example: "x*Exp(x)" },
  definite: { title: "被积表达式", help: "解析积分失败时会尝试自适应辛普森数值积分。", example: "Sin(x)" },
  transform: { title: "待变换表达式", help: "Apart 使用上方变量，其余变换不使用变量参数。", example: "(x+1)^2" },
  equation: { title: "方程（每行一个）", help: "方程组每行一个等式；变量栏可填写 x,y。", example: "x+y==3\nx-y==1" },
  limit: { title: "极限表达式", help: "趋近值可填写 0、Infinity 等；支持左、右和双侧极限。", example: "Sin(x)/x" },
  plot: { title: "待绘制表达式", help: "使用批量采样和曲率细分；非有限点会断开曲线。", example: "Sin(x)" },
  evaluate: { title: "Yacas 表达式", help: "直接访问当前会话中的引擎，适合体验尚无专用界面的能力。", example: "Factor(x^4-1)" },
};

const STEP_MODES = new Set(["derivative", "integral", "definite"]);

function updateMode(setExample = true) {
  const mode = modeEl.value;
  document.querySelectorAll("[data-for]").forEach((element) => {
    const targets = element.dataset.for.split(" ");
    element.hidden = !(targets.includes(mode) || (targets.includes("steps") && STEP_MODES.has(mode)));
  });
  $("#expression-title").textContent = MODES[mode].title;
  $("#input-help").textContent = MODES[mode].help;
  if (setExample) exprEl.value = MODES[mode].example;
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
}

function showError(error) {
  errorEl.textContent = typeof error === "string" ? error : JSON.stringify(error);
  errorEl.hidden = false;
}

function renderMath(tex, target, displayMode = true) {
  window.katex.render(tex || "", target, { throwOnError: false, displayMode });
}

function renderSteps(steps) {
  steps.forEach((step, index) => {
    const item = document.createElement("article");
    item.className = `step importance-${step.importance}`;
    const heading = document.createElement("div");
    heading.className = "step-heading";
    heading.innerHTML = `<span>${index + 1}</span><strong></strong><small>${step.rule}</small>`;
    heading.querySelector("strong").textContent = step.why || "计算";
    const math = document.createElement("div");
    math.className = "math";
    item.append(heading, math);
    stepsEl.appendChild(item);
    renderMath(step.tex, math);
  });
}

function showStructured(result) {
  rawEl.textContent = JSON.stringify(result, null, 2);
  rawBoxEl.hidden = false;
}

function summary(title, tex, metadata = []) {
  summaryEl.hidden = false;
  const heading = document.createElement("h3");
  heading.textContent = title;
  const math = document.createElement("div");
  math.className = "summary-math";
  summaryEl.append(heading, math);
  renderMath(tex, math);
  if (metadata.length) {
    const meta = document.createElement("p");
    meta.className = "metadata";
    meta.textContent = metadata.join(" · ");
    summaryEl.appendChild(meta);
  }
}

function conditionText(condition) {
  if (condition.kind === "property") return `${condition.expression}: ${condition.fact}`;
  if (condition.kind === "relation") return `${condition.left} ${condition.relation} ${condition.right}`;
  return (condition.conditions || []).map(conditionText).join(condition.kind === "all" ? " 且 " : " 或 ");
}

function renderPlot(result) {
  plotEl.hidden = false;
  const ratio = window.devicePixelRatio || 1;
  const width = plotEl.clientWidth || 800;
  const height = 390;
  plotEl.width = width * ratio;
  plotEl.height = height * ratio;
  const ctx = plotEl.getContext("2d");
  ctx.scale(ratio, ratio);
  ctx.clearRect(0, 0, width, height);
  const finite = result.points.filter((point) => Number.isFinite(point.y));
  if (!finite.length) throw new Error("采样结果中没有有限点");
  const xs = result.points.map((point) => point.x);
  const ys = finite.map((point) => point.y).sort((a, b) => a - b);
  const xMin = Math.min(...xs);
  const xMax = Math.max(...xs);
  const low = ys[Math.floor(ys.length * 0.02)];
  const high = ys[Math.min(ys.length - 1, Math.ceil(ys.length * 0.98))];
  const span = Math.max(high - low, 1e-9);
  const yMin = low - span * 0.08;
  const yMax = high + span * 0.08;
  const px = (x) => 34 + ((x - xMin) / (xMax - xMin)) * (width - 52);
  const py = (y) => height - 24 - ((y - yMin) / (yMax - yMin)) * (height - 48);

  ctx.strokeStyle = getComputedStyle(document.documentElement).getPropertyValue("--border");
  ctx.lineWidth = 1;
  ctx.beginPath();
  if (xMin <= 0 && xMax >= 0) { ctx.moveTo(px(0), 12); ctx.lineTo(px(0), height - 18); }
  if (yMin <= 0 && yMax >= 0) { ctx.moveTo(26, py(0)); ctx.lineTo(width - 10, py(0)); }
  ctx.stroke();

  const breaks = new Set(result.breaks);
  ctx.strokeStyle = "#3767d6";
  ctx.lineWidth = 2;
  ctx.beginPath();
  let drawing = false;
  result.points.forEach((point, index) => {
    const visible = Number.isFinite(point.y) && point.y >= yMin && point.y <= yMax;
    if (!visible || breaks.has(index) || breaks.has(index - 1)) {
      drawing = false;
      return;
    }
    if (drawing) ctx.lineTo(px(point.x), py(point.y));
    else { ctx.moveTo(px(point.x), py(point.y)); drawing = true; }
  });
  ctx.stroke();
  summary("采样完成", "", [`${result.points.length} 个点`, `${result.breaks.length} 个断点`]);
}

async function calculate() {
  const expr = exprEl.value.trim();
  if (!expr) return;
  resetOutput();
  setBusy(true);
  const started = performance.now();
  try {
    const mode = modeEl.value;
    const variable = variableEl.value.trim();
    let result;
    if (STEP_MODES.has(mode)) {
      result = await invoke("calculate_steps", {
        request: {
          kind: mode,
          expr,
          variable,
          verbosity: $("#verbosity").value,
          order: Number($("#order").value),
          from: $("#from").value.trim(),
          to: $("#to").value.trim(),
        },
      });
      renderSteps(result);
    } else if (mode === "transform") {
      result = await invoke("transform_expression", { expr, operation: $("#operation").value, variable });
      summary(result.operation, result.tex, [result.changed ? "表达式已改变" : "表达式未改变", result.unresolved ? "未完成" : "已完成"]);
      showStructured(result);
    } else if (mode === "equation") {
      const equations = expr.split(/\n+/).map((line) => line.trim()).filter(Boolean);
      const variables = variable.split(",").map((item) => item.trim()).filter(Boolean);
      result = await invoke("solve_equations", { equations, variables });
      summary("方程结果", result.tex, [`状态：${result.status}`, `${result.solutions.length} 组解`]);
      showStructured(result);
    } else if (mode === "limit") {
      result = await invoke("calculate_limit", { expr, variable, at: $("#at").value.trim(), direction: $("#direction").value });
      const conditions = result.conditions.map(conditionText);
      summary("极限结果", result.tex, [`状态：${result.status}`, ...conditions]);
      showStructured(result);
    } else if (mode === "plot") {
      result = await invoke("sample_plot", { expr, variable, min: Number($("#plot-min").value), max: Number($("#plot-max").value) });
      renderPlot(result);
      showStructured({ points: result.points.length, breaks: result.breaks });
    } else {
      result = await invoke("evaluate", { expr });
      summary("求值结果", result.tex, [result.expression]);
      showStructured(result);
    }
  } catch (error) {
    showError(error);
  } finally {
    timingEl.textContent = `${Math.round(performance.now() - started)} ms`;
    setBusy(false);
  }
}

function renderAssumptions() {
  const list = $("#assumption-list");
  list.innerHTML = "";
  if (!assumptions.size) {
    list.innerHTML = '<span class="empty">暂无假设</span>';
    return;
  }
  assumptions.forEach(({ fact, symbol }) => {
    const chip = document.createElement("span");
    chip.className = "chip";
    chip.textContent = `${symbol}: ${fact}`;
    list.appendChild(chip);
  });
}

async function addAssumption() {
  try {
    const symbol = $("#assumption-symbol").value.trim();
    const result = await invoke("set_assumption", { symbol, fact: $("#assumption-fact").value });
    assumptions.set(`${result.symbol}:${result.fact}`, result);
    renderAssumptions();
  } catch (error) {
    resetOutput();
    showError(error);
  }
}

async function clearAssumptions() {
  try {
    await invoke("clear_assumptions");
    assumptions.clear();
    renderAssumptions();
  } catch (error) {
    resetOutput();
    showError(error);
  }
}

modeEl.addEventListener("change", () => updateMode(true));
$("#go").addEventListener("click", calculate);
$("#add-assumption").addEventListener("click", addAssumption);
$("#clear-assumptions").addEventListener("click", clearAssumptions);
exprEl.addEventListener("keydown", (event) => {
  if ((event.ctrlKey || event.metaKey) && event.key === "Enter") calculate();
});
updateMode(false);
