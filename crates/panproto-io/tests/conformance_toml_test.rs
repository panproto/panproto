//! The TOML preserving route against the vendored toml-test corpus.
//!
//! Every case of the TOML 1.0.0 list is run. A valid case passes when the
//! route reads it, its tree equals the case's tagged JSON, and an unedited
//! emission reproduces its bytes; an invalid case passes when the route
//! refuses it. Every other outcome is a declared limit: the case is listed
//! in `toml-test-limits.txt` with the class the text-formats page states,
//! and the test checks the list is exactly the set of cases that do not
//! pass.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use panproto_inst::value::{FieldPresence, Temporal, Value};
use panproto_inst::{NodeShape, WInstance};
use panproto_io::unified_codec::UnifiedCodec;
use panproto_schema::{Protocol, Schema, SchemaBuilder};
use serde_json::Value as Json;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance/toml-test")
}

fn open_schema() -> Schema {
    let proto = Protocol {
        name: "toml".into(),
        schema_theory: "ThTomlSchema".into(),
        instance_theory: "ThTomlInstance".into(),
        ..Default::default()
    };
    SchemaBuilder::new(&proto)
        .vertex("root", "object", None)
        .expect("vertex")
        .build()
        .expect("schema")
}

/// The tagged JSON toml-test writes for a node.
fn tagged(instance: &WInstance, node_id: u32) -> Json {
    let node = &instance.nodes[&node_id];
    let children = || {
        instance
            .arcs
            .iter()
            .filter(move |(parent, _, _)| *parent == node_id)
    };
    match &node.value {
        Some(FieldPresence::Present(value)) => scalar(value),
        Some(other) => Json::String(format!("unexpected presence {other:?}")),
        None if node.shape == NodeShape::List => Json::Array(
            children()
                .map(|(_, child, _)| tagged(instance, *child))
                .collect(),
        ),
        None => Json::Object(
            children()
                .map(|(_, child, edge)| {
                    (
                        edge.name.as_deref().unwrap_or_default().to_owned(),
                        tagged(instance, *child),
                    )
                })
                .collect(),
        ),
    }
}

fn tag(kind: &str, value: String) -> Json {
    serde_json::json!({"type": kind, "value": value})
}

fn temporal_kind(temporal: &Temporal) -> &'static str {
    match temporal {
        Temporal::Date(_) => "date-local",
        Temporal::Time(_, offset) if !offset.is_zoned() => "time-local",
        Temporal::DateTime(_, _, offset, _) if offset.is_zoned() => "datetime",
        Temporal::DateTime(..) => "datetime-local",
        _ => "temporal",
    }
}

fn scalar(value: &Value) -> Json {
    match value {
        Value::Str(text) => tag("string", text.clone()),
        Value::Bool(flag) => tag("bool", flag.to_string()),
        Value::Int(number) => tag("integer", number.to_string()),
        Value::BigInt(number) => tag("integer", number.to_string()),
        Value::Float(number) => tag("float", float_text(*number)),
        Value::Decimal(number) => tag(
            "float",
            float_text(number.to_string().parse::<f64>().unwrap_or(f64::NAN)),
        ),
        Value::Temporal(temporal) => tag(
            temporal_kind(temporal),
            normalize_temporal(&temporal.to_string()),
        ),
        other => tag("unexpected", format!("{other:?}")),
    }
}

/// A float as a comparable spelling: `nan`, `inf`, `-inf`, or the shortest
/// decimal, with the sign of a zero kept.
fn float_text(number: f64) -> String {
    if number.is_nan() {
        "nan".to_owned()
    } else if number.is_infinite() {
        if number > 0.0 { "inf" } else { "-inf" }.to_owned()
    } else {
        format!("{number:?}")
    }
}

/// Uppercase the separator and zone letter, write a space separator as
/// `T`, and drop trailing zeros of a fractional second, so a temporal and
/// toml-test's JSON spelling of it compare equal exactly when they denote
/// the same time.
fn normalize_temporal(text: &str) -> String {
    let text = text
        .replacen(' ', "T", 1)
        .replace('t', "T")
        .replace('z', "Z");
    let Some(dot) = text.find('.') else {
        return text;
    };
    let digits = text[dot + 1..]
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    let fraction = text[dot + 1..dot + 1 + digits].trim_end_matches('0');
    let point = if fraction.is_empty() { "" } else { "." };
    format!(
        "{}{point}{fraction}{}",
        &text[..dot],
        &text[dot + 1 + digits..]
    )
}

/// The expected tagged JSON with floats and temporals in the same spelling
/// as [`scalar`].
fn normalize_expected(json: &Json) -> Json {
    match json {
        Json::Object(members)
            if members.len() == 2
                && members.contains_key("type")
                && members.contains_key("value") =>
        {
            let kind = members["type"].as_str().unwrap_or_default();
            let value = members["value"].as_str().unwrap_or_default();
            match kind {
                "float" => tag(
                    "float",
                    match value {
                        "nan" | "+nan" | "-nan" => "nan".to_owned(),
                        "inf" | "+inf" => "inf".to_owned(),
                        "-inf" => "-inf".to_owned(),
                        other => float_text(other.parse::<f64>().unwrap_or(f64::NAN)),
                    },
                ),
                "datetime" | "datetime-local" | "date-local" | "time-local" => {
                    tag(kind, normalize_temporal(value))
                }
                _ => json.clone(),
            }
        }
        Json::Object(members) => Json::Object(
            members
                .iter()
                .map(|(key, value)| (key.clone(), normalize_expected(value)))
                .collect(),
        ),
        Json::Array(items) => Json::Array(items.iter().map(normalize_expected).collect()),
        other => other.clone(),
    }
}

/// The outcome of one case: `None` when it passes, otherwise what happened.
fn run_case(codec: &UnifiedCodec, schema: &Schema, relative: &str) -> Option<String> {
    let path = corpus().join("tests").join(relative);
    let bytes = std::fs::read(&path).expect("a corpus file");
    let parsed = codec.parse_wtype_preserving(schema, &bytes);
    if relative.starts_with("invalid/") {
        return parsed.is_ok().then(|| "accepted".to_owned());
    }
    let (instance, complement) = match parsed {
        Ok(read) => read,
        Err(error) => return Some(format!("refused: {error}")),
    };
    let expected: Json = serde_json::from_slice(
        &std::fs::read(path.with_extension("json")).expect("the case's JSON"),
    )
    .expect("valid JSON");
    let read = tagged(&instance, instance.root);
    if read != normalize_expected(&expected) {
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

/// The declared limits: case path, then the class the book states.
fn declared_limits() -> BTreeMap<String, String> {
    std::fs::read_to_string(corpus().join("../toml-test-limits.txt"))
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (case, class) = line.split_once(' ').expect("a case and its class");
            (case.to_owned(), class.trim().to_owned())
        })
        .collect()
}

#[test]
fn toml_test_cases_pass_or_are_declared_limits() {
    let codec = UnifiedCodec::toml("toml").expect("TOML codec");
    let schema = open_schema();
    let list = std::fs::read_to_string(corpus().join("tests/files-toml-1.0.0")).unwrap();
    let cases: Vec<&str> = list
        .lines()
        .filter(|line| {
            std::path::Path::new(line)
                .extension()
                .is_some_and(|ext| ext == "toml")
        })
        .collect();
    let limits = declared_limits();
    let mut undeclared = Vec::new();
    let mut failing = std::collections::BTreeSet::new();
    for case in &cases {
        if let Some(outcome) = run_case(&codec, &schema, case) {
            failing.insert((*case).to_owned());
            // The grammar's one limit shows as parser recovery: the
            // tree-sitter TOML grammar cannot lex the string.
            let observed = if outcome.contains("parser recovery marker") {
                "line-ending-backslash-before-whitespace"
            } else {
                "unclassified"
            };
            if limits.get(*case).map(String::as_str) != Some(observed) {
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

/// The declared limit `line-ending-backslash-before-whitespace`: in a
/// multi-line basic string, a line-ending backslash followed by spaces or
/// tabs before the line break is refused with a parse error, while the
/// same backslash directly before the line break is read.
#[test]
fn a_line_ending_backslash_before_whitespace_is_refused() {
    let codec = UnifiedCodec::toml("toml").expect("TOML codec");
    let schema = open_schema();
    for input in ["a = \"\"\"x\\  \ny\"\"\"\n", "a = \"\"\"x\\\t\ny\"\"\"\n"] {
        assert!(
            matches!(
                codec.parse_wtype_preserving(&schema, input.as_bytes()),
                Err(panproto_io::ParseInstanceError::Parse { .. })
            ),
            "{input:?}"
        );
    }
    let (instance, _) = codec
        .parse_wtype_preserving(&schema, b"a = \"\"\"x\\\n   y\"\"\"\n")
        .expect("a backslash directly before the line break");
    assert_eq!(
        tagged(&instance, instance.root),
        serde_json::json!({"a": {"type": "string", "value": "xy"}})
    );
}
