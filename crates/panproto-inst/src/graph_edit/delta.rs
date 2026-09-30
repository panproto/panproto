//! The canonical graph delta and its attribution keys and values.

use std::collections::{BTreeMap, HashMap};

use panproto_gat::Name;
use panproto_schema::Edge;
use serde::{Deserialize, Serialize};

use super::canonical::{canonical_graph_delta_bytes, same_edge, same_node, same_val};
use super::compose::ElemRef;
use crate::ginstance::{EdgeId, GEdge};
use crate::metadata::{Node, NodeShape};
use crate::value::{FieldPresence, Value};
use crate::view_ref::RowId;

/// The key of one attribution entry: a piece of a kept element's payload
/// that a delta may change without replacing the element.
///
/// The derived order sorts by variant in declaration order, then by payload;
/// the two table variants come last, so the relative order of the graph
/// variants does not depend on them. A [`GraphDelta`] holds graph keys only;
/// the table keys name the same entries of a table delta.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AttrKey {
    /// A node's value ([`Node::value`]).
    Value {
        /// The node.
        node: u32,
    },
    /// One extra field of a node ([`Node::extra_fields`]).
    Field {
        /// The node.
        node: u32,
        /// The field name.
        field: String,
    },
    /// A node's metadata ([`NodeMeta`]).
    Meta {
        /// The node.
        node: u32,
    },
    /// A node's schema anchor ([`Node::anchor`]).
    Anchor {
        /// The node.
        node: u32,
    },
    /// The schema edge an edge instantiates ([`GEdge::edge`]).
    EdgeLabel {
        /// The edge.
        edge: EdgeId,
    },
    /// An edge's position ([`GEdge::position`]).
    EdgePosition {
        /// The edge.
        edge: EdgeId,
    },
    /// One cell of a table row.
    Cell {
        /// The row.
        row: RowId,
        /// The column.
        column: String,
    },
    /// The table a row belongs to.
    Table {
        /// The row.
        row: RowId,
    },
}

impl AttrKey {
    /// The element whose payload holds this entry.
    #[must_use]
    pub const fn element(&self) -> ElemRef {
        match self {
            Self::Value { node }
            | Self::Field { node, .. }
            | Self::Meta { node }
            | Self::Anchor { node } => ElemRef::Node(*node),
            Self::EdgeLabel { edge } | Self::EdgePosition { edge } => ElemRef::Edge(*edge),
            Self::Cell { row, .. } | Self::Table { row } => ElemRef::Row(*row),
        }
    }

    /// Whether this key names an entry of a table row rather than of a
    /// graph element.
    #[must_use]
    pub const fn is_table(&self) -> bool {
        matches!(self, Self::Cell { .. } | Self::Table { .. })
    }

    /// Whether `value` is of the [`AttrVal`] variant this key takes:
    /// `Presence` for `Value`, `Field` for `Field` and `Cell`, `Meta` for
    /// `Meta`, `Anchor` for `Anchor` and `Table`, `Label` for `EdgeLabel`
    /// and `Position` for `EdgePosition`.
    #[must_use]
    pub const fn accepts(&self, value: &AttrVal) -> bool {
        matches!(
            (self, value),
            (Self::Value { .. }, AttrVal::Presence(_))
                | (Self::Field { .. } | Self::Cell { .. }, AttrVal::Field(_))
                | (Self::Meta { .. }, AttrVal::Meta(_))
                | (Self::Anchor { .. } | Self::Table { .. }, AttrVal::Anchor(_))
                | (Self::EdgeLabel { .. }, AttrVal::Label(_))
                | (Self::EdgePosition { .. }, AttrVal::Position(_))
        )
    }
}

/// The metadata of a node: every part of its payload other than its id,
/// anchor, value and extra fields.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeMeta {
    /// The discriminator ([`Node::discriminator`]).
    pub discriminator: Option<Name>,
    /// The node's position ([`Node::position`]).
    pub position: Option<u32>,
    /// The node's structural shape ([`Node::shape`]).
    pub shape: NodeShape,
    /// The node's annotations ([`Node::annotations`]), in key order.
    pub annotations: BTreeMap<String, Value>,
}

impl NodeMeta {
    /// The metadata of `node`.
    #[must_use]
    pub fn of(node: &Node) -> Self {
        Self {
            discriminator: node.discriminator.clone(),
            position: node.position,
            shape: node.shape.clone(),
            annotations: node
                .annotations
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        }
    }

    /// Write this metadata into `node`, replacing its own.
    pub fn write_to(&self, node: &mut Node) {
        node.discriminator.clone_from(&self.discriminator);
        node.position = self.position;
        node.shape = self.shape.clone();
        node.annotations = self
            .annotations
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<HashMap<_, _>>();
    }
}

/// The value of one attribution entry. The variant always matches the
/// variant of the entry's [`AttrKey`] ([`AttrKey::accepts`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AttrVal {
    /// A node's value; `None` when the node has none.
    Presence(Option<FieldPresence>),
    /// An extra field or a cell; `None` when it is absent.
    Field(Option<Value>),
    /// A node's metadata.
    Meta(NodeMeta),
    /// A node's anchor, or a row's table.
    Anchor(Name),
    /// An edge's schema edge.
    Label(Edge),
    /// An edge's position.
    Position(Option<u32>),
}

/// A change of one attribution entry from `old` to `new`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AttrChange {
    /// The entry's value before the change.
    pub old: AttrVal,
    /// The entry's value after the change.
    pub new: AttrVal,
}

/// Why a set of delta parts is not a graph delta.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum MalformedDelta {
    /// An attribution change's `old` or `new` is not of the variant its key
    /// takes.
    #[error("attribution entry {key:?} carries a value of the wrong variant")]
    ValueVariant {
        /// The key.
        key: AttrKey,
    },
    /// A graph delta names a table entry.
    #[error("attribution entry {key:?} belongs to a table, not a graph")]
    TableKey {
        /// The key.
        key: AttrKey,
    },
    /// A node is filed under an id other than its own.
    #[error("node {node} is filed under id {key}")]
    NodeIdMismatch {
        /// The id it is filed under.
        key: u32,
        /// The node's own id.
        node: u32,
    },
}

/// The parts of a [`GraphDelta`], in any form: the input of
/// [`GraphDelta::normalize`] and the output of [`GraphDelta::into_parts`].
///
/// Serialized exactly as a [`GraphDelta`] is.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GraphDeltaParts {
    /// Deleted nodes, with the payload they have before the delta.
    pub del_nodes: BTreeMap<u32, Node>,
    /// Deleted edges, with the payload they have before the delta.
    pub del_edges: BTreeMap<EdgeId, GEdge>,
    /// Added nodes.
    pub add_nodes: BTreeMap<u32, Node>,
    /// Added edges.
    pub add_edges: BTreeMap<EdgeId, GEdge>,
    /// Attribution changes of kept elements.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub attrs: BTreeMap<AttrKey, AttrChange>,
}

/// A graph delta in canonical form.
///
/// The fields, in this order and serialized under these names, are
/// `del_nodes: BTreeMap<u32, Node>`, `del_edges: BTreeMap<EdgeId, GEdge>`,
/// `add_nodes: BTreeMap<u32, Node>`, `add_edges: BTreeMap<EdgeId, GEdge>` and
/// `attrs: BTreeMap<AttrKey, AttrChange>` (written as a sequence of pairs,
/// since its keys are not strings). They are private, so every value is in
/// normal form:
///
/// 1. no attribution change has `old` equal to `new`;
/// 2. no attribution key names an element the same delta deletes or adds;
/// 3. an id in both a delete part and an add part is a replacement and is
///    kept even when the two payloads are equal.
///
/// Every constructor, decoding included, goes through
/// [`GraphDelta::normalize`]. Equality is equality of the canonical bytes
/// ([`canonical_graph_delta_bytes`]), so two deltas are equal exactly when
/// they have the same representation, a `NaN` float equal to itself.
///
/// ```
/// use panproto_inst::graph_edit::{AttrKey, AttrVal, GraphDelta};
/// use panproto_inst::{GInstance, Node};
///
/// let mut graph = GInstance::new().with_node(Node::new(0, "person"));
/// let delta = GraphDelta::identity()
///     .with_change(
///         AttrKey::Anchor { node: 0 },
///         AttrVal::Anchor("person".into()),
///         AttrVal::Anchor("agent".into()),
///     )
///     .unwrap();
/// delta.apply(&mut graph).unwrap();
/// assert_eq!(graph.node(0).unwrap().anchor.as_ref(), "agent");
/// ```
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(try_from = "GraphDeltaParts")]
pub struct GraphDelta {
    del_nodes: BTreeMap<u32, Node>,
    del_edges: BTreeMap<EdgeId, GEdge>,
    add_nodes: BTreeMap<u32, Node>,
    add_edges: BTreeMap<EdgeId, GEdge>,
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    attrs: BTreeMap<AttrKey, AttrChange>,
}

impl PartialEq for GraphDelta {
    fn eq(&self, other: &Self) -> bool {
        canonical_graph_delta_bytes(self) == canonical_graph_delta_bytes(other)
    }
}

impl Eq for GraphDelta {}

impl TryFrom<GraphDeltaParts> for GraphDelta {
    type Error = MalformedDelta;

    fn try_from(parts: GraphDeltaParts) -> Result<Self, Self::Error> {
        Self::normalize(parts)
    }
}

impl From<GraphDelta> for GraphDeltaParts {
    fn from(delta: GraphDelta) -> Self {
        delta.into_parts()
    }
}

impl GraphDelta {
    /// The empty delta, the unit of [`Self::compose`].
    #[must_use]
    pub fn identity() -> Self {
        Self::default()
    }

    /// Bring `parts` into normal form.
    ///
    /// Drops every attribution change whose `old` equals its `new` (by
    /// canonical bytes) and every attribution change of an element the parts
    /// also delete or add; keeps every replacement.
    ///
    /// # Errors
    ///
    /// Returns [`MalformedDelta::NodeIdMismatch`] when a node is filed under
    /// an id other than its own, [`MalformedDelta::TableKey`] when an
    /// attribution key names a table entry, and
    /// [`MalformedDelta::ValueVariant`] when an `old` or `new` is not of the
    /// variant its key takes.
    pub fn normalize(parts: GraphDeltaParts) -> Result<Self, MalformedDelta> {
        let GraphDeltaParts {
            del_nodes,
            del_edges,
            add_nodes,
            add_edges,
            mut attrs,
        } = parts;
        for (key, node) in del_nodes.iter().chain(&add_nodes) {
            if *key != node.id {
                return Err(MalformedDelta::NodeIdMismatch {
                    key: *key,
                    node: node.id,
                });
            }
        }
        for (key, change) in &attrs {
            if key.is_table() {
                return Err(MalformedDelta::TableKey { key: key.clone() });
            }
            if !key.accepts(&change.old) || !key.accepts(&change.new) {
                return Err(MalformedDelta::ValueVariant { key: key.clone() });
            }
        }
        attrs.retain(|key, change| {
            let structural = match key.element() {
                ElemRef::Node(n) => del_nodes.contains_key(&n) || add_nodes.contains_key(&n),
                ElemRef::Edge(e) => del_edges.contains_key(&e) || add_edges.contains_key(&e),
                _ => false,
            };
            !structural && !same_val(&change.old, &change.new)
        });
        Ok(Self {
            del_nodes,
            del_edges,
            add_nodes,
            add_edges,
            attrs,
        })
    }

    /// The parts of this delta.
    #[must_use]
    pub fn into_parts(self) -> GraphDeltaParts {
        GraphDeltaParts {
            del_nodes: self.del_nodes,
            del_edges: self.del_edges,
            add_nodes: self.add_nodes,
            add_edges: self.add_edges,
            attrs: self.attrs,
        }
    }

    /// A copy of this delta's parts.
    #[must_use]
    pub fn to_parts(&self) -> GraphDeltaParts {
        self.clone().into_parts()
    }

    /// Deleted nodes, by id, with their payloads before the delta.
    #[must_use]
    pub const fn del_nodes(&self) -> &BTreeMap<u32, Node> {
        &self.del_nodes
    }

    /// Deleted edges, by id, with their payloads before the delta.
    #[must_use]
    pub const fn del_edges(&self) -> &BTreeMap<EdgeId, GEdge> {
        &self.del_edges
    }

    /// Added nodes, by id.
    #[must_use]
    pub const fn add_nodes(&self) -> &BTreeMap<u32, Node> {
        &self.add_nodes
    }

    /// Added edges, by id.
    #[must_use]
    pub const fn add_edges(&self) -> &BTreeMap<EdgeId, GEdge> {
        &self.add_edges
    }

    /// Attribution changes of kept elements, by key.
    #[must_use]
    pub const fn attrs(&self) -> &BTreeMap<AttrKey, AttrChange> {
        &self.attrs
    }

    /// Whether this is the identity delta.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.del_nodes.is_empty()
            && self.del_edges.is_empty()
            && self.add_nodes.is_empty()
            && self.add_edges.is_empty()
            && self.attrs.is_empty()
    }

    /// This delta with `node` deleted as well (its payload as it is before
    /// the delta). Drops any attribution change of the node.
    #[must_use]
    pub fn with_deleted_node(self, node: Node) -> Self {
        let mut parts = self.into_parts();
        parts.del_nodes.insert(node.id, node);
        Self::renormalized(parts)
    }

    /// This delta with `node` added as well. Drops any attribution change of
    /// the node.
    #[must_use]
    pub fn with_added_node(self, node: Node) -> Self {
        let mut parts = self.into_parts();
        parts.add_nodes.insert(node.id, node);
        Self::renormalized(parts)
    }

    /// This delta with the edge `id` deleted as well (its payload as it is
    /// before the delta). Drops any attribution change of the edge.
    #[must_use]
    pub fn with_deleted_edge(self, id: EdgeId, edge: GEdge) -> Self {
        let mut parts = self.into_parts();
        parts.del_edges.insert(id, edge);
        Self::renormalized(parts)
    }

    /// This delta with the edge `id` added as well. Drops any attribution
    /// change of the edge.
    #[must_use]
    pub fn with_added_edge(self, id: EdgeId, edge: GEdge) -> Self {
        let mut parts = self.into_parts();
        parts.add_edges.insert(id, edge);
        Self::renormalized(parts)
    }

    /// This delta with the attribution change `key: old -> new` as well,
    /// replacing any change of `key` it has.
    ///
    /// # Errors
    ///
    /// As [`Self::normalize`]: a table key or a value of the wrong variant.
    pub fn with_change(
        self,
        key: AttrKey,
        old: AttrVal,
        new: AttrVal,
    ) -> Result<Self, MalformedDelta> {
        let mut parts = self.into_parts();
        parts.attrs.insert(key, AttrChange { old, new });
        Self::normalize(parts)
    }

    /// [`Self::normalize`] of parts built from a delta by adding a node or an
    /// edge, which cannot fail.
    fn renormalized(mut parts: GraphDeltaParts) -> Self {
        parts.attrs.retain(|key, _| match key.element() {
            ElemRef::Node(n) => {
                !parts.del_nodes.contains_key(&n) && !parts.add_nodes.contains_key(&n)
            }
            ElemRef::Edge(e) => {
                !parts.del_edges.contains_key(&e) && !parts.add_edges.contains_key(&e)
            }
            _ => true,
        });
        Self {
            del_nodes: parts.del_nodes,
            del_edges: parts.del_edges,
            add_nodes: parts.add_nodes,
            add_edges: parts.add_edges,
            attrs: parts.attrs,
        }
    }

    /// Build a delta from parts known to be free of table keys and of
    /// wrongly-typed values, normalizing the rest.
    pub(super) fn from_valid_parts(parts: GraphDeltaParts) -> Self {
        let mut delta = Self::renormalized(parts);
        delta
            .attrs
            .retain(|_, change| !same_val(&change.old, &change.new));
        delta
    }
}

/// A payload that attribution entries are read from and written into: a
/// node or an edge.
pub(super) trait Payload: Clone {
    /// The entry `key` of this payload, `None` when the key belongs to the
    /// other element kind.
    fn get(&self, key: &AttrKey) -> Option<AttrVal>;
    /// Write `value` into the entry `key`; a key or value of the other
    /// element kind leaves the payload unchanged.
    fn set(&mut self, key: &AttrKey, value: &AttrVal);
    /// Payload equality by canonical bytes.
    fn same(&self, other: &Self) -> bool;
}

impl Payload for Node {
    fn get(&self, key: &AttrKey) -> Option<AttrVal> {
        match key {
            AttrKey::Value { .. } => Some(AttrVal::Presence(self.value.clone())),
            AttrKey::Field { field, .. } => {
                Some(AttrVal::Field(self.extra_fields.get(field).cloned()))
            }
            AttrKey::Meta { .. } => Some(AttrVal::Meta(NodeMeta::of(self))),
            AttrKey::Anchor { .. } => Some(AttrVal::Anchor(self.anchor.clone())),
            _ => None,
        }
    }

    fn set(&mut self, key: &AttrKey, value: &AttrVal) {
        match (key, value) {
            (AttrKey::Value { .. }, AttrVal::Presence(v)) => self.value.clone_from(v),
            (AttrKey::Field { field, .. }, AttrVal::Field(v)) => match v {
                Some(v) => {
                    self.extra_fields.insert(field.clone(), v.clone());
                }
                None => {
                    self.extra_fields.remove(field);
                }
            },
            (AttrKey::Meta { .. }, AttrVal::Meta(meta)) => meta.write_to(self),
            (AttrKey::Anchor { .. }, AttrVal::Anchor(anchor)) => self.anchor = anchor.clone(),
            _ => {}
        }
    }

    fn same(&self, other: &Self) -> bool {
        same_node(self, other)
    }
}

impl Payload for GEdge {
    fn get(&self, key: &AttrKey) -> Option<AttrVal> {
        match key {
            AttrKey::EdgeLabel { .. } => Some(AttrVal::Label(self.edge.clone())),
            AttrKey::EdgePosition { .. } => Some(AttrVal::Position(self.position)),
            _ => None,
        }
    }

    fn set(&mut self, key: &AttrKey, value: &AttrVal) {
        match (key, value) {
            (AttrKey::EdgeLabel { .. }, AttrVal::Label(label)) => self.edge = label.clone(),
            (AttrKey::EdgePosition { .. }, AttrVal::Position(p)) => self.position = *p,
            _ => {}
        }
    }

    fn same(&self, other: &Self) -> bool {
        same_edge(self, other)
    }
}
