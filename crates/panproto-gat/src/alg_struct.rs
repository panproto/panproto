//! Algebraic struct types for theories.
//!
//! An `AlgStruct` declares a record type within a theory, with named
//! fields typed by the theory's sorts. This enables schemas to be
//! modeled as algebraic objects within the theory framework.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// An algebraic struct type declared within a theory.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AlgStruct {
    /// The struct type name.
    pub name: Arc<str>,
    /// Type parameters.
    pub params: Vec<StructParam>,
    /// Named fields.
    pub fields: Vec<StructField>,
}

/// A type parameter of an algebraic struct.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StructParam {
    /// The parameter name.
    pub name: Arc<str>,
    /// The sort this parameter ranges over.
    pub sort: Arc<str>,
}

/// A field of an algebraic struct.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StructField {
    /// The field name.
    pub name: Arc<str>,
    /// The sort of values in this field.
    pub sort: Arc<str>,
    /// Whether this field is optional.
    pub optional: bool,
}

impl AlgStruct {
    /// Create a new algebraic struct with the given name and no parameters or fields.
    #[must_use]
    pub fn new(name: impl Into<Arc<str>>) -> Self {
        Self {
            name: name.into(),
            params: Vec::new(),
            fields: Vec::new(),
        }
    }

    /// Add a type parameter.
    #[must_use]
    pub fn with_param(mut self, name: impl Into<Arc<str>>, sort: impl Into<Arc<str>>) -> Self {
        self.params.push(StructParam {
            name: name.into(),
            sort: sort.into(),
        });
        self
    }

    /// Add a required field.
    #[must_use]
    pub fn with_field(mut self, name: impl Into<Arc<str>>, sort: impl Into<Arc<str>>) -> Self {
        self.fields.push(StructField {
            name: name.into(),
            sort: sort.into(),
            optional: false,
        });
        self
    }

    /// Add an optional field.
    #[must_use]
    pub fn with_optional_field(
        mut self,
        name: impl Into<Arc<str>>,
        sort: impl Into<Arc<str>>,
    ) -> Self {
        self.fields.push(StructField {
            name: name.into(),
            sort: sort.into(),
            optional: true,
        });
        self
    }

    /// Return the number of required (non-optional) fields.
    #[must_use]
    pub fn required_field_count(&self) -> usize {
        self.fields.iter().filter(|f| !f.optional).count()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn build_struct_with_builder() {
        let s = AlgStruct::new("Person")
            .with_param("T", "Type")
            .with_field("name", "string")
            .with_field("age", "int")
            .with_optional_field("email", "string");

        assert_eq!(&*s.name, "Person");
        assert_eq!(s.params.len(), 1);
        assert_eq!(s.fields.len(), 3);
        assert_eq!(s.required_field_count(), 2);
    }

    #[test]
    fn empty_struct() {
        let s = AlgStruct::new("Unit");
        assert!(s.params.is_empty());
        assert!(s.fields.is_empty());
        assert_eq!(s.required_field_count(), 0);
    }

    #[test]
    fn serialization_round_trip() {
        let s = AlgStruct::new("Pair")
            .with_param("A", "Sort")
            .with_param("B", "Sort")
            .with_field("fst", "A")
            .with_field("snd", "B");

        let json = serde_json::to_string(&s).expect("serialize");
        let deserialized: AlgStruct = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(s, deserialized);
    }
}
