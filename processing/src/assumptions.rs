//! Product-facing access to an engine session's symbolic assumptions.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::validate_symbol;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssumptionFact {
    Real,
    Integer,
    Positive,
    Negative,
    NonZero,
}

impl AssumptionFact {
    fn engine_name(self) -> &'static str {
        match self {
            Self::Real => "Real",
            Self::Integer => "Integer",
            Self::Positive => "Positive",
            Self::Negative => "Negative",
            Self::NonZero => "NonZero",
        }
    }

    fn from_engine_name(name: &str) -> Option<Self> {
        match name {
            "Real" => Some(Self::Real),
            "Integer" => Some(Self::Integer),
            "Positive" => Some(Self::Positive),
            "Negative" => Some(Self::Negative),
            "NonZero" => Some(Self::NonZero),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssumptionState {
    pub symbol: String,
    pub fact: AssumptionFact,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssumptionSpec {
    pub symbol: String,
    pub fact: AssumptionFact,
}

pub fn assume(
    engine: &mut dyn Engine,
    symbol: &str,
    fact: AssumptionFact,
) -> Result<AssumptionState, EngineError> {
    validate_symbol(symbol, "假设变量")?;
    engine.eval(&format!("Assume({symbol},{})", fact.engine_name()))?;
    Ok(AssumptionState {
        symbol: symbol.into(),
        fact,
        active: true,
    })
}

pub fn is_assumed(
    engine: &mut dyn Engine,
    symbol: &str,
    fact: AssumptionFact,
) -> Result<bool, EngineError> {
    validate_symbol(symbol, "假设变量")?;
    let result = engine.eval(&format!("IsAssumed({symbol},{})", fact.engine_name()))?;
    match result.expr.to_string().as_str() {
        "True" => Ok(true),
        "False" => Ok(false),
        other => Err(EngineError::Parse(format!(
            "假设查询没有返回布尔值: {other}"
        ))),
    }
}

pub fn clear_assumptions(engine: &mut dyn Engine) -> Result<(), EngineError> {
    engine.eval("ClearAssumptions()")?;
    Ok(())
}

pub fn list_assumptions(engine: &mut dyn Engine) -> Result<Vec<AssumptionState>, EngineError> {
    let Expr::Call { head, args } = engine.eval_expr("ListAssumptions()")? else {
        return Err(EngineError::Parse("假设列表不是 List".into()));
    };
    if head != "List" {
        return Err(EngineError::Parse("假设列表不是 List".into()));
    }
    args.into_iter()
        .map(|entry| {
            let Expr::Call { head, args } = entry else {
                return Err(EngineError::Parse("假设条目不是 List".into()));
            };
            if head != "List" || args.len() != 2 {
                return Err(EngineError::Parse("假设条目格式无效".into()));
            }
            let symbol = match &args[0] {
                Expr::Symbol(symbol) => symbol.clone(),
                _ => return Err(EngineError::Parse("假设符号无效".into())),
            };
            let fact = match &args[1] {
                Expr::Symbol(name) => AssumptionFact::from_engine_name(name),
                _ => None,
            }
            .ok_or_else(|| EngineError::Parse("假设性质无效".into()))?;
            Ok(AssumptionState {
                symbol,
                fact,
                active: true,
            })
        })
        .collect()
}

pub fn with_assumptions<T>(
    engine: &mut dyn Engine,
    assumptions: &[AssumptionSpec],
    operation: impl FnOnce(&mut dyn Engine) -> Result<T, EngineError>,
) -> Result<T, EngineError> {
    engine.eval("PushAssumptions()")?;
    let result = (|| {
        for assumption in assumptions {
            assume(engine, &assumption.symbol, assumption.fact)?;
        }
        operation(engine)
    })();
    let restore = engine.eval("PopAssumptions()");
    match (result, restore) {
        (Ok(value), Ok(_)) => Ok(value),
        (Err(error), Ok(_)) => Err(error),
        (_, Err(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algebra::{transform, TransformKind};
    use crate::engine::RustEngine;

    #[test]
    fn assumptions_drive_simplification_and_can_be_cleared() {
        let mut engine = RustEngine::spawn().unwrap();
        assume(&mut engine, "x", AssumptionFact::Positive).unwrap();
        assert_eq!(
            list_assumptions(&mut engine).unwrap(),
            vec![
                AssumptionState {
                    symbol: "x".into(),
                    fact: AssumptionFact::Real,
                    active: true
                },
                AssumptionState {
                    symbol: "x".into(),
                    fact: AssumptionFact::Positive,
                    active: true
                },
                AssumptionState {
                    symbol: "x".into(),
                    fact: AssumptionFact::NonZero,
                    active: true
                },
            ]
        );
        assert!(is_assumed(&mut engine, "x", AssumptionFact::Real).unwrap());
        assert_eq!(
            transform(&mut engine, "Sqrt(x^2)", TransformKind::Simplify, None)
                .unwrap()
                .output,
            "x"
        );
        assert_eq!(
            transform(&mut engine, "Abs(x)", TransformKind::Simplify, None)
                .unwrap()
                .output,
            "x"
        );

        clear_assumptions(&mut engine).unwrap();
        assert!(list_assumptions(&mut engine).unwrap().is_empty());
        assert!(!is_assumed(&mut engine, "x", AssumptionFact::Positive).unwrap());
        assert_eq!(
            transform(&mut engine, "Sqrt(x^2)", TransformKind::Simplify, None)
                .unwrap()
                .output,
            "Sqrt((x ^ 2))"
        );
    }

    #[test]
    fn rejects_invalid_symbols_and_conflicting_signs() {
        let mut engine = RustEngine::spawn().unwrap();
        assert!(assume(&mut engine, "x;Echo(1)", AssumptionFact::Real).is_err());
        assume(&mut engine, "x", AssumptionFact::Positive).unwrap();
        assert!(assume(&mut engine, "x", AssumptionFact::Negative).is_err());
        assert!(is_assumed(&mut engine, "x", AssumptionFact::Positive).unwrap());
    }

    #[test]
    fn scoped_assumptions_restore_after_success_and_error() {
        let mut engine = RustEngine::spawn().unwrap();
        assume(&mut engine, "x", AssumptionFact::Real).unwrap();
        let specs = [AssumptionSpec {
            symbol: "x".into(),
            fact: AssumptionFact::Positive,
        }];
        let output = with_assumptions(&mut engine, &specs, |engine| {
            Ok(transform(engine, "Sqrt(x^2)", TransformKind::Simplify, None)?.output)
        })
        .unwrap();
        assert_eq!(output, "x");
        assert!(is_assumed(&mut engine, "x", AssumptionFact::Real).unwrap());
        assert!(!is_assumed(&mut engine, "x", AssumptionFact::Positive).unwrap());

        let error = with_assumptions(&mut engine, &specs, |_engine| {
            Err::<(), _>(EngineError::Eval("probe".into()))
        });
        assert!(error.is_err());
        assert!(!is_assumed(&mut engine, "x", AssumptionFact::Positive).unwrap());
    }
}
