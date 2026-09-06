//! 引擎缺陷专项:最小复现基线(每缺陷一个断言;修一个绿一个)。
//!
//! 缺陷清单(探查记录见 processing 侧会话):
//! - D1 IsNumber(1/2) = False(有理式以未求值 MathDiv 存在)
//! - D2 N() 经函数参数传入不求值(顶层 N(1/2)=0.5,参数路径原样)
//! - D3 IsFreeOf(函数头, 含该头的表达式) = True(误判"自由")
//! - D4 IsOddFunction(x^3, x) = False((−x)^n 未展开/规范化缺失)
//! - D5 Simplify 对非规范存储树产出代数不等结果(零 → 非零)
//! - D6 求值快路径:已规范化树每次求值仍全量重扫规则表(性能)
//! - D7 引擎求值无超时机制(病态输入挂死)

use yacas_rs::env::Environment;
use yacas_rs::evaluator::eval;
use yacas_rs::printer::infix_print;

fn run(env: &mut Environment, src: &str) -> String {
    let tree = yacas_rs::parser::parse_expression(env, &format!("{src};"))
        .unwrap_or_else(|e| panic!("parse {src}: {e:?}"))
        .expect("非空");
    let result = eval(env, &tree).unwrap_or_else(|e| panic!("eval {src}: {e:?}"));
    infix_print(env, &result)
}

/// 统一装载序(照 yacasinit.ys;含 predicates/numerical 等后续按需追加)。
fn boot(env: &mut Environment) {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/");
    for f in [
        "patterns.rep/code.ys",
        "deffunc.rep/code.ys",
        "standard.ys",
        "stdarith.ys",
        "stubs.rep/code.ys",
        "linalg.rep/code.ys",
        "lists.rep/code.ys",
        "lists.rep/scopestack.ys",
        "localrules.rep/code.ys",
        "logic.rep/code.ys",
        "newly.rep/code.ys",
        "predicates.rep/code.ys",
        "controlflow.rep/code.ys",
    ] {
        yacas_rs::standard::internal_load(env, &format!("{p}{f}")).unwrap();
    }
}

#[test]
fn d7_eval_timeout() {
    let mut env = Environment::new();
    boot(&mut env);
    env.set_eval_timeout(Some(std::time::Duration::from_millis(100)));
    let t0 = std::time::Instant::now();
    let tree = yacas_rs::parser::parse_expression(&mut env, "For(i:=1,i<=50000000,i++,j)")
        .unwrap()
        .expect("非空");
    let r = eval(&mut env, &tree);
    assert!(r.is_err(), "超时应产生错误: {:?}", r.map(|v| infix_print(&env, &v)));
    assert!(t0.elapsed() < std::time::Duration::from_secs(5), "超时应在 bounded 时间内触发,实际 {:?}", t0.elapsed());
    // 清除后恢复正常
    env.set_eval_timeout(None);
}

/// D1/D2 的共同根因:fork 用未求值 `/` 树表示有理数(上游 BigNumber 原生
/// QQ,Rational 是 Number 节点)。修复 = 数值层加有理数表示(独立里程碑),
/// 此前两断言保持失败挂账。
#[test]
#[ignore = "D1:等有理数表示(数值层里程碑)"]
fn d1_is_number_rational() {
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "IsNumber(4)"), "True");
    assert_eq!(run(&mut env, "IsNumber(1/2)"), "True");
}

#[test]
#[ignore = "D2:与 D1 同根(有理数表示);上游变量取值同样不重求值"]
fn d2_n_through_parameter() {
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "N(1/2)"), "0.5");
    assert_eq!(run(&mut env, "scrap(y) := N(y)"), "True");
    assert_eq!(run(&mut env, "scrap(1/2)"), "0.5");
}

#[test]
fn d3_is_free_of_function_head() {
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "IsFreeOf(Integrate, Sin(x))"), "True");
    assert_eq!(run(&mut env, "IsFreeOf(Integrate, Integrate(x)(x))"), "False");
}

#[test]
fn d4_is_odd_function_power() {
    let mut env = Environment::new();
    boot(&mut env);
    // 注:Sin(x) 的奇偶判定还依赖完整应用链的三角规范化规则,不在最小 boot 内
    assert_eq!(run(&mut env, "IsOddFunction(x^3, x)"), "True");
    assert_eq!(run(&mut env, "IsOddFunction(x^2, x)"), "False");
}
