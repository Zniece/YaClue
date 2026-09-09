# 函数可用性审计

本页把详细函数手册中的条目分为核心命令、标准脚本、包内助手和迁移条目。审计数字描述文档结构。

## 2026-09-09 结构审计

审计使用当前 `yacas-rs` 建立 `Environment`，装载 `yacasinit.ys`，然后将参考手册标题与核心命令注册表、启动规则库和 `.def` 延迟装载表对照。

| 项目 | 数量 |
|---|---:|
| 参考手册三级标题 | 525 |
| 可识别为函数或运算符的标题 | 509 |
| 名称可在完整启动环境中直接发现 | 505 |
| 随包装载后可用的名称 | 4 |

4 个包内名称会随所属包的装载而可用。

## 迁移条目

- `Factorize`：迁移到标准脚本的 `Factor` 或具体多项式接口。
- `ExtraInfo'Set`：迁移到当前表达式模型支持的数据结构。
- `MathSinh`、`MathCosh`、`MathTanh` 及三个双曲 `MathArc*` 名称：使用对应的标准脚本公共函数。
- `IsPromptShown`、`ReadCmdLineString`：控制台交互由 Rust 宿主提供。
- `GetTime`：使用 Rust benchmark 或宿主计时。

这些名称为基于早期接口编写的脚本提供迁移指引。

## 包装载后出现的名称

- 图运算符 `->` 随图包声明。
- `OrthoPoly`、`OrthoPolySum` 是正交多项式包的内部助手。
- `GetError` 通过 I/O 错误包的公共入口随包装载。

检查包内函数前，先调用该包公开登记的入口完成装载。

## 模块对照

“测试提及”统计函数名在现有 `yacas/tests/*.yts` 中的文本出现次数。参数与边界覆盖由专门的行为测试记录；该数字用于确定后续核验优先级。

| 页面 | 函数型标题 | 启动环境可发现 | `.yts` 测试提及 |
|---|---:|---:|---:|
| `arithmetic.md` | 30 | 30 | 28 |
| `calc.md` | 25 | 25 | 18 |
| `controlflow.md` | 15 | 15 | 11 |
| `elementary.md` | 11 | 11 | 11 |
| `functional.md` | 5 | 5 | 4 |
| `graphs.md` | 8 | 7 | 6 |
| `io.md` | 34 | 34 | 17 |
| `linear-algebra.md` | 44 | 44 | 36 |
| `lists.md` | 57 | 57 | 34 |
| `number-theory.md` | 34 | 34 | 18 |
| `ode.md` | 5 | 5 | 3 |
| `plot.md` | 2 | 2 | 2 |
| `predicates.md` | 29 | 29 | 13 |
| `probability-and-statistics.md` | 12 | 12 | 2 |
| `programming-language.md` | 35 | 35 | 10 |
| `numeric-programming.md` | 23 | 23 | 11 |
| `errors.md` | 12 | 11 | 8 |
| `core-functions.md` | 34 | 34 | 10 |
| `containers.md` | 17 | 17 | 9 |
| `testing.md` | 9 | 9 | 7 |
| `random.md` | 8 | 8 | 5 |
| `solvers.md` | 12 | 12 | 9 |
| `univariate-polynomials.md` | 14 | 12 | 9 |

函数较少的页面遵循同一可用性规则。行为证据用于将描述提升为稳定承诺。

## 证据层级

结构审计记录核心或脚本库登记的名称；`.yts`、Rust 测试和产品回归用例为参数语义与结果提供证据。文档采用以下层级：

1. **语言核心**：由 Rust 核心测试覆盖；
2. **标准脚本入口**：由 `.def` 暴露，并应有 `.yts` 行为测试；
3. **包内助手**：覆盖所属公开入口所需的行为；
4. **迁移条目**：为早期脚本提供迁移参考。

后续审计把测试覆盖和已知边界记录到对应页面，使支持声明始终关联可执行证据。
