//! The JSON Schema reader on the cases its reference does not reach: exact
//! numbers, byte offsets, other dialects, bundles with retrieval URIs and
//! the writer's refusals.

#![cfg(feature = "tree-sitter")]

use panproto_io::data_schema::json_schema::{
    BundleDocument, JsonSchemaErrorKind, JsonSchemaReading, JsonSchemaWriteError, read_bundle,
    read_schema, write_document,
};
use panproto_schema::{Schema, SchemaBuilder};

/// The reading of `text`, which the test requires to read.
fn reading(text: &str) -> JsonSchemaReading {
    read_schema(text.as_bytes()).unwrap_or_else(|e| panic!("{text}: {e}"))
}

/// The refusal of `text`: path, byte offset and kind.
fn refusal(text: &str) -> (String, usize, JsonSchemaErrorKind) {
    match read_schema(text.as_bytes()) {
        Ok(reading) => panic!("{text} read as {:?}", reading.schema.vertices),
        Err(error) => (error.path, error.offset, error.kind),
    }
}

/// The JSON text of constraint `sort` of `vertex`.
fn kept<'s>(schema: &'s Schema, vertex: &str, sort: &str) -> Option<&'s str> {
    schema
        .constraints
        .get(vertex)?
        .iter()
        .find(|c| c.sort.as_ref() == sort)
        .map(|c| c.value.as_str())
}

/// The canonical text of a lone document, which must read back as itself.
fn canonical(text: &str) -> String {
    let first = reading(text);
    let written =
        write_document(&first.schema, &first.roots[0]).unwrap_or_else(|e| panic!("{text}: {e}"));
    let again = reading(&written);
    let rewritten =
        write_document(&again.schema, &again.roots[0]).unwrap_or_else(|e| panic!("{written}: {e}"));
    assert_eq!(rewritten, written, "the canonical text is a fixed point");
    written
}

#[test]
fn numbers_keep_their_exact_kind_and_text() {
    let text = r#"{"minimum": 1.5, "multipleOf": 0.1, "minLength": 2.0,
        "const": 12345678901234567890123, "maximum": 1E400}"#;
    let got = reading(text);
    assert_eq!(kept(&got.schema, "#", "minimum"), Some("1.5"));
    assert_eq!(kept(&got.schema, "#", "multipleOf"), Some("0.1"));
    assert_eq!(kept(&got.schema, "#", "minLength"), Some("2.0"));
    assert_eq!(
        kept(&got.schema, "#", "const"),
        Some("12345678901234567890123")
    );
    let maximum = kept(&got.schema, "#", "maximum").map(str::to_owned);
    let written = canonical(text);
    assert_eq!(
        kept(&reading(&written).schema, "#", "maximum").map(str::to_owned),
        maximum
    );
}

#[test]
fn a_count_with_a_fractional_part_is_refused_at_its_offset() {
    let (path, offset, kind) = refusal(r#"{"properties": {"a": {"minLength": 2.5}}}"#);
    assert_eq!(path, "/properties/a/minLength");
    assert_eq!(offset, 35);
    assert_eq!(
        kind,
        JsonSchemaErrorKind::WrongType("a non-negative integer")
    );
}

#[test]
fn earlier_dialects_and_their_keywords_are_refused_by_name() {
    for dialect in [
        "http://json-schema.org/draft-04/schema#",
        "http://json-schema.org/draft-07/schema#",
        "https://json-schema.org/draft/2019-09/schema",
    ] {
        let (path, _, kind) = refusal(&format!(r#"{{"$schema": "{dialect}"}}"#));
        assert_eq!(path, "/$schema");
        assert_eq!(
            kind,
            JsonSchemaErrorKind::UnsupportedDialect(dialect.into())
        );
    }
    let (_, _, kind) = refusal(r#"{"type": "array", "items": [{}]}"#);
    assert_eq!(kind, JsonSchemaErrorKind::WrongType("a schema"));
    let (path, _, kind) = refusal(r##"{"definitions": {"a": {}}, "$ref": "#/definitions/a"}"##);
    assert_eq!(
        (path.as_str(), kind),
        (
            "/definitions",
            JsonSchemaErrorKind::Superseded("definitions")
        )
    );
}

#[test]
fn a_reference_into_another_document_needs_that_document_in_the_bundle() {
    let main =
        br#"{"$id": "https://e.test/main", "properties": {"a": {"$ref": "other#/$defs/x"}}}"#;
    let other = br#"{"$defs": {"x": {"type": "string"}}}"#;
    let bundle = read_bundle(&[
        BundleDocument {
            retrieval_uri: None,
            source: main,
        },
        BundleDocument {
            retrieval_uri: Some("https://e.test/other"),
            source: other,
        },
    ])
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        bundle.roots,
        ["https://e.test/main#", "https://e.test/other#"]
    );
    let edges = bundle
        .schema
        .outgoing_edges("https://e.test/main#/properties/a");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].tgt.to_string(), "https://e.test/other#/$defs/x");
    let alone = read_schema(main).map(|_| ()).map_err(|e| (e.path, e.kind));
    assert_eq!(
        alone,
        Err((
            "/properties/a/$ref".to_owned(),
            JsonSchemaErrorKind::RemoteRef("other#/$defs/x".into())
        ))
    );
}

#[test]
fn a_document_retrieved_from_its_own_id_is_one_resource() {
    let text = br##"{"$id": "https://e.test/s", "$defs": {"a": {"$anchor": "x"}}, "$ref": "#x"}"##;
    let got = read_bundle(&[BundleDocument {
        retrieval_uri: Some("https://e.test/s"),
        source: text,
    }])
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(got.roots, ["https://e.test/s#"]);
}

#[test]
fn a_subschema_inside_an_unknown_keyword_is_not_identified() {
    let (path, _, kind) =
        refusal(r#"{"x-hidden": {"$id": "https://e.test/h"}, "$ref": "https://e.test/h"}"#);
    assert_eq!(path, "/$ref");
    assert_eq!(
        kind,
        JsonSchemaErrorKind::RemoteRef("https://e.test/h".into())
    );
    let got = reading(r#"{"x-hidden": {"$id": "https://e.test/h"}}"#);
    assert_eq!(
        kept(&got.schema, "#", "annotation"),
        Some(r#"{"x-hidden":{"$id":"https://e.test/h"}}"#)
    );
}

#[test]
fn the_writer_refuses_what_no_document_reads_as() {
    let protocol = panproto_protocols::data_schema::json_schema::protocol();
    let schema = SchemaBuilder::new(&protocol)
        .vertex("#", "true", None)
        .map(|b| b.constraint("#", "title", "\"t\""))
        .and_then(SchemaBuilder::build)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        write_document(&schema, "#"),
        Err(JsonSchemaWriteError::Structure("#".into()))
    );
    assert_eq!(
        write_document(&schema, "#/missing"),
        Err(JsonSchemaWriteError::Missing("#/missing".into()))
    );
}
