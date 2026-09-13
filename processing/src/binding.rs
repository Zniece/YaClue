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

fn binder_specs(head: &str, argument_count: usize) -> Vec<BinderSpec> {
    let Some(descriptor) = crate::semantic_core::operator_descriptor(head)
        .filter(|descriptor| descriptor.arities.contains(&argument_count))
    else {
        return Vec::new();
    };
    let Some(signature) = descriptor
        .slot_signatures
        .iter()
        .find(|signature| signature.arity == argument_count)
    else {
        return Vec::new();
    };
    descriptor
        .binders
        .iter()
        .filter_map(|binder| {
            if signature.requirements.get(binder.binder_argument)
                != Some(&crate::semantic_core::Requirement::Variable)
            {
                return None;
            }
            let body = match binder.scope_argument {
                crate::semantic_core::ScopeArgument::First => 0,
                crate::semantic_core::ScopeArgument::Last => argument_count.checked_sub(1)?,
                crate::semantic_core::ScopeArgument::Index(index) => index,
            };
            (binder.binder_argument < argument_count && body < argument_count).then_some(
                BinderSpec {
                    variable: binder.binder_argument,
                    body,
                },
            )
        })
        .collect()
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
            let specs = binder_specs(head, arguments.len());
            let mut binders = BTreeMap::new();
            for spec in &specs {
                if binders.contains_key(&spec.variable) {
                    continue;
                }
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
                    binders.insert(spec.variable, (variable.to_string(), binder));
                }
            }
            for (index, argument) in arguments.iter().enumerate() {
                if binders.contains_key(&index) {
                    continue;
                }
                let scoped: Vec<_> = specs
                    .iter()
                    .filter(|spec| spec.body == index)
                    .filter_map(|spec| binders.get(&spec.variable).cloned())
                    .collect();
                scopes.extend(scoped.iter().cloned());
                visit(argument, scopes, result);
                for _ in &scoped {
                    scopes.pop();
                }
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
            let specs = head.map_or_else(Vec::new, |head| binder_specs(head, arguments.len()));
            let mut parts = vec![head.map_or_else(|| "?".into(), ToString::to_string)];
            let mut binders = BTreeMap::new();
            for spec in &specs {
                if binders.contains_key(&spec.variable) {
                    continue;
                }
                if let Some(variable) = arguments[spec.variable].atom_string() {
                    let binder = *next;
                    *next += 1;
                    binders.insert(spec.variable, (variable.to_string(), binder));
                }
            }
            for (index, argument) in arguments.iter().enumerate() {
                if let Some((_, binder)) = binders.get(&index) {
                    parts.push(format!("!{binder}"));
                    continue;
                }
                let scoped: Vec<_> = specs
                    .iter()
                    .filter(|spec| spec.body == index)
                    .filter_map(|spec| binders.get(&spec.variable).cloned())
                    .collect();
                scopes.extend(scoped.iter().cloned());
                parts.push(canonical_node(argument, scopes, next));
                for _ in &scoped {
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
    for spec in binder_specs(head, arguments.len()) {
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

pub(crate) fn substitute_free_objects(
    input: &crate::semantic_core::MathematicalObject,
    symbol: &str,
    replacement: &crate::semantic_core::MathematicalObject,
) -> Result<Rc<LispObject>, EngineError> {
    crate::input::validate_symbol(symbol, "替换符号")?;
    with_parse_env(|env| {
        let tree = input.raw_expression();
        let replacement_tree = replacement.raw_expression();
        let replacement_free = analyze_tree(&replacement_tree).free_symbols;
        let analyzed = analyze_tree(&tree);
        let mut occupied = analyzed.free_symbols;
        occupied.extend(analyzed.bound_symbols);
        occupied.extend(replacement_free.iter().cloned());
        Ok(substitute_node(
            &tree,
            symbol,
            &replacement_tree,
            &replacement_free,
            &mut occupied,
            &mut Vec::new(),
            &mut env.symtab,
        ))
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
            let specs = binder_specs(head, arguments.len());
            let mut binder_renames = BTreeMap::new();
            for spec in &specs {
                binder_renames.entry(spec.variable).or_insert_with(|| {
                    let old = arguments[spec.variable]
                        .atom_string()
                        .map(ToString::to_string);
                    let new: Option<String> = old
                        .as_deref()
                        .filter(|name| *name == from)
                        .map(|_| to.into());
                    (old, new)
                });
            }
            let mut output = Vec::with_capacity(arguments.len());
            for (index, argument) in arguments.iter().enumerate() {
                if let Some((_, new)) = binder_renames.get(&index) {
                    output.push(
                        new.as_deref()
                            .map_or_else(|| deep_clone(argument), |name| atom(symbols, name)),
                    );
                } else {
                    let mut scope = BTreeMap::new();
                    for spec in specs.iter().filter(|spec| spec.body == index) {
                        if let Some((Some(old), Some(new))) = binder_renames.get(&spec.variable) {
                            scope.insert(old.clone(), new.clone());
                        }
                    }
                    renames.push(scope);
                    output.push(rename_bound(argument, from, to, renames, symbols));
                    renames.pop();
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
            let specs = binder_specs(head, arguments.len());
            let mut binder_names = BTreeMap::new();
            for spec in &specs {
                binder_names.entry(spec.variable).or_insert_with(|| {
                    let original = arguments[spec.variable]
                        .atom_string()
                        .map(ToString::to_string);
                    let renamed = original.as_deref().and_then(|name| {
                        (name != target && replacement_free.contains(name))
                            .then(|| fresh_name(name, occupied))
                    });
                    (original, renamed)
                });
            }
            let mut output = Vec::with_capacity(arguments.len());
            for (index, argument) in arguments.iter().enumerate() {
                if let Some((_, renamed)) = binder_names.get(&index) {
                    output.push(
                        renamed
                            .as_deref()
                            .map_or_else(|| deep_clone(argument), |name| atom(symbols, name)),
                    );
                    continue;
                }
                let scoped: Vec<_> = specs
                    .iter()
                    .filter(|spec| spec.body == index)
                    .filter_map(|spec| binder_names.get(&spec.variable))
                    .filter_map(|(original, renamed)| {
                        original
                            .as_ref()
                            .map(|original| (original.clone(), renamed.clone()))
                    })
                    .collect();
                let mut prepared = deep_clone(argument);
                for (original, renamed) in &scoped {
                    if let Some(new) = renamed {
                        let mut scope = BTreeMap::new();
                        scope.insert(original.clone(), new.clone());
                        prepared =
                            rename_bound(&prepared, original, new, &mut vec![scope], symbols);
                    }
                }
                bound.extend(scoped.iter().map(|(original, renamed)| {
                    renamed.clone().unwrap_or_else(|| original.clone())
                }));
                output.push(substitute_node(
                    &prepared,
                    target,
                    replacement,
                    replacement_free,
                    occupied,
                    bound,
                    symbols,
                ));
                bound.truncate(bound.len() - scoped.len());
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
            assert!(crate::semantic_core::operator_descriptor(name)
                .unwrap()
                .arities
                .iter()
                .any(|arity| !binder_specs(name, *arity).is_empty()));
        }
    }

    #[test]
    fn improper_integral_binds_the_variable_in_the_leading_integrand() {
        let result = analyze("ImproperIntegral(t^(a-1)*Exp(-t),t,0,Infinity)").unwrap();
        assert_eq!(result.free_symbols, ["a"]);
        assert_eq!(result.bound_symbols, ["t"]);
    }

    #[test]
    fn multi_binder_operators_scope_every_declared_variable() {
        let double = analyze("DoubleIntegral(x*y+a,x,0,1,y,0,b)").unwrap();
        assert_eq!(double.free_symbols, ["a", "b"]);
        assert_eq!(double.bound_symbols, ["x", "y"]);
        assert!(alpha_equivalent(
            "DoubleIntegral(x*y+a,x,0,1,y,0,b)",
            "DoubleIntegral(u*v+a,u,0,1,v,0,b)"
        )
        .unwrap());

        let extrema = analyze("Extrema(x^2+y^2+c,x,y)").unwrap();
        assert_eq!(extrema.free_symbols, ["c"]);
        assert_eq!(extrema.bound_symbols, ["x", "y"]);

        let lagrange = analyze("Lagrange(x^2+y^2+a,x+y-b,x,y)").unwrap();
        assert_eq!(lagrange.free_symbols, ["a", "b"]);
        assert_eq!(lagrange.bound_symbols, ["x", "y"]);
        assert!(alpha_equivalent(
            "Lagrange(x^2+y^2+a,x+y-b,x,y)",
            "Lagrange(u^2+v^2+a,u+v-b,u,v)"
        )
        .unwrap());
    }

    #[test]
    fn multi_binder_rewrites_are_capture_safe_in_every_scope() {
        let renamed = alpha_rename("Lagrange(x+y,x-y,x,y)", "x", "u").unwrap();
        assert!(alpha_equivalent(&renamed, "Lagrange(u+y,u-y,u,y)").unwrap());
        assert!(alpha_rename("Lagrange(x+y+u,x-y,x,y)", "x", "u").is_err());

        let substituted = substitute_free("DoubleIntegral(x*y+a,x,0,1,y,0,b)", "a", "x+y").unwrap();
        let analysis = analyze(&substituted).unwrap();
        assert!(analysis.free_symbols.contains(&"x".into()));
        assert!(analysis.free_symbols.contains(&"y".into()));
        assert_eq!(analysis.bound_symbols.len(), 2);
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
