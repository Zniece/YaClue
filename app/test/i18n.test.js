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
const { yaclueKeyboardLayouts } = await import("../src/keyboard.js");

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

test("every script step explanation has a translation in both locales", () => {
  const script = readFileSync(new URL("../../processing/scripts/steps.rep/code.ys", import.meta.url), "utf8");
  const rules = [...new Set([...script.matchAll(/Steps'Text\("([^"]+)"/g)].map((match) => match[1]))];
  for (const locale of ["zh-CN", "en-US"]) {
    setLocale(locale);
    for (const rule of rules) assert.equal(hasLocaleTranslation(`steps.${rule}`, locale), true, `${locale}: ${rule}`);
  }
});
