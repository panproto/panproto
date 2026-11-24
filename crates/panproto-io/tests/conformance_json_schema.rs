//! The JSON Schema reader against the vendored JSON-Schema-Test-Suite.
//!
//! Every test case of a vendored file that is compatible with the 2020-12
//! dialect (by the suite's `compatibility` rule) is read: its schema is the
//! first document of a bundle whose further documents are the case's
//! `externalSchemas` at their retrieval URIs. The reading must validate
//! against the `json-schema` protocol, each document's canonical text must
//! read back, at the same retrieval URI, as the same schema, and writing it
//! again must give the same text. A case that does not pass is a declared
//! limit in `json-schema-test-suite-limits.txt`, whose refusal must be the
//! one its limit names; the test checks the list is exactly the set of
//! failing cases. The suite's instance tests check a validator's verdicts,
//! which a reading does not give, and the limits file declares them as one
//! class whose size the test pins.

#![cfg(feature = "tree-sitter")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use panproto_io::config::tree::{Node, read_json};
use panproto_io::data_schema::json_schema::{
    BundleDocument, JsonSchemaErrorKind, JsonSchemaReading, read_bundle, read_trees, write_document,
};
use panproto_schema::Schema;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance")
}

fn text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn member<'n>(node: &'n Node, key: &str) -> Option<&'n Node> {
    node.mapping()?
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}

/// Whether a case's `compatibility` admits 2020-12, by the rule of the
/// suite's `validation/README.md`.
fn compatible(compatibility: Option<&str>) -> bool {
    let Some(compatibility) = compatibility else {
        return true;
    };
    let under_test = 2020;
    let mut valid = true;
    for constraint in compatibility.split(',') {
        let constraint = constraint.trim();
        let (operator, release) = constraint.strip_prefix("<=").map_or_else(
            || {
                constraint
                    .strip_prefix('=')
                    .map_or(("", constraint), |r| ("=", r))
            },
            |r| ("<=", r),
        );
        let release: i32 = release
            .parse()
            .unwrap_or_else(|e| panic!("{compatibility}: {e}"));
        valid = match operator {
            "" => under_test >= release,
            "<=" => under_test <= release,
            _ => under_test == release,
        };
        if under_test <= release {
            break;
        }
    }
    valid
}

/// The schema as sorted renderings of its vertices, ordered out-edges,
/// constraints, required edges and entries.
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
    for (id, edges) in &schema.required {
        let mut names: Vec<String> = edges.iter().map(|e| format!("{:?}", e.name)).collect();
        names.sort();
        out.push(format!("required {id} {names:?}"));
    }
    out.sort();
    out.push(format!("entries {:?}", schema.entries));
    out
}

/// The outcome of a bundle that reads: `None` when it passes.
fn round_trip(documents: &[(Option<&str>, &Node)], reading: &JsonSchemaReading) -> Option<String> {
    let protocol = panproto_protocols::data_schema::json_schema::protocol();
    let errors = panproto_schema::validate(&reading.schema, &protocol);
    if !errors.is_empty() {
        return Some(format!("the reading does not validate: {errors:?}"));
    }
    let mut texts = Vec::new();
    for root in &reading.roots {
        match write_document(&reading.schema, root) {
            Ok(text) => texts.push(text),
            Err(error) => return Some(format!("write refused: {error}")),
        }
    }
    let again: Vec<BundleDocument<'_>> = documents
        .iter()
        .zip(&texts)
        .map(|((uri, _), text)| BundleDocument {
            retrieval_uri: *uri,
            source: text.as_bytes(),
        })
        .collect();
    let back = match read_bundle(&again) {
        Ok(back) => back,
        Err(error) => return Some(format!("canonical text refused: {error}")),
    };
    if rendering(&back.schema) != rendering(&reading.schema) || back.roots != reading.roots {
        return Some("canonical text reads as another schema".into());
    }
    let rewritten: Vec<Option<String>> = back
        .roots
        .iter()
        .map(|root| write_document(&back.schema, root).ok())
        .collect();
    if rewritten
        .iter()
        .zip(&texts)
        .any(|(again, text)| again.as_deref() != Some(text.as_str()))
    {
        return Some("canonical text is not a fixed point".into());
    }
    None
}

/// The limit a refusal's kind falls under.
const fn limit_of(kind: &JsonSchemaErrorKind) -> &'static str {
    match kind {
        JsonSchemaErrorKind::DynamicScope(_) => "dynamic-scope",
        JsonSchemaErrorKind::UnsupportedDialect(_) => "custom-metaschema",
        _ => "refused",
    }
}

/// The MANIFEST lines: file, 2020-12 cases, instance tests.
fn manifest() -> Vec<(String, usize, usize)> {
    text(&corpus().join("json-schema-test-suite/MANIFEST"))
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let fields: Vec<&str> = l.split('\t').collect();
            let count =
                |i: usize| -> usize { fields[i].parse().unwrap_or_else(|e| panic!("{l}: {e}")) };
            (fields[0].to_owned(), count(1), count(2))
        })
        .collect()
}

#[test]
fn json_schema_test_suite_cases_pass_or_are_declared_limits() {
    let files = manifest();
    assert_eq!(files.len(), 59, "the MANIFEST lists 59 files");
    let mut failing = BTreeMap::new();
    let (mut cases, mut instances) = (0, 0);
    for (file, expected_cases, expected_instances) in &files {
        let bytes = std::fs::read(corpus().join("json-schema-test-suite/tests").join(file))
            .unwrap_or_else(|e| panic!("{file}: {e}"));
        let tree = read_json(&bytes).unwrap_or_else(|e| panic!("{file}: {e}"));
        let tests = member(&tree, "tests")
            .and_then(Node::sequence)
            .unwrap_or_else(|| panic!("{file} has a tests array"));
        let (mut file_cases, mut file_instances) = (0, 0);
        for (i, case) in tests.iter().enumerate() {
            if !compatible(member(case, "compatibility").and_then(Node::string)) {
                continue;
            }
            file_cases += 1;
            file_instances += member(case, "tests")
                .and_then(Node::sequence)
                .map_or(0, <[Node]>::len);
            let schema = member(case, "schema").unwrap_or_else(|| panic!("{file}#{i}"));
            let mut documents: Vec<(Option<&str>, &Node)> = vec![(None, schema)];
            for (uri, external) in member(case, "externalSchemas")
                .and_then(Node::mapping)
                .unwrap_or_default()
            {
                documents.push((Some(uri.as_str()), external));
            }
            let outcome = match read_trees(&documents) {
                Ok(reading) => round_trip(&documents, &reading).map(|o| ("failed", o)),
                Err(error) => Some((limit_of(&error.kind), error.to_string())),
            };
            if let Some(outcome) = outcome {
                failing.insert(format!("{file}#{i}"), outcome);
            }
        }
        assert_eq!(
            (file_cases, file_instances),
            (*expected_cases, *expected_instances),
            "{file}"
        );
        cases += file_cases;
        instances += file_instances;
    }
    assert_eq!((cases, instances), (430, 2082));

    let declared: BTreeMap<String, String> =
        text(&corpus().join("json-schema-test-suite-limits.txt"))
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .map(|l| {
                let (case, limit) = l
                    .split_once('\t')
                    .unwrap_or_else(|| panic!("{l} has a limit"));
                (case.to_owned(), limit.to_owned())
            })
            .collect();
    assert_eq!(
        declared.get("tests/*.json#*/tests").map(String::as_str),
        Some("instance-validation"),
        "the instance tests are declared"
    );
    let failed: BTreeMap<&String, &str> = failing.iter().map(|(c, (l, _))| (c, *l)).collect();
    let limits: BTreeMap<&String, &str> = declared
        .iter()
        .filter(|(case, _)| case.as_str() != "tests/*.json#*/tests")
        .map(|(c, l)| (c, l.as_str()))
        .collect();
    assert_eq!(
        failed, limits,
        "failing cases differ from the declared limits: {failing:#?}"
    );
    assert_eq!(limits.len(), 15, "15 cases are declared limits");
}
