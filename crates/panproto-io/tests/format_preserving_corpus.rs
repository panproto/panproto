//! Byte-equal round-trip property test for the format-preserving
//! `UnifiedCodec`: for every fixture in the curated corpus,
//! `emit_wtype_preserving(parse_wtype_preserving(bytes)) == bytes`.
//!
//! This is the load-bearing claim of the CST-extraction pipeline:
//! formatting (whitespace, ordering, comments where applicable) is
//! captured as a complement and reinjected on emit, so unmodified
//! data round-trips losslessly at the byte level. The macro-driven
//! tests in `roundtrip.rs` only check structural (`node_count`)
//! stability across `parse → emit → re-parse`; this file asserts the
//! stricter byte-identity property the audit flagged as untested.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_io::unified_codec::UnifiedCodec;
use panproto_schema::{Protocol, SchemaBuilder};

fn open_schema(protocol_name: &str) -> panproto_schema::Schema {
    let proto = Protocol {
        name: protocol_name.into(),
        schema_theory: format!("Th{protocol_name}Schema"),
        instance_theory: format!("Th{protocol_name}Instance"),
        edge_rules: vec![],
        obj_kinds: vec![],
        constraint_sorts: vec![],
        ..Protocol::default()
    };
    SchemaBuilder::new(&proto)
        .vertex("root", "object", None)
        .expect("root vertex")
        .build()
        .expect("build schema")
}

fn assert_byte_equal_round_trip(
    codec: &UnifiedCodec,
    schema: &panproto_schema::Schema,
    input: &[u8],
    label: &str,
) {
    let (instance, complement) = codec
        .parse_wtype_preserving(schema, input)
        .unwrap_or_else(|e| panic!("[{label}] parse_wtype_preserving failed: {e}"));
    let emitted = codec
        .emit_wtype_preserving(schema, &instance, &complement)
        .unwrap_or_else(|e| panic!("[{label}] emit_wtype_preserving failed: {e}"));
    if emitted != input {
        let in_str = String::from_utf8_lossy(input);
        let out_str = String::from_utf8_lossy(&emitted);
        panic!(
            "[{label}] format-preserving round-trip violated byte equality\n\
             input ({} bytes):\n{in_str}\n\n\
             emitted ({} bytes):\n{out_str}",
            input.len(),
            emitted.len(),
        );
    }
}

// ─── JSON corpus ──────────────────────────────────────────────────────

macro_rules! json_byte_round_trip {
    ($name:ident, $protocol:expr, $fixture:expr) => {
        #[test]
        fn $name() {
            let codec = UnifiedCodec::json($protocol).expect("json codec");
            let schema = open_schema($protocol);
            let input = include_bytes!($fixture);
            assert_byte_equal_round_trip(&codec, &schema, input, stringify!($name));
        }
    };
}

json_byte_round_trip!(
    byte_eq_openapi,
    "openapi",
    "../fixtures/api/openapi_response.json"
);
json_byte_round_trip!(
    byte_eq_atproto,
    "atproto",
    "../fixtures/web_document/atproto_record.json"
);
json_byte_round_trip!(
    byte_eq_geojson,
    "geojson",
    "../fixtures/domain/geojson_features.json"
);
json_byte_round_trip!(byte_eq_fhir, "fhir", "../fixtures/domain/fhir_patient.json");
json_byte_round_trip!(
    byte_eq_brat,
    "brat",
    "../fixtures/annotation/brat_annotation.json"
);

// ─── XML corpus ───────────────────────────────────────────────────────

macro_rules! xml_byte_round_trip {
    ($name:ident, $protocol:expr, $fixture:expr) => {
        #[test]
        fn $name() {
            let codec = UnifiedCodec::xml($protocol).expect("xml codec");
            let schema = open_schema($protocol);
            let input = include_bytes!($fixture);
            assert_byte_equal_round_trip(&codec, &schema, input, stringify!($name));
        }
    };
}

xml_byte_round_trip!(
    byte_eq_tei,
    "tei",
    "../fixtures/annotation/tei_document.xml"
);
xml_byte_round_trip!(
    byte_eq_naf,
    "naf",
    "../fixtures/annotation/naf_document.xml"
);
xml_byte_round_trip!(byte_eq_rss, "rss_atom", "../fixtures/domain/rss_feed.xml");

// ─── Synthetic corpus (small, hand-crafted; assert *known* byte
// equality to catch regressions in the tightest possible cases). ───

#[test]
fn byte_eq_json_minimal_object() {
    let codec = UnifiedCodec::json("test").unwrap();
    let schema = open_schema("test");
    let input = br#"{"name": "Alice", "value": 42}"#;
    assert_byte_equal_round_trip(&codec, &schema, input, "json_minimal_object");
}

#[test]
fn byte_eq_json_pretty_printed() {
    let codec = UnifiedCodec::json("test").unwrap();
    let schema = open_schema("test");
    let input = b"{\n  \"name\": \"Alice\",\n  \"value\": 42\n}";
    assert_byte_equal_round_trip(&codec, &schema, input, "json_pretty");
}

#[test]
fn byte_eq_json_with_trailing_newline() {
    let codec = UnifiedCodec::json("test").unwrap();
    let schema = open_schema("test");
    let input = b"{\"k\": 1}\n";
    assert_byte_equal_round_trip(&codec, &schema, input, "json_trailing_newline");
}

#[test]
fn byte_eq_json_nested_array() {
    let codec = UnifiedCodec::json("test").unwrap();
    let schema = open_schema("test");
    let input = br#"{"matrix":[[1,2,3],[4,5,6]]}"#;
    assert_byte_equal_round_trip(&codec, &schema, input, "json_nested_array");
}
