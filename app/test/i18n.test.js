import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const saved = new Map([["yaclue.locale", "zh-CN"]]);
globalThis.localStorage = {
  getItem: (key) => saved.get(key) ?? null,
  setItem: (key, value) => saved.set(key, value),
};

const textNode = { dataset: { i18n: "ui.help" }, textContent: "" };
const ariaNode = {
  dataset: { i18nAriaLabel: "ui.mainNavigation" },
  setAttribute(name, value) { this[name] = value; },
};
const localeEvents = [];
globalThis.document = {
  documentElement: { lang: "" },
  querySelectorAll(selector) {
    if (selector === "[data-i18n]") return [textNode];
    if (selector === "[data-i18n-aria-label]") return [ariaNode];
    return [];
  },
  dispatchEvent(event) { localeEvents.push(event.detail); },
};
globalThis.CustomEvent = class { constructor(_name, options) { this.detail = options.detail; } };

const { applyTranslations, getLocale, hasLocaleTranslation, setLocale, t } = await import("../src/i18n.js");
const { YACLUE_KEY_SEMANTICS, localizeKeyboardLayouts, yaclueKeyboardLayouts } = await import("../src/keyboard.js");

test("all app shell translation markers exist in both languages", () => {
  const html = readFileSync(new URL("../src/index.html", import.meta.url), "utf8");
  const keys = [...html.matchAll(/data-i18n(?:-aria-label|-title)?="([^"]+)"/g)].map((match) => match[1]);
  for (const locale of ["zh-CN", "en-US"]) {
    setLocale(locale);
    for (const key of keys) assert.equal(hasLocaleTranslation(key, locale), true, `${locale}: ${key}`);
    for (const layout of yaclueKeyboardLayouts)
      assert.equal(hasLocaleTranslation(`keyboard.${layout.id.slice("yaclue-".length)}`, locale), true, `${locale}: ${layout.id}`);
  }
});

test("switching language updates the document and persists the choice", () => {
  setLocale("zh-CN");
  applyTranslations();
  assert.equal(textNode.textContent, "帮助");
  assert.equal(ariaNode["aria-label"], "主导航");

  setLocale("en-US");
  assert.equal(getLocale(), "en-US");
  assert.equal(saved.get("yaclue.locale"), "en-US");
  assert.equal(document.documentElement.lang, "en-US");
  assert.equal(textNode.textContent, "Help");
  assert.equal(ariaNode["aria-label"], "Main navigation");
  assert.equal(t("input.missing-slot"), "Complete the empty input slot");
  assert.equal(localeEvents.at(-1), "en-US");
});

test("keyboard hints and first-use guidance are translated in both languages", () => {
  for (const locale of ["zh-CN", "en-US"]) {
    setLocale(locale);
    assert.equal(hasLocaleTranslation("ui.firstCalculationHint", locale), true);
    for (const name of Object.keys(YACLUE_KEY_SEMANTICS))
      assert.equal(hasLocaleTranslation(`keyboard.key.${name}`, locale), true, `${locale}: ${name}`);
    for (const name of ["digit", "variable", "moveLeft", "moveRight", "backspace", "calculate", "openParenthesis", "closeParenthesis", "pi", "addRow", "addCol"])
      assert.equal(hasLocaleTranslation(`keyboard.hint.${name}`, locale), true, `${locale}: ${name}`);

    const layouts = localizeKeyboardLayouts(t);
    assert.equal(layouts.length, yaclueKeyboardLayouts.length);
    for (const layout of layouts) {
      assert.ok(layout.tooltip && !layout.tooltip.startsWith("keyboard."), `${locale}: ${layout.id}`);
      for (const key of layout.rows.flat())
        assert.ok(key.tooltip && !key.tooltip.startsWith("keyboard."), `${locale}: ${layout.id} ${key.label || key.latex}`);
    }
    const commonKeys = layouts[0].rows.map((row) => row.slice(4));
    for (const layout of layouts)
      layout.rows.forEach((row, index) => assert.deepEqual(row.slice(4), commonKeys[index], `${locale}: ${layout.id} row ${index}`));
    assert.equal(layouts[0].rows[4][7].label, "[action]");
  }
});

test("every script step explanation has a translation in both locales", () => {
  const script = readFileSync(new URL("../../processing/scripts/steps.rep/code.ys", import.meta.url), "utf8");
  const rules = [...new Set([...script.matchAll(/Steps'Text\("([^"]+)"/g)].map((match) => match[1]))];
  for (const locale of ["zh-CN", "en-US"]) {
    setLocale(locale);
    for (const rule of rules) assert.equal(hasLocaleTranslation(`steps.${rule}`, locale), true, `${locale}: ${rule}`);
  }
});
