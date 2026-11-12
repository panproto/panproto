//! The `AsyncAPI` reader against the vendored `AsyncAPI` corpus.
//!
//! Every example document under `asyncapi/upstream/examples` (taken
//! unchanged from the asyncapi/spec repository at the 3.1.0 release) must
//! read, and its canonical text must read back as the same schema and
//! write as the same text, unless `asyncapi/MANIFEST` lists it with the
//! refusal it draws. The five `social-media` services are read as bundles
//! with the four documents of `social-media/common` as fragment documents,
//! each under its path as retrieval URI. Every case under
//! `asyncapi/invalid`, written from one rule of the specification and
//! listed in the MANIFEST, must be refused with that refusal at that JSON
//! pointer. A case that does not pass is a declared limit listed in
//! `asyncapi-limits.txt`, and the test checks the list is exactly the set
//! of failing cases.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use panproto_io::api::asyncapi::{
    AsyncApiDocument, AsyncApiErrorKind, AsyncApiReading, DocumentKind, read_asyncapi_bundle,
    write_asyncapi,
};
use panproto_schema::Schema;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance")
}

/// The retrieval URI a corpus file is read under.
fn uri(name: &str) -> String {
    format!("https://corpus.panproto.test/asyncapi/{name}")
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

/// The fragment documents of the social-media example.
const COMMON: [&str; 4] = [
    "upstream/examples/social-media/common/messages.yaml",
    "upstream/examples/social-media/common/parameters.yaml",
    "upstream/examples/social-media/common/schemas.yaml",
    "upstream/examples/social-media/common/servers.yaml",
];

/// Read the entry document `entry` (its bytes `source`) under its URI, with
/// the fragment documents of its example when it is a social-media
/// service.
fn read(
    name: &str,
    source: &[u8],
) -> Result<AsyncApiReading, panproto_io::api::asyncapi::AsyncApiError> {
    let entry_uri = uri(name);
    let common: Vec<(String, Vec<u8>)> = if name.contains("social-media/") {
        COMMON
            .iter()
            .map(|c| {
                (
                    uri(c),
                    std::fs::read(corpus().join("asyncapi").join(c)).unwrap(),
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut documents = vec![AsyncApiDocument {
        retrieval_uri: Some(&entry_uri),
        source,
        kind: DocumentKind::AsyncApi,
    }];
    for (u, bytes) in &common {
        documents.push(AsyncApiDocument {
            retrieval_uri: Some(u),
            source: bytes,
            kind: DocumentKind::Fragment,
        });
    }
    read_asyncapi_bundle(&documents)
}

/// The outcome of one upstream document that must read: `None` when it
/// passes.
fn upstream_case(name: &str) -> Option<String> {
    let bytes = std::fs::read(corpus().join("asyncapi").join(name)).unwrap();
    let reading = match read(name, &bytes) {
        Ok(reading) => reading,
        Err(error) => return Some(format!("refused: {error}")),
    };
    let text = match write_asyncapi(&reading.schema, &reading.roots[0]) {
        Ok(text) => text,
        Err(error) => return Some(format!("write refused: {error}")),
    };
    let again = match read(name, text.as_bytes()) {
        Ok(again) => again,
        Err(error) => return Some(format!("canonical text refused: {error}")),
    };
    if rendering(&again.schema) != rendering(&reading.schema) || again.roots != reading.roots {
        return Some("canonical text reads as another schema".into());
    }
    if write_asyncapi(&again.schema, &again.roots[0])
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
    std::fs::read_to_string(corpus().join("asyncapi/MANIFEST"))
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

fn class(kind: &AsyncApiErrorKind) -> String {
    let debug = format!("{kind:?}");
    debug
        .split(['(', ' '])
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// The upstream entry documents: every example but the fragments.
fn upstream_documents() -> Vec<String> {
    fn walk(dir: &Path, prefix: &str, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let name = format!("{prefix}/{}", entry.file_name().to_string_lossy());
            if entry.file_type().unwrap().is_dir() {
                walk(&entry.path(), &name, found);
            } else {
                found.push(name);
            }
        }
    }
    let mut found = Vec::new();
    walk(
        &corpus().join("asyncapi/upstream/examples"),
        "upstream/examples",
        &mut found,
    );
    found.retain(|name| !COMMON.contains(&name.as_str()));
    found.sort();
    found
}

#[test]
fn asyncapi_corpus_cases_pass_or_are_declared_limits() {
    let cases = manifest();
    let mut failing = BTreeMap::new();
    let upstream = upstream_documents();
    assert_eq!(
        upstream.len(),
        24,
        "the upstream corpus has 24 entry documents"
    );
    for name in &upstream {
        if cases.iter().any(|(file, ..)| file == name) {
            continue;
        }
        if let Some(outcome) = upstream_case(name) {
            failing.insert(name.clone(), outcome);
        }
    }
    assert_eq!(cases.len(), 57, "the MANIFEST lists 57 refusals");
    for (file, path, expected) in &cases {
        let bytes = std::fs::read(corpus().join("asyncapi").join(file)).unwrap();
        match read(file, &bytes) {
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
        std::fs::read_to_string(corpus().join("asyncapi-limits.txt"))
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
fn every_invalid_asyncapi_case_is_listed() {
    let listed: Vec<String> = manifest()
        .into_iter()
        .map(|(file, ..)| file)
        .filter(|f| f.starts_with("invalid/"))
        .collect();
    let mut files: Vec<String> = std::fs::read_dir(corpus().join("asyncapi/invalid"))
        .unwrap()
        .map(|e| format!("invalid/{}", e.unwrap().file_name().to_string_lossy()))
        .collect();
    files.sort();
    let mut listed = listed;
    listed.sort();
    assert_eq!(files, listed);
    assert_eq!(files.len(), 57);
}
