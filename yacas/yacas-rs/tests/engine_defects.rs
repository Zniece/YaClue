//! Regression coverage for fixed defects and completed investigations.
//!
//! D3: function-head freedom checks; D4: odd-power parity; D7: eval timeout.
//! D5: closed without reproducing the historical report; reconstructed
//! expression trees and derivative grouping remain covered below.
//! D2: reproducible inherited behavior: N evaluates a direct rational but
//! loses numeric evaluation when the same expression arrives via a function
//! parameter. It remains ignored until fixed.

use yacas_rs::env::Environment;
use yacas_rs::errors::YacasError;
use yacas_rs::evaluator::eval;
use yacas_rs::printer::infix_print;

fn run(env: &mut Environment, src: &str) -> String {
    let tree = yacas_rs::parser::parse_expression(env, &format!("{src};"))
        .unwrap_or_else(|e| panic!("parse {src}: {e:?}"))
        .expect("非空");
    let result = eval(env, &tree).unwrap_or_else(|e| panic!("eval {src}: {e:?}"));
    infix_print(env, &result)
}

fn run_error(env: &mut Environment, src: &str) -> YacasError {
    let tree = yacas_rs::parser::parse_expression(env, &format!("{src};"))
        .unwrap_or_else(|e| panic!("parse {src}: {e:?}"))
        .expect("非空");
    match eval(env, &tree) {
        Ok(value) => panic!("expression should fail, got {}", infix_print(env, &value)),
        Err(error) => error,
    }
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

#[test]
fn malformed_exponent_is_rejected_without_poisoning_engine() {
    let mut env = Environment::new();
    boot(&mut env);
    for source in ["1e*2;", "1E+*2;", "1e-*2;"] {
        assert!(
            yacas_rs::parser::parse_expression(&mut env, source).is_err(),
            "malformed exponent should be rejected: {source}"
        );
    }
    assert_eq!(run(&mut env, "2+2"), "4");
}

#[test]
fn oversized_exact_binary_shifts_are_bounded_without_poisoning_engine() {
    let mut env = Environment::new();
    boot(&mut env);
    for shift in ["1000001", "-1000001", "-9223372036854775808"] {
        assert!(
            matches!(run_error(&mut env, &format!("MathMul2Exp(1., {shift})")), YacasError::NumericOverflow),
            "shift {shift} should return NumericOverflow"
        );
        assert_eq!(run(&mut env, "2+2"), "4");
    }
}

#[test]
#[ignore = "D2: N() does not numerically evaluate a rational passed through a function parameter"]
fn d2_n_through_parameter() {
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "N(1/2)"), "0.5");
    assert_eq!(run(&mut env, "d2numeric(y) := N(y)"), "True");
    assert_eq!(run(&mut env, "d2numeric(1/2)"), "0.5");
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

/// D5 was reported without the original F4 construction. These are guards
/// for reconstructed cases, not a claim that the original defect is fixed.
/// The frozen C++ oracle agrees on their mathematical values; Simplify may
/// leave trigonometric identities unevaluated, so compare numeric values
/// against independent analytic expectations instead of requiring literal 0.
fn boot_d5(env: &mut Environment) {
    env.set_eval_timeout(Some(std::time::Duration::from_secs(20)));
    let scripts = concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts/");
    assert_eq!(run(env, &format!("DefaultDirectory(\"{scripts}\")")), "True");
    assert_eq!(run(env, "Load(\"yacasinit.ys\")"), "True");
    assert_eq!(run(env, "Builtin'Precision'Set(20)"), "True");
}

fn d5_value_at(env: &mut Environment, expression: &str, point: &str) -> f64 {
    let command = format!("N(Eval(ApplyPure(\"Subst\",{{x,{point},{expression}}})))");
    let result = run(env, &command);
    result.parse().unwrap_or_else(|e| panic!("{command}: non-numeric {result:?}: {e}"))
}

fn d5_assert_close(actual: f64, expected: f64, context: &str) {
    assert!(
        actual.is_finite() && (actual - expected).abs() < 1e-12,
        "{context}: expected {expected}, got {actual}"
    );
}

#[test]
fn d5_simplify_preserves_reconstructed_antiderivatives() {
    let mut env = Environment::new();
    boot_d5(&mut env);
    for (setup, noncanonical) in [
        ("F4:=x/2-Sin(x)*Cos(x)/2", false),
        ("F4:=Hold((x+(-2*Sin(x)*Cos(x))/2)/2)", true),
        ("F4:=Subst(theta,x)Hold((theta+(-2*Sin(theta)*Cos(theta))/2)/2)", true),
        ("F4:=ApplyPure(\"Subst\",{theta,x,Hold(theta/2-Sin(2*theta)/4)})", false),
    ] {
        run(&mut env, setup);
        let stored = run(&mut env, "F4");
        if noncanonical {
            assert_ne!(stored, run(&mut env, "Eval(F4)"), "{setup}: must exercise a raw stored tree");
        }
        run(&mut env, "d5Residual:=(Deriv(x) F4)-Sin(x)^2");
        run(&mut env, "d5SimplifiedResidual:=Simplify(d5Residual)");
        run(&mut env, "d5SimplifiedPrimitive:=Simplify(F4)");
        for (point, x) in [
            ("0", 0.0_f64),
            ("1/4", 0.25),
            ("Pi/4", std::f64::consts::FRAC_PI_4),
            ("1", 1.0),
        ] {
            // Analytic primitive of sin(x)^2, evaluated independently in Rust.
            let expected = x / 2.0 - (2.0 * x).sin() / 4.0;
            for expression in ["F4", "d5SimplifiedPrimitive"] {
                let actual = d5_value_at(&mut env, expression, point);
                d5_assert_close(actual, expected, &format!("{setup}: {expression} at {point}"));
            }
            for expression in ["d5Residual", "d5SimplifiedResidual"] {
                let actual = d5_value_at(&mut env, expression, point);
                d5_assert_close(actual, 0.0, &format!("{setup}: {expression} at {point}"));
            }
        }
        assert_eq!(run(&mut env, "F4"), stored, "Simplify must not mutate the stored input");
    }
}

#[test]
fn d5_derivative_residual_requires_explicit_grouping() {
    let mut env = Environment::new();
    boot_d5(&mut env);
    run(&mut env, "F4:=Subst(theta,x)Hold((theta+(-2*Sin(theta)*Cos(theta))/2)/2)");
    // Deriv is bodied: the subtraction belongs to its body unless the
    // derivative call itself is parenthesized. This also holds in cyacas.
    run(&mut env, "d5Correct:=Simplify((Deriv(x) F4)-Sin(x)^2)");
    run(&mut env, "d5Ungrouped:=Simplify(Deriv(x) F4-Sin(x)^2)");
    d5_assert_close(d5_value_at(&mut env, "d5Correct", "Pi/4"), 0.0, "grouped residual");
    d5_assert_close(d5_value_at(&mut env, "d5Ungrouped", "Pi/4"), -0.5, "derivative of the whole difference");
}
