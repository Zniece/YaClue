import assert from "node:assert/strict";
import test from "node:test";
import { getHelpSections } from "../src/help.js";

test("help covers the documented input rules in both languages", () => {
  for (const locale of ["zh-CN", "en-US"]) {
    const sections = getHelpSections(locale);
    assert.equal(sections.length, 8);
    for (const section of sections) {
      assert.ok(section.title && section.text);
      assert.ok(!section.text.includes("我是帮助"));
    }
    const examples = sections.flatMap((section) => section.examples);
    for (const expected of ["D(x)x^2", "Solve(x^2-5*x+6==0,x)", "Sqrt(x^2);x>0", "Determinant({{1,2},{3,4}})"])
      assert.ok(examples.includes(expected), `${locale}: ${expected}`);
  }
});
