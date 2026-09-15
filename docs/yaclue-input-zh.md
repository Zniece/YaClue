# YaClue 数学输入

本页说明 YaClue 输入框中的**数学输入表达式**。它不是用于编写 `.ys` 文件的 [Yacas 脚本语言](yacas-language-zh/README.md)。

## 一次提交一个表达式

输入必须是一个非空表达式。以下内容不属于 YaClue 输入：

- 语句结束符 `;`、换行和多语句程序；
- 定义或赋值，例如 `f(x):=x^2`、`x:=1`；
- 字符串和以 `:` 引入的脚本构造。

这些构造应写入 Yacas `.ys` 脚本。不要把计算器输入框当作脚本控制台。

## 基础记法

数字、符号、函数调用、括号、列表、矩阵和通常的数学运算可用于表达式。例如：

```text
2*x^2-3*x+1
Sin(x)^2+Cos(x)^2
f(x+1)
{1,2,3}
{{1,2},{3,4}}
x^2-5*x+6==0
```

`==` 构造方程，不会自动求解；使用 `Solve` 才会求解。列表用 `{...}`，矩阵是等长行的列表。函数名和符号区分大小写；例如 `Sin`、`Pi` 是惯用的标准拼写。

表达式可嵌套。YaClue 从内到外计算，并将内层的结构化结果传给外层运算：

```text
D(x)Integrate(t,0,x)Sin(t^2)
Integrate(x)Taylor(Exp(x),0,2)
N(Determinant({{1,2},{3,4}}))
```

## 运算符形式与参数约定

下表中 `expr` 表示一个数学表达式，`var` 表示符号变量，`a`/`b` 表示边界或点，`n` 表示非负整数阶数，`dir` 表示方向，`p` 表示数值精度。只有表内列出的参数个数是该产品运算的完整形式。

### 常用运算

| 类别 | 形式 | 说明 |
|---|---|---|
| 导数 | `D(var)expr`；`D(var,n)expr` | `Deriv` 是 `D` 的别名；`var` 在 `expr` 中绑定。 |
| 不定/定积分 | `Integrate(var)expr`；`Integrate(var,a,b)expr` | `var` 在 `expr` 中绑定。 |
| 极限 | `Limit(expr,a)`；`Limit(var,a)expr`；`Limit(var,a,dir)expr` | 第一种是值在前的常规形式；后两种中 `var` 绑定 `expr`。 |
| 泰勒展开 | `Taylor(expr,a,n)`；`Taylor(var,a,n)expr` | 第一种是常规形式；后一种中 `var` 绑定 `expr`。 |
| 代换 | `Subst(var,replacement)expr` | 将 `expr` 中的 `var` 替换为 `replacement`。 |
| 方程 | `Solve(equation,var)`；`Solve({equation,...},{var,...})` | 变量参数给出待解未知量。 |
| 代数变换 | `Factor(expr)`、`Expand(expr)`、`Simplify(expr)`、`Tidy(expr)`、`Apart(expr,var)` | 变换的输出仍可嵌套为其他运算的输入。 |
| 数值近似 | `N(expr)`；`N(expr,p)`；`Approximate(expr[,p])` | `N` 与 `Approximate` 是别名。 |
| 求和 | `Sum(var,a,b)expr` | `var` 在 `expr` 中绑定。 |
| 常微分方程 | `OdeSolve(equation)` | 例如 `OdeSolve(y'==y+2*x)`。 |
| 绘图 | `Plot(expr,var,a,b)` | 产生绘图 effect，并可附带正常数学结果。 |

### 线性代数与数值/多变量运算

| 类别 | 形式 |
|---|---|
| 矩阵变换 | `Transpose(matrix)`、`Determinant(matrix)`、`Inverse(matrix)` |
| 矩阵分析 | `Rank(matrix)`、`RREF(matrix)`、`RowReduce(matrix)`、`EigenValues(matrix)`、`NullSpace(matrix)`、`ColumnSpace(matrix)`、`EigenSpaces(matrix)` |
| 矩阵分解 | `PLDU(matrix)`、`Cholesky(matrix)`、`GramSchmidt(matrix)`、`OrthogonalBasis(matrix)`、`OrthonormalBasis(matrix)`、`Factors(matrix)` |
| 线性系统 | `MatrixSolve(matrix,vector)`；`SolveMatrix(matrix,vector)` |
| 广义/多重积分 | `ImproperIntegral(expr,var,a,b[,options])`、`PrincipalValueIntegral(expr,var,a,b[,options])`、`DoubleIntegral(expr,x,a,b,y,c,d)`、`PolarIntegral(expr,x,y,r,theta,r0,r1,t0,t1)` |
| 数值问题 | `FindRoot(expr,var,initial)`；`OdeSolveNumeric(expr,var,dependent,x0,y0,x1)` |
| 极值 | `Extrema(expr,var1,var2)`；`Lagrange(objective,constraint,var1,var2)` |
| 多变量微分 | `Gradient(expr,vars)`、`Jacobian(expr,vars)`、`Hessian(expr,vars)`、`Divergence(expr,vars)`、`Curl(expr,vars)`；这些接受 2 或 3 个参数。`DirectionalDerivative(expr,vars,dir[,assumption[,extra]])` 接受 3–5 个参数。 |
| 曲线/曲面积分 | `ScalarLineIntegral(...)`、`VectorLineIntegral(...)` 各 6 个参数；`ScalarSurfaceIntegral(...)`、`VectorSurfaceIntegral(...)` 各 6 或 7 个参数。 |

后几类的领域含义和可接受对象会随对应求解器能力而受限；输入语法合法不承诺一定获得封闭形式结果。

## 绑定和作用域

`D`、`Integrate`、带变量的 `Limit`/`Taylor`、`Sum` 等形式中的变量是**绑定变量**，只作用于其 operand。例如，在 `D(x)(x*y)` 中 `x` 是被微分的局部变量，`y` 是自由参数。嵌套时，内层绑定不改变外层同名符号的含义：

```text
D(x)Integrate(x,0,t)(x^2)
```

不要以显示文本推断变量身份；产品会保留绑定关系与 AST 一起参与组合。

## 部分输入与柯里化

YaClue 接受部分输入，但有明确边界，不能按一般编程语言的“所有函数都自动柯里化”理解。

1. **可直接补 operand 的部分应用**：对于 bodied 运算，已给参数合法且只缺最后 `expr` 时，会产生一个可组合的部分应用。例如 `D(x)`、`Integrate(x)` 是等待 operand 的对象；接上 `x^2` 后分别形成 `D(x)x^2`、`Integrate(x)x^2`。
2. **不完整但有歧义的调用**：若同一前缀对应多个完整签名，系统保留全部候选，而不猜测最短形式。例如 `Limit(t)` 可通向不同参数数目的 `Limit` 形式，结果会报告候选参数槽。
3. **不支持的缺口**：不满足已登记签名、不是最后 operand 的缺失，或参数类型不合格时，不会被提升为可执行的柯里化调用。
4. **逐槽校验**：补变量槽必须给符号；补阶数槽必须给非负整数。其余槽也按运算符登记的角色（边界、方向、精度等）保留给后续验证。

部分应用是结构化的 `Held`/未决数学对象，不是错误，也不是已经计算出的普通数值。

## 合法输入不等于必定求解

每个成功解析的输入都会先成为结构化数学对象。随后结果可能是：

- **Value**：得到可继续组合的值；
- **Held / Unresolved**：输入合法，但仍缺参数、算法未覆盖，或不能安全完成；
- **NoValue**：已有数学结论，但没有可作为下一运算操作数的值，例如无解或发散；
- **EffectsOnly**：终端效果；绘图通常作为附加 effect 输出；
- **Error**：语法、资源或引擎故障。

带条件的变换会连同条件输出。不要把未决、无值、无解、发散和输入错误视作同一种“失败”。

## 与 Yacas 脚本的关系

YaClue 使用 yacas-rs 进行表达式解析和符号计算。要查阅或编写规则、赋值、程序块和 `.ys` 标准库，请阅读 [Yacas 脚本语言文档](yacas-language-zh/README.md)。
