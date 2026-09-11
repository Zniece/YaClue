//! Type-directed lowering from a complete semantic application to an
//! equivalent, usually more native, mathematical representation.

use crate::engine::{Engine, EngineError};
use crate::semantic_core::{
    Computation, MathematicalObject, NormalizationLevel, OperatorId, SemanticInterpretation,
    TraceMode,
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
/// Keeping this registry empty in A7 proves that installing the dispatcher
/// does not change mathematical behavior before the first A8 rule lands.
pub const LOWERING_RULES: &[LoweringRuleDescriptor] = &[];

pub fn try_lower_application(
    engine: &mut dyn Engine,
    input: &MathematicalObject,
    trace_mode: TraceMode,
) -> Result<Option<Computation>, EngineError> {
    dispatch_registered(engine, input, trace_mode, LOWERING_RULES)
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
}
