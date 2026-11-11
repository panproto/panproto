//! Segmented graph restriction: the graph restrict engine run segment by
//! segment.
//!
//! [`graph_restrict_segmented`] runs [`graph_restrict_with_plan`] over a
//! graph held as the segments of a [`Partition`], never assembling the
//! graph. Each segment's run holds the segment's own nodes and edges, a
//! placeholder for every node its edges and in-stubs name, and the facts it
//! needs about those placeholders, and decides only its own elements. The
//! stages of the engine read three kinds of fact about nodes owned
//! elsewhere:
//!
//! - stage 1 reads whether the target of an owned edge survives, and its
//!   anchors; one round of boundary exchange carries these from each
//!   in-stub's target to the segment owning its source;
//! - stage 2 reads which surviving nodes a search through dropped nodes
//!   reaches after it leaves the segment, which may be any number of
//!   segments away. Each segment summarizes, for every dropped node an
//!   in-stub enters, the survivors it reaches locally and the dropped ghosts
//!   it leaves through; the coordinator closes these summaries over the
//!   boundary, as the partial evaluation and assembly of distributed
//!   reachability queries does (fanwangwu2012performance, Sec 3, stated:
//!   ghosts are its virtual nodes, in-stub targets its in-nodes), and
//!   returns to each segment the survivors each of its dropped ghosts
//!   reaches;
//! - stage 3 reads whether the targets of an owned fan's edges survive, and
//!   their anchors, from the same exchange as stage 1.
//!
//! Synthesized elements are owned by the segment owning their chain (the
//! first node of a contracted pair, the source of an expanded edge) and take
//! ids from that segment's allocation ranges, so the view segments are laid
//! out under the same partition as the source. [`SegmentedRestrict::assemble`]
//! glues them and numbers the synthesized elements as whole-state restrict
//! does, which gives whole-state restrict's view and complement exactly.
//!
//! [`graph_restrict_with_plan`]: crate::graph_restrict_with_plan

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use panproto_gat::Name;
use panproto_schema::CanonicalWriter;
use serde::{Deserialize, Serialize};

use super::body::{Boundary, Ghost, GraphSegment, InStub};
use super::error::{Residual, SegmentError};
use super::ids::{IdAllocator, IdKind, SegmentId};
use super::partition::{OwnerIndex, Partition};
use super::split::assemble_graph;
use crate::error::RestrictError;
use crate::ginstance::{
    EdgeId, FanDomain, GEdge, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND, write_edge_record,
    write_node_record,
};
use crate::graph_complement::GraphComplement;
use crate::graph_restrict::{Outside, RemoteFate, RunParts, decide_nodes, run_scoped};
use crate::identity::{CanonicalSink, Digest, SEGMENT_COMPLEMENT_CONTEXT, SEGMENT_FORMAT_VERSION};
use crate::locality::{GlobalReason, Locality, SegmentOp, SummaryKind};
use crate::metadata::Node;
use crate::restrict_plan::RestrictPlan;
use crate::value::{FieldPresence, Value};
use crate::view_ref::{IdFloor, SynthKey, ViewRef};
use crate::wtype::FieldTransform;

/// The complement of one segment of a segmented graph restrict: the entries
/// of the whole complement about the elements the segment owns.
///
/// A source node's entries belong to its owner, a source edge's to the
/// owner of its source, and a synthesized element's to the segment owning
/// its chain, which allocated its id. `floors` replaces the whole
/// complement's id floor: for each kind, the floor of the segment's
/// allocation range after the run (one above the greatest id the segment
/// holds or allocated in the range, and at least the range's lower bound),
/// or 0 when the partition declares no range of that kind.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphComplementSegment {
    /// The segment.
    pub segment: SegmentId,
    /// The dropped source nodes it owns.
    pub dropped_nodes: BTreeMap<u32, Node>,
    /// The dropped source edges whose source it owns.
    pub dropped_edges: BTreeMap<EdgeId, GEdge>,
    /// The source form of each kept edge it owns whose label the view does
    /// not determine.
    pub original_edges: BTreeMap<EdgeId, GEdge>,
    /// The source anchor of each kept node it owns that the view does not
    /// determine.
    pub original_anchors: BTreeMap<u32, Name>,
    /// The source fields of each kept node it owns whose fields changed.
    pub original_extra_fields: BTreeMap<u32, BTreeMap<String, Value>>,
    /// The source value of each kept node it owns whose value changed.
    pub original_values: BTreeMap<u32, Option<FieldPresence>>,
    /// The synthesized nodes it owns, by id.
    pub synthesized_nodes: BTreeMap<u32, SynthKey>,
    /// The synthesized edges it owns, by id.
    pub synthesized_edges: BTreeMap<EdgeId, SynthKey>,
    /// The floors of its allocation ranges after the run.
    pub floors: IdFloor,
}

impl GraphComplementSegment {
    /// The number of entries the complement segment retains: its dropped
    /// nodes and edges, its recorded original edges, anchors, fields and
    /// values, and its synthesized nodes and edges.
    #[must_use]
    pub fn element_count(&self) -> u64 {
        let count = self.dropped_nodes.len()
            + self.dropped_edges.len()
            + self.original_edges.len()
            + self.original_anchors.len()
            + self.original_extra_fields.len()
            + self.original_values.len()
            + self.synthesized_nodes.len()
            + self.synthesized_edges.len();
        u64::try_from(count).unwrap_or(u64::MAX)
    }

    /// The complement segment's canonical bytes.
    ///
    /// ```text
    /// version: u16 (SEGMENT_FORMAT_VERSION), shape: u8 (2, graph)
    /// dropped nodes:    count, node records            (ascending id)
    /// dropped edges:    count, edge records            (ascending id)
    /// original edges:   count, edge records            (ascending id)
    /// original anchors: count, (node, anchor)          (ascending node)
    /// original fields:  count, (node, field map)       (ascending node)
    /// original values:  count, (node, value presence)  (ascending node)
    /// synthesized nodes and then edges: count, (id, key) (ascending id)
    /// floors:           node floor: u32, edge floor: u32
    /// ```
    ///
    /// A node or edge record is the one the segment bytes use. A value
    /// presence is a tag (`0` none, `1` present and the value, `2` null,
    /// `3` absent). A key is a tag and its fields: `0` a contracted pair
    /// (source, target, edge label), `1` an expanded edge (edge id, hop as
    /// `u16`), `2` an expanded contraction (source, target, hop). The segment
    /// id is not written: the manifest the complement is filed under names
    /// it.
    ///
    /// # Errors
    ///
    /// [`SegmentError::NodeIdMismatch`] for a dropped node stored under a
    /// key other than its id, and [`SegmentError::Decode`] for a key of a
    /// resolution, which graph restrict never synthesizes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, SegmentError> {
        let mut sink = CanonicalWriter::new();
        sink.write(&SEGMENT_FORMAT_VERSION.to_le_bytes());
        sink.tag(2);
        sink.len(self.dropped_nodes.len());
        for (key, node) in &self.dropped_nodes {
            if *key != node.id {
                return Err(SegmentError::NodeIdMismatch {
                    key: *key,
                    found: node.id,
                });
            }
            write_node_record(&mut sink, node);
        }
        for edges in [&self.dropped_edges, &self.original_edges] {
            sink.len(edges.len());
            for (id, edge) in edges {
                write_edge_record(&mut sink, *id, edge);
            }
        }
        sink.len(self.original_anchors.len());
        for (id, anchor) in &self.original_anchors {
            sink.u32(*id);
            sink.name(anchor);
        }
        sink.len(self.original_extra_fields.len());
        for (id, fields) in &self.original_extra_fields {
            sink.u32(*id);
            sink.len(fields.len());
            for (key, value) in fields {
                sink.str(key);
                sink.value(value);
            }
        }
        sink.len(self.original_values.len());
        for (id, value) in &self.original_values {
            sink.u32(*id);
            match value {
                None => sink.tag(0),
                Some(FieldPresence::Present(value)) => {
                    sink.tag(1);
                    sink.value(value);
                }
                Some(FieldPresence::Null) => sink.tag(2),
                Some(FieldPresence::Absent) => sink.tag(3),
            }
        }
        sink.len(self.synthesized_nodes.len());
        for (id, key) in &self.synthesized_nodes {
            sink.u32(*id);
            write_synth_key(&mut sink, key)?;
        }
        sink.len(self.synthesized_edges.len());
        for (id, key) in &self.synthesized_edges {
            sink.u32(id.0);
            write_synth_key(&mut sink, key)?;
        }
        sink.u32(self.floors.node);
        sink.u32(self.floors.edge);
        Ok(sink.into_bytes())
    }

    /// The complement segment's digest: its canonical bytes under
    /// [`SEGMENT_COMPLEMENT_CONTEXT`].
    ///
    /// # Errors
    ///
    /// As [`GraphComplementSegment::canonical_bytes`].
    pub fn digest(&self) -> Result<Digest, SegmentError> {
        Ok(Digest::derive(
            SEGMENT_COMPLEMENT_CONTEXT,
            &self.canonical_bytes()?,
        ))
    }
}

/// Write a graph restrict's synthesized key: a tag and its fields.
fn write_synth_key(sink: &mut CanonicalWriter, key: &SynthKey) -> Result<(), SegmentError> {
    match key {
        SynthKey::Contract { src, tgt, edge } => {
            sink.tag(0);
            sink.u32(*src);
            sink.u32(*tgt);
            sink.edge(edge);
        }
        SynthKey::Expand { source_edge, hop } => {
            sink.tag(1);
            sink.u32(source_edge.0);
            sink.write(&hop.to_le_bytes());
        }
        SynthKey::ContractExpand { src, tgt, hop } => {
            sink.tag(2);
            sink.u32(*src);
            sink.u32(*tgt);
            sink.write(&hop.to_le_bytes());
        }
        SynthKey::ResolutionReference { .. } | SynthKey::ResolutionSpan { .. } => {
            return Err(SegmentError::Decode {
                artifact: super::store::ArtifactKind::ComplementSegment,
                message: "a resolution key has no graph complement segment encoding".to_owned(),
            });
        }
    }
    Ok(())
}

/// The output of a segmented graph restrict: one view segment and one
/// complement segment per segment of the partition, in id order.
#[derive(Clone, Debug)]
pub struct SegmentedRestrict {
    /// The view, laid out under the source's partition.
    pub views: Vec<GraphSegment>,
    /// The complement, segment by segment.
    pub complements: Vec<GraphComplementSegment>,
}

/// Why a segmented graph restrict returned no output.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SegmentedRestrictError {
    /// The partition or the segments are invalid, a segment that must
    /// allocate has no allocation range or exhausts it, or an edge
    /// allocation range does not lie above every edge id in use.
    #[error(transparent)]
    Segment(SegmentError),
    /// Whole-state restrict refuses the graph the segments hold, with this
    /// error, which arose in `segment`.
    #[error("restrict refused in {segment}: {error}")]
    Restrict {
        /// The segment owning the element the refusal is about.
        segment: SegmentId,
        /// The refusal whole-state restrict returns.
        error: RestrictError,
    },
    /// The migration's restrict is not supported segment by segment.
    #[error("{0}")]
    Residual(Residual),
}

/// The locality class of graph restrict along `plan`.
///
/// [`Locality::Global`] with [`GlobalReason::ExpressionDemand`] at the least
/// source vertex carrying an expression (a survival predicate, a term
/// assignment, or a field transform other than renaming, dropping, adding or
/// keeping fields); otherwise [`Locality::Summary`] with
/// [`SummaryKind::Reach`] when some source vertex does not survive, since a
/// contraction search may then cross any number of segments, and
/// [`Locality::Exchange`] when every source vertex survives.
#[must_use]
pub fn graph_restrict_locality(plan: &RestrictPlan) -> Locality {
    let migration = plan.migration();
    let local = |transform: &FieldTransform| {
        matches!(
            transform,
            FieldTransform::RenameField { .. }
                | FieldTransform::DropField { .. }
                | FieldTransform::AddField { .. }
                | FieldTransform::KeepFields { .. }
        )
    };
    let expression = migration
        .conditional_survival
        .keys()
        .chain(migration.op_term_assignments.keys())
        .chain(
            migration
                .field_transforms
                .iter()
                .filter(|(_, transforms)| !transforms.iter().all(local))
                .map(|(vertex, _)| vertex),
        )
        .min();
    if let Some(vertex) = expression {
        return Locality::Global(GlobalReason::ExpressionDemand {
            vertex: vertex.clone(),
        });
    }
    if plan
        .source()
        .vertices
        .keys()
        .any(|vertex| !plan.anchor_survives(vertex))
    {
        Locality::Summary(SummaryKind::Reach)
    } else {
        Locality::Exchange
    }
}

/// One segment's graph as its run holds it: the owned nodes and edges, a
/// placeholder for each ghost and each in-stub source, and each in-stub as
/// an edge from its source's placeholder.
pub(super) struct Local {
    pub(super) graph: GInstance,
    pub(super) foreign: BTreeSet<u32>,
}

impl Local {
    pub(super) fn of(segment: &GraphSegment) -> Result<Self, SegmentError> {
        let mut graph = GInstance::new();
        let mut foreign = BTreeSet::new();
        for node in segment.nodes.values() {
            graph = graph.with_node(node.clone());
        }
        let mut placeholder = |graph: &mut GInstance, id: u32, anchor: &Name| {
            if graph.node(id).is_none() {
                foreign.insert(id);
                *graph = std::mem::take(graph).with_node(Node::new(id, anchor.clone()));
            }
        };
        for ghost in segment.boundary.ghosts.values() {
            placeholder(&mut graph, ghost.node, &ghost.anchor);
        }
        for stub in segment.boundary.in_stubs.values() {
            placeholder(&mut graph, stub.src, &stub.src_anchor);
        }
        for (id, edge) in &segment.edges {
            graph
                .insert_edge(*id, edge.clone())
                .map_err(|_| dangling(ViewRef::Edge(*id), edge.tgt))?;
        }
        for (elem, stub) in &segment.boundary.in_stubs {
            let ViewRef::Edge(id) = elem else {
                return Err(SegmentError::InStubMismatch { elem: elem.clone() });
            };
            graph
                .insert_edge(*id, GEdge::new(stub.src, stub.tgt, stub.label.clone()))
                .map_err(|_| dangling(elem.clone(), stub.tgt))?;
        }
        Ok(Self { graph, foreign })
    }

    /// The first fan-domain violation among the owned nodes, ascending.
    pub(super) fn fan_violation(&self) -> Option<(u32, crate::FanViolation)> {
        let mut ids: Vec<u32> = self
            .graph
            .nodes()
            .keys()
            .copied()
            .filter(|id| !self.foreign.contains(id))
            .collect();
        ids.sort_unstable();
        ids.into_iter().find_map(|id| {
            FanDomain::violations_at(&self.graph, id, None)
                .into_iter()
                .next()
                .map(|reason| (id, reason))
        })
    }
}

/// The error for an element naming a node its segment does not hold.
const fn dangling(elem: ViewRef, node: u32) -> SegmentError {
    SegmentError::DanglingEndpoint { elem, node }
}

/// The segment error for an edge assembly could not place: an endpoint it
/// names is missing, or its id is taken.
const fn placed(error: &crate::error::InstError, id: EdgeId) -> SegmentError {
    match error {
        crate::error::InstError::DanglingEdge { endpoint, .. } => {
            dangling(ViewRef::Edge(id), *endpoint)
        }
        _ => SegmentError::DuplicateEdgeId {
            edge: id,
            segments: Vec::new(),
        },
    }
}

/// Whether an edge has a reserved fan kind.
fn is_fan_edge(edge: &GEdge) -> bool {
    edge.edge.kind == HYPER_PARENT_KIND || edge.edge.kind == HYPER_LABEL_KIND
}

/// The reach summary of one segment: for every dropped plain node it owns
/// that a non-fan in-stub enters, the surviving nodes a search from it
/// reaches without leaving the segment, and the dropped ghosts it leaves
/// through.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct ReachSummary {
    /// Each entry's survivors reached and dropped ghosts left through.
    pub(super) entries: BTreeMap<u32, (BTreeSet<u32>, BTreeSet<u32>)>,
    /// The fate of every surviving node the summary names.
    pub(super) kept: BTreeMap<u32, RemoteFate>,
}

/// The fate of `id` as `segment` sees it: its own decision or its import.
fn fate_in<'f>(
    own: &'f BTreeMap<u32, RemoteFate>,
    imports: &'f BTreeMap<u32, RemoteFate>,
    id: u32,
) -> Option<&'f RemoteFate> {
    own.get(&id).or_else(|| imports.get(&id))
}

/// The search of a dropped node of `local` through dropped plain nodes the
/// segment owns: the survivors reached, and the dropped ghosts left
/// through.
fn local_reach(
    local: &Local,
    own: &BTreeMap<u32, RemoteFate>,
    imports: &BTreeMap<u32, RemoteFate>,
    start: u32,
) -> (BTreeSet<u32>, BTreeSet<u32>) {
    let mut kept = BTreeSet::new();
    let mut exits = BTreeSet::new();
    let mut visited: BTreeSet<u32> = BTreeSet::from([start]);
    let mut queue: VecDeque<u32> = VecDeque::from([start]);
    while let Some(at) = queue.pop_front() {
        for id in local.graph.out_edges(at) {
            let Some(edge) = local.graph.edge(*id) else {
                continue;
            };
            if is_fan_edge(edge) || local.foreign.contains(&edge.src) {
                continue;
            }
            match fate_in(own, imports, edge.tgt) {
                Some(RemoteFate::Kept { .. }) => {
                    kept.insert(edge.tgt);
                }
                Some(RemoteFate::Dropped) if local.foreign.contains(&edge.tgt) => {
                    exits.insert(edge.tgt);
                }
                Some(RemoteFate::Dropped) => {
                    if visited.insert(edge.tgt) {
                        queue.push_back(edge.tgt);
                    }
                }
                Some(RemoteFate::Fan) | None => {}
            }
        }
    }
    (kept, exits)
}

/// The position of segment `id` in a list laid out by id.
fn slot(id: SegmentId) -> usize {
    usize::try_from(id.0).unwrap_or(usize::MAX)
}

/// The precedence key of a located restrict refusal; a refusal the engine
/// does not locate sorts first.
pub(super) const fn refusal_key(error: &RestrictError) -> Option<super::error::ErrorKey> {
    match error {
        RestrictError::At { key, .. } => Some(*key),
        _ => None,
    }
}

/// Keep the refusal with the least key, the earlier segment on a tie.
pub(super) fn keep_least(
    first: &mut Option<(SegmentId, RestrictError)>,
    segment: SegmentId,
    error: RestrictError,
) {
    let better = first
        .as_ref()
        .is_none_or(|(_, held)| refusal_key(&error) < refusal_key(held));
    if better {
        *first = Some((segment, error));
    }
}

/// Restrict the graph held as `segments` under `partition` along `plan`,
/// segment by segment.
///
/// The segments are checked as [`verify_partition`] checks them, and every
/// declared edge allocation range must lie above every edge id they hold.
/// The run then follows the stages of
/// [`graph_restrict_with_plan`](crate::graph_restrict_with_plan): the fan
/// domain, stage 0 in every segment, the exchange of the fates of in-stub
/// targets, the reach summaries and their closure, stages 1 to 3 in every
/// segment, and the allocation of synthesized ids from each segment's
/// ranges, in ascending key order, nodes before edges.
///
/// [`SegmentedRestrict::assemble`] of the output is the view and complement
/// whole-state restrict gives on the assembled graph.
///
/// [`verify_partition`]: super::verify_partition
///
/// # Errors
///
/// [`SegmentedRestrictError::Residual`] with [`Residual::Global`] when
/// [`graph_restrict_locality`] classifies the plan as global, before any
/// segment is read; [`SegmentedRestrictError::Segment`] with the error of
/// [`verify_partition`](super::verify_partition), then
/// [`SegmentError::AllocationBelowFloor`] for the least segment whose edge
/// allocation range starts at or below the greatest edge id held;
/// [`SegmentedRestrictError::Restrict`] with the refusal whole-state
/// restrict returns, other than the exhaustion of synthesized ids, in the
/// segment owning its element; and, in the least segment that fails to
/// allocate, [`SegmentError::NoAllocationRange`] or
/// [`SegmentError::AllocationExhausted`], nodes before edges.
pub fn graph_restrict_segmented(
    plan: &RestrictPlan,
    segments: &[GraphSegment],
    partition: &Partition,
) -> Result<SegmentedRestrict, SegmentedRestrictError> {
    if let Locality::Global(reason) = graph_restrict_locality(plan) {
        return Err(SegmentedRestrictError::Residual(Residual::Global {
            op: SegmentOp::Restrict,
            reason,
        }));
    }
    let index = check_family(segments, partition).map_err(SegmentedRestrictError::Segment)?;
    let locals: Vec<Local> = segments
        .iter()
        .map(Local::of)
        .collect::<Result<_, _>>()
        .map_err(SegmentedRestrictError::Segment)?;

    // The fan domain, before any stage.
    let first_fan = segments
        .iter()
        .zip(&locals)
        .filter_map(|(segment, local)| {
            local
                .fan_violation()
                .map(|(node, reason)| (node, segment.segment, reason))
        })
        .min_by_key(|(node, _, _)| *node);
    if let Some((node, segment, reason)) = first_fan {
        return Err(SegmentedRestrictError::Restrict {
            segment,
            error: RestrictError::MalformedFan { node, reason },
        });
    }

    // Stage 0 in every segment.
    let mut first: Option<(SegmentId, RestrictError)> = None;
    let mut fates: Vec<BTreeMap<u32, RemoteFate>> = Vec::with_capacity(segments.len());
    for (segment, local) in segments.iter().zip(&locals) {
        match decide_nodes(&local.graph, plan, local.foreign.clone()) {
            Ok(own) => fates.push(own),
            Err(error) => {
                keep_least(&mut first, segment.segment, error);
                fates.push(BTreeMap::new());
            }
        }
    }
    if let Some((segment, error)) = first {
        return Err(SegmentedRestrictError::Restrict { segment, error });
    }

    // Exchange: the fate of every in-stub target goes to the owner of the
    // in-stub's source.
    let mut imports: Vec<BTreeMap<u32, RemoteFate>> = vec![BTreeMap::new(); segments.len()];
    for (segment, own) in segments.iter().zip(&fates) {
        for stub in segment.boundary.in_stubs.values() {
            if let (Some(fate), Some(target)) =
                (own.get(&stub.tgt), imports.get_mut(slot(stub.src_owner)))
            {
                target.insert(stub.tgt, fate.clone());
            }
        }
    }

    // Summaries, and their closure by the coordinator.
    let summaries: Vec<ReachSummary> = segments
        .iter()
        .zip(&locals)
        .zip(fates.iter().zip(&imports))
        .map(|((segment, local), (own, imported))| summarize(segment, local, own, imported))
        .collect();
    let closed = close_reach(&summaries);

    // Stages 1 to 3 in every segment.
    let mut parts: Vec<RunParts> = Vec::with_capacity(segments.len());
    let mut first: Option<(SegmentId, RestrictError)> = None;
    for (position, (segment, local)) in segments.iter().zip(&locals).enumerate() {
        let outside = outside_of(local, &imports[position], &closed);
        match run_scoped(&local.graph, plan, outside) {
            Ok(run) => parts.push(run),
            Err(error) => keep_least(&mut first, segment.segment, error),
        }
    }
    if let Some((segment, error)) = first {
        return Err(SegmentedRestrictError::Restrict { segment, error });
    }

    emit(segments, partition, &index, parts).map_err(SegmentedRestrictError::Segment)
}

/// Check the family and the edge allocation ranges, and index the owners.
fn check_family(
    segments: &[GraphSegment],
    partition: &Partition,
) -> Result<OwnerIndex, SegmentError> {
    let any: Vec<super::body::AnySegment> = segments
        .iter()
        .cloned()
        .map(super::body::AnySegment::Graph)
        .collect();
    super::check::verify_partition(&any, partition)?;
    let max_edge = segments
        .iter()
        .filter_map(|s| s.edges.keys().next_back().map(|id| id.0))
        .max();
    if let Some(max_edge) = max_edge {
        for spec in partition.specs() {
            if spec.alloc_edges.is_some_and(|range| range.lo <= max_edge) {
                return Err(SegmentError::AllocationBelowFloor {
                    segment: spec.id,
                    kind: IdKind::Edge,
                });
            }
        }
    }
    partition.owner_index()
}

/// One segment's reach summary.
pub(super) fn summarize(
    segment: &GraphSegment,
    local: &Local,
    own: &BTreeMap<u32, RemoteFate>,
    imports: &BTreeMap<u32, RemoteFate>,
) -> ReachSummary {
    let mut summary = ReachSummary::default();
    for stub in segment.boundary.in_stubs.values() {
        let crossing_plain = stub.label.kind != HYPER_PARENT_KIND
            && stub.label.kind != HYPER_LABEL_KIND
            && matches!(own.get(&stub.tgt), Some(RemoteFate::Dropped));
        if !crossing_plain || summary.entries.contains_key(&stub.tgt) {
            continue;
        }
        let (kept, exits) = local_reach(local, own, imports, stub.tgt);
        for id in &kept {
            if let Some(fate) = fate_in(own, imports, *id) {
                summary.kept.insert(*id, fate.clone());
            }
        }
        summary.entries.insert(stub.tgt, (kept, exits));
    }
    summary
}

/// The closure of the reach summaries: for every entry of every segment,
/// every surviving node reached from it through dropped nodes of any
/// segment, with the fates of all the survivors named.
pub(super) struct ClosedReach {
    /// Every survivor each entry reaches.
    pub(super) reach: HashMap<u32, BTreeSet<u32>>,
    /// The fate of every survivor reached.
    pub(super) kept: BTreeMap<u32, RemoteFate>,
}

/// Close the reach summaries over the boundary: an entry reaches what it
/// reaches locally and what every entry it leaves through reaches.
///
/// The work is a search of the boundary graph (entries and their exits)
/// from each entry, so it is bounded by the summaries, not by the graph.
pub(super) fn close_reach<'s>(
    summaries: impl IntoIterator<Item = &'s ReachSummary>,
) -> ClosedReach {
    let mut entries: HashMap<u32, &(BTreeSet<u32>, BTreeSet<u32>)> = HashMap::new();
    let mut kept = BTreeMap::new();
    for summary in summaries {
        entries.extend(summary.entries.iter().map(|(id, entry)| (*id, entry)));
        kept.extend(summary.kept.iter().map(|(id, fate)| (*id, fate.clone())));
    }
    let mut reach = HashMap::with_capacity(entries.len());
    for start in entries.keys() {
        let mut reached = BTreeSet::new();
        let mut visited: BTreeSet<u32> = BTreeSet::from([*start]);
        let mut queue: VecDeque<u32> = VecDeque::from([*start]);
        while let Some(at) = queue.pop_front() {
            let Some((survivors, exits)) = entries.get(&at) else {
                continue;
            };
            reached.extend(survivors.iter().copied());
            for exit in exits {
                if visited.insert(*exit) {
                    queue.push_back(*exit);
                }
            }
        }
        reach.insert(*start, reached);
    }
    ClosedReach { reach, kept }
}

/// What a segment's stages 1 to 3 know of the nodes it does not own: the
/// fates of its ghosts, and the closed reach of its dropped ghosts with the
/// fates of the survivors they reach.
pub(super) fn outside_of(
    local: &Local,
    imports: &BTreeMap<u32, RemoteFate>,
    closed: &ClosedReach,
) -> Outside {
    let mut outside = Outside {
        foreign: local.foreign.clone(),
        fates: imports.clone(),
        reach: BTreeMap::new(),
    };
    for (id, fate) in imports {
        if *fate != RemoteFate::Dropped {
            continue;
        }
        let reached = closed.reach.get(id).cloned().unwrap_or_default();
        for target in &reached {
            if let Some(fate) = closed.kept.get(target) {
                outside.fates.entry(*target).or_insert_with(|| fate.clone());
            }
        }
        outside.reach.insert(*id, reached);
    }
    outside
}

/// The allocator of one kind for a segment that must allocate `count` ids:
/// `None` when it allocates none.
fn allocator_for(
    segment: SegmentId,
    kind: IdKind,
    range: Option<super::ids::IdRange>,
    held: impl IntoIterator<Item = u32>,
    count: usize,
) -> Result<Option<IdAllocator>, SegmentError> {
    match range {
        Some(range) => Ok(Some(IdAllocator::from_source(range.check(segment)?, held))),
        None if count == 0 => Ok(None),
        None => Err(SegmentError::NoAllocationRange { segment, kind }),
    }
}

/// Allocate one id per key, in ascending key order.
fn allocate_keys<'k>(
    allocator: Option<&mut IdAllocator>,
    keys: impl IntoIterator<Item = &'k SynthKey>,
    segment: SegmentId,
    kind: IdKind,
) -> Result<BTreeMap<SynthKey, u32>, SegmentError> {
    let mut ids = BTreeMap::new();
    let mut allocator = allocator;
    for key in keys {
        let id = allocator
            .as_deref_mut()
            .ok_or(SegmentError::NoAllocationRange { segment, kind })?
            .allocate()
            .map_err(|_| SegmentError::AllocationExhausted { segment, kind })?;
        ids.insert(key.clone(), id);
    }
    Ok(ids)
}

/// Allocate every segment's synthesized ids and lay the view and the
/// complement out segment by segment, with the view's boundaries.
fn emit(
    segments: &[GraphSegment],
    partition: &Partition,
    index: &OwnerIndex,
    parts: Vec<RunParts>,
) -> Result<SegmentedRestrict, SegmentError> {
    let mut views: Vec<GraphSegment> = Vec::with_capacity(segments.len());
    let mut complements = Vec::with_capacity(segments.len());
    for (segment, run) in segments.iter().zip(parts) {
        let (view, complement) = emit_segment(segment, partition, run)?;
        views.push(view);
        complements.push(complement);
    }
    let mut boundaries = ViewBoundaries::default();
    for view in &views {
        boundaries.add(view, index, |_| true)?;
    }
    for (view, boundary) in views.iter_mut().zip(boundaries.finish(segments.len())?) {
        view.boundary = boundary;
    }
    Ok(SegmentedRestrict { views, complements })
}

/// Allocate one segment's synthesized ids from its ranges and lay its view
/// and complement out, the view with an empty boundary.
///
/// Node ids come from the segment's node allocation range and edge ids from
/// its edge allocation range, each in ascending key order, above every id
/// the segment holds.
pub(super) fn emit_segment(
    segment: &GraphSegment,
    partition: &Partition,
    run: RunParts,
) -> Result<(GraphSegment, GraphComplementSegment), SegmentError> {
    let id = segment.segment;
    let spec = partition.spec(id).ok_or(SegmentError::MalformedPartition {
        segment: id,
        defect: super::error::PartitionDefect::UnknownSegment,
    })?;
    let mut nodes_alloc = allocator_for(
        id,
        IdKind::Node,
        spec.alloc_nodes,
        segment.nodes.keys().copied(),
        run.synth_nodes.len(),
    )?;
    let node_ids = allocate_keys(
        nodes_alloc.as_mut(),
        run.synth_nodes.keys(),
        id,
        IdKind::Node,
    )?;
    let arcs = run.arcs(&node_ids);
    let mut edges_alloc = allocator_for(
        id,
        IdKind::Edge,
        spec.alloc_edges,
        segment.edges.keys().map(|edge| edge.0),
        arcs.len(),
    )?;
    let edge_keys: BTreeSet<&SynthKey> = arcs.iter().map(|(key, ..)| key).collect();
    let edge_ids = allocate_keys(edges_alloc.as_mut(), edge_keys, id, IdKind::Edge)?;

    let RunParts {
        nodes: mut view_nodes,
        edges: mut view_edges,
        synth_nodes,
        complement,
        ..
    } = run;
    let mut synthesized_nodes = BTreeMap::new();
    for (key, anchor) in synth_nodes {
        let node = node_ids[&key];
        view_nodes.insert(node, Node::new(node, anchor));
        synthesized_nodes.insert(node, key);
    }
    let mut synthesized_edges = BTreeMap::new();
    for (key, src, tgt, label) in arcs {
        let edge = EdgeId(edge_ids[&key]);
        view_edges.insert(edge, GEdge::new(src, tgt, label));
        synthesized_edges.insert(edge, key);
    }
    let complement = GraphComplementSegment {
        segment: id,
        dropped_nodes: complement.dropped_nodes,
        dropped_edges: complement.dropped_edges,
        original_edges: complement.original_edges,
        original_anchors: complement.original_anchors,
        original_extra_fields: complement.original_extra_fields,
        original_values: complement.original_values,
        synthesized_nodes,
        synthesized_edges,
        floors: IdFloor {
            node: nodes_alloc.map_or(0, |a| a.floor()),
            edge: edges_alloc.map_or(0, |a| a.floor()),
        },
    };
    let view = GraphSegment {
        segment: id,
        nodes: view_nodes,
        edges: view_edges,
        boundary: Boundary::default(),
    };
    Ok((view, complement))
}

/// The boundaries of a family of view segments, gathered one view at a
/// time: a ghost for every remote target of an owned view edge, and an
/// in-stub for it in the target's segment.
///
/// A ghost carries its node's anchor in the owner's view, so each view
/// added offers the anchors of the nodes a remote view edge may target
/// (`candidates`); a view edge whose target's owner offered no anchor for it
/// is dangling.
#[derive(Default)]
pub(super) struct ViewBoundaries {
    /// Each crossing view edge: the segment holding it, its element, and its
    /// in-stub.
    crossings: Vec<(SegmentId, ViewRef, InStub, SegmentId)>,
    /// The view anchors the views offered.
    anchors: BTreeMap<u32, Name>,
}

impl ViewBoundaries {
    /// Add one view: record each owned view edge whose target the view does
    /// not hold, and offer the anchor of each view node `candidates` admits.
    ///
    /// # Errors
    ///
    /// [`SegmentError::Unowned`] for a crossing edge whose target no segment
    /// owns, and [`SegmentError::DanglingEndpoint`] for one whose source the
    /// view does not hold.
    pub(super) fn add(
        &mut self,
        view: &GraphSegment,
        index: &OwnerIndex,
        candidates: impl Fn(u32) -> bool,
    ) -> Result<(), SegmentError> {
        for (id, node) in &view.nodes {
            if candidates(*id) {
                self.anchors.insert(*id, node.anchor.clone());
            }
        }
        for (id, edge) in &view.edges {
            if view.nodes.contains_key(&edge.tgt) {
                continue;
            }
            let elem = ViewRef::Edge(*id);
            let owner = index
                .owner_of_node(edge.tgt)
                .ok_or_else(|| SegmentError::Unowned { elem: elem.clone() })?;
            let src_anchor = view
                .nodes
                .get(&edge.src)
                .map(|node| node.anchor.clone())
                .ok_or_else(|| dangling(elem.clone(), edge.src))?;
            self.crossings.push((
                view.segment,
                elem,
                InStub {
                    src: edge.src,
                    src_owner: view.segment,
                    src_anchor,
                    tgt: edge.tgt,
                    label: edge.edge.clone(),
                },
                owner,
            ));
        }
        Ok(())
    }

    /// The boundary of each of the `count` segments, in id order.
    ///
    /// # Errors
    ///
    /// [`SegmentError::DanglingEndpoint`] for the first crossing edge whose
    /// target's anchor no view offered.
    pub(super) fn finish(self, count: usize) -> Result<Vec<Boundary>, SegmentError> {
        let mut boundaries = vec![Boundary::default(); count];
        for (holder, elem, stub, owner) in self.crossings {
            let anchor = self
                .anchors
                .get(&stub.tgt)
                .cloned()
                .ok_or_else(|| dangling(elem.clone(), stub.tgt))?;
            if let Some(boundary) = boundaries.get_mut(slot(holder)) {
                boundary.ghosts.entry(stub.tgt).or_insert(Ghost {
                    node: stub.tgt,
                    owner,
                    anchor,
                });
            }
            if let Some(boundary) = boundaries.get_mut(slot(owner)) {
                boundary.in_stubs.insert(elem, stub);
            }
        }
        Ok(boundaries)
    }
}

impl SegmentedRestrict {
    /// The whole view and complement: the view segments glued under
    /// `partition`, the complement segments joined, and the synthesized
    /// elements renumbered as whole-state restrict numbers them, in
    /// ascending key order from one above the greatest source id of each
    /// kind (the source ids being the view's other ids and the dropped
    /// ones). The complement's floors are those floors, and it is
    /// unstamped, as the engine leaves it.
    ///
    /// # Errors
    ///
    /// The error assembly of the view segments returns
    /// ([`assemble_graph`]).
    pub fn assemble(
        &self,
        partition: &Partition,
    ) -> Result<(GInstance, GraphComplement), SegmentError> {
        let glued = assemble_graph(self.views.iter().cloned(), partition)?;
        let mut complement = GraphComplement::empty();
        let mut synth_nodes: BTreeMap<u32, SynthKey> = BTreeMap::new();
        let mut synth_edges: BTreeMap<EdgeId, SynthKey> = BTreeMap::new();
        for part in &self.complements {
            complement.dropped_nodes.extend(part.dropped_nodes.clone());
            complement.dropped_edges.extend(part.dropped_edges.clone());
            complement
                .original_edges
                .extend(part.original_edges.clone());
            complement
                .original_anchors
                .extend(part.original_anchors.clone());
            complement
                .original_extra_fields
                .extend(part.original_extra_fields.clone());
            complement
                .original_values
                .extend(part.original_values.clone());
            synth_nodes.extend(part.synthesized_nodes.clone());
            synth_edges.extend(part.synthesized_edges.clone());
        }
        let source_node_max = glued
            .nodes()
            .keys()
            .copied()
            .filter(|id| !synth_nodes.contains_key(id))
            .chain(complement.dropped_nodes.keys().copied())
            .max();
        let source_edge_max = glued
            .edges()
            .keys()
            .copied()
            .filter(|id| !synth_edges.contains_key(id))
            .chain(complement.dropped_edges.keys().copied())
            .map(|id| id.0)
            .max();
        let floor = IdFloor {
            node: source_node_max.map_or(0, |m| m.saturating_add(1)),
            edge: source_edge_max.map_or(0, |m| m.saturating_add(1)),
        };
        let renumber = |held: &BTreeMap<u32, SynthKey>, floor: u32| -> HashMap<u32, u32> {
            let by_key: BTreeMap<&SynthKey, u32> =
                held.iter().map(|(id, key)| (key, *id)).collect();
            by_key.into_values().zip(floor..).collect()
        };
        let node_ids = renumber(&synth_nodes, floor.node);
        let edge_ids = renumber(
            &synth_edges
                .iter()
                .map(|(id, key)| (id.0, key.clone()))
                .collect(),
            floor.edge,
        );
        let node = |id: u32| node_ids.get(&id).copied().unwrap_or(id);
        let mut view = GInstance::new();
        for (id, held) in glued.nodes() {
            let mut held = held.clone();
            held.id = node(*id);
            view = view.with_node(held);
        }
        for (id, edge) in glued.edges_sorted() {
            let id = EdgeId(edge_ids.get(&id.0).copied().unwrap_or(id.0));
            let mut edge = edge.clone();
            edge.src = node(edge.src);
            edge.tgt = node(edge.tgt);
            view.insert_edge(id, edge)
                .map_err(|error| placed(&error, id))?;
        }
        complement.synthesized_nodes = synth_nodes
            .into_iter()
            .map(|(id, key)| (node(id), key))
            .collect();
        complement.synthesized_edges = synth_edges
            .into_iter()
            .map(|(id, key)| (EdgeId(edge_ids.get(&id.0).copied().unwrap_or(id.0)), key))
            .collect();
        complement.id_floor = floor;
        Ok((view, complement))
    }
}
