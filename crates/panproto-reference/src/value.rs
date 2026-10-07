//! The instance value model: every variant a cell or a node value can hold,
//! with floats held by their bit patterns so that equality is bit-wise.

use std::collections::BTreeMap;

use crate::exact::{BigIntValue, Decimal, ExactInteger};
use crate::temporal::Temporal;

/// An instance value. A float is its binary64 bit pattern, so two floats are
/// equal iff their patterns are (a NaN equals itself and differs from a NaN
/// of another sign or payload; `0.0` differs from `-0.0`), which is
/// canonical-byte equality.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Value {
    /// A boolean.
    Bool(bool),
    /// An integer within `i64`.
    Int(i64),
    /// A binary64 float, by its bits.
    Float(u64),
    /// A string.
    Str(String),
    /// Bytes.
    Bytes(Vec<u8>),
    /// A content-identifier link.
    CidLink(String),
    /// A blob reference.
    Blob {
        /// Reference.
        ref_: String,
        /// MIME type.
        mime: String,
        /// Size.
        size: u64,
    },
    /// A token.
    Token(String),
    /// An explicit null.
    Null,
    /// An opaque typed value.
    Opaque {
        /// Type identifier.
        type_: String,
        /// Fields.
        fields: BTreeMap<String, Self>,
    },
    /// A record of named values.
    Unknown(BTreeMap<String, Self>),
    /// A list.
    List(Vec<Self>),
    /// A labelled null.
    LabeledNull(u64),
    /// An exact decimal.
    Decimal(Decimal),
    /// An integer outside `i64`.
    BigInt(BigIntValue),
    /// A date, time or date-time.
    Temporal(Temporal),
}

impl Value {
    /// The value of an exact integer: `Int` when it fits, else `BigInt`.
    #[must_use]
    pub fn integer(n: ExactInteger) -> Self {
        match n {
            ExactInteger::Int(v) => Self::Int(v),
            ExactInteger::Big(b) => Self::BigInt(b),
        }
    }

    /// Whether a labelled null occurs anywhere inside this value (the value
    /// itself excluded).
    #[must_use]
    pub fn has_nested_labeled_null(&self) -> bool {
        let inner = |v: &Self| matches!(v, Self::LabeledNull(_)) || v.has_nested_labeled_null();
        match self {
            Self::Opaque { fields, .. } | Self::Unknown(fields) => fields.values().any(inner),
            Self::List(items) => items.iter().any(inner),
            _ => false,
        }
    }
}

/// Whether a field is present with a value, explicitly null, or absent.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Presence {
    /// Present with a value.
    Present(Value),
    /// Explicitly null.
    Null,
    /// Absent.
    Absent,
}
