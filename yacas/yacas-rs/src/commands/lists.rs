//! List construction, access, and structural mutation commands.

use std::rc::Rc;

use super::{arg, arity_of, int_text_of, write_back_arg0};
use crate::env::Environment;
use crate::errors::YacasError;
use crate::evaluator::eval;
use crate::value::{spine_kinds, LispObject, ObjectKind};

/// Head (See upstream: cyacas/libyacas/src/mathcommands.cpp LispHead = InternalNth(ARG, 1)): step one link into the
/// inner chain and return the first element. Upstream behavior: Head({a,b,c}) -> a.
pub fn cmd_head(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    let sub = v.sublist().ok_or(YacasError::NotList)?;
    crate::standard::internal_nth(sub, 1)
}

/// Tail (See upstream: cyacas/libyacas/src/mathcommands.cpp LispTail): returns (List rest...) — everything past the head,
/// wrapped in a List head.
pub fn cmd_tail(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_length(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_listify(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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

/// UnList (See upstream: cyacas/libyacas/src/mathcommands.cpp LispUnList): the evaluated argument must be a
/// List-headed sublist; returns the *call-shaped* sublist with the List head removed
/// (the tail is returned as a call expression, not a bare
/// element chain). The if-else fallback rule body `UnList({Atom("else"),...})`
/// relies on this expanding into a proper call that the printer renders with infix
/// operators: if(3) 11 else 22 -> "if(3)11 else 22".
pub fn cmd_unlist(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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

/// List — evaluate arguments one by one and build {List,e1..eN}.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispList (Macro|Variable).
pub fn cmd_list(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_math_nth(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let list_node = eval(env, arg(inner, 0)?)?;
    let index_node = eval(env, arg(inner, 1)?)?;
    let index = int_text_of(&index_node)?;
    let index = usize::try_from(index).map_err(|_| YacasError::InvalidArg)?;
    if std::env::var_os("YACAS_TRACE_LOAD").is_some() {
        eprintln!(
            "[MATHNTH] arg1={} idx={index}",
            crate::printer::infix_print(env, &list_node)
        );
    }
    let sub = list_node.sublist().ok_or(YacasError::NotList)?;
    crate::standard::internal_nth(sub, index)
}

/// Element chain of a list (kinds; skips the List head), matching upstream
/// SubList().Next() semantics. An empty list `{}` (content chain with only the
/// List head, no elements) -> empty Vec (upstream Reverse({})={} does not error).
fn element_kinds(
    _env: &mut Environment,
    node: &Rc<LispObject>,
) -> Result<Vec<ObjectKind>, YacasError> {
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
pub fn cmd_reverse(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
    let result = Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(chain),
    });
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
pub fn cmd_destructive_insert(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let result = cmd_insert(env, inner)?;
    write_back_arg0(env, inner, &result)?;
    Ok(result)
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

pub fn cmd_delete(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    internal_delete(env, inner, false)
}
pub fn cmd_destructive_delete(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    internal_delete(env, inner, true)
}

/// Insert / DestructiveInsert (like upstream InternalInsert): 1-based; inserts
/// before the element reached at step index; index = length + 1 -> append at
/// the end; out of range -> ListNotLongEnough. Insert(aa,1,xx) -> {xx,1,2,3}.
/// Note: the only observable difference of the destructive variant is
/// in-place mutation — Rust rebuilds the list; in-place writeback is handled
/// by the call layer.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp InternalInsert.
pub fn cmd_insert(
    env: &mut Environment,
    inner: &Rc<LispObject>,
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
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(chain),
    }))
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
    let result = Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(chain),
    });
    if destructive {
        write_back_arg0(env, inner, &result)?;
    }
    Ok(result)
}
pub fn cmd_replace(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    internal_replace(env, inner, false)
}
pub fn cmd_destructive_replace(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    internal_replace(env, inner, true)
}
