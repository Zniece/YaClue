# 术语表

## Arity（参数个数）

函数由名称和参数个数共同标识，因此 `f(x)` 与 `f(x,y)` 可以拥有不同规则库。Listed 规则允许把超出的实参收集为列表，用于可变参数函数。

## Array（数组）

由 Rust 引擎管理的定长容器，通过 `Array'Create`、`Array'Get`、`Array'Set` 和 `Array'Size` 操作。数组元素可变，但长度固定。它在历史接口中被归入 generic object；当前实现不加载任意原生对象插件。

## Atom（原子）

表达式树中没有子节点的值，包括符号、字符串和数值。符号区分大小写；未绑定符号求值为自身。参见 [Atom](../yacas-language/reference/strings.md#atomstring) 和 [String](../yacas-language/reference/strings.md#stringatom)。

<a id="bodied-function"></a>

## Bodied function（带函数体语法）

最后一个参数写在括号外的运算符形式。例如：

```ys
D(x) Sin(x);
```

`Bodied` 声明这种解析方式及其优先级。它只改变表面语法，求值时仍是普通调用树。

## CAS

Computer Algebra System，即计算机代数系统。Yacas 是面向 CAS 的规则语言；`yacas-rs` 是其求值引擎，标准脚本库提供高层数学算法。

<a id="constant"></a>

<a id="constant"></a>
## Constant（常量）

没有普通变量绑定、但被标准脚本赋予数学语义的符号，例如 `Pi`。符号形式可以参与化简，`N` 在需要时请求数值近似。

<a id="cached-constant"></a>

<a id="cached-constant"></a>
## Cached constant（缓存常量）

计算成本较高并按已求得精度缓存的常量。脚本应通过 [CachedConstant](../yacas-language/reference/numeric-programming.md#cachedconstantcache-cname-cfunc) 使用这一机制，不直接依赖缓存的内部变量名。

## Equation（方程）

`lhs == rhs` 构造符号方程。`==` 不赋值；方程可以传给 `Solve`、ODE 求解器和验证函数。`=` 在部分语言上下文中执行相等性判断，两者不能混用。

## Function（函数）

具有调用头和参数的表达式。函数可以由 Rust 核心命令实现，也可以由一个或多个 Yacas 规则实现。未知函数会保留调用头并求值参数，而不会产生“未声明函数”错误。

## List（列表）

可变长度序列，表面语法为 `{a,b,c}`，内部形式等价于 `List(a,b,c)`。调用树和列表都使用链式表达式节点，但函数调用的头部具有求值意义。参见 [List](../yacas-language/reference/lists.md#listexpr1-expr2)、[Listify](../yacas-language/reference/lists.md#listifyexpr) 和 [FullForm](../yacas-language/reference/io.md#fullformexpr)。

## Matrix（矩阵）

按行存储的列表的列表，例如 `{{a,b},{c,d}}`。矩阵是否合法、是否为方阵以及元素域要求由具体线性代数函数检查。

## Operator（运算符）

具有特殊表面语法的函数，可声明为中缀、前缀、后缀或 bodied。运算符表属于 `Environment`；语法声明影响此后解析的文本，不改变已生成的表达式树。

## Precedence（优先级）

控制运算符结合范围的非负整数。在 Yacas 中数值越小结合越紧。中缀运算符可以拥有不同的左右优先级和结合性。参见 `OpPrecedence`、`OpLeftPrecedence` 与 `OpRightPrecedence`。

## Property（属性）

旧文档曾用 `ExtraInfo'Set` 给表达式附加标签。当前 Rust 表达式模型没有公开这一入口，因此 property 只作为旧脚本迁移术语保留，不属于当前必须接口。

## Rule（规则）

把某种函数调用转换为另一表达式的求值单元。规则由函数名、参数个数、优先级、模式或谓词以及规则体组成。没有适用规则时，调用保持未求值。参见 [RuleBase](../yacas-language/reference/programming-language.md#rulebasename-params)、[Rule](../yacas-language/reference/programming-language.md#bodied-rulebody-operator-arity-precedence-predicate) 和 [Retract](../yacas-language/reference/programming-language.md#retractfunction-arity)。

## String（字符串）

双引号包围的文本原子。字符串与符号是不同类型；`String` 和 `Atom` 提供受控转换。字符串索引与修改由 `StringMid'Get`、`StringMid'Set` 等函数完成。

## Syntax（语法）

Yacas 使用函数调用和可配置运算符组成的表面语法。解析器结合当前环境中的运算符表，将文本转换为统一的调用树。完整规则见[语言规范](language-spec.md)。

## Threaded function（逐元素函数）

标准脚本可以为函数定义列表规则，使一次调用逐元素作用于列表，例如 `Cos({Pi/2,Pi/4})`。这不是求值器对所有函数自动实施的广播语义；是否 threaded 由具体脚本规则决定。

## Variable（变量）

绑定到值的符号。求值时先查局部绑定，再查全局绑定。普通绑定读出后不会无条件再次求值；惰性全局绑定在首次读取时求值并缓存。参见 [Eval](../yacas-language/reference/controlflow.md#evalexpr)、`:=` 和 [Clear](../yacas-language/reference/vars.md#clearvar)。

## Warranty（担保）

软件按现状提供，不附带担保。Rust 引擎与产品代码采用 MIT 许可证，具有继承关系的标准脚本与测试采用 LGPL-2.1-or-later，历史文档许可见[许可说明](../yacas-language/license.md)。
