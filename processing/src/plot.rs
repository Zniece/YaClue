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

use crate::engine::{Engine, EngineError};
use crate::input::{validate_expression, validate_symbol};
use crate::numeric::evaluate_real_batch;

pub const MAX_BASE_INTERVALS: usize = 4_096;
pub const MAX_REFINEMENT_DEPTH: u32 = 12;
pub const MAX_BATCH_SIZE: usize = 4_096;
pub const MAX_SAMPLED_POINTS: usize = 100_000;

/// One sampled point.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct PlotPoint {
    pub x: f64,
    pub y: f64,
}

/// Inclusive point-index range containing one continuous finite polyline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PlotSegment {
    pub start_index: usize,
    pub end_index: usize,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct PlotBounds {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleTermination {
    Complete,
    RefinementLimit,
    PointLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlotBreakKind {
    NonFinite,
    SuspectedVerticalAsymptote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PlotBreak {
    /// A renderer must not connect this point to the following point.
    pub after_index: usize,
    pub kind: PlotBreakKind,
}

/// A sampled curve: points in ascending x order, plus the indices (into
/// `points`) after which a renderer must NOT draw a connecting line
/// (non-finite values on either side of the break).
#[derive(Debug, Clone, Serialize)]
pub struct SampledPlot {
    pub points: Vec<PlotPoint>,
    pub breaks: Vec<usize>,
    pub discontinuities: Vec<PlotBreak>,
    pub segments: Vec<PlotSegment>,
    pub suggested_bounds: Option<PlotBounds>,
    pub evaluations: usize,
    pub termination: SampleTermination,
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
    /// Maximum number of points evaluated by this request.
    pub max_points: usize,
}

impl Default for SampleOptions {
    fn default() -> Self {
        SampleOptions {
            points: 64,
            max_depth: 5,
            eps: 1e-3,
            batch: 64,
            max_points: 10_000,
        }
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
) -> Result<SampledPlot, EngineError> {
    validate_expression(func, "绘图表达式")?;
    validate_symbol(var, "绘图变量")?;
    let (a, b) = range;
    if !(a.is_finite() && b.is_finite() && b > a && (b - a).is_finite()) {
        return Err(EngineError::InvalidInput(format!(
            "invalid range ({a}, {b})"
        )));
    }
    validate_options(options)?;

    // Base grid: (points + 1) nodes.
    let n = options.points;
    let dx = (b - a) / n as f64;
    let xs: Vec<f64> = (0..=n).map(|i| a + dx * i as f64).collect();
    let ys = evaluate_real_batch(engine, func, var, &xs, options.batch)?;

    // Refine all intervals at the same depth together. Midpoints are evaluated
    // in batches, avoiding hundreds of interpreter round trips on wide ranges.
    let base: Vec<(f64, f64)> = xs.into_iter().zip(ys).collect();
    let intervals: Vec<_> = base.windows(2).map(|pair| (pair[0], pair[1])).collect();
    let refined = refine_intervals(engine, func, var, intervals, options, n + 1)?;

    let points: Vec<_> = refined
        .points
        .into_iter()
        .map(|(x, y)| PlotPoint { x, y })
        .collect();
    let discontinuities = detect_breaks(&points);
    let breaks = discontinuities
        .iter()
        .map(|discontinuity| discontinuity.after_index)
        .collect();
    let segments = finite_segments(&points, &discontinuities);
    let suggested_bounds = suggested_bounds(&points, range);
    Ok(SampledPlot {
        points,
        breaks,
        discontinuities,
        segments,
        suggested_bounds,
        evaluations: refined.evaluations,
        termination: refined.termination,
    })
}

fn validate_options(options: &SampleOptions) -> Result<(), EngineError> {
    if !(2..=MAX_BASE_INTERVALS).contains(&options.points) {
        return Err(EngineError::InvalidInput(format!(
            "points must be between 2 and {MAX_BASE_INTERVALS}"
        )));
    }
    if options.max_depth > MAX_REFINEMENT_DEPTH {
        return Err(EngineError::InvalidInput(format!(
            "max_depth must be <= {MAX_REFINEMENT_DEPTH}"
        )));
    }
    if !options.eps.is_finite() || options.eps <= 0.0 {
        return Err(EngineError::InvalidInput(
            "eps must be a finite positive number".into(),
        ));
    }
    if !(1..=MAX_BATCH_SIZE).contains(&options.batch) {
        return Err(EngineError::InvalidInput(format!(
            "batch must be between 1 and {MAX_BATCH_SIZE}"
        )));
    }
    if options.max_points > MAX_SAMPLED_POINTS || options.max_points < options.points + 1 {
        return Err(EngineError::InvalidInput(format!(
            "max_points must cover the base grid and be <= {MAX_SAMPLED_POINTS}"
        )));
    }
    Ok(())
}

type Interval = ((f64, f64), (f64, f64));

struct RefinedPlot {
    points: Vec<(f64, f64)>,
    termination: SampleTermination,
    evaluations: usize,
}

fn refine_intervals(
    engine: &mut dyn Engine,
    func: &str,
    var: &str,
    mut active: Vec<Interval>,
    options: &SampleOptions,
    mut evaluations: usize,
) -> Result<RefinedPlot, EngineError> {
    let mut leaves = Vec::new();
    let mut termination = SampleTermination::Complete;
    for _ in 0..options.max_depth {
        if active.is_empty() {
            break;
        }
        if active.len() > options.max_points - evaluations {
            termination = SampleTermination::PointLimit;
            break;
        }
        let midpoints: Vec<_> = active
            .iter()
            .map(|(left, right)| (left.0 + right.0) / 2.0)
            .collect();
        let values = evaluate_real_batch(engine, func, var, &midpoints, options.batch)?;
        evaluations += midpoints.len();
        let mut next = Vec::new();
        for (((left, right), x), y) in active.into_iter().zip(midpoints).zip(values) {
            let middle = (x, y);
            let interpolation = (left.1 + right.1) / 2.0;
            let scale = left.1.abs().max(right.1.abs()).max(y.abs()).max(1.0);
            if y.is_finite() && (y - interpolation).abs() > options.eps * scale {
                next.push((left, middle));
                next.push((middle, right));
            } else if y.is_finite() {
                leaves.push((left, right));
            } else {
                leaves.push((left, middle));
                leaves.push((middle, right));
            }
        }
        active = next;
    }
    if !active.is_empty() && termination == SampleTermination::Complete {
        termination = SampleTermination::RefinementLimit;
    }
    leaves.extend(active);
    leaves.sort_by(|a, b| a.0 .0.total_cmp(&b.0 .0));
    let Some(first) = leaves.first() else {
        return Ok(RefinedPlot {
            points: Vec::new(),
            termination,
            evaluations,
        });
    };
    let mut points = vec![first.0];
    points.extend(leaves.into_iter().map(|interval| interval.1));
    Ok(RefinedPlot {
        points,
        termination,
        evaluations,
    })
}

fn detect_breaks(points: &[PlotPoint]) -> Vec<PlotBreak> {
    let typical_scale = typical_y_scale(points);
    points
        .windows(2)
        .enumerate()
        .filter_map(|(index, pair)| {
            let left = pair[0].y;
            let right = pair[1].y;
            let kind = if !(left.is_finite() && right.is_finite()) {
                PlotBreakKind::NonFinite
            } else if left.is_sign_positive() != right.is_sign_positive()
                && left.abs().min(right.abs()) > typical_scale * 16.0
            {
                PlotBreakKind::SuspectedVerticalAsymptote
            } else {
                return None;
            };
            Some(PlotBreak {
                after_index: index,
                kind,
            })
        })
        .collect()
}

fn typical_y_scale(points: &[PlotPoint]) -> f64 {
    let mut magnitudes: Vec<_> = points
        .iter()
        .map(|point| point.y.abs())
        .filter(|value| value.is_finite())
        .collect();
    if magnitudes.is_empty() {
        return 1.0;
    }
    magnitudes.sort_by(f64::total_cmp);
    magnitudes[magnitudes.len() / 2].max(1.0)
}

fn finite_segments(points: &[PlotPoint], discontinuities: &[PlotBreak]) -> Vec<PlotSegment> {
    let mut segments = Vec::new();
    let mut start = None;
    let mut breaks = discontinuities.iter().peekable();
    for (index, point) in points.iter().enumerate() {
        let split_before = index > 0
            && breaks
                .peek()
                .is_some_and(|discontinuity| discontinuity.after_index == index - 1);
        if split_before {
            if let Some(first) = start.take() {
                segments.push(PlotSegment {
                    start_index: first,
                    end_index: index - 1,
                });
            }
            breaks.next();
        }
        match (start, point.y.is_finite()) {
            (None, true) => start = Some(index),
            (Some(first), false) => {
                segments.push(PlotSegment {
                    start_index: first,
                    end_index: index - 1,
                });
                start = None;
            }
            _ => {}
        }
    }
    if let Some(first) = start {
        segments.push(PlotSegment {
            start_index: first,
            end_index: points.len() - 1,
        });
    }
    segments
}

fn suggested_bounds(points: &[PlotPoint], range: (f64, f64)) -> Option<PlotBounds> {
    let (low, high) = robust_y_range(points)?;
    let padding = if high > low {
        (high - low) * 0.08
    } else {
        high.abs().mul_add(0.05, 1.0).max(1.0)
    };
    Some(PlotBounds {
        x_min: range.0,
        x_max: range.1,
        y_min: low - padding,
        y_max: high + padding,
    })
}

fn robust_y_range(points: &[PlotPoint]) -> Option<(f64, f64)> {
    let mut ys: Vec<_> = points
        .iter()
        .map(|point| point.y)
        .filter(|value| value.is_finite())
        .collect();
    if ys.is_empty() {
        return None;
    }
    ys.sort_by(f64::total_cmp);
    let last = ys.len() - 1;
    let low = ys[last.saturating_mul(2) / 100];
    let high = ys[(last.saturating_mul(98) / 100).min(last)];
    Some((low, high))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{EvalResult, RustEngine};

    struct CountingEngine {
        inner: RustEngine,
        calls: usize,
    }

    impl Engine for CountingEngine {
        fn eval(&mut self, command: &str) -> Result<EvalResult, EngineError> {
            self.calls += 1;
            self.inner.eval(command)
        }
    }

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
        let mid = plot
            .points
            .iter()
            .find(|p| (p.x - 1.5).abs() < 0.1)
            .expect("mid point");
        assert!((mid.y - 1.5f64.sin()).abs() < 1e-2);
        assert!(plot.breaks.is_empty(), "Sin is finite everywhere");
        assert_eq!(
            plot.segments,
            vec![PlotSegment {
                start_index: 0,
                end_index: plot.points.len() - 1,
            }]
        );
        assert!(plot.suggested_bounds.is_some());
        assert_eq!(plot.termination, SampleTermination::Complete);
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn samples_sine_over_playground_default_range() {
        let plot = sample_default("Sin(x)", (-6.28, 6.28));
        assert!(plot.points.len() >= 65);
        assert!(plot.breaks.is_empty());
        assert!(plot.points.windows(2).all(|pair| pair[0].x < pair[1].x));
    }

    #[test]
    fn evaluates_negative_trigonometric_samples() {
        let mut engine = RustEngine::spawn().expect("boot");
        for function in ["Sin", "Cos", "Tan"] {
            let value = engine
                .eval(&format!("N({function}(-0.58875))"))
                .expect("negative trigonometric sample");
            assert!(value.expr.to_string().parse::<f64>().unwrap().is_finite());
        }
    }

    #[test]
    fn refines_curvature_but_not_lines() {
        // A straight line must not be refined (curvature criterion).
        let mut engine = RustEngine::spawn().expect("boot");
        let opts = SampleOptions {
            points: 16,
            max_depth: 5,
            ..Default::default()
        };
        let line = sample(&mut engine, "2*x+1", "x", (0.0, 10.0), &opts).expect("sample");
        assert_eq!(line.points.len(), 17, "linear: base grid only");
        // x^2 has constant curvature: every interval refines to max depth.
        let quad = sample(&mut engine, "x^2", "x", (0.0, 10.0), &opts).expect("sample");
        assert!(quad.points.len() > 17, "quadratic: refinement happened");
        // And the refinement actually reduces approximation error: midpoint
        // of [0,10] must be very close to 25.
        let mid = quad
            .points
            .iter()
            .find(|p| (p.x - 5.0).abs() < 1e-9)
            .expect("midpoint sampled");
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
        assert_eq!(plot.segments.len(), 2);
        assert!(plot
            .discontinuities
            .iter()
            .all(|item| item.kind == PlotBreakKind::NonFinite));
        for segment in &plot.segments {
            assert!(plot.points[segment.start_index..=segment.end_index]
                .iter()
                .all(|point| point.y.is_finite()));
        }
    }

    #[test]
    fn constant_function_plots() {
        // Plotting constants is a documented upstream edge case too.
        let plot = sample_default("5", (0.0, 1.0));
        assert!(plot.points.iter().all(|p| (p.y - 5.0).abs() < 1e-9));
        assert!(plot.breaks.is_empty());
        let bounds = plot.suggested_bounds.expect("constant axis bounds");
        assert!(bounds.y_min < 5.0 && bounds.y_max > 5.0);
    }

    #[test]
    fn reports_resource_limits_without_exceeding_the_point_budget() {
        let mut engine = RustEngine::spawn().expect("boot");
        let point_limited = sample(
            &mut engine,
            "x^2",
            "x",
            (0.0, 1.0),
            &SampleOptions {
                points: 4,
                max_depth: 5,
                eps: 1e-12,
                batch: 4,
                max_points: 5,
            },
        )
        .unwrap();
        assert_eq!(point_limited.termination, SampleTermination::PointLimit);
        assert_eq!(point_limited.evaluations, 5);
        assert_eq!(point_limited.points.len(), 5);

        let depth_limited = sample(
            &mut engine,
            "x^2",
            "x",
            (0.0, 1.0),
            &SampleOptions {
                points: 4,
                max_depth: 1,
                eps: 1e-12,
                batch: 4,
                max_points: 100,
            },
        )
        .unwrap();
        assert_eq!(
            depth_limited.termination,
            SampleTermination::RefinementLimit
        );
        assert_eq!(depth_limited.evaluations, 9);
    }

    #[test]
    fn validates_public_inputs_and_options() {
        let mut engine = RustEngine::spawn().expect("boot");
        assert!(sample(
            &mut engine,
            "x); Echo(1); (x",
            "x",
            (0.0, 1.0),
            &SampleOptions::default()
        )
        .is_err());
        assert!(sample(
            &mut engine,
            "x",
            "x",
            (0.0, 1.0),
            &SampleOptions {
                max_depth: MAX_REFINEMENT_DEPTH + 1,
                ..Default::default()
            }
        )
        .is_err());
        assert!(sample(&mut engine, "x", "x", (0.0, 1.0), &SampleOptions::default()).is_ok());
    }

    #[test]
    fn evaluates_grids_in_batches_instead_of_per_point() {
        let mut engine = CountingEngine {
            inner: RustEngine::spawn().expect("boot"),
            calls: 0,
        };
        let plot = sample(
            &mut engine,
            "2*x+1",
            "x",
            (0.0, 1.0),
            &SampleOptions {
                points: 64,
                batch: 64,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(plot.evaluations, 129);
        assert_eq!(
            engine.calls, 3,
            "65 base points and 64 midpoints need 3 batches"
        );
    }

    #[test]
    fn separates_sampled_vertical_asymptotes_conservatively() {
        let tan = sample_default("Tan(x)", (-3.0, 3.0));
        let suspected: Vec<_> = tan
            .discontinuities
            .iter()
            .filter(|item| item.kind == PlotBreakKind::SuspectedVerticalAsymptote)
            .collect();
        assert_eq!(suspected.len(), 2, "Tan has poles at ±Pi/2: {suspected:?}");
        assert_eq!(tan.segments.len(), 3);

        let steep_line = sample_default("100000*x", (-1.0, 1.0));
        assert!(steep_line.discontinuities.is_empty());
        assert_eq!(steep_line.segments.len(), 1);
    }
}
