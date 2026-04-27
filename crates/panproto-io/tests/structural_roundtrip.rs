//! Structural-equality round-trip tests across every protocol fixture.
//!
//! The macro-generated `roundtrip.rs` tests check `node_count` parity
//! after a parse → emit → re-parse cycle. That misses bugs where the
//! number of nodes survives but the *shape* corrupts (singleton
//! arrays collapsing to `{"item": x}` objects, empty arrays becoming
//! `{}`, TSV columns folding under an empty-string key, etc.).
//!
//! Each test below runs a single fixture through the registry and
//! asserts that the emitted bytes parse back to a `serde_json::Value`
//! / TSV row table that is *structurally equal* to the original
//! input, modulo:
//!
//! - JSON object key order (objects are unordered by spec).
//! - Whitespace inside JSON.
//! - Numeric literal normalisation (`1.0` and `1` compare equal).
//!
//! When a fixture fails, the assertion message points at the first
//! diverging path so the underlying parse-or-emit bug is easy to
//! locate.
//!
//! XML structural round-trip is covered separately in the existing
//! `roundtrip.rs` macro suite; quick-xml's `Reader` event stream is
//! the right comparison primitive there but is not yet exposed by
//! the registry helpers.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use panproto_io::ProtocolRegistry;
use panproto_io::traits::NativeRepr;
use panproto_schema::{Protocol, Schema, SchemaBuilder};

fn open_schema(name: &str) -> Schema {
    let proto = Protocol {
        name: name.into(),
        schema_theory: format!("Th{name}Schema"),
        instance_theory: format!("Th{name}Instance"),
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

fn registry() -> ProtocolRegistry {
    panproto_io::default_registry()
}

/// Recursively normalise a `serde_json::Value` for order-independent
/// comparison: object keys are sorted and integer-valued numbers
/// collapse to a single integer representation.
fn normalise(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), normalise(&map[k]));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(normalise).collect())
        }
        serde_json::Value::Number(n) => n.as_f64().map_or_else(
            || serde_json::Value::Number(n.clone()),
            |f| {
                if f.fract() == 0.0 && f.is_finite() && f.abs() < 1e15 {
                    #[allow(clippy::cast_possible_truncation)]
                    let as_i = f as i64;
                    serde_json::json!(as_i)
                } else {
                    serde_json::json!(f)
                }
            },
        ),
        other => other.clone(),
    }
}

fn assert_json_round_trip(protocol: &str, input: &[u8]) {
    let reg = registry();
    let schema = open_schema(protocol);
    let inst = reg
        .parse_wtype(protocol, &schema, input)
        .unwrap_or_else(|e| panic!("{protocol}: parse failed: {e}"));
    let emitted = reg
        .emit_wtype(protocol, &schema, &inst)
        .unwrap_or_else(|e| panic!("{protocol}: emit failed: {e}"));
    let original: serde_json::Value = serde_json::from_slice(input)
        .unwrap_or_else(|e| panic!("{protocol}: fixture is not valid JSON: {e}"));
    let recovered: serde_json::Value = serde_json::from_slice(&emitted).unwrap_or_else(|e| {
        let preview = std::str::from_utf8(&emitted).unwrap_or("<non-utf8>");
        let preview = if preview.len() > 600 {
            format!("{}...", &preview[..600])
        } else {
            preview.to_owned()
        };
        panic!("{protocol}: emitted bytes do not parse as JSON: {e}\nemitted:\n{preview}")
    });
    let n_orig = normalise(&original);
    let n_recovered = normalise(&recovered);
    if n_orig != n_recovered {
        let orig_pp = serde_json::to_string_pretty(&n_orig).unwrap();
        let rec_pp = serde_json::to_string_pretty(&n_recovered).unwrap();
        panic!(
            "{protocol}: structural divergence after round-trip\n\
             ── original ──\n{orig_pp}\n── recovered ──\n{rec_pp}",
        );
    }
}

macro_rules! json_structural {
    ($name:ident, $protocol:expr, $fixture:expr) => {
        #[test]
        fn $name() {
            let input: &[u8] = include_bytes!($fixture);
            assert_json_round_trip($protocol, input);
        }
    };
}

// API
json_structural!(
    struct_openapi,
    "openapi",
    "../fixtures/api/openapi_response.json"
);
json_structural!(
    struct_asyncapi,
    "asyncapi",
    "../fixtures/api/asyncapi_event.json"
);
json_structural!(
    struct_jsonapi,
    "jsonapi",
    "../fixtures/api/jsonapi_response.json"
);
json_structural!(struct_raml, "raml", "../fixtures/api/raml_response.json");

// Data Schema
json_structural!(
    struct_cddl,
    "cddl",
    "../fixtures/data_schema/cddl_instance.json"
);
json_structural!(
    struct_bson,
    "bson",
    "../fixtures/data_schema/bson_instance.json"
);

// Database
json_structural!(
    struct_mongodb,
    "mongodb",
    "../fixtures/database/mongodb_document.json"
);
json_structural!(
    struct_dynamodb,
    "dynamodb",
    "../fixtures/database/dynamodb_item.json"
);
json_structural!(
    struct_cassandra,
    "cassandra",
    "../fixtures/database/cassandra_rows.json"
);
json_structural!(
    struct_neo4j,
    "neo4j",
    "../fixtures/database/neo4j_result.json"
);

// Config
json_structural!(
    struct_cloudformation,
    "cloudformation",
    "../fixtures/config/cloudformation_template.json"
);
json_structural!(
    struct_ansible,
    "ansible",
    "../fixtures/config/ansible_playbook.json"
);
json_structural!(struct_k8s, "k8s_crd", "../fixtures/config/k8s_crd.json");

// Data Science
json_structural!(
    struct_dataframe,
    "dataframe",
    "../fixtures/data_science/dataframe_instance.json"
);
json_structural!(
    struct_parquet,
    "parquet",
    "../fixtures/data_science/parquet_record.json"
);
json_structural!(
    struct_arrow,
    "arrow",
    "../fixtures/data_science/arrow_batch.json"
);

// Domain
json_structural!(
    struct_geojson,
    "geojson",
    "../fixtures/domain/geojson_features.json"
);
json_structural!(struct_fhir, "fhir", "../fixtures/domain/fhir_patient.json");
json_structural!(
    struct_vcard,
    "vcard_ical",
    "../fixtures/domain/vcard_contact.json"
);

// Serialization
json_structural!(
    struct_avro,
    "avro",
    "../fixtures/serialization/avro_record.json"
);
json_structural!(
    struct_flatbuffers,
    "flatbuffers",
    "../fixtures/serialization/flatbuffers_table.json"
);
json_structural!(
    struct_asn1,
    "asn1",
    "../fixtures/serialization/asn1_cert.json"
);
json_structural!(
    struct_bond,
    "bond",
    "../fixtures/serialization/bond_struct.json"
);
json_structural!(
    struct_msgpack,
    "msgpack_schema",
    "../fixtures/serialization/msgpack_data.json"
);

// Web Document
json_structural!(
    struct_atproto,
    "atproto",
    "../fixtures/web_document/atproto_record.json"
);

// Annotation (JSON-flavoured)
json_structural!(
    struct_brat,
    "brat",
    "../fixtures/annotation/brat_annotation.json"
);
json_structural!(
    struct_decomp,
    "decomp",
    "../fixtures/annotation/decomp_annotation.json"
);
json_structural!(
    struct_ucca,
    "ucca",
    "../fixtures/annotation/ucca_passage.json"
);
json_structural!(
    struct_fovea,
    "fovea",
    "../fixtures/annotation/fovea_annotation.json"
);
json_structural!(
    struct_bead,
    "bead",
    "../fixtures/annotation/bead_experiment.json"
);
json_structural!(
    struct_web_annotation,
    "web_annotation",
    "../fixtures/annotation/web_annotation.json"
);
json_structural!(
    struct_concrete,
    "concrete",
    "../fixtures/annotation/concrete_comm.json"
);
json_structural!(
    struct_nif,
    "nif",
    "../fixtures/annotation/nif_document.json"
);

// ── Tabular structural round-trip ──────────────────────────────────────

fn assert_tabular_round_trip(protocol: &str, input: &[u8], table: &str) {
    let reg = registry();
    if reg.native_repr(protocol).expect("known protocol") != NativeRepr::Functor {
        return;
    }
    let proto = Protocol {
        name: protocol.into(),
        schema_theory: format!("Th{protocol}Schema"),
        instance_theory: format!("Th{protocol}Instance"),
        edge_rules: vec![],
        obj_kinds: vec![],
        constraint_sorts: vec![],
        ..Protocol::default()
    };
    let schema = SchemaBuilder::new(&proto)
        .vertex("table", "table", None)
        .expect("table vertex")
        .build()
        .expect("build schema");
    let inst = reg
        .parse_functor(protocol, &schema, input)
        .unwrap_or_else(|e| panic!("{protocol}: parse_functor failed: {e}"));
    let emitted = reg
        .emit_functor(protocol, &schema, &inst)
        .unwrap_or_else(|e| panic!("{protocol}: emit_functor failed: {e}"));
    let inst2 = reg
        .parse_functor(protocol, &schema, &emitted)
        .unwrap_or_else(|e| panic!("{protocol}: re-parse_functor failed: {e}"));
    let rows1 = inst.tables.get(table).expect("table missing in original");
    let rows2 = inst2
        .tables
        .get(table)
        .expect("table missing after round-trip");
    assert_eq!(rows1.len(), rows2.len(), "{protocol}: row count diverged");
    for (i, (a, b)) in rows1.iter().zip(rows2.iter()).enumerate() {
        assert_eq!(a, b, "{protocol}: row {i} content diverged");
    }
}

#[test]
fn struct_amr_tsv() {
    assert_tabular_round_trip(
        "amr",
        include_bytes!("../fixtures/annotation/amr_graph.tsv"),
        "amr_graph",
    );
}

// ── XML structural round-trip ──────────────────────────────────────────
//
// XML round-trip cannot use byte equality because attribute order is
// not significant per spec, and quick-xml does not emit empty-element
// `<foo/>` shorthand by default. Instead we canonicalise both sides:
// each element becomes `(tag, sorted_attrs, children)` recursively,
// whitespace-only text nodes are dropped, and resulting trees compare
// for equality.

#[derive(Debug, PartialEq, Eq)]
enum XmlNode {
    Element {
        tag: String,
        attrs: Vec<(String, String)>,
        children: Vec<Self>,
    },
    Text(String),
}

fn collect_attrs(e: &quick_xml::events::BytesStart<'_>) -> Vec<(String, String)> {
    e.attributes()
        .filter_map(Result::ok)
        .map(|a| {
            let k = std::str::from_utf8(a.key.as_ref()).unwrap().to_owned();
            // Decode entity escapes inside attribute values so
            // `&apos;` and `'` compare equal across the round-trip.
            let raw = std::str::from_utf8(&a.value).unwrap_or_default();
            let decoded = quick_xml::escape::unescape(raw)
                .map_or_else(|_| raw.to_owned(), |c| c.into_owned());
            (k, decoded)
        })
        .collect()
}

fn parse_xml_canonical(input: &[u8]) -> XmlNode {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_reader(input);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<(String, Vec<(String, String)>, Vec<XmlNode>)> =
        vec![("__doc__".into(), Vec::new(), Vec::new())];
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap().to_owned();
                let mut attrs = collect_attrs(&e);
                attrs.sort();
                stack.push((tag, attrs, Vec::new()));
            }
            Ok(Event::Empty(e)) => {
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap().to_owned();
                let mut attrs = collect_attrs(&e);
                attrs.sort();
                let node = XmlNode::Element {
                    tag,
                    attrs,
                    children: Vec::new(),
                };
                stack.last_mut().unwrap().2.push(node);
            }
            Ok(Event::End(_)) => {
                let (tag, attrs, children) = stack.pop().unwrap();
                let node = XmlNode::Element {
                    tag,
                    attrs,
                    children,
                };
                stack.last_mut().unwrap().2.push(node);
            }
            Ok(Event::Text(t)) => {
                // Use unescape so that `&apos;` and `'` compare equal
                // (quick-xml's writer emits `&apos;` when serialising
                // a literal apostrophe). Trimming kills whitespace
                // between elements.
                let decoded = t.unescape().expect("XML text decode");
                let s = decoded.trim().to_owned();
                if !s.is_empty() {
                    stack.last_mut().unwrap().2.push(XmlNode::Text(s));
                }
            }
            Ok(_) => {} // ignore CDATA, comments, declarations, etc.
            Err(e) => panic!("XML parse error: {e}"),
        }
        buf.clear();
    }
    let (_, _, mut roots) = stack.pop().unwrap();
    if roots.len() == 1 {
        roots.pop().unwrap()
    } else {
        XmlNode::Element {
            tag: "__doc__".into(),
            attrs: Vec::new(),
            children: roots,
        }
    }
}

fn assert_xml_round_trip(protocol: &str, input: &[u8]) {
    let reg = registry();
    let schema = open_schema(protocol);
    let inst = reg
        .parse_wtype(protocol, &schema, input)
        .unwrap_or_else(|e| panic!("{protocol}: parse failed: {e}"));
    let emitted = reg
        .emit_wtype(protocol, &schema, &inst)
        .unwrap_or_else(|e| panic!("{protocol}: emit failed: {e}"));
    let orig = parse_xml_canonical(input);
    let recv = parse_xml_canonical(&emitted);
    if orig != recv {
        let preview = std::str::from_utf8(&emitted).unwrap_or("<non-utf8>");
        let preview = if preview.len() > 1200 {
            format!("{}...", &preview[..1200])
        } else {
            preview.to_owned()
        };
        panic!(
            "{protocol}: XML structural divergence after round-trip\n\
             ── original (canonical) ──\n{orig:#?}\n\
             ── recovered (canonical) ──\n{recv:#?}\n\
             ── emitted bytes ──\n{preview}"
        );
    }
}

macro_rules! xml_structural {
    ($name:ident, $protocol:expr, $fixture:expr) => {
        #[test]
        fn $name() {
            let input: &[u8] = include_bytes!($fixture);
            assert_xml_round_trip($protocol, input);
        }
    };
}

// XML annotation
xml_structural!(struct_naf, "naf", "../fixtures/annotation/naf_document.xml");
xml_structural!(struct_uima, "uima", "../fixtures/annotation/uima_cas.xml");
xml_structural!(
    struct_folia,
    "folia",
    "../fixtures/annotation/folia_document.xml"
);
xml_structural!(struct_tei, "tei", "../fixtures/annotation/tei_document.xml");
xml_structural!(
    struct_timeml,
    "timeml",
    "../fixtures/annotation/timeml_document.xml"
);
xml_structural!(
    struct_elan,
    "elan",
    "../fixtures/annotation/elan_annotation.xml"
);
xml_structural!(
    struct_iso_space,
    "iso_space",
    "../fixtures/annotation/iso_space_document.xml"
);
xml_structural!(
    struct_paula,
    "paula",
    "../fixtures/annotation/paula_annotation.xml"
);
xml_structural!(
    struct_laf_graf,
    "laf_graf",
    "../fixtures/annotation/laf_graf_annotation.xml"
);

// XML domain / web document
xml_structural!(
    struct_rss_atom,
    "rss_atom",
    "../fixtures/domain/rss_feed.xml"
);
xml_structural!(
    struct_docx,
    "docx",
    "../fixtures/web_document/docx_content.xml"
);
xml_structural!(
    struct_odf,
    "odf",
    "../fixtures/web_document/odf_content.xml"
);
