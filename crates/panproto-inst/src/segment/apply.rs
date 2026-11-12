//! Applying a graph delta to a graph held as segments.
//!
//! A [`GraphDelta`] names every element it deletes, adds or changes by id,
//! so the segments it can change are known from the delta and the input
//! manifest alone: the owners of the nodes it names, the segments that may
//! hold the edges it names, the owners of the endpoints of the edges it
//! deletes or adds, and, for an anchor or label change, the peers whose
//! boundary records copy the anchor or label ([`FamilyIndex::read_set`]).
//! A segmented apply reads those segments and no other; every other segment
//! keeps its bytes and its manifest record.
//!
//! Each segment it reads reports its *facts*: the nodes it owns that the
//! delta names or touches, the edges it holds that the delta names, and
//! every edge incident to a node the delta deletes or touches (its own edges
//! with their payloads, its in-stubs with their ids, endpoints and labels).
//! These are exactly what [`GraphDelta::check`] reads of the whole graph, so
//! one coordinator glues them into a small graph and runs the whole-state
//! checks on it, in their order, returning the whole-state refusal before
//! any segment is rewritten. Each segment then rewrites itself: its own
//! nodes and edges as the delta changes them, and its boundary from its own
//! edges and in-stubs with the anchors after the delta, which the facts
//! carry for every node a new ghost or in-stub names.
//!
//! The ids a delta adds are the caller's. A node no range owns is refused
//! ([`SegmentError::Unowned`]) and an edge id the seal floor rule refuses
//! for the segment owning its source is refused
//! ([`SegmentError::EdgeIdOutsideAllocation`],
//! [`SegmentError::NoAllocationRange`]), as sealing the edited whole graph
//! under the partition would refuse them. [`FreshIds`] hands out ids that
//! neither refuses.
//!
//! The fan-domain check ([`GraphDelta::check`], check 7) is made at the
//! nodes whose fan conditions the delta can change: added nodes, nodes whose
//! metadata it changes, endpoints of edges it deletes or adds and sources of
//! edges whose label it changes. On an input in the fan domain these are the
//! only nodes that can leave it, so the check agrees with the whole-state
//! check.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_schema::CanonicalWriter;

use super::body::{AnySegment, Boundary, Ghost, GraphSegment, InStub};
use super::bytes::{read_edges, read_nodes};
use super::error::SegmentError;
use super::ids::{EdgeFloor, IdAllocator, IdKind, IdRange, SegmentId};
use super::manifest::{ManifestRole, SegmentManifest};
use super::partition::{OwnerIndex, Partition, SegmentSpec};
use super::reader::Reader;
use super::runner::{
    Budget, Emission, PhaseContext, PhaseOperation, PhaseStop, Progress, RunSpec, SegmentSource,
    SegmentedRun, run_segmented,
};
use super::store::{ArtifactKind, Phase, SegmentStore};
use crate::edit_error::EditError;
use crate::ginstance::{EdgeId, FanDomain, GEdge, GInstance};
use crate::graph_edit::{AttrKey, ElemRef, GraphDelta, Payload};
use crate::locality::{Locality, SegmentOp};
use crate::metadata::Node;
use crate::view_ref::ViewRef;
use crate::{write_edge_record, write_node_record};

/// The operation bytes of graph delta application in a run's identity
/// ([`RunJournal::run_digest`](super::RunJournal::run_digest)).
pub const GRAPH_APPLY_OP: &[u8] = b"graph apply";

/// Why a segmented graph delta application returned no output.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SegmentedApplyError {
    /// The family is invalid, or the edited graph cannot be laid out and
    /// sealed under the partition: an added node no range owns, or an added
    /// edge id the seal floor rule refuses.
    #[error(transparent)]
    Segment(SegmentError),
    /// Whole-state application refuses the delta with this error.
    #[error(transparent)]
    Edit(EditError),
}

impl From<SegmentError> for SegmentedApplyError {
    fn from(error: SegmentError) -> Self {
        Self::Segment(error)
    }
}

/// What the read set of a delta is computed from, for one family of graph
/// segments: the owner of each node id, the seal floor rule of the edge ids,
/// each segment's largest edge id and the peers of each segment's crossing
/// edges.
///
/// It is read off a manifest's records ([`FamilyIndex::of_manifest`]) or off
/// the segments themselves ([`FamilyIndex::of_segments`]), which give the
/// same index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FamilyIndex {
    index: OwnerIndex,
    floor: EdgeFloor,
    alloc_edges: Vec<(IdRange, SegmentId)>,
    max_edge: BTreeMap<SegmentId, u32>,
    cross_out: BTreeMap<SegmentId, BTreeSet<SegmentId>>,
    cross_in: BTreeMap<SegmentId, BTreeSet<SegmentId>>,
}

impl FamilyIndex {
    fn new(partition: &Partition) -> Result<Self, SegmentError> {
        let index = partition.owner_index()?;
        let floor = EdgeFloor::new(partition.specs().iter().map(|s| (s.id, s.alloc_edges)));
        let alloc_edges = partition
            .specs()
            .iter()
            .filter_map(|s| s.alloc_edges.map(|range| (range, s.id)))
            .collect();
        Ok(Self {
            index,
            floor,
            alloc_edges,
            max_edge: BTreeMap::new(),
            cross_out: BTreeMap::new(),
            cross_in: BTreeMap::new(),
        })
    }

    /// The index of the family `manifest` seals, from its records alone.
    ///
    /// # Errors
    ///
    /// The partition's error when the records' ranges are malformed.
    pub fn of_manifest(manifest: &SegmentManifest) -> Result<Self, SegmentError> {
        let mut family = Self::new(&manifest.partition()?)?;
        for record in &manifest.segments {
            if let Some(max) = record.max_edge {
                family.max_edge.insert(record.id, max);
            }
            family
                .cross_out
                .insert(record.id, record.cross_out.keys().copied().collect());
            family
                .cross_in
                .insert(record.id, record.cross_in.keys().copied().collect());
        }
        Ok(family)
    }

    /// The index of the family `segments` under `partition`.
    ///
    /// # Errors
    ///
    /// The partition's error when it is invalid.
    pub fn of_segments(
        segments: &[GraphSegment],
        partition: &Partition,
    ) -> Result<Self, SegmentError> {
        let mut family = Self::new(partition)?;
        for segment in segments {
            if let Some(max) = segment.edges.keys().next_back() {
                family.max_edge.insert(segment.segment, max.0);
            }
            family.cross_out.insert(
                segment.segment,
                segment.boundary.ghosts.values().map(|g| g.owner).collect(),
            );
            family.cross_in.insert(
                segment.segment,
                segment
                    .boundary
                    .in_stubs
                    .values()
                    .map(|s| s.src_owner)
                    .collect(),
            );
        }
        Ok(family)
    }

    /// The segment owning node `node`, `None` when no range contains it.
    #[must_use]
    pub fn owner(&self, node: u32) -> Option<SegmentId> {
        self.index.owner_of_node(node)
    }

    /// The segments that may hold edge id `edge`: under the seal floor rule
    /// an id at or above the floor is held only by the segment whose edge
    /// allocation range contains it, and an id below it by any segment whose
    /// largest edge id is at least it.
    #[must_use]
    pub fn holders(&self, edge: EdgeId) -> BTreeSet<SegmentId> {
        match self.floor.floor() {
            Some(floor) if edge.0 >= floor => self
                .alloc_edges
                .iter()
                .filter(|(range, _)| range.contains(edge.0))
                .map(|(_, segment)| *segment)
                .collect(),
            _ => self
                .max_edge
                .iter()
                .filter(|(_, max)| **max >= edge.0)
                .map(|(segment, _)| *segment)
                .collect(),
        }
    }

    /// The segments a segmented apply of `delta` reads, and the only ones it
    /// can rewrite:
    ///
    /// - the owner of every node the delta deletes, adds or changes;
    /// - for an anchor change, also the peers of its owner's crossing edges
    ///   in both directions, whose ghosts and in-stubs copy the anchor;
    /// - for a deleted or added edge, the owners of the endpoints its
    ///   payload names, and every segment that may hold its id
    ///   ([`Self::holders`]);
    /// - for an edge whose label or position changes, every segment that may
    ///   hold it, and for a label change the peers those segments' crossing
    ///   edges enter, whose in-stubs copy the label.
    ///
    /// A node no range owns contributes no segment.
    #[must_use]
    pub fn read_set(&self, delta: &GraphDelta) -> BTreeSet<SegmentId> {
        let mut read = BTreeSet::new();
        let own = |read: &mut BTreeSet<SegmentId>, node: u32| {
            if let Some(owner) = self.owner(node) {
                read.insert(owner);
            }
        };
        for node in delta.del_nodes().keys().chain(delta.add_nodes().keys()) {
            own(&mut read, *node);
        }
        for (id, edge) in delta.del_edges().iter().chain(delta.add_edges()) {
            own(&mut read, edge.src);
            own(&mut read, edge.tgt);
            read.extend(self.holders(*id));
        }
        for key in delta.attrs().keys() {
            match key {
                AttrKey::Anchor { node } => {
                    if let Some(owner) = self.owner(*node) {
                        read.insert(owner);
                        read.extend(peers(&self.cross_out, owner));
                        read.extend(peers(&self.cross_in, owner));
                    }
                }
                AttrKey::EdgeLabel { edge } => {
                    for holder in self.holders(*edge) {
                        read.insert(holder);
                        read.extend(peers(&self.cross_out, holder));
                    }
                }
                AttrKey::EdgePosition { edge } => read.extend(self.holders(*edge)),
                other => {
                    if let ElemRef::Node(node) = other.element() {
                        own(&mut read, node);
                    }
                }
            }
        }
        read
    }
}

/// The peers `map` records for `segment`.
fn peers(
    map: &BTreeMap<SegmentId, BTreeSet<SegmentId>>,
    segment: SegmentId,
) -> impl Iterator<Item = SegmentId> + '_ {
    map.get(&segment).into_iter().flatten().copied()
}

/// The nodes whose fan conditions `delta` can change, as `held` sees them:
/// added nodes, nodes whose metadata changes, the endpoints of deleted and
/// added edges, and the sources of the edges `held` holds whose label
/// changes.
fn touched(delta: &GraphDelta, held: Option<&GraphSegment>) -> BTreeSet<u32> {
    let mut nodes: BTreeSet<u32> = delta.add_nodes().keys().copied().collect();
    for edge in delta.del_edges().values().chain(delta.add_edges().values()) {
        nodes.insert(edge.src);
        nodes.insert(edge.tgt);
    }
    for key in delta.attrs().keys() {
        match key {
            AttrKey::Meta { node } => {
                nodes.insert(*node);
            }
            AttrKey::EdgeLabel { edge } => {
                if let Some(held) = held.and_then(|h| h.edges.get(edge)) {
                    nodes.insert(held.src);
                }
            }
            _ => {}
        }
    }
    nodes
}

/// What one segment reports to the coordinator about a delta.
///
/// `nodes` holds the nodes the segment owns that the delta deletes, changes
/// or touches ([`touched`], which covers the added nodes and the endpoints
/// of added edges); `edges` the edges it holds that the delta deletes, adds
/// or changes, and every edge it holds incident to a node it owns that the
/// delta deletes or touches; `stubs` every in-stub entering such a node, as
/// its id, source, target and label.
#[derive(Clone, Debug, Default)]
struct Facts {
    nodes: BTreeMap<u32, Node>,
    edges: BTreeMap<EdgeId, GEdge>,
    stubs: BTreeMap<EdgeId, (u32, u32, panproto_schema::Edge)>,
}

impl Facts {
    /// The facts of `held` about `delta`.
    fn of(held: &GraphSegment, delta: &GraphDelta) -> Self {
        let mut named: BTreeSet<u32> = touched(delta, Some(held));
        named.extend(delta.del_nodes().keys());
        let incident: BTreeSet<u32> = named
            .iter()
            .copied()
            .filter(|n| held.nodes.contains_key(n))
            .collect();
        named.extend(delta.attrs().keys().filter_map(|key| match key.element() {
            ElemRef::Node(node) => Some(node),
            _ => None,
        }));
        let named_edge = |id: &EdgeId| {
            delta.del_edges().contains_key(id)
                || delta.add_edges().contains_key(id)
                || delta
                    .attrs()
                    .keys()
                    .any(|key| key.element() == ElemRef::Edge(*id))
        };
        let nodes = held
            .nodes
            .iter()
            .filter(|(id, _)| named.contains(id))
            .map(|(id, node)| (*id, node.clone()))
            .collect();
        let edges = held
            .edges
            .iter()
            .filter(|(id, edge)| {
                named_edge(id) || incident.contains(&edge.src) || incident.contains(&edge.tgt)
            })
            .map(|(id, edge)| (*id, edge.clone()))
            .collect();
        let stubs = held
            .boundary
            .in_stubs
            .iter()
            .filter_map(|(elem, stub)| match elem {
                ViewRef::Edge(id) if incident.contains(&stub.tgt) => {
                    Some((*id, (stub.src, stub.tgt, stub.label.clone())))
                }
                _ => None,
            })
            .collect();
        Self {
            nodes,
            edges,
            stubs,
        }
    }

    /// The facts' canonical bytes: the nodes as node records, the edges as
    /// edge records, then the stubs as id, source, target and label, each
    /// part counted and ascending by id.
    fn bytes(&self) -> Vec<u8> {
        let mut sink = CanonicalWriter::new();
        sink.len(self.nodes.len());
        for node in self.nodes.values() {
            write_node_record(&mut sink, node);
        }
        sink.len(self.edges.len());
        for (id, edge) in &self.edges {
            write_edge_record(&mut sink, *id, edge);
        }
        sink.len(self.stubs.len());
        for (id, (src, tgt, label)) in &self.stubs {
            sink.u32(id.0);
            sink.u32(*src);
            sink.u32(*tgt);
            sink.edge(label);
        }
        sink.into_bytes()
    }

    fn decode(bytes: &[u8]) -> Result<Self, SegmentError> {
        let mut r = Reader::new(bytes, ArtifactKind::Summary);
        let nodes = read_nodes(&mut r)?;
        let edges = read_edges(&mut r)?;
        let n = r.count(16)?;
        let mut stubs = BTreeMap::new();
        for _ in 0..n {
            let id = EdgeId(r.u32()?);
            if stubs.keys().next_back().is_some_and(|last| *last >= id) {
                return Err(r.error("stubs are not strictly ascending"));
            }
            stubs.insert(id, (r.u32()?, r.u32()?, r.edge()?));
        }
        r.finish()?;
        Ok(Self {
            nodes,
            edges,
            stubs,
        })
    }
}

/// The graph the facts of the segments a delta reads describe: every
/// reported node, every reported edge (a held edge's payload over an
/// in-stub's copy), and a placeholder for every endpoint no fact reports.
fn facts_graph<'f>(facts: impl IntoIterator<Item = &'f Facts>) -> Result<GInstance, EditError> {
    let mut nodes: BTreeMap<u32, Node> = BTreeMap::new();
    let mut edges: BTreeMap<EdgeId, GEdge> = BTreeMap::new();
    let mut stubs: BTreeMap<EdgeId, GEdge> = BTreeMap::new();
    for fact in facts {
        nodes.extend(fact.nodes.iter().map(|(id, node)| (*id, node.clone())));
        edges.extend(fact.edges.iter().map(|(id, edge)| (*id, edge.clone())));
        stubs.extend(
            fact.stubs
                .iter()
                .map(|(id, (src, tgt, label))| (*id, GEdge::new(*src, *tgt, label.clone()))),
        );
    }
    for (id, stub) in stubs {
        edges.entry(id).or_insert(stub);
    }
    for edge in edges.values() {
        for end in [edge.src, edge.tgt] {
            nodes
                .entry(end)
                .or_insert_with(|| Node::new(end, Name::default()));
        }
    }
    let mut graph = GInstance::new();
    for node in nodes.into_values() {
        graph
            .insert_node(node)
            .map_err(|error| EditError::ApplyFailed(error.to_string()))?;
    }
    for (id, edge) in edges {
        graph
            .insert_edge(id, edge)
            .map_err(|error| EditError::ApplyFailed(error.to_string()))?;
    }
    Ok(graph)
}

/// The whole-state refusal of `delta` on the family whose read segments
/// reported `facts`, and then the refusals of laying the edited graph out
/// under the family's partition: the least added node no range owns, then
/// the least added edge id the seal floor rule refuses for the segment
/// owning its source.
fn check<'f>(
    delta: &GraphDelta,
    facts: impl IntoIterator<Item = &'f Facts> + Clone,
    family: &FamilyIndex,
) -> Result<(), SegmentedApplyError> {
    let graph = facts_graph(facts.clone()).map_err(SegmentedApplyError::Edit)?;
    delta
        .check_local(&graph)
        .map_err(SegmentedApplyError::Edit)?;
    let mut edited = graph;
    delta
        .mutate(&mut edited)
        .map_err(SegmentedApplyError::Edit)?;
    let mut nodes = touched(delta, None);
    for fact in facts {
        for key in delta.attrs().keys() {
            if let AttrKey::EdgeLabel { edge } = key
                && let Some(held) = fact.edges.get(edge)
            {
                nodes.insert(held.src);
            }
        }
    }
    for node in nodes {
        if let Some(reason) = FanDomain::violations_at(&edited, node, None)
            .into_iter()
            .next()
        {
            return Err(SegmentedApplyError::Edit(EditError::MalformedFan {
                node,
                reason,
            }));
        }
    }
    if let Some(node) = delta
        .add_nodes()
        .keys()
        .find(|node| family.owner(**node).is_none())
    {
        return Err(SegmentedApplyError::Segment(SegmentError::Unowned {
            elem: ViewRef::Node(*node),
        }));
    }
    for (id, edge) in delta.add_edges() {
        let holder = family
            .owner(edge.src)
            .ok_or_else(|| SegmentError::Unowned {
                elem: ViewRef::Edge(*id),
            })?;
        family.floor.check(holder, id.0)?;
    }
    Ok(())
}

/// The anchor of every node the facts report, as it is after `delta`.
fn anchors_after<'f>(
    delta: &GraphDelta,
    facts: impl IntoIterator<Item = &'f Facts>,
) -> BTreeMap<u32, Name> {
    let mut anchors: BTreeMap<u32, Name> = BTreeMap::new();
    for fact in facts {
        anchors.extend(fact.nodes.iter().map(|(id, n)| (*id, n.anchor.clone())));
    }
    for (key, change) in delta.attrs() {
        if let (AttrKey::Anchor { node }, crate::graph_edit::AttrVal::Anchor(anchor)) =
            (key, &change.new)
        {
            anchors.insert(*node, anchor.clone());
        }
    }
    anchors.extend(
        delta
            .add_nodes()
            .iter()
            .map(|(id, n)| (*id, n.anchor.clone())),
    );
    anchors
}

/// The segment `held` after `delta`, once the delta is checked: its own
/// nodes and edges as the delta deletes, adds and changes them, a ghost for
/// every remote target of its edges and an in-stub for every remote edge
/// entering it, with the anchors after the delta (`anchors`, then the
/// segment's own copies for the nodes the delta leaves alone).
fn edit_segment(
    held: &GraphSegment,
    delta: &GraphDelta,
    family: &FamilyIndex,
    anchors: &BTreeMap<u32, Name>,
) -> Result<GraphSegment, SegmentError> {
    let here = held.segment;
    let owned = |node: u32| family.owner(node) == Some(here);
    let mut nodes = held.nodes.clone();
    let mut edges = held.edges.clone();
    for id in delta.del_nodes().keys() {
        nodes.remove(id);
    }
    for id in delta.del_edges().keys() {
        edges.remove(id);
    }
    for (id, node) in delta.add_nodes() {
        if owned(*id) {
            nodes.insert(*id, node.clone());
        }
    }
    for (id, edge) in delta.add_edges() {
        if owned(edge.src) {
            edges.insert(*id, edge.clone());
        }
    }
    for (key, change) in delta.attrs() {
        match key.element() {
            ElemRef::Node(node) => {
                if let Some(node) = nodes.get_mut(&node) {
                    node.set(key, &change.new);
                }
            }
            ElemRef::Edge(edge) => {
                if let Some(edge) = edges.get_mut(&edge) {
                    edge.set(key, &change.new);
                }
            }
            _ => {}
        }
    }
    let anchor = |node: u32| {
        anchors
            .get(&node)
            .or_else(|| held.boundary.ghosts.get(&node).map(|g| &g.anchor))
            .or_else(|| {
                held.boundary
                    .in_stubs
                    .values()
                    .find(|stub| stub.src == node)
                    .map(|stub| &stub.src_anchor)
            })
            .cloned()
            .unwrap_or_default()
    };
    let owner_of =
        |node: u32, elem: ViewRef| family.owner(node).ok_or(SegmentError::Unowned { elem });
    let mut boundary = Boundary::default();
    for (id, edge) in &edges {
        let owner = owner_of(edge.tgt, ViewRef::Edge(*id))?;
        if owner != here {
            boundary.ghosts.entry(edge.tgt).or_insert_with(|| Ghost {
                node: edge.tgt,
                owner,
                anchor: anchor(edge.tgt),
            });
        }
    }
    for (elem, stub) in &held.boundary.in_stubs {
        let ViewRef::Edge(id) = elem else {
            return Err(SegmentError::InStubMismatch { elem: elem.clone() });
        };
        if delta.del_edges().contains_key(id) {
            continue;
        }
        let mut stub = stub.clone();
        if let Some(change) = delta.attrs().get(&AttrKey::EdgeLabel { edge: *id })
            && let crate::graph_edit::AttrVal::Label(label) = &change.new
        {
            stub.label = label.clone();
        }
        stub.src_anchor = anchor(stub.src);
        boundary.in_stubs.insert(elem.clone(), stub);
    }
    for (id, edge) in delta.add_edges() {
        let src_owner = owner_of(edge.src, ViewRef::Edge(*id))?;
        if owned(edge.tgt) && src_owner != here {
            boundary.in_stubs.insert(
                ViewRef::Edge(*id),
                InStub {
                    src: edge.src,
                    src_owner,
                    src_anchor: anchor(edge.src),
                    tgt: edge.tgt,
                    label: edge.edge.clone(),
                },
            );
        }
    }
    Ok(GraphSegment {
        segment: here,
        nodes,
        edges,
        boundary,
    })
}

/// Apply `delta` to the graph held as `segments` under `partition`, reading
/// only the segments it can change.
///
/// The segments are checked as [`verify_partition`] checks them. The
/// segments of [`FamilyIndex::read_set`] report their facts, the
/// coordinator runs the whole-state checks of [`GraphDelta::check`] on them,
/// and each of those segments is rewritten; every other segment is returned
/// unchanged. Gluing the output under `partition` gives
/// [`GraphDelta::apply`] of the glued input, and the output is the split of
/// that graph under `partition` ([`partition_graph`]).
///
/// [`verify_partition`]: super::verify_partition
/// [`partition_graph`]: super::partition_graph
///
/// # Errors
///
/// [`SegmentedApplyError::Segment`] with the error of `verify_partition`;
/// then [`SegmentedApplyError::Edit`] with the refusal of
/// [`GraphDelta::apply`] on the glued graph, the fan-domain check made at
/// the nodes the delta touches; then [`SegmentedApplyError::Segment`] with
/// [`SegmentError::Unowned`] for the least added node no range owns, and
/// with the seal floor rule's refusal ([`EdgeFloor::check`]) for the least
/// added edge id it refuses for the owner of the edge's source.
pub fn graph_apply_segmented(
    delta: &GraphDelta,
    segments: &[GraphSegment],
    partition: &Partition,
) -> Result<Vec<GraphSegment>, SegmentedApplyError> {
    let any: Vec<AnySegment> = segments.iter().cloned().map(AnySegment::Graph).collect();
    super::check::verify_partition(&any, partition)?;
    let family = FamilyIndex::of_segments(segments, partition)?;
    let read = family.read_set(delta);
    let held: BTreeMap<SegmentId, &GraphSegment> =
        segments.iter().map(|s| (s.segment, s)).collect();
    let facts: Vec<Facts> = read
        .iter()
        .filter_map(|id| held.get(id))
        .map(|segment| Facts::of(segment, delta))
        .collect();
    check(delta, &facts, &family)?;
    let anchors = anchors_after(delta, &facts);
    let mut output = Vec::with_capacity(segments.len());
    for segment in partition.specs().iter().filter_map(|s| held.get(&s.id)) {
        output.push(if read.contains(&segment.segment) {
            edit_segment(segment, delta, &family, &anchors)?
        } else {
            (*segment).clone()
        });
    }
    Ok(output)
}

/// Fresh ids for the nodes and edges a delta adds to one segment, drawn from
/// the allocation ranges its partition assigns it.
///
/// A node id is drawn from the segment's node allocation range above every
/// node the segment owns in that range, so the segment owns it and holds no
/// node under it; an edge id from its edge allocation range above every edge
/// id the segment holds in that range, so no segment holds it and the seal
/// floor rule admits it for an edge whose source the segment owns. A delta
/// whose added nodes take node ids from the owner's allocator and whose added
/// edges take edge ids from the allocator of their source's owner is refused
/// by [`graph_apply_segmented`] for no id it adds.
///
/// ```
/// use std::collections::BTreeMap;
///
/// use panproto_inst::graph_edit::GraphDelta;
/// use panproto_inst::segment::{
///     Boundary, FreshIds, GraphSegment, IdRange, SegmentId, SegmentSpec,
/// };
/// use panproto_inst::{EdgeId, GEdge, Node};
/// use panproto_schema::Edge;
///
/// let spec = SegmentSpec {
///     id: SegmentId(0),
///     nodes: vec![IdRange { lo: 0, hi: 99 }],
///     alloc_nodes: Some(IdRange { lo: 50, hi: 99 }),
///     alloc_edges: Some(IdRange { lo: 1_000, hi: 1_999 }),
/// };
/// let segment = GraphSegment {
///     segment: SegmentId(0),
///     nodes: BTreeMap::from([(50, Node::new(50, "person"))]),
///     edges: BTreeMap::new(),
///     boundary: Boundary::default(),
/// };
/// let mut fresh = FreshIds::of(&segment, &spec)?;
/// let node = fresh.node()?;
/// let edge = fresh.edge()?;
/// assert_eq!((node, edge), (51, EdgeId(1_000)));
/// let knows = Edge {
///     src: "person".into(),
///     tgt: "person".into(),
///     kind: "knows".into(),
///     name: None,
/// };
/// let delta = GraphDelta::identity()
///     .with_added_node(Node::new(node, "person"))
///     .with_added_edge(edge, GEdge::new(50, node, knows));
/// assert_eq!(delta.add_nodes().len(), 1);
/// # Ok::<(), panproto_inst::segment::SegmentError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FreshIds {
    segment: SegmentId,
    nodes: Option<IdAllocator>,
    edges: Option<IdAllocator>,
}

impl FreshIds {
    /// The fresh ids of `segment`, held under `spec`.
    ///
    /// # Errors
    ///
    /// [`SegmentError::InvertedRange`] when an allocation range of `spec`
    /// is inverted.
    pub fn of(segment: &GraphSegment, spec: &SegmentSpec) -> Result<Self, SegmentError> {
        let nodes = spec
            .alloc_nodes
            .map(|range| range.check(spec.id))
            .transpose()?
            .map(|range| IdAllocator::from_source(range, segment.nodes.keys().copied()));
        let edges = spec
            .alloc_edges
            .map(|range| range.check(spec.id))
            .transpose()?
            .map(|range| IdAllocator::from_source(range, segment.edges.keys().map(|e| e.0)));
        Ok(Self {
            segment: spec.id,
            nodes,
            edges,
        })
    }

    /// A fresh node id.
    ///
    /// # Errors
    ///
    /// [`SegmentError::NoAllocationRange`] when the segment has no node
    /// allocation range and [`SegmentError::AllocationExhausted`] when no id
    /// of it is left.
    pub fn node(&mut self) -> Result<u32, SegmentError> {
        draw(self.segment, IdKind::Node, self.nodes.as_mut())
    }

    /// A fresh edge id.
    ///
    /// # Errors
    ///
    /// [`SegmentError::NoAllocationRange`] when the segment has no edge
    /// allocation range and [`SegmentError::AllocationExhausted`] when no id
    /// of it is left.
    pub fn edge(&mut self) -> Result<EdgeId, SegmentError> {
        draw(self.segment, IdKind::Edge, self.edges.as_mut()).map(EdgeId)
    }
}

fn draw(
    segment: SegmentId,
    kind: IdKind,
    allocator: Option<&mut IdAllocator>,
) -> Result<u32, SegmentError> {
    allocator
        .ok_or(SegmentError::NoAllocationRange { segment, kind })?
        .allocate()
        .map_err(|exhausted| exhausted.in_segment(segment, kind))
}

type ApplyStop = PhaseStop<SegmentedApplyError>;

/// Graph delta application as a [`PhaseOperation`] over the segments the
/// delta can change ([`FamilyIndex::read_set`] of the input manifest, the
/// operation's affected set).
///
/// - Validate keeps each read segment's canonical bytes, checked against the
///   input manifest.
/// - Export writes nothing.
/// - Summarize writes the segment's facts: the nodes, edges and in-stubs the
///   whole-state checks read of it.
/// - Assemble (the coordinator) glues the facts and returns the whole-state
///   refusal, then the refusal of laying the result out under the
///   partition; otherwise it writes nothing. It holds the facts, never a
///   segment.
/// - Emit rewrites the segment, reading the facts for the anchors of the
///   nodes its new ghosts and in-stubs name, without a complement.
/// - Seal reseals the input manifest with the rewritten records as a
///   source, keeping every other record.
pub struct GraphApply<'d> {
    /// The delta.
    pub delta: &'d GraphDelta,
}

impl GraphApply<'_> {
    fn validated(cx: &PhaseContext<'_>, segment: SegmentId) -> Result<GraphSegment, ApplyStop> {
        let bytes = cx.artifact(Phase::Validate, Some(segment))?;
        match AnySegment::from_canonical_bytes(&bytes, cx.partition())?.0 {
            AnySegment::Graph(segment) => Ok(segment),
            _ => Err(ApplyStop::from(SegmentError::Decode {
                artifact: ArtifactKind::Segment,
                message: "not a graph segment".to_owned(),
            })),
        }
    }

    fn facts(cx: &PhaseContext<'_>) -> Result<Vec<Facts>, ApplyStop> {
        let mut facts = Vec::new();
        for segment in cx.segments() {
            facts.push(Facts::decode(
                &cx.artifact(Phase::Summarize, Some(segment))?,
            )?);
        }
        Ok(facts)
    }
}

impl PhaseOperation for GraphApply<'_> {
    type Error = SegmentedApplyError;

    fn op(&self) -> &'static [u8] {
        GRAPH_APPLY_OP
    }

    fn segment_op(&self) -> SegmentOp {
        SegmentOp::Apply
    }

    fn locality(&self) -> Locality {
        Locality::Exchange
    }

    fn affected(&self, cx: &PhaseContext<'_>) -> Result<Option<BTreeSet<SegmentId>>, SegmentError> {
        Ok(Some(
            FamilyIndex::of_manifest(cx.input())?.read_set(self.delta),
        ))
    }

    fn export(&self, _: &PhaseContext<'_>, _: SegmentId) -> Result<Vec<u8>, ApplyStop> {
        Ok(Vec::new())
    }

    fn summarize(&self, cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Vec<u8>, ApplyStop> {
        Ok(Facts::of(&Self::validated(cx, segment)?, self.delta).bytes())
    }

    fn assemble(&self, cx: &PhaseContext<'_>) -> Result<Vec<u8>, ApplyStop> {
        let family = FamilyIndex::of_manifest(cx.input())?;
        let facts = Self::facts(cx)?;
        check(self.delta, &facts, &family).map_err(ApplyStop::Failed)?;
        Ok(Vec::new())
    }

    fn emit(&self, cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Emission, ApplyStop> {
        let family = FamilyIndex::of_manifest(cx.input())?;
        let anchors = anchors_after(self.delta, &Self::facts(cx)?);
        let held = Self::validated(cx, segment)?;
        Ok(Emission {
            segment: edit_segment(&held, self.delta, &family, &anchors)?,
            complement: None,
        })
    }

    fn output_role(&self, _: &PhaseContext<'_>) -> ManifestRole {
        ManifestRole::Source
    }
}

/// The outcome of [`graph_apply_run`].
pub type GraphApplyRun = SegmentedRun<SegmentedApplyError>;

/// Apply `delta` to the graph the input manifest `input` seals as a
/// checkpointed run.
///
/// The run is [`run_segmented`] of [`GraphApply`], reading its segments from
/// `source` and committing to `store`, with `spec.lens` replaced by the
/// delta's digest ([`GraphDelta::digest`]), so the run's identity names the
/// delta, and `spec.output_schema` by the input's schema. It reads only the
/// segments of [`FamilyIndex::read_set`], `4k + 2` tasks for `k` of them, and
/// a segment it does not read keeps its record in the output manifest. A
/// refused delta ends at Assemble with the whole-state refusal, before any
/// Emit task, so it seals nothing and every segment is left as it was. A
/// complete run emits each rewritten segment ([`read_emitted`]); gluing them
/// with the unread input segments gives [`GraphDelta::apply`] of the glued
/// input, and the output manifest is the seal of that graph's split under
/// the input's partition as a source, without an instance digest.
///
/// [`read_emitted`]: super::read_emitted
pub fn graph_apply_run<Src: SegmentSource, St: SegmentStore>(
    delta: &GraphDelta,
    input: &SegmentManifest,
    source: &Src,
    spec: RunSpec,
    budget: Budget,
    store: &mut St,
    progress: &mut dyn FnMut(&Progress),
) -> GraphApplyRun {
    run_segmented(
        &GraphApply { delta },
        input,
        source,
        RunSpec {
            lens: delta.digest(),
            output_schema: input.schema,
            ..spec
        },
        budget,
        store,
        progress,
    )
}
