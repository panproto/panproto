//! The complement: data discarded when projecting an instance forward.
//!
//! A [`Complement`] records everything a forward projection (a lens `get` or a
//! `restrict_with_complement`) drops from a source [`WInstance`](crate::WInstance),
//! so that the backward direction can reconstruct the original source from a
//! (possibly modified) view. It is the shared complement type for both the
//! asymmetric lens in `panproto-lens` and the polynomial-functor restrict
//! pipeline in [`crate::poly`].
//!
//! Not every field is populated by every producer: the lens `get` path fills
//! the structural-reconstruction fields (dropped nodes/arcs/fans, contraction
//! choices, parent map, arc-edge disambiguators, snapshots, synthesized
//! nodes, source fingerprint); the restrict pipeline additionally records
//! [`Complement::contracted_into`], the surviving ancestor each dropped node
//! collapsed into. Unused fields stay empty and are omitted from the
//! serialized form.

use std::collections::{HashMap, HashSet};

use panproto_schema::Edge;

use crate::fan::Fan;
use crate::metadata::Node;
use crate::value::{FieldPresence, Value};

/// The complement: data discarded by a forward projection, needed by the
/// backward direction to restore the original source instance.
///
/// When a forward projection maps a source instance to a target view, some
/// nodes, arcs, and structural decisions are lost. The complement records all
/// of this so the backward direction can reconstruct the full source.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Complement {
    /// Nodes from the source that do not appear in the target view.
    pub dropped_nodes: HashMap<u32, Node>,
    /// Arcs from the source that do not appear in the target view.
    pub dropped_arcs: Vec<(u32, u32, Edge)>,
    /// Fans from the source whose parent or children were dropped.
    pub dropped_fans: Vec<Fan>,
    /// Resolver decisions made during ancestor contraction.
    pub contraction_choices: HashMap<(u32, u32), Edge>,
    /// Original parent mapping before contraction.
    pub original_parent: HashMap<u32, u32>,
    /// Fingerprint of the source schema at projection time, used by the
    /// backward direction to validate that the complement matches the lens's
    /// source schema.
    #[serde(default)]
    pub source_fingerprint: u64,
    /// Pre-transform `extra_fields` for nodes that had `field_transforms`
    /// applied. Used by the backward direction to restore original field
    /// values.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub original_extra_fields: HashMap<u32, HashMap<String, Value>>,
    /// Exact edge used for every arc in the view, keyed by
    /// `(parent_id, child_id)`. This makes the backward direction
    /// deterministic when the source schema has parallel edges between the
    /// same vertex pair, ensuring the cartesian lift is unique.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub arc_edges: HashMap<(u32, u32), Edge>,
    /// Pre-coercion `node.value` for nodes that had `__value__` field
    /// transforms applied. Used by the backward direction to restore the
    /// original leaf value.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub original_values: HashMap<u32, Option<FieldPresence>>,
    /// View node ids synthesized during forward evaluation by the nest-style
    /// `expansion_path` mechanism. These nodes exist in the view (to satisfy
    /// the target schema's multi-hop path) but have no counterpart in the
    /// source instance. The backward direction drops them when reconstructing
    /// the source.
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub synthesized_nodes: HashSet<u32>,
    /// For each dropped node, the surviving node it contracted into (its
    /// nearest surviving ancestor). Populated by the restrict pipeline's
    /// ancestor contraction; the fiber of a target node is its direct
    /// preimage together with every source node recorded here as contracted
    /// into it.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub contracted_into: HashMap<u32, u32>,
}

impl Complement {
    /// Create an empty complement (no data discarded).
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Returns `true` if the complement discards no data (lossless
    /// transformation).
    ///
    /// The [`source_fingerprint`](Self::source_fingerprint) is provenance
    /// metadata rather than discarded data, so a lossless projection that
    /// still records a fingerprint counts as empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.dropped_nodes.is_empty()
            && self.dropped_arcs.is_empty()
            && self.dropped_fans.is_empty()
            && self.contraction_choices.is_empty()
            && self.original_parent.is_empty()
            && self.original_extra_fields.is_empty()
            && self.arc_edges.is_empty()
            && self.original_values.is_empty()
            && self.synthesized_nodes.is_empty()
            && self.contracted_into.is_empty()
    }
}
