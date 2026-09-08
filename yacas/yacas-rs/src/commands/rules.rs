//! Rule-base declaration, pattern registration, and lazy definition loading.

use std::rc::Rc;

use super::{arg, arity_of, int_text_of, symbol_name_of};
use crate::env::Environment;
use crate::errors::YacasError;
use crate::evaluator::eval;
use crate::value::{LispObject, ObjectKind};

/// MacroRuleBase (See upstream: cyacas/libyacas/src/mathcommands.cpp LispMacroRuleBase; Function flag: arguments
/// already evaluated): (name, {params...}) — registers a rule-base function. The
/// parameter chain is the argument sublist minus its List head (Java:
/// args.SubList().Get().Next()); the name goes through SymbolName.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispMacroRuleBase ->
/// InternalRuleBase -> DeclareRuleBase: this creates a *branched* (rule-base)
/// function, NOT a macro — macro/pattern binding happens at rule-match time.
pub fn cmd_macro_rule_base(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let name_node = eval(env, arg(inner, 0)?)?;
    let name = {
        let s = name_node.atom_string().ok_or(YacasError::InvalidArg)?;
        crate::standard::symbol_name(env, s)
    };
    let params = params_opt_of(env, {
        let _a1 = arg(inner, 1)?;
        _a1
    })?;
    env.declare_rule_base(name, params.as_ref(), false)?;
    Ok(env.true_atom())
}
/// Pattern'Create (the script-level Pattern'Create shape): variable-name chain +
/// post-predicate -> a Pattern generic object.
pub fn cmd_pattern_create(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_macro_rule_pattern(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
    let arity: usize = arity_node
        .number_string()
        .and_then(|s| s.parse().ok())
        .ok_or(YacasError::InvalidArg)?;
    let prec_node = eval(env, arg(inner, 2)?)?;
    let prec: i32 = prec_node
        .number_string()
        .and_then(|s| s.parse().ok())
        .ok_or(YacasError::InvalidArg)?;
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

// Rule-base parameters: an empty `{}` is legal upstream (a 0-arity function;
// standard.ys InNumericMode/InVerboseMode etc. use `Function("X",{}) body`).
// `{}` -> None; otherwise the first-element chain.
fn params_opt_of(
    env: &mut Environment,
    node: &Rc<LispObject>,
) -> Result<Option<Rc<LispObject>>, YacasError> {
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
pub fn cmd_rule_base(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = symbol_name_of(env, {
        let _a0 = arg(inner, 0)?;
        _a0
    })?;
    // The parameter list is NOT evaluated (matching upstream InternalRuleBase's
    // `LispPtr args(ARGUMENT(2))`: the argument is taken directly, the same path as
    // MacroRuleBase). Evaluating it first would substitute current bindings for the
    // parameter names whenever loading happens in a scope where those names are
    // bound, so the function body's variables would resolve to wrong values.
    let params = params_hold_opt_of({
        let _a1 = arg(inner, 1)?;
        _a1
    })?;
    env.declare_rule_base(name, params.as_ref(), false)?;
    Ok(env.true_atom())
}

/// RuleBaseListed (See upstream: cyacas/libyacas/src/mathcommands.cpp LispRuleBaseListed): same as RuleBase with
/// listed=true (the parameter list is likewise not evaluated).
pub fn cmd_rule_base_listed(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = symbol_name_of(env, {
        let _a0 = arg(inner, 0)?;
        _a0
    })?;
    let params = params_hold_opt_of({
        let _a1 = arg(inner, 1)?;
        _a1
    })?;
    env.declare_rule_base(name, params.as_ref(), true)?;
    Ok(env.true_atom())
}

/// MacroRuleBaseListed (See upstream: cyacas/libyacas/src/mathcommands.cpp LispMacroRuleBaseListed; Function flag:
/// arguments already evaluated).
pub fn cmd_macro_rule_base_listed(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let name = symbol_name_of(env, &name_node)?;
    let params = params_opt_of(env, {
        let _a1 = arg(inner, 1)?;
        _a1
    })?;
    // Upstream behavior (InternalRuleBase with listed): a branched (rule-base) function, same
    // family as RuleBaseListed — not a macro.
    env.declare_rule_base(name, params.as_ref(), true)?;
    Ok(env.true_atom())
}

/// DefMacroRuleBase (See upstream: cyacas/libyacas/src/mathcommands.cpp InternalDefMacroRuleBase; Macro flag:
/// arguments held): (name, {params...}) — the parameter list is taken unevaluated
/// (ARGUMENT(2) directly), so parameter-name keys keep the atoms as declared.
pub fn cmd_def_macro_rule_base(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = symbol_name_of(env, {
        let _a0 = arg(inner, 0)?;
        _a0
    })?;
    let params = params_hold_opt_of({
        let _a1 = arg(inner, 1)?;
        _a1
    })?;
    env.declare_macro_rule_base(name, params.as_ref(), false)?;
    Ok(env.true_atom())
}

/// DefMacroRuleBaseListed (See upstream: cyacas/libyacas/src/mathcommands.cpp InternalDefMacroRuleBase with
/// aListed=true; Macro flag). Same as cmd_def_macro_rule_base: the parameter list is
/// taken unevaluated (keys keep the atomic names).
pub fn cmd_def_macro_rule_base_listed(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = symbol_name_of(env, {
        let _a0 = arg(inner, 0)?;
        _a0
    })?;
    let params = params_hold_opt_of({
        let _a1 = arg(inner, 1)?;
        _a1
    })?;
    env.declare_macro_rule_base(name, params.as_ref(), true)?;
    Ok(env.true_atom())
}

/// HoldArg (See upstream: cyacas/libyacas/src/mathcommands.cpp LispHoldArg; Macro): (name, paramName) — adds the
/// parameter to the hold list.
pub fn cmd_hold_arg(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = symbol_name_of(env, {
        let _a0 = arg(inner, 0)?;
        _a0
    })?;
    let var = arg(inner, 1)?
        .atom_string()
        .ok_or(YacasError::InvalidArg)?
        .to_string();
    env.hold_argument(name, &var)?;
    Ok(env.true_atom())
}

/// Rule (See upstream: cyacas/libyacas/src/mathcommands.cpp InternalNewRule; Macro flag: arguments
/// held): (name, arity, precedence, predicate, body) -> env.define_rule.
pub fn cmd_rule(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 5 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = symbol_name_of(env, {
        let _a0 = arg(inner, 0)?;
        _a0
    })?;
    let arity = int_text_of(arg(inner, 1)?)? as i32 as usize;
    let prec = int_text_of(arg(inner, 2)?)? as i32;
    let predicate = arg(inner, 3)?;
    let body = arg(inner, 4)?;
    env.define_rule(name, arity, prec, predicate, body)?;
    Ok(env.true_atom())
}

/// RulePattern (See upstream: cyacas/libyacas/src/mathcommands.cpp InternalNewRulePattern; Macro flag: arguments held):
/// (name, arity, precedence, pattern, body) -> env.define_rule_pattern.
pub fn cmd_rule_pattern(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 5 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = symbol_name_of(env, {
        let _a0 = arg(inner, 0)?;
        _a0
    })?;
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
pub fn cmd_macro_rule(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_un_fence(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_retract(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_rule_base_arg_list(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
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
pub fn cmd_def_load(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if env.secure {
        return Err(YacasError::SecurityBreach);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let s = name_node.atom_string().ok_or(YacasError::InvalidArg)?;
    let def_name = crate::standard::internal_unstringify(s)
        .unwrap_or(s)
        .to_string();
    // LoadDefFile: flatfile = unstringify(name)+".def", opened via directory
    // search; failure -> FileNotFound.
    let flatfile = format!("{def_name}.def");
    let path =
        crate::standard::internal_find_file(env, &flatfile).ok_or(YacasError::FileNotFound)?;
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
    let Environment {
        user_functions,
        def_files,
        symtab,
        protected,
        ..
    } = env;
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
