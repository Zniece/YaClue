//! Structured API for finite, infinite, and one-sided limits.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{strip_tex_delimiters, validate_expression, validate_symbol};
use crate::steps::{Step, StepImportance, StepVerbosity};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitDirection {
    Both,
    Left,
    Right,
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
        "Transform" if args.len() == 6 => &args[5],
        _ => &args[0],
    };
    let (final_value_node, final_conditions) = unpack_conditional(final_node.clone())?;
    let final_value_override = final_value_node.to_string();

    let direction_suffix = match direction {
        LimitDirection::Both => "",
        LimitDirection::Left => "^{-}",
        LimitDirection::Right => "^{+}",
    };
    let mut steps = Vec::new();
    if verbosity == StepVerbosity::Detailed {
        let rendered = engine
            .render_tex_batch(&[expression.trim().into(), at.trim().into()])?
            .into_iter()
            .map(|tex| strip_tex_delimiters(&tex))
            .collect::<Vec<_>>();
        steps.push(Step {
            rule: "limit-start".into(),
            expr: format!("Limit({variable},{at})({})", expression.trim()),
            why: "建立极限问题".into(),
            tex: format!(
                "\\lim_{{{} \\to {}{}}} {}",
                variable, rendered[1], direction_suffix, rendered[0]
            ),
            importance: StepImportance::Routine,
        });
    }

    match args[0].to_string().as_str() {
        "Direct" if args.len() == 3 => {
            let substituted = args[1].to_string();
            let final_value = final_value_override.clone();
            let tex = engine
                .render_tex_batch(&[substituted.clone(), final_value.clone()])?
                .into_iter()
                .map(|tex| strip_tex_delimiters(&tex))
                .collect::<Vec<_>>();
            steps.push(Step {
                rule: "limit-direct-substitution".into(),
                expr: substituted.clone(),
                why: format!("直接代入 {variable} = {}", at.trim()),
                tex: tex[0].clone(),
                importance: StepImportance::Key,
            });
            if final_value != substituted {
                steps.push(Step {
                    rule: "limit-result".into(),
                    expr: final_value,
                    why: "得到极限值".into(),
                    tex: tex[1].clone(),
                    importance: StepImportance::Key,
                });
            }
        }
        "LHopital" if args.len() == 3 => {
            let Expr::Call {
                head: event_head,
                args: event_args,
            } = &args[1]
            else {
                return Err(EngineError::Parse("洛必达事件链不是列表".into()));
            };
            if event_head != "List" || event_args.is_empty() {
                return Err(EngineError::Parse("洛必达事件链为空".into()));
            }
            let mut events = Vec::with_capacity(event_args.len());
            for event in event_args {
                let Expr::Call { head, args: values } = event else {
                    return Err(EngineError::Parse("洛必达事件不是列表".into()));
                };
                if head != "List" || values.len() != 3 {
                    return Err(EngineError::Parse("洛必达事件形态异常".into()));
                }
                events.push((
                    values[0].to_string(),
                    values[1].to_string(),
                    values[2].to_string(),
                ));
            }
            let final_value = final_value_override.clone();
            let show_indeterminate = verbosity != StepVerbosity::Concise;
            let mut expressions =
                Vec::with_capacity(events.len() * if show_indeterminate { 3 } else { 1 } + 1);
            for (numerator, denominator, derivative) in &events {
                if show_indeterminate {
                    expressions.push(numerator.clone());
                    expressions.push(denominator.clone());
                }
                expressions.push(derivative.clone());
            }
            expressions.push(final_value.clone());
            let tex = engine
                .render_tex_batch(&expressions)?
                .into_iter()
                .map(|tex| strip_tex_delimiters(&tex))
                .collect::<Vec<_>>();
            let mut tex_index = 0;
            let event_count = events.len();
            for (index, (numerator, denominator, derivative)) in events.into_iter().enumerate() {
                if show_indeterminate {
                    steps.push(Step {
                        rule: "limit-indeterminate-form".into(),
                        expr: format!("{numerator}/{denominator}"),
                        why: format!("代入后分子与分母分别趋于 {numerator} 和 {denominator}"),
                        tex: format!("\\frac{{{}}}{{{}}}", tex[tex_index], tex[tex_index + 1]),
                        importance: StepImportance::Normal,
                    });
                    tex_index += 2;
                }
                steps.push(Step {
                    rule: "limit-lhopital".into(),
                    expr: derivative,
                    why: if event_count == 1 {
                        "应用洛必达法则，分别对分子和分母求导".into()
                    } else {
                        format!("第 {} 次应用洛必达法则", index + 1)
                    },
                    tex: tex[tex_index].clone(),
                    importance: StepImportance::Key,
                });
                tex_index += 1;
            }
            steps.push(Step {
                rule: "limit-result".into(),
                expr: final_value,
                why: "计算变换后的极限".into(),
                tex: tex[tex_index].clone(),
                importance: StepImportance::Key,
            });
        }
        "Transform" if args.len() == 6 => {
            let kind = args[1].to_string();
            let left_limit = args[2].to_string();
            let right_limit = args[3].to_string();
            let transformed = args[4].to_string();
            let final_value = final_value_override.clone();
            let show_indeterminate = verbosity != StepVerbosity::Concise;
            let mut expressions = if show_indeterminate {
                vec![left_limit.clone(), right_limit.clone()]
            } else {
                vec![]
            };
            expressions.push(transformed.clone());
            expressions.push(final_value.clone());
            let tex = engine
                .render_tex_batch(&expressions)?
                .into_iter()
                .map(|tex| strip_tex_delimiters(&tex))
                .collect::<Vec<_>>();
            let offset = if show_indeterminate {
                let (operator, symbol) = match kind.as_str() {
                    "Product" => ("*", "\\cdot"),
                    "Difference" => ("-", "-"),
                    "Power" => ("^", "^"),
                    _ => return Err(EngineError::Parse(format!("未知极限变换类型: {kind}"))),
                };
                let form_tex = if kind == "Power" {
                    format!("{{{}}}^{{{}}}", tex[0], tex[1])
                } else {
                    format!("{} {} {}", tex[0], symbol, tex[1])
                };
                steps.push(Step {
                    rule: "limit-indeterminate-form".into(),
                    expr: format!("{left_limit}{operator}{right_limit}"),
                    why: format!("代入后得到 {left_limit} 与 {right_limit} 构成的不定式"),
                    tex: form_tex,
                    importance: StepImportance::Normal,
                });
                2
            } else {
                0
            };
            let (rule, why) = match kind.as_str() {
                "Product" => (
                    "limit-transform-product",
                    "将乘积型不定式改写为商式，再计算其极限",
                ),
                "Difference" => (
                    "limit-transform-difference",
                    "提取一个发散因子，将无穷差改写为可计算的形式",
                ),
                "Power" => (
                    "limit-transform-power",
                    "取对数，将幂型不定式化为指数中的乘积极限",
                ),
                _ => return Err(EngineError::Parse(format!("未知极限变换类型: {kind}"))),
            };
            steps.push(Step {
                rule: rule.into(),
                expr: transformed,
                why: why.into(),
                tex: tex[offset].clone(),
                importance: StepImportance::Key,
            });
            steps.push(Step {
                rule: "limit-result".into(),
                expr: final_value,
                why: "计算变换后的极限".into(),
                tex: tex[offset + 1].clone(),
                importance: StepImportance::Key,
            });
        }
        method => return Err(EngineError::Parse(format!("未知极限步骤方法: {method}"))),
    }

    if let Some(condition) = final_conditions.first() {
        let condition_expr = format_condition_expression(condition);
        let condition_tex = strip_tex_delimiters(
            &engine
                .render_tex_batch(std::slice::from_ref(&condition_expr))?
                .remove(0),
        );
        let insert_at = steps.len().saturating_sub(1);
        steps.insert(
            insert_at,
            Step {
                rule: "limit-condition".into(),
                expr: condition_expr,
                why: "在此参数条件下采用对应的极限分支".into(),
                tex: condition_tex,
                importance: StepImportance::Normal,
            },
        );
    }

    Ok(steps)
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

    let args = match direction {
        LimitDirection::Both => format!("{variable},{at}"),
        LimitDirection::Left => format!("{variable},{at},Left"),
        LimitDirection::Right => format!("{variable},{at},Right"),
    };
    let result = engine.eval(&format!("Limit({args})({expression})"))?;
    let (value_expr, conditions) = unpack_conditional(result.expr)?;
    let value = value_expr.to_string();
    let status = classify(&value_expr, &value);
    let tex = if conditions.is_empty() {
        strip_tex_delimiters(&result.tex)
    } else {
        strip_tex_delimiters(&engine.eval(&value)?.tex)
    };
    Ok(LimitResult {
        status,
        expression: expression.trim().into(),
        variable: variable.into(),
        at: at.trim().into(),
        value,
        tex,
        direction,
        conditions,
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

fn classify(expr: &Expr, value: &str) -> LimitStatus {
    if matches!(expr, Expr::Call { head, .. } if head == "Limit") || value.starts_with("Limit(") {
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
    }

    #[test]
    fn explains_direct_substitution_and_lhopital_limits() {
        let mut engine = RustEngine::spawn().unwrap();
        let direct = limit_steps(&mut engine, "x^2+1", "x", "2", LimitDirection::Both).unwrap();
        assert_eq!(direct.last().unwrap().rule, "limit-direct-substitution");
        assert_eq!(direct.last().unwrap().expr, "5");

        let lhopital =
            limit_steps(&mut engine, "(x^2-4)/(x-2)", "x", "2", LimitDirection::Both).unwrap();
        assert_eq!(
            lhopital
                .iter()
                .map(|step| step.rule.as_str())
                .collect::<Vec<_>>(),
            vec![
                "limit-start",
                "limit-indeterminate-form",
                "limit-lhopital",
                "limit-result"
            ]
        );
        assert_eq!(lhopital[1].expr, "0/0");
        assert_eq!(lhopital[2].expr, "(2 * x)");
        assert_eq!(lhopital[3].expr, "4");
        assert!(lhopital.iter().all(|step| !step.tex.is_empty()));
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
        assert_eq!(concise[0].rule, "limit-lhopital");
        assert_eq!(concise[1].rule, "limit-result");
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
}
