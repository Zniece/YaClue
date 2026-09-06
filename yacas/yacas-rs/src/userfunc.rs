//! User functions: `BranchingUserFunction`, `MacroUserFunction`, and the
//! `Listed*` variants, dispatched through `MultiUserFunction`. See upstream
//! `cyacas/libyacas/src/mathuserfunc.cpp` and `include/yacas/lispuserfunc.h`.
//!
//! Design notes:
//! - `MultiUserFunction` is held as `Rc` in `env.user_functions`, detached
//!   from `&mut env` so new rules can be inserted while a call is being
//!   evaluated. Interior state lives in a `RefCell`: rule insertion takes
//!   `borrow_mut`, the matching loop takes short `borrow`s, which is what
//!   makes "rules may be inserted during matching" work.
//! - Rules are inserted by precedence (binary search); a rule with the same
//!   precedence is inserted *before* existing ones of that precedence, so
//!   the most recently defined rule wins.
//! - Evaluation: arguments are held or evaluated per the hold flags → push
//!   a (fenced) frame → bind parameters → rule loop (with rollback if the
//!   rule table grew mid-loop) → evaluate the matched body; with no match,
//!   rebuild the call from the evaluated arguments.
//! - Macros: all parameters held + unfenced at construction; a matched
//!   macro body first goes through backquote `@` substitution.
//! - Listed variants relax `is_arity` to `n <= arity`; surplus arguments
//!   are wrapped in a `List`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::env::Environment;
use crate::errors::YacasError;
use crate::value::{copy_node, ObjectKind, LispObject};

/// A formal parameter: name + hold flag.
#[derive(Clone)]
pub struct BranchParameter {
    pub name: Rc<str>,
    pub hold: bool,
}

/// The three rule forms.
pub enum RuleKind {
    /// Predicate rule: the predicate must evaluate to True.
    Predicate { predicate: Rc<LispObject>, body: Rc<LispObject> },
    /// Unconditional rule.
    True { body: Rc<LispObject> },
    /// Pattern rule (a Pattern generic object; matched via
    /// `GenericClass::matches_pattern`).
    Pattern { pattern: Rc<dyn crate::value::GenericClass>, body: Rc<LispObject> },
}

/// One rule, with a unique id used for rollback detection.
pub struct Rule {
    pub id: u64,
    pub precedence: i32,
    pub kind: RuleKind,
}

fn next_rule_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Interior function state.
pub struct FunctionInner {
    pub parameters: Vec<BranchParameter>,
    pub rules: Vec<Rule>,
    pub param_list: Option<Rc<LispObject>>,
    pub fenced: bool,
    pub traced: bool,
}

/// Branching (rule-based) function; the Macro/Listed variants reuse its
/// implementation.
pub struct BranchingUserFunction {
    pub inner: RefCell<FunctionInner>,
}

/// User function interface.
pub trait UserFunction {
    fn arity(&self) -> usize;
    fn is_arity(&self, n: usize) -> bool;
    fn hold_argument(&self, variable: &str);
    fn declare_rule(
        &self,
        precedence: i32,
        predicate: Option<&Rc<LispObject>>,
        body: &Rc<LispObject>,
    ) -> Result<(), YacasError>;
    fn declare_pattern(
        &self,
        precedence: i32,
        pattern: Rc<dyn crate::value::GenericClass>,
        body: &Rc<LispObject>,
    ) -> Result<(), YacasError>;
    fn arg_list(&self) -> Option<Rc<LispObject>>;
    fn fenced(&self) -> bool;
    fn un_fence(&self);
    /// TraceRule flag: set while `TraceRule(head, body)` is active; calls
    /// emit `TrEnter`/`TrLeave` lines.
    fn set_traced(&self, on: bool);
    fn is_traced(&self) -> bool;
    /// Evaluate `call` (a call node including the head).
    fn evaluate(&self, env: &mut Environment, call: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError>;
}

impl BranchingUserFunction {
    /// Build from the parameter list: every node must be an atom (or a
    /// number, whose rendered text becomes the parameter name) — number
    /// atoms occur when `{qq}` evaluates to `{99}` in a macro definition.
    /// A `None`/empty parameter list yields a zero-arity function.
    pub fn new(param_list: Option<&Rc<LispObject>>) -> Result<Self, YacasError> {
        let mut parameters = Vec::new();
        let mut cur = param_list;
        while let Some(node) = cur {
            let name = match node.atom_string() {
                Some(s) => s.clone(),
                None => match &node.kind {
                    crate::value::ObjectKind::Number(n) => Rc::from(n.string()),
                    _ => return Err(YacasError::CreatingUserFunction),
                },
            };
            parameters.push(BranchParameter { name, hold: false });
            cur = node.next.as_ref();
        }
        Ok(BranchingUserFunction {
            inner: RefCell::new(FunctionInner {
                parameters,
                rules: Vec::new(),
                param_list: param_list.cloned(),
                fenced: true,
                traced: false,
            }),
        })
    }

    /// Shared evaluation body for branching and macro functions.
    pub fn evaluate_common(
        &self,
        env: &mut Environment,
        call: &Rc<LispObject>,
        is_macro: bool,
    ) -> Result<Rc<LispObject>, YacasError> {
        let (arity, fenced) = {
            let inner = self.inner.borrow();
            (inner.parameters.len(), inner.fenced)
        };
        // TrEnter fires before argument evaluation, TrLeave after the result
        // is produced.
        let traced = self.inner.borrow().traced;
        if traced {
            trace_show_enter(env, call);
        }

        if std::env::var_os("YACAS_TRACE_LOAD").is_some() {
            let head = call.atom_string().map(|s| s.to_string()).unwrap_or_default();
            eprintln!("[EVAL-FN] {head} arity={arity}");
        }
        // 1) Arguments: held → copy verbatim; otherwise evaluate.
        //    `raw_args` always holds copies of the raw argument nodes for
        //    macro substitution (macro `@x` expands to the raw argument,
        //    e.g. the atom `arity`, not its value).
        let mut arguments: Vec<Rc<LispObject>> = Vec::with_capacity(arity);
        let mut raw_args: Vec<Rc<LispObject>> = Vec::with_capacity(arity);
        // Clone the parameter metadata first and drop the borrow: evaluating
        // arguments or a rule body may re-enter and insert new rules for
        // this very function, which needs `borrow_mut`.
        let params_meta: Vec<(Rc<str>, bool)> = {
            let inner = self.inner.borrow();
            inner.parameters.iter().map(|p| (p.name.clone(), p.hold)).collect()
        };
        {
            let mut cur = call.next.as_ref();
            for (_name, hold) in &params_meta {
                let node = cur.ok_or(YacasError::WrongNumberOfArgs)?;
                raw_args.push(copy_node(node));
                if *hold {
                    // Held arguments must share the caller's Rc chain so that
                    // `propagate_alias` broadcasts reach them.
                    arguments.push(node.clone());
                } else {
                    arguments.push(crate::evaluator::eval(env, node)?);
                }
                cur = node.next.as_ref();
            }
        }
        if traced {
            for i in 0..arity {
                trace_show_arg(env, &raw_args[i], &arguments[i]);
            }
        }
        // 2) Push the frame and bind parameters. Branching functions bind
        //    all arguments; macros bind *none* — their bodies read the
        //    caller's/global bindings, and `Pattern'Create` macros bind
        //    during rule matching instead.
        env.push_local_frame(fenced);
        let rule_result = {
            if !is_macro {
                for i in 0..arity {
                    env.new_local(params_meta[i].0.clone(), Some(arguments[i].clone()));
                }
            }
            self.rule_loop(env, &raw_args, &arguments, is_macro)
        };
        env.pop_local_frame()?;
        let matched = rule_result?;
        let res_obj = match matched {
            Some(result) => result,
            None => {
                // No rule matched: rebuild the call from evaluated arguments.
                rebuild_call(call, &arguments)
            }
        };
        if traced {
            trace_show_leave(env, call, &res_obj);
        }
        Ok(res_obj)
    }

    /// Rule loop: walk the table by index; if the table grew during body
    /// evaluation (rollback detected by id), step back and retry.
    /// `Ok(Some(result))` = matched, `Ok(None)` = no rule matched.
    fn rule_loop(
        &self,
        env: &mut Environment,
        raw_args: &[Rc<LispObject>],
        arguments: &[Rc<LispObject>],
        is_macro: bool,
    ) -> Result<Option<Rc<LispObject>>, YacasError> {
        let mut i: usize = 0;
        loop {
            // Short borrow per rule; released before evaluation so rule
            // insertion during evaluation stays possible.
            let cur = {
                let inner = self.inner.borrow();
                inner.rules.get(i).map(|r| (r.id, rule_kind_clone(&r.kind)))
            };
            let (id, kind) = match cur {
                Some(x) => x,
                None => return Ok(None),
            };
            if std::env::var_os("YACAS_TRACE_LOAD").is_some() {
                match &kind {
                    RuleKind::Predicate { predicate, body } => eprintln!(
                        "[RULE] try id={id} PRED={} BODY={}",
                        crate::printer::infix_print(env, predicate),
                        crate::printer::infix_print(env, body)
                    ),
                    RuleKind::True { body } => eprintln!(
                        "[RULE] try id={id} TRUE BODY={}",
                        crate::printer::infix_print(env, body)
                    ),
                    RuleKind::Pattern { body, .. } => eprintln!(
                        "[RULE] try id={id} PATTERN BODY={}",
                        crate::printer::infix_print(env, body)
                    ),
                }
            }
            let matched = rule_matches(&kind, env, arguments)?;
            if matched {
                let body = rule_body(&kind).expect("a rule always has a body");
                if is_macro {
                    // Macro: substitute `@` references in the body (by
                    // positional pairs of formal name → raw argument), then
                    // evaluate.
                    let pairs: Vec<(Rc<str>, Rc<LispObject>)> = {
                        let inner = self.inner.borrow();
                        inner
                            .parameters
                            .iter()
                            .zip(raw_args.iter())
                            .map(|(p, a)| (p.name.clone(), a.clone()))
                            .collect()
                    };
                    let mut behaviour =
                        crate::substitute::BackQuoteBehaviour::new_with_pairs(env, pairs);
                    let substed = crate::standard::internal_substitute(env, body, &mut behaviour)?;
                    return Ok(Some(crate::evaluator::eval(env, &substed)?));
                }
                // Plain rule: evaluate the body in the parameter frame.
                // Assignments inside the body (Set(local, …)) must write the
                // frame-local parameter, which is what plain evaluation
                // provides; inlining raw argument nodes instead would make
                // `Set(<literal>, …)` fail.
                return crate::evaluator::eval(env, body).map(Some);
            }
            // Rollback: rule i is no longer the same rule (an insertion
            // happened); step back to the first index still holding it.
            let mut j = i;
            while j > 0 {
                let same = self.inner.borrow().rules.get(j).map(|r| r.id == id).unwrap_or(false);
                if same {
                    break;
                }
                j -= 1;
            }
            i = j + 1;
        }
    }
}

impl UserFunction for BranchingUserFunction {
    fn arity(&self) -> usize {
        self.inner.borrow().parameters.len()
    }
    fn is_arity(&self, n: usize) -> bool {
        self.arity() == n
    }
    fn hold_argument(&self, variable: &str) {
        let mut inner = self.inner.borrow_mut();
        for p in &mut inner.parameters {
            if p.name.as_ref() == variable {
                p.hold = true;
            }
        }
    }
    fn declare_rule(
        &self,
        precedence: i32,
        predicate: Option<&Rc<LispObject>>,
        body: &Rc<LispObject>,
    ) -> Result<(), YacasError> {
        let mut inner = self.inner.borrow_mut();
        let kind = match predicate {
            Some(p) => RuleKind::Predicate { predicate: p.clone(), body: body.clone() },
            None => RuleKind::True { body: body.clone() },
        };
        insert_rule(&mut inner.rules, Rule { id: next_rule_id(), precedence, kind });
        Ok(())
    }
    fn declare_pattern(
        &self,
        precedence: i32,
        pattern: Rc<dyn crate::value::GenericClass>,
        body: &Rc<LispObject>,
    ) -> Result<(), YacasError> {
        let mut inner = self.inner.borrow_mut();
        insert_rule(&mut inner.rules, Rule {
            id: next_rule_id(),
            precedence,
            kind: RuleKind::Pattern { pattern, body: body.clone() },
        });
        Ok(())
    }
    fn arg_list(&self) -> Option<Rc<LispObject>> {
        self.inner.borrow().param_list.clone()
    }
    fn fenced(&self) -> bool {
        self.inner.borrow().fenced
    }
    fn un_fence(&self) {
        self.inner.borrow_mut().fenced = false;
    }
    fn set_traced(&self, on: bool) {
        self.inner.borrow_mut().traced = on;
    }
    fn is_traced(&self) -> bool {
        self.inner.borrow().traced
    }
    fn evaluate(&self, env: &mut Environment, call: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
        self.evaluate_common(env, call, false)
    }
}

/// Insert by precedence (ascending: lower precedence tried first); equal
/// precedence inserts before existing entries — the most recently defined
/// rule wins.
fn insert_rule(rules: &mut Vec<Rule>, new_rule: Rule) {
    let precedence = new_rule.precedence;
    let mut low = 0usize;
    let mut high = rules.len();
    if high > 0 {
        if rules[0].precedence > precedence {
            rules.insert(0, new_rule);
            return;
        }
        if rules[high - 1].precedence < precedence {
            rules.push(new_rule);
            return;
        }
    }
    loop {
        if low >= high {
            rules.insert(low, new_rule);
            return;
        }
        let mid = (low + high) >> 1;
        if rules[mid].precedence > precedence {
            high = mid;
        } else if rules[mid].precedence < precedence {
            low = mid + 1;
        } else {
            rules.insert(mid, new_rule);
            return;
        }
    }
}

fn rule_kind_clone(kind: &RuleKind) -> RuleKind {
    match kind {
        RuleKind::Predicate { predicate, body } => RuleKind::Predicate {
            predicate: predicate.clone(),
            body: body.clone(),
        },
        RuleKind::True { body } => RuleKind::True { body: body.clone() },
        RuleKind::Pattern { pattern, body } => RuleKind::Pattern {
            pattern: pattern.clone(),
            body: body.clone(),
        },
    }
}

fn rule_body(kind: &RuleKind) -> Option<&Rc<LispObject>> {
    match kind {
        RuleKind::Predicate { body, .. } | RuleKind::True { body } | RuleKind::Pattern { body, .. } => Some(body),
    }
}

fn rule_matches(
    kind: &RuleKind,
    env: &mut Environment,
    arguments: &[Rc<LispObject>],
) -> Result<bool, YacasError> {
    match kind {
        RuleKind::True { .. } => Ok(true),
        RuleKind::Predicate { predicate, .. } => {
            let p = crate::evaluator::eval(env, predicate)?;
            Ok(crate::standard::is_true(env, &p))
        }
        RuleKind::Pattern { pattern, .. } => match pattern.matches_pattern(env, arguments)? {
            Some(b) => Ok(b),
            None => Err(YacasError::InvalidArg),
        },
    }
}

/// Rebuild the call chain: copy the head, append the evaluated arguments,
/// wrap in a sublist.
pub(crate) fn rebuild_call(call: &Rc<LispObject>, arguments: &[Rc<LispObject>]) -> Rc<LispObject> {
    let mut chain: Option<Rc<LispObject>> = None;
    for a in arguments.iter().rev() {
        let mut node = copy_node(a); // exclusive copy: safe to link
        if let Some(m) = Rc::get_mut(&mut node) {
            m.next = chain;
        }
        chain = Some(node);
    }
    let mut head = copy_node(call);
    if let Some(m) = Rc::get_mut(&mut head) {
        m.next = chain;
    }
    Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(head),
    })
}

/// Macro function: all parameters held + unfenced at construction; a
/// matched body is `@`-substituted before evaluation.
pub struct MacroUserFunction {
    pub branching: BranchingUserFunction,
}

impl MacroUserFunction {
    pub fn new(param_list: Option<&Rc<LispObject>>) -> Result<Self, YacasError> {
        let branching = BranchingUserFunction::new(param_list)?;
        {
            let mut inner = branching.inner.borrow_mut();
            for p in &mut inner.parameters {
                p.hold = true;
            }
            inner.fenced = false;
        }
        Ok(MacroUserFunction { branching })
    }
}

impl UserFunction for MacroUserFunction {
    fn arity(&self) -> usize {
        self.branching.arity()
    }
    fn is_arity(&self, n: usize) -> bool {
        self.branching.is_arity(n)
    }
    fn hold_argument(&self, variable: &str) {
        self.branching.hold_argument(variable);
    }
    fn declare_rule(
        &self,
        precedence: i32,
        predicate: Option<&Rc<LispObject>>,
        body: &Rc<LispObject>,
    ) -> Result<(), YacasError> {
        self.branching.declare_rule(precedence, predicate, body)
    }
    fn declare_pattern(
        &self,
        precedence: i32,
        pattern: Rc<dyn crate::value::GenericClass>,
        body: &Rc<LispObject>,
    ) -> Result<(), YacasError> {
        self.branching.declare_pattern(precedence, pattern, body)
    }
    fn arg_list(&self) -> Option<Rc<LispObject>> {
        self.branching.arg_list()
    }
    fn fenced(&self) -> bool {
        self.branching.fenced()
    }
    fn un_fence(&self) {
        self.branching.un_fence();
    }
    fn set_traced(&self, on: bool) {
        self.branching.set_traced(on);
    }
    fn is_traced(&self) -> bool {
        self.branching.is_traced()
    }
    fn evaluate(&self, env: &mut Environment, call: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
        self.branching.evaluate_common(env, call, true)
    }
}

/// Shared preparation for Listed variants: build
/// `[head, args 1..arity-1] + tail`, where the tail is the last argument
/// as-is when there are no surplus arguments, or `{List, surplus…}`
/// wrapping the entire remaining chain otherwise.
pub fn listed_prepare(
    env: &mut Environment,
    call: &Rc<LispObject>,
    listed_arity: usize,
) -> Result<Rc<LispObject>, YacasError> {
    let mut items: Vec<Rc<LispObject>> = Vec::new();
    items.push(copy_node(call)); // head
    let mut cur = call.next.as_ref();
    let mut i = 1usize; // head already copied
    while i < listed_arity {
        let node = cur.ok_or(YacasError::WrongNumberOfArgs)?;
        items.push(copy_node(node));
        cur = node.next.as_ref();
        i += 1;
    }
    match cur {
        None => build_chain(&items).ok_or(YacasError::generic("listed: empty chain")),
        Some(node) => {
            if node.next.is_none() {
                // Exactly at the end of the chain: close with this argument.
                items.push(copy_node(node));
                build_chain(&items).ok_or(YacasError::generic("listed: empty chain"))
            } else {
                // Surplus arguments: wrap the *whole remaining chain* (from
                // `node` on) into {List, …}. Copying only the single node
                // would drop the rest.
                let mut rest = copy_node(node);
                if let Some(m) = Rc::get_mut(&mut rest) {
                    m.next = node.next.clone();
                }
                let list_sym = env.symtab.look_up("List");
                let mut head = LispObject::atom(list_sym);
                if let Some(m) = Rc::get_mut(&mut head) {
                    m.next = Some(rest);
                }
                items.push(Rc::new(LispObject {
                    next: None,
                    kind: ObjectKind::Sublist(head),
                }));
                build_chain(&items).ok_or(YacasError::generic("listed: empty chain"))
            }
        }
    }
}

fn build_chain(items: &[Rc<LispObject>]) -> Option<Rc<LispObject>> {
    let mut chain: Option<Rc<LispObject>> = None;
    for a in items.iter().rev() {
        let mut node = copy_node(a);
        if let Some(m) = Rc::get_mut(&mut node) {
            m.next = chain;
        }
        chain = Some(node);
    }
    chain
}

/// Listed branching function.
pub struct ListedBranchingUserFunction {
    pub branching: BranchingUserFunction,
}

impl ListedBranchingUserFunction {
    pub fn new(param_list: Option<&Rc<LispObject>>) -> Result<Self, YacasError> {
        Ok(ListedBranchingUserFunction { branching: BranchingUserFunction::new(param_list)? })
    }
}

impl UserFunction for ListedBranchingUserFunction {
    fn arity(&self) -> usize {
        self.branching.arity()
    }
    fn is_arity(&self, n: usize) -> bool {
        self.arity() <= n
    }
    fn hold_argument(&self, variable: &str) {
        self.branching.hold_argument(variable);
    }
    fn declare_rule(
        &self,
        precedence: i32,
        predicate: Option<&Rc<LispObject>>,
        body: &Rc<LispObject>,
    ) -> Result<(), YacasError> {
        self.branching.declare_rule(precedence, predicate, body)
    }
    fn declare_pattern(
        &self,
        precedence: i32,
        pattern: Rc<dyn crate::value::GenericClass>,
        body: &Rc<LispObject>,
    ) -> Result<(), YacasError> {
        self.branching.declare_pattern(precedence, pattern, body)
    }
    fn arg_list(&self) -> Option<Rc<LispObject>> {
        self.branching.arg_list()
    }
    fn fenced(&self) -> bool {
        self.branching.fenced()
    }
    fn un_fence(&self) {
        self.branching.un_fence();
    }
    fn set_traced(&self, on: bool) {
        self.branching.set_traced(on);
    }
    fn is_traced(&self) -> bool {
        self.branching.is_traced()
    }
    fn evaluate(&self, env: &mut Environment, call: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
        let call2 = listed_prepare(env, call, self.arity())?;
        self.branching.evaluate_common(env, &call2, false)
    }
}

/// Listed macro function.
pub struct ListedMacroUserFunction {
    pub macro_fn: MacroUserFunction,
}

impl ListedMacroUserFunction {
    pub fn new(param_list: Option<&Rc<LispObject>>) -> Result<Self, YacasError> {
        Ok(ListedMacroUserFunction { macro_fn: MacroUserFunction::new(param_list)? })
    }
}

impl UserFunction for ListedMacroUserFunction {
    fn arity(&self) -> usize {
        self.macro_fn.arity()
    }
    fn is_arity(&self, n: usize) -> bool {
        self.arity() <= n
    }
    fn hold_argument(&self, variable: &str) {
        self.macro_fn.hold_argument(variable);
    }
    fn declare_rule(
        &self,
        precedence: i32,
        predicate: Option<&Rc<LispObject>>,
        body: &Rc<LispObject>,
    ) -> Result<(), YacasError> {
        self.macro_fn.declare_rule(precedence, predicate, body)
    }
    fn declare_pattern(
        &self,
        precedence: i32,
        pattern: Rc<dyn crate::value::GenericClass>,
        body: &Rc<LispObject>,
    ) -> Result<(), YacasError> {
        self.macro_fn.declare_pattern(precedence, pattern, body)
    }
    fn arg_list(&self) -> Option<Rc<LispObject>> {
        self.macro_fn.arg_list()
    }
    fn fenced(&self) -> bool {
        self.macro_fn.fenced()
    }
    fn un_fence(&self) {
        self.macro_fn.un_fence();
    }
    fn set_traced(&self, on: bool) {
        self.macro_fn.set_traced(on);
    }
    fn is_traced(&self) -> bool {
        self.macro_fn.is_traced()
    }
    fn evaluate(&self, env: &mut Environment, call: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
        let call2 = listed_prepare(env, call, self.arity())?;
        self.macro_fn.evaluate(env, &call2)
    }
}

/// Multi-arity dispatcher: finds the function variant matching the call's
/// arity; holds apply to every variant; `file_to_open` drives `.def`
/// lazy loading.
#[derive(Clone)]
pub struct MultiUserFunction {
    pub inner: RefCell<MultiUserFunctionInner>,
}

#[derive(Clone)]
pub struct MultiUserFunctionInner {
    pub functions: Vec<Rc<dyn UserFunction>>,
    pub file_to_open: Option<crate::loader::DefFile>,
}

impl MultiUserFunction {
    pub fn new() -> Self {
        MultiUserFunction {
            inner: RefCell::new(MultiUserFunctionInner {
                functions: Vec::new(),
                file_to_open: None,
            }),
        }
    }

    pub fn user_func(&self, arity: usize) -> Option<Rc<dyn UserFunction>> {
        let inner = self.inner.borrow();
        inner.functions.iter().find(|f| f.is_arity(arity)).cloned()
    }

    pub fn hold_argument(&self, variable: &str) {
        let inner = self.inner.borrow();
        for f in &inner.functions {
            f.hold_argument(variable);
        }
    }

    pub fn define_rule_base(&self, new_func: Rc<dyn UserFunction>) -> Result<(), YacasError> {
        let mut inner = self.inner.borrow_mut();
        let new_arity = new_func.arity();
        for f in &inner.functions {
            if f.is_arity(new_arity) {
                return Err(YacasError::ArityAlreadyDefined);
            }
        }
        inner.functions.push(new_func);
        Ok(())
    }

    pub fn delete_base(&self, arity: usize) {
        let mut inner = self.inner.borrow_mut();
        if let Some(pos) = inner.functions.iter().position(|f| f.is_arity(arity)) {
            inner.functions.remove(pos);
        }
    }
}

impl Default for MultiUserFunction {
    fn default() -> Self {
        Self::new()
    }
}

/// `TrEnter` trace line: indentation = eval_depth × 2 spaces, format
/// `TrEnter("<head>","<call>","",0);` (file empty, line 0). Written to the
/// current output buffer (same channel as `Write`).
pub fn trace_show_enter(env: &mut Environment, call: &Rc<LispObject>) {
    let indent = "  ".repeat(env.eval_depth.min(4096) as usize);
    let func = call.atom_string().map(|s| s.to_string()).unwrap_or_default();
    let wrapped = Rc::new(LispObject {
        next: None,
        kind: crate::value::ObjectKind::Sublist(call.clone()),
    });
    let expr = trace_escape(&crate::printer::infix_print(env, &wrapped));
    write_trace_line(env, &format!("{indent}TrEnter(\"{func}\",\"{expr}\",\"\",0);\n"));
}

/// `TrLeave` trace line: `TrLeave("<call>","<result>");`.
pub fn trace_show_leave(env: &mut Environment, call: &Rc<LispObject>, result: &Rc<LispObject>) {
    let indent = "  ".repeat(env.eval_depth.min(4096) as usize);
    let wrapped = Rc::new(LispObject {
        next: None,
        kind: crate::value::ObjectKind::Sublist(call.clone()),
    });
    let expr = trace_escape(&crate::printer::infix_print(env, &wrapped));
    let res = trace_escape(&crate::printer::infix_print(env, result));
    write_trace_line(env, &format!("{indent}TrLeave(\"{expr}\",\"{res}\");\n"));
}

/// `TrArg` trace line (one per argument, after evaluation):
/// `TrArg("<raw>","<evaluated>");`.
pub fn trace_show_arg(env: &mut Environment, raw: &Rc<LispObject>, value: &Rc<LispObject>) {
    let indent = "  ".repeat(env.eval_depth.min(4094) as usize + 2);
    let p = trace_escape(&crate::printer::infix_print(env, raw));
    let v = trace_escape(&crate::printer::infix_print(env, value));
    write_trace_line(env, &format!("{indent}TrArg(\"{p}\",\"{v}\");\n"));
}

/// Escape `"` (when not already preceded by a backslash), matching the
/// upstream `ShowExpression` quoting.
fn trace_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    let mut prev_bs = false;
    for c in s.chars() {
        if c == '"' && !prev_bs {
            out.push('\\');
        }
        out.push(c);
        prev_bs = c == '\\';
    }
    out
}

/// Write one line to the current output buffer (pushing a default buffer if
/// the stack is empty).
fn write_trace_line(env: &mut Environment, line: &str) {
    let mut output = env.output_stack.borrow_mut();
    if output.is_empty() {
        output.push(crate::env::OutputBuffer::default());
    }
    let buf = output.last_mut().expect("output");
    buf.text.push_str(line);
}
