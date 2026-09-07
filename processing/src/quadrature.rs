//! Bounded adaptive Simpson quadrature for definite-integral fallback.

use crate::engine::{Engine, EngineError};
use crate::plot::eval_batched;

#[derive(Debug, Clone)]
pub struct QuadratureOptions {
    pub abs_tol: f64,
    pub rel_tol: f64,
    pub max_depth: u32,
    pub max_evaluations: usize,
}

impl Default for QuadratureOptions {
    fn default() -> Self {
        Self { abs_tol: 1e-9, rel_tol: 1e-9, max_depth: 20, max_evaluations: 20_000 }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct QuadratureResult {
    pub value: f64,
    pub estimated_error: f64,
    pub evaluations: usize,
}

struct AdaptiveState<'a> {
    engine: &'a mut dyn Engine,
    func: &'a str,
    var: &'a str,
    options: &'a QuadratureOptions,
    evaluations: usize,
}

impl AdaptiveState<'_> {
    fn values(&mut self, xs: &[f64]) -> Result<Vec<f64>, EngineError> {
        if self.evaluations + xs.len() > self.options.max_evaluations {
            return Err(EngineError::Eval(format!(
                "数值积分超过最大采样数 {}",
                self.options.max_evaluations
            )));
        }
        let values = eval_batched(self.engine, self.func, self.var, xs, xs.len().max(1))?;
        self.evaluations += xs.len();
        if let Some((x, _)) = xs.iter().zip(&values).find(|(_, y)| !y.is_finite()) {
            return Err(EngineError::Eval(format!("数值积分在 x={x} 处得到非有限值")));
        }
        Ok(values)
    }

    #[allow(clippy::too_many_arguments)]
    fn refine(
        &mut self,
        a: f64,
        b: f64,
        fa: f64,
        fm: f64,
        fb: f64,
        whole: f64,
        tolerance: f64,
        depth: u32,
    ) -> Result<(f64, f64), EngineError> {
        let m = (a + b) / 2.0;
        let left_mid = (a + m) / 2.0;
        let right_mid = (m + b) / 2.0;
        let values = self.values(&[left_mid, right_mid])?;
        let left = (m - a) * (fa + 4.0 * values[0] + fm) / 6.0;
        let right = (b - m) * (fm + 4.0 * values[1] + fb) / 6.0;
        let combined = left + right;
        let error = (combined - whole).abs() / 15.0;

        if error <= tolerance {
            return Ok((combined + (combined - whole) / 15.0, error));
        }
        if depth == 0 {
            return Err(EngineError::Eval(format!(
                "数值积分未在指定深度内收敛（估计误差 {error:e}，目标 {tolerance:e}）"
            )));
        }
        let (left_value, left_error) = self.refine(
            a,
            m,
            fa,
            values[0],
            fm,
            left,
            tolerance / 2.0,
            depth - 1,
        )?;
        let (right_value, right_error) = self.refine(
            m,
            b,
            fm,
            values[1],
            fb,
            right,
            tolerance / 2.0,
            depth - 1,
        )?;
        Ok((left_value + right_value, left_error + right_error))
    }
}

pub fn adaptive_simpson(
    engine: &mut dyn Engine,
    func: &str,
    var: &str,
    range: (f64, f64),
    options: &QuadratureOptions,
) -> Result<QuadratureResult, EngineError> {
    let (a, b) = range;
    if !(a.is_finite() && b.is_finite()) || a == b {
        if a == b && a.is_finite() {
            return Ok(QuadratureResult { value: 0.0, estimated_error: 0.0, evaluations: 0 });
        }
        return Err(EngineError::Eval("数值积分上下限必须是有限数".into()));
    }
    if !(options.abs_tol > 0.0
        && options.rel_tol >= 0.0
        && options.abs_tol.is_finite()
        && options.rel_tol.is_finite()
        && options.max_evaluations >= 5)
    {
        return Err(EngineError::Eval("数值积分选项无效".into()));
    }

    let (lo, hi, sign) = if a < b { (a, b, 1.0) } else { (b, a, -1.0) };
    let mid = (lo + hi) / 2.0;
    let mut state = AdaptiveState { engine, func, var, options, evaluations: 0 };
    let values = state.values(&[lo, mid, hi])?;
    let whole = (hi - lo) * (values[0] + 4.0 * values[1] + values[2]) / 6.0;
    let tolerance = options.abs_tol.max(options.rel_tol * whole.abs());
    let (value, estimated_error) = state.refine(
        lo,
        hi,
        values[0],
        values[1],
        values[2],
        whole,
        tolerance,
        options.max_depth,
    )?;
    Ok(QuadratureResult {
        value: sign * value,
        estimated_error,
        evaluations: state.evaluations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;

    #[test]
    fn integrates_smooth_functions_and_reversed_ranges() {
        let mut engine = RustEngine::spawn().unwrap();
        let options = QuadratureOptions::default();
        let forward = adaptive_simpson(&mut engine, "Sin(x)", "x", (0.0, std::f64::consts::PI), &options).unwrap();
        let reverse = adaptive_simpson(&mut engine, "Sin(x)", "x", (std::f64::consts::PI, 0.0), &options).unwrap();
        assert!((forward.value - 2.0).abs() < 3e-9, "{}", forward.value);
        assert!((reverse.value + 2.0).abs() < 3e-9, "{}", reverse.value);
        assert!(forward.estimated_error <= 3e-9);
    }

    #[test]
    fn rejects_non_finite_samples_and_nonconvergence() {
        let mut engine = RustEngine::spawn().unwrap();
        let options = QuadratureOptions::default();
        assert!(adaptive_simpson(&mut engine, "1/x", "x", (-1.0, 1.0), &options).is_err());

        let shallow = QuadratureOptions { max_depth: 0, abs_tol: 1e-15, rel_tol: 0.0, ..options };
        assert!(adaptive_simpson(&mut engine, "Exp(x)", "x", (0.0, 1.0), &shallow).is_err());
    }
}
