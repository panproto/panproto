//! Segmented execution as whole-state execution on the glued state.
//!
//! A partition assigns node ids to segments by inclusive ranges; a node is
//! owned by the segment whose range holds its id, and an edge by the owner
//! of its source. A segment holds its owned nodes and edges, a ghost (the
//! owner and anchor) for each remote target of an owned edge, and an in-stub
//! for each edge owned elsewhere whose target it owns. Gluing checks the
//! segment contract and takes the union; every segmented operation is
//! defined as the whole-state operation on the glued instance.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{GEdge, Graph, Name, Node, SEdge};

/// An inclusive id range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IdRange {
    /// Least id.
    pub lo: u32,
    /// Greatest id.
    pub hi: u32,
}

impl IdRange {
    /// Whether the range holds `id`.
    #[must_use]
    pub const fn contains(self, id: u32) -> bool {
        self.lo <= id && id <= self.hi
    }
}

/// One segment's ranges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentSpec {
    /// The segment id (its index).
    pub id: u32,
    /// Owned node ranges, ascending and disjoint.
    pub nodes: Vec<IdRange>,
    /// Node allocation range.
    pub alloc_nodes: Option<IdRange>,
    /// Edge allocation range.
    pub alloc_edges: Option<IdRange>,
}

/// A partition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Partition {
    /// The segments, numbered densely.
    pub specs: Vec<SegmentSpec>,
}

/// A violation of the segment contract.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SegmentError {
    /// A range with `lo > hi`.
    InvertedRange {
        /// The segment.
        segment: u32,
    },
    /// Two segments' ranges overlap.
    OverlappingRanges {
        /// The first.
        a: u32,
        /// The second.
        b: u32,
    },
    /// Segment ids are not `0..S` in order.
    NonDense,
    /// An element no range owns.
    Unowned {
        /// The node id.
        node: u32,
    },
    /// An element held by the wrong segment.
    WrongOwner {
        /// The node or edge source.
        node: u32,
        /// The expected owner.
        expected: u32,
        /// The holder.
        found: u32,
    },
    /// A node map key differs from its node's id.
    NodeIdMismatch {
        /// The key.
        key: u32,
    },
    /// A ghost disagrees with its owner or is missing or extra.
    GhostMismatch {
        /// The ghost node.
        node: u32,
    },
    /// An in-stub disagrees with its edge or is missing or extra.
    InStubMismatch {
        /// The edge.
        edge: u32,
    },
    /// An edge id held by two segments.
    DuplicateEdgeId {
        /// The edge.
        edge: u32,
    },
}

impl Partition {
    /// Check dense ids, ordered ranges and disjointness across segments.
    ///
    /// # Errors
    ///
    /// The first violation.
    pub fn validate(&self) -> Result<(), SegmentError> {
        for (i, s) in self.specs.iter().enumerate() {
            if usize::try_from(s.id).ok() != Some(i) {
                return Err(SegmentError::NonDense);
            }
            if s.nodes
                .iter()
                .chain(s.alloc_nodes.iter())
                .chain(s.alloc_edges.iter())
                .any(|r| r.lo > r.hi)
            {
                return Err(SegmentError::InvertedRange { segment: s.id });
            }
        }
        for a in &self.specs {
            for b in self.specs.iter().filter(|b| b.id > a.id) {
                let overlap = |x: &IdRange, y: &IdRange| x.lo <= y.hi && y.lo <= x.hi;
                let nodes = a
                    .nodes
                    .iter()
                    .any(|x| b.nodes.iter().any(|y| overlap(x, y)));
                let edges =
                    matches!((a.alloc_edges, b.alloc_edges), (Some(x), Some(y)) if overlap(&x, &y));
                if nodes || edges {
                    return Err(SegmentError::OverlappingRanges { a: a.id, b: b.id });
                }
            }
        }
        Ok(())
    }

    /// The owner of a node id.
    #[must_use]
    pub fn owner(&self, id: u32) -> Option<u32> {
        self.specs
            .iter()
            .find(|s| s.nodes.iter().any(|r| r.contains(id)))
            .map(|s| s.id)
    }
}

/// A remote target of an owned edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ghost {
    /// Its owner.
    pub owner: u32,
    /// Its anchor.
    pub anchor: Name,
}

/// An edge owned elsewhere whose target is owned here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InStub {
    /// Its source.
    pub src: u32,
    /// The source's owner.
    pub src_owner: u32,
    /// The source's anchor.
    pub src_anchor: Name,
    /// Its target.
    pub tgt: u32,
    /// Its label.
    pub label: SEdge,
}

/// A graph segment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphSegment {
    /// Its id.
    pub segment: u32,
    /// Owned nodes.
    pub nodes: BTreeMap<u32, Node>,
    /// Owned edges.
    pub edges: BTreeMap<u32, GEdge>,
    /// Ghosts by node.
    pub ghosts: BTreeMap<u32, Ghost>,
    /// In-stubs by edge.
    pub in_stubs: BTreeMap<u32, InStub>,
}

/// Split a graph by a valid partition.
///
/// # Errors
///
/// [`SegmentError::Unowned`] for the least node no range owns.
pub fn partition(g: &Graph, p: &Partition) -> Result<Vec<GraphSegment>, SegmentError> {
    p.validate()?;
    let owner = |n: u32| p.owner(n).ok_or(SegmentError::Unowned { node: n });
    let mut segs: Vec<GraphSegment> = p
        .specs
        .iter()
        .map(|s| GraphSegment {
            segment: s.id,
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            ghosts: BTreeMap::new(),
            in_stubs: BTreeMap::new(),
        })
        .collect();
    for (id, n) in &g.nodes {
        segs[usize::try_from(owner(*id)?).unwrap_or(0)]
            .nodes
            .insert(*id, n.clone());
    }
    for (id, e) in &g.edges {
        let (so, to) = (owner(e.src)?, owner(e.tgt)?);
        let s = &mut segs[usize::try_from(so).unwrap_or(0)];
        s.edges.insert(*id, e.clone());
        if so != to {
            s.ghosts.insert(
                e.tgt,
                Ghost {
                    owner: to,
                    anchor: g.nodes[&e.tgt].anchor.clone(),
                },
            );
            let t = &mut segs[usize::try_from(to).unwrap_or(0)];
            t.in_stubs.insert(
                *id,
                InStub {
                    src: e.src,
                    src_owner: so,
                    src_anchor: g.nodes[&e.src].anchor.clone(),
                    tgt: e.tgt,
                    label: e.label.clone(),
                },
            );
        }
    }
    Ok(segs)
}

/// Glue segments into the whole instance, checking the segment contract:
///
/// every node is owned by its holder and keyed by its id, every edge's
/// source is owned by its holder, every remote target is a ghost naming
/// its true owner and anchor, ghosts are exactly the remote targets, every
/// in-stub matches the edge its source owner holds, in-stubs are exactly
/// the incoming cross edges, and no edge id is held twice.
///
/// # Errors
///
/// The first violation, in segment then element order.
pub fn glue(segs: &[GraphSegment], p: &Partition) -> Result<Graph, SegmentError> {
    p.validate()?;
    let mut g = Graph::default();
    for s in segs {
        for (k, n) in &s.nodes {
            if *k != n.id {
                return Err(SegmentError::NodeIdMismatch { key: *k });
            }
            let o = p.owner(*k).ok_or(SegmentError::Unowned { node: *k })?;
            if o != s.segment {
                return Err(SegmentError::WrongOwner {
                    node: *k,
                    expected: o,
                    found: s.segment,
                });
            }
            g.nodes.insert(*k, n.clone());
        }
    }
    let mut cross: BTreeSet<u32> = BTreeSet::new();
    for s in segs {
        for (id, e) in &s.edges {
            if !s.nodes.contains_key(&e.src) {
                let expected = p
                    .owner(e.src)
                    .ok_or(SegmentError::Unowned { node: e.src })?;
                return Err(SegmentError::WrongOwner {
                    node: e.src,
                    expected,
                    found: s.segment,
                });
            }
            if g.edges.insert(*id, e.clone()).is_some() {
                return Err(SegmentError::DuplicateEdgeId { edge: *id });
            }
            if !s.nodes.contains_key(&e.tgt) {
                cross.insert(*id);
                let ok = s.ghosts.get(&e.tgt).is_some_and(|gh| {
                    Some(gh.owner) == p.owner(e.tgt)
                        && g.nodes.get(&e.tgt).is_some_and(|n| n.anchor == gh.anchor)
                });
                if !ok {
                    return Err(SegmentError::GhostMismatch { node: e.tgt });
                }
            }
        }
        let remote: BTreeSet<u32> = s
            .edges
            .values()
            .map(|e| e.tgt)
            .filter(|t| !s.nodes.contains_key(t))
            .collect();
        if let Some(extra) = s.ghosts.keys().find(|k| !remote.contains(*k)) {
            return Err(SegmentError::GhostMismatch { node: *extra });
        }
    }
    for s in segs {
        for (id, st) in &s.in_stubs {
            let ok = g
                .edges
                .get(id)
                .is_some_and(|e| e.src == st.src && e.tgt == st.tgt && e.label == st.label)
                && s.nodes.contains_key(&st.tgt)
                && Some(st.src_owner) == p.owner(st.src)
                && st.src_owner != s.segment
                && g.nodes
                    .get(&st.src)
                    .is_some_and(|n| n.anchor == st.src_anchor);
            if !ok {
                return Err(SegmentError::InStubMismatch { edge: *id });
            }
        }
        for id in cross
            .iter()
            .filter(|id| s.nodes.contains_key(&g.edges[id].tgt))
        {
            if !s.in_stubs.contains_key(id) {
                return Err(SegmentError::InStubMismatch { edge: *id });
            }
        }
    }
    Ok(g)
}

/// A segmented operation: the whole-state operation on the glued state.
///
/// # Errors
///
/// The gluing error.
pub fn segmented<T>(
    segs: &[GraphSegment],
    p: &Partition,
    op: &dyn Fn(&Graph) -> T,
) -> Result<T, SegmentError> {
    glue(segs, p).map(|g| op(&g))
}

/// The fresh id of an allocation range.
///
/// The range's floor (its least id, or one above the greatest source id it holds) raised above
/// every used id it holds; `None` when that passes the range's top or is `u32::MAX`.
#[must_use]
pub fn fresh(range: IdRange, source: &BTreeSet<u32>, used: &BTreeSet<u32>) -> Option<u32> {
    let inside = |s: &BTreeSet<u32>| s.iter().copied().filter(|i| range.contains(*i)).max();
    let floor = inside(source).map_or(Some(range.lo), |m| {
        m.checked_add(1).map(|f| f.max(range.lo))
    })?;
    let f = inside(used).map_or(Some(floor), |m| m.checked_add(1).map(|x| x.max(floor)))?;
    (f <= range.hi && f != u32::MAX).then_some(f)
}
