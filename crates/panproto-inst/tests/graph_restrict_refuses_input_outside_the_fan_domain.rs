#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Graph restrict is defined on the fan domain only. The engine that takes
//! a compiled plan refuses an input outside it with the least offending
//! node and that node's first violation, in the order `FanDomain::check`
//! tests them, before any stage runs; the validating entry points refuse
//! the same input at the input boundary.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::{
    BoundaryProtocols, BoundarySide, CompiledMigration, EdgeId, FanDomain, FanViolation, GEdge,
    GInstance, InstError, Node, NodeShape, RestrictError, RestrictPlan, ValidationOptions,
    ValidationPlan, graph_restrict, graph_restrict_validated, graph_restrict_with_complement,
    graph_restrict_with_plan,
};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

/// Vertices `a` and `b`, and the hyper-edge `h` with signature
/// `{p -> a, l -> b}` and parent label `p`.
fn fan_schema() -> Schema {
    let protocol = Protocol::default();
    SchemaBuilder::new(&protocol)
        .vertex("a", "record", None)
        .unwrap()
        .vertex("b", "record", None)
        .unwrap()
        .hyper_edge(
            "h",
            "fan",
            HashMap::from([
                ("p".to_owned(), "a".to_owned()),
                ("l".to_owned(), "b".to_owned()),
            ]),
            "p",
        )
        .unwrap()
        .build()
        .unwrap()
}

fn identity() -> CompiledMigration {
    CompiledMigration {
        surviving_verts: [Name::from("a"), Name::from("b")].into_iter().collect(),
        ..CompiledMigration::default()
    }
}

fn parent() -> Edge {
    Edge {
        src: "h".into(),
        tgt: "a".into(),
        kind: "hyper_parent".into(),
        name: Some("p".into()),
    }
}

fn label() -> Edge {
    Edge {
        src: "h".into(),
        tgt: "b".into(),
        kind: "hyper_label".into(),
        name: Some("l".into()),
    }
}

/// A graph with the given nodes (`(anchor, fan shape)`, ids ascending from
/// 0) and edges (`(src, tgt, label)`, ids ascending from 0).
fn graph(nodes: &[(&str, bool)], edges: &[(u32, u32, Edge)]) -> GInstance {
    let mut g = GInstance::new();
    for (id, (anchor, fan)) in (0u32..).zip(nodes) {
        let shape = if *fan {
            NodeShape::HyperEdge
        } else {
            NodeShape::Plain
        };
        g.insert_node(Node::new(id, *anchor).with_shape(shape))
            .unwrap();
    }
    for (id, (src, tgt, edge)) in (0u32..).zip(edges) {
        g.insert_edge(EdgeId(id), GEdge::new(*src, *tgt, edge.clone()))
            .unwrap();
    }
    g
}

/// The sources outside the fan domain on which the two restrict engines
/// disagreed before the domain was fixed, each with the refusal the domain
/// gives it: the least node that must be a fan and is not, and its first
/// violation.
fn recorded_witnesses() -> Vec<(GInstance, u32, FanViolation)> {
    vec![
        // A plain node with a parent edge and a child edge.
        (
            graph(
                &[("a", false), ("a", false), ("a", false)],
                &[(2, 0, parent()), (2, 0, label())],
            ),
            2,
            FanViolation::ReservedKindOutsideFan { edge: EdgeId(0) },
        ),
        // Two plain nodes, each with a child self-loop.
        (
            graph(
                &[("a", false), ("a", false)],
                &[(1, 1, label()), (0, 0, label())],
            ),
            0,
            FanViolation::ReservedKindOutsideFan { edge: EdgeId(1) },
        ),
        // A plain node with a child edge into a fan node and a child
        // self-loop.
        (
            graph(
                &[("a", false), ("b", false), ("h", true)],
                &[(0, 2, label()), (0, 0, label())],
            ),
            0,
            FanViolation::ReservedKindOutsideFan { edge: EdgeId(0) },
        ),
        // A fan node whose parent edge is a self-loop, and a plain node
        // with a parent edge.
        (
            graph(
                &[("a", false), ("b", false), ("h", true)],
                &[(2, 2, parent()), (1, 0, parent())],
            ),
            1,
            FanViolation::ReservedKindOutsideFan { edge: EdgeId(1) },
        ),
        // A parent self-loop on one plain node, a child self-loop on another.
        (
            graph(
                &[("a", false), ("a", false)],
                &[(0, 0, parent()), (1, 1, label())],
            ),
            0,
            FanViolation::ReservedKindOutsideFan { edge: EdgeId(0) },
        ),
        // A plain node with a child edge and a parent edge to one node.
        (
            graph(
                &[("a", false), ("a", false)],
                &[(0, 1, label()), (0, 1, parent())],
            ),
            0,
            FanViolation::ReservedKindOutsideFan { edge: EdgeId(0) },
        ),
        // One plain node with a parent and a child self-loop.
        (
            graph(&[("b", false)], &[(0, 0, parent()), (0, 0, label())]),
            0,
            FanViolation::ReservedKindOutsideFan { edge: EdgeId(0) },
        ),
        // A fan node with no parent.
        (
            graph(&[("b", false), ("h", true)], &[(1, 0, label())]),
            1,
            FanViolation::MissingParent,
        ),
        // A fan node with an in-edge: its own parent self-loop.
        (
            graph(&[("a", false), ("h", true)], &[(1, 1, parent())]),
            1,
            FanViolation::ForeignIncidentEdge { edge: EdgeId(0) },
        ),
        // A fan node with two parents.
        (
            graph(
                &[("a", false), ("h", true)],
                &[(1, 0, parent()), (1, 0, parent())],
            ),
            1,
            FanViolation::MultipleParents {
                edges: vec![EdgeId(0), EdgeId(1)],
            },
        ),
    ]
}

#[test]
fn the_witnesses_are_outside_the_fan_domain_with_the_stated_violation() {
    for (g, node, reason) in recorded_witnesses() {
        match FanDomain::check(&g) {
            Err(InstError::MalformedFan {
                node: found,
                reason: found_reason,
            }) => assert_eq!((found, found_reason), (node, reason), "{g:?}"),
            other => panic!("{g:?} is not refused as expected: {other:?}"),
        }
    }
}

#[test]
fn the_plan_engine_refuses_each_witness_with_its_least_violation() {
    let s = fan_schema();
    let plan = RestrictPlan::compile(&s, &s, &identity()).unwrap();
    for (g, node, reason) in recorded_witnesses() {
        let error = graph_restrict_with_plan(&g, &plan).unwrap_err();
        assert_eq!(error.key(), None, "{g:?}: {error:?}");
        match error {
            RestrictError::MalformedFan {
                node: found,
                reason: found_reason,
            } => assert_eq!((found, found_reason), (node, reason.clone()), "{g:?}"),
            other => panic!("{g:?} is not refused as malformed: {other:?}"),
        }
    }
}

#[test]
fn the_refusal_comes_before_every_stage() {
    // Node 1 has a survival predicate that cannot evaluate (stage 0), and
    // node 2 is a plain node with a reserved edge: the fan domain is
    // checked first, so the refusal is the malformed fan.
    let s = fan_schema();
    let mut m = identity();
    m.add_conditional_survival("b", panproto_expr::Expr::Var("missing".into()));
    let plan = RestrictPlan::compile(&s, &s, &m).unwrap();
    let g = graph(
        &[("a", false), ("b", false), ("a", false)],
        &[(2, 0, parent())],
    );
    match graph_restrict_with_plan(&g, &plan).unwrap_err() {
        RestrictError::MalformedFan { node, reason } => {
            assert_eq!(
                (node, reason),
                (2, FanViolation::ReservedKindOutsideFan { edge: EdgeId(0) })
            );
        }
        other => panic!("not refused as malformed: {other:?}"),
    }
}

#[test]
fn a_well_formed_fan_still_restricts() {
    let s = fan_schema();
    let plan = RestrictPlan::compile(&s, &s, &identity()).unwrap();
    let g = graph(
        &[("a", false), ("b", false), ("h", true)],
        &[(2, 0, parent()), (2, 1, label())],
    );
    assert!(FanDomain::contains(&g));
    let (view, complement) = graph_restrict_with_plan(&g, &plan).unwrap();
    assert_eq!(view.nodes().len(), 3);
    assert_eq!(view.edges().len(), 2);
    assert!(complement.residue_is_trivial(), "{complement:?}");
}

#[test]
fn the_message_names_the_node_and_the_violation() {
    let error = RestrictError::MalformedFan {
        node: 2,
        reason: FanViolation::MissingParent,
    };
    assert_eq!(
        error.to_string(),
        format!("fan node 2 is malformed: {}", FanViolation::MissingParent)
    );
    let detail = error.detail();
    assert_eq!(
        detail,
        panproto_inst::ErrorDetail::Restrict {
            key: None,
            kind: "malformed_fan".to_owned(),
            message: error.to_string(),
        }
    );
}

/// The input report of a refused validating restrict, which must name a
/// malformed fan.
fn assert_input_refusal(error: &RestrictError, g: &GInstance) {
    let RestrictError::Validation { side, report } = error else {
        panic!("{g:?} is not refused at the boundary: {error:?}");
    };
    assert_eq!(*side, BoundarySide::Input, "{g:?}");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| matches!(d.violation, panproto_inst::Violation::MalformedFan { .. })),
        "{g:?}: {report:?}"
    );
}

#[test]
fn the_validating_entry_points_refuse_each_witness_at_the_input() {
    let s = fan_schema();
    let m = identity();
    let protocol = Protocol::default();
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let validation = ValidationPlan::compile(&s, &protocol, &ValidationOptions::default());
    let plan = RestrictPlan::compile(&s, &s, &m).unwrap();
    for (g, _, _) in recorded_witnesses() {
        assert_input_refusal(
            &graph_restrict_with_complement(&g, &s, &s, &m, protocols).unwrap_err(),
            &g,
        );
        assert_input_refusal(&graph_restrict(&g, &s, &s, &m, protocols).unwrap_err(), &g);
        assert_input_refusal(
            &graph_restrict_validated(&g, &validation, &validation, &plan).unwrap_err(),
            &g,
        );
    }
}
