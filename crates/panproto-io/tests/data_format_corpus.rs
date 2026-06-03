//! Byte-faithful round-trip over the full data-format fixture corpus.
//!
//! The project ships ~50 instance-level data-format protocols (annotation,
//! api, config, `data_schema`, `data_science`, database, domain,
//! serialization, `web_document` families). Each rides one of the
//! tree-sitter `FormatKind`
//! syntaxes (JSON/XML/YAML/TOML/CSV/TSV) via `UnifiedCodec`'s preserving
//! path, or a legacy tabular/custom codec.
//!
//! The mandate (handoff §6 "ALL data-model formats") is that every format
//! round-trips losslessly: `emit(parse(bytes)) == bytes`. The macro tests in
//! `roundtrip.rs` only assert *structural* (`node_count`) stability; this
//! file asserts the stricter byte-identity property over every fixture whose
//! format has a preserving path. Legacy tabular/custom formats
//! (`amr`/`conllu`/`redis`/`edi_x12`/`swift_mt`) do not yet have a byte-faithful
//! preserving path and are tracked separately (see `LEGACY_NO_PRESERVING`).

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

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

/// Extensions whose formats have no byte-faithful preserving path yet
/// (legacy `TabularCodec`/`ConlluCodec`). Tracked so the corpus walk does
/// not silently skip them: when these gain a preserving path, drop the
/// entry and they become covered automatically.
const LEGACY_NO_PRESERVING: &[&str] = &["tsv", "conllu", "txt"];

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

        if LEGACY_NO_PRESERVING.contains(&ext.as_str()) {
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
        let input = std::fs::read(fixture).expect("read fixture");
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
    // moves the fixtures dir): we expect the JSON/XML/YAML/TOML majority.
    assert!(
        covered >= 40,
        "expected the preserving-path fixture corpus to be large; only {covered} covered"
    );
}
