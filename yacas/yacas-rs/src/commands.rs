//! Core commands XXX//! port; first batch: everything needed to close the `eval` loop).
//!
//! Calling convention: `fn(&mut Environment, inner chain)` where `inner = [head, args...]`
//! (as when BasicEvaluator invokes core commands). Each command decides whether to
//! Each command decides whether to evaluate or hold its arguments (the HoldArg
//! semantics of the script level).
//!
//! This batch covers the `<--`/`#`/`DefinePattern` script rule chain and basic control
//! flow: `:=`/Set, Local, If, Not, Equals, Head, Tail, Length, Listify, String, Type,
//! RuleBaseDefined, DefLoadFunction, MacroRuleBase, MacroRulePattern, Pattern'Create,
//! arg, Atom, ConcatStrings, While, True/False (constant atoms).
//! Note: MakeVector is a script-level function (its patterns.rep/code.ys definition is
//! self-contained, with 1-based arg1..argn), so it is not in the builtin table here.

use std::rc::Rc;

use crate::assumptions::Assumption;
use crate::env::Environment;
use crate::errors::YacasError;
use crate::evaluator::eval;
use crate::operators::Operator;
use crate::standard::{internal_equals, is_false, is_true};
use crate::value::{copy_node, spine_kinds, spine_refs, LispObject, ObjectKind};

// Fetch argument i (the command's ARGUMENT(i)): the i-th argument of the inner
// chain; an error if absent.
fn arg(inner: &Rc<LispObject>, i: usize) -> Result<&Rc<LispObject>, YacasError> {
    spine_refs(inner).nth(i + 1).ok_or(YacasError::WrongNumberOfArgs)
}

// Arity: InternalListLength(head) - 1.
fn arity_of(inner: &Rc<LispObject>) -> usize {
    crate::standard::internal_list_length(inner) - 1
}

/// Set / `:=` (See upstream: cyacas/libyacas/src/mathcommands.cpp LispSetVar): the first argument of each pair (variable name)
/// is held, the second is evaluated and assigned. Multiple arguments are processed
/// as consecutive (name, value) pairs.
pub fn cmd_set(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n < 2 || !n.is_multiple_of(2) {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let mut last = env.false_atom();
    let mut i = 0;
    while i < n {
        let name = arg(inner, i)?.atom_string().ok_or(YacasError::InvalidArg)?.clone();
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
pub fn nth_assign_target(env: &mut Environment, left: &Rc<LispObject>) -> Option<(Rc<str>, Vec<i64>)> {
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
    let cur = env.get_variable(root.as_ref())?.ok_or(YacasError::InvalidArg)?;
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
        let new_container = Rc::new(LispObject { next: None, kind: ObjectKind::Sublist(chain) });
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
pub fn cmd_local(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n == 0 {
        let name = env.symtab.look_up("_");
        env.new_local(name, None);
        return Ok(env.true_atom());
    }
    for i in 0..n {
        let name = arg(inner, i)?.atom_string().ok_or(YacasError::InvalidArg)?.clone();
        env.new_local(name, None);
    }
    Ok(env.true_atom())
}

/// If (See upstream: cyacas/libyacas/src/mathcommands.cpp LispIf): evaluate pred; True -> then-branch; False -> else-branch
/// (False when the else-branch is absent).
pub fn cmd_if(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n < 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let pred = eval(env, arg(inner, 0)?)?;
    if is_true(env, &pred) {
        return eval(env, arg(inner, 1)?);
    }
    if n >= 3 {
        return eval(env, arg(inner, 2)?);
    }
    Ok(env.false_atom())
}

/// Not (See upstream: cyacas/libyacas/src/mathcommands.cpp InternalNot): True -> False, False -> True; a non-boolean argument
/// is an error.
pub fn cmd_not(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    if is_true(env, &v) {
        Ok(env.false_atom())
    } else if is_false(env, &v) {
        Ok(env.true_atom())
    } else {
        Err(YacasError::InvalidArg)
    }
}

/// Equals (See upstream: cyacas/libyacas/src/mathcommands.cpp InternalEquals): evaluate both arguments, then deep equality.
pub fn cmd_equals(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let a = eval(env, arg(inner, 0)?)?;
    let b = eval(env, arg(inner, 1)?)?;
    if internal_equals(env, &a, &b) {
        Ok(env.true_atom())
    } else {
        Ok(env.false_atom())
    }
}

/// Head (See upstream: cyacas/libyacas/src/mathcommands.cpp LispHead = InternalNth(ARG, 1)): step one link into the
/// inner chain and return the first element. Upstream behavior: Head({a,b,c}) -> a.
pub fn cmd_head(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    let sub = v.sublist().ok_or(YacasError::NotList)?;
    crate::standard::internal_nth(sub, 1)
}

/// Tail (See upstream: cyacas/libyacas/src/mathcommands.cpp LispTail): returns (List rest...) — everything past the head,
/// wrapped in a List head.
pub fn cmd_tail(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    let sub = v.sublist().ok_or(YacasError::NotList)?;
    // Upstream behavior (LispTail): InternalTail twice (drop the head, then the first
    // element), then wrap the remainder in a List head. The remaining chain is
    // shared, not copied (copy_node would drop the next links).
    let tail1 = sub.next.as_ref().ok_or(YacasError::InvalidArg)?;
    let list_sym = env.symtab.look_up("List");
    // Single-element list (`Tail({aa})` -> `{}`): after dropping the head and the
    // first element nothing remains, so return an empty list rather than an error.
    if tail1.next.is_none() {
        return Ok(Rc::new(LispObject {
            next: None,
            kind: ObjectKind::Sublist(LispObject::atom(list_sym)),
        }));
    }
    let mut head = LispObject::atom(list_sym);
    if let Some(m) = Rc::get_mut(&mut head) {
        m.next = tail1.next.clone();
    }
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(head),
    }))
}

/// Length (See upstream: cyacas/libyacas/src/mathcommands.cpp LispLength): returns the length as an atomic number. Content
/// chains come in two shapes: head-bearing chains (List elem...) include the List
/// head; headless chains (FlatCopy/Insert results) start directly at the elements.
/// Counting uniformly from "the first element after the content-chain head" (the
/// same pattern as cmd_insert) is required: subtracting only a List head would
/// over-count headless chains by one, so Length(FlatCopy({a,b})) would be 1 instead
/// of 2 and DestructiveAppend's DestructiveInsert(list, Length(list)+1, ...) would
/// insert in the middle instead of appending.
pub fn cmd_length(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    // For any sublist, count the content chain minus its head (i.e. the argument
    // count). Upstream: InternalListLength((*subList)->Nixed()) — SubList() yields
    // the content chain (a List literal head, or a function name such as Min), and
    // Nixed() skips it before counting. Function expressions (Min(l1,l2,l3,...)) must
    // be treated the same as List-headed chains, otherwise the ellipsis predicate
    // MathNth(oper, Length(oper)) used by Function(...) goes out of bounds.
    let v = eval(env, arg(inner, 0)?)?;
    // Upstream behavior (LispLength): sublist -> content chain length minus 1; string ->
    // raw length minus 2 (drop the quotes); Array/Association generics -> their size.
    let len = if let Some(sub) = v.sublist() {
        crate::standard::internal_list_length(sub).saturating_sub(1)
    } else if let Some(s) = v.atom_string() {
        if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
            s.len() - 2
        } else {
            return Err(YacasError::NotList);
        }
    } else if let Some(g) = match &v.kind {
        ObjectKind::Generic(g) => Some(g.clone()),
        _ => None,
    } {
        if let Some(a) = g.downcast_assoc() {
            a.size()
        } else if let Some(a) = g.downcast_array() {
            a.size()
        } else {
            return Err(YacasError::NotList);
        }
    } else {
        return Err(YacasError::NotList);
    };
    let num = crate::value::LispNumber::from_text(len.to_string());
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Number(num),
    }))
}

/// Listify (See upstream: cyacas/libyacas/src/mathcommands.cpp LispListify): splice the *inner-chain head node* of the
/// argument's sublist onto a fresh List head (Java: head.Next().Set(ARG(1).SubList().Get())).
/// The inner-chain head itself is kept, not its next link: Listify({a,b,c}) ->
/// {List,a,b,c} with the original List head preserved.
pub fn cmd_listify(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    let sub = v.sublist().ok_or(YacasError::InvalidArg)?;
    let list_sym = env.symtab.look_up("List");
    let mut head = LispObject::atom(list_sym);
    if let Some(m) = Rc::get_mut(&mut head) {
        m.next = Some(sub.clone());
    }
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(head),
    }))
}

/// String (See upstream: cyacas/libyacas/src/mathcommands.cpp InternalStringify; Function flag: argument evaluated):
/// returns the argument's text, unconditionally wrapped in quotes. Upstream behavior:
/// String("xx") -> ""xx"", String(aa) -> "aa", String(12) -> "12", and String(f(x))
/// is InvalidArg (sublists have no text form).
pub fn cmd_string(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    let text = match v.atom_string() {
        Some(s) => s.to_string(),
        None => match v.number_string() {
            Some(n) => n,
            None => return Err(YacasError::InvalidArg),
        },
    };
    let quoted = format!("\"{text}\"");
    let sym = env.symtab.look_up(&quoted);
    Ok(LispObject::atom(sym))
}

/// Type (See upstream: cyacas/libyacas/src/mathcommands.cpp LispType): a call tree yields its head as a quoted string
/// (Equals(Type(2*x),"*") = True, Equals(Type(Sin(x)),"Sin") = True); non-lists
/// (atoms/numbers) yield the plain atom ""; a non-atomic head (a generic object)
/// also yields "".
pub fn cmd_type(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    let text = match v.atom_string() {
        Some(_) => "\"\"".to_string(), // Upstream behavior: the non-list branch yields an empty string.
        None => match v.sublist() {
            Some(sub) => match sub.atom_string() {
                Some(s) => format!("\"{s}\""), // Quoted string form (as the stringify lookup produces).
                None => "\"\"".into(),         // Non-atomic head -> empty string (same Java branch).
            },
            None => "\"\"".into(),             // Numbers/other non-lists -> empty string (Type(5) -> "").
        },
    };
    let sym = env.symtab.look_up(&text);
    Ok(LispObject::atom(sym))
}

/// RuleBaseDefined (See upstream: cyacas/libyacas/src/mathcommands.cpp LispRuleBaseDefined): evaluate both arguments;
/// the name goes through SymbolName (unquote + intern) before the lookup. True when
/// a rule base with that name and arity exists.
pub fn cmd_rule_base_defined(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let name_node = eval(env, arg(inner, 0)?)?;
    let name = {
        let s = name_node.atom_string().ok_or(YacasError::InvalidArg)?;
        crate::standard::symbol_name(env, s)
    };
    let arity_node = eval(env, arg(inner, 1)?)?;
    let arity: usize = arity_node
        .number_string()
        .and_then(|s| s.parse().ok())
        .ok_or(YacasError::InvalidArg)?;
    let found = env
        .user_functions
        .get(&name)
        .map(|m| m.user_func(arity).is_some())
        .unwrap_or(false);
    if found {
        Ok(env.true_atom())
    } else {
        Ok(env.false_atom())
    }
}

/// DefLoadFunction (See upstream: cyacas/libyacas/src/mathcommands.cpp LispDefLoadFunction):
/// (name) — evaluate name and unquote it; get-or-create the MultiUserFunction entry
/// (an empty one is created if absent). If a pending file is attached, it is only
/// detached, NOT loaded (the C++ InternalUse call is commented out upstream, so
/// `DefLoadFunction(f); f(3)` stays unexpanded). Once detached, the lazy trigger no
/// longer fires (the upstream GetUserFunction clears the field before checking).
/// Semantic decision: this deliberately diverges from the Java version, which issues
/// an immediate InternalUse — patterns.rep/code.ys calls
/// `DefLoadFunction(patternoper)` inside a DefinePattern rule body with no immediate
/// call depending on it, and immediate loading would observably differ from
/// upstream's canceled lazy trigger.
pub fn cmd_def_load_function(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let raw = name_node.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::symbol_name(env, raw);
    let entry = env
        .user_functions
        .entry(name)
        .or_default();
    entry.inner.borrow_mut().file_to_open = None;
    Ok(env.true_atom())
}

/// Use (See upstream: cyacas/libyacas/src/mathcommands.cpp LispUse): (fileName) —
/// evaluate the argument (Function|Fixed), unquote, then InternalUse (goes through
/// the def registry; already-loaded files are skipped). Used by the yacasinit.ys
/// boot loading entries.
pub fn cmd_use(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s).unwrap_or(s).to_string();
    crate::standard::internal_use(env, &name)?;
    Ok(env.true_atom())
}

/// Load (See upstream: cyacas/libyacas/src/mathcommands.cpp LispLoad): (fileName) —
/// evaluate, unquote, then InternalLoad (does NOT go through the def registry; that
/// is the core difference from Use). Upstream performs a CheckSecure check; here the
/// secure flag only blocks file reads.
pub fn cmd_load(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if env.secure {
        return Err(YacasError::SecurityBreach);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s).unwrap_or(s).to_string();
    crate::standard::internal_load(env, &name)?;
    Ok(env.true_atom())
}

/// Hold (See upstream: cyacas/libyacas/src/mathcommands.cpp LispHold; Macro|Fixed,
/// arguments held): returns a copy of ARGUMENT(1) without evaluating it. yacasinit.ys
/// Defun bodies rely on `Set(fn,Hold(@func))`: without the hold, fn would bind to a
/// (Hold ...) sublist and rule definitions would see a non-atomic function name.
pub fn cmd_hold(_env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    Ok(copy_node(arg(inner, 0)?))
}

/// Subst (See upstream: cyacas/libyacas/src/corefunctions.h LispSubst; Function
/// flag: arguments already evaluated): (from, to, body) — replace every
/// subexpression of body equal to `from` with a copy of `to` (C++ LispSubst ->
/// InternalSubstitute + SubstBehaviour). limit.rep rule 701 routes Limit through
/// ApplyPure("Subst",...); without this command the Limit simplification path would
/// recurse into a dead end and overflow the stack.
pub fn cmd_subst(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {    if arity_of(inner) != 3 {
        return Err(YacasError::WrongNumberOfArgs);
    }    let from = eval(env, arg(inner, 0)?)?;
    let to = eval(env, arg(inner, 1)?)?;    let body = eval(env, arg(inner, 2)?)?;
    let mut behaviour = crate::substitute::SubstBehaviourImpl::new(&from, &to);
    crate::standard::internal_substitute(env, &body, &mut behaviour)
}

/// DefaultDirectory (See upstream: cyacas/libyacas/src/mathcommands.cpp
/// LispDefaultDirectory): (directoryName) — evaluate, unquote, then append to the
/// input-directories list (push_back semantics, not replace).
pub fn cmd_default_directory(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s).unwrap_or(s).to_string();
    env.input_directories.push(name);
    Ok(env.true_atom())
}

/// arg (See upstream: cyacas/libyacas/src/mathcommands.cpp LispArg): the i-th command argument, held (Macro call context).
pub fn cmd_arg(_env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let i_node = arg(inner, 0)?;
    let i: usize = i_node
        .number_string()
        .and_then(|s| s.parse().ok())
        .ok_or(YacasError::InvalidArg)?;
    let slot = arg(inner, 1)?.sublist().ok_or(YacasError::InvalidArg)?.next.as_ref().ok_or(YacasError::WrongNumberOfArgs)?;
    let v = spine_refs(slot).nth(i - 1).ok_or(YacasError::WrongNumberOfArgs)?;
    Ok(copy_node(v))
}

/// Atom (See upstream: cyacas/libyacas/src/mathcommands.cpp LispAtom): turns the evaluated argument's text (a number or
/// atom) into an atom node.
pub fn cmd_atom(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    // Upstream behavior: Atom("aa") -> aa; Atom(ConcatStrings("aa","3")) -> aa3 —
    // the evaluated text is unquoted before building the atom. (The Java LispAtomize
    // wraps the text in quotes first, which does not match upstream; upstream is
    // authoritative.)
    let v = eval(env, arg(inner, 0)?)?;
    let text = match v.atom_string() {
        Some(t) => t.to_string(),
        None => v.number_string().unwrap_or_default(),
    };
    let unquoted = crate::standard::internal_unstringify(&text).unwrap_or(&text);
    let sym = env.symtab.look_up(unquoted);
    Ok(LispObject::atom(sym))
}

/// ConcatStrings (See upstream: cyacas/libyacas/src/mathcommands.cpp LispConcatStrings): concatenates the string arguments.
pub fn cmd_concat_strings(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let mut out = String::new();
    for i in 0..arity_of(inner) {
        let v = eval(env, arg(inner, i)?)?;
        let text = match v.atom_string() {
            Some(s) => s.to_string(),
            None => v.number_string().unwrap_or_default(),
        };
        let t = crate::standard::internal_unstringify(&text).unwrap_or(&text);
        out.push_str(t);
    }
    let sym = env.symtab.look_up_stringify(out.as_str());
    Ok(LispObject::atom(sym))
}

/// Concat (See upstream: cyacas/libyacas/src/corefunctions.h LispConcatenate;
/// Function|Variable): every argument must be a list (or a function expression);
/// their contents (each minus its List/function head) are concatenated into a new
/// list: Concat({a},{b}) -> {a,b}. lists.rep's MapArgs body uses
/// UnList(Concat({expr[1]},...)); without this command the argument would stay held
/// and UnList would reject the non-List head.
pub fn cmd_concat(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    let mut kinds: Vec<ObjectKind> = Vec::new();
    for i in 0..n {
        let v = eval(env, arg(inner, i)?)?;
        let sub = v.sublist().ok_or(YacasError::NotList)?;
        // Splice in this argument's contents (minus its List/function head); an
        // empty list (only a List head) contributes no elements.
        if let Some(first) = sub.next.as_ref() {
            kinds.extend(spine_kinds(first));
        }
    }
    let mut all: Vec<ObjectKind> = Vec::with_capacity(kinds.len() + 1);
    all.push(ObjectKind::Atom(env.symtab.look_up("List")));
    all.extend(kinds);
    let chain = crate::value::build_list(all).ok_or(YacasError::InvalidArg)?;
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(chain),
    }))
}

/// StringMid'Get (See upstream: cyacas/libyacas/src/mathcommands.cpp YacasStringMidGet):
/// (from, count, str) -> the `count` characters of str starting at 1-based `from`,
/// as a new string.
pub fn cmd_string_mid_get(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 3 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let from = int_text_of(&eval(env, arg(inner, 0)?)?)?;
    let count = int_text_of(&eval(env, arg(inner, 1)?)?)?;
    if from < 1 {
        return Err(YacasError::InvalidArg);
    }
    let s = eval(env, arg(inner, 2)?)?;
    let text = match s.atom_string() {
        Some(t) => crate::standard::internal_unstringify(t).unwrap_or(t).to_string(),
        None => return Err(YacasError::InvalidArg),
    };
    let chars: Vec<char> = text.chars().collect();
    let from = (from as usize) - 1;
    let count = count as usize;
    if from + count > chars.len() {
        return Err(YacasError::InvalidArg);
    }
    let sub: String = chars[from..from + count].iter().collect();
    let quoted = format!("\"{sub}\"");
    Ok(LispObject::atom(env.symtab.look_up(&quoted)))
}

/// StringMid'Set (See upstream: cyacas/libyacas/src/mathcommands.cpp YacasStringMidSet):
/// (from, repl, str) -> the contents of str starting at 1-based `from` are replaced
/// by repl, as a new string.
pub fn cmd_string_mid_set(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 3 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let from = int_text_of(&eval(env, arg(inner, 0)?)?)?;
    if from < 1 {
        return Err(YacasError::InvalidArg);
    }
    let repl_node = eval(env, arg(inner, 1)?)?;
    let repl = match repl_node.atom_string() {
        Some(t) => crate::standard::internal_unstringify(t).unwrap_or(t).to_string(),
        None => return Err(YacasError::InvalidArg),
    };
    let s = eval(env, arg(inner, 2)?)?;
    let text = match s.atom_string() {
        Some(t) => crate::standard::internal_unstringify(t).unwrap_or(t).to_string(),
        None => return Err(YacasError::InvalidArg),
    };
    let chars: Vec<char> = text.chars().collect();
    let from = (from as usize) - 1;
    if from + repl.chars().count() > chars.len() {
        return Err(YacasError::InvalidArg);
    }
    let mut out: Vec<char> = chars.clone();
    for (i, c) in repl.chars().enumerate() {
        out[from + i] = c;
    }
    let res: String = out.iter().collect();
    let quoted = format!("\"{res}\"");
    Ok(LispObject::atom(env.symtab.look_up(&quoted)))
}

/// `+` (See upstream: cyacas/libyacas/src/mathcommands.cpp LispAdd): arguments evaluated; all numbers -> numeric sum; any
/// symbol -> held (a+b...).
/// MathAdd (See upstream: cyacas/libyacas/src/mathcommands.cpp LispAdd; Function|Fixed, 2 args): fixed arity —
/// MathAdd(5,2,1) and MathAdd(7) are WrongNumberOfArgs (distinguishing it from the
/// variadic `+`).
pub fn cmd_math_add(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    cmd_add(env, inner)
}

/// Addition (variadic; the fixed-2-arg MathAdd is cmd_math_add).
pub fn cmd_add(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    // Seed the accumulator with zero at prec=0 (a storage form, not a truncation
    // cap) — the upstream starts from BigNumber("0", BinaryPrecision()): the
    // environment precision only enters through the add's precision parameter, so
    // results are truncated at the session precision.
    let mut acc = crate::number::float::Float::from_decimal("0").expect("zero").with_prec(0);
    let mut symbolic = false;
    let mut any_float = false; // Float contamination: 9+0. -> "9." while 2+3 -> "5".
    let n = arity_of(inner);
    let mut held: Vec<ObjectKind> = Vec::new();
    for i in 0..n {
        let mut v = eval(env, arg(inner, i)?)?;
        // Upstream behavior (held trees reaching math commands):
        // `T(_x)<--x+x; T(f(aa));` yields 4*aa — math commands re-evaluate held
        // arguments once before holding. A macro parameter read produces the held
        // tree f(aa); `+` re-evaluates it -> 2*aa (re-evaluating atoms/numbers is a
        // no-op). If the re-evaluation is still symbolic, collect it as held directly
        // (a third evaluation would be wrong; falling back to return_un_evaluated on
        // the original inner argument would discard the re-evaluated result).
        if matches!(v.kind, ObjectKind::Sublist(_)) {
            v = eval(env, &v)?;
        }
        match &v.kind {
            ObjectKind::Number(num) => {
                any_float |= num.is_float();
                acc = acc.add(&num.float_at(env.precision()), env.precision());
            }
            _ => {
                symbolic = true;
                held.push(spine_kinds(&v).next().expect("node kind"));
            }
        }
    }
    if symbolic {
        // Held shape = original head (`+`) plus the re-evaluated arguments (the same
        // head+args structure as return_un_evaluated, but with the re-evaluated
        // results; upstream prints `2*aa+bb`, not a List-headed form).
        let plus_sym = env.symtab.look_up("+");
        let mut all: Vec<ObjectKind> = Vec::with_capacity(held.len() + 1);
        all.push(ObjectKind::Atom(plus_sym));
        all.extend(held);
        let chain = crate::value::build_list(all).ok_or(YacasError::InvalidArg)?;
        return Ok(Rc::new(LispObject { next: None, kind: ObjectKind::Sublist(chain) }));
    }
    let num = crate::value::LispNumber::from_float_flag(acc, any_float);
    Ok(Rc::new(LispObject { next: None, kind: ObjectKind::Number(num) }))
}

/// `*` (See upstream: cyacas/libyacas/src/mathcommands.cpp LispMultiply): multiplies numbers.
pub fn cmd_mul(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let mut acc = crate::number::float::Float::from_decimal("1").expect("one").with_prec(0);
    let mut symbolic = false;
    let mut any_float = false; // Float contamination.
    let n = arity_of(inner);
    for i in 0..n {
        let v = eval(env, arg(inner, i)?)?;
        match &v.kind {
            ObjectKind::Number(num) => {
                any_float |= num.is_float();
                acc = acc.mul(&num.float_at(env.precision()), env.precision())
            }
            _ => symbolic = true,
        }
    }
    if symbolic {
        return crate::standard::return_un_evaluated(env, inner);
    }
    let num = crate::value::LispNumber::from_float_flag(acc, any_float);
    Ok(Rc::new(LispObject { next: None, kind: ObjectKind::Number(num) }))
}

/// `-` (See upstream: cyacas/libyacas/src/mathcommands.cpp LispSubtract): one argument negates; more arguments subtract in
/// sequence.
pub fn cmd_sub(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n == 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let mut acc = eval(env, arg(inner, 0)?)?;
    let mut any_float = false; // Float contamination.
    if n == 1 {
        if let ObjectKind::Number(num) = &acc.kind {
            any_float = num.is_float();
            let f = num.float_at(env.precision()).negate();
            let num = crate::value::LispNumber::from_float_flag(f, any_float);
            return Ok(Rc::new(LispObject { next: None, kind: ObjectKind::Number(num) }));
        }
        return crate::standard::return_un_evaluated(env, inner);
    }
    let mut symbolic = !matches!(&acc.kind, ObjectKind::Number(_));
    if let ObjectKind::Number(num) = &acc.kind {
        any_float = num.is_float();
    }
    for i in 1..n {
        let v = eval(env, arg(inner, i)?)?;
        match (&acc.kind, &v.kind) {
            (ObjectKind::Number(a), ObjectKind::Number(b)) => {
                any_float |= b.is_float();
                let f = a.float_at(env.precision()).sub(&b.float_at(env.precision()), env.precision());
                acc = Rc::new(LispObject { next: None, kind: ObjectKind::Number(crate::value::LispNumber::from_float_flag(f, any_float)) });
            }
            _ => symbolic = true,
        }
    }
    if symbolic {
        return crate::standard::return_un_evaluated(env, inner);
    }
    Ok(acc)
}

/// `/` (See upstream: cyacas/libyacas/src/mathcommands.cpp LispDivide): divides the arguments numerically.
pub fn cmd_div(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n < 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let mut acc = eval(env, arg(inner, 0)?)?;
    let mut any_float = false; // Float contamination.
    let mut symbolic = !matches!(&acc.kind, ObjectKind::Number(_));
    if let ObjectKind::Number(num) = &acc.kind {
        any_float = num.is_float();
    }
    for i in 1..n {
        let v = eval(env, arg(inner, i)?)?;
        match (&acc.kind, &v.kind) {
            (ObjectKind::Number(a), ObjectKind::Number(b)) => {
                any_float |= b.is_float();
                let f = a.float_at(env.precision()).div(&b.float_at(env.precision()), env.precision())
                    .ok_or(YacasError::DivideByZero)?;
                acc = Rc::new(LispObject { next: None, kind: ObjectKind::Number(crate::value::LispNumber::from_float_flag(f, any_float)) });
            }
            _ => symbolic = true,
        }
    }
    if symbolic {
        return crate::standard::return_un_evaluated(env, inner);
    }
    Ok(acc)
}

// Comparison value: numeric or textual ordering (the BigNumber/string
// LexCompare semantics).
enum OrderVal {
    Num(crate::number::float::Float),
    Text(String),
}

// LessThan comparison (See upstream: cyacas/libyacas/src/mathcommands.cpp LispLess): evaluate the arguments, then compare
// numerically or by string order.
fn num_or_text(env: &mut Environment, v: &Rc<LispObject>) -> Result<OrderVal, YacasError> {
    match v.atom_string() {
        Some(s) => Ok(OrderVal::Text(s.to_string())),
        None => match v.number_string() {
            Some(n) => {
                let f = crate::number::float::Float::from_decimal_with_prec(&n, env.precision())
                    .ok_or(YacasError::InvalidArg)?;
                Ok(OrderVal::Num(f))
            }
            None => Err(YacasError::InvalidArg),
        },
    }
}
pub fn cmd_less(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let va = eval(env, arg(inner, 0)?)?;
    let a = num_or_text(env, &va)?;
    let vb = eval(env, arg(inner, 1)?)?;
    let b = num_or_text(env, &vb)?;
    let lt = match (&a, &b) {
        (OrderVal::Num(x), OrderVal::Num(y)) => x.less_than(y),
        (OrderVal::Text(x), OrderVal::Text(y)) => x < y,
        _ => return Err(YacasError::InvalidArg),
    };
    if lt { Ok(env.true_atom()) } else { Ok(env.false_atom()) }
}
pub fn cmd_greater(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let va = eval(env, arg(inner, 0)?)?;
    let a = num_or_text(env, &va)?;
    let vb = eval(env, arg(inner, 1)?)?;
    let b = num_or_text(env, &vb)?;
    let gt = match (&a, &b) {
        (OrderVal::Num(x), OrderVal::Num(y)) => y.less_than(x),
        (OrderVal::Text(x), OrderVal::Text(y)) => x > y,
        _ => return Err(YacasError::InvalidArg),
    };
    if gt { Ok(env.true_atom()) } else { Ok(env.false_atom()) }
}
pub fn cmd_less_eq(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let va = eval(env, arg(inner, 0)?)?;
    let a = num_or_text(env, &va)?;
    let vb = eval(env, arg(inner, 1)?)?;
    let b = num_or_text(env, &vb)?;
    let le = match (&a, &b) {
        (OrderVal::Num(x), OrderVal::Num(y)) => x.less_than(y) || x.equals(y),
        (OrderVal::Text(x), OrderVal::Text(y)) => x <= y,
        _ => return Err(YacasError::InvalidArg),
    };
    if le { Ok(env.true_atom()) } else { Ok(env.false_atom()) }
}
pub fn cmd_greater_eq(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let va = eval(env, arg(inner, 0)?)?;
    let a = num_or_text(env, &va)?;
    let vb = eval(env, arg(inner, 1)?)?;
    let b = num_or_text(env, &vb)?;
    let ge = match (&a, &b) {
        (OrderVal::Num(x), OrderVal::Num(y)) => y.less_than(x) || x.equals(y),
        (OrderVal::Text(x), OrderVal::Text(y)) => x >= y,
        _ => return Err(YacasError::InvalidArg),
    };
    if ge {
        Ok(env.true_atom())
    } else {
        Ok(env.false_atom())
    }
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
pub fn write_back_arg0(env: &mut Environment, inner: &Rc<LispObject>, result: &Rc<LispObject>) -> Result<(), YacasError> {
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
pub fn cmd_prog(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n == 0 {
        return Ok(env.true_atom());
    }
    // Upstream behavior (LispProgBody): LispLocalFrame(env, false) is not fenced
    // ("Allow accessing previous locals"), so the block can read function parameters
    // and outer locals.
    env.push_local_frame(false);
    let r = (|| {
        let mut last = env.false_atom();
        for i in 0..n {
            last = eval(env, arg(inner, i)?)?;
        }
        Ok(last)
    })();
    env.pop_local_frame()?;
    r
}

/// While (See upstream: cyacas/libyacas/src/mathcommands.cpp LispWhile): evaluate the condition each round; while true,
/// evaluate the body; exit when the predicate is False and ALWAYS return True.
pub fn cmd_while(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    // Upstream behavior (LispWhile): after the loop the CheckArg predicate must be False
    // and the result is always True (not the last body value). Downstream code such
    // as sparsetree.ys MultiDropScan relies on the always-true return to prune empty
    // sparse branches.
    loop {
        let cond = eval(env, arg(inner, 0)?)?;
        if is_true(env, &cond) {
            eval(env, arg(inner, 1)?)?;
        } else {
            if !is_false(env, &cond) {
                return Err(YacasError::InvalidArg);
            }
            return Ok(env.true_atom());
        }
    }
}

/// MacroRuleBase (See upstream: cyacas/libyacas/src/mathcommands.cpp LispMacroRuleBase; Function flag: arguments
/// already evaluated): (name, {params...}) — registers a rule-base function. The
/// parameter chain is the argument sublist minus its List head (Java:
/// args.SubList().Get().Next()); the name goes through SymbolName.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispMacroRuleBase ->
/// InternalRuleBase -> DeclareRuleBase: this creates a *branched* (rule-base)
/// function, NOT a macro — macro/pattern binding happens at rule-match time.
pub fn cmd_macro_rule_base(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let name_node = eval(env, arg(inner, 0)?)?;
    let name = {
        let s = name_node.atom_string().ok_or(YacasError::InvalidArg)?;
        crate::standard::symbol_name(env, s)
    };
    let params = params_opt_of(env, { let _a1=arg(inner, 1)?; _a1 })?;
    env.declare_rule_base(name, params.as_ref(), false)?;
    Ok(env.true_atom())
}
/// Pattern'Create (the script-level Pattern'Create shape): variable-name chain +
/// post-predicate -> a Pattern generic object.
pub fn cmd_pattern_create(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let vars = eval(env, arg(inner, 0)?)?;
    let vars_chain = match vars.sublist() {
        Some(s) => s.next.as_ref().cloned(), // An empty `{}` (0-arity rule) -> None.
        None => return Err(YacasError::InvalidArg),
    };
    let post = eval(env, arg(inner, 1)?)?;
    let pred = crate::pattern::PatternPredicate::from_vars(env, vars_chain.as_ref(), &post)?;
    let pc = crate::pattern::PatternClass { pattern: pred };
    let g: Rc<dyn crate::value::GenericClass> = Rc::new(pc);
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Generic(g),
    }))
}

/// MacroRulePattern (See upstream: cyacas/libyacas/src/mathcommands.cpp LispMacroNewRulePattern; Function flag:
/// arguments already evaluated): (name, arity, prec, pattern) body — pattern-rule
/// registration. The name goes through SymbolName; the pattern argument is a Pattern
/// generic object.
pub fn cmd_macro_rule_pattern(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n < 4 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let name = {
        let s = name_node.atom_string().ok_or(YacasError::InvalidArg)?;
        crate::standard::symbol_name(env, s)
    };
    let arity_node = eval(env, arg(inner, 1)?)?;
    let arity: usize = arity_node.number_string().and_then(|s| s.parse().ok()).ok_or(YacasError::InvalidArg)?;
    let prec_node = eval(env, arg(inner, 2)?)?;
    let prec: i32 = prec_node.number_string().and_then(|s| s.parse().ok()).ok_or(YacasError::InvalidArg)?;
    let pattern = eval(env, arg(inner, 3)?)?;
    // The body is also evaluated (per the Java InternalNewRulePattern Function
    // calling convention: all MacroRulePattern arguments arrive evaluated, including
    // a bodied body). code.ys rule 40 relies on this: `patternright` is a parameter
    // variable, and without evaluation it would be registered as a literal symbol
    // with no slot to read at call time. (Only the Macro variant's body is held; the
    // difference matches the upstream command flags.)
    let body = eval(env, arg(inner, 4)?)?;
    env.define_rule_pattern(name, arity, prec, &pattern, &body)?;
    Ok(env.true_atom())
}

// Symbol name (unquote a quoted string
// and intern).
fn symbol_name_of(env: &mut Environment, node: &Rc<LispObject>) -> Result<Rc<str>, YacasError> {
    let s = node.atom_string().ok_or(YacasError::InvalidArg)?;
    Ok(crate::standard::symbol_name(env, s))
}

// Integer argument (number or atomic
// text.
fn int_text_of(node: &Rc<LispObject>) -> Result<i64, YacasError> {
    let t = match node.number_string() {
        Some(n) => n,
        None => node.atom_string().ok_or(YacasError::InvalidArg)?.to_string(),
    };
    t.trim().parse().map_err(|_| YacasError::InvalidArg)
}

// Rule-base parameters: an empty `{}` is legal upstream (a 0-arity function;
// standard.ys InNumericMode/InVerboseMode etc. use `Function("X",{}) body`).
// `{}` -> None; otherwise the first-element chain.
fn params_opt_of(env: &mut Environment, node: &Rc<LispObject>) -> Result<Option<Rc<LispObject>>, YacasError> {
    let v = eval(env, node)?;
    let sub = v.sublist().ok_or(YacasError::NotList)?;
    if crate::standard::internal_list_length(sub) <= 1 {
        return Ok(None);
    }
    Ok(sub.next.clone())
}

// Macro rule-base parameters (matching upstream InternalDefMacroRuleBase's
// `LispPtr args(ARGUMENT(2))`: the argument is taken unevaluated, so parameter-name
// atoms keep their original form). Key difference from `params_opt_of`: that one
// evaluates first (harmless for ordinary function parameter lists, which stay
// atomic), but a macro parameter list must NOT be evaluated: after qq:=99, the call
// Macro(q,{qq}) needs the key "qq" so @-substitution finds it; evaluating would turn
// it into {99} and @qq would no longer match.
fn params_hold_opt_of(node: &Rc<LispObject>) -> Result<Option<Rc<LispObject>>, YacasError> {
    let sub = node.sublist().ok_or(YacasError::NotList)?;
    if crate::standard::internal_list_length(sub) <= 1 {
        return Ok(None);
    }
    Ok(sub.next.clone())
}

/// RuleBase (See upstream: cyacas/libyacas/src/mathcommands.cpp LispRuleBase; Macro flag: arguments held):
/// (name, {params...}).
pub fn cmd_rule_base(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = symbol_name_of(env, { let _a0=arg(inner, 0)?; _a0 })?;
    // The parameter list is NOT evaluated (matching upstream InternalRuleBase's
    // `LispPtr args(ARGUMENT(2))`: the argument is taken directly, the same path as
    // MacroRuleBase). Evaluating it first would substitute current bindings for the
    // parameter names whenever loading happens in a scope where those names are
    // bound, so the function body's variables would resolve to wrong values.
    let params = params_hold_opt_of({ let _a1=arg(inner, 1)?; _a1 })?;
    env.declare_rule_base(name, params.as_ref(), false)?;
    Ok(env.true_atom())
}

/// RuleBaseListed (See upstream: cyacas/libyacas/src/mathcommands.cpp LispRuleBaseListed): same as RuleBase with
/// listed=true (the parameter list is likewise not evaluated).
pub fn cmd_rule_base_listed(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = symbol_name_of(env, { let _a0=arg(inner, 0)?; _a0 })?;
    let params = params_hold_opt_of({ let _a1=arg(inner, 1)?; _a1 })?;
    env.declare_rule_base(name, params.as_ref(), true)?;
    Ok(env.true_atom())
}

/// MacroRuleBaseListed (See upstream: cyacas/libyacas/src/mathcommands.cpp LispMacroRuleBaseListed; Function flag:
/// arguments already evaluated).
pub fn cmd_macro_rule_base_listed(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let name = symbol_name_of(env, &name_node)?;
    let params = params_opt_of(env, { let _a1=arg(inner, 1)?; _a1 })?;
    // Upstream behavior (InternalRuleBase with listed): a branched (rule-base) function, same
    // family as RuleBaseListed — not a macro.
    env.declare_rule_base(name, params.as_ref(), true)?;
    Ok(env.true_atom())
}

/// DefMacroRuleBase (See upstream: cyacas/libyacas/src/mathcommands.cpp InternalDefMacroRuleBase; Macro flag:
/// arguments held): (name, {params...}) — the parameter list is taken unevaluated
/// (ARGUMENT(2) directly), so parameter-name keys keep the atoms as declared.
pub fn cmd_def_macro_rule_base(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = symbol_name_of(env, { let _a0=arg(inner, 0)?; _a0 })?;
    let params = params_hold_opt_of({ let _a1=arg(inner, 1)?; _a1 })?;
    env.declare_macro_rule_base(name, params.as_ref(), false)?;
    Ok(env.true_atom())
}

/// DefMacroRuleBaseListed (See upstream: cyacas/libyacas/src/mathcommands.cpp InternalDefMacroRuleBase with
/// aListed=true; Macro flag). Same as cmd_def_macro_rule_base: the parameter list is
/// taken unevaluated (keys keep the atomic names).
pub fn cmd_def_macro_rule_base_listed(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = symbol_name_of(env, { let _a0=arg(inner, 0)?; _a0 })?;
    let params = params_hold_opt_of({ let _a1=arg(inner, 1)?; _a1 })?;
    env.declare_macro_rule_base(name, params.as_ref(), true)?;
    Ok(env.true_atom())
}

/// UnList (See upstream: cyacas/libyacas/src/mathcommands.cpp LispUnList): the evaluated argument must be a
/// List-headed sublist; returns the *call-shaped* sublist with the List head removed
/// (the tail is returned as a call expression, not a bare
/// element chain). The if-else fallback rule body `UnList({Atom("else"),...})`
/// relies on this expanding into a proper call that the printer renders with infix
/// operators: if(3) 11 else 22 -> "if(3)11 else 22".
pub fn cmd_unlist(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let sub = v.sublist().ok_or(YacasError::NotList)?;
    if sub.atom_string().map(|s| s.as_ref()) != Some("List") {
        return Err(YacasError::InvalidArg);
    }
    let inner_chain = sub.next.clone().ok_or(YacasError::InvalidArg)?;
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(inner_chain),
    }))
}

/// HoldArg (See upstream: cyacas/libyacas/src/mathcommands.cpp LispHoldArg; Macro): (name, paramName) — adds the
/// parameter to the hold list.
pub fn cmd_hold_arg(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = symbol_name_of(env, { let _a0=arg(inner, 0)?; _a0 })?;
    let var = arg(inner, 1)?.atom_string().ok_or(YacasError::InvalidArg)?.to_string();
    env.hold_argument(name, &var)?;
    Ok(env.true_atom())
}

/// Rule (See upstream: cyacas/libyacas/src/mathcommands.cpp InternalNewRule; Macro flag: arguments
/// held): (name, arity, precedence, predicate, body) -> env.define_rule.
pub fn cmd_rule(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 5 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = symbol_name_of(env, { let _a0=arg(inner, 0)?; _a0 })?;
    let arity = int_text_of(arg(inner, 1)?)? as i32 as usize;
    let prec = int_text_of(arg(inner, 2)?)? as i32;
    let predicate = arg(inner, 3)?;
    let body = arg(inner, 4)?;
    env.define_rule(name, arity, prec, predicate, body)?;
    Ok(env.true_atom())
}

/// RulePattern (See upstream: cyacas/libyacas/src/mathcommands.cpp InternalNewRulePattern; Macro flag: arguments held):
/// (name, arity, precedence, pattern, body) -> env.define_rule_pattern.
pub fn cmd_rule_pattern(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 5 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = symbol_name_of(env, { let _a0=arg(inner, 0)?; _a0 })?;
    let arity = int_text_of(arg(inner, 1)?)? as i32 as usize;
    let prec = int_text_of(arg(inner, 2)?)? as i32;
    let pattern = arg(inner, 3)?;
    let body = arg(inner, 4)?;
    env.define_rule_pattern(name, arity, prec, pattern, body)?;
    Ok(env.true_atom())
}

/// MacroRule (See upstream: cyacas/libyacas/src/mathcommands.cpp InternalNewRule; Function flag:
/// arguments already evaluated): (name, arity, precedence, predicate, body) ->
/// env.define_rule.
pub fn cmd_macro_rule(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 5 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let name = symbol_name_of(env, &name_node)?;
    let arity_node = eval(env, arg(inner, 1)?)?;
    let arity = int_text_of(&arity_node)? as i32 as usize;
    let prec_node = eval(env, arg(inner, 2)?)?;
    let prec = int_text_of(&prec_node)? as i32;
    let pred_node = eval(env, arg(inner, 3)?)?;
    // The body argument is evaluated: upstream `MacroRule` has the Function|Fixed
    // flags (arguments arrive already evaluated), and Rust commands receive raw
    // arguments, so this is an unconditional eval. Held parameter atoms (e.g. the
    // aRightAssign body of z(x):=5) read the frame value without executing the
    // block; expressions (e.g. deffunc's `arglist[2]`, an Nth over a LocalSymbols
    // result) evaluate to the template body; bare literal block bodies execute as
    // written. Holding non-atoms instead would register Table's rule body as the
    // literal `arglist[2]`, breaking it.
    let body_arg = arg(inner, 4)?;
    let body_node = eval(env, body_arg)?;
    env.define_rule(name, arity, prec, &pred_node, &body_node)?;
    Ok(env.true_atom())
}

/// UnFence — (name, arity) removes the fence.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispUnFence (Function flag:
/// arguments already evaluated).
pub fn cmd_un_fence(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let name = symbol_name_of(env, &name_node)?;
    let arity_node = eval(env, arg(inner, 1)?)?;
    let arity = int_text_of(&arity_node)? as i32 as usize;
    env.un_fence_rule(name, arity)?;
    Ok(env.true_atom())
}

/// Retract — (name, arity) deletes that rule base.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispRetract (Function flag:
/// arguments already evaluated).
pub fn cmd_retract(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let name = symbol_name_of(env, &name_node)?;
    let arity_node = eval(env, arg(inner, 1)?)?;
    let arity = int_text_of(&arity_node)? as i32 as usize;
    env.retract(name, arity)?;
    Ok(env.true_atom())
}

/// RuleBaseArgList — (name, arity) returns the function's parameter chain wrapped
/// in a List head.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispRuleBaseArgList (Function
/// flag: arguments already evaluated).
pub fn cmd_rule_base_arg_list(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let name = symbol_name_of(env, &name_node)?;
    let arity_node = eval(env, arg(inner, 1)?)?;
    let arity = int_text_of(&arity_node)? as i32 as usize;
    let f = env.user_func(&name, arity).ok_or(YacasError::InvalidArg)?;
    let params = f.arg_list().ok_or(YacasError::InvalidArg)?;
    let list_sym = env.symtab.look_up("List");
    let mut head = LispObject::atom(list_sym);
    if let Some(m) = Rc::get_mut(&mut head) {
        m.next = Some(params.clone());
    }
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(head),
    }))
}

/// DefLoad — (fileName) reads the `fileName.def` manifest (resolved through the
/// directory search chain), tokenizing until `}` or EOF. For each symbol:
/// get-or-create the MultiUserFunction, set file_to_open=def, insert into
/// def.symbols, and Protect. If a symbol is already registered (file_to_open
/// non-empty), DefFileAlreadyChosen is raised. Registration only — no code is
/// loaded here (lazy loading triggers on first call; see
/// evaluator::get_user_function).
/// Note: the def-file table keys on the unquoted file name (internal
/// normalization; upstream keys on the quoted raw text — equivalent internally).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispDefLoad;
/// cyacas/libyacas/src/deffile.cpp LoadDefFile / DoLoadDefFile (Function|Fixed:
/// arguments already evaluated).
pub fn cmd_def_load(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if env.secure {
        return Err(YacasError::SecurityBreach);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let s = name_node.atom_string().ok_or(YacasError::InvalidArg)?;
    let def_name = crate::standard::internal_unstringify(s).unwrap_or(s).to_string();
    // LoadDefFile: flatfile = unstringify(name)+".def", opened via directory
    // search; failure -> FileNotFound.
    let flatfile = format!("{def_name}.def");
    let path = crate::standard::internal_find_file(env, &flatfile).ok_or(YacasError::FileNotFound)?;
    let text = std::fs::read_to_string(&path).map_err(|_| YacasError::FileNotFound)?;
    // DoLoadDefFile: tokens are read until `}` or EOF (the manifest is a list of
    // symbol lines plus a trailing `}`; some manifests end at EOF without a
    // closing `}`, and none start with `{`). Symbol names are collected first,
    // then registered (avoids a cross-field double mutable borrow).
    let mut symbols: Vec<String> = Vec::new();
    {
        let mut tok = crate::tokenizer::Tokenizer::new(&text);
        loop {
            let t = tok.next_token().map_err(|_| YacasError::InvalidArg)?;
            if t.is_empty() || t == "}" {
                break;
            }
            symbols.push(t);
        }
    }
    // Register (DoLoadDefFile semantics): map entry get-or-create; per symbol
    // get-or-create + file_to_open + symbols.insert + Protect. Destructure
    // &mut env: entry and user_functions are different fields, no borrow conflict.
    let done = env.true_atom();
    let Environment { user_functions, def_files, symtab, protected, .. } = env;
    let entry = def_files
        .map
        .entry(def_name.clone())
        .or_insert_with(|| crate::loader::DefFile::new(&def_name));
    let def = entry.clone();
    for t in &symbols {
        let sym = symtab.look_up(t);
        let m = user_functions
            .entry(sym.clone())
            .or_insert_with(crate::userfunc::MultiUserFunction::new);
        let mut st = m.inner.borrow_mut();
        if st.file_to_open.is_some() {
            // Upstream prints "[token]\n" to CurrentOutput before raising
            // DefFileAlreadyChosen; there is no console stream here, so only the
            // error is raised (failure behavior is the same).
            return Err(YacasError::DefFileAlreadyChosen);
        }
        st.file_to_open = Some(def.clone());
        entry.symbols.insert(sym.clone());
        protected.insert(sym.clone());
    }
    Ok(done)
}

/// ── Operator command family ─────────────────────────────────────────────
///
/// The table key is the key the parser looks up at parse time (the symbol as
/// resident in symtab, matching parser.rs infix_lookup's look_up approach), the
/// same set of keys as the static startup table in
/// operators.rs::register_stdops — loading stdopers.ys must reproduce the
/// static table entry by entry.
/// Precedence upper bound (upstream KMaxPrecedence = 60000; MultiFix family
/// rejects a second argument above this).
const K_MAX_PRECEDENCE: i32 = 60000;

/// One of the operator tables (upstream keeps four LispOperators tables).
enum OpTable {
    Infix,
    Prefix,
    Postfix,
    Bodied,
}

/// Extract an operator name (upstream MultiFix's ARGUMENT(1)->String()): string
/// literals are unquoted, plain atoms keep their name — in stdopers.ys,
/// Bodied(Assert,60000) without quotes is also valid.
fn operator_name(env: &mut Environment, node: &Rc<LispObject>) -> Result<Rc<str>, YacasError> {
    let v = eval(env, node)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    match crate::standard::internal_unstringify(s) {
        Ok(un) => Ok(un.into()),
        Err(_) => Ok(s.clone()),
    }
}

/// Extract a precedence (upstream InternalAsciiToInt +
/// CheckArg(prec<=KMaxPrecedence,2)): a numeric atom or a numeric text atom both
/// work; non-numeric or out-of-range -> InvalidArg.
fn operator_precedence(env: &mut Environment, node: &Rc<LispObject>) -> Result<i32, YacasError> {
    let v = eval(env, node)?;
    let text = match &v.kind {
        ObjectKind::Number(num) => num.string(),
        _ => v.atom_string().ok_or(YacasError::InvalidArg)?.to_string(),
    };
    let prec: i32 = text.parse().map_err(|_| YacasError::InvalidArg)?;
    if prec > K_MAX_PRECEDENCE {
        return Err(YacasError::InvalidArg);
    }
    Ok(prec)
}

/// MultiFix family (Infix/Prefix/Postfix/Bodied).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp MultiFix.
fn multi_fix(
    env: &mut Environment,
    inner: &Rc<LispObject>,
    table: OpTable,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = operator_name(env, arg(inner, 0)?)?;
    let prec = operator_precedence(env, arg(inner, 1)?)?;
    let op = Operator {
        prec,
        left_prec: prec,
        right_prec: prec,
        right_assoc: false,
    };
    let sym = env.symtab.look_up(&name);
    match table {
        OpTable::Infix => {
            env.infix.insert(sym, op);
        }
        OpTable::Prefix => {
            env.prefix.insert(sym, op);
        }
        OpTable::Postfix => {
            env.postfix.insert(sym, op);
        }
        OpTable::Bodied => {
            env.bodied.insert(sym, op);
        }
    }
    Ok(env.true_atom())
}

pub fn cmd_infix(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    multi_fix(env, inner, OpTable::Infix)
}
pub fn cmd_prefix(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    multi_fix(env, inner, OpTable::Prefix)
}
pub fn cmd_postfix(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    multi_fix(env, inner, OpTable::Postfix)
}
pub fn cmd_bodied(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    multi_fix(env, inner, OpTable::Bodied)
}

/// RightAssociative: mark the infix operator as right-associative; a name not in
/// the infix table -> NotAnInfixOperator.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispRightAssociative.
pub fn cmd_right_associative(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = operator_name(env, arg(inner, 0)?)?;
    let sym = env.symtab.look_up(&name);
    match env.infix.get_mut(&sym) {
        Some(op) => {
            op.right_assoc = true;
            Ok(env.true_atom())
        }
        None => Err(YacasError::NotAnInfixOperator),
    }
}

/// Left/RightPrecedence: rewrite one side of an infix operator's precedence; a
/// name not in the infix table -> NotAnInfixOperator.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispLeftPrecedence /
/// LispRightPrecedence.
fn set_infix_precedence(
    env: &mut Environment,
    inner: &Rc<LispObject>,
    right: bool,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = operator_name(env, arg(inner, 0)?)?;
    let prec = operator_precedence(env, arg(inner, 1)?)?;
    let sym = env.symtab.look_up(&name);
    match env.infix.get_mut(&sym) {
        Some(op) => {
            if right {
                op.right_prec = prec;
            } else {
                op.left_prec = prec;
            }
            Ok(env.true_atom())
        }
        None => Err(YacasError::NotAnInfixOperator),
    }
}

pub fn cmd_left_precedence(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    set_infix_precedence(env, inner, false)
}
pub fn cmd_right_precedence(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    set_infix_precedence(env, inner, true)
}

/// OpPrecedence family: return the precedence as a numeric atom, for scripts to
/// feed through AsciiToInt (upstream: OpPrecedence("=")=90,
/// OpRightPrecedence("-")=40); a name not in the infix table ->
/// NotAnInfixOperator.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispGetPrecedence /
/// LispGetLeftPrecedence / LispGetRightPrecedence.
enum OpField {
    Precedence,
    LeftPrecedence,
    RightPrecedence,
}

fn get_infix_field(
    env: &mut Environment,
    inner: &Rc<LispObject>,
    field: OpField,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = operator_name(env, arg(inner, 0)?)?;
    let sym = env.symtab.look_up(&name);
    let prec = match env.infix.get(&sym) {
        Some(op) => match field {
            OpField::Precedence => op.prec,
            OpField::LeftPrecedence => op.left_prec,
            OpField::RightPrecedence => op.right_prec,
        },
        None => return Err(YacasError::NotAnInfixOperator),
    };
    let num = crate::value::LispNumber::from_text(prec.to_string());
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Number(num),
    }))
}

pub fn cmd_op_precedence(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    get_infix_field(env, inner, OpField::Precedence)
}
pub fn cmd_op_left_precedence(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    get_infix_field(env, inner, OpField::LeftPrecedence)
}
pub fn cmd_op_right_precedence(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    get_infix_field(env, inner, OpField::RightPrecedence)
}

/// List — evaluate arguments one by one and build {List,e1..eN}.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispList (Macro|Variable).
pub fn cmd_list(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    let list_sym = env.symtab.look_up("List");
    let mut kinds: Vec<ObjectKind> = Vec::with_capacity(n + 1);
    kinds.push(ObjectKind::Atom(list_sym));
    for i in 0..n {
        let node = eval(env, arg(inner, i)?)?;
        kinds.push(spine_kinds(&node).next().expect("node kind"));
    }
    let chain = crate::value::build_list(kinds).ok_or(YacasError::InvalidArg)?;
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(chain),
    }))
}

/// MathNth — (list,index) returns the index+1-th item of the internal chain
/// (upstream InternalNth: the chain walks index steps from 0, index=1 -> first
/// element). Function flag: arguments already evaluated.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispNth.
pub fn cmd_math_nth(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let list_node = eval(env, arg(inner, 0)?)?;
    let index_node = eval(env, arg(inner, 1)?)?;
    let index = int_text_of(&index_node)?;
    let index = usize::try_from(index).map_err(|_| YacasError::InvalidArg)?;
    if std::env::var_os("YACAS_TRACE_LOAD").is_some() {
        eprintln!("[MATHNTH] arg1={} idx={index}", crate::printer::infix_print(env, &list_node));
    }
    let sub = list_node.sublist().ok_or(YacasError::NotList)?;
    crate::standard::internal_nth(sub, index)
}

/// And — evaluate arguments one by one (Macro|Variable, short-circuit): return
/// False immediately on a False argument; collect non-boolean values; a single
/// non-boolean is returned as-is; multiple ones are repacked as {And, original
/// order}; all-boolean returns True.
pub fn cmd_and(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    let mut nogos: Vec<Rc<LispObject>> = Vec::new();
    for i in 0..n {
        let v = eval(env, arg(inner, i)?)?;
        if is_false(env, &v) {
            return Ok(env.false_atom());
        }
        if !is_true(env, &v) {
            nogos.push(v);
        }
    }
    if nogos.is_empty() {
        return Ok(env.true_atom());
    }
    if nogos.len() == 1 {
        return Ok(copy_node(&nogos[0]));
    }
    // Rebuild with the head = the call name (upstream ARGUMENT(0)->Copy():
    // And(1,2) -> "1 And 2", MathAnd(1,2) -> "MathAnd(1,2)").
    repack_list(env, &call_head_name(inner), &nogos)
}

/// Or — evaluate arguments one by one (Macro|Variable, short-circuit): return
/// True immediately on a True argument; collect non-boolean values; a single
/// non-boolean is returned as-is; multiple ones are repacked as {Or, original
/// order}; all-boolean returns False.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispLazyOr.
pub fn cmd_or(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    let mut nogos: Vec<Rc<LispObject>> = Vec::new();
    for i in 0..n {
        let v = eval(env, arg(inner, i)?)?;
        if is_true(env, &v) {
            return Ok(env.true_atom());
        }
        if !is_false(env, &v) {
            nogos.push(v);
        }
    }
    if nogos.is_empty() {
        return Ok(env.false_atom());
    }
    if nogos.len() == 1 {
        return Ok(copy_node(&nogos[0]));
    }
    repack_list(env, &call_head_name(inner), &nogos)
}

/// The call head name (first atom of the inner chain; rebuilds keep the original
/// call name).
fn call_head_name(inner: &Rc<LispObject>) -> String {
    spine_refs(inner)
        .next()
        .and_then(|h| h.atom_string())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "And".into())
}

/// Repack {headName, items...} (upstream Copy(false) + Next splicing; shallow
/// copy keeps contents shared).
fn repack_list(env: &mut Environment, head: &str, items: &[Rc<LispObject>]) -> Result<Rc<LispObject>, YacasError> {
    let head_sym = env.symtab.look_up(head);
    let mut kinds: Vec<ObjectKind> = Vec::with_capacity(items.len() + 1);
    kinds.push(ObjectKind::Atom(head_sym));
    for it in items {
        kinds.push(spine_kinds(it).next().expect("node kind"));
    }
    let chain = crate::value::build_list(kinds).ok_or(YacasError::InvalidArg)?;
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(chain),
    }))
}

/// Predicate family — Function flag: arguments already evaluated.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispIsXxx.
/// Semantics: IsFunction(f(x)) -> True, IsNumber("5") -> False, and so on.
/// IsFunction: does the value hold a sublist?
pub fn cmd_is_function(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    Ok(crate::standard::internal_boolean(env, v.sublist().is_some()))
}

/// IsAtom — true unless the value is a sublist: numbers, strings and plain atoms
/// are all atoms (upstream: SubList()==null); a sublist is False.
/// Upstream behavior: IsAtom(1)=True, IsAtom(2.5)=True, IsAtom(foo)=True,
/// IsAtom({1})=False, IsAtom("s")=True.
/// Note: numbers count as atoms (LispNumber and LispAtom are the same family).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispIsAtom.
pub fn cmd_is_atom(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    Ok(crate::standard::internal_boolean(
        env,
        !matches!(v.kind, ObjectKind::Sublist(_)),
    ))
}

/// IsNumber — true only for number nodes (upstream: Number()!=null); a string
/// like "5" is not a number.
pub fn cmd_is_number(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    Ok(crate::standard::internal_boolean(env, matches!(v.kind, ObjectKind::Number(_))))
}

/// IsInteger — true when the numeric representation has no decimal point and no
/// exponent (upstream: LispIsInteger -> BigNumber.IsInt). The check is decided
/// directly on the text: 5 -> True, 5.5 -> False, 3.0 -> False.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispIsInteger.
/// Integer sign predicate family (upstream: the &/|/% rule suffix predicates in
/// standard.ys use a_IsNonNegativeInteger & b_IsNonNegativeInteger <-- BitAnd(a,b)
/// etc.; 2&5 -> 0, 5%2 -> 1 depend on these). Integer argument -> compare by
/// sign; otherwise False.
fn int_sign_predicate(
    env: &mut Environment,
    inner: &Rc<LispObject>,
    f: fn(i64) -> bool,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let n = match int_text_of(&v) {
        Ok(n) => n,
        Err(_) => return Ok(env.false_atom()),
    };
    Ok(crate::standard::internal_boolean(env, f(n)))
}
pub fn cmd_is_non_negative_integer(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    int_sign_predicate(env, inner, |n| n >= 0)
}
pub fn cmd_is_positive_integer(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    int_sign_predicate(env, inner, |n| n > 0)
}
pub fn cmd_is_non_positive_integer(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    int_sign_predicate(env, inner, |n| n <= 0)
}
pub fn cmd_is_negative_integer(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    int_sign_predicate(env, inner, |n| n < 0)
}
pub fn cmd_is_integer(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    let text = match &v.kind {
        ObjectKind::Number(n) => Some(n.string()),
        _ => return Ok(env.false_atom()),
    };
    let is_int = match text {
        Some(t) => !t.contains('.') && !t.contains('e') && !t.contains('E'),
        None => false,
    };
    Ok(crate::standard::internal_boolean(env, is_int))
}

/// Add/query/clear environment-local assumptions. `Assume` and `IsAssumed`
/// hold the symbol name. `IsAssumedValue` evaluates its first argument once
/// for use inside script rules whose pattern parameter is locally bound; a
/// resolved non-symbol is simply not an assumed atom.
pub fn cmd_assume(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let symbol = arg(inner, 0)?.atom_string().ok_or(YacasError::InvalidArg)?;
    let fact_name = arg(inner, 1)?.atom_string().ok_or(YacasError::InvalidArg)?;
    let fact = Assumption::parse(fact_name).ok_or(YacasError::InvalidArg)?;
    env.assume(symbol, fact).map_err(|_| YacasError::InvalidArg)?;
    Ok(env.true_atom())
}

pub fn cmd_is_assumed(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let symbol = arg(inner, 0)?.atom_string().ok_or(YacasError::InvalidArg)?;
    let fact_name = arg(inner, 1)?.atom_string().ok_or(YacasError::InvalidArg)?;
    let fact = Assumption::parse(fact_name).ok_or(YacasError::InvalidArg)?;
    Ok(crate::standard::internal_boolean(
        env,
        env.is_assumed(symbol, fact),
    ))
}

pub fn cmd_is_assumed_value(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let symbol_node = eval(env, arg(inner, 0)?)?;
    let fact_name = arg(inner, 1)?.atom_string().ok_or(YacasError::InvalidArg)?;
    let fact = Assumption::parse(fact_name).ok_or(YacasError::InvalidArg)?;
    let Some(symbol) = symbol_node.atom_string() else {
        return Ok(env.false_atom());
    };
    Ok(crate::standard::internal_boolean(
        env,
        env.is_assumed(symbol, fact),
    ))
}

pub fn cmd_clear_assumptions(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    env.clear_assumptions();
    Ok(env.true_atom())
}

pub fn cmd_push_assumptions(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    env.push_assumptions();
    Ok(env.true_atom())
}

pub fn cmd_pop_assumptions(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if !env.pop_assumptions() {
        return Err(YacasError::InvalidArg);
    }
    Ok(env.true_atom())
}

/// IsList — true when the value is a sublist whose head is List
/// (upstream: InternalIsList).
pub fn cmd_is_list(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    Ok(crate::standard::internal_boolean(env, crate::standard::internal_is_list(&v)))
}

/// IsString — true for string atoms (upstream: InternalIsString(String())).
pub fn cmd_is_string(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    let s = match v.atom_string() {
        Some(s) => s.to_string(),
        None => return Ok(env.false_atom()),
    };
    Ok(crate::standard::internal_boolean(env, crate::standard::internal_is_string(&s)))
}

/// Element chain of a list (kinds; skips the List head), matching upstream
/// SubList().Next() semantics. An empty list `{}` (content chain with only the
/// List head, no elements) -> empty Vec (upstream Reverse({})={} does not error).
fn element_kinds(_env: &mut Environment, node: &Rc<LispObject>) -> Result<Vec<ObjectKind>, YacasError> {
    let sub = node.sublist().ok_or(YacasError::NotList)?;
    match sub.next.as_ref() {
        Some(elems) => Ok(spine_kinds(elems).collect()),
        None => Ok(Vec::new()),
    }
}

/// DestructiveReverse / Reverse — a new List head with the element chain
/// reversed. The destructive variant writes the result back into the first
/// argument's variable slot (upstream reverses the shared chain in place; the
/// contract here is "rebuild + write back").
/// Invariant: MakeVector's body calls `DestructiveReverse(res)` on a non-empty
/// accumulated list — if the destructive Insert did not write back, res would
/// stay empty and this would raise InvalidArg; the two write-backs go together.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispDestructiveReverse.
pub fn cmd_reverse(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let mut kinds = element_kinds(env, &v)?;
    kinds.reverse();
    let mut all: Vec<ObjectKind> = Vec::with_capacity(kinds.len() + 1);
    all.push(ObjectKind::Atom(env.symtab.look_up("List")));
    all.extend(kinds);
    let chain = crate::value::build_list(all).ok_or(YacasError::InvalidArg)?;
    let result = Rc::new(LispObject { next: None, kind: ObjectKind::Sublist(chain) });
    // Destructive variant: upstream reverses the shared chain in place, which is
    // visible through the variable slot; the Rust equivalent is rebuild + write
    // back.
    write_back_arg0(env, inner, &result)?;
    Ok(result)
}

/// DestructiveInsert — rebuilds like the non-destructive Insert (works on empty
/// lists), then writes the result back into the first argument's variable slot.
/// MakeVector's body accumulates through `DestructiveInsert(res,1,…)`; upstream
/// shares the chain in place, and rebuild + write back is the observably
/// equivalent destructive behavior here.
pub fn cmd_destructive_insert(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let result = cmd_insert(env, inner)?;
    write_back_arg0(env, inner, &result)?;
    Ok(result)
}

/// Eval — the argument is pre-evaluated by the evaluator, then InternalEval'd
/// once more inside the command: **two evaluations total**. Rust commands receive
/// raw arguments, so the two evals are explicit here (upstream behavior: within
/// the rule body of `aa:=5`, `Eval(aLeftAssign)` first yields the variable name
/// aa, then 5; a single evaluation would stop at aa).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispEval (Function flag).
pub fn cmd_eval(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let first = eval(env, arg(inner, 0)?)?;
    eval(env, &first)
}

/// MacroSet — evaluate the name argument, take its atom name; evaluate the value
/// argument and assign (upstream LispMacroSetVar -> InternalSetVar(macro=true)).
/// A numeric name -> InvalidArg (upstream CheckArg(!IsNumber(...),1): `1:=2`
/// reports "In function \"MacroSet\" : Invalid argument").
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispMacroSetVar.
pub fn cmd_macro_set(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_value = eval(env, arg(inner, 0)?)?;
    if matches!(name_value.kind, ObjectKind::Number(_)) {
        return Err(YacasError::InvalidArg);
    }
    let name = name_value.atom_string().ok_or(YacasError::InvalidArg)?.clone();
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
pub fn cmd_set_global_lazy_variable(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_value = eval(env, arg(inner, 0)?)?;
    if matches!(name_value.kind, ObjectKind::Number(_)) {
        return Err(YacasError::InvalidArg);
    }
    let name = name_value.atom_string().ok_or(YacasError::InvalidArg)?.clone();
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
pub fn cmd_clear(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    for i in 0..n {
        let s = arg(inner, i)?.atom_string().ok_or(YacasError::InvalidArg)?;
        env.unset_variable(s.clone())?;
    }
    Ok(env.true_atom())
}
pub fn cmd_macro_clear(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    for i in 0..n {
        let v = eval(env, arg(inner, i)?)?;
        let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
        env.unset_variable(s.clone())?;
    }
    Ok(env.true_atom())
}

/// Delete/DestructiveDelete (upstream InternalDelete): 1-based index; removes the
/// idx-th item. idx<1 -> InvalidArg (upstream CheckArg(ind>0,2)); an index past
/// the end of the list -> ListNotLongEnough.
/// Upstream behavior: Delete({1,2,3},1) -> {2,3}; Delete({1,2,3},3) -> {1,2};
/// Delete({1,2,3},5) raises ListNotLongEnough; DestructiveDelete(cc,2) -> {1,3}
/// with the variable cc itself becoming {1,3} (the destructive variant writes
/// back to arg0, as with DestructiveInsert).
fn internal_delete(
    env: &mut Environment,
    inner: &Rc<LispObject>,
    destructive: bool,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let list_node = eval(env, arg(inner, 0)?)?;
    let idx_node = eval(env, arg(inner, 1)?)?;
    let idx = int_text_of(&idx_node)?;
    if idx < 1 {
        return Err(YacasError::InvalidArg);
    }
    let sub = list_node.sublist().ok_or(YacasError::NotList)?;
    let mut kinds: Vec<ObjectKind> = Vec::new();
    let mut cur: Option<&Rc<LispObject>> = sub.next.as_ref();
    let mut i: i64 = 1;
    loop {
        match cur {
            Some(e) => {
                if i != idx {
                    kinds.push(spine_kinds(e).next().expect("elem kind"));
                }
                cur = e.next.as_ref();
                i += 1;
            }
            None => {
                // Reached the end of the list without seeing idx -> index past
                // the end (Delete({1,2,3},5) and similar).
                if i <= idx {
                    return Err(YacasError::ListNotLongEnough);
                }
                break;
            }
        }
    }
    let mut all: Vec<ObjectKind> = Vec::with_capacity(kinds.len() + 1);
    all.push(ObjectKind::Atom(env.symtab.look_up("List")));
    all.extend(kinds);
    let chain = crate::value::build_list(all).ok_or(YacasError::InvalidArg)?;
    let result = Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(chain),
    });
    if destructive {
        write_back_arg0(env, inner, &result)?;
    }
    Ok(result)
}

pub fn cmd_delete(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    internal_delete(env, inner, false)
}
pub fn cmd_destructive_delete(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    internal_delete(env, inner, true)
}

/// BackQuote — the argument is @-substituted and then evaluated. Required by the
/// `...@...` statements inside Macro/Function rule bodies of
/// deffunc.rep/code.ys. Registered under the name "`".
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispBackQuote.
pub fn cmd_back_quote(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let expr = arg(inner, 0)?;
    let mut behaviour = crate::substitute::BackQuoteBehaviour::new(env);
    let substed = crate::standard::internal_substitute(env, expr, &mut behaviour)?;
    eval(env, &substed)
}

/// Protect/UnProtect — symbol name -> protect/unprotect tables. Required by
/// standard.ys top-level `Protect(Nth)`, `UnProtect(%)`, `Protect(%)`.
/// Names are unquoted via symbol_name, so Protect("X") shares the same key as the
/// bare symbol X registered by DefLoad (quoting the name directly would insert
/// the quoted string into the table, which would not match the bare-name
/// UnProtect from internal_use and would leave lazily loaded file symbols
/// protected -> SymbolProtected).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispProtect / LispUnProtect.
pub fn cmd_protect(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let name = symbol_name_of(env, &name_node)?;
    env.protect(&name);
    Ok(env.true_atom())
}
pub fn cmd_unprotect(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_is_protected(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let name = symbol_name_of(env, &name_node)?;
    Ok(if env.is_protected(&name) { env.true_atom() } else { env.false_atom() })
}

/// IsBound — (name) reports whether the name is bound (any local frame or
/// global). Macro flag: the name is not evaluated. Required by numerical.ys's
/// `If(Not IsBound(mathExpThreshold), 500)` — without it If would hit a
/// non-boolean error.
/// See upstream: cyacas/libyacas/src/corefunctions.h LispIsBound.
pub fn cmd_is_bound(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    // Take the name from the raw argument (upstream ARGUMENT(1)->String(), no
    // eval — the question is "is this name bound").
    let s = arg(inner, 0)?.atom_string().ok_or(YacasError::InvalidArg)?;
    Ok(if env.is_bound(s) { env.true_atom() } else { env.false_atom() })
}

/// IsGeneric — reports whether the argument is a Generic object (Pattern/Array
/// etc.).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispIsGeneric.
pub fn cmd_is_generic(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    Ok(if matches!(v.kind, ObjectKind::Generic(_)) { env.true_atom() } else { env.false_atom() })
}

/// GenericTypeName — the type name of a Generic object, as a quoted string;
/// non-Generic -> InvalidArg.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispGenericTypeName.
pub fn cmd_generic_type_name(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    match &v.kind {
        ObjectKind::Generic(g) => {
            let name = g.type_name(); // e.g. "\"Pattern\"" (quotes included)
            Ok(LispObject::atom(env.symtab.look_up(name)))
        }
        _ => Err(YacasError::InvalidArg),
    }
}

/// Pattern'Matches — (pattern, list) reports whether the pattern's generic object
/// matches the list argument (with the List head stripped). Required by
/// localrules.rep runtime validation.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp GenPatternMatches.
pub fn cmd_pattern_matches(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let pattern_node = eval(env, arg(inner, 0)?)?;
    let g = match &pattern_node.kind {
        ObjectKind::Generic(g) => g.clone(),
        _ => return Err(YacasError::InvalidArg),
    };
    let list_node = eval(env, arg(inner, 1)?)?;
    let sub = list_node.sublist().ok_or(YacasError::InvalidArg)?;
    let mut elems: Vec<Rc<LispObject>> = Vec::new();
    let mut cur = sub.next.as_ref();
    while let Some(n) = cur {
        elems.push(n.clone());
        cur = n.next.as_ref();
    }
    match g.matches_pattern(env, &elems)? {
        Some(b) => Ok(if b { env.true_atom() } else { env.false_atom() }),
        None => Err(YacasError::InvalidArg),
    }
}

// ============ Association/Array container commands ============

/// Take the argument's AssociationClass (non-Association -> InvalidArg).
fn as_association(v: &Rc<LispObject>) -> Result<Rc<crate::containers::AssociationClass>, YacasError> {
    match &v.kind {
        ObjectKind::Generic(g) => {
            g.downcast_assoc().ok_or(YacasError::InvalidArg)
        }
        _ => Err(YacasError::InvalidArg),
    }
}

/// Take the argument's ArrayClass (non-Array -> InvalidArg).
fn as_array(v: &Rc<LispObject>) -> Result<Rc<crate::containers::ArrayClass>, YacasError> {
    match &v.kind {
        ObjectKind::Generic(g) => {
            g.downcast_array().ok_or(YacasError::InvalidArg)
        }
        _ => Err(YacasError::InvalidArg),
    }
}

fn list_of(kinds: Vec<ObjectKind>, env: &mut Environment) -> Rc<LispObject> {
    let mut all = Vec::with_capacity(kinds.len() + 1);
    all.push(ObjectKind::Atom(env.symtab.look_up("List")));
    all.extend(kinds);
    let chain = crate::value::build_list(all).expect("list");
    Rc::new(LispObject { next: None, kind: ObjectKind::Sublist(chain) })
}

/// Association'Create — create a new empty association (Generic).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp GenAssociationCreate.
pub fn cmd_assoc_create(_env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 { return Err(YacasError::WrongNumberOfArgs); }
    let g: Rc<dyn crate::value::GenericClass> = Rc::new(crate::containers::AssociationClass::new());
    Ok(LispObject::generic(g))
}
pub fn cmd_assoc_size(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 { return Err(YacasError::WrongNumberOfArgs); }
    let v = eval(env, arg(inner, 0)?)?;
    let a = as_association(&v)?;
    Ok(num_text(a.size().to_string()))
}
pub fn cmd_assoc_contains(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 { return Err(YacasError::WrongNumberOfArgs); }
    let v = eval(env, arg(inner, 0)?)?;
    let a = as_association(&v)?;
    let k = eval(env, arg(inner, 1)?)?;
    Ok(if a.contains(env, &k) { env.true_atom() } else { env.false_atom() })
}
pub fn cmd_assoc_get(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 { return Err(YacasError::WrongNumberOfArgs); }
    let v = eval(env, arg(inner, 0)?)?;
    let a = as_association(&v)?;
    let k = eval(env, arg(inner, 1)?)?;
    match a.get(env, &k) {
        Some(val) => Ok(copy_node(&val)),
        None => Ok(LispObject::atom(env.symtab.look_up("Undefined"))),
    }
}
pub fn cmd_assoc_set(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 3 { return Err(YacasError::WrongNumberOfArgs); }
    let v = eval(env, arg(inner, 0)?)?;
    let a = as_association(&v)?;
    let k = eval(env, arg(inner, 1)?)?;
    let val = eval(env, arg(inner, 2)?)?;
    a.set(env, &k, &val);
    Ok(env.true_atom())
}
pub fn cmd_assoc_drop(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 { return Err(YacasError::WrongNumberOfArgs); }
    let v = eval(env, arg(inner, 0)?)?;
    let a = as_association(&v)?;
    let k = eval(env, arg(inner, 1)?)?;
    Ok(if a.drop_key(env, &k) { env.true_atom() } else { env.false_atom() })
}
pub fn cmd_assoc_keys(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 { return Err(YacasError::WrongNumberOfArgs); }
    let v = eval(env, arg(inner, 0)?)?;
    let a = as_association(&v)?;
    // Upstream's std::map orders keys by InternalStrictTotalOrder; Keys/ToList
    // both emit in that order.
    let mut entries: Vec<(Rc<LispObject>, Rc<LispObject>)> =
        a.pairs.borrow().iter().cloned().collect();
    entries.sort_by(|x, y| {
        if crate::standard::total_less(env, &x.0, &y.0) { std::cmp::Ordering::Less }
        else if crate::standard::total_less(env, &y.0, &x.0) { std::cmp::Ordering::Greater }
        else { std::cmp::Ordering::Equal }
    });
    let kinds: Vec<ObjectKind> = entries.iter()
        .map(|(k, _)| crate::value::spine_kinds(k).next().expect("kind")).collect();
    Ok(list_of(kinds, env))
}
pub fn cmd_assoc_to_list(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 { return Err(YacasError::WrongNumberOfArgs); }
    let v = eval(env, arg(inner, 0)?)?;
    let a = as_association(&v)?;
    let mut entries: Vec<(Rc<LispObject>, Rc<LispObject>)> =
        a.pairs.borrow().iter().cloned().collect();
    entries.sort_by(|x, y| {
        if crate::standard::total_less(env, &x.0, &y.0) { std::cmp::Ordering::Less }
        else if crate::standard::total_less(env, &y.0, &x.0) { std::cmp::Ordering::Greater }
        else { std::cmp::Ordering::Equal }
    });
    let pairs: Vec<Rc<LispObject>> = entries.iter()
        .map(|(k, val)| {
            let mut all = vec![ObjectKind::Atom(env.symtab.look_up("List"))];

            all.push(crate::value::spine_kinds(k).next().expect("k"));
            all.push(crate::value::spine_kinds(val).next().expect("v"));
            let chain = crate::value::build_list(all).expect("pair");
            Rc::new(LispObject { next: None, kind: ObjectKind::Sublist(chain) })
        }).collect();
    let kinds: Vec<ObjectKind> = pairs.iter()
        .map(|p| crate::value::spine_kinds(p).next().expect("p")).collect();
    Ok(list_of(kinds, env))
}

/// Association'Head — errors on an empty association; otherwise returns the
/// first sorted pair {key, value} (upstream AssociationClass::Head takes
/// _map.begin()).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp GenAssociationHead.
pub fn cmd_assoc_head(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 { return Err(YacasError::WrongNumberOfArgs); }
    let v = eval(env, arg(inner, 0)?)?;
    let a = as_association(&v)?;
    let entries: Vec<(Rc<LispObject>, Rc<LispObject>)> =
        a.pairs.borrow().iter().cloned().collect();
    if entries.is_empty() {
        return Err(YacasError::InvalidArg); // upstream CheckArg(Size,1): bad arg number 1
    }
    let mut sorted = entries;
    sorted.sort_by(|x, y| {
        if crate::standard::total_less(env, &x.0, &y.0) { std::cmp::Ordering::Less }
        else if crate::standard::total_less(env, &y.0, &x.0) { std::cmp::Ordering::Greater }
        else { std::cmp::Ordering::Equal }
    });
    let (k, val) = &sorted[0];
    let mut all = vec![ObjectKind::Atom(env.symtab.look_up("List"))];

    all.push(crate::value::spine_kinds(k).next().expect("k"));
    all.push(crate::value::spine_kinds(val).next().expect("v"));
    let chain = crate::value::build_list(all).expect("pair");
    Ok(Rc::new(LispObject { next: None, kind: ObjectKind::Sublist(chain) }))
}

/// Array'Create/Get/Set/Size (upstream GenArray*; 1-based index, size 0 allowed).
/// Signature Array'Create(size, fill): the first argument is the length
/// (upstream GenArrayCreate); fill is only the initial value, per script usage
/// such as Array'Create(7,0).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp GenArrayCreate.
pub fn cmd_array_create(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 { return Err(YacasError::WrongNumberOfArgs); }
    let size_node = eval(env, arg(inner, 0)?)?;
    let size = int_text_of(&size_node)?;
    if size < 0 { return Err(YacasError::InvalidArg); }
    let fill = eval(env, arg(inner, 1)?)?;
    let arr = crate::containers::ArrayClass::new(size as usize, &fill);
    let g: Rc<dyn crate::value::GenericClass> = Rc::new(arr);
    Ok(LispObject::generic(g))
}
pub fn cmd_array_get(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 { return Err(YacasError::WrongNumberOfArgs); }
    let v = eval(env, arg(inner, 0)?)?;
    let arr = as_array(&v)?;
    let idx_node = eval(env, arg(inner, 1)?)?;
    let idx = int_text_of(&idx_node)?;
    // 1-based indexing; non-positive indices are argument errors, not
    // underflow (upstream checks before subtracting).
    if idx < 1 {
        return Err(YacasError::InvalidArg);
    }
    let val = arr.get((idx as usize) - 1)?;
    Ok(copy_node(&val))
}
pub fn cmd_array_set(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 3 { return Err(YacasError::WrongNumberOfArgs); }
    let v = eval(env, arg(inner, 0)?)?;
    let arr = as_array(&v)?;
    let idx_node = eval(env, arg(inner, 1)?)?;
    let idx = int_text_of(&idx_node)?;
    let val = eval(env, arg(inner, 2)?)?;
    // 1-based indexing; non-positive indices are argument errors.
    if idx < 1 {
        return Err(YacasError::InvalidArg);
    }
    arr.set((idx as usize) - 1, &val)?;
    Ok(env.true_atom())
}
pub fn cmd_array_size(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 { return Err(YacasError::WrongNumberOfArgs); }
    let v = eval(env, arg(inner, 0)?)?;
    let arr = as_array(&v)?;
    Ok(num_text(arr.size().to_string()))
}

/// IsInfix/IsPrefix/IsPostfix/IsBodied — look the name up in the corresponding
/// operator table.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispIsInFix etc.
fn op_table_has(table: &std::collections::HashMap<Rc<str>, crate::operators::Operator>, name: &str) -> bool {
    table.keys().any(|k| k.as_ref() == name)
}
pub fn cmd_is_infix(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 { return Err(YacasError::WrongNumberOfArgs); }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s).unwrap_or(s).to_string();
    Ok(if op_table_has(&env.infix, &name) { env.true_atom() } else { env.false_atom() })
}
pub fn cmd_is_prefix(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 { return Err(YacasError::WrongNumberOfArgs); }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s).unwrap_or(s).to_string();
    Ok(if op_table_has(&env.prefix, &name) { env.true_atom() } else { env.false_atom() })
}
pub fn cmd_is_postfix(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 { return Err(YacasError::WrongNumberOfArgs); }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s).unwrap_or(s).to_string();
    Ok(if op_table_has(&env.postfix, &name) { env.true_atom() } else { env.false_atom() })
}
pub fn cmd_is_bodied(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 { return Err(YacasError::WrongNumberOfArgs); }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s).unwrap_or(s).to_string();
    Ok(if op_table_has(&env.bodied, &name) { env.true_atom() } else { env.false_atom() })
}

/// FullForm — evaluates the argument, prints it in FullForm format to the current
/// output using the **local LispPrinter** (not CurrentPrinter) plus a newline,
/// and returns the evaluated argument (not True). Used by scripts such as
/// showq*.ys for debugging.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispFullForm (Function|Fixed).
pub fn cmd_full_form(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let ff = crate::printer::full_form(&v);
    let mut output = env.output_stack.borrow_mut();
    if output.is_empty() {
        output.push(crate::env::OutputBuffer::default());
    }
    let buf = output.last_mut().expect("output");
    buf.text.push_str(&ff);
    buf.text.push('\n');
    drop(output);
    Ok(v)
}

// ======================= Output stream family (Write/WriteString/ToString) =======================
/// Write — evaluates the arguments one by one and prints them to the **current
/// output buffer** with the printer (the space rule is shared across calls,
/// upstream InfixPrinter::iPrevLastChar); returns True. `Write(a,b)` puts a
/// space between arguments ("1 2").
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispWrite (Function|Variable).
pub fn cmd_write(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    // Upstream: with Variable arity the remaining arguments are packed into a
    // List; Rust commands receive all arguments, so evaluate and print one by
    // one. Each argument is evaluated then written to the buffer (no RefCell
    // borrow held across eval, avoiding aliasing conflicts).
    for i in 0..n {
        let v = eval(env, arg(inner, i)?)?;
        let mut output = env.output_stack.borrow_mut();
        if output.is_empty() {
            output.push(crate::env::OutputBuffer::default());
        }
        let buf = output.last_mut().expect("output");
        crate::printer::infix_print_into(env, &v, buf);
        drop(output);
    }
    Ok(env.true_atom())
}

/// WriteString — the argument must be a string atom; its unquoted body is written
/// **verbatim** to the current output (no spaces, contents not evaluated) and the
/// printer's last character is updated. Returns True. Used heavily by io.rep
/// (WriteString("...")).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispWriteString (Function|Fixed).
pub fn cmd_write_string(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    // Function flag: the argument is already evaluated (eval here for the value,
    // consistent with other Function commands).
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    if !crate::standard::internal_is_string(s) {
        return Err(YacasError::InvalidArg);
    }
    let body = crate::standard::internal_unstringify(s).expect("unquote string");
    let mut output = env.output_stack.borrow_mut();
    if output.is_empty() {
        output.push(crate::env::OutputBuffer::default());
    }
    let buf = output.last_mut().expect("output");
    buf.text.push_str(body);
    buf.prev_last_char = body.chars().next_back().unwrap_or(buf.prev_last_char);
    drop(output);
    Ok(env.true_atom())
}

/// ToString — call form `ToString()[body]` (bodied, Macro|Fixed).
/// Pushes a new output buffer capturing the Write/WriteString output produced
/// while the body evaluates; afterwards pops the stack and returns the captured
/// text wrapped as a **quoted** string atom (upstream: stringify(os.str())).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispToString.
pub fn cmd_to_string(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    // bodied: arguments are [head, body] (body is a Prog block or a single
    // expression); Macro does not pre-evaluate arguments.
    let n = arity_of(inner);
    if n < 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let depth = env.push_output();
    // Macro: the body is not evaluated by the evaluator; evaluate it here
    // (capturing its Write/WriteString output).
    let body_result = eval(env, arg(inner, n - 1)?);
    let captured = env.pop_output(depth);
    match body_result {
        Ok(_) => {
            let quoted = format!("\"{}\"", captured.text);
            Ok(crate::value::make_atom(&mut env.symtab, &quoted))
        }
        Err(e) => Err(e),
    }
}

/// ToFile — `ToFile("name")body` (Macro|Fixed + bodied). The first argument
/// (file name) is evaluated and unquoted; pushes an output buffer bound to the
/// file, evaluates the body (its Write/WriteString output accumulates in the
/// buffer), and on pop **truncates and writes the file** (upstream LispLocalFile
/// with ios_base::out: overwrites every time); returns the body's result.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispToFile.
pub fn cmd_to_file(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n < 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if env.secure {
        return Err(YacasError::SecurityBreach);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let s = name_node.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s).unwrap_or(s).to_string();
    let depth = env.push_output_to(Some(name));
    let body_result = eval(env, arg(inner, n - 1)?);
    env.pop_output(depth); // flush to disk (truncating write)
    body_result
}

/// ToStdout — `ToStdout()body` (Macro|Fixed + bodied). The body's output is
/// forced to the **initial stdout** (bypassing ToString/ToFile capture; upstream
/// LispLocalOutput(*iInitialOutput)). In the library scenario there is no real
/// stdout, so this is equivalent to a discard sink: push a discard buffer ->
/// eval body -> pop and drop (text neither hits disk nor any capture). Returns
/// the body's result.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispToStdout.
pub fn cmd_to_stdout(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n < 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let depth = env.push_output();
    let body_result = eval(env, arg(inner, n - 1)?);
    env.pop_output(depth);
    body_result
}

// ======================= SystemCall / SystemName =======================
/// SystemCall — the argument must be a string; it is unquoted and executed via
/// the shell (system() semantics); exit code == 0 -> True, otherwise False.
/// CheckSecure: raises SecurityBreach when env.secure. Required by the
/// mysql/rm toolchains in io.rep/html.rep.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispSystemCall (Function|Fixed).
pub fn cmd_system_call(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if env.secure {
        return Err(YacasError::SecurityBreach);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let cmd = crate::standard::internal_unstringify(s).unwrap_or(s).to_string();
    // system() semantics: executed via sh -c; exit code 0 -> True.
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .status();
    match status {
        Ok(st) => {
            if st.success() {
                Ok(env.true_atom())
            } else {
                Ok(env.false_atom())
            }
        }
        Err(_) => Ok(env.false_atom()),
    }
}

/// SystemName — returns the platform name as a quoted string
/// (Linux/MacOSX/Windows/Unknown).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispSystemName (Function|Fixed, arity 0).
pub fn cmd_system_name(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "MacOSX"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else {
        "Unknown"
    };
    Ok(crate::value::make_atom(&mut env.symtab, &format!("\"{name}\"")))
}

/// TmpFile — mkstemp semantics: creates a unique temporary file (success only if
/// it does not exist, looping over random suffixes) and returns the **quoted**
/// path string (upstream oracle: "/tmp/yacas-uuedFA"). CheckSecure. Used by the
/// plots backends mostly for intermediate control/data files. The file is kept
/// after creation (consumers use ToFile/FromFile on it).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispTmpFile (Function|Fixed, arity 0).
pub fn cmd_tmp_file(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if env.secure {
        return Err(YacasError::SecurityBreach);
    }
    use std::io::Write;
    // mkstemp("/tmp/yacas-XXXXXX") semantics: 6 random chars; create_new guarantees uniqueness.
    let dir = "/tmp";
    let chars: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = 0u64;
    // Seed from time + address (no extra dependencies needed)
    rng ^= std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64).unwrap_or(0);
    rng ^= (env as *const Environment) as u64;
    for _attempt in 0..100 {
        let mut suffix = String::new();
        for _ in 0..6 {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let idx = ((rng >> 33) % chars.len() as u64) as usize;
            suffix.push(chars[idx] as char);
        }
        let path = format!("{dir}/yacas-{suffix}");
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                let _ = f.write_all(b"");
                drop(f);
                return Ok(crate::value::make_atom(&mut env.symtab, &format!("\"{path}\"")));
            }
            Err(_) => continue, // already exists, retry
        }
    }
    Err(YacasError::FileNotFound)
}

/// Secure — `Secure(body)` (Macro|Fixed): sets env.secure=true while the body
/// is evaluated (restored on exit, like upstream LispSecureFrame: set on
/// entry, restore the previous value on exit). CheckSecure commands called
/// inside the body (SystemCall/ToFile/FromFile/Load etc.) then raise
/// SecurityBreach. The openmath `Secure(Eval(...))` pattern depends on this.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispSecure.
pub fn cmd_secure(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n < 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let prev = env.secure;
    env.secure = true;
    // Macro: the body is not evaluated by the dispatcher; eval here (the secure
    // flag is restored manually so it recovers even on error)
    let result = eval(env, arg(inner, n - 1)?);
    env.secure = prev;
    result
}

// ==================== Input stream family FromString/Read/ReadToken ====================
/// FromString — `FromString("str")body` (Macro|Fixed + bodied): evaluates
/// argument 1 to a string, pushes a new input stream (the tokenizer holds the
/// string + cursor), evaluates the body (its Read/ReadToken read from and
/// advance that string), then pops the stack to restore. Returns the body's
/// result. The yacasinit REPL `FromString(input)Read()` depends on this.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispFromString.
pub fn cmd_from_string(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n < 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let s = name_node.atom_string().ok_or(YacasError::InvalidArg)?;
    let text = crate::standard::internal_unstringify(s).unwrap_or(s).to_string();
    let mut tok = crate::tokenizer::Tokenizer::new(&text);
    tok.xml = env.xml_tokenizer.get();
    env.input_stack.borrow_mut().push(Some(tok));
    // Input state = "String" (like upstream LispFromString SetTo/RestoreFrom)
    let old_file = env.input_file.borrow().clone();
    *env.input_file.borrow_mut() = "String".to_string();
    let body_result = eval(env, arg(inner, n - 1)?);
    *env.input_file.borrow_mut() = old_file;
    env.input_stack.borrow_mut().pop();
    body_result
}

/// FromFile — `FromFile("name")body` (Macro|Fixed + bodied): evaluates the
/// argument to a file name, looks it up via input_directories (bare names:
/// CWD first, like the LispLocalFile read path); open failure -> FileNotFound.
/// Pushes a file-content input stream, evaluates the body (Read/ReadToken read
/// from the file), pops to restore. Returns the body's result. CheckSecure.
/// The sql toolchain's FromFile(...)Read() in io.rep depends on this.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispFromFile.
pub fn cmd_from_file(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n < 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if env.secure {
        return Err(YacasError::SecurityBreach);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let s = name_node.atom_string().ok_or(YacasError::InvalidArg)?;
    let fname = crate::standard::internal_unstringify(s).unwrap_or(s).to_string();
    let path = crate::standard::internal_find_file(env, &fname)
        .ok_or(YacasError::FileNotFound)?;
    let text = std::fs::read_to_string(&path).map_err(|_| YacasError::FileNotFound)?;
    let mut tok = crate::tokenizer::Tokenizer::new(&text);
    tok.xml = env.xml_tokenizer.get();
    env.input_stack.borrow_mut().push(Some(tok));
    // Input state = file name (like upstream LispFromFile SetTo/RestoreFrom)
    let old_file = env.input_file.borrow().clone();
    *env.input_file.borrow_mut() = fname.clone();
    let body_result = eval(env, arg(inner, n - 1)?);
    *env.input_file.borrow_mut() = old_file;
    env.input_stack.borrow_mut().pop();
    body_result
}
/// Read (Function|Fixed, arity 0): reads one expression from the current
/// input stream (like upstream InfixParser::Parse: up to `;` or EOF; the
/// input string must end with `;` or "Error parsing expression" is raised).
/// Errors when no input stream is active.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispRead.
pub fn cmd_read(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    // Take the tokenizer off the stack top (releasing the borrow), parse one
    // expression, then put it back (cursor advanced).
    let mut tok = {
        let mut stack = env.input_stack.borrow_mut();
        stack
            .last_mut()
            .ok_or(YacasError::Generic("Read: no active input stream".to_string()))?
            .take()
            .ok_or(YacasError::Generic("Read: input tokenizer missing".to_string()))?
    };
    let expr = crate::parser::parse_one(env, &mut tok);
    // Put back (even on parse error; the cursor stays at the failure point)
    env.input_stack.borrow_mut().last_mut().expect("input").replace(tok);
    match expr {
        Ok(Some(e)) => {
            // EndOfFile atom -> return the EndOfFile symbol (upstream Read at end of stream)
            if e.atom_string().map(|s| s.as_ref() == "EndOfFile").unwrap_or(false) {
                Ok(LispObject::atom(env.symtab.look_up("EndOfFile")))
            } else {
                // Parse only, do not evaluate: FromString("x;")Read() returns x unevaluated.
                Ok(e)
            }
        }
        Ok(None) => Err(YacasError::Generic("Read: parse error".to_string())),
        Err(_) => Err(YacasError::Generic("Error parsing expression".to_string())),
    }
}

/// ReadToken (Function|Fixed, arity 0): reads one token from the current
/// input stream (verbatim atom; end of stream -> EndOfFile). Errors when no
/// input stream is active.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispReadToken.
pub fn cmd_read_token(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let mut tok = {
        let mut stack = env.input_stack.borrow_mut();
        stack
            .last_mut()
            .ok_or(YacasError::Generic("ReadToken: no active input stream".to_string()))?
            .take()
            .ok_or(YacasError::Generic("ReadToken: input tokenizer missing".to_string()))?
    };
    let token = tok.next_token();
    env.input_stack.borrow_mut().last_mut().expect("input").replace(tok);
    let token = token.unwrap_or_default();
    if token.is_empty() {
        Ok(LispObject::atom(env.symtab.look_up("EndOfFile")))
    } else {
        // Build the atom verbatim (like upstream LispAtom::New(*result); numbers/strings created by content)
        Ok(crate::value::atom_or_number(&mut env.symtab, &token))
    }
}

/// Check (Macro|Fixed): (pred, message) -> evaluates the predicate; if not
/// True, evaluates the message (must be a string) and raises it as the error
/// (like upstream LispErrUser). Used throughout the scripts for argument
/// validation (e.g. limit.rep's Check(IsAtom(var),"...")).
/// See upstream: cyacas/libyacas/src/corefunctions.h LispCheck.
pub fn cmd_check(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let pred = eval(env, arg(inner, 0)?)?;
    if !crate::standard::is_true(env, &pred) {
        let msg = eval(env, arg(inner, 1)?)?;
        let text = match msg.atom_string() {
            Some(s) => crate::standard::internal_unstringify(s).unwrap_or(s).to_string(),
            None => {
                // Non-string message (like CheckArgIsString): quoted-string atoms or numbers are both InvalidArg
                return Err(YacasError::InvalidArg);
            }
        };
        return Err(YacasError::Generic(text));
    }
    Ok(pred)
}

// ==================== CustomEval family (debugger) ====================
/// CustomEval (Macro|Fixed, arity 4): (entercb, leavecb, errorcb, expr) —
/// installs the debugger state (enter/leave/error callbacks), evaluates expr
/// (each eval step goes through the debug hooks calling the Enter/Leave
/// callbacks), clears the debugger afterwards, and returns expr's result.
/// debug.rep's TraceExp/Debug/TraceRule depend on this.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispCustomEval.
pub fn cmd_custom_eval(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 4 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    // Macro: arguments are stored unevaluated as callbacks/expression
    let enter = arg(inner, 0)?.clone();
    let leave = arg(inner, 1)?.clone();
    let error = arg(inner, 2)?.clone();
    let expr = arg(inner, 3)?.clone();
    let state = crate::env::DebuggerState {
        enter_cb: enter,
        leave_cb: leave,
        error_cb: error,
        stopped: false,
        top_expr: None,
        top_result: None,
        in_callback: false,
    };
    *env.debugger.borrow_mut() = Some(state);
    let result = eval(env, &expr);
    *env.debugger.borrow_mut() = None;
    result
}

/// CustomEval'Expression (Function|Fixed, arity 0): returns the sub-expression
/// currently being evaluated (for use inside debug callbacks); errors when no
/// debugger is active.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispCustomEvalExpression.
pub fn cmd_custom_eval_expression(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let d = env.debugger.borrow();
    let state = d.as_ref().ok_or_else(|| {
        YacasError::Generic("Trying to get CustomEval results while not in custom evaluation".to_string())
    })?;
    match &state.top_expr {
        Some(e) => Ok(copy_node(e)),
        None => Ok(LispObject::atom(env.symtab.look_up("Undefined"))),
    }
}

/// CustomEval'Result (Function|Fixed, arity 0): returns the evaluation result
/// of the current sub-expression (for use in the Leave callback); errors when
/// no debugger is active.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispCustomEvalResult.
pub fn cmd_custom_eval_result(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let d = env.debugger.borrow();
    let state = d.as_ref().ok_or_else(|| {
        YacasError::Generic("Trying to get CustomEval results while not in custom evaluation".to_string())
    })?;
    match &state.top_result {
        Some(r) => Ok(copy_node(r)),
        None => Ok(LispObject::atom(env.symtab.look_up("Undefined"))),
    }
}

/// CustomEval'Locals (Function|Fixed, arity 0): returns the list of currently
/// visible local variable names (like CurrentLocals: from the innermost frame
/// outwards, stopping at a fenced frame). Returns even without an active
/// debugger (upstream returns {} for a standalone call).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispCustomEvalLocals.
pub fn cmd_custom_eval_locals(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    // Collect visible local names (like CurrentLocals: innermost frame outwards, stop at a fenced frame)
    let mut names: Vec<Rc<str>> = Vec::new();
    let mut frame = env.locals.as_ref();
    while let Some(f) = frame {
        let mut t = f.first.as_ref();
        while let Some(node) = t {
            names.push(node.variable.clone());
            t = node.next.as_ref();
        }
        if f.fenced {
            break;
        }
        frame = f.next.as_ref();
    }
    // Deduplicate (CurrentLocals collects per frame, so names may repeat; order-preserving dedup)
    let mut seen = std::collections::HashSet::new();
    names.retain(|n| seen.insert(n.clone()));
    let kinds: Vec<ObjectKind> = names
        .iter()
        .map(|n| ObjectKind::Atom(n.clone()))
        .collect();
    Ok(list_of(kinds, env))
}

/// CustomEval'Stop (Function|Fixed, arity 0): sets debugger.stopped (later
/// evals raise to abort); errors when no debugger is active. Returns True.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispCustomEvalStop.
pub fn cmd_custom_eval_stop(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let mut d = env.debugger.borrow_mut();
    let state = d.as_mut().ok_or_else(|| {
        YacasError::Generic("Trying to get CustomEval results while not in custom evaluation".to_string())
    })?;
    state.stopped = true;
    Ok(env.true_atom())
}

/// MacroLocal (like upstream LispNewLocal; Function flags: arguments are
/// evaluated first and taken as atom names for new locals; the only difference
/// from Local is the pre-evaluation of arguments). controlflow.rep's ForEach
/// macro body `MacroLocal(item)` depends on this — item is the loop variable
/// name.
/// See upstream: cyacas/libyacas/src/corefunctions.h LispNewLocal.
pub fn cmd_macro_local(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    for i in 0..n {
        let v = eval(env, arg(inner, i)?)?;
        let name = v.atom_string().ok_or(YacasError::InvalidArg)?.clone();
        env.new_local(name, None);
    }
    Ok(env.true_atom())
}

/// TrapError (Macro|Fixed): (body, handler) -> try to evaluate body; on error
/// (the error message lands in env.error_output) and a non-empty buffer,
/// evaluate the handler (typically Set(errorString,GetCoreError())) and clear
/// the buffer. The N macro `TrapError(Set(result,@expr), Set(errorString,GetCoreError()))`
/// depends on this.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispTrapError.
pub fn cmd_trap_error(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let body_result = (|| -> Result<Rc<LispObject>, YacasError> {
        let r = eval(env, arg(inner, 0)?)?;
        Ok(r)
    })();
    // If the body errors, the error text is written into the error_output
    // buffer here (upstream writes iErrorOutput via HandleError).
    let body_ok = match body_result {
        Ok(r) => Some(r),
        Err(e) => {
            let msg = format!("{e:?}");
            *env.error_output.borrow_mut() = msg;
            None
        }
    };
    if !env.error_output.borrow().is_empty() {
        // Error present: run the handler (e.g. Set(errorString, GetCoreError())); clear the buffer
        let handler = eval(env, arg(inner, 1)?);
        *env.error_output.borrow_mut() = String::new();
        return handler;
    }
    // No error: return the body's result
    Ok(body_ok.expect("body must be Ok when no error"))
}

/// GetCoreError: returns the error_output buffer string as a quoted string atom.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispGetCoreError.
pub fn cmd_get_core_error(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let s = env.error_output.borrow().clone();
    // Like upstream: a string atom (including quotes)
    let quoted = format!("\"{s}\"");
    Ok(LispObject::atom(env.symtab.look_up(&quoted)))
}

/// Builtin'Assoc (key, assoc-list): walks the list and returns the first item
/// whose head element == key; returns the Empty atom when not found.
/// constants.rep's CachedConstant macro body
/// `Equals(Builtin'Assoc(C'name,Eval(C'cache)),Empty)` depends on this:
/// without it the value is kept, If sees a non-boolean, the cache entry is
/// skipped, and N(Pi,10) fails to simplify.
/// See upstream: cyacas/libyacas/src/corefunctions.h YacasBuiltinAssoc.
pub fn cmd_builtin_assoc(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let key = eval(env, arg(inner, 0)?)?;
    let list = eval(env, arg(inner, 1)?)?;
    let sub = list.sublist().ok_or(YacasError::InvalidArg)?;
    let mut cur = sub.next.as_ref();
    while let Some(item) = cur {
        if let Some(isub) = item.sublist() {
            if let Some(first) = isub.next.as_ref() {
                if crate::standard::internal_equals(env, &key, first) {
                    return Ok(item.clone());
                }
            }
        }
        cur = item.next.as_ref();
    }
    // Not found -> Empty atom (like upstream LispAtom::New("Empty"))
    Ok(LispObject::atom(env.symtab.look_up("Empty")))
}

/// LocalSymbols: the leading arguments are symbol names and the last argument
/// is the block body — atoms in the body that match the name table are all
/// renamed to unique new names (LocalSymbolBehaviour), then evaluated.
/// Zero-based arguments: symbol count = arity - 1 (upstream nrArguments
/// includes the head). standard.ys's Numeric/Verbose blocks and yacasinit.ys's
/// Input/Output/REP blocks depend on this.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispLocalSymbols.
pub fn cmd_local_symbols(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n < 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let nr_symbols = n - 1;
    let mut names: Vec<Rc<str>> = Vec::with_capacity(nr_symbols);
    for i in 0..nr_symbols {
        // Name arguments are NOT evaluated (like upstream LispLocalSymbols:
        // Argument(...)->String() directly). Evaluating them could hit a local
        // of the same name inside a rule frame (e.g. x -> 2) and yield
        // InvalidArg. Non-atom names (e.g. the args literal list in a
        // TemplateFunction macro body) are tolerated upstream (String() yields
        // a harmless unique symbol that no rename entry matches) — skipping
        // them here is equivalent and harmless.
        let raw = arg(inner, i)?;
        if let Some(s) = raw.atom_string() {
            names.push(s.clone());
        } else if let ObjectKind::Number(num) = &raw.kind {
            names.push(Rc::from(num.string()));
        } // Sublist/Generic names: skipped (upstream tolerates them too; nothing matches).
    }
    let body = arg(inner, n - 1)?;
    let mut behaviour = crate::substitute::LocalSymbolBehaviour::new(env, &names);
    let substed = crate::standard::internal_substitute(env, body, &mut behaviour)?;
    eval(env, &substed)
}

/// ApplyPure (oper, args-list): pure application.
/// - oper is a string: dispatches directly to the function of that name with
///   the *original argument elements* of args-list (arguments are not
///   pre-evaluated; like InternalApplyString — deffunc.rep TemplateFunction's
///   `ApplyPure("LocalSymbols",arglist)` depends on this: LocalSymbols is a
///   core command and receives the unevaluated formal parameter chain).
/// - oper is a {params,body} sublist: the head is not a string -> the
///   evaluator's InternalApplyPure (lambda) path.
/// - args must be a list node (like upstream CheckArg(args->SubList(),...)).
///
/// See upstream: cyacas/libyacas/src/mathcommands3.cpp LispApplyPure.
pub fn cmd_apply_pure(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let oper_node = eval(env, arg(inner, 0)?)?;
    let args_node = eval(env, arg(inner, 1)?)?;
    // Argument elements = the args content chain *without* the List/function
    // head (like upstream LispApplyPure `(*args->SubList())->Nixed()`:
    // SubList() is the first element of the content chain and Nixed() walks
    // the rest). FlatCopy yields Sublist(List a b), so the arguments must be
    // [a, b] (List head removed) — otherwise LocalSymbols would treat "List"
    // as the first name to rename and pollute the UniqueSymbol head.
    let args_sub = args_node.sublist().ok_or(YacasError::NotList)?;
    let mut elems: Vec<Rc<LispObject>> = Vec::new();
    let mut cur = args_sub.next.as_ref().cloned();
    while let Some(n) = cur {
        elems.push(n.clone());
        cur = n.next.as_ref().cloned();
    }
    match oper_node.atom_string() {
        Some(name) => {
            // Named pure application: build a (name, e1..en) call and dispatch
            // directly to the core command (like the evaluator; arguments are
            // passed as-is — the core command decides evaluation/holding).
            // TemplateFunction/LocalSymbols macro bodies rely on this path
            // (deffunc.rep's `ApplyPure("LocalSymbols",arglist)`).
            // The head atom must be unquoted via symbol_name (upstream
            // InternalApplyString calls SymbolName before building the head):
            // with oper = "Deriv" (a quoted string atom), the call head would
            // otherwise keep the quotes and get_user_function would not find it.
            let key = crate::standard::symbol_name(env, name.as_ref());
            let head = LispObject::atom(env.symtab.look_up(&key));
            let call = crate::userfunc::rebuild_call(&head, &elems);
            if let Some(cmd) = env.core_commands.get(&key) {
                // Like the evaluator's convention (commands receive the content
                // chain): pass the call's content chain directly, otherwise
                // arity_of(wrapper) = 0 and LocalSymbols etc. would wrongly
                // report WrongNumberOfArgs.
                let content = call.sublist().ok_or(YacasError::NotList)?;
                return (cmd.func)(env, content);
            }
            if let Some(_f) = crate::evaluator::get_user_function(env, &call)? {
                eval(env, &call)
            } else {
                // return_un_evaluated receives the content chain (like the
                // evaluator's convention of passing the sub_list content);
                // passing the wrapper node would wrap it in another Sublist.
                let content = call.sublist().ok_or(YacasError::NotList)?;
                crate::standard::return_un_evaluated(env, content)
            }
        }
        None => {
            // {params,body} lambda: the head is not a string -> the evaluator's InternalApplyPure branch.
            let call = crate::userfunc::rebuild_call(&oper_node, &elems);
            eval(env, &call)
        }
    }
}

/// FlatCopy: shallow-copies the whole chain into a new sublist. The argument
/// is evaluated first (upstream: FlatCopy(a) works on atoms holding list
/// values — the atom resolves to its value; literal evaluation is identity).
/// The result must be a sublist (like CheckArgIsList). deffunc.rep's
/// TemplateFunction macro body depends on it: arglist:=FlatCopy(args) ->
/// DestructiveAppend(arglist,...) -> ApplyPure("LocalSymbols",arglist)
/// (args/arglist are macro-frame local atoms).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispFlatCopy.
pub fn cmd_flat_copy(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    // Like upstream LispFlatCopy: InternalFlatCopy copies the *entire content
    // chain (including the List/function head)* and wraps it in a new sublist.
    // Keeping the head means every chain is uniformly head-carrying.
    let val = eval(env, arg(inner, 0)?)?;
    let first = val.sublist().ok_or(YacasError::NotList)?;
    let spine = crate::value::copy_spine(first);
    Ok(LispObject::new(ObjectKind::Sublist(spine)))
}

/// MathSubtract: (a, b) -> numeric a - b (the MathAdd family counterpart).
/// standard.ys's `--` function body depends on this.
pub fn cmd_math_subtract(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let a = eval(env, arg(inner, 0)?)?;
    let b = eval(env, arg(inner, 1)?)?;
    let (an, bn, any_float) = match (&a.kind, &b.kind) {
        (ObjectKind::Number(na), ObjectKind::Number(nb)) => {
            // float_at(env precision) + sub(env precision): like the C++
            // LispSubtract path via AddNumericTotal (the mantissa is trimmed
            // to the global precision).
            (
                na.float_at(env.precision()),
                nb.float_at(env.precision()),
                na.is_float() || nb.is_float(),
            )
        }
        _ => return Err(YacasError::InvalidArg),
    };
    let num = crate::value::LispNumber::from_float_flag(an.sub(&bn, env.precision()), any_float);
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Number(num),
    }))
}

/// Bitwise/modulo ops (like upstream BitAnd/BitOr/Mod; standard.ys's &—|—%
/// rules call these). Arguments are converted to integers, then combined via
/// i64 bitwise ops.
fn two_ints(env: &mut Environment, inner: &Rc<LispObject>) -> Result<(i64, i64), YacasError> {
    let a = eval(env, arg(inner, 0)?)?;
    let b = eval(env, arg(inner, 1)?)?;
    let an = int_text_of(&a)?;
    let bn = int_text_of(&b)?;
    Ok((an, bn))
}
fn int_number(v: i64) -> Rc<LispObject> {
    let num = crate::value::LispNumber::from_text(v.to_string());
    Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Number(num),
    })
}
pub fn cmd_bit_and(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let (a, b) = two_ints(env, inner)?;
    Ok(int_number(a & b))
}
pub fn cmd_bit_or(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let (a, b) = two_ints(env, inner)?;
    Ok(int_number(a | b))
}
pub fn cmd_mod(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let a = eval(env, arg(inner, 0)?)?;
    let b = eval(env, arg(inner, 1)?)?;
    let at = a.number_string().or_else(|| a.atom_string().map(|s| s.to_string()))
        .ok_or(YacasError::InvalidArg)?;
    let bt = b.number_string().or_else(|| b.atom_string().map(|s| s.to_string()))
        .ok_or(YacasError::InvalidArg)?;
    // Fast path: i64; on overflow fall back to Nat big integers (PollardRho's
    // Mod(PollardRhoPolynomial(x),n) depends on big-int mod)
    if let (Ok(x), Ok(y)) = (at.trim().parse::<i64>(), bt.trim().parse::<i64>()) {
        if y == 0 {
            return Err(YacasError::InvalidArg);
        }
        return Ok(int_number(x % y));
    }
    let x = crate::number::nat::Nat::from_decimal(at.trim_start_matches('-')).ok_or(YacasError::InvalidArg)?;
    let y = crate::number::nat::Nat::from_decimal(bt.trim_start_matches('-')).ok_or(YacasError::InvalidArg)?;
    let (_, r) = x.divrem(&y).ok_or(YacasError::InvalidArg)?;
    Ok(num_text(r.to_decimal()))
}
/// Shift (like upstream LispShiftLeft/LispShiftRight). Positive-integer domain
/// semantics (yacas BigNumber): left shift = *2^k, right shift = /2^k (floor).
/// Overflow (shift amount >= 64) yields 0 under the engine's finite-precision
/// model (positive values shift to 0; unrepresentably large results are not
/// modeled). PositiveIntPower's `n <-- ShiftRight(n,1)` depends on this.
fn cmd_shift(env: &mut Environment, inner: &Rc<LispObject>, left: bool) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let (a, k) = two_ints(env, inner)?;
    if k < 0 {
        return Err(YacasError::InvalidArg);
    }
    let v = if left {
        if k >= 64 { 0 } else { a << k }
    } else {
        if k >= 64 { 0 } else { a >> k }
    };
    Ok(int_number(v))
}
pub fn cmd_shift_left(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    cmd_shift(env, inner, true)
}
pub fn cmd_shift_right(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    cmd_shift(env, inner, false)
}

/// Numeric argument to Float (same argument pattern as cmd_math_subtract).
/// Returns (Float, type float flag).
fn arg_float_flag(env: &mut Environment, inner: &Rc<LispObject>, i: usize) -> Result<(crate::number::float::Float, bool), YacasError> {
    let v = eval(env, arg(inner, i)?)?;
    match &v.kind {
        ObjectKind::Number(num) => Ok((num.float(), num.is_float())),
        _ => Err(YacasError::InvalidArg),
    }
}
/// Variant that ignores the type flag (for commands whose results are always floats).
fn arg_float(env: &mut Environment, inner: &Rc<LispObject>, i: usize) -> Result<crate::number::float::Float, YacasError> {
    Ok(arg_float_flag(env, inner, i)?.0)
}
fn num_of(f: crate::number::float::Float) -> Rc<LispObject> {
    num_of_flag(f, true)
}
fn num_of_flag(f: crate::number::float::Float, is_float: bool) -> Rc<LispObject> {
    Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Number(crate::value::LispNumber::from_float_flag(f, is_float)),
    })
}

/// Math core family (like upstream corefunctions.h; stubs/base.rep rule bodies
/// depend on these).
pub fn cmd_math_negate(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let (x, fl) = arg_float_flag(env, inner, 0)?;
    Ok(num_of_flag(x.negate(), fl))
}
pub fn cmd_math_abs(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let z = crate::number::float::Float::from_decimal("0").expect("0");
    let (x, fl) = arg_float_flag(env, inner, 0)?;
    Ok(num_of_flag(if x.less_than(&z) { x.negate() } else { x }, fl))
}
pub fn cmd_math_sign(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let z = crate::number::float::Float::from_decimal("0").expect("0");
    let x = arg_float(env, inner, 0)?;
    let v = if x.is_zero() {
        0
    } else if x.less_than(&z) {
        -1
    } else {
        1
    };
    Ok(num_text(v.to_string()))
}
/// Number text atom (like LispNumber::from_text; builds integer results for sign/floor/ceil etc.).
fn num_text(s: String) -> Rc<LispObject> {
    LispObject::new(ObjectKind::Number(crate::value::LispNumber::from_text(s)))
}

// ==================== FastIsPrime / MathFac ====================
/// FastIsPrime (Function|Fixed): small-prime table lookup (upstream
/// primes_table_check, MAX_SMALL_PRIME = 65537). Semantics: p == 0 -> 65537;
/// 2 -> 1; < 2, > 65537, or even -> 0; odd table lookup -> 1/0.
/// numbers.rep's IsSmallPrime / IsPrime(n<=FastIsPrime(0)) depend on this.
/// See upstream: cyacas/libyacas/src/mathcommands3.cpp LispFastIsPrime.
pub fn cmd_fast_is_prime(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let p: u64 = int_text_of(&v)? as u64;
    let r = fast_is_prime_check(p);
    Ok(num_text(r.to_string()))
}

/// Port of upstream primes_table_check (platmath.cpp). Sieve of Eratosthenes
/// computed once, cached in a thread_local.
fn fast_is_prime_check(p: u64) -> u64 {
    const MAX: u64 = 65537;
    if p == 0 {
        return MAX;
    }
    if p == 2 {
        return 1;
    }
    if !(2..=MAX).contains(&p) || (p & 1) == 0 {
        return 0;
    }
    use std::cell::RefCell;
    thread_local! {
        // Odd composite table (like upstream bitset<MAX/2+1>: index p/2, p odd)
        static COMP: RefCell<Option<Vec<bool>>> = const { RefCell::new(None) };
    }
    COMP.with(|c| {
        let mut guard = c.borrow_mut();
        let tbl = guard.get_or_insert_with(|| {
            let half = (MAX / 2 + 1) as usize;
            let mut comp = vec![false; half];
            let mut i: u64 = 3;
            while i < MAX {
                if !comp[(i / 2) as usize] {
                    let mut j: u64 = 3;
                    while j < MAX / i {
                        comp[((i * j) / 2) as usize] = true;
                        j += 2;
                    }
                }
                i += 2;
            }
            comp
        });
        if tbl[(p / 2) as usize] { 0 } else { 1 }
    })
}

/// MathFac (like upstream LispFac -> LispFactorial; Function|Fixed): exact
/// integer n! (accumulated with big Nat integers). sums.rep's
/// `20 # ((n_IsPositiveInteger)!)` rule body `MathFac(n)` depends on this.
/// See upstream: cyacas/libyacas/src/mathcommands3.cpp LispFac.
pub fn cmd_math_fac(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let n: i64 = int_text_of(&v)?;
    if n < 0 {
        return Err(YacasError::InvalidArg);
    }
    let mut acc = crate::number::nat::Nat::from_decimal("1").expect("1");
    let mut k: i64 = 2;
    while k <= n {
        let kk = crate::number::nat::Nat::from_decimal(&k.to_string()).expect("k");
        acc = acc.mul(&kk);
        k += 1;
    }
    Ok(num_text(acc.to_decimal()))
}

/// MathFloor/MathCeil (like upstream LispFloor/LispCeil): via Float.floor/ceil.
/// stubs' Floor/Ceil/Round rule bodies and the IsZero chain (MathFloor(N(x+0.5))
/// etc.) depend on these.
pub fn cmd_math_floor(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    // Upstream behavior: Floor/MathFloor always return an integer-typed
    // result (Floor(3.5) -> 3, no decimal point)
    Ok(num_of_flag(arg_float(env, inner, 0)?.floor(), false))
}
pub fn cmd_math_ceil(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    Ok(num_of_flag(arg_float(env, inner, 0)?.ceil(), false))
}

/// MathDiv (like upstream LispDiv -> BigNumber::Divide -> ZZ::operator/=):
/// integer quotient truncated **toward zero** (MathDiv(-7,3) = -2, not -3;
/// Rem relies on n - m*Div(n,m) giving -1). Operands are arbitrary-precision
/// integers (FactorizeInt/ContFrac/simplify paths divide numbers beyond the
/// i64 range); a fast i64 path covers the common small-operand case.
pub fn cmd_math_div(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let a = eval(env, arg(inner, 0)?)?;
    let b = eval(env, arg(inner, 1)?)?;
    let at = a
        .number_string()
        .or_else(|| a.atom_string().map(|s| s.to_string()))
        .ok_or(YacasError::InvalidArg)?
        .trim()
        .to_string();
    let bt = b
        .number_string()
        .or_else(|| b.atom_string().map(|s| s.to_string()))
        .ok_or(YacasError::InvalidArg)?
        .trim()
        .to_string();
    if let (Ok(x), Ok(y)) = (at.parse::<i64>(), bt.parse::<i64>()) {
        if y == 0 {
            return Err(YacasError::InvalidArg);
        }
        if !(x == i64::MIN && y == -1) {
            // i64::MIN / -1 溢出(wrapping_div 会静默回绕成 i64::MIN),回落大数路径
            return Ok(int_number(x.wrapping_div(y)));
        }
    }
    let neg = at.starts_with('-') ^ bt.starts_with('-');
    let x = crate::number::nat::Nat::from_decimal(at.trim_start_matches('-'))
        .ok_or(YacasError::InvalidArg)?;
    let y = crate::number::nat::Nat::from_decimal(bt.trim_start_matches('-'))
        .ok_or(YacasError::InvalidArg)?;
    if y.is_zero() {
        return Err(YacasError::InvalidArg);
    }
    let (q, _) = x.divrem(&y).ok_or(YacasError::InvalidArg)?;
    let text = q.to_decimal();
    let text = if neg && !q.is_zero() {
        format!("-{text}")
    } else {
        text
    };
    Ok(num_text(text))
}

/// MathGcd (like upstream LispGcd): Euclidean algorithm on the absolute values
/// of both arguments.
/// See upstream: cyacas/libyacas/src/mathcommands3.cpp LispGcd.
pub fn cmd_math_gcd(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let a = eval(env, arg(inner, 0)?)?;
    let b = eval(env, arg(inner, 1)?)?;
    let at = a.number_string().or_else(|| a.atom_string().map(|s| s.to_string()))
        .ok_or(YacasError::InvalidArg)?;
    let bt = b.number_string().or_else(|| b.atom_string().map(|s| s.to_string()))
        .ok_or(YacasError::InvalidArg)?;
    // Fast path: within the i64 domain (the vast majority of calls); on
    // overflow fall back to Nat big-integer Euclid (FactorizeInt's
    // Gcd(ProductPrimesTo257(),n) needs a ~128-bit product).
    if let (Ok(x), Ok(y)) = (at.trim().parse::<i64>(), bt.trim().parse::<i64>()) {
        let mut x: i64 = x.unsigned_abs() as i64;
        let mut y: i64 = y.unsigned_abs() as i64;
        while y != 0 {
            let r = x % y;
            x = y;
            y = r;
        }
        return Ok(int_number(x));
    }
    let mut x = crate::number::nat::Nat::from_decimal(at.trim_start_matches('-')).ok_or(YacasError::InvalidArg)?;
    let mut y = crate::number::nat::Nat::from_decimal(bt.trim_start_matches('-')).ok_or(YacasError::InvalidArg)?;
    while !y.is_zero() {
        let (_, r) = x.divrem(&y).ok_or(YacasError::InvalidArg)?;
        x = y;
        y = r;
    }
    Ok(num_text(x.to_decimal()))
}

/// Builtin'Precision'Get/Set (like upstream corefunctions.h: global decimal
/// precision). predicates' IsZero body `MathPower(10,-Builtin'Precision'Get())`
/// and the base.rep/math.ys + stdfuncs MathSqrtFloat chain depend on these.
pub fn cmd_precision_get(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    Ok(num_text(env.precision.to_string()))
}
pub fn cmd_precision_set(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let n = int_text_of(&eval(env, arg(inner, 0)?)?)?;
    if n < 1 {
        return Err(YacasError::InvalidArg);
    }
    env.precision = n as u32;
    Ok(env.true_atom())
}

/// MathBitCount (like upstream LispBitCount): binary bit length of a positive integer.
fn cmd_math_bit_count(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let t = match v.number_string() {
        Some(n) => n,
        None => v.atom_string().ok_or(YacasError::InvalidArg)?.to_string(),
    };
    // Like upstream LispBitCount(x->BitCount()): bit length of the *integer
    // part* (2.5 -> 2, 0.5 -> 0, 2^100 -> 101, 0 -> 0, -5 -> 3; the sign is
    // not counted).
    let t = t.trim();
    let neg = t.starts_with('-');
    let body = if neg { &t[1..] } else { t };
    // Truncate the fractional part (float text: before '.'/exponent)
    let int_part = match body.find(['.', 'e', 'E']) {
        Some(i) => &body[..i],
        None => body,
    };
    let int_part = if int_part.is_empty() { "0" } else { int_part };
    let big = crate::number::nat::Nat::from_decimal(int_part)
        .ok_or(YacasError::InvalidArg)?;
    if big.is_zero() {
        return Ok(int_number(0));
    }
    let bits = nat_bit_len(&big);
    Ok(int_number(bits as i64))
}

/// MathGetExactBits: integer -> BitCount (binary bit length); float ->
/// ceil(stored mantissa digits * log2(10)). The stored form is the literal
/// padded at *creation-time* precision (float_at pad), independent of the
/// current precision — the MathSqrtFloat chain computes targetbits after
/// Builtin'Precision'Set(10), so using stored digits (not the current
/// precision) keeps targetbits stable (e.g. GetExactBits(2.) is 34 at
/// precision 10 and 84 at precision 25).
/// See upstream: cyacas/libyacas/src/mathcommands3.cpp LispGetExactBits.
fn cmd_math_get_exact_bits(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let n = match &v.kind {
        ObjectKind::Number(n) => n,
        _ => return Err(YacasError::InvalidArg),
    };
    let t = n.string();
    if n.is_float() || t.contains('.') || t.contains('e') || t.contains('E') {
        // Stored digits = max(session precision, all literal digits including
        // trailing zeros after the point; ".0"'s 0 counts); bits =
        // ceil(digits * log2(10)).
        let digits = n.digit_count_storage(env.precision());
        let bits = (digits as f64 * (10f64).log2()).ceil() as i64;
        Ok(int_number(bits))
    } else {
        let neg = t.starts_with('-');
        let digits = if neg { &t[1..] } else { t.as_str() };
        let big = crate::number::nat::Nat::from_decimal(digits)
            .ok_or(YacasError::InvalidArg)?;
        let bits = nat_bit_len(&big);
        Ok(int_number(bits as i64))
    }
}

/// MathSetExactBits: float -> cap the decimal fraction digits at
/// floor(bits * log10(2)), rounding half-carry to cap+1 digits then dropping
/// the last (the carry can chain into the integer part, 9.99 -> 10.), trailing
/// zeros trimmed. Integers pass through unchanged. Inverse of GetExactBits'
/// ceil(digits * log2(10)).
/// See upstream: cyacas/libyacas/src/mathcommands3.cpp LispSetExactBits.
fn cmd_math_set_exact_bits(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let bits = int_text_of(&eval(env, arg(inner, 1)?)?)?;
    let v = eval(env, arg(inner, 0)?)?;
    match &v.kind {
        ObjectKind::Number(n) if n.is_float() => {
            let frac = ((bits.max(0) as f64) * std::f64::consts::LOG10_2).floor();
            let frac = if frac < 0.0 { 0u32 } else { frac as u32 };
            // float_at(0): unpadded stored form (padding would count trailing
            // zeros as significant and over-trim)
            let f = n.float_at(0).round_to_frac(frac);
            Ok(num_of_flag(f, true))
        }
        _ => Ok(v), // integer type: unchanged
    }
}

/// Binary bit length of a Nat (like upstream BitCount; via Nat::bit_len).
fn nat_bit_len(n: &crate::number::nat::Nat) -> u64 {
    n.bit_len()
}

/// MathMul2Exp: multiplies by 2^n exactly (like the upstream kernel's shift
/// semantics; the script-level division version in base.rep/math.ys would
/// truncate 123456.789/2^16 to 9 digits at the working precision, making the
/// later sqrt chain converge to the truncated a0; the kernel's *2^n is always
/// exact — see Float::mul2exp).
fn cmd_math_mul2_exp(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let (x, fl) = arg_float_flag(env, inner, 0)?;
    let n = int_text_of(&eval(env, arg(inner, 1)?)?)?;
    Ok(num_of_flag(x.mul2exp(n), fl))
}

/// DigitsToBits/BitsToDigits: decimal digit count <-> bit count conversions.
fn cmd_digits_to_bits(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let d = int_text_of(&eval(env, arg(inner, 0)?)?)?;
    let base = int_text_of(&eval(env, arg(inner, 1)?)?)?;
    if d < 0 || !(2..=32).contains(&base) {
        return Err(YacasError::InvalidArg);
    }
    // Like upstream numbers.cpp: ceil(digits * log2(base))
    let v = (d as f64 * (base as f64).log2()).ceil() as i64;
    Ok(int_number(v))
}
fn cmd_bits_to_digits(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let b = int_text_of(&eval(env, arg(inner, 0)?)?)?;
    let base = int_text_of(&eval(env, arg(inner, 1)?)?)?;
    if b < 0 || !(2..=32).contains(&base) {
        return Err(YacasError::InvalidArg);
    }
    // Like upstream numbers.cpp: floor(bits / log2(base))
    let v = (b as f64 / (base as f64).log2()).floor() as i64;
    Ok(int_number(v))
}

/// FastPower (like upstream LispFastPower): floating-point fast power.
/// Integer/negative-integer exponents use fast exponentiation (negative ->
/// reciprocal); non-integer exponents raise InvalidArg since the engine has
/// no exp/ln.
/// See upstream: cyacas/libyacas/src/corefunctions.h LispFastPower.
fn cmd_fast_power(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let (x, fl) = arg_float_flag(env, inner, 0)?;
    let n = int_text_of(&eval(env, arg(inner, 1)?)?)?;
    let one = crate::number::float::Float::from_decimal("1").expect("1");
    let zero = crate::number::float::Float::from_decimal("0").expect("0");
    // Negative exponent: like upstream MathIntPower = MathDivide(1,
    // PositiveIntPower(x,-n)) — i.e. the base becomes the reciprocal 1/x.
    // (Using x/1 instead of 1/x would not invert and would break negative
    // exponents and the downstream IsZero/GreaterThan predicates.)
    let base = if n < 0 { one.div(&x, env.precision).unwrap_or(zero.clone()) } else { x };
    let mut acc = one;
    let mut b = base;
    let mut k = n.unsigned_abs();
    while k > 0 {
        if k & 1 == 1 {
            acc = acc.mul(&b, env.precision);
        }
        b = b.mul(&b, env.precision);
        k >>= 1;
    }
    Ok(num_of_flag(acc, fl))
}

/// FastLog (like upstream LispFastLog = std::log): natural logarithm. Upstream
/// uses the platform double log; the Rust decimal Float model has no exact log
/// — approximation: text -> f64 -> ln -> back to Float (precise enough for
/// MathFloor(FastLog(x)/FastLog(10)) decisions; FloatIsInt's digit-count
/// estimation depends on this). Non-positive argument -> InvalidArg (log
/// domain).
fn cmd_fast_log(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = match &v.kind {
        ObjectKind::Number(n) => n.string(),
        _ => return Err(YacasError::InvalidArg),
    };
    let x: f64 = s.parse().map_err(|_| YacasError::InvalidArg)?;
    if x <= 0.0 {
        return Err(YacasError::InvalidArg);
    }
    let l = x.ln();
    let f = crate::number::float::Float::from_decimal(&format!("{l:.17}"))
        .ok_or(YacasError::InvalidArg)?;
    Ok(num_of(f))
}

/// Platform unary functions (like upstream's PLATFORM_UNARY macro: platform
/// double function, converted back to decimal). The Rust decimal Float has no
/// high-precision transcendentals — f64 approximation, sufficient for
/// downstream decisions.
fn platform_unary<F: Fn(f64) -> f64>(
    env: &mut Environment,
    inner: &Rc<LispObject>,
    f: F,
    domain_ok: fn(f64) -> bool,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = match &v.kind {
        ObjectKind::Number(n) => n.string(),
        _ => return Err(YacasError::InvalidArg),
    };
    let x: f64 = s.parse().map_err(|_| YacasError::InvalidArg)?;
    if !domain_ok(x) {
        return Err(YacasError::InvalidArg);
    }
    let y = f(x);
    let r = crate::number::float::Float::from_decimal(&format!("{y:.17}"))
        .ok_or(YacasError::InvalidArg)?;
    Ok(num_of(r))
}

/// FastArcSin (like upstream LispFastArcSin = std::asin; numerical.ys's
/// ArcSinNum -> MathArcSin (Taylor) uses it as an initial value).
/// See upstream: cyacas/libyacas/src/mathcommands3.cpp LispFastArcSin.
fn cmd_fast_arc_sin(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    platform_unary(env, inner, f64::asin, |x| (-1.0..=1.0).contains(&x))
}

/// Insert / DestructiveInsert (like upstream InternalInsert): 1-based; inserts
/// before the element reached at step index; index = length + 1 -> append at
/// the end; out of range -> ListNotLongEnough. Insert(aa,1,xx) -> {xx,1,2,3}.
/// Note: the only observable difference of the destructive variant is
/// in-place mutation — Rust rebuilds the list; in-place writeback is handled
/// by the call layer.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp InternalInsert.
pub fn cmd_insert(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 3 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let list_node = eval(env, arg(inner, 0)?)?;
    let idx_node = eval(env, arg(inner, 1)?)?;
    let idx = int_text_of(&idx_node)?;
    if idx < 1 {
        return Err(YacasError::InvalidArg);
    }
    let item = eval(env, arg(inner, 2)?)?;
    let sub = list_node.sublist().ok_or(YacasError::NotList)?;
    // The cursor always starts at the *first element of the content chain*
    // (like upstream InternalInsert on headless chains: the iter starts at
    // the first element; while ind>0 GoNext). Two chain forms here:
    // - head-carrying chain (literal {a,b} -> List head on the content chain,
    //   sub is the List head): start = sub.next (first element a);
    // - headless chain (FlatCopy / Insert results -> sub is the first element
    //   a): start = sub itself.
    // Starting uniformly at sub.next would skip the first element of headless
    // chains, off-by-one the count, and wrongly report ListNotLongEnough for a
    // legal append at "length + 1".
    // For an empty list with idx = 1 the cursor stops at the sub slot, same as
    // upstream (content chain has only the List head or nothing -> insert
    // directly).
    let start_elem = if sub.atom_string().map(|s| s.as_ref()) == Some("List") {
        sub.next.as_ref()
    } else {
        Some(sub)
    };
    let mut kinds: Vec<ObjectKind> = Vec::new();
    let mut cur: Option<&Rc<LispObject>> = start_elem;
    let mut i: i64 = 1;
    loop {
        if i == idx {
            kinds.push(spine_kinds(&item).next().expect("item kind"));
        }
        match cur {
            Some(e) => {
                kinds.push(spine_kinds(e).next().expect("elem kind"));
                cur = e.next.as_ref();
            }
            None => {
                if i < idx {
                    return Err(YacasError::ListNotLongEnough);
                }
                break;
            }
        }
        i += 1;
    }
    let mut all: Vec<ObjectKind> = Vec::with_capacity(kinds.len() + 1);
    all.push(ObjectKind::Atom(env.symtab.look_up("List")));
    all.extend(kinds);
    let chain = crate::value::build_list(all).ok_or(YacasError::InvalidArg)?;
    Ok(Rc::new(LispObject { next: None, kind: ObjectKind::Sublist(chain) }))
}

/// Replace/DestructiveReplace (like upstream InternalReplace; Function|Fixed,
/// 3 args): (list, index, value) -> replaces the index-th (1-based) element
/// with value. The destructive variant writes back into the first argument's
/// variable slot (like the Insert convention). constants.rep's Internal'X
/// cache rule `DestructiveReplace(cached'C,2,new'prec)` depends on this —
/// if the cached precision/value is not updated, N is recomputed on every use.
fn internal_replace(
    env: &mut Environment,
    inner: &Rc<LispObject>,
    destructive: bool,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 3 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let list_node = eval(env, arg(inner, 0)?)?;
    let idx_node = eval(env, arg(inner, 1)?)?;
    let idx = int_text_of(&idx_node)?;
    if idx < 1 {
        return Err(YacasError::InvalidArg);
    }
    let value = eval(env, arg(inner, 2)?)?;
    let sub = list_node.sublist().ok_or(YacasError::NotList)?;
    // Start = first element of the content chain (strip the head for a
    // head-carrying List; same criterion as cmd_insert)
    let start_elem = if sub.atom_string().map(|s| s.as_ref()) == Some("List") {
        sub.next.as_ref()
    } else {
        Some(sub)
    };
    let mut kinds: Vec<ObjectKind> = Vec::new();
    let mut cur: Option<&Rc<LispObject>> = start_elem;
    let mut i: i64 = 1;
    loop {
        match cur {
            Some(e) => {
                if i == idx {
                    kinds.push(spine_kinds(&value).next().expect("value kind"));
                } else {
                    kinds.push(spine_kinds(e).next().expect("elem kind"));
                }
                cur = e.next.as_ref();
            }
            None => {
                if i <= idx {
                    return Err(YacasError::ListNotLongEnough);
                }
                break;
            }
        }
        i += 1;
    }
    let mut all: Vec<ObjectKind> = Vec::with_capacity(kinds.len() + 1);
    all.push(ObjectKind::Atom(env.symtab.look_up("List")));
    all.extend(kinds);
    let chain = crate::value::build_list(all).ok_or(YacasError::InvalidArg)?;
    let result = Rc::new(LispObject { next: None, kind: ObjectKind::Sublist(chain) });
    if destructive {
        write_back_arg0(env, inner, &result)?;
    }
    Ok(result)
}
pub fn cmd_replace(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    internal_replace(env, inner, false)
}
pub fn cmd_destructive_replace(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    internal_replace(env, inner, true)
}

// (The hand-written Rust bootstrap_nth and its helpers call_of/single_kind_of
// have been retired: the Nth rule is now loaded from the standard.ys script.)

// Nth rule 10 (per standard.ys; the `<--` rule bodies in patterns.rep use
// `patternleft[1]/[2]`, i.e. Nth — this registration makes them evaluable):
//   RuleBase("Nth",{alist,aindex});
//   Rule("Nth",2,10,
//       And(Equals(IsFunction(alist),True),
//           Equals(IsInteger(aindex),True),
//           Not(Equals(Head(Listify(alist)),Nth))))
//       MathNth(alist,aindex);
// (bootstrap_nth has been retired: see the note above; the Nth rule is loaded
// by standard.ys.)

/// Command registration (the corresponding entries of MathCommands.AddCommands;
/// called from Environment::new).
pub fn register_core_commands(env: &mut Environment) {
    let add = |env: &mut Environment, name: &'static str, func: fn(&mut Environment, &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError>| {
        env.core_commands.insert(
            env.symtab.look_up(name),
            crate::evaluator::CoreCommand {
                name,
                func,
                hold_args: vec![],
                un_fenced: vec![],
            },
        );
    };
    // `:=` is not core-registered — as in cyacas it is handled by the script
    // rules of deffunc.rep/code.ys (RuleBase(":=",…) plus atom/list assignment
    // rules and Function/Macro definition rules).
    // The core table keeps only Set/MacroSet (used inside script rule bodies).
    add(env, "Set", cmd_set);
    add(env, "Local", cmd_local);

    // `+ - * /` are NOT core-registered — in cyacas they are stdarith.ys
    // script rules (fraction arithmetic / symbolic collection). Core
    // registration would shadow the scripts: pure core float arithmetic would
    // yield 0.5 for `1/4+1/4` and 3.5 for `7/2` instead of `1/2` and `7/2`.
    // MathAdd/MathSubtract/MathMultiply/MathDivide stay core (called from the
    // stdarith rule bodies); `^` was never core (stdarith ^ rule ->
    // MathPower/MathIntPower).
    // add(env, "+", cmd_add);
    // add(env, "-", cmd_sub);
    // add(env, "*", cmd_mul);
    // add(env, "/", cmd_div);
    add(env, "If", cmd_if);
    add(env, "Not", cmd_not);
    // The relational operators `<`/`>`/`<=`/`>=` are NOT core commands
    // (absent from cyacas corefunctions.h); stubs.rep script rules such as
    // `(n_IsNumber < m_IsNumber) <-- LessThan(n-m,0)` handle them, matching
    // numbers only and leaving non-numbers unevaluated.
    add(env, "Equals", cmd_equals);
    // Single "=" (as in cyacas/Java CoreCommands: "=" → LispEquals): equality
    // on numbers, strings, and atoms. The if/else else-branch rule predicate
    // (Eval(pred) = True) relies on this channel.
    add(env, "=", cmd_equals);
    add(env, "Head", cmd_head);
    add(env, "Tail", cmd_tail);
    add(env, "Length", cmd_length);
    add(env, "Listify", cmd_listify);
    // UnList (see upstream: cyacas/libyacas/src/mathcommands.cpp LispUnList;
    // the if/else 1# fallback rule body depends on it — strips the List head
    // and returns the element chain).
    add(env, "UnList", cmd_unlist);
    add(env, "String", cmd_string);
    add(env, "Type", cmd_type);
    add(env, "RuleBaseDefined", cmd_rule_base_defined);
    add(env, "DefLoadFunction", cmd_def_load_function);
    // Use/Load/Hold/DefaultDirectory — required by the yacasinit.ys boot chain
    // (the Use calls, Defun body Hold, DefaultDirectory directory injection);
    // see upstream: cyacas/libyacas/src/mathcommands.cpp
    // LispUse/LispLoad/LispHold/LispDefaultDirectory.
    add(env, "Use", cmd_use);
    add(env, "Load", cmd_load);
    add(env, "Hold", cmd_hold);
    add(env, "Subst", cmd_subst);
    add(env, "DefaultDirectory", cmd_default_directory);
    add(env, "RuleBase", cmd_rule_base);
    add(env, "RuleBaseListed", cmd_rule_base_listed);
    add(env, "MacroRuleBase", cmd_macro_rule_base);
    add(env, "MacroRuleBaseListed", cmd_macro_rule_base_listed);
    add(env, "DefMacroRuleBase", cmd_def_macro_rule_base);
    add(env, "DefMacroRuleBaseListed", cmd_def_macro_rule_base_listed);
    add(env, "HoldArg", cmd_hold_arg);
    add(env, "Rule", cmd_rule);
    add(env, "RulePattern", cmd_rule_pattern);
    add(env, "MacroRule", cmd_macro_rule);
    add(env, "UnFence", cmd_un_fence);
    add(env, "Retract", cmd_retract);
    add(env, "RuleBaseArgList", cmd_rule_base_arg_list);
    add(env, "DefLoad", cmd_def_load);
    add(env, "List", cmd_list);
    add(env, "MathNth", cmd_math_nth);
    add(env, "And", cmd_and);
    add(env, "Or", cmd_or);
    add(env, "LessThan", cmd_less);
    add(env, "GreaterThan", cmd_greater);
    add(env, "MathAdd", cmd_math_add);
    // MathMultiply/MathDivide — called from the stdarith `*`/`/` rule bodies
    // (both present in the cyacas core table).
    add(env, "MathMultiply", cmd_mul);
    add(env, "MathDivide", cmd_div);
    add(env, "IsFunction", cmd_is_function);
    add(env, "IsAtom", cmd_is_atom);
    add(env, "IsNumber", cmd_is_number);
    add(env, "IsInteger", cmd_is_integer);
    add(env, "IsNonNegativeInteger", cmd_is_non_negative_integer);
    add(env, "IsPositiveInteger", cmd_is_positive_integer);
    add(env, "IsNonPositiveInteger", cmd_is_non_positive_integer);
    add(env, "IsNegativeInteger", cmd_is_negative_integer);
    add(env, "Assume", cmd_assume);
    add(env, "IsAssumed", cmd_is_assumed);
    add(env, "IsAssumedValue", cmd_is_assumed_value);
    add(env, "ClearAssumptions", cmd_clear_assumptions);
    add(env, "PushAssumptions", cmd_push_assumptions);
    add(env, "PopAssumptions", cmd_pop_assumptions);
    add(env, "IsList", cmd_is_list);
    add(env, "IsString", cmd_is_string);
    add(env, "Insert", cmd_insert);
    add(env, "DestructiveInsert", cmd_destructive_insert);
    add(env, "Replace", cmd_replace);
    add(env, "DestructiveReplace", cmd_destructive_replace);
    add(env, "DestructiveReverse", cmd_reverse);
    add(env, "MacroRulePattern", cmd_macro_rule_pattern);
    add(env, "Pattern'Create", cmd_pattern_create);
    add(env, "arg", cmd_arg);
    // Operator commands (registered as in C++ mathcommands.cpp and the Java
    // MathCommands table; the stdopers.ys script invokes these commands to
    // modify the four operator tables)
    add(env, "Infix", cmd_infix);
    add(env, "Prefix", cmd_prefix);
    add(env, "Postfix", cmd_postfix);
    add(env, "Bodied", cmd_bodied);
    add(env, "RightAssociative", cmd_right_associative);
    add(env, "LeftPrecedence", cmd_left_precedence);
    add(env, "RightPrecedence", cmd_right_precedence);
    add(env, "OpPrecedence", cmd_op_precedence);
    add(env, "OpLeftPrecedence", cmd_op_left_precedence);
    add(env, "OpRightPrecedence", cmd_op_right_precedence);
    // Commands the scripted `:=` depends on (BackQuote/Eval/MacroSet/Delete/
    // DestructiveDelete, as in C++/Java).
    add(env, "`", cmd_back_quote);
    add(env, "Eval", cmd_eval);
    add(env, "MacroSet", cmd_macro_set);
    add(env, "SetGlobalLazyVariable", cmd_set_global_lazy_variable);
    add(env, "Clear", cmd_clear);
    add(env, "MacroClear", cmd_macro_clear);
    add(env, "Delete", cmd_delete);
    add(env, "DestructiveDelete", cmd_destructive_delete);
    // Commands needed to load standard.ys (Protect/UnProtect/LocalSymbols/
    // MathSubtract/BitAnd/BitOr/Mod, as in C++).
    add(env, "Protect", cmd_protect);
    add(env, "UnProtect", cmd_unprotect);
    add(env, "IsProtected", cmd_is_protected);
    add(env, "IsGeneric", cmd_is_generic);
    add(env, "GenericTypeName", cmd_generic_type_name);
    add(env, "IsInfix", cmd_is_infix);
    add(env, "IsPrefix", cmd_is_prefix);
    add(env, "IsPostfix", cmd_is_postfix);
    add(env, "IsBodied", cmd_is_bodied);
    add(env, "Pattern'Matches", cmd_pattern_matches);
    add(env, "Association'Create", cmd_assoc_create);
    add(env, "Association'Size", cmd_assoc_size);
    add(env, "Association'Contains", cmd_assoc_contains);
    add(env, "Association'Get", cmd_assoc_get);
    add(env, "Association'Set", cmd_assoc_set);
    add(env, "Association'Drop", cmd_assoc_drop);
    add(env, "Association'Keys", cmd_assoc_keys);
    add(env, "Association'ToList", cmd_assoc_to_list);
    add(env, "Association'Head", cmd_assoc_head);
    add(env, "Array'Create", cmd_array_create);
    add(env, "Array'Get", cmd_array_get);
    add(env, "Array'Set", cmd_array_set);
    add(env, "Array'Size", cmd_array_size);
    add(env, "IsBound", cmd_is_bound);
    add(env, "MacroLocal", cmd_macro_local);
    add(env, "Check", cmd_check);
    add(env, "TrapError", cmd_trap_error);
    add(env, "GetCoreError", cmd_get_core_error);

    // Output stream family (as in cyacas corefunctions.h): Write/WriteString/ToString/FullForm
    add(env, "Write", cmd_write);
    add(env, "WriteString", cmd_write_string);
    add(env, "ToString", cmd_to_string);
    add(env, "FullForm", cmd_full_form);
    add(env, "ToFile", cmd_to_file);
    add(env, "ToStdout", cmd_to_stdout);

    // CustomEval family (as in cyacas corefunctions.h; the core of the debug.rep debugger)
    add(env, "CustomEval", cmd_custom_eval);
    add(env, "CustomEval'Expression", cmd_custom_eval_expression);
    add(env, "CustomEval'Result", cmd_custom_eval_result);
    add(env, "CustomEval'Locals", cmd_custom_eval_locals);
    add(env, "CustomEval'Stop", cmd_custom_eval_stop);

    // FastIsPrime/MathFac (as in cyacas corefunctions.h; used by the IsPrime and factorial scripts)
    add(env, "FastIsPrime", cmd_fast_is_prime);
    add(env, "MathFac", cmd_math_fac);

    // SystemCall/SystemName (as in cyacas corefunctions.h; shell utility chain)
    add(env, "SystemCall", cmd_system_call);
    add(env, "SystemName", cmd_system_name);
    add(env, "TmpFile", cmd_tmp_file);
    add(env, "Secure", cmd_secure);

    // Input stream family (as in cyacas corefunctions.h; used by REPL/io)
    add(env, "FromString", cmd_from_string);
    add(env, "FromFile", cmd_from_file);
    add(env, "Read", cmd_read);
    add(env, "ReadToken", cmd_read_token);

    add(env, "Builtin'Assoc", cmd_builtin_assoc);
    add(env, "LocalSymbols", cmd_local_symbols);
    // ApplyPure is a core command in cyacas
    // (see upstream: cyacas/libyacas/src/mathcommands3.cpp LispApplyPure);
    // without it the script statement `arglist:=ApplyPure("LocalSymbols",arglist)`
    // returns unevaluated, so arglist[1] -> MathNth -> NotList.
    add(env, "ApplyPure", cmd_apply_pure);
    // FlatCopy is a core command in cyacas
    // (see upstream: cyacas/libyacas/src/mathcommands.cpp LispFlatCopy); the
    // TemplateFunction macro body `arglist:=FlatCopy(args)` depends on it —
    // without it arglist stays an unevaluated call node and the subsequent
    // DestructiveAppend/LocalSymbols chain breaks with WrongNumberOfArgs.
    add(env, "FlatCopy", cmd_flat_copy);
    add(env, "MathSubtract", cmd_math_subtract);
    add(env, "BitAnd", cmd_bit_and);
    add(env, "BitOr", cmd_bit_or);
    // Mod is not core-registered (as in cyacas: corefunctions.h registers only
    // MathMod; "Mod" is fully handled by script rules — the stubs.rep
    // number/floor semantics and the univar.rep polynomial domains).
    // The core keeps MathMod (same name in cyacas, mathematical remainder)
    // for direct calls from script rule bodies.
    // ShiftLeft/ShiftRight (cyacas core commands; the PositiveIntPower body in
    // base.rep/math.ys uses `n <-- ShiftRight(n,1)`, so the While predicate
    // GreaterThan(thunk,0) must evaluate).
    add(env, "ShiftLeft", cmd_shift_left);
    add(env, "ShiftRight", cmd_shift_right);
    // Math core family (as in cyacas corefunctions.h; called from the
    // stubs/base.rep rule bodies: Abs->MathAbs, Div/fraction simplification->
    // MathDiv, Floor/Ceil/Round->MathFloor/MathCeil, Gcd->MathGcd,
    // PositiveIntPower/IsZero chain->MathNegate, IsZero->MathSign,
    // MathMod=Mod alias).
    add(env, "MathAbs", cmd_math_abs);
    add(env, "MathDiv", cmd_math_div);
    add(env, "MathFloor", cmd_math_floor);
    add(env, "MathCeil", cmd_math_ceil);
    add(env, "MathGcd", cmd_math_gcd);
    add(env, "MathNegate", cmd_math_negate);
    add(env, "MathSign", cmd_math_sign);
    add(env, "MathMod", cmd_mod);
    // Precision/bit-length/fast-power family (as in the cyacas core table).
    // The rule-59 predicate chain of 2^10 (IsZero -> 10^Builtin'Precision'Get)
    // and the stdfuncs MathSqrtFloat chain (MathBitCount/DigitsToBits/
    // MathGetExactBits/FastPower) depend on these.
    add(env, "Builtin'Precision'Get", cmd_precision_get);
    add(env, "Builtin'Precision'Set", cmd_precision_set);
    add(env, "MathBitCount", cmd_math_bit_count);
    add(env, "MathGetExactBits", cmd_math_get_exact_bits);
    add(env, "MathSetExactBits", cmd_math_set_exact_bits);
    add(env, "MathMul2Exp", cmd_math_mul2_exp);
    add(env, "DigitsToBits", cmd_digits_to_bits);
    add(env, "BitsToDigits", cmd_bits_to_digits);
    add(env, "FastPower", cmd_fast_power);
    add(env, "MathIntPower", cmd_fast_power);
    add(env, "FastLog", cmd_fast_log);
    add(env, "FastArcSin", cmd_fast_arc_sin);
    add(env, "Atom", cmd_atom);
    add(env, "ConcatStrings", cmd_concat_strings);
    add(env, "Concat", cmd_concat);
    add(env, "StringMid'Get", cmd_string_mid_get);
    add(env, "StringMid'Set", cmd_string_mid_set);
    add(env, "While", cmd_while);
    add(env, "Prog", cmd_prog);
    // Assorted core commands (as in the cyacas corefunctions.h table).
    // MathAnd/MathOr/MathNot are aliases of Not/And/Or sharing the same C
    // functions, like CORE_KERNEL_FUNCTION_ALIAS.
    add(env, "MathAnd", cmd_and);
    add(env, "MathOr", cmd_or);
    add(env, "MathNot", cmd_not);
    add(env, "BitXor", cmd_bit_xor);
    add(env, "StrictTotalOrder", cmd_strict_total_order);
    add(env, "MathIsSmall", cmd_math_is_small);
    add(env, "FromBase", cmd_from_base);
    add(env, "ToBase", cmd_to_base);
    add(env, "CharString", cmd_char_string);
    add(env, "Version", cmd_version);
    add(env, "Interpreter", cmd_interpreter);
    add(env, "Variables", cmd_variables);
    add(env, "FindFile", cmd_find_file);
    add(env, "FindFunction", cmd_find_function);
    add(env, "MaxEvalDepth", cmd_max_eval_depth);
    add(env, "GarbageCollect", cmd_garbage_collect);
    add(env, "InDebugMode", cmd_in_debug_mode);
    add(env, "DebugFile", cmd_debug_file);
    add(env, "DebugLine", cmd_debug_line);
    add(env, "MathDebugInfo", cmd_math_debug_info);
    add(env, "PrettyReader'Set", cmd_pretty_reader_set);
    add(env, "PrettyReader'Get", cmd_pretty_reader_get);
    add(env, "PrettyPrinter'Set", cmd_pretty_printer_set);
    add(env, "PrettyPrinter'Get", cmd_pretty_printer_get);
    add(env, "CurrentFile", cmd_current_file);
    add(env, "CurrentLine", cmd_current_line);
    add(env, "TraceRule", cmd_trace_rule);
    add(env, "TraceStack", cmd_trace_stack);
    add(env, "XmlTokenizer", cmd_xml_tokenizer);
    add(env, "DefaultTokenizer", cmd_default_tokenizer);
    add(env, "XmlExplodeTag", cmd_xml_explode_tag);
    add(env, "PatchLoad", cmd_patch_load);
    add(env, "PatchString", cmd_patch_string);
    add(env, "LispRead", cmd_lisp_read);
    add(env, "LispReadListed", cmd_lisp_read_listed);
}

// ============ Assorted core commands (per the cyacas corefunctions.h table) ============

/// BitXor (see upstream: cyacas/libyacas/src/mathcommands3.cpp LispBitXor;
/// same family as BitAnd/BitOr): bitwise XOR of integers.
pub fn cmd_bit_xor(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let (a, b) = two_ints(env, inner)?;
    Ok(int_number(a ^ b))
}

/// StrictTotalOrder (see upstream: cyacas/libyacas/src/standard.cpp
/// LispStrictTotalOrder / InternalStrictTotalOrder): strict total order
/// (used for sort key ordering; same ordering as std::map).
/// Identical pointers -> False; numbers < non-numbers; two numbers compared by
/// value (equal -> compare the chain tail next); strings by strcmp (equal ->
/// compare the tail); sublists compared element by element with InternalEquals,
/// recursing on the first unequal pair; shorter first, equal length -> False.
fn strict_less(env: &mut Environment, e1: &Rc<LispObject>, e2: &Rc<LispObject>) -> Result<bool, YacasError> {
    if Rc::ptr_eq(e1, e2) {
        return Ok(false);
    }
    match (&e1.kind, &e2.kind) {
        (ObjectKind::Number(a), ObjectKind::Number(b)) => {
            let fa = a.float();
            let fb = b.float();
            if fa.less_than(&fb) {
                return Ok(true);
            }
            if !fa.equals(&fb) {
                return Ok(false);
            }
            strict_tail(env, e1, e2)
        }
        (ObjectKind::Number(_), _) => Ok(true),
        (_, ObjectKind::Number(_)) => Ok(false),
        (ObjectKind::Atom(s1), ObjectKind::Atom(s2)) => match s1.as_ref().cmp(s2.as_ref()) {
            std::cmp::Ordering::Less => Ok(true),
            std::cmp::Ordering::Greater => Ok(false),
            std::cmp::Ordering::Equal => strict_tail(env, e1, e2),
        },
        (ObjectKind::Atom(_), ObjectKind::Sublist(_)) => Ok(true),
        (ObjectKind::Sublist(_), ObjectKind::Atom(_)) => Ok(false),
        (ObjectKind::Sublist(_), ObjectKind::Sublist(_)) => {
            let mut i1 = Some(e1.clone());
            let mut i2 = Some(e2.clone());
            loop {
                match (&i1, &i2) {
                    (Some(a), Some(b)) => {
                        if crate::standard::internal_equals(env, a, b) {
                            i1 = a.next.clone();
                            i2 = b.next.clone();
                        } else {
                            return strict_less(env, a, b);
                        }
                    }
                    (None, None) => return Ok(false),
                    (None, Some(_)) => return Ok(true),
                    (Some(_), None) => return Ok(false),
                }
            }
        }
        // Generic and other kinds (as in C++: fall through to return false)
        _ => Ok(false),
    }
}

/// Chain-tail comparison for equal elements (like the InternalStrictTotalOrder recursion).
fn strict_tail(env: &mut Environment, e1: &Rc<LispObject>, e2: &Rc<LispObject>) -> Result<bool, YacasError> {
    match (&e1.next, &e2.next) {
        (None, None) => Ok(false),
        (None, Some(_)) => Ok(true),
        (Some(_), None) => Ok(false),
        (Some(t1), Some(t2)) => strict_less(env, t1, t2),
    }
}

pub fn cmd_strict_total_order(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let e1 = eval(env, arg(inner, 0)?)?;
    let e2 = eval(env, arg(inner, 1)?)?;
    let lt = strict_less(env, &e1, &e2)?;
    Ok(crate::standard::internal_boolean(env, lt))
}

/// MathIsSmall (see upstream: cyacas/libyacas/src/yacasnumbers.cpp
/// LispMathIsSmall / BigNumber::IsSmall): integers -> bit length <= 53
/// (2^52 True, 2^53 False); floats -> creation precision (decimal digits) <= 53
/// and |te| < 1021 (N(2.5,16) True, 1e1020 True, 1e1021 False).
pub fn cmd_math_is_small(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let small = match &v.kind {
        ObjectKind::Number(n) if n.is_float() => {
            let f = n.float_at(0);
            f.prec() <= 53 && f.tens_exp_of().abs() < 1021
        }
        ObjectKind::Number(n) => {
            let t = n.string();
            let mag = t.strip_prefix('-').unwrap_or(&t);
            match crate::number::nat::Nat::from_decimal(mag) {
                Some(x) => x.bit_len() <= 53,
                None => return Err(YacasError::InvalidArg),
            }
        }
        _ => return Err(YacasError::InvalidArg),
    };
    Ok(crate::standard::internal_boolean(env, small))
}

/// CharString (see upstream: cyacas/libyacas/src/mathcommands2.cpp
/// LispCharString): ASCII code -> single-character quoted string. The argument
/// must be numeric text; atoi semantics ("1.5" -> 1); truncated to 8 bits as
/// (char) (CharString(300) -> ",").
pub fn cmd_char_string(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let text = match &v.kind {
        ObjectKind::Atom(s) => s.to_string(),
        ObjectKind::Number(n) => n.string(),
        _ => return Err(YacasError::InvalidArg),
    };
    if crate::number::float::Float::from_decimal_with_prec(&text, 0).is_none() {
        return Err(YacasError::InvalidArg);
    }
    let t = text.trim_start();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    let mut code: i64 = digits.parse().unwrap_or(0);
    if neg {
        code = -code;
    }
    let byte = (code & 0xFF) as u8;
    Ok(crate::value::make_atom(&mut env.symtab, &format!("\"{}\"", byte as char)))
}

/// Base-b digit character -> value (like DigitIndex; '0'-'9', 'a'-'z', 'A'-'Z';
/// out of base -> None).
fn base_digit(c: char, base: i64) -> Option<i64> {
    let v = match c {
        '0'..='9' => c as i64 - '0' as i64,
        'a'..='z' => c as i64 - 'a' as i64 + 10,
        'A'..='Z' => c as i64 - 'A' as i64 + 10,
        _ => return None,
    };
    if v < base {
        Some(v)
    } else {
        None
    }
}

/// Nat -> base-b digit string (lowercase 0-9a-z; like ZZ::to_string(base); zero -> "0").
fn nat_to_base(n: &crate::number::nat::Nat, base: u32) -> String {
    if n.is_zero() {
        return "0".into();
    }
    let b = crate::number::nat::Nat::from_decimal(&base.to_string()).expect("base");
    let mut ds: Vec<u8> = Vec::new();
    let mut cur = n.clone();
    while !cur.is_zero() {
        let (q, r) = cur.divrem(&b).expect("div by nonzero");
        let d: u32 = r.to_decimal().parse().expect("r < base");
        ds.push(if d < 10 { b'0' + d as u8 } else { b'a' + (d - 10) as u8 });
        cur = q;
    }
    ds.reverse();
    String::from_utf8(ds).expect("base digits")
}

/// Base-b digit string -> Nat (FromBase mantissa parsing; out-of-base digit -> None).
fn nat_from_base(s: &str, base: i64) -> Option<crate::number::nat::Nat> {
    let b = crate::number::nat::Nat::from_decimal(&base.to_string())?;
    let mut acc = crate::number::nat::Nat::zero();
    for c in s.chars() {
        let d = base_digit(c, base)?;
        acc = acc.mul(&b).add(&crate::number::nat::Nat::from_decimal(&d.to_string())?);
    }
    Some(acc)
}

/// FromBase (see upstream: cyacas/libyacas/src/mathcommands3.cpp LispFromBase
/// and cyacas/libyacas/src/anumber.cpp ANumber::SetTo): base integer 2..=32
/// (log2_table_range); the second argument must be a string.
/// The digit string is parsed in the given base: integers -> exact integer;
/// with '.' or (when base < 14) 'e'/'E' -> float:
/// value = mantissa × b^(-number of fraction digits), with the e exponent
/// applied via decimal atoi (same as SetTo; FromBase(2,"1.1e2") = 150 = 0.15e3).
/// The 'E' quirk of cyacas at base >= 11 (1E5 -> 0) is not replicated; it is
/// treated as a mantissa digit (consistent with FromBase(16,"1e5") = 485).
/// Precision conversion (like CalculatePrecision): bits = ceil(max(BinaryPrecision,
/// significant digits) × log2(base)), decimal digits = floor(bits × log10 2)
/// (FromBase(3,"0.1") -> 16 digits).
pub fn cmd_from_base(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let bv = eval(env, arg(inner, 0)?)?;
    let base = match &bv.kind {
        ObjectKind::Number(n) if !n.is_float() => n
            .string()
            .parse::<i64>()
            .map_err(|_| YacasError::InvalidArg)?,
        _ => return Err(YacasError::InvalidArg),
    };
    if !(2..=32).contains(&base) {
        return Err(YacasError::InvalidArg);
    }
    let sv = eval(env, arg(inner, 1)?)?;
    let st = sv.atom_string().ok_or(YacasError::InvalidArg)?;
    let s = crate::standard::internal_unstringify(st).unwrap_or(st);

    let (neg, rest) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s),
    };
    let chars: Vec<char> = rest.chars().collect();
    // Locate the '.' and e markers (like SetTo: lowercase 'e' only when
    // base < 14; for base >= 14, e/E are digits)
    let mut dot: Option<usize> = None;
    let mut emark: Option<usize> = None;
    for (i, &c) in chars.iter().enumerate() {
        if c == '.' {
            dot = Some(i);
        }
        if base < 14 && (c == 'e' || c == 'E') {
            emark = Some(i);
        }
    }
    let mant_end = emark.unwrap_or(chars.len());
    // Significant digits (like CalculatePrecision: skip leading '.', '-', '0'
    // -> digit1; sig = mant_end − digit1; -1 if a '.' appears after digit1.
    // Trailing zeros / the leading segment all count, per the C++ comment)
    let digit1 = chars
        .iter()
        .take(mant_end)
        .position(|&c| c != '.' && c != '0')
        .unwrap_or(mant_end);
    let mut sig = (mant_end - digit1) as i64;
    if chars[digit1..mant_end].contains(&'.') {
        sig -= 1;
    }
    let (int_part, frac_part) = match dot {
        Some(d) if d < mant_end => {
            let ip: String = chars[..d].iter().collect();
            let fp: String = chars[d + 1..mant_end].iter().collect();
            (ip, fp)
        }
        _ => (chars[..mant_end].iter().collect::<String>(), String::new()),
    };
    // e exponent: decimal atoi (like SetTo; optional '+')
    let te: i64 = match emark {
        Some(e) => {
            let mut t: String = chars[e + 1..].iter().collect();
            if let Some(r) = t.strip_prefix('+') {
                t = r.to_string();
            }
            let dg: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
            dg.parse().unwrap_or(0)
        }
        None => 0,
    };
    let is_float = dot.is_some() || emark.is_some();
    let mant_str = format!("{int_part}{frac_part}");
    let m = nat_from_base(&mant_str, base).ok_or(YacasError::InvalidArg)?;
    if !is_float {
        let mut txt = m.to_decimal();
        if txt == "0" {
            txt = "0".into();
        }
        if neg && txt != "0" {
            txt = format!("-{txt}");
        }
        return Ok(Rc::new(LispObject {
            next: None,
            kind: ObjectKind::Number(crate::value::LispNumber::from_text(txt)),
        }));
    }
    // Float: value = M × 10^te / b^k; precision bits = ceil(max(BinaryPrec, sig)×log2 b),
    // decimal digits = floor(bits×log10 2); t = floor(M×10^dec/b^k) (round-half-up).
    let bits_env = (env.precision() as f64 * 10f64.log2()).ceil() as i64;
    let bits = ((bits_env.max(sig)) as f64 * (base as f64).log2()).ceil() as i64;
    let dec = (((bits as f64) * 2f64.log10()).floor().max(1.0)) as u32;
    let bnat = crate::number::nat::Nat::from_decimal(&base.to_string()).expect("base");
    let denom = bnat.pow(frac_part.len() as u32);
    let scaled = m.mul_pow10(dec);
    let (q, r) = scaled.divrem(&denom).ok_or(YacasError::InvalidArg)?;
    let two = crate::number::nat::Nat::from_decimal("2").expect("two");
    let q = if r.mul(&two).cmp(&denom) != std::cmp::Ordering::Less {
        q.add(&crate::number::nat::Nat::from_decimal("1").expect("one"))
    } else {
        q
    };
    let f = crate::number::float::Float::from_parts_trimmed(q, dec, te as i32, dec, neg);
    Ok(num_of_flag(f, true))
}

/// ToBase (see upstream: cyacas/libyacas/src/mathcommands3.cpp LispToBase and
/// cyacas/libyacas/src/anumber.cpp ANumberToString): base 2..=32. Integers ->
/// plain base-b string (lowercase); floats -> only the **mantissa** is
/// converted (digits×10^-scale, te is not folded in): integer part as a base-b
/// string + fraction digits via repeated multiplication by b for
/// BinaryPrecision+1 digits (last digit >= base/2 rounds up, possibly
/// overflowing into the integer part), trailing fraction zeros trimmed;
/// floats always carry '.' (integral values "X.", zero special case "0");
/// te != 0 appends "e<te>" (e.g. ToBase(2, 0.1e21-like) -> "0.0001100…e21").
pub fn cmd_to_base(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let bv = eval(env, arg(inner, 0)?)?;
    let base = match &bv.kind {
        ObjectKind::Number(n) if !n.is_float() => n
            .string()
            .parse::<i64>()
            .map_err(|_| YacasError::InvalidArg)?,
        _ => return Err(YacasError::InvalidArg),
    };
    if !(2..=32).contains(&base) {
        return Err(YacasError::InvalidArg);
    }
    let nv = eval(env, arg(inner, 1)?)?;
    let n = match &nv.kind {
        ObjectKind::Number(n) => n,
        _ => return Err(YacasError::InvalidArg),
    };
    let b32 = base as u32;
    if !n.is_float() {
        let t = n.string();
        let (neg, mag) = match t.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, t.as_str()),
        };
        let m = crate::number::nat::Nat::from_decimal(mag).ok_or(YacasError::InvalidArg)?;
        let mut s = nat_to_base(&m, b32);
        if neg && s != "0" {
            s = format!("-{s}");
        }
        return Ok(crate::value::make_atom(&mut env.symtab, &format!("\"{s}\"")));
    }
    // Float: mantissa digits×10^-scale conversion (te is attached separately
    // as the e suffix)
    let f = n.float();
    let scale = f.frac_scale();
    let den = crate::number::nat::Nat::from_decimal("1")
        .expect("one")
        .mul_pow10(scale);
    let (int_part, mut rem) = f
        .digits_nat()
        .divrem(&den)
        .ok_or(YacasError::InvalidArg)?;
    let mut int_s = nat_to_base(&int_part, b32);
    // Fraction: repeated multiplication by b, BinaryPrecision+1 digits (guard digit)
    let bits = ((env.precision() as f64) * 10f64.log2()).ceil() as u32;
    let bb = crate::number::nat::Nat::from_decimal(&base.to_string()).expect("base");
    let mut digs: Vec<u32> = Vec::new();
    for _ in 0..(bits + 1) {
        rem = rem.mul(&bb);
        let (d, r2) = rem.divrem(&den).ok_or(YacasError::InvalidArg)?;
        digs.push(if d.is_zero() {
            0
        } else {
            d.to_decimal().parse().unwrap_or(0)
        });
        rem = r2;
    }
    // Guard-digit carry (as in C++ anumber.cpp ANumberToString: the chain
    // starts by incrementing the guard digit itself, and the guard digit is
    // then dropped — while the guard < base-1, the change disappears with the
    // dropped guard digit, a quirk of the vendored code: ToBase(3,0.5) has
    // guard 1 -> no carry -> 34 ones; ToBase(3,1/3) has guard 2 = base-1 ->
    // the chain of zeros overflows into the integer part -> "0.1")
    if *digs.last().expect("digs") as i64 >= (base >> 1) {
        let mut carry = 1i64;
        for d in digs.iter_mut().rev() {
            let w = *d as i64 + carry;
            *d = (w % base) as u32;
            carry = w / base;
            if carry == 0 {
                break;
            }
        }
        if carry > 0 {
            int_part_nat_inc(&mut int_s, b32);
        }
    }
    digs.pop();
    // Trim trailing fraction zeros (like ANumberToString; "0." special case -> "0")
    while digs.last() == Some(&0) {
        digs.pop();
    }
    let frac_s: String = digs
        .iter()
        .map(|&d| base_char(d))
        .collect();
    let mut out = if frac_s.is_empty() {
        if int_s == "0" {
            "0".to_string()
        } else {
            format!("{int_s}.")
        }
    } else {
        format!("{int_s}.{frac_s}")
    };
    let te = f.tens_exp_of();
    if te != 0 && out != "0" {
        out = format!("{out}e{te}");
    }
    if f.is_neg() && out != "0" {
        out = format!("-{out}");
    }
    Ok(crate::value::make_atom(&mut env.symtab, &format!("\"{out}\"")))
}

fn base_char(d: u32) -> char {
    if d < 10 {
        (b'0' + d as u8) as char
    } else {
        (b'a' + (d - 10) as u8) as char
    }
}

/// Add 1 with carry to a base-b integer string (rounding overflow into the integer part).
fn int_part_nat_inc(s: &mut String, base: u32) {
    let mut ds: Vec<u32> = s.chars().map(|c| base_digit(c, base as i64).unwrap_or(0) as u32).collect();
    let mut carry = 1i64;
    for d in ds.iter_mut().rev() {
        let w = *d as i64 + carry;
        *d = (w % base as i64) as u32;
        carry = w / base as i64;
        if carry == 0 {
            break;
        }
    }
    if carry > 0 {
        ds.insert(0, carry as u32);
    }
    *s = ds.iter().map(|&d| base_char(d)).collect();
}

/// Version (see upstream: cyacas/libyacas/src/mathcommands3.cpp LispVersion;
/// the vendored build leaves YACAS_VERSION empty -> ""). Interpreter
/// (see upstream: cyacas/libyacas/include/yacas/corefunctions.h "Interpreter")
/// -> "yacas".
pub fn cmd_version(env: &mut Environment, _inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(_inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    Ok(crate::value::make_atom(&mut env.symtab, "\"\""))
}
pub fn cmd_interpreter(env: &mut Environment, _inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(_inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    Ok(crate::value::make_atom(&mut env.symtab, "\"yacas\""))
}

/// Variables (see upstream: cyacas/libyacas/src/mathcommands.cpp LispVars and
/// lispenvironment.cpp GlobalVariables): a List of global variable names,
/// skipping names with a '$'/'%' prefix. The upstream unordered table has a
/// non-reproducible order; sorting keeps the output deterministic.
pub fn cmd_variables(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
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

/// FindFile (see upstream: cyacas/libyacas/src/mathcommands.cpp LispFindFile):
/// searches input_directories; on a hit -> quoted path, otherwise "".
/// Raises an error in Secure mode.
pub fn cmd_find_file(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if env.secure {
        return Err(YacasError::SecurityBreach);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s).unwrap_or(s).to_string();
    let path = crate::standard::internal_find_file(env, &name).unwrap_or_default();
    Ok(crate::value::make_atom(&mut env.symtab, &format!("\"{path}\"")))
}

/// FindFunction (see upstream: cyacas/libyacas/src/mathcommands.cpp
/// LispFindFunction): the defining file name (quoted) of a user function;
/// none -> the bare symbol Empty (same as cyacas, unquoted).
pub fn cmd_find_function(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if env.secure {
        return Err(YacasError::SecurityBreach);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s).unwrap_or(s).to_string();
    if let Some(mf) = env.user_functions.get(name.as_str()) {
        if let Some(def) = mf.inner.borrow().file_to_open.as_ref() {
            return Ok(crate::value::make_atom(
                &mut env.symtab,
                &format!("\"{}\"", def.file_name()),
            ));
        }
    }
    Ok(crate::value::make_atom(&mut env.symtab, "Empty"))
}

/// MaxEvalDepth (see upstream: cyacas/libyacas/src/mathcommands.cpp
/// LispMaxEvalDepth): sets the limit -> True.
pub fn cmd_max_eval_depth(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let n = int_text_of(&v)?;
    env.max_eval_depth = n.max(0) as u32;
    Ok(env.true_atom())
}

/// GarbageCollect (see upstream: cyacas/libyacas/src/mathcommands3.cpp
/// LispGarbageCollect): this engine has no GC -> True.
pub fn cmd_garbage_collect(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    Ok(env.true_atom())
}

/// InDebugMode (see upstream: cyacas/libyacas/src/mathcommands2.cpp
/// LispInDebugMode): non-debug -> False.
pub fn cmd_in_debug_mode(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    Ok(env.false_atom())
}

/// DebugFile/DebugLine (see upstream: cyacas/libyacas/src/mathcommands2.cpp
/// LispDebugFile / LispDebugLine): in a non-debug build they always raise
/// "Cannot call DebugFile in non-debug version of Yacas" (debug.rep guards
/// with InDebugMode, so this is unreachable in non-debug builds; matches
/// upstream behavior).
pub fn cmd_debug_file(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let _ = eval(env, arg(inner, 0)?)?;
    Err(YacasError::generic(
        "Cannot call DebugFile in non-debug version of Yacas",
    ))
}
pub fn cmd_debug_line(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let _ = eval(env, arg(inner, 0)?)?;
    Err(YacasError::generic(
        "Cannot call DebugLine in non-debug version of Yacas",
    ))
}

/// PrettyReader'/PrettyPrinter' Set/Get (see upstream:
/// cyacas/libyacas/src/mathcommands3.cpp YacasPrettyReaderSet/Get and
/// YacasPrettyPrinterSet/Get): Set with 0 args -> clears; Set with 1 string arg
/// -> stores the **quoted** string verbatim (oper->String()); Get -> the
/// stored name atom or "" (unset). Set always returns True.
fn pretty_get(env: &mut Environment, which: bool) -> Result<Rc<LispObject>, YacasError> {
    let s = if which {
        env.pretty_reader.as_ref()
    } else {
        env.pretty_printer.as_ref()
    };
    match s {
        Some(t) => Ok(crate::value::atom_or_number(&mut env.symtab, t)),
        None => Ok(crate::value::make_atom(&mut env.symtab, "\"\"")),
    }
}
fn pretty_set(env: &mut Environment, which: bool, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    match arity_of(inner) {
        0 => {
            if which {
                env.pretty_reader = None;
            } else {
                env.pretty_printer = None;
            }
            Ok(env.true_atom())
        }
        1 => {
            let v = eval(env, arg(inner, 0)?)?;
            let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
            if !crate::standard::internal_is_string(s) {
                return Err(YacasError::InvalidArg);
            }
            if which {
                env.pretty_reader = Some(s.to_string());
            } else {
                env.pretty_printer = Some(s.to_string());
            }
            Ok(env.true_atom())
        }
        _ => Err(YacasError::WrongNumberOfArgs),
    }
}
pub fn cmd_pretty_reader_set(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    pretty_set(env, true, inner)
}
pub fn cmd_pretty_reader_get(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    pretty_get(env, true)
}
pub fn cmd_pretty_printer_set(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    pretty_set(env, false, inner)
}
pub fn cmd_pretty_printer_get(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    pretty_get(env, false)
}

/// CurrentFile (see upstream: cyacas/libyacas/src/mathcommands3.cpp
/// LispCurrentFile): the input state file name (quoted); defaults to
/// "CommandLine".
pub fn cmd_current_file(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let f = env.input_file.borrow().clone();
    let name = if f.is_empty() { "CommandLine".to_string() } else { f };
    Ok(crate::value::make_atom(&mut env.symtab, &format!("\"{name}\"")))
}

/// CurrentLine (see upstream: cyacas/libyacas/src/mathcommands3.cpp
/// LispCurrentLine): the active input tokenizer's consumed-'\n' count + 1;
/// no active input -> 1.
pub fn cmd_current_line(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let line = env
        .input_stack
        .borrow()
        .last()
        .and_then(|t| t.as_ref())
        .map(|t| t.line())
        .unwrap_or(1);
    Ok(int_number(line as i64))
}

/// MathDebugInfo (see upstream: cyacas/libyacas/src/mathcommands3.cpp
/// LispDumpBigNumberDebugInfo and cyacas/libyacas/src/yacasnumbers.cpp
/// BigNumber::DumpDebugInfo): integers -> "No number representation"; floats ->
/// an ANumber::Print dump (words / after point / te / prec + 32-bit binary
/// limbs). Written to the current output; always returns True.
pub fn cmd_math_debug_info(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let line = match &v.kind {
        ObjectKind::Number(n) if n.is_float() => {
            // float_at(0): the stored form without padding; for literals the
            // precision field defaults to 10 (as in cyacas, where ANumber
            // iPrecision is the creation precision in digits, "10-prec 10")
            let mut f = n.float_at(0);
            if f.prec() == 0 {
                f = f.with_prec(env.precision().max(1));
            }
            format!("Number:\n{}", f.dump_debug())
        }
        ObjectKind::Number(_) => "No number representation\n".to_string(),
        _ => return Err(YacasError::InvalidArg),
    };
    let mut output = env.output_stack.borrow_mut();
    if output.is_empty() {
        output.push(crate::env::OutputBuffer::default());
    }
    output.last_mut().expect("output").text.push_str(&line);
    drop(output);
    Ok(env.true_atom())
}

/// TraceRule (see upstream: cyacas/libyacas/src/mathcommands.cpp LispTraceRule;
/// Macro|Fixed): the first argument is **not evaluated** and must be a sublist
/// (of the form g(_x)) -> locates the user function by (head, arity) and marks
/// it traced; evaluates the body; restores the flag (when a traced function is
/// called, userfunc prints TrEnter/TrLeave).
/// TraceStack (see upstream: cyacas/libyacas/src/mathcommands.cpp
/// LispTraceStack): cyacas installs TracedStackEvaluator (which only prints
/// the stack at MaxRecurseDepth; normal evaluation has no output); this engine
/// has no frame stack -> identity evaluation.
pub fn cmd_trace_rule(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let head = arg(inner, 0)?;
    let target: Option<(Rc<str>, usize)> = match &head.kind {
        ObjectKind::Sublist(sub) => {
            let mut it = crate::value::spine_refs(sub);
            let first = it.next();
            first.and_then(|f| f.atom_string()).map(|h| (h.clone(), crate::standard::internal_list_length(head) - 1))
        }
        _ => None,
    };
    // Clone the target function Rc first (the env borrow is released
    // immediately; &mut env is needed during evaluation)
    let mut traced_fn: Option<Rc<dyn crate::userfunc::UserFunction>> = None;
    if let Some((name, arity)) = &target {
        if let Some(mf) = env.user_functions.get(name.as_ref()) {
            let funcs = &mf.inner.borrow().functions;
            if let Some(f) = funcs.iter().find(|f| f.is_arity(*arity)) {
                traced_fn = Some(f.clone());
            }
        }
    }
    if let Some(f) = &traced_fn {
        f.set_traced(true);
    }
    let result = eval(env, arg(inner, 1)?);
    if let Some(f) = &traced_fn {
        f.set_traced(false);
    }
    result
}

pub fn cmd_trace_stack(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let body = arg(inner, 0)?.clone();
    eval(env, &body)
}

/// XmlTokenizer()/DefaultTokenizer() (see upstream:
/// cyacas/libyacas/src/mathcommands3.cpp LispXmlTokenizer / LispDefaultTokenizer):
/// switch the current tokenizer mode (environment-level flag, synchronized
/// with the active input).
pub fn cmd_xml_tokenizer(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    env.xml_tokenizer.set(true);
    if let Some(Some(t)) = env.input_stack.borrow_mut().last_mut() {
        t.xml = true;
    }
    Ok(env.true_atom())
}
pub fn cmd_default_tokenizer(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    env.xml_tokenizer.set(false);
    if let Some(Some(t)) = env.input_stack.borrow_mut().last_mut() {
        t.xml = false;
    }
    Ok(env.true_atom())
}

/// XmlExplodeTag (see upstream: cyacas/libyacas/src/mathcommands3.cpp
/// LispExplodeTag): an XML tag string -> XmlTag("TAG",{attribute pairs...},
/// "Open"/"Close"/"OpenClose"). Not starting with '<' -> returned unchanged;
/// tag names / attribute names uppercased; an attribute pair =
/// List("NAME","value") (value keeps its quotes); the attribute chain order
/// follows the C++ prepend (later attributes come first).
pub fn cmd_xml_explode_tag(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    if !crate::standard::internal_is_string(s) {
        return Err(YacasError::InvalidArg);
    }
    let body = crate::standard::internal_unstringify(s).unwrap_or(s);
    let chars: Vec<char> = body.chars().collect();
    if chars.first() != Some(&'<') {
        return Ok(v);
    }
    let mut i = 1usize;
    let mut typ = "\"Open\"";
    if chars.get(i) == Some(&'/') {
        typ = "\"Close\"";
        i += 1;
    }
    let mut tag = String::from("\"");
    while i < chars.len() && chars[i].is_alphabetic() {
        tag.push(chars[i].to_ascii_uppercase());
        i += 1;
    }
    tag.push('"');
    let mut pairs: Vec<ObjectKind> = Vec::new();
    loop {
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
        if i >= chars.len() || chars[i] == '>' || chars[i] == '/' {
            break;
        }
        let mut name = String::from("\"");
        while i < chars.len() && chars[i].is_alphabetic() {
            name.push(chars[i].to_ascii_uppercase());
            i += 1;
        }
        name.push('"');
        if chars.get(i) != Some(&'=') {
            return Err(YacasError::InvalidArg);
        }
        i += 1;
        if chars.get(i) != Some(&'"') {
            return Err(YacasError::InvalidArg);
        }
        let mut value = String::from("\"");
        i += 1;
        while i < chars.len() && chars[i] != '"' {
            value.push(chars[i]);
            i += 1;
        }
        value.push('"');
        i += 1;
        let pk = crate::value::build_list(vec![
            ObjectKind::Atom(env.symtab.look_up("List")),
            ObjectKind::Atom(env.symtab.look_up(&name)),
            ObjectKind::Atom(env.symtab.look_up(&value)),
        ])
        .ok_or(YacasError::InvalidArg)?;
        pairs.push(ObjectKind::Sublist(pk));
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
    }
    if chars.get(i) == Some(&'/') {
        typ = "\"OpenClose\"";
        i += 1;
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
    }
    pairs.reverse(); // as in C++: prepend to the chain (later attributes come first)
    let mut list_kinds: Vec<ObjectKind> = Vec::with_capacity(pairs.len() + 1);
    list_kinds.push(ObjectKind::Atom(env.symtab.look_up("List")));
    list_kinds.extend(pairs);
    let list_chain = crate::value::build_list(list_kinds).ok_or(YacasError::InvalidArg)?;
    let chain = crate::value::build_list(vec![
        ObjectKind::Atom(env.symtab.look_up("XmlTag")),
        ObjectKind::Atom(env.symtab.look_up(&tag)),
        ObjectKind::Sublist(list_chain),
        ObjectKind::Atom(env.symtab.look_up(typ)),
    ])
    .ok_or(YacasError::InvalidArg)?;
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(chain),
    }))
}

/// PatchLoad core (see upstream: cyacas/libyacas/src/patcher.cpp PatchLoad):
/// text outside '<?'/'?>' segments is written **verbatim** to the current
/// output; inside a segment, all statements of the script are parsed and
/// evaluated (like DoInternalLoad; output goes to the current output);
/// unclosed segment -> "closing tag not found when patching".
fn patch_process(env: &mut Environment, content: &str) -> Result<(), YacasError> {
    let mut i = 0usize;
    loop {
        let p = content[i..].find("<?").map(|x| x + i);
        let out_end = p.unwrap_or(content.len());
        output_append(env, &content[i..out_end]);
        let pp = match p {
            Some(x) => x,
            None => return Ok(()),
        };
        let q = content[pp + 2..]
            .find("?>")
            .map(|x| x + pp + 2)
            .ok_or_else(|| YacasError::generic("closing tag not found when patching"))?;
        let seg = &content[pp + 2..q];
        let old_file = env.input_file.borrow().clone();
        *env.input_file.borrow_mut() = "String".to_string();
        let r = crate::standard::do_internal_load(env, seg);
        *env.input_file.borrow_mut() = old_file;
        r?;
        i = q + 2;
    }
}

fn output_append(env: &mut Environment, s: &str) {
    let mut output = env.output_stack.borrow_mut();
    if output.is_empty() {
        output.push(crate::env::OutputBuffer::default());
    }
    output.last_mut().expect("output").text.push_str(s);
}

/// PatchLoad (see upstream: cyacas/libyacas/src/mathcommands3.cpp
/// LispPatchLoad): evaluates and unquotes the file name, finds it via
/// input_directories; patches the content into the current output; always
/// returns True.
pub fn cmd_patch_load(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let fname = crate::standard::internal_unstringify(s).unwrap_or(s).to_string();
    let path = crate::standard::internal_find_file(env, &fname).ok_or(YacasError::FileNotFound)?;
    let text = std::fs::read_to_string(&path).map_err(|_| YacasError::FileNotFound)?;
    patch_process(env, &text)?;
    Ok(env.true_atom())
}

/// PatchString (see upstream: cyacas/libyacas/src/mathcommands3.cpp
/// LispPatchString): patches into a fresh buffer -> quoted string (stringify
/// does not escape embedded quotes, as in C++).
pub fn cmd_patch_string(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let content = crate::standard::internal_unstringify(s).unwrap_or(s).to_string();
    let depth = env.push_output();
    let r = patch_process(env, &content);
    let captured = env.pop_output(depth);
    r?;
    Ok(crate::value::make_atom(
        &mut env.symtab,
        &format!("\"{}\"", captured.text),
    ))
}

/// LispRead/LispReadListed (see upstream:
/// cyacas/libyacas/src/mathcommands3.cpp LispReadLisp / LispReadLispListed and
/// cyacas/libyacas/src/lispparser.cpp LispParser): **purely prefix** parsing
/// (infix not recognized): one token -> atom; '(' -> collect tokens until ')'
/// into a sublist (nested '(' recurses; EOF -> InvalidToken); listed mode
/// prepends a List head before the first element; empty token -> EndOfFile
/// atom. No ';' check.
fn plain_parse(env: &mut Environment, tok: &mut crate::tokenizer::Tokenizer, listed: bool) -> Result<Rc<LispObject>, YacasError> {
    let token = tok
        .next_token()
        .map_err(|_| YacasError::generic("Invalid token"))?;
    if token.is_empty() {
        return Ok(LispObject::atom(env.symtab.look_up("EndOfFile")));
    }
    if token != "(" {
        return Ok(crate::value::atom_or_number(&mut env.symtab, &token));
    }
    let mut kinds: Vec<ObjectKind> = Vec::new();
    if listed {
        kinds.push(ObjectKind::Atom(env.symtab.look_up("List")));
    }
    loop {
        let t = tok
            .next_token()
            .map_err(|_| YacasError::generic("Invalid token"))?;
        if t.is_empty() {
            return Err(YacasError::generic("Invalid token"));
        }
        if t == ")" {
            break;
        }
        if t == "(" {
            let sub = plain_parse(env, tok, false)?;
            kinds.push(spine_kinds(&sub).next().expect("node kind"));
        } else {
            kinds.push(ObjectKind::Atom(env.symtab.look_up(&t)));
        }
    }
    let chain = crate::value::build_list(kinds).ok_or(YacasError::InvalidArg)?;
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(chain),
    }))
}

fn cmd_lisp_read_impl(env: &mut Environment, listed: bool) -> Result<Rc<LispObject>, YacasError> {
    let mut tok = {
        let mut stack = env.input_stack.borrow_mut();
        stack
            .last_mut()
            .ok_or(YacasError::Generic("LispRead: no active input stream".to_string()))?
            .take()
            .ok_or(YacasError::Generic("LispRead: input tokenizer missing".to_string()))?
    };
    let r = plain_parse(env, &mut tok, listed);
    env.input_stack
        .borrow_mut()
        .last_mut()
        .expect("input")
        .replace(tok);
    r
}
pub fn cmd_lisp_read(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    cmd_lisp_read_impl(env, false)
}
pub fn cmd_lisp_read_listed(env: &mut Environment, inner: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    cmd_lisp_read_impl(env, true)
}
