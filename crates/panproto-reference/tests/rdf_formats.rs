//! Direct witnesses for the rdf-formats reference: N-Triples reading on the
//! examples of the RDF 1.1 N-Triples Recommendation, the RDF graph encoding,
//! canonical writing and its refusals, with the scopes' counts.

use panproto_reference::enumerate::rdf_formats::{
    mutations, mutations_count, ntriples_documents, ntriples_documents_count, triple_lists,
    triple_lists_count,
};
use panproto_reference::pages::rdf_formats::ntriples::{
    Graph, RDF_LANG_STRING, Term, Triple, WriteRefusal, XSD_STRING, canonical_text, encode,
    read_document, write_graph,
};

fn iri(text: &str) -> Term {
    Term::Iri(text.into())
}

fn literal(lexical: &str, datatype: &str, language: Option<&str>) -> Term {
    Term::Literal(lexical.into(), datatype.into(), language.map(Into::into))
}

#[test]
fn ntriples_reads_the_examples_of_the_recommendation() {
    // Example 1 and the literal forms of Sec 2.4 of RDF 1.1 N-Triples.
    let document = "<http://one.example/subject1> <http://one.example/predicate1> <http://one.example/object1> . # comments here\n\
                    # or on a line by themselves\n\
                    _:subject1 <http://an.example/predicate1> \"object1\" .\n\
                    _:subject2 <http://an.example/predicate2> \"object2\" .\n\
                    <http://example.org/show/218> <http://www.w3.org/2000/01/rdf-schema#label> \"That Seventies Show\"^^<http://www.w3.org/2001/XMLSchema#string> .\n\
                    <http://example.org/show/218> <http://example.org/show/localName> \"Cette S\\u00E9rie des Ann\\u00E9es Septante\"@fr-be .\n\
                    <http://en.wikipedia.org/wiki/Helium> <http://example.org/elements/atomicNumber> \"2\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n";
    let Ok(triples) = read_document(document) else {
        panic!("the examples do not read");
    };
    assert_eq!(triples.len(), 6);
    assert_eq!(
        triples[0],
        Triple(
            iri("http://one.example/subject1"),
            "http://one.example/predicate1".into(),
            iri("http://one.example/object1"),
        )
    );
    assert_eq!(triples[1].0, Term::Blank("subject1".into()));
    assert_eq!(
        triples[3].2,
        literal("That Seventies Show", XSD_STRING, None)
    );
    assert_eq!(
        triples[4].2,
        literal(
            "Cette S\u{E9}rie des Ann\u{E9}es Septante",
            RDF_LANG_STRING,
            Some("fr-be")
        )
    );
    assert_eq!(
        triples[5].2,
        literal("2", "http://www.w3.org/2001/XMLSchema#integer", None)
    );
}

#[test]
fn ntriples_refusals_name_the_line_of_the_first_fault() {
    let good = "<http://e/s> <http://e/p> <http://e/o> .\n";
    for (bad, line) in [
        ("<s> <http://e/p> <http://e/o> .", 1),
        ("<http://e/s> <http://e/p> 1 .", 1),
        ("@prefix e: <http://e/> .", 1),
        ("<http://e/s> <http://e/p> \"\\uD800\" .", 1),
        ("<http://e/s> <http://e/p> <http://e/\\u0020> .", 1),
        ("<http://e/s> <http://e/p> \"a\u{0}\" .", 1),
    ] {
        assert_eq!(read_document(&format!("{good}{bad}\n")), Err(line), "{bad}");
    }
    assert_eq!(
        read_document(
            "<http://e/s> <http://e/p> <http://e/o> . <http://e/s> <http://e/p> <http://e/o> .\n"
        ),
        Err(0)
    );
    assert_eq!(read_document(""), Ok(Vec::new()));
}

#[test]
fn the_graph_encoding_numbers_nodes_by_first_mention_and_reads_a_set() {
    let triple = Triple(
        Term::Blank("x".into()),
        "http://e/p".into(),
        literal("v", RDF_LANG_STRING, Some("en")),
    );
    let graph = encode(&[triple.clone(), triple]);
    let anchors: Vec<&str> = graph.nodes.values().map(|(a, _)| a.as_str()).collect();
    assert_eq!(
        anchors,
        ["triple", "blank", "iri", "literal", "iri", "language"]
    );
    assert_eq!(
        graph.edges,
        vec![
            (0, 1, "subject".to_owned()),
            (0, 2, "predicate".to_owned()),
            (3, 4, "datatype".to_owned()),
            (3, 5, "language".to_owned()),
            (0, 3, "object".to_owned()),
        ]
    );
}

#[test]
fn canonical_text_sorts_lines_and_escapes_as_the_canonical_form_does() {
    let triples = [
        Triple(
            iri("http://e/b"),
            "http://e/p".into(),
            literal("a\n\u{1}\"\\\u{FFFF}\u{E9}\u{7F}", XSD_STRING, None),
        ),
        Triple(
            iri("http://e/a"),
            "http://e/p".into(),
            literal("01", "http://www.w3.org/2001/XMLSchema#integer", None),
        ),
    ];
    assert_eq!(
        canonical_text(&triples),
        "<http://e/a> <http://e/p> \"01\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n\
         <http://e/b> <http://e/p> \"a\\n\\u0001\\\"\\\\\\uFFFF\u{E9}\\u007F\" .\n"
    );
}

#[test]
fn the_writer_refuses_what_no_document_reads_back_as() {
    let triple = Triple(iri("http://e/s"), "http://e/p".into(), iri("http://e/o"));
    let graph = encode(std::slice::from_ref(&triple));
    assert_eq!(write_graph(&graph), Ok(canonical_text(&[triple])));

    let mut duplicate = graph.clone();
    duplicate
        .nodes
        .insert(4, ("iri".into(), Some("http://e/o".into())));
    assert_eq!(
        write_graph(&duplicate),
        Err((4, WriteRefusal::DuplicateTerm(3)))
    );

    let mut relative = graph.clone();
    relative.nodes.insert(1, ("iri".into(), Some("s".into())));
    assert_eq!(write_graph(&relative), Err((1, WriteRefusal::RelativeIri)));

    let mut dropped = graph.clone();
    dropped.edges.pop();
    assert_eq!(
        write_graph(&dropped),
        Err((0, WriteRefusal::EdgeCount("object".into())))
    );

    let mut unused = graph;
    unused.nodes.insert(4, ("blank".into(), Some("u".into())));
    assert_eq!(write_graph(&unused), Err((4, WriteRefusal::Unused)));
    assert_eq!(write_graph(&Graph::default()), Ok(String::new()));
}

#[test]
fn the_rdf_scopes_have_their_closed_form_counts() {
    assert_eq!(ntriples_documents().len(), ntriples_documents_count());
    assert_eq!(ntriples_documents_count(), 1704);
    let lists = triple_lists();
    assert_eq!(lists.len() as u64, triple_lists_count());
    assert_eq!(triple_lists_count(), 601);
    let mut total = 0;
    for list in &lists {
        let graph = encode(list);
        let variants = mutations(&graph);
        assert_eq!(variants.len(), mutations_count(&graph));
        total += variants.len();
    }
    assert_eq!(total, 21_043);
}
