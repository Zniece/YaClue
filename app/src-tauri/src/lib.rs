use processing::algebra::TransformKind;
use processing::assumptions::{AssumptionFact, AssumptionState};
use processing::engine::{Engine, EngineError, ErrorCode, ErrorResponse, RustEngineProxy};
use processing::improper_integrals::ImproperIntegralRequest;
use processing::limits::LimitDirection;
use processing::linear_algebra::MatrixOperation;
use processing::multiple_integrals::{IntegralBound, PolarRegion};
use processing::ode::InitialCondition;
use processing::ode_numeric::NumericOdeOptions;
use processing::plot::SampleOptions;
use processing::protocol::{
    Condition, ConditionSet, OutcomeReason, ResultCompleteness, ResultMetadata,
};
use processing::semantic::{AnalyzedInput, SemanticSummary, ValueKind};
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

#[derive(Deserialize)]
pub struct ProcessExpressionRequest {
    pub expression: String,
    pub steps: bool,
    pub verbosity: String,
}

#[derive(Serialize)]
pub struct ProcessExpressionResult {
    kind: String,
    title: String,
    expression: String,
    tex: String,
    steps: Vec<Step>,
    data: Value,
    semantic: SemanticSummary,
    outcome: ResultMetadata,
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

fn result_metadata(
    result: &DispatchExpressionResult,
    exactness: processing::semantic::Exactness,
) -> Result<ResultMetadata, ErrorResponse> {
    let domain = result.data.get("result").unwrap_or(&result.data);
    let conditions = condition_set_from_value(domain.get("conditions"))?;
    let status = domain
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("completed");
    let held_operation = ["Integrate(", "D(", "Deriv(", "Limit(", "Solve("]
        .iter()
        .any(|prefix| result.expression.trim_start().starts_with(prefix));
    let mut metadata = if status == "condition_insufficient" {
        ResultMetadata::unresolved(exactness, OutcomeReason::ConditionInsufficient)
    } else if status == "unsupported" {
        ResultMetadata::unresolved(exactness, OutcomeReason::UnsupportedOperation)
    } else if status == "divergent" {
        ResultMetadata::no_result(exactness, OutcomeReason::Divergent)
    } else if matches!(
        status,
        "does_not_exist" | "no_solution" | "no_points" | "no_critical_points"
    ) {
        ResultMetadata::no_result(exactness, OutcomeReason::MathematicalAbsence)
    } else if held_operation
        || status.contains("unresolved")
        || matches!(status, "inconclusive" | "no_convergence")
    {
        ResultMetadata::unresolved(exactness, OutcomeReason::AlgorithmUncovered)
    } else {
        ResultMetadata::solved(exactness, conditions)
    };
    if let Some(completeness) = domain.get("completeness").and_then(Value::as_str) {
        metadata.completeness = match completeness {
            "complete" | "parametric" | "periodic" => ResultCompleteness::Complete,
            "representative" => ResultCompleteness::Representative,
            _ => ResultCompleteness::Unknown,
        };
    }
    Ok(metadata)
}

fn arbitrary_constants(result: &DispatchExpressionResult) -> Vec<String> {
    let domain = result.data.get("result").unwrap_or(&result.data);
    domain
        .get("constants")
        .or_else(|| domain.get("arbitrary_constants"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn project_domain_semantic(
    input: &SemanticSummary,
    result: &DispatchExpressionResult,
) -> Result<SemanticSummary, ErrorResponse> {
    if result.data.get("held").is_some_and(|held| !held.is_null()) {
        let mut semantic = input.clone();
        semantic.kind = ValueKind::Unevaluated;
        semantic.completeness = None;
        return Ok(semantic);
    }
    let generated = arbitrary_constants(result);
    if result.kind != "ode" && result.kind != "composition" && generated.is_empty() {
        return Ok(input.clone());
    }
    let semantic_expression = result
        .data
        .get("semantic_expression")
        .and_then(Value::as_str)
        .unwrap_or(&result.expression);
    processing::semantic::project_result(
        input,
        semantic_expression,
        &generated,
        if result.kind == "ode" { &["x"] } else { &[] },
        match result.kind.as_str() {
            "ode" => Some(ValueKind::SolutionSet),
            "integral" => Some(ValueKind::FunctionFamily),
            _ => None,
        },
    )
    .map_err(message)
}

fn condition_set_from_value(value: Option<&Value>) -> Result<ConditionSet, ErrorResponse> {
    let mut conditions = Vec::new();
    let items = match value {
        Some(Value::Array(items)) => Some(items),
        Some(Value::Object(object)) => object.get("conditions").and_then(Value::as_array),
        _ => None,
    };
    if let Some(items) = items {
        for item in items {
            collect_conditions(item, &mut conditions);
        }
    }
    ConditionSet::new(conditions).map_err(message)
}

fn collect_conditions(value: &Value, output: &mut Vec<Condition>) {
    let Some(object) = value.as_object() else {
        if let Some(description) = value.as_str() {
            output.push(Condition::Unknown {
                description: description.into(),
            });
        }
        return;
    };
    if let Some(predicate) = object.get("predicate").and_then(Value::as_str) {
        let expression = object
            .get("expression")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        output.push(match predicate {
            "real_part_positive" => Condition::RealPartPositive { expression },
            "positive" => Condition::Positive { expression },
            "negative" => Condition::Negative { expression },
            "non_zero" => Condition::NonZero { expression },
            "real" => Condition::Real { expression },
            "integer" => Condition::Integer { expression },
            _ => Condition::Unknown {
                description: object
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("未知条件")
                    .into(),
            },
        });
        return;
    }
    match object.get("kind").and_then(Value::as_str) {
        Some("property") => {
            let expression = object
                .get("expression")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let condition = match object.get("fact").and_then(Value::as_str) {
                Some("Positive" | "positive") => Condition::Positive { expression },
                Some("Negative" | "negative") => Condition::Negative { expression },
                Some("NonZero" | "non_zero") => Condition::NonZero { expression },
                Some("Real" | "real") => Condition::Real { expression },
                Some("Integer" | "integer") => Condition::Integer { expression },
                _ => Condition::Unknown {
                    description: value.to_string(),
                },
            };
            output.push(condition);
        }
        Some("relation")
            if object.get("relation").and_then(Value::as_str) == Some("greater_than")
                && object.get("right").and_then(Value::as_str) == Some("0") =>
        {
            let left = object
                .get("left")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if let Some(expression) = left
                .strip_prefix("Re(")
                .and_then(|value| value.strip_suffix(')'))
            {
                output.push(Condition::RealPartPositive {
                    expression: expression.into(),
                });
            } else {
                output.push(Condition::Positive {
                    expression: left.into(),
                });
            }
        }
        Some("all") => {
            if let Some(Value::Array(items)) = object.get("conditions") {
                for item in items {
                    collect_conditions(item, output);
                }
            }
        }
        _ => output.push(Condition::Unknown {
            description: value.to_string(),
        }),
    }
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

fn lower_composable_operand(
    engine: &mut RustEngineProxy,
    expression: &str,
    verbosity: StepVerbosity,
) -> Result<(String, Vec<Step>, Vec<String>), ErrorResponse> {
    let call = processing::input::root_call(expression, "方程操作数").map_err(message)?;
    if !call
        .as_ref()
        .is_some_and(processing::composition::is_candidate)
    {
        return Ok((expression.into(), Vec::new(), Vec::new()));
    }
    let Some(result) =
        processing::composition::execute_steps(engine, expression, verbosity).map_err(message)?
    else {
        return Ok((expression.into(), Vec::new(), Vec::new()));
    };
    if result.status == processing::composition::CompositionStatus::Unsupported {
        return Err(invalid_input(
            result
                .reason
                .unwrap_or_else(|| "方程中的组合运算不受支持".into()),
        ));
    }
    Ok((result.value, result.steps, result.arbitrary_constants))
}

fn dispatch_expression_with_engine(
    request: ProcessExpressionRequest,
    engine: &mut RustEngineProxy,
    analyzed: &AnalyzedInput,
) -> Result<DispatchExpressionResult, ErrorResponse> {
    let call = &analyzed.root_call;
    let verbosity = parse_verbosity(&request.verbosity)?;
    if let Some(call) = call
        .as_ref()
        .filter(|call| call.head == "Limit" && call.arguments.len() == 2)
    {
        let expression = &call.arguments[0];
        let at = &call.arguments[1];
        let result =
            processing::limits::limit(&mut *engine, expression, "x", at, LimitDirection::Both)
                .map_err(message)?;
        let steps = if request.steps {
            processing::limits::limit_steps_with_verbosity(
                &mut *engine,
                expression,
                "x",
                at,
                LimitDirection::Both,
                verbosity,
            )
            .map_err(message)?
        } else {
            Vec::new()
        };
        return unified_result(
            "limit",
            "极限",
            result.value.clone(),
            result.tex.clone(),
            steps,
            &result,
        );
    }
    let conventional_bodied = call.as_ref().is_some_and(|call| {
        (call.head == "Limit" && call.arguments.len() == 2)
            || (call.head == "Taylor" && call.arguments.len() == 3)
    });
    if (request.steps || conventional_bodied)
        && call
            .as_ref()
            .is_some_and(processing::composition::is_candidate)
    {
        if let Some(mut result) =
            processing::composition::execute_steps(&mut *engine, &request.expression, verbosity)
                .map_err(message)?
        {
            if !request.steps {
                result.steps.clear();
            }
            return unified_result(
                "composition",
                "组合运算",
                result.value.clone(),
                result.tex.clone(),
                result.steps.clone(),
                &result,
            );
        }
    }
    if let Some(call) = call {
        match (call.head.as_str(), call.arguments.as_slice()) {
            ("D", [variable, expression]) | ("Deriv", [variable, expression]) => {
                let derivative_request = processing::derivatives::DerivativeRequest {
                    variable: variable.clone(),
                    order: 1,
                };
                let computation = processing::derivatives::derivative_computation(
                    &mut *engine,
                    expression,
                    variable,
                    1,
                )
                .map_err(message)?;
                let result =
                    processing::derivatives::derivative_result(&computation, &derivative_request);
                let output = computation
                    .subject()
                    .expect("derivative has an output object");
                let expression = output.print_source();
                let tex = processing::input::strip_tex_delimiters(
                    &engine
                        .render_tex_batch(std::slice::from_ref(&expression))
                        .map_err(message)?[0],
                );
                let steps = if request.steps {
                    processing::steps::render_rule_trace(
                        &mut *engine,
                        computation
                            .trace
                            .as_ref()
                            .expect("derivative records a trace"),
                        verbosity,
                    )
                    .map_err(message)?
                } else {
                    Vec::new()
                };
                return unified_result("derivative", "导数", expression, tex, steps, &result);
            }
            ("D", [variable, order, expression]) | ("Deriv", [variable, order, expression]) => {
                let order = order
                    .parse::<u32>()
                    .map_err(|_| invalid_input("导数阶数必须是非负整数"))?;
                let derivative_request = processing::derivatives::DerivativeRequest {
                    variable: variable.clone(),
                    order,
                };
                let computation = processing::derivatives::derivative_computation(
                    &mut *engine,
                    expression,
                    variable,
                    order,
                )
                .map_err(message)?;
                let result =
                    processing::derivatives::derivative_result(&computation, &derivative_request);
                let output = computation
                    .subject()
                    .expect("derivative has an output object");
                let expression = output.print_source();
                let tex = processing::input::strip_tex_delimiters(
                    &engine
                        .render_tex_batch(std::slice::from_ref(&expression))
                        .map_err(message)?[0],
                );
                let steps = if request.steps {
                    processing::steps::render_rule_trace(
                        &mut *engine,
                        computation
                            .trace
                            .as_ref()
                            .expect("derivative records a trace"),
                        verbosity,
                    )
                    .map_err(message)?
                } else {
                    Vec::new()
                };
                return unified_result("derivative", "导数", expression, tex, steps, &result);
            }
            (head @ ("ImproperIntegral" | "PrincipalValueIntegral"), arguments)
                if matches!(arguments.len(), 4 | 5) =>
            {
                let expression = &arguments[0];
                let variable = &arguments[1];
                let lower = &arguments[2];
                let upper = &arguments[3];
                let points = if arguments.len() == 5 {
                    list_or_single(&arguments[4], "奇点列表")?
                } else {
                    Vec::new()
                };
                let object_request = ImproperIntegralRequest {
                    expression: expression.clone(),
                    variable: variable.clone(),
                    lower: lower.clone(),
                    upper: upper.clone(),
                    singular_points: points,
                };
                let step_verbosity = request.steps.then_some(verbosity);
                if head == "ImproperIntegral" {
                    if let Some(lowered) = processing::intrinsics::try_lower_improper_integral(
                        &mut *engine,
                        &object_request,
                        step_verbosity,
                    )
                    .map_err(message)?
                    {
                        return unified_result(
                            "intrinsic",
                            "原生特殊函数",
                            lowered.value.clone(),
                            lowered.tex.clone(),
                            lowered.steps.clone(),
                            &lowered,
                        );
                    }
                }
                let result = if head == "PrincipalValueIntegral" {
                    processing::improper_integrals::principal_value(
                        &mut *engine,
                        &object_request,
                        step_verbosity,
                    )
                } else {
                    processing::improper_integrals::evaluate(
                        &mut *engine,
                        &object_request,
                        step_verbosity,
                    )
                }
                .map_err(message)?;
                return unified_result(
                    "defined_object",
                    if head == "PrincipalValueIntegral" {
                        "Cauchy 主值"
                    } else {
                        "反常积分"
                    },
                    result.value.clone(),
                    result.tex.clone(),
                    result.steps.clone(),
                    &result,
                );
            }
            ("Integrate", [variable, lower, upper, expression])
                if lower.contains("Infinity") || upper.contains("Infinity") =>
            {
                let object_request = ImproperIntegralRequest {
                    expression: expression.clone(),
                    variable: variable.clone(),
                    lower: lower.clone(),
                    upper: upper.clone(),
                    singular_points: Vec::new(),
                };
                if let Some(lowered) = processing::intrinsics::try_lower_improper_integral(
                    &mut *engine,
                    &object_request,
                    request.steps.then_some(verbosity),
                )
                .map_err(message)?
                {
                    return unified_result(
                        "intrinsic",
                        "原生特殊函数",
                        lowered.value.clone(),
                        lowered.tex.clone(),
                        lowered.steps.clone(),
                        &lowered,
                    );
                }
                let result = processing::improper_integrals::evaluate(
                    &mut *engine,
                    &object_request,
                    request.steps.then_some(verbosity),
                )
                .map_err(message)?;
                return unified_result(
                    "defined_object",
                    "反常积分",
                    result.value.clone(),
                    result.tex.clone(),
                    result.steps.clone(),
                    &result,
                );
            }
            ("Integrate", [variable, expression]) => {
                let arbitrary_constant = processing::semantic::display_arbitrary_constants(
                    &analyzed.semantic.symbols,
                    1,
                )
                .into_iter()
                .next()
                .expect("one arbitrary constant was requested");
                if request.steps {
                    let result = processing::steps::derive_antiderivative_family_with_verbosity(
                        &mut *engine,
                        expression,
                        variable,
                        arbitrary_constant,
                        verbosity,
                    )
                    .map_err(message)?;
                    return unified_result(
                        "integral",
                        "不定积分",
                        result.result.expression.clone(),
                        result.result.tex.clone(),
                        result.steps.clone(),
                        &result,
                    );
                }
                let evaluated = engine.eval(&request.expression).map_err(message)?;
                let result = processing::steps::antiderivative_family(
                    evaluated.expr.to_string(),
                    evaluated.tex.trim_matches('$').to_string(),
                    variable,
                    arbitrary_constant,
                );
                return unified_result(
                    "integral",
                    "不定积分",
                    result.expression.clone(),
                    result.tex.clone(),
                    vec![],
                    &result,
                );
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
                    let result = processing::limits::limit(
                        &mut *engine,
                        expression,
                        variable,
                        at,
                        LimitDirection::Both,
                    )
                    .map_err(message)?;
                    let steps = processing::limits::limit_steps_with_verbosity(
                        &mut *engine,
                        expression,
                        variable,
                        at,
                        LimitDirection::Both,
                        verbosity,
                    )
                    .map_err(message)?;
                    return unified_result(
                        "limit",
                        "极限",
                        result.value.clone(),
                        result.tex.clone(),
                        steps,
                        &result,
                    );
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
                    let result = processing::limits::limit(
                        &mut *engine,
                        expression,
                        variable,
                        at,
                        direction,
                    )
                    .map_err(message)?;
                    let steps = processing::limits::limit_steps_with_verbosity(
                        &mut *engine,
                        expression,
                        variable,
                        at,
                        direction,
                        verbosity,
                    )
                    .map_err(message)?;
                    return unified_result(
                        "limit",
                        "极限",
                        result.value.clone(),
                        result.tex.clone(),
                        steps,
                        &result,
                    );
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
            ("Solve", [equations, variables]) => {
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
            ("=" | "==", [left, right]) => {
                let (left, mut lowering_steps, mut generated_constants) =
                    lower_composable_operand(engine, left, verbosity)?;
                let (right, right_steps, right_constants) =
                    lower_composable_operand(engine, right, verbosity)?;
                lowering_steps.extend(right_steps);
                generated_constants.extend(right_constants);
                let lowered_equation = format!("({left})==({right})");
                let equations = [&lowered_equation[..]];
                let preferred_variables = processing::input::validate_symbol(&left, "等式左侧")
                    .is_ok()
                    .then_some([left.as_str()]);
                let variables = preferred_variables
                    .as_ref()
                    .map(|variables| &variables[..])
                    .unwrap_or(&[]);
                let solved = processing::equations::solve(&mut *engine, &equations, variables)
                    .map_err(message)?;
                let (solved, equation_steps) = if request.steps && solved.variables.len() == 1 {
                    let stepped = processing::equations::solve_steps_with_verbosity(
                        &mut *engine,
                        &lowered_equation,
                        &solved.variables[0],
                        verbosity,
                    )
                    .map_err(message)?;
                    (stepped.result, stepped.steps)
                } else {
                    (solved, vec![])
                };
                let steps = if request.steps {
                    lowering_steps.extend(equation_steps);
                    lowering_steps
                } else {
                    Vec::new()
                };
                let mut output = unified_result(
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
                )?;
                if let Value::Object(data) = &mut output.data {
                    data.insert(
                        "semantic_expression".into(),
                        Value::String(lowered_equation),
                    );
                    data.insert(
                        "arbitrary_constants".into(),
                        serde_json::to_value(generated_constants).map_err(|error| {
                            invalid_input(format!("任意常数序列化失败: {error}"))
                        })?,
                    );
                }
                return Ok(output);
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

pub fn process_expression_with_engine(
    request: ProcessExpressionRequest,
    engine: &mut RustEngineProxy,
) -> Result<ProcessExpressionResult, ErrorResponse> {
    let analyzed =
        processing::semantic::analyze_input(&request.expression, "表达式").map_err(message)?;
    let semantic_input = match analyzed.root_call.as_ref() {
        Some(call) if call.head == "Limit" && call.arguments.len() == 2 => {
            processing::semantic::analyze_input(
                &format!("Limit(x,{}){}", call.arguments[1], call.arguments[0]),
                "极限表达式",
            )
            .map_err(message)?
            .semantic
        }
        Some(call) if call.head == "Limit" && call.arguments.len() == 4 => {
            processing::semantic::analyze_input(
                &format!(
                    "Limit({},{}){}",
                    call.arguments[0], call.arguments[1], call.arguments[3]
                ),
                "极限表达式",
            )
            .map_err(message)?
            .semantic
        }
        _ => analyzed.semantic.clone(),
    };
    let result = dispatch_expression_with_engine(request, engine, &analyzed)?;
    let projected_input = result
        .data
        .get("semantic_expression")
        .and_then(Value::as_str)
        .map(|expression| processing::semantic::analyze_input(expression, "降低后的表达式"))
        .transpose()
        .map_err(message)?;
    let semantic = project_domain_semantic(
        projected_input
            .as_ref()
            .map(|input| &input.semantic)
            .unwrap_or(&semantic_input),
        &result,
    )?;
    let outcome = result_metadata(&result, semantic.exactness)?;
    Ok(ProcessExpressionResult {
        kind: result.kind,
        title: result.title,
        expression: result.expression,
        tex: result.tex,
        steps: result.steps,
        data: result.data,
        semantic,
        outcome,
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
            set_assumption,
            clear_assumptions,
            get_assumptions,
            process_expression,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use processing::binding::SymbolRole;

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
        assert!(derivative.semantic.symbols.is_empty());
        assert_eq!(derivative.semantic.bound_symbols, ["x".to_string()]);

        let composed =
            process_expression_with_engine(request("D(x)Integrate(x)x*Exp(x)", true), &mut engine)
                .unwrap();
        assert_eq!(composed.kind, "composition");
        let integration = composed
            .steps
            .iter()
            .position(|step| step.rule == "method-parts")
            .unwrap();
        let outer_derivative = composed
            .steps
            .iter()
            .position(|step| step.why.contains("外层求导"))
            .unwrap();
        assert!(integration < outer_derivative);
        assert!(composed
            .semantic
            .symbol_identities
            .iter()
            .all(|identity| identity.role != SymbolRole::ArbitraryConstant));
        assert!(composed
            .steps
            .iter()
            .any(|step| { step.rule == "antiderivative-family" && step.expr.contains(" + C)") }));

        let repeated_integral =
            process_expression_with_engine(request("Integrate(x)Integrate(x)x", true), &mut engine)
                .unwrap();
        assert_eq!(repeated_integral.kind, "composition");
        assert!(repeated_integral.expression.contains('C'));
        assert!(repeated_integral.expression.contains("C1"));
        assert_eq!(
            repeated_integral
                .semantic
                .symbol_identities
                .iter()
                .filter(|identity| identity.role == SymbolRole::ArbitraryConstant)
                .count(),
            2
        );

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
        assert!(ode.semantic.symbols.is_empty());
        assert_eq!(ode.semantic.bound_symbols, ["x"]);
        assert!(ode.semantic.symbol_identities.iter().any(|identity| {
            identity.name == "C" && identity.role == SymbolRole::ArbitraryConstant
        }));

        let oscillatory =
            process_expression_with_engine(request("OdeSolve(y''+2*y'+5*y==0)", true), &mut engine)
                .unwrap();
        assert!(!oscillatory.expression.contains("Complex("));
        assert!(oscillatory.expression.contains("Cos"));
        assert_eq!(
            oscillatory.data["result"]["preferred_representation"],
            "real_basis"
        );
        assert_eq!(
            oscillatory.data["result"]["representations"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(!oscillatory
            .semantic
            .symbol_identities
            .iter()
            .any(|identity| identity.name == "y" || identity.name.starts_with("y'")));

        let equations = process_expression_with_engine(
            request("Solve({x+y==3,x-y==1},{x,y})", false),
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

        let improper = process_expression_with_engine(
            request("Integrate(x,0,Infinity)Exp(-x)", false),
            &mut engine,
        )
        .unwrap();
        assert_eq!(improper.kind, "intrinsic");
        assert_eq!(improper.expression, "1");

        let parameterized_gamma = process_expression_with_engine(
            request("Integrate(t,0,Infinity)t^(1/x-1)*Exp(-t)", true),
            &mut engine,
        )
        .unwrap();
        assert_eq!(parameterized_gamma.kind, "intrinsic");
        assert_eq!(
            parameterized_gamma.outcome.conditionality,
            processing::protocol::Conditionality::Conditional
        );

        let explicit_gamma = process_expression_with_engine(
            request("ImproperIntegral(t^(a-1)*Exp(-t),t,0,Infinity)", true),
            &mut engine,
        )
        .unwrap();
        assert_eq!(explicit_gamma.kind, "intrinsic");
        assert!(explicit_gamma.expression.contains("Gamma"));
        assert_eq!(explicit_gamma.semantic.symbols, ["a".to_string()]);
        assert_eq!(explicit_gamma.semantic.bound_symbols, ["t".to_string()]);
        assert_eq!(
            explicit_gamma.outcome.conditionality,
            processing::protocol::Conditionality::Conditional
        );

        let symbolic_integral = process_expression_with_engine(
            request("Integrate(x)(theta+theta1)*x^2/Sqrt(4-x^2)", true),
            &mut engine,
        )
        .unwrap();
        assert_eq!(symbolic_integral.kind, "integral");
        assert!(!symbolic_integral.expression.is_empty());
        assert!(!symbolic_integral.tex.is_empty());
        assert!(!symbolic_integral.steps.is_empty());
        assert_eq!(
            symbolic_integral.semantic.symbols,
            ["theta".to_string(), "theta1".to_string()]
        );
        assert_eq!(symbolic_integral.semantic.bound_symbols, ["x".to_string()]);
        assert_eq!(symbolic_integral.semantic.kind, ValueKind::FunctionFamily);
        assert!(symbolic_integral.expression.ends_with(" + C)"));
        assert_eq!(
            symbolic_integral.steps.last().unwrap().rule,
            "antiderivative-family"
        );
        assert_eq!(
            symbolic_integral.data["result"]["representative"],
            symbolic_integral.steps[symbolic_integral.steps.len() - 2].expr
        );
        assert!(
            symbolic_integral
                .semantic
                .symbol_identities
                .iter()
                .any(|identity| identity.name == "C"
                    && identity.role == SymbolRole::ArbitraryConstant)
        );

        let colliding_constant =
            process_expression_with_engine(request("Integrate(x)C*x", false), &mut engine).unwrap();
        assert_eq!(colliding_constant.kind, "integral");
        assert!(colliding_constant.expression.contains("C1"));
        assert_eq!(colliding_constant.semantic.symbols, ["C"]);
        assert!(colliding_constant
            .semantic
            .symbol_identities
            .iter()
            .any(|identity| {
                identity.name == "C1" && identity.role == SymbolRole::ArbitraryConstant
            }));

        let direct_gamma =
            process_expression_with_engine(request("Gamma(3)", false), &mut engine).unwrap();
        assert_eq!(direct_gamma.kind, "evaluation");
        assert_eq!(direct_gamma.expression, "2");

        let polar_template = process_expression_with_engine(
            request("PolarIntegral(x^2+y^2,x,y,r,t,0,1,0,2*Pi)", true),
            &mut engine,
        )
        .unwrap();
        assert_eq!(polar_template.kind, "polar_integral");
        assert_eq!(
            polar_template.outcome.resolution,
            processing::protocol::ResolutionState::Solved
        );
        assert!(!polar_template.steps.is_empty());

        let gaussian_disk = process_expression_with_engine(
            request(
                "PolarIntegral(Exp(-(x^2+y^2)),x,y,r,theta,0,2,0,2*Pi)",
                true,
            ),
            &mut engine,
        )
        .unwrap();
        assert_eq!(gaussian_disk.kind, "polar_integral");
        assert_eq!(
            gaussian_disk.outcome.resolution,
            processing::protocol::ResolutionState::Solved
        );
        assert_eq!(
            gaussian_disk.data["result"]["integral"]["status"],
            "evaluated"
        );
        assert!(!gaussian_disk.data["result"]["transformed_integrand"]
            .as_str()
            .unwrap()
            .contains("theta"));

        let principal_value = process_expression_with_engine(
            request("PrincipalValueIntegral(1/x,x,-1,1,{0})", false),
            &mut engine,
        )
        .unwrap();
        assert_eq!(principal_value.expression, "0");

        let divergent = process_expression_with_engine(
            request("ImproperIntegral(1/x,x,-1,1,{0})", false),
            &mut engine,
        )
        .unwrap();
        assert_eq!(
            divergent.outcome.reason,
            Some(processing::protocol::OutcomeReason::Divergent)
        );
    }

    #[test]
    fn unified_input_accepts_two_argument_limit_with_default_x() {
        let mut engine = RustEngineProxy::spawn().unwrap();
        for steps in [true, false] {
            let result =
                process_expression_with_engine(request("Limit(x,0)", steps), &mut engine).unwrap();
            assert_eq!(result.kind, "limit");
            assert_eq!(result.expression, "0");
            assert_eq!(result.steps.is_empty(), !steps);
            assert_eq!(result.semantic.bound_symbols, ["x"]);
            assert!(result.semantic.symbols.is_empty());
            assert_eq!(
                result.outcome.support,
                processing::protocol::SupportState::Supported
            );
            assert_eq!(
                result.outcome.resolution,
                processing::protocol::ResolutionState::Solved
            );
        }
    }

    #[test]
    fn unified_input_accepts_three_argument_taylor_with_default_x() {
        let mut engine = RustEngineProxy::spawn().unwrap();
        for steps in [true, false] {
            let result =
                process_expression_with_engine(request("Taylor(Exp(x),0,6)", steps), &mut engine)
                    .unwrap();
            assert_eq!(result.kind, "composition");
            assert!(result.expression.contains("x ^ 6"), "{}", result.expression);
            assert_eq!(result.steps.is_empty(), !steps);
            assert!(result.semantic.bound_symbols.is_empty());
            assert_eq!(result.semantic.symbols, ["x"]);
            assert_eq!(
                result.outcome.resolution,
                processing::protocol::ResolutionState::Solved
            );
        }
    }

    #[test]
    fn unified_equation_lowers_structured_operands_before_solving() {
        let mut engine = RustEngineProxy::spawn().unwrap();
        let result = process_expression_with_engine(
            request("y'==(Integrate(x)Taylor(Exp(x),0,2))", true),
            &mut engine,
        )
        .unwrap();
        assert_eq!(result.kind, "equation");
        assert!(
            !result.expression.contains("Integrate"),
            "{}",
            result.expression
        );
        assert!(
            !result.expression.contains("Taylor"),
            "{}",
            result.expression
        );
        assert!(result.expression.contains("C"), "{}", result.expression);
        assert!(
            result.expression.contains("variable: \"y'\""),
            "{}",
            result.expression
        );
        assert!(result
            .steps
            .iter()
            .any(|step| step.rule == "compose_taylor"));
        assert!(result
            .steps
            .iter()
            .any(|step| step.rule == "antiderivative-family"));
        assert!(result.semantic.bound_symbols.is_empty());
        assert!(result.semantic.symbols.contains(&"x".into()));
        assert!(result.semantic.symbol_identities.iter().any(|identity| {
            identity.name == "C" && identity.role == SymbolRole::ArbitraryConstant
        }));
        assert_eq!(
            result.outcome.resolution,
            processing::protocol::ResolutionState::Solved
        );
    }

    #[test]
    fn unified_ode_composition_uses_normalized_solutions_and_result_semantics() {
        let mut engine = RustEngineProxy::spawn().unwrap();
        let first =
            process_expression_with_engine(request("D(x)OdeSolve(y'==y)", true), &mut engine)
                .unwrap();
        assert_eq!(first.kind, "composition");
        assert!(
            !first.expression.contains("C179"),
            "{:#?}",
            first.expression
        );
        assert!(!first
            .semantic
            .symbols
            .iter()
            .any(|name| name.starts_with('y')));
        assert_eq!(first.semantic.bound_symbols, ["x"]);
        assert!(first.semantic.symbol_identities.iter().any(|identity| {
            identity.name == "C" && identity.role == SymbolRole::ArbitraryConstant
        }));

        let second = process_expression_with_engine(
            request("D(x)OdeSolve(y''+4*y==Sin(x))", true),
            &mut engine,
        )
        .unwrap();
        assert_eq!(second.kind, "composition");
        assert!(
            !second.expression.contains("Deriv(x,y"),
            "{}",
            second.expression
        );
        assert!(!second.expression.contains("y(2)"), "{}", second.expression);
        assert!(!second
            .semantic
            .symbols
            .iter()
            .any(|name| name.starts_with('y')));

        let transformed =
            process_expression_with_engine(request("D(x)Simplify((x+x)/2)", true), &mut engine)
                .unwrap();
        assert_eq!(transformed.kind, "composition");
        assert_eq!(transformed.expression, "1");
        assert!(transformed
            .steps
            .iter()
            .any(|step| step.rule == "compose_algebra_transform"));

        let apart =
            process_expression_with_engine(request("Apart((x+1)/(x^2-1),x)", true), &mut engine)
                .unwrap();
        assert_eq!(apart.kind, "algebra");
        assert!(!apart.expression.contains("List"), "{}", apart.expression);

        let limited = process_expression_with_engine(
            request("D(x)Limit(t,0)(Sin(t)/t+x^2)", true),
            &mut engine,
        )
        .unwrap();
        assert_eq!(limited.kind, "composition");
        assert_eq!(limited.expression, "2*x");
        assert_eq!(limited.semantic.bound_symbols, ["x"]);
    }

    #[test]
    fn unified_input_preserves_unlowered_structured_compositions() {
        let mut engine = RustEngineProxy::spawn().unwrap();
        for expression in [
            "D(x)Solve({x==1},{x})",
            "Factor(DoubleIntegral(x+y,y,0,x,x,0,1))",
            "N(MatrixSolve({{1,0},{0,1}},{1,2}),10)",
        ] {
            let result =
                process_expression_with_engine(request(expression, true), &mut engine).unwrap();
            assert_eq!(result.kind, "composition", "{expression}");
            assert_eq!(result.expression, expression);
            assert_eq!(
                result.outcome.support,
                processing::protocol::SupportState::Supported,
                "{expression}"
            );
            assert_eq!(
                result.outcome.resolution,
                processing::protocol::ResolutionState::Unresolved,
                "{expression}"
            );
            assert_eq!(result.steps[0].rule, "held-operator-application");
            assert_eq!(result.semantic.kind, ValueKind::Unevaluated);
        }
    }

    #[test]
    fn unified_outcome_distinguishes_reasons_and_registered_conditions() {
        let make = |expression: &str, data: Value| DispatchExpressionResult {
            kind: "test".into(),
            title: "test".into(),
            expression: expression.into(),
            tex: String::new(),
            steps: Vec::new(),
            data,
        };
        let unresolved = result_metadata(
            &make("Integrate(x)f(x)", Value::Null),
            processing::semantic::Exactness::Unknown,
        )
        .unwrap();
        assert_eq!(
            unresolved.reason,
            Some(processing::protocol::OutcomeReason::AlgorithmUncovered)
        );

        let absent = result_metadata(
            &make("Undefined", serde_json::json!({"status": "does_not_exist"})),
            processing::semantic::Exactness::Exact,
        )
        .unwrap();
        assert_eq!(
            absent.resolution,
            processing::protocol::ResolutionState::NoResult
        );

        let conditional = result_metadata(
            &make(
                "Gamma(a)",
                serde_json::json!({
                    "status": "converged",
                    "conditions": [{
                        "kind": "relation",
                        "left": "Re(a)",
                        "relation": "greater_than",
                        "right": "0"
                    }]
                }),
            ),
            processing::semantic::Exactness::Symbolic,
        )
        .unwrap();
        assert_eq!(
            conditional.conditionality,
            processing::protocol::Conditionality::Conditional
        );
        assert!(matches!(
            conditional.conditions.conditions(),
            [processing::protocol::Condition::RealPartPositive { expression }] if expression == "a"
        ));
    }
}
