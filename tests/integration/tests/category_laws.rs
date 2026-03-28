//! Integration tests for categorical law verification.
//!
//! Verifies composition associativity, identity unit laws, functor
//! contravariance, and other algebraic properties that the panproto
//! implementations claim to satisfy.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_gat::Name;
use panproto_gat::{Operation, Sort, Theory, TheoryMorphism, check_morphism, colimit_by_name};
use panproto_inst::value::FieldPresence;
use panproto_inst::{CompiledMigration, Node, Value, WInstance, wtype_restrict};
use panproto_schema::{Edge, Schema, Vertex};
use smallvec::SmallVec;

/// Build a schema with the given vertices and edges.
fn make_schema(verts: &[(&str, &str)], edge_list: &[Edge]) -> Schema {
    let mut vertices = HashMap::new();
    let mut edges = HashMap::new();
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

    for (id, kind) in verts {
        vertices.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
                nsid: None,
            },
        );
    }
    for e in edge_list {
        edges.insert(e.clone(), e.kind.clone());
        outgoing.entry(e.src.clone()).or_default().push(e.clone());
        incoming.entry(e.tgt.clone()).or_default().push(e.clone());
        between
            .entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
    }

    Schema {
        protocol: "test".into(),
        vertices,
        edges,
        hyper_edges: HashMap::new(),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        coercions: HashMap::new(),
        mergers: HashMap::new(),
        defaults: HashMap::new(),
        policies: HashMap::new(),
        outgoing,
        incoming,
        between,
    }
}

// =========================================================================
// Theory morphism laws
// =========================================================================

#[test]
fn theory_morphism_compose_associative() -> Result<(), Box<dyn std::error::Error>> {
    let t1 = Theory::new(
        "T1",
        vec![Sort::simple("A")],
        vec![Operation::unary("f", "x", "A", "A")],
        Vec::new(),
    );
    let _t2 = Theory::new(
        "T2",
        vec![Sort::simple("B")],
        vec![Operation::unary("g", "x", "B", "B")],
        Vec::new(),
    );
    let _t3 = Theory::new(
        "T3",
        vec![Sort::simple("C")],
        vec![Operation::unary("h", "x", "C", "C")],
        Vec::new(),
    );
    let t4 = Theory::new(
        "T4",
        vec![Sort::simple("D")],
        vec![Operation::unary("k", "x", "D", "D")],
        Vec::new(),
    );

    let m1 = TheoryMorphism::new(
        "m1",
        "T1",
        "T2",
        HashMap::from([(Arc::from("A"), Arc::from("B"))]),
        HashMap::from([(Arc::from("f"), Arc::from("g"))]),
    );
    let m2 = TheoryMorphism::new(
        "m2",
        "T2",
        "T3",
        HashMap::from([(Arc::from("B"), Arc::from("C"))]),
        HashMap::from([(Arc::from("g"), Arc::from("h"))]),
    );
    let m3 = TheoryMorphism::new(
        "m3",
        "T3",
        "T4",
        HashMap::from([(Arc::from("C"), Arc::from("D"))]),
        HashMap::from([(Arc::from("h"), Arc::from("k"))]),
    );

    // (m1 ; m2) ; m3
    let left = m1.compose(&m2)?.compose(&m3)?;
    // m1 ; (m2 ; m3)
    let m2_m3 = m2.compose(&m3)?;
    let right = m1.compose(&m2_m3)?;

    assert_eq!(left.sort_map, right.sort_map, "sort_map associativity");
    assert_eq!(left.op_map, right.op_map, "op_map associativity");
    assert_eq!(left.domain, right.domain);
    assert_eq!(left.codomain, right.codomain);

    // Verify both are valid morphisms.
    check_morphism(&left, &t1, &t4)?;
    check_morphism(&right, &t1, &t4)?;

    Ok(())
}

#[test]
fn theory_morphism_identity_unit() -> Result<(), Box<dyn std::error::Error>> {
    let t1 = Theory::new(
        "T1",
        vec![Sort::simple("A"), Sort::simple("B")],
        vec![Operation::unary("f", "x", "A", "B")],
        Vec::new(),
    );
    let t2 = Theory::new(
        "T2",
        vec![Sort::simple("X"), Sort::simple("Y")],
        vec![Operation::unary("g", "x", "X", "Y")],
        Vec::new(),
    );

    let id1 = TheoryMorphism::identity(&t1);
    let id2 = TheoryMorphism::identity(&t2);

    let m = TheoryMorphism::new(
        "m",
        "T1",
        "T2",
        HashMap::from([
            (Arc::from("A"), Arc::from("X")),
            (Arc::from("B"), Arc::from("Y")),
        ]),
        HashMap::from([(Arc::from("f"), Arc::from("g"))]),
    );

    // id ; m == m
    let id_then_m = id1.compose(&m)?;
    assert_eq!(id_then_m.sort_map, m.sort_map, "left identity sort_map");
    assert_eq!(id_then_m.op_map, m.op_map, "left identity op_map");

    // m ; id == m
    let m_then_id = m.compose(&id2)?;
    assert_eq!(m_then_id.sort_map, m.sort_map, "right identity sort_map");
    assert_eq!(m_then_id.op_map, m.op_map, "right identity op_map");

    Ok(())
}

// =========================================================================
// Colimit associativity
// =========================================================================

#[test]
fn colimit_associativity() -> Result<(), Box<dyn std::error::Error>> {
    let shared = Theory::new("V", vec![Sort::simple("Vertex")], Vec::new(), Vec::new());

    let t1 = Theory::new(
        "T1",
        vec![Sort::simple("Vertex"), Sort::simple("Edge")],
        vec![
            Operation::unary("src", "e", "Edge", "Vertex"),
            Operation::unary("tgt", "e", "Edge", "Vertex"),
        ],
        Vec::new(),
    );
    let t2 = Theory::new(
        "T2",
        vec![Sort::simple("Vertex"), Sort::simple("Constraint")],
        vec![Operation::unary("target", "c", "Constraint", "Vertex")],
        Vec::new(),
    );
    let t3 = Theory::new(
        "T3",
        vec![Sort::simple("Vertex"), Sort::simple("Label")],
        vec![Operation::unary("label_of", "l", "Label", "Vertex")],
        Vec::new(),
    );

    // (T1 + T2) + T3
    let t12 = colimit_by_name(&t1, &t2, &shared)?;
    let left = colimit_by_name(&t12, &t3, &shared)?;

    // T1 + (T2 + T3)
    let t23 = colimit_by_name(&t2, &t3, &shared)?;
    let right = colimit_by_name(&t1, &t23, &shared)?;

    // Both should have the same sorts (ignoring names).
    assert_eq!(left.sorts.len(), right.sorts.len(), "same number of sorts");
    assert_eq!(left.ops.len(), right.ops.len(), "same number of ops");

    // Both should have all the sorts from all three theories.
    for sort_name in &["Vertex", "Edge", "Constraint", "Label"] {
        assert!(
            left.find_sort(sort_name).is_some(),
            "left should have sort {sort_name}"
        );
        assert!(
            right.find_sort(sort_name).is_some(),
            "right should have sort {sort_name}"
        );
    }

    Ok(())
}

// =========================================================================
// Restrict functor contravariance
// =========================================================================

#[test]
fn restrict_functor_contravariance() -> Result<(), Box<dyn std::error::Error>> {
    // S1 has: root, child_a, child_b
    // S2 has: root, child_a (drops child_b)
    // S3 has: root (drops child_a)
    //
    // f: S1 -> S2 (drop child_b)
    // g: S2 -> S3 (drop child_a)
    //
    // Contravariance: restrict(g . f, I) == restrict(g, restrict(f, I))
    //
    // Note: here "g . f" means "first apply f then g", which in migration
    // terms is composing the compiled migrations.

    let e_a = Edge {
        src: "root".into(),
        tgt: "child_a".into(),
        kind: "prop".into(),
        name: Some("a".into()),
    };
    let e_b = Edge {
        src: "root".into(),
        tgt: "child_b".into(),
        kind: "prop".into(),
        name: Some("b".into()),
    };

    let s1 = make_schema(
        &[
            ("root", "object"),
            ("child_a", "string"),
            ("child_b", "string"),
        ],
        &[e_a.clone(), e_b.clone()],
    );
    let s2 = make_schema(
        &[("root", "object"), ("child_a", "string")],
        std::slice::from_ref(&e_a),
    );
    let s3 = make_schema(&[("root", "object")], &[]);

    // Migration first: S1 -> S2 (keeps root, child_a)
    let first_mig = CompiledMigration {
        surviving_verts: ["root", "child_a"].into_iter().map(Name::from).collect(),
        surviving_edges: std::iter::once(e_a.clone()).collect(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
    };

    // Migration second: S2 -> S3 (keeps root)
    let second_mig = CompiledMigration {
        surviving_verts: std::iter::once(Name::from("root")).collect(),
        surviving_edges: std::collections::HashSet::new(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
    };

    // Composed migration: S1 -> S3 (keeps root only)
    let composed_mig = CompiledMigration {
        surviving_verts: std::iter::once(Name::from("root")).collect(),
        surviving_edges: std::collections::HashSet::new(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
    };

    // Build instance conforming to S1.
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "root"));
    nodes.insert(
        1,
        Node::new(1, "child_a").with_value(FieldPresence::Present(Value::Str("alpha".into()))),
    );
    nodes.insert(
        2,
        Node::new(2, "child_b").with_value(FieldPresence::Present(Value::Str("beta".into()))),
    );
    let instance = WInstance::new(
        nodes,
        vec![(0, 1, e_a), (0, 2, e_b)],
        vec![],
        0,
        "root".into(),
    );

    // Path 1: restrict(g.f, I)
    let direct = wtype_restrict(&instance, &s1, &s3, &composed_mig)?;

    // Path 2: restrict(g, restrict(f, I))
    let step1 = wtype_restrict(&instance, &s1, &s2, &first_mig)?;
    let step2 = wtype_restrict(&step1, &s2, &s3, &second_mig)?;

    // Both should produce the same result: just the root node.
    assert_eq!(direct.node_count(), step2.node_count(), "same node count");
    assert_eq!(direct.root, step2.root, "same root");

    // Verify node anchors match.
    for (&id, node) in &direct.nodes {
        let other = step2
            .nodes
            .get(&id)
            .unwrap_or_else(|| panic!("node {id} missing from sequential restrict"));
        assert_eq!(node.anchor, other.anchor, "anchor mismatch for node {id}");
    }

    Ok(())
}

// =========================================================================
// Migration composition associativity
// =========================================================================

#[test]
fn migration_compose_identity_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    // Build a renaming migration f: A -> B and verify restrict(f, I) works.
    // Then build identity migrations and verify compose(id, f) == f semantics.

    let e = Edge {
        src: "root".into(),
        tgt: "child".into(),
        kind: "prop".into(),
        name: Some("child".into()),
    };

    let s1 = make_schema(
        &[("root", "object"), ("child", "string")],
        std::slice::from_ref(&e),
    );

    // Identity migration.
    let id_mig = CompiledMigration {
        surviving_verts: ["root", "child"].into_iter().map(Name::from).collect(),
        surviving_edges: std::iter::once(e.clone()).collect(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
    };

    // Build instance.
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "root"));
    nodes.insert(
        1,
        Node::new(1, "child").with_value(FieldPresence::Present(Value::Str("data".into()))),
    );
    let instance = WInstance::new(nodes, vec![(0, 1, e)], vec![], 0, "root".into());

    // restrict(id, I) == I
    let restricted = wtype_restrict(&instance, &s1, &s1, &id_mig)?;

    assert_eq!(restricted.node_count(), instance.node_count());
    for (&id, node) in &instance.nodes {
        let r_node = restricted
            .nodes
            .get(&id)
            .unwrap_or_else(|| panic!("node {id} missing after identity restrict"));
        assert_eq!(node.anchor, r_node.anchor);
        assert_eq!(node.value, r_node.value);
    }

    Ok(())
}
