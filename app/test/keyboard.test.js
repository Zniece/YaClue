import assert from "node:assert/strict";
import test from "node:test";
import { yaclueKeyboardLayouts } from "../src/keyboard.js";

test("every keyboard page keeps the numeric and editing keys in fixed positions", () => {
  const common = yaclueKeyboardLayouts[0].rows.map((row) => row.slice(4));
  assert.ok(yaclueKeyboardLayouts.length >= 2);
  for (const layout of yaclueKeyboardLayouts) {
    assert.equal(layout.rows.length, 5, layout.id);
    for (const [index, row] of layout.rows.entries()) {
      assert.equal(row.length, 8, `${layout.id} row ${index + 1}`);
      assert.deepEqual(row.slice(4), common[index], `${layout.id} row ${index + 1}`);
    }
  }
  assert.deepEqual(common.slice(0, 3).map((row) => row.slice(0, 3)), [
    ["[7]", "[8]", "[9]"],
    ["[4]", "[5]", "[6]"],
    ["[1]", "[2]", "[3]"],
  ]);
});

test("the basic keyboard retains fraction, power, trig, variables and infinity", () => {
  const keys = yaclueKeyboardLayouts[0].rows.flatMap((row) => row.slice(0, 4));
  for (const token of ["\\frac{#@}{#0}", "#@^{#0}", "\\sin\\left(#0\\right)", "x", "\\infty"])
    assert.ok(keys.some((key) => typeof key === "object" ? key.insert === token || key.latex === token : key === token), token);
});
