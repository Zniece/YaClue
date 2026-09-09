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

/// Stable, storage-independent view of a parsed root call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootCall {
    pub head: String,
    pub arguments: Vec<String>,
    pub argument_heads: Vec<Option<String>>,
}

pub fn root_call(input: &str, label: &str) -> Result<Option<RootCall>, EngineError> {
    validate_safe_text(input, label)?;
    PARSE_ENV.with(|cell| {
        let mut env = cell.borrow_mut();
        let tree = yacas_rs::parser::parse_expression(&mut env, &format!("{input};"))
            .map_err(|error| EngineError::InvalidInput(format!("{label}语法错误: {error:?}")))?
            .ok_or_else(|| EngineError::InvalidInput(format!("{label}为空")))?;
        let ObjectKind::Sublist(first) = &tree.kind else {
            return Ok(None);
        };
        let nodes: Vec<_> = spine_refs(first).collect();
        let Some(head) = nodes.first().and_then(|node| node.atom_string()) else {
            return Ok(None);
        };
        let mut arguments = Vec::with_capacity(nodes.len().saturating_sub(1));
        let mut argument_heads = Vec::with_capacity(nodes.len().saturating_sub(1));
        for argument in &nodes[1..] {
            arguments.push(yacas_rs::printer::infix_print(&env, argument));
            let argument_head = match &argument.kind {
                ObjectKind::Sublist(first) => spine_refs(first)
                    .next()
                    .and_then(|node| node.atom_string())
                    .map(|name| name.to_string()),
                _ => None,
            };
            argument_heads.push(argument_head);
        }
        Ok(Some(RootCall {
            head: head.to_string(),
            arguments,
            argument_heads,
        }))
    })
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

/// Return the function head when an equation has the exact form
/// `Function(variable) == expression` (in either order) and the other side is
/// free of the solved variable.
pub fn direct_function_equation(
    input: &str,
    variable: &str,
    functions: &[&str],
) -> Result<Option<String>, EngineError> {
    validate_safe_text(input, "方程")?;
    let tree = PARSE_ENV.with(|env| {
        yacas_rs::parser::parse_expression(&mut env.borrow_mut(), &format!("{input};"))
    });
    let tree = tree
        .map_err(|error| EngineError::InvalidInput(format!("方程语法错误: {error:?}")))?
        .ok_or_else(|| EngineError::InvalidInput("方程为空".into()))?;
    let ObjectKind::Sublist(first) = &tree.kind else {
        return Ok(None);
    };
    let nodes: Vec<_> = spine_refs(first).collect();
    if nodes.len() != 3
        || !nodes[0]
            .atom_string()
            .is_some_and(|head| matches!(head.as_ref(), "=" | "=="))
    {
        return Ok(None);
    }
    for (call, other) in [(nodes[1], nodes[2]), (nodes[2], nodes[1])] {
        if contains_atom(other, variable) {
            continue;
        }
        let ObjectKind::Sublist(call_first) = &call.kind else {
            continue;
        };
        let call_nodes: Vec<_> = spine_refs(call_first).collect();
        if call_nodes.len() == 2
            && call_nodes[1]
                .atom_string()
                .is_some_and(|argument| argument.as_ref() == variable)
        {
            if let Some(function) = call_nodes[0].atom_string().filter(|function| {
                functions
                    .iter()
                    .any(|candidate| *candidate == function.as_ref())
            }) {
                return Ok(Some(function.to_string()));
            }
        }
    }
    Ok(None)
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

pub fn contains_product_factor(
    input: &str,
    symbol: &str,
    label: &str,
) -> Result<bool, EngineError> {
    validate_safe_text(input, label)?;
    let tree = PARSE_ENV.with(|env| {
        yacas_rs::parser::parse_expression(&mut env.borrow_mut(), &format!("{input};"))
    });
    let tree = tree
        .map_err(|error| EngineError::InvalidInput(format!("{label}语法错误: {error:?}")))?
        .ok_or_else(|| EngineError::InvalidInput(format!("{label}为空")))?;
    Ok(has_product_factor(&tree, symbol))
}

pub fn contains_ratio_symbols(
    input: &str,
    first_symbol: &str,
    second_symbol: &str,
    label: &str,
) -> Result<bool, EngineError> {
    validate_safe_text(input, label)?;
    let tree = PARSE_ENV.with(|env| {
        yacas_rs::parser::parse_expression(&mut env.borrow_mut(), &format!("{input};"))
    });
    let tree = tree
        .map_err(|error| EngineError::InvalidInput(format!("{label}语法错误: {error:?}")))?
        .ok_or_else(|| EngineError::InvalidInput(format!("{label}为空")))?;
    Ok(has_ratio_symbols(&tree, first_symbol, second_symbol))
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

fn has_product_factor(node: &Rc<LispObject>, symbol: &str) -> bool {
    let ObjectKind::Sublist(first) = &node.kind else {
        return false;
    };
    let nodes: Vec<_> = spine_refs(first).collect();
    if nodes.len() > 2
        && nodes[0]
            .atom_string()
            .is_some_and(|name| name.as_ref() == "*")
        && nodes[1..].iter().any(|factor| {
            factor
                .atom_string()
                .is_some_and(|name| name.as_ref() == symbol)
        })
    {
        return true;
    }
    nodes.iter().any(|child| has_product_factor(child, symbol))
}

fn has_ratio_symbols(node: &Rc<LispObject>, first_symbol: &str, second_symbol: &str) -> bool {
    let ObjectKind::Sublist(first) = &node.kind else {
        return false;
    };
    let nodes: Vec<_> = spine_refs(first).collect();
    if nodes.len() == 3
        && nodes[0]
            .atom_string()
            .is_some_and(|name| name.as_ref() == "/")
        && contains_atom(node, first_symbol)
        && contains_atom(node, second_symbol)
    {
        return true;
    }
    nodes
        .iter()
        .any(|child| has_ratio_symbols(child, first_symbol, second_symbol))
}

fn contains_atom(node: &Rc<LispObject>, symbol: &str) -> bool {
    if node
        .atom_string()
        .is_some_and(|name| name.as_ref() == symbol)
    {
        return true;
    }
    let ObjectKind::Sublist(first) = &node.kind else {
        return false;
    };
    spine_refs(first).any(|child| contains_atom(child, symbol))
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

    #[test]
    fn detects_symbols_used_as_product_factors_without_evaluation() {
        assert!(contains_product_factor("M+(x^2+y)*y'==0", "y'", "ODE").unwrap());
        assert!(!contains_product_factor("y'+y==x", "y'", "ODE").unwrap());
    }

    #[test]
    fn detects_ratios_containing_both_symbols_without_evaluation() {
        assert!(contains_ratio_symbols("y'==(x+y)/x", "x", "y", "ODE").unwrap());
        assert!(contains_ratio_symbols("y'==(y/x)^2", "x", "y", "ODE").unwrap());
        assert!(!contains_ratio_symbols("y'==x+y", "x", "y", "ODE").unwrap());
    }

    #[test]
    fn recognizes_only_direct_function_equations() {
        let functions = ["Sin", "Cos", "Tan"];
        assert_eq!(
            direct_function_equation("Sin(x)==1/2", "x", &functions).unwrap(),
            Some("Sin".into())
        );
        assert_eq!(
            direct_function_equation("0==Cos(x)", "x", &functions).unwrap(),
            Some("Cos".into())
        );
        assert_eq!(
            direct_function_equation("Sin(2*x)==0", "x", &functions).unwrap(),
            None
        );
        assert_eq!(
            direct_function_equation("Sin(x)==x", "x", &functions).unwrap(),
            None
        );
    }

    #[test]
    fn exposes_root_calls_without_engine_storage_types() {
        let derivative = root_call("D(x,2)Sin(x)", "表达式").unwrap().unwrap();
        assert_eq!(derivative.head, "D");
        assert_eq!(derivative.arguments, ["x", "2", "Sin(x)"]);

        for (source, head, arguments) in [
            ("Integrate(x,0,Pi)Sin(x)", "Integrate", 4),
            ("Limit(x,0)Sin(x)/x", "Limit", 3),
            ("N(Pi,30)", "N", 2),
            ("OdeSolve(y'==y)", "OdeSolve", 1),
            ("OldSolve({x+y==3,x-y==1},{x,y})", "OldSolve", 2),
            ("Plot(Sin(x),x,-6.28,6.28)", "Plot", 4),
        ] {
            let call = root_call(source, "表达式").unwrap().unwrap();
            assert_eq!(call.head, head, "{source}");
            assert_eq!(call.arguments.len(), arguments, "{source}");
        }

        let product = root_call("{{1,2},{3,4}}*{{5,6},{7,8}}", "表达式")
            .unwrap()
            .unwrap();
        assert_eq!(product.head, "*");
        assert_eq!(
            product.argument_heads,
            [Some("List".into()), Some("List".into())]
        );
    }
}
