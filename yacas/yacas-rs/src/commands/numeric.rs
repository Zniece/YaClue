//! Float, precision, and bounded numeric core commands.

use std::rc::Rc;

use super::integer::int_number;
use super::{arg, arity_of, int_text_of};
use crate::env::Environment;
use crate::errors::YacasError;
use crate::evaluator::eval;
use crate::value::{LispObject, ObjectKind};

/// Numeric argument to Float (same argument pattern as cmd_math_subtract).
/// Returns (Float, type float flag).
pub(super) fn arg_float_flag(
    env: &mut Environment,
    inner: &Rc<LispObject>,
    i: usize,
) -> Result<(crate::number::float::Float, bool), YacasError> {
    let v = eval(env, arg(inner, i)?)?;
    match &v.kind {
        ObjectKind::Number(num) => Ok((num.float(), num.is_float())),
        _ => Err(YacasError::InvalidArg),
    }
}
/// Variant that ignores the type flag (for commands whose results are always floats).
pub(super) fn arg_float(
    env: &mut Environment,
    inner: &Rc<LispObject>,
    i: usize,
) -> Result<crate::number::float::Float, YacasError> {
    Ok(arg_float_flag(env, inner, i)?.0)
}
pub(super) fn num_of(f: crate::number::float::Float) -> Rc<LispObject> {
    num_of_flag(f, true)
}
pub(super) fn num_of_flag(f: crate::number::float::Float, is_float: bool) -> Rc<LispObject> {
    Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Number(crate::value::LispNumber::from_float_flag(f, is_float)),
    })
}

pub(super) fn map_numeric_work_error(error: crate::number::limits::NumericWorkError) -> YacasError {
    match error {
        crate::number::limits::NumericWorkError::Interrupted => YacasError::UserInterrupt,
        crate::number::limits::NumericWorkError::Overflow => YacasError::NumericOverflow,
    }
}

/// Math core family (like upstream corefunctions.h; stubs/base.rep rule bodies
/// depend on these).
pub fn cmd_math_negate(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let (x, fl) = arg_float_flag(env, inner, 0)?;
    Ok(num_of_flag(x.negate(), fl))
}
pub fn cmd_math_abs(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let z = crate::number::float::Float::from_decimal("0").expect("0");
    let (x, fl) = arg_float_flag(env, inner, 0)?;
    Ok(num_of_flag(
        if x.less_than(&z) { x.negate() } else { x },
        fl,
    ))
}
pub fn cmd_math_sign(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let z = crate::number::float::Float::from_decimal("0").expect("0");
    let x = arg_float(env, inner, 0)?;
    let v = if x.is_zero() {
        0
    } else if x.less_than(&z) {
        -1
    } else {
        1
    };
    Ok(num_text(v.to_string()))
}
/// Number text atom (like LispNumber::from_text; builds integer results for sign/floor/ceil etc.).
pub(super) fn num_text(s: String) -> Rc<LispObject> {
    LispObject::new(ObjectKind::Number(crate::value::LispNumber::from_text(s)))
}

// ==================== FastIsPrime / MathFac ====================
/// FastIsPrime (Function|Fixed): small-prime table lookup (upstream
/// primes_table_check, MAX_SMALL_PRIME = 65537). Semantics: p == 0 -> 65537;
/// 2 -> 1; < 2, > 65537, or even -> 0; odd table lookup -> 1/0.
/// numbers.rep's IsSmallPrime / IsPrime(n<=FastIsPrime(0)) depend on this.
/// See upstream: cyacas/libyacas/src/mathcommands3.cpp LispFastIsPrime.
pub fn cmd_fast_is_prime(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let p: u64 = int_text_of(&v)? as u64;
    let r = fast_is_prime_check(p);
    Ok(num_text(r.to_string()))
}

/// Port of upstream primes_table_check (platmath.cpp). Sieve of Eratosthenes
/// computed once, cached in a thread_local.
fn fast_is_prime_check(p: u64) -> u64 {
    const MAX: u64 = 65537;
    if p == 0 {
        return MAX;
    }
    if p == 2 {
        return 1;
    }
    if !(2..=MAX).contains(&p) || (p & 1) == 0 {
        return 0;
    }
    use std::cell::RefCell;
    thread_local! {
        // Odd composite table (like upstream bitset<MAX/2+1>: index p/2, p odd)
        static COMP: RefCell<Option<Vec<bool>>> = const { RefCell::new(None) };
    }
    COMP.with(|c| {
        let mut guard = c.borrow_mut();
        let tbl = guard.get_or_insert_with(|| {
            let half = (MAX / 2 + 1) as usize;
            let mut comp = vec![false; half];
            let mut i: u64 = 3;
            while i < MAX {
                if !comp[(i / 2) as usize] {
                    let mut j: u64 = 3;
                    while j < MAX / i {
                        comp[((i * j) / 2) as usize] = true;
                        j += 2;
                    }
                }
                i += 2;
            }
            comp
        });
        if tbl[(p / 2) as usize] {
            0
        } else {
            1
        }
    })
}

/// MathFloor/MathCeil (like upstream LispFloor/LispCeil): via Float.floor/ceil.
/// stubs' Floor/Ceil/Round rule bodies and the IsZero chain (MathFloor(N(x+0.5))
/// etc.) depend on these.
pub fn cmd_math_floor(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    // Upstream behavior: Floor/MathFloor always return an integer-typed
    // result (Floor(3.5) -> 3, no decimal point)
    Ok(num_of_flag(arg_float(env, inner, 0)?.floor(), false))
}
pub fn cmd_math_ceil(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    Ok(num_of_flag(arg_float(env, inner, 0)?.ceil(), false))
}

/// Builtin'Precision'Get/Set (like upstream corefunctions.h: global decimal
/// precision). predicates' IsZero body `MathPower(10,-Builtin'Precision'Get())`
/// and the base.rep/math.ys + stdfuncs MathSqrtFloat chain depend on these.
pub fn cmd_precision_get(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    Ok(num_text(env.precision.to_string()))
}
pub fn cmd_precision_set(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let n = int_text_of(&eval(env, arg(inner, 0)?)?)?;
    if n < 1 {
        return Err(YacasError::InvalidArg);
    }
    let precision = u32::try_from(n).map_err(|_| YacasError::NumericOverflow)?;
    if precision > crate::number::limits::MAX_DECIMAL_WORK_DIGITS {
        return Err(YacasError::NumericOverflow);
    }
    env.precision = precision;
    Ok(env.true_atom())
}

/// MathBitCount (like upstream LispBitCount): binary bit length of a positive integer.
pub(super) fn cmd_math_bit_count(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let number = match &v.kind {
        ObjectKind::Number(number) => number,
        _ => return Err(YacasError::InvalidArg),
    };
    // Like upstream LispBitCount(x->BitCount()): bit length of the absolute
    // integer part (2.5 -> 2, 0.5 -> 0, 2^100 -> 101, -5 -> 3). Work from
    // Float's represented value so a decimal exponent is not discarded.
    let bits = number
        .float_at(0)
        .integer_bit_len()
        .ok_or(YacasError::NumericOverflow)?;
    Ok(int_number(bits as i64))
}

/// MathGetExactBits: integer -> BitCount (binary bit length); float ->
/// ceil(stored mantissa digits * log2(10)). The stored form is the literal
/// padded at *creation-time* precision (float_at pad), independent of the
/// current precision — the MathSqrtFloat chain computes targetbits after
/// Builtin'Precision'Set(10), so using stored digits (not the current
/// precision) keeps targetbits stable (e.g. GetExactBits(2.) is 34 at
/// precision 10 and 84 at precision 25).
/// See upstream: cyacas/libyacas/src/mathcommands3.cpp LispGetExactBits.
pub(super) fn cmd_math_get_exact_bits(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let n = match &v.kind {
        ObjectKind::Number(n) => n,
        _ => return Err(YacasError::InvalidArg),
    };
    let t = n.string();
    if n.is_float() || t.contains('.') || t.contains('e') || t.contains('E') {
        // Stored digits = max(session precision, all literal digits including
        // trailing zeros after the point; ".0"'s 0 counts); bits =
        // ceil(digits * log2(10)).
        let digits = n.digit_count_storage(env.precision());
        let bits = (digits as f64 * (10f64).log2()).ceil() as i64;
        Ok(int_number(bits))
    } else {
        let neg = t.starts_with('-');
        let digits = if neg { &t[1..] } else { t.as_str() };
        let big = crate::number::nat::Nat::from_decimal(digits).ok_or(YacasError::InvalidArg)?;
        let bits = nat_bit_len(&big);
        Ok(int_number(bits as i64))
    }
}

/// MathSetExactBits: float -> cap the decimal fraction digits at
/// floor(bits * log10(2)), rounding half-carry to cap+1 digits then dropping
/// the last (the carry can chain into the integer part, 9.99 -> 10.), trailing
/// zeros trimmed. Integers pass through unchanged. Inverse of GetExactBits'
/// ceil(digits * log2(10)).
/// See upstream: cyacas/libyacas/src/mathcommands3.cpp LispSetExactBits.
pub(super) fn cmd_math_set_exact_bits(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let bits = int_text_of(&eval(env, arg(inner, 1)?)?)?;
    let v = eval(env, arg(inner, 0)?)?;
    match &v.kind {
        ObjectKind::Number(n) if n.is_float() => {
            if bits > crate::number::limits::MAX_BINARY_WORK_BITS as i64 {
                return Err(YacasError::NumericOverflow);
            }
            let frac = ((bits.max(0) as f64) * std::f64::consts::LOG10_2).floor();
            let frac = if frac < 0.0 { 0u32 } else { frac as u32 };
            // float_at(0): unpadded stored form (padding would count trailing
            // zeros as significant and over-trim)
            let f = n.float_at(0).round_to_frac(frac);
            Ok(num_of_flag(f, true))
        }
        _ => Ok(v), // integer type: unchanged
    }
}

/// Binary bit length of a Nat (like upstream BitCount; via Nat::bit_len).
pub(super) fn nat_bit_len(n: &crate::number::nat::Nat) -> u64 {
    n.bit_len()
}

/// MathMul2Exp: multiplies by 2^n exactly (like the upstream kernel's shift
/// semantics; the script-level division version in base.rep/math.ys would
/// truncate 123456.789/2^16 to 9 digits at the working precision, making the
/// later sqrt chain converge to the truncated a0; the kernel's *2^n is always
/// exact — see Float::mul2exp).
pub(super) fn cmd_math_mul2_exp(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let (x, fl) = arg_float_flag(env, inner, 0)?;
    let n = int_text_of(&eval(env, arg(inner, 1)?)?)?;
    Ok(num_of_flag(
        x.mul2exp_with_limits(n, || env.check_eval_deadline().is_err())
            .map_err(map_numeric_work_error)?,
        fl,
    ))
}

/// DigitsToBits/BitsToDigits: decimal digit count <-> bit count conversions.
pub(super) fn cmd_digits_to_bits(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let d = int_text_of(&eval(env, arg(inner, 0)?)?)?;
    let base = int_text_of(&eval(env, arg(inner, 1)?)?)?;
    if d < 0 || !(2..=32).contains(&base) {
        return Err(YacasError::InvalidArg);
    }
    // Like upstream numbers.cpp: ceil(digits * log2(base))
    let v = (d as f64 * (base as f64).log2()).ceil() as i64;
    Ok(int_number(v))
}
pub(super) fn cmd_bits_to_digits(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let b = int_text_of(&eval(env, arg(inner, 0)?)?)?;
    let base = int_text_of(&eval(env, arg(inner, 1)?)?)?;
    if b < 0 || !(2..=32).contains(&base) {
        return Err(YacasError::InvalidArg);
    }
    // Like upstream numbers.cpp: floor(bits / log2(base))
    let v = (b as f64 / (base as f64).log2()).floor() as i64;
    Ok(int_number(v))
}

/// FastPower (like upstream LispFastPower): floating-point fast power.
/// Integer/negative-integer exponents use fast exponentiation (negative ->
/// reciprocal); non-integer exponents raise InvalidArg since the engine has
/// no exp/ln.
/// See upstream: cyacas/libyacas/src/corefunctions.h LispFastPower.
pub(super) fn cmd_fast_power(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let (x, fl) = arg_float_flag(env, inner, 0)?;
    let n = int_text_of(&eval(env, arg(inner, 1)?)?)?;
    if n.unsigned_abs() > crate::number::limits::MAX_BINARY_WORK_BITS {
        return Err(YacasError::NumericOverflow);
    }
    let one = crate::number::float::Float::from_decimal("1").expect("1");
    let zero = crate::number::float::Float::from_decimal("0").expect("0");
    // Negative exponent: like upstream MathIntPower = MathDivide(1,
    // PositiveIntPower(x,-n)) — i.e. the base becomes the reciprocal 1/x.
    // (Using x/1 instead of 1/x would not invert and would break negative
    // exponents and the downstream IsZero/GreaterThan predicates.)
    let base = if n < 0 {
        one.div_with_limits(&x, env.precision, || env.check_eval_deadline().is_err())
            .map_err(map_numeric_work_error)?
            .unwrap_or(zero.clone())
    } else {
        x
    };
    let mut acc = one;
    let mut b = base;
    let mut k = n.unsigned_abs();
    while k > 0 {
        if k & 1 == 1 {
            acc = acc
                .mul_with_limits(&b, env.precision, || env.check_eval_deadline().is_err())
                .map_err(map_numeric_work_error)?;
        }
        k >>= 1;
        if k > 0 {
            b = b
                .mul_with_limits(&b, env.precision, || env.check_eval_deadline().is_err())
                .map_err(map_numeric_work_error)?;
        }
    }
    Ok(num_of_flag(acc, fl))
}

/// FastLog (like upstream LispFastLog = std::log): natural logarithm. Upstream
/// uses the platform double log; the Rust decimal Float model has no exact log
/// — approximation: text -> f64 -> ln -> back to Float (precise enough for
/// MathFloor(FastLog(x)/FastLog(10)) decisions; FloatIsInt's digit-count
/// estimation depends on this). Non-positive argument -> InvalidArg (log
/// domain).
pub(super) fn cmd_fast_log(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = match &v.kind {
        ObjectKind::Number(n) => n.string(),
        _ => return Err(YacasError::InvalidArg),
    };
    let x: f64 = s.parse().map_err(|_| YacasError::InvalidArg)?;
    if x <= 0.0 {
        return Err(YacasError::InvalidArg);
    }
    let l = x.ln();
    let f = crate::number::float::Float::from_decimal(&format!("{l:.17}"))
        .ok_or(YacasError::InvalidArg)?;
    Ok(num_of(f))
}

/// Platform unary functions (like upstream's PLATFORM_UNARY macro: platform
/// double function, converted back to decimal). The Rust decimal Float has no
/// high-precision transcendentals — f64 approximation, sufficient for
/// downstream decisions.
fn platform_unary<F: Fn(f64) -> f64>(
    env: &mut Environment,
    inner: &Rc<LispObject>,
    f: F,
    domain_ok: fn(f64) -> bool,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = match &v.kind {
        ObjectKind::Number(n) => n.string(),
        _ => return Err(YacasError::InvalidArg),
    };
    let x: f64 = s.parse().map_err(|_| YacasError::InvalidArg)?;
    if !domain_ok(x) {
        return Err(YacasError::InvalidArg);
    }
    let y = f(x);
    let r = crate::number::float::Float::from_decimal(&format!("{y:.17}"))
        .ok_or(YacasError::InvalidArg)?;
    Ok(num_of(r))
}

/// FastArcSin (like upstream LispFastArcSin = std::asin; numerical.ys's
/// ArcSinNum -> MathArcSin (Taylor) uses it as an initial value).
/// See upstream: cyacas/libyacas/src/mathcommands3.cpp LispFastArcSin.
pub(super) fn cmd_fast_arc_sin(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    platform_unary(env, inner, f64::asin, |x| (-1.0..=1.0).contains(&x))
}
