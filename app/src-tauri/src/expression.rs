use processing::engine::{Engine, ErrorCode, ErrorResponse, RustEngineProxy};
use processing::protocol::{OutcomeReason, ResultMetadata};
use processing::semantic::{SemanticSummary, ValueKind};
use processing::steps::{Step, StepVerbosity};

use crate::expression_protocol::{
    ProcessExpressionDetails, ProcessExpressionRequest, ProcessExpressionResult,
};
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
    let root_descriptor = match &elaborated.root.form {
        processing::elaboration::MathematicalForm::Application { head }
        | processing::elaboration::MathematicalForm::EffectApplication { head } => {
            processing::semantic_core::operator_descriptor(head)
                .filter(|_| processing::semantic_core::is_object_native_operator(head))
        }
        _ => None,
    };
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
    let has_native_child = processing::arithmetic::has_object_native_descendant(&elaborated.root);
    let (kind, title) = match (&elaborated.root.form, root_descriptor) {
        (processing::elaboration::MathematicalForm::Structural { operator }, _)
            if matches!(operator.as_str(), "+" | "*")
                && elaborated.root.children.iter().all(|child| {
                    matches!(
                        child.form,
                        processing::elaboration::MathematicalForm::Collection
                    )
                }) =>
        {
            ("matrix", "线性代数")
        }
        (processing::elaboration::MathematicalForm::Relation { .. }, _) => ("equation", "方程"),
        (_, Some(descriptor)) => descriptor
            .product_presentation(
                has_native_child,
                result.status == processing::composition::CompositionStatus::Completed,
            )
            .unwrap_or(("composition", "组合运算")),
        (
            processing::elaboration::MathematicalForm::Number
            | processing::elaboration::MathematicalForm::Symbol
            | processing::elaboration::MathematicalForm::Collection
            | processing::elaboration::MathematicalForm::OpaqueEngineValue { .. }
            | processing::elaboration::MathematicalForm::Application { .. },
            None,
        ) if !has_native_child => ("evaluation", "计算结果"),
        _ => ("composition", "组合运算"),
    };
    unified_result(kind, title, result)
}

pub fn process_expression_with_engine(
    request: ProcessExpressionRequest,
    engine: &mut RustEngineProxy,
) -> Result<ProcessExpressionResult, ErrorResponse> {
    let elaborated =
        processing::elaboration::elaborate_input(&request.expression).map_err(message)?;
    let analyzed = &elaborated.analyzed;
    if let Some(partial_object) =
        processing::elaboration::operand_partial(&elaborated.root).map_err(message)?
    {
        let processing::semantic_core::SemanticInterpretation::PartialApplication(partial) =
            &partial_object.semantics.interpretation
        else {
            unreachable!("operand_partial returns a partial application")
        };
        let mut semantic = analyzed.semantic.clone();
        semantic.kind = ValueKind::Unevaluated;
        for scope in &partial.binder_scopes {
            let Some(name) = elaborated
                .root
                .children
                .get(scope.binder_slot)
                .map(|child| child.object.print_source())
            else {
                continue;
            };
            semantic.symbols.retain(|symbol| symbol != &name);
            if !semantic.bound_symbols.contains(&name) {
                semantic.bound_symbols.push(name.clone());
                semantic.bound_symbols.sort();
            }
            if let Some(identity) = semantic
                .symbol_identities
                .iter_mut()
                .find(|identity| identity.name == name)
            {
                identity.role = processing::binding::SymbolRole::Bound;
                identity.binder = Some(scope.binder_slot as u32);
            }
        }
        let outcome =
            ResultMetadata::unresolved(semantic.exactness, OutcomeReason::AlgorithmUncovered);
        let parameter_sources = elaborated
            .root
            .children
            .iter()
            .map(|child| child.object.print_source())
            .collect::<Vec<_>>();
        let parameter_tex = engine
            .render_tex_batch(&parameter_sources)
            .map_err(message)?
            .into_iter()
            .map(|tex| processing::input::strip_tex_delimiters(&tex))
            .collect::<Vec<_>>()
            .join(", ");
        let tex = format!(
            "\\operatorname{{{}}}\\left({parameter_tex}\\right)",
            partial.spelling
        );
        return Ok(ProcessExpressionResult {
            kind: "partial_application".into(),
            title: "部分应用".into(),
            expression: request.expression,
            tex,
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
            details: Some(ProcessExpressionDetails::PartialApplication {
                partial: partial.clone(),
            }),
            semantic,
            outcome,
        });
    }
    if let Some(partials) =
        processing::elaboration::partial_candidates(&elaborated.root).map_err(message)?
    {
        let mut semantic = analyzed.semantic.clone();
        semantic.kind = ValueKind::Unevaluated;
        let bound_sources = elaborated
            .root
            .children
            .iter()
            .map(|child| child.object.print_source())
            .collect::<Vec<_>>();
        let templates = partials
            .candidates
            .iter()
            .map(|candidate| candidate.display_template(&bound_sources))
            .collect::<Vec<_>>();
        return Ok(ProcessExpressionResult {
            kind: "partial_application".into(),
            title: "部分应用".into(),
            expression: request.expression,
            tex: format!("\\operatorname{{{}}}", partials.spelling),
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
            details: Some(ProcessExpressionDetails::AmbiguousPartialApplication {
                candidates: partials.candidates,
                display_templates: templates,
            }),
            semantic,
            outcome: ResultMetadata::unresolved(
                analyzed.semantic.exactness,
                OutcomeReason::AlgorithmUncovered,
            ),
        });
    }
    let result = dispatch_expression_with_engine(request, engine, &elaborated)?;
    Ok(ProcessExpressionResult {
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
