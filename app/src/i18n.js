const STORAGE_KEY = "yaclue.locale";

const locales = {
  "zh-CN": {
    tagline: "可解释的符号计算器", engineReady: "引擎就绪", engineBusy: "正在计算",
    calculateEyebrow: "计算", prompt: "你想计算什么？", outputMode: "输出方式",
    showSteps: "显示步骤", resultOnly: "只看结果", verbosity: "步骤粒度",
    concise: "简洁", standard: "标准", detailed: "详细", shortcuts: "快捷输入",
    shortcutHelp: "在光标处插入表达式模板", expression: "数学表达式",
    clearInput: "清空输入", calculate: "计算", context: "计算上下文与输入帮助",
    noAssumptions: "无假设", currentAssumptions: "当前假设", assumptionSymbol: "假设符号",
    assumptionFact: "假设性质", real: "实数", integer: "整数", positive: "正数",
    negative: "负数", nonZero: "非零", add: "添加", noneYet: "暂无假设",
    clearAll: "清除全部假设", commonInput: "常用输入", multiplication: "乘法",
    power: "乘方", constructEquation: "构造方程", pi: "圆周率", infinity: "无穷",
    equationHelp: "方程本身是数学对象；求解时请显式使用 Solve 或 OdeSolve。模板使用 YaClue 数学输入运算符。",
    resultEyebrow: "结果", result: "计算结果", structuredResult: "结构化结果",
    emptyResult: "输入表达式后开始计算。", devConsole: "命令行测试组件",
    devOnly: "仅用于开发测试 · JSON Lines",
    devHelp: "每行输入一个表达式，结果逐行输出为 JSON。也可输入含 expression、steps、verbosity 的 JSON 请求。",
    run: "运行", clearOutput: "清空输出", language: "语言", completed: "计算完成",
    computation: "计算", analysisBasis: "分析依据", retry: "（可以重试）",
    finitePlotError: "采样结果中没有有限点", assumptionCount: "{count} 项假设",
    kind_scalar: "标量", kind_expression: "符号表达式", kind_equation: "方程", kind_matrix: "矩阵",
    kind_solution_set: "解集", kind_function_family: "函数族", kind_unevaluated: "未求值对象",
    exact_exact: "精确", exact_symbolic: "符号", exact_approximate: "近似", exact_unknown: "精确性未知",
    symbols: "符号：{value}", boundSymbols: "绑定：{value}", constants: "常量：{value}",
    conditional: "条件化结果", representative: "代表解",
    "result.partial_application": "部分应用", "result.derivative": "导数", "result.integral": "积分",
    "result.limit": "极限", "result.equation": "方程", "result.plot": "函数图像",
  },
  "en-US": {
    tagline: "Explainable symbolic calculator", engineReady: "Engine ready", engineBusy: "Calculating",
    calculateEyebrow: "Calculate", prompt: "What would you like to calculate?", outputMode: "Output",
    showSteps: "Show steps", resultOnly: "Result only", verbosity: "Step detail",
    concise: "Concise", standard: "Standard", detailed: "Detailed", shortcuts: "Templates",
    shortcutHelp: "Insert an expression template at the cursor", expression: "Mathematical expression",
    clearInput: "Clear input", calculate: "Calculate", context: "Context and input help",
    noAssumptions: "No assumptions", currentAssumptions: "Current assumptions", assumptionSymbol: "Assumption symbol",
    assumptionFact: "Assumption property", real: "Real", integer: "Integer", positive: "Positive",
    negative: "Negative", nonZero: "Non-zero", add: "Add", noneYet: "No assumptions yet",
    clearAll: "Clear all", commonInput: "Common syntax", multiplication: "multiply",
    power: "power", constructEquation: "construct equation", pi: "pi", infinity: "infinity",
    equationHelp: "An equation is a mathematical object. Use Solve or OdeSolve explicitly to solve it.",
    resultEyebrow: "Result", result: "Result", structuredResult: "Structured result",
    emptyResult: "Enter an expression to begin.", devConsole: "Command-line test component",
    devOnly: "Development only · JSON Lines",
    devHelp: "Enter one expression per line. Results are emitted as JSON Lines. JSON requests may include expression, steps, and verbosity.",
    run: "Run", clearOutput: "Clear output", language: "Language", completed: "Completed",
    computation: "Computation", analysisBasis: "Analysis", retry: " (retry available)",
    finitePlotError: "The sampled result contains no finite points", assumptionCount: "{count} assumptions",
    kind_scalar: "Scalar", kind_expression: "Symbolic expression", kind_equation: "Equation", kind_matrix: "Matrix",
    kind_solution_set: "Solution set", kind_function_family: "Function family", kind_unevaluated: "Unevaluated object",
    exact_exact: "Exact", exact_symbolic: "Symbolic", exact_approximate: "Approximate", exact_unknown: "Unknown exactness",
    symbols: "Symbols: {value}", boundSymbols: "Bound: {value}", constants: "Constants: {value}",
    conditional: "Conditional result", representative: "Representative solution",
    "result.partial_application": "Partial application", "result.derivative": "Derivative", "result.integral": "Integral",
    "result.limit": "Limit", "result.equation": "Equation", "result.plot": "Plot",
  },
};

function initialLocale() {
  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved && locales[saved]) return saved;
  return navigator.language?.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US";
}

let locale = initialLocale();

export function t(key, args = {}) {
  const template = locales[locale][key] ?? locales["en-US"][key] ?? key;
  return Object.entries(args).reduce((text, [name, value]) => text.replaceAll(`{${name}}`, value), template);
}

export function hasTranslation(key) {
  return Object.prototype.hasOwnProperty.call(locales[locale], key)
    || Object.prototype.hasOwnProperty.call(locales["en-US"], key);
}

export function getLocale() { return locale; }

export function setLocale(next) {
  if (!locales[next]) return;
  locale = next;
  localStorage.setItem(STORAGE_KEY, next);
  applyTranslations();
  document.dispatchEvent(new CustomEvent("localechange", { detail: next }));
}

export function applyTranslations(root = document) {
  document.documentElement.lang = locale;
  root.querySelectorAll("[data-i18n]").forEach((element) => { element.textContent = t(element.dataset.i18n); });
  root.querySelectorAll("[data-i18n-title]").forEach((element) => { element.title = t(element.dataset.i18nTitle); });
  root.querySelectorAll("[data-i18n-aria-label]").forEach((element) => element.setAttribute("aria-label", t(element.dataset.i18nAriaLabel)));
}
