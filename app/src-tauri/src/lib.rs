use processing::algebra::{TransformKind, TransformResult};
use processing::assumptions::{AssumptionFact, AssumptionState};
use processing::engine::{Engine, EngineError, ErrorCode, ErrorResponse, RustEngineProxy};
use processing::equations::{EquationStepResult, SolveResult};
use processing::extrema::{ExtremaResult, ExtremaStepResult, LagrangeResult, LagrangeStepResult};
use processing::limits::{LimitDirection, LimitResult};
use processing::linear_algebra::{MatrixOperation, MatrixResult};
use processing::multiple_integrals::{
    DoubleIntegralResult, DoubleIntegralStepResult, IntegralBound, PolarIntegralResult,
    PolarIntegralStepResult, PolarRegion,
};
use processing::numeric::{NumericResult, RootResult, TaylorResult};
use processing::ode::{InitialCondition, OdeResult, OdeStepResult};
use processing::ode_numeric::{NumericOdeOptions, NumericOdeResult};
use processing::plot::{SampleOptions, SampledPlot};
use processing::semantic::{AnalyzedInput, SemanticSummary};
use processing::steps::{Step, StepVerbosity};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Mutex, MutexGuard};
use tauri::Manager;

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
async fn calculate_steps(
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
async fn transform_expression(
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
async fn solve_equations(
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
async fn solve_equation_steps(
    equation: String,
    variable: String,
    verbosity: String,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<EquationStepResult, ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    processing::equations::solve_steps_with_verbosity(
        &mut *engine,
        &equation,
        &variable,
        parse_verbosity(&verbosity)?,
    )
    .map_err(message)
}

#[derive(Deserialize)]
struct DoubleIntegralRequest {
    expression: String,
    inner_variable: String,
    inner_lower: String,
    inner_upper: String,
    outer_variable: String,
    outer_lower: String,
    outer_upper: String,
    verbosity: Option<String>,
}

fn integral_bounds(request: &DoubleIntegralRequest) -> (IntegralBound<'_>, IntegralBound<'_>) {
    (
        IntegralBound {
            variable: &request.inner_variable,
            lower: &request.inner_lower,
            upper: &request.inner_upper,
        },
        IntegralBound {
            variable: &request.outer_variable,
            lower: &request.outer_lower,
            upper: &request.outer_upper,
        },
    )
}

#[tauri::command]
async fn calculate_double_integral(
    request: DoubleIntegralRequest,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<DoubleIntegralResult, ErrorResponse> {
    let (inner, outer) = integral_bounds(&request);
    let mut engine = lock_engine(&engine)?;
    processing::multiple_integrals::double_integral(&mut *engine, &request.expression, inner, outer)
        .map_err(message)
}

#[tauri::command]
async fn calculate_double_integral_steps(
    request: DoubleIntegralRequest,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<DoubleIntegralStepResult, ErrorResponse> {
    let verbosity = parse_verbosity(request.verbosity.as_deref().unwrap_or("detailed"))?;
    let (inner, outer) = integral_bounds(&request);
    let mut engine = lock_engine(&engine)?;
    processing::multiple_integrals::double_integral_steps_with_verbosity(
        &mut *engine,
        &request.expression,
        inner,
        outer,
        verbosity,
    )
    .map_err(message)
}

#[derive(Deserialize)]
struct PolarIntegralRequest {
    expression: String,
    x_variable: String,
    y_variable: String,
    radius_variable: String,
    angle_variable: String,
    radial_lower: String,
    radial_upper: String,
    angle_lower: String,
    angle_upper: String,
    verbosity: Option<String>,
}

fn polar_region(request: &PolarIntegralRequest) -> PolarRegion<'_> {
    PolarRegion {
        radial_lower: &request.radial_lower,
        radial_upper: &request.radial_upper,
        angle_lower: &request.angle_lower,
        angle_upper: &request.angle_upper,
    }
}

#[tauri::command]
async fn calculate_polar_integral(
    request: PolarIntegralRequest,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<PolarIntegralResult, ErrorResponse> {
    let region = polar_region(&request);
    let mut engine = lock_engine(&engine)?;
    processing::multiple_integrals::polar_integral(
        &mut *engine,
        &request.expression,
        &request.x_variable,
        &request.y_variable,
        &request.radius_variable,
        &request.angle_variable,
        region,
    )
    .map_err(message)
}

#[tauri::command]
async fn calculate_polar_integral_steps(
    request: PolarIntegralRequest,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<PolarIntegralStepResult, ErrorResponse> {
    let verbosity = parse_verbosity(request.verbosity.as_deref().unwrap_or("detailed"))?;
    let region = polar_region(&request);
    let mut engine = lock_engine(&engine)?;
    processing::multiple_integrals::polar_integral_steps_with_verbosity(
        &mut *engine,
        &request.expression,
        &request.x_variable,
        &request.y_variable,
        &request.radius_variable,
        &request.angle_variable,
        region,
        verbosity,
    )
    .map_err(message)
}

#[derive(Deserialize)]
struct ExtremaRequest {
    expression: String,
    x_variable: String,
    y_variable: String,
    verbosity: Option<String>,
}

#[tauri::command]
async fn analyze_extrema(
    request: ExtremaRequest,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<ExtremaResult, ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    processing::extrema::analyze(
        &mut *engine,
        &request.expression,
        &request.x_variable,
        &request.y_variable,
    )
    .map_err(message)
}

#[tauri::command]
async fn analyze_extrema_steps(
    request: ExtremaRequest,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<ExtremaStepResult, ErrorResponse> {
    let verbosity = parse_verbosity(request.verbosity.as_deref().unwrap_or("detailed"))?;
    let mut engine = lock_engine(&engine)?;
    processing::extrema::analyze_steps_with_verbosity(
        &mut *engine,
        &request.expression,
        &request.x_variable,
        &request.y_variable,
        verbosity,
    )
    .map_err(message)
}

#[derive(Deserialize)]
struct LagrangeRequest {
    expression: String,
    constraint: String,
    x_variable: String,
    y_variable: String,
    verbosity: Option<String>,
}

#[tauri::command]
async fn analyze_lagrange(
    request: LagrangeRequest,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<LagrangeResult, ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    processing::extrema::analyze_lagrange(
        &mut *engine,
        &request.expression,
        &request.constraint,
        &request.x_variable,
        &request.y_variable,
    )
    .map_err(message)
}

#[tauri::command]
async fn analyze_lagrange_steps(
    request: LagrangeRequest,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<LagrangeStepResult, ErrorResponse> {
    let verbosity = parse_verbosity(request.verbosity.as_deref().unwrap_or("detailed"))?;
    let mut engine = lock_engine(&engine)?;
    processing::extrema::analyze_lagrange_steps_with_verbosity(
        &mut *engine,
        &request.expression,
        &request.constraint,
        &request.x_variable,
        &request.y_variable,
        verbosity,
    )
    .map_err(message)
}

#[tauri::command]
async fn calculate_limit(
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
async fn calculate_limit_steps(
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
async fn solve_ode(
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
async fn solve_ode_steps(
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
async fn solve_ode_numeric(
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
async fn approximate_numeric(
    expr: String,
    precision_digits: u32,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<NumericResult, ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    processing::numeric::approximate(&mut *engine, &expr, precision_digits).map_err(message)
}

#[tauri::command]
async fn find_numeric_root(
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
async fn calculate_taylor(
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
async fn calculate_matrix(
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
async fn sample_plot(
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
async fn set_assumption(
    symbol: String,
    fact: String,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<AssumptionState, ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    processing::assumptions::assume(&mut *engine, &symbol, parse_assumption_fact(&fact)?)
        .map_err(message)
}

#[tauri::command]
async fn clear_assumptions(
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<(), ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    processing::assumptions::clear_assumptions(&mut *engine).map_err(message)
}

#[tauri::command]
async fn get_assumptions(
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

#[derive(Deserialize)]
struct ProcessExpressionRequest {
    expression: String,
    steps: bool,
    verbosity: String,
}

#[derive(Serialize)]
struct ProcessExpressionResult {
    kind: String,
    title: String,
    expression: String,
    tex: String,
    steps: Vec<Step>,
    data: Value,
    semantic: SemanticSummary,
}

struct DispatchExpressionResult {
    kind: String,
    title: String,
    expression: String,
    tex: String,
    steps: Vec<Step>,
    data: Value,
}

fn unified_result<T: Serialize>(
    kind: &str,
    title: &str,
    expression: String,
    tex: String,
    steps: Vec<Step>,
    data: &T,
) -> Result<DispatchExpressionResult, ErrorResponse> {
    Ok(DispatchExpressionResult {
        kind: kind.into(),
        title: title.into(),
        expression,
        tex,
        steps,
        data: serde_json::to_value(data)
            .map_err(|error| invalid_input(format!("结果序列化失败: {error}")))?,
    })
}

fn final_step(steps: &[Step]) -> (String, String) {
    steps
        .last()
        .map(|step| (step.expr.clone(), step.tex.clone()))
        .unwrap_or_default()
}

fn list_or_single(expression: &str, label: &str) -> Result<Vec<String>, ErrorResponse> {
    let call = processing::input::root_call(expression, label).map_err(message)?;
    Ok(match call {
        Some(call) if call.head == "List" => call.arguments,
        _ => vec![expression.to_string()],
    })
}

fn dispatch_expression_with_engine(
    request: ProcessExpressionRequest,
    engine: &mut RustEngineProxy,
    analyzed: &AnalyzedInput,
) -> Result<DispatchExpressionResult, ErrorResponse> {
    let call = &analyzed.root_call;
    let verbosity = parse_verbosity(&request.verbosity)?;
    if let Some(call) = call {
        match (call.head.as_str(), call.arguments.as_slice()) {
            ("D", [variable, expression]) | ("Deriv", [variable, expression]) => {
                if request.steps {
                    let steps = processing::steps::derive_steps_order_with_verbosity(
                        &mut *engine,
                        expression,
                        variable,
                        1,
                        verbosity,
                    )
                    .map_err(message)?;
                    let (expression, tex) = final_step(&steps);
                    return unified_result("derivative", "导数", expression, tex, steps, &());
                }
            }
            ("D", [variable, order, expression]) | ("Deriv", [variable, order, expression]) => {
                let order = order
                    .parse::<u32>()
                    .map_err(|_| invalid_input("导数阶数必须是非负整数"))?;
                if request.steps {
                    let steps = processing::steps::derive_steps_order_with_verbosity(
                        &mut *engine,
                        expression,
                        variable,
                        order,
                        verbosity,
                    )
                    .map_err(message)?;
                    let (expression, tex) = final_step(&steps);
                    return unified_result("derivative", "导数", expression, tex, steps, &());
                }
            }
            ("Integrate", [variable, expression]) if request.steps => {
                let steps = processing::steps::derive_integrals_with_verbosity(
                    &mut *engine,
                    expression,
                    variable,
                    verbosity,
                )
                .map_err(message)?;
                let (expression, tex) = final_step(&steps);
                return unified_result("integral", "不定积分", expression, tex, steps, &());
            }
            ("Integrate", [variable, from, to, expression]) if request.steps => {
                let steps = processing::steps::derive_definite_with_verbosity(
                    &mut *engine,
                    expression,
                    variable,
                    from,
                    to,
                    verbosity,
                )
                .map_err(message)?;
                let (expression, tex) = final_step(&steps);
                return unified_result("definite_integral", "定积分", expression, tex, steps, &());
            }
            (
                "DoubleIntegral",
                [expression, inner_var, inner_from, inner_to, outer_var, outer_from, outer_to],
            ) => {
                let inner = IntegralBound {
                    variable: inner_var,
                    lower: inner_from,
                    upper: inner_to,
                };
                let outer = IntegralBound {
                    variable: outer_var,
                    lower: outer_from,
                    upper: outer_to,
                };
                if request.steps {
                    let result =
                        processing::multiple_integrals::double_integral_steps_with_verbosity(
                            &mut *engine,
                            expression,
                            inner,
                            outer,
                            verbosity,
                        )
                        .map_err(message)?;
                    return unified_result(
                        "double_integral",
                        "二重积分",
                        result.result.value.clone(),
                        result.result.tex.clone(),
                        result.steps.clone(),
                        &result,
                    );
                }
                let result = processing::multiple_integrals::double_integral(
                    &mut *engine,
                    expression,
                    inner,
                    outer,
                )
                .map_err(message)?;
                return unified_result(
                    "double_integral",
                    "二重积分",
                    result.value.clone(),
                    result.tex.clone(),
                    vec![],
                    &result,
                );
            }
            (
                "PolarIntegral",
                [expression, x, y, radius, angle, radial_from, radial_to, angle_from, angle_to],
            ) => {
                let region = PolarRegion {
                    radial_lower: radial_from,
                    radial_upper: radial_to,
                    angle_lower: angle_from,
                    angle_upper: angle_to,
                };
                if request.steps {
                    let result =
                        processing::multiple_integrals::polar_integral_steps_with_verbosity(
                            &mut *engine,
                            expression,
                            x,
                            y,
                            radius,
                            angle,
                            region,
                            verbosity,
                        )
                        .map_err(message)?;
                    return unified_result(
                        "polar_integral",
                        "极坐标积分",
                        result.result.integral.value.clone(),
                        result.result.integral.tex.clone(),
                        result.steps.clone(),
                        &result,
                    );
                }
                let result = processing::multiple_integrals::polar_integral(
                    &mut *engine,
                    expression,
                    x,
                    y,
                    radius,
                    angle,
                    region,
                )
                .map_err(message)?;
                return unified_result(
                    "polar_integral",
                    "极坐标积分",
                    result.integral.value.clone(),
                    result.integral.tex.clone(),
                    vec![],
                    &result,
                );
            }
            ("Limit", [variable, at, expression]) => {
                if request.steps {
                    let steps = processing::limits::limit_steps_with_verbosity(
                        &mut *engine,
                        expression,
                        variable,
                        at,
                        LimitDirection::Both,
                        verbosity,
                    )
                    .map_err(message)?;
                    let (expression, tex) = final_step(&steps);
                    return unified_result("limit", "极限", expression, tex, steps, &());
                }
                let result = processing::limits::limit(
                    &mut *engine,
                    expression,
                    variable,
                    at,
                    LimitDirection::Both,
                )
                .map_err(message)?;
                return unified_result(
                    "limit",
                    "极限",
                    result.value.clone(),
                    result.tex.clone(),
                    vec![],
                    &result,
                );
            }
            ("Limit", [variable, at, direction, expression]) => {
                let direction = match direction.as_str() {
                    "Left" => LimitDirection::Left,
                    "Right" => LimitDirection::Right,
                    _ => return Err(invalid_input("极限方向应为 Left 或 Right")),
                };
                if request.steps {
                    let steps = processing::limits::limit_steps_with_verbosity(
                        &mut *engine,
                        expression,
                        variable,
                        at,
                        direction,
                        verbosity,
                    )
                    .map_err(message)?;
                    let (expression, tex) = final_step(&steps);
                    return unified_result("limit", "极限", expression, tex, steps, &());
                }
                let result =
                    processing::limits::limit(&mut *engine, expression, variable, at, direction)
                        .map_err(message)?;
                return unified_result(
                    "limit",
                    "极限",
                    result.value.clone(),
                    result.tex.clone(),
                    vec![],
                    &result,
                );
            }
            ("OdeSolve", [equation]) => {
                if request.steps {
                    let result = processing::ode::solve_steps_with_verbosity(
                        &mut *engine,
                        equation,
                        "x",
                        "y",
                        &[],
                        verbosity,
                    )
                    .map_err(message)?;
                    return unified_result(
                        "ode",
                        "常微分方程",
                        result.result.solution.clone(),
                        result.result.tex.clone(),
                        result.steps.clone(),
                        &result,
                    );
                }
                let result = processing::ode::solve(&mut *engine, equation, "x", "y", &[])
                    .map_err(message)?;
                return unified_result(
                    "ode",
                    "常微分方程",
                    result.solution.clone(),
                    result.tex.clone(),
                    vec![],
                    &result,
                );
            }
            ("Solve" | "OldSolve", [equations, variables]) => {
                let equations = list_or_single(equations, "方程列表")?;
                let variables = list_or_single(variables, "变量列表")?;
                let equation_refs: Vec<_> = equations.iter().map(String::as_str).collect();
                let variable_refs: Vec<_> = variables.iter().map(String::as_str).collect();
                let (solved, steps) = if request.steps
                    && equations.len() == 1
                    && variables.len() == 1
                {
                    let stepped = processing::equations::solve_steps_with_verbosity(
                        &mut *engine,
                        &equations[0],
                        &variables[0],
                        verbosity,
                    )
                    .map_err(message)?;
                    (stepped.result, stepped.steps)
                } else {
                    (
                        processing::equations::solve(&mut *engine, &equation_refs, &variable_refs)
                            .map_err(message)?,
                        vec![],
                    )
                };
                return unified_result(
                    "equation",
                    if equations.len() == 1 {
                        "方程"
                    } else {
                        "方程组"
                    },
                    String::new(),
                    solved.tex.clone(),
                    steps,
                    &solved,
                );
            }
            ("OdeSolveNumeric", [equation, independent, dependent, start, value, end]) => {
                let end = end
                    .parse::<f64>()
                    .map_err(|_| invalid_input("数值 ODE 的终点必须是有限数字"))?;
                let condition = [InitialCondition {
                    derivative_order: 0,
                    point: start,
                    value,
                }];
                let result = processing::ode_numeric::solve_initial_value(
                    &mut *engine,
                    equation,
                    independent,
                    dependent,
                    &condition,
                    NumericOdeOptions {
                        end,
                        ..NumericOdeOptions::default()
                    },
                )
                .map_err(message)?;
                return unified_result(
                    "numeric_ode",
                    "常微分方程数值解",
                    String::new(),
                    String::new(),
                    vec![],
                    &result,
                );
            }
            ("N", [expression, precision]) => {
                let precision = precision
                    .parse::<u32>()
                    .map_err(|_| invalid_input("近似精度必须是正整数"))?;
                let result = processing::numeric::approximate(&mut *engine, expression, precision)
                    .map_err(message)?;
                return unified_result(
                    "numeric",
                    "数值近似",
                    result.output.clone(),
                    result.tex.clone(),
                    vec![],
                    &result,
                );
            }
            ("FindRoot", [expression, variable, initial]) => {
                let initial = initial
                    .parse::<f64>()
                    .map_err(|_| invalid_input("数值求根初值必须是有限数字"))?;
                let result = processing::numeric::find_root(
                    &mut *engine,
                    expression,
                    variable,
                    initial,
                    1e-8,
                    None,
                )
                .map_err(message)?;
                return unified_result(
                    "numeric_root",
                    "数值根",
                    result.output.clone(),
                    result.tex.clone(),
                    vec![],
                    &result,
                );
            }
            ("Plot", [expression, variable, min, max]) => {
                let min = min
                    .parse::<f64>()
                    .map_err(|_| invalid_input("绘图区间下界必须是有限数字"))?;
                let max = max
                    .parse::<f64>()
                    .map_err(|_| invalid_input("绘图区间上界必须是有限数字"))?;
                let result = processing::plot::sample(
                    &mut *engine,
                    expression,
                    variable,
                    (min, max),
                    &SampleOptions::default(),
                )
                .map_err(message)?;
                return unified_result(
                    "plot",
                    "函数图像",
                    request.expression.clone(),
                    String::new(),
                    vec![],
                    &result,
                );
            }
            (head @ ("Factor" | "Expand" | "Simplify" | "Tidy"), [expression]) => {
                let kind = match head {
                    "Factor" => TransformKind::Factor,
                    "Expand" => TransformKind::Expand,
                    "Simplify" => TransformKind::Simplify,
                    _ => TransformKind::Tidy,
                };
                let result = processing::algebra::transform(&mut *engine, expression, kind, None)
                    .map_err(message)?;
                return unified_result(
                    "algebra",
                    "代数变换",
                    result.output.clone(),
                    result.tex.clone(),
                    vec![],
                    &result,
                );
            }
            ("Apart", [expression, variable]) => {
                let result = processing::algebra::transform(
                    &mut *engine,
                    expression,
                    TransformKind::Apart,
                    Some(variable),
                )
                .map_err(message)?;
                return unified_result(
                    "algebra",
                    "部分分式分解",
                    result.output.clone(),
                    result.tex.clone(),
                    vec![],
                    &result,
                );
            }
            ("Taylor", [variable, point, degree, expression]) => {
                let degree = degree
                    .parse::<u32>()
                    .map_err(|_| invalid_input("Taylor 次数必须是非负整数"))?;
                let result =
                    processing::numeric::taylor(&mut *engine, expression, variable, point, degree)
                        .map_err(message)?;
                return unified_result(
                    "taylor",
                    "Taylor 多项式",
                    result.output.clone(),
                    result.tex.clone(),
                    vec![],
                    &result,
                );
            }
            ("Extrema", [expression, x, y]) => {
                if request.steps {
                    let result = processing::extrema::analyze_steps_with_verbosity(
                        &mut *engine,
                        expression,
                        x,
                        y,
                        verbosity,
                    )
                    .map_err(message)?;
                    return unified_result(
                        "extrema",
                        "无约束极值",
                        result.result.expression.clone(),
                        result.result.tex.clone(),
                        result.steps.clone(),
                        &result,
                    );
                }
                let result = processing::extrema::analyze(&mut *engine, expression, x, y)
                    .map_err(message)?;
                return unified_result(
                    "extrema",
                    "无约束极值",
                    result.expression.clone(),
                    result.tex.clone(),
                    vec![],
                    &result,
                );
            }
            ("Lagrange", [expression, constraint, x, y]) => {
                if request.steps {
                    let result = processing::extrema::analyze_lagrange_steps_with_verbosity(
                        &mut *engine,
                        expression,
                        constraint,
                        x,
                        y,
                        verbosity,
                    )
                    .map_err(message)?;
                    return unified_result(
                        "lagrange",
                        "约束极值",
                        result.result.expression.clone(),
                        result.result.tex.clone(),
                        result.steps.clone(),
                        &result,
                    );
                }
                let result = processing::extrema::analyze_lagrange(
                    &mut *engine,
                    expression,
                    constraint,
                    x,
                    y,
                )
                .map_err(message)?;
                return unified_result(
                    "lagrange",
                    "约束极值",
                    result.expression.clone(),
                    result.tex.clone(),
                    vec![],
                    &result,
                );
            }
            (head @ ("Determinant" | "Inverse" | "Transpose" | "EigenValues"), [matrix]) => {
                let operation = match head {
                    "Determinant" => MatrixOperation::Determinant,
                    "Inverse" => MatrixOperation::Inverse,
                    "Transpose" => MatrixOperation::Transpose,
                    _ => MatrixOperation::Eigenvalues,
                };
                let result =
                    processing::linear_algebra::compute(&mut *engine, matrix, operation, None)
                        .map_err(message)?;
                return unified_result(
                    "matrix",
                    "线性代数",
                    result.output.clone(),
                    result.tex.clone(),
                    vec![],
                    &result,
                );
            }
            ("MatrixSolve" | "SolveMatrix", [matrix, vector]) => {
                let result = processing::linear_algebra::compute(
                    &mut *engine,
                    matrix,
                    MatrixOperation::Solve,
                    Some(vector),
                )
                .map_err(message)?;
                return unified_result(
                    "matrix",
                    "线性方程组",
                    result.output.clone(),
                    result.tex.clone(),
                    vec![],
                    &result,
                );
            }
            (head @ ("+" | "*"), [left, right])
                if call
                    .argument_heads
                    .iter()
                    .all(|head| head.as_deref() == Some("List")) =>
            {
                let operation = if head == "+" {
                    MatrixOperation::Add
                } else {
                    MatrixOperation::Multiply
                };
                let result =
                    processing::linear_algebra::compute(&mut *engine, left, operation, Some(right))
                        .map_err(message)?;
                return unified_result(
                    "matrix",
                    "线性代数",
                    result.output.clone(),
                    result.tex.clone(),
                    vec![],
                    &result,
                );
            }
            ("=" | "==", [_, _]) => {
                let equations = [&request.expression[..]];
                let solved =
                    processing::equations::solve(&mut *engine, &equations, &[]).map_err(message)?;
                let (solved, steps) = if request.steps && solved.variables.len() == 1 {
                    let stepped = processing::equations::solve_steps_with_verbosity(
                        &mut *engine,
                        &request.expression,
                        &solved.variables[0],
                        verbosity,
                    )
                    .map_err(message)?;
                    (stepped.result, stepped.steps)
                } else {
                    (solved, vec![])
                };
                return unified_result(
                    "equation",
                    "方程",
                    solved
                        .solutions
                        .first()
                        .map(|solution| format!("{solution:?}"))
                        .unwrap_or_default(),
                    solved.tex.clone(),
                    steps,
                    &solved,
                );
            }
            _ => {}
        }
    }

    let evaluated = engine.eval(&request.expression).map_err(message)?;
    unified_result(
        "evaluation",
        "计算结果",
        evaluated.expr.to_string(),
        evaluated.tex.trim_matches('$').to_string(),
        vec![],
        &(),
    )
}

fn process_expression_with_engine(
    request: ProcessExpressionRequest,
    engine: &mut RustEngineProxy,
) -> Result<ProcessExpressionResult, ErrorResponse> {
    let analyzed =
        processing::semantic::analyze_input(&request.expression, "表达式").map_err(message)?;
    let result = dispatch_expression_with_engine(request, engine, &analyzed)?;
    Ok(ProcessExpressionResult {
        kind: result.kind,
        title: result.title,
        expression: result.expression,
        tex: result.tex,
        steps: result.steps,
        data: result.data,
        semantic: analyzed.semantic,
    })
}

#[tauri::command]
async fn process_expression(
    request: ProcessExpressionRequest,
    engine: tauri::State<'_, Mutex<RustEngineProxy>>,
) -> Result<ProcessExpressionResult, ErrorResponse> {
    let mut engine = lock_engine(&engine)?;
    process_expression_with_engine(request, &mut engine)
}

#[tauri::command]
async fn evaluate(
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
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(target_os = "android")]
            let resources = app
                .path()
                .app_data_dir()?
                .join("files")
                .join("bundled")
                .join(env!("CARGO_PKG_VERSION"));
            #[cfg(not(target_os = "android"))]
            let resources = app.path().resource_dir()?;
            let scripts = std::env::var("YACAS_SCRIPTS").unwrap_or_else(|_| {
                resources
                    .join("yacas/scripts")
                    .to_string_lossy()
                    .into_owned()
            });
            let steps = std::env::var("YACAS_STEPS_SCRIPTS").unwrap_or_else(|_| {
                resources
                    .join("processing/scripts")
                    .to_string_lossy()
                    .into_owned()
            });
            let engine = RustEngineProxy::spawn_with_scripts(scripts, steps)
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            app.manage(Mutex::new(engine));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            calculate_steps,
            transform_expression,
            solve_equations,
            solve_equation_steps,
            calculate_double_integral,
            calculate_double_integral_steps,
            calculate_polar_integral,
            calculate_polar_integral_steps,
            analyze_extrema,
            analyze_extrema_steps,
            analyze_lagrange,
            analyze_lagrange_steps,
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
            process_expression,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(expression: &str, steps: bool) -> ProcessExpressionRequest {
        ProcessExpressionRequest {
            expression: expression.into(),
            steps,
            verbosity: "standard".into(),
        }
    }

    #[test]
    fn unified_expression_dispatches_core_calculator_paths() {
        let mut engine = RustEngineProxy::spawn().unwrap();

        let derivative =
            process_expression_with_engine(request("D(x)Sin(x)^2", true), &mut engine).unwrap();
        assert_eq!(derivative.kind, "derivative");
        assert!(!derivative.steps.is_empty());
        assert_eq!(derivative.semantic.symbols, ["x".to_string()]);

        let matrix = process_expression_with_engine(
            request("{{1,2},{3,4}}*{{5,6},{7,8}}", false),
            &mut engine,
        )
        .unwrap();
        assert_eq!(matrix.kind, "matrix");
        assert!(matrix.expression.contains("19"));
        assert_eq!(
            matrix.semantic.shape,
            Some(processing::semantic::MatrixShape {
                rows: 2,
                columns: 2
            })
        );

        let ode =
            process_expression_with_engine(request("OdeSolve(y'==y)", true), &mut engine).unwrap();
        assert_eq!(ode.kind, "ode");
        assert!(!ode.steps.is_empty());
        assert!(!ode.expression.contains("C7"));

        let equations = process_expression_with_engine(
            request("OldSolve({x+y==3,x-y==1},{x,y})", false),
            &mut engine,
        )
        .unwrap();
        assert_eq!(equations.kind, "equation");
        assert!(!equations.tex.is_empty());
        assert_eq!(
            equations.semantic.kind,
            processing::semantic::ValueKind::SolutionSet
        );

        let double_integral = process_expression_with_engine(
            request("DoubleIntegral(x+y,y,0,x,x,0,1)", true),
            &mut engine,
        )
        .unwrap();
        assert_eq!(double_integral.kind, "double_integral");
        assert!(!double_integral.steps.is_empty());
    }
}
