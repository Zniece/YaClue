//! Object-native indefinite integration. The engine remains the temporary
//! domain adapter, while the operation boundary owns Held/family semantics.

use crate::engine::{Engine, EngineError};
use crate::input::validate_symbol;
use crate::protocol::{ConditionSet, OutcomeReason, ResultMetadata};
use crate::semantic::{Exactness, ValueKind};
use crate::semantic_core::{
    object_from_source, CapabilitySet, Computation, ComputationOutput, NormalizationLevel,
    NormalizationMetadata, NormalizationMode, ObjectDelta, ObjectId, OperatorId, Requirement,
    RuleEvent, RuleImportance, RulePayload, RulePresentation, RuleTrace, SemanticInterpretation,
    SemanticOperation, SemanticState,
};

/// Curried `Integrate(variable)`, waiting for its integrand.
pub fn integral_partial(
    id: ObjectId,
    variable: &str,
) -> Result<crate::semantic_core::MathematicalObject, EngineError> {
    validate_symbol(variable, "积分变量")?;
    object_from_source(
        id,
        &format!("Integrate({variable})"),
        SemanticState {
            kind: ValueKind::Unevaluated,
            interpretation: SemanticInterpretation::PartialApplication(
                crate::semantic_core::operand_partial_state("Integrate", 1)?,
            ),
            metadata: ResultMetadata::unresolved(
                Exactness::Symbolic,
                OutcomeReason::AlgorithmUncovered,
            ),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: vec![Requirement::Operand],
        },
    )
}

/// Complete an integral partial using its AST parameters and an object operand.
pub fn apply_integral_partial(
    engine: &mut dyn Engine,
    partial: &crate::semantic_core::MathematicalObject,
    operand: &crate::semantic_core::MathematicalObject,
) -> Result<Computation, EngineError> {
    crate::semantic_core::require_operand_partial(partial, OperatorId::Integral)?;
    let _application = crate::semantic_core::complete_operand_partial(partial, operand)?;
    let variable = crate::input::with_parse_env(|env| {
        let view = partial.view(env);
        let arguments = view.arguments();
        let [variable] = arguments.as_slice() else {
            return Err(EngineError::Parse("Integrate 部分应用参数数量异常".into()));
        };
        if view.head() != Some("Integrate") {
            return Err(EngineError::Parse("Integrate 部分应用 AST 形态异常".into()));
        }
        Ok(variable.print_source())
    })?;
    IntegralOperation.compute(
        engine,
        operand,
        &IntegralRequest {
            variable,
            arbitrary_constant: available_constant(operand),
        },
    )
}

pub(crate) fn available_constant(input: &crate::semantic_core::MathematicalObject) -> String {
    let occupied = crate::input::with_parse_env(|env| {
        let analyzed = crate::semantic::analyze_tree(env, &input.raw_expression());
        analyzed
            .semantic
            .symbols
            .into_iter()
            .chain(analyzed.semantic.constants)
            .collect::<Vec<_>>()
    });
    crate::semantic::display_arbitrary_constants(&occupied, 1)
        .into_iter()
        .next()
        .expect("one arbitrary constant requested")
}

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
        let derivation = crate::steps::derive_antiderivative_family_with_verbosity(
            engine,
            &source,
            &request.variable,
            request.arbitrary_constant.clone(),
            crate::steps::StepVerbosity::Detailed,
        )?;
        let representative = derivation.result.representative.clone();
        let unresolved = crate::input::with_parse_env(|env| {
            let parsed = yacas_rs::parser::parse_expression(env, &format!("{representative};"))
                .map_err(|error| EngineError::Parse(format!("积分结果语法异常: {error:?}")))?
                .ok_or_else(|| EngineError::Parse("积分结果为空".into()))?;
            Ok(matches!(
                crate::semantic_core::ExpressionView::new(env, &parsed).head(),
                Some("Integrate" | "Int")
            ))
        })?;
        let output_source = if unresolved {
            format!("Integrate({})({source})", request.variable)
        } else {
            format!("({representative})+{}", request.arbitrary_constant)
        };
        let metadata = if unresolved {
            ResultMetadata::unresolved(Exactness::Symbolic, OutcomeReason::AlgorithmUncovered)
        } else {
            ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty())
        };
        let mut semantics = SemanticState {
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
        let parsed = crate::semantic_core::parse_engine_expression(&output_source)?;
        if unresolved {
            crate::semantic_core::promote_held_application(
                "Integrate",
                &parsed.raw_expression(),
                &mut semantics,
            )?;
        }
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
        let events = derivation
            .steps
            .into_iter()
            .map(|step| {
                let is_family = step.rule == "antiderivative-family";
                let held_family = unresolved && is_family;
                RuleEvent {
                    rule: if held_family {
                        "hold-integral".into()
                    } else {
                        step.rule
                    },
                    input: input.reference(None),
                    additional_inputs: Vec::new(),
                    output: output.reference(None),
                    bindings: if is_family && !unresolved {
                        vec![
                            ("variable".into(), request.variable.clone()),
                            ("constant".into(), request.arbitrary_constant.clone()),
                        ]
                    } else {
                        vec![("variable".into(), request.variable.clone())]
                    },
                    conditions: Vec::new(),
                    payload: if is_family {
                        RulePayload::Rewrite
                    } else {
                        RulePayload::Structural
                    },
                    importance: match step.importance {
                        crate::steps::StepImportance::Routine => RuleImportance::Routine,
                        crate::steps::StepImportance::Normal => RuleImportance::Normal,
                        crate::steps::StepImportance::Key => RuleImportance::Key,
                    },
                    transformation: None,
                    presentation: Some(RulePresentation {
                        expression: if held_family {
                            output.print_source()
                        } else {
                            step.expr
                        },
                        explanation: if held_family {
                            "保留尚无闭式结果的积分对象。".into()
                        } else {
                            step.why
                        },
                        tex_override: (!held_family).then_some(step.tex),
                    }),
                }
            })
            .collect::<Vec<_>>();
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefiniteIntegralRequest {
    pub variable: String,
    pub lower: String,
    pub upper: String,
}

impl DefiniteIntegralRequest {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_symbol(&self.variable, "积分变量")?;
        crate::input::validate_expression(&self.lower, "积分下限")?;
        crate::input::validate_expression(&self.upper, "积分上限")
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DefiniteIntegralOperation;

impl SemanticOperation<DefiniteIntegralRequest> for DefiniteIntegralOperation {
    fn compute(
        &self,
        engine: &mut dyn Engine,
        input: &crate::semantic_core::MathematicalObject,
        request: &DefiniteIntegralRequest,
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
            .eval_expr(&format!(
                "Integrate({},{},{}){source}",
                request.variable, request.lower, request.upper
            ))?
            .to_string();
        let unresolved = crate::input::with_parse_env(|env| {
            let parsed = yacas_rs::parser::parse_expression(env, &format!("{representative};"))
                .map_err(|error| EngineError::Parse(format!("定积分结果语法异常: {error:?}")))?
                .ok_or_else(|| EngineError::Parse("定积分结果为空".into()))?;
            Ok(crate::semantic_core::ExpressionView::new(env, &parsed).head() == Some("Integrate"))
        })?;
        let metadata = if unresolved {
            ResultMetadata::unresolved(Exactness::Symbolic, OutcomeReason::AlgorithmUncovered)
        } else {
            ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty())
        };
        let mut semantics = SemanticState {
            kind: if unresolved {
                ValueKind::Unevaluated
            } else {
                crate::input::with_parse_env(|env| {
                    let parsed =
                        yacas_rs::parser::parse_expression(env, &format!("{representative};"))
                            .expect("engine result parses")
                            .expect("engine result exists");
                    crate::semantic::analyze_tree(env, &parsed).semantic.kind
                })
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
        let parsed = crate::semantic_core::parse_engine_expression(&representative)?;
        if unresolved {
            crate::semantic_core::promote_held_application(
                "Integrate",
                &parsed.raw_expression(),
                &mut semantics,
            )?;
        }
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
        let events = crate::steps::derive_definite_with_verbosity(
            engine,
            &source,
            &request.variable,
            &request.lower,
            &request.upper,
            crate::steps::StepVerbosity::Detailed,
        )?
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
            conditions: Vec::new(),
            payload: RulePayload::Structural,
            importance: match step.importance {
                crate::steps::StepImportance::Routine => RuleImportance::Routine,
                crate::steps::StepImportance::Normal => RuleImportance::Normal,
                crate::steps::StepImportance::Key => RuleImportance::Key,
            },
            transformation: None,
            presentation: Some(RulePresentation {
                expression: step.expr,
                explanation: step.why,
                tex_override: Some(step.tex),
            }),
        })
        .collect();
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

    #[test]
    fn partial_integral_is_a_typed_callable_and_avoids_constant_collisions() {
        let partial = integral_partial(ObjectId(8), "x").unwrap();
        assert_eq!(partial.semantics.requirements, vec![Requirement::Operand]);
        assert!(matches!(partial.semantics.interpretation,
            SemanticInterpretation::PartialApplication(ref state)
                if state.operator == OperatorId::Integral));

        let mut engine = RustEngine::spawn().unwrap();
        let result = apply_integral_partial(&mut engine, &partial, &input("C*x")).unwrap();
        let output = result.value().unwrap();
        assert_eq!(output.semantics.kind, ValueKind::FunctionFamily);
        assert!(
            output.print_source().contains("C1"),
            "{}",
            output.print_source()
        );
    }

    #[test]
    fn definite_integral_returns_a_scalar_without_an_arbitrary_constant() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = DefiniteIntegralOperation
            .compute(
                &mut engine,
                &input("x^2"),
                &DefiniteIntegralRequest {
                    variable: "x".into(),
                    lower: "0".into(),
                    upper: "1".into(),
                },
            )
            .unwrap();
        let output = result.value().unwrap();
        assert_eq!(output.print_source(), "1/3");
        assert_eq!(output.semantics.kind, ValueKind::Scalar);
        assert!(!output.print_source().contains('C'));
    }

    #[test]
    fn unresolved_definite_integral_remains_held() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = DefiniteIntegralOperation
            .compute(
                &mut engine,
                &input("f(x)"),
                &DefiniteIntegralRequest {
                    variable: "x".into(),
                    lower: "0".into(),
                    upper: "1".into(),
                },
            )
            .unwrap();
        assert!(matches!(result.output, ComputationOutput::Held(_)));
        assert!(result
            .subject()
            .unwrap()
            .print_source()
            .starts_with("Integrate("));
    }
}
