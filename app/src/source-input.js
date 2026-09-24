const symbol = /^[A-Za-z][A-Za-z0-9]*$/;

const invalid = (code) => ({ ok: false, diagnostics: [{ severity: "error", code }] });

function parseAssumption(text) {
  const match = text.match(/^\s*(.*?)\s*(>|<|!=|≠)\s*(.*?)\s*$/);
  if (!match) return null;
  const [, left, operator, right] = match;
  let name;
  let fact;
  if (symbol.test(left) && right === "0") {
    name = left;
    fact = operator === ">" ? "positive" : operator === "<" ? "negative" : "non_zero";
  } else if (left === "0" && symbol.test(right)) {
    name = right;
    fact = operator === "<" ? "positive" : operator === ">" ? "negative" : "non_zero";
  } else return null;
  return { symbol: name, fact };
}

export function parseYaClueSource(input) {
  const source = input.trim();
  if (!source) return invalid("empty-expression");
  if (/[\r\n]/.test(source)) return invalid("multiline-expression");

  let depth = 0;
  const separators = [];
  for (let index = 0; index < source.length; index++) {
    const char = source[index];
    if (char === "(" || char === "{") depth++;
    else if (char === ")" || char === "}") depth--;
    else if (char === ";" && depth === 0) separators.push(index);
  }
  if (separators.length > 1) return invalid("invalid-assumption");
  if (separators.length === 0) return { ok: true, expression: source, assumptions: [], diagnostics: [] };

  const expression = source.slice(0, separators[0]).trim();
  if (!expression) return invalid("empty-expression");
  const parts = source.slice(separators[0] + 1).split(",");
  if (parts.some((part) => !part.trim())) return invalid("invalid-assumption");
  const assumptions = [];
  for (const part of parts) {
    const assumption = parseAssumption(part);
    if (!assumption) return invalid("invalid-assumption");
    if (assumptions.some((item) => item.symbol === assumption.symbol
      && ((item.fact === "positive" && assumption.fact === "negative")
        || (item.fact === "negative" && assumption.fact === "positive"))))
      return invalid("invalid-assumption");
    if (!assumptions.some((item) => item.symbol === assumption.symbol && item.fact === assumption.fact))
      assumptions.push(assumption);
  }
  return { ok: true, expression, assumptions, diagnostics: [] };
}
