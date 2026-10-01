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
//! - the typed failure [`SegmentError`] and the [`Residual`] of an operation
//!   declined within a budget.

mod body;
mod builder;
mod bytes;
mod check;
mod error;
mod ids;
mod manifest;
mod partition;
mod reader;
mod split;
mod store;
mod stream;
#[cfg(test)]
mod tests;
mod tree_bytes;

pub use body::{
    AnySegment, Boundary, Ghost, GraphSegment, InStub, SegmentStats, TableSegment, TreeSegment,
};
pub use builder::{BuilderEdge, BuilderNode, SegmentBuilder};
pub use bytes::{write_segment, write_view_ref};
pub use check::{check_segment, verify_partition};
pub use error::{ErrorAt, ErrorKey, PartitionDefect, Residual, Resource, SegmentError};
pub use ids::{IdAllocator, IdKind, IdRange, RangeExhausted, SegmentId, check_edge_range_above};
pub use manifest::{
    EdgeIdRun, ManifestHeader, ManifestRole, SEGMENT_EDGE_IDS_CONTEXT, SegmentManifest,
    SegmentRecord, audit_path, check_edge_id_runs, edge_ids_digest, merkle_root, seal, seal_full,
    verify_audit_path, verify_manifest, verify_segment, verify_segment_bytes,
};
pub use partition::{OwnerIndex, Partition, SegmentSpec, shape_from_tag, shape_tag};
pub use split::{
    assemble_graph, assemble_table, assemble_tree, partition_graph, partition_table, partition_tree,
};
pub use store::{ArtifactKey, ArtifactKind, Phase, RunJournal, SegmentStore, TaskKey, TaskRecord};
pub use stream::{InstanceRecord, RecordSource, SegmentRecords, instance_digest};
pub use tree_bytes::{
    FanRecord, canonical_winstance_bytes, winstance_digest, write_arc_record, write_winstance,
    write_winstance_header,
};
