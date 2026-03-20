//! Integration test 8: `MessagePack` serialization fidelity.
//!
//! Verifies that core types which cross the WASM boundary survive
//! `MessagePack` round-trips with no data loss. These tests run as
//! native Rust (not inside a WASM runtime) so they exercise the
//! serialization layer only, not actual WASM host↔guest interaction.

use std::collections::{HashMap, HashSet};

use panproto_inst::value::FieldPresence;
use panproto_inst::{CompiledMigration, Node, Value, WInstance};
use panproto_lens::Complement;
use panproto_mig::Migration;
use panproto_schema::{Edge, EdgeRule, Protocol};

#[test]
fn protocol_msgpack_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = Protocol {
        name: "test".into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["object".into()],
            tgt_kinds: vec![],
        }],
        obj_kinds: vec!["object".into(), "string".into()],
        constraint_sorts: vec!["maxLength".into()],
        ..Protocol::default()
    };

    let bytes = rmp_serde::to_vec(&protocol)?;
    let decoded: Protocol = rmp_serde::from_slice(&bytes)?;

    assert_eq!(decoded.name, protocol.name);
    assert_eq!(decoded.schema_theory, protocol.schema_theory);
    assert_eq!(decoded.instance_theory, protocol.instance_theory);
    assert_eq!(decoded.edge_rules.len(), protocol.edge_rules.len());
    assert_eq!(decoded.obj_kinds, protocol.obj_kinds);

    Ok(())
}

#[test]
fn migration_msgpack_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let edge = Edge {
        src: "a".into(),
        tgt: "b".into(),
        kind: "prop".into(),
        name: Some("x".into()),
    };

    let migration = Migration {
        vertex_map: HashMap::from([("a".into(), "a".into()), ("b".into(), "b".into())]),
        edge_map: HashMap::from([(edge.clone(), edge)]),
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        expr_resolvers: HashMap::new(),
    };

    let bytes = rmp_serde::to_vec(&migration)?;
    let decoded: Migration = rmp_serde::from_slice(&bytes)?;

    assert_eq!(decoded.vertex_map.len(), 2);
    assert_eq!(decoded.edge_map.len(), 1);

    Ok(())
}

#[test]
fn winstance_msgpack_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let edge = Edge {
        src: "body".into(),
        tgt: "body.text".into(),
        kind: "prop".into(),
        name: Some("text".into()),
    };

    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "body"));
    nodes.insert(
        1,
        Node::new(1, "body.text").with_value(FieldPresence::Present(Value::Str("hello".into()))),
    );

    let instance = WInstance::new(nodes, vec![(0, 1, edge)], vec![], 0, "body".into());

    let bytes = rmp_serde::to_vec(&instance)?;
    let decoded: WInstance = rmp_serde::from_slice(&bytes)?;

    assert_eq!(decoded.node_count(), 2);
    assert_eq!(decoded.root, 0);
    assert_eq!(decoded.schema_root, "body");

    Ok(())
}

#[test]
fn complement_msgpack_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let dropped_node = Node::new(99, "dropped.field")
        .with_value(FieldPresence::Present(Value::Str("gone".into())));

    let complement = Complement {
        dropped_nodes: HashMap::from([(99, dropped_node)]),
        dropped_arcs: vec![(
            0,
            99,
            Edge {
                src: "body".into(),
                tgt: "dropped.field".into(),
                kind: "prop".into(),
                name: Some("dropped".into()),
            },
        )],
        dropped_fans: vec![],
        contraction_choices: HashMap::new(),
        original_parent: HashMap::from([(99, 0)]),
    };

    let bytes = rmp_serde::to_vec(&complement)?;
    let decoded: Complement = rmp_serde::from_slice(&bytes)?;

    assert_eq!(decoded.dropped_nodes.len(), 1);
    assert!(decoded.dropped_nodes.contains_key(&99));
    assert_eq!(decoded.dropped_arcs.len(), 1);
    assert_eq!(decoded.original_parent.len(), 1);

    Ok(())
}

#[test]
fn compiled_migration_msgpack_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let edge = Edge {
        src: "a".into(),
        tgt: "b".into(),
        kind: "prop".into(),
        name: None,
    };

    let compiled = CompiledMigration {
        surviving_verts: HashSet::from(["a".into(), "b".into()]),
        surviving_edges: HashSet::from([edge]),
        vertex_remap: HashMap::from([("x".into(), "a".into())]),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
    };

    let bytes = rmp_serde::to_vec(&compiled)?;
    let decoded: CompiledMigration = rmp_serde::from_slice(&bytes)?;

    assert_eq!(decoded.surviving_verts.len(), 2);
    assert_eq!(decoded.surviving_edges.len(), 1);
    assert_eq!(decoded.vertex_remap.len(), 1);

    Ok(())
}

#[test]
fn value_types_msgpack_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let values = vec![
        Value::Bool(true),
        Value::Int(42),
        Value::Str("hello".into()),
        Value::Bytes(vec![0xFF, 0x00]),
        Value::CidLink("bafyrei...".into()),
        Value::Null,
        Value::Token("app.bsky.feed.post".into()),
    ];

    for val in &values {
        let bytes = rmp_serde::to_vec(val)?;
        let decoded: Value = rmp_serde::from_slice(&bytes)?;
        assert_eq!(
            &decoded, val,
            "value should round-trip through `MessagePack`"
        );
    }

    Ok(())
}

#[test]
fn large_instance_msgpack_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    // Build a moderately large instance (100 nodes) and verify it
    // survives a `MessagePack` round-trip.
    let mut nodes = HashMap::new();
    let mut arcs = Vec::new();

    nodes.insert(0, Node::new(0, "root"));
    for i in 1..100_u32 {
        let anchor = format!("field_{i}");
        nodes.insert(
            i,
            Node::new(i, anchor.as_str())
                .with_value(FieldPresence::Present(Value::Int(i64::from(i)))),
        );
        let edge = Edge {
            src: "root".into(),
            tgt: anchor.into(),
            kind: "prop".into(),
            name: Some(format!("f{i}").into()),
        };
        arcs.push((0, i, edge));
    }

    let instance = WInstance::new(nodes, arcs, vec![], 0, "root".into());
    assert_eq!(instance.node_count(), 100);

    let bytes = rmp_serde::to_vec(&instance)?;
    let decoded: WInstance = rmp_serde::from_slice(&bytes)?;

    assert_eq!(decoded.node_count(), 100);
    assert_eq!(decoded.arc_count(), 99);

    Ok(())
}
