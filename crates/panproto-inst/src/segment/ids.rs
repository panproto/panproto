//! Segment ids, inclusive id ranges, and fresh-id allocation restricted to a
//! range.
//!
//! # Fresh ids
//!
//! An [`IdAllocator`] is an [`IdRange`] `R` together with a floor. The floor
//! of `R` for a source is
//!
//! ```text
//! floor = max(R.lo, 1 + max{ ids in R present in the source, dropped objects included })
//! ```
//!
//! and `R.lo` when `R` holds no id of the source. A fresh id is
//!
//! ```text
//! fresh = max(floor, 1 + max(view ids ∪ reserved ids ∪ synthesized ids, each restricted to R))
//! ```
//!
//! which is the floor when that set has no id in `R`; the result must not
//! exceed `R.hi`. Nothing computes `floor - 1`, so an id below the floor is
//! never handed out again.
//!
//! No allocator hands out `u32::MAX`. A floor is a `u32` and saturates
//! there: one above `u32::MAX` cannot be stored, so a source holding
//! `u32::MAX` records the floor `u32::MAX`, and allocation from a range
//! ending at `u32::MAX` stops at `u32::MAX - 1`. Saturation loses nothing.
//! With `u32::MAX` withheld, the ids allocation may use at floor `f` are the
//! free ids of `[f, u32::MAX - 1]`; for the true floor `F = max + 1` and
//! `s = min(F, u32::MAX)`, either `F <= u32::MAX` and `s = F`, or
//! `F = 2^32` and both `[s, u32::MAX - 1]` and `[F, u32::MAX - 1]` are
//! empty. A range ending below `u32::MAX` hands out its own `hi`.
//!
//! Whole-state execution uses one allocator over [`IdRange::FULL`] per id
//! kind. Its floor is then the [`IdFloor`] component of the
//! source (the maximum id plus one, saturating at `u32::MAX`, and 0 for an
//! empty source), and its fresh ids are the ones the whole-state rule
//! gives. A segment allocates from the ranges its partition assigns it
//! instead.
//!
//! ```
//! use panproto_inst::segment::{IdAllocator, IdRange};
//!
//! // Whole-state: the source holds node ids 0, 3 and 7.
//! let mut nodes = IdAllocator::from_source(IdRange::FULL, [0, 3, 7]);
//! assert_eq!(nodes.floor(), 8);
//! // A view that already uses id 9 pushes the fresh id past it.
//! assert_eq!(nodes.fresh([2, 9]), Ok(10));
//! assert_eq!(nodes.allocate(), Ok(8));
//! assert_eq!(nodes.allocate(), Ok(9));
//!
//! // A segment that owns ids 100 to 101 runs out after two allocations.
//! let range = IdRange { lo: 100, hi: 101 };
//! let mut segment = IdAllocator::from_source(range, [3, 7]);
//! assert_eq!(segment.allocate(), Ok(100));
//! assert_eq!(segment.allocate(), Ok(101));
//! assert!(segment.allocate().is_err());
//!
//! // At the top of the id space, `u32::MAX` itself is withheld.
//! let mut top = IdAllocator::from_floor(IdRange::FULL, u32::MAX - 1);
//! assert_eq!(top.allocate(), Ok(u32::MAX - 1));
//! assert_eq!(top.floor(), u32::MAX);
//! assert!(top.allocate().is_err());
//! ```

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::error::SegmentError;
use crate::error::InstError;
use crate::ginstance::EdgeId;

/// Identifier of a segment of a partitioned instance.
///
/// The segments of one partition are numbered densely from 0 in the order
/// the partition lists them. A segment id has the same derives and the same
/// transparent serde form as [`EdgeId`](crate::EdgeId): a bare `u32`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SegmentId(pub u32);

impl fmt::Display for SegmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "segment {}", self.0)
    }
}

/// An inclusive range `[lo, hi]` of `u32` ids.
///
/// The range is inclusive at both ends, so the whole `u32` space is the one
/// range [`IdRange::FULL`]. A well-formed range has `lo <= hi`; the fields are
/// public, so a range read from storage is checked with [`IdRange::check`].
/// Ranges order by `(lo, hi)`.
///
/// ```
/// use panproto_inst::segment::{IdRange, SegmentError, SegmentId};
///
/// let range = IdRange::new(SegmentId(0), 10, 19)?;
/// assert!(range.contains(10) && range.contains(19) && !range.contains(20));
/// assert!(matches!(
///     IdRange::new(SegmentId(0), 5, 4),
///     Err(SegmentError::InvertedRange { .. })
/// ));
/// # Ok::<(), SegmentError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct IdRange {
    /// The least id in the range.
    pub lo: u32,
    /// The greatest id in the range.
    pub hi: u32,
}

impl IdRange {
    /// The whole `u32` id space, `[0, u32::MAX]`.
    pub const FULL: Self = Self {
        lo: 0,
        hi: u32::MAX,
    };

    /// The range `[lo, hi]` of `segment`.
    ///
    /// # Errors
    ///
    /// Returns [`SegmentError::InvertedRange`] when `lo > hi`.
    pub const fn new(segment: SegmentId, lo: u32, hi: u32) -> Result<Self, SegmentError> {
        Self { lo, hi }.check(segment)
    }

    /// This range, if it is well formed; `segment` names the segment it
    /// belongs to in the error.
    ///
    /// # Errors
    ///
    /// Returns [`SegmentError::InvertedRange`] when `lo > hi`.
    pub const fn check(self, segment: SegmentId) -> Result<Self, SegmentError> {
        if self.lo > self.hi {
            Err(SegmentError::InvertedRange {
                segment,
                range: self,
            })
        } else {
            Ok(self)
        }
    }

    /// Whether `id` lies in the range. An inverted range contains no id.
    #[must_use]
    pub const fn contains(self, id: u32) -> bool {
        self.lo <= id && id <= self.hi
    }
}

impl fmt::Display for IdRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}, {}]", self.lo, self.hi)
    }
}

/// The kind of id an allocation draws: node ids (tree nodes, graph nodes,
/// table rows) or edge ids (graph edges, and the edge ids of tree arcs and
/// table pairs in their graph form).
///
/// Serialized in `snake_case`: `"node"` or `"edge"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdKind {
    /// Node ids, which also number table rows.
    Node,
    /// Edge ids.
    Edge,
}

impl fmt::Display for IdKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Node => "node",
            Self::Edge => "edge",
        })
    }
}

/// A fresh-id allocator over one [`IdRange`]: the range and its floor.
///
/// The floor is the least id the allocator may hand out. It starts at the
/// range's `lo`, rises past every id [observed](IdAllocator::observe) inside
/// the range and past every id [allocated](IdAllocator::allocate), and never
/// falls, so an id below it is never handed out. Ids outside the range do not
/// move it. The floor is a `u32` that saturates at `u32::MAX`, and the
/// allocator never hands out `u32::MAX`: an allocator that has observed
/// `u32::MAX`, or handed out `u32::MAX - 1`, records the floor `u32::MAX`
/// and reports exhaustion on its next allocation rather than when the floor
/// is formed.
///
/// ```
/// use panproto_inst::segment::{IdAllocator, IdRange};
///
/// let mut edges = IdAllocator::new(IdRange { lo: 10, hi: 19 });
/// edges.observe(4); // outside the range: ignored
/// edges.observe(12);
/// assert_eq!(edges.floor(), 13);
/// assert_eq!(edges.allocate(), Ok(13));
/// assert_eq!(edges.floor(), 14);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IdAllocator {
    range: IdRange,
    /// The least id allocation may use, saturating at `u32::MAX`, which is
    /// never handed out.
    floor: u32,
}

impl IdAllocator {
    /// An allocator over `range` whose source holds no id in it: the floor is
    /// `range.lo`.
    #[must_use]
    pub const fn new(range: IdRange) -> Self {
        Self {
            range,
            floor: range.lo,
        }
    }

    /// An allocator over `range` for a source holding `source_ids`, dropped
    /// objects included: the floor is
    /// `max(range.lo, 1 + max{ source ids in range })`, and `range.lo` when
    /// none lies in the range.
    #[must_use]
    pub fn from_source(range: IdRange, source_ids: impl IntoIterator<Item = u32>) -> Self {
        let mut allocator = Self::new(range);
        for id in source_ids {
            allocator.observe(id);
        }
        allocator
    }

    /// An allocator over `range` resuming from a recorded floor (an
    /// [`IdFloor`](crate::IdFloor) component or a stored counter): the floor
    /// is `max(range.lo, floor)`.
    #[must_use]
    pub fn from_floor(range: IdRange, floor: u32) -> Self {
        Self {
            range,
            floor: range.lo.max(floor),
        }
    }

    /// The allocator of `segment` for ids of `kind`, from the allocation range
    /// its partition assigns it, with the floor at the range's `lo`.
    ///
    /// # Errors
    ///
    /// Returns [`SegmentError::NoAllocationRange`] when `range` is `None` and
    /// [`SegmentError::InvertedRange`] when it is inverted.
    pub fn for_segment(
        segment: SegmentId,
        kind: IdKind,
        range: Option<IdRange>,
    ) -> Result<Self, SegmentError> {
        let range = range.ok_or(SegmentError::NoAllocationRange { segment, kind })?;
        Ok(Self::new(range.check(segment)?))
    }

    /// The range this allocator draws from.
    #[must_use]
    pub const fn range(&self) -> IdRange {
        self.range
    }

    /// The floor: one above every id observed or allocated in the range,
    /// and at least the range's `lo`, saturating at `u32::MAX`. This is the
    /// value a recorded floor ([`IdFloor`](crate::IdFloor), a stored
    /// counter) holds.
    #[must_use]
    pub const fn floor(&self) -> u32 {
        self.floor
    }

    /// Whether no id of the range below `u32::MAX` is left at or above the
    /// floor.
    #[must_use]
    pub const fn is_exhausted(&self) -> bool {
        self.next().is_err()
    }

    /// Record an id in use (a source id, a view id, a reserved id or a
    /// synthesized id): when it lies in the range, the floor rises past it,
    /// saturating at `u32::MAX`.
    pub fn observe(&mut self, id: u32) {
        if self.range.contains(id) {
            self.floor = self.floor.max(id.saturating_add(1));
        }
    }

    /// The fresh id for a call that must also avoid `taken` (view ids,
    /// reserved ids and synthesized ids), without changing the allocator:
    /// `max(floor, 1 + max(taken ∩ range))`, the floor when no taken id lies
    /// in the range.
    ///
    /// # Errors
    ///
    /// Returns [`RangeExhausted`] when that id exceeds the range's `hi` or
    /// is `u32::MAX`.
    pub fn fresh(&self, taken: impl IntoIterator<Item = u32>) -> Result<u32, RangeExhausted> {
        let mut probe = *self;
        for id in taken {
            probe.observe(id);
        }
        probe.next()
    }

    /// Hand out the floor and raise the floor past it.
    ///
    /// # Errors
    ///
    /// Returns [`RangeExhausted`] when the floor exceeds the range's `hi`
    /// or is `u32::MAX`; the allocator is then unchanged.
    pub fn allocate(&mut self) -> Result<u32, RangeExhausted> {
        let id = self.next()?;
        self.floor = id.saturating_add(1);
        Ok(id)
    }

    /// The floor, when it is an id allocation may hand out: at most the
    /// range's `hi`, and never `u32::MAX`.
    const fn next(&self) -> Result<u32, RangeExhausted> {
        if self.floor <= self.range.hi && self.floor != u32::MAX {
            Ok(self.floor)
        } else {
            Err(RangeExhausted { range: self.range })
        }
    }
}

/// No id of an allocator's range below `u32::MAX` is left at or above its
/// floor.
///
/// The allocator does not know which kind of id it draws or which segment it
/// serves; the caller converts the failure with
/// [`RangeExhausted::whole_state`] or [`RangeExhausted::in_segment`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
#[error("no id below u32::MAX is free at or above the floor in {range}")]
pub struct RangeExhausted {
    /// The exhausted range.
    pub range: IdRange,
}

impl RangeExhausted {
    /// The whole-state error for ids of `kind`:
    /// [`InstError::NodeIdsExhausted`] for nodes (and rows) and
    /// [`InstError::EdgeIdsExhausted`] for edges.
    #[must_use]
    pub const fn whole_state(self, kind: IdKind) -> InstError {
        match kind {
            IdKind::Node => InstError::NodeIdsExhausted,
            IdKind::Edge => InstError::EdgeIdsExhausted,
        }
    }

    /// The segmented-run error for ids of `kind` in `segment`:
    /// [`SegmentError::AllocationExhausted`].
    #[must_use]
    pub const fn in_segment(self, segment: SegmentId, kind: IdKind) -> SegmentError {
        SegmentError::AllocationExhausted { segment, kind }
    }
}

/// The seal floor rule of a segment family's edge ids.
///
/// Edge ids are not owned by range: an edge is held by the segment owning
/// its source, whatever its id. The *floor* is the least lower bound of the
/// family's edge allocation ranges. An edge id below the floor may be held
/// by any segment; an edge id at or above the floor must lie in the edge
/// allocation range of the segment holding it. Edge allocation ranges are
/// pairwise disjoint ([`Partition::validate`]), so under the rule an id
/// inside the range of segment `i` is held only by segment `i`, and every
/// id outside every range lies below every range.
///
/// The rule is what allocation needs. A segment allocates from its own range
/// at or above the append floor of its own ids in that range (the source
/// floor of [`IdAllocator`]); no other segment holds an id there, so a fresh
/// id collides with no held id, and it lies above every edge its segment
/// holds, so fresh edges follow their existing siblings (which share their
/// source and hence their segment) in ascending edge-id order. A family with
/// no edge id at or above the floor satisfies the rule, and appending a
/// fresh id to a family that satisfies it gives a family that satisfies it.
///
/// [`Partition::validate`]: super::Partition::validate
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EdgeFloor {
    floor: Option<u32>,
    ranges: BTreeMap<SegmentId, IdRange>,
}

impl EdgeFloor {
    /// The rule for a family whose segments declare the given edge
    /// allocation ranges, `None` for a segment without one.
    #[must_use]
    pub fn new(ranges: impl IntoIterator<Item = (SegmentId, Option<IdRange>)>) -> Self {
        let ranges: BTreeMap<SegmentId, IdRange> = ranges
            .into_iter()
            .filter_map(|(segment, range)| range.map(|range| (segment, range)))
            .collect();
        let floor = ranges.values().map(|range| range.lo).min();
        Self { floor, ranges }
    }

    /// The least lower bound of the edge allocation ranges, `None` when no
    /// segment declares one (every edge id then lies below the floor).
    #[must_use]
    pub const fn floor(&self) -> Option<u32> {
        self.floor
    }

    /// Check that `segment` may hold edge id `edge`.
    ///
    /// # Errors
    ///
    /// For an id at or above the floor, [`SegmentError::NoAllocationRange`]
    /// with [`IdKind::Edge`] when `segment` declares no edge allocation
    /// range, and [`SegmentError::EdgeIdOutsideAllocation`] when its range
    /// does not contain `edge`.
    pub fn check(&self, segment: SegmentId, edge: u32) -> Result<(), SegmentError> {
        if self.floor.is_none_or(|floor| edge < floor) {
            return Ok(());
        }
        match self.ranges.get(&segment) {
            None => Err(SegmentError::NoAllocationRange {
                segment,
                kind: IdKind::Edge,
            }),
            Some(range) if range.contains(edge) => Ok(()),
            Some(_) => Err(SegmentError::EdgeIdOutsideAllocation {
                edge: EdgeId(edge),
                segment,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use panproto_schema::Edge;
    use proptest::prelude::*;

    use super::{EdgeFloor, IdAllocator, IdKind, IdRange, RangeExhausted, SegmentId};
    use crate::error::InstError;
    use crate::ginstance::{EdgeId, GInstance};
    use crate::metadata::Node;
    use crate::segment::SegmentError;
    use crate::view_ref::IdFloor;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// The whole-state floor of a set of ids: the maximum plus one,
    /// saturating at `u32::MAX`, and 0 for no id.
    fn whole_state_floor(ids: &[u32]) -> u32 {
        ids.iter().max().map_or(0, |max| max.saturating_add(1))
    }

    /// The whole-state fresh id: the floor, raised past every taken id, and
    /// none when that is `u32::MAX`, which no allocation hands out.
    fn whole_state_fresh(source: &[u32], taken: &[u32]) -> Option<u32> {
        let floor = whole_state_floor(source);
        let fresh = taken
            .iter()
            .max()
            .map_or(floor, |max| floor.max(max.saturating_add(1)));
        (fresh != u32::MAX).then_some(fresh)
    }

    /// A range and ids spread around it, some inside and some outside.
    fn range_and_ids() -> impl Strategy<Value = (IdRange, Vec<u32>)> {
        (any::<u32>(), 0_u32..64).prop_flat_map(|(lo, width)| {
            let hi = lo.saturating_add(width);
            let lo_probe = lo.saturating_sub(8);
            let hi_probe = hi.saturating_add(8);
            (
                Just(IdRange { lo, hi }),
                prop::collection::vec(lo_probe..=hi_probe, 0..12),
            )
        })
    }

    #[test]
    fn segment_id_serializes_as_a_bare_u32() -> TestResult {
        assert_eq!(serde_json::to_string(&SegmentId(7))?, "7");
        assert_eq!(serde_json::from_str::<SegmentId>("7")?, SegmentId(7));
        let packed = rmp_serde::to_vec(&SegmentId(7))?;
        assert_eq!(rmp_serde::from_slice::<u32>(&packed)?, 7);
        let mut cbor = Vec::new();
        ciborium::into_writer(&SegmentId(7), &mut cbor)?;
        assert_eq!(ciborium::from_reader::<u32, _>(cbor.as_slice())?, 7);
        assert!(SegmentId(1) < SegmentId(2));
        assert_eq!(SegmentId(3).to_string(), "segment 3");
        Ok(())
    }

    #[test]
    fn an_inverted_range_is_rejected() -> TestResult {
        let segment = SegmentId(4);
        assert_eq!(
            IdRange::new(segment, 9, 3),
            Err(SegmentError::InvertedRange {
                segment,
                range: IdRange { lo: 9, hi: 3 },
            })
        );
        assert_eq!(
            IdRange { lo: 9, hi: 3 }.check(segment),
            Err(SegmentError::InvertedRange {
                segment,
                range: IdRange { lo: 9, hi: 3 },
            })
        );
        assert!(!IdRange { lo: 9, hi: 3 }.contains(5));
        let single = IdRange::new(segment, 5, 5)?;
        assert!(single.contains(5) && !single.contains(4) && !single.contains(6));
        assert!(IdRange::FULL.contains(0) && IdRange::FULL.contains(u32::MAX));
        // Field order and serde form are exact.
        assert_eq!(serde_json::to_string(&single)?, r#"{"lo":5,"hi":5}"#);
        // Ranges order by (lo, hi).
        let mut ranges = vec![
            IdRange { lo: 2, hi: 3 },
            IdRange { lo: 1, hi: 9 },
            IdRange { lo: 2, hi: 2 },
        ];
        ranges.sort();
        assert_eq!(
            ranges,
            vec![
                IdRange { lo: 1, hi: 9 },
                IdRange { lo: 2, hi: 2 },
                IdRange { lo: 2, hi: 3 },
            ]
        );
        assert_eq!(IdRange { lo: 1, hi: 9 }.to_string(), "[1, 9]");
        Ok(())
    }

    #[test]
    fn allocation_past_the_range_end_is_exhausted() -> TestResult {
        let segment = SegmentId(2);
        let range = IdRange::new(segment, 40, 42)?;
        let mut alloc = IdAllocator::for_segment(segment, IdKind::Edge, Some(range))?;
        alloc.observe(41);
        assert_eq!(alloc.allocate(), Ok(42));
        assert!(alloc.is_exhausted());
        let failure = alloc.allocate();
        assert_eq!(failure, Err(RangeExhausted { range }));
        // A failed allocation leaves the allocator unchanged.
        assert_eq!(alloc.floor(), 43);
        assert_eq!(alloc.fresh([]), Err(RangeExhausted { range }));
        assert_eq!(
            RangeExhausted { range }.in_segment(segment, IdKind::Edge),
            SegmentError::AllocationExhausted {
                segment,
                kind: IdKind::Edge,
            }
        );
        // A segment without an allocation range, or with an inverted one.
        assert_eq!(
            IdAllocator::for_segment(segment, IdKind::Node, None),
            Err(SegmentError::NoAllocationRange {
                segment,
                kind: IdKind::Node,
            })
        );
        assert_eq!(
            IdAllocator::for_segment(segment, IdKind::Node, Some(IdRange { lo: 3, hi: 1 })),
            Err(SegmentError::InvertedRange {
                segment,
                range: IdRange { lo: 3, hi: 1 },
            })
        );
        Ok(())
    }

    #[test]
    fn an_edge_id_at_or_above_the_floor_is_held_only_in_its_holders_range() {
        let (zero, one, two) = (SegmentId(0), SegmentId(1), SegmentId(2));
        let rule = EdgeFloor::new([
            (zero, Some(IdRange { lo: 20, hi: 29 })),
            (one, Some(IdRange { lo: 10, hi: 19 })),
            (two, None),
        ]);
        assert_eq!(rule.floor(), Some(10));
        // Below the floor any segment may hold an id.
        for segment in [zero, one, two] {
            assert_eq!(rule.check(segment, 9), Ok(()));
        }
        // At or above it, only the segment whose range holds the id.
        assert_eq!(rule.check(one, 10), Ok(()));
        assert_eq!(rule.check(zero, 29), Ok(()));
        assert_eq!(
            rule.check(zero, 10),
            Err(SegmentError::EdgeIdOutsideAllocation {
                edge: EdgeId(10),
                segment: zero,
            })
        );
        // An id above the floor in no range is admitted for no segment.
        assert_eq!(
            rule.check(one, 30),
            Err(SegmentError::EdgeIdOutsideAllocation {
                edge: EdgeId(30),
                segment: one,
            })
        );
        assert_eq!(
            rule.check(two, 10),
            Err(SegmentError::NoAllocationRange {
                segment: two,
                kind: IdKind::Edge,
            })
        );
        // Without any edge range the rule admits every id.
        let none = EdgeFloor::new([(zero, None), (one, None)]);
        assert_eq!(none.floor(), None);
        assert_eq!(none.check(zero, u32::MAX), Ok(()));
    }

    proptest! {
        /// Allocation keeps the rule: a fresh id that a segment draws from
        /// its own range above its own ids there differs from every id the
        /// family holds, lies above every id its segment holds, and is
        /// admitted by the rule.
        #[test]
        fn a_fresh_edge_id_keeps_the_seal_floor_rule(
            held in prop::collection::vec((0_u32..3, 0_u32..40), 0..16),
            drawer in 0_u32..3,
        ) {
            let ranges = [
                (SegmentId(0), Some(IdRange { lo: 10, hi: 19 })),
                (SegmentId(1), Some(IdRange { lo: 20, hi: 29 })),
                (SegmentId(2), Some(IdRange { lo: 30, hi: 39 })),
            ];
            let rule = EdgeFloor::new(ranges);
            let mut family: Vec<(u32, u32)> = Vec::new();
            for (segment, edge) in held {
                if rule.check(SegmentId(segment), edge).is_ok()
                    && family.iter().all(|(_, other)| *other != edge)
                {
                    family.push((segment, edge));
                }
            }
            let range = IdRange { lo: 10 + 10 * drawer, hi: 19 + 10 * drawer };
            let own = family
                .iter()
                .filter(|(holder, _)| *holder == drawer)
                .map(|(_, edge)| *edge);
            if let Ok(fresh) = IdAllocator::from_source(range, own).allocate() {
                prop_assert!(family.iter().all(|(_, edge)| *edge != fresh));
                prop_assert!(
                    family
                        .iter()
                        .filter(|(holder, _)| *holder == drawer)
                        .all(|(_, edge)| *edge < fresh)
                );
                prop_assert_eq!(rule.check(SegmentId(drawer), fresh), Ok(()));
            }
        }
    }

    #[test]
    fn a_segment_allocator_reports_allocation_exhausted() -> TestResult {
        let segment = SegmentId(0);
        let kind = IdKind::Node;
        let mut alloc = IdAllocator::for_segment(segment, kind, Some(IdRange { lo: 7, hi: 8 }))?;
        let mut drawn = Vec::new();
        let error = loop {
            match alloc.allocate() {
                Ok(id) => drawn.push(id),
                Err(e) => break e.in_segment(segment, kind),
            }
        };
        assert_eq!(drawn, vec![7, 8]);
        assert_eq!(error, SegmentError::AllocationExhausted { segment, kind });
        Ok(())
    }

    #[test]
    fn a_full_range_allocator_reports_exhaustion_per_kind() {
        // The source already holds u32::MAX: it records the floor u32::MAX,
        // and no fresh id exists.
        let mut nodes = IdAllocator::from_source(IdRange::FULL, [0, u32::MAX]);
        assert_eq!(nodes.floor(), u32::MAX);
        assert!(nodes.is_exhausted());
        let exhausted = nodes.allocate();
        assert_eq!(
            exhausted,
            Err(RangeExhausted {
                range: IdRange::FULL
            })
        );
        assert!(matches!(
            RangeExhausted {
                range: IdRange::FULL
            }
            .whole_state(IdKind::Node),
            InstError::NodeIdsExhausted
        ));
        assert!(matches!(
            RangeExhausted {
                range: IdRange::FULL
            }
            .whole_state(IdKind::Edge),
            InstError::EdgeIdsExhausted
        ));
        // u32::MAX is never handed out: the floor u32::MAX allocates
        // nothing, and the floor below it hands out u32::MAX - 1 and then
        // fails, with the floor at u32::MAX.
        let mut edges = IdAllocator::from_floor(IdRange::FULL, u32::MAX);
        assert!(edges.allocate().is_err());
        let mut edges = IdAllocator::from_floor(IdRange::FULL, u32::MAX - 1);
        assert_eq!(edges.allocate(), Ok(u32::MAX - 1));
        assert_eq!(edges.floor(), u32::MAX);
        assert!(edges.allocate().is_err());
        // A view id at u32::MAX - 1 or u32::MAX exhausts a fresh id without
        // touching the floor.
        let fresh = IdAllocator::new(IdRange::FULL);
        assert!(fresh.fresh([u32::MAX]).is_err());
        assert!(fresh.fresh([u32::MAX - 1]).is_err());
        assert_eq!(fresh.floor(), 0);
    }

    fn knows() -> Edge {
        Edge {
            src: "person".into(),
            tgt: "person".into(),
            kind: "knows".into(),
            name: None,
        }
    }

    /// A graph with one node whose self-loops carry `edge_ids`.
    fn loops(edge_ids: &BTreeSet<u32>) -> GInstance {
        edge_ids.iter().fold(
            GInstance::new().with_node(Node::new(0, "person")),
            |g, &id| g.with_edge_id(EdgeId(id), 0, 0, knows()),
        )
    }

    proptest! {
        #[test]
        fn an_allocator_floor_follows_the_range_rule((range, ids) in range_and_ids()) {
            let alloc = IdAllocator::from_source(range, ids.iter().copied());
            let inside = ids.iter().copied().filter(|&id| range.contains(id)).max();
            let expected = inside.map_or(range.lo, |max| max.saturating_add(1));
            prop_assert_eq!(alloc.floor(), expected);
            prop_assert_eq!(alloc.range(), range);
        }

        #[test]
        fn fresh_ids_never_reuse_an_id_below_the_floor(
            (range, ids) in range_and_ids(),
            taken in prop::collection::vec(any::<u32>(), 0..6),
            draws in 0_usize..80,
        ) {
            let mut alloc = IdAllocator::from_source(range, ids.iter().copied());
            let floor = alloc.floor();
            let mut seen = BTreeSet::new();
            let mut last_error = None;
            for id in taken.iter().copied() {
                alloc.observe(id);
            }
            for _ in 0..draws {
                match alloc.allocate() {
                    Ok(id) => {
                        prop_assert!(range.contains(id));
                        prop_assert!(id != u32::MAX);
                        prop_assert!(id >= floor);
                        prop_assert!(!ids.contains(&id) && !taken.contains(&id));
                        prop_assert!(seen.insert(id), "id {} handed out twice", id);
                    }
                    Err(e) => {
                        prop_assert_eq!(e, RangeExhausted { range });
                        last_error = Some(e);
                    }
                }
            }
            // Exhaustion happens exactly when the range's end has been passed.
            if last_error.is_some() {
                prop_assert!(alloc.is_exhausted());
            }
        }

        #[test]
        fn a_fresh_id_at_the_range_end_is_the_last(lo in any::<u32>(), width in 0_u32..16) {
            let hi = lo.saturating_add(width);
            let range = IdRange { lo, hi };
            let alloc = IdAllocator::from_source(range, [hi.saturating_sub(1)]);
            // Taking hi - 1 leaves hi as the one fresh id (when hi > lo),
            // unless hi is u32::MAX, which is never handed out.
            if hi > lo && hi < u32::MAX {
                prop_assert_eq!(alloc.fresh([]), Ok(hi));
            } else if hi > lo {
                prop_assert_eq!(alloc.fresh([]), Err(RangeExhausted { range }));
            }
            prop_assert_eq!(alloc.fresh([hi]), Err(RangeExhausted { range }));
        }

        #[test]
        fn a_full_range_allocator_reproduces_the_id_floor(
            nodes in prop::collection::vec(any::<u32>(), 0..10),
            edges in prop::collection::vec(any::<u32>(), 0..10),
        ) {
            let node_alloc = IdAllocator::from_source(IdRange::FULL, nodes.iter().copied());
            let edge_alloc = IdAllocator::from_source(IdRange::FULL, edges.iter().copied());
            prop_assert_eq!(node_alloc.floor(), whole_state_floor(&nodes));
            prop_assert_eq!(edge_alloc.floor(), whole_state_floor(&edges));
            let floor = IdFloor {
                node: node_alloc.floor(),
                edge: edge_alloc.floor(),
            };
            // Resuming from the recorded floor gives the same allocator.
            prop_assert_eq!(IdAllocator::from_floor(IdRange::FULL, floor.node), node_alloc);
            prop_assert_eq!(IdAllocator::from_floor(IdRange::FULL, floor.edge), edge_alloc);
        }

        #[test]
        fn a_full_range_allocator_reproduces_the_whole_state_fresh_id_rule(
            source in prop::collection::vec(any::<u32>(), 0..10),
            taken in prop::collection::vec(any::<u32>(), 0..10),
            near_max in prop::collection::vec((u32::MAX - 3)..=u32::MAX, 0..3),
        ) {
            let mut taken = taken;
            taken.extend(near_max);
            let alloc = IdAllocator::from_source(IdRange::FULL, source.iter().copied());
            prop_assert_eq!(
                alloc.fresh(taken.iter().copied()).ok(),
                whole_state_fresh(&source, &taken)
            );
        }

        #[test]
        fn a_full_range_allocator_agrees_with_add_edge(
            ids in prop::collection::btree_set(
                prop_oneof![0_u32..64, (u32::MAX - 2)..=u32::MAX],
                0..8,
            ),
        ) {
            let mut graph = loops(&ids);
            let mut alloc = IdAllocator::from_source(IdRange::FULL, ids.iter().copied());
            match (graph.add_edge(0, 0, knows()), alloc.allocate()) {
                (Ok(EdgeId(from_graph)), Ok(from_alloc)) => prop_assert_eq!(from_graph, from_alloc),
                (Err(InstError::EdgeIdsExhausted), Err(e)) => {
                    prop_assert!(matches!(
                        e.whole_state(IdKind::Edge),
                        InstError::EdgeIdsExhausted
                    ));
                }
                (graph_result, alloc_result) => {
                    prop_assert!(
                        false,
                        "add_edge gave {:?}, the allocator {:?}",
                        graph_result,
                        alloc_result
                    );
                }
            }
        }

        #[test]
        fn segment_errors_round_trip_through_serde(
            segment in any::<u32>(),
            lo in any::<u32>(),
            hi in any::<u32>(),
        ) {
            let segment = SegmentId(segment);
            let errors = [
                SegmentError::InvertedRange { segment, range: IdRange { lo, hi } },
                SegmentError::NoAllocationRange { segment, kind: IdKind::Node },
                SegmentError::AllocationExhausted { segment, kind: IdKind::Edge },
                SegmentError::AllocationBelowFloor { segment, kind: IdKind::Edge },
            ];
            let fail = |e: &dyn std::fmt::Display| TestCaseError::fail(e.to_string());
            for error in errors {
                let json = serde_json::to_string(&error).map_err(|e| fail(&e))?;
                let from_json = serde_json::from_str::<SegmentError>(&json).map_err(|e| fail(&e))?;
                prop_assert_eq!(&from_json, &error);
                let packed = rmp_serde::to_vec_named(&error).map_err(|e| fail(&e))?;
                let from_packed =
                    rmp_serde::from_slice::<SegmentError>(&packed).map_err(|e| fail(&e))?;
                prop_assert_eq!(&from_packed, &error);
            }
        }
    }

    #[test]
    fn segment_error_serde_tags_are_snake_case() -> TestResult {
        let error = SegmentError::AllocationExhausted {
            segment: SegmentId(2),
            kind: IdKind::Edge,
        };
        assert_eq!(
            serde_json::to_string(&error)?,
            r#"{"allocation_exhausted":{"segment":2,"kind":"edge"}}"#
        );
        assert_eq!(serde_json::to_string(&IdKind::Node)?, r#""node""#);
        Ok(())
    }
}
