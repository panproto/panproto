//! The `jsonld` protocol through the registry: its alias, the keys the
//! standard drops, an in-place edit, and the refusals of a structural edit
//! and of a graph canonical JSON-LD cannot write.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_inst::value::{FieldPresence, Value};
use panproto_io::default_registry;
use panproto_io::error::EmitInstanceError;
use panproto_io::rdf::jsonld::JsonLdCodec;
use panproto_io::rdf::{RdfWriteErrorKind, Term, Triple, from_graph, to_graph};
use panproto_protocols::rdf::jsonld::graph_schema;

#[test]
fn the_alias_names_the_jsonld_codec() {
    let registry = default_registry();
    let schema = graph_schema().unwrap();
    let doc = br#"{"@id": "http://e/s", "http://e/p": "x"}"#;
    let by_name = registry.parse_graph("jsonld", &schema, doc).unwrap();
    let by_alias = registry.parse_graph("json-ld", &schema, doc).unwrap();
    assert_eq!(
        from_graph(&by_name).unwrap(),
        from_graph(&by_alias).unwrap()
    );
}

#[test]
fn keys_that_expand_to_no_iri_are_dropped() {
    let codec = JsonLdCodec::new().unwrap();
    let triples = codec
        .read(br#"{"@id": "http://e/s", "name": "x", "@unknown": 1, "http://e/p": "y"}"#)
        .unwrap();
    assert_eq!(triples.len(), 1);
    assert_eq!(triples[0].predicate, "http://e/p");
}

#[test]
fn an_edited_value_rewrites_its_token_and_nothing_else() {
    let codec = JsonLdCodec::new().unwrap();
    let schema = graph_schema().unwrap();
    let doc = b"{\"@context\": {\"@vocab\": \"http://e/\"},\n  \"@id\": \"http://e/s\", \"n\": 5, \"t\": \"x\"}";
    let (mut graph, complement) = codec.parse_graph_preserving(&schema, doc).unwrap();
    let id = graph
        .nodes()
        .iter()
        .find(|(_, n)| n.value == Some(FieldPresence::Present(Value::Str("x".into()))))
        .map(|(id, _)| *id)
        .unwrap();
    graph.node_mut(id).unwrap().value = Some(FieldPresence::Present(Value::Str("a\"b".into())));
    let written = codec
        .emit_graph_preserving(&schema, &graph, &complement)
        .unwrap();
    assert_eq!(
        written,
        b"{\"@context\": {\"@vocab\": \"http://e/\"},\n  \"@id\": \"http://e/s\", \"n\": 5, \"t\": \"a\\\"b\"}"
    );
}

#[test]
fn a_structural_edit_is_refused() {
    let codec = JsonLdCodec::new().unwrap();
    let schema = graph_schema().unwrap();
    let doc = br#"{"@id": "http://e/s", "http://e/p": ["x", "y"]}"#;
    let (graph, complement) = codec.parse_graph_preserving(&schema, doc).unwrap();
    let mut triples = from_graph(&graph).unwrap();
    triples.pop();
    let smaller = to_graph(&triples).unwrap();
    let refused = codec.emit_graph_preserving(&schema, &smaller, &complement);
    assert!(matches!(
        refused,
        Err(EmitInstanceError::Rdf { error, .. })
            if matches!(error.kind, RdfWriteErrorKind::StructuralEdit { .. })
    ));
}

#[test]
fn an_iri_with_two_fragments_is_refused_by_the_writer() {
    let registry = default_registry();
    let schema = graph_schema().unwrap();
    let graph = to_graph(&[Triple {
        subject: Term::Iri("http://e/a#b#c".into()),
        predicate: "http://e/p".into(),
        object: Term::Iri("http://e/o".into()),
    }])
    .unwrap();
    let refused = registry.emit_graph("jsonld", &schema, &graph);
    assert!(matches!(
        refused,
        Err(EmitInstanceError::Rdf { error, .. })
            if error.kind == RdfWriteErrorKind::IriFragments
    ));
}
