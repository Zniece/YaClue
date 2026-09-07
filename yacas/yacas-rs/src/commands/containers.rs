//! Association and array core commands.

use std::rc::Rc;

use super::{arg, arity_of, int_text_of, num_text};
use crate::env::Environment;
use crate::errors::YacasError;
use crate::evaluator::eval;
use crate::value::{copy_node, LispObject, ObjectKind};

// ============ Association/Array container commands ============

/// Take the argument's AssociationClass (non-Association -> InvalidArg).
fn as_association(
    v: &Rc<LispObject>,
) -> Result<Rc<crate::containers::AssociationClass>, YacasError> {
    match &v.kind {
        ObjectKind::Generic(g) => g.downcast_assoc().ok_or(YacasError::InvalidArg),
        _ => Err(YacasError::InvalidArg),
    }
}

/// Take the argument's ArrayClass (non-Array -> InvalidArg).
fn as_array(v: &Rc<LispObject>) -> Result<Rc<crate::containers::ArrayClass>, YacasError> {
    match &v.kind {
        ObjectKind::Generic(g) => g.downcast_array().ok_or(YacasError::InvalidArg),
        _ => Err(YacasError::InvalidArg),
    }
}

pub(super) fn list_of(kinds: Vec<ObjectKind>, env: &mut Environment) -> Rc<LispObject> {
    let mut all = Vec::with_capacity(kinds.len() + 1);
    all.push(ObjectKind::Atom(env.symtab.look_up("List")));
    all.extend(kinds);
    let chain = crate::value::build_list(all).expect("list");
    Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(chain),
    })
}

/// Association'Create — create a new empty association (Generic).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp GenAssociationCreate.
pub fn cmd_assoc_create(
    _env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let g: Rc<dyn crate::value::GenericClass> = Rc::new(crate::containers::AssociationClass::new());
    Ok(LispObject::generic(g))
}
pub fn cmd_assoc_size(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let a = as_association(&v)?;
    Ok(num_text(a.size().to_string()))
}
pub fn cmd_assoc_contains(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let a = as_association(&v)?;
    let k = eval(env, arg(inner, 1)?)?;
    Ok(if a.contains(env, &k) {
        env.true_atom()
    } else {
        env.false_atom()
    })
}
pub fn cmd_assoc_get(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let a = as_association(&v)?;
    let k = eval(env, arg(inner, 1)?)?;
    match a.get(env, &k) {
        Some(val) => Ok(copy_node(&val)),
        None => Ok(LispObject::atom(env.symtab.look_up("Undefined"))),
    }
}
pub fn cmd_assoc_set(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 3 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let a = as_association(&v)?;
    let k = eval(env, arg(inner, 1)?)?;
    let val = eval(env, arg(inner, 2)?)?;
    a.set(env, &k, &val);
    Ok(env.true_atom())
}
pub fn cmd_assoc_drop(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let a = as_association(&v)?;
    let k = eval(env, arg(inner, 1)?)?;
    Ok(if a.drop_key(env, &k) {
        env.true_atom()
    } else {
        env.false_atom()
    })
}
pub fn cmd_assoc_keys(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let a = as_association(&v)?;
    // Upstream's std::map orders keys by InternalStrictTotalOrder; Keys/ToList
    // both emit in that order.
    let mut entries: Vec<(Rc<LispObject>, Rc<LispObject>)> =
        a.pairs.borrow().iter().cloned().collect();
    entries.sort_by(|x, y| {
        if crate::standard::total_less(env, &x.0, &y.0) {
            std::cmp::Ordering::Less
        } else if crate::standard::total_less(env, &y.0, &x.0) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
    let kinds: Vec<ObjectKind> = entries
        .iter()
        .map(|(k, _)| crate::value::spine_kinds(k).next().expect("kind"))
        .collect();
    Ok(list_of(kinds, env))
}
pub fn cmd_assoc_to_list(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let a = as_association(&v)?;
    let mut entries: Vec<(Rc<LispObject>, Rc<LispObject>)> =
        a.pairs.borrow().iter().cloned().collect();
    entries.sort_by(|x, y| {
        if crate::standard::total_less(env, &x.0, &y.0) {
            std::cmp::Ordering::Less
        } else if crate::standard::total_less(env, &y.0, &x.0) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
    let pairs: Vec<Rc<LispObject>> = entries
        .iter()
        .map(|(k, val)| {
            let mut all = vec![ObjectKind::Atom(env.symtab.look_up("List"))];

            all.push(crate::value::spine_kinds(k).next().expect("k"));
            all.push(crate::value::spine_kinds(val).next().expect("v"));
            let chain = crate::value::build_list(all).expect("pair");
            Rc::new(LispObject {
                next: None,
                kind: ObjectKind::Sublist(chain),
            })
        })
        .collect();
    let kinds: Vec<ObjectKind> = pairs
        .iter()
        .map(|p| crate::value::spine_kinds(p).next().expect("p"))
        .collect();
    Ok(list_of(kinds, env))
}

/// Association'Head — errors on an empty association; otherwise returns the
/// first sorted pair {key, value} (upstream AssociationClass::Head takes
/// _map.begin()).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp GenAssociationHead.
pub fn cmd_assoc_head(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let a = as_association(&v)?;
    let entries: Vec<(Rc<LispObject>, Rc<LispObject>)> = a.pairs.borrow().iter().cloned().collect();
    if entries.is_empty() {
        return Err(YacasError::InvalidArg); // upstream CheckArg(Size,1): bad arg number 1
    }
    let mut sorted = entries;
    sorted.sort_by(|x, y| {
        if crate::standard::total_less(env, &x.0, &y.0) {
            std::cmp::Ordering::Less
        } else if crate::standard::total_less(env, &y.0, &x.0) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
    let (k, val) = &sorted[0];
    let mut all = vec![ObjectKind::Atom(env.symtab.look_up("List"))];

    all.push(crate::value::spine_kinds(k).next().expect("k"));
    all.push(crate::value::spine_kinds(val).next().expect("v"));
    let chain = crate::value::build_list(all).expect("pair");
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(chain),
    }))
}

/// Array'Create/Get/Set/Size (upstream GenArray*; 1-based index, size 0 allowed).
/// Signature Array'Create(size, fill): the first argument is the length
/// (upstream GenArrayCreate); fill is only the initial value, per script usage
/// such as Array'Create(7,0).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp GenArrayCreate.
pub fn cmd_array_create(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let size_node = eval(env, arg(inner, 0)?)?;
    let size = int_text_of(&size_node)?;
    if size < 0 {
        return Err(YacasError::InvalidArg);
    }
    let fill = eval(env, arg(inner, 1)?)?;
    let arr = crate::containers::ArrayClass::new(size as usize, &fill);
    let g: Rc<dyn crate::value::GenericClass> = Rc::new(arr);
    Ok(LispObject::generic(g))
}
pub fn cmd_array_get(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let arr = as_array(&v)?;
    let idx_node = eval(env, arg(inner, 1)?)?;
    let idx = int_text_of(&idx_node)?;
    // 1-based indexing; non-positive indices are argument errors, not
    // underflow (upstream checks before subtracting).
    if idx < 1 {
        return Err(YacasError::InvalidArg);
    }
    let val = arr.get((idx as usize) - 1)?;
    Ok(copy_node(&val))
}
pub fn cmd_array_set(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 3 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let arr = as_array(&v)?;
    let idx_node = eval(env, arg(inner, 1)?)?;
    let idx = int_text_of(&idx_node)?;
    let val = eval(env, arg(inner, 2)?)?;
    // 1-based indexing; non-positive indices are argument errors.
    if idx < 1 {
        return Err(YacasError::InvalidArg);
    }
    arr.set((idx as usize) - 1, &val)?;
    Ok(env.true_atom())
}
pub fn cmd_array_size(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let arr = as_array(&v)?;
    Ok(num_text(arr.size().to_string()))
}
