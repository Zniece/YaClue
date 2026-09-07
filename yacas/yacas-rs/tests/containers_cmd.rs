//! 容器命令对拍测试(照 cyacas GenArray*/GenAssociation* + InfixPrinter 特型打印)。
//!
//! 覆盖:Array'Create(size,fill) 签名/逐槽拷贝/空数组/越界错误捕获;
//! Association 的键序(InternalStrictTotalOrder:数字<字符串<列表,等前缀短在前)、
//! Head/Drop/Get miss;Association'CreateFromList 端到端(assoc.rep 真实脚本路径);
//! Generic 对象 InfixPrinter 特型(Array({...})/Association({...}),照 infixparser.cpp:379)。
//! 期望全部来自 cyacas oracle(-pc 逐条 + TrapError 实证)。

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
fn container_family_oracle() {
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
        // Array:size/fill 拷贝、空数组、子列表 fill、越界被 TrapError 捕获
        ("a := Array'Create(3, 7)", "Array({7,7,7})"),
        ("Array'Size(a)", "3"),
        ("Array'Get(a,1)", "7"),
        ("Array'Set(a,2,9)", "True"),
        ("Array'Get(a,2)", "9"),
        ("Array'Get(a,1)", "7"),
        ("b := Array'Create(0,0)", "Array({})"),
        ("Array'Size(b)", "0"),
        (r#"Array'Set(a,1,{1,2})"#, "True"),
        ("Array'Get(a,1)", "{1,2}"),
        (
            r#"TrapError(Array'Get(Array'Create(1,0),5), "caught")"#,
            "\"caught\"",
        ),
        (
            r#"TrapError(Array'Set(Array'Create(1,0),2,9), "caught")"#,
            "\"caught\"",
        ),
        (r#"TrapError(Array'Create(2,3,4), "caught")"#, "\"caught\""),
        // Association:排序(字符串/数字/列表键)、Head、Drop、错误路径
        ("c := Association'Create()", "Association({})"),
        ("Association'Size(c)", "0"),
        (r#"Association'Set(c,"x",10)"#, "True"),
        (r#"Association'Set(c,"y",20)"#, "True"),
        (r#"Association'Keys(c)"#, r#"{"x","y"}"#),
        (r#"Association'ToList(c)"#, r#"{{"x",10},{"y",20}}"#),
        (r#"Association'Head(c)"#, r#"{"x",10}"#),
        (r#"Association'Set(c,5,"five")"#, "True"),
        (r#"Association'Keys(c)"#, r#"{5,"x","y"}"#),
        (r#"Association'Contains(c,"x")"#, "True"),
        (r#"Association'Get(c,"z")"#, "Undefined"),
        (r#"Association'Drop(c,"x")"#, "True"),
        (r#"Association'Keys(c)"#, r#"{5,"y"}"#),
        ("Association'Size(c)", "2"),
        // 列表键排序:等前缀短者在前
        ("d := Association'Create()", "Association({})"),
        (r#"Association'Set(d,{1,2},"A")"#, "True"),
        (r#"Association'Set(d,{1,2,0},"C")"#, "True"),
        (r#"Association'Set(d,{1,3},"B")"#, "True"),
        (r#"Association'Keys(d)"#, r#"{{1,2},{1,2,0},{1,3}}"#),
        (
            r#"Association'ToList(d)"#,
            r#"{{{1,2},"A"},{{1,2,0},"C"},{{1,3},"B"}}"#,
        ),
        (r#"Association'Head(d)"#, r#"{{1,2},"A"}"#),
        (
            r#"TrapError(Association'Head(Association'Create()), "caught")"#,
            "\"caught\"",
        ),
        (
            r#"TrapError(Association'Get(42,"k"), "caught")"#,
            "\"caught\"",
        ),
        // CreateFromList 端到端
        (
            r#"m := Association'CreateFromList({{"a",1},{"b",2},{"c",3}})"#,
            r#"Association({{"a",1},{"b",2},{"c",3}})"#,
        ),
        ("Association'Size(m)", "3"),
        (r#"Association'Keys(m)"#, r#"{"a","b","c"}"#),
        (r#"Association'ToList(m)"#, r#"{{"a",1},{"b",2},{"c",3}}"#),
        (r#"Association'Get(m,"b")"#, "2"),
        (r#"Association'Get(m,"z")"#, "Undefined"),
        (r#"Association'Drop(m,"a")"#, "True"),
        (r#"Association'Keys(m)"#, r#"{"b","c"}"#),
        ("Association'CreateFromList({})", "Association({})"),
        (
            r#"Association'Size(Association'CreateFromList({{"x",{1,2}},{"x",99}}))"#,
            "1",
        ),
    ];
    for (p, exp) in probes {
        let got = run(&mut e, p);
        assert_eq!(got, exp, "probe {p}");
    }
}
