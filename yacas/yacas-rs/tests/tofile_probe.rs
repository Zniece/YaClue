//! ToFile/ToStdout 对拍测试(照 cyacas LispToFile/LispToStdout;bodied Macro)。
//!
//! ToFile("name")body:body 的 Write/WriteString 累积后**截断写文件**(覆盖,照
//! LispLocalFile ios_base::out);返回 body 结果。ToStdout()body:穿透 ToString
//! 捕获(cyacas 实证 ToString()[ToStdout()WriteString("force");WriteString("inner")]
//! →"inner",force 出 stdout);库场景无 stdout → 丢弃等价。

use yacas_rs as ys;
use ys::env::Environment;
use ys::evaluator::eval;
use ys::parser::parse_expression;
use ys::printer::infix_print;
fn run(env: &mut Environment, src: &str) -> String {
    let t = parse_expression(env, &format!("{src};")).unwrap_or_else(|e| panic!("parse {src}: {e:?}")).expect("ok");
    match eval(env, &t) { Ok(r) => infix_print(env, &r), Err(e) => format!("ERR({e:?})") }
}
#[test]
fn tofile_probe() {
    let mut e = Environment::new();
    run(&mut e, &format!("DefaultDirectory(\"{}/\")", concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts")));
    assert_eq!(run(&mut e, "Load(\"yacasinit.ys\")"), "True");
    let tf = std::env::temp_dir().join("yacas_rust_tf.txt");
    let p = tf.display().to_string();
    let probes = [
        (format!("ToFile(\"{p}\")WriteString(\"file-abc\");"), "True"),
        ("ToString()[WriteString(\"inner\");]".to_string(), "\"inner\""),
        // 二次 ToFile 覆盖
        (format!("ToFile(\"{p}\")WriteString(\"overwrite\");"), "True"),
    ];
    for (src, exp) in &probes {
        let got = run(&mut e, src);
        assert_eq!(&got, exp, "probe {src}");
    }
    let content = std::fs::read_to_string(&p).expect("read tf");
    assert_eq!(content, "overwrite", "ToFile 应覆盖写(cyacas ios_base::out)");
    // ToStdout 穿透:ToString 捕获不到 ToStdout body
    let got = run(&mut e, "ToString()[ToStdout()WriteString(\"force\");WriteString(\"inner\");]");
    assert_eq!(got, "\"inner\"", "ToStdout 穿透捕获,cyacas 实证 r=[inner]");
    let _ = std::fs::remove_file(&p);
}
