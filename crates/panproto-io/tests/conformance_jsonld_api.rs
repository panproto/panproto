//! The JSON-LD route against the vendored W3C JSON-LD 1.1 API `toRdf`
//! tests.
//!
//! Each entry of `toRdf-manifest.jsonld` is a positive evaluation test, a
//! negative evaluation test or a positive syntax test. An entry whose
//! options ask for what the reader does not offer (JSON-LD 1.0 processing,
//! generalized RDF, an `rdfDirection` form, an `expandContext`) is not run
//! and counts as a limit of that class. Every other entry is read with the
//! base IRI its options give, or the manifest's `baseIri` followed by its
//! input path.
//!
//! A positive evaluation test passes when the route reads the input, the
//! triples are isomorphic to those the N-Triples route reads from the
//! expected N-Quads (which then hold no graph name), the format-preserving
//! route reads the same graph and writes the unedited graph back as the
//! input's bytes, and canonical JSON-LD reads back as the same triples. A
//! negative test passes when the route refuses the input with the JSON-LD
//! error the manifest expects. A positive syntax test passes when the route
//! reads the input.
//!
//! An entry the route does not pass is a declared limit in
//! `json-ld-api-limits.txt`, with the class the book's RDF formats page
//! states for it, and the test fails if the list and the failing entries
//! differ in either direction.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use panproto_io::rdf::jsonld::{JsonLdCodec, JsonLdRefusal, write};
use panproto_io::rdf::ntriples::NTriplesCodec;
use panproto_io::rdf::{RdfReadError, RdfReadErrorKind, Triple, from_graph};
use panproto_protocols::rdf::jsonld::graph_schema;

#[path = "support/rdf_isomorphism.rs"]
mod rdf_isomorphism;
use rdf_isomorphism::isomorphic;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance/json-ld-api")
}

/// The declared limits: `toRdf/#tNNNN`, then the class the book states.
fn declared_limits() -> BTreeMap<String, String> {
    std::fs::read_to_string(corpus().join("../json-ld-api-limits.txt"))
        .unwrap()
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let (case, class) = line.split_once(' ').unwrap();
            (case.to_owned(), class.trim().to_owned())
        })
        .collect()
}

/// The class of a refusal, as the limits file names it.
fn refusal_class(error: &RdfReadError) -> String {
    match &error.kind {
        RdfReadErrorKind::JsonLd { refusal, .. } => match refusal {
            JsonLdRefusal::Unsupported { feature } => {
                format!("unsupported {feature}").replace(' ', "-")
            }
            JsonLdRefusal::Error { code } => format!("error {code}").replace(' ', "-"),
            JsonLdRefusal::NamedGraph => "named-graph".into(),
            JsonLdRefusal::FreeFloating => "refused-free-floating".into(),
            JsonLdRefusal::BlankPredicate => "refused-blank-predicate".into(),
            JsonLdRefusal::RelativeIri => "refused-relative-iri".into(),
            JsonLdRefusal::IriCharacter { .. } => "refused-iri-character".into(),
            JsonLdRefusal::InvalidLanguageTag => "refused-language-tag".into(),
            JsonLdRefusal::MalformedIri => "refused-malformed-iri".into(),
            JsonLdRefusal::ReservedKeyword { .. } => "refused-reserved-keyword".into(),
            JsonLdRefusal::DuplicateKey { .. } => "refused-duplicate-key".into(),
            JsonLdRefusal::NumberOutOfRange => "refused-number-range".into(),
            _ => "unclassified".into(),
        },
        _ => "json-syntax".into(),
    }
}

/// The class of an entry that is not run, from its options.
fn option_class(entry: &serde_json::Value) -> Option<&'static str> {
    let option = entry.get("option")?;
    let text = |key: &str| option.get(key).and_then(serde_json::Value::as_str);
    if text("specVersion") == Some("json-ld-1.0") || text("processingMode") == Some("json-ld-1.0") {
        Some("json-ld-1.0")
    } else if option.get("produceGeneralizedRdf").is_some() {
        Some("generalized-rdf-option")
    } else if option.get("rdfDirection").is_some() {
        Some("rdf-direction-option")
    } else if option.get("expandContext").is_some() {
        Some("expand-context-option")
    } else {
        None
    }
}

/// What the route does with one run entry, as a limit class, when it does
/// not pass.
fn run(entry: &serde_json::Value, base: &str, ntriples: &NTriplesCodec) -> Option<String> {
    let field = |key: &str| entry.get(key).and_then(serde_json::Value::as_str);
    let input = field("input").unwrap();
    let base = entry
        .get("option")
        .and_then(|o| o.get("base"))
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| format!("{base}{input}"), str::to_owned);
    let codec = JsonLdCodec::new().unwrap().with_base(base).unwrap();
    let bytes = std::fs::read(corpus().join(input)).unwrap();
    let kinds: Vec<&str> = entry["@type"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    let read = codec.read(&bytes);
    if kinds.contains(&"jld:NegativeEvaluationTest") {
        let expected = field("expectErrorCode").unwrap();
        return match read {
            Ok(_) => Some("over-accepted".into()),
            Err(RdfReadError {
                kind:
                    RdfReadErrorKind::JsonLd {
                        refusal: JsonLdRefusal::Error { code },
                        ..
                    },
                ..
            }) if code.code() == expected => None,
            Err(error) => Some(refusal_class(&error)),
        };
    }
    let triples = match read {
        Ok(triples) => triples,
        Err(error) => return Some(refusal_class(&error)),
    };
    let set: BTreeSet<Triple> = triples.into_iter().collect();
    let schema = graph_schema().unwrap();
    let (graph, complement) = codec.parse_graph_preserving(&schema, &bytes).unwrap();
    let preserved: BTreeSet<Triple> = from_graph(&graph).unwrap().into_iter().collect();
    if preserved != set {
        return Some("preserving-route-reads-another-graph".into());
    }
    if codec
        .emit_graph_preserving(&schema, &graph, &complement)
        .ok()
        != Some(bytes)
    {
        return Some("preserving-route-rewrites-bytes".into());
    }
    let canonical = write(&set.iter().cloned().collect::<Vec<_>>());
    let reread: BTreeSet<Triple> = codec
        .read(canonical.as_bytes())
        .unwrap()
        .into_iter()
        .collect();
    if reread != set {
        return Some("canonical-reads-back-otherwise".into());
    }
    if let Some(expect) = field("expect").filter(|_| kinds.contains(&"jld:PositiveEvaluationTest"))
    {
        let expected = std::fs::read(corpus().join(expect)).unwrap();
        let Ok(expected) = ntriples.read(&expected) else {
            return Some("expected-named-graph".into());
        };
        if !isomorphic(&set, &expected.into_iter().collect()) {
            return Some("not-isomorphic".into());
        }
    }
    None
}

#[test]
fn to_rdf_entries_pass_or_are_declared_limits() {
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(corpus().join("toRdf-manifest.jsonld")).unwrap())
            .unwrap();
    let base = manifest["baseIri"].as_str().unwrap();
    let entries = manifest["sequence"].as_array().unwrap();
    assert_eq!(
        entries.len(),
        467,
        "the vendored manifest lists 467 entries"
    );
    let limits = declared_limits();
    let ntriples = NTriplesCodec::new().unwrap();
    let mut undeclared = Vec::new();
    let mut failing = Vec::new();
    let mut passed = 0;
    for entry in entries {
        let key = format!("toRdf/{}", entry["@id"].as_str().unwrap());
        let outcome = option_class(entry)
            .map(str::to_owned)
            .or_else(|| run(entry, base, &ntriples));
        match outcome {
            None => passed += 1,
            Some(class) => {
                if limits.get(&key) != Some(&class) {
                    undeclared.push(format!("{key} {class}"));
                }
                failing.push(key);
            }
        }
    }
    let stale: Vec<_> = limits.keys().filter(|key| !failing.contains(key)).collect();
    assert!(
        undeclared.is_empty() && stale.is_empty(),
        "passed {passed}\nundeclared:\n{}\nstale limits: {stale:?}",
        undeclared.join("\n")
    );
    assert_eq!(passed + failing.len(), 467);
}
