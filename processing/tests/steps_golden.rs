//! 步骤层 golden 快照测试(步骤生成的安全网)。
//!
//! 对一组覆盖全部规则键的题目,经 `derive_steps`/`derive_integrals`
//! (与 GUI 完全同一条代码路径)捕获完整步骤序列
//! (规则名 / 表达式 / LaTeX / 文案),与提交进仓库的基线
//! `tests/golden/steps_golden.txt` 逐行比对。任何步骤生成逻辑或
//! 文案的改动都会在这里产生可逐行审阅的 diff。
//!
//! 重新生成基线(改完步骤生成器 / 文案,人工确认 diff 符合预期后提交):
//!
//! ```text
//! UPDATE_GOLDEN=1 cargo test -p processing --test steps_golden
//! ```
//!
//! 走 RustEngine(进程内),不依赖 C++ 参考二进制,任何环境默认运行。
//! 注意:基线锁定的是"当前行为"(含已知局限,如求导前几步中的
//! DPrime 占位符),不是"理想行为"——它保障的是重构不改行为。

use std::collections::BTreeSet;

use processing::engine::{Engine, RustEngine};
use processing::steps::{derive_integrals, derive_steps, derive_steps_order};

const GOLDEN_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/steps_golden.txt");

/// 覆盖全部规则键的题目清单:(种类, 表达式, 变量)。
/// 注释标注每题的目标规则键;code.ys 新增规则时必须在这里补题
/// (下方的覆盖断言会强制这一点)。
const CASES: &[(&str, &str, &str)] = &[
    // ---- 求导 StepsD'Full ----
    ("D", "5", "x"),             // const-rule
    ("D", "x", "x"),             // identity-rule
    ("D", "3*x^2", "x"),         // constant-multiple-rule + power-rule
    ("D", "-Sin(x)", "x"),       // constant-multiple-rule(一元负号路径)
    ("D", "x^2 + Sin(x)", "x"),  // sum-rule
    ("D", "x*Cos(x)", "x"),      // product-rule
    ("D", "Sin(x)/x", "x"),      // quotient-rule
    ("D", "Sin(x)^2", "x"),      // power-rule(复合)+ sin-rule + simplify
    ("D", "Exp(x)", "x"),        // exp-rule
    ("D", "2^x", "x"),           // exponential-rule
    ("D", "Ln(x)", "x"),         // ln-rule
    ("D", "Sqrt(x)", "x"),       // sqrt-rule
    ("D", "x^x", "x"),           // direct(底指数均含变量,引擎直求)
    ("D", "Tan(x)", "x"),        // tan-rule
    ("D", "Cot(x)", "x"),        // 引擎规范化为 1/Tan 后走商法则(钉住规范化行为)
    ("D", "Sec(x)", "x"),        // 引擎规范化为 1/Cos 后走商法则
    ("D", "Csc(x)", "x"),        // 引擎规范化为 1/Sin 后走商法则
    ("D", "ArcSin(x)", "x"),     // arcsin-rule
    ("D", "ArcCos(x)", "x"),     // arccos-rule
    ("D", "ArcTan(x)", "x"),     // arctan-rule
    ("D", "Sinh(x)", "x"),       // sinh-rule
    ("D", "Cosh(x)", "x"),       // cosh-rule
    ("D", "Tanh(x)", "x"),       // tanh-rule
    ("D", "Tan(x^2)", "x"),      // tan-chain-rule + power-chain-rule(显式链式)
    ("D", "ArcTan(x^2)", "x"),   // arctan-chain-rule
    ("D", "Sinh(x^2)", "x"),     // sinh-chain-rule
    ("D", "Ln(Sin(x))", "x"),    // ln-chain-rule + sin-rule
    ("D", "Exp(Cos(x))", "x"),   // exp-chain-rule + cos-rule
    ("D", "2^(x^2)", "x"),       // exponential-chain-rule
    ("D2", "Sin(x)", "x"),       // 高阶:两轮步骤拼接(sin-rule → cos-rule 轮)
    ("D2", "x^4", "x"),          // 高阶:两轮 power-rule
    // ---- 积分 StepsI'Full ----
    ("I", "7", "x"),             // const-integral-rule
    ("I", "x", "x"),             // power-rule
    ("I", "Sqrt(x)", "x"),       // direct(积分侧无 Sqrt 规则,引擎直积)
    ("I", "x^2 + Cos(x)", "x"),  // sum-rule
    ("I", "3*Sin(x)", "x"),      // constant-multiple-rule
    ("I", "-Exp(x)", "x"),       // constant-multiple-rule(一元负号路径)
    ("I", "2/x", "x"),           // ln-rule
    ("I", "1/(1 + x^2)", "x"),   // arctan-rule
    ("I", "Sin(x)", "x"),        // sin-rule
    ("I", "Cos(x)", "x"),        // cos-rule
    ("I", "Exp(x)", "x"),        // exp-rule
    ("I", "Tan(x)", "x"),        // tan-rule(积分侧文案)
    ("I", "x*Sin(x)", "x"),      // parts-rule
    ("I", "x^2*Sin(x)", "x"),    // parts-rule(幂×三角)
    ("I", "Ln(x)", "x"),         // parts-rule(Ln 单独)
    ("I", "x*Ln(x)", "x"),       // parts-rule(rest×Ln)
    ("I", "Sin(x^2)*2*x", "x"),  // u-sub-rule + back-sub-rule
    ("I", "x*Exp(x^2)", "x"),    // u-sub-rule(rest*Exp)+ back-sub-rule
    ("I", "x/(x^2 + 1)", "x"),   // u-sub-rule + direct(换元无进展,引擎直积)+ back-sub-rule
    ("I", "Tan(x)", "x"),        // tan-rule(积分侧文案)
    ("I", "Cot(x)", "x"),        // cot-rule
    ("I", "Sinh(x)", "x"),       // sinh-rule
    ("I", "Cosh(x)", "x"),       // cosh-rule
    ("I", "Tanh(x)", "x"),       // tanh-rule
    ("I", "2/Sqrt(1 - x^2)", "x"), // arcsin-rule(反正弦形)
];

/// code.ys 登记的全部规则键(普通键,含顶层追加的 simplify)。
/// 覆盖断言保证题目清单把每个键都命中一次,规则不被静默遗漏。
/// 链式键("<name>-chain-rule")由 SD'FuncDiff 按普通键自动派生,
/// 不单列门禁,代表样本(Tan/ArcTan/Sinh/Ln/Exp/2^ 复合用例)已在
/// 题目清单中钉住。
const EXPECTED_RULES: &[&str] = &[
    "arctan-rule",
    "arccos-rule",
    "arcsin-rule",
    "back-sub-rule",
    "const-integral-rule",
    "const-rule",
    "constant-multiple-rule",
    "cos-rule",
    "cosh-rule",
    "direct",
    "exp-rule",
    "exponential-rule",
    "identity-rule",
    "ln-rule",
    "parts-rule",
    "power-rule",
    "product-rule",
    "quotient-rule",
    "sin-rule",
    "simplify",
    "sinh-rule",
    "sqrt-rule",
    "sum-rule",
    "tan-rule",
    "tanh-rule",
    "u-sub-rule",
];

/// 渲染一个用例:`### <种类> <表达式> @ <变量>` 头行 + 每步一行
/// `规则名 \t 表达式 \t LaTeX \t 文案`(tab 分隔,文案可为空串)。
fn render_case(
    engine: &mut dyn Engine,
    kind: &str,
    expr: &str,
    var: &str,
) -> Result<String, String> {
    let steps = match kind {
        "D" => derive_steps(engine, expr, var),
        "D2" => derive_steps_order(engine, expr, var, 2),
        _ => derive_integrals(engine, expr, var),
    }
    .map_err(|e| e.to_string())?;
    let mut out = format!("### {kind} {expr} @ {var}\n");
    for s in &steps {
        out.push_str(&format!("{}\t{}\t{}\t{}\n", s.rule, s.expr, s.tex, s.why));
    }
    Ok(out)
}

#[test]
fn golden_matches_baseline() {
    let mut engine = RustEngine::spawn().expect("启动 RustEngine 失败");

    let mut body = String::new();
    let mut seen = BTreeSet::new();
    for (kind, expr, var) in CASES {
        let text = render_case(&mut engine, kind, expr, var)
            .unwrap_or_else(|e| panic!("用例 {kind}: {expr} 生成步骤失败: {e}"));
        for line in text.lines().skip(1) {
            if let Some(rule) = line.split('\t').next() {
                if !rule.is_empty() {
                    seen.insert(rule.to_string());
                }
            }
        }
        body.push_str(&text);
    }

    // 覆盖门禁:每个已登记规则键至少被一题命中
    let missing: Vec<&str> = EXPECTED_RULES
        .iter()
        .filter(|r| !seen.contains(**r))
        .copied()
        .collect();
    assert!(
        missing.is_empty(),
        "以下规则键未被任何用例命中(步骤生成器改了规则集?请补题或同步 EXPECTED_RULES): {missing:?}"
    );

    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        if let Some(dir) = std::path::Path::new(GOLDEN_PATH).parent() {
            std::fs::create_dir_all(dir).expect("创建 golden 目录失败");
        }
        std::fs::write(GOLDEN_PATH, &body).expect("写入 golden 基线失败");
        eprintln!("golden 基线已更新: {GOLDEN_PATH} —— 请 git diff 逐行审阅后提交");
        return;
    }

    let baseline = std::fs::read_to_string(GOLDEN_PATH).unwrap_or_else(|e| {
        panic!(
            "读取 golden 基线失败({e})。首次生成: UPDATE_GOLDEN=1 cargo test -p processing --test steps_golden"
        )
    });
    let b: Vec<&str> = baseline.lines().collect();
    let a: Vec<&str> = body.lines().collect();
    if b == a {
        return;
    }
    // 找首个差异行,附上所在用例头行,便于定位
    let mut ctx = "(文件头)";
    for i in 0..b.len().max(a.len()) {
        match (b.get(i), a.get(i)) {
            (Some(x), Some(y)) if x == y => {
                if x.starts_with("### ") {
                    ctx = x;
                }
            }
            _ => panic!(
                "步骤输出与 golden 基线不一致,首个差异在「{ctx}」之后第 {} 行:\n  基线: {:?}\n  实际: {:?}\n确认改动符合预期后,用 UPDATE_GOLDEN=1 重新生成基线",
                i + 1,
                b.get(i).unwrap_or(&"<基线到此结束>"),
                a.get(i).unwrap_or(&"<实际输出到此结束>")
            ),
        }
    }
    panic!(
        "步骤输出与 golden 基线行数不一致: 基线 {} 行, 实际 {} 行",
        b.len(),
        a.len()
    );
}

/// 输入校验(引擎无关,走 RustEngine 默认即可运行):
/// 注入/残缺输入必须在进引擎前被拒绝。
#[test]
fn validation_rejects_bad_input() {
    let mut engine = RustEngine::spawn().expect("启动 RustEngine 失败");
    for bad in [
        "x); Echo(1); (x",
        "x;\nEcho(1)",
        "a:=99; Sin(x)^2",
        "Sin(x",
        "",
    ] {
        assert!(
            derive_steps(&mut engine, bad, "x").is_err(),
            "应拒绝非法输入: {bad:?}"
        );
    }
}
