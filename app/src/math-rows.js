const makeMathField = (latex) => {
  const field = document.createElement("math-field");
  field.readOnly = true;
  field.value = latex || "";
  return field;
};

const rowCleanup = new WeakMap();
const answerFrames = new WeakMap();

const syncHorizontalOverflow = (element) => {
  const scrollable = element.scrollWidth > element.clientWidth + 1;
  element.classList.toggle("can-scroll-x", scrollable);
  element.classList.toggle("at-scroll-start", element.scrollLeft <= 1);
  element.classList.toggle("at-scroll-end", element.scrollLeft + element.clientWidth >= element.scrollWidth - 1);
};

const observeHorizontalOverflow = (element) => {
  const sync = () => syncHorizontalOverflow(element);
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

  const meta = document.createElement("div");
  meta.className = "step-meta";
  const number = document.createElement("b");
  number.className = "step-number";
  number.textContent = String(index + 1).padStart(2, "0");
  const explanation = document.createElement("span");
  explanation.className = "step-explanation";
  explanation.textContent = step.explanation || "";
  meta.append(number, explanation);

  const formula = document.createElement("div");
  formula.className = "math-row step-formula";
  formula.appendChild(makeMathField(step.latex));
  row.append(meta, formula);

  const stopExplanation = observeHorizontalOverflow(explanation);
  const stopFormula = observeHorizontalOverflow(formula);
  rowCleanup.set(row, () => {
    stopExplanation();
    stopFormula();
  });
  return row;
}

export function renderSteps(container, items) {
  for (const row of container.children) {
    rowCleanup.get(row)?.();
    rowCleanup.delete(row);
  }
  container.replaceChildren(...items.map(createStepRow));
  container.classList.toggle("visible", items.length > 0);
}

export function renderAnswer(container, latex) {
  const previousFrame = answerFrames.get(container);
  if (previousFrame !== undefined) cancelAnimationFrame(previousFrame);
  container.replaceChildren(makeMathField(latex));
  container.classList.toggle("visible", Boolean(latex));
  answerFrames.set(container, requestAnimationFrame(() => {
    answerFrames.delete(container);
    syncHorizontalOverflow(container);
  }));
}

export function clearCalculationRows(steps, answer) {
  renderSteps(steps, []);
  const previousFrame = answerFrames.get(answer);
  if (previousFrame !== undefined) cancelAnimationFrame(previousFrame);
  answerFrames.delete(answer);
  answer.replaceChildren();
  answer.classList.remove("visible");
}
