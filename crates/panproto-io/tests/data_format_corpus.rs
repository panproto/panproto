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

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::too_many_lines)]

use panproto_inst::value::Value;
use panproto_io::byte_tabular::ByteTabularCodec;
use panproto_io::unified_codec::UnifiedCodec;
use panproto_schema::{Protocol, SchemaBuilder};
use std::path::{Path, PathBuf};

fn generic_schema() -> panproto_schema::Schema {
    let proto = Protocol {
        name: "fixture".into(),
        schema_theory: "ThfixtureSchema".into(),
        instance_theory: "ThfixtureInstance".into(),
        ..Protocol::default()
    };
    SchemaBuilder::new(&proto)
        .vertex("root", "object", None)
        .expect("root vertex")
        .build()
        .expect("build schema")
}

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
struct DelimitedConfig {
    table: &'static str,
    delimiter: u8,
    comment_prefix: Option<u8>,
}

/// Resolve the [`ByteTabularCodec`] configuration for a fixture, keyed by the
/// family directory or extension. Returns `None` for fixtures handled by a
/// tree-sitter codec (JSON/XML/YAML/TOML/TSV).
fn delimited_config(label: &str) -> Option<DelimitedConfig> {
    // CoNLL-U: tab-delimited, `#` comment lines, blank-line boundaries.
    if label.ends_with(".conllu") {
        return Some(DelimitedConfig {
            table: "rows",
            delimiter: b'\t',
            comment_prefix: Some(b'#'),
        });
    }
    // redis (RESP key/value): space-delimited.
    if label.starts_with("redis/") || label.ends_with("redis_resp.txt") {
        return Some(DelimitedConfig {
            table: "entries",
            delimiter: b' ',
            comment_prefix: None,
        });
    }
    // SWIFT MT: colon-delimited.
    if label.starts_with("swift_mt/") || label.contains("swift_mt") {
        return Some(DelimitedConfig {
            table: "fields",
            delimiter: b':',
            comment_prefix: None,
        });
    }
    // EDI X12: asterisk-delimited segments.
    if label.starts_with("edi_x12/") || label.contains("edi_x12") {
        return Some(DelimitedConfig {
            table: "segments",
            delimiter: b'*',
            comment_prefix: None,
        });
    }
    None
}

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
    let mut covered = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for fixture in &fixtures {
        let ext = fixture
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();
        let label = fixture
            .strip_prefix(&dir)
            .unwrap_or(fixture)
            .display()
            .to_string();
        let input = std::fs::read(fixture).expect("read fixture");

        // Delimited line-oriented formats with no tree-sitter grammar.
        if let Some(cfg) = delimited_config(&label) {
            let codec =
                ByteTabularCodec::new("fixture", cfg.table, cfg.delimiter, cfg.comment_prefix);
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
        if ext == "tsv" {
            let codec = match UnifiedCodec::tsv("fixture", "amr_graph") {
                Ok(c) => c,
                Err(e) => {
                    failures.push(format!("{label}: codec init failed: {e}"));
                    continue;
                }
            };
            match codec.parse_functor_preserving(&schema, &input) {
                Ok((instance, complement)) => {
                    match codec.emit_functor_preserving(&schema, &instance, &complement) {
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

        // Byte-faithfulness is protocol-agnostic on the unmodified preserving
        // path (the preserved CST is replayed), so a generic protocol name +
        // root schema exercises the same machinery every real protocol uses.
        let codec = match ext.as_str() {
            "json" => UnifiedCodec::json("fixture"),
            "xml" => UnifiedCodec::xml("fixture"),
            "yaml" | "yml" => UnifiedCodec::yaml("fixture"),
            "toml" => UnifiedCodec::toml("fixture"),
            other => {
                failures.push(format!("{label}: unhandled extension .{other}"));
                continue;
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
    let rows = instance.tables.get_mut(table).expect("table");
    rows[0].insert(col.to_string(), Value::Str(new_value.to_string()));
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

#[test]
fn amr_edit_rewrites_one_cell() {
    // AMR rides the tree-sitter TSV preserving path; an edited cell value
    // re-emits changed while the rest of the table stays byte-identical.
    let schema = generic_schema();
    let input = b"variable\tconcept\trole\ttarget\nb\tboy\t_\t_\nw\twant-01\tARG0\tb\n";
    let codec = UnifiedCodec::tsv("amr", "amr_graph").expect("codec");
    let (mut instance, complement) = codec
        .parse_functor_preserving(&schema, input)
        .expect("parse");
    let rows = instance
        .tables
        .get_mut("amr_graph")
        .expect("amr_graph table");
    // Change the second data row's concept (want-01 -> like-01).
    rows[1].insert("concept".into(), Value::Str("like-01".into()));
    let out = codec
        .emit_functor_preserving(&schema, &instance, &complement)
        .expect("emit");
    assert_eq!(
        out,
        b"variable\tconcept\trole\ttarget\nb\tboy\t_\t_\nw\tlike-01\tARG0\tb\n"
    );
}
