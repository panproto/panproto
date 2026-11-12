//! Applying a graph delta to a graph held as segments.
//!
//! A [`GraphDelta`] names every element it deletes, adds or changes by id.
//! Its read set ([`SegmentDelta::read_set`]) is the owners of the nodes it
//! names, the segments that may hold the edges it names
//! ([`FamilyIndex::holders`]), the owners of the endpoints of the edges it
//! deletes or adds, and, for an anchor or label change, the peers whose
//! boundary records copy the anchor or label.
//!
//! Each segment it reads reports its facts: the nodes it owns that the
//! delta names or touches, the edges it holds that the delta names, and
//! every edge incident to a node the delta deletes or touches (its own edges
//! with their payloads, its in-stubs with their ids, endpoints and labels).
//! These are exactly what [`GraphDelta::check`] reads of the whole graph.
//!
//! The ids a delta adds are the caller's. A node no range owns is refused
//! ([`SegmentError::Unowned`]) and an edge id the seal floor rule refuses
//! for the segment owning its source is refused
//! ([`SegmentError::EdgeIdOutsideAllocation`],
//! [`SegmentError::NoAllocationRange`]), as sealing the edited whole graph
//! under the partition would refuse them. [`FreshIds`](super::FreshIds)
//! hands out ids that neither refuses.
//!
//! The fan-domain check ([`GraphDelta::check`], check 7) is made at the
//! nodes whose fan conditions the delta can change: added nodes, nodes whose
//! metadata it changes, endpoints of edges it deletes or adds and sources of
//! edges whose label it changes. On an input in the fan domain these are the
//! only nodes that can leave it, so the check agrees with the whole-state
//! check.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_schema::{CanonicalWriter, InstanceShapeKind};

use super::apply::{FamilyIndex, SegmentDelta, SegmentedApplyError};
use super::body::{AnySegment, Boundary, Ghost, GraphSegment, InStub};
use super::bytes::{read_edges, read_nodes};
use super::error::SegmentError;
use super::ids::SegmentId;
use super::reader::Reader;
use super::store::ArtifactKind;
use crate::edit_error::EditError;
use crate::ginstance::{EdgeId, FanDomain, GEdge, GInstance};
use crate::graph_edit::{AttrKey, ElemRef, GraphDelta, Payload};
use crate::identity::Digest;
use crate::metadata::Node;
use crate::view_ref::ViewRef;
use crate::{write_edge_record, write_node_record};

/// The operation bytes of graph delta application in a run's identity
/// ([`RunJournal::run_digest`](super::RunJournal::run_digest)).
pub const GRAPH_APPLY_OP: &[u8] = b"graph apply";

impl SegmentDelta for GraphDelta {
    type Segment = GraphSegment;

    const SHAPE: InstanceShapeKind = InstanceShapeKind::Graph;

    const OP: &'static [u8] = GRAPH_APPLY_OP;

    fn digest(&self) -> Digest {
        Self::digest(self)
    }

    /// The owner of every node the delta deletes, adds or changes; for an
    /// anchor change also the peers of its owner's crossing edges in both
    /// directions, whose ghosts and in-stubs copy the anchor; for a deleted
    /// or added edge, the owners of the endpoints its payload names, and
    /// every segment that may hold its id ([`FamilyIndex::holders`]); for an
    /// edge whose label or position changes, every segment that may hold it,
    /// and for a label change the peers those segments' crossing edges
    /// enter, whose in-stubs copy the label. A node no range owns
    /// contributes no segment.
    fn read_set(&self, family: &FamilyIndex) -> BTreeSet<SegmentId> {
        let mut read = BTreeSet::new();
        let own = |read: &mut BTreeSet<SegmentId>, node: u32| {
            if let Some(owner) = family.owner(node) {
                read.insert(owner);
            }
        };
        for node in self.del_nodes().keys().chain(self.add_nodes().keys()) {
            own(&mut read, *node);
        }
        for (id, edge) in self.del_edges().iter().chain(self.add_edges()) {
            own(&mut read, edge.src);
            own(&mut read, edge.tgt);
            read.extend(family.holders(*id));
        }
        for key in self.attrs().keys() {
            match key {
                AttrKey::Anchor { node } => {
                    if let Some(owner) = family.owner(*node) {
                        read.insert(owner);
                        read.extend(family.peers_out(owner));
                        read.extend(family.peers_in(owner));
                    }
                }
                AttrKey::EdgeLabel { edge } => {
                    for holder in family.holders(*edge) {
                        read.insert(holder);
                        read.extend(family.peers_out(holder));
                    }
                }
                AttrKey::EdgePosition { edge } => read.extend(family.holders(*edge)),
                other => {
                    if let ElemRef::Node(node) = other.element() {
                        own(&mut read, node);
                    }
                }
            }
        }
        read
    }

    fn facts(&self, held: &GraphSegment) -> Vec<u8> {
        Facts::of(held, self).bytes()
    }

    fn check_facts(
        &self,
        facts: &[Vec<u8>],
        family: &FamilyIndex,
    ) -> Result<(), SegmentedApplyError> {
        check_glued(self, &decode_all(facts)?, family)
    }

    fn edit(
        &self,
        held: &GraphSegment,
        facts: &[Vec<u8>],
        family: &FamilyIndex,
    ) -> Result<GraphSegment, SegmentError> {
        let anchors = anchors_after(self, &decode_all(facts)?);
        edit_segment(held, self, family, &anchors)
    }

    fn id(segment: &GraphSegment) -> SegmentId {
        segment.segment
    }

    fn into_any(segment: GraphSegment) -> AnySegment {
        AnySegment::Graph(segment)
    }

    fn from_any(segment: AnySegment) -> Option<GraphSegment> {
        match segment {
            AnySegment::Graph(segment) => Some(segment),
            _ => None,
        }
    }
}

/// Every segment's facts, decoded.
fn decode_all(facts: &[Vec<u8>]) -> Result<Vec<Facts>, SegmentError> {
    facts.iter().map(|bytes| Facts::decode(bytes)).collect()
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
fn check_glued<'f>(
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
        let holder = family.owner(edge.src).ok_or(SegmentError::Unowned {
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
    let is_here = |node: u32| family.owner(node) == Some(here);
    let mut nodes = held.nodes.clone();
    let mut edges = held.edges.clone();
    for id in delta.del_nodes().keys() {
        nodes.remove(id);
    }
    for id in delta.del_edges().keys() {
        edges.remove(id);
    }
    for (id, node) in delta.add_nodes() {
        if is_here(*id) {
            nodes.insert(*id, node.clone());
        }
    }
    for (id, edge) in delta.add_edges() {
        if is_here(edge.src) {
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
    let owner_or_refuse =
        |node: u32, elem: ViewRef| family.owner(node).ok_or(SegmentError::Unowned { elem });
    let mut boundary = Boundary::default();
    for (id, edge) in &edges {
        let owner = owner_or_refuse(edge.tgt, ViewRef::Edge(*id))?;
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
        let src_owner = owner_or_refuse(edge.src, ViewRef::Edge(*id))?;
        if is_here(edge.tgt) && src_owner != here {
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
