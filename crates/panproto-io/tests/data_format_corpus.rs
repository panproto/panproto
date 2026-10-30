//! Byte-faithful round-trip over the full data-format fixture corpus.
//!
//! The project ships ~50 instance-level data-format protocols (annotation,
//! api, config, `data_schema`, `data_science`, database, domain,
//! serialization, `web_document` families). Each rides one of the
//! tree-sitter `FormatKind`
//! syntaxes (JSON/XML/YAML/TOML/CSV/TSV) via `UnifiedCodec`'s preserving
//! path, or a delimited line-oriented codec.
//!
//! The mandate (handoff §6 "ALL data-model formats") is that every format
//! round-trips losslessly: `emit(parse(bytes)) == bytes`. The macro tests in
//! `roundtrip.rs` only assert *structural* (`node_count`) stability; this
//! file asserts the stricter byte-identity property over every fixture.
//!
//! The five formats that used to lack a byte-faithful path
//! (`amr`/`conllu`/`redis`/`edi_x12`/`swift_mt`) now route through:
//!
//! - `amr` (TSV): `UnifiedCodec::parse_functor_preserving` /
//!   `emit_functor_preserving` (the tree-sitter CST-complement path, the same
//!   one CSV/TSV use).
//! - `conllu`/`redis`/`swift_mt`/`edi_x12` (own delimited line syntaxes, no
//!   tree-sitter grammar): [`ByteTabularCodec`], which records the exact
//!   original layout as a complement and replays it, splicing only changed
//!   cell values.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::too_many_lines)]

use panproto_inst::value::Value;
use panproto_io::byte_tabular::ByteTabularCodec;
#[cfg(feature = "tree-sitter")]
use panproto_io::unified_codec::UnifiedCodec;
#[cfg(feature = "tree-sitter")]
use panproto_schema::{Protocol, SchemaBuilder};
#[cfg(feature = "tree-sitter")]
use std::path::{Path, PathBuf};

#[cfg(feature = "tree-sitter")]
fn generic_schema() -> panproto_schema::Schema {
    let proto = Protocol {
        name: "fixture".into(),
        schema_theory: "ThfixtureSchema".into(),
        instance_theory: "ThfixtureInstance".into(),
        ..Default::default()
    };
    SchemaBuilder::new(&proto)
        .vertex("root", "object", None)
        .expect("root vertex")
        .build()
        .expect("build schema")
}

/// The generic schema with the `amr_graph` table vertex that the AMR TSV
/// codec is configured to read rows into: a codec whose configured table is
/// not a vertex of the schema refuses to anchor rows outside it.
#[cfg(feature = "tree-sitter")]
fn amr_schema() -> panproto_schema::Schema {
    let proto = Protocol {
        name: "fixture".into(),
        schema_theory: "ThfixtureSchema".into(),
        instance_theory: "ThfixtureInstance".into(),
        ..Default::default()
    };
    SchemaBuilder::new(&proto)
        .vertex("root", "object", None)
        .expect("root vertex")
        .vertex("amr_graph", "table", None)
        .expect("amr_graph vertex")
        .build()
        .expect("build schema")
}

#[cfg(feature = "tree-sitter")]
fn collect_fixtures(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read fixtures dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_fixtures(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Configuration for a delimited line-oriented (non-tree-sitter) format.
#[cfg(feature = "tree-sitter")]
struct DelimitedConfig {
    table: &'static str,
    delimiter: u8,
    comment_prefix: Option<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FixtureRoute {
    Json,
    Xml,
    Yaml,
    AmrTsv,
    Delimited,
}

/// Select a syntax from the protocol declaration, never from the fixture's
/// suffix. In particular, JSON-shaped fixtures for YAML-native protocols
/// remain YAML inputs because JSON is a YAML subset.
fn fixture_route(protocol: &str) -> FixtureRoute {
    match protocol {
        "asyncapi" | "raml" | "ansible" | "k8s-crd" | "cloudformation" => FixtureRoute::Yaml,
        "elan" | "folia" | "iso-space" | "laf-graf" | "naf" | "paula" | "tei" | "timeml"
        | "uima" | "rss-atom" | "docx" | "odf" => FixtureRoute::Xml,
        "amr" => FixtureRoute::AmrTsv,
        "conllu" | "redis" | "swift-mt" | "edi-x12" => FixtureRoute::Delimited,
        _ => FixtureRoute::Json,
    }
}

/// Name the protocol whose fixture `label` exercises.
///
/// This table is intentionally explicit. A suffix such as `.json` does not
/// determine the codec: JSON is valid YAML, and the YAML-native protocols
/// have both JSON-shaped and block-YAML fixtures. Routing by suffix would
/// therefore test the old JSON registration rather than the protocol route.
fn fixture_protocol(label: &str) -> Option<&'static str> {
    let protocol = match label {
        path if path.starts_with("amr/") || path == "annotation/amr_graph.tsv" => "amr",
        path if path.starts_with("conllu/") || path == "annotation/sample.conllu" => "conllu",
        path if path.starts_with("redis/") || path == "database/redis_resp.txt" => "redis",
        path if path.starts_with("edi_x12/") || path == "domain/edi_x12_850.txt" => "edi-x12",
        path if path.starts_with("swift_mt/") || path == "domain/swift_mt103.txt" => "swift-mt",
        "annotation/bead_experiment.json" => "bead",
        "annotation/brat_annotation.json" => "brat",
        "annotation/concrete_comm.json" => "concrete",
        "annotation/decomp_annotation.json" => "decomp",
        "annotation/elan_annotation.xml" => "elan",
        "annotation/folia_document.xml" => "folia",
        "annotation/fovea_annotation.json" => "fovea",
        "annotation/iso_space_document.xml" => "iso-space",
        "annotation/laf_graf_annotation.xml" => "laf-graf",
        "annotation/naf_document.xml" => "naf",
        "annotation/nif_document.json" => "nif",
        "annotation/paula_annotation.xml" => "paula",
        "annotation/tei_document.xml" => "tei",
        "annotation/timeml_document.xml" => "timeml",
        "annotation/ucca_passage.json" => "ucca",
        "annotation/uima_cas.xml" => "uima",
        "annotation/web_annotation.json" => "web-annotation",
        path if path.starts_with("api/asyncapi_") => "asyncapi",
        path if path.starts_with("api/jsonapi_") => "jsonapi",
        path if path.starts_with("api/openapi_") => "openapi",
        path if path.starts_with("api/raml_") => "raml",
        path if path.starts_with("config/ansible_") => "ansible",
        path if path.starts_with("config/cloudformation_") => "cloudformation",
        path if path.starts_with("config/k8s_") => "k8s-crd",
        "data_schema/bson_instance.json" => "bson",
        "data_schema/cddl_instance.json" => "cddl",
        "data_science/arrow_batch.json" => "arrow",
        "data_science/dataframe_instance.json" => "dataframe",
        "data_science/parquet_record.json" => "parquet",
        "database/cassandra_rows.json" => "cassandra",
        "database/dynamodb_item.json" => "dynamodb",
        "database/mongodb_document.json" => "mongodb",
        "database/neo4j_result.json" => "neo4j",
        "domain/atom_feed.xml" | "domain/rss_feed.xml" | "domain/rss_namespaced.xml" => "rss-atom",
        path if path.starts_with("domain/fhir_") => "fhir",
        path if path.starts_with("domain/geojson_") => "geojson",
        "domain/ical_event.json" | "domain/vcard_contact.json" | "domain/vcard_minimal.json" => {
            "vcard-ical"
        }
        "serialization/asn1_cert.json" => "asn1",
        "serialization/avro_record.json" => "avro",
        "serialization/bond_struct.json" => "bond",
        "serialization/flatbuffers_table.json" => "flatbuffers",
        "serialization/msgpack_data.json" => "msgpack-schema",
        path if path.starts_with("web_document/atproto_") => "atproto",
        path if path.starts_with("web_document/docx_") => "docx",
        path if path.starts_with("web_document/odf_") => "odf",
        _ => return None,
    };
    Some(protocol)
}

#[test]
fn fixture_routes_are_selected_by_protocol_not_suffix() {
    for label in [
        "api/asyncapi_event.json",
        "api/raml_response.json",
        "config/ansible_playbook.json",
        "config/cloudformation_template.json",
        "config/k8s_crd.json",
    ] {
        let protocol = fixture_protocol(label).expect("declared protocol route");
        assert_eq!(
            fixture_route(protocol),
            FixtureRoute::Yaml,
            "{label} must not select JSON from its suffix"
        );
    }
    assert_eq!(fixture_route("naf"), FixtureRoute::Xml);
    assert_eq!(fixture_route("brat"), FixtureRoute::Json);
}

/// Resolve a non-tree-sitter codec from the protocol declaration, never from
/// the fixture's suffix.
#[cfg(feature = "tree-sitter")]
fn delimited_config(protocol: &str) -> Option<DelimitedConfig> {
    match protocol {
        "conllu" => Some(DelimitedConfig {
            table: "rows",
            delimiter: b'\t',
            comment_prefix: Some(b'#'),
        }),
        "redis" => Some(DelimitedConfig {
            table: "entries",
            delimiter: b' ',
            comment_prefix: None,
        }),
        "swift-mt" => Some(DelimitedConfig {
            table: "fields",
            delimiter: b':',
            comment_prefix: None,
        }),
        "edi-x12" => Some(DelimitedConfig {
            table: "segments",
            delimiter: b'*',
            comment_prefix: None,
        }),
        _ => None,
    }
}

#[cfg(feature = "tree-sitter")]
#[test]
fn fixture_corpus_is_byte_faithful() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let mut fixtures = Vec::new();
    collect_fixtures(&dir, &mut fixtures);
    fixtures.sort();
    assert!(
        !fixtures.is_empty(),
        "no fixtures found under {}",
        dir.display()
    );

    let schema = generic_schema();
    let amr_schema = amr_schema();
    let mut covered = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for fixture in &fixtures {
        let label = fixture
            .strip_prefix(&dir)
            .unwrap_or(fixture)
            .display()
            .to_string();
        let input = std::fs::read(fixture).expect("read fixture");
        let Some(protocol) = fixture_protocol(&label) else {
            failures.push(format!("{label}: fixture has no declared protocol route"));
            continue;
        };

        // Delimited line-oriented formats with no tree-sitter grammar.
        if let Some(cfg) = delimited_config(protocol) {
            let codec =
                ByteTabularCodec::new(protocol, cfg.table, cfg.delimiter, cfg.comment_prefix);
            match codec.parse(&input) {
                Ok((instance, complement)) => match codec.emit(&instance, &complement) {
                    Ok(out) if out == input => covered += 1,
                    Ok(out) => failures.push(format!(
                        "{label}: byte mismatch ({} in / {} out)",
                        input.len(),
                        out.len()
                    )),
                    Err(e) => failures.push(format!("{label}: emit failed: {e}")),
                },
                Err(e) => failures.push(format!("{label}: parse failed: {e}")),
            }
            continue;
        }

        // AMR (TSV) goes through the tree-sitter CST-complement preserving path.
        if fixture_route(protocol) == FixtureRoute::AmrTsv {
            let codec = match UnifiedCodec::tsv(protocol, "amr_graph") {
                Ok(c) => c,
                Err(e) => {
                    failures.push(format!("{label}: codec init failed: {e}"));
                    continue;
                }
            };
            match codec.parse_functor_preserving(&amr_schema, &input) {
                Ok((instance, complement)) => {
                    match codec.emit_functor_preserving(&amr_schema, &instance, &complement) {
                        Ok(out) if out == input => covered += 1,
                        Ok(out) => failures.push(format!(
                            "{label}: byte mismatch ({} in / {} out)",
                            input.len(),
                            out.len()
                        )),
                        Err(e) => failures.push(format!("{label}: emit failed: {e}")),
                    }
                }
                Err(e) => failures.push(format!("{label}: parse failed: {e}")),
            }
            continue;
        }

        // Select the syntax from the protocol route. JSON-shaped fixtures of
        // YAML-native protocols deliberately enter the YAML parser because
        // JSON is a YAML subset; this exercises the declared YAML parser
        // semantics rather than treating the fixture suffix as authoritative.
        let codec = match fixture_route(protocol) {
            FixtureRoute::Yaml => UnifiedCodec::yaml(protocol),
            FixtureRoute::Xml => UnifiedCodec::xml(protocol),
            FixtureRoute::Json => UnifiedCodec::json(protocol),
            FixtureRoute::AmrTsv | FixtureRoute::Delimited => {
                unreachable!("non-tree route handled above")
            }
        };
        let codec = match codec {
            Ok(c) => c,
            Err(e) => {
                failures.push(format!("{label}: codec init failed: {e}"));
                continue;
            }
        };
        match codec.parse_wtype_preserving(&schema, &input) {
            Ok((instance, complement)) => {
                match codec.emit_wtype_preserving(&schema, &instance, &complement) {
                    Ok(emitted) if emitted == input => covered += 1,
                    Ok(emitted) => failures.push(format!(
                        "{label}: byte mismatch ({} in / {} out)",
                        input.len(),
                        emitted.len()
                    )),
                    Err(e) => failures.push(format!("{label}: emit failed: {e}")),
                }
            }
            Err(e) => failures.push(format!("{label}: parse failed: {e}")),
        }
    }

    assert!(
        failures.is_empty(),
        "{} fixture(s) failed byte-faithful round-trip:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // Guard against the walk silently covering nothing (e.g. a refactor that
    // moves the fixtures dir): we expect the JSON/XML/YAML/TOML majority plus
    // the five newly byte-faithful delimited formats.
    assert!(
        covered >= 55,
        "expected the preserving-path fixture corpus to be large; only {covered} covered"
    );
}

// ════════════════════════════════════════════════════════════════════════
// Edit tests: a changed cell value must re-emit changed, while every other
// byte stays identical. This proves the splice path (not just verbatim
// replay) is wired for each delimited format.
// ════════════════════════════════════════════════════════════════════════

fn edit_first_data_cell(
    codec: &ByteTabularCodec,
    table: &str,
    input: &[u8],
    col: &str,
    new_value: &str,
) -> Vec<u8> {
    let (mut instance, complement) = codec.parse(input).expect("parse");
    let first = instance.table(table).expect("table").ids()[0];
    instance
        .row_mut(first)
        .expect("the first row")
        .insert(col.to_string(), Value::Str(new_value.to_string()));
    codec.emit(&instance, &complement).expect("emit")
}

#[test]
fn redis_edit_rewrites_one_cell() {
    let input = b"key user:1001\nname Alice Chen\nscore 94.5\n";
    let codec = ByteTabularCodec::new("redis", "entries", b' ', None);
    let out = edit_first_data_cell(&codec, "entries", input, "col_1", "user:2002");
    assert_eq!(out, b"key user:2002\nname Alice Chen\nscore 94.5\n");
}

#[test]
fn swift_mt_edit_rewrites_one_cell() {
    let input = b"tag:20:Transaction Reference\nvalue:FT2603130001\n";
    let codec = ByteTabularCodec::new("swift_mt", "fields", b':', None);
    let out = edit_first_data_cell(&codec, "fields", input, "col_1", "23B");
    assert_eq!(out, b"tag:23B:Transaction Reference\nvalue:FT2603130001\n");
}

#[test]
fn edi_x12_edit_rewrites_one_cell() {
    let input = b"ST*850*0001\nBEG*00*NE*PO-12345**20260313\n";
    let codec = ByteTabularCodec::new("edi_x12", "segments", b'*', None);
    let out = edit_first_data_cell(&codec, "segments", input, "col_2", "0099");
    assert_eq!(out, b"ST*850*0099\nBEG*00*NE*PO-12345**20260313\n");
}

#[test]
fn conllu_edit_rewrites_one_cell() {
    // Comment lines are preserved verbatim and excluded from the data rows, so
    // the first data row is token line 1; col_3 is the UPOS column.
    let input =
        b"# sent_id = s1\n1\tHello\thello\tINTJ\tUH\t_\t0\troot\t_\t_\n2\tworld\tworld\tNOUN\tNN\t_\t1\tvocative\t_\t_\n\n";
    let codec = ByteTabularCodec::new("conllu", "rows", b'\t', Some(b'#'));
    let out = edit_first_data_cell(&codec, "rows", input, "col_3", "PROPN");
    assert_eq!(
        out,
        &b"# sent_id = s1\n1\tHello\thello\tPROPN\tUH\t_\t0\troot\t_\t_\n2\tworld\tworld\tNOUN\tNN\t_\t1\tvocative\t_\t_\n\n"[..]
    );
}

#[cfg(feature = "tree-sitter")]
#[test]
fn xml_mixed_content_round_trips_byte_faithfully() {
    // The four mixed-content XML fixtures interleave CDATA sections,
    // comments, processing instructions, and entity references with text and
    // elements. They exercise the structural extraction path
    // (`emit_mixed_content`) and the single-`CharData`-leaf splice guard:
    // entity-split text runs (`Aaron &amp; Blaine`) must replay verbatim, not
    // collapse the entity into a dropped fragment.
    let schema = generic_schema();
    for name in [
        "domain/atom_feed.xml",
        "domain/rss_namespaced.xml",
        "web_document/docx_mixed.xml",
        "web_document/odf_styled.xml",
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(name);
        let input = std::fs::read(&path).expect("read fixture");
        let codec = UnifiedCodec::xml("fixture").expect("codec");
        let (instance, complement) = codec
            .parse_wtype_preserving(&schema, &input)
            .expect("parse");
        let out = codec
            .emit_wtype_preserving(&schema, &instance, &complement)
            .expect("emit");
        assert_eq!(
            out, input,
            "{name}: mixed-content XML must round-trip byte-faithfully"
        );
    }
}

#[cfg(feature = "tree-sitter")]
#[test]
fn xml_entity_text_canonical_emit_is_node_count_idempotent() {
    // The coupled property: a text run containing an entity reference
    // (`&amp;`/`&apos;`) parses as `CharData EntityRef CharData`. The
    // canonical (non-complement) emit re-escapes the value, so the re-parse
    // re-splits the run; the entity-merging extractor folds both shapes to the
    // same node count, keeping `emit_wtype`/`parse_wtype` idempotent (the
    // property `roundtrip_rss_atom` also pins). Comments and PIs are not
    // covered here: their position and bytes live only in the layout
    // complement. CDATA, by contrast, is character data and is exposed as
    // text by the CST extractor. This test isolates the entity case (no opaque
    // siblings) to lock the merge fix.
    use panproto_io::traits::{InstanceEmitter, InstanceParser};
    let schema = generic_schema();
    let codec = UnifiedCodec::xml("fixture").expect("codec");
    // Entity-only mixed content: a leaf run and an element-interleaved run,
    // each carrying an `&amp;`/`&apos;`.
    let input = b"<doc><name>Aaron &amp; Blaine</name><p>the team&apos;s <em>first</em> release &amp; more</p></doc>";
    let inst = codec.parse_wtype(&schema, input).expect("parse");
    let emitted = codec.emit_wtype(&schema, &inst).expect("emit");
    let inst2 = codec.parse_wtype(&schema, &emitted).expect("reparse");
    assert_eq!(
        inst.node_count(),
        inst2.node_count(),
        "entity-split text must not inflate node count across a canonical round-trip"
    );
    // And a second canonical round-trip must be a fixed point.
    let emitted2 = codec.emit_wtype(&schema, &inst2).expect("emit2");
    let inst3 = codec.parse_wtype(&schema, &emitted2).expect("reparse2");
    assert_eq!(inst2.node_count(), inst3.node_count(), "second round-trip");
}

#[cfg(feature = "tree-sitter")]
#[test]
fn amr_edit_rewrites_one_cell() {
    // AMR rides the tree-sitter TSV preserving path; an edited cell value
    // re-emits changed while the rest of the table stays byte-identical.
    let schema = amr_schema();
    let input = b"variable\tconcept\trole\ttarget\nb\tboy\t_\t_\nw\twant-01\tARG0\tb\n";
    let codec = UnifiedCodec::tsv("amr", "amr_graph").expect("codec");
    let (mut instance, complement) = codec
        .parse_functor_preserving(&schema, input)
        .expect("parse");
    let second = instance.table("amr_graph").expect("amr_graph table").ids()[1];
    // Change the second data row's concept (want-01 -> like-01).
    instance
        .row_mut(second)
        .expect("the second row")
        .insert("concept".into(), Value::Str("like-01".into()));
    let out = codec
        .emit_functor_preserving(&schema, &instance, &complement)
        .expect("emit");
    assert_eq!(
        out,
        b"variable\tconcept\trole\ttarget\nb\tboy\t_\t_\nw\tlike-01\tARG0\tb\n"
    );
}
