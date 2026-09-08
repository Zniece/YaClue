//! Base conversion commands and digit encoding helpers.

use std::rc::Rc;

use super::integer::divrem_with_deadline;
use super::numeric::{map_numeric_work_error, num_of_flag};
use super::{arg, arity_of};
use crate::env::Environment;
use crate::errors::YacasError;
use crate::evaluator::eval;
use crate::value::{LispObject, ObjectKind};

/// Base-b digit character -> value (like DigitIndex; '0'-'9', 'a'-'z', 'A'-'Z';
/// out of base -> None).
fn base_digit(c: char, base: i64) -> Option<i64> {
    let v = match c {
        '0'..='9' => c as i64 - '0' as i64,
        'a'..='z' => c as i64 - 'a' as i64 + 10,
        'A'..='Z' => c as i64 - 'A' as i64 + 10,
        _ => return None,
    };
    if v < base {
        Some(v)
    } else {
        None
    }
}

/// Nat -> base-b digit string (lowercase 0-9a-z; like ZZ::to_string(base); zero -> "0").
fn nat_to_base(
    env: &Environment,
    n: &crate::number::nat::Nat,
    base: u32,
) -> Result<String, YacasError> {
    if n.is_zero() {
        return Ok("0".into());
    }
    let b = crate::number::nat::Nat::from_decimal(&base.to_string()).expect("base");
    let mut ds: Vec<u8> = Vec::new();
    let mut cur = n.clone();
    while !cur.is_zero() {
        if ds.len() & 0xff == 0 {
            env.check_eval_deadline()?;
        }
        let (q, r) = cur.divrem(&b).expect("div by nonzero");
        let d: u32 = r.to_decimal().parse().expect("r < base");
        ds.push(if d < 10 {
            b'0' + d as u8
        } else {
            b'a' + (d - 10) as u8
        });
        cur = q;
    }
    ds.reverse();
    Ok(String::from_utf8(ds).expect("base digits"))
}

/// Base-b digit string -> Nat (FromBase mantissa parsing; out-of-base digit -> None).
fn nat_from_base(
    env: &Environment,
    s: &str,
    base: i64,
) -> Result<crate::number::nat::Nat, YacasError> {
    let b =
        crate::number::nat::Nat::from_decimal(&base.to_string()).ok_or(YacasError::InvalidArg)?;
    let mut acc = crate::number::nat::Nat::zero();
    for (index, c) in s.chars().enumerate() {
        if index & 0xff == 0 {
            env.check_eval_deadline()?;
        }
        let d = base_digit(c, base).ok_or(YacasError::InvalidArg)?;
        let digit =
            crate::number::nat::Nat::from_decimal(&d.to_string()).ok_or(YacasError::InvalidArg)?;
        acc = acc.mul(&b).add(&digit);
    }
    Ok(acc)
}

/// FromBase (see upstream: cyacas/libyacas/src/mathcommands3.cpp LispFromBase
/// and cyacas/libyacas/src/anumber.cpp ANumber::SetTo): base integer 2..=32
/// (log2_table_range); the second argument must be a string.
/// The digit string is parsed in the given base: integers -> exact integer;
/// with '.' or (when base < 14) 'e'/'E' -> float:
/// value = mantissa × b^(-number of fraction digits), with the e exponent
/// applied via decimal atoi (same as SetTo; FromBase(2,"1.1e2") = 150 = 0.15e3).
/// The 'E' quirk of cyacas at base >= 11 (1E5 -> 0) is not replicated; it is
/// treated as a mantissa digit (consistent with FromBase(16,"1e5") = 485).
/// Precision conversion (like CalculatePrecision): bits = ceil(max(BinaryPrecision,
/// significant digits) × log2(base)), decimal digits = floor(bits × log10 2)
/// (FromBase(3,"0.1") -> 16 digits).
pub fn cmd_from_base(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if env.precision() > crate::number::limits::MAX_DECIMAL_WORK_DIGITS {
        return Err(YacasError::NumericOverflow);
    }
    let bv = eval(env, arg(inner, 0)?)?;
    let base = match &bv.kind {
        ObjectKind::Number(n) if !n.is_float() => n
            .string()
            .parse::<i64>()
            .map_err(|_| YacasError::InvalidArg)?,
        _ => return Err(YacasError::InvalidArg),
    };
    if !(2..=32).contains(&base) {
        return Err(YacasError::InvalidArg);
    }
    let sv = eval(env, arg(inner, 1)?)?;
    let st = sv.atom_string().ok_or(YacasError::InvalidArg)?;
    let s = crate::standard::internal_unstringify(st).unwrap_or(st);
    if s.len() > crate::number::limits::MAX_DECIMAL_WORK_DIGITS as usize + 32 {
        return Err(YacasError::NumericOverflow);
    }

    let (neg, rest) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s),
    };
    let chars: Vec<char> = rest.chars().collect();
    // Locate the '.' and e markers (like SetTo: lowercase 'e' only when
    // base < 14; for base >= 14, e/E are digits)
    let mut dot: Option<usize> = None;
    let mut emark: Option<usize> = None;
    for (i, &c) in chars.iter().enumerate() {
        if c == '.' {
            dot = Some(i);
        }
        if base < 14 && (c == 'e' || c == 'E') {
            emark = Some(i);
        }
    }
    let mant_end = emark.unwrap_or(chars.len());
    // Significant digits (like CalculatePrecision: skip leading '.', '-', '0'
    // -> digit1; sig = mant_end − digit1; -1 if a '.' appears after digit1.
    // Trailing zeros / the leading segment all count, per the C++ comment)
    let digit1 = chars
        .iter()
        .take(mant_end)
        .position(|&c| c != '.' && c != '0')
        .unwrap_or(mant_end);
    let mut sig = (mant_end - digit1) as i64;
    if chars[digit1..mant_end].contains(&'.') {
        sig -= 1;
    }
    if sig > crate::number::limits::MAX_DECIMAL_WORK_DIGITS as i64 {
        return Err(YacasError::NumericOverflow);
    }
    let (int_part, frac_part) = match dot {
        Some(d) if d < mant_end => {
            let ip: String = chars[..d].iter().collect();
            let fp: String = chars[d + 1..mant_end].iter().collect();
            (ip, fp)
        }
        _ => (chars[..mant_end].iter().collect::<String>(), String::new()),
    };
    // e exponent: decimal atoi (like SetTo; optional '+')
    let te: i64 = match emark {
        Some(e) => {
            let mut t: String = chars[e + 1..].iter().collect();
            if let Some(r) = t.strip_prefix('+') {
                t = r.to_string();
            }
            let dg: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
            dg.parse().unwrap_or(0)
        }
        None => 0,
    };
    let is_float = dot.is_some() || emark.is_some();
    let mant_str = format!("{int_part}{frac_part}");
    let m = nat_from_base(env, &mant_str, base)?;
    if !is_float {
        let mut txt = m.to_decimal();
        if txt == "0" {
            txt = "0".into();
        }
        if neg && txt != "0" {
            txt = format!("-{txt}");
        }
        return Ok(Rc::new(LispObject {
            next: None,
            kind: ObjectKind::Number(crate::value::LispNumber::from_text(txt)),
        }));
    }
    // Float: value = M × 10^te / b^k; precision bits = ceil(max(BinaryPrec, sig)×log2 b),
    // decimal digits = floor(bits×log10 2); t = floor(M×10^dec/b^k) (round-half-up).
    let bits_env = (env.precision() as f64 * 10f64.log2()).ceil() as i64;
    let bits = ((bits_env.max(sig)) as f64 * (base as f64).log2()).ceil() as i64;
    let dec = (((bits as f64) * 2f64.log10()).floor().max(1.0)) as u32;
    let bnat = crate::number::nat::Nat::from_decimal(&base.to_string()).expect("base");
    let denom = bnat
        .pow_with_limits(frac_part.len() as u32, || {
            env.check_eval_deadline().is_err()
        })
        .map_err(map_numeric_work_error)?;
    let scaled = m.mul_pow10(dec);
    let (q, r) = divrem_with_deadline(env, &scaled, &denom)?;
    let two = crate::number::nat::Nat::from_decimal("2").expect("two");
    let q = if r.mul(&two).cmp(&denom) != std::cmp::Ordering::Less {
        q.add(&crate::number::nat::Nat::from_decimal("1").expect("one"))
    } else {
        q
    };
    let f = crate::number::float::Float::from_parts_trimmed(q, dec, te, dec, neg);
    Ok(num_of_flag(f, true))
}

/// ToBase (see upstream: cyacas/libyacas/src/mathcommands3.cpp LispToBase and
/// cyacas/libyacas/src/anumber.cpp ANumberToString): base 2..=32. Integers ->
/// plain base-b string (lowercase); floats -> only the **mantissa** is
/// converted (digits×10^-scale, te is not folded in): integer part as a base-b
/// string + fraction digits via repeated multiplication by b for
/// BinaryPrecision+1 digits (last digit >= base/2 rounds up, possibly
/// overflowing into the integer part), trailing fraction zeros trimmed;
/// floats always carry '.' (integral values "X.", zero special case "0");
/// te != 0 appends "e<te>" (e.g. ToBase(2, 0.1e21-like) -> "0.0001100…e21").
pub fn cmd_to_base(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let bv = eval(env, arg(inner, 0)?)?;
    let base = match &bv.kind {
        ObjectKind::Number(n) if !n.is_float() => n
            .string()
            .parse::<i64>()
            .map_err(|_| YacasError::InvalidArg)?,
        _ => return Err(YacasError::InvalidArg),
    };
    if !(2..=32).contains(&base) {
        return Err(YacasError::InvalidArg);
    }
    if env.precision() > crate::number::limits::MAX_DECIMAL_WORK_DIGITS {
        return Err(YacasError::NumericOverflow);
    }
    let nv = eval(env, arg(inner, 1)?)?;
    let n = match &nv.kind {
        ObjectKind::Number(n) => n,
        _ => return Err(YacasError::InvalidArg),
    };
    let b32 = base as u32;
    if !n.is_float() {
        let t = n.string();
        let (neg, mag) = match t.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, t.as_str()),
        };
        if mag.len() > crate::number::limits::MAX_DECIMAL_WORK_DIGITS as usize {
            return Err(YacasError::NumericOverflow);
        }
        let m = crate::number::nat::Nat::from_decimal(mag).ok_or(YacasError::InvalidArg)?;
        let mut s = nat_to_base(env, &m, b32)?;
        if neg && s != "0" {
            s = format!("-{s}");
        }
        return Ok(crate::value::make_atom(
            &mut env.symtab,
            &format!("\"{s}\""),
        ));
    }
    // Float: mantissa digits×10^-scale conversion (te is attached separately
    // as the e suffix)
    let f = n.float();
    let scale = f.frac_scale();
    if scale > crate::number::limits::MAX_DECIMAL_WORK_DIGITS
        || f.digit_count() > crate::number::limits::MAX_DECIMAL_WORK_DIGITS
    {
        return Err(YacasError::NumericOverflow);
    }
    let den = crate::number::nat::Nat::from_decimal("1")
        .expect("one")
        .mul_pow10(scale);
    let (int_part, mut rem) = divrem_with_deadline(env, f.digits_nat(), &den)?;
    let mut int_s = nat_to_base(env, &int_part, b32)?;
    // Fraction: repeated multiplication by b, BinaryPrecision+1 digits (guard digit)
    let bits = ((env.precision() as f64) * 10f64.log2()).ceil() as u32;
    let bb = crate::number::nat::Nat::from_decimal(&base.to_string()).expect("base");
    let mut digs: Vec<u32> = Vec::new();
    for index in 0..(bits + 1) {
        if index & 0xff == 0 {
            env.check_eval_deadline()?;
        }
        rem = rem.mul(&bb);
        let (d, r2) = rem.divrem(&den).ok_or(YacasError::InvalidArg)?;
        digs.push(if d.is_zero() {
            0
        } else {
            d.to_decimal().parse().unwrap_or(0)
        });
        rem = r2;
    }
    // Guard-digit carry (as in C++ anumber.cpp ANumberToString: the chain
    // starts by incrementing the guard digit itself, and the guard digit is
    // then dropped — while the guard < base-1, the change disappears with the
    // dropped guard digit, a quirk of the vendored code: ToBase(3,0.5) has
    // guard 1 -> no carry -> 34 ones; ToBase(3,1/3) has guard 2 = base-1 ->
    // the chain of zeros overflows into the integer part -> "0.1")
    if *digs.last().expect("digs") as i64 >= (base >> 1) {
        let mut carry = 1i64;
        for d in digs.iter_mut().rev() {
            let w = *d as i64 + carry;
            *d = (w % base) as u32;
            carry = w / base;
            if carry == 0 {
                break;
            }
        }
        if carry > 0 {
            int_part_nat_inc(&mut int_s, b32);
        }
    }
    digs.pop();
    // Trim trailing fraction zeros (like ANumberToString; "0." special case -> "0")
    while digs.last() == Some(&0) {
        digs.pop();
    }
    let frac_s: String = digs.iter().map(|&d| base_char(d)).collect();
    let mut out = if frac_s.is_empty() {
        if int_s == "0" {
            "0".to_string()
        } else {
            format!("{int_s}.")
        }
    } else {
        format!("{int_s}.{frac_s}")
    };
    let te = f.tens_exp_of();
    if te != 0 && out != "0" {
        out = format!("{out}e{te}");
    }
    if f.is_neg() && out != "0" {
        out = format!("-{out}");
    }
    Ok(crate::value::make_atom(
        &mut env.symtab,
        &format!("\"{out}\""),
    ))
}

fn base_char(d: u32) -> char {
    if d < 10 {
        (b'0' + d as u8) as char
    } else {
        (b'a' + (d - 10) as u8) as char
    }
}

/// Add 1 with carry to a base-b integer string (rounding overflow into the integer part).
fn int_part_nat_inc(s: &mut String, base: u32) {
    let mut ds: Vec<u32> = s
        .chars()
        .map(|c| base_digit(c, base as i64).unwrap_or(0) as u32)
        .collect();
    let mut carry = 1i64;
    for d in ds.iter_mut().rev() {
        let w = *d as i64 + carry;
        *d = (w % base as i64) as u32;
        carry = w / base as i64;
        if carry == 0 {
            break;
        }
    }
    if carry > 0 {
        ds.insert(0, carry as u32);
    }
    *s = ds.iter().map(|&d| base_char(d)).collect();
}
