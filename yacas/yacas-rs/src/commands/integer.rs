//! Integer core commands and their arbitrary-precision adapters.

use std::rc::Rc;

use super::{arg, arity_of, int_text_of, map_numeric_work_error, num_text};
use crate::env::Environment;
use crate::errors::YacasError;
use crate::evaluator::eval;
use crate::value::{LispObject, ObjectKind};

/// Bitwise/modulo ops (like upstream BitAnd/BitOr/Mod; standard.ys's &—|—%
/// rules call these). Arguments are converted to integers, then combined via
/// i64 bitwise ops.
pub(super) fn two_ints(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<(i64, i64), YacasError> {
    let a = eval(env, arg(inner, 0)?)?;
    let b = eval(env, arg(inner, 1)?)?;
    let an = int_text_of(&a)?;
    let bn = int_text_of(&b)?;
    Ok((an, bn))
}
pub(super) fn int_number(v: i64) -> Rc<LispObject> {
    let num = crate::value::LispNumber::from_text(v.to_string());
    Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Number(num),
    })
}

#[derive(Debug)]
struct SignedNat {
    negative: bool,
    magnitude: crate::number::nat::Nat,
}

impl SignedNat {
    fn parse(text: &str) -> Result<Self, YacasError> {
        let text = text.trim();
        let (negative, digits) = match text.strip_prefix('-') {
            Some(digits) => (true, digits),
            None => (false, text.strip_prefix('+').unwrap_or(text)),
        };
        let magnitude =
            crate::number::nat::Nat::from_decimal(digits).ok_or(YacasError::InvalidArg)?;
        Ok(Self {
            negative: negative && !magnitude.is_zero(),
            magnitude,
        })
    }

    fn into_text(self) -> String {
        let magnitude = self.magnitude.to_decimal();
        if self.negative && magnitude != "0" {
            format!("-{magnitude}")
        } else {
            magnitude
        }
    }
}

pub(super) fn divrem_with_deadline(
    env: &Environment,
    numerator: &crate::number::nat::Nat,
    denominator: &crate::number::nat::Nat,
) -> Result<(crate::number::nat::Nat, crate::number::nat::Nat), YacasError> {
    numerator
        .divrem_interruptible(denominator, || env.check_eval_deadline().is_err())
        .map_err(|_| YacasError::UserInterrupt)?
        .ok_or(YacasError::InvalidArg)
}

fn integer_text(node: &Rc<LispObject>) -> Result<String, YacasError> {
    node.number_string()
        .or_else(|| node.atom_string().map(|s| s.to_string()))
        .ok_or(YacasError::InvalidArg)
}
pub fn cmd_bit_and(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let (a, b) = two_ints(env, inner)?;
    Ok(int_number(a & b))
}
pub fn cmd_bit_or(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let (a, b) = two_ints(env, inner)?;
    Ok(int_number(a | b))
}
pub fn cmd_mod(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let a = eval(env, arg(inner, 0)?)?;
    let b = eval(env, arg(inner, 1)?)?;
    let at = integer_text(&a)?;
    let bt = integer_text(&b)?;
    // Fast path: i64; on overflow fall back to Nat big integers (PollardRho's
    // Mod(PollardRhoPolynomial(x),n) depends on big-int mod)
    if let (Ok(x), Ok(y)) = (at.trim().parse::<i64>(), bt.trim().parse::<i64>()) {
        if y == 0 {
            return Err(YacasError::InvalidArg);
        }
        if y < 0 {
            return crate::standard::return_un_evaluated(env, inner);
        }
        return Ok(int_number(x.rem_euclid(y)));
    }
    let x = SignedNat::parse(&at)?;
    let y = SignedNat::parse(&bt)?;
    if y.negative {
        return crate::standard::return_un_evaluated(env, inner);
    }
    let (_, mut remainder) = divrem_with_deadline(env, &x.magnitude, &y.magnitude)?;
    if x.negative && !remainder.is_zero() {
        remainder = y.magnitude.sub(&remainder).ok_or(YacasError::InvalidArg)?;
    }
    Ok(num_text(remainder.to_decimal()))
}
/// Shift (like upstream LispShiftLeft/LispShiftRight): left shift multiplies
/// by 2^k; right shift divides the magnitude by 2^k and truncates toward zero.
fn cmd_shift(
    env: &mut Environment,
    inner: &Rc<LispObject>,
    left: bool,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let a = eval(env, arg(inner, 0)?)?;
    let k = int_text_of(&eval(env, arg(inner, 1)?)?)?;
    if k < 0 {
        return Err(YacasError::InvalidArg);
    }
    if left && k as u64 > crate::number::limits::MAX_BINARY_WORK_BITS {
        return Err(YacasError::NumericOverflow);
    }
    let text = integer_text(&a)?;
    if let Ok(small) = text.trim().parse::<i64>() {
        if left {
            if let Ok(k32) = u32::try_from(k) {
                if let Some(wide) = (small as i128).checked_shl(k32) {
                    if let Ok(shifted) = i64::try_from(wide) {
                        return Ok(int_number(shifted));
                    }
                }
            }
        } else {
            let shifted = if k >= 64 {
                0
            } else {
                ((small as i128) / (1i128 << k)) as i64
            };
            return Ok(int_number(shifted));
        }
    }
    let mut value = SignedNat::parse(&text)?;
    if !left && k as u64 >= value.magnitude.bit_len() {
        return Ok(num_text("0".to_string()));
    }
    let k = u32::try_from(k).map_err(|_| YacasError::NumericOverflow)?;
    let factor = crate::number::nat::Nat::from_decimal("2")
        .expect("2")
        .pow_with_limits(k, || env.check_eval_deadline().is_err())
        .map_err(map_numeric_work_error)?;
    value.magnitude = if left {
        let product = value
            .magnitude
            .mul_interruptible(&factor, || env.check_eval_deadline().is_err())
            .map_err(|_| YacasError::UserInterrupt)?;
        if product.to_decimal().len() > crate::number::limits::MAX_DECIMAL_WORK_DIGITS as usize {
            return Err(YacasError::NumericOverflow);
        }
        product
    } else {
        value
            .magnitude
            .divrem(&factor)
            .ok_or(YacasError::InvalidArg)?
            .0
    };
    value.negative &= !value.magnitude.is_zero();
    Ok(num_text(value.into_text()))
}
pub fn cmd_shift_left(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    cmd_shift(env, inner, true)
}
pub fn cmd_shift_right(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    cmd_shift(env, inner, false)
}

/// Bitwise XOR of integers (the same family as `BitAnd` and `BitOr`).
pub fn cmd_bit_xor(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let (a, b) = two_ints(env, inner)?;
    Ok(int_number(a ^ b))
}

/// MathFac (like upstream LispFac -> LispFactorial; Function|Fixed): exact
/// integer n! (accumulated with big Nat integers). sums.rep's
/// `20 # ((n_IsPositiveInteger)!)` rule body `MathFac(n)` depends on this.
/// See upstream: cyacas/libyacas/src/mathcommands3.cpp LispFac.
pub fn cmd_math_fac(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let n: i64 = int_text_of(&v)?;
    if n < 0 {
        return Err(YacasError::InvalidArg);
    }
    if n > crate::number::limits::MAX_FACTORIAL_ARGUMENT {
        return Err(YacasError::NumericOverflow);
    }
    let mut acc = crate::number::nat::Nat::from_decimal("1").expect("1");
    let mut k: i64 = 2;
    while k <= n {
        if k & 0xff == 0 {
            env.check_eval_deadline()?;
        }
        let kk = crate::number::nat::Nat::from_decimal(&k.to_string()).expect("k");
        acc = acc.mul(&kk);
        k += 1;
    }
    Ok(num_text(acc.to_decimal()))
}

/// MathDiv (like upstream LispDiv -> BigNumber::Divide -> ZZ::operator/=):
/// integer quotient truncated **toward zero** (MathDiv(-7,3) = -2, not -3;
/// Rem relies on n - m*Div(n,m) giving -1). Operands are arbitrary-precision
/// integers (FactorizeInt/ContFrac/simplify paths divide numbers beyond the
/// i64 range); a fast i64 path covers the common small-operand case.
pub fn cmd_math_div(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let a = eval(env, arg(inner, 0)?)?;
    let b = eval(env, arg(inner, 1)?)?;
    let at = a
        .number_string()
        .or_else(|| a.atom_string().map(|s| s.to_string()))
        .ok_or(YacasError::InvalidArg)?
        .trim()
        .to_string();
    let bt = b
        .number_string()
        .or_else(|| b.atom_string().map(|s| s.to_string()))
        .ok_or(YacasError::InvalidArg)?
        .trim()
        .to_string();
    if let (Ok(x), Ok(y)) = (at.parse::<i64>(), bt.parse::<i64>()) {
        if y == 0 {
            return Err(YacasError::InvalidArg);
        }
        if !(x == i64::MIN && y == -1) {
            // i64::MIN / -1 溢出(wrapping_div 会静默回绕成 i64::MIN),回落大数路径
            return Ok(int_number(x.wrapping_div(y)));
        }
    }
    let neg = at.starts_with('-') ^ bt.starts_with('-');
    let x = crate::number::nat::Nat::from_decimal(at.trim_start_matches('-'))
        .ok_or(YacasError::InvalidArg)?;
    let y = crate::number::nat::Nat::from_decimal(bt.trim_start_matches('-'))
        .ok_or(YacasError::InvalidArg)?;
    if y.is_zero() {
        return Err(YacasError::InvalidArg);
    }
    let (q, _) = divrem_with_deadline(env, &x, &y)?;
    let text = q.to_decimal();
    let text = if neg && !q.is_zero() {
        format!("-{text}")
    } else {
        text
    };
    Ok(num_text(text))
}

/// MathGcd (like upstream LispGcd): Euclidean algorithm on the absolute values
/// of both arguments.
/// See upstream: cyacas/libyacas/src/mathcommands3.cpp LispGcd.
pub fn cmd_math_gcd(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let a = eval(env, arg(inner, 0)?)?;
    let b = eval(env, arg(inner, 1)?)?;
    let at = a
        .number_string()
        .or_else(|| a.atom_string().map(|s| s.to_string()))
        .ok_or(YacasError::InvalidArg)?;
    let bt = b
        .number_string()
        .or_else(|| b.atom_string().map(|s| s.to_string()))
        .ok_or(YacasError::InvalidArg)?;
    // Fast path: within the i64 domain (the vast majority of calls); on
    // overflow fall back to Nat big-integer Euclid (FactorizeInt's
    // Gcd(ProductPrimesTo257(),n) needs a ~128-bit product).
    if let (Ok(x), Ok(y)) = (at.trim().parse::<i64>(), bt.trim().parse::<i64>()) {
        let mut x = x.unsigned_abs();
        let mut y = y.unsigned_abs();
        while y != 0 {
            let r = x % y;
            x = y;
            y = r;
        }
        return Ok(num_text(x.to_string()));
    }
    let mut x = crate::number::nat::Nat::from_decimal(at.trim_start_matches('-'))
        .ok_or(YacasError::InvalidArg)?;
    let mut y = crate::number::nat::Nat::from_decimal(bt.trim_start_matches('-'))
        .ok_or(YacasError::InvalidArg)?;
    while !y.is_zero() {
        let (_, r) = divrem_with_deadline(env, &x, &y)?;
        x = y;
        y = r;
    }
    Ok(num_text(x.to_decimal()))
}
