//! Improper integrals as a finite semantic reduction to ordinary definite
//! integrals and independent one-sided limits.

use crate::assumptions::{self, AssumptionFact, AssumptionSpec};
use crate::binding;
use crate::engine::{Engine, EngineError, Expr};
use crate::input::{
    fresh_internal_symbols, strip_tex_delimiters, validate_expression, validate_symbol,
};
use crate::limits::{self, LimitCondition, LimitDirection, LimitStatus, Relation};
use crate::objects::{
    ComponentStatus, DefinedObjectKind, DefinedObjectResult, DefinedObjectStatus, ObjectComponent,
    PrimitiveOperation,
};
use crate::protocol::{Condition, ConditionSet, OutcomeReason, ResultMetadata};
use crate::semantic::{Exactness, ValueKind};
#[cfg(test)]
use crate::semantic_core::object_from_source;
use crate::semantic_core::{
    CapabilitySet, Computation, ComputationOutput, NormalizationLevel, NormalizationMetadata,
    NormalizationMode, ObjectDelta, OperatorId, RuleEvent, RuleImportance, RulePayload,
    RulePresentation, RuleTrace, SemanticInterpretation, SemanticOperation, SemanticState,
};
use crate::steps::{render_events, StepEvent, StepImportance, StepVerbosity};

const MAX_SINGULAR_POINTS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinedIntegralOperationKind {
    Improper,
    PrincipalValue,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DefinedIntegralOperation;

impl SemanticOperation<(DefinedIntegralOperationKind, ImproperIntegralRequest)>
    for DefinedIntegralOperation
{
    fn compute(
        &self,
        engine: &mut dyn Engine,
        input: &crate::semantic_core::MathematicalObject,
        request: &(DefinedIntegralOperationKind, ImproperIntegralRequest),
    ) -> Result<Computation, EngineError> {
        if !input
            .semantics
            .capabilities
            .contains(crate::semantic_core::ObjectCapability::IntegrateDefined)
        {
            return Err(EngineError::InvalidInput(
                "该数学对象不具备定义型积分能力".into(),
            ));
        }
        let (kind, template) = request;
        let mut request = template.clone();
        request.expression = input.print_source();
        let result = match kind {
            DefinedIntegralOperationKind::Improper => {
                evaluate(engine, &request, Some(StepVerbosity::Detailed))?
            }
            DefinedIntegralOperationKind::PrincipalValue => {
                principal_value(engine, &request, Some(StepVerbosity::Detailed))?
            }
        };
        let (metadata, held, no_value) = match result.status {
            DefinedObjectStatus::Converged => (
                ResultMetadata::solved(Exactness::Symbolic, result.conditions.clone()),
                false,
                false,
            ),
            DefinedObjectStatus::Unresolved => (
                ResultMetadata::unresolved(Exactness::Symbolic, OutcomeReason::AlgorithmUncovered),
                true,
                false,
            ),
            DefinedObjectStatus::Divergent => (
                ResultMetadata::no_result(Exactness::Symbolic, OutcomeReason::Divergent),
                false,
                true,
            ),
        };
        let source = if no_value {
            result.source.clone()
        } else {
            result.value.clone()
        };
        let operator = match kind {
            DefinedIntegralOperationKind::Improper => "ImproperIntegral",
            DefinedIntegralOperationKind::PrincipalValue => "PrincipalValueIntegral",
        };
        let mut semantics = SemanticState {
            kind: if held || no_value {
                ValueKind::Unevaluated
            } else {
                crate::input::with_parse_env(|env| {
                    let parsed = yacas_rs::parser::parse_expression(env, &format!("{source};"))
                        .expect("defined integral output parses")
                        .expect("defined integral output exists");
                    crate::semantic::analyze_tree(env, &parsed).semantic.kind
                })
            },
            interpretation: if held {
                SemanticInterpretation::HeldApplication {
                    operator: operator.into(),
                }
            } else {
                SemanticInterpretation::PlainExpression
            },
            metadata,
            capabilities: if no_value {
                CapabilitySet::empty()
            } else {
                CapabilitySet::symbolic_expression()
            },
            requirements: Vec::new(),
        };
        let parsed = crate::semantic_core::parse_engine_expression(&source)?;
        if held {
            crate::semantic_core::promote_held_application(
                operator,
                &parsed.raw_expression(),
                &mut semantics,
            )?;
        }
        let mut output = input.clone();
        output.apply(ObjectDelta {
            expression: Some(parsed.raw_expression()),
            semantics: Some(semantics),
            overlay: None,
            normalization: (!held && !no_value).then_some(NormalizationMetadata {
                level: NormalizationLevel::Domain,
                assumptions: result.conditions.conditions().to_vec(),
                mode: NormalizationMode::Operation(match kind {
                    DefinedIntegralOperationKind::Improper => OperatorId::ImproperIntegral,
                    DefinedIntegralOperationKind::PrincipalValue => {
                        OperatorId::PrincipalValueIntegral
                    }
                }),
            }),
        });
        let events = result
            .steps
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
                conditions: result.conditions.conditions().to_vec(),
                payload: RulePayload::Structural,
                importance: match step.importance {
                    StepImportance::Routine => RuleImportance::Routine,
                    StepImportance::Normal => RuleImportance::Normal,
                    StepImportance::Key => RuleImportance::Key,
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
            output: if no_value {
                ComputationOutput::NoValue(output)
            } else if held {
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
pub struct ImproperIntegralRequest {
    pub expression: String,
    pub variable: String,
    pub lower: String,
    pub upper: String,
    /// Ordered singular points strictly inside the interval. Finite endpoint
    /// singularities are represented by repeating the endpoint here.
    pub singular_points: Vec<String>,
}

#[derive(Clone)]
struct Boundary {
    value: String,
    improper: bool,
}

#[derive(Clone)]
struct Segment {
    lower: Boundary,
    upper: Boundary,
}

pub fn evaluate(
    engine: &mut dyn Engine,
    request: &ImproperIntegralRequest,
    steps: Option<StepVerbosity>,
) -> Result<DefinedObjectResult, EngineError> {
    validate_request(request)?;
    let segments = reduction_segments(request)?;
    execute_segments(
        engine,
        request,
        segments,
        DefinedObjectKind::ImproperIntegral,
        steps,
    )
}

/// Explicit Cauchy principal value. It is never selected as a fallback for
/// an ordinary improper integral.
pub fn principal_value(
    engine: &mut dyn Engine,
    request: &ImproperIntegralRequest,
    steps: Option<StepVerbosity>,
) -> Result<DefinedObjectResult, EngineError> {
    validate_request(request)?;
    let source = source_expression("PrincipalValueIntegral", request);
    let cutoff = fresh_symbol(request, "PrincipalValueRadius");
    let command = if request.lower == "-Infinity"
        && request.upper == "Infinity"
        && request.singular_points.is_empty()
    {
        format!(
            "Integrate({},-{},{} )({})",
            request.variable, cutoff, cutoff, request.expression
        )
    } else if request.singular_points.len() == 1
        && request.lower != "-Infinity"
        && request.upper != "Infinity"
    {
        let point = &request.singular_points[0];
        let lower = bound_value(&request.lower)?;
        let upper = bound_value(&request.upper)?;
        let center = bound_value(point)?;
        let radius = (center - lower).min(upper - center);
        let dummy = fresh_symbol(request, "PrincipalValueVariable");
        let left = binding::substitute_free(
            &request.expression,
            &request.variable,
            &format!("({point})-{dummy}"),
        )?;
        let right = binding::substitute_free(
            &request.expression,
            &request.variable,
            &format!("({point})+{dummy}"),
        )?;
        let mut terms = vec![format!(
            "Integrate({dummy},{cutoff},{})(({left})+({right}))",
            compact_number(radius)
        )];
        if center - lower > radius {
            terms.push(finite_integral(
                request,
                &request.lower,
                &compact_number(center - radius),
            ));
        }
        if upper - center > radius {
            terms.push(finite_integral(
                request,
                &compact_number(center + radius),
                &request.upper,
            ));
        }
        terms
            .into_iter()
            .map(|term| format!("({term})"))
            .collect::<Vec<_>>()
            .join("+")
    } else {
        return Err(EngineError::InvalidInput(
            "主值当前支持双无穷对称截断，或一个显式有限内部奇点".into(),
        ));
    };
    let (truncated, limit) = assumptions::with_assumptions(
        engine,
        &[AssumptionSpec {
            symbol: cutoff.clone(),
            fact: AssumptionFact::Positive,
        }],
        |engine| {
            let truncated = engine.eval_expr(&command)?;
            let limit = limits::limit(
                engine,
                &truncated.to_string(),
                &cutoff,
                if request.lower == "-Infinity" {
                    "Infinity"
                } else {
                    "0"
                },
                LimitDirection::Right,
            )?;
            Ok((truncated, limit))
        },
    )?;
    let truncated_value = truncated.to_string();
    let (status, component_status) = limit_status(limit.status);
    let conditions = conditions_from_limits(&limit.conditions)?;
    let mut components = vec![ObjectComponent {
        branch: 0,
        operation: PrimitiveOperation::FiniteIntegral,
        input: command,
        value: truncated_value.clone(),
        status: if held_integral(&truncated) {
            ComponentStatus::Unresolved
        } else {
            ComponentStatus::Completed
        },
    }];
    components.push(ObjectComponent {
        branch: 0,
        operation: PrimitiveOperation::SymmetricLimit,
        input: format!("Limit({cutoff})({truncated_value})"),
        value: limit.value.clone(),
        status: component_status,
    });
    let status = if held_integral(&truncated) {
        DefinedObjectStatus::Unresolved
    } else {
        status
    };
    finish_result(
        engine,
        DefinedObjectResult {
            kind: DefinedObjectKind::CauchyPrincipalValue,
            status,
            source,
            value: limit.value,
            tex: String::new(),
            conditions,
            components,
            steps: Vec::new(),
        },
        steps,
    )
}

fn execute_segments(
    engine: &mut dyn Engine,
    request: &ImproperIntegralRequest,
    segments: Vec<Segment>,
    kind: DefinedObjectKind,
    steps: Option<StepVerbosity>,
) -> Result<DefinedObjectResult, EngineError> {
    let source = source_expression("ImproperIntegral", request);
    let mut components = Vec::new();
    let mut values = Vec::new();
    let mut conditions = Vec::new();
    let mut status = DefinedObjectStatus::Converged;
    for (branch, segment) in segments.iter().enumerate() {
        let (value, branch_status, mut branch_conditions) =
            execute_segment(engine, request, segment, branch, &mut components)?;
        values.push(value);
        conditions.append(&mut branch_conditions);
        status = combine_status(status, branch_status);
    }

    // Divergent and unresolved branches are never algebraically combined.
    // This prevents false cancellation such as the two sides of 1/x.
    let value = if status == DefinedObjectStatus::Converged {
        let expression = values
            .iter()
            .map(|value| format!("({value})"))
            .collect::<Vec<_>>()
            .join("+");
        let assembled = engine.eval_expr(&format!("Simplify({expression})"))?;
        let value = assembled.to_string();
        components.push(ObjectComponent {
            branch: segments.len(),
            operation: PrimitiveOperation::Assemble,
            input: expression,
            value: value.clone(),
            status: ComponentStatus::Completed,
        });
        value
    } else {
        source.clone()
    };
    finish_result(
        engine,
        DefinedObjectResult {
            kind,
            status,
            source,
            value,
            tex: String::new(),
            conditions: ConditionSet::new(conditions)?,
            components,
            steps: Vec::new(),
        },
        steps,
    )
}

fn execute_segment(
    engine: &mut dyn Engine,
    request: &ImproperIntegralRequest,
    segment: &Segment,
    branch: usize,
    components: &mut Vec<ObjectComponent>,
) -> Result<(String, DefinedObjectStatus, Vec<Condition>), EngineError> {
    if !segment.lower.improper && !segment.upper.improper {
        let command = finite_integral(request, &segment.lower.value, &segment.upper.value);
        let result = engine.eval_expr(&command)?;
        let value = result.to_string();
        let unresolved = held_integral(&result);
        components.push(ObjectComponent {
            branch,
            operation: PrimitiveOperation::FiniteIntegral,
            input: command,
            value: value.clone(),
            status: if unresolved {
                ComponentStatus::Unresolved
            } else {
                ComponentStatus::Completed
            },
        });
        return Ok((
            value,
            if unresolved {
                DefinedObjectStatus::Unresolved
            } else {
                DefinedObjectStatus::Converged
            },
            Vec::new(),
        ));
    }
    if segment.lower.improper && segment.upper.improper {
        return Err(EngineError::InvalidInput(
            "内部规划错误：单个截断分支不能同时含两个反常端点".into(),
        ));
    }

    let cutoff = fresh_symbol(request, &format!("ImproperCutoff{branch}"));
    let (command, at, direction) = if segment.lower.improper {
        (
            finite_integral(request, &cutoff, &segment.upper.value),
            segment.lower.value.as_str(),
            direction_for_lower(&segment.lower.value),
        )
    } else {
        (
            finite_integral(request, &segment.lower.value, &cutoff),
            segment.upper.value.as_str(),
            direction_for_upper(&segment.upper.value),
        )
    };
    let truncated = engine.eval_expr(&command)?;
    let truncated_value = truncated.to_string();
    let unresolved_integral = held_integral(&truncated);
    components.push(ObjectComponent {
        branch,
        operation: PrimitiveOperation::FiniteIntegral,
        input: command,
        value: truncated_value.clone(),
        status: if unresolved_integral {
            ComponentStatus::Unresolved
        } else {
            ComponentStatus::Completed
        },
    });
    if unresolved_integral {
        return Ok((truncated_value, DefinedObjectStatus::Unresolved, Vec::new()));
    }
    let limit = limits::limit(engine, &truncated_value, &cutoff, at, direction)?;
    let (status, component_status) = limit_status(limit.status);
    components.push(ObjectComponent {
        branch,
        operation: PrimitiveOperation::OneSidedLimit,
        input: format!("Limit({cutoff},{at})({truncated_value})"),
        value: limit.value.clone(),
        status: component_status,
    });
    Ok((
        limit.value,
        status,
        conditions_from_limits(&limit.conditions)?
            .conditions()
            .to_vec(),
    ))
}

fn reduction_segments(request: &ImproperIntegralRequest) -> Result<Vec<Segment>, EngineError> {
    let lower = bound_value(&request.lower)?;
    let upper = bound_value(&request.upper)?;
    if lower >= upper {
        return Err(EngineError::InvalidInput("积分上下限必须严格递增".into()));
    }
    let mut points = request
        .singular_points
        .iter()
        .map(|point| {
            point
                .parse::<f64>()
                .map_err(|_| EngineError::InvalidInput("显式奇点当前必须是有限实数字面量".into()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if points.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(EngineError::InvalidInput(
            "显式奇点必须严格递增且不重复".into(),
        ));
    }
    if points.iter().any(|point| *point < lower || *point > upper) {
        return Err(EngineError::InvalidInput("显式奇点位于积分区间外".into()));
    }

    let mut boundaries = vec![Boundary {
        value: request.lower.clone(),
        improper: request.lower.contains("Infinity")
            || points.first().is_some_and(|point| *point == lower),
    }];
    for point in points.drain(..) {
        if point != lower && point != upper {
            boundaries.push(Boundary {
                value: compact_number(point),
                improper: true,
            });
        }
    }
    boundaries.push(Boundary {
        value: request.upper.clone(),
        improper: request.upper.contains("Infinity")
            || request
                .singular_points
                .last()
                .and_then(|point| point.parse::<f64>().ok())
                .is_some_and(|point| point == upper),
    });

    let mut segments = Vec::new();
    for pair in boundaries.windows(2) {
        if pair[0].improper && pair[1].improper {
            let left = bound_value(&pair[0].value)?;
            let right = bound_value(&pair[1].value)?;
            let anchor = if left.is_infinite() && right.is_infinite() {
                0.0
            } else if left.is_infinite() {
                right - 1.0
            } else if right.is_infinite() {
                left + 1.0
            } else {
                (left + right) / 2.0
            };
            let anchor = Boundary {
                value: compact_number(anchor),
                improper: false,
            };
            segments.push(Segment {
                lower: pair[0].clone(),
                upper: anchor.clone(),
            });
            segments.push(Segment {
                lower: anchor,
                upper: pair[1].clone(),
            });
        } else {
            segments.push(Segment {
                lower: pair[0].clone(),
                upper: pair[1].clone(),
            });
        }
    }
    Ok(segments)
}

fn validate_request(request: &ImproperIntegralRequest) -> Result<(), EngineError> {
    validate_expression(&request.expression, "反常积分表达式")?;
    validate_symbol(&request.variable, "积分变量")?;
    validate_expression(&request.lower, "积分下限")?;
    validate_expression(&request.upper, "积分上限")?;
    if request.singular_points.len() > MAX_SINGULAR_POINTS {
        return Err(EngineError::InvalidInput(format!(
            "显式奇点超过上限 {MAX_SINGULAR_POINTS}"
        )));
    }
    for point in &request.singular_points {
        validate_expression(point, "积分奇点")?;
    }
    Ok(())
}

fn finish_result(
    engine: &mut dyn Engine,
    mut result: DefinedObjectResult,
    verbosity: Option<StepVerbosity>,
) -> Result<DefinedObjectResult, EngineError> {
    if result.status == DefinedObjectStatus::Converged {
        result.tex = strip_tex_delimiters(&engine.eval(&result.value)?.tex);
    }
    result.steps = if let Some(verbosity) = verbosity {
        let events = result
            .components
            .iter()
            .map(|component| {
                let (rule, why) = match component.operation {
                    PrimitiveOperation::FiniteIntegral => {
                        ("object-finite-integral", "先计算该分支的截断定积分。")
                    }
                    PrimitiveOperation::OneSidedLimit => (
                        "object-one-sided-limit",
                        "分别判断该反常端点对应的单侧极限。",
                    ),
                    PrimitiveOperation::SymmetricLimit => {
                        ("object-symmetric-limit", "按主值定义计算对称截断极限。")
                    }
                    PrimitiveOperation::Assemble => {
                        ("object-assemble", "所有分支均收敛后再合并结果。")
                    }
                };
                StepEvent::new(rule, &component.value, why, StepImportance::Normal)
            })
            .collect();
        render_events(engine, events, verbosity)?
    } else {
        Vec::new()
    };
    Ok(result)
}

fn source_expression(head: &str, request: &ImproperIntegralRequest) -> String {
    let points = if request.singular_points.is_empty() {
        String::new()
    } else {
        format!(",{{{}}}", request.singular_points.join(","))
    };
    format!(
        "{head}(({}),{},{},{}{})",
        request.expression, request.variable, request.lower, request.upper, points
    )
}

fn finite_integral(request: &ImproperIntegralRequest, lower: &str, upper: &str) -> String {
    format!(
        "Integrate({},{lower},{upper})({})",
        request.variable, request.expression
    )
}

fn fresh_symbol(request: &ImproperIntegralRequest, namespace: &str) -> String {
    let mut inputs = vec![
        request.expression.as_str(),
        request.variable.as_str(),
        request.lower.as_str(),
        request.upper.as_str(),
    ];
    inputs.extend(request.singular_points.iter().map(String::as_str));
    fresh_internal_symbols(namespace, &inputs, ["Value"])[0].clone()
}

fn bound_value(value: &str) -> Result<f64, EngineError> {
    match value.trim() {
        "Infinity" => Ok(f64::INFINITY),
        "-Infinity" => Ok(f64::NEG_INFINITY),
        value => {
            let parsed = value.parse::<f64>().map_err(|_| {
                EngineError::InvalidInput("当前反常积分边界必须是实数字面量或 Infinity".into())
            })?;
            if parsed.is_finite() {
                Ok(parsed)
            } else {
                Err(EngineError::InvalidInput(
                    "有限边界不能是 NaN 或非标准无穷值".into(),
                ))
            }
        }
    }
}

fn compact_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn direction_for_lower(value: &str) -> LimitDirection {
    if value == "-Infinity" {
        LimitDirection::Both
    } else {
        LimitDirection::Right
    }
}

fn direction_for_upper(value: &str) -> LimitDirection {
    if value == "Infinity" {
        LimitDirection::Both
    } else {
        LimitDirection::Left
    }
}

fn held_integral(expression: &Expr) -> bool {
    matches!(expression, Expr::Call { head, .. } if head == "Integrate")
        || expression.to_string().contains("Integrate(")
}

fn limit_status(status: LimitStatus) -> (DefinedObjectStatus, ComponentStatus) {
    match status {
        LimitStatus::Converged => (DefinedObjectStatus::Converged, ComponentStatus::Completed),
        LimitStatus::Unresolved => (DefinedObjectStatus::Unresolved, ComponentStatus::Unresolved),
        LimitStatus::PositiveInfinity
        | LimitStatus::NegativeInfinity
        | LimitStatus::DoesNotExist => (DefinedObjectStatus::Divergent, ComponentStatus::Divergent),
    }
}

fn combine_status(left: DefinedObjectStatus, right: DefinedObjectStatus) -> DefinedObjectStatus {
    if left == DefinedObjectStatus::Divergent || right == DefinedObjectStatus::Divergent {
        DefinedObjectStatus::Divergent
    } else if left == DefinedObjectStatus::Unresolved || right == DefinedObjectStatus::Unresolved {
        DefinedObjectStatus::Unresolved
    } else {
        DefinedObjectStatus::Converged
    }
}

fn conditions_from_limits(conditions: &[LimitCondition]) -> Result<ConditionSet, EngineError> {
    let mut output = Vec::new();
    for condition in conditions {
        convert_condition(condition, &mut output);
    }
    ConditionSet::new(output)
}

fn convert_condition(condition: &LimitCondition, output: &mut Vec<Condition>) {
    match condition {
        LimitCondition::Property { expression, fact } => output.push(match fact.as_str() {
            "Positive" => Condition::Positive {
                expression: expression.clone(),
            },
            "Negative" => Condition::Negative {
                expression: expression.clone(),
            },
            "NonZero" => Condition::NonZero {
                expression: expression.clone(),
            },
            "Real" => Condition::Real {
                expression: expression.clone(),
            },
            "Integer" => Condition::Integer {
                expression: expression.clone(),
            },
            _ => Condition::Unknown {
                description: format!("{expression} is {fact}"),
            },
        }),
        LimitCondition::Relation {
            left,
            relation: Relation::GreaterThan,
            right,
        } if right == "0" => {
            if let Some(expression) = left
                .strip_prefix("Re(")
                .and_then(|left| left.strip_suffix(')'))
            {
                output.push(Condition::RealPartPositive {
                    expression: expression.into(),
                });
            } else {
                output.push(Condition::Positive {
                    expression: left.clone(),
                });
            }
        }
        LimitCondition::All { conditions } => {
            for condition in conditions {
                convert_condition(condition, output);
            }
        }
        other => output.push(Condition::Unknown {
            description: format!("{other:?}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;

    fn object(source: &str) -> crate::semantic_core::MathematicalObject {
        object_from_source(
            crate::semantic_core::ObjectId(73),
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
    fn defined_integral_operation_preserves_value_absence_and_identity() {
        let mut engine = RustEngine::spawn().unwrap();
        let converged = DefinedIntegralOperation
            .compute(
                &mut engine,
                &object("Exp(-x)"),
                &(
                    DefinedIntegralOperationKind::Improper,
                    request("", "0", "Infinity", &[]),
                ),
            )
            .unwrap();
        assert!(matches!(converged.output, ComputationOutput::Value(_)));
        assert_eq!(converged.subject().unwrap().print_source(), "1");
        assert_eq!(
            converged.subject().unwrap().id,
            crate::semantic_core::ObjectId(73)
        );

        let divergent = DefinedIntegralOperation
            .compute(
                &mut engine,
                &object("1/x"),
                &(
                    DefinedIntegralOperationKind::Improper,
                    request("", "-1", "1", &["0"]),
                ),
            )
            .unwrap();
        assert!(matches!(divergent.output, ComputationOutput::NoValue(_)));

        let principal = DefinedIntegralOperation
            .compute(
                &mut engine,
                &object("1/x"),
                &(
                    DefinedIntegralOperationKind::PrincipalValue,
                    request("", "-1", "1", &["0"]),
                ),
            )
            .unwrap();
        assert!(matches!(principal.output, ComputationOutput::Value(_)));
        assert_eq!(principal.subject().unwrap().print_source(), "0");
    }

    fn request(
        expression: &str,
        lower: &str,
        upper: &str,
        points: &[&str],
    ) -> ImproperIntegralRequest {
        ImproperIntegralRequest {
            expression: expression.into(),
            variable: "x".into(),
            lower: lower.into(),
            upper: upper.into(),
            singular_points: points.iter().map(|point| (*point).into()).collect(),
        }
    }

    #[test]
    fn internal_symbols_avoid_every_request_fragment() {
        let request = request(
            "YaCluePrincipalValueRadiusInternal0Value*Exp(-x)",
            "0",
            "Infinity",
            &["YaCluePrincipalValueRadiusInternal1Value"],
        );
        assert_eq!(
            fresh_symbol(&request, "PrincipalValueRadius"),
            "YaCluePrincipalValueRadiusInternal2Value"
        );
    }

    #[test]
    fn converges_at_single_and_double_infinite_endpoints() {
        let mut engine = RustEngine::spawn().unwrap();
        let single =
            evaluate(&mut engine, &request("Exp(-x)", "0", "Infinity", &[]), None).unwrap();
        assert_eq!(single.status, DefinedObjectStatus::Converged);
        assert_eq!(single.value, "1");

        let double = evaluate(
            &mut engine,
            &request("1/(1+x^2)", "-Infinity", "Infinity", &[]),
            None,
        )
        .unwrap();
        assert_eq!(double.status, DefinedObjectStatus::Converged);
        assert_eq!(
            engine
                .eval(&format!("Simplify(({})-Pi)", double.value))
                .unwrap()
                .expr
                .to_string(),
            "0"
        );
    }

    #[test]
    fn endpoint_and_internal_singularities_are_independent_branches() {
        let mut engine = RustEngine::spawn().unwrap();
        let endpoint =
            evaluate(&mut engine, &request("1/Sqrt(x)", "0", "1", &["0"]), None).unwrap();
        assert_eq!(endpoint.status, DefinedObjectStatus::Converged);
        assert_eq!(endpoint.value, "2");

        let ordinary = evaluate(&mut engine, &request("1/x", "-1", "1", &["0"]), None).unwrap();
        assert_eq!(ordinary.status, DefinedObjectStatus::Divergent);
        assert!(!ordinary
            .components
            .iter()
            .any(|component| component.operation == PrimitiveOperation::Assemble));
    }

    #[test]
    fn principal_value_is_an_explicit_distinct_object() {
        let mut engine = RustEngine::spawn().unwrap();
        let value = principal_value(
            &mut engine,
            &request("1/x", "-1", "1", &["0"]),
            Some(StepVerbosity::Standard),
        )
        .unwrap();
        assert_eq!(value.kind, DefinedObjectKind::CauchyPrincipalValue);
        assert_eq!(value.status, DefinedObjectStatus::Converged, "{value:#?}");
        assert_eq!(value.value, "0");
        assert!(!value.steps.is_empty());
    }

    #[test]
    fn invalid_or_excessive_singularities_are_bounded() {
        let mut engine = RustEngine::spawn().unwrap();
        assert!(evaluate(&mut engine, &request("1/x", "-1", "1", &["a"]), None).is_err());
        let points = (0..=MAX_SINGULAR_POINTS)
            .map(|index| index.to_string())
            .collect::<Vec<_>>();
        let request = ImproperIntegralRequest {
            expression: "1/x".into(),
            variable: "x".into(),
            lower: "-1".into(),
            upper: "20".into(),
            singular_points: points,
        };
        assert!(evaluate(&mut engine, &request, None).is_err());
    }
}
