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

// ─── TOML corpus ──────────────────────────────────────────────────────

#[test]
fn byte_eq_toml_key_value() {
    let codec = UnifiedCodec::toml("test").unwrap();
    let schema = open_schema("test");
    let input = b"key = \"value\"\nnum = 42\n";
    assert_byte_equal_round_trip(&codec, &schema, input, "toml_key_value");
}

#[test]
fn byte_eq_toml_table() {
    let codec = UnifiedCodec::toml("test").unwrap();
    let schema = open_schema("test");
    let input = b"[server]\nhost = \"localhost\"\nport = 8080\n";
    assert_byte_equal_round_trip(&codec, &schema, input, "toml_table");
}

#[test]
fn byte_eq_toml_comments_and_whitespace() {
    let codec = UnifiedCodec::toml("test").unwrap();
    let schema = open_schema("test");
    // Comments and irregular spacing live in the layout complement; the
    // round-trip must preserve them byte-for-byte.
    let input = b"# top comment\n\ntitle   =   \"demo\"  # trailing\n[deps]\nserde = \"1.0\"\n";
    assert_byte_equal_round_trip(&codec, &schema, input, "toml_comments");
}

// ─── YAML corpus ──────────────────────────────────────────────────────

#[test]
fn byte_eq_yaml_mapping() {
    let codec = UnifiedCodec::yaml("test").unwrap();
    let schema = open_schema("test");
    let input = b"name: Alice\nvalue: 42\n";
    assert_byte_equal_round_trip(&codec, &schema, input, "yaml_mapping");
}

#[test]
fn byte_eq_yaml_block_sequence() {
    let codec = UnifiedCodec::yaml("test").unwrap();
    let schema = open_schema("test");
    let input = b"items:\n  - a\n  - b\n";
    assert_byte_equal_round_trip(&codec, &schema, input, "yaml_block_sequence");
}

#[test]
fn byte_eq_yaml_comments_and_indentation() {
    let codec = UnifiedCodec::yaml("test").unwrap();
    let schema = open_schema("test");
    let input = b"# header\nserver:\n  host: localhost   # inline\n  ports:\n    - 80\n    - 443\n";
    assert_byte_equal_round_trip(&codec, &schema, input, "yaml_comments");
}

// ─── CSV / TSV corpus (tabular, format-preserving functor path) ───────

fn assert_tabular_byte_equal_round_trip(
    codec: &UnifiedCodec,
    schema: &panproto_schema::Schema,
    input: &[u8],
    label: &str,
) {
    let (instance, complement) = codec
        .parse_functor_preserving(schema, input)
        .unwrap_or_else(|e| panic!("[{label}] parse_functor_preserving failed: {e}"));
    let emitted = codec
        .emit_functor_preserving(schema, &instance, &complement)
        .unwrap_or_else(|e| panic!("[{label}] emit_functor_preserving failed: {e}"));
    assert!(
        emitted == input,
        "[{label}] tabular round-trip violated byte equality\n\
         input:\n{}\n\nemitted:\n{}",
        String::from_utf8_lossy(input),
        String::from_utf8_lossy(&emitted),
    );
}

fn tabular_schema() -> panproto_schema::Schema {
    let proto = Protocol {
        name: "test".into(),
        schema_theory: "ThtestSchema".into(),
        instance_theory: "ThtestInstance".into(),
        ..Protocol::default()
    };
    SchemaBuilder::new(&proto)
        .vertex("rows", "object", None)
        .expect("rows vertex")
        .build()
        .expect("build schema")
}

#[test]
fn byte_eq_csv_simple() {
    let codec = UnifiedCodec::csv("test").unwrap();
    let schema = tabular_schema();
    // Column order must survive (the legacy functor path reordered columns).
    let input = b"name,age\nAlice,30\nBob,25\n";
    assert_tabular_byte_equal_round_trip(&codec, &schema, input, "csv_simple");
}

#[test]
fn byte_eq_csv_quoted_embedded_delimiter() {
    let codec = UnifiedCodec::csv("test").unwrap();
    let schema = tabular_schema();
    let input = b"name,note\nAlice,\"hello, world\"\nBob,\"a\nb\"\n";
    assert_tabular_byte_equal_round_trip(&codec, &schema, input, "csv_quoted");
}

#[test]
fn byte_eq_csv_no_trailing_newline_and_empty_cells() {
    let codec = UnifiedCodec::csv("test").unwrap();
    let schema = tabular_schema();
    let input = b"a,b,c\n1,,3";
    assert_tabular_byte_equal_round_trip(&codec, &schema, input, "csv_empty_cells");
}

#[test]
fn byte_eq_tsv_simple() {
    let codec = UnifiedCodec::tsv("test", "rows").unwrap();
    let schema = tabular_schema();
    let input = b"name\tage\nAlice\t30\nBob\t25\n";
    assert_tabular_byte_equal_round_trip(&codec, &schema, input, "tsv_simple");
}

/// The format-preserving tabular path must also apply edits: changing a cell
/// value rewrites exactly that field while keeping every other byte intact.
#[test]
fn tabular_edit_rewrites_one_cell() {
    use panproto_inst::value::Value;
    let codec = UnifiedCodec::csv("test").unwrap();
    let schema = tabular_schema();
    let input = b"name,age\nAlice,30\nBob,25\n";
    let (mut instance, complement) = codec.parse_functor_preserving(&schema, input).unwrap();
    for rows in instance.tables.values_mut() {
        for row in rows.iter_mut() {
            if row.get("name") == Some(&Value::Str("Alice".into())) {
                row.insert("age".into(), Value::Str("99".into()));
            }
        }
    }
    let emitted = codec
        .emit_functor_preserving(&schema, &instance, &complement)
        .unwrap();
    assert_eq!(emitted.as_slice(), b"name,age\nAlice,99\nBob,25\n");
}
