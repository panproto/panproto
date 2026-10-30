//! The contract of a segment, and the invariants that span segments.
//!
//! A segment's own contract is checked from the segment and the partition
//! alone:
//!
//! - (a) every graph/tree node-map key equals its stored node id, every
//!   owned node is owned by the segment under the partition, and every owned
//!   edge's source is an owned node;
//! - (b) every edge target is owned here or is a ghost whose owner is the
//!   target's owner and another segment, and the ghosts are exactly the
//!   remote targets of owned edges;
//! - (c) every in-stub's target is owned here and its source owner is the
//!   source's owner and another segment.
//!
//! A tree segment's fans must also be well formed in their graph form, its
//! ordinary arcs and `Arc`-keyed in-stubs must not meet reified fan nodes,
//! every owned non-fan node must have at most one ordinary incoming arc
//! after owned arcs and in-stubs are combined, and its root, when it has
//! one, must be owned and held.
//!
//! The invariants that span segments need every segment: the ghost and
//! in-stub copies of every crossing edge agree, edge ids are unique, and a
//! tree's parent relation is a tree whose root reaches every node.

use std::collections::{BTreeMap, BTreeSet};

use panproto_schema::{Edge, InstanceShapeKind};

use super::body::{AnySegment, InStub, TreeSegment, is_fan_node};
use super::error::{PartitionDefect, SegmentError};
use super::ids::SegmentId;
use super::partition::{OwnerIndex, Partition};
use crate::ginstance::{EdgeId, HYPER_LABEL_KIND, HYPER_PARENT_KIND};
use crate::view_ref::ViewRef;

/// A violation located by the segment and the element it is about.
pub type Located = (SegmentId, ViewRef, SegmentError);

/// The first of `errors` in `(segment, ViewRef)` order.
pub fn first_located(errors: Vec<Located>) -> Result<(), SegmentError> {
    errors
        .into_iter()
        .enumerate()
        .min_by(|(i, a), (j, b)| (&a.0, &a.1, i).cmp(&(&b.0, &b.1, j)))
        .map_or(Ok(()), |(_, (_, _, e))| Err(e))
}

/// Every violation of segment `seg`'s own contract, each with the element it
/// is about, in the order found.
pub fn contract_violations(seg: &AnySegment, index: &OwnerIndex) -> Vec<(ViewRef, SegmentError)> {
    let me = seg.segment();
    let mut out = node_id_violations(seg);
    // (a) owned nodes.
    for (id, _) in seg.node_anchors() {
        let elem = seg.node_ref(id);
        match index.owner_of_node(id) {
            None => out.push((elem.clone(), SegmentError::Unowned { elem })),
            Some(owner) if owner != me => out.push((
                elem.clone(),
                SegmentError::WrongOwner {
                    elem,
                    expected: owner,
                    found: me,
                },
            )),
            Some(_) => {}
        }
    }
    let boundary = seg.boundary();
    let mut remote_targets: BTreeMap<u32, SegmentId> = BTreeMap::new();
    for edge in seg.owned_edges() {
        let elem = edge.elem.clone();
        // (a) owned edges: the source is an owned node. A source violation
        // does not hide the edge's target violations: the contract lists
        // every violation.
        match index.owner_of_node(edge.src) {
            None => {
                out.push((elem.clone(), SegmentError::Unowned { elem: elem.clone() }));
            }
            Some(owner) if owner != me => {
                out.push((
                    elem.clone(),
                    SegmentError::WrongOwner {
                        elem: elem.clone(),
                        expected: owner,
                        found: me,
                    },
                ));
            }
            Some(_) => {
                if seg.anchor_of(edge.src).is_none() {
                    out.push((
                        elem.clone(),
                        SegmentError::DanglingEndpoint {
                            elem: elem.clone(),
                            node: edge.src,
                        },
                    ));
                }
            }
        }
        // (b) the target is owned here or a ghost of its owner.
        match index.owner_of_node(edge.tgt) {
            None => out.push((elem.clone(), SegmentError::Unowned { elem })),
            Some(owner) if owner == me => {
                if seg.anchor_of(edge.tgt).is_none() {
                    out.push((
                        elem.clone(),
                        SegmentError::DanglingEndpoint {
                            elem,
                            node: edge.tgt,
                        },
                    ));
                }
            }
            Some(owner) => {
                remote_targets.insert(edge.tgt, owner);
            }
        }
    }
    // (b) the ghosts are exactly the remote targets, each naming its owner.
    let mut ghost_nodes: BTreeSet<u32> = remote_targets.keys().copied().collect();
    ghost_nodes.extend(boundary.ghosts.keys().copied());
    for node in ghost_nodes {
        let expected = remote_targets.get(&node).copied();
        let matches = boundary
            .ghosts
            .get(&node)
            .is_some_and(|g| g.node == node && Some(g.owner) == expected);
        if !matches {
            out.push((seg.node_ref(node), SegmentError::GhostMismatch { node }));
        }
    }
    out.extend(in_stub_violations(seg, index));
    if let AnySegment::Tree(tree) = seg {
        out.extend(tree_violations(tree, index));
    }
    out
}

/// Node-map entries whose key and embedded [`crate::Node::id`] disagree.
///
/// This check must precede every local view construction: graph insertion
/// keys nodes by the embedded id, while segment ownership, edge endpoints
/// and canonical segment bytes address the map key.
fn node_id_violations(seg: &AnySegment) -> Vec<(ViewRef, SegmentError)> {
    let nodes = match seg {
        AnySegment::Graph(graph) => &graph.nodes,
        AnySegment::Tree(tree) => &tree.nodes,
        AnySegment::Table(_) => return Vec::new(),
    };
    nodes
        .iter()
        .filter(|(key, node)| **key != node.id)
        .map(|(key, node)| {
            (
                ViewRef::Node(*key),
                SegmentError::NodeIdMismatch {
                    key: *key,
                    found: node.id,
                },
            )
        })
        .collect()
}

/// The violations of (c): every in-stub's target is owned and held here, and
/// its source owner is the source's owner and another segment.
fn in_stub_violations(seg: &AnySegment, index: &OwnerIndex) -> Vec<(ViewRef, SegmentError)> {
    let me = seg.segment();
    let mut out = Vec::new();
    for (key, stub) in &seg.boundary().in_stubs {
        if !stub_matches_key(seg.shape(), key, stub) {
            out.push((
                key.clone(),
                SegmentError::InStubMismatch { elem: key.clone() },
            ));
            continue;
        }
        let target_here = index.owner_of_node(stub.tgt) == Some(me);
        let source_owner = index.owner_of_node(stub.src);
        if !target_here || source_owner != Some(stub.src_owner) || stub.src_owner == me {
            out.push((
                key.clone(),
                SegmentError::InStubMismatch { elem: key.clone() },
            ));
        } else if seg.anchor_of(stub.tgt).is_none() {
            out.push((
                key.clone(),
                SegmentError::DanglingEndpoint {
                    elem: key.clone(),
                    node: stub.tgt,
                },
            ));
        }
    }
    out
}

/// Whether an in-stub's key is the reference the segment's shape gives the
/// edge the stub describes: `Edge` for a graph edge; `Arc { parent, child }`
/// naming the stub's endpoints for a tree arc, and `Edge` for a tree's fan
/// edge, whose label is of a fan edge kind; `Pair { edge, src, tgt }` naming
/// the stub's label and endpoints for a table pair.
fn stub_matches_key(shape: InstanceShapeKind, key: &ViewRef, stub: &InStub) -> bool {
    let fan_kind = || {
        stub.label.kind.as_ref() == HYPER_PARENT_KIND
            || stub.label.kind.as_ref() == HYPER_LABEL_KIND
    };
    match (shape, key) {
        (InstanceShapeKind::Graph, ViewRef::Edge(_)) => true,
        (InstanceShapeKind::Tree, ViewRef::Edge(_)) => fan_kind(),
        (InstanceShapeKind::Tree, ViewRef::Arc { parent, child }) => {
            *parent == stub.src && *child == stub.tgt
        }
        (InstanceShapeKind::Table, ViewRef::Pair { edge, src, tgt }) => {
            *edge == stub.label && src.0 == stub.src && tgt.0 == stub.tgt
        }
        _ => false,
    }
}

/// The label a tree segment gives a fan edge: from the fan's hyper-edge to
/// the target's anchor, of the parent kind with no name or of the label kind
/// named by the child label.
pub fn fan_edge_label(hyper_edge: &str, target_anchor: &str, child_label: Option<&str>) -> Edge {
    Edge {
        src: hyper_edge.into(),
        tgt: target_anchor.into(),
        kind: child_label
            .map_or(HYPER_PARENT_KIND, |_| HYPER_LABEL_KIND)
            .into(),
        name: child_label.map(Into::into),
    }
}

/// The violations of a tree segment's fans, ordinary parent map and root.
fn tree_violations(tree: &TreeSegment, index: &OwnerIndex) -> Vec<(ViewRef, SegmentError)> {
    let me = tree.segment;
    let mut out = Vec::new();
    let not_embedded = |elem: ViewRef| (elem.clone(), SegmentError::NotEmbedded { elem });
    let mut seen_nodes = BTreeSet::new();
    let mut seen_edges: BTreeSet<EdgeId> = BTreeSet::new();
    let mut previous: Option<u32> = None;
    for fan in &tree.fans {
        let elem = ViewRef::Node(fan.node);
        let ordered = previous.is_none_or(|p| p < fan.node);
        previous = Some(fan.node);
        let Some(node) = tree.nodes.get(&fan.node) else {
            out.push(not_embedded(elem));
            continue;
        };
        let ids = std::iter::once((None, fan.parent_edge)).chain(
            fan.child_edges
                .iter()
                .map(|(label, id)| (Some(label.as_str()), *id)),
        );
        // Record the fan node even when it is out of order, so the misordered
        // fan is reported once and not again as a fan node without a record.
        let first = seen_nodes.insert(fan.node);
        let mut well_formed = ordered && is_fan_node(node) && first;
        for (label, id) in ids {
            well_formed &= seen_edges.insert(id);
            well_formed &= tree.arcs.get(&id).is_some_and(|(src, tgt, edge)| {
                let anchor = tree
                    .nodes
                    .get(tgt)
                    .map(|n| &n.anchor)
                    .or_else(|| tree.boundary.ghosts.get(tgt).map(|g| &g.anchor));
                *src == fan.node
                    && anchor.is_some_and(|a| *edge == fan_edge_label(&node.anchor, a, label))
            });
        }
        if !well_formed {
            out.push(not_embedded(elem));
        }
    }
    for (id, node) in &tree.nodes {
        if is_fan_node(node) && !seen_nodes.contains(id) {
            out.push(not_embedded(ViewRef::Node(*id)));
        }
    }
    out.extend(ordinary_fan_incidences(tree).into_iter().map(not_embedded));
    out.extend(
        duplicate_ordinary_parents(tree)
            .into_iter()
            .map(|node| not_embedded(ViewRef::Node(node))),
    );
    if let Some(root) = tree.root {
        let held = tree.nodes.get(&root).is_some_and(|n| !is_fan_node(n));
        if !held || index.owner_of_node(root) != Some(me) {
            out.push(not_embedded(ViewRef::Node(root)));
        }
    }
    out.extend(counter_violations(tree).into_iter().map(not_embedded));
    out
}

/// Owned nodes with more than one ordinary incoming tree arc, counting
/// both locally owned arcs and ordinary cross-boundary in-stubs.
fn duplicate_ordinary_parents(tree: &TreeSegment) -> Vec<u32> {
    // A malformed tree can contain a reified fan-kind node without the
    // corresponding `FanEmbedding` record. Its incident arcs are still fan
    // incidences, not ordinary tree parents; `tree_violations` reports the
    // missing record independently.
    let fan_nodes: BTreeSet<u32> = tree
        .nodes
        .iter()
        .filter_map(|(id, node)| is_fan_node(node).then_some(*id))
        .collect();
    let fan_edges = tree.fan_edge_ids();
    let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
    for (id, (src, tgt, _)) in &tree.arcs {
        if !fan_edges.contains(id)
            && !fan_nodes.contains(src)
            && !fan_nodes.contains(tgt)
            && tree.nodes.contains_key(tgt)
        {
            *counts.entry(*tgt).or_default() += 1;
        }
    }
    for (at, stub) in &tree.boundary.in_stubs {
        if matches!(at, ViewRef::Arc { .. })
            && tree.nodes.contains_key(&stub.tgt)
            && !fan_nodes.contains(&stub.tgt)
        {
            *counts.entry(stub.tgt).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .filter_map(|(node, count)| (count > 1).then_some(node))
        .collect()
}

/// Ordinary tree arcs cannot meet reified fan nodes. Such an owned arc has
/// no native-tree interpretation; such an in-stub also erases the arc's
/// embedding edge id at the segment boundary.
fn ordinary_fan_incidences(tree: &TreeSegment) -> Vec<ViewRef> {
    let fan_nodes = tree.fan_node_ids();
    let fan_edges = tree.fan_edge_ids();
    let mut out: Vec<ViewRef> = tree
        .arcs
        .iter()
        .filter(|(id, (src, tgt, _))| {
            !fan_edges.contains(id) && (fan_nodes.contains(src) || fan_nodes.contains(tgt))
        })
        .map(|(_, (src, tgt, _))| ViewRef::Arc {
            parent: *src,
            child: *tgt,
        })
        .collect();
    out.extend(
        tree.boundary
            .in_stubs
            .iter()
            .filter(|(at, stub)| matches!(at, ViewRef::Arc { .. }) && fan_nodes.contains(&stub.tgt))
            .map(|(at, _)| at.clone()),
    );
    out
}

/// The elements a tree segment's embedding counters are wrong about: the
/// root when it holds the root without both counters, and the least node id
/// and edge id it holds at or above the counter of their kind. A counter in
/// a segment without the root is about no element; [`check_counter_holder`]
/// refuses it.
fn counter_violations(tree: &TreeSegment) -> Vec<ViewRef> {
    let mut out = Vec::new();
    if let Some(root) = tree.root
        && (tree.next_node.is_none() || tree.next_edge.is_none())
    {
        out.push(ViewRef::Node(root));
    }
    if let Some(next) = tree.next_node
        && let Some(id) = tree.nodes.range(next..).next()
    {
        out.push(ViewRef::Node(*id.0));
    }
    if let Some(next) = tree.next_edge
        && let Some(id) = tree.arcs.range(EdgeId(next)..).next()
    {
        out.push(ViewRef::Edge(*id.0));
    }
    out
}

/// Check one segment's own contract under `partition`.
///
/// # Errors
///
/// In this order: [`SegmentError::ShapeMismatch`] when the segment's shape
/// is not the partition's, the partition's own error when it is invalid,
/// [`SegmentError::MalformedPartition`] when the partition does not list the
/// segment or a tree segment holds the embedding's counters without the
/// root, and [`SegmentError::Invalid`] with every violation, sorted by the
/// element it is about, when the segment breaks its contract.
pub fn check_segment(segment: &AnySegment, partition: &Partition) -> Result<(), SegmentError> {
    check_shape(segment, partition)?;
    let index = partition.owner_index()?;
    check_listed(segment, partition)?;
    check_segment_with(segment, &index)
}

/// Check that a tree segment holds the embedding's counters only when it
/// holds the tree's root.
///
/// # Errors
///
/// [`SegmentError::MalformedPartition`] with
/// [`PartitionDefect::CountersWithoutRoot`] when it holds `next_node` or
/// `next_edge` without `root`: the counters' values need not be ids any
/// segment holds, so no element names the defect.
pub const fn check_counter_holder(segment: &AnySegment) -> Result<(), SegmentError> {
    match segment {
        AnySegment::Tree(tree)
            if tree.root.is_none() && (tree.next_node.is_some() || tree.next_edge.is_some()) =>
        {
            Err(SegmentError::MalformedPartition {
                segment: tree.segment,
                defect: PartitionDefect::CountersWithoutRoot,
            })
        }
        _ => Ok(()),
    }
}

/// [`check_segment`] with the owner index already built and the segment
/// known to be listed.
///
/// # Errors
///
/// As [`check_counter_holder`], then [`SegmentError::Invalid`] with every
/// violation, sorted by the element it is about.
pub fn check_segment_with(segment: &AnySegment, index: &OwnerIndex) -> Result<(), SegmentError> {
    check_counter_holder(segment)?;
    let mut violations = contract_violations(segment, index);
    if violations.is_empty() {
        return Ok(());
    }
    violations.sort_by(|a, b| a.0.cmp(&b.0));
    Err(SegmentError::Invalid {
        violations: violations.into_iter().map(|(_, e)| e).collect(),
    })
}

/// Check that `segment` has the partition's shape. Every segment entry
/// point checks this before it validates the partition.
///
/// # Errors
///
/// [`SegmentError::ShapeMismatch`] when the shapes differ.
pub fn check_shape(segment: &AnySegment, partition: &Partition) -> Result<(), SegmentError> {
    if segment.shape() == partition.shape() {
        Ok(())
    } else {
        Err(SegmentError::ShapeMismatch {
            expected: partition.shape(),
            found: segment.shape(),
        })
    }
}

/// Check that the partition lists `segment` with its shape.
pub fn check_listed(segment: &AnySegment, partition: &Partition) -> Result<(), SegmentError> {
    check_shape(segment, partition)?;
    if partition.spec(segment.segment()).is_none() {
        return Err(SegmentError::MalformedPartition {
            segment: segment.segment(),
            defect: PartitionDefect::UnknownSegment,
        });
    }
    Ok(())
}

/// The segments laid out by id under `partition`: each listed, with the
/// partition's shape, exactly once, and a tree segment holding the
/// embedding's counters only with the root ([`check_counter_holder`]).
pub fn arrange<'a>(
    segments: impl IntoIterator<Item = &'a AnySegment>,
    partition: &Partition,
) -> Result<Vec<&'a AnySegment>, SegmentError> {
    let mut slots: Vec<Option<&AnySegment>> = vec![None; partition.specs().len()];
    for segment in segments {
        check_listed(segment, partition)?;
        check_counter_holder(segment)?;
        let at = usize::try_from(segment.segment().0).unwrap_or(usize::MAX);
        let slot = slots
            .get_mut(at)
            .ok_or_else(|| SegmentError::MalformedPartition {
                segment: segment.segment(),
                defect: PartitionDefect::UnknownSegment,
            })?;
        if slot.replace(segment).is_some() {
            return Err(SegmentError::MalformedPartition {
                segment: segment.segment(),
                defect: PartitionDefect::SegmentRepeated,
            });
        }
    }
    slots
        .into_iter()
        .enumerate()
        .map(|(i, slot)| {
            slot.ok_or_else(|| SegmentError::MalformedPartition {
                segment: SegmentId(u32::try_from(i).unwrap_or(u32::MAX)),
                defect: PartitionDefect::SegmentMissing,
            })
        })
        .collect()
}

/// Every violation of an invariant spanning the segments `segs` (laid out
/// by id): ghost and in-stub agreement, edge id uniqueness, and for trees
/// one root, at most one parent per node and every node reached from the
/// root.
pub fn cross_violations(segs: &[&AnySegment], index: &OwnerIndex) -> Vec<Located> {
    let mut out = Vec::new();
    let mut edge_holders: BTreeMap<EdgeId, Vec<SegmentId>> = BTreeMap::new();
    for seg in segs {
        let me = seg.segment();
        let mut crossing: BTreeSet<ViewRef> = BTreeSet::new();
        for edge in seg.owned_edges() {
            if let Some(id) = edge.id {
                edge_holders.entry(id).or_default().push(me);
            }
            let Some(owner) = index.owner_of_node(edge.tgt).filter(|o| *o != me) else {
                continue;
            };
            let Some(peer) = segs.get(usize::try_from(owner.0).unwrap_or(usize::MAX)) else {
                continue;
            };
            crossing.insert(edge.elem.clone());
            let expected = seg.anchor_of(edge.src).map(|anchor| InStub {
                src: edge.src,
                src_owner: me,
                src_anchor: anchor.clone(),
                tgt: edge.tgt,
                label: edge.label.clone(),
            });
            if expected.is_none() || peer.boundary().in_stubs.get(&edge.elem) != expected.as_ref() {
                out.push((
                    owner,
                    edge.elem.clone(),
                    SegmentError::InStubMismatch { elem: edge.elem },
                ));
            }
            let ghost_anchor = seg.boundary().ghosts.get(&edge.tgt).map(|g| &g.anchor);
            if ghost_anchor.is_none() || peer.anchor_of(edge.tgt) != ghost_anchor {
                out.push((
                    me,
                    seg.node_ref(edge.tgt),
                    SegmentError::GhostMismatch { node: edge.tgt },
                ));
            }
        }
        // Every in-stub of another segment from this one names an edge this
        // segment owns.
        for peer in segs {
            if peer.segment() == me {
                continue;
            }
            for (key, stub) in &peer.boundary().in_stubs {
                if stub.src_owner == me && !crossing.contains(key) {
                    out.push((
                        peer.segment(),
                        key.clone(),
                        SegmentError::InStubMismatch { elem: key.clone() },
                    ));
                }
            }
        }
    }
    for (edge, holders) in edge_holders {
        if holders.len() > 1 {
            out.push((
                holders[0],
                ViewRef::Edge(edge),
                SegmentError::DuplicateEdgeId {
                    edge,
                    segments: holders,
                },
            ));
        }
    }
    let trees: Vec<&TreeSegment> = segs
        .iter()
        .filter_map(|s| match s {
            AnySegment::Tree(t) => Some(t),
            _ => None,
        })
        .collect();
    if !trees.is_empty() {
        out.extend(tree_cross_violations(&trees, index));
    }
    out
}

/// The tree invariants that span segments: one root, at most one parent
/// arc per node, and every tree node reached from the root.
fn tree_cross_violations(trees: &[&TreeSegment], index: &OwnerIndex) -> Vec<Located> {
    let mut out = Vec::new();
    let locate = |node: u32| index.owner_of_node(node).unwrap_or(SegmentId(0));
    let not_embedded = |node: u32| {
        (
            locate(node),
            ViewRef::Node(node),
            SegmentError::NotEmbedded {
                elem: ViewRef::Node(node),
            },
        )
    };
    let roots: Vec<u32> = trees.iter().filter_map(|t| t.root).collect();
    let mut children: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut parents: BTreeMap<u32, usize> = BTreeMap::new();
    let mut tree_nodes: BTreeSet<u32> = BTreeSet::new();
    for tree in trees {
        let fan_nodes = tree.fan_node_ids();
        let fan_edges = tree.fan_edge_ids();
        tree_nodes.extend(tree.nodes.keys().filter(|id| !fan_nodes.contains(id)));
        for (id, (parent, child, _)) in &tree.arcs {
            if !fan_edges.contains(id) {
                children.entry(*parent).or_default().push(*child);
                *parents.entry(*child).or_default() += 1;
            }
        }
    }
    for (node, count) in &parents {
        if *count > 1 {
            out.push(not_embedded(*node));
        }
    }
    let Some(&root) = roots.first() else {
        out.push((
            SegmentId(0),
            ViewRef::Node(0),
            SegmentError::NotEmbedded {
                elem: ViewRef::Node(0),
            },
        ));
        return out;
    };
    for extra in roots.iter().skip(1) {
        out.push(not_embedded(*extra));
    }
    if parents.contains_key(&root) {
        out.push(not_embedded(root));
    }
    let mut reached = BTreeSet::from([root]);
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        for child in children.get(&node).into_iter().flatten() {
            if reached.insert(*child) {
                stack.push(*child);
            }
        }
    }
    if let Some(unreached) = tree_nodes.difference(&reached).next() {
        out.push(not_embedded(*unreached));
    }
    out.extend(counters_above_every_id(trees, root, index));
    out
}

/// The least node id and the least edge id, across every tree segment, at or
/// above the embedding counter of its kind that the root's segment carries.
fn counters_above_every_id(trees: &[&TreeSegment], root: u32, index: &OwnerIndex) -> Vec<Located> {
    let Some((next_node, next_edge)) = trees
        .iter()
        .find(|t| t.root == Some(root))
        .and_then(|t| t.next_node.zip(t.next_edge))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let node = trees
        .iter()
        .filter_map(|t| t.nodes.range(next_node..).next().map(|(id, _)| *id))
        .min();
    if let Some(node) = node {
        let elem = ViewRef::Node(node);
        out.push((
            index.owner_of_node(node).unwrap_or(SegmentId(0)),
            elem.clone(),
            SegmentError::NotEmbedded { elem },
        ));
    }
    let edge = trees
        .iter()
        .filter_map(|t| {
            t.arcs
                .range(EdgeId(next_edge)..)
                .next()
                .map(|(id, _)| (*id, t.segment))
        })
        .min();
    if let Some((edge, holder)) = edge {
        let elem = ViewRef::Edge(edge);
        out.push((holder, elem.clone(), SegmentError::NotEmbedded { elem }));
    }
    out
}

/// Check the invariants of a whole set of segments under `partition`.
///
/// They are every segment's own contract, the agreement of the ghost and
/// in-stub copies of every crossing edge, the uniqueness of edge ids, and
/// for trees that the parent relation over the boundary is a tree whose
/// root reaches every node.
///
/// # Errors
///
/// The partition's own error, a [`SegmentError::MalformedPartition`] or
/// [`SegmentError::ShapeMismatch`] when the segments are not laid out under
/// it, and otherwise the first violation in `(segment, ViewRef)` order.
pub fn verify_partition(
    segments: &[AnySegment],
    partition: &Partition,
) -> Result<(), SegmentError> {
    for segment in segments {
        check_shape(segment, partition)?;
    }
    let index = partition.owner_index()?;
    let segs = arrange(segments, partition)?;
    first_located(all_violations(&segs, &index))
}

/// Every own-contract and cross-segment violation of the arranged segments.
pub fn all_violations(segs: &[&AnySegment], index: &OwnerIndex) -> Vec<Located> {
    let mut errors: Vec<Located> = segs
        .iter()
        .flat_map(|seg| {
            contract_violations(seg, index)
                .into_iter()
                .map(|(elem, e)| (seg.segment(), elem, e))
        })
        .collect();
    errors.extend(cross_violations(segs, index));
    errors
}
