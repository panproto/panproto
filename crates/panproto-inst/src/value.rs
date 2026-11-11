//! Value types and field presence for W-type instances.
//!
//! [`Value`] represents the leaf data in an instance tree, while
//! [`FieldPresence`] distinguishes between present, null, and absent
//! fields in the W-type model. The exact scalar types a [`Value`] can hold
//! (decimals, integers outside `i64`, dates and times) are defined in
//! [`panproto_expr::exact`] and re-exported here, and [`coerce`](crate::value::coerce::coerce) reads a
//! lexeme into a [`Value`] under the scalar kind a schema declares for it,
//! reporting a [`ValueFidelityError`] whenever the kind cannot hold the
//! lexeme exactly.

pub mod coerce;

use panproto_gat::Name;
use panproto_schema::ScalarKind;
use serde::{Deserialize, Serialize};

pub use panproto_expr::exact::{
    BigIntValue, CANONICAL_NAN, CivilDate, CivilTime, DateFields, DateTimeSeparator, DecimalClass,
    DecimalValue, ExactInteger, ExtendedTemporal, Fraction, NEGATIVE_CANONICAL_NAN, Offset,
    OffsetForm, Temporal, TemporalForm, TemporalInfinity, TemporalLexicalForm, TemporalParseError,
    TemporalYear, TimeFields, WideInt, ZuluCase, binary32_bits,
};
pub use panproto_expr::{ExactKind, ExactParseError, FidelityReason, Value};

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

/// Where in a document a value was read.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ValueSite {
    /// The schema vertex the value belongs to.
    pub vertex: Name,
    /// The byte offset of the lexeme in the source, when the codec knows
    /// it.
    pub byte_offset: Option<u64>,
}

impl ValueSite {
    /// The site of a value at `vertex`, at `byte_offset` when known.
    #[must_use]
    pub fn new(vertex: impl Into<Name>, byte_offset: Option<u64>) -> Self {
        Self {
            vertex: vertex.into(),
            byte_offset,
        }
    }

    /// A site not yet known: an empty vertex name and no offset. A codec
    /// replaces it with [`ValueFidelityError::at`].
    #[must_use]
    pub fn unknown() -> Self {
        Self::new("", None)
    }
}

impl std::fmt::Display for ValueSite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "vertex {:?}", self.vertex.as_ref())?;
        if let Some(offset) = self.byte_offset {
            write!(f, " (byte {offset})")?;
        }
        Ok(())
    }
}

/// A lexeme the declared scalar kind of its vertex cannot hold exactly.
///
/// Built by [`coerce`](coerce::coerce), or from an [`ExactParseError`] by
/// `From`, which leaves the site unknown, the lexeme empty and the target
/// the kind the parser was reading; the codec then fills them in with
/// [`at`](Self::at), [`with_lexeme`](Self::with_lexeme) and
/// [`with_target`](Self::with_target).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, thiserror::Error)]
#[error("{lexeme:?} at {site} cannot be held exactly as {target:?}: {reason}")]
pub struct ValueFidelityError {
    /// Where the lexeme was read.
    pub site: ValueSite,
    /// The lexeme as the source wrote it.
    pub lexeme: String,
    /// The scalar kind it was read as.
    pub target: ScalarKind,
    /// Why the kind cannot hold it.
    pub reason: FidelityReason,
}

impl ValueFidelityError {
    /// The error with its site set to `site`.
    #[must_use]
    pub fn at(mut self, site: ValueSite) -> Self {
        self.site = site;
        self
    }

    /// The error with its lexeme set to `lexeme`.
    #[must_use]
    pub fn with_lexeme(mut self, lexeme: impl Into<String>) -> Self {
        self.lexeme = lexeme.into();
        self
    }

    /// The error with its target kind set to `target`.
    #[must_use]
    pub const fn with_target(mut self, target: ScalarKind) -> Self {
        self.target = target;
        self
    }
}

impl From<ExactParseError> for ValueFidelityError {
    fn from(error: ExactParseError) -> Self {
        let target = match error.kind {
            ExactKind::Decimal => ScalarKind::Decimal,
            ExactKind::BigInt => ScalarKind::BigInt,
            ExactKind::Temporal => ScalarKind::DateTime,
        };
        Self {
            site: ValueSite::unknown(),
            lexeme: String::new(),
            target,
            reason: error.reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

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
