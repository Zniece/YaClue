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
    "result.evaluation": "计算结果", "result.composition": "组合运算", "result.algebra": "代数变换",
    "result.substitution": "变量替换", "result.taylor": "Taylor 多项式", "result.matrix": "线性代数",
    "result.ode": "常微分方程", "result.numeric": "数值近似", "result.series": "级数",
    "result.defined_object": "数学对象", "result.double_integral": "二重积分", "result.polar_integral": "极坐标积分",
    "result.numeric_ode": "常微分方程数值解", "result.numeric_root": "数值根", "result.extrema": "无约束极值",
    "result.lagrange": "约束极值", "result.multivariate": "多元微分", "result.line_integral": "线积分",
    "result.surface_integral": "曲面积分", "fact.Real": "实数", "fact.Integer": "整数",
    "fact.Positive": "正数", "fact.Negative": "负数", "fact.NonZero": "非零",
    "reason.condition_insufficient": "条件不足", "reason.algorithm_uncovered": "算法未覆盖",
    "reason.mathematical_absence": "数学上不存在", "reason.divergent": "发散",
    "reason.unsupported_operation": "不支持的运算",
    "template.derivative.label": "求导", "template.derivative.help": "D(变量[,阶数])表达式",
    "template.indefiniteIntegral.label": "不定积分", "template.indefiniteIntegral.help": "Integrate(变量)表达式",
    "template.definiteIntegral.label": "定积分", "template.definiteIntegral.help": "Integrate(变量,下限,上限)表达式",
    "template.limit.label": "极限", "template.limit.help": "支持 Limit(表达式,趋近值)，或 Limit(变量,趋近值[,方向])表达式",
    "template.taylor.label": "Taylor 展开", "template.taylor.help": "支持 Taylor(表达式,展开点,次数)，默认变量为 x",
    "template.doubleIntegral.label": "二重积分", "template.doubleIntegral.help": "被积式、内层变量与上下限、外层变量与上下限",
    "template.polarIntegral.label": "极坐标积分", "template.polarIntegral.help": "被积式、直角变量、极坐标变量及边界",
    "template.solveEquation.label": "解代数方程", "template.solveEquation.help": "Solve(方程,变量)；单独输入 == 只构造方程",
    "template.equationSystem.label": "方程组", "template.equationSystem.help": "Solve({方程...},{变量...})",
    "template.ode.label": "常微分方程", "template.ode.help": "当前标准形式使用自变量 x、因变量 y",
    "template.numericOde.label": "ODE 数值解", "template.numericOde.help": "方程、自变量、因变量、起点、初值、终点",
    "template.factor.label": "因式分解", "template.factor.help": "Factor(表达式)",
    "template.expand.label": "展开", "template.expand.help": "Expand(表达式)",
    "template.simplify.label": "化简", "template.simplify.help": "Simplify(表达式)",
    "template.tidy.label": "整理", "template.tidy.help": "Tidy(表达式)",
    "template.apart.label": "部分分式", "template.apart.help": "Apart(表达式,变量)",
    "template.extrema.label": "无约束极值", "template.extrema.help": "Extrema(表达式,x变量,y变量)",
    "template.lagrange.label": "约束极值", "template.lagrange.help": "约束按等于 0 的表达式输入",
    "template.matrixMultiply.label": "矩阵乘法", "template.matrixMultiply.help": "直接使用 + 或 *",
    "template.determinant.label": "行列式", "template.determinant.help": "Determinant(矩阵)",
    "template.inverse.label": "逆矩阵", "template.inverse.help": "Inverse(矩阵)",
    "template.transpose.label": "转置", "template.transpose.help": "Transpose(矩阵)",
    "template.eigenvalues.label": "特征值", "template.eigenvalues.help": "EigenValues(矩阵)",
    "template.matrixSolve.label": "线性方程组", "template.matrixSolve.help": "MatrixSolve(系数矩阵,常数向量)",
    "template.numeric.label": "高精度近似", "template.numeric.help": "N(表达式,精度)",
    "template.findRoot.label": "数值求根", "template.findRoot.help": "FindRoot(表达式,变量,初值)",
    "template.plot.label": "函数绘图", "template.plot.help": "Plot(表达式,变量,数值下界,数值上界)",
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
    "result.evaluation": "Result", "result.composition": "Composition", "result.algebra": "Algebraic transformation",
    "result.substitution": "Substitution", "result.taylor": "Taylor polynomial", "result.matrix": "Linear algebra",
    "result.ode": "Ordinary differential equation", "result.numeric": "Numeric approximation", "result.series": "Series",
    "result.defined_object": "Mathematical object", "result.double_integral": "Double integral", "result.polar_integral": "Polar integral",
    "result.numeric_ode": "Numeric ODE solution", "result.numeric_root": "Numeric root", "result.extrema": "Unconstrained extrema",
    "result.lagrange": "Constrained extrema", "result.multivariate": "Multivariate calculus", "result.line_integral": "Line integral",
    "result.surface_integral": "Surface integral", "fact.Real": "Real", "fact.Integer": "Integer",
    "fact.Positive": "Positive", "fact.Negative": "Negative", "fact.NonZero": "Non-zero",
    "reason.condition_insufficient": "Insufficient conditions", "reason.algorithm_uncovered": "Algorithm not available",
    "reason.mathematical_absence": "No mathematical result", "reason.divergent": "Divergent",
    "reason.unsupported_operation": "Unsupported operation",
    "template.derivative.label": "Derivative", "template.derivative.help": "D(variable[, order]) expression",
    "template.indefiniteIntegral.label": "Indefinite integral", "template.indefiniteIntegral.help": "Integrate(variable) expression",
    "template.definiteIntegral.label": "Definite integral", "template.definiteIntegral.help": "Integrate(variable, lower, upper) expression",
    "template.limit.label": "Limit", "template.limit.help": "Limit(expression, point), or Limit(variable, point[, direction]) expression",
    "template.taylor.label": "Taylor expansion", "template.taylor.help": "Taylor(expression, point, order); the default variable is x",
    "template.doubleIntegral.label": "Double integral", "template.doubleIntegral.help": "Integrand, inner variable and bounds, outer variable and bounds",
    "template.polarIntegral.label": "Polar integral", "template.polarIntegral.help": "Integrand, Cartesian variables, polar variables and bounds",
    "template.solveEquation.label": "Solve equation", "template.solveEquation.help": "Solve(equation, variable); == alone only constructs an equation",
    "template.equationSystem.label": "Equation system", "template.equationSystem.help": "Solve({equations...},{variables...})",
    "template.ode.label": "ODE", "template.ode.help": "The standard form currently uses x as independent and y as dependent variable",
    "template.numericOde.label": "Numeric ODE", "template.numericOde.help": "Equation, independent and dependent variables, start, initial value, end",
    "template.factor.label": "Factor", "template.factor.help": "Factor(expression)",
    "template.expand.label": "Expand", "template.expand.help": "Expand(expression)",
    "template.simplify.label": "Simplify", "template.simplify.help": "Simplify(expression)",
    "template.tidy.label": "Tidy", "template.tidy.help": "Tidy(expression)",
    "template.apart.label": "Partial fractions", "template.apart.help": "Apart(expression, variable)",
    "template.extrema.label": "Unconstrained extrema", "template.extrema.help": "Extrema(expression, x-variable, y-variable)",
    "template.lagrange.label": "Constrained extrema", "template.lagrange.help": "Enter the constraint as an expression equal to zero",
    "template.matrixMultiply.label": "Matrix multiplication", "template.matrixMultiply.help": "Use + or * directly",
    "template.determinant.label": "Determinant", "template.determinant.help": "Determinant(matrix)",
    "template.inverse.label": "Inverse matrix", "template.inverse.help": "Inverse(matrix)",
    "template.transpose.label": "Transpose", "template.transpose.help": "Transpose(matrix)",
    "template.eigenvalues.label": "Eigenvalues", "template.eigenvalues.help": "EigenValues(matrix)",
    "template.matrixSolve.label": "Linear system", "template.matrixSolve.help": "MatrixSolve(coefficient matrix, constant vector)",
    "template.numeric.label": "High-precision value", "template.numeric.help": "N(expression, precision)",
    "template.findRoot.label": "Numeric root", "template.findRoot.help": "FindRoot(expression, variable, initial value)",
    "template.plot.label": "Plot", "template.plot.help": "Plot(expression, variable, numeric lower bound, numeric upper bound)",
  },
};

const referenceKeys = Object.keys(locales["en-US"]).sort();
for (const [name, messages] of Object.entries(locales)) {
  const keys = Object.keys(messages).sort();
  const missing = referenceKeys.filter((key) => !Object.prototype.hasOwnProperty.call(messages, key));
  const extra = keys.filter((key) => !Object.prototype.hasOwnProperty.call(locales["en-US"], key));
  if (missing.length || extra.length) {
    throw new Error(`Locale ${name} has inconsistent keys (missing: ${missing.join(", ")}; extra: ${extra.join(", ")})`);
  }
}

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
