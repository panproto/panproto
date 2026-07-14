//! Integration tests for the `schema compat` subcommand.
//!
//! Exercises the CI-usable exit codes (0 = compatible, 1 = breaking,
//! 2 = usage/load error) and the `--format json` output.

#![allow(clippy::unwrap_used)]

use assert_cmd::Command;
use std::path::Path;

fn schema_cmd() -> Command {
    Command::cargo_bin("schema").unwrap()
}

/// Write a schema JSON file with the full field set so it deserialises
/// into `panproto_schema::Schema`.
fn write_schema(dir: &Path, name: &str, vertices: &[(&str, &str)]) {
    let mut verts = serde_json::Map::new();
    for (id, kind) in vertices {
        verts.insert(
            (*id).to_string(),
            serde_json::json!({ "id": id, "kind": kind, "nsid": null }),
        );
    }
    let schema = serde_json::json!({
        "protocol": "atproto",
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
    std::fs::write(
        dir.join(name),
        serde_json::to_string_pretty(&schema).unwrap(),
    )
    .unwrap();
}

#[test]
fn compat_breaking_pair_exits_1() {
    let dir = tempfile::tempdir().unwrap();
    // Removing the `post` vertex is a breaking change.
    write_schema(dir.path(), "old.json", &[("post", "record")]);
    write_schema(dir.path(), "new.json", &[]);

    schema_cmd()
        .args(["compat", "old.json", "new.json", "--protocol", "atproto"])
        .current_dir(dir.path())
        .assert()
        .code(1)
        .stdout(predicates::str::contains("breaking"));
}

#[test]
fn compat_compatible_pair_exits_0() {
    let dir = tempfile::tempdir().unwrap();
    // Adding a vertex is backward-compatible.
    write_schema(dir.path(), "old.json", &[]);
    write_schema(dir.path(), "new.json", &[("post", "record")]);

    schema_cmd()
        .args(["compat", "old.json", "new.json", "--protocol", "atproto"])
        .current_dir(dir.path())
        .assert()
        .code(0)
        .stdout(predicates::str::contains("COMPATIBLE"));
}

#[test]
fn compat_json_format_reports_classification() {
    let dir = tempfile::tempdir().unwrap();
    write_schema(dir.path(), "old.json", &[("post", "record")]);
    write_schema(dir.path(), "new.json", &[]);

    let output = schema_cmd()
        .args([
            "compat",
            "old.json",
            "new.json",
            "--protocol",
            "atproto",
            "--format",
            "json",
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["classification"], "breaking");
    assert!(value["breaking_count"].as_u64().unwrap() >= 1);
}

#[test]
fn compat_missing_file_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    write_schema(dir.path(), "old.json", &[("post", "record")]);

    schema_cmd()
        .args([
            "compat",
            "old.json",
            "does-not-exist.json",
            "--protocol",
            "atproto",
        ])
        .current_dir(dir.path())
        .assert()
        .code(2);
}

#[test]
fn compat_bad_format_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    write_schema(dir.path(), "old.json", &[("post", "record")]);
    write_schema(dir.path(), "new.json", &[("post", "record")]);

    schema_cmd()
        .args([
            "compat",
            "old.json",
            "new.json",
            "--protocol",
            "atproto",
            "--format",
            "toml",
        ])
        .current_dir(dir.path())
        .assert()
        .code(2);
}
