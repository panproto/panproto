//! The edge-list routes against the vendored conformance set.
//!
//! No standard suite exists for edge lists, so the set under
//! `fixtures/conformance/edgelist` is built from the rules the edge-lists
//! page cites, each case's rule and source recorded in its `MANIFEST`. A
//! positive case passes when the route of its extension's dialect reads
//! it, writes the canonical text its `.expected` file holds, and reads that
//! text back as the same edges; a negative case passes when the route
//! refuses it with the refusal the manifest names.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use panproto_io::edgelist::{Dialect, EdgeListCodec, EdgeListReadError, write};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance/edgelist")
}

/// The refusal kind's name, as the manifest writes it.
fn refusal_name(error: &EdgeListReadError) -> &'static str {
    match error {
        EdgeListReadError::NotUtf8 { .. } => "NotUtf8",
        EdgeListReadError::Table { .. } => "Table",
        EdgeListReadError::NoHeader => "NoHeader",
        EdgeListReadError::UnsupportedColumn { .. } => "UnsupportedColumn",
        EdgeListReadError::DuplicateColumn { .. } => "DuplicateColumn",
        EdgeListReadError::MissingColumn { .. } => "MissingColumn",
        EdgeListReadError::EmptyName { .. } => "EmptyName",
        other => panic!("unnamed refusal {other:?}"),
    }
}

#[test]
fn every_edge_list_conformance_case_reads_writes_or_refuses_as_its_manifest_states() {
    let manifest = std::fs::read_to_string(corpus().join("MANIFEST")).unwrap();
    let csv = EdgeListCodec::new(Dialect::Csv).unwrap();
    let tsv = EdgeListCodec::new(Dialect::Tsv).unwrap();
    let (mut positive, mut negative) = (0, 0);
    for line in manifest.lines().filter(|line| !line.starts_with('#')) {
        let mut parts = line.split('\t');
        let (name, outcome) = (parts.next().unwrap(), parts.next().unwrap());
        assert!(parts.next().is_some_and(|rule| !rule.is_empty()), "{name}");
        let tab_separated = Path::new(name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("tsv"));
        let codec = if tab_separated { &tsv } else { &csv };
        let input = std::fs::read(corpus().join(name)).unwrap();
        let read = codec.read(&input);
        if outcome == "ok" {
            positive += 1;
            let edges = read.unwrap_or_else(|e| panic!("{name}: {e}"));
            let text = write(&edges, codec.dialect()).unwrap();
            let expected =
                std::fs::read_to_string(corpus().join(format!("{name}.expected"))).unwrap();
            assert_eq!(text, expected, "{name}");
            assert_eq!(codec.read(text.as_bytes()).unwrap(), edges, "{name}");
        } else {
            negative += 1;
            let error = read.expect_err(name);
            assert_eq!(refusal_name(&error), outcome, "{name}: {error}");
        }
    }
    assert_eq!((positive, negative), (11, 7));
}
