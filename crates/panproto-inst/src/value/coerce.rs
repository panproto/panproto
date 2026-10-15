//! Reading a lexeme into a [`Value`] under a declared scalar kind.
//!
//! [`coerce`] is the one coercion every codec calls: a codec that reads a
//! scalar hands over its [`Lexeme`] (the source text and the lexical class
//! the format gave it) and the [`ValueSpec`] of the vertex it belongs to,
//! and receives the [`Value`] the kind reads the lexeme as, or a
//! [`ValueFidelityError`]. [`coerce_value`] coerces a value that already
//! exists (a transform's output, a cell moved into a column of another
//! kind) by reading its lexeme the same way.
//!
//! # Declared kinds
//!
//! Under a declared kind a lexeme is read by the kind's lexical mapping or
//! refused with a typed [`FidelityReason`]; nothing is truncated, rounded
//! to a value whose text is not the lexeme, or replaced by a default.
//!
//! - Under `Float { bits }` a finite numeral t denotes `L(t)`, its IEEE 754
//!   round-to-nearest-even value at the kind's width (32 bits for
//!   `bits == 32`, 64 bits for every other width), with a negative-zero
//!   numeral denoting the negative zero. The numeral is accepted exactly
//!   when `L(t)` is finite and either equals t, or has t (numerically) as
//!   its shortest text `C(L(t))` at that width: among the decimals with
//!   the fewest significant digits that the width reads back as `L(t)`,
//!   the one closest to `L(t)`, and at an exact tie the one whose last
//!   significant digit is even (ties to even, as ECMA-262 recommends; see
//!   [`DecimalValue::from_f64_shortest`]). So `0.1` is accepted under a
//!   32-bit kind and read as the binary32 nearest it,
//!   `0.100000001490116119384765625` (that binary32 exactly) and
//!   `9007199254740992` are accepted, and `16777217.0`, which rounds to
//!   `16777216`, is refused with [`FidelityReason::PrecisionExceeded`]; a
//!   numeral that rounds to an infinity is refused with
//!   [`FidelityReason::Overflow`].
//! - Under `Decimal`, `Int` and `BigInt` a numeral is accepted when its
//!   value lies in the kind's value space and the spec's `precision`,
//!   `scale`, `int_bits` and `unsigned` facets admit it, whatever its
//!   exponent: a decimal keeps an exponent of any size, and under `BigInt`
//!   every numeral whose value is an integer is read, `1e4097` as the same
//!   value as that integer written out, held as a significand and a count
//!   of trailing zeros, so no digit beyond the significand is written.
//!   Under the fixed-width `Int` kinds the range is checked before any
//!   digit is expanded.
//! - Under the other kinds a lexeme is accepted when it is of a class and
//!   form the kind reads (a boolean literal, a string, an encoding of
//!   bytes, an RFC 3339 date or time).
//!
//! # Open schemas
//!
//! With [`ScalarKind::Any`] the value is the most exact one the lexeme
//! allows: an integer that fits `i64` is [`Value::Int`] and a larger one
//! [`Value::BigInt`]; a number with a fraction or exponent is
//! [`Value::Float`] exactly when its binary64 value is finite and its
//! shortest decimal is numerically the lexeme, and [`Value::Decimal`]
//! otherwise (`-0.0` is `Float(-0.0)`, `0.10` is `Float(0.1)`, `4e-324` is
//! a decimal); strings stay strings.
//!
//! # Non-finite floats
//!
//! A NaN spelling (`NaN`, `nan`, `+nan`, `.nan`, in any case) reads as the
//! canonical quiet NaN, [`CANONICAL_NAN`] (bits `0x7ff8000000000000`), and
//! one written with a minus sign (`-nan`, `-NaN`, `-.nan`) as the same NaN
//! with its sign bit set, [`NEGATIVE_CANONICAL_NAN`] (bits
//! `0xfff8000000000000`), under both float
//! kinds and under `Any`. The written sign is carried because a format
//! that writes it (TOML 1.0 makes `-nan` valid with an
//! implementation-specific encoding; Preston-Werner 2021, TOML v1.0.0,
//! Float, stated) would otherwise lose it on a round trip, though the float
//! value space has one notANumber, written `NaN` without a sign (W3C 2012,
//! XML Schema 1.1 Part 2: Datatypes, Secs 3.3.4.1 and 3.3.4.2, stated). No
//! payload is read from a lexeme. The infinity spellings read as the signed
//! infinities.
//!
//! [`FidelityLog`] collects the errors of a whole document, in document
//! order, into one [`ParseError::Fidelity`].

use panproto_gat::Name;
use panproto_schema::value_kinds::{FloatReading, NumeralDialect};
use panproto_schema::{BytesText, ScalarKind, ValueSpec};

use super::{
    BigIntValue, CANONICAL_NAN, DecimalClass, DecimalValue, ExactInteger, FidelityReason, NEGATIVE_CANONICAL_NAN,
    Offset, Temporal, Value, ValueFidelityError, ValueSite, WideInt, binary32_bits,
};
use panproto_expr::exact::{BINARY16_OVERFLOW, binary16_bits, binary16_nearest};
use crate::error::ParseError;

/// A scalar as a source wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lexeme<'a> {
    /// The source text of the scalar, without quotes or escapes.
    pub text: &'a str,
    /// The lexical class the format gave the scalar.
    pub class: LexClass,
}

/// The lexical class a format gives a scalar.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LexClass {
    /// A numeral (JSON number, unquoted YAML or TOML number, SQL numeric
    /// literal), including the NaN and infinity spellings of the formats
    /// that have them.
    Number,
    /// A string.
    String,
    /// A boolean literal.
    Bool,
    /// A null literal.
    Null,
    /// A date, time or date-time literal (TOML, YAML timestamps, SQL
    /// temporal literals).
    DateTime,
    /// Bytes written as text (base64, hexadecimal, or raw).
    Bytes,
    /// A scalar carrying an explicit type tag (a YAML tag, an extended-JSON
    /// type key), named by the tag.
    Tagged(Name),
}

impl<'a> Lexeme<'a> {
    /// The lexeme `text` of class `class`.
    #[must_use]
    pub const fn new(text: &'a str, class: LexClass) -> Self {
        Self { text, class }
    }
}

/// An error for `lexeme` read as `kind`, at an unknown site.
fn fail(lexeme: &Lexeme<'_>, kind: &ScalarKind, reason: FidelityReason) -> ValueFidelityError {
    ValueFidelityError {
        site: ValueSite::unknown(),
        lexeme: lexeme.text.to_owned(),
        target: kind.clone(),
        reason,
    }
}

/// Read `lexeme` as a value of `spec`.
///
/// Under a declared kind the lexeme is read by the kind's lexical mapping
/// and accepted or refused as the [module documentation](self) states:
/// under `Float { bits }` a finite numeral t gives `Float(L(t))`, its IEEE
/// 754 round-to-nearest-even value at the kind's width (32 bits for
/// `bits == 32`, else 64), accepted exactly when `L(t)` is finite and
/// equals t or has t numerically as its shortest text at that width, and
/// refused with [`FidelityReason::Overflow`] when `L(t)` is infinite and
/// with [`FidelityReason::PrecisionExceeded`] otherwise. The shortest text
/// breaks an exact tie to even, so of the two shortest decimals equally
/// close to a float only the one whose last significant digit is even is
/// its shortest text, and the other, which rounds to the same float, is
/// refused unless it equals the float: under `Float { bits: 64 }`
/// `1125899906842624.2` gives 2^50 + 0.25 and `1125899906842624.3` is
/// refused with `PrecisionExceeded { max: 17 }`; under `Any`, which reads a
/// fraction as a float only when it is the float's shortest text, the
/// former gives the float and the latter the decimal it writes; at 32 bits
/// `52401.312` gives 52401.3125 and `52401.313` is refused. A NaN spelling
/// (`NaN`, `nan`, `+nan`, `.nan`, in any case) gives [`CANONICAL_NAN`], bits
/// `0x7ff8000000000000`, and one written with a minus sign (`-nan`, `-NaN`)
/// the same NaN with its sign bit set, [`NEGATIVE_CANONICAL_NAN`], bits
/// `0xfff8000000000000`, under
/// both float kinds and under `Any`: the written sign is carried, so a
/// format that writes `-nan` (valid TOML 1.0 with an implementation-specific
/// encoding; Preston-Werner 2021, TOML v1.0.0, Float, stated) gets it back,
/// although the float value space has one notANumber, written without a
/// sign (W3C 2012, XML Schema 1.1 Part 2: Datatypes, Secs 3.3.4.1 and
/// 3.3.4.2, stated); no payload is read from a lexeme. The infinity
/// spellings give the signed infinities.
///
/// A numeral whose significand digits are all zero is zero whatever its
/// exponent: under `Int`, `BigInt`, `Float { bits }` and `Any`, whose
/// values keep no scale, it reads as `0e0` does (as `-0e0` does when written
/// with a minus sign), and under `Decimal` as the zero of its written scale
/// (`0.0e-3000000000` has scale 3000000001), since a [`DecimalValue`] keeps
/// an exponent of any size.
///
/// Under `Int` and `BigInt` every numeral whose value is an integer is
/// read, whatever its exponent: `1e4097`, `1e100000000000000000000` and the
/// integer `1` followed by 4097 zeros written out are read under `BigInt`,
/// the first and the last as one value, held as a significand and a count of
/// trailing zeros so that no digit beyond the significand is written. Under
/// a fixed-width `Int` the range is checked on the count of digits before
/// any digit is expanded, so `1e2147483647` is refused with
/// [`FidelityReason::Overflow`] at once.
///
/// A null lexeme is [`Value::Null`] under every kind: whether the vertex
/// may be null is a validation question, not a fidelity one. The error's
/// site is unknown; the codec sets it with [`ValueFidelityError::at`] (or
/// reads through [`FidelityLog::coerce`], which does).
///
/// ```
/// use panproto_inst::Value;
/// use panproto_inst::value::coerce::{LexClass, Lexeme, coerce};
/// use panproto_inst::value::{DecimalValue, FidelityReason};
/// use panproto_schema::{ScalarKind, ValueSpec};
///
/// let number = |text| Lexeme::new(text, LexClass::Number);
/// let any = ValueSpec::new(ScalarKind::Any);
/// assert_eq!(coerce(&number("0.1"), &any)?, Value::Float(0.1));
/// assert_eq!(
///     coerce(&number("1e400"), &any)?,
///     Value::Decimal(DecimalValue::parse("1e400")?)
/// );
///
/// let money = ValueSpec { scale: Some(2), ..ValueSpec::new(ScalarKind::Decimal) };
/// let err = coerce(&number("1.005"), &money).unwrap_err();
/// assert_eq!(err.reason, FidelityReason::ScaleExceeded { max: 2 });
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Errors
///
/// A [`ValueFidelityError`] naming the lexeme, the kind and the
/// [`FidelityReason`] the kind cannot hold it for.
pub fn coerce(lexeme: &Lexeme<'_>, spec: &ValueSpec) -> Result<Value, ValueFidelityError> {
    if lexeme.class == LexClass::Null {
        return Ok(Value::Null);
    }
    if let LexClass::Tagged(tag) = &lexeme.class {
        return coerce_tagged(lexeme, tag, spec);
    }
    if let Some(dialect) = spec.numeral {
        let numeric_kind = matches!(
            spec.kind,
            ScalarKind::Int { .. } | ScalarKind::BigInt | ScalarKind::Float { .. } | ScalarKind::Decimal
        );
        let numeral_class = lexeme.class == LexClass::Number
            || (lexeme.class == LexClass::String && numeric_kind);
        if numeral_class {
            return coerce_in_dialect(lexeme, dialect, spec);
        }
    }
    let kind = &spec.kind;
    let fail = |reason| fail(lexeme, kind, reason);
    match kind {
        ScalarKind::Any => coerce_any(lexeme),
        ScalarKind::Bool => coerce_bool(lexeme).ok_or_else(|| {
            fail(if lexeme.class == LexClass::Number {
                FidelityReason::NotANumberForKind
            } else {
                FidelityReason::InvalidEncoding
            })
        }),
        ScalarKind::Int { bits, signed } => {
            let bits = spec.int_bits.unwrap_or(*bits);
            let signed = *signed && !spec.unsigned;
            numeric_class(lexeme, kind)?;
            coerce_int(lexeme.text, bits, signed).map_err(fail)
        }
        ScalarKind::BigInt => {
            numeric_class(lexeme, kind)?;
            integral(lexeme.text).map(Value::integer).map_err(fail)
        }
        ScalarKind::Float { bits } => {
            numeric_class(lexeme, kind)?;
            coerce_float(lexeme.text, *bits, spec.float_reading).map_err(fail)
        }
        ScalarKind::Decimal => {
            numeric_class(lexeme, kind)?;
            coerce_decimal(lexeme.text, spec).map_err(fail)
        }
        ScalarKind::Str | ScalarKind::Token => match &lexeme.class {
            LexClass::Number => Err(fail(FidelityReason::NotANumberForKind)),
            LexClass::Bool => Err(fail(FidelityReason::InvalidEncoding)),
            _ if *kind == ScalarKind::Token => Ok(Value::Token(lexeme.text.to_owned())),
            _ => Ok(Value::Str(lexeme.text.to_owned())),
        },
        ScalarKind::Bytes(encoding) => match &lexeme.class {
            LexClass::Number => Err(fail(FidelityReason::NotANumberForKind)),
            LexClass::Bool => Err(fail(FidelityReason::InvalidEncoding)),
            _ => decode_bytes(lexeme.text, encoding)
                .map(Value::Bytes)
                .ok_or_else(|| fail(FidelityReason::InvalidEncoding)),
        },
        ScalarKind::Date | ScalarKind::Time | ScalarKind::DateTime => match &lexeme.class {
            LexClass::Number => Err(fail(FidelityReason::NotANumberForKind)),
            LexClass::Bool => Err(fail(FidelityReason::InvalidTemporal)),
            _ => coerce_temporal(lexeme.text, kind, spec.with_time_zone).map_err(fail),
        },
    }
}

/// Read a numeral lexeme in the numeral `dialect` its route declares: the
/// dialect decides whether the lexeme is a numeral and which number it
/// denotes ([`dialect_numeral`]), and the number, written in the default
/// grammar, is then read under the spec as any numeral is. A lexeme the
/// dialect does not admit is refused with [`FidelityReason::NotANumber`].
/// An error names the lexeme as the route wrote it.
fn coerce_in_dialect(
    lexeme: &Lexeme<'_>,
    dialect: NumeralDialect,
    spec: &ValueSpec,
) -> Result<Value, ValueFidelityError> {
    let Some(numeral) = dialect_numeral(lexeme.text, &lexeme.class, &spec.kind, dialect) else {
        return Err(fail(lexeme, &spec.kind, FidelityReason::NotANumber));
    };
    let plain = ValueSpec {
        numeral: None,
        ..spec.clone()
    };
    coerce(&Lexeme::new(&numeral, lexeme.class.clone()), &plain).map_err(|error| {
        ValueFidelityError {
            lexeme: lexeme.text.to_owned(),
            ..error
        }
    })
}

/// The number the numeral `text` denotes in `dialect`, written in the
/// default numeral grammar (a decimal numeral, or `NaN`, `-NaN`,
/// `Infinity` or `-Infinity`), or `None` when the dialect does not admit
/// `text` as a numeral of `kind`.
///
/// The grammars:
///
/// - `Json`: RFC 8259 Sec 6's number, `-? int frac? exp?` with `int` a `0`
///   or digits without a leading zero; no `+`, no `_`, no radix prefix, no
///   non-finite spelling (Bray 2017, RFC 8259, Sec 6, stated).
/// - `Toml`: `inf` and `nan` with an optional sign; `0x`, `0o` and `0b`
///   integers without a sign; and decimal numerals with an optional sign,
///   `_` only between two digits, digits on both sides of a point and no
///   leading zero (TOML 1.0.0 and 1.1.0, Integer and Float, stated).
/// - `Yaml12`: the YAML 1.2.2 core schema (Sec 10.3.2, stated): `.inf`,
///   `.Inf`, `.INF` with an optional sign, `.nan`, `.NaN`, `.NAN`; `0o` and
///   `0x` integers; and decimal numerals with an optional sign, leading
///   zeros allowed and either side of the point optional, with no `_`.
/// - `Yaml11`: the YAML 1.1 type pages (stated): the special floats of
///   `Yaml12`; `0x`, `0o` and `0b` integers and decimal numerals with `_`
///   between digits; base 60 integers (`190:20:30`); a leading `0` for
///   octal; and a float exponent only with a point, so `1e3` is no numeral.
/// - `Xsd`: XML Schema 1.1's decimal (`(\+|-)?([0-9]+(\.[0-9]*)?|\.[0-9]+)`,
///   Sec 3.3.3) and, under a float kind, double (Sec 3.3.5) with `INF`,
///   `+INF`, `-INF` and `NaN`, case-sensitive (W3C 2012, XML Schema 1.1
///   Part 2: Datatypes, stated); `-NaN` is admitted too and reads as the
///   NaN with its sign set.
/// - `Java`: decimal numerals, and under a float kind `NaN`, `Infinity`
///   and `-Infinity` as `Double.toString` writes them (Oracle, Java SE 21
///   API, `Double.toString`, stated).
/// - `ProtoJson`: JSON's numbers, and for a string lexeme under a float
///   kind the strings `NaN`, `Infinity` and `-Infinity` (Protocol Buffers,
///   `ProtoJSON` format, stated).
/// - `Sql`: decimal numerals, and under a float or decimal kind
///   PostgreSQL's `NaN`, `Infinity`, `-Infinity`, `inf` and `-inf`, in any
///   case (PostgreSQL 16, Sec 8.1.3 and 8.1.2, stated).
fn dialect_numeral(
    text: &str,
    class: &LexClass,
    kind: &ScalarKind,
    dialect: NumeralDialect,
) -> Option<String> {
    let float_kind = matches!(kind, ScalarKind::Float { .. });
    match dialect {
        NumeralDialect::Json => json_number(text).then(|| text.to_owned()),
        NumeralDialect::Toml => toml_numeral(text),
        NumeralDialect::Yaml12 => yaml12_numeral(text),
        NumeralDialect::Yaml11 => yaml11_numeral(text),
        NumeralDialect::Xsd => xsd_numeral(text, float_kind),
        NumeralDialect::Java => {
            if float_kind && matches!(text, "NaN" | "Infinity" | "-Infinity") {
                return Some(text.to_owned());
            }
            decimal_numeral(text, DecimalGrammar::PERMISSIVE).then(|| text.to_owned())
        }
        NumeralDialect::ProtoJson => {
            if *class == LexClass::String
                && float_kind
                && matches!(text, "NaN" | "Infinity" | "-Infinity")
            {
                return Some(text.to_owned());
            }
            json_number(text).then(|| text.to_owned())
        }
        NumeralDialect::Sql => {
            if float_kind || matches!(kind, ScalarKind::Decimal) {
                let special = match text.to_ascii_lowercase().as_str() {
                    "nan" => Some("NaN"),
                    "-nan" => Some("-NaN"),
                    "infinity" | "+infinity" | "inf" | "+inf" => Some("Infinity"),
                    "-infinity" | "-inf" => Some("-Infinity"),
                    _ => None,
                };
                if let Some(special) = special {
                    return Some(special.to_owned());
                }
            }
            decimal_numeral(text, DecimalGrammar::PERMISSIVE).then(|| text.to_owned())
        }
    }
}

/// Which features a decimal numeral grammar admits beyond an optional
/// minus sign and digits, as a set of the flags below.
#[derive(Clone, Copy)]
struct DecimalGrammar(u8);

impl DecimalGrammar {
    /// A leading `+`.
    const PLUS: u8 = 1;
    /// A leading zero before further integer digits.
    const LEADING_ZERO: u8 = 2;
    /// A decimal point.
    const POINT: u8 = 4;
    /// An exponent.
    const EXPONENT: u8 = 8;

    const JSON: Self = Self(Self::POINT | Self::EXPONENT);
    const TOML: Self = Self(Self::PLUS | Self::POINT | Self::EXPONENT);
    const PERMISSIVE: Self = Self(Self::PLUS | Self::LEADING_ZERO | Self::POINT | Self::EXPONENT);
    const XSD_DECIMAL: Self = Self(Self::PLUS | Self::LEADING_ZERO | Self::POINT);

    const fn admits(self, feature: u8) -> bool {
        self.0 & feature != 0
    }
}

/// Whether `text` is a decimal numeral of `grammar`: a sign, digits with an
/// optional point (one side may be empty, not both), and an optional
/// exponent `e`/`E` with an optional sign and at least one digit, each as
/// the grammar admits.
fn decimal_numeral(text: &str, grammar: DecimalGrammar) -> bool {
    let bytes = text.as_bytes();
    let mut at = match bytes.first() {
        Some(b'+') if grammar.admits(DecimalGrammar::PLUS) => 1,
        Some(b'+') => return false,
        Some(b'-') => 1,
        _ => 0,
    };
    if at == bytes.len() {
        return false;
    }
    let start = at;
    while bytes.get(at).is_some_and(u8::is_ascii_digit) {
        at += 1;
    }
    let integer = &text[start..at];
    let fraction = if grammar.admits(DecimalGrammar::POINT) && bytes.get(at) == Some(&b'.') {
        at += 1;
        let from = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        at - from
    } else {
        0
    };
    if integer.is_empty() && fraction == 0 {
        return false;
    }
    if !grammar.admits(DecimalGrammar::LEADING_ZERO) && integer.len() > 1 && integer.starts_with('0')
    {
        return false;
    }
    if grammar.admits(DecimalGrammar::EXPONENT) && matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        if matches!(bytes.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        let from = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if from == at {
            return false;
        }
    }
    at == bytes.len()
}

/// The mantissa of a numeral without its sign: the text before any
/// exponent.
fn unsigned_mantissa(text: &str) -> &str {
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    unsigned.split(['e', 'E']).next().unwrap_or(unsigned)
}

/// RFC 8259's number: the JSON decimal grammar with digits on both sides of
/// any point.
fn json_number(text: &str) -> bool {
    let mantissa = text.split(['e', 'E']).next().unwrap_or(text);
    decimal_numeral(text, DecimalGrammar::JSON)
        && !mantissa.ends_with('.')
        && !mantissa.starts_with('.')
        && !mantissa.starts_with("-.")
}

/// `text` with its `_` digit separators removed, when every `_` stands
/// between two characters that `digit` accepts.
fn without_separators(text: &str, digit: impl Fn(u8) -> bool) -> Option<String> {
    let bytes = text.as_bytes();
    let separated = |i: usize| {
        i.checked_sub(1)
            .and_then(|before| bytes.get(before))
            .is_some_and(|b| digit(*b))
            && bytes.get(i + 1).is_some_and(|b| digit(*b))
    };
    bytes
        .iter()
        .enumerate()
        .all(|(i, b)| *b != b'_' || separated(i))
        .then(|| text.replace('_', ""))
}

/// The decimal digits of the non-negative integer the digits `digits`
/// write in base `radix` (2 to 16, either case), or `None` when `digits` is
/// empty or holds a digit the radix lacks.
fn radix_to_decimal(digits: &str, radix: u32) -> Option<String> {
    if digits.is_empty() {
        return None;
    }
    // Little-endian decimal digits of the value read so far.
    let mut value: Vec<u32> = vec![0];
    for c in digits.chars() {
        let mut carry = c.to_digit(radix)?;
        for digit in &mut value {
            let next = *digit * radix + carry;
            *digit = next % 10;
            carry = next / 10;
        }
        while carry > 0 {
            value.push(carry % 10);
            carry /= 10;
        }
    }
    while value.len() > 1 && value.last() == Some(&0) {
        value.pop();
    }
    Some(value.iter().rev().filter_map(|d| char::from_digit(*d, 10)).collect())
}

/// The decimal digits of a `0x`, `0o` or `0b` integer (no sign), with `_`
/// separators between its digits.
fn prefixed_integer(text: &str) -> Option<String> {
    let (digits, radix) = [("0x", 16), ("0o", 8), ("0b", 2)]
        .into_iter()
        .find_map(|(prefix, radix)| text.strip_prefix(prefix).map(|d| (d, radix)))?;
    radix_to_decimal(&without_separators(digits, |b| b.is_ascii_hexdigit())?, radix)
}

/// A TOML numeral (see [`dialect_numeral`]).
fn toml_numeral(text: &str) -> Option<String> {
    let special = match text {
        "inf" | "+inf" => Some("Infinity"),
        "-inf" => Some("-Infinity"),
        "nan" | "+nan" => Some("NaN"),
        "-nan" => Some("-NaN"),
        _ => None,
    };
    if let Some(special) = special {
        return Some(special.to_owned());
    }
    if !text.starts_with(['+', '-'])
        && let Some(value) = prefixed_integer(text)
    {
        return Some(value);
    }
    let plain = without_separators(text, |b| b.is_ascii_digit())?;
    let mantissa = unsigned_mantissa(&plain);
    (decimal_numeral(&plain, DecimalGrammar::TOML)
        && !mantissa.starts_with('.')
        && !mantissa.ends_with('.'))
    .then_some(plain)
}

/// The special float a YAML lexeme spells, as `NaN`, `-NaN`, `Infinity` or
/// `-Infinity`: `.inf`, `.Inf` or `.INF` with an optional sign, or `.nan`,
/// `.NaN` or `.NAN` with no sign or a minus sign (the NaN with its sign
/// set; the YAML schemas write a NaN without a sign).
fn yaml_special(text: &str) -> Option<&'static str> {
    let (sign, body) = match text.as_bytes().first() {
        Some(b'-') => (Some(true), &text[1..]),
        Some(b'+') => (Some(false), &text[1..]),
        _ => (None, text),
    };
    match (body, sign) {
        (".inf" | ".Inf" | ".INF", None | Some(false)) => Some("Infinity"),
        (".inf" | ".Inf" | ".INF", Some(true)) => Some("-Infinity"),
        (".nan" | ".NaN" | ".NAN", None) => Some("NaN"),
        (".nan" | ".NaN" | ".NAN", Some(true)) => Some("-NaN"),
        _ => None,
    }
}

/// A YAML 1.2 core-schema numeral (see [`dialect_numeral`]).
fn yaml12_numeral(text: &str) -> Option<String> {
    if let Some(special) = yaml_special(text) {
        return Some(special.to_owned());
    }
    if let Some(digits) = text.strip_prefix("0o") {
        return radix_to_decimal(digits, 8);
    }
    if let Some(digits) = text.strip_prefix("0x") {
        return radix_to_decimal(digits, 16);
    }
    decimal_numeral(text, DecimalGrammar::PERMISSIVE).then(|| text.to_owned())
}

/// The decimal digits of `value × 60 + field` for decimal digits `value`.
fn times_sixty_plus(value: &str, field: u32) -> String {
    let mut digits: Vec<u32> = value.chars().rev().filter_map(|c| c.to_digit(10)).collect();
    let mut carry = field;
    for digit in &mut digits {
        let next = *digit * 60 + carry;
        *digit = next % 10;
        carry = next / 10;
    }
    while carry > 0 {
        digits.push(carry % 10);
        carry /= 10;
    }
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    digits.iter().rev().filter_map(|d| char::from_digit(*d, 10)).collect()
}

/// The decimal digits of the base 60 numeral `first(:field)+`: `first`
/// decimal digits with `_` separators (its first digit 1 to 9 when
/// `nonzero_lead`), each field one or two digits of value below 60.
fn sexagesimal(text: &str, nonzero_lead: bool) -> Option<String> {
    let mut fields = text.split(':');
    let first = fields.next()?;
    let lead_ok = first
        .bytes()
        .next()
        .is_some_and(|b| if nonzero_lead { matches!(b, b'1'..=b'9') } else { b.is_ascii_digit() });
    if !lead_ok || !first.bytes().all(|b| b.is_ascii_digit() || b == b'_') {
        return None;
    }
    let mut value = radix_to_decimal(&first.replace('_', ""), 10)?;
    let mut any = false;
    for field in fields {
        let well_formed = (1..=2).contains(&field.len()) && field.bytes().all(|b| b.is_ascii_digit());
        let field_value: u32 = field.parse().ok().filter(|v| well_formed && *v < 60)?;
        value = times_sixty_plus(&value, field_value);
        any = true;
    }
    any.then_some(value)
}

/// A YAML 1.1 numeral (see [`dialect_numeral`]): the `int` and `float`
/// type pages' expressions, with every `_` ignored.
///
/// Integers: `[-+]?0b[0-1_]+`, `[-+]?0[0-7_]+`, `[-+]?(0|[1-9][0-9_]*)`,
/// `[-+]?0x[0-9a-fA-F_]+` and `[-+]?[1-9][0-9_]*(:[0-5]?[0-9])+`. Floats:
/// `[-+]?([0-9][0-9_]*)?\.[0-9_]*([eE][-+][0-9]+)?` (a point required, an
/// exponent's sign required, and at least one digit), the base 60 form
/// `[-+]?[0-9][0-9_]*(:[0-5]?[0-9])+\.[0-9_]*`, and the special floats.
/// The float page prints the base 10 fraction class as `[0-9.]`; its own
/// example `685.230_15e+03` shows that `[0-9_]` is meant, which is what is
/// read here. A prefixed form with no digit (`0b_`, `0x_`) is no numeral.
fn yaml11_numeral(text: &str) -> Option<String> {
    if let Some(special) = yaml_special(text) {
        return Some(special.to_owned());
    }
    let (sign, body) = match text.as_bytes().first() {
        Some(b'-') => ("-", &text[1..]),
        Some(b'+') => ("", &text[1..]),
        _ => ("", text),
    };
    let signed = |digits: String| format!("{sign}{digits}");
    let digits_of = |part: &str, accept: fn(u8) -> bool| {
        (part.bytes().all(|b| accept(b) || b == b'_') && part.bytes().any(accept))
            .then(|| part.replace('_', ""))
    };
    if let Some(part) = body.strip_prefix("0b") {
        return radix_to_decimal(&digits_of(part, |b| matches!(b, b'0' | b'1'))?, 2).map(signed);
    }
    if let Some(part) = body.strip_prefix("0x") {
        return radix_to_decimal(&digits_of(part, |b| b.is_ascii_hexdigit())?, 16).map(signed);
    }
    if body.contains(':') {
        return match body.split_once('.') {
            None => sexagesimal(body, true).map(signed),
            Some((whole, fraction)) => {
                let whole = sexagesimal(whole, false)?;
                fraction
                    .bytes()
                    .all(|b| b.is_ascii_digit() || b == b'_')
                    .then(|| signed(format!("{whole}.{}", fraction.replace('_', ""))))
            }
        };
    }
    if body.len() > 1
        && body.starts_with('0')
        && body[1..].bytes().all(|b| matches!(b, b'0'..=b'7' | b'_'))
    {
        let digits = body[1..].replace('_', "");
        return Some(signed(if digits.is_empty() {
            "0".to_owned()
        } else {
            radix_to_decimal(&digits, 8)?
        }));
    }
    if body == "0" || (body.bytes().next().is_some_and(|b| matches!(b, b'1'..=b'9'))
        && body.bytes().all(|b| b.is_ascii_digit() || b == b'_'))
    {
        return Some(signed(body.replace('_', "")));
    }
    // A float: digits, a point, digits, and an exponent with a sign.
    let (mantissa, exponent) = body
        .find(['e', 'E'])
        .map_or((body, None), |at| (&body[..at], Some(&body[at + 1..])));
    let (whole, fraction) = mantissa.split_once('.')?;
    let whole_ok = whole.is_empty()
        || (whole.as_bytes()[0].is_ascii_digit() && whole.bytes().all(|b| b.is_ascii_digit() || b == b'_'));
    let fraction_ok = fraction.bytes().all(|b| b.is_ascii_digit() || b == b'_');
    let exponent_ok = exponent.is_none_or(|e| {
        e.len() > 1 && matches!(e.as_bytes()[0], b'+' | b'-') && e[1..].bytes().all(|b| b.is_ascii_digit())
    });
    let has_digit = mantissa.bytes().any(|b| b.is_ascii_digit());
    if !(whole_ok && fraction_ok && exponent_ok && has_digit) {
        return None;
    }
    let whole = whole.replace('_', "");
    let fraction = fraction.replace('_', "");
    let exponent = exponent.map(|e| format!("e{e}")).unwrap_or_default();
    Some(signed(format!(
        "{}.{}{exponent}",
        if whole.is_empty() { "0" } else { &whole },
        if fraction.is_empty() { "0" } else { &fraction }
    )))
}

/// An XML Schema numeral (see [`dialect_numeral`]).
fn xsd_numeral(text: &str, float_kind: bool) -> Option<String> {
    if float_kind {
        return match text {
            "INF" | "+INF" => Some("Infinity".to_owned()),
            "-INF" => Some("-Infinity".to_owned()),
            "NaN" | "-NaN" => Some(text.to_owned()),
            _ => decimal_numeral(text, DecimalGrammar::PERMISSIVE).then(|| text.to_owned()),
        };
    }
    decimal_numeral(text, DecimalGrammar::XSD_DECIMAL).then(|| text.to_owned())
}

/// Reject a lexeme whose class cannot write a number: booleans, and bytes.
/// Strings pass, since text formats (CSV, XML) write every number as one.
fn numeric_class(lexeme: &Lexeme<'_>, kind: &ScalarKind) -> Result<(), ValueFidelityError> {
    match &lexeme.class {
        LexClass::Bool | LexClass::Bytes | LexClass::DateTime => {
            Err(fail(lexeme, kind, FidelityReason::NotANumber))
        }
        _ => Ok(()),
    }
}

/// `true` or `false`, in any case.
fn coerce_bool(lexeme: &Lexeme<'_>) -> Option<Value> {
    match &lexeme.class {
        LexClass::Bool | LexClass::String => {
            if lexeme.text.eq_ignore_ascii_case("true") {
                Some(Value::Bool(true))
            } else if lexeme.text.eq_ignore_ascii_case("false") {
                Some(Value::Bool(false))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// The non-finite float a lexeme spells, if it spells one: `NaN`, `inf`,
/// `Infinity` and YAML's `.nan` and `.inf`, in any case, with an optional
/// sign. A NaN spelling gives [`CANONICAL_NAN`], or [`NEGATIVE_CANONICAL_NAN`] when it
/// is written with a minus sign; no payload is read.
fn non_finite(text: &str) -> Option<f64> {
    let (negative, unsigned) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let unsigned = unsigned.strip_prefix('.').unwrap_or(unsigned);
    if unsigned.eq_ignore_ascii_case("nan") {
        Some(if negative {
            NEGATIVE_CANONICAL_NAN
        } else {
            CANONICAL_NAN
        })
    } else if unsigned.eq_ignore_ascii_case("inf") || unsigned.eq_ignore_ascii_case("infinity") {
        Some(if negative {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        })
    } else {
        None
    }
}

/// Whether a numeral has neither a fraction nor an exponent.
fn is_integer_numeral(text: &str) -> bool {
    !text.contains(['.', 'e', 'E'])
}

/// The value of an open-schema lexeme (see the module documentation).
fn coerce_any(lexeme: &Lexeme<'_>) -> Result<Value, ValueFidelityError> {
    let fail = |reason| fail(lexeme, &ScalarKind::Any, reason);
    match &lexeme.class {
        LexClass::Number => {
            if let Some(f) = non_finite(lexeme.text) {
                return Ok(Value::Float(f));
            }
            if is_integer_numeral(lexeme.text) {
                return Value::parse_integer(lexeme.text).map_err(|e| fail(e.reason));
            }
            let decimal = DecimalValue::parse(lexeme.text)
                .ok()
                .filter(DecimalValue::is_finite)
                .ok_or_else(|| fail(FidelityReason::NotANumber))?;
            let x = nearest_float(&decimal, 64);
            Ok(float_if_shortest(&decimal, x).map_or(Value::Decimal(decimal), Value::Float))
        }
        LexClass::Bool => coerce_bool(lexeme).ok_or_else(|| fail(FidelityReason::InvalidEncoding)),
        LexClass::DateTime => Temporal::parse(lexeme.text)
            .map(Value::Temporal)
            .map_err(|e| fail(e.reason)),
        LexClass::Bytes => decode_bytes(lexeme.text, &BytesText::Base64)
            .map(Value::Bytes)
            .ok_or_else(|| fail(FidelityReason::InvalidEncoding)),
        LexClass::String | LexClass::Null | LexClass::Tagged(_) => {
            Ok(Value::Str(lexeme.text.to_owned()))
        }
    }
}

/// `x` when it is finite and its shortest decimal is numerically
/// `decimal`.
fn float_if_shortest(decimal: &DecimalValue, x: f64) -> Option<f64> {
    let back = DecimalValue::from_f64_shortest(x)?;
    back.cmp_numeric(decimal).is_eq().then_some(x)
}

/// The float of `bits` bits nearest the finite `decimal`, widened to
/// `f64`: `L_b`, IEEE 754 round to nearest, ties to even, at the width (16,
/// 32, or 64 for any other width), an infinity when the decimal rounds
/// beyond the width's largest finite value and a zero when it rounds below
/// its least, whatever the size of the exponent. A zero keeps the decimal's
/// sign, so `-0` and `-0e5` give the negative zero. At 32 and 64 bits Rust's
/// correctly rounded parsers round; at 16 bits [`binary16_nearest`] does,
/// exactly.
fn nearest_float(decimal: &DecimalValue, bits: u8) -> f64 {
    match bits {
        16 => binary16_nearest(decimal),
        32 => f64::from(decimal.to_f32()),
        _ => decimal.to_f64(),
    }
}

/// The shortest text that reads back as `x` at `bits` bits (16, 32, or 64
/// for any other width), for a finite `x` that the width holds: `C_b`, its
/// closest-shortest decimal, ties to even.
fn shortest_text(x: f64, bits: u8) -> Option<DecimalValue> {
    match bits {
        16 => DecimalValue::from_f16_shortest(x),
        32 => DecimalValue::from_f32_shortest(f32::from_bits(binary32_bits(x)?)),
        _ => DecimalValue::from_f64_shortest(x),
    }
}

/// The significant digits within which every value of a float width is
/// written by its shortest text: 5 at 16 bits, 9 at 32, 17 at 64.
const fn max_float_digits(bits: u8) -> u32 {
    match bits {
        16 => 5,
        32 => 9,
        _ => 17,
    }
}

/// The most significant digits a finite binary64 value's exact decimal
/// expansion has (that of the largest subnormal); a binary32 value, widened,
/// needs fewer.
const MAX_EXACT_DIGITS: usize = 767;

/// Whether the finite float `x` equals `decimal` exactly. A binary float's
/// decimal expansion terminates, within [`MAX_EXACT_DIGITS`] significant
/// digits, and Rust's formatting with an explicit precision writes the
/// correctly rounded digits, so at that precision it writes the expansion
/// itself.
fn denotes_exactly(decimal: &DecimalValue, x: f64) -> bool {
    if decimal.digits.trim_end_matches('0').len() > MAX_EXACT_DIGITS {
        return false;
    }
    DecimalValue::parse(&format!("{x:.MAX_EXACT_DIGITS$e}"))
        .is_ok_and(|exact| exact.cmp_numeric(decimal).is_eq())
}

/// The exact integer a numeral writes: an integer numeral, or a decimal
/// numeral whose value is integral (`1.0`, `1e3`, `1e4097`), held without
/// expanding its exponent.
fn integral(text: &str) -> Result<ExactInteger, FidelityReason> {
    if non_finite(text).is_some() {
        return Err(FidelityReason::NonFinite);
    }
    if is_integer_numeral(text) {
        return BigIntValue::parse(text).map_err(|e| e.reason);
    }
    let decimal = DecimalValue::parse(text).map_err(|e| e.reason)?;
    BigIntValue::from_scaled(decimal.negative, &decimal.digits, &decimal.exponent)
        .ok_or(FidelityReason::ScaleExceeded { max: 0 })
}

/// An integer of `bits` bits, signed or not.
fn coerce_int(text: &str, bits: u8, signed: bool) -> Result<Value, FidelityReason> {
    let value = integral(text)?;
    let bits = u32::from(bits.clamp(1, 128));
    let in_range = match &value {
        ExactInteger::Int(n) => {
            let n = i128::from(*n);
            if signed {
                let half = 1_i128 << (bits - 1).min(126);
                bits > 127 || (-half <= n && n < half)
            } else {
                n >= 0 && (bits >= 64 || n < (1_i128 << bits))
            }
        }
        ExactInteger::Big(big) => {
            // Outside i64: only a kind wider than 64 bits holds it, and no
            // integer of more than 39 digits fits 128 bits, so the digits
            // are written out only when there are at most 39 of them.
            let magnitude = big
                .digit_count()
                .to_i64()
                .filter(|&n| n <= 39)
                .and_then(|_| big.to_string().trim_start_matches('-').parse::<u128>().ok());
            match (signed, big.negative, magnitude) {
                (_, _, None) | (false, true, _) => false,
                (false, false, Some(m)) => bits >= 128 || m < (1_u128 << bits),
                (true, negative, Some(m)) => {
                    let half = 1_u128 << (bits - 1);
                    if negative { m <= half } else { m < half }
                }
            }
        }
    };
    if in_range {
        Ok(Value::integer(value))
    } else {
        Err(FidelityReason::Overflow)
    }
}

/// A float of `bits` bits (16, 32 or 64; any other width reads as 64)
/// read under `reading`: the value `L_b(t)` nearest the numeral t. Under
/// [`FloatReading::Exact`] it is accepted when it is finite and equals t or
/// its shortest text at the width is numerically t, refused with
/// [`FidelityReason::Overflow`] when it is infinite and with
/// [`FidelityReason::PrecisionExceeded`] otherwise (see the module
/// documentation). Under [`FloatReading::NearestKeepingLexeme`] every
/// finite numeral gives `L_b(t)`, an overflow the infinity of t's sign, and
/// no numeral is refused; the route keeps the lexeme.
fn coerce_float(text: &str, bits: u8, reading: FloatReading) -> Result<Value, FidelityReason> {
    if let Some(f) = non_finite(text) {
        return Ok(Value::Float(f));
    }
    let decimal = DecimalValue::parse(text)
        .ok()
        .filter(DecimalValue::is_finite)
        .ok_or(FidelityReason::NotANumber)?;
    let x = nearest_float(&decimal, bits);
    if reading == FloatReading::NearestKeepingLexeme {
        return Ok(Value::Float(x));
    }
    if !x.is_finite() {
        return Err(FidelityReason::Overflow);
    }
    let shortest = shortest_text(x, bits).is_some_and(|back| back.cmp_numeric(&decimal).is_eq());
    if shortest || denotes_exactly(&decimal, x) {
        Ok(Value::Float(x))
    } else {
        Err(FidelityReason::PrecisionExceeded {
            max: max_float_digits(bits),
        })
    }
}

/// A decimal within the spec's precision and scale, or a special decimal
/// (an infinity or a NaN, which no precision or scale bounds).
///
/// With a scale `s`, the number may need at most `s` digits after the
/// point (trailing zeros beyond it are no loss); with a precision `p` as
/// well, at most `p - s` before it; with a precision alone, at most `p`
/// digits in all.
fn coerce_decimal(text: &str, spec: &ValueSpec) -> Result<Value, FidelityReason> {
    let decimal = DecimalValue::parse(text).map_err(|e| e.reason)?;
    if !decimal.is_finite() {
        return Ok(Value::Decimal(decimal));
    }
    if let Some(scale) = spec.scale
        && decimal.minimal_scale() > WideInt::from(scale)
    {
        return Err(FidelityReason::ScaleExceeded { max: scale });
    }
    if let Some(precision) = spec.precision {
        let fits = spec.scale.map_or_else(
            || decimal.significant_digits() <= WideInt::from(precision),
            |scale| decimal.integer_digits() <= WideInt::from(precision.saturating_sub(scale)),
        );
        if !fits {
            return Err(FidelityReason::PrecisionExceeded { max: precision });
        }
    }
    Ok(Value::Decimal(decimal))
}

/// A date, time or date-time of `kind`, with the offset `with_time_zone`
/// requires (`Some(true)`: an offset; `Some(false)`: none).
fn coerce_temporal(
    text: &str,
    kind: &ScalarKind,
    with_time_zone: Option<bool>,
) -> Result<Value, FidelityReason> {
    let parsed = match kind {
        ScalarKind::Date => Temporal::parse_date(text),
        ScalarKind::Time => Temporal::parse_time(text),
        _ => Temporal::parse_date_time(text),
    }
    .map_err(|e| e.reason)?;
    match (parsed.offset(), with_time_zone) {
        (Some(Offset::Local), Some(true)) => Err(FidelityReason::OffsetRequired),
        (Some(offset), Some(false)) if offset.is_zoned() => Err(FidelityReason::OffsetForbidden),
        _ => Ok(Value::Temporal(parsed)),
    }
}

/// The value of a digit in `alphabet`, if it is one.
fn digit_value(alphabet: &[u8; 64], byte: u8) -> Option<u32> {
    alphabet
        .iter()
        .position(|&b| b == byte)
        .and_then(|i| u32::try_from(i).ok())
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const BASE64_URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Decode base64 over `alphabet`, padded or not; `None` for a character
/// outside the alphabet, a misplaced or wrong amount of padding, a length
/// no encoding produces, or nonzero bits after the last byte (which a
/// canonical encoder never writes, so two texts never decode to one
/// value).
fn decode_base64(text: &str, alphabet: &[u8; 64]) -> Option<Vec<u8>> {
    let body = text.trim_end_matches('=');
    let padding = text.len() - body.len();
    if padding > 2 || (padding > 0 && text.len() % 4 != 0) || body.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(body.len() * 3 / 4);
    let mut acc = 0_u32;
    let mut bits = 0_u32;
    for byte in body.bytes() {
        acc = (acc << 6) | digit_value(alphabet, byte)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xff).ok()?);
            acc &= (1 << bits) - 1;
        }
    }
    (acc == 0).then_some(out)
}

/// Decode hexadecimal, either case, an even number of digits.
fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    text.as_bytes()
        .chunks(2)
        .map(|pair| {
            let high = char::from(pair[0]).to_digit(16)?;
            let low = char::from(pair[1]).to_digit(16)?;
            u8::try_from(high * 16 + low).ok()
        })
        .collect()
}

/// The bytes `text` encodes in `encoding`.
fn decode_bytes(text: &str, encoding: &BytesText) -> Option<Vec<u8>> {
    match encoding {
        BytesText::Base64 => decode_base64(text, BASE64),
        BytesText::Base64Url => decode_base64(text, BASE64_URL),
        BytesText::Hex => decode_hex(text),
        BytesText::Raw => Some(text.as_bytes().to_vec()),
    }
}

/// A type tag without any YAML `tag:yaml.org,2002:` or `!!` prefix.
fn tag_name(tag: &str) -> &str {
    tag.strip_prefix("tag:yaml.org,2002:")
        .or_else(|| tag.strip_prefix("!!"))
        .unwrap_or(tag)
}

/// The scalar kind a type tag names (see [`tag_name`]): the YAML core and
/// extended-JSON names of the scalar kinds.
fn tagged_kind(name: &str) -> Option<ScalarKind> {
    Some(match name {
        "bool" | "boolean" => ScalarKind::Bool,
        "int" | "integer" | "numberLong" | "numberInt" => ScalarKind::BigInt,
        "float" | "double" | "numberDouble" => ScalarKind::Float { bits: 64 },
        "decimal" | "numberDecimal" => ScalarKind::Decimal,
        "str" | "string" => ScalarKind::Str,
        "binary" | "base64" => ScalarKind::Bytes(BytesText::Base64),
        "hex" => ScalarKind::Bytes(BytesText::Hex),
        "timestamp" | "datetime" | "date-time" => ScalarKind::DateTime,
        "date" => ScalarKind::Date,
        "time" => ScalarKind::Time,
        _ => return None,
    })
}

/// A tagged scalar: the declared kind governs when there is one; under
/// [`ScalarKind::Any`] the tag's own kind does, and a tag naming no scalar
/// kind keeps the scalar as an opaque value of that type, so the tag is
/// never dropped.
fn coerce_tagged(
    lexeme: &Lexeme<'_>,
    tag: &Name,
    spec: &ValueSpec,
) -> Result<Value, ValueFidelityError> {
    if spec.kind != ScalarKind::Any {
        return coerce(&Lexeme::new(lexeme.text, LexClass::String), spec);
    }
    let name = tag_name(tag.as_ref());
    if name == "null" {
        return Ok(Value::Null);
    }
    tagged_kind(name).map_or_else(
        || {
            Ok(Value::Opaque {
                type_: tag.as_ref().to_owned(),
                fields: std::iter::once(("value".to_owned(), Value::Str(lexeme.text.to_owned())))
                    .collect(),
            })
        },
        |kind| {
            let class = match kind {
                ScalarKind::Bool => LexClass::Bool,
                ScalarKind::Str => LexClass::String,
                ScalarKind::Bytes(_) => LexClass::Bytes,
                ScalarKind::Date | ScalarKind::Time | ScalarKind::DateTime => LexClass::DateTime,
                _ => LexClass::Number,
            };
            coerce(&Lexeme::new(lexeme.text, class), &ValueSpec::new(kind))
        },
    )
}

/// Read a lexeme whose kind a sibling discriminator names, under `spec`.
///
/// A value position whose kind is named by a sibling (jCal's and jCard's
/// value-type element, FHIR's `resourceType`, atproto's `$type`) is read
/// through the discriminator table of its spec ([`ValueSpec::discriminators`]):
/// a lexeme whose `tag` the table holds is read by [`coerce`] under the
/// spec with that kind, its class unchanged, so a number tagged as text is
/// refused under `Str` as an untagged one is; a lexeme whose tag the table
/// lacks is kept, never refused and never typed by guess, as
/// `Value::Opaque { type_: tag, fields: { "value": Str(lexeme) } }` with
/// the lexeme byte for byte. A lexeme with no tag, and every lexeme under a
/// spec with no table, is read by [`coerce`] under the spec as it is.
///
/// ```
/// use std::collections::BTreeMap;
///
/// use panproto_inst::Value;
/// use panproto_inst::value::coerce::{LexClass, Lexeme, coerce_discriminated};
/// use panproto_schema::{ScalarKind, ValueSpec};
///
/// let spec = ValueSpec {
///     discriminators: Some(BTreeMap::from([
///         ("integer".to_owned(), ScalarKind::Int { bits: 32, signed: true }),
///         ("text".to_owned(), ScalarKind::Str),
///     ])),
///     ..ValueSpec::new(ScalarKind::Any)
/// };
/// let string = Lexeme::new("42", LexClass::String);
/// assert_eq!(coerce_discriminated(&string, Some("integer"), &spec)?, Value::Int(42));
/// let custom = coerce_discriminated(&Lexeme::new("abc", LexClass::String), Some("x-custom"), &spec)?;
/// assert!(matches!(custom, Value::Opaque { type_, .. } if type_ == "x-custom"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Errors
///
/// The [`ValueFidelityError`] [`coerce`] gives under the kind the tag
/// names, or under the spec's own kind.
pub fn coerce_discriminated(
    lexeme: &Lexeme<'_>,
    tag: Option<&str>,
    spec: &ValueSpec,
) -> Result<Value, ValueFidelityError> {
    let (Some(tag), Some(table)) = (tag, &spec.discriminators) else {
        return coerce(lexeme, spec);
    };
    table.get(tag).map_or_else(
        || {
            Ok(Value::Opaque {
                type_: tag.to_owned(),
                fields: std::iter::once((
                    "value".to_owned(),
                    Value::Str(lexeme.text.to_owned()),
                ))
                .collect(),
            })
        },
        |kind| {
            coerce(
                lexeme,
                &ValueSpec {
                    kind: kind.clone(),
                    ..spec.clone()
                },
            )
        },
    )
}

/// The fidelity errors of one document, collected in the order its values
/// are read.
///
/// A codec reads every scalar through [`coerce`](Self::coerce), which
/// records an error instead of stopping, so one parse reports every value
/// the document cannot hold; [`finish`](Self::finish) turns them into one
/// [`ParseError::Fidelity`]. There is no lenient mode: a value whose error
/// is recorded is not produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FidelityLog {
    errors: Vec<ValueFidelityError>,
}

impl FidelityLog {
    /// An empty log.
    #[must_use]
    pub const fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// Read `lexeme` as a value of `spec` at `site`, recording the error
    /// (with its site) and returning `None` when the kind cannot hold it.
    pub fn coerce(
        &mut self,
        site: ValueSite,
        lexeme: &Lexeme<'_>,
        spec: &ValueSpec,
    ) -> Option<Value> {
        match coerce(lexeme, spec) {
            Ok(value) => Some(value),
            Err(error) => {
                self.errors.push(error.at(site));
                None
            }
        }
    }

    /// Record an error the codec found itself.
    pub fn record(&mut self, error: ValueFidelityError) {
        self.errors.push(error);
    }

    /// The errors recorded so far, in order.
    #[must_use]
    pub fn errors(&self) -> &[ValueFidelityError] {
        &self.errors
    }

    /// `Ok` when nothing was recorded, else every recorded error, in order.
    ///
    /// # Errors
    ///
    /// [`ParseError::Fidelity`] listing every recorded error.
    pub fn finish(self) -> Result<(), ParseError> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(ParseError::Fidelity(self.errors))
        }
    }
}

/// Coerce an already-built `value` to `spec`, at `site`: the value a column
/// of kind `spec` holds for it.
///
/// The value is read through its lexeme, with these rules:
///
/// - Under [`ScalarKind::Any`] every value is returned unchanged.
/// - A [`Value::Float`] under `Float { bits: 32 }` is returned unchanged
///   exactly when binary32 holds it ([`binary32_bits`]): a finite value
///   that 24 significant bits and the binary32 exponent range hold, `-0.0`
///   included; an infinity; or a NaN whose low 29 fraction bits are zero,
///   quiet or signalling, decided on its bits. Otherwise it is refused: a
///   NaN with [`FidelityReason::NonFinite`], since binary32 cannot hold its
///   payload; a finite value that rounds to an infinity at binary32, that
///   is of magnitude at least 2^128 - 2^103 = `3.4028235677973366e38`, with
///   [`FidelityReason::Overflow`]; and every other finite value with
///   [`FidelityReason::PrecisionExceeded`] with `max` 9. These are the
///   reason and `max` the lexeme route gives whenever it also refuses the
///   value's exact decimal text. So `Float(0.1)` is refused, while
///   `Float(0.10000000149011612)`, the binary32 nearest 0.1, is kept: a
///   stored value never differs from its input.
/// - A non-finite [`Value::Float`] under any other float kind is returned
///   unchanged, sign and payload included, and refused with
///   [`FidelityReason::NonFinite`] under a kind that is not a float kind.
/// - A [`Value::Int`], [`Value::BigInt`] or [`Value::Decimal`] under
///   `Float { bits }` gives `Float(x)` exactly when the kind holds its value
///   x exactly (a binary16 value for `bits == 16`, a binary32 value for
///   `bits == 32`, a binary64 value for every other width; a zero gives the
///   positive zero), and is refused otherwise: with
///   [`FidelityReason::Overflow`] when the value rounds to an infinity at
///   that width, and with [`FidelityReason::PrecisionExceeded`] else, `max`
///   5, 9 or 17. So `Decimal(0.1)` is refused under both 32 and 64 bits,
///   though [`coerce`] reads the lexeme `0.1` under them as the float
///   nearest it; `Int(16777216)` is kept under 32 bits and `Int(16777217)`
///   refused. The lexeme route under `Float { bits: 16 }` reads binary64
///   until a binary16 lexical mapping is stated, so `coerce` of `2049`
///   there gives `Float(2049.0)` while `Int(2049)` is refused here.
/// - Every other value with a scalar lexeme is [`coerce`] of that lexeme:
///   `true` or `false` of class [`LexClass::Bool`] for a boolean, the
///   decimal digits of class [`LexClass::Number`] for an integer that fits
///   `i64`, the exponent form `<sign><significand>e<count>` of class
///   `Number` for a [`Value::BigInt`] (so no digit beyond the significand is
///   written), the canonical text of class `Number` for a decimal, the
///   closest-shortest binary64 text (ties to even, written positionally) of
///   class `Number` for a finite float, the text of class
///   [`LexClass::String`] for a string or a token, the RFC 3339 text of
///   class [`LexClass::DateTime`] for a date or time, and `null` of class
///   [`LexClass::Null`] for null.
/// - Any other value (bytes, links, blobs, records, lists, labelled nulls)
///   has no scalar lexeme and is returned unchanged; validation types it.
///
/// An `Ok` result is therefore the kind's reading of the input's lexeme
/// and, under a float kind, a value equal to the input; coercing it again
/// returns it unchanged.
///
/// The two routes into a 32-bit float agree wherever both succeed: for a
/// finite numeral t, if `coerce(t, Float { bits: 32 })` and
/// `coerce_value(coerce(t, Float { bits: 64 })?, Float { bits: 32 })` are
/// both `Ok`, they are equal. The second is `Ok` only if y, the binary64
/// nearest t, is a binary32 value, and then it is y. Let z be any other
/// binary32 value. No two distinct binary32 values are adjacent binary64
/// values, so some binary64 value w lies strictly between y and z. If z
/// were no farther from t than y, then, y being the binary64 nearest t, t
/// would be the midpoint of y and z, and w would be nearer t than y; so z
/// is farther from t than y, y is the binary32 nearest t with no tie, and
/// `|t|` stays below the binary32 overflow threshold (t is within half a
/// binary64 unit of a finite binary32 value). Both routes give zeros the
/// sign of t. So the first returns y too. Their refusals differ in both
/// directions: `0.10000000149011612` is refused directly and kept on the
/// path, and `0.1` is accepted directly and refused on the path.
///
/// ```
/// use panproto_inst::Value;
/// use panproto_inst::value::{DecimalValue, FidelityReason, ValueSite};
/// use panproto_inst::value::coerce::coerce_value;
/// use panproto_schema::{ScalarKind, ValueSpec};
///
/// let int = ValueSpec::new(ScalarKind::Int { bits: 64, signed: true });
/// let site = ValueSite::new("person.age", None);
/// assert_eq!(coerce_value(&Value::Float(2.0), &int, site.clone())?, Value::Int(2));
/// assert!(coerce_value(&Value::Float(2.5), &int, site.clone()).is_err());
/// let any = ValueSpec::new(ScalarKind::Any);
/// assert_eq!(coerce_value(&Value::Float(2.0), &any, site.clone())?, Value::Float(2.0));
/// let narrow = ValueSpec::new(ScalarKind::Float { bits: 32 });
/// let refused = coerce_value(&Value::Float(0.1), &narrow, site.clone()).unwrap_err();
/// assert_eq!(refused.reason, FidelityReason::PrecisionExceeded { max: 9 });
/// let exact = Value::Float(f64::from(0.1_f32));
/// assert_eq!(coerce_value(&exact, &narrow, site.clone())?, exact);
/// let tenth = Value::Decimal(DecimalValue::parse("0.1")?);
/// let refused = coerce_value(&tenth, &narrow, site.clone()).unwrap_err();
/// assert_eq!(refused.reason, FidelityReason::PrecisionExceeded { max: 9 });
/// assert_eq!(coerce_value(&Value::Int(16_777_216), &narrow, site)?, Value::Float(16_777_216.0));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Errors
///
/// A [`ValueFidelityError`] at `site` when the kind refuses the value, for
/// the reason the rules above give.
pub fn coerce_value(
    value: &Value,
    spec: &ValueSpec,
    site: ValueSite,
) -> Result<Value, ValueFidelityError> {
    let refuse = |site, lexeme: String, reason| ValueFidelityError {
        site,
        lexeme,
        target: spec.kind.clone(),
        reason,
    };
    if spec.kind == ScalarKind::Any {
        return Ok(value.clone());
    }
    let (text, class) = match (value, &spec.kind) {
        (Value::Float(x), ScalarKind::Float { bits: 32 }) => {
            return if binary32_bits(*x).is_some() {
                Ok(value.clone())
            } else if !x.is_finite() {
                Err(refuse(site, x.to_string(), FidelityReason::NonFinite))
            } else if x.abs() >= BINARY32_OVERFLOW {
                Err(refuse(site, x.to_string(), FidelityReason::Overflow))
            } else {
                Err(refuse(
                    site,
                    x.to_string(),
                    FidelityReason::PrecisionExceeded { max: 9 },
                ))
            };
        }
        (Value::Float(x), ScalarKind::Float { bits: 16 }) => {
            return if binary16_bits(*x).is_some() {
                Ok(value.clone())
            } else if !x.is_finite() {
                Err(refuse(site, x.to_string(), FidelityReason::NonFinite))
            } else if x.abs() >= BINARY16_OVERFLOW {
                Err(refuse(site, x.to_string(), FidelityReason::Overflow))
            } else {
                Err(refuse(
                    site,
                    x.to_string(),
                    FidelityReason::PrecisionExceeded { max: 5 },
                ))
            };
        }
        (Value::Float(x), kind) if !x.is_finite() => {
            return if matches!(kind, ScalarKind::Float { .. }) {
                Ok(value.clone())
            } else {
                Err(refuse(site, x.to_string(), FidelityReason::NonFinite))
            };
        }
        (Value::Decimal(d), ScalarKind::Float { .. } | ScalarKind::BigInt) if !d.is_finite() => {
            return Err(refuse(site, d.to_string(), FidelityReason::NonFinite));
        }
        (Value::Int(_) | Value::BigInt(_) | Value::Decimal(_), ScalarKind::Float { bits }) => {
            let (decimal, lexeme) = match value {
                Value::Int(n) => (DecimalValue::from_i64(*n), n.to_string()),
                Value::BigInt(n) => (
                    DecimalValue {
                        negative: n.negative,
                        digits: n.significand.clone(),
                        exponent: n.zeros.clone(),
                        class: DecimalClass::Finite,
                    },
                    n.exponent_form(),
                ),
                Value::Decimal(d) => (d.clone(), d.to_string()),
                _ => return Ok(value.clone()),
            };
            return held_exactly(&decimal, *bits)
                .map(Value::Float)
                .map_err(|reason| refuse(site, lexeme, reason));
        }
        (Value::Float(x), _) => (float_text(*x), LexClass::Number),
        (Value::Bool(b), _) => (b.to_string(), LexClass::Bool),
        (Value::Int(n), _) => (n.to_string(), LexClass::Number),
        (Value::BigInt(n), _) => (n.exponent_form(), LexClass::Number),
        (Value::Decimal(d), _) => (d.to_string(), LexClass::Number),
        (Value::Str(s) | Value::Token(s), _) => (s.clone(), LexClass::String),
        (Value::Temporal(t), _) => (t.to_string(), LexClass::DateTime),
        (Value::Null, _) => ("null".to_owned(), LexClass::Null),
        (
            Value::Bytes(_)
            | Value::CidLink(_)
            | Value::Blob { .. }
            | Value::Opaque { .. }
            | Value::Unknown(_)
            | Value::List(_)
            | Value::LabeledNull(_),
            _,
        ) => return Ok(value.clone()),
    };
    coerce(&Lexeme::new(&text, class), spec).map_err(|error| error.at(site))
}

/// The float of `bits` bits (16, 32, or 64 for any other width) equal to
/// the finite `decimal`, widened to `f64`, when the width holds that number
/// exactly; a zero gives the zero of its sign. Otherwise [`FidelityReason::Overflow`]
/// when the number rounds to an infinity at the width under
/// round-to-nearest-even, and [`FidelityReason::PrecisionExceeded`] with
/// the width's `max` (5, 9 or 17) else.
///
/// The number is held exactly iff the float of the width nearest it, `L_b`
/// ([`nearest_float`]), equals it; `L_b` is infinite exactly when the number
/// rounds to an infinity (at 16 bits, when its magnitude is at least 65520,
/// the largest binary16, 65504, plus half its gap, a tie that goes to the
/// even neighbour 2^16 and so overflows).
fn held_exactly(decimal: &DecimalValue, bits: u8) -> Result<f64, FidelityReason> {
    if decimal.is_zero() {
        return Ok(if decimal.negative { -0.0 } else { 0.0 });
    }
    let x = nearest_float(decimal, bits);
    if !x.is_finite() {
        Err(FidelityReason::Overflow)
    } else if denotes_exactly(decimal, x) {
        Ok(x)
    } else {
        Err(FidelityReason::PrecisionExceeded {
            max: max_float_digits(bits),
        })
    }
}

/// The lexeme of a finite float: its closest-shortest binary64 decimal
/// ([`DecimalValue::from_f64_shortest`]: the minimal significand, the
/// candidate closest to `x`, and at an exact tie the one whose last
/// significant digit is even, ties to even as ECMA-262's `Number::toString`
/// recommends and as this follows; Ecma International TC39, ECMAScript
/// Language Specification, Sec 6.1.6.1.20, Note 2, stated), written
/// positionally (`1e300` as a 1 and
/// 300 zeros, `1e-7` as `0.0000001`) with a `-` for a negative value or
/// the negative zero (`-0`), as Rust's `Display` of an `f64` writes, except
/// that at an exact tie `Display` takes the candidate of larger magnitude.
fn float_text(x: f64) -> String {
    let sign = if x.is_sign_negative() { "-" } else { "" };
    let Some(decimal) = DecimalValue::from_f64_shortest(x) else {
        return x.to_string();
    };
    let exponent = decimal.exponent.to_i64().unwrap_or(0);
    let digits = decimal.digits;
    let magnitude = usize::try_from(exponent).map_or_else(
        |_| {
            let scale = usize::try_from(-exponent).unwrap_or(0);
            if scale >= digits.len() {
                format!("0.{}{digits}", "0".repeat(scale - digits.len()))
            } else {
                let (int, frac) = digits.split_at(digits.len() - scale);
                format!("{int}.{frac}")
            }
        },
        |zeros| format!("{digits}{}", "0".repeat(zeros)),
    );
    format!("{sign}{magnitude}")
}

/// The least magnitude that rounds to an infinity at binary32 under
/// round-to-nearest-even: 2^128 - 2^103, the largest binary32,
/// (2^24 - 1) 2^104, plus half its gap to 2^128 (a tie, which goes to the
/// even neighbour 2^128 and so overflows).
const BINARY32_OVERFLOW: f64 = f64::from_bits(0x47ef_ffff_f000_0000);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_float_comparison_keeps_the_767_digit_boundary() {
        let x = f64::from_bits(0x000f_ffff_ffff_ffff);
        let Ok(decimal) = DecimalValue::parse(&format!("{x:.MAX_EXACT_DIGITS$e}")) else {
            panic!("the exact expansion is a decimal")
        };
        assert_eq!(decimal.digits.trim_end_matches('0').len(), MAX_EXACT_DIGITS);
        assert!(denotes_exactly(&decimal, x));
    }

    #[test]
    fn binary16_membership_agrees_with_its_bit_boundaries() {
        for x in [
            0.0,
            -0.0,
            1.0,
            1.0 + 2.0_f64.powi(-10),
            2.0_f64.powi(-24),
            1023.0 * 2.0_f64.powi(-24),
            2.0_f64.powi(-14),
            65_504.0,
        ] {
            assert!(binary16_bits(x).is_some(), "{x:e}");
        }
        for x in [
            f64::from_bits(1),
            1.0 + 2.0_f64.powi(-11),
            1.5 * 2.0_f64.powi(-24),
            2.0_f64.powi(-25),
            2.0_f64.powi(16),
        ] {
            assert!(binary16_bits(x).is_none(), "{x:e}");
        }
    }

    #[test]
    fn integer_and_float_text_boundaries_use_their_exact_counts() {
        assert!(matches!(
            coerce_int("18446744073709551616", 128, false),
            Ok(Value::BigInt(_))
        ));
        assert!(matches!(
            coerce_int("-18446744073709551616", 128, true),
            Ok(Value::BigInt(_))
        ));
        assert_eq!(float_text(12.5), "12.5");
        assert_eq!(float_text(-12.5), "-12.5");
        assert_eq!(float_text(0.125), "0.125");
    }
}
