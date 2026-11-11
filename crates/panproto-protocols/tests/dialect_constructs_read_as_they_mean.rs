//! JSON Schema constructs whose meaning depends on the dialect read as
//! that dialect defines them, and survive an emit and re-parse.
//!
//! Each case is one construct: a `$id` ending in an empty
//! fragment, the draft-04 `id` keyword, 2020-12 `prefixItems` beside
//! `items`, and boolean `items` and `additionalItems`.

use panproto_protocols::data_schema::json_schema::{emit_json_schema, parse_json_schema};
use panproto_schema::Schema;
use serde_json::json;

/// A result a test requires to be `Ok`.
trait Required<T> {
    /// The value, or a panic naming `what` and the error.
    fn required(self, what: &str) -> T;
}

impl<T, E: std::fmt::Display> Required<T> for Result<T, E> {
    fn required(self, what: &str) -> T {
        self.unwrap_or_else(|e| panic!("{what}: {e}"))
    }
}

fn kind<'a>(schema: &'a Schema, vertex: &str) -> &'a str {
    schema
        .vertices
        .get(vertex)
        .unwrap_or_else(|| panic!("no vertex {vertex}"))
        .kind
        .as_ref()
}

/// The target of the one `ref` edge out of `src`.
fn ref_target<'a>(schema: &'a Schema, src: &str) -> &'a str {
    let refs: Vec<_> = schema
        .outgoing_edges(src)
        .iter()
        .filter(|e| e.kind == "ref")
        .collect();
    assert_eq!(refs.len(), 1, "one ref edge out of {src}: {refs:?}");
    refs[0].tgt.as_ref()
}

#[test]
fn a_ref_written_against_an_id_with_an_empty_fragment_resolves() {
    let schema = parse_json_schema(&json!({
        "$id": "http://x.test/s#",
        "definitions": { "a": { "type": "string" } },
        "type": "object",
        "properties": {
            "p": { "$ref": "http://x.test/s#/definitions/a" },
            "q": { "$ref": "http://x.test/s#" }
        }
    }))
    .required("schema parses");
    let root = schema
        .entries
        .iter()
        .find(|e| !e.contains(":definitions/"))
        .map(ToString::to_string)
        .required_some("the document has a root entry");
    let p = ref_target(&schema, &format!("{root}.p"));
    assert_eq!(kind(&schema, p), "string", "the ref reaches the definition");
    assert_eq!(
        ref_target(&schema, &format!("{root}.q")),
        root,
        "the ref reaches the root"
    );
}

#[test]
fn the_draft_04_id_keyword_is_the_document_identity() {
    let schema = parse_json_schema(&json!({
        "id": "http://x.test/d4",
        "definitions": { "a": { "type": "integer" } },
        "type": "object",
        "properties": { "p": { "$ref": "http://x.test/d4#/definitions/a" } }
    }))
    .required("schema parses");
    let p = ref_target(&schema, "http://x.test/d4.p");
    assert_eq!(
        kind(&schema, p),
        "integer",
        "the ref reaches the definition"
    );
}

#[test]
fn prefix_items_and_items_survive_emit() {
    let doc = json!({
        "type": "array",
        "prefixItems": [{ "type": "string" }, { "type": "boolean" }],
        "items": { "type": "integer" }
    });
    let emitted = emit_json_schema(&parse_json_schema(&doc).required("parse")).required("emit");
    assert_eq!(emitted["prefixItems"][0]["type"], "string", "{emitted}");
    assert_eq!(emitted["prefixItems"][1]["type"], "boolean", "{emitted}");
    assert_eq!(emitted["items"]["type"], "integer", "{emitted}");
}

#[test]
fn a_tuple_items_and_prefix_items_stay_apart() {
    let doc = json!({
        "type": "array",
        "items": [{ "type": "string" }],
        "prefixItems": [{ "type": "boolean" }]
    });
    let emitted = emit_json_schema(&parse_json_schema(&doc).required("parse")).required("emit");
    assert_eq!(emitted["items"][0]["type"], "string", "{emitted}");
    assert_eq!(emitted["prefixItems"][0]["type"], "boolean", "{emitted}");
}

#[test]
fn boolean_items_keep_their_meaning() {
    let none = parse_json_schema(&json!({ "type": "array", "items": false })).required("parse");
    assert!(
        !none.has_vertex("root:items"),
        "false admits no item schema"
    );
    let emitted = emit_json_schema(&none).required("emit");
    assert_eq!(emitted["items"], json!(false), "{emitted}");

    let any = parse_json_schema(&json!({ "type": "array", "items": true })).required("parse");
    assert!(!any.has_vertex("root:items"), "true is the default");
    let emitted = emit_json_schema(&any).required("emit");
    assert!(emitted.get("items").is_none(), "{emitted}");

    let closed = parse_json_schema(&json!({
        "type": "array",
        "items": [{ "type": "string" }],
        "additionalItems": false
    }))
    .required("parse");
    let emitted = emit_json_schema(&closed).required("emit");
    assert_eq!(emitted["items"][0]["type"], "string", "{emitted}");
    assert_eq!(emitted["additionalItems"], json!(false), "{emitted}");
}

#[test]
fn a_document_with_an_identity_emits_from_its_root() {
    for doc in [
        json!({
            "$schema": "http://json-schema.org/draft-04/schema#",
            "id": "http://x.test/d4",
            "type": "object",
            "properties": { "p": { "type": "string" } }
        }),
        json!({
            "$id": "http://x.test/d7",
            "type": "object",
            "properties": { "p": { "type": "string" } }
        }),
    ] {
        let schema = parse_json_schema(&doc).required("schema parses");
        let emitted = emit_json_schema(&schema).required("a document with an identity emits");
        assert_eq!(emitted["type"], "object", "{emitted}");
        assert_eq!(emitted["properties"]["p"]["type"], "string", "{emitted}");
    }
}

#[test]
fn every_json_schema_fixture_emits() {
    for name in ["tsconfig.json", "package.json", "github-workflow.json"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/jsonschema")
            .join(name);
        let text = std::fs::read_to_string(path).required("the fixture exists");
        let doc: serde_json::Value = serde_json::from_str(&text).required("the fixture is JSON");
        let schema = parse_json_schema(&doc).required(name);
        let emitted = emit_json_schema(&schema).required(name);
        assert!(emitted.is_object(), "{name} emits a schema object");
    }
}

#[test]
fn an_id_outside_draft_04_is_not_the_document_identity() {
    let schema = parse_json_schema(&json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "id": "whatever",
        "type": "object",
        "properties": { "p": { "type": "string" } }
    }))
    .required("schema parses");
    assert!(schema.has_vertex("root"), "a 2020-12 `id` is no keyword");
    assert!(schema.has_vertex("root.p"));
}

#[test]
fn an_identity_that_is_only_a_fragment_leaves_the_document_unnamed() {
    for doc in [
        json!({ "$id": "#", "type": "object", "properties": { "p": { "type": "string" } } }),
        json!({ "id": "#", "type": "object", "properties": { "p": { "type": "string" } } }),
        json!({ "$id": "", "type": "object", "properties": { "p": { "type": "string" } } }),
    ] {
        let schema = parse_json_schema(&doc).required("schema parses");
        assert!(schema.has_vertex("root"), "{doc}: the root is `root`");
        assert!(
            schema.has_vertex("root.p"),
            "{doc}: its property is `root.p`"
        );
        let emitted = emit_json_schema(&schema).required("the document emits");
        assert_eq!(emitted["properties"]["p"]["type"], "string", "{emitted}");
    }
}

/// An option a test requires to be `Some`.
trait RequiredSome<T> {
    /// The value, or a panic naming `what`.
    fn required_some(self, what: &str) -> T;
}

impl<T> RequiredSome<T> for Option<T> {
    fn required_some(self, what: &str) -> T {
        self.unwrap_or_else(|| panic!("{what}"))
    }
}
