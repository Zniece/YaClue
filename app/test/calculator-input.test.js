import assert from "node:assert/strict";
import test from "node:test";
import { materializeDisplayedZero } from "../src/calculator-input.js";

test("the displayed zero becomes an expression only when submitted empty", () => {
  const field = { value: "" };
  assert.equal(materializeDisplayedZero(field), true);
  assert.equal(field.value, "0");
});

test("an entered expression is not replaced by the displayed zero", () => {
  for (const expression of ["7", "x", "\\sin\\left(\\placeholder{}\\right)"]) {
    const field = { value: expression };
    assert.equal(materializeDisplayedZero(field), false);
    assert.equal(field.value, expression);
  }
});
