//! Smoke test for the panproto REPL driver.
//!
//! Feeds a scripted sequence of commands via [`Repl::handle_line`] and
//! verifies the outcomes, avoiding any terminal interaction.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use panproto_repl::{Repl, ReplOutcome};

fn nat_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("panproto-theory-dsl")
        .join("tests")
        .join("fixtures")
        .join("nat_inductive.json")
}

fn unwrap_output(outcome: ReplOutcome) -> String {
    match outcome {
        ReplOutcome::Output(s) => s,
        ReplOutcome::Error(s) => panic!("expected output, got error: {s}"),
        ReplOutcome::Quit => panic!("unexpected quit"),
    }
}

#[test]
fn load_use_type_and_normalize_nat() {
    let mut repl = Repl::new();
    let path = nat_fixture();
    let out = unwrap_output(repl.handle_line(&format!(":load {}", path.display())));
    assert!(out.contains("Nat"), "load message should name Nat: {out}");

    let out = unwrap_output(repl.handle_line(":use Nat"));
    assert!(out.contains("Nat"), "use output: {out}");

    let out = unwrap_output(repl.handle_line(":sorts"));
    assert!(out.contains("Nat"), "sorts output: {out}");

    let out = unwrap_output(repl.handle_line(":ops"));
    assert!(out.contains("zero"), "ops should list zero: {out}");
    assert!(out.contains("succ"), "ops should list succ: {out}");

    let out = unwrap_output(repl.handle_line(":type zero()"));
    assert!(out.contains("Nat"), ":type output: {out}");

    // Bare-input form also typechecks.
    let out = unwrap_output(repl.handle_line("succ(zero())"));
    assert!(out.contains("Nat"), "bare typecheck output: {out}");

    let out = unwrap_output(repl.handle_line(":normalize succ(zero())"));
    assert!(out.contains("succ(zero())"), ":normalize output: {out}");

    assert!(matches!(repl.handle_line(":quit"), ReplOutcome::Quit));
}

#[test]
fn type_in_empty_repl_errors_with_message() {
    let mut repl = Repl::new();
    match repl.handle_line(":type zero()") {
        ReplOutcome::Error(msg) => assert!(msg.contains("no active theory")),
        other => panic!("expected error, got {other:?}"),
    }
}
