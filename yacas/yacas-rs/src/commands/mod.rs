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

mod bases;
mod containers;
mod debug;
mod integer;
mod io;
mod lists;
mod numeric;
mod operators;
mod predicates;
mod rules;

pub use bases::{cmd_from_base, cmd_to_base};
pub use containers::{
    cmd_array_create, cmd_array_get, cmd_array_set, cmd_array_size, cmd_assoc_contains,
    cmd_assoc_create, cmd_assoc_drop, cmd_assoc_get, cmd_assoc_head, cmd_assoc_keys, cmd_assoc_set,
    cmd_assoc_size, cmd_assoc_to_list,
};
pub use debug::{
    cmd_current_file, cmd_current_line, cmd_custom_eval, cmd_custom_eval_expression,
    cmd_custom_eval_locals, cmd_custom_eval_result, cmd_custom_eval_stop, cmd_debug_file,
    cmd_debug_line, cmd_in_debug_mode, cmd_math_debug_info, cmd_pretty_printer_get,
    cmd_pretty_printer_set, cmd_pretty_reader_get, cmd_pretty_reader_set, cmd_trace_rule,
    cmd_trace_stack,
};
pub use integer::{
    cmd_bit_and, cmd_bit_or, cmd_bit_xor, cmd_math_div, cmd_math_fac, cmd_math_gcd, cmd_mod,
    cmd_shift_left, cmd_shift_right,
};
pub use io::{
    cmd_from_file, cmd_from_string, cmd_read, cmd_read_token, cmd_secure, cmd_system_call,
    cmd_system_name, cmd_tmp_file, cmd_to_file, cmd_to_stdout, cmd_to_string, cmd_write,
    cmd_write_string,
};
pub use lists::{
    cmd_delete, cmd_destructive_delete, cmd_destructive_insert, cmd_destructive_replace, cmd_head,
    cmd_insert, cmd_length, cmd_list, cmd_listify, cmd_math_nth, cmd_replace, cmd_reverse,
    cmd_tail, cmd_unlist,
};
use numeric::{
    cmd_bits_to_digits, cmd_digits_to_bits, cmd_fast_arc_sin, cmd_fast_log, cmd_fast_power,
    cmd_math_bit_count, cmd_math_get_exact_bits, cmd_math_mul2_exp, cmd_math_set_exact_bits,
    map_numeric_work_error, num_text,
};
pub use numeric::{
    cmd_fast_is_prime, cmd_math_abs, cmd_math_ceil, cmd_math_floor, cmd_math_negate, cmd_math_sign,
    cmd_precision_get, cmd_precision_set,
};
pub use operators::{
    cmd_bodied, cmd_infix, cmd_is_bodied, cmd_is_infix, cmd_is_postfix, cmd_is_prefix,
    cmd_left_precedence, cmd_op_left_precedence, cmd_op_precedence, cmd_op_right_precedence,
    cmd_postfix, cmd_prefix, cmd_right_associative, cmd_right_precedence,
};

pub use predicates::{
    cmd_assume, cmd_clear_assumptions, cmd_is_assumed, cmd_is_assumed_value, cmd_is_atom,
    cmd_is_function, cmd_is_integer, cmd_is_list, cmd_is_negative_integer,
    cmd_is_non_negative_integer, cmd_is_non_positive_integer, cmd_is_number,
    cmd_is_positive_integer, cmd_is_string, cmd_pop_assumptions, cmd_push_assumptions,
};
pub use rules::{
    cmd_def_load, cmd_def_macro_rule_base, cmd_def_macro_rule_base_listed, cmd_hold_arg,
    cmd_macro_rule, cmd_macro_rule_base, cmd_macro_rule_base_listed, cmd_macro_rule_pattern,
    cmd_pattern_create, cmd_retract, cmd_rule, cmd_rule_base, cmd_rule_base_arg_list,
    cmd_rule_base_listed, cmd_rule_pattern, cmd_un_fence,
};

use std::rc::Rc;

use crate::env::Environment;
use crate::errors::YacasError;
use crate::evaluator::eval;
use crate::standard::{internal_equals, is_false, is_true};
use crate::value::{copy_node, spine_kinds, spine_refs, LispObject, ObjectKind};

// Fetch argument i (the command's ARGUMENT(i)): the i-th argument of the inner
// chain; an error if absent.
fn arg(inner: &Rc<LispObject>, i: usize) -> Result<&Rc<LispObject>, YacasError> {
    spine_refs(inner)
        .nth(i + 1)
        .ok_or(YacasError::WrongNumberOfArgs)
}

// Arity: InternalListLength(head) - 1.
fn arity_of(inner: &Rc<LispObject>) -> usize {
    crate::standard::internal_list_length(inner) - 1
}

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
pub fn cmd_not(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_equals(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let a = eval(env, arg(inner, 0)?)?;
    let b = eval(env, arg(inner, 1)?)?;
    if internal_equals(env, &a, &b) {
        Ok(env.true_atom())
    } else {
        Ok(env.false_atom())
    }
}

/// String (See upstream: cyacas/libyacas/src/mathcommands.cpp InternalStringify; Function flag: argument evaluated):
/// returns the argument's text, unconditionally wrapped in quotes. Upstream behavior:
/// String("xx") -> ""xx"", String(aa) -> "aa", String(12) -> "12", and String(f(x))
/// is InvalidArg (sublists have no text form).
pub fn cmd_string(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_type(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    let text = match v.atom_string() {
        Some(_) => "\"\"".to_string(), // Upstream behavior: the non-list branch yields an empty string.
        None => match v.sublist() {
            Some(sub) => match sub.atom_string() {
                Some(s) => format!("\"{s}\""), // Quoted string form (as the stringify lookup produces).
                None => "\"\"".into(), // Non-atomic head -> empty string (same Java branch).
            },
            None => "\"\"".into(), // Numbers/other non-lists -> empty string (Type(5) -> "").
        },
    };
    let sym = env.symtab.look_up(&text);
    Ok(LispObject::atom(sym))
}

/// RuleBaseDefined (See upstream: cyacas/libyacas/src/mathcommands.cpp LispRuleBaseDefined): evaluate both arguments;
/// the name goes through SymbolName (unquote + intern) before the lookup. True when
/// a rule base with that name and arity exists.
pub fn cmd_rule_base_defined(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_def_load_function(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let raw = name_node.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::symbol_name(env, raw);
    let entry = env.user_functions.entry(name).or_default();
    entry.inner.borrow_mut().file_to_open = None;
    Ok(env.true_atom())
}

/// Use (See upstream: cyacas/libyacas/src/mathcommands.cpp LispUse): (fileName) —
/// evaluate the argument (Function|Fixed), unquote, then InternalUse (goes through
/// the def registry; already-loaded files are skipped). Used by the yacasinit.ys
/// boot loading entries.
pub fn cmd_use(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s)
        .unwrap_or(s)
        .to_string();
    crate::standard::internal_use(env, &name)?;
    Ok(env.true_atom())
}

/// Load (See upstream: cyacas/libyacas/src/mathcommands.cpp LispLoad): (fileName) —
/// evaluate, unquote, then InternalLoad (does NOT go through the def registry; that
/// is the core difference from Use). Upstream performs a CheckSecure check; here the
/// secure flag only blocks file reads.
pub fn cmd_load(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if env.secure {
        return Err(YacasError::SecurityBreach);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s)
        .unwrap_or(s)
        .to_string();
    crate::standard::internal_load(env, &name)?;
    Ok(env.true_atom())
}

/// Hold (See upstream: cyacas/libyacas/src/mathcommands.cpp LispHold; Macro|Fixed,
/// arguments held): returns a copy of ARGUMENT(1) without evaluating it. yacasinit.ys
/// Defun bodies rely on `Set(fn,Hold(@func))`: without the hold, fn would bind to a
/// (Hold ...) sublist and rule definitions would see a non-atomic function name.
pub fn cmd_hold(
    _env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_subst(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 3 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let from = eval(env, arg(inner, 0)?)?;
    let to = eval(env, arg(inner, 1)?)?;
    let body = eval(env, arg(inner, 2)?)?;
    let mut behaviour = crate::substitute::SubstBehaviourImpl::new(&from, &to);
    crate::standard::internal_substitute(env, &body, &mut behaviour)
}

/// DefaultDirectory (See upstream: cyacas/libyacas/src/mathcommands.cpp
/// LispDefaultDirectory): (directoryName) — evaluate, unquote, then append to the
/// input-directories list (push_back semantics, not replace).
pub fn cmd_default_directory(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s)
        .unwrap_or(s)
        .to_string();
    env.input_directories.push(name);
    Ok(env.true_atom())
}

/// arg (See upstream: cyacas/libyacas/src/mathcommands.cpp LispArg): the i-th command argument, held (Macro call context).
pub fn cmd_arg(
    _env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let i_node = arg(inner, 0)?;
    let i: usize = i_node
        .number_string()
        .and_then(|s| s.parse().ok())
        .ok_or(YacasError::InvalidArg)?;
    let slot = arg(inner, 1)?
        .sublist()
        .ok_or(YacasError::InvalidArg)?
        .next
        .as_ref()
        .ok_or(YacasError::WrongNumberOfArgs)?;
    let v = spine_refs(slot)
        .nth(i - 1)
        .ok_or(YacasError::WrongNumberOfArgs)?;
    Ok(copy_node(v))
}

/// Atom (See upstream: cyacas/libyacas/src/mathcommands.cpp LispAtom): turns the evaluated argument's text (a number or
/// atom) into an atom node.
pub fn cmd_atom(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_concat_strings(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_concat(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_string_mid_get(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
        Some(t) => crate::standard::internal_unstringify(t)
            .unwrap_or(t)
            .to_string(),
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
pub fn cmd_string_mid_set(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 3 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let from = int_text_of(&eval(env, arg(inner, 0)?)?)?;
    if from < 1 {
        return Err(YacasError::InvalidArg);
    }
    let repl_node = eval(env, arg(inner, 1)?)?;
    let repl = match repl_node.atom_string() {
        Some(t) => crate::standard::internal_unstringify(t)
            .unwrap_or(t)
            .to_string(),
        None => return Err(YacasError::InvalidArg),
    };
    let s = eval(env, arg(inner, 2)?)?;
    let text = match s.atom_string() {
        Some(t) => crate::standard::internal_unstringify(t)
            .unwrap_or(t)
            .to_string(),
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
pub fn cmd_math_add(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    cmd_add(env, inner)
}

/// Addition (variadic; the fixed-2-arg MathAdd is cmd_math_add).
pub fn cmd_add(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    // Seed the accumulator with zero at prec=0 (a storage form, not a truncation
    // cap) — the upstream starts from BigNumber("0", BinaryPrecision()): the
    // environment precision only enters through the add's precision parameter, so
    // results are truncated at the session precision.
    let mut acc = crate::number::float::Float::from_decimal("0")
        .expect("zero")
        .with_prec(0);
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
        return Ok(Rc::new(LispObject {
            next: None,
            kind: ObjectKind::Sublist(chain),
        }));
    }
    let num = crate::value::LispNumber::from_float_flag(acc, any_float);
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Number(num),
    }))
}

/// `*` (See upstream: cyacas/libyacas/src/mathcommands.cpp LispMultiply): multiplies numbers.
pub fn cmd_mul(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let mut acc = crate::number::float::Float::from_decimal("1")
        .expect("one")
        .with_prec(0);
    let mut symbolic = false;
    let mut any_float = false; // Float contamination.
    let n = arity_of(inner);
    for i in 0..n {
        let v = eval(env, arg(inner, i)?)?;
        match &v.kind {
            ObjectKind::Number(num) => {
                any_float |= num.is_float();
                acc = acc
                    .mul_with_limits(&num.float_at(env.precision()), env.precision(), || {
                        env.check_eval_deadline().is_err()
                    })
                    .map_err(map_numeric_work_error)?
            }
            _ => symbolic = true,
        }
    }
    if symbolic {
        return crate::standard::return_un_evaluated(env, inner);
    }
    let num = crate::value::LispNumber::from_float_flag(acc, any_float);
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Number(num),
    }))
}

/// `-` (See upstream: cyacas/libyacas/src/mathcommands.cpp LispSubtract): one argument negates; more arguments subtract in
/// sequence.
pub fn cmd_sub(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
            return Ok(Rc::new(LispObject {
                next: None,
                kind: ObjectKind::Number(num),
            }));
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
                let f = a
                    .float_at(env.precision())
                    .sub(&b.float_at(env.precision()), env.precision());
                acc = Rc::new(LispObject {
                    next: None,
                    kind: ObjectKind::Number(crate::value::LispNumber::from_float_flag(
                        f, any_float,
                    )),
                });
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
pub fn cmd_div(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
                let f = a
                    .float_at(env.precision())
                    .div_with_limits(&b.float_at(env.precision()), env.precision(), || {
                        env.check_eval_deadline().is_err()
                    })
                    .map_err(map_numeric_work_error)?
                    .ok_or(YacasError::DivideByZero)?;
                acc = Rc::new(LispObject {
                    next: None,
                    kind: ObjectKind::Number(crate::value::LispNumber::from_float_flag(
                        f, any_float,
                    )),
                });
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
pub fn cmd_less(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let va = eval(env, arg(inner, 0)?)?;
    let a = num_or_text(env, &va)?;
    let vb = eval(env, arg(inner, 1)?)?;
    let b = num_or_text(env, &vb)?;
    let lt = match (&a, &b) {
        (OrderVal::Num(x), OrderVal::Num(y)) => x.less_than(y),
        (OrderVal::Text(x), OrderVal::Text(y)) => x < y,
        _ => return Err(YacasError::InvalidArg),
    };
    if lt {
        Ok(env.true_atom())
    } else {
        Ok(env.false_atom())
    }
}
pub fn cmd_greater(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let va = eval(env, arg(inner, 0)?)?;
    let a = num_or_text(env, &va)?;
    let vb = eval(env, arg(inner, 1)?)?;
    let b = num_or_text(env, &vb)?;
    let gt = match (&a, &b) {
        (OrderVal::Num(x), OrderVal::Num(y)) => y.less_than(x),
        (OrderVal::Text(x), OrderVal::Text(y)) => x > y,
        _ => return Err(YacasError::InvalidArg),
    };
    if gt {
        Ok(env.true_atom())
    } else {
        Ok(env.false_atom())
    }
}
pub fn cmd_less_eq(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let va = eval(env, arg(inner, 0)?)?;
    let a = num_or_text(env, &va)?;
    let vb = eval(env, arg(inner, 1)?)?;
    let b = num_or_text(env, &vb)?;
    let le = match (&a, &b) {
        (OrderVal::Num(x), OrderVal::Num(y)) => x.less_than(y) || x.equals(y),
        (OrderVal::Text(x), OrderVal::Text(y)) => x <= y,
        _ => return Err(YacasError::InvalidArg),
    };
    if le {
        Ok(env.true_atom())
    } else {
        Ok(env.false_atom())
    }
}
pub fn cmd_greater_eq(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_prog(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_while(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
        None => node
            .atom_string()
            .ok_or(YacasError::InvalidArg)?
            .to_string(),
    };
    t.trim().parse().map_err(|_| YacasError::InvalidArg)
}

/// And — evaluate arguments one by one (Macro|Variable, short-circuit): return
/// False immediately on a False argument; collect non-boolean values; a single
/// non-boolean is returned as-is; multiple ones are repacked as {And, original
/// order}; all-boolean returns True.
pub fn cmd_and(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
fn repack_list(
    env: &mut Environment,
    head: &str,
    items: &[Rc<LispObject>],
) -> Result<Rc<LispObject>, YacasError> {
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

/// Eval — the argument is pre-evaluated by the evaluator, then InternalEval'd
/// once more inside the command: **two evaluations total**. Rust commands receive
/// raw arguments, so the two evals are explicit here (upstream behavior: within
/// the rule body of `aa:=5`, `Eval(aLeftAssign)` first yields the variable name
/// aa, then 5; a single evaluation would stop at aa).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispEval (Function flag).
pub fn cmd_eval(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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

/// BackQuote — the argument is @-substituted and then evaluated. Required by the
/// `...@...` statements inside Macro/Function rule bodies of
/// deffunc.rep/code.ys. Registered under the name "`".
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispBackQuote.
pub fn cmd_back_quote(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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

/// IsGeneric — reports whether the argument is a Generic object (Pattern/Array
/// etc.).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispIsGeneric.
pub fn cmd_is_generic(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    Ok(if matches!(v.kind, ObjectKind::Generic(_)) {
        env.true_atom()
    } else {
        env.false_atom()
    })
}

/// GenericTypeName — the type name of a Generic object, as a quoted string;
/// non-Generic -> InvalidArg.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispGenericTypeName.
pub fn cmd_generic_type_name(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_pattern_matches(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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

/// FullForm — evaluates the argument, prints it in FullForm format to the current
/// output using the **local LispPrinter** (not CurrentPrinter) plus a newline,
/// and returns the evaluated argument (not True). Used by scripts such as
/// showq*.ys for debugging.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispFullForm (Function|Fixed).
pub fn cmd_full_form(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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

/// Check (Macro|Fixed): (pred, message) -> evaluates the predicate; if not
/// True, evaluates the message (must be a string) and raises it as the error
/// (like upstream LispErrUser). Used throughout the scripts for argument
/// validation (e.g. limit.rep's Check(IsAtom(var),"...")).
/// See upstream: cyacas/libyacas/src/corefunctions.h LispCheck.
pub fn cmd_check(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let pred = eval(env, arg(inner, 0)?)?;
    if !crate::standard::is_true(env, &pred) {
        let msg = eval(env, arg(inner, 1)?)?;
        let text = match msg.atom_string() {
            Some(s) => crate::standard::internal_unstringify(s)
                .unwrap_or(s)
                .to_string(),
            None => {
                // Non-string message (like CheckArgIsString): quoted-string atoms or numbers are both InvalidArg
                return Err(YacasError::InvalidArg);
            }
        };
        return Err(YacasError::Generic(text));
    }
    Ok(pred)
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

/// TrapError (Macro|Fixed): (body, handler) -> try to evaluate body; on error
/// (the error message lands in env.error_output) and a non-empty buffer,
/// evaluate the handler (typically Set(errorString,GetCoreError())) and clear
/// the buffer. The N macro `TrapError(Set(result,@expr), Set(errorString,GetCoreError()))`
/// depends on this.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispTrapError.
pub fn cmd_trap_error(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_get_core_error(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_builtin_assoc(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_local_symbols(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_apply_pure(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_flat_copy(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_math_subtract(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
    let add = |env: &mut Environment,
               name: &'static str,
               func: fn(
        &mut Environment,
        &Rc<LispObject>,
    ) -> Result<Rc<LispObject>, YacasError>| {
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
    add(
        env,
        "DefMacroRuleBaseListed",
        cmd_def_macro_rule_base_listed,
    );
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

/// StrictTotalOrder (see upstream: cyacas/libyacas/src/standard.cpp
/// LispStrictTotalOrder / InternalStrictTotalOrder): strict total order
/// (used for sort key ordering; same ordering as std::map).
/// Identical pointers -> False; numbers < non-numbers; two numbers compared by
/// value (equal -> compare the chain tail next); strings by strcmp (equal ->
/// compare the tail); sublists compared element by element with InternalEquals,
/// recursing on the first unequal pair; shorter first, equal length -> False.
fn strict_less(
    env: &mut Environment,
    e1: &Rc<LispObject>,
    e2: &Rc<LispObject>,
) -> Result<bool, YacasError> {
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
fn strict_tail(
    env: &mut Environment,
    e1: &Rc<LispObject>,
    e2: &Rc<LispObject>,
) -> Result<bool, YacasError> {
    match (&e1.next, &e2.next) {
        (None, None) => Ok(false),
        (None, Some(_)) => Ok(true),
        (Some(_), None) => Ok(false),
        (Some(t1), Some(t2)) => strict_less(env, t1, t2),
    }
}

pub fn cmd_strict_total_order(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_math_is_small(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_char_string(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
    Ok(crate::value::make_atom(
        &mut env.symtab,
        &format!("\"{}\"", byte as char),
    ))
}

/// Version (see upstream: cyacas/libyacas/src/mathcommands3.cpp LispVersion;
/// the vendored build leaves YACAS_VERSION empty -> ""). Interpreter
/// (see upstream: cyacas/libyacas/include/yacas/corefunctions.h "Interpreter")
/// -> "yacas".
pub fn cmd_version(
    env: &mut Environment,
    _inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(_inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    Ok(crate::value::make_atom(&mut env.symtab, "\"\""))
}
pub fn cmd_interpreter(
    env: &mut Environment,
    _inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(_inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    Ok(crate::value::make_atom(&mut env.symtab, "\"yacas\""))
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

/// FindFile (see upstream: cyacas/libyacas/src/mathcommands.cpp LispFindFile):
/// searches input_directories; on a hit -> quoted path, otherwise "".
/// Raises an error in Secure mode.
pub fn cmd_find_file(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if env.secure {
        return Err(YacasError::SecurityBreach);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s)
        .unwrap_or(s)
        .to_string();
    let path = crate::standard::internal_find_file(env, &name).unwrap_or_default();
    Ok(crate::value::make_atom(
        &mut env.symtab,
        &format!("\"{path}\""),
    ))
}

/// FindFunction (see upstream: cyacas/libyacas/src/mathcommands.cpp
/// LispFindFunction): the defining file name (quoted) of a user function;
/// none -> the bare symbol Empty (same as cyacas, unquoted).
pub fn cmd_find_function(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if env.secure {
        return Err(YacasError::SecurityBreach);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s)
        .unwrap_or(s)
        .to_string();
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
pub fn cmd_max_eval_depth(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_garbage_collect(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    Ok(env.true_atom())
}

/// XmlTokenizer()/DefaultTokenizer() (see upstream:
/// cyacas/libyacas/src/mathcommands3.cpp LispXmlTokenizer / LispDefaultTokenizer):
/// switch the current tokenizer mode (environment-level flag, synchronized
/// with the active input).
pub fn cmd_xml_tokenizer(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    env.xml_tokenizer.set(true);
    if let Some(Some(t)) = env.input_stack.borrow_mut().last_mut() {
        t.xml = true;
    }
    Ok(env.true_atom())
}
pub fn cmd_default_tokenizer(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_xml_explode_tag(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_patch_load(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let fname = crate::standard::internal_unstringify(s)
        .unwrap_or(s)
        .to_string();
    let path = crate::standard::internal_find_file(env, &fname).ok_or(YacasError::FileNotFound)?;
    let text = std::fs::read_to_string(&path).map_err(|_| YacasError::FileNotFound)?;
    patch_process(env, &text)?;
    Ok(env.true_atom())
}

/// PatchString (see upstream: cyacas/libyacas/src/mathcommands3.cpp
/// LispPatchString): patches into a fresh buffer -> quoted string (stringify
/// does not escape embedded quotes, as in C++).
pub fn cmd_patch_string(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let content = crate::standard::internal_unstringify(s)
        .unwrap_or(s)
        .to_string();
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
fn plain_parse(
    env: &mut Environment,
    tok: &mut crate::tokenizer::Tokenizer,
    listed: bool,
) -> Result<Rc<LispObject>, YacasError> {
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
            .ok_or(YacasError::Generic(
                "LispRead: no active input stream".to_string(),
            ))?
            .take()
            .ok_or(YacasError::Generic(
                "LispRead: input tokenizer missing".to_string(),
            ))?
    };
    let r = plain_parse(env, &mut tok, listed);
    env.input_stack
        .borrow_mut()
        .last_mut()
        .expect("input")
        .replace(tok);
    r
}
pub fn cmd_lisp_read(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    cmd_lisp_read_impl(env, false)
}
pub fn cmd_lisp_read_listed(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    cmd_lisp_read_impl(env, true)
}
