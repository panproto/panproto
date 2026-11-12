//! Production edge-list routes against the edge-lists reference: reading
//! in both dialects, the edge-list graph encoding, and canonical writing
//! with its refusals, compared outcome for outcome on the bounded scopes
//! and on generated documents.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{GInstance, Node};
use panproto_io::edgelist::{
    self, Dialect, EdgeListCodec, EdgeListReadError, EdgeListWriteErrorKind,
};
use panproto_protocols::edgelist::EDGE_KIND;
use panproto_reference::enumerate::edge_lists::{
    CSV_HEADERS, CSV_RECORDS, PREFIXES, TSV_HEADERS, TSV_RECORDS, documents, edge_lists,
    line_endings, mutations,
};
use panproto_reference::pages::edge_lists::{
    self as reference, Edge, Graph, ReadRefusal, WriteRefusal,
};
use panproto_schema::Edge as SchemaEdge;
use proptest::prelude::*;

const fn reference_dialect(dialect: Dialect) -> reference::Dialect {
    match dialect {
        Dialect::Csv => reference::Dialect::Csv,
        Dialect::Tsv => reference::Dialect::Tsv,
    }
}

fn to_reference_edges(edges: &[edgelist::Edge]) -> Vec<Edge> {
    edges
        .iter()
        .map(|e| Edge(e.source.clone(), e.target.clone(), e.label.clone()))
        .collect()
}

fn to_production_edges(edges: &[Edge]) -> Vec<edgelist::Edge> {
    edges
        .iter()
        .map(|Edge(source, target, label)| edgelist::Edge {
            source: source.clone(),
            target: target.clone(),
            label: label.clone(),
        })
        .collect()
}

fn reference_read_refusal(error: EdgeListReadError) -> ReadRefusal {
    match error {
        EdgeListReadError::Table { .. } => ReadRefusal::Table,
        EdgeListReadError::NoHeader => ReadRefusal::NoHeader,
        EdgeListReadError::UnsupportedColumn { column, name } => {
            ReadRefusal::UnsupportedColumn(column, name)
        }
        EdgeListReadError::DuplicateColumn { column, name } => {
            ReadRefusal::DuplicateColumn(column, name)
        }
        EdgeListReadError::MissingColumn { name } => ReadRefusal::MissingColumn(name),
        EdgeListReadError::EmptyName { record, column } => ReadRefusal::EmptyName(record, column),
        other => panic!("no reference refusal for {other:?}"),
    }
}

fn assert_reads_agree(codec: &EdgeListCodec, text: &str) {
    let production = codec
        .read(text.as_bytes())
        .map(|edges| to_reference_edges(&edges))
        .map_err(reference_read_refusal);
    assert_eq!(
        production,
        reference::read_document(text, reference_dialect(codec.dialect())),
        "{:?} {text:?}",
        codec.dialect()
    );
}

#[test]
fn edge_list_documents_read_as_the_reference_on_every_bounded_document() {
    for (dialect, count) in [(Dialect::Csv, 25_321), (Dialect::Tsv, 21_547)] {
        let codec = EdgeListCodec::new(dialect).unwrap();
        let documents = documents(reference_dialect(dialect));
        assert_eq!(documents.len(), count);
        for text in &documents {
            assert_reads_agree(&codec, text);
        }
    }
}

fn document_strategy(dialect: Dialect) -> impl Strategy<Value = String> {
    let (headers, records) = match dialect {
        Dialect::Csv => (CSV_HEADERS, CSV_RECORDS),
        Dialect::Tsv => (TSV_HEADERS, TSV_RECORDS),
    };
    let endings = line_endings(reference_dialect(dialect));
    (
        prop::sample::select(PREFIXES),
        prop::sample::select(headers),
        prop::collection::vec(
            (prop::sample::select(records), prop::sample::select(endings)),
            0..6,
        ),
        prop::sample::select(endings),
    )
        .prop_map(|(prefix, header, body, ending)| {
            let mut text = format!("{prefix}{header}{ending}");
            for (record, ending) in body {
                text.push_str(record);
                text.push_str(ending);
            }
            text
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_edge_list_documents_read_as_the_reference(
        csv in document_strategy(Dialect::Csv),
        tsv in document_strategy(Dialect::Tsv),
    ) {
        assert_reads_agree(&EdgeListCodec::new(Dialect::Csv).unwrap(), &csv);
        assert_reads_agree(&EdgeListCodec::new(Dialect::Tsv).unwrap(), &tsv);
    }
}

/// A production graph in the reference's form.
fn to_reference_graph(graph: &GInstance) -> Graph {
    let nodes = graph
        .nodes()
        .iter()
        .map(|(&id, node)| {
            let value = match &node.value {
                Some(FieldPresence::Present(Value::Str(text))) => Some(text.clone()),
                None => None,
                other => panic!("node {id} holds {other:?}"),
            };
            (id, (node.anchor.to_string(), value))
        })
        .collect();
    let edges = graph
        .edges_sorted()
        .map(|(_, edge)| {
            (
                edge.src,
                edge.tgt,
                edge.edge.name.as_deref().unwrap_or("").to_owned(),
            )
        })
        .collect();
    Graph { nodes, edges }
}

/// A reference graph as a production graph, each edge labelled by its
/// endpoints' anchors.
fn to_production_graph(graph: &Graph) -> GInstance {
    let mut out = GInstance::new();
    for (&id, (anchor, value)) in &graph.nodes {
        let mut node = Node::new(id, anchor.as_str());
        if let Some(text) = value {
            node = node.with_value(FieldPresence::Present(Value::Str(text.clone())));
        }
        out.insert_node(node).unwrap();
    }
    for (src, tgt, name) in &graph.edges {
        let label = SchemaEdge {
            src: graph.nodes[src].0.as_str().into(),
            tgt: graph.nodes[tgt].0.as_str().into(),
            kind: EDGE_KIND.into(),
            name: Some(name.as_str().into()),
        };
        out.add_edge(*src, *tgt, label).unwrap();
    }
    out
}

#[test]
fn the_edge_list_encoding_matches_the_reference_on_every_bounded_list() {
    let lists = edge_lists();
    assert_eq!(lists.len(), 73);
    for list in &lists {
        let graph = edgelist::to_graph(&to_production_edges(list)).unwrap();
        assert_eq!(
            to_reference_graph(&graph),
            reference::encode(list),
            "{list:?}"
        );
    }
}

fn reference_write_refusal(kind: &EdgeListWriteErrorKind) -> WriteRefusal {
    match kind {
        EdgeListWriteErrorKind::UnknownAnchor { .. } => WriteRefusal::UnknownAnchor,
        EdgeListWriteErrorKind::Value => WriteRefusal::Value,
        EdgeListWriteErrorKind::UnexpectedEdge { .. } => WriteRefusal::UnexpectedEdge,
        EdgeListWriteErrorKind::EdgeCount { name, .. } => WriteRefusal::EdgeCount(name.clone()),
        EdgeListWriteErrorKind::EmptyName => WriteRefusal::EmptyName,
        EdgeListWriteErrorKind::UnwritableCharacter { character } => {
            WriteRefusal::UnwritableCharacter(*character)
        }
        EdgeListWriteErrorKind::DuplicateNode { other } => WriteRefusal::DuplicateNode(*other),
        EdgeListWriteErrorKind::MixedLabels { first } => WriteRefusal::MixedLabels(*first),
        EdgeListWriteErrorKind::Isolated => WriteRefusal::Isolated,
        other => panic!("no reference refusal for {other:?}"),
    }
}

#[test]
fn canonical_edge_lists_match_the_reference_on_every_bounded_graph() {
    let mut graphs = 0;
    for dialect in [Dialect::Csv, Dialect::Tsv] {
        let codec = EdgeListCodec::new(dialect).unwrap();
        for list in edge_lists() {
            for graph in mutations(&reference::encode(&list)) {
                graphs += 1;
                let production = to_production_graph(&graph);
                let written = edgelist::from_graph(&production, dialect)
                    .map_err(|error| (error.node, reference_write_refusal(&error.kind)))
                    .map(|edges| edgelist::write(&edges, dialect).unwrap());
                assert_eq!(
                    written,
                    reference::write_graph(&graph, reference_dialect(dialect)),
                    "{dialect:?} {graph:?}"
                );
                if let Ok(text) = written {
                    let edges = codec.read(text.as_bytes()).unwrap();
                    let again = edgelist::to_graph(&edges).unwrap();
                    assert_eq!(
                        edgelist::write(&edgelist::from_graph(&again, dialect).unwrap(), dialect),
                        Some(text),
                        "{dialect:?} {graph:?}"
                    );
                }
            }
        }
    }
    assert_eq!(graphs, 2 * 1_387);
}
