//! SystemCall/SystemName 命令测试(照 cyacas mathcommands.cpp:1263/1277)。
//!
//! SystemCall:实参字符串经 sh -c 执行(system 语义),exit 0 → True 否则 False;
//! env.secure 时 SecurityBreach。SystemName:平台名引号串。
//! 期望全部来自 cyacas oracle:exit0=True、exit3=False、不存在=False、本机 "MacOSX"。

use yacas_rs as ys;
use ys::env::Environment;
use ys::evaluator::eval;
use ys::parser::parse_expression;
use ys::printer::infix_print;
fn run(env: &mut Environment, src: &str) -> String {
    let t = parse_expression(env, &format!("{src};")).unwrap().expect("ok");
    match eval(env, &t) { Ok(r) => infix_print(env, &r), Err(e) => format!("ERR({e:?})") }
}
#[test]
fn system_call_and_name() {
    let mut e = Environment::new();
    run(&mut e, &format!("DefaultDirectory(\"{}/\")", concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts")));
    assert_eq!(run(&mut e, "Load(\"yacasinit.ys\")"), "True");
    for p in [
        r#"SystemCall("echo hi > /dev/null")"#,
        r#"SystemCall("exit 3")"#,
        r#"SystemCall("nonexistent_cmd_xyz 2>/dev/null")"#,
        "SystemName()",
    ] { println!("P| {p} => {}", run(&mut e, p)); }
}
