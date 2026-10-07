//! Exact numbers: numeral lexemes, decimals with an unbounded exponent and
//! integers outside `i64` held as a significand and a count of trailing
//! zeros.

use std::cmp::Ordering;

use crate::num::{Int, Nat, Rat};

/// A decimal numeral as written: sign, the digits before and after the
/// point, and the written exponent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Numeral {
    /// Whether a minus sign was written.
    pub negative: bool,
    /// The digits before the point (possibly empty).
    pub integer: String,
    /// The digits after the point (possibly empty).
    pub fraction: String,
    /// The written exponent (zero when none was written).
    pub exponent: Int,
    /// Whether a point was written.
    pub has_point: bool,
    /// Whether an exponent was written.
    pub has_exponent: bool,
}

/// A NaN or infinity spelling: `nan`, `inf` or `infinity` in any case,
/// after an optional sign and an optional `.` (YAML's `.nan`, `.inf`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonFinite {
    /// A NaN spelling, with whether a minus sign was written.
    NaN {
        /// A minus sign was written.
        negative: bool,
    },
    /// An infinity spelling with its sign.
    Infinity {
        /// A minus sign was written.
        negative: bool,
    },
}

/// Read a NaN or infinity spelling.
#[must_use]
pub fn parse_non_finite(text: &str) -> Option<NonFinite> {
    let (negative, rest) = split_sign(text);
    let rest = rest.strip_prefix('.').unwrap_or(rest);
    if rest.eq_ignore_ascii_case("nan") {
        Some(NonFinite::NaN { negative })
    } else if rest.eq_ignore_ascii_case("inf") || rest.eq_ignore_ascii_case("infinity") {
        Some(NonFinite::Infinity { negative })
    } else {
        None
    }
}

fn split_sign(text: &str) -> (bool, &str) {
    match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    }
}

fn all_digits(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_digit())
}

/// Read a decimal numeral: an optional sign, digits with an optional point
/// (either side of the point may be empty, not both), and an optional
/// exponent `e`/`E` with an optional sign and at least one digit. This is
/// the JSON number grammar widened by the decimal forms of SQL and XML
/// Schema (a leading `+`, a point with an empty side). `None` otherwise.
#[must_use]
pub fn parse_numeral(text: &str) -> Option<Numeral> {
    let (negative, rest) = split_sign(text);
    let (mantissa, exponent_text) = rest
        .find(['e', 'E'])
        .map_or((rest, None), |at| (&rest[..at], Some(&rest[at + 1..])));
    let (integer, fraction, has_point) = mantissa
        .find('.')
        .map_or((mantissa, "", false), |at| (&mantissa[..at], &mantissa[at + 1..], true));
    if (integer.is_empty() && fraction.is_empty()) || !all_digits(integer) || !all_digits(fraction) {
        return None;
    }
    let exponent = match exponent_text {
        None => Int::zero(),
        Some(e) => {
            let (en, digits) = split_sign(e);
            if digits.is_empty() || !all_digits(digits) {
                return None;
            }
            Int::new(en, Nat::parse(digits)?)
        }
    };
    Some(Numeral {
        negative,
        integer: integer.to_owned(),
        fraction: fraction.to_owned(),
        exponent,
        has_point,
        has_exponent: exponent_text.is_some(),
    })
}

impl Numeral {
    /// The significand digits `integer ++ fraction` as a natural.
    #[must_use]
    pub fn significand(&self) -> Nat {
        Nat::parse(&format!("{}{}", self.integer, self.fraction)).unwrap_or_default()
    }

    /// The stored exponent: the written exponent less the number of
    /// fractional digits.
    #[must_use]
    pub fn stored_exponent(&self) -> Int {
        let f = Int::new(false, Nat::from_u64(u64::try_from(self.fraction.len()).unwrap_or(u64::MAX)));
        self.exponent.sub(&f)
    }

    /// Whether every significand digit is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.significand().is_zero()
    }

    /// Whether this is an integer numeral (no point, no exponent).
    #[must_use]
    pub const fn is_integer_form(&self) -> bool {
        !self.has_point && !self.has_exponent
    }
}

/// A decimal value placed against two powers of ten, so that a number with
/// an exponent too large to expand is still classified exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scaled {
    /// The exact value (its magnitude lies in `[10^lo, 10^hi)` or is zero).
    Value(Rat),
    /// The magnitude is at least `10^hi`.
    AtLeastHi,
    /// The magnitude is non-zero and below `10^lo`.
    BelowLo,
}

/// `10^k` for an exponent `k` that fits `i64`.
fn pow10_small(k: &Int) -> Option<Rat> {
    k.to_i64().map(Rat::pow10)
}
/// A finite exact decimal `(-1)^negative x digits x 10^exponent` with an
/// unbounded integer exponent: `digits` has no leading zero (zero is
/// `"0"`), a zero keeps a written minus sign (`-0` and `-0E+3` are negative
/// zeros), and the representation keeps the written scale (`1.50` and `1.5` differ; [`DecimalValue::cmp_numeric`]
/// compares numbers).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DecimalValue {
    /// The sign (a zero may be negative).
    pub negative: bool,
    /// The significant digits, without leading zeros.
    pub digits: Nat,
    /// The power of ten.
    pub exponent: Int,
}

impl DecimalValue {
    /// The decimal of a numeral: digits = integer and fraction digits with
    /// leading zeros dropped, exponent = the stored exponent.
    #[must_use]
    pub fn of_numeral(n: &Numeral) -> Self {
        let digits = n.significand();
        Self {
            negative: n.negative,
            digits,
            exponent: n.stored_exponent(),
        }
    }

    /// Read a decimal lexeme ([`parse_numeral`]); `None` otherwise.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        parse_numeral(text).map(|n| Self::of_numeral(&n))
    }

    /// Whether this is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.digits.is_zero()
    }

    /// `max(0, -exponent)`, the number of fractional digits written.
    #[must_use]
    pub fn scale(&self) -> Nat {
        if self.exponent.is_negative() {
            self.exponent.magnitude().clone()
        } else {
            Nat::zero()
        }
    }

    /// The number of trailing zero digits of `digits` (none for zero).
    fn trailing_zeros(&self) -> Nat {
        if self.is_zero() {
            return Nat::zero();
        }
        let mut k = 0u64;
        while self.digits.digit(usize::try_from(k).unwrap_or(0)) == 0 {
            k += 1;
        }
        Nat::from_u64(k)
    }

    /// The scale with trailing fractional zeros dropped; zero has scale 0.
    #[must_use]
    pub fn minimal_scale(&self) -> Nat {
        if self.is_zero() {
            return Nat::zero();
        }
        self.scale().checked_sub(&self.trailing_zeros()).unwrap_or_default()
    }

    /// The number of digits before the point once leading zeros are dropped:
    /// `max(0, len(digits) + exponent)`; zero has none.
    #[must_use]
    pub fn integer_digits(&self) -> Nat {
        if self.is_zero() {
            return Nat::zero();
        }
        let len = Int::new(false, Nat::from_u64(u64::try_from(self.digits.digit_count()).unwrap_or(0)));
        let pos = len.add(&self.exponent);
        if pos.is_negative() { Nat::zero() } else { pos.magnitude().clone() }
    }

    /// The digits in positional notation: integer digits plus minimal scale.
    #[must_use]
    pub fn positional_digits(&self) -> Nat {
        self.integer_digits().add(&self.minimal_scale())
    }

    /// The position of the leading digit: the magnitude lies in
    /// `[10^(p-1), 10^p)` for `p = len(digits) + exponent` (zero has none).
    #[must_use]
    pub fn position(&self) -> Int {
        Int::new(false, Nat::from_u64(u64::try_from(self.digits.digit_count()).unwrap_or(0))).add(&self.exponent)
    }

    /// The value placed against `10^lo` and `10^hi` (`lo < hi`): exact when
    /// the magnitude lies in `[10^lo, 10^hi)` or is zero, else which side
    /// it lies on. Decided on the leading-digit position, so no power of
    /// ten beyond the two bounds is ever expanded.
    #[must_use]
    pub fn scaled(&self, lo: i64, hi: i64) -> Scaled {
        if self.is_zero() {
            return Scaled::Value(Rat::zero());
        }
        let p = self.position();
        if p > Int::from_i64(hi) {
            return Scaled::AtLeastHi;
        }
        if p <= Int::from_i64(lo) {
            return Scaled::BelowLo;
        }
        let e = pow10_small(&self.exponent).unwrap_or_else(Rat::zero);
        Scaled::Value(Rat::from_int(Int::new(self.negative, self.digits.clone())).mul(&e))
    }

    /// Compare as numbers, ignoring scale. Exact for every exponent: the
    /// position of the leading digit (`len(digits) + exponent`) decides
    /// unless equal, then the digit strings aligned at the leading digit.
    #[must_use]
    pub fn cmp_numeric(&self, other: &Self) -> Ordering {
        let sign = |d: &Self| {
            if d.is_zero() {
                0i8
            } else if d.negative {
                -1
            } else {
                1
            }
        };
        let (a, b) = (sign(self), sign(other));
        if a != b || a == 0 {
            return a.cmp(&b);
        }
        let m = cmp_magnitude(self, other);
        if a < 0 { m.reverse() } else { m }
    }
}

fn cmp_magnitude(a: &DecimalValue, b: &DecimalValue) -> Ordering {
    a.position().cmp(&b.position()).then_with(|| {
        let (x, y) = (a.digits.to_string(), b.digits.to_string());
        let n = x.len().max(y.len());
        let pad = |s: &str| format!("{s:0<n$}");
        pad(&x).cmp(&pad(&y))
    })
}

/// An integer outside `i64`: `(-1)^negative x significand x 10^zeros`, the
/// significand without leading zeros and with a last digit other than 0,
/// `zeros` an unbounded count of trailing zeros.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BigIntValue {
    /// Whether the integer is negative.
    pub negative: bool,
    /// The significand, its last digit non-zero.
    pub significand: Nat,
    /// The count of trailing zeros.
    pub zeros: Nat,
}

/// An exact integer: `Int` when it fits `i64`, else `BigInt`, so each
/// integer has one representation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ExactInteger {
    /// An integer within `i64`.
    Int(i64),
    /// An integer outside `i64`.
    Big(BigIntValue),
}

impl ExactInteger {
    /// The representation of `(-1)^negative x significand x 10^zeros`:
    /// trailing zeros of the significand move into the count, and a value
    /// within `i64` is `Int`. The count may be any natural, so no expansion
    /// past a bound ever happens.
    #[must_use]
    pub fn from_parts(negative: bool, significand: &Nat, zeros: &Nat) -> Self {
        if significand.is_zero() {
            return Self::Int(0);
        }
        let mut s = significand.clone();
        let mut z = zeros.clone();
        while s.digit(0) == 0 {
            s = s.div_rem(&Nat::from_u64(10)).map(|(q, _)| q).unwrap_or_default();
            z = z.add(&Nat::one());
        }
        // Small enough to expand: 19 digits bound i64.
        if z.to_u64().is_some_and(|k| k <= 19) && s.digit_count() <= 19 {
            let full = s.shift10(usize::try_from(z.to_u64().unwrap_or(0)).unwrap_or(0));
            if let Some(v) = Int::new(negative, full).to_i64() {
                return Self::Int(v);
            }
        }
        Self::Big(BigIntValue { negative, significand: s, zeros: z })
    }

    /// The exact value; `None` when the count of zeros exceeds `i64`.
    #[must_use]
    pub fn value(&self) -> Option<Rat> {
        match self {
            Self::Int(v) => Some(Rat::from_int(Int::from_i64(*v))),
            Self::Big(b) => {
                let k = i64::try_from(b.zeros.to_u64()?).ok()?;
                Some(Rat::from_int(Int::new(b.negative, b.significand.clone())).mul(&Rat::pow10(k)))
            }
        }
    }

    /// Compare two integers as numbers, exactly, for any count of zeros.
    #[must_use]
    pub fn cmp_numeric(&self, other: &Self) -> Ordering {
        self.as_decimal().cmp_numeric(&other.as_decimal())
    }

    /// The integer as a decimal of exponent `zeros` (or 0).
    #[must_use]
    pub fn as_decimal(&self) -> DecimalValue {
        match self {
            Self::Int(v) => DecimalValue {
                negative: *v < 0,
                digits: Nat::from_u64(v.unsigned_abs()),
                exponent: Int::zero(),
            },
            Self::Big(b) => DecimalValue {
                negative: b.negative,
                digits: b.significand.clone(),
                exponent: Int::new(false, b.zeros.clone()),
            },
        }
    }
}

/// The integer an integral numeral denotes, or `None` when the numeral has
/// a fractional part that is not zero (its minimal scale is positive).
/// Every integral numeral is read, whatever its exponent.
#[must_use]
pub fn integer_of_numeral(n: &Numeral) -> Option<ExactInteger> {
    let d = DecimalValue::of_numeral(n);
    if d.is_zero() {
        return Some(ExactInteger::Int(0));
    }
    if !d.minimal_scale().is_zero() {
        return None;
    }
    // Integral: strip trailing zeros into the count, then add the exponent.
    let mut s = d.digits.clone();
    let mut z = Int::zero();
    while s.digit(0) == 0 {
        s = s.div_rem(&Nat::from_u64(10)).map(|(q, _)| q).unwrap_or_default();
        z = z.add(&Int::from_i64(1));
    }
    let count = z.add(&d.exponent);
    Some(ExactInteger::from_parts(d.negative, &s, count.magnitude()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numerals_read_their_stored_exponent_and_scale() {
        let d = DecimalValue::parse("1.50");
        let shown = d.as_ref().map(|d| (d.digits.to_string(), d.exponent.to_string(), d.scale().to_string()));
        assert_eq!(shown, Some(("150".into(), "-2".into(), "2".into())));
        assert_eq!(d.map(|d| d.minimal_scale().to_string()), Some("1".into()));
        assert!(parse_numeral(".5").is_some() && parse_numeral("5.").is_some());
        assert!(parse_numeral(".").is_none() && parse_numeral("1e").is_none() && parse_numeral("--1").is_none());
        let big = parse_numeral("0.0e-3000000000").map(|n| DecimalValue::of_numeral(&n));
        assert_eq!(big.map(|d| d.scale().to_string()), Some("3000000001".into()));
    }

    #[test]
    fn integers_keep_one_representation_and_any_count() {
        let n = parse_numeral("1e4097").and_then(|n| integer_of_numeral(&n));
        let written = parse_numeral(&format!("1{}", "0".repeat(4097))).and_then(|n| integer_of_numeral(&n));
        assert_eq!(n, written);
        assert_eq!(parse_numeral("9223372036854775807").and_then(|n| integer_of_numeral(&n)), Some(ExactInteger::Int(i64::MAX)));
        assert!(matches!(parse_numeral("9223372036854775808").and_then(|n| integer_of_numeral(&n)), Some(ExactInteger::Big(_))));
        assert_eq!(parse_numeral("-9223372036854775808").and_then(|n| integer_of_numeral(&n)), Some(ExactInteger::Int(i64::MIN)));
        assert_eq!(parse_numeral("1.5").and_then(|n| integer_of_numeral(&n)), None);
        assert_eq!(parse_numeral("150e-1").and_then(|n| integer_of_numeral(&n)), Some(ExactInteger::Int(15)));
        let huge = parse_numeral("1e9223372036854775807").and_then(|n| integer_of_numeral(&n));
        let bigger = parse_numeral("1e9223372036854775808").and_then(|n| integer_of_numeral(&n));
        assert_eq!(huge.zip(bigger).map(|(a, b)| a.cmp_numeric(&b)), Some(Ordering::Less));
    }
}

/// A decimal value: a finite decimal, an infinity, or a NaN with its sign,
/// signalling flag and payload (the General Decimal Arithmetic values).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Decimal {
    /// A finite decimal.
    Finite(DecimalValue),
    /// An infinity.
    Infinite {
        /// The sign.
        negative: bool,
    },
    /// A NaN; a plain `NaN` has payload zero (`NaN` and `NaN0` are one value).
    NaN {
        /// The sign.
        negative: bool,
        /// Whether it is signalling (`sNaN`).
        signalling: bool,
        /// The diagnostic payload.
        payload: Nat,
    },
}

/// Read a decimal lexeme: a finite numeral ([`parse_numeral`]) or a special
/// form of the General Decimal Arithmetic numeric string syntax:
/// `Infinity` or `Inf`, `NaN` or `sNaN` followed by optional payload
/// digits, in any case, with an optional sign. `None` otherwise.
#[must_use]
pub fn parse_decimal(text: &str) -> Option<Decimal> {
    if let Some(n) = parse_numeral(text) {
        return Some(Decimal::Finite(DecimalValue::of_numeral(&n)));
    }
    let (negative, rest) = split_sign(text);
    let lower = rest.to_ascii_lowercase();
    if lower == "inf" || lower == "infinity" {
        return Some(Decimal::Infinite { negative });
    }
    let (signalling, tail) = if let Some(t) = lower.strip_prefix("snan") {
        (true, t)
    } else {
        (false, lower.strip_prefix("nan")?)
    };
    let payload = if tail.is_empty() { Nat::zero() } else { Nat::parse(tail)? };
    Some(Decimal::NaN { negative, signalling, payload })
}

impl Decimal {
    /// The rank of a value's class in the numeric preorder: negative quiet
    /// NaNs, negative signalling NaNs, minus infinity, finite, plus
    /// infinity, positive signalling NaNs, positive quiet NaNs.
    fn class_rank(&self) -> u8 {
        match self {
            Self::NaN { negative: true, signalling: false, .. } => 0,
            Self::NaN { negative: true, signalling: true, .. } => 1,
            Self::Infinite { negative: true } => 2,
            Self::Finite(_) => 3,
            Self::Infinite { negative: false } => 4,
            Self::NaN { negative: false, signalling: true, .. } => 5,
            Self::NaN { negative: false, signalling: false, .. } => 6,
        }
    }

    /// The total numeric preorder: finite values by number (every zero
    /// `Equal`), the infinities at the ends of the finite values, and NaNs
    /// in compare-total's places, payloads ignored.
    #[must_use]
    pub fn cmp_numeric(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Finite(a), Self::Finite(b)) => a.cmp_numeric(b),
            _ => self.class_rank().cmp(&other.class_rank()),
        }
    }

    /// The General Decimal Arithmetic compare-total order, from its rules:
    /// for representations of sign 0, numbers by value and, at equal value,
    /// by exponent (larger higher); every signalling NaN below every quiet
    /// NaN, NaNs by payload; all of it reversed for sign 1, and every
    /// representation of sign 1 below every one of sign 0.
    #[must_use]
    pub fn compare_total(&self, other: &Self) -> Ordering {
        let (sa, sb) = (self.sign(), other.sign());
        if sa != sb {
            return if sa { Ordering::Less } else { Ordering::Greater };
        }
        let positive = cmp_total_positive(self, other);
        if sa { positive.reverse() } else { positive }
    }

    fn sign(&self) -> bool {
        match self {
            Self::Finite(d) => d.negative,
            Self::Infinite { negative } | Self::NaN { negative, .. } => *negative,
        }
    }
}

/// Compare-total of two representations of one sign, as if both were of
/// sign 0.
fn cmp_total_positive(a: &Decimal, b: &Decimal) -> Ordering {
    let rank = |d: &Decimal| match d {
        Decimal::Finite(_) => 0u8,
        Decimal::Infinite { .. } => 1,
        Decimal::NaN { signalling: true, .. } => 2,
        Decimal::NaN { signalling: false, .. } => 3,
    };
    match (a, b) {
        (Decimal::Finite(x), Decimal::Finite(y)) => {
            let (mx, my) = (DecimalValue { negative: false, ..x.clone() }, DecimalValue { negative: false, ..y.clone() });
            mx.cmp_numeric(&my).then_with(|| x.exponent.cmp(&y.exponent))
        }
        (Decimal::NaN { signalling: s1, payload: p1, .. }, Decimal::NaN { signalling: s2, payload: p2, .. }) if s1 == s2 => {
            p1.cmp(p2)
        }
        _ => rank(a).cmp(&rank(b)),
    }
}

#[cfg(test)]
mod decimal_tests {
    use super::*;

    #[test]
    fn compare_total_follows_its_rules() {
        // Rules applied to the published list: sign 1 reverses the exponent
        // rule, so -1 sorts below -1.00 and -0 below -0.000.
        let order = ["-NaN", "-sNaN", "-Infinity", "-127", "-1", "-1.00", "-0", "-0.000", "0", "1.2300", "1.23", "1E+9", "Infinity", "sNaN", "NaN", "NaN456"];
        let parsed: Vec<Decimal> = order.iter().filter_map(|t| parse_decimal(t)).collect();
        assert_eq!(parsed.len(), order.len());
        for w in parsed.windows(2) {
            assert_eq!(w[0].compare_total(&w[1]), Ordering::Less, "{:?} {:?}", w[0], w[1]);
        }
        for a in &parsed {
            for b in &parsed {
                if a.cmp_numeric(b) == Ordering::Less {
                    assert_eq!(a.compare_total(b), Ordering::Less);
                }
            }
        }
        assert_eq!(parse_decimal("NaN"), parse_decimal("nan0"));
    }
}
