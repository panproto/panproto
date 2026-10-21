#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The graph lens reads its source through the plan-taking graph restrict,
//! so a source outside the fan domain is refused by every graph-lens path
//! with the restrict's refusal: the least node that must be a fan and is
//! not, and its first violation.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::{
    BoundaryProtocols, CompiledMigration, EdgeId, FanViolation, GEdge, GInstance, Node, NodeShape,
    RestrictError,
};
use panproto_lens::{Lens, LensError, graph_lens};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

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

fn identity_lens() -> Lens {
    let s = fan_schema();
    Lens {
        compiled: CompiledMigration {
            surviving_verts: [Name::from("a"), Name::from("b")].into_iter().collect(),
            ..CompiledMigration::default()
        },
        src_schema: s.clone(),
        tgt_schema: s,
    }
}

/// Plain nodes 0, 1 and 2 at `a`, and a parent edge and a child edge from
/// node 2 to node 0: node 2 holds reserved edges without the fan shape.
fn malformed() -> GInstance {
    let mut g = GInstance::new();
    for id in 0..3 {
        g.insert_node(Node::new(id, "a").with_shape(NodeShape::Plain))
            .unwrap();
    }
    for (id, kind, name, tgt) in [(0, "hyper_parent", "p", "a"), (1, "hyper_label", "l", "b")] {
        let edge = Edge {
            src: "h".into(),
            tgt: tgt.into(),
            kind: kind.into(),
            name: Some(name.into()),
        };
        g.insert_edge(EdgeId(id), GEdge::new(2, 0, edge)).unwrap();
    }
    g
}

fn assert_malformed(result: Result<impl std::fmt::Debug, LensError>) {
    match result {
        Err(LensError::Restrict(RestrictError::MalformedFan { node, reason })) => assert_eq!(
            (node, reason),
            (2, FanViolation::ReservedKindOutsideFan { edge: EdgeId(0) })
        ),
        other => panic!("not refused as a malformed fan: {other:?}"),
    }
}

#[test]
fn graph_get_refuses_the_source() {
    assert_malformed(graph_lens::get(&identity_lens(), &malformed()));
}

#[test]
fn graph_get_with_protocols_refuses_the_source() {
    let protocol = Protocol::default();
    assert_malformed(graph_lens::get_with_protocols(
        &identity_lens(),
        &malformed(),
        BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        },
    ));
}

#[test]
fn contraction_witnesses_refuse_the_source() {
    assert_malformed(graph_lens::contraction_witnesses(
        &identity_lens(),
        &malformed(),
    ));
}
