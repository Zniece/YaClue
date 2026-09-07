# YaClue 状态同步（开发进度共享文件）

> 每完成一个里程碑/提交单元，更新本文件的「当前状态」与「待办清单」并随代码一起提交。
> 最后更新：2026-09-07（A2 已推送 d72c319）

## 项目一句话

YaClue：本地优先的逐步式数学应用（Symbolab 风格），基于 yacas-rs（Yacas 1.9.x 方言分支的 Rust CAS 引擎）。
三层架构：`yacas/`（引擎 + LGPL 脚本库）/ `processing/`（步骤层：.rs API + scripts/steps.rep）/ `app/`（Tauri + KaTeX GUI）。

## 当前状态

- 最新提交：`d72c319`（A2：u-sub 触发器廉价优先比例判定），已推送 `origin/main`
- 已推送未合并的里程碑共 11 个 commit（M0-golden → A2）
- 门禁全绿：yacas-rs 套件、processing golden（70+11 例 + 规则覆盖门禁）、steps_yts 全量（~400s）、workspace clippy
- 引擎缺陷专项第一轮完成（D3/D4/D7 已修；D1/D2/D5/D6 已立案）

## 已交付里程碑

| 里程碑 | 内容 | 提交 |
|---|---|---|
| M0-lite | golden 快照安全网（70 例 + EXPECTED_RULES 覆盖门禁）、步骤文案表 | cfe2a31 |
| M1 | 链式法则显式步骤、函数覆盖（tan/反三角/双曲）、高阶导数 | cfe2a31 |
| M2a | 部分分式（PF 作为次末手段，避免 Apart 税）+ 规范模块化（steps.yts → 8 模块） | 538c2fe |
| M2b | 换元族补全（Tan/Sinh/Cosh/Tanh）+ 方法横幅（SD'Merge "m" 类型） | c86dcbe |
| M2c | θ 机器（三角换元完整管线：计算-提交-门禁）+ 加固 | c86dcbe |
| 递推表 | Sinⁿ/Cosⁿ 幂约简递推（吸收 manualintegrate 思想，θ 链黑盒 direct 消除） | a92e755 |
| M2d | 定积分（不定积分链 + 牛顿-莱布尼茨求值；无解析原函数留 Hold） | aabcd2d |
| 缺陷专项 1 | D3 IsFreeOf 函数头 / D4 奇次幂规则 / D7 求值超时（30s） | c222e50 |
| D6 诊断 | 规则尝试统计探针（YACAS_RULE_STATS=1，默认关） | 7580a20 |
| A2 | SI'PropCk 廉价优先比例判定（27 触发器；x/x 结构可消零成本快路径） | d72c319 |

## 待办清单

### A. θ 机器 / 步骤层补全

- [ ] **A1 tan/sec 型 θ 域积分表**：sec³、tanⁿ 递推条目上游没有需自建（sympy 的 `Iₙ = Secⁿ⁻²Tan/((n−1)a) + ((n−2)/(n−1))Iₙ₋₂` 思路可复用幂约简骨架 a92e755）
- [x] ~~A2 decline 预判~~（SI'PropCk 已做；体内拒绝方案因遮蔽 sqrt-sum/diff 族被 golden 门禁拦截，教训见 d72c319 commit message）

### B. 引擎缺陷（已立案，均为独立里程碑）

- [ ] **B1/D6 求值快路径**（架构项）：按参数形态索引规则表。基准数据：单次 θ 案例 165k 规则尝试/54k 命中、谓词仅占 21%（`YACAS_RULE_STATS=1` 可复测）；谓词 memo 原型已试并撤（5-10% 不值）
- [ ] **B2/D5 Simplify 正确性**：非规范存储树输入下把代数零改写成非零（复现：`Simplify(Deriv(x) F4 - Sin(x)^2)`，F4 为 Subst 构建树）；产品路径有 Tidy 探针/yts 断言保护，引擎级必修
- [ ] **B3/D1+D2 有理数表示**（数值层里程碑）：fork 用未求值 `/` 树表示分数（上游 BigNumber 原生 QQ）；`IsNumber(1/2)=False`、`N()` 传参失效同根；挂账测试在 `yacas/yacas-rs/tests/engine_defects.rs`（#[ignore]）

### C. M3 定积分深化

- [ ] **C1 GUI 定积分入口**：Rust API `derive_definite(expr, var, from, to)` 已备好（aabcd2d），前端 `app/main.js` 未接
- [ ] C2 数值方法兜底：Hold 分支接入数值求积（Simpson/自适应）
- [ ] C3 奇偶对称性捷径：∫(-a,a) 奇函数 = 0 的教学步骤（依赖 IsOddFunction 在完整链下可靠）
- [ ] C4 定限积分 TeX 头行/终式打磨

### D. 产品化

- [ ] **D1' GUI 高阶导数入口**：`derive_steps_order()` API 已有，前端未接
- [ ] D2' 绘图渲染组件（plot.rs 已有 RefineCfg 基础）
- [ ] D3' 会话管理（多输入历史、引擎状态保持）
- [ ] D4' 超时错误 GUI 呈现（30s UserInterrupt 现以 Eval 错误文本冒出，应区分提示并支持重试）

### E. 工程化

- [ ] E1 yacas-rs 发 crates.io
- [ ] E2 发布验收清单文档化（UPDATE_GOLDEN / steps_yts 门禁操作说明）

## 下一步建议

**主线 A1**（tan/sec θ 域积分表，步骤层 scripts/steps.rep）与**支线 C1+D1'**（GUI 入口，app/ + processing API）相互独立，可并行推进；B 类里程碑穿插在功能间隙。

## 协作约定（工程操作）

- 门禁命令（提交前必须全绿）：
  - `cargo test`（yacas-rs 全套件）
  - `UPDATE_GOLDEN=1 cargo test -p processing --test steps_golden`（重生成基线后 git diff 逐行审阅）
  - `cargo test -p processing --test steps_yts -- --ignored`（全量规范门禁，约 7 分钟）
  - `cargo clippy --workspace --all-targets`
- 脚本 A/B 对比：`YACAS_STEPS_SCRIPTS=/path/to/dir cargo test …`（覆盖步骤脚本目录）
- 步骤脚本位置：`processing/scripts/steps.rep/code.ys`；规则按同优先级「后定义先尝试」插入块首
- `IsFreeOf(var, expr)` 参数序：**变量在前、表达式在后**
- 换元哑元名不得与 SD'Merge 局部变量（t、u 等）冲突（θ 机器用 `theta`）
- 提交纪律：半理解的代码不进库；golden 基线改动需逐行审阅 diff
