//! Lightweight, non-evaluating semantic classification for product inputs.

use std::collections::BTreeSet;

use serde::Serialize;
use yacas_rs::value::{spine_refs, LispObject, ObjectKind};

use crate::engine::EngineError;
use crate::input::{collect_symbols, root_call_from_tree, validate_safe_text, RootCall};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueKind {
    Scalar,
    Expression,
    Equation,
    Matrix,
    SolutionSet,
    Unevaluated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Exactness {
    Exact,
    Symbolic,
    Approximate,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Completeness {
    Complete,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MatrixShape {
    pub rows: usize,
    pub columns: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SemanticSummary {
    pub kind: ValueKind,
    pub symbols: Vec<String>,
    pub constants: Vec<String>,
    pub shape: Option<MatrixShape>,
    pub exactness: Exactness,
    pub completeness: Option<Completeness>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzedInput {
    pub semantic: SemanticSummary,
    pub root_call: Option<RootCall>,
    pub function_heads: Vec<String>,
}

pub fn analyze_input(input: &str, label: &str) -> Result<AnalyzedInput, EngineError> {
    validate_safe_text(input, label)?;
    crate::input::with_parse_env(|env| {
        let tree = yacas_rs::parser::parse_expression(env, &format!("{input};"))
            .map_err(|error| EngineError::InvalidInput(format!("{label}语法错误: {error:?}")))?
            .ok_or_else(|| EngineError::InvalidInput(format!("{label}为空")))?;
        let root_call = root_call_from_tree(env, &tree);
        let mut symbols = BTreeSet::new();
        let mut function_heads = BTreeSet::new();
        let mut constants = BTreeSet::new();
        collect_symbols(&tree, &mut symbols, &mut function_heads, &mut constants);
        let shape = matrix_shape(&tree);
        let kind = classify(&tree, shape, symbols.is_empty());
        let exactness = exactness(&tree, symbols.is_empty(), kind);
        Ok(AnalyzedInput {
            semantic: SemanticSummary {
                kind,
                symbols: symbols.into_iter().collect(),
                constants: constants.into_iter().collect(),
                shape,
                exactness,
                completeness: (kind == ValueKind::SolutionSet).then_some(Completeness::Unknown),
            },
            root_call,
            function_heads: function_heads.into_iter().collect(),
        })
    })
}

fn classify(
    node: &std::rc::Rc<LispObject>,
    shape: Option<MatrixShape>,
    no_symbols: bool,
) -> ValueKind {
    if shape.is_some() {
        return ValueKind::Matrix;
    }
    match &node.kind {
        ObjectKind::Number(_) => ValueKind::Scalar,
        ObjectKind::Atom(name) if yacas_rs::standard::is_constant_symbol(name) => ValueKind::Scalar,
        ObjectKind::Atom(_) => ValueKind::Expression,
        ObjectKind::Generic(_) => ValueKind::Unevaluated,
        ObjectKind::Sublist(first) => {
            let nodes: Vec<_> = spine_refs(first).collect();
            let head = nodes.first().and_then(|node| node.atom_string());
            if head.as_ref().is_some_and(|head| {
                matches!(head.as_ref(), "=" | "==" | "!=" | "<" | ">" | "<=" | ">=")
            }) {
                ValueKind::Equation
            } else if head
                .as_ref()
                .is_some_and(|head| matches!(head.as_ref(), "Solve" | "OldSolve" | "OdeSolve"))
            {
                ValueKind::SolutionSet
            } else if head.as_ref().is_some_and(|head| head.as_ref() == "List") {
                ValueKind::Expression
            } else if no_symbols {
                ValueKind::Scalar
            } else {
                ValueKind::Expression
            }
        }
    }
}

fn matrix_shape(node: &std::rc::Rc<LispObject>) -> Option<MatrixShape> {
    let ObjectKind::Sublist(first) = &node.kind else {
        return None;
    };
    let nodes: Vec<_> = spine_refs(first).collect();
    let head = nodes.first()?.atom_string()?;
    if head.as_ref() == "List" {
        let rows = &nodes[1..];
        let columns = rows.first().and_then(|row| list_length(row))?;
        if rows.is_empty()
            || columns == 0
            || rows.iter().any(|row| list_length(row) != Some(columns))
        {
            return None;
        }
        return Some(MatrixShape {
            rows: rows.len(),
            columns,
        });
    }
    if matches!(head.as_ref(), "+" | "-") && nodes.len() == 3 {
        let left = matrix_shape(nodes[1])?;
        return (matrix_shape(nodes[2])? == left).then_some(left);
    }
    if head.as_ref() == "*" && nodes.len() == 3 {
        let left = matrix_shape(nodes[1])?;
        let right = matrix_shape(nodes[2])?;
        return (left.columns == right.rows).then_some(MatrixShape {
            rows: left.rows,
            columns: right.columns,
        });
    }
    if head.as_ref() == "Inverse" && nodes.len() == 2 {
        let shape = matrix_shape(nodes[1])?;
        return (shape.rows == shape.columns).then_some(shape);
    }
    if head.as_ref() == "Transpose" && nodes.len() == 2 {
        let shape = matrix_shape(nodes[1])?;
        return Some(MatrixShape {
            rows: shape.columns,
            columns: shape.rows,
        });
    }
    None
}

fn list_length(node: &std::rc::Rc<LispObject>) -> Option<usize> {
    let ObjectKind::Sublist(first) = &node.kind else {
        return None;
    };
    let nodes: Vec<_> = spine_refs(first).collect();
    nodes
        .first()
        .and_then(|head| head.atom_string())
        .filter(|head| head.as_ref() == "List")
        .map(|_| nodes.len() - 1)
}

fn exactness(node: &std::rc::Rc<LispObject>, no_symbols: bool, kind: ValueKind) -> Exactness {
    if matches!(kind, ValueKind::SolutionSet | ValueKind::Unevaluated) {
        Exactness::Unknown
    } else if contains_approximate_number(node) || has_root_head(node, "N") {
        Exactness::Approximate
    } else if no_symbols {
        Exactness::Exact
    } else {
        Exactness::Symbolic
    }
}

fn has_root_head(node: &std::rc::Rc<LispObject>, expected: &str) -> bool {
    let ObjectKind::Sublist(first) = &node.kind else {
        return false;
    };
    spine_refs(first)
        .next()
        .and_then(|head| head.atom_string())
        .is_some_and(|head| head.as_ref() == expected)
}

fn contains_approximate_number(node: &std::rc::Rc<LispObject>) -> bool {
    match &node.kind {
        ObjectKind::Number(number) => {
            let text = number.string();
            text.contains('.') || text.contains('e') || text.contains('E')
        }
        ObjectKind::Sublist(first) => spine_refs(first).any(contains_approximate_number),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_core_values_in_one_parse() {
        let expression = analyze_input("Sin(x)+Pi", "表达式").unwrap();
        assert_eq!(expression.semantic.kind, ValueKind::Expression);
        assert_eq!(expression.semantic.symbols, ["x"]);
        assert_eq!(expression.semantic.constants, ["Pi"]);
        assert_eq!(expression.semantic.exactness, Exactness::Symbolic);

        let equation = analyze_input("x^2==1", "表达式").unwrap();
        assert_eq!(equation.semantic.kind, ValueKind::Equation);

        let matrix = analyze_input("{{1,2},{3,4}}*{{5},{6}}", "表达式").unwrap();
        assert_eq!(matrix.semantic.kind, ValueKind::Matrix);
        assert_eq!(
            matrix.semantic.shape,
            Some(MatrixShape {
                rows: 2,
                columns: 1
            })
        );

        let transpose = analyze_input("Transpose({{1,2,3},{4,5,6}})", "表达式").unwrap();
        assert_eq!(transpose.semantic.kind, ValueKind::Matrix);
        assert_eq!(
            transpose.semantic.shape,
            Some(MatrixShape {
                rows: 3,
                columns: 2
            })
        );

        let solutions = analyze_input("OldSolve({x==1},{x})", "表达式").unwrap();
        assert_eq!(solutions.semantic.kind, ValueKind::SolutionSet);
        assert_eq!(solutions.semantic.completeness, Some(Completeness::Unknown));

        let ode = analyze_input("OdeSolve(y'==y+x)", "表达式").unwrap();
        assert_eq!(ode.semantic.kind, ValueKind::SolutionSet);
        assert_eq!(ode.semantic.exactness, Exactness::Unknown);

        let approximate = analyze_input("N(Pi,30)", "表达式").unwrap();
        assert_eq!(approximate.semantic.kind, ValueKind::Scalar);
        assert_eq!(approximate.semantic.exactness, Exactness::Approximate);
    }
}
