//! Value types and field presence for W-type instances.
//!
//! [`Value`] represents the leaf data in an instance tree, while
//! [`FieldPresence`] distinguishes between present, null, and absent
//! fields in the W-type model.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Field presence in a W-type instance node.
///
/// Distinguishes between a field that is present with a value,
/// explicitly null, or absent (not provided).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FieldPresence {
    /// The field is present with the given value.
    Present(Value),
    /// The field is explicitly null.
    Null,
    /// The field is absent (not provided).
    Absent,
}

impl FieldPresence {
    /// Returns `true` if the field is present (not null or absent).
    #[must_use]
    pub const fn is_present(&self) -> bool {
        matches!(self, Self::Present(_))
    }

    /// Returns `true` if the field is absent.
    #[must_use]
    pub const fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }

    /// Returns `true` if the field is null.
    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Returns the inner value if present.
    #[must_use]
    pub const fn as_value(&self) -> Option<&Value> {
        match self {
            Self::Present(v) => Some(v),
            Self::Null | Self::Absent => None,
        }
    }
}

/// A concrete data value in an instance.
///
/// Covers the common leaf types across protocols: booleans, integers,
/// strings, bytes, CID links (for content-addressed protocols), blobs,
/// tokens (enum variants), null, and extensibility via opaque/unknown.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// Boolean value.
    Bool(bool),
    /// 64-bit signed integer.
    Int(i64),
    /// 64-bit floating-point number.
    Float(f64),
    /// UTF-8 string.
    Str(String),
    /// Raw bytes.
    Bytes(Vec<u8>),
    /// A content-identifier link (CID).
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
    /// A token (enum variant name).
    Token(String),
    /// Explicit null.
    Null,
    /// An opaque typed value (protocol-specific extension).
    Opaque {
        /// The type identifier.
        type_: String,
        /// Opaque fields.
        fields: HashMap<String, Self>,
    },
    /// An unknown value (unrecognized fields preserved for round-tripping).
    Unknown(HashMap<String, Self>),
}

impl Value {
    /// Returns a human-readable type name for this value.
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_presence_predicates() {
        let present = FieldPresence::Present(Value::Int(42));
        assert!(present.is_present());
        assert!(!present.is_null());
        assert!(!present.is_absent());

        let null = FieldPresence::Null;
        assert!(null.is_null());

        let absent = FieldPresence::Absent;
        assert!(absent.is_absent());
    }

    #[test]
    fn value_type_names() {
        assert_eq!(Value::Bool(true).type_name(), "bool");
        assert_eq!(Value::Str("hello".into()).type_name(), "str");
        assert_eq!(Value::Null.type_name(), "null");
    }

    #[test]
    fn field_presence_as_value() {
        let present = FieldPresence::Present(Value::Int(42));
        assert_eq!(present.as_value(), Some(&Value::Int(42)));

        let null = FieldPresence::Null;
        assert_eq!(null.as_value(), None);
    }
}
