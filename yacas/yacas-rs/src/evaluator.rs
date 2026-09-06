//! Basic evaluator. See upstream `cyacas/libyacas/src/lispeval.cpp`
//! (`BasicEvaluator::Eval`).
//!
//! Dispatch order: depth check → atom (string literal copied verbatim /
//! variable fetched from the global frame and copied / literal copied) →
//! list (string head: core commands → user function (with `.def` lazy
//! loading) → return unevaluated; non-string head → `apply_pure`) →
//! non-list copied.
//!
//! Contract: the result must be an exclusive copy — callers may rewrite its
//! `next` pointer.

use std::rc::Rc;

use crate::env::Environment;
use crate::errors::YacasError;
use crate::value::{copy_node, LispObject};

/// A core command: evaluation function plus `Hold`/`UnFence` argument info.
pub struct CoreCommand {
    pub name: &'static str,
    pub func: fn(&mut Environment, call: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError>,
    pub hold_args: Vec<&'static str>,
    pub un_fenced: Vec<&'static str>,
}

/// Evaluate an owned expression.
pub fn eval_owned(env: &mut Environment, expr: Rc<LispObject>) -> Result<Rc<LispObject>, crate::errors::YacasError> { eval(env, &expr) }

pub fn eval(env: &mut Environment, expr: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    env.eval_depth += 1;
    if env.eval_depth > env.max_eval_depth {
        let err = if env.eval_depth > env.max_eval_depth + 20 {
            YacasError::UserInterrupt
        } else {
            YacasError::MaxRecurseDepthReached
        };
        env.eval_depth -= 1;
        return Err(err);
    }
    // Debugger hooks (see upstream `TracedEvaluator::Eval` +
    // `DefaultDebugger`): while CustomEval is active (debugger set) and this
    // is not a callback evaluation, every sub-expression first runs
    // `enter_cb`, then evaluates, then runs `leave_cb`. Callback evaluations
    // temporarily set `in_callback` so the hooks do not re-trigger.
    let debug_active = {
        let d = env.debugger.borrow();
        d.as_ref().map(|d| !d.in_callback && !d.stopped).unwrap_or(false)
    };
    let mut err_after_leave: Option<YacasError> = None;
    if debug_active {
        // Enter: record top_expr, then evaluate the enter callback.
        let enter_cb = env.debugger.borrow().as_ref().expect("dbg").enter_cb.clone();
        {
            let mut d = env.debugger.borrow_mut();
            d.as_mut().expect("dbg").top_expr = Some(copy_node(expr));
            d.as_mut().expect("dbg").in_callback = true;
        }
        let _ = eval_cb(env, &enter_cb);
        {
            let mut d = env.debugger.borrow_mut();
            d.as_mut().expect("dbg").in_callback = false;
        }
    }
    let result = eval_inner(env, expr);
    env.eval_depth -= 1;
    let result = match result {
        Ok(v) => v,
        Err(e) => {
            // On error the Leave callback is skipped (the upstream error path
            // goes to the Error callback instead); the error propagates.
            return Err(e);
        }
    };
    if debug_active {
        // Leave: record top_expr/top_result, then evaluate the leave callback.
        let leave_cb = env.debugger.borrow().as_ref().expect("dbg").leave_cb.clone();
        {
            let mut d = env.debugger.borrow_mut();
            d.as_mut().expect("dbg").top_expr = Some(copy_node(expr));
            d.as_mut().expect("dbg").top_result = Some(copy_node(&result));
            d.as_mut().expect("dbg").in_callback = true;
        }
        let cb_err = eval_cb(env, &leave_cb);
        {
            let mut d = env.debugger.borrow_mut();
            d.as_mut().expect("dbg").in_callback = false;
        }
        if let Err(e) = cb_err {
            err_after_leave = Some(e);
        }
    }
    if let Some(e) = err_after_leave {
        return Err(e);
    }
    Ok(result)
}

/// Evaluate a debugger callback. The callbacks are ordinary expressions;
/// `in_callback` is already set, so the hooks do not re-trigger.
fn eval_cb(env: &mut Environment, cb: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    eval(env, cb)
}

/// Evaluation body (depth already checked); returns an exclusive copy.
fn eval_inner(env: &mut Environment, expr: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    // Atoms.
    if let Some(s) = expr.atom_string() {
        // String literals (leading `"`) are copied verbatim.
        if s.starts_with('"') {
            return Ok(copy_node(expr));
        }
        let var = env.get_variable(s.as_ref())?;
        if let Some(v) = var {
            return Ok(copy_node(&v));
        }
        return Ok(copy_node(expr));
    }
    // Lists.
    if let Some(sub_list) = expr.sublist() {
        let head = sub_list;
        if let Some(head_str) = head.atom_string() {
            // 1. Core command table (commands also receive the inner chain).
            if let Some(cmd) = env.core_commands.get(head_str) {
                if std::env::var_os("YACAS_TRACE_LOAD").is_some() {
                    let mut n = 0usize;
                    let mut cur = sub_list.next.as_ref();
                    while let Some(c) = cur {
                        n += 1;
                        cur = c.next.as_ref();
                    }
                    eprintln!("[EVAL-CMD] head={head_str:?} arity={n}");
                }
                return (cmd.func)(env, sub_list);
            }
            // 1b. `:=` with a nested-index lvalue: the left side is a pure
            // `Nth(Nth(...var, i), j)` tree, so the engine writes back to the
            // root variable directly (the observable result of upstream's
            // shared-chain mutation; script rule 10 loses the root-variable
            // identity while decomposing, so this model must recognize the
            // shape here). Other shapes fall back to the script `:=` rules.
            if head_str.as_ref() == ":=" {
                let nargs = {
                    let mut n = 0usize;
                    let mut c = sub_list.next.as_ref();
                    while let Some(x) = c { n += 1; c = x.next.as_ref(); }
                    n
                };
                if nargs == 2 {
                    let left = sub_list.next.as_ref().expect(":= left");
                    let right = sub_list
                        .next.as_ref().expect(":= right1")
                        .next.as_ref().expect(":= right2");
                    if let Some(r) = crate::commands::try_nth_assign(env, left, right)? {
                        return Ok(r);
                    }
                }
            }
            // 2. User function (with `.def` lazy loading); the call node is
            // the inner chain starting at the head.
            if let Some(user_func) = get_user_function(env, expr)? {
                return user_func.evaluate(env, sub_list);
            }
            // 3. Undefined: return unevaluated (args evaluated, head kept).
            return crate::standard::return_un_evaluated(env, sub_list);
        }
        // Non-string head: lambda application.
        return apply_pure(env, expr);
    }
    // Generics etc.: copy.
    Ok(copy_node(expr))
}

/// `GetUserFunction`: look up by name+arity; if the symbol is declared via a
/// `.def` file that is not yet loaded, load it once and retry.
pub fn get_user_function(
    env: &mut Environment,
    call: &Rc<LispObject>,
) -> Result<Option<Rc<dyn crate::userfunc::UserFunction>>, YacasError> {
    let head = call.sublist().expect("get_user_function: call node");
    let name = head.atom_string().expect("head is an atom").clone();
    let arity = crate::standard::internal_list_length(head) - 1;
    let user_func = user_func_lookup(env, &name, arity);
    if let Some(f) = user_func {
        return Ok(Some(f));
    }
    if let Some(name_str) = env.user_functions.get(&name) {
        let file_to_open = name_str.inner.borrow().file_to_open.clone();
        if let Some(def) = file_to_open {
            {
                let mut inner = name_str.inner.borrow_mut();
                inner.file_to_open = None;
            }
            if std::env::var_os("YACAS_TRACE_LOAD").is_some() {
                eprintln!("[LAZY-TRIGGER] {name}/{arity} -> {}", def.file_name);
            }
            let res = crate::standard::internal_use(env, &def.file_name);
            res?;
        }
        return Ok(user_func_lookup(env, &name, arity));
    }
    Ok(None)
}

fn user_func_lookup(
    env: &Environment,
    name: &Rc<str>,
    arity: usize,
) -> Option<Rc<dyn crate::userfunc::UserFunction>> {
    env.user_functions.get(name).and_then(|m| m.user_func(arity))
}

/// Lambda application: `(({x,y} body) args...)` binds x,y to args and
/// evaluates body.
fn apply_pure(env: &mut Environment, call: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let inner = call.sublist().expect("apply_pure: call node");
    let oper = inner;
    let oper_subl = match oper.sublist() {
        Some(s) => s,
        None => return Err(YacasError::Generic("apply_pure: head is not a sublist".into())),
    };
    let oper2 = oper_subl.next.as_ref().ok_or_else(|| {
        let s = crate::printer::infix_print(env, call);
        YacasError::Generic(format!("apply_pure(oper2 chain) on {s}"))
    })?;
    // oper2 is the parameter-list sublist; the body follows it.
    let body = oper2.next.as_ref().ok_or_else(|| {
        let s = crate::printer::infix_print(env, call);
        YacasError::Generic(format!("apply_pure(missing body) on {s} | ff={}", crate::printer::full_form(call)))
    })?;
    let params = match oper2.sublist() {
        Some(p) => p,
        None => return Err(YacasError::Generic("apply_pure: params is not a sublist".into())),
    };
    let args_cur = inner.next.as_ref().ok_or_else(|| YacasError::Generic("apply_pure: missing arguments".into()))?;
    let s = crate::printer::infix_print(env, call);
    env.push_local_frame(false);
    let result = (|| {
        // The parameter content chain skips the `List` head before binding;
        // binding without the skip would pair List with the first argument.
        let mut p = params.next.as_ref();
        let mut a = Some(args_cur);
        while let Some(pp) = p {
            let var = pp.atom_string().ok_or(YacasError::InvalidArg)?.clone();
            let av = a.ok_or_else(|| YacasError::Generic(format!("apply_pure(arg) on {s}")))?;
            let node = copy_node(av); // exclusive copy
            env.new_local(var, Some(node));
            p = pp.next.as_ref();
            a = av.next.as_ref();
        }
        if a.is_some() {
            return Err(YacasError::Generic("apply_pure: more arguments than parameters".into()));
        }
        crate::evaluator::eval(env, body)
    })();
    env.pop_local_frame()?;
    result
}
