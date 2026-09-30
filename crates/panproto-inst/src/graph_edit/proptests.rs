//! Property tests of the graph delta algebra and its laws.

use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

use panproto_schema::Edge;

use super::*;
use crate::ginstance::{EdgeId, GEdge, GInstance};
use crate::metadata::Node;
use crate::value::{FieldPresence, Value};

const ANCHORS: [&str; 2] = ["a", "b"];
const KINDS: [&str; 2] = ["k", "l"];

fn label(kind: usize) -> Edge {
    Edge {
        src: "a".into(),
        tgt: "b".into(),
        kind: KINDS[kind % 2].into(),
        name: None,
    }
}

/// A node's generated data: anchor, value, field `x`, field `y`, position.
type NodeSpec = (usize, Option<i64>, Option<i64>, Option<i64>, Option<u32>);

fn node_from(id: u32, (anchor, value, x, y, position): NodeSpec) -> Node {
    let mut node = Node::new(id, ANCHORS[anchor % 2]);
    node.value = value.map(|v| {
        if v == 0 {
            FieldPresence::Null
        } else {
            FieldPresence::Present(Value::Int(v))
        }
    });
    if let Some(x) = x {
        node.extra_fields.insert("x".into(), Value::Int(x));
    }
    if let Some(y) = y {
        node.extra_fields.insert("y".into(), Value::Int(y));
    }
    node.position = position;
    node
}

fn node_spec() -> impl Strategy<Value = NodeSpec> {
    (
        0usize..2,
        prop::option::of(0i64..3),
        prop::option::of(0i64..3),
        prop::option::of(0i64..2),
        prop::option::of(0u32..2),
    )
}

/// An edge's generated data: id, source and target (indices into the node
/// list), label, position.
type EdgeSpec = (u32, usize, usize, usize, Option<u32>);

fn edge_spec() -> impl Strategy<Value = EdgeSpec> {
    (
        0u32..8,
        0usize..8,
        0usize..8,
        0usize..2,
        prop::option::of(0u32..3),
    )
}

fn graph_from(nodes: &std::collections::BTreeMap<u32, NodeSpec>, edges: &[EdgeSpec]) -> GInstance {
    let mut graph = GInstance::new();
    let ids: Vec<u32> = nodes.keys().copied().collect();
    for (id, spec) in nodes {
        graph = graph.with_node(node_from(*id, *spec));
    }
    if ids.is_empty() {
        return graph;
    }
    for (id, src, tgt, kind, position) in edges {
        let edge = GEdge::new(ids[src % ids.len()], ids[tgt % ids.len()], label(*kind))
            .with_position(*position);
        // A repeated id keeps its first edge.
        let _ = graph.insert_edge(EdgeId(*id), edge);
    }
    graph
}

fn arb_graph() -> impl Strategy<Value = GInstance> {
    (
        prop::collection::btree_map(0u32..6, node_spec(), 0..6),
        prop::collection::vec(edge_spec(), 0..8),
    )
        .prop_map(|(nodes, edges)| graph_from(&nodes, &edges))
}

/// One mutation of a graph: an operation code and its parameters.
type Op = (u8, usize, usize, usize, i64);

fn arb_ops() -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec((0u8..10, 0usize..16, 0usize..16, 0usize..2, 0i64..3), 0..4)
}

/// Apply mutations directly through the instance's own methods, so the
/// target state does not depend on the code under test.
fn mutate(graph: &GInstance, ops: &[Op]) -> GInstance {
    let mut g = graph.clone();
    for &(code, i, j, k, v) in ops {
        let mut nodes: Vec<u32> = g.nodes().keys().copied().collect();
        nodes.sort_unstable();
        let edges: Vec<EdgeId> = g.edges_sorted().map(|(id, _)| id).collect();
        let node = (!nodes.is_empty()).then(|| nodes[i % nodes.len()]);
        let other = (!nodes.is_empty()).then(|| nodes[j % nodes.len()]);
        let edge = (!edges.is_empty()).then(|| edges[i % edges.len()]);
        match code {
            0 => {
                if let Some(n) = node {
                    let mut incident: Vec<EdgeId> = g
                        .out_edges(n)
                        .iter()
                        .chain(g.in_edges(n))
                        .copied()
                        .collect();
                    incident.sort_unstable();
                    incident.dedup();
                    for e in incident {
                        let _ = g.remove_edge(e);
                    }
                    let _ = g.remove_node(n);
                }
            }
            1 => {
                let id = 6 + u32::try_from(i % 4).unwrap_or(0);
                let _ = g.insert_node(node_from(id, (k, Some(v), None, None, None)));
            }
            2 => {
                if let Some(n) = node.and_then(|n| g.node_mut(n)) {
                    n.extra_fields
                        .insert(if k == 0 { "x" } else { "y" }.into(), Value::Int(v));
                }
            }
            3 => {
                if let Some(n) = node.and_then(|n| g.node_mut(n)) {
                    n.anchor = ANCHORS[k].into();
                }
            }
            4 => {
                if let (Some(s), Some(t)) = (node, other) {
                    let id = 8 + u32::try_from(j % 4).unwrap_or(0);
                    let _ = g.insert_edge(EdgeId(id), GEdge::new(s, t, label(k)));
                }
            }
            5 => {
                if let (Some(e), Some(s), Some(t)) = (edge, node, other) {
                    let _ = g.set_endpoints(e, t, s);
                }
            }
            6 => {
                if let Some(e) = edge {
                    let _ = g.relabel_edge(e, label(k));
                }
            }
            7 => {
                if let Some(e) = edge {
                    let _ = g.set_edge_position(e, u32::try_from(v).ok());
                }
            }
            8 => {
                if let Some(n) = node.and_then(|n| g.node_mut(n)) {
                    n.extra_fields.remove("x");
                    n.value = None;
                    n.position = Some(u32::try_from(v).unwrap_or(0));
                }
            }
            _ => {
                if let Some(e) = edge {
                    let _ = g.remove_edge(e);
                }
            }
        }
    }
    g
}

fn fail(msg: impl std::fmt::Display) -> TestCaseError {
    TestCaseError::fail(msg.to_string())
}

/// Whether a delta is in normal form.
fn is_normal(delta: &GraphDelta) -> bool {
    delta.attrs().iter().all(|(key, change)| {
        let structural = match key.element() {
            ElemRef::Node(n) => {
                delta.del_nodes().contains_key(&n) || delta.add_nodes().contains_key(&n)
            }
            ElemRef::Edge(e) => {
                delta.del_edges().contains_key(&e) || delta.add_edges().contains_key(&e)
            }
            _ => true,
        };
        !structural && !key.is_table() && change.old != change.new
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn every_constructed_graph_delta_is_normal(a in arb_graph(), b in arb_graph(), ops in arb_ops()) {
        let d = GraphDelta::diff(&a, &b);
        prop_assert!(is_normal(&d));
        let e = GraphDelta::diff(&a, &mutate(&a, &ops));
        prop_assert!(is_normal(&e.invert()));
        if let Ok(c) = d.compose(&e) {
            prop_assert!(is_normal(&c));
        }
        if let Ok(m) = d.merge(&e) {
            prop_assert!(is_normal(&m));
        }
        // Raw parts with attribution changes on deleted and added elements
        // and unchanged values normalize to a normal delta.
        let mut parts = d.to_parts();
        for key in e.attrs().keys() {
            parts.attrs.insert(key.clone(), AttrChange {
                old: e.attrs()[key].old.clone(),
                new: e.attrs()[key].old.clone(),
            });
        }
        parts.attrs.extend(e.attrs().iter().map(|(k, c)| (k.clone(), c.clone())));
        let normal = GraphDelta::normalize(parts).map_err(fail)?;
        prop_assert!(is_normal(&normal));
        let decoded: GraphDelta = serde_json::from_str(&serde_json::to_string(&normal).map_err(fail)?).map_err(fail)?;
        prop_assert!(is_normal(&decoded));
        prop_assert_eq!(decoded, normal);
    }

    #[test]
    fn a_failing_graph_delta_leaves_the_instance_unchanged(a in arb_graph(), b in arb_graph(), c in arb_graph()) {
        // A delta between two unrelated graphs usually does not apply to a
        // third; whenever it fails, the third is unchanged.
        let d = GraphDelta::diff(&a, &b);
        let mut target = c.clone();
        if d.apply(&mut target).is_err() {
            prop_assert!(GInstance::identical(&target, &c));
        }
    }

    #[test]
    fn check_agrees_with_apply(a in arb_graph(), b in arb_graph(), c in arb_graph()) {
        for (d, s) in [(GraphDelta::diff(&a, &b), c), (GraphDelta::diff(&a, &b), a.clone())] {
            let mut target = s.clone();
            prop_assert_eq!(d.check(&s), d.apply(&mut target));
            prop_assert!(GInstance::identical(&s, &s));
        }
    }

    #[test]
    fn graph_delta_invert_is_an_involution(a in arb_graph(), b in arb_graph()) {
        let d = GraphDelta::diff(&a, &b);
        prop_assert!(d.invert().invert() == d);
        // Apply then apply the inverse restores the instance.
        let mut s = a.clone();
        d.apply(&mut s).map_err(fail)?;
        prop_assert!(GInstance::identical(&s, &b));
        d.invert().apply(&mut s).map_err(fail)?;
        prop_assert!(GInstance::identical(&s, &a));
    }

    #[test]
    fn graph_delta_identity_is_a_unit(a in arb_graph(), b in arb_graph()) {
        let d = GraphDelta::diff(&a, &b);
        prop_assert!(GraphDelta::identity().compose(&d).map_err(fail)? == d);
        prop_assert_eq!(d.compose(&GraphDelta::identity()).map_err(fail)?, d);
    }

    #[test]
    fn graph_delta_compose_is_associative(
        g in arb_graph(), o1 in arb_ops(), o2 in arb_ops(), o3 in arb_ops(), x in arb_graph(), y in arb_graph(),
    ) {
        let g1 = mutate(&g, &o1);
        let g2 = mutate(&g1, &o2);
        let g3 = mutate(&g2, &o3);
        let (a, b, c) = (GraphDelta::diff(&g, &g1), GraphDelta::diff(&g1, &g2), GraphDelta::diff(&g2, &g3));
        let left = a.compose(&b).and_then(|ab| ab.compose(&c)).map_err(fail)?;
        let right = b.compose(&c).and_then(|bc| a.compose(&bc)).map_err(fail)?;
        prop_assert_eq!(&left, &right);
        let mut s = g;
        left.apply(&mut s).map_err(fail)?;
        prop_assert!(GInstance::identical(&s, &g3));
        // On unrelated deltas, whenever both sides are defined they agree.
        let z = GraphDelta::diff(&x, &y);
        if let (Ok(l), Ok(r)) = (a.compose(&z).and_then(|az| az.compose(&c)), z.compose(&c).and_then(|zc| a.compose(&zc))) {
            prop_assert_eq!(l, r);
        }
    }

    #[test]
    fn graph_delta_invert_reverses_compose(g in arb_graph(), o1 in arb_ops(), o2 in arb_ops(), x in arb_graph(), y in arb_graph()) {
        let g1 = mutate(&g, &o1);
        let g2 = mutate(&g1, &o2);
        let (a, b) = (GraphDelta::diff(&g, &g1), GraphDelta::diff(&g1, &g2));
        let forward = a.compose(&b).map_err(fail)?;
        let backward = b.invert().compose(&a.invert()).map_err(fail)?;
        prop_assert_eq!(forward.invert(), backward);
        let z = GraphDelta::diff(&x, &y);
        if let Ok(az) = a.compose(&z) {
            let za = z.invert().compose(&a.invert()).map_err(fail)?;
            prop_assert_eq!(az.invert(), za);
        }
    }

    #[test]
    fn touched_ids_cover_every_mentioned_id(a in arb_graph(), b in arb_graph()) {
        let d = GraphDelta::diff(&a, &b);
        let nodes = d.touched_nodes();
        let edges = d.touched_edges();
        for id in d.del_nodes().keys().chain(d.add_nodes().keys()) {
            prop_assert!(nodes.contains(id));
        }
        for (id, e) in d.del_edges().iter().chain(d.add_edges()) {
            prop_assert!(edges.contains(id));
            prop_assert!(nodes.contains(&e.src) && nodes.contains(&e.tgt));
        }
        for key in d.attrs().keys() {
            match key.element() {
                ElemRef::Node(n) => prop_assert!(nodes.contains(&n)),
                ElemRef::Edge(e) => prop_assert!(edges.contains(&e)),
                other => return Err(fail(format!("graph delta names {other:?}"))),
            }
        }
        let json = serde_json::to_string(&d).map_err(fail)?;
        prop_assert_eq!(nodes.is_empty() && edges.is_empty(), d.is_identity(), "{}", json);
    }

    #[test]
    fn independent_graph_deltas_commute_and_merge(s in arb_graph(), o1 in arb_ops(), o2 in arb_ops()) {
        let a = GraphDelta::diff(&s, &mutate(&s, &o1));
        let b = GraphDelta::diff(&s, &mutate(&s, &o2));
        if a.independence(&b).is_empty() {
            let mut ab = s.clone();
            a.apply(&mut ab).map_err(fail)?;
            b.apply(&mut ab).map_err(fail)?;
            let mut ba = s.clone();
            b.apply(&mut ba).map_err(fail)?;
            a.apply(&mut ba).map_err(fail)?;
            let mut merged = s;
            a.merge(&b).map_err(fail)?.apply(&mut merged).map_err(fail)?;
            prop_assert!(GInstance::identical(&ab, &ba));
            prop_assert!(GInstance::identical(&ab, &merged));
        } else {
            prop_assert_eq!(a.independence(&b), b.independence(&a));
        }
    }

    #[test]
    fn diff_emits_no_same_payload_replacement(a in arb_graph(), b in arb_graph()) {
        let d = GraphDelta::diff(&a, &b);
        prop_assert!(d.del_nodes().keys().all(|n| !d.add_nodes().contains_key(n)));
        for (id, old) in d.del_edges() {
            if let Some(new) = d.add_edges().get(id) {
                prop_assert!((old.src, old.tgt) != (new.src, new.tgt));
            }
        }
    }

    #[test]
    fn graph_differ_laws(a in arb_graph(), b in arb_graph()) {
        let mut s = a.clone();
        GraphDelta::diff(&a, &b).apply(&mut s).map_err(fail)?;
        prop_assert!(GInstance::identical(&s, &b));
        prop_assert!(GraphDelta::diff(&a, &a).is_identity());
    }

    #[test]
    fn graph_edit_apply_is_delta_apply(s in arb_graph(), o1 in arb_ops(), o2 in arb_ops(), x in arb_graph()) {
        let d1 = GraphDelta::diff(&s, &mutate(&s, &o1));
        let d2 = GraphDelta::diff(&x, &mutate(&x, &o2));
        let mut edges: Vec<(EdgeId, u32, u32)> = s.edges_sorted().map(|(id, e)| (id, e.src, e.tgt)).collect();
        edges.truncate(1);
        let mut steps = vec![GraphEdit::Delta(d1), GraphEdit::Delta(d2)];
        steps.extend(edges.into_iter().map(|(id, src, tgt)| GraphEdit::Reconnect { id, old: (src, tgt), new: (tgt, src) }));
        for edit in [GraphEdit::Batch(steps.clone()), GraphEdit::Batch(steps.into_iter().rev().collect())] {
            let mut by_edit = s.clone();
            let mut by_delta = s.clone();
            let via_edit = edit.apply(&mut by_edit);
            match edit.to_delta_in(&s) {
                Ok(delta) => {
                    let via_delta = delta.apply(&mut by_delta);
                    prop_assert_eq!(via_edit.is_ok(), via_delta.is_ok());
                    prop_assert!(GInstance::identical(&by_edit, &by_delta));
                }
                Err(_) => prop_assert!(via_edit.is_err() && GInstance::identical(&by_edit, &s)),
            }
        }
    }
}

/// The independence law is exercised: the generator yields independent
/// pairs of non-identity deltas, and dependent pairs, in useful numbers.
#[test]
fn the_independence_generator_covers_both_outcomes() -> Result<(), TestCaseError> {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    let mut runner = TestRunner::deterministic();
    let strategy = (arb_graph(), arb_ops(), arb_ops());
    let (mut independent, mut dependent) = (0, 0);
    for _ in 0..512 {
        let (s, o1, o2) = strategy.new_tree(&mut runner).map_err(fail)?.current();
        let a = GraphDelta::diff(&s, &mutate(&s, &o1));
        let b = GraphDelta::diff(&s, &mutate(&s, &o2));
        if a.is_identity() || b.is_identity() {
            continue;
        }
        if a.independence(&b).is_empty() {
            independent += 1;
        } else {
            dependent += 1;
        }
    }
    prop_assert!(independent >= 50, "only {} independent pairs", independent);
    prop_assert!(dependent >= 50, "only {} dependent pairs", dependent);
    Ok(())
}
