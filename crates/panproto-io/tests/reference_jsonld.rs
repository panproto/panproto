//! The production JSON-LD route against the rdf-formats reference: reading,
//! the conversion of native numbers, canonical JSON-LD and the
//! format-preserving route's edits, compared outcome for outcome on the
//! bounded scopes and on generated inputs.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::fmt::Write as _;

use panproto_inst::GInstance;
use panproto_inst::value::{FieldPresence, Value};
use panproto_io::error::EmitInstanceError;
use panproto_io::rdf::jsonld::{JsonLdCodec, JsonLdRefusal, number_literal, write};
use panproto_io::rdf::{self, Literal, RdfReadErrorKind, RdfWriteErrorKind};
use panproto_protocols::rdf::jsonld::graph_schema;
use panproto_reference::enumerate::rdf_formats::{
    JSONLD_BASES, JSONLD_BODIES, JSONLD_CONTEXTS, JSONLD_NUMBER_TYPES, JSONLD_NUMERALS,
    jsonld_documents, triple_lists,
};
use panproto_reference::pages::rdf_formats::jsonld::{self as reference, Refusal};
use panproto_reference::pages::rdf_formats::ntriples::{Term, Triple};
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

fn from_reference_term(term: &Term) -> rdf::Term {
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

fn codec(base: Option<&str>) -> JsonLdCodec {
    let codec = JsonLdCodec::new().unwrap();
    match base {
        Some(base) => codec.with_base(base).unwrap(),
        None => codec,
    }
}

/// The reference's name of a production refusal kind.
fn kind_name(refusal: &JsonLdRefusal) -> String {
    match refusal {
        JsonLdRefusal::Error { code } => format!("error:{code}"),
        JsonLdRefusal::Unsupported { feature } => format!("unsupported:{feature}"),
        JsonLdRefusal::DuplicateKey { .. } => "duplicate-key".into(),
        JsonLdRefusal::FreeFloating => "free-floating".into(),
        JsonLdRefusal::NamedGraph => "named-graph".into(),
        JsonLdRefusal::BlankPredicate => "blank-predicate".into(),
        JsonLdRefusal::ReservedKeyword { .. } => "reserved-keyword".into(),
        JsonLdRefusal::RelativeIri => "relative-iri".into(),
        JsonLdRefusal::IriCharacter { .. } => "iri-character".into(),
        JsonLdRefusal::MalformedIri => "malformed-iri".into(),
        JsonLdRefusal::InvalidLanguageTag => "language-tag".into(),
        JsonLdRefusal::NumberOutOfRange => "number-range".into(),
        other => format!("{other:?}"),
    }
}

/// The production reading of `text` in the reference's terms.
fn production_read(base: Option<&str>, text: &str) -> Result<Vec<Triple>, Refusal> {
    match codec(base).read(text.as_bytes()) {
        Ok(triples) => Ok(to_reference_triples(&triples)),
        Err(error) => match error.kind {
            RdfReadErrorKind::JsonLd { refusal, pointer } => {
                Err(Refusal::JsonLd(kind_name(&refusal), pointer))
            }
            _ => Err(Refusal::NotJson),
        },
    }
}

fn assert_reads_agree(base: Option<&str>, text: &str) {
    assert_eq!(
        production_read(base, text),
        reference::read_document(text, base),
        "base {base:?}, document {text}"
    );
}

#[test]
fn jsonld_documents_read_as_the_reference_on_every_bounded_document() {
    let documents = jsonld_documents();
    assert_eq!(documents.len(), 1538);
    let mut read = 0;
    for (base, text) in &documents {
        assert_reads_agree(*base, text);
        read += usize::from(reference::read_document(text, *base).is_ok());
    }
    assert_eq!(read, 628, "documents of the scope that read");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_jsonld_documents_read_as_the_reference(
        base in 0..JSONLD_BASES.len(),
        context in 0..JSONLD_CONTEXTS.len(),
        bodies in proptest::collection::vec(0..JSONLD_BODIES.len(), 1..4),
        nest in proptest::bool::ANY,
    ) {
        let body: Vec<&str> = bodies.iter().map(|&i| JSONLD_BODIES[i]).collect();
        let inner = format!("{{{}{}}}", JSONLD_CONTEXTS[context], body.join(", "));
        let text = if nest {
            format!("{{\"@id\": \"http://e/top\", \"http://e/has\": {inner}}}")
        } else {
            inner
        };
        assert_reads_agree(JSONLD_BASES[base], &text);
    }
}

fn production_number(numeral: &str, datatype: Option<&str>) -> Option<(String, String)> {
    number_literal(numeral, datatype).map(|l| (l.lexical, l.datatype))
}

#[test]
fn native_numbers_convert_as_the_reference_on_every_bounded_numeral() {
    let mut pairs = 0;
    for numeral in JSONLD_NUMERALS {
        for datatype in JSONLD_NUMBER_TYPES {
            assert_eq!(
                production_number(numeral, *datatype),
                reference::number_form(numeral, *datatype),
                "{numeral} typed {datatype:?}"
            );
            pairs += 1;
        }
    }
    assert_eq!(pairs, 37 * 4);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    #[test]
    fn randomized_numerals_convert_as_the_reference(
        negative in proptest::bool::ANY,
        whole in "(0|[1-9][0-9]{0,24})",
        fraction in proptest::option::of("[0-9]{1,20}"),
        exponent in proptest::option::of(-340i32..340),
        typed in 0..JSONLD_NUMBER_TYPES.len(),
    ) {
        let mut numeral = String::new();
        if negative {
            numeral.push('-');
        }
        numeral.push_str(&whole);
        if let Some(fraction) = fraction {
            numeral.push('.');
            numeral.push_str(&fraction);
        }
        if let Some(exponent) = exponent {
            let _ = write!(numeral, "e{exponent}");
        }
        let datatype = JSONLD_NUMBER_TYPES[typed];
        prop_assert_eq!(
            production_number(&numeral, datatype),
            reference::number_form(&numeral, datatype)
        );
    }
}

#[test]
fn canonical_jsonld_matches_the_reference_on_every_bounded_triple_list() {
    let lists = triple_lists();
    assert_eq!(lists.len(), 601);
    let reader = codec(None);
    for list in &lists {
        let production: Vec<rdf::Triple> = list
            .iter()
            .map(|Triple(s, p, o)| rdf::Triple {
                subject: from_reference_term(s),
                predicate: p.clone(),
                object: from_reference_term(o),
            })
            .collect();
        let text = write(&production);
        assert_eq!(text, reference::canonical_text(list), "{list:?}");
        let back: BTreeSet<rdf::Triple> =
            reader.read(text.as_bytes()).unwrap().into_iter().collect();
        let expected: BTreeSet<rdf::Triple> = production.into_iter().collect();
        assert_eq!(back, expected, "{text}");
    }
}

/// The edit values the preserving scope sets each node to, by anchor.
fn edit_values(anchor: &str) -> &'static [&'static str] {
    match anchor {
        "iri" => &["http://e/new", "http://e/a#b#c"],
        "blank" => &["z", "b0"],
        "literal" => &["y", "7"],
        _ => &["fr", "en-GB"],
    }
}

/// The refusals the preserving route may give an edit of a value.
const fn permitted_refusal(kind: &RdfWriteErrorKind) -> bool {
    matches!(
        kind,
        RdfWriteErrorKind::ImplicitTerm
            | RdfWriteErrorKind::NativeLexical
            | RdfWriteErrorKind::ReadBackDiffers
            | RdfWriteErrorKind::DuplicateTerm { .. }
            | RdfWriteErrorKind::DuplicateTriple { .. }
            | RdfWriteErrorKind::LanguageDatatype
    )
}

#[test]
fn preserving_edits_read_back_under_the_reference_on_every_bounded_document() {
    let schema = graph_schema().unwrap();
    let base = Some("http://b/d/f");
    let codec = codec(base);
    let (mut documents, mut edits, mut written) = (0, 0, 0);
    for (doc_base, text) in jsonld_documents() {
        if doc_base != base || codec.read(text.as_bytes()).is_err() {
            continue;
        }
        documents += 1;
        let (graph, complement) = codec
            .parse_graph_preserving(&schema, text.as_bytes())
            .unwrap();
        assert_eq!(
            codec
                .emit_graph_preserving(&schema, &graph, &complement)
                .unwrap(),
            text.as_bytes(),
            "{text}"
        );
        let mut ids: Vec<u32> = graph.nodes().keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let anchor = graph.nodes()[&id].anchor.to_string();
            if anchor == "triple" {
                continue;
            }
            for value in edit_values(&anchor) {
                edits += 1;
                let mut edited: GInstance = graph.clone();
                edited.node_mut(id).unwrap().value =
                    Some(FieldPresence::Present(Value::Str((*value).to_owned())));
                match codec.emit_graph_preserving(&schema, &edited, &complement) {
                    Ok(bytes) => {
                        written += 1;
                        let output = String::from_utf8(bytes).unwrap();
                        let expected: BTreeSet<Triple> =
                            to_reference_triples(&rdf::from_graph(&edited).unwrap())
                                .into_iter()
                                .collect();
                        let reread: BTreeSet<Triple> = reference::read_document(&output, base)
                            .unwrap()
                            .into_iter()
                            .collect();
                        assert_eq!(reread, expected, "{text} edited to {output}");
                    }
                    Err(EmitInstanceError::Rdf { error, .. }) => assert!(
                        permitted_refusal(&error.kind)
                            || matches!(
                                error.kind,
                                RdfWriteErrorKind::InvalidBlankLabel
                                    | RdfWriteErrorKind::InvalidLanguageTag
                                    | RdfWriteErrorKind::RelativeIri
                            ),
                        "{text}: node {id} to {value}: {error}"
                    ),
                    Err(other) => panic!("{text}: {other}"),
                }
            }
        }
    }
    assert_eq!((documents, edits, written), (320, 2040, 900));
}
