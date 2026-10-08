//! Tests of segmented instances, and the generators they share.
//!
//! The generators cover the three instance shapes and every kind of
//! partition the segment laws quantify over: one segment, one segment per
//! node, random contiguous ranges, and interleaved ranges that put most
//! edges across a boundary.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_schema::{Edge, InstanceShapeKind};
use proptest::prelude::*;

use super::{IdRange, Partition, SegmentId, SegmentSpec};
use crate::fan::Fan;
use crate::functor::{FInstance, Row};
use crate::ginstance::{EdgeId, GEdge, GInstance, TreeEmbedding};
use crate::metadata::Node;
use crate::value::{FieldPresence, Value};
use crate::view_ref::RowId;
use crate::wtype::WInstance;

mod bytes;
mod errors;
mod fixtures;
mod laws;
mod manifest;
mod partition;
mod segments;
mod validation;

/// The result of a test that reports its failures as errors.
pub(super) type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A schema edge between two anchors.
pub(super) fn label(src: &str, tgt: &str, kind: &str, name: Option<&str>) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: kind.into(),
        name: name.map(Into::into),
    }
}

/// A small value.
pub(super) fn arb_value() -> impl Strategy<Value = Value> {
    prop_oneof![
        any::<i64>().prop_map(Value::Int),
        "[a-z]{0,4}".prop_map(Value::Str),
        any::<bool>().prop_map(Value::Bool),
        Just(Value::Null),
        any::<f64>().prop_map(Value::Float),
    ]
}

/// A node with id `id` and one of `anchors`, carrying a value, extra
/// fields, annotations and a position some of the time.
pub(super) fn arb_node(id: u32, anchors: Vec<&'static str>) -> impl Strategy<Value = Node> {
    (
        prop::sample::select(anchors),
        prop::option::of(arb_value()),
        prop::collection::hash_map("[a-z]{1,3}", arb_value(), 0..3),
        prop::collection::hash_map("[a-z]{1,3}", arb_value(), 0..2),
        prop::option::of(0u32..4),
    )
        .prop_map(move |(anchor, value, extra, annotations, position)| {
            let mut node = Node::new(id, anchor);
            node.value = value.map(FieldPresence::Present);
            node.extra_fields = extra;
            node.annotations = annotations;
            node.position = position;
            node
        })
}

/// A graph over up to nine nodes with sparse ids, with up to fourteen edges
/// under sparse edge ids: self-loops, parallel edges and cycles included.
pub(super) fn arb_ginstance() -> impl Strategy<Value = GInstance> {
    (
        prop::collection::btree_set(0u32..40, 1..9),
        prop::collection::btree_set(0u32..60, 0..14),
    )
        .prop_flat_map(|(ids, edge_ids)| {
            let ids: Vec<u32> = ids.into_iter().collect();
            let nodes: Vec<_> = ids
                .iter()
                .map(|id| arb_node(*id, vec!["a", "b", "c"]))
                .collect();
            let n = ids.len();
            let edges = prop::collection::vec(
                (
                    0..n,
                    0..n,
                    prop::sample::select(vec!["x", "y"]),
                    prop::option::of(0u32..3),
                ),
                edge_ids.len(),
            );
            (Just(edge_ids), nodes, edges)
        })
        .prop_map(|(edge_ids, nodes, edges)| {
            let mut graph = GInstance::new();
            let ids: Vec<u32> = nodes.iter().map(|n| n.id).collect();
            let anchors: Vec<Name> = nodes.iter().map(|n| n.anchor.clone()).collect();
            for node in nodes {
                graph = graph.with_node(node);
            }
            for (id, (s, t, name, position)) in edge_ids.into_iter().zip(edges) {
                let edge = GEdge {
                    src: ids[s],
                    tgt: ids[t],
                    edge: label(&anchors[s], &anchors[t], "prop", Some(name)),
                    position,
                };
                graph.insert_edge(EdgeId(id), edge).unwrap_or_else(|_| {
                    unreachable!("edge ids are distinct and endpoints present")
                });
            }
            graph
        })
}

/// The cells of a row.
fn arb_row() -> impl Strategy<Value = Row> {
    prop::collection::hash_map("[a-c]", arb_value(), 0..3)
}

/// Tables `a` and `b` over up to nine rows with sparse ids, with foreign keys
/// `a -> b`, `b -> a` and `a -> a`.
pub(super) fn arb_finstance() -> impl Strategy<Value = FInstance> {
    prop::collection::btree_set(0u32..40, 0..9)
        .prop_flat_map(|ids| {
            let ids: Vec<u32> = ids.into_iter().collect();
            let rows: Vec<_> = ids
                .iter()
                .map(|id| (Just(*id), any::<bool>(), arb_row()))
                .collect();
            let n = ids.len().max(1);
            let pairs = prop::collection::vec((0usize..3, 0..n, 0..n), 0..12);
            (rows, pairs)
        })
        .prop_map(|(rows, pairs)| {
            let mut tables: BTreeMap<Name, Vec<(RowId, Row)>> = BTreeMap::new();
            let mut table_of: Vec<(RowId, &'static str)> = Vec::new();
            for (id, in_a, row) in rows {
                let table = if in_a { "a" } else { "b" };
                tables
                    .entry(table.into())
                    .or_default()
                    .push((RowId(id), row));
                table_of.push((RowId(id), table));
            }
            let keys = [
                label("a", "b", "fk", Some("ab")),
                label("b", "a", "fk", Some("ba")),
                label("a", "a", "fk", Some("aa")),
            ];
            let mut set: BTreeMap<Edge, BTreeSet<(RowId, RowId)>> = BTreeMap::new();
            for (k, s, t) in pairs {
                let (Some(src), Some(tgt)) = (table_of.get(s), table_of.get(t)) else {
                    continue;
                };
                let key = &keys[k];
                if key.src.as_ref() == src.1 && key.tgt.as_ref() == tgt.1 {
                    set.entry(key.clone()).or_default().insert((src.0, tgt.0));
                }
            }
            FInstance::from_parts(
                tables,
                set.into_iter().map(|(e, p)| (e, p.into_iter().collect())),
            )
            .unwrap_or_else(|_| unreachable!("rows are distinct and pairs typed"))
        })
}

/// A tree over nodes `0..n` (node `i > 0` hangs under some node `< i`)
/// whose schema root is its root's anchor, its arcs in a shuffled order,
/// with fans over its nodes, some repeated; and its graph embedding.
pub(super) fn arb_winstance() -> impl Strategy<Value = (WInstance, TreeEmbedding)> {
    (1u32..9)
        .prop_flat_map(|n| {
            let nodes: Vec<_> = (0..n)
                .map(|id| arb_node(id, vec!["doc", "para", "word"]))
                .collect();
            let parents: Vec<_> = (1..n).map(|i| 0..i).collect();
            let fans = prop::collection::vec(
                (
                    prop::sample::select(vec!["rel", "pair"]),
                    0..n,
                    prop::collection::hash_map("[a-c]", 0..n, 0..3),
                ),
                0..4,
            );
            (nodes, parents, fans, any::<bool>())
        })
        .prop_flat_map(|(nodes, parents, fans, repeat)| {
            let arcs: Vec<(u32, u32, Edge)> = parents
                .iter()
                .zip(1u32..)
                .map(|(&parent, child)| {
                    let name = format!("c{child}");
                    let edge = label(
                        &nodes[parent as usize].anchor,
                        &nodes[child as usize].anchor,
                        "prop",
                        Some(&name),
                    );
                    (parent, child, edge)
                })
                .collect();
            let mut fans: Vec<Fan> = fans
                .into_iter()
                .map(|(he, parent, children)| Fan {
                    hyper_edge_id: he.to_owned(),
                    parent,
                    children,
                })
                .collect();
            if repeat && let Some(first) = fans.first().cloned() {
                fans.push(first);
            }
            (Just(nodes), Just(arcs).prop_shuffle(), Just(fans))
        })
        .prop_map(|(nodes, arcs, fans)| {
            let schema_root = nodes[0].anchor.clone();
            let nodes: HashMap<u32, Node> = nodes.into_iter().map(|n| (n.id, n)).collect();
            let tree = WInstance::new(nodes, arcs, fans, 0, schema_root);
            let (_, embedding) = GInstance::from_wtype_embedded(&tree, None)
                .unwrap_or_else(|_| unreachable!("a generated tree embeds"));
            (tree, embedding)
        })
}

/// One past the largest id an instance uses, node or edge: allocation
/// ranges start here.
pub(super) fn next_free(max_node: Option<u32>, max_edge: Option<u32>) -> u32 {
    max_node.max(max_edge).map_or(0, |m| m + 1)
}

/// A partition of `shape` over the ids below `free`, of one of four kinds:
/// one segment; one segment per id; random contiguous ranges; interleaved
/// ids (`id mod k`). Each segment allocates from its own range above
/// `free` when `alloc` is not zero.
pub(super) fn arb_partition(
    shape: InstanceShapeKind,
    free: u32,
) -> impl Strategy<Value = Partition> {
    let top = free.max(1);
    (
        0u8..4,
        prop::collection::btree_set(1..top.max(2), 0..6),
        2u32..5,
        prop_oneof![Just(0u32), 1u32..4],
    )
        .prop_map(move |(kind, cuts, k, alloc)| match kind {
            0 => Partition::single(shape, free.checked_sub(1)),
            1 => {
                let bounds: Vec<u32> = (1..top).collect();
                Partition::by_node_ranges(shape, &bounds, alloc, top)
                    .unwrap_or_else(|_| unreachable!("ascending bounds"))
            }
            2 => {
                let bounds: Vec<u32> = cuts.into_iter().filter(|c| *c < top).collect();
                Partition::by_node_ranges(shape, &bounds, alloc, top)
                    .unwrap_or_else(|_| unreachable!("ascending bounds"))
            }
            _ => interleaved(shape, top, k, alloc),
        })
}

/// The partition putting id `x < top` in segment `x mod k`, with each
/// segment allocating `alloc` ids from consecutive ranges at `top`.
pub(super) fn interleaved(shape: InstanceShapeKind, top: u32, k: u32, alloc: u32) -> Partition {
    let specs = (0..k)
        .map(|s| {
            let mut nodes: Vec<IdRange> = (s..top)
                .step_by(k as usize)
                .map(|x| IdRange { lo: x, hi: x })
                .collect();
            let range = (alloc > 0).then(|| IdRange {
                lo: top + s * alloc,
                hi: top + (s + 1) * alloc - 1,
            });
            nodes.extend(range);
            SegmentSpec {
                id: SegmentId(s),
                nodes,
                alloc_nodes: range,
                alloc_edges: range.filter(|_| shape != InstanceShapeKind::Table),
            }
        })
        .collect();
    Partition::new(shape, specs)
}

/// Whether two trees are equal: the same canonical bytes (nodes, arcs in
/// order, fans), root and schema root, and fans in the same order.
pub(super) fn same_tree(a: &WInstance, b: &WInstance) -> bool {
    let fans = |t: &WInstance| t.fans.iter().map(super::FanRecord::of).collect::<Vec<_>>();
    super::canonical_winstance_bytes(a) == super::canonical_winstance_bytes(b)
        && a.root == b.root
        && a.schema_root == b.schema_root
        && fans(a) == fans(b)
}
