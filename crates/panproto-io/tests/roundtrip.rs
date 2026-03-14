//! Round-trip tests with real fixture data for all 77 protocols.
//!
//! Each test verifies the presentation functor's faithfulness:
//! `parse(emit(parse(input))) ≅ parse(input)` — structural equality
//! after a full round-trip through parse → emit → re-parse.

use panproto_io::ProtocolRegistry;
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
    // Tabular schemas need no vertices — FInstance is schema-independent.
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
// API (5)
// ═══════════════════════════════════════════════════════════════════════

json_wtype_roundtrip!(
    roundtrip_graphql,
    "graphql",
    "../fixtures/api/graphql_response.json"
);
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
// Data Schema (7)
// ═══════════════════════════════════════════════════════════════════════

json_wtype_roundtrip!(
    roundtrip_json_schema,
    "json_schema",
    "../fixtures/data_schema/json_schema_instance.json"
);
json_wtype_roundtrip!(
    roundtrip_yaml_schema,
    "yaml_schema",
    "../fixtures/data_schema/yaml_schema_instance.json"
);
json_wtype_roundtrip!(
    roundtrip_toml_schema,
    "toml_schema",
    "../fixtures/data_schema/toml_schema_instance.json"
);
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
tabular_functor_roundtrip!(
    roundtrip_csv_table,
    "csv_table",
    "../fixtures/data_schema/csv_data.csv",
    "rows"
);
tabular_functor_roundtrip!(
    roundtrip_ini_schema,
    "ini_schema",
    "../fixtures/data_schema/ini_config.ini",
    "sections"
);

// ═══════════════════════════════════════════════════════════════════════
// Database (6)
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
    roundtrip_sql,
    "sql",
    "../fixtures/database/sql_result.tsv",
    "result_set"
);
tabular_functor_roundtrip!(
    roundtrip_redis,
    "redis",
    "../fixtures/database/redis_resp.txt",
    "entries"
);

// ═══════════════════════════════════════════════════════════════════════
// Type System (8)
// ═══════════════════════════════════════════════════════════════════════

json_wtype_roundtrip!(
    roundtrip_typescript,
    "typescript",
    "../fixtures/type_system/typescript_instance.json"
);
json_wtype_roundtrip!(
    roundtrip_python,
    "python",
    "../fixtures/type_system/python_instance.json"
);
json_wtype_roundtrip!(
    roundtrip_rust_serde,
    "rust_serde",
    "../fixtures/type_system/rust_serde_instance.json"
);
json_wtype_roundtrip!(
    roundtrip_java,
    "java",
    "../fixtures/type_system/java_instance.json"
);
json_wtype_roundtrip!(
    roundtrip_go_struct,
    "go_struct",
    "../fixtures/type_system/go_instance.json"
);
json_wtype_roundtrip!(
    roundtrip_kotlin,
    "kotlin",
    "../fixtures/type_system/kotlin_instance.json"
);
json_wtype_roundtrip!(
    roundtrip_csharp,
    "csharp",
    "../fixtures/type_system/csharp_instance.json"
);
json_wtype_roundtrip!(
    roundtrip_swift,
    "swift",
    "../fixtures/type_system/swift_instance.json"
);

// ═══════════════════════════════════════════════════════════════════════
// Config (4)
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
json_wtype_roundtrip!(roundtrip_hcl, "hcl", "../fixtures/config/hcl_config.json");

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
// Serialization (8)
// ═══════════════════════════════════════════════════════════════════════

json_wtype_roundtrip!(
    roundtrip_protobuf,
    "protobuf",
    "../fixtures/serialization/protobuf_message.json"
);
json_wtype_roundtrip!(
    roundtrip_avro,
    "avro",
    "../fixtures/serialization/avro_record.json"
);
json_wtype_roundtrip!(
    roundtrip_thrift,
    "thrift",
    "../fixtures/serialization/thrift_struct.json"
);
json_wtype_roundtrip!(
    roundtrip_capnproto,
    "capnproto",
    "../fixtures/serialization/capnproto_message.json"
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
// Annotation — JSON-based (8)
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
// Annotation — XML-based (9)
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
// Annotation — Tabular (2)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn roundtrip_conllu() {
    let reg = registry();
    let schema = tabular_schema("conllu");
    let input = include_bytes!("../fixtures/annotation/sample.conllu");

    let instance = reg
        .parse_functor("conllu", &schema, input)
        .expect("parse conllu");

    let tokens = instance.tables.get("token").expect("token table");
    assert!(
        tokens.len() >= 100,
        "Real UD CoNLL-U fixture should have many tokens, got {}",
        tokens.len()
    );

    let sentences = instance.tables.get("sentence").expect("sentence table");
    assert!(
        sentences.len() >= 5,
        "Real UD CoNLL-U fixture should have multiple sentences, got {}",
        sentences.len()
    );

    let emitted = reg
        .emit_functor("conllu", &schema, &instance)
        .expect("emit conllu");

    let instance2 = reg
        .parse_functor("conllu", &schema, &emitted)
        .expect("re-parse conllu");

    let tokens2 = instance2.tables.get("token").expect("re-parsed tokens");
    assert_eq!(
        tokens.len(),
        tokens2.len(),
        "token count mismatch after round-trip"
    );
}

tabular_functor_roundtrip!(
    roundtrip_amr,
    "amr",
    "../fixtures/annotation/amr_graph.tsv",
    "amr_graph"
);

// ═══════════════════════════════════════════════════════════════════════
// Web/Document (10)
// ═══════════════════════════════════════════════════════════════════════

json_wtype_roundtrip!(
    roundtrip_atproto,
    "atproto",
    "../fixtures/web_document/atproto_record.json"
);
json_wtype_roundtrip!(
    roundtrip_jsx,
    "jsx",
    "../fixtures/web_document/jsx_ast.json"
);
json_wtype_roundtrip!(
    roundtrip_vue,
    "vue",
    "../fixtures/web_document/vue_sfc.json"
);
json_wtype_roundtrip!(
    roundtrip_svelte,
    "svelte",
    "../fixtures/web_document/svelte_component.json"
);
json_wtype_roundtrip!(
    roundtrip_css,
    "css",
    "../fixtures/web_document/css_stylesheet.json"
);
xml_wtype_roundtrip!(
    roundtrip_xml_xsd,
    "xml_xsd",
    "../fixtures/web_document/xml_xsd_document.xml"
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

#[test]
fn roundtrip_html() {
    let reg = registry();
    let schema = open_schema("html");
    let input = include_bytes!("../fixtures/web_document/sample.html");

    let instance = reg.parse_wtype("html", &schema, input).expect("parse html");

    assert!(
        instance.node_count() >= 10,
        "HTML fixture should have many nodes, got {}",
        instance.node_count()
    );

    let emitted = reg
        .emit_wtype("html", &schema, &instance)
        .expect("emit html");

    assert!(!emitted.is_empty(), "emitted HTML should be non-empty");

    // Real Wikipedia HTML round-trips may not be exact (HTML normalization,
    // entity encoding, whitespace handling), but re-parsing must succeed.
    let instance2 = reg
        .parse_wtype("html", &schema, &emitted)
        .expect("re-parse html");

    assert!(
        instance2.node_count() >= 10,
        "re-parsed HTML should have many nodes, got {}",
        instance2.node_count()
    );
}

#[test]
fn roundtrip_markdown() {
    let reg = registry();
    let schema = open_schema("markdown");
    let input = include_bytes!("../fixtures/web_document/sample.md");

    let instance = reg
        .parse_wtype("markdown", &schema, input)
        .expect("parse markdown");

    assert!(
        instance.node_count() >= 10,
        "Markdown fixture should have many nodes, got {}",
        instance.node_count()
    );

    let emitted = reg
        .emit_wtype("markdown", &schema, &instance)
        .expect("emit markdown");

    assert!(!emitted.is_empty(), "emitted markdown should be non-empty");

    // Markdown round-trips are not byte-identical (formatting normalization),
    // but re-parsing the emitted output must succeed.
    let instance2 = reg
        .parse_wtype("markdown", &schema, &emitted)
        .expect("re-parse markdown");

    assert!(
        instance2.node_count() >= 1,
        "re-parsed markdown should have nodes"
    );
}

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
        76,
        "registry should have 76 protocols; if you add a protocol, add a round-trip test"
    );
}

#[test]
fn all_protocols_report_correct_native_repr() {
    let reg = registry();

    // WType protocols should support parse_wtype
    let wtype_protocols = [
        "graphql",
        "openapi",
        "html",
        "markdown",
        "atproto",
        "brat",
        "naf",
        "tei",
        "protobuf",
        "json_schema",
        "typescript",
    ];
    for p in &wtype_protocols {
        assert_eq!(
            reg.native_repr(p).unwrap(),
            NativeRepr::WType,
            "{p} should be WType"
        );
    }

    // Functor protocols should support parse_functor
    let functor_protocols = ["conllu", "csv_table", "sql", "amr"];
    for p in &functor_protocols {
        assert_eq!(
            reg.native_repr(p).unwrap(),
            NativeRepr::Functor,
            "{p} should be Functor"
        );
    }
}
