//! The CRD reader against the vendored Kubernetes corpus.
//!
//! Every CRD under `k8s-crd/upstream` (taken from the Kubernetes repository
//! at v1.37.2) must read, and its canonical text must read back as the same
//! schema and write as the same text. Every case under `k8s-crd/invalid`,
//! built from one rule of the CRD definition and listed in
//! `k8s-crd/MANIFEST` with the refusal it draws, must be refused with that
//! refusal at that path. A case that does not pass is a declared limit
//! listed in `k8s-crd-limits.txt`, and the test checks the list is exactly
//! the set of failing cases.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use panproto_io::config::k8s_crd::{CrdErrorKind, read_crd, write_crd};
use panproto_schema::Schema;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance")
}

/// The schema as sorted renderings of its vertices, ordered out-edges,
/// constraints, required edges and keys.
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
    for (id, edges) in &schema.required {
        let mut names: Vec<String> = edges.iter().map(|e| format!("{:?}", e.name)).collect();
        names.sort();
        out.push(format!("required {id} {names:?}"));
    }
    out.sort();
    out
}

/// The outcome of one upstream case: `None` when it passes.
fn upstream_case(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).unwrap();
    let reading = match read_crd(&bytes) {
        Ok(reading) => reading,
        Err(error) => return Some(format!("refused: {error}")),
    };
    let text = match write_crd(&reading.schema) {
        Ok(text) => text,
        Err(error) => return Some(format!("write refused: {error}")),
    };
    let again = match read_crd(text.as_bytes()) {
        Ok(again) => again,
        Err(error) => return Some(format!("canonical text refused: {error}")),
    };
    if rendering(&again.schema) != rendering(&reading.schema) {
        return Some("canonical text reads as another schema".into());
    }
    if write_crd(&again.schema).ok().as_deref() != Some(text.as_str()) {
        return Some("canonical text is not a fixed point".into());
    }
    None
}

/// The MANIFEST lines: file, path, refusal class.
fn manifest() -> Vec<(String, String, String)> {
    std::fs::read_to_string(corpus().join("k8s-crd/MANIFEST"))
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

fn class(kind: &CrdErrorKind) -> String {
    let debug = format!("{kind:?}");
    debug
        .split(['(', ' '])
        .next()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn kubernetes_crd_corpus_cases_pass_or_are_declared_limits() {
    let mut failing = BTreeMap::new();
    let mut upstream: Vec<PathBuf> = std::fs::read_dir(corpus().join("k8s-crd/upstream"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "yaml"))
        .collect();
    upstream.sort();
    assert_eq!(upstream.len(), 11, "the upstream corpus has 11 CRDs");
    for path in &upstream {
        if let Some(outcome) = upstream_case(path) {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            failing.insert(format!("upstream/{name}"), outcome);
        }
    }
    let cases = manifest();
    assert_eq!(cases.len(), 18, "the MANIFEST lists 18 refusal cases");
    for (file, path, expected) in &cases {
        let bytes = std::fs::read(corpus().join("k8s-crd/invalid").join(file)).unwrap();
        match read_crd(&bytes) {
            Ok(_) => {
                failing.insert(format!("invalid/{file}"), "read".into());
            }
            Err(error) if error.path == *path && class(&error.kind) == *expected => {}
            Err(error) => {
                failing.insert(
                    format!("invalid/{file}"),
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
        std::fs::read_to_string(corpus().join("k8s-crd-limits.txt"))
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
