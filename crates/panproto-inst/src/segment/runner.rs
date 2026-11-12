//! The checkpointed run of a segmented operation.
//!
//! [`run_segmented`] runs a [`PhaseOperation`] as the six phases of a
//! segmented run ([`Phase`]), one task per segment in the segment phases and
//! one coordinator task in Assemble and Seal, so a run of `n` segments has
//! `4n + 2` tasks. Each task writes one artifact to the caller's
//! [`SegmentStore`] and commits it with its journal record, and a task reads
//! only the input segment it names and the committed artifacts of earlier
//! phases. The operation says what each task computes; the runner owns the
//! rest. Graph restriction is one operation ([`GraphRestrict`], run by
//! [`graph_restrict_run`]), graph extension another
//! ([`GraphExtend`](super::GraphExtend)), and graph put, in `panproto-lens`,
//! a third.
//!
//! [`graph_restrict_run`] runs [`graph_restrict_segmented`] as follows:
//!
//! - Validate reads one input segment from the caller's [`SegmentSource`]
//!   and checks it against the input manifest; its artifact is the
//!   segment's canonical bytes.
//! - Export decides the segment's own nodes (stage 0 of the engine, after
//!   the fan domain) and writes the fate of every in-stub target, keyed by
//!   the segment owning the in-stub's source; or, when the segment refuses,
//!   the position of its refusal.
//! - Summarize writes the segment's reach summary, built from its own fates
//!   and the fates the other segments exported to it.
//! - Assemble (the coordinator) returns the least refusal of the fan domain
//!   and stage 0, closes the reach summaries over the boundary, runs every
//!   segment's later stages and allocation one segment at a time to find the
//!   least later refusal and the boundaries of the view segments, and
//!   writes the closure and those boundaries. The coordinator holds the
//!   summaries, the closure and the boundaries, never more than one segment.
//! - Emit runs one segment's later stages again with the closure and writes
//!   its view segment and its complement segment, and beside them what Seal
//!   reads of the view: its manifest record, its sorted edge id run and its
//!   record stream ([`AnySegment::record_stream`]), each located by the
//!   artifact's fixed-width header.
//! - Seal (the coordinator) seals the view segments into the output
//!   manifest, a [`ManifestRole::View`] of the input manifest. It reads each
//!   Emit artifact's header and record whole and its edge id run and record
//!   stream one id and one record at a time ([`SegmentStore::read_at`]), so
//!   it holds the `n` records, one id per run and one record per stream,
//!   and never a view segment. Its reported peak
//!   ([`PhaseReport::peak_estimated_bytes`]) is that holding as the Emit
//!   headers state it, so it grows with the number of segments and the
//!   longest record of each, not with the records a segment holds.
//!
//! A task whose key is journaled with the same inputs, and whose artifact
//! still hashes to the recorded output, is reused rather than recomputed
//! ([`RunJournal::reusable`]). Since every task is deterministic, a run
//! cancelled after any number of tasks and resumed against the same store
//! produces the artifacts and manifest of an uninterrupted run.
//!
//! The budget is checked deterministically: the working set of every input
//! segment ([`MemoryModel`]) and the number of segments before the first
//! task, the bytes the coordinator holds (the summaries and its own
//! artifact) at Assemble, and the running total of complement bytes before
//! each Emit task commits. An exceeded budget ends the run with a
//! [`Residual::BudgetExceeded`].
//!
//! [`graph_restrict_segmented`]: super::graph_restrict_segmented

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use panproto_schema::{CanonicalWriter, InstanceShapeKind};
use serde::{Deserialize, Serialize};

use super::body::{AnySegment, GraphSegment, SegmentStats};
use super::bytes::{read_boundary, write_boundary};
use super::error::{Residual, Resource, SegmentError};
use super::ids::SegmentId;
use super::manifest::{
    EdgeIdRun, ManifestHeader, ManifestRole, SegmentManifest, SegmentRecord, reseal, seal_full,
    verify_manifest, verify_segment_bytes,
};
use super::partition::{OwnerIndex, Partition};
use super::reader::Reader;
use super::restrict::{
    ClosedReach, GraphComplementSegment, Local, ReachSummary, SegmentedRestrictError,
    ViewBoundaries, close_reach, emit_segment, graph_restrict_locality, keep_least, outside_of,
    refusal_key, summarize,
};
use super::store::{
    ArtifactKey, ArtifactKind, Phase, RunJournal, SegmentStore, TaskKey, TaskRecord,
};
use super::stream::{InstanceRecord, RecordSource};
use crate::error::RestrictError;
use crate::graph_restrict::{RemoteFate, decide_nodes, run_scoped};
use crate::identity::{
    CanonicalSink, ComplementFormat, ComplementShape, Digest, GRAPH_COMPLEMENT_FORMAT_VERSION,
    IDENTITY_ENVELOPE_VERSION, IdentityEnvelope, SEGMENT_TASK_CONTEXT,
};
use crate::locality::{Locality, SegmentOp};
use crate::restrict_plan::RestrictPlan;

/// The operation bytes of graph restriction in a run's identity
/// ([`RunJournal::run_digest`]).
pub const GRAPH_RESTRICT_OP: &[u8] = b"graph restrict";

/// The deterministic resource budget of a segmented run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Budget {
    /// The greatest estimate ([`MemoryModel`]) of one input segment.
    pub working_set: u64,
    /// The greatest total of complement bytes, or `None` for no bound.
    pub complement: Option<u64>,
    /// The greatest number of bytes the coordinator holds at Assemble.
    pub summary: u64,
    /// The greatest number of segments.
    pub affected_segments: u32,
}

impl Budget {
    /// The budget that bounds nothing.
    #[must_use]
    pub const fn unlimited() -> Self {
        Self {
            working_set: u64::MAX,
            complement: None,
            summary: u64::MAX,
            affected_segments: u32::MAX,
        }
    }
}

/// The deterministic estimate of the bytes needed to hold one segment.
pub struct MemoryModel;

impl MemoryModel {
    /// `160 * nodes + 128 * edges + 192 * fans + 96 * ghosts +
    /// 128 * in_stubs + payload_bytes`, saturating.
    ///
    /// ```
    /// use panproto_inst::segment::{MemoryModel, SegmentId, SegmentStats};
    ///
    /// let stats = SegmentStats {
    ///     segment: SegmentId(0),
    ///     nodes: 2,
    ///     edges: 1,
    ///     fans: 0,
    ///     ghosts: 1,
    ///     in_stubs: 0,
    ///     payload_bytes: 10,
    /// };
    /// assert_eq!(MemoryModel::estimate(&stats), 320 + 128 + 96 + 10);
    /// ```
    #[must_use]
    pub const fn estimate(stats: &SegmentStats) -> u64 {
        stats
            .nodes
            .saturating_mul(160)
            .saturating_add(stats.edges.saturating_mul(128))
            .saturating_add(stats.fans.saturating_mul(192))
            .saturating_add(stats.ghosts.saturating_mul(96))
            .saturating_add(stats.in_stubs.saturating_mul(128))
            .saturating_add(stats.payload_bytes)
    }
}

/// The progress of a run after a task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Progress {
    /// The phase of the latest task.
    pub phase: Phase,
    /// The tasks committed or reused so far.
    pub tasks_done: u32,
    /// The tasks of the whole run.
    pub tasks_total: u32,
    /// The work done so far, one unit per task.
    pub elements_done: u64,
    /// The work of the whole run.
    pub elements_total: u64,
}

/// The complement a run has emitted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComplementSize {
    /// The complement entries ([`GraphComplementSegment::element_count`]).
    ///
    /// [`GraphComplementSegment::element_count`]: super::GraphComplementSegment::element_count
    pub elements: u64,
    /// The complement bytes.
    pub bytes: u64,
    /// The entries and bytes of each segment's complement.
    pub per_segment: BTreeMap<SegmentId, (u64, u64)>,
}

/// The accounting of one finished phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PhaseReport {
    /// The phase.
    pub phase: Phase,
    /// Its tasks this invocation computed.
    pub tasks: u32,
    /// Its tasks this invocation reused.
    pub reused: u32,
    /// The phase's deterministic memory estimate: for Seal of a graph
    /// restrict run ([`graph_restrict_run`]), the bytes Seal holds at once
    /// (per Emit artifact its 56-byte header, its manifest record, four
    /// bytes when its edge id run is not empty and its longest record frame
    /// with its eight-byte length, summed over the artifacts); for every
    /// other phase, the greatest working-set estimate ([`MemoryModel`]) of
    /// an input segment.
    pub peak_estimated_bytes: u64,
    /// The measured wall time, when the caller measures it.
    pub wall_ms: Option<u64>,
}

/// Why a run stopped before its last task, leaving a journal to resume
/// from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunIncompleteReason {
    /// The caller's cancellation point was reached.
    Cancelled,
    /// The store or the segment source failed.
    Store {
        /// The failure's message.
        message: String,
    },
}

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus<E> {
    /// Every task committed; the output manifest has this digest.
    Complete {
        /// The output manifest's digest.
        manifest: Digest,
    },
    /// The run stopped and can be resumed.
    Incomplete {
        /// Why it stopped.
        reason: RunIncompleteReason,
    },
    /// The run is declined within its budget or locality.
    Residual(Residual),
    /// The input is refused.
    Failed(E),
}

/// The deterministic report of a run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentRunReport<E> {
    /// How the run ended.
    pub status: RunStatus<E>,
    /// The locality class of the operation.
    pub locality: Locality,
    /// The budget the run was given.
    pub budget: Budget,
    /// The progress after the last task.
    pub progress: Progress,
    /// The greatest working-set estimate of an input segment.
    pub peak_estimated_bytes: u64,
    /// A measured peak, when the caller measures one.
    pub observed_peak_bytes: Option<u64>,
    /// The complement emitted.
    pub complement: ComplementSize,
    /// The tasks this invocation computed.
    pub tasks_run: u32,
    /// The tasks this invocation reused.
    pub tasks_reused: u32,
    /// The artifacts of the run staged but not committed, for the caller to
    /// delete.
    pub orphans: Vec<ArtifactKey>,
    /// The accounting of every finished phase, in run order.
    pub phases: Vec<PhaseReport>,
}

/// What names a segmented run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunSpec {
    /// The identity of the lens the plan compiles.
    pub lens: Digest,
    /// The trusted digest of the input manifest.
    pub input: Digest,
    /// The schema digest the output segments are written under: the view
    /// schema of a restriction, the source schema of a put.
    pub output_schema: Digest,
    /// Stop once this many tasks are committed or reused.
    pub cancel_after_tasks: Option<u32>,
}

/// The caller's source of input segments: each segment's canonical bytes.
pub trait SegmentSource {
    /// The source's own error.
    type Error: std::error::Error + Send + Sync + 'static;

    /// The canonical bytes of input segment `segment`.
    ///
    /// # Errors
    ///
    /// The source's error when they cannot be read.
    fn read(&self, segment: SegmentId) -> Result<Vec<u8>, Self::Error>;

    /// The length of the canonical bytes of segment `segment`, which a run
    /// reads to check a budget before it reads any segment.
    ///
    /// # Errors
    ///
    /// The source's error when the length cannot be read.
    fn byte_len(&self, segment: SegmentId) -> Result<u64, Self::Error> {
        self.read(segment).map(|bytes| length(&bytes))
    }
}

/// A segment a [`SegmentSource`] does not hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the source holds no bytes for {0}")]
pub struct MissingSegment(pub SegmentId);

impl SegmentSource for BTreeMap<SegmentId, Vec<u8>> {
    type Error = MissingSegment;

    fn read(&self, segment: SegmentId) -> Result<Vec<u8>, Self::Error> {
        self.get(&segment).cloned().ok_or(MissingSegment(segment))
    }

    fn byte_len(&self, segment: SegmentId) -> Result<u64, Self::Error> {
        self.get(&segment)
            .map(|bytes| length(bytes))
            .ok_or(MissingSegment(segment))
    }
}

/// A [`SegmentStore`] in memory, with atomic commit.
#[derive(Clone, Debug, Default)]
pub struct MemoryStore {
    staged: BTreeMap<ArtifactKey, Vec<u8>>,
    committed: BTreeMap<ArtifactKey, Vec<u8>>,
    journals: BTreeMap<Digest, RunJournal>,
}

impl SegmentStore for MemoryStore {
    type Error = std::convert::Infallible;

    fn read(&self, key: &ArtifactKey) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.committed.get(key).cloned())
    }

    fn read_at(
        &self,
        key: &ArtifactKey,
        offset: u64,
        len: u64,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.committed.get(key).and_then(|bytes| {
            let start = usize::try_from(offset).ok()?;
            let end = start.checked_add(usize::try_from(len).ok()?)?;
            bytes.get(start..end).map(<[u8]>::to_vec)
        }))
    }

    fn stage(&mut self, key: &ArtifactKey, bytes: Vec<u8>) -> Result<(), Self::Error> {
        self.staged.insert(*key, bytes);
        Ok(())
    }

    fn commit(&mut self, key: &ArtifactKey, record: &TaskRecord) -> Result<(), Self::Error> {
        if let Some(bytes) = self.staged.remove(key) {
            self.committed.insert(*key, bytes);
            self.journals
                .entry(key.run)
                .or_insert_with(|| RunJournal::new(key.run))
                .tasks
                .insert(key.task, *record);
        }
        Ok(())
    }

    fn journal(&self, run: &Digest) -> Result<RunJournal, Self::Error> {
        Ok(self
            .journals
            .get(run)
            .cloned()
            .unwrap_or_else(|| RunJournal::new(*run)))
    }

    fn staged_uncommitted(&self, run: &Digest) -> Result<Vec<ArtifactKey>, Self::Error> {
        Ok(self
            .staged
            .keys()
            .filter(|key| key.run == *run)
            .copied()
            .collect())
    }
}

/// The outcome of a run of [`run_segmented`].
#[derive(Debug)]
pub struct SegmentedRun<E> {
    /// The run's identity, which names its artifacts in the store; `None`
    /// when the input manifest is refused before the run has one.
    pub run: Option<Digest>,
    /// The output manifest of a complete run.
    pub manifest: Option<SegmentManifest>,
    /// The run's report.
    pub report: SegmentRunReport<E>,
}

/// The outcome of [`graph_restrict_run`].
pub type GraphRestrictRun = SegmentedRun<SegmentedRestrictError>;

/// One emitted segment of a complete run, read back from the store.
#[derive(Clone, Debug)]
pub struct EmittedSegment {
    /// The view segment.
    pub view: GraphSegment,
    /// The canonical bytes of the complement segment
    /// ([`GraphComplementSegment::canonical_bytes`]).
    ///
    /// [`GraphComplementSegment::canonical_bytes`]: super::GraphComplementSegment::canonical_bytes
    pub complement: Vec<u8>,
}

/// Why a phase of an operation stopped.
#[derive(Debug)]
pub enum PhaseStop<E> {
    /// The store or a source failed, with its message; the run can be
    /// resumed.
    Store(String),
    /// The input is refused.
    Failed(E),
    /// The run is declined within its budget or locality.
    Residual(Residual),
}

impl<E: From<SegmentError>> From<SegmentError> for PhaseStop<E> {
    fn from(error: SegmentError) -> Self {
        Self::Failed(E::from(error))
    }
}

/// The store failure of a phase, with the failure's message.
pub fn store_failure<E>(error: impl fmt::Display) -> PhaseStop<E> {
    PhaseStop::Store(error.to_string())
}

/// Why a run stopped, inside the runner.
enum Stop<E> {
    Cancelled,
    Phase(PhaseStop<E>),
}

impl<E> From<PhaseStop<E>> for Stop<E> {
    fn from(stop: PhaseStop<E>) -> Self {
        Self::Phase(stop)
    }
}

impl<E: From<SegmentError>> From<SegmentError> for Stop<E> {
    fn from(error: SegmentError) -> Self {
        Self::Phase(PhaseStop::from(error))
    }
}

fn store_stop<E>(error: impl fmt::Display) -> Stop<E> {
    Stop::Phase(store_failure(error))
}

/// A failure met while reading the sections of an artifact.
enum ReadStop {
    Store(String),
    Segment(SegmentError),
}

impl<E: From<SegmentError>> From<ReadStop> for Stop<E> {
    fn from(stop: ReadStop) -> Self {
        match stop {
            ReadStop::Store(message) => Self::Phase(PhaseStop::Store(message)),
            ReadStop::Segment(error) => Self::from(error),
        }
    }
}

/// The digest naming an artifact's bytes in its task record.
fn artifact_digest(bytes: &[u8]) -> Digest {
    Digest::derive(SEGMENT_TASK_CONTEXT, bytes)
}

fn length(bytes: &[u8]) -> u64 {
    u64::try_from(bytes.len()).unwrap_or(u64::MAX)
}

/// The decode error of an artifact.
fn malformed(artifact: ArtifactKind, message: &str) -> SegmentError {
    SegmentError::Decode {
        artifact,
        message: message.to_owned(),
    }
}

fn write_fate<S: CanonicalSink + ?Sized>(sink: &mut S, fate: &RemoteFate) {
    match fate {
        RemoteFate::Fan => sink.tag(0),
        RemoteFate::Dropped => sink.tag(1),
        RemoteFate::Kept {
            source,
            view,
            recorded,
        } => {
            sink.tag(2);
            sink.name(source);
            sink.name(view);
            sink.opt_name(recorded.as_ref());
        }
    }
}

fn read_fate(r: &mut Reader<'_>) -> Result<RemoteFate, SegmentError> {
    match r.u8()? {
        0 => Ok(RemoteFate::Fan),
        1 => Ok(RemoteFate::Dropped),
        2 => Ok(RemoteFate::Kept {
            source: r.name()?,
            view: r.name()?,
            recorded: r.opt_name()?,
        }),
        other => Err(r.error(format!("fate tag {other}"))),
    }
}

fn write_ids<S: CanonicalSink + ?Sized>(sink: &mut S, ids: &BTreeSet<u32>) {
    sink.len(ids.len());
    for id in ids {
        sink.u32(*id);
    }
}

fn read_ids(r: &mut Reader<'_>) -> Result<BTreeSet<u32>, SegmentError> {
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

fn write_fates<S: CanonicalSink + ?Sized>(sink: &mut S, fates: &BTreeMap<u32, RemoteFate>) {
    sink.len(fates.len());
    for (id, fate) in fates {
        sink.u32(*id);
        write_fate(sink, fate);
    }
}

fn read_fates(r: &mut Reader<'_>) -> Result<BTreeMap<u32, RemoteFate>, SegmentError> {
    let n = r.count(5)?;
    let mut fates = BTreeMap::new();
    for _ in 0..n {
        let id = r.u32()?;
        if fates.keys().next_back().is_some_and(|last| *last >= id) {
            return Err(r.error("fates are not strictly ascending"));
        }
        fates.insert(id, read_fate(r)?);
    }
    Ok(fates)
}

/// One segment's Export artifact.
enum Export {
    /// The least node of the segment outside the fan domain.
    Fan(u32),
    /// The position of the segment's stage 0 refusal, `None` when the engine
    /// does not locate it.
    Refused(Option<super::error::ErrorKey>),
    /// The fate of every in-stub target, by the segment owning the
    /// in-stub's source.
    Fates(BTreeMap<(SegmentId, u32), RemoteFate>),
}

impl Export {
    fn bytes(&self) -> Vec<u8> {
        let mut sink = CanonicalWriter::new();
        match self {
            Self::Fan(node) => {
                sink.tag(0);
                sink.u32(*node);
            }
            Self::Refused(key) => {
                sink.tag(1);
                match key {
                    None => sink.bool(false),
                    Some(key) => {
                        sink.bool(true);
                        key.write(&mut sink);
                    }
                }
            }
            Self::Fates(fates) => {
                sink.tag(2);
                sink.len(fates.len());
                for ((owner, node), fate) in fates {
                    sink.u32(owner.0);
                    sink.u32(*node);
                    write_fate(&mut sink, fate);
                }
            }
        }
        sink.into_bytes()
    }

    fn decode(bytes: &[u8]) -> Result<Self, SegmentError> {
        let mut r = Reader::new(bytes, ArtifactKind::Export);
        let export = match r.u8()? {
            0 => Self::Fan(r.u32()?),
            1 => Self::Refused(if r.bool()? {
                Some(super::error::ErrorKey {
                    stage: r.u8()?,
                    id: r.u32()?,
                    id2: r.u32()?,
                    kind: r.u8()?,
                })
            } else {
                None
            }),
            2 => {
                let n = r.count(9)?;
                let mut fates = BTreeMap::new();
                for _ in 0..n {
                    let key = (SegmentId(r.u32()?), r.u32()?);
                    if fates.keys().next_back().is_some_and(|last| *last >= key) {
                        return Err(r.error("exports are not strictly ascending"));
                    }
                    fates.insert(key, read_fate(&mut r)?);
                }
                Self::Fates(fates)
            }
            other => return Err(r.error(format!("export tag {other}"))),
        };
        r.finish()?;
        Ok(export)
    }
}

/// A reach summary's canonical bytes: its entries (node, survivors reached,
/// dropped ghosts left through) and then the fates of the survivors, each
/// ascending by node.
fn summary_bytes(summary: &ReachSummary) -> Vec<u8> {
    let mut sink = CanonicalWriter::new();
    sink.len(summary.entries.len());
    for (node, (kept, exits)) in &summary.entries {
        sink.u32(*node);
        write_ids(&mut sink, kept);
        write_ids(&mut sink, exits);
    }
    write_fates(&mut sink, &summary.kept);
    sink.into_bytes()
}

fn decode_summary(bytes: &[u8]) -> Result<ReachSummary, SegmentError> {
    let mut r = Reader::new(bytes, ArtifactKind::Summary);
    let n = r.count(20)?;
    let mut summary = ReachSummary::default();
    for _ in 0..n {
        let node = r.u32()?;
        if summary
            .entries
            .keys()
            .next_back()
            .is_some_and(|last| *last >= node)
        {
            return Err(r.error("entries are not strictly ascending"));
        }
        let kept = read_ids(&mut r)?;
        let exits = read_ids(&mut r)?;
        summary.entries.insert(node, (kept, exits));
    }
    summary.kept = read_fates(&mut r)?;
    r.finish()?;
    Ok(summary)
}

/// The coordinator's artifact: the closed reach and the view boundaries.
struct Assembly {
    closed: ClosedReach,
    boundaries: Vec<super::body::Boundary>,
}

/// The Assemble artifact: every entry with the survivors it reaches, the
/// fates of the survivors, then each view segment's boundary in id order.
fn assembly_bytes(closed: &ClosedReach, boundaries: &[super::body::Boundary]) -> Vec<u8> {
    let mut sink = CanonicalWriter::new();
    let reach: BTreeMap<&u32, &BTreeSet<u32>> = closed.reach.iter().collect();
    sink.len(reach.len());
    for (node, reached) in reach {
        sink.u32(*node);
        write_ids(&mut sink, reached);
    }
    write_fates(&mut sink, &closed.kept);
    sink.len(boundaries.len());
    for boundary in boundaries {
        write_boundary(&mut sink, boundary);
    }
    sink.into_bytes()
}

fn decode_assembly(bytes: &[u8]) -> Result<Assembly, SegmentError> {
    let mut r = Reader::new(bytes, ArtifactKind::Assembly);
    let n = r.count(12)?;
    let mut reach = std::collections::HashMap::with_capacity(n);
    let mut last: Option<u32> = None;
    for _ in 0..n {
        let node = r.u32()?;
        if last.is_some_and(|last| last >= node) {
            return Err(r.error("entries are not strictly ascending"));
        }
        last = Some(node);
        reach.insert(node, read_ids(&mut r)?);
    }
    let kept = read_fates(&mut r)?;
    let n = r.count(16)?;
    let mut boundaries = Vec::with_capacity(n);
    for _ in 0..n {
        boundaries.push(read_boundary(&mut r)?);
    }
    r.finish()?;
    Ok(Assembly {
        closed: ClosedReach { reach, kept },
        boundaries,
    })
}

/// The length of an Emit artifact's header: seven eight-byte fields.
const EMIT_HEADER_LEN: u64 = 56;

/// The header of an Emit artifact, which locates its five sections.
///
/// The artifact is the header and then its sections, in this order and
/// with nothing between them: the view segment's canonical bytes, the
/// complement segment's canonical bytes, the view's manifest record
/// ([`SegmentRecord::canonical_bytes`]), the view's sorted edge id run (each
/// id as four little-endian bytes) and the view's record stream
/// ([`AnySegment::record_stream`]). The header writes, each as eight
/// little-endian bytes, the complement's entry count, the lengths of the
/// view, complement, record and record stream sections in that order with
/// the number of ids of the run between the last two, and the length of the
/// longest frame of the record stream with its length prefix (`0` for an
/// empty stream).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EmitLayout {
    elements: u64,
    view: u64,
    complement: u64,
    record: u64,
    run_ids: u64,
    stream: u64,
    longest: u64,
}

impl EmitLayout {
    fn write<S: CanonicalSink + ?Sized>(&self, sink: &mut S) {
        for field in [
            self.elements,
            self.view,
            self.complement,
            self.record,
            self.run_ids,
            self.stream,
            self.longest,
        ] {
            sink.u64(field);
        }
    }

    /// The header `bytes` hold, refused unless its sections fit in a `u64`
    /// length.
    fn decode(bytes: &[u8]) -> Result<Self, SegmentError> {
        let mut r = Reader::new(bytes, ArtifactKind::ComplementSegment);
        let layout = Self {
            elements: r.u64()?,
            view: r.u64()?,
            complement: r.u64()?,
            record: r.u64()?,
            run_ids: r.u64()?,
            stream: r.u64()?,
            longest: r.u64()?,
        };
        r.finish()?;
        layout.end()?;
        Ok(layout)
    }

    const fn complement_at(&self) -> u64 {
        EMIT_HEADER_LEN + self.view
    }

    const fn record_at(&self) -> u64 {
        self.complement_at() + self.complement
    }

    const fn run_at(&self) -> u64 {
        self.record_at() + self.record
    }

    const fn stream_at(&self) -> u64 {
        self.run_at() + 4 * self.run_ids
    }

    /// The artifact's length, refused when it overflows a `u64`; every
    /// offset above is at most this length once it is accepted.
    fn end(&self) -> Result<u64, SegmentError> {
        EMIT_HEADER_LEN
            .checked_add(self.view)
            .and_then(|at| at.checked_add(self.complement))
            .and_then(|at| at.checked_add(self.record))
            .and_then(|at| at.checked_add(self.run_ids.checked_mul(4)?))
            .and_then(|at| at.checked_add(self.stream))
            .ok_or_else(|| {
                malformed(
                    ArtifactKind::ComplementSegment,
                    "the sections of an emitted artifact overflow its length",
                )
            })
    }
}

/// The Emit artifact of one output segment and, when the operation emits
/// one, its complement segment; without one the complement section is
/// empty and its entry count 0.
fn emit_bytes(
    view: &AnySegment,
    view_bytes: &[u8],
    complement: Option<&GraphComplementSegment>,
    record: &SegmentRecord,
) -> Result<Vec<u8>, SegmentError> {
    let complement_bytes = match complement {
        Some(complement) => complement.canonical_bytes()?,
        None => Vec::new(),
    };
    let record_bytes = record.canonical_bytes();
    let run = view.edge_id_run();
    let records = view.records();
    let mut stream = CanonicalWriter::new();
    let mut longest = 0;
    for record in records.as_slice() {
        let frame = record.frame();
        longest = longest.max(length(&frame).saturating_add(8));
        stream.bytes(&frame);
    }
    let stream = stream.into_bytes();
    let layout = EmitLayout {
        elements: complement.map_or(0, GraphComplementSegment::element_count),
        view: length(view_bytes),
        complement: length(&complement_bytes),
        record: length(&record_bytes),
        run_ids: u64::try_from(run.len()).unwrap_or(u64::MAX),
        stream: length(&stream),
        longest,
    };
    let mut sink = CanonicalWriter::new();
    layout.write(&mut sink);
    sink.write(view_bytes);
    sink.write(&complement_bytes);
    sink.write(&record_bytes);
    for id in run {
        sink.u32(id.0);
    }
    sink.write(&stream);
    Ok(sink.into_bytes())
}

/// The header of an Emit artifact, the view segment's bytes and the
/// complement segment's bytes, refused unless the header's sections span
/// the artifact exactly.
fn decode_emit(bytes: &[u8]) -> Result<(EmitLayout, &[u8], &[u8]), SegmentError> {
    let header_len = usize::try_from(EMIT_HEADER_LEN).unwrap_or(usize::MAX);
    let header = bytes.get(..header_len).ok_or_else(|| {
        malformed(
            ArtifactKind::ComplementSegment,
            "an emitted artifact is shorter than its header",
        )
    })?;
    let layout = EmitLayout::decode(header)?;
    if layout.end()? != length(bytes) {
        return Err(malformed(
            ArtifactKind::ComplementSegment,
            "the sections of an emitted artifact do not span it",
        ));
    }
    let at = |offset: u64| usize::try_from(offset).unwrap_or(usize::MAX);
    let view = &bytes[header_len..at(layout.complement_at())];
    let complement = &bytes[at(layout.complement_at())..at(layout.record_at())];
    Ok((layout, view, complement))
}

/// A failure met while Seal reads from the store, kept until the seal
/// returns, since an edge id run cannot return one.
type ReadFailure = RefCell<Option<ReadStop>>;

fn fail(failure: &ReadFailure, stop: ReadStop) {
    failure.borrow_mut().get_or_insert(stop);
}

/// One edge id run of an Emit artifact, read one id at a time.
struct StoredIds<'s, St> {
    store: &'s St,
    key: ArtifactKey,
    at: u64,
    end: u64,
    failure: &'s ReadFailure,
}

impl<St: SegmentStore> Iterator for StoredIds<'_, St> {
    type Item = crate::ginstance::EdgeId;

    fn next(&mut self) -> Option<Self::Item> {
        if self.at >= self.end {
            return None;
        }
        let bytes = match self.store.read_at(&self.key, self.at, 4) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => {
                fail(self.failure, missing_section(self.key.kind));
                return None;
            }
            Err(error) => {
                fail(self.failure, ReadStop::Store(error.to_string()));
                return None;
            }
        };
        let Ok(id) = <[u8; 4]>::try_from(bytes.as_slice()) else {
            fail(self.failure, missing_section(self.key.kind));
            return None;
        };
        self.at += 4;
        Some(crate::ginstance::EdgeId(u32::from_le_bytes(id)))
    }
}

/// One record stream of an Emit artifact, read one frame at a time.
struct StoredRecords<'s, St> {
    store: &'s St,
    key: ArtifactKey,
    stats: SegmentStats,
    at: u64,
    end: u64,
    failure: &'s ReadFailure,
}

impl<St: SegmentStore> StoredRecords<'_, St> {
    /// The `len` bytes at `offset`, or the failure recorded and the error
    /// the stream returns for it.
    fn read(&self, offset: u64, len: u64) -> Result<Vec<u8>, SegmentError> {
        let stop = match self.store.read_at(&self.key, offset, len) {
            Ok(Some(bytes)) if length(&bytes) == len => return Ok(bytes),
            Ok(_) => missing_section(self.key.kind),
            Err(error) => ReadStop::Store(error.to_string()),
        };
        fail(self.failure, stop);
        Err(malformed(
            self.key.kind,
            "a section of an emitted artifact cannot be read",
        ))
    }
}

impl<St: SegmentStore> RecordSource for StoredRecords<'_, St> {
    fn stats(&self) -> SegmentStats {
        self.stats
    }

    fn next_record(&mut self) -> Result<Option<InstanceRecord>, SegmentError> {
        if self.at >= self.end {
            return Ok(None);
        }
        let prefix = self.read(self.at, 8)?;
        let len = u64::from_le_bytes(<[u8; 8]>::try_from(prefix.as_slice()).unwrap_or_default());
        let start = self.at + 8;
        if len > self.end.saturating_sub(start) {
            return Err(malformed(
                self.key.kind,
                "a record frame runs past its record stream",
            ));
        }
        let frame = self.read(start, len)?;
        self.at = start + len;
        InstanceRecord::from_frame(&frame).map(Some)
    }
}

/// The failure of a section the store does not hold.
fn missing_section(kind: ArtifactKind) -> ReadStop {
    ReadStop::Segment(malformed(
        kind,
        "a section of a committed artifact is missing",
    ))
}

/// The view segment and complement bytes `segment` emitted in run `run`.
///
/// # Errors
///
/// [`SegmentError::Decode`] when the store holds no Emit artifact for the
/// segment or holds a malformed one, and the error of
/// [`AnySegment::from_canonical_bytes`] for its view segment.
pub fn read_emitted<S: SegmentStore>(
    store: &S,
    run: Digest,
    partition: &Partition,
    segment: SegmentId,
) -> Result<EmittedSegment, SegmentError> {
    let key = ArtifactKey {
        run,
        task: TaskKey {
            phase: Phase::Emit,
            segment: Some(segment),
        },
        kind: ArtifactKind::ComplementSegment,
    };
    let bytes = store
        .read(&key)
        .map_err(|error| malformed(ArtifactKind::ComplementSegment, &error.to_string()))?
        .ok_or_else(|| malformed(ArtifactKind::ComplementSegment, "no emitted segment"))?;
    let (_, view, complement) = decode_emit(&bytes)?;
    let (view, _) = AnySegment::from_canonical_bytes(view, partition)?;
    let AnySegment::Graph(view) = view else {
        return Err(malformed(ArtifactKind::Segment, "not a graph segment"));
    };
    Ok(EmittedSegment {
        view,
        complement: complement.to_vec(),
    })
}

/// The artifact kind each phase's task writes: the validated input segment,
/// an export, a summary, the coordinator's assembly, an Emit artifact and
/// the output manifest.
#[must_use]
pub const fn phase_artifact(phase: Phase) -> ArtifactKind {
    match phase {
        Phase::Validate => ArtifactKind::Segment,
        Phase::Export => ArtifactKind::Export,
        Phase::Summarize => ArtifactKind::Summary,
        Phase::Assemble => ArtifactKind::Assembly,
        Phase::Emit => ArtifactKind::ComplementSegment,
        Phase::Seal => ArtifactKind::Manifest,
    }
}

/// What a phase of an operation reads: the input manifest, its partition
/// and owner index, the run's spec, the segments the run reads, and the
/// committed artifacts of earlier phases.
pub struct PhaseContext<'r> {
    input: &'r SegmentManifest,
    partition: &'r Partition,
    index: &'r OwnerIndex,
    spec: &'r RunSpec,
    segments: &'r [SegmentId],
    read: &'r dyn Fn(TaskKey) -> Result<Option<Vec<u8>>, String>,
}

impl PhaseContext<'_> {
    /// The input manifest.
    #[must_use]
    pub const fn input(&self) -> &SegmentManifest {
        self.input
    }

    /// The input's partition.
    #[must_use]
    pub const fn partition(&self) -> &Partition {
        self.partition
    }

    /// The owner index of the partition.
    #[must_use]
    pub const fn index(&self) -> &OwnerIndex {
        self.index
    }

    /// The run's spec.
    #[must_use]
    pub const fn spec(&self) -> &RunSpec {
        self.spec
    }

    /// The segments the run reads, in id order: every segment of the
    /// partition, or the operation's affected segments
    /// ([`PhaseOperation::affected`]).
    #[must_use]
    pub fn segments(&self) -> Vec<SegmentId> {
        self.segments.to_vec()
    }

    /// The committed artifact of the task of `phase` for `segment` (`None`
    /// for a coordinator task).
    ///
    /// # Errors
    ///
    /// [`PhaseStop::Store`] when the store fails, and a
    /// [`SegmentError::Decode`] refusal when the artifact is missing.
    pub fn artifact<E: From<SegmentError>>(
        &self,
        phase: Phase,
        segment: Option<SegmentId>,
    ) -> Result<Vec<u8>, PhaseStop<E>> {
        let kind = phase_artifact(phase);
        (self.read)(task_key(phase, segment))
            .map_err(PhaseStop::Store)?
            .ok_or_else(|| PhaseStop::from(malformed(kind, "a committed artifact is missing")))
    }
}

/// One output segment an Emit task writes, with the complement segment of
/// an operation that emits one.
#[derive(Clone, Debug)]
pub struct Emission {
    /// The output segment, laid out under the input's partition.
    pub segment: GraphSegment,
    /// Its complement segment, `None` for an operation that emits none.
    pub complement: Option<GraphComplementSegment>,
}

/// An operation [`run_segmented`] runs as the six phases of a segmented
/// run: what each task computes from the artifacts of earlier phases.
///
/// The runner owns everything else: the order of the tasks, their input
/// digests, journalling and reuse, cancellation, the budgets, the Emit
/// artifact's layout and the seal. Every method must be deterministic in
/// what it reads, so a resumed run produces the artifacts of an
/// uninterrupted one.
pub trait PhaseOperation {
    /// The operation's refusal.
    type Error: From<SegmentError>;

    /// The operation bytes of the run's identity
    /// ([`RunJournal::run_digest`]).
    fn op(&self) -> &'static [u8];

    /// The operation a global residual names.
    fn segment_op(&self) -> SegmentOp;

    /// The operation's locality class; a global class is declined before
    /// any task.
    fn locality(&self) -> Locality;

    /// The segments the run reads and rewrites, or `None` for every segment
    /// of the partition.
    ///
    /// With `None` the run has a task per segment in each segment phase and
    /// Seal seals every output segment with its streaming instance digest
    /// ([`seal_full`]). With a set, the segment phases run for its segments
    /// only, a segment outside it is never read, and Seal keeps the input
    /// record of every segment outside it, sealing the rewritten records
    /// beside them without an instance digest. An operation returning a set
    /// guarantees that no segment outside it changes and that no edge id of
    /// a rewritten segment is held by a segment outside it.
    ///
    /// # Errors
    ///
    /// The refusal of an input the set cannot be computed for.
    fn affected(&self, cx: &PhaseContext<'_>) -> Result<Option<BTreeSet<SegmentId>>, SegmentError> {
        let _ = cx;
        Ok(None)
    }

    /// The working-set estimate of the input segment `record` describes,
    /// checked against [`Budget::working_set`] before any task.
    ///
    /// # Errors
    ///
    /// A source's failure message.
    fn working_set(&self, record: &SegmentRecord) -> Result<u64, String> {
        Ok(MemoryModel::estimate(&record.stats))
    }

    /// The digests the Validate task of `segment` reads beyond the input
    /// segment's manifest leaf.
    ///
    /// # Errors
    ///
    /// A source's failure message.
    fn validate_inputs(&self, segment: SegmentId) -> Result<Vec<Digest>, String> {
        let _ = segment;
        Ok(Vec::new())
    }

    /// The Validate artifact of `segment`, from the input segment's
    /// canonical bytes `bytes`, already checked against the input manifest.
    ///
    /// # Errors
    ///
    /// A refusal or a source failure.
    fn validate(
        &self,
        cx: &PhaseContext<'_>,
        segment: SegmentId,
        bytes: Vec<u8>,
    ) -> Result<Vec<u8>, PhaseStop<Self::Error>> {
        let _ = (cx, segment);
        Ok(bytes)
    }

    /// The Export artifact of `segment`.
    ///
    /// # Errors
    ///
    /// A refusal or a store failure.
    fn export(
        &self,
        cx: &PhaseContext<'_>,
        segment: SegmentId,
    ) -> Result<Vec<u8>, PhaseStop<Self::Error>>;

    /// The Summarize artifact of `segment`, which may read every Export
    /// artifact.
    ///
    /// # Errors
    ///
    /// A refusal or a store failure.
    fn summarize(
        &self,
        cx: &PhaseContext<'_>,
        segment: SegmentId,
    ) -> Result<Vec<u8>, PhaseStop<Self::Error>>;

    /// The coordinator's Assemble artifact, which may read every earlier
    /// artifact; the operation's refusal is returned here.
    ///
    /// # Errors
    ///
    /// A refusal or a store failure.
    fn assemble(&self, cx: &PhaseContext<'_>) -> Result<Vec<u8>, PhaseStop<Self::Error>>;

    /// The output segment of `segment`, which may read every earlier
    /// artifact.
    ///
    /// # Errors
    ///
    /// A refusal or a store failure.
    fn emit(
        &self,
        cx: &PhaseContext<'_>,
        segment: SegmentId,
    ) -> Result<Emission, PhaseStop<Self::Error>>;

    /// The role the output manifest records.
    fn output_role(&self, cx: &PhaseContext<'_>) -> ManifestRole;
}

/// Graph restriction along a plan as a [`PhaseOperation`]: the phases
/// described in the module documentation.
pub struct GraphRestrict<'p> {
    /// The compiled plan.
    pub plan: &'p RestrictPlan,
}

impl GraphRestrict<'_> {
    fn validated(
        cx: &PhaseContext<'_>,
        segment: SegmentId,
    ) -> Result<GraphSegment, PhaseStop<SegmentedRestrictError>> {
        let bytes = cx.artifact(Phase::Validate, Some(segment))?;
        match AnySegment::from_canonical_bytes(&bytes, cx.partition())?.0 {
            AnySegment::Graph(segment) => Ok(segment),
            _ => Err(PhaseStop::from(malformed(
                ArtifactKind::Segment,
                "not a graph segment",
            ))),
        }
    }

    fn exported(
        cx: &PhaseContext<'_>,
        segment: SegmentId,
    ) -> Result<Export, PhaseStop<SegmentedRestrictError>> {
        Ok(Export::decode(&cx.artifact(Phase::Export, Some(segment))?)?)
    }

    /// The fates the other segments exported to `segment`; `None` when a
    /// segment refused.
    fn imports(
        cx: &PhaseContext<'_>,
        segment: SegmentId,
    ) -> Result<Option<BTreeMap<u32, RemoteFate>>, PhaseStop<SegmentedRestrictError>> {
        let mut imports = BTreeMap::new();
        for id in cx.segments() {
            match Self::exported(cx, id)? {
                Export::Fates(fates) => imports.extend(
                    fates
                        .into_iter()
                        .filter(|((owner, _), _)| *owner == segment)
                        .map(|((_, node), fate)| (node, fate)),
                ),
                Export::Fan(_) | Export::Refused(_) => return Ok(None),
            }
        }
        Ok(Some(imports))
    }

    /// The least refusal of the fan domain and stage 0, recomputed in the
    /// segment that recorded it.
    fn first_refusal(
        &self,
        cx: &PhaseContext<'_>,
    ) -> Result<Option<PhaseStop<SegmentedRestrictError>>, PhaseStop<SegmentedRestrictError>> {
        let mut fan: Option<(u32, SegmentId)> = None;
        let mut refused: Option<(Option<super::error::ErrorKey>, SegmentId)> = None;
        for id in cx.segments() {
            match Self::exported(cx, id)? {
                Export::Fan(node) => {
                    if fan.is_none_or(|(least, _)| node < least) {
                        fan = Some((node, id));
                    }
                }
                Export::Refused(key) => {
                    if refused.is_none_or(|(least, _)| key < least) {
                        refused = Some((key, id));
                    }
                }
                Export::Fates(_) => {}
            }
        }
        let Some(id) = fan.map(|(_, id)| id).or_else(|| refused.map(|(_, id)| id)) else {
            return Ok(None);
        };
        let segment = Self::validated(cx, id)?;
        let local = Local::of(&segment)?;
        if let Some((node, reason)) = local.fan_violation() {
            return Ok(Some(restrict_stop(
                id,
                RestrictError::MalformedFan { node, reason },
            )));
        }
        match decide_nodes(&local.graph, self.plan, local.foreign.clone()) {
            Err(error) => Ok(Some(restrict_stop(id, error))),
            Ok(_) => Err(PhaseStop::from(malformed(
                ArtifactKind::Export,
                "an export records a refusal its segment does not give",
            ))),
        }
    }

    fn summaries(
        cx: &PhaseContext<'_>,
    ) -> Result<Vec<ReachSummary>, PhaseStop<SegmentedRestrictError>> {
        let mut summaries = Vec::new();
        for id in cx.segments() {
            summaries.push(decode_summary(&cx.artifact(Phase::Summarize, Some(id))?)?);
        }
        Ok(summaries)
    }
}

impl PhaseOperation for GraphRestrict<'_> {
    type Error = SegmentedRestrictError;

    fn op(&self) -> &'static [u8] {
        GRAPH_RESTRICT_OP
    }

    fn segment_op(&self) -> SegmentOp {
        SegmentOp::Restrict
    }

    fn locality(&self) -> Locality {
        graph_restrict_locality(self.plan)
    }

    fn export(
        &self,
        cx: &PhaseContext<'_>,
        segment: SegmentId,
    ) -> Result<Vec<u8>, PhaseStop<Self::Error>> {
        let held = Self::validated(cx, segment)?;
        let local = Local::of(&held)?;
        let export = if let Some((node, _)) = local.fan_violation() {
            Export::Fan(node)
        } else {
            match decide_nodes(&local.graph, self.plan, local.foreign.clone()) {
                Err(error) => Export::Refused(refusal_key(&error)),
                Ok(own) => Export::Fates(
                    held.boundary
                        .in_stubs
                        .values()
                        .filter_map(|stub| {
                            own.get(&stub.tgt)
                                .map(|fate| ((stub.src_owner, stub.tgt), fate.clone()))
                        })
                        .collect(),
                ),
            }
        };
        Ok(export.bytes())
    }

    fn summarize(
        &self,
        cx: &PhaseContext<'_>,
        segment: SegmentId,
    ) -> Result<Vec<u8>, PhaseStop<Self::Error>> {
        let Some(imports) = Self::imports(cx, segment)? else {
            return Ok(summary_bytes(&ReachSummary::default()));
        };
        let held = Self::validated(cx, segment)?;
        let local = Local::of(&held)?;
        let own = decide_nodes(&local.graph, self.plan, local.foreign.clone())
            .map_err(|error| restrict_stop(segment, error))?;
        Ok(summary_bytes(&summarize(&held, &local, &own, &imports)))
    }

    fn assemble(&self, cx: &PhaseContext<'_>) -> Result<Vec<u8>, PhaseStop<Self::Error>> {
        if let Some(stop) = self.first_refusal(cx)? {
            return Err(stop);
        }
        let summaries = Self::summaries(cx)?;
        let closed = close_reach(&summaries);
        let mut first: Option<(SegmentId, RestrictError)> = None;
        let mut allocation: Option<SegmentError> = None;
        let mut boundaries = ViewBoundaries::default();
        for (id, summary) in cx.segments().into_iter().zip(&summaries) {
            let segment = Self::validated(cx, id)?;
            let local = Local::of(&segment)?;
            let imports = Self::imports(cx, id)?.unwrap_or_default();
            let outside = outside_of(&local, &imports, &closed);
            match run_scoped(&local.graph, self.plan, outside) {
                Err(error) => keep_least(&mut first, id, error),
                Ok(parts) => match emit_segment(&segment, cx.partition(), parts) {
                    Err(error) => {
                        allocation.get_or_insert(error);
                    }
                    Ok((view, _)) => {
                        let targets: BTreeSet<u32> = segment
                            .boundary
                            .in_stubs
                            .values()
                            .map(|stub| stub.tgt)
                            .chain(summary.kept.keys().copied())
                            .collect();
                        boundaries.add(&view, cx.index(), |node| targets.contains(&node))?;
                    }
                },
            }
        }
        if let Some((segment, error)) = first {
            return Err(restrict_stop(segment, error));
        }
        if let Some(error) = allocation {
            return Err(PhaseStop::from(error));
        }
        let boundaries = boundaries.finish(summaries.len())?;
        Ok(assembly_bytes(&closed, &boundaries))
    }

    fn emit(
        &self,
        cx: &PhaseContext<'_>,
        segment: SegmentId,
    ) -> Result<Emission, PhaseStop<Self::Error>> {
        let assembly = decode_assembly(&cx.artifact(Phase::Assemble, None)?)?;
        let held = Self::validated(cx, segment)?;
        let local = Local::of(&held)?;
        let imports = Self::imports(cx, segment)?.unwrap_or_default();
        let outside = outside_of(&local, &imports, &assembly.closed);
        let parts = run_scoped(&local.graph, self.plan, outside)
            .map_err(|error| restrict_stop(segment, error))?;
        let (mut view, complement) = emit_segment(&held, cx.partition(), parts)?;
        let position = usize::try_from(segment.0).unwrap_or(usize::MAX);
        view.boundary = assembly
            .boundaries
            .into_iter()
            .nth(position)
            .ok_or_else(|| malformed(ArtifactKind::Assembly, "a boundary is missing"))?;
        Ok(Emission {
            segment: view,
            complement: Some(complement),
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
                    shape: ComplementShape::Graph,
                    version: GRAPH_COMPLEMENT_FORMAT_VERSION,
                },
            },
        }
    }
}

/// A run in progress.
struct Runner<'a, O: PhaseOperation, Src, St> {
    op: &'a O,
    input: &'a SegmentManifest,
    source: &'a Src,
    partition: Partition,
    index: OwnerIndex,
    ids: Vec<SegmentId>,
    rewrites: bool,
    spec: RunSpec,
    budget: Budget,
    store: &'a mut St,
    progress: &'a mut dyn FnMut(&Progress),
    run: Digest,
    peak: u64,
    outputs: BTreeMap<TaskKey, Digest>,
    report: SegmentRunReport<O::Error>,
}

impl<O: PhaseOperation, Src: SegmentSource, St: SegmentStore> Runner<'_, O, Src, St> {
    fn ids(&self) -> Vec<SegmentId> {
        self.ids.clone()
    }

    fn cancelled(&self) -> bool {
        self.spec
            .cancel_after_tasks
            .is_some_and(|limit| self.report.progress.tasks_done >= limit)
    }

    const fn key(&self, task: TaskKey, kind: ArtifactKind) -> ArtifactKey {
        ArtifactKey {
            run: self.run,
            task,
            kind,
        }
    }

    /// Run `f` with the phase context of this run.
    fn with_cx<T>(&self, f: impl FnOnce(&PhaseContext<'_>) -> T) -> T {
        let store: &St = self.store;
        let run = self.run;
        let read = move |task: TaskKey| {
            store
                .read(&ArtifactKey {
                    run,
                    task,
                    kind: phase_artifact(task.phase),
                })
                .map_err(|error| error.to_string())
        };
        f(&PhaseContext {
            input: self.input,
            partition: &self.partition,
            index: &self.index,
            spec: &self.spec,
            segments: &self.ids,
            read: &read,
        })
    }

    /// The digest of task `task`'s inputs, from the outputs of `from`.
    fn inputs(&self, task: TaskKey, from: &[TaskKey]) -> Digest {
        let digests: Vec<Digest> = from
            .iter()
            .filter_map(|key| self.outputs.get(key).copied())
            .collect();
        TaskRecord::inputs_digest(&self.spec.lens, task, &digests)
    }

    /// The committed artifact of `task`, when it may be reused.
    fn reusable(
        &self,
        task: TaskKey,
        kind: ArtifactKind,
        inputs: &Digest,
    ) -> Result<Option<Vec<u8>>, Stop<O::Error>> {
        let journal = self.store.journal(&self.run).map_err(store_stop)?;
        let Some(record) = journal.tasks.get(&task) else {
            return Ok(None);
        };
        if record.inputs != *inputs {
            return Ok(None);
        }
        let Some(bytes) = self.store.read(&self.key(task, kind)).map_err(store_stop)? else {
            return Ok(None);
        };
        let reuse = length(&bytes) == record.bytes
            && journal.reusable(&task, inputs, &artifact_digest(&bytes));
        Ok(reuse.then_some(bytes))
    }

    /// Stage and commit a computed artifact.
    fn commit(
        &mut self,
        task: TaskKey,
        kind: ArtifactKind,
        inputs: Digest,
        bytes: &[u8],
    ) -> Result<(), Stop<O::Error>> {
        let key = self.key(task, kind);
        let record = TaskRecord {
            inputs,
            output: artifact_digest(bytes),
            bytes: length(bytes),
        };
        self.store.stage(&key, bytes.to_vec()).map_err(store_stop)?;
        self.store.commit(&key, &record).map_err(store_stop)?;
        Ok(())
    }

    /// Account a finished task.
    fn account(&mut self, task: TaskKey, bytes: &[u8], reused: bool) {
        if reused {
            self.report.tasks_reused += 1;
        } else {
            self.report.tasks_run += 1;
        }
        self.outputs.insert(task, artifact_digest(bytes));
        let progress = &mut self.report.progress;
        progress.tasks_done = progress.tasks_done.saturating_add(1);
        progress.elements_done = progress.elements_done.saturating_add(1);
        progress.phase = task.phase;
        (self.progress)(&self.report.progress);
    }

    /// Run one task with inputs `inputs`: reuse its artifact or compute
    /// and commit it.
    fn task(
        &mut self,
        task: TaskKey,
        inputs: Digest,
        compute: impl FnOnce(&Self) -> Result<Vec<u8>, Stop<O::Error>>,
    ) -> Result<Vec<u8>, Stop<O::Error>> {
        let kind = phase_artifact(task.phase);
        if let Some(bytes) = self.reusable(task, kind, &inputs)? {
            self.account(task, &bytes, true);
            return Ok(bytes);
        }
        let bytes = compute(self)?;
        self.commit(task, kind, inputs, &bytes)?;
        self.account(task, &bytes, false);
        Ok(bytes)
    }

    /// Close a phase's accounting, the phase holding `peak` bytes at once.
    fn finish_phase(&mut self, phase: Phase, before: (u32, u32), peak: u64) {
        self.report.phases.push(PhaseReport {
            phase,
            tasks: self.report.tasks_run - before.0,
            reused: self.report.tasks_reused - before.1,
            peak_estimated_bytes: peak,
            wall_ms: None,
        });
    }

    const fn counts(&self) -> (u32, u32) {
        (self.report.tasks_run, self.report.tasks_reused)
    }

    /// The committed artifact of an earlier task of this invocation.
    fn artifact(&self, task: TaskKey) -> Result<Vec<u8>, Stop<O::Error>> {
        let kind = phase_artifact(task.phase);
        self.store
            .read(&self.key(task, kind))
            .map_err(store_stop)?
            .ok_or_else(|| Stop::from(malformed(kind, "a committed artifact is missing")))
    }

    fn validate(&mut self) -> Result<(), Stop<O::Error>> {
        let before = self.counts();
        for id in self.ids() {
            if self.cancelled() {
                return Err(Stop::Cancelled);
            }
            let record = self
                .input
                .record(id)
                .ok_or_else(|| malformed(ArtifactKind::Manifest, "a record is missing"))?;
            let mut read = vec![record.leaf()];
            read.extend(self.op.validate_inputs(id).map_err(store_stop)?);
            let inputs = TaskRecord::inputs_digest(&self.spec.lens, validate_key(id), &read);
            self.task(validate_key(id), inputs, |runner| {
                let bytes = runner.source.read(id).map_err(store_stop)?;
                verify_segment_bytes(runner.input, &bytes)?;
                Ok(runner.with_cx(|cx| runner.op.validate(cx, id, bytes))?)
            })?;
        }
        self.finish_phase(Phase::Validate, before, self.peak);
        Ok(())
    }

    fn exports(&mut self) -> Result<(), Stop<O::Error>> {
        let before = self.counts();
        for id in self.ids() {
            if self.cancelled() {
                return Err(Stop::Cancelled);
            }
            let inputs = self.inputs(export_key(id), &[validate_key(id)]);
            self.task(export_key(id), inputs, |runner| {
                Ok(runner.with_cx(|cx| runner.op.export(cx, id))?)
            })?;
        }
        self.finish_phase(Phase::Export, before, self.peak);
        Ok(())
    }

    fn summaries(&mut self) -> Result<(), Stop<O::Error>> {
        let before = self.counts();
        let exports: Vec<TaskKey> = self.ids().into_iter().map(export_key).collect();
        for id in self.ids() {
            if self.cancelled() {
                return Err(Stop::Cancelled);
            }
            let mut from = vec![validate_key(id)];
            from.extend(&exports);
            let inputs = self.inputs(summary_key(id), &from);
            self.task(summary_key(id), inputs, |runner| {
                Ok(runner.with_cx(|cx| runner.op.summarize(cx, id))?)
            })?;
        }
        self.finish_phase(Phase::Summarize, before, self.peak);
        Ok(())
    }

    fn assemble(&mut self) -> Result<(), Stop<O::Error>> {
        if self.cancelled() {
            return Err(Stop::Cancelled);
        }
        let before = self.counts();
        let mut from = Vec::new();
        for id in self.ids() {
            from.extend([validate_key(id), export_key(id), summary_key(id)]);
        }
        let task = assemble_key();
        let kind = ArtifactKind::Assembly;
        let inputs = self.inputs(task, &from);
        let (bytes, reused) = match self.reusable(task, kind, &inputs)? {
            Some(bytes) => (bytes, true),
            None => (self.with_cx(|cx| self.op.assemble(cx))?, false),
        };
        let mut needed = length(&bytes);
        for id in self.ids() {
            let summary = self.artifact(summary_key(id))?;
            needed = needed.saturating_add(length(&summary));
        }
        if needed > self.budget.summary {
            return Err(Stop::Phase(PhaseStop::Residual(Residual::BudgetExceeded {
                resource: Resource::Summary,
                phase: Phase::Assemble,
                segment: None,
                needed,
                budget: self.budget.summary,
            })));
        }
        if !reused {
            self.commit(task, kind, inputs, &bytes)?;
        }
        self.account(task, &bytes, reused);
        self.finish_phase(Phase::Assemble, before, self.peak);
        Ok(())
    }

    /// One segment's Emit artifact.
    fn emit_artifact(&self, id: SegmentId) -> Result<Vec<u8>, Stop<O::Error>> {
        let Emission {
            segment,
            complement,
        } = self.with_cx(|cx| self.op.emit(cx, id))?;
        let position = usize::try_from(id.0).unwrap_or(usize::MAX);
        let spec = self
            .partition
            .specs()
            .get(position)
            .ok_or_else(|| malformed(ArtifactKind::Assembly, "a segment spec is missing"))?;
        let view = AnySegment::Graph(segment);
        let view_bytes = view.canonical_bytes(&self.spec.output_schema, &self.partition)?;
        let record = SegmentRecord::of(&view, spec, &self.spec.output_schema);
        Ok(emit_bytes(
            &view,
            &view_bytes,
            complement.as_ref(),
            &record,
        )?)
    }

    fn emit(&mut self) -> Result<(), Stop<O::Error>> {
        let before = self.counts();
        let mut shared: Vec<TaskKey> = self.ids().into_iter().map(export_key).collect();
        shared.extend(self.ids().into_iter().map(summary_key));
        shared.push(assemble_key());
        for id in self.ids() {
            if self.cancelled() {
                return Err(Stop::Cancelled);
            }
            let task = emit_key(id);
            let kind = ArtifactKind::ComplementSegment;
            let mut from = vec![validate_key(id)];
            from.extend(&shared);
            let inputs = self.inputs(task, &from);
            let (bytes, reused) = match self.reusable(task, kind, &inputs)? {
                Some(bytes) => (bytes, true),
                None => (self.emit_artifact(id)?, false),
            };
            let (layout, _, complement) = decode_emit(&bytes)?;
            let (elements, size) = (layout.elements, length(complement));
            let needed = self.report.complement.bytes.saturating_add(size);
            if let Some(limit) = self.budget.complement
                && needed > limit
            {
                return Err(Stop::Phase(PhaseStop::Residual(Residual::BudgetExceeded {
                    resource: Resource::Complement,
                    phase: Phase::Emit,
                    segment: Some(id),
                    needed,
                    budget: limit,
                })));
            }
            if !reused {
                self.commit(task, kind, inputs, &bytes)?;
            }
            self.account(task, &bytes, reused);
            let complement = &mut self.report.complement;
            complement.elements = complement.elements.saturating_add(elements);
            complement.bytes = needed;
            if size > 0 {
                complement.per_segment.insert(id, (elements, size));
            }
        }
        self.finish_phase(Phase::Emit, before, self.peak);
        Ok(())
    }

    /// The output manifest, sealed from the emitted artifacts, and the
    /// bytes Seal holds at once.
    ///
    /// Each Emit artifact's header and manifest record are read whole; its
    /// edge id run and record stream are read one id and one frame at a
    /// time while [`seal_full`] merges them. Seal thus holds, per artifact,
    /// its header, its record, one id when the run is not empty and at most
    /// its longest frame with its length prefix; the sum over the artifacts
    /// bounds what it holds at once. An operation with an affected set
    /// ([`PhaseOperation::affected`]) is resealed instead ([`reseal`]): the
    /// records of the rewritten segments replace their input records, their
    /// edge id runs are merged and no record stream is read, so Seal holds
    /// each artifact's header, its record and one id.
    fn sealed(&self) -> Result<(SegmentManifest, u64), Stop<O::Error>> {
        let journal = self.store.journal(&self.run).map_err(store_stop)?;
        let kind = ArtifactKind::ComplementSegment;
        let mut sections = Vec::new();
        let mut records = Vec::new();
        let mut held: u64 = 0;
        for id in self.ids() {
            let key = self.key(emit_key(id), kind);
            let header = self
                .store
                .read_at(&key, 0, EMIT_HEADER_LEN)
                .map_err(store_stop)?
                .ok_or_else(|| Stop::from(missing_section(kind)))?;
            let layout = EmitLayout::decode(&header)?;
            if journal.tasks.get(&emit_key(id)).map(|task| task.bytes) != Some(layout.end()?) {
                return Err(Stop::from(malformed(
                    kind,
                    "the sections of an emitted artifact do not span it",
                )));
            }
            let record = self
                .store
                .read_at(&key, layout.record_at(), layout.record)
                .map_err(store_stop)?
                .ok_or_else(|| Stop::from(missing_section(kind)))?;
            let record = SegmentRecord::from_canonical_bytes(&record)?;
            let one_id = if layout.run_ids == 0 { 0 } else { 4 };
            let longest = if self.rewrites { 0 } else { layout.longest };
            held = held
                .saturating_add(EMIT_HEADER_LEN)
                .saturating_add(layout.record)
                .saturating_add(one_id)
                .saturating_add(longest);
            sections.push((id, key, layout, record.stats));
            records.push(record);
        }
        let failure: ReadFailure = RefCell::new(None);
        let store: &St = self.store;
        let runs: Vec<EdgeIdRun<'_>> = sections
            .iter()
            .map(|(id, key, layout, _)| {
                (
                    *id,
                    Box::new(StoredIds {
                        store,
                        key: *key,
                        at: layout.run_at(),
                        end: layout.stream_at(),
                        failure: &failure,
                    }) as Box<dyn Iterator<Item = crate::ginstance::EdgeId>>,
                )
            })
            .collect();
        let role = self.with_cx(|cx| self.op.output_role(cx));
        let sealed = if self.rewrites {
            reseal(self.input, role, records, runs)
        } else {
            let sources = sections
                .iter()
                .map(|(_, key, layout, stats)| {
                    Box::new(StoredRecords {
                        store,
                        key: *key,
                        stats: *stats,
                        at: layout.stream_at(),
                        end: layout.stream_at() + layout.stream,
                        failure: &failure,
                    }) as Box<dyn RecordSource + '_>
                })
                .collect();
            let header = ManifestHeader {
                shape: InstanceShapeKind::Graph,
                schema: self.spec.output_schema,
                role,
                binding: self.input.binding,
            };
            seal_full(records, header, runs, sources)
        };
        if let Some(stop) = failure.into_inner() {
            return Err(Stop::from(stop));
        }
        Ok((sealed?, held))
    }

    fn seal(&mut self) -> Result<SegmentManifest, Stop<O::Error>> {
        if self.cancelled() {
            return Err(Stop::Cancelled);
        }
        let before = self.counts();
        let from: Vec<TaskKey> = self.ids().into_iter().map(emit_key).collect();
        let inputs = self.inputs(seal_key(), &from);
        let (manifest, held) = self.sealed()?;
        let computed = manifest.canonical_bytes();
        let bytes = self.task(seal_key(), inputs, |_| Ok(computed.clone()))?;
        self.finish_phase(Phase::Seal, before, held);
        if bytes != computed {
            return Err(Stop::from(malformed(
                ArtifactKind::Manifest,
                "the sealed manifest differs from its emitted segments",
            )));
        }
        Ok(manifest)
    }

    fn phases(&mut self) -> Result<SegmentManifest, Stop<O::Error>> {
        self.validate()?;
        self.exports()?;
        self.summaries()?;
        self.assemble()?;
        self.emit()?;
        self.seal()
    }

    fn orphans(&mut self) -> Result<(), String> {
        match self.store.staged_uncommitted(&self.run) {
            Ok(orphans) => {
                self.report.orphans = orphans;
                Ok(())
            }
            Err(error) => Err(error.to_string()),
        }
    }

    fn stop(mut self, stop: Stop<O::Error>) -> SegmentedRun<O::Error> {
        self.report.peak_estimated_bytes = self.peak;
        self.report.status = match stop {
            Stop::Cancelled => match self.orphans() {
                Ok(()) => RunStatus::Incomplete {
                    reason: RunIncompleteReason::Cancelled,
                },
                Err(error) => RunStatus::Incomplete {
                    reason: RunIncompleteReason::Store {
                        message: format!("orphan scan failed after cancellation: {error}"),
                    },
                },
            },
            Stop::Phase(PhaseStop::Store(message)) => match self.orphans() {
                Ok(()) => RunStatus::Incomplete {
                    reason: RunIncompleteReason::Store { message },
                },
                Err(error) => RunStatus::Incomplete {
                    reason: RunIncompleteReason::Store {
                        message: format!("{message}; orphan scan failed: {error}"),
                    },
                },
            },
            Stop::Phase(PhaseStop::Failed(error)) => RunStatus::Failed(error),
            Stop::Phase(PhaseStop::Residual(residual)) => RunStatus::Residual(residual),
        };
        SegmentedRun {
            run: Some(self.run),
            manifest: None,
            report: self.report,
        }
    }
}

const fn restrict_stop(
    segment: SegmentId,
    error: RestrictError,
) -> PhaseStop<SegmentedRestrictError> {
    PhaseStop::Failed(SegmentedRestrictError::Restrict { segment, error })
}

const fn task_key(phase: Phase, segment: Option<SegmentId>) -> TaskKey {
    TaskKey { phase, segment }
}

const fn validate_key(segment: SegmentId) -> TaskKey {
    task_key(Phase::Validate, Some(segment))
}

const fn export_key(segment: SegmentId) -> TaskKey {
    task_key(Phase::Export, Some(segment))
}

const fn summary_key(segment: SegmentId) -> TaskKey {
    task_key(Phase::Summarize, Some(segment))
}

const fn assemble_key() -> TaskKey {
    task_key(Phase::Assemble, None)
}

const fn emit_key(segment: SegmentId) -> TaskKey {
    task_key(Phase::Emit, Some(segment))
}

const fn seal_key() -> TaskKey {
    task_key(Phase::Seal, None)
}

/// A run refused before its first task: its status and, when the partition
/// gives one, its identity.
type Refused<E> = (RunStatus<E>, Option<Digest>);

/// What the checks before the first task establish.
struct Prepared {
    partition: Partition,
    index: OwnerIndex,
    ids: Vec<SegmentId>,
    rewrites: bool,
    run: Digest,
    peak: u64,
}

/// The checks before the first task: the input manifest, its shape and
/// partition, the operation's locality and affected segments, the number of
/// segments the run reads, and each of their records' working-set estimate.
/// A refusal comes with the run identity when the partition gives one.
/// `progress` is set to the run's task count once the segments it reads are
/// known.
fn preflight<O: PhaseOperation>(
    op: &O,
    input: &SegmentManifest,
    spec: RunSpec,
    budget: Budget,
    progress: &mut Progress,
) -> Result<Prepared, Box<Refused<O::Error>>> {
    let failed = |error: SegmentError| Box::new((RunStatus::Failed(O::Error::from(error)), None));
    verify_manifest(input, spec.input).map_err(failed)?;
    if input.shape != InstanceShapeKind::Graph {
        return Err(failed(SegmentError::ShapeMismatch {
            expected: InstanceShapeKind::Graph,
            found: input.shape,
        }));
    }
    let partition = input.partition().map_err(failed)?;
    let index = partition.owner_index().map_err(failed)?;
    let run = RunJournal::run_digest(&spec.lens, &spec.input, &partition, op.op());
    let declined = |residual| Box::new((RunStatus::Residual(residual), Some(run)));
    if let Locality::Global(reason) = op.locality() {
        return Err(declined(Residual::Global {
            op: op.segment_op(),
            reason,
        }));
    }
    let every: Vec<SegmentId> = partition.specs().iter().map(|spec| spec.id).collect();
    let affected = op
        .affected(&PhaseContext {
            input,
            partition: &partition,
            index: &index,
            spec: &spec,
            segments: &every,
            read: &|_| Ok(None),
        })
        .map_err(|error| Box::new((RunStatus::Failed(O::Error::from(error)), Some(run))))?;
    let rewrites = affected.is_some();
    let ids: Vec<SegmentId> = match affected {
        Some(set) => set.into_iter().collect(),
        None => every,
    };
    let count = u32::try_from(ids.len()).unwrap_or(u32::MAX);
    let total = count.saturating_mul(4).saturating_add(2);
    progress.tasks_total = total;
    progress.elements_total = u64::from(total);
    let segments = u64::from(count);
    if segments > u64::from(budget.affected_segments) {
        return Err(declined(Residual::BudgetExceeded {
            resource: Resource::AffectedSegments,
            phase: Phase::Validate,
            segment: None,
            needed: segments,
            budget: u64::from(budget.affected_segments),
        }));
    }
    let mut peak = 0;
    for id in &ids {
        let record = input
            .record(*id)
            .ok_or_else(|| failed(malformed(ArtifactKind::Manifest, "a record is missing")))?;
        let estimate = op.working_set(record).map_err(|message| {
            Box::new((
                RunStatus::Incomplete {
                    reason: RunIncompleteReason::Store { message },
                },
                Some(run),
            ))
        })?;
        if estimate > budget.working_set {
            return Err(declined(Residual::BudgetExceeded {
                resource: Resource::WorkingSet,
                phase: Phase::Validate,
                segment: Some(record.stats.segment),
                needed: estimate,
                budget: budget.working_set,
            }));
        }
        peak = peak.max(estimate);
    }
    Ok(Prepared {
        partition,
        index,
        ids,
        rewrites,
        run,
        peak,
    })
}

/// Run the operation `op` over the graph the input manifest `input` seals,
/// reading its segments from `source`, as a checkpointed run in `store`.
///
/// The run's identity is [`RunJournal::run_digest`] of the lens, the input
/// manifest's digest, the partition and the operation's bytes
/// ([`PhaseOperation::op`]). Before any task the input manifest is verified
/// against `spec.input` ([`verify_manifest`]), an operation whose locality
/// is global is declined, and the number of segments and the working-set
/// estimate of every record ([`PhaseOperation::working_set`]) are checked
/// against `budget`. The six phases then run, `4n + 2` tasks for `n`
/// segments, each task committing one artifact ([`phase_artifact`]) with its
/// journal record and reading only the artifacts of the tasks its input
/// digest names: Validate of a segment its input segment, Export the
/// segment's Validate, Summarize the segment's Validate and every Export,
/// Assemble every earlier task, Emit the segment's Validate, every Export
/// and Summarize and Assemble, and Seal every Emit. Validate checks the
/// input segment against the input manifest before the operation reads it;
/// Assemble declines when the summaries and its own artifact exceed the
/// summary budget; each Emit task writes the operation's output segment and
/// complement in the Emit layout and is preceded by the running complement
/// budget; Seal seals the output segments under `spec.output_schema` with
/// the operation's role, reading each Emit artifact a section at a time.
/// `progress` is called after every committed or reused task, and the run
/// stops once `spec.cancel_after_tasks` tasks are done.
pub fn run_segmented<O: PhaseOperation, Src: SegmentSource, St: SegmentStore>(
    op: &O,
    input: &SegmentManifest,
    source: &Src,
    spec: RunSpec,
    budget: Budget,
    store: &mut St,
    progress: &mut dyn FnMut(&Progress),
) -> SegmentedRun<O::Error> {
    let segments = u32::try_from(input.segments.len()).unwrap_or(u32::MAX);
    let total = segments.saturating_mul(4).saturating_add(2);
    let mut report = SegmentRunReport {
        status: RunStatus::Incomplete {
            reason: RunIncompleteReason::Cancelled,
        },
        locality: op.locality(),
        budget,
        progress: Progress {
            phase: Phase::Validate,
            tasks_done: 0,
            tasks_total: total,
            elements_done: 0,
            elements_total: u64::from(total),
        },
        peak_estimated_bytes: 0,
        observed_peak_bytes: None,
        complement: ComplementSize::default(),
        tasks_run: 0,
        tasks_reused: 0,
        orphans: Vec::new(),
        phases: Vec::new(),
    };
    let prepared = match preflight(op, input, spec, budget, &mut report.progress) {
        Ok(prepared) => prepared,
        Err(refused) => {
            let (status, run) = *refused;
            report.status = status;
            return SegmentedRun {
                run,
                manifest: None,
                report,
            };
        }
    };
    let Prepared {
        partition,
        index,
        ids,
        rewrites,
        run,
        peak,
    } = prepared;
    report.peak_estimated_bytes = peak;
    let mut runner = Runner {
        op,
        input,
        source,
        partition,
        index,
        ids,
        rewrites,
        spec,
        budget,
        store,
        progress,
        run,
        peak,
        outputs: BTreeMap::new(),
        report,
    };
    let manifest = match runner.phases() {
        Ok(manifest) => manifest,
        Err(stop) => return runner.stop(stop),
    };
    if let Err(message) = runner.orphans() {
        runner.report.status = RunStatus::Incomplete {
            reason: RunIncompleteReason::Store { message },
        };
        return SegmentedRun {
            run: Some(run),
            manifest: None,
            report: runner.report,
        };
    }
    runner.report.status = RunStatus::Complete {
        manifest: manifest.digest(),
    };
    SegmentedRun {
        run: Some(run),
        manifest: Some(manifest),
        report: runner.report,
    }
}

/// Run segmented graph restriction along `plan` as a checkpointed run.
///
/// The run is [`run_segmented`] of [`GraphRestrict`] over the graph the
/// input manifest `input` seals, reading its segments from `source` and
/// committing to `store`; the module documentation describes its phases.
///
/// The view segments and complement segments of a complete run are the
/// output of [`graph_restrict_segmented`] on the input segments, with the
/// view segments written under `spec.output_schema`; read them back with
/// [`read_emitted`].
///
/// [`graph_restrict_segmented`]: super::graph_restrict_segmented
pub fn graph_restrict_run<Src: SegmentSource, St: SegmentStore>(
    plan: &RestrictPlan,
    input: &SegmentManifest,
    source: &Src,
    spec: RunSpec,
    budget: Budget,
    store: &mut St,
    progress: &mut dyn FnMut(&Progress),
) -> GraphRestrictRun {
    run_segmented(
        &GraphRestrict { plan },
        input,
        source,
        spec,
        budget,
        store,
        progress,
    )
}
