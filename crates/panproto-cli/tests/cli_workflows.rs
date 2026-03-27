//! Integration tests for the `schema` CLI binary.
//!
//! Each test creates an isolated temporary directory and exercises one or more
//! CLI commands, asserting on exit codes and stdout/stderr content.

#![allow(clippy::unwrap_used)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn schema_cmd() -> Command {
    Command::cargo_bin("schema").unwrap()
}

fn init_repo(dir: &Path) {
    schema_cmd()
        .args(["init", dir.to_str().unwrap()])
        .current_dir(dir)
        .assert()
        .success();
}

fn write_schema(dir: &Path, name: &str, vertices: &[(&str, &str)]) {
    let mut verts = serde_json::Map::new();
    for (id, kind) in vertices {
        verts.insert(
            id.to_string(),
            serde_json::json!({
                "id": id, "kind": kind, "nsid": null
            }),
        );
    }
    let schema = serde_json::json!({
        "protocol": "test",
        "vertices": verts,
        "edges": [],
        "hyper_edges": {},
        "constraints": {},
        "required": {},
        "nsids": {},
        "variants": {},
        "orderings": [],
        "recursion_points": {},
        "spans": {},
        "usage_modes": [],
        "nominal": {},
        "outgoing": {},
        "incoming": {},
        "between": []
    });
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();
}

fn add_and_commit(dir: &Path, schema_file: &str, message: &str) {
    schema_cmd()
        .args(["add", schema_file])
        .current_dir(dir)
        .assert()
        .success();
    schema_cmd()
        .args(["commit", "-m", message])
        .current_dir(dir)
        .assert()
        .success();
}

/// Run a command and return its stdout as a `String`.
fn stdout_of(cmd: &mut Command) -> String {
    let output = cmd.output().unwrap();
    String::from_utf8(output.stdout).unwrap()
}

// ===========================================================================
// Group 1: Init & Status
// ===========================================================================

#[test]
fn cli_init_success() {
    let tmp = tempfile::tempdir().unwrap();
    schema_cmd()
        .args(["init", tmp.path().to_str().unwrap()])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized"));
}

#[test]
fn cli_init_with_initial_branch() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "seed");

    // Rename the default branch to "develop" (rename_branch requires a
    // ref to exist, which only happens after the first commit).
    schema_cmd()
        .args(["branch", "main", "-m", "develop"])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Status should reflect the new branch name.
    schema_cmd()
        .args(["status"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("develop"));
}

#[test]
fn cli_status_no_commits() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    schema_cmd()
        .args(["status"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("no commits yet"));
}

#[test]
fn cli_status_short() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    schema_cmd()
        .args(["status", "-s", "-b"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("## main"));
}

#[test]
fn cli_status_porcelain() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    schema_cmd()
        .args(["status", "--porcelain"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("## main"));
}

// ===========================================================================
// Group 2: Add & Commit
// ===========================================================================

#[test]
fn cli_add_commit_log() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object"), ("b", "string")]);

    add_and_commit(tmp.path(), "v1.json", "initial schema");

    schema_cmd()
        .args(["log"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("initial schema")
                .and(predicate::str::contains("Author:"))
                .and(predicate::str::contains("Date:")),
        );
}

#[test]
fn cli_add_dry_run() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);

    schema_cmd()
        .args(["add", "--dry-run", "v1.json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Would stage"));

    // Commit should fail because nothing was actually staged.
    schema_cmd()
        .args(["commit", "-m", "should fail"])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

#[test]
fn cli_commit_amend() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "original message");

    // Amend with a new message.
    schema_cmd()
        .args(["commit", "--amend", "-m", "amended message"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("amended"));

    // Log should show only the amended message.
    let log_out = stdout_of(
        schema_cmd()
            .args(["log", "--oneline"])
            .current_dir(tmp.path()),
    );
    assert!(log_out.contains("amended message"));
    // Should be only one commit.
    assert_eq!(log_out.trim().lines().count(), 1);
}

#[test]
fn cli_commit_no_staged_fails() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    schema_cmd()
        .args(["commit", "-m", "nothing staged"])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

#[test]
fn cli_add_unchanged_fails() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "first");

    // Adding the exact same schema again should fail.
    schema_cmd()
        .args(["add", "v1.json"])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

// ===========================================================================
// Group 3: Log Formatting
// ===========================================================================

#[test]
fn cli_log_default() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "first commit");

    schema_cmd()
        .args(["log"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Author:")
                .and(predicate::str::contains("Date:"))
                .and(predicate::str::contains("first commit")),
        );
}

#[test]
fn cli_log_oneline() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "one-liner");

    let out = stdout_of(
        schema_cmd()
            .args(["log", "--oneline"])
            .current_dir(tmp.path()),
    );
    let lines: Vec<&str> = out.trim().lines().collect();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("one-liner"));
}

#[test]
fn cli_log_limit() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "first");

    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    add_and_commit(tmp.path(), "v2.json", "second");

    let out = stdout_of(
        schema_cmd()
            .args(["log", "--oneline", "-n", "1"])
            .current_dir(tmp.path()),
    );
    let lines: Vec<&str> = out.trim().lines().collect();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("second"));
}

#[test]
fn cli_log_format() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "formatted");

    let out = stdout_of(
        schema_cmd()
            .args(["log", "--format", "%h %s"])
            .current_dir(tmp.path()),
    );
    let line = out.trim();
    // Should be "<7-char hash> formatted"
    assert!(line.contains("formatted"));
    let parts: Vec<&str> = line.splitn(2, ' ').collect();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].len(), 7); // short hash length
}

#[test]
fn cli_log_grep() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "first commit");

    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    add_and_commit(tmp.path(), "v2.json", "second fix");

    let out = stdout_of(
        schema_cmd()
            .args(["log", "--oneline", "--grep", "second"])
            .current_dir(tmp.path()),
    );
    let lines: Vec<&str> = out.trim().lines().collect();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("second fix"));
}

// ===========================================================================
// Group 4: Diff
// ===========================================================================

#[test]
fn cli_diff_two_files() {
    let tmp = tempfile::tempdir().unwrap();
    write_schema(tmp.path(), "old.json", &[("a", "object")]);
    write_schema(tmp.path(), "new.json", &[("a", "object"), ("c", "string")]);

    schema_cmd()
        .args(["diff", "old.json", "new.json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("change(s) detected"));
}

#[test]
fn cli_diff_stat() {
    let tmp = tempfile::tempdir().unwrap();
    write_schema(tmp.path(), "old.json", &[("a", "object")]);
    write_schema(tmp.path(), "new.json", &[("a", "object"), ("c", "string")]);

    schema_cmd()
        .args(["diff", "--stat", "old.json", "new.json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("addition(s)"));
}

#[test]
fn cli_diff_name_only() {
    let tmp = tempfile::tempdir().unwrap();
    write_schema(tmp.path(), "old.json", &[("a", "object")]);
    write_schema(tmp.path(), "new.json", &[("a", "object"), ("c", "string")]);

    schema_cmd()
        .args(["diff", "--name-only", "old.json", "new.json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("c"));
}

#[test]
fn cli_diff_staged() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    schema_cmd()
        .args(["add", "v2.json"])
        .current_dir(tmp.path())
        .assert()
        .success();

    schema_cmd()
        .args(["diff", "--staged"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("change(s) detected"));
}

// ===========================================================================
// Group 5: Branch & Tag
// ===========================================================================

#[test]
fn cli_branch_create_list() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    schema_cmd()
        .args(["branch", "feature"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Created branch feature"));

    schema_cmd()
        .args(["branch"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("feature").and(predicate::str::contains("main")));
}

#[test]
fn cli_branch_delete() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    schema_cmd()
        .args(["branch", "feature"])
        .current_dir(tmp.path())
        .assert()
        .success();

    schema_cmd()
        .args(["branch", "-d", "feature"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Deleted branch feature"));
}

#[test]
fn cli_branch_force_delete() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    schema_cmd()
        .args(["branch", "feature"])
        .current_dir(tmp.path())
        .assert()
        .success();

    schema_cmd()
        .args(["branch", "-D", "feature"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Deleted branch feature"));
}

#[test]
fn cli_branch_rename() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    schema_cmd()
        .args(["branch", "old-name"])
        .current_dir(tmp.path())
        .assert()
        .success();

    schema_cmd()
        .args(["branch", "old-name", "-m", "new-name"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Renamed branch old-name -> new-name",
        ));

    // Listing should show new-name, not old-name.
    let out = stdout_of(schema_cmd().args(["branch"]).current_dir(tmp.path()));
    assert!(out.contains("new-name"));
    assert!(!out.contains("old-name"));
}

#[test]
fn cli_tag_annotated() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    schema_cmd()
        .args(["tag", "-a", "v1.0", "-m", "release"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Tagged").and(predicate::str::contains("v1.0")));

    // Verify the tag appears in the tag list.
    schema_cmd()
        .args(["tag"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("v1.0"));
}

// ===========================================================================
// Group 6: Checkout
// ===========================================================================

#[test]
fn cli_checkout_branch() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    schema_cmd()
        .args(["branch", "feature"])
        .current_dir(tmp.path())
        .assert()
        .success();

    schema_cmd()
        .args(["checkout", "feature"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Switched to branch 'feature'"));

    schema_cmd()
        .args(["status"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("feature"));
}

#[test]
fn cli_checkout_create() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    schema_cmd()
        .args(["checkout", "-b", "new-feature"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Switched to a new branch 'new-feature'",
        ));

    schema_cmd()
        .args(["status"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("new-feature"));
}

#[test]
fn cli_checkout_detached() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "for detach");

    // Get the full commit hash via `log --format "%H"`.
    let log_out = stdout_of(
        schema_cmd()
            .args(["log", "--format", "%H"])
            .current_dir(tmp.path()),
    );
    let full_hash = log_out.trim();

    schema_cmd()
        .args(["checkout", "--detach", full_hash])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("HEAD is now at"));

    schema_cmd()
        .args(["status"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("detached"));
}

// ===========================================================================
// Group 7: Merge
// ===========================================================================

#[test]
fn cli_merge_fast_forward() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial on main");

    // Create a feature branch and add a commit there.
    schema_cmd()
        .args(["checkout", "-b", "feature"])
        .current_dir(tmp.path())
        .assert()
        .success();

    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    add_and_commit(tmp.path(), "v2.json", "feature work");

    // Switch back to main and merge.
    schema_cmd()
        .args(["checkout", "main"])
        .current_dir(tmp.path())
        .assert()
        .success();

    schema_cmd()
        .args(["merge", "feature"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Merge successful"));
}

#[test]
fn cli_merge_ff_only_fails() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    // Create feature branch with one commit.
    schema_cmd()
        .args(["checkout", "-b", "feature"])
        .current_dir(tmp.path())
        .assert()
        .success();
    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    add_and_commit(tmp.path(), "v2.json", "feature");

    // Go back to main and make a diverging commit.
    schema_cmd()
        .args(["checkout", "main"])
        .current_dir(tmp.path())
        .assert()
        .success();
    write_schema(tmp.path(), "v3.json", &[("a", "object"), ("c", "integer")]);
    add_and_commit(tmp.path(), "v3.json", "main diverge");

    // ff-only merge should fail because branches diverged.
    schema_cmd()
        .args(["merge", "--ff-only", "feature"])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

// ===========================================================================
// Group 8: Stash
// ===========================================================================

#[test]
fn cli_stash_push_list_pop() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    // Stage something to stash.
    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    schema_cmd()
        .args(["add", "v2.json"])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Stash push.
    schema_cmd()
        .args(["stash", "push", "-m", "wip changes"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Saved working state"));

    // Stash list.
    schema_cmd()
        .args(["stash", "list"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("stash@{0}"));

    // Stash pop.
    schema_cmd()
        .args(["stash", "pop"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Restored stash"));
}

#[test]
fn cli_stash_apply() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    schema_cmd()
        .args(["add", "v2.json"])
        .current_dir(tmp.path())
        .assert()
        .success();

    schema_cmd()
        .args(["stash", "push", "-m", "save"])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Apply (should preserve the stash entry).
    schema_cmd()
        .args(["stash", "apply"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Applied stash@{0}"));

    // Stash list should still show the entry.
    schema_cmd()
        .args(["stash", "list"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("stash@{0}"));
}

#[test]
fn cli_stash_clear() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    schema_cmd()
        .args(["add", "v2.json"])
        .current_dir(tmp.path())
        .assert()
        .success();

    schema_cmd()
        .args(["stash", "push", "-m", "will clear"])
        .current_dir(tmp.path())
        .assert()
        .success();

    schema_cmd()
        .args(["stash", "clear"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Cleared all stash entries"));
}

// ===========================================================================
// Group 9: GC & Blame
// ===========================================================================

#[test]
fn cli_gc_dry_run() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    schema_cmd()
        .args(["gc", "--dry-run"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Reachable objects:")
                .and(predicate::str::contains("Would delete:")),
        );
}

#[test]
fn cli_blame_vertex() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object"), ("b", "string")]);
    add_and_commit(tmp.path(), "v1.json", "added vertices");

    schema_cmd()
        .args(["blame", "--element-type", "vertex", "a"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("added vertices").and(predicate::str::contains("Date:")));
}

// ===========================================================================
// Group 10: Remote Stubs
// ===========================================================================

#[test]
fn cli_remote_stubs_complete() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    // remote list is not yet implemented (stored remotes).
    schema_cmd()
        .args(["remote", "list"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not yet implemented"));
}

#[test]
fn cli_push_requires_url() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    // push without a URL fails with a helpful message.
    schema_cmd()
        .args(["push"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("remote URL required"));
}

#[test]
fn cli_clone_requires_cospan_url() {
    let tmp = tempfile::tempdir().unwrap();

    // clone with a non-cospan:// URL fails.
    schema_cmd()
        .args(["clone", "https://example.com/repo"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("cospan://"));
}

// ===========================================================================
// Group 11: Schema Tool Commands (validate, check, lift)
// ===========================================================================

/// Write a schema JSON file with the given protocol name and vertices.
fn write_protocol_schema(dir: &Path, name: &str, protocol: &str, vertices: &[(&str, &str)]) {
    let mut verts = serde_json::Map::new();
    for (id, kind) in vertices {
        verts.insert(
            id.to_string(),
            serde_json::json!({
                "id": id, "kind": kind, "nsid": null
            }),
        );
    }
    let schema = serde_json::json!({
        "protocol": protocol,
        "vertices": verts,
        "edges": [],
        "hyper_edges": {},
        "constraints": {},
        "required": {},
        "nsids": {},
        "variants": {},
        "orderings": [],
        "recursion_points": {},
        "spans": {},
        "usage_modes": [],
        "nominal": {},
        "outgoing": {},
        "incoming": {},
        "between": []
    });
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();
}

/// Write a migration JSON file (vertex-only, no edge mappings).
fn write_migration(dir: &Path, name: &str, vertex_map: &[(&str, &str)]) {
    let vmap: serde_json::Map<String, serde_json::Value> = vertex_map
        .iter()
        .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
        .collect();
    let mig = serde_json::json!({
        "vertex_map": vmap,
        "edge_map": [],
        "hyper_edge_map": {},
        "label_map": [],
        "resolver": [],
        "hyper_resolver": []
    });
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(&mig).unwrap()).unwrap();
}

#[test]
fn cli_validate_valid_schema() {
    let tmp = tempfile::tempdir().unwrap();
    write_protocol_schema(
        tmp.path(),
        "schema.json",
        "atproto",
        &[("root", "object"), ("root.name", "string")],
    );

    schema_cmd()
        .args(["validate", "--protocol", "atproto", "schema.json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Schema is valid."));
}

#[test]
fn cli_validate_invalid_schema() {
    let tmp = tempfile::tempdir().unwrap();
    write_protocol_schema(tmp.path(), "bad.json", "atproto", &[("root", "bogus_kind")]);

    schema_cmd()
        .args(["validate", "--protocol", "atproto", "bad.json"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("error"));
}

#[test]
fn cli_check_valid_migration() {
    let tmp = tempfile::tempdir().unwrap();
    write_protocol_schema(
        tmp.path(),
        "src.json",
        "atproto",
        &[("a", "object"), ("b", "string")],
    );
    write_protocol_schema(
        tmp.path(),
        "tgt.json",
        "atproto",
        &[("a", "object"), ("b", "string"), ("c", "integer")],
    );
    write_migration(tmp.path(), "mig.json", &[("a", "a"), ("b", "b")]);

    schema_cmd()
        .args([
            "check",
            "--src",
            "src.json",
            "--tgt",
            "tgt.json",
            "--mapping",
            "mig.json",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Migration is valid."));
}

#[test]
fn cli_lift_identity() {
    let tmp = tempfile::tempdir().unwrap();

    // Use a single-vertex schema (string) for a simple identity lift.
    write_protocol_schema(tmp.path(), "src.json", "atproto", &[("root", "string")]);
    write_protocol_schema(tmp.path(), "tgt.json", "atproto", &[("root", "string")]);

    // Identity migration: root -> root, no edges.
    write_migration(tmp.path(), "mig.json", &[("root", "root")]);

    // Record: a simple string value.
    let record = serde_json::json!("Alice");
    std::fs::write(
        tmp.path().join("record.json"),
        serde_json::to_string_pretty(&record).unwrap(),
    )
    .unwrap();

    let output = schema_cmd()
        .args([
            "lift",
            "--migration",
            "mig.json",
            "--src-schema",
            "src.json",
            "--tgt-schema",
            "tgt.json",
            "record.json",
        ])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        output.status.success(),
        "lift command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("Alice"),
        "expected 'Alice' in output: {stdout}"
    );
}

#[test]
fn cli_diff_name_status() {
    let tmp = tempfile::tempdir().unwrap();
    write_schema(tmp.path(), "old.json", &[("a", "object")]);
    write_schema(tmp.path(), "new.json", &[("a", "object"), ("c", "string")]);

    let output = schema_cmd()
        .args(["diff", "--name-status", "old.json", "new.json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success());
    // Added vertex "c" should produce an "A" marker.
    assert!(
        stdout.contains("A\t"),
        "expected 'A\\t' marker in name-status output: {stdout}"
    );
}

// ===========================================================================
// Group 12: VCS Commands (show, rebase, cherry-pick, reset, bisect, reflog)
// ===========================================================================

#[test]
fn cli_show_commit() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial commit");

    schema_cmd()
        .args(["show", "HEAD"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("commit")
                .and(predicate::str::contains("Schema:"))
                .and(predicate::str::contains("Author:")),
        );
}

#[test]
fn cli_show_with_stat() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "first");

    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    add_and_commit(tmp.path(), "v2.json", "second");

    // Show HEAD --stat should include diff stats between the two commits.
    schema_cmd()
        .args(["show", "HEAD", "--stat"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("addition"));
}

#[test]
fn cli_rebase_success() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    // Seed commit on main.
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "seed on main");

    // Create feature branch with a commit.
    schema_cmd()
        .args(["checkout", "-b", "feature"])
        .current_dir(tmp.path())
        .assert()
        .success();
    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    add_and_commit(tmp.path(), "v2.json", "feature work");

    // Go back to main and add another commit (diverge).
    schema_cmd()
        .args(["checkout", "main"])
        .current_dir(tmp.path())
        .assert()
        .success();
    write_schema(tmp.path(), "v3.json", &[("a", "object"), ("c", "integer")]);
    add_and_commit(tmp.path(), "v3.json", "main advance");

    // Switch to feature and rebase onto main.
    schema_cmd()
        .args(["checkout", "feature"])
        .current_dir(tmp.path())
        .assert()
        .success();

    schema_cmd()
        .args(["rebase", "main"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Rebased onto"));
}

#[test]
fn cli_cherry_pick_success() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial on main");

    // Create feature branch with a commit.
    schema_cmd()
        .args(["checkout", "-b", "feature"])
        .current_dir(tmp.path())
        .assert()
        .success();
    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    add_and_commit(tmp.path(), "v2.json", "feature commit");

    // Get the feature commit hash.
    let log_out = stdout_of(
        schema_cmd()
            .args(["log", "--format", "%H"])
            .current_dir(tmp.path()),
    );
    let feature_hash = log_out.trim().lines().next().unwrap().trim();

    // Switch to main and cherry-pick.
    schema_cmd()
        .args(["checkout", "main"])
        .current_dir(tmp.path())
        .assert()
        .success();

    schema_cmd()
        .args(["cherry-pick", feature_hash])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Cherry-picked"));
}

#[test]
fn cli_cherry_pick_with_x() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    schema_cmd()
        .args(["checkout", "-b", "feature"])
        .current_dir(tmp.path())
        .assert()
        .success();
    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    add_and_commit(tmp.path(), "v2.json", "feature work");

    let log_out = stdout_of(
        schema_cmd()
            .args(["log", "--format", "%H"])
            .current_dir(tmp.path()),
    );
    let feature_hash = log_out.trim().lines().next().unwrap().trim();

    schema_cmd()
        .args(["checkout", "main"])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Cherry-pick with -x flag (record origin).
    schema_cmd()
        .args(["cherry-pick", "-x", feature_hash])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Cherry-picked"));
}

#[test]
fn cli_reset_soft_output() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "first");
    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    add_and_commit(tmp.path(), "v2.json", "second");

    // Get the first commit hash.
    let log_out = stdout_of(
        schema_cmd()
            .args(["log", "--format", "%H"])
            .current_dir(tmp.path()),
    );
    let first_hash = log_out.trim().lines().last().unwrap().trim();

    schema_cmd()
        .args(["reset", "--soft", first_hash])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("HEAD is now at").and(predicate::str::contains("soft")));
}

#[test]
fn cli_reset_hard_output() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "first");
    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    add_and_commit(tmp.path(), "v2.json", "second");

    let log_out = stdout_of(
        schema_cmd()
            .args(["log", "--format", "%H"])
            .current_dir(tmp.path()),
    );
    let first_hash = log_out.trim().lines().last().unwrap().trim();

    schema_cmd()
        .args(["reset", "--hard", first_hash])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("HEAD is now at").and(predicate::str::contains("hard")));
}

#[test]
fn cli_bisect_output() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    // Create 3 commits.
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "commit one");
    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    add_and_commit(tmp.path(), "v2.json", "commit two");
    write_schema(
        tmp.path(),
        "v3.json",
        &[("a", "object"), ("b", "string"), ("c", "integer")],
    );
    add_and_commit(tmp.path(), "v3.json", "commit three");

    // Get first and last commit hashes.
    let log_out = stdout_of(
        schema_cmd()
            .args(["log", "--format", "%H"])
            .current_dir(tmp.path()),
    );
    let hashes: Vec<&str> = log_out.trim().lines().map(str::trim).collect();
    let last = hashes.first().unwrap();
    let first = hashes.last().unwrap();

    let output = schema_cmd()
        .args(["bisect", first, last])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        output.status.success(),
        "bisect failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Should contain either "Breaking commit" or "Test commit".
    assert!(
        stdout.contains("Breaking commit") || stdout.contains("Test commit"),
        "expected bisect output, got: {stdout}"
    );
}

#[test]
fn cli_reflog_shows_history() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "first");
    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    add_and_commit(tmp.path(), "v2.json", "second");

    let output = schema_cmd()
        .args(["reflog", "HEAD"])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success());
    // Should show at least one reflog entry with HEAD@{0}.
    assert!(
        stdout.contains("HEAD@{0}") || stdout.contains("->"),
        "expected reflog entries, got: {stdout}"
    );
}

#[test]
fn cli_reflog_with_all() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    // --all should not error even with minimal history.
    schema_cmd()
        .args(["reflog", "--all"])
        .current_dir(tmp.path())
        .assert()
        .success();
}

// ===========================================================================
// Group 13: Flag Coverage
// ===========================================================================

#[test]
fn cli_log_author_filter() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    // Commit with a specific author.
    schema_cmd()
        .args(["add", "v1.json"])
        .current_dir(tmp.path())
        .assert()
        .success();
    schema_cmd()
        .args(["commit", "-m", "by alice", "--author", "alice"])
        .current_dir(tmp.path())
        .assert()
        .success();

    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    schema_cmd()
        .args(["add", "v2.json"])
        .current_dir(tmp.path())
        .assert()
        .success();
    schema_cmd()
        .args(["commit", "-m", "by bob", "--author", "bob"])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Filter by author "alice".
    let out = stdout_of(
        schema_cmd()
            .args(["log", "--oneline", "--author", "alice"])
            .current_dir(tmp.path()),
    );
    assert!(out.contains("by alice"), "expected alice's commit: {out}");
    assert!(
        !out.contains("by bob"),
        "should not contain bob's commit: {out}"
    );
}

#[test]
fn cli_branch_verbose() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    schema_cmd()
        .args(["branch", "feature"])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Verbose branch listing should show commit hash.
    let out = stdout_of(schema_cmd().args(["branch", "-v"]).current_dir(tmp.path()));
    assert!(
        out.contains("main"),
        "expected 'main' in branch list: {out}"
    );
    assert!(
        out.contains("feature"),
        "expected 'feature' in branch list: {out}"
    );
    // Verbose mode includes the commit message after the hash.
    assert!(
        out.contains("initial"),
        "expected commit message in verbose output: {out}"
    );
}

#[test]
fn cli_merge_no_commit() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial on main");

    // Create feature branch with a commit.
    schema_cmd()
        .args(["checkout", "-b", "feature"])
        .current_dir(tmp.path())
        .assert()
        .success();
    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    add_and_commit(tmp.path(), "v2.json", "feature work");

    // Switch to main and merge with --no-commit.
    schema_cmd()
        .args(["checkout", "main"])
        .current_dir(tmp.path())
        .assert()
        .success();

    schema_cmd()
        .args(["merge", "--no-commit", "feature"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Merge successful"));
}

#[test]
fn cli_merge_abort() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    // Merge --abort should handle gracefully even if no merge in progress.
    schema_cmd()
        .args(["merge", "--abort"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Merge aborted"));
}

#[test]
fn cli_commit_allow_empty() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    // --allow-empty is currently a placeholder; verify it doesn't crash.
    // (It will still fail because nothing is staged, but shouldn't panic.)
    let output = schema_cmd()
        .args(["commit", "--allow-empty", "-m", "empty commit"])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    // Either succeeds or fails gracefully (no panic).
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            || stderr.contains("failed to commit")
            || stderr.contains("nothing"),
        "expected graceful behavior, got stderr: {stderr}"
    );
}

#[test]
fn cli_stash_show() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    // Stage something and push to stash.
    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    schema_cmd()
        .args(["add", "v2.json"])
        .current_dir(tmp.path())
        .assert()
        .success();
    schema_cmd()
        .args(["stash", "push", "-m", "wip"])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Show stash entry.
    schema_cmd()
        .args(["stash", "show"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("stash@{0}"));
}

#[test]
fn cli_stash_drop() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);
    add_and_commit(tmp.path(), "v1.json", "initial");

    // Stage and stash.
    write_schema(tmp.path(), "v2.json", &[("a", "object"), ("b", "string")]);
    schema_cmd()
        .args(["add", "v2.json"])
        .current_dir(tmp.path())
        .assert()
        .success();
    schema_cmd()
        .args(["stash", "push", "-m", "will drop"])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Drop stash.
    schema_cmd()
        .args(["stash", "drop"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Dropped stash@{0}"));
}

#[test]
fn cli_pull_fetch_require_url() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());

    // Pull without URL.
    schema_cmd()
        .args(["pull"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("remote URL required"));

    // Fetch without URL.
    schema_cmd()
        .args(["fetch"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("remote URL required"));
}

// ===========================================================================
// Group 12: Data Lifting through Schema Migrations (CLI)
// ===========================================================================

// NOTE: Schema and Migration types use HashMap<Edge, _> and HashMap<(String,String), _>
// as map keys, which serde_json cannot serialize/deserialize. Therefore CLI lift tests
// are limited to leaf-value schemas (no edges). For full structural lifting tests with
// field-level add/drop/rename, see the library-level tests in
// crates/panproto-cli/tests/cli_workflows.rs (lift_api_* tests below) which exercise
// the same compile + parse_json + lift_wtype + to_json pipeline directly.

/// Lift a string value through an identity migration (leaf schema, no edges).
/// Verifies the basic lift pipeline works end-to-end through the CLI.
#[test]
fn cli_lift_string_identity() {
    let tmp = tempfile::tempdir().unwrap();
    write_protocol_schema(tmp.path(), "src.json", "atproto", &[("root", "string")]);
    write_protocol_schema(tmp.path(), "tgt.json", "atproto", &[("root", "string")]);
    write_migration(tmp.path(), "mig.json", &[("root", "root")]);

    let record = serde_json::json!("Hello, world!");
    std::fs::write(
        tmp.path().join("record.json"),
        serde_json::to_string_pretty(&record).unwrap(),
    )
    .unwrap();

    schema_cmd()
        .args([
            "lift",
            "--migration",
            "mig.json",
            "--src-schema",
            "src.json",
            "--tgt-schema",
            "tgt.json",
            "record.json",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello, world!"));
}

/// Lift an integer value through an identity migration.
#[test]
fn cli_lift_integer_identity() {
    let tmp = tempfile::tempdir().unwrap();
    write_protocol_schema(tmp.path(), "src.json", "atproto", &[("root", "integer")]);
    write_protocol_schema(tmp.path(), "tgt.json", "atproto", &[("root", "integer")]);
    write_migration(tmp.path(), "mig.json", &[("root", "root")]);

    std::fs::write(tmp.path().join("record.json"), "42").unwrap();

    schema_cmd()
        .args([
            "lift",
            "--migration",
            "mig.json",
            "--src-schema",
            "src.json",
            "--tgt-schema",
            "tgt.json",
            "record.json",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("42"));
}

/// Lift with --verbose flag: verify diagnostic output on stderr.
#[test]
fn cli_lift_verbose() {
    let tmp = tempfile::tempdir().unwrap();
    write_protocol_schema(tmp.path(), "src.json", "atproto", &[("root", "string")]);
    write_protocol_schema(tmp.path(), "tgt.json", "atproto", &[("root", "string")]);
    write_migration(tmp.path(), "mig.json", &[("root", "root")]);

    std::fs::write(
        tmp.path().join("record.json"),
        serde_json::to_string_pretty(&serde_json::json!("Alice")).unwrap(),
    )
    .unwrap();

    let output = schema_cmd()
        .args([
            "--verbose",
            "lift",
            "--migration",
            "mig.json",
            "--src-schema",
            "src.json",
            "--tgt-schema",
            "tgt.json",
            "record.json",
        ])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "verbose lift should succeed");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("vertex mappings"),
        "stderr should mention vertex mappings, got: {stderr}"
    );
    assert!(
        stderr.contains("nodes") && stderr.contains("arcs"),
        "stderr should mention node and arc counts, got: {stderr}"
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Alice"));
}

/// Lift fails when migration references a vertex not in the target schema.
#[test]
fn cli_lift_bad_migration_fails() {
    let tmp = tempfile::tempdir().unwrap();
    write_protocol_schema(tmp.path(), "src.json", "atproto", &[("root", "string")]);
    write_protocol_schema(tmp.path(), "tgt.json", "atproto", &[("root", "string")]);
    // Migration maps root to "nonexistent" — not present in target schema.
    write_migration(tmp.path(), "mig.json", &[("root", "nonexistent")]);

    std::fs::write(tmp.path().join("record.json"), "\"test\"").unwrap();

    schema_cmd()
        .args([
            "lift",
            "--migration",
            "mig.json",
            "--src-schema",
            "src.json",
            "--tgt-schema",
            "tgt.json",
            "record.json",
        ])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

// ===========================================================================
// Group 13: Structural Lifting via Library API
// ===========================================================================
//
// These tests exercise the full lift pipeline (compile + parse_json +
// lift_wtype + to_json) directly through the Rust API, bypassing the
// JSON serialization limitation for schemas with edges.

use panproto_core::gat::Name;
use panproto_core::inst;
use panproto_core::mig;
use panproto_core::schema::{Edge, Schema, Vertex};
use smallvec::SmallVec;
use std::collections::HashMap;

/// Build a schema with named prop edges and all required adjacency indices.
fn make_lift_schema(
    vertices: &[(&str, &str)],
    edges: &[(&str, &str, &str, &str)], // (src, tgt, kind, name)
) -> Schema {
    let mut vert_map = HashMap::new();
    for (id, kind) in vertices {
        vert_map.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
                nsid: None,
            },
        );
    }

    let mut edge_map = HashMap::new();
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

    for (src, tgt, kind, name) in edges {
        let edge = Edge {
            src: (*src).into(),
            tgt: (*tgt).into(),
            kind: (*kind).into(),
            name: Some((*name).into()),
        };
        edge_map.insert(edge.clone(), Name::from(*kind));
        outgoing
            .entry(Name::from(*src))
            .or_default()
            .push(edge.clone());
        incoming
            .entry(Name::from(*tgt))
            .or_default()
            .push(edge.clone());
        between
            .entry((Name::from(*src), Name::from(*tgt)))
            .or_default()
            .push(edge);
    }

    Schema {
        protocol: "test".into(),
        vertices: vert_map,
        edges: edge_map,
        hyper_edges: HashMap::new(),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        coercions: HashMap::new(),
        mergers: HashMap::new(),
        defaults: HashMap::new(),
        policies: HashMap::new(),
        outgoing,
        incoming,
        between,
    }
}

fn make_migration(
    vertex_map: &[(&str, &str)],
    edge_map_entries: &[(Edge, Edge)],
) -> mig::Migration {
    mig::Migration {
        vertex_map: vertex_map
            .iter()
            .map(|(k, v)| (Name::from(*k), Name::from(*v)))
            .collect(),
        edge_map: edge_map_entries.iter().cloned().collect(),
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        expr_resolvers: HashMap::new(),
    }
}

/// Add-field migration: source has "name", target adds "email".
///
/// The migration maps root->root, root.name->root.name. The "email"
/// field is new and absent from the migration. The lifted record should
/// contain "name" = "Alice" but no "email".
#[test]
fn lift_api_add_field() {
    let src_schema = make_lift_schema(
        &[("root", "object"), ("root.name", "string")],
        &[("root", "root.name", "prop", "name")],
    );
    let tgt_schema = make_lift_schema(
        &[
            ("root", "object"),
            ("root.name", "string"),
            ("root.email", "string"),
        ],
        &[
            ("root", "root.name", "prop", "name"),
            ("root", "root.email", "prop", "email"),
        ],
    );

    let name_edge = Edge {
        src: "root".into(),
        tgt: "root.name".into(),
        kind: "prop".into(),
        name: Some("name".into()),
    };
    let migration = make_migration(
        &[("root", "root"), ("root.name", "root.name")],
        &[(name_edge.clone(), name_edge)],
    );

    let compiled = mig::compile(&src_schema, &tgt_schema, &migration).unwrap();
    let record = serde_json::json!({"name": "Alice"});
    let instance = inst::parse_json(&src_schema, "root", &record).unwrap();
    let lifted = mig::lift_wtype(&compiled, &src_schema, &tgt_schema, &instance).unwrap();
    let output = inst::to_json(&tgt_schema, &lifted);

    assert_eq!(output["name"], "Alice", "name should be preserved");
    assert!(
        output.get("email").is_none() || output["email"].is_null(),
        "email should be absent or null in lifted output"
    );
}

/// Drop-field migration: source has "name" and "age", target has only "name".
///
/// The migration maps root->root, root.name->root.name. The "age" field
/// is dropped. The lifted record should contain "name" but NOT "age".
#[test]
fn lift_api_drop_field() {
    let src_schema = make_lift_schema(
        &[
            ("root", "object"),
            ("root.name", "string"),
            ("root.age", "integer"),
        ],
        &[
            ("root", "root.name", "prop", "name"),
            ("root", "root.age", "prop", "age"),
        ],
    );
    let tgt_schema = make_lift_schema(
        &[("root", "object"), ("root.name", "string")],
        &[("root", "root.name", "prop", "name")],
    );

    let name_edge = Edge {
        src: "root".into(),
        tgt: "root.name".into(),
        kind: "prop".into(),
        name: Some("name".into()),
    };
    let migration = make_migration(
        &[("root", "root"), ("root.name", "root.name")],
        &[(name_edge.clone(), name_edge)],
    );

    let compiled = mig::compile(&src_schema, &tgt_schema, &migration).unwrap();
    let record = serde_json::json!({"name": "Bob", "age": 30});
    let instance = inst::parse_json(&src_schema, "root", &record).unwrap();
    let lifted = mig::lift_wtype(&compiled, &src_schema, &tgt_schema, &instance).unwrap();
    let output = inst::to_json(&tgt_schema, &lifted);

    assert_eq!(output["name"], "Bob", "name should be preserved");
    assert!(
        output.get("age").is_none(),
        "age should be absent from lifted output, got: {output}"
    );
}

/// Identity migration with matching source and target schemas.
///
/// All fields survive when the migration maps every vertex to itself.
#[test]
fn lift_api_identity_all_fields_survive() {
    let schema = make_lift_schema(
        &[
            ("root", "object"),
            ("root.name", "string"),
            ("root.email", "string"),
        ],
        &[
            ("root", "root.name", "prop", "name"),
            ("root", "root.email", "prop", "email"),
        ],
    );

    let name_edge = Edge {
        src: "root".into(),
        tgt: "root.name".into(),
        kind: "prop".into(),
        name: Some("name".into()),
    };
    let email_edge = Edge {
        src: "root".into(),
        tgt: "root.email".into(),
        kind: "prop".into(),
        name: Some("email".into()),
    };
    let migration = make_migration(
        &[
            ("root", "root"),
            ("root.name", "root.name"),
            ("root.email", "root.email"),
        ],
        &[
            (name_edge.clone(), name_edge),
            (email_edge.clone(), email_edge),
        ],
    );

    let compiled = mig::compile(&schema, &schema, &migration).unwrap();
    let record = serde_json::json!({"name": "Eve", "email": "eve@example.com"});
    let instance = inst::parse_json(&schema, "root", &record).unwrap();
    let lifted = mig::lift_wtype(&compiled, &schema, &schema, &instance).unwrap();
    let output = inst::to_json(&schema, &lifted);

    assert_eq!(output["name"], "Eve");
    assert_eq!(output["email"], "eve@example.com");
}

/// Multi-field lift: two fields survive, one is dropped.
#[test]
fn lift_api_multi_field_projection() {
    let src_schema = make_lift_schema(
        &[
            ("root", "object"),
            ("root.name", "string"),
            ("root.email", "string"),
            ("root.age", "integer"),
        ],
        &[
            ("root", "root.name", "prop", "name"),
            ("root", "root.email", "prop", "email"),
            ("root", "root.age", "prop", "age"),
        ],
    );
    let tgt_schema = make_lift_schema(
        &[
            ("root", "object"),
            ("root.name", "string"),
            ("root.email", "string"),
        ],
        &[
            ("root", "root.name", "prop", "name"),
            ("root", "root.email", "prop", "email"),
        ],
    );

    let name_edge = Edge {
        src: "root".into(),
        tgt: "root.name".into(),
        kind: "prop".into(),
        name: Some("name".into()),
    };
    let email_edge = Edge {
        src: "root".into(),
        tgt: "root.email".into(),
        kind: "prop".into(),
        name: Some("email".into()),
    };
    let migration = make_migration(
        &[
            ("root", "root"),
            ("root.name", "root.name"),
            ("root.email", "root.email"),
        ],
        &[
            (name_edge.clone(), name_edge),
            (email_edge.clone(), email_edge),
        ],
    );

    let compiled = mig::compile(&src_schema, &tgt_schema, &migration).unwrap();
    let record = serde_json::json!({"name": "Dana", "email": "dana@example.com", "age": 25});
    let instance = inst::parse_json(&src_schema, "root", &record).unwrap();
    let lifted = mig::lift_wtype(&compiled, &src_schema, &tgt_schema, &instance).unwrap();
    let output = inst::to_json(&tgt_schema, &lifted);

    assert_eq!(output["name"], "Dana");
    assert_eq!(output["email"], "dana@example.com");
    assert!(
        output.get("age").is_none(),
        "age should be absent from lifted output, got: {output}"
    );
}

/// Lift preserves boolean and null values correctly.
#[test]
fn lift_api_preserves_value_types() {
    let src_schema = make_lift_schema(
        &[
            ("root", "object"),
            ("root.active", "boolean"),
            ("root.name", "string"),
        ],
        &[
            ("root", "root.active", "prop", "active"),
            ("root", "root.name", "prop", "name"),
        ],
    );
    let tgt_schema = make_lift_schema(
        &[("root", "object"), ("root.active", "boolean")],
        &[("root", "root.active", "prop", "active")],
    );

    let active_edge = Edge {
        src: "root".into(),
        tgt: "root.active".into(),
        kind: "prop".into(),
        name: Some("active".into()),
    };
    let migration = make_migration(
        &[("root", "root"), ("root.active", "root.active")],
        &[(active_edge.clone(), active_edge)],
    );

    let compiled = mig::compile(&src_schema, &tgt_schema, &migration).unwrap();
    let record = serde_json::json!({"active": true, "name": "test"});
    let instance = inst::parse_json(&src_schema, "root", &record).unwrap();
    let lifted = mig::lift_wtype(&compiled, &src_schema, &tgt_schema, &instance).unwrap();
    let output = inst::to_json(&tgt_schema, &lifted);

    assert_eq!(output["active"], true, "boolean value should be preserved");
    assert!(
        output.get("name").is_none(),
        "dropped field should be absent, got: {output}"
    );
}
