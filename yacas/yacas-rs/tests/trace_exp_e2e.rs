//! TraceExp 脚本级端到端(debug.rep) + Echo 多参(listed 超参打包修复验收)。
//!
//! Echo bug 根因:listed_prepare 超参打包用 copy_node(单节点,丢 next 链),
//! Echo("a","b","c") 打成 {"a"} 只打第一参。修复:rest 保留整条超参链。
//! 验收:cyacas oracle 逐字节 —— Echo 多参全打+换行;TraceExp(1+1) 输出
//! "  Enter 1+1 \n    Enter 1 \n...Leave 2 \n"(每子表达式 Enter/Leave)。

use yacas_rs as ys;
use ys::env::Environment;
use ys::evaluator::eval;
use ys::parser::parse_expression;
use ys::printer::infix_print;

fn boot() -> Environment {
    let mut e = Environment::new();
    let scripts = concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts");
    let t = parse_expression(&mut e, &format!("DefaultDirectory(\"{scripts}/\");"))
        .unwrap().expect("ok");
    eval(&mut e, &t).unwrap();
    let t = parse_expression(&mut e, "Load(\"yacasinit.ys\");").unwrap().expect("ok");
    assert_eq!(eval(&mut e, &t).map(|r| infix_print(&e, &r)).unwrap(), "True");
    e
}

fn run1(e: &mut Environment, src: &str) -> String {
    let t = parse_expression(e, &format!("{src};"))
        .unwrap_or_else(|er| panic!("parse {src}: {er:?}")).expect("ok");
    match eval(e, &t) {
        Ok(r) => infix_print(e, &r),
        Err(er) => format!("ERR({er:?})"),
    }
}

#[test]
fn echo_multiarg_listed_fix() {
    let mut e = boot();
    // cyacas oracle: Echo 多参全打 + 换行(ToString 捕获含 \n)
    assert_eq!(run1(&mut e, r#"ToString()[Echo("a","b","c");]"#), "\"abc\n\"");
    assert_eq!(run1(&mut e, r#"ToString()[Echo("single");]"#), "\"single\n\"");
    assert_eq!(run1(&mut e, r#"ToString()[Echo({1,2});]"#), "\"1 2 \n\"");
    assert_eq!(run1(&mut e, r#"ToString()[Echo("x=",1+1);]"#), "\"x=2 \n\"");
}

#[test]
fn trace_exp_e2e_structure() {
    let mut e = boot();
    let got = run1(&mut e, r#"ToString()[TraceExp(1+1);]"#);
    // cyacas oracle(TraceExp(1+1)):Enter/Leave 逐子表达式,缩进随深度
    let expected = "\"  Enter 1+1 \n    Enter 1 \n    Leave 1 \n    Enter 1 \n    Leave 1 \n    Enter IsNumber(x) \n      Enter x \n      Leave 1 \n    Leave True \n    Enter IsNumber(y) \n      Enter y \n      Leave 1 \n    Leave True \n    Enter True \n    Leave True \n    Enter MathAdd(x,y) \n      Enter x \n      Leave 1 \n      Enter y \n      Leave 1 \n    Leave 2 \n  Leave 2 \n\"";
    assert_eq!(got, expected, "TraceExp 追踪输出应与 cyacas 逐字节一致");
}
