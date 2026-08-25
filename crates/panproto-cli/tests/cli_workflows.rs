//! Integration tests for the `schema` CLI binary.
//!
//! Each test creates an isolated temporary directory and exercises one or more
//! CLI commands, asserting on exit codes and stdout/stderr content.

#![allow(clippy::unwrap_used)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::collections::HashMap;
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

fn write_atproto_project(dir: &Path, target_required: bool) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("panproto.toml"),
        r#"[workspace]
name = "lexicons"

[[package]]
name = "lexicons"
path = "."
protocol = "atproto"
"#,
    )
    .unwrap();
    let mut annotation = serde_json::json!({
        "lexicon": 1,
        "id": "pub.example.annotation",
        "defs": {
            "main": {
                "type": "record",
                "record": {
                    "type": "object",
                    "properties": {
                        "target": {
                            "type": "ref",
                            "ref": "pub.example.defs#target"
                        }
                    }
                }
            }
        }
    });
    if target_required {
        annotation["defs"]["main"]["record"]["required"] = serde_json::json!(["target"]);
    }
    std::fs::write(
        dir.join("annotation.json"),
        serde_json::to_vec_pretty(&annotation).unwrap(),
    )
    .unwrap();
    std::fs::write(
        dir.join("defs.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "lexicon": 1,
            "id": "pub.example.defs",
            "defs": {
                "target": {
                    "type": "object",
                    "properties": {
                        "value": {"type": "string"}
                    }
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();
}

fn staged_leaf_ids(
    dir: &Path,
) -> (
    panproto_core::vcs::ObjectId,
    HashMap<String, panproto_core::vcs::ObjectId>,
) {
    use panproto_core::vcs::{Repository, hash::hash_file_schema, walk_tree};

    let repo = Repository::open(dir).unwrap();
    let root = repo.read_index().unwrap().staged.unwrap().schema_id;
    let mut leaves = HashMap::new();
    walk_tree(repo.store(), &root, |path, file| {
        leaves.insert(
            path.to_string_lossy().into_owned(),
            hash_file_schema(file).unwrap(),
        );
        Ok(())
    })
    .unwrap();
    (root, leaves)
}

/// Read the staged data entries from the index as `(source path, data id)`.
fn staged_data_entries(dir: &Path) -> Vec<(String, panproto_core::vcs::ObjectId)> {
    panproto_core::vcs::Repository::open(dir)
        .unwrap()
        .read_index()
        .unwrap()
        .staged_data
        .into_iter()
        .map(|staged| {
            (
                staged.source_path.to_string_lossy().into_owned(),
                staged.data_id,
            )
        })
        .collect()
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
fn cli_add_atproto_directory_stages_per_file_tree_and_reuses_unchanged_leaf() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    let project = tmp.path().join("lexicons");
    write_atproto_project(&project, false);

    schema_cmd()
        .args(["add", "--skip-verify", "lexicons"])
        .current_dir(tmp.path())
        .assert()
        .success();
    let (first_root, first_leaves) = staged_leaf_ids(tmp.path());
    assert_eq!(
        first_leaves.len(),
        2,
        "the manifest and directory must not be flattened into the schema"
    );

    schema_cmd()
        .args(["commit", "-m", "first"])
        .current_dir(tmp.path())
        .assert()
        .success();
    write_atproto_project(&project, true);

    schema_cmd()
        .args(["add", "--skip-verify", "lexicons"])
        .current_dir(tmp.path())
        .assert()
        .success();
    let (second_root, second_leaves) = staged_leaf_ids(tmp.path());
    assert_ne!(
        first_root, second_root,
        "changing one file must change the project root"
    );
    assert_eq!(
        first_leaves.get("defs.json"),
        second_leaves.get("defs.json"),
        "the unchanged file must retain its object id"
    );
    assert_ne!(
        first_leaves.get("annotation.json"),
        second_leaves.get("annotation.json"),
        "the changed file must receive a new object id"
    );
}

#[test]
fn cli_add_data_stages_one_index_entry_per_file() {
    use panproto_core::vcs::{Object, Repository, Store as _};

    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);

    let data_dir = tmp.path().join("records");
    std::fs::create_dir(&data_dir).unwrap();
    let first = br#"[{"a": 1}, {"a": 2}]"#;
    let second = br#"[{"a": 3}]"#;
    std::fs::write(data_dir.join("one.json"), first).unwrap();
    std::fs::write(data_dir.join("two.json"), second).unwrap();
    // A non-JSON sibling must not be staged.
    std::fs::write(data_dir.join("notes.txt"), b"ignored").unwrap();

    schema_cmd()
        .args(["add", "v1.json", "--data", "records"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Staged 2 data file(s)"));

    let staged = staged_data_entries(tmp.path());
    assert_eq!(
        staged.len(),
        2,
        "the index must hold one entry per data file, not just a printed count"
    );
    let paths: Vec<&str> = staged.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(paths, vec!["records/one.json", "records/two.json"]);

    // Each entry points at a stored data set holding that file's bytes.
    let repo = Repository::open(tmp.path()).unwrap();
    for ((_, data_id), expected) in staged.iter().zip([first.as_slice(), second.as_slice()]) {
        match repo.store().get(data_id).unwrap() {
            Object::DataSet(set) => assert_eq!(set.data, expected),
            other => panic!("expected a data set, found {}", other.type_name()),
        }
    }

    // Committing carries the sets forward, keyed by their source paths.
    schema_cmd()
        .args(["commit", "-m", "schema and data"])
        .current_dir(tmp.path())
        .assert()
        .success();
    let repo = Repository::open(tmp.path()).unwrap();
    let mut keys: Vec<String> = repo
        .data_at("HEAD")
        .unwrap()
        .into_iter()
        .filter_map(|set| set.key)
        .collect();
    keys.sort();
    assert_eq!(keys, vec!["records/one.json", "records/two.json"]);
    assert!(staged_data_entries(tmp.path()).is_empty());
}

#[test]
fn cli_add_data_failure_stages_nothing_and_reports_it() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    write_schema(tmp.path(), "v1.json", &[("a", "object")]);

    let data_dir = tmp.path().join("records");
    std::fs::create_dir(&data_dir).unwrap();
    std::fs::write(data_dir.join("good.json"), br#"[{"a": 1}]"#).unwrap();
    // A directory carrying a .json name is discovered as a data file and
    // then fails to read, which is the failure the staging loop must not
    // report as success.
    std::fs::create_dir(data_dir.join("unreadable.json")).unwrap();

    let assertion = schema_cmd()
        .args(["add", "v1.json", "--data", "records"])
        .current_dir(tmp.path())
        .assert()
        .failure();
    let output = assertion.get_output();
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(
        !stdout.contains("data file(s)"),
        "a failed run must not print a success count: {stdout}"
    );
    assert!(
        stderr.contains("unreadable.json"),
        "the failing file must be named in the error: {stderr}"
    );

    // All or nothing: the file that did stage is rolled back, while the
    // schema staged before it survives.
    assert!(
        staged_data_entries(tmp.path()).is_empty(),
        "a partial run must leave no data staged"
    );
    assert!(
        panproto_core::vcs::Repository::open(tmp.path())
            .unwrap()
            .read_index()
            .unwrap()
            .staged
            .is_some(),
        "the staged schema must survive a data staging failure"
    );
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
fn cli_clone_requires_panproto_url() {
    let tmp = tempfile::tempdir().unwrap();

    // clone with an unrecognized URL scheme fails; the error mentions panproto://.
    schema_cmd()
        .args(["clone", "https://example.com/repo"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("panproto://"));
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

/// Write a schema with edges, for the cases the vertex-only helper cannot
/// express.
fn write_schema_with_edges(
    dir: &Path,
    name: &str,
    protocol: &str,
    vertices: &[(&str, &str)],
    edges: &[(&str, &str, &str, &str)],
) {
    let mut verts = serde_json::Map::new();
    for (id, kind) in vertices {
        verts.insert(
            id.to_string(),
            serde_json::json!({ "id": id, "kind": kind, "nsid": null }),
        );
    }
    let edge_entries: Vec<serde_json::Value> = edges
        .iter()
        .map(|(src, tgt, kind, edge_name)| {
            serde_json::json!([
                { "src": src, "tgt": tgt, "kind": kind, "name": edge_name },
                kind
            ])
        })
        .collect();
    let mut outgoing: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    let mut incoming: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    let mut between: Vec<serde_json::Value> = Vec::new();
    for (src, tgt, kind, edge_name) in edges {
        let edge = serde_json::json!({
            "src": src, "tgt": tgt, "kind": kind, "name": edge_name
        });
        outgoing
            .entry((*src).to_string())
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .unwrap()
            .push(edge.clone());
        incoming
            .entry((*tgt).to_string())
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .unwrap()
            .push(edge.clone());
        between.push(serde_json::json!([[src, tgt], [edge]]));
    }

    let schema = serde_json::json!({
        "protocol": protocol,
        "vertices": verts,
        "edges": edge_entries,
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
        "outgoing": outgoing,
        "incoming": incoming,
        "between": between
    });
    std::fs::write(
        dir.join(name),
        serde_json::to_string_pretty(&schema).unwrap(),
    )
    .unwrap();
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

/// `schema validate` must honor an exit-code contract — exit zero for a
/// clean schema whose protocol theories type-check, and non-zero for a failing
/// schema with the failure surfaced on stderr — so CI gates can trust the exit
/// status. (The theory-type-check bail itself is unit-tested in
/// `cmd::schema::tests`, since the only CLI-wired protocol, atproto, type-checks
/// cleanly and so cannot exercise that branch through the binary.)
#[test]
fn cli_validate_exit_code_contract() {
    let tmp = tempfile::tempdir().unwrap();

    // Clean fixture: a structurally valid atproto schema whose protocol
    // theories type-check. Exit status must be zero.
    write_protocol_schema(
        tmp.path(),
        "clean.json",
        "atproto",
        &[("root", "object"), ("root.name", "string")],
    );
    schema_cmd()
        .args(["validate", "--protocol", "atproto", "clean.json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Theory type-check: OK."));

    // Failing fixture: a schema that fails validation. Exit status must be
    // non-zero and the failure must be reported on stderr.
    write_protocol_schema(
        tmp.path(),
        "failing.json",
        "atproto",
        &[("root", "bogus_kind")],
    );
    let output = schema_cmd()
        .args(["validate", "--protocol", "atproto", "failing.json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "failing fixture must exit non-zero"
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("validation failed"),
        "failure must be reported on stderr, got: {stderr}"
    );
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
    // Migration maps root to "nonexistent", not present in target schema.
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
        entries: Vec::new(),
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
        coercions: HashMap::new(),
        domain: None,
        codomain: None,
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

/// `lens generate --json --top-n N --requirements` must emit a single
/// well-formed JSON document on stdout, carrying the base lens,
/// candidate list, and requirements as sections inside one root value
/// rather than as concatenated top-level JSON values that would break
/// `jq` and any strict JSON consumer. Parse stdout via
/// `serde_json::from_str` to guarantee a single root value, and assert
/// the composite sections landed inside it.
#[test]
fn cli_lens_generate_json_topn_requirements_single_document() {
    let tmp = tempfile::tempdir().unwrap();
    write_protocol_schema(tmp.path(), "src.json", "atproto", &[("root", "string")]);
    write_protocol_schema(tmp.path(), "tgt.json", "atproto", &[("root", "string")]);

    let output = schema_cmd()
        .args([
            "lens",
            "generate",
            "src.json",
            "tgt.json",
            "--protocol",
            "atproto",
            "--json",
            "--top-n",
            "2",
            "--requirements",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .get_output()
        .clone();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("stdout was not a single JSON document: {e}\n---\n{stdout}\n---")
    });
    assert!(parsed.is_object(), "root must be an object, got {parsed}");
    assert!(
        parsed.get("candidates").is_some(),
        "root must carry a `candidates` section; got keys {:?}",
        parsed.as_object().map(|o| o.keys().collect::<Vec<_>>()),
    );
    assert!(
        parsed.get("requirements").is_some(),
        "root must carry a `requirements` section; got keys {:?}",
        parsed.as_object().map(|o| o.keys().collect::<Vec<_>>()),
    );
}

/// `lens generate --json --fuse` must not emit two concatenated JSON
/// documents. When the chain is empty (identical schemas), fuse errors
/// cleanly before any JSON is printed; when non-empty, the fused
/// payload folds into the single root document. This test covers the
/// empty-chain error path: stdout must be empty, not a partial JSON.
#[test]
fn cli_lens_generate_json_fuse_empty_chain_does_not_leak_partial_json() {
    let tmp = tempfile::tempdir().unwrap();
    write_protocol_schema(tmp.path(), "src.json", "atproto", &[("root", "string")]);
    write_protocol_schema(tmp.path(), "tgt.json", "atproto", &[("root", "string")]);

    let output = schema_cmd()
        .args([
            "lens",
            "generate",
            "src.json",
            "tgt.json",
            "--protocol",
            "atproto",
            "--json",
            "--fuse",
        ])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    // Empty chain => fuse errors, exit code non-zero, stdout empty.
    // The important invariant: if stdout has any content, it parses as a
    // single JSON document (no concatenated documents).
    let stdout = String::from_utf8(output.stdout).unwrap();
    if !stdout.trim().is_empty() {
        serde_json::from_str::<serde_json::Value>(&stdout).unwrap_or_else(|e| {
            panic!("stdout must be empty or a single JSON document: {e}\n---\n{stdout}\n---")
        });
    }
}

/// `lens generate --chain --top-n N` must also emit a single JSON doc.
/// The `--chain` branch picks a different root shape than `--json`, so
/// guard that path separately.
#[test]
fn cli_lens_generate_chain_topn_single_document() {
    let tmp = tempfile::tempdir().unwrap();
    write_protocol_schema(tmp.path(), "src.json", "atproto", &[("root", "string")]);
    write_protocol_schema(tmp.path(), "tgt.json", "atproto", &[("root", "string")]);

    let output = schema_cmd()
        .args([
            "lens",
            "generate",
            "src.json",
            "tgt.json",
            "--protocol",
            "atproto",
            "--chain",
            "--top-n",
            "3",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .get_output()
        .clone();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("stdout was not a single JSON document: {e}\n---\n{stdout}\n---")
    });
    assert!(
        parsed.get("candidates").is_some(),
        "chain-mode root with --top-n must embed `candidates`; got {parsed}"
    );
}

/// A theory document with a deliberately lying Iso declaration:
/// `upper` forward, identity inverse, declared `iso`. The sample-based
/// law check must reject any non-uppercase string sample because
/// `identity(upper(s)) != s` when `s` has lowercase content.
fn write_lying_iso_theory(dir: &Path, name: &str) {
    let doc = serde_json::json!({
        "id": "test.lying_iso",
        "description": "fixture with a lying coercion",
        "theory": "LyingIso",
        "sorts": [
            { "name": "Str", "kind": { "type": "val", "value_kind": "string" } }
        ],
        "ops": [
            { "name": "upper", "input": "Str", "output": "Str" }
        ],
        "equations": [],
        "directed_equations": [
            {
                "name": "lying_upper_iso",
                "lhs": "upper(x)",
                "rhs": "x",
                "impl_expr": "upper(x)",
                "inverse": "x",
                "source_kind": "string",
                "target_kind": "string",
                "coercion_class": "iso"
            }
        ],
        "policies": []
    });
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
}

fn write_honest_identity_theory(dir: &Path, name: &str) {
    let doc = serde_json::json!({
        "id": "test.honest_iso",
        "description": "fixture with honest identity coercion",
        "theory": "HonestIso",
        "sorts": [
            { "name": "Str", "kind": { "type": "val", "value_kind": "string" } }
        ],
        // `id(x) -> x` rather than `x -> x`: a rule whose two sides are the
        // same term never makes progress, so it is not LPO-terminating and
        // the rewrite-system gate refuses it.
        "ops": [
            { "name": "id", "input": "Str", "output": "Str" }
        ],
        "equations": [],
        "directed_equations": [
            {
                "name": "identity_iso",
                "lhs": "id(x)",
                "rhs": "x",
                "impl_expr": "x",
                "inverse": "x",
                "source_kind": "string",
                "target_kind": "string",
                "coercion_class": "iso"
            }
        ],
        "policies": []
    });
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
}

#[test]
fn theory_check_coercion_laws_fails_on_lying_iso() {
    let tmp = tempfile::tempdir().unwrap();
    write_lying_iso_theory(tmp.path(), "lying.json");

    let output = schema_cmd()
        .args(["theory", "check-coercion-laws", "lying.json"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .get_output()
        .clone();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        combined.contains("lying_upper_iso") || combined.contains("violation"),
        "expected violation output, got:\nSTDOUT:\n{stdout}\nSTDERR:\n{stderr}",
    );
}

#[test]
fn theory_check_coercion_laws_passes_on_honest_iso() {
    let tmp = tempfile::tempdir().unwrap();
    write_honest_identity_theory(tmp.path(), "honest.json");

    schema_cmd()
        .args(["theory", "check-coercion-laws", "honest.json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("clean"));
}

#[test]
fn theory_check_coercion_laws_json_output_is_valid() {
    let tmp = tempfile::tempdir().unwrap();
    write_honest_identity_theory(tmp.path(), "honest.json");

    let output = schema_cmd()
        .args(["theory", "check-coercion-laws", "honest.json", "--json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("stdout was not a single JSON document: {e}\n---\n{stdout}\n---")
    });
    assert_eq!(parsed["clean"], serde_json::Value::Bool(true));
    assert_eq!(parsed["total_violations"], serde_json::json!(0));
}

#[test]
fn theory_check_coercion_laws_json_violations_carry_typed_kind() {
    // On a lying Iso declaration the JSON payload must surface
    // structured violations: each entry carries a typed `kind` field
    // (e.g. "Backward", "Forward") rather than a `Debug`-format
    // string, so downstream consumers can tree-shake by variant.
    let tmp = tempfile::tempdir().unwrap();
    write_lying_iso_theory(tmp.path(), "lying.json");

    let output = schema_cmd()
        .args(["theory", "check-coercion-laws", "lying.json", "--json"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .get_output()
        .clone();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("stdout was not a single JSON document: {e}\n---\n{stdout}\n---")
    });
    assert_eq!(parsed["clean"], serde_json::Value::Bool(false));
    let theories = parsed["theories"].as_array().unwrap();
    let mut saw_violation = false;
    let mut saw_typed_kind = false;
    for theory in theories {
        let eqs = theory["equations"].as_array().unwrap();
        for eq in eqs {
            let vs = eq["violations"].as_array().unwrap();
            for v in vs {
                saw_violation = true;
                let kind = v["kind"].as_str().unwrap_or_else(|| {
                    panic!(
                        "violation must carry a typed `kind` string field, \
                         got {v}"
                    )
                });
                assert!(
                    matches!(
                        kind,
                        "Backward"
                            | "Forward"
                            | "NonDeterministic"
                            | "MissingInverse"
                            | "ForwardEvalError"
                            | "InverseEvalError"
                            | "UnknownClass"
                    ),
                    "unexpected violation kind {kind:?} in {v}"
                );
                if kind == "Backward" || kind == "Forward" {
                    saw_typed_kind = true;
                }
            }
        }
    }
    assert!(saw_violation, "expected at least one violation in {parsed}");
    assert!(
        saw_typed_kind,
        "expected at least one Backward or Forward kind in {parsed}"
    );
}

/// Write a bundle document containing two theories. The DSL stores
/// theories in a `HashMap`, so iteration order is not insertion order;
/// the CLI must sort by theory name before rendering to keep JSON
/// output byte-stable across runs.
fn write_two_theory_bundle(dir: &Path, name: &str) {
    let doc = serde_json::json!({
        "id": "test.two_theory_bundle",
        "description": "two-theory bundle for determinism regression test",
        "bundle": "Pair",
        "theories": [
            {
                "theory": "BetaTheory",
                "sorts": [
                    { "name": "Str", "kind": { "type": "val", "value_kind": "string" } }
                ],
                "ops": [],
                "equations": [],
                "directed_equations": [],
                "policies": []
            },
            {
                "theory": "AlphaTheory",
                "sorts": [
                    { "name": "Str", "kind": { "type": "val", "value_kind": "string" } }
                ],
                "ops": [],
                "equations": [],
                "directed_equations": [],
                "policies": []
            }
        ]
    });
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
}

/// Theory whose directed equation binds `v` rather than the default
/// `x`. Under the default `--var-name x` the checker will surface
/// `ForwardEvalError { error: "unbound variable: v" }` on every
/// sample; the CLI must emit a hint suggesting `--var-name v`.
fn write_theory_binding_alternate_var(dir: &Path, name: &str) {
    let doc = serde_json::json!({
        "id": "test.alternate_var",
        "description": "fixture whose equation binds `v`",
        "theory": "AltVar",
        "sorts": [
            { "name": "Str", "kind": { "type": "val", "value_kind": "string" } }
        ],
        // `id(v) -> v` rather than `v -> v`: a rule whose two sides are the
        // same term never makes progress, so it is not LPO-terminating and
        // the rewrite-system gate refuses it. The bound variable stays `v`,
        // which is what these tests are about.
        "ops": [
            { "name": "id", "input": "Str", "output": "Str" }
        ],
        "equations": [],
        "directed_equations": [
            {
                "name": "alt_var_iso",
                "lhs": "id(v)",
                "rhs": "v",
                "impl_expr": "v",
                "inverse": "v",
                "source_kind": "string",
                "target_kind": "string",
                "coercion_class": "iso"
            }
        ],
        "policies": []
    });
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
}

#[test]
fn theory_check_coercion_laws_emits_var_name_hint() {
    let tmp = tempfile::tempdir().unwrap();
    write_theory_binding_alternate_var(tmp.path(), "alt.json");

    let output = schema_cmd()
        .args(["theory", "check-coercion-laws", "alt.json"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .get_output()
        .clone();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("hint:") && stdout.contains("--var-name v"),
        "expected unbound-variable hint mentioning `--var-name v`; got stdout:\n{stdout}",
    );
}

#[test]
fn theory_check_coercion_laws_accepts_var_name_override() {
    let tmp = tempfile::tempdir().unwrap();
    write_theory_binding_alternate_var(tmp.path(), "alt.json");

    // With `--var-name v` the identity equation round-trips cleanly
    // and the command must succeed.
    schema_cmd()
        .args([
            "theory",
            "check-coercion-laws",
            "alt.json",
            "--var-name",
            "v",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();
}

#[test]
fn theory_compile_json_output_is_deterministic_across_runs() {
    let tmp = tempfile::tempdir().unwrap();
    write_two_theory_bundle(tmp.path(), "pair.json");

    let run_once = || -> String {
        let output = schema_cmd()
            .args(["theory", "compile", "pair.json", "--json"])
            .current_dir(tmp.path())
            .assert()
            .success()
            .get_output()
            .clone();
        String::from_utf8(output.stdout).unwrap()
    };

    let first = run_once();
    // Repeated runs must produce byte-identical stdout. Collect a
    // handful so a spurious insertion-order coincidence is unlikely to
    // pass.
    for _ in 0..5 {
        assert_eq!(run_once(), first, "theory compile JSON must be stable");
    }
    // Theories must appear alphabetically (AlphaTheory before
    // BetaTheory) regardless of declaration order.
    let parsed: serde_json::Value = serde_json::from_str(&first).unwrap();
    let theories = parsed["theories"].as_array().unwrap();
    let names: Vec<&str> = theories.iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(names, vec!["AlphaTheory", "BetaTheory"]);
}

// ---------------------------------------------------------------------------
// `schema lens compile`: DSL document compilation
// ---------------------------------------------------------------------------

/// `schema lens compile <doc.yaml> --body-vertex <v>` loads a lens DSL
/// document and prints one versioned JSON artifact carrying the executable
/// chain and its ordered stages.
#[test]
fn cli_lens_compile_yaml_emits_chain_json() {
    let tmp = tempfile::tempdir().unwrap();
    let doc = tmp.path().join("rename.yaml");
    std::fs::write(
        &doc,
        "id: dev.test.rename\nsource: s\ntarget: \"\"\nsteps:\n  - rename_field:\n      old: title\n      new: heading\n",
    )
    .unwrap();

    let output = schema_cmd()
        .args([
            "lens",
            "compile",
            doc.to_str().unwrap(),
            "--body-vertex",
            "main",
        ])
        .assert()
        .success()
        .get_output()
        .clone();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("stdout was not a single JSON document: {e}\n---\n{stdout}\n---")
    });
    assert_eq!(parsed["format"], "panproto-compiled-lens-v1");
    assert!(
        parsed.get("chain").is_some(),
        "compile output must carry a `chain` key; got keys {:?}",
        parsed.as_object().map(|o| o.keys().collect::<Vec<_>>()),
    );
    // The embedded chain round-trips through the engine's own codec.
    assert!(
        parsed["chain"].get("steps").is_some(),
        "chain value must be the serde ProtolensChain shape"
    );
    assert_eq!(parsed["stages"].as_array().unwrap().len(), 1);
    assert!(parsed.get("field_transforms").is_some());
}

fn write_ordered_lens_document(dir: &Path, name: &str) {
    std::fs::write(
        dir.join(name),
        r#"{
          "id": "dev.test.ordered",
          "source": "s",
          "target": "t",
          "steps": [
            { "rename_field": { "old": "text", "new": "amount" } },
            { "compute_field": {
                "target": "derived",
                "expr": "amount ++ \"!\""
            } }
          ]
        }"#,
    )
    .unwrap();
}

#[test]
fn cli_lens_compile_retains_value_transforms_and_stage_order() {
    let tmp = tempfile::tempdir().unwrap();
    write_ordered_lens_document(tmp.path(), "ordered.json");

    let output = schema_cmd()
        .args(["lens", "compile", "ordered.json", "--body-vertex", "root"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .get_output()
        .clone();

    let artifact: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(artifact["format"], "panproto-compiled-lens-v1");
    assert_eq!(artifact["step_count"], 1);
    assert_eq!(artifact["field_transform_vertices"], 1);
    assert_eq!(artifact["stages"].as_array().unwrap().len(), 2);
    assert_eq!(
        artifact["field_transforms"]["root"][0]["ComputeField"]["target_key"],
        "derived"
    );
    assert!(
        artifact["stages"][0]["chain"]["steps"]
            .as_array()
            .is_some_and(|steps| !steps.is_empty())
    );
    assert_eq!(
        artifact["stages"][1]["field_transforms"]["root"][0]["ComputeField"]["target_key"],
        "derived"
    );
}

#[test]
fn cli_lens_apply_accepts_compiled_artifact_with_ordered_stages() {
    let tmp = tempfile::tempdir().unwrap();
    write_ordered_lens_document(tmp.path(), "ordered.json");
    write_schema_with_edges(
        tmp.path(),
        "source.json",
        "atproto",
        &[("root", "object"), ("root.text", "string")],
        &[("root", "root.text", "prop", "text")],
    );
    std::fs::write(tmp.path().join("data.json"), r#"{"text":"hello"}"#).unwrap();

    schema_cmd()
        .args([
            "lens",
            "compile",
            "ordered.json",
            "--body-vertex",
            "root",
            "--out",
            "compiled.json",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = schema_cmd()
        .args([
            "lens",
            "apply",
            "compiled.json",
            "data.json",
            "--protocol",
            "atproto",
            "--schema",
            "source.json",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .get_output()
        .clone();

    let view: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(view["amount"], "hello");
    assert_eq!(view["derived"], "hello!");
    assert!(view.get("text").is_none());
}

/// `schema lens compile` on an `auto` body without schema context must
/// exit non-zero: auto-generation needs source/target schemas, which the
/// pure DSL compile path does not have.
#[test]
fn cli_lens_compile_auto_body_without_schemas_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let doc = tmp.path().join("auto.yaml");
    std::fs::write(&doc, "id: dev.test.auto\nsource: s\ntarget: t\nauto: {}\n").unwrap();

    schema_cmd()
        .args([
            "lens",
            "compile",
            doc.to_str().unwrap(),
            "--body-vertex",
            "main",
        ])
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// `schema auto-migrate`
// ---------------------------------------------------------------------------
//
// The command runs one span search and the three flags form a strictness
// ladder over its answer, so each test below fixes a schema pair and varies
// only the flag. The pair that drops a field is the case the command exists
// for: no total morphism maps it, and the span does.

/// A source field whose kind the target has no vertex for cannot be mapped, so
/// no total morphism exists. The default acceptance reports the span anyway,
/// and says how much of the source it covers.
#[test]
fn cli_auto_migrate_reports_a_partial_span_by_default() {
    let tmp = tempfile::tempdir().unwrap();
    write_protocol_schema(
        tmp.path(),
        "old.json",
        "atproto",
        &[
            ("root", "object"),
            ("root.name", "string"),
            ("root.count", "integer"),
        ],
    );
    write_protocol_schema(
        tmp.path(),
        "new.json",
        "atproto",
        &[("root", "object"), ("root.name", "string")],
    );

    schema_cmd()
        .args(["auto-migrate", "old.json", "new.json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Found span"))
        .stdout(predicate::str::contains("Apex: 2 of 3 vertices"))
        .stdout(predicate::str::contains("66.7% coverage"));
}

/// `--total` accepts only a span covering every source vertex, and says how
/// far short the answer fell rather than reporting that nothing was found.
#[test]
fn cli_auto_migrate_total_refuses_a_partial_span_and_says_what_it_found() {
    let tmp = tempfile::tempdir().unwrap();
    write_protocol_schema(
        tmp.path(),
        "old.json",
        "atproto",
        &[
            ("root", "object"),
            ("root.name", "string"),
            ("root.count", "integer"),
        ],
    );
    write_protocol_schema(
        tmp.path(),
        "new.json",
        "atproto",
        &[("root", "object"), ("root.name", "string")],
    );

    schema_cmd()
        .args(["auto-migrate", "--total", "old.json", "new.json"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("no total morphism exists"))
        .stderr(predicate::str::contains("2 of 3 source vertices"));
}

/// `--total` asks whether a total morphism *exists*. The span search answers a
/// different question, and its answer is no evidence about existence: span
/// quality excludes the drop count while the objective is lexicographic in
/// `(quality, drops)`, so dropping a vertex that only contributes mismatch cost
/// wins outright. Reading a partial optimum as "no total morphism exists" is a
/// false statement about the pair, not a conservative one.
#[test]
fn cli_auto_migrate_total_finds_a_morphism_the_optimal_span_is_not() {
    let tmp = tempfile::tempdir().unwrap();
    write_schema_with_edges(
        tmp.path(),
        "old.json",
        "atproto",
        &[("s_beta_1", "record"), ("s_epsilon_0", "string")],
        &[("s_beta_1", "s_epsilon_0", "prop", "r")],
    );
    write_schema_with_edges(
        tmp.path(),
        "new.json",
        "atproto",
        &[
            ("t_alpha_0", "record"),
            ("t_beta_1", "record"),
            ("t_beta_2", "string"),
            ("t_delta_3", "record"),
        ],
        &[
            ("t_alpha_0", "t_delta_3", "prop", "r"),
            ("t_beta_1", "t_beta_2", "prop", "q"),
        ],
    );

    // The premise: without `--total` the optimal span is partial, covering one
    // of the two source vertices.
    schema_cmd()
        .args(["auto-migrate", "old.json", "new.json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("1 of 2 vertices"));

    // And `--total` must find the total morphism that exists anyway.
    schema_cmd()
        .args(["auto-migrate", "--total", "old.json", "new.json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("total morphism"))
        .stdout(predicate::str::contains("s_beta_1 -> t_beta_1"))
        .stdout(predicate::str::contains("s_epsilon_0 -> t_beta_2"));
}

/// A right leg that identifies two source vertices is reported, and on stderr
/// so that `--json` stays pipeable.
///
/// Such a migration is an ordinary answer from the default search, and it is
/// not one a lift can carry out: both fields' data would arrive under the
/// survivor's name. Handing it over without a word is how the caller ends up
/// running it.
#[test]
fn cli_auto_migrate_warns_when_the_right_leg_contracts() {
    let tmp = tempfile::tempdir().unwrap();
    write_schema_with_edges(
        tmp.path(),
        "old.json",
        "atproto",
        &[
            ("post", "object"),
            ("post.title", "string"),
            ("post.subtitle", "string"),
        ],
        &[
            ("post", "post.title", "prop", "title"),
            ("post", "post.subtitle", "prop", "subtitle"),
        ],
    );
    write_schema_with_edges(
        tmp.path(),
        "new.json",
        "atproto",
        &[("post", "object"), ("post.heading", "string")],
        &[("post", "post.heading", "prop", "heading")],
    );

    schema_cmd()
        .args(["auto-migrate", "--json", "old.json", "new.json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("not injective on vertices"))
        .stdout(predicate::str::contains("vertex_map"));
}

/// A pair admitting a total morphism reports one, and `--total` accepts it. The
/// heading distinguishes the two answers, so this is what pins the default path
/// above to the partial case rather than to the wording alone.
#[test]
fn cli_auto_migrate_total_accepts_a_pair_that_maps_entirely() {
    let tmp = tempfile::tempdir().unwrap();
    write_protocol_schema(
        tmp.path(),
        "old.json",
        "atproto",
        &[("root", "object"), ("root.name", "string")],
    );
    write_protocol_schema(
        tmp.path(),
        "new.json",
        "atproto",
        &[("root", "object"), ("root.label", "string")],
    );

    schema_cmd()
        .args(["auto-migrate", "--total", "old.json", "new.json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Found total morphism"))
        .stdout(predicate::str::contains("100.0% coverage"));
}

/// `--total` and `--span` are the two ends of one ladder and clap refuses them
/// together, so neither can silently win.
#[test]
fn cli_auto_migrate_total_and_span_conflict() {
    let tmp = tempfile::tempdir().unwrap();
    write_protocol_schema(tmp.path(), "old.json", "atproto", &[("root", "object")]);
    write_protocol_schema(tmp.path(), "new.json", "atproto", &[("root", "object")]);

    schema_cmd()
        .args(["auto-migrate", "--total", "--span", "old.json", "new.json"])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

/// `--json` writes the span's right leg, a migration out of the apex, and
/// nothing else on stdout.
#[test]
fn cli_auto_migrate_json_emits_the_right_leg() {
    let tmp = tempfile::tempdir().unwrap();
    write_protocol_schema(
        tmp.path(),
        "old.json",
        "atproto",
        &[("root", "object"), ("root.name", "string")],
    );
    write_protocol_schema(
        tmp.path(),
        "new.json",
        "atproto",
        &[("root", "object"), ("root.label", "string")],
    );

    let out = schema_cmd()
        .args(["auto-migrate", "--json", "old.json", "new.json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let migration: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert!(
        migration.get("vertex_map").is_some(),
        "the right leg is a migration and must carry a vertex map: {migration}"
    );
}

/// The apex is a schema, and a schema is well formed only against a protocol,
/// so the command resolves the source schema's protocol and refuses a name it
/// does not carry. `validate`, `integrate` and `check` already read it this
/// way; this is the test that keeps `auto-migrate` reading it the same way.
#[test]
fn cli_auto_migrate_refuses_an_unresolvable_protocol() {
    let tmp = tempfile::tempdir().unwrap();
    write_protocol_schema(
        tmp.path(),
        "old.json",
        "not_a_registered_protocol",
        &[("root", "object")],
    );
    write_protocol_schema(
        tmp.path(),
        "new.json",
        "not_a_registered_protocol",
        &[("root", "object")],
    );

    schema_cmd()
        .args(["auto-migrate", "old.json", "new.json"])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// ATProto project workflow, end to end
// ---------------------------------------------------------------------------

/// Write a two-document `ATProto` project rooted at `dir`.
///
/// `com.example.record` holds a `ref` to `com.example.defs`, so the two
/// documents only assemble into one schema if the bundle parser resolves
/// references across files. `extra_defs_property` adds a second, optional
/// property to `com.example.defs`, which is the compatible change the
/// compatibility check is driven with.
fn write_reported_atproto_project(dir: &Path, extra_defs_property: bool) {
    std::fs::create_dir_all(dir.join("lexicons")).unwrap();
    std::fs::write(
        dir.join("panproto.toml"),
        "[workspace]\n\
         name = \"atproto-project\"\n\
         \n\
         [[package]]\n\
         name = \"lexicons\"\n\
         path = \"lexicons\"\n\
         protocol = \"atproto\"\n",
    )
    .unwrap();

    let mut properties = serde_json::Map::new();
    properties.insert(
        "value".to_owned(),
        serde_json::json!({ "type": "string", "format": "datetime" }),
    );
    if extra_defs_property {
        properties.insert("note".to_owned(), serde_json::json!({ "type": "string" }));
    }
    let defs = serde_json::json!({
        "lexicon": 1,
        "id": "com.example.defs",
        "defs": { "main": { "type": "object", "properties": properties } }
    });
    std::fs::write(
        dir.join("lexicons/com.example.defs.json"),
        serde_json::to_string_pretty(&defs).unwrap(),
    )
    .unwrap();

    let record = serde_json::json!({
        "lexicon": 1,
        "id": "com.example.record",
        "defs": { "main": {
            "type": "record",
            "key": "tid",
            "record": {
                "type": "object",
                "required": ["item"],
                "properties": { "item": { "type": "ref", "ref": "com.example.defs" } }
            }
        } }
    });
    std::fs::write(
        dir.join("lexicons/com.example.record.json"),
        serde_json::to_string_pretty(&record).unwrap(),
    )
    .unwrap();
}

/// The reported `ATProto` project workflow, driven end to end.
///
/// A homogeneous two-document `ATProto` project staged with `add . --data`
/// must resolve its cross-document `ref`, stage the data it reports
/// staging, keep its own protocol so equations are checked against the
/// `ATProto` theory rather than skipped, and carry the data into the
/// commit.
#[test]
fn cli_atproto_project_stages_data_and_keeps_its_protocol() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_reported_atproto_project(root, false);
    std::fs::create_dir_all(root.join("fixtures/records")).unwrap();
    std::fs::write(
        root.join("fixtures/records/example.json"),
        r#"{"$type": "com.example.record", "item": {"value": "2026-08-12T00:00:00Z"}}"#,
    )
    .unwrap();
    init_repo(root);

    schema_cmd()
        .args(["add", ".", "--data", "fixtures/records"])
        .current_dir(root)
        .assert()
        .success()
        .stdout(predicate::str::contains("Staged 1 data file(s)"));

    // The printed count must be backed by the index, which was the
    // defect: the count was printed without staging anything.
    let staged = staged_data_entries(root);
    assert_eq!(
        staged.len(),
        1,
        "the index must hold the data file the command reported staging"
    );
    assert_eq!(staged[0].0, "fixtures/records/example.json");

    let repo = panproto_core::vcs::Repository::open(root).unwrap();
    let index = repo.read_index().unwrap();
    let staged_schema = index.staged.as_ref().unwrap();

    // The assembled project keeps `atproto`, so the theory lookup finds
    // one and the equations are checked instead of skipped.
    let schema = panproto_core::vcs::assemble_schema(
        repo.store(),
        &staged_schema.schema_id,
        &panproto_core::vcs::project_coproduct_protocol(),
    )
    .unwrap();
    assert_eq!(
        schema.protocol, "atproto",
        "a homogeneous ATProto project must not be stamped with the coproduct protocol"
    );

    let diagnostics = staged_schema.gat_diagnostics.as_ref().unwrap();
    assert!(
        diagnostics.equation_notes.is_empty(),
        "a registered protocol theory leaves no missing-theory advisory, got: {:?}",
        diagnostics.equation_notes
    );
    assert!(
        diagnostics.equation_errors.is_empty(),
        "the ATProto project must satisfy the ATProto theory, got: {:?}",
        diagnostics.equation_errors
    );

    // The cross-document `ref` resolves: the edge leaves the record
    // document and lands on a vertex owned by the defs document.
    let (ref_edge, _) = schema
        .edges
        .iter()
        .find(|(edge, _)| &*edge.kind == "ref")
        .unwrap();
    assert!(
        ref_edge.src.contains("com.example.record.json")
            && ref_edge.tgt.contains("com.example.defs.json"),
        "the ref must cross documents, got {} -> {}",
        ref_edge.src,
        ref_edge.tgt
    );

    schema_cmd()
        .args(["commit", "-m", "atproto project with data"])
        .current_dir(root)
        .assert()
        .success();

    // The commit carries the staged data set, keyed by its source path.
    let committed = repo.data_at("HEAD").unwrap();
    assert_eq!(committed.len(), 1, "the commit must carry the staged data");
    assert_eq!(
        committed[0].key.as_deref(),
        Some("fixtures/records/example.json")
    );
    assert!(
        String::from_utf8_lossy(&committed[0].data).contains("com.example.record"),
        "the committed data set must hold the record's bytes"
    );
}

/// Two manifest-backed versions of the project compare through the
/// supported compatibility path.
///
/// Both operands are project directories, which `compat` could not load
/// before: it deserialized each operand as an internal schema. Adding an
/// optional property is compatible (exit 0); removing the referenced
/// property is breaking (exit 1).
#[test]
fn cli_atproto_project_versions_compare_through_compat() {
    let tmp = tempfile::tempdir().unwrap();
    let old = tmp.path().join("old");
    let new = tmp.path().join("new");
    write_reported_atproto_project(&old, false);
    write_reported_atproto_project(&new, true);

    schema_cmd()
        .args(["compat", "old", "new", "--protocol", "atproto"])
        .current_dir(tmp.path())
        .assert()
        .code(0)
        .stdout(predicate::str::contains("com.example.defs.note"));

    // Reversing the operands removes a property, which is breaking.
    schema_cmd()
        .args(["compat", "new", "old", "--protocol", "atproto"])
        .current_dir(tmp.path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains("breaking"));

    // A protocol that disagrees with the manifest is a usage error, not
    // a silent override, and must be distinguishable from "breaking".
    schema_cmd()
        .args(["compat", "old", "new", "--protocol", "sql"])
        .current_dir(tmp.path())
        .assert()
        .code(2);
}
