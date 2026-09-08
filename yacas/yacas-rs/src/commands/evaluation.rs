use std::rc::Rc;

use super::{arg, arity_of};
use crate::env::Environment;
use crate::errors::YacasError;
use crate::evaluator::eval;
use crate::value::{copy_node, spine_refs, LispObject, ObjectKind};

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
