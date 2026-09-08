//! Shared numerical evaluation services for product-level algorithms.

use crate::engine::{Engine, EngineError, EvalResult, Expr};

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
}
