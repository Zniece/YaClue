//! Non-evaluating symbol identity, lexical binding, and capture-safe rewrites.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use serde::Serialize;
use yacas_rs::value::{build_list, clone_kind, spine_refs, LispObject, ObjectKind};

use crate::engine::EngineError;
use crate::input::{validate_safe_text, with_parse_env};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolRole {
    Free,
    Bound,
    UserParameter,
    ArbitraryConstant,
    AuxiliaryCoefficient,
    InternalTemporary,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct SymbolIdentity {
    pub name: String,
    pub role: SymbolRole,
    /// Stable within one analyzed expression; absent for free identities.
    pub binder: Option<u32>,
}

impl SymbolIdentity {
    pub fn generated(name: impl Into<String>, role: SymbolRole) -> Result<Self, EngineError> {
        if matches!(
            role,
            SymbolRole::Free | SymbolRole::Bound | SymbolRole::UserParameter
        ) {
            return Err(EngineError::InvalidInput(
                "生成符号必须具有显式的内部来源身份".into(),
            ));
        }
        Ok(Self {
            name: name.into(),
            role,
            binder: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BindingAnalysis {
    pub identities: Vec<SymbolIdentity>,
    pub free_symbols: Vec<String>,
    pub bound_symbols: Vec<String>,
}

#[derive(Default)]
pub(crate) struct TreeBindingAnalysis {
    pub identities: BTreeSet<SymbolIdentity>,
    pub free_symbols: BTreeSet<String>,
    pub bound_symbols: BTreeSet<String>,
    pub function_heads: BTreeSet<String>,
    pub constants: BTreeSet<String>,
    has_binder: bool,
    next_binder: u32,
}

#[derive(Clone, Copy)]
struct BinderSpec {
    variable: usize,
    body: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BindingSignature {
    pub name: &'static str,
    pub arities: &'static [usize],
    pub variable_argument: usize,
    pub body_argument_from_end: usize,
}

const DERIVATIVE_ARITIES: &[usize] = &[2, 3];
const INTEGRAL_ARITIES: &[usize] = &[2, 4];
const LIMIT_ARITIES: &[usize] = &[3, 4];
const SUM_ARITIES: &[usize] = &[4];
const DEFINED_INTEGRAL_ARITIES: &[usize] = &[4, 5];

pub const BINDING_SIGNATURES: &[BindingSignature] = &[
    BindingSignature {
        name: "D",
        arities: DERIVATIVE_ARITIES,
        variable_argument: 0,
        body_argument_from_end: 0,
    },
    BindingSignature {
        name: "Deriv",
        arities: DERIVATIVE_ARITIES,
        variable_argument: 0,
        body_argument_from_end: 0,
    },
    BindingSignature {
        name: "Integrate",
        arities: INTEGRAL_ARITIES,
        variable_argument: 0,
        body_argument_from_end: 0,
    },
    BindingSignature {
        name: "Limit",
        arities: LIMIT_ARITIES,
        variable_argument: 0,
        body_argument_from_end: 0,
    },
    BindingSignature {
        name: "Sum",
        arities: SUM_ARITIES,
        variable_argument: 0,
        body_argument_from_end: 0,
    },
    BindingSignature {
        name: "ImproperIntegral",
        arities: DEFINED_INTEGRAL_ARITIES,
        variable_argument: 0,
        body_argument_from_end: 0,
    },
    BindingSignature {
        name: "PrincipalValueIntegral",
        arities: DEFINED_INTEGRAL_ARITIES,
        variable_argument: 0,
        body_argument_from_end: 0,
    },
];

fn binder_spec(head: &str, argument_count: usize) -> Option<BinderSpec> {
    let signature = BINDING_SIGNATURES
        .iter()
        .find(|signature| signature.name == head && signature.arities.contains(&argument_count))?;
    Some(BinderSpec {
        variable: signature.variable_argument,
        body: argument_count.checked_sub(signature.body_argument_from_end + 1)?,
    })
}

pub fn analyze(input: &str) -> Result<BindingAnalysis, EngineError> {
    validate_safe_text(input, "表达式")?;
    with_parse_env(|env| {
        let tree = yacas_rs::parser::parse_expression(env, &format!("{input};"))
            .map_err(|error| EngineError::InvalidInput(format!("表达式语法错误: {error:?}")))?
            .ok_or_else(|| EngineError::InvalidInput("表达式为空".into()))?;
        let analysis = analyze_tree(&tree);
        Ok(BindingAnalysis {
            identities: analysis.identities.into_iter().collect(),
            free_symbols: analysis.free_symbols.into_iter().collect(),
            bound_symbols: analysis.bound_symbols.into_iter().collect(),
        })
    })
}

pub(crate) fn analyze_tree(tree: &Rc<LispObject>) -> TreeBindingAnalysis {
    let mut result = TreeBindingAnalysis::default();
    visit(tree, &mut Vec::new(), &mut result);
    if result.has_binder {
        result.identities = result
            .identities
            .into_iter()
            .map(|mut identity| {
                if identity.role == SymbolRole::Free {
                    identity.role = SymbolRole::UserParameter;
                }
                identity
            })
            .collect();
    }
    result
}

fn visit(node: &Rc<LispObject>, scopes: &mut Vec<(String, u32)>, result: &mut TreeBindingAnalysis) {
    match &node.kind {
        ObjectKind::Atom(name) => record_atom(name, scopes, result),
        ObjectKind::Number(_) | ObjectKind::Generic(_) => {}
        ObjectKind::Sublist(first) => {
            let nodes: Vec<_> = spine_refs(first).collect();
            let Some(head) = nodes.first().and_then(|node| node.atom_string()) else {
                for node in nodes {
                    visit(node, scopes, result);
                }
                return;
            };
            result.function_heads.insert(head.to_string());
            let arguments = &nodes[1..];
            let spec = binder_spec(head, arguments.len());
            for (index, argument) in arguments.iter().enumerate() {
                if spec.is_some_and(|spec| index == spec.variable) {
                    continue;
                }
                if let Some(spec) = spec.filter(|spec| index == spec.body) {
                    if let Some(variable) = arguments[spec.variable].atom_string() {
                        result.has_binder = true;
                        let binder = result.next_binder;
                        result.next_binder += 1;
                        result.bound_symbols.insert(variable.to_string());
                        result.identities.insert(SymbolIdentity {
                            name: variable.to_string(),
                            role: SymbolRole::Bound,
                            binder: Some(binder),
                        });
                        scopes.push((variable.to_string(), binder));
                        visit(argument, scopes, result);
                        scopes.pop();
                        continue;
                    }
                }
                visit(argument, scopes, result);
            }
        }
    }
}

fn record_atom(name: &str, scopes: &[(String, u32)], result: &mut TreeBindingAnalysis) {
    if name.starts_with('"') {
        return;
    }
    if yacas_rs::standard::is_constant_symbol(name) {
        result.constants.insert(name.into());
    } else if let Some((_, binder)) = scopes.iter().rev().find(|(bound, _)| bound == name) {
        result.bound_symbols.insert(name.into());
        result.identities.insert(SymbolIdentity {
            name: name.into(),
            role: SymbolRole::Bound,
            binder: Some(*binder),
        });
    } else {
        result.free_symbols.insert(name.into());
        result.identities.insert(SymbolIdentity {
            name: name.into(),
            role: SymbolRole::Free,
            binder: None,
        });
    }
}

pub fn alpha_equivalent(left: &str, right: &str) -> Result<bool, EngineError> {
    Ok(canonical(left)? == canonical(right)?)
}

/// Compare parsed trees without evaluation or alpha-renaming.
pub fn structurally_equal(left: &str, right: &str) -> Result<bool, EngineError> {
    Ok(structural(left)? == structural(right)?)
}

fn structural(input: &str) -> Result<String, EngineError> {
    validate_safe_text(input, "表达式")?;
    with_parse_env(|env| {
        let tree = yacas_rs::parser::parse_expression(env, &format!("{input};"))
            .map_err(|error| EngineError::InvalidInput(format!("表达式语法错误: {error:?}")))?
            .ok_or_else(|| EngineError::InvalidInput("表达式为空".into()))?;
        Ok(structural_node(&tree))
    })
}

fn structural_node(node: &Rc<LispObject>) -> String {
    match &node.kind {
        ObjectKind::Atom(name) => format!("A{}:{name}", name.len()),
        ObjectKind::Number(number) => {
            let value = number.string();
            format!("N{}:{value}", value.len())
        }
        ObjectKind::Generic(generic) => format!("G{}", generic.type_name()),
        ObjectKind::Sublist(first) => {
            let children = spine_refs(first).map(structural_node).collect::<String>();
            format!("L{}:[{children}]", spine_refs(first).count())
        }
    }
}

fn canonical(input: &str) -> Result<String, EngineError> {
    validate_safe_text(input, "表达式")?;
    with_parse_env(|env| {
        let tree = yacas_rs::parser::parse_expression(env, &format!("{input};"))
            .map_err(|error| EngineError::InvalidInput(format!("表达式语法错误: {error:?}")))?
            .ok_or_else(|| EngineError::InvalidInput("表达式为空".into()))?;
        let mut next = 0_u32;
        Ok(canonical_node(&tree, &mut Vec::new(), &mut next))
    })
}

fn canonical_node(
    node: &Rc<LispObject>,
    scopes: &mut Vec<(String, u32)>,
    next: &mut u32,
) -> String {
    match &node.kind {
        ObjectKind::Atom(name) => scopes
            .iter()
            .rev()
            .find(|(bound, _)| bound == name.as_ref())
            .map(|(_, binder)| format!("#{binder}"))
            .unwrap_or_else(|| format!("${name}")),
        ObjectKind::Number(number) => format!("%{}", number.string()),
        ObjectKind::Generic(generic) => format!("@{}", generic.type_name()),
        ObjectKind::Sublist(first) => {
            let nodes: Vec<_> = spine_refs(first).collect();
            let head = nodes.first().and_then(|node| node.atom_string());
            let arguments = &nodes[1..];
            let spec = head.and_then(|head| binder_spec(head, arguments.len()));
            let mut parts = vec![head.map_or_else(|| "?".into(), ToString::to_string)];
            let mut pending_binder = None;
            for (index, argument) in arguments.iter().enumerate() {
                if spec.is_some_and(|spec| index == spec.variable) {
                    let binder = *next;
                    *next += 1;
                    parts.push(format!("!{binder}"));
                    if let Some(variable) = argument.atom_string() {
                        pending_binder = Some((variable.to_string(), binder));
                    }
                    continue;
                }
                let body_scope = spec.is_some_and(|spec| index == spec.body);
                if body_scope {
                    if let Some(binder) = pending_binder.take() {
                        scopes.push(binder);
                    }
                }
                parts.push(canonical_node(argument, scopes, next));
                if body_scope && arguments[spec.unwrap().variable].atom_string().is_some() {
                    scopes.pop();
                }
            }
            format!("({})", parts.join(" "))
        }
    }
}

pub fn alpha_rename(input: &str, from: &str, to: &str) -> Result<String, EngineError> {
    crate::input::validate_symbol(from, "原绑定变量")?;
    crate::input::validate_symbol(to, "新绑定变量")?;
    rewrite(input, |tree, env| {
        if rename_would_capture(tree, from, to) {
            return Err(EngineError::InvalidInput(format!(
                "新名称 {to} 会捕获自由符号"
            )));
        }
        Ok(rename_bound(
            tree,
            from,
            to,
            &mut Vec::new(),
            &mut env.symtab,
        ))
    })
}

fn rename_would_capture(node: &Rc<LispObject>, from: &str, to: &str) -> bool {
    let ObjectKind::Sublist(first) = &node.kind else {
        return false;
    };
    let nodes: Vec<_> = spine_refs(first).collect();
    let Some(head) = nodes.first().and_then(|node| node.atom_string()) else {
        return nodes
            .iter()
            .any(|node| rename_would_capture(node, from, to));
    };
    let arguments = &nodes[1..];
    if let Some(spec) = binder_spec(head, arguments.len()) {
        if arguments[spec.variable]
            .atom_string()
            .is_some_and(|name| name.as_ref() == from)
            && analyze_tree(arguments[spec.body]).free_symbols.contains(to)
        {
            return true;
        }
    }
    arguments
        .iter()
        .any(|argument| rename_would_capture(argument, from, to))
}

pub fn substitute_free(
    input: &str,
    symbol: &str,
    replacement: &str,
) -> Result<String, EngineError> {
    crate::input::validate_symbol(symbol, "替换符号")?;
    validate_safe_text(replacement, "替换表达式")?;
    with_parse_env(|env| {
        let tree = parse(env, input)?;
        let replacement_tree = parse(env, replacement)?;
        let replacement_free = analyze_tree(&replacement_tree).free_symbols;
        let mut occupied = analyze_tree(&tree).free_symbols;
        occupied.extend(analyze_tree(&tree).bound_symbols);
        occupied.extend(replacement_free.iter().cloned());
        let rewritten = substitute_node(
            &tree,
            symbol,
            &replacement_tree,
            &replacement_free,
            &mut occupied,
            &mut Vec::new(),
            &mut env.symtab,
        );
        Ok(yacas_rs::printer::infix_print(env, &rewritten))
    })
}

fn rewrite(
    input: &str,
    transform: impl FnOnce(
        &Rc<LispObject>,
        &mut yacas_rs::env::Environment,
    ) -> Result<Rc<LispObject>, EngineError>,
) -> Result<String, EngineError> {
    validate_safe_text(input, "表达式")?;
    with_parse_env(|env| {
        let tree = parse(env, input)?;
        let rewritten = transform(&tree, env)?;
        Ok(yacas_rs::printer::infix_print(env, &rewritten))
    })
}

fn parse(env: &mut yacas_rs::env::Environment, input: &str) -> Result<Rc<LispObject>, EngineError> {
    yacas_rs::parser::parse_expression(env, &format!("{input};"))
        .map_err(|error| EngineError::InvalidInput(format!("表达式语法错误: {error:?}")))?
        .ok_or_else(|| EngineError::InvalidInput("表达式为空".into()))
}

fn rename_bound(
    node: &Rc<LispObject>,
    from: &str,
    to: &str,
    renames: &mut Vec<BTreeMap<String, String>>,
    symbols: &mut yacas_rs::symtab::SymbolTable,
) -> Rc<LispObject> {
    match &node.kind {
        ObjectKind::Atom(name) => renames
            .iter()
            .rev()
            .find_map(|scope| scope.get(name.as_ref()))
            .map_or_else(|| deep_clone(node), |name| atom(symbols, name)),
        ObjectKind::Sublist(first) => {
            let nodes: Vec<_> = spine_refs(first).collect();
            let Some(head) = nodes.first().and_then(|node| node.atom_string()) else {
                return deep_clone(node);
            };
            let arguments = &nodes[1..];
            let spec = binder_spec(head, arguments.len());
            let mut output = Vec::with_capacity(arguments.len());
            let mut pending_scope = None;
            for (index, argument) in arguments.iter().enumerate() {
                if spec.is_some_and(|spec| index == spec.variable) {
                    let old = argument.atom_string().map(ToString::to_string);
                    let new = old.as_deref().filter(|name| *name == from).map(|_| to);
                    output
                        .push(new.map_or_else(|| deep_clone(argument), |name| atom(symbols, name)));
                    let mut scope = BTreeMap::new();
                    if let (Some(old), Some(new)) = (old, new) {
                        scope.insert(old, new.into());
                    }
                    pending_scope = Some(scope);
                } else if spec.is_some_and(|spec| index == spec.body) {
                    renames.push(pending_scope.take().unwrap_or_default());
                    output.push(rename_bound(argument, from, to, renames, symbols));
                    renames.pop();
                } else {
                    output.push(rename_bound(argument, from, to, renames, symbols));
                }
            }
            let mut kinds = vec![ObjectKind::Atom(symbols.look_up(head))];
            kinds.extend(output.into_iter().map(|item| clone_kind(&item.kind)));
            LispObject::new(ObjectKind::Sublist(build_list(kinds).unwrap()))
        }
        _ => deep_clone(node),
    }
}

fn substitute_node(
    node: &Rc<LispObject>,
    target: &str,
    replacement: &Rc<LispObject>,
    replacement_free: &BTreeSet<String>,
    occupied: &mut BTreeSet<String>,
    bound: &mut Vec<String>,
    symbols: &mut yacas_rs::symtab::SymbolTable,
) -> Rc<LispObject> {
    match &node.kind {
        ObjectKind::Atom(name)
            if name.as_ref() == target && !bound.iter().any(|item| item == target) =>
        {
            deep_clone(replacement)
        }
        ObjectKind::Sublist(first) => {
            let nodes: Vec<_> = spine_refs(first).collect();
            let Some(head) = nodes.first().and_then(|node| node.atom_string()) else {
                return deep_clone(node);
            };
            let arguments = &nodes[1..];
            let spec = binder_spec(head, arguments.len());
            let mut output = Vec::with_capacity(arguments.len());
            let mut active_name = None;
            for (index, argument) in arguments.iter().enumerate() {
                if spec.is_some_and(|spec| index == spec.variable) {
                    let original = argument.atom_string().map(ToString::to_string);
                    let renamed = original.as_deref().and_then(|name| {
                        (name != target && replacement_free.contains(name))
                            .then(|| fresh_name(name, occupied))
                    });
                    active_name = original.clone().map(|name| (name, renamed.clone()));
                    output.push(
                        renamed
                            .as_deref()
                            .map_or_else(|| deep_clone(argument), |name| atom(symbols, name)),
                    );
                    continue;
                }
                if spec.is_some_and(|spec| index == spec.body) {
                    if let Some((original, renamed)) = &active_name {
                        let prepared = renamed.as_deref().map_or_else(
                            || deep_clone(argument),
                            |new| {
                                let mut scope = BTreeMap::new();
                                scope.insert(original.clone(), new.into());
                                rename_bound(argument, original, new, &mut vec![scope], symbols)
                            },
                        );
                        bound.push(renamed.clone().unwrap_or_else(|| original.clone()));
                        output.push(substitute_node(
                            &prepared,
                            target,
                            replacement,
                            replacement_free,
                            occupied,
                            bound,
                            symbols,
                        ));
                        bound.pop();
                        continue;
                    }
                }
                output.push(substitute_node(
                    argument,
                    target,
                    replacement,
                    replacement_free,
                    occupied,
                    bound,
                    symbols,
                ));
            }
            let mut kinds = vec![ObjectKind::Atom(symbols.look_up(head))];
            kinds.extend(output.into_iter().map(|item| clone_kind(&item.kind)));
            LispObject::new(ObjectKind::Sublist(build_list(kinds).unwrap()))
        }
        _ => deep_clone(node),
    }
}

fn fresh_name(base: &str, occupied: &mut BTreeSet<String>) -> String {
    for index in 1_u32.. {
        let candidate = format!("{base}{index}");
        if occupied.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!()
}

fn atom(symbols: &mut yacas_rs::symtab::SymbolTable, name: &str) -> Rc<LispObject> {
    LispObject::atom(symbols.look_up(name))
}

fn deep_clone(node: &Rc<LispObject>) -> Rc<LispObject> {
    match &node.kind {
        ObjectKind::Sublist(first) => {
            let kinds = spine_refs(first)
                .map(|item| clone_kind(&deep_clone(item).kind))
                .collect();
            LispObject::new(ObjectKind::Sublist(build_list(kinds).unwrap()))
        }
        _ => LispObject::new(clone_kind(&node.kind)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separates_bound_variables_and_free_parameters() {
        let result = analyze("Integrate(t,0,X)Sin(t^2)").unwrap();
        assert_eq!(result.free_symbols, ["X"]);
        assert_eq!(result.bound_symbols, ["t"]);
        assert!(result.identities.iter().any(|identity| {
            identity.name == "X" && identity.role == SymbolRole::UserParameter
        }));
        let bound = analyze("Integrate(t,t,X)t").unwrap();
        assert!(bound.free_symbols.contains(&"t".into()));
        assert!(bound.bound_symbols.contains(&"t".into()));
    }

    #[test]
    fn declares_the_initial_bodied_operator_set() {
        for name in [
            "D",
            "Integrate",
            "Limit",
            "Sum",
            "ImproperIntegral",
            "PrincipalValueIntegral",
        ] {
            assert!(BINDING_SIGNATURES
                .iter()
                .any(|signature| signature.name == name));
        }
    }

    #[test]
    fn alpha_equivalence_respects_free_symbols_and_nested_shadowing() {
        assert!(structurally_equal("x + 1", "x+1").unwrap());
        assert!(
            !structurally_equal("Integrate(t,0,X)Sin(t^2)", "Integrate(u,0,X)Sin(u^2)").unwrap()
        );
        assert!(alpha_equivalent("Integrate(t,0,X)Sin(t^2)", "Integrate(u,0,X)Sin(u^2)").unwrap());
        assert!(!alpha_equivalent("Integrate(t,0,X)Sin(t^2)", "Integrate(u,0,Y)Sin(u^2)").unwrap());
        assert!(alpha_equivalent(
            "Integrate(t,0,X)(t+Integrate(t,0,1)t)",
            "Integrate(u,0,X)(u+Integrate(v,0,1)v)"
        )
        .unwrap());
    }

    #[test]
    fn alpha_rename_rejects_capture() {
        let renamed = alpha_rename("Integrate(t,0,X)(t+a)", "t", "u").unwrap();
        assert!(alpha_equivalent(&renamed, "Integrate(u,0,X)(u+a)").unwrap());
        assert!(alpha_rename("Integrate(t,0,X)(t+u)", "t", "u").is_err());
        assert!(alpha_rename("u+Integrate(t,0,X)t", "t", "u").is_ok());
    }

    #[test]
    fn substitution_is_capture_avoiding() {
        let result = substitute_free("Integrate(t,0,X)(x+t)", "x", "t").unwrap();
        let analysis = analyze(&result).unwrap();
        assert!(analysis.free_symbols.contains(&"t".into()));
        assert!(analysis.bound_symbols.iter().any(|name| name != "t"));
        assert!(alpha_equivalent(&result, "Integrate(u,0,X)(t+u)").unwrap());
    }

    #[test]
    fn user_spelling_is_not_inferred_as_an_internal_role() {
        let result = analyze("C311+x").unwrap();
        assert!(result.identities.iter().all(|identity| {
            matches!(identity.role, SymbolRole::Free | SymbolRole::UserParameter)
        }));
        assert!(SymbolIdentity::generated("k", SymbolRole::InternalTemporary).is_ok());
    }
}
