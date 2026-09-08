//! Bounded numerical initial-value integration for ODEs without a symbolic solution.

use crate::engine::{Engine, EngineError, ErrorResponse, Expr};
use crate::input::{validate_expression, validate_symbol};
use crate::ode::{self, InitialCondition, OdeResult, OdeStatus, MAX_ODE_ORDER};
use serde::Serialize;

pub const MAX_NUMERIC_ODE_STEPS: usize = 20_000;
pub const MAX_NUMERIC_ODE_EVALUATIONS: usize = 150_000;

#[derive(Debug, Clone, Copy)]
pub struct NumericOdeOptions {
    pub end: f64,
    pub initial_step: f64,
    pub absolute_tolerance: f64,
    pub relative_tolerance: f64,
    pub max_steps: usize,
    pub max_evaluations: usize,
}

impl Default for NumericOdeOptions {
    fn default() -> Self {
        Self {
            end: 1.0,
            initial_step: 0.01,
            absolute_tolerance: 1e-9,
            relative_tolerance: 1e-7,
            max_steps: 10_000,
            max_evaluations: 100_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NumericOdeStatus {
    Completed,
    Unresolved,
    NonFinite,
    StepLimit,
    EvaluationLimit,
    StepUnderflow,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct NumericOdePoint {
    pub independent: f64,
    /// Function value followed by derivatives through order `order - 1`.
    pub state: Vec<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NumericOdeResult {
    pub status: NumericOdeStatus,
    pub order: u32,
    pub points: Vec<NumericOdePoint>,
    pub accepted_steps: usize,
    pub rejected_steps: usize,
    pub evaluations: usize,
    pub estimated_error: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OdeInitialValueMode {
    Symbolic,
    Numeric,
}

#[derive(Debug, Clone, Serialize)]
pub struct OdeInitialValueResult {
    pub mode: OdeInitialValueMode,
    pub symbolic: Option<OdeResult>,
    pub numeric: Option<NumericOdeResult>,
    pub symbolic_error: Option<ErrorResponse>,
}

/// Prefer an analytic solution and use the bounded numerical integrator only
/// when the symbolic solver returns an unresolved result.
pub fn solve_with_numeric_fallback(
    engine: &mut dyn Engine,
    equation: &str,
    independent: &str,
    dependent: &str,
    initial_conditions: &[InitialCondition<'_>],
    options: NumericOdeOptions,
) -> Result<OdeInitialValueResult, EngineError> {
    let symbolic = match ode::solve(engine, equation, independent, dependent, initial_conditions) {
        Ok(symbolic) if symbolic.status == OdeStatus::Solved => {
            return Ok(OdeInitialValueResult {
                mode: OdeInitialValueMode::Symbolic,
                symbolic: Some(symbolic),
                numeric: None,
                symbolic_error: None,
            });
        }
        Ok(symbolic) => Some(symbolic),
        Err(error @ (EngineError::Eval(_) | EngineError::Timeout(_))) => {
            let symbolic_error = error.response();
            let numeric = solve_initial_value(
                engine,
                equation,
                independent,
                dependent,
                initial_conditions,
                options,
            )?;
            return Ok(OdeInitialValueResult {
                mode: OdeInitialValueMode::Numeric,
                symbolic: None,
                numeric: Some(numeric),
                symbolic_error: Some(symbolic_error),
            });
        }
        Err(error) => return Err(error),
    };
    let numeric = solve_initial_value(
        engine,
        equation,
        independent,
        dependent,
        initial_conditions,
        options,
    )?;
    Ok(OdeInitialValueResult {
        mode: OdeInitialValueMode::Numeric,
        symbolic,
        numeric: Some(numeric),
        symbolic_error: None,
    })
}

pub fn solve_initial_value(
    engine: &mut dyn Engine,
    equation: &str,
    independent: &str,
    dependent: &str,
    initial_conditions: &[InitialCondition<'_>],
    options: NumericOdeOptions,
) -> Result<NumericOdeResult, EngineError> {
    solve_initial_value_with_cancel(
        engine,
        equation,
        independent,
        dependent,
        initial_conditions,
        options,
        &|| false,
    )
}

pub fn solve_initial_value_with_cancel(
    engine: &mut dyn Engine,
    equation: &str,
    independent: &str,
    dependent: &str,
    initial_conditions: &[InitialCondition<'_>],
    options: NumericOdeOptions,
    cancelled: &dyn Fn() -> bool,
) -> Result<NumericOdeResult, EngineError> {
    validate_expression(equation, "微分方程")?;
    validate_symbol(independent, "自变量")?;
    validate_symbol(dependent, "因变量")?;
    validate_options(options)?;
    let order = ode::equation_order(equation, dependent)?;
    if order == 0 || order > MAX_ODE_ORDER {
        return Err(EngineError::InvalidInput(format!(
            "数值 ODE 当前支持 1..={MAX_ODE_ORDER} 阶方程"
        )));
    }
    let (start, state) = numeric_initial_state(engine, initial_conditions, order)?;
    let canonical = ode::to_canonical(equation, independent, dependent, order);
    let derivative = isolate_derivative(engine, &canonical, order)?;
    let Some(derivative) = derivative else {
        return Ok(empty_result(
            NumericOdeStatus::Unresolved,
            order,
            start,
            state,
        ));
    };
    if !supports_numeric_expression(&derivative, order) {
        return Ok(empty_result(
            NumericOdeStatus::Unresolved,
            order,
            start,
            state,
        ));
    }
    integrate(&derivative, order, start, state, options, cancelled)
}

fn validate_options(options: NumericOdeOptions) -> Result<(), EngineError> {
    if !options.end.is_finite()
        || !options.initial_step.is_finite()
        || options.initial_step <= 0.0
        || !options.absolute_tolerance.is_finite()
        || options.absolute_tolerance <= 0.0
        || !options.relative_tolerance.is_finite()
        || options.relative_tolerance <= 0.0
        || options.max_steps == 0
        || options.max_steps > MAX_NUMERIC_ODE_STEPS
        || options.max_evaluations == 0
        || options.max_evaluations > MAX_NUMERIC_ODE_EVALUATIONS
    {
        return Err(EngineError::InvalidInput(
            "数值 ODE 的区间、容差、步长或资源上限无效".into(),
        ));
    }
    Ok(())
}

fn numeric_initial_state(
    engine: &mut dyn Engine,
    conditions: &[InitialCondition<'_>],
    order: u32,
) -> Result<(f64, Vec<f64>), EngineError> {
    if conditions.len() != order as usize {
        return Err(EngineError::InvalidInput(format!(
            "{order} 阶数值初值问题需要 {order} 个初值条件"
        )));
    }
    let point = conditions[0].point.trim();
    let mut values = vec![None; order as usize];
    for condition in conditions {
        validate_expression(condition.point, "初值点")?;
        validate_expression(condition.value, "初值")?;
        if condition.point.trim() != point || condition.derivative_order >= order {
            return Err(EngineError::InvalidInput(
                "数值初值必须位于同一点并覆盖从零开始的各阶状态".into(),
            ));
        }
        let slot = &mut values[condition.derivative_order as usize];
        if slot.is_some() {
            return Err(EngineError::InvalidInput("数值初值阶数不能重复".into()));
        }
        *slot = Some(condition.value.trim());
    }
    let mut expressions = vec![point];
    expressions.extend(values.into_iter().map(|value| value.expect("complete")));
    let evaluated = engine.eval_expr(&format!("N({{{}}})", expressions.join(",")))?;
    let Expr::Call { head, args } = evaluated else {
        return Err(EngineError::Eval("数值初值未求值为实数列表".into()));
    };
    if head != "List" || args.len() != order as usize + 1 {
        return Err(EngineError::Eval("数值初值未求值为实数列表".into()));
    }
    let numbers: Option<Vec<f64>> = args.iter().map(real_number).collect();
    let numbers =
        numbers.ok_or_else(|| EngineError::InvalidInput("数值初值必须是有限实数".into()))?;
    Ok((numbers[0], numbers[1..].to_vec()))
}

fn isolate_derivative(
    engine: &mut dyn Engine,
    canonical: &str,
    order: u32,
) -> Result<Option<Expr>, EngineError> {
    let derivative = format!("y{}", "'".repeat(order as usize));
    let solved = engine.eval_expr(&format!("Solve({canonical},{derivative})"))?;
    let Expr::Call { head, args } = solved else {
        return Ok(None);
    };
    if head != "List" || args.len() != 1 {
        return Ok(None);
    }
    match &args[0] {
        Expr::Call { head, args }
            if (head == "=" || head == "==")
                && args.len() == 2
                && args[0] == Expr::Symbol(derivative) =>
        {
            Ok(Some(args[1].clone()))
        }
        _ => Ok(None),
    }
}

fn real_number(expr: &Expr) -> Option<f64> {
    let value: f64 = match expr {
        Expr::Number(value) => value.parse().ok()?,
        _ => return None,
    };
    value.is_finite().then_some(value)
}

fn eval_numeric(expr: &Expr, x: f64, state: &[f64]) -> Option<f64> {
    let value = match expr {
        Expr::Number(value) => value.parse().ok()?,
        Expr::Symbol(symbol) => match symbol.as_str() {
            "x" => x,
            "y" => state[0],
            "y'" => *state.get(1)?,
            "Pi" => std::f64::consts::PI,
            "E" => std::f64::consts::E,
            _ => return None,
        },
        Expr::Call { head, args } => {
            let values: Option<Vec<f64>> =
                args.iter().map(|arg| eval_numeric(arg, x, state)).collect();
            let values = values?;
            match (head.as_str(), values.as_slice()) {
                ("+", values) => values.iter().sum(),
                ("*", values) => values.iter().product(),
                ("-", [one]) => -one,
                ("-", [left, right]) => left - right,
                ("/", [left, right]) => left / right,
                ("^", [base, exponent]) => base.powf(*exponent),
                ("Sin", [value]) => value.sin(),
                ("Cos", [value]) => value.cos(),
                ("Tan", [value]) => value.tan(),
                ("Exp", [value]) => value.exp(),
                ("Ln" | "Log", [value]) => value.ln(),
                ("Sqrt", [value]) => value.sqrt(),
                ("Abs", [value]) => value.abs(),
                _ => return None,
            }
        }
    };
    value.is_finite().then_some(value)
}

fn supports_numeric_expression(expr: &Expr, order: u32) -> bool {
    match expr {
        Expr::Number(_) => true,
        Expr::Symbol(symbol) => {
            matches!(symbol.as_str(), "x" | "y" | "Pi" | "E") || (order == 2 && symbol == "y'")
        }
        Expr::Call { head, args } => {
            let arity_ok = match head.as_str() {
                "+" | "*" => !args.is_empty(),
                "-" => matches!(args.len(), 1 | 2),
                "/" | "^" => args.len() == 2,
                "Sin" | "Cos" | "Tan" | "Exp" | "Ln" | "Log" | "Sqrt" | "Abs" => args.len() == 1,
                _ => false,
            };
            arity_ok
                && args
                    .iter()
                    .all(|arg| supports_numeric_expression(arg, order))
        }
    }
}

fn derivative(expr: &Expr, order: u32, x: f64, state: &[f64]) -> Option<Vec<f64>> {
    let highest = eval_numeric(expr, x, state)?;
    Some(if order == 1 {
        vec![highest]
    } else {
        vec![state[1], highest]
    })
}

fn integrate(
    expr: &Expr,
    order: u32,
    start: f64,
    initial: Vec<f64>,
    options: NumericOdeOptions,
    cancelled: &dyn Fn() -> bool,
) -> Result<NumericOdeResult, EngineError> {
    let mut result = empty_result(NumericOdeStatus::Completed, order, start, initial.clone());
    if start == options.end {
        return Ok(result);
    }
    let direction = (options.end - start).signum();
    let mut h = options.initial_step.min((options.end - start).abs()) * direction;
    let mut x = start;
    let mut y = initial;
    let minimum_step = f64::EPSILON.sqrt() * start.abs().max(options.end.abs()).max(1.0);
    while direction * (options.end - x) > 0.0 {
        if cancelled() {
            result.status = NumericOdeStatus::Cancelled;
            break;
        }
        if result.accepted_steps + result.rejected_steps >= options.max_steps {
            result.status = NumericOdeStatus::StepLimit;
            break;
        }
        if result.evaluations + 7 > options.max_evaluations {
            result.status = NumericOdeStatus::EvaluationLimit;
            break;
        }
        if h.abs() < minimum_step {
            result.status = NumericOdeStatus::StepUnderflow;
            break;
        }
        if direction * (x + h - options.end) > 0.0 {
            h = options.end - x;
        }
        let Some((next, error)) = dopri54_step(expr, order, x, &y, h, &mut result.evaluations)
        else {
            result.status = NumericOdeStatus::NonFinite;
            break;
        };
        let norm = error
            .iter()
            .zip(y.iter().zip(&next))
            .map(|(error, (old, new))| {
                let scale = options.absolute_tolerance
                    + options.relative_tolerance * old.abs().max(new.abs());
                (error / scale).powi(2)
            })
            .sum::<f64>()
            / order as f64;
        let norm = norm.sqrt();
        result.estimated_error = error
            .iter()
            .fold(0.0_f64, |max, value| max.max(value.abs()));
        let factor = if norm == 0.0 {
            5.0
        } else {
            (0.9 * norm.powf(-0.2)).clamp(0.2, 5.0)
        };
        if norm <= 1.0 {
            x += h;
            y = next;
            result.accepted_steps += 1;
            result.points.push(NumericOdePoint {
                independent: x,
                state: y.clone(),
            });
        } else {
            result.rejected_steps += 1;
        }
        h *= factor;
    }
    Ok(result)
}

fn dopri54_step(
    expr: &Expr,
    order: u32,
    x: f64,
    y: &[f64],
    h: f64,
    evaluations: &mut usize,
) -> Option<(Vec<f64>, Vec<f64>)> {
    let mut ks: Vec<Vec<f64>> = Vec::with_capacity(7);
    let stages: &[(f64, &[f64])] = &[
        (0.0, &[]),
        (1.0 / 5.0, &[1.0 / 5.0]),
        (3.0 / 10.0, &[3.0 / 40.0, 9.0 / 40.0]),
        (4.0 / 5.0, &[44.0 / 45.0, -56.0 / 15.0, 32.0 / 9.0]),
        (
            8.0 / 9.0,
            &[
                19372.0 / 6561.0,
                -25360.0 / 2187.0,
                64448.0 / 6561.0,
                -212.0 / 729.0,
            ],
        ),
        (
            1.0,
            &[
                9017.0 / 3168.0,
                -355.0 / 33.0,
                46732.0 / 5247.0,
                49.0 / 176.0,
                -5103.0 / 18656.0,
            ],
        ),
        (
            1.0,
            &[
                35.0 / 384.0,
                0.0,
                500.0 / 1113.0,
                125.0 / 192.0,
                -2187.0 / 6784.0,
                11.0 / 84.0,
            ],
        ),
    ];
    for (c, coefficients) in stages {
        let mut stage = y.to_vec();
        for (coefficient, k) in coefficients.iter().zip(&ks) {
            for (value, derivative) in stage.iter_mut().zip(k) {
                *value += h * coefficient * derivative;
            }
        }
        ks.push(derivative(expr, order, x + c * h, &stage)?);
        *evaluations += 1;
    }
    let fifth = combine(
        y,
        h,
        &ks,
        &[
            35.0 / 384.0,
            0.0,
            500.0 / 1113.0,
            125.0 / 192.0,
            -2187.0 / 6784.0,
            11.0 / 84.0,
            0.0,
        ],
    );
    let fourth = combine(
        y,
        h,
        &ks,
        &[
            5179.0 / 57600.0,
            0.0,
            7571.0 / 16695.0,
            393.0 / 640.0,
            -92097.0 / 339200.0,
            187.0 / 2100.0,
            1.0 / 40.0,
        ],
    );
    let error = fifth
        .iter()
        .zip(fourth)
        .map(|(left, right)| left - right)
        .collect();
    Some((fifth, error))
}

fn combine(initial: &[f64], h: f64, ks: &[Vec<f64>], weights: &[f64]) -> Vec<f64> {
    let mut output = initial.to_vec();
    for (weight, k) in weights.iter().zip(ks) {
        for (value, derivative) in output.iter_mut().zip(k) {
            *value += h * weight * derivative;
        }
    }
    output
}

fn empty_result(
    status: NumericOdeStatus,
    order: u32,
    start: f64,
    state: Vec<f64>,
) -> NumericOdeResult {
    NumericOdeResult {
        status,
        order,
        points: vec![NumericOdePoint {
            independent: start,
            state,
        }],
        accepted_steps: 0,
        rejected_steps: 0,
        evaluations: 0,
        estimated_error: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;

    #[test]
    fn integrates_first_and_second_order_initial_value_problems() {
        let mut engine = RustEngine::spawn().unwrap();
        let first = solve_initial_value(
            &mut engine,
            "y'==y",
            "x",
            "y",
            &[InitialCondition {
                derivative_order: 0,
                point: "0",
                value: "1",
            }],
            NumericOdeOptions {
                end: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(first.status, NumericOdeStatus::Completed);
        assert!((first.points.last().unwrap().state[0] - std::f64::consts::E).abs() < 1e-6);

        let second = solve_initial_value(
            &mut engine,
            "y''+y==0",
            "x",
            "y",
            &[
                InitialCondition {
                    derivative_order: 0,
                    point: "0",
                    value: "0",
                },
                InitialCondition {
                    derivative_order: 1,
                    point: "0",
                    value: "1",
                },
            ],
            NumericOdeOptions {
                end: std::f64::consts::PI / 2.0,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(second.status, NumericOdeStatus::Completed);
        assert!((second.points.last().unwrap().state[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn reports_unsupported_rhs_limits_and_cancellation() {
        let mut engine = RustEngine::spawn().unwrap();
        let conditions = [InitialCondition {
            derivative_order: 0,
            point: "0",
            value: "1",
        }];
        let unsupported = solve_initial_value(
            &mut engine,
            "y'==Gamma(y)",
            "x",
            "y",
            &conditions,
            NumericOdeOptions::default(),
        )
        .unwrap();
        assert_eq!(unsupported.status, NumericOdeStatus::Unresolved);

        let cancelled = solve_initial_value_with_cancel(
            &mut engine,
            "y'==y",
            "x",
            "y",
            &conditions,
            NumericOdeOptions::default(),
            &|| true,
        )
        .unwrap();
        assert_eq!(cancelled.status, NumericOdeStatus::Cancelled);
        assert_eq!(cancelled.points.len(), 1);

        let limited = solve_initial_value(
            &mut engine,
            "y'==y",
            "x",
            "y",
            &conditions,
            NumericOdeOptions {
                max_evaluations: 1,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(limited.status, NumericOdeStatus::EvaluationLimit);
        assert_eq!(limited.evaluations, 0);
    }

    #[test]
    fn fallback_keeps_symbolic_results_distinct_from_numeric_trajectories() {
        let mut engine = RustEngine::spawn().unwrap();
        let symbolic = solve_with_numeric_fallback(
            &mut engine,
            "y'==y",
            "x",
            "y",
            &[InitialCondition {
                derivative_order: 0,
                point: "0",
                value: "1",
            }],
            NumericOdeOptions::default(),
        )
        .unwrap();
        assert_eq!(symbolic.mode, OdeInitialValueMode::Symbolic);
        assert!(symbolic.symbolic.is_some());
        assert!(symbolic.numeric.is_none());
        assert!(symbolic.symbolic_error.is_none());

        let numeric = solve_with_numeric_fallback(
            &mut engine,
            "y'==Sin(x*y)",
            "x",
            "y",
            &[InitialCondition {
                derivative_order: 0,
                point: "0",
                value: "1",
            }],
            NumericOdeOptions {
                end: 0.5,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(numeric.mode, OdeInitialValueMode::Numeric);
        assert!(numeric.symbolic.is_none());
        assert!(numeric.symbolic_error.is_some());
        assert_eq!(numeric.numeric.unwrap().status, NumericOdeStatus::Completed);
    }
}
