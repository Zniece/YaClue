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
    SemanticInterpretation, SemanticOperation, SemanticState, UnarySemanticOperation,
};

/// Recursively execute a structural elaboration tree without reparsing its
/// children. Registered domain applications remain typed Held operands until
/// their own executor has lowered them.
pub fn can_execute_elaborated_tree(expression: &crate::elaboration::ElaboratedObject) -> bool {
    match &expression.form {
        crate::elaboration::MathematicalForm::Structural { .. } => {
            expression.children.iter().all(can_execute_elaborated_tree)
        }
        crate::elaboration::MathematicalForm::Application { head }
            if matches!(head.as_str(), "Limit" | "D" | "Deriv") =>
        {
            let operand_index = match (head.as_str(), expression.children.len()) {
                ("Limit", 2) => 0,
                ("Limit", 3) => 2,
                ("Limit", 4) => 3,
                ("D" | "Deriv", 2) => 1,
                ("D" | "Deriv", 3) => 2,
                _ => return false,
            };
            can_execute_elaborated_tree(&expression.children[operand_index])
        }
        crate::elaboration::MathematicalForm::Application { head } => {
            !crate::semantic_core::is_known_operator(head)
                && expression.children.iter().all(can_execute_elaborated_tree)
        }
        crate::elaboration::MathematicalForm::EffectApplication { .. } => true,
        crate::elaboration::MathematicalForm::OpaqueEngineValue { .. } => false,
        crate::elaboration::MathematicalForm::Relation { .. }
        | crate::elaboration::MathematicalForm::Collection => false,
        _ => true,
    }
}

pub fn has_migrated_calculus_descendant(expression: &crate::elaboration::ElaboratedObject) -> bool {
    expression.children.iter().any(|child| {
        matches!(&child.form,
            crate::elaboration::MathematicalForm::Application { head }
                if matches!(head.as_str(), "Limit" | "D" | "Deriv"))
            || has_migrated_calculus_descendant(child)
    })
}

pub fn has_effect_descendant(expression: &crate::elaboration::ElaboratedObject) -> bool {
    expression.children.iter().any(|child| {
        matches!(
            child.form,
            crate::elaboration::MathematicalForm::EffectApplication { .. }
        ) || has_effect_descendant(child)
    })
}

pub fn execute_elaborated_structure(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
) -> Result<Computation, EngineError> {
    if let crate::elaboration::MathematicalForm::Application { head } = &expression.form {
        if matches!(head.as_str(), "Limit" | "D" | "Deriv") {
            return execute_calculus_application(engine, expression, head);
        }
        if !crate::semantic_core::is_known_operator(head) {
            return execute_function_application(engine, expression, head);
        }
    }
    if matches!(
        expression.form,
        crate::elaboration::MathematicalForm::EffectApplication { .. }
    ) {
        return Ok(Computation {
            output: ComputationOutput::EffectsOnly,
            trace: None,
            certificates: Vec::new(),
            effects: Vec::new(),
        });
    }
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
    if child_computations
        .iter()
        .any(|child| matches!(child.output, ComputationOutput::EffectsOnly))
    {
        return Err(EngineError::InvalidInput(
            "带副作用的动作不能作为数学结构运算的操作数".into(),
        ));
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

fn execute_function_application(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    head: &str,
) -> Result<Computation, EngineError> {
    let mut children = expression
        .children
        .iter()
        .map(|child| execute_elaborated_structure(engine, child))
        .collect::<Result<Vec<_>, _>>()?;
    if children
        .iter()
        .any(|child| matches!(child.output, ComputationOutput::EffectsOnly))
    {
        return Err(EngineError::InvalidInput(format!(
            "带副作用的动作不能作为 {head} 的参数"
        )));
    }
    let arguments: Vec<_> = children
        .iter()
        .map(|child| {
            child
                .subject()
                .expect("mathematical function argument owns an object")
                .clone()
        })
        .collect();
    let rebuilt = expression.object.rebuild_application_with(&arguments)?;
    let conditions = conditions_from_objects(&arguments)?;
    let exactness = exactness_from_objects(&arguments);
    let no_value = children
        .iter()
        .any(|child| matches!(child.output, ComputationOutput::NoValue(_)));
    let held = children
        .iter()
        .any(|child| matches!(child.output, ComputationOutput::Held(_)));
    let mut output = expression.object.clone();
    let semantics = if no_value {
        SemanticState {
            kind: ValueKind::Unevaluated,
            interpretation: SemanticInterpretation::StructuredUnevaluated {
                reason: "argument has no mathematical value".into(),
            },
            metadata: metadata_with_conditions(
                ResultMetadata::no_result(exactness, OutcomeReason::MathematicalAbsence),
                &conditions,
            ),
            capabilities: CapabilitySet::empty(),
            requirements: Vec::new(),
        }
    } else if held {
        SemanticState {
            kind: ValueKind::Unevaluated,
            interpretation: SemanticInterpretation::HeldApplication {
                operator: head.into(),
            },
            metadata: metadata_with_conditions(
                ResultMetadata::unresolved(exactness, OutcomeReason::AlgorithmUncovered),
                &conditions,
            ),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        }
    } else {
        let rebuilt_object = crate::semantic_core::MathematicalObject::new(
            expression.object.id,
            rebuilt.clone(),
            expression.object.semantics.clone(),
        );
        let evaluated = engine
            .eval_expr(&rebuilt_object.print_source())?
            .to_string();
        let evaluated_object = object_from_source(
            expression.object.id,
            &evaluated,
            expression.object.semantics.clone(),
        )?;
        let kind = crate::input::with_parse_env(|env| {
            crate::semantic::analyze_tree(env, &evaluated_object.raw_expression())
                .semantic
                .kind
        });
        output = evaluated_object;
        SemanticState {
            kind,
            interpretation: SemanticInterpretation::PlainExpression,
            metadata: ResultMetadata::solved(exactness, conditions.clone()),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        }
    };
    output.apply(ObjectDelta {
        expression: (!matches!(semantics.metadata.resolution, ResolutionState::Solved))
            .then_some(rebuilt),
        semantics: Some(semantics),
        overlay: None,
        normalization: (!held && !no_value).then_some(NormalizationMetadata {
            level: NormalizationLevel::Structural,
            assumptions: conditions.conditions().to_vec(),
            mode: NormalizationMode::Safe,
        }),
    });
    let rule = if no_value {
        "propagate-no-value"
    } else if held {
        "hold-function-application"
    } else {
        "apply-function"
    };
    let event = RuleEvent {
        rule: rule.into(),
        input: arguments
            .first()
            .map(|argument| argument.reference(None))
            .unwrap_or_else(|| expression.object.reference(None)),
        additional_inputs: arguments
            .iter()
            .skip(1)
            .map(|argument| argument.reference(None))
            .collect(),
        output: output.reference(None),
        bindings: vec![("function".into(), head.into())],
        conditions: conditions.conditions().to_vec(),
        payload: RulePayload::Rewrite,
        importance: RuleImportance::Key,
        presentation: Some(RulePresentation {
            expression: output.print_source(),
            explanation: "将已类型化的参数应用到数学函数。".into(),
            tex_override: None,
        }),
    };
    let mut events = Vec::new();
    for child in &mut children {
        if let Some(trace) = child.trace.take() {
            events.extend(trace.events);
        }
    }
    events.push(event);
    Ok(Computation {
        output: if no_value {
            ComputationOutput::NoValue(output)
        } else if held {
            ComputationOutput::Held(output)
        } else {
            ComputationOutput::Value(output)
        },
        trace: Some(RuleTrace { events }),
        certificates: children
            .iter_mut()
            .flat_map(|child| std::mem::take(&mut child.certificates))
            .collect(),
        effects: children
            .iter_mut()
            .flat_map(|child| std::mem::take(&mut child.effects))
            .collect(),
    })
}

fn execute_calculus_application(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    head: &str,
) -> Result<Computation, EngineError> {
    let (operand_index, request) = match head {
        "Limit" => {
            let (operand_index, variable, at, direction) = match expression.children.as_slice() {
                [_operand, at] => (
                    0,
                    "x".to_string(),
                    at.object.print_source(),
                    crate::limits::LimitDirection::Both,
                ),
                [variable, at, _operand] => (
                    2,
                    variable.object.print_source(),
                    at.object.print_source(),
                    crate::limits::LimitDirection::Both,
                ),
                [variable, at, direction, _operand] => {
                    let direction = match direction.object.print_source().as_str() {
                        "Left" => crate::limits::LimitDirection::Left,
                        "Right" => crate::limits::LimitDirection::Right,
                        _ => {
                            return Err(EngineError::InvalidInput(
                                "极限方向应为 Left 或 Right".into(),
                            ))
                        }
                    };
                    (
                        3,
                        variable.object.print_source(),
                        at.object.print_source(),
                        direction,
                    )
                }
                _ => {
                    return Err(EngineError::InvalidInput(format!(
                        "Limit 不支持 {} 个参数",
                        expression.children.len()
                    )))
                }
            };
            (
                operand_index,
                CalculusRequest::Limit(crate::limits::LimitRequest {
                    variable,
                    at,
                    direction,
                }),
            )
        }
        "D" | "Deriv" => {
            let (operand_index, variable, order) =
                match expression.children.as_slice() {
                    [variable, _operand] => (1, variable.object.print_source(), 1),
                    [variable, order, _operand] => (
                        2,
                        variable.object.print_source(),
                        order.object.print_source().parse::<u32>().map_err(|_| {
                            EngineError::InvalidInput("求导阶数必须是正整数".into())
                        })?,
                    ),
                    _ => {
                        return Err(EngineError::InvalidInput(format!(
                            "{head} 不支持 {} 个参数",
                            expression.children.len()
                        )))
                    }
                };
            (
                operand_index,
                CalculusRequest::Derivative(crate::derivatives::DerivativeRequest {
                    variable,
                    order,
                }),
            )
        }
        _ => unreachable!(),
    };
    let mut operand = execute_elaborated_structure(engine, &expression.children[operand_index])?;
    if !matches!(operand.output, ComputationOutput::Value(_)) {
        return Ok(operand);
    }
    let input = operand.value().expect("checked value computation").clone();
    let required_capability = match &request {
        CalculusRequest::Limit(_) => ObjectCapability::EvaluateLimit,
        CalculusRequest::Derivative(_) => ObjectCapability::Differentiate,
    };
    if !input.semantics.capabilities.contains(required_capability) {
        let mut blocked = input.clone();
        let mut semantics = blocked.semantics.clone();
        semantics.kind = ValueKind::Unevaluated;
        semantics.interpretation = SemanticInterpretation::HeldApplication {
            operator: head.into(),
        };
        semantics.metadata = ResultMetadata::unresolved(
            semantics.metadata.exactness,
            OutcomeReason::UnsupportedOperation,
        );
        blocked.apply(ObjectDelta {
            expression: None,
            semantics: Some(semantics),
            overlay: None,
            normalization: None,
        });
        let mut computation = Computation {
            output: ComputationOutput::Held(blocked),
            trace: None,
            certificates: Vec::new(),
            effects: Vec::new(),
        };
        merge_prior_computation(&mut computation, &mut operand);
        return Ok(computation);
    }
    let mut computation = match request {
        CalculusRequest::Limit(request) => {
            crate::limits::LimitOperation.compute(engine, &input, &request)?
        }
        CalculusRequest::Derivative(request) => {
            crate::derivatives::DerivativeOperation.compute(engine, &input, &request)?
        }
    };
    merge_prior_computation(&mut computation, &mut operand);
    Ok(computation)
}

enum CalculusRequest {
    Limit(crate::limits::LimitRequest),
    Derivative(crate::derivatives::DerivativeRequest),
}

fn merge_prior_computation(current: &mut Computation, prior: &mut Computation) {
    let mut events = prior
        .trace
        .take()
        .map(|trace| trace.events)
        .unwrap_or_default();
    if let Some(trace) = current.trace.as_mut() {
        events.append(&mut trace.events);
        trace.events = events;
    } else if !events.is_empty() {
        current.trace = Some(RuleTrace { events });
    }
    current.certificates.append(&mut prior.certificates);
    current.effects.append(&mut prior.effects);
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
        let symbol = symbol(request.operation);
        let source = format!(
            "({}){symbol}({})",
            left.print_source(),
            right.print_source()
        );
        if left.semantics.metadata.resolution == ResolutionState::NoResult
            || right.semantics.metadata.resolution == ResolutionState::NoResult
        {
            return no_value_structure(
                request.output_id,
                &source,
                left,
                Some(right),
                request.operation,
            );
        }
        let capability = capability(request.operation);
        if !left.semantics.capabilities.contains(capability)
            || !right.semantics.capabilities.contains(capability)
        {
            return Err(EngineError::InvalidInput(
                "数学对象不具备该结构运算能力".into(),
            ));
        }
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
        if input.semantics.metadata.resolution == ResolutionState::NoResult {
            return no_value_structure(
                request.output_id,
                &format!("-({})", input.print_source()),
                input,
                None,
                request.operation,
            );
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

fn conditions_from_objects(
    objects: &[crate::semantic_core::MathematicalObject],
) -> Result<ConditionSet, EngineError> {
    ConditionSet::new(objects.iter().flat_map(|object| {
        object
            .semantics
            .metadata
            .conditions
            .conditions()
            .iter()
            .cloned()
    }))
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

fn exactness_from_objects(objects: &[crate::semantic_core::MathematicalObject]) -> Exactness {
    objects.iter().fold(Exactness::Exact, |combined, object| {
        match (combined, object.semantics.metadata.exactness) {
            (Exactness::Approximate, _) | (_, Exactness::Approximate) => Exactness::Approximate,
            (Exactness::Unknown, _) | (_, Exactness::Unknown) => Exactness::Unknown,
            (Exactness::Symbolic, _) | (_, Exactness::Symbolic) => Exactness::Symbolic,
            (Exactness::Exact, Exactness::Exact) => Exactness::Exact,
        }
    })
}

fn metadata_with_conditions(
    mut metadata: ResultMetadata,
    conditions: &ConditionSet,
) -> ResultMetadata {
    if !conditions.is_empty() {
        metadata.conditionality = Conditionality::Conditional;
        metadata.conditions = conditions.clone();
    }
    metadata
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

fn no_value_structure(
    output_id: ObjectId,
    source: &str,
    left: &crate::semantic_core::MathematicalObject,
    right: Option<&crate::semantic_core::MathematicalObject>,
    operation: ArithmeticOperation,
) -> Result<Computation, EngineError> {
    let conditions = combined_conditions(left, right)?;
    let mut metadata = ResultMetadata::no_result(
        combined_exactness(left, right),
        OutcomeReason::MathematicalAbsence,
    );
    if !conditions.is_empty() {
        metadata.conditionality = Conditionality::Conditional;
        metadata.conditions = conditions.clone();
    }
    let output = object_from_source(
        output_id,
        source,
        SemanticState {
            kind: ValueKind::Unevaluated,
            interpretation: SemanticInterpretation::StructuredUnevaluated {
                reason: "operand has no mathematical value".into(),
            },
            metadata,
            capabilities: CapabilitySet::empty(),
            requirements: Vec::new(),
        },
    )?;
    let event = RuleEvent {
        rule: "propagate-no-value".into(),
        input: left.reference(None),
        additional_inputs: right
            .into_iter()
            .map(|object| object.reference(None))
            .collect(),
        output: output.reference(None),
        bindings: vec![("operator".into(), symbol(operation).into())],
        conditions: conditions.conditions().to_vec(),
        payload: RulePayload::Inference,
        importance: RuleImportance::Key,
        presentation: Some(RulePresentation {
            expression: output.print_source(),
            explanation: "操作数没有数学值，因此结构运算也没有值。".into(),
            tex_override: None,
        }),
    };
    Ok(Computation {
        output: ComputationOutput::NoValue(output),
        trace: Some(RuleTrace {
            events: vec![event],
        }),
        certificates: Vec::new(),
        effects: Vec::new(),
    })
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

    #[test]
    fn recursively_executes_limit_and_derivative_inside_structures() {
        let mut engine = RustEngine::spawn().unwrap();
        let elaborated = crate::elaboration::elaborate("(Limit(t,0)(Sin(t)/t+x^2))+3").unwrap();
        let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
        assert!(matches!(result.output, ComputationOutput::Value(_)));
        assert_eq!(
            engine
                .eval_expr(&format!(
                    "Simplify(({})-(x^2+4))",
                    result.value().unwrap().print_source()
                ))
                .unwrap()
                .to_string(),
            "0"
        );
        assert!(result
            .trace
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| event.rule == "limit-result"));

        let chain = crate::elaboration::elaborate("D(x)((Limit(t,0)(Sin(t)/t+x^2)))").unwrap();
        let result = execute_elaborated_structure(&mut engine, &chain).unwrap();
        assert_eq!(result.value().unwrap().print_source(), "2*x");
        assert!(result.trace.as_ref().unwrap().events.len() >= 2);
    }

    #[test]
    fn no_value_propagates_through_outer_structures() {
        let mut engine = RustEngine::spawn().unwrap();
        let elaborated = crate::elaboration::elaborate("(Limit(x,0)(1/x))+1").unwrap();
        let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
        assert!(matches!(result.output, ComputationOutput::NoValue(_)));
        assert_eq!(
            result.subject().unwrap().semantics.metadata.resolution,
            ResolutionState::NoResult
        );
        assert!(result
            .trace
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| event.rule == "propagate-no-value"));
    }

    #[test]
    fn generic_functions_consume_nested_semantic_objects() {
        let mut engine = RustEngine::spawn().unwrap();
        let elaborated = crate::elaboration::elaborate("Sin(Limit(t,0)(Sin(t)/t))").unwrap();
        let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
        assert_eq!(result.value().unwrap().print_source(), "Sin(1)");
        assert!(result
            .trace
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| event.rule == "apply-function"));

        let chain = crate::elaboration::elaborate("D(x)(Sin(Limit(t,0)(Sin(t)/t+x)))").unwrap();
        let result = execute_elaborated_structure(&mut engine, &chain).unwrap();
        assert_eq!(result.value().unwrap().print_source(), "Cos(x+1)");
    }

    #[test]
    fn generic_functions_propagate_no_value_without_evaluation() {
        let mut engine = RustEngine::spawn().unwrap();
        let elaborated = crate::elaboration::elaborate("Sin(Limit(x,0)(1/x))").unwrap();
        let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
        assert!(matches!(result.output, ComputationOutput::NoValue(_)));
        assert!(result.subject().unwrap().print_source().contains("Limit"));
    }

    #[test]
    fn effects_are_rejected_as_function_or_structural_operands() {
        let mut engine = RustEngine::spawn().unwrap();
        for source in ["Sin(Plot(x,x,0,1))", "(Plot(x,x,0,1))+1"] {
            let elaborated = crate::elaboration::elaborate(source).unwrap();
            let error = match execute_elaborated_structure(&mut engine, &elaborated) {
                Ok(_) => panic!("{source} should reject an effect operand"),
                Err(error) => error,
            };
            assert!(error.to_string().contains("副作用"), "{source}: {error}");
        }
    }
}
