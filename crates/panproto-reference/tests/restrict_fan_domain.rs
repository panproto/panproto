#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Restriction is defined on the fan domain only: an input outside it is
//! refused with the least node that must be a fan and is not, and that
//! node's first violation, before any stage runs.

use std::collections::BTreeMap;

use panproto_reference::enumerate::{GraphScope, graphs, graphs_count};
use panproto_reference::fan::{self, FanViolation};
use panproto_reference::lens::{self, GraphLens};
use panproto_reference::migration::Migration;
use panproto_reference::model::{GEdge, Graph, Node, NodeShape, SEdge};
use panproto_reference::restrict::{self, NoValueLayer, RestrictError};
use panproto_reference::schema::{HyperEdge, Schema};

/// Vertices `a` and `b`, the edge `a -e-> b`, and the hyper-edge `h` with
/// signature `{p -> a, l -> b}` and parent label `p`.
fn fan_schema() -> Schema {
    let mut schema = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .edge("a", "b", "prop", Some("e"));
    schema.hyper_edges.insert(
        "h".to_owned(),
        HyperEdge {
            kind: "fan".to_owned(),
            signature: BTreeMap::from([
                ("l".to_owned(), "b".to_owned()),
                ("p".to_owned(), "a".to_owned()),
            ]),
            parent_label: "p".to_owned(),
        },
    );
    schema
}

fn identity() -> Migration {
    Migration {
        surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
        surviving_edges: [SEdge::new("a", "b", "prop", Some("e"))].into(),
        ..Migration::default()
    }
}

fn parent() -> SEdge {
    SEdge::new("h", "a", restrict::HYPER_PARENT, Some("p"))
}

fn label() -> SEdge {
    SEdge::new("h", "b", restrict::HYPER_LABEL, Some("l"))
}

fn graph(nodes: &[(&str, bool)], edges: &[(u32, u32, SEdge)]) -> Graph {
    let mut g = Graph::default();
    for (id, (anchor, fan)) in (0u32..).zip(nodes) {
        let mut node = Node::new(id, anchor);
        if *fan {
            node.shape = NodeShape::HyperEdge;
        }
        g.nodes.insert(id, node);
    }
    for (id, (src, tgt, label)) in (0u32..).zip(edges) {
        g.edges.insert(
            id,
            GEdge {
                src: *src,
                tgt: *tgt,
                label: label.clone(),
                position: None,
            },
        );
    }
    g
}

/// The sources outside the fan domain on which the two restrict engines
/// disagreed before the domain was fixed, with their refusals.
fn recorded_witnesses() -> Vec<(Graph, u32, FanViolation)> {
    let outside = |edge| FanViolation::ReservedKindOutsideFan { edge };
    vec![
        (
            graph(
                &[("a", false), ("a", false), ("a", false)],
                &[(2, 0, parent()), (2, 0, label())],
            ),
            2,
            outside(0),
        ),
        (
            graph(
                &[("a", false), ("a", false)],
                &[(1, 1, label()), (0, 0, label())],
            ),
            0,
            outside(1),
        ),
        (
            graph(
                &[("a", false), ("b", false), ("h", true)],
                &[(0, 2, label()), (0, 0, label())],
            ),
            0,
            outside(0),
        ),
        (
            graph(
                &[("a", false), ("b", false), ("h", true)],
                &[(2, 2, parent()), (1, 0, parent())],
            ),
            1,
            outside(1),
        ),
        (
            graph(
                &[("a", false), ("a", false)],
                &[(0, 0, parent()), (1, 1, label())],
            ),
            0,
            outside(0),
        ),
        (
            graph(
                &[("a", false), ("a", false)],
                &[(0, 1, label()), (0, 1, parent())],
            ),
            0,
            outside(0),
        ),
        (
            graph(&[("b", false)], &[(0, 0, parent()), (0, 0, label())]),
            0,
            outside(0),
        ),
        (
            graph(&[("b", false), ("h", true)], &[(1, 0, label())]),
            1,
            FanViolation::MissingParent,
        ),
        (
            graph(&[("a", false), ("h", true)], &[(1, 1, parent())]),
            1,
            FanViolation::ForeignIncidentEdge { edge: 0 },
        ),
    ]
}

#[test]
fn restrict_refuses_each_witness_with_its_least_violation() {
    let s = fan_schema();
    let m = identity();
    for (g, node, reason) in recorded_witnesses() {
        assert_eq!(
            restrict::get(&s, &s, &m, &NoValueLayer, &g),
            Err(RestrictError::MalformedFan { node, reason }),
            "{g:?}"
        );
    }
}

#[test]
fn graph_lens_get_refuses_each_witness() {
    let s = fan_schema();
    let m = identity();
    let plan = restrict::compile(&s, &s, &m).unwrap();
    let lens = GraphLens::new(&s, &s, &m, &plan, &BTreeMap::new(), &NoValueLayer).unwrap();
    for (g, node, reason) in recorded_witnesses() {
        assert_eq!(
            lens::get(&lens, &g).map(|_| ()),
            Err(RestrictError::MalformedFan { node, reason }),
            "{g:?}"
        );
    }
}

#[test]
fn restrict_refuses_exactly_the_scope_outside_the_fan_domain() {
    // Every graph of up to three nodes at `a`, `b` or `h`, of either shape,
    // with up to two edges carrying the parent label, the child label or
    // the plain edge: a graph outside the fan domain is refused with the
    // fan domain's first violation, and no graph inside it is refused so.
    let labels = [parent(), label(), SEdge::new("a", "b", "prop", Some("e"))];
    let scope = GraphScope {
        anchors: &["a", "b", "h"],
        values: &[],
        shapes: &[NodeShape::Plain, NodeShape::HyperEdge],
        labels: &labels,
        positions: &[None],
        max_nodes: 3,
        max_edges: 2,
    };
    let all = graphs(&scope);
    assert_eq!(
        u64::try_from(all.len()).unwrap(),
        graphs_count(3, 0, 2, 3, 1, 3, 2)
    );
    let s = fan_schema();
    let m = identity();
    let (mut outside, mut inside) = (0usize, 0usize);
    for g in &all {
        let result = restrict::get(&s, &s, &m, &NoValueLayer, g);
        if let Some((node, reason)) = fan::first_violation(g, None) {
            outside += 1;
            assert_eq!(
                result,
                Err(RestrictError::MalformedFan { node, reason }),
                "{g:?}"
            );
        } else {
            inside += 1;
            assert!(
                !matches!(result, Err(RestrictError::MalformedFan { .. })),
                "{g:?}: {result:?}"
            );
        }
    }
    println!(
        "fan scope: {} graphs, {inside} inside the fan domain, {outside} outside",
        all.len()
    );
    assert!(inside > 0 && outside > 0);
}
