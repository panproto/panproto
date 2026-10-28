//! The complement: data discarded when projecting an instance forward.
//!
//! A [`Complement`] records everything a forward projection (a lens `get` or a
//! `restrict_with_complement`) drops from a source [`WInstance`],
//! so that the backward direction can reconstruct the original source from a
//! (possibly modified) view. It is the shared complement type for both the
//! asymmetric lens in `panproto-lens` and the polynomial-functor restrict
//! pipeline in [`crate::poly`].
//!
//! Not every field is populated by every producer: the lens `get` path fills
//! the structural-reconstruction fields (dropped nodes/arcs/fans, contraction
//! choices, parent map, arc-edge disambiguators, snapshots, synthesized
//! nodes, original anchors, identity envelope); the restrict pipeline
//! additionally records [`Complement::contracted_into`], the surviving
//! ancestor each dropped node collapsed into. Unused fields stay empty.

use std::collections::{BTreeMap, HashMap, HashSet};

use panproto_gat::Name;
use panproto_schema::Edge;

use crate::error::InstError;
use crate::fan::Fan;
use crate::ginstance::{EdgeId, TreeEmbedding};
use crate::identity::{ComplementShape, IdentityCarrier, IdentityEnvelope};
use crate::metadata::Node;
use crate::segment::{IdAllocator, IdKind, IdRange};
use crate::value::{FieldPresence, Value};
use crate::view_ref::{SynthIndex, SynthKey};
use crate::wtype::WInstance;

/// The complement: data discarded by a forward projection, needed by the
/// backward direction to restore the original source instance.
///
/// When a forward projection maps a source instance to a target view, some
/// nodes, arcs, and structural decisions are lost. The complement records all
/// of this so the backward direction can reconstruct the full source.
///
/// Every field but the last is written on every encoding, for the reason
/// [`Node`] gives: `MessagePack` writes a struct as an array of its fields
/// in declaration order, so omitting one puts every later field in the
/// wrong slot. A field absent from an encoding still reads as its default,
/// which is how complements written before the later fields existed still
/// decode. [`identity`](Self::identity) is last and omitted when absent,
/// which is safe for the same reason: nothing follows it.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Complement {
    /// Nodes from the source that do not appear in the target view.
    #[serde(with = "panproto_schema::serde_helpers::sorted_map")]
    pub dropped_nodes: HashMap<u32, Node>,
    /// Arcs from the source that do not appear in the target view.
    pub dropped_arcs: Vec<(u32, u32, Edge)>,
    /// Source fans whose forward image is absent or differs from the source.
    ///
    /// This includes fans whose parent or every child was dropped, partially
    /// pruned fans, and fans retargeted or relabelled by a resolver. The
    /// backward direction removes one matching projected fan before restoring
    /// each recorded source fan, so this existing field carries fan residue
    /// without changing the serialized complement shape.
    pub dropped_fans: Vec<Fan>,
    /// Resolver decisions made during ancestor contraction.
    #[serde(with = "panproto_schema::serde_helpers::map_as_vec")]
    pub contraction_choices: HashMap<(u32, u32), Edge>,
    /// Original parent mapping before contraction.
    #[serde(with = "panproto_schema::serde_helpers::sorted_map")]
    pub original_parent: HashMap<u32, u32>,
    /// The legacy fingerprint of the source schema: the only binding to a
    /// lens that complements carried before [`identity`](Self::identity).
    ///
    /// Written alongside the envelope at 64-bit width (see
    /// [`legacy_schema_fingerprint`](crate::identity::legacy_schema_fingerprint))
    /// so that a reader which predates envelopes keeps its check. A reader
    /// that finds an envelope ignores it.
    #[serde(default)]
    pub source_fingerprint: u64,
    /// Pre-transform `extra_fields` for nodes that had `field_transforms`
    /// applied. Used by the backward direction to restore original field
    /// values.
    #[serde(default, with = "panproto_schema::serde_helpers::sorted_nested_map")]
    pub original_extra_fields: HashMap<u32, HashMap<String, Value>>,
    /// The source edge of every source arc between two nodes the view keeps
    /// whose edge the view does not determine, keyed by
    /// `(parent_id, child_id)`: the view holds no arc between the two, or
    /// the label of the view arc is not the image of exactly this one
    /// source edge between the endpoints' source anchors (for instance two
    /// parallel edges the edge map merges). Any other arc is not recorded,
    /// since the backward direction reads its edge off the view label and
    /// the source schema; recording only these keeps the cartesian lift
    /// unique without counting derivable data as residue.
    #[serde(default, with = "panproto_schema::serde_helpers::map_as_vec_default")]
    pub arc_edges: HashMap<(u32, u32), Edge>,
    /// The source instance's arcs in order, as `(parent_id, child_id)`.
    ///
    /// Arc order is not incidental: the children of a collection node are
    /// its elements *in sequence*, so serialization reads array order
    /// straight off it. The backward direction rebuilds arcs by walking
    /// `original_parent`, a `HashMap`, whose iteration order varies between
    /// runs, which reorders every array it reconstructs. Recording the
    /// sequence here is what lets `put` put them back as they were.
    ///
    /// Empty on a complement produced before this was recorded, in which
    /// case the backward direction falls back to a deterministic order
    /// rather than a random one.
    #[serde(default)]
    pub arc_order: Vec<(u32, u32)>,
    /// Pre-coercion `node.value` for nodes that had `__value__` field
    /// transforms applied. Used by the backward direction to restore the
    /// original leaf value.
    #[serde(default, with = "panproto_schema::serde_helpers::sorted_map")]
    pub original_values: HashMap<u32, Option<FieldPresence>>,
    /// View node ids synthesized during forward evaluation by the nest-style
    /// `expansion_path` mechanism. These nodes exist in the view (to satisfy
    /// the target schema's multi-hop path) but have no counterpart in the
    /// source instance. The backward direction drops them when reconstructing
    /// the source.
    #[serde(default, with = "panproto_schema::serde_helpers::sorted_set")]
    pub synthesized_nodes: HashSet<u32>,
    /// For each dropped node, the surviving node it contracted into (its
    /// nearest surviving ancestor). Populated by the restrict pipeline's
    /// ancestor contraction; the fiber of a target node is its direct
    /// preimage together with every source node recorded here as contracted
    /// into it.
    #[serde(default, with = "panproto_schema::serde_helpers::sorted_map")]
    pub contracted_into: HashMap<u32, u32>,
    /// The source anchor of every surviving node whose anchor the forward
    /// projection rewrote to a target vertex with more than one source
    /// preimage.
    ///
    /// Under an injective vertex remap the backward direction recovers a
    /// node's source anchor by inverting the remap. When two source
    /// vertices map to the same target vertex the inverse is not a
    /// function, and the view alone cannot say which of them a node came
    /// from; this records it.
    #[serde(default, with = "panproto_schema::serde_helpers::sorted_map")]
    pub original_anchors: HashMap<u32, Name>,
    /// The node id floor of the source at `get` time: one above every node
    /// id of the source's graph form, its fan nodes included (see
    /// [`tree_id_floor`]). Fresh view ids are allocated at or above it
    /// ([`fresh_view_node_id`]), so a node the view adds never takes the id
    /// of a source node, even one the view deleted.
    ///
    /// 0 on a complement written before the floor was recorded, which then
    /// derives its floor from the ids it and the view hold. `u32::MAX` when
    /// the source's floor does not fit in a `u32` (it holds the id
    /// `u32::MAX`); no fresh id is allocated against it.
    #[serde(default)]
    pub id_floor: u32,
    /// The key of every node `get` synthesized, by view node id.
    ///
    /// A synthesized node has no source counterpart; its key names it
    /// independently of the id it was given, which is a presentation. Keys
    /// naming a source arc name it by its graph edge id in the source's
    /// embedding, which is the arc's position in the source's arc order.
    #[serde(default)]
    pub synthesized_keys: BTreeMap<u32, SynthKey>,
    /// The lens this complement was produced by. See
    /// [`crate::identity`].
    ///
    /// `None` on a complement written before envelopes existed, which
    /// binds itself only through [`source_fingerprint`](Self::source_fingerprint).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<IdentityEnvelope>,
}

impl Complement {
    /// Create an empty complement (no data discarded).
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Returns `true` if every field of the complement is unset.
    ///
    /// The [`source_fingerprint`](Self::source_fingerprint) and the
    /// [`identity`](Self::identity) envelope are provenance rather than
    /// discarded data, and [`id_floor`](Self::id_floor),
    /// [`synthesized_keys`](Self::synthesized_keys) and the contraction
    /// bookkeeping ([`contraction_choices`](Self::contraction_choices),
    /// [`contracted_into`](Self::contracted_into)) are derived from the
    /// source and the lens, so a projection that records only those counts
    /// as empty.
    ///
    /// This is a statement about the *representation*, not about
    /// information loss: a lens that discards nothing still populates
    /// `original_parent` and `arc_order`, so it is not empty by this
    /// measure. For the question "did this projection lose
    /// anything", see [`residue_is_trivial`](Self::residue_is_trivial).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.dropped_nodes.is_empty()
            && self.dropped_arcs.is_empty()
            && self.dropped_fans.is_empty()
            && self.original_parent.is_empty()
            && self.original_extra_fields.is_empty()
            && self.arc_edges.is_empty()
            && self.arc_order.is_empty()
            && self.original_values.is_empty()
            && self.synthesized_nodes.is_empty()
            && self.original_anchors.is_empty()
    }

    /// Returns `true` when this complement carries no information that the
    /// view lacks, i.e. when the complement is terminal.
    ///
    /// A lens with complement is a decomposition `S ≅ V × C`: `get` splits
    /// a source into a view and a complement, and `put` reassembles it. The
    /// complement's fields divide into two kinds under that reading, and
    /// only one of them is `C`:
    ///
    /// * **Residue.** Dropped nodes and arcs; source fans whose projection is
    ///   absent or non-identical; snapshots of values the forward pass
    ///   overwrote; source anchors a non-injective remap made unrecoverable;
    ///   and the source edge of an arc whose view label does not determine
    ///   it. This is the information present in `S` and absent from `V`, so
    ///   it *is* the complement, and no amount of looking at `V` recovers it.
    /// * **Bookkeeping.** `original_parent`, `arc_order`, the contraction
    ///   choices and `contracted_into`, `id_floor`, `synthesized_keys`.
    ///   These record the shape of the reassembly, which is a function of
    ///   the view's own structure, the dropped nodes and the lens rather
    ///   than of the particular record. They make `put` cheap and
    ///   deterministic; they carry no information the view lacks.
    ///
    /// Two residue fields are recorded by `get` only where the view does
    /// not determine them, so that counting their entries is counting
    /// information: [`original_anchors`](Self::original_anchors) holds a
    /// node only when the unique reverse of the vertex remap would not
    /// return its source anchor, and [`arc_edges`](Self::arc_edges) holds
    /// an arc only when the source edges between its endpoints' anchors
    /// whose image is its view label are not exactly one. These are the
    /// criteria of the graph complement's `original_anchors` and
    /// `original_edges`, so a tree and its graph form classify alike.
    ///
    /// So `C ≅ 1` exactly when the residue is empty, whatever bookkeeping
    /// is present. That is the condition under which `S ≅ V × 1 ≅ V`, and
    /// therefore the condition under which a source can be reconstructed
    /// from a view alone. See `panproto_lens::put_without_complement`.
    #[must_use]
    pub fn residue_is_trivial(&self) -> bool {
        self.dropped_nodes.is_empty()
            && self.dropped_arcs.is_empty()
            && self.dropped_fans.is_empty()
            && self.original_extra_fields.is_empty()
            && self.original_values.is_empty()
            && self.original_anchors.is_empty()
            && self.arc_edges.is_empty()
    }

    /// The synthesized elements of the view this complement was produced
    /// with, keyed by [`SynthKey`], with arcs named by their graph edge ids
    /// in `embedding`, the embedding of that view.
    ///
    /// `nodes` is [`synthesized_keys`](Self::synthesized_keys). `edges`
    /// holds the arc into each synthesized node under the node's own key
    /// (the edge entering intermediate `h` has hop `h`), the arc from the
    /// last intermediate of each chain into its endpoint under the next hop,
    /// and each contracted arc
    /// ([`contraction_choices`](Self::contraction_choices)) under
    /// [`SynthKey::Contract`]. The endpoint of an `Expand` chain is the
    /// child of the source arc its key names, read off
    /// [`arc_order`](Self::arc_order) (the source's arc `i` is the graph
    /// edge `EdgeId(i)` of its embedding); that of a `ContractExpand` chain
    /// is its `tgt`. An arc `embedding` does not record is left out.
    #[must_use]
    pub fn synth_index(&self, embedding: &TreeEmbedding) -> SynthIndex {
        let mut edges: BTreeMap<EdgeId, SynthKey> = BTreeMap::new();
        let mut last_hop: BTreeMap<SynthKey, (u16, u32)> = BTreeMap::new();
        for (&node, key) in &self.synthesized_keys {
            if let Some(&edge) = embedding.arcs.get(&node) {
                edges.insert(edge, key.clone());
            }
            let (chain, hop) = chain_of(key);
            let entry = last_hop.entry(chain).or_insert((hop, node));
            if hop >= entry.0 {
                *entry = (hop, node);
            }
        }
        for (chain, (hop, _)) in last_hop {
            let Some(next) = hop.checked_add(1) else {
                continue;
            };
            let (endpoint, key) = match chain {
                SynthKey::Expand { source_edge, .. } => {
                    let endpoint = usize::try_from(source_edge.0)
                        .ok()
                        .and_then(|i| self.arc_order.get(i))
                        .map(|&(_, child)| child);
                    (
                        endpoint,
                        SynthKey::Expand {
                            source_edge,
                            hop: next,
                        },
                    )
                }
                SynthKey::ContractExpand { src, tgt, .. } => (
                    Some(tgt),
                    SynthKey::ContractExpand {
                        src,
                        tgt,
                        hop: next,
                    },
                ),
                SynthKey::Contract { .. }
                | SynthKey::ResolutionReference { .. }
                | SynthKey::ResolutionSpan { .. } => (None, chain),
            };
            if let Some(edge) = endpoint.and_then(|child| embedding.arcs.get(&child)) {
                edges.insert(*edge, key);
            }
        }
        for (&(src, tgt), edge) in &self.contraction_choices {
            if let Some(&id) = embedding.arcs.get(&tgt) {
                edges.insert(
                    id,
                    SynthKey::Contract {
                        src,
                        tgt,
                        edge: edge.clone(),
                    },
                );
            }
        }
        SynthIndex {
            nodes: self.synthesized_keys.clone(),
            edges,
        }
    }
}

/// The chain a synthesized node's key belongs to (the key with its hop set
/// to 0) and its hop.
fn chain_of(key: &SynthKey) -> (SynthKey, u16) {
    match key {
        SynthKey::Expand { source_edge, hop } => (
            SynthKey::Expand {
                source_edge: *source_edge,
                hop: 0,
            },
            *hop,
        ),
        SynthKey::ContractExpand { src, tgt, hop } => (
            SynthKey::ContractExpand {
                src: *src,
                tgt: *tgt,
                hop: 0,
            },
            *hop,
        ),
        SynthKey::Contract { .. }
        | SynthKey::ResolutionReference { .. }
        | SynthKey::ResolutionSpan { .. } => (key.clone(), 0),
    }
}

/// The node id floor of a tree: one above every node id of its graph form,
/// that is `max(tree node ids ∪ fan node ids) + 1`, and 0 for an empty tree.
///
/// A tree's graph form
/// ([`GInstance::from_wtype_embedded`](crate::GInstance::from_wtype_embedded))
/// keeps the tree's node ids and gives each fan, duplicates included, the
/// next node id above them, so this is the embedding's `next_node` and the
/// graph's own floor.
///
/// The floor saturates at `u32::MAX`: a tree holding the node id
/// `u32::MAX` records the floor `u32::MAX`.
///
/// # Errors
///
/// Returns [`InstError::NodeIdsExhausted`] when no node id below `u32::MAX`
/// is left for some fan.
pub fn tree_id_floor(tree: &WInstance) -> Result<u32, InstError> {
    let mut nodes = IdAllocator::from_source(IdRange::FULL, tree.nodes.keys().copied());
    for _ in &tree.fans {
        nodes.allocate().map_err(|e| e.whole_state(IdKind::Node))?;
    }
    Ok(nodes.floor())
}

/// A node id for a node added to `view`, a view of a source whose
/// complement is `complement`.
///
/// The id is `max(floor, 1 + max(view ids ∪ reserved ids ∪ synthesized
/// ids))`, where the reserved ids are the nodes the complement holds as
/// dropped and the synthesized ids those `get` created. The floor is
/// [`Complement::id_floor`]; a complement that predates it (floor 0) takes
/// every node id it records (dropped, parented, ordered or anchored) as
/// taken instead. The result is therefore never the id of a source node,
/// even one the view deleted, whose residue would otherwise reattach to the
/// new node on `put`.
///
/// Deterministic, and correct for a single writer: two writers allocating
/// against the same view may draw the same id.
///
/// The id `u32::MAX` is never returned: the floor that would follow it does
/// not fit in a `u32`, so, as
/// [`TreeEmbedding::allocate_node`](crate::TreeEmbedding::allocate_node)
/// does for the tree's graph form, allocation reports exhaustion there. A
/// complement whose source holds that id records the floor `u32::MAX`, and
/// every allocation against it fails.
///
/// # Errors
///
/// Returns [`InstError::NodeIdsExhausted`] when no id at or above the floor
/// and below `u32::MAX` is free.
pub fn fresh_view_node_id(view: &WInstance, complement: &Complement) -> Result<u32, InstError> {
    let allocator = IdAllocator::from_floor(IdRange::FULL, complement.id_floor);
    let reserved = complement.dropped_nodes.keys().copied();
    let synthesized = complement
        .synthesized_nodes
        .iter()
        .chain(complement.synthesized_keys.keys())
        .copied();
    let taken = view
        .nodes
        .keys()
        .copied()
        .chain(reserved)
        .chain(synthesized);
    let result = if complement.id_floor == 0 {
        let recorded = complement
            .original_parent
            .iter()
            .flat_map(|(&child, &parent)| [child, parent])
            .chain(
                complement
                    .arc_order
                    .iter()
                    .flat_map(|&pair| <[u32; 2]>::from(pair)),
            )
            .chain(complement.original_anchors.keys().copied())
            .chain(
                complement
                    .dropped_arcs
                    .iter()
                    .flat_map(|(p, c, _)| [*p, *c]),
            );
        allocator.fresh(taken.chain(recorded))
    } else {
        allocator.fresh(taken)
    };
    result.map_err(|e| e.whole_state(IdKind::Node))
}

impl IdentityCarrier for Complement {
    const SHAPE: ComplementShape = ComplementShape::WType;

    fn identity(&self) -> Option<&IdentityEnvelope> {
        self.identity.as_ref()
    }

    fn set_identity(&mut self, envelope: Option<IdentityEnvelope>) {
        self.identity = envelope;
    }

    fn legacy_fingerprint(&self) -> u64 {
        self.source_fingerprint
    }

    fn set_legacy_fingerprint(&mut self, fingerprint: u64) {
        self.source_fingerprint = fingerprint;
    }

    fn records_nothing(&self) -> bool {
        self.is_empty()
    }
}
