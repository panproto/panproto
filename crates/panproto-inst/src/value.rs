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
/// This is the ADT of *leaf-or-opaque* data carried by a W-type node.
/// It mirrors the free term algebra of JSON-like values and forms a
/// faithful round-trip target for any schema-unanchored data that
/// parses into the instance (e.g. values landing in `extra_fields`).
///
/// Category-theoretically, the variants partition into:
///
/// - **Primitive atoms** ([`Self::Bool`], [`Self::Int`], [`Self::Float`],
///   [`Self::Str`], [`Self::Bytes`], [`Self::CidLink`], [`Self::Blob`],
///   [`Self::Token`], [`Self::Null`]) — generators of the ADT.
/// - **Records** ([`Self::Opaque`], [`Self::Unknown`]) — finite products
///   indexed by string field names (heterogeneous, unordered).
/// - **Lists** ([`Self::List`]) — the free monoid / list object, an
///   ordered collection with anonymous positions. This is the list
///   constructor needed to faithfully embed JSON arrays and, more
///   generally, any ordered-collection leaf value.
///
/// The `Unknown` and `List` variants together give the enum closure
/// under the two fundamental JSON constructors (object and array) so
/// that values with no schema anchor still round-trip losslessly.
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
        #[serde(with = "panproto_schema::serde_helpers::sorted_map")]
        fields: HashMap<String, Self>,
    },
    /// An unknown record value: a finite product of name/value pairs.
    /// Used for schema-unanchored objects that must round-trip.
    Unknown(#[serde(with = "panproto_schema::serde_helpers::sorted_map")] HashMap<String, Self>),
    /// An ordered list of values: the free-monoid list object over
    /// `Value`. Used for schema-unanchored arrays and for transforms
    /// that operate on ordered collections carried in `extra_fields`.
    List(Vec<Self>),
    /// A labeled null: an existential placeholder carrying a distinct
    /// identity.
    ///
    /// Labeled nulls are introduced by the term-level chase in
    /// `panproto-mig` when a tuple-generating dependency fires: the
    /// existentially-quantified positions of its head are filled with
    /// fresh labeled nulls. Two labeled nulls with the same identity are
    /// the same unknown value; equality-generating dependencies may merge
    /// a labeled null with another value (a constant or another null).
    /// Distinct from [`Self::Null`], which is a concrete (SQL-style) null.
    LabeledNull(u64),
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
            Self::List(_) => "list",
            Self::LabeledNull(_) => "labeled-null",
        }
    }

    /// Returns the identity of this value if it is a labeled null.
    #[must_use]
    pub const fn as_labeled_null(&self) -> Option<u64> {
        match self {
            Self::LabeledNull(id) => Some(*id),
            _ => None,
        }
    }

    /// Returns `true` if this value is a labeled null.
    #[must_use]
    pub const fn is_labeled_null(&self) -> bool {
        matches!(self, Self::LabeledNull(_))
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
        assert_eq!(Value::List(Vec::new()).type_name(), "list");
        assert_eq!(Value::Unknown(HashMap::new()).type_name(), "unknown");
    }

    #[test]
    fn value_list_round_trip_via_serde() -> Result<(), serde_json::Error> {
        // A Value::List of mixed primitives should survive a JSON round
        // trip through its derived Serde impl.
        let original = Value::List(vec![
            Value::Int(1),
            Value::Str("two".into()),
            Value::Bool(true),
        ]);
        let json = serde_json::to_string(&original)?;
        let restored: Value = serde_json::from_str(&json)?;
        assert_eq!(original, restored);
        Ok(())
    }

    #[test]
    fn value_list_is_free_monoid_over_values() {
        // Concatenation of two Value::List instances is itself a
        // Value::List (monoid closure under +). Empty list is the
        // identity element (neutrality on both sides).
        let a = Value::List(vec![Value::Int(1), Value::Int(2)]);
        let b = Value::List(vec![Value::Int(3)]);
        let empty = Value::List(Vec::new());

        let concat = |xs: &Value, ys: &Value| match (xs, ys) {
            (Value::List(x), Value::List(y)) => {
                let mut out = x.clone();
                out.extend(y.iter().cloned());
                Value::List(out)
            }
            _ => panic!("expected lists"),
        };

        assert_eq!(
            concat(&a, &b),
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
        assert_eq!(concat(&empty, &a), a, "left identity");
        assert_eq!(concat(&a, &empty), a, "right identity");
    }

    #[test]
    fn field_presence_as_value() {
        let present = FieldPresence::Present(Value::Int(42));
        assert_eq!(present.as_value(), Some(&Value::Int(42)));

        let null = FieldPresence::Null;
        assert_eq!(null.as_value(), None);
    }
}
