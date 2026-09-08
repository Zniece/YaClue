# 快速开始

项目使用稳定版 Rust 与随仓库提供的标准脚本完成构建和测试。

## 构建与测试

进入产品仓库并使用稳定版 Rust：

```bash
cd prj
cargo build -p yacas-rs
cargo test -p yacas-rs
cargo test -p processing
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

`Cargo.lock` 是 workspace 的统一锁文件。

## 运行桌面体验台

桌面应用使用 Node.js，并从项目本地安装 Tauri CLI。

```bash
cd app
npm install
npm run tauri dev
```

也可以直接执行 `cargo build -p app` 编译 Rust 应用成员。当前界面用于正式 GUI 设计前的后端全链路验证。普通数学输入框接受单个 Yacas 表达式子集；方程组由界面按行拆分。“Yacas 表达式”入口直接访问持久引擎会话，适合受信任的开发测试。

## 启动标准脚本

裸 `Environment::new()` 只注册语言核心。完整 CAS 设置 `yacas/scripts` 为脚本目录并装载：

```ys
DefaultDirectory("/absolute/path/to/repository/yacas/scripts/");
Load("yacasinit.ys");
```

processing 已实现这段流程，并在其上装载教学步骤脚本。嵌入应用通过打包后的资源目录定位脚本。

## 编写脚本

Yacas 文件使用 `.ys` 扩展名，每条表达式以分号结束：

```ys
Square(x) := x^2;
Square(5);
```

包通常放在 `name.rep/code.ys`，并用 `.def` 登记延迟装载的函数。细节见[语言规范](../language-spec.md)与[编程指南](../../yacas-language/programming/index.md)。

仓库中的 `oracle` 为特定兼容性调查提供可选证据；现行契约与回归测试决定有意改进。
