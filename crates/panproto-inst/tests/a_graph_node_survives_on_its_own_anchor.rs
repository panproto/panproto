//! A graph node whose anchor survives unchanged is kept by restrict.
//!
//! A compiled migration lists a surviving vertex in `surviving_verts` and
//! names it in `vertex_remap` only when the vertex is renamed. A node over a
//! vertex that survives under its own name is therefore absent from the remap,
//! and restrict decides its survival by looking the anchor up in
//! `surviving_verts` directly. The tree restrict has always done this; the
//! graph restrict must agree with it.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::{HashMap, HashSet};

use panproto_gat::Name;
use panproto_inst::{CompiledMigration, GInstance, Node, graph_restrict};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

fn knows() -> Edge {
    Edge {
        src: "person".into(),
        tgt: "person".into(),
        kind: "knows".into(),
        name: None,
    }
}

fn migration(surviving: &[&str], remap: &[(&str, &str)]) -> CompiledMigration {
    CompiledMigration {
        surviving_verts: surviving.iter().map(|v| Name::from(*v)).collect(),
        surviving_edges: HashSet::from([knows()]),
        vertex_remap: remap
            .iter()
            .map(|(s, t)| (Name::from(*s), Name::from(*t)))
            .collect(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
        op_term_assignments: HashMap::new(),
        expansion_path: HashMap::new(),
    }
}

fn schema() -> Schema {
    SchemaBuilder::new(&Protocol::default())
        .vertex("person", "record", None)
        .expect("vertex")
        .edge("person", "person", "knows", None)
        .expect("edge")
        .build()
        .expect("schema")
}

fn two_people() -> GInstance {
    GInstance::new()
        .with_node(Node::new(0, "person"))
        .with_node(Node::new(1, "person"))
        .with_node(Node::new(2, "place"))
        .with_edge(0, 1, knows())
}

#[test]
fn a_node_over_an_unrenamed_surviving_vertex_is_kept() {
    let restricted = graph_restrict(
        &two_people(),
        &schema(),
        &schema(),
        &migration(&["person"], &[]),
    )
    .expect("restrict succeeds");
    assert_eq!(
        restricted.node_count(),
        2,
        "both people survive under their own anchor; the place does not"
    );
    assert_eq!(restricted.edge_count(), 1, "the edge between them survives");
}

#[test]
fn a_node_renamed_onto_a_vertex_that_does_not_survive_is_dropped() {
    let restricted = graph_restrict(
        &two_people(),
        &schema(),
        &schema(),
        &migration(&["human"], &[("person", "ghost")]),
    )
    .expect("restrict succeeds");
    assert_eq!(
        restricted.node_count(),
        0,
        "a remap onto a vertex outside surviving_verts keeps nothing"
    );
}
