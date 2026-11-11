//! A schema's and a lens's identity is the same in every process.
//!
//! An identity is only durable if another process, on another day, computes
//! the same bytes from the same schema and lens. Schemas and compiled
//! migrations are built from `HashMap`s and `HashSet`s whose enumeration
//! order the per-process hash seed decides; any digest that let that order
//! through would differ between processes, and every stored complement
//! would then fail its identity check on the next run.
//!
//! Each of 32 child processes computes every identity in the corpus
//! (`tests/fixtures/identity/golden.json`): parsing the fixture schemas,
//! compiling the migrations, and running `auto_generate`, all under its own
//! hash seed. The parent asserts that the children agree with each other
//! and with the golden file. Run a child directly with
//! `PP_IDENTITY_DUMP=1` to see one process's answer.

#![allow(
    clippy::expect_used,
    reason = "a corpus entry that will not build is a defect in the fixture"
)]

mod identity_corpus;

use std::collections::BTreeMap;
use std::process::Command;

use identity_corpus::{every_identity, read_golden};

/// How many separate processes the answer is compared across.
const PROCESSES: usize = 32;

fn answer() -> String {
    every_identity(&read_golden())
        .into_iter()
        .map(|(name, identity)| format!("{name}={identity}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The child half: one process's identities, printed.
#[test]
fn one_process_answer() {
    if std::env::var_os("PP_IDENTITY_DUMP").is_none() {
        // The parent below runs this with the variable set.
        return;
    }
    print!("<<<{}>>>", answer());
}

fn child_answer(exe: &std::path::Path) -> String {
    let output = Command::new(exe)
        .args(["one_process_answer", "--exact", "--nocapture"])
        .env("PP_IDENTITY_DUMP", "1")
        .output()
        .expect("the test binary re-runs");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let start = text.find("<<<").expect("the child printed its answer");
    let end = text.find(">>>").expect("the child closed its answer");
    text[start + 3..end].to_owned()
}

#[test]
fn every_identity_is_the_same_in_every_process() {
    if std::env::var_os("PP_IDENTITY_DUMP").is_some() {
        return;
    }
    let exe = std::env::current_exe().expect("the test binary knows its own path");
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for _ in 0..PROCESSES {
        *seen.entry(child_answer(&exe)).or_insert(0) += 1;
    }
    assert_eq!(
        seen.len(),
        1,
        "{PROCESSES} processes computed {} different sets of identities",
        seen.len()
    );

    // And they are the identities the golden file records.
    let golden = read_golden();
    let mut expected: Vec<String> = golden
        .schemas
        .iter()
        .map(|e| format!("schema {}={}", e.name, e.digest))
        .collect();
    expected.extend(golden.lenses.iter().map(|e| {
        format!(
            "lens {}={}",
            e.name,
            serde_json::to_string(&e.envelope).expect("an envelope serializes")
        )
    }));
    let children = seen.into_keys().next().expect("one answer");
    assert_eq!(children, expected.join("\n"));
}
