//! MathSqrt 惰性加载语义 + 数值链回归(与 cyacas oracle 锁步)。
//!
//! 两个关键结论(2026-09 诊断):
//! 1. `MathSqrt1`/`MathSqrtFloat` 不在 elemfuncs.ys.def 清单 → 默认 boot 下直调
//!    永不触发该文件加载 → 停在自身(cyacas 实测同行为,是惰性加载设计,非 bug)。
//!    先调 `MathSqrt`(在清单)→ 触发加载 → 之后 MathSqrt1 恢复化简(顺序敏感)。
//! 2. `MathSqrtFloat(2.)` 曾死循环(高内存挂死):Float::div 的 scale=prec+shift 在
//!    shift<0(分子有效位多)时 u32 下溢 → mul_pow10 天文数字 → 逐位长除卡死。
//!    修复:整除保留全精度、scale 恒非负 + 分母位差补偿。
use yacas_rs::env::Environment;
use yacas_rs::evaluator::eval;
use yacas_rs::parser::parse_expression;
use yacas_rs::printer::infix_print;

fn run(env: &mut Environment, src: &str) -> String {
    let t = parse_expression(env, &format!("{src};"))
        .unwrap_or_else(|e| panic!("parse {src}: {e:?}")).expect("非空");
    match eval(env, &t) {
        Ok(r) => infix_print(env, &r),
        Err(e) => format!("ERR({e:?})"),
    }
}

/// 惰性加载语义:MathSqrt1/MathSqrtFloat 默认停在自身,MathSqrt 触发加载后恢复。
#[test]
fn lazy_load_semantics() {
    let mut env = Environment::new();
    let d = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/");
    run(&mut env, &format!("DefaultDirectory(\"{d}\")"));
    assert_eq!(run(&mut env, "Load(\"yacasinit.ys\")"), "True");
    // 未加载 elemfuncs:MathSqrt1/MathSqrtFloat 直调停在自身(cyacas 同)
    assert_eq!(run(&mut env, "MathSqrt1(4)"), "MathSqrt1(4)");
    assert_eq!(run(&mut env, "MathSqrtFloat(2.)"), "MathSqrtFloat(2.)");
    // MathSqrt(0) 在 elemfuncs.def 清单 → 触发加载 → 化简
    assert_eq!(run(&mut env, "MathSqrt(0)"), "0");
    // 加载后(MathSqrt1/MathSqrtFloat 规则就位)直调恢复化简 —— 顺序敏感
    assert_eq!(run(&mut env, "MathSqrt1(4)"), "2");
    assert_eq!(run(&mut env, "MathSqrt1(0)"), "0");
    assert_eq!(run(&mut env, "MathSqrt(4)"), "2");
    assert_eq!(run(&mut env, "MathSqrt(16)"), "4");
}

/// Float::div 下溢回归:整除保留全精度、非整除不挂死、极小商不丢信息。
#[test]
fn div_no_underflow() {
    use yacas_rs::number::float::Float;
    let one = Float::from_decimal("1").unwrap();
    let three = Float::from_decimal("3").unwrap();
    let two = Float::from_decimal("2").unwrap();
    // 常规小数除法(golden 值)
    assert_eq!(one.div(&three, 10).unwrap().format(), "0.3333333333");
    assert_eq!(two.div(&three, 10).unwrap().format(), "0.6666666666");
    assert_eq!(one.div(&two, 10).unwrap().format(), "0.5");
    // 整数商(整除)
    assert_eq!(two.div(&one, 10).unwrap().format(), "2");
    // 除 1 保留被除数全精度(cyacas:MathDivide(1.414…,1) 原值不变)
    let x = Float::from_decimal("1.414213562373097945823").unwrap();
    assert_eq!(x.div(&one, 13).unwrap().format(), "1.414213562373097945823");
    // 分子有效位多于分母(shift<0)不挂死(旧实现 u32 下溢 → 死循环);值不脆断言
    let a = Float::from_decimal("0.69314718055994529").unwrap();
    let b = Float::from_decimal("2.30258509299404590").unwrap();
    let r = a.div(&b, 10).unwrap().format();
    assert!(r.starts_with("0.3010"), "0.693/2.302@10 = {r}");
}

/// MathSqrtFloat(2.) 死循环回归:能返回且值对(以前高内存挂死)。
#[test]
fn math_sqrt_float_no_hang() {
    let mut env = Environment::new();
    let d = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/");
    run(&mut env, &format!("DefaultDirectory(\"{d}\")"));
    assert_eq!(run(&mut env, "Load(\"yacasinit.ys\")"), "True");
    assert_eq!(run(&mut env, "MathSqrt(0)"), "0"); // 触发加载
    // 能返回、值以 1.4142 开头(精确位数受精度控制链影响,不断言全串)
    let r = run(&mut env, "MathSqrtFloat(2.)");
    assert!(r.starts_with("1.41421"), "MathSqrtFloat(2.) = {r}");
    let r2 = run(&mut env, "MathSqrt(4)");
    assert_eq!(r2, "2");
    let scaled = run(&mut env, "N(Sqrt(1e100)/1e50)");
    assert!(scaled.starts_with("0.999999"), "scaled sqrt(1e100) = {scaled}");
    let scaled = run(&mut env, "N(Sqrt(1e1000)/1e500)");
    assert!(scaled.starts_with("0.999999"), "scaled sqrt(1e1000) = {scaled}");
}
