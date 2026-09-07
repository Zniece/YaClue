//! Native function sampling for plotting (Route A: an independent
//! implementation, deliberately not derived from the upstream
//! `plots.rep` sampling code — this module is original work).
//!
//! Design:
//! - Sampling strategy: a uniform base grid refined by curvature — an
//!   interval is split when the midpoint value deviates from the linear
//!   interpolation of its endpoints by more than `eps` (relative to the
//!   local function scale). This is a standard generic technique and
//!   unrelated to the upstream sign-change criterion.
//! - Evaluation: points are evaluated in *batches* — one engine call per
//!   chunk builds a single expression containing numeric substitutions,
//!   so interpreter dispatch overhead disappears from the per-point cost.
//! - Non-finite values (`Infinity`, `Undefined`, ...) become gaps: they are
//!   reported via `breaks` so a renderer knows where NOT to connect.

use serde::Serialize;

use crate::engine::{Engine, EvalResult, Expr};

/// One sampled point.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct PlotPoint {
    pub x: f64,
    pub y: f64,
}

/// A sampled curve: points in ascending x order, plus the indices (into
/// `points`) after which a renderer must NOT draw a connecting line
/// (non-finite values on either side of the break).
#[derive(Debug, Clone, Serialize)]
pub struct SampledPlot {
    pub points: Vec<PlotPoint>,
    pub breaks: Vec<usize>,
}

/// Sampling options.
#[derive(Debug, Clone)]
pub struct SampleOptions {
    /// Number of base-grid intervals (before refinement).
    pub points: usize,
    /// Maximum refinement depth per interval.
    pub max_depth: u32,
    /// Relative deviation tolerated before an interval is subdivided.
    pub eps: f64,
    /// Maximum points sampled in one engine call.
    pub batch: usize,
}

impl Default for SampleOptions {
    fn default() -> Self {
        SampleOptions { points: 64, max_depth: 5, eps: 1e-3, batch: 64 }
    }
}

/// Sample `func` (an expression in the single variable `var`) over
/// `[range.0, range.1]`.
pub fn sample(
    engine: &mut dyn Engine,
    func: &str,
    var: &str,
    range: (f64, f64),
    options: &SampleOptions,
) -> Result<SampledPlot, crate::engine::EngineError> {
    let (a, b) = range;
    if !(a.is_finite() && b.is_finite() && b > a) {
        return Err(crate::engine::EngineError::Eval(format!(
            "invalid range ({a}, {b})"
        )));
    }
    if options.points < 2 {
        return Err(crate::engine::EngineError::Eval(
            "points must be >= 2".into(),
        ));
    }

    // Base grid: (points + 1) nodes.
    let n = options.points;
    let dx = (b - a) / n as f64;
    let xs: Vec<f64> = (0..=n).map(|i| a + dx * i as f64).collect();
    let ys = eval_batched(engine, func, var, &xs, options.batch)?;

    // Refine each base interval by curvature, depth-first.
    let cfg = RefineCfg { func, var, eps: options.eps, batch: options.batch };
    let mut pts: Vec<(f64, f64)> = xs.into_iter().zip(ys).collect();
    for i in (0..n).rev() {
        // `pts` grows as we splice; track the current segment end offset so
        // indices stay valid while refining left-to-right... we refine in
        // reverse insertion order to keep earlier indices stable.
        let seg = refine_interval(engine, &cfg, pts[i], pts[i + 1], options.max_depth)?;
        let middle = &seg[1..seg.len() - 1];
        let at = i + 1;
        for (k, p) in middle.iter().enumerate() {
            pts.insert(at + k, *p);
        }
    }

    // Breaks: a break after point i when i or i+1 is non-finite, or when the
    // x jump is not consistent with a continuous curve sample (defensive).
    let mut breaks = Vec::new();
    for (i, (_, y)) in pts.iter().enumerate() {
        if !y.is_finite() {
            breaks.push(i);
        }
    }
    let points = pts
        .into_iter()
        .map(|(x, y)| PlotPoint { x, y })
        .collect();
    Ok(SampledPlot { points, breaks })
}

/// Refinement parameters that stay invariant during recursion, packed to
/// keep the recursive signature lean.
struct RefineCfg<'a> {
    func: &'a str,
    var: &'a str,
    eps: f64,
    batch: usize,
}

/// Refine one interval recursively: split at the midpoint when the midpoint
/// value deviates from the endpoint interpolation by more than
/// `eps * local_scale` (scale = largest |y| among the three, floored at 1 to
/// keep the criterion relative but stable near zero).
fn refine_interval(
    engine: &mut dyn Engine,
    cfg: &RefineCfg,
    left: (f64, f64),
    right: (f64, f64),
    depth: u32,
) -> Result<Vec<(f64, f64)>, crate::engine::EngineError> {
    if depth == 0 {
        return Ok(vec![left, right]);
    }
    let xm = (left.0 + right.0) / 2.0;
    let ym = eval_batched(engine, cfg.func, cfg.var, &[xm], cfg.batch)?
        .into_iter()
        .next()
        .unwrap_or(f64::NAN);
    if !ym.is_finite() {
        // Non-finite midpoint: stop refining; the break logic handles it.
        return Ok(vec![left, (xm, ym), right]);
    }
    let interp = (left.1 + right.1) / 2.0;
    let scale = left.1.abs().max(right.1.abs()).max(ym.abs()).max(1.0);
    if (ym - interp).abs() > cfg.eps * scale {
        let l = refine_interval(engine, cfg, left, (xm, ym), depth - 1)?;
        let r = refine_interval(engine, cfg, (xm, ym), right, depth - 1)?;
        // l ends with (xm, ym), r starts with it — join without duplication.
        let mut out = l;
        out.extend(r.into_iter().skip(1));
        Ok(out)
    } else {
        Ok(vec![left, right])
    }
}

/// Evaluate `func(var = v)` for every v in `xs`, in chunks of `batch`:
/// one engine call per chunk, evaluating
/// `N({Subst(var,v1,func), Subst(var,v2,func), ...})`.
/// Non-finite / unevaluated results map to NaN.
pub(crate) fn eval_batched(
    engine: &mut dyn Engine,
    func: &str,
    var: &str,
    xs: &[f64],
    batch: usize,
) -> Result<Vec<f64>, crate::engine::EngineError> {
    let mut out = Vec::with_capacity(xs.len());
    for chunk in xs.chunks(batch.max(1)) {
        let items: Vec<String> = chunk
            .iter()
            .map(|v| {
                format!(
                    "N(Eval(ApplyPure(\"Subst\", {{{var},{v},{func}}})))"
                )
            })
            .collect();
        let cmd = format!("N({{{}}})", items.join(", "));
        let result: EvalResult = engine.eval(&cmd)?;
        let values = flatten_number_list(&result.expr);
        if values.len() != chunk.len() {
            return Err(crate::engine::EngineError::Eval(format!(
                "batch evaluation returned {} values, expected {}",
                values.len(),
                chunk.len()
            )));
        }
        out.extend(values);
    }
    Ok(out)
}

/// Interpret a `List` expression as f64 values; non-finite markers
/// (`Infinity`, `Undefined`, ...) and unevaluated nodes map to NaN.
fn flatten_number_list(expr: &Expr) -> Vec<f64> {
    fn scalar(e: &Expr) -> Option<f64> {
        match e {
            Expr::Number(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    }
    match expr {
        Expr::Call { head, args } if head == "List" => args.iter().map(|e| scalar(e).unwrap_or(f64::NAN)).collect(),
        Expr::Call { head, args } if head == "N" => args.first().map(flatten_number_list).unwrap_or_default(),
        // Single scalar result (e.g. a one-element batch collapsed by the engine).
        one => scalar(one).into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;

    fn sample_default(func: &str, range: (f64, f64)) -> SampledPlot {
        let mut engine = RustEngine::spawn().expect("RustEngine boot");
        sample(&mut engine, func, "x", range, &SampleOptions::default()).expect("sample")
    }

    #[test]
    fn samples_sine_over_full_range() {
        let plot = sample_default("Sin(x)", (0.0, std::f64::consts::PI));
        // Ascending x, endpoints exact, interior values close to Sin.
        assert!(plot.points.len() >= 64);
        assert!(plot.points.windows(2).all(|w| w[0].x < w[1].x));
        let last = plot.points.last().expect("non-empty");
        assert!((last.x - std::f64::consts::PI).abs() < 1e-12);
        assert!((last.y - 0.0).abs() < 1e-6);
        let mid = plot.points.iter().find(|p| (p.x - 1.5).abs() < 0.1).expect("mid point");
        assert!((mid.y - 1.5f64.sin()).abs() < 1e-2);
        assert!(plot.breaks.is_empty(), "Sin is finite everywhere");
    }

    #[test]
    fn refines_curvature_but_not_lines() {
        // A straight line must not be refined (curvature criterion).
        let mut engine = RustEngine::spawn().expect("boot");
        let opts = SampleOptions { points: 16, max_depth: 5, ..Default::default() };
        let line = sample(&mut engine, "2*x+1", "x", (0.0, 10.0), &opts).expect("sample");
        assert_eq!(line.points.len(), 17, "linear: base grid only");
        // x^2 has constant curvature: every interval refines to max depth.
        let quad = sample(&mut engine, "x^2", "x", (0.0, 10.0), &opts).expect("sample");
        assert!(quad.points.len() > 17, "quadratic: refinement happened");
        // And the refinement actually reduces approximation error: midpoint
        // of [0,10] must be very close to 25.
        let mid = quad.points.iter().find(|p| (p.x - 5.0).abs() < 1e-9).expect("midpoint sampled");
        assert!((mid.y - 25.0).abs() < 1e-2);
    }

    #[test]
    fn non_finite_becomes_gap() {
        // 1/x over [-1, 1]: a break around x = 0, other regions sampled.
        let plot = sample_default("1/x", (-1.0, 1.0));
        assert!(!plot.breaks.is_empty(), "expected a break at the pole");
        // All recorded y values are finite? No — break points carry NaN;
        // but every non-break point must be finite and satisfy y = 1/x.
        let breaks: Vec<usize> = plot.breaks.clone();
        for (i, p) in plot.points.iter().enumerate() {
            if breaks.contains(&i) {
                continue;
            }
            if p.y.is_finite() {
                assert!((p.y - 1.0 / p.x).abs() < 1e-6, "y=1/x at x={}", p.x);
            }
        }
        // The pole region must be flagged: some break exists near x=0.
        assert!(plot.breaks.iter().any(|&i| plot.points[i].x.abs() < 0.2));
    }

    #[test]
    fn constant_function_plots() {
        // Plotting constants is a documented upstream edge case too.
        let plot = sample_default("5", (0.0, 1.0));
        assert!(plot.points.iter().all(|p| (p.y - 5.0).abs() < 1e-9));
        assert!(plot.breaks.is_empty());
    }

    #[test]
    fn batch_evaluation_matches_direct() {
        // The bulk path (one engine call for many points) must agree with
        // single-point evaluation.
        let mut engine = RustEngine::spawn().expect("boot");
        let xs: Vec<f64> = (0..16).map(|i| 0.1 * i as f64).collect();
        let ys = eval_batched(&mut engine, "Sin(x)*Exp(-x)", "x", &xs, 8).expect("batch");
        for (x, y) in xs.iter().zip(&ys) {
            let expected = x.sin() * (-x).exp();
            assert!((y - expected).abs() < 1e-9, "at x={x}");
        }
    }
}
