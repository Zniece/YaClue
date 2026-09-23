import "mathlive";
import "mathlive/fonts.css";
import { yaclueKeyboardLayouts } from "./keyboard.js";
import { hasTranslation, t } from "./i18n.js";
import { clearCalculationRows, renderAnswer, renderSteps } from "./math-rows.js";

const $ = (selector) => document.querySelector(selector);
const app = $("#app");
const calculatorView = $("#calculator-view");
const workspace = $(".workspace");
const field = $("#math-input");
const steps = $("#steps");
const answer = $("#answer");
const keyboardToggle = $("#keyboard-toggle");
let keyboardExpanded = true;
let keyboardRequest = 0;
let keyboardMenuScrollLeft = 0;
let lastSubmissionResult = null;
let calculationRequest = 0;
let lastCalculationResult = null;
let lastCalculationError = null;

const diagnosticMessages = {
  "empty-expression": "请输入需要计算的表达式",
  "mathlive-error": "输入中含有无法解析的数学结构",
  "missing-slot": "请补全尚未填写的输入框",
  "ambiguous-semantics": "这个符号存在多种含义，请使用键盘中的明确形式",
  "unsupported-atom": "当前尚不支持这种数学结构",
  "unsupported-command": "当前尚不支持这个数学命令",
  "invalid-argument": "函数或运算符的参数无效",
  "invalid-assumption": "分号后的假设条件无效",
  "lossy-conversion": "该输入无法无损转换为 YaClue 表达式",
};

const keyboardElement = () => Array.from(calculatorView.children).find((element) => element.classList.contains("ML__keyboard"));

function restoreKeyboardMenuScroll() {
  const scroller = keyboardElement()?.querySelector(".MLK__layer.is-visible .MLK__toolbar > .left");
  if (scroller) scroller.scrollLeft = keyboardMenuScrollLeft;
}

function syncKeyboardHeight() {
  if (!keyboardExpanded) return;
  const plate = keyboardElement()?.querySelector(".MLK__plate");
  const apiHeight = Math.ceil(window.mathVirtualKeyboard?.boundingRect?.height || 0);
  const elementHeight = Math.ceil(plate?.getBoundingClientRect().height || 0);
  const height = Math.max(apiHeight, elementHeight);
  if (height > 0) app.style.setProperty("--keyboard-height", `${height}px`);
}

function syncAnswerHeight() {
  const height = answer.classList.contains("visible") ? Math.ceil(answer.getBoundingClientRect().height) : 0;
  workspace.style.setProperty("--answer-height", `${height}px`);
}

function ensureKeyboard(request, attempt = 0) {
  if (request !== keyboardRequest || !keyboardExpanded || calculatorView.hidden) return;
  const keyboard = window.mathVirtualKeyboard;
  if (!keyboard) return;
  let element = keyboardElement();
  if (keyboard.visible && !element) keyboard.hide({ animate: false });
  keyboard.show({ animate: false });
  element = keyboardElement();
  const height = element?.querySelector(".MLK__plate")?.getBoundingClientRect().height || 0;
  if (element?.isConnected && height > 0) {
    element.classList.add("is-visible");
    restoreKeyboardMenuScroll();
    syncKeyboardHeight();
    return;
  }
  if (attempt >= 8) return;
  if (attempt > 1 && keyboard.visible) keyboard.hide({ animate: false });
  const delays = [0, 40, 100, 180, 300, 500, 800, 1200, 1800];
  setTimeout(() => ensureKeyboard(request, attempt + 1), delays[attempt + 1]);
}

function setKeyboardExpanded(expanded) {
  keyboardExpanded = expanded;
  const request = ++keyboardRequest;
  app.classList.toggle("keyboard-collapsed", !expanded);
  keyboardToggle.setAttribute("aria-expanded", String(expanded));
  keyboardToggle.setAttribute("aria-label", expanded ? "收起数学键盘" : "展开数学键盘");
  if (expanded && !calculatorView.hidden) {
    field.focus({ preventScroll: true });
    requestAnimationFrame(() => ensureKeyboard(request));
  } else window.mathVirtualKeyboard?.hide({ animate: false });
}

function setView(name) {
  for (const view of document.querySelectorAll(".view")) view.hidden = view.id !== `${name}-view`;
  const calculating = name === "calculator";
  $("#calculate-tab").classList.toggle("active", calculating);
  $("#menu-tab").classList.toggle("active", !calculating);
  $("#calculate-tab").toggleAttribute("aria-current", calculating);
  if (calculating) setKeyboardExpanded(keyboardExpanded);
  else window.mathVirtualKeyboard?.hide({ animate: false });
}

function localizedMessage(reference, fallback) {
  if (!reference?.key) return fallback || "";
  if (hasTranslation(reference.key)) return t(reference.key, reference.args);
  return reference.fallback || fallback || reference.key;
}

function renderCalculationError(error) {
  lastCalculationResult = null;
  lastCalculationError = error;
  const explanation = localizedMessage(error?.message_ref, error?.message || "计算失败");
  renderSteps(steps, [{ explanation, latex: field.value || "\\placeholder{}" }]);
  renderAnswer(answer, "\\mathrm{Calculation\\ failed}");
  requestAnimationFrame(syncAnswerHeight);
}

function renderCalculation(result) {
  lastCalculationResult = result;
  lastCalculationError = null;
  const analyses = (result.analyses || []).map((analysis) => ({
    explanation: localizedMessage(analysis.message_ref, analysis.rule),
    latex: analysis.tex,
  }));
  const calculationSteps = (result.steps || []).map((step) => ({
    explanation: localizedMessage(step.message_ref, step.rule),
    latex: step.tex,
  }));
  const conclusions = (result.conclusions || []).map((conclusion) => ({
    explanation: localizedMessage(conclusion.message_ref, conclusion.kind),
    latex: conclusion.tex,
  }));
  renderSteps(steps, [...analyses, ...calculationSteps, ...conclusions]);
  renderAnswer(answer, result.tex || result.expression);
  requestAnimationFrame(syncAnswerHeight);
}

async function showSubmissionResult() {
  const request = ++calculationRequest;
  lastCalculationResult = null;
  lastCalculationError = null;
  clearCalculationRows(steps, answer);

  if (typeof field.getYaClueSubmissionResult !== "function") {
    lastSubmissionResult = {
      ok: false,
      diagnostics: [{ severity: "error", code: "adapter-unavailable", message: "YaClue input adapter is unavailable" }],
    };
  } else {
    lastSubmissionResult = field.getYaClueSubmissionResult();
  }
  document.dispatchEvent(new CustomEvent("yaclue-submission", { detail: lastSubmissionResult }));

  if (!lastSubmissionResult.ok) {
    const diagnostic = lastSubmissionResult.diagnostics[0];
    const explanation = diagnosticMessages[diagnostic?.code] || diagnostic?.message || "输入无法转换";
    steps.dataset.diagnosticCode = diagnostic?.code || "unknown";
    renderSteps(steps, [{ explanation, latex: field.value || "\\placeholder{}" }]);
    renderAnswer(answer, "\\mathrm{Invalid\\ input}");
    requestAnimationFrame(syncAnswerHeight);
    return;
  }

  delete steps.dataset.diagnosticCode;
  const invoke = window.__TAURI__?.core?.invoke;
  if (typeof invoke !== "function") {
    renderCalculationError({ message: "当前环境无法连接计算引擎" });
    return;
  }

  renderAnswer(answer, "\\mathrm{Calculating}\\ldots");
  requestAnimationFrame(syncAnswerHeight);
  try {
    const result = await invoke("process_expression", {
      request: {
        expression: lastSubmissionResult.expression,
        steps: true,
        verbosity: "standard",
        assumptions: lastSubmissionResult.assumptions || [],
      },
    });
    if (request === calculationRequest) renderCalculation(result);
  } catch (error) {
    if (request === calculationRequest) renderCalculationError(error);
  }
}

customElements.whenDefined("math-field").then(() => {
  const keyboard = window.mathVirtualKeyboard;
  keyboard.container = calculatorView;
  keyboard.layouts = yaclueKeyboardLayouts;
  keyboard.editToolbar = "none";
  field.mathVirtualKeyboardPolicy = "manual";
  field.value = "0";
  field.focus({ preventScroll: true });
  setKeyboardExpanded(true);
  keyboard.addEventListener("geometrychange", syncKeyboardHeight);
  new ResizeObserver(syncAnswerHeight).observe(answer);
});

keyboardToggle.addEventListener("pointerdown", (event) => event.preventDefault());
keyboardToggle.addEventListener("click", () => setKeyboardExpanded(!keyboardExpanded));
calculatorView.addEventListener("scroll", (event) => {
  const scroller = event.target;
  if (scroller instanceof Element && scroller.matches(".MLK__toolbar > .left")) {
    keyboardMenuScrollLeft = scroller.scrollLeft;
  }
}, { capture: true, passive: true });
calculatorView.addEventListener("pointerdown", (event) => {
  const switcher = event.target instanceof Element ? event.target.closest("[data-layer]") : null;
  if (!switcher) return;
  const scroller = switcher.closest(".MLK__toolbar > .left");
  if (scroller) keyboardMenuScrollLeft = scroller.scrollLeft;
  requestAnimationFrame(restoreKeyboardMenuScroll);
}, { capture: true });
field.addEventListener("focus", () => keyboardExpanded && !calculatorView.hidden && ensureKeyboard(++keyboardRequest));
field.addEventListener("input", () => { calculationRequest += 1; });
field.addEventListener("beforeinput", (event) => { if (event.inputType === "insertLineBreak") { event.preventDefault(); showSubmissionResult(); } });
$("#calculate-tab").addEventListener("click", () => setView("calculator"));
$("#menu-tab").addEventListener("click", () => setView("menu"));
document.querySelectorAll("[data-view]").forEach((button) => button.addEventListener("click", () => setView(button.dataset.view)));
window.addEventListener("pageshow", () => keyboardExpanded && !calculatorView.hidden && ensureKeyboard(++keyboardRequest));
document.addEventListener("visibilitychange", () => { if (!document.hidden && keyboardExpanded && !calculatorView.hidden) ensureKeyboard(++keyboardRequest); });
document.addEventListener("localechange", () => {
  if (lastCalculationResult) renderCalculation(lastCalculationResult);
  else if (lastCalculationError) renderCalculationError(lastCalculationError);
});
