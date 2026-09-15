# YaClue

[English](README.md)

[![CI](https://github.com/Zniece/YaClue/actions/workflows/ci.yml/badge.svg)](https://github.com/Zniece/YaClue/actions/workflows/ci.yml)

**Yet another clue** —— 一个开源、本地优先的数学应用。输入一道问题，即可查看其结构化结果，以及得到结果所采用的数学变换与分析。

YaClue 的底层是用 Rust 编写的通用计算机代数引擎 **yacas-rs**。它是 [Yacas](https://github.com/grzegorzmazur/yacas) 1.9.x 的方言分支：脚本库由本项目维护，引擎本身则是全新的实现。YaClue 的类型化语义层把引擎表达式转化为可组合的数学对象，并明确区分等价变换、辅助分析、终止结论和 UI 效果。

可从 [YaClue 0.1.0-alpha.4](https://github.com/Zniece/YaClue/releases/tag/v0.1.0-alpha.4) 下载当前预发布版。桌面界面支持英语和简体中文：首次使用系统语言，也可在应用内切换。

## 数学输入

YaClue 接受紧凑的数学表达式，而不是通用脚本语言。运算可以直接组合，内层运算的结果会以类型化数学对象的形式传给外层运算。

输入框一次接受一个数学表达式。定义、赋值、脚本语句和语句结束符不属于这里的输入；编写或扩展 CAS 脚本时，请使用 Yacas 脚本语言和 `.ys` 文件。

```text
D(x)Sin(x)^2
Integrate(x,0,Pi)Sin(x)
Limit(Sin(x)/x,0)
Solve(x^2-5*x+6==0,x)
OdeSolve(y'==y+2*x)
Plot(Sin(x),x,-Pi,Pi)
```

`==` 只构造方程，不会隐式求解。代数方程及方程组使用 `Solve`，常微分方程使用 `OdeSolve`。函数式运算可以接收表达式，也可以接收其他运算产生的兼容结果。符号运算暂时无法求解时，系统会尽可能保留结构化数学对象，而不是把结果压平成展示字符串。

## 目录结构

```text
yacas/                CAS（yacas-rs），目录布局与上游相近
├── yacas-rs/         Rust 实现的 CAS 引擎（上游对应 C++ 的 cyacas/）
├── scripts/          标准脚本库
├── tests/            脚本库的 .yts 行为规范
├── COPYING/AUTHORS   上游许可证与作者信息（LGPL-2.1+）
processing/           类型化语义对象、运算注册表、组合、领域求解器、
                      结构化轨迹及产品输出投影
app/                  YaClue GUI 外壳（Tauri + KaTeX）
docs/                 Yacas 脚本语言的中英文文档
```

内核（`yacas/`）可以独立供其他应用使用；YaClue 是它的第一个应用。

## 架构

输入会被解析并精化为一棵类型化对象树。运算通过共享注册表由内向外执行；结果流入后续运算时，AST 身份和语义状态都会得到保留。yacas-rs 负责符号计算；`processing` 负责数学类型、能力、部分应用、规范化、组合和结构化解释。

产品输出把四类概念分开，而不是把每个事件都表现成等式步骤：

- 完整表达式的等价变换；
- 用于选择或验证方法的数学分析；
- 无值或未求解对象等终止结论；
- 附加在普通数学结果上的绘图等效果。

结构化传输格式与语言无关。结果标题使用稳定的 `title_key`；步骤、分析、结论和面向用户的错误使用 `message_ref` 对象，其中包含稳定的 `key` 和具名 `args`。可读文本由前端的语言目录选择，而不是由数学后端序列化。

## 符号安全边界

嵌入动态生成的 Yacas 作用域时，用户表达式绝不能与外围命令共享固定局部名称。处理代码在生成这类作用域前，必须对所有用户可控片段使用 `fresh_internal_symbols`；`LocalSymbols` 不能替代它，因为后者可能同时重命名插入的用户树和命令模板。AST 替换与 alpha 重命名由 `processing::binding` 负责，ODE 展示名称只在计算完成后分配。新的结构化入口必须加入回归测试，覆盖用户参数与每一类旧临时名称相同的情况，同时不得在结果路径中增加 CAS 往返。

## 许可证

| 路径 | 许可证 |
|---|---|
| `yacas/scripts/`、`yacas/tests/` | LGPL-2.1+（继承上游；见 `yacas/COPYING`） |
| `yacas/yacas-rs/` | MIT |
| `processing/` | MIT |
| `app/` | MIT |
| `docs/yacas-language/`、`docs/yacas-language-zh/` | GFDL-1.1（改编的 Yacas 文档、项目新增内容及翻译） |

设计中坚持以下边界：

1. MIT 许可的 crate 不嵌入 LGPL 脚本内容；脚本始终是运行时加载的外部文件，自然满足 LGPL §6 的可替换性；
2. 脚本库的修改继续采用 LGPL；
3. 引擎与脚本库共同发布，并使用同一版本；
4. 如果将 `.ys` 规则改写进 Rust 引擎，该代码将成为 LGPL 衍生作品，移动脚本逻辑前必须审慎判断。

## yacas-rs 与上游

- yacas-rs 的上游是采用 LGPL-2.1+ 的 [Yacas](https://github.com/grzegorzmazur/yacas)：脚本库继承该项目，Yacas 的 C++ 引擎不在本仓库内；
- yacas-rs 持续维护脚本库；
- Rust 引擎是独立实现，其受支持行为由 `yacas/yacas-rs/tests/` 中包含 100 多项测试及 golden 文件的一致性套件覆盖。

## 构建与运行

产品路径使用 Rust 实现。

### 桌面端

```bash
# 运行桌面应用
cd app
npm install          # 将项目本地 CLI 安装到 app/node_modules
npm run tauri dev

# 不使用 Node CLI 编译桌面二进制文件
cd ..
cargo build -p app

# 常规开发检查
./scripts/test-gate.sh fast

# 引擎或步骤层的专项回归
./scripts/test-gate.sh domain engine
./scripts/test-gate.sh domain steps
```

### 测试用标准输入输出接口

仅供开发使用的 `yaclue-stdio` 二进制程序，与桌面应用共用统一输入分派。它每行读取一个请求，并每行写出一个 JSON 结果。普通文本行按启用步骤的表达式处理；JSON 行可设置 `expression`、`steps` 和 `verbosity`。

```bash
printf '%s\n' 'Limit(x,0)' | cargo run -q -p app --features test-cli --bin yaclue-stdio
printf '%s\n' '{"expression":"D(x)x^2","steps":false,"verbosity":"concise"}' \
  | cargo run -q -p app --features test-cli --bin yaclue-stdio
```

桌面前端还包含一个默认折叠的命令行测试组件，可交互地通过该协议发送多行输入。此接口用于仓库测试，并不是稳定的公开 API。

### Android

仓库包含 Android 外壳。构建它需要 JDK、Android SDK、Android NDK 27，以及对应设备的 Rust target。典型的 64 位 ARM 手机或 ARM 模拟器可使用：

```bash
rustup target add aarch64-linux-android

export ANDROID_HOME="$HOME/Library/Android/sdk" # 替换为你的 SDK 路径
export NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
export JAVA_HOME="/path/to/your/jdk"

cd app
npm ci
npm run tauri -- android build --debug --target aarch64 --apk --ci
```

APK 输出到 `app/src-tauri/gen/android/app/build/outputs/apk/universal/debug/`，可用 `adb install -r <apk>` 安装到已连接的设备或模拟器。针对运行中的设备或模拟器进行开发时，使用 `npm run tauri -- android dev --target aarch64`。

构建过程会把 Yacas 和 processing 脚本打包为 Android asset。首次启动时，YaClue 会将它们复制到应用私有存储，以便 CAS 像普通文件一样加载。卸载应用时，Android 会删除该存储。

Android 移植目前用于开发测试。预发布版提供使用临时评估密钥签名、可直接安装的优化版 arm64 Release APK。它不是应用商店软件包；商店 APK 或 AAB 需要持久签名密钥，仓库不包含该密钥。

贡献者检查和 CI／发布构建细节见 [CONTRIBUTING.md](CONTRIBUTING.md)。每次推送和拉取请求都会由 GitHub Actions 执行格式检查、Clippy、快速引擎与 processing 测试，以及前端 JavaScript 检查。预发布标签会在完整测试通过后，构建已捆绑依赖的 Linux、macOS、Windows 软件包和 Android arm64 Release APK。

引擎通过 `DefaultDirectory` + `Load("yacasinit.ys")` 序列启动脚本库（见 `processing/src/engine/rust.rs`）；步骤包（`processing/scripts/steps.rep`）在标准库之上显式加载。

## 状态

版本 `0.1.0-alpha.4` 是首个基于类型化语义核心的预发布版。后端和结构化产品契约已经足以作为后续开发基线；应用仍处于预发布阶段，数学覆盖面和呈现方式会继续演进。

当前产品路径包括算术与代数变换、极限、求导、符号与定积分、级数、方程与方程组、符号与数值 ODE、数值求值与求根、线性代数、多元微积分和绘图。受支持的结果通过语义对象管线组合；不受支持或带条件的结果会保持明确，而不会悄然退化成字符串。

工作区统一使用根目录的 `Cargo.lock`，Rust workspace 成员不维护独立 lockfile。独立构建的 Tauri 应用保留自己的 `app/src-tauri/Cargo.lock`。

底层 Yacas tokenizer 接受 Unicode 字母。面向产品的 `processing` API 目前有意把变量及参数标识符限制为 ASCII 字母、首字符之后的 ASCII 数字和撇号，直至所有领域脚本和 TeX 渲染都能一致支持 Unicode 标识符。
