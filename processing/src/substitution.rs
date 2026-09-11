//! Capture-avoiding substitution over mathematical objects.

use crate::engine::{Engine, EngineError};
use crate::protocol::{ConditionSet, OutcomeReason, ResolutionState, ResultMetadata};
use crate::semantic::{Exactness, ValueKind};
use crate::semantic_core::{
    object_from_source, BinarySemanticOperation, CapabilitySet, Computation, ComputationOutput,
    NormalizationLevel, NormalizationMetadata, NormalizationMode, ObjectCapability, ObjectDelta,
    OperatorId, RuleEvent, RuleImportance, RulePayload, RulePresentation, RuleTrace,
    SemanticInterpretation, SemanticState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubstitutionRequest {
    pub variable: String,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SubstitutionOperation;

impl BinarySemanticOperation<SubstitutionRequest> for SubstitutionOperation {
    fn compute(
        &self,
        engine: &mut dyn Engine,
        input: &crate::semantic_core::MathematicalObject,
        replacement: &crate::semantic_core::MathematicalObject,
        request: &SubstitutionRequest,
    ) -> Result<Computation, EngineError> {
        crate::input::validate_symbol(&request.variable, "替换变量")?;
        if !input
            .semantics
            .capabilities
            .contains(ObjectCapability::Substitute)
        {
            return Err(EngineError::InvalidInput("该数学对象不具备替换能力".into()));
        }
        let rewritten =
            crate::binding::substitute_free_objects(input, &request.variable, replacement)?;
        let unresolved = input.semantics.metadata.resolution == ResolutionState::Unresolved
            || replacement.semantics.metadata.resolution == ResolutionState::Unresolved;
        let expression = if unresolved {
            rewritten
        } else {
            let structural = crate::semantic_core::MathematicalObject::new(
                input.id,
                rewritten,
                input.semantics.clone(),
            );
            let normalized = engine.eval_expr(&structural.print_source())?.to_string();
            object_from_source(input.id, &normalized, input.semantics.clone())?.raw_expression()
        };
        let kind = crate::input::with_parse_env(|env| {
            crate::semantic::analyze_tree(env, &expression)
                .semantic
                .kind
        });
        let semantics = SemanticState {
            kind: if unresolved {
                ValueKind::Unevaluated
            } else {
                kind
            },
            interpretation: if unresolved {
                SemanticInterpretation::HeldApplication {
                    operator: "Subst".into(),
                }
            } else {
                SemanticInterpretation::PlainExpression
            },
            metadata: if unresolved {
                ResultMetadata::unresolved(Exactness::Symbolic, OutcomeReason::AlgorithmUncovered)
            } else {
                ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty())
            },
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        };
        let mut output = input.clone();
        output.apply(ObjectDelta {
            expression: Some(expression),
            semantics: Some(semantics),
            overlay: None,
            normalization: (!unresolved).then_some(NormalizationMetadata {
                level: NormalizationLevel::Structural,
                assumptions: Vec::new(),
                mode: NormalizationMode::Operation(OperatorId::Substitute),
            }),
        });
        let event = RuleEvent {
            rule: if unresolved {
                "substitute-into-held-object"
            } else {
                "substitute-free-symbol"
            }
            .into(),
            input: input.reference(None),
            additional_inputs: vec![replacement.reference(None)],
            output: output.reference(None),
            bindings: vec![("variable".into(), request.variable.clone())],
            conditions: Vec::new(),
            payload: RulePayload::Rewrite,
            importance: RuleImportance::Key,
            presentation: (!unresolved).then(|| RulePresentation {
                expression: output.print_source(),
                explanation: "仅替换自由出现的变量，并避免捕获绑定变量。".into(),
                tex_override: None,
            }),
        };
        Ok(Computation {
            output: if unresolved {
                ComputationOutput::Held(output)
            } else {
                ComputationOutput::Value(output)
            },
            trace: Some(RuleTrace {
                events: vec![event],
            }),
            certificates: Vec::new(),
            effects: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;
    use crate::semantic_core::{MathematicalObject, ObjectId};

    fn object(source: &str, unresolved: bool) -> MathematicalObject {
        object_from_source(
            ObjectId(41),
            source,
            SemanticState {
                kind: if unresolved {
                    ValueKind::Unevaluated
                } else {
                    ValueKind::Expression
                },
                interpretation: if unresolved {
                    SemanticInterpretation::HeldApplication {
                        operator: "Integrate".into(),
                    }
                } else {
                    SemanticInterpretation::PlainExpression
                },
                metadata: if unresolved {
                    ResultMetadata::unresolved(
                        Exactness::Symbolic,
                        OutcomeReason::AlgorithmUncovered,
                    )
                } else {
                    ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty())
                },
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            },
        )
        .unwrap()
    }

    #[test]
    fn substitutes_and_normalizes_object_values() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = BinarySemanticOperation::compute(
            &SubstitutionOperation,
            &mut engine,
            &object("x^2+1", false),
            &object("2", false),
            &SubstitutionRequest {
                variable: "x".into(),
            },
        )
        .unwrap();
        assert_eq!(result.value().unwrap().print_source(), "5");
        assert_eq!(result.value().unwrap().id, ObjectId(41));
        assert!(result
            .value()
            .unwrap()
            .meets_normalization(NormalizationLevel::Structural));
    }

    #[test]
    fn avoids_capture_and_can_rewrite_held_math() {
        let mut engine = RustEngine::spawn().unwrap();
        let captured = BinarySemanticOperation::compute(
            &SubstitutionOperation,
            &mut engine,
            &object("Integrate(t,0,1)(x+t)", false),
            &object("t", false),
            &SubstitutionRequest {
                variable: "x".into(),
            },
        )
        .unwrap();
        assert_eq!(captured.value().unwrap().print_source(), "1/2+t");

        let held = BinarySemanticOperation::compute(
            &SubstitutionOperation,
            &mut engine,
            &object("Integrate(t)(x*f(t))", true),
            &object("2", false),
            &SubstitutionRequest {
                variable: "x".into(),
            },
        )
        .unwrap();
        assert!(matches!(held.output, ComputationOutput::Held(_)));
        let source = held.subject().unwrap().print_source();
        assert!(source.contains("Integrate"));
        assert!(source.contains("2*f(t)"), "{source}");
    }
}
