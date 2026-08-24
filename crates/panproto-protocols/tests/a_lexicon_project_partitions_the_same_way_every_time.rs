//! Which file a shared external placeholder lands in must be a function of
//! the documents, not of the process.
//!
//! A `ref` whose target is outside the parsed set has no owning document, so
//! `parse_lexicon_project` keeps it in the file that references it. When two
//! documents reference the same external target, the target can only live in
//! one of them, and the choice used to be made by whichever edge the
//! monolith's edge table happened to hand over first. Identical input then
//! yields different per-file schemas run to run, which shows up downstream as
//! spurious version-control diffs.
//!
//! This parses one such set in many separate processes and compares the whole
//! partition.
//!
//! Run the child directly with `PP_LEXICON_PARTITION_DUMP=1` to see one
//! process's answer.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use panproto_protocols::web_document::atproto::{LexiconDoc, parse_lexicon_project};

/// How many separate processes the answer is compared across.
const PROCESSES: usize = 48;

/// Two documents, each referencing the same def in a third lexicon that is not
/// part of the set. Both also carry a local def, so each file has content of
/// its own and the shared placeholder is the only thing in contention.
fn shared_external_ref_docs() -> Vec<LexiconDoc> {
    let mut docs = Vec::new();
    for name in ["alpha", "beta", "gamma", "delta"] {
        docs.push(LexiconDoc {
            path: PathBuf::from(format!("{name}.json")),
            value: serde_json::json!({
                "lexicon": 1,
                "id": format!("pub.layers.{name}"),
                "defs": {
                    "main": {
                        "type": "record",
                        "record": {
                            "type": "object",
                            "required": ["anchor", "own"],
                            "properties": {
                                "anchor": {
                                    "type": "ref",
                                    "ref": "pub.layers.elsewhere#sharedAnchor"
                                },
                                "own": {"type": "ref", "ref": "#local"}
                            }
                        }
                    },
                    "local": {
                        "type": "object",
                        "required": ["n"],
                        "properties": {"n": {"type": "integer"}}
                    }
                }
            }),
        });
    }
    docs
}

/// The whole partition, written out so that two runs can be compared literally.
fn dump() -> String {
    use std::fmt::Write as _;

    let project = parse_lexicon_project(&shared_external_ref_docs()).expect("the fixture parses");

    let mut out = String::new();
    for (path, schema) in &project.files {
        let mut vertices: Vec<String> = schema.vertices.keys().map(ToString::to_string).collect();
        vertices.sort_unstable();
        let _ = writeln!(out, "{} vertices = {vertices:?}", path.display());

        let mut edges: Vec<String> = schema
            .edges
            .keys()
            .map(|e| format!("{}->{}({}:{:?})", e.src, e.tgt, e.kind, e.name))
            .collect();
        edges.sort_unstable();
        let _ = writeln!(out, "{} edges = {edges:?}", path.display());
    }

    let cross: BTreeMap<String, Vec<String>> = project
        .cross_file_edges
        .iter()
        .map(|(path, edges)| {
            let mut rendered: Vec<String> = edges
                .iter()
                .map(|e| format!("{}->{}({}:{:?})", e.src, e.tgt, e.kind, e.name))
                .collect();
            rendered.sort_unstable();
            (path.display().to_string(), rendered)
        })
        .collect();
    let _ = writeln!(out, "cross-file edges = {cross:?}");
    out
}

/// The child half: one process's partition, printed.
#[test]
fn one_process_answer() {
    if std::env::var_os("PP_LEXICON_PARTITION_DUMP").is_none() {
        // Nothing to do: the parent below is what runs this with the variable
        // set. Left as a normal test so it type-checks in every run.
        return;
    }
    print!("<<<{}>>>", dump());
}

/// Extract what the child printed between the markers.
fn child_answer(exe: &std::path::Path) -> String {
    let output = Command::new(exe)
        .args(["one_process_answer", "--exact", "--nocapture"])
        .env("PP_LEXICON_PARTITION_DUMP", "1")
        .output()
        .expect("the test binary re-runs");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let start = text.find("<<<").expect("the child printed its answer");
    let end = text.find(">>>").expect("the child closed its answer");
    text[start + 3..end].to_owned()
}

#[test]
fn the_partition_does_not_depend_on_the_hash_seed() {
    if std::env::var_os("PP_LEXICON_PARTITION_DUMP").is_some() {
        return;
    }
    let exe = std::env::current_exe().expect("the test binary knows its own path");
    let mut distinct: BTreeMap<String, usize> = BTreeMap::new();
    for _ in 0..PROCESSES {
        *distinct.entry(child_answer(&exe)).or_insert(0) += 1;
    }

    assert_eq!(
        distinct.len(),
        1,
        "{PROCESSES} processes gave {} different partitions for one document set:\n{}",
        distinct.len(),
        distinct
            .iter()
            .map(|(answer, count)| format!("--- seen {count} times ---\n{answer}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The placeholder goes to the first document, in input order, that references
/// it — the rule that makes the choice a function of the input.
#[test]
fn the_first_referencing_document_keeps_the_shared_placeholder() {
    let project = parse_lexicon_project(&shared_external_ref_docs()).expect("the fixture parses");
    let holders: Vec<String> = project
        .files
        .iter()
        .filter(|(_, schema)| {
            schema
                .vertices
                .keys()
                .any(|v| v.as_str() == "pub.layers.elsewhere#sharedAnchor")
        })
        .map(|(path, _)| path.display().to_string())
        .collect();

    assert_eq!(
        holders,
        vec!["alpha.json".to_string()],
        "exactly the first referencing document holds the placeholder"
    );
}
