//! Protocol theory registration routes through the checked colimit
//! [`panproto_gat::pushout_by_name`], which builds identity inclusion
//! morphisms and runs the cocone check that the name-based `colimit_by_name`
//! path does not.
//!
//! These tests assert that, for every `register_*` function, the theory
//! produced through the checked path has the same sort, op, equation, and
//! directed-equation sets as the `colimit_by_name` output. That they pass at
//! all also confirms the cocone check succeeds for every registered protocol
//! theory (registration would panic otherwise).
//!
//! The reference oracle `colimit_by_name` is used only here in test code,
//! never on the production registration path (see the acceptance grep over
//! `crates/panproto-protocols/src`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use panproto_gat::{Sort, Theory, colimit_by_name};
use panproto_protocols::theories::{
    register_constrained_graph_instance, register_constrained_multigraph_wtype,
    register_hypergraph_functor, register_multigraph_wtype_meta, register_simple_graph_flat,
    register_typed_graph_wtype, th_constraint, th_graph, th_hypergraph, th_interface, th_meta,
    th_multi, th_simple_graph, th_wtype,
};

/// The full structural content of a theory: one Debug string per sort / op
/// / equation / directed equation, each list sorted so the comparison is
/// order-independent (a *set* comparison). Debug captures signatures and
/// closures, not just names, so a drift in an op's inputs or a sort's
/// parameters would be caught.
fn structural_sets(t: &Theory) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    let mut sorts: Vec<String> = t.sorts.iter().map(|s| format!("{s:?}")).collect();
    let mut ops: Vec<String> = t.ops.iter().map(|o| format!("{o:?}")).collect();
    let mut eqs: Vec<String> = t.eqs.iter().map(|e| format!("{e:?}")).collect();
    let mut deqs: Vec<String> = t.directed_eqs.iter().map(|d| format!("{d:?}")).collect();
    sorts.sort();
    ops.sort();
    eqs.sort();
    deqs.sort();
    (sorts, ops, eqs, deqs)
}

fn shared_vertex() -> Theory {
    Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![])
}

fn shared_ve() -> Theory {
    Theory::new(
        "ThVertexEdge",
        vec![Sort::simple("Vertex"), Sort::simple("Edge")],
        vec![],
        vec![],
    )
}

/// `colimit(colimit(ThGraph, ThConstraint), ThMulti)` via the legacy
/// name-based path, shared by Groups A, E, and F.
fn expected_constrained_multigraph() -> Theory {
    let gc = colimit_by_name(&th_graph(), &th_constraint(), &shared_vertex()).unwrap();
    colimit_by_name(&gc, &th_multi(), &shared_ve()).unwrap()
}

#[test]
fn group_a_constrained_multigraph_wtype() {
    let mut registry: HashMap<String, Theory> = HashMap::new();
    register_constrained_multigraph_wtype(&mut registry, "SchemaA", "InstA");
    let registered = registry.get("SchemaA").expect("SchemaA registered");
    assert_eq!(
        structural_sets(registered),
        structural_sets(&expected_constrained_multigraph())
    );
}

#[test]
fn group_b_hypergraph_functor() {
    let mut registry: HashMap<String, Theory> = HashMap::new();
    register_hypergraph_functor(&mut registry, "SchemaB", "InstB");
    let registered = registry.get("SchemaB").expect("SchemaB registered");
    let expected = colimit_by_name(&th_hypergraph(), &th_constraint(), &shared_vertex()).unwrap();
    assert_eq!(structural_sets(registered), structural_sets(&expected));
}

#[test]
fn group_c_simple_graph_flat() {
    let mut registry: HashMap<String, Theory> = HashMap::new();
    register_simple_graph_flat(&mut registry, "SchemaC", "InstC");
    let registered = registry.get("SchemaC").expect("SchemaC registered");
    let expected = colimit_by_name(&th_simple_graph(), &th_constraint(), &shared_vertex()).unwrap();
    assert_eq!(structural_sets(registered), structural_sets(&expected));
}

#[test]
fn group_d_typed_graph_wtype() {
    let mut registry: HashMap<String, Theory> = HashMap::new();
    register_typed_graph_wtype(&mut registry, "SchemaD", "InstD");
    let registered = registry.get("SchemaD").expect("SchemaD registered");

    let gc = colimit_by_name(&th_graph(), &th_constraint(), &shared_vertex()).unwrap();
    let gcm = colimit_by_name(&gc, &th_multi(), &shared_ve()).unwrap();
    let shared_vertex_only = Theory::new("ThVertex2", vec![Sort::simple("Vertex")], vec![], vec![]);
    let expected = colimit_by_name(&gcm, &th_interface(), &shared_vertex_only).unwrap();
    assert_eq!(structural_sets(registered), structural_sets(&expected));
}

#[test]
fn group_e_multigraph_wtype_meta() {
    let mut registry: HashMap<String, Theory> = HashMap::new();
    register_multigraph_wtype_meta(&mut registry, "SchemaE", "InstE");

    // Schema theory matches the constrained-multigraph reconstruction.
    let schema = registry.get("SchemaE").expect("SchemaE registered");
    assert_eq!(
        structural_sets(schema),
        structural_sets(&expected_constrained_multigraph())
    );

    // Instance theory is itself a colimit: colimit(ThWType, ThMeta).
    let inst = registry.get("InstE").expect("InstE registered");
    let shared_node_value = Theory::new(
        "ThNodeValue",
        vec![Sort::simple("Node"), Sort::simple("Value")],
        vec![],
        vec![],
    );
    let expected_inst = colimit_by_name(&th_wtype(), &th_meta(), &shared_node_value).unwrap();
    assert_eq!(structural_sets(inst), structural_sets(&expected_inst));
}

#[test]
fn group_f_constrained_graph_instance() {
    let mut registry: HashMap<String, Theory> = HashMap::new();
    register_constrained_graph_instance(&mut registry, "SchemaF", "InstF");
    let registered = registry.get("SchemaF").expect("SchemaF registered");
    assert_eq!(
        structural_sets(registered),
        structural_sets(&expected_constrained_multigraph())
    );
}

#[test]
fn atproto_register_theories_schema_and_instance() {
    let mut registry: HashMap<String, Theory> = HashMap::new();
    panproto_protocols::atproto::register_theories(&mut registry);

    // Schema: colimit(colimit(ThGraph, ThConstraint), ThMulti).
    let schema = registry
        .get("ThATProtoSchema")
        .expect("ThATProtoSchema registered");
    assert_eq!(
        structural_sets(schema),
        structural_sets(&expected_constrained_multigraph())
    );

    // Instance: colimit(ThWType, ThMeta) over shared Node (atproto uses a
    // Node-only shared theory, distinct from Group E's Node+Value).
    let shared_node = Theory::new("ThNode", vec![Sort::simple("Node")], vec![], vec![]);
    let expected_inst = colimit_by_name(&th_wtype(), &th_meta(), &shared_node).unwrap();
    let inst = registry
        .get("ThATProtoInstance")
        .expect("ThATProtoInstance registered");
    assert_eq!(structural_sets(inst), structural_sets(&expected_inst));
}
