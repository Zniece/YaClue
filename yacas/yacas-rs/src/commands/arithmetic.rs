use std::rc::Rc;

use super::numeric::map_numeric_work_error;
use super::{arg, arity_of};
use crate::env::Environment;
use crate::errors::YacasError;
use crate::evaluator::eval;
use crate::value::{spine_kinds, LispObject, ObjectKind};

/// `+` (See upstream: cyacas/libyacas/src/mathcommands.cpp LispAdd): arguments evaluated; all numbers -> numeric sum; any
/// symbol -> held (a+b...).
/// MathAdd (See upstream: cyacas/libyacas/src/mathcommands.cpp LispAdd; Function|Fixed, 2 args): fixed arity —
/// MathAdd(5,2,1) and MathAdd(7) are WrongNumberOfArgs (distinguishing it from the
/// variadic `+`).
pub fn cmd_math_add(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    cmd_add(env, inner)
}

/// Addition (variadic; the fixed-2-arg MathAdd is cmd_math_add).
pub fn cmd_add(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    // Seed the accumulator with zero at prec=0 (a storage form, not a truncation
    // cap) — the upstream starts from BigNumber("0", BinaryPrecision()): the
    // environment precision only enters through the add's precision parameter, so
    // results are truncated at the session precision.
    let mut acc = crate::number::float::Float::from_decimal("0")
        .expect("zero")
        .with_prec(0);
    let mut symbolic = false;
    let mut any_float = false; // Float contamination: 9+0. -> "9." while 2+3 -> "5".
    let n = arity_of(inner);
    let mut held: Vec<ObjectKind> = Vec::new();
    for i in 0..n {
        let mut v = eval(env, arg(inner, i)?)?;
        // Upstream behavior (held trees reaching math commands):
        // `T(_x)<--x+x; T(f(aa));` yields 4*aa — math commands re-evaluate held
        // arguments once before holding. A macro parameter read produces the held
        // tree f(aa); `+` re-evaluates it -> 2*aa (re-evaluating atoms/numbers is a
        // no-op). If the re-evaluation is still symbolic, collect it as held directly
        // (a third evaluation would be wrong; falling back to return_un_evaluated on
        // the original inner argument would discard the re-evaluated result).
        if matches!(v.kind, ObjectKind::Sublist(_)) {
            v = eval(env, &v)?;
        }
        match &v.kind {
            ObjectKind::Number(num) => {
                any_float |= num.is_float();
                acc = acc.add(&num.float_at(env.precision()), env.precision());
            }
            _ => {
                symbolic = true;
                held.push(spine_kinds(&v).next().expect("node kind"));
            }
        }
    }
    if symbolic {
        // Held shape = original head (`+`) plus the re-evaluated arguments (the same
        // head+args structure as return_un_evaluated, but with the re-evaluated
        // results; upstream prints `2*aa+bb`, not a List-headed form).
        let plus_sym = env.symtab.look_up("+");
        let mut all: Vec<ObjectKind> = Vec::with_capacity(held.len() + 1);
        all.push(ObjectKind::Atom(plus_sym));
        all.extend(held);
        let chain = crate::value::build_list(all).ok_or(YacasError::InvalidArg)?;
        return Ok(Rc::new(LispObject {
            next: None,
            kind: ObjectKind::Sublist(chain),
        }));
    }
    let num = crate::value::LispNumber::from_float_flag(acc, any_float);
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Number(num),
    }))
}

/// `*` (See upstream: cyacas/libyacas/src/mathcommands.cpp LispMultiply): multiplies numbers.
pub fn cmd_mul(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let mut acc = crate::number::float::Float::from_decimal("1")
        .expect("one")
        .with_prec(0);
    let mut symbolic = false;
    let mut any_float = false; // Float contamination.
    let n = arity_of(inner);
    for i in 0..n {
        let v = eval(env, arg(inner, i)?)?;
        match &v.kind {
            ObjectKind::Number(num) => {
                any_float |= num.is_float();
                acc = acc
                    .mul_with_limits(&num.float_at(env.precision()), env.precision(), || {
                        env.check_eval_deadline().is_err()
                    })
                    .map_err(map_numeric_work_error)?
            }
            _ => symbolic = true,
        }
    }
    if symbolic {
        return crate::standard::return_un_evaluated(env, inner);
    }
    let num = crate::value::LispNumber::from_float_flag(acc, any_float);
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Number(num),
    }))
}

/// `-` (See upstream: cyacas/libyacas/src/mathcommands.cpp LispSubtract): one argument negates; more arguments subtract in
/// sequence.
pub fn cmd_sub(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n == 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let mut acc = eval(env, arg(inner, 0)?)?;
    let mut any_float = false; // Float contamination.
    if n == 1 {
        if let ObjectKind::Number(num) = &acc.kind {
            any_float = num.is_float();
            let f = num.float_at(env.precision()).negate();
            let num = crate::value::LispNumber::from_float_flag(f, any_float);
            return Ok(Rc::new(LispObject {
                next: None,
                kind: ObjectKind::Number(num),
            }));
        }
        return crate::standard::return_un_evaluated(env, inner);
    }
    let mut symbolic = !matches!(&acc.kind, ObjectKind::Number(_));
    if let ObjectKind::Number(num) = &acc.kind {
        any_float = num.is_float();
    }
    for i in 1..n {
        let v = eval(env, arg(inner, i)?)?;
        match (&acc.kind, &v.kind) {
            (ObjectKind::Number(a), ObjectKind::Number(b)) => {
                any_float |= b.is_float();
                let f = a
                    .float_at(env.precision())
                    .sub(&b.float_at(env.precision()), env.precision());
                acc = Rc::new(LispObject {
                    next: None,
                    kind: ObjectKind::Number(crate::value::LispNumber::from_float_flag(
                        f, any_float,
                    )),
                });
            }
            _ => symbolic = true,
        }
    }
    if symbolic {
        return crate::standard::return_un_evaluated(env, inner);
    }
    Ok(acc)
}

/// `/` (See upstream: cyacas/libyacas/src/mathcommands.cpp LispDivide): divides the arguments numerically.
pub fn cmd_div(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n < 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let mut acc = eval(env, arg(inner, 0)?)?;
    let mut any_float = false; // Float contamination.
    let mut symbolic = !matches!(&acc.kind, ObjectKind::Number(_));
    if let ObjectKind::Number(num) = &acc.kind {
        any_float = num.is_float();
    }
    for i in 1..n {
        let v = eval(env, arg(inner, i)?)?;
        match (&acc.kind, &v.kind) {
            (ObjectKind::Number(a), ObjectKind::Number(b)) => {
                any_float |= b.is_float();
                let f = a
                    .float_at(env.precision())
                    .div_with_limits(&b.float_at(env.precision()), env.precision(), || {
                        env.check_eval_deadline().is_err()
                    })
                    .map_err(map_numeric_work_error)?
                    .ok_or(YacasError::DivideByZero)?;
                acc = Rc::new(LispObject {
                    next: None,
                    kind: ObjectKind::Number(crate::value::LispNumber::from_float_flag(
                        f, any_float,
                    )),
                });
            }
            _ => symbolic = true,
        }
    }
    if symbolic {
        return crate::standard::return_un_evaluated(env, inner);
    }
    Ok(acc)
}

// Comparison value: numeric or textual ordering (the BigNumber/string
// LexCompare semantics).
enum OrderVal {
    Num(crate::number::float::Float),
    Text(String),
}

// LessThan comparison (See upstream: cyacas/libyacas/src/mathcommands.cpp LispLess): evaluate the arguments, then compare
// numerically or by string order.
fn num_or_text(env: &mut Environment, v: &Rc<LispObject>) -> Result<OrderVal, YacasError> {
    match v.atom_string() {
        Some(s) => Ok(OrderVal::Text(s.to_string())),
        None => match v.number_string() {
            Some(n) => {
                let f = crate::number::float::Float::from_decimal_with_prec(&n, env.precision())
                    .ok_or(YacasError::InvalidArg)?;
                Ok(OrderVal::Num(f))
            }
            None => Err(YacasError::InvalidArg),
        },
    }
}
pub fn cmd_less(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let va = eval(env, arg(inner, 0)?)?;
    let a = num_or_text(env, &va)?;
    let vb = eval(env, arg(inner, 1)?)?;
    let b = num_or_text(env, &vb)?;
    let lt = match (&a, &b) {
        (OrderVal::Num(x), OrderVal::Num(y)) => x.less_than(y),
        (OrderVal::Text(x), OrderVal::Text(y)) => x < y,
        _ => return Err(YacasError::InvalidArg),
    };
    if lt {
        Ok(env.true_atom())
    } else {
        Ok(env.false_atom())
    }
}
pub fn cmd_greater(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let va = eval(env, arg(inner, 0)?)?;
    let a = num_or_text(env, &va)?;
    let vb = eval(env, arg(inner, 1)?)?;
    let b = num_or_text(env, &vb)?;
    let gt = match (&a, &b) {
        (OrderVal::Num(x), OrderVal::Num(y)) => y.less_than(x),
        (OrderVal::Text(x), OrderVal::Text(y)) => x > y,
        _ => return Err(YacasError::InvalidArg),
    };
    if gt {
        Ok(env.true_atom())
    } else {
        Ok(env.false_atom())
    }
}
pub fn cmd_less_eq(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let va = eval(env, arg(inner, 0)?)?;
    let a = num_or_text(env, &va)?;
    let vb = eval(env, arg(inner, 1)?)?;
    let b = num_or_text(env, &vb)?;
    let le = match (&a, &b) {
        (OrderVal::Num(x), OrderVal::Num(y)) => x.less_than(y) || x.equals(y),
        (OrderVal::Text(x), OrderVal::Text(y)) => x <= y,
        _ => return Err(YacasError::InvalidArg),
    };
    if le {
        Ok(env.true_atom())
    } else {
        Ok(env.false_atom())
    }
}
pub fn cmd_greater_eq(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let va = eval(env, arg(inner, 0)?)?;
    let a = num_or_text(env, &va)?;
    let vb = eval(env, arg(inner, 1)?)?;
    let b = num_or_text(env, &vb)?;
    let ge = match (&a, &b) {
        (OrderVal::Num(x), OrderVal::Num(y)) => y.less_than(x) || x.equals(y),
        (OrderVal::Text(x), OrderVal::Text(y)) => x >= y,
        _ => return Err(YacasError::InvalidArg),
    };
    if ge {
        Ok(env.true_atom())
    } else {
        Ok(env.false_atom())
    }
}

/// MathSubtract: (a, b) -> numeric a - b (the MathAdd family counterpart).
/// standard.ys's `--` function body depends on this.
pub fn cmd_math_subtract(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let a = eval(env, arg(inner, 0)?)?;
    let b = eval(env, arg(inner, 1)?)?;
    let (an, bn, any_float) = match (&a.kind, &b.kind) {
        (ObjectKind::Number(na), ObjectKind::Number(nb)) => {
            // float_at(env precision) + sub(env precision): like the C++
            // LispSubtract path via AddNumericTotal (the mantissa is trimmed
            // to the global precision).
            (
                na.float_at(env.precision()),
                nb.float_at(env.precision()),
                na.is_float() || nb.is_float(),
            )
        }
        _ => return Err(YacasError::InvalidArg),
    };
    let num = crate::value::LispNumber::from_float_flag(an.sub(&bn, env.precision()), any_float);
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Number(num),
    }))
}
