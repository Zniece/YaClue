//! Structured API for finite, infinite, and one-sided limits.

use crate::engine::{Engine, EngineError, Expr};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LimitCondition {
    pub expression: String,
    pub fact: String,
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

pub fn limit(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    at: &str,
    direction: LimitDirection,
) -> Result<LimitResult, EngineError> {
    validate_expression(expression, "极限表达式")?;
    validate_expression(at, "趋近点")?;
    validate_variable(variable)?;

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
        strip_dollars(&result.tex)
    } else {
        strip_dollars(&engine.eval(&value)?.tex)
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
    let conditions = list_items(&condition_expr)?
        .iter()
        .map(|condition| {
            let pair = list_items(condition)?;
            if pair.len() != 2 {
                return Err(EngineError::Parse(format!(
                    "极限条件不是二元组: {condition}"
                )));
            }
            Ok(LimitCondition {
                expression: pair[0].to_string(),
                fact: pair[1].to_string(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((value, conditions))
}

fn list_items(expr: &Expr) -> Result<&[Expr], EngineError> {
    match expr {
        Expr::Call { head, args } if head == "List" => Ok(args),
        other => Err(EngineError::Parse(format!("极限条件不是列表: {other}"))),
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

fn validate_expression(input: &str, label: &str) -> Result<(), EngineError> {
    let input = input.trim();
    if input.is_empty()
        || input
            .chars()
            .any(|c| matches!(c, ';' | '\n' | '\r' | ':' | '"'))
    {
        return Err(EngineError::Eval(format!("{label}为空或包含不允许的字符")));
    }
    for (open, close) in [('(', ')'), ('{', '}'), ('[', ']')] {
        if input.chars().filter(|&c| c == open).count()
            != input.chars().filter(|&c| c == close).count()
        {
            return Err(EngineError::Eval(format!("{label}括号不匹配")));
        }
    }
    Ok(())
}

fn validate_variable(variable: &str) -> Result<(), EngineError> {
    let mut chars = variable.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '\'')
    {
        return Err(EngineError::Eval(format!("无效极限变量: {variable}")));
    }
    Ok(())
}

fn strip_dollars(tex: &str) -> String {
    let tex = tex.trim();
    tex.strip_prefix('$')
        .and_then(|value| value.strip_suffix('$'))
        .unwrap_or(tex)
        .to_string()
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
            vec![LimitCondition {
                expression: "n".into(),
                fact: "Positive".into(),
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
        assert_eq!(negative.conditions[0].fact, "Negative");
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
