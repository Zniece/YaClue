import { applyTranslations, getLocale, hasTranslation, setLocale, t } from "./i18n.js";

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
let lastResult;

const TEMPLATES = [
  ["derivative", "D(x)x^3*Sin(x)"], ["indefiniteIntegral", "Integrate(x)x*Exp(x)"],
  ["definiteIntegral", "Integrate(x,0,Pi)Sin(x)"], ["limit", "Limit(Sin(x)/x,0)"],
  ["taylor", "Taylor(Exp(x),0,6)"], ["doubleIntegral", "DoubleIntegral(x*y,y,0,x,x,0,1)"],
  ["polarIntegral", "PolarIntegral(x^2+y^2,x,y,r,t,0,1,0,2*Pi)"],
  ["solveEquation", "Solve(x^2-5*x+6==0,x)"], ["equationSystem", "Solve({x+y==3,x-y==1},{x,y})"],
  ["ode", "OdeSolve(y''+4*y==Sin(x))"], ["numericOde", "OdeSolveNumeric(y'==y,x,y,0,1,2)"],
  ["factor", "Factor(x^4-1)"], ["expand", "Expand((x+1)^4)"],
  ["simplify", "Simplify((x^2-1)/(x-1))"], ["tidy", "Tidy((x+x)/2+x^2-x^2)"],
  ["apart", "Apart(1/(x^2-1),x)"], ["extrema", "Extrema(x^2+y^2-2*x+4*y,x,y)"],
  ["lagrange", "Lagrange(x+y,x^2+y^2-1,x,y)"], ["matrixMultiply", "{{1,2},{3,4}}*{{5,6},{7,8}}"],
  ["determinant", "Determinant({{1,2},{3,4}})"], ["inverse", "Inverse({{1,2},{3,4}})"],
  ["transpose", "Transpose({{1,2,3},{4,5,6}})"], ["eigenvalues", "EigenValues({{2,1},{1,2}})"],
  ["matrixSolve", "MatrixSolve({{2,1},{1,-1}},{5,1})"], ["numeric", "N(Pi,30)"],
  ["findRoot", "FindRoot(Cos(x)-x,x,1)"], ["plot", "Plot(Sin(x)+Cos(2*x)/2,x,-6.28,6.28)"],
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
  TEMPLATES.forEach(([key, expression]) => {
    const label = t(`template.${key}.label`);
    const help = t(`template.${key}.help`);
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
  stateEl.textContent = t(busy ? "engineBusy" : "engineReady");
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
  const retry = error?.code === "timeout" || error?.retryable ? t("retry") : "";
  errorEl.textContent = `${message}${retry}`;
  errorEl.hidden = false;
}

function renderMath(tex, target, displayMode = true) {
  window.katex.render(tex || "", target, { throwOnError: false, displayMode });
}

function renderSummary(result) {
  summaryEl.hidden = false;
  const math = document.createElement("div");
  math.className = "summary-math";
  summaryEl.appendChild(math);
  if (result.tex) renderMath(result.tex, math);
  else math.textContent = result.expression || t("completed");
}

function localizedResultLabel(result) {
  return hasTranslation(result.title_key) ? t(result.title_key) : result.title;
}

function renderSemantic(semantic, outcome) {
  if (!semantic) return;
  const kindNames = {
    scalar: t("kind_scalar"), expression: t("kind_expression"), equation: t("kind_equation"),
    matrix: t("kind_matrix"), solution_set: t("kind_solution_set"),
    function_family: t("kind_function_family"), unevaluated: t("kind_unevaluated"),
  };
  const exactnessNames = {
    exact: t("exact_exact"), symbolic: t("exact_symbolic"), approximate: t("exact_approximate"), unknown: t("exact_unknown"),
  };
  const items = [kindNames[semantic.kind] || semantic.kind];
  if (semantic.shape) items.push(`${semantic.shape.rows} × ${semantic.shape.columns}`);
  items.push(exactnessNames[semantic.exactness] || semantic.exactness);
  if (semantic.symbols?.length) items.push(t("symbols", { value: semantic.symbols.join(", ") }));
  if (semantic.bound_symbols?.length) items.push(t("boundSymbols", { value: semantic.bound_symbols.join(", ") }));
  if (semantic.constants?.length) items.push(t("constants", { value: semantic.constants.join(", ") }));
  const reasonNames = {
    condition_insufficient: t("reason.condition_insufficient"),
    algorithm_uncovered: t("reason.algorithm_uncovered"),
    mathematical_absence: t("reason.mathematical_absence"),
    divergent: t("reason.divergent"),
    unsupported_operation: t("reason.unsupported_operation"),
  };
  if (outcome?.conditionality === "conditional") items.push(t("conditional"));
  if (outcome?.completeness === "representative") items.push(t("representative"));
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
    heading.querySelector("strong").textContent = step.why || t("computation");
    heading.querySelector("small").textContent = step.rule;
    const math = document.createElement("div");
    math.className = "math";
    item.append(heading, math);
    stepsEl.appendChild(item);
    const transformation = step.before_tex
      ? `${step.before_tex}\\;\\Longrightarrow\\;${step.tex}`
      : step.tex;
    renderMath(transformation, math);
  });
}

function renderConclusions(conclusions) {
  conclusions.forEach((conclusion) => {
    const item = document.createElement("article");
    item.className = "step conclusion";
    const heading = document.createElement("div");
    heading.className = "step-heading";
    const label = document.createElement("strong");
    label.textContent = conclusion.message;
    const math = document.createElement("div");
    math.className = "math";
    heading.appendChild(label);
    item.append(heading, math);
    stepsEl.appendChild(item);
    renderMath(conclusion.tex, math);
  });
}

function renderAnalyses(analyses) {
  analyses.forEach((analysis) => {
    const item = document.createElement("article");
    item.className = `step analysis importance-${analysis.importance}`;
    const heading = document.createElement("div");
    heading.className = "step-heading";
    const label = document.createElement("strong");
    label.textContent = analysis.message || t("analysisBasis");
    const detail = document.createElement("small");
    detail.textContent = analysis.rule;
    const math = document.createElement("div");
    math.className = "math";
    heading.append(label, detail);
    item.append(heading, math);
    stepsEl.appendChild(item);
    renderMath(analysis.tex, math);
  });
}

function renderPlot(data) {
  const points = data.kind === "numeric_ode"
    ? (data.sampled_data?.points || [])
        .map((point) => ({ x: point.independent, y: point.state[0] }))
    : (data.plot?.sampled?.points || []);
  plotEl.hidden = false;
  const ratio = window.devicePixelRatio || 1;
  const width = plotEl.clientWidth || 800;
  const height = Math.min(390, Math.max(280, width * 0.48));
  plotEl.width = width * ratio;
  plotEl.height = height * ratio;
  const ctx = plotEl.getContext("2d");
  ctx.scale(ratio, ratio);
  const finite = points.filter((point) => Number.isFinite(point.y));
  if (!finite.length) throw new Error(t("finitePlotError"));
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
    lastResult = result;
    $("#result-title").textContent = localizedResultLabel(result);
    $("#result-kind").textContent = localizedResultLabel(result);
    renderSemantic(result.semantic, result.outcome);
    if (result.kind === "plot") {
      renderSummary(result);
      renderPlot(result);
    } else if (result.kind === "numeric_ode") renderPlot(result);
    else renderSummary(result);
    renderAnalyses(result.analyses || []);
    renderSteps(result.steps || []);
    renderConclusions(result.conclusions || []);
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

async function runStdio() {
  const input = $("#stdio-input").value;
  const output = $("#stdio-output");
  const button = $("#stdio-run");
  button.disabled = true;
  output.textContent = "";
  const lines = input.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  for (const line of lines) {
    try {
      const parsed = /^\{\s*"/.test(line) ? JSON.parse(line) : { expression: line };
      const result = await invoke("process_expression", {
        request: {
          expression: parsed.expression,
          steps: parsed.steps ?? true,
          verbosity: parsed.verbosity ?? "standard",
        },
      });
      output.textContent += `${JSON.stringify(result)}\n`;
    } catch (error) {
      output.textContent += `${JSON.stringify({ error })}\n`;
    }
  }
  button.disabled = false;
}

function renderAssumptions() {
  const list = $("#assumption-list");
  list.innerHTML = "";
  if (!assumptions.size) list.innerHTML = `<span class="empty">${t("noneYet")}</span>`;
  assumptions.forEach(({ fact, symbol }) => {
    const chip = document.createElement("span");
    chip.className = "chip";
    chip.textContent = `${symbol}: ${t(`fact.${fact}`)}`;
    list.appendChild(chip);
  });
  $("#assumption-summary").textContent = assumptions.size ? t("assumptionCount", { count: assumptions.size }) : t("noAssumptions");
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
$("#stdio-run").addEventListener("click", runStdio);
$("#stdio-clear").addEventListener("click", () => { $("#stdio-output").textContent = ""; });
$("#locale").value = getLocale();
$("#locale").addEventListener("change", (event) => setLocale(event.target.value));
document.addEventListener("localechange", () => {
  renderTemplates();
  renderAssumptions();
  setBusy(calculating);
  if (lastResult) {
    $("#result-title").textContent = localizedResultLabel(lastResult);
    $("#result-kind").textContent = localizedResultLabel(lastResult);
    semanticEl.innerHTML = "";
    renderSemantic(lastResult.semantic, lastResult.outcome);
  }
});
exprEl.addEventListener("keydown", (event) => { if ((event.ctrlKey || event.metaKey) && event.key === "Enter") calculate(); });
exprEl.addEventListener("input", hideInputHelp);
applyTranslations();
renderTemplates();
refreshAssumptions().catch(showError);
