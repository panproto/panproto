//! The RDF routes against the vendored W3C RDF 1.1 test suites.
//!
//! Each entry of a suite's `manifest.ttl` is a positive or a negative syntax
//! test, or (Turtle) an evaluation test. The N-Triples route passes a
//! positive test when it reads the file, its canonical output reads back as
//! the same graph and writing that graph again gives the same bytes; it
//! passes a negative test when it refuses the file.
//!
//! The Turtle route reads each file with the base IRI the manifest assumes
//! for it (`mf:assumedTestBase` followed by the file name). It passes a
//! positive syntax test when it reads the file, the format-preserving
//! route reads the same graph and writes the unedited graph back as the
//! file's bytes, and canonical Turtle reads back as the same triples; an
//! evaluation test when, in addition, the triples it reads are isomorphic
//! to those the N-Triples route reads from the expected result; and a
//! negative syntax test when it refuses the file.
//!
//! An entry a route does not pass is a declared limit in
//! `rdf-tests-limits.txt`, with the class the book's RDF formats page states
//! for it, and the test fails if the list and the failing entries differ in
//! either direction.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use panproto_inst::GInstance;
use panproto_io::ProtocolRegistry;
use panproto_io::default_registry;
use panproto_io::rdf::ntriples::NTriplesCodec;
use panproto_io::rdf::turtle::{TurtleCodec, write};
use panproto_io::rdf::{Term, Triple, from_graph};
use panproto_protocols::rdf::ntriples::graph_schema;

#[path = "support/rdf_isomorphism.rs"]
mod rdf_isomorphism;
use rdf_isomorphism::isomorphic;

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
    } else if outcome.contains("the grammar does not read the document here") {
        "grammar-syntax"
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

/// The entries of the Turtle manifest: name, kind, action file and, for an
/// evaluation test, result file.
fn turtle_entries(manifest: &str) -> Vec<(String, String, String, Option<String>)> {
    let file = |rest: &str, key: &str| {
        rest.split_once(key)
            .and_then(|(_, after)| after.split_once('<'))
            .and_then(|(_, after)| after.split_once('>'))
            .map(|(file, _)| file.to_owned())
    };
    let mut entries = Vec::new();
    for block in manifest.split("\n<#").skip(1) {
        let Some((name, rest)) = block.split_once('>') else {
            continue;
        };
        let Some(kind) = rest.split_whitespace().nth(1) else {
            continue;
        };
        let Some(kind) = kind.strip_prefix("rdft:") else {
            continue;
        };
        let rest = rest.split("\n<#").next().unwrap_or(rest);
        entries.push((
            name.to_owned(),
            kind.to_owned(),
            file(rest, "mf:action").unwrap(),
            file(rest, "mf:result"),
        ));
    }
    entries
}

#[test]
fn isomorphism_maps_blank_nodes_and_refuses_a_different_shape() {
    let blank = |l: &str| Term::Blank(l.into());
    let iri = |i: &str| Term::Iri(i.into());
    let t = |s: Term, o: Term| Triple {
        subject: s,
        predicate: "http://e/p".into(),
        object: o,
    };
    let chain = |x: &str, y: &str| -> BTreeSet<Triple> {
        [t(blank(x), blank(y)), t(blank(y), iri("http://e/o"))]
            .into_iter()
            .collect()
    };
    assert!(isomorphic(&chain("a", "b"), &chain("q", "r")));
    let fork: BTreeSet<Triple> = [t(blank("q"), blank("r")), t(blank("q"), iri("http://e/o"))]
        .into_iter()
        .collect();
    assert!(!isomorphic(&chain("a", "b"), &fork));
    let two_loops: BTreeSet<Triple> = [t(blank("a"), blank("a")), t(blank("b"), blank("b"))]
        .into_iter()
        .collect();
    let one_cycle: BTreeSet<Triple> = [t(blank("a"), blank("b")), t(blank("b"), blank("a"))]
        .into_iter()
        .collect();
    assert!(!isomorphic(&two_loops, &one_cycle));
}

/// The base IRI the manifest assumes for every file of the suite.
fn assumed_base(manifest: &str) -> String {
    manifest
        .split_once("mf:assumedTestBase")
        .and_then(|(_, after)| after.split_once('<'))
        .and_then(|(_, after)| after.split_once('>'))
        .map(|(base, _)| base.to_owned())
        .unwrap()
}

/// What the Turtle route does with one entry, when it does not pass.
fn run_turtle(
    codec: &TurtleCodec,
    ntriples: &NTriplesCodec,
    kind: &str,
    bytes: &[u8],
    expected: Option<&[u8]>,
) -> Option<String> {
    let schema = graph_schema().unwrap();
    let read = codec.read(bytes);
    if kind == "TestTurtleNegativeSyntax" {
        return read.is_ok().then(|| "accepted".to_owned());
    }
    let triples = match read {
        Err(error) => return Some(format!("refused: {error}")),
        Ok(triples) => triples,
    };
    let set: BTreeSet<Triple> = triples.into_iter().collect();
    let Ok((graph, complement)) = codec.parse_graph_preserving(&schema, bytes) else {
        return Some("the preserving route refuses the document".into());
    };
    let preserved: BTreeSet<Triple> = from_graph(&graph).unwrap().into_iter().collect();
    if preserved != set {
        return Some("the preserving route reads another graph".into());
    }
    match codec.emit_graph_preserving(&schema, &graph, &complement) {
        Ok(emitted) if emitted == bytes => {}
        Ok(_) => return Some("the unedited graph is not written back byte for byte".into()),
        Err(error) => return Some(format!("the unedited graph is refused: {error}")),
    }
    let canonical = write(&set.iter().cloned().collect::<Vec<_>>());
    let reread: BTreeSet<Triple> = codec
        .read(canonical.as_bytes())
        .unwrap()
        .into_iter()
        .collect();
    if reread != set {
        return Some("canonical Turtle reads back as other triples".into());
    }
    if let Some(expected) = expected {
        let expected: BTreeSet<Triple> = ntriples.read(expected).unwrap().into_iter().collect();
        if !isomorphic(&set, &expected) {
            return Some("not isomorphic to the expected result".into());
        }
    }
    None
}

#[test]
fn turtle_suite_entries_pass_or_are_declared_limits() {
    let limits = declared_limits();
    let suite = "rdf11/rdf-turtle";
    let manifest = std::fs::read_to_string(corpus().join(suite).join("manifest.ttl")).unwrap();
    let base = assumed_base(&manifest);
    let entries = turtle_entries(&manifest);
    let count = |kind: &str| entries.iter().filter(|e| e.1 == kind).count();
    assert_eq!(
        (
            entries.len(),
            count("TestTurtleEval"),
            count("TestTurtlePositiveSyntax"),
            count("TestTurtleNegativeSyntax")
        ),
        (313, 145, 74, 94),
        "the vendored manifest's entries by kind"
    );
    let ntriples = NTriplesCodec::new().unwrap();
    let mut undeclared = Vec::new();
    let mut failing = Vec::new();
    for (name, kind, action, result) in &entries {
        let codec = TurtleCodec::new()
            .unwrap()
            .with_base(format!("{base}{action}"))
            .unwrap();
        let bytes = std::fs::read(corpus().join(suite).join(action)).unwrap();
        let expected = result
            .as_ref()
            .map(|file| std::fs::read(corpus().join(suite).join(file)).unwrap());
        if let Some(outcome) = run_turtle(&codec, &ntriples, kind, &bytes, expected.as_deref()) {
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
