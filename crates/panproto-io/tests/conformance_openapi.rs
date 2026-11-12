//! The `OpenAPI` reader against the vendored `OpenAPI` corpus.
//!
//! Every document under `openapi/upstream` (taken unchanged from the
//! OpenAPI-Specification repository: the 3.1 meta-schema's pass and fail
//! test documents and the 3.1 JSON examples) must read, and its canonical
//! text must read back as the same schema and write as the same text,
//! unless `openapi/MANIFEST` lists it with the refusal it draws. Every case
//! under `openapi/invalid`, written from one rule of the specification and
//! listed in the MANIFEST, must be refused with that refusal at that JSON
//! pointer. A case that does not pass is a declared limit listed in
//! `openapi-limits.txt`, and the test checks the list is exactly the set
//! of failing cases.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use panproto_io::api::openapi::{OpenApiErrorKind, read_openapi, write_openapi};
use panproto_schema::Schema;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance")
}

/// The schema as sorted renderings of its vertices, ordered out-edges and
/// constraints, and its entries.
fn rendering(schema: &Schema) -> Vec<String> {
    let mut out = Vec::new();
    for (id, vertex) in &schema.vertices {
        out.push(format!("vertex {id} {}", vertex.kind));
        let edges: Vec<String> = schema
            .outgoing_edges(id)
            .iter()
            .map(|e| format!("{}>{}:{:?}", e.kind, e.tgt, e.name))
            .collect();
        out.push(format!("edges {id} {edges:?}"));
    }
    for (id, constraints) in &schema.constraints {
        let cs: Vec<String> = constraints
            .iter()
            .map(|c| format!("{}={}", c.sort, c.value))
            .collect();
        out.push(format!("constraints {id} {cs:?}"));
    }
    out.sort();
    out.push(format!("entries {:?}", schema.entries));
    out
}

/// The outcome of one upstream document that must read: `None` when it
/// passes.
fn upstream_case(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).unwrap();
    let reading = match read_openapi(&bytes) {
        Ok(reading) => reading,
        Err(error) => return Some(format!("refused: {error}")),
    };
    let text = match write_openapi(&reading.schema, &reading.roots[0]) {
        Ok(text) => text,
        Err(error) => return Some(format!("write refused: {error}")),
    };
    let again = match read_openapi(text.as_bytes()) {
        Ok(again) => again,
        Err(error) => return Some(format!("canonical text refused: {error}")),
    };
    if rendering(&again.schema) != rendering(&reading.schema) || again.roots != reading.roots {
        return Some("canonical text reads as another schema".into());
    }
    if write_openapi(&again.schema, &again.roots[0])
        .ok()
        .as_deref()
        != Some(text.as_str())
    {
        return Some("canonical text is not a fixed point".into());
    }
    None
}

/// The MANIFEST lines: file, JSON pointer, refusal class.
fn manifest() -> Vec<(String, String, String)> {
    std::fs::read_to_string(corpus().join("openapi/MANIFEST"))
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

fn class(kind: &OpenApiErrorKind) -> String {
    let debug = format!("{kind:?}");
    debug
        .split(['(', ' '])
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn upstream_documents() -> Vec<String> {
    let mut found = Vec::new();
    for dir in ["pass", "fail", "examples"] {
        for entry in std::fs::read_dir(corpus().join("openapi/upstream").join(dir)).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            found.push(format!("upstream/{dir}/{name}"));
        }
    }
    found.sort();
    found
}

#[test]
fn openapi_corpus_cases_pass_or_are_declared_limits() {
    let cases = manifest();
    let mut failing = BTreeMap::new();
    let upstream = upstream_documents();
    assert_eq!(upstream.len(), 48, "the upstream corpus has 48 documents");
    for name in &upstream {
        if cases.iter().any(|(file, ..)| file == name) {
            continue;
        }
        if let Some(outcome) = upstream_case(&corpus().join("openapi").join(name)) {
            failing.insert(name.clone(), outcome);
        }
    }
    assert_eq!(cases.len(), 60, "the MANIFEST lists 60 refusals");
    for (file, path, expected) in &cases {
        let bytes = std::fs::read(corpus().join("openapi").join(file)).unwrap();
        match read_openapi(&bytes) {
            Ok(_) => {
                failing.insert(file.clone(), "read".into());
            }
            Err(error) if error.path == *path && class(&error.kind) == *expected => {}
            Err(error) => {
                failing.insert(
                    file.clone(),
                    format!(
                        "refused as {} at {}: {error}",
                        class(&error.kind),
                        error.path
                    ),
                );
            }
        }
    }
    let declared: BTreeMap<String, String> =
        std::fs::read_to_string(corpus().join("openapi-limits.txt"))
            .unwrap()
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .map(|l| {
                let (case, limit) = l.split_once('\t').unwrap();
                (case.to_owned(), limit.to_owned())
            })
            .collect();
    assert_eq!(declared.len(), 2, "two declared limits");
    let failing_cases: Vec<&String> = failing.keys().collect();
    let declared_cases: Vec<&String> = declared.keys().collect();
    assert_eq!(
        failing_cases, declared_cases,
        "failing cases differ from the declared limits: {failing:#?}"
    );
}

/// Every invalid case is listed in the MANIFEST, so no case is skipped.
#[test]
fn every_invalid_case_is_listed() {
    let listed: Vec<String> = manifest()
        .into_iter()
        .map(|(file, ..)| file)
        .filter(|f| f.starts_with("invalid/"))
        .collect();
    let mut files: Vec<String> = std::fs::read_dir(corpus().join("openapi/invalid"))
        .unwrap()
        .map(|e| format!("invalid/{}", e.unwrap().file_name().to_string_lossy()))
        .collect();
    files.sort();
    let mut listed = listed;
    listed.sort();
    assert_eq!(files, listed);
    assert_eq!(files.len(), 46);
}
