//! The GraphQL reader against the vendored GraphQL corpus.
//!
//! Every document under `graphql/upstream` (SDL strings of the graphql-js
//! test utilities, taken from the repository at the commit `UPSTREAM.md`
//! records) and the repository's `fixtures/graphql/swapi.graphql` must
//! read, and its canonical text must read back as the same schema and
//! write as the same text, unless `graphql/MANIFEST` lists it with the
//! refusal it draws. Every case under `graphql/invalid`, written from one
//! rule of the specification and listed in the MANIFEST, must be refused
//! with that refusal at that schema coordinate. A case that does not pass
//! is a declared limit listed in `graphql-limits.txt`, and the test checks
//! the list is exactly the set of failing cases.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use panproto_io::api::graphql::{GraphQlErrorKind, read_graphql, write_graphql};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance")
}

/// The bytes of a case: a corpus file, or the repository's SWAPI fixture.
fn bytes(name: &str) -> Vec<u8> {
    if name == SWAPI {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/graphql/swapi.graphql"),
        )
        .unwrap()
    } else {
        std::fs::read(corpus().join("graphql").join(name)).unwrap()
    }
}

/// The repository fixture read as an upstream case.
const SWAPI: &str = "fixtures/graphql/swapi.graphql";

/// The outcome of one upstream document that must read: `None` when it
/// passes.
fn upstream_case(name: &str) -> Option<String> {
    let schema = match read_graphql(&bytes(name)) {
        Ok(reading) => reading.schema,
        Err(error) => return Some(format!("refused: {error}")),
    };
    let text = match write_graphql(&schema) {
        Ok(text) => text,
        Err(error) => return Some(format!("write refused: {error}")),
    };
    let again = match read_graphql(text.as_bytes()) {
        Ok(again) => again.schema,
        Err(error) => return Some(format!("canonical text refused: {error}")),
    };
    if panproto_schema::canonical_bytes(&again) != panproto_schema::canonical_bytes(&schema) {
        return Some("canonical text reads as another schema".into());
    }
    if write_graphql(&again).ok().as_deref() != Some(text.as_str()) {
        return Some("canonical text is not a fixed point".into());
    }
    None
}

/// The MANIFEST lines: file, schema coordinate, refusal class.
fn manifest() -> Vec<(String, String, String)> {
    std::fs::read_to_string(corpus().join("graphql/MANIFEST"))
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let fields: Vec<&str> = l.split('\t').collect();
            (
                fields[0].to_owned(),
                fields[1].to_owned(),
                fields[2].to_owned(),
            )
        })
        .collect()
}

/// The class of a refusal: the rule, the construct not read, or the kind.
fn class(kind: &GraphQlErrorKind) -> String {
    match kind {
        GraphQlErrorKind::Invalid(rule) => format!("{rule:?}"),
        GraphQlErrorKind::NotRead(what) => format!("{what:?}"),
        other => format!("{other:?}")
            .split(['(', ' '])
            .next()
            .unwrap_or_default()
            .to_owned(),
    }
}

fn upstream_documents() -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(corpus().join("graphql/upstream"))
        .unwrap()
        .map(|e| format!("upstream/{}", e.unwrap().file_name().to_string_lossy()))
        .collect();
    found.push(SWAPI.to_owned());
    found.sort();
    found
}

#[test]
fn graphql_corpus_cases_pass_or_are_declared_limits() {
    let cases = manifest();
    let mut failing = BTreeMap::new();
    let upstream = upstream_documents();
    assert_eq!(upstream.len(), 3, "the upstream corpus has 3 documents");
    for name in &upstream {
        if cases.iter().any(|(file, ..)| file == name) {
            continue;
        }
        if let Some(outcome) = upstream_case(name) {
            failing.insert(name.clone(), outcome);
        }
    }
    assert_eq!(cases.len(), 59, "the MANIFEST lists 59 refusals");
    for (file, path, expected) in &cases {
        match read_graphql(&bytes(file)) {
            Ok(_) => {
                failing.insert(file.clone(), "read".into());
            }
            Err(error) if error.path == *path && class(&error.kind) == *expected => {}
            Err(error) => {
                failing.insert(
                    file.clone(),
                    format!(
                        "refused as {} at {:?}: {error}",
                        class(&error.kind),
                        error.path
                    ),
                );
            }
        }
    }
    let declared: BTreeMap<String, String> =
        std::fs::read_to_string(corpus().join("graphql-limits.txt"))
            .unwrap()
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .map(|l| {
                let (case, limit) = l.split_once('\t').unwrap();
                (case.to_owned(), limit.to_owned())
            })
            .collect();
    let failing_cases: Vec<&String> = failing.keys().collect();
    let declared_cases: Vec<&String> = declared.keys().collect();
    assert_eq!(
        failing_cases, declared_cases,
        "failing cases differ from the declared limits: {failing:#?}"
    );
}

/// Every invalid case is listed in the MANIFEST, so no case is skipped.
#[test]
fn every_invalid_graphql_case_is_listed() {
    let mut listed: Vec<String> = manifest()
        .into_iter()
        .map(|(file, ..)| file)
        .filter(|f| f.starts_with("invalid/"))
        .collect();
    let mut files: Vec<String> = std::fs::read_dir(corpus().join("graphql/invalid"))
        .unwrap()
        .map(|e| format!("invalid/{}", e.unwrap().file_name().to_string_lossy()))
        .collect();
    files.sort();
    listed.sort();
    assert_eq!(files, listed);
    assert_eq!(files.len(), 58);
}
