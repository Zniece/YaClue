//! Standard core utilities. See upstream `cyacas/libyacas/src/standard.cpp`
//! and `substitute.cpp`.

use std::rc::Rc;

use crate::env::Environment;
use crate::errors::YacasError;
use crate::value::{copy_node, spine_refs, ObjectKind, LispObject};

// Diagnostic tracing, enabled by `YACAS_TRACE_LOAD` in the environment.
thread_local! {
    static LOAD_STACK: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    static TRACE_STMTS: std::cell::RefCell<bool> = const { std::cell::RefCell::new(false) };
}

fn load_trace_enabled() -> bool {
    std::env::var_os("YACAS_TRACE_LOAD").is_some()
}

/// Enable statement-level tracing for lazy-load triggered loads (explicit
/// `Load`/`LoadPackage` are not traced at statement level).
pub fn lazy_load_trace_enter() {
    TRACE_STMTS.with(|t| *t.borrow_mut() = true);
}
pub fn lazy_load_trace_exit() {
    TRACE_STMTS.with(|t| *t.borrow_mut() = false);
}

/// Numeric literal test: optional sign, integer/fractional digits, optional
/// `e`/`E` exponent (floats allowed per `allow_float`).
pub fn is_number(s: &str, allow_float: bool) -> bool {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut pos = 0usize;
    if len == 0 {
        return false;
    }
    if bytes[pos] == b'-' || bytes[pos] == b'+' {
        pos += 1;
    }
    let mut nr_digits = 0usize;
    while pos < len && bytes[pos].is_ascii_digit() {
        nr_digits += 1;
        pos += 1;
    }
    if pos == len {
        return nr_digits > 0; // plain integer: at least one digit
    }
    if bytes[pos] == b'.' {
        if !allow_float {
            return false;
        }
        pos += 1;
        while pos < len && bytes[pos].is_ascii_digit() {
            nr_digits += 1;
            pos += 1;
        }
    }
    if nr_digits == 0 {
        return false;
    }
    if pos < len && (bytes[pos] == b'e' || bytes[pos] == b'E') {
        if !allow_float {
            return false;
        }
        pos += 1;
        if pos < len && (bytes[pos] == b'-' || bytes[pos] == b'+') {
            pos += 1;
        }
        while pos < len && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
    }
    pos == len
}

pub fn is_true(env: &Environment, expr: &Rc<LispObject>) -> bool {
    match expr.atom_string() {
        Some(s) => match env.true_atom().atom_string() {
            Some(t) => Rc::ptr_eq(s, t),
            None => false,
        },
        None => false,
    }
}

pub fn is_false(env: &Environment, expr: &Rc<LispObject>) -> bool {
    match expr.atom_string() {
        Some(s) => match env.false_atom().atom_string() {
            Some(t) => Rc::ptr_eq(s, t),
            None => false,
        },
        None => false,
    }
}

pub fn internal_boolean(env: &mut Environment, value: bool) -> Rc<LispObject> {
    if value {
        copy_node(&env.true_atom())
    } else {
        copy_node(&env.false_atom())
    }
}

pub fn internal_list_length(list: &Rc<LispObject>) -> usize {
    spine_refs(list).count()
}

/// Reverse a chain (rebuild-only; the input chain is untouched).
pub fn internal_reverse_list(list: &Rc<LispObject>) -> Rc<LispObject> {
    let kinds: Vec<ObjectKind> = crate::value::spine_kinds(list).collect();
    let mut kinds = kinds;
    kinds.reverse();
    crate::value::build_list(kinds).expect("reverse input is non-empty")
}

/// N-th element (0-based; `ListNotLongEnough` if out of range).
pub fn internal_nth(list: &Rc<LispObject>, n: usize) -> Result<Rc<LispObject>, YacasError> {
    let node = spine_refs(list).nth(n).ok_or(YacasError::ListNotLongEnough)?;
    Ok(copy_node(node))
}

/// Tail of a chain: everything but the head, wrapped in a sublist.
pub fn internal_tail(list: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let head = list;
    let rest = head.next.as_ref().ok_or(YacasError::InvalidArg)?;
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(copy_node(rest)),
    }))
}

/// Symbol name lookup: quoted strings are unquoted before interning.
pub fn symbol_name(env: &mut Environment, s: &str) -> Rc<str> {
    if s.starts_with('"') {
        env.symtab.look_up_unstringify(s)
    } else {
        env.symtab.look_up(s)
    }
}

pub fn internal_is_string(s: &str) -> bool {
    s.starts_with('"') && s.ends_with('"') && s.len() >= 2
}

/// Strip surrounding quotes.
pub fn internal_unstringify(s: &str) -> Result<&str, YacasError> {
    if internal_is_string(s) {
        Ok(&s[1..s.len() - 1])
    } else {
        Err(YacasError::InvalidArg)
    }
}

/// List test: a sublist whose first element is the `List` atom.
pub fn internal_is_list(ptr: &Rc<LispObject>) -> bool {
    match ptr.sublist() {
        Some(inner) => match inner.atom_string() {
            Some(s) => s.as_ref() == "List",
            None => false,
        },
        None => false,
    }
}

/// Deep equality: pointer-identical → true; numbers by value; strings by
/// interned pointer; sublists element-wise recursive.
pub fn internal_equals(env: &Environment, e1: &Rc<LispObject>, e2: &Rc<LispObject>) -> bool {
    if Rc::ptr_eq(e1, e2) {
        return true;
    }
    let n1 = num_of(env, e1);
    let n2 = num_of(env, e2);
    match (n1, n2) {
        (Some(a), Some(b)) => return a.equals(&b),
        (Some(_), None) | (None, Some(_)) => return false,
        (None, None) => {}
    }
    let s1 = e1.atom_string();
    let s2 = e2.atom_string();
    if s1.is_some() || s2.is_some() {
        return match (s1, s2) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            _ => false,
        };
    }
    match (e1.sublist(), e2.sublist()) {
        (Some(l1), Some(l2)) => {
            let mut i1 = spine_refs(l1);
            let mut i2 = spine_refs(l2);
            loop {
                match (i1.next(), i2.next()) {
                    (None, None) => return true,
                    (Some(a), Some(b)) => {
                        if !internal_equals(env, a, b) {
                            return false;
                        }
                    }
                    _ => return false,
                }
            }
        }
        _ => false,
    }
}

/// Strict total order: number < string < sublist < generic (generics are
/// unimplemented and compare false). Numbers compare by value, strings by
/// text, lists element-wise (equal elements skipped, first difference
/// decides; a shorter list sorts first; fully equal returns false). This is
/// the key order for `Association`'s sorted views, matching the upstream
/// `InternalStrictTotalOrder`.
pub fn total_less(env: &Environment, e1: &Rc<LispObject>, e2: &Rc<LispObject>) -> bool {
    if Rc::ptr_eq(e1, e2) {
        return false;
    }
    let n1 = num_of(env, e1);
    let n2 = num_of(env, e2);
    if n1.is_some() || n2.is_some() {
        return match (n1, n2) {
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (Some(a), Some(b)) => {
                if a.less_than(&b) {
                    true
                } else if !a.equals(&b) {
                    false
                } else {
                    // Numerically equal: compare the chain continuation.
                    total_less_tail(env, e1, e2)
                }
            }
            (None, None) => unreachable!(),
        };
    }
    let s1 = e1.atom_string();
    let s2 = e2.atom_string();
    if s1.is_some() || s2.is_some() {
        return match (s1, s2) {
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (Some(a), Some(b)) => {
                if a != b {
                    a.as_ref() < b.as_ref()
                } else {
                    total_less_tail(env, e1, e2)
                }
            }
            (None, None) => unreachable!(),
        };
    }
    let l1 = e1.sublist();
    let l2 = e2.sublist();
    if l1.is_some() || l2.is_some() {
        return match (l1, l2) {
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (Some(x1), Some(x2)) => {
                let mut i1 = spine_refs(x1);
                let mut i2 = spine_refs(x2);
                loop {
                    match (i1.next(), i2.next()) {
                        (None, None) => return false,
                        (Some(p1), Some(p2)) => {
                            if !internal_equals(env, p1, p2) {
                                return total_less(env, p1, p2);
                            }
                        }
                        (None, Some(_)) => return true,
                        (Some(_), None) => return false,
                    }
                }
            }
            (None, None) => unreachable!(),
        };
    }
    // Generic (or unknown): unimplemented upstream as well → false.
    false
}

/// Chain-tail comparison when the heads are equal-but-distinct. Atoms and
/// numbers have no continuation in this model, so this is always false.
fn total_less_tail(_env: &Environment, e1: &Rc<LispObject>, e2: &Rc<LispObject>) -> bool {
    let _ = (e1, e2);
    false
}

/// Numeric view of a node (`None` for non-numbers).
fn num_of(env: &Environment, node: &Rc<LispObject>) -> Option<crate::number::float::Float> {
    match &node.kind {
        ObjectKind::Number(n) => Some(n.float_at(env.precision())),
        _ => None,
    }
}

/// Return-unevaluated fallback: copy the call, evaluate each argument, and
/// rebuild the chain in argument order.
pub fn return_un_evaluated(env: &mut Environment, call: &Rc<LispObject>) -> Result<Rc<LispObject>, YacasError> {
    let head = copy_node(call);
    let mut chain: Option<Rc<LispObject>> = None;
    let mut cur = call.next.as_ref();
    let mut args: Vec<Rc<LispObject>> = Vec::new();
    while let Some(node) = cur {
        args.push(crate::evaluator::eval(env, node)?);
        cur = node.next.as_ref();
    }
    for a in args.into_iter().rev() {
        let mut node = copy_node(&a);
        if let Some(m) = Rc::get_mut(&mut node) {
            m.next = chain;
        }
        chain = Some(node);
    }
    let mut head = head;
    if let Some(m) = Rc::get_mut(&mut head) {
        m.next = chain;
    }
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(head),
    }))
}

/// Flat copy of a sublist's contents into a fresh chain.
pub fn internal_flat_copy(list: &Rc<LispObject>) -> Rc<LispObject> {
    let kinds: Vec<ObjectKind> = crate::value::spine_kinds(list).collect();
    crate::value::build_list(kinds).expect("flat_copy input is non-empty")
}

/// Recursive substitution: a behaviour hit replaces the element; sublists
/// recurse element-wise and rebuild; atoms are copied. The same behaviour
/// instance is passed down through the recursion.
pub fn internal_substitute(
    env: &mut Environment,
    source: &Rc<LispObject>,
    behaviour: &mut impl crate::substitute::SubstBehaviour,
) -> Result<Rc<LispObject>, YacasError> {
    if let Some(repl) = behaviour.matches(env, source)? {
        return Ok(repl);
    }
    if let Some(old_list) = source.sublist() {
        let mut items: Vec<Rc<LispObject>> = Vec::new();
        for node in spine_refs(old_list) {
            items.push(internal_substitute(env, node, behaviour)?);
        }
        let kinds: Vec<ObjectKind> = items
            .iter()
            .map(|n| crate::value::spine_kinds(n).next().expect("substitute item kind"))
            .collect();
        let inner = crate::value::build_list(kinds).expect("substitute inner");
        Ok(Rc::new(LispObject {
            next: None,
            kind: ObjectKind::Sublist(inner),
        }))
    } else {
        Ok(copy_node(source))
    }
}

/// Load a file: read the text, then parse+eval expression by expression
/// until `EndOfFile`.
pub fn do_internal_load(env: &mut Environment, text: &str) -> Result<(), YacasError> {
    let mut tok = crate::tokenizer::Tokenizer::new(text);
    // New tokenizers inherit the current XML-tokenizer mode.
    tok.xml = env.xml_tokenizer.get();
    loop {
        // The &mut env borrow exists only for the duration of parse_one, so
        // eval can run right after — definitions from earlier statements
        // affect later parsing.
        let expr = crate::parser::parse_one(env, &mut tok)?
            .expect("do_internal_load: expression is non-empty");
        let eof = env.symtab.look_up("EndOfFile");
        if matches!(expr.atom_string(), Some(s) if Rc::ptr_eq(s, &eof)) {
            return Ok(());
        }
        if load_trace_enabled() && TRACE_STMTS.with(|t| *t.borrow()) {
            let file = LOAD_STACK.with(|s| s.borrow().last().cloned());
            eprintln!("[LOAD-STMT {}] {}", file.as_deref().unwrap_or("(direct)"), crate::printer::infix_print(env, &expr));
        }
        crate::evaluator::eval(env, &expr)?;
    }
}

/// File resolution order: bare name (relative to CWD, takes priority), then
/// each input directory in insertion order; `None` if all fail. Absolute
/// paths hit the bare-name branch.
pub fn internal_find_file(env: &Environment, file_name: &str) -> Option<String> {
    if std::path::Path::new(file_name).is_file() {
        return Some(file_name.to_string());
    }
    for d in &env.input_directories {
        let candidate = format!("{d}{file_name}");
        if std::path::Path::new(&candidate).is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Load by name: unquote, resolve via `internal_find_file`, then read-parse-
/// eval the file. The input status carries the file name during the load and
/// is restored afterwards.
pub fn internal_load(env: &mut Environment, file_name: &str) -> Result<(), YacasError> {
    let path = internal_find_file(env, file_name).ok_or(YacasError::FileNotFound)?;
    let text = std::fs::read_to_string(&path).map_err(|_| YacasError::FileNotFound)?;
    let old = env.input_file.borrow().clone();
    *env.input_file.borrow_mut() = file_name.to_string();
    let r = do_internal_load(env, &text);
    *env.input_file.borrow_mut() = old;
    r
}

/// `Use` a file: get-or-create the def-file entry; if not loaded, mark it
/// loaded **before** loading, unprotect the file's registered symbols, load,
/// then re-protect.
///
/// The pre-load `is_loaded` flag is deliberate upstream behavior: a failed
/// load still counts as loaded, so later `Use`s and lazy-load triggers do
/// not retry (the lazy hook is already detached by then). The symbol set is
/// the one registered by `DefLoad` — the same entry, not a clone.
pub fn internal_use(env: &mut Environment, file_name: &str) -> Result<(), YacasError> {
    let (is_loaded, symbols) = {
        let entry = env
            .def_files
            .map
            .entry(file_name.to_string())
            .or_insert_with(|| crate::loader::DefFile::new(file_name));
        (entry.is_loaded, entry.symbols.iter().cloned().collect::<Vec<Rc<str>>>())
    };
    if !is_loaded {
        if load_trace_enabled() {
            let depth = LOAD_STACK.with(|s| s.borrow().len());
            eprintln!("[LOAD-BEGIN] {file_name} (depth {depth})");
        }
        LOAD_STACK.with(|s| s.borrow_mut().push(file_name.to_string()));
        if let Some(e) = env.def_files.map.get_mut(file_name) {
            e.is_loaded = true;
        }
        for s in &symbols {
            env.unprotect(s.as_ref());
        }
        let res = internal_load(env, file_name);
        LOAD_STACK.with(|s| s.borrow_mut().pop());
        // Loading may fail after DefLoad temporarily unprotected its symbols.
        // Keep the upstream no-retry loaded flag, but always restore this
        // environment invariant before propagating the error.
        for s in &symbols {
            env.protect(s.as_ref());
        }
        if let Err(e) = res {
            if load_trace_enabled() {
                eprintln!("[LOAD-FAIL] {file_name}: {e:?}");
            }
            return Err(e);
        }
        if load_trace_enabled() {
            eprintln!("[LOAD-END] {file_name}");
        }
    }
    Ok(())
}
