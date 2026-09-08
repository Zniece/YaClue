use std::rc::Rc;

use super::{arg, arity_of, int_text_of, symbol_name_of};
use crate::env::Environment;
use crate::errors::YacasError;
use crate::evaluator::eval;
use crate::value::{copy_node, LispObject, ObjectKind};

/// Set / `:=` (See upstream: cyacas/libyacas/src/mathcommands.cpp LispSetVar): the first argument of each pair (variable name)
/// is held, the second is evaluated and assigned. Multiple arguments are processed
/// as consecutive (name, value) pairs.
pub fn cmd_set(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n < 2 || !n.is_multiple_of(2) {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let mut last = env.false_atom();
    let mut i = 0;
    while i < n {
        let name = arg(inner, i)?
            .atom_string()
            .ok_or(YacasError::InvalidArg)?
            .clone();
        let value_node = arg(inner, i + 1)?;
        let value = eval(env, value_node)?;
        let evaluated = value;
        env.set_variable(name, &evaluated, false)?;
        last = evaluated;
        i += 2;
    }
    Ok(last)
}

/// Recognize a left-hand side that is a pure tree of nested `Nth` calls with atomic
/// variable leaves, i.e. Nth(Nth(...(m,i),j),k) Same shape as the
/// `aLeftAssign` pattern of rule 10 in deffunc.rep (`IsFunction` with `Head = Nth`). The Rust side
/// special-cases this so nested indexed assignment `m[i][j] := v` writes back to the
/// root variable: upstream relies on shared chains, while the Rust rebuild model
/// must identify the root variable explicitly.
pub fn nth_assign_target(
    env: &mut Environment,
    left: &Rc<LispObject>,
) -> Option<(Rc<str>, Vec<i64>)> {
    // `left` is an unevaluated Nth call tree: peel the outer Nth layers into
    // (list, index) and recurse down to the atomic root. Indices are evaluated
    // Indices are evaluated like rule 10 does, which supports loop variables i/j.
    fn peel(env: &mut Environment, node: &Rc<LispObject>, idxs: &mut Vec<i64>) -> Option<Rc<str>> {
        // An atom (no sublist) is taken directly as the root variable; only a node with a sublist is checked for Nth.
        let sub = match node.sublist() {
            Some(s) => s,
            None => return node.atom_string().cloned(),
        };
        let head = sub.atom_string()?;
        if head.as_ref() != "Nth" {
            return Some(head.clone()); // Not an Nth call (e.g. f(x) := ): fall back to the script path.
        }
        let list_arg = sub.next.as_ref()?;
        let idx_arg = sub.next.as_ref()?.next.as_ref()?;
        let idxv_node = eval(env, idx_arg).ok()?;
        let idxv = int_text_of(&idxv_node).ok()?;
        idxs.push(idxv);
        peel(env, list_arg, idxs)
    }
    let mut idxs = Vec::new();
    let root = peel(env, left, &mut idxs)?;
    if idxs.is_empty() {
        return None; // Not an indexed assignment (plain variable).
    }
    idxs.reverse();
    Some((root, idxs))
}

/// Deep assignment: set `value` at the last position of the 1-based index chain
/// `idxs` of `m`, rebuilding `m` and writing it back to the variable slot.
/// Reproduces the observable result of the upstream shared-chain semantics; the
/// Rust path rebuilds the whole value and writes it back.
pub fn deep_assign(
    env: &mut Environment,
    root: &Rc<str>,
    idxs: &[i64],
    value: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    // Fetch the current value of the root variable.
    let cur = env
        .get_variable(root.as_ref())?
        .ok_or(YacasError::InvalidArg)?;
    // Descend through idxs[..len-1] to locate the parent container and set the last
    // index to `value`, rebuilding along the chain bottom-up: modify the deepest
    // level, then wrap back up layer by layer.
    //
    // Alias semantics (matches the upstream shared chain):
    //   Local(l,it); l:={{1,5}}; it:=l[1]; it[2]:=-7;  ->  l={{1,-7}} and it={1,-7}.
    // Every old sub-element replaced along the path must alias the new sub-element
    // (it holds the middle container, the root variable holds the top one). After
    // the recursion returns the new chain, this layer broadcasts the old element to
    // the new one; the parent layer does the same for its replaced element.
    fn descend(
        env: &mut Environment,
        node: &Rc<LispObject>,
        idxs: &[i64],
        value: &Rc<LispObject>,
    ) -> Result<Rc<LispObject>, YacasError> {
        if idxs.is_empty() {
            return Ok(copy_node(value));
        }
        let sub = node.sublist().ok_or(YacasError::NotList)?;
        let idx = idxs[0];
        // Rebuild the list (List head included): the idx-th element after the head
        // (1-based) is replaced by the descend result, the rest stay as-is.
        let mut kinds: Vec<ObjectKind> = Vec::new();
        // Keep the List head as-is.
        kinds.push(crate::value::spine_kinds(sub).next().expect("list head"));
        let mut cur = sub.next.as_ref();
        let mut i: i64 = 1;
        while let Some(e) = cur {
            let new_kind = if i == idx {
                let replaced = descend(env, e, &idxs[1..], value)?;
                let kinds: Vec<ObjectKind> = crate::value::spine_kinds(&replaced).collect();
                kinds.into_iter().next().expect("k")
            } else {
                let kinds: Vec<ObjectKind> = crate::value::spine_kinds(e).collect();
                kinds.into_iter().next().expect("ek")
            };
            kinds.push(new_kind);
            cur = e.next.as_ref();
            i += 1;
        }
        if i <= idx {
            return Err(YacasError::ListNotLongEnough);
        }
        let chain = crate::value::build_list(kinds).ok_or(YacasError::NotList)?;
        let new_container = Rc::new(LispObject {
            next: None,
            kind: ObjectKind::Sublist(chain),
        });
        // Container-level alias broadcast: this layer's node (the old container,
        // e.g. {1,5}) is rebuilt as new_container ({1,-7}); every variable slot still
        // holding the old container (or sharing its content chain) is updated.
        // Broadcasting at every recursion return keeps all intermediate containers
        // alias-consistent after a deep write.
        //
        // Note: the *replaced element* itself is deliberately NOT broadcast
        // (old_elem -> new element). Upstream semantics: l[1]:=v is a destructive
        // replace that touches only that one link on l's chain; a shared copy read
        // earlier (temp:=l[1]) is unaffected by the write and keeps the old value.
        // Broadcasting at element level would corrupt the SmallSort swap idiom
        // `temp:=l[1]; l[1]:=l[2]; l[2]:=temp`.
        env.propagate_alias(node, &new_container);
        Ok(new_container)
    }
    let new_val = descend(env, &cur, idxs, value)?;
    // Write back to the root variable and broadcast aliases: after b:=a, b[1]:=9
    // must also change a[1] (shared-chain semantics; the AssignArray path of deffunc
    // rule 10 gets this via write_back_arg0, so the deep_assign path needs it too).
    env.propagate_alias(&cur, &new_val);
    env.set_variable(root.clone(), &new_val, false)?;
    // Return value: `:=` yields the evaluated right-hand side.
    Ok(copy_node(value))
}

/// Entry point for the `:=` special case in the eval layer: if the left-hand side is
/// a nested Nth tree, perform the deep assignment; otherwise return None (script path).
pub fn try_nth_assign(
    env: &mut Environment,
    left: &Rc<LispObject>,
    right: &Rc<LispObject>,
) -> Result<Option<Rc<LispObject>>, YacasError> {
    if let Some((root, idxs)) = nth_assign_target(env, left) {
        let v = eval(env, right)?;
        let r = deep_assign(env, &root, &idxs, &v)?;
        Ok(Some(r))
    } else {
        Ok(None)
    }
}

/// Local (See upstream: cyacas/libyacas/src/mathcommands.cpp LispLocal): every argument is held (a variable name) and a local
/// variable is created (value initially empty). With no arguments, creates a local
/// named `_`.
pub fn cmd_local(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n == 0 {
        let name = env.symtab.look_up("_");
        env.new_local(name, None);
        return Ok(env.true_atom());
    }
    for i in 0..n {
        let name = arg(inner, i)?
            .atom_string()
            .ok_or(YacasError::InvalidArg)?
            .clone();
        env.new_local(name, None);
    }
    Ok(env.true_atom())
}

/// Destructive commands (the Java Destructive* family): upstream mutates the shared
/// chain in place, so every alias sees the change. The observable equivalent in the
/// Rust rebuild model is to write the rebuilt result back to the first argument's
/// variable slot (when the first argument is an atomic variable name); literal or
/// expression arguments are skipped (matching Java, where a shared reference has no
/// slot to write). Aliases under other names must observe the update too: upstream
/// makes them visible by mutating the shared chain; here the old pointer is saved
/// and, after the write-back, every variable slot pointer-equal to the old value is
/// updated to the result, emulating alias visibility.
pub fn write_back_arg0(
    env: &mut Environment,
    inner: &Rc<LispObject>,
    result: &Rc<LispObject>,
) -> Result<(), YacasError> {
    match arg(inner, 0)?.atom_string() {
        // Atomic variable name: write back to that slot and broadcast.
        Some(name) => {
            let old_ptr: Option<Rc<LispObject>> = env.get_variable(name).unwrap_or(None);
            env.set_variable(name.clone(), result, false)?;
            if let Some(old) = old_ptr {
                env.propagate_alias(&old, result);
            }
            Ok(())
        }
        // Non-atomic expression (e.g. a CachedConstant macro body
        // `DestructiveInsert(Eval(C'cache),...)`): upstream shares the content chain
        // of the evaluated copy, so the in-place mutation becomes visible through the
        // variable slot. The equivalent here: evaluate to the old value and
        // broadcast via propagate_alias — every slot sharing the old chain is updated
        // to the result.
        None => {
            let old = eval(env, arg(inner, 0)?)?;
            env.propagate_alias(&old, result);
            Ok(())
        }
    }
}

/// MacroSet — evaluate the name argument, take its atom name; evaluate the value
/// argument and assign (upstream LispMacroSetVar -> InternalSetVar(macro=true)).
/// A numeric name -> InvalidArg (upstream CheckArg(!IsNumber(...),1): `1:=2`
/// reports "In function \"MacroSet\" : Invalid argument").
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispMacroSetVar.
pub fn cmd_macro_set(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_value = eval(env, arg(inner, 0)?)?;
    if matches!(name_value.kind, ObjectKind::Number(_)) {
        return Err(YacasError::InvalidArg);
    }
    let name = name_value
        .atom_string()
        .ok_or(YacasError::InvalidArg)?
        .clone();
    let value = eval(env, arg(inner, 1)?)?;
    env.set_variable(name, &value, false)?;
    Ok(env.true_atom())
}

/// SetGlobalLazyVariable — (name, expr) defines a lazy global: reading evaluates
/// expr and caches the result (upstream LispSetGlobalLazyVariable =
/// InternalSetVar(false,true); Macro|Fixed).
/// Required by constants.rep AssignCachedConstantsN's `SetGlobalLazyVariable(@var,
/// UnList({Atom(fname)}))` — cached constants like Pi/gamma become lazy variables
/// through this; without it N(Pi,10) does not simplify.
/// See upstream: cyacas/libyacas/src/corefunctions.h LispSetGlobalLazyVariable.
pub fn cmd_set_global_lazy_variable(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_value = eval(env, arg(inner, 0)?)?;
    if matches!(name_value.kind, ObjectKind::Number(_)) {
        return Err(YacasError::InvalidArg);
    }
    let name = name_value
        .atom_string()
        .ok_or(YacasError::InvalidArg)?
        .clone();
    let value = eval(env, arg(inner, 1)?)?;
    env.set_variable(name, &value, true)?;
    Ok(env.true_atom())
}

/// Clear/MacroClear (upstream LispClearVar; Variable arity: clears multiple
/// variables). Clear does not eval its arguments (takes atom names directly);
/// MacroClear evals its arguments first, then takes the atom names.
/// Required by constants.rep AssignCachedConstantsN's `MacroClear(Atom(var))` —
/// without it cached values linger and a repeated N re-triggers the old lazy
/// recomputation re-entrantly.
/// See upstream: cyacas/libyacas/src/corefunctions.h LispClearVar.
pub fn cmd_clear(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    for i in 0..n {
        let s = arg(inner, i)?.atom_string().ok_or(YacasError::InvalidArg)?;
        env.unset_variable(s.clone())?;
    }
    Ok(env.true_atom())
}
pub fn cmd_macro_clear(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    for i in 0..n {
        let v = eval(env, arg(inner, i)?)?;
        let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
        env.unset_variable(s.clone())?;
    }
    Ok(env.true_atom())
}

/// Protect/UnProtect — symbol name -> protect/unprotect tables. Required by
/// standard.ys top-level `Protect(Nth)`, `UnProtect(%)`, `Protect(%)`.
/// Names are unquoted via symbol_name, so Protect("X") shares the same key as the
/// bare symbol X registered by DefLoad (quoting the name directly would insert
/// the quoted string into the table, which would not match the bare-name
/// UnProtect from internal_use and would leave lazily loaded file symbols
/// protected -> SymbolProtected).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispProtect / LispUnProtect.
pub fn cmd_protect(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let name = symbol_name_of(env, &name_node)?;
    env.protect(&name);
    Ok(env.true_atom())
}
pub fn cmd_unprotect(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let name = symbol_name_of(env, &name_node)?;
    env.unprotect(&name);
    Ok(env.true_atom())
}

/// IsProtected — (name) reports whether the symbol is in the protect table.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispIsProtected.
pub fn cmd_is_protected(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let name = symbol_name_of(env, &name_node)?;
    Ok(if env.is_protected(&name) {
        env.true_atom()
    } else {
        env.false_atom()
    })
}

/// IsBound — (name) reports whether the name is bound (any local frame or
/// global). Macro flag: the name is not evaluated. Required by numerical.ys's
/// `If(Not IsBound(mathExpThreshold), 500)` — without it If would hit a
/// non-boolean error.
/// See upstream: cyacas/libyacas/src/corefunctions.h LispIsBound.
pub fn cmd_is_bound(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    // Take the name from the raw argument (upstream ARGUMENT(1)->String(), no
    // eval — the question is "is this name bound").
    let s = arg(inner, 0)?.atom_string().ok_or(YacasError::InvalidArg)?;
    Ok(if env.is_bound(s) {
        env.true_atom()
    } else {
        env.false_atom()
    })
}

/// MacroLocal (like upstream LispNewLocal; Function flags: arguments are
/// evaluated first and taken as atom names for new locals; the only difference
/// from Local is the pre-evaluation of arguments). controlflow.rep's ForEach
/// macro body `MacroLocal(item)` depends on this — item is the loop variable
/// name.
/// See upstream: cyacas/libyacas/src/corefunctions.h LispNewLocal.
pub fn cmd_macro_local(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    for i in 0..n {
        let v = eval(env, arg(inner, i)?)?;
        let name = v.atom_string().ok_or(YacasError::InvalidArg)?.clone();
        env.new_local(name, None);
    }
    Ok(env.true_atom())
}

/// Variables (see upstream: cyacas/libyacas/src/mathcommands.cpp LispVars and
/// lispenvironment.cpp GlobalVariables): a List of global variable names,
/// skipping names with a '$'/'%' prefix. The upstream unordered table has a
/// non-reproducible order; sorting keeps the output deterministic.
pub fn cmd_variables(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let mut names: Vec<String> = env
        .globals
        .keys()
        .filter(|k| !k.starts_with('$') && !k.starts_with('%'))
        .map(|k| k.to_string())
        .collect();
    names.sort();
    let mut kinds: Vec<ObjectKind> = Vec::with_capacity(names.len() + 1);
    kinds.push(ObjectKind::Atom(env.symtab.look_up("List")));
    for nm in &names {
        kinds.push(ObjectKind::Atom(env.symtab.look_up(nm)));
    }
    let chain = crate::value::build_list(kinds).ok_or(YacasError::InvalidArg)?;
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(chain),
    }))
}
