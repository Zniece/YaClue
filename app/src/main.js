import "mathlive";
import "mathlive/fonts.css";
import { yaclueKeyboardLayouts } from "./keyboard.js";
import { applyTranslations, getLocale, hasTranslation, setLocale, t } from "./i18n.js";
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
let calculationPending = false;
let lastCalculationResult = null;
let lastCalculationError = null;

const statusLatex = (key) => `\\text{${t(key)}}`;
const localizedKeyboardLayouts = () => yaclueKeyboardLayouts.map((layout) => ({
  ...layout,
  tooltip: t(`keyboard.${layout.id.slice("yaclue-".length)}`),
}));

function updateLocaleControls() {
  for (const button of document.querySelectorAll("[data-locale]"))
    button.setAttribute("aria-pressed", String(button.dataset.locale === getLocale()));
}

function renderInputDiagnostic() {
  const diagnostic = lastSubmissionResult?.diagnostics?.[0];
  const key = `input.${diagnostic?.code}`;
  const explanation = hasTranslation(key)
    ? t(key)
    : diagnostic?.message || t("ui.inputConversionFailed");
  steps.dataset.diagnosticCode = diagnostic?.code || "unknown";
  renderSteps(steps, [{ explanation, latex: field.value || "\\placeholder{}" }]);
  renderAnswer(answer, statusLatex("ui.invalidInput"));
  requestAnimationFrame(syncAnswerHeight);
}

const keyboardElement = () => Array.from(calculatorView.children).find((element) => element.classList.contains("ML__keyboard"));

function restoreKeyboardMenuScroll() {
  const scroller = keyboardElement()?.querySelector(".MLK__layer.is-visible .MLK__toolbar > .left");
  if (scroller) scroller.scrollLeft = keyboardMenuScrollLeft;
}

function setKeyboardUiState(state) {
  app.dataset.keyboardState = state;
  const expanded = state === "opening" || state === "open";
  app.classList.toggle("keyboard-collapsed", !expanded);
  keyboardToggle.setAttribute("aria-expanded", String(expanded));
  const labels = { opening: "ui.keyboardOpening", open: "ui.keyboardOpen", closed: "ui.keyboardClosed", failed: "ui.keyboardFailed" };
  keyboardToggle.setAttribute("aria-label", t(labels[state]));
}

function failKeyboardRequest(request) {
  if (request !== keyboardRequest) return;
  keyboardExpanded = false;
  window.mathVirtualKeyboard?.hide({ animate: false });
  setKeyboardUiState("failed");
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
  if (!keyboard) {
    if (attempt >= 8) failKeyboardRequest(request);
    else scheduleKeyboardRetry(request, attempt);
    return;
  }
  let element = keyboardElement();
  if (keyboard.visible && !element) keyboard.hide({ animate: false });
  keyboard.show({ animate: false });
  element = keyboardElement();
  const height = element?.querySelector(".MLK__plate")?.getBoundingClientRect().height || 0;
  if (element?.isConnected && height > 0) {
    element.classList.add("is-visible");
    setKeyboardUiState("open");
    restoreKeyboardMenuScroll();
    syncKeyboardHeight();
    return;
  }
  if (attempt >= 8) {
    failKeyboardRequest(request);
    return;
  }
  if (attempt > 1 && keyboard.visible) keyboard.hide({ animate: false });
  scheduleKeyboardRetry(request, attempt);
}

function scheduleKeyboardRetry(request, attempt) {
  const delays = [0, 40, 100, 180, 300, 500, 800, 1200, 1800];
  setTimeout(() => ensureKeyboard(request, attempt + 1), delays[attempt + 1]);
}

function setKeyboardExpanded(expanded) {
  keyboardExpanded = expanded;
  const request = ++keyboardRequest;
  if (expanded && !calculatorView.hidden) {
    setKeyboardUiState("opening");
    field.focus({ preventScroll: true });
    requestAnimationFrame(() => ensureKeyboard(request));
  } else {
    setKeyboardUiState("closed");
    window.mathVirtualKeyboard?.hide({ animate: false });
  }
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
  calculationPending = false;
  lastCalculationResult = null;
  lastCalculationError = error;
  const explanation = localizedMessage(error?.message_ref, error?.message || t("ui.calculationFailed"));
  renderSteps(steps, [{ explanation, latex: field.value || "\\placeholder{}" }]);
  renderAnswer(answer, statusLatex("ui.calculationFailed"));
  requestAnimationFrame(syncAnswerHeight);
}

function renderCalculation(result) {
  calculationPending = false;
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
  calculationPending = false;
  lastCalculationResult = null;
  lastCalculationError = null;
  clearCalculationRows(steps, answer);

  if (typeof field.getYaClueSubmissionResult !== "function") {
    lastSubmissionResult = {
      ok: false,
      diagnostics: [{ severity: "error", code: "adapter-unavailable", message: "YaClue input adapter is unavailable" }],
    };
  } else {
    try {
      lastSubmissionResult = field.getYaClueSubmissionResult();
    } catch (_error) {
      lastSubmissionResult = {
        ok: false,
        diagnostics: [{
          severity: "error",
          code: "adapter-failure",
          message: "The mathematical input adapter failed",
        }],
      };
    }
  }
  document.dispatchEvent(new CustomEvent("yaclue-submission", { detail: lastSubmissionResult }));

  if (!lastSubmissionResult.ok) {
    renderInputDiagnostic();
    return;
  }

  delete steps.dataset.diagnosticCode;
  const invoke = window.__TAURI__?.core?.invoke;
  if (typeof invoke !== "function") {
    renderCalculationError({ message_ref: { key: "ui.engineUnavailable" } });
    return;
  }

  renderAnswer(answer, statusLatex("ui.calculating"));
  calculationPending = true;
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

applyTranslations();
updateLocaleControls();

customElements.whenDefined("math-field").then(() => {
  const keyboard = window.mathVirtualKeyboard;
  keyboard.container = calculatorView;
  keyboard.layouts = localizedKeyboardLayouts();
  keyboard.editToolbar = "none";
  field.mathVirtualKeyboardPolicy = "manual";
  field.value = "0";
  field.focus({ preventScroll: true });
  setKeyboardExpanded(true);
  keyboard.addEventListener("geometrychange", () => {
    const plate = keyboardElement()?.querySelector(".MLK__plate");
    if (keyboardExpanded && plate?.getBoundingClientRect().height > 0)
      setKeyboardUiState("open");
    syncKeyboardHeight();
  });
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
field.addEventListener("input", () => {
  calculationRequest += 1;
  lastSubmissionResult = null;
  if (steps.dataset.diagnosticCode) {
    delete steps.dataset.diagnosticCode;
    clearCalculationRows(steps, answer);
    syncAnswerHeight();
  }
  if (!calculationPending) return;
  calculationPending = false;
  clearCalculationRows(steps, answer);
  syncAnswerHeight();
});
field.addEventListener("beforeinput", (event) => { if (event.inputType === "insertLineBreak") { event.preventDefault(); showSubmissionResult(); } });
$("#calculate-tab").addEventListener("click", () => setView("calculator"));
$("#menu-tab").addEventListener("click", () => setView("menu"));
document.querySelectorAll("[data-view]").forEach((button) => button.addEventListener("click", () => setView(button.dataset.view)));
document.querySelectorAll("[data-locale]").forEach((button) => button.addEventListener("click", () => setLocale(button.dataset.locale)));
window.addEventListener("pageshow", () => keyboardExpanded && !calculatorView.hidden && ensureKeyboard(++keyboardRequest));
document.addEventListener("visibilitychange", () => { if (!document.hidden && keyboardExpanded && !calculatorView.hidden) ensureKeyboard(++keyboardRequest); });
document.addEventListener("localechange", () => {
  updateLocaleControls();
  setKeyboardUiState(app.dataset.keyboardState || "closed");
  if (window.mathVirtualKeyboard) {
    window.mathVirtualKeyboard.layouts = localizedKeyboardLayouts();
    if (keyboardExpanded && !calculatorView.hidden) ensureKeyboard(++keyboardRequest);
  }
  if (lastCalculationResult) renderCalculation(lastCalculationResult);
  else if (lastCalculationError) renderCalculationError(lastCalculationError);
  else if (lastSubmissionResult && !lastSubmissionResult.ok) renderInputDiagnostic();
  else if (calculationPending) renderAnswer(answer, statusLatex("ui.calculating"));
});
