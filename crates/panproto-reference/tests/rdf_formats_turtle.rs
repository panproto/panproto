//! Direct witnesses for the Turtle reference of the rdf-formats page:
//! reading, IRI resolution on the examples of RFC 3986, canonical Turtle,
//! the edit behaviour of the format-preserving route, and the scopes'
//! counts.

use panproto_reference::enumerate::rdf_formats::{
    preserving_documents, resolution_pairs, resolution_pairs_count, turtle_documents,
    turtle_documents_count,
};
use panproto_reference::pages::rdf_formats::ntriples::{RDF_LANG_STRING, Term, Triple, XSD_STRING};
use panproto_reference::pages::rdf_formats::turtle::{
    EditOutcome, Key, RDF_FIRST, RDF_NIL, RDF_REST, RDF_TYPE, XSD_INTEGER, canonical_text, edit,
    read, read_document, resolve,
};

fn iri(text: &str) -> Term {
    Term::Iri(text.into())
}

fn blank(label: &str) -> Term {
    Term::Blank(label.into())
}

fn literal(lexical: &str, datatype: &str, language: Option<&str>) -> Term {
    Term::Literal(lexical.into(), datatype.into(), language.map(Into::into))
}

#[test]
fn turtle_reads_each_abbreviation_as_its_triples() {
    let document = "@prefix ex: <http://e/> .\n\
                    ex:s a ex:C ; ex:p \"x\"@en , 7 ; ex:q [ ex:r 'y' ] , ( ex:a ) .\n";
    let s = iri("http://e/s");
    assert_eq!(
        read_document(document, None),
        Ok(vec![
            Triple(s.clone(), RDF_TYPE.into(), iri("http://e/C")),
            Triple(
                s.clone(),
                "http://e/p".into(),
                literal("x", RDF_LANG_STRING, Some("en"))
            ),
            Triple(
                s.clone(),
                "http://e/p".into(),
                literal("7", XSD_INTEGER, None)
            ),
            Triple(
                blank("b0"),
                "http://e/r".into(),
                literal("y", XSD_STRING, None)
            ),
            Triple(s.clone(), "http://e/q".into(), blank("b0")),
            Triple(blank("b1"), RDF_FIRST.into(), iri("http://e/a")),
            Triple(blank("b1"), RDF_REST.into(), iri(RDF_NIL)),
            Triple(s, "http://e/q".into(), blank("b1")),
        ])
    );
}

#[test]
fn turtle_refusals_name_the_line_of_the_first_fault() {
    assert_eq!(read_document("<s> <p> <o> .\n", None), Err(0..=0));
    assert_eq!(
        read_document(
            "<http://e/s> <http://e/p> <http://e/o> .\nu:s <http://e/p> 1 .\n",
            None
        ),
        Err(1..=1)
    );
    assert_eq!(
        read_document("<http://e/s> <http://e/p> \"\\ud800\" .", None),
        Err(0..=0)
    );
    // The missing `.` may be named anywhere from the end of `1` to the end
    // of the document.
    assert_eq!(
        read_document("\n<http://e/s> <http://e/p> 1\n", None),
        Err(1..=2)
    );
    assert_eq!(
        read_document("<http://e/s> <http://e/p> <http://e/\\u0020> .", None),
        Err(0..=0)
    );
}

#[test]
fn fresh_labels_skip_the_labels_the_document_writes() {
    assert_eq!(
        read_document("_:b0 <http://e/p> [] .\n", None),
        Ok(vec![Triple(blank("b0"), "http://e/p".into(), blank("b1"))])
    );
}

#[test]
fn resolution_follows_the_examples_of_rfc_3986() {
    let base = "http://a/b/c/d;p?q";
    for (reference, expected) in [
        ("g:h", "g:h"),
        ("g", "http://a/b/c/g"),
        ("//g", "http://g"),
        ("?y", "http://a/b/c/d;p?y"),
        ("#s", "http://a/b/c/d;p?q#s"),
        ("", "http://a/b/c/d;p?q"),
        ("../..", "http://a/"),
        ("../../../g", "http://a/g"),
        ("/./g", "http://a/g"),
        ("g;x=1/../y", "http://a/b/c/y"),
        ("g?y/../x", "http://a/b/c/g?y/../x"),
        ("http:g", "http:g"),
    ] {
        assert_eq!(resolve(base, reference), expected, "{reference}");
    }
}

#[test]
fn canonical_turtle_groups_subjects_and_predicates() {
    let triples = [
        Triple(
            iri("http://e/b"),
            "http://e/q".into(),
            literal("z", XSD_STRING, None),
        ),
        Triple(iri("http://e/b"), "http://e/p".into(), blank("x")),
        Triple(
            iri("http://e/b"),
            "http://e/q".into(),
            literal("a", XSD_STRING, None),
        ),
        Triple(iri("http://e/a"), "http://e/p".into(), iri("http://e/b")),
    ];
    assert_eq!(
        canonical_text(&triples),
        "<http://e/a> <http://e/p> <http://e/b> .\n\
         <http://e/b> <http://e/p> _:x ;\n    <http://e/q> \"a\", \"z\" .\n"
    );
    assert_eq!(canonical_text(&[]), "");
}

#[test]
fn edits_rewrite_the_terms_tokens_or_name_the_refusal() {
    let text = "@prefix : <http://e/> .\n:s a :C ; :p 42 , \"x\"@en , ( 1 ) .\n";
    let base = None;
    let written = |key: Key, value: &str| match edit(text, base, &key, value) {
        EditOutcome::Written(out) => out,
        EditOutcome::Refused(why) => panic!("refused: {why}"),
    };
    assert_eq!(
        written(Key::Term(iri("http://e/C")), "http://e/D"),
        text.replace(":C", "<http://e/D>")
    );
    assert_eq!(
        written(Key::Term(literal("42", XSD_INTEGER, None)), "x"),
        text.replace("42", "\"x\"^^<http://www.w3.org/2001/XMLSchema#integer>")
    );
    assert_eq!(
        written(Key::Language("en".into()), "fr"),
        text.replace("@en", "@fr")
    );
    for (key, value, why) in [
        (Key::Term(iri(XSD_INTEGER)), "http://e/int", "ImplicitTerm"),
        (Key::Term(iri(RDF_FIRST)), "http://e/head", "ImplicitTerm"),
        (Key::Term(blank("b0")), "c", "ImplicitTerm"),
        (Key::Term(iri("http://e/C")), "http://e/s", "Duplicate"),
        (Key::Term(iri("http://e/C")), "rel", "RelativeIri"),
        (
            Key::Term(iri(RDF_LANG_STRING)),
            "http://e/l",
            "LanguageDatatype",
        ),
        (Key::Language("en".into()), "e-", "InvalidLanguageTag"),
    ] {
        assert_eq!(
            edit(text, base, &key, value),
            EditOutcome::Refused(why),
            "{key:?}"
        );
    }
    assert_eq!(read(text, base).map(|r| r.implicit.len()), Ok(6));
}

#[test]
fn the_turtle_scopes_have_their_closed_form_counts() {
    assert_eq!(turtle_documents().len(), turtle_documents_count());
    assert_eq!(turtle_documents_count(), 2 * 4 * 9 * 4 * 26 * 4 + 7 * 7 * 2);
    assert_eq!(preserving_documents().len(), 9 * 4 * 26 * 4);
    assert_eq!(resolution_pairs().len() as u64, resolution_pairs_count());
    assert_eq!(resolution_pairs_count(), 5 * (1 + 9 + 81 + 729));
}
