use std::rc::Rc;

use super::{arg, arity_of};
use crate::env::Environment;
use crate::errors::YacasError;
use crate::evaluator::eval;
use crate::standard::{internal_equals, is_false, is_true};
use crate::value::{copy_node, spine_kinds, spine_refs, LispObject, ObjectKind};

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
