//! Fixed cases of the N-Triples route outside the reference's scope: the
//! refusals of data the RDF graph encoding has no place for, the schema
//! check, RDF-star syntax, and a round trip through the registry.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;

use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{GInstance, NodeShape};
use panproto_io::rdf::{RdfReadErrorKind, RdfWriteErrorKind};
use panproto_io::{EmitInstanceError, ParseInstanceError, default_registry};
use panproto_protocols::rdf::ntriples::graph_schema;
use panproto_schema::Schema;

const DOCUMENT: &str = "<http://e/s> <http://e/p> \"v\"@en .\n";

fn read(text: &str) -> Result<GInstance, ParseInstanceError> {
    default_registry().parse_graph("ntriples", &graph_schema().unwrap(), text.as_bytes())
}

fn write_kind(graph: &GInstance) -> RdfWriteErrorKind {
    match default_registry().emit_graph("ntriples", &graph_schema().unwrap(), graph) {
        Err(EmitInstanceError::Rdf { error, .. }) => error.kind,
        other => panic!("expected an RDF refusal, got {other:?}"),
    }
}

fn read_kind(text: &str) -> RdfReadErrorKind {
    match read(text) {
        Err(ParseInstanceError::Rdf { error, .. }) => error.kind,
        other => panic!("expected an RDF refusal, got {other:?}"),
    }
}

#[test]
fn an_unedited_graph_writes_its_canonical_document() {
    let graph = read("# a comment\n<http://e/s>\t<http://e/p> \"v\" @en.\n").unwrap();
    let written = default_registry()
        .emit_graph("ntriples", &graph_schema().unwrap(), &graph)
        .unwrap();
    assert_eq!(String::from_utf8(written).unwrap(), DOCUMENT);
}

#[test]
fn a_repeated_triple_reads_as_one_triple_node() {
    let once = read(DOCUMENT).unwrap();
    let twice = read(&format!("{DOCUMENT}{DOCUMENT}")).unwrap();
    assert_eq!(once, twice);
    assert_eq!(once.node_count(), 6);
}

#[test]
fn presentation_data_the_encoding_cannot_hold_is_refused() {
    let graph = read(DOCUMENT).unwrap();
    let edits: [fn(&mut panproto_inst::Node); 5] = [
        |node| node.discriminator = Some("d".into()),
        |node| {
            node.extra_fields = HashMap::from([("k".to_owned(), Value::Bool(true))]);
        },
        |node| {
            node.annotations = HashMap::from([("k".to_owned(), Value::Bool(true))]);
        },
        |node| node.position = Some(0),
        |node| node.shape = NodeShape::List,
    ];
    for edit in edits {
        let mut edited = graph.clone();
        edit(edited.node_mut(1).unwrap());
        assert_eq!(write_kind(&edited), RdfWriteErrorKind::Presentation);
    }
}

#[test]
fn a_value_other_than_a_string_is_refused() {
    let mut graph = read(DOCUMENT).unwrap();
    graph.node_mut(1).unwrap().value = Some(FieldPresence::Present(Value::Int(1)));
    assert_eq!(write_kind(&graph), RdfWriteErrorKind::Value);
}

#[test]
fn an_edge_with_a_position_is_refused() {
    let mut edited = read(DOCUMENT).unwrap();
    let (id, edge) = edited
        .edges_sorted()
        .next()
        .map(|(i, e)| (i, e.clone()))
        .unwrap();
    edited.remove_edge(id).unwrap();
    edited.insert_edge(id, edge.with_position(Some(0))).unwrap();
    assert!(matches!(
        write_kind(&edited),
        RdfWriteErrorKind::UnexpectedEdge { .. }
    ));
}

#[test]
fn a_schema_other_than_the_encoding_is_refused() {
    let registry = default_registry();
    assert!(matches!(
        registry.parse_graph("ntriples", &Schema::default(), DOCUMENT.as_bytes()),
        Err(ParseInstanceError::SchemaMismatch { .. })
    ));
    assert!(matches!(
        registry.emit_graph("ntriples", &Schema::default(), &GInstance::new()),
        Err(EmitInstanceError::Emit { .. })
    ));
}

#[test]
fn rdf_star_and_invalid_utf8_are_refused() {
    assert_eq!(
        read_kind("<< <http://e/s> <http://e/p> <http://e/o> >> <http://e/q> <http://e/r> .\n"),
        RdfReadErrorKind::Syntax
    );
    assert!(matches!(
        default_registry().parse_graph("ntriples", &graph_schema().unwrap(), b"\xff"),
        Err(ParseInstanceError::Rdf { error, .. }) if error.kind == RdfReadErrorKind::NotUtf8
    ));
    assert_eq!(read("").unwrap(), GInstance::new());
}
