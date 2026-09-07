//! TmpFile 命令测试(照 cyacas mathcommands.cpp:777;Function|Fixed arity=0)。
//!
//! mkstemp 语义:创建唯一临时文件(create_new + 随机后缀),返回带引号路径串
//! (cyacas oracle:"/tmp/yacas-uuedFA");两次调用唯一;文件已创建。CheckSecure。
//! 用途闭环:ToFile 写 + FromFile 读回(plots 后端中间文件)。

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
fn tmp_file() {
    let mut e = Environment::new();
    run(
        &mut e,
        &format!(
            "DefaultDirectory(\"{}/\")",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts")
        ),
    );
    assert_eq!(run(&mut e, "Load(\"yacasinit.ys\")"), "True");
    let f1 = run(&mut e, "TmpFile()");
    assert!(f1.starts_with("\"/tmp/yacas-"), "路径格式 {f1}");
    assert!(f1.ends_with('"'), "带引号");
    let f2 = run(&mut e, "TmpFile()");
    assert_ne!(f1, f2, "两次应唯一");
    // 文件确实存在:路径去引号后 is_file
    let p = f1.trim_matches('"');
    assert!(std::path::Path::new(p).is_file(), "文件应已创建 {p}");
    // 写读闭环:ToFile 写文件,再 FromFile 读回(临时文件用途:先写后读)
    run(&mut e, &format!("ToFile({f1})WriteString(\"xyz;\");"));
    let r = run(&mut e, &format!("FromFile({f1})Read();"));
    assert_eq!(r, "xyz", "写入内容应读回(带分号结尾)");
    let r2 = run(&mut e, &format!("FromFile({f1})[Read(); Read();]"));
    assert_eq!(r2, "EndOfFile", "流尾 EndOfFile");
    let _ = std::fs::remove_file(p);
}
