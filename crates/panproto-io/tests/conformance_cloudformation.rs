//! The template reader against the vendored `CloudFormation` corpus.
//!
//! Every template under `cloudformation/upstream` (taken unchanged from
//! the aws-cloudformation-templates repository) must read, and its
//! canonical text must read back as the same schema and write as the same
//! text, unless `cloudformation/MANIFEST` lists it with the refusal it
//! draws. Every case under `cloudformation/invalid`, built from one rule
//! of the template format and listed in the MANIFEST, must be refused with
//! that refusal at that path. A case that does not pass is a declared limit
//! listed in `cloudformation-limits.txt`, and the test checks the list is
//! exactly the set of failing cases.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use panproto_io::config::cloudformation::{CfnErrorKind, read_template, write_template};
use panproto_schema::Schema;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance")
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

/// The outcome of one upstream template that must read: `None` when it
/// passes.
fn upstream_case(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).unwrap();
    let reading = match read_template(&bytes) {
        Ok(reading) => reading,
        Err(error) => return Some(format!("refused: {error}")),
    };
    let text = match write_template(&reading.schema) {
        Ok(text) => text,
        Err(error) => return Some(format!("write refused: {error}")),
    };
    let again = match read_template(text.as_bytes()) {
        Ok(again) => again,
        Err(error) => return Some(format!("canonical text refused: {error}")),
    };
    if rendering(&again.schema) != rendering(&reading.schema) {
        return Some("canonical text reads as another schema".into());
    }
    if write_template(&again.schema).ok().as_deref() != Some(text.as_str()) {
        return Some("canonical text is not a fixed point".into());
    }
    None
}

/// The MANIFEST lines: file, path, refusal class.
fn manifest() -> Vec<(String, String, String)> {
    std::fs::read_to_string(corpus().join("cloudformation/MANIFEST"))
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let fields: Vec<&str> = l.split('\t').collect();
            (
                fields[0].to_owned(),
                fields[1].to_owned(),
                fields[2].to_owned(),
            )
        })
        .collect()
}

fn class(kind: &CfnErrorKind) -> String {
    let debug = format!("{kind:?}");
    debug
        .split(['(', ' '])
        .next()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn cloudformation_corpus_cases_pass_or_are_declared_limits() {
    let cases = manifest();
    let mut failing = BTreeMap::new();
    let mut upstream: Vec<PathBuf> = std::fs::read_dir(corpus().join("cloudformation/upstream"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "yaml" || x == "json"))
        .collect();
    upstream.sort();
    assert_eq!(upstream.len(), 17, "the upstream corpus has 17 templates");
    for path in &upstream {
        let name = format!("upstream/{}", path.file_name().unwrap().to_string_lossy());
        if cases.iter().any(|(file, ..)| *file == name) {
            continue;
        }
        if let Some(outcome) = upstream_case(path) {
            failing.insert(name, outcome);
        }
    }
    assert_eq!(cases.len(), 21, "the MANIFEST lists 21 refusals");
    for (file, path, expected) in &cases {
        let bytes = std::fs::read(corpus().join("cloudformation").join(file)).unwrap();
        match read_template(&bytes) {
            Ok(_) => {
                failing.insert(file.clone(), "read".into());
            }
            Err(error) if error.path == *path && class(&error.kind) == *expected => {}
            Err(error) => {
                failing.insert(
                    file.clone(),
                    format!(
                        "refused as {} at {}: {error}",
                        class(&error.kind),
                        error.path
                    ),
                );
            }
        }
    }
    let declared: BTreeMap<String, String> =
        std::fs::read_to_string(corpus().join("cloudformation-limits.txt"))
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
fn a_short_form_tag_on_a_sequence_is_refused_by_name_and_offset() {
    let source = b"Resources:\n  T:\n    Type: AWS::SNS::Topic\n    Properties:\n      A: !Join [\"\", [x]]\n";
    let error = read_template(source).unwrap_err();
    assert_eq!(error.kind, CfnErrorKind::TaggedCollection("!Join".into()));
    assert_eq!(error.offset, 67);
}

#[test]
fn scalar_short_forms_read_as_their_full_forms() {
    let short = b"Parameters:\n  P:\n    Type: String\nResources:\n  T:\n    Type: AWS::SNS::Topic\n    Properties:\n      A: !Ref P\n      B: !Sub \"${AWS::Region}\"\n      C: !GetAtt T.TopicName\n";
    let full = b"{\"Parameters\":{\"P\":{\"Type\":\"String\"}},\"Resources\":{\"T\":{\"Type\":\"AWS::SNS::Topic\",\"Properties\":{\"A\":{\"Ref\":\"P\"},\"B\":{\"Fn::Sub\":\"${AWS::Region}\"},\"C\":{\"Fn::GetAtt\":\"T.TopicName\"}}}}}";
    let short = read_template(short).unwrap();
    let full = read_template(full).unwrap();
    assert_eq!(rendering(&short.schema), rendering(&full.schema));
    assert_eq!(
        write_template(&short.schema).unwrap(),
        String::from_utf8(full_text()).unwrap()
    );
}

fn full_text() -> Vec<u8> {
    b"{\"Parameters\":{\"P\":{\"Type\":\"String\"}},\"Resources\":{\"T\":{\"Type\":\"AWS::SNS::Topic\",\"Properties\":{\"A\":{\"Ref\":\"P\"},\"B\":{\"Fn::Sub\":\"${AWS::Region}\"},\"C\":{\"Fn::GetAtt\":\"T.TopicName\"}}}}}\n".to_vec()
}

#[test]
fn an_import_is_reported_unresolved_with_its_path() {
    let source = b"Resources:\n  T:\n    Type: AWS::SNS::Topic\n    Properties:\n      A: !ImportValue shared-topic\n";
    let reading = read_template(source).unwrap();
    assert_eq!(reading.imports.len(), 1);
    assert_eq!(
        reading.imports[0].path,
        "Resources.T.Properties.A.Fn::ImportValue"
    );
    assert_eq!(reading.imports[0].name.as_deref(), Some("shared-topic"));
}
