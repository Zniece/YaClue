//! Value nodes (contract §1): four content kinds + a `next` chain; copies
//! are exclusive.
//!
//! Evaluation contract (critical for correctness): `copy_node`/`copy_spine`
//! produce independent chains, which is what makes it safe for callers to
//! rewrite their `next` pointers (copy first, then own, before surgery).
//! Kind *contents* stay shared (interned symbols, immutable numbers and
//! floats, shared sublists/generics). In-place mutation of a shared node
//! must go through `Rc::get_mut`, which returns `None` while the node is
//! shared — the Rust equivalent of the upstream `use_count == 1`
//! destructive-update discipline.

use std::rc::Rc;

use crate::number::float::Float;
use crate::symtab::SymbolTable;

/// A symbol is an interned string (pointer equality implies value equality).
pub type Symbol = Rc<str>;

/// Generic object abstraction (arrays, associations, patterns, ...).
pub trait GenericClass {
    fn type_name(&self) -> &'static str;
    /// Downcast hook for container commands; concrete container classes
    /// override this to return `Some(self)`, the default is `None`.
    fn downcast_assoc(&self) -> Option<std::rc::Rc<crate::containers::AssociationClass>> {
        None
    }
    fn downcast_array(&self) -> Option<std::rc::Rc<crate::containers::ArrayClass>> {
        None
    }
    /// Optional pattern-matching capability (implemented by the Pattern
    /// generic object; `None` for everything else).
    fn matches_pattern(
        &self,
        env: &mut crate::env::Environment,
        arguments: &[Rc<LispObject>],
    ) -> Result<Option<bool>, crate::errors::YacasError> {
        let _ = (env, arguments);
        Ok(None)
    }
}

/// A number: lazy text ↔ float pair plus a *type* flag.
///
/// `is_float` is the type marker (integer vs float, as in the upstream
/// `BigNumber`): a literal containing `.`/exponent, or any operand
/// contamination during arithmetic (`9+0.` → `9.`, `2+3` → `5`). It affects
/// printing (float-valued integers print as `"9."`) and the bit commands'
/// type dispatch.
#[derive(Clone)]
pub struct LispNumber {
    text: Option<String>,
    num: Option<Float>,
    is_float: bool,
}

impl LispNumber {
    pub fn from_text(s: String) -> LispNumber {
        let is_float = s.contains('.') || s.contains('e') || s.contains('E');
        LispNumber {
            text: Some(s),
            num: None,
            is_float,
        }
    }
    pub fn from_float(f: Float) -> LispNumber {
        LispNumber {
            text: None,
            num: Some(f),
            is_float: true,
        }
    }
    /// Construct with an explicit type flag (arithmetic commands decide by
    /// operand contamination: int op int → int).
    ///
    /// `prec` semantics: add/mul/div outputs are truncated to session
    /// precision on exit and carry `prec` = session precision; paths with
    /// their own bit count (exact integer division, `MathSetExactBits`,
    /// `N` results) set `prec = 0` meaning "storage form" — printing does
    /// not truncate those to session precision (`x := N(2/3, 20)` keeps 20
    /// digits even at session precision 10).
    pub fn from_float_flag(f: Float, is_float: bool) -> LispNumber {
        LispNumber {
            text: None,
            num: Some(f),
            is_float,
        }
    }
    /// Type flag.
    pub fn is_float(&self) -> bool {
        self.is_float
    }
    /// Float value at the requested precision.
    ///
    /// Storage = literal text: literals with a fractional part are neither
    /// padded nor truncated (`MathAdd(1.414…2097, 0)` at precision 30 keeps
    /// all 28 digits); literals without a fractional part are zero-padded to
    /// session precision (the int→float storage form). An already-parsed
    /// float whose literal is longer is re-parsed to raise precision.
    pub fn float_at(&self, prec: u32) -> Float {
        let mut f = match &self.num {
            Some(f) => {
                if !f.is_integer() && f.prec() < prec && self.text.is_some() {
                    Float::from_decimal_with_prec(self.text.as_deref().unwrap_or("0"), prec)
                        .unwrap_or_else(|| f.clone())
                } else {
                    f.clone()
                }
            }
            None => Float::from_decimal_with_prec(self.text.as_deref().unwrap_or("0"), prec)
                .expect("LispNumber text must be a valid number"),
        };
        if f.frac_scale() == 0 && !f.is_zero() {
            f.pad_to_precision(prec);
        }
        f
    }

    /// Stored bit count: `max(session precision, all literal digits)`.
    pub fn digit_count_storage(&self, session_prec: u32) -> u32 {
        self.float_at(session_prec).digit_count().max(session_prec)
    }

    /// Float value at default precision.
    pub fn float(&self) -> Float {
        self.float_at(crate::number::float::DEFAULT_PREC)
    }
    /// Render as text: unevaluated numbers pass the literal through;
    /// evaluated ones go through canonical float printing, truncated to
    /// session precision.
    pub fn string_at(&self, session_prec: u32) -> String {
        let s = match &self.num {
            Some(f) => {
                // prec == 0 = storage form: print in full, not truncated to
                // session precision.
                let s = if f.prec() == 0 {
                    f.format()
                } else {
                    f.format_at(session_prec)
                };
                if self.is_float
                    && f.is_integer()
                    && !f.is_zero()
                    && !s.contains('.')
                    && !s.contains('e')
                    && !s.contains('E')
                {
                    format!("{s}.")
                } else {
                    s
                }
            }
            None => return self.text.clone().unwrap_or_else(|| "0".into()),
        };
        // Truncation can land exactly on an integer value (1.9999999 → 2);
        // floats keep the trailing dot.
        if self.is_float && !s.contains('.') && !s.contains('e') && !s.contains('E') {
            return format!("{s}.");
        }
        s
    }
    /// Render as text at default precision (literals pass through).
    pub fn string(&self) -> String {
        match &self.num {
            Some(f) => {
                let s = f.format();
                if self.is_float
                    && f.is_integer()
                    && !f.is_zero()
                    && !s.contains('.')
                    && !s.contains('e')
                    && !s.contains('E')
                {
                    format!("{s}.")
                } else {
                    s
                }
            }
            None => self.text.clone().unwrap_or_else(|| "0".into()),
        }
    }
    /// Integer test (simple view: text without fraction/exponent, or an
    /// integral float; full semantics live in the evaluation layer).
    pub fn is_int(&self) -> bool {
        if self.is_float {
            return false;
        }
        match &self.num {
            Some(f) => f.is_integer(),
            None => match &self.text {
                Some(t) => !t.contains('.') && !t.contains('e') && !t.contains('E'),
                None => true,
            },
        }
    }
}

/// The four content kinds of a node.
pub enum ObjectKind {
    Atom(Symbol),
    Number(LispNumber),
    /// Sublist contents: the first element; its `next` chain is the list.
    Sublist(Rc<LispObject>),
    Generic(Rc<dyn GenericClass>),
}

/// A value node: `next` chain + content.
pub struct LispObject {
    pub next: Option<Rc<LispObject>>,
    pub kind: ObjectKind,
}

impl LispObject {
    pub fn new(kind: ObjectKind) -> Rc<LispObject> {
        Rc::new(LispObject { next: None, kind })
    }

    pub fn atom(sym: Symbol) -> Rc<LispObject> {
        LispObject::new(ObjectKind::Atom(sym))
    }

    pub fn number(n: LispNumber) -> Rc<LispObject> {
        LispObject::new(ObjectKind::Number(n))
    }

    /// Atom name (`String()` semantics); numbers are not atoms here.
    pub fn atom_string(&self) -> Option<&Symbol> {
        match &self.kind {
            ObjectKind::Atom(s) => Some(s),
            _ => None,
        }
    }

    /// Rendered number text (unevaluated literals pass through).
    pub fn number_string(&self) -> Option<String> {
        match &self.kind {
            ObjectKind::Number(n) => Some(n.string()),
            _ => None,
        }
    }

    /// `SubList()` semantics: the sublist content chain.
    pub fn sublist(&self) -> Option<&Rc<LispObject>> {
        match &self.kind {
            ObjectKind::Sublist(first) => Some(first),
            _ => None,
        }
    }
}

/// Build an atom node via the symbol table.
pub fn make_atom(table: &mut SymbolTable, s: &str) -> Rc<LispObject> {
    LispObject::atom(table.look_up(s))
}

/// Unified entry for atoms and numbers: if `s` is a number, build a
/// `LispNumber` (literal passthrough), otherwise an interned atom. The
/// parser builds all nodes through this.
pub fn atom_or_number(table: &mut SymbolTable, s: &str) -> Rc<LispObject> {
    // Operators like `+`, `-`, `.` with an empty mantissa must NOT classify
    // as numbers (Float::from_decimal would parse them as 0) — see
    // `is_number`'s strict mode.
    if crate::standard::is_number(s, true) {
        LispObject::number(LispNumber::from_text(s.to_string()))
    } else {
        make_atom(table, s)
    }
}

/// Build a chain from contents (`next = None` on the last node); empty input
/// returns `None`. During construction each node is held by a single `Rc`,
/// so the finished head has refcount 1 and can be `make_mut`-ed.
pub fn build_list(kinds: Vec<ObjectKind>) -> Option<Rc<LispObject>> {
    let mut next: Option<Rc<LispObject>> = None;
    for kind in kinds.into_iter().rev() {
        next = Some(Rc::new(LispObject {
            next: next.take(),
            kind,
        }));
    }
    next
}

pub fn clone_kind(k: &ObjectKind) -> ObjectKind {
    match k {
        ObjectKind::Atom(s) => ObjectKind::Atom(s.clone()),
        ObjectKind::Number(num) => ObjectKind::Number(num.clone()),
        ObjectKind::Sublist(first) => ObjectKind::Sublist(first.clone()),
        ObjectKind::Generic(g) => ObjectKind::Generic(g.clone()),
    }
}

/// Shallow copy (contract §1): a fresh node with `next = None` and shared
/// kind content. The result is independent, so its `next` is safe to mutate.
pub fn copy_node(n: &Rc<LispObject>) -> Rc<LispObject> {
    LispObject::new(clone_kind(&n.kind))
}

/// Copy the whole spine: every node gets a new `Rc`, contents stay shared,
/// order is preserved — the result is exclusively owned.
pub fn copy_spine(start: &Rc<LispObject>) -> Rc<LispObject> {
    let kinds: Vec<ObjectKind> = spine_kinds(start).collect();
    build_list(kinds).expect("copy_spine: input must be non-empty")
}

/// Iterator over the kinds of each node on the spine (read-only; does not
/// borrow the chain structurally).
pub fn spine_kinds(start: &Rc<LispObject>) -> impl Iterator<Item = ObjectKind> + '_ {
    spine_refs(start).map(|n| clone_kind(&n.kind))
}

/// Read-only traversal along the `next` chain.
pub fn spine_refs(start: &Rc<LispObject>) -> impl Iterator<Item = &Rc<LispObject>> {
    std::iter::successors(Some(start), |n| n.next.as_ref())
}

/// Structural equality: same rendered text, sublists compared element-wise.
/// Numbers compare by rendered text first.
pub fn equal(a: &LispObject, b: &LispObject) -> bool {
    match (&a.kind, &b.kind) {
        (ObjectKind::Atom(x), ObjectKind::Atom(y)) => Rc::ptr_eq(x, y),
        (ObjectKind::Number(x), ObjectKind::Number(y)) => x.string() == y.string(),
        (ObjectKind::Sublist(x), ObjectKind::Sublist(y)) => {
            let mut ia = spine_refs(x);
            let mut ib = spine_refs(y);
            loop {
                match (ia.next(), ib.next()) {
                    (None, None) => return true,
                    (Some(na), Some(nb)) => {
                        if !equal(na, nb) {
                            return false;
                        }
                    }
                    _ => return false,
                }
            }
        }
        (ObjectKind::Generic(x), ObjectKind::Generic(y)) => {
            Rc::ptr_eq(x, y) || x.type_name() == y.type_name()
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_state_accessors() {
        let mut t = SymbolTable::default();
        let a = make_atom(&mut t, "f");
        assert_eq!(a.atom_string().map(|s| s.as_ref()), Some("f"));
        assert!(a.sublist().is_none());
        let num = LispObject::number(LispNumber::from_text("0.000".into()));
        assert_eq!(num.number_string().as_deref(), Some("0.000"));
        assert!(num.atom_string().is_none());
    }

    #[test]
    fn build_and_iterate() {
        let mut t = SymbolTable::default();
        let f = make_atom(&mut t, "f");
        let g = make_atom(&mut t, "g");
        let list = build_list(vec![
            ObjectKind::Atom(f.as_ref().atom_string().unwrap().clone()),
            ObjectKind::Atom(g.as_ref().atom_string().unwrap().clone()),
        ])
        .unwrap();
        let names: Vec<&str> = spine_refs(&list)
            .filter_map(|n| n.atom_string().map(|s| s.as_ref()))
            .collect();
        assert_eq!(names, vec!["f", "g"]);
    }

    #[test]
    fn copy_spine_independent() {
        let mut t = SymbolTable::default();
        let f = make_atom(&mut t, "f");
        let g = make_atom(&mut t, "g");
        let list = build_list(vec![
            ObjectKind::Atom(f.as_ref().atom_string().unwrap().clone()),
            ObjectKind::Atom(g.as_ref().atom_string().unwrap().clone()),
        ])
        .unwrap();
        let copy = copy_spine(&list);
        // The copy's head is exclusively owned, so the original is untouched
        // by mutations to the copy.
        let mut copy_head = copy;
        assert!(
            Rc::get_mut(&mut copy_head).is_some(),
            "copied head is exclusive"
        );
        // Chain surgery rebuilds: clone kinds + append, then build anew.
        let mut kinds: Vec<ObjectKind> = spine_kinds(&copy_head).collect();
        kinds.push(ObjectKind::Atom(t.look_up("h")));
        let copy2 = build_list(kinds).unwrap();
        let orig_names: Vec<&str> = spine_refs(&list)
            .filter_map(|n| n.atom_string().map(|s| s.as_ref()))
            .collect();
        assert_eq!(orig_names, vec!["f", "g"], "original chain untouched");
        let copy_names: Vec<&str> = spine_refs(&copy2)
            .filter_map(|n| n.atom_string().map(|s| s.as_ref()))
            .collect();
        assert_eq!(copy_names, vec!["f", "g", "h"]);
    }

    #[test]
    fn shared_node_get_mut_none_when_shared() {
        let mut t = SymbolTable::default();
        let a = make_atom(&mut t, "a");
        let a2 = a.clone(); // refcount 2
        let mut m = a;
        assert!(
            Rc::get_mut(&mut m).is_none(),
            "shared nodes refuse get_mut (copy first, then own)"
        );
        let _ = a2;
    }

    #[test]
    fn atom_or_number_dispatches() {
        let mut t = SymbolTable::default();
        let num = atom_or_number(&mut t, "1.5");
        assert!(num.sublist().is_none());
        assert_eq!(num.number_string().as_deref(), Some("1.5"));
        let int = atom_or_number(&mut t, "12345");
        assert!(int.number_string().is_some(), "integers are numbers too");
        let sym = atom_or_number(&mut t, "x+y");
        assert_eq!(sym.atom_string().map(|s| s.as_ref()), Some("x+y"));
        let _ = atom_or_number(&mut t, "-6"); // signed integers are numbers
    }
}
