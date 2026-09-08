//! Shared, non-evaluating validation and syntax analysis for product inputs.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use crate::engine::EngineError;
use yacas_rs::value::{spine_refs, LispObject, ObjectKind};

thread_local! {
    static PARSE_ENV: RefCell<yacas_rs::env::Environment> =
        RefCell::new(yacas_rs::env::Environment::new());
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpressionAnalysis {
    pub symbols: Vec<String>,
    pub function_heads: Vec<String>,
    pub constants: Vec<String>,
}

pub fn analyze_expression(input: &str, label: &str) -> Result<ExpressionAnalysis, EngineError> {
    validate_safe_text(input, label)?;
    let tree = PARSE_ENV.with(|env| {
        yacas_rs::parser::parse_expression(&mut env.borrow_mut(), &format!("{input};"))
    });
    let tree = tree
        .map_err(|error| EngineError::InvalidInput(format!("{label}语法错误: {error:?}")))?
        .ok_or_else(|| EngineError::InvalidInput(format!("{label}为空")))?;

    let mut symbols = BTreeSet::new();
    let mut function_heads = BTreeSet::new();
    let mut constants = BTreeSet::new();
    collect_symbols(&tree, &mut symbols, &mut function_heads, &mut constants);
    Ok(ExpressionAnalysis {
        symbols: symbols.into_iter().collect(),
        function_heads: function_heads.into_iter().collect(),
        constants: constants.into_iter().collect(),
    })
}

pub fn validate_expression(input: &str, label: &str) -> Result<(), EngineError> {
    analyze_expression(input, label).map(|_| ())
}

pub fn contains_exact_power(
    input: &str,
    symbol: &str,
    exponent: &str,
    label: &str,
) -> Result<bool, EngineError> {
    validate_safe_text(input, label)?;
    let tree = PARSE_ENV.with(|env| {
        yacas_rs::parser::parse_expression(&mut env.borrow_mut(), &format!("{input};"))
    });
    let tree = tree
        .map_err(|error| EngineError::InvalidInput(format!("{label}语法错误: {error:?}")))?
        .ok_or_else(|| EngineError::InvalidInput(format!("{label}为空")))?;
    Ok(has_exact_power(&tree, symbol, exponent))
}

pub fn validate_symbol(symbol: &str, label: &str) -> Result<(), EngineError> {
    let mut chars = symbol.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '\'')
    {
        return Err(EngineError::InvalidInput(format!("无效{label}: {symbol}")));
    }
    Ok(())
}

pub fn strip_tex_delimiters(tex: &str) -> String {
    let tex = tex.trim();
    tex.strip_prefix('$')
        .and_then(|value| value.strip_suffix('$'))
        .unwrap_or(tex)
        .to_string()
}

fn validate_safe_text(input: &str, label: &str) -> Result<(), EngineError> {
    if input.trim().is_empty()
        || input
            .chars()
            .any(|character| matches!(character, ';' | '\n' | '\r' | ':' | '"'))
    {
        return Err(EngineError::InvalidInput(format!(
            "{label}为空或包含不允许的字符"
        )));
    }
    Ok(())
}

fn collect_symbols(
    node: &Rc<LispObject>,
    symbols: &mut BTreeSet<String>,
    function_heads: &mut BTreeSet<String>,
    constants: &mut BTreeSet<String>,
) {
    match &node.kind {
        ObjectKind::Sublist(first) => {
            let mut nodes = spine_refs(first);
            if let Some(head) = nodes.next() {
                if let Some(name) = head.atom_string() {
                    function_heads.insert(name.to_string());
                } else {
                    collect_symbols(head, symbols, function_heads, constants);
                }
            }
            for argument in nodes {
                collect_symbols(argument, symbols, function_heads, constants);
            }
        }
        ObjectKind::Atom(name) => {
            if name.starts_with('"') {
                return;
            }
            if yacas_rs::standard::is_constant_symbol(name) {
                constants.insert(name.to_string());
            } else {
                symbols.insert(name.to_string());
            }
        }
        ObjectKind::Number(_) | ObjectKind::Generic(_) => {}
    }
}

fn has_exact_power(node: &Rc<LispObject>, symbol: &str, exponent: &str) -> bool {
    let ObjectKind::Sublist(first) = &node.kind else {
        return false;
    };
    let nodes: Vec<_> = spine_refs(first).collect();
    if nodes.len() == 3
        && nodes[0]
            .atom_string()
            .is_some_and(|name| name.as_ref() == "^")
        && nodes[1]
            .atom_string()
            .is_some_and(|name| name.as_ref() == symbol)
        && matches!(&nodes[2].kind, ObjectKind::Number(number) if number.string() == exponent)
    {
        return true;
    }
    nodes
        .iter()
        .any(|child| has_exact_power(child, symbol, exponent))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_uses_the_language_parser_and_separates_symbol_roles() {
        let result = analyze_expression("Sin(x)+Pi*y==1e10", "方程").unwrap();
        assert_eq!(result.symbols, ["x", "y"]);
        assert!(result.function_heads.contains(&"Sin".to_string()));
        assert!(result.constants.contains(&"Pi".to_string()));
    }

    #[test]
    fn validation_rejects_bad_structure_and_command_injection() {
        for input in ["", "x);Echo(1);(x", "(x+1]", ")("] {
            assert!(validate_expression(input, "表达式").is_err(), "{input}");
        }
        assert!(validate_expression("(x+1)^2", "表达式").is_ok());
    }

    #[test]
    fn detects_exact_symbol_powers_without_evaluation() {
        assert!(contains_exact_power("y'+y==x*y^2", "y", "2", "ODE").unwrap());
        assert!(!contains_exact_power("y'+y==x^2*y", "y", "2", "ODE").unwrap());
    }
}
