//! The span a corpus pair yields is a function of the two schemas.
//!
//! The apex digest is canonical and sorts everything it reads, so pinning it
//! proves nothing about the rest of the span. What is not canonicalised is the
//! right leg's **edge map**: `edge_image` takes the first target edge of the
//! source edge's kind out of `Schema::edges_between`, and that slice's order
//! is whatever built the schema's `between` index. For a schema the builder or
//! the lexicon parser produced that order is insertion order, and this is the
//! test that says so: sixteen separate processes, each with its own hash seed,
//! over all 5852 ordered pairs, compared on the whole span rather than on the
//! digest.
//!
//! It is a companion to
//! `panproto-vcs/tests/span_is_the_same_in_every_process.rs`, which runs the
//! same comparison on a schema whose index was rebuilt from a
//! `HashMap` and gets a different answer.

#![allow(
    clippy::expect_used,
    reason = "a malformed committed fixture should fail the test loudly"
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::process::Command;

use panproto_mig::hom_search::{SearchOptions, find_span};

#[path = "support/lexicons.rs"]
mod lexicons;

/// How many separate processes the comparison runs.
///
/// Each starts with its own `RandomState` seed, which is the whole point: a
/// span that varied with the seed would show up as more than one digest here.
const RUNS: usize = 16;

/// The same corpus pair searched in separate processes.

#[test]
fn every_corpus_pair_yields_the_same_span_in_every_process() {
    if std::env::var_os("PP_CORPUS_DUMP").is_some() {
        let corpus = lexicons::corpus();
        let protocol = panproto_protocols::atproto::protocol();
        let mut out = String::new();
        for (i, left) in corpus.iter().enumerate() {
            for (j, right) in corpus.iter().enumerate() {
                if i == j {
                    continue;
                }
                let span = find_span(
                    &left.schema,
                    &right.schema,
                    &protocol,
                    &SearchOptions::default(),
                )
                .expect("poses");
                let mut edges: Vec<String> = span
                    .right
                    .edge_map
                    .iter()
                    .map(|(k, v)| format!("{k:?}|{v:?}"))
                    .collect();
                edges.sort_unstable();
                let mut verts: Vec<String> = span
                    .right
                    .vertex_map
                    .iter()
                    .map(|(k, v)| format!("{k}|{v}"))
                    .collect();
                verts.sort_unstable();
                writeln!(
                    out,
                    "{}->{} q={:.12} c={:.12} v={verts:?} e={edges:?}",
                    left.nsid, right.nsid, span.quality, span.apex_coverage
                )
                .expect("writing to a String cannot fail");
            }
        }
        print!("<<<{}>>>", blake3::hash(out.as_bytes()).to_hex());
        // The full dump goes to a per-process file so a mismatch can be diffed.
        let path = std::env::var("PP_CORPUS_DUMP").expect("the branch tested it");
        if path != "1" {
            std::fs::write(path, out).expect("dump written");
        }
        return;
    }

    let exe = std::env::current_exe().expect("own path");
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for run in 0..RUNS {
        let dir = std::env::temp_dir().join(format!("pp-corpus-{run}.txt"));
        let output = Command::new(&exe)
            .args([
                "every_corpus_pair_yields_the_same_span_in_every_process",
                "--exact",
                "--nocapture",
            ])
            .env("PP_CORPUS_DUMP", &dir)
            .output()
            .expect("child runs");
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        let start = text.find("<<<").expect("child printed");
        let end = text.find(">>>").expect("child closed");
        *seen.entry(text[start + 3..end].to_owned()).or_insert(0) += 1;
    }
    println!("corpus cross-process: {RUNS} processes, digests {seen:?}");
    assert_eq!(
        seen.len(),
        1,
        "the corpus span is not a function of the pair"
    );
}
