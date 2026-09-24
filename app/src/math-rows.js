const makeMathField = (latex) => {
  const field = document.createElement("math-field");
  field.readOnly = true;
  field.tabIndex = -1;
  field.value = latex || "";
  return field;
};

const rowCleanup = new WeakMap();
const answerFrames = new WeakMap();
const stepRenderState = new WeakMap();
const STEP_BATCH_SIZE = 24;
let nextStepDetailId = 0;

const syncHorizontalOverflow = (element, focusWhenFits = false, allowFocus = true) => {
  const scrollable = element.scrollWidth > element.clientWidth + 1;
  element.tabIndex = allowFocus && (scrollable || focusWhenFits) ? 0 : -1;
  element.classList.toggle("can-scroll-x", scrollable);
  element.classList.toggle("at-scroll-start", element.scrollLeft <= 1);
  element.classList.toggle("at-scroll-end", element.scrollLeft + element.clientWidth >= element.scrollWidth - 1);
};

const observeHorizontalOverflow = (element, allowFocus = true) => {
  const sync = () => syncHorizontalOverflow(element, false, allowFocus);
  const observer = new ResizeObserver(sync);
  element.addEventListener("scroll", sync, { passive: true });
  observer.observe(element);
  const frame = requestAnimationFrame(sync);
  return () => {
    observer.disconnect();
    element.removeEventListener("scroll", sync);
    cancelAnimationFrame(frame);
  };
};

export function createStepRow(step, index) {
  const row = document.createElement("article");
  row.className = "step-row";

  const meta = document.createElement("button");
  meta.className = "step-meta";
  meta.type = "button";
  const number = document.createElement("b");
  number.className = "step-number";
  number.textContent = String(index + 1).padStart(2, "0");
  const explanation = document.createElement("span");
  explanation.className = "step-explanation";
  const explanationText = step.explanation || "";
  explanation.textContent = explanationText;
  meta.append(number, explanation);

  const formula = document.createElement("div");
  formula.className = "math-row step-formula";
  formula.appendChild(makeMathField(step.latex));
  row.append(meta, formula);

  const detail = document.createElement("div");
  detail.className = "step-detail";
  detail.id = `step-detail-${++nextStepDetailId}`;
  detail.textContent = explanationText;
  detail.hidden = true;
  meta.setAttribute("aria-expanded", "false");
  meta.setAttribute("aria-controls", detail.id);
  row.append(detail);

  let pointerStart = null;
  let pointerMoved = false;
  const onPointerDown = (event) => {
    pointerStart = { x: event.clientX, y: event.clientY };
    pointerMoved = false;
  };
  const onPointerMove = (event) => {
    if (pointerStart && Math.hypot(event.clientX - pointerStart.x, event.clientY - pointerStart.y) > 8)
      pointerMoved = true;
  };
  const onPointerCancel = () => { pointerMoved = true; pointerStart = null; };
  const onClick = (event) => {
    pointerStart = null;
    const wasDrag = pointerMoved;
    pointerMoved = false;
    if (wasDrag && event.detail !== 0) {
      event.preventDefault();
      return;
    }
    detail.hidden = !detail.hidden;
    meta.setAttribute("aria-expanded", String(!detail.hidden));
  };
  const onKeyDown = (event) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    if (explanation.scrollWidth <= explanation.clientWidth + 1) return;
    event.preventDefault();
    explanation.scrollLeft += event.key === "ArrowRight" ? 40 : -40;
  };
  meta.addEventListener("pointerdown", onPointerDown);
  meta.addEventListener("pointermove", onPointerMove);
  meta.addEventListener("pointercancel", onPointerCancel);
  meta.addEventListener("click", onClick);
  meta.addEventListener("keydown", onKeyDown);

  const stopExplanation = observeHorizontalOverflow(explanation, false);
  const stopFormula = observeHorizontalOverflow(formula);
  rowCleanup.set(row, () => {
    stopExplanation();
    stopFormula();
    meta.removeEventListener("pointerdown", onPointerDown);
    meta.removeEventListener("pointermove", onPointerMove);
    meta.removeEventListener("pointercancel", onPointerCancel);
    meta.removeEventListener("click", onClick);
    meta.removeEventListener("keydown", onKeyDown);
  });
  return row;
}

export function renderSteps(container, items) {
  const previous = stepRenderState.get(container);
  if (previous) {
    container.removeEventListener("scroll", previous.onScroll);
    previous.observer.disconnect();
    if (previous.frame !== undefined) cancelAnimationFrame(previous.frame);
    stepRenderState.delete(container);
  }
  for (const row of container.children) {
    rowCleanup.get(row)?.();
    rowCleanup.delete(row);
  }
  container.replaceChildren();
  container.scrollTop = 0;
  container.classList.toggle("visible", items.length > 0);
  container.tabIndex = items.length > 0 ? 0 : -1;
  if (items.length === 0) return;

  const state = { items, next: 0, frame: undefined, onScroll: undefined, observer: undefined };
  const appendBatch = () => {
    const end = Math.min(state.next + STEP_BATCH_SIZE, state.items.length);
    const rows = [];
    for (let index = state.next; index < end; index++)
      rows.push(createStepRow(state.items[index], index));
    container.append(...rows);
    state.next = end;
  };
  const loadIfNearEnd = () => {
    state.frame = undefined;
    if (stepRenderState.get(container) !== state || state.next >= state.items.length) return;
    if (container.clientHeight <= 0) return;
    if (container.scrollHeight - container.scrollTop - container.clientHeight > Math.max(container.clientHeight, 200)) return;
    appendBatch();
    if (state.next < state.items.length) scheduleCheck();
  };
  const scheduleCheck = () => {
    if (state.frame === undefined) state.frame = requestAnimationFrame(loadIfNearEnd);
  };
  state.onScroll = scheduleCheck;
  state.observer = new ResizeObserver(scheduleCheck);
  stepRenderState.set(container, state);
  container.addEventListener("scroll", state.onScroll, { passive: true });
  state.observer.observe(container);
  appendBatch();
  scheduleCheck();
}

export function renderAnswer(container, latex) {
  const previousFrame = answerFrames.get(container);
  if (previousFrame !== undefined) cancelAnimationFrame(previousFrame);
  container.replaceChildren(makeMathField(latex));
  container.classList.toggle("visible", Boolean(latex));
  container.tabIndex = latex ? 0 : -1;
  answerFrames.set(container, requestAnimationFrame(() => {
    answerFrames.delete(container);
    syncHorizontalOverflow(container, Boolean(latex));
  }));
}

export function clearCalculationRows(steps, answer) {
  renderSteps(steps, []);
  const previousFrame = answerFrames.get(answer);
  if (previousFrame !== undefined) cancelAnimationFrame(previousFrame);
  answerFrames.delete(answer);
  answer.replaceChildren();
  answer.classList.remove("visible");
  answer.tabIndex = -1;
}
