//! 程序块 `;` 终止符语义锁步(oracle 探针钉死 10 形态;修正 COVERAGE §30 误诊:
//! cyacas 块内 `;` = 语句终止符,`]` 前必带,`[1; 2]`/`f(_x)<--[x+1]` 均报错 ——
//! 本引擎逐形态一致;顶层 EOF 无 `;` 两引擎均合法)。
use yacas_rs::env::Environment;
use yacas_rs::evaluator::eval;
use yacas_rs::parser::{parse_expression, ParseError};
use yacas_rs::printer::infix_print;

fn run(env: &mut Environment, src: &str) -> String {
    match parse_expression(env, src) {
        Err(e) => match e {
            ParseError::Generic(m) => format!("PARSE: {m}"),
            other => format!("PARSE({other:?})"),
        },
        Ok(None) => "(empty)".into(),
        Ok(Some(t)) => match eval(env, &t) {
            Ok(r) => infix_print(env, &r),
            Err(e) => format!("ERR({e:?})"),
        },
    }
}

#[test]
fn block_terminator_semantics() {
    // oracle 实测(错误文本逐字符):
    //   [1; 2] → Expecting ; end of statement in program block, but got ] instead
    //   [1 2]  → ... but got 2 instead;[a; b] 同错;f(_x)<--[x+1]; 同错
    //   [1; 2;] → 2;[x:=1; x+1;] → 2;[1;]; → 1;顶层 1+1(无 ;)→ 2
    let mut env = Environment::new();
    let d = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/");
    run(&mut env, &format!("DefaultDirectory(\"{d}\")"));
    assert_eq!(run(&mut env, "Load(\"yacasinit.ys\")"), "True");

    assert_eq!(
        run(&mut env, "[1; 2]"),
        "PARSE: Expecting ; end of statement in program block, but got ] instead"
    );
    assert_eq!(
        run(&mut env, "[1 2]"),
        "PARSE: Expecting ; end of statement in program block, but got 2 instead"
    );
    assert_eq!(run(&mut env, "[1; 2;]"), "2");
    assert_eq!(run(&mut env, "[x:=1; x+1;]"), "2");
    assert_eq!(run(&mut env, "[1;];"), "1");
    assert_eq!(run(&mut env, "1+1"), "2"); // 顶层末语句无 ';' 到 EOF 合法
    assert_eq!(run(&mut env, "[a:=2; a*3;]"), "6");
}
