# Rust 引擎与嵌入

`yacas-rs` 是 Yacas 的当前引擎。它负责解析、求值、规则匹配、基础数值运算和脚本装载；数学算法主要由外部标准脚本提供。

## 组成

| 模块 | 职责 |
|---|---|
| `tokenizer.rs`、`parser.rs` | 词法、表面语法和内部前缀语法 |
| `value.rs`、`symtab.rs` | 表达式节点、原子和符号驻留 |
| `env.rs` | 变量、规则、运算符、精度、装载状态和资源限制 |
| `evaluator.rs` | 核心命令、规则、延迟装载和未求值回退 |
| `userfunc.rs`、`pattern.rs` | 用户规则、模式匹配和优先级 |
| `commands/` | 可从 Yacas 调用的 Rust 核心命令 |
| `number/` | 任意精度整数和十进制数值层 |
| `loader.rs`、`standard.rs` | `.def` 索引、文件定位和公共运行机制 |

代码目录与唯一注册表共同构成核心接口的权威清单。

## 建立环境

`Environment::new()` 建立独立的核心环境，注册标准运算符和核心命令，创建布尔值，保护基础符号并建立局部帧。宿主随后配置标准脚本目录。

嵌入者通常创建环境、配置脚本目录、装载 `yacasinit.ys`、装载应用扩展，然后在同一环境中解析和求值。

```rust
use yacas_rs::{env::Environment, evaluator, parser};

let mut env = Environment::new();
let tree = parser::parse_expression(&mut env, "1+2;")?
    .expect("one expression");
let value = evaluator::eval(&mut env, &tree)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`+` 的完整符号语义来自标准脚本，真实应用应先完成脚本启动。processing 的引擎适配器展示了当前产品流程。

## 环境、解析与线程

变量、规则、假设、精度、装载状态和运算符表都属于 `Environment`，每个环境拥有独立的可变状态。表达式节点使用 `Rc`；跨线程应用把环境固定在工作线程，并通过消息传递串行请求。

`parse_expression` 解析一条以分号结束的表达式，`parse_one` 从输入流逐条读取脚本。解析使用当前运算符表。`evaluator::eval` 返回供宿主直接消费的表达式树。

## 核心扩展边界

新增 Rust 核心命令时应满足：

- 能力需要访问环境内部、底层数值、受控 I/O，或其可靠实现需要核心支持；
- 明确参数的求值、保持和作用域行为；
- 返回结构化 `YacasError`；
- 长循环检查截止时间；
- 在 `commands` 下按领域实现并由唯一注册表登记；
- 添加核心测试和依赖它的脚本行为测试。

符号恒等变换和可读规则通常写在 `.ys` 标准库中。Rust 适合基础机制、数值热点、资源边界和宿主接口；processing 适合产品调度、验证和教学事件。

## 资源与安全

`Environment::set_eval_timeout` 设置请求截止时间，`max_eval_depth` 限制递归。宿主应在请求结束后清理截止时间，并按会话策略回收临时唯一符号。

Yacas 包含文件、装载和系统调用能力。产品向普通用户提供经过解析验证的单表达式入口，并将完整脚本执行保留给受信任脚本包或明确的开发接口。

## Rust API 稳定性

当前 crate 暴露供仓库内部使用的底层模块。后续稳定的 CAS facade 将封装会话、求值、批量数值和错误接口；`Rc<LispObject>`、Environment 字段和线程协议属于实现细节。
