//! Type-directed lowering from a complete semantic application to an
//! equivalent, usually more native, mathematical representation.

use crate::engine::{Engine, EngineError};
use crate::semantic_core::{
    object_from_source, CapabilitySet, Certificate, Computation, ComputationOutput,
    MathematicalObject, NormalizationLevel, NormalizationMetadata, NormalizationMode, ObjectDelta,
    OperatorId, Requirement, RuleEvent, RuleImportance, RulePayload, RulePresentation, RuleTrace,
    SemanticInterpretation, SemanticState, TraceMode,
};
use crate::{improper_integrals::ImproperIntegralRequest, protocol::ConditionSet};
use crate::{
    protocol::ResultMetadata,
    semantic::{Exactness, ValueKind},
};

pub type LoweringExecutor =
    fn(&mut dyn Engine, &MathematicalObject, TraceMode) -> Result<Option<Computation>, EngineError>;

#[derive(Clone, Copy)]
pub struct LoweringRuleDescriptor {
    pub id: &'static str,
    pub source_operator: OperatorId,
    pub priority: u16,
    pub minimum_normalization: NormalizationLevel,
    pub execute: LoweringExecutor,
}

/// Rules are added only when their object-native implementation is ready.
/// Ordering is explicit and bounded; unrelated operator recognizers are never
/// visited.
pub const LOWERING_RULES: &[LoweringRuleDescriptor] = &[LoweringRuleDescriptor {
    id: "integral.euler_gamma",
    source_operator: OperatorId::Integral,
    priority: 100,
    minimum_normalization: NormalizationLevel::Structural,
    execute: lower_euler_gamma,
}];

pub fn try_lower_application(
    engine: &mut dyn Engine,
    input: &MathematicalObject,
    trace_mode: TraceMode,
) -> Result<Option<Computation>, EngineError> {
    dispatch_registered(engine, input, trace_mode, LOWERING_RULES)
}

fn lower_euler_gamma(
    engine: &mut dyn Engine,
    input: &MathematicalObject,
    trace_mode: TraceMode,
) -> Result<Option<Computation>, EngineError> {
    let application = match &input.semantics.interpretation {
        SemanticInterpretation::TypedApplication(application)
        | SemanticInterpretation::HeldTypedApplication(application) => application,
        _ => return Ok(None),
    };
    let source_for = |requirement: Requirement| -> Result<Option<String>, EngineError> {
        let Some(argument) = application
            .arguments
            .iter()
            .find(|argument| argument.requirement == requirement)
        else {
            return Ok(None);
        };
        crate::input::with_parse_env(|env| {
            input
                .view(env)
                .at_path(&argument.path)
                .map(|view| view.print_source())
                .ok_or_else(|| EngineError::Parse("typed application 参数路径失效".into()))
                .map(Some)
        })
    };
    let (Some(variable), Some(lower), Some(upper), Some(expression)) = (
        source_for(Requirement::Variable)?,
        source_for(Requirement::LowerBound)?,
        source_for(Requirement::UpperBound)?,
        source_for(Requirement::Operand)?,
    ) else {
        return Ok(None);
    };
    let request = ImproperIntegralRequest {
        expression,
        variable,
        lower,
        upper,
        singular_points: Vec::new(),
    };
    // The legacy result is now only an engine adapter around the shared
    // structural matcher. Semantic state is constructed here from the typed
    // source object, never inferred from its display string.
    let Some(lowered) = crate::intrinsics::try_lower_improper_integral(
        engine,
        &request,
        Some(crate::steps::StepVerbosity::Detailed),
    )?
    else {
        return Ok(None);
    };
    let conditions = ConditionSet::new(
        application
            .conditions
            .conditions()
            .iter()
            .chain(lowered.conditions.conditions())
            .cloned(),
    )?;
    let semantics = SemanticState {
        kind: ValueKind::Expression,
        interpretation: SemanticInterpretation::PlainExpression,
        metadata: ResultMetadata::solved(Exactness::Symbolic, conditions.clone()),
        capabilities: CapabilitySet::symbolic_expression(),
        requirements: Vec::new(),
    };
    let parsed = object_from_source(input.id, &lowered.value, semantics.clone())?;
    let mut output = input.clone();
    output.apply(ObjectDelta {
        expression: Some(parsed.raw_expression()),
        semantics: Some(semantics),
        overlay: None,
        normalization: Some(NormalizationMetadata {
            level: NormalizationLevel::Domain,
            assumptions: conditions.conditions().to_vec(),
            mode: NormalizationMode::Operation(OperatorId::Integral),
        }),
    });
    let event = RuleEvent {
        rule: "intrinsic-gamma-lowering".into(),
        input: input.reference(None),
        additional_inputs: Vec::new(),
        output: output.reference(None),
        bindings: lowered
            .certificate
            .as_ref()
            .map(|certificate| {
                certificate
                    .bindings
                    .iter()
                    .map(|binding| (binding.name.clone(), binding.value.clone()))
                    .collect()
            })
            .unwrap_or_default(),
        conditions: conditions.conditions().to_vec(),
        payload: RulePayload::Rewrite,
        importance: RuleImportance::Key,
        presentation: Some(RulePresentation {
            expression: output.print_source(),
            explanation: "识别 Euler 型积分核，在成立条件下使用原生 Gamma 对象。".into(),
            tex_override: Some(lowered.tex),
        }),
    };
    let certificate = lowered.certificate.map(|certificate| Certificate {
        kind: certificate.rule,
        payload: format!(
            "source=improper_integral;target=gamma;bindings={:?};conditions={:?}",
            certificate.bindings,
            certificate.conditions.conditions()
        ),
    });
    Ok(Some(Computation {
        output: ComputationOutput::Value(output),
        trace: (!matches!(trace_mode, TraceMode::Off)).then(|| RuleTrace {
            events: vec![event],
        }),
        certificates: certificate.into_iter().collect(),
        effects: Vec::new(),
    }))
}

fn dispatch_registered(
    engine: &mut dyn Engine,
    input: &MathematicalObject,
    trace_mode: TraceMode,
    rules: &[LoweringRuleDescriptor],
) -> Result<Option<Computation>, EngineError> {
    let operator = match &input.semantics.interpretation {
        SemanticInterpretation::TypedApplication(application)
        | SemanticInterpretation::HeldTypedApplication(application) => application.operator,
        _ => return Ok(None),
    };
    debug_assert!(rules
        .windows(2)
        .all(|pair| pair[0].priority <= pair[1].priority));
    for rule in rules.iter().filter(|rule| rule.source_operator == operator) {
        if rule.minimum_normalization > NormalizationLevel::Structural
            && !input.meets_normalization(rule.minimum_normalization)
        {
            continue;
        }
        if let Some(lowered) = (rule.execute)(engine, input, trace_mode)? {
            return Ok(Some(lowered));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn execute(source: &str) -> (crate::semantic_core::ObjectId, Computation) {
        let elaborated = crate::elaboration::elaborate(source).unwrap();
        let input_id = elaborated.object.id;
        let mut engine = crate::engine::RustEngine::spawn().unwrap();
        (
            input_id,
            crate::arithmetic::execute_elaborated_structure(&mut engine, &elaborated).unwrap(),
        )
    }

    fn lower(object: &MathematicalObject) -> Computation {
        let mut engine = crate::engine::RustEngine::spawn().unwrap();
        try_lower_application(&mut engine, object, TraceMode::Detailed)
            .unwrap()
            .expect("Euler application matches lowering")
    }

    fn staged_integral() -> MathematicalObject {
        use crate::protocol::{OutcomeReason, ResultMetadata};
        use crate::semantic_core::{
            fill_next_partial_argument, object_from_source, partial_application_state, Requirement,
            SemanticState,
        };
        let state = partial_application_state("Integrate", 4, 1).unwrap();
        let partial = object_from_source(
            crate::semantic_core::ObjectId(900),
            "Integrate(t)",
            SemanticState {
                kind: ValueKind::Unevaluated,
                interpretation: SemanticInterpretation::PartialApplication(state.clone()),
                metadata: ResultMetadata::unresolved(
                    Exactness::Symbolic,
                    OutcomeReason::AlgorithmUncovered,
                ),
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: state.missing,
            },
        )
        .unwrap();
        let argument = |id, source| {
            object_from_source(
                crate::semantic_core::ObjectId(id),
                source,
                SemanticState {
                    kind: ValueKind::Expression,
                    interpretation: SemanticInterpretation::PlainExpression,
                    metadata: ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty()),
                    capabilities: CapabilitySet::symbolic_expression(),
                    requirements: Vec::<Requirement>::new(),
                },
            )
            .unwrap()
        };
        let lower = fill_next_partial_argument(&partial, &argument(901, "0")).unwrap();
        let upper = fill_next_partial_argument(&lower, &argument(902, "Infinity")).unwrap();
        fill_next_partial_argument(&upper, &argument(903, "t^(x-1)*Exp(-t)")).unwrap()
    }

    static MATCHING_CALLS: AtomicUsize = AtomicUsize::new(0);
    static UNRELATED_CALLS: AtomicUsize = AtomicUsize::new(0);

    fn matching(
        _engine: &mut dyn Engine,
        _input: &MathematicalObject,
        _trace_mode: TraceMode,
    ) -> Result<Option<Computation>, EngineError> {
        MATCHING_CALLS.fetch_add(1, Ordering::Relaxed);
        Ok(None)
    }

    fn unrelated(
        _engine: &mut dyn Engine,
        _input: &MathematicalObject,
        _trace_mode: TraceMode,
    ) -> Result<Option<Computation>, EngineError> {
        UNRELATED_CALLS.fetch_add(1, Ordering::Relaxed);
        Ok(None)
    }

    #[test]
    fn dispatches_only_rules_for_the_typed_source_operator() {
        MATCHING_CALLS.store(0, Ordering::Relaxed);
        UNRELATED_CALLS.store(0, Ordering::Relaxed);
        let input = crate::elaboration::elaborate("Limit(t,0)(Sin(t)/t)")
            .unwrap()
            .object;
        let rules = [
            LoweringRuleDescriptor {
                id: "unrelated",
                source_operator: OperatorId::Derivative,
                priority: 0,
                minimum_normalization: NormalizationLevel::Structural,
                execute: unrelated,
            },
            LoweringRuleDescriptor {
                id: "matching",
                source_operator: OperatorId::Limit,
                priority: 0,
                minimum_normalization: NormalizationLevel::Structural,
                execute: matching,
            },
        ];
        let mut engine = crate::engine::RustEngine::spawn().unwrap();
        assert!(
            dispatch_registered(&mut engine, &input, TraceMode::Off, &rules)
                .unwrap()
                .is_none()
        );
        assert_eq!(MATCHING_CALLS.load(Ordering::Relaxed), 1);
        assert_eq!(UNRELATED_CALLS.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn euler_gamma_lowering_is_identical_across_application_stages() {
        let direct = crate::elaboration::elaborate("Integrate(t,0,Infinity,t^(x-1)*Exp(-t))")
            .unwrap()
            .object;
        let curried = crate::elaboration::elaborate("Integrate(t,0,Infinity)t^(x-1)*Exp(-t)")
            .unwrap()
            .object;
        let staged = staged_integral();
        for input in [&direct, &curried, &staged] {
            let result = lower(input);
            let output = result.value().expect("Euler integral lowers to a value");
            assert_eq!(output.print_source().replace(' ', ""), "Gamma(x)");
            assert_eq!(output.id, input.id);
            assert_eq!(output.revision.0, input.revision.0 + 1);
            assert!(matches!(
                output.semantics.metadata.conditions.conditions(),
                [crate::protocol::Condition::RealPartPositive { expression }] if expression == "x"
            ));
            assert_eq!(result.certificates.len(), 1);
            assert_eq!(
                result.trace.as_ref().unwrap().events[0].rule,
                "intrinsic-gamma-lowering"
            );
        }
    }

    #[test]
    fn nested_derivative_receives_lowered_gamma_object() {
        let (_, result) = execute("D(x)(Integrate(t,0,Infinity)(t^(x-1)*Exp(-t)))");
        let output = result.subject().expect("held derivative remains an object");
        assert_eq!(output.print_source().replace(' ', ""), "D(x,1)Gamma(x)");
        assert!(matches!(result.output, ComputationOutput::Held(_)));
        let rules = result
            .trace
            .as_ref()
            .unwrap()
            .events
            .iter()
            .map(|event| event.rule.as_str())
            .collect::<Vec<_>>();
        assert!(rules.contains(&"intrinsic-gamma-lowering"));
    }
}
