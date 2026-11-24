//! The YAML preserving route against the vendored yaml-test-suite data.
//!
//! A case with an `error` file passes when the route refuses it. A case
//! with an `in.json` holding one JSON document passes when the route reads
//! `in.yaml`, its tree equals that document, and an unedited emission
//! reproduces the bytes. Every other outcome is a declared limit listed in
//! `yaml-test-suite-limits.txt` with the class the text-formats page
//! states, and the test checks the list is exactly the set of cases that
//! do not pass, each with the class its outcome shows.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{NodeShape, WInstance};
use panproto_io::unified_codec::UnifiedCodec;
use panproto_schema::{Protocol, Schema, SchemaBuilder};
use serde_json::Value as Json;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance/yaml-test-suite")
}

fn open_schema() -> Schema {
    let proto = Protocol {
        name: "yaml".into(),
        schema_theory: "ThYamlSchema".into(),
        instance_theory: "ThYamlInstance".into(),
        ..Default::default()
    };
    SchemaBuilder::new(&proto)
        .vertex("root", "object", None)
        .expect("vertex")
        .build()
        .expect("schema")
}

/// The JSON data a node reads as, numbers as `f64` so that `1` and `1.0`
/// compare equal as JSON numbers do.
fn json_of(instance: &WInstance, node_id: u32) -> Json {
    let node = &instance.nodes[&node_id];
    let children = || {
        instance
            .arcs
            .iter()
            .filter(move |(parent, _, _)| *parent == node_id)
    };
    match &node.value {
        Some(FieldPresence::Present(value)) => scalar(value),
        Some(FieldPresence::Null) => Json::Null,
        Some(other) => Json::String(format!("unexpected presence {other:?}")),
        None if node.shape == NodeShape::List => Json::Array(
            children()
                .map(|(_, child, _)| json_of(instance, *child))
                .collect(),
        ),
        None => Json::Object(
            children()
                .map(|(_, child, edge)| {
                    (
                        edge.name.as_deref().unwrap_or_default().to_owned(),
                        json_of(instance, *child),
                    )
                })
                .collect(),
        ),
    }
}

fn number(value: f64) -> Json {
    serde_json::Number::from_f64(value)
        .map_or_else(|| Json::String(format!("{value}")), Json::Number)
}

fn scalar(value: &Value) -> Json {
    match value {
        Value::Null => Json::Null,
        Value::Str(text) => Json::String(text.clone()),
        Value::Bool(flag) => Json::Bool(*flag),
        Value::Int(integer) => number(integer.to_string().parse().unwrap_or(f64::NAN)),
        Value::BigInt(big) => number(big.to_string().parse().unwrap_or(f64::NAN)),
        Value::Float(float) => number(*float),
        Value::Decimal(decimal) => number(decimal.to_string().parse().unwrap_or(f64::NAN)),
        other => Json::String(format!("unexpected value {other:?}")),
    }
}

fn normalize(json: &Json) -> Json {
    match json {
        Json::Number(n) => number(n.as_f64().unwrap_or(f64::NAN)),
        Json::Array(items) => Json::Array(items.iter().map(normalize).collect()),
        Json::Object(members) => Json::Object(
            members
                .iter()
                .map(|(key, value)| (key.clone(), normalize(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Every case directory holding an `in.yaml`, as a path relative to the
/// corpus root.
fn cases() -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![corpus()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|name| name == "in.yaml") {
                let case = path.parent().unwrap().strip_prefix(corpus()).unwrap();
                out.push(case.to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    out
}

/// The outcome of one case: `None` when it passes, otherwise what happened.
fn run_case(codec: &UnifiedCodec, schema: &Schema, case: &str) -> Option<String> {
    let dir = corpus().join(case);
    let bytes = std::fs::read(dir.join("in.yaml")).unwrap();
    let parsed = codec.parse_wtype_preserving(schema, &bytes);
    if dir.join("error").exists() {
        return parsed
            .is_ok()
            .then(|| "accepted an invalid stream".to_owned());
    }
    let Ok(json_bytes) = std::fs::read(dir.join("in.json")) else {
        return Some(match parsed {
            Ok(_) => "read a stream with no JSON form".to_owned(),
            Err(error) => format!("refused a stream with no JSON form: {error}"),
        });
    };
    let documents: Vec<Json> = serde_json::Deserializer::from_slice(&json_bytes)
        .into_iter::<Json>()
        .collect::<Result<_, _>>()
        .expect("valid JSON");
    let (instance, complement) = match parsed {
        Ok(read) => read,
        Err(error) => return Some(format!("refused: {error}")),
    };
    let [expected] = documents.as_slice() else {
        return Some(format!("read a stream of {} documents", documents.len()));
    };
    let read = json_of(&instance, instance.root);
    if read != normalize(expected) {
        return Some(format!("read {read} expected {expected}"));
    }
    match codec.emit_wtype_preserving(schema, &instance, &complement) {
        Ok(emitted) if emitted == bytes => None,
        Ok(emitted) => Some(format!(
            "unedited emission wrote {:?}",
            String::from_utf8_lossy(&emitted)
        )),
        Err(error) => Some(format!("emission refused: {error}")),
    }
}

/// The declared limits: case, then the class the book states.
fn declared_limits() -> BTreeMap<String, String> {
    std::fs::read_to_string(corpus().join("../yaml-test-suite-limits.txt"))
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (case, class) = line.split_once(' ').expect("a case and its class");
            (case.to_owned(), class.trim().to_owned())
        })
        .collect()
}

/// The limit class an outcome shows, as the text-formats page names it.
fn limit_class(outcome: &str) -> &'static str {
    const CLASSES: [(&str, &str); 13] = [
        ("with no JSON form", "no-json-form"),
        ("aliases require anchor resolution", "alias"),
        ("documents; exactly one is required", "multiple-documents"),
        ("more than one document", "multiple-documents"),
        ("stream has no document", "no-document"),
        ("%TAG directive", "tag-directive"),
        ("this route reads YAML 1.2 only", "version-directive"),
        ("cannot represent YAML tag", "container-tag"),
        ("is not a scalar", "complex-key"),
        ("is not valid YAML", "decoder-refuses"),
        (
            "has a structure this route does not read",
            "structure-not-read",
        ),
        ("is outside what this route reads", "decoder-disagrees"),
        ("accepted an invalid stream", "accepts-invalid"),
    ];
    if outcome.contains("Opaque") && outcome.contains("!foo") {
        return "local-tag-kept";
    }
    if outcome.contains("Bytes(") {
        return "binary-tag-read-as-bytes";
    }
    CLASSES
        .iter()
        .find(|(pattern, _)| outcome.contains(pattern))
        .map_or("unclassified", |(_, class)| class)
}

#[test]
fn yaml_test_suite_cases_pass_or_are_declared_limits() {
    let codec = UnifiedCodec::yaml("yaml").expect("YAML codec");
    let schema = open_schema();
    let cases = cases();
    let limits = declared_limits();
    let mut undeclared = Vec::new();
    let mut failing = BTreeSet::new();
    for case in &cases {
        if let Some(outcome) = run_case(&codec, &schema, case) {
            failing.insert(case.clone());
            let observed = limit_class(&outcome);
            if limits.get(case).map(String::as_str) != Some(observed) {
                undeclared.push(format!("{case} ({observed}): {outcome}"));
            }
        }
    }
    let stale: Vec<_> = limits
        .keys()
        .filter(|case| !failing.contains(*case))
        .collect();
    assert!(
        undeclared.is_empty() && stale.is_empty(),
        "{} cases; undeclared:\n{}\nstale limits: {stale:?}",
        cases.len(),
        undeclared.join("\n")
    );
}
