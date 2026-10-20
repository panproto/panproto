//! A schema read from a JSON document does not depend on the order in which
//! the document writes an object's members.
//!
//! JSON gives an object's members no order (RFC 8259, Sec. 4), but a schema
//! keeps the order it was built in, and its identity, the bytes of
//! `panproto_schema::canonical_bytes`, covers that order. `serde_json` sorts
//! an object's members unless its `preserve_order` feature is on; with the
//! feature on, a parser that walks members as the map yields them builds a
//! different schema from each ordering of one document. Each test below
//! reads documents in their written order and with every object's members
//! reversed, and requires the two schemas to have the same bytes.
//!
//! In a default build both orderings reach the parser sorted, so the tests
//! hold whatever the parser does. Run them with the feature on to make the
//! reversal reach the parser:
//!
//! ```sh
//! cargo nextest run -p panproto-protocols --features serde_json/preserve_order \
//!     --test a_schema_does_not_depend_on_member_order
//! ```
//!
//! `in_this_build_member_order_is` reports which case a run is in.

use std::path::{Path, PathBuf};

use panproto_protocols::{parse_schema_bundle, parse_schema_document};
use panproto_schema::canonical_bytes;
use serde_json::{Map, Value};

/// Whether this build's `serde_json` keeps the order members were
/// inserted in.
fn members_keep_insertion_order() -> bool {
    let mut map = Map::new();
    map.insert("b".to_owned(), Value::Null);
    map.insert("a".to_owned(), Value::Null);
    map.keys().next().is_some_and(|first| first == "b")
}

/// `value` with every object's members, at every depth, inserted in the
/// reverse of the order the object yields them.
fn reversed(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(reversed).collect()),
        Value::Object(members) => {
            let mut out = Map::new();
            for (key, member) in members.iter().rev() {
                out.insert(key.clone(), reversed(member));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn read(relative: &str) -> Value {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{relative} is not JSON: {e}"))
}

fn lexicons() -> Vec<(String, Value)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/atproto/lexicons");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("listing {}: {e}", dir.display()))
        .map(|entry| {
            entry
                .unwrap_or_else(|e| panic!("listing {}: {e}", dir.display()))
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no lexicons under {}", dir.display());
    names
        .into_iter()
        .map(|name| {
            let doc = read(&format!("atproto/lexicons/{name}"));
            (name, doc)
        })
        .collect()
}

/// One schema document of each of several document protocols, with an
/// object of more than one member at every level that has one.
fn samples() -> Vec<(&'static str, Value)> {
    vec![
        (
            "avro",
            serde_json::json!({
                "type": "record",
                "name": "User",
                "namespace": "com.example",
                "fields": [
                    { "name": "id", "type": "long" },
                    {
                        "name": "address",
                        "type": {
                            "type": "record",
                            "name": "Address",
                            "fields": [
                                { "name": "zip", "type": "int" },
                                { "name": "city", "type": "string" }
                            ]
                        }
                    }
                ]
            }),
        ),
        (
            "openapi",
            serde_json::json!({
                "openapi": "3.0.0",
                "info": { "title": "t", "version": "1" },
                "paths": {
                    "/users": { "get": { "responses": { "200": { "description": "ok" } } } },
                    "/items": { "post": { "responses": { "201": { "description": "made" } } } }
                },
                "components": {
                    "schemas": {
                        "User": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" },
                                "age": { "type": "integer" },
                                "email": { "type": "string" }
                            }
                        },
                        "Item": {
                            "type": "object",
                            "properties": { "sku": { "type": "string" }, "qty": { "type": "integer" } }
                        }
                    }
                }
            }),
        ),
        (
            "dataframe",
            serde_json::json!({
                "name": "users",
                "columns": {
                    "name": { "dtype": "string" },
                    "age": { "dtype": "int64" },
                    "zip": { "dtype": "int32" }
                }
            }),
        ),
        (
            "swift-mt",
            serde_json::json!({
                "messages": {
                    "MT103": {
                        "blocks": {
                            "block4": {
                                "fields": {
                                    "20": { "type": "string", "tag": ":20:" },
                                    "32A": { "type": "amount", "tag": ":32A:" },
                                    "50K": { "type": "string", "tag": ":50K:" }
                                }
                            }
                        }
                    },
                    "MT202": {
                        "blocks": {
                            "block4": {
                                "fields": { "21": { "type": "string", "tag": ":21:" } }
                            }
                        }
                    }
                }
            }),
        ),
    ]
}

/// The protocol, document pairs whose two orderings give different bytes.
fn order_dependent<'a>(
    cases: impl IntoIterator<Item = (&'a str, String, &'a Value)>,
) -> Vec<String> {
    let mut wrong = Vec::new();
    for (protocol, what, doc) in cases {
        let written = parse_schema_document(protocol, doc)
            .unwrap_or_else(|e| panic!("{protocol} {what}: {e}"));
        let flipped = parse_schema_document(protocol, &reversed(doc))
            .unwrap_or_else(|e| panic!("{protocol} {what}, members reversed: {e}"));
        if canonical_bytes(&written) != canonical_bytes(&flipped) {
            wrong.push(format!("{protocol} {what}"));
        }
    }
    wrong
}

#[test]
fn in_this_build_member_order_is() {
    // Not an assertion: the line says whether the tests below reach the
    // parsers with the members reversed (insertion order kept) or sorted.
    eprintln!(
        "serde_json keeps insertion order: {}",
        members_keep_insertion_order()
    );
}

#[test]
fn every_lexicon_reads_to_one_schema_in_any_member_order() {
    let docs = lexicons();
    let wrong = order_dependent(
        docs.iter()
            .map(|(name, doc)| ("atproto", name.clone(), doc)),
    );
    assert!(
        wrong.is_empty(),
        "member order changed the schema of: {wrong:?}"
    );
}

#[test]
fn a_lexicon_bundle_reads_to_one_schema_in_any_member_order() {
    let docs: Vec<Value> = lexicons().into_iter().map(|(_, doc)| doc).collect();
    let flipped: Vec<Value> = docs.iter().map(reversed).collect();
    let written = parse_schema_bundle("atproto", &docs).unwrap_or_else(|e| panic!("{e}"));
    let other = parse_schema_bundle("atproto", &flipped).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        canonical_bytes(&written) == canonical_bytes(&other),
        "member order changed the schema of the lexicon bundle"
    );
}

#[test]
fn every_json_schema_fixture_reads_to_one_schema_in_any_member_order() {
    let docs: Vec<(String, Value)> = ["github-workflow.json", "package.json", "tsconfig.json"]
        .into_iter()
        .map(|name| (name.to_owned(), read(&format!("jsonschema/{name}"))))
        .collect();
    let wrong = order_dependent(
        docs.iter()
            .map(|(name, doc)| ("json-schema", name.clone(), doc)),
    );
    assert!(
        wrong.is_empty(),
        "member order changed the schema of: {wrong:?}"
    );
}

#[test]
fn every_sample_reads_to_one_schema_in_any_member_order() {
    let docs = samples();
    let wrong = order_dependent(
        docs.iter()
            .map(|(protocol, doc)| (*protocol, "sample".to_owned(), doc)),
    );
    assert!(
        wrong.is_empty(),
        "member order changed the schema of: {wrong:?}"
    );
}
