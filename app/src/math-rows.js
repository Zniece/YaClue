const makeMathField = (latex) => {
  const field = document.createElement("math-field");
  field.readOnly = true;
  field.value = latex || "";
  return field;
};

const syncHorizontalOverflow = (element) => {
  const scrollable = element.scrollWidth > element.clientWidth + 1;
  element.classList.toggle("can-scroll-x", scrollable);
  element.classList.toggle("at-scroll-start", element.scrollLeft <= 1);
  element.classList.toggle("at-scroll-end", element.scrollLeft + element.clientWidth >= element.scrollWidth - 1);
};

const observeHorizontalOverflow = (element) => {
  const sync = () => syncHorizontalOverflow(element);
  element.addEventListener("scroll", sync, { passive: true });
  new ResizeObserver(sync).observe(element);
  requestAnimationFrame(sync);
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

  observeHorizontalOverflow(explanation);
  observeHorizontalOverflow(formula);
  return row;
}

export function renderSteps(container, items) {
  container.replaceChildren(...items.map(createStepRow));
  container.classList.toggle("visible", items.length > 0);
}

export function renderAnswer(container, latex) {
  container.replaceChildren(makeMathField(latex));
  container.classList.toggle("visible", Boolean(latex));
  requestAnimationFrame(() => syncHorizontalOverflow(container));
}

export function clearCalculationRows(steps, answer) {
  steps.replaceChildren();
  steps.classList.remove("visible");
  answer.replaceChildren();
  answer.classList.remove("visible");
}
