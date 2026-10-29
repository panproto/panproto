//! Cross-format exact-value corpus for the default-build value pathways.
//!
//! Every exercised route parses a checked-in fixture, changes a value so the
//! emitter cannot replay the input, emits it, and checks the reparsed value or
//! the format's structured refusal. Tree-sitter preserving routes extend this
//! same fixture directory at the final gate.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use panproto_gat::Name;
use panproto_inst::metadata::{Node, NodeShape};
#[cfg(feature = "tree-sitter")]
use panproto_inst::value::{CANONICAL_NAN, NEGATIVE_CANONICAL_NAN};
use panproto_inst::value::{DecimalValue, FieldPresence, Temporal, Value};
use panproto_inst::{FInstance, Fan, FidelityReason, WInstance};
use panproto_io::byte_tabular::{ByteTabularCodec, ByteTabularComplement};
#[cfg(feature = "tree-sitter")]
use panproto_io::error::ParseInstanceError;
use panproto_io::error::{EmitInstanceError, EmitValueReason};
use panproto_io::json_pathway::{emit_json_bytes, parse_json_bytes};
use panproto_io::tabular_pathway::{emit_tsv_with_schema, parse_tsv};
use panproto_io::toml_pathway::emit_toml_bytes;
use panproto_io::xml_pathway::{emit_xml_bytes, parse_xml_bytes};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/value_fidelity")
        .join(relative)
}

fn read_fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixture(relative)).expect("read value-fidelity fixture")
}

fn exact_value_schema(root: &str) -> Schema {
    let protocol = panproto_protocols::database::cassandra::protocol();
    SchemaBuilder::new(&protocol)
        .vertex(root, "table", None)
        .expect("root")
        .vertex("long_decimal", "decimal", None)
        .expect("long decimal")
        .vertex("numeric_38_10", "decimal", None)
        .expect("numeric")
        .vertex("timestamp_offset", "timestamp", None)
        .expect("offset timestamp")
        .vertex("timestamp_local", "timestamp", None)
        .expect("local timestamp")
        .vertex("bytes", "blob", None)
        .expect("bytes")
        .vertex("u64_max", "varint", None)
        .expect("u64 max")
        .vertex("ten_to_29", "varint", None)
        .expect("ten to the twenty-ninth")
        .vertex("huge", "decimal", None)
        .expect("huge decimal")
        .vertex("nan", "double", None)
        .expect("NaN")
        .vertex("null_value", "text", None)
        .expect("null")
        .vertex("empty", "text", None)
        .expect("empty")
        .vertex("absent", "text", None)
        .expect("absent")
        .edge(root, "long_decimal", "prop", Some("long_decimal"))
        .expect("long decimal edge")
        .edge(root, "numeric_38_10", "prop", Some("numeric_38_10"))
        .expect("numeric edge")
        .edge(root, "timestamp_offset", "prop", Some("timestamp_offset"))
        .expect("offset timestamp edge")
        .edge(root, "timestamp_local", "prop", Some("timestamp_local"))
        .expect("local timestamp edge")
        .edge(root, "bytes", "prop", Some("bytes"))
        .expect("bytes edge")
        .edge(root, "u64_max", "prop", Some("u64_max"))
        .expect("u64 edge")
        .edge(root, "ten_to_29", "prop", Some("ten_to_29"))
        .expect("ten-to-29 edge")
        .edge(root, "huge", "prop", Some("huge"))
        .expect("huge edge")
        .edge(root, "nan", "prop", Some("nan"))
        .expect("NaN edge")
        .edge(root, "null_value", "prop", Some("null_value"))
        .expect("null edge")
        .edge(root, "empty", "prop", Some("empty"))
        .expect("empty edge")
        .edge(root, "absent", "prop", Some("absent"))
        .expect("absent edge")
        .constraint("numeric_38_10", "precision", "38")
        .constraint("numeric_38_10", "scale", "10")
        .build()
        .expect("exact value schema")
}

fn open_number_schema() -> Schema {
    let protocol = Protocol {
        name: "fidelity-open".into(),
        obj_kinds: vec!["object".into(), "number".into()],
        ..Protocol::default()
    };
    SchemaBuilder::new(&protocol)
        .vertex("root", "object", None)
        .expect("root")
        .vertex("u64_max", "number", None)
        .expect("u64")
        .vertex("ten_to_29", "number", None)
        .expect("big integer")
        .vertex("huge", "number", None)
        .expect("huge decimal")
        .edge("root", "u64_max", "prop", Some("u64_max"))
        .expect("u64 edge")
        .edge("root", "ten_to_29", "prop", Some("ten_to_29"))
        .expect("big integer edge")
        .edge("root", "huge", "prop", Some("huge"))
        .expect("huge edge")
        .build()
        .expect("open numeric schema")
}

fn value_at<'a>(instance: &'a WInstance, anchor: &str) -> &'a FieldPresence {
    instance
        .nodes
        .values()
        .find(|node| node.anchor.as_ref() == anchor)
        .and_then(|node| node.value.as_ref())
        .expect("value at schema anchor")
}

fn set_tree_value(instance: &mut WInstance, anchor: &str, presence: FieldPresence) {
    instance
        .nodes
        .values_mut()
        .find(|node| node.anchor.as_ref() == anchor)
        .unwrap_or_else(|| panic!("missing node at {anchor}"))
        .value = Some(presence);
}

fn insert_tree_child(
    instance: &mut WInstance,
    schema: &Schema,
    anchor: &str,
    presence: FieldPresence,
) {
    let node_id = instance
        .nodes
        .keys()
        .copied()
        .max()
        .and_then(|id| id.checked_add(1))
        .expect("fresh node id");
    let edge = schema
        .outgoing_edges(&instance.schema_root)
        .iter()
        .find(|edge| edge.tgt.as_ref() == anchor)
        .unwrap_or_else(|| panic!("missing schema edge to {anchor}"))
        .clone();
    let mut node = Node::new(node_id, anchor).with_value(presence);
    node.shape = NodeShape::XmlElement { tag: anchor.into() };
    instance.nodes.insert(node_id, node);
    instance.arcs.push((instance.root, node_id, edge));
    instance.parent_map.insert(node_id, instance.root);
    instance
        .children_map
        .entry(instance.root)
        .or_default()
        .push(node_id);
}

fn with_xml_element_shapes(mut instance: WInstance) -> WInstance {
    for node in instance.nodes.values_mut() {
        if !node.is_xml_text_segment() {
            let tag = node
                .anchor
                .rsplit(':')
                .next()
                .unwrap_or(&node.anchor)
                .to_owned();
            node.shape = NodeShape::XmlElement { tag: tag.into() };
        }
    }
    instance
}

fn table_value<'a>(instance: &'a FInstance, table: &str, column: &str) -> &'a Value {
    instance.rows(table)[0].1.get(column).expect("table value")
}

fn one_child_instance(root: &str, child: &str, presence: FieldPresence) -> WInstance {
    let nodes = HashMap::from([
        (0, Node::new(0, root)),
        (1, Node::new(1, child).with_value(presence)),
    ]);
    let edge = Edge {
        src: Name::from(root),
        tgt: Name::from(child),
        kind: Name::from("prop"),
        name: Some(Name::from(child)),
    };
    WInstance::new(nodes, vec![(0, 1, edge)], Vec::new(), 0, root.into())
}

const EXACT_FINITE_COLUMNS: [&str; 8] = [
    "long_decimal",
    "numeric_38_10",
    "timestamp_offset",
    "timestamp_local",
    "bytes",
    "u64_max",
    "ten_to_29",
    "huge",
];

fn edited_exact_values() -> [(&'static str, Value); 8] {
    [
        (
            "long_decimal",
            Value::Decimal(
                DecimalValue::parse("12345678901234567890.123456790").expect("edited decimal"),
            ),
        ),
        (
            "numeric_38_10",
            Value::Decimal(
                DecimalValue::parse("1234567890123456789012345678.1234567891")
                    .expect("edited numeric"),
            ),
        ),
        (
            "timestamp_offset",
            Value::Temporal(
                Temporal::parse("2026-09-29T12:34:56.789+05:30").expect("edited offset timestamp"),
            ),
        ),
        (
            "timestamp_local",
            Value::Temporal(
                Temporal::parse("2026-09-29T12:34:56.789").expect("edited local timestamp"),
            ),
        ),
        ("bytes", Value::Bytes(vec![0x03, 0x04, 0x05])),
        (
            "u64_max",
            Value::parse_integer("18446744073709551614").expect("edited u64 integer"),
        ),
        (
            "ten_to_29",
            Value::parse_integer("100000000000000000000000000001").expect("edited 10^29 integer"),
        ),
        (
            "huge",
            Value::Decimal(DecimalValue::parse("2e400").expect("edited huge decimal")),
        ),
    ]
}

fn assert_exact_tree_input(instance: &WInstance) {
    assert_exact_tree_input_with_huge(instance, "1e400");
}

/// The exact-value checks of a tree, with `huge` the expected display of the
/// `huge` decimal.
fn assert_exact_tree_input_with_huge(instance: &WInstance, huge: &str) {
    assert!(matches!(
        value_at(instance, "long_decimal"),
        FieldPresence::Present(Value::Decimal(value))
            if value.to_string() == "12345678901234567890.123456789"
    ));
    assert_eq!(
        value_at(instance, "bytes"),
        &FieldPresence::Present(Value::Bytes(vec![0x00, 0x01, 0x02, 0xff]))
    );
    assert!(matches!(
        value_at(instance, "numeric_38_10"),
        FieldPresence::Present(Value::Decimal(value))
            if value.to_string() == "1234567890123456789012345678.1234567890"
    ));
    for (anchor, expected) in [
        ("timestamp_offset", "2026-09-28T12:34:56.789+05:30"),
        ("timestamp_local", "2026-09-28T12:34:56.789"),
    ] {
        assert!(matches!(
            value_at(instance, anchor),
            FieldPresence::Present(Value::Temporal(value)) if value.to_string() == expected
        ));
    }
    for (anchor, expected) in [
        ("u64_max", "18446744073709551615"),
        ("ten_to_29", "100000000000000000000000000000"),
    ] {
        assert!(matches!(
            value_at(instance, anchor),
            FieldPresence::Present(Value::BigInt(value)) if value.to_string() == expected
        ));
    }
    assert!(matches!(
        value_at(instance, "huge"),
        FieldPresence::Present(Value::Decimal(value)) if value.to_string() == huge
    ));
}

fn assert_exact_table_input(instance: &FInstance) {
    assert!(matches!(
        table_value(instance, "rows", "long_decimal"),
        Value::Decimal(value) if value.to_string() == "12345678901234567890.123456789"
    ));
    assert_eq!(
        table_value(instance, "rows", "bytes"),
        &Value::Bytes(vec![0x00, 0x01, 0x02, 0xff])
    );
    assert!(matches!(
        table_value(instance, "rows", "numeric_38_10"),
        Value::Decimal(value)
            if value.to_string() == "1234567890123456789012345678.1234567890"
    ));
    for (column, expected) in [
        ("timestamp_offset", "2026-09-28T12:34:56.789+05:30"),
        ("timestamp_local", "2026-09-28T12:34:56.789"),
    ] {
        assert!(matches!(
            table_value(instance, "rows", column),
            Value::Temporal(value) if value.to_string() == expected
        ));
    }
    for (column, expected) in [
        ("u64_max", "18446744073709551615"),
        ("ten_to_29", "100000000000000000000000000000"),
    ] {
        assert!(matches!(
            table_value(instance, "rows", column),
            Value::BigInt(value) if value.to_string() == expected
        ));
    }
    assert!(matches!(
        table_value(instance, "rows", "huge"),
        Value::Decimal(value) if value.to_string() == "1e400"
    ));
}

#[test]
fn corpus_contains_every_required_text_family_and_literal() {
    for relative in [
        "json/exact.json",
        "json/open_numbers.json",
        "csv/exact.csv",
        "tsv/exact.tsv",
        "yaml/exact.yaml",
        "toml/exact.toml",
        "xml/exact.xml",
        "xml/null.xml",
        "byte_tabular/exact.txt",
    ] {
        assert!(fixture(relative).is_file(), "missing {relative}");
    }

    let exact_literals = [
        "12345678901234567890.123456789",
        "1234567890123456789012345678.1234567890",
        "2026-09-28T12:34:56.789+05:30",
        "2026-09-28T12:34:56.789",
        "18446744073709551615",
        "100000000000000000000000000000",
        "1e400",
    ];
    for path in [
        "json/exact.json",
        "csv/exact.csv",
        "tsv/exact.tsv",
        "yaml/exact.yaml",
        "toml/exact.toml",
        "xml/exact.xml",
        "byte_tabular/exact.txt",
    ] {
        let text = String::from_utf8_lossy(&read_fixture(path)).into_owned();
        for literal in exact_literals {
            assert!(
                text.contains(literal),
                "{path} is missing literal {literal}"
            );
        }
    }
    for path in [
        "csv/exact.csv",
        "tsv/exact.tsv",
        "yaml/exact.yaml",
        "toml/exact.toml",
        "xml/exact.xml",
        "byte_tabular/exact.txt",
    ] {
        let text = String::from_utf8_lossy(&read_fixture(path)).to_ascii_lowercase();
        assert!(text.contains("nan"), "{path} is missing its NaN case");
    }

    let json = String::from_utf8_lossy(&read_fixture("json/exact.json")).into_owned();
    assert!(json.contains("\"null_value\":null"));
    assert!(!json.contains("\"absent\""));
    assert!(!json.to_ascii_lowercase().contains("nan"));
    let yaml = String::from_utf8_lossy(&read_fixture("yaml/exact.yaml")).into_owned();
    assert!(yaml.contains("null_value: null"));
    assert!(!yaml.contains("absent:"));
    let toml = String::from_utf8_lossy(&read_fixture("toml/exact.toml")).into_owned();
    assert!(!toml.contains("null_value"));
    assert!(!toml.contains("absent"));
    let xml = String::from_utf8_lossy(&read_fixture("xml/exact.xml")).into_owned();
    assert!(xml.contains("xsi:nil=\"true\""));
    assert!(xml.contains("<empty/>"));
    let byte_tabular =
        String::from_utf8_lossy(&read_fixture("byte_tabular/exact.txt")).into_owned();
    assert!(byte_tabular.contains("null|NULL"));
    assert!(byte_tabular.contains("absent|_"));
}

#[test]
fn json_exact_values_survive_a_forced_rerender() {
    let schema = exact_value_schema("root");
    let input = read_fixture("json/exact.json");
    let mut instance = parse_json_bytes(&schema, "root", &input, "cassandra").expect("parse JSON");

    assert_exact_tree_input(&instance);
    assert!(matches!(
        value_at(&instance, "null_value"),
        FieldPresence::Null
    ));
    assert!(
        instance
            .nodes
            .values()
            .all(|node| node.anchor.as_ref() != "absent")
    );

    for (anchor, value) in edited_exact_values()
        .into_iter()
        .chain([("null_value", Value::Str("materialized".into()))])
    {
        set_tree_value(&mut instance, anchor, FieldPresence::Present(value));
    }

    let emitted = emit_json_bytes(&schema, &instance, "cassandra").expect("emit JSON");
    let text = std::str::from_utf8(&emitted).expect("JSON UTF-8");
    assert!(text.contains("12345678901234567890.123456790"));
    let reparsed = parse_json_bytes(&schema, "root", &emitted, "cassandra").expect("reparse JSON");
    for anchor in EXACT_FINITE_COLUMNS.into_iter().chain(["null_value"]) {
        assert_eq!(value_at(&reparsed, anchor), value_at(&instance, anchor));
    }
}

#[test]
fn json_open_numbers_are_exact_and_nonfinite_emit_is_typed() {
    let schema = open_number_schema();
    let input = read_fixture("json/open_numbers.json");
    let mut instance =
        parse_json_bytes(&schema, "root", &input, "fidelity-open").expect("parse JSON");
    for (anchor, expected) in [
        ("u64_max", "18446744073709551615"),
        ("ten_to_29", "100000000000000000000000000000"),
    ] {
        assert!(matches!(
            value_at(&instance, anchor),
            FieldPresence::Present(Value::BigInt(value)) if value.to_string() == expected
        ));
    }
    assert!(matches!(
        value_at(&instance, "huge"),
        FieldPresence::Present(Value::Decimal(value)) if value.to_string() == "1e400"
    ));

    for (anchor, value) in [
        (
            "u64_max",
            Value::parse_integer("18446744073709551614").expect("edited u64-sized integer"),
        ),
        (
            "ten_to_29",
            Value::parse_integer("100000000000000000000000000001").expect("edited 10^29 integer"),
        ),
        (
            "huge",
            Value::Decimal(DecimalValue::parse("2e400").expect("edited huge decimal")),
        ),
    ] {
        set_tree_value(&mut instance, anchor, FieldPresence::Present(value));
    }
    let emitted = emit_json_bytes(&schema, &instance, "fidelity-open").expect("emit JSON");
    let reparsed =
        parse_json_bytes(&schema, "root", &emitted, "fidelity-open").expect("reparse JSON");
    for anchor in ["u64_max", "ten_to_29", "huge"] {
        assert_eq!(value_at(&reparsed, anchor), value_at(&instance, anchor));
    }

    let nonfinite = one_child_instance(
        "root",
        "huge",
        FieldPresence::Present(Value::Float(f64::NAN)),
    );
    let error =
        emit_json_bytes(&schema, &nonfinite, "fidelity-open").expect_err("JSON has no NaN literal");
    assert!(matches!(
        error,
        EmitInstanceError::Value(error) if error.reason == EmitValueReason::NonFinite
    ));
}

#[test]
fn canonical_tree_emitters_refuse_unreachable_nodes_and_hyper_edges() {
    type TreeEmitter = fn(&Schema, &WInstance, &str) -> Result<Vec<u8>, EmitInstanceError>;

    let protocol = Protocol {
        name: "fidelity-tree".into(),
        obj_kinds: vec!["object".into(), "value".into()],
        ..Protocol::default()
    };
    let schema = SchemaBuilder::new(&protocol)
        .vertex("root", "object", None)
        .expect("root")
        .vertex("orphan", "value", None)
        .expect("orphan")
        .build()
        .expect("schema");
    let emitters: [(&str, TreeEmitter); 3] = [
        ("JSON", emit_json_bytes),
        ("TOML", emit_toml_bytes),
        ("XML", emit_xml_bytes),
    ];

    let orphan = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "root")),
            (
                1,
                Node::new(1, "orphan")
                    .with_value(FieldPresence::Present(Value::Str("lost".into()))),
            ),
        ]),
        Vec::new(),
        Vec::new(),
        0,
        "root".into(),
    );
    for (format, emit) in emitters {
        let candidate = if format == "XML" {
            with_xml_element_shapes(orphan.clone())
        } else {
            orphan.clone()
        };
        assert!(matches!(
            emit(&schema, &candidate, "fidelity-tree"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains(format) && message.contains("unreachable")
        ));
    }

    let fan = WInstance::new(
        HashMap::from([(0, Node::new(0, "root"))]),
        Vec::new(),
        vec![Fan::new("unrepresentable", 0).with_child("self", 0)],
        0,
        "root".into(),
    );
    for (format, emit) in emitters {
        let candidate = if format == "XML" {
            with_xml_element_shapes(fan.clone())
        } else {
            fan.clone()
        };
        assert!(matches!(
            emit(&schema, &candidate, "fidelity-tree"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains(format) && message.contains("hyper-edge fan")
        ));
    }

    let invented_edge = WInstance::new(
        HashMap::from([(0, Node::new(0, "root")), (1, Node::new(1, "orphan"))]),
        vec![(
            0,
            1,
            Edge {
                src: "root".into(),
                tgt: "orphan".into(),
                kind: "invented".into(),
                name: Some("invented".into()),
            },
        )],
        Vec::new(),
        0,
        "root".into(),
    );
    for (format, emit) in emitters {
        let candidate = if format == "XML" {
            with_xml_element_shapes(invented_edge.clone())
        } else {
            invented_edge.clone()
        };
        assert!(matches!(
            emit(&schema, &candidate, "fidelity-tree"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains(format) && message.contains("not declared")
        ));
    }
}

#[test]
fn canonical_tree_emitters_refuse_a_disconnected_cycle() {
    type TreeEmitter = fn(&Schema, &WInstance, &str) -> Result<Vec<u8>, EmitInstanceError>;

    let protocol = Protocol {
        name: "fidelity-tree".into(),
        obj_kinds: vec!["object".into()],
        ..Protocol::default()
    };
    let schema = SchemaBuilder::new(&protocol)
        .vertex("root", "object", None)
        .expect("root")
        .vertex("b", "object", None)
        .expect("b")
        .vertex("c", "object", None)
        .expect("c")
        .edge("b", "c", "prop", Some("c"))
        .expect("b to c")
        .edge("c", "b", "prop", Some("b"))
        .expect("c to b")
        .build()
        .expect("cycle schema");
    let instance = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "root")),
            (1, Node::new(1, "b")),
            (2, Node::new(2, "c")),
        ]),
        vec![
            (
                1,
                2,
                Edge {
                    src: "b".into(),
                    tgt: "c".into(),
                    kind: "prop".into(),
                    name: Some("c".into()),
                },
            ),
            (
                2,
                1,
                Edge {
                    src: "c".into(),
                    tgt: "b".into(),
                    kind: "prop".into(),
                    name: Some("b".into()),
                },
            ),
        ],
        Vec::new(),
        0,
        "root".into(),
    );
    let emitters: [(&str, TreeEmitter); 3] = [
        ("JSON", emit_json_bytes),
        ("TOML", emit_toml_bytes),
        ("XML", emit_xml_bytes),
    ];
    for (format, emit) in emitters {
        let candidate = if format == "XML" {
            with_xml_element_shapes(instance.clone())
        } else {
            instance.clone()
        };
        assert!(matches!(
            emit(&schema, &candidate, "fidelity-tree"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains(format) && message.contains("root traversal reaches 1 of 3 nodes")
        ));
    }
}

#[test]
fn canonical_value_emitters_bound_adversarial_container_depth() {
    let protocol = Protocol {
        name: "fidelity-depth".into(),
        obj_kinds: vec!["object".into()],
        ..Protocol::default()
    };
    let schema = SchemaBuilder::new(&protocol)
        .vertex("root", "object", None)
        .expect("root")
        .build()
        .expect("schema");
    let mut deep = Value::Null;
    for _ in 0..256 {
        deep = Value::List(vec![deep]);
    }
    let mut json_root = Node::new(0, "root");
    json_root.extra_fields.insert("deep".into(), deep.clone());
    let json_instance = WInstance::new(
        HashMap::from([(0, json_root)]),
        Vec::new(),
        Vec::new(),
        0,
        "root".into(),
    );
    assert!(matches!(
        emit_json_bytes(&schema, &json_instance, "fidelity-depth"),
        Err(EmitInstanceError::Value(error))
            if error.reason == EmitValueReason::NotRepresentable
    ));

    let toml_instance = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "root")),
            (
                1,
                Node::new(1, "root:deep").with_value(FieldPresence::Present(deep)),
            ),
        ]),
        vec![(
            0,
            1,
            Edge {
                src: "root".into(),
                tgt: "root:deep".into(),
                kind: "prop".into(),
                name: Some("deep".into()),
            },
        )],
        Vec::new(),
        0,
        "root".into(),
    );
    assert!(matches!(
        emit_toml_bytes(&schema, &toml_instance, "fidelity-depth"),
        Err(EmitInstanceError::Value(error))
            if error.reason == EmitValueReason::NotRepresentable
    ));
}

fn canonical_chain(levels: usize) -> (Schema, WInstance) {
    assert!(levels > 0);
    let protocol = Protocol {
        name: "fidelity-chain".into(),
        obj_kinds: vec!["object".into()],
        ..Protocol::default()
    };
    let mut builder = SchemaBuilder::new(&protocol)
        .vertex("n0", "object", None)
        .expect("root vertex");
    let mut nodes = HashMap::from([(0, Node::new(0, "n0"))]);
    let mut arcs = Vec::with_capacity(levels.saturating_sub(1));
    for level in 1..levels {
        let parent = format!("n{}", level - 1);
        let child = format!("n{level}");
        builder = builder
            .vertex(&child, "object", None)
            .expect("chain vertex")
            .edge(&parent, &child, "prop", Some(&child))
            .expect("chain edge");
        let node_id = u32::try_from(level).expect("small chain");
        nodes.insert(node_id, Node::new(node_id, child.as_str()));
        arcs.push((
            node_id - 1,
            node_id,
            Edge {
                src: parent.into(),
                tgt: child.clone().into(),
                kind: "prop".into(),
                name: Some(child.into()),
            },
        ));
    }
    (
        builder.build().expect("chain schema"),
        WInstance::new(nodes, arcs, Vec::new(), 0, "n0".into()),
    )
}

#[test]
fn canonical_tree_emitters_accept_128_nodes_and_refuse_129() {
    type TreeEmitter = fn(&Schema, &WInstance, &str) -> Result<Vec<u8>, EmitInstanceError>;
    let emitters = [
        ("JSON", emit_json_bytes as TreeEmitter),
        ("TOML", emit_toml_bytes as TreeEmitter),
        ("XML", emit_xml_bytes as TreeEmitter),
    ];

    let (schema, instance) = canonical_chain(128);
    for (format, emit) in emitters {
        let candidate = if format == "XML" {
            with_xml_element_shapes(instance.clone())
        } else {
            instance.clone()
        };
        emit(&schema, &candidate, "fidelity-chain")
            .unwrap_or_else(|error| panic!("{format} rejected the 128-node boundary: {error}"));
    }

    let (schema, instance) = canonical_chain(129);
    for (format, emit) in emitters {
        let candidate = if format == "XML" {
            with_xml_element_shapes(instance.clone())
        } else {
            instance.clone()
        };
        assert!(matches!(
            emit(&schema, &candidate, "fidelity-chain"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains(format) && message.contains("128 nodes")
        ));
    }
}

#[test]
fn delimited_exact_values_survive_forced_rerenders() {
    let schema = exact_value_schema("rows");
    for (relative, delimiter) in [("tsv/exact.tsv", b'\t'), ("csv/exact.csv", b',')] {
        let format = if delimiter == b'\t' { "tsv" } else { "csv" };
        let input = read_fixture(relative);
        // A delimited cell reads numerals in the JSON grammar, which has no
        // NaN: the fixture's `NaN` cell is the one typed refusal.
        assert!(matches!(
            parse_tsv(&schema, &input, "cassandra", "rows", delimiter, None),
            Err(panproto_io::error::ParseInstanceError::Fidelity(ref errors))
                if matches!(errors.as_slice(), [error]
                    if error.site.vertex.as_ref() == "nan"
                        && error.reason == FidelityReason::NotANumber)
        ));
        let separator = char::from(delimiter);
        let input = String::from_utf8(input)
            .expect("fixture UTF-8")
            .replace(
                &format!("{separator}NaN{separator}"),
                &format!("{separator}0.5{separator}"),
            )
            .into_bytes();
        let instance =
            parse_tsv(&schema, &input, "cassandra", "rows", delimiter, None).expect("parse table");
        assert_exact_table_input(&instance);
        assert!(matches!(
            table_value(&instance, "rows", "nan"),
            Value::Float(value) if value.to_bits() == 0.5_f64.to_bits()
        ));
        assert_eq!(
            instance.rows("rows")[0].1.get("null_value"),
            Some(&Value::Str("_".into()))
        );
        assert_eq!(
            instance.rows("rows")[0].1.get("absent"),
            Some(&Value::Str("_".into()))
        );

        let mut edited = instance.clone();
        let row = edited.table("rows").expect("rows table").ids()[0];
        let edited_row = edited.row_mut(row).expect("row");
        for (column, value) in edited_exact_values().into_iter().chain([
            ("null_value", Value::Str("materialized-null".into())),
            ("absent", Value::Str("materialized-absent".into())),
        ]) {
            edited_row.insert(column.into(), value);
        }
        let emitted = emit_tsv_with_schema(&schema, &edited, "cassandra", "rows", delimiter)
            .expect("emit table");
        assert!(
            std::str::from_utf8(&emitted)
                .expect("table UTF-8")
                .contains("12345678901234567890.123456790")
        );
        let reparsed =
            parse_tsv(&schema, &emitted, "cassandra", "rows", delimiter, None).expect("reparse");
        assert!(matches!(
            table_value(&reparsed, "rows", "nan"),
            Value::Float(value) if value.to_bits() == 0.5_f64.to_bits()
        ));
        for column in EXACT_FINITE_COLUMNS
            .into_iter()
            .chain(["null_value", "absent"])
        {
            assert_eq!(
                table_value(&reparsed, "rows", column),
                table_value(&edited, "rows", column)
            );
        }

        let mut with_nan = edited.clone();
        with_nan
            .row_mut(row)
            .expect("row")
            .insert("nan".into(), Value::Float(f64::NAN));
        assert!(matches!(
            emit_tsv_with_schema(&schema, &with_nan, "cassandra", "rows", delimiter),
            Err(EmitInstanceError::Value(ref error))
                if error.reason == EmitValueReason::NonFinite && error.format == format
        ));

        edited
            .row_mut(row)
            .expect("row")
            .insert("null_value".into(), Value::Null);
        let error = emit_tsv_with_schema(&schema, &edited, "cassandra", "rows", delimiter)
            .expect_err("null token is not configured");
        assert!(matches!(
            error,
            EmitInstanceError::Value(error)
                if error.reason == EmitValueReason::NullNotRepresentable
                    && error.format == format
        ));
    }
}

#[test]
fn xml_distinguishes_null_empty_and_absent_then_rerenders() {
    let schema = exact_value_schema("doc");
    let input = read_fixture("xml/exact.xml");
    // XML text reads numerals in the XML Schema grammar, whose decimal has no
    // exponent: the fixture's `1e400` is the one typed refusal.
    assert!(matches!(
        parse_xml_bytes(&schema, &input, "cassandra"),
        Err(panproto_io::error::ParseInstanceError::Fidelity(ref errors))
            if matches!(errors.as_slice(), [error]
                if error.site.vertex.as_ref() == "huge"
                    && error.reason == FidelityReason::NotANumber)
    ));
    let positional = format!("1{}", "0".repeat(400));
    let input = String::from_utf8(input)
        .expect("fixture UTF-8")
        .replace("<huge>1e400</huge>", &format!("<huge>{positional}</huge>"))
        .into_bytes();
    let mut instance = parse_xml_bytes(&schema, &input, "cassandra").expect("parse XML");
    let huge = DecimalValue::parse(&positional)
        .expect("positional decimal")
        .to_string();
    assert_exact_tree_input_with_huge(&instance, &huge);
    assert!(matches!(
        value_at(&instance, "null_value"),
        FieldPresence::Null
    ));
    let empty = instance
        .nodes
        .values()
        .find(|node| node.anchor.as_ref() == "empty")
        .expect("empty element");
    assert!(empty.value.is_none());
    assert!(
        instance
            .nodes
            .values()
            .all(|node| node.anchor.as_ref() != "absent")
    );

    assert!(matches!(
        value_at(&instance, "u64_max"),
        FieldPresence::Present(Value::BigInt(value))
            if value.to_string() == "18446744073709551615"
    ));
    assert!(matches!(
        value_at(&instance, "ten_to_29"),
        FieldPresence::Present(Value::BigInt(value))
            if value.to_string() == "100000000000000000000000000000"
    ));
    assert!(matches!(
        value_at(&instance, "nan"),
        FieldPresence::Present(Value::Float(value)) if value.is_nan()
    ));

    for (anchor, value) in edited_exact_values().into_iter().chain([
        ("null_value", Value::Str("materialized-null".into())),
        ("empty", Value::Str("materialized-empty".into())),
    ]) {
        set_tree_value(&mut instance, anchor, FieldPresence::Present(value));
    }
    instance
        .nodes
        .values_mut()
        .find(|node| node.anchor.as_ref() == "null_value")
        .expect("materialized null node")
        .extra_fields
        .insert("xsi:nil".into(), Value::Str("false".into()));
    insert_tree_child(
        &mut instance,
        &schema,
        "absent",
        FieldPresence::Present(Value::Str("materialized-absent".into())),
    );
    let emitted = emit_xml_bytes(&schema, &instance, "cassandra").expect("emit XML");
    let text = std::str::from_utf8(&emitted).expect("XML UTF-8");
    assert!(text.contains("xsi:nil=\"false\""));
    assert!(text.contains(">materialized-null</null_value>"));
    let reparsed = parse_xml_bytes(&schema, &emitted, "cassandra").expect("reparse XML");
    assert!(matches!(
        value_at(&reparsed, "nan"),
        FieldPresence::Present(Value::Float(value))
            if value.is_nan() && !value.is_sign_negative()
    ));
    for anchor in EXACT_FINITE_COLUMNS
        .into_iter()
        .chain(["null_value", "empty", "absent"])
    {
        assert_eq!(value_at(&reparsed, anchor), value_at(&instance, anchor));
    }
}

#[test]
fn canonical_xml_normalizes_literal_eols_and_preserves_semantic_carriage_returns() {
    let protocol = Protocol {
        name: "fidelity-xml".into(),
        obj_kinds: vec!["object".into()],
        ..Protocol::default()
    };
    let schema = SchemaBuilder::new(&protocol)
        .vertex("root", "object", None)
        .expect("root")
        .build()
        .expect("schema");
    let input = b"<root note=\"a\tb\nc\r\nd&#x9;e\">a\r\nb\rc&#xD;d</root>";
    let mut instance = parse_xml_bytes(&schema, input, "fidelity-xml").expect("parse XML EOLs");
    assert_eq!(
        instance
            .node(instance.root)
            .and_then(|node| node.value.as_ref()),
        Some(&FieldPresence::Present(Value::Str("a\nb\nc\rd".into())))
    );
    assert_eq!(
        instance
            .node(instance.root)
            .and_then(|node| node.extra_fields.get("note")),
        Some(&Value::Str("a b c d\te".into()))
    );
    let root = instance.nodes.get_mut(&instance.root).expect("root");
    root.value = Some(FieldPresence::Present(Value::Str("x\ry".into())));
    root.extra_fields
        .insert("note".into(), Value::Str("a\tb\nc\rd".into()));
    let emitted = emit_xml_bytes(&schema, &instance, "fidelity-xml").expect("emit XML EOLs");
    let text = String::from_utf8(emitted.clone()).expect("XML UTF-8");
    assert!(text.contains("x&#xD;y"));
    assert!(text.contains("note=\"a&#x9;b&#xA;c&#xD;d\""));
    let reparsed = parse_xml_bytes(&schema, &emitted, "fidelity-xml").expect("reparse XML EOLs");
    assert_eq!(
        reparsed
            .node(reparsed.root)
            .and_then(|node| node.value.as_ref()),
        instance
            .node(instance.root)
            .and_then(|node| node.value.as_ref())
    );
    assert_eq!(
        reparsed
            .node(reparsed.root)
            .and_then(|node| node.extra_fields.get("note")),
        instance
            .node(instance.root)
            .and_then(|node| node.extra_fields.get("note"))
    );
}

#[test]
fn toml_native_values_rerender_or_return_a_typed_refusal() {
    let protocol = Protocol {
        name: "fidelity-toml".into(),
        obj_kinds: vec!["object".into(), "value".into()],
        ..Protocol::default()
    };
    let schema = SchemaBuilder::new(&protocol)
        .vertex("root", "object", None)
        .expect("root")
        .vertex("value", "value", None)
        .expect("value")
        .edge("root", "value", "prop", Some("value"))
        .expect("edge")
        .build()
        .expect("TOML schema");

    let temporal = Temporal::parse("2026-09-28T12:34:56.789+05:30").expect("timestamp");
    let temporal_instance = one_child_instance(
        "root",
        "value",
        FieldPresence::Present(Value::Temporal(temporal)),
    );
    let emitted =
        emit_toml_bytes(&schema, &temporal_instance, "fidelity-toml").expect("emit timestamp");
    assert!(
        std::str::from_utf8(&emitted)
            .expect("TOML UTF-8")
            .contains("2026-09-28T12:34:56.789+05:30")
    );

    let nan_instance = one_child_instance(
        "root",
        "value",
        FieldPresence::Present(Value::Float(f64::NAN)),
    );
    let emitted = emit_toml_bytes(&schema, &nan_instance, "fidelity-toml").expect("emit NaN");
    assert!(
        std::str::from_utf8(&emitted)
            .expect("TOML UTF-8")
            .contains("nan")
    );

    let decimal_instance = one_child_instance(
        "root",
        "value",
        FieldPresence::Present(Value::Decimal(
            DecimalValue::parse("12345678901234567890.123456789").expect("decimal"),
        )),
    );
    let emitted = emit_toml_bytes(&schema, &decimal_instance, "fidelity-toml")
        .expect("TOML writes the exact decimal token");
    assert!(
        std::str::from_utf8(&emitted)
            .expect("TOML UTF-8")
            .contains("12345678901234567890.123456789")
    );
}

fn assert_byte_tabular_refusals(
    codec: &ByteTabularCodec,
    instance: &FInstance,
    complement: &ByteTabularComplement,
    rows: &[panproto_inst::RowId],
) {
    let mut refusals: Vec<_> = edited_exact_values()
        .into_iter()
        .map(|(_, value)| (value, EmitValueReason::NotRepresentable))
        .collect();
    refusals.extend([
        (Value::Float(1.25), EmitValueReason::NotRepresentable),
        (Value::Float(f64::NAN), EmitValueReason::NonFinite),
        (
            Value::Bytes(b"valid UTF-8".to_vec()),
            EmitValueReason::NotRepresentable,
        ),
        (Value::Null, EmitValueReason::NullNotRepresentable),
    ]);
    for (value, reason) in refusals {
        let mut invalid = instance.clone();
        invalid
            .row_mut(rows[0])
            .expect("value row")
            .insert("col_1".into(), value);
        let error = codec.emit(&invalid, complement).expect_err("typed refusal");
        assert!(matches!(error, EmitInstanceError::Value(error) if error.reason == reason));
    }

    let mut absent = instance.clone();
    absent.row_mut(rows[0]).expect("value row").remove("col_1");
    let error = codec
        .emit(&absent, complement)
        .expect_err("a positional byte cell cannot be omitted");
    assert!(matches!(
        error,
        EmitInstanceError::Value(error)
            if error.reason == EmitValueReason::AbsentNotRepresentable
    ));

    for reserved in ["left|right", "left\rright", "left\nright"] {
        let mut invalid = instance.clone();
        invalid
            .row_mut(rows[0])
            .expect("first row")
            .insert("col_1".into(), Value::Str(reserved.into()));
        let error = codec
            .emit(&invalid, complement)
            .expect_err("reserved byte-tabular spelling must be refused");
        assert!(matches!(
            error,
            EmitInstanceError::Value(error)
                if error.reason == EmitValueReason::NotRepresentable
        ));
    }
}

#[test]
fn byte_tabular_values_survive_a_forced_rerender() {
    let codec = ByteTabularCodec::new("fidelity", "rows", b'|', None);
    let unchanged_reserved = b"marker|_\n";
    let (reserved_instance, reserved_complement) = codec
        .parse(unchanged_reserved)
        .expect("parse unchanged reserved cell");
    assert_eq!(
        codec
            .emit(&reserved_instance, &reserved_complement)
            .expect("replay unchanged reserved cell"),
        unchanged_reserved
    );
    let mut retyped = reserved_instance;
    let reserved_row = retyped.table("rows").expect("reserved row").ids()[0];
    retyped
        .row_mut(reserved_row)
        .expect("reserved row")
        .insert("col_0".into(), Value::Bytes(b"marker".to_vec()));
    let error = codec
        .emit(&retyped, &reserved_complement)
        .expect_err("valid UTF-8 bytes would reparse as a string");
    assert!(matches!(
        error,
        EmitInstanceError::Value(error)
            if error.reason == EmitValueReason::NotRepresentable
    ));

    let mut input = read_fixture("byte_tabular/exact.txt");
    input.extend_from_slice(b"bytes|\x00\x01\x02\xff\n");
    let (instance, complement) = codec.parse(&input).expect("parse byte table");
    let rows = instance.table("rows").expect("rows").ids();
    let expected_fixture_rows = [
        ("decimal", "12345678901234567890.123456789"),
        ("numeric_38_10", "1234567890123456789012345678.1234567890"),
        ("timestamp", "2026-09-28T12:34:56.789+05:30"),
        ("timestamp_local", "2026-09-28T12:34:56.789"),
        ("bytes", "000102ff"),
        ("u64_max", "18446744073709551615"),
        ("ten_to_29", "100000000000000000000000000000"),
        ("huge", "1e400"),
        ("nan", "NaN"),
        ("null", "NULL"),
        ("absent", "_"),
    ];
    for (row_id, (key, value)) in rows.iter().zip(expected_fixture_rows) {
        let row = instance.row(*row_id).expect("fixture row");
        assert_eq!(row.get("col_0"), Some(&Value::Str(key.into())));
        assert_eq!(row.get("col_1"), Some(&Value::Str(value.into())));
    }
    assert_eq!(
        instance
            .row(*rows.last().expect("raw byte row"))
            .and_then(|row| row.get("col_1")),
        Some(&Value::Bytes(vec![0x00, 0x01, 0x02, 0xff]))
    );

    let mut edited = instance.clone();
    edited
        .row_mut(rows[0])
        .expect("first value row")
        .insert("col_1".into(), Value::Str("materialized-text".into()));
    edited
        .row_mut(*rows.last().expect("raw byte row"))
        .expect("raw byte row")
        .insert("col_1".into(), Value::Bytes(vec![0x03, 0x04, 0xff]));
    let emitted = codec.emit(&edited, &complement).expect("emit byte table");
    for expected in [b"materialized-text".as_slice(), &[0x03, 0x04, 0xff]] {
        assert!(
            emitted
                .windows(expected.len())
                .any(|window| window == expected)
        );
    }
    let (reparsed, _) = codec.parse(&emitted).expect("reparse byte table");
    assert_eq!(
        reparsed.row(rows[0]).and_then(|row| row.get("col_1")),
        Some(&Value::Str("materialized-text".into()))
    );
    assert_eq!(
        reparsed
            .row(*rows.last().expect("raw byte row"))
            .and_then(|row| row.get("col_1")),
        Some(&Value::Bytes(vec![0x03, 0x04, 0xff]))
    );
    for (row_id, (key, value)) in rows
        .iter()
        .skip(1)
        .zip(expected_fixture_rows.into_iter().skip(1))
    {
        let row = reparsed.row(*row_id).expect("reparsed fixture row");
        assert_eq!(row.get("col_0"), Some(&Value::Str(key.into())));
        assert_eq!(row.get("col_1"), Some(&Value::Str(value.into())));
    }
    assert_byte_tabular_refusals(&codec, &instance, &complement, &rows);
}

#[cfg(feature = "tree-sitter")]
mod preserving {
    use super::*;
    use panproto_inst::{NodeShape, RowId};
    use panproto_io::cst_extract::CstComplement;
    use panproto_io::traits::{InstanceEmitter, InstanceParser};
    use panproto_io::unified_codec::UnifiedCodec;

    fn edited_decimal() -> Value {
        Value::Decimal(
            DecimalValue::parse("12345678901234567890.123456790").expect("edited exact decimal"),
        )
    }

    /// One edited value for each exact-value anchor every forced-rerender
    /// fixture carries, each different from the value the fixture holds.
    fn exact_value_edits() -> Vec<(&'static str, Value)> {
        vec![
            ("long_decimal", edited_decimal()),
            (
                "numeric_38_10",
                Value::Decimal(
                    DecimalValue::parse("1234567890123456789012345678.1234567891")
                        .expect("edited numeric"),
                ),
            ),
            (
                "timestamp_offset",
                Value::Temporal(
                    Temporal::parse("2026-09-29T12:34:56.789+05:30")
                        .expect("edited offset timestamp"),
                ),
            ),
            (
                "timestamp_local",
                Value::Temporal(
                    Temporal::parse("2026-09-29T12:34:56.789").expect("edited local timestamp"),
                ),
            ),
            ("bytes", Value::Bytes(vec![0x03, 0x04, 0x05])),
            (
                "u64_max",
                Value::parse_integer("18446744073709551614").expect("edited u64 integer"),
            ),
            (
                "ten_to_29",
                Value::parse_integer("100000000000000000000000000001")
                    .expect("edited 10^29 integer"),
            ),
            (
                "huge",
                Value::Decimal(DecimalValue::parse("2e400").expect("edited huge decimal")),
            ),
        ]
    }

    /// The fixture's integers beyond `i64` parse as big integers and its
    /// decimal beyond `f64` as an exact decimal.
    fn assert_big_numbers_parsed(instance: &WInstance) {
        assert!(matches!(
            value_at(instance, "u64_max"),
            FieldPresence::Present(Value::BigInt(value))
                if value.to_string() == "18446744073709551615"
        ));
        assert!(matches!(
            value_at(instance, "ten_to_29"),
            FieldPresence::Present(Value::BigInt(value))
                if value.to_string() == "100000000000000000000000000000"
        ));
        assert!(matches!(
            value_at(instance, "huge"),
            FieldPresence::Present(Value::Decimal(value)) if value.to_string() == "1e400"
        ));
    }

    fn open_yaml_schema() -> Schema {
        let protocol = Protocol {
            name: "fidelity-cst".into(),
            obj_kinds: vec!["object".into()],
            ..Protocol::default()
        };
        SchemaBuilder::new(&protocol)
            .vertex("root", "object", None)
            .expect("root")
            .build()
            .expect("open YAML schema")
    }

    #[test]
    fn yaml_cst_decodes_quoted_block_and_tagged_scalars_before_an_edit() {
        let schema = open_yaml_schema();
        let codec = UnifiedCodec::yaml("fidelity-cst").expect("YAML CST codec");
        let input = br#"quoted: "line\nfeed"
block: |-
  first
  second
binary: !!binary AAEC/w==
block_binary: !!binary |
  AAEC
  /w==
timestamp: !!timestamp 2026-09-28T12:34:56.789+05:30
"#;
        let (mut instance, complement) = codec
            .parse_wtype_preserving(&schema, input)
            .expect("parse YAML CST");
        assert_eq!(
            value_at(&instance, "root:quoted"),
            &FieldPresence::Present(Value::Str("line\nfeed".into()))
        );
        assert_eq!(
            value_at(&instance, "root:block"),
            &FieldPresence::Present(Value::Str("first\nsecond".into()))
        );
        assert_eq!(
            value_at(&instance, "root:binary"),
            &FieldPresence::Present(Value::Bytes(vec![0x00, 0x01, 0x02, 0xff]))
        );
        assert_eq!(
            value_at(&instance, "root:block_binary"),
            &FieldPresence::Present(Value::Bytes(vec![0x00, 0x01, 0x02, 0xff]))
        );
        assert!(matches!(
            value_at(&instance, "root:timestamp"),
            FieldPresence::Present(Value::Temporal(value))
                if value.to_string() == "2026-09-28T12:34:56.789+05:30"
        ));

        set_tree_value(
            &mut instance,
            "root:block",
            FieldPresence::Present(Value::Str("edited block value".into())),
        );
        let emitted = codec
            .emit_wtype_preserving(&schema, &instance, &complement)
            .expect("emit edited YAML CST");
        let (reparsed, _) = codec
            .parse_wtype_preserving(&schema, &emitted)
            .expect("reparse edited YAML CST");
        assert_eq!(
            value_at(&reparsed, "root:block"),
            value_at(&instance, "root:block")
        );
    }

    #[test]
    fn toml_and_yaml_cst_decode_format_local_radix_integers_exactly() {
        let schema = open_yaml_schema();
        let toml = UnifiedCodec::toml("fidelity-cst").expect("TOML CST codec");
        let toml_input = b"hex = 0x10\noct = 0o20\nbin = 0b10000\n";
        let (mut toml_instance, complement) = toml
            .parse_wtype_preserving(&schema, toml_input)
            .expect("parse TOML radix integers");
        for anchor in ["root:hex", "root:oct", "root:bin"] {
            assert_eq!(
                value_at(&toml_instance, anchor),
                &FieldPresence::Present(Value::Int(16))
            );
        }
        assert_eq!(
            toml.emit_wtype_preserving(&schema, &toml_instance, &complement)
                .expect("replay TOML radix integers"),
            toml_input
        );
        set_tree_value(
            &mut toml_instance,
            "root:hex",
            FieldPresence::Present(Value::Int(17)),
        );
        let emitted = toml
            .emit_wtype_preserving(&schema, &toml_instance, &complement)
            .expect("edit TOML radix integer");
        let (reparsed, _) = toml
            .parse_wtype_preserving(&schema, &emitted)
            .expect("reparse edited TOML integer");
        assert_eq!(
            value_at(&reparsed, "root:hex"),
            &FieldPresence::Present(Value::Int(17))
        );

        let yaml = UnifiedCodec::yaml("fidelity-cst").expect("YAML CST codec");
        let yaml_input = b"hex: 0x10\noct: 0o20\nbin: 0b10000\n";
        let (yaml_instance, complement) = yaml
            .parse_wtype_preserving(&schema, yaml_input)
            .expect("parse YAML radix-like scalars");
        for anchor in ["root:hex", "root:oct"] {
            assert_eq!(
                value_at(&yaml_instance, anchor),
                &FieldPresence::Present(Value::Int(16))
            );
        }
        assert_eq!(
            value_at(&yaml_instance, "root:bin"),
            &FieldPresence::Present(Value::Str("0b10000".into()))
        );
        assert_eq!(
            yaml.emit_wtype_preserving(&schema, &yaml_instance, &complement)
                .expect("replay YAML radix-like scalars"),
            yaml_input
        );
    }

    #[test]
    fn yaml_tagged_containers_receive_a_named_typed_refusal() {
        let open = open_yaml_schema();
        let codec = UnifiedCodec::yaml("fidelity-cst").expect("YAML CST codec");
        for (input, kind) in [
            (b"x: !foo {a: 1}\n".as_slice(), "mapping"),
            (b"x: !foo [1, 2]\n".as_slice(), "sequence"),
        ] {
            assert!(matches!(
                codec.parse_wtype_preserving(&open, input),
                Err(ParseInstanceError::UnsupportedTaggedContainer {
                    ref tag,
                    ref container_kind,
                    byte_offset: Some(3),
                    ..
                }) if tag == "!foo" && container_kind == kind
            ));
        }

        let protocol = Protocol {
            name: "fidelity-cst".into(),
            obj_kinds: vec!["object".into()],
            ..Protocol::default()
        };
        let declared = SchemaBuilder::new(&protocol)
            .vertex("root", "object", None)
            .expect("root")
            .vertex("x", "object", None)
            .expect("x")
            .edge("root", "x", "prop", Some("x"))
            .expect("x edge")
            .build()
            .expect("declared schema");
        assert!(matches!(
            codec.parse_wtype_preserving(&declared, b"x: !foo {a: 1}\n"),
            Err(ParseInstanceError::UnsupportedTaggedContainer { .. })
        ));

        let partial = SchemaBuilder::new(&protocol)
            .vertex("root", "object", None)
            .expect("root")
            .vertex("known", "object", None)
            .expect("known")
            .edge("root", "known", "prop", Some("known"))
            .expect("known edge")
            .build()
            .expect("partial schema");
        assert!(matches!(
            codec.parse_wtype_preserving(&partial, b"known: ok\nx: !foo [1]\n"),
            Err(ParseInstanceError::UnsupportedTaggedContainer { .. })
        ));

        let (scalar, _) = codec
            .parse_wtype_preserving(&open, b"x: !foo value\n")
            .expect("tagged scalar remains representable");
        assert!(matches!(
            value_at(&scalar, "root:x"),
            FieldPresence::Present(Value::Opaque { type_, .. }) if type_ == "!foo"
        ));
    }

    /// The canonical (non-preserving) route of a unified CSV (`csv`) or
    /// TSV codec: the empty table and an exact value round-trip, and a
    /// quote in a cell or a header is refused by CSV and ordinary text in
    /// TSV.
    fn assert_unified_tabular_canonical(
        codec: &UnifiedCodec,
        csv: bool,
        schema: &Schema,
        instance: &FInstance,
    ) {
        let empty_emitted = codec
            .emit_functor(schema, &FInstance::new())
            .expect("schema-guided empty canonical emit");
        assert_eq!(
            codec
                .parse_functor(schema, &empty_emitted)
                .expect("reparse schema-guided empty canonical emit")
                .total_row_count(),
            0
        );
        let emitted = codec
            .emit_functor(schema, instance)
            .expect("schema-guided canonical emit");
        let reparsed = codec
            .parse_functor(schema, &emitted)
            .expect("schema-guided canonical reparse");
        let (_, row) = reparsed.rows("rows")[0];
        assert_eq!(row.get("u64_max"), Some(&Value::Int(1)));

        let quoted_cell = FInstance::new()
            .with_table(
                "rows",
                vec![HashMap::from([(
                    "null_value".to_owned(),
                    Value::Str("\"wrapped\"".into()),
                )])],
            )
            .expect("quoted-cell table");
        if csv {
            assert!(matches!(
                codec.emit_functor(schema, &quoted_cell),
                Err(EmitInstanceError::Value(error))
                    if error.reason == EmitValueReason::NotRepresentable
            ));
        } else {
            let emitted = codec
                .emit_functor(schema, &quoted_cell)
                .expect("a quote is ordinary TSV cell text");
            let reparsed = codec
                .parse_functor(schema, &emitted)
                .expect("reparse TSV quoted text");
            assert_eq!(
                reparsed.rows("rows")[0].1.get("null_value"),
                Some(&Value::Str("\"wrapped\"".into()))
            );
        }

        let quoted_header = FInstance::new()
            .with_table(
                "rows",
                vec![HashMap::from([(
                    "\"quoted\"".to_owned(),
                    Value::Str("value".into()),
                )])],
            )
            .expect("quoted-header table");
        if csv {
            assert!(matches!(
                codec.emit_functor(schema, &quoted_header),
                Err(EmitInstanceError::Emit { message, .. })
                    if message.contains("header")
            ));
        } else {
            let emitted = codec
                .emit_functor(schema, &quoted_header)
                .expect("a quote is ordinary TSV header text");
            let reparsed = codec
                .parse_functor(schema, &emitted)
                .expect("reparse TSV quoted header");
            assert_eq!(
                reparsed.rows("rows")[0].1.get("\"quoted\""),
                Some(&Value::Str("value".into()))
            );
        }
    }

    #[test]
    #[expect(
        deprecated,
        reason = "the deprecated public codec remains an explicit compatibility seam"
    )]
    fn public_tabular_codecs_preserve_exact_value_variants() {
        let schema = exact_value_schema("rows");
        let instance = FInstance::new()
            .with_table(
                "rows",
                vec![HashMap::from([("u64_max".to_owned(), Value::Int(1))])],
            )
            .expect("table");
        for (codec, csv) in [
            (UnifiedCodec::csv("cassandra").expect("CSV codec"), true),
            (
                UnifiedCodec::tsv("cassandra", "rows").expect("TSV codec"),
                false,
            ),
        ] {
            assert_unified_tabular_canonical(&codec, csv, &schema, &instance);
        }

        for codec in [
            panproto_io::tabular_codec::TabularCodec::csv("cassandra", "rows"),
            panproto_io::tabular_codec::TabularCodec::tsv("cassandra", "rows"),
        ] {
            let empty_emitted = codec
                .emit_functor(&schema, &FInstance::new())
                .expect("legacy public codec empty canonical emit");
            assert_eq!(
                codec
                    .parse_functor(&schema, &empty_emitted)
                    .expect("legacy public codec reparse empty canonical emit")
                    .total_row_count(),
                0
            );
            let emitted = codec
                .emit_functor(&schema, &instance)
                .expect("legacy public codec schema-guided emit");
            let reparsed = codec
                .parse_functor(&schema, &emitted)
                .expect("legacy public codec schema-guided reparse");
            assert_eq!(
                reparsed.rows("rows")[0].1.get("u64_max"),
                Some(&Value::Int(1))
            );
        }
    }

    #[test]
    fn unified_tabular_complements_are_bound_to_the_selected_table() {
        let protocol = panproto_protocols::database::cassandra::protocol();
        let schema = SchemaBuilder::new(&protocol)
            .vertex("b", "table", None)
            .expect("table b")
            .vertex("other", "table", None)
            .expect("source table")
            .vertex("rows", "table", None)
            .expect("destination table")
            .build()
            .expect("schema");

        let other = UnifiedCodec::tsv("cassandra", "other").expect("other TSV codec");
        let (other_instance, other_complement) = other
            .parse_functor_preserving(&schema, b"x\nvalue\n")
            .expect("parse other table");
        let rows = UnifiedCodec::tsv("cassandra", "rows").expect("rows TSV codec");
        assert!(matches!(
            rows.emit_functor_preserving(&schema, &other_instance, &other_complement),
            Err(EmitInstanceError::UnsupportedStructuralEdit { ref detail, .. })
                if detail.contains("source table") && detail.contains("rows")
        ));
    }

    #[test]
    fn json_and_toml_csts_refuse_invalid_unicode_scalar_escapes() {
        let schema = open_yaml_schema();
        let json = UnifiedCodec::json("fidelity-cst").expect("JSON CST codec");
        for input in [
            br#"{"value":"\uD800"}"#.as_slice(),
            br#"{"value":"\uDE00"}"#.as_slice(),
            br#"{"value":"\uD83Dz\uDE00"}"#.as_slice(),
            br#"{"\uD800":"value"}"#.as_slice(),
        ] {
            assert!(matches!(
                json.parse_wtype_preserving(&schema, input),
                Err(ParseInstanceError::Parse { ref message, .. })
                    if message.contains("invalid JSON string token")
            ));
        }
        json.parse_wtype_preserving(&schema, br#"{"value":"\uD83D\uDE00"}"#)
            .expect("paired JSON surrogate");

        let toml = UnifiedCodec::toml("fidelity-cst").expect("TOML CST codec");
        for input in [
            br#"value = "\uD800""#.as_slice(),
            br#"value = "\U00110000""#.as_slice(),
            br#""\uD800" = "value""#.as_slice(),
        ] {
            assert!(matches!(
                toml.parse_wtype_preserving(&schema, input),
                Err(ParseInstanceError::Parse { ref message, .. })
                    if message.contains("invalid TOML basic string token")
            ));
        }
        toml.parse_wtype_preserving(&schema, br#"value = "\U0001F600""#)
            .expect("valid TOML Unicode scalar");
        for input in [
            b"value = \"\"\"abc\"\"\"\"".as_slice(),
            b"value = \"\"\"abc\"\"\"\"\"".as_slice(),
        ] {
            toml.parse_wtype_preserving(&schema, input)
                .expect("valid terminal quote run in TOML multiline string");
        }
    }

    #[test]
    fn cst_extractors_refuse_parser_recovery_markers() {
        let schema = open_yaml_schema();
        for (codec, input, label) in [
            (
                UnifiedCodec::json("fidelity-cst").expect("JSON CST codec"),
                b"{\"a\":".as_slice(),
                "JSON",
            ),
            (
                UnifiedCodec::toml("fidelity-cst").expect("TOML CST codec"),
                b"a =".as_slice(),
                "TOML",
            ),
        ] {
            let error = codec
                .parse_wtype_preserving(&schema, input)
                .expect_err("parser recovery must not produce an instance");
            assert!(
                matches!(
                    error,
                    ParseInstanceError::Parse { ref message, .. }
                        if message.contains("parser recovery marker")
                ),
                "{label} recovery must be refused"
            );
        }
    }

    #[test]
    fn unified_json_rejects_yaml_input() {
        let codec = UnifiedCodec::json("fidelity-cst").expect("JSON CST codec");
        let result = codec.parse_wtype_preserving(&open_yaml_schema(), b"name: Alice\n");
        assert!(
            matches!(result, Err(ParseInstanceError::Parse { .. })),
            "YAML must not parse on the JSON CST route: {result:?}"
        );
    }

    #[test]
    fn yaml_empty_mapping_values_are_null_and_editable() {
        let open_schema = open_yaml_schema();
        let codec = UnifiedCodec::yaml("fidelity-cst").expect("YAML CST codec");
        let input = b"open:\n";
        let (mut open, complement) = codec
            .parse_wtype_preserving(&open_schema, input)
            .expect("parse empty open-schema value");
        assert!(matches!(value_at(&open, "root:open"), FieldPresence::Null));
        assert_eq!(
            codec
                .emit_wtype_preserving(&open_schema, &open, &complement)
                .expect("replay empty value"),
            input
        );
        set_tree_value(
            &mut open,
            "root:open",
            FieldPresence::Present(Value::Str("filled".into())),
        );
        let emitted = codec
            .emit_wtype_preserving(&open_schema, &open, &complement)
            .expect("materialize empty value");
        let (reparsed, _) = codec
            .parse_wtype_preserving(&open_schema, &emitted)
            .expect("reparse materialized value");
        assert_eq!(
            value_at(&reparsed, "root:open"),
            &FieldPresence::Present(Value::Str("filled".into()))
        );

        let commented_input = b"open: # keep this comment\n";
        let (mut commented, complement) = codec
            .parse_wtype_preserving(&open_schema, commented_input)
            .expect("parse commented empty value");
        set_tree_value(
            &mut commented,
            "root:open",
            FieldPresence::Present(Value::Str("filled".into())),
        );
        let emitted = codec
            .emit_wtype_preserving(&open_schema, &commented, &complement)
            .expect("materialize commented empty value");
        assert_eq!(emitted, b"open: \"filled\" # keep this comment\n");
        let (reparsed, _) = codec
            .parse_wtype_preserving(&open_schema, &emitted)
            .expect("reparse commented materialized value");
        assert_eq!(
            value_at(&reparsed, "root:open"),
            &FieldPresence::Present(Value::Str("filled".into()))
        );

        let protocol = Protocol {
            name: "fidelity-cst".into(),
            obj_kinds: vec!["object".into(), "value".into()],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("root", "object", None)
            .expect("root")
            .vertex("known", "value", None)
            .expect("known")
            .edge("root", "known", "prop", Some("known"))
            .expect("known edge")
            .build()
            .expect("schema");
        let input = b"known:\nextra:\n";
        let (instance, complement) = codec
            .parse_wtype_preserving(&schema, input)
            .expect("parse known and extra empty values");
        assert!(matches!(value_at(&instance, "known"), FieldPresence::Null));
        assert_eq!(
            instance
                .node(instance.root)
                .and_then(|node| node.extra_fields.get("extra")),
            Some(&Value::Null)
        );
        assert_eq!(
            codec
                .emit_wtype_preserving(&schema, &instance, &complement)
                .expect("replay known and extra empty values"),
            input
        );
    }

    #[test]
    fn yaml_empty_sequence_items_are_null_and_keep_the_dash_carrier() {
        let schema = open_yaml_schema();
        let codec = UnifiedCodec::yaml("fidelity-cst").expect("YAML CST codec");
        for (input, expected) in [
            (
                b"items:\n  -\n  - kept\n".as_slice(),
                b"items:\n  - \"filled\"\n  - kept\n".as_slice(),
            ),
            (
                b"items:\n  - # comment\n  - kept\n".as_slice(),
                b"items:\n  - \"filled\" # comment\n  - kept\n".as_slice(),
            ),
        ] {
            let (mut instance, complement) = codec
                .parse_wtype_preserving(&schema, input)
                .expect("parse empty sequence item");
            assert_eq!(
                codec
                    .emit_wtype_preserving(&schema, &instance, &complement)
                    .expect("replay empty sequence item"),
                input
            );
            let list = instance
                .nodes
                .values()
                .find(|node| node.anchor.as_ref() == "root:items")
                .map(|node| node.id)
                .expect("items list");
            let first = instance.children(list)[0];
            assert!(matches!(
                instance.node(first).and_then(|node| node.value.as_ref()),
                Some(FieldPresence::Null)
            ));
            instance.nodes.get_mut(&first).expect("first item").value =
                Some(FieldPresence::Present(Value::Str("filled".into())));
            let emitted = codec
                .emit_wtype_preserving(&schema, &instance, &complement)
                .expect("materialize empty sequence item");
            assert_eq!(emitted, expected);
            let (reparsed, _) = codec
                .parse_wtype_preserving(&schema, &emitted)
                .expect("reparse materialized sequence item");
            let reparsed_list = reparsed
                .nodes
                .values()
                .find(|node| node.anchor.as_ref() == "root:items")
                .map(|node| node.id)
                .expect("reparsed items list");
            let reparsed_first = reparsed.children(reparsed_list)[0];
            assert!(matches!(
                reparsed
                    .node(reparsed_first)
                    .and_then(|node| node.value.as_ref()),
                Some(FieldPresence::Present(Value::Str(value))) if value == "filled"
            ));
        }
    }

    #[test]
    fn yaml_partial_schema_preserves_nested_empty_mapping_and_sequence_values() {
        let protocol = Protocol {
            name: "fidelity-cst".into(),
            obj_kinds: vec!["object".into(), "value".into()],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("root", "object", None)
            .expect("root")
            .vertex("known", "value", None)
            .expect("known")
            .edge("root", "known", "prop", Some("known"))
            .expect("known edge")
            .build()
            .expect("partial schema");
        let codec = UnifiedCodec::yaml("fidelity-cst").expect("YAML CST codec");
        let input = b"known: old\nextra_map: {a:}\nextra_seq:\n  -\n  - kept\n";
        let (mut instance, complement) = codec
            .parse_wtype_preserving(&schema, input)
            .expect("parse nested generic empty values");
        let extras = &instance.node(instance.root).expect("root").extra_fields;
        assert!(matches!(
            extras.get("extra_map"),
            Some(Value::Unknown(fields)) if fields.get("a") == Some(&Value::Null)
        ));
        assert!(matches!(
            extras.get("extra_seq"),
            Some(Value::List(items))
                if items == &vec![Value::Null, Value::Str("kept".into())]
        ));

        set_tree_value(
            &mut instance,
            "known",
            FieldPresence::Present(Value::Str("changed".into())),
        );
        let emitted = codec
            .emit_wtype_preserving(&schema, &instance, &complement)
            .expect("rewrite declared value without dropping generic nulls");
        let (reparsed, _) = codec
            .parse_wtype_preserving(&schema, &emitted)
            .expect("reparse nested generic empty values");
        let extras = &reparsed.node(reparsed.root).expect("root").extra_fields;
        assert!(matches!(
            extras.get("extra_map"),
            Some(Value::Unknown(fields)) if fields.get("a") == Some(&Value::Null)
        ));
        assert!(matches!(
            extras.get("extra_seq"),
            Some(Value::List(items))
                if items == &vec![Value::Null, Value::Str("kept".into())]
        ));
    }

    #[test]
    fn yaml_aliases_are_refused_until_anchor_resolution_is_available() {
        let schema = open_yaml_schema();
        let codec = UnifiedCodec::yaml("fidelity-cst").expect("YAML CST codec");
        let error = codec
            .parse_wtype_preserving(&schema, b"a: &x 1\nb: *x\n")
            .expect_err("an alias must not degrade to a string");
        assert!(matches!(
            error,
            ParseInstanceError::Parse { message, .. }
                if message.contains("anchor resolution")
        ));
    }

    #[test]
    fn json_empty_object_keys_remain_addressable() {
        let schema = open_yaml_schema();
        let codec = UnifiedCodec::json("fidelity-cst").expect("JSON CST codec");
        let (mut instance, complement) = codec
            .parse_wtype_preserving(&schema, br#"{"":1}"#)
            .expect("parse empty JSON key");
        assert_eq!(
            value_at(&instance, "root:"),
            &FieldPresence::Present(Value::Int(1))
        );
        set_tree_value(
            &mut instance,
            "root:",
            FieldPresence::Present(Value::Int(2)),
        );
        let emitted = codec
            .emit_wtype_preserving(&schema, &instance, &complement)
            .expect("edit empty-key value");
        let (reparsed, _) = codec
            .parse_wtype_preserving(&schema, &emitted)
            .expect("reparse empty-key value");
        assert_eq!(
            value_at(&reparsed, "root:"),
            &FieldPresence::Present(Value::Int(2))
        );
    }

    #[test]
    fn toml_dotted_keys_and_headers_are_refused_instead_of_flattened() {
        let schema = open_yaml_schema();
        let codec = UnifiedCodec::toml("fidelity-cst").expect("TOML CST codec");
        for input in [b"a.b = 1\n".as_slice(), b"[a.b]\nc = 1\n".as_slice()] {
            let error = codec
                .parse_wtype_preserving(&schema, input)
                .expect_err("dotted TOML structure must not become a flat key");
            assert!(matches!(
                error,
                ParseInstanceError::Parse { message, .. }
                    if message.contains("dotted keys")
            ));
        }
    }

    #[test]
    fn yaml_cst_decodes_escaped_mapping_keys_before_routing_them() {
        let protocol = Protocol {
            name: "fidelity-cst".into(),
            obj_kinds: vec!["object".into(), "value".into()],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("root", "object", None)
            .expect("root")
            .vertex("escaped_key", "value", None)
            .expect("escaped key value")
            .edge("root", "escaped_key", "prop", Some("line\nfeed"))
            .expect("semantic key edge")
            .build()
            .expect("schema");
        let codec = UnifiedCodec::yaml("fidelity-cst").expect("YAML CST codec");
        let input = b"\"line\\nfeed\": 7\n";

        let (instance, complement) = codec
            .parse_wtype_preserving(&schema, input)
            .expect("route an escaped key by its decoded value");
        assert!(
            instance
                .nodes
                .values()
                .any(|node| node.anchor.as_ref() == "escaped_key")
        );
        assert_eq!(
            codec
                .emit_wtype_preserving(&schema, &instance, &complement)
                .expect("replay escaped key"),
            input
        );

        let open_schema = open_yaml_schema();
        let (open_instance, _) = codec
            .parse_wtype_preserving(&open_schema, input)
            .expect("decode an escaped key under an open schema");
        assert!(
            open_instance
                .nodes
                .values()
                .any(|node| node.anchor.as_ref() == "root:line\nfeed")
        );
        assert_eq!(open_instance.arcs[0].2.name.as_deref(), Some("line\nfeed"));

        for duplicate in [
            b"same: 1\nsame: 2\n".as_slice(),
            b"\"s\\u0061me\": 1\nsame: 2\n".as_slice(),
            b"outer:\n  same: 1\n  same: 2\n".as_slice(),
        ] {
            assert!(matches!(
                codec.parse_wtype_preserving(&open_schema, duplicate),
                Err(ParseInstanceError::Parse { ref message, .. })
                    if message.contains("duplicate YAML mapping key")
            ));
        }
    }

    #[test]
    fn toml_cst_rejects_duplicate_non_array_keys() {
        let schema = open_yaml_schema();
        let codec = UnifiedCodec::toml("fidelity-cst").expect("TOML CST codec");
        for input in [
            b"same = 1\nsame = 2\n".as_slice(),
            b"[outer]\nsame = 1\nsame = 2\n".as_slice(),
        ] {
            let error = codec
                .parse_wtype_preserving(&schema, input)
                .expect_err("duplicate TOML keys must not become lists");
            assert!(matches!(error, ParseInstanceError::Parse { .. }));
        }
    }

    /// The TOML CST route writes exact numeral and integer tokens and
    /// representable timestamp, byte and float edits, and refuses a null,
    /// which TOML cannot spell.
    fn assert_toml_cst_rerenders(
        codec: &UnifiedCodec,
        schema: &Schema,
        mut instance: WInstance,
        complement: &CstComplement,
    ) {
        for (anchor, value) in [
            ("long_decimal", edited_decimal()),
            (
                "numeric_38_10",
                Value::Decimal(
                    DecimalValue::parse("1234567890123456789012345678.1234567891")
                        .expect("edited numeric"),
                ),
            ),
            (
                "huge",
                Value::Decimal(DecimalValue::parse("2e400").expect("edited huge decimal")),
            ),
        ] {
            let mut edited = instance.clone();
            set_tree_value(&mut edited, anchor, FieldPresence::Present(value.clone()));
            let emitted = codec
                .emit_wtype_preserving(schema, &edited, complement)
                .expect("TOML CST writes an exact numeral token");
            let (reparsed, _) = codec
                .parse_wtype_preserving(schema, &emitted)
                .expect("reparse exact TOML numeral token");
            assert_eq!(value_at(&reparsed, anchor), &FieldPresence::Present(value));
        }

        let edited_timestamp =
            Temporal::parse("2026-09-29T01:02:03.456").expect("edited local timestamp");
        set_tree_value(
            &mut instance,
            "timestamp_local",
            FieldPresence::Present(Value::Temporal(edited_timestamp)),
        );
        let edited_offset =
            Temporal::parse("2026-09-29T01:02:03.456+05:30").expect("edited offset timestamp");
        set_tree_value(
            &mut instance,
            "timestamp_offset",
            FieldPresence::Present(Value::Temporal(edited_offset)),
        );
        set_tree_value(
            &mut instance,
            "bytes",
            FieldPresence::Present(Value::Bytes(vec![0x03, 0x04, 0x05])),
        );
        set_tree_value(
            &mut instance,
            "nan",
            FieldPresence::Present(Value::Float(f64::INFINITY)),
        );
        let emitted = codec
            .emit_wtype_preserving(schema, &instance, complement)
            .expect("TOML CST renders a representable timestamp edit");
        let (reparsed, _) = codec
            .parse_wtype_preserving(schema, &emitted)
            .expect("reparse edited TOML CST");
        assert_eq!(
            value_at(&reparsed, "timestamp_local"),
            value_at(&instance, "timestamp_local"),
            "TOML changed the timestamp during rerender"
        );
        assert_eq!(
            value_at(&reparsed, "timestamp_offset"),
            value_at(&instance, "timestamp_offset"),
            "TOML changed the offset timestamp during rerender"
        );
        assert_eq!(value_at(&reparsed, "bytes"), value_at(&instance, "bytes"));
        assert_eq!(value_at(&reparsed, "nan"), value_at(&instance, "nan"));

        for (anchor, value) in [
            (
                "u64_max",
                Value::parse_integer("18446744073709551614").expect("edited TOML u64"),
            ),
            (
                "ten_to_29",
                Value::parse_integer("100000000000000000000000000001").expect("edited TOML 10^29"),
            ),
        ] {
            let mut edited = instance.clone();
            set_tree_value(&mut edited, anchor, FieldPresence::Present(value.clone()));
            let emitted = codec
                .emit_wtype_preserving(schema, &edited, complement)
                .expect("TOML CST writes an exact integer token");
            let (reparsed, _) = codec
                .parse_wtype_preserving(schema, &emitted)
                .expect("reparse exact TOML integer token");
            assert_eq!(value_at(&reparsed, anchor), &FieldPresence::Present(value));
        }

        for presence in [FieldPresence::Null, FieldPresence::Present(Value::Null)] {
            set_tree_value(&mut instance, "long_decimal", presence);
            let error = codec
                .emit_wtype_preserving(schema, &instance, complement)
                .expect_err("TOML CST has no null spelling");
            assert!(matches!(
                error,
                EmitInstanceError::Value(error)
                    if error.reason == EmitValueReason::NullNotRepresentable
            ));
        }
    }

    #[test]
    fn json_cst_yaml_cst_and_toml_cst_exact_routes_force_a_rerender() {
        let schema = exact_value_schema("root");
        for (codec, fixture_path, label) in [
            (
                UnifiedCodec::json("cassandra").expect("JSON CST codec"),
                "json/exact.json",
                "JSON",
            ),
            (
                UnifiedCodec::yaml("cassandra").expect("YAML CST codec"),
                "yaml/exact.yaml",
                "YAML",
            ),
            (
                UnifiedCodec::toml("cassandra").expect("TOML CST codec"),
                "toml/exact.toml",
                "TOML",
            ),
        ] {
            let input = read_fixture(fixture_path);
            let (mut instance, complement) = codec
                .parse_wtype_preserving(&schema, &input)
                .unwrap_or_else(|error| panic!("parse {label} CST: {error}"));
            assert_exact_tree_input(&instance);
            let has_anchor = |anchor: &str| {
                instance
                    .nodes
                    .values()
                    .any(|node| node.anchor.as_ref() == anchor)
            };
            if label == "TOML" {
                assert!(!has_anchor("null_value"), "TOML synthesized a null field");
            } else {
                assert_eq!(
                    value_at(&instance, "null_value"),
                    &FieldPresence::Null,
                    "{label} did not classify the explicit null"
                );
            }
            assert!(!has_anchor("absent"), "{label} synthesized an absent field");
            assert!(matches!(
                value_at(&instance, "long_decimal"),
                FieldPresence::Present(Value::Decimal(value))
                    if value.to_string() == "12345678901234567890.123456789"
            ));
            assert_big_numbers_parsed(&instance);
            if label == "YAML" {
                assert_eq!(
                    value_at(&instance, "bytes"),
                    &FieldPresence::Present(Value::Bytes(vec![0x00, 0x01, 0x02, 0xff]))
                );
            }

            let unchanged = codec
                .emit_wtype_preserving(&schema, &instance, &complement)
                .unwrap_or_else(|error| panic!("emit unchanged {label} CST: {error}"));
            assert_eq!(unchanged, input, "{label} changed untouched source bytes");

            if label == "TOML" {
                assert_toml_cst_rerenders(&codec, &schema, instance, &complement);
                continue;
            }

            for (anchor, value) in exact_value_edits()
                .into_iter()
                .chain([("null_value", Value::Str("materialized-null".into()))])
            {
                set_tree_value(&mut instance, anchor, FieldPresence::Present(value));
            }
            if label == "YAML" {
                set_tree_value(
                    &mut instance,
                    "nan",
                    FieldPresence::Present(Value::Float(1.25)),
                );
            }

            let emitted = codec
                .emit_wtype_preserving(&schema, &instance, &complement)
                .unwrap_or_else(|error| panic!("emit edited {label} CST: {error}"));
            assert!(
                String::from_utf8_lossy(&emitted).contains("12345678901234567890.123456790"),
                "{label} did not render the edit: {}",
                String::from_utf8_lossy(&emitted)
            );
            let (reparsed, _) = codec
                .parse_wtype_preserving(&schema, &emitted)
                .unwrap_or_else(|error| panic!("reparse edited {label} CST: {error}"));
            for (anchor, _) in exact_value_edits() {
                assert_eq!(
                    value_at(&reparsed, anchor),
                    value_at(&instance, anchor),
                    "{label} changed {anchor} during rerender"
                );
            }
            assert_eq!(
                value_at(&reparsed, "null_value"),
                value_at(&instance, "null_value"),
                "{label} changed null_value during rerender"
            );
            if label == "YAML" {
                assert_eq!(value_at(&reparsed, "nan"), value_at(&instance, "nan"));
            }
        }
    }

    #[test]
    fn yaml_cst_and_toml_cst_preserve_nonfinite_spellings() {
        let schema = open_yaml_schema();
        for (codec, input, label, expected) in vec![
            (
                UnifiedCodec::yaml("fidelity-cst").expect("YAML CST codec"),
                b"value: .NaN\n".as_slice(),
                "YAML",
                CANONICAL_NAN,
            ),
            (
                UnifiedCodec::yaml("fidelity-cst").expect("YAML CST codec"),
                b"value: .Inf\n".as_slice(),
                "YAML",
                f64::INFINITY,
            ),
            (
                UnifiedCodec::yaml("fidelity-cst").expect("YAML CST codec"),
                b"value: -.Inf\n".as_slice(),
                "YAML",
                f64::NEG_INFINITY,
            ),
            (
                UnifiedCodec::yaml("fidelity-cst").expect("YAML CST codec"),
                b"value: -.NaN\n".as_slice(),
                "YAML",
                NEGATIVE_CANONICAL_NAN,
            ),
            (
                UnifiedCodec::toml("fidelity-cst").expect("TOML CST codec"),
                b"value = +nan\n".as_slice(),
                "TOML",
                CANONICAL_NAN,
            ),
            (
                UnifiedCodec::toml("fidelity-cst").expect("TOML CST codec"),
                b"value = +inf\n".as_slice(),
                "TOML",
                f64::INFINITY,
            ),
            (
                UnifiedCodec::toml("fidelity-cst").expect("TOML CST codec"),
                b"value = -inf\n".as_slice(),
                "TOML",
                f64::NEG_INFINITY,
            ),
            (
                UnifiedCodec::toml("fidelity-cst").expect("TOML CST codec"),
                b"value = -nan\n".as_slice(),
                "TOML",
                NEGATIVE_CANONICAL_NAN,
            ),
        ]
        .into_boxed_slice()
        {
            let (instance, complement) = codec
                .parse_wtype_preserving(&schema, input)
                .unwrap_or_else(|error| panic!("parse {label} NaN: {error}"));
            let actual = value_at(&instance, "root:value");
            assert!(
                matches!(
                    actual,
                    FieldPresence::Present(Value::Float(value))
                        if value.to_bits() == expected.to_bits()
                ),
                "{label} parsed {input:?} as {actual:?}, expected float bits {:x}",
                expected.to_bits()
            );
            let emitted = codec
                .emit_wtype_preserving(&schema, &instance, &complement)
                .unwrap_or_else(|error| panic!("emit unchanged {label} NaN: {error}"));
            assert_eq!(emitted, input, "{label} changed an untouched NaN token");
        }
    }

    #[test]
    fn yaml_payload_nan_is_not_silently_rewritten() {
        let schema = open_yaml_schema();
        let codec = UnifiedCodec::yaml("fidelity-cst").expect("YAML CST codec");
        let (mut instance, complement) = codec
            .parse_wtype_preserving(&schema, b"value: 1.0\n")
            .expect("parse finite YAML source");
        set_tree_value(
            &mut instance,
            "root:value",
            FieldPresence::Present(Value::Float(f64::from_bits(CANONICAL_NAN.to_bits() + 1))),
        );
        let error = codec
            .emit_wtype_preserving(&schema, &instance, &complement)
            .expect_err("a payload NaN has no exact YAML spelling");
        assert!(matches!(
            error,
            EmitInstanceError::Value(error)
                if error.reason == EmitValueReason::NotRepresentable
                    && error.format == "yaml"
                    && error.value_type == "float"
        ));
    }

    #[test]
    fn yaml_open_schema_reports_invalid_explicit_tags() {
        let schema = open_yaml_schema();
        let codec = UnifiedCodec::yaml("fidelity-cst").expect("YAML CST codec");
        let error = codec
            .parse_wtype_preserving(&schema, b"value: !!binary \"%%%\"\n")
            .expect_err("invalid tagged bytes must not become a string");
        assert!(matches!(error, ParseInstanceError::Fidelity(_)));
    }

    #[test]
    fn yaml_cst_refuses_multi_document_streams() {
        let schema = open_yaml_schema();
        let codec = UnifiedCodec::yaml("fidelity-cst").expect("YAML CST codec");
        let error = codec
            .parse_wtype_preserving(&schema, b"---\na: 1\n---\na: 2\n")
            .expect_err("one WInstance cannot silently select one YAML document");
        assert!(matches!(
            error,
            ParseInstanceError::Parse { message, .. }
                if message.contains("exactly one is required")
        ));
    }

    #[test]
    fn yaml_partial_schema_keeps_undeclared_sequences_as_lists() {
        let schema = exact_value_schema("root");
        let codec = UnifiedCodec::yaml("cassandra").expect("YAML CST codec");
        let (instance, _) = codec
            .parse_wtype_preserving(&schema, b"long_decimal: 1.0\nextra: [1, 2]\n")
            .expect("parse partial-schema YAML");
        assert_eq!(
            instance
                .node(instance.root)
                .and_then(|node| node.extra_fields.get("extra")),
            Some(&Value::List(vec![Value::Int(1), Value::Int(2)]))
        );
    }

    #[test]
    fn xml_csv_and_tsv_csts_render_nonfinite_edits_from_finite_sources() {
        let tree_schema = exact_value_schema("doc");
        let xml = UnifiedCodec::xml("cassandra").expect("XML CST codec");
        let (source, complement) = xml
            .parse_wtype_preserving(&tree_schema, b"<doc><nan>0.0</nan></doc>")
            .expect("parse finite XML source");
        for expected in [
            CANONICAL_NAN,
            NEGATIVE_CANONICAL_NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            let mut edited = source.clone();
            set_tree_value(
                &mut edited,
                "nan",
                FieldPresence::Present(Value::Float(expected)),
            );
            let emitted = xml
                .emit_wtype_preserving(&tree_schema, &edited, &complement)
                .expect("render XML nonfinite edit");
            let (reparsed, _) = xml
                .parse_wtype_preserving(&tree_schema, &emitted)
                .expect("reparse XML nonfinite edit");
            assert!(matches!(
                value_at(&reparsed, "nan"),
                FieldPresence::Present(Value::Float(actual))
                    if actual.to_bits() == expected.to_bits()
            ));
        }

        let table_schema = exact_value_schema("rows");
        for (codec, input, format) in [
            (
                UnifiedCodec::csv("cassandra").expect("CSV CST codec"),
                b"nan\n0.0\n".as_slice(),
                "CSV",
            ),
            (
                UnifiedCodec::tsv("cassandra", "rows").expect("TSV CST codec"),
                b"nan\n0.0\n".as_slice(),
                "TSV",
            ),
        ] {
            let (source, complement) = codec
                .parse_functor_preserving(&table_schema, input)
                .unwrap_or_else(|error| panic!("parse finite {format} source: {error}"));
            let row = source.table("rows").expect("rows table").ids()[0];
            for expected in [
                CANONICAL_NAN,
                NEGATIVE_CANONICAL_NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
            ] {
                let mut edited = source.clone();
                edited
                    .row_mut(row)
                    .expect("value row")
                    .insert("nan".into(), Value::Float(expected));
                let emitted = codec
                    .emit_functor_preserving(&table_schema, &edited, &complement)
                    .unwrap_or_else(|error| panic!("render {format} nonfinite edit: {error}"));
                let (reparsed, _) = codec
                    .parse_functor_preserving(&table_schema, &emitted)
                    .unwrap_or_else(|error| panic!("reparse {format} nonfinite edit: {error}"));
                assert!(matches!(
                    table_value(&reparsed, "rows", "nan"),
                    Value::Float(actual)
                        if actual.to_bits() == expected.to_bits()
                ));
            }
        }
    }

    #[test]
    fn json_cst_edits_replace_complete_tokens_across_scalar_classes() {
        let schema = open_yaml_schema();
        let codec = UnifiedCodec::json("fidelity-cst").expect("JSON CST codec");

        let (mut null_instance, null_complement) = codec
            .parse_wtype_preserving(&schema, br#"{"value":null}"#)
            .expect("parse null JSON CST");
        set_tree_value(
            &mut null_instance,
            "root:value",
            FieldPresence::Present(Value::Str("edited".into())),
        );
        let emitted = codec
            .emit_wtype_preserving(&schema, &null_instance, &null_complement)
            .expect("replace null with string token");
        assert_eq!(emitted, br#"{"value":"edited"}"#);
        let (reparsed, _) = codec
            .parse_wtype_preserving(&schema, &emitted)
            .expect("reparse string token");
        assert_eq!(
            value_at(&reparsed, "root:value"),
            value_at(&null_instance, "root:value")
        );

        let (mut string_instance, string_complement) = codec
            .parse_wtype_preserving(&schema, br#"{"value":"1"}"#)
            .expect("parse string JSON CST");
        let decimal = DecimalValue::parse("1e400").expect("exact decimal token");
        set_tree_value(
            &mut string_instance,
            "root:value",
            FieldPresence::Present(Value::Decimal(decimal)),
        );
        let emitted = codec
            .emit_wtype_preserving(&schema, &string_instance, &string_complement)
            .expect("replace string with numeric token");
        assert_eq!(emitted, br#"{"value":1e400}"#);
        let (reparsed, _) = codec
            .parse_wtype_preserving(&schema, &emitted)
            .expect("reparse numeric token");
        assert_eq!(
            value_at(&reparsed, "root:value"),
            value_at(&string_instance, "root:value")
        );
    }

    #[test]
    fn json_cst_rejects_duplicate_keys_and_preserves_non_string_type_data() {
        let codec = UnifiedCodec::json("fidelity-cst").expect("JSON CST codec");
        let open_schema = open_yaml_schema();
        for input in [
            br#"{"same":1,"same":2}"#.as_slice(),
            br#"{"outer":{"same":1,"same":2}}"#.as_slice(),
            br#"{"$type":"a","$type":"b"}"#.as_slice(),
        ] {
            assert!(matches!(
                codec.parse_wtype_preserving(&open_schema, input),
                Err(ParseInstanceError::Parse { ref message, .. })
                    if message.contains("duplicate JSON object key")
            ));
        }

        let input = br#"{"$type":7}"#;
        let (instance, complement) = codec
            .parse_wtype_preserving(&open_schema, input)
            .expect("parse non-string $type as data");
        assert_eq!(
            value_at(&instance, "root:$type"),
            &FieldPresence::Present(Value::Int(7))
        );
        assert_eq!(
            codec
                .emit_wtype_preserving(&open_schema, &instance, &complement)
                .expect("replay non-string $type"),
            input
        );

        let protocol = Protocol {
            name: "fidelity-cst".into(),
            obj_kinds: vec!["object".into(), "value".into()],
            ..Protocol::default()
        };
        let declared = SchemaBuilder::new(&protocol)
            .vertex("root", "object", None)
            .expect("root")
            .vertex("root:type", "value", None)
            .expect("type")
            .edge("root", "root:type", "prop", Some("$type"))
            .expect("type edge")
            .build()
            .expect("declared type schema");
        let (mut instance, complement) = codec
            .parse_wtype_preserving(&declared, br#"{"$type":"kind"}"#)
            .expect("parse declared discriminator carrier");
        assert_eq!(
            instance
                .node(instance.root)
                .and_then(|node| node.discriminator.as_deref()),
            Some("kind")
        );
        set_tree_value(
            &mut instance,
            "root:type",
            FieldPresence::Present(Value::Int(7)),
        );
        assert!(matches!(
            codec.emit_wtype_preserving(&declared, &instance, &complement),
            Err(EmitInstanceError::UnsupportedStructuralEdit { ref detail, .. })
                if detail.contains("$type") && detail.contains("discriminator")
        ));
    }

    #[test]
    fn xml_cst_exact_values_and_nil_semantics_survive_forced_rerenders() {
        let schema = exact_value_schema("doc");
        let codec = UnifiedCodec::xml("cassandra").expect("XML CST codec");
        let input = read_fixture("xml/exact.xml");
        let (mut instance, complement) = codec
            .parse_wtype_preserving(&schema, &input)
            .expect("parse XML CST");
        assert_exact_tree_input(&instance);
        assert!(matches!(
            value_at(&instance, "null_value"),
            FieldPresence::Null
        ));
        assert_big_numbers_parsed(&instance);
        assert!(matches!(
            value_at(&instance, "nan"),
            FieldPresence::Present(Value::Float(value)) if value.is_nan()
        ));
        assert!(
            instance
                .nodes
                .values()
                .all(|node| node.anchor.as_ref() != "absent")
        );
        let unchanged = codec
            .emit_wtype_preserving(&schema, &instance, &complement)
            .expect("emit unchanged XML CST");
        assert_eq!(unchanged, input);

        let extra_edits = [
            ("nan", Value::Float(f64::INFINITY)),
            ("null_value", Value::Str("materialized-null".into())),
            ("empty", Value::Str("materialized-empty".into())),
        ];
        for (anchor, value) in exact_value_edits().into_iter().chain(extra_edits.clone()) {
            set_tree_value(&mut instance, anchor, FieldPresence::Present(value));
        }
        instance
            .nodes
            .values_mut()
            .find(|node| node.anchor.as_ref() == "null_value")
            .expect("materialized null node")
            .extra_fields
            .insert("xsi:nil".into(), Value::Str("false".into()));
        let emitted = codec
            .emit_wtype_preserving(&schema, &instance, &complement)
            .expect("emit edited XML CST");
        let (reparsed, _) = codec
            .parse_wtype_preserving(&schema, &emitted)
            .expect("reparse edited XML CST");
        for (anchor, _) in exact_value_edits().into_iter().chain(extra_edits) {
            assert_eq!(value_at(&reparsed, anchor), value_at(&instance, anchor));
        }
        assert!(
            reparsed
                .nodes
                .values()
                .all(|node| node.anchor.as_ref() != "absent")
        );
    }

    #[test]
    fn xml_cst_preserves_whitespace_only_leaf_values() {
        let schema = exact_value_schema("doc");
        let codec = UnifiedCodec::xml("cassandra").expect("XML CST codec");
        let input = b"<doc><empty>   \t</empty></doc>";
        let (instance, complement) = codec
            .parse_wtype_preserving(&schema, input)
            .expect("parse whitespace XML CST");
        assert_eq!(
            value_at(&instance, "empty"),
            &FieldPresence::Present(Value::Str("   \t".into()))
        );
        let emitted = codec
            .emit_wtype_preserving(&schema, &instance, &complement)
            .expect("emit whitespace XML CST");
        assert_eq!(emitted, input);
    }

    #[test]
    fn xml_cst_preserves_whitespace_runs_between_and_after_children() {
        let schema = exact_value_schema("doc");
        let codec = UnifiedCodec::xml("cassandra").expect("XML CST codec");
        let input =
            b"<doc><long_decimal>1.0</long_decimal> <numeric_38_10>2.0</numeric_38_10> </doc>";
        let (instance, complement) = codec
            .parse_wtype_preserving(&schema, input)
            .expect("parse mixed XML whitespace");
        let text_segments: Vec<_> = instance
            .children(instance.root)
            .iter()
            .filter_map(|id| instance.node(*id))
            .filter(|node| node.shape == NodeShape::XmlTextSegment)
            .map(|node| node.value.as_ref())
            .collect();
        assert_eq!(
            text_segments,
            vec![
                Some(&FieldPresence::Present(Value::Str(" ".into()))),
                Some(&FieldPresence::Present(Value::Str(" ".into()))),
            ]
        );
        assert_eq!(
            codec
                .emit_wtype_preserving(&schema, &instance, &complement)
                .expect("replay mixed XML whitespace"),
            input
        );
    }

    #[test]
    fn xml_cst_mixed_children_use_declared_edge_targets_and_value_specs() {
        let protocol = panproto_protocols::database::cassandra::protocol();
        let schema = SchemaBuilder::new(&protocol)
            .vertex("doc", "table", None)
            .expect("document")
            .vertex("decimal_field", "decimal", None)
            .expect("decimal field")
            .edge("doc", "decimal_field", "prop", Some("b"))
            .expect("named XML child")
            .build()
            .expect("mixed-content schema");
        let codec = UnifiedCodec::xml("cassandra").expect("XML CST codec");
        let input = b"<doc>before<b>0.1</b>after</doc>";
        let (instance, complement) = codec
            .parse_wtype_preserving(&schema, input)
            .expect("parse schema-guided mixed XML");

        assert!(matches!(
            value_at(&instance, "decimal_field"),
            FieldPresence::Present(Value::Decimal(value)) if value.to_string() == "0.1"
        ));
        assert_eq!(
            codec
                .emit_wtype_preserving(&schema, &instance, &complement)
                .expect("replay schema-guided mixed XML"),
            input
        );
    }

    #[test]
    fn xml_cst_partial_schema_keeps_declared_and_undeclared_children_once() {
        let protocol = Protocol {
            name: "fidelity-cst".into(),
            obj_kinds: vec!["object".into(), "value".into()],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("root", "object", None)
            .expect("root")
            .vertex("known", "value", None)
            .expect("known child")
            .edge("root", "known", "prop", Some("known"))
            .expect("known edge")
            .build()
            .expect("partial XML schema");
        let codec = UnifiedCodec::xml("fidelity-cst").expect("XML CST codec");
        let input = b"<root><known>a</known><unknown>b</unknown></root>";
        let (instance, complement) = codec
            .parse_wtype_preserving(&schema, input)
            .expect("parse partial-schema XML");

        let child_anchors: Vec<_> = instance
            .children(instance.root)
            .iter()
            .filter_map(|id| instance.node(*id))
            .map(|node| node.anchor.as_str())
            .collect();
        assert_eq!(child_anchors, ["known", "root:unknown"]);
        assert_eq!(
            codec
                .emit_wtype_preserving(&schema, &instance, &complement)
                .expect("replay partial-schema XML"),
            input
        );
    }

    #[test]
    fn xml_cst_exposes_and_edits_cdata_as_character_data() {
        let schema = open_yaml_schema();
        let codec = UnifiedCodec::xml("fidelity-cst").expect("XML CST codec");
        let input = b"<root><![CDATA[raw <x> &amp;]]></root>";
        let (mut instance, complement) = codec
            .parse_wtype_preserving(&schema, input)
            .expect("parse XML CDATA");
        assert_eq!(
            instance
                .node(instance.root)
                .and_then(|node| node.value.as_ref()),
            Some(&FieldPresence::Present(Value::Str("raw <x> &amp;".into())))
        );
        assert_eq!(
            codec
                .emit_wtype_preserving(&schema, &instance, &complement)
                .expect("replay unchanged CDATA"),
            input
        );

        let root = instance.root;
        instance.nodes.get_mut(&root).expect("root node").value = Some(FieldPresence::Present(
            Value::Str("edited <y> & value".into()),
        ));
        let emitted = codec
            .emit_wtype_preserving(&schema, &instance, &complement)
            .expect("edit CDATA character data");
        let (reparsed, _) = codec
            .parse_wtype_preserving(&schema, &emitted)
            .expect("reparse edited XML character data");
        assert_eq!(
            reparsed
                .node(reparsed.root)
                .and_then(|node| node.value.as_ref()),
            instance
                .node(instance.root)
                .and_then(|node| node.value.as_ref()),
            "emitted XML: {}",
            String::from_utf8_lossy(&emitted)
        );

        instance.nodes.get_mut(&root).expect("root node").value = Some(FieldPresence::Present(
            Value::Str("edited ]]> boundary".into()),
        ));
        let emitted = codec
            .emit_wtype_preserving(&schema, &instance, &complement)
            .expect("replace CDATA carrier with escaped character data");
        let (reparsed, _) = codec
            .parse_wtype_preserving(&schema, &emitted)
            .expect("reparse escaped CDATA-boundary edit");
        assert_eq!(
            reparsed
                .node(reparsed.root)
                .and_then(|node| node.value.as_ref()),
            instance
                .node(instance.root)
                .and_then(|node| node.value.as_ref()),
            "emitted XML: {}",
            String::from_utf8_lossy(&emitted)
        );
    }

    #[test]
    fn xml_cst_attributes_decode_references_and_accept_edits() {
        let schema = open_yaml_schema();
        let codec = UnifiedCodec::xml("fidelity-cst").expect("XML CST codec");

        let lexical_input = br#"<root  b='a&amp;b' a = "c"/>"#;
        let (mut materialized, complement) = codec
            .parse_wtype_preserving(&schema, lexical_input)
            .expect("parse childless XML with lexical carriers");
        materialized
            .nodes
            .get_mut(&materialized.root)
            .expect("root")
            .value = Some(FieldPresence::Present(Value::Str("filled".into())));
        assert_eq!(
            codec
                .emit_wtype_preserving(&schema, &materialized, &complement)
                .expect("materialize character data without rebuilding attributes"),
            br#"<root  b='a&amp;b' a = "c">filled</root>"#
        );

        let input = br"<root note='a&amp;b'/>";
        let (mut instance, complement) = codec
            .parse_wtype_preserving(&schema, input)
            .expect("parse referenced XML attribute");
        assert_eq!(
            instance
                .node(instance.root)
                .and_then(|node| node.extra_fields.get("note")),
            Some(&Value::Str("a&b".into()))
        );

        let (normalized, _) = codec
            .parse_wtype_preserving(&schema, b"<root note=\"a\tb\nc\r\nd&#x9;e\"/>")
            .expect("parse normalized XML attribute whitespace");
        assert_eq!(
            normalized
                .node(normalized.root)
                .and_then(|node| node.extra_fields.get("note")),
            Some(&Value::Str("a b c d\te".into()))
        );
        assert_eq!(
            codec
                .emit_wtype_preserving(&schema, &instance, &complement)
                .expect("replay referenced XML attribute"),
            input
        );

        instance
            .nodes
            .get_mut(&instance.root)
            .expect("root")
            .extra_fields
            .insert("note".into(), Value::Str("edited < & >".into()));
        let emitted = codec
            .emit_wtype_preserving(&schema, &instance, &complement)
            .expect("edit referenced XML attribute");
        let (reparsed, _) = codec
            .parse_wtype_preserving(&schema, &emitted)
            .expect("reparse edited XML attribute");
        assert_eq!(
            reparsed
                .node(reparsed.root)
                .and_then(|node| node.extra_fields.get("note")),
            Some(&Value::Str("edited < & >".into()))
        );

        let root = instance.nodes.get_mut(&instance.root).expect("root");
        root.value = Some(FieldPresence::Present(Value::Str("x\ry".into())));
        root.extra_fields
            .insert("note".into(), Value::Str("a\tb\nc\rd".into()));
        let emitted = codec
            .emit_wtype_preserving(&schema, &instance, &complement)
            .expect("edit XML attribute whitespace and text carriage return");
        let emitted_text = String::from_utf8(emitted.clone()).expect("XML UTF-8");
        assert!(emitted_text.contains("x&#xD;y"));
        assert!(emitted_text.contains("note=\"a&#x9;b&#xA;c&#xD;d\""));
        let (reparsed, _) = codec
            .parse_wtype_preserving(&schema, &emitted)
            .expect("reparse XML attribute whitespace edit");
        assert_eq!(
            reparsed
                .node(reparsed.root)
                .and_then(|node| node.value.as_ref()),
            Some(&FieldPresence::Present(Value::Str("x\ry".into())))
        );
        assert_eq!(
            reparsed
                .node(reparsed.root)
                .and_then(|node| node.extra_fields.get("note")),
            Some(&Value::Str("a\tb\nc\rd".into()))
        );

        assert!(matches!(
            codec.parse_wtype_preserving(&schema, br#"<root note="a" note="b"/>"#),
            Err(ParseInstanceError::Parse { ref message, .. })
                if message.contains("duplicate XML attribute")
        ));
    }

    #[test]
    fn xml_cst_refuses_illegal_text_and_attribute_characters() {
        let schema = open_yaml_schema();
        let codec = UnifiedCodec::xml("fidelity-cst").expect("XML CST codec");
        let input = br#"<root note="kept">text</root>"#;

        for illegal_source in [
            b"<root>&#1;</root>".as_slice(),
            b"<root note=\"&#1;\"/>".as_slice(),
            b"<root>&#xD800;</root>".as_slice(),
            b"<root>&#x110000;</root>".as_slice(),
        ] {
            assert!(matches!(
                codec.parse_wtype_preserving(&schema, illegal_source),
                Err(ParseInstanceError::Parse { .. })
            ));
        }
        for invalid_namespace in [
            b"<p:root/>".as_slice(),
            b"<root p:note=\"x\"/>".as_slice(),
            b"<root xmlns:xml=\"urn:evil\"/>".as_slice(),
            b"<xmlns:root/>".as_slice(),
        ] {
            assert!(matches!(
                codec.parse_wtype_preserving(&schema, invalid_namespace),
                Err(ParseInstanceError::Parse { .. })
            ));
        }

        let bound = br#"<p:root xmlns:p="urn:test" p:note="kept"/>"#;
        let (mut unbound, complement) = codec
            .parse_wtype_preserving(&schema, bound)
            .expect("parse bound namespace");
        unbound
            .nodes
            .get_mut(&unbound.root)
            .expect("root")
            .extra_fields
            .insert("xmlns:p".into(), Value::Str(String::new()));
        assert!(matches!(
            codec.emit_wtype_preserving(&schema, &unbound, &complement),
            Err(EmitInstanceError::UnsupportedStructuralEdit { ref detail, .. })
                if detail.contains("namespace")
        ));

        let (mut illegal_text, complement) = codec
            .parse_wtype_preserving(&schema, input)
            .expect("parse XML text");
        illegal_text
            .nodes
            .get_mut(&illegal_text.root)
            .expect("root")
            .value = Some(FieldPresence::Present(Value::Str("bad\0text".into())));
        assert!(matches!(
            codec.emit_wtype_preserving(&schema, &illegal_text, &complement),
            Err(EmitInstanceError::Value(error))
                if error.reason == EmitValueReason::NotRepresentable && error.format == "xml"
        ));

        let (mut illegal_attribute, complement) = codec
            .parse_wtype_preserving(&schema, input)
            .expect("parse XML attribute");
        illegal_attribute
            .nodes
            .get_mut(&illegal_attribute.root)
            .expect("root")
            .extra_fields
            .insert("note".into(), Value::Str("bad\0attribute".into()));
        assert!(matches!(
            codec.emit_wtype_preserving(&schema, &illegal_attribute, &complement),
            Err(EmitInstanceError::Value(error))
                if error.reason == EmitValueReason::NotRepresentable && error.format == "xml"
        ));
    }

    #[test]
    fn xml_cst_reads_xsi_nil_as_an_xsd_boolean() {
        let schema = open_yaml_schema();
        let codec = UnifiedCodec::xml("fidelity-cst").expect("XML CST codec");
        let document = |nil: &str| {
            format!(
                r#"<root xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:nil="{nil}"/>"#
            )
        };
        for (nil, null) in [
            (" true ", true),
            ("1", true),
            ("&#9;0&#10;", false),
            ("false", false),
        ] {
            let input = document(nil);
            let (instance, complement) = codec
                .parse_wtype_preserving(&schema, input.as_bytes())
                .unwrap_or_else(|error| panic!("xsi:nil={nil:?} parses: {error}"));
            let root = instance.node(instance.root).expect("root");
            assert_eq!(
                matches!(root.value, Some(FieldPresence::Null)),
                null,
                "{nil}"
            );
            assert_eq!(
                codec
                    .emit_wtype_preserving(&schema, &instance, &complement)
                    .unwrap_or_else(|error| panic!("xsi:nil={nil:?} re-emits: {error}")),
                input.as_bytes()
            );
        }
        for nil in ["TRUE", "yes", "", "t rue"] {
            let input = document(nil);
            let result = codec.parse_wtype_preserving(&schema, input.as_bytes());
            assert!(
                matches!(
                    &result,
                    Err(ParseInstanceError::Fidelity(errors))
                        if errors.len() == 1
                            && errors[0].lexeme == nil
                            && errors[0].reason == panproto_inst::FidelityReason::InvalidEncoding
                ),
                "xsi:nil={nil:?} must be a typed refusal: {:?}",
                result.err()
            );
        }
    }

    #[test]
    fn xml_cst_refuses_conflicting_nil_markers_and_namespace_rebinding() {
        let schema = open_yaml_schema();
        let codec = UnifiedCodec::xml("fidelity-cst").expect("XML CST codec");
        let input = br#"<root xmlns:q="http://www.w3.org/2001/XMLSchema-instance" q:nil="true"/>"#;
        let (mut conflicting, complement) = codec
            .parse_wtype_preserving(&schema, input)
            .expect("parse nil XML");
        conflicting
            .nodes
            .get_mut(&conflicting.root)
            .expect("root")
            .extra_fields
            .insert("q:nil".into(), Value::Str("false".into()));
        assert!(matches!(
            codec.emit_wtype_preserving(&schema, &conflicting, &complement),
            Err(EmitInstanceError::Value(error))
                if error.reason == EmitValueReason::NotRepresentable
                    && error.format == "xml"
                    && error.value_type == "str"
        ));

        let (mut rebound, complement) = codec
            .parse_wtype_preserving(&schema, input)
            .expect("parse nil XML again");
        rebound
            .nodes
            .get_mut(&rebound.root)
            .expect("root")
            .extra_fields
            .insert("xmlns:q".into(), Value::Str("urn:rebound".into()));
        assert!(matches!(
            codec.emit_wtype_preserving(&schema, &rebound, &complement),
            Err(EmitInstanceError::Value(error))
                if error.reason == EmitValueReason::NotRepresentable
                    && error.format == "xml"
                    && error.value_type == "str"
        ));
    }

    /// A delimited CST codec refuses a repeated header name and a row with
    /// more fields than headers.
    fn assert_delimited_parse_refusals(codec: &UnifiedCodec, schema: &Schema, label: &str) {
        let duplicate_header = if label == "CSV" {
            b"x,x\na,b\n".as_slice()
        } else {
            b"x\tx\na\tb\n".as_slice()
        };
        let error = codec
            .parse_functor_preserving(schema, duplicate_header)
            .expect_err("duplicate headers must not collapse");
        assert!(matches!(
            error,
            ParseInstanceError::Parse { ref message, .. }
                if message.contains("duplicate tabular header name 'x'")
        ));

        let overlong_row = if label == "CSV" {
            b"1\nleft,right\n".as_slice()
        } else {
            b"1\nleft\tright\n".as_slice()
        };
        let error = codec
            .parse_functor_preserving(schema, overlong_row)
            .expect_err("a synthesized column must not overwrite header 1");
        assert!(matches!(
            error,
            ParseInstanceError::Parse { ref message, .. }
                if message.contains("data row 1")
                    && message.contains("2 fields")
                    && message.contains("1 header")
        ));
    }

    /// A delimited CST codec refuses a null or an absent cell, which it has
    /// no token for, and a cell spelling a reserved delimiter, and keeps
    /// `_` as ordinary text.
    fn assert_delimited_cell_refusals(
        codec: &UnifiedCodec,
        schema: &Schema,
        instance: &FInstance,
        complement: &CstComplement,
        row: RowId,
        label: &str,
    ) {
        let mut null = instance.clone();
        null.row_mut(row)
            .expect("value row")
            .insert("null_value".into(), Value::Null);
        let error = codec
            .emit_functor_preserving(schema, &null, complement)
            .expect_err("delimited CST has no configured null token");
        assert!(matches!(
            error,
            EmitInstanceError::Value(error)
                if error.reason == EmitValueReason::NullNotRepresentable
        ));

        let mut absent = instance.clone();
        absent
            .row_mut(row)
            .expect("value row")
            .remove("long_decimal");
        let error = codec
            .emit_functor_preserving(schema, &absent, complement)
            .expect_err("an absent delimited cell has no implicit token");
        assert!(matches!(
            error,
            EmitInstanceError::Value(error)
                if error.reason == EmitValueReason::AbsentNotRepresentable
        ));

        let mut underscore = instance.clone();
        underscore
            .row_mut(row)
            .expect("value row")
            .insert("null_value".into(), Value::Str("_".into()));
        let emitted = codec
            .emit_functor_preserving(schema, &underscore, complement)
            .expect("underscore is ordinary delimited text");
        let (reparsed, _) = codec
            .parse_functor_preserving(schema, &emitted)
            .expect("reparse underscore cell");
        assert_eq!(
            reparsed.rows("rows")[0].1.get("null_value"),
            Some(&Value::Str("_".into()))
        );

        for reserved in [
            if label == "CSV" {
                "left,right"
            } else {
                "left\tright"
            },
            "left\nright",
        ] {
            let mut invalid = instance.clone();
            invalid
                .row_mut(row)
                .expect("value row")
                .insert("null_value".into(), Value::Str(reserved.into()));
            let error = codec
                .emit_functor_preserving(schema, &invalid, complement)
                .expect_err("reserved delimited spelling must be refused");
            assert!(matches!(
                error,
                EmitInstanceError::Value(error)
                    if error.reason == EmitValueReason::NotRepresentable
            ));
        }
    }

    #[test]
    fn delimited_cst_routes_force_exact_cell_rerenders() {
        let schema = exact_value_schema("rows");
        for (codec, fixture_path, label) in [
            (
                UnifiedCodec::csv("cassandra").expect("CSV CST codec"),
                "csv/exact.csv",
                "CSV",
            ),
            (
                UnifiedCodec::tsv("cassandra", "rows").expect("TSV CST codec"),
                "tsv/exact.tsv",
                "TSV",
            ),
        ] {
            assert_delimited_parse_refusals(&codec, &schema, label);

            let input = read_fixture(fixture_path);
            let (mut instance, complement) = codec
                .parse_functor_preserving(&schema, &input)
                .unwrap_or_else(|error| panic!("parse {label} CST: {error}"));
            let row = instance.table("rows").expect("rows table").ids()[0];
            assert!(matches!(
                table_value(&instance, "rows", "nan"),
                Value::Float(value) if value.is_nan()
            ));
            assert_eq!(
                instance.rows("rows")[0].1.get("null_value"),
                Some(&Value::Str("_".into()))
            );
            assert_eq!(
                instance.rows("rows")[0].1.get("absent"),
                Some(&Value::Str("_".into()))
            );
            let unchanged = codec
                .emit_functor_preserving(&schema, &instance, &complement)
                .unwrap_or_else(|error| panic!("emit unchanged {label} CST: {error}"));
            assert_eq!(unchanged, input, "{label} changed untouched source bytes");

            let edited_row = instance.row_mut(row).expect("value row");
            let extra_edits = [
                ("nan", Value::Float(f64::INFINITY)),
                ("null_value", Value::Str("materialized-null".into())),
                ("absent", Value::Str("materialized-absent".into())),
            ];
            for (column, value) in exact_value_edits().into_iter().chain(extra_edits.clone()) {
                edited_row.insert(column.into(), value);
            }
            let emitted = codec
                .emit_functor_preserving(&schema, &instance, &complement)
                .unwrap_or_else(|error| panic!("emit edited {label} CST: {error}"));
            let (reparsed, _) = codec
                .parse_functor_preserving(&schema, &emitted)
                .unwrap_or_else(|error| panic!("reparse edited {label} CST: {error}"));
            for (column, _) in exact_value_edits().into_iter().chain(extra_edits) {
                assert_eq!(
                    table_value(&reparsed, "rows", column),
                    table_value(&instance, "rows", column),
                    "{label} changed {column} during rerender"
                );
            }

            assert_delimited_cell_refusals(&codec, &schema, &instance, &complement, row, label);
        }
    }

    #[test]
    fn csv_quotes_are_syntax_while_tsv_quotes_are_ordinary_text() {
        let schema = exact_value_schema("rows");
        for (codec, input, expected, label) in [
            (
                UnifiedCodec::csv("cassandra").expect("CSV CST codec"),
                b"\"null_value\"\n\"left,right \"\"quoted\"\"\"\n\"_\"\n".as_slice(),
                "left,right \"quoted\"",
                "CSV",
            ),
            (
                UnifiedCodec::tsv("cassandra", "rows").expect("TSV CST codec"),
                b"null_value\n\"left right \"\"quoted\"\"\"\n_\n".as_slice(),
                "\"left right \"\"quoted\"\"\"",
                "TSV",
            ),
        ] {
            let (mut instance, complement) = codec
                .parse_functor_preserving(&schema, input)
                .unwrap_or_else(|error| panic!("parse quoted {label}: {error}"));
            let rows = instance.table("rows").expect("rows table").ids();
            let row = rows[0];
            assert_eq!(
                instance
                    .row(row)
                    .and_then(|values| values.get("null_value")),
                Some(&Value::Str(expected.to_owned()))
            );
            assert_eq!(
                instance
                    .row(rows[1])
                    .and_then(|values| values.get("null_value")),
                Some(&Value::Str("_".into()))
            );
            instance
                .row_mut(row)
                .expect("quoted row")
                .insert("null_value".into(), Value::Str("replacement".into()));
            let emitted = codec
                .emit_functor_preserving(&schema, &instance, &complement)
                .unwrap_or_else(|error| panic!("emit edited {label}: {error}"));
            let (reparsed, _) = codec
                .parse_functor_preserving(&schema, &emitted)
                .unwrap_or_else(|error| panic!("reparse edited {label}: {error}"));
            assert_eq!(
                reparsed
                    .row(row)
                    .and_then(|values| values.get("null_value")),
                Some(&Value::Str("replacement".into()))
            );
            assert_eq!(
                reparsed
                    .row(rows[1])
                    .and_then(|values| values.get("null_value")),
                Some(&Value::Str("_".into()))
            );
        }
    }

    fn assert_tree_structural_refusal(
        codec: &UnifiedCodec,
        schema: &Schema,
        instance: &WInstance,
        complement: &CstComplement,
        format: &str,
    ) {
        assert!(matches!(
            codec.emit_wtype_preserving(schema, instance, complement),
            Err(EmitInstanceError::UnsupportedStructuralEdit {
                format: ref actual,
                ..
            }) if actual == format
        ));
    }

    #[test]
    fn preserving_tree_emitters_refuse_added_removed_reordered_and_reparented_nodes() {
        let schema = exact_value_schema("root");
        for (codec, input, format) in [
            (
                UnifiedCodec::json("cassandra").expect("JSON CST codec"),
                br#"{"long_decimal":1.0,"numeric_38_10":2.0}"#.as_slice(),
                "json",
            ),
            (
                UnifiedCodec::yaml("cassandra").expect("YAML CST codec"),
                b"long_decimal: 1.0\nnumeric_38_10: 2.0\n".as_slice(),
                "yaml",
            ),
            (
                UnifiedCodec::toml("cassandra").expect("TOML CST codec"),
                b"long_decimal = 1.0\nnumeric_38_10 = 2.0\n".as_slice(),
                "toml",
            ),
            (
                UnifiedCodec::xml("cassandra").expect("XML CST codec"),
                b"<root><long_decimal>1.0</long_decimal><numeric_38_10>2.0</numeric_38_10></root>"
                    .as_slice(),
                "xml",
            ),
        ] {
            let (instance, complement) = codec
                .parse_wtype_preserving(&schema, input)
                .unwrap_or_else(|error| panic!("parse {format} structural fixture: {error}"));
            assert!(instance.arcs.len() >= 2, "{format} fixture needs two arcs");

            let mut added = instance.clone();
            let added_id = added.nodes.keys().max().copied().expect("root node") + 1;
            added
                .nodes
                .insert(added_id, Node::new(added_id, "unaddressed"));
            assert_tree_structural_refusal(&codec, &schema, &added, &complement, format);

            let mut removed = instance.clone();
            let removed_id = removed.nodes.keys().max().copied().expect("leaf node");
            removed.nodes.remove(&removed_id);
            assert_tree_structural_refusal(&codec, &schema, &removed, &complement, format);

            let mut reordered = instance.clone();
            reordered.arcs.reverse();
            assert_tree_structural_refusal(&codec, &schema, &reordered, &complement, format);

            let mut reparented = instance.clone();
            reparented.arcs[1].0 = reparented.arcs[0].1;
            assert_tree_structural_refusal(&codec, &schema, &reparented, &complement, format);
        }
    }

    #[test]
    fn xml_opaque_nodes_cannot_be_rewritten_as_elements() {
        let schema = exact_value_schema("doc");
        let codec = UnifiedCodec::xml("cassandra").expect("XML CST codec");
        let (mut instance, complement) = codec
            .parse_wtype_preserving(&schema, b"<doc><!--kept--></doc>")
            .expect("parse XML opaque node");
        let opaque = instance
            .nodes
            .values()
            .find(|node| node.shape == NodeShape::XmlTextSegment)
            .map(|node| node.id)
            .expect("opaque XML node");
        instance.nodes.get_mut(&opaque).expect("opaque node").value =
            Some(FieldPresence::Present(Value::Str("replacement".into())));
        assert_tree_structural_refusal(&codec, &schema, &instance, &complement, "xml");
    }

    #[test]
    fn xml_cst_rejects_content_inside_xsi_nil_elements() {
        let schema = exact_value_schema("doc");
        let codec = UnifiedCodec::xml("cassandra").expect("XML CST codec");
        for input in [
            br#"<doc xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:nil="true">payload</doc>"#.as_slice(),
            br#"<doc xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:nil="true"> </doc>"#.as_slice(),
            br#"<doc xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:nil="true"><empty>child</empty></doc>"#.as_slice(),
        ] {
            assert!(matches!(
                codec.parse_wtype_preserving(&schema, input),
                Err(ParseInstanceError::Parse { ref message, .. })
                    if message.contains("xsi:nil XML element cannot contain")
            ));
        }

        codec
            .parse_wtype_preserving(
                &schema,
                br#"<doc xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:nil="true"><![CDATA[]]></doc>"#,
            )
            .expect("empty CDATA carries no characters");
    }

    #[test]
    fn preserving_tabular_emitters_refuse_row_and_column_structure_edits() {
        let schema = exact_value_schema("rows");
        for (codec, fixture_path, format) in [
            (
                UnifiedCodec::csv("cassandra").expect("CSV CST codec"),
                "csv/exact.csv",
                "csv",
            ),
            (
                UnifiedCodec::tsv("cassandra", "rows").expect("TSV CST codec"),
                "tsv/exact.tsv",
                "tsv",
            ),
        ] {
            let input = read_fixture(fixture_path);
            let (instance, complement) = codec
                .parse_functor_preserving(&schema, &input)
                .unwrap_or_else(|error| panic!("parse {format} structural fixture: {error}"));
            let row = instance.table("rows").expect("rows table").ids()[0];

            let mut wrong_format = complement.clone();
            wrong_format.format = if format == "csv" { "tsv" } else { "csv" }.into();
            assert!(matches!(
                codec.emit_functor_preserving(&schema, &instance, &wrong_format),
                Err(EmitInstanceError::UnsupportedStructuralEdit {
                    format: ref actual,
                    ..
                }) if actual == format
            ));

            let mut removed = instance.clone();
            assert!(removed.remove_row(row).is_some());
            assert!(matches!(
                codec.emit_functor_preserving(&schema, &removed, &complement),
                Err(EmitInstanceError::UnsupportedStructuralEdit {
                    format: ref actual,
                    ..
                }) if actual == format
            ));

            let mut added = instance.clone();
            added
                .push_row(
                    "rows",
                    HashMap::from([("long_decimal".into(), Value::Int(1))]),
                )
                .expect("add row");
            assert!(matches!(
                codec.emit_functor_preserving(&schema, &added, &complement),
                Err(EmitInstanceError::UnsupportedStructuralEdit {
                    format: ref actual,
                    ..
                }) if actual == format
            ));

            let mut extra_column = instance.clone();
            extra_column
                .row_mut(row)
                .expect("value row")
                .insert("unaddressed".into(), Value::Str("extra".into()));
            assert!(matches!(
                codec.emit_functor_preserving(&schema, &extra_column, &complement),
                Err(EmitInstanceError::UnsupportedStructuralEdit {
                    format: ref actual,
                    ..
                }) if actual == format
            ));

            assert_eq!(row, RowId(0));
        }
    }
}
