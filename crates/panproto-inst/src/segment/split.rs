//! Splitting a whole instance into segments, and assembling it back.
//!
//! The splitters serve callers whose instance fits in memory, and are the
//! oracle the segment laws are tested against: assembling the segments a
//! splitter writes gives back the instance it was given.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_schema::{Edge, InstanceShapeKind};

use super::body::{AnySegment, Boundary, Ghost, GraphSegment, InStub, TableSegment, TreeSegment};
use super::check::{all_violations, arrange, fan_edge_label, first_located};
use super::error::SegmentError;
use super::ids::SegmentId;
use super::partition::{OwnerIndex, Partition};
use super::store::ArtifactKind;
use crate::error::InstError;
use crate::fan::Fan;
use crate::functor::{FInstance, Row};
use crate::ginstance::{EdgeId, FanEmbedding, GEdge, GInstance, TreeEmbedding};
use crate::metadata::{Node, NodeShape};
use crate::view_ref::{RowId, ViewRef};
use crate::wtype::WInstance;

/// Check that `partition` is valid and of `shape`, and index its owners.
fn index_for(partition: &Partition, shape: InstanceShapeKind) -> Result<OwnerIndex, SegmentError> {
    if partition.shape != shape {
        return Err(SegmentError::ShapeMismatch {
            expected: shape,
            found: partition.shape,
        });
    }
    partition.owner_index()
}

/// The owner of node `id`, or the error for `elem`, the element carrying it.
fn owner(
    index: &OwnerIndex,
    id: u32,
    elem: impl FnOnce() -> ViewRef,
) -> Result<SegmentId, SegmentError> {
    index
        .owner_of_node(id)
        .ok_or_else(|| SegmentError::Unowned { elem: elem() })
}

/// The position of segment `id` in a list laid out by id.
fn slot(id: SegmentId) -> usize {
    usize::try_from(id.0).unwrap_or(usize::MAX)
}

/// Record a crossing edge in both boundaries: a ghost of the target in the
/// source's segment, an in-stub in the target's.
fn cross(
    boundaries: &mut [Boundary],
    key: ViewRef,
    stub: InStub,
    tgt_owner: SegmentId,
    tgt_anchor: &Name,
) {
    let src_owner = stub.src_owner;
    if let Some(b) = boundaries.get_mut(slot(src_owner)) {
        b.ghosts.entry(stub.tgt).or_insert_with(|| Ghost {
            node: stub.tgt,
            owner: tgt_owner,
            anchor: tgt_anchor.clone(),
        });
    }
    if let Some(b) = boundaries.get_mut(slot(tgt_owner)) {
        b.in_stubs.insert(key, stub);
    }
}

/// Split a graph instance into the segments of `partition`.
///
/// Every node goes to its owner, and every edge to the owner of its
/// source; an edge whose target is owned elsewhere leaves a ghost in its
/// segment and an in-stub in its target's. Every segment of the partition is
/// returned, in id order, empty ones included.
///
/// # Errors
///
/// [`SegmentError::ShapeMismatch`] when the partition is not a graph
/// partition, the partition's own error when it is invalid, and
/// [`SegmentError::Unowned`] for the first node, then the first edge, in
/// ascending id, whose id or target no range contains.
pub fn partition_graph(
    graph: &GInstance,
    partition: &Partition,
) -> Result<Vec<GraphSegment>, SegmentError> {
    let index = index_for(partition, InstanceShapeKind::Graph)?;
    let mut segments: Vec<GraphSegment> = partition
        .specs
        .iter()
        .map(|s| GraphSegment {
            segment: s.id,
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            boundary: Boundary::default(),
        })
        .collect();
    let mut ids: Vec<u32> = graph.nodes().keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let o = owner(&index, id, || ViewRef::Node(id))?;
        if let (Some(seg), Some(node)) = (segments.get_mut(slot(o)), graph.node(id)) {
            seg.nodes.insert(id, node.clone());
        }
    }
    let mut boundaries: Vec<Boundary> = vec![Boundary::default(); segments.len()];
    for (id, edge) in graph.edges_sorted() {
        let elem = || ViewRef::Edge(id);
        let src_owner = owner(&index, edge.src, elem)?;
        let tgt_owner = owner(&index, edge.tgt, elem)?;
        if let Some(seg) = segments.get_mut(slot(src_owner)) {
            seg.edges.insert(id, edge.clone());
        }
        if tgt_owner != src_owner {
            let anchor = |n: u32| graph.node(n).map(|n| n.anchor.clone()).unwrap_or_default();
            let stub = InStub {
                src: edge.src,
                src_owner,
                src_anchor: anchor(edge.src),
                tgt: edge.tgt,
                label: edge.edge.clone(),
            };
            cross(
                &mut boundaries,
                ViewRef::Edge(id),
                stub,
                tgt_owner,
                &anchor(edge.tgt),
            );
        }
    }
    for (seg, boundary) in segments.iter_mut().zip(boundaries) {
        seg.boundary = boundary;
    }
    Ok(segments)
}

/// Split a table instance into the segments of `partition`.
///
/// Every row goes to its owner, and every foreign-key pair to the owner of
/// its referencing row; a pair whose referenced row is owned elsewhere leaves
/// a ghost (anchored at the row's table) in its segment and an in-stub in the
/// referenced row's.
///
/// # Errors
///
/// [`SegmentError::ShapeMismatch`] when the partition is not a table
/// partition, the partition's own error when it is invalid, and
/// [`SegmentError::Unowned`] for the first row, in ascending id, that no
/// range contains.
pub fn partition_table(
    tables: &FInstance,
    partition: &Partition,
) -> Result<Vec<TableSegment>, SegmentError> {
    let index = index_for(partition, InstanceShapeKind::Table)?;
    let mut segments: Vec<TableSegment> = partition
        .specs
        .iter()
        .map(|s| TableSegment {
            segment: s.id,
            rows: BTreeMap::new(),
            pairs: BTreeSet::new(),
            boundary: Boundary::default(),
        })
        .collect();
    for id in tables.row_ids() {
        let o = owner(&index, id.0, || ViewRef::Row(id))?;
        if let (Some(seg), Some(table), Some(row)) = (
            segments.get_mut(slot(o)),
            tables.table_of(id),
            tables.row(id),
        ) {
            seg.rows.insert(id, (table.clone(), row.clone()));
        }
    }
    let mut boundaries: Vec<Boundary> = vec![Boundary::default(); segments.len()];
    for (edge, pairs) in tables.foreign_keys() {
        for &(src, tgt) in pairs {
            let elem = || ViewRef::Pair {
                edge: edge.clone(),
                src,
                tgt,
            };
            let src_owner = owner(&index, src.0, elem)?;
            let tgt_owner = owner(&index, tgt.0, elem)?;
            if let Some(seg) = segments.get_mut(slot(src_owner)) {
                seg.pairs.insert((edge.clone(), src, tgt));
            }
            if tgt_owner != src_owner {
                let anchor = |r: RowId| tables.table_of(r).cloned().unwrap_or_default();
                let stub = InStub {
                    src: src.0,
                    src_owner,
                    src_anchor: anchor(src),
                    tgt: tgt.0,
                    label: edge.clone(),
                };
                cross(&mut boundaries, elem(), stub, tgt_owner, &anchor(tgt));
            }
        }
    }
    for (seg, boundary) in segments.iter_mut().zip(boundaries) {
        seg.boundary = boundary;
    }
    Ok(segments)
}

/// Check that `embedding` embeds `tree` as one tree, in the form a tree
/// segment can carry: node ids match their keys; the root is a node anchored
/// at the schema root and every node is reached from it along arcs, each
/// node having at most one parent; every arc has an edge id, ascending in arc
/// order; every fan has its ids, with its fan node not a tree node, the
/// fan's labels, its endpoints tree nodes, and parent edge ids ascending in
/// fan order; and no edge id is used twice.
fn check_embedding(
    tree: &WInstance,
    embedding: &TreeEmbedding,
    index: &OwnerIndex,
) -> Result<(), SegmentError> {
    let fail = |elem: ViewRef| Err(SegmentError::NotEmbedded { elem });
    let mut keys: Vec<u32> = tree.nodes.keys().copied().collect();
    keys.sort_unstable();
    if let Some(key) = keys.iter().find(|k| tree.nodes[k].id != **k) {
        return fail(ViewRef::Node(*key));
    }
    match tree.nodes.get(&tree.root) {
        Some(root) if root.anchor == tree.schema_root => {}
        _ => return fail(ViewRef::Node(tree.root)),
    }
    let mut parent_of: HashMap<u32, u32> = HashMap::new();
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut last: Option<EdgeId> = None;
    // Each edge id with the segments of the elements holding it.
    let mut holders: BTreeMap<EdgeId, Vec<SegmentId>> = BTreeMap::new();
    for (parent, child, _) in &tree.arcs {
        let elem = ViewRef::Arc {
            parent: *parent,
            child: *child,
        };
        let known = tree.nodes.contains_key(parent) && tree.nodes.contains_key(child);
        if !known || *child == tree.root || parent_of.insert(*child, *parent).is_some() {
            return fail(elem);
        }
        let Some(&id) = embedding.arcs.get(child) else {
            return fail(elem);
        };
        if last.is_some_and(|l| l >= id) {
            return fail(elem);
        }
        last = Some(id);
        children.entry(*parent).or_default().push(*child);
        holders
            .entry(id)
            .or_default()
            .push(index.owner_of_node(*parent).unwrap_or(SegmentId(0)));
    }
    if let Some(extra) = embedding.arcs.keys().find(|c| !parent_of.contains_key(c)) {
        return fail(ViewRef::Node(*extra));
    }
    let mut reached = BTreeSet::from([tree.root]);
    let mut stack = vec![tree.root];
    while let Some(node) = stack.pop() {
        for child in children.get(&node).into_iter().flatten() {
            if reached.insert(*child) {
                stack.push(*child);
            }
        }
    }
    if let Some(unreached) = keys.iter().find(|k| !reached.contains(k)) {
        return fail(ViewRef::Node(*unreached));
    }
    if embedding.fans.len() != tree.fans.len() {
        let node = embedding
            .fans
            .get(tree.fans.len())
            .map_or(embedding.next_node, |f| f.node);
        return fail(ViewRef::Node(node));
    }
    let mut fan_nodes = BTreeSet::new();
    let mut last_parent_edge: Option<EdgeId> = None;
    for (fan, ids) in tree.fans.iter().zip(&embedding.fans) {
        let elem = ViewRef::Node(ids.node);
        let labels_match = fan.children.len() == ids.child_edges.len()
            && fan.children.keys().all(|l| ids.child_edges.contains_key(l));
        let endpoints_known = tree.nodes.contains_key(&fan.parent)
            && fan.children.values().all(|c| tree.nodes.contains_key(c));
        let ordered = last_parent_edge.is_none_or(|l| l < ids.parent_edge);
        if tree.nodes.contains_key(&ids.node)
            || !fan_nodes.insert(ids.node)
            || !labels_match
            || !endpoints_known
            || !ordered
        {
            return fail(elem);
        }
        last_parent_edge = Some(ids.parent_edge);
        let owner = index.owner_of_node(ids.node).unwrap_or(SegmentId(0));
        for id in std::iter::once(ids.parent_edge).chain(ids.child_edges.values().copied()) {
            holders.entry(id).or_default().push(owner);
        }
    }
    if let Some((edge, mut segments)) = holders.into_iter().find(|(_, h)| h.len() > 1) {
        segments.sort_unstable();
        return Err(SegmentError::DuplicateEdgeId { edge, segments });
    }
    Ok(())
}

/// Split a tree instance, with the graph embedding that names its arcs and
/// fans, into the segments of `partition`.
///
/// Every tree node goes to its owner, and every arc, under its embedding
/// edge id, to the owner of its parent. Every fan goes, in its graph form,
/// to the owner of its fan node: the fan node (anchored at the fan's
/// hyper-edge, with the hyper-edge shape) and its parent and child edges,
/// labelled as [`crate::reify_fans`] labels them without a schema. Arcs and
/// fan edges into nodes owned elsewhere leave ghosts and in-stubs. The root
/// is recorded in its owner.
///
/// # Errors
///
/// [`SegmentError::ShapeMismatch`] when the partition is not a tree
/// partition; the partition's own error when it is invalid;
/// [`SegmentError::NotEmbedded`] when the embedding does not embed the tree
/// as one tree in arc order (see the type's documentation of the checks);
/// [`SegmentError::DuplicateEdgeId`] when an edge id names two elements; and
/// [`SegmentError::Unowned`] for the first node, arc or fan whose id no range
/// contains.
pub fn partition_tree(
    tree: &WInstance,
    embedding: &TreeEmbedding,
    partition: &Partition,
) -> Result<Vec<TreeSegment>, SegmentError> {
    let index = index_for(partition, InstanceShapeKind::Tree)?;
    check_embedding(tree, embedding, &index)?;
    let mut segments: Vec<TreeSegment> = partition
        .specs
        .iter()
        .map(|s| TreeSegment {
            segment: s.id,
            nodes: BTreeMap::new(),
            arcs: BTreeMap::new(),
            fans: Vec::new(),
            root: None,
            boundary: Boundary::default(),
        })
        .collect();
    let mut ids: Vec<u32> = tree.nodes.keys().copied().collect();
    ids.sort_unstable();
    for id in &ids {
        let o = owner(&index, *id, || ViewRef::Node(*id))?;
        if let Some(seg) = segments.get_mut(slot(o)) {
            seg.nodes.insert(*id, tree.nodes[id].clone());
            if *id == tree.root {
                seg.root = Some(*id);
            }
        }
    }
    let anchor = |n: u32| tree.nodes[&n].anchor.clone();
    let mut boundaries: Vec<Boundary> = vec![Boundary::default(); segments.len()];
    for (parent, child, label) in &tree.arcs {
        let elem = ViewRef::Arc {
            parent: *parent,
            child: *child,
        };
        let id = embedding.arcs[child];
        let src_owner = owner(&index, *parent, || elem.clone())?;
        let tgt_owner = owner(&index, *child, || elem.clone())?;
        if let Some(seg) = segments.get_mut(slot(src_owner)) {
            seg.arcs.insert(id, (*parent, *child, label.clone()));
        }
        if tgt_owner != src_owner {
            let stub = InStub {
                src: *parent,
                src_owner,
                src_anchor: anchor(*parent),
                tgt: *child,
                label: label.clone(),
            };
            cross(&mut boundaries, elem, stub, tgt_owner, &anchor(*child));
        }
    }
    for (fan, ids) in tree.fans.iter().zip(&embedding.fans) {
        let fan_owner = owner(&index, ids.node, || ViewRef::Node(ids.node))?;
        let hyper: Name = fan.hyper_edge_id.as_str().into();
        let edges = std::iter::once((None, ids.parent_edge, fan.parent)).chain(
            fan.children
                .iter()
                .map(|(label, child)| (Some(label.as_str()), ids.child_edges[label], *child)),
        );
        for (label, id, target) in edges {
            let edge = fan_edge_label(&hyper, &anchor(target), label);
            let tgt_owner = owner(&index, target, || ViewRef::Edge(id))?;
            if tgt_owner != fan_owner {
                let stub = InStub {
                    src: ids.node,
                    src_owner: fan_owner,
                    src_anchor: hyper.clone(),
                    tgt: target,
                    label: edge.clone(),
                };
                cross(
                    &mut boundaries,
                    ViewRef::Edge(id),
                    stub,
                    tgt_owner,
                    &anchor(target),
                );
            }
            if let Some(seg) = segments.get_mut(slot(fan_owner)) {
                seg.arcs.insert(id, (ids.node, target, edge));
            }
        }
        if let Some(seg) = segments.get_mut(slot(fan_owner)) {
            seg.nodes.insert(
                ids.node,
                Node::new(ids.node, hyper).with_shape(NodeShape::HyperEdge),
            );
            seg.fans.push(ids.clone());
        }
    }
    for (seg, boundary) in segments.iter_mut().zip(boundaries) {
        seg.boundary = boundary;
        seg.fans.sort_unstable_by_key(|f| f.node);
    }
    Ok(segments)
}

/// The error of an instance that refuses the assembled parts.
fn refused(error: &InstError) -> SegmentError {
    SegmentError::Decode {
        artifact: ArtifactKind::Assembly,
        message: error.to_string(),
    }
}

/// Lay the segments out under `partition` and check every invariant they
/// must keep together.
fn checked(
    segments: &[AnySegment],
    partition: &Partition,
    shape: InstanceShapeKind,
) -> Result<(), SegmentError> {
    let index = index_for(partition, shape)?;
    let segs = arrange(segments, partition)?;
    first_located(all_violations(&segs, &index))
}

/// Assemble a graph instance from its segments.
///
/// # Errors
///
/// [`SegmentError::ShapeMismatch`] or the partition's own error when the
/// partition is not a valid graph partition,
/// [`SegmentError::MalformedPartition`] when a segment is missing, repeated
/// or not listed, and otherwise the first broken invariant (a segment's own
/// contract, ghost and in-stub agreement, edge id uniqueness) in
/// `(segment, ViewRef)` order.
pub fn assemble_graph(
    segments: impl IntoIterator<Item = GraphSegment>,
    partition: &Partition,
) -> Result<GInstance, SegmentError> {
    let segments: Vec<AnySegment> = segments.into_iter().map(AnySegment::Graph).collect();
    checked(&segments, partition, InstanceShapeKind::Graph)?;
    let mut graph = GInstance::new();
    let mut edges: Vec<(EdgeId, GEdge)> = Vec::new();
    for segment in segments {
        if let AnySegment::Graph(seg) = segment {
            for node in seg.nodes.into_values() {
                graph.insert_node(node).map_err(|e| refused(&e))?;
            }
            edges.extend(seg.edges);
        }
    }
    edges.sort_unstable_by_key(|(id, _)| *id);
    for (id, edge) in edges {
        graph.insert_edge(id, edge).map_err(|e| refused(&e))?;
    }
    Ok(graph)
}

/// Assemble a table instance from its segments.
///
/// # Errors
///
/// As [`assemble_graph`], for a table partition, and
/// [`SegmentError::Decode`] of the assembly when the assembled rows and
/// pairs break a table invariant (a pair whose rows are not in its edge's
/// endpoint tables).
pub fn assemble_table(
    segments: impl IntoIterator<Item = TableSegment>,
    partition: &Partition,
) -> Result<FInstance, SegmentError> {
    let segments: Vec<AnySegment> = segments.into_iter().map(AnySegment::Table).collect();
    checked(&segments, partition, InstanceShapeKind::Table)?;
    let mut tables: BTreeMap<Name, Vec<(RowId, Row)>> = BTreeMap::new();
    let mut keys: BTreeMap<Edge, Vec<(RowId, RowId)>> = BTreeMap::new();
    for segment in segments {
        if let AnySegment::Table(seg) = segment {
            for (id, (table, row)) in seg.rows {
                tables.entry(table).or_default().push((id, row));
            }
            for (edge, src, tgt) in seg.pairs {
                keys.entry(edge).or_default().push((src, tgt));
            }
        }
    }
    FInstance::from_parts(tables, keys).map_err(|e| refused(&e))
}

/// Assemble a tree instance and its graph embedding from its segments.
///
/// The arcs are merged by edge id, which is arc order, and the fans by
/// parent edge id, which is fan order. The tree's schema root is its root's
/// anchor. The embedding's floors are the least ids above every node and
/// edge id the segments hold.
///
/// # Errors
///
/// As [`assemble_graph`], for a tree partition, together with
/// [`SegmentError::NotEmbedded`] when the parts do not form one tree (no
/// root or two, a node with two parents, a node the root does not reach, a
/// malformed fan), and [`SegmentError::AllocationExhausted`] when a floor
/// would pass `u32::MAX`.
pub fn assemble_tree(
    segments: impl IntoIterator<Item = TreeSegment>,
    partition: &Partition,
) -> Result<(WInstance, TreeEmbedding), SegmentError> {
    let segments: Vec<AnySegment> = segments.into_iter().map(AnySegment::Tree).collect();
    checked(&segments, partition, InstanceShapeKind::Tree)?;
    let mut nodes: HashMap<u32, Node> = HashMap::new();
    let mut arcs: BTreeMap<EdgeId, (u32, u32, Edge)> = BTreeMap::new();
    let mut fans: Vec<(FanEmbedding, Fan)> = Vec::new();
    let mut root = None;
    let mut max_node: Option<(u32, SegmentId)> = None;
    let mut max_edge: Option<(u32, SegmentId)> = None;
    for segment in segments {
        let AnySegment::Tree(mut seg) = segment else {
            continue;
        };
        let fan_edges = seg.fan_edge_ids();
        if let Some(id) = seg.nodes.keys().next_back().copied() {
            max_node = max_node.max(Some((id, seg.segment)));
        }
        if let Some(id) = seg.arcs.keys().next_back().copied() {
            max_edge = max_edge.max(Some((id.0, seg.segment)));
        }
        root = root.or(seg.root);
        for ids in std::mem::take(&mut seg.fans) {
            let hyper = seg
                .nodes
                .remove(&ids.node)
                .map(|n| n.anchor)
                .unwrap_or_default();
            let target = |id: &EdgeId| seg.arcs.get(id).map_or(0, |(_, tgt, _)| *tgt);
            let mut fan = Fan::new(hyper.as_ref(), target(&ids.parent_edge));
            for (label, id) in &ids.child_edges {
                fan = fan.with_child(label.clone(), target(id));
            }
            fans.push((ids, fan));
        }
        nodes.extend(seg.nodes);
        arcs.extend(
            seg.arcs
                .into_iter()
                .filter(|(id, _)| !fan_edges.contains(id)),
        );
    }
    let root = root.ok_or(SegmentError::NotEmbedded {
        elem: ViewRef::Node(0),
    })?;
    let schema_root = nodes
        .get(&root)
        .map(|n| n.anchor.clone())
        .unwrap_or_default();
    fans.sort_unstable_by_key(|(ids, _)| ids.parent_edge);
    let floor = |max: Option<(u32, SegmentId)>, kind| match max {
        None => Ok(0),
        Some((id, segment)) => id
            .checked_add(1)
            .ok_or(SegmentError::AllocationExhausted { segment, kind }),
    };
    let embedding = TreeEmbedding {
        arcs: arcs
            .iter()
            .map(|(id, (_, child, _))| (*child, *id))
            .collect(),
        fans: fans.iter().map(|(ids, _)| ids.clone()).collect(),
        next_node: floor(max_node, super::ids::IdKind::Node)?,
        next_edge: floor(max_edge, super::ids::IdKind::Edge)?,
    };
    let tree = WInstance::new(
        nodes,
        arcs.into_values().collect(),
        fans.into_iter().map(|(_, fan)| fan).collect(),
        root,
        schema_root,
    );
    Ok((tree, embedding))
}
