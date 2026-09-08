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
            let final_value = args[2].to_string();
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
        "LHopital" if args.len() == 5 => {
            let numerator = args[1].to_string();
            let denominator = args[2].to_string();
            let derivative = args[3].to_string();
            let final_value = args[4].to_string();
            let show_indeterminate = verbosity != StepVerbosity::Concise;
            let expressions = if show_indeterminate {
                vec![
                    numerator.clone(),
                    denominator.clone(),
                    derivative.clone(),
                    final_value.clone(),
                ]
            } else {
                vec![derivative.clone(), final_value.clone()]
            };
            let tex = engine
                .render_tex_batch(&expressions)?
                .into_iter()
                .map(|tex| strip_tex_delimiters(&tex))
                .collect::<Vec<_>>();
            let offset = if show_indeterminate {
                steps.push(Step {
                    rule: "limit-indeterminate-form".into(),
                    expr: format!("{numerator}/{denominator}"),
                    why: format!("代入后分子与分母分别趋于 {numerator} 和 {denominator}"),
                    tex: format!("\\frac{{{}}}{{{}}}", tex[0], tex[1]),
                    importance: StepImportance::Normal,
                });
                2
            } else {
                0
            };
            steps.push(Step {
                rule: "limit-lhopital".into(),
                expr: derivative,
                why: "应用洛必达法则，分别对分子和分母求导".into(),
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

    Ok(steps)
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
