//! Integration tests for the `schema compat` subcommand.
//!
//! Exercises the CI-usable exit codes (0 = compatible, 1 = breaking,
//! 2 = usage/load error), the `--format json` output, and the input
//! shapes the shared loader accepts: panproto's own schema JSON, a
//! single lexicon document, a manifest-backed project directory, and a
//! bare directory of lexicons named by `--protocol`.

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

/// A `panproto.toml` declaring one `ATProto` package named `lexicons`.
const ATPROTO_MANIFEST: &str = "\
[workspace]
name = \"atproto-project\"

[[package]]
name = \"lexicons\"
path = \"lexicons\"
protocol = \"atproto\"
";

/// Write a two-document `ATProto` lexicon set under `root/lexicons`,
/// with `com.example.record` referring across documents into
/// `com.example.defs`, whose properties the caller chooses.
///
/// `with_manifest` controls whether the project is manifest-backed (so
/// the protocol comes from `panproto.toml`) or a bare directory of
/// lexicons (so it comes from `--protocol`).
fn write_atproto_project(root: &Path, defs_properties: &serde_json::Value, with_manifest: bool) {
    let lexicons = root.join("lexicons");
    std::fs::create_dir_all(&lexicons).unwrap();
    if with_manifest {
        std::fs::write(root.join("panproto.toml"), ATPROTO_MANIFEST).unwrap();
    }

    write_json(
        &lexicons.join("com.example.defs.json"),
        &serde_json::json!({
            "lexicon": 1,
            "id": "com.example.defs",
            "defs": {
                "main": { "type": "object", "properties": defs_properties }
            }
        }),
    );
    write_json(
        &lexicons.join("com.example.record.json"),
        &serde_json::json!({
            "lexicon": 1,
            "id": "com.example.record",
            "defs": {
                "main": {
                    "type": "record",
                    "key": "tid",
                    "record": {
                        "type": "object",
                        "required": ["item"],
                        "properties": {
                            "item": { "type": "ref", "ref": "com.example.defs" }
                        }
                    }
                }
            }
        }),
    );
}

fn write_json(path: &Path, value: &serde_json::Value) {
    std::fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
}

/// The property set the `old` side of every project pair starts from.
fn base_properties() -> serde_json::Value {
    serde_json::json!({ "value": { "type": "string", "format": "datetime" } })
}

#[test]
fn compat_manifest_projects_breaking_change_exits_1() {
    let dir = tempfile::tempdir().unwrap();
    write_atproto_project(&dir.path().join("old"), &base_properties(), true);
    // Dropping the `value` property removes a vertex another document
    // refers into, which is breaking.
    write_atproto_project(&dir.path().join("new"), &serde_json::json!({}), true);

    schema_cmd()
        .args(["compat", "old", "new", "--protocol", "atproto"])
        .current_dir(dir.path())
        .assert()
        .code(1)
        .stdout(predicates::str::contains(
            "lexicons/com.example.defs.json::com.example.defs.value",
        ));
}

#[test]
fn compat_manifest_projects_compatible_change_exits_0() {
    let dir = tempfile::tempdir().unwrap();
    write_atproto_project(&dir.path().join("old"), &base_properties(), true);
    // Adding an optional property is backward-compatible.
    write_atproto_project(
        &dir.path().join("new"),
        &serde_json::json!({
            "value": { "type": "string", "format": "datetime" },
            "note": { "type": "string" }
        }),
        true,
    );

    schema_cmd()
        .args(["compat", "old", "new", "--protocol", "atproto"])
        .current_dir(dir.path())
        .assert()
        .code(0)
        .stdout(predicates::str::contains("COMPATIBLE"));
}

#[test]
fn compat_manifest_projects_json_format_reports_classification() {
    let dir = tempfile::tempdir().unwrap();
    write_atproto_project(&dir.path().join("old"), &base_properties(), true);
    write_atproto_project(&dir.path().join("new"), &serde_json::json!({}), true);

    let output = schema_cmd()
        .args([
            "compat",
            "old",
            "new",
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
fn diff_manifest_project_reports_cross_document_ref() {
    let dir = tempfile::tempdir().unwrap();
    write_atproto_project(&dir.path().join("old"), &base_properties(), true);
    write_atproto_project(&dir.path().join("new"), &base_properties(), true);
    // Removing the referenced document takes the cross-document ref
    // edge with it. That edge exists only because the two lexicons were
    // parsed as one bundle: parsed alone, the record's `item` ref would
    // point at an opaque placeholder inside its own document.
    std::fs::remove_file(dir.path().join("new/lexicons/com.example.defs.json")).unwrap();

    schema_cmd()
        .args(["diff", "old", "new"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "- edge lexicons/com.example.record.json::com.example.record:body.item \
             -> lexicons/com.example.defs.json::com.example.defs (ref)",
        ));
}

#[test]
fn compat_bare_lexicon_directories_use_requested_protocol() {
    let dir = tempfile::tempdir().unwrap();
    write_atproto_project(&dir.path().join("old"), &base_properties(), false);
    write_atproto_project(&dir.path().join("new"), &serde_json::json!({}), false);

    schema_cmd()
        .args([
            "compat",
            "old/lexicons",
            "new/lexicons",
            "--protocol",
            "atproto",
        ])
        .current_dir(dir.path())
        .assert()
        .code(1)
        .stdout(predicates::str::contains("com.example.defs.value"));
}

#[test]
fn compat_single_lexicon_documents_use_manifest_protocol() {
    let dir = tempfile::tempdir().unwrap();
    write_atproto_project(&dir.path().join("old"), &base_properties(), true);
    write_atproto_project(&dir.path().join("new"), &serde_json::json!({}), true);

    schema_cmd()
        .args([
            "compat",
            "old/lexicons/com.example.defs.json",
            "new/lexicons/com.example.defs.json",
            "--protocol",
            "atproto",
        ])
        .current_dir(dir.path())
        .assert()
        .code(1)
        .stdout(predicates::str::contains("com.example.defs.value"));
}

#[test]
fn compat_protocol_disagreeing_with_manifest_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    write_atproto_project(&dir.path().join("old"), &base_properties(), true);
    write_atproto_project(&dir.path().join("new"), &base_properties(), true);

    schema_cmd()
        .args(["compat", "old", "new", "--protocol", "sql"])
        .current_dir(dir.path())
        .assert()
        .code(2)
        .stderr(predicates::str::contains("disagrees with"));
}

#[test]
fn compat_unparseable_project_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    write_atproto_project(&dir.path().join("old"), &base_properties(), true);
    write_atproto_project(&dir.path().join("new"), &base_properties(), true);
    std::fs::write(
        dir.path().join("new/lexicons/com.example.defs.json"),
        "{ this is not json",
    )
    .unwrap();

    schema_cmd()
        .args(["compat", "old", "new", "--protocol", "atproto"])
        .current_dir(dir.path())
        .assert()
        .code(2);
}

#[test]
fn compat_directory_without_bundle_protocol_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    write_atproto_project(&dir.path().join("old"), &base_properties(), false);
    write_atproto_project(&dir.path().join("new"), &base_properties(), false);

    // `sql` has no bundle parser, so a bare directory cannot be read
    // under it; that is a load error, not a breaking change.
    schema_cmd()
        .args([
            "compat",
            "old/lexicons",
            "new/lexicons",
            "--protocol",
            "sql",
        ])
        .current_dir(dir.path())
        .assert()
        .code(2)
        .stderr(predicates::str::contains("no bundle parser"));
}

#[test]
fn diff_manifest_projects_lists_both_documents() {
    let dir = tempfile::tempdir().unwrap();
    write_atproto_project(&dir.path().join("old"), &base_properties(), true);
    write_atproto_project(
        &dir.path().join("new"),
        &serde_json::json!({
            "value": { "type": "string", "format": "datetime" },
            "note": { "type": "string" }
        }),
        true,
    );

    schema_cmd()
        .args(["diff", "old", "new"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "lexicons/com.example.defs.json::com.example.defs.note",
        ));
}
