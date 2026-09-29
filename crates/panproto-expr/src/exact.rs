//! Exact scalar values: decimals, integers wider than `i64`, and civil
//! dates and times.
//!
//! A binary float cannot hold `0.1`, an `i64` cannot hold `u64::MAX`, and a
//! string cannot tell a timestamp from any other text, so a value read from
//! a document that carries one of these is kept in one of the types here
//! instead of being rounded, overflowed or left untyped:
//!
//! - [`DecimalValue`]: a sign, a digit string and a power-of-ten exponent,
//!   which keeps both the value and the written scale (`1.50` is not `1.5`);
//! - [`BigIntValue`]: an integer outside the range of `i64` (an integer
//!   inside it is always an `i64`, see [`ExactInteger`]);
//! - [`Temporal`]: a civil date, a time of day, or both, with the
//!   [`Offset`] the lexeme wrote (or none).
//!
//! Every parser here returns an [`ExactParseError`] naming the kind it was
//! reading and a [`FidelityReason`].
//!
//! Each type also has one canonical byte encoding
//! ([`DecimalValue::canonical_bytes`] and its siblings), written in the
//! workspace's canonical conventions (integers little-endian, every string
//! behind a `u64` length, every sum behind a one-byte discriminant); the
//! canonical writers of expression literals and instance values append it
//! after their own tag.

mod decimal;
mod integer;
mod temporal;

use serde::{Deserialize, Serialize};

pub use decimal::DecimalValue;
pub use integer::{BigIntValue, ExactInteger};
pub use temporal::{CivilDate, CivilTime, Offset, Temporal};

/// The exact kind a parser was reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExactKind {
    /// A [`DecimalValue`].
    Decimal,
    /// An integer ([`ExactInteger`]).
    BigInt,
    /// A [`Temporal`].
    Temporal,
}

impl ExactKind {
    /// The kind's name, as error messages write it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Decimal => "decimal",
            Self::BigInt => "bigint",
            Self::Temporal => "temporal",
        }
    }
}

/// Why a lexeme could not be held exactly by the kind it was read as.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FidelityReason {
    /// The value is outside the range the kind holds.
    Overflow,
    /// The value has more significant digits than the kind holds.
    PrecisionExceeded {
        /// The number of significant digits the kind holds.
        max: u32,
    },
    /// The value has more digits after the decimal point than the kind
    /// holds.
    ScaleExceeded {
        /// The number of digits after the decimal point the kind holds.
        max: u32,
    },
    /// The lexeme is not a number.
    NotANumber,
    /// The lexeme is a NaN or an infinity, which the kind cannot hold.
    NonFinite,
    /// The lexeme is not a valid date, time or date-time.
    InvalidTemporal,
    /// The kind requires a UTC offset and the lexeme has none.
    OffsetRequired,
    /// The kind forbids a UTC offset and the lexeme has one.
    OffsetForbidden,
    /// The lexeme is not in the text encoding the kind declares (base64,
    /// hexadecimal, a boolean literal).
    InvalidEncoding,
    /// The lexeme is a number, and the kind holds no numbers.
    NotANumberForKind,
}

impl std::fmt::Display for FidelityReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overflow => f.write_str("out of range"),
            Self::PrecisionExceeded { max } => write!(f, "more than {max} significant digits"),
            Self::ScaleExceeded { max } => write!(f, "more than {max} digits after the point"),
            Self::NotANumber => f.write_str("not a number"),
            Self::NonFinite => f.write_str("not finite"),
            Self::InvalidTemporal => f.write_str("not a valid date or time"),
            Self::OffsetRequired => f.write_str("a UTC offset is required"),
            Self::OffsetForbidden => f.write_str("a UTC offset is not allowed"),
            Self::InvalidEncoding => f.write_str("not in the declared text encoding"),
            Self::NotANumberForKind => f.write_str("a number where the kind holds none"),
        }
    }
}

/// A lexeme an exact parser could not read.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, thiserror::Error)]
#[error("cannot read a {} exactly: {reason}", kind.as_str())]
pub struct ExactParseError {
    /// The kind the parser was reading.
    pub kind: ExactKind,
    /// Why the lexeme could not be read.
    pub reason: FidelityReason,
}

impl ExactParseError {
    /// An error reading `kind` for `reason`.
    #[must_use]
    pub const fn new(kind: ExactKind, reason: FidelityReason) -> Self {
        Self { kind, reason }
    }
}

/// Append `n` as eight little-endian bytes (the canonical length prefix).
fn put_len(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&u64::try_from(n).unwrap_or(u64::MAX).to_le_bytes());
}

/// Append `s` behind its canonical length prefix.
fn put_str(out: &mut Vec<u8>, s: &str) {
    put_len(out, s.len());
    out.extend_from_slice(s.as_bytes());
}

/// Whether `s` is a non-empty ASCII digit string without a leading zero,
/// or exactly `"0"`.
fn is_canonical_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && (s == "0" || !s.starts_with('0'))
}

/// `s` without its leading zeros, or `"0"` when nothing else is left.
fn strip_leading_zeros(s: &str) -> &str {
    let trimmed = s.trim_start_matches('0');
    if trimmed.is_empty() { "0" } else { trimmed }
}

/// Compare two canonical digit strings (see [`is_canonical_digits`]) as
/// the non-negative integers they write.
fn cmp_digit_strings(a: &str, b: &str) -> std::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}
