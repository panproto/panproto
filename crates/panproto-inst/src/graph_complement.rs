//! The complement of a graph restrict.
//!
//! [`GraphComplement`] is what [`graph_restrict_with_plan`] leaves out of its
//! view: the source's dropped elements, the source data of kept elements
//! that the view does not determine, and the keys of what it synthesized.
//!
//! [`graph_restrict_with_plan`]: crate::graph_restrict_with_plan

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use serde::{Deserialize, Serialize};

use crate::ginstance::{EdgeId, GEdge};
use crate::identity::IdentityEnvelope;
use crate::metadata::Node;
use crate::value::{FieldPresence, Value};
use crate::view_ref::{IdFloor, SynthKey};

/// The complement of a graph restrict: the part of the source outside the
/// image of `get`, together with the fibre coordinates `get` forgets.
///
/// Get paired with this complement is injective on sources, so it is a
/// complement in the sense of bancilhonspyratos1981update (Def 4.4, stated).
/// It is not a constant complement: a view deletion cascades into it (an
/// edge the complement holds as dropped goes with a node the view deletes),
/// so `put` changes it (inferred). A view without a constant complement can
/// still be a lens of the kind johnsonrosebrughwood2012lenses build (Ex 4.2,
/// stated), so put-put can hold for deltas where it fails for states; the
/// very well behaved state put-put is not expected.
///
/// Every map is ordered, and the fields are serialized in declaration order
/// with no defaults, so a complement has one encoding.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphComplement {
    /// The source nodes the view does not hold, fan nodes included.
    pub dropped_nodes: BTreeMap<u32, Node>,
    /// The source edges the view does not hold, as they were in the source:
    /// edges with a dropped endpoint, edges whose fate drops them, source
    /// edges replaced by an expansion, and fan edges to dropped children.
    pub dropped_edges: BTreeMap<EdgeId, GEdge>,
    /// The source form of each kept edge whose source label the view does
    /// not determine: the edge maps back to its label only when exactly one
    /// source edge has its view label as image (for a fan edge, when its
    /// label is unchanged). A kept edge keeps its endpoints, so the recorded
    /// edge's endpoints are the view's.
    pub original_edges: BTreeMap<EdgeId, GEdge>,
    /// The source anchor of each kept node whose view anchor does not map
    /// back to it: the vertex map has several preimages for the view anchor,
    /// or one that is not the source anchor (for a fan node, when its anchor
    /// changed). An injective rename records nothing.
    pub original_anchors: BTreeMap<u32, Name>,
    /// The source fields of each kept node whose fields the restrict
    /// changed.
    pub original_extra_fields: BTreeMap<u32, BTreeMap<String, Value>>,
    /// The source value of each kept node whose value the restrict changed.
    pub original_values: BTreeMap<u32, Option<FieldPresence>>,
    /// Each synthesized node, by id, with its key.
    pub synthesized_nodes: BTreeMap<u32, SynthKey>,
    /// Each synthesized edge, by id, with its key.
    pub synthesized_edges: BTreeMap<EdgeId, SynthKey>,
    /// The floors synthesized ids were allocated from.
    pub id_floor: IdFloor,
    /// The envelope binding the complement to a lens, once stamped.
    pub identity: Option<IdentityEnvelope>,
}

impl GraphComplement {
    /// The complement of nothing: every map empty, floors zero, no
    /// envelope.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            dropped_nodes: BTreeMap::new(),
            dropped_edges: BTreeMap::new(),
            original_edges: BTreeMap::new(),
            original_anchors: BTreeMap::new(),
            original_extra_fields: BTreeMap::new(),
            original_values: BTreeMap::new(),
            synthesized_nodes: BTreeMap::new(),
            synthesized_edges: BTreeMap::new(),
            id_floor: IdFloor { node: 0, edge: 0 },
            identity: None,
        }
    }

    /// Whether the complement is [`Self::empty`]: every map empty, floors
    /// zero, no envelope.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.residue_is_trivial()
            && self.synthesized_nodes.is_empty()
            && self.synthesized_edges.is_empty()
            && self.id_floor == IdFloor::default()
            && self.identity.is_none()
    }

    /// Whether the complement holds no residue: nothing dropped and no
    /// source data recorded. The synthesized maps, the floors and the
    /// envelope are not residue.
    #[must_use]
    pub fn residue_is_trivial(&self) -> bool {
        self.dropped_nodes.is_empty()
            && self.dropped_edges.is_empty()
            && self.original_edges.is_empty()
            && self.original_anchors.is_empty()
            && self.original_extra_fields.is_empty()
            && self.original_values.is_empty()
    }

    /// The node ids the complement reserves: those of the dropped nodes.
    #[must_use]
    pub fn reserved_node_ids(&self) -> BTreeSet<u32> {
        self.dropped_nodes.keys().copied().collect()
    }

    /// The edge ids the complement reserves: those of the dropped edges.
    #[must_use]
    pub fn reserved_edge_ids(&self) -> BTreeSet<EdgeId> {
        self.dropped_edges.keys().copied().collect()
    }

    /// The key of a synthesized node.
    #[must_use]
    pub fn synth_key_of_node(&self, id: u32) -> Option<&SynthKey> {
        self.synthesized_nodes.get(&id)
    }

    /// The key of a synthesized edge.
    #[must_use]
    pub fn synth_key_of_edge(&self, id: EdgeId) -> Option<&SynthKey> {
        self.synthesized_edges.get(&id)
    }

    /// The id of the node synthesized under `key`.
    #[must_use]
    pub fn synth_node_id(&self, key: &SynthKey) -> Option<u32> {
        self.synthesized_nodes
            .iter()
            .find(|(_, k)| *k == key)
            .map(|(id, _)| *id)
    }

    /// The id of the edge synthesized under `key`.
    #[must_use]
    pub fn synth_edge_id(&self, key: &SynthKey) -> Option<EdgeId> {
        self.synthesized_edges
            .iter()
            .find(|(_, k)| *k == key)
            .map(|(id, _)| *id)
    }
}

impl Default for GraphComplement {
    fn default() -> Self {
        Self::empty()
    }
}
