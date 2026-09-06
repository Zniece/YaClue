//! Symbol interning table (contract §1).
//!
//! Symbols are interned as `Rc<str>`: strings with identical contents share
//! one `Rc` (pointer equality implies value equality). An entry referenced
//! only by the table (`strong_count == 1`) is dead and is reclaimed by
//! [`SymbolTable::garbage_collect`]. See upstream
//! `cyacas/libyacas/src/lisphash.cpp` for the equivalent interning semantics.

use std::collections::HashMap;
use std::rc::Rc;

#[derive(Default)]
pub struct SymbolTable {
    map: HashMap<Rc<str>, ()>,
}

impl SymbolTable {
    /// Intern: return the existing `Rc` on hit, otherwise insert a new one.
    pub fn look_up(&mut self, s: &str) -> Rc<str> {
        if let Some((rc, _)) = self.map.get_key_value(s) {
            return rc.clone();
        }
        let rc: Rc<str> = Rc::from(s);
        self.map.insert(rc.clone(), ());
        rc
    }

    /// Read-only lookup without interning (for side-effect-free users such as
    /// the printer).
    pub fn get(&self, s: &str) -> Option<&Rc<str>> {
        self.map.get_key_value(s).map(|(k, _)| k)
    }

    /// Intern wrapped in double quotes (the quoted form produced by the C++
    /// `String()` command).
    pub fn look_up_stringify(&mut self, s: &str) -> Rc<str> {
        self.look_up(&format!("\"{s}\""))
    }

    /// Intern after stripping surrounding quotes (callers guarantee the
    /// string is quoted).
    pub fn look_up_unstringify(&mut self, s: &str) -> Rc<str> {
        if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
            self.look_up(&s[1..s.len() - 1])
        } else {
            self.look_up(s)
        }
    }

    /// Reclaim dead symbols: drop entries referenced only by this table
    /// (C++ hash `GarbageCollect` semantics). Triggered by upper layers when
    /// symbols may have become unreachable (variables/rules cleared).
    pub fn garbage_collect(&mut self) {
        self.map.retain(|k, _| Rc::strong_count(k) > 1);
    }

    /// Total number of interned symbols (tests/diagnostics).
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether the table has no symbols.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_identity() {
        let mut t = SymbolTable::default();
        let a = t.look_up("f");
        let b = t.look_up("f");
        assert!(Rc::ptr_eq(&a, &b), "identical strings share one Rc");
        assert_ne!(t.look_up("g"), t.look_up("h"));
    }

    #[test]
    fn gc_removes_dead_symbols() {
        let mut t = SymbolTable::default();
        let s = t.look_up("dead");
        assert_eq!(t.len(), 1);
        drop(s); // the only external reference is gone; the table holds the last one
        t.garbage_collect();
        assert_eq!(t.len(), 0, "dead symbols are reclaimed");
    }

    #[test]
    fn gc_keeps_live_symbols() {
        let mut t = SymbolTable::default();
        let live = t.look_up("alive");
        let _also = t.look_up("alive");
        t.garbage_collect();
        assert_eq!(t.len(), 1, "symbols still held externally survive");
        assert!(Rc::ptr_eq(&live, &t.look_up("alive")));
    }
}
