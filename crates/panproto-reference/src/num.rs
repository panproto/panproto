//! Exact arithmetic written naively: naturals and integers as decimal digit
//! vectors, rationals as reduced pairs.
//!
//! Nothing here calls a big-number library or a float operation. Every
//! operation is the schoolbook algorithm on base-10 digits, so a reader can
//! check each one against its textbook definition. Speed is not a goal: the
//! reference runs on small exhaustive scopes.

use std::cmp::Ordering;
use std::fmt;

/// A natural number, held as its decimal digits, least significant first,
/// with no most-significant zero digit (zero is the empty vector).
#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct Nat {
    digits: Vec<u8>,
}

impl fmt::Debug for Nat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl fmt::Display for Nat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.digits.is_empty() {
            return f.write_str("0");
        }
        for d in self.digits.iter().rev() {
            write!(f, "{d}")?;
        }
        Ok(())
    }
}

impl Nat {
    /// Zero.
    #[must_use]
    pub const fn zero() -> Self {
        Self { digits: Vec::new() }
    }

    /// One.
    #[must_use]
    pub fn one() -> Self {
        Self::from_u64(1)
    }

    fn normalized(mut digits: Vec<u8>) -> Self {
        while digits.last() == Some(&0) {
            digits.pop();
        }
        Self { digits }
    }

    /// The natural number `n`.
    #[must_use]
    pub fn from_u64(mut n: u64) -> Self {
        let mut digits = Vec::new();
        while n > 0 {
            digits.push(u8::try_from(n % 10).unwrap_or(0));
            n /= 10;
        }
        Self { digits }
    }

    /// The natural number `n`.
    #[must_use]
    pub fn from_u128(mut n: u128) -> Self {
        let mut digits = Vec::new();
        while n > 0 {
            digits.push(u8::try_from(n % 10).unwrap_or(0));
            n /= 10;
        }
        Self { digits }
    }

    /// Read a non-empty string of ASCII decimal digits (leading zeros
    /// allowed); `None` for any other string.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(Self::normalized(text.bytes().rev().map(|b| b - b'0').collect()))
    }

    /// Whether this is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }

    /// The number of decimal digits (zero has none).
    #[must_use]
    pub fn digit_count(&self) -> usize {
        self.digits.len()
    }

    /// The digit of weight `10^i` (zero past the most significant digit).
    #[must_use]
    pub fn digit(&self, i: usize) -> u8 {
        self.digits.get(i).copied().unwrap_or(0)
    }

    /// The value as a `u64`, or `None` when it does not fit.
    #[must_use]
    pub fn to_u64(&self) -> Option<u64> {
        let mut out: u64 = 0;
        for d in self.digits.iter().rev() {
            out = out.checked_mul(10)?.checked_add(u64::from(*d))?;
        }
        Some(out)
    }

    /// The value as a `u128`, or `None` when it does not fit.
    #[must_use]
    pub fn to_u128(&self) -> Option<u128> {
        let mut out: u128 = 0;
        for d in self.digits.iter().rev() {
            out = out.checked_mul(10)?.checked_add(u128::from(*d))?;
        }
        Some(out)
    }

    /// The sum `self + other`, digit by digit with carry.
    #[must_use]
    pub fn add(&self, other: &Self) -> Self {
        let n = self.digits.len().max(other.digits.len());
        let mut out = Vec::with_capacity(n + 1);
        let mut carry = 0u8;
        for i in 0..n {
            let s = self.digit(i) + other.digit(i) + carry;
            out.push(s % 10);
            carry = s / 10;
        }
        if carry > 0 {
            out.push(carry);
        }
        Self::normalized(out)
    }

    /// The difference `self - other`, or `None` when `other > self`.
    #[must_use]
    pub fn checked_sub(&self, other: &Self) -> Option<Self> {
        if *self < *other {
            return None;
        }
        let mut out = Vec::with_capacity(self.digits.len());
        let mut borrow = 0u8;
        for i in 0..self.digits.len() {
            let subtrahend = other.digit(i) + borrow;
            let minuend = self.digit(i);
            if minuend >= subtrahend {
                out.push(minuend - subtrahend);
                borrow = 0;
            } else {
                out.push(minuend + 10 - subtrahend);
                borrow = 1;
            }
        }
        Some(Self::normalized(out))
    }

    /// The product `self * d` for one digit-sized factor `d < 10`.
    fn mul_digit(&self, d: u8) -> Self {
        let mut out = Vec::with_capacity(self.digits.len() + 1);
        let mut carry = 0u8;
        for x in &self.digits {
            let p = x * d + carry;
            out.push(p % 10);
            carry = p / 10;
        }
        if carry > 0 {
            out.push(carry);
        }
        Self::normalized(out)
    }

    /// `self * 10^k`.
    #[must_use]
    pub fn shift10(&self, k: usize) -> Self {
        if self.is_zero() {
            return Self::zero();
        }
        let mut digits = vec![0; k];
        digits.extend_from_slice(&self.digits);
        Self { digits }
    }

    /// The product `self * other`, schoolbook: the sum over the digits of
    /// `other` of `self` times that digit, shifted.
    #[must_use]
    pub fn mul(&self, other: &Self) -> Self {
        let mut acc = Self::zero();
        for (i, d) in other.digits.iter().enumerate() {
            acc = acc.add(&self.mul_digit(*d).shift10(i));
        }
        acc
    }

    /// Quotient and remainder of `self / divisor` by long division, one
    /// decimal digit of the quotient at a time; `None` for a zero divisor.
    #[must_use]
    pub fn div_rem(&self, divisor: &Self) -> Option<(Self, Self)> {
        if divisor.is_zero() {
            return None;
        }
        let mut quotient = vec![0u8; self.digits.len()];
        let mut rem = Self::zero();
        for i in (0..self.digits.len()).rev() {
            rem = rem.shift10(1).add(&Self::from_u64(u64::from(self.digit(i))));
            let mut q = 0u8;
            while rem >= *divisor {
                rem = rem.checked_sub(divisor).unwrap_or_default();
                q += 1;
            }
            quotient[i] = q;
        }
        Some((Self::normalized(quotient), rem))
    }

    /// `base^exp` by repeated multiplication.
    #[must_use]
    pub fn pow(base: &Self, exp: u32) -> Self {
        let mut acc = Self::one();
        for _ in 0..exp {
            acc = acc.mul(base);
        }
        acc
    }

    /// `2^k`.
    #[must_use]
    pub fn pow2(k: u32) -> Self {
        Self::pow(&Self::from_u64(2), k)
    }

    /// `10^k`.
    #[must_use]
    pub fn pow10(k: usize) -> Self {
        Self::one().shift10(k)
    }

    /// The greatest common divisor by Euclid's algorithm (`gcd(0, 0) = 0`).
    #[must_use]
    pub fn gcd(a: &Self, b: &Self) -> Self {
        let (mut x, mut y) = (a.clone(), b.clone());
        while !y.is_zero() {
            let r = x.div_rem(&y).map(|(_, r)| r).unwrap_or_default();
            x = y;
            y = r;
        }
        x
    }

    /// Whether the number is even.
    #[must_use]
    pub fn is_even(&self) -> bool {
        self.digit(0) % 2 == 0
    }
}

impl PartialOrd for Nat {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Nat {
    fn cmp(&self, other: &Self) -> Ordering {
        self.digits.len().cmp(&other.digits.len()).then_with(|| {
            for i in (0..self.digits.len()).rev() {
                let o = self.digits[i].cmp(&other.digits[i]);
                if o != Ordering::Equal {
                    return o;
                }
            }
            Ordering::Equal
        })
    }
}

/// An integer: a sign and a magnitude, zero never negative.
#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct Int {
    negative: bool,
    magnitude: Nat,
}

impl fmt::Debug for Int {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl fmt::Display for Int {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.negative {
            f.write_str("-")?;
        }
        write!(f, "{}", self.magnitude)
    }
}

impl Int {
    /// The integer with this sign and magnitude (a negative zero is zero).
    #[must_use]
    pub fn new(negative: bool, magnitude: Nat) -> Self {
        Self {
            negative: negative && !magnitude.is_zero(),
            magnitude,
        }
    }

    /// Zero.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            negative: false,
            magnitude: Nat::zero(),
        }
    }

    /// The integer `n`.
    #[must_use]
    pub fn from_i64(n: i64) -> Self {
        Self::new(n < 0, Nat::from_u64(n.unsigned_abs()))
    }

    /// The integer `n`.
    #[must_use]
    pub fn from_i128(n: i128) -> Self {
        Self::new(n < 0, Nat::from_u128(n.unsigned_abs()))
    }

    /// Read an optional sign (`+` or `-`) followed by decimal digits.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (negative, rest) = match text.as_bytes().first() {
            Some(b'-') => (true, &text[1..]),
            Some(b'+') => (false, &text[1..]),
            _ => (false, text),
        };
        Nat::parse(rest).map(|m| Self::new(negative, m))
    }

    /// Whether this integer is negative.
    #[must_use]
    pub const fn is_negative(&self) -> bool {
        self.negative
    }

    /// Whether this integer is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.magnitude.is_zero()
    }

    /// The magnitude.
    #[must_use]
    pub const fn magnitude(&self) -> &Nat {
        &self.magnitude
    }

    /// The value as an `i64`, or `None` when it does not fit.
    #[must_use]
    pub fn to_i64(&self) -> Option<i64> {
        let m = self.magnitude.to_u64()?;
        if self.negative {
            if m == 1u64 << 63 {
                Some(i64::MIN)
            } else {
                i64::try_from(m).ok().map(|v| -v)
            }
        } else {
            i64::try_from(m).ok()
        }
    }

    /// The negation.
    #[must_use]
    pub fn neg(&self) -> Self {
        Self::new(!self.negative, self.magnitude.clone())
    }

    /// The sum.
    #[must_use]
    pub fn add(&self, other: &Self) -> Self {
        if self.negative == other.negative {
            return Self::new(self.negative, self.magnitude.add(&other.magnitude));
        }
        match self.magnitude.cmp(&other.magnitude) {
            Ordering::Less => Self::new(
                other.negative,
                other.magnitude.checked_sub(&self.magnitude).unwrap_or_default(),
            ),
            _ => Self::new(
                self.negative,
                self.magnitude.checked_sub(&other.magnitude).unwrap_or_default(),
            ),
        }
    }

    /// The difference.
    #[must_use]
    pub fn sub(&self, other: &Self) -> Self {
        self.add(&other.neg())
    }

    /// The product.
    #[must_use]
    pub fn mul(&self, other: &Self) -> Self {
        Self::new(
            self.negative != other.negative,
            self.magnitude.mul(&other.magnitude),
        )
    }
}

impl PartialOrd for Int {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Int {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.negative, other.negative) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => self.magnitude.cmp(&other.magnitude),
            (true, true) => other.magnitude.cmp(&self.magnitude),
        }
    }
}

/// A rational number `num / den` in lowest terms with `den > 0`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Rat {
    num: Int,
    den: Nat,
}

impl fmt::Debug for Rat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.num, self.den)
    }
}

impl Rat {
    /// The reduced fraction `num / den`; `None` for a zero denominator.
    #[must_use]
    pub fn new(num: Int, den: Nat) -> Option<Self> {
        if den.is_zero() {
            return None;
        }
        let g = Nat::gcd(num.magnitude(), &den);
        if g.is_zero() || g == Nat::one() {
            return Some(Self { num, den });
        }
        let (m, _) = num.magnitude().div_rem(&g)?;
        let (d, _) = den.div_rem(&g)?;
        Some(Self {
            num: Int::new(num.is_negative(), m),
            den: d,
        })
    }

    /// The integer `n` as a rational.
    #[must_use]
    pub fn from_int(n: Int) -> Self {
        Self { num: n, den: Nat::one() }
    }

    /// Zero.
    #[must_use]
    pub fn zero() -> Self {
        Self::from_int(Int::zero())
    }

    /// The numerator.
    #[must_use]
    pub const fn num(&self) -> &Int {
        &self.num
    }

    /// The (positive) denominator.
    #[must_use]
    pub const fn den(&self) -> &Nat {
        &self.den
    }

    /// Whether this is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.num.is_zero()
    }

    /// Whether this is negative.
    #[must_use]
    pub const fn is_negative(&self) -> bool {
        self.num.is_negative()
    }

    /// The absolute value.
    #[must_use]
    pub fn abs(&self) -> Self {
        Self {
            num: Int::new(false, self.num.magnitude().clone()),
            den: self.den.clone(),
        }
    }

    /// The negation.
    #[must_use]
    pub fn neg(&self) -> Self {
        Self {
            num: self.num.neg(),
            den: self.den.clone(),
        }
    }

    /// The sum.
    #[must_use]
    pub fn add(&self, other: &Self) -> Self {
        let a = self.num.mul(&Int::new(false, other.den.clone()));
        let b = other.num.mul(&Int::new(false, self.den.clone()));
        Self::new(a.add(&b), self.den.mul(&other.den)).unwrap_or_else(Self::zero)
    }

    /// The difference.
    #[must_use]
    pub fn sub(&self, other: &Self) -> Self {
        self.add(&other.neg())
    }

    /// The product.
    #[must_use]
    pub fn mul(&self, other: &Self) -> Self {
        Self::new(self.num.mul(&other.num), self.den.mul(&other.den)).unwrap_or_else(Self::zero)
    }

    /// The quotient, or `None` when `other` is zero.
    #[must_use]
    pub fn div(&self, other: &Self) -> Option<Self> {
        if other.is_zero() {
            return None;
        }
        let num = self.num.mul(&Int::new(other.is_negative(), other.den.clone()));
        let den = self.den.mul(other.num.magnitude());
        Self::new(num, den)
    }

    /// `2^k` for an integer exponent `k` (negative allowed).
    #[must_use]
    pub fn pow2(k: i64) -> Self {
        let mag = Nat::pow2(u32::try_from(k.unsigned_abs()).unwrap_or(u32::MAX));
        if k >= 0 {
            Self::from_int(Int::new(false, mag))
        } else {
            Self { num: Int::from_i64(1), den: mag }
        }
    }

    /// `10^k` for an integer exponent `k` (negative allowed).
    #[must_use]
    pub fn pow10(k: i64) -> Self {
        let mag = Nat::pow10(usize::try_from(k.unsigned_abs()).unwrap_or(usize::MAX));
        if k >= 0 {
            Self::from_int(Int::new(false, mag))
        } else {
            Self { num: Int::from_i64(1), den: mag }
        }
    }

    /// The greatest integer not above this number.
    #[must_use]
    pub fn floor(&self) -> Int {
        let (q, r) = self
            .num
            .magnitude()
            .div_rem(&self.den)
            .unwrap_or_default();
        if self.is_negative() && !r.is_zero() {
            Int::new(true, q.add(&Nat::one()))
        } else {
            Int::new(self.is_negative(), q)
        }
    }

    /// Whether this rational is an integer.
    #[must_use]
    pub fn is_integer(&self) -> bool {
        self.den == Nat::one()
    }
}

impl PartialOrd for Rat {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Rat {
    fn cmp(&self, other: &Self) -> Ordering {
        let a = self.num.mul(&Int::new(false, other.den.clone()));
        let b = other.num.mul(&Int::new(false, self.den.clone()));
        a.cmp(&b)
    }
}

/// `floor(log2(q))` for a positive rational `q`, found by exact comparison
/// with powers of two, starting from an estimate made from digit counts.
///
/// Returns `None` for a non-positive `q`.
#[must_use]
pub fn floor_log2(q: &Rat) -> Option<i64> {
    if q.is_zero() || q.is_negative() {
        return None;
    }
    let ln = i64::try_from(q.num().magnitude().digit_count()).unwrap_or(i64::MAX);
    let ld = i64::try_from(q.den().digit_count()).unwrap_or(i64::MAX);
    // log2(10) is 3.3219...; the estimate only chooses where the exact search
    // starts, so its accuracy does not matter for the result.
    let mut e = (ln - ld) * 33_219 / 10_000;
    while Rat::pow2(e) > *q {
        e -= 1;
    }
    while Rat::pow2(e + 1) <= *q {
        e += 1;
    }
    Some(e)
}

/// `floor(log10(q))` for a positive rational `q`, by exact comparison with
/// powers of ten. Returns `None` for a non-positive `q`.
#[must_use]
pub fn floor_log10(q: &Rat) -> Option<i64> {
    if q.is_zero() || q.is_negative() {
        return None;
    }
    let ln = i64::try_from(q.num().magnitude().digit_count()).unwrap_or(i64::MAX);
    let ld = i64::try_from(q.den().digit_count()).unwrap_or(i64::MAX);
    let mut e = ln - ld;
    while Rat::pow10(e) > *q {
        e -= 1;
    }
    while Rat::pow10(e + 1) <= *q {
        e += 1;
    }
    Some(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nat(s: &str) -> Nat {
        Nat::parse(s).unwrap_or_default()
    }

    #[test]
    fn schoolbook_operations_agree_with_machine_integers_on_small_operands() {
        for a in 0u64..60 {
            for b in 0u64..60 {
                let (x, y) = (Nat::from_u64(a), Nat::from_u64(b));
                assert_eq!(x.add(&y).to_u64(), Some(a + b));
                assert_eq!(x.mul(&y).to_u64(), Some(a * b));
                assert_eq!(x.checked_sub(&y).and_then(|n| n.to_u64()), a.checked_sub(b));
                assert_eq!(x.cmp(&y), a.cmp(&b));
                if b > 0 {
                    let (q, r) = x.div_rem(&y).unwrap_or_default();
                    assert_eq!((q.to_u64(), r.to_u64()), (Some(a / b), Some(a % b)));
                }
            }
        }
    }

    #[test]
    fn large_products_and_powers_are_exact() {
        // 2^64 = 18446744073709551616 and 10^20 = 2^20 * 5^20.
        assert_eq!(Nat::pow2(64).to_string(), "18446744073709551616");
        assert_eq!(
            nat("18446744073709551616").mul(&nat("18446744073709551616")).to_string(),
            "340282366920938463463374607431768211456"
        );
        assert_eq!(Nat::gcd(&Nat::pow10(20), &Nat::pow2(64)), Nat::pow2(20));
    }

    #[test]
    fn rationals_reduce_and_order() {
        let half = Rat::new(Int::from_i64(2), Nat::from_u64(4)).unwrap_or_else(Rat::zero);
        assert_eq!(half, Rat::pow2(-1));
        assert!(Rat::pow2(-1) < Rat::pow10(0));
        assert_eq!(floor_log2(&Rat::pow2(-1074)), Some(-1074));
        assert_eq!(floor_log2(&Rat::pow10(3)), Some(9));
        assert_eq!(floor_log10(&Rat::pow2(10)), Some(3));
        let third = Rat::new(Int::from_i64(-1), Nat::from_u64(3)).unwrap_or_else(Rat::zero);
        assert_eq!(third.floor(), Int::from_i64(-1));
    }
}
