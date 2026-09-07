//! T5 全链装载 + golden 复查(boot_init):DefaultDirectory(scripts)+Load("yacasinit.ys")
//! 完整启动链(照 cyacas yacasmain.cpp LoadYacas:DefaultDirectory 注入 + Load("yacasinit.ys")),
//! 然后对拍 tests/golden_t5.txt 的 19 条探针(cyacas -pc 实测,golden.rs 生成)。
//! 失败面即 T5 的 P1 缺口清单(G9/G10 等),按暴露逐项补齐(照 cyacas 语义)。

use yacas_rs::env::Environment;
use yacas_rs::evaluator::eval;
use yacas_rs::parser::parse_expression;
use yacas_rs::printer::infix_print;

fn run(env: &mut Environment, src: &str) -> String {
    let tree = parse_expression(env, &format!("{src};"))
        .unwrap_or_else(|e| panic!("parse {src}: {e:?}"))
        .expect("非空");
    let result = eval(env, &tree).unwrap_or_else(|e| panic!("eval {src}: {e:?}"));
    infix_print(env, &result)
}

fn scripts_root() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts").to_string()
}

fn boot_init(env: &mut Environment) {
    // 照 cyacas LoadYacas:DefaultDirectory 注入 rootdir/scripts(DeclarePath 尾斜杠语义)
    let d = format!("{}/", scripts_root());
    run(env, &format!("DefaultDirectory(\"{d}\")"));
    // Load("yacasinit.ys") = 全链装载(照 cyacas 主程序 Load 进入)
    assert_eq!(
        run(env, "Load(\"yacasinit.ys\")"),
        "True",
        "yacasinit.ys 全链装载失败"
    );
}

#[test]
fn init_full_boot_state() {
    let mut env = Environment::new();
    boot_init(&mut env);
    // boot 状态抽查(全部 cyacas 实测 = golden_t5 前部)
    // 88:steps.rep 剥离至 processing 后,packages.ys 不再登记(2026-09-06)
    assert_eq!(run(&mut env, "Length(DefFileList())"), "88");
    assert_eq!(run(&mut env, "RuleBaseDefined(\"Nth\",2)"), "True");
    assert_eq!(run(&mut env, "Nth({a,b,c},2)"), "b");
    assert_eq!(run(&mut env, "if(True) 11 else 22"), "11");
}

#[test]
fn init_golden_recheck() {
    // 逐行对拍 golden_t5.txt(expr<TAB>expected)。**单 env 顺序**执行——cyacas golden
    // 是单会话顺序测量(D 的 False→True 是会话内状态);按行新 boot 会错位。
    let golden = include_str!("golden_t5.txt");
    let mut env = Environment::new();
    boot_init(&mut env);
    let mut failures: Vec<String> = Vec::new();
    let mut checked = 0;
    for line in golden.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (expr, expected) = line.split_once('\t').expect("golden row: expr<TAB>out");
        let actual = run(&mut env, expr);
        checked += 1;
        if actual != expected {
            failures.push(format!("{expr}\t期望 {expected} 实得 {actual}"));
        }
    }
    if !failures.is_empty() {
        panic!(
            "golden_t5 复查失败 {}/{}:\n{}",
            failures.len(),
            checked,
            failures.join("\n")
        );
    }
    assert_eq!(checked, 19, "应有 19 条探针对拍");
}

#[test]
fn init_lazy_chain_state() {
    let mut env = Environment::new();
    boot_init(&mut env);
    // R5 懒加载抽查:D 首调前未定义(False) → D(x)x^2 触发 deriv 懒装载 → 2*x →
    // 装载后 RuleBaseDefined(D,2)=True(照 golden_t5)。
    assert_eq!(run(&mut env, "RuleBaseDefined(\"D\",2)"), "False");
    assert_eq!(run(&mut env, "D(x)x^2"), "2*x");
    assert_eq!(run(&mut env, "RuleBaseDefined(\"D\",2)"), "True");
}
