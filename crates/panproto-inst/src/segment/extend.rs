//! Segmented graph extension: the graph extend engine run segment by
//! segment.
//!
//! Total graph extension ([`graph_extend_with_plan`]) keeps every node id,
//! edge id, endpoint and position, moves each node to the image of its
//! anchor and each edge to the image of its label, and applies each node's
//! value transforms; it synthesizes nothing and drops nothing. Every fact it
//! reads about an element is the element's own, apart from what a value
//! transform's expression reads, so an extension whose transforms evaluate
//! no expression is [`Locality::Local`]: each segment's run decides its own
//! nodes and the edges leaving them, and the anchors its boundary records
//! carry are mapped by the same images. No identifier is allocated, so the
//! extended family holds exactly the input family's ids and is laid out
//! under the same partition.
//!
//! What a segmented run must exchange is only which segment refuses first.
//! Whole-state extension refuses at the least node with no anchor image,
//! then at the least edge with no label image, then at the least node whose
//! transforms fail ([`graph_extend_with_plan`]); each segment finds its own
//! first refusal in that order and the least of them, which no two segments
//! share since ids are global, is the whole-state refusal.
//!
//! [`graph_extend_with_plan`]: crate::graph_extend_with_plan

use panproto_gat::Name;
use panproto_schema::{CanonicalWriter, InstanceShapeKind};

use super::body::{AnySegment, Boundary, Ghost, GraphSegment, InStub};
use super::error::{Residual, SegmentError};
use super::ids::SegmentId;
use super::manifest::{ManifestRole, SegmentManifest};
use super::partition::Partition;
use super::reader::Reader;
use super::restrict::Local;
use super::runner::{
    Budget, Emission, PhaseContext, PhaseOperation, PhaseStop, Progress, RunSpec, SegmentSource,
    SegmentedRun, run_segmented,
};
use super::store::{ArtifactKind, Phase, SegmentStore};
use crate::error::RestrictError;
use crate::graph_restrict::{ExtendAt, ExtendParts, extend_scoped};
use crate::locality::{GlobalReason, Locality, SegmentOp};
use crate::restrict_plan::RestrictPlan;
use crate::wtype::{FieldTransform, extension_edge_image, extension_vertex_image};

/// The operation bytes of graph extension in a run's identity
/// ([`RunJournal::run_digest`](super::RunJournal::run_digest)).
pub const GRAPH_EXTEND_OP: &[u8] = b"graph extend";

/// Why a segmented graph extension returned no output.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SegmentedExtendError {
    /// The partition or the segments are invalid.
    #[error(transparent)]
    Segment(SegmentError),
    /// Whole-state extension refuses the graph the segments hold, with this
    /// error, which arose in `segment`.
    #[error("extend refused in {segment}: {error}")]
    Extend {
        /// The segment owning the element the refusal is about.
        segment: SegmentId,
        /// The refusal whole-state extension returns.
        error: RestrictError,
    },
    /// The extension is not supported segment by segment.
    #[error("{0}")]
    Residual(Residual),
}

impl From<SegmentError> for SegmentedExtendError {
    fn from(error: SegmentError) -> Self {
        Self::Segment(error)
    }
}

/// The locality class of graph extension along `plan`.
///
/// [`Locality::Global`] with [`GlobalReason::ExpressionDemand`] at the least
/// source vertex whose value transforms evaluate an expression (a term
/// assignment, or a field transform other than renaming, dropping, adding or
/// keeping fields), since an expression is evaluated with the whole instance
/// in scope; otherwise [`Locality::Local`].
#[must_use]
pub fn graph_extend_locality(plan: &RestrictPlan) -> Locality {
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
        .op_term_assignments
        .keys()
        .chain(
            migration
                .field_transforms
                .iter()
                .filter(|(_, transforms)| !transforms.iter().all(local))
                .map(|(vertex, _)| vertex),
        )
        .min();
    expression.map_or(Locality::Local, |vertex| {
        Locality::Global(GlobalReason::ExpressionDemand {
            vertex: vertex.clone(),
        })
    })
}

/// The residual of a plan [`graph_extend_locality`] classifies as global.
fn global(plan: &RestrictPlan) -> Option<Residual> {
    match graph_extend_locality(plan) {
        Locality::Global(reason) => Some(Residual::Global {
            op: SegmentOp::Extend,
            reason,
        }),
        _ => None,
    }
}

/// One segment's run: the extended parts of the nodes it owns and the edges
/// leaving them, or its first refusal with its position.
fn run_segment(
    plan: &RestrictPlan,
    segment: &GraphSegment,
) -> Result<Result<ExtendParts, (ExtendAt, RestrictError)>, SegmentError> {
    let local = Local::of(segment)?;
    Ok(extend_scoped(&local.graph, plan, &local.foreign))
}

/// The extended segment of `held` from its run's parts: the parts with the
/// boundary's anchors and labels mapped by the images the owners of their
/// elements apply.
///
/// # Errors
///
/// [`SegmentedExtendError::Extend`] in the owning segment when a boundary
/// record's anchor or label has no image, which the owner's own run
/// refuses first in a family every segment of which accepts.
fn extended_segment(
    plan: &RestrictPlan,
    held: &GraphSegment,
    parts: ExtendParts,
) -> Result<GraphSegment, SegmentedExtendError> {
    let (target, migration) = (plan.target(), plan.migration());
    let anchor = |anchor: &Name, node: u32, owner: SegmentId| {
        extension_vertex_image(target, migration, anchor).ok_or_else(|| {
            SegmentedExtendError::Extend {
                segment: owner,
                error: RestrictError::UnmappedAnchor {
                    anchor: anchor.clone(),
                    node_id: node,
                },
            }
        })
    };
    let mut boundary = Boundary::default();
    for (key, ghost) in &held.boundary.ghosts {
        boundary.ghosts.insert(
            *key,
            Ghost {
                anchor: anchor(&ghost.anchor, ghost.node, ghost.owner)?,
                ..ghost.clone()
            },
        );
    }
    for (elem, stub) in &held.boundary.in_stubs {
        let label = extension_edge_image(target, migration, &stub.label).ok_or_else(|| {
            SegmentedExtendError::Extend {
                segment: stub.src_owner,
                error: RestrictError::UnmappedEdge {
                    edge: Box::new(stub.label.clone()),
                    element: elem.clone(),
                },
            }
        })?;
        boundary.in_stubs.insert(
            elem.clone(),
            InStub {
                src_anchor: anchor(&stub.src_anchor, stub.src, stub.src_owner)?,
                label,
                ..stub.clone()
            },
        );
    }
    Ok(GraphSegment {
        segment: held.segment,
        nodes: parts.nodes,
        edges: parts.edges,
        boundary,
    })
}

/// Keep the refusal at the least position.
fn keep_least(
    first: &mut Option<(ExtendAt, SegmentId, RestrictError)>,
    segment: SegmentId,
    (at, error): (ExtendAt, RestrictError),
) {
    if first.as_ref().is_none_or(|(least, ..)| at < *least) {
        *first = Some((at, segment, error));
    }
}

/// Extend the graph held as `segments` under `partition` along the
/// extension plan `plan` ([`compile_extension_plan`]), segment by segment.
///
/// The segments are checked as [`verify_partition`] checks them. Each
/// segment's run holds its own nodes and edges with a placeholder for every
/// node owned elsewhere that its edges and in-stubs name, and runs
/// [`graph_extend_with_plan`] on its own nodes and the edges leaving them.
/// When every segment accepts, segment `i` of the output holds the extended
/// nodes and edges segment `i` owns, with each ghost's anchor and each
/// in-stub's source anchor and label mapped as their owners map them; it
/// holds the same ids as the input segment, so the output seals exactly when
/// the input does.
///
/// Gluing the output under `partition` gives [`graph_extend_with_plan`] of
/// the glued input.
///
/// [`compile_extension_plan`]: crate::compile_extension_plan
/// [`graph_extend_with_plan`]: crate::graph_extend_with_plan
/// [`verify_partition`]: super::verify_partition
///
/// # Errors
///
/// [`SegmentedExtendError::Residual`] with [`Residual::Global`] when
/// [`graph_extend_locality`] classifies the plan as global, before any
/// segment is read; [`SegmentedExtendError::Segment`] with the error of
/// `verify_partition`; otherwise [`SegmentedExtendError::Extend`] with the
/// refusal whole-state extension returns, in the segment owning its
/// element.
pub fn graph_extend_segmented(
    plan: &RestrictPlan,
    segments: &[GraphSegment],
    partition: &Partition,
) -> Result<Vec<GraphSegment>, SegmentedExtendError> {
    if let Some(residual) = global(plan) {
        return Err(SegmentedExtendError::Residual(residual));
    }
    let any: Vec<AnySegment> = segments.iter().cloned().map(AnySegment::Graph).collect();
    super::check::verify_partition(&any, partition)?;
    let mut first = None;
    let mut decided = Vec::with_capacity(segments.len());
    for segment in segments {
        match run_segment(plan, segment)? {
            Ok(parts) => decided.push((segment, parts)),
            Err(refusal) => keep_least(&mut first, segment.segment, refusal),
        }
    }
    if let Some((_, segment, error)) = first {
        return Err(SegmentedExtendError::Extend { segment, error });
    }
    decided
        .into_iter()
        .map(|(segment, parts)| extended_segment(plan, segment, parts))
        .collect()
}

/// The Summarize artifact of a segment that accepts: one zero byte.
const ACCEPTED: u8 = 0;

/// The Summarize artifact of a segment that refuses: one byte `1`, the
/// stage byte and the four-byte id of its first refusal's position.
const REFUSED: u8 = 1;

fn position_bytes(position: Option<ExtendAt>) -> Vec<u8> {
    let mut sink = CanonicalWriter::new();
    match position {
        None => sink.tag(ACCEPTED),
        Some((stage, id)) => {
            sink.tag(REFUSED);
            sink.tag(stage);
            sink.u32(id);
        }
    }
    sink.into_bytes()
}

fn decode_position(bytes: &[u8]) -> Result<Option<ExtendAt>, SegmentError> {
    let mut r = Reader::new(bytes, ArtifactKind::Summary);
    let position = match r.u8()? {
        ACCEPTED => None,
        REFUSED => Some((r.u8()?, r.u32()?)),
        other => return Err(r.error(format!("summary tag {other}"))),
    };
    r.finish()?;
    Ok(position)
}

type ExtendStop = PhaseStop<SegmentedExtendError>;

/// Graph extension along an extension plan as a [`PhaseOperation`].
///
/// - Validate keeps the input segment's canonical bytes, checked against the
///   input manifest.
/// - Export writes nothing: the extension is local, so no segment needs a
///   fact another segment holds.
/// - Summarize runs the segment and writes the position of its first
///   refusal, or that it accepts.
/// - Assemble (the coordinator) reads every summary and returns the refusal
///   at the least position, recomputed in the segment that recorded it;
///   otherwise it writes nothing. It holds one position per segment.
/// - Emit runs the segment again and writes its extended segment, without a
///   complement.
/// - Seal seals the extended segments as a source.
pub struct GraphExtend<'p> {
    /// The compiled extension plan.
    pub plan: &'p RestrictPlan,
}

impl GraphExtend<'_> {
    fn validated(cx: &PhaseContext<'_>, segment: SegmentId) -> Result<GraphSegment, ExtendStop> {
        let bytes = cx.artifact(Phase::Validate, Some(segment))?;
        match AnySegment::from_canonical_bytes(&bytes, cx.partition())?.0 {
            AnySegment::Graph(segment) => Ok(segment),
            _ => Err(ExtendStop::from(SegmentError::Decode {
                artifact: ArtifactKind::Segment,
                message: "not a graph segment".to_owned(),
            })),
        }
    }
}

impl PhaseOperation for GraphExtend<'_> {
    type Error = SegmentedExtendError;

    fn op(&self) -> &'static [u8] {
        GRAPH_EXTEND_OP
    }

    fn shape(&self) -> InstanceShapeKind {
        InstanceShapeKind::Graph
    }

    fn segment_op(&self) -> SegmentOp {
        SegmentOp::Extend
    }

    fn locality(&self) -> Locality {
        graph_extend_locality(self.plan)
    }

    fn export(&self, _: &PhaseContext<'_>, _: SegmentId) -> Result<Vec<u8>, ExtendStop> {
        Ok(Vec::new())
    }

    fn summarize(&self, cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Vec<u8>, ExtendStop> {
        let held = Self::validated(cx, segment)?;
        let position = run_segment(self.plan, &held)?.err().map(|(at, _)| at);
        Ok(position_bytes(position))
    }

    fn assemble(&self, cx: &PhaseContext<'_>) -> Result<Vec<u8>, ExtendStop> {
        let mut first: Option<(ExtendAt, SegmentId)> = None;
        for segment in cx.segments() {
            let position = decode_position(&cx.artifact(Phase::Summarize, Some(segment))?)?;
            if let Some(at) = position
                && first.is_none_or(|(least, _)| at < least)
            {
                first = Some((at, segment));
            }
        }
        let Some((_, segment)) = first else {
            return Ok(Vec::new());
        };
        match run_segment(self.plan, &Self::validated(cx, segment)?)? {
            Err((_, error)) => Err(ExtendStop::Failed(SegmentedExtendError::Extend {
                segment,
                error,
            })),
            Ok(_) => Err(ExtendStop::from(SegmentError::Decode {
                artifact: ArtifactKind::Summary,
                message: "a summary records a refusal its segment does not give".to_owned(),
            })),
        }
    }

    fn emit(&self, cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Emission, ExtendStop> {
        let held = Self::validated(cx, segment)?;
        let parts = run_segment(self.plan, &held)?.map_err(|(_, error)| {
            ExtendStop::Failed(SegmentedExtendError::Extend { segment, error })
        })?;
        let extended = extended_segment(self.plan, &held, parts).map_err(ExtendStop::Failed)?;
        Ok(Emission {
            segment: AnySegment::Graph(extended),
            complement: None,
        })
    }

    fn output_role(&self, _: &PhaseContext<'_>) -> ManifestRole {
        ManifestRole::Source
    }
}

/// The outcome of [`graph_extend_run`].
pub type GraphExtendRun = SegmentedRun<SegmentedExtendError>;

/// Run segmented graph extension along the extension plan `plan` as a
/// checkpointed run.
///
/// The run is [`run_segmented`] of [`GraphExtend`] over the graph the input
/// manifest `input` seals, reading its segments from `source` and
/// committing to `store`; [`GraphExtend`] describes its phases. A complete
/// run emits each extended segment, written under `spec.output_schema` (the
/// target schema's digest) and read back with
/// [`read_emitted`](super::read_emitted); they are
/// [`graph_extend_segmented`] of the input segments, and the output manifest
/// seals them as a source. A refused extension ends at Assemble with the
/// whole-state refusal, before any Emit task, so it seals nothing.
pub fn graph_extend_run<Src: SegmentSource, St: SegmentStore>(
    plan: &RestrictPlan,
    input: &SegmentManifest,
    source: &Src,
    spec: RunSpec,
    budget: Budget,
    store: &mut St,
    progress: &mut dyn FnMut(&Progress),
) -> GraphExtendRun {
    run_segmented(
        &GraphExtend { plan },
        input,
        source,
        spec,
        budget,
        store,
        progress,
    )
}
