//! Generic container classes. See upstream
//! `cyacas/libyacas/include/yacas/associationclass.h` and `arrayclass.h`.
//!
//! - `Association` = an ordered `Vec` of key/value pairs (both `Rc`-shared).
//!   Lookup uses the engine's `internal_equals` so keys compare with engine
//!   equality semantics. Linear scan is intentional: associations are small,
//!   and hashing `Rc` keys structurally would risk drifting from engine
//!   equality. Interior mutability via `RefCell`.
//! - Printing: `type_name` returns `"\"Association\""` / `"\"Array\""`
//!   (quoted, as the infix printer expects).

use std::cell::RefCell;
use std::rc::Rc;

use crate::env::Environment;
use crate::errors::YacasError;
use crate::value::{GenericClass, LispObject};

/// Alias for the shared pair list of an `AssociationClass`.
pub type SharedPairs = Rc<RefCell<Vec<(Rc<LispObject>, Rc<LispObject>)>>>;

/// Association list: ordered pairs with engine-equality key lookup.
pub struct AssociationClass {
    pub pairs: SharedPairs,
}

impl Default for AssociationClass {
    fn default() -> Self {
        Self::new()
    }
}

impl AssociationClass {
    pub fn new() -> Self {
        AssociationClass { pairs: Rc::new(RefCell::new(Vec::new())) }
    }

    pub fn size(&self) -> usize {
        self.pairs.borrow().len()
    }

    /// Index of the first pair whose key equals `key` (engine equality).
    fn find(&self, env: &Environment, key: &Rc<LispObject>) -> Option<usize> {
        self.pairs.borrow().iter().position(|(k, _)| crate::standard::internal_equals(env, k, key))
    }

    pub fn contains(&self, env: &Environment, key: &Rc<LispObject>) -> bool {
        self.find(env, key).is_some()
    }

    pub fn get(&self, env: &Environment, key: &Rc<LispObject>) -> Option<Rc<LispObject>> {
        self.find(env, key).map(|i| self.pairs.borrow()[i].1.clone())
    }

    pub fn set(&self, env: &Environment, key: &Rc<LispObject>, value: &Rc<LispObject>) {
        let mut pairs = self.pairs.borrow_mut();
        if let Some(i) = pairs.iter().position(|(k, _)| crate::standard::internal_equals(env, k, key)) {
            pairs[i].1 = value.clone();
        } else {
            pairs.push((key.clone(), value.clone()));
        }
    }

    pub fn drop_key(&self, env: &Environment, key: &Rc<LispObject>) -> bool {
        let mut pairs = self.pairs.borrow_mut();
        if let Some(i) = pairs.iter().position(|(k, _)| crate::standard::internal_equals(env, k, key)) {
            pairs.remove(i);
            true
        } else {
            false
        }
    }
}

impl GenericClass for AssociationClass {
    fn type_name(&self) -> &'static str {
        "\"Association\""
    }
    fn downcast_assoc(&self) -> Option<Rc<crate::containers::AssociationClass>> {
        Some(Rc::new(crate::containers::AssociationClass { pairs: self.pairs.clone() }))
    }
    fn downcast_array(&self) -> Option<Rc<crate::containers::ArrayClass>> {
        None
    }
}

/// Fixed-length array with 1-based indexing; out-of-range access is an error.
pub struct ArrayClass {
    pub slots: Rc<RefCell<Vec<Option<Rc<LispObject>>>>>,
}

impl ArrayClass {
    /// Create a fixed-length array; every slot holds a copy of `fill`.
    /// Each slot gets its own node (observed as independent values via
    /// `Copy()` on access). `len` of 0 is allowed (`Array'Create(0, ...)`).
    pub fn new(len: usize, fill: &Rc<LispObject>) -> Self {
        ArrayClass {
            slots: Rc::new(RefCell::new(vec![Some(crate::value::copy_node(fill)); len])),
        }
    }
    pub fn size(&self) -> usize {
        self.slots.borrow().len()
    }
    /// Slot contents as a copy: the result is independent of the slot and its
    /// `next` pointer can be safely mutated.
    pub fn get(&self, idx: usize) -> Result<Rc<LispObject>, YacasError> {
        let slots = self.slots.borrow();
        let slot = slots.get(idx).ok_or(YacasError::InvalidArg)?;
        match slot {
            Some(v) => Ok(crate::value::copy_node(v)),
            None => Err(YacasError::InvalidArg),
        }
    }
    /// Overwrite a slot in place with a copy of `value`.
    pub fn set(&self, idx: usize, value: &Rc<LispObject>) -> Result<(), YacasError> {
        let mut slots = self.slots.borrow_mut();
        let slot = slots.get_mut(idx).ok_or(YacasError::InvalidArg)?;
        *slot = Some(crate::value::copy_node(value));
        Ok(())
    }
}

impl GenericClass for ArrayClass {
    fn type_name(&self) -> &'static str {
        "\"Array\""
    }
    fn downcast_array(&self) -> Option<Rc<crate::containers::ArrayClass>> {
        Some(Rc::new(crate::containers::ArrayClass { slots: self.slots.clone() }))
    }
}

impl LispObject {
    /// Wrap a generic object into a value node.
    pub fn generic(g: Rc<dyn GenericClass>) -> Rc<LispObject> {
        LispObject::new(crate::value::ObjectKind::Generic(g))
    }
}
