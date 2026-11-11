//! The RDF routes against the vendored W3C RDF 1.1 test suites.
//!
//! Each entry of a suite's `manifest.ttl` is a positive or a negative syntax
//! test. The N-Triples route passes a positive test when it reads the file,
//! its canonical output reads back as the same graph and writing that graph
//! again gives the same bytes; it passes a negative test when it refuses
//! the file. An entry the route does not pass is a declared limit in
//! `rdf-tests-limits.txt`, with the class the book's RDF formats page states
//! for it, and the test fails if the list and the failing entries differ in
//! either direction.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use panproto_inst::GInstance;
use panproto_io::ProtocolRegistry;
use panproto_io::default_registry;
use panproto_protocols::rdf::ntriples::graph_schema;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance/rdf-tests")
}

/// The entries of a suite's manifest: name, whether positive, and file.
fn entries(suite: &str) -> Vec<(String, bool, String)> {
    let manifest = std::fs::read_to_string(corpus().join(suite).join("manifest.ttl")).unwrap();
    let mut entries = Vec::new();
    for block in manifest.split("\n<#").skip(1) {
        let Some((name, rest)) = block.split_once('>') else {
            continue;
        };
        let Some(kind) = rest.split_whitespace().nth(1) else {
            continue;
        };
        if !kind.starts_with("rdft:") {
            continue;
        }
        let positive = !kind.contains("Negative");
        let action = rest
            .split_once("mf:action")
            .and_then(|(_, after)| after.split_once('<'))
            .and_then(|(_, after)| after.split_once('>'))
            .map(|(file, _)| file.to_owned())
            .unwrap();
        entries.push((name.to_owned(), positive, action));
    }
    entries
}

/// What the N-Triples route does with one entry, when it does not pass.
fn run_ntriples(registry: &ProtocolRegistry, positive: bool, bytes: &[u8]) -> Option<String> {
    let schema = graph_schema().unwrap();
    let read = registry.parse_graph("ntriples", &schema, bytes);
    match (positive, read) {
        (false, Ok(_)) => Some("accepted".into()),
        (false, Err(_)) => None,
        (true, Err(error)) => Some(format!("refused: {error}")),
        (true, Ok(graph)) => {
            let written = registry.emit_graph("ntriples", &schema, &graph).unwrap();
            let again: GInstance = registry.parse_graph("ntriples", &schema, &written).unwrap();
            let rewritten = registry.emit_graph("ntriples", &schema, &again).unwrap();
            if rewritten != written {
                Some("canonical output is not stable".into())
            } else if again.node_count() != graph.node_count()
                || again.edge_count() != graph.edge_count()
            {
                Some("canonical output reads back as another graph".into())
            } else {
                None
            }
        }
    }
}

/// The declared limits: `suite/entry`, then the class the book states.
fn declared_limits() -> BTreeMap<String, String> {
    std::fs::read_to_string(corpus().join("../rdf-tests-limits.txt"))
        .unwrap()
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let (case, class) = line.split_once(' ').unwrap();
            (case.to_owned(), class.trim().to_owned())
        })
        .collect()
}

/// The class of an observed failure, as the limits file names it.
fn limit_class(outcome: &str) -> &'static str {
    if outcome.contains("cannot read the raw character") {
        "raw-nul-or-delete"
    } else if outcome == "accepted" {
        "over-accepted"
    } else {
        "unclassified"
    }
}

#[test]
fn ntriples_suite_entries_pass_or_are_declared_limits() {
    let registry = default_registry();
    let limits = declared_limits();
    let suite = "rdf11/rdf-n-triples";
    let entries = entries(suite);
    assert_eq!(entries.len(), 70, "the vendored manifest lists 70 entries");
    assert_eq!(
        entries.iter().filter(|(_, positive, _)| *positive).count(),
        41
    );
    let mut undeclared = Vec::new();
    let mut failing = Vec::new();
    for (name, positive, file) in &entries {
        let bytes = std::fs::read(corpus().join(suite).join(file)).unwrap();
        if let Some(outcome) = run_ntriples(&registry, *positive, &bytes) {
            let key = format!("{suite}/{name}");
            let class = limit_class(&outcome);
            if limits.get(&key).map(String::as_str) != Some(class) {
                undeclared.push(format!("{key} {class}: {outcome}"));
            }
            failing.push(key);
        }
    }
    let stale: Vec<_> = limits
        .keys()
        .filter(|key| key.starts_with(suite) && !failing.contains(key))
        .collect();
    assert!(
        undeclared.is_empty() && stale.is_empty(),
        "undeclared:\n{}\nstale limits: {stale:?}",
        undeclared.join("\n")
    );
}
