//! Every vertex kind a protocol's parser emits on the repository's
//! fixtures is classified: either the protocol's value-kind table gives the
//! scalar kind its vertices hold, or the table declares the kind
//! non-scalar (a record, a list, a reference, a declaration).
//!
//! The fixtures are the real-world schemas under `fixtures/`, parsed by
//! each protocol's registered parser. A parser that starts emitting a new
//! vertex kind fails this test until the kind is classified, so a codec
//! reading instances of that schema never meets a scalar vertex whose kind
//! it has to guess.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use panproto_protocols::registry::descriptor;
use panproto_protocols::semantics::value_kinds;
use panproto_protocols::{parse_schema_document, parse_schema_source};
use panproto_schema::Schema;

/// The fixture files of each protocol, relative to `fixtures/`, and
/// whether the protocol reads a JSON document (else source text).
const FIXTURES: &[(&str, bool, &[&str])] = &[
    (
        "atproto",
        true,
        &[
            "atproto/lexicons/app.bsky.actor.profile.json",
            "atproto/lexicons/app.bsky.feed.like.json",
            "atproto/lexicons/app.bsky.feed.post.json",
            "atproto/lexicons/app.bsky.feed.repost.json",
            "atproto/lexicons/app.bsky.graph.follow.json",
            "atproto/lexicons/com.atproto.repo.createRecord.json",
            "atproto/lexicons/site.standard.document.json",
        ],
    ),
    ("graphql", false, &["graphql/swapi.graphql"]),
    (
        "json-schema",
        true,
        &[
            "jsonschema/github-workflow.json",
            "jsonschema/package.json",
            "jsonschema/tsconfig.json",
        ],
    ),
    (
        "protobuf",
        false,
        &[
            "protobuf/common.proto",
            "protobuf/descriptor.proto",
            "protobuf/resource.proto",
            "protobuf/trace.proto",
        ],
    ),
    (
        "sql",
        false,
        &["sql/sakila-postgres.sql", "sql/sakila-sqlite.sql"],
    ),
];

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(relative)
}

fn parse(protocol: &str, document: bool, relative: &str) -> Schema {
    let text = std::fs::read_to_string(fixture(relative)).expect("the fixture exists");
    if document {
        let doc: serde_json::Value = serde_json::from_str(&text).expect("the fixture is JSON");
        parse_schema_document(protocol, &doc)
    } else {
        parse_schema_source(protocol, &text)
    }
    .unwrap_or_else(|e| panic!("{relative}: {e}"))
}

/// The vertex kinds each protocol's parser emits on its fixtures.
fn emitted() -> BTreeMap<&'static str, BTreeSet<String>> {
    FIXTURES
        .iter()
        .map(|(protocol, document, files)| {
            let kinds = files
                .iter()
                .flat_map(|file| {
                    parse(protocol, *document, file)
                        .vertices
                        .values()
                        .map(|v| v.kind.to_string())
                        .collect::<Vec<_>>()
                })
                .collect();
            (*protocol, kinds)
        })
        .collect()
}

#[test]
fn every_emitted_vertex_kind_has_a_value_kind() {
    let mut unclassified = BTreeMap::new();
    for (protocol, kinds) in emitted() {
        let d = descriptor(protocol).expect("a registered protocol");
        let built = (d.protocol)();
        let non_scalar = value_kinds::non_scalar(protocol);
        let missing: Vec<String> = kinds
            .into_iter()
            .filter(|k| !built.value_kinds.contains_key(k) && !non_scalar.contains(k.as_str()))
            .collect();
        if !missing.is_empty() {
            unclassified.insert(protocol, missing);
        }
    }
    assert!(
        unclassified.is_empty(),
        "vertex kinds with no value kind and no non-scalar declaration: {unclassified:#?}"
    );
}

/// Every value-kind and non-scalar row names a vertex kind its protocol
/// declares, no kind is both scalar and non-scalar, and no non-scalar row
/// is listed twice.
#[test]
fn every_classified_kind_is_a_kind_of_its_protocol() {
    let mut seen = BTreeSet::new();
    for row in value_kinds::non_scalar_rows() {
        assert!(
            seen.insert((row.protocol, row.vertex_kind)),
            "{} {} is declared non-scalar twice",
            row.protocol,
            row.vertex_kind
        );
    }
    let scalar: Vec<(&str, &str)> = value_kinds::rows()
        .map(|row| (row.protocol, row.vertex_kind))
        .collect();
    let non_scalar: Vec<(&str, &str)> = value_kinds::non_scalar_rows()
        .map(|row| (row.protocol, row.vertex_kind))
        .collect();
    for (protocol, kind) in scalar.iter().chain(&non_scalar) {
        let d = descriptor(protocol).unwrap_or_else(|| panic!("{protocol} is registered"));
        assert_eq!(d.name, *protocol, "rows use the canonical name");
        assert!(
            (d.protocol)().obj_kinds.iter().any(|k| k == kind),
            "{protocol} declares no vertex kind {kind}"
        );
    }
    for pair in &scalar {
        assert!(
            !non_scalar.contains(pair),
            "{pair:?} is both scalar and non-scalar"
        );
    }
}
