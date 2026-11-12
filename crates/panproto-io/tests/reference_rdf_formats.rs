//! Production RDF routes against the rdf-formats reference: N-Triples
//! reading, the RDF graph encoding, and canonical writing with its
//! refusals, compared outcome for outcome on the bounded scopes and on
//! generated documents.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;

use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{GInstance, Node};
use panproto_io::rdf::ntriples::NTriplesCodec;
use panproto_io::rdf::{self, Literal, RdfWriteErrorKind};
use panproto_io::traits::{GraphInstanceEmitter, GraphInstanceParser};
use panproto_protocols::rdf::EDGE_KIND;
use panproto_protocols::rdf::ntriples::graph_schema;
use panproto_reference::enumerate::rdf_formats::{
    ENDINGS, LINES, OBJECTS, PREDICATES, SEPARATORS, SUBJECTS, mutations, ntriples_documents,
    triple_lists,
};
use panproto_reference::pages::rdf_formats::ntriples::{
    self as reference, Graph, Term, Triple, WriteRefusal,
};
use panproto_schema::Edge;
use proptest::prelude::*;

fn to_reference_term(term: &rdf::Term) -> Term {
    match term {
        rdf::Term::Iri(iri) => Term::Iri(iri.clone()),
        rdf::Term::Blank(label) => Term::Blank(label.clone()),
        rdf::Term::Literal(Literal {
            lexical,
            datatype,
            language,
        }) => Term::Literal(lexical.clone(), datatype.clone(), language.clone()),
    }
}

fn to_production_term(term: &Term) -> rdf::Term {
    match term {
        Term::Iri(iri) => rdf::Term::Iri(iri.clone()),
        Term::Blank(label) => rdf::Term::Blank(label.clone()),
        Term::Literal(lexical, datatype, language) => rdf::Term::Literal(Literal {
            lexical: lexical.clone(),
            datatype: datatype.clone(),
            language: language.clone(),
        }),
    }
}

fn to_production_triples(triples: &[Triple]) -> Vec<rdf::Triple> {
    triples
        .iter()
        .map(|Triple(s, p, o)| rdf::Triple {
            subject: to_production_term(s),
            predicate: p.clone(),
            object: to_production_term(o),
        })
        .collect()
}

/// The production reading of `text` in the reference's terms: the triples,
/// or the line of the refusal's offset.
fn production_read(codec: &NTriplesCodec, text: &str) -> Result<Vec<Triple>, usize> {
    match codec.read(text.as_bytes()) {
        Ok(triples) => Ok(triples
            .iter()
            .map(|t| {
                Triple(
                    to_reference_term(&t.subject),
                    t.predicate.clone(),
                    to_reference_term(&t.object),
                )
            })
            .collect()),
        Err(error) => Err(text[..error.offset].matches('\n').count()),
    }
}

fn assert_reads_agree(codec: &NTriplesCodec, text: &str) {
    assert_eq!(
        production_read(codec, text),
        reference::read_document(text),
        "{text:?}"
    );
}

#[test]
fn ntriples_documents_read_as_the_reference_on_every_bounded_document() {
    let codec = NTriplesCodec::new().unwrap();
    let documents = ntriples_documents();
    assert_eq!(documents.len(), 1704);
    for text in &documents {
        assert_reads_agree(&codec, text);
    }
}

fn line_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        (
            prop::sample::select(SUBJECTS),
            prop::sample::select(PREDICATES),
            prop::sample::select(OBJECTS),
            prop::sample::select(ENDINGS),
        )
            .prop_map(|(s, p, o, e)| format!("{s} {p} {o}{e}")),
        prop::sample::select(LINES).prop_map(str::to_owned),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn randomized_ntriples_documents_read_as_the_reference(
        lines in prop::collection::vec(
            (line_strategy(), prop::sample::select(SEPARATORS)),
            0..6,
        ),
    ) {
        let codec = NTriplesCodec::new().unwrap();
        let mut text = String::new();
        for (line, separator) in &lines {
            text.push_str(line);
            text.push_str(separator);
        }
        assert_reads_agree(&codec, &text);
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
        let label = Edge {
            src: graph.nodes[src].0.as_str().into(),
            tgt: graph.nodes[tgt].0.as_str().into(),
            kind: EDGE_KIND.into(),
            name: Some(name.as_str().into()),
        };
        out.add_edge(*src, *tgt, label).unwrap();
    }
    out
}

fn reference_refusal(kind: &RdfWriteErrorKind) -> WriteRefusal {
    match kind {
        RdfWriteErrorKind::UnknownAnchor { .. } => WriteRefusal::UnknownAnchor,
        RdfWriteErrorKind::Value => WriteRefusal::Value,
        RdfWriteErrorKind::UnexpectedEdge { .. } => WriteRefusal::UnexpectedEdge,
        RdfWriteErrorKind::EdgeCount { name, .. } => WriteRefusal::EdgeCount(name.clone()),
        RdfWriteErrorKind::LanguageDatatype => WriteRefusal::LanguageDatatype,
        RdfWriteErrorKind::DuplicateTerm { other } => WriteRefusal::DuplicateTerm(*other),
        RdfWriteErrorKind::DuplicateTriple { other } => WriteRefusal::DuplicateTriple(*other),
        RdfWriteErrorKind::Unused => WriteRefusal::Unused,
        RdfWriteErrorKind::RelativeIri => WriteRefusal::RelativeIri,
        RdfWriteErrorKind::IriCharacter { character } => WriteRefusal::IriCharacter(*character),
        RdfWriteErrorKind::InvalidBlankLabel => WriteRefusal::InvalidBlankLabel,
        RdfWriteErrorKind::InvalidLanguageTag => WriteRefusal::InvalidLanguageTag,
        other => panic!("no reference refusal for {other:?}"),
    }
}

#[test]
fn the_graph_encoding_matches_the_reference_on_every_bounded_triple_list() {
    let lists = triple_lists();
    assert_eq!(lists.len(), 601);
    for list in &lists {
        let graph = rdf::to_graph(&to_production_triples(list)).unwrap();
        assert_eq!(
            to_reference_graph(&graph),
            reference::encode(list),
            "{list:?}"
        );
    }
}

#[test]
fn canonical_ntriples_matches_the_reference_on_every_bounded_graph() {
    let codec = NTriplesCodec::new().unwrap();
    let schema = graph_schema().unwrap();
    let mut outcomes: BTreeMap<&str, usize> = BTreeMap::new();
    for list in &triple_lists() {
        let encoded = reference::encode(list);
        let written = codec
            .emit_graph(&schema, &to_production_graph(&encoded))
            .unwrap();
        let text = String::from_utf8(written).unwrap();
        assert_eq!(text, reference::canonical_text(list), "{list:?}");
        let again = codec.parse_graph(&schema, text.as_bytes()).unwrap();
        assert_eq!(
            reference::write_graph(&to_reference_graph(&again)),
            Ok(text),
            "{list:?}"
        );
        for variant in mutations(&encoded) {
            let production = rdf::from_graph(&to_production_graph(&variant)).map_or_else(
                |error| Err((error.node, reference_refusal(&error.kind))),
                |triples| Ok(rdf::ntriples::write(&triples)),
            );
            let expected = reference::write_graph(&variant);
            *outcomes
                .entry(if expected.is_ok() {
                    "written"
                } else {
                    "refused"
                })
                .or_default() += 1;
            assert_eq!(production, expected, "{variant:?}");
        }
    }
    assert_eq!(outcomes.values().sum::<usize>(), 21_043);
}
