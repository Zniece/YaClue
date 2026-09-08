//! Value predicates and environment-local assumption commands.

use std::rc::Rc;

use super::{arg, arity_of, int_text_of};
use crate::assumptions::Assumption;
use crate::env::Environment;
use crate::errors::YacasError;
use crate::evaluator::eval;
use crate::value::{LispObject, ObjectKind};

/// Predicate family — Function flag: arguments already evaluated.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispIsXxx.
/// Semantics: IsFunction(f(x)) -> True, IsNumber("5") -> False, and so on.
/// IsFunction: does the value hold a sublist?
pub fn cmd_is_function(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    Ok(crate::standard::internal_boolean(
        env,
        v.sublist().is_some(),
    ))
}

/// IsAtom — true unless the value is a sublist: numbers, strings and plain atoms
/// are all atoms (upstream: SubList()==null); a sublist is False.
/// Upstream behavior: IsAtom(1)=True, IsAtom(2.5)=True, IsAtom(foo)=True,
/// IsAtom({1})=False, IsAtom("s")=True.
/// Note: numbers count as atoms (LispNumber and LispAtom are the same family).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispIsAtom.
pub fn cmd_is_atom(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    Ok(crate::standard::internal_boolean(
        env,
        !matches!(v.kind, ObjectKind::Sublist(_)),
    ))
}

/// IsNumber — true only for number nodes (upstream: Number()!=null); a string
/// like "5" is not a number.
pub fn cmd_is_number(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    Ok(crate::standard::internal_boolean(
        env,
        matches!(v.kind, ObjectKind::Number(_)),
    ))
}

/// IsInteger — true when the numeric representation has no decimal point and no
/// exponent (upstream: LispIsInteger -> BigNumber.IsInt). The check is decided
/// directly on the text: 5 -> True, 5.5 -> False, 3.0 -> False.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispIsInteger.
/// Integer sign predicate family (upstream: the &/|/% rule suffix predicates in
/// standard.ys use a_IsNonNegativeInteger & b_IsNonNegativeInteger <-- BitAnd(a,b)
/// etc.; 2&5 -> 0, 5%2 -> 1 depend on these). Integer argument -> compare by
/// sign; otherwise False.
fn int_sign_predicate(
    env: &mut Environment,
    inner: &Rc<LispObject>,
    f: fn(i64) -> bool,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let n = match int_text_of(&v) {
        Ok(n) => n,
        Err(_) => return Ok(env.false_atom()),
    };
    Ok(crate::standard::internal_boolean(env, f(n)))
}
pub fn cmd_is_non_negative_integer(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    int_sign_predicate(env, inner, |n| n >= 0)
}
pub fn cmd_is_positive_integer(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    int_sign_predicate(env, inner, |n| n > 0)
}
pub fn cmd_is_non_positive_integer(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    int_sign_predicate(env, inner, |n| n <= 0)
}
pub fn cmd_is_negative_integer(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    int_sign_predicate(env, inner, |n| n < 0)
}
pub fn cmd_is_integer(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    let text = match &v.kind {
        ObjectKind::Number(n) => Some(n.string()),
        _ => return Ok(env.false_atom()),
    };
    let is_int = match text {
        Some(t) => !t.contains('.') && !t.contains('e') && !t.contains('E'),
        None => false,
    };
    Ok(crate::standard::internal_boolean(env, is_int))
}

/// Add/query/clear environment-local assumptions. `Assume` and `IsAssumed`
/// hold the symbol name. `IsAssumedValue` evaluates its first argument once
/// for use inside script rules whose pattern parameter is locally bound; a
/// resolved non-symbol is simply not an assumed atom.
pub fn cmd_assume(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let symbol = arg(inner, 0)?.atom_string().ok_or(YacasError::InvalidArg)?;
    let fact_name = arg(inner, 1)?.atom_string().ok_or(YacasError::InvalidArg)?;
    let fact = Assumption::parse(fact_name).ok_or(YacasError::InvalidArg)?;
    env.assume(symbol, fact)
        .map_err(|_| YacasError::InvalidArg)?;
    Ok(env.true_atom())
}

pub fn cmd_is_assumed(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let symbol = arg(inner, 0)?.atom_string().ok_or(YacasError::InvalidArg)?;
    let fact_name = arg(inner, 1)?.atom_string().ok_or(YacasError::InvalidArg)?;
    let fact = Assumption::parse(fact_name).ok_or(YacasError::InvalidArg)?;
    Ok(crate::standard::internal_boolean(
        env,
        env.is_assumed(symbol, fact),
    ))
}

pub fn cmd_is_assumed_value(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let symbol_node = eval(env, arg(inner, 0)?)?;
    let fact_name = arg(inner, 1)?.atom_string().ok_or(YacasError::InvalidArg)?;
    let fact = Assumption::parse(fact_name).ok_or(YacasError::InvalidArg)?;
    let Some(symbol) = symbol_node.atom_string() else {
        return Ok(env.false_atom());
    };
    Ok(crate::standard::internal_boolean(
        env,
        env.is_assumed(symbol, fact),
    ))
}

pub fn cmd_clear_assumptions(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    env.clear_assumptions();
    Ok(env.true_atom())
}

pub fn cmd_push_assumptions(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    env.push_assumptions();
    Ok(env.true_atom())
}

pub fn cmd_pop_assumptions(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if !env.pop_assumptions() {
        return Err(YacasError::InvalidArg);
    }
    Ok(env.true_atom())
}

/// IsList — true when the value is a sublist whose head is List
/// (upstream: InternalIsList).
pub fn cmd_is_list(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    Ok(crate::standard::internal_boolean(
        env,
        crate::standard::internal_is_list(&v),
    ))
}

/// IsString — true for string atoms (upstream: InternalIsString(String())).
pub fn cmd_is_string(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    let s = match v.atom_string() {
        Some(s) => s.to_string(),
        None => return Ok(env.false_atom()),
    };
    Ok(crate::standard::internal_boolean(
        env,
        crate::standard::internal_is_string(&s),
    ))
}
