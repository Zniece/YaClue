//! standard.ys 直装路径锁步(boot:patterns/deffunc/standard/stdarith/stubs)。
//! 注:2026-09-05 文件曾被误清空,依据 target 旧编译产物字符串 + oracle 复核重建;
//! 七个测试名与探针逐一恢复,断言值全部经 oracle 重新钉死。
use yacas_rs::env::Environment;
use yacas_rs::errors::YacasError;
use yacas_rs::evaluator::eval;
use yacas_rs::parser::parse_expression;
use yacas_rs::printer::infix_print;

fn run(env: &mut Environment, src: &str) -> String {
    let t = parse_expression(env, &format!("{src};"))
        .unwrap()
        .expect("ok");
    match eval(env, &t) {
        Ok(r) => infix_print(env, &r),
        Err(e) => format!("ERR({e:?})"),
    }
}
fn err_of(env: &mut Environment, src: &str) -> YacasError {
    let t = parse_expression(env, &format!("{src};"))
        .unwrap()
        .expect("ok");
    match eval(env, &t) {
        Err(e) => e,
        Ok(_) => panic!("期望 {src} 报错,实际成功"),
    }
}

fn boot(env: &mut Environment) {
    let code_ys = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../yacas/scripts/patterns.rep/code.ys"
    );
    yacas_rs::standard::internal_load(env, code_ys).unwrap();
    let deffunc_ys = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../yacas/scripts/deffunc.rep/code.ys"
    );
    yacas_rs::standard::internal_load(env, deffunc_ys).unwrap();
    let standard_ys = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../yacas/scripts/standard.ys"
    );
    yacas_rs::standard::internal_load(env, standard_ys).unwrap();
    // T5-b:stdarith(cyacas console 序 standard 之后)—— V(3+4) 等脚本算术。
    let stdarith_ys = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../yacas/scripts/stdarith.ys"
    );
    yacas_rs::standard::internal_load(env, stdarith_ys).unwrap();
    // Step 2:Mod 去核心注册(照 cyacas corefunctions.h 只留 MathMod)——
    // Mod 语义全由 stubs.rep 规则承担(负数 floor 调整),boot 环境须与
    // console 一致装载 stubs,否则 Mod 保持(oracle 同环境下也保持)。
    let stubs_ys = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../yacas/scripts/stubs.rep/code.ys"
    );
    yacas_rs::standard::internal_load(env, stubs_ys).unwrap();
}

#[test]
fn bitwise_and_mod() {
    // cyacas 实测:2&5→0(位与)、1|2→3(位或)、5%2→1、Mod(5,2)→1 ——
    // standard.ys 的 a_IsNonNegativeInteger & b_IsNonNegativeInteger <-- BitAnd(a,b)
    // 等后缀谓词模式规则 + 对应命令(行为锁,非手写)。
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "2&5"), "0");
    assert_eq!(run(&mut env, "1|2"), "3");
    assert_eq!(run(&mut env, "5%2"), "1");
    assert_eq!(run(&mut env, "Mod(5,2)"), "1");
}

#[test]
fn eq_neq_rules_bases_only() {
    // cyacas 实测:==/!== 只有规则底、无规则 —— 全部保持原样输出。
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "a==b"), "a==b");
    assert_eq!(run(&mut env, "1==1"), "1==1");
    assert_eq!(run(&mut env, "1==2"), "1==2");
    assert_eq!(run(&mut env, "a!==b"), "a!==b");
    assert_eq!(run(&mut env, "1!==1"), "1!==1");
    assert_eq!(run(&mut env, "1!==2"), "1!==2");
}

#[test]
fn if_else_from_script() {
    // cyacas 实测:if(True) 1 else 2 → 1;False → 2;3(非布尔)→ 保持 if(3)1 else 2。
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "if(True) 1 else 2"), "1");
    assert_eq!(run(&mut env, "if(False) 1 else 2"), "2");
    assert_eq!(run(&mut env, "if(3) 1 else 2"), "if(3)1 else 2");
}

#[test]
fn increment_decrement() {
    // cyacas 实测:w:=5 → 5(:= 返回右值);w++ 语句值 True、变量副作用 +1;
    // z-- 同理 -1(oracle 复核 2026-09-05)。
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "w := 5"), "5");
    assert_eq!(run(&mut env, "w++"), "True");
    assert_eq!(run(&mut env, "w"), "6");
    assert_eq!(run(&mut env, "z := 8"), "8");
    assert_eq!(run(&mut env, "z--"), "True");
    assert_eq!(run(&mut env, "z"), "7");
}

#[test]
fn nrargs_and_normalform() {
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "NrArgs(f(a,b))"), "2");
    assert_eq!(run(&mut env, "NrArgs(f(a,b,c))"), "3");
    assert!(matches!(
        err_of(&mut env, "NrArgs(x)"),
        YacasError::InvalidArg
    ));
    assert_eq!(run(&mut env, "NormalForm(2*x)"), "2*x");
}

#[test]
fn nth_from_script_matches_cyacas() {
    // cyacas 实测:Nth({a,b,c},1/2/3)→a/b/c;Nth({a,b,c},4)→ListNotLongEnough 错;
    // Nth(x,1)→x[1](规则 10 谓词不命中 → 保持,InfixPrinter 的 Nth 分支打印 x[1])。
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "Nth({a,b,c},1)"), "a");
    assert_eq!(run(&mut env, "Nth({a,b,c},2)"), "b");
    assert_eq!(run(&mut env, "Nth({a,b,c},3)"), "c");
    assert!(matches!(
        err_of(&mut env, "Nth({a,b,c},4)"),
        YacasError::ListNotLongEnough
    ));
}

#[test]
fn verbose_v_func() {
    // cyacas 实测:V(expr) = 关 verbose 求值,结果原样(V(3+4) → 7)。
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "V(3+4)"), "7");
}
