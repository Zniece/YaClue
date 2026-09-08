# Yacas 脚本语言文档

[English](../yacas-language/README.md) | **中文**

本目录描述 YaClue 使用的 Yacas 脚本语言、Rust 求值引擎和随项目维护的标准脚本库。Yacas 脚本语言是面向符号计算的规则语言，`.ys` 文件是它的脚本文件。当前实现位于 `yacas/yacas-rs`，正常构建和运行不依赖其他引擎。

## 文档层级

以下文档由本项目维护，并以当前实现和测试为准：

1. [语言规范](language-spec.md)：语法、数据模型、求值、规则和脚本装载。
2. [Rust 引擎](rust-engine.md)：实现结构、嵌入方式、资源限制和扩展边界。
3. [兼容性](compatibility.md)：稳定契约和有意差异的处理原则。
4. [快速开始](getting-started/index.md)：构建、测试和运行脚本。

详细资料包括[编程指南](../yacas-language/programming/index.md)、[函数参考](../yacas-language/reference/index.md)、[算法说明](../yacas-language/algorithms/index.md)、[教程](../yacas-language/tutorial/index.md)和[术语表](glossary.md)。这些资料源自 Yacas 文档，现由本项目随实现维护。它们覆盖的历史功能多于当前产品界面；某个函数是否属于当前保证，以测试和兼容性文档为准。版权与历史贡献见[许可](../yacas-language/license.md)和[致谢](../yacas-language/credits.md)。

## 规范、实现与产品

| 层 | 位置 | 职责 |
|---|---|---|
| Yacas 脚本语言 | `yacas/yacas-rs/src/` | 解析、对象、求值、规则、数值核心与装载器 |
| 标准脚本库 | `yacas/scripts/` | 用 Yacas 实现的符号算法和用户函数 |
| 行为规范 | `yacas/tests/`、`yacas/yacas-rs/tests/` | 语言与标准库的可执行契约 |
| 教学步骤 | `processing/` | 调度、验证、数值算法和结构化步骤 |
| 桌面应用 | `app/` | 产品交互，不定义 Yacas 语义 |

语言核心只提供脚本运行所需的机制。`Simplify`、`Solve`、`Integrate` 等数学能力主要来自标准脚本库。

## 维护规则

- 语言语义以 Rust 实现和可执行测试为依据。
- 核心命令与脚本函数分开标注。
- 新增语法或改变既有语义时，同一提交更新规范和回归测试。
- 实现细节不写成语言保证；性能数字放在开发状态中。
- 历史实现只用于解释来源或许可，不作为当前行为的自动裁决者。

## 翻译范围

中文站覆盖本项目直接维护的语言规范、Rust 引擎、兼容性、快速开始、术语表与可用性审计。篇幅较大的历史教程、函数手册和算法资料由中文导航链接至[英文主文档](../yacas-language/README.md)，避免两份参考手册长期失去同步。

## 许可

中文文档包含由上游 Yacas 文档改写和翻译的内容，整个中文文档目录按 GNU
自由文档许可证 1.1 版发布。完整条款与历史贡献者信息见英文站的
[许可](../yacas-language/license.md)和[致谢](../yacas-language/credits.md)。
