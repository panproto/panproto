//! Exact arithmetic written naively: naturals and integers as digit vectors
//! in base `10^9`, rationals as reduced pairs.
//!
//! Nothing here calls a big-number library or a float operation. Every
//! operation is the schoolbook algorithm on base-`10^9` digits (each digit a
//! block of nine decimal digits, so the decimal text is the digits written
//! out), and a reader can check each one against its textbook definition.
//! Speed is not a goal beyond making the exhaustive scopes run.

use std::cmp::Ordering;
use std::fmt;

/// The base of a digit: `10^9`.
const BASE: u64 = 1_000_000_000;

/// A natural number, held as its base-`10^9` digits, least significant
/// first, with no most-significant zero digit (zero is the empty vector).
#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct Nat {
    limbs: Vec<u32>,
}

impl fmt::Debug for Nat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl fmt::Display for Nat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some((top, rest)) = self.limbs.split_last() else {
            return f.write_str("0");
        };
        write!(f, "{top}")?;
        for l in rest.iter().rev() {
            write!(f, "{l:09}")?;
        }
        Ok(())
    }
}

impl Nat {
    /// Zero.
    #[must_use]
    pub const fn zero() -> Self {
        Self { limbs: Vec::new() }
    }

    /// One.
    #[must_use]
    pub fn one() -> Self {
        Self::from_u64(1)
    }

    fn normalized(mut limbs: Vec<u32>) -> Self {
        while limbs.last() == Some(&0) {
            limbs.pop();
        }
        Self { limbs }
    }

    /// The natural number `n`.
    #[must_use]
    pub fn from_u64(n: u64) -> Self {
        Self::from_u128(u128::from(n))
    }

    /// The natural number `n`.
    #[must_use]
    pub fn from_u128(mut n: u128) -> Self {
        let mut limbs = Vec::new();
        while n > 0 {
            limbs.push(u32::try_from(n % u128::from(BASE)).unwrap_or(0));
            n /= u128::from(BASE);
        }
        Self { limbs }
    }

    /// Read a non-empty string of ASCII decimal digits (leading zeros
    /// allowed); `None` for any other string.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let bytes = text.as_bytes();
        let mut limbs = Vec::new();
        let mut end = bytes.len();
        while end > 0 {
            let start = end.saturating_sub(9);
            let chunk = std::str::from_utf8(&bytes[start..end]).ok()?;
            limbs.push(chunk.parse().ok()?);
            end = start;
        }
        Some(Self::normalized(limbs))
    }

    /// Whether this is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    /// The number of decimal digits (zero has none).
    #[must_use]
    pub fn digit_count(&self) -> usize {
        self.limbs
            .last()
            .map_or(0, |top| (self.limbs.len() - 1) * 9 + top.to_string().len())
    }

    /// The decimal digit of weight `10^i` (zero past the most significant
    /// digit).
    #[must_use]
    pub fn digit(&self, i: usize) -> u8 {
        let limb = self.limbs.get(i / 9).copied().unwrap_or(0);
        let mut v = limb;
        for _ in 0..(i % 9) {
            v /= 10;
        }
        u8::try_from(v % 10).unwrap_or(0)
    }

    /// The value as a `u64`, or `None` when it does not fit.
    #[must_use]
    pub fn to_u64(&self) -> Option<u64> {
        self.to_u128().and_then(|v| u64::try_from(v).ok())
    }

    /// The value as a `u128`, or `None` when it does not fit.
    #[must_use]
    pub fn to_u128(&self) -> Option<u128> {
        let mut out: u128 = 0;
        for l in self.limbs.iter().rev() {
            out = out
                .checked_mul(u128::from(BASE))?
                .checked_add(u128::from(*l))?;
        }
        Some(out)
    }

    fn limb(&self, i: usize) -> u64 {
        u64::from(self.limbs.get(i).copied().unwrap_or(0))
    }

    /// The sum `self + other`, digit by digit with carry.
    #[must_use]
    pub fn add(&self, other: &Self) -> Self {
        let n = self.limbs.len().max(other.limbs.len());
        let mut out = Vec::with_capacity(n + 1);
        let mut carry = 0u64;
        for i in 0..n {
            let s = self.limb(i) + other.limb(i) + carry;
            out.push(u32::try_from(s % BASE).unwrap_or(0));
            carry = s / BASE;
        }
        if carry > 0 {
            out.push(u32::try_from(carry).unwrap_or(0));
        }
        Self::normalized(out)
    }

    /// The difference `self - other`, or `None` when `other > self`.
    #[must_use]
    pub fn checked_sub(&self, other: &Self) -> Option<Self> {
        if *self < *other {
            return None;
        }
        let mut out = Vec::with_capacity(self.limbs.len());
        let mut borrow = 0u64;
        for i in 0..self.limbs.len() {
            let subtrahend = other.limb(i) + borrow;
            let minuend = self.limb(i);
            if minuend >= subtrahend {
                out.push(u32::try_from(minuend - subtrahend).unwrap_or(0));
                borrow = 0;
            } else {
                out.push(u32::try_from(minuend + BASE - subtrahend).unwrap_or(0));
                borrow = 1;
            }
        }
        Some(Self::normalized(out))
    }

    /// The product by one digit `d < 10^9`.
    fn mul_small(&self, d: u64) -> Self {
        let mut out = Vec::with_capacity(self.limbs.len() + 1);
        let mut carry = 0u64;
        for x in &self.limbs {
            let p = u64::from(*x) * d + carry;
            out.push(u32::try_from(p % BASE).unwrap_or(0));
            carry = p / BASE;
        }
        while carry > 0 {
            out.push(u32::try_from(carry % BASE).unwrap_or(0));
            carry /= BASE;
        }
        Self::normalized(out)
    }

    fn shift_limbs(&self, k: usize) -> Self {
        if self.is_zero() {
            return Self::zero();
        }
        let mut limbs = vec![0; k];
        limbs.extend_from_slice(&self.limbs);
        Self { limbs }
    }

    /// `self * 10^k`.
    #[must_use]
    pub fn shift10(&self, k: usize) -> Self {
        let mut p = 1u64;
        for _ in 0..(k % 9) {
            p *= 10;
        }
        self.mul_small(p).shift_limbs(k / 9)
    }

    /// The product `self * other`, schoolbook: the sum over the digits of
    /// `other` of `self` times that digit, shifted.
    #[must_use]
    pub fn mul(&self, other: &Self) -> Self {
        let mut acc = Self::zero();
        for (i, d) in other.limbs.iter().enumerate() {
            acc = acc.add(&self.mul_small(u64::from(*d)).shift_limbs(i));
        }
        acc
    }

    /// Quotient and remainder of `self / divisor` by long division, one
    /// base-`10^9` digit of the quotient at a time; each digit is the
    /// largest q with `divisor * q <= remainder`, found from a lower
    /// estimate (the remainder's leading digits over the divisor's leading
    /// digit plus one) raised one step at a time; `None` for a zero
    /// divisor.
    #[must_use]
    pub fn div_rem(&self, divisor: &Self) -> Option<(Self, Self)> {
        if divisor.is_zero() {
            return None;
        }
        let k = divisor.limbs.len();
        let b = u128::from(BASE);
        // The divisor's two leading digits (rounded up) and the remainder's
        // matching three leading digits give a lower estimate within a
        // few units of the true digit.
        let (lead, low) = if k >= 2 {
            (
                u128::from(divisor.limb(k - 1)) * b + u128::from(divisor.limb(k - 2)) + 1,
                k - 2,
            )
        } else {
            (u128::from(divisor.limb(0)), 0)
        };
        let mut quotient = vec![0u32; self.limbs.len()];
        let mut rem = Self::zero();
        for i in (0..self.limbs.len()).rev() {
            rem = rem.shift_limbs(1).add(&Self::from_u64(self.limb(i)));
            let top = (low..=k)
                .rev()
                .fold(0u128, |acc, j| acc * b + u128::from(rem.limb(j)));
            let mut q = u64::try_from((top / lead).min(u128::from(BASE - 1))).unwrap_or(0);
            let mut product = divisor.mul_small(q);
            while product > rem {
                q -= 1;
                product = divisor.mul_small(q);
            }
            loop {
                let next = product.add(divisor);
                if next > rem {
                    break;
                }
                product = next;
                q += 1;
            }
            rem = rem.checked_sub(&product).unwrap_or_default();
            quotient[i] = u32::try_from(q).unwrap_or(0);
        }
        Some((Self::normalized(quotient), rem))
    }

    /// `base^exp` by repeated squaring.
    #[must_use]
    pub fn pow(base: &Self, exp: u32) -> Self {
        let mut acc = Self::one();
        let mut b = base.clone();
        let mut e = exp;
        while e > 0 {
            if e & 1 == 1 {
                acc = acc.mul(&b);
            }
            b = b.mul(&b);
            e >>= 1;
        }
        acc
    }

    /// `2^k`, memoized per thread (the memo changes no result).
    #[must_use]
    pub fn pow2(k: u32) -> Self {
        thread_local! {
            static MEMO: std::cell::RefCell<std::collections::HashMap<u32, Nat>> =
                std::cell::RefCell::new(std::collections::HashMap::new());
        }
        MEMO.with(|m| {
            if let Some(v) = m.borrow().get(&k) {
                return v.clone();
            }
            let v = Self::pow(&Self::from_u64(2), k);
            m.borrow_mut().insert(k, v.clone());
            v
        })
    }

    /// `10^k`.
    #[must_use]
    pub fn pow10(k: usize) -> Self {
        Self::one().shift10(k)
    }

    /// Half of an even number (the quotient by two, rounding down).
    fn half(&self) -> Self {
        let mut out = vec![0u32; self.limbs.len()];
        let mut carry = 0u64;
        for i in (0..self.limbs.len()).rev() {
            let cur = carry * BASE + self.limb(i);
            out[i] = u32::try_from(cur / 2).unwrap_or(0);
            carry = cur % 2;
        }
        Self::normalized(out)
    }

    /// The greatest common divisor by the binary algorithm (Stein): strip
    /// the common factors of two, then repeatedly replace the larger odd
    /// number by half the difference (`gcd(0, 0) = 0`).
    #[must_use]
    pub fn gcd(a: &Self, b: &Self) -> Self {
        if a.is_zero() {
            return b.clone();
        }
        if b.is_zero() {
            return a.clone();
        }
        let (mut x, mut y) = (a.clone(), b.clone());
        let mut twos = 0u32;
        while x.is_even() && y.is_even() {
            x = x.half();
            y = y.half();
            twos += 1;
        }
        while x.is_even() {
            x = x.half();
        }
        while !y.is_zero() {
            while y.is_even() {
                y = y.half();
            }
            if x > y {
                std::mem::swap(&mut x, &mut y);
            }
            y = y.checked_sub(&x).unwrap_or_default();
        }
        x.mul(&Self::pow2(twos))
    }

    /// Whether the number is even.
    #[must_use]
    pub fn is_even(&self) -> bool {
        self.limb(0) % 2 == 0
    }
}

impl PartialOrd for Nat {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Nat {
    fn cmp(&self, other: &Self) -> Ordering {
        self.limbs.len().cmp(&other.limbs.len()).then_with(|| {
            for i in (0..self.limbs.len()).rev() {
                let o = self.limbs[i].cmp(&other.limbs[i]);
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
                other
                    .magnitude
                    .checked_sub(&self.magnitude)
                    .unwrap_or_default(),
            ),
            _ => Self::new(
                self.negative,
                self.magnitude
                    .checked_sub(&other.magnitude)
                    .unwrap_or_default(),
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
        if den == Nat::one() {
            return Some(Self { num, den });
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
        Self {
            num: n,
            den: Nat::one(),
        }
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
        let num = self
            .num
            .mul(&Int::new(other.is_negative(), other.den.clone()));
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
            Self {
                num: Int::from_i64(1),
                den: mag,
            }
        }
    }

    /// `10^k` for an integer exponent `k` (negative allowed).
    #[must_use]
    pub fn pow10(k: i64) -> Self {
        let mag = Nat::pow10(usize::try_from(k.unsigned_abs()).unwrap_or(usize::MAX));
        if k >= 0 {
            Self::from_int(Int::new(false, mag))
        } else {
            Self {
                num: Int::from_i64(1),
                den: mag,
            }
        }
    }

    /// The greatest integer not above this number.
    #[must_use]
    pub fn floor(&self) -> Int {
        let (q, r) = self.num.magnitude().div_rem(&self.den).unwrap_or_default();
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
                let (q, r) = x
                    .div_rem(&y)
                    .map_or((None, None), |(q, r)| (q.to_u64(), r.to_u64()));
                assert_eq!((q, r), (a.checked_div(b), a.checked_rem(b)));
            }
        }
    }

    #[test]
    fn large_products_and_powers_are_exact() {
        // 2^64 = 18446744073709551616 and 10^20 = 2^20 * 5^20.
        assert_eq!(Nat::pow2(64).to_string(), "18446744073709551616");
        assert_eq!(
            nat("18446744073709551616")
                .mul(&nat("18446744073709551616"))
                .to_string(),
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
