//! Conditions carried by symbolic results.

use crate::engine::{EngineError, Expr};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Equal,
    GreaterThan,
    LessThan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Condition {
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
        conditions: Vec<Condition>,
    },
    Any {
        conditions: Vec<Condition>,
    },
}

pub(crate) fn unpack_conditional(expr: Expr) -> Result<(Expr, Vec<Condition>), EngineError> {
    let Expr::Call { head, mut args } = expr else {
        return Ok((expr, vec![]));
    };
    if head != "ConditionalValue" || args.len() != 2 {
        return Ok((Expr::Call { head, args }, vec![]));
    }
    let condition = parse_condition(&args.pop().unwrap())?;
    Ok((args.pop().unwrap(), vec![condition]))
}

pub(crate) fn parse_condition(expr: &Expr) -> Result<Condition, EngineError> {
    match expr {
        Expr::Call { head, args }
            if [
                "=", "==", ">", "<", "ConditionEqual", "ConditionGreater", "ConditionLess",
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
            Ok(Condition::Relation {
                left: args[0].to_string(),
                relation,
                right: args[1].to_string(),
            })
        }
        Expr::Call { head, args } if head == "ConditionProperty" && args.len() == 2 => {
            Ok(Condition::Property {
                expression: args[0].to_string(),
                fact: args[1].to_string(),
            })
        }
        Expr::Call { head, args } if head == "ConditionAnd" || head == "ConditionOr" => {
            let conditions = args
                .iter()
                .map(parse_condition)
                .collect::<Result<Vec<_>, _>>()?;
            if head == "ConditionAnd" {
                Ok(Condition::All { conditions })
            } else {
                Ok(Condition::Any { conditions })
            }
        }
        other => Err(EngineError::Parse(format!("无法识别的结果条件: {other}"))),
    }
}
