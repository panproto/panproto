//! Prop1 regressions for the canonical JSON, XML, and byte-tabular routes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_inst::WInstance;
use panproto_inst::value::{FieldPresence, Value};
use panproto_io::byte_tabular::ByteTabularCodec;
use panproto_io::json_pathway::{emit_json_bytes, parse_json_bytes};
use panproto_io::xml_pathway::{emit_xml_bytes, parse_xml_bytes};
use panproto_schema::{Schema, SchemaBuilder};

fn value_schema(root: &str, include_nan: bool) -> Schema {
    let protocol = panproto_protocols::database::cassandra::protocol();
    let mut builder = SchemaBuilder::new(&protocol)
        .vertex(root, "table", None)
        .expect("root")
        .vertex("decimal", "decimal", None)
        .expect("decimal")
        .vertex("bigint", "varint", None)
        .expect("bigint")
        .vertex("timestamp", "timestamp", None)
        .expect("timestamp")
        .vertex("text", "text", None)
        .expect("text")
        .edge(root, "decimal", "prop", Some("decimal"))
        .expect("decimal edge")
        .edge(root, "bigint", "prop", Some("bigint"))
        .expect("bigint edge")
        .edge(root, "timestamp", "prop", Some("timestamp"))
        .expect("timestamp edge")
        .edge(root, "text", "prop", Some("text"))
        .expect("text edge");
    if include_nan {
        builder = builder
            .vertex("nan", "double", None)
            .expect("nan")
            .edge(root, "nan", "prop", Some("nan"))
            .expect("nan edge");
    }
    builder.build().expect("value schema")
}

fn value_at<'a>(instance: &'a WInstance, anchor: &str) -> &'a FieldPresence {
    instance
        .nodes
        .values()
        .find(|node| node.anchor.as_ref() == anchor)
        .and_then(|node| node.value.as_ref())
        .unwrap_or_else(|| panic!("missing value at {anchor}"))
}

fn assert_finite_values_equal(left: &WInstance, right: &WInstance) {
    for anchor in ["decimal", "bigint", "timestamp", "text"] {
        assert_eq!(value_at(left, anchor), value_at(right, anchor), "{anchor}");
    }
}

#[test]
fn every_parsed_value_emits_on_the_value_io_routes() {
    const LONG_TIME: &str = "2026-09-28T12:00:00.123456789012Z";

    let json_schema = value_schema("root", false);
    let json_input = format!(
        "{{\"decimal\":0.1000000000000000055511151231257827,\"bigint\":100000000000000000000,\"timestamp\":\"{LONG_TIME}\",\"text\":\"_\"}}"
    );
    let json = parse_json_bytes(&json_schema, "root", json_input.as_bytes(), "cassandra")
        .expect("every accepted JSON value has a carrier");
    let json_bytes =
        emit_json_bytes(&json_schema, &json, "cassandra").expect("accepted JSON values emit");
    let json_again = parse_json_bytes(&json_schema, "root", &json_bytes, "cassandra")
        .expect("emitted JSON reparses");
    assert_finite_values_equal(&json, &json_again);
    assert!(matches!(
        value_at(&json_again, "timestamp"),
        FieldPresence::Present(Value::Temporal(value)) if value.to_string() == LONG_TIME
    ));

    let xml_schema = value_schema("doc", true);
    let xml_input = format!(
        "<doc><decimal>0.1000000000000000055511151231257827</decimal><bigint>100000000000000000000</bigint><timestamp>{LONG_TIME}</timestamp><text>_</text><nan>-NaN</nan></doc>"
    );
    let xml = parse_xml_bytes(&xml_schema, xml_input.as_bytes(), "cassandra")
        .expect("every accepted XML value has a carrier");
    let xml_bytes =
        emit_xml_bytes(&xml_schema, &xml, "cassandra").expect("accepted XML values emit");
    let xml_again =
        parse_xml_bytes(&xml_schema, &xml_bytes, "cassandra").expect("emitted XML reparses");
    assert_finite_values_equal(&xml, &xml_again);
    assert!(matches!(
        value_at(&xml_again, "nan"),
        FieldPresence::Present(Value::Float(value))
            if value.to_bits() == 0xfff8_0000_0000_0000
    ));

    let byte_input = format!(
        "decimal|0.1000000000000000055511151231257827\nbigint|100000000000000000000\ntimestamp|{LONG_TIME}\ntext|_\n"
    );
    let codec = ByteTabularCodec::new("value-fidelity", "rows", b'|', None);
    let (table, complement) = codec
        .parse(byte_input.as_bytes())
        .expect("byte-tabular parsing is total");
    let byte_output = codec
        .emit(&table, &complement)
        .expect("every parsed byte-tabular value emits");
    assert_eq!(byte_output, byte_input.as_bytes());
}
