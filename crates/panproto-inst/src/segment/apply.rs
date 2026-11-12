//! Applying a delta to an instance held as segments.
//!
//! A delta names every element it deletes, adds or changes by id, so the
//! segments it can change are known from the delta and the input manifest
//! alone ([`SegmentDelta::read_set`]). A segmented apply reads those
//! segments and no other; every other segment keeps its bytes and its
//! manifest record.
//!
//! Each segment it reads reports its *facts*: the elements it holds that
//! the whole-state checks of the delta read ([`SegmentDelta::facts`]). One
//! coordinator glues the facts into a small instance and runs the
//! whole-state checks on it, in their order, returning the whole-state
//! refusal before any segment is rewritten ([`SegmentDelta::check_facts`]). Each
//! read segment then rewrites itself ([`SegmentDelta::edit`]): its own
//! elements as the delta changes them, and its boundary with the anchors
//! after the delta, which the facts carry for every element a new ghost or
//! in-stub names.
//!
//! Graph deltas ([`GraphDelta`]) and table deltas ([`TableDelta`]) are
//! segment deltas. [`apply_segmented`] applies one in memory, and
//! [`apply_run`] as a checkpointed run of the phase operation
//! [`SegmentApply`].
//!
//! [`GraphDelta`]: crate::graph_edit::GraphDelta
//! [`TableDelta`]: crate::table_delta::TableDelta

use std::collections::{BTreeMap, BTreeSet};

use panproto_schema::InstanceShapeKind;

use super::body::AnySegment;
use super::error::SegmentError;
use super::ids::{EdgeFloor, IdAllocator, IdKind, IdRange, SegmentId};
use super::manifest::{ManifestRole, SegmentManifest};
use super::partition::{OwnerIndex, Partition, SegmentSpec};
use super::runner::{
    Budget, Emission, PhaseContext, PhaseOperation, PhaseStop, Progress, RunSpec, SegmentSource,
    SegmentedRun, run_segmented,
};
use super::store::{ArtifactKind, Phase, SegmentStore};
use crate::edit_error::EditError;
use crate::ginstance::EdgeId;
use crate::identity::Digest;
use crate::locality::{Locality, SegmentOp};

/// Why a segmented delta application returned no output.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SegmentedApplyError {
    /// The family is invalid, or the edited instance cannot be laid out and
    /// sealed under the partition: an added node or row no range owns, or
    /// an added edge id the seal floor rule refuses.
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

/// What the read set of a delta is computed from, for one family of
/// segments.
///
/// It holds the owner of each node or row id, the seal floor rule of the
/// edge ids, each segment's largest edge id and the peers of each segment's
/// crossing edges (or crossing pairs).
///
/// It is read off a manifest's records ([`FamilyIndex::of_manifest`]) or off
/// the segments themselves ([`FamilyIndex::of_segments`]), which give the
/// same index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FamilyIndex {
    index: OwnerIndex,
    pub(super) floor: EdgeFloor,
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
        segments: &[AnySegment],
        partition: &Partition,
    ) -> Result<Self, SegmentError> {
        let mut family = Self::new(partition)?;
        for segment in segments {
            let id = segment.segment();
            if let Some(max) = segment.max_edge() {
                family.max_edge.insert(id, max);
            }
            let boundary = segment.boundary();
            family
                .cross_out
                .insert(id, boundary.ghosts.values().map(|g| g.owner).collect());
            family.cross_in.insert(
                id,
                boundary.in_stubs.values().map(|s| s.src_owner).collect(),
            );
        }
        Ok(family)
    }

    /// The segment owning node (or row) `node`, `None` when no range
    /// contains it.
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

    /// The segments the crossing edges (or pairs) of `segment` enter: the
    /// owners of its ghosts.
    pub(super) fn peers_out(&self, segment: SegmentId) -> impl Iterator<Item = SegmentId> + '_ {
        self.cross_out.get(&segment).into_iter().flatten().copied()
    }

    /// The segments whose crossing edges (or pairs) enter `segment`: the
    /// owners of the sources of its in-stubs.
    pub(super) fn peers_in(&self, segment: SegmentId) -> impl Iterator<Item = SegmentId> + '_ {
        self.cross_in.get(&segment).into_iter().flatten().copied()
    }

    /// The segments a segmented apply of `delta` reads, and the only ones
    /// it can rewrite ([`SegmentDelta::read_set`]).
    #[must_use]
    pub fn read_set<D: SegmentDelta>(&self, delta: &D) -> BTreeSet<SegmentId> {
        delta.read_set(self)
    }
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for crate::graph_edit::GraphDelta {}
    impl Sealed for crate::table_delta::TableDelta {}
}

/// A delta that a family of segments applies segment by segment, reading
/// only the segments it can change.
///
/// The facts a segment reports, and the refusal and rewriting computed from
/// them, are exchanged as canonical bytes, the form a checkpointed run
/// commits them in. The trait is sealed: it is implemented by
/// [`GraphDelta`] and [`TableDelta`], whose implementations carry the proofs
/// of the segmented edit laws.
///
/// [`GraphDelta`]: crate::graph_edit::GraphDelta
/// [`TableDelta`]: crate::table_delta::TableDelta
pub trait SegmentDelta: sealed::Sealed {
    /// The segments of the instances the delta applies to.
    type Segment: Clone;

    /// The shape of those instances.
    const SHAPE: InstanceShapeKind;

    /// The operation bytes of its application in a run's identity
    /// ([`RunJournal::run_digest`](super::RunJournal::run_digest)).
    const OP: &'static [u8];

    /// The digest naming the delta in a run's identity.
    fn digest(&self) -> Digest;

    /// The segments a segmented apply of the delta reads, and the only ones
    /// it can rewrite, computed from the delta and `family` alone.
    fn read_set(&self, family: &FamilyIndex) -> BTreeSet<SegmentId>;

    /// The canonical bytes of the facts `held` reports about the delta.
    fn facts(&self, held: &Self::Segment) -> Vec<u8>;

    /// The refusal of the delta on the family whose read segments reported
    /// `facts`: the whole-state refusal, then the refusal of laying the
    /// edited instance out under the family's partition.
    ///
    /// # Errors
    ///
    /// That refusal, and [`SegmentError::Decode`] for malformed facts.
    fn check_facts(
        &self,
        facts: &[Vec<u8>],
        family: &FamilyIndex,
    ) -> Result<(), SegmentedApplyError>;

    /// The segment `held` after the delta, once [`Self::check_facts`] has passed
    /// on `facts`, the facts of every read segment.
    ///
    /// # Errors
    ///
    /// [`SegmentError::Decode`] for malformed facts, and
    /// [`SegmentError::Unowned`] for an element whose owner no range
    /// gives, which a checked delta never names.
    fn edit(
        &self,
        held: &Self::Segment,
        facts: &[Vec<u8>],
        family: &FamilyIndex,
    ) -> Result<Self::Segment, SegmentError>;

    /// The id of `segment`.
    fn id(segment: &Self::Segment) -> SegmentId;

    /// `segment` as a segment of any shape.
    fn into_any(segment: Self::Segment) -> AnySegment;

    /// `segment` as a segment of this delta's shape, `None` when it is of
    /// another shape.
    fn from_any(segment: AnySegment) -> Option<Self::Segment>;
}

/// Apply `delta` to the instance held as `segments` under `partition`,
/// reading only the segments it can change.
///
/// The segments are checked as [`verify_partition`] checks them. The
/// segments of [`SegmentDelta::read_set`] report their facts, the
/// coordinator runs the whole-state checks on them, and each of those
/// segments is rewritten; every other segment is returned unchanged.
/// Gluing the output under `partition` gives whole-state application
/// ([`GraphDelta::apply`], [`TableDelta::apply`]) of the glued input, and
/// the output is the split of that instance under `partition`
/// ([`partition_graph`], [`partition_table`]).
///
/// [`verify_partition`]: super::verify_partition
/// [`GraphDelta::apply`]: crate::graph_edit::GraphDelta::apply
/// [`TableDelta::apply`]: crate::table_delta::TableDelta::apply
/// [`partition_graph`]: super::partition_graph
/// [`partition_table`]: super::partition_table
///
/// # Errors
///
/// [`SegmentedApplyError::Segment`] with the error of `verify_partition`;
/// then the refusal of [`SegmentDelta::check_facts`]: the whole-state refusal as
/// [`SegmentedApplyError::Edit`], then [`SegmentedApplyError::Segment`] with
/// [`SegmentError::Unowned`] for the least added node or row no range owns,
/// and, for a graph, with the seal floor rule's refusal
/// ([`EdgeFloor::check`]) for the least added edge id it refuses for the
/// owner of the edge's source.
pub fn apply_segmented<D: SegmentDelta>(
    delta: &D,
    segments: &[D::Segment],
    partition: &Partition,
) -> Result<Vec<D::Segment>, SegmentedApplyError> {
    let any: Vec<AnySegment> = segments.iter().cloned().map(D::into_any).collect();
    super::check::verify_partition(&any, partition)?;
    let family = FamilyIndex::of_segments(&any, partition)?;
    let read = family.read_set(delta);
    let held: BTreeMap<SegmentId, &D::Segment> = segments.iter().map(|s| (D::id(s), s)).collect();
    let facts: Vec<Vec<u8>> = read
        .iter()
        .filter_map(|id| held.get(id))
        .map(|segment| delta.facts(segment))
        .collect();
    delta.check_facts(&facts, &family)?;
    let mut output = Vec::with_capacity(segments.len());
    for segment in partition.specs().iter().filter_map(|s| held.get(&s.id)) {
        output.push(if read.contains(&D::id(segment)) {
            delta.edit(segment, &facts, &family)?
        } else {
            (*segment).clone()
        });
    }
    Ok(output)
}

/// Fresh ids for the nodes (or rows) and edges a delta adds to one segment,
/// drawn from the allocation ranges its partition assigns it.
///
/// A node id is drawn from the segment's node allocation range above every
/// node the segment owns in that range, so the segment owns it and holds no
/// node under it; an edge id from its edge allocation range above every edge
/// id the segment holds in that range, so no segment holds it and the seal
/// floor rule admits it for an edge whose source the segment owns. A delta
/// whose added nodes or rows take ids from the owner's allocator and whose
/// added edges take edge ids from the allocator of their source's owner is
/// refused by [`apply_segmented`] for no id it adds.
///
/// ```
/// use std::collections::BTreeMap;
///
/// use panproto_inst::graph_edit::GraphDelta;
/// use panproto_inst::segment::{
///     AnySegment, Boundary, FreshIds, GraphSegment, IdRange, SegmentId, SegmentSpec,
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
/// let segment = AnySegment::Graph(GraphSegment {
///     segment: SegmentId(0),
///     nodes: BTreeMap::from([(50, Node::new(50, "person"))]),
///     edges: BTreeMap::new(),
///     boundary: Boundary::default(),
/// });
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
    pub fn of(segment: &AnySegment, spec: &SegmentSpec) -> Result<Self, SegmentError> {
        let nodes = spec
            .alloc_nodes
            .map(|range| range.check(spec.id))
            .transpose()?
            .map(|range| {
                IdAllocator::from_source(range, segment.node_anchors().into_iter().map(|(n, _)| n))
            });
        let edges = spec
            .alloc_edges
            .map(|range| range.check(spec.id))
            .transpose()?
            .map(|range| {
                IdAllocator::from_source(range, segment.edge_id_run().into_iter().map(|e| e.0))
            });
        Ok(Self {
            segment: spec.id,
            nodes,
            edges,
        })
    }

    /// A fresh node (or row) id.
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

/// Delta application as a [`PhaseOperation`] over the segments the delta
/// can change ([`SegmentDelta::read_set`] of the input manifest, the
/// operation's affected set).
///
/// - Validate keeps each read segment's canonical bytes, checked against the
///   input manifest.
/// - Export writes nothing.
/// - Summarize writes the segment's facts ([`SegmentDelta::facts`]).
/// - Assemble (the coordinator) returns the refusal of
///   [`SegmentDelta::check_facts`] on the facts; otherwise it writes nothing. It
///   holds the facts, never a segment.
/// - Emit rewrites the segment ([`SegmentDelta::edit`]), reading the facts
///   for the anchors after the delta, without a complement.
/// - Seal reseals the input manifest with the rewritten records as a
///   source, keeping every other record.
pub struct SegmentApply<'d, D> {
    /// The delta.
    pub delta: &'d D,
}

impl<D: SegmentDelta> SegmentApply<'_, D> {
    fn validated(cx: &PhaseContext<'_>, segment: SegmentId) -> Result<D::Segment, ApplyStop> {
        let bytes = cx.artifact(Phase::Validate, Some(segment))?;
        D::from_any(AnySegment::from_canonical_bytes(&bytes, cx.partition())?.0).ok_or_else(|| {
            ApplyStop::from(SegmentError::Decode {
                artifact: ArtifactKind::Segment,
                message: "a segment of another shape".to_owned(),
            })
        })
    }

    fn facts(cx: &PhaseContext<'_>) -> Result<Vec<Vec<u8>>, ApplyStop> {
        let mut facts = Vec::new();
        for segment in cx.segments() {
            facts.push(cx.artifact(Phase::Summarize, Some(segment))?);
        }
        Ok(facts)
    }
}

impl<D: SegmentDelta> PhaseOperation for SegmentApply<'_, D> {
    type Error = SegmentedApplyError;

    fn op(&self) -> &'static [u8] {
        D::OP
    }

    fn shape(&self) -> InstanceShapeKind {
        D::SHAPE
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
        Ok(self.delta.facts(&Self::validated(cx, segment)?))
    }

    fn assemble(&self, cx: &PhaseContext<'_>) -> Result<Vec<u8>, ApplyStop> {
        let family = FamilyIndex::of_manifest(cx.input())?;
        self.delta
            .check_facts(&Self::facts(cx)?, &family)
            .map_err(ApplyStop::Failed)?;
        Ok(Vec::new())
    }

    fn emit(&self, cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Emission, ApplyStop> {
        let family = FamilyIndex::of_manifest(cx.input())?;
        let held = Self::validated(cx, segment)?;
        Ok(Emission {
            segment: D::into_any(self.delta.edit(&held, &Self::facts(cx)?, &family)?),
            complement: None,
        })
    }

    fn output_role(&self, _: &PhaseContext<'_>) -> ManifestRole {
        ManifestRole::Source
    }
}

/// The outcome of [`apply_run`].
pub type ApplyRun = SegmentedRun<SegmentedApplyError>;

/// Apply `delta` to the instance the input manifest `input` seals as a
/// checkpointed run.
///
/// The run is [`run_segmented`] of [`SegmentApply`], reading its segments
/// from `source` and committing to `store`, with `spec.lens` replaced by the
/// delta's digest ([`SegmentDelta::digest`]), so the run's identity names
/// the delta, and `spec.output_schema` by the input's schema. It reads only
/// the segments of [`SegmentDelta::read_set`], `4k + 2` tasks for `k` of
/// them, and a segment it does not read keeps its record in the output
/// manifest. A refused delta ends at Assemble with the refusal of
/// [`apply_segmented`], before any Emit task, so it seals nothing and every
/// segment is left as it was. A complete run emits each rewritten segment
/// ([`read_emitted_segment`]); gluing them with the unread input segments
/// gives whole-state application to the glued input, and the output
/// manifest is the seal of that instance's split under the input's
/// partition as a source, without an instance digest.
///
/// [`read_emitted_segment`]: super::read_emitted_segment
pub fn apply_run<D: SegmentDelta, Src: SegmentSource, St: SegmentStore>(
    delta: &D,
    input: &SegmentManifest,
    source: &Src,
    spec: RunSpec,
    budget: Budget,
    store: &mut St,
    progress: &mut dyn FnMut(&Progress),
) -> ApplyRun {
    run_segmented(
        &SegmentApply { delta },
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
