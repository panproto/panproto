//! Exact data values shared by schemas and instances.
//!
//! This carrier lives below both `panproto-schema` and `panproto-inst` so a
//! schema declaration may contain a value without introducing a dependency
//! cycle. `panproto-inst` re-exports the same type at its established path.

use std::cmp::Ordering;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::ExactParseError;
use crate::exact::{BigIntValue, DecimalValue, ExactInteger, Temporal};

mod serde_forms;

/// A concrete data value.
///
/// Variants serialize externally tagged in declaration order. New variants
/// are appended so the existing variant indices remain fixed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// Boolean value.
    Bool(bool),
    /// 64-bit signed integer.
    Int(i64),
    /// 64-bit floating-point number.
    Float(#[serde(with = "serde_forms::float")] f64),
    /// UTF-8 string.
    Str(String),
    /// Raw bytes.
    Bytes(#[serde(with = "serde_forms::bytes")] Vec<u8>),
    /// A content-identifier link.
    CidLink(String),
    /// A blob reference.
    Blob {
        /// Reference identifier.
        ref_: String,
        /// MIME type.
        mime: String,
        /// Size in bytes.
        size: u64,
    },
    /// A token or enum variant name.
    Token(String),
    /// Explicit null.
    Null,
    /// An opaque typed value.
    Opaque {
        /// Type identifier.
        type_: String,
        /// Opaque fields.
        #[serde(with = "serde_forms::sorted_map")]
        fields: HashMap<String, Self>,
    },
    /// A schema-unanchored record value.
    Unknown(#[serde(with = "serde_forms::sorted_map")] HashMap<String, Self>),
    /// An ordered list of values.
    List(Vec<Self>),
    /// An existential placeholder with a distinct identity.
    LabeledNull(u64),
    /// An exact decimal, including its written scale.
    Decimal(DecimalValue),
    /// An integer outside the range of `i64`.
    BigInt(BigIntValue),
    /// A date, time, or date-time with its written form.
    Temporal(Temporal),
}

impl Value {
    /// A stable human-readable variant name.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Str(_) => "str",
            Self::Bytes(_) => "bytes",
            Self::CidLink(_) => "cid-link",
            Self::Blob { .. } => "blob",
            Self::Token(_) => "token",
            Self::Null => "null",
            Self::Opaque { .. } => "opaque",
            Self::Unknown(_) => "unknown",
            Self::List(_) => "list",
            Self::LabeledNull(_) => "labeled-null",
            Self::Decimal(_) => "decimal",
            Self::BigInt(_) => "bigint",
            Self::Temporal(_) => "temporal",
        }
    }

    /// Build an exact integer in its one representation.
    #[must_use]
    pub fn integer(value: ExactInteger) -> Self {
        match value {
            ExactInteger::Int(value) => Self::Int(value),
            ExactInteger::Big(value) => Self::BigInt(value),
        }
    }

    /// Read an optional sign and ASCII digits into the one exact integer
    /// representation.
    ///
    /// # Errors
    ///
    /// Returns [`crate::FidelityReason::NotANumber`] when `text` is not an
    /// integer lexeme.
    pub fn parse_integer(text: &str) -> Result<Self, ExactParseError> {
        BigIntValue::parse(text).map(Self::integer)
    }

    /// The canonical bytes of this value.
    ///
    /// The stable tags are `Bool=0`, `Int=1`, `Float=2`, `Str=3`,
    /// `Bytes=4`, `CidLink=5`, `Blob=6`, `Token=7`, `Null=8`, `Opaque=9`,
    /// `Unknown=10`, `List=11`, `LabeledNull=12`, `Decimal=13`,
    /// `BigInt=14`, and `Temporal=15`.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Writer::default();
        out.value(self);
        out.0
    }

    /// Total order by [`Self::canonical_bytes`].
    #[must_use]
    pub fn canonical_cmp(&self, other: &Self) -> Ordering {
        if std::ptr::eq(self, other) {
            return Ordering::Equal;
        }
        self.canonical_bytes().cmp(&other.canonical_bytes())
    }

    /// The identity carried by a labeled null.
    #[must_use]
    pub const fn as_labeled_null(&self) -> Option<u64> {
        match self {
            Self::LabeledNull(id) => Some(*id),
            _ => None,
        }
    }

    /// Whether this is a labeled null.
    #[must_use]
    pub const fn is_labeled_null(&self) -> bool {
        matches!(self, Self::LabeledNull(_))
    }
}

impl From<ExactInteger> for Value {
    fn from(value: ExactInteger) -> Self {
        Self::integer(value)
    }
}

#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn tag(&mut self, tag: u8) {
        self.0.push(tag);
    }

    fn len(&mut self, len: usize) {
        self.u64(u64::try_from(len).unwrap_or(u64::MAX));
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.len(bytes.len());
        self.0.extend_from_slice(bytes);
    }

    fn str(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn value_map(&mut self, map: &HashMap<String, Value>) {
        let mut entries: Vec<_> = map.iter().collect();
        entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
        self.len(entries.len());
        for (key, value) in entries {
            self.str(key);
            self.value(value);
        }
    }

    fn value(&mut self, value: &Value) {
        match value {
            Value::Bool(value) => {
                self.tag(0);
                self.tag(u8::from(*value));
            }
            Value::Int(value) => {
                self.tag(1);
                self.0.extend_from_slice(&value.to_le_bytes());
            }
            Value::Float(value) => {
                self.tag(2);
                self.0.extend_from_slice(&value.to_bits().to_le_bytes());
            }
            Value::Str(value) => {
                self.tag(3);
                self.str(value);
            }
            Value::Bytes(value) => {
                self.tag(4);
                self.bytes(value);
            }
            Value::CidLink(value) => {
                self.tag(5);
                self.str(value);
            }
            Value::Blob { ref_, mime, size } => {
                self.tag(6);
                self.str(ref_);
                self.str(mime);
                self.u64(*size);
            }
            Value::Token(value) => {
                self.tag(7);
                self.str(value);
            }
            Value::Null => self.tag(8),
            Value::Opaque { type_, fields } => {
                self.tag(9);
                self.str(type_);
                self.value_map(fields);
            }
            Value::Unknown(fields) => {
                self.tag(10);
                self.value_map(fields);
            }
            Value::List(items) => {
                self.tag(11);
                self.len(items.len());
                for item in items {
                    self.value(item);
                }
            }
            Value::LabeledNull(id) => {
                self.tag(12);
                self.u64(*id);
            }
            Value::Decimal(value) => {
                self.tag(13);
                self.0.extend_from_slice(&value.canonical_bytes());
            }
            Value::BigInt(value) => {
                self.tag(14);
                self.0.extend_from_slice(&value.canonical_bytes());
            }
            Value::Temporal(value) => {
                self.tag(15);
                self.0.extend_from_slice(&value.canonical_bytes());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Value;

    #[test]
    fn canonical_tags_and_payloads_are_pinned() {
        assert_eq!(Value::Bool(true).canonical_bytes(), [0, 1]);
        assert_eq!(
            Value::Int(-2).canonical_bytes(),
            [1, 254, 255, 255, 255, 255, 255, 255, 255]
        );
        assert_eq!(
            Value::Str("x".into()).canonical_bytes(),
            [3, 1, 0, 0, 0, 0, 0, 0, 0, b'x']
        );
        assert_eq!(
            Value::LabeledNull(7).canonical_bytes(),
            [12, 7, 0, 0, 0, 0, 0, 0, 0]
        );
    }
}
