//! Complete finite scopes for the value-law page.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::bytes::{encode_base64, latin1_decode};

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

/// The decimals of the special-value laws.
///
/// Every decimal of at most two significant digits (significands 1 to 99) and the zero, with exponent -2
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
const fn finite_offsets() -> [(&'static str, Offset); 11] {
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

/// The offset scope of the unwritten-UTC representation law.
///
/// The first thirteen entries are the unwritten Zulu marker, the twelve
/// replacements named by the law (ten written zero forms, unwritten fixed
/// zero and local), followed by three nonzero controls: a written
/// minute-resolution offset with a seconds field, a genuine second-resolution
/// offset and an unwritten fixed offset.
#[must_use]
pub const fn unwritten_zulu_offset_scope() -> [(&'static str, Offset); 16] {
    [
        ("", Offset::Zulu(ZuluCase::Unwritten)),
        ("", Offset::Local),
        ("Z", Offset::Zulu(ZuluCase::Upper)),
        ("z", Offset::Zulu(ZuluCase::Lower)),
        ("+00:00", Offset::Fixed(0, OffsetForm::Extended)),
        ("+0000", Offset::Fixed(0, OffsetForm::Basic)),
        ("+00", Offset::Fixed(0, OffsetForm::HourOnly)),
        ("+00:00:00", Offset::Fixed(0, OffsetForm::ExtendedSeconds)),
        ("-00:00", Offset::NegZero(OffsetForm::Extended)),
        ("-0000", Offset::NegZero(OffsetForm::Basic)),
        ("-00", Offset::NegZero(OffsetForm::HourOnly)),
        ("-00:00:00", Offset::NegZero(OffsetForm::ExtendedSeconds)),
        ("", Offset::Fixed(0, OffsetForm::Unwritten)),
        ("+05:30:00", Offset::Fixed(330, OffsetForm::ExtendedSeconds)),
        ("-04:56:02", Offset::FixedSeconds(-17_762)),
        ("", Offset::Fixed(330, OffsetForm::Unwritten)),
    ]
}

/// The complete finite scope of the unwritten-Zulu representation law.
///
/// The forms contain all 658 temporal forms, including the 62 unwritten-Zulu
/// baselines. The offsets contain the marker, its twelve representation
/// replacements and three unrelated controls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnwrittenZuluTemporalScope {
    /// Every written temporal form in the values-page scope.
    pub forms: Vec<(String, TemporalLexicalForm)>,
    /// The marker, its replacements and the unrelated offset controls.
    pub offsets: [(&'static str, Offset); 16],
}

/// Construct the complete finite scope of the unwritten-Zulu representation
/// law.
#[must_use]
pub fn unwritten_zulu_temporal_scope() -> UnwrittenZuluTemporalScope {
    UnwrittenZuluTemporalScope {
        forms: temporal_forms(),
        offsets: unwritten_zulu_offset_scope(),
    }
}

/// The written temporal forms with their exact descriptors.
///
/// Every date body (12), time body (14) and date-time pairing (4 x 8) of reduced,
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

/// The values of the representation and canonical-byte laws.
///
/// They are the 1010
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

// The coercion scope.

const SPELLINGS: &[&str] = &[
    "NaN",
    "-NaN",
    "nan",
    "+nan",
    ".nan",
    "-.nan",
    "NAN",
    "sNaN",
    "-sNaN",
    "NaN5",
    "sNaN7",
    "inf",
    "-inf",
    "+inf",
    "Infinity",
    "-Infinity",
    "INF",
    ".inf",
    "-.inf",
    "+.inf",
    "infinity",
];

const EDGE_NUMERALS: &[&str] = &[
    "0",
    "-0",
    "0.0",
    "-0.0",
    "0e5",
    "-0e-5",
    "0.00",
    "0.10",
    "0.1",
    "1.50",
    "-1.5",
    "65504",
    "65505",
    "65519",
    "65519.99",
    "65520",
    "-65520",
    "6.103515625e-5",
    "5.960464477539063e-8",
    "2.98023223876953125e-8",
    "2.9802322387695312e-8",
    "16777216",
    "16777217",
    "16777217.0",
    "3.4028235e38",
    "3.4028235677973366e38",
    "3.4028236e38",
    "1.401298464324817e-45",
    "7.006492321624085e-46",
    "1e308",
    "1.7976931348623157e308",
    "1.7976931348623158e308",
    "1.7976931348623159e308",
    "1e309",
    "4.9e-324",
    "5e-324",
    "2.4703282292062328e-324",
    "2.4703282292062327e-324",
    "1e-400",
    "1e400",
    "1e401",
    "-1e401",
    "1e-401",
    "9007199254740992",
    "9007199254740993",
    "1125899906842624.2",
    "1125899906842624.3",
    "52401.312",
    "52401.313",
    "127",
    "128",
    "-128",
    "-129",
    "255",
    "256",
    "32767",
    "32768",
    "-32768",
    "-32769",
    "65535",
    "65536",
    "2147483647",
    "2147483648",
    "-2147483648",
    "-2147483649",
    "4294967295",
    "4294967296",
    "9223372036854775807",
    "9223372036854775808",
    "-9223372036854775808",
    "-9223372036854775809",
    "18446744073709551615",
    "18446744073709551616",
    "1e20",
    "1e4097",
    "-1e20",
    "1.5e1",
    "1.25e1",
    "12.5",
    "0.5e1",
];

const WORDS: &[&str] = &[
    "true", "false", "TRUE", "False", "yes", "null", "", " ", "x", "abc", "\u{e9}", "\u{100}", "T",
    "Z", "00:00", "+04:00",
];

/// The byte texts of `b`: padded and unpadded base64, URL-safe base64, both
/// hex cases and the Latin-1 text.
fn byte_texts(b: &[u8]) -> Vec<String> {
    let padded = encode_base64(b);
    let unpadded = padded.trim_end_matches('=').to_owned();
    let url = padded.replace('+', "-").replace('/', "_");
    let lower: String = b.iter().fold(String::new(), |mut out, x| {
        let _ = write!(out, "{x:02x}");
        out
    });
    vec![
        unpadded,
        url.trim_end_matches('=').to_owned(),
        url,
        lower.to_uppercase(),
        lower,
        padded,
        latin1_decode(b),
    ]
}

fn lexemes_with_byte_texts(byte_text_len: usize) -> Vec<String> {
    let mut out: Vec<String> = numerals();
    out.extend(SPELLINGS.iter().map(|s| (*s).to_owned()));
    out.extend(EDGE_NUMERALS.iter().map(|s| (*s).to_owned()));
    out.extend(WORDS.iter().map(|s| (*s).to_owned()));
    out.extend(temporal_lexemes());
    for b in byte_strings().into_iter().flatten() {
        if b.len() <= byte_text_len {
            out.extend(byte_texts(&b));
        }
    }
    dedupe(out)
}

/// Every reference spec of the coercion scope.
///
/// They are each scalar kind, the integer
/// widths 8 to 64 either signed or not, the float widths 16, 32 and 64 under
/// both readings, the decimal facets, the byte-text forms, the offset
/// requirements, the integer-width overrides, and each numeral dialect over
/// the numeric kinds.
#[must_use]
pub fn coercion_specs() -> Vec<crate::coerce::ValueSpec> {
    use crate::coerce::ScalarKind as K;
    let of = crate::coerce::ValueSpec::of;
    let mut out = vec![
        of(K::Any),
        of(K::Bool),
        of(K::BigInt),
        of(K::Str),
        of(K::Token),
    ];
    for bits in [8, 16, 32, 64] {
        for signed in [true, false] {
            out.push(of(K::Int { bits, signed }));
        }
    }
    for int_bits in [Some(8), Some(16)] {
        for unsigned in [false, true] {
            out.push(crate::coerce::ValueSpec {
                int_bits,
                unsigned,
                ..of(K::Int {
                    bits: 64,
                    signed: true,
                })
            });
        }
    }
    out.push(crate::coerce::ValueSpec {
        unsigned: true,
        ..of(K::Int {
            bits: 64,
            signed: true,
        })
    });
    for bits in [16, 32, 64] {
        out.push(of(K::Float { bits }));
        out.push(crate::coerce::ValueSpec {
            float_reading: crate::coerce::FloatReading::NearestKeepingLexeme,
            ..of(K::Float { bits })
        });
    }
    for precision in [None, Some(1), Some(2)] {
        for scale in [None, Some(0), Some(1)] {
            out.push(crate::coerce::ValueSpec {
                precision,
                scale,
                ..of(K::Decimal)
            });
        }
    }
    for text in [
        crate::coerce::BytesText::Base64,
        crate::coerce::BytesText::Base64Url,
        crate::coerce::BytesText::Hex,
        crate::coerce::BytesText::Raw,
        crate::coerce::BytesText::Base64Padded,
        crate::coerce::BytesText::Base64UrlPadded,
        crate::coerce::BytesText::Latin1,
    ] {
        out.push(of(K::Bytes(text)));
    }
    for kind in [K::Date, K::Time, K::DateTime] {
        for with_time_zone in [None, Some(true), Some(false)] {
            out.push(crate::coerce::ValueSpec {
                with_time_zone,
                ..of(kind)
            });
        }
    }
    for numeral in [
        crate::coerce::NumeralDialect::Json,
        crate::coerce::NumeralDialect::Toml,
        crate::coerce::NumeralDialect::Yaml12,
        crate::coerce::NumeralDialect::Yaml11,
        crate::coerce::NumeralDialect::Xsd,
        crate::coerce::NumeralDialect::Java,
        crate::coerce::NumeralDialect::ProtoJson,
        crate::coerce::NumeralDialect::Sql,
    ] {
        for kind in [
            K::Int {
                bits: 64,
                signed: true,
            },
            K::BigInt,
            K::Float { bits: 64 },
            K::Decimal,
        ] {
            out.push(crate::coerce::ValueSpec {
                numeral: Some(numeral),
                ..of(kind)
            });
        }
    }
    out
}

/// Every reference value of the coercion scope.
///
/// They are every binary16 pattern
/// widened with its two binary64 neighbours; the binary64 boundary patterns
/// of the binary32 narrowing; every binary32 boundary pattern widened with
/// its neighbours; each lexeme of [`coercion_lexemes`] read under the numeric
/// and temporal kinds wherever the reference accepts it, and as a string and
/// a token; the integer extremes; every byte string of length at most two;
/// booleans, null and one value of each structured variant.
#[must_use]
pub fn coercion_values() -> Vec<Value> {
    use crate::coerce::ScalarKind as K;
    let mut out = Vec::new();
    let around = |w: u64| [w, w.wrapping_sub(1), w.wrapping_add(1)];
    for b in binary16_patterns() {
        out.extend(around(widen_bits(Format::B16, b)).map(Value::Float));
    }
    let exps: Vec<u64> = (871..=1153).chain([0, 1, 2046, 2047]).collect();
    out.extend(
        super::boundary_bits(Format::B64, Some(&exps), &super::binary64_fractions())
            .into_iter()
            .map(Value::Float),
    );
    for b in super::boundary_bits(Format::B32, None, &super::binary32_fractions()) {
        out.extend(around(widen_bits(Format::B32, b)).map(Value::Float));
    }
    let numeric = [
        K::Any,
        K::BigInt,
        K::Decimal,
        K::Float { bits: 16 },
        K::Float { bits: 32 },
        K::Float { bits: 64 },
        K::Int {
            bits: 64,
            signed: true,
        },
    ];
    let lexemes = coercion_lexemes();
    for t in &lexemes {
        for kind in numeric {
            if let Some(Ok(v)) = crate::coerce::coerce(
                t,
                &crate::coerce::LexClass::Number,
                &crate::coerce::ValueSpec::of(kind),
            ) {
                out.push(v);
            }
        }
        for kind in [K::Date, K::Time, K::DateTime] {
            if let Some(Ok(v)) = crate::coerce::coerce(
                t,
                &crate::coerce::LexClass::DateTime,
                &crate::coerce::ValueSpec::of(kind),
            ) {
                out.push(v);
            }
        }
        out.push(Value::Str(t.clone()));
        out.push(Value::Token(t.clone()));
    }
    for i in [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX] {
        out.push(Value::Int(i));
    }
    out.extend(byte_strings().into_iter().flatten().map(Value::Bytes));
    out.extend([
        Value::Bool(true),
        Value::Bool(false),
        Value::Null,
        Value::CidLink("bafy".to_owned()),
        Value::Blob {
            ref_: "bafy".to_owned(),
            mime: "text/plain".to_owned(),
            size: 3,
        },
        Value::Opaque {
            type_: "custom".to_owned(),
            fields: std::iter::once(("value".to_owned(), Value::Str("1".to_owned()))).collect(),
        },
        Value::Unknown(std::iter::once(("a".to_owned(), Value::Int(1))).collect()),
        Value::List(vec![]),
        Value::List(vec![Value::Int(1), Value::Float(f64::NAN.to_bits())]),
        Value::LabeledNull(0),
        Value::LabeledNull(u64::MAX),
    ]);
    dedupe(out)
}

/// The decimal edge numerals of the coercion scope, at the float, integer
/// and rounding boundaries.
#[must_use]
pub fn edge_numerals() -> Vec<String> {
    EDGE_NUMERALS
        .iter()
        .map(|text| (*text).to_owned())
        .collect()
}

/// The lexemes of the coercion scope without byte texts beyond those of the
/// empty byte string, the alphabet of the generated coercion cases.
#[must_use]
pub fn coercion_words() -> Vec<String> {
    lexemes_with_byte_texts(0)
}

/// Every lexeme of the coercion scope: the numerals, the special spellings,
/// the edge numerals, the words, every temporal lexeme, and every byte text
/// of the byte strings of length at most one.
#[must_use]
pub fn coercion_lexemes() -> Vec<String> {
    lexemes_with_byte_texts(1)
}

fn dedupe<T: Clone + Eq + std::hash::Hash>(items: Vec<T>) -> Vec<T> {
    let mut seen = std::collections::HashSet::new();
    items
        .into_iter()
        .filter(|x| seen.insert(x.clone()))
        .collect()
}

/// Every binary32 boundary pattern: either sign, every exponent field, and
/// each fraction of the binary32 boundary fractions: 2 x 256 x 8 = 4096.
#[must_use]
pub fn binary32_boundary_patterns() -> Vec<u64> {
    super::boundary_bits(Format::B32, None, &super::binary32_fractions())
}

/// The binary64 boundary patterns of the binary32 and binary64 sweeps.
///
/// They are either sign, the biased exponent fields 871 to 1153 (which hold every
/// binary32 value and its midpoints) and 0, 1, 2046, 2047, and each
/// fraction of the binary64 boundary fractions: 2 x 287 x 14 = 8036.
#[must_use]
pub fn binary64_boundary_patterns() -> Vec<u64> {
    let mut exponents: Vec<u64> = (871..=1153).collect();
    exponents.extend([0, 1, 2046, 2047]);
    super::boundary_bits(Format::B64, Some(&exponents), &super::binary64_fractions())
}

/// Every binary64 exponent field with each boundary fraction, of either
/// sign: 2 x 2048 x 14 = 57,344 patterns.
#[must_use]
pub fn binary64_exponent_patterns() -> Vec<u64> {
    super::boundary_bits(Format::B64, None, &super::binary64_fractions())
}

/// The numeral-dialect probes: every example of the dialect table, the
/// listed malformed spellings, and further forms at each grammar's edges.
const DIALECT_PROBE_TEXTS: &[&str] = &[
    "1_000",
    "0x1F",
    "0x1f",
    "0X1F",
    "+inf",
    "-inf",
    "inf",
    "nan",
    "+nan",
    "-nan",
    "1__0",
    "_1",
    "1_",
    "017",
    "0o17",
    "0o",
    "0b101",
    "0b2",
    "22:30",
    "190:20:30",
    "1:60",
    "0:30",
    "1e3",
    "1e+3",
    "1.0e3",
    "1.e+3",
    ".5",
    "0.",
    "5.",
    "+12e03",
    "-2E+05",
    "INF",
    "+INF",
    "-INF",
    "NaN",
    "-NaN",
    "+NaN",
    "Infinity",
    "-Infinity",
    "+Infinity",
    "infinity",
    ".inf",
    "-.inf",
    "+.inf",
    ".Inf",
    ".INF",
    ".nan",
    ".NaN",
    ".NAN",
    "-.nan",
    "+.nan",
    "+1",
    "-1",
    "-0",
    "00",
    "01",
    "1",
    "1.5",
    "0x",
    "+0x1",
    "-0x1F",
    "0x_1F",
    "0x1_F",
    "685.230_15e+03",
    "685_230.15",
    "190:20:30.15",
    "9.0e+999",
    "-9.0e+999",
    "0.1000000000000000055",
    "12345678901234567170.0",
    "1e2",
    "1.5e1",
    "0b1010_0111",
    "02472256",
    "08",
    "0_",
    "-0b11",
    "+0o17",
    "1_000.5",
    "1_000e1",
    "1.5_5",
    "12e",
    "",
    "-",
    "+",
    ".",
    "abc",
    "sNaN",
    "Inf",
    "inf5",
    "9223372036854775808",
    "-0.0",
];

/// The numeral-dialect probes as texts.
#[must_use]
pub fn dialect_probes() -> Vec<String> {
    DIALECT_PROBE_TEXTS
        .iter()
        .map(|text| (*text).to_owned())
        .collect()
}

/// The lexemes of the discriminator laws: a number, an exponent numeral, a
/// fraction, a word, a boolean, the empty text, a date and a base64 text.
#[must_use]
pub fn tagged_lexemes() -> Vec<String> {
    ["42", "1e2", "0.5", "abc", "true", "", "2024-01-02", "AQI="]
        .map(str::to_owned)
        .to_vec()
}

/// The scope of the chronological-order laws: the order law's product
/// extended by the offsets with seconds and the unwritten offsets, and the
/// written temporal forms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChronologicalScope {
    /// Every shape over the order law's years, months, days, hours,
    /// minutes, seconds and fractions under eleven written offsets: the nine
    /// of [`temporal_lexemes`] (19,098 lexemes) followed by `-04:56:02` and
    /// `+05:30:00` (2 x 2122 = 4244 lexemes), each read under every carrier
    /// kind.
    pub lexemes: Vec<String>,
    /// Every local value of the product (the product's lexemes with no
    /// offset, read under each carrier kind by the reference) with its
    /// offset replaced by an unwritten fixed offset of 0 and then of +05:30,
    /// as a lexeme and the exact descriptor that reads it.
    pub unwritten: Vec<(String, TemporalLexicalForm)>,
    /// Every form of [`temporal_forms`] (658 forms, which add the
    /// front-truncated shapes, both infinities of each form under local and
    /// zoned offsets, offsets with seconds and the unwritten offsets).
    pub forms: Vec<(String, TemporalLexicalForm)>,
}

/// The scope of the chronological-order laws, [`ChronologicalScope`].
#[must_use]
pub fn chronological_scope() -> ChronologicalScope {
    let mut lexemes = temporal_lexemes();
    lexemes.extend(super::temporal_lexemes_with_offsets(&[
        "-04:56:02",
        "+05:30:00",
    ]));
    let local = super::temporal_lexemes_with_offsets(&[""]);
    let mut unwritten = Vec::new();
    for offset in [
        Offset::Fixed(0, OffsetForm::Unwritten),
        Offset::Fixed(330, OffsetForm::Unwritten),
    ] {
        for text in &local {
            for kind in [
                TemporalForm::Date,
                TemporalForm::Time,
                TemporalForm::DateTime,
            ] {
                let Some(value) = crate::temporal::parse(kind, text) else {
                    continue;
                };
                if value.offset != Offset::Local {
                    continue;
                }
                if let Some(form) = value.lexical_form() {
                    unwritten.push((text.clone(), TemporalLexicalForm { offset, ..form }));
                }
            }
        }
    }
    ChronologicalScope {
        lexemes,
        unwritten,
        forms: temporal_forms(),
    }
}

/// The binary32 NaNs of the narrowing laws.
///
/// For every binary16 NaN pattern `p`, its binary32 image is constructed
/// directly as `((p >> 15) << 31) | (0xff << 23) | ((p & 0x03ff) << 13)`.
/// These 2 x 1023 images are supplemented by each sign and flag with payloads
/// at the low edge, in the middle and at the high edge of the binary32
/// fraction (18), less the two probes the images already hold: 2062 NaNs.
#[must_use]
pub fn binary32_nans() -> Vec<u64> {
    let mut nans: Vec<u64> = binary16_patterns()
        .into_iter()
        .filter(|pattern| pattern & 0x7c00 == 0x7c00 && pattern & 0x03ff != 0)
        .map(|p| ((p >> 15) << 31) | (0xff << 23) | ((p & 0x03ff) << 13))
        .collect();
    for sign in [0_u64, 0x8000_0000] {
        for quiet in [0_u64, 1 << 22] {
            for payload in [0_u64, 1, 2, 0x1234, (1 << 22) - 1] {
                if quiet != 0 || payload != 0 {
                    nans.push(sign | 0x7f80_0000 | quiet | payload);
                }
            }
        }
    }
    dedupe(nans)
}

/// The binary32 scope of the narrowing laws: every pattern of
/// [`binary32_boundary_patterns`] and of [`binary32_nans`].
#[must_use]
pub fn binary32_scope() -> Vec<u64> {
    let mut patterns = binary32_boundary_patterns();
    patterns.extend(binary32_nans());
    dedupe(patterns)
}

/// The scope of the nearest-reading sweep.
///
/// Every binary32 boundary pattern (2 x 256 x 8 = 4096), every binary64
/// exponent pattern (2 x 2048 x 14 = 57,344) and the values-page numerals (at
/// most two significand digits, exponents -1 to 1: 600).
#[must_use]
pub fn nearest_reading_scope() -> (Vec<u64>, Vec<u64>, Vec<String>) {
    (
        binary32_boundary_patterns(),
        binary64_exponent_patterns(),
        numerals(),
    )
}

/// The scope of the binary rounding and printing sweeps.
///
/// Every binary16 pattern (65,536), every binary32 boundary pattern (4096)
/// and every binary64 boundary pattern (8036).
#[must_use]
pub fn binary_rounding_scope() -> (Vec<u64>, Vec<u64>, Vec<u64>) {
    (
        binary16_patterns(),
        binary32_boundary_patterns(),
        binary64_boundary_patterns(),
    )
}

/// The three pattern scopes used by the NaN narrowing law.
///
/// Binary16 is exhaustive. Binary32 contains every rounding-boundary pattern
/// and every NaN required by the narrowing partition. Binary64 contains every
/// exponent/fraction boundary pattern used to establish the carrier-width
/// identities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NanNarrowingScope {
    /// Every binary16 pattern.
    pub binary16: Vec<u64>,
    /// Every binary32 boundary pattern and scoped NaN.
    pub binary32: Vec<u64>,
    /// Every binary64 boundary pattern.
    pub binary64: Vec<u64>,
}

/// Construct the three pattern scopes used by the NaN narrowing law.
#[must_use]
pub fn nan_narrowing_scope() -> NanNarrowingScope {
    NanNarrowingScope {
        binary16: binary16_patterns(),
        binary32: binary32_scope(),
        binary64: binary64_boundary_patterns(),
    }
}

/// The expressions of the expression-evaluation scope.
///
/// Every expression uses only forms the production expression language
/// shares with the reference (no `Equal`, no `If`) and only values a
/// production literal denotes. The scope covers the higher-order builtins
/// over the empty list and short lists, with well-typed, refusing and
/// function-valued functions and with too many arguments; partial and
/// curried builtins, a shadowed builtin name and a bare builtin; `Add`,
/// `Sub`, `Mul`, `Div` and `Mod` on every pair of five boundary integers;
/// the refusal orders of the numeric builtins; the four orderings and `Eq`
/// on every pair of eight operands of distinct kinds; `Index`, `Field`,
/// `Match` and `Range` shapes, including negative and extreme indexes,
/// repeated record fields, order-free record equality, repeated pattern
/// variables and constructor patterns; closures in every data position; and
/// lexical capture, shadowing and unbound names.
#[must_use]
pub fn evaluation_expressions() -> Vec<crate::value::Expr> {
    let mut out = Vec::new();
    higher_order_expressions(&mut out);
    curried_expressions(&mut out);
    arithmetic_expressions(&mut out);
    comparison_expressions(&mut out);
    lookup_expressions(&mut out);
    match_and_range_expressions(&mut out);
    function_expressions(&mut out);
    out
}

mod ev {
    //! Constructors of reference expressions for the evaluation scope.

    use crate::value::{BuiltinOp, Expr, Pattern, Value};

    pub const fn int(n: i64) -> Expr {
        Expr::Literal(Value::Int(n))
    }

    pub const fn float(x: f64) -> Expr {
        Expr::Literal(Value::Float(x.to_bits()))
    }

    pub fn text(s: &str) -> Expr {
        Expr::Literal(Value::Str(s.to_owned()))
    }

    pub fn var(name: &str) -> Expr {
        Expr::Variable(name.to_owned())
    }

    pub fn pvar(name: &str) -> Pattern {
        Pattern::Variable(name.to_owned())
    }

    pub fn lam(name: &str, body: Expr) -> Expr {
        Expr::Lambda(name.to_owned(), Box::new(body))
    }

    pub fn app(f: Expr, a: Expr) -> Expr {
        Expr::Apply(Box::new(f), Box::new(a))
    }

    pub const fn call(op: BuiltinOp, args: Vec<Expr>) -> Expr {
        Expr::Builtin(op, args)
    }

    pub fn list(items: &[i64]) -> Expr {
        Expr::List(items.iter().map(|n| int(*n)).collect())
    }

    pub fn record(fields: Vec<(&str, Expr)>) -> Expr {
        Expr::Record(
            fields
                .into_iter()
                .map(|(name, field)| (name.to_owned(), field))
                .collect(),
        )
    }

    pub fn field(r: Expr, name: &str) -> Expr {
        Expr::Field(Box::new(r), name.to_owned())
    }

    pub fn index(l: Expr, i: Expr) -> Expr {
        Expr::Index(Box::new(l), Box::new(i))
    }

    pub fn let_in(name: &str, value: Expr, body: Expr) -> Expr {
        Expr::Let {
            name: name.to_owned(),
            value: Box::new(value),
            body: Box::new(body),
        }
    }

    pub fn matching(scrutinee: Expr, arms: Vec<(Pattern, Expr)>) -> Expr {
        Expr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
        }
    }

    /// A decimal literal; every lexeme the scope writes is a decimal.
    pub fn decimal(s: &str) -> Expr {
        Expr::Literal(crate::exact::parse_decimal(s).map_or(Value::Null, Value::Decimal))
    }

    /// `\x -> x + 1`.
    pub fn add_one() -> Expr {
        lam("x", call(BuiltinOp::Add, vec![var("x"), int(1)]))
    }

    /// `\a -> \b -> a + b`.
    pub fn sum() -> Expr {
        lam(
            "a",
            lam("b", call(BuiltinOp::Add, vec![var("a"), var("b")])),
        )
    }

    /// `\z -> z`.
    pub fn identity() -> Expr {
        lam("z", var("z"))
    }

    /// A field access on an integer, refused when evaluated.
    pub fn broken() -> Expr {
        field(int(1), "f")
    }

    /// A division by zero.
    pub fn failing() -> Expr {
        call(BuiltinOp::Div, vec![int(1), int(0)])
    }
}

/// Higher-order builtins over short lists: 15 shapes on each of 3 lists and
/// 5 others.
fn higher_order_expressions(out: &mut Vec<crate::value::Expr>) {
    use crate::value::BuiltinOp as Op;
    use ev::{add_one, app, broken, call, failing, identity, int, lam, list, sum, text, var};
    let positive = lam("x", call(Op::Gt, vec![var("x"), int(0)]));
    let twice = lam("x", crate::value::Expr::List(vec![var("x"), var("x")]));
    let to_function = lam("x", lam("y", var("y")));
    let lists: [&[i64]; 3] = [&[], &[1], &[-2, 0, 3]];
    for items in lists {
        let xs = list(items);
        out.push(call(Op::Map, vec![xs.clone(), add_one()]));
        out.push(call(Op::Filter, vec![xs.clone(), positive.clone()]));
        out.push(call(Op::FlatMap, vec![xs.clone(), twice.clone()]));
        out.push(call(Op::Fold, vec![xs.clone(), int(10), sum()]));
        out.push(call(Op::Map, vec![xs.clone(), broken()]));
        out.push(call(Op::Fold, vec![xs.clone(), failing(), sum()]));
        out.push(call(Op::Fold, vec![xs.clone(), int(0), broken()]));
        out.push(call(Op::Map, vec![xs.clone(), to_function.clone()]));
        out.push(call(Op::Filter, vec![xs.clone(), add_one()]));
        out.push(call(Op::FlatMap, vec![xs.clone(), add_one()]));
        out.push(call(Op::Fold, vec![xs.clone(), int(0), add_one()]));
        out.push(call(Op::Map, vec![xs.clone(), int(3)]));
        out.push(app(app(var("map"), add_one()), xs.clone()));
        out.push(app(app(app(var("fold"), sum()), int(0)), xs.clone()));
        out.push(app(call(Op::Filter, vec![positive.clone()]), xs));
    }
    out.push(call(Op::Map, vec![int(1), broken()]));
    out.push(call(Op::Fold, vec![text("a"), failing(), broken()]));
    out.push(call(Op::Map, vec![identity(), add_one()]));
    out.push(call(Op::Map, vec![list(&[1]), add_one(), var("unbound")]));
    out.push(call(Op::Fold, vec![list(&[1, 2]), int(0), var("add")]));
}

/// Partial and curried builtins, shadowing and bare names: 14 shapes.
fn curried_expressions(out: &mut Vec<crate::value::Expr>) {
    use crate::value::BuiltinOp as Op;
    use ev::{app, call, failing, identity, int, let_in, var};
    out.push(app(app(var("sub"), int(5)), int(3)));
    out.push(app(call(Op::Sub, vec![int(5)]), int(3)));
    out.push(call(Op::Sub, vec![int(5)]));
    out.push(call(Op::Sub, vec![failing()]));
    out.push(var("add"));
    out.push(app(var("add"), int(1)));
    out.push(app(app(app(var("add"), int(1)), int(2)), int(3)));
    out.push(let_in("add", int(5), var("add")));
    out.push(let_in("add", identity(), app(var("add"), int(4))));
    out.push(app(app(app(var("clamp"), int(9)), int(0)), int(5)));
    out.push(app(call(Op::Clamp, vec![int(9), int(0)]), int(5)));
    out.push(app(app(var("range"), int(1)), int(3)));
    out.push(call(Op::Add, vec![int(1), int(2), var("unbound")]));
    out.push(call(Op::Add, vec![int(1), int(2), int(3)]));
}

/// Integer arithmetic at the extremes (5 operations on 25 pairs, 2 on 5
/// integers) and 9 refusal orders of the numeric builtins.
fn arithmetic_expressions(out: &mut Vec<crate::value::Expr>) {
    use crate::value::BuiltinOp as Op;
    use ev::{call, decimal, float, int, text};
    let extremes = [i64::MIN, -1, 0, 1, i64::MAX];
    for op in [Op::Add, Op::Sub, Op::Mul, Op::Div, Op::Mod] {
        for a in extremes {
            for b in extremes {
                out.push(call(op, vec![int(a), int(b)]));
            }
        }
    }
    for op in [Op::Neg, Op::Abs] {
        for a in extremes {
            out.push(call(op, vec![int(a)]));
        }
    }
    out.push(call(Op::Div, vec![text("a"), int(0)]));
    out.push(call(Op::Div, vec![int(1), float(-0.0)]));
    out.push(call(Op::Div, vec![float(1.0), int(0)]));
    out.push(call(Op::Div, vec![decimal("1.5"), int(0)]));
    out.push(call(Op::Add, vec![int(1), decimal("1.5")]));
    out.push(call(Op::Mod, vec![float(1.0), int(0)]));
    out.push(call(Op::Mod, vec![decimal("NaN"), text("a")]));
    out.push(call(Op::Add, vec![int(1), text("a")]));
    out.push(call(Op::Neg, vec![decimal("1")]));
}

/// The four orderings and `Eq` on every pair of 8 operands, and 3 record and
/// list equalities.
fn comparison_expressions(out: &mut Vec<crate::value::Expr>) {
    use crate::value::{BuiltinOp as Op, Expr};
    use ev::{call, decimal, float, int, list, record, text};
    let operands = [
        int(1),
        float(1.5),
        text("a"),
        Expr::List(vec![]),
        Expr::Literal(Value::Null),
        Expr::Literal(Value::Bool(true)),
        decimal("NaN"),
        decimal("1.50"),
    ];
    for op in [Op::Lt, Op::Lte, Op::Gt, Op::Gte, Op::Eq] {
        for a in &operands {
            for b in &operands {
                out.push(call(op, vec![a.clone(), b.clone()]));
            }
        }
    }
    out.push(call(
        Op::Eq,
        vec![
            record(vec![("a", int(1)), ("b", int(2))]),
            record(vec![("b", int(2)), ("a", int(1))]),
        ],
    ));
    out.push(call(
        Op::Ne,
        vec![
            record(vec![("a", int(1)), ("a", int(2))]),
            record(vec![("a", int(2))]),
        ],
    ));
    out.push(call(Op::Eq, vec![list(&[1, 2]), list(&[2, 1])]));
}

/// 5 indexes, 3 index refusals and 9 record shapes, one record repeating a
/// field.
fn lookup_expressions(out: &mut Vec<crate::value::Expr>) {
    use crate::value::BuiltinOp as Op;
    use ev::{call, failing, field, index, int, list, record, text, var};
    for i in [i64::MIN, -1, 0, 2, 3] {
        out.push(index(list(&[10, 20, 30]), int(i)));
    }
    out.push(index(int(1), int(0)));
    out.push(index(list(&[1]), text("0")));
    out.push(index(failing(), var("unbound")));
    let r = record(vec![("a", int(1)), ("b", int(2)), ("a", int(3))]);
    out.push(field(r.clone(), "a"));
    out.push(field(r.clone(), "b"));
    out.push(field(r.clone(), "c"));
    out.push(field(int(1), "a"));
    out.push(r.clone());
    out.push(call(Op::Keys, vec![r.clone()]));
    out.push(call(Op::Values, vec![r.clone()]));
    out.push(call(
        Op::MergeRecords,
        vec![r.clone(), record(vec![("b", int(9)), ("c", int(8))])],
    ));
    out.push(call(Op::HasField, vec![r, text("a")]));
}

/// 10 match shapes and 8 ranges.
fn match_and_range_expressions(out: &mut Vec<crate::value::Expr>) {
    use crate::value::{BuiltinOp as Op, Pattern};
    use ev::{call, failing, int, list, matching, pvar, record, text, var};
    let arms = || {
        vec![
            (Pattern::Literal(Value::Int(9)), int(0)),
            (Pattern::List(vec![pvar("x"), pvar("x")]), var("x")),
            (
                Pattern::List(vec![pvar("x"), Pattern::Wildcard]),
                call(Op::Neg, vec![var("x")]),
            ),
        ]
    };
    out.push(matching(list(&[2, 2]), arms()));
    out.push(matching(list(&[1, 2]), arms()));
    out.push(matching(list(&[1, 2, 3]), arms()));
    let field_b = || vec![(Pattern::Record(vec![("b".to_owned(), pvar("y"))]), var("y"))];
    out.push(matching(
        record(vec![("a", int(1)), ("b", int(2))]),
        field_b(),
    ));
    out.push(matching(record(vec![("a", int(1))]), field_b()));
    for tag in ["Just", "None"] {
        out.push(matching(
            record(vec![("$tag", text(tag)), ("$0", int(5))]),
            vec![
                (
                    Pattern::Constructor("Just".to_owned(), vec![pvar("v")]),
                    var("v"),
                ),
                (Pattern::Constructor("None".to_owned(), vec![]), int(0)),
            ],
        ));
    }
    out.push(matching(
        record(vec![("$tag", int(1))]),
        vec![(Pattern::Constructor("Just".to_owned(), vec![]), int(0))],
    ));
    out.push(matching(
        int(3),
        vec![(Pattern::Literal(Value::Int(4)), int(0))],
    ));
    out.push(matching(failing(), vec![(Pattern::Wildcard, int(0))]));
    for (start, stop) in [(1, 3), (3, 1), (1, 1), (-1, 1)] {
        out.push(call(Op::Range, vec![int(start), int(stop)]));
    }
    out.push(call(Op::Range, vec![text("a"), int(1)]));
    out.push(call(Op::Range, vec![int(1), text("a")]));
    out.push(call(Op::Range, vec![int(1), int(2), var("unbound")]));
    out.push(call(Op::Range, vec![text("a"), failing()]));
}

/// 14 closures in data positions and functions passed and applied, and 4
/// shapes of lexical capture, shadowing and unbound names.
fn function_expressions(out: &mut Vec<crate::value::Expr>) {
    use crate::value::{BuiltinOp as Op, Expr, Pattern};
    use ev::{
        app, call, failing, field, identity, index, int, lam, let_in, list, matching, record, sum,
        var,
    };
    out.push(Expr::List(vec![int(1), identity()]));
    out.push(record(vec![("f", identity())]));
    out.push(index(identity(), int(0)));
    out.push(field(identity(), "f"));
    out.push(matching(identity(), vec![(Pattern::Wildcard, int(0))]));
    out.push(call(Op::Add, vec![identity(), int(1)]));
    out.push(call(Op::Add, vec![int(1), identity()]));
    out.push(call(Op::Add, vec![identity(), failing()]));
    out.push(call(Op::TypeOf, vec![identity()]));
    out.push(call(Op::Range, vec![identity(), int(1)]));
    out.push(call(Op::Fold, vec![list(&[1]), identity(), sum()]));
    out.push(app(identity(), identity()));
    out.push(app(app(identity(), identity()), int(7)));
    out.push(app(int(1), int(2)));
    out.push(let_in(
        "y",
        int(1),
        let_in(
            "f",
            lam("z", var("y")),
            let_in("y", int(2), app(var("f"), int(0))),
        ),
    ));
    out.push(let_in("x", int(1), app(lam("x", var("x")), int(2))));
    out.push(var("unbound"));
    out.push(app(lam("x", var("unbound")), int(1)));
}
