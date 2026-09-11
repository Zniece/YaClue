//! Structured API for finite, infinite, and one-sided limits.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{strip_tex_delimiters, validate_expression, validate_symbol};
use crate::protocol::{Condition, ConditionSet, OutcomeReason, ResultMetadata};
use crate::semantic::{Exactness, ValueKind};
use crate::semantic_core::{
    object_from_source, CapabilitySet, Computation, ComputationOutput, ExpressionView,
    NormalizationLevel, NormalizationMetadata, NormalizationMode, ObjectDelta, ObjectId,
    ObjectReference, OperatorId, Requirement, RuleEvent, RuleImportance, RulePayload,
    RulePresentation, RuleTrace, SemanticInterpretation, SemanticOperation, SemanticState,
};
use crate::steps::{Step, StepVerbosity};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitDirection {
    Both,
    Left,
    Right,
}

/// Structured request for the object-native Limit operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitRequest {
    pub variable: String,
    pub at: String,
    pub direction: LimitDirection,
}

impl LimitRequest {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_symbol(&self.variable, "极限变量")?;
        validate_expression(&self.at, "趋近点")
    }
}

/// Build a curried Limit application that awaits its operand. It is a normal
/// mathematical object, not a command string or an effect.
pub fn limit_partial(
    id: ObjectId,
    request: &LimitRequest,
) -> Result<crate::semantic_core::MathematicalObject, EngineError> {
    request.validate()?;
    let source = match request.direction {
        LimitDirection::Both => format!("Limit({},{})", request.variable, request.at),
        LimitDirection::Left => format!("Limit({},{},Left)", request.variable, request.at),
        LimitDirection::Right => format!("Limit({},{},Right)", request.variable, request.at),
    };
    object_from_source(
        id,
        &source,
        SemanticState {
            kind: ValueKind::Unevaluated,
            interpretation: SemanticInterpretation::HeldApplication {
                operator: "Limit".into(),
            },
            metadata: ResultMetadata::unresolved(
                Exactness::Symbolic,
                OutcomeReason::AlgorithmUncovered,
            ),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: vec![Requirement::Operand],
        },
    )
}

/// Complete a previously built curried Limit application using an object
/// operand. The request is recovered from the partial object's AST, so its
/// parameter structure is not reconstructed from a display string.
pub fn apply_limit_partial(
    engine: &mut dyn Engine,
    partial: &crate::semantic_core::MathematicalObject,
    operand: &crate::semantic_core::MathematicalObject,
) -> Result<Computation, EngineError> {
    if !matches!(
        partial.semantics.interpretation,
        SemanticInterpretation::HeldApplication { ref operator } if operator == "Limit"
    ) || partial.semantics.requirements != [Requirement::Operand]
    {
        return Err(EngineError::InvalidInput(
            "对象不是等待 operand 的 Limit 部分应用".into(),
        ));
    }
    let request = crate::input::with_parse_env(|env| {
        let view = partial.view(env);
        if view.head() != Some("Limit") {
            return Err(EngineError::Parse("Limit 部分应用 AST 形态异常".into()));
        }
        let arguments = view.arguments();
        if !(2..=3).contains(&arguments.len()) {
            return Err(EngineError::Parse("Limit 部分应用参数数量异常".into()));
        }
        let direction = match arguments.get(2).and_then(ExpressionView::atom) {
            None => LimitDirection::Both,
            Some("Left") => LimitDirection::Left,
            Some("Right") => LimitDirection::Right,
            Some(_) => return Err(EngineError::Parse("Limit 部分应用方向异常".into())),
        };
        Ok(LimitRequest {
            variable: arguments[0].print_source(),
            at: arguments[1].print_source(),
            direction,
        })
    })?;
    LimitOperation.compute(engine, operand, &request)
}

/// Limit's domain implementation under the common semantic operation
/// contract. Other domains should expose an equivalent operation rather than
/// inventing another string-to-string interface.
#[derive(Debug, Default, Clone, Copy)]
pub struct LimitOperation;

impl SemanticOperation<LimitRequest> for LimitOperation {
    fn compute(
        &self,
        engine: &mut dyn Engine,
        input: &crate::semantic_core::MathematicalObject,
        request: &LimitRequest,
    ) -> Result<Computation, EngineError> {
        limit_computation_for_object(
            engine,
            input,
            &request.variable,
            &request.at,
            request.direction,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitStatus {
    Converged,
    PositiveInfinity,
    NegativeInfinity,
    DoesNotExist,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Equal,
    GreaterThan,
    LessThan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LimitCondition {
    Property {
        expression: String,
        fact: String,
    },
    Relation {
        left: String,
        relation: Relation,
        right: String,
    },
    All {
        conditions: Vec<LimitCondition>,
    },
    Any {
        conditions: Vec<LimitCondition>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct LimitResult {
    pub status: LimitStatus,
    pub expression: String,
    pub variable: String,
    pub at: String,
    pub value: String,
    pub tex: String,
    pub direction: LimitDirection,
    pub conditions: Vec<LimitCondition>,
}

/// Domain-owned result of the structured limit decision program.  Both the
/// eventual computation facade and the legacy Step projection consume this
/// representation; neither reparses the `StepsL'Data` response independently.
struct LimitTraceData {
    args: Vec<Expr>,
    final_node: Expr,
    final_value: String,
    conditions: Vec<LimitCondition>,
}

/// Domain facts emitted by the limit decision program.  This is deliberately
/// presentation-free: RuleTrace and the legacy teaching projection consume it
/// instead of independently inferring a branch from strings.
#[derive(Debug, Clone)]
struct LimitEventData {
    rule: &'static str,
    expression: String,
    explanation: String,
    payload: RulePayload,
    importance: RuleImportance,
    tex_override: Option<String>,
}

fn trace_data(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    at: &str,
    direction: LimitDirection,
) -> Result<LimitTraceData, EngineError> {
    let direction_symbol = match direction {
        LimitDirection::Both => "Both",
        LimitDirection::Left => "Left",
        LimitDirection::Right => "Right",
    };
    let data = engine.eval_expr(&format!(
        "StepsL'Data({expression},{variable},{at},{direction_symbol})"
    ))?;
    let Expr::Call { head, args } = data else {
        return Err(EngineError::Parse("极限步骤事件不是列表".into()));
    };
    if head != "List" || args.is_empty() {
        return Err(EngineError::Parse("极限步骤事件形态异常".into()));
    }
    let final_node = match args[0].to_string().as_str() {
        "Direct" | "LHopital" if args.len() == 3 => &args[2],
        "OneSided" if args.len() == 2 => &args[1],
        "Cancel" if args.len() == 8 => &args[7],
        "Transform" if args.len() == 6 => &args[5],
        _ => &args[0],
    };
    let (value, conditions) = unpack_conditional(final_node.clone())?;
    Ok(LimitTraceData {
        args,
        final_node: value.clone(),
        final_value: value.to_string(),
        conditions,
    })
}

/// First semantic-core adapter.  The limit algorithm remains the existing
/// tested implementation; this function makes its result and its terminal
/// mathematical fact available without constructing `Step` values.
pub fn limit_computation(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    at: &str,
    direction: LimitDirection,
) -> Result<Computation, EngineError> {
    validate_expression(expression, "极限表达式")?;
    validate_symbol(variable, "极限变量")?;
    validate_expression(at, "趋近点")?;
    let initial_metadata =
        ResultMetadata::unresolved(Exactness::Unknown, OutcomeReason::AlgorithmUncovered);
    let object = object_from_source(
        ObjectId(1),
        expression,
        SemanticState {
            kind: ValueKind::Unevaluated,
            interpretation: SemanticInterpretation::PlainExpression,
            metadata: initial_metadata,
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        },
    )?;
    limit_computation_for_object(engine, &object, variable, at, direction)
}

/// Object-native Limit boundary.  The operand's AST identity and revision are
/// the transition input; source printing below is only the temporary adapter
/// required by the existing Yacas limit program.
pub fn limit_computation_for_object(
    engine: &mut dyn Engine,
    operand: &crate::semantic_core::MathematicalObject,
    variable: &str,
    at: &str,
    direction: LimitDirection,
) -> Result<Computation, EngineError> {
    validate_symbol(variable, "极限变量")?;
    validate_expression(at, "趋近点")?;
    if !operand
        .semantics
        .capabilities
        .contains(crate::semantic_core::ObjectCapability::EvaluateLimit)
    {
        return Err(EngineError::InvalidInput(
            "该数学对象不具备取极限能力".into(),
        ));
    }
    let expression = operand.print_source();
    let mut object = operand.clone();
    let input = object.reference(None);
    let data = trace_data(engine, &expression, variable, at, direction)?;
    let result = limit_from_trace_data(engine, &expression, variable, at, direction, &data)?;
    let metadata = limit_metadata(&result)?;
    let application_source = format!("Limit({variable},{at})({expression})");
    let produces_value = metadata.resolution == crate::protocol::ResolutionState::Solved;
    // Infinity is an extended-real Limit conclusion, not an ordinary
    // differentiable expression.  Its capability boundary must prevent a
    // later operation from silently treating it as a constant.
    let output_capabilities = if matches!(
        result.status,
        LimitStatus::Converged | LimitStatus::Unresolved
    ) {
        CapabilitySet::symbolic_expression()
    } else {
        CapabilitySet::empty()
    };
    let output_source = if produces_value {
        result.value.as_str()
    } else {
        application_source.as_str()
    };
    let interpretation = match metadata.resolution {
        crate::protocol::ResolutionState::Solved => SemanticInterpretation::PlainExpression,
        crate::protocol::ResolutionState::Unresolved => SemanticInterpretation::HeldApplication {
            operator: "Limit".into(),
        },
        crate::protocol::ResolutionState::NoResult => {
            SemanticInterpretation::StructuredUnevaluated {
                reason: "limit does not exist".into(),
            }
        }
    };
    let parsed_output = object_from_source(
        object.id,
        output_source,
        SemanticState {
            kind: if produces_value {
                ValueKind::Scalar
            } else {
                ValueKind::Unevaluated
            },
            interpretation: interpretation.clone(),
            metadata: metadata.clone(),
            capabilities: output_capabilities,
            requirements: Vec::new(),
        },
    )?;
    let output_ast = parsed_output.raw_expression();
    let output_kind = if produces_value {
        crate::input::with_parse_env(|env| {
            crate::semantic::analyze_tree(env, &output_ast)
                .semantic
                .kind
        })
    } else {
        ValueKind::Unevaluated
    };
    let normalization_assumptions = metadata.conditions.conditions().to_vec();
    object.apply(ObjectDelta {
        expression: Some(output_ast),
        semantics: Some(SemanticState {
            kind: output_kind,
            interpretation,
            metadata,
            capabilities: output_capabilities,
            requirements: Vec::new(),
        }),
        overlay: None,
        normalization: produces_value.then_some(NormalizationMetadata {
            level: NormalizationLevel::Domain,
            assumptions: normalization_assumptions,
            mode: NormalizationMode::Operation(OperatorId::Limit),
        }),
    });
    let output = object.reference(None);
    Ok(Computation {
        output: match result.status {
            LimitStatus::Converged
            | LimitStatus::PositiveInfinity
            | LimitStatus::NegativeInfinity => ComputationOutput::Value(object),
            LimitStatus::Unresolved => ComputationOutput::Held(object),
            LimitStatus::DoesNotExist => ComputationOutput::NoValue(object),
        },
        trace: Some(RuleTrace {
            events: limit_rule_events(engine, &data, &result, input, output)?,
        }),
        certificates: Vec::new(),
        effects: Vec::new(),
    })
}

fn limit_rule_events(
    engine: &mut dyn Engine,
    data: &LimitTraceData,
    result: &LimitResult,
    input: ObjectReference,
    output: ObjectReference,
) -> Result<Vec<RuleEvent>, EngineError> {
    let conditions = result
        .conditions
        .iter()
        .flat_map(limit_condition_delta)
        .collect::<Vec<_>>();
    let make =
        |rule: &'static str, payload, importance, expression: String, explanation: String| {
            let event = LimitEventData {
                rule,
                expression,
                explanation,
                payload,
                importance,
                tex_override: None,
            };
            RuleEvent {
                rule: event.rule.into(),
                input: input.clone(),
                additional_inputs: Vec::new(),
                output: output.clone(),
                bindings: vec![
                    ("variable".into(), result.variable.clone()),
                    ("at".into(), result.at.clone()),
                ],
                conditions: conditions.clone(),
                payload: event.payload,
                importance: event.importance,
                presentation: Some(RulePresentation {
                    expression: event.expression,
                    explanation: event.explanation,
                    tex_override: event.tex_override,
                }),
            }
        };
    let method = data.args.first().map(ToString::to_string);
    let mut events = vec![make(
        "limit-start",
        RulePayload::Structural,
        RuleImportance::Routine,
        format!(
            "Limit({},{})({})",
            result.variable, result.at, result.expression
        ),
        "建立极限问题".into(),
    )];
    let method_events = match method.as_deref() {
        Some("Direct") => vec![make(
            "limit-direct-substitution",
            RulePayload::Rewrite,
            RuleImportance::Key,
            data.args[1].to_string(),
            format!("直接代入 {} = {}", result.variable, result.at),
        )],
        Some("OneSided") => vec![make(
            "limit-one-sided-approach",
            RulePayload::Decision,
            RuleImportance::Key,
            result.value.clone(),
            "判断单侧趋近的符号与大小".into(),
        )],
        Some("Cancel") => vec![
            make(
                "limit-indeterminate-form",
                RulePayload::Inference,
                RuleImportance::Normal,
                format!("{}/{}", data.args[1], data.args[2]),
                "直接代入后得到 0/0 型不定式".into(),
            ),
            make(
                "limit-factor",
                RulePayload::Rewrite,
                RuleImportance::Normal,
                format!(
                    "(({})*({}))/(({})*({}))",
                    data.args[3], data.args[5], data.args[4], data.args[5]
                ),
                "因式分解并显露公共因子".into(),
            ),
            make(
                "limit-cancel-common-factor",
                RulePayload::Rewrite,
                RuleImportance::Key,
                data.args[6].to_string(),
                "在趋近点之外约去公共因子".into(),
            ),
        ],
        Some("LHopital") => {
            let mut events = vec![make(
                "limit-indeterminate-form",
                RulePayload::Inference,
                RuleImportance::Normal,
                "0/0".into(),
                "识别可应用洛必达法则的不定式".into(),
            )];
            if let Expr::Call { head, args } = &data.args[1] {
                if head == "List" {
                    for (index, event) in args.iter().enumerate() {
                        if let Expr::Call { head, args } = event {
                            if head == "List" && args.len() == 3 {
                                events.push(make(
                                    "limit-lhopital",
                                    RulePayload::Rewrite,
                                    RuleImportance::Key,
                                    args[2].to_string(),
                                    format!("第 {} 次应用洛必达法则", index + 1),
                                ));
                            }
                        }
                    }
                }
            }
            events
        }
        Some("Transform") => vec![
            make(
                "limit-indeterminate-form",
                RulePayload::Inference,
                RuleImportance::Normal,
                match data.args[1].to_string().as_str() {
                    "Product" => format!("{}*{}", data.args[2], data.args[3]),
                    "Difference" => format!("{}-{}", data.args[2], data.args[3]),
                    "Power" => format!("({})^({})", data.args[2], data.args[3]),
                    _ => format!("{} {} {}", data.args[2], data.args[1], data.args[3]),
                },
                "识别需要变换的不定式".into(),
            ),
            make(
                match data.args[1].to_string().as_str() {
                    "Product" => "limit-transform-product",
                    "Difference" => "limit-transform-difference",
                    "Power" => "limit-transform-power",
                    _ => "limit-transform",
                },
                RulePayload::Rewrite,
                RuleImportance::Key,
                data.args[4].to_string(),
                "改写为可计算的极限形式".into(),
            ),
        ],
        _ => Vec::new(),
    };
    events.extend(method_events);
    let payload = match result.status {
        LimitStatus::Converged | LimitStatus::PositiveInfinity | LimitStatus::NegativeInfinity => {
            RulePayload::Convergence
        }
        LimitStatus::DoesNotExist => RulePayload::Decision,
        LimitStatus::Unresolved => RulePayload::Structural,
    };
    if let Some(condition) = result.conditions.first() {
        events.push(make(
            "limit-condition",
            RulePayload::Inference,
            RuleImportance::Normal,
            format_condition_expression(condition),
            "在此参数条件下采用对应的极限分支".into(),
        ));
    }
    let direct_is_terminal = matches!(method.as_deref(), Some("Direct"))
        && data
            .args
            .get(1)
            .is_some_and(|value| value.to_string() == result.value)
        && result.conditions.is_empty();
    if !direct_is_terminal && !matches!(method.as_deref(), Some("OneSided")) {
        events.push(make(
            "limit-result",
            payload,
            RuleImportance::Key,
            result.value.clone(),
            "得到极限结论".into(),
        ));
    }
    let direction_suffix = match result.direction {
        LimitDirection::Both => "",
        LimitDirection::Left => "^{-}",
        LimitDirection::Right => "^{+}",
    };
    let rendered = engine
        .render_tex_batch(&[result.expression.clone(), result.at.clone()])?
        .into_iter()
        .map(|tex| strip_tex_delimiters(&tex))
        .collect::<Vec<_>>();
    if let Some(presentation) = events
        .first_mut()
        .and_then(|event| event.presentation.as_mut())
    {
        presentation.tex_override = Some(format!(
            "\\lim_{{{} \\to {}{}}} {}",
            result.variable, rendered[1], direction_suffix, rendered[0]
        ));
    }
    Ok(events)
}

fn limit_condition_delta(condition: &LimitCondition) -> Vec<Condition> {
    match condition {
        LimitCondition::Property { expression, fact } => vec![match fact.as_str() {
            "Positive" | "positive" => Condition::Positive {
                expression: expression.clone(),
            },
            "Negative" | "negative" => Condition::Negative {
                expression: expression.clone(),
            },
            "NonZero" | "non_zero" => Condition::NonZero {
                expression: expression.clone(),
            },
            "Real" | "real" => Condition::Real {
                expression: expression.clone(),
            },
            "Integer" | "integer" => Condition::Integer {
                expression: expression.clone(),
            },
            _ => Condition::Unknown {
                description: format_condition_expression(condition),
            },
        }],
        LimitCondition::Relation {
            left,
            relation: Relation::GreaterThan,
            right,
        } if right == "0" => {
            vec![Condition::Positive {
                expression: left.clone(),
            }]
        }
        LimitCondition::All { conditions } => {
            conditions.iter().flat_map(limit_condition_delta).collect()
        }
        _ => vec![Condition::Unknown {
            description: format_condition_expression(condition),
        }],
    }
}

fn limit_metadata(result: &LimitResult) -> Result<ResultMetadata, EngineError> {
    let conditions =
        ConditionSet::new(
            result
                .conditions
                .iter()
                .map(|condition| Condition::Unknown {
                    description: format!("{condition:?}"),
                }),
        )?;
    Ok(match result.status {
        LimitStatus::Converged | LimitStatus::PositiveInfinity | LimitStatus::NegativeInfinity => {
            ResultMetadata::solved(Exactness::Symbolic, conditions)
        }
        LimitStatus::DoesNotExist => {
            ResultMetadata::no_result(Exactness::Unknown, OutcomeReason::MathematicalAbsence)
        }
        LimitStatus::Unresolved => {
            ResultMetadata::unresolved(Exactness::Unknown, OutcomeReason::AlgorithmUncovered)
        }
    })
}

pub fn limit_steps(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    at: &str,
    direction: LimitDirection,
) -> Result<Vec<Step>, EngineError> {
    limit_steps_with_verbosity(
        engine,
        expression,
        variable,
        at,
        direction,
        StepVerbosity::Detailed,
    )
}

pub fn limit_steps_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    at: &str,
    direction: LimitDirection,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    validate_expression(expression, "极限表达式")?;
    validate_expression(at, "趋近点")?;
    validate_symbol(variable, "极限变量")?;
    let computation = limit_computation(engine, expression, variable, at, direction)?;
    crate::steps::render_rule_trace(
        engine,
        computation
            .trace
            .as_ref()
            .expect("limit computation always records its rule trace"),
        verbosity,
    )
}

fn format_condition_expression(condition: &LimitCondition) -> String {
    match condition {
        LimitCondition::Property { expression, fact } => {
            format!("ConditionProperty({expression},{fact})")
        }
        LimitCondition::Relation {
            left,
            relation,
            right,
        } => {
            let operator = match relation {
                Relation::Equal => "==",
                Relation::GreaterThan => ">",
                Relation::LessThan => "<",
            };
            format!("{left}{operator}{right}")
        }
        LimitCondition::All { conditions } => format!(
            "ConditionAnd({})",
            conditions
                .iter()
                .map(format_condition_expression)
                .collect::<Vec<_>>()
                .join(",")
        ),
        LimitCondition::Any { conditions } => format!(
            "ConditionOr({})",
            conditions
                .iter()
                .map(format_condition_expression)
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

pub fn limit(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    at: &str,
    direction: LimitDirection,
) -> Result<LimitResult, EngineError> {
    validate_expression(expression, "极限表达式")?;
    validate_expression(at, "趋近点")?;
    validate_symbol(variable, "极限变量")?;

    let data = trace_data(engine, expression, variable, at, direction)?;
    limit_from_trace_data(engine, expression, variable, at, direction, &data)
}

fn limit_from_trace_data(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    at: &str,
    direction: LimitDirection,
    data: &LimitTraceData,
) -> Result<LimitResult, EngineError> {
    let status = classify(&data.final_node, &data.final_value);
    let tex = strip_tex_delimiters(&engine.eval(&data.final_value)?.tex);
    Ok(LimitResult {
        status,
        expression: expression.trim().into(),
        variable: variable.into(),
        at: at.trim().into(),
        value: data.final_value.clone(),
        tex,
        direction,
        conditions: data.conditions.clone(),
    })
}

fn unpack_conditional(expr: Expr) -> Result<(Expr, Vec<LimitCondition>), EngineError> {
    let Expr::Call { head, mut args } = expr else {
        return Ok((expr, vec![]));
    };
    if head != "ConditionalValue" || args.len() != 2 {
        return Ok((Expr::Call { head, args }, vec![]));
    }
    let condition_expr = args.pop().unwrap();
    let value = args.pop().unwrap();
    let conditions = vec![parse_condition(&condition_expr)?];
    Ok((value, conditions))
}

fn parse_condition(expr: &Expr) -> Result<LimitCondition, EngineError> {
    match expr {
        Expr::Call { head, args }
            if [
                "=",
                "==",
                ">",
                "<",
                "ConditionEqual",
                "ConditionGreater",
                "ConditionLess",
            ]
            .contains(&head.as_str())
                && args.len() == 2 =>
        {
            let relation = match head.as_str() {
                "=" | "==" | "ConditionEqual" => Relation::Equal,
                ">" | "ConditionGreater" => Relation::GreaterThan,
                "<" | "ConditionLess" => Relation::LessThan,
                _ => unreachable!(),
            };
            Ok(LimitCondition::Relation {
                left: args[0].to_string(),
                relation,
                right: args[1].to_string(),
            })
        }
        Expr::Call { head, args } if head == "ConditionProperty" && args.len() == 2 => {
            Ok(LimitCondition::Property {
                expression: args[0].to_string(),
                fact: args[1].to_string(),
            })
        }
        Expr::Call { head, args } if (head == "ConditionAnd" || head == "ConditionOr") => {
            let conditions = args
                .iter()
                .map(parse_condition)
                .collect::<Result<Vec<_>, _>>()?;
            if head == "ConditionAnd" {
                Ok(LimitCondition::All { conditions })
            } else {
                Ok(LimitCondition::Any { conditions })
            }
        }
        other => Err(EngineError::Parse(format!("无法识别的极限条件: {other}"))),
    }
}

fn classify(expr: &Expr, _value: &str) -> LimitStatus {
    if matches!(expr, Expr::Call { head, .. } if head == "Limit") {
        LimitStatus::Unresolved
    } else if matches!(expr, Expr::Symbol(symbol) if symbol == "Infinity") {
        LimitStatus::PositiveInfinity
    } else if matches!(
        expr,
        Expr::Call { head, args }
            if head == "-" && args.len() == 1
                && matches!(&args[0], Expr::Symbol(symbol) if symbol == "Infinity")
    ) {
        LimitStatus::NegativeInfinity
    } else if matches!(expr, Expr::Symbol(symbol) if symbol == "Undefined") {
        LimitStatus::DoesNotExist
    } else {
        LimitStatus::Converged
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;

    #[test]
    fn classifies_finite_infinite_and_directional_limits() {
        let mut engine = RustEngine::spawn().unwrap();
        let finite = limit(&mut engine, "Sin(x)/x", "x", "0", LimitDirection::Both).unwrap();
        assert_eq!(finite.status, LimitStatus::Converged);
        assert_eq!(finite.value, "1");
        assert!(!finite.tex.is_empty());

        let right = limit(&mut engine, "1/x", "x", "0", LimitDirection::Right).unwrap();
        assert_eq!(right.status, LimitStatus::PositiveInfinity);
        let left = limit(&mut engine, "1/x", "x", "0", LimitDirection::Left).unwrap();
        assert_eq!(left.status, LimitStatus::NegativeInfinity, "{}", left.value);
        let both = limit(&mut engine, "1/x", "x", "0", LimitDirection::Both).unwrap();
        assert_eq!(both.status, LimitStatus::DoesNotExist);

        let abs_right = limit(&mut engine, "Abs(x)/x", "x", "0", LimitDirection::Right).unwrap();
        let abs_left = limit(&mut engine, "Abs(x)/x", "x", "0", LimitDirection::Left).unwrap();
        assert_eq!(abs_right.value, "1");
        assert_eq!(abs_left.value, "-1");
    }

    #[test]
    fn explains_direct_substitution_and_polynomial_cancellation() {
        let mut engine = RustEngine::spawn().unwrap();
        let direct = limit_steps(&mut engine, "x^2+1", "x", "2", LimitDirection::Both).unwrap();
        assert_eq!(direct.last().unwrap().rule, "limit-direct-substitution");
        assert_eq!(direct.last().unwrap().expr, "5");

        let cancellation =
            limit_steps(&mut engine, "(x^2-4)/(x-2)", "x", "2", LimitDirection::Both).unwrap();
        assert_eq!(
            cancellation
                .iter()
                .map(|step| step.rule.as_str())
                .collect::<Vec<_>>(),
            vec![
                "limit-start",
                "limit-indeterminate-form",
                "limit-factor",
                "limit-cancel-common-factor",
                "limit-result"
            ]
        );
        assert_eq!(cancellation[1].expr, "0/0");
        assert_eq!(cancellation[3].expr, "(x + 2)");
        assert_eq!(cancellation[4].expr, "4");
        assert!(cancellation.iter().all(|step| !step.tex.is_empty()));

        let higher_degree = limit_steps(
            &mut engine,
            "(x^3-x)/(x^2-1)",
            "x",
            "1",
            LimitDirection::Both,
        )
        .unwrap();
        assert!(higher_degree
            .iter()
            .any(|step| step.rule == "limit-cancel-common-factor"));
        assert_eq!(higher_degree.last().unwrap().expr, "1");
    }

    #[test]
    fn limit_step_verbosity_filters_before_rendering() {
        let mut engine = RustEngine::spawn().unwrap();
        let concise = limit_steps_with_verbosity(
            &mut engine,
            "(x^2-4)/(x-2)",
            "x",
            "2",
            LimitDirection::Both,
            StepVerbosity::Concise,
        )
        .unwrap();
        assert_eq!(concise.len(), 2);
        assert_eq!(concise[0].rule, "limit-cancel-common-factor");
        assert_eq!(concise[1].rule, "limit-result");
    }

    #[test]
    fn one_sided_steps_use_directional_behavior_instead_of_substitution() {
        let mut engine = RustEngine::spawn().unwrap();
        for (direction, expected) in [
            (LimitDirection::Right, "Infinity"),
            (LimitDirection::Left, "(- Infinity)"),
        ] {
            let steps = limit_steps(&mut engine, "1/x", "x", "0", direction).unwrap();
            assert_eq!(steps.last().unwrap().rule, "limit-one-sided-approach");
            assert_eq!(steps.last().unwrap().expr, expected);
            assert!(!steps
                .iter()
                .any(|step| step.rule == "limit-direct-substitution"));
        }

        let right = limit_steps(&mut engine, "Abs(x)/x", "x", "0", LimitDirection::Right).unwrap();
        let left = limit_steps(&mut engine, "Abs(x)/x", "x", "0", LimitDirection::Left).unwrap();
        assert_eq!(right.last().unwrap().expr, "1");
        assert_eq!(left.last().unwrap().expr, "-1");
    }

    #[test]
    fn explains_repeated_lhopital_without_restarting_the_limit() {
        let mut engine = RustEngine::spawn().unwrap();
        let steps = limit_steps(
            &mut engine,
            "(1-Cos(x))/x^2",
            "x",
            "0",
            LimitDirection::Both,
        )
        .unwrap();
        let lhopital = steps
            .iter()
            .filter(|step| step.rule == "limit-lhopital")
            .collect::<Vec<_>>();
        assert_eq!(lhopital.len(), 2);
        assert_eq!(lhopital[0].why, "第 1 次应用洛必达法则");
        assert_eq!(lhopital[1].why, "第 2 次应用洛必达法则");
        assert_eq!(steps.last().unwrap().rule, "limit-result");
        assert_eq!(steps.last().unwrap().expr, "(1 / 2)");
    }

    #[test]
    fn explains_product_difference_and_power_indeterminate_forms() {
        let mut engine = RustEngine::spawn().unwrap();
        for (expression, at, direction, rule, expected) in [
            (
                "x*Ln(x)",
                "0",
                LimitDirection::Right,
                "limit-transform-product",
                "0",
            ),
            (
                "1/(1-x)-1/(1-x^2)",
                "1",
                LimitDirection::Left,
                "limit-transform-difference",
                "Infinity",
            ),
            (
                "(1+1/x)^x",
                "Infinity",
                LimitDirection::Both,
                "limit-transform-power",
                "Exp(1)",
            ),
        ] {
            let steps = limit_steps(&mut engine, expression, "x", at, direction)
                .unwrap_or_else(|error| panic!("{expression}: {error}"));
            assert!(steps.iter().any(|step| step.rule == rule), "{expression}");
            assert_eq!(steps.last().unwrap().rule, "limit-result");
            assert_eq!(steps.last().unwrap().expr, expected, "{expression}");
            assert!(steps.iter().all(|step| !step.tex.is_empty()));
        }
    }

    #[test]
    fn supports_infinity_and_recognizes_unresolved_results() {
        let mut engine = RustEngine::spawn().unwrap();
        let infinity = limit(
            &mut engine,
            "Ln(x)/x",
            "x",
            "Infinity",
            LimitDirection::Both,
        )
        .unwrap();
        assert_eq!(infinity.status, LimitStatus::Converged);
        assert_eq!(infinity.value, "0");

        let held = Expr::Call {
            head: "Limit".into(),
            args: vec![],
        };
        assert_eq!(classify(&held, "Limit(x,0)f(x)"), LimitStatus::Unresolved);
    }

    #[test]
    fn parameter_limit_reports_the_assumption_it_used() {
        let mut engine = RustEngine::spawn().unwrap();
        engine.eval("Assume(n,Positive)").unwrap();
        let result = limit(
            &mut engine,
            "x^n/Ln(x)",
            "x",
            "Infinity",
            LimitDirection::Both,
        )
        .unwrap();
        assert_eq!(result.status, LimitStatus::PositiveInfinity);
        assert_eq!(result.value, "Infinity");
        assert_eq!(
            result.conditions,
            vec![LimitCondition::Relation {
                left: "n".into(),
                relation: Relation::GreaterThan,
                right: "0".into(),
            }]
        );

        engine.eval("ClearAssumptions()").unwrap();
        engine.eval("Assume(n,Negative)").unwrap();
        let negative = limit(
            &mut engine,
            "x^n/Ln(x)",
            "x",
            "Infinity",
            LimitDirection::Both,
        )
        .unwrap();
        assert_eq!(negative.status, LimitStatus::Converged);
        assert_eq!(negative.value, "0");
        assert_eq!(
            negative.conditions[0],
            LimitCondition::Relation {
                left: "n".into(),
                relation: Relation::LessThan,
                right: "0".into(),
            }
        );
    }

    #[test]
    fn limit_steps_show_the_parameter_condition_used_by_the_solver() {
        let mut engine = RustEngine::spawn().unwrap();
        engine.eval("Assume(n,Positive)").unwrap();
        let steps = limit_steps(
            &mut engine,
            "x^n/Ln(x)",
            "x",
            "Infinity",
            LimitDirection::Both,
        )
        .unwrap();
        let condition = steps
            .iter()
            .find(|step| step.rule == "limit-condition")
            .unwrap_or_else(|| panic!("steps: {steps:#?}"));
        assert_eq!(condition.expr, "n>0");
        assert_eq!(steps.last().unwrap().expr, "Infinity");
    }

    #[test]
    fn compound_parameter_limit_reports_the_derived_condition() {
        let mut engine = RustEngine::spawn().unwrap();
        engine.eval("Assume(m,Positive)").unwrap();
        let result = limit(
            &mut engine,
            "x^(2*m)/Ln(x)",
            "x",
            "Infinity",
            LimitDirection::Both,
        )
        .unwrap();
        assert_eq!(result.status, LimitStatus::PositiveInfinity);
        assert_eq!(result.value, "Infinity");
        assert_eq!(
            result.conditions,
            vec![LimitCondition::Relation {
                left: "(2 * m)".into(),
                relation: Relation::GreaterThan,
                right: "0".into(),
            }]
        );
    }

    #[test]
    fn parses_nested_condition_trees() {
        let condition = Expr::Call {
            head: "ConditionAnd".into(),
            args: vec![
                Expr::Call {
                    head: "ConditionGreater".into(),
                    args: vec![Expr::Symbol("n".into()), Expr::Number("0".into())],
                },
                Expr::Call {
                    head: "ConditionProperty".into(),
                    args: vec![Expr::Symbol("a".into()), Expr::Symbol("Real".into())],
                },
            ],
        };
        assert_eq!(
            parse_condition(&condition).unwrap(),
            LimitCondition::All {
                conditions: vec![
                    LimitCondition::Relation {
                        left: "n".into(),
                        relation: Relation::GreaterThan,
                        right: "0".into(),
                    },
                    LimitCondition::Property {
                        expression: "a".into(),
                        fact: "Real".into(),
                    },
                ],
            }
        );
    }

    #[test]
    fn rejects_invalid_requests_without_poisoning_the_engine() {
        let mut engine = RustEngine::spawn().unwrap();
        assert!(limit(&mut engine, "x);Echo(1);(x", "x", "0", LimitDirection::Both).is_err());
        assert!(limit(&mut engine, "x", "x;Echo(1)", "0", LimitDirection::Both).is_err());
        assert!(limit(&mut engine, "x", "x", "0;Echo(1)", LimitDirection::Both).is_err());
        assert_eq!(
            limit(&mut engine, "x", "x", "2", LimitDirection::Both)
                .unwrap()
                .value,
            "2"
        );
    }

    #[test]
    fn semantic_core_adapter_returns_result_without_steps() {
        let mut engine = RustEngine::spawn().unwrap();
        let computation =
            limit_computation(&mut engine, "Sin(x)/x", "x", "0", LimitDirection::Both).unwrap();
        assert_eq!(
            computation.value().unwrap().semantics.kind,
            ValueKind::Scalar
        );
        assert_eq!(computation.value().unwrap().revision.0, 1);
        let normalization = computation.value().unwrap().normalization.as_ref().unwrap();
        assert_eq!(
            normalization.revision,
            computation.value().unwrap().revision
        );
        assert_eq!(normalization.metadata.level, NormalizationLevel::Domain);
        let trace = computation.trace.as_ref().unwrap();
        assert!(trace.events.len() >= 2);
        assert_eq!(trace.events[0].rule, "limit-start");
        assert_eq!(trace.events[1].rule, "limit-indeterminate-form");
        assert_eq!(trace.events.last().unwrap().rule, "limit-result");
        assert_eq!(trace.events[1].input.revision.0, 0);
        assert_eq!(trace.events.last().unwrap().output.revision.0, 1);
    }

    #[test]
    fn limit_result_kind_is_derived_from_the_result_ast() {
        let mut engine = RustEngine::spawn().unwrap();
        let expression =
            limit_computation(&mut engine, "Sin(t)/t+x^2", "t", "0", LimitDirection::Both).unwrap();
        let output = expression.value().unwrap();
        assert_eq!(output.print_source(), "x^2+1");
        assert_eq!(output.semantics.kind, ValueKind::Expression);
        assert_eq!(
            output.normalization.as_ref().unwrap().metadata.level,
            NormalizationLevel::Domain
        );

        let scalar =
            limit_computation(&mut engine, "Sin(t)/t", "t", "0", LimitDirection::Both).unwrap();
        assert_eq!(scalar.value().unwrap().semantics.kind, ValueKind::Scalar);
    }

    #[test]
    fn limit_output_distinguishes_value_from_mathematical_absence() {
        let mut engine = RustEngine::spawn().unwrap();
        let solved =
            limit_computation(&mut engine, "Sin(x)/x", "x", "0", LimitDirection::Both).unwrap();
        assert!(matches!(&solved.output, ComputationOutput::Value(_)));
        let absent = limit_computation(&mut engine, "1/x", "x", "0", LimitDirection::Both).unwrap();
        assert!(matches!(&absent.output, ComputationOutput::NoValue(_)));
        assert!(absent.value().is_none());
        assert_eq!(
            absent.subject().unwrap().semantics.metadata.resolution,
            crate::protocol::ResolutionState::NoResult
        );
        assert!(limit_computation_for_object(
            &mut engine,
            absent.subject().unwrap(),
            "x",
            "0",
            LimitDirection::Both,
        )
        .is_err());
    }

    #[test]
    fn object_native_adapter_preserves_operand_identity() {
        let mut engine = RustEngine::spawn().unwrap();
        let operand = object_from_source(
            ObjectId(42),
            "Sin(x)/x",
            SemanticState {
                kind: ValueKind::Unevaluated,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata: ResultMetadata::unresolved(
                    Exactness::Unknown,
                    OutcomeReason::AlgorithmUncovered,
                ),
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            },
        )
        .unwrap();
        let computation =
            limit_computation_for_object(&mut engine, &operand, "x", "0", LimitDirection::Both)
                .unwrap();
        let trace = computation.trace.as_ref().unwrap();
        assert_eq!(trace.events[0].input.object, ObjectId(42));
        assert_eq!(computation.value().unwrap().id, ObjectId(42));
        assert_eq!(computation.value().unwrap().revision.0, 1);
    }

    #[test]
    fn partial_limit_is_a_held_object_with_an_operand_requirement() {
        let partial = limit_partial(
            ObjectId(9),
            &LimitRequest {
                variable: "x".into(),
                at: "0".into(),
                direction: LimitDirection::Both,
            },
        )
        .unwrap();
        assert!(matches!(
            partial.semantics.interpretation,
            SemanticInterpretation::HeldApplication { .. }
        ));
        assert_eq!(partial.semantics.requirements, vec![Requirement::Operand]);
        assert!(partial
            .semantics
            .capabilities
            .contains(crate::semantic_core::ObjectCapability::EvaluateLimit));
    }

    #[test]
    fn partial_limit_recovers_its_request_from_ast_before_execution() {
        let mut engine = RustEngine::spawn().unwrap();
        let partial = limit_partial(
            ObjectId(9),
            &LimitRequest {
                variable: "x".into(),
                at: "0".into(),
                direction: LimitDirection::Both,
            },
        )
        .unwrap();
        let operand = object_from_source(
            ObjectId(10),
            "Sin(x)/x",
            SemanticState {
                kind: ValueKind::Expression,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata: ResultMetadata::unresolved(
                    Exactness::Unknown,
                    OutcomeReason::AlgorithmUncovered,
                ),
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            },
        )
        .unwrap();
        let result = apply_limit_partial(&mut engine, &partial, &operand).unwrap();
        assert!(matches!(&result.output, ComputationOutput::Value(_)));
        assert_eq!(result.value().unwrap().print_source(), "1");
    }

    #[test]
    fn semantic_core_trace_projects_concise_steps_without_domain_result_access() {
        let mut engine = RustEngine::spawn().unwrap();
        let computation =
            limit_computation(&mut engine, "Sin(x)/x", "x", "0", LimitDirection::Both).unwrap();
        let steps = crate::steps::render_rule_trace(
            &mut engine,
            computation.trace.as_ref().unwrap(),
            StepVerbosity::Concise,
        )
        .unwrap();
        assert!(!steps.is_empty());
        assert_eq!(steps.last().unwrap().rule, "limit-result");
        assert_eq!(steps.last().unwrap().expr, "1");
    }
}
