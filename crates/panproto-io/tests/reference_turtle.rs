//! The production Turtle route against the rdf-formats reference: reading,
//! IRI resolution, canonical Turtle and the format-preserving route's
//! edits, compared outcome for outcome on the bounded scopes and on
//! generated inputs.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};

use panproto_inst::GInstance;
use panproto_inst::value::{FieldPresence, Value};
use panproto_io::error::EmitInstanceError;
use panproto_io::rdf::turtle::{TurtleCodec, resolve_iri, write};
use panproto_io::rdf::{self, Literal, RdfWriteErrorKind};
use panproto_protocols::rdf::turtle::graph_schema;
use panproto_reference::enumerate::rdf_formats::{
    EDIT_VALUES, RESOLUTION_BASES, RESOLUTION_PIECES, TURTLE_BASES, TURTLE_ENDINGS, TURTLE_HEADERS,
    TURTLE_LINES, TURTLE_OBJECTS, TURTLE_PREDICATES, TURTLE_SEPARATORS, TURTLE_SUBJECTS,
    preserving_documents, resolution_pairs, triple_lists, turtle_documents,
};
use panproto_reference::pages::rdf_formats::ntriples::{Term, Triple};
use panproto_reference::pages::rdf_formats::turtle::{self as reference, EditOutcome, Key};
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

fn to_reference_triples(triples: &[rdf::Triple]) -> Vec<Triple> {
    triples
        .iter()
        .map(|t| {
            Triple(
                to_reference_term(&t.subject),
                t.predicate.clone(),
                to_reference_term(&t.object),
            )
        })
        .collect()
}

fn codec(base: Option<&str>) -> TurtleCodec {
    let codec = TurtleCodec::new().unwrap();
    match base {
        Some(base) => codec.with_base(base).unwrap(),
        None => codec,
    }
}

/// The production reading of `text`: the triples, or the line of the
/// refusal's offset.
fn production_read(codec: &TurtleCodec, text: &str) -> Result<Vec<Triple>, usize> {
    codec
        .read(text.as_bytes())
        .map(|triples| to_reference_triples(&triples))
        .map_err(|error| text[..error.offset].matches('\n').count())
}

/// Whether the production reading agrees with the reference: the same
/// triples, or a refusal on a line the reference allows.
fn agrees(production: &Result<Vec<Triple>, usize>, base: Option<&str>, text: &str) -> bool {
    match (production, reference::read_document(text, base)) {
        (Ok(triples), Ok(expected)) => *triples == expected,
        (Err(line), Err(lines)) => lines.contains(line),
        _ => false,
    }
}

fn assert_reads_agree(base: Option<&str>, text: &str) {
    let production = production_read(&codec(base), text);
    assert!(
        agrees(&production, base, text),
        "{base:?} {text:?}: production {production:?}, reference {:?}",
        reference::read_document(text, base)
    );
}

#[test]
fn turtle_documents_read_as_the_reference_on_every_bounded_document() {
    let documents = turtle_documents();
    assert_eq!(documents.len(), 30_050);
    let (with, without) = (codec(TURTLE_BASES[1]), codec(None));
    let mut read = 0;
    for (base, text) in &documents {
        let production = production_read(if base.is_some() { &with } else { &without }, text);
        assert!(
            agrees(&production, *base, text),
            "{base:?} {text:?}: production {production:?}, reference {:?}",
            reference::read_document(text, *base)
        );
        read += usize::from(production.is_ok());
    }
    assert_eq!(read, 8_101, "the documents of the scope that read");
}

fn statement_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        (
            prop::sample::select(TURTLE_SUBJECTS),
            prop::sample::select(TURTLE_PREDICATES),
            prop::sample::select(TURTLE_OBJECTS),
            prop::sample::select(TURTLE_ENDINGS),
        )
            .prop_map(|(s, p, o, e)| format!("{s} {p} {o}{e}")),
        prop::sample::select(TURTLE_LINES).prop_map(str::to_owned),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn randomized_turtle_documents_read_as_the_reference(
        base in prop::sample::select(TURTLE_BASES),
        header in prop::sample::select(TURTLE_HEADERS),
        statements in prop::collection::vec(
            (statement_strategy(), prop::sample::select(TURTLE_SEPARATORS)),
            0..5,
        ),
    ) {
        let mut text = header.to_owned();
        for (statement, separator) in &statements {
            text.push_str(statement);
            text.push_str(separator);
        }
        assert_reads_agree(base, &text);
    }

    #[test]
    fn randomized_references_resolve_as_the_reference(
        base in prop::sample::select(RESOLUTION_BASES),
        pieces in prop::collection::vec(prop::sample::select(RESOLUTION_PIECES), 0..8),
        tail in "[a-z./?#:;=]{0,6}",
    ) {
        let reference = format!("{}{tail}", pieces.concat());
        prop_assert_eq!(
            resolve_iri(base, &reference),
            reference::resolve(base, &reference),
            "{} {}", base, reference
        );
    }
}

#[test]
fn iri_resolution_matches_the_reference_on_every_bounded_pair() {
    let pairs = resolution_pairs();
    assert_eq!(pairs.len(), 4_100);
    for (base, reference) in &pairs {
        assert_eq!(
            resolve_iri(base, reference),
            reference::resolve(base, reference),
            "{base} {reference}"
        );
    }
}

fn to_production_triples(triples: &[Triple]) -> Vec<rdf::Triple> {
    let term = |t: &Term| match t {
        Term::Iri(iri) => rdf::Term::Iri(iri.clone()),
        Term::Blank(label) => rdf::Term::Blank(label.clone()),
        Term::Literal(lexical, datatype, language) => rdf::Term::Literal(Literal {
            lexical: lexical.clone(),
            datatype: datatype.clone(),
            language: language.clone(),
        }),
    };
    triples
        .iter()
        .map(|Triple(s, p, o)| rdf::Triple {
            subject: term(s),
            predicate: p.clone(),
            object: term(o),
        })
        .collect()
}

#[test]
fn canonical_turtle_matches_the_reference_on_every_bounded_triple_list() {
    let lists = triple_lists();
    assert_eq!(lists.len(), 601);
    let reader = codec(None);
    for list in &lists {
        let text = write(&to_production_triples(list));
        assert_eq!(text, reference::canonical_text(list), "{list:?}");
        let distinct: BTreeSet<&Triple> = list.iter().collect();
        let reread = production_read(&reader, &text).unwrap();
        assert_eq!(reread.iter().collect::<BTreeSet<_>>(), distinct, "{text}");
        assert_eq!(reference::read_document(&text, None), Ok(reread));
    }
}

/// The reference key of each term node of a production graph.
fn keys(graph: &GInstance) -> BTreeMap<Key, u32> {
    let string = |id: u32| match &graph.nodes()[&id].value {
        Some(FieldPresence::Present(Value::Str(text))) => text.clone(),
        other => panic!("node {id} holds {other:?}"),
    };
    let mut out = BTreeMap::new();
    for (&id, node) in graph.nodes() {
        let key = match node.anchor.as_ref() {
            "iri" => Key::Term(Term::Iri(string(id))),
            "blank" => Key::Term(Term::Blank(string(id))),
            "language" => Key::Language(string(id)),
            "literal" => {
                let mut datatype = None;
                let mut language = None;
                for &edge in graph.out_edges(id) {
                    let edge = &graph.edges()[&edge];
                    match edge.edge.name.as_deref() {
                        Some("datatype") => datatype = Some(string(edge.tgt)),
                        Some("language") => language = Some(string(edge.tgt)),
                        _ => {}
                    }
                }
                Key::Term(Term::Literal(string(id), datatype.unwrap(), language))
            }
            _ => continue,
        };
        out.insert(key, id);
    }
    out
}

/// What production does with one value edit, in the reference's terms.
fn production_edit(text: &str, key: &Key, value: &str) -> EditOutcome {
    let schema = graph_schema().unwrap();
    let codec = codec(TURTLE_BASES[1]);
    let (mut graph, complement) = codec
        .parse_graph_preserving(&schema, text.as_bytes())
        .unwrap();
    let id = keys(&graph)[key];
    graph.node_mut(id).unwrap().value = Some(FieldPresence::Present(Value::Str(value.into())));
    match codec.emit_graph_preserving(&schema, &graph, &complement) {
        Ok(bytes) => EditOutcome::Written(String::from_utf8(bytes).unwrap()),
        Err(EmitInstanceError::Rdf { error, .. }) => EditOutcome::Refused(match error.kind {
            RdfWriteErrorKind::DuplicateTerm { .. } | RdfWriteErrorKind::DuplicateTriple { .. } => {
                "Duplicate"
            }
            RdfWriteErrorKind::ImplicitTerm => "ImplicitTerm",
            RdfWriteErrorKind::InvalidBlankLabel => "InvalidBlankLabel",
            RdfWriteErrorKind::RelativeIri => "RelativeIri",
            RdfWriteErrorKind::IriCharacter { .. } => "IriCharacter",
            RdfWriteErrorKind::InvalidLanguageTag => "InvalidLanguageTag",
            RdfWriteErrorKind::LanguageDatatype => "LanguageDatatype",
            other => panic!("unexpected refusal {other:?}"),
        }),
        Err(other) => panic!("unexpected error {other}"),
    }
}

/// `triples` with every blank node not in `labelled` renamed by its first
/// occurrence, so that two readings that differ only in fresh labels
/// compare equal.
fn up_to_fresh_labels(triples: &[Triple], labelled: &BTreeSet<String>) -> Vec<Triple> {
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    let mut rename = |term: &Term| match term {
        Term::Blank(label) if !labelled.contains(label) => {
            let next = format!("fresh{}", names.len());
            Term::Blank(names.entry(label.clone()).or_insert(next).clone())
        }
        other => other.clone(),
    };
    triples
        .iter()
        .map(|Triple(s, p, o)| Triple(rename(s), p.clone(), rename(o)))
        .collect()
}

fn labelled(reading: &reference::Reading) -> BTreeSet<String> {
    reading
        .tokens
        .iter()
        .filter_map(|token| match &token.key {
            Key::Term(Term::Blank(label)) => Some(label.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn preserving_edits_match_the_reference_on_every_bounded_document() {
    let documents = preserving_documents();
    assert_eq!(documents.len(), 3_744);
    let schema = graph_schema().unwrap();
    let base = TURTLE_BASES[1];
    let preserving = codec(base);
    let (mut read, mut edits, mut written) = (0, 0, 0);
    for text in &documents {
        let Ok(reading) = reference::read(text, base) else {
            continue;
        };
        read += 1;
        let (graph, complement) = preserving
            .parse_graph_preserving(&schema, text.as_bytes())
            .unwrap();
        let unedited = preserving
            .emit_graph_preserving(&schema, &graph, &complement)
            .unwrap();
        assert_eq!(unedited, text.as_bytes(), "{text}");
        for key in reference::term_keys(&reading) {
            let kind = match &key {
                Key::Term(Term::Iri(_)) => "iri",
                Key::Term(Term::Blank(_)) => "blank",
                Key::Term(Term::Literal(..)) => "literal",
                Key::Language(_) => "language",
            };
            let values = EDIT_VALUES.iter().find(|(k, _)| *k == kind).unwrap().1;
            for value in values {
                edits += 1;
                let expected = reference::edit(text, base, &key, value);
                assert_eq!(
                    production_edit(text, &key, value),
                    expected,
                    "{text} {key:?} {value}"
                );
                if let EditOutcome::Written(output) = expected {
                    written += 1;
                    let again = reference::read(&output, base).unwrap();
                    let edited = reference::edited_triples(&reading, &key, value);
                    assert_eq!(
                        up_to_fresh_labels(&again.triples, &labelled(&again)),
                        up_to_fresh_labels(&edited, &labelled(&again)),
                        "{output}"
                    );
                }
            }
        }
    }
    assert_eq!(
        (read, edits, written),
        (1_449, 18_438, 10_323),
        "documents read, edits tried, edits written"
    );
}
