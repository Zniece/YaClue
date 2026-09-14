//! Object-native indefinite integration. The engine remains the temporary
//! domain adapter, while the operation boundary owns Held/family semantics.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{validate_expression, validate_symbol};
use crate::protocol::{ConditionSet, OutcomeReason, ResultMetadata};
use crate::quadrature::{adaptive_simpson, QuadratureOptions};
use crate::semantic::{Exactness, ValueKind};
use crate::semantic_core::{
    object_from_source, CapabilitySet, Computation, ComputationOutput, EventSink,
    NormalizationLevel, NormalizationMetadata, NormalizationMode, ObjectDelta, ObjectId,
    OperatorId, Requirement, RuleFact, RuleImportance, RulePayload, RulePresentation, RuleTrace,
    SemanticInterpretation, SemanticOperation, SemanticState, VecEventSink,
};
use crate::steps::{
    render_events, render_rule_trace, Step, StepEvent, StepImportance, StepKind, StepVerbosity,
};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AntiderivativeFamily {
    pub representative: String,
    pub representative_tex: String,
    pub expression: String,
    pub tex: String,
    pub variable: String,
    pub arbitrary_constants: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AntiderivativeStepResult {
    pub result: AntiderivativeFamily,
    pub steps: Vec<Step>,
}

struct IntegralRuleEmission {
    rule: String,
    expression: String,
    explanation: String,
    importance: RuleImportance,
}

struct IntegralEvaluation {
    result: String,
    emissions: Vec<IntegralRuleEmission>,
}

pub fn derive_integrals_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    Ok(integral_compatibility_evaluation(engine, expression, variable, verbosity)?.1)
}

fn integral_compatibility_evaluation(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    verbosity: StepVerbosity,
) -> Result<(IntegralEvaluation, Vec<Step>), EngineError> {
    validate_expression(expression, "表达式")?;
    validate_symbol(variable, "积分变量")?;
    let evaluation = evaluate_integral_rules(
        engine,
        &format!("StepsI'Computation({expression},{variable})"),
    )?;
    let events = evaluation
        .emissions
        .iter()
        .map(|emission| {
            StepEvent::new(
                &emission.rule,
                &emission.expression,
                &emission.explanation,
                match emission.importance {
                    RuleImportance::Routine => StepImportance::Routine,
                    RuleImportance::Normal => StepImportance::Normal,
                    RuleImportance::Key => StepImportance::Key,
                },
            )
        })
        .collect();
    let steps = render_events(engine, events, verbosity)?;
    Ok((evaluation, steps))
}

pub fn derive_integrals(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
) -> Result<Vec<Step>, EngineError> {
    derive_integrals_with_verbosity(engine, expression, variable, StepVerbosity::Detailed)
}

pub fn antiderivative_family(
    representative: String,
    representative_tex: String,
    variable: &str,
    arbitrary_constant: String,
) -> AntiderivativeFamily {
    let constant_tex = arbitrary_constant
        .strip_prefix('C')
        .filter(|suffix| !suffix.is_empty())
        .map_or_else(
            || r"\mathrm{C}".to_string(),
            |suffix| format!(r"\mathrm{{C}}_{{{suffix}}}"),
        );
    AntiderivativeFamily {
        expression: format!("({representative} + {arbitrary_constant})"),
        tex: format!("{representative_tex} + {constant_tex}"),
        representative,
        representative_tex,
        variable: variable.into(),
        arbitrary_constants: vec![arbitrary_constant],
    }
}

pub fn derive_antiderivative_family_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    arbitrary_constant: String,
    verbosity: StepVerbosity,
) -> Result<AntiderivativeStepResult, EngineError> {
    let (evaluation, mut steps) =
        integral_compatibility_evaluation(engine, expression, variable, verbosity)?;
    let representative = evaluation.result;
    let representative_tex = engine
        .render_syntax_tex_batch(std::slice::from_ref(&representative))?
        .pop()
        .map(|tex| crate::input::strip_tex_delimiters(&tex))
        .ok_or_else(|| EngineError::Parse("不定积分代表元缺少 TeX 投影".into()))?;
    let result = antiderivative_family(
        representative,
        representative_tex,
        variable,
        arbitrary_constant,
    );
    steps.push(Step {
        kind: StepKind::EquivalentTransformation,
        before_expr: None,
        before_tex: None,
        rule: "antiderivative-family".into(),
        expr: result.expression.clone(),
        why: "加入任意常数，表示全部原函数。".into(),
        tex: result.tex.clone(),
        importance: StepImportance::Key,
    });
    Ok(AntiderivativeStepResult { result, steps })
}

fn evaluate_integral_rules(
    engine: &mut dyn Engine,
    command: &str,
) -> Result<IntegralEvaluation, EngineError> {
    let Expr::Call { head, args } = engine.eval_expr(command)? else {
        return Err(EngineError::Parse("积分规则执行结果不是列表".into()));
    };
    if head != "List" || args.len() != 2 {
        return Err(EngineError::Parse(
            "积分规则执行结果必须包含结果和规则发射".into(),
        ));
    }
    let result = args[0].to_string();
    let Expr::Call {
        head: emission_head,
        args: emission_args,
    } = &args[1]
    else {
        return Err(EngineError::Parse("积分规则发射不是列表".into()));
    };
    if emission_head != "List" || emission_args.is_empty() {
        return Err(EngineError::Eval("未能生成积分规则事实".into()));
    }
    let emissions = emission_args
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let Expr::Call { head, args: fields } = item else {
                return Err(EngineError::Parse(format!("积分规则发射 {index} 不是列表")));
            };
            if head != "List" || fields.len() != 4 {
                return Err(EngineError::Parse(format!(
                    "积分规则发射 {index} 必须是四字段 List"
                )));
            }
            let text = |field: &Expr, name| match field {
                Expr::Symbol(value)
                    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') =>
                {
                    Ok(value[1..value.len() - 1].to_owned())
                }
                _ => Err(EngineError::Parse(format!(
                    "积分规则发射 {index} 的 {name} 必须是字符串"
                ))),
            };
            let importance = match &fields[3] {
                Expr::Number(value) if value == "0" => RuleImportance::Routine,
                Expr::Number(value) if value == "1" => RuleImportance::Normal,
                Expr::Number(value) if value == "2" => RuleImportance::Key,
                _ => {
                    return Err(EngineError::Parse(format!(
                        "积分规则发射 {index} 的 importance 必须是 0、1 或 2"
                    )))
                }
            };
            Ok(IntegralRuleEmission {
                rule: text(&fields[0], "rule")?,
                expression: fields[1].to_string(),
                explanation: text(&fields[2], "explanation")?,
                importance,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(IntegralEvaluation { result, emissions })
}

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
        let evaluation = evaluate_integral_rules(
            engine,
            &format!("StepsI'Computation({source},{})", request.variable),
        )?;
        let representative = evaluation.result;
        let representative_ast =
            crate::semantic_core::parse_engine_expression(&representative)?.raw_expression();
        let unresolved = crate::input::with_parse_env(|env| {
            matches!(
                crate::semantic_core::ExpressionView::new(env, &representative_ast).head(),
                Some("Integrate" | "Int")
            )
        });
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
        let input_ref = input.reference(None);
        let output_ref = output.reference(None);
        let variable_binding = vec![("variable".into(), request.variable.clone())];
        let mut sink = VecEventSink::default();
        let mut transition_expressions = evaluation
            .emissions
            .iter()
            .map(|emission| emission.expression.clone())
            .collect::<Vec<_>>();
        evaluation.emissions.into_iter().for_each(|emission| {
            let fact = RuleFact {
                class: crate::semantic_core::RuleEventClass::EquivalentTransformation,
                rule: emission.rule,
                input: input_ref.clone(),
                additional_inputs: Vec::new(),
                output: output_ref.clone(),
                bindings: variable_binding.clone(),
                conditions: Vec::new(),
                payload: RulePayload::Structural,
                importance: emission.importance,
                transformation: None,
            };
            sink.record_fact(fact, || {
                Some(RulePresentation {
                    expression: emission.expression,
                    explanation: emission.explanation,
                    tex_override: None,
                })
            });
        });
        let family_fact = RuleFact {
            class: crate::semantic_core::RuleEventClass::EquivalentTransformation,
            rule: if unresolved {
                "hold-integral".into()
            } else {
                "antiderivative-family".into()
            },
            input: input_ref,
            additional_inputs: Vec::new(),
            output: output_ref,
            bindings: if unresolved {
                variable_binding
            } else {
                vec![
                    ("variable".into(), request.variable.clone()),
                    ("constant".into(), request.arbitrary_constant.clone()),
                ]
            },
            conditions: Vec::new(),
            payload: RulePayload::Rewrite,
            importance: RuleImportance::Key,
            transformation: None,
        };
        sink.record_fact(family_fact, || {
            Some(RulePresentation {
                expression: output.print_source(),
                explanation: if unresolved {
                    "保留尚无闭式结果的积分对象。".into()
                } else {
                    "加入任意常数，表示全部原函数。".into()
                },
                tex_override: None,
            })
        });
        transition_expressions.push(output.print_source());
        let (output, events) = crate::semantic_core::materialize_rule_transitions(
            input,
            output,
            sink.events,
            &transition_expressions,
        )?;
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
        let evaluation = evaluate_integral_rules(
            engine,
            &format!(
                "StepsI'Def'Computation({source},{},{},{})",
                request.variable, request.lower, request.upper
            ),
        )?;
        let representative = evaluation.result;
        let parsed = crate::semantic_core::parse_engine_expression(&representative)?;
        let output_ast = parsed.raw_expression();
        let unresolved = crate::input::with_parse_env(|env| {
            crate::semantic_core::ExpressionView::new(env, &output_ast).head() == Some("Integrate")
        });
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
                    crate::semantic::analyze_tree(env, &output_ast)
                        .semantic
                        .kind
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
        if unresolved {
            crate::semantic_core::promote_held_application(
                "Integrate",
                &output_ast,
                &mut semantics,
            )?;
        }
        let mut output = input.clone();
        output.apply(ObjectDelta {
            expression: Some(output_ast),
            semantics: Some(semantics),
            overlay: None,
            normalization: (!unresolved).then_some(NormalizationMetadata {
                level: NormalizationLevel::Domain,
                assumptions: Vec::new(),
                mode: NormalizationMode::Operation(OperatorId::Integral),
            }),
        });
        let input_ref = input.reference(None);
        let output_ref = output.reference(None);
        let bindings = vec![
            ("variable".into(), request.variable.clone()),
            ("lower".into(), request.lower.clone()),
            ("upper".into(), request.upper.clone()),
        ];
        let last = evaluation.emissions.len() - 1;
        let mut sink = VecEventSink::default();
        let transition_expressions = evaluation
            .emissions
            .iter()
            .map(|emission| emission.expression.clone())
            .collect::<Vec<_>>();
        evaluation
            .emissions
            .into_iter()
            .enumerate()
            .for_each(|(index, emission)| {
                let fact = RuleFact {
                    class: crate::semantic_core::RuleEventClass::EquivalentTransformation,
                    rule: emission.rule,
                    input: input_ref.clone(),
                    additional_inputs: Vec::new(),
                    output: output_ref.clone(),
                    bindings: bindings.clone(),
                    conditions: Vec::new(),
                    payload: if index == last {
                        RulePayload::Rewrite
                    } else {
                        RulePayload::Structural
                    },
                    importance: if index == last {
                        RuleImportance::Key
                    } else {
                        emission.importance
                    },
                    transformation: None,
                };
                sink.record_fact(fact, || {
                    Some(RulePresentation {
                        expression: emission.expression,
                        explanation: emission.explanation,
                        tex_override: None,
                    })
                });
            });
        let (output, events) = crate::semantic_core::materialize_rule_transitions(
            input,
            output,
            sink.events,
            &transition_expressions,
        )?;
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

/// Compatibility entry for callers that explicitly request bounded numeric
/// degradation. The exact/held decision and the numeric rule fact both come
/// from the semantic computation; product steps are only a later projection.
pub(crate) fn definite_integral_computation_with_options(
    engine: &mut dyn Engine,
    expression: &str,
    request: &DefiniteIntegralRequest,
    options: Option<&QuadratureOptions>,
) -> Result<Computation, EngineError> {
    let input = object_from_source(
        ObjectId(1),
        expression,
        SemanticState {
            kind: ValueKind::Expression,
            interpretation: SemanticInterpretation::PlainExpression,
            metadata: ResultMetadata::solved(Exactness::Unknown, ConditionSet::empty()),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        },
    )?;
    let computation = DefiniteIntegralOperation.compute(engine, &input, request)?;
    let Some(options) = options else {
        return Ok(computation);
    };
    let Computation {
        output: ComputationOutput::Held(mut output),
        trace,
        certificates,
        effects,
    } = computation
    else {
        return Ok(computation);
    };
    let lower = numeric_bound(engine, &request.lower, "下限")?;
    let upper = numeric_bound(engine, &request.upper, "上限")?;
    let result = adaptive_simpson(
        engine,
        expression,
        &request.variable,
        (lower, upper),
        options,
    )?;
    let value = format_numeric_result(result.value);
    let parsed = crate::semantic_core::parse_engine_expression(&value)?;
    let numeric_input_ref = output.reference(None);
    output.apply(ObjectDelta {
        expression: Some(parsed.raw_expression()),
        semantics: Some(SemanticState {
            kind: ValueKind::Scalar,
            interpretation: SemanticInterpretation::PlainExpression,
            metadata: ResultMetadata::solved(Exactness::Approximate, ConditionSet::empty()),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        }),
        overlay: None,
        normalization: Some(NormalizationMetadata {
            level: NormalizationLevel::Domain,
            assumptions: Vec::new(),
            mode: NormalizationMode::Operation(OperatorId::Integral),
        }),
    });
    let fact = RuleFact {
        class: crate::semantic_core::RuleEventClass::EquivalentTransformation,
        rule: "numeric-integration-rule".into(),
        input: numeric_input_ref,
        additional_inputs: Vec::new(),
        output: output.reference(None),
        bindings: vec![
            ("variable".into(), request.variable.clone()),
            ("lower".into(), request.lower.clone()),
            ("upper".into(), request.upper.clone()),
            ("estimated_error".into(), result.estimated_error.to_string()),
            ("evaluations".into(), result.evaluations.to_string()),
        ],
        conditions: Vec::new(),
        payload: RulePayload::Rewrite,
        importance: RuleImportance::Key,
        transformation: None,
    };
    let mut sink = VecEventSink {
        events: trace.map(|trace| trace.events).unwrap_or_default(),
    };
    sink.record_fact(fact, || {
        Some(RulePresentation {
            expression: value,
            explanation: format!(
                "自适应辛普森数值积分（估计误差 {:.2e}，{} 次采样）",
                result.estimated_error, result.evaluations
            ),
            tex_override: None,
        })
    });
    let events = sink.events;
    Ok(Computation {
        output: ComputationOutput::Value(output),
        trace: Some(RuleTrace { events }),
        certificates,
        effects,
    })
}

pub fn derive_definite(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    lower: &str,
    upper: &str,
) -> Result<Vec<Step>, EngineError> {
    derive_definite_configured(
        engine,
        expression,
        variable,
        lower,
        upper,
        None,
        StepVerbosity::Detailed,
    )
}

pub fn derive_definite_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    lower: &str,
    upper: &str,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    derive_definite_configured(engine, expression, variable, lower, upper, None, verbosity)
}

pub fn derive_definite_with_options(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    lower: &str,
    upper: &str,
    options: &QuadratureOptions,
) -> Result<Vec<Step>, EngineError> {
    derive_definite_configured(
        engine,
        expression,
        variable,
        lower,
        upper,
        Some(options),
        StepVerbosity::Detailed,
    )
}

#[allow(clippy::too_many_arguments)]
fn derive_definite_configured(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    lower: &str,
    upper: &str,
    numeric_fallback: Option<&QuadratureOptions>,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    validate_expression(expression, "被积表达式")?;
    validate_expression(lower, "下限")?;
    validate_expression(upper, "上限")?;
    validate_symbol(variable, "积分变量")?;
    let computation = definite_integral_computation_with_options(
        engine,
        expression,
        &DefiniteIntegralRequest {
            variable: variable.into(),
            lower: lower.into(),
            upper: upper.into(),
        },
        numeric_fallback,
    )?;
    render_rule_trace(
        engine,
        computation
            .trace
            .as_ref()
            .ok_or_else(|| EngineError::Eval("定积分计算缺少规则轨迹".into()))?,
        verbosity,
    )
}

fn numeric_bound(
    engine: &mut dyn Engine,
    expression: &str,
    label: &str,
) -> Result<f64, EngineError> {
    match engine.eval_expr(&format!("N({expression})"))? {
        Expr::Number(value) => value
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .ok_or_else(|| EngineError::Eval(format!("{label}不是有限实数"))),
        _ => Err(EngineError::Eval(format!("{label}不是数值"))),
    }
}

fn format_numeric_result(value: f64) -> String {
    if value == 0.0 {
        "0".into()
    } else {
        format!("{value:.15}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
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
        assert!(result.trace.as_ref().unwrap().events.iter().any(|event| {
            event.rule == "antiderivative-family"
                && event.bindings.contains(&("constant".into(), "C".into()))
        }));
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
