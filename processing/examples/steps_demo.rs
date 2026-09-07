//! 分步求导演示:经 processing 的步骤 API 生成求导步骤,
//! 输出每步的规则名、表达式与 LaTeX,并验证最终步与引擎 D 一致。
//!
//! 引擎替换检查:同一组用例分别在 C++ 子进程引擎(ReplEngine)与
//! Rust 进程内引擎(RustEngine)上运行,逐步对拍步骤序列与 TeX。
//!
//! 运行:cargo run -p processing --example steps_demo
//! (C++ 引擎可用 YACAS_BIN 指定;RustEngine 用 YACAS_SCRIPTS 指定脚本库)

use processing::engine::{Engine, ReplEngine, RustEngine};
use processing::steps::derive_steps;

const CASES: [&str; 5] = [
    "Sin(x)^2",
    "x*Sin(x)",
    "(x^2+1)*Exp(x)",
    "Sin(x)/x",
    "Ln(x^2+1)",
];

/// 单用例快照:(表达式, 每步 (rule, expr, tex) 序列, 末步一致性)
type SuiteSnapshot = Vec<(String, Vec<(String, String, String)>, bool)>;

/// 跑一组用例,返回快照
fn run_suite(engine: &mut dyn Engine, label: &str) -> SuiteSnapshot {
    let mut out = Vec::new();
    for expr in CASES {
        let steps = derive_steps(engine, expr, "x")
            .unwrap_or_else(|e| panic!("[{label}] derive_steps({expr}) 失败: {e}"));
        let triples: Vec<_> = steps
            .iter()
            .map(|s| (s.rule.clone(), s.expr.clone(), s.tex.clone()))
            .collect();
        let check = engine
            .eval(&format!(
                "Simplify(StepsD({expr}, x)[Length(StepsD({expr}, x))][2] - D(x)({expr}))"
            ))
            .expect("验证求值失败");
        out.push((expr.to_string(), triples, check.expr.to_string() == "0"));
    }
    out
}

fn main() {
    println!("== 引擎替换检查:C++(子进程) vs Rust(进程内) ==");
    let mut cpp = ReplEngine::spawn().expect("启动 C++ yacas 引擎失败");
    let cpp_out = run_suite(&mut cpp, "C++");

    let mut rs = RustEngine::spawn().expect("初始化 Rust 引擎失败");
    let rs_out = run_suite(&mut rs, "Rust");

    let mut diffs = 0;
    for (c, r) in cpp_out.iter().zip(rs_out.iter()) {
        println!("────────────────────────────────────────────");
        println!(
            "StepsD({}, x):  C++ {} 步 / Rust {} 步",
            c.0,
            c.1.len(),
            r.1.len()
        );
        if c.1 != r.1 {
            diffs += 1;
            println!("  ✗ 步骤序列分歧:");
            for (i, (cs, rs_step)) in c.1.iter().zip(r.1.iter()).enumerate() {
                if cs != rs_step {
                    println!(
                        "    第 {} 步:\n      C++ : {:?}\n      Rust: {:?}",
                        i + 1,
                        cs,
                        rs_step
                    );
                }
            }
            if c.1.len() != r.1.len() {
                let extra = if c.1.len() < r.1.len() {
                    &r.1[c.1.len()..]
                } else {
                    &c.1[r.1.len()..]
                };
                println!("    长度差多余步: {extra:?}");
            }
        } else {
            println!("  ✓ {} 步全部一致(rule/expr/tex 三元组)", c.1.len());
        }
        println!("  末步一致性验证: C++ {} / Rust {}", c.2, r.2);
        if c.2 != r.2 {
            diffs += 1;
        }
    }
    println!("────────────────────────────────────────────");
    println!(
        "{}",
        if diffs == 0 {
            "双引擎完全一致,可替换 ✓".to_string()
        } else {
            format!("{diffs} 处分歧,替换前需修 ✗")
        }
    );
}
