//! 嵌套索引赋值 m[i][j] := v(照 cyacas deffunc.rep `:=` 规则 10 的共享链语义)。
//!
//! 根因:cyacas Nth 返回共享链节点,DestructiveReplace 原地改 → 写回 m 可见;
//! Rust eval 拷贝断链 + 脚本规则丢根变量信息 → 改孤儿拷贝。修复:eval 层 `:=`
//! 特判,左值为纯 Nth 嵌套树时解析根变量+索引链,深层重建写回根变量槽。
//! 76 处脚本调用(linalg/solve/graph 等),Transpose 等依赖。
use yacas_rs::env::Environment;
use yacas_rs::evaluator::eval;
use yacas_rs::parser::parse_expression;
use yacas_rs::printer::infix_print;
fn run(env: &mut Environment, src: &str) -> String {
    let t = parse_expression(env, &format!("{src};")).unwrap().expect("ok");
    match eval(env, &t) { Ok(r) => infix_print(env, &r), Err(e) => format!("ERR({e:?})") }
}
#[test]
fn nested_assign() {
    let mut e = Environment::new();
    run(&mut e, &format!("DefaultDirectory(\"{}/\")", concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts")));
    assert_eq!(run(&mut e, "Load(\"yacasinit.ys\")"), "True");
    let probes = [
        ("[Local(m);m:=ZeroMatrix(2,2);m[1][2]:=a;m;]", "{{0,a},{0,0}}"),
        ("[Local(m2);m2:={{0,0},{0,0}};m2[2][1]:=7;m2;]", "{{0,0},{7,0}}"),
        ("[Local(m);m:=ZeroMatrix(2,2);m[1][1]:=5;m[2][2]:=6;m;]", "{{5,0},{0,6}}"),
        // 单层索引仍走原路径(回归)
        ("[Local(v);v:={1,2,3};v[2]:=9;v;]", "{1,9,3}"),
        // Transpose(依赖嵌套赋值)修复
        ("Transpose({{a,b},{1,2}})", "{{a,1},{b,2}}"),
        ("Transpose({{1,2},{3,4}})", "{{1,3},{2,4}}"),
        // 深层三索引
        ("[Local(t);t:={{{1,2},{3,4}},{{5,6},{7,8}}};t[2][1][2]:=x;t;]", "{{{1,2},{3,4}},{{5,x},{7,8}}}"),
    ];
    for (p, exp) in probes {
        assert_eq!(run(&mut e, p), exp, "probe {p}");
    }
}
