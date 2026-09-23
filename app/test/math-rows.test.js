import assert from "node:assert/strict";
import test from "node:test";

const observers = [];
const frames = new Map();
let nextFrame = 0;

class ElementStub {
  children = [];
  listeners = new Map();
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
