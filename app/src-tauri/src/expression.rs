use processing::engine::{ErrorCode, ErrorResponse, RustEngineProxy};
use processing::protocol::ResultMetadata;
use processing::semantic::SemanticSummary;
use processing::steps::{Step, StepVerbosity};

use crate::expression_protocol::{ProcessExpressionRequest, ProcessExpressionResult};
use crate::{invalid_input, message};

fn parse_verbosity(value: &str) -> Result<StepVerbosity, ErrorResponse> {
    match value {
        "concise" => Ok(StepVerbosity::Concise),
        "standard" => Ok(StepVerbosity::Standard),
        "detailed" => Ok(StepVerbosity::Detailed),
        _ => Err(invalid_input(format!("未知步骤粒度: {value}"))),
    }
}

struct DispatchExpressionResult {
    kind: String,
    title: String,
    expression: String,
    tex: String,
    steps: Vec<Step>,
    analyses: Vec<processing::steps::MathematicalAnalysis>,
    conclusions: Vec<processing::steps::MathematicalConclusion>,
    status: processing::composition::CompositionStatus,
    operators: Vec<processing::semantic_core::OperatorId>,
    held: Option<processing::composition::HeldApplication>,
    sampled_data: Option<processing::semantic_core::SampledTrajectory>,
    plot: Option<processing::plot::PlotEffect>,
    effect_only: bool,
    analysis: Option<processing::semantic_core::ComputationAnalysis>,
    semantic: SemanticSummary,
    outcome: ResultMetadata,
}

fn unified_result(
    kind: &str,
    title: &str,
    result: processing::composition::CompositionResult,
) -> Result<DispatchExpressionResult, ErrorResponse> {
    Ok(DispatchExpressionResult {
        kind: kind.into(),
        title: title.into(),
        expression: result.value,
        tex: result.tex,
        steps: result.steps,
        analyses: result.analyses,
        conclusions: result.conclusions,
        status: result.status,
        operators: result.operators,
        held: result.held,
        sampled_data: result.sampled_data,
        plot: result.plot,
        effect_only: result.effect_only,
        analysis: result.analysis,
        semantic: result.semantic,
        outcome: result.outcome,
    })
}

fn dispatch_expression_with_engine(
    request: ProcessExpressionRequest,
    engine: &mut RustEngineProxy,
    elaborated: &processing::elaboration::ElaboratedInput,
) -> Result<DispatchExpressionResult, ErrorResponse> {
    let verbosity = parse_verbosity(&request.verbosity)?;
    let Some(mut result) = processing::composition::execute_elaborated(
        &mut *engine,
        elaborated,
        verbosity,
        request.steps,
    )
    .map_err(message)?
    else {
        return Err(ErrorResponse {
            code: ErrorCode::Internal,
            message: "完整数学输入未产生结构化计算结果".into(),
            retryable: false,
        });
    };
    if !request.steps {
        result.steps.clear();
    }
    let classification = processing::composition::classify_product(elaborated, &result);
    unified_result(classification.kind, classification.title, result)
}

pub fn process_expression_with_engine(
    request: ProcessExpressionRequest,
    engine: &mut RustEngineProxy,
) -> Result<ProcessExpressionResult, ErrorResponse> {
    let elaborated =
        processing::elaboration::elaborate_input(&request.expression).map_err(message)?;
    if let Some(partial) =
        processing::composition::project_partial(&mut *engine, &elaborated).map_err(message)?
    {
        return Ok(ProcessExpressionResult {
            kind: "partial_application".into(),
            title_key: "result.partial_application".into(),
            title: "部分应用".into(),
            expression: partial.expression,
            tex: partial.tex,
            steps: Vec::new(),
            analyses: Vec::new(),
            conclusions: Vec::new(),
            status: None,
            operators: Vec::new(),
            held: None,
            sampled_data: None,
            plot: None,
            effect_only: false,
            analysis: None,
            details: Some(partial.details),
            semantic: partial.semantic,
            outcome: partial.outcome,
        });
    }
    let result = dispatch_expression_with_engine(request, engine, &elaborated)?;
    Ok(ProcessExpressionResult {
        title_key: format!("result.{}", result.kind),
        kind: result.kind,
        title: result.title,
        expression: result.expression,
        tex: result.tex,
        steps: result.steps,
        analyses: result.analyses,
        conclusions: result.conclusions,
        status: Some(result.status),
        operators: result.operators,
        held: result.held,
        sampled_data: result.sampled_data,
        plot: result.plot,
        effect_only: result.effect_only,
        analysis: result.analysis,
        details: None,
        semantic: result.semantic,
        outcome: result.outcome,
    })
}
