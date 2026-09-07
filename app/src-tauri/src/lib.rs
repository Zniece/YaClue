use processing::algebra::{TransformKind, TransformResult};
use processing::assumptions::{AssumptionFact, AssumptionState};
use processing::engine::{Engine, EngineError, RustEngineProxy};
use processing::equations::SolveResult;
use processing::limits::{LimitDirection, LimitResult};
use processing::plot::{SampleOptions, SampledPlot};
use processing::steps::{Step, StepVerbosity};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, MutexGuard};

fn lock_engine<'a>(
    state: &'a tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<MutexGuard<'a, RustEngineProxy>, String> {
    state.lock().map_err(|error| error.to_string())
}

fn message(error: EngineError) -> String {
    error.to_string()
}

fn parse_verbosity(value: &str) -> Result<StepVerbosity, String> {
    match value {
        "concise" => Ok(StepVerbosity::Concise),
        "standard" => Ok(StepVerbosity::Standard),
        "detailed" => Ok(StepVerbosity::Detailed),
        _ => Err(format!("未知步骤粒度: {value}")),
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
) -> Result<Vec<Step>, String> {
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
        _ => return Err(format!("未知步骤运算: {}", request.kind)),
    }
    .map_err(message)
}

#[tauri::command]
fn transform_expression(
    expr: String,
    operation: String,
    variable: String,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<TransformResult, String> {
    let kind = match operation.as_str() {
        "simplify" => TransformKind::Simplify,
        "tidy" => TransformKind::Tidy,
        "expand" => TransformKind::Expand,
        "factor" => TransformKind::Factor,
        "apart" => TransformKind::Apart,
        _ => return Err(format!("未知代数变换: {operation}")),
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
) -> Result<SolveResult, String> {
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
) -> Result<LimitResult, String> {
    let direction = match direction.as_str() {
        "both" => LimitDirection::Both,
        "left" => LimitDirection::Left,
        "right" => LimitDirection::Right,
        _ => return Err(format!("未知极限方向: {direction}")),
    };
    let mut engine = lock_engine(&engine)?;
    processing::limits::limit(&mut *engine, &expr, &variable, &at, direction).map_err(message)
}

#[tauri::command]
fn sample_plot(
    expr: String,
    variable: String,
    min: f64,
    max: f64,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<SampledPlot, String> {
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

fn parse_assumption_fact(value: &str) -> Result<AssumptionFact, String> {
    match value {
        "real" => Ok(AssumptionFact::Real),
        "integer" => Ok(AssumptionFact::Integer),
        "positive" => Ok(AssumptionFact::Positive),
        "negative" => Ok(AssumptionFact::Negative),
        "non_zero" => Ok(AssumptionFact::NonZero),
        _ => Err(format!("未知假设性质: {value}")),
    }
}

#[tauri::command]
fn set_assumption(
    symbol: String,
    fact: String,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<AssumptionState, String> {
    let mut engine = lock_engine(&engine)?;
    processing::assumptions::assume(&mut *engine, &symbol, parse_assumption_fact(&fact)?)
        .map_err(message)
}

#[tauri::command]
fn clear_assumptions(
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<(), String> {
    let mut engine = lock_engine(&engine)?;
    processing::assumptions::clear_assumptions(&mut *engine).map_err(message)
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
) -> Result<RawResult, String> {
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
            sample_plot,
            set_assumption,
            clear_assumptions,
            evaluate,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
