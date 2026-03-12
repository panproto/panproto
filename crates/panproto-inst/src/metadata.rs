//! Metadata types for W-type instance nodes.
//!
//! Nodes carry optional metadata: discriminators (for union types),
//! extra fields (for round-trip preservation), and opaque values.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::value::{FieldPresence, Value};

/// A node in a W-type instance tree.
///
/// Each node is anchored to a schema vertex and carries optional
/// value data, a discriminator (for union vertices), and extra
/// fields for round-trip fidelity.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    /// Unique numeric identifier within the instance.
    pub id: u32,
    /// The schema vertex this node is anchored to.
    pub anchor: String,
    /// The node's value, if it is a leaf.
    pub value: Option<FieldPresence>,
    /// Discriminator for union-typed vertices (e.g., `"$type"` value).
    pub discriminator: Option<String>,
    /// Extra fields preserved for round-trip fidelity.
    pub extra_fields: HashMap<String, Value>,
}

impl Node {
    /// Create a new node with the given id and anchor vertex.
    #[must_use]
    pub fn new(id: u32, anchor: impl Into<String>) -> Self {
        Self {
            id,
            anchor: anchor.into(),
            value: None,
            discriminator: None,
            extra_fields: HashMap::new(),
        }
    }

    /// Set the node's value.
    #[must_use]
    pub fn with_value(mut self, value: FieldPresence) -> Self {
        self.value = Some(value);
        self
    }

    /// Set the node's discriminator.
    #[must_use]
    pub fn with_discriminator(mut self, disc: impl Into<String>) -> Self {
        self.discriminator = Some(disc.into());
        self
    }

    /// Add an extra field for round-trip preservation.
    #[must_use]
    pub fn with_extra_field(mut self, key: impl Into<String>, value: Value) -> Self {
        self.extra_fields.insert(key.into(), value);
        self
    }

    /// Returns `true` if this node has a present value.
    #[must_use]
    pub fn has_value(&self) -> bool {
        self.value.as_ref().is_some_and(FieldPresence::is_present)
    }

    /// Returns `true` if this node is a leaf (has a value or is null).
    #[must_use]
    pub const fn is_leaf(&self) -> bool {
        self.value.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_builder() {
        let node = Node::new(0, "post:body.text")
            .with_value(FieldPresence::Present(Value::Str("hello".into())))
            .with_discriminator("string")
            .with_extra_field("$lang", Value::Str("en".into()));

        assert_eq!(node.id, 0);
        assert_eq!(node.anchor, "post:body.text");
        assert!(node.has_value());
        assert!(node.is_leaf());
        assert_eq!(node.discriminator.as_deref(), Some("string"));
        assert_eq!(
            node.extra_fields.get("$lang"),
            Some(&Value::Str("en".into()))
        );
    }

    #[test]
    fn node_without_value() {
        let node = Node::new(1, "post:body");
        assert!(!node.has_value());
        assert!(!node.is_leaf());
    }
}
