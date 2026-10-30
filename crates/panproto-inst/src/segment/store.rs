//! Tasks, journals and the storage a segmented run is checkpointed in.
//!
//! A segmented run is a sequence of [`Phase`]s, each a set of tasks: one per
//! segment, or one for the coordinator. A task is named by its [`TaskKey`]
//! and recorded, once its output is committed, as a [`TaskRecord`] in the
//! run's [`RunJournal`]. A task whose key is journaled with the same inputs,
//! and whose artifact still hashes to the recorded output, is reused by a
//! later run; every other task is recomputed. Since every phase is
//! deterministic, a recomputed task writes the same bytes.
//!
//! The caller supplies the storage through [`SegmentStore`]. Its
//! [`commit`](SegmentStore::commit) must make a staged artifact readable and
//! record its task in one atomic step (Dean and Ghemawat 2004, Sec 3.3,
//! stated), so an interrupted run leaves either a journaled artifact or a
//! staged one the journal does not name; a run never reads the latter.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::ids::SegmentId;
use super::partition::Partition;
use crate::identity::{CanonicalSink, Digest, RUN_JOURNAL_CONTEXT, SEGMENT_TASK_CONTEXT};

/// A phase of a segmented run, in the order a run executes them.
///
/// Serialized in `snake_case`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Validate each segment.
    Validate,
    /// Send each segment's boundary facts to the segments that need them.
    Export,
    /// Summarize each segment for the coordinator.
    Summarize,
    /// Assemble the summaries (coordinator).
    Assemble,
    /// Write each output segment and complement segment.
    Emit,
    /// Seal the output manifest (coordinator).
    Seal,
}

impl Phase {
    /// The phase's one-byte tag in canonical bytes: its position in run
    /// order.
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::Validate => 0,
            Self::Export => 1,
            Self::Summarize => 2,
            Self::Assemble => 3,
            Self::Emit => 4,
            Self::Seal => 5,
        }
    }
}

/// The name of one task: its phase and its segment, `None` for the
/// coordinator.
///
/// Keys order by phase, then segment, with the coordinator first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TaskKey {
    /// The phase.
    pub phase: Phase,
    /// The segment, or `None` for the coordinator.
    pub segment: Option<SegmentId>,
}

impl TaskKey {
    /// Write the key's canonical bytes: the phase tag, then the segment as
    /// a presence byte and its id.
    pub fn write<S: CanonicalSink + ?Sized>(&self, sink: &mut S) {
        sink.tag(self.phase.tag());
        match self.segment {
            None => sink.bool(false),
            Some(segment) => {
                sink.bool(true);
                sink.u32(segment.0);
            }
        }
    }
}

impl fmt::Display for TaskKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.segment {
            None => write!(f, "{:?} (coordinator)", self.phase),
            Some(segment) => write!(f, "{:?} of {segment}", self.phase),
        }
    }
}

/// The journal record of a committed task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskRecord {
    /// The digest of the task's inputs ([`TaskRecord::inputs_digest`]).
    pub inputs: Digest,
    /// The digest of the task's output artifact.
    pub output: Digest,
    /// The length of the output artifact in bytes.
    pub bytes: u64,
}

impl TaskRecord {
    /// The digest naming a task's inputs: [`SEGMENT_TASK_CONTEXT`] over the
    /// lens identity, the task key and the sorted multiset of input-artifact
    /// digests.
    ///
    /// An execution budget is not part of this digest.
    ///
    /// ```
    /// use panproto_inst::identity::Digest;
    /// use panproto_inst::segment::{Phase, SegmentId, TaskKey, TaskRecord};
    ///
    /// let lens = Digest([1; 32]);
    /// let task = TaskKey { phase: Phase::Emit, segment: Some(SegmentId(0)) };
    /// let (a, b) = (Digest([2; 32]), Digest([3; 32]));
    /// // The input digests form a sorted multiset: their order does not matter.
    /// assert_eq!(
    ///     TaskRecord::inputs_digest(&lens, task, &[a, b]),
    ///     TaskRecord::inputs_digest(&lens, task, &[b, a]),
    /// );
    /// ```
    #[must_use]
    pub fn inputs_digest(lens: &Digest, task: TaskKey, inputs: &[Digest]) -> Digest {
        let mut sorted = inputs.to_vec();
        sorted.sort_unstable();
        let mut hasher = Digest::hasher(SEGMENT_TASK_CONTEXT);
        hasher.write(&lens.0);
        task.write(&mut hasher);
        hasher.len(sorted.len());
        for digest in &sorted {
            hasher.write(&digest.0);
        }
        hasher.finalize()
    }
}

/// The journal of one segmented run: the run's identity and every committed
/// task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunJournal {
    /// The run's identity ([`RunJournal::run_digest`]).
    pub run: Digest,
    /// The committed tasks.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub tasks: BTreeMap<TaskKey, TaskRecord>,
}

impl RunJournal {
    /// The empty journal of run `run`.
    #[must_use]
    pub const fn new(run: Digest) -> Self {
        Self {
            run,
            tasks: BTreeMap::new(),
        }
    }

    /// The identity of a run: [`RUN_JOURNAL_CONTEXT`] over the lens
    /// identity, the input manifest's digest, the partition's canonical
    /// bytes ([`Partition::canonical_bytes`]) and the canonical bytes of
    /// the operation, which the operation's own type writes.
    #[must_use]
    pub fn run_digest(lens: &Digest, input: &Digest, partition: &Partition, op: &[u8]) -> Digest {
        let mut hasher = Digest::hasher(RUN_JOURNAL_CONTEXT);
        hasher.write(&lens.0);
        hasher.write(&input.0);
        hasher.bytes(&partition.canonical_bytes());
        hasher.bytes(op);
        hasher.finalize()
    }

    /// Whether the task is journaled with these inputs and its artifact
    /// hashes to the recorded output. A task for which this holds is reused;
    /// every other task is recomputed.
    #[must_use]
    pub fn reusable(&self, task: &TaskKey, inputs: &Digest, artifact_digest: &Digest) -> bool {
        self.tasks
            .get(task)
            .is_some_and(|record| record.inputs == *inputs && record.output == *artifact_digest)
    }
}

/// The kind of an artifact a segmented run stores.
///
/// Serialized in `snake_case`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// An instance segment.
    Segment,
    /// A complement segment.
    ComplementSegment,
    /// An export batch.
    Export,
    /// A segment summary.
    Summary,
    /// The coordinator's assembly.
    Assembly,
    /// A manifest.
    Manifest,
    /// A sorted run of a segment's edge ids.
    EdgeIdRun,
}

/// The name of one stored artifact: its run, its task and its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ArtifactKey {
    /// The run that wrote it.
    pub run: Digest,
    /// The task that wrote it.
    pub task: TaskKey,
    /// What it is.
    pub kind: ArtifactKind,
}

/// The storage a segmented run reads and writes, supplied by the caller.
///
/// An artifact is first staged, then committed together with its task's
/// journal record. A run reads only artifacts its journal names, so an
/// artifact staged but never committed is never read; the run reports it for
/// the caller to delete ([`SegmentStore::staged_uncommitted`]).
pub trait SegmentStore {
    /// The store's own error.
    type Error: std::error::Error + Send + Sync + 'static;

    /// The bytes of a committed artifact, or `None` when there is none.
    ///
    /// # Errors
    ///
    /// The store's error when it cannot be read.
    fn read(&self, key: &ArtifactKey) -> Result<Option<Vec<u8>>, Self::Error>;

    /// Stage an artifact's bytes without making them readable.
    ///
    /// # Errors
    ///
    /// The store's error when they cannot be written.
    fn stage(&mut self, key: &ArtifactKey, bytes: Vec<u8>) -> Result<(), Self::Error>;

    /// Atomically make a staged artifact readable and record its task in the
    /// journal of the key's run.
    ///
    /// # Errors
    ///
    /// The store's error when the commit fails; the artifact then stays
    /// staged and unrecorded.
    fn commit(&mut self, key: &ArtifactKey, record: &TaskRecord) -> Result<(), Self::Error>;

    /// The journal of a run, empty when the run has committed nothing.
    ///
    /// # Errors
    ///
    /// The store's error when it cannot be read.
    fn journal(&self, run: &Digest) -> Result<RunJournal, Self::Error>;

    /// The artifacts of a run that are staged but not committed.
    ///
    /// # Errors
    ///
    /// The store's error when it cannot be read.
    fn staged_uncommitted(&self, run: &Digest) -> Result<Vec<ArtifactKey>, Self::Error>;
}
