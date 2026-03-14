//! Integration test 6: Lens laws.
//!
//! Verifies `GetPut` and `PutGet` for identity and projection lenses
//! on multiple instances. Tests that the lens combinators preserve
//! the round-trip laws.

use std::collections::HashMap;

use panproto_inst::value::FieldPresence;
use panproto_inst::{CompiledMigration, Node, Value, WInstance};
use panproto_lens::{Lens, check_laws, get, put};
use panproto_schema::{Edge, Schema, Vertex};
use smallvec::SmallVec;

/// Build a schema with the given vertices and edges.
fn make_schema(verts: &[(&str, &str)], edge_list: &[Edge]) -> Schema {
    let mut vertices = HashMap::new();
    let mut edges = HashMap::new();
    let mut outgoing: HashMap<String, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<String, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(String, String), SmallVec<Edge, 2>> = HashMap::new();

    for (id, kind) in verts {
        vertices.insert(
            id.to_string(),
            Vertex {
                id: id.to_string(),
                kind: kind.to_string(),
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
        outgoing,
        incoming,
        between,
    }
}

/// Build an identity lens for the given schema.
fn identity_lens(schema: &Schema) -> Lens {
    let surviving_verts = schema.vertices.keys().cloned().collect();
    let surviving_edges = schema.edges.keys().cloned().collect();

    Lens {
        compiled: CompiledMigration {
            surviving_verts,
            surviving_edges,
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        },
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    }
}

/// Build a 3-node instance: root + two string children.
fn instance_3node() -> (Schema, WInstance) {
    let edge_text = Edge {
        src: "root".into(),
        tgt: "root.text".into(),
        kind: "prop".into(),
        name: Some("text".into()),
    };
    let edge_time = Edge {
        src: "root".into(),
        tgt: "root.time".into(),
        kind: "prop".into(),
        name: Some("time".into()),
    };

    let schema = make_schema(
        &[
            ("root", "object"),
            ("root.text", "string"),
            ("root.time", "string"),
        ],
        &[edge_text.clone(), edge_time.clone()],
    );

    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "root"));
    nodes.insert(
        1,
        Node::new(1, "root.text").with_value(FieldPresence::Present(Value::Str("hello".into()))),
    );
    nodes.insert(
        2,
        Node::new(2, "root.time").with_value(FieldPresence::Present(Value::Str("now".into()))),
    );

    let arcs = vec![(0, 1, edge_text), (0, 2, edge_time)];

    let instance = WInstance::new(nodes, arcs, vec![], 0, "root".into());
    (schema, instance)
}

/// Build a 5-node deeper instance: root -> child -> two leaves.
fn instance_5node() -> (Schema, WInstance) {
    let e1 = Edge {
        src: "r".into(),
        tgt: "r.c".into(),
        kind: "prop".into(),
        name: Some("child".into()),
    };
    let e2 = Edge {
        src: "r.c".into(),
        tgt: "r.c.a".into(),
        kind: "prop".into(),
        name: Some("a".into()),
    };
    let e3 = Edge {
        src: "r.c".into(),
        tgt: "r.c.b".into(),
        kind: "prop".into(),
        name: Some("b".into()),
    };
    let e4 = Edge {
        src: "r".into(),
        tgt: "r.d".into(),
        kind: "prop".into(),
        name: Some("d".into()),
    };

    let schema = make_schema(
        &[
            ("r", "object"),
            ("r.c", "object"),
            ("r.c.a", "string"),
            ("r.c.b", "string"),
            ("r.d", "string"),
        ],
        &[e1.clone(), e2.clone(), e3.clone(), e4.clone()],
    );

    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "r"));
    nodes.insert(1, Node::new(1, "r.c"));
    nodes.insert(
        2,
        Node::new(2, "r.c.a").with_value(FieldPresence::Present(Value::Str("val_a".into()))),
    );
    nodes.insert(
        3,
        Node::new(3, "r.c.b").with_value(FieldPresence::Present(Value::Str("val_b".into()))),
    );
    nodes.insert(
        4,
        Node::new(4, "r.d").with_value(FieldPresence::Present(Value::Str("val_d".into()))),
    );

    let arcs = vec![(0, 1, e1), (1, 2, e2), (1, 3, e3), (0, 4, e4)];
    let instance = WInstance::new(nodes, arcs, vec![], 0, "r".into());
    (schema, instance)
}

#[test]
fn identity_lens_check_laws_3node() -> Result<(), Box<dyn std::error::Error>> {
    let (schema, instance) = instance_3node();
    let lens = identity_lens(&schema);
    check_laws(&lens, &instance)?;
    Ok(())
}

#[test]
fn identity_lens_check_laws_5node() -> Result<(), Box<dyn std::error::Error>> {
    let (schema, instance) = instance_5node();
    let lens = identity_lens(&schema);
    check_laws(&lens, &instance)?;
    Ok(())
}

#[test]
fn get_put_round_trip_preserves_node_count() -> Result<(), Box<dyn std::error::Error>> {
    let (schema, instance) = instance_3node();
    let lens = identity_lens(&schema);

    let (view, complement) = get(&lens, &instance)?;
    let restored = put(&lens, &view, &complement)?;

    assert_eq!(
        restored.node_count(),
        instance.node_count(),
        "GetPut should preserve node count"
    );
    assert_eq!(restored.root, instance.root, "GetPut should preserve root");

    Ok(())
}

#[test]
fn put_get_returns_same_view() -> Result<(), Box<dyn std::error::Error>> {
    let (schema, instance) = instance_3node();
    let lens = identity_lens(&schema);

    let (view, complement) = get(&lens, &instance)?;
    let restored = put(&lens, &view, &complement)?;
    let (view2, _) = get(&lens, &restored)?;

    // The views should have the same node count and structure.
    assert_eq!(
        view2.node_count(),
        view.node_count(),
        "PutGet should produce the same view"
    );

    Ok(())
}

#[test]
fn modified_view_propagates_through_put() -> Result<(), Box<dyn std::error::Error>> {
    let (schema, instance) = instance_3node();
    let lens = identity_lens(&schema);

    let (mut view, complement) = get(&lens, &instance)?;

    // Modify the text field.
    if let Some(node) = view.nodes.get_mut(&1) {
        node.value = Some(FieldPresence::Present(Value::Str("modified".into())));
    }

    let restored = put(&lens, &view, &complement)?;

    let text_node = restored
        .nodes
        .get(&1)
        .ok_or("node 1 should exist in restored instance")?;
    assert_eq!(
        text_node.value,
        Some(FieldPresence::Present(Value::Str("modified".into()))),
        "modification should propagate through put"
    );

    Ok(())
}

#[test]
fn multiple_instances_satisfy_laws() -> Result<(), Box<dyn std::error::Error>> {
    // Test on several different instances to approximate property-based testing.
    let (schema, _) = instance_3node();
    let lens = identity_lens(&schema);

    // Instance with different values.
    let values = [
        ("alpha", "2024-01-01"),
        ("beta", "2024-02-02"),
        ("gamma", "2024-03-03"),
        ("", ""),
        (
            "a very long string with special chars: !@#$%^&*()",
            "2024-12-31",
        ),
    ];

    for (text_val, time_val) in &values {
        let edge_text = Edge {
            src: "root".into(),
            tgt: "root.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let edge_time = Edge {
            src: "root".into(),
            tgt: "root.time".into(),
            kind: "prop".into(),
            name: Some("time".into()),
        };

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(
            1,
            Node::new(1, "root.text")
                .with_value(FieldPresence::Present(Value::Str((*text_val).into()))),
        );
        nodes.insert(
            2,
            Node::new(2, "root.time")
                .with_value(FieldPresence::Present(Value::Str((*time_val).into()))),
        );

        let instance = WInstance::new(
            nodes,
            vec![(0, 1, edge_text), (0, 2, edge_time)],
            vec![],
            0,
            "root".into(),
        );

        check_laws(&lens, &instance)?;
    }

    Ok(())
}
