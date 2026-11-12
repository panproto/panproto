//! Graph lens put over a view held as the segments of a partition.
//!
//! [`put_segmented`] restores a view laid out as segments (the output of a
//! segmented get, possibly edited) against the complement segments of that
//! get, one segment at a time, never assembling the view or the source;
//! [`put_segmented_run`] runs the same put as a checkpointed run of
//! [`run_segmented`], one task per segment and phase. Each segment restores
//! the nodes it owns and the edges it holds against its own complement
//! segment, by the same restoration whole-state [`put`] runs, with the nodes
//! other segments own held as placeholders. What a segment reads of another
//! segment's nodes and edges arrives in three steps:
//!
//! - a census of edge ids, a merge of each segment's sorted runs of view,
//!   dropped, synthesized and recorded edge ids, which finds every view edge
//!   whose id a complement elsewhere also uses;
//! - one exchange of node facts: each segment asks the owner of every
//!   remote node its edges, dropped edges and synthesized chains name for
//!   that node's view anchor and its restored form;
//! - after restoration, the boundary of the restored family: every restored
//!   edge entering another segment becomes an in-stub there.
//!
//! Forward stability restricts the restored family segment by segment
//! ([`graph_restrict_segmented_nodes_with`]); uniqueness keys merge each
//! segment's sorted key runs. Every segment's violations are collected and
//! the least in report order is returned, so a refused put changes no
//! segment. The restored segments are laid out under the view's partition.
//!
//! [`put`]: super::put

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use panproto_inst::identity::{
    CanonicalSink, ComplementShape, Digest, IdentityCarrier, SEGMENT_COMPLEMENT_CONTEXT,
};
use panproto_inst::locality::{GlobalReason, Locality, SegmentOp, SummaryKind};
use panproto_inst::segment::{
    AnySegment, ArtifactKind, Boundary, Budget, Emission, Ghost, GraphComplementSegment,
    GraphSegment, InStub, ManifestRole, MemoryModel, OwnerIndex, Partition, PartitionDefect, Phase,
    PhaseContext, PhaseOperation, PhaseStop, Reader, Residual, RunSpec, SegmentError, SegmentId,
    SegmentManifest, SegmentRecord, SegmentSource, SegmentStore, SegmentedRestrictError,
    SegmentedRun, StreamStop, graph_restrict_locality, graph_restrict_segmented_nodes_with,
    read_boundary, run_segmented, store_failure, verify_partition, write_boundary,
};
use panproto_inst::{
    EdgeId, GEdge, GInstance, GraphComplement, Node, NodeShape, PutDomainError, RestrictPlan,
    SynthKey, ViewRef, write_edge_record, write_node_record,
};
use panproto_schema::{CanonicalWriter, KeyPart, KeyRole, KeyScope, Schema};

use super::{
    PutScope, Restored, fan_violations, key_values, refusal_from_restrict, required_violations,
    restorable_part, restore_nodes, restore_parts, stability_against, synthesized_edits,
    uniqueness_role,
};
use crate::{Lens, LensError};

/// The operation bytes of graph put in a run's identity.
pub const GRAPH_PUT_OP: &[u8] = b"graph put";

/// Why a segmented graph put returned no source.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SegmentedPutError {
    /// The view segments are not a valid family under the partition, or a
    /// complement segment is missing, repeated, or holds an entry about an
    /// element another segment owns.
    #[error(transparent)]
    Segment(SegmentError),
    /// Whole-state put of the assembled view refuses with this error: the
    /// least put-domain violation of every segment, in report order.
    #[error(transparent)]
    Lens(LensError),
    /// The lens's put is not supported segment by segment.
    #[error("{0}")]
    Residual(Residual),
}

impl From<SegmentError> for SegmentedPutError {
    fn from(error: SegmentError) -> Self {
        Self::Segment(error)
    }
}

/// The locality class of graph put along `lens`.
///
/// Put restricts the restored source again to check forward stability, so
/// a restrict that [`graph_restrict_locality`] classifies as global is
/// global here too. A uniqueness or primary key of the source schema that
/// reads a position among siblings, a scope found among ancestors, or a
/// path of more than one edge is [`GlobalReason::KeyDemand`] at the least
/// such vertex and key. Otherwise put exchanges node facts across the
/// boundary and merges sorted runs of edge ids and key values, so its class
/// is [`SummaryKind::SortedMerge`].
///
/// # Errors
///
/// The plan compilation error.
pub fn graph_put_locality(lens: &Lens) -> Result<Locality, LensError> {
    let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)?;
    Ok(put_locality(lens, &plan))
}

fn put_locality(lens: &Lens, plan: &RestrictPlan) -> Locality {
    let restrict = graph_restrict_locality(plan);
    if matches!(restrict, Locality::Global(_)) {
        return restrict;
    }
    if let Some(reason) = key_demand(&lens.src_schema) {
        return Locality::Global(reason);
    }
    Locality::of_clauses([
        restrict,
        Locality::Exchange,
        Locality::Summary(SummaryKind::SortedMerge),
    ])
}

/// The least key put reads beyond an element and its out-edges' targets.
fn key_demand(schema: &Schema) -> Option<GlobalReason> {
    schema
        .declarations
        .keys
        .iter()
        .flat_map(|(vertex, keys)| keys.iter().map(move |key| (vertex, key)))
        .filter(|(_, key)| {
            (uniqueness_role(&key.role) || key.role == KeyRole::Primary)
                && (matches!(key.scope, KeyScope::Within(_))
                    || key.parts.iter().any(|part| match part {
                        KeyPart::Position { .. } => true,
                        KeyPart::Path(path) => path.len() > 1,
                    }))
        })
        .map(|(vertex, key)| (vertex.clone(), key.id.clone()))
        .min()
        .map(|(vertex, key)| GlobalReason::KeyDemand { vertex, key })
}

/// The plan of `lens`, its compilation failure reported as whole-state put
/// reports it.
fn compile(lens: &Lens) -> Result<RestrictPlan, SegmentedPutError> {
    RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled).map_err(|error| {
        SegmentedPutError::Lens(LensError::PutDomain(Box::new(
            PutDomainError::PlanCompilation {
                detail: error.to_string(),
            },
        )))
    })
}

/// One segment's part of the put: its view with placeholders, its
/// complement, and what it knows of other segments.
struct Part {
    view: GraphSegment,
    /// The view segment's owned nodes and held edges, with a placeholder
    /// for each remote view node the segment reads.
    local: GInstance,
    /// The complement segment as a whole-state complement, stamped with the
    /// lens's graph identity.
    complement: GraphComplement,
    scope: PutScope,
}

impl Part {
    /// The part of `view` against `complement`, with what the census
    /// `census` tells it of the edge ids other segments record.
    fn new(
        lens: &Lens,
        view: GraphSegment,
        complement: &GraphComplementSegment,
        census: CensusPart,
    ) -> Result<Self, SegmentError> {
        let mut whole = whole_complement(complement, &census.borrowed);
        whole.stamp(lens.identity(ComplementShape::Graph), &lens.src_schema);
        let (local, foreign) = local_view(&view)?;
        Ok(Self {
            view,
            local,
            complement: whole,
            scope: PutScope {
                foreign,
                synthesized_edges: census.synthesized_elsewhere,
                ..PutScope::default()
            },
        })
    }

    /// The facts this part's owner offers of the nodes `wanted`, from its
    /// own view and complement segments.
    fn offer(&self, lens: &Lens, wanted: &BTreeSet<u32>) -> BTreeMap<u32, NodeFact> {
        let (restored, _) = restore_nodes(lens, &self.local, &self.complement, &self.scope);
        wanted
            .iter()
            .map(|&node| {
                let fact = NodeFact {
                    shown: self.view.nodes.get(&node).map(|shown| shown.anchor.clone()),
                    restored: restored.node(node).map(|restored| {
                        let mut placeholder = Node::new(node, restored.anchor.clone());
                        placeholder.value.clone_from(&restored.value);
                        placeholder
                    }),
                };
                (node, fact)
            })
            .collect()
    }

    /// Take the facts of the remote nodes this part reads: a placeholder
    /// with the node's view anchor when the view holds it, and the node's
    /// restored anchor and value when it restores. `fact` answers a node
    /// with its owner's fact, `None` when no segment answers it.
    fn take(&mut self, index: &OwnerIndex, fact: &dyn Fn(u32) -> Option<NodeFact>) {
        for node in requests(&self.view, &self.complement, index) {
            let Some(fact) = fact(node) else {
                continue;
            };
            if let Some(anchor) = fact.shown
                && !self.scope.foreign.contains(&node)
            {
                self.local = std::mem::take(&mut self.local).with_node(Node::new(node, anchor));
                self.scope.foreign.insert(node);
            }
            if let Some(placeholder) = fact.restored {
                self.scope.restored.insert(node, placeholder);
            }
        }
    }

    /// Restore this part: the violations it finds and the restoration.
    fn restore(&self, lens: &Lens, plan: &RestrictPlan) -> (Vec<PutDomainError>, Restored) {
        let mut found = synthesized_edits(&self.local, &self.complement, plan);
        let outcome = restore_parts(lens, &self.local, &self.complement, plan, &self.scope);
        found.extend(outcome.errors.iter().cloned());
        found.extend(outcome.trace_errors.iter().cloned());
        (found, outcome)
    }
}

/// What an owner tells a segment of one of its nodes: the node's view
/// anchor when the owner's view holds it, and its restored form (anchor and
/// value) when the owner restores it.
#[derive(Clone, Debug)]
struct NodeFact {
    shown: Option<panproto_gat::Name>,
    restored: Option<Node>,
}

/// The canonical bytes of an owner's facts: their count, then per node
/// ascending its id, a presence byte and the view anchor, and a presence
/// byte and the restored placeholder's node record.
fn facts_bytes(facts: &BTreeMap<u32, NodeFact>) -> Vec<u8> {
    let mut sink = CanonicalWriter::new();
    sink.len(facts.len());
    for (node, fact) in facts {
        sink.u32(*node);
        sink.opt_name(fact.shown.as_ref());
        match &fact.restored {
            None => sink.bool(false),
            Some(placeholder) => {
                sink.bool(true);
                write_node_record(&mut sink, placeholder);
            }
        }
    }
    sink.into_bytes()
}

fn decode_facts(bytes: &[u8]) -> Result<BTreeMap<u32, NodeFact>, SegmentError> {
    let mut r = Reader::new(bytes, ArtifactKind::Summary);
    let n = r.count(6)?;
    let mut facts = BTreeMap::new();
    for _ in 0..n {
        let node = r.u32()?;
        if facts.keys().next_back().is_some_and(|last| *last >= node) {
            return Err(r.error("facts are not strictly ascending"));
        }
        let shown = r.opt_name()?;
        let restored = if r.bool()? { Some(r.node()?) } else { None };
        facts.insert(node, NodeFact { shown, restored });
    }
    r.finish()?;
    Ok(facts)
}

/// One segment's census runs and node-fact requests: its held view edge
/// ids, its complement's dropped, synthesized and recorded edge ids (with
/// the recorded source form), its view nodes whose ids its complement
/// records as dropped, and the remote nodes it reads, by owner.
#[derive(Debug, Default)]
struct Exported {
    held: BTreeSet<EdgeId>,
    dropped: BTreeSet<EdgeId>,
    synthesized: BTreeSet<EdgeId>,
    original: BTreeMap<EdgeId, GEdge>,
    node_collisions: BTreeSet<u32>,
    requests: BTreeMap<SegmentId, BTreeSet<u32>>,
}

impl Exported {
    fn of(view: &GraphSegment, complement: &GraphComplementSegment, index: &OwnerIndex) -> Self {
        let mut requested: BTreeMap<SegmentId, BTreeSet<u32>> = BTreeMap::new();
        for node in requests_of(view, complement, index) {
            if let Some(owner) = index.owner_of_node(node) {
                requested.entry(owner).or_default().insert(node);
            }
        }
        Self {
            held: view.edges.keys().copied().collect(),
            dropped: complement.dropped_edges.keys().copied().collect(),
            synthesized: complement.synthesized_edges.keys().copied().collect(),
            original: complement.original_edges.clone(),
            node_collisions: view
                .nodes
                .keys()
                .filter(|id| complement.dropped_nodes.contains_key(id))
                .copied()
                .collect(),
            requests: requested,
        }
    }

    /// The canonical bytes: each id run counted and ascending (held,
    /// dropped, synthesized), the recorded edges as edge records, the
    /// colliding node ids, then each owner with its requested nodes.
    fn bytes(&self) -> Vec<u8> {
        let mut sink = CanonicalWriter::new();
        for run in [&self.held, &self.dropped, &self.synthesized] {
            sink.len(run.len());
            for id in run {
                sink.u32(id.0);
            }
        }
        sink.len(self.original.len());
        for (id, edge) in &self.original {
            write_edge_record(&mut sink, *id, edge);
        }
        sink.len(self.node_collisions.len());
        for id in &self.node_collisions {
            sink.u32(*id);
        }
        sink.len(self.requests.len());
        for (owner, nodes) in &self.requests {
            sink.u32(owner.0);
            sink.len(nodes.len());
            for node in nodes {
                sink.u32(*node);
            }
        }
        sink.into_bytes()
    }

    fn decode(bytes: &[u8]) -> Result<Self, SegmentError> {
        fn ids(r: &mut Reader<'_>) -> Result<BTreeSet<u32>, SegmentError> {
            let n = r.count(4)?;
            let mut ids = BTreeSet::new();
            for _ in 0..n {
                let id = r.u32()?;
                if ids.last().is_some_and(|last| *last >= id) {
                    return Err(r.error("ids are not strictly ascending"));
                }
                ids.insert(id);
            }
            Ok(ids)
        }
        let edges = |set: BTreeSet<u32>| set.into_iter().map(EdgeId).collect();
        let mut r = Reader::new(bytes, ArtifactKind::Export);
        let held = edges(ids(&mut r)?);
        let dropped = edges(ids(&mut r)?);
        let synthesized = edges(ids(&mut r)?);
        let n = r.count(32)?;
        let mut original = BTreeMap::new();
        for _ in 0..n {
            let (id, edge) = r.edge_record()?;
            if original.keys().next_back().is_some_and(|last| *last >= id) {
                return Err(r.error("recorded edges are not strictly ascending"));
            }
            original.insert(id, edge);
        }
        let node_collisions = ids(&mut r)?;
        let n = r.count(8)?;
        let mut requests = BTreeMap::new();
        for _ in 0..n {
            let owner = SegmentId(r.u32()?);
            if requests
                .keys()
                .next_back()
                .is_some_and(|last| *last >= owner)
            {
                return Err(r.error("owners are not strictly ascending"));
            }
            requests.insert(owner, ids(&mut r)?);
        }
        r.finish()?;
        Ok(Self {
            held,
            dropped,
            synthesized,
            original,
            node_collisions,
            requests,
        })
    }
}

/// What the census tells one segment: the view edges it holds whose ids
/// another segment's complement records as synthesized, and the recorded
/// source form of each view edge it holds whose record another segment
/// keeps.
#[derive(Default)]
struct CensusPart {
    synthesized_elsewhere: BTreeSet<EdgeId>,
    borrowed: BTreeMap<EdgeId, GEdge>,
    /// The held ids some export has listed as synthesized or recorded; the
    /// first segment listing an id answers for it.
    answered: BTreeSet<EdgeId>,
    recorded: BTreeSet<EdgeId>,
}

impl CensusPart {
    /// Read the export of `segment` for the view segment `view`; exports
    /// are read in segment order.
    fn add(&mut self, view: &GraphSegment, segment: SegmentId, exported: &Exported) {
        for id in view.edges.keys() {
            if exported.synthesized.contains(id)
                && self.answered.insert(*id)
                && segment != view.segment
            {
                self.synthesized_elsewhere.insert(*id);
            }
            if let Some(edge) = exported.original.get(id)
                && self.recorded.insert(*id)
                && segment != view.segment
            {
                self.borrowed.insert(*id, edge.clone());
            }
        }
    }
}

/// The census part of the view segment `view`, from every segment's export
/// in segment order.
fn census_part<'e>(
    view: &GraphSegment,
    exports: impl IntoIterator<Item = (SegmentId, &'e Exported)>,
) -> CensusPart {
    let mut part = CensusPart::default();
    for (segment, exported) in exports {
        part.add(view, segment, exported);
    }
    part
}

/// The census merge of every segment's export, in segment order: an edge
/// id two complements record as dropped, as synthesized or as recorded
/// (checked in that order, each by ascending id) is refused, and otherwise
/// every view edge whose id a complement records as dropped and every view
/// node whose id its own complement records as dropped is an id collision.
fn census_collisions<'e>(
    exports: impl IntoIterator<Item = (SegmentId, &'e Exported)> + Clone,
) -> Result<Vec<PutDomainError>, SegmentError> {
    let listed = |pick: &dyn Fn(&Exported) -> Vec<EdgeId>| {
        let mut by_id: BTreeMap<EdgeId, Vec<SegmentId>> = BTreeMap::new();
        for (segment, exported) in exports.clone() {
            for id in pick(exported) {
                by_id.entry(id).or_default().push(segment);
            }
        }
        by_id
    };
    let dropped = listed(&|e| e.dropped.iter().copied().collect());
    let synthesized = listed(&|e| e.synthesized.iter().copied().collect());
    let original = listed(&|e| e.original.keys().copied().collect());
    for by_id in [&dropped, &synthesized, &original] {
        if let Some((edge, segments)) = by_id.iter().find(|(_, segments)| segments.len() > 1) {
            return Err(SegmentError::DuplicateEdgeId {
                edge: *edge,
                segments: segments.clone(),
            });
        }
    }
    let mut collisions: Vec<PutDomainError> = Vec::new();
    for (_, exported) in exports {
        collisions.extend(
            exported
                .held
                .iter()
                .filter(|id| dropped.contains_key(id))
                .map(|id| PutDomainError::IdCollision {
                    elem: ViewRef::Edge(*id),
                }),
        );
        collisions.extend(
            exported
                .node_collisions
                .iter()
                .map(|&id| PutDomainError::IdCollision {
                    elem: ViewRef::Node(id),
                }),
        );
    }
    Ok(collisions)
}

/// Restore the graph a lens's view held as `views` under `partition`
/// against the complement segments `complements`, segment by segment.
///
/// Write `glue` for gluing segments under `partition` and `join` for joining
/// the complement segments into one complement stamped with the graph
/// identity of `lens`, as [`get`](super::get) stamps it. Then the output,
/// glued, is [`put`](super::put) of `glue(views)` against `join(complements)`,
/// and each output segment holds the restored nodes its segment owns and the
/// restored edges whose source it owns, with the boundary those edges
/// cross. When whole-state put refuses, the segmented put returns the same
/// refusal, which is the least violation found in any segment, and returns
/// no segment.
///
/// Every view segment restores against its own complement segment; the
/// segments exchange only a census of edge ids, the view anchors and
/// restored forms of the remote nodes their elements name, the restored
/// edges that cross the boundary, the forward-stability run of
/// [`graph_restrict_segmented_nodes_with`] and sorted runs of key values.
///
/// # Errors
///
/// [`SegmentedPutError::Residual`] with [`Residual::Global`] when
/// [`graph_put_locality`] classifies the lens as global, before any segment
/// is read; [`SegmentedPutError::Segment`] when the views are not a family
/// under `partition` ([`verify_partition`]), when the complements are not one
/// per segment in the views' order (a missing or repeated segment), when a
/// complement entry is about an element its segment does not own
/// ([`SegmentError::WrongOwner`] or [`SegmentError::Unowned`]), or when two
/// complement segments record one edge id
/// ([`SegmentError::DuplicateEdgeId`]); [`SegmentedPutError::Lens`] with the
/// refusal of whole-state put: [`LensError::PutDomain`] with
/// [`PutDomainError::PlanCompilation`] when the plan does not compile, and
/// otherwise the least put-domain violation in report order.
pub fn put_segmented(
    lens: &Lens,
    views: &[GraphSegment],
    complements: &[GraphComplementSegment],
    partition: &Partition,
) -> Result<Vec<GraphSegment>, SegmentedPutError> {
    let plan = compile(lens)?;
    if let Locality::Global(reason) = put_locality(lens, &plan) {
        return Err(SegmentedPutError::Residual(Residual::Global {
            op: SegmentOp::Put,
            reason,
        }));
    }
    let index = check_inputs(views, complements, partition).map_err(SegmentedPutError::Segment)?;
    let exports: Vec<(SegmentId, Exported)> = views
        .iter()
        .zip(complements)
        .map(|(view, complement)| (view.segment, Exported::of(view, complement, &index)))
        .collect();
    let listed = || {
        exports
            .iter()
            .map(|(segment, exported)| (*segment, exported))
    };

    // Id collisions are the least violations in report order, so when there
    // is one the least of them is the refusal.
    let collisions = census_collisions(listed()).map_err(SegmentedPutError::Segment)?;
    if let Some(least) = least(collisions) {
        return Err(refused(least));
    }

    let mut parts: Vec<Part> = Vec::with_capacity(views.len());
    for (view, complement) in views.iter().zip(complements) {
        let census = census_part(view, listed());
        parts.push(Part::new(lens, view.clone(), complement, census)?);
    }
    let offered: Vec<BTreeMap<u32, NodeFact>> = parts
        .iter()
        .map(|part| {
            let wanted: BTreeSet<u32> = listed()
                .filter_map(|(_, exported)| exported.requests.get(&part.view.segment))
                .flatten()
                .copied()
                .collect();
            part.offer(lens, &wanted)
        })
        .collect();
    let position: BTreeMap<SegmentId, usize> = views
        .iter()
        .enumerate()
        .map(|(at, view)| (view.segment, at))
        .collect();
    for part in &mut parts {
        part.take(&index, &|node| {
            let owner = index.owner_of_node(node)?;
            offered.get(*position.get(&owner)?)?.get(&node).cloned()
        });
    }

    let mut found: Vec<PutDomainError> = Vec::new();
    let mut restored: Vec<Restored> = Vec::with_capacity(parts.len());
    for part in &parts {
        let (violations, outcome) = part.restore(lens, &plan);
        found.extend(violations);
        restored.push(outcome);
    }

    // The clauses read on the restored source run on what restores, unless
    // a node's transforms cannot be traced, which leaves the restored source
    // undetermined.
    let mut family = Vec::new();
    if restored
        .iter()
        .all(|outcome| outcome.trace_errors.is_empty())
    {
        family = restored_family(&parts, &restored, &index)?;
        let held = InMemory {
            parts: &parts,
            restored: &restored,
            family: &family,
        };
        found.extend(source_clauses(lens, &plan, &held)?);
    }
    least(found).map_or(Ok(family), |error| Err(refused(error)))
}

fn refused(error: PutDomainError) -> SegmentedPutError {
    SegmentedPutError::Lens(LensError::PutDomain(Box::new(error)))
}

/// The least violation in report order.
fn least(mut found: Vec<PutDomainError>) -> Option<PutDomainError> {
    PutDomainError::sort_report(&mut found);
    found.into_iter().next()
}

/// Check the views as a family under `partition`, and that the complements
/// are one per view segment in the same order, each holding entries only
/// about the elements its segment owns.
fn check_inputs(
    views: &[GraphSegment],
    complements: &[GraphComplementSegment],
    partition: &Partition,
) -> Result<OwnerIndex, SegmentError> {
    let any: Vec<AnySegment> = views.iter().cloned().map(AnySegment::Graph).collect();
    verify_partition(&any, partition)?;
    let index = partition.owner_index()?;
    for (at, view) in views.iter().enumerate() {
        let Some(complement) = complements.get(at) else {
            return Err(SegmentError::MalformedPartition {
                segment: view.segment,
                defect: PartitionDefect::SegmentMissing,
            });
        };
        if complement.segment != view.segment {
            return Err(SegmentError::MalformedPartition {
                segment: complement.segment,
                defect: PartitionDefect::UnknownSegment,
            });
        }
    }
    if let Some(extra) = complements.get(views.len()) {
        return Err(SegmentError::MalformedPartition {
            segment: extra.segment,
            defect: PartitionDefect::SegmentRepeated,
        });
    }
    for complement in complements {
        check_complement(complement, &index)?;
    }
    Ok(index)
}

/// Check that every entry of `complement` is about an element its segment
/// owns: a node's entries, an edge's by the owner of its source, a
/// synthesized element's by the owner of its chain.
fn check_complement(
    complement: &GraphComplementSegment,
    index: &OwnerIndex,
) -> Result<(), SegmentError> {
    let holder = complement.segment;
    let place = |elem: ViewRef, node: u32| match index.owner_of_node(node) {
        None => Err(SegmentError::Unowned { elem }),
        Some(owner) if owner != holder => Err(SegmentError::WrongOwner {
            elem,
            expected: owner,
            found: holder,
        }),
        Some(_) => Ok(()),
    };
    let nodes = complement
        .dropped_nodes
        .keys()
        .chain(complement.original_anchors.keys())
        .chain(complement.original_extra_fields.keys())
        .chain(complement.original_values.keys())
        .chain(complement.synthesized_nodes.keys());
    for id in nodes {
        place(ViewRef::Node(*id), *id)?;
    }
    for (id, edge) in complement
        .dropped_edges
        .iter()
        .chain(&complement.original_edges)
    {
        place(ViewRef::Edge(*id), edge.src)?;
    }
    for (id, key) in &complement.synthesized_edges {
        let chain = match key {
            SynthKey::Contract { src, .. } | SynthKey::ContractExpand { src, .. } => Some(*src),
            SynthKey::Expand { source_edge, .. } => complement
                .dropped_edges
                .get(source_edge)
                .map(|edge| edge.src),
            SynthKey::ResolutionReference { .. } | SynthKey::ResolutionSpan { .. } => None,
        };
        match chain {
            Some(src) => place(ViewRef::Edge(*id), src)?,
            None => {
                return Err(SegmentError::Unowned {
                    elem: ViewRef::Edge(*id),
                });
            }
        }
    }
    Ok(())
}

/// A complement segment as a whole-state complement, with the recorded
/// source form `borrowed` of every view edge the segment holds whose record
/// another segment keeps.
fn whole_complement(
    complement: &GraphComplementSegment,
    borrowed: &BTreeMap<EdgeId, GEdge>,
) -> GraphComplement {
    let mut whole = GraphComplement::empty();
    whole.dropped_nodes.clone_from(&complement.dropped_nodes);
    whole.dropped_edges.clone_from(&complement.dropped_edges);
    whole.original_edges.clone_from(&complement.original_edges);
    for (id, edge) in borrowed {
        whole.original_edges.insert(*id, edge.clone());
    }
    whole
        .original_anchors
        .clone_from(&complement.original_anchors);
    whole
        .original_extra_fields
        .clone_from(&complement.original_extra_fields);
    whole
        .original_values
        .clone_from(&complement.original_values);
    whole
        .synthesized_nodes
        .clone_from(&complement.synthesized_nodes);
    whole
        .synthesized_edges
        .clone_from(&complement.synthesized_edges);
    whole.id_floor = complement.floors;
    whole
}

/// The view segment's owned nodes and held edges, with a placeholder
/// carrying the view anchor for each ghost, and the ids of the
/// placeholders.
fn local_view(view: &GraphSegment) -> Result<(GInstance, BTreeSet<u32>), SegmentError> {
    let mut local = GInstance::new();
    for node in view.nodes.values() {
        local = local.with_node(node.clone());
    }
    let mut foreign = BTreeSet::new();
    for ghost in view.boundary.ghosts.values() {
        if local.node(ghost.node).is_none() {
            local = local.with_node(Node::new(ghost.node, ghost.anchor.clone()));
            foreign.insert(ghost.node);
        }
    }
    for (id, edge) in &view.edges {
        local
            .insert_edge(*id, edge.clone())
            .map_err(|_| SegmentError::DanglingEndpoint {
                elem: ViewRef::Edge(*id),
                node: edge.tgt,
            })?;
    }
    Ok((local, foreign))
}

/// The remote nodes a segment's restoration reads: the targets of its view
/// edges and of its dropped edges, and the endpoints of its synthesized
/// chains, that another segment owns.
fn requests(
    view: &GraphSegment,
    complement: &GraphComplement,
    index: &OwnerIndex,
) -> BTreeSet<u32> {
    let mut nodes: BTreeSet<u32> = view.edges.values().map(|edge| edge.tgt).collect();
    nodes.extend(
        complement
            .dropped_edges
            .values()
            .flat_map(|edge| [edge.src, edge.tgt]),
    );
    for key in complement
        .synthesized_nodes
        .values()
        .chain(complement.synthesized_edges.values())
    {
        if let SynthKey::Contract { src, tgt, .. } | SynthKey::ContractExpand { src, tgt, .. } = key
        {
            nodes.extend([*src, *tgt]);
        }
    }
    nodes.retain(|node| index.owner_of_node(*node) != Some(view.segment));
    nodes
}

/// [`requests`] of a view segment against its complement segment.
fn requests_of(
    view: &GraphSegment,
    complement: &GraphComplementSegment,
    index: &OwnerIndex,
) -> BTreeSet<u32> {
    let mut whole = GraphComplement::empty();
    whole.dropped_edges.clone_from(&complement.dropped_edges);
    whole
        .synthesized_nodes
        .clone_from(&complement.synthesized_nodes);
    whole
        .synthesized_edges
        .clone_from(&complement.synthesized_edges);
    requests(view, &whole, index)
}

/// A crossing of one restored edge: the segment holding it, its id, its
/// in-stub, the owner of its target and the target's anchor.
type Crossing = (SegmentId, EdgeId, InStub, SegmentId, panproto_gat::Name);

/// One segment's restored segment: the restored nodes it owns and the
/// restored edges it holds, with a ghost for each target owned elsewhere,
/// and the crossings of those edges, which become in-stubs of the targets'
/// owners.
fn restored_segment(
    segment: SegmentId,
    outcome: &Restored,
    index: &OwnerIndex,
) -> Result<(GraphSegment, Vec<Crossing>), SegmentedPutError> {
    let mut restored = GraphSegment {
        segment,
        nodes: BTreeMap::new(),
        edges: BTreeMap::new(),
        boundary: Boundary::default(),
    };
    for (id, node) in outcome.source.nodes() {
        if index.owner_of_node(*id) == Some(segment) {
            restored.nodes.insert(*id, node.clone());
        }
    }
    let mut crossings = Vec::new();
    for (id, edge) in outcome.source.edges() {
        restored.edges.insert(*id, edge.clone());
        if restored.nodes.contains_key(&edge.tgt) {
            continue;
        }
        let owner = index
            .owner_of_node(edge.tgt)
            .ok_or(SegmentedPutError::Segment(SegmentError::Unowned {
                elem: ViewRef::Edge(*id),
            }))?;
        let anchor = |node: u32| {
            outcome
                .source
                .node(node)
                .map(|node| node.anchor.clone())
                .unwrap_or_default()
        };
        crossings.push((
            segment,
            *id,
            InStub {
                src: edge.src,
                src_owner: segment,
                src_anchor: anchor(edge.src),
                tgt: edge.tgt,
                label: edge.edge.clone(),
            },
            owner,
            anchor(edge.tgt),
        ));
    }
    for (_, _, stub, owner, anchor) in &crossings {
        restored.boundary.ghosts.entry(stub.tgt).or_insert(Ghost {
            node: stub.tgt,
            owner: *owner,
            anchor: anchor.clone(),
        });
    }
    Ok((restored, crossings))
}

/// Add each crossing's in-stub to the boundary of its target's owner.
fn add_in_stubs(boundaries: &mut [Boundary], crossings: Vec<Crossing>) {
    for (_, id, stub, owner, _) in crossings {
        if let Some(boundary) = boundaries.get_mut(usize::try_from(owner.0).unwrap_or(usize::MAX)) {
            boundary.in_stubs.insert(ViewRef::Edge(id), stub);
        }
    }
}

/// The restored family: each segment's restored owned nodes and held
/// edges, with the boundary its crossing edges give it.
fn restored_family(
    parts: &[Part],
    restored: &[Restored],
    index: &OwnerIndex,
) -> Result<Vec<GraphSegment>, SegmentedPutError> {
    let mut family: Vec<GraphSegment> = Vec::with_capacity(parts.len());
    let mut crossings = Vec::new();
    for (part, outcome) in parts.iter().zip(restored) {
        let (segment, crossed) = restored_segment(part.view.segment, outcome, index)?;
        crossings.extend(crossed);
        family.push(segment);
    }
    let mut boundaries: Vec<Boundary> = family
        .iter()
        .map(|segment| segment.boundary.clone())
        .collect();
    add_in_stubs(&mut boundaries, crossings);
    for (segment, boundary) in family.iter_mut().zip(boundaries) {
        segment.boundary = boundary;
    }
    Ok(family)
}

/// The restored parts of a put, one segment at a time: what the clauses
/// read on the restored source read of each segment.
trait RestoredParts {
    /// Why reading a segment stopped.
    type Stop: From<SegmentedPutError>;

    /// The segments, in id order.
    fn ids(&self) -> Vec<SegmentId>;

    /// Apply `f` to segment `segment`'s part and restoration.
    fn with_restored<T>(
        &self,
        segment: SegmentId,
        f: impl FnOnce(&Part, &Restored) -> T,
    ) -> Result<T, Self::Stop>;

    /// Segment `segment` of the restored family, with its boundary.
    fn family_segment(&self, segment: SegmentId) -> Result<Cow<'_, GraphSegment>, Self::Stop>;
}

/// The restored parts of a put held in memory.
struct InMemory<'a> {
    parts: &'a [Part],
    restored: &'a [Restored],
    family: &'a [GraphSegment],
}

fn position(segment: SegmentId) -> usize {
    usize::try_from(segment.0).unwrap_or(usize::MAX)
}

impl RestoredParts for InMemory<'_> {
    type Stop = SegmentedPutError;

    fn ids(&self) -> Vec<SegmentId> {
        self.parts.iter().map(|part| part.view.segment).collect()
    }

    fn with_restored<T>(
        &self,
        segment: SegmentId,
        f: impl FnOnce(&Part, &Restored) -> T,
    ) -> Result<T, Self::Stop> {
        let at = position(segment);
        match (self.parts.get(at), self.restored.get(at)) {
            (Some(part), Some(outcome)) => Ok(f(part, outcome)),
            _ => Err(SegmentedPutError::Segment(
                SegmentError::MalformedPartition {
                    segment,
                    defect: PartitionDefect::SegmentMissing,
                },
            )),
        }
    }

    fn family_segment(&self, segment: SegmentId) -> Result<Cow<'_, GraphSegment>, Self::Stop> {
        self.family
            .get(position(segment))
            .map(Cow::Borrowed)
            .ok_or(SegmentedPutError::Segment(
                SegmentError::MalformedPartition {
                    segment,
                    defect: PartitionDefect::SegmentMissing,
                },
            ))
    }
}

/// The clauses read on the restored source: forward stability, the source
/// schema's required cells and keys, and the fan domain.
fn source_clauses<R: RestoredParts>(
    lens: &Lens,
    plan: &RestrictPlan,
    held: &R,
) -> Result<Vec<PutDomainError>, R::Stop> {
    let ids = held.ids();
    let mut found = Vec::new();
    let mut stability = Vec::new();
    let outcome = graph_restrict_segmented_nodes_with(
        plan,
        &ids,
        &mut |segment| held.family_segment(segment),
        &mut |segment, nodes| {
            stability.extend(held.with_restored(segment, |part, outcome| {
                let in_view = restorable_part(&part.local, &outcome.errors);
                stability_against(
                    lens,
                    &in_view,
                    &part.complement,
                    &outcome.source,
                    plan,
                    &part.scope,
                    &|id| nodes.get(&id),
                )
            })?);
            Ok(())
        },
    );
    match outcome {
        Ok(()) => found.extend(stability),
        Err(StreamStop::Source(stop)) => return Err(stop),
        Err(StreamStop::Refused(SegmentedRestrictError::Restrict { error, .. })) => {
            found.extend(refusal_from_restrict(error));
        }
        Err(StreamStop::Refused(SegmentedRestrictError::Segment(error))) => {
            return Err(R::Stop::from(SegmentedPutError::Segment(error)));
        }
        Err(StreamStop::Refused(SegmentedRestrictError::Residual(residual))) => {
            return Err(R::Stop::from(SegmentedPutError::Residual(residual)));
        }
        Err(StreamStop::Refused(other)) => {
            return Err(R::Stop::from(SegmentedPutError::Lens(
                LensError::ComplementMismatch {
                    detail: other.to_string(),
                },
            )));
        }
    }
    let schema = &lens.src_schema;
    let mut keys = KeyRuns::default();
    for segment in ids {
        let restored = held.family_segment(segment)?;
        let violations = held.with_restored(segment, |part, outcome| {
            let mine = |id: u32| restored.nodes.contains_key(&id);
            let in_view = restorable_part(&part.local, &outcome.errors);
            let mut violations: Vec<PutDomainError> = required_violations(&outcome.source, schema)
                .into_iter()
                .filter(|error| matches!(error, PutDomainError::MissingRequiredCell { elem: ViewRef::Node(id), .. } if mine(*id)))
                .collect();
            keys.add(&outcome.source, schema, &mine, &|id| {
                in_view.nodes().contains_key(&id) && !part.scope.foreign.contains(&id)
            });
            violations.extend(
                fan_violations(&fan_graph(&outcome.source, &restored)?, schema)
                    .into_iter()
                    .filter(|error| matches!(error, PutDomainError::MalformedFan { node, .. } if mine(*node))),
            );
            Ok::<_, SegmentedPutError>(violations)
        })?;
        found.extend(violations.map_err(R::Stop::from)?);
    }
    found.extend(keys.finish());
    Ok(found)
}

/// Graph put along a lens as a [`PhaseOperation`] of the checkpointed
/// runner, over the view segments the input manifest seals and the
/// complement segments `complements` holds.
///
/// Its phases, each one task per segment except the coordinator's
/// Assemble:
///
/// - Validate reads a view segment (checked against the view manifest by
///   the runner) and its complement segment's canonical bytes
///   ([`GraphComplementSegment::from_canonical_bytes`]), checks that every
///   complement entry is about an element the segment owns, and writes the
///   eight-byte length of the view bytes, the view bytes and the complement
///   bytes. The complement's digest is among the task's inputs, and the
///   working set of a segment is its view's estimate plus its complement's
///   length.
/// - Export writes the segment's census runs (held view edge ids, dropped,
///   synthesized and recorded edge ids with the recorded edges, colliding
///   node ids) and its node-fact requests by owner.
/// - Summarize answers every request addressed to the segment with the
///   node's fact (its view anchor, its restored anchor and value), restored
///   from the segment's own view and complement.
/// - Assemble merges the census and refuses the least id collision; then
///   restores every segment from its part, the census and its owners'
///   facts, one segment at a time, and checks the clauses read on the
///   restored source (forward stability by
///   [`graph_restrict_segmented_nodes_with`], fetching each restored
///   segment by restoring it again; required cells, keys and the fan
///   domain). The least violation is the run's refusal, so a refused put
///   emits and seals nothing; otherwise its artifact is the boundary of
///   every restored segment.
/// - Emit restores the segment again and writes it with its boundary from
///   the assembly; it emits no complement.
/// - Seal seals the restored segments as a source.
///
/// The coordinator holds the census runs, the boundaries, the synthesized
/// keys of the forward-stability run and the key runs, which grow with the
/// boundary, the edge ids and the keyed elements; never two segments.
pub struct GraphPut<'a, C> {
    lens: &'a Lens,
    plan: &'a RestrictPlan,
    complements: &'a C,
}

type PutStop = PhaseStop<SegmentedPutError>;

impl<C: SegmentSource> GraphPut<'_, C> {
    fn validated(
        cx: &PhaseContext<'_>,
        segment: SegmentId,
    ) -> Result<(GraphSegment, GraphComplementSegment), PutStop> {
        let bytes = cx.artifact(Phase::Validate, Some(segment))?;
        let mut r = Reader::new(&bytes, ArtifactKind::Segment);
        let len = usize::try_from(r.u64()?).unwrap_or(usize::MAX);
        let view = r.take(len)?;
        let complement = r.take(bytes.len().saturating_sub(8).saturating_sub(len))?;
        r.finish()?;
        let AnySegment::Graph(view) = AnySegment::from_canonical_bytes(view, cx.partition())?.0
        else {
            return Err(PutStop::from(SegmentError::Decode {
                artifact: ArtifactKind::Segment,
                message: "not a graph segment".to_owned(),
            }));
        };
        let complement = GraphComplementSegment::from_canonical_bytes(complement, segment)?;
        Ok((view, complement))
    }

    fn exports(cx: &PhaseContext<'_>) -> Result<Vec<(SegmentId, Exported)>, PutStop> {
        cx.segments()
            .into_iter()
            .map(|segment| {
                let bytes = cx.artifact(Phase::Export, Some(segment))?;
                Ok((segment, Exported::decode(&bytes)?))
            })
            .collect()
    }

    /// Segment `segment`'s part, with the facts its owners offered.
    fn part(
        &self,
        cx: &PhaseContext<'_>,
        segment: SegmentId,
        exports: &[(SegmentId, Exported)],
    ) -> Result<Part, PutStop> {
        let (view, complement) = Self::validated(cx, segment)?;
        let census = census_part(&view, exports.iter().map(|(id, e)| (*id, e)));
        let mut part = Part::new(self.lens, view, &complement, census)?;
        let mut offered: BTreeMap<SegmentId, BTreeMap<u32, NodeFact>> = BTreeMap::new();
        let owners: BTreeSet<SegmentId> = exports
            .iter()
            .find(|(id, _)| *id == segment)
            .map(|(_, exported)| exported.requests.keys().copied().collect())
            .unwrap_or_default();
        for owner in owners {
            let bytes = cx.artifact(Phase::Summarize, Some(owner))?;
            offered.insert(owner, decode_facts(&bytes)?);
        }
        let index = cx.index();
        part.take(index, &|node| {
            offered
                .get(&index.owner_of_node(node)?)?
                .get(&node)
                .cloned()
        });
        Ok(part)
    }
}

/// The restored parts of a put run, each restored again whenever it is
/// read.
struct Rerun<'r, 'c, C> {
    put: &'r GraphPut<'r, C>,
    cx: &'r PhaseContext<'c>,
    exports: &'r [(SegmentId, Exported)],
    boundaries: &'r [Boundary],
}

impl<C: SegmentSource> RestoredParts for Rerun<'_, '_, C> {
    type Stop = PutStop;

    fn ids(&self) -> Vec<SegmentId> {
        self.cx.segments()
    }

    fn with_restored<T>(
        &self,
        segment: SegmentId,
        f: impl FnOnce(&Part, &Restored) -> T,
    ) -> Result<T, Self::Stop> {
        let part = self.put.part(self.cx, segment, self.exports)?;
        let (_, outcome) = part.restore(self.put.lens, self.put.plan);
        Ok(f(&part, &outcome))
    }

    fn family_segment(&self, segment: SegmentId) -> Result<Cow<'_, GraphSegment>, Self::Stop> {
        let part = self.put.part(self.cx, segment, self.exports)?;
        let (_, outcome) = part.restore(self.put.lens, self.put.plan);
        let (mut restored, _) = restored_segment(segment, &outcome, self.cx.index())?;
        restored.boundary = self
            .boundaries
            .get(position(segment))
            .cloned()
            .unwrap_or_default();
        Ok(Cow::Owned(restored))
    }
}

impl From<SegmentedPutError> for PhaseStop<SegmentedPutError> {
    fn from(error: SegmentedPutError) -> Self {
        Self::Failed(error)
    }
}

/// The Assemble artifact of a put: the number of segments and each
/// restored segment's boundary, in segment order.
fn boundaries_bytes(boundaries: &[Boundary]) -> Vec<u8> {
    let mut sink = CanonicalWriter::new();
    sink.len(boundaries.len());
    for boundary in boundaries {
        write_boundary(&mut sink, boundary);
    }
    sink.into_bytes()
}

fn decode_boundaries(bytes: &[u8]) -> Result<Vec<Boundary>, SegmentError> {
    let mut r = Reader::new(bytes, ArtifactKind::Assembly);
    let n = r.count(16)?;
    let mut boundaries = Vec::with_capacity(n);
    for _ in 0..n {
        boundaries.push(read_boundary(&mut r)?);
    }
    r.finish()?;
    Ok(boundaries)
}

impl<C: SegmentSource> PhaseOperation for GraphPut<'_, C> {
    type Error = SegmentedPutError;

    fn op(&self) -> &'static [u8] {
        GRAPH_PUT_OP
    }

    fn segment_op(&self) -> SegmentOp {
        SegmentOp::Put
    }

    fn locality(&self) -> Locality {
        put_locality(self.lens, self.plan)
    }

    fn working_set(&self, record: &SegmentRecord) -> Result<u64, String> {
        let complement = self
            .complements
            .byte_len(record.id)
            .map_err(|error| error.to_string())?;
        Ok(MemoryModel::estimate(&record.stats).saturating_add(complement))
    }

    fn validate_inputs(&self, segment: SegmentId) -> Result<Vec<Digest>, String> {
        let bytes = self
            .complements
            .read(segment)
            .map_err(|error| error.to_string())?;
        Ok(vec![Digest::derive(SEGMENT_COMPLEMENT_CONTEXT, &bytes)])
    }

    fn validate(
        &self,
        cx: &PhaseContext<'_>,
        segment: SegmentId,
        bytes: Vec<u8>,
    ) -> Result<Vec<u8>, PutStop> {
        let complement = self.complements.read(segment).map_err(store_failure)?;
        let decoded = GraphComplementSegment::from_canonical_bytes(&complement, segment)?;
        check_complement(&decoded, cx.index())?;
        let mut sink = CanonicalWriter::new();
        sink.u64(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        sink.write(&bytes);
        sink.write(&complement);
        Ok(sink.into_bytes())
    }

    fn export(&self, cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Vec<u8>, PutStop> {
        let (view, complement) = Self::validated(cx, segment)?;
        Ok(Exported::of(&view, &complement, cx.index()).bytes())
    }

    fn summarize(&self, cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Vec<u8>, PutStop> {
        let (view, complement) = Self::validated(cx, segment)?;
        let mut wanted = BTreeSet::new();
        let mut census = CensusPart::default();
        for id in cx.segments() {
            let exported = Exported::decode(&cx.artifact(Phase::Export, Some(id))?)?;
            if let Some(nodes) = exported.requests.get(&segment) {
                wanted.extend(nodes.iter().copied());
            }
            census.add(&view, id, &exported);
        }
        let part = Part::new(self.lens, view, &complement, census)?;
        Ok(facts_bytes(&part.offer(self.lens, &wanted)))
    }

    fn assemble(&self, cx: &PhaseContext<'_>) -> Result<Vec<u8>, PutStop> {
        let exports = Self::exports(cx)?;
        let collisions = census_collisions(exports.iter().map(|(id, e)| (*id, e)))?;
        if let Some(least) = least(collisions) {
            return Err(PutStop::Failed(refused(least)));
        }
        let ids = cx.segments();
        let mut found: Vec<PutDomainError> = Vec::new();
        let mut traced = true;
        let mut boundaries: Vec<Boundary> = vec![Boundary::default(); ids.len()];
        let mut crossings = Vec::new();
        let mut family_error: Option<SegmentedPutError> = None;
        for &segment in &ids {
            let part = self.part(cx, segment, &exports)?;
            let (violations, outcome) = part.restore(self.lens, self.plan);
            found.extend(violations);
            traced &= outcome.trace_errors.is_empty();
            if family_error.is_some() {
                continue;
            }
            match restored_segment(segment, &outcome, cx.index()) {
                Ok((restored, crossed)) => {
                    if let Some(boundary) = boundaries.get_mut(position(segment)) {
                        boundary.ghosts = restored.boundary.ghosts;
                    }
                    crossings.extend(crossed);
                }
                Err(error) => family_error = Some(error),
            }
        }
        if traced {
            if let Some(error) = family_error {
                return Err(PutStop::Failed(error));
            }
            add_in_stubs(&mut boundaries, crossings);
            let rerun = Rerun {
                put: self,
                cx,
                exports: &exports,
                boundaries: &boundaries,
            };
            found.extend(source_clauses(self.lens, self.plan, &rerun)?);
        }
        match least(found) {
            Some(error) => Err(PutStop::Failed(refused(error))),
            None => Ok(boundaries_bytes(&boundaries)),
        }
    }

    fn emit(&self, cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Emission, PutStop> {
        let boundaries = decode_boundaries(&cx.artifact(Phase::Assemble, None)?)?;
        let exports = Self::exports(cx)?;
        let part = self.part(cx, segment, &exports)?;
        let (_, outcome) = part.restore(self.lens, self.plan);
        let (mut restored, _) = restored_segment(segment, &outcome, cx.index())?;
        restored.boundary = boundaries
            .into_iter()
            .nth(position(segment))
            .ok_or_else(|| SegmentError::Decode {
                artifact: ArtifactKind::Assembly,
                message: "a boundary is missing".to_owned(),
            })?;
        Ok(Emission {
            segment: restored,
            complement: None,
        })
    }

    fn output_role(&self, _: &PhaseContext<'_>) -> ManifestRole {
        ManifestRole::Source
    }
}

/// The outcome of [`put_segmented_run`].
pub type GraphPutRun = SegmentedRun<SegmentedPutError>;

/// A view held as segments with the complement segments of the get that
/// made it.
pub struct HeldView<'a, Src, C> {
    /// The manifest sealing the view segments: the view manifest of a
    /// segmented get, or of an edited view under the same partition.
    pub manifest: &'a SegmentManifest,
    /// The manifest's trusted digest.
    pub input: Digest,
    /// The view segments' canonical bytes.
    pub views: &'a Src,
    /// The canonical bytes of each segment's complement segment
    /// ([`GraphComplementSegment::canonical_bytes`]).
    pub complements: &'a C,
}

impl<Src, C> Clone for HeldView<'_, Src, C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Src, C> Copy for HeldView<'_, Src, C> {}

/// Backward graph-lens direction over a view held as segments, as a
/// checkpointed run of [`GraphPut`]. The run's lens identity is
/// the graph identity of `lens` ([`Lens::identity`]) and the restored
/// segments are written under its source schema digest, so the run identity
/// is a function of the lens and the view; it stops once
/// `cancel_after_tasks` tasks are done, and a later call against the same
/// store resumes it. A complete run emits each restored segment, read back
/// with [`read_emitted`](panproto_inst::segment::read_emitted); they are
/// [`put_segmented`] of the view segments against the complement segments.
/// A refused put ends at Assemble with the least violation, before any
/// Emit task, so it seals nothing.
///
/// # Errors
///
/// The plan compilation error, as [`put_segmented`] reports it.
pub fn put_segmented_run<Src: SegmentSource, C: SegmentSource, St: SegmentStore>(
    lens: &Lens,
    view: &HeldView<'_, Src, C>,
    budget: Budget,
    store: &mut St,
    cancel_after_tasks: Option<u32>,
) -> Result<GraphPutRun, SegmentedPutError> {
    let HeldView {
        manifest,
        input,
        views,
        complements,
    } = *view;
    let plan = compile(lens)?;
    let identity = lens.identity(ComplementShape::Graph);
    let spec = RunSpec {
        lens: identity.lens,
        input,
        output_schema: identity.source_schema,
        cancel_after_tasks,
    };
    let put = GraphPut {
        lens,
        plan: &plan,
        complements,
    };
    Ok(run_segmented(
        &put,
        manifest,
        views,
        spec,
        budget,
        store,
        &mut |_| {},
    ))
}

/// A segment's restored graph with every restored edge entering it from
/// another segment, its source held as a placeholder, so each owned node
/// sees all of its in-edges.
fn fan_graph(source: &GInstance, segment: &GraphSegment) -> Result<GInstance, SegmentedPutError> {
    let mut graph = source.clone();
    for (elem, stub) in &segment.boundary.in_stubs {
        let ViewRef::Edge(id) = elem else {
            continue;
        };
        if graph.node(stub.src).is_none() {
            graph = graph.with_node(Node::new(stub.src, stub.src_anchor.clone()));
        }
        graph
            .insert_edge(*id, GEdge::new(stub.src, stub.tgt, stub.label.clone()))
            .map_err(|_| {
                SegmentedPutError::Segment(SegmentError::InStubMismatch { elem: elem.clone() })
            })?;
    }
    Ok(graph)
}

/// The sorted runs of key values of every segment, merged per key.
#[derive(Default)]
struct KeyRuns {
    /// Per key (vertex and key id, with its parts): the canonical bytes, id
    /// and values of each element with a complete tuple, and the least
    /// element whose tuple the keying kernel refuses, with the refusal.
    runs: BTreeMap<(panproto_gat::Name, panproto_gat::Name), KeyRun>,
    /// The primary-key parts missing at restored view elements.
    missing: Vec<PutDomainError>,
}

struct KeyRun {
    parts: Vec<KeyPart>,
    entries: Vec<(Vec<u8>, u32, Vec<panproto_inst::Value>)>,
    refused: Option<(u32, PutDomainError)>,
}

impl KeyRuns {
    /// Add one segment's run: the elements it owns (`mine`) of every keyed
    /// vertex of `source`, and the primary-key parts missing at those that
    /// `in_view` admits.
    fn add(
        &mut self,
        source: &GInstance,
        schema: &Schema,
        mine: &dyn Fn(u32) -> bool,
        in_view: &dyn Fn(u32) -> bool,
    ) {
        for (vertex, keys) in &schema.declarations.keys {
            let mut ids: Vec<u32> = source
                .nodes()
                .iter()
                .filter_map(|(&id, node)| (&node.anchor == vertex && mine(id)).then_some(id))
                .collect();
            ids.sort_unstable();
            for key in keys {
                if uniqueness_role(&key.role) {
                    let run = self
                        .runs
                        .entry((vertex.clone(), key.id.clone()))
                        .or_insert_with(|| KeyRun {
                            parts: key.parts.clone(),
                            entries: Vec::new(),
                            refused: None,
                        });
                    for &id in &ids {
                        if source
                            .node(id)
                            .is_some_and(|node| node.shape == NodeShape::HyperEdge)
                        {
                            continue;
                        }
                        let Some(values) = key_values(source, id, &key.parts) else {
                            continue;
                        };
                        match panproto_inst::key::canonical_key_bytes(key, &values) {
                            Ok(bytes) => run.entries.push((bytes, id, values)),
                            Err(error) => {
                                if run.refused.as_ref().is_none_or(|(least, _)| id < *least) {
                                    run.refused = Some((
                                        id,
                                        PutDomainError::KeyRefused {
                                            table: vertex.clone(),
                                            key: key.id.clone(),
                                            reason: error.into(),
                                        },
                                    ));
                                }
                                break;
                            }
                        }
                    }
                }
                if key.role == KeyRole::Primary {
                    for &id in ids.iter().filter(|&&id| in_view(id)) {
                        for part in &key.parts {
                            if super::part_value(source, id, part).is_none() {
                                self.missing.push(PutDomainError::MissingRequiredCell {
                                    elem: ViewRef::Node(id),
                                    field: super::key_part_name(part),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    /// Merge the runs: a key whose kernel refuses an element reports the
    /// refusal at the least such element and nothing else; otherwise every
    /// group of elements with equal key bytes is a duplicate, with the
    /// values of its least element.
    fn finish(self) -> Vec<PutDomainError> {
        let mut found = self.missing;
        for ((vertex, _), run) in self.runs {
            if let Some((_, refusal)) = run.refused {
                found.push(refusal);
                continue;
            }
            let mut entries = run.entries;
            entries.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
            let mut at = 0;
            while at < entries.len() {
                let end = entries[at..]
                    .iter()
                    .position(|entry| entry.0 != entries[at].0)
                    .map_or(entries.len(), |offset| at + offset);
                if end - at > 1 {
                    found.push(PutDomainError::DuplicateKey {
                        table: vertex.clone(),
                        key: run
                            .parts
                            .iter()
                            .map(super::key_part_name)
                            .zip(entries[at].2.clone())
                            .collect(),
                        elems: entries[at..end]
                            .iter()
                            .map(|entry| ViewRef::Node(entry.1))
                            .collect(),
                    });
                }
                at = end;
            }
        }
        found
    }
}
