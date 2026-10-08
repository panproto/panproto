//! The compiled migration model: the data a restrict reads.
//!
//! The value layer (survival predicates, field transforms, op-term
//! assignments) is evaluated by the expression language, which the
//! reference does not re-implement: a migration records only which source
//! vertices carry a survival predicate and which carry transforms, and the
//! operations that evaluate them take a [`crate::restrict::ValueLayer`]
//! supplied by the caller.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Name, SEdge};

/// A hyper-edge resolver key: the hyper-edge and the set of child labels a
/// fan carries.
pub type HyperShape = (Name, BTreeSet<Name>);

/// A compiled migration.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Migration {
    /// Vertices that survive (by target name).
    pub surviving_verts: BTreeSet<Name>,
    /// Edges that survive.
    pub surviving_edges: BTreeSet<SEdge>,
    /// Source vertex to target vertex.
    pub vertex_remap: BTreeMap<Name, Name>,
    /// Source edge to target edge.
    pub edge_remap: BTreeMap<SEdge, SEdge>,
    /// Contraction resolver, keyed in the target space.
    pub resolver: BTreeMap<(Name, Name), SEdge>,
    /// Expansion paths: target anchor pair to intermediate target anchors.
    pub expansion_path: BTreeMap<(Name, Name), Vec<Name>>,
    /// Hyper-edge resolver: fan shape to target hyper-edge and label map.
    pub hyper_resolver: BTreeMap<HyperShape, (Name, BTreeMap<Name, Name>)>,
    /// Source vertices with a survival predicate.
    pub conditional: BTreeSet<Name>,
    /// Source vertices with field transforms or op-term assignments.
    pub transformed: BTreeSet<Name>,
}

impl Migration {
    /// The image of a source vertex: `vertex_remap` extended by the
    /// identity.
    #[must_use]
    pub fn vertex_image<'a>(&'a self, v: &'a str) -> &'a str {
        self.vertex_remap.get(v).map_or(v, String::as_str)
    }

    /// Whether a source vertex survives: its image is a surviving vertex.
    #[must_use]
    pub fn survives(&self, v: &str) -> bool {
        self.surviving_verts.contains(self.vertex_image(v))
    }
}

/// Observable equality for the graph conjugation square.
#[must_use]
pub fn graph_square(left: &crate::model::Graph, right: &crate::model::Graph) -> bool {
    left == right
}

/// Observable equality for the table conjugation square.
#[must_use]
pub fn table_square(left: &crate::model::Table, right: &crate::model::Table) -> bool {
    left == right
}

/// Observable equality for the tree conjugation square.
#[must_use]
pub fn tree_square(left: &crate::model::Tree, right: &crate::model::Tree) -> bool {
    left == right
}
