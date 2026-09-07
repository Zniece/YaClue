//! Secure 命令测试(照 cyacas mathcommands.cpp:1519;Macro|Fixed)。
//!
//! Secure(body):body 求值期间 env.secure=true(RAII 恢复);体内调 CheckSecure
//! 命令(SystemCall/ToFile/Load/DefLoad/FromFile/TmpFile)→ SecurityBreach
//! (TrapError 可捕获);正常表达式照常求值;错误后 secure 复位。
//! 期望全部来自 cyacas oracle:Secure(SystemCall)→caught、Secure(1+1)→2、
//! Secure(42)→42、Secure 外 SystemCall 正常。

use yacas_rs as ys;
use ys::env::Environment;
use ys::evaluator::eval;
use ys::parser::parse_expression;
use ys::printer::infix_print;
fn run(env: &mut Environment, src: &str) -> String {
    let t = parse_expression(env, &format!("{src};"))
        .unwrap()
        .expect("ok");
    match eval(env, &t) {
        Ok(r) => infix_print(env, &r),
        Err(e) => format!("ERR({e:?})"),
    }
}
#[test]
fn secure_frame() {
    let mut e = Environment::new();
    run(
        &mut e,
        &format!(
            "DefaultDirectory(\"{}/\")",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts")
        ),
    );
    assert_eq!(run(&mut e, "Load(\"yacasinit.ys\")"), "True");
    // Secure 体内 CheckSecure 命令 → SecurityBreach(TrapError 捕获);体外正常
    let probes = [
        (
            r#"TrapError(Secure(SystemCall("echo hi")), "caught")"#,
            "\"caught\"",
        ),
        (r#"Secure(1+1)"#, "2"),
        (
            r#"TrapError(Secure(ToFile("/tmp/sec_t.txt")WriteString("x")), "caught")"#,
            "\"caught\"",
        ),
        (r#"SystemCall("echo ok > /dev/null")"#, "True"),
        (
            r#"TrapError(Secure([SystemCall("echo a");SystemCall("echo b");]), "caught")"#,
            "\"caught\"",
        ),
        (r#"TrapError(Secure(42), "nope")"#, "42"),
        // 嵌套恢复:Secure 错误后 secure 复位,体外 SystemCall 仍正常
        (
            r#"[TrapError(Secure(SystemCall("x")), "c"); SystemCall("echo ok > /dev/null");]"#,
            "True",
        ),
    ];
    for (p, exp) in probes {
        assert_eq!(run(&mut e, p), exp, "probe {p}");
    }
}
