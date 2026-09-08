//! Evaluation environment: symbol table, operator tables (used by the
//! parser), global variables, user functions, the local-frame chain,
//! precision, evaluation depth, True/False atoms, the protection set, and
//! the secure flag. See upstream `cyacas/libyacas/include/yacas/lispenvironment.h`.
//!
//! Local-frame semantics: a fenced frame starts its own chain (`first` =
//! None); a non-fenced frame's lookup falls through to outer frames.
//! `new_local` prepends to the current frame only, so parent frames see
//! their own variables and child frames shadow them. Frames are `Box`-owned
//! chains; variable nodes are `Rc`-shared with `RefCell` slots so
//! `set_variable` can write back in place.
//!
//! Lookup stops at the first fenced frame (`find_local`): a fenced frame is
//! a function-call argument frame and thus a shadowing boundary — a
//! function body must not see the caller's locals. Non-fenced frames (e.g.
//! `Local` blocks) are transparent to the search.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::operators::OperatorTable;
use crate::symtab::SymbolTable;
use crate::value::{atom_or_number, LispObject};

fn local_symbol_id(name: &str) -> Option<u32> {
    let local = name.strip_prefix('$')?;
    let digit_start = local
        .rfind(|character: char| !character.is_ascii_digit())
        .map_or(0, |index| index + 1);
    (digit_start < local.len())
        .then(|| local[digit_start..].parse().ok())
        .flatten()
}

/// A global variable slot with a lazy-evaluation flag.
pub struct GlobalVariable {
    pub value: Option<Rc<LispObject>>,
    pub eval_before_return: bool,
}

/// A node in a local-frame variable chain; the value slot is writable in place.
pub struct LocalVarNode {
    pub next: Option<Rc<LocalVarNode>>,
    pub variable: Rc<str>,
    pub value: RefCell<Option<Rc<LispObject>>>,
}

/// A local frame: head/tail of its variable chain, link to the outer frame,
/// and the fenced (shadowing-boundary) flag.
pub struct LocalFrame {
    pub next: Option<Box<LocalFrame>>,
    pub first: Option<Rc<LocalVarNode>>,
    pub last: Option<Rc<LocalVarNode>>,
    pub fenced: bool,
}

/// The evaluation environment.
#[derive(Default)]
pub struct Environment {
    /// Facts about symbolic variables used by assumption-aware predicates.
    pub(crate) assumptions: crate::assumptions::AssumptionContext,
    assumption_stack: Vec<crate::assumptions::AssumptionContext>,
    pub symtab: SymbolTable,
    pub prefix: OperatorTable,
    pub infix: OperatorTable,
    pub postfix: OperatorTable,
    pub bodied: OperatorTable,

    pub globals: HashMap<Rc<str>, GlobalVariable>,
    pub user_functions: HashMap<Rc<str>, crate::userfunc::MultiUserFunction>,
    pub locals: Option<Box<LocalFrame>>,

    pub precision: u32,
    pub eval_depth: u32,
    pub max_eval_depth: u32,
    pub last_unique_id: u32,

    /// 求值截止时间(None = 不限时)。由求值循环按操作数采样检查
    /// (见 evaluator::eval),超时返回 UserInterrupt;病态输入不再挂死。
    pub eval_deadline: Option<std::time::Instant>,
    /// 求值操作计数(采样触发器)。
    pub eval_ops: u64,

    pub true_atom: Option<Rc<LispObject>>,
    pub false_atom: Option<Rc<LispObject>>,

    pub protected: HashSet<Rc<str>>,
    pub secure: bool,

    pub core_commands: HashMap<Rc<str>, crate::evaluator::CoreCommand>,

    pub def_files: crate::loader::DefFiles,

    /// Input directory search chain. `internal_load` tries the bare name
    /// (CWD) first, then the directories in insertion order; see upstream
    /// `cyacas/libyacas/src/stdfileio.cpp` `InternalFindFile`. The
    /// `DefaultDirectory` command appends here.
    pub input_directories: Vec<String>,

    /// Error output buffer: errors captured by `TrapError` are staged here
    /// and read by `GetCoreError`.
    pub error_output: std::cell::RefCell<String>,

    /// Output stream stack. The top entry is the current output target plus
    /// the last character of the previous token (drives the printer's
    /// spacing rules for `Write`/`WriteString`). `ToString`/`ToFile`/
    /// `ToStdout` push a fresh buffer to capture body output and pop it when
    /// done.
    pub output_stack: std::cell::RefCell<Vec<OutputBuffer>>,

    /// Input stream stack; the top entry is the current tokenizer.
    /// `FromString`/`FromFile` push, `Read`/`ReadToken` consume from the top
    /// and pop when the body ends. An empty stack means no active input
    /// (`Read` errors). The `Option` lets `Read` take the tokenizer out to
    /// run the parser and put it back afterwards.
    pub input_stack: std::cell::RefCell<Vec<Option<crate::tokenizer::Tokenizer>>>,

    /// Debugger state for `CustomEval` (enter/leave callbacks, stop flag,
    /// TopExpr/TopResult). `None` outside custom evaluation.
    pub debugger: std::cell::RefCell<Option<DebuggerState>>,

    /// Registered PrettyReader name (stored quoted); `None` = unset.
    pub pretty_reader: Option<String>,
    /// Registered PrettyPrinter name (stored quoted); `None` = unset.
    pub pretty_printer: Option<String>,

    /// Current input file name for status reports: the file name while
    /// loading, `"String"` for `FromString`/`PatchLoad`, `"CommandLine"`
    /// otherwise. Read by `CurrentFile`; restored after loading finishes.
    pub input_file: std::cell::RefCell<String>,

    /// Current tokenizer mode: `true` = XML tokenizer, `false` = default.
    /// Toggled by `XmlTokenizer()`/`DefaultTokenizer()`; new tokenizers
    /// (loading/`FromString`) initialize from this flag.
    pub xml_tokenizer: std::cell::Cell<bool>,
}

/// Debugger state for `CustomEval(enter, leave, error, expr)`: `eval` runs
/// the Enter/Leave callbacks around each sub-expression; callback evaluation
/// itself disables the hooks.
pub struct DebuggerState {
    /// Enter callback (before each sub-expression).
    pub enter_cb: Rc<LispObject>,
    /// Leave callback (after each successful sub-expression).
    pub leave_cb: Rc<LispObject>,
    /// Error/stop callback (invoked on error re-entry).
    pub error_cb: Rc<LispObject>,
    /// Set by `CustomEval'Stop` → subsequent evals abort with an error.
    pub stopped: bool,
    /// Current sub-expression (stored on Enter and on Leave).
    pub top_expr: Option<Rc<LispObject>>,
    /// Evaluation result of the current sub-expression (stored on Leave).
    pub top_result: Option<Rc<LispObject>>,
    /// `false` while a callback is being evaluated (hooks disabled).
    pub in_callback: bool,
}

/// Current output buffer: accumulated text, the previous token's last
/// character (spacing rule), and an optional file to write the text to on
/// pop (truncating, `ToFile` semantics).
#[derive(Default)]
pub struct OutputBuffer {
    pub text: String,
    pub prev_last_char: char,
    pub file: Option<String>,
}

impl Environment {
    /// Push a fresh in-memory output buffer; returns the previous stack depth.
    pub fn push_output(&self) -> usize {
        self.push_output_to(None)
    }

    /// Push an output buffer, optionally targeting a file (`ToFile`).
    pub fn push_output_to(&self, file: Option<String>) -> usize {
        let mut s = self.output_stack.borrow_mut();
        s.push(OutputBuffer {
            text: String::new(),
            prev_last_char: '\0',
            file,
        });
        s.len() - 1
    }

    /// Pop back to `depth`, returning the popped buffer; a file-backed
    /// buffer is written to its path (truncating).
    pub fn pop_output(&self, depth: usize) -> OutputBuffer {
        let mut s = self.output_stack.borrow_mut();
        while s.len() - 1 > depth {
            s.pop();
        }
        let buf = s.pop().unwrap_or_default();
        if let Some(path) = &buf.file {
            if let Err(e) = std::fs::write(path, &buf.text) {
                eprintln!("[ToFile] failed to write {path}: {e}");
            }
        }
        buf
    }
}

impl Environment {
    /// Startup environment: operator + core-command registration, True/False
    /// atoms, protection of `List`/`Prog`/`Infinity`/`Undefined`, and the
    /// initial fenced frame.
    pub fn new() -> Self {
        let mut e = Environment {
            precision: 10,
            max_eval_depth: 600,
            ..Default::default()
        };
        crate::operators::register_stdops(&mut e);
        crate::commands::register_core_commands(&mut e);
        let t = atom_or_number(&mut e.symtab, "True");
        let f = atom_or_number(&mut e.symtab, "False");
        e.true_atom = Some(t);
        e.false_atom = Some(f);
        for s in ["List", "Prog", "Infinity", "Undefined"] {
            e.protect(s);
        }
        e.push_local_frame(true);
        e
    }

    /// 设置求值超时(从现在起 `dur`);再次调用刷新,传 None 清除。
    pub fn set_eval_timeout(&mut self, dur: Option<std::time::Duration>) {
        self.eval_deadline = dur.map(|d| std::time::Instant::now() + d);
        self.eval_ops = 0;
    }

    /// Check the current evaluation deadline from long-running core loops.
    pub(crate) fn check_eval_deadline(&self) -> Result<(), crate::errors::YacasError> {
        if self
            .eval_deadline
            .is_some_and(|deadline| std::time::Instant::now() >= deadline)
        {
            Err(crate::errors::YacasError::UserInterrupt)
        } else {
            Ok(())
        }
    }

    pub fn assume(
        &mut self,
        symbol: &str,
        fact: crate::assumptions::Assumption,
    ) -> Result<(), crate::assumptions::AssumptionError> {
        self.assumptions.assume(symbol, fact)
    }

    pub fn is_assumed(&self, symbol: &str, fact: crate::assumptions::Assumption) -> bool {
        self.assumptions.is_assumed(symbol, fact)
    }

    pub fn clear_assumptions(&mut self) {
        self.assumptions.clear();
    }

    pub fn push_assumptions(&mut self) {
        self.assumption_stack.push(self.assumptions.clone());
    }

    pub fn pop_assumptions(&mut self) -> bool {
        let Some(previous) = self.assumption_stack.pop() else {
            return false;
        };
        self.assumptions = previous;
        true
    }

    pub fn precision(&self) -> u32 {
        self.precision
    }

    pub fn set_precision(&mut self, p: u32) {
        self.precision = p;
    }

    pub fn true_atom(&self) -> Rc<LispObject> {
        self.true_atom
            .clone()
            .expect("env: True atom not initialized")
    }

    pub fn false_atom(&self) -> Rc<LispObject> {
        self.false_atom
            .clone()
            .expect("env: False atom not initialized")
    }

    /// Search local frames from innermost outward, scanning each frame's
    /// variable chain, and **stop at the first fenced frame** (a function
    /// argument frame is a shadowing boundary).
    pub fn find_local(&self, name: &str) -> Option<&LocalVarNode> {
        let mut frame = self.locals.as_ref();
        while let Some(f) = frame {
            let mut t = f.first.as_ref();
            while let Some(node) = t {
                if node.variable.as_ref() == name {
                    return Some(node);
                }
                t = node.next.as_ref();
            }
            if f.fenced {
                break; // shadowing boundary: do not look further outward
            }
            frame = f.next.as_ref();
        }
        None
    }

    /// Whether the name is bound in any local frame or the global table.
    pub fn is_bound(&self, name: &str) -> bool {
        if self.find_local(name).is_some() {
            return true;
        }
        self.globals.contains_key(name)
    }

    /// Set a variable: a local hit writes the local slot; otherwise the
    /// protection check runs and the global table is written.
    pub fn set_variable(
        &mut self,
        name: Rc<str>,
        value: &Rc<LispObject>,
        lazy: bool,
    ) -> Result<(), crate::errors::YacasError> {
        if let Some(local) = self.find_local(&name) {
            *local.value.borrow_mut() = Some(value.clone());
            return Ok(());
        }
        if self.is_protected(&name) {
            return Err(crate::errors::YacasError::SymbolProtected);
        }
        let g = GlobalVariable {
            value: Some(value.clone()),
            eval_before_return: lazy,
        };
        self.globals.insert(name, g);
        Ok(())
    }

    /// Get a variable: a local hit returns the stored value as-is; a global
    /// hit with the lazy flag evaluates once and caches.
    ///
    /// Local and non-lazy global reads deliberately do **not** re-evaluate:
    /// `w := Hold(f(aa)); w;` must yield `f(aa)` unevaluated, and reading a
    /// rule's pattern parameters must not evaluate pattern left-hand sides.
    /// (A macro parameter's read-evaluates behavior comes from the macro
    /// body's `@` substitution, not from variable reads.)
    pub fn get_variable(
        &mut self,
        name: &str,
    ) -> Result<Option<Rc<LispObject>>, crate::errors::YacasError> {
        if let Some(local) = self.find_local(name) {
            return Ok(local.value.borrow().clone());
        }
        if let Some(g) = self.globals.get(name) {
            if g.eval_before_return {
                let value = g.value.clone().expect("global value");
                let evaled = crate::evaluator::eval(self, &value)?;
                // Cache the evaluated result and clear the lazy flag.
                let g = self.globals.get_mut(name).expect("global value");
                g.value = Some(evaled.clone());
                g.eval_before_return = false;
                return Ok(Some(evaled));
            }
            return Ok(g.value.clone());
        }
        Ok(None)
    }

    /// Unset a variable: a local hit empties the slot; globals are removed
    /// (after the protection check).
    pub fn unset_variable(&mut self, name: Rc<str>) -> Result<(), crate::errors::YacasError> {
        if let Some(local) = self.find_local(&name) {
            *local.value.borrow_mut() = None;
            return Ok(());
        }
        if self.is_protected(&name) {
            return Err(crate::errors::YacasError::SymbolProtected);
        }
        self.globals.remove(&name);
        Ok(())
    }

    /// Create a local variable in the current frame (prepend).
    pub fn new_local(&mut self, name: Rc<str>, value: Option<Rc<LispObject>>) {
        let frame = self.locals.as_mut().expect("new_local: no frame");
        let node = Rc::new(LocalVarNode {
            next: frame.first.take(),
            variable: name,
            value: RefCell::new(value),
        });
        frame.first = Some(node);
    }

    /// Push a frame; `fenced` marks it as a shadowing boundary.
    pub fn push_local_frame(&mut self, fenced: bool) {
        let parent = self.locals.take();
        self.locals = Some(Box::new(LocalFrame {
            next: parent,
            first: None,
            last: None,
            fenced,
        }));
    }

    /// Pop the current frame.
    pub fn pop_local_frame(&mut self) -> Result<(), crate::errors::YacasError> {
        let frame = self
            .locals
            .take()
            .ok_or(crate::errors::YacasError::InvalidStack)?;
        self.locals = frame.next;
        Ok(())
    }

    pub fn protect(&mut self, name: &str) {
        let sym = self.symtab.look_up(name);
        self.protected.insert(sym);
    }

    pub fn unprotect(&mut self, name: &str) {
        let sym = self.symtab.look_up(name);
        self.protected.remove(&sym);
    }

    pub fn is_protected(&self, name: &Rc<str>) -> bool {
        self.protected.contains(name)
    }

    /// Broadcast a destructive list update to every variable slot aliasing
    /// the old value. The rebuild-only value model lacks upstream's shared
    /// in-place chain mutation, so destructive commands call this to make
    /// the update visible through all aliases.
    ///
    /// Aliasing is two-level: (1) the slot value is the same `Rc` node, or
    /// (2) the slot is a sublist whose *content chain head* is the same `Rc`
    /// (shallow copies share contents). Nested containment counts too: a
    /// slot value that merely *contains* the aliased list gets rebuilt with
    /// the sub-list replaced (`l := {{1,5}}; it := l[1]; it[2] := -7` must
    /// update `l`).
    ///
    /// Within one broadcast, rebuilds are memoized by the source content
    /// chain's `Rc` pointer, so all slots holding the same chain receive the
    /// same rebuilt node — chain identity is preserved, which later
    /// identity-based broadcasts rely on.
    pub fn propagate_alias(&mut self, old: &Rc<LispObject>, new_val: &Rc<LispObject>) {
        let old_chain = match &old.kind {
            crate::value::ObjectKind::Sublist(first) => Some(first.clone()),
            _ => None,
        };
        let is_alias = |v: &Rc<LispObject>| -> bool {
            if std::rc::Rc::ptr_eq(v, old) {
                return true;
            }
            if let Some(h) = &old_chain {
                if let crate::value::ObjectKind::Sublist(vf) = &v.kind {
                    if std::rc::Rc::ptr_eq(vf, h) {
                        return true;
                    }
                }
            }
            false
        };
        // Memo key = the node's content-chain Rc pointer; clones sharing one
        // chain share one rebuilt result.
        let memo: std::cell::RefCell<std::collections::HashMap<usize, Option<Rc<LispObject>>>> =
            std::cell::RefCell::new(std::collections::HashMap::new());
        fn deep_replace(
            v: &Rc<LispObject>,
            old: &Rc<LispObject>,
            old_chain: &Option<Rc<LispObject>>,
            new_val: &Rc<LispObject>,
            memo: &std::cell::RefCell<std::collections::HashMap<usize, Option<Rc<LispObject>>>>,
        ) -> Option<Rc<LispObject>> {
            let is_alias = |v: &Rc<LispObject>| -> bool {
                if std::rc::Rc::ptr_eq(v, old) {
                    return true;
                }
                if let Some(h) = old_chain {
                    if let crate::value::ObjectKind::Sublist(vf) = &v.kind {
                        if std::rc::Rc::ptr_eq(vf, h) {
                            return true;
                        }
                    }
                }
                false
            };
            if is_alias(v) {
                return None; // direct hit: replaced by the caller
            }
            let memo_key = v.sublist().map(|c| Rc::as_ptr(c) as usize);
            if let Some(k) = memo_key {
                if let Some(hit) = memo.borrow().get(&k) {
                    return hit.clone();
                }
            }
            let sub = v.sublist()?;
            let mut kinds: Vec<crate::value::ObjectKind> = Vec::new();
            let mut changed = false;
            for (i, n) in crate::value::spine_refs(sub).enumerate() {
                if i == 0 {
                    kinds.push(crate::value::clone_kind(&n.kind));
                    continue;
                }
                if is_alias(n) {
                    kinds.push(crate::value::clone_kind(&new_val.kind));
                    changed = true;
                } else if let Some(r) = deep_replace(n, old, old_chain, new_val, memo) {
                    kinds.push(crate::value::clone_kind(&r.kind));
                    changed = true;
                } else {
                    kinds.push(crate::value::clone_kind(&n.kind));
                }
            }
            if !changed {
                if let Some(k) = memo_key {
                    memo.borrow_mut().insert(k, None);
                }
                return None;
            }
            let chain = crate::value::build_list(kinds)?;
            let r = Some(Rc::new(crate::value::LispObject {
                next: None,
                kind: crate::value::ObjectKind::Sublist(chain),
            }));
            if let Some(k) = memo_key {
                memo.borrow_mut().insert(k, r.clone());
            }
            r
        }
        let mut frame = self.locals.as_ref();
        while let Some(f) = frame {
            let mut t = f.first.as_ref();
            while let Some(node) = t {
                let action = match node.value.borrow().as_ref() {
                    Some(v) if is_alias(v) => 1,
                    Some(v) => deep_replace(v, old, &old_chain, new_val, &memo).map_or(0, |_| 2),
                    None => 0,
                };
                match action {
                    1 => *node.value.borrow_mut() = Some(new_val.clone()),
                    2 => {
                        let cur = node.value.borrow().clone().expect("v");
                        if let Some(r) = deep_replace(&cur, old, &old_chain, new_val, &memo) {
                            *node.value.borrow_mut() = Some(r);
                        }
                    }
                    _ => {}
                }
                t = node.next.as_ref();
            }
            frame = f.next.as_ref();
        }
        for g in self.globals.values_mut() {
            if let Some(v) = g.value.as_ref() {
                if is_alias(v) {
                    g.value = Some(new_val.clone());
                } else if let Some(r) = deep_replace(v, old, &old_chain, new_val, &memo) {
                    g.value = Some(r);
                }
            }
        }
    }

    /// Unique id (used by `LocalSymbols` renaming).
    pub fn gen_unique_id(&mut self) -> u32 {
        self.last_unique_id += 1;
        self.last_unique_id
    }

    /// Remove runtime `LocalSymbols` slots created after a host request began.
    /// Script loading stays outside this lifecycle because definitions may
    /// retain generated bindings.
    pub fn clear_unique_globals_since(&mut self, first_id: u32) {
        self.globals
            .retain(|name, _| local_symbol_id(name).is_none_or(|id| id <= first_id));
        self.symtab.garbage_collect();
    }

    /// Look up a user function by name and arity (`None` if absent).
    pub fn user_func(
        &self,
        name: &Rc<str>,
        arity: usize,
    ) -> Option<Rc<dyn crate::userfunc::UserFunction>> {
        self.user_functions
            .get(name)
            .and_then(|m| m.user_func(arity))
    }

    /// `DeclareRuleBase`: register a plain (non-macro) branching function;
    /// `listed` selects the `Listed` variant.
    pub fn declare_rule_base(
        &mut self,
        name: Rc<str>,
        params: Option<&Rc<LispObject>>,
        listed: bool,
    ) -> Result<(), crate::errors::YacasError> {
        if self.is_protected(&name) {
            return Err(crate::errors::YacasError::SymbolProtected);
        }
        let new_func: Rc<dyn crate::userfunc::UserFunction> = if listed {
            Rc::new(crate::userfunc::ListedBranchingUserFunction::new(params)?)
        } else {
            Rc::new(crate::userfunc::BranchingUserFunction::new(params)?)
        };
        let entry = self.user_functions.entry(name).or_default();
        entry.define_rule_base(new_func)
    }

    /// `DeclareMacroRuleBase`: register a macro function; `listed` selects
    /// the `Listed` variant.
    pub fn declare_macro_rule_base(
        &mut self,
        name: Rc<str>,
        params: Option<&Rc<LispObject>>,
        listed: bool,
    ) -> Result<(), crate::errors::YacasError> {
        if self.is_protected(&name) {
            return Err(crate::errors::YacasError::SymbolProtected);
        }
        let new_func: Rc<dyn crate::userfunc::UserFunction> = if listed {
            Rc::new(crate::userfunc::ListedMacroUserFunction::new(params)?)
        } else {
            Rc::new(crate::userfunc::MacroUserFunction::new(params)?)
        };
        let entry = self.user_functions.entry(name).or_default();
        entry.define_rule_base(new_func)
    }

    /// `DefineRule`: a `True` predicate becomes an unconditional rule,
    /// otherwise a predicate rule.
    pub fn define_rule(
        &mut self,
        name: Rc<str>,
        arity: usize,
        precedence: i32,
        predicate: &Rc<LispObject>,
        body: &Rc<LispObject>,
    ) -> Result<(), crate::errors::YacasError> {
        if self.is_protected(&name) {
            return Err(crate::errors::YacasError::SymbolProtected);
        }
        let entry = self
            .user_functions
            .get(&name)
            .ok_or(crate::errors::YacasError::CreatingRule)?;
        let f = entry
            .user_func(arity)
            .ok_or(crate::errors::YacasError::CreatingRule)?;
        if crate::standard::is_true(self, predicate) {
            f.declare_rule(precedence, None, body)
        } else {
            f.declare_rule(precedence, Some(predicate), body)
        }
    }

    /// `DefineRulePattern`: the predicate must be a Generic pattern object;
    /// no protection check (matching upstream).
    pub fn define_rule_pattern(
        &mut self,
        name: Rc<str>,
        arity: usize,
        precedence: i32,
        pattern: &Rc<LispObject>,
        body: &Rc<LispObject>,
    ) -> Result<(), crate::errors::YacasError> {
        let entry = self
            .user_functions
            .get(&name)
            .ok_or(crate::errors::YacasError::CreatingRule)?;
        let f = entry
            .user_func(arity)
            .ok_or(crate::errors::YacasError::CreatingRule)?;
        let g = match &pattern.kind {
            crate::value::ObjectKind::Generic(g) => g.clone(),
            _ => return Err(crate::errors::YacasError::InvalidArg),
        };
        f.declare_pattern(precedence, g, body)
    }

    /// `HoldArgument`: add a variable name to the function's hold list.
    pub fn hold_argument(
        &mut self,
        name: Rc<str>,
        variable: &str,
    ) -> Result<(), crate::errors::YacasError> {
        let entry = self
            .user_functions
            .get(&name)
            .ok_or(crate::errors::YacasError::InvalidArg)?;
        entry.hold_argument(variable);
        Ok(())
    }

    /// `UnFenceRule` (protection check; the function must exist).
    pub fn un_fence_rule(
        &mut self,
        name: Rc<str>,
        arity: usize,
    ) -> Result<(), crate::errors::YacasError> {
        if self.is_protected(&name) {
            return Err(crate::errors::YacasError::SymbolProtected);
        }
        let entry = self
            .user_functions
            .get(&name)
            .ok_or(crate::errors::YacasError::InvalidArg)?;
        let f = entry
            .user_func(arity)
            .ok_or(crate::errors::YacasError::InvalidArg)?;
        f.un_fence();
        Ok(())
    }

    /// `Retract` (protection check; deletes the rule base for the arity).
    pub fn retract(
        &mut self,
        name: Rc<str>,
        arity: usize,
    ) -> Result<(), crate::errors::YacasError> {
        if self.is_protected(&name) {
            return Err(crate::errors::YacasError::SymbolProtected);
        }
        if let Some(entry) = self.user_functions.get(&name) {
            entry.delete_base(arity);
        }
        Ok(())
    }

    /// `DefLoadFunction`: get-or-create the `MultiUserFunction`; if a `.def`
    /// file is pending and not yet loaded, clear the hook and load it.
    pub fn def_load_function(&mut self, name: Rc<str>) -> Result<(), crate::errors::YacasError> {
        let def = {
            let entry = self.user_functions.entry(name).or_default();
            let mut inner = entry.inner.borrow_mut();
            inner.file_to_open.take()
        };
        if let Some(def) = def {
            if !def.is_loaded {
                crate::standard::internal_use(self, &def.file_name)?;
            }
        }
        Ok(())
    }
}
