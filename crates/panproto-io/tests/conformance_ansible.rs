//! The Ansible reader against the vendored corpus.
//!
//! Every file under `ansible/upstream/<class>` (taken unchanged from the
//! ansible-for-devops repository) must read under its class, and its
//! canonical text must read back as the same schema and write as the same
//! text, unless `ansible/MANIFEST` lists it with the refusal it draws.
//! Every case under `ansible/invalid`, built from one rule of ansible-core
//! and listed in the MANIFEST with its class, must be refused with that
//! refusal at that path. A case that does not pass is a declared limit
//! listed in `ansible-limits.txt`, and the test checks the list is exactly
//! the set of failing cases.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use panproto_io::config::ansible::{AnsibleErrorKind, FileClass, read_ansible, write_ansible};
use panproto_schema::Schema;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance")
}

fn class(name: &str) -> FileClass {
    match name {
        "playbook" => FileClass::Playbook,
        "tasks" => FileClass::Tasks,
        "vars" => FileClass::Vars,
        other => panic!("unknown class {other}"),
    }
}

/// The schema as sorted renderings of its vertices, ordered out-edges and
/// constraints.
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
    out
}

/// The outcome of one upstream file that must read: `None` when it passes.
fn upstream_case(path: &Path, file_class: FileClass) -> Option<String> {
    let bytes = std::fs::read(path).unwrap();
    let reading = match read_ansible(&bytes, file_class) {
        Ok(reading) => reading,
        Err(error) => return Some(format!("refused: {error}")),
    };
    let text = match write_ansible(&reading.schema) {
        Ok(text) => text,
        Err(error) => return Some(format!("write refused: {error}")),
    };
    let again = match read_ansible(text.as_bytes(), file_class) {
        Ok(again) => again,
        Err(error) => return Some(format!("canonical text refused: {error}")),
    };
    if rendering(&again.schema) != rendering(&reading.schema) {
        return Some("canonical text reads as another schema".into());
    }
    if write_ansible(&again.schema).ok().as_deref() != Some(text.as_str()) {
        return Some("canonical text is not a fixed point".into());
    }
    None
}

/// The MANIFEST lines: file, class, path, refusal class.
fn manifest() -> Vec<[String; 4]> {
    std::fs::read_to_string(corpus().join("ansible/MANIFEST"))
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            [
                f[0].to_owned(),
                f[1].to_owned(),
                f[2].to_owned(),
                f[3].to_owned(),
            ]
        })
        .collect()
}

fn kind_class(kind: &AnsibleErrorKind) -> String {
    let debug = format!("{kind:?}");
    debug
        .split(['(', ' '])
        .next()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn ansible_corpus_cases_pass_or_are_declared_limits() {
    let cases = manifest();
    let mut failing = BTreeMap::new();
    let mut count = 0;
    for dir in ["playbook", "tasks", "vars"] {
        let mut files: Vec<PathBuf> =
            std::fs::read_dir(corpus().join("ansible/upstream").join(dir))
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
        files.sort();
        for path in &files {
            count += 1;
            let name = format!(
                "upstream/{dir}/{}",
                path.file_name().unwrap().to_string_lossy()
            );
            if cases.iter().any(|[file, ..]| *file == name) {
                continue;
            }
            if let Some(outcome) = upstream_case(path, class(dir)) {
                failing.insert(name, outcome);
            }
        }
    }
    assert_eq!(count, 19, "the upstream corpus has 19 files");
    assert_eq!(cases.len(), 16, "the MANIFEST lists 16 refusals");
    for [file, file_class, path, expected] in &cases {
        let bytes = std::fs::read(corpus().join("ansible").join(file)).unwrap();
        match read_ansible(&bytes, class(file_class)) {
            Ok(_) => {
                failing.insert(file.clone(), "read".into());
            }
            Err(error) if error.path == *path && kind_class(&error.kind) == *expected => {}
            Err(error) => {
                failing.insert(
                    file.clone(),
                    format!(
                        "refused as {} at {}: {error}",
                        kind_class(&error.kind),
                        error.path
                    ),
                );
            }
        }
    }
    let declared: BTreeMap<String, String> =
        std::fs::read_to_string(corpus().join("ansible-limits.txt"))
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

#[test]
fn vault_and_unsafe_tags_read_as_ansible_writes_them_in_json() {
    let source = b"secret: !vault |\n  $ANSIBLE_VAULT;1.1;AES256\n  6162\nraw: !unsafe '{{ x }}'\n";
    let reading = read_ansible(source, FileClass::Vars).unwrap();
    let text = write_ansible(&reading.schema).unwrap();
    assert!(
        text.contains("{\"__ansible_vault\":\"$ANSIBLE_VAULT;1.1;AES256\\n6162\\n\"}"),
        "{text}"
    );
    assert!(
        text.contains("\"raw\":{\"__ansible_unsafe\":\"{{ x }}\"}"),
        "{text}"
    );
}

#[test]
fn a_task_file_reports_every_notify_as_an_unresolved_handler() {
    let source = b"- name: restart it\n  service: name=x state=restarted\n  notify: restart\n";
    let reading = read_ansible(source, FileClass::Tasks).unwrap();
    assert_eq!(reading.external.len(), 1);
    assert_eq!(reading.external[0].kind, "handler");
    assert_eq!(reading.external[0].path, "[0].notify");
}

#[test]
fn plain_yaml_1_1_booleans_and_octals_read_as_ansible_reads_them() {
    let source = b"a: yes\nb: 'yes'\nc: 0755\nd: '0755'\n";
    let reading = read_ansible(source, FileClass::Vars).unwrap();
    assert_eq!(
        write_ansible(&reading.schema).unwrap(),
        "{\"a\":true,\"b\":\"yes\",\"c\":493,\"d\":\"0755\"}\n"
    );
    let refused = read_ansible(b"when: 2001-12-14\n", FileClass::Vars).unwrap_err();
    assert_eq!(refused.path, "when");
}
