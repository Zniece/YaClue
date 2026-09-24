import assert from "node:assert/strict";
import test from "node:test";
import { parseYaClueSource } from "../src/source-input.js";

test("direct YaClue input preserves expression syntax and one-shot assumptions", () => {
  assert.deepEqual(parseYaClueSource("D(x)Sqrt(x^2)"), {
    ok: true, expression: "D(x)Sqrt(x^2)", assumptions: [], diagnostics: [],
  });
  assert.deepEqual(parseYaClueSource(" Sqrt(x^2);x>0,y!=0 "), {
    ok: true, expression: "Sqrt(x^2)",
    assumptions: [{ symbol: "x", fact: "positive" }, { symbol: "y", fact: "non_zero" }], diagnostics: [],
  });
  assert.deepEqual(parseYaClueSource("x;0>x,0≠y").assumptions, [
    { symbol: "x", fact: "negative" }, { symbol: "y", fact: "non_zero" },
  ]);
});

test("direct input rejects statement separators and malformed assumptions", () => {
  for (const source of ["", "x\ny", "x;", "x;x>=0", "x;x>0; y<0", "x;x>0,x<0", "x;x>0,"])
    assert.equal(parseYaClueSource(source).ok, false, source);
});
