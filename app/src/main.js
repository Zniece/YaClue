import "mathlive";
import "mathlive/fonts.css";
import { localizeKeyboardLayouts } from "./keyboard.js";
import { renderHelp } from "./help.js";
import { parseYaClueSource } from "./source-input.js";
import { applyTranslations, getLocale, hasTranslation, setLocale, t } from "./i18n.js";
import { clearCalculationRows, renderAnswer, renderSteps } from "./math-rows.js";
import { materializeDisplayedZero } from "./calculator-input.js";

const $ = (selector) => document.querySelector(selector);
const app = $("#app");
const calculatorView = $("#calculator-view");
const workspace = $(".workspace");
const field = $("#math-input");
const firstCalculationHint = $("#first-calculation-hint");
const calculationError = $("#calculation-error");
const steps = $("#steps");
const answer = $("#answer");
const calculationStatus = $("#calculation-status");
const keyboardToggle = $("#keyboard-toggle");
const sourceInput = $("#source-input");
const sourceSteps = $("#source-steps");
const sourceAnswer = $("#source-answer");
const sourceStatus = $("#source-status");
let sourceRequest = 0;
let sourceResult = null;
let sourceError = null;
let sourceDiagnostic = null;
let sourcePending = false;
let keyboardExpanded = true;
let keyboardRequest = 0;
let keyboardMenuScrollLeft = 0;
let touchedKeyboardTab = null;
let lastSubmissionResult = null;
let calculationRequest = 0;
let calculationPending = false;
let lastCalculationResult = null;
let lastCalculationError = null;
const firstCalculationStorageKey = "yaclue.hasSubmittedCalculation";

firstCalculationHint.hidden = localStorage.getItem(firstCalculationStorageKey) === "1";
const completeFirstCalculationGuide = () => {
  if (firstCalculationHint.hidden) return;
  firstCalculationHint.hidden = true;
  localStorage.setItem(firstCalculationStorageKey, "1");
};
const showAnswer = (latex, state = "result") => {
  answer.dataset.state = state;
  renderAnswer(answer, latex);
};

const statusLatex = (key) => `\\text{${t(key)}}`;
const announce = (message) => { calculationStatus.textContent = message; };
const showCalculationError = (message) => {
  calculationError.textContent = message;
  calculationError.hidden = false;
  calculationError.scrollTop = 0;
};
const clearCalculationError = () => {
  calculationError.hidden = true;
  calculationError.textContent = "";
};
const localizedKeyboardLayouts = () => localizeKeyboardLayouts(t);

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
  renderSteps(steps, []);
  showCalculationError(explanation);
  showAnswer(statusLatex("ui.invalidInput"), "error");
  announce(explanation);
  requestAnimationFrame(syncAnswerHeight);
}

const keyboardElement = () => Array.from(calculatorView.children).find((element) => element.classList.contains("ML__keyboard"));

function syncKeyboardMenuOverflow(scroller) {
  if (!scroller) return;
  scroller.classList.toggle("can-scroll-x", scroller.scrollWidth > scroller.clientWidth + 1);
  scroller.classList.toggle("at-scroll-start", scroller.scrollLeft <= 1);
  scroller.classList.toggle("at-scroll-end", scroller.scrollLeft + scroller.clientWidth >= scroller.scrollWidth - 1);
}

function restoreKeyboardMenuScroll() {
  if (!keyboardExpanded || calculatorView.hidden) return;
  const scroller = keyboardElement()?.querySelector(".MLK__layer.is-visible .MLK__toolbar > .left");
  if (!scroller) return;
  scroller.scrollLeft = keyboardMenuScrollLeft;
  const selected = scroller.querySelector(".selected");
  if (selected) {
    const tab = selected.getBoundingClientRect();
    const viewport = scroller.getBoundingClientRect();
    if (tab.left < viewport.left + 12 || tab.right > viewport.right - 12)
      selected.scrollIntoView({ block: "nearest", inline: "center" });
  }
  keyboardMenuScrollLeft = scroller.scrollLeft;
  syncKeyboardMenuOverflow(scroller);
}

function setKeyboardUiState(state) {
  app.dataset.keyboardState = state;
  const expanded = state === "opening" || state === "open";
  app.classList.toggle("keyboard-collapsed", !expanded);
  keyboardToggle.setAttribute("aria-expanded", String(expanded));
  const labels = { opening: "ui.keyboardOpening", open: "ui.keyboardOpen", closed: "ui.keyboardClosed", failed: "ui.keyboardFailed" };
  keyboardToggle.setAttribute("aria-label", t(labels[state]));
  if (state === "failed") announce(t("ui.keyboardFailed"));
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
  for (const [tab, current] of [["#calculate-tab", calculating], ["#menu-tab", !calculating]]) {
    if (current) $(tab).setAttribute("aria-current", "page");
    else $(tab).removeAttribute("aria-current");
  }
  if (calculating) {
    setKeyboardExpanded(keyboardExpanded);
    if (!keyboardExpanded) field.focus({ preventScroll: true });
  } else {
    touchedKeyboardTab = null;
    window.mathVirtualKeyboard?.hide({ animate: false });
    document.querySelector(`#${name}-view button`)?.focus({ preventScroll: true });
  }
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
  renderSteps(steps, []);
  showCalculationError(explanation);
  showAnswer(statusLatex("ui.calculationFailed"), "error");
  announce(explanation);
  requestAnimationFrame(syncAnswerHeight);
}

function resultRows(result) {
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
  return [...analyses, ...calculationSteps, ...conclusions];
}

function renderCalculation(result) {
  calculationPending = false;
  lastCalculationResult = result;
  lastCalculationError = null;
  clearCalculationError();
  renderSteps(steps, resultRows(result));
  showAnswer(result.tex || result.expression);
  announce(t("ui.calculationComplete"));
  requestAnimationFrame(syncAnswerHeight);
}

function renderSourceState() {
  clearCalculationRows(sourceSteps, sourceAnswer);
  if (sourceDiagnostic) {
    sourceStatus.textContent = t(`input.${sourceDiagnostic}`);
  } else if (sourceError) {
    sourceStatus.textContent = localizedMessage(sourceError.message_ref, sourceError.message || t("ui.calculationFailed"));
  } else if (sourcePending) {
    sourceStatus.textContent = t("ui.calculating");
  } else if (sourceResult) {
    sourceStatus.textContent = t("ui.calculationComplete");
    renderSteps(sourceSteps, resultRows(sourceResult));
    renderAnswer(sourceAnswer, sourceResult.tex || sourceResult.expression);
  } else {
    sourceStatus.textContent = "";
  }
}

async function submitSource() {
  const request = ++sourceRequest;
  sourceResult = null;
  sourceError = null;
  sourcePending = false;
  const submission = parseYaClueSource(sourceInput.value);
  sourceDiagnostic = submission.ok ? null : submission.diagnostics[0].code;
  renderSourceState();
  if (!submission.ok) return;
  const invoke = window.__TAURI__?.core?.invoke;
  if (typeof invoke !== "function") {
    sourceError = { message_ref: { key: "ui.engineUnavailable" } };
    renderSourceState();
    return;
  }
  sourcePending = true;
  renderSourceState();
  try {
    const result = await invoke("process_expression", {
      request: { expression: submission.expression, steps: true, verbosity: "standard", assumptions: submission.assumptions },
    });
    if (request !== sourceRequest) return;
    sourcePending = false;
    sourceResult = result;
    renderSourceState();
  } catch (error) {
    if (request !== sourceRequest) return;
    sourcePending = false;
    sourceError = error;
    renderSourceState();
  }
}

async function showSubmissionResult() {
  completeFirstCalculationGuide();
  materializeDisplayedZero(field);
  const request = ++calculationRequest;
  calculationPending = false;
  lastCalculationResult = null;
  lastCalculationError = null;
  clearCalculationRows(steps, answer);
  clearCalculationError();
  announce("");

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

  const invoke = window.__TAURI__?.core?.invoke;
  if (typeof invoke !== "function") {
    renderCalculationError({ message_ref: { key: "ui.engineUnavailable" } });
    return;
  }

  showAnswer(statusLatex("ui.calculating"), "pending");
  announce(t("ui.calculating"));
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
renderHelp($("#help-content"), getLocale());

customElements.whenDefined("math-field").then(() => {
  const keyboard = window.mathVirtualKeyboard;
  keyboard.container = calculatorView;
  keyboard.layouts = localizedKeyboardLayouts();
  keyboard.editToolbar = "none";
  field.mathVirtualKeyboardPolicy = "manual";
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
  if (scroller instanceof Element && scroller.matches(".MLK__layer.is-visible .MLK__toolbar > .left")) {
    keyboardMenuScrollLeft = scroller.scrollLeft;
    syncKeyboardMenuOverflow(scroller);
  }
}, { capture: true, passive: true });
calculatorView.addEventListener("pointerdown", (event) => {
  const switcher = event.target instanceof Element ? event.target.closest("[data-layer]") : null;
  if (!switcher) return;
  const scroller = switcher.closest(".MLK__toolbar > .left");
  if (scroller) keyboardMenuScrollLeft = scroller.scrollLeft;
  if (event.pointerType === "touch") {
    touchedKeyboardTab = {
      pointerId: event.pointerId,
      x: event.clientX,
      y: event.clientY,
      layer: switcher.dataset.layer,
      moved: false,
    };
    event.stopPropagation();
    return;
  }
  requestAnimationFrame(() => requestAnimationFrame(restoreKeyboardMenuScroll));
}, { capture: true });
calculatorView.addEventListener("pointermove", (event) => {
  const tab = touchedKeyboardTab;
  if (tab?.pointerId !== event.pointerId) return;
  if (Math.hypot(event.clientX - tab.x, event.clientY - tab.y) > 10) tab.moved = true;
}, { capture: true, passive: true });
calculatorView.addEventListener("pointerup", (event) => {
  const tab = touchedKeyboardTab;
  if (tab?.pointerId !== event.pointerId) return;
  touchedKeyboardTab = null;
  event.stopPropagation();
  if (tab.moved) return;
  const target = document.elementFromPoint(event.clientX, event.clientY)?.closest("[data-layer]");
  if (target?.dataset.layer !== tab.layer) return;
  if (window.mathVirtualKeyboard) window.mathVirtualKeyboard.currentLayer = tab.layer;
  requestAnimationFrame(() => requestAnimationFrame(restoreKeyboardMenuScroll));
}, { capture: true });
calculatorView.addEventListener("pointercancel", (event) => {
  if (touchedKeyboardTab?.pointerId === event.pointerId) touchedKeyboardTab = null;
}, { capture: true });
field.addEventListener("focus", () => keyboardExpanded && !calculatorView.hidden && ensureKeyboard(++keyboardRequest));
field.addEventListener("input", () => {
  calculationRequest += 1;
  lastSubmissionResult = null;
  if (!calculationError.hidden || calculationPending || lastCalculationError) {
    lastCalculationError = null;
    calculationPending = false;
    clearCalculationError();
    clearCalculationRows(steps, answer);
    syncAnswerHeight();
    announce("");
  }
});
field.addEventListener("beforeinput", (event) => { if (event.inputType === "insertLineBreak") { event.preventDefault(); showSubmissionResult(); } });
$("#calculate-tab").addEventListener("click", () => setView("calculator"));
$("#menu-tab").addEventListener("click", () => setView("menu"));
document.querySelectorAll("[data-view]").forEach((button) => button.addEventListener("click", () => setView(button.dataset.view)));
$("#help-content").addEventListener("click", (event) => {
  const example = event.target instanceof Element ? event.target.closest(".help-example") : null;
  if (!example) return;
  sourceInput.value = example.dataset.example;
  ++sourceRequest;
  sourcePending = false;
  sourceResult = sourceError = sourceDiagnostic = null;
  renderSourceState();
  setView("source");
  sourceInput.focus({ preventScroll: true });
});
$("#source-submit").addEventListener("click", submitSource);
sourceInput.addEventListener("input", () => {
  ++sourceRequest;
  sourcePending = false;
  sourceResult = sourceError = sourceDiagnostic = null;
  renderSourceState();
});
sourceInput.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && !event.isComposing) {
    event.preventDefault();
    submitSource();
  }
});
document.querySelectorAll("[data-locale]").forEach((button) => button.addEventListener("click", () => setLocale(button.dataset.locale)));
window.addEventListener("pageshow", () => keyboardExpanded && !calculatorView.hidden && ensureKeyboard(++keyboardRequest));
window.addEventListener("resize", restoreKeyboardMenuScroll);
document.addEventListener("visibilitychange", () => { if (!document.hidden && keyboardExpanded && !calculatorView.hidden) ensureKeyboard(++keyboardRequest); });
document.addEventListener("localechange", () => {
  updateLocaleControls();
  renderHelp($("#help-content"), getLocale());
  if (sourceResult || sourceError || sourceDiagnostic || sourcePending) renderSourceState();
  setKeyboardUiState(app.dataset.keyboardState || "closed");
  if (window.mathVirtualKeyboard) {
    window.mathVirtualKeyboard.layouts = localizedKeyboardLayouts();
    if (keyboardExpanded && !calculatorView.hidden) ensureKeyboard(++keyboardRequest);
  }
  if (lastCalculationResult) renderCalculation(lastCalculationResult);
  else if (lastCalculationError) renderCalculationError(lastCalculationError);
  else if (lastSubmissionResult && !lastSubmissionResult.ok) renderInputDiagnostic();
  else if (calculationPending) {
    showAnswer(statusLatex("ui.calculating"), "pending");
    announce(t("ui.calculating"));
  }
});
