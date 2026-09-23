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
  assert.equal(observers.length, 4);
  assert.equal(frames.size, 4);

  renderSteps(steps, [{ explanation: "replacement", latex: "z" }]);
  assert.equal(observers.filter((observer) => observer.disconnected).length, 4);
  assert.equal(frames.size, 2);
  for (const row of oldRows) {
    assert.equal(row.children[0].children[1].listeners.size, 0);
    assert.equal(row.children[1].listeners.size, 0);
  }

  renderAnswer(answer, "1");
  renderAnswer(answer, "2");
  assert.equal(frames.size, 3);
  clearCalculationRows(steps, answer);
  assert.equal(observers.filter((observer) => observer.disconnected).length, 6);
  assert.equal(frames.size, 0);
  assert.equal(steps.children.length, 0);
  assert.equal(answer.children.length, 0);
});
