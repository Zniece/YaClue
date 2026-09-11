//! Object-native indefinite integration. The engine remains the temporary
//! domain adapter, while the operation boundary owns Held/family semantics.

use crate::engine::{Engine, EngineError};
use crate::input::validate_symbol;
use crate::protocol::{ConditionSet, OutcomeReason, ResultMetadata};
use crate::semantic::{Exactness, ValueKind};
use crate::semantic_core::{
    object_from_source, CapabilitySet, Computation, ComputationOutput, NormalizationLevel,
    NormalizationMetadata, NormalizationMode, ObjectDelta, OperatorId, RuleEvent, RuleImportance,
    RulePayload, RulePresentation, RuleTrace, SemanticInterpretation, SemanticOperation,
    SemanticState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegralRequest {
    pub variable: String,
    pub arbitrary_constant: String,
}

impl IntegralRequest {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_symbol(&self.variable, "积分变量")?;
        validate_symbol(&self.arbitrary_constant, "积分任意常数")
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct IntegralOperation;

impl SemanticOperation<IntegralRequest> for IntegralOperation {
    fn compute(
        &self,
        engine: &mut dyn Engine,
        input: &crate::semantic_core::MathematicalObject,
        request: &IntegralRequest,
    ) -> Result<Computation, EngineError> {
        request.validate()?;
        if !input
            .semantics
            .capabilities
            .contains(crate::semantic_core::ObjectCapability::Integrate)
        {
            return Err(EngineError::InvalidInput("该数学对象不具备积分能力".into()));
        }
        let source = input.print_source();
        let representative = engine
            .eval_expr(&format!("Integrate({}){source}", request.variable))?
            .to_string();
        let unresolved = crate::input::with_parse_env(|env| {
            let parsed = yacas_rs::parser::parse_expression(env, &format!("{representative};"))
                .map_err(|error| EngineError::Parse(format!("积分结果语法异常: {error:?}")))?
                .ok_or_else(|| EngineError::Parse("积分结果为空".into()))?;
            Ok(crate::semantic_core::ExpressionView::new(env, &parsed).head() == Some("Integrate"))
        })?;
        let output_source = if unresolved {
            representative.clone()
        } else {
            format!("({representative})+{}", request.arbitrary_constant)
        };
        let metadata = if unresolved {
            ResultMetadata::unresolved(Exactness::Symbolic, OutcomeReason::AlgorithmUncovered)
        } else {
            ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty())
        };
        let semantics = SemanticState {
            kind: if unresolved {
                ValueKind::Unevaluated
            } else {
                ValueKind::FunctionFamily
            },
            interpretation: if unresolved {
                SemanticInterpretation::HeldApplication {
                    operator: "Integrate".into(),
                }
            } else {
                SemanticInterpretation::PlainExpression
            },
            metadata,
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        };
        let parsed = object_from_source(input.id, &output_source, semantics.clone())?;
        let mut output = input.clone();
        output.apply(ObjectDelta {
            expression: Some(parsed.raw_expression()),
            semantics: Some(semantics),
            overlay: None,
            normalization: (!unresolved).then_some(NormalizationMetadata {
                level: NormalizationLevel::Domain,
                assumptions: Vec::new(),
                mode: NormalizationMode::Operation(OperatorId::Integral),
            }),
        });
        let mut events = crate::steps::derive_integrals_with_verbosity(
            engine,
            &source,
            &request.variable,
            crate::steps::StepVerbosity::Detailed,
        )?
        .into_iter()
        .map(|step| RuleEvent {
            rule: step.rule,
            input: input.reference(None),
            additional_inputs: Vec::new(),
            output: output.reference(None),
            bindings: vec![("variable".into(), request.variable.clone())],
            conditions: Vec::new(),
            payload: RulePayload::Structural,
            importance: match step.importance {
                crate::steps::StepImportance::Routine => RuleImportance::Routine,
                crate::steps::StepImportance::Normal => RuleImportance::Normal,
                crate::steps::StepImportance::Key => RuleImportance::Key,
            },
            presentation: Some(RulePresentation {
                expression: step.expr,
                explanation: step.why,
                tex_override: Some(step.tex),
            }),
        })
        .collect::<Vec<_>>();
        events.push(RuleEvent {
            rule: if unresolved {
                "hold-integral"
            } else {
                "antiderivative-family"
            }
            .into(),
            input: input.reference(None),
            additional_inputs: Vec::new(),
            output: output.reference(None),
            bindings: vec![
                ("variable".into(), request.variable.clone()),
                ("constant".into(), request.arbitrary_constant.clone()),
            ],
            conditions: Vec::new(),
            payload: RulePayload::Rewrite,
            importance: RuleImportance::Key,
            presentation: Some(RulePresentation {
                expression: if unresolved {
                    output.print_source()
                } else {
                    format!("({representative} + {})", request.arbitrary_constant)
                },
                explanation: if unresolved {
                    "保留尚无闭式结果的积分对象。"
                } else {
                    "构造包含任意常数的原函数族。"
                }
                .into(),
                tex_override: None,
            }),
        });
        Ok(Computation {
            output: if unresolved {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;
    use crate::semantic_core::{object_from_source, ObjectId};

    fn input(source: &str) -> crate::semantic_core::MathematicalObject {
        object_from_source(
            ObjectId(7),
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
    fn returns_a_typed_antiderivative_family() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = IntegralOperation
            .compute(
                &mut engine,
                &input("x^2"),
                &IntegralRequest {
                    variable: "x".into(),
                    arbitrary_constant: "C".into(),
                },
            )
            .unwrap();
        let output = result.value().unwrap();
        assert_eq!(output.semantics.kind, ValueKind::FunctionFamily);
        assert_eq!(output.id, ObjectId(7));
        assert!(output.print_source().contains("C"));
        assert!(
            output.print_source().contains("x^3"),
            "{}",
            output.print_source()
        );
        assert_eq!(
            engine
                .eval_expr(&format!(
                    "Simplify(ApplyPure(\"D\",{{x,{}}})-x^2)",
                    output.print_source()
                ))
                .unwrap()
                .to_string(),
            "0"
        );
    }

    #[test]
    fn retains_an_unresolved_integral_as_a_held_object() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = IntegralOperation
            .compute(
                &mut engine,
                &input("f(x)"),
                &IntegralRequest {
                    variable: "x".into(),
                    arbitrary_constant: "C".into(),
                },
            )
            .unwrap();
        assert!(matches!(result.output, ComputationOutput::Held(_)));
        assert!(result
            .subject()
            .unwrap()
            .print_source()
            .contains("Integrate"));
    }
}
