//! Protocols and schemas written before they carried shape, constraint,
//! value-kind and key declarations still decode, with every new field empty.
//!
//! The TypeScript, Python and DSL paths serialize protocols and schemas
//! themselves, so a document from before these fields existed has none of
//! them; each must default rather than fail to decode.

use std::collections::BTreeMap;

use panproto_gat::Name;
use panproto_schema::{
    Cardinality, ConstraintSemantics, Edge, InstanceShapeKind, Key, KeyPart, KeyRole, KeyScope,
    LengthUnit, OnUnresolved, Protocol, RefSource, Reference, ResolveMode, ScalarKind, Schema,
    SchemaBuilder, SchemaDeclarations,
};

/// A protocol as a pre-0.75 writer produced it: no shape, constraint
/// semantics or value kinds.
const PROTOCOL_BEFORE: &str = r#"{
    "name": "demo",
    "schema_theory": "ThDemoSchema",
    "instance_theory": "ThDemoInstance",
    "schema_composition": null,
    "instance_composition": null,
    "edge_rules": [{"edge_kind": "prop", "src_kinds": ["object"], "tgt_kinds": []}],
    "obj_kinds": ["object", "string"],
    "constraint_sorts": ["maxLength"],
    "has_order": true
}"#;

#[test]
fn protocols_serialized_before_0_75_still_decode() -> Result<(), serde_json::Error> {
    let protocol: Protocol = serde_json::from_str(PROTOCOL_BEFORE)?;
    assert_eq!(protocol.name, "demo");
    assert!(protocol.has_order);
    assert_eq!(protocol.syntactic_shape, InstanceShapeKind::Tree);
    assert_eq!(protocol.semantic_shape, InstanceShapeKind::Tree);
    assert!(protocol.constraint_semantics.is_empty());
    assert!(protocol.value_kinds.is_empty());
    Ok(())
}

#[test]
fn the_new_protocol_fields_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = Protocol {
        name: "demo".into(),
        syntactic_shape: InstanceShapeKind::Table,
        semantic_shape: InstanceShapeKind::Graph,
        constraint_semantics: BTreeMap::from([(
            "maxLength".to_owned(),
            ConstraintSemantics::MaxLength(LengthUnit::Utf8Bytes),
        )]),
        value_kinds: BTreeMap::from([(
            "integer".to_owned(),
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        )]),
        ..Default::default()
    };
    let json = serde_json::to_string(&protocol)?;
    assert!(json.contains(r#""syntactic_shape":"table""#), "{json}");
    assert!(json.contains(r#""max_length":"utf8_bytes""#), "{json}");
    assert_eq!(serde_json::from_str::<Protocol>(&json)?, protocol);
    let msgpack = rmp_serde::to_vec(&protocol)?;
    assert_eq!(rmp_serde::from_slice::<Protocol>(&msgpack)?, protocol);
    Ok(())
}

/// A protocol written before it declared its non-value kinds decodes with
/// none, from JSON and from msgpack, and a declared set round-trips.
#[test]
fn a_protocol_without_non_value_kinds_decodes_with_none() -> Result<(), Box<dyn std::error::Error>>
{
    let before: Protocol = serde_json::from_str(PROTOCOL_BEFORE)?;
    assert!(before.non_value_kinds.is_empty());
    let mut declared = before;
    declared.non_value_kinds = std::collections::BTreeSet::from(["object".to_owned()]);
    let json = serde_json::to_string(&declared)?;
    assert!(json.contains(r#""non_value_kinds":["object"]"#), "{json}");
    assert_eq!(serde_json::from_str::<Protocol>(&json)?, declared);
    let msgpack = rmp_serde::to_vec(&declared)?;
    assert_eq!(rmp_serde::from_slice::<Protocol>(&msgpack)?, declared);
    Ok(())
}

/// A schema with one key and one reference on its only vertex pair.
fn declaring_schema() -> Result<Schema, Box<dyn std::error::Error>> {
    let protocol = Protocol {
        name: "demo".into(),
        obj_kinds: vec!["object".into(), "string".into()],
        ..Default::default()
    };
    let mut schema = SchemaBuilder::new(&protocol)
        .vertex("user", "object", None::<&str>)?
        .vertex("user.id", "string", None::<&str>)?
        .edge("user", "user.id", "prop", Some("id"))?
        .build()?;
    let id_edge = Edge {
        src: "user".into(),
        tgt: "user.id".into(),
        kind: "prop".into(),
        name: Some("id".into()),
    };
    let mut declarations = SchemaDeclarations::default();
    declarations.keys.insert(
        Name::from("user"),
        vec![Key {
            id: "pk".into(),
            parts: vec![KeyPart::Path(vec![id_edge.clone()])],
            role: KeyRole::Primary,
            scope: KeyScope::Document,
            equality: vec![],
        }],
    );
    declarations.references.insert(
        Name::from("self_ref"),
        Reference {
            id: "self_ref".into(),
            holder: "user".into(),
            source: RefSource::KeyValue {
                parts: vec![vec![id_edge]],
            },
            target_vertex: "user".into(),
            target_key: Some("pk".into()),
            edge: None,
            cardinality: Cardinality::One,
            nullable: false,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
            guard: None,
            null_match: Default::default(),
            null_vertex: None,
            scope_via: None,
        },
    );
    schema.declarations = declarations;
    Ok(schema)
}

#[test]
fn schema_declarations_round_trip_named_and_positional() -> Result<(), Box<dyn std::error::Error>> {
    let schema = declaring_schema()?;
    assert!(!schema.declarations.is_empty());

    let json = serde_json::to_string(&schema)?;
    assert!(json.contains(r#""declarations":{"keys":{"user""#), "{json}");
    let from_json: Schema = serde_json::from_str(&json)?;
    assert_eq!(from_json.declarations, schema.declarations);

    let msgpack = rmp_serde::to_vec(&schema)?;
    let from_msgpack: Schema = rmp_serde::from_slice(&msgpack)?;
    assert_eq!(from_msgpack.declarations, schema.declarations);
    Ok(())
}

#[test]
fn a_schema_without_declarations_decodes_with_empty_declarations()
-> Result<(), Box<dyn std::error::Error>> {
    let mut schema = declaring_schema()?;
    schema.declarations = SchemaDeclarations::default();

    let json = serde_json::to_string(&schema)?;
    assert!(!json.contains("declarations"), "{json}");
    let from_json: Schema = serde_json::from_str(&json)?;
    assert!(from_json.declarations.is_empty());

    let msgpack = rmp_serde::to_vec(&schema)?;
    let from_msgpack: Schema = rmp_serde::from_slice(&msgpack)?;
    assert!(from_msgpack.declarations.is_empty());
    Ok(())
}

#[test]
fn a_default_schema_is_empty() {
    let schema = Schema::default();
    assert!(schema.vertices.is_empty());
    assert!(schema.edges.is_empty());
    assert!(schema.declarations.is_empty());
}
