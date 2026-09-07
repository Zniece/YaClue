//! Stable processing API for common algebraic transformations.

use crate::engine::{Engine, EngineError, Expr};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformKind {
    Simplify,
    Tidy,
    Expand,
    Factor,
    Apart,
}

impl TransformKind {
    fn name(self) -> &'static str {
        match self {
            Self::Simplify => "Simplify",
            Self::Tidy => "Tidy",
            Self::Expand => "Expand",
            Self::Factor => "Factor",
            Self::Apart => "Apart",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TransformResult {
    pub operation: String,
    pub input: String,
    pub output: String,
    pub tex: String,
    /// Whether the output tree differs from the engine's evaluated input tree.
    pub changed: bool,
    /// The operation remained as the result head and was not completed.
    pub unresolved: bool,
}

pub fn transform(
    engine: &mut dyn Engine,
    input: &str,
    kind: TransformKind,
    variable: Option<&str>,
) -> Result<TransformResult, EngineError> {
    validate_expression(input)?;
    let operation = kind.name();
    let command = match kind {
        TransformKind::Apart => {
            let variable = variable.ok_or_else(|| {
                EngineError::Eval("Apart 需要指定变量".into())
            })?;
            validate_variable(variable)?;
            format!("Apart({input},{variable})")
        }
        _ => {
            if variable.is_some() {
                return Err(EngineError::Eval(format!("{operation} 不接受变量参数")));
            }
            format!("{operation}({input})")
        }
    };

    // Evaluate the input independently so `changed` describes the requested
    // transformation rather than ordinary parsing/canonicalization.
    let before = engine.eval(input)?.expr;
    let result = engine.eval(&command)?;
    let unresolved = matches!(&result.expr, Expr::Call { head, .. } if head == operation);
    Ok(TransformResult {
        operation: operation.into(),
        input: input.trim().into(),
        output: result.expr.to_string(),
        tex: strip_dollars(&result.tex),
        changed: result.expr != before,
        unresolved,
    })
}

fn validate_expression(input: &str) -> Result<(), EngineError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(EngineError::Eval("表达式为空".into()));
    }
    if input.chars().any(|c| matches!(c, ';' | '\n' | '\r' | ':' | '"')) {
        return Err(EngineError::Eval("表达式包含不允许的字符".into()));
    }
    let open = input.chars().filter(|&c| c == '(').count();
    let close = input.chars().filter(|&c| c == ')').count();
    if open != close {
        return Err(EngineError::Eval("表达式括号不匹配".into()));
    }
    Ok(())
}

fn validate_variable(variable: &str) -> Result<(), EngineError> {
    let mut chars = variable.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '\'')
    {
        return Err(EngineError::Eval("变量名无效".into()));
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

    fn equivalent(engine: &mut dyn Engine, left: &str, right: &str) {
        let result = engine.eval(&format!("Simplify(({left})-({right}))")).unwrap();
        assert_eq!(result.expr.to_string(), "0", "{left} != {right}");
    }

    #[test]
    fn common_polynomial_transforms_are_structured_and_equivalent() {
        let mut engine = RustEngine::spawn().unwrap();
        for (kind, input, expected, variable) in [
            (TransformKind::Simplify, "x+x", "2*x", None),
            (TransformKind::Tidy, "(x+x)/2", "x", None),
            (TransformKind::Expand, "(x+1)^2", "x^2+2*x+1", None),
            (TransformKind::Factor, "x^2-1", "(x-1)*(x+1)", None),
            (TransformKind::Apart, "1/(x^2-1)", "1/(2*(x-1))-1/(2*(x+1))", Some("x")),
        ] {
            let result = transform(&mut engine, input, kind, variable).unwrap();
            assert!(!result.unresolved, "{}: {}", result.operation, result.output);
            assert!(!result.tex.is_empty());
            equivalent(&mut engine, &result.output, expected);
        }
    }

    #[test]
    fn unchanged_and_unresolved_are_distinct() {
        let mut engine = RustEngine::spawn().unwrap();
        let unchanged = transform(&mut engine, "x", TransformKind::Simplify, None).unwrap();
        assert!(!unchanged.changed);
        assert!(!unchanged.unresolved);

        let unsupported = transform(&mut engine, "Sin(x)", TransformKind::Factor, None).unwrap();
        assert!(unsupported.unresolved);
        assert!(unsupported.output.starts_with("Factor("));
    }

    #[test]
    fn rejects_bad_inputs_and_operation_arguments() {
        let mut engine = RustEngine::spawn().unwrap();
        assert!(transform(&mut engine, "x);Echo(1);(x", TransformKind::Simplify, None).is_err());
        assert!(transform(&mut engine, "x^2", TransformKind::Apart, None).is_err());
        assert!(transform(&mut engine, "x^2", TransformKind::Apart, Some("x;Echo(1)")).is_err());
        assert!(transform(&mut engine, "x^2", TransformKind::Expand, Some("x")).is_err());
    }
}
