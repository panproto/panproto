#![allow(clippy::similar_names)]

//! Round-trip tests with real fixture data for all 50 protocols.
//!
//! Each test verifies the presentation functor's faithfulness:
//! `parse(emit(parse(input))) ≅ parse(input)`, structural equality
//! after a full round-trip through parse → emit → re-parse.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_io::ProtocolRegistry;
use panproto_io::byte_tabular::ByteTabularCodec;
use panproto_io::traits::NativeRepr;
use panproto_schema::{Protocol, SchemaBuilder};

/// Build a minimal open schema for JSON/XML round-trip testing.
/// Open schemas accept any vertex kind and edge kind.
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

/// Build a minimal open schema for tabular protocols.
fn tabular_schema(protocol_name: &str) -> panproto_schema::Schema {
    let proto = Protocol {
        name: protocol_name.into(),
        schema_theory: format!("Th{protocol_name}Schema"),
        instance_theory: format!("Th{protocol_name}Instance"),
        edge_rules: vec![],
        obj_kinds: vec![],
        constraint_sorts: vec![],
        ..Protocol::default()
    };
    // Tabular schemas need no vertices; FInstance is schema-independent.
    SchemaBuilder::new(&proto)
        .vertex("table", "table", None)
        .expect("table vertex")
        .build()
        .expect("build schema")
}

fn registry() -> ProtocolRegistry {
    panproto_io::default_registry()
}

// ── Macro for JSON-based WType round-trip tests ────────────────────────

macro_rules! json_wtype_roundtrip {
    ($name:ident, $protocol:expr, $fixture:expr) => {
        #[test]
        fn $name() {
            let reg = registry();
            let schema = open_schema($protocol);
            let input = include_bytes!($fixture);

            // Parse: raw bytes → WInstance (root node with extra_fields
            // capturing the full JSON structure when schema has no edges).
            let instance = reg
                .parse_wtype($protocol, &schema, input)
                .expect(concat!("parse ", $protocol));

            assert!(
                instance.node_count() >= 1,
                "{}: expected at least 1 node (root), got {}",
                $protocol,
                instance.node_count()
            );

            // Emit → re-parse: verify structural stability.
            let emitted = reg
                .emit_wtype($protocol, &schema, &instance)
                .expect(concat!("emit ", $protocol));

            let instance2 = reg
                .parse_wtype($protocol, &schema, &emitted)
                .expect(concat!("re-parse ", $protocol));

            assert_eq!(
                instance.node_count(),
                instance2.node_count(),
                "{}: node count mismatch after round-trip",
                $protocol
            );
        }
    };
}

// ── Macro for XML-based WType round-trip tests ─────────────────────────

macro_rules! xml_wtype_roundtrip {
    ($name:ident, $protocol:expr, $fixture:expr) => {
        #[test]
        fn $name() {
            let reg = registry();
            let schema = open_schema($protocol);
            let input = include_bytes!($fixture);

            let instance = reg
                .parse_wtype($protocol, &schema, input)
                .expect(concat!("parse ", $protocol));

            assert!(
                instance.node_count() >= 2,
                "{}: expected at least 2 nodes, got {}",
                $protocol,
                instance.node_count()
            );

            let emitted = reg
                .emit_wtype($protocol, &schema, &instance)
                .expect(concat!("emit ", $protocol));

            let instance2 = reg
                .parse_wtype($protocol, &schema, &emitted)
                .expect(concat!("re-parse ", $protocol));

            assert_eq!(
                instance.node_count(),
                instance2.node_count(),
                "{}: node count mismatch after round-trip",
                $protocol
            );
        }
    };
}

// ── Macro for tabular Functor round-trip tests ─────────────────────────

macro_rules! tabular_functor_roundtrip {
    ($name:ident, $protocol:expr, $fixture:expr, $table:expr) => {
        #[test]
        fn $name() {
            let reg = registry();
            let schema = tabular_schema($protocol);
            let input = include_bytes!($fixture);

            let instance = reg
                .parse_functor($protocol, &schema, input)
                .expect(concat!("parse ", $protocol));

            let rows = instance.tables.get($table).expect(concat!(
                $protocol,
                " table '",
                $table,
                "' should exist"
            ));
            assert!(
                !rows.is_empty(),
                "{}: table '{}' should have rows",
                $protocol,
                $table
            );

            let emitted = reg
                .emit_functor($protocol, &schema, &instance)
                .expect(concat!("emit ", $protocol));

            let instance2 = reg
                .parse_functor($protocol, &schema, &emitted)
                .expect(concat!("re-parse ", $protocol));

            let rows2 = instance2.tables.get($table).expect("re-parsed table");
            assert_eq!(
                rows.len(),
                rows2.len(),
                "{}: row count mismatch in '{}' after round-trip",
                $protocol,
                $table
            );
        }
    };
}

// ═══════════════════════════════════════════════════════════════════════
// API (4)
// ═══════════════════════════════════════════════════════════════════════

json_wtype_roundtrip!(
    roundtrip_openapi,
    "openapi",
    "../fixtures/api/openapi_response.json"
);
json_wtype_roundtrip!(
    roundtrip_asyncapi,
    "asyncapi",
    "../fixtures/api/asyncapi_event.json"
);
json_wtype_roundtrip!(
    roundtrip_jsonapi,
    "jsonapi",
    "../fixtures/api/jsonapi_response.json"
);
json_wtype_roundtrip!(roundtrip_raml, "raml", "../fixtures/api/raml_response.json");

// ═══════════════════════════════════════════════════════════════════════
// Data Schema (2)
// ═══════════════════════════════════════════════════════════════════════

json_wtype_roundtrip!(
    roundtrip_cddl,
    "cddl",
    "../fixtures/data_schema/cddl_instance.json"
);
json_wtype_roundtrip!(
    roundtrip_bson,
    "bson",
    "../fixtures/data_schema/bson_instance.json"
);

// ═══════════════════════════════════════════════════════════════════════
// Database (5)
// ═══════════════════════════════════════════════════════════════════════

json_wtype_roundtrip!(
    roundtrip_mongodb,
    "mongodb",
    "../fixtures/database/mongodb_document.json"
);
json_wtype_roundtrip!(
    roundtrip_dynamodb,
    "dynamodb",
    "../fixtures/database/dynamodb_item.json"
);
json_wtype_roundtrip!(
    roundtrip_cassandra,
    "cassandra",
    "../fixtures/database/cassandra_rows.json"
);
json_wtype_roundtrip!(
    roundtrip_neo4j,
    "neo4j",
    "../fixtures/database/neo4j_result.json"
);
tabular_functor_roundtrip!(
    roundtrip_redis,
    "redis",
    "../fixtures/database/redis_resp.txt",
    "entries"
);

// ═══════════════════════════════════════════════════════════════════════
// Config (3)
// ═══════════════════════════════════════════════════════════════════════

json_wtype_roundtrip!(
    roundtrip_cloudformation,
    "cloudformation",
    "../fixtures/config/cloudformation_template.json"
);
json_wtype_roundtrip!(
    roundtrip_ansible,
    "ansible",
    "../fixtures/config/ansible_playbook.json"
);
json_wtype_roundtrip!(
    roundtrip_k8s_crd,
    "k8s_crd",
    "../fixtures/config/k8s_crd.json"
);

// ═══════════════════════════════════════════════════════════════════════
// Data Science (3)
// ═══════════════════════════════════════════════════════════════════════

json_wtype_roundtrip!(
    roundtrip_dataframe,
    "dataframe",
    "../fixtures/data_science/dataframe_instance.json"
);
json_wtype_roundtrip!(
    roundtrip_parquet,
    "parquet",
    "../fixtures/data_science/parquet_record.json"
);
json_wtype_roundtrip!(
    roundtrip_arrow,
    "arrow",
    "../fixtures/data_science/arrow_batch.json"
);

// ═══════════════════════════════════════════════════════════════════════
// Serialization (5)
// ═══════════════════════════════════════════════════════════════════════

json_wtype_roundtrip!(
    roundtrip_avro,
    "avro",
    "../fixtures/serialization/avro_record.json"
);
json_wtype_roundtrip!(
    roundtrip_flatbuffers,
    "flatbuffers",
    "../fixtures/serialization/flatbuffers_table.json"
);
json_wtype_roundtrip!(
    roundtrip_asn1,
    "asn1",
    "../fixtures/serialization/asn1_cert.json"
);
json_wtype_roundtrip!(
    roundtrip_bond,
    "bond",
    "../fixtures/serialization/bond_struct.json"
);
json_wtype_roundtrip!(
    roundtrip_msgpack_schema,
    "msgpack_schema",
    "../fixtures/serialization/msgpack_data.json"
);

// ═══════════════════════════════════════════════════════════════════════
// Annotation: JSON-based (8)
// ═══════════════════════════════════════════════════════════════════════

json_wtype_roundtrip!(
    roundtrip_brat,
    "brat",
    "../fixtures/annotation/brat_annotation.json"
);
json_wtype_roundtrip!(
    roundtrip_decomp,
    "decomp",
    "../fixtures/annotation/decomp_annotation.json"
);
json_wtype_roundtrip!(
    roundtrip_ucca,
    "ucca",
    "../fixtures/annotation/ucca_passage.json"
);
json_wtype_roundtrip!(
    roundtrip_fovea,
    "fovea",
    "../fixtures/annotation/fovea_annotation.json"
);
json_wtype_roundtrip!(
    roundtrip_bead,
    "bead",
    "../fixtures/annotation/bead_experiment.json"
);
json_wtype_roundtrip!(
    roundtrip_web_annotation,
    "web_annotation",
    "../fixtures/annotation/web_annotation.json"
);
json_wtype_roundtrip!(
    roundtrip_concrete,
    "concrete",
    "../fixtures/annotation/concrete_comm.json"
);
json_wtype_roundtrip!(
    roundtrip_nif,
    "nif",
    "../fixtures/annotation/nif_document.json"
);

// ═══════════════════════════════════════════════════════════════════════
// Annotation: XML-based (9)
// ═══════════════════════════════════════════════════════════════════════

xml_wtype_roundtrip!(
    roundtrip_naf,
    "naf",
    "../fixtures/annotation/naf_document.xml"
);
xml_wtype_roundtrip!(
    roundtrip_uima,
    "uima",
    "../fixtures/annotation/uima_cas.xml"
);
xml_wtype_roundtrip!(
    roundtrip_folia,
    "folia",
    "../fixtures/annotation/folia_document.xml"
);
xml_wtype_roundtrip!(
    roundtrip_tei,
    "tei",
    "../fixtures/annotation/tei_document.xml"
);
xml_wtype_roundtrip!(
    roundtrip_timeml,
    "timeml",
    "../fixtures/annotation/timeml_document.xml"
);
xml_wtype_roundtrip!(
    roundtrip_elan,
    "elan",
    "../fixtures/annotation/elan_annotation.xml"
);
xml_wtype_roundtrip!(
    roundtrip_iso_space,
    "iso_space",
    "../fixtures/annotation/iso_space_document.xml"
);
xml_wtype_roundtrip!(
    roundtrip_paula,
    "paula",
    "../fixtures/annotation/paula_annotation.xml"
);
xml_wtype_roundtrip!(
    roundtrip_laf_graf,
    "laf_graf",
    "../fixtures/annotation/laf_graf_annotation.xml"
);

// ═══════════════════════════════════════════════════════════════════════
// Annotation: Tabular (2)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn roundtrip_conllu() {
    // CoNLL-U now rides the byte-faithful `ByteTabularCodec`: comment lines and
    // blank-line sentence boundaries are preserved verbatim, every token line
    // (including multiword `N-M` rows) becomes a positional `rows` entry, and a
    // `parse → emit` round-trip is byte-identical. The fixture is a real UD
    // CoNLL-U file with multiword tokens, so it exercises every quirk.
    let codec = ByteTabularCodec::new("conllu", "rows", b'\t', Some(b'#'));
    let input = include_bytes!("../fixtures/annotation/sample.conllu");

    let (instance, complement) = codec.parse(input).expect("parse conllu");
    let rows = instance.tables.get("rows").expect("rows table");
    assert!(
        rows.len() >= 100,
        "Real UD CoNLL-U fixture should have many token rows, got {}",
        rows.len()
    );

    let emitted = codec.emit(&instance, &complement).expect("emit conllu");
    assert_eq!(
        emitted.as_slice(),
        &input[..],
        "CoNLL-U round-trip must be byte-identical"
    );
}

tabular_functor_roundtrip!(
    roundtrip_amr,
    "amr",
    "../fixtures/annotation/amr_graph.tsv",
    "amr_graph"
);

// ═══════════════════════════════════════════════════════════════════════
// Web/Document (3)
// ═══════════════════════════════════════════════════════════════════════

json_wtype_roundtrip!(
    roundtrip_atproto,
    "atproto",
    "../fixtures/web_document/atproto_record.json"
);
xml_wtype_roundtrip!(
    roundtrip_docx,
    "docx",
    "../fixtures/web_document/docx_content.xml"
);
xml_wtype_roundtrip!(
    roundtrip_odf,
    "odf",
    "../fixtures/web_document/odf_content.xml"
);

// ═══════════════════════════════════════════════════════════════════════
// Domain (6)
// ═══════════════════════════════════════════════════════════════════════

json_wtype_roundtrip!(
    roundtrip_geojson,
    "geojson",
    "../fixtures/domain/geojson_features.json"
);
json_wtype_roundtrip!(
    roundtrip_fhir,
    "fhir",
    "../fixtures/domain/fhir_patient.json"
);
json_wtype_roundtrip!(
    roundtrip_vcard_ical,
    "vcard_ical",
    "../fixtures/domain/vcard_contact.json"
);
xml_wtype_roundtrip!(
    roundtrip_rss_atom,
    "rss_atom",
    "../fixtures/domain/rss_feed.xml"
);
tabular_functor_roundtrip!(
    roundtrip_swift_mt,
    "swift_mt",
    "../fixtures/domain/swift_mt103.txt",
    "fields"
);
tabular_functor_roundtrip!(
    roundtrip_edi_x12,
    "edi_x12",
    "../fixtures/domain/edi_x12_850.txt",
    "segments"
);

// ═══════════════════════════════════════════════════════════════════════
// Coverage verification
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn all_protocols_have_roundtrip_tests() {
    // This test ensures we haven't missed any protocol.
    // The count of tests in this file must match the registry.
    let reg = registry();
    assert_eq!(
        reg.len(),
        50,
        "registry should have 50 protocols; if you add a protocol, add a round-trip test"
    );
}

#[test]
fn all_protocols_report_correct_native_repr() {
    let reg = registry();

    // WType protocols should support parse_wtype
    let wtype_protocols = ["openapi", "atproto", "brat", "naf", "tei", "avro", "cddl"];
    for p in &wtype_protocols {
        assert_eq!(
            reg.native_repr(p).unwrap(),
            NativeRepr::WType,
            "{p} should be WType"
        );
    }

    // Functor protocols should support parse_functor
    let functor_protocols = ["conllu", "amr"];
    for p in &functor_protocols {
        assert_eq!(
            reg.native_repr(p).unwrap(),
            NativeRepr::Functor,
            "{p} should be Functor"
        );
    }
}

// ── Structural array round-trip tests ──────────────────────────────────
//
// The macro-generated tests above check `node_count` equality, which
// is satisfied even when arrays collapse to `{"item": x}` objects on
// emit (the node count survives, only the JSON structure breaks). The
// tests below assert the *bytes* round-trip parses back to the same
// `serde_json::Value`, catching regressions where empty / singleton /
// many-element arrays drop, collapse, or wrap into objects.

#[cfg(feature = "tree-sitter")]
fn parse_emit_parse_json(protocol: &str, input: &[u8]) -> serde_json::Value {
    let reg = registry();
    let schema = open_schema(protocol);
    let inst = reg
        .parse_wtype(protocol, &schema, input)
        .expect("parse should succeed");
    let emitted = reg
        .emit_wtype(protocol, &schema, &inst)
        .expect("emit should succeed");
    serde_json::from_slice(&emitted).expect("emitted bytes must be valid JSON")
}

#[cfg(feature = "tree-sitter")]
#[test]
fn json_array_with_many_elements_survives_round_trip() {
    let v = parse_emit_parse_json("openapi", br#"{"items":["a","b","c"]}"#);
    assert_eq!(
        v.pointer("/items"),
        Some(&serde_json::json!(["a", "b", "c"])),
        "many-element array must round-trip as a JSON array; got {v}"
    );
}

#[cfg(feature = "tree-sitter")]
#[test]
fn json_singleton_array_survives_round_trip() {
    let v = parse_emit_parse_json("openapi", br#"{"langs":["en"]}"#);
    assert_eq!(
        v.pointer("/langs"),
        Some(&serde_json::json!(["en"])),
        "singleton array must stay an array, not collapse to {{\"item\": x}}; got {v}"
    );
}

#[cfg(feature = "tree-sitter")]
#[test]
fn json_empty_array_survives_round_trip() {
    let v = parse_emit_parse_json("openapi", br#"{"errors":[]}"#);
    assert_eq!(
        v.pointer("/errors"),
        Some(&serde_json::json!([])),
        "empty array must stay an empty array, not become {{}}; got {v}"
    );
}

#[cfg(feature = "tree-sitter")]
#[test]
fn json_array_of_objects_survives_round_trip() {
    let v = parse_emit_parse_json("openapi", br#"{"facets":[{"id":1},{"id":2}]}"#);
    assert_eq!(
        v.pointer("/facets"),
        Some(&serde_json::json!([{"id": 1}, {"id": 2}])),
        "array of objects must round-trip element-for-element; got {v}"
    );
}

#[cfg(feature = "tree-sitter")]
#[test]
fn json_nested_arrays_survive_round_trip() {
    let v = parse_emit_parse_json("openapi", br#"{"matrix":[[1,2],[3,4]]}"#);
    assert_eq!(
        v.pointer("/matrix"),
        Some(&serde_json::json!([[1, 2], [3, 4]])),
        "nested arrays must round-trip; got {v}"
    );
}

#[cfg(feature = "tree-sitter")]
#[test]
fn tsv_header_and_rows_survive_round_trip() {
    let reg = registry();
    let schema = tabular_schema("amr");
    let input = b"variable\tconcept\nb\tboy\nw\twant-01\n" as &[u8];
    let inst = reg
        .parse_functor("amr", &schema, input)
        .expect("parse should succeed");
    let rows = inst
        .tables
        .get("amr_graph")
        .expect("amr_graph table present");
    assert_eq!(rows.len(), 2);
    let row0 = &rows[0];
    let row1 = &rows[1];
    let names: std::collections::BTreeSet<&str> = row0.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        std::collections::BTreeSet::from(["variable", "concept"]),
        "TSV column names must be the literal header text, not empty strings"
    );
    assert!(matches!(
        row0.get("variable"),
        Some(panproto_inst::Value::Str(s)) if s == "b"
    ));
    assert!(matches!(
        row1.get("concept"),
        Some(panproto_inst::Value::Str(s)) if s == "want-01"
    ));
    let emitted = reg
        .emit_functor("amr", &schema, &inst)
        .expect("emit should succeed");
    let inst2 = reg
        .parse_functor("amr", &schema, &emitted)
        .expect("re-parse should succeed");
    let rows2 = inst2
        .tables
        .get("amr_graph")
        .expect("re-parsed table present");
    assert_eq!(
        rows.len(),
        rows2.len(),
        "row count must match after round-trip"
    );
    for (a, b) in rows.iter().zip(rows2.iter()) {
        assert_eq!(a, b, "row content must match after round-trip");
    }
}
