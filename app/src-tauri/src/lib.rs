use processing::algebra::{TransformKind, TransformResult};
use processing::assumptions::{AssumptionFact, AssumptionState};
use processing::engine::{Engine, EngineError, ErrorCode, ErrorResponse, RustEngineProxy};
use processing::equations::SolveResult;
use processing::limits::{LimitDirection, LimitResult};
use processing::linear_algebra::{MatrixOperation, MatrixResult};
use processing::numeric::{NumericResult, RootResult, TaylorResult};
use processing::ode::{InitialCondition, OdeResult, OdeStepResult};
use processing::ode_numeric::{NumericOdeOptions, NumericOdeResult};
use processing::plot::{SampleOptions, SampledPlot};
use processing::steps::{Step, StepVerbosity};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, MutexGuard};

fn lock_engine<'a>(
    state: &'a tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<MutexGuard<'a, RustEngineProxy>, ErrorResponse> {
    state.lock().map_err(|error| ErrorResponse {
        code: ErrorCode::Internal,
        message: format!("引擎状态锁不可用: {error}"),
        retryable: true,
    })
}

fn message(error: EngineError) -> ErrorResponse {
    error.response()
}

fn invalid_input(message: impl Into<String>) -> ErrorResponse {
    ErrorResponse {
        code: ErrorCode::InvalidInput,
        message: message.into(),
        retryable: false,
    }
}

fn parse_verbosity(value: &str) -> Result<StepVerbosity, ErrorResponse> {
    match value {
        "concise" => Ok(StepVerbosity::Concise),
        "standard" => Ok(StepVerbosity::Standard),
        "detailed" => Ok(StepVerbosity::Detailed),
        _ => Err(invalid_input(format!("未知步骤粒度: {value}"))),
    }
}

#[derive(Deserialize)]
struct StepRequest {
    kind: String,
    expr: String,
    variable: String,
    verbosity: String,
    order: Option<u32>,
    from: Option<String>,
    to: Option<String>,
}

#[tauri::command]
fn calculate_steps(
    request: StepRequest,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<Vec<Step>, ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    let verbosity = parse_verbosity(&request.verbosity)?;
    match request.kind.as_str() {
        "derivative" => processing::steps::derive_steps_order_with_verbosity(
            &mut *engine,
            &request.expr,
            &request.variable,
            request.order.unwrap_or(1),
            verbosity,
        ),
        "integral" => processing::steps::derive_integrals_with_verbosity(
            &mut *engine,
            &request.expr,
            &request.variable,
            verbosity,
        ),
        "definite" => processing::steps::derive_definite_with_verbosity(
            &mut *engine,
            &request.expr,
            &request.variable,
            request.from.as_deref().unwrap_or("0"),
            request.to.as_deref().unwrap_or("1"),
            verbosity,
        ),
        _ => return Err(invalid_input(format!("未知步骤运算: {}", request.kind))),
    }
    .map_err(message)
}

#[tauri::command]
fn transform_expression(
    expr: String,
    operation: String,
    variable: String,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<TransformResult, ErrorResponse> {
    let kind = match operation.as_str() {
        "simplify" => TransformKind::Simplify,
        "tidy" => TransformKind::Tidy,
        "expand" => TransformKind::Expand,
        "factor" => TransformKind::Factor,
        "apart" => TransformKind::Apart,
        _ => return Err(invalid_input(format!("未知代数变换: {operation}"))),
    };
    let mut engine = lock_engine(&engine)?;
    processing::algebra::transform(
        &mut *engine,
        &expr,
        kind,
        (kind == TransformKind::Apart).then_some(variable.as_str()),
    )
    .map_err(message)
}

#[tauri::command]
fn solve_equations(
    equations: Vec<String>,
    variables: Vec<String>,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<SolveResult, ErrorResponse> {
    let equations: Vec<_> = equations.iter().map(String::as_str).collect();
    let variables: Vec<_> = variables.iter().map(String::as_str).collect();
    let mut engine = lock_engine(&engine)?;
    processing::equations::solve(&mut *engine, &equations, &variables).map_err(message)
}

#[tauri::command]
fn calculate_limit(
    expr: String,
    variable: String,
    at: String,
    direction: String,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<LimitResult, ErrorResponse> {
    let direction = parse_limit_direction(&direction)?;
    let mut engine = lock_engine(&engine)?;
    processing::limits::limit(&mut *engine, &expr, &variable, &at, direction).map_err(message)
}

fn parse_limit_direction(direction: &str) -> Result<LimitDirection, ErrorResponse> {
    match direction {
        "both" => Ok(LimitDirection::Both),
        "left" => Ok(LimitDirection::Left),
        "right" => Ok(LimitDirection::Right),
        _ => Err(invalid_input(format!("未知极限方向: {direction}"))),
    }
}

#[tauri::command]
fn calculate_limit_steps(
    expr: String,
    variable: String,
    at: String,
    direction: String,
    verbosity: String,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<Vec<Step>, ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    processing::limits::limit_steps_with_verbosity(
        &mut *engine,
        &expr,
        &variable,
        &at,
        parse_limit_direction(&direction)?,
        parse_verbosity(&verbosity)?,
    )
    .map_err(message)
}

#[derive(Deserialize)]
struct OdeInitialConditionRequest {
    derivative_order: u32,
    point: String,
    value: String,
}

fn ode_conditions(requests: &[OdeInitialConditionRequest]) -> Vec<InitialCondition<'_>> {
    requests
        .iter()
        .map(|condition| InitialCondition {
            derivative_order: condition.derivative_order,
            point: &condition.point,
            value: &condition.value,
        })
        .collect()
}

#[tauri::command]
fn solve_ode(
    equation: String,
    independent: String,
    dependent: String,
    initial_conditions: Vec<OdeInitialConditionRequest>,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<OdeResult, ErrorResponse> {
    let conditions = ode_conditions(&initial_conditions);
    let mut engine = lock_engine(&engine)?;
    processing::ode::solve(
        &mut *engine,
        &equation,
        &independent,
        &dependent,
        &conditions,
    )
    .map_err(message)
}

#[tauri::command]
fn solve_ode_steps(
    equation: String,
    independent: String,
    dependent: String,
    initial_conditions: Vec<OdeInitialConditionRequest>,
    verbosity: String,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<OdeStepResult, ErrorResponse> {
    let conditions = ode_conditions(&initial_conditions);
    let mut engine = lock_engine(&engine)?;
    processing::ode::solve_steps_with_verbosity(
        &mut *engine,
        &equation,
        &independent,
        &dependent,
        &conditions,
        parse_verbosity(&verbosity)?,
    )
    .map_err(message)
}

#[derive(Deserialize)]
struct NumericOdeOptionsRequest {
    end: f64,
    initial_step: Option<f64>,
    absolute_tolerance: Option<f64>,
    relative_tolerance: Option<f64>,
    max_steps: Option<usize>,
    max_evaluations: Option<usize>,
}

#[tauri::command]
fn solve_ode_numeric(
    equation: String,
    independent: String,
    dependent: String,
    initial_conditions: Vec<OdeInitialConditionRequest>,
    options: NumericOdeOptionsRequest,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<NumericOdeResult, ErrorResponse> {
    let conditions = ode_conditions(&initial_conditions);
    let defaults = NumericOdeOptions::default();
    let options = NumericOdeOptions {
        end: options.end,
        initial_step: options.initial_step.unwrap_or(defaults.initial_step),
        absolute_tolerance: options
            .absolute_tolerance
            .unwrap_or(defaults.absolute_tolerance),
        relative_tolerance: options
            .relative_tolerance
            .unwrap_or(defaults.relative_tolerance),
        max_steps: options.max_steps.unwrap_or(defaults.max_steps),
        max_evaluations: options.max_evaluations.unwrap_or(defaults.max_evaluations),
    };
    let mut engine = lock_engine(&engine)?;
    processing::ode_numeric::solve_initial_value(
        &mut *engine,
        &equation,
        &independent,
        &dependent,
        &conditions,
        options,
    )
    .map_err(message)
}

#[tauri::command]
fn approximate_numeric(
    expr: String,
    precision_digits: u32,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<NumericResult, ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    processing::numeric::approximate(&mut *engine, &expr, precision_digits).map_err(message)
}

#[tauri::command]
fn find_numeric_root(
    expr: String,
    variable: String,
    initial: f64,
    accuracy: f64,
    min: Option<f64>,
    max: Option<f64>,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<RootResult, ErrorResponse> {
    let bounds = match (min, max) {
        (Some(min), Some(max)) => Some((min, max)),
        (None, None) => None,
        _ => return Err(invalid_input("求根区间必须同时填写上下界")),
    };
    let mut engine = lock_engine(&engine)?;
    processing::numeric::find_root(&mut *engine, &expr, &variable, initial, accuracy, bounds)
        .map_err(message)
}

#[tauri::command]
fn calculate_taylor(
    expr: String,
    variable: String,
    point: String,
    degree: u32,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<TaylorResult, ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    processing::numeric::taylor(&mut *engine, &expr, &variable, &point, degree).map_err(message)
}

#[tauri::command]
fn calculate_matrix(
    left: String,
    operation: String,
    right: Option<String>,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<MatrixResult, ErrorResponse> {
    let operation = match operation.as_str() {
        "add" => MatrixOperation::Add,
        "multiply" => MatrixOperation::Multiply,
        "transpose" => MatrixOperation::Transpose,
        "determinant" => MatrixOperation::Determinant,
        "inverse" => MatrixOperation::Inverse,
        "solve" => MatrixOperation::Solve,
        "eigenvalues" => MatrixOperation::Eigenvalues,
        _ => return Err(invalid_input(format!("未知矩阵运算: {operation}"))),
    };
    let mut engine = lock_engine(&engine)?;
    processing::linear_algebra::compute(&mut *engine, &left, operation, right.as_deref())
        .map_err(message)
}

#[tauri::command]
fn sample_plot(
    expr: String,
    variable: String,
    min: f64,
    max: f64,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<SampledPlot, ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    processing::plot::sample(
        &mut *engine,
        &expr,
        &variable,
        (min, max),
        &SampleOptions::default(),
    )
    .map_err(message)
}

fn parse_assumption_fact(value: &str) -> Result<AssumptionFact, ErrorResponse> {
    match value {
        "real" => Ok(AssumptionFact::Real),
        "integer" => Ok(AssumptionFact::Integer),
        "positive" => Ok(AssumptionFact::Positive),
        "negative" => Ok(AssumptionFact::Negative),
        "non_zero" => Ok(AssumptionFact::NonZero),
        _ => Err(invalid_input(format!("未知假设性质: {value}"))),
    }
}

#[tauri::command]
fn set_assumption(
    symbol: String,
    fact: String,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<AssumptionState, ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    processing::assumptions::assume(&mut *engine, &symbol, parse_assumption_fact(&fact)?)
        .map_err(message)
}

#[tauri::command]
fn clear_assumptions(
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<(), ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    processing::assumptions::clear_assumptions(&mut *engine).map_err(message)
}

#[tauri::command]
fn get_assumptions(
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<Vec<AssumptionState>, ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    processing::assumptions::list_assumptions(&mut *engine).map_err(message)
}

#[derive(Serialize)]
struct RawResult {
    expression: String,
    tex: String,
}

#[tauri::command]
fn evaluate(
    expr: String,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<RawResult, ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    let result = engine.eval(&expr).map_err(message)?;
    Ok(RawResult {
        expression: result.expr.to_string(),
        tex: result.tex.trim_matches('$').to_string(),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let engine = RustEngineProxy::spawn()
        .expect("无法初始化 Rust yacas 引擎(可用 YACAS_SCRIPTS 环境变量指定脚本库)");
    tauri::Builder::default()
        .manage(Mutex::new(engine))
        .invoke_handler(tauri::generate_handler![
            calculate_steps,
            transform_expression,
            solve_equations,
            calculate_limit,
            calculate_limit_steps,
            solve_ode,
            solve_ode_steps,
            solve_ode_numeric,
            approximate_numeric,
            find_numeric_root,
            calculate_taylor,
            calculate_matrix,
            sample_plot,
            set_assumption,
            clear_assumptions,
            get_assumptions,
            evaluate,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
