//! Unsigned decimal big integers: little-endian base-10^9 limb vector.
//!
//! Invariant: no leading zero limbs (an empty vector is zero).

use std::fmt;

const BASE: u64 = 1_000_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nat {
    groups: Vec<u32>, // little-endian, each < BASE
}

impl Nat {
    pub fn zero() -> Self {
        Nat { groups: Vec::new() }
    }

    pub fn is_zero(&self) -> bool {
        // Empty or all-zero limbs both count as 0 (parsing/subtraction can
        // leave a single [0] limb behind).
        self.groups.iter().all(|&g| g == 0)
    }

    pub fn from_decimal(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let mut groups = Vec::new();
        let n = s.len();
        let mut i = n;
        while i > 0 {
            let start = i.saturating_sub(9);
            groups.push(s[start..i].parse::<u32>().ok()?);
            i = start;
        }
        while groups.last() == Some(&0) {
            groups.pop();
        }
        Some(Nat { groups })
    }

    pub fn to_decimal(&self) -> String {
        if self.groups.is_empty() {
            return "0".into();
        }
        let mut s = self.groups.last().unwrap().to_string();
        for g in self.groups[..self.groups.len() - 1].iter().rev() {
            s.push_str(&format!("{:09}", g));
        }
        s
    }

    pub fn add(&self, o: &Nat) -> Nat {
        let mut a = self.groups.clone();
        let b = &o.groups;
        let n = a.len().max(b.len());
        let mut carry: u64 = 0;
        for i in 0..n {
            let av = if i < a.len() { a[i] as u64 } else { 0 };
            let bv = if i < b.len() { b[i] as u64 } else { 0 };
            let s = av + bv + carry;
            if i < a.len() {
                a[i] = (s % BASE) as u32;
            } else {
                a.push((s % BASE) as u32);
            }
            carry = s / BASE;
        }
        if carry > 0 {
            a.push(carry as u32);
        }
        Nat { groups: a }
    }

    pub fn mul(&self, o: &Nat) -> Nat {
        self.mul_interruptible(o, || false)
            .expect("non-interruptible multiplication")
    }

    /// Schoolbook multiplication with a cancellation probe between limbs.
    pub(crate) fn mul_interruptible(
        &self,
        o: &Nat,
        mut interrupted: impl FnMut() -> bool,
    ) -> Result<Nat, ()> {
        if interrupted() {
            return Err(());
        }
        if self.is_zero() || o.is_zero() {
            return Ok(Nat::zero());
        }
        let mut res = vec![0u32; self.groups.len() + o.groups.len() + 1];
        for (i, &ai) in self.groups.iter().enumerate() {
            if interrupted() {
                return Err(());
            }
            let mut carry: u64 = 0;
            for (j, &bj) in o.groups.iter().enumerate() {
                let k = i + j;
                let cur = res[k] as u64 + ai as u64 * bj as u64 + carry;
                res[k] = (cur % BASE) as u32;
                carry = cur / BASE;
            }
            let mut k = i + o.groups.len();
            while carry > 0 {
                let cur = res[k] as u64 + carry;
                res[k] = (cur % BASE) as u32;
                carry = cur / BASE;
                k += 1;
            }
        }
        while res.last() == Some(&0) {
            res.pop();
        }
        Ok(Nat { groups: res })
    }

    /// Binary exponentiation: `base^exp` (exp ≥ 0).
    pub fn pow(&self, exp: u32) -> Nat {
        self.pow_impl(exp, false, || false)
            .expect("unbounded exponentiation")
    }

    pub(crate) fn pow_with_limits(
        &self,
        exp: u32,
        interrupted: impl FnMut() -> bool,
    ) -> Result<Nat, super::limits::NumericWorkError> {
        self.pow_impl(exp, true, interrupted)
    }

    fn pow_impl(
        &self,
        exp: u32,
        enforce_limit: bool,
        mut interrupted: impl FnMut() -> bool,
    ) -> Result<Nat, super::limits::NumericWorkError> {
        let mut e = exp;
        let mut base = self.clone();
        let mut acc = Nat::from_decimal("1").unwrap();
        while e > 0 {
            if interrupted() {
                return Err(super::limits::NumericWorkError::Interrupted);
            }
            if e & 1 == 1 {
                acc = pow_multiply(&acc, &base, enforce_limit, &mut interrupted)?;
            }
            e >>= 1;
            if e > 0 {
                base = pow_multiply(&base, &base, enforce_limit, &mut interrupted)?;
            }
        }
        Ok(acc)
    }

    /// Multiply by 10^k (append k zeros; k ≥ 0).
    pub fn mul_pow10(&self, k: u32) -> Nat {
        if self.is_zero() {
            return Nat::zero();
        }
        let s = self.to_decimal();
        let mut t = s;
        for _ in 0..k {
            t.push('0');
        }
        Nat::from_decimal(&t).unwrap()
    }

    /// Numeric magnitude comparison.
    // (Upstream-equivalent naming; intentionally not `Ord` - ordering here
    // is total for `Nat` but the type does not implement `PartialEq`/`Ord`.)
    #[allow(clippy::should_implement_trait)]
    pub fn cmp(&self, o: &Nat) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        if self.groups.len() != o.groups.len() {
            return self.groups.len().cmp(&o.groups.len());
        }
        for i in (0..self.groups.len()).rev() {
            if self.groups[i] != o.groups[i] {
                return self.groups[i].cmp(&o.groups[i]);
            }
        }
        Ordering::Equal
    }

    /// Subtraction `self − o`; requires `self ≥ o` (`None` otherwise).
    pub fn sub(&self, o: &Nat) -> Option<Nat> {
        if self.cmp(o) == std::cmp::Ordering::Less {
            return None;
        }
        let mut a = self.groups.clone();
        let b = &o.groups;
        let mut borrow: i64 = 0;
        for i in 0..a.len() {
            let bv = if i < b.len() { b[i] as i64 } else { 0 };
            let mut cur = a[i] as i64 - bv - borrow;
            if cur < 0 {
                cur += BASE as i64;
                borrow = 1;
            } else {
                borrow = 0;
            }
            a[i] = cur as u32;
        }
        while a.last() == Some(&0) {
            a.pop();
        }
        Some(Nat { groups: a })
    }

    fn mul_small(&self, factor: u32) -> Nat {
        if factor == 0 || self.is_zero() {
            return Nat::zero();
        }
        let mut groups = Vec::with_capacity(self.groups.len() + 1);
        let mut carry = 0u64;
        for &group in &self.groups {
            let value = group as u64 * factor as u64 + carry;
            groups.push((value % BASE) as u32);
            carry = value / BASE;
        }
        if carry != 0 {
            groups.push(carry as u32);
        }
        Nat { groups }
    }

    fn div_small(&self, divisor: u32) -> (Nat, u32) {
        let mut quotient = vec![0u32; self.groups.len()];
        let mut remainder = 0u64;
        for i in (0..self.groups.len()).rev() {
            let value = remainder * BASE + self.groups[i] as u64;
            quotient[i] = (value / divisor as u64) as u32;
            remainder = value % divisor as u64;
        }
        while quotient.last() == Some(&0) {
            quotient.pop();
        }
        (Nat { groups: quotient }, remainder as u32)
    }

    /// Normalized long division directly on base-10^9 limbs (Knuth D).
    /// Returns `(quotient, remainder)`, or `None` for a zero denominator.
    pub fn divrem(&self, den: &Nat) -> Option<(Nat, Nat)> {
        self.divrem_interruptible(den, || false)
            .expect("non-interruptible division")
    }

    /// Long division with a cancellation probe between quotient limbs.
    pub(crate) fn divrem_interruptible(
        &self,
        den: &Nat,
        mut interrupted: impl FnMut() -> bool,
    ) -> Result<Option<(Nat, Nat)>, ()> {
        if interrupted() {
            return Err(());
        }
        if den.is_zero() {
            return Ok(None);
        }
        if self.cmp(den) == std::cmp::Ordering::Less {
            return Ok(Some((Nat::zero(), self.clone())));
        }
        if den.groups.len() == 1 {
            let (quotient, remainder) = self.div_small(den.groups[0]);
            return Ok(Some((
                quotient,
                if remainder == 0 {
                    Nat::zero()
                } else {
                    Nat { groups: vec![remainder] }
                },
            )));
        }

        let n = den.groups.len();
        let normalization = (BASE / (den.groups[n - 1] as u64 + 1)) as u32;
        let normalized_den = den.mul_small(normalization);
        let mut dividend = self.mul_small(normalization).groups;
        dividend.resize(self.groups.len() + 1, 0);
        let m = dividend.len() - n - 1;
        let mut quotient = vec![0u32; m + 1];

        for j in (0..=m).rev() {
            if interrupted() {
                return Err(());
            }
            let top = dividend[j + n] as u64;
            let next = dividend[j + n - 1] as u64;
            let numerator = top * BASE + next;
            let mut estimate = numerator / normalized_den.groups[n - 1] as u64;
            let mut remainder = numerator % normalized_den.groups[n - 1] as u64;
            while estimate == BASE
                || estimate * normalized_den.groups[n - 2] as u64
                    > BASE * remainder + dividend[j + n - 2] as u64
            {
                estimate -= 1;
                remainder += normalized_den.groups[n - 1] as u64;
                if remainder >= BASE {
                    break;
                }
            }

            let mut borrow = 0u64;
            for i in 0..n {
                let product = estimate * normalized_den.groups[i] as u64 + borrow;
                let low = product % BASE;
                borrow = product / BASE;
                if dividend[j + i] as u64 >= low {
                    dividend[j + i] = (dividend[j + i] as u64 - low) as u32;
                } else {
                    dividend[j + i] = (dividend[j + i] as u64 + BASE - low) as u32;
                    borrow += 1;
                }
            }
            let underflow = (dividend[j + n] as u64) < borrow;
            dividend[j + n] = if underflow {
                (dividend[j + n] as u64 + BASE - borrow) as u32
            } else {
                (dividend[j + n] as u64 - borrow) as u32
            };
            if underflow {
                estimate -= 1;
                let mut carry = 0u64;
                for i in 0..n {
                    let sum = dividend[j + i] as u64
                        + normalized_den.groups[i] as u64
                        + carry;
                    dividend[j + i] = (sum % BASE) as u32;
                    carry = sum / BASE;
                }
                dividend[j + n] = ((dividend[j + n] as u64 + carry) % BASE) as u32;
            }
            quotient[j] = estimate as u32;
        }

        while quotient.last() == Some(&0) {
            quotient.pop();
        }
        let normalized_remainder = Nat { groups: dividend[..n].to_vec() };
        let (remainder, _) = normalized_remainder.div_small(normalization);
        Ok(Some((Nat { groups: quotient }, remainder)))
    }
}

fn pow_multiply(
    left: &Nat,
    right: &Nat,
    enforce_limit: bool,
    interrupted: impl FnMut() -> bool,
) -> Result<Nat, super::limits::NumericWorkError> {
    if enforce_limit
        && left.to_decimal().len() + right.to_decimal().len()
            > super::limits::MAX_DECIMAL_WORK_DIGITS as usize + 1
    {
        return Err(super::limits::NumericWorkError::Overflow);
    }
    let product = left
        .mul_interruptible(right, interrupted)
        .map_err(|_| super::limits::NumericWorkError::Interrupted)?;
    if enforce_limit
        && product.to_decimal().len() > super::limits::MAX_DECIMAL_WORK_DIGITS as usize
    {
        return Err(super::limits::NumericWorkError::Overflow);
    }
    Ok(product)
}

impl fmt::Display for Nat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_decimal())
    }
}

impl Nat {
    /// Exact binary bit length (0 → 0). Convert base-10^9 limbs to base-2^32
    /// words once, then inspect the highest word.
    pub fn bit_len(&self) -> u64 {
        let mut words: Vec<u32> = Vec::new();
        for &group in self.groups.iter().rev() {
            let mut carry = group as u64;
            for word in &mut words {
                let value = *word as u64 * BASE + carry;
                *word = value as u32;
                carry = value >> 32;
            }
            if carry != 0 {
                words.push(carry as u32);
            }
        }
        words
            .last()
            .map(|word| (words.len() as u64 - 1) * 32 + (32 - word.leading_zeros()) as u64)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(s: &str) -> Nat {
        Nat::from_decimal(s).unwrap()
    }

    #[test]
    fn decimal_roundtrip() {
        for s in ["0", "1", "9", "10", "999999999", "1000000000", "123456789012345678901234567890"] {
            assert_eq!(n(s).to_decimal(), s, "roundtrip {s}");
        }
    }

    #[test]
    fn parse_rejects_non_digits() {
        assert!(Nat::from_decimal("12a").is_none());
        assert!(Nat::from_decimal("-1").is_none());
        assert!(Nat::from_decimal("").is_none());
    }

    #[test]
    fn add_carries() {
        assert_eq!(n("1").add(&n("1")).to_decimal(), "2");
        assert_eq!(n("999999999").add(&n("1")).to_decimal(), "1000000000");
        assert_eq!(
            n("12345678901234567890").add(&n("98765432109876543210")).to_decimal(),
            "111111111011111111100"
        );
    }

    #[test]
    fn mul_basic() {
        assert_eq!(n("6").mul(&n("7")).to_decimal(), "42");
        assert_eq!(n("12").mul(&n("12")).to_decimal(), "144");
        assert_eq!(n("999999999").mul(&n("999999999")).to_decimal(), "999999998000000001");
        assert_eq!(
            n("123456789").mul(&n("987654321")).to_decimal(),
            "121932631112635269"
        );
    }

    #[test]
    fn pow_golden_values() {
        assert_eq!(n("2").pow(100).to_decimal(), "1267650600228229401496703205376");
        assert_eq!(
            n("2").pow(200).to_decimal(),
            "1606938044258990275541962092341162602522202993782792835301376"
        );
        assert_eq!(
            n("10").pow(30).to_decimal(),
            "1000000000000000000000000000000"
        );
    }

    #[test]
    fn sub_borrows() {
        assert_eq!(n("1000000000").sub(&n("1")).unwrap().to_decimal(), "999999999");
        assert_eq!(n("623").sub(&n("623")).unwrap().to_decimal(), "0");
        assert!(n("1").sub(&n("2")).is_none());
    }

    #[test]
    fn mul_pow10_zero_pad() {
        assert_eq!(n("623").mul_pow10(2).to_decimal(), "62300");
        assert_eq!(n("0").mul_pow10(5).to_decimal(), "0");
    }

    #[test]
    fn divrem_long_division() {
        assert_eq!(n("42").divrem(&n("6")).unwrap().0.to_decimal(), "7");
        assert_eq!(n("1").divrem(&n("3")).unwrap().0.to_decimal(), "0");
        assert_eq!(n("100").divrem(&n("3")).unwrap().0.to_decimal(), "33");
        assert_eq!(n("100").divrem(&n("3")).unwrap().1.to_decimal(), "1");
        assert!(n("5").divrem(&n("0")).is_none());
        // Large operands: 2^100 / 7.
        let big = n("1267650600228229401496703205376");
        let (q, r) = big.divrem(&n("7")).unwrap();
        assert_eq!(q.to_decimal(), "181092942889747057356671886482");
        assert_eq!(r.to_decimal(), "2");
    }

    #[test]
    fn divrem_matches_u128_and_large_identity() {
        let mut state = 0x6a09_e667_f3bc_c908_bb67_ae85_84ca_a73bu128;
        for _ in 0..5_000 {
            state = state
                .wrapping_mul(0x2360_ed05_1fc6_5da4_4385_df64_9fcc_f645u128)
                .wrapping_add(0x9e37_79b9_7f4a_7c15u128);
            let dividend = state;
            state = state.rotate_left(47).wrapping_add(0xda94_2042_e4dd_58b5u128);
            let divisor = state | 1;
            let (quotient, remainder) = n(&dividend.to_string())
                .divrem(&n(&divisor.to_string()))
                .expect("nonzero divisor");
            assert_eq!(quotient.to_decimal(), (dividend / divisor).to_string());
            assert_eq!(remainder.to_decimal(), (dividend % divisor).to_string());
        }

        let dividend = n(&"1234567890".repeat(200));
        let divisor = n(&"9876543210".repeat(100));
        let (quotient, remainder) = dividend.divrem(&divisor).expect("nonzero divisor");
        assert_eq!(quotient.mul(&divisor).add(&remainder), dividend);
        assert_eq!(remainder.cmp(&divisor), std::cmp::Ordering::Less);
    }

    #[test]
    fn bit_len_exact() {
        // Reference computed in u128 (repeated halving).
        fn ref_bits(mut v: u128) -> u64 {
            let mut b = 0;
            while v > 0 {
                b += 1;
                v >>= 1;
            }
            b
        }
        for s in [
            "0", "1", "2", "3", "1023", "1024", "999999999", "1000000000",
            "1073741823", "1073741824", "123456789012345678901234567890",
            "99999999999999999999999999999999999999",
        ] {
            let v: u128 = s.parse().unwrap();
            assert_eq!(n(s).bit_len(), ref_bits(v), "bit_len of {s}");
        }
        // 2^100 is beyond u128? No: 2^100 > u128::MAX is false (2^100 < 2^128).
        let p100 = n("2").pow(100);
        assert_eq!(p100.bit_len(), 101);
        let p200 = n("2").pow(200);
        assert_eq!(p200.bit_len(), 201);
        assert_eq!(Nat::zero(), n("0"));
    }
}
