//! The edge-list codecs through the registry: reading, canonical writing,
//! and the refusals the bounded reference cannot express (presentation
//! data, non-string values, positioned edges, invalid UTF-8, a schema that
//! is not the encoding).

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_inst::GInstance;
use panproto_inst::value::{FieldPresence, Value};
use panproto_io::default_registry;
use panproto_io::edgelist::{EdgeListReadError, EdgeListWriteErrorKind};
use panproto_io::error::{EmitInstanceError, ParseInstanceError};
use panproto_protocols::edgelist::csv::graph_schema;

fn read(protocol: &str, input: &[u8]) -> Result<GInstance, ParseInstanceError> {
    default_registry().parse_graph(protocol, &graph_schema().unwrap(), input)
}

fn write_refusal(graph: &GInstance) -> EdgeListWriteErrorKind {
    match default_registry().emit_graph("edgelist-csv", &graph_schema().unwrap(), graph) {
        Err(EmitInstanceError::EdgeList { error, .. }) => error.kind,
        other => panic!("expected an edge-list refusal, got {other:?}"),
    }
}

#[test]
fn both_dialects_read_and_write_through_the_registry() {
    let graph = read("edgelist-csv", b"Target,Source\nb,a\nb,a\n").unwrap();
    assert_eq!(graph.node_count(), 4);
    let text = default_registry()
        .emit_graph("edgelist-csv", &graph_schema().unwrap(), &graph)
        .unwrap();
    assert_eq!(text, b"source,target\na,b\na,b\n");
    let tsv = read("edgelist-tsv", b"source\ttarget\tlabel\na\tb\tx,y\n").unwrap();
    let text = default_registry()
        .emit_graph("edgelist-tsv", &graph_schema().unwrap(), &tsv)
        .unwrap();
    assert_eq!(text, b"source\ttarget\tlabel\na\tb\tx,y\n");
}

#[test]
fn invalid_utf8_and_a_foreign_schema_are_refused() {
    assert!(matches!(
        read("edgelist-csv", b"source,target\na,\xFF\n"),
        Err(ParseInstanceError::EdgeList {
            error: EdgeListReadError::NotUtf8 { offset: 16 },
            ..
        })
    ));
    let foreign = panproto_protocols::rdf::ntriples::graph_schema().unwrap();
    assert!(matches!(
        default_registry().parse_graph("edgelist-csv", &foreign, b"source,target\n"),
        Err(ParseInstanceError::SchemaMismatch { .. })
    ));
}

#[test]
fn presentation_data_values_that_are_not_strings_and_positioned_edges_are_refused() {
    let graph = read("edgelist-csv", b"source,target\na,b\n").unwrap();

    let mut annotated = graph.clone();
    annotated.node_mut(1).unwrap().position = Some(0);
    assert_eq!(
        write_refusal(&annotated),
        EdgeListWriteErrorKind::Presentation
    );

    let mut numeric = graph.clone();
    numeric.node_mut(1).unwrap().value = Some(FieldPresence::Present(Value::Int(1)));
    assert_eq!(write_refusal(&numeric), EdgeListWriteErrorKind::Value);

    let mut labelled = graph.clone();
    labelled.node_mut(0).unwrap().value = Some(FieldPresence::Present(Value::Int(1)));
    assert_eq!(write_refusal(&labelled), EdgeListWriteErrorKind::Value);

    let mut positioned = graph;
    positioned
        .set_edge_position(panproto_inst::EdgeId(0), Some(3))
        .unwrap();
    assert_eq!(
        write_refusal(&positioned),
        EdgeListWriteErrorKind::UnexpectedEdge {
            name: "source".into()
        }
    );
}
