//! Debugger callbacks, tracing, and diagnostic state commands.

use std::rc::Rc;

use super::containers::list_of;
use super::integer::int_number;
use super::{arg, arity_of};
use crate::env::Environment;
use crate::errors::YacasError;
use crate::evaluator::eval;
use crate::value::{copy_node, LispObject, ObjectKind};

// ==================== CustomEval family (debugger) ====================
/// CustomEval (Macro|Fixed, arity 4): (entercb, leavecb, errorcb, expr) —
/// installs the debugger state (enter/leave/error callbacks), evaluates expr
/// (each eval step goes through the debug hooks calling the Enter/Leave
/// callbacks), clears the debugger afterwards, and returns expr's result.
/// debug.rep's TraceExp/Debug/TraceRule depend on this.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispCustomEval.
pub fn cmd_custom_eval(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_custom_eval_expression(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let d = env.debugger.borrow();
    let state = d.as_ref().ok_or_else(|| {
        YacasError::Generic(
            "Trying to get CustomEval results while not in custom evaluation".to_string(),
        )
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
pub fn cmd_custom_eval_result(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let d = env.debugger.borrow();
    let state = d.as_ref().ok_or_else(|| {
        YacasError::Generic(
            "Trying to get CustomEval results while not in custom evaluation".to_string(),
        )
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
pub fn cmd_custom_eval_locals(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
    let kinds: Vec<ObjectKind> = names.iter().map(|n| ObjectKind::Atom(n.clone())).collect();
    Ok(list_of(kinds, env))
}

/// CustomEval'Stop (Function|Fixed, arity 0): sets debugger.stopped (later
/// evals raise to abort); errors when no debugger is active. Returns True.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispCustomEvalStop.
pub fn cmd_custom_eval_stop(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let mut d = env.debugger.borrow_mut();
    let state = d.as_mut().ok_or_else(|| {
        YacasError::Generic(
            "Trying to get CustomEval results while not in custom evaluation".to_string(),
        )
    })?;
    state.stopped = true;
    Ok(env.true_atom())
}

/// InDebugMode (see upstream: cyacas/libyacas/src/mathcommands2.cpp
/// LispInDebugMode): non-debug -> False.
pub fn cmd_in_debug_mode(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_debug_file(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let _ = eval(env, arg(inner, 0)?)?;
    Err(YacasError::generic(
        "Cannot call DebugFile in non-debug version of Yacas",
    ))
}
pub fn cmd_debug_line(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
fn pretty_set(
    env: &mut Environment,
    which: bool,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_pretty_reader_set(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    pretty_set(env, true, inner)
}
pub fn cmd_pretty_reader_get(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    pretty_get(env, true)
}
pub fn cmd_pretty_printer_set(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    pretty_set(env, false, inner)
}
pub fn cmd_pretty_printer_get(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    pretty_get(env, false)
}

/// CurrentFile (see upstream: cyacas/libyacas/src/mathcommands3.cpp
/// LispCurrentFile): the input state file name (quoted); defaults to
/// "CommandLine".
pub fn cmd_current_file(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let f = env.input_file.borrow().clone();
    let name = if f.is_empty() {
        "CommandLine".to_string()
    } else {
        f
    };
    Ok(crate::value::make_atom(
        &mut env.symtab,
        &format!("\"{name}\""),
    ))
}

/// CurrentLine (see upstream: cyacas/libyacas/src/mathcommands3.cpp
/// LispCurrentLine): the active input tokenizer's consumed-'\n' count + 1;
/// no active input -> 1.
pub fn cmd_current_line(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_math_debug_info(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_trace_rule(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let head = arg(inner, 0)?;
    let target: Option<(Rc<str>, usize)> = match &head.kind {
        ObjectKind::Sublist(sub) => {
            let mut it = crate::value::spine_refs(sub);
            let first = it.next();
            first
                .and_then(|f| f.atom_string())
                .map(|h| (h.clone(), crate::standard::internal_list_length(head) - 1))
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

pub fn cmd_trace_stack(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let body = arg(inner, 0)?.clone();
    eval(env, &body)
}
