//! Restricting a tree held as segments, segment by segment.
//!
//! [`TreeRestrict`] runs tree get along a compiled plan as the phases of a
//! checkpointed run ([`run_segmented`](super::run_segmented)). A tree node
//! keeps its id, an arc and a fan edge keep their embedding ids, and an arc
//! or a fan edge is held by the owner of its source (the parent, or the fan
//! node), so the view is laid out under the source's partition.
//!
//! Tree get walks the tree breadth first from its root. A node whose anchor
//! survives is kept when its parent is: an arc between two surviving nodes
//! follows its edge's fate (kept, expanded into a path, or dropped together
//! with the subtree below it), and a surviving node below a dropped one is
//! joined to its nearest surviving ancestor by the plan's resolution of
//! their anchors. Without an expression, whether a node's anchor survives
//! depends on its anchor alone, which a ghost and an in-stub carry; what
//! crosses segments is whether a node is reached (a dropped arc above it
//! removes its subtree), its position in breadth-first order (which orders
//! the refusals), and the survivors a search through dropped nodes finds
//! below a surviving node. The phases are:
//!
//! - Export: each segment walks its own subtrees, each from a node whose
//!   parent arc enters from elsewhere (a *local root*) or from the tree's
//!   root, assuming the local root is reached. It records, for each arc
//!   leaving it, the local root above, whether the walk is cut on the way
//!   and the sibling ranks from the local root; for each node a fan edge
//!   from elsewhere targets, the same; for each dropped local root, the
//!   survivors and dropped exits a search through its dropped nodes finds;
//!   and the root, its greatest tree node id and its number of fans;
//! - Summarize: each segment closes the exports from the root down, which
//!   gives every local root whether it is reached and its position, walks
//!   its own nodes, and records the least refusal of each stage it finds,
//!   the synthesized node keys, and the in-stubs and greatest ids of its
//!   view;
//! - Assemble: in whole-state get's order, `RootPruned`, the least
//!   refusal of the walk in breadth-first order, the least merged field,
//!   the exhaustion of synthesized node ids from the tree floor, the least
//!   transform refusal in node id, and the least fan refusal in fan order,
//!   each recomputed in the segment owning its element; then the least
//!   allocation failure;
//! - Emit: each segment's view (its surviving nodes, kept arcs, synthesized
//!   paths, surviving fans, root, the embedding counters in the root's
//!   segment, and its boundary) and its [`TreeComplementSegment`].
//!
//! Synthesized elements are owned by the segment owning the source of
//! their chain (the parent of an expanded arc, the ancestor of a contracted
//! pair) and take ids from that segment's allocation ranges, in ascending
//! key order, as segmented graph restriction numbers them. A plan whose
//! survival or transforms carry an expression is declined with
//! [`Residual::Global`] before any task.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use panproto_gat::Name;
use panproto_schema::{CanonicalWriter, Edge, InstanceShapeKind};
use serde::{Deserialize, Serialize};

use super::body::{AnySegment, Boundary, Ghost, InStub, TreeSegment, is_fan_node};
use super::check::fan_edge_label;
use super::error::SegmentError;
use super::ids::{IdKind, SegmentId};
use super::manifest::ManifestRole;
use super::partition::{OwnerIndex, Partition};
use super::reader::Reader;
use super::restrict::{
    GraphComplementSegment, SegmentedRestrictError, graph_restrict_locality, number_keys,
};
use super::runner::{Emission, PhaseContext, PhaseOperation, PhaseStop};
use super::store::{ArtifactKind, Phase};
use super::table_restrict::ComplementSegment;
use crate::error::RestrictError;
use crate::fan::Fan;
use crate::ginstance::{EdgeId, FanEmbedding, GEdge};
use crate::identity::{
    CanonicalSink, ComplementFormat, ComplementShape, Digest, IDENTITY_ENVELOPE_VERSION,
    IdentityEnvelope, SEGMENT_COMPLEMENT_CONTEXT, SEGMENT_FORMAT_VERSION,
    TREE_COMPLEMENT_FORMAT_VERSION,
};
use crate::locality::{Locality, SegmentOp};
use crate::metadata::{Node, NodeShape};
use crate::restrict_plan::{EdgeFate, RestrictPlan, allocate_synthesized};
use crate::segment::{IdAllocator, IdRange};
use crate::value::FieldPresence;
use crate::view_ref::{IdFloor, SynthKey, ViewRef};
use crate::wtype::{
    FanResolver, InstanceRef, SynthChain, TransformContext, TreeChildren, WInstance,
    contracted_chain_between, first_merged_field, project_fan,
};

/// The operation bytes of tree restriction in a run's identity
/// ([`RunJournal::run_digest`](super::RunJournal::run_digest)).
pub const TREE_RESTRICT_OP: &[u8] = b"tree restrict";

/// The complement of one segment of a segmented tree restriction, in the
/// graph form of the tree's embedding: the entries about the elements the
/// segment owns, and the ids of the source arcs it owns.
///
/// `graph` holds the dropped nodes (tree nodes and the fan nodes of dropped
/// fans), the dropped arcs and fan edges by embedding id, the source form
/// of each kept arc whose label the view does not determine, the original
/// anchors, fields and values of its surviving nodes, its synthesized nodes
/// and edges, and the floors of its allocation ranges after the run.
/// `arc_order` holds the embedding ids of every source tree arc it owns,
/// ascending, which is their arc order.
#[derive(Clone, Debug)]
pub struct TreeComplementSegment {
    /// The entries in graph form, with the segment and its floors.
    pub graph: GraphComplementSegment,
    /// The embedding ids of the source tree arcs it owns, ascending.
    pub arc_order: Vec<EdgeId>,
}

impl TreeComplementSegment {
    /// The number of entries the complement segment retains: the graph
    /// form's entries and the arc ids.
    #[must_use]
    pub fn element_count(&self) -> u64 {
        self.graph
            .element_count()
            .saturating_add(u64::try_from(self.arc_order.len()).unwrap_or(u64::MAX))
    }

    /// The complement segment's canonical bytes.
    ///
    /// ```text
    /// version: u16 (SEGMENT_FORMAT_VERSION), shape: u8 (0, tree)
    /// the graph complement segment's fields, from the dropped nodes to the
    ///   synthesized edges, as GraphComplementSegment::canonical_bytes writes them
    /// arc order: count, edge id (ascending)
    /// floors:    node floor: u32, edge floor: u32
    /// ```
    ///
    /// # Errors
    ///
    /// As [`GraphComplementSegment::canonical_bytes`], and
    /// [`SegmentError::Decode`] when the arc ids do not strictly ascend.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, SegmentError> {
        if self.arc_order.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(SegmentError::Decode {
                artifact: ArtifactKind::ComplementSegment,
                message: "tree arc order is not strictly ascending".to_owned(),
            });
        }
        let mut sink = CanonicalWriter::new();
        sink.write(&SEGMENT_FORMAT_VERSION.to_le_bytes());
        sink.tag(0);
        self.graph.write_fields(&mut sink)?;
        sink.len(self.arc_order.len());
        for edge in &self.arc_order {
            sink.u32(edge.0);
        }
        sink.u32(self.graph.floors.node);
        sink.u32(self.graph.floors.edge);
        Ok(sink.into_bytes())
    }

    /// The complement segment of `segment` whose canonical bytes are
    /// `bytes`: the inverse of [`TreeComplementSegment::canonical_bytes`].
    ///
    /// # Errors
    ///
    /// [`SegmentError::Decode`] for the complement segment artifact when the
    /// version or shape tag is not the tree's, a field is malformed as
    /// [`GraphComplementSegment::from_canonical_bytes`] refuses it, the arc
    /// ids do not strictly ascend, or bytes follow the floors.
    pub fn from_canonical_bytes(bytes: &[u8], segment: SegmentId) -> Result<Self, SegmentError> {
        let mut r = Reader::new(bytes, ArtifactKind::ComplementSegment);
        let version = r.u16()?;
        if version != SEGMENT_FORMAT_VERSION {
            return Err(r.error(format!("complement format version {version}")));
        }
        let shape = r.u8()?;
        if shape != 0 {
            return Err(r.error(format!("complement shape tag {shape}")));
        }
        let mut graph = GraphComplementSegment::read_fields(&mut r, segment)?;
        let mut arc_order = Vec::new();
        for _ in 0..r.count(4)? {
            let edge = EdgeId(r.u32()?);
            if arc_order.last().is_some_and(|last| *last >= edge) {
                return Err(r.error("tree arc order is not strictly ascending"));
            }
            arc_order.push(edge);
        }
        graph.floors = IdFloor {
            node: r.u32()?,
            edge: r.u32()?,
        };
        r.finish()?;
        Ok(Self { graph, arc_order })
    }

    /// The complement segment's digest: its canonical bytes under
    /// [`SEGMENT_COMPLEMENT_CONTEXT`].
    ///
    /// # Errors
    ///
    /// As [`TreeComplementSegment::canonical_bytes`].
    pub fn digest(&self) -> Result<Digest, SegmentError> {
        Ok(Digest::derive(
            SEGMENT_COMPLEMENT_CONTEXT,
            &self.canonical_bytes()?,
        ))
    }
}

/// Tree restriction along a compiled plan as a [`PhaseOperation`]: the
/// phases described in the module documentation.
pub struct TreeRestrict<'p> {
    /// The compiled plan.
    pub plan: &'p RestrictPlan,
}

/// A position in breadth-first order: the sibling ranks from the root.
/// Positions compare by length, then lexicographically.
type Position = Vec<u32>;

/// Whether position `a` comes before position `b` in breadth-first order.
fn earlier(a: &[u32], b: &[u32]) -> bool {
    (a.len(), a) < (b.len(), b)
}

/// What a walk from a local root, assumed reached, finds at a node: the
/// local root, whether the walk is cut before the node, and the sibling
/// ranks from the local root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Down {
    root: u32,
    cut: bool,
    ranks: Position,
}

/// A survivor a search through dropped nodes finds: the node, its source
/// anchor, the label of the arc entering it, and the sibling ranks from
/// where the search began.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Survivor {
    node: u32,
    anchor: Name,
    entering: Edge,
    ranks: Position,
}

/// What a search through the dropped nodes a segment owns finds below a
/// node: the survivors, and the dropped nodes owned elsewhere it leaves
/// through, each with its sibling ranks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Reach {
    survivors: Vec<Survivor>,
    exits: Vec<(u32, Position)>,
}

/// One segment's Export artifact.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Export {
    /// The tree's root and whether its anchor survives, in its segment.
    root: Option<(u32, bool)>,
    /// The greatest tree node id the segment owns.
    tree_max: Option<u32>,
    /// The number of fans the segment owns.
    fans: u64,
    /// The walk at each remote child of an arc the segment owns.
    down: BTreeMap<u32, Down>,
    /// The walk at each owned node a fan edge from elsewhere targets.
    points: BTreeMap<u32, Down>,
    /// The search below each dropped owned node an arc from elsewhere
    /// enters.
    entries: BTreeMap<u32, Reach>,
}

/// One segment's Summarize artifact.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Summary {
    /// The position of the segment's least refusal of the walk.
    walk: Option<Position>,
    /// The parent and edge of its least merged field.
    merged: Option<(u32, Edge)>,
    /// The least surviving node whose transform refuses.
    transform: Option<u32>,
    /// The parent edge of its first fan whose projection refuses.
    fan: Option<u32>,
    /// Whether its synthesized elements fail to take ids from its ranges.
    allocation: bool,
    /// The keys of its synthesized nodes.
    keys: BTreeSet<SynthKey>,
    /// The in-stubs of its view edges entering each other segment.
    stubs: BTreeMap<SegmentId, Vec<(ViewRef, InStub)>>,
    /// The greatest node id its view holds.
    max_node: Option<u32>,
    /// The greatest edge id its view holds.
    max_edge: Option<u32>,
}

fn malformed(artifact: ArtifactKind, message: &str) -> SegmentError {
    SegmentError::Decode {
        artifact,
        message: message.to_owned(),
    }
}

fn encode<T: Serialize>(value: &T, artifact: ArtifactKind) -> Result<Vec<u8>, SegmentError> {
    rmp_serde::to_vec(value).map_err(|error| malformed(artifact, &error.to_string()))
}

fn decode<T: for<'de> Deserialize<'de>>(
    bytes: &[u8],
    artifact: ArtifactKind,
) -> Result<T, SegmentError> {
    rmp_serde::from_slice(bytes).map_err(|error| malformed(artifact, &error.to_string()))
}

const fn restrict_error(segment: SegmentId, error: RestrictError) -> SegmentedRestrictError {
    SegmentedRestrictError::Restrict { segment, error }
}

type Stop = PhaseStop<SegmentedRestrictError>;

/// A tree arc a segment owns: its embedding id, child and label.
type OwnedArc<'s> = (EdgeId, u32, &'s Edge);

/// One tree segment as its run reads it.
struct Local<'s> {
    held: &'s TreeSegment,
    /// The fan edges it owns.
    fan_edges: BTreeSet<EdgeId>,
    /// The tree arcs it owns, by parent, in arc order.
    children: BTreeMap<u32, Vec<OwnedArc<'s>>>,
    /// The local parent of each owned node with one.
    parent: BTreeMap<u32, u32>,
    /// The source anchor of every node it names.
    anchors: BTreeMap<u32, &'s Name>,
    /// The owned nodes a tree arc from elsewhere enters.
    entered: BTreeSet<u32>,
    /// The owned tree nodes, ascending.
    tree_nodes: Vec<u32>,
}

impl<'s> Local<'s> {
    fn of(held: &'s TreeSegment) -> Self {
        let fan_edges = held.fan_edge_ids();
        let mut children: BTreeMap<u32, Vec<OwnedArc<'s>>> = BTreeMap::new();
        let mut parent = BTreeMap::new();
        for (id, (src, tgt, label)) in &held.arcs {
            if fan_edges.contains(id) {
                continue;
            }
            children.entry(*src).or_default().push((*id, *tgt, label));
            parent.insert(*tgt, *src);
        }
        let mut anchors: BTreeMap<u32, &Name> = BTreeMap::new();
        for ghost in held.boundary.ghosts.values() {
            anchors.insert(ghost.node, &ghost.anchor);
        }
        let mut entered = BTreeSet::new();
        for (elem, stub) in &held.boundary.in_stubs {
            anchors.insert(stub.src, &stub.src_anchor);
            if matches!(elem, ViewRef::Arc { .. }) {
                entered.insert(stub.tgt);
            }
        }
        for (id, node) in &held.nodes {
            anchors.insert(*id, &node.anchor);
        }
        let tree_nodes = held
            .nodes
            .iter()
            .filter(|(_, node)| !is_fan_node(node))
            .map(|(id, _)| *id)
            .collect();
        Self {
            held,
            fan_edges,
            children,
            parent,
            anchors,
            entered,
            tree_nodes,
        }
    }

    fn owns(&self, node: u32) -> bool {
        self.held.nodes.contains_key(&node)
    }

    fn anchor(&self, node: u32) -> Result<&'s Name, SegmentError> {
        self.anchors
            .get(&node)
            .copied()
            .ok_or_else(|| malformed(ArtifactKind::Segment, "a node without an anchor"))
    }

    fn children_of(&self, node: u32) -> &[OwnedArc<'s>] {
        self.children.get(&node).map_or(&[], Vec::as_slice)
    }

    /// The owned tree nodes whose parent arc is not owned here, ascending.
    fn local_roots(&self) -> Vec<u32> {
        self.tree_nodes
            .iter()
            .copied()
            .filter(|node| !self.parent.contains_key(node))
            .collect()
    }
}

/// Whether and where the walk reaches each owned tree node: reached, and
/// its position.
type Walk = BTreeMap<u32, (bool, Position)>;

/// The exports closed from the root down.
struct Closure {
    /// Whether each local root of the family is reached, and its position.
    roots: BTreeMap<u32, (bool, Position)>,
    /// Whether each node a fan edge from elsewhere targets is reached.
    points: BTreeMap<u32, bool>,
    /// The search below every dropped local root.
    entries: BTreeMap<u32, Reach>,
}

impl Closure {
    fn of(exports: &[Export]) -> Result<Self, SegmentError> {
        let mut roots_held = exports.iter().filter_map(|export| export.root);
        let (root, _) = roots_held
            .next()
            .ok_or_else(|| malformed(ArtifactKind::Export, "no segment holds the root"))?;
        if roots_held.next().is_some() {
            return Err(malformed(ArtifactKind::Export, "two segments hold a root"));
        }
        let mut below: BTreeMap<u32, Vec<(u32, &Down)>> = BTreeMap::new();
        for export in exports {
            for (child, down) in &export.down {
                below.entry(down.root).or_default().push((*child, down));
            }
        }
        let mut roots: BTreeMap<u32, (bool, Position)> =
            BTreeMap::from([(root, (true, Vec::new()))]);
        let mut queue = VecDeque::from([root]);
        while let Some(at) = queue.pop_front() {
            let (reached, position) = roots[&at].clone();
            for (child, down) in below.get(&at).map_or(&[][..], Vec::as_slice) {
                let mut child_position = position.clone();
                child_position.extend(&down.ranks);
                if roots
                    .insert(*child, (reached && !down.cut, child_position))
                    .is_some()
                {
                    return Err(malformed(ArtifactKind::Export, "a node has two parents"));
                }
                queue.push_back(*child);
            }
        }
        let mut points = BTreeMap::new();
        let mut entries = BTreeMap::new();
        for export in exports {
            for (node, down) in &export.points {
                let (reached, _) = roots
                    .get(&down.root)
                    .ok_or_else(|| malformed(ArtifactKind::Export, "an unreached local root"))?;
                points.insert(*node, *reached && !down.cut);
            }
            entries.extend(
                export
                    .entries
                    .iter()
                    .map(|(node, reach)| (*node, reach.clone())),
            );
        }
        Ok(Self {
            roots,
            points,
            entries,
        })
    }
}

/// A located refusal of one stage.
type Located<K> = Option<(K, RestrictError)>;

/// Hold the refusal `error` located at `key` when none is held or `key`
/// comes `before` the held one.
fn keep_first<K>(
    held: &mut Located<K>,
    key: K,
    error: RestrictError,
    before: impl Fn(&K, &K) -> bool,
) {
    if held.as_ref().is_none_or(|(old, _)| before(&key, old)) {
        *held = Some((key, error));
    }
}

/// Everything one segment's run computes: the located refusals of each
/// stage, and, when its ids are allocated, its view and complement.
struct Analysis {
    walk: Located<Position>,
    merged: Located<(u32, Edge)>,
    transform: Located<u32>,
    fan: Located<u32>,
    keys: BTreeSet<SynthKey>,
    allocation: Option<SegmentError>,
    output: Option<Output>,
}

/// One segment's view and complement before the boundary is laid out.
struct Output {
    view: TreeSegment,
    complement: TreeComplementSegment,
    stubs: BTreeMap<SegmentId, Vec<(ViewRef, InStub)>>,
}

/// A kept arc: its id, parent, child, view label and source label.
type Kept<'s> = (EdgeId, u32, u32, Edge, &'s Edge);

/// A synthesized chain, with the source label of the arc it expands.
type Chain<'s> = (SynthChain, Option<&'s Edge>);

/// A surviving fan: its embedding, its projection, and the label each
/// surviving child moves from and to.
type ViewFan<'s> = (&'s FanEmbedding, Fan, Vec<(String, String)>);

/// What the walk stage finds in one segment.
#[derive(Default)]
struct WalkStage<'s> {
    /// The segment's least refusal, by position.
    refusal: Located<Position>,
    /// The kept arcs.
    kept: Vec<Kept<'s>>,
    /// The expanded arcs and contracted pairs whose source it owns.
    chains: Vec<Chain<'s>>,
    /// The label of the source arc entering each contracted survivor, by
    /// its pair.
    entering: BTreeMap<(u32, u32), Edge>,
}

/// The parts of a run that refuses nothing, from which its output is
/// built.
struct Parts<'a, 's> {
    stage: &'a WalkStage<'s>,
    node_keys: &'a BTreeMap<SynthKey, Name>,
    view_nodes: BTreeMap<u32, Node>,
    view_fans: Vec<ViewFan<'s>>,
}

/// An output under construction.
struct Built {
    nodes: BTreeMap<u32, Node>,
    arcs: BTreeMap<EdgeId, (u32, u32, Edge)>,
    fans: Vec<FanEmbedding>,
    complement: GraphComplementSegment,
}

impl TreeRestrict<'_> {
    fn validated(cx: &PhaseContext<'_>, segment: SegmentId) -> Result<TreeSegment, Stop> {
        let bytes = cx.artifact(Phase::Validate, Some(segment))?;
        match AnySegment::from_canonical_bytes(&bytes, cx.partition())?.0 {
            AnySegment::Tree(segment) => Ok(segment),
            _ => Err(PhaseStop::from(malformed(
                ArtifactKind::Segment,
                "not a tree segment",
            ))),
        }
    }

    fn exports(cx: &PhaseContext<'_>) -> Result<Vec<Export>, Stop> {
        let mut exports = Vec::new();
        for id in cx.segments() {
            exports.push(decode(
                &cx.artifact(Phase::Export, Some(id))?,
                ArtifactKind::Export,
            )?);
        }
        Ok(exports)
    }

    fn summaries(cx: &PhaseContext<'_>) -> Result<Vec<Summary>, Stop> {
        let mut summaries = Vec::new();
        for id in cx.segments() {
            summaries.push(decode(
                &cx.artifact(Phase::Summarize, Some(id))?,
                ArtifactKind::Summary,
            )?);
        }
        Ok(summaries)
    }

    fn survives(&self, anchor: &Name) -> bool {
        self.plan.anchor_survives(anchor)
    }

    /// Whether the arc from `parent` to `child` of label `label` cuts the
    /// walk: both anchors survive and the edge is dropped.
    fn cuts(&self, local: &Local<'_>, (parent, child): (u32, u32), label: &Edge) -> bool {
        let survive = |node: u32| local.anchor(node).is_ok_and(|anchor| self.survives(anchor));
        survive(parent)
            && survive(child)
            && matches!(self.plan.edge_image.fate(label), Some(EdgeFate::Drop))
    }

    /// The walk of the owned tree nodes below `root`, starting from
    /// `start`, with the remote children of the arcs it crosses.
    fn walk_from(
        &self,
        local: &Local<'_>,
        root: u32,
        start: (bool, Position),
        walk: &mut Walk,
        remote: &mut BTreeMap<u32, (bool, Position)>,
    ) {
        let mut queue = VecDeque::from([(root, start)]);
        while let Some((at, (reached, position))) = queue.pop_front() {
            for (rank, (_, child, label)) in (0_u32..).zip(local.children_of(at)) {
                let child_reached = reached && !self.cuts(local, (at, *child), label);
                let mut child_position = position.clone();
                child_position.push(rank);
                if local.owns(*child) {
                    queue.push_back((*child, (child_reached, child_position)));
                } else {
                    remote.insert(*child, (child_reached, child_position));
                }
            }
            walk.insert(at, (reached, position));
        }
    }

    /// The search through the dropped nodes `local` owns below `start`, its
    /// first steps being the children of `start`. A child of `start` whose
    /// anchor survives is a survivor only when `direct` holds: below a
    /// surviving `start` it is joined by its own arc, not contracted.
    fn search(&self, local: &Local<'_>, start: u32, direct: bool) -> Result<Reach, SegmentError> {
        let mut reach = Reach::default();
        let mut queue: VecDeque<(u32, Position)> = VecDeque::from([(start, Vec::new())]);
        while let Some((at, path)) = queue.pop_front() {
            for (rank, (_, child, label)) in (0_u32..).zip(local.children_of(at)) {
                let mut ranks = path.clone();
                ranks.push(rank);
                let anchor = local.anchor(*child)?;
                if self.survives(anchor) {
                    if at == start && !direct {
                        continue;
                    }
                    reach.survivors.push(Survivor {
                        node: *child,
                        anchor: anchor.clone(),
                        entering: (*label).clone(),
                        ranks,
                    });
                } else if local.owns(*child) {
                    queue.push_back((*child, ranks));
                } else {
                    reach.exits.push((*child, ranks));
                }
            }
        }
        Ok(reach)
    }

    /// The Export of `held`.
    fn export_of(&self, held: &TreeSegment) -> Result<Export, SegmentError> {
        let local = Local::of(held);
        let mut export = Export {
            root: match held.root {
                Some(root) => Some((root, self.survives(local.anchor(root)?))),
                None => None,
            },
            tree_max: local.tree_nodes.last().copied(),
            fans: u64::try_from(held.fans.len()).unwrap_or(u64::MAX),
            ..Export::default()
        };
        let targeted: BTreeSet<u32> = held
            .boundary
            .in_stubs
            .iter()
            .filter(|(elem, _)| matches!(elem, ViewRef::Edge(_)))
            .map(|(_, stub)| stub.tgt)
            .collect();
        for root in local.local_roots() {
            let mut walk = Walk::new();
            let mut remote = BTreeMap::new();
            self.walk_from(&local, root, (true, Vec::new()), &mut walk, &mut remote);
            for (child, (reached, ranks)) in remote {
                export.down.insert(
                    child,
                    Down {
                        root,
                        cut: !reached,
                        ranks,
                    },
                );
            }
            for (node, (reached, _)) in &walk {
                if targeted.contains(node) {
                    export.points.insert(
                        *node,
                        Down {
                            root,
                            cut: !reached,
                            ranks: Vec::new(),
                        },
                    );
                }
            }
            if local.entered.contains(&root) && !self.survives(local.anchor(root)?) {
                export
                    .entries
                    .insert(root, self.search(&local, root, true)?);
            }
        }
        Ok(export)
    }

    /// The walk of every owned tree node, from the closure.
    fn walk(&self, local: &Local<'_>, closure: &Closure) -> Result<(Walk, Walk), SegmentError> {
        let mut walk = Walk::new();
        let mut remote = BTreeMap::new();
        for root in local.local_roots() {
            let start = closure
                .roots
                .get(&root)
                .cloned()
                .ok_or_else(|| malformed(ArtifactKind::Export, "an unreached local root"))?;
            self.walk_from(local, root, start, &mut walk, &mut remote);
        }
        Ok((walk, remote))
    }

    /// The survivors `start`, a surviving node at `position`, reaches
    /// through dropped nodes of any segment, each with its position.
    fn contracted(
        &self,
        local: &Local<'_>,
        closure: &Closure,
        start: u32,
        position: &[u32],
    ) -> Result<Vec<Survivor>, SegmentError> {
        let first = self.search(local, start, false)?;
        let mut found = Vec::new();
        let mut visited = BTreeSet::new();
        let mut queue: VecDeque<(Reach, Position)> = VecDeque::from([(first, Vec::new())]);
        while let Some((reach, prefix)) = queue.pop_front() {
            for survivor in reach.survivors {
                let mut ranks = position.to_vec();
                ranks.extend(&prefix);
                ranks.extend(&survivor.ranks);
                found.push(Survivor { ranks, ..survivor });
            }
            for (exit, ranks) in reach.exits {
                if !visited.insert(exit) {
                    continue;
                }
                let entry = closure.entries.get(&exit).cloned().ok_or_else(|| {
                    malformed(ArtifactKind::Export, "a dropped node entered has no entry")
                })?;
                let mut next = prefix.clone();
                next.extend(&ranks);
                queue.push_back((entry, next));
            }
        }
        Ok(found)
    }

    /// Whether `node` survives, as the segment owning a fan sees it.
    fn fan_point_survives(
        &self,
        local: &Local<'_>,
        walk: &Walk,
        closure: &Closure,
        node: u32,
    ) -> Result<bool, SegmentError> {
        let reached = match walk.get(&node) {
            Some((reached, _)) => *reached,
            None => *closure
                .points
                .get(&node)
                .ok_or_else(|| malformed(ArtifactKind::Export, "a fan endpoint without a walk"))?,
        };
        Ok(reached && self.survives(local.anchor(node)?))
    }

    /// The source fan an owned fan embedding holds.
    fn source_fan(local: &Local<'_>, ids: &FanEmbedding) -> Result<Fan, SegmentError> {
        let target = |id: &EdgeId| {
            local
                .held
                .arcs
                .get(id)
                .map(|(_, tgt, _)| *tgt)
                .ok_or_else(|| malformed(ArtifactKind::Segment, "a fan edge is missing"))
        };
        let hyper = local.anchor(ids.node)?;
        let mut fan = Fan::new(hyper.as_ref(), target(&ids.parent_edge)?);
        for (label, id) in &ids.child_edges {
            fan = fan.with_child(label.clone(), target(id)?);
        }
        Ok(fan)
    }

    /// The source form of the arc `label` from `parent` to `child` that the
    /// view does not determine: the source edges between their anchors
    /// whose image is `view_label` are not exactly `label`.
    fn undetermined(&self, anchors: (&Name, &Name), label: &Edge, view_label: &Edge) -> bool {
        let mut preimages = self
            .plan
            .source()
            .edges_between(anchors.0, anchors.1)
            .iter()
            .filter(|candidate| self.edge_image(candidate) == *view_label);
        !(preimages.next() == Some(label) && preimages.next().is_none())
    }

    /// The image of a source edge: its entry in the edge remap; else the
    /// same kind and name across remapped endpoints when the target holds
    /// it, or the resolver's edge for those endpoints when the source holds
    /// one edge between them; else the carried edge.
    fn edge_image(&self, source: &Edge) -> Edge {
        let migration = self.plan.migration();
        if let Some(mapped) = migration.edge_remap.get(source) {
            return mapped.clone();
        }
        let remap = |vertex: &Name| {
            migration
                .vertex_remap
                .get(vertex)
                .cloned()
                .unwrap_or_else(|| vertex.clone())
        };
        let carried = Edge {
            src: remap(&source.src),
            tgt: remap(&source.tgt),
            kind: source.kind.clone(),
            name: source.name.clone(),
        };
        if self.plan.target().edges.contains_key(&carried) {
            return carried;
        }
        if self
            .plan
            .source()
            .edges_between(&source.src, &source.tgt)
            .len()
            == 1
            && let Some(resolved) = migration
                .resolver
                .get(&(carried.src.clone(), carried.tgt.clone()))
        {
            return resolved.clone();
        }
        carried
    }

    /// Whether the source anchor `source` of a node the view anchors at
    /// `view` is not the one the inverse of the vertex map returns.
    fn anchor_recorded(&self, view: &Name, source: &Name) -> bool {
        let mut preimages: Vec<&Name> = self
            .plan
            .migration()
            .vertex_remap
            .iter()
            .filter(|(_, image)| *image == view)
            .map(|(preimage, _)| preimage)
            .collect();
        preimages.sort();
        match preimages.as_slice() {
            [] => view != source,
            [only] => *only != source,
            _ => true,
        }
    }

    /// The view node of the surviving source node `source`: re-anchored,
    /// then transformed.
    fn view_node(&self, source: &Node) -> Result<Node, RestrictError> {
        let mut node = source.clone();
        node.anchor
            .clone_from(self.plan.target_anchor(&source.anchor));
        if !self
            .plan
            .migration()
            .value_transforms(&source.anchor)
            .is_empty()
        {
            let alone = WInstance::new(
                HashMap::from([(source.id, source.clone())]),
                Vec::new(),
                Vec::new(),
                source.id,
                source.anchor.clone(),
            );
            let children = TreeChildren::new(&alone);
            let ctx = TransformContext::bound(
                Some(self.plan.source()),
                &children,
                InstanceRef::Tree(&alone, source.id),
                &self.plan.demand(&source.anchor),
            );
            self.plan
                .transform(&mut node, &source.anchor, &ctx, false)?;
        }
        Ok(node)
    }

    /// Whether `node`, owned here or the remote child of an owned arc,
    /// survives: reached, with a surviving anchor.
    fn node_survives(
        &self,
        local: &Local<'_>,
        (walk, remote): (&Walk, &Walk),
        node: u32,
    ) -> Result<bool, SegmentError> {
        let reached = walk
            .get(&node)
            .or_else(|| remote.get(&node))
            .is_some_and(|(reached, _)| *reached);
        Ok(reached && self.survives(local.anchor(node)?))
    }

    /// The view anchor of a node `local` names.
    fn image<'n>(&'n self, local: &Local<'n>, node: u32) -> Result<&'n Name, SegmentError> {
        Ok(self.plan.target_anchor(local.anchor(node)?))
    }

    /// The fate of the arc `label` from the surviving `parent` to the child
    /// `child` whose anchor survives: kept, expanded, dropped or refused.
    fn arc_fate<'s>(
        &self,
        local: &Local<'s>,
        (id, parent, child): (EdgeId, u32, u32),
        label: &'s Edge,
        stage: &mut WalkStage<'s>,
    ) -> Result<Result<(), RestrictError>, SegmentError> {
        Ok(match self.plan.edge_image.fate(label) {
            None => Err(RestrictError::IncoherentEdgeImage {
                source_edge: Box::new(label.clone()),
                image: Box::new(label.clone()),
            }),
            Some(EdgeFate::Keep(edge)) => {
                stage.kept.push((id, parent, child, edge.clone(), label));
                Ok(())
            }
            Some(EdgeFate::Expand {
                intermediates,
                labels,
            }) => SynthChain::expand(
                id,
                (parent, child),
                (self.image(local, parent)?, self.image(local, child)?),
                intermediates,
                labels,
            )
            .map(|chain| stage.chains.push((chain, Some(label)))),
            Some(EdgeFate::Drop) => Ok(()),
            Some(EdgeFate::Unresolved(reason)) => Err(self.plan.unresolved(
                self.image(local, parent)?,
                self.image(local, child)?,
                *reason,
            )),
        })
    }

    /// The walk stage over the owned nodes: each arc from a reached
    /// surviving parent to a child whose anchor survives, and each survivor
    /// a surviving node reaches through dropped nodes.
    fn walk_stage<'s>(
        &self,
        local: &Local<'s>,
        walks: (&Walk, &Walk),
        closure: &Closure,
    ) -> Result<WalkStage<'s>, SegmentError> {
        let mut stage = WalkStage::default();
        for (parent, (reached, position)) in walks.0 {
            if !reached || !self.node_survives(local, walks, *parent)? {
                continue;
            }
            for (rank, (id, child, label)) in (0_u32..).zip(local.children_of(*parent)) {
                if !self.survives(local.anchor(*child)?) {
                    continue;
                }
                if let Err(error) =
                    self.arc_fate(local, (*id, *parent, *child), label, &mut stage)?
                {
                    let mut at = position.clone();
                    at.push(rank);
                    keep_first(&mut stage.refusal, at, error, |a, b| earlier(a, b));
                }
            }
            for survivor in self.contracted(local, closure, *parent, position)? {
                match contracted_chain_between(
                    self.plan,
                    (*parent, local.anchor(*parent)?),
                    (survivor.node, &survivor.anchor),
                ) {
                    Ok(chain) => {
                        stage
                            .entering
                            .insert((*parent, survivor.node), survivor.entering);
                        stage.chains.push((chain, None));
                    }
                    Err(error) => {
                        keep_first(&mut stage.refusal, survivor.ranks, error, |a, b| {
                            earlier(a, b)
                        });
                    }
                }
            }
        }
        Ok(stage)
    }

    /// The least merged field of the walk's kept arcs and chains, each
    /// counted under the source edge it comes from.
    fn merged_stage(stage: &WalkStage<'_>) -> Result<Located<(u32, Edge)>, SegmentError> {
        let mut images: Vec<(u32, &Edge, &Edge)> = Vec::new();
        for (_, parent, _, edge, source) in &stage.kept {
            images.push((*parent, edge, source));
        }
        for (chain, source) in &stage.chains {
            let image = match chain {
                SynthChain::Expand { parent, labels, .. } => labels
                    .first()
                    .zip(*source)
                    .map(|(first, source)| (*parent, first, source)),
                SynthChain::Contract { src, tgt, edge } => stage
                    .entering
                    .get(&(*src, *tgt))
                    .map(|entering| (*src, edge, entering)),
                SynthChain::ContractExpand {
                    src, tgt, labels, ..
                } => labels
                    .first()
                    .zip(stage.entering.get(&(*src, *tgt)))
                    .map(|(first, entering)| (*src, first, entering)),
            };
            images.extend(image);
        }
        match first_merged_field(images) {
            Ok(()) => Ok(None),
            Err(error) => match &error {
                RestrictError::MultipleArcsOnSingleValuedEdge { parent, edge, .. } => {
                    Ok(Some(((*parent, (**edge).clone()), error)))
                }
                _ => Err(malformed(ArtifactKind::Summary, "a merged field")),
            },
        }
    }

    /// The view nodes of the surviving owned nodes, and the least whose
    /// transform refuses.
    fn transform_stage(
        &self,
        local: &Local<'_>,
        walks: (&Walk, &Walk),
    ) -> Result<(BTreeMap<u32, Node>, Located<u32>), SegmentError> {
        let mut nodes = BTreeMap::new();
        let mut refusal: Located<u32> = None;
        for node in &local.tree_nodes {
            if !self.node_survives(local, walks, *node)? {
                continue;
            }
            match self.view_node(&local.held.nodes[node]) {
                Ok(view) => {
                    nodes.insert(*node, view);
                }
                Err(error) => {
                    if refusal.is_none() {
                        refusal = Some((*node, error));
                    }
                }
            }
        }
        Ok((nodes, refusal))
    }

    /// The projection of every owned fan, in fan order, and the first whose
    /// projection refuses.
    fn fan_stage<'s>(
        &self,
        local: &Local<'s>,
        walk: &Walk,
        closure: &Closure,
    ) -> Result<(Vec<ViewFan<'s>>, Located<u32>), SegmentError> {
        let resolver = FanResolver::new(self.plan.migration());
        let mut fans: Vec<&FanEmbedding> = local.held.fans.iter().collect();
        fans.sort_unstable_by_key(|ids| ids.parent_edge);
        let mut refusal: Located<u32> = None;
        let mut view = Vec::new();
        for ids in fans {
            let fan = Self::source_fan(local, ids)?;
            let mut known: BTreeMap<u32, bool> = BTreeMap::new();
            for node in std::iter::once(fan.parent).chain(fan.children.values().copied()) {
                known.insert(node, self.fan_point_survives(local, walk, closure, node)?);
            }
            let survives = |node: u32| known.get(&node).copied().unwrap_or(false);
            match project_fan(&fan, &survives, &resolver) {
                Ok(Some((projected, moves))) => view.push((ids, projected, moves)),
                Ok(None) => {}
                Err(error) => {
                    if refusal.is_none() {
                        refusal = Some((ids.parent_edge.0, error));
                    }
                }
            }
        }
        Ok((view, refusal))
    }

    /// One segment's run, from the closure of the family's exports.
    fn analyze(
        &self,
        held: &TreeSegment,
        partition: &Partition,
        index: &OwnerIndex,
        closure: &Closure,
    ) -> Result<Analysis, SegmentError> {
        let local = Local::of(held);
        let (walk, remote) = self.walk(&local, closure)?;
        let walks = (&walk, &remote);
        let mut stage = self.walk_stage(&local, walks, closure)?;
        let walk_refusal = stage.refusal.take();
        let merged = Self::merged_stage(&stage)?;
        let mut node_keys: BTreeMap<SynthKey, Name> = BTreeMap::new();
        for (chain, _) in &stage.chains {
            node_keys.extend(chain.node_keys());
        }
        let (view_nodes, transform) = self.transform_stage(&local, walks)?;
        let (view_fans, fan) = self.fan_stage(&local, &walk, closure)?;
        let refused =
            walk_refusal.is_some() || merged.is_some() || transform.is_some() || fan.is_some();
        let mut analysis = Analysis {
            walk: walk_refusal,
            merged,
            transform,
            fan,
            keys: node_keys.keys().cloned().collect(),
            allocation: None,
            output: None,
        };
        if !refused {
            let parts = Parts {
                stage: &stage,
                node_keys: &node_keys,
                view_nodes,
                view_fans,
            };
            match self.output(&local, partition, index, parts)? {
                Ok(output) => analysis.output = Some(output),
                Err(error) => analysis.allocation = Some(error),
            }
        }
        Ok(analysis)
    }

    /// The view and complement of a segment whose run refuses nothing, or
    /// the failure to take its synthesized ids from its ranges.
    fn output(
        &self,
        local: &Local<'_>,
        partition: &Partition,
        index: &OwnerIndex,
        parts: Parts<'_, '_>,
    ) -> Result<Result<Output, SegmentError>, SegmentError> {
        let held = local.held;
        let segment = held.segment;
        let spec = partition
            .spec(segment)
            .ok_or(SegmentError::MalformedPartition {
                segment,
                defect: super::error::PartitionDefect::UnknownSegment,
            })?;
        let source_nodes: BTreeSet<u32> = held.nodes.keys().copied().collect();
        let (node_ids, node_floor) = match number_keys(
            (segment, IdKind::Node),
            spec.alloc_nodes,
            &source_nodes,
            parts.node_keys.keys(),
            &BTreeMap::new(),
        ) {
            Ok(numbered) => numbered,
            Err(error) => return Ok(Err(error)),
        };
        let mut synthesized_arcs = Vec::new();
        for (chain, _) in &parts.stage.chains {
            synthesized_arcs.extend(chain.arcs(&node_ids));
        }
        let source_edges: BTreeSet<u32> = held.arcs.keys().map(|id| id.0).collect();
        let edge_keys: BTreeSet<&SynthKey> = synthesized_arcs.iter().map(|(key, ..)| key).collect();
        let (edge_ids, edge_floor) = match number_keys(
            (segment, IdKind::Edge),
            spec.alloc_edges,
            &source_edges,
            edge_keys,
            &BTreeMap::new(),
        ) {
            Ok(numbered) => numbered,
            Err(error) => return Ok(Err(error)),
        };
        let mut built = Built {
            nodes: BTreeMap::new(),
            arcs: BTreeMap::new(),
            fans: Vec::new(),
            complement: GraphComplementSegment {
                segment,
                dropped_nodes: BTreeMap::new(),
                dropped_edges: BTreeMap::new(),
                original_edges: BTreeMap::new(),
                original_anchors: BTreeMap::new(),
                original_extra_fields: BTreeMap::new(),
                original_values: BTreeMap::new(),
                synthesized_nodes: BTreeMap::new(),
                synthesized_edges: BTreeMap::new(),
                floors: IdFloor {
                    node: node_floor,
                    edge: edge_floor,
                },
            },
        };
        self.build_nodes(local, &parts, &node_ids, &mut built);
        self.build_arcs(local, &parts, synthesized_arcs, &edge_ids, &mut built)?;
        self.build_fans(local, &parts, &mut built)?;
        Ok(Ok(self.finish(local, index, built)?))
    }

    /// The view's surviving and synthesized nodes, with the residue of the
    /// surviving ones.
    fn build_nodes(
        &self,
        local: &Local<'_>,
        parts: &Parts<'_, '_>,
        node_ids: &BTreeMap<SynthKey, u32>,
        built: &mut Built,
    ) {
        for (id, view) in &parts.view_nodes {
            let source = &local.held.nodes[id];
            if self.anchor_recorded(&view.anchor, &source.anchor) {
                built
                    .complement
                    .original_anchors
                    .insert(*id, source.anchor.clone());
            }
            if !same_fields(&view.extra_fields, &source.extra_fields) {
                built.complement.original_extra_fields.insert(
                    *id,
                    source
                        .extra_fields
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect(),
                );
            }
            if !same_presence(view.value.as_ref(), source.value.as_ref()) {
                built
                    .complement
                    .original_values
                    .insert(*id, source.value.clone());
            }
            built.nodes.insert(*id, view.clone());
        }
        for (key, anchor) in parts.node_keys {
            let id = node_ids[key];
            built.nodes.insert(id, Node::new(id, anchor.clone()));
            built.complement.synthesized_nodes.insert(id, key.clone());
        }
    }

    /// The view's kept and synthesized arcs, with the source form of each
    /// kept arc the view does not determine.
    fn build_arcs(
        &self,
        local: &Local<'_>,
        parts: &Parts<'_, '_>,
        synthesized: Vec<(SynthKey, u32, u32, Edge)>,
        edge_ids: &BTreeMap<SynthKey, u32>,
        built: &mut Built,
    ) -> Result<(), SegmentError> {
        for (id, parent, child, edge, source) in &parts.stage.kept {
            if self.undetermined(
                (local.anchor(*parent)?, local.anchor(*child)?),
                source,
                edge,
            ) {
                built
                    .complement
                    .original_edges
                    .insert(*id, GEdge::new(*parent, *child, (*source).clone()));
            }
            built.arcs.insert(*id, (*parent, *child, edge.clone()));
        }
        for (key, from, to, label) in synthesized {
            let id = EdgeId(edge_ids[&key]);
            built.arcs.insert(id, (from, to, label));
            built.complement.synthesized_edges.insert(id, key);
        }
        Ok(())
    }

    /// The view anchor of `node`: its view node's, or the image of its
    /// source anchor.
    fn view_anchor(
        &self,
        local: &Local<'_>,
        nodes: &BTreeMap<u32, Node>,
        node: u32,
    ) -> Result<Name, SegmentError> {
        match nodes.get(&node) {
            Some(view) => Ok(view.anchor.clone()),
            None => Ok(self.image(local, node)?.clone()),
        }
    }

    /// The view's surviving fans, their nodes and edges.
    fn build_fans(
        &self,
        local: &Local<'_>,
        parts: &Parts<'_, '_>,
        built: &mut Built,
    ) -> Result<(), SegmentError> {
        for (ids, projected, moves) in &parts.view_fans {
            let hyper: Name = projected.hyper_edge_id.as_str().into();
            let parent_anchor = self.view_anchor(local, &built.nodes, projected.parent)?;
            built.arcs.insert(
                ids.parent_edge,
                (
                    ids.node,
                    projected.parent,
                    fan_edge_label(&hyper, &parent_anchor, None),
                ),
            );
            let mut child_edges = BTreeMap::new();
            for (old, new) in moves {
                let edge = ids.child_edges[old];
                let child = projected.children[new];
                let child_anchor = self.view_anchor(local, &built.nodes, child)?;
                built.arcs.insert(
                    edge,
                    (
                        ids.node,
                        child,
                        fan_edge_label(&hyper, &child_anchor, Some(new)),
                    ),
                );
                child_edges.insert(new.clone(), edge);
            }
            built.nodes.insert(
                ids.node,
                Node::new(ids.node, hyper).with_shape(NodeShape::HyperEdge),
            );
            built.fans.push(FanEmbedding {
                node: ids.node,
                parent_edge: ids.parent_edge,
                child_edges,
            });
        }
        built.fans.sort_unstable_by_key(|ids| ids.node);
        Ok(())
    }

    /// The dropped elements, the arc order and the view's crossing edges:
    /// a ghost here and an in-stub in the target's segment for each.
    fn finish(
        &self,
        local: &Local<'_>,
        index: &OwnerIndex,
        mut built: Built,
    ) -> Result<Output, SegmentError> {
        let held = local.held;
        for (id, node) in &held.nodes {
            if !built.nodes.contains_key(id) {
                built.complement.dropped_nodes.insert(*id, node.clone());
            }
        }
        let mut arc_order = Vec::new();
        for (id, (src, tgt, label)) in &held.arcs {
            if !local.fan_edges.contains(id) {
                arc_order.push(*id);
            }
            if !built.arcs.contains_key(id) {
                built
                    .complement
                    .dropped_edges
                    .insert(*id, GEdge::new(*src, *tgt, label.clone()));
            }
        }
        let view_fan_edges: BTreeSet<EdgeId> = built
            .fans
            .iter()
            .flat_map(|ids| {
                std::iter::once(ids.parent_edge).chain(ids.child_edges.values().copied())
            })
            .collect();
        let mut boundary = Boundary::default();
        let mut stubs: BTreeMap<SegmentId, Vec<(ViewRef, InStub)>> = BTreeMap::new();
        for (id, (src, tgt, label)) in &built.arcs {
            if built.nodes.contains_key(tgt) {
                continue;
            }
            let elem = if view_fan_edges.contains(id) {
                ViewRef::Edge(*id)
            } else {
                ViewRef::Arc {
                    parent: *src,
                    child: *tgt,
                }
            };
            let owner = index
                .owner_of_node(*tgt)
                .ok_or_else(|| SegmentError::Unowned { elem: elem.clone() })?;
            let src_anchor = built.nodes.get(src).map(|node| node.anchor.clone()).ok_or(
                SegmentError::DanglingEndpoint {
                    elem: elem.clone(),
                    node: *src,
                },
            )?;
            boundary.ghosts.entry(*tgt).or_insert(Ghost {
                node: *tgt,
                owner,
                anchor: self.view_anchor(local, &built.nodes, *tgt)?,
            });
            stubs.entry(owner).or_default().push((
                elem,
                InStub {
                    src: *src,
                    src_owner: held.segment,
                    src_anchor,
                    tgt: *tgt,
                    label: label.clone(),
                },
            ));
        }
        let root = held.root.filter(|root| built.nodes.contains_key(root));
        Ok(Output {
            view: TreeSegment {
                segment: held.segment,
                nodes: built.nodes,
                arcs: built.arcs,
                fans: built.fans,
                root,
                next_node: None,
                next_edge: None,
                boundary,
            },
            complement: TreeComplementSegment {
                graph: built.complement,
                arc_order,
            },
            stubs,
        })
    }

    /// The Summary of one segment's analysis.
    fn summary_of(analysis: &Analysis) -> Summary {
        let output = analysis.output.as_ref();
        Summary {
            walk: analysis.walk.as_ref().map(|(position, _)| position.clone()),
            merged: analysis.merged.as_ref().map(|(at, _)| at.clone()),
            transform: analysis.transform.as_ref().map(|(node, _)| *node),
            fan: analysis.fan.as_ref().map(|(edge, _)| *edge),
            allocation: analysis.allocation.is_some(),
            keys: analysis.keys.clone(),
            stubs: output
                .map(|output| output.stubs.clone())
                .unwrap_or_default(),
            max_node: output.and_then(|output| output.view.nodes.keys().next_back().copied()),
            max_edge: output.and_then(|output| output.view.arcs.keys().next_back().map(|id| id.0)),
        }
    }

    /// The refusal of the family, in whole-state get's order, each
    /// recomputed by `analysis` in the segment owning its element.
    fn refusal(
        segments: &[SegmentId],
        exports: &[Export],
        summaries: &[Summary],
        analysis: &mut dyn FnMut(SegmentId) -> Result<Analysis, Stop>,
    ) -> Result<Option<SegmentedRestrictError>, Stop> {
        let root = segments
            .iter()
            .zip(exports)
            .find_map(|(id, export)| export.root.map(|root| (*id, root)));
        if let Some((segment, (_, false))) = root {
            return Ok(Some(restrict_error(segment, RestrictError::RootPruned)));
        }
        let mut walk: Option<(&Position, SegmentId)> = None;
        let mut merged: Option<(&(u32, Edge), SegmentId)> = None;
        let mut transform: Option<(u32, SegmentId)> = None;
        let mut fan: Option<(u32, SegmentId)> = None;
        for (id, summary) in segments.iter().zip(summaries) {
            if let Some(position) = &summary.walk
                && walk.is_none_or(|(least, _)| earlier(position, least))
            {
                walk = Some((position, *id));
            }
            if let Some(at) = &summary.merged
                && merged.is_none_or(|(least, _)| at < least)
            {
                merged = Some((at, *id));
            }
            if let Some(node) = summary.transform
                && transform.is_none_or(|(least, _)| node < least)
            {
                transform = Some((node, *id));
            }
            if let Some(edge) = summary.fan
                && fan.is_none_or(|(least, _)| edge < least)
            {
                fan = Some((edge, *id));
            }
        }
        let confirmed = |located: Option<RestrictError>, segment: SegmentId| match located {
            Some(error) => Ok(Some(restrict_error(segment, error))),
            None => Err(PhaseStop::from(malformed(
                ArtifactKind::Summary,
                "a summary records a refusal its segment does not give",
            ))),
        };
        if let Some((_, segment)) = walk {
            return confirmed(analysis(segment)?.walk.map(|(_, error)| error), segment);
        }
        if let Some((_, segment)) = merged {
            return confirmed(analysis(segment)?.merged.map(|(_, error)| error), segment);
        }
        // The tree floor: one above every tree node id, then one id for each
        // fan's node, as whole-state get allocates synthesized nodes from it.
        let tree_max = segments
            .iter()
            .zip(exports)
            .filter_map(|(id, export)| export.tree_max.map(|max| (max, *id)))
            .max();
        let fans: u64 = exports.iter().map(|export| export.fans).sum();
        let mut floor = IdAllocator::from_source(IdRange::FULL, tree_max.map(|(max, _)| max));
        let mut tree_floor = Some(());
        for _ in 0..fans {
            if floor.allocate().is_err() {
                tree_floor = None;
                break;
            }
        }
        let floor = tree_floor.map_or(u32::MAX, |()| floor.floor());
        let keys: BTreeSet<SynthKey> = summaries
            .iter()
            .flat_map(|summary| summary.keys.iter().cloned())
            .collect();
        if let Err(error) = allocate_synthesized(keys, floor, IdKind::Node) {
            let segment = tree_max.map_or(SegmentId(0), |(_, segment)| segment);
            return Ok(Some(restrict_error(segment, error)));
        }
        if let Some((_, segment)) = transform {
            return confirmed(
                analysis(segment)?.transform.map(|(_, error)| error),
                segment,
            );
        }
        if let Some((_, segment)) = fan {
            return confirmed(analysis(segment)?.fan.map(|(_, error)| error), segment);
        }
        for (id, summary) in segments.iter().zip(summaries) {
            if summary.allocation {
                return match analysis(*id)?.allocation {
                    Some(error) => Ok(Some(SegmentedRestrictError::Segment(error))),
                    None => Err(PhaseStop::from(malformed(
                        ArtifactKind::Summary,
                        "a summary records an allocation failure its segment does not give",
                    ))),
                };
            }
        }
        Ok(None)
    }

    /// The view and complement segment of `segment`, its boundary's in-stubs
    /// and, in the root's segment, the embedding counters taken from every
    /// summary.
    fn emitted(
        held: &TreeSegment,
        analysis: Analysis,
        summaries: &[Summary],
    ) -> Result<(TreeSegment, TreeComplementSegment), SegmentError> {
        let Output {
            mut view,
            complement,
            ..
        } = analysis
            .output
            .ok_or_else(|| malformed(ArtifactKind::Summary, "an emitted segment refuses"))?;
        for summary in summaries {
            for (elem, stub) in summary
                .stubs
                .get(&view.segment)
                .map_or(&[][..], Vec::as_slice)
            {
                view.boundary.in_stubs.insert(elem.clone(), stub.clone());
            }
        }
        if view.root.is_some() {
            let above = |max: Option<u32>| max.map_or(0, |max| max.saturating_add(1));
            let max_node = summaries
                .iter()
                .filter_map(|summary| summary.max_node)
                .max();
            let max_edge = summaries
                .iter()
                .filter_map(|summary| summary.max_edge)
                .max();
            let (next_node, next_edge) = held
                .next_node
                .zip(held.next_edge)
                .ok_or_else(|| malformed(ArtifactKind::Segment, "the root has no counters"))?;
            view.next_node = Some(next_node.max(above(max_node)));
            view.next_edge = Some(next_edge.max(above(max_edge)));
        }
        Ok((view, complement))
    }
}

/// Whether two field maps hold the same keys with values of equal
/// canonical bytes.
fn same_fields(
    a: &HashMap<String, crate::value::Value>,
    b: &HashMap<String, crate::value::Value>,
) -> bool {
    a.len() == b.len()
        && a.iter().all(|(key, value)| {
            b.get(key)
                .is_some_and(|other| value.canonical_cmp(other).is_eq())
        })
}

/// Whether two node values are equal, a present value by canonical bytes.
fn same_presence(a: Option<&FieldPresence>, b: Option<&FieldPresence>) -> bool {
    match (a, b) {
        (Some(FieldPresence::Present(a)), Some(FieldPresence::Present(b))) => {
            a.canonical_cmp(b).is_eq()
        }
        _ => a == b,
    }
}

/// A caller's source of the segments of a tree family, one at a time.
pub type FetchTreeSegment<'f, 's, E> =
    dyn FnMut(SegmentId) -> Result<std::borrow::Cow<'s, TreeSegment>, E> + 'f;

/// Run the phases of [`TreeRestrict`] over a tree family read one segment
/// at a time.
///
/// The family's segments are those `ids` names, in id order, laid out
/// under `partition`. Each segment is read from `fetch` whenever a pass
/// reaches it, never two at once: the Export pass, the Summarize pass, the
/// segment owning a refusal, and the Emit pass, which gives `visit` each
/// segment's view and complement segment. Between passes the run holds the
/// exports and the summaries, which grow with the boundary and the
/// synthesized keys. The caller must supply a family the partition checks
/// accept and the same segment each time one is fetched; the plan's
/// locality is the caller's to check.
///
/// # Errors
///
/// [`super::StreamStop::Source`] with the first error of `fetch` or
/// `visit`; [`super::StreamStop::Refused`] with the refusal the checkpointed
/// run's Assemble returns.
pub fn tree_restrict_segmented_with<E>(
    plan: &RestrictPlan,
    ids: &[SegmentId],
    partition: &Partition,
    fetch: &mut FetchTreeSegment<'_, '_, E>,
    visit: &mut dyn FnMut(SegmentId, TreeSegment, TreeComplementSegment) -> Result<(), E>,
) -> Result<(), super::StreamStop<E>> {
    use super::StreamStop;
    let op = TreeRestrict { plan };
    let refused = |error: SegmentError| StreamStop::Refused(SegmentedRestrictError::from(error));
    let index = partition.owner_index().map_err(refused)?;
    let mut exports = Vec::with_capacity(ids.len());
    for &id in ids {
        let held = fetch(id).map_err(StreamStop::Source)?;
        exports.push(op.export_of(&held).map_err(refused)?);
    }
    let closure = Closure::of(&exports).map_err(refused)?;
    let mut summaries = Vec::with_capacity(ids.len());
    for &id in ids {
        let held = fetch(id).map_err(StreamStop::Source)?;
        let analysis = op
            .analyze(&held, partition, &index, &closure)
            .map_err(refused)?;
        summaries.push(TreeRestrict::summary_of(&analysis));
    }
    let mut source_error = None;
    let refusal = TreeRestrict::refusal(ids, &exports, &summaries, &mut |id| {
        let held = fetch(id).map_err(|error| {
            source_error = Some(error);
            PhaseStop::Store("the source failed".to_owned())
        })?;
        Ok(op.analyze(&held, partition, &index, &closure)?)
    });
    if let Some(error) = source_error {
        return Err(StreamStop::Source(error));
    }
    match refusal {
        Ok(None) => {}
        Ok(Some(error)) => return Err(StreamStop::Refused(error)),
        Err(PhaseStop::Failed(error)) => return Err(StreamStop::Refused(error)),
        Err(_) => {
            return Err(refused(malformed(
                ArtifactKind::Assembly,
                "the refusal could not be located",
            )));
        }
    }
    for &id in ids {
        let held = fetch(id).map_err(StreamStop::Source)?;
        let analysis = op
            .analyze(&held, partition, &index, &closure)
            .map_err(refused)?;
        let (view, complement) =
            TreeRestrict::emitted(&held, analysis, &summaries).map_err(refused)?;
        visit(id, view, complement).map_err(StreamStop::Source)?;
    }
    Ok(())
}

impl PhaseOperation for TreeRestrict<'_> {
    type Error = SegmentedRestrictError;

    fn op(&self) -> &'static [u8] {
        TREE_RESTRICT_OP
    }

    fn shape(&self) -> InstanceShapeKind {
        InstanceShapeKind::Tree
    }

    fn segment_op(&self) -> SegmentOp {
        SegmentOp::Restrict
    }

    fn locality(&self) -> Locality {
        graph_restrict_locality(self.plan)
    }

    fn portable(&self, phase: Phase) -> bool {
        phase != Phase::Seal
    }

    fn export(&self, cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Vec<u8>, Stop> {
        let held = Self::validated(cx, segment)?;
        let export = self.export_of(&held)?;
        Ok(encode(&export, ArtifactKind::Export)?)
    }

    fn summarize(&self, cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Vec<u8>, Stop> {
        let closure = Closure::of(&Self::exports(cx)?)?;
        let held = Self::validated(cx, segment)?;
        let analysis = self.analyze(&held, cx.partition(), cx.index(), &closure)?;
        Ok(encode(&Self::summary_of(&analysis), ArtifactKind::Summary)?)
    }

    fn assemble(&self, cx: &PhaseContext<'_>) -> Result<Vec<u8>, Stop> {
        let exports = Self::exports(cx)?;
        let summaries = Self::summaries(cx)?;
        let closure = Closure::of(&exports)?;
        let refusal = Self::refusal(&cx.segments(), &exports, &summaries, &mut |id| {
            let held = Self::validated(cx, id)?;
            Ok(self.analyze(&held, cx.partition(), cx.index(), &closure)?)
        })?;
        match refusal {
            None => Ok(Vec::new()),
            Some(error) => Err(PhaseStop::Failed(error)),
        }
    }

    fn emit(&self, cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Emission, Stop> {
        let closure = Closure::of(&Self::exports(cx)?)?;
        let summaries = Self::summaries(cx)?;
        let held = Self::validated(cx, segment)?;
        let analysis = self.analyze(&held, cx.partition(), cx.index(), &closure)?;
        let (view, complement) = Self::emitted(&held, analysis, &summaries)?;
        Ok(Emission {
            segment: AnySegment::Tree(view),
            complement: Some(ComplementSegment::Tree(complement)),
        })
    }

    fn output_role(&self, cx: &PhaseContext<'_>) -> ManifestRole {
        ManifestRole::View {
            source: cx.spec().input,
            envelope: IdentityEnvelope {
                version: IDENTITY_ENVELOPE_VERSION,
                source_schema: cx.input().schema,
                target_schema: cx.spec().output_schema,
                lens: cx.spec().lens,
                complement_format: ComplementFormat {
                    shape: ComplementShape::WType,
                    version: TREE_COMPLEMENT_FORMAT_VERSION,
                },
            },
        }
    }
}
