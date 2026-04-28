//! Metadata types for W-type instance nodes.
//!
//! Nodes carry optional metadata: discriminators (for union types),
//! extra fields (for round-trip preservation), and opaque values.

use std::collections::HashMap;

use panproto_gat::Name;
use serde::{Deserialize, Serialize};

use crate::value::{FieldPresence, Value};

/// Sum type discriminating the structural shape of a node beyond its
/// schema anchor.
///
/// The schema anchor identifies which vertex of the protocol schema
/// the node sits over; this sum captures categorical information
/// orthogonal to that, namely whether the node is the source of a
/// free-monoid (list) structure, a renamed element from XML aliasing,
/// or an inline text segment in mixed-content XML. Encoding these as
/// typed variants rather than reserved-string entries on
/// [`Node::annotations`] gives the type system jurisdiction over
/// "marker only on the right node shape" and removes the collision
/// risk with user-supplied annotation keys.
///
/// `Default` is `Plain`: a regular schema-anchored node with no
/// extra structural shape.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodeShape {
    /// Regular schema-anchored node. The default.
    #[default]
    Plain,
    /// Node is the source of an ordered collection (free-monoid /
    /// list functor). Used by `to_json` and friends to emit a JSON
    /// array even when zero or one child arcs are present.
    List,
    /// Node was produced by an XML ALIAS that renamed the parser's
    /// internal kind to the carried tag. Carries the original XML
    /// element name so emitters can write back `<NAF>...</NAF>`
    /// rather than the schema anchor.
    XmlElement {
        /// The XML tag name as it appeared in the source document.
        tag: Name,
    },
    /// Inline text run inside a mixed-content XML element. Emitters
    /// write `node.value` as bare text without surrounding start /
    /// end tags so `<p>x<em>y</em>z</p>` round-trips with text and
    /// element children interleaved in source order.
    XmlTextSegment,
}

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
    pub anchor: Name,
    /// The node's value, if it is a leaf.
    pub value: Option<FieldPresence>,
    /// Discriminator for union-typed vertices (e.g., `"$type"` value).
    pub discriminator: Option<Name>,
    /// Extra fields preserved for round-trip fidelity.
    pub extra_fields: HashMap<String, Value>,
    /// Position in an ordered collection (if any).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<u32>,
    /// Structural shape of the node, orthogonal to the schema anchor.
    ///
    /// Defaults to [`NodeShape::Plain`]. Set to [`NodeShape::List`],
    /// [`NodeShape::XmlElement`], or [`NodeShape::XmlTextSegment`] by
    /// the CST extractors when they recover a list / aliased
    /// element / inline text run from a parsed document; consumed by
    /// the corresponding emitters to drive serialisation choices.
    #[serde(default, skip_serializing_if = "node_shape_is_default")]
    pub shape: NodeShape,
    /// Out-of-band annotations (metadata distinct from data).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub annotations: HashMap<String, Value>,
}

const fn node_shape_is_default(shape: &NodeShape) -> bool {
    matches!(shape, NodeShape::Plain)
}

impl Node {
    /// Create a new node with the given id and anchor vertex.
    #[must_use]
    pub fn new(id: u32, anchor: impl Into<Name>) -> Self {
        Self {
            id,
            anchor: anchor.into(),
            value: None,
            discriminator: None,
            extra_fields: HashMap::new(),
            position: None,
            shape: NodeShape::Plain,
            annotations: HashMap::new(),
        }
    }

    /// Set the node's structural shape.
    #[must_use]
    pub fn with_shape(mut self, shape: NodeShape) -> Self {
        self.shape = shape;
        self
    }

    /// Returns `true` iff the node represents an ordered collection
    /// (the source of a free-monoid structure). Equivalent to
    /// `matches!(self.shape, NodeShape::List)`.
    #[must_use]
    pub const fn is_list(&self) -> bool {
        matches!(self.shape, NodeShape::List)
    }

    /// Returns `true` iff the node is an inline XML text segment.
    #[must_use]
    pub const fn is_xml_text_segment(&self) -> bool {
        matches!(self.shape, NodeShape::XmlTextSegment)
    }

    /// Returns the original XML tag name when the node was produced
    /// by an aliased XML element; `None` otherwise.
    #[must_use]
    pub const fn xml_tag(&self) -> Option<&Name> {
        match &self.shape {
            NodeShape::XmlElement { tag } => Some(tag),
            _ => None,
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
    pub fn with_discriminator(mut self, disc: impl Into<Name>) -> Self {
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
#[allow(clippy::expect_used, clippy::unwrap_used)]
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

    #[test]
    fn default_shape_is_plain() {
        let node = Node::new(0, "v");
        assert!(matches!(node.shape, NodeShape::Plain));
        assert!(!node.is_list());
        assert!(!node.is_xml_text_segment());
        assert_eq!(node.xml_tag(), None);
    }

    #[test]
    fn with_shape_list() {
        let node = Node::new(0, "v").with_shape(NodeShape::List);
        assert!(node.is_list());
        assert!(!node.is_xml_text_segment());
        assert_eq!(node.xml_tag(), None);
    }

    #[test]
    fn with_shape_xml_element_carries_tag() {
        let node = Node::new(0, "v").with_shape(NodeShape::XmlElement {
            tag: Name::from("para"),
        });
        assert!(!node.is_list());
        assert!(!node.is_xml_text_segment());
        assert_eq!(node.xml_tag().map(Name::as_ref), Some("para"));
    }

    #[test]
    fn with_shape_xml_text_segment() {
        let node = Node::new(0, "v").with_shape(NodeShape::XmlTextSegment);
        assert!(!node.is_list());
        assert!(node.is_xml_text_segment());
        assert_eq!(node.xml_tag(), None);
    }

    #[test]
    fn shape_serialization_skips_default() {
        let node = Node::new(0, "v");
        let json = serde_json::to_string(&node).expect("serialize plain node");
        assert!(
            !json.contains("shape"),
            "Plain shape must skip-serialize: {json}"
        );
    }

    #[test]
    fn shape_serialization_emits_non_default() {
        let node = Node::new(0, "v").with_shape(NodeShape::List);
        let json = serde_json::to_string(&node).expect("serialize list node");
        assert!(json.contains("\"shape\""), "non-Plain shape must serialize");
        // serde serializes with `rename_all = "snake_case"` and `tag = "kind"`,
        // so `NodeShape::List` becomes `{"kind":"list"}`.
        assert!(json.contains("\"list\""), "expected list tag in: {json}");
    }
}
