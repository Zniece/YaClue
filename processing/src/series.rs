//! Structured finite-sum and infinite-series APIs.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{
    analyze_expression, strip_tex_delimiters, validate_expression, validate_symbol,
};
use crate::steps::{render_events, Step, StepEvent, StepImportance, StepVerbosity};
use serde::Serialize;

use crate::protocol::{Condition, ConditionSet, OutcomeReason, ResultMetadata};
use crate::semantic::{Exactness, ValueKind};
#[cfg(test)]
use crate::semantic_core::object_from_source;
use crate::semantic_core::{
    CapabilitySet, Computation, ComputationOutput, NormalizationLevel, NormalizationMetadata,
    NormalizationMode, ObjectDelta, OperatorId, RuleEvent, RuleImportance, RulePayload,
    RulePresentation, RuleTrace, SemanticInterpretation, SemanticOperation, SemanticState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SumRequest {
    pub variable: String,
    pub lower: String,
    pub upper: String,
}

impl SumRequest {
    fn validate(&self) -> Result<(), EngineError> {
        validate_symbol(&self.variable, "求和变量")?;
        validate_expression(&self.lower, "求和下限")?;
        validate_expression(&self.upper, "求和上限")
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SumOperation;

impl SemanticOperation<SumRequest> for SumOperation {
    fn compute(
        &self,
        engine: &mut dyn Engine,
        input: &crate::semantic_core::MathematicalObject,
        request: &SumRequest,
    ) -> Result<Computation, EngineError> {
        request.validate()?;
        if !input
            .semantics
            .capabilities
            .contains(crate::semantic_core::ObjectCapability::SumSeries)
        {
            return Err(EngineError::InvalidInput("该数学对象不具备求和能力".into()));
        }
        let term = input.print_source();
        let held_source = format!(
            "Sum({},{},{},{term})",
            request.variable, request.lower, request.upper
        );
        let (source, metadata, held, no_value, steps) = if request.upper == "Infinity" {
            let stepped = infinite_series_steps_with_verbosity(
                engine,
                &term,
                &request.variable,
                &request.lower,
                StepVerbosity::Detailed,
            )?;
            let conditions = ConditionSet::new(
                stepped
                    .result
                    .conditions
                    .iter()
                    .map(|description| Condition::Unknown {
                        description: description.clone(),
                    })
                    .collect::<Vec<_>>(),
            )?;
            let (metadata, held, no_value) = match stepped.result.status {
                SeriesStatus::AbsolutelyConvergent | SeriesStatus::ConditionallyConvergent => (
                    ResultMetadata::solved(Exactness::Symbolic, conditions),
                    false,
                    false,
                ),
                SeriesStatus::Conditional => (
                    ResultMetadata::solved(Exactness::Symbolic, conditions),
                    false,
                    false,
                ),
                SeriesStatus::Divergent => (
                    ResultMetadata::no_result(Exactness::Symbolic, OutcomeReason::Divergent),
                    false,
                    true,
                ),
                SeriesStatus::Inconclusive => (
                    ResultMetadata::unresolved(
                        Exactness::Symbolic,
                        OutcomeReason::AlgorithmUncovered,
                    ),
                    true,
                    false,
                ),
            };
            (
                stepped.result.value.unwrap_or_else(|| held_source.clone()),
                metadata,
                held,
                no_value,
                stepped.steps,
            )
        } else {
            let result = finite_sum(
                engine,
                &term,
                &request.variable,
                &request.lower,
                &request.upper,
            )?;
            let (metadata, held, no_value) = match result.status {
                SumStatus::Evaluated => (
                    ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty()),
                    false,
                    false,
                ),
                SumStatus::Unresolved => (
                    ResultMetadata::unresolved(
                        Exactness::Symbolic,
                        OutcomeReason::AlgorithmUncovered,
                    ),
                    true,
                    false,
                ),
                SumStatus::Undefined => (
                    ResultMetadata::no_result(
                        Exactness::Symbolic,
                        OutcomeReason::MathematicalAbsence,
                    ),
                    false,
                    true,
                ),
            };
            let presentation = Step {
                rule: if held { "hold-sum" } else { "finite-sum" }.into(),
                expr: if held {
                    held_source.clone()
                } else {
                    result.value.clone()
                },
                why: if held {
                    "保留尚无闭式结果的求和对象。".into()
                } else {
                    "计算有限求和。".into()
                },
                tex: result.tex.clone(),
                importance: StepImportance::Key,
            };
            (
                if held {
                    held_source.clone()
                } else {
                    result.value
                },
                metadata,
                held,
                no_value,
                vec![presentation],
            )
        };

        let mut semantics = SemanticState {
            kind: if held || no_value {
                ValueKind::Unevaluated
            } else {
                crate::input::with_parse_env(|env| {
                    let parsed = yacas_rs::parser::parse_expression(env, &format!("{source};"))
                        .expect("sum output parses")
                        .expect("sum output exists");
                    crate::semantic::analyze_tree(env, &parsed).semantic.kind
                })
            },
            interpretation: if held {
                SemanticInterpretation::HeldApplication {
                    operator: "Sum".into(),
                }
            } else {
                SemanticInterpretation::PlainExpression
            },
            metadata,
            capabilities: if no_value {
                CapabilitySet::empty()
            } else {
                CapabilitySet::symbolic_expression()
            },
            requirements: Vec::new(),
        };
        let parsed = crate::semantic_core::parse_engine_expression(&source)?;
        if held {
            crate::semantic_core::promote_held_application(
                "Sum",
                &parsed.raw_expression(),
                &mut semantics,
            )?;
        }
        let mut output = input.clone();
        output.apply(ObjectDelta {
            expression: Some(parsed.raw_expression()),
            semantics: Some(semantics),
            overlay: None,
            normalization: (!held && !no_value).then_some(NormalizationMetadata {
                level: NormalizationLevel::Domain,
                assumptions: Vec::new(),
                mode: NormalizationMode::Operation(OperatorId::Sum),
            }),
        });
        let events = steps
            .into_iter()
            .map(|step| RuleEvent {
                rule: step.rule,
                input: input.reference(None),
                additional_inputs: Vec::new(),
                output: output.reference(None),
                bindings: vec![
                    ("variable".into(), request.variable.clone()),
                    ("lower".into(), request.lower.clone()),
                    ("upper".into(), request.upper.clone()),
                ],
                conditions: output.semantics.metadata.conditions.conditions().to_vec(),
                payload: RulePayload::Structural,
                importance: match step.importance {
                    StepImportance::Routine => RuleImportance::Routine,
                    StepImportance::Normal => RuleImportance::Normal,
                    StepImportance::Key => RuleImportance::Key,
                },
                presentation: Some(RulePresentation {
                    expression: step.expr,
                    explanation: step.why,
                    tex_override: Some(step.tex),
                }),
            })
            .collect();
        Ok(Computation {
            output: if no_value {
                ComputationOutput::NoValue(output)
            } else if held {
                ComputationOutput::Held(output)
            } else {
                ComputationOutput::Value(output)
            },
            trace: Some(RuleTrace { events }),
            certificates: Vec::new(),
            effects: Vec::new(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SumStatus {
    Evaluated,
    Undefined,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FiniteSumResult {
    pub variable: String,
    pub from: String,
    pub to: String,
    pub term: String,
    pub value: String,
    pub status: SumStatus,
    pub tex: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SeriesStatus {
    AbsolutelyConvergent,
    Divergent,
    ConditionallyConvergent,
    /// Convergence depends on the returned conditions.
    Conditional,
    Inconclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConvergenceMethod {
    Geometric,
    PSeries,
    Alternating,
    Comparison,
    Term,
    Ratio,
    Root,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InfiniteSeriesResult {
    pub variable: String,
    pub from: String,
    pub term: String,
    pub status: SeriesStatus,
    pub method: ConvergenceMethod,
    pub test_value: Option<String>,
    pub conditions: Vec<String>,
    /// A closed form is exposed only when convergence is established.
    pub value: Option<String>,
    pub tex: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InfiniteSeriesStepResult {
    pub result: InfiniteSeriesResult,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerSeriesStatus {
    Convergent,
    Inconclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PowerSeriesResult {
    pub index: String,
    pub variable: String,
    pub center: String,
    pub coefficient: String,
    pub status: PowerSeriesStatus,
    pub method: ConvergenceMethod,
    pub radius: Option<String>,
    pub left_endpoint: Option<String>,
    pub right_endpoint: Option<String>,
    pub left_status: Option<SeriesStatus>,
    pub right_status: Option<SeriesStatus>,
    pub left_included: bool,
    pub right_included: bool,
    pub tex: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PowerSeriesStepResult {
    pub result: PowerSeriesResult,
    pub steps: Vec<Step>,
}

pub fn finite_sum(
    engine: &mut dyn Engine,
    term: &str,
    variable: &str,
    from: &str,
    to: &str,
) -> Result<FiniteSumResult, EngineError> {
    validate_request(term, variable, from)?;
    validate_expression(to, "求和上限")?;
    let evaluated = engine.eval(&format!("Sum({variable},{from},{to},{term})"))?;
    let value = evaluated.expr.to_string();
    let status = if matches!(&evaluated.expr, Expr::Symbol(symbol) if symbol == "Undefined") {
        SumStatus::Undefined
    } else if matches!(&evaluated.expr, Expr::Call { head, .. } if head == "Sum") {
        SumStatus::Unresolved
    } else {
        SumStatus::Evaluated
    };
    Ok(FiniteSumResult {
        variable: variable.into(),
        from: from.into(),
        to: to.into(),
        term: term.into(),
        value,
        status,
        tex: strip_tex_delimiters(&evaluated.tex),
    })
}

pub fn infinite_series(
    engine: &mut dyn Engine,
    term: &str,
    variable: &str,
    from: &str,
) -> Result<InfiniteSeriesResult, EngineError> {
    infinite_series_internal(engine, term, variable, from, true)
}

fn infinite_series_internal(
    engine: &mut dyn Engine,
    term: &str,
    variable: &str,
    from: &str,
    render_value: bool,
) -> Result<InfiniteSeriesResult, EngineError> {
    validate_request(term, variable, from)?;
    if from.parse::<i64>().is_err() {
        return Err(EngineError::InvalidInput(
            "无穷级数下限必须是显式整数".into(),
        ));
    }
    let command = format!("SeriesConvergence({variable},{from},{term})");
    let (expression, tex) = if render_value {
        let evaluated = engine.eval(&command)?;
        (evaluated.expr, strip_tex_delimiters(&evaluated.tex))
    } else {
        (engine.eval_expr(&command)?, String::new())
    };
    let analysis = list(&expression, "收敛分析")?;
    if analysis.len() != 5 {
        return Err(EngineError::Parse("收敛分析字段数量错误".into()));
    }
    let status = parse_status(&analysis[0])?;
    let method = parse_method(&analysis[1])?;
    let test_value = (!matches!(&analysis[2], Expr::Symbol(value) if value == "Undefined"))
        .then(|| analysis[2].to_string());
    let conditions = list(&analysis[3], "收敛条件")?
        .iter()
        .map(ToString::to_string)
        .collect();
    let value = matches!(
        status,
        SeriesStatus::AbsolutelyConvergent | SeriesStatus::ConditionallyConvergent
    )
    .then(|| analysis[4].to_string())
    .filter(|value| value != "Undefined");
    Ok(InfiniteSeriesResult {
        variable: variable.into(),
        from: from.into(),
        term: term.into(),
        status,
        method,
        test_value,
        conditions,
        value,
        tex,
    })
}

pub fn infinite_series_steps(
    engine: &mut dyn Engine,
    term: &str,
    variable: &str,
    from: &str,
) -> Result<InfiniteSeriesStepResult, EngineError> {
    infinite_series_steps_with_verbosity(engine, term, variable, from, StepVerbosity::Detailed)
}

pub fn infinite_series_steps_with_verbosity(
    engine: &mut dyn Engine,
    term: &str,
    variable: &str,
    from: &str,
    verbosity: StepVerbosity,
) -> Result<InfiniteSeriesStepResult, EngineError> {
    let mut result = infinite_series_internal(engine, term, variable, from, false)?;
    let series = format!("Sum({variable},{from},Infinity,{term})");
    let mut events = vec![StepEvent::new(
        "series-start",
        &series,
        "建立无穷级数并检查通项与适用的收敛判别法。",
        StepImportance::Routine,
    )];
    events.push(StepEvent::new(
        method_rule(result.method),
        result.test_value.as_deref().unwrap_or(term),
        method_explanation(result.method),
        StepImportance::Key,
    ));
    for condition in &result.conditions {
        events.push(StepEvent::new(
            "series-condition",
            condition,
            "只有满足该条件时，当前判别结论成立。",
            StepImportance::Key,
        ));
    }
    events.push(StepEvent::new(
        "series-result",
        result.value.as_deref().unwrap_or(&series),
        series_status_explanation(result.status),
        StepImportance::Key,
    ));
    let steps = render_events(engine, events, verbosity)?;
    result.tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    Ok(InfiniteSeriesStepResult { result, steps })
}

/// Analyze `Sum(coefficient * (variable-center)^index)`.
pub fn power_series(
    engine: &mut dyn Engine,
    coefficient: &str,
    index: &str,
    variable: &str,
    center: &str,
) -> Result<PowerSeriesResult, EngineError> {
    power_series_internal(engine, coefficient, index, variable, center, true)
}

fn power_series_internal(
    engine: &mut dyn Engine,
    coefficient: &str,
    index: &str,
    variable: &str,
    center: &str,
    render_value: bool,
) -> Result<PowerSeriesResult, EngineError> {
    let coefficient_analysis = analyze_expression(coefficient, "幂级数系数")?;
    validate_symbol(index, "幂级数指标")?;
    validate_symbol(variable, "幂级数变量")?;
    if index == variable {
        return Err(EngineError::InvalidInput(
            "幂级数指标与展开变量必须不同".into(),
        ));
    }
    if coefficient_analysis
        .symbols
        .iter()
        .any(|symbol| symbol == variable)
    {
        return Err(EngineError::InvalidInput(
            "幂级数系数不能包含展开变量".into(),
        ));
    }
    let center_analysis = analyze_expression(center, "幂级数中心")?;
    if center_analysis
        .symbols
        .iter()
        .any(|symbol| symbol == index || symbol == variable)
    {
        return Err(EngineError::InvalidInput(
            "幂级数中心不能依赖指标或展开变量".into(),
        ));
    }
    let command = format!("PowerSeriesConvergence({index},{center},{coefficient})");
    let (expression, tex) = if render_value {
        let evaluated = engine.eval(&command)?;
        (evaluated.expr, strip_tex_delimiters(&evaluated.tex))
    } else {
        (engine.eval_expr(&command)?, String::new())
    };
    let fields = list(&expression, "幂级数分析")?;
    if fields.len() != 9 {
        return Err(EngineError::Parse("幂级数分析字段数量错误".into()));
    }
    let status = match string(&fields[0], "幂级数状态")? {
        "convergent" => PowerSeriesStatus::Convergent,
        "inconclusive" => PowerSeriesStatus::Inconclusive,
        value => return Err(EngineError::Parse(format!("未知幂级数状态: {value}"))),
    };
    let optional = |expr: &Expr| {
        (!matches!(expr, Expr::Symbol(value) if value == "Undefined")).then(|| expr.to_string())
    };
    Ok(PowerSeriesResult {
        index: index.into(),
        variable: variable.into(),
        center: center.into(),
        coefficient: coefficient.into(),
        status,
        method: parse_method(&fields[1])?,
        radius: optional(&fields[2]),
        left_endpoint: optional(&fields[3]),
        right_endpoint: optional(&fields[4]),
        left_status: parse_optional_status(&fields[5])?,
        right_status: parse_optional_status(&fields[6])?,
        left_included: parse_bool(&fields[7], "左端点包含状态")?,
        right_included: parse_bool(&fields[8], "右端点包含状态")?,
        tex,
    })
}

pub fn power_series_steps(
    engine: &mut dyn Engine,
    coefficient: &str,
    index: &str,
    variable: &str,
    center: &str,
) -> Result<PowerSeriesStepResult, EngineError> {
    power_series_steps_with_verbosity(
        engine,
        coefficient,
        index,
        variable,
        center,
        StepVerbosity::Detailed,
    )
}

pub fn power_series_steps_with_verbosity(
    engine: &mut dyn Engine,
    coefficient: &str,
    index: &str,
    variable: &str,
    center: &str,
    verbosity: StepVerbosity,
) -> Result<PowerSeriesStepResult, EngineError> {
    let mut result = power_series_internal(engine, coefficient, index, variable, center, false)?;
    let mut events = Vec::new();
    if let Some(radius) = &result.radius {
        events.push(StepEvent::new(
            "power-series-radius",
            radius,
            "对系数应用比值或根值判别，得到收敛半径。",
            StepImportance::Key,
        ));
    }
    for (side, endpoint, status, included) in [
        (
            "left",
            &result.left_endpoint,
            result.left_status,
            result.left_included,
        ),
        (
            "right",
            &result.right_endpoint,
            result.right_status,
            result.right_included,
        ),
    ] {
        if let (Some(endpoint), Some(status)) = (endpoint, status) {
            events.push(StepEvent::new(
                &format!("power-series-endpoint-{side}"),
                endpoint,
                if included {
                    match status {
                        SeriesStatus::ConditionallyConvergent => {
                            "代入该端点后级数条件收敛，因此包含此端点。"
                        }
                        _ => "代入该端点后级数收敛，因此包含此端点。",
                    }
                } else {
                    "代入该端点后级数发散或无法证明收敛，因此不包含此端点。"
                },
                StepImportance::Normal,
            ));
        }
    }
    let interval = power_series_interval(&result);
    events.push(StepEvent::new(
        "power-series-result",
        &interval,
        if result.status == PowerSeriesStatus::Convergent {
            "得到收敛半径，并逐一检查所有有限端点。"
        } else {
            "当前有界判别规则不足以确定收敛区间。"
        },
        StepImportance::Key,
    ));
    let steps = render_events(engine, events, verbosity)?;
    result.tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    Ok(PowerSeriesStepResult { result, steps })
}

fn validate_request(term: &str, variable: &str, from: &str) -> Result<(), EngineError> {
    validate_expression(term, "求和项")?;
    validate_symbol(variable, "求和变量")?;
    validate_expression(from, "求和下限")
}

fn method_rule(method: ConvergenceMethod) -> &'static str {
    match method {
        ConvergenceMethod::Geometric => "series-geometric",
        ConvergenceMethod::PSeries => "series-p",
        ConvergenceMethod::Alternating => "series-alternating",
        ConvergenceMethod::Comparison => "series-comparison",
        ConvergenceMethod::Term => "series-term",
        ConvergenceMethod::Ratio => "series-ratio",
        ConvergenceMethod::Root => "series-root",
        ConvergenceMethod::None => "series-inconclusive",
    }
}

fn method_explanation(method: ConvergenceMethod) -> &'static str {
    match method {
        ConvergenceMethod::Geometric => "识别为几何级数，并检查公比绝对值是否小于 1。",
        ConvergenceMethod::PSeries => "识别为 p 级数；p>1 时收敛，p<=1 时发散。",
        ConvergenceMethod::Alternating => {
            "应用交错级数判别，并另外检查绝对值级数以区分绝对收敛与条件收敛。"
        }
        ConvergenceMethod::Comparison => "与已知 p 级数比较其尾项阶数。",
        ConvergenceMethod::Term => "检查通项极限；通项不趋于零时级数必发散。",
        ConvergenceMethod::Ratio => "计算相邻项绝对值之比的极限并与 1 比较。",
        ConvergenceMethod::Root => "计算通项绝对值的 n 次方根极限并与 1 比较。",
        ConvergenceMethod::None => "现有有界判别规则未识别出可证明的收敛方法。",
    }
}

fn series_status_explanation(status: SeriesStatus) -> &'static str {
    match status {
        SeriesStatus::AbsolutelyConvergent => "判别表明级数绝对收敛。",
        SeriesStatus::Divergent => "判别表明级数发散。",
        SeriesStatus::ConditionallyConvergent => "级数收敛，但其绝对值级数发散。",
        SeriesStatus::Conditional => "收敛性取决于列出的参数条件。",
        SeriesStatus::Inconclusive => "当前判别规则无法确定该级数的收敛性。",
    }
}

fn power_series_interval(result: &PowerSeriesResult) -> String {
    match (&result.left_endpoint, &result.right_endpoint) {
        (Some(_), Some(_)) => result.radius.clone().unwrap_or_else(|| "Undefined".into()),
        _ if result.radius.as_deref() == Some("Infinity") => "Infinity".into(),
        _ => "Undefined".into(),
    }
}

fn list<'a>(expr: &'a Expr, label: &str) -> Result<&'a [Expr], EngineError> {
    match expr {
        Expr::Call { head, args } if head == "List" => Ok(args),
        _ => Err(EngineError::Parse(format!("{label} 不是列表"))),
    }
}

fn string<'a>(expr: &'a Expr, label: &str) -> Result<&'a str, EngineError> {
    match expr {
        Expr::Symbol(value) => Ok(value.trim_matches('"')),
        _ => Err(EngineError::Parse(format!("{label} 不是字符串"))),
    }
}

fn parse_status(expr: &Expr) -> Result<SeriesStatus, EngineError> {
    match string(expr, "收敛状态")? {
        "absolutely_convergent" => Ok(SeriesStatus::AbsolutelyConvergent),
        "divergent" => Ok(SeriesStatus::Divergent),
        "conditionally_convergent" => Ok(SeriesStatus::ConditionallyConvergent),
        "conditional" => Ok(SeriesStatus::Conditional),
        "inconclusive" => Ok(SeriesStatus::Inconclusive),
        value => Err(EngineError::Parse(format!("未知收敛状态: {value}"))),
    }
}

fn parse_method(expr: &Expr) -> Result<ConvergenceMethod, EngineError> {
    match string(expr, "判别方法")? {
        "geometric" => Ok(ConvergenceMethod::Geometric),
        "p_series" => Ok(ConvergenceMethod::PSeries),
        "alternating" => Ok(ConvergenceMethod::Alternating),
        "comparison" => Ok(ConvergenceMethod::Comparison),
        "term" => Ok(ConvergenceMethod::Term),
        "ratio" => Ok(ConvergenceMethod::Ratio),
        "root" => Ok(ConvergenceMethod::Root),
        "none" => Ok(ConvergenceMethod::None),
        value => Err(EngineError::Parse(format!("未知判别方法: {value}"))),
    }
}

fn parse_optional_status(expr: &Expr) -> Result<Option<SeriesStatus>, EngineError> {
    if string(expr, "端点状态")? == "none" {
        Ok(None)
    } else {
        parse_status(expr).map(Some)
    }
}

fn parse_bool(expr: &Expr, label: &str) -> Result<bool, EngineError> {
    match expr {
        Expr::Symbol(value) if value == "True" => Ok(true),
        Expr::Symbol(value) if value == "False" => Ok(false),
        _ => Err(EngineError::Parse(format!("{label} 不是布尔值"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;

    fn object(source: &str) -> crate::semantic_core::MathematicalObject {
        object_from_source(
            crate::semantic_core::ObjectId(91),
            source,
            SemanticState {
                kind: ValueKind::Expression,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata: ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty()),
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            },
        )
        .unwrap()
    }

    #[test]
    fn sum_operation_returns_typed_values_held_objects_and_absence() {
        let mut engine = RustEngine::spawn().unwrap();
        let finite = SumOperation
            .compute(
                &mut engine,
                &object("k"),
                &SumRequest {
                    variable: "k".into(),
                    lower: "1".into(),
                    upper: "10".into(),
                },
            )
            .unwrap();
        assert!(matches!(finite.output, ComputationOutput::Value(_)));
        assert_eq!(finite.subject().unwrap().print_source(), "55");
        assert_eq!(finite.subject().unwrap().revision.0, 1);

        let convergent = SumOperation
            .compute(
                &mut engine,
                &object("1/k^2"),
                &SumRequest {
                    variable: "k".into(),
                    lower: "1".into(),
                    upper: "Infinity".into(),
                },
            )
            .unwrap();
        assert!(matches!(convergent.output, ComputationOutput::Value(_)));
        assert!(convergent.subject().unwrap().print_source().contains("Pi"));

        let divergent = SumOperation
            .compute(
                &mut engine,
                &object("1/k"),
                &SumRequest {
                    variable: "k".into(),
                    lower: "1".into(),
                    upper: "Infinity".into(),
                },
            )
            .unwrap();
        assert!(matches!(divergent.output, ComputationOutput::NoValue(_)));

        let held = SumOperation
            .compute(
                &mut engine,
                &object("1/k^2"),
                &SumRequest {
                    variable: "k".into(),
                    lower: "0".into(),
                    upper: "Infinity".into(),
                },
            )
            .unwrap();
        assert!(matches!(held.output, ComputationOutput::Held(_)));
        assert!(matches!(
            held.subject().unwrap().semantics.interpretation,
            SemanticInterpretation::HeldTypedApplication(_)
        ));
    }

    #[test]
    fn computes_finite_symbolic_and_numeric_sums() {
        let mut engine = RustEngine::spawn().unwrap();
        let symbolic = finite_sum(&mut engine, "k^2", "k", "1", "n").unwrap();
        assert_eq!(symbolic.status, SumStatus::Evaluated);
        assert!(symbolic.value.contains("n"));

        let numeric = finite_sum(&mut engine, "k", "k", "1", "100").unwrap();
        assert_eq!(numeric.value, "5050");
        assert!(!numeric.tex.is_empty());
    }

    #[test]
    fn classifies_geometric_and_p_series() {
        let mut engine = RustEngine::spawn().unwrap();
        let geometric = infinite_series(&mut engine, "(1/2)^k", "k", "0").unwrap();
        assert_eq!(geometric.status, SeriesStatus::AbsolutelyConvergent);
        assert_eq!(geometric.method, ConvergenceMethod::Geometric);
        assert_eq!(geometric.test_value.as_deref(), Some("(1 / 2)"));
        assert_eq!(geometric.value.as_deref(), Some("2"));

        let p_series = infinite_series(&mut engine, "1/k^2", "k", "1").unwrap();
        assert_eq!(p_series.status, SeriesStatus::AbsolutelyConvergent);
        assert_eq!(p_series.method, ConvergenceMethod::PSeries);
        assert_eq!(p_series.value.as_deref(), Some("((Pi ^ 2) / 6)"));
        assert_eq!(
            engine.eval("Zeta(2)").unwrap().expr.to_string(),
            "((Pi ^ 2) / 6)"
        );
        assert_eq!(
            engine
                .eval("Sum(k,1,Infinity,1/k^2)")
                .unwrap()
                .expr
                .to_string(),
            "((Pi ^ 2) / 6)"
        );

        let harmonic = infinite_series(&mut engine, "1/k", "k", "1").unwrap();
        assert_eq!(harmonic.status, SeriesStatus::Divergent);
        assert_eq!(harmonic.method, ConvergenceMethod::PSeries);
        assert_eq!(harmonic.value, None);
    }

    #[test]
    fn uses_ratio_test_and_reports_inconclusive_cases() {
        let mut engine = RustEngine::spawn().unwrap();
        let ratio = infinite_series(&mut engine, "k/2^k", "k", "1").unwrap();
        assert_eq!(ratio.status, SeriesStatus::AbsolutelyConvergent);
        assert_eq!(ratio.method, ConvergenceMethod::Ratio);
        assert_eq!(ratio.test_value.as_deref(), Some("(1 / 2)"));

        let factorial = infinite_series(&mut engine, "1/k!", "k", "1").unwrap();
        assert_eq!(factorial.method, ConvergenceMethod::Ratio);

        let root = infinite_series(&mut engine, "(1/2)^(k^2)", "k", "1").unwrap();
        assert_eq!(root.status, SeriesStatus::AbsolutelyConvergent);
        assert_eq!(root.method, ConvergenceMethod::Root);

        let alternating = infinite_series(&mut engine, "(-1)^k/k", "k", "1").unwrap();
        assert_eq!(alternating.status, SeriesStatus::ConditionallyConvergent);
    }

    #[test]
    fn returns_symbolic_geometric_conditions_and_rejects_bad_input() {
        let mut engine = RustEngine::spawn().unwrap();
        let symbolic = infinite_series(&mut engine, "r^k", "k", "0").unwrap();
        assert_eq!(symbolic.status, SeriesStatus::Conditional);
        assert_eq!(symbolic.method, ConvergenceMethod::Geometric);
        assert_eq!(symbolic.conditions.len(), 1);
        assert!(infinite_series(&mut engine, "1/k^2", "k", "n").is_err());
        assert_eq!(
            infinite_series(&mut engine, "1/k^2", "k", "0")
                .unwrap()
                .status,
            SeriesStatus::Inconclusive
        );
        assert!(finite_sum(&mut engine, "k);Echo(1);(", "k", "1", "2").is_err());
        assert_eq!(engine.eval("2+3").unwrap().expr.to_string(), "5");
    }

    #[test]
    fn classifies_alternating_and_comparison_series() {
        let mut engine = RustEngine::spawn().unwrap();
        let alternating = infinite_series(&mut engine, "(-1)^(k-1)/k", "k", "1").unwrap();
        assert_eq!(alternating.status, SeriesStatus::ConditionallyConvergent);
        assert_eq!(alternating.method, ConvergenceMethod::Alternating);
        assert_eq!(alternating.value.as_deref(), Some("Ln(2)"));

        let absolute = infinite_series(&mut engine, "(-1)^k/k^2", "k", "1").unwrap();
        assert_eq!(absolute.status, SeriesStatus::AbsolutelyConvergent);
        assert_eq!(absolute.method, ConvergenceMethod::Alternating);

        let comparison = infinite_series(&mut engine, "1/(k^2+1)", "k", "1").unwrap();
        assert_eq!(comparison.status, SeriesStatus::AbsolutelyConvergent);
        assert_eq!(comparison.method, ConvergenceMethod::Comparison);
        assert_eq!(comparison.test_value.as_deref(), Some("1"));

        let divergent = infinite_series(&mut engine, "1/(k+1)", "k", "1").unwrap();
        assert_eq!(divergent.status, SeriesStatus::Divergent);
        assert_eq!(divergent.method, ConvergenceMethod::Comparison);
    }

    #[test]
    fn returns_power_series_radius_and_endpoint_contracts() {
        let mut engine = RustEngine::spawn().unwrap();
        let harmonic = power_series(&mut engine, "1/k", "k", "x", "2").unwrap();
        assert_eq!(harmonic.status, PowerSeriesStatus::Convergent);
        assert_eq!(harmonic.radius.as_deref(), Some("1"));
        assert_eq!(harmonic.left_endpoint.as_deref(), Some("1"));
        assert_eq!(harmonic.right_endpoint.as_deref(), Some("3"));
        assert_eq!(
            harmonic.left_status,
            Some(SeriesStatus::ConditionallyConvergent)
        );
        assert_eq!(harmonic.right_status, Some(SeriesStatus::Divergent));
        assert!(harmonic.left_included);
        assert!(!harmonic.right_included);

        let square = power_series(&mut engine, "1/k^2", "k", "x", "0").unwrap();
        assert!(square.left_included && square.right_included);
        assert_eq!(
            square.right_status,
            Some(SeriesStatus::AbsolutelyConvergent)
        );

        let exponential = power_series(&mut engine, "1/3^k", "k", "x", "0").unwrap();
        assert_eq!(exponential.radius.as_deref(), Some("3"));
        assert!(!exponential.left_included && !exponential.right_included);

        let entire = power_series(&mut engine, "1/k!", "k", "x", "0").unwrap();
        assert_eq!(entire.radius.as_deref(), Some("Infinity"));
        assert_eq!(entire.left_status, None);
        assert!(power_series(&mut engine, "1/k", "x", "x", "0").is_err());
        assert!(power_series(&mut engine, "x/k", "k", "x", "0").is_err());
        assert!(power_series(&mut engine, "1/k", "k", "x", "x+1").is_err());
    }

    #[test]
    fn infinite_series_steps_explain_methods_conditions_and_outcomes() {
        let mut engine = RustEngine::spawn().unwrap();
        for (term, expected_rule) in [
            ("(1/2)^k", "series-geometric"),
            ("1/k^2", "series-p"),
            ("(-1)^(k-1)/k", "series-alternating"),
            ("k/2^k", "series-ratio"),
        ] {
            let output = infinite_series_steps(&mut engine, term, "k", "1").unwrap();
            assert!(output.steps.iter().any(|step| step.rule == expected_rule));
            assert_eq!(output.steps.last().unwrap().rule, "series-result");
        }

        let conditional = infinite_series_steps(&mut engine, "r^k", "k", "0").unwrap();
        assert_eq!(conditional.result.status, SeriesStatus::Conditional);
        assert!(conditional
            .steps
            .iter()
            .any(|step| step.rule == "series-condition"));

        let inconclusive = infinite_series_steps(&mut engine, "1/k^2", "k", "0").unwrap();
        assert_eq!(inconclusive.result.status, SeriesStatus::Inconclusive);
        assert!(inconclusive
            .steps
            .iter()
            .any(|step| step.rule == "series-inconclusive"));
    }

    #[test]
    fn power_series_steps_check_each_finite_endpoint() {
        let mut engine = RustEngine::spawn().unwrap();
        let output = power_series_steps(&mut engine, "1/k", "k", "x", "2").unwrap();
        for rule in [
            "power-series-radius",
            "power-series-endpoint-left",
            "power-series-endpoint-right",
            "power-series-result",
        ] {
            assert!(output.steps.iter().any(|step| step.rule == rule));
        }
        assert!(output.result.left_included);
        assert!(!output.result.right_included);

        let entire = power_series_steps(&mut engine, "1/k!", "k", "x", "0").unwrap();
        assert!(!entire
            .steps
            .iter()
            .any(|step| step.rule.starts_with("power-series-endpoint-")));
    }
}
