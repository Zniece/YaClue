//! Lightweight, non-evaluating semantic classification for product inputs.

use std::collections::BTreeSet;

use serde::Serialize;
use yacas_rs::value::{spine_refs, LispObject, ObjectKind};

use crate::engine::EngineError;
use crate::input::{root_call_from_tree, validate_safe_text, RootCall};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueKind {
    Scalar,
    Expression,
    Equation,
    Matrix,
    SolutionSet,
    FunctionFamily,
    SampledData,
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
    pub bound_symbols: Vec<String>,
    pub symbol_identities: Vec<crate::binding::SymbolIdentity>,
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
        Ok(analyze_tree(env, &tree))
    })
}

/// Build the legacy product summary from an already parsed AST. This lets the
/// elaboration boundary remain the only parser on the request path.
pub(crate) fn analyze_tree(
    env: &yacas_rs::env::Environment,
    tree: &std::rc::Rc<LispObject>,
) -> AnalyzedInput {
    let root_call = root_call_from_tree(env, tree);
    let binding = crate::binding::analyze_tree(tree);
    let no_symbols = binding.free_symbols.is_empty() && binding.bound_symbols.is_empty();
    let symbols = binding.free_symbols;
    let function_heads = binding.function_heads;
    let constants = binding.constants;
    let shape = matrix_shape(tree);
    let kind = classify(tree, shape, no_symbols);
    let exactness = exactness(tree, no_symbols, kind);
    AnalyzedInput {
        semantic: SemanticSummary {
            kind,
            symbols: symbols.into_iter().collect(),
            bound_symbols: binding.bound_symbols.into_iter().collect(),
            symbol_identities: binding.identities.into_iter().collect(),
            constants: constants.into_iter().collect(),
            shape,
            exactness,
            completeness: (kind == ValueKind::SolutionSet).then_some(Completeness::Unknown),
        },
        root_call,
        function_heads: function_heads.into_iter().collect(),
    }
}

/// Reclassify a domain result without carrying consumed input symbols into the
/// public result. Generated identities and operation-bound symbols are supplied
/// by the domain adapter rather than inferred from their spelling.
pub fn project_result(
    input: &SemanticSummary,
    expression: &str,
    arbitrary_constants: &[String],
    additional_bound_symbols: &[&str],
    kind: Option<ValueKind>,
) -> Result<SemanticSummary, EngineError> {
    let generated: BTreeSet<_> = arbitrary_constants.iter().cloned().collect();
    let mut output = analyze_input(expression, "结果表达式")?.semantic;
    let mut bound: BTreeSet<_> = output.bound_symbols.iter().cloned().collect();
    if kind == Some(ValueKind::FunctionFamily) {
        bound.extend(input.bound_symbols.iter().cloned());
    }
    bound.extend(
        additional_bound_symbols
            .iter()
            .map(|name| (*name).to_string()),
    );
    let output_names: BTreeSet<_> = output
        .symbols
        .iter()
        .chain(&output.bound_symbols)
        .cloned()
        .collect();
    bound.retain(|name| output_names.contains(name));
    output
        .symbols
        .retain(|name| !bound.contains(name) && !generated.contains(name));
    output.bound_symbols = bound.iter().cloned().collect();
    output.symbol_identities = output_names
        .into_iter()
        .map(|name| {
            if generated.contains(&name) {
                crate::binding::SymbolIdentity::generated(
                    name,
                    crate::binding::SymbolRole::ArbitraryConstant,
                )
                .expect("arbitrary constant has a generated role")
            } else if bound.contains(&name) {
                let binder = input
                    .symbol_identities
                    .iter()
                    .find(|identity| identity.name == name && identity.binder.is_some())
                    .and_then(|identity| identity.binder)
                    .or(Some(0));
                crate::binding::SymbolIdentity {
                    name,
                    role: crate::binding::SymbolRole::Bound,
                    binder,
                }
            } else if let Some(identity) = input.symbol_identities.iter().find(|identity| {
                identity.name == name && identity.role != crate::binding::SymbolRole::Bound
            }) {
                identity.clone()
            } else {
                crate::binding::SymbolIdentity {
                    name,
                    role: crate::binding::SymbolRole::Free,
                    binder: None,
                }
            }
        })
        .collect();
    if let Some(kind) = kind {
        output.kind = kind;
        output.completeness = match kind {
            ValueKind::SolutionSet => Some(Completeness::Unknown),
            ValueKind::FunctionFamily => Some(Completeness::Complete),
            _ => None,
        };
    }
    Ok(output)
}

/// Choose stable public names for generated arbitrary constants without
/// colliding with symbols supplied by the user.
pub fn display_arbitrary_constants(occupied: &[String], count: usize) -> Vec<String> {
    let mut next_index = if count == 1 { 0 } else { 1 };
    let mut names = Vec::with_capacity(count);
    while names.len() < count {
        let candidate = if next_index == 0 {
            "C".to_string()
        } else {
            format!("C{next_index}")
        };
        next_index += 1;
        if !occupied.contains(&candidate) {
            names.push(candidate);
        }
    }
    names
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
            } else if head.as_ref().is_some_and(|head| {
                crate::semantic_core::operator_descriptor(head).is_some_and(|descriptor| {
                    matches!(
                        descriptor.id,
                        crate::semantic_core::OperatorId::Solve
                            | crate::semantic_core::OperatorId::OdeSolve
                    )
                })
            }) {
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

pub(crate) fn matrix_shape(node: &std::rc::Rc<LispObject>) -> Option<MatrixShape> {
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
    } else if contains_approximate_number(node)
        || root_operator_id(node) == Some(crate::semantic_core::OperatorId::Approximate)
    {
        Exactness::Approximate
    } else if no_symbols {
        Exactness::Exact
    } else {
        Exactness::Symbolic
    }
}

fn root_operator_id(node: &std::rc::Rc<LispObject>) -> Option<crate::semantic_core::OperatorId> {
    let ObjectKind::Sublist(first) = &node.kind else {
        return None;
    };
    spine_refs(first)
        .next()
        .and_then(|head| head.atom_string())
        .and_then(|head| crate::semantic_core::operator_descriptor(head).map(|item| item.id))
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

        let solutions = analyze_input("Solve({x==1},{x})", "表达式").unwrap();
        assert_eq!(solutions.semantic.kind, ValueKind::SolutionSet);
        assert_eq!(solutions.semantic.completeness, Some(Completeness::Unknown));

        let ode = analyze_input("OdeSolve(y'==y+x)", "表达式").unwrap();
        assert_eq!(ode.semantic.kind, ValueKind::SolutionSet);
        assert_eq!(ode.semantic.exactness, Exactness::Unknown);

        let approximate = analyze_input("N(Pi,30)", "表达式").unwrap();
        assert_eq!(approximate.semantic.kind, ValueKind::Scalar);
        assert_eq!(approximate.semantic.exactness, Exactness::Approximate);
    }

    #[test]
    fn generated_constant_names_avoid_user_symbols() {
        assert_eq!(display_arbitrary_constants(&[], 1), ["C"]);
        assert_eq!(
            display_arbitrary_constants(&["C".into(), "C1".into()], 2),
            ["C2", "C3"]
        );
    }

    #[test]
    fn result_projection_uses_explicit_generated_symbol_roles() {
        let input = analyze_input("D(x)OdeSolve(y'==y+C179*x)", "表达式").unwrap();
        let result = project_result(
            &input.semantic,
            "C*Exp(x)+C179*x",
            &["C".into()],
            &["x"],
            None,
        )
        .unwrap();
        assert_eq!(result.bound_symbols, ["x"]);
        assert_eq!(result.symbols, ["C179"]);
        assert!(result.symbol_identities.iter().any(|identity| {
            identity.name == "C" && identity.role == crate::binding::SymbolRole::ArbitraryConstant
        }));
        assert!(result.symbol_identities.iter().any(|identity| {
            identity.name == "C179" && identity.role == crate::binding::SymbolRole::UserParameter
        }));
        assert!(!result
            .symbol_identities
            .iter()
            .any(|identity| { identity.name.starts_with('y') }));
    }
}
