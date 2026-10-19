//! Complete finite scopes for the value-law page.

use std::collections::BTreeMap;

use crate::exact::{BigIntValue, Decimal, DecimalValue};
use crate::float::{Format, widen_bits};
use crate::num::{Int, Nat};
use crate::temporal::{
    Offset, OffsetForm, Separator, TemporalForm, TemporalInfinity, TemporalLexicalForm, TimeUnit,
    ZuluCase, day_number, parse_as,
};
use crate::value::Value;

/// Numerals with at most two significand digits and exponents from -1 to 1.
#[must_use]
pub fn numerals() -> Vec<String> {
    super::numerals(2, -1, 1)
}

/// All binary16 bit patterns.
#[must_use]
pub fn binary16_patterns() -> Vec<u64> {
    super::boundary_bits(Format::B16, None, &(0..1024).collect::<Vec<_>>())
}

/// The fixed temporal lexical product used by the page.
#[must_use]
pub fn temporal_lexemes() -> Vec<String> {
    super::temporal_lexemes()
}

/// Every byte string of length at most two.
#[must_use]
pub fn byte_strings() -> Option<super::ByteStrings> {
    super::byte_strings(2)
}

/// Every byte string of length at most three.
#[must_use]
pub fn byte_strings_up_to_three() -> Option<super::ByteStrings> {
    super::byte_strings(3)
}

/// The decimals of the special-value laws: every decimal of at most two
/// significant digits (significands 1 to 99) and the zero, with exponent -2
/// to 2 and either sign, both infinities, and the NaNs of each sign and
/// flag with payloads 0 and 1; 5 x 2 x 100 + 2 + 8 = 1010 values.
#[must_use]
pub fn decimals() -> Vec<Decimal> {
    let mut out = Vec::new();
    for exponent in -2_i64..=2 {
        for negative in [false, true] {
            for significand in 0_u64..=99 {
                out.push(Decimal::Finite(DecimalValue {
                    negative,
                    digits: Nat::from_u64(significand),
                    exponent: Int::from_i64(exponent),
                }));
            }
        }
    }
    for negative in [false, true] {
        out.push(Decimal::Infinite { negative });
        for signalling in [false, true] {
            for payload in [0_u64, 1] {
                out.push(Decimal::NaN {
                    negative,
                    signalling,
                    payload: Nat::from_u64(payload),
                });
            }
        }
    }
    out
}

/// The base64 alphabet, the padding character and one character outside the
/// alphabet.
const BASE64_SYMBOLS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=@";

/// Every text of length at most `max_len` over a fixed list of symbols, in
/// order of length and then of symbol index.
#[derive(Clone, Debug)]
pub struct Words {
    symbols: Vec<char>,
    max_len: u32,
    len: u32,
    index: u64,
}

impl Iterator for Words {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        let base = u64::try_from(self.symbols.len()).ok()?;
        while self.index >= base.checked_pow(self.len)? {
            if self.len == self.max_len {
                return None;
            }
            self.len += 1;
            self.index = 0;
        }
        let mut rest = self.index;
        let mut word = Vec::with_capacity(usize::try_from(self.len).ok()?);
        for _ in 0..self.len {
            word.push(self.symbols[usize::try_from(rest % base).ok()?]);
            rest /= base;
        }
        word.reverse();
        self.index += 1;
        Some(word.into_iter().collect())
    }
}

fn words(symbols: &str, max_len: u32) -> Words {
    Words {
        symbols: symbols.chars().collect(),
        max_len,
        len: 0,
        index: 0,
    }
}

/// Every text of length at most four over the 64 base64 characters, `=`
/// and `@`: 1 + 66 + 66^2 + 66^3 + 66^4 = 19,266,655 texts.
#[must_use]
pub fn base64_texts() -> Words {
    words(BASE64_SYMBOLS, 4)
}

/// Every text without `=` of length 0, 2 or 3 over the 64 base64 characters
/// and `@`, the texts a padding can complete: 1 + 65^2 + 65^3 = 278,851.
#[must_use]
pub fn unpadded_texts() -> Vec<String> {
    let symbols: String = BASE64_SYMBOLS.chars().filter(|c| *c != '=').collect();
    words(&symbols, 3)
        .filter(|text| text.chars().count() != 1)
        .collect()
}

/// Every string of length at most two over the 256 Latin-1 code points and
/// three code points beyond it (U+0100, U+20AC and U+10FFFF):
/// 1 + 259 + 259^2 = 67,341 strings.
#[must_use]
pub fn latin1_strings() -> Vec<String> {
    let mut symbols: String = (0_u8..=255).map(char::from).collect();
    symbols.extend(['\u{100}', '\u{20ac}', '\u{10ffff}']);
    words(&symbols, 2).collect()
}

type Shape = (bool, bool, bool);

const DATE_BODIES: [(&str, Shape); 12] = [
    ("0000", (true, false, false)),
    ("-0000", (true, false, false)),
    ("2000-01", (true, true, false)),
    ("2000-02", (true, true, false)),
    ("2000-01-01", (true, true, true)),
    ("2000-01-02", (true, true, true)),
    ("--01", (false, true, false)),
    ("--02", (false, true, false)),
    ("--01-01", (false, true, true)),
    ("--01-02", (false, true, true)),
    ("---01", (false, false, true)),
    ("---02", (false, false, true)),
];

const TIME_BODIES: [(&str, Shape); 14] = [
    ("00", (true, false, false)),
    ("01", (true, false, false)),
    ("00:00", (true, true, false)),
    ("00:01", (true, true, false)),
    ("00:00:00", (true, true, true)),
    ("00:00:00.0", (true, true, true)),
    ("23:59:60", (true, true, true)),
    ("23:59:60.5", (true, true, true)),
    ("-00", (false, true, false)),
    ("-01", (false, true, false)),
    ("-00:00", (false, true, true)),
    ("-00:01", (false, true, true)),
    ("--00", (false, false, true)),
    ("--01", (false, false, true)),
];

const DATETIME_DATES: [(&str, Shape); 4] = [
    ("2000-01-01", (true, true, true)),
    ("2000-01-02", (true, true, true)),
    ("---01", (false, false, true)),
    ("---02", (false, false, true)),
];

/// The eleven offsets of the finite temporal forms: local, `Z`, an
/// unwritten UTC zone, the extended, basic and hour-only zero offsets,
/// `+05:30:00` written with seconds, the second-resolution `-04:56:02`, a
/// basic negative zero, and unwritten fixed offsets of 0 and +05:30.
fn finite_offsets() -> [(&'static str, Offset); 11] {
    [
        ("", Offset::Local),
        ("Z", Offset::Zulu(ZuluCase::Upper)),
        ("", Offset::Zulu(ZuluCase::Unwritten)),
        ("+00:00", Offset::Fixed(0, OffsetForm::Extended)),
        ("+0000", Offset::Fixed(0, OffsetForm::Basic)),
        ("+00", Offset::Fixed(0, OffsetForm::HourOnly)),
        ("+05:30:00", Offset::Fixed(330, OffsetForm::ExtendedSeconds)),
        ("-04:56:02", Offset::FixedSeconds(-17_762)),
        ("-0000", Offset::NegZero(OffsetForm::Basic)),
        ("", Offset::Fixed(0, OffsetForm::Unwritten)),
        ("", Offset::Fixed(330, OffsetForm::Unwritten)),
    ]
}

/// The five offsets of the temporal infinities: local, `Z`, an unwritten
/// UTC zone, an unwritten +05:30 and the second-resolution `-04:56:02`.
const INFINITY_OFFSETS: [Offset; 5] = [
    Offset::Local,
    Offset::Zulu(ZuluCase::Upper),
    Offset::Zulu(ZuluCase::Unwritten),
    Offset::Fixed(330, OffsetForm::Unwritten),
    Offset::FixedSeconds(-17_762),
];

/// The written temporal forms with their exact descriptors: every date
/// body (12), time body (14) and date-time pairing (4 x 8) of reduced,
/// truncated and complete shape under each of eleven offsets, and both
/// infinities of the date and date-time forms under each of five offsets:
/// 132 + 154 + 352 + 20 = 658 forms.
#[must_use]
pub fn temporal_forms() -> Vec<(String, TemporalLexicalForm)> {
    let finite = |kind, date_shape, time_shape, separator, offset| TemporalLexicalForm {
        kind,
        date_shape,
        time_shape,
        separator,
        offset,
        infinity: None,
    };
    let mut out = Vec::new();
    for (body, shape) in DATE_BODIES {
        for (suffix, offset) in finite_offsets() {
            let form = finite(TemporalForm::Date, Some(shape), None, None, offset);
            out.push((format!("{body}{suffix}"), form));
        }
    }
    for (body, shape) in TIME_BODIES {
        for (suffix, offset) in finite_offsets() {
            let form = finite(TemporalForm::Time, None, Some(shape), None, offset);
            out.push((format!("{body}{suffix}"), form));
        }
    }
    for (date, date_shape) in DATETIME_DATES {
        for (time, time_shape) in &TIME_BODIES[..8] {
            for (suffix, offset) in finite_offsets() {
                let form = finite(
                    TemporalForm::DateTime,
                    Some(date_shape),
                    Some(*time_shape),
                    Some(Separator::Upper),
                    offset,
                );
                out.push((format!("{date}T{time}{suffix}"), form));
            }
        }
    }
    for kind in [TemporalForm::Date, TemporalForm::DateTime] {
        for offset in INFINITY_OFFSETS {
            for sign in [TemporalInfinity::Negative, TemporalInfinity::Positive] {
                let text = match sign {
                    TemporalInfinity::Negative => "-infinity",
                    TemporalInfinity::Positive => "infinity",
                };
                let form = TemporalLexicalForm {
                    kind,
                    date_shape: None,
                    time_shape: None,
                    separator: None,
                    offset,
                    infinity: Some(sign),
                };
                out.push((text.to_owned(), form));
            }
        }
    }
    out
}

const fn ticks(unit: TimeUnit) -> i128 {
    match unit {
        TimeUnit::Seconds => 1,
        TimeUnit::Millis => 1_000,
        TimeUnit::Micros => 1_000_000,
        TimeUnit::Nanos => 1_000_000_000,
    }
}

/// The least and the greatest count of `unit` whose date-time has a year in
/// `i64`: the first moment of year `i64::MIN` and the last unit of year
/// `i64::MAX`, counted from 1970-01-01T00:00:00.
#[must_use]
pub fn epoch_count_bounds(unit: TimeUnit) -> (i128, i128) {
    let epoch = day_number(1970, 1, 1);
    let per_day = 86_400 * ticks(unit);
    let first = (day_number(i64::MIN, 1, 1) - epoch) * per_day;
    let last = (day_number(i64::MAX, 12, 31) - epoch + 1) * per_day - 1;
    (first, last)
}

/// The epoch counts of the conversion laws: for each of the four units,
/// every count in `[-10^4, 10^4]` and within `10^4` of each bound of the
/// unit's domain: 4 x (20,001 + 2 x 10,001) = 160,012 counts.
#[must_use]
pub fn epoch_counts() -> Vec<(TimeUnit, i128)> {
    let mut out = Vec::new();
    for unit in [
        TimeUnit::Seconds,
        TimeUnit::Millis,
        TimeUnit::Micros,
        TimeUnit::Nanos,
    ] {
        let (lo, hi) = epoch_count_bounds(unit);
        for count in (-10_000..=10_000)
            .chain(lo..=lo + 10_000)
            .chain(hi - 10_000..=hi)
        {
            out.push((unit, count));
        }
    }
    out
}

/// The values of the representation and canonical-byte laws: the 1010
/// [`decimals`], every byte string of length at most one (257), the 658
/// [`temporal_forms`], the integers -2 to 2 and `i64`'s bounds (7), both
/// booleans, null, four strings and the same four tokens, the widened
/// binary16 patterns of exponent fields 0, 1, 15, 30 and 31 with fractions
/// 0, 1, 0x200 and 0x3ff and either sign (40), three big integers, a CID
/// link, a blob, an opaque value, two records, four lists and two labelled
/// nulls.
#[must_use]
pub fn canonical_values() -> Vec<Value> {
    let mut out: Vec<Value> = decimals().into_iter().map(Value::Decimal).collect();
    out.extend(
        super::byte_strings(1)
            .into_iter()
            .flatten()
            .map(Value::Bytes),
    );
    out.extend(
        temporal_forms()
            .into_iter()
            .filter_map(|(text, form)| parse_as(form, &text))
            .map(Value::Temporal),
    );
    out.extend([-2_i64, -1, 0, 1, 2, i64::MIN, i64::MAX].map(Value::Int));
    out.extend([Value::Bool(false), Value::Bool(true), Value::Null]);
    for text in ["", "a", "0", "1e0"] {
        out.push(Value::Str(text.to_owned()));
        out.push(Value::Token(text.to_owned()));
    }
    for sign in [0_u64, 0x8000] {
        for field in [0_u64, 1, 15, 30, 31] {
            for fraction in [0_u64, 1, 0x200, 0x3ff] {
                let pattern = sign | (field << 10) | fraction;
                out.push(Value::Float(widen_bits(Format::B16, pattern)));
            }
        }
    }
    for (negative, significand, zeros) in [(false, 1_u64, 19_u64), (true, 92, 18), (false, 123, 30)]
    {
        out.push(Value::BigInt(BigIntValue {
            negative,
            significand: Nat::from_u64(significand),
            zeros: Nat::from_u64(zeros),
        }));
    }
    out.push(Value::CidLink("bafy".to_owned()));
    out.push(Value::Blob {
        ref_: "bafy".to_owned(),
        mime: "text/plain".to_owned(),
        size: 3,
    });
    out.push(Value::Opaque {
        type_: "custom".to_owned(),
        fields: BTreeMap::from([("value".to_owned(), Value::Str("1".to_owned()))]),
    });
    out.push(Value::Unknown(BTreeMap::new()));
    out.push(Value::Unknown(BTreeMap::from([(
        "a".to_owned(),
        Value::Int(1),
    )])));
    out.push(Value::List(Vec::new()));
    out.push(Value::List(vec![Value::Int(1)]));
    out.push(Value::List(vec![Value::Int(1), Value::Null]));
    out.push(Value::List(vec![Value::List(vec![Value::Int(1)])]));
    out.push(Value::LabeledNull(0));
    out.push(Value::LabeledNull(u64::MAX));
    out
}
