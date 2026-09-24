import assert from "node:assert/strict";
import test from "node:test";

const observers = [];
const frames = new Map();
let nextFrame = 0;

class ElementStub {
  children = [];
  listeners = new Map();
  attributes = new Map();
  classes = new Set();
  classList = {
    toggle: (name, enabled) => enabled ? this.classes.add(name) : this.classes.delete(name),
    remove: (name) => this.classes.delete(name),
  };
  scrollWidth = 10;
  clientWidth = 10;
  scrollLeft = 0;
  clientHeight = 400;
  scrollTop = 0;
  get scrollHeight() { return this.children.length * 54; }

  append(...children) { this.children.push(...children); }
  appendChild(child) { this.children.push(child); }
  replaceChildren(...children) { this.children = children; }
  addEventListener(name, callback) { this.listeners.set(name, callback); }
  removeEventListener(name, callback) {
    if (this.listeners.get(name) === callback) this.listeners.delete(name);
  }
  setAttribute(name, value) { this.attributes.set(name, value); }
  getAttribute(name) { return this.attributes.get(name) ?? null; }
}

globalThis.document = { createElement: () => new ElementStub() };
globalThis.ResizeObserver = class {
  disconnected = false;
  constructor(callback) { this.callback = callback; observers.push(this); }
  observe(element) { this.element = element; }
  disconnect() { this.disconnected = true; }
};
globalThis.requestAnimationFrame = (callback) => {
  const id = ++nextFrame;
  frames.set(id, callback);
  return id;
};
globalThis.cancelAnimationFrame = (id) => frames.delete(id);

const { renderSteps, renderAnswer, clearCalculationRows } = await import("../src/math-rows.js");

test("replacing and clearing steps releases observers and scroll listeners", () => {
  const steps = new ElementStub();
  const answer = new ElementStub();
  renderSteps(steps, [{ explanation: "first", latex: "x" }, { explanation: "second", latex: "y" }]);
  const oldRows = [...steps.children];
  assert.equal(observers.length, 5);
  assert.equal(frames.size, 5);

  renderSteps(steps, [{ explanation: "replacement", latex: "z" }]);
  assert.equal(observers.filter((observer) => observer.disconnected).length, 5);
  assert.equal(frames.size, 3);
  for (const row of oldRows) {
    assert.equal(row.children[0].children[1].listeners.size, 0);
    assert.equal(row.children[0].listeners.size, 0);
    assert.equal(row.children[1].listeners.size, 0);
  }

  renderAnswer(answer, "1");
  renderAnswer(answer, "2");
  assert.equal(frames.size, 4);
  clearCalculationRows(steps, answer);
  assert.equal(observers.filter((observer) => observer.disconnected).length, 8);
  assert.equal(frames.size, 0);
  assert.equal(steps.children.length, 0);
  assert.equal(answer.children.length, 0);
});

test("hundreds of steps load in order as the user approaches the end", () => {
  const steps = new ElementStub();
  const answer = new ElementStub();
  const items = Array.from({ length: 500 }, (_, index) => ({
    explanation: `Step ${index + 1}`,
    latex: String(index + 1),
  }));
  renderSteps(steps, items);
  assert.equal(steps.children.length, 24);

  const advanceFrame = () => {
    const callbacks = [...frames.values()];
    frames.clear();
    for (const callback of callbacks) callback();
  };
  advanceFrame();
  assert.equal(steps.children.length, 24);

  steps.scrollTop = 900;
  steps.listeners.get("scroll")();
  advanceFrame();
  assert.equal(steps.children.length, 48);
  assert.equal(steps.children[47].children[0].children[0].textContent, "48");

  clearCalculationRows(steps, answer);
  assert.equal(steps.listeners.size, 0);
  assert.equal(observers.filter((observer) => !observer.disconnected).length, 0);
  assert.equal(frames.size, 0);
});

test("a hidden calculation view pauses batches until it becomes visible", () => {
  const steps = new ElementStub();
  const answer = new ElementStub();
  steps.clientHeight = 0;
  renderSteps(steps, Array.from({ length: 100 }, (_, index) => ({ latex: String(index) })));
  const advanceFrame = () => {
    const callbacks = [...frames.values()];
    frames.clear();
    for (const callback of callbacks) callback();
  };
  advanceFrame();
  assert.equal(steps.children.length, 24);
  assert.equal(frames.size, 0);

  steps.clientHeight = 2000;
  const containerObserver = observers.findLast((observer) => observer.element === steps);
  containerObserver.callback();
  advanceFrame();
  assert.equal(steps.children.length, 48);
  clearCalculationRows(steps, answer);
});

test("step explanation expands, and swiping its preview does not toggle it", () => {
  const steps = new ElementStub();
  const answer = new ElementStub();
  renderSteps(steps, [{ explanation: "A long explanation that needs more space", latex: "x+1" }]);
  const [meta, formula, detail] = steps.children[0].children;
  assert.equal(meta.getAttribute("aria-expanded"), "false");
  assert.equal(meta.getAttribute("aria-controls"), detail.id);
  assert.equal(detail.hidden, true);
  assert.equal(detail.textContent, "A long explanation that needs more space");
  assert.equal(formula.children[0].value, "x+1");

  meta.listeners.get("click")({ detail: 1, preventDefault() {} });
  assert.equal(detail.hidden, false);
  assert.equal(meta.getAttribute("aria-expanded"), "true");
  meta.listeners.get("click")({ detail: 0, preventDefault() {} });
  assert.equal(detail.hidden, true);

  meta.listeners.get("pointerdown")({ clientX: 20, clientY: 10 });
  meta.listeners.get("pointermove")({ clientX: 45, clientY: 10 });
  let prevented = false;
  meta.listeners.get("click")({ detail: 1, preventDefault() { prevented = true; } });
  assert.equal(prevented, true);
  assert.equal(detail.hidden, true);
  meta.listeners.get("pointerdown")({ clientX: 20, clientY: 10 });
  meta.listeners.get("pointercancel")();
  meta.listeners.get("click")({ detail: 0, preventDefault() {} });
  assert.equal(detail.hidden, false);
  clearCalculationRows(steps, answer);
});

test("read-only math fields stay out of Tab order while formula and answer remain reachable", () => {
  const steps = new ElementStub();
  const answer = new ElementStub();
  renderSteps(steps, [{ explanation: "A long explanation", latex: "x+1" }]);
  const formula = steps.children[0].children[1];
  const meta = steps.children[0].children[0];
  const explanation = meta.children[1];
  assert.equal(steps.tabIndex, 0);
  assert.equal(formula.children[0].tabIndex, -1);

  formula.scrollWidth = 500;
  explanation.scrollWidth = 500;
  for (const observer of observers.filter((candidate) => candidate.element === formula || candidate.element === explanation))
    observer.callback();
  assert.equal(formula.tabIndex, 0);
  assert.equal(explanation.tabIndex, -1);
  const keyEvent = { key: "ArrowRight", preventDefault() {} };
  meta.listeners.get("keydown")(keyEvent);
  assert.equal(explanation.scrollLeft, 40);

  renderAnswer(answer, "x+1");
  assert.equal(answer.tabIndex, 0);
  assert.equal(answer.children[0].tabIndex, -1);
  clearCalculationRows(steps, answer);
  assert.equal(steps.tabIndex, -1);
  assert.equal(answer.tabIndex, -1);
});
