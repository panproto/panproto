//! `--protocol` reads a schema document with the protocol's own reader,
//! including the protocols read through a tree-sitter grammar.
//!
//! Each case reads a document of the protocol's conformance corpus with
//! `schema validate --verbose`, which reports the identity of the schema
//! it read, and compares that identity with the one the Rust reader gives
//! for the same bytes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use panproto_core::inst::identity::schema_digest;
use panproto_core::io::api::{asyncapi, openapi};
use panproto_core::io::config::{ansible, cloudformation, k8s_crd};
use panproto_core::io::data_schema::json_schema;
use panproto_core::io::postgresql;
use panproto_core::protocols::config::ansible::FileClass;
use panproto_core::protocols::database::postgresql::OtherStatements;
use panproto_core::schema::Schema;

/// The conformance corpora of `panproto-io`.
fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../panproto-io/tests/fixtures/conformance")
}

/// The identity `schema validate --verbose` reports for `path` read under
/// `protocol`, with `extra` arguments.
fn identity_read_by_the_cli(protocol: &str, path: &Path, extra: &[&str]) -> String {
    let output = Command::cargo_bin("schema")
        .unwrap()
        .args(["validate", "--verbose", "--protocol", protocol])
        .args(extra)
        .arg(path)
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{protocol}: {stderr}");
    let line = stderr
        .lines()
        .find(|line| line.starts_with("Validating schema "))
        .unwrap_or_else(|| panic!("{protocol}: no identity reported in {stderr}"));
    line["Validating schema ".len()..]
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned()
}

/// Read `relative` under `protocol` with the CLI, and assert it is the
/// schema `read` builds from the same bytes.
fn agrees(protocol: &str, relative: &str, extra: &[&str], read: impl Fn(&[u8]) -> Schema) {
    let path = corpus().join(relative);
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(
        identity_read_by_the_cli(protocol, &path, extra),
        schema_digest(&read(&bytes)).to_hex(),
        "{protocol}: {relative}"
    );
}

#[test]
fn openapi_is_read_by_its_reader() {
    agrees(
        "openapi",
        "openapi/upstream/pass/minimal_paths.yaml",
        &[],
        |b| openapi::read_openapi(b).unwrap().schema,
    );
}

#[test]
fn asyncapi_is_read_by_its_reader() {
    agrees(
        "asyncapi",
        "asyncapi/upstream/examples/simple-asyncapi.yml",
        &[],
        |b| asyncapi::read_asyncapi(b).unwrap().schema,
    );
}

#[test]
fn a_json_schema_is_read_by_its_reader() {
    // A case of the JSON-Schema-Test-Suite holds its schema under
    // `schema`; the document read is that schema, as the suite gives it.
    let cases: serde_json::Value = serde_json::from_slice(
        &std::fs::read(corpus().join("json-schema-test-suite/tests/minLength.json")).unwrap(),
    )
    .unwrap();
    let document = serde_json::to_string(&cases["tests"][0]["schema"]).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("min-length.json");
    std::fs::write(&path, &document).unwrap();
    assert_eq!(
        identity_read_by_the_cli("json-schema", &path, &[]),
        schema_digest(
            &json_schema::read_schema(document.as_bytes())
                .unwrap()
                .schema
        )
        .to_hex()
    );
}

#[test]
fn a_crd_is_read_by_its_reader() {
    agrees("k8s-crd", "k8s-crd/upstream/crd.yaml", &[], |b| {
        k8s_crd::read_crd(b).unwrap().schema
    });
}

#[test]
fn a_cloudformation_template_is_read_by_its_reader() {
    agrees(
        "cloudformation",
        "cloudformation/upstream/SNS_SNSTopic.json",
        &[],
        |b| cloudformation::read_template(b).unwrap().schema,
    );
}

#[test]
fn an_ansible_task_file_is_read_by_its_reader() {
    agrees(
        "ansible",
        "ansible/upstream/tasks/includes_provisioning_tasks_apache.yml",
        &["--ansible-class", "tasks"],
        |b| ansible::read_ansible(b, FileClass::Tasks).unwrap().schema,
    );
}

#[test]
fn an_ansible_file_without_its_class_is_refused() {
    let path = corpus().join("ansible/upstream/tasks/includes_provisioning_tasks_apache.yml");
    let output = Command::cargo_bin("schema")
        .unwrap()
        .args(["validate", "--protocol", "ansible"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("needs the file's class"), "{stderr}");
}

#[test]
fn a_postgresql_script_is_read_by_its_reader() {
    agrees(
        "postgresql",
        "postgresql-docs/cases/create-table-examples-01.sql",
        &[],
        |b| {
            postgresql::read_ddl(std::str::from_utf8(b).unwrap(), OtherStatements::Refuse)
                .unwrap()
                .schema
        },
    );
}

#[test]
fn compat_reads_two_openapi_documents() {
    let path = corpus().join("openapi/upstream/pass/minimal_paths.yaml");
    Command::cargo_bin("schema")
        .unwrap()
        .args(["compat", "--protocol", "openapi"])
        .arg(&path)
        .arg(&path)
        .assert()
        .code(0);
}

#[test]
fn bson_reads_no_schema_documents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.json");
    std::fs::write(&path, r#"{"$jsonSchema": {"bsonType": "object"}}"#).unwrap();
    let output = Command::cargo_bin("schema")
        .unwrap()
        .args(["validate", "--protocol", "bson"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("reads no schema documents"), "{stderr}");
}
