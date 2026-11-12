//! BSON 1.1 documents, in their binary form and in MongoDB Extended JSON.
//!
//! # The standards
//!
//! The binary form is [BSON 1.1](https://bsonspec.org/spec.html), and its
//! decimal128 values are the BID encoding of IEEE 754-2008 decimal128 as
//! the [BSON Decimal128 specification](https://github.com/mongodb/specifications/blob/master/source/bson-decimal128/decimal128.md)
//! states. The text form is [MongoDB Extended JSON version 2](https://github.com/mongodb/specifications/blob/master/source/extended-json/extended-json.md),
//! in both of its formats: *canonical*, which wraps every value whose type
//! JSON cannot tell apart in a type wrapper object (`{"$numberInt": "1"}`),
//! and *relaxed*, which writes int32, int64 and finite double values as JSON
//! numbers and a datetime between the years 1970 and 9999 as an RFC 3339
//! string. [`read_extended_json`] reads either format, as the specification
//! asks of a parser; [`write_extended_json`] writes the one its [`Mode`]
//! names. Legacy Extended JSON (`$regex`, `{"$binary": …, "$type": …}`,
//! `{"$date": <number>}`) is not a format of version 2 and is not read: a
//! `$regex` or `$type` key is an ordinary key, and a `$binary` wrapper with
//! a `$type` key is refused.
//!
//! # The model
//!
//! A [`BsonDocument`] is its elements in order, each a name and a
//! [`BsonValue`]. Two elements may share a name, as BSON allows, and both
//! are kept in their order. Every type of BSON 1.1 is read exactly,
//! the deprecated ones (undefined, `DBPointer`, symbol, code with scope)
//! included: a double keeps its bits (a NaN its payload), an int32 and an
//! int64 stay apart, a decimal128 keeps its coefficient, exponent, sign
//! and special value, a binary its subtype, a regular expression its
//! pattern and options, a timestamp its time and increment, and a datetime
//! its millisecond count. [`to_instance`] encodes a document in the BSON
//! tree encoding of [`panproto_protocols::data_schema::bson`], holding each
//! value through panproto's value machinery, and [`from_instance`] reads it
//! back, refusing every instance no document reads back as.
//!
//! # Decoding
//!
//! The binary form is a byte layout, so [`decode`] reads it by the BSON 1.1
//! grammar directly, refusing every rule it breaks with a
//! [`BsonDecodeError`] that names the byte offset at fault. It refuses the
//! *degenerate* BSON that the BSON corpus calls parseable but not
//! compliant: an array whose keys are not `"0"`, `"1"`, … in order, and
//! regular expression options out of alphabetical order. It refuses a
//! decimal128 whose bits are not the canonical encoding of a value (a
//! coefficient above `10^34 - 1`, which IEEE 754 reads as zero, or bits an
//! infinity or a NaN ignores), so that [`encode`] reproduces every byte
//! string [`decode`] accepts.

use panproto_expr::exact::DecimalValue;

mod binary;
mod decimal128;
#[cfg(feature = "tree-sitter")]
mod extjson_read;
mod extjson_write;
mod instance;

pub use binary::{decode, encode};
pub use decimal128::{Decimal128ParseError, decimal128_string, parse_decimal128};
#[cfg(feature = "tree-sitter")]
pub use extjson_read::{
    ExtJsonError, ExtJsonErrorKind, read_extended_json, read_extended_json_tree,
};
pub use extjson_write::{
    CANONICAL_NAN_BITS, Mode, WRAPPER_KEYS, double_string, write_extended_json,
};
pub use instance::{
    BsonCodec, BsonInstanceError, BsonInstanceErrorKind, from_instance, schema_holds_encoding,
    to_instance,
};

/// The deepest nesting of documents, arrays and scopes the readers and
/// writers accept, counting the top-level document as depth 1.
pub const MAX_DEPTH: usize = 128;

/// A BSON document: its elements in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BsonDocument(pub Vec<(String, BsonValue)>);

/// A BSON value, one variant per BSON 1.1 type.
#[derive(Clone, Debug, PartialEq)]
pub enum BsonValue {
    /// `0x01`: a binary64 float, NaN payloads included.
    Double(f64),
    /// `0x02`: a UTF-8 string, which may hold U+0000.
    String(String),
    /// `0x03`: an embedded document.
    Document(BsonDocument),
    /// `0x04`: an array, its values in order.
    Array(Vec<Self>),
    /// `0x05`: binary data with its subtype. For subtype 2 the bytes are
    /// the payload, without the inner length the binary form repeats.
    Binary {
        /// The subtype byte.
        subtype: u8,
        /// The bytes.
        bytes: Vec<u8>,
    },
    /// `0x06`: the deprecated undefined value.
    Undefined,
    /// `0x07`: an object id.
    ObjectId([u8; 12]),
    /// `0x08`: a boolean.
    Boolean(bool),
    /// `0x09`: a UTC datetime, as milliseconds since the Unix epoch.
    DateTime(i64),
    /// `0x0A`: null.
    Null,
    /// `0x0B`: a regular expression: its pattern and its options, which
    /// hold no U+0000, the options in alphabetical order.
    Regex {
        /// The pattern.
        pattern: String,
        /// The options.
        options: String,
    },
    /// `0x0C`: the deprecated `DBPointer`: a namespace and an object id.
    DbPointer {
        /// The namespace.
        namespace: String,
        /// The object id.
        id: [u8; 12],
    },
    /// `0x0D`: JavaScript code.
    Code(String),
    /// `0x0E`: the deprecated symbol.
    Symbol(String),
    /// `0x0F`: the deprecated JavaScript code with scope.
    CodeWithScope {
        /// The code.
        code: String,
        /// The scope.
        scope: BsonDocument,
    },
    /// `0x10`: a 32-bit integer.
    Int32(i32),
    /// `0x11`: a timestamp: seconds since the Unix epoch and an increment.
    Timestamp {
        /// The seconds, the high four bytes of the binary form.
        time: u32,
        /// The increment, the low four bytes of the binary form.
        increment: u32,
    },
    /// `0x12`: a 64-bit integer.
    Int64(i64),
    /// `0x13`: a decimal128, as an exact decimal that decimal128 holds as
    /// written: at most 34 digits and an exponent from -6176 to 6111, or an
    /// infinity, or a NaN (signalling or quiet) with a payload of at most 33
    /// digits.
    Decimal128(DecimalValue),
    /// `0xFF`: the minimum key.
    MinKey,
    /// `0x7F`: the maximum key.
    MaxKey,
}

impl BsonValue {
    /// The value's BSON type byte.
    #[must_use]
    pub const fn type_byte(&self) -> u8 {
        match self {
            Self::Double(_) => 0x01,
            Self::String(_) => 0x02,
            Self::Document(_) => 0x03,
            Self::Array(_) => 0x04,
            Self::Binary { .. } => 0x05,
            Self::Undefined => 0x06,
            Self::ObjectId(_) => 0x07,
            Self::Boolean(_) => 0x08,
            Self::DateTime(_) => 0x09,
            Self::Null => 0x0A,
            Self::Regex { .. } => 0x0B,
            Self::DbPointer { .. } => 0x0C,
            Self::Code(_) => 0x0D,
            Self::Symbol(_) => 0x0E,
            Self::CodeWithScope { .. } => 0x0F,
            Self::Int32(_) => 0x10,
            Self::Timestamp { .. } => 0x11,
            Self::Int64(_) => 0x12,
            Self::Decimal128(_) => 0x13,
            Self::MinKey => 0xFF,
            Self::MaxKey => 0x7F,
        }
    }
}

/// A refused binary BSON document: the byte offset at fault and why.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("byte {offset}: {kind}")]
pub struct BsonDecodeError {
    /// The offset of the byte at fault, as each kind states.
    pub offset: usize,
    /// Why the bytes are refused.
    pub kind: BsonDecodeErrorKind,
}

/// Why [`decode`] refuses a byte string.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum BsonDecodeErrorKind {
    /// A document's length, at the offset of its length field, is less
    /// than 5 or runs past the bytes its container leaves it.
    #[error("the document length {declared} is less than 5 or runs past its container")]
    DocumentLength {
        /// The declared length.
        declared: i64,
    },
    /// A fixed-size field (a length, a number, an object id, a subtype or
    /// a type byte), at its offset, runs past the bytes its container
    /// leaves it.
    #[error("the field runs past the end of its container")]
    Truncated,
    /// The last byte of a document, at its offset, is not `0x00`.
    #[error("the document ends in 0x{byte:02X}, not 0x00")]
    Terminator {
        /// The byte.
        byte: u8,
    },
    /// A `0x00` type byte, at its offset, before a document's last byte.
    #[error("the document's terminator comes before its declared end")]
    EarlyTerminator,
    /// A type byte, at its offset, that is no BSON 1.1 type.
    #[error("0x{byte:02X} is not a BSON type")]
    UnknownType {
        /// The byte.
        byte: u8,
    },
    /// A key or a regular expression part, at its offset, with no `0x00`
    /// before the end of its container.
    #[error("the C string has no terminating 0x00")]
    UnterminatedCString,
    /// A key, string or regular expression part that is not UTF-8, at the
    /// offset of the first byte of its first invalid sequence.
    #[error("the text is not UTF-8")]
    InvalidUtf8,
    /// A string's length, at the offset of its length field, is less than
    /// 1 or runs past its container.
    #[error("the string length {declared} is less than 1 or runs past its container")]
    StringLength {
        /// The declared length.
        declared: i64,
    },
    /// The last byte of a string, at its offset, is not `0x00`.
    #[error("the string ends in 0x{byte:02X}, not 0x00")]
    StringTerminator {
        /// The byte.
        byte: u8,
    },
    /// A boolean byte, at its offset, other than `0x00` and `0x01`.
    #[error("0x{byte:02X} is not a boolean")]
    Boolean {
        /// The byte.
        byte: u8,
    },
    /// A binary length, at the offset of its length field, that is
    /// negative or runs past its container.
    #[error("the binary length {declared} is negative or runs past its container")]
    BinaryLength {
        /// The declared length.
        declared: i64,
    },
    /// A subtype 2 binary whose inner length, at its offset, is not the
    /// outer length less 4.
    #[error("the subtype 2 binary's inner length {declared} is not its length less 4")]
    OldBinaryLength {
        /// The declared inner length.
        declared: i64,
    },
    /// A code-with-scope length, at the offset of its length field, that
    /// is less than 14, runs past its container, or is not the length of
    /// its code and scope.
    #[error("the code-with-scope length {declared} is not the length of its code and scope")]
    CodeWithScopeLength {
        /// The declared length.
        declared: i64,
    },
    /// An array key, at its offset, that is not the index of its element.
    #[error("the array key is not {expected:?}")]
    ArrayKey {
        /// The key the element's index gives.
        expected: String,
    },
    /// Regular expression options, at their offset, not in alphabetical
    /// order.
    #[error("the regular expression options are not in alphabetical order")]
    RegexOptions,
    /// A decimal128, at its offset, whose bits are not the canonical
    /// encoding of a value.
    #[error("the decimal128 bits are not a canonical encoding")]
    NonCanonicalDecimal128,
    /// Bytes after the top-level document, at the offset of the first.
    #[error("bytes follow the document")]
    TrailingBytes,
    /// A document, array or scope, at the offset of its length field,
    /// nested deeper than [`MAX_DEPTH`].
    #[error("the nesting is deeper than {MAX_DEPTH}")]
    Depth,
}

impl BsonDecodeErrorKind {
    /// The refusal's class, the name the conformance limits use.
    #[must_use]
    pub const fn class(&self) -> &'static str {
        match self {
            Self::DocumentLength { .. } => "document-length",
            Self::Truncated => "truncated",
            Self::Terminator { .. } => "terminator",
            Self::EarlyTerminator => "early-terminator",
            Self::UnknownType { .. } => "unknown-type",
            Self::UnterminatedCString => "unterminated-cstring",
            Self::InvalidUtf8 => "invalid-utf8",
            Self::StringLength { .. } => "string-length",
            Self::StringTerminator { .. } => "string-terminator",
            Self::Boolean { .. } => "boolean",
            Self::BinaryLength { .. } => "binary-length",
            Self::OldBinaryLength { .. } => "old-binary-length",
            Self::CodeWithScopeLength { .. } => "code-with-scope-length",
            Self::ArrayKey { .. } => "array-key",
            Self::RegexOptions => "regex-options",
            Self::NonCanonicalDecimal128 => "non-canonical-decimal128",
            Self::TrailingBytes => "trailing-bytes",
            Self::Depth => "depth",
        }
    }
}

/// A document [`encode`] or [`write_extended_json`] cannot write: the
/// path of the value at fault and why.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{}: {kind}", if path.is_empty() { "the document" } else { path.as_str() })]
pub struct BsonWriteError {
    /// The JSON pointer of the value at fault, from the top-level
    /// document (array items by index), empty for the document itself.
    pub path: String,
    /// Why the document cannot be written.
    pub kind: BsonWriteErrorKind,
}

/// Why a document cannot be written.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum BsonWriteErrorKind {
    /// A key holds U+0000, which a C string cannot.
    #[error("the key holds U+0000")]
    NulInKey,
    /// A regular expression's pattern or options hold U+0000.
    #[error("the regular expression holds U+0000")]
    NulInRegex,
    /// Regular expression options not in alphabetical order.
    #[error("the regular expression options are not in alphabetical order")]
    RegexOptions,
    /// A decimal that decimal128 does not hold as written.
    #[error("{0} is not a decimal128 value as written")]
    Decimal128(String),
    /// A document or string longer than an `int32` length can state.
    #[error("the value is longer than a BSON length can state")]
    TooLong,
    /// Nesting deeper than [`MAX_DEPTH`].
    #[error("the nesting is deeper than {MAX_DEPTH}")]
    Depth,
    /// Two elements of one document with the same name, which Extended
    /// JSON cannot write: the shared JSON route refuses a repeated member
    /// name.
    #[error("the name {0:?} is given to two elements")]
    DuplicateKey(String),
    /// An embedded document holding a key of an Extended JSON type
    /// wrapper, which would read back as that wrapper or be refused.
    #[error("the key {0:?} is a key of an Extended JSON type wrapper")]
    WrapperKey(String),
    /// A NaN Extended JSON does not tell apart from the one `NaN` reads
    /// as: a double NaN with other bits, or a decimal128 NaN with a sign,
    /// a signal or a payload.
    #[error("the NaN is not the one Extended JSON's NaN reads as")]
    NaN,
}

impl BsonWriteErrorKind {
    /// The refusal's class, the name the conformance limits use.
    #[must_use]
    pub const fn class(&self) -> &'static str {
        match self {
            Self::NulInKey => "write-nul-in-key",
            Self::NulInRegex => "write-nul-in-regex",
            Self::RegexOptions => "write-regex-options",
            Self::Decimal128(_) => "write-decimal128",
            Self::TooLong => "write-too-long",
            Self::Depth => "write-depth",
            Self::DuplicateKey(_) => "write-duplicate-key",
            Self::WrapperKey(_) => "write-wrapper-key",
            Self::NaN => "write-nan",
        }
    }
}

/// The refusal at `path` of `kind`.
const fn write_error(path: String, kind: BsonWriteErrorKind) -> BsonWriteError {
    BsonWriteError { path, kind }
}

/// `path` extended by one JSON pointer token.
fn child_path(path: &str, token: &str) -> String {
    let mut out = String::with_capacity(path.len() + token.len() + 1);
    out.push_str(path);
    out.push('/');
    for c in token.chars() {
        match c {
            '~' => out.push_str("~0"),
            '/' => out.push_str("~1"),
            _ => out.push(c),
        }
    }
    out
}

/// Whether regular expression options are in alphabetical order (each
/// character no earlier than the one before it).
fn options_in_order(options: &str) -> bool {
    options.as_bytes().windows(2).all(|pair| pair[0] <= pair[1])
}
