//! Shared numerical evaluation services for product-level algorithms.

use crate::engine::{Engine, EngineError, EvalResult, Expr};
use crate::input::{strip_tex_delimiters, validate_expression, validate_symbol};
use serde::Serialize;

/// Product API limit. The core supports a larger technical ceiling, but an
/// interactive request at extreme precision is not a useful default contract.
pub const MAX_PRECISION_DIGITS: u32 = 1_000;
pub const MAX_TAYLOR_DEGREE: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NumericKind {
    ExactReal,
    ApproximateReal,
    Complex,
    NonFinite,
    Unresolved,
}

#[derive(Debug, Clone, Serialize)]
pub struct NumericResult {
    pub output: String,
    pub tex: String,
    pub precision_digits: u32,
    pub kind: NumericKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RootStatus {
    Converged,
    NoConvergence,
    Unresolved,
}

#[derive(Debug, Clone, Serialize)]
pub struct RootResult {
    pub status: RootStatus,
    pub output: String,
    pub tex: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaylorResult {
    pub output: String,
    pub tex: String,
    pub degree: u32,
    pub unresolved: bool,
}

pub fn approximate(
    engine: &mut dyn Engine,
    expression: &str,
    precision_digits: u32,
) -> Result<NumericResult, EngineError> {
    validate_expression(expression, "数值表达式")?;
    validate_precision(precision_digits)?;
    let result = engine.eval(&format!("N({expression},{precision_digits})"))?;
    Ok(NumericResult {
        output: result.expr.to_string(),
        tex: strip_tex_delimiters(&result.tex),
        precision_digits,
        kind: numeric_kind(&result.expr),
    })
}

pub fn find_root(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    initial: f64,
    accuracy: f64,
    bounds: Option<(f64, f64)>,
) -> Result<RootResult, EngineError> {
    validate_expression(expression, "求根表达式")?;
    validate_symbol(variable, "求根变量")?;
    if !initial.is_finite() || !accuracy.is_finite() || accuracy <= 0.0 {
        return Err(EngineError::InvalidInput(
            "初值必须有限，精度必须为有限正数".into(),
        ));
    }
    let command = if let Some((min, max)) = bounds {
        if !min.is_finite() || !max.is_finite() || min >= max || initial <= min || initial >= max {
            return Err(EngineError::InvalidInput(
                "求根区间必须有限且递增，初值必须位于区间内部".into(),
            ));
        }
        format!("Newton({expression},{variable},{initial},{accuracy},{min},{max})")
    } else {
        format!("Newton({expression},{variable},{initial},{accuracy})")
    };
    let result = engine.eval(&command)?;
    let status = match &result.expr {
        Expr::Symbol(value) if value == "Fail" => RootStatus::NoConvergence,
        expr if matches!(
            numeric_kind(expr),
            NumericKind::ExactReal | NumericKind::ApproximateReal | NumericKind::Complex
        ) =>
        {
            RootStatus::Converged
        }
        _ => RootStatus::Unresolved,
    };
    Ok(RootResult {
        status,
        output: result.expr.to_string(),
        tex: strip_tex_delimiters(&result.tex),
    })
}

pub fn taylor(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    point: &str,
    degree: u32,
) -> Result<TaylorResult, EngineError> {
    validate_expression(expression, "级数表达式")?;
    validate_expression(point, "展开点")?;
    validate_symbol(variable, "展开变量")?;
    if degree > MAX_TAYLOR_DEGREE {
        return Err(EngineError::InvalidInput(format!(
            "Taylor 阶数不能超过 {MAX_TAYLOR_DEGREE}"
        )));
    }
    let result = engine.eval(&format!(
        "Taylor({variable},{point},{degree})({expression})"
    ))?;
    let unresolved = matches!(&result.expr, Expr::Call { head, .. } if head == "Taylor");
    Ok(TaylorResult {
        output: result.expr.to_string(),
        tex: strip_tex_delimiters(&result.tex),
        degree,
        unresolved,
    })
}

fn validate_precision(precision_digits: u32) -> Result<(), EngineError> {
    if (1..=MAX_PRECISION_DIGITS).contains(&precision_digits) {
        Ok(())
    } else {
        Err(EngineError::InvalidInput(format!(
            "数值精度必须在 1..={MAX_PRECISION_DIGITS} 位之间"
        )))
    }
}

fn numeric_kind(expr: &Expr) -> NumericKind {
    match expr {
        Expr::Number(value) if value.contains(['.', 'e', 'E']) => NumericKind::ApproximateReal,
        Expr::Number(_) => NumericKind::ExactReal,
        Expr::Symbol(value) if matches!(value.as_str(), "Infinity" | "Undefined") => {
            NumericKind::NonFinite
        }
        Expr::Call { head, args }
            if head == "-"
                && args.len() == 1
                && numeric_kind(&args[0]) == NumericKind::NonFinite =>
        {
            NumericKind::NonFinite
        }
        Expr::Call { head, args } if head == "Complex" && args.len() == 2 => {
            if args
                .iter()
                .any(|value| numeric_kind(value) == NumericKind::NonFinite)
            {
                NumericKind::NonFinite
            } else if args.iter().all(|value| {
                matches!(
                    numeric_kind(value),
                    NumericKind::ExactReal | NumericKind::ApproximateReal
                )
            }) {
                NumericKind::Complex
            } else {
                NumericKind::Unresolved
            }
        }
        _ => NumericKind::Unresolved,
    }
}

/// Evaluate `expression(variable = value)` for all values in bounded batches.
///
/// Each batch uses one engine request. Non-finite or unresolved scalar results
/// are represented as `NaN`, allowing callers to apply domain-specific gap or
/// convergence policies without duplicating engine protocol code.
pub fn evaluate_real_batch(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    values: &[f64],
    batch_size: usize,
) -> Result<Vec<f64>, EngineError> {
    let mut output = Vec::with_capacity(values.len());
    for chunk in values.chunks(batch_size.max(1)) {
        let items: Vec<String> = chunk
            .iter()
            .map(|value| {
                format!("N(Eval(ApplyPure(\"Subst\", {{{variable},{value},{expression}}})))")
            })
            .collect();
        let command = format!("N({{{}}})", items.join(", "));
        let result: EvalResult = engine.eval(&command)?;
        let batch = flatten_number_list(&result.expr);
        if batch.len() != chunk.len() {
            return Err(EngineError::Eval(format!(
                "batch evaluation returned {} values, expected {}",
                batch.len(),
                chunk.len()
            )));
        }
        output.extend(batch);
    }
    Ok(output)
}

fn flatten_number_list(expr: &Expr) -> Vec<f64> {
    fn scalar(expr: &Expr) -> Option<f64> {
        match expr {
            Expr::Number(value) => value.parse::<f64>().ok(),
            _ => None,
        }
    }
    match expr {
        Expr::Call { head, args } if head == "List" => args
            .iter()
            .map(|expr| scalar(expr).unwrap_or(f64::NAN))
            .collect(),
        Expr::Call { head, args } if head == "N" => {
            args.first().map(flatten_number_list).unwrap_or_default()
        }
        one => scalar(one).into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;

    #[test]
    fn batched_evaluation_matches_direct_values_and_preserves_gaps() {
        let mut engine = RustEngine::spawn().expect("boot");
        let xs: Vec<f64> = (0..16).map(|i| 0.1 * i as f64).collect();
        let ys = evaluate_real_batch(&mut engine, "Sin(x)*Exp(-x)", "x", &xs, 8).expect("batch");
        for (x, y) in xs.iter().zip(&ys) {
            let expected = x.sin() * (-x).exp();
            assert!((y - expected).abs() < 1e-9, "at x={x}");
        }

        let with_pole = evaluate_real_batch(&mut engine, "1/x", "x", &[-1.0, 0.0, 1.0], 3)
            .expect("batch with pole");
        assert_eq!(with_pole[0], -1.0);
        assert!(with_pole[1].is_nan());
        assert_eq!(with_pole[2], 1.0);
    }

    #[test]
    fn classifies_public_numeric_results() {
        let mut engine = RustEngine::spawn().expect("boot");
        for (expression, expected) in [
            ("1/2", NumericKind::ApproximateReal),
            ("1", NumericKind::ExactReal),
            ("Ln(-2)", NumericKind::Complex),
            ("1/0", NumericKind::NonFinite),
            ("-Infinity", NumericKind::NonFinite),
            ("x+1/2", NumericKind::Unresolved),
        ] {
            assert_eq!(
                approximate(&mut engine, expression, 20).unwrap().kind,
                expected
            );
        }
    }

    #[test]
    fn finds_roots_and_builds_taylor_polynomials() {
        let mut engine = RustEngine::spawn().expect("boot");
        let root = find_root(&mut engine, "Sin(x)", "x", 3.0, 1e-12, Some((2.0, 4.0))).unwrap();
        assert_eq!(root.status, RootStatus::Converged);
        let value: f64 = root.output.parse().unwrap();
        assert!((value - std::f64::consts::PI).abs() < 1e-9);

        let failed = find_root(&mut engine, "x^2+1", "x", 1.0, 1e-10, Some((0.0, 2.0))).unwrap();
        assert_eq!(failed.status, RootStatus::NoConvergence);

        let series = taylor(&mut engine, "Exp(x)", "x", "0", 5).unwrap();
        assert!(!series.unresolved);
        assert_eq!(
            engine
                .eval(&format!(
                    "Simplify(({})-(1+x+x^2/2+x^3/6+x^4/24+x^5/120))",
                    series.output
                ))
                .unwrap()
                .expr
                .to_string(),
            "0"
        );
    }

    #[test]
    fn rejects_invalid_public_numeric_requests() {
        let mut engine = RustEngine::spawn().expect("boot");
        assert!(approximate(&mut engine, "Pi", 0).is_err());
        assert!(approximate(&mut engine, "Pi", MAX_PRECISION_DIGITS + 1).is_err());
        assert!(find_root(&mut engine, "x", "x", 0.0, 0.0, None).is_err());
        assert!(find_root(&mut engine, "x", "x", 0.0, 1e-6, Some((1.0, -1.0))).is_err());
        assert!(taylor(&mut engine, "Exp(x)", "x", "0", MAX_TAYLOR_DEGREE + 1).is_err());
        assert!(taylor(&mut engine, "x);Echo(1);(x", "x", "0", 2).is_err());
        assert_eq!(engine.eval("2+3").unwrap().expr.to_string(), "5");
    }
}
