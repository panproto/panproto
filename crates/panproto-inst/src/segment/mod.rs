//! Segmented instances: one logical tree, table or graph instance held as a
//! set of segments.
//!
//! A partition assigns every node id (tree node, graph node, table row) to the
//! segment whose id range contains it, and gives each segment the ranges from
//! which it allocates fresh and synthesized ids. This module holds the id
//! vocabulary of that arrangement:
//!
//! - [`SegmentId`] numbers the segments of one partition.
//! - [`IdRange`] is an inclusive range of `u32` ids.
//! - [`IdAllocator`] hands out fresh ids from one range, above a floor that
//!   no id of the source reaches. The whole-state engines use one allocator
//!   over [`IdRange::FULL`] per id kind; a segment uses the ranges its
//!   partition carves out for it.
//! - [`SegmentError`] is the typed failure of a segmented operation.

mod ids;

pub use ids::{
    IdAllocator, IdKind, IdRange, RangeExhausted, SegmentError, SegmentId, check_edge_range_above,
};
