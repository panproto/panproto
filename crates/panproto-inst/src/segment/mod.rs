//! Segmented instances: one logical tree, table or graph instance held as a
//! set of segments.
//!
//! A partition assigns every node id (tree node, graph node, table row) to the
//! segment whose id range contains it, and gives each segment the ranges from
//! which it allocates fresh and synthesized ids. This module holds:
//!
//! - the id vocabulary: [`SegmentId`], the inclusive [`IdRange`] and the
//!   [`IdAllocator`] that hands out fresh ids above a floor;
//! - partitions ([`Partition`], [`SegmentSpec`]) and the ownership rule: a
//!   node is owned by the segment whose range contains its id, an edge by
//!   the owner of its source;
//! - segment bodies ([`GraphSegment`], [`TableSegment`], [`TreeSegment`],
//!   [`AnySegment`]) with their [`Boundary`] of [`Ghost`]s and [`InStub`]s,
//!   the splitters that cut a whole instance into segments and the assemblers
//!   that put them back together;
//! - canonical bytes and digests of segments, the streaming
//!   [`instance_digest`], and the [`SegmentManifest`] that binds the segments
//!   under a Merkle root and is verified without the segments;
//! - the task, journal and storage vocabulary of a checkpointed run
//!   ([`TaskKey`], [`RunJournal`], [`SegmentStore`]);
//! - segmented graph restriction ([`graph_restrict_segmented`]), which runs
//!   the graph restrict engine segment by segment and lays its view and
//!   complement out under the source's partition, and its checkpointed run
//!   ([`graph_restrict_run`]) as resumable tasks in a caller's store;
//! - segmented graph extension ([`graph_extend_segmented`]), which runs the
//!   graph extend engine segment by segment under the source's partition,
//!   and its checkpointed run ([`graph_extend_run`]);
//! - segmented edits ([`apply_segmented`]), which apply a
//!   [`GraphDelta`](crate::graph_edit::GraphDelta), a
//!   [`TableDelta`](crate::table_delta::TableDelta) or a
//!   [`TreeEdit`](crate::tree_edit::TreeEdit) reading only the segments it
//!   can change ([`FamilyIndex::read_set`]), and their
//!   checkpointed run ([`apply_run`]), with [`FreshIds`] for the ids a delta
//!   adds;
//! - the typed failure [`SegmentError`] and the [`Residual`] of an operation
//!   declined within a budget.

mod apply;
mod body;
mod builder;
mod bytes;
mod check;
mod error;
mod extend;
mod graph_apply;
mod ids;
mod manifest;
mod partition;
mod reader;
pub use reader::Reader;
mod restrict;
mod runner;
mod split;
mod store;
mod stream;
mod table_apply;
#[cfg(test)]
mod tests;
mod tree_apply;
mod tree_bytes;
mod validate;

pub use crate::identity::SEGMENT_EDGE_IDS_CONTEXT;
pub use apply::{
    ApplyRun, FamilyIndex, FreshIds, SegmentApply, SegmentDelta, SegmentedApplyError, apply_run,
    apply_segmented,
};
pub use body::{
    AnySegment, Boundary, Ghost, GraphSegment, InStub, SegmentStats, TableSegment, TreeSegment,
};
pub use builder::{BuilderEdge, BuilderNode, SegmentBuilder};
pub use bytes::{read_boundary, write_boundary, write_segment, write_view_ref};
pub use check::{check_segment, verify_partition};
pub use error::{ErrorAt, ErrorKey, PartitionDefect, Residual, Resource, SegmentError};
pub use extend::{
    GRAPH_EXTEND_OP, GraphExtend, GraphExtendRun, SegmentedExtendError, graph_extend_locality,
    graph_extend_run, graph_extend_segmented,
};
pub use graph_apply::GRAPH_APPLY_OP;
pub use ids::{EdgeFloor, IdAllocator, IdKind, IdRange, RangeExhausted, SegmentId};
pub use manifest::{
    EdgeIdRun, ManifestHeader, ManifestRole, SegmentManifest, SegmentRecord, audit_path,
    check_edge_id_runs, edge_ids_digest, merkle_root, seal, seal_full, verify_audit_path,
    verify_manifest, verify_segment, verify_segment_bytes,
};
pub use partition::{OwnerIndex, Partition, SegmentSpec, shape_from_tag, shape_tag};
pub use restrict::{
    FetchSegment, GraphComplementSegment, SegmentedRestrict, SegmentedRestrictError, StreamStop,
    graph_restrict_locality, graph_restrict_segmented, graph_restrict_segmented_held,
    graph_restrict_segmented_nodes, graph_restrict_segmented_nodes_with,
};
pub use runner::{
    Budget, ComplementSize, Emission, EmittedSegment, GRAPH_RESTRICT_OP, GraphRestrict,
    GraphRestrictRun, HeldComplements, MemoryModel, MemoryStore, MissingSegment, PhaseContext,
    PhaseOperation, PhaseReport, PhaseStop, Progress, RunIncompleteReason, RunSpec, RunStatus,
    SegmentRunReport, SegmentSource, SegmentedRun, graph_restrict_run, phase_artifact,
    read_emitted, read_emitted_segment, run_segmented, store_failure,
};
pub use split::{
    assemble_graph, assemble_table, assemble_tree, partition_graph, partition_table, partition_tree,
};
pub use store::{ArtifactKey, ArtifactKind, Phase, RunJournal, SegmentStore, TaskKey, TaskRecord};
pub use stream::{
    InstanceRecord, RecordSource, SegmentRecords, decode_record_stream, instance_digest,
    record_stream,
};
pub use table_apply::TABLE_APPLY_OP;
pub use tree_apply::TREE_APPLY_OP;
pub use tree_bytes::{
    FanRecord, canonical_winstance_bytes, winstance_digest, write_arc_record, write_winstance,
    write_winstance_header,
};
pub use validate::validate_segment;
