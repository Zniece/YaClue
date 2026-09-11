//! Typed structural composition for scalar/symbolic expressions.

use crate::engine::{Engine, EngineError};
use crate::protocol::{
    Condition, ConditionSet, Conditionality, OutcomeReason, ResolutionState, ResultMetadata,
};
use crate::semantic::{Exactness, ValueKind};
use crate::semantic_core::{
    object_from_source, BinarySemanticOperation, CapabilitySet, Computation, ComputationOutput,
    NormalizationLevel, NormalizationMetadata, NormalizationMode, ObjectCapability, ObjectDelta,
    ObjectId, RuleEvent, RuleImportance, RulePayload, RulePresentation, RuleTrace,
    SemanticInterpretation, SemanticState, UnarySemanticOperation,
};

/// Recursively execute a structural elaboration tree without reparsing its
/// children. Registered domain applications remain typed Held operands until
/// their own executor has lowered them.
pub fn execute_elaborated_structure(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
) -> Result<Computation, EngineError> {
    let crate::elaboration::MathematicalForm::Structural { operator } = &expression.form else {
        return Ok(Computation {
            output: if expression.object.semantics.metadata.resolution
                == ResolutionState::Unresolved
            {
                ComputationOutput::Held(expression.object.clone())
            } else {
                ComputationOutput::Value(expression.object.clone())
            },
            trace: None,
            certificates: Vec::new(),
            effects: Vec::new(),
        });
    };
    let operation = match (operator.as_str(), expression.children.len()) {
        ("-", 1) => ArithmeticOperation::Negate,
        ("+", 2) => ArithmeticOperation::Add,
        ("-", 2) => ArithmeticOperation::Subtract,
        ("*", 2) => ArithmeticOperation::Multiply,
        ("/", 2) => ArithmeticOperation::Divide,
        ("^", 2) => ArithmeticOperation::Power,
        _ => {
            return Err(EngineError::InvalidInput(format!(
                "结构运算 {operator} 不支持 {} 个操作数",
                expression.children.len()
            )))
        }
    };
    let mut child_computations = expression
        .children
        .iter()
        .map(|child| execute_elaborated_structure(engine, child))
        .collect::<Result<Vec<_>, _>>()?;
    let mut child_traces = Vec::new();
    for child in &mut child_computations {
        if let Some(trace) = child.trace.take() {
            child_traces.extend(trace.events);
        }
    }
    let request = ArithmeticRequest {
        output_id: expression.object.id,
        operation,
    };
    let mut computation = if operation == ArithmeticOperation::Negate {
        UnarySemanticOperation::compute(
            &ArithmeticOperationExecutor,
            engine,
            child_computations[0]
                .subject()
                .expect("mathematical child has an object"),
            &request,
        )?
    } else {
        BinarySemanticOperation::compute(
            &ArithmeticOperationExecutor,
            engine,
            child_computations[0]
                .subject()
                .expect("mathematical child has an object"),
            child_computations[1]
                .subject()
                .expect("mathematical child has an object"),
            &request,
        )?
    };
    if let Some(trace) = computation.trace.as_mut() {
        child_traces.append(&mut trace.events);
        trace.events = child_traces;
    }
    Ok(computation)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithmeticOperation {
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
    Negate,
}

#[derive(Debug, Clone, Copy)]
pub struct ArithmeticRequest {
    pub output_id: ObjectId,
    pub operation: ArithmeticOperation,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ArithmeticOperationExecutor;

impl BinarySemanticOperation<ArithmeticRequest> for ArithmeticOperationExecutor {
    fn compute(
        &self,
        engine: &mut dyn Engine,
        left: &crate::semantic_core::MathematicalObject,
        right: &crate::semantic_core::MathematicalObject,
        request: &ArithmeticRequest,
    ) -> Result<Computation, EngineError> {
        if request.operation == ArithmeticOperation::Negate {
            return Err(EngineError::InvalidInput(
                "一元负号不能作为二元运算执行".into(),
            ));
        }
        let capability = capability(request.operation);
        if !left.semantics.capabilities.contains(capability)
            || !right.semantics.capabilities.contains(capability)
        {
            return Err(EngineError::InvalidInput(
                "数学对象不具备该结构运算能力".into(),
            ));
        }
        if left.semantics.metadata.resolution == ResolutionState::NoResult
            || right.semantics.metadata.resolution == ResolutionState::NoResult
        {
            return Err(EngineError::InvalidInput("无值结论不能参与结构运算".into()));
        }
        let symbol = symbol(request.operation);
        let source = format!(
            "({}){symbol}({})",
            left.print_source(),
            right.print_source()
        );
        let held = left.semantics.metadata.resolution == ResolutionState::Unresolved
            || right.semantics.metadata.resolution == ResolutionState::Unresolved;
        let conditionally_sensitive = matches!(
            request.operation,
            ArithmeticOperation::Divide | ArithmeticOperation::Power
        ) && (left.semantics.kind != ValueKind::Scalar
            || right.semantics.kind != ValueKind::Scalar);
        let output_source = if held || conditionally_sensitive {
            source
        } else {
            engine.eval_expr(&source)?.to_string()
        };
        let conditions = combined_conditions(left, Some(right))?;
        let mut semantics = SemanticState {
            kind: if held {
                ValueKind::Unevaluated
            } else {
                ValueKind::Expression
            },
            interpretation: if held {
                SemanticInterpretation::HeldApplication {
                    operator: symbol.into(),
                }
            } else {
                SemanticInterpretation::PlainExpression
            },
            metadata: arithmetic_metadata(
                held,
                combined_exactness(left, Some(right)),
                conditions.clone(),
            ),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        };
        let mut output = object_from_source(request.output_id, &output_source, semantics.clone())?;
        if !held {
            semantics.kind = crate::input::with_parse_env(|env| {
                crate::semantic::analyze_tree(env, &output.raw_expression())
                    .semantic
                    .kind
            });
            output.apply(ObjectDelta {
                expression: None,
                semantics: Some(semantics),
                overlay: None,
                normalization: Some(NormalizationMetadata {
                    level: NormalizationLevel::Structural,
                    assumptions: conditions.conditions().to_vec(),
                    mode: NormalizationMode::Safe,
                }),
            });
        }
        let event = RuleEvent {
            rule: match request.operation {
                ArithmeticOperation::Add => "add",
                ArithmeticOperation::Subtract => "subtract",
                ArithmeticOperation::Multiply => "multiply",
                ArithmeticOperation::Divide => "divide",
                ArithmeticOperation::Power => "power",
                ArithmeticOperation::Negate => unreachable!(),
            }
            .into(),
            input: left.reference(None),
            additional_inputs: vec![right.reference(None)],
            output: output.reference(None),
            bindings: Vec::new(),
            conditions: conditions.conditions().to_vec(),
            payload: RulePayload::Rewrite,
            importance: RuleImportance::Key,
            presentation: Some(RulePresentation {
                expression: output.print_source(),
                explanation: "组合两个已类型化的数学对象。".into(),
                tex_override: None,
            }),
        };
        Ok(Computation {
            output: if held {
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

impl UnarySemanticOperation<ArithmeticRequest> for ArithmeticOperationExecutor {
    fn compute(
        &self,
        engine: &mut dyn Engine,
        input: &crate::semantic_core::MathematicalObject,
        request: &ArithmeticRequest,
    ) -> Result<Computation, EngineError> {
        if request.operation != ArithmeticOperation::Negate {
            return Err(EngineError::InvalidInput("该算术请求不是一元运算".into()));
        }
        if !input
            .semantics
            .capabilities
            .contains(ObjectCapability::Negate)
        {
            return Err(EngineError::InvalidInput(
                "数学对象不具备一元取负能力".into(),
            ));
        }
        if input.semantics.metadata.resolution == ResolutionState::NoResult {
            return Err(EngineError::InvalidInput("无值结论不能参与结构运算".into()));
        }
        let source = format!("-({})", input.print_source());
        let held = input.semantics.metadata.resolution == ResolutionState::Unresolved;
        let output_source = if held {
            source
        } else {
            engine.eval_expr(&source)?.to_string()
        };
        let conditions = combined_conditions(input, None)?;
        let mut semantics = SemanticState {
            kind: if held {
                ValueKind::Unevaluated
            } else {
                ValueKind::Expression
            },
            interpretation: if held {
                SemanticInterpretation::HeldApplication {
                    operator: "-".into(),
                }
            } else {
                SemanticInterpretation::PlainExpression
            },
            metadata: arithmetic_metadata(
                held,
                combined_exactness(input, None),
                conditions.clone(),
            ),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        };
        let mut output = object_from_source(request.output_id, &output_source, semantics.clone())?;
        if !held {
            semantics.kind = crate::input::with_parse_env(|env| {
                crate::semantic::analyze_tree(env, &output.raw_expression())
                    .semantic
                    .kind
            });
            output.apply(ObjectDelta {
                expression: None,
                semantics: Some(semantics),
                overlay: None,
                normalization: Some(NormalizationMetadata {
                    level: NormalizationLevel::Structural,
                    assumptions: conditions.conditions().to_vec(),
                    mode: NormalizationMode::Safe,
                }),
            });
        }
        let event = RuleEvent {
            rule: "negate".into(),
            input: input.reference(None),
            additional_inputs: Vec::new(),
            output: output.reference(None),
            bindings: Vec::new(),
            conditions: conditions.conditions().to_vec(),
            payload: RulePayload::Rewrite,
            importance: RuleImportance::Key,
            presentation: Some(RulePresentation {
                expression: output.print_source(),
                explanation: "对已类型化的数学对象取负。".into(),
                tex_override: None,
            }),
        };
        Ok(Computation {
            output: if held {
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

fn capability(operation: ArithmeticOperation) -> ObjectCapability {
    match operation {
        ArithmeticOperation::Add => ObjectCapability::Add,
        ArithmeticOperation::Subtract => ObjectCapability::Subtract,
        ArithmeticOperation::Multiply => ObjectCapability::Multiply,
        ArithmeticOperation::Divide => ObjectCapability::Divide,
        ArithmeticOperation::Power => ObjectCapability::Power,
        ArithmeticOperation::Negate => ObjectCapability::Negate,
    }
}

fn symbol(operation: ArithmeticOperation) -> &'static str {
    match operation {
        ArithmeticOperation::Add => "+",
        ArithmeticOperation::Subtract => "-",
        ArithmeticOperation::Multiply => "*",
        ArithmeticOperation::Divide => "/",
        ArithmeticOperation::Power => "^",
        ArithmeticOperation::Negate => "-",
    }
}

fn combined_conditions(
    left: &crate::semantic_core::MathematicalObject,
    right: Option<&crate::semantic_core::MathematicalObject>,
) -> Result<ConditionSet, EngineError> {
    let conditions: Vec<Condition> = left
        .semantics
        .metadata
        .conditions
        .conditions()
        .iter()
        .chain(
            right
                .into_iter()
                .flat_map(|object| object.semantics.metadata.conditions.conditions().iter()),
        )
        .cloned()
        .collect();
    ConditionSet::new(conditions)
}

fn combined_exactness(
    left: &crate::semantic_core::MathematicalObject,
    right: Option<&crate::semantic_core::MathematicalObject>,
) -> Exactness {
    let mut exactness = left.semantics.metadata.exactness;
    if let Some(right) = right {
        exactness = match (exactness, right.semantics.metadata.exactness) {
            (Exactness::Approximate, _) | (_, Exactness::Approximate) => Exactness::Approximate,
            (Exactness::Unknown, _) | (_, Exactness::Unknown) => Exactness::Unknown,
            (Exactness::Symbolic, _) | (_, Exactness::Symbolic) => Exactness::Symbolic,
            (Exactness::Exact, Exactness::Exact) => Exactness::Exact,
        };
    }
    exactness
}

fn arithmetic_metadata(
    held: bool,
    exactness: Exactness,
    conditions: ConditionSet,
) -> ResultMetadata {
    if !held {
        return ResultMetadata::solved(exactness, conditions);
    }
    let mut metadata = ResultMetadata::unresolved(exactness, OutcomeReason::AlgorithmUncovered);
    if !conditions.is_empty() {
        metadata.conditionality = Conditionality::Conditional;
        metadata.conditions = conditions;
    }
    metadata
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;
    use crate::semantic_core::{ObjectId, SemanticState};

    fn expression(
        id: u64,
        source: &str,
        resolution: ResolutionState,
    ) -> crate::semantic_core::MathematicalObject {
        object_from_source(
            ObjectId(id),
            source,
            SemanticState {
                kind: ValueKind::Expression,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata: match resolution {
                    ResolutionState::Solved => {
                        ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty())
                    }
                    ResolutionState::Unresolved => ResultMetadata::unresolved(
                        Exactness::Symbolic,
                        OutcomeReason::AlgorithmUncovered,
                    ),
                    ResolutionState::NoResult => ResultMetadata::no_result(
                        Exactness::Symbolic,
                        OutcomeReason::MathematicalAbsence,
                    ),
                },
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            },
        )
        .unwrap()
    }

    #[test]
    fn combines_two_typed_objects_and_records_both_provenances() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = BinarySemanticOperation::compute(
            &ArithmeticOperationExecutor,
            &mut engine,
            &expression(1, "x", ResolutionState::Solved),
            &expression(2, "x", ResolutionState::Solved),
            &ArithmeticRequest {
                output_id: ObjectId(3),
                operation: ArithmeticOperation::Add,
            },
        )
        .unwrap();
        assert_eq!(result.value().unwrap().print_source(), "2*x");
        let event = &result.trace.as_ref().unwrap().events[0];
        assert_eq!(event.input.object, ObjectId(1));
        assert_eq!(event.additional_inputs[0].object, ObjectId(2));
    }

    #[test]
    fn preserves_an_unresolved_operand_without_engine_lowering() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = BinarySemanticOperation::compute(
            &ArithmeticOperationExecutor,
            &mut engine,
            &expression(1, "Limit(x,0)(f(x))", ResolutionState::Unresolved),
            &expression(2, "x", ResolutionState::Solved),
            &ArithmeticRequest {
                output_id: ObjectId(3),
                operation: ArithmeticOperation::Multiply,
            },
        )
        .unwrap();
        assert!(matches!(result.output, ComputationOutput::Held(_)));
        assert!(result.subject().unwrap().print_source().contains("Limit"));
    }

    #[test]
    fn recursively_executes_every_structural_operator() {
        let mut engine = RustEngine::spawn().unwrap();
        let numeric = crate::elaboration::elaborate("2+3*4").unwrap();
        let result = execute_elaborated_structure(&mut engine, &numeric).unwrap();
        assert_eq!(result.value().unwrap().print_source(), "14");
        assert_eq!(result.trace.as_ref().unwrap().events.len(), 2);

        for source in ["x-1", "x/2", "x^2", "-(x-1)"] {
            let elaborated = crate::elaboration::elaborate(source).unwrap();
            let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
            assert!(
                matches!(result.output, ComputationOutput::Value(_)),
                "{source}"
            );
            let output = result.value().unwrap();
            assert_eq!(
                output.normalization.as_ref().unwrap().metadata.level,
                NormalizationLevel::Structural
            );
        }
    }

    #[test]
    fn structural_division_does_not_apply_conditional_cancellation() {
        let mut engine = RustEngine::spawn().unwrap();
        let elaborated = crate::elaboration::elaborate("x/x").unwrap();
        let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
        assert_eq!(result.value().unwrap().print_source(), "x/x");
        assert!(result
            .value()
            .unwrap()
            .normalization
            .as_ref()
            .unwrap()
            .metadata
            .assumptions
            .is_empty());
    }

    #[test]
    fn arithmetic_declares_its_minimum_input_normalization() {
        assert_eq!(
            BinarySemanticOperation::<ArithmeticRequest>::minimum_input_normalization(
                &ArithmeticOperationExecutor
            ),
            NormalizationLevel::Structural
        );
        assert_eq!(
            UnarySemanticOperation::<ArithmeticRequest>::minimum_input_normalization(
                &ArithmeticOperationExecutor
            ),
            NormalizationLevel::Structural
        );
    }
}
