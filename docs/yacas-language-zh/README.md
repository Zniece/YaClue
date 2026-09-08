# Yacas 脚本语言文档

[English](../yacas-language/README.md) | **中文**

本目录描述 YaClue 使用的 Yacas 脚本语言、Rust 求值引擎和随项目维护的标准脚本库。Yacas 脚本语言面向符号计算，`.ys` 是其脚本文件扩展名。`yacas/yacas-rs` 提供正常构建使用的引擎。

## 文档层级

以下文档由本项目维护，并以当前实现和测试为准：

1. [语言规范](language-spec.md)：语法、数据模型、求值、规则和脚本装载。
2. [Rust 引擎](rust-engine.md)：实现结构、嵌入方式、资源限制和扩展边界。
3. [兼容性](compatibility.md)：稳定契约和有意差异的处理原则。
4. [快速开始](getting-started/index.md)：构建、测试和运行脚本。

详细资料包括[编程指南](../yacas-language/programming/index.md)、[函数参考](../yacas-language/reference/index.md)、[算法说明](../yacas-language/algorithms/index.md)、[教程](../yacas-language/tutorial/index.md)和[术语表](glossary.md)。这些资料源自 Yacas 文档，现由本项目随实现维护；测试与兼容性文档定义当前保证的功能。版权与历史贡献见[许可](../yacas-language/license.md)和[致谢](../yacas-language/credits.md)。

## 规范、实现与产品

| 层 | 位置 | 职责 |
|---|---|---|
| Yacas 脚本语言 | `yacas/yacas-rs/src/` | 解析、对象、求值、规则、数值核心与装载器 |
| 标准脚本库 | `yacas/scripts/` | 用 Yacas 实现的符号算法和用户函数 |
| 行为规范 | `yacas/tests/`、`yacas/yacas-rs/tests/` | 语言与标准库的可执行契约 |
| 教学步骤 | `processing/` | 调度、验证、数值算法和结构化步骤 |
| 桌面应用 | `app/` | 基于语言与 processing API 的产品交互 |

语言核心提供脚本运行所需的机制。`Simplify`、`Solve`、`Integrate` 等数学能力主要来自标准脚本库。

## 维护规则

- 语言语义以 Rust 实现和可执行测试为依据。
- 核心命令与脚本函数分开标注。
- 新增语法或改变既有语义时，同一提交更新规范和回归测试。
- 语言保证聚焦稳定行为；性能数字记录在开发状态中。
- 历史实现用于说明来源、许可并辅助兼容性调查。

## 翻译范围

中文站覆盖本项目直接维护的语言规范、Rust 引擎、兼容性、快速开始、术语表与可用性审计。教程、完整函数手册和算法资料通过中文导航连接到持续维护的[英文主文档](../yacas-language/README.md)。

## 许可

中文文档包含由上游 Yacas 文档改写和翻译的内容，整个中文文档目录按 GNU
自由文档许可证 1.1 版发布。完整条款与历史贡献者信息见英文站的
[许可](../yacas-language/license.md)和[致谢](../yacas-language/credits.md)。
