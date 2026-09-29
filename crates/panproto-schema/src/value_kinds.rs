//! The scalar kinds a protocol's vertex kinds hold.
//!
//! A [`Protocol`](crate::Protocol) maps each scalar vertex kind to a
//! [`ScalarKind`] in [`Protocol::value_kinds`](crate::Protocol::value_kinds);
//! a [`ValueSpec`] combines that kind with a vertex's value parameters.
//!
//! Enums serialize externally tagged in `snake_case`; structs serialize with
//! named fields in declaration order.

use serde::{Deserialize, Serialize};

/// The kind of scalar a vertex holds.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScalarKind {
    /// A boolean.
    Bool,
    /// A fixed-width integer.
    Int {
        /// The width in bits.
        bits: u8,
        /// Whether the integer is signed.
        signed: bool,
    },
    /// An arbitrary-precision integer.
    BigInt,
    /// A binary floating-point number.
    Float {
        /// The width in bits.
        bits: u8,
    },
    /// An exact decimal number.
    Decimal,
    /// A string.
    Str,
    /// A byte string, with the text encoding it takes in textual formats.
    Bytes(BytesText),
    /// A calendar date.
    Date,
    /// A time of day.
    Time,
    /// A date and time.
    DateTime,
    /// A token from a closed vocabulary.
    Token,
    /// No declared kind.
    Any,
}

/// The text encoding of a byte string.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BytesText {
    /// Standard base64.
    Base64,
    /// URL-safe base64.
    Base64Url,
    /// Hexadecimal.
    Hex,
    /// The raw bytes.
    Raw,
}

/// A scalar kind together with the value parameters of one vertex.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ValueSpec {
    /// The scalar kind.
    pub kind: ScalarKind,
    /// The total number of significant digits, if declared.
    pub precision: Option<u32>,
    /// The number of digits after the decimal point, if declared.
    pub scale: Option<u32>,
    /// The maximum length, if declared.
    pub max_length: Option<u32>,
    /// Whether a temporal value carries a time zone, if declared.
    pub with_time_zone: Option<bool>,
    /// The integer width in bits, if declared.
    pub int_bits: Option<u8>,
    /// Whether an integer is unsigned.
    pub unsigned: bool,
}
