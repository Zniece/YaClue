//! FromFile 输入流测试(照 cyacas mathcommands.cpp:1116;Macro|Fixed + bodied)。
//!
//! FromFile("name")body:按 input_directories 查文件(裸名 CWD 优先),打开失败
//! FileNotFound;压文件内容输入流,body 内 Read/ReadToken 从文件读并推进。
//! 期望全部来自 cyacas oracle:读首个表达式(不求值)、推进读 {aa,bb}、
//! 流尾 EndOfFile、文件不存在 FileNotFound(TrapError 可捕获)。

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
fn from_file_read() {
    std::fs::write("/tmp/ff_in.ys", "1+2;\n").unwrap();
    std::fs::write("/tmp/ff_in2.ys", "aa;bb;\n42;\n").unwrap();
    let mut e = Environment::new();
    run(
        &mut e,
        &format!(
            "DefaultDirectory(\"{}/\")",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts")
        ),
    );
    assert_eq!(run(&mut e, "Load(\"yacasinit.ys\")"), "True");
    let probes = [
        (r#"FromFile("/tmp/ff_in.ys")Read()"#, "1+2"),
        (
            r#"FromFile("/tmp/ff_in2.ys")[a:=Read(); b:=Read(); {a,b};]"#,
            "{aa,bb}",
        ),
        (r#"FromFile("/tmp/ff_in.ys")[Read(); Read();]"#, "EndOfFile"),
        (
            r#"TrapError(FromFile("/no/such/file.ys")Read(), "caught")"#,
            "\"caught\"",
        ),
    ];
    for (p, exp) in probes {
        assert_eq!(run(&mut e, p), exp, "probe {p}");
    }
}
