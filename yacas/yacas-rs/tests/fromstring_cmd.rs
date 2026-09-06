//! FromString/Read/ReadToken 输入流测试(照 cyacas mathcommands.cpp:1150/1173/1187)。
//!
//! FromString("str")body:压字符串输入流,body 内 Read/ReadToken 从串读并推进。
//! Read:解析一个表达式(**不求值**,cyacas oracle:FromString("x;")Read()→x);
//! 到流尾返回 EndOfFile。ReadToken:读一个原样 token,流尾 EndOfFile。
//! 期望全部来自 cyacas oracle:推进读 {1,2,3}、Eval(Read()) 求值、token 推进。

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
fn from_string_read_readtoken() {
    let mut e = Environment::new();
    run(&mut e, &format!("DefaultDirectory(\"{}/\")", concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts")));
    assert_eq!(run(&mut e, "Load(\"yacasinit.ys\")"), "True");
    // Read 只解析不求值(cyacas oracle:x→x,x+1→x+1,Eval(Read())→10)
    let probes = [
        (r#"FromString("x;")Read()"#, "x"),
        (r#"FromString("x+1;")Read()"#, "x+1"),
        (r#"FromString("42;")Read()"#, "42"),
        (r#"x := 10; FromString("x;")Eval(Read())"#, "10"), // 跨语句用 Prog
    ];
    // 上面的 x:=10 需独立(parse 单表达式),改用 Prog
    let probes2 = [
        (r#"[x := 10; FromString("x;")Eval(Read());]"#, "10"),
        (r#"FromString("1;2;3;")[a:=Read(); b:=Read(); c:=Read(); {a,b,c};]"#, "{1,2,3}"),
        (r#"FromString("hello world")ReadToken()"#, "hello"),
        (r#"FromString("hello world")[ReadToken();ReadToken();]"#, "world"),
    ];
    for (p, exp) in probes { let g = run(&mut e, p); println!("P| {p} => {g}"); assert_eq!(g, exp, "probe {p}"); }
    for (p, exp) in probes2 { let g = run(&mut e, p); println!("P| {p} => {g}"); assert_eq!(g, exp, "probe {p}"); }
}
