//! The Turtle route: IRI resolution, the refusals beyond the grammar, the
//! canonical writer, and the format-preserving route's edit behaviour.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;

use panproto_inst::{FieldPresence, GInstance, Value};
use panproto_io::default_registry;
use panproto_io::error::EmitInstanceError;
use panproto_io::rdf::turtle::{TurtleCodec, resolve_iri, write};
use panproto_io::rdf::{Literal, RdfReadErrorKind, RdfWriteErrorKind, Term, Triple, from_graph};
use panproto_protocols::rdf::turtle::graph_schema;

const BASE: &str = "http://a/b/c/d;p?q";

/// The examples of RFC 3986 Sec 5.4.1 and 5.4.2, with the strict parser.
const RESOLUTIONS: &[(&str, &str)] = &[
    ("g:h", "g:h"),
    ("g", "http://a/b/c/g"),
    ("./g", "http://a/b/c/g"),
    ("g/", "http://a/b/c/g/"),
    ("/g", "http://a/g"),
    ("//g", "http://g"),
    ("?y", "http://a/b/c/d;p?y"),
    ("g?y", "http://a/b/c/g?y"),
    ("#s", "http://a/b/c/d;p?q#s"),
    ("g#s", "http://a/b/c/g#s"),
    ("g?y#s", "http://a/b/c/g?y#s"),
    (";x", "http://a/b/c/;x"),
    ("g;x", "http://a/b/c/g;x"),
    ("g;x?y#s", "http://a/b/c/g;x?y#s"),
    ("", "http://a/b/c/d;p?q"),
    (".", "http://a/b/c/"),
    ("./", "http://a/b/c/"),
    ("..", "http://a/b/"),
    ("../", "http://a/b/"),
    ("../g", "http://a/b/g"),
    ("../..", "http://a/"),
    ("../../", "http://a/"),
    ("../../g", "http://a/g"),
    ("../../../g", "http://a/g"),
    ("../../../../g", "http://a/g"),
    ("/./g", "http://a/g"),
    ("/../g", "http://a/g"),
    ("g.", "http://a/b/c/g."),
    (".g", "http://a/b/c/.g"),
    ("g..", "http://a/b/c/g.."),
    ("..g", "http://a/b/c/..g"),
    ("./../g", "http://a/b/g"),
    ("./g/.", "http://a/b/c/g/"),
    ("g/./h", "http://a/b/c/g/h"),
    ("g/../h", "http://a/b/c/h"),
    ("g;x=1/./y", "http://a/b/c/g;x=1/y"),
    ("g;x=1/../y", "http://a/b/c/y"),
    ("g?y/./x", "http://a/b/c/g?y/./x"),
    ("g?y/../x", "http://a/b/c/g?y/../x"),
    ("g#s/./x", "http://a/b/c/g#s/./x"),
    ("g#s/../x", "http://a/b/c/g#s/../x"),
    ("http:g", "http:g"),
];

#[test]
fn iri_references_resolve_as_the_examples_of_rfc_3986() {
    for (reference, expected) in RESOLUTIONS {
        assert_eq!(
            resolve_iri(BASE, reference),
            *expected,
            "resolving {reference:?}"
        );
    }
}

fn codec() -> TurtleCodec {
    TurtleCodec::new().unwrap()
}

fn iri(i: &str) -> Term {
    Term::Iri(i.into())
}

#[test]
fn relative_references_need_a_base_and_resolve_against_the_one_in_force() {
    let doc = b"<s> <p> <o> .\n";
    let refused = codec().read(doc).unwrap_err();
    assert_eq!(
        (refused.offset, refused.kind),
        (1, RdfReadErrorKind::RelativeIri)
    );
    let read = codec()
        .with_base("http://e/x/y")
        .unwrap()
        .read(doc)
        .unwrap();
    assert_eq!(
        read,
        [Triple {
            subject: iri("http://e/x/s"),
            predicate: "http://e/x/p".into(),
            object: iri("http://e/x/o"),
        }]
    );
    let rebased = codec()
        .read(b"@base <http://e/a/> .\n<s> <p> <o> .\nBASE <../z/>\n<s> <p> <o> .\n")
        .unwrap();
    assert_eq!(rebased[0].subject, iri("http://e/a/s"));
    assert_eq!(rebased[1].subject, iri("http://e/z/s"));
    assert!(codec().with_base("relative/base").is_err());
    assert!(codec().with_base("http://e/ space").is_err());
}

#[test]
fn abbreviations_read_as_the_triples_they_stand_for() {
    let doc = br#"@prefix ex: <http://e/> .
PREFIX : <http://d/>
ex:s a ex:C ; ex:p "x"@en , 'y' , """l
o""" , 42 , -1.5 , 1e3 , true ; :q ( 1 _:z ) , [ :r ex:t ] , () ; ex:esc\-x ex:o .
"#;
    let read = codec().read(doc).unwrap();
    let lit = |lexical: &str, datatype: &str, language: Option<&str>| {
        Term::Literal(Literal {
            lexical: lexical.into(),
            datatype: datatype.into(),
            language: language.map(Into::into),
        })
    };
    let xsd = |name: &str| format!("http://www.w3.org/2001/XMLSchema#{name}");
    let rdf = |name: &str| format!("http://www.w3.org/1999/02/22-rdf-syntax-ns#{name}");
    let s = iri("http://e/s");
    let t = |subject: &Term, predicate: &str, object: Term| Triple {
        subject: subject.clone(),
        predicate: predicate.into(),
        object,
    };
    let (b0, b1, b2) = (
        Term::Blank("b0".into()),
        Term::Blank("b1".into()),
        Term::Blank("b2".into()),
    );
    assert_eq!(
        read,
        [
            t(&s, &rdf("type"), iri("http://e/C")),
            t(&s, "http://e/p", lit("x", &rdf("langString"), Some("en"))),
            t(&s, "http://e/p", lit("y", &xsd("string"), None)),
            t(&s, "http://e/p", lit("l\no", &xsd("string"), None)),
            t(&s, "http://e/p", lit("42", &xsd("integer"), None)),
            t(&s, "http://e/p", lit("-1.5", &xsd("decimal"), None)),
            t(&s, "http://e/p", lit("1e3", &xsd("double"), None)),
            t(&s, "http://e/p", lit("true", &xsd("boolean"), None)),
            t(&b0, &rdf("first"), lit("1", &xsd("integer"), None)),
            t(&b0, &rdf("rest"), b1.clone()),
            t(&b1, &rdf("first"), Term::Blank("z".into())),
            t(&b1, &rdf("rest"), iri(&rdf("nil"))),
            t(&s, "http://d/q", b0),
            t(&b2, "http://d/r", iri("http://e/t")),
            t(&s, "http://d/q", b2),
            t(&s, "http://d/q", iri(&rdf("nil"))),
            t(&s, "http://e/esc-x", iri("http://e/o")),
        ]
    );
}

#[test]
fn fresh_blank_labels_skip_the_labels_the_document_writes() {
    let read = codec()
        .read(b"_:b0 <http://e/p> [] .\n_:b2 <http://e/p> [] .\n")
        .unwrap();
    let objects: Vec<&Term> = read.iter().map(|t| &t.object).collect();
    assert_eq!(
        objects,
        [&Term::Blank("b1".into()), &Term::Blank("b3".into())]
    );
}

/// The over-accepted negative syntax tests of the W3C suite, refused by
/// the walker: the refusal kind and its byte offset.
#[test]
fn what_the_grammar_admits_and_turtle_does_not_is_refused_by_kind_and_offset() {
    let cases: &[(&[u8], usize, RdfReadErrorKind)] = &[
        (
            b"<http://e/\\u0020> <http://e/p> <http://e/o> .",
            10,
            RdfReadErrorKind::IriCharacter { character: ' ' },
        ),
        (
            b"<http://e/\\u003C> <http://e/p> <http://e/o> .",
            10,
            RdfReadErrorKind::IriCharacter { character: '<' },
        ),
        (
            b":s <http://e/p> \"x\" .",
            0,
            RdfReadErrorKind::UndefinedPrefix {
                prefix: String::new(),
            },
        ),
        (
            b"@prefix rdf: <http://r/> .\n<http://e/s> rdf:type :C .",
            49,
            RdfReadErrorKind::UndefinedPrefix {
                prefix: String::new(),
            },
        ),
        (
            b"{ <http://e/s> <http://e/p> <http://e/o> }",
            0,
            RdfReadErrorKind::NotInFormat {
                construct: "a graph block".into(),
            },
        ),
        (
            b"<http://e/s> <http://e/p> \"\\ud800\" .",
            27,
            RdfReadErrorKind::NotAScalarValue { code_point: 0xD800 },
        ),
        (
            b"<http://e/s> <http://e/p> '''\\udfff''' .",
            29,
            RdfReadErrorKind::NotAScalarValue { code_point: 0xDFFF },
        ),
        (
            b"<http://e/s> <http://e/p> <\\ud800> .",
            27,
            RdfReadErrorKind::NotAScalarValue { code_point: 0xD800 },
        ),
        (
            b"<http://e/s> <http://e/p> \"\x00\" .",
            27,
            RdfReadErrorKind::UnsupportedCharacter { character: '\0' },
        ),
    ];
    for (doc, offset, kind) in cases {
        let refused = codec().read(doc).unwrap_err();
        assert_eq!(
            (refused.offset, &refused.kind),
            (*offset, kind),
            "{}",
            String::from_utf8_lossy(doc)
        );
    }
    let n3 = codec()
        .read(b"@prefix : <http://e/> .\n:z is :p of :x .\n")
        .unwrap_err();
    assert_eq!(n3.kind, RdfReadErrorKind::Syntax);
}

#[test]
fn canonical_turtle_groups_by_subject_in_code_point_order() {
    let doc = br#"@prefix : <http://e/> .
:b :q "z" , "a"@en ; :p _:x .
:a :p 1 .
"#;
    let triples = codec().read(doc).unwrap();
    let text = write(&triples);
    assert_eq!(
        text,
        "<http://e/a> <http://e/p> \"1\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n\
         <http://e/b> <http://e/p> _:x ;\n    <http://e/q> \"a\"@en, \"z\" .\n"
    );
    let reread: BTreeSet<Triple> = codec().read(text.as_bytes()).unwrap().into_iter().collect();
    assert_eq!(reread, triples.into_iter().collect());
}

#[test]
fn the_registry_reads_turtle_and_writes_canonical_turtle() {
    let registry = default_registry();
    let schema = graph_schema().unwrap();
    let doc = b"@prefix : <http://e/> .\n:s :p :o , :o .\n";
    let graph = registry.parse_graph("turtle", &schema, doc).unwrap();
    assert_eq!(graph.node_count(), 4, "one triple node and three IRI nodes");
    let written = registry.emit_graph("turtle", &schema, &graph).unwrap();
    assert_eq!(written, b"<http://e/s> <http://e/p> <http://e/o> .\n");
}

const DOC: &str = "@prefix ex: <http://example.org/> .\n\
# a comment that stays\n\
ex:s a ex:C ;\n    ex:p \"x\"@en , 42 , ( 1 ) ;\n    ex:q [ ex:r _:b ] .\n";

fn node_with(graph: &GInstance, anchor: &str, value: &str) -> u32 {
    let mut ids: Vec<u32> = graph
        .nodes()
        .values()
        .filter(|n| {
            n.anchor.as_ref() == anchor
                && n.value == Some(FieldPresence::Present(Value::Str(value.into())))
        })
        .map(|n| n.id)
        .collect();
    assert_eq!(ids.len(), 1, "{anchor} {value}");
    ids.pop().unwrap()
}

fn set_value(graph: &mut GInstance, id: u32, value: &str) {
    graph.node_mut(id).unwrap().value = Some(FieldPresence::Present(Value::Str(value.into())));
}

/// Edit one node's value, emit through the preserving route and return the
/// bytes, checking that they read back as the edited graph's triples.
fn edited(anchor: &str, from: &str, to: &str) -> Result<String, EmitInstanceError> {
    let schema = graph_schema().unwrap();
    let (mut graph, complement) = codec()
        .parse_graph_preserving(&schema, DOC.as_bytes())
        .unwrap();
    let id = node_with(&graph, anchor, from);
    set_value(&mut graph, id, to);
    let bytes = codec().emit_graph_preserving(&schema, &graph, &complement)?;
    let reread: BTreeSet<Triple> = codec().read(&bytes).unwrap().into_iter().collect();
    let expected: BTreeSet<Triple> = from_graph(&graph).unwrap().into_iter().collect();
    assert_eq!(reread, expected, "the emitted document reads as the edit");
    Ok(String::from_utf8(bytes).unwrap())
}

fn refusal(result: Result<String, EmitInstanceError>) -> RdfWriteErrorKind {
    match result {
        Err(EmitInstanceError::Rdf { error, .. }) => error.kind,
        other => panic!("expected an RDF refusal, got {other:?}"),
    }
}

#[test]
fn an_unedited_graph_is_written_back_byte_for_byte() {
    let schema = graph_schema().unwrap();
    let (graph, complement) = codec()
        .parse_graph_preserving(&schema, DOC.as_bytes())
        .unwrap();
    let native = codec().read(DOC.as_bytes()).unwrap();
    assert_eq!(
        from_graph(&graph)
            .unwrap()
            .into_iter()
            .collect::<BTreeSet<_>>(),
        native.into_iter().collect(),
        "the preserving and native routes read the same graph"
    );
    let bytes = codec()
        .emit_graph_preserving(&schema, &graph, &complement)
        .unwrap();
    assert_eq!(String::from_utf8(bytes).unwrap(), DOC);
}

#[test]
fn value_edits_rewrite_only_the_tokens_of_the_edited_term() {
    let replace = |from: &str, to: &str| DOC.replacen(from, to, 1);
    assert_eq!(
        edited("iri", "http://example.org/C", "http://example.org/D").unwrap(),
        replace("ex:C", "<http://example.org/D>")
    );
    assert_eq!(
        edited(
            "iri",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            "http://example.org/kind"
        )
        .unwrap(),
        replace("ex:s a ex:C", "ex:s <http://example.org/kind> ex:C")
    );
    assert_eq!(edited("literal", "42", "43").unwrap(), replace("42", "43"));
    assert_eq!(
        edited("literal", "42", "4x").unwrap(),
        replace("42", "\"4x\"^^<http://www.w3.org/2001/XMLSchema#integer>")
    );
    assert_eq!(
        edited("literal", "x", "say \"hi\"\n").unwrap(),
        replace("\"x\"", "\"say \\\"hi\\\"\\n\"")
    );
    assert_eq!(
        edited("language", "en", "fr").unwrap(),
        replace("@en", "@fr")
    );
    assert_eq!(edited("blank", "b", "c").unwrap(), replace("_:b", "_:c"));
}

#[test]
fn edits_the_document_cannot_carry_in_place_are_refused() {
    assert_eq!(
        refusal(edited(
            "iri",
            "http://www.w3.org/2001/XMLSchema#integer",
            "http://example.org/int"
        )),
        RdfWriteErrorKind::ImplicitTerm
    );
    assert_eq!(
        refusal(edited(
            "iri",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#first",
            "http://example.org/head"
        )),
        RdfWriteErrorKind::ImplicitTerm
    );
    assert_eq!(
        refusal(edited("blank", "b0", "fresh")),
        RdfWriteErrorKind::ImplicitTerm
    );
    assert_eq!(
        refusal(edited("blank", "b", "x:y")),
        RdfWriteErrorKind::InvalidBlankLabel
    );
    assert!(matches!(
        refusal(edited(
            "iri",
            "http://example.org/C",
            "http://example.org/s"
        )),
        RdfWriteErrorKind::DuplicateTerm { .. }
    ));
    assert_eq!(
        refusal(edited("iri", "http://example.org/C", "relative")),
        RdfWriteErrorKind::RelativeIri
    );

    let schema = graph_schema().unwrap();
    let (mut graph, complement) = codec()
        .parse_graph_preserving(&schema, DOC.as_bytes())
        .unwrap();
    let triple = graph
        .nodes()
        .values()
        .find(|n| n.anchor.as_ref() == "triple")
        .map(|n| n.id)
        .unwrap();
    for edge in graph.out_edges(triple).to_vec() {
        graph.remove_edge(edge).unwrap();
    }
    graph.remove_node(triple).unwrap();
    match codec().emit_graph_preserving(&schema, &graph, &complement) {
        Err(EmitInstanceError::Rdf { error, .. }) => assert!(matches!(
            error.kind,
            RdfWriteErrorKind::StructuralEdit { .. }
        )),
        other => panic!("expected a structural refusal, got {other:?}"),
    }
}
