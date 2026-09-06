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
        while groups.len() > 1 && groups.last() == Some(&0) {
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
        if self.is_zero() || o.is_zero() {
            return Nat::zero();
        }
        let mut res = vec![0u32; self.groups.len() + o.groups.len() + 1];
        for (i, &ai) in self.groups.iter().enumerate() {
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
        while res.len() > 1 && res.last() == Some(&0) {
            res.pop();
        }
        Nat { groups: res }
    }

    /// Binary exponentiation: `base^exp` (exp ≥ 0).
    pub fn pow(&self, exp: u32) -> Nat {
        let mut e = exp;
        let mut base = self.clone();
        let mut acc = Nat::from_decimal("1").unwrap();
        while e > 0 {
            if e & 1 == 1 {
                acc = acc.mul(&base);
            }
            e >>= 1;
            if e > 0 {
                base = base.mul(&base);
            }
        }
        acc
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
        while a.len() > 1 && a.last() == Some(&0) {
            a.pop();
        }
        Some(Nat { groups: a })
    }

    /// Decimal long division: returns (quotient, remainder); `None` when the
    /// denominator is zero. Digit-at-a-time schoolbook division
    /// (quotient digit 0..9 with trial subtraction), O(n²) — sufficient for
    /// this layer's scale.
    pub fn divrem(&self, den: &Nat) -> Option<(Nat, Nat)> {
        if den.is_zero() {
            return None;
        }
        if self.cmp(den) == std::cmp::Ordering::Less {
            return Some((Nat::zero(), self.clone()));
        }
        let a = self.to_decimal();
        let mut rem = Nat::zero();
        let mut q = String::new();
        for c in a.bytes() {
            let digit = (c - b'0') as u32;
            rem = rem.mul_pow10(1).add(&Nat::from_decimal(&digit.to_string()).unwrap());
            let mut qd = 0u32;
            while rem.cmp(den) != std::cmp::Ordering::Less {
                rem = rem.sub(den).unwrap();
                qd += 1;
            }
            q.push(char::from_digit(qd, 10).unwrap());
        }
        let qq = q.trim_start_matches('0');
        let qq = if qq.is_empty() { "0" } else { qq };
        Some((Nat::from_decimal(qq).unwrap(), rem))
    }
}

impl fmt::Display for Nat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_decimal())
    }
}

impl Nat {
    /// Exact binary bit length (0 → 0), computed by repeatedly dividing by
    /// 2^30: `bits(q·2^30 + r) = bits(q) + 30` for `q > 0, r < 2^30`.
    pub fn bit_len(&self) -> u64 {
        const TWO_POW_30: u64 = 1 << 30;
        let mut v = self.clone();
        let mut bits = 0u64;
        loop {
            if v.is_zero() {
                return bits;
            }
            let den = Nat::from_decimal(&TWO_POW_30.to_string()).unwrap();
            let (q, r) = v.divrem(&den).expect("divisor is nonzero");
            if q.is_zero() {
                // Final chunk < 2^30: count its bits directly.
                let n = r.to_decimal().parse::<u64>().unwrap_or(0);
                return bits + (64 - n.leading_zeros()) as u64;
            }
            bits += 30;
            v = q;
        }
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
    }
}
