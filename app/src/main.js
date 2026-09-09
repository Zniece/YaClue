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
  equation: { title: "方程（每行一个）", help: "方程组每行一个等式；求解变量可留空自动发现，或填写 x,y 显式指定。", example: "x+y==3\nx-y==1" },
  limit: { title: "极限表达式", help: "趋近值可填写 0、Infinity 等；支持左、右和双侧极限。", example: "Sin(x)/x" },
  limit_steps: { title: "极限表达式", help: "展示连续洛必达、乘积/差/幂型不定式变换和参数条件。", example: "(1-Cos(x))/x^2" },
  ode: { title: "常微分方程", help: "自变量使用上方输入，另行指定因变量；初值格式为 阶数,点,值。", example: "y'==(x+y)/x" },
  ode_steps: { title: "常微分方程", help: "展示求解器实际产生的标准形、代换、积分因子和验算事件。", example: "y'+y==x" },
  ode_numeric: { title: "常微分方程初值问题", help: "使用自适应数值积分；初值数量需要与方程阶数一致。", example: "y'+y==x" },
  approximate: { title: "数值表达式", help: "按指定十进制精度近似，最高 1000 位。", example: "Pi" },
  root: { title: "等于零的表达式", help: "Newton 数值求根；区间上下界可同时留空。", example: "Cos(x)-x" },
  taylor: { title: "待展开表达式", help: "生成指定点和次数的 Taylor 多项式。", example: "Sin(x)" },
  matrix: { title: "左矩阵", help: "使用 Yacas 列表矩阵语法；二元运算填写右操作数。", example: "{{1,2},{3,4}}" },
  plot: { title: "待绘制表达式", help: "使用批量采样和曲率细分；非有限点会断开曲线。", example: "Sin(x)" },
  evaluate: { title: "Yacas 表达式", help: "直接访问当前会话中的引擎，适合体验尚无专用界面的能力。", example: "Factor(x^4-1)" },
};

const STEP_MODES = new Set(["derivative", "integral", "definite", "limit_steps", "ode_steps"]);

function updateMode(setExample = true) {
  const mode = modeEl.value;
  document.querySelectorAll("[data-for]").forEach((element) => {
    const targets = element.dataset.for.split(" ");
    element.hidden = !(targets.includes(mode) || (targets.includes("steps") && STEP_MODES.has(mode)));
  });
  $("#expression-title").textContent = MODES[mode].title;
  $("#input-help").textContent = MODES[mode].help;
  variableEl.placeholder = mode === "equation" ? "自动发现（如 x,y）" : "x";
  if (setExample) {
    exprEl.value = MODES[mode].example;
    variableEl.value = mode === "equation" ? "" : "x";
    if (["ode", "ode_steps", "ode_numeric"].includes(mode)) {
      $("#dependent").value = "y";
      if (mode === "ode_numeric") $("#ode-conditions").value = "0,0,1";
    }
  }
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
  if (error && typeof error === "object" && error.message) {
    const suffix = error.code === "timeout"
      ? "（计算超时，可以重试）"
      : error.retryable
        ? "（可以重试）"
        : "";
    errorEl.textContent = `${error.message}${suffix}`;
  } else {
    errorEl.textContent = typeof error === "string" ? error : JSON.stringify(error);
  }
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

function parseOdeConditions(value) {
  if (!value.trim()) return [];
  return value.split(/[;\n]+/).map((item) => {
    const fields = item.split(",").map((field) => field.trim());
    if (fields.length !== 3) throw new Error("初值条件格式应为：阶数,点,值；多项用分号分隔");
    const derivativeOrder = Number(fields[0]);
    if (!Number.isInteger(derivativeOrder) || derivativeOrder < 0) throw new Error("初值导数阶数必须是非负整数");
    return { derivative_order: derivativeOrder, point: fields[1], value: fields[2] };
  });
}

function optionalNumber(selector) {
  const value = $(selector).value.trim();
  return value === "" ? null : Number(value);
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
  const bounds = result.suggested_bounds;
  const xs = result.points.map((point) => point.x);
  const ys = finite.map((point) => point.y).sort((a, b) => a - b);
  const xMin = bounds?.x_min ?? Math.min(...xs);
  const xMax = bounds?.x_max ?? Math.max(...xs);
  const low = ys[Math.floor(ys.length * 0.02)];
  const high = ys[Math.min(ys.length - 1, Math.ceil(ys.length * 0.98))];
  const span = Math.max(high - low, 1e-9);
  const yMin = bounds?.y_min ?? low - span * 0.08;
  const yMax = bounds?.y_max ?? high + span * 0.08;
  const px = (x) => 34 + ((x - xMin) / (xMax - xMin)) * (width - 52);
  const py = (y) => height - 24 - ((y - yMin) / (yMax - yMin)) * (height - 48);

  ctx.strokeStyle = getComputedStyle(document.documentElement).getPropertyValue("--border");
  ctx.lineWidth = 1;
  ctx.beginPath();
  if (xMin <= 0 && xMax >= 0) { ctx.moveTo(px(0), 12); ctx.lineTo(px(0), height - 18); }
  if (yMin <= 0 && yMax >= 0) { ctx.moveTo(26, py(0)); ctx.lineTo(width - 10, py(0)); }
  ctx.stroke();

  ctx.strokeStyle = "#3767d6";
  ctx.lineWidth = 2;
  ctx.beginPath();
  if (result.segments?.length) {
    result.segments.forEach((segment) => {
      let drawing = false;
      for (let index = segment.start_index; index <= segment.end_index; index += 1) {
        const point = result.points[index];
        const visible = point.y >= yMin && point.y <= yMax;
        if (!visible) {
          drawing = false;
        } else if (drawing) {
          ctx.lineTo(px(point.x), py(point.y));
        } else {
          ctx.moveTo(px(point.x), py(point.y));
          drawing = true;
        }
      }
    });
  } else {
    const breaks = new Set(result.breaks || []);
    let drawing = false;
    result.points.forEach((point, index) => {
      const visible = Number.isFinite(point.y) && point.y >= yMin && point.y <= yMax;
      if (!visible || breaks.has(index) || breaks.has(index - 1)) {
        drawing = false;
      } else if (drawing) {
        ctx.lineTo(px(point.x), py(point.y));
      } else {
        ctx.moveTo(px(point.x), py(point.y));
        drawing = true;
      }
    });
  }
  ctx.stroke();
  const details = [`${result.points.length} 个点`, `${result.breaks?.length || 0} 个断点`];
  if (result.segments) details.push(`${result.segments.length} 个连续区段`);
  if (result.termination && result.termination !== "complete") details.push(`终止：${result.termination}`);
  summary("采样完成", "", details);
}

function renderNumericOdePlot(result) {
  renderPlot({
    points: result.points.map((point) => ({ x: point.independent, y: point.state[0] })),
    breaks: [],
  });
  summaryEl.innerHTML = "";
  summary("数值 ODE 结果", "", [
    `状态：${result.status}`,
    `阶数：${result.order}`,
    `接受 ${result.accepted_steps} 步`,
    `拒绝 ${result.rejected_steps} 步`,
    `${result.evaluations} 次求值`,
    `估计误差：${result.estimated_error}`,
  ]);
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
    if (["derivative", "integral", "definite"].includes(mode)) {
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
      const source = result.variable_source === "inferred" ? "自动发现" : "显式指定";
      const details = [
        `状态：${result.status}`,
        `完整性：${result.completeness}`,
        `求解变量：${result.variables.join(", ")}（${source}）`,
        `${result.solutions.length} 组解`,
      ];
      if (result.parameters.length) details.push(`参数化结果：${result.parameters.join(", ")}`);
      summary("方程结果", result.tex, details);
      showStructured(result);
    } else if (mode === "limit") {
      result = await invoke("calculate_limit", { expr, variable, at: $("#at").value.trim(), direction: $("#direction").value });
      const conditions = result.conditions.map(conditionText);
      summary("极限结果", result.tex, [`状态：${result.status}`, ...conditions]);
      showStructured(result);
    } else if (mode === "limit_steps") {
      result = await invoke("calculate_limit_steps", {
        expr,
        variable,
        at: $("#at").value.trim(),
        direction: $("#direction").value,
        verbosity: $("#verbosity").value,
      });
      renderSteps(result);
      showStructured(result);
    } else if (mode === "ode" || mode === "ode_steps") {
      const request = {
        equation: expr,
        independent: variable,
        dependent: $("#dependent").value.trim(),
        initialConditions: parseOdeConditions($("#ode-conditions").value),
      };
      if (mode === "ode_steps") {
        result = await invoke("solve_ode_steps", { ...request, verbosity: $("#verbosity").value });
        summary("ODE 结果", result.result.tex, [`状态：${result.result.status}`, `方法：${result.result.method}`, `${result.result.solutions.length} 个分支`]);
        renderSteps(result.steps);
      } else {
        result = await invoke("solve_ode", request);
        summary("ODE 结果", result.tex, [`状态：${result.status}`, `方法：${result.method}`, `${result.solutions.length} 个分支`]);
      }
      showStructured(result);
    } else if (mode === "ode_numeric") {
      result = await invoke("solve_ode_numeric", {
        equation: expr,
        independent: variable,
        dependent: $("#dependent").value.trim(),
        initialConditions: parseOdeConditions($("#ode-conditions").value),
        options: {
          end: Number($("#ode-end").value),
          initialStep: optionalNumber("#ode-step"),
          absoluteTolerance: optionalNumber("#ode-absolute-tolerance"),
          relativeTolerance: optionalNumber("#ode-relative-tolerance"),
          maxSteps: null,
          maxEvaluations: null,
        },
      });
      renderNumericOdePlot(result);
      showStructured(result);
    } else if (mode === "approximate") {
      result = await invoke("approximate_numeric", { expr, precisionDigits: Number($("#precision").value) });
      summary("数值近似", result.tex, [`类型：${result.kind}`, `精度：${result.precision_digits} 位`]);
      showStructured(result);
    } else if (mode === "root") {
      result = await invoke("find_numeric_root", {
        expr,
        variable,
        initial: Number($("#root-initial").value),
        accuracy: Number($("#root-accuracy").value),
        min: optionalNumber("#root-min"),
        max: optionalNumber("#root-max"),
      });
      summary("数值根", result.tex, [`状态：${result.status}`]);
      showStructured(result);
    } else if (mode === "taylor") {
      result = await invoke("calculate_taylor", {
        expr,
        variable,
        point: $("#taylor-point").value.trim(),
        degree: Number($("#taylor-degree").value),
      });
      summary("Taylor 多项式", result.tex, [`次数：${result.degree}`, result.unresolved ? "未完成" : "已完成"]);
      showStructured(result);
    } else if (mode === "matrix") {
      const operation = $("#matrix-operation").value;
      const binary = ["add", "multiply", "solve"].includes(operation);
      result = await invoke("calculate_matrix", {
        left: expr,
        operation,
        right: binary ? $("#matrix-right").value.trim() : null,
      });
      summary("线性代数结果", result.tex, [`运算：${result.operation}`, result.unresolved ? "未完成" : "已完成"]);
      showStructured(result);
    } else if (mode === "plot") {
      result = await invoke("sample_plot", { expr, variable, min: Number($("#plot-min").value), max: Number($("#plot-max").value) });
      renderPlot(result);
      showStructured({
        points: result.points.length,
        breaks: result.breaks,
        discontinuities: result.discontinuities,
        segments: result.segments,
        suggested_bounds: result.suggested_bounds,
        evaluations: result.evaluations,
        termination: result.termination,
      });
    } else {
      result = await invoke("evaluate", { expr });
      summary("求值结果", result.tex, [result.expression]);
      showStructured(result);
      await refreshAssumptions();
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

async function refreshAssumptions() {
  const states = await invoke("get_assumptions");
  assumptions.clear();
  states.forEach((state) => assumptions.set(`${state.symbol}:${state.fact}`, state));
  renderAssumptions();
}

async function addAssumption() {
  try {
    const symbol = $("#assumption-symbol").value.trim();
    await invoke("set_assumption", { symbol, fact: $("#assumption-fact").value });
    await refreshAssumptions();
  } catch (error) {
    resetOutput();
    showError(error);
  }
}

async function clearAssumptions() {
  try {
    await invoke("clear_assumptions");
    await refreshAssumptions();
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
refreshAssumptions().catch(showError);
