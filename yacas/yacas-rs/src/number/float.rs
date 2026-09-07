//! Float: value = `digits × 10^(tens_exp − scale)` with a decimal requested
//! precision `prec` (contract §2).
//!
//! Key semantic decisions (verified against the upstream engine):
//! - Literals pass through: an unoperated number prints its original text.
//! - Derived results print canonically, truncating (no carry-in) to `prec`
//!   fraction digits; `prec == 0` means "storage form" and is never
//!   truncated to session precision on output.
//! - Zero results normalize to `(digits=0, scale=0, tens_exp=0)`: zero has
//!   no exponent, and a stale nonzero exponent poisons later add/div
//!   alignment.
//! - A product below 0.1 stays in plain form; only division results
//!   normalize into e-form.

use super::nat::Nat;

/// Default decimal precision (the engine's global precision).
pub const DEFAULT_PREC: u32 = 25;

#[derive(Debug, Clone)]
pub struct Float {
    digits: Nat,     // integral digits (leading zeros trimmed; empty = 0)
    scale: u32,      // number of fractional digits (≥ 0)
    tens_exp: i32,   // tens exponent (te==0 prints verbatim, te!=0 e-form)
    prec: u32,       // requested decimal precision (drives guard truncation)
    neg: bool,
    text: Option<String>, // original literal text (passthrough printing)
}

impl Float {
    /// Parse a decimal literal (optional sign, decimal point, e/E exponent)
    /// at requested precision `prec`. The original text is kept for
    /// passthrough printing.
    pub fn from_decimal_with_prec(s: &str, prec: u32) -> Option<Self> {
        let orig = s.trim().to_string();
        let t = orig.as_str();
        let neg = t.starts_with('-');
        let mant = t
            .strip_prefix('-')
            .or_else(|| t.strip_prefix('+'))
            .unwrap_or(t);
        let (mant, te) = match mant.find(['e', 'E']) {
            Some(i) => (&mant[..i], mant[i + 1..].parse::<i32>().ok()?),
            None => (mant, 0),
        };
        let (ip, fp) = match mant.find('.') {
            Some(i) => (&mant[..i], &mant[i + 1..]),
            None => (mant, ""),
        };
        if !ip.bytes().all(|b| b.is_ascii_digit()) || !fp.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let digits_s = format!("{ip}{fp}");
        let digits = Nat::from_decimal(if digits_s.is_empty() { "0" } else { &digits_s })?;
        Some(Float {
            digits,
            scale: fp.len() as u32,
            tens_exp: te,
            prec,
            neg,
            text: Some(orig),
        })
    }

    /// Parse a decimal literal at default precision.
    pub fn from_decimal(s: &str) -> Option<Self> {
        Self::from_decimal_with_prec(s, DEFAULT_PREC)
    }

    /// Raw constructor for exact core commands (e.g. the 2^n/5^m shifts of
    /// `MathMul2Exp`); value = digits × 10^(te − scale).
    pub fn from_parts(digits: Nat, scale: u32, tens_exp: i32, prec: u32, neg: bool) -> Self {
        Float { digits, scale, tens_exp, prec, neg, text: None }
    }

    /// ×2^n shift (always exact): n ≥ 0 multiplies the digits by 2^n;
    /// n < 0 divides by 2^m = ×5^m/10^m, which terminates exactly in
    /// decimal.
    pub fn mul2exp(&self, n: i64) -> Float {
        if self.digits.is_zero() {
            return Float {
                digits: self.digits.clone(),
                scale: 0,
                tens_exp: 0,
                prec: self.prec,
                neg: self.neg,
                text: None,
            };
        }
        if n >= 0 {
            let two_n = Nat::from_decimal("2").expect("2").pow(n as u32);
            Float {
                digits: self.digits.mul(&two_n),
                scale: self.scale,
                tens_exp: self.tens_exp,
                prec: self.prec,
                neg: self.neg,
                text: None,
            }
        } else {
            let m = (-n) as u32;
            let five_m = Nat::from_decimal("5").expect("5").pow(m);
            Float {
                digits: self.digits.mul(&five_m),
                scale: self.scale + m,
                tens_exp: self.tens_exp,
                prec: self.prec,
                neg: self.neg,
                text: None,
            }
        }
    }

    pub fn is_zero(&self) -> bool {
        self.digits.is_zero()
    }

    pub fn prec(&self) -> u32 {
        self.prec
    }

    /// Same value at a different requested precision, with the passthrough
    /// text cleared. Canonical printing truncates when `scale > prec`, so
    /// digits/scale need no change.
    pub fn with_prec(&self, prec: u32) -> Float {
        Float {
            digits: self.digits.clone(),
            scale: self.scale,
            tens_exp: self.tens_exp,
            prec,
            neg: self.neg,
            text: None,
        }
    }

    /// Zero-pad a literal's tail to `prec` digits (the storage form of an
    /// int→float conversion: `GetExactBits(2.)` reflects the padded digit
    /// count). Value is unchanged (×10^k with `scale` growing equally).
    /// Only literals are padded; computed results are not.
    pub fn pad_to_precision(&mut self, prec: u32) {
        if self.text.is_none() {
            return;
        }
        let have = self.digits.to_decimal().len() as i64;
        let add = prec as i64 - have;
        if add <= 0 || self.digits.is_zero() {
            return;
        }
        self.digits = self.digits.mul_pow10(add as u32);
        self.scale += add as u32;
    }

    /// Stored mantissa digit count (literals must go through
    /// `pad_to_precision` first).
    pub fn digit_count(&self) -> u32 {
        self.digits.to_decimal().len() as u32
    }

    /// Truncate (no carry-in) the fraction to at most `cap` digits; the
    /// value is unchanged (low digits dropped, `scale` lowered). Used by
    /// `MathSetExactBits`, which requires truncation, not rounding. A
    /// smaller scale is not padded (`SetExactBits(2., 37)` prints `"2."`).
    pub fn cut_to(&self, cap: u32) -> Float {
        if self.scale <= cap {
            return Float {
                digits: self.digits.clone(),
                scale: self.scale,
                tens_exp: self.tens_exp,
                prec: cap.max(self.prec),
                neg: self.neg,
                text: None,
            };
        }
        let (digits, scale) = truncate_down(self.digits.clone(), self.scale, cap);
        Float {
            digits,
            scale,
            tens_exp: self.tens_exp,
            prec: cap,
            neg: self.neg,
            text: None,
        }
    }

    /// Truncate the fraction to `frac` digits without carry-in (the
    /// `MathSetExactBits` behavior).
    pub fn round_to_frac(&self, frac: u32) -> Float {
        self.cut_to(frac)
    }

    /// Integer-value test: the value `digits × 10^(te − scale)` is integral
    /// even when internal `scale` has not been trimmed (e.g. `1.5×2` =
    /// Float(digits 30, scale 1), value 3.0 → true).
    pub fn is_integer(&self) -> bool {
        if self.digits.is_zero() {
            return true;
        }
        let e = self.tens_exp as i64 - self.scale as i64;
        if e >= 0 {
            return true;
        }
        let need = (-e) as usize;
        let ds = self.digits.to_decimal();
        ds.len() > need && ds[ds.len() - need..].bytes().all(|b| b == b'0')
    }

    pub fn format(&self) -> String {
        if let Some(t) = &self.text {
            return t.clone();
        }
        self.canonical()
    }

    /// Session-precision rendering: the digit count shown is the number's
    /// own `prec` (set at computation time to the then-session precision),
    /// independent of the precision at print time. Literals pass through;
    /// canonical output truncates when `prec > 0` and is left in full when
    /// `prec == 0` (storage form).
    pub fn format_at(&self, _session_prec: u32) -> String {
        self.format()
    }

    /// Canonical printing of a derived value (literals never reach here).
    /// `prec == 0` = storage form: no truncation, digit count as carried by
    /// the computation.
    fn canonical(&self) -> String {
        let mut digits = self.digits.clone();
        let mut scale = self.scale;
        trim(&mut digits, &mut scale);
        if digits.is_zero() {
            return "0".into();
        }
        // Truncate to the requested precision (drop low digits, no
        // carry-in; arithmetic carries guard digits which are dropped here).
        if self.prec > 0 && scale > self.prec {
            let (d, s) = truncate_down(digits, scale, self.prec);
            digits = d;
            scale = s;
        }
        trim(&mut digits, &mut scale);
        if digits.is_zero() {
            return "0".into();
        }
        let ds = digits.to_decimal();
        let sign = if self.neg { "-" } else { "" };
        if self.tens_exp == 0 {
            // Verbatim form: integer part + decimal point.
            if scale == 0 {
                return format!("{sign}{ds}");
            }
            let mut t = ds;
            while (t.len() as u32) <= scale {
                t.insert(0, '0');
            }
            let dot = t.len() as u32 - scale;
            let (ip, fp) = t.split_at(dot as usize);
            format!("{sign}{ip}.{fp}")
        } else {
            // e-form: mantissa in [0.1, 1), exponent = te − scale + digits.
            let len = ds.len() as i32;
            let e = self.tens_exp - scale as i32;
            format!("{sign}0.{ds}e{}", e + len)
        }
    }

    /// Negate (derived; clears passthrough text).
    pub fn negate(&self) -> Float {
        Float {
            neg: !self.neg,
            text: None,
            digits: self.digits.clone(),
            scale: self.scale,
            tens_exp: self.tens_exp,
            prec: self.prec,
        }
    }

    /// Absolute value.
    pub fn abs(&self) -> Float {
        if self.is_zero() {
            return Float {
                neg: false,
                text: None,
                digits: self.digits.clone(),
                scale: self.scale,
                tens_exp: self.tens_exp,
                prec: self.prec,
            };
        }
        Float {
            neg: false,
            text: None,
            digits: self.digits.clone(),
            scale: self.scale,
            tens_exp: self.tens_exp,
            prec: self.prec,
        }
    }

    /// Split the absolute value into (integer-part digits, fraction non-zero?).
    fn int_frac(&self) -> (Nat, bool) {
        let effective_exp = self.tens_exp - self.scale as i32;
        if effective_exp >= 0 {
            return (self.digits.mul_pow10(effective_exp as u32), false);
        }
        let p10 = Nat::from_decimal("1").unwrap().mul_pow10((-effective_exp) as u32);
        match self.digits.divrem(&p10) {
            Some((q, r)) => (q, !r.is_zero()),
            None => (Nat::zero(), !self.digits.is_zero()),
        }
    }

    /// Floor (toward −∞): the integer part, minus one more when a negative
    /// value has a fraction.
    pub fn floor(&self) -> Float {
        let (q, rnz) = self.int_frac();
        let val = if rnz && self.neg { q.add(&Nat::from_decimal("1").unwrap()) } else { q };
        Float {
            neg: self.neg && (rnz || !val.is_zero()),
            text: None,
            digits: val,
            scale: 0,
            tens_exp: 0,
            prec: self.prec,
        }
    }

    /// Ceil (toward +∞): the integer part, plus one more when a positive
    /// value has a fraction.
    pub fn ceil(&self) -> Float {
        let (q, rnz) = self.int_frac();
        let val = if rnz && !self.neg { q.add(&Nat::from_decimal("1").unwrap()) } else { q };
        Float {
            neg: self.neg && !val.is_zero(),
            text: None,
            digits: val,
            scale: 0,
            tens_exp: 0,
            prec: self.prec,
        }
    }

    /// Normalize the mantissa to `0.<significant digits> × 10^E`; returns
    /// (digits, digit count, tens_exp).
    fn mantle(&self) -> (Nat, u32, i32) {
        let ds = self.digits.to_decimal();
        if ds == "0" {
            return (Nat::zero(), 0, 0);
        }
        let trimmed = ds.trim_end_matches('0');
        let zeros = (ds.len() - trimmed.len()) as i32;
        let e = self.tens_exp - self.scale as i32 + zeros;
        let digs = Nat::from_decimal(if trimmed.is_empty() { "0" } else { trimmed }).unwrap();
        let len = trimmed.len() as u32;
        (digs, len, e + len as i32)
    }

    /// Normalize downward (when the value < 0.1): shift mantissa digits into
    /// the exponent so the e-form holds. Condition: value < 0.1 ⟺
    /// `te ≤ scale − len − 1`.
    fn normalize_down(&mut self) {
        if self.digits.is_zero() {
            return;
        }
        let l = self.digits.to_decimal().len() as i32;
        let te = self.tens_exp;
        let sc = self.scale as i32;
        if te < sc - l {
            let (d, s, t) = self.mantle();
            self.digits = d;
            self.scale = s;
            self.tens_exp = t;
        }
    }

    /// Division. Value = digits × 10^(te − scale); the quotient exponent is
    /// `E = (te_s − scale_s) − (te_o − scale_o)`. The computation stays on
    /// the internal digits/te/scale directly — folding te/scale into the
    /// mantissa first would lose magnitude information and turn decimal
    /// divisions into spurious e-forms.
    pub fn div(&self, o: &Float, prec: u32) -> Option<Float> {
        if o.digits.is_zero() {
            return None;
        }
        let e_s = self.tens_exp as i64 - self.scale as i64;
        let e_o = o.tens_exp as i64 - o.scale as i64;
        let e = e_s - e_o;
        // Exact division: quotient digits = self.digits/o.digits with
        // exponent e, keeping the dividend's full digit count (storage form,
        // not truncated to `prec`).
        let (dq, dr) = self.digits.divrem(&o.digits).expect("div: denominator is nonzero");
        if dr.is_zero() {
            // Quotient value = dq × 10^e, represented as digits × 10^(te−scale).
            // e ≥ 0 goes into te; e < 0 borrows into scale (te must stay
            // non-negative).
            if dq.is_zero() {
                return Some(Float {
                    digits: dq,
                    scale: 0,
                    tens_exp: 0,
                    prec: 0, // exact division = storage form
                    neg: self.neg != o.neg,
                    text: None,
                });
            }
            let mut f = Float {
                digits: dq,
                scale: 0,
                tens_exp: 0,
                prec: 0,
                neg: self.neg != o.neg,
                text: None,
            };
            if e >= 0 {
                f.tens_exp = e as i32;
            } else {
                f.scale = (-e) as u32;
            }
            f.normalize_down();
            return Some(f);
        }
        // Inexact division: the quotient's significant digits = requested
        // precision (long division continues with zero-padding to `prec`
        // significant digits even if it terminates early).
        // Shift choice: the quotient of num×10^k / den has ≈ k + s_s − s_o
        // significant digits, so k = prec + (s_o − s_s) when positive.
        let s_self = self.digits.to_decimal().len() as i32;
        let s_o = o.digits.to_decimal().len() as i32;
        let shift = (s_o - s_self).max(0);
        let k = prec as i32 + shift;
        // One extra digit as the rounding basis (half-carry); the final
        // quotient mantissa is cut back to `prec` significant digits.
        let q10 = div_scaled(&self.digits, &o.digits, (k + 1) as u32);
        // Zero quotient normalizes like the exact branch.
        if q10.is_zero() {
            return Some(Float {
                digits: q10,
                scale: 0,
                tens_exp: 0,
                prec,
                neg: self.neg != o.neg,
                text: None,
            });
        }
        // Value = q10 × 10^(e − k − 1) (one extra digit was divided out).
        let e_adj = e - k as i64 - 1;
        let mut f = Float {
            digits: q10,
            scale: 0,
            tens_exp: 0,
            prec,
            neg: self.neg != o.neg,
            text: None,
        };
        if e_adj >= 0 {
            f.tens_exp = e_adj as i32;
            f.scale = (k + 1) as u32;
        } else {
            // The scale carries the negative exponent (canonical truncates
            // to `prec` fraction digits when scale > prec).
            f.scale = (k as i64 + 1 - e) as u32;
        }
        f.normalize_down();
        // Align significant digits to `prec`: strip the padding zeros added
        // by the ÷10^k bookkeeping and the inflated scale. The extra divided
        // digit is dropped (quotients truncate, no carry-in).
        let q_len = f.digits.to_decimal().len() as i64;
        let mut td = f.digits.clone();
        let mut ts = f.scale;
        trim(&mut td, &mut ts);
        let dl = td.to_decimal().len() as i64;
        let z = q_len - dl; // trailing zeros of the padded quotient
        let val_exp = e - k as i64 - 1 + z + dl - 1; // leading digit = 10^val_exp
        if f.tens_exp == 0 && f.scale > prec {
            let lead_frac_zeros = (-val_exp - 1).max(0);
            let want_scale = (prec as i64 + lead_frac_zeros).min(f.scale as i64).max(0) as u32;
            if want_scale < f.scale {
                let (d, s) = truncate_down(f.digits.clone(), f.scale, want_scale);
                f.digits = d;
                f.scale = s;
            }
        } else if f.tens_exp != 0 {
            // e-form: cut the mantissa to `prec` digits, te unchanged.
            if dl > prec as i64 {
                let (d, s) = truncate_down(f.digits.clone(), f.scale, f.scale - (dl - prec as i64) as u32);
                f.digits = d;
                f.scale = s;
            }
        }
        f.prec = 0;
        Some(f)
    }

    /// Numeric equality: signs and aligned mantissas.
    pub fn equals(&self, o: &Float) -> bool {
        if self.neg != o.neg && !(self.is_zero() && o.is_zero()) {
            return false;
        }
        e_align(self, o, |a, b| a.cmp(b) == std::cmp::Ordering::Equal)
    }

    /// Numeric less-than.
    pub fn less_than(&self, o: &Float) -> bool {
        if self.is_zero() && o.is_zero() {
            return false;
        }
        if self.neg != o.neg {
            return self.neg; // the negative side is smaller
        }
        let lt = e_align(self, o, |a, b| a.cmp(b) == std::cmp::Ordering::Less);
        if self.neg {
            !lt && !self.equals(o)
        } else {
            lt
        }
    }

    /// Addition: align to the common tens_exp (max) and common scale (max),
    /// then merge by sign. The result precision = session `prec` (operand
    /// precisions only act as a *computation guard*; the output precision is
    /// not raised). A zero operand `prec` (storage form) is not a cap; with
    /// session `prec == 0` the operand max is used.
    pub fn add(&self, o: &Float, prec: u32) -> Float {
        use std::cmp::Ordering;
        let common = self.tens_exp.max(o.tens_exp);
        let s1 = self.scale as i64 + (common - self.tens_exp) as i64;
        let s2 = o.scale as i64 + (common - o.tens_exp) as i64;
        let sm = s1.max(s2);
        let d1 = self.digits.mul_pow10((sm - s1) as u32);
        let d2 = o.digits.mul_pow10((sm - s2) as u32);
        let (digits, neg) = match (self.neg, o.neg) {
            (false, false) => (d1.add(&d2), false),
            (true, true) => (d1.add(&d2), true),
            (false, true) => match d1.cmp(&d2) {
                Ordering::Less => (d2.sub(&d1).unwrap(), true),
                _ => (d1.sub(&d2).unwrap_or_else(Nat::zero), false),
            },
            (true, false) => match d1.cmp(&d2) {
                Ordering::Less => (d2.sub(&d1).unwrap(), false),
                _ => (d1.sub(&d2).unwrap_or_else(Nat::zero), true),
            },
        };
        let out_prec = if prec > 0 { prec } else { self.prec.max(o.prec) };
        // Computation guard: keep max(session, operand) digits internally so
        // low digits are not lost; the guard digits stay in storage and are
        // cut only when printing.
        let guard = out_prec.max(self.prec).max(o.prec);
        // Zero normalizes to (0, 0, 0).
        if digits.is_zero() {
            return Float {
                digits,
                scale: 0,
                tens_exp: 0,
                prec: out_prec,
                neg,
                text: None,
            };
        }
        let (digits, scale) = if sm as u32 > guard {
            truncate_down(digits, sm as u32, guard)
        } else {
            (digits, sm as u32)
        };
        Float {
            digits,
            scale,
            tens_exp: common,
            prec: out_prec,
            neg,
            text: None,
        }
    }

    /// Subtraction: add the negation.
    pub fn sub(&self, o: &Float, prec: u32) -> Float {
        self.add(&o.negate(), prec)
    }

    /// Multiplication: mantissas multiply, scale/tens_exp add; fraction
    /// digits beyond `guard + 1` truncate (guard digit). The result
    /// precision follows the same session/guard model as `add`. A product
    /// below 0.1 stays in plain form.
    pub fn mul(&self, o: &Float, prec: u32) -> Float {
        let out_prec = if prec > 0 { prec } else { self.prec.max(o.prec) };
        let digits = self.digits.mul(&o.digits);
        if digits.is_zero() {
            return Float {
                digits,
                scale: 0,
                tens_exp: 0,
                prec: out_prec,
                neg: self.neg != o.neg,
                text: None,
            };
        }
        let scale = self.scale + o.scale;
        let te = self.tens_exp + o.tens_exp;
        let guard = out_prec.max(self.prec).max(o.prec);
        let (digits, scale) = if scale > guard + 1 {
            truncate_down(digits, scale, guard + 1)
        } else {
            (digits, scale)
        };
        Float {
            digits,
            scale,
            tens_exp: te,
            prec: out_prec,
            neg: self.neg != o.neg,
            text: None,
        }
    }
}

/// Trim trailing fraction zeros (only while scale > 0; integer-side trailing
/// zeros stay).
fn trim(digits: &mut Nat, scale: &mut u32) {
    loop {
        if *scale == 0 || digits.is_zero() {
            break;
        }
        let ds = digits.to_decimal();
        if ds.ends_with('0') {
            *digits = Nat::from_decimal(&ds[..ds.len() - 1]).unwrap();
            *scale -= 1;
        } else {
            break;
        }
    }
}

/// Mantissa division: `floor(num / den × 10^scale)`.
fn div_scaled(num: &Nat, den: &Nat, scale: u32) -> Nat {
    let dividend = num.mul_pow10(scale);
    dividend.divrem(den).expect("div: denominator is nonzero").0
}

/// Compare after exponent alignment: both values convert to the common
/// minimum exponent, then the predicate applies to the mantissas.
fn e_align(a: &Float, b: &Float, cmp: impl Fn(&Nat, &Nat) -> bool) -> bool {
    // value = digs × 10^E with E = te − scale; align to min(E) so both
    // multipliers stay non-negative.
    let ea = a.tens_exp - a.scale as i32;
    let eb = b.tens_exp - b.scale as i32;
    let emin = ea.min(eb);
    let da = a.digits.mul_pow10((ea - emin) as u32);
    let db = b.digits.mul_pow10((eb - emin) as u32);
    cmp(&da, &db)
}

/// Truncating digit drop (no carry-in).
fn truncate_down(digits: Nat, scale: u32, cap: u32) -> (Nat, u32) {
    if scale <= cap {
        return (digits, scale);
    }
    let drop = scale - cap;
    let ds = digits.to_decimal();
    if ds.len() as u32 <= drop {
        return (Nat::zero(), cap);
    }
    let keep = &ds[..ds.len() - drop as usize];
    (Nat::from_decimal(keep).unwrap(), cap)
}

impl Float {
    /// Access to the mantissa digit string (read-only; used by ToBase's
    /// integer/fraction split).
    pub fn digits_nat(&self) -> &Nat {
        &self.digits
    }
    /// Fraction digit count.
    pub fn frac_scale(&self) -> u32 {
        self.scale
    }
    /// Tens exponent.
    pub fn tens_exp_of(&self) -> i32 {
        self.tens_exp
    }
    /// Sign flag.
    pub fn is_neg(&self) -> bool {
        self.neg
    }
    /// Construct with fraction-side trailing zeros trimmed (used by FromBase:
    /// `FromBase(10, "0.2222222222222222222")` stores 19 digits, not 34).
    pub fn from_parts_trimmed(digits: Nat, scale: u32, tens_exp: i32, prec: u32, neg: bool) -> Self {
        let mut f = Float::from_parts(digits, scale, tens_exp, prec, neg);
        if f.tens_exp == 0 {
            let mut d = f.digits.clone();
            let mut s = f.scale;
            trim(&mut d, &mut s);
            f.digits = d;
            f.scale = s;
        }
        f
    }
    /// Float dump for `MathDebugInfo`, matching the layout of the upstream
    /// `ANumber::Print` layout: a header line, then each 9-digit limb
    /// (high to low) as 32 binary bits in groups of 4; a `.` line marks the
    /// fraction-limb boundary.
    pub fn dump_debug(&self) -> String {
        let mut ds = self.digits.to_decimal();
        if ds.is_empty() {
            ds = "0".into();
        }
        let words = ds.len().div_ceil(9);
        let after = (self.scale as usize).div_ceil(9);
        let mut out = format!(
            "{words} words, {after} after point (x10^{}), 10-prec {}\n",
            self.tens_exp, self.prec
        );
        // Zero-pad to a whole number of limbs, 9 digits each (high first).
        let mut padded = String::new();
        for _ in 0..(words * 9 - ds.len()) {
            padded.push('0');
        }
        padded.push_str(&ds);
        let bytes = padded.as_bytes();
        for (i, chunk) in bytes.chunks(9).enumerate() {
            let w = std::str::from_utf8(chunk)
                .ok()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            if after > 0 && i + 1 == words - after {
                out.push_str(".\n");
            }
            let mut bit = 1u32 << 31;
            let mut k = 0;
            while bit != 0 {
                if k % 4 == 0 {
                    out.push(' ');
                }
                out.push(if w & bit != 0 { '1' } else { '0' });
                bit >>= 1;
                k += 1;
            }
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(s: &str) -> Float {
        Float::from_decimal(s).unwrap()
    }

    #[test]
    fn literal_text_pass() {
        // Unevaluated literals keep their original text.
        for s in ["0.000", "0.1e2", "12345.678", "1.5", "0.1", "0.0001"] {
            assert_eq!(f(s).format(), s, "{s}");
        }
    }

    #[test]
    fn add_golden() {
        assert_eq!(f("-6.23").add(&f("0.0"), DEFAULT_PREC).format(), "-6.23");
        assert_eq!(f("0.1").add(&f("0.0"), DEFAULT_PREC).format(), "0.1");
        assert_eq!(f("0.1").add(&f("-0.1"), DEFAULT_PREC).format(), "0");
        assert_eq!(f("2.5").add(&f("-2.5"), DEFAULT_PREC).format(), "0");
        assert_eq!(f("-6.23").add(&f("6.23"), DEFAULT_PREC).format(), "0");
        // A derived zero prints canonically (unlike the literal "0.000").
        assert_eq!(f("0.000").add(&f("0"), DEFAULT_PREC).format(), "0");
    }

    #[test]
    fn mul_golden() {
        assert_eq!(f("2.5").mul(&f("2.5"), DEFAULT_PREC).format(), "6.25");
    }

    #[test]
    fn sub_golden() {
        assert_eq!(f("6.25").sub(&f("6.25"), DEFAULT_PREC).format(), "0");
    }

    #[test]
    fn div_golden_layer2() {
        // Quotients carry the requested precision (guard truncation).
        assert_eq!(f("1").div(&f("3"), 20).unwrap().format(), "0.33333333333333333333");
        assert_eq!(f("1").div(&f("7"), 25).unwrap().format(), "0.1428571428571428571428571");
        assert_eq!(f("4").div(&f("3"), 5).unwrap().format(), "1.33333");
        assert_eq!(f("-1").div(&f("3"), 20).unwrap().format(), "-0.33333333333333333333");
        assert_eq!(f("1").div(&f("3"), 5).unwrap().format(), "0.33333");
        assert_eq!(f("1").div(&f("8"), 20).unwrap().format(), "0.125");
        assert_eq!(f("2").div(&f("3"), 10).unwrap().format(), "0.6666666666");
        assert_eq!(f("1").div(&f("6"), 10).unwrap().format(), "0.1666666666");
        // Any magnitude keeps precision: 1/1000 → 0.1e-2 (normalized).
        assert_eq!(f("1").div(&f("1000"), 25).unwrap().format(), "0.1e-2");
        // 0.1/3 → 0.333…e-1 (decimal divisor, truncated at request).
        assert_eq!(f("0.1").div(&f("3"), 10).unwrap().format(), "0.3333333333e-1");
        // Division by zero.
        assert!(f("1").div(&f("0"), 20).is_none());
    }

    #[test]
    fn mul_plain_under_one() {
        // A product below 0.1 stays plain; only division enters e-form.
        assert_eq!(f("0.1").mul(&f("0.1"), DEFAULT_PREC).format(), "0.01");
        assert_eq!(f("0.5").mul(&f("0.2"), DEFAULT_PREC).format(), "0.1");
    }

    #[test]
    fn compare_exp_align() {
        assert!(f("0.1e2").equals(&f("10")));
        assert!(f("0.5e-3").less_than(&f("0.001")));
        assert!(!f("0.5e-3").less_than(&f("0.5e-3")));
        assert!(f("-6.23").less_than(&f("6.23")));
        assert!(!f("6.23").less_than(&f("-6.23")));
    }

    #[test]
    fn canonical_plain() {
        assert_eq!(f("1.5").negate().format(), "-1.5");
        // Derived 1.500 canonicalizes (trailing zeros trimmed).
        assert_eq!(f("1.5").add(&f("0.0"), DEFAULT_PREC).format(), "1.5");
    }

    #[test]
    fn floor_and_ceil_respect_decimal_exponents() {
        let cases = [
            ("-0.9370247274e-1", "-1", "0"),
            ("0.9370247274e-1", "0", "1"),
            ("-12.75", "-13", "-12"),
            ("12.75", "12", "13"),
            ("1.25e2", "125", "125"),
            ("1.25e1", "12", "13"),
            ("-1.25e1", "-13", "-12"),
            ("7e3", "7000", "7000"),
            ("0", "0", "0"),
        ];
        for (input, floor, ceil) in cases {
            assert_eq!(f(input).floor().format(), floor, "floor({input})");
            assert_eq!(f(input).ceil().format(), ceil, "ceil({input})");
        }
    }
}
