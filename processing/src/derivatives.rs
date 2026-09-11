//! Object-native differentiation over the existing, tested `StepsD'Full`
//! decision program.  The script remains the algorithm fact source while this
//! module owns semantic state transitions and rule-trace construction.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{validate_expression, validate_symbol};
use crate::protocol::{ConditionSet, OutcomeReason, ResultMetadata};
use crate::semantic::{Exactness, ValueKind};
use crate::semantic_core::{
    object_from_source, CapabilitySet, Computation, ComputationOutput, ExpressionView,
    NormalizationLevel, NormalizationMetadata, NormalizationMode, ObjectDelta, ObjectId,
    OperatorId, Requirement, RuleEvent, RuleImportance, RulePayload, RulePresentation, RuleTrace,
    SemanticInterpretation, SemanticOperation, SemanticState,
};
use crate::steps::{Step, StepVerbosity};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DerivativeStatus {
    Computed,
    Unresolved,
}

/// JSON/product projection of the derivative's semantic conclusion.  It is
/// deliberately derived from the computation object, never from a Step.
#[derive(Debug, Clone, Serialize)]
pub struct DerivativeResult {
    pub status: DerivativeStatus,
    pub expression: String,
    pub variable: String,
    pub order: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivativeRequest {
    pub variable: String,
    pub order: u32,
}

impl DerivativeRequest {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_symbol(&self.variable, "求导变量")?;
        if self.order == 0 {
            return Err(EngineError::InvalidInput("求导阶数必须 >= 1".into()));
        }
        Ok(())
    }
}

/// Curried `D(x[, order])`, waiting for an operand in the semantic data plane.
pub fn derivative_partial(
    id: ObjectId,
    request: &DerivativeRequest,
) -> Result<crate::semantic_core::MathematicalObject, EngineError> {
    request.validate()?;
    let source = if request.order == 1 {
        format!("D({})", request.variable)
    } else {
        format!("D({},{})", request.variable, request.order)
    };
    object_from_source(
        id,
        &source,
        SemanticState {
            kind: ValueKind::Unevaluated,
            interpretation: SemanticInterpretation::PartialApplication(
                crate::semantic_core::operand_partial_state(
                    "D",
                    if request.order == 1 { 1 } else { 2 },
                )?,
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

pub fn apply_derivative_partial(
    engine: &mut dyn Engine,
    partial: &crate::semantic_core::MathematicalObject,
    operand: &crate::semantic_core::MathematicalObject,
) -> Result<Computation, EngineError> {
    crate::semantic_core::require_operand_partial(partial, OperatorId::Derivative)?;
    let _application = crate::semantic_core::complete_operand_partial(partial, operand)?;
    let request = crate::input::with_parse_env(|env| {
        let view = partial.view(env);
        if view.head() != Some("D") {
            return Err(EngineError::Parse("D 部分应用 AST 形态异常".into()));
        }
        let arguments = view.arguments();
        let order = match arguments.as_slice() {
            [variable] => {
                return Ok(DerivativeRequest {
                    variable: variable.print_source(),
                    order: 1,
                });
            }
            [_variable, order] => order
                .print_source()
                .parse()
                .map_err(|_| EngineError::Parse("D 部分应用阶数异常".into()))?,
            _ => return Err(EngineError::Parse("D 部分应用参数数量异常".into())),
        };
        Ok(DerivativeRequest {
            variable: arguments[0].print_source(),
            order,
        })
    })?;
    DerivativeOperation.compute(engine, operand, &request)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DerivativeOperation;

impl SemanticOperation<DerivativeRequest> for DerivativeOperation {
    fn compute(
        &self,
        engine: &mut dyn Engine,
        input: &crate::semantic_core::MathematicalObject,
        request: &DerivativeRequest,
    ) -> Result<Computation, EngineError> {
        derivative_computation_for_object(engine, input, request)
    }
}

#[derive(Clone)]
struct DerivativeFact {
    rule: String,
    expression: String,
    explanation: String,
    importance: RuleImportance,
}

fn facts(
    engine: &mut dyn Engine,
    expression: &str,
    request: &DerivativeRequest,
) -> Result<Vec<DerivativeFact>, EngineError> {
    let command = format!(
        "StepsD'Full({expression},{},{})",
        request.variable, request.order
    );
    let Expr::Call { head, args } = engine.eval_expr(&command)? else {
        return Err(EngineError::Parse("求导步骤事实不是列表".into()));
    };
    if head != "List" || args.is_empty() {
        return Err(EngineError::Eval("未能生成求导步骤".into()));
    }
    args.into_iter()
        .enumerate()
        .map(|(index, item)| {
            let Expr::Call { head, args } = item else {
                return Err(EngineError::Parse(format!("求导步骤事件 {index} 不是列表")));
            };
            if head != "List" || args.len() != 4 {
                return Err(EngineError::Parse(format!(
                    "求导步骤事件 {index} 必须是四字段 List"
                )));
            }
            let text = |item: &Expr, field| match item {
                Expr::Symbol(value)
                    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') =>
                {
                    Ok(value[1..value.len() - 1].to_owned())
                }
                _ => Err(EngineError::Parse(format!(
                    "求导步骤事件 {index} 的 {field} 必须是字符串"
                ))),
            };
            let importance = match &args[3] {
                Expr::Number(value) if value == "0" => RuleImportance::Routine,
                Expr::Number(value) if value == "1" => RuleImportance::Normal,
                Expr::Number(value) if value == "2" => RuleImportance::Key,
                _ => {
                    return Err(EngineError::Parse(format!(
                        "求导步骤事件 {index} 的 importance 必须是 0、1 或 2"
                    )))
                }
            };
            Ok(DerivativeFact {
                rule: text(&args[0], "rule")?,
                expression: args[1].to_string(),
                explanation: text(&args[2], "explanation")?,
                importance,
            })
        })
        .collect()
}

pub fn derivative_computation(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    order: u32,
) -> Result<Computation, EngineError> {
    validate_expression(expression, "求导表达式")?;
    let input = object_from_source(
        ObjectId(1),
        expression,
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
    )?;
    derivative_computation_for_object(
        engine,
        &input,
        &DerivativeRequest {
            variable: variable.into(),
            order,
        },
    )
}

pub fn derivative_computation_for_object(
    engine: &mut dyn Engine,
    operand: &crate::semantic_core::MathematicalObject,
    request: &DerivativeRequest,
) -> Result<Computation, EngineError> {
    request.validate()?;
    if !operand
        .semantics
        .capabilities
        .contains(crate::semantic_core::ObjectCapability::Differentiate)
    {
        return Err(EngineError::InvalidInput("该数学对象不具备求导能力".into()));
    }
    let source = operand.print_source();
    let facts = facts(engine, &source, request)?;
    let final_expression = facts
        .last()
        .expect("checked nonempty facts")
        .expression
        .clone();
    let unresolved = crate::input::with_parse_env(|env| {
        let tree = yacas_rs::parser::parse_expression(env, &format!("{final_expression};"))
            .map_err(|error| EngineError::Parse(format!("求导结果语法异常: {error:?}")))?
            .ok_or_else(|| EngineError::Parse("求导结果为空".into()))?;
        Ok(ExpressionView::new(env, &tree)
            .head()
            .is_some_and(|head| matches!(head, "D" | "Deriv")))
    })?;
    let metadata = if unresolved {
        ResultMetadata::unresolved(Exactness::Symbolic, OutcomeReason::AlgorithmUncovered)
    } else {
        ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty())
    };
    let mut output_object = operand.clone();
    let output_source = if unresolved {
        format!("D({},{})({source})", request.variable, request.order)
    } else {
        final_expression.clone()
    };
    let mut semantics = SemanticState {
        kind: ValueKind::Unevaluated,
        interpretation: if unresolved {
            SemanticInterpretation::HeldApplication {
                operator: "D".into(),
            }
        } else {
            SemanticInterpretation::PlainExpression
        },
        metadata,
        capabilities: CapabilitySet::symbolic_expression(),
        requirements: Vec::new(),
    };
    let output_ast =
        object_from_source(output_object.id, &output_source, semantics.clone())?.raw_expression();
    if !unresolved {
        semantics.kind = crate::input::with_parse_env(|env| {
            crate::semantic::analyze_tree(env, &output_ast)
                .semantic
                .kind
        });
    }
    let input_ref = output_object.reference(None);
    output_object.apply(ObjectDelta {
        expression: Some(output_ast),
        semantics: Some(semantics),
        overlay: None,
        normalization: (!unresolved).then_some(NormalizationMetadata {
            level: NormalizationLevel::Domain,
            assumptions: Vec::new(),
            mode: NormalizationMode::Operation(OperatorId::Derivative),
        }),
    });
    let output_ref = output_object.reference(None);
    let last = facts.len() - 1;
    let events = facts
        .into_iter()
        .enumerate()
        .map(|(index, fact)| RuleEvent {
            // Preserve the established rule vocabulary at the compatibility
            // projection boundary.  The trace itself supplies the missing
            // object/revision semantics without inventing display-only names.
            rule: fact.rule,
            input: input_ref.clone(),
            additional_inputs: Vec::new(),
            output: output_ref.clone(),
            bindings: vec![
                ("variable".into(), request.variable.clone()),
                ("order".into(), request.order.to_string()),
            ],
            conditions: Vec::new(),
            payload: if index == last {
                RulePayload::Rewrite
            } else {
                RulePayload::Structural
            },
            importance: if index == last {
                RuleImportance::Key
            } else {
                fact.importance
            },
            presentation: Some(RulePresentation {
                expression: fact.expression,
                explanation: fact.explanation,
                tex_override: None,
            }),
        })
        .collect();
    Ok(Computation {
        output: if unresolved {
            ComputationOutput::Held(output_object)
        } else {
            ComputationOutput::Value(output_object)
        },
        trace: Some(RuleTrace { events }),
        certificates: Vec::new(),
        effects: Vec::new(),
    })
}

pub fn derivative_result(
    computation: &Computation,
    request: &DerivativeRequest,
) -> DerivativeResult {
    let object = computation
        .subject()
        .expect("derivative computations always produce an object");
    DerivativeResult {
        status: match &computation.output {
            ComputationOutput::Held(_) => DerivativeStatus::Unresolved,
            ComputationOutput::Value(_) => DerivativeStatus::Computed,
            ComputationOutput::NoValue(_) | ComputationOutput::EffectsOnly => {
                unreachable!("derivative never produces an absent/effect-only result")
            }
        },
        expression: object.print_source(),
        variable: request.variable.clone(),
        order: request.order,
    }
}

pub fn derivative_steps_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    order: u32,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    let computation = derivative_computation(engine, expression, variable, order)?;
    crate::steps::render_rule_trace(
        engine,
        computation
            .trace
            .as_ref()
            .expect("derivative always emits trace"),
        verbosity,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;
    use crate::semantic_core::{object_from_source, ObjectId};

    #[test]
    fn semantic_operation_preserves_object_identity_and_projects_steps() {
        let mut engine = RustEngine::spawn().unwrap();
        let computation = derivative_computation(&mut engine, "Sin(x)^2", "x", 1).unwrap();
        assert!(matches!(computation.output, ComputationOutput::Value(_)));
        assert_eq!(
            computation.value().unwrap().print_source(),
            "2*Sin(x)*Cos(x)"
        );
        assert_eq!(computation.value().unwrap().revision.0, 1);
        assert_eq!(
            computation
                .value()
                .unwrap()
                .normalization
                .as_ref()
                .unwrap()
                .metadata
                .level,
            NormalizationLevel::Domain
        );
        let steps = crate::steps::render_rule_trace(
            &mut engine,
            computation.trace.as_ref().unwrap(),
            StepVerbosity::Concise,
        )
        .unwrap();
        assert_eq!(steps.last().unwrap().expr, "(2 * (Sin(x) * Cos(x)))");
    }

    #[test]
    fn partial_derivative_recovers_request_from_ast() {
        let mut engine = RustEngine::spawn().unwrap();
        let partial = derivative_partial(
            ObjectId(9),
            &DerivativeRequest {
                variable: "x".into(),
                order: 1,
            },
        )
        .unwrap();
        let operand = object_from_source(
            ObjectId(42),
            "x^2",
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
        let computation = apply_derivative_partial(&mut engine, &partial, &operand).unwrap();
        assert_eq!(computation.value().unwrap().id, ObjectId(42));
        assert_eq!(computation.value().unwrap().print_source(), "2*x");
    }

    #[test]
    fn differentiates_a_limit_value_as_the_same_mathematical_object() {
        let mut engine = RustEngine::spawn().unwrap();
        let limit = crate::limits::limit_computation(
            &mut engine,
            "Sin(t)/t+x^2",
            "t",
            "0",
            crate::limits::LimitDirection::Both,
        )
        .unwrap();
        let limited = limit.value().expect("this limit is solved");
        let derivative = DerivativeOperation
            .compute(
                &mut engine,
                limited,
                &DerivativeRequest {
                    variable: "x".into(),
                    order: 1,
                },
            )
            .unwrap();
        assert_eq!(derivative.value().unwrap().id, limited.id);
        assert_eq!(
            derivative.value().unwrap().revision.0,
            limited.revision.0 + 1
        );
        assert_eq!(derivative.value().unwrap().print_source(), "2*x");
        assert_eq!(
            derivative
                .value()
                .unwrap()
                .normalization
                .as_ref()
                .unwrap()
                .revision,
            derivative.value().unwrap().revision
        );
    }

    #[test]
    fn derivative_result_kind_is_derived_from_the_result_ast() {
        let mut engine = RustEngine::spawn().unwrap();
        let constant = derivative_computation(&mut engine, "x", "x", 1).unwrap();
        assert_eq!(constant.value().unwrap().print_source(), "1");
        assert_eq!(constant.value().unwrap().semantics.kind, ValueKind::Scalar);

        let expression = derivative_computation(&mut engine, "x^2", "x", 1).unwrap();
        assert_eq!(
            expression.value().unwrap().semantics.kind,
            ValueKind::Expression
        );
    }
}
