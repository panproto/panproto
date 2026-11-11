//! Open-schema lexemes whose carriers are fixed by the fidelity contract.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use panproto_inst::WInstance;
use panproto_inst::value::{FieldPresence, Value};
use panproto_io::json_pathway::parse_json_bytes;
use panproto_schema::SchemaBuilder;

fn fixture() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/value_fidelity/json/open_lexemes.json"),
    )
    .expect("open-lexeme fixture")
}

fn value<'a>(instance: &'a WInstance, anchor: &str) -> &'a Value {
    let presence = instance
        .nodes
        .values()
        .find(|node| node.anchor.as_ref() == anchor)
        .and_then(|node| node.value.as_ref())
        .unwrap_or_else(|| panic!("missing {anchor}"));
    let FieldPresence::Present(value) = presence else {
        panic!("{anchor} is not present: {presence:?}");
    };
    value
}

#[test]
fn open_json_lexemes_choose_the_exact_contract_carriers() {
    let protocol = panproto_protocols::database::cassandra::protocol();
    let mut builder = SchemaBuilder::new(&protocol)
        .vertex("root", "table", None)
        .expect("root");
    for anchor in [
        "float_scale_010",
        "float_scale_150",
        "below_binary64",
        "min_binary64",
        "beyond_safe_integer",
        "negative_zero",
        "datetime_as_string",
        "two_to_sixty",
    ] {
        builder = builder
            .vertex(anchor, "column", None)
            .unwrap_or_else(|error| panic!("vertex {anchor}: {error}"))
            .edge("root", anchor, "prop", Some(anchor))
            .unwrap_or_else(|error| panic!("edge {anchor}: {error}"));
    }
    builder = builder
        .vertex("datetime", "timestamp", None)
        .expect("datetime")
        .edge("root", "datetime", "prop", Some("datetime"))
        .expect("datetime edge");
    let schema = builder.build().expect("open schema");
    let instance =
        parse_json_bytes(&schema, "root", &fixture(), "cassandra").expect("parse fixture");

    assert!(
        matches!(value(&instance, "float_scale_010"), Value::Float(x) if x.to_bits() == 0.1_f64.to_bits())
    );
    assert!(
        matches!(value(&instance, "float_scale_150"), Value::Float(x) if x.to_bits() == 1.5_f64.to_bits())
    );
    assert!(
        matches!(value(&instance, "below_binary64"), Value::Decimal(x) if x.to_string() == "4e-324")
    );
    assert!(matches!(value(&instance, "min_binary64"), Value::Float(x) if x.to_bits() == 1));
    assert!(
        matches!(value(&instance, "beyond_safe_integer"), Value::Decimal(x) if x.to_string() == "9007199254740993.0")
    );
    assert_eq!(value(&instance, "negative_zero"), &Value::Int(0));
    assert!(
        matches!(value(&instance, "datetime"), Value::Temporal(x) if x.to_string() == "2026-09-28T12:00:00.123456789012Z")
    );
    assert_eq!(
        value(&instance, "datetime_as_string"),
        &Value::Str("2026-09-28T12:00:00.123456789012Z".into())
    );
    assert!(
        matches!(value(&instance, "two_to_sixty"), Value::Float(x) if x.to_bits() == 0x43b0_0000_0000_0000)
    );
}
