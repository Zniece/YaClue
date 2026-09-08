# 快速开始

当前 Yacas 引擎由 Rust 实现，标准脚本随项目存放。构建和测试不需要 C++、CMake、Java 或单独安装的 Yacas。

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

桌面应用还需要 Node.js。Tauri CLI 安装在项目内，不要求全局安装。

```bash
cd app
npm install
npm run tauri dev
```

也可以直接执行 `cargo build -p app` 编译 Rust 应用成员。当前界面用于验证后端全链路，不是正式 GUI。普通数学输入框接受单个 Yacas 表达式子集；方程组由界面按行拆分。“Yacas 表达式”入口直接访问持久引擎会话，适合受信任的开发测试。

## 启动标准脚本

裸 `Environment::new()` 只注册语言核心。完整 CAS 设置 `yacas/scripts` 为脚本目录并装载：

```ys
DefaultDirectory("/absolute/path/to/repository/yacas/scripts/");
Load("yacasinit.ys");
```

processing 已实现这段流程，并在其上装载教学步骤脚本。嵌入应用应使用资源目录，不要依赖工作目录偶然指向源码树。

## 编写脚本

Yacas 文件使用 `.ys` 扩展名，每条表达式以分号结束：

```ys
Square(x) := x^2;
Square(5);
```

包通常放在 `name.rep/code.ys`，并用 `.def` 登记延迟装载的函数。细节见[语言规范](../language-spec.md)与[编程指南](../../yacas-language/programming/index.md)。

仓库中的 `oracle` 仅用于特定兼容性调查，不参与正常构建，也不决定有意改进是否应回退。
