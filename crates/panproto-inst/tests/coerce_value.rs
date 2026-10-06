#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The coercion laws, checked against oracles written here without the
//! code under test.
//!
//! On its domain (`coerce` under a kind other than `Any`; `coerce_value`
//! on a value with a scalar lexeme, non-finite floats excluded):
//! (i) lexical fidelity, an `Ok` result is the kind's reading of the
//! lexeme, and a float kind accepts a numeral exactly when the value it
//! rounds to equals it or writes it numerically as its shortest text;
//! (ii) `coerce_value` is idempotent; (iii) every value `coerce` returns is
//! a fixed point of `coerce_value`. Outside the domain, values pass through
//! unchanged. Under a 32-bit float kind `coerce_value` keeps a float only
//! when it is exactly a binary32 value.

use std::cmp::Ordering;

use panproto_inst::Value;
use panproto_inst::value::coerce::{LexClass, Lexeme, coerce, coerce_value};
use panproto_inst::value::{DecimalValue, FidelityReason, Temporal, ValueSite};
use panproto_schema::{BytesText, ScalarKind, ValueSpec};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// An exact oracle, written without the code under test: numerals as sign,
// integer digits and fraction digits; binary floats as dyadic rationals
// m * 2^e; natural numbers little-endian in base 2^32. It decides whether a
// float is the round-to-nearest-even reading of a numeral at a width, and
// whether a numeral equals a float exactly, by integer comparison.
// ---------------------------------------------------------------------------

/// A finite numeral `[sign] digits [. digits] [e [sign] digits]` with its
/// point moved to the end of the integer digits: sign, the integer digits
/// without leading zeros and the fraction digits without trailing zeros.
fn normalize(text: &str) -> (bool, String, String) {
    let (negative, rest) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let (mantissa, exponent) = rest.find(['e', 'E']).map_or((rest, 0), |i| {
        (&rest[..i], rest[i + 1..].parse::<i64>().unwrap())
    });
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut digits = format!("{int}{frac}");
    let mut point = i64::try_from(int.len()).unwrap() + exponent;
    if point < 0 {
        digits = format!("{}{digits}", "0".repeat(usize::try_from(-point).unwrap()));
        point = 0;
    }
    let point = usize::try_from(point).unwrap();
    if point > digits.len() {
        digits.push_str(&"0".repeat(point - digits.len()));
    }
    let int = digits[..point].trim_start_matches('0').to_owned();
    let frac = digits[point..].trim_end_matches('0').to_owned();
    let zero = int.is_empty() && frac.is_empty();
    (negative && !zero, int, frac)
}

/// Whether a numeral is written with a leading `-`, the sign a zero keeps.
fn written_negative(text: &str) -> bool {
    text.starts_with('-')
}

/// A natural number, little-endian in base 2^32, without high zero limbs.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Nat(Vec<u32>);

impl Nat {
    fn trim(mut self) -> Self {
        while self.0.last() == Some(&0) {
            self.0.pop();
        }
        self
    }

    fn from_u64(n: u64) -> Self {
        Self(vec![
            u32::try_from(n & 0xffff_ffff).unwrap(),
            u32::try_from(n >> 32).unwrap(),
        ])
        .trim()
    }

    fn mul_add(&mut self, factor: u32, addend: u32) {
        let mut carry = u64::from(addend);
        for limb in &mut self.0 {
            let product = u64::from(*limb) * u64::from(factor) + carry;
            *limb = u32::try_from(product & 0xffff_ffff).unwrap();
            carry = product >> 32;
        }
        if carry > 0 {
            self.0.push(u32::try_from(carry).unwrap());
        }
    }

    fn from_digits(digits: &str) -> Self {
        let mut n = Self(Vec::new());
        for d in digits.bytes() {
            n.mul_add(10, u32::from(d - b'0'));
        }
        n.trim()
    }

    /// `self * 10^power`, nine decimal digits at a time.
    fn mul_pow10(mut self, power: u64) -> Self {
        let mut left = power;
        while left > 0 {
            let step = left.min(9);
            self.mul_add(10_u32.pow(u32::try_from(step).unwrap()), 0);
            left -= step;
        }
        self.trim()
    }

    /// `self * 2^power`.
    fn shl(self, power: u64) -> Self {
        if self.0.is_empty() {
            return self;
        }
        let limbs = usize::try_from(power / 32).unwrap();
        let bits = u32::try_from(power % 32).unwrap();
        let mut out = vec![0_u32; limbs];
        let mut carry = 0_u32;
        for limb in self.0 {
            let wide = (u64::from(limb) << bits) | u64::from(carry);
            out.push(u32::try_from(wide & 0xffff_ffff).unwrap());
            carry = u32::try_from(wide >> 32).unwrap();
        }
        out.push(carry);
        Self(out).trim()
    }

    fn add(&self, other: &Self) -> Self {
        let mut out = Vec::with_capacity(self.0.len().max(other.0.len()) + 1);
        let mut carry = 0_u64;
        for i in 0..self.0.len().max(other.0.len()) {
            let sum = u64::from(*self.0.get(i).unwrap_or(&0))
                + u64::from(*other.0.get(i).unwrap_or(&0))
                + carry;
            out.push(u32::try_from(sum & 0xffff_ffff).unwrap());
            carry = sum >> 32;
        }
        out.push(u32::try_from(carry).unwrap());
        Self(out).trim()
    }

    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .len()
            .cmp(&other.0.len())
            .then_with(|| self.0.iter().rev().cmp(other.0.iter().rev()))
    }
}

/// A non-negative dyadic rational `m * 2^e`.
#[derive(Clone, Debug)]
struct Dyadic {
    m: Nat,
    e: i64,
}

impl Dyadic {
    /// The magnitude of a finite float given as its significand bits, its
    /// biased exponent field, and the width's fraction length and bias.
    fn of_fields(fraction: u64, biased: i64, fraction_bits: i64, bias: i64) -> Self {
        let (m, e) = if biased == 0 {
            (fraction, 1 - bias - fraction_bits)
        } else {
            (
                fraction | (1 << fraction_bits),
                biased - bias - fraction_bits,
            )
        };
        Self {
            m: Nat::from_u64(m),
            e,
        }
    }

    /// The midpoint of two dyadic rationals.
    fn midpoint(&self, other: &Self) -> Self {
        let e = self.e.min(other.e);
        let a = self.m.clone().shl(u64::try_from(self.e - e).unwrap());
        let b = other.m.clone().shl(u64::try_from(other.e - e).unwrap());
        Self {
            m: a.add(&b),
            e: e - 1,
        }
    }
}

/// A float width: fraction bits, exponent bias and the largest biased
/// exponent of a finite value.
#[derive(Clone, Copy)]
struct Width {
    fraction_bits: i64,
    bias: i64,
    max_biased: i64,
}

const BINARY32: Width = Width {
    fraction_bits: 23,
    bias: 127,
    max_biased: 254,
};
const BINARY64: Width = Width {
    fraction_bits: 52,
    bias: 1023,
    max_biased: 2046,
};

const fn width(bits: u8) -> Width {
    if bits == 32 { BINARY32 } else { BINARY64 }
}

/// The fields `(fraction, biased exponent)` of the magnitude of `x` at
/// `w`, for an `x` that the width holds.
fn fields(x: f64, w: Width) -> (u64, i64) {
    if w.fraction_bits == 23 {
        let narrow = x.to_string().parse::<f32>().unwrap();
        assert_eq!(
            f64::from(narrow).to_bits(),
            x.to_bits(),
            "{x} is not a binary32 value"
        );
        let bits = u64::from(narrow.abs().to_bits());
        (bits & ((1 << 23) - 1), i64::try_from(bits >> 23).unwrap())
    } else {
        let bits = x.abs().to_bits();
        (bits & ((1 << 52) - 1), i64::try_from(bits >> 52).unwrap())
    }
}

/// Compare the magnitude of the numeral `(int, frac)` with `d` exactly:
/// `D / 10^f` against `m * 2^e`, with D the digits and f their fraction
/// length, by `D * 2^max(-e, 0)` against `m * 2^max(e, 0) * 10^f`.
fn cmp_numeral(int: &str, frac: &str, d: &Dyadic) -> Ordering {
    let f = u64::try_from(frac.len()).unwrap();
    let numeral = Nat::from_digits(&format!("{int}{frac}"));
    let (lhs, rhs) = if d.e >= 0 {
        (
            numeral,
            d.m.clone().shl(u64::try_from(d.e).unwrap()).mul_pow10(f),
        )
    } else {
        (
            numeral.shl(u64::try_from(-d.e).unwrap()),
            d.m.clone().mul_pow10(f),
        )
    };
    lhs.cmp(&rhs)
}

/// Whether the finite numeral `text` equals the finite float `x` exactly
/// (`-0` and `0` both equal either zero).
fn numeral_equals_float(text: &str, x: f64, bits: u8) -> bool {
    let (negative, int, frac) = normalize(text);
    if int.is_empty() && frac.is_empty() {
        return x == 0.0;
    }
    if x == 0.0 || negative != x.is_sign_negative() {
        return false;
    }
    let w = width(bits);
    let (fraction, biased) = fields(x, w);
    let d = Dyadic::of_fields(fraction, biased, w.fraction_bits, w.bias);
    cmp_numeral(&int, &frac, &d) == Ordering::Equal
}

/// Whether `x` (a float of width `bits`, possibly infinite) is the IEEE 754
/// round-to-nearest-even reading of the finite numeral `text` at that
/// width: `|t|` lies within half the gap to each neighbour of `|x|`, on a
/// boundary only when the significand of `x` is even; an infinity exactly
/// when `|t|` reaches the largest finite value plus half its gap; and a
/// zero, of the numeral's written sign, exactly when `|t|` is at most half
/// the least subnormal.
fn is_nearest_even(text: &str, x: f64, bits: u8) -> bool {
    let (_, int, frac) = normalize(text);
    let w = width(bits);
    let half_gap_from = |fraction: u64, biased: i64| {
        let at = Dyadic::of_fields(fraction, biased, w.fraction_bits, w.bias);
        let up = if fraction == (1 << w.fraction_bits) - 1 {
            Dyadic::of_fields(0, biased + 1, w.fraction_bits, w.bias)
        } else {
            Dyadic::of_fields(fraction + 1, biased, w.fraction_bits, w.bias)
        };
        at.midpoint(&up)
    };
    let max_fraction = (1 << w.fraction_bits) - 1;
    let overflow_at = half_gap_from(max_fraction, w.max_biased);
    if x.is_infinite() {
        return written_negative(text) == x.is_sign_negative()
            && cmp_numeral(&int, &frac, &overflow_at) != Ordering::Less;
    }
    let sign_ok = x.is_sign_negative() == written_negative(text);
    if x == 0.0 {
        let half_least = half_gap_from(0, 0);
        return sign_ok && cmp_numeral(&int, &frac, &half_least) != Ordering::Greater;
    }
    let (fraction, biased) = fields(x, w);
    let even = fraction & 1 == 0;
    let up = half_gap_from(fraction, biased);
    let below = if fraction == 0 {
        // The gap below the least value of a binade is that of the binade
        // under it (the subnormals share the least normal binade's gap).
        let lower = if biased > 1 {
            Dyadic::of_fields(max_fraction, biased - 1, w.fraction_bits, w.bias)
        } else {
            Dyadic::of_fields(max_fraction, 0, w.fraction_bits, w.bias)
        };
        Dyadic::of_fields(0, biased, w.fraction_bits, w.bias).midpoint(&lower)
    } else {
        Dyadic::of_fields(fraction - 1, biased, w.fraction_bits, w.bias).midpoint(
            &Dyadic::of_fields(fraction, biased, w.fraction_bits, w.bias),
        )
    };
    let above_low = match cmp_numeral(&int, &frac, &below) {
        Ordering::Greater => true,
        Ordering::Equal => even,
        Ordering::Less => false,
    };
    let below_high = match cmp_numeral(&int, &frac, &up) {
        Ordering::Less => true,
        Ordering::Equal => even,
        Ordering::Greater => false,
    };
    sign_ok && above_low && below_high
}

/// Whether two finite numerals are numerically equal.
fn numerals_equal(a: &str, b: &str) -> bool {
    normalize(a) == normalize(b)
}

/// The reading of a finite numeral at a float kind of `bits` bits, widened
/// to `f64`: Rust's parser's result, checked here to be the
/// round-to-nearest-even reading; the zero of a numeral written with `-`
/// is the negative zero. Then the shortest text of that value at the
/// width, Rust's `{:e}` (the closest-shortest text, ties away from zero),
/// checked to read back to the value, and the significant digits that
/// text may have at most.
fn read_float(text: &str, bits: u8) -> (f64, String, u32) {
    let (x, shortest, max) = if bits == 32 {
        let x = text.parse::<f32>().unwrap();
        (f64::from(x), format!("{x:e}"), 9)
    } else {
        let x = text.parse::<f64>().unwrap();
        (x, format!("{x:e}"), 17)
    };
    assert!(
        is_nearest_even(text, x, bits),
        "{text} does not read as {x:e} at {bits} bits"
    );
    if x.is_finite() {
        assert!(
            is_nearest_even(&shortest, x, bits),
            "{shortest} does not read back as {x:e}"
        );
    }
    (x, shortest, max)
}

/// What a float kind of `bits` bits must do with the finite numeral `text`
/// (law (i)): the float it reads, or the reason it is refused.
fn expected_float(text: &str, bits: u8) -> Result<f64, FidelityReason> {
    let (x, shortest, max) = read_float(text, bits);
    if !x.is_finite() {
        return Err(FidelityReason::Overflow);
    }
    if numeral_equals_float(text, x, bits) || numerals_equal(&shortest, text) {
        Ok(x)
    } else {
        Err(FidelityReason::PrecisionExceeded { max })
    }
}

/// Whether the finite or infinite `x` is a binary32 value widened, decided
/// by writing |x| as m * 2^e with m odd: m below 2^24, e at least -149 and
/// the value at most the largest binary32, (2^24 - 1) * 2^104; or x is an
/// infinity, a zero, or a NaN with the low 29 fraction bits zero.
fn is_binary32(x: f64) -> bool {
    let bits = x.to_bits();
    if x.is_nan() {
        // Widening quiets a NaN: a widened binary32 NaN has the quiet bit
        // (fraction bit 51) set and its low 29 fraction bits zero.
        return bits & (1 << 51) != 0 && bits & ((1 << 29) - 1) == 0;
    }
    if x.is_infinite() || x == 0.0 {
        return true;
    }
    let (fraction, biased) = (
        bits & ((1 << 52) - 1),
        i64::try_from((bits >> 52) & 0x7ff).unwrap(),
    );
    let (mut m, mut e) = if biased == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1 << 52), biased - 1075)
    };
    while m % 2 == 0 {
        m /= 2;
        e += 1;
    }
    m < (1 << 24) && e >= -149 && i64::from(64 - m.leading_zeros()) + e <= 128
}

/// The least magnitude that rounds to an infinity at binary32, 2^128 -
/// 2^103: the largest binary32, (2^24 - 1) 2^104, plus half its gap 2^104.
const BINARY32_OVERFLOW: f64 = 340_282_356_779_733_661_637_539_395_458_142_568_448.0;

/// What a decimal kind with `precision` and `scale` must do with `text`.
fn expected_decimal(
    text: &str,
    precision: Option<u32>,
    scale: Option<u32>,
) -> Result<(), FidelityReason> {
    let (_, int, frac) = normalize(text);
    let frac_len = u32::try_from(frac.len()).unwrap();
    let int_len = u32::try_from(int.len()).unwrap();
    if let Some(s) = scale
        && frac_len > s
    {
        return Err(FidelityReason::ScaleExceeded { max: s });
    }
    if let Some(p) = precision {
        // Without a scale, the digits the number needs in positional notation.
        let fits = scale.map_or(int_len + frac_len <= p, |s| int_len <= p.saturating_sub(s));
        if !fits {
            return Err(FidelityReason::PrecisionExceeded { max: p });
        }
    }
    Ok(())
}

/// The integer a numeral writes, as its sign and digits without leading
/// zeros (empty for zero), or `None` when it is not integral.
fn integer_of(text: &str) -> Option<(bool, String)> {
    let (negative, int, frac) = normalize(text);
    frac.is_empty().then_some((negative, int))
}

/// The integer `(negative, digits)` as an `i128`, when it fits.
fn as_i128((negative, digits): &(bool, String)) -> Option<i128> {
    if digits.is_empty() {
        return Some(0);
    }
    let sign = if *negative { "-" } else { "" };
    format!("{sign}{digits}").parse::<i128>().ok()
}

/// Whether an integer kind of `bits` bits holds `n`.
fn int_holds(n: i128, bits: u8, signed: bool) -> bool {
    let bits = u32::from(bits);
    if bits >= 128 {
        return signed || n >= 0;
    }
    if signed {
        let half = 1_i128 << (bits - 1);
        -half <= n && n < half
    } else {
        n >= 0 && n < (1_i128 << bits)
    }
}

/// The integer value of `(negative, digits)`: `Int` when it fits `i64`,
/// else `BigInt`.
fn integer_value((negative, digits): (bool, String)) -> Value {
    let digits = if digits.is_empty() {
        "0".to_owned()
    } else {
        digits
    };
    let sign = if negative { "-" } else { "" };
    Value::parse_integer(&format!("{sign}{digits}")).unwrap()
}

// ---------------------------------------------------------------------------
// Specs, generators and comparisons.
// ---------------------------------------------------------------------------

/// One spec of every scalar kind, with the parameters that change what a
/// kind holds.
fn specs() -> Vec<ValueSpec> {
    let spec = ValueSpec::new;
    let int = |bits, signed| spec(ScalarKind::Int { bits, signed });
    vec![
        spec(ScalarKind::Bool),
        int(8, true),
        int(16, false),
        int(32, true),
        int(64, true),
        int(64, false),
        int(128, true),
        spec(ScalarKind::BigInt),
        spec(ScalarKind::Float { bits: 32 }),
        spec(ScalarKind::Float { bits: 64 }),
        spec(ScalarKind::Decimal),
        ValueSpec {
            precision: Some(5),
            scale: Some(2),
            ..spec(ScalarKind::Decimal)
        },
        ValueSpec {
            precision: Some(4),
            ..spec(ScalarKind::Decimal)
        },
        spec(ScalarKind::Str),
        spec(ScalarKind::Token),
        spec(ScalarKind::Bytes(BytesText::Hex)),
        spec(ScalarKind::Bytes(BytesText::Base64)),
        spec(ScalarKind::Date),
        spec(ScalarKind::Time),
        spec(ScalarKind::DateTime),
        ValueSpec {
            with_time_zone: Some(true),
            ..spec(ScalarKind::DateTime)
        },
        spec(ScalarKind::Any),
    ]
}

/// Finite numerals a float kind must decide: shortest texts and exact
/// expansions of floats of both widths, arbitrary numerals, and the
/// boundary cases.
fn float_numerals() -> impl Strategy<Value = String> {
    let finite64 = any::<f64>().prop_filter("finite", |x| x.is_finite());
    let finite32 = any::<f32>().prop_filter("finite", |x| x.is_finite());
    prop_oneof![
        finite64.clone().prop_map(|x| format!("{x:e}")),
        finite64.clone().prop_map(|x| format!("{x:.1100e}")),
        finite32.clone().prop_map(|x| format!("{x:e}")),
        finite32
            .clone()
            .prop_map(|x| format!("{:.200e}", f64::from(x))),
        finite32.prop_map(|x| format!("{}", f64::from(x))),
        "-?[0-9]{1,20}(\\.[0-9]{1,20})?([eE]-?[0-9]{1,2})?",
        "-?[0-9]{1,3}\\.[0-9]{1,12}",
        prop::sample::select(vec![
            "0.1",
            "-0.0",
            "-0",
            "0e5",
            "16777217.0",
            "0.10000000149011612",
            "9007199254740992",
            "1125899906842624",
            "3.4028235e38",
            "3.4028236e38",
            "1e39",
            "1e-45",
            "1e-46",
            "7e-46",
            "4e-324",
            "5e-324",
            "2.4703282292062328e-324",
            "1e-400",
            "1e400",
            "1.7976931348623157e308",
            "1.7976931348623159e308",
        ])
        .prop_map(str::to_owned),
    ]
}

/// Numerals for the decimal and integer kinds, integral and not.
fn exact_numerals() -> impl Strategy<Value = String> {
    prop_oneof![
        "-?[0-9]{1,22}",
        "-?[0-9]{1,8}\\.[0-9]{0,8}",
        "-?[0-9]{1,6}(\\.[0-9]{1,4})?[eE]-?[0-9]",
        prop::sample::select(vec![
            "0", "-0", "0.000", "1.0", "1e3", "-12.50e1", "0e5", "1.005"
        ])
        .prop_map(str::to_owned),
    ]
}

/// Texts that some kind reads: numerals, booleans, dates, encodings, words.
fn texts() -> impl Strategy<Value = String> {
    prop_oneof![
        any::<i64>().prop_map(|n| n.to_string()),
        "-?[0-9]{1,3}\\.[0-9]{1,4}",
        "[0-9]{19,25}",
        Just("true".to_owned()),
        Just("FALSE".to_owned()),
        Just("2024-05-06".to_owned()),
        Just("12:30:05".to_owned()),
        Just("23:59:60.123+05:30".to_owned()),
        Just("2024-05-06T12:30:05Z".to_owned()),
        Just("2024-05-06T12:30:05-00:00".to_owned()),
        Just("2024-05-06T12:30:05".to_owned()),
        "([0-9a-f]{2}){0,4}",
        Just("aGk=".to_owned()),
        "[a-z]{0,5}",
    ]
}

/// A non-finite `f64`, any sign and payload.
fn non_finite() -> impl Strategy<Value = f64> {
    (any::<bool>(), 0_u64..(1 << 52)).prop_map(|(negative, fraction)| {
        f64::from_bits((u64::from(negative) << 63) | (0x7ff << 52) | fraction)
    })
}

fn values() -> impl Strategy<Value = Value> {
    prop_oneof![
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::Int),
        "[1-9][0-9]{19,24}".prop_map(|t| Value::parse_integer(&t).unwrap()),
        "-?[0-9]{1,4}\\.[0-9]{1,5}".prop_map(|t| Value::Decimal(DecimalValue::parse(&t).unwrap())),
        any::<f64>().prop_map(Value::Float),
        any::<i32>().prop_map(|n| Value::Float(f64::from(n))),
        any::<f32>().prop_map(|f| Value::Float(f64::from(f))),
        non_finite().prop_map(Value::Float),
        prop::sample::select(vec![-0.0, 0.1, 1e300, 1e39, 16_777_217.0, 0.123_456_789])
            .prop_map(Value::Float),
        texts().prop_map(Value::Str),
        texts().prop_map(Value::Token),
        Just(Value::Temporal(
            Temporal::parse("2024-05-06T12:30:05+02:00").unwrap()
        )),
        Just(Value::Temporal(Temporal::parse("2024-05-06").unwrap())),
        Just(Value::Null),
        no_lexeme_values(),
    ]
}

/// Values with no scalar lexeme.
fn no_lexeme_values() -> impl Strategy<Value = Value> {
    prop_oneof![
        prop::collection::vec(any::<u8>(), 0..4).prop_map(Value::Bytes),
        Just(Value::List(vec![Value::Int(1)])),
        Just(Value::List(vec![Value::Float(f64::NAN)])),
        any::<u64>().prop_map(Value::LabeledNull),
        Just(Value::Opaque {
            type_: "point".into(),
            fields: std::iter::once(("x".to_owned(), Value::Int(1))).collect(),
        }),
    ]
}

fn same(a: &Value, b: &Value) -> bool {
    a.canonical_cmp(b).is_eq()
}

fn site() -> ValueSite {
    ValueSite::new("v", None)
}

/// The finite float `bits` reads `result` as, or a failed comparison.
fn assert_float_result(
    result: &Result<Value, panproto_inst::ValueFidelityError>,
    expected: &Result<f64, FidelityReason>,
) -> Result<(), TestCaseError> {
    match (result, expected) {
        (Ok(Value::Float(y)), Ok(x)) => prop_assert_eq!(y.to_bits(), x.to_bits()),
        (Err(e), Err(reason)) => prop_assert_eq!(&e.reason, reason),
        _ => prop_assert!(false, "got {result:?}, expected {expected:?}"),
    }
    Ok(())
}

/// What a numeric kind must do with the finite numeral `text`, read as
/// the kind's value: `None` when the kind is not numeric.
fn expected_numeral(text: &str, spec: &ValueSpec) -> Option<Result<Value, FidelityReason>> {
    let integral = |n: Option<(bool, String)>| n.ok_or(FidelityReason::ScaleExceeded { max: 0 });
    Some(match &spec.kind {
        ScalarKind::Float { bits } => expected_float(text, *bits).map(Value::Float),
        ScalarKind::Decimal => expected_decimal(text, spec.precision, spec.scale)
            .map(|()| Value::Decimal(DecimalValue::parse(text).unwrap())),
        ScalarKind::BigInt => integral(integer_of(text)).map(integer_value),
        ScalarKind::Int { bits, signed } => integral(integer_of(text)).and_then(|n| {
            if as_i128(&n).is_some_and(|m| int_holds(m, *bits, *signed)) {
                Ok(integer_value(n))
            } else {
                Err(FidelityReason::Overflow)
            }
        }),
        _ => return None,
    })
}

/// What `coerce_value` must do with a numeric `value` under a numeric
/// kind (law (i), with the narrowing rule of a 32-bit float kind): `None`
/// when the value or the kind is not numeric, or the value is a non-finite
/// float.
fn expected_value(value: &Value, spec: &ValueSpec) -> Option<Result<Value, FidelityReason>> {
    let text = match value {
        Value::Float(x) if !x.is_finite() => return None,
        Value::Float(x) => {
            if spec.kind == (ScalarKind::Float { bits: 32 }) {
                return Some(if is_binary32(*x) {
                    Ok(value.clone())
                } else if x.abs() >= BINARY32_OVERFLOW {
                    Err(FidelityReason::Overflow)
                } else {
                    Err(FidelityReason::PrecisionExceeded { max: 9 })
                });
            }
            if spec.kind == (ScalarKind::Float { bits: 64 }) {
                return Some(Ok(value.clone()));
            }
            format!("{x}")
        }
        Value::Int(n) => n.to_string(),
        Value::BigInt(n) => n.to_string(),
        Value::Decimal(d) => d.to_string(),
        _ => return None,
    };
    expected_numeral(&text, spec)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// Law (i) for `coerce` under the float, decimal and integer kinds,
    /// with and without facets, and for `coerce_value` on numeric values.
    #[test]
    fn coercion_has_lexical_fidelity(
        float in float_numerals(),
        exact in exact_numerals(),
        precision in prop::option::of(1_u32..12),
        scale in prop::option::of(0_u32..6),
        int_bits in prop::sample::select(vec![8_u8, 16, 32, 64, 128]),
        signed in any::<bool>(),
        value in values(),
        index in 0..32_usize,
    ) {
        for bits in [32, 64] {
            let spec = ValueSpec::new(ScalarKind::Float { bits });
            let result = coerce(&Lexeme::new(&float, LexClass::Number), &spec);
            assert_float_result(&result, &expected_float(&float, bits))?;
        }
        let kinds = [
            ValueSpec { precision, scale, ..ValueSpec::new(ScalarKind::Decimal) },
            ValueSpec::new(ScalarKind::Decimal),
            ValueSpec::new(ScalarKind::BigInt),
            ValueSpec::new(ScalarKind::Int { bits: int_bits, signed }),
        ];
        for spec in &kinds {
            let result = coerce(&Lexeme::new(&exact, LexClass::Number), spec);
            let expected = expected_numeral(&exact, spec).unwrap();
            match (&result, &expected) {
                (Ok(got), Ok(want)) => prop_assert!(same(got, want), "{exact} as {spec:?}: {got:?}"),
                (Err(e), Err(reason)) => prop_assert_eq!(&e.reason, reason, "{} as {:?}", exact, spec),
                _ => prop_assert!(false, "{exact} as {spec:?}: got {result:?}, expected {expected:?}"),
            }
        }
        let all = specs();
        let spec = &all[index % all.len()];
        if let Some(expected) = expected_value(&value, spec) {
            let result = coerce_value(&value, spec, site());
            match (&result, &expected) {
                (Ok(Value::Float(got)), Ok(Value::Float(want))) => {
                    prop_assert_eq!(got.to_bits(), want.to_bits(), "{:?} as {:?}", value, spec.kind);
                }
                (Ok(got), Ok(want)) => prop_assert!(same(got, want), "{value:?} as {spec:?}: {got:?}"),
                (Err(e), Err(reason)) => prop_assert_eq!(&e.reason, reason, "{:?} as {:?}", value, spec),
                _ => prop_assert!(false, "{value:?} as {spec:?}: got {result:?}, expected {expected:?}"),
            }
        }
    }

    /// Law (ii): coercing a coerced value again returns it unchanged.
    #[test]
    fn coerce_value_is_idempotent(value in values(), index in 0..32_usize) {
        let all = specs();
        let spec = &all[index % all.len()];
        if let Ok(once) = coerce_value(&value, spec, site()) {
            let twice = coerce_value(&once, spec, site());
            prop_assert!(
                twice.as_ref().is_ok_and(|t| same(t, &once)),
                "{value:?} as {:?}: {once:?} then {twice:?}", spec.kind
            );
        }
    }

    /// Law (iii): every value `coerce` returns under a kind is a fixed
    /// point of `coerce_value` under that kind, by canonical bytes.
    #[test]
    fn coerced_values_are_fixed_points(
        text in prop_oneof![texts(), float_numerals(), exact_numerals()],
        class in prop::sample::select(vec![
            LexClass::Number, LexClass::String, LexClass::DateTime, LexClass::Bool,
        ]),
    ) {
        for spec in specs() {
            if let Ok(read) = coerce(&Lexeme::new(&text, class.clone()), &spec) {
                let again = coerce_value(&read, &spec, site());
                prop_assert!(
                    again.as_ref().is_ok_and(|v| same(v, &read)),
                    "{text:?} ({class:?}) as {:?}: {read:?} then {again:?}", spec.kind
                );
            }
        }
    }

    /// Outside the laws' domain values pass through: every value under
    /// `Any`; a non-finite float under a float kind that holds it, sign and
    /// payload included (a 32-bit kind refuses a NaN whose payload uses
    /// the low 29 fraction bits); a value with no scalar lexeme under
    /// every kind.
    #[test]
    fn coerce_value_passes_through_outside_its_domain(
        value in values(),
        nan_or_inf in non_finite(),
        opaque in no_lexeme_values(),
    ) {
        let any = ValueSpec::new(ScalarKind::Any);
        let kept = coerce_value(&value, &any, site()).unwrap();
        prop_assert!(same(&kept, &value));
        let float = Value::Float(nan_or_inf);
        let kept = coerce_value(&float, &ValueSpec::new(ScalarKind::Float { bits: 64 }), site());
        prop_assert!(kept.is_ok_and(|v| same(&v, &float)));
        let narrow = coerce_value(&float, &ValueSpec::new(ScalarKind::Float { bits: 32 }), site());
        if is_binary32(nan_or_inf) {
            prop_assert!(narrow.is_ok_and(|v| same(&v, &float)));
        } else {
            prop_assert_eq!(narrow.unwrap_err().reason, FidelityReason::NonFinite);
        }
        for spec in specs() {
            let kept = coerce_value(&opaque, &spec, site());
            prop_assert!(kept.is_ok_and(|v| same(&v, &opaque)));
        }
    }

    /// The two routes into a 32-bit float agree wherever both succeed: a
    /// numeral read directly, and the same numeral read as a 64-bit float
    /// and then narrowed; and an integer or decimal value coerced directly
    /// or through a 64-bit float.
    #[test]
    fn float32_routes_agree_on_ok_results(
        float in float_numerals(),
        exact in exact_numerals(),
    ) {
        let f32_ = ValueSpec::new(ScalarKind::Float { bits: 32 });
        let f64_ = ValueSpec::new(ScalarKind::Float { bits: 64 });
        let direct = coerce(&Lexeme::new(&float, LexClass::Number), &f32_);
        let path = coerce(&Lexeme::new(&float, LexClass::Number), &f64_)
            .and_then(|wide| coerce_value(&wide, &f32_, site()));
        if let (Ok(a), Ok(b)) = (&direct, &path) {
            prop_assert!(same(a, b), "{float}: {a:?} and {b:?}");
        }
        let inputs = [
            Value::Decimal(DecimalValue::parse(&exact).unwrap()),
            coerce(&Lexeme::new(&exact, LexClass::Number), &ValueSpec::new(ScalarKind::BigInt))
                .unwrap_or(Value::Null),
        ];
        for input in inputs.iter().filter(|v| **v != Value::Null) {
            for target in [&f32_, &f64_] {
                // An integer or decimal input is coerced as its lexeme.
                let lexeme = match input { Value::Decimal(d) => d.to_string(), other => integer_text(other) };
                prop_assert!(same_result(
                    &coerce_value(input, target, site()),
                    &coerce(&Lexeme::new(&lexeme, LexClass::Number), target),
                ), "{input:?} as {target:?}");
                let direct = coerce_value(input, target, site());
                let path = coerce_value(input, &f64_, site())
                    .and_then(|wide| coerce_value(&wide, target, site()));
                if let (Ok(a), Ok(b)) = (&direct, &path) {
                    prop_assert!(same(a, b), "{input:?}: {a:?} and {b:?}");
                }
            }
        }
    }
}

#[test]
fn coerce_value_refuses_a_float_that_is_not_a_binary32() {
    let f32_ = ValueSpec::new(ScalarKind::Float { bits: 32 });
    for x in [0.1, 1.1, 0.123_456_789, 16_777_217.0, 1e-46] {
        let error = coerce_value(&Value::Float(x), &f32_, site()).unwrap_err();
        assert_eq!(
            error.reason,
            FidelityReason::PrecisionExceeded { max: 9 },
            "{x}"
        );
        assert_eq!(error.site, site());
    }
    let error = coerce_value(&Value::Float(1e39), &f32_, site()).unwrap_err();
    assert_eq!(error.reason, FidelityReason::Overflow);
    // The overflow threshold is where binary32 rounding gives an infinity,
    // 2^128 - 2^103, as the lexeme route decides: the binary64 just below
    // it rounds to the largest binary32 and is refused for precision.
    let threshold = BINARY32_OVERFLOW;
    assert_eq!(threshold.to_bits(), 0x47ef_ffff_f000_0000);
    let below = f64::from_bits(threshold.to_bits() - 1);
    for (x, reason) in [
        (threshold, FidelityReason::Overflow),
        (-threshold, FidelityReason::Overflow),
        (below, FidelityReason::PrecisionExceeded { max: 9 }),
        (-below, FidelityReason::PrecisionExceeded { max: 9 }),
    ] {
        let error = coerce_value(&Value::Float(x), &f32_, site()).unwrap_err();
        assert_eq!(error.reason, reason, "{x:e}");
        // The exact decimal text of x (x is an integer here).
        let exact = format!("{x:.0}");
        let lexeme = coerce(&Lexeme::new(&exact, LexClass::Number), &f32_);
        assert_eq!(lexeme.unwrap_err().reason, reason, "{exact} as a lexeme");
    }
    let largest = Value::Float(f64::from(f32::MAX));
    assert!(same(
        &coerce_value(&largest, &f32_, site()).unwrap(),
        &largest
    ));
    // Powers of two, whose fraction bits are all zero: in range only from
    // 2^-149 to 2^127.
    for (power, reason) in [
        (128, Some(FidelityReason::Overflow)),
        (200, Some(FidelityReason::Overflow)),
        (1023, Some(FidelityReason::Overflow)),
        (127, None),
        (-126, None),
        (-140, None),
        (-149, None),
        (-150, Some(FidelityReason::PrecisionExceeded { max: 9 })),
        (-200, Some(FidelityReason::PrecisionExceeded { max: 9 })),
        (-1074, Some(FidelityReason::PrecisionExceeded { max: 9 })),
    ] {
        // 2^power, from its bits (normal, or subnormal below 2^-1022).
        let x = if power >= -1022 {
            f64::from_bits(u64::try_from(power + 1023).unwrap() << 52)
        } else {
            f64::from_bits(1 << (power + 1074))
        };
        let result = coerce_value(&Value::Float(x), &f32_, site());
        match reason {
            Some(reason) => assert_eq!(result.unwrap_err().reason, reason, "2^{power}"),
            None => assert!(same(&result.unwrap(), &Value::Float(x)), "2^{power}"),
        }
    }
    for x in [
        0.100_000_001_490_116_12,
        -0.0,
        3.402_823_466_385_288_6e38,
        1.401_298_464_324_817e-45,
        16_777_216.0,
    ] {
        let kept = coerce_value(&Value::Float(x), &f32_, site()).unwrap();
        let Value::Float(y) = kept else {
            panic!("a float")
        };
        assert_eq!(y.to_bits(), x.to_bits(), "{x}");
    }
    // A decimal or integer keeps the lexeme rule: `0.1` reads as the
    // binary32 nearest it.
    let tenth = Value::Decimal(DecimalValue::parse("0.1").unwrap());
    assert!(same(
        &coerce_value(&tenth, &f32_, site()).unwrap(),
        &Value::Float(f64::from(0.1_f32))
    ));
}

#[test]
fn float32_route_refusals_differ_in_both_directions() {
    let f32_ = ValueSpec::new(ScalarKind::Float { bits: 32 });
    let f64_ = ValueSpec::new(ScalarKind::Float { bits: 64 });
    let direct = |t: &str| coerce(&Lexeme::new(t, LexClass::Number), &f32_);
    let path = |t: &str| {
        coerce(&Lexeme::new(t, LexClass::Number), &f64_)
            .and_then(|wide| coerce_value(&wide, &f32_, site()))
    };
    let tenth32 = Value::Float(f64::from(0.1_f32));
    assert!(direct("0.10000000149011612").is_err());
    assert!(same(&path("0.10000000149011612").unwrap(), &tenth32));
    assert!(same(&direct("0.1").unwrap(), &tenth32));
    assert!(path("0.1").is_err());
    let exact_tenth64 = "0.1000000000000000055511151231257827021181583404541015625";
    assert!(same(
        &coerce(&Lexeme::new(exact_tenth64, LexClass::Number), &f64_).unwrap(),
        &Value::Float(0.1)
    ));
    assert!(path(exact_tenth64).is_err());
    assert!(direct(exact_tenth64).is_err());
}

#[test]
fn a_precision_refusal_says_what_the_kind_cannot_hold() {
    let f32_ = ValueSpec::new(ScalarKind::Float { bits: 32 });
    // "16777217.0" has eight significant digits, so the message cannot
    // claim more than nine.
    let error = coerce(&Lexeme::new("16777217.0", LexClass::Number), &f32_).unwrap_err();
    assert_eq!(
        error.reason.to_string(),
        "not exactly a value of the kind, whose values are written in at most 9 significant digits"
    );
    let money = ValueSpec {
        precision: Some(3),
        ..ValueSpec::new(ScalarKind::Decimal)
    };
    let error = coerce(&Lexeme::new("1.234", LexClass::Number), &money).unwrap_err();
    assert_eq!(
        error.reason.to_string(),
        "not exactly a value of the kind, whose values are written in at most 3 significant digits"
    );
}

#[test]
fn a_non_finite_float_is_kept_only_by_a_float_kind() {
    let float = ValueSpec::new(ScalarKind::Float { bits: 64 });
    let decimal = ValueSpec::new(ScalarKind::Decimal);
    let nan = Value::Float(f64::NAN);
    assert!(same(&coerce_value(&nan, &float, site()).unwrap(), &nan));
    let error = coerce_value(&nan, &decimal, site()).unwrap_err();
    assert_eq!(error.reason, FidelityReason::NonFinite);
    assert_eq!(error.site.vertex.as_ref(), "v");
}

#[test]
fn a_nan_binary32_cannot_hold_is_refused_under_float32() {
    let f32_ = ValueSpec::new(ScalarKind::Float { bits: 32 });
    let f64_ = ValueSpec::new(ScalarKind::Float { bits: 64 });
    let payload = Value::Float(f64::from_bits(0x7ff8_0000_0000_1234));
    let error = coerce_value(&payload, &f32_, site()).unwrap_err();
    assert_eq!(error.reason, FidelityReason::NonFinite);
    assert!(same(
        &coerce_value(&payload, &f64_, site()).unwrap(),
        &payload
    ));
    // A signaling NaN whose low 29 fraction bits are zero is no widened
    // binary32 either: widening quiets a NaN.
    let signaling = Value::Float(f64::from_bits(0x7ff0_0000_2000_0000));
    let error = coerce_value(&signaling, &f32_, site()).unwrap_err();
    assert_eq!(error.reason, FidelityReason::NonFinite);
    assert_eq!(
        coerce_value(&signaling, &f64_, site())
            .unwrap()
            .canonical_bytes(),
        signaling.canonical_bytes()
    );
    for bits in [
        0x7ff8_0000_0000_0000_u64,
        0xfff8_0000_0000_0000,
        0x7ff8_0000_2000_0000,
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
    ] {
        let kept = Value::Float(f64::from_bits(bits));
        for spec in [&f32_, &f64_] {
            assert!(
                same(&coerce_value(&kept, spec, site()).unwrap(), &kept),
                "{bits:x}"
            );
        }
    }
}

/// The decimal digits of an integer value.
fn integer_text(value: &Value) -> String {
    match value {
        Value::Int(n) => n.to_string(),
        Value::BigInt(n) => n.to_string(),
        other => panic!("not an integer: {other:?}"),
    }
}

/// Whether two coercion results agree: equal values by canonical bytes,
/// or refusals for one reason.
fn same_result(
    a: &Result<Value, panproto_inst::ValueFidelityError>,
    b: &Result<Value, panproto_inst::ValueFidelityError>,
) -> bool {
    match (a, b) {
        (Ok(x), Ok(y)) => same(x, y),
        (Err(x), Err(y)) => x.reason == y.reason,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Exhaustive checks on bounded scopes, enumerated by plain loops.
// ---------------------------------------------------------------------------

/// Every numeral of the bounded float scope: (a) `{m}e{k}` and `-{m}e{k}`
/// for every integer m in 0..=999 and every k in -48..=-36 (32-bit
/// subnormals and underflow), -6..=6 and 30..=41 (32-bit overflow); (b) for every positive binary32
/// whose bit pattern is a multiple of 2^18 below the infinity, its shortest
/// text, that text with its last digit raised and lowered by one, its exact
/// decimal expansion, and the exact expansion with a 1 appended; (c) the
/// same five texts for every binary64 whose bit pattern is a multiple of
/// 2^50 below the infinity.
fn bounded_float_numerals() -> Vec<String> {
    let mut out = Vec::new();
    for m in 0..=999_u32 {
        for k in (-48..=-36_i32).chain(-6..=6).chain(30..=41) {
            out.push(format!("{m}e{k}"));
            out.push(format!("-{m}e{k}"));
        }
    }
    let around = |out: &mut Vec<String>, shortest: String, exact: String| {
        let (mantissa, exponent) = shortest.split_once('e').unwrap();
        let last = mantissa.bytes().last().unwrap() - b'0';
        let stem = &mantissa[..mantissa.len() - 1];
        if last < 9 {
            out.push(format!("{stem}{}e{exponent}", last + 1));
        }
        if last > 0 {
            out.push(format!("{stem}{}e{exponent}", last - 1));
        }
        out.push(format!("{}1", exact.trim_end_matches('0')));
        out.push(shortest);
        out.push(exact);
    };
    for i in 1..(0x7f80_0000_u32 >> 18) {
        let x = f32::from_bits(i << 18);
        around(&mut out, format!("{x:e}"), format!("{:.200}", f64::from(x)));
    }
    for i in 1..(0x7ff0_0000_0000_0000_u64 >> 50) {
        let x = f64::from_bits(i << 50);
        around(&mut out, format!("{x:e}"), format!("{x:.1100}"));
    }
    out
}

/// Law (i) for `coerce` under both float kinds, on every numeral of
/// [`bounded_float_numerals`].
#[test]
fn float_lexemes_follow_law_i_on_a_bounded_scope() {
    let numerals = bounded_float_numerals();
    let mut checked = 0_usize;
    for text in &numerals {
        for bits in [32, 64] {
            let spec = ValueSpec::new(ScalarKind::Float { bits });
            let result = coerce(&Lexeme::new(text, LexClass::Number), &spec);
            let expected = expected_float(text, bits);
            match (&result, &expected) {
                (Ok(Value::Float(y)), Ok(x)) => assert_eq!(y.to_bits(), x.to_bits(), "{text}"),
                (Err(e), Err(reason)) => assert_eq!(&e.reason, reason, "{text} at {bits}"),
                _ => panic!("{text} at {bits}: got {result:?}, expected {expected:?}"),
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 2 * numerals.len());
    assert!(numerals.len() > 140_000, "{}", numerals.len());
}

/// The two routes into a 32-bit float agree on every `Ok` result over the
/// bounded scope, and an integer or decimal value is coerced to each float
/// kind as its lexeme is.
#[test]
fn float32_routes_agree_on_a_bounded_scope() {
    let f32_ = ValueSpec::new(ScalarKind::Float { bits: 32 });
    let f64_ = ValueSpec::new(ScalarKind::Float { bits: 64 });
    let (mut both, mut direct_only, mut path_only) = (0_u32, 0_u32, 0_u32);
    for text in bounded_float_numerals() {
        let direct = coerce(&Lexeme::new(&text, LexClass::Number), &f32_);
        let path = coerce(&Lexeme::new(&text, LexClass::Number), &f64_)
            .and_then(|wide| coerce_value(&wide, &f32_, site()));
        match (&direct, &path) {
            (Ok(a), Ok(b)) => {
                assert!(same(a, b), "{text}: {a:?} and {b:?}");
                let (Value::Float(a), Value::Float(b)) = (a, b) else {
                    panic!("floats")
                };
                assert_eq!(a.to_bits(), b.to_bits(), "{text}");
                both += 1;
            }
            (Ok(_), Err(_)) => direct_only += 1,
            (Err(_), Ok(_)) => path_only += 1,
            (Err(_), Err(_)) => {}
        }
    }
    // The refusal domains differ in both directions.
    assert!(
        both > 0 && direct_only > 0 && path_only > 0,
        "{both} {direct_only} {path_only}"
    );
    for m in -300..=300_i64 {
        for k in -4..=4_i32 {
            let decimal = Value::Decimal(DecimalValue::parse(&format!("{m}e{k}")).unwrap());
            for input in [Value::Int(m), decimal] {
                let lexeme = match &input {
                    Value::Decimal(d) => d.to_string(),
                    other => integer_text(other),
                };
                for target in [&f32_, &f64_] {
                    assert!(
                        same_result(
                            &coerce_value(&input, target, site()),
                            &coerce(&Lexeme::new(&lexeme, LexClass::Number), target),
                        ),
                        "{input:?} as {target:?}"
                    );
                }
            }
        }
    }
}

/// Values of the bounded scope: integers -300..=300 and four beyond `i64`;
/// decimals `{m}e{k}` and `{m}.{f}`; every binary32 whose bit pattern is a
/// multiple of 2^18, of both signs, widened; every binary64 whose bit
/// pattern is a multiple of 2^50, of both signs; the non-finite floats of
/// [`special_nans`]; booleans, strings and tokens over a small alphabet;
/// temporals of each form and offset; null; and the values with no scalar
/// lexeme.
fn bounded_values() -> Vec<Value> {
    let mut out = vec![Value::Null, Value::Bool(true), Value::Bool(false)];
    out.extend((-300..=300).map(Value::Int));
    for big in [
        "9223372036854775808",
        "-9223372036854775809",
        "340282366920938463463374607431768211456",
        "18446744073709551615",
    ] {
        out.push(Value::parse_integer(big).unwrap());
    }
    for m in [-1005, -150, -1, 0, 1, 7, 15, 150, 999, 12_345] {
        for k in -4..=3 {
            out.push(Value::Decimal(
                DecimalValue::parse(&format!("{m}e{k}")).unwrap(),
            ));
        }
        out.push(Value::Decimal(
            DecimalValue::parse(&format!("{m}.00")).unwrap(),
        ));
    }
    for i in 0..(0x7f80_0000_u32 >> 18) {
        let x = f64::from(f32::from_bits(i << 18));
        out.extend([Value::Float(x), Value::Float(-x)]);
    }
    for i in 0..(0x7ff0_0000_0000_0000_u64 >> 50) {
        let x = f64::from_bits(i << 50);
        out.extend([Value::Float(x), Value::Float(-x)]);
    }
    out.extend(special_nans().into_iter().map(Value::Float));
    for text in [
        "",
        "a",
        "0",
        "1.5",
        "true",
        "FALSE",
        "2024-05-06",
        "12:30:05",
        "aGk=",
        "0a",
    ] {
        out.extend([Value::Str(text.to_owned()), Value::Token(text.to_owned())]);
    }
    for text in [
        "2024-05-06",
        "12:30:05",
        "23:59:60.125+05:30",
        "2024-05-06T12:30:05Z",
        "2024-05-06T12:30:05-00:00",
        "2024-05-06T12:30:05+00:00",
        "2024-05-06T12:30:05.1234567891",
    ] {
        out.push(Value::Temporal(Temporal::parse(text).unwrap()));
    }
    out.extend([
        Value::Bytes(vec![1, 2]),
        Value::List(vec![Value::Int(1)]),
        Value::LabeledNull(3),
    ]);
    out
}

/// Infinities and NaNs of both signs, with payloads that a binary32 NaN
/// can and cannot carry.
fn special_nans() -> Vec<f64> {
    [
        0x7ff0_0000_0000_0000_u64,
        0xfff0_0000_0000_0000,
        0x7ff8_0000_0000_0000,
        0xfff8_0000_0000_0000,
        0x7ff8_0000_0000_1234,
        0x7ff0_0000_2000_0000,
        0x7ff0_0000_1fff_ffff,
        0xffff_ffff_e000_0000,
        0x7ff0_0000_0000_0001,
    ]
    .map(f64::from_bits)
    .to_vec()
}

/// Laws (i) (for `coerce_value`), (ii) and the pass-through rules, on every
/// value of [`bounded_values`] under every spec of [`specs`].
#[test]
fn coerce_value_laws_hold_on_a_bounded_scope() {
    let values = bounded_values();
    let all = specs();
    let mut checked = 0_usize;
    for value in &values {
        for spec in &all {
            let once = coerce_value(value, spec, site());
            if let Some(expected) = expected_value(value, spec) {
                match (&once, &expected) {
                    (Ok(Value::Float(got)), Ok(Value::Float(want))) => {
                        assert_eq!(
                            got.to_bits(),
                            want.to_bits(),
                            "{value:?} as {:?}",
                            spec.kind
                        );
                    }
                    (Ok(got), Ok(want)) => {
                        assert!(same(got, want), "{value:?} as {spec:?}: {got:?}");
                    }
                    (Err(e), Err(reason)) => assert_eq!(&e.reason, reason, "{value:?} as {spec:?}"),
                    _ => panic!("{value:?} as {spec:?}: got {once:?}, expected {expected:?}"),
                }
            }
            // Pass-through outside the domain.
            let passes = match (value, &spec.kind) {
                (_, ScalarKind::Any)
                | (Value::Bytes(_) | Value::List(_) | Value::LabeledNull(_), _) => Some(true),
                (Value::Float(x), ScalarKind::Float { bits: 32 }) if !x.is_finite() => {
                    Some(is_binary32(*x))
                }
                (Value::Float(x), ScalarKind::Float { .. }) if !x.is_finite() => Some(true),
                (Value::Float(x), _) if !x.is_finite() => Some(false),
                _ => None,
            };
            match passes {
                Some(true) => assert!(
                    once.as_ref().is_ok_and(|v| same(v, value)),
                    "{value:?} as {:?}: {once:?}",
                    spec.kind
                ),
                Some(false) => assert_eq!(
                    once.as_ref().unwrap_err().reason,
                    FidelityReason::NonFinite,
                    "{value:?} as {:?}",
                    spec.kind
                ),
                None => {}
            }
            // Law (ii): idempotence, by canonical bytes.
            if let Ok(once) = &once {
                let twice = coerce_value(once, spec, site());
                assert!(
                    twice
                        .as_ref()
                        .is_ok_and(|t| t.canonical_bytes() == once.canonical_bytes()),
                    "{value:?} as {:?}: {once:?} then {twice:?}",
                    spec.kind
                );
            }
            checked += 1;
        }
    }
    assert_eq!(checked, values.len() * all.len());
}

/// Law (iii) on a bounded scope of lexemes: every float numeral of
/// [`bounded_float_numerals`] under both float kinds; and every numeral
/// `{m}e{k}`, `-{m}e{k}` and `{m}.{f}` with m in 0..=99, k in -6..=6 and f
/// in 0..=99, every word, boolean, temporal and null literal below, of
/// every class `coerce_value` writes, under every spec of [`specs`].
#[test]
fn coerced_values_are_fixed_points_on_a_bounded_scope() {
    let fixed = |text: &str, class: &LexClass, spec: &ValueSpec| -> bool {
        let Ok(read) = coerce(&Lexeme::new(text, class.clone()), spec) else {
            return false;
        };
        let again = coerce_value(&read, spec, site());
        assert!(
            again
                .as_ref()
                .is_ok_and(|v| v.canonical_bytes() == read.canonical_bytes()),
            "{text:?} ({class:?}) as {:?}: {read:?} then {again:?}",
            spec.kind
        );
        true
    };
    let mut accepted = 0_usize;
    for text in bounded_float_numerals() {
        for bits in [32, 64] {
            accepted += usize::from(fixed(
                &text,
                &LexClass::Number,
                &ValueSpec::new(ScalarKind::Float { bits }),
            ));
        }
    }
    let mut texts = Vec::new();
    for m in 0..=99 {
        for k in -6..=6 {
            texts.extend([format!("{m}e{k}"), format!("-{m}e{k}")]);
        }
        for f in 0..=99 {
            texts.push(format!("{m}.{f}"));
        }
    }
    texts.extend(
        [
            "",
            "a",
            "0",
            "-0",
            "1.5",
            "true",
            "FALSE",
            "null",
            "2024-05-06",
            "12:30:05",
            "aGk=",
            "0a",
            "23:59:60.125+05:30",
            "2024-05-06T12:30:05Z",
            "2024-05-06T12:30:05z",
            "2024-05-06 12:30:05-00:00",
            "2024-05-06T12:30:05.1234567891",
            "NaN",
            "-inf",
        ]
        .map(str::to_owned),
    );
    let classes = [
        LexClass::Number,
        LexClass::String,
        LexClass::DateTime,
        LexClass::Bool,
        LexClass::Null,
    ];
    for text in &texts {
        for class in &classes {
            for spec in &specs() {
                accepted += usize::from(fixed(text, class, spec));
            }
        }
    }
    assert!(accepted > 100_000, "{accepted}");
}

/// The fraction fields of the stated scope: each boundary of the narrowing
/// rules (the low 29 bits, binary32's 23-bit field, the quiet bit) and its
/// neighbours.
const FRACTION_SET: [u64; 14] = [
    0,
    1,
    (1 << 28) - 1,
    1 << 28,
    (1 << 28) + 1,
    (1 << 29) - 1,
    1 << 29,
    (1 << 52) - (1 << 29),
    (1 << 52) - (1 << 28) - 1,
    (1 << 52) - (1 << 28),
    (1 << 52) - 1,
    (1 << 51) - (1 << 29),
    1 << 51,
    (1 << 51) + (1 << 29),
];

/// Every f64 of the stated scope: both signs, every exponent field, every
/// fraction of [`FRACTION_SET`]; then 12345679000.0.
fn stated_f64_scope() -> Vec<f64> {
    let mut out = Vec::new();
    for sign in [0_u64, 1] {
        for exponent in 0..=0x7ff_u64 {
            for fraction in FRACTION_SET {
                out.push(f64::from_bits((sign << 63) | (exponent << 52) | fraction));
            }
        }
    }
    out.push(12_345_679_000.0);
    out
}

/// What `coerce_value(Float(x), Float { bits: 32 })` must do, from the
/// definition: keep x iff it is the widening of a binary32 value (for a
/// NaN, iff its fraction is a binary32 NaN fraction g, with its quiet bit,
/// bit 22, set by widening, shifted up by 29 bits); otherwise refuse a NaN
/// with `NonFinite`, a finite x that rounds to an infinity at binary32 with
/// `Overflow`, and any other with `PrecisionExceeded { max: 9 }`.
fn narrowing_reference(x: f64) -> Result<f64, FidelityReason> {
    let fraction = x.to_bits() & ((1 << 52) - 1);
    if x.is_nan() {
        let widened = fraction % (1 << 29) == 0 && (fraction >> 29) & (1 << 22) != 0;
        return if widened {
            Ok(x)
        } else {
            Err(FidelityReason::NonFinite)
        };
    }
    if is_binary32(x) {
        return Ok(x);
    }
    // |x| rounds to an infinity at binary32 iff it reaches the midpoint of
    // the largest binary32 and 2^128, compared exactly.
    let threshold = Dyadic {
        m: Nat::from_u64((1 << 25) - 1),
        e: 103,
    };
    let (int, frac) = (format!("{:.0}", x.abs().trunc()), String::new());
    let reaches = x.abs().fract() == 0.0 && cmp_numeral(&int, &frac, &threshold) != Ordering::Less;
    Err(if reaches {
        FidelityReason::Overflow
    } else {
        FidelityReason::PrecisionExceeded { max: 9 }
    })
}

/// The narrowing rule of a 32-bit float kind, and its agreement with the
/// lexeme route, on every f64 of [`stated_f64_scope`]: `coerce_value`
/// equals [`narrowing_reference`]; for a finite x, `coerce` of x's exact
/// decimal text equals law (i)'s exact reading ([`expected_float`]),
/// accepts that text with x when x is a binary32 value, and, when both
/// routes refuse, refuses with the same reason and `max`.
#[test]
fn float32_rules_agree_with_the_exact_reference_on_the_stated_scope() {
    let f32_ = ValueSpec::new(ScalarKind::Float { bits: 32 });
    let scope = stated_f64_scope();
    let (mut kept, mut both_refused, mut lexeme_only) = (0_usize, 0_usize, 0_usize);
    for &x in &scope {
        let got = coerce_value(&Value::Float(x), &f32_, site());
        let want = narrowing_reference(x);
        match (&got, &want) {
            (Ok(Value::Float(y)), Ok(z)) => {
                assert_eq!(y.to_bits(), z.to_bits(), "{:#x}", x.to_bits());
                kept += 1;
            }
            (Err(e), Err(reason)) => assert_eq!(&e.reason, reason, "{:#x}", x.to_bits()),
            _ => panic!("{:#x}: got {got:?}, expected {want:?}", x.to_bits()),
        }
        if !x.is_finite() {
            continue;
        }
        let exact = format!("{x:.1100e}");
        let lexeme = coerce(&Lexeme::new(&exact, LexClass::Number), &f32_);
        assert_float_result(&lexeme, &expected_float(&exact, 32))
            .unwrap_or_else(|e| panic!("{:#x}: {e}", x.to_bits()));
        match (&got, &lexeme) {
            (Ok(_), lexeme) => assert!(
                lexeme.as_ref().is_ok_and(|v| same(v, &Value::Float(x))),
                "{:#x}: the exact text of a binary32 value is accepted",
                x.to_bits()
            ),
            (Err(a), Err(b)) => {
                assert_eq!(a.reason, b.reason, "{:#x}", x.to_bits());
                both_refused += 1;
            }
            (Err(_), Ok(_)) => lexeme_only += 1,
        }
    }
    // 12345679000.0 is no binary32, and its exact text is the shortest
    // text of the binary32 it rounds to, so only the lexeme route accepts.
    let pinned = coerce(&Lexeme::new("12345679000", LexClass::Number), &f32_).unwrap();
    assert!(same(&pinned, &Value::Float(12_345_678_848.0)));
    assert!(coerce_value(&Value::Float(12_345_679_000.0), &f32_, site()).is_err());
    println!(
        "bound: {} f64 values (2 signs x 2048 exponent fields x {} fractions, and 12345679000.0); \
         kept {kept}, both refused {both_refused}, accepted by the lexeme route only {lexeme_only}",
        scope.len(),
        FRACTION_SET.len()
    );
    assert_eq!(scope.len(), 2 * 2048 * 14 + 1);
}

/// The two routes into a 32-bit float agree on `Ok` results, bit for bit,
/// on the stated numerals: the exact decimal texts of the finite values of
/// [`stated_f64_scope`], and every numeral `{m}e{k}` and `-{m}e{k}` with m
/// in 0..=999 (at most three significant digits) and k in -50..=40.
#[test]
fn float32_routes_agree_on_the_stated_numerals() {
    let f32_ = ValueSpec::new(ScalarKind::Float { bits: 32 });
    let f64_ = ValueSpec::new(ScalarKind::Float { bits: 64 });
    let mut numerals: Vec<String> = stated_f64_scope()
        .into_iter()
        .filter(|x| x.is_finite())
        .map(|x| format!("{x:.1100e}"))
        .collect();
    for m in 0..=999 {
        for k in -50..=40 {
            numerals.extend([format!("{m}e{k}"), format!("-{m}e{k}")]);
        }
    }
    let mut agreed = 0_usize;
    for text in &numerals {
        let direct = coerce(&Lexeme::new(text, LexClass::Number), &f32_);
        let path = coerce(&Lexeme::new(text, LexClass::Number), &f64_)
            .and_then(|wide| coerce_value(&wide, &f32_, site()));
        if let (Ok(Value::Float(a)), Ok(Value::Float(b))) = (&direct, &path) {
            assert_eq!(a.to_bits(), b.to_bits(), "{text}");
            agreed += 1;
        }
    }
    println!(
        "bound: {} numerals; both routes Ok on {agreed}",
        numerals.len()
    );
    assert!(agreed > 0);
}
