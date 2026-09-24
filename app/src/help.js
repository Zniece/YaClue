const content = {
  "zh-CN": [
    { title: "开始计算", text: "在计算页输入一个数学表达式，按数学键盘上的回车计算。结果显示在键盘上方；有步骤时，可在输入行下方上下滚动查看。键盘可收起，以便查看较长步骤。", examples: ["2*x^2-3*x+1", "Sin(x)^2+Cos(x)^2"] },
    { title: "基本记法", text: "使用 * 表示乘法、^ 表示乘方。名称以英文字母开头，可包含英文字母和数字，并区分大小写。== 构造方程；要求解，请使用 Solve。一次只提交一个表达式，不支持赋值、定义或多语句程序。", examples: ["x^2-5*x+6==0", "Solve(x^2-5*x+6==0,x)"] },
    { title: "函数与常量", text: "函数名区分大小写。常用函数有 Sin、Cos、Tan、Exp、Ln、Sqrt、Abs、Gamma 和 Zeta。Pi 是常量。绝对值使用 Abs，向量范数使用 Norm 或 PNorm。", examples: ["Sqrt(2)+Pi", "Norm({3,4})", "PNorm({x,y},2)"] },
    { title: "导数、积分与极限", text: "D(var)expr 求导；D(var,n)expr 求 n 阶导数。Integrate(var)expr 求不定积分，Integrate(var,a,b)expr 求定积分。Limit(expr,a) 使用默认变量；也可用 Limit(var,a)expr 指定变量。变量只在其后所作用的表达式中绑定。", examples: ["D(x)x^2", "D(x,2)Sin(x)", "Integrate(x,0,1)x^2", "Limit(x,0)Sin(x)/x"] },
    { title: "列表、向量与矩阵", text: "列表用花括号；矩阵由等长的行列表组成。向量使用列表记法。可对矩阵使用 Determinant、Inverse、Transpose、Rank 等运算。", examples: ["{1,2,3}", "{{1,2},{3,4}}", "Determinant({{1,2},{3,4}})"] },
    { title: "单次计算假设", text: "在表达式后加一个顶层分号，分号后写本次计算的假设。多个假设以逗号分隔。支持 var>0、var<0、var!=0 及左右对调的等价形式。假设在本次计算结束后自动清除。", examples: ["Sqrt(x^2);x>0", "x/y;x>0,y!=0"] },
    { title: "部分输入与组合", text: "部分运算可以先输入参数，稍后补上最后的表达式，例如 D(x) 和 Integrate(x)。这不是所有函数都适用的自动柯里化：缺少非末尾参数、参数类型错误或形式未登记时，仍会提示无效或保留为未决对象。内层结果可以继续参与外层运算。", examples: ["D(x)", "D(x)Integrate(t,0,x)Sin(t^2)", "N(Determinant({{1,2},{3,4}}))"] },
    { title: "结果可能的状态", text: "合法输入不保证一定得到封闭形式答案。结果可能是数值或符号值、等待补全的部分应用、暂时无法求出的未决对象，或无解、发散等数学结论。它们与输入错误不同。", examples: [] },
  ],
  "en-US": [
    { title: "Getting started", text: "Enter one mathematical expression on the calculator page, then press Return on the math keyboard. The answer appears above the keyboard. Scroll the steps below the input, or collapse the keyboard for more room.", examples: ["2*x^2-3*x+1", "Sin(x)^2+Cos(x)^2"] },
    { title: "Basic notation", text: "Use * for multiplication and ^ for powers. Names start with an ASCII letter, may contain letters and digits, and are case-sensitive. == constructs an equation; use Solve to solve it. Submit one expression at a time; assignments, definitions and multi-statement programs are not supported.", examples: ["x^2-5*x+6==0", "Solve(x^2-5*x+6==0,x)"] },
    { title: "Functions and constants", text: "Function names are case-sensitive. Common functions include Sin, Cos, Tan, Exp, Ln, Sqrt, Abs, Gamma and Zeta. Pi is a constant. Use Abs for absolute value and Norm or PNorm for vector norms.", examples: ["Sqrt(2)+Pi", "Norm({3,4})", "PNorm({x,y},2)"] },
    { title: "Derivatives, integrals and limits", text: "D(var)expr differentiates; D(var,n)expr gives the nth derivative. Integrate(var)expr is an indefinite integral; Integrate(var,a,b)expr is definite. Limit(expr,a) uses the default variable, or use Limit(var,a)expr to specify it. Bound variables apply only to the following operand.", examples: ["D(x)x^2", "D(x,2)Sin(x)", "Integrate(x,0,1)x^2", "Limit(x,0)Sin(x)/x"] },
    { title: "Lists, vectors and matrices", text: "Use braces for a list; a matrix is a list of equally sized rows. Vectors use list notation. Matrix operations include Determinant, Inverse, Transpose and Rank.", examples: ["{1,2,3}", "{{1,2},{3,4}}", "Determinant({{1,2},{3,4}})"] },
    { title: "One-shot assumptions", text: "Add one top-level semicolon after the expression, followed by assumptions for this calculation. Separate multiple assumptions with commas. Supported forms are var>0, var<0, var!=0 and their reversed equivalents. Assumptions are cleared after the calculation.", examples: ["Sqrt(x^2);x>0", "x/y;x>0,y!=0"] },
    { title: "Partial input and composition", text: "Some operations can receive their arguments before the final expression, such as D(x) and Integrate(x). This is not automatic currying for every function: missing non-final arguments, invalid argument types or unregistered forms remain invalid or unresolved. Inner results can feed outer operations.", examples: ["D(x)", "D(x)Integrate(t,0,x)Sin(t^2)", "N(Determinant({{1,2},{3,4}}))"] },
    { title: "Possible outcomes", text: "Valid input does not guarantee a closed-form answer. A result may be a value, a partial application awaiting input, an unresolved object, or a mathematical conclusion such as no solution or divergence. These are different from input errors.", examples: [] },
  ],
};

export const getHelpSections = (locale) => content[locale] || content["zh-CN"];

export function renderHelp(container, locale) {
  const sections = getHelpSections(locale);
  container.replaceChildren(...sections.map((section, index) => {
    const details = document.createElement("details");
    details.className = "help-section";
    details.open = index === 0;
    const summary = document.createElement("summary");
    summary.textContent = section.title;
    const paragraph = document.createElement("p");
    paragraph.textContent = section.text;
    details.append(summary, paragraph);
    for (const example of section.examples) {
      const button = document.createElement("button");
      button.className = "help-example";
      button.type = "button";
      button.dataset.example = example;
      button.textContent = example;
      details.append(button);
    }
    return details;
  }));
}
