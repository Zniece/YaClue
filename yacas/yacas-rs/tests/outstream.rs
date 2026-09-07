use yacas_rs as ys;
use ys::env::Environment;
use ys::evaluator::eval;
use ys::parser::parse_expression;
use ys::printer::infix_print;
fn run(env: &mut Environment, src: &str) -> String {
    let t = parse_expression(env, &format!("{src};"))
        .unwrap_or_else(|e| panic!("parse {src}: {e:?}"))
        .expect("ok");
    match eval(env, &t) {
        Ok(r) => infix_print(env, &r),
        Err(e) => format!("ERR({e:?})"),
    }
}
#[test]
fn outstream_oracle() {
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
        // ToString 捕获:返回带引号串(cyacas 实证 "2ab" 等)
        (r#"ToString()[Write(1+1); WriteString("ab");]"#, "\"2ab\""),
        (r#"ToString()[Write(1);]"#, "\"1\""),
        (r#"ToString()[Write(1,2);]"#, "\"1 2\""),
        (r#"ToString()[Write("x");]"#, "\"\"x\"\""), // Write("x")→"x"(带引号)
        (r#"ToString()[Write(Atom("x"));]"#, "\"x\""),
        (r#"ToString()[WriteString("xy");]"#, "\"xy\""),
        (r#"ToString()[Write({1,2});]"#, "\"{1,2}\""),
        (
            r#"ToString()[Write(1);WriteString(" ");Write(2);]"#,
            "\"1 2\"",
        ),
        (r#"ToString()[Write(1);Write(2);]"#, "\"1 2\""),
        (
            r#"ToString()[WriteString("ab");WriteString("cd");]"#,
            "\"abcd\"",
        ),
        (r#"ToString()[Write(1);Write("x");]"#, "\"1\"x\"\""), // 数字接引号无空格
        (r#"ToString()[Write(1);Write(x);]"#, "\"1 x\""),      // 数字接符号有空格
        (r#"ToString()[Write(1);Write({1,2});]"#, "\"1{1,2}\""),
        (r#"ToString()[WriteString("a");Write("x");]"#, "\"a\"x\"\""),
        (r#"ToString()[Write("x");Write("y");]"#, "\"\"x\"\"y\"\""),
        (r#"ToString()[Write({1,2});Write({3});]"#, "\"{1,2}{3}\""),
        // ToString 捕获后原输出恢复(不影响后续独立求值)
        (r#"ToString()[WriteString("zz");]"#, "\"zz\""),
        // WriteString 参数须字符串(非串报错被 TrapError 捕获)
        (r#"TrapError(WriteString(5), "caught")"#, "\"caught\""),
        // 边界:空 body / body 最后值丢弃 / 嵌套 ToString(内层 Write 打内层缓冲,外层空)
        (r#"ToString()[]"#, "\"\""),
        (r#"ToString()[Write(1);1+1;]"#, "\"1\""),
        (r#"ToString()[ToString()[Write(1);];]"#, "\"\""),
        // 真实脚本端到端:lists.rep PrintList(ToString 捕获 + ForEach/WriteString/Write/递归)
        ("PrintList({aa,{bb,cc},dd})", "\"aa, {bb, cc}, dd\""),
        ("PrintList({})", "\"\""),
        ("PrintList({\"x\",1})", "\"x, 1\""),
        // FullForm:副作用 FullForm 打印+换行,返回实参求值结果(cyacas LispFullForm)
        (r#"ToString()[FullForm(a+b);]"#, "\"(+ a b )\n\""),
        (r#"ToString()[FullForm({1,2});]"#, "\"(List 1 2 )\n\""),
        (r#"r := FullForm(a+b); r"#, "a+b"),
        (r#"r2 := FullForm(2+3); r2"#, "5"),
    ];
    for (p, exp) in probes {
        let got = run(&mut e, p);
        assert_eq!(got, exp, "probe {p}");
    }
}
