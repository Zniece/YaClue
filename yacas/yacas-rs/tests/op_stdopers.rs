//! 层 7 T1 验收:stdopers.ys 真装载(运算符命令实装)。
//!
//! 断言方向照契约 §7(行为锁 cyacas,期望值全部 cyacas 实测,零手写):
//! 1) 装载幂等:`internal_use(stdopers.ys)` 后四张运算符表 == 装载前静态启动表
//!    (静态表是层 4 已对拍 cyacas 启动状态的基准;脚本执行的 Infix/Prefix/Bodied/
//!    Postfix/RightAssociative/RightPrecedence 命令必须逐条复现之,不多不差);
//! 2) 解析探针与 cyacas 实测一致(a^b^c 右结合平铺、1+2*3=7、-a、新注册中缀
//!    Infix("====",OpPrecedence("=")) 后 a====b、OpPrecedence("=")=90、
//!    OpRightPrecedence("-")=40);
//! 3) 错误路径:RightAssociative(非中缀名) → YacasError::NotAnInfixOperator
//!    (cyacas 实测报"非中缀"错,对应 C++ LispErrNotAnInFixOperator)。

use yacas_rs::env::Environment;
use yacas_rs::errors::YacasError;
use yacas_rs::evaluator::eval;
use yacas_rs::parser::parse_expression;
use yacas_rs::printer::infix_print;
use yacas_rs::standard::internal_use;

fn run(env: &mut Environment, src: &str) -> String {
    let tree = parse_expression(env, &format!("{src};"))
        .unwrap_or_else(|e| panic!("parse {src}: {e:?}"))
        .expect("非空");
    let result = eval(env, &tree).unwrap_or_else(|e| panic!("eval {src}: {e:?}"));
    infix_print(env, &result)
}

/// 四张运算符表 → 确定性文本行(表别+名排序;每行 "表 名 prec left right assoc")。
fn dump(env: &Environment) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    for (tag, slot) in [
        ("i", &env.infix),
        ("p", &env.prefix),
        ("f", &env.postfix),
        ("b", &env.bodied),
    ] {
        for (k, op) in slot {
            rows.push(format!(
                "{tag} {} {} {} {} {}",
                k, op.prec, op.left_prec, op.right_prec, op.right_assoc
            ));
        }
    }
    rows.sort();
    rows
}

fn stdopers_path() -> String {
    format!(
        "{}/../../yacas/scripts/stdopers.ys",
        env!("CARGO_MANIFEST_DIR")
    )
}

#[test]
fn use_stdopers_is_idempotent() {
    let mut env = Environment::new();
    let before = dump(&env);
    internal_use(&mut env, &stdopers_path()).unwrap();
    let after = dump(&env);
    assert_eq!(
        before, after,
        "stdopers.ys 装载必须逐条复现静态启动表(不多、不差、不改)"
    );
    // 再装一轮仍幂等(cyacas Use() 对已载文件直接 True)
    internal_use(&mut env, &stdopers_path()).unwrap();
    assert_eq!(after, dump(&env));
}

#[test]
fn stdopers_parse_probes_match_cyacas() {
    let mut env = Environment::new();
    // `1+2*3 → 7` 需完整脚本链(照 yacasinit.ys:36-45:patterns → deffunc →
    // standard → stdarith)。运算符表静态启动表已内建,stdopers 装载幂等复现。
    for f in [
        "patterns.rep/code.ys",
        "deffunc.rep/code.ys",
        "standard.ys",
        "stdarith.ys",
    ] {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/");
        internal_use(&mut env, &format!("{p}{f}")).unwrap();
    }
    // cyacas 实测:RightAssociative("^") → a^b^c 平铺(右结合,无多余括号)
    assert_eq!(run(&mut env, "a^b^c"), "a^b^c");
    // 中缀优先级:1+2*3 → 7
    assert_eq!(run(&mut env, "1+2*3"), "7");
    // 前缀减:-a
    assert_eq!(run(&mut env, "-a"), "-a");
    // OpPrecedence 族(cyacas 实测 90 / 40)
    assert_eq!(run(&mut env, "OpPrecedence(\"=\")"), "90");
    assert_eq!(run(&mut env, "OpRightPrecedence(\"-\")"), "40");
    // 新注册同优先级中缀(cyacas 实测 a====b 平铺)
    assert_eq!(
        run(&mut env, "Infix(\"====\", OpPrecedence(\"=\"))"),
        "True"
    );
    assert_eq!(run(&mut env, "a ==== b"), "a====b");
    // RightAssociative 本身(cyacas 实测返回 True)
    assert_eq!(run(&mut env, "RightAssociative(\"^\")"), "True");
}

#[test]
fn right_assoc_non_infix_is_error() {
    let mut env = Environment::new();
    // cyacas 实测:非中缀名报"非中缀操作符"错 → 错误变体 NotAnInfixOperator
    let tree = parse_expression(&mut env, "RightAssociative(\"zzz\");")
        .unwrap()
        .unwrap();
    let err = match eval(&mut env, &tree) {
        Err(e) => e,
        Ok(_) => panic!("期望 RightAssociative(非中缀) 报错,实际成功"),
    };
    assert!(
        matches!(err, YacasError::NotAnInfixOperator),
        "期望 NotAnInfixOperator,实际 {err:?}"
    );
}
