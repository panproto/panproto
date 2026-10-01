//! Partitions: which segment owns which node ids, and where each segment
//! allocates.
//!
//! A [`Partition`] lists one [`SegmentSpec`] per segment. A node (a tree
//! node, a graph node, a fan node or a table row) is owned by the segment
//! whose node ranges contain its id; an edge (a graph edge, a tree arc, a fan
//! edge or a foreign-key pair) by the owner of its source. So a node and all
//! its out-edges are always in one segment. Edge ids are not owned by range:
//! their uniqueness across segments is checked when a manifest is sealed.

use panproto_schema::{CanonicalWriter, InstanceShapeKind};
use serde::{Deserialize, Serialize};

use super::error::{PartitionDefect, SegmentError};
use super::ids::{IdKind, IdRange, SegmentId};
use crate::identity::CanonicalSink;

/// The one-byte tag of an instance shape in canonical bytes: `0` tree, `1`
/// table, `2` graph.
#[must_use]
pub const fn shape_tag(shape: InstanceShapeKind) -> u8 {
    match shape {
        InstanceShapeKind::Tree => 0,
        InstanceShapeKind::Table => 1,
        InstanceShapeKind::Graph => 2,
    }
}

/// The shape a canonical shape tag names.
#[must_use]
pub const fn shape_from_tag(tag: u8) -> Option<InstanceShapeKind> {
    match tag {
        0 => Some(InstanceShapeKind::Tree),
        1 => Some(InstanceShapeKind::Table),
        2 => Some(InstanceShapeKind::Graph),
        _ => None,
    }
}

/// Write an optional range as a presence byte and then its bounds.
pub(crate) fn write_opt_range<S: CanonicalSink + ?Sized>(sink: &mut S, range: Option<IdRange>) {
    match range {
        None => sink.bool(false),
        Some(range) => {
            sink.bool(true);
            sink.u32(range.lo);
            sink.u32(range.hi);
        }
    }
}

/// Write a list of ranges as their count and then each range's bounds.
pub(crate) fn write_ranges<S: CanonicalSink + ?Sized>(sink: &mut S, ranges: &[IdRange]) {
    sink.len(ranges.len());
    for range in ranges {
        sink.u32(range.lo);
        sink.u32(range.hi);
    }
}

/// One segment of a partition: the node ids it owns and the ranges it
/// allocates fresh ids from.
///
/// `nodes` is sorted ascending and pairwise disjoint; for tables it holds
/// row ids. `alloc_nodes` lies inside the union of `nodes`. `alloc_edges` is
/// unused for tables and must be `None` there.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SegmentSpec {
    /// The segment's id, its position in the partition.
    pub id: SegmentId,
    /// The node ids the segment owns.
    pub nodes: Vec<IdRange>,
    /// The range the segment allocates node ids from.
    pub alloc_nodes: Option<IdRange>,
    /// The range the segment allocates edge ids from.
    pub alloc_edges: Option<IdRange>,
}

impl SegmentSpec {
    /// Whether the segment owns node id `id`.
    #[must_use]
    pub fn owns(&self, id: u32) -> bool {
        let at = self.nodes.partition_point(|r| r.hi < id);
        self.nodes.get(at).is_some_and(|r| r.contains(id))
    }

    /// Write the spec's canonical bytes: id, node ranges, then the two
    /// optional allocation ranges.
    pub fn write<S: CanonicalSink + ?Sized>(&self, sink: &mut S) {
        sink.u32(self.id.0);
        write_ranges(sink, &self.nodes);
        write_opt_range(sink, self.alloc_nodes);
        write_opt_range(sink, self.alloc_edges);
    }

    /// Check the spec on its own, as the spec at `position` of a partition
    /// of `shape`.
    fn check(&self, position: SegmentId, shape: InstanceShapeKind) -> Result<(), SegmentError> {
        let malformed = |defect| SegmentError::MalformedPartition {
            segment: position,
            defect,
        };
        if self.id != position {
            return Err(malformed(PartitionDefect::NotDense { found: self.id }));
        }
        for range in &self.nodes {
            range.check(self.id)?;
        }
        for pair in self.nodes.windows(2) {
            let (prev, cur) = (pair[0], pair[1]);
            if cur.lo > prev.hi {
                continue;
            }
            if cur.hi < prev.lo {
                return Err(malformed(PartitionDefect::UnsortedRanges));
            }
            return Err(SegmentError::OverlappingRanges {
                a: self.id,
                b: self.id,
            });
        }
        if let Some(alloc) = self.alloc_nodes {
            alloc.check(self.id)?;
            if !covered(&self.nodes, alloc) {
                return Err(malformed(PartitionDefect::AllocationOutsideNodes));
            }
        }
        if let Some(alloc) = self.alloc_edges {
            if shape == InstanceShapeKind::Table {
                return Err(malformed(PartitionDefect::EdgeAllocationOnTable));
            }
            alloc.check(self.id)?;
        }
        Ok(())
    }
}

/// Whether the sorted, disjoint `ranges` cover every id of `target`.
fn covered(ranges: &[IdRange], target: IdRange) -> bool {
    let mut next = u64::from(target.lo);
    for range in ranges {
        if u64::from(range.hi) < next {
            continue;
        }
        if u64::from(range.lo) > next {
            return false;
        }
        next = u64::from(range.hi) + 1;
        if next > u64::from(target.hi) {
            return true;
        }
    }
    false
}

/// A partition of one instance into segments.
///
/// Segments are numbered densely from 0 in `specs` order.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Partition {
    /// The shape of the partitioned instance.
    pub shape: InstanceShapeKind,
    /// One spec per segment, in id order.
    pub specs: Vec<SegmentSpec>,
}

/// The owner of every node id under a partition, by binary search over all
/// node ranges.
///
/// Built once from a valid partition by [`Partition::owner_index`]; each
/// lookup is `O(log R)` for `R` ranges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnerIndex {
    ranges: Vec<(IdRange, SegmentId)>,
}

impl OwnerIndex {
    /// The segment owning node id `id`, or `None` when no range contains
    /// it.
    #[must_use]
    pub fn owner_of_node(&self, id: u32) -> Option<SegmentId> {
        let at = self.ranges.partition_point(|(r, _)| r.hi < id);
        self.ranges
            .get(at)
            .filter(|(r, _)| r.contains(id))
            .map(|(_, s)| *s)
    }
}

/// The first pair of ranges, among ranges sorted by `lo`, that overlap: the
/// two segments, the lesser first.
fn first_overlap(ranges: &[(IdRange, SegmentId)]) -> Option<SegmentError> {
    ranges.windows(2).find_map(|pair| {
        let ((prev, a), (cur, b)) = (pair[0], pair[1]);
        (cur.lo <= prev.hi).then(|| SegmentError::OverlappingRanges {
            a: a.min(b),
            b: a.max(b),
        })
    })
}

impl Partition {
    /// Check the partition: ids dense from 0 in `specs` order, every range
    /// well formed, each spec's node ranges ascending and disjoint, its node
    /// allocation range inside them, no edge allocation range on a table,
    /// and no node id or edge allocation id claimed by two segments.
    ///
    /// # Errors
    ///
    /// The first defect, spec by spec and then across specs:
    /// [`SegmentError::MalformedPartition`],
    /// [`SegmentError::InvertedRange`] or
    /// [`SegmentError::OverlappingRanges`].
    pub fn validate(&self) -> Result<(), SegmentError> {
        for (position, spec) in self.specs.iter().enumerate() {
            spec.check(position_id(position)?, self.shape)?;
        }
        let mut nodes: Vec<(IdRange, SegmentId)> = self
            .specs
            .iter()
            .flat_map(|s| s.nodes.iter().map(move |r| (*r, s.id)))
            .collect();
        nodes.sort_unstable();
        if let Some(error) = first_overlap(&nodes) {
            return Err(error);
        }
        let mut edges: Vec<(IdRange, SegmentId)> = self
            .specs
            .iter()
            .filter_map(|s| s.alloc_edges.map(|r| (r, s.id)))
            .collect();
        edges.sort_unstable();
        first_overlap(&edges).map_or(Ok(()), Err)
    }

    /// The owner index of a valid partition.
    ///
    /// # Errors
    ///
    /// The error [`Partition::validate`] reports.
    pub fn owner_index(&self) -> Result<OwnerIndex, SegmentError> {
        self.validate()?;
        let mut ranges: Vec<(IdRange, SegmentId)> = self
            .specs
            .iter()
            .flat_map(|s| s.nodes.iter().map(move |r| (*r, s.id)))
            .collect();
        ranges.sort_unstable();
        Ok(OwnerIndex { ranges })
    }

    /// The segment owning node id `id`, or `None` when no range contains
    /// it.
    ///
    /// Each spec's ranges are searched by binary search, so a lookup is
    /// `O(S log R)` for `S` segments; a caller looking up many ids builds an
    /// [`OwnerIndex`] once ([`Partition::owner_index`]), whose lookups are
    /// `O(log R)`.
    #[must_use]
    pub fn owner_of_node(&self, id: u32) -> Option<SegmentId> {
        self.specs.iter().find(|s| s.owns(id)).map(|s| s.id)
    }

    /// The spec of segment `id`.
    #[must_use]
    pub fn spec(&self, id: SegmentId) -> Option<&SegmentSpec> {
        self.specs
            .get(usize::try_from(id.0).unwrap_or(usize::MAX))
            .filter(|s| s.id == id)
    }

    /// A partition into contiguous node ranges split at `bounds`, with
    /// allocation ranges carved consecutively from `next_free`.
    ///
    /// The ids below `next_free` are split at each bound: segment 0 owns
    /// `[0, bounds[0] - 1]`, segment `i` owns `[bounds[i - 1], bounds[i] - 1]`
    /// and the last segment owns `[bounds[k - 1], next_free - 1]`. When
    /// `alloc_per_segment` is not zero, segment `i` also owns and allocates
    /// node ids from `[next_free + i * alloc_per_segment, next_free + (i + 1)
    /// * alloc_per_segment - 1]`, and, unless the shape is a table, edge ids
    /// from the same interval; `next_free` must then lie above every node id
    /// and every edge id of the instance.
    ///
    /// ```
    /// use panproto_inst::InstanceShapeKind;
    /// use panproto_inst::segment::{IdRange, Partition};
    ///
    /// let p = Partition::by_node_ranges(InstanceShapeKind::Graph, &[10], 5, 20)?;
    /// assert_eq!(p.specs[0].nodes, vec![IdRange { lo: 0, hi: 9 }, IdRange { lo: 20, hi: 24 }]);
    /// assert_eq!(p.specs[1].nodes, vec![IdRange { lo: 10, hi: 19 }, IdRange { lo: 25, hi: 29 }]);
    /// assert_eq!(p.specs[1].alloc_edges, Some(IdRange { lo: 25, hi: 29 }));
    /// # Ok::<(), panproto_inst::segment::SegmentError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// [`SegmentError::MalformedPartition`] with
    /// [`PartitionDefect::UnsortedRanges`] when the bounds are not strictly
    /// ascending inside `(0, next_free]`, and
    /// [`SegmentError::AllocationExhausted`] when the allocation ranges pass
    /// `u32::MAX`.
    pub fn by_node_ranges(
        shape: InstanceShapeKind,
        bounds: &[u32],
        alloc_per_segment: u32,
        next_free: u32,
    ) -> Result<Self, SegmentError> {
        let count = bounds.len() + 1;
        let mut specs = Vec::with_capacity(count);
        let mut lo = 0_u32;
        for i in 0..count {
            let id = position_id(i)?;
            let last = i + 1 == count;
            let end = bounds.get(i).copied().unwrap_or(next_free);
            let mut nodes = Vec::new();
            if end > lo {
                nodes.push(IdRange { lo, hi: end - 1 });
            } else if !last || end < lo {
                return Err(SegmentError::MalformedPartition {
                    segment: id,
                    defect: PartitionDefect::UnsortedRanges,
                });
            }
            lo = end;
            let alloc = if alloc_per_segment == 0 {
                None
            } else {
                let width = u64::from(alloc_per_segment);
                let start = u64::from(next_free) + u64::try_from(i).unwrap_or(u64::MAX) * width;
                let range = u32::try_from(start)
                    .ok()
                    .zip(u32::try_from(start + width - 1).ok())
                    .map(|(lo, hi)| IdRange { lo, hi })
                    .ok_or(SegmentError::AllocationExhausted {
                        segment: id,
                        kind: IdKind::Node,
                    })?;
                nodes.push(range);
                Some(range)
            };
            specs.push(SegmentSpec {
                id,
                nodes,
                alloc_nodes: alloc,
                alloc_edges: alloc.filter(|_| shape != InstanceShapeKind::Table),
            });
        }
        let partition = Self { shape, specs };
        partition.validate()?;
        Ok(partition)
    }

    /// The partition with one segment owning every id, allocating from above
    /// `max_id`, the largest node or edge id in use (`None` for an empty
    /// instance). Whole-state execution is the run over this partition.
    ///
    /// ```
    /// use panproto_inst::InstanceShapeKind;
    /// use panproto_inst::segment::{IdRange, Partition};
    ///
    /// let p = Partition::single(InstanceShapeKind::Table, Some(7));
    /// assert_eq!(p.specs[0].nodes, vec![IdRange::FULL]);
    /// assert_eq!(p.specs[0].alloc_nodes, Some(IdRange { lo: 8, hi: u32::MAX }));
    /// assert_eq!(p.specs[0].alloc_edges, None);
    /// assert!(p.validate().is_ok());
    /// ```
    #[must_use]
    pub fn single(shape: InstanceShapeKind, max_id: Option<u32>) -> Self {
        let lo = match max_id {
            None => Some(0),
            Some(max) => max.checked_add(1),
        };
        let alloc = lo.map(|lo| IdRange { lo, hi: u32::MAX });
        Self {
            shape,
            specs: vec![SegmentSpec {
                id: SegmentId(0),
                nodes: vec![IdRange::FULL],
                alloc_nodes: alloc,
                alloc_edges: alloc.filter(|_| shape != InstanceShapeKind::Table),
            }],
        }
    }

    /// Write the partition's canonical bytes into `sink`: the shape tag,
    /// then the spec count and each spec ([`SegmentSpec::write`]).
    pub fn write<S: CanonicalSink + ?Sized>(&self, sink: &mut S) {
        sink.tag(shape_tag(self.shape));
        sink.len(self.specs.len());
        for spec in &self.specs {
            spec.write(sink);
        }
    }

    /// The partition's canonical bytes, as [`Partition::write`] writes them.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut writer = CanonicalWriter::new();
        self.write(&mut writer);
        writer.into_bytes()
    }
}

/// The segment id of the spec at `position`.
fn position_id(position: usize) -> Result<SegmentId, SegmentError> {
    u32::try_from(position)
        .map(SegmentId)
        .map_err(|_| SegmentError::AllocationExhausted {
            segment: SegmentId(u32::MAX),
            kind: IdKind::Node,
        })
}
