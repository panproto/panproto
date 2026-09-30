//! Property tests of the graph delta algebra and its laws.

use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

use panproto_schema::Edge;

use super::*;
use crate::edit_error::EditError;
use crate::ginstance::{EdgeId, FanDomain, GEdge, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND};
use crate::metadata::{Node, NodeShape};
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

const FAN_LABELS: [&str; 3] = ["x", "y", "z"];

/// A reserved-kind label: `hyper_parent` when `name` is `None`, else
/// `hyper_label` with that name.
fn fan_edge(name: Option<&str>) -> Edge {
    Edge {
        src: "f".into(),
        tgt: "a".into(),
        kind: if name.is_some() {
            HYPER_LABEL_KIND
        } else {
            HYPER_PARENT_KIND
        }
        .into(),
        name: name.map(Into::into),
    }
}

/// One fan's generated data: its parent's index into the regular nodes, and
/// for each of [`FAN_LABELS`] the index of the labelled child, if any.
type FanSpec = (usize, [Option<usize>; 3]);

/// A graph in the fan domain: [`arb_graph`]'s regular nodes (at least one)
/// and edges, plus up to two fan nodes 20 and 21 (shape `HyperEdge`, anchor
/// `f`), each with one `hyper_parent` edge (id 20 or 21) to a regular node
/// and `hyper_label` edges with distinct names (ids 24 up) to regular nodes.
fn arb_fan_graph() -> impl Strategy<Value = GInstance> {
    (
        prop::collection::btree_map(0u32..6, node_spec(), 1..6),
        prop::collection::vec(edge_spec(), 0..6),
        prop::collection::vec(
            (
                0usize..8,
                [
                    prop::option::of(0usize..8),
                    prop::option::of(0usize..8),
                    prop::option::of(0usize..8),
                ],
            ),
            0..3,
        ),
    )
        .prop_map(|(nodes, edges, fans): (_, _, Vec<FanSpec>)| {
            let mut graph = graph_from(&nodes, &edges);
            let ids: Vec<u32> = nodes.keys().copied().collect();
            let mut next = 24u32;
            for (f, (parent, children)) in (0u32..).zip(fans) {
                let fan = 20 + f;
                graph = graph
                    .with_node(Node::new(fan, "f").with_shape(NodeShape::HyperEdge))
                    .with_edge_id(EdgeId(fan), fan, ids[parent % ids.len()], fan_edge(None));
                for (name, child) in FAN_LABELS.iter().zip(children) {
                    if let Some(child) = child {
                        graph = graph.with_edge_id(
                            EdgeId(next),
                            fan,
                            ids[child % ids.len()],
                            fan_edge(Some(name)),
                        );
                        next += 1;
                    }
                }
            }
            graph
        })
}

/// Mutations weighted towards the fan operations 10 to 13 of [`mutate`],
/// so that two deltas often change one fan.
fn arb_fan_ops() -> impl Strategy<Value = Vec<Op>> {
    let code = prop_oneof![4 => 10u8..11, 2 => 11u8..14, 1 => Just(9u8), 3 => 0u8..9];
    prop::collection::vec((code, 0usize..16, 0usize..16, 0usize..2, 0i64..3), 0..4)
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
            10..=13 => mutate_fan(&mut g, &nodes, &edges, (code, i, j, k, v)),
            _ => {
                if let Some(e) = edge {
                    let _ = g.remove_edge(e);
                }
            }
        }
    }
    g
}

/// The fan operations 10 to 13 of [`mutate`] on `graph`, whose sorted node and
/// edge ids are `nodes` and `edges`.
fn mutate_fan(graph: &mut GInstance, nodes: &[u32], edges: &[EdgeId], (code, i, j, k, v): Op) {
    let node = (!nodes.is_empty()).then(|| nodes[i % nodes.len()]);
    let other = (!nodes.is_empty()).then(|| nodes[j % nodes.len()]);
    match code {
        10 => {
            // A labelled edge from a node of the fan shape.
            let fans: Vec<u32> = nodes
                .iter()
                .copied()
                .filter(|n| {
                    graph
                        .node(*n)
                        .is_some_and(|n| n.shape == NodeShape::HyperEdge)
                })
                .collect();
            let plain: Vec<u32> = nodes
                .iter()
                .copied()
                .filter(|n| !fans.contains(n))
                .collect();
            if !fans.is_empty() && !plain.is_empty() {
                let id = 40 + u32::try_from(j % 4).unwrap_or(0);
                let name = FAN_LABELS[usize::try_from(v).unwrap_or(0) % 3];
                let _ = graph.insert_edge(
                    EdgeId(id),
                    GEdge::new(
                        fans[i % fans.len()],
                        plain[j % plain.len()],
                        fan_edge(Some(name)),
                    ),
                );
            }
        }
        11 => {
            if let Some(n) = node.and_then(|n| graph.node_mut(n)) {
                n.shape = if n.shape == NodeShape::HyperEdge {
                    NodeShape::default()
                } else {
                    NodeShape::HyperEdge
                };
            }
        }
        12 => {
            // Rename a reserved-kind edge's label, or make it a parent edge.
            let reserved: Vec<EdgeId> = edges
                .iter()
                .copied()
                .filter(|e| {
                    graph.edge(*e).is_some_and(|e| {
                        e.edge.kind == HYPER_LABEL_KIND || e.edge.kind == HYPER_PARENT_KIND
                    })
                })
                .collect();
            if !reserved.is_empty() {
                let name = (k == 0).then(|| FAN_LABELS[usize::try_from(v).unwrap_or(0) % 3]);
                let _ = graph.relabel_edge(reserved[j % reserved.len()], fan_edge(name));
            }
        }
        13 => {
            if let (Some(s), Some(t)) = (node, other) {
                let id = 44 + u32::try_from(j % 4).unwrap_or(0);
                let _ = graph.insert_edge(EdgeId(id), GEdge::new(s, t, fan_edge(None)));
            }
        }
        _ => {}
    }
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

/// The independence law at `s`: when `a` and `b` are independent, applying
/// one after the other in either order, and applying their merge, succeed
/// and agree under `identical`; otherwise the conflicts do not depend on the
/// argument order.
fn independence_law(s: &GInstance, a: &GraphDelta, b: &GraphDelta) -> Result<(), TestCaseError> {
    if a.independence(b).is_empty() {
        let mut ab = s.clone();
        a.apply(&mut ab).map_err(fail)?;
        b.apply(&mut ab).map_err(fail)?;
        let mut ba = s.clone();
        b.apply(&mut ba).map_err(fail)?;
        a.apply(&mut ba).map_err(fail)?;
        let mut merged = s.clone();
        a.merge(b).map_err(fail)?.apply(&mut merged).map_err(fail)?;
        prop_assert!(GInstance::identical(&ab, &ba));
        prop_assert!(GInstance::identical(&ab, &merged));
    } else {
        prop_assert_eq!(a.independence(b), b.independence(a));
    }
    Ok(())
}

/// What applying an [`ApplyCase`]'s delta to its state must do.
enum Expect {
    /// Nothing is required of the outcome.
    Unknown,
    /// It applies, with this result.
    Applies(Box<GInstance>),
    /// It fails the fan-domain check, the last check of
    /// [`GraphDelta::apply`], after mutating the state.
    FailsFan,
}

/// A delta, a state to apply it to, and what applying it must do.
struct ApplyCase {
    delta: GraphDelta,
    state: GInstance,
    expect: Expect,
}

impl std::fmt::Debug for ApplyCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let expect = match &self.expect {
            Expect::Unknown => "unknown",
            Expect::Applies(_) => "applies",
            Expect::FailsFan => "fails the fan check",
        };
        f.debug_struct("ApplyCase")
            .field("delta", &self.delta)
            .field("state", &self.state)
            .field("expect", &expect)
            .finish()
    }
}

/// The cases of the apply and check properties: a delta between two
/// unrelated graphs applied to a third (it usually fails a check of the
/// input), the same delta applied to its own source (it applies), and a
/// delta from a graph with fans to a mutation of it applied to that graph,
/// which fails exactly when the mutation leaves the fan domain.
fn apply_cases() -> impl Strategy<Value = Vec<ApplyCase>> {
    (
        arb_graph(),
        arb_graph(),
        arb_graph(),
        arb_fan_graph(),
        arb_fan_ops(),
    )
        .prop_map(|(a, b, c, s, ops)| {
            let d = GraphDelta::diff(&a, &b);
            let t = mutate(&s, &ops);
            let fan = if FanDomain::contains(&t) {
                Expect::Applies(Box::new(t.clone()))
            } else {
                Expect::FailsFan
            };
            vec![
                ApplyCase {
                    delta: d.clone(),
                    state: c,
                    expect: Expect::Unknown,
                },
                ApplyCase {
                    delta: d,
                    state: a,
                    expect: Expect::Applies(Box::new(b)),
                },
                ApplyCase {
                    delta: GraphDelta::diff(&s, &t),
                    state: s,
                    expect: fan,
                },
            ]
        })
}

/// Three deltas drawn from a pool that makes composable triples likely: two
/// consecutive deltas of one chain, their inverses (so that adds and
/// deletes cancel), a second delta from the chain's start, and a delta
/// between two unrelated graphs.
fn composable_triple_candidates() -> impl Strategy<Value = (GraphDelta, GraphDelta, GraphDelta)> {
    (
        arb_graph(),
        arb_ops(),
        arb_ops(),
        arb_ops(),
        arb_graph(),
        arb_graph(),
        [0usize..6, 0usize..6, 0usize..6],
    )
        .prop_map(|(g, o1, o2, o3, x, y, picks)| {
            let g1 = mutate(&g, &o1);
            let g2 = mutate(&g1, &o2);
            let first = GraphDelta::diff(&g, &g1);
            let second = GraphDelta::diff(&g1, &g2);
            let pool = [
                first.invert(),
                second.invert(),
                first,
                second,
                GraphDelta::diff(&g, &mutate(&g, &o3)),
                GraphDelta::diff(&x, &y),
            ];
            let [i, j, k] = picks;
            (pool[i].clone(), pool[j].clone(), pool[k].clone())
        })
}

/// Whether two instances allocate the same id to the next added edge.
fn allocates_alike(x: &GInstance, y: &GInstance) -> bool {
    let Some(&n) = x.nodes().keys().min() else {
        return y.nodes().is_empty();
    };
    let (mut x, mut y) = (x.clone(), y.clone());
    x.add_edge(n, n, label(0)).ok() == y.add_edge(n, n, label(0)).ok()
}

/// 256 cases, and no failure-persistence file: a shrunk failure is kept as a
/// named unit test in this module instead, so running the properties writes
/// nothing outside the source tree's test modules.
fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 256,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

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
    fn a_failing_graph_delta_leaves_the_instance_unchanged(cases in apply_cases()) {
        for case in cases {
            let mut target = case.state.clone();
            match (case.delta.apply(&mut target), &case.expect) {
                (Err(err), expect) => {
                    prop_assert!(GInstance::identical(&target, &case.state), "{}", err);
                    prop_assert!(allocates_alike(&target, &case.state), "{}", err);
                    if matches!(expect, Expect::FailsFan) {
                        prop_assert!(matches!(err, EditError::MalformedFan { .. }), "{}", err);
                    } else {
                        prop_assert!(matches!(expect, Expect::Unknown), "{}", err);
                    }
                }
                (Ok(()), Expect::Applies(result)) => {
                    prop_assert!(GInstance::identical(&target, result));
                }
                (Ok(()), Expect::Unknown) => {}
                (Ok(()), Expect::FailsFan) => {
                    return Err(fail("a delta to a graph outside the fan domain applied"));
                }
            }
        }
    }

    #[test]
    fn check_agrees_with_apply(cases in apply_cases()) {
        for case in cases {
            let mut target = case.state.clone();
            let applied = case.delta.apply(&mut target);
            prop_assert_eq!(case.delta.check(&case.state), applied.clone());
            match (applied, &case.expect) {
                (Err(_), _) => prop_assert!(GInstance::identical(&target, &case.state)),
                (Ok(()), Expect::Applies(result)) => {
                    prop_assert!(GInstance::identical(&target, result));
                }
                (Ok(()), _) => {}
            }
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
    fn graph_delta_compose_is_associative_on_composable_triples(
        triple in composable_triple_candidates(),
    ) {
        let (a, b, c) = triple;
        if let (Ok(ab), Ok(bc)) = (a.compose(&b), b.compose(&c)) {
            match (ab.compose(&c), a.compose(&bc)) {
                (Ok(left), Ok(right)) => prop_assert_eq!(left, right),
                (Err(_), Err(_)) => {}
                (left, right) => {
                    return Err(fail(format!("one bracketing only is defined: {left:?} vs {right:?}")));
                }
            }
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
        independence_law(&s, &a, &b)?;
    }

    #[test]
    fn independent_graph_deltas_commute_and_merge_around_fans(s in arb_fan_graph(), o1 in arb_fan_ops(), o2 in arb_fan_ops()) {
        prop_assert!(FanDomain::contains(&s));
        let (t1, t2) = (mutate(&s, &o1), mutate(&s, &o2));
        // The law is over deltas that apply to `s`: both targets must be in
        // the fan domain.
        if FanDomain::contains(&t1) && FanDomain::contains(&t2) {
            independence_law(&s, &GraphDelta::diff(&s, &t1), &GraphDelta::diff(&s, &t2))?;
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
        let mut steps = vec![GraphEdit::Delta(d1.clone()), GraphEdit::Delta(d2)];
        // A reconnect of the first edge, as the constructor's step and as a
        // bare `Reconnect`, which does not lower without the instance.
        if let Some((id, e)) = s.edges_sorted().next() {
            steps.push(GraphEdit::reconnect(&s, id, e.tgt, e.src).map_err(fail)?);
            steps.push(GraphEdit::Reconnect { id, old: (e.src, e.tgt), new: (e.tgt, e.src) });
        }
        let mut edits = vec![
            GraphEdit::Batch(steps[..2].to_vec()),
            GraphEdit::Batch(steps.clone()),
            GraphEdit::Batch(steps.iter().rev().cloned().collect()),
        ];
        edits.extend(steps);
        for edit in edits {
            let mut by_edit = s.clone();
            let mut by_delta = s.clone();
            let via_edit = edit.apply(&mut by_edit);
            match edit.to_delta() {
                Ok(delta) => {
                    let via_delta = delta.apply(&mut by_delta);
                    if matches!(edit, GraphEdit::Batch(_)) {
                        prop_assert_eq!(via_edit.is_ok(), via_delta.is_ok());
                    } else {
                        prop_assert_eq!(&via_edit, &via_delta);
                    }
                    prop_assert!(GInstance::identical(&by_edit, &by_delta));
                }
                Err(err) => {
                    prop_assert!(via_edit.is_err() && GInstance::identical(&by_edit, &s));
                    if !matches!(edit, GraphEdit::Batch(_)) {
                        prop_assert_eq!(via_edit, Err(EditError::Compose(err)));
                    }
                }
            }
        }
        // The first delta alone always applies to `s`.
        let mut t = s;
        prop_assert!(GraphEdit::Delta(d1).apply(&mut t).is_ok());
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

/// The apply and check properties are exercised on every outcome: deltas
/// that fail a check of the input, deltas that apply, and deltas that fail
/// the fan-domain check after mutating the state, which [`GraphDelta::apply`]
/// then rolls back.
#[test]
fn the_apply_cases_cover_every_outcome() -> Result<(), TestCaseError> {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    let mut runner = TestRunner::deterministic();
    let strategy = apply_cases();
    let (mut applied, mut local, mut fan) = (0, 0, 0);
    for _ in 0..512 {
        for case in strategy.new_tree(&mut runner).map_err(fail)?.current() {
            match case.delta.apply(&mut case.state.clone()) {
                Ok(()) if !case.delta.is_identity() => applied += 1,
                Ok(()) => {}
                Err(EditError::MalformedFan { .. }) => fan += 1,
                Err(_) => local += 1,
            }
        }
    }
    let counts = format!(
        "{applied} applied, {local} failed a check of the input, {fan} left the fan domain"
    );
    prop_assert!(applied >= 50, "{}", counts);
    prop_assert!(local >= 50, "{}", counts);
    prop_assert!(fan >= 50, "{}", counts);
    Ok(())
}

/// The associativity property on composable triples is exercised: the
/// generator yields composable triples of non-identity deltas, among them
/// triples where a composite of two neighbours absorbs an added element (an
/// add cancelled by a later delete, or followed by a replacement).
#[test]
fn the_composable_triple_generator_covers_cancellation() -> Result<(), TestCaseError> {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    let mut runner = TestRunner::deterministic();
    let strategy = composable_triple_candidates();
    let (mut composable, mut cancelling) = (0, 0);
    for _ in 0..1024 {
        let (a, b, c) = strategy.new_tree(&mut runner).map_err(fail)?.current();
        if a.is_identity() || b.is_identity() || c.is_identity() {
            continue;
        }
        if let (Ok(ab), Ok(bc)) = (a.compose(&b), b.compose(&c)) {
            composable += 1;
            let added = |d: &GraphDelta| d.add_nodes().len() + d.add_edges().len();
            if added(&ab) < added(&a) + added(&b) || added(&bc) < added(&b) + added(&c) {
                cancelling += 1;
            }
        }
    }
    let counts =
        format!("{composable} composable triples, {cancelling} absorbing an added element");
    prop_assert!(composable >= 100, "{}", counts);
    prop_assert!(cancelling >= 20, "{}", counts);
    Ok(())
}

/// The fan law is exercised: among pairs of non-identity deltas that each
/// apply to a graph with fans, the generator yields independent pairs,
/// pairs whose only conflicts concern fans, and pairs where applying one
/// after the other leaves the fan domain.
#[test]
fn the_fan_independence_generator_covers_fan_conflicts() -> Result<(), TestCaseError> {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    let mut runner = TestRunner::deterministic();
    let strategy = (arb_fan_graph(), arb_fan_ops(), arb_fan_ops());
    let (mut independent, mut fan_only, mut broken) = (0, 0, 0);
    for _ in 0..4096 {
        let (s, o1, o2) = strategy.new_tree(&mut runner).map_err(fail)?.current();
        let (t1, t2) = (mutate(&s, &o1), mutate(&s, &o2));
        if !FanDomain::contains(&t1) || !FanDomain::contains(&t2) {
            continue;
        }
        let a = GraphDelta::diff(&s, &t1);
        let b = GraphDelta::diff(&s, &t2);
        if a.is_identity() || b.is_identity() {
            continue;
        }
        let conflicts = a.independence(&b);
        if conflicts.is_empty() {
            independent += 1;
        } else if conflicts
            .iter()
            .all(|c| matches!(c, EditConflict::FanUse { .. }))
        {
            fan_only += 1;
        }
        let mut ab = s.clone();
        if a.apply(&mut ab).is_ok()
            && matches!(b.apply(&mut ab), Err(EditError::MalformedFan { .. }))
        {
            broken += 1;
        }
    }
    let counts = format!(
        "{independent} independent, {fan_only} with fan conflicts alone, {broken} leave the fan domain"
    );
    prop_assert!(independent >= 50, "{}", counts);
    prop_assert!(fan_only >= 20, "{}", counts);
    prop_assert!(broken >= 5, "{}", counts);
    Ok(())
}

/// The case `independent_graph_deltas_commute_and_merge_around_fans` shrank
/// to before fan conflicts were reported: one fan node 20 with its
/// `hyper_parent` edge to node 0, and two deltas that each give the fan a
/// `hyper_label` edge of the same name. Each keeps the fan well formed;
/// together they would not, so the pair must not be independent.
#[test]
fn two_same_name_labels_on_one_fan_are_a_fan_conflict() -> Result<(), TestCaseError> {
    let s = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(20, "f").with_shape(NodeShape::HyperEdge))
        .with_edge_id(EdgeId(20), 20, 0, fan_edge(None));
    let (t1, t2) = (
        mutate(&s, &[(10, 0, 6, 0, 1)]),
        mutate(&s, &[(10, 0, 0, 0, 1)]),
    );
    prop_assert!(FanDomain::contains(&s));
    prop_assert!(FanDomain::contains(&t1) && FanDomain::contains(&t2));
    let (a, b) = (GraphDelta::diff(&s, &t1), GraphDelta::diff(&s, &t2));
    let conflicts = a.independence(&b);
    prop_assert!(
        conflicts
            .iter()
            .any(|c| matches!(c, EditConflict::FanUse { .. })),
        "{:?}",
        conflicts
    );
    independence_law(&s, &a, &b)
}
