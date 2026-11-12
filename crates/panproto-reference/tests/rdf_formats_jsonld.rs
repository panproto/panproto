//! The JSON-LD reference of the rdf-formats page on fixed examples, and
//! the closed-form counts of its scopes.

use panproto_reference::enumerate::rdf_formats::{
    JSONLD_BASES, JSONLD_BODIES, JSONLD_CONTEXTS, JSONLD_EXTRA, JSONLD_NUMBER_TYPES,
    JSONLD_NUMERALS, jsonld_documents, jsonld_documents_count,
};
use panproto_reference::pages::rdf_formats::jsonld::{
    Refusal, canonical_text, number_form, read_document,
};
use panproto_reference::pages::rdf_formats::ntriples::{Term, Triple};

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

fn iri(s: &str) -> Term {
    Term::Iri(s.to_owned())
}

fn literal(lexical: &str, datatype: &str) -> Term {
    Term::Literal(lexical.to_owned(), datatype.to_owned(), None)
}

fn triple(s: Term, p: &str, o: Term) -> Triple {
    Triple(s, p.to_owned(), o)
}

fn refusal(kind: &str, pointer: &str) -> Refusal {
    Refusal::JsonLd(kind.to_owned(), pointer.to_owned())
}

#[test]
fn jsonld_reads_contexts_values_lists_and_reverse_properties() {
    let text = r#"{
        "@context": {"@vocab": "http://e/", "ex": "http://e/", "l": {"@container": "@list"},
                     "r": {"@type": "@id"}, "rev": {"@reverse": "http://e/p"}},
        "@id": "http://e/s", "@type": "C", "ex:n": [1, 1.5, true],
        "l": ["a"], "r": "http://e/o", "rev": {"@id": "http://e/q"}
    }"#;
    let s = iri("http://e/s");
    assert_eq!(
        read_document(text, None),
        Ok(vec![
            triple(
                s.clone(),
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                iri("http://e/C")
            ),
            triple(
                s.clone(),
                "http://e/n",
                literal("1", &format!("{XSD}integer"))
            ),
            triple(
                s.clone(),
                "http://e/n",
                literal("1.5E0", &format!("{XSD}double"))
            ),
            triple(
                s.clone(),
                "http://e/n",
                literal("true", &format!("{XSD}boolean"))
            ),
            triple(
                Term::Blank("b0".into()),
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#first",
                literal("a", &format!("{XSD}string"))
            ),
            triple(
                Term::Blank("b0".into()),
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest",
                iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#nil")
            ),
            triple(s.clone(), "http://e/l", Term::Blank("b0".into())),
            triple(s.clone(), "http://e/r", iri("http://e/o")),
            triple(iri("http://e/q"), "http://e/p", s),
        ])
    );
}

#[test]
fn jsonld_refusals_name_the_kind_and_the_pointer_of_the_value() {
    let cases = [
        (
            r#"{"@context": "http://c/", "http://e/p": 1}"#,
            refusal("unsupported:remote context", "/@context"),
        ),
        (
            r#"{"http://e/p": {"@value": "x", "http://e/q": 1}}"#,
            refusal("error:invalid value object", "/http:~1~1e~1p"),
        ),
        (r#"{"@id": 5}"#, refusal("error:invalid @id value", "/@id")),
        (
            r#"{"http://e/p": 1, "http://e/p": 2}"#,
            refusal("duplicate-key", "/http:~1~1e~1p"),
        ),
        (
            r#"{"@id": "s", "http://e/p": 1}"#,
            refusal("relative-iri", "/@id"),
        ),
        (r#"["x"]"#, refusal("free-floating", "/0")),
        (
            r#"{"@context": {"a": "b:c", "b": "a:d"}}"#,
            refusal("error:cyclic IRI mapping", "/@context/a"),
        ),
        (
            r#"{"@id": "http://e/g", "@graph": []}"#,
            refusal("named-graph", ""),
        ),
        (
            r#"{"http://e/p": 1e400}"#,
            refusal("number-range", "/http:~1~1e~1p"),
        ),
    ];
    for (text, expected) in cases {
        assert_eq!(read_document(text, None), Err(expected), "{text}");
    }
    assert_eq!(read_document("{\"a\": 1,}", None), Err(Refusal::NotJson));
}

#[test]
fn keys_that_expand_to_no_iri_are_dropped_as_the_standard_requires() {
    assert_eq!(
        read_document(r#"{"@id": "http://e/s", "name": "x", "@foo": 1}"#, None),
        Ok(vec![])
    );
}

#[test]
fn numbers_take_the_api_forms_at_their_boundaries() {
    let form = |n: &str, t: Option<&str>| number_form(n, t).map(|(lexical, _)| lexical);
    assert_eq!(form("-0", None).as_deref(), Some("0"));
    assert_eq!(form("1e20", None).as_deref(), Some("100000000000000000000"));
    assert_eq!(form("1e21", None).as_deref(), Some("1.0E21"));
    assert_eq!(
        form("999999999999999868928", None).as_deref(),
        Some("999999999999999868928")
    );
    assert_eq!(form("0.1", None).as_deref(), Some("1.0E-1"));
    assert_eq!(
        form("1.0000152587890625", None).as_deref(),
        Some("1.000015258789063E0")
    );
    assert_eq!(
        form("5e-324", None).as_deref(),
        Some("4.940656458412465E-324")
    );
    assert_eq!(
        form("5", Some(&format!("{XSD}double"))).as_deref(),
        Some("5.0E0")
    );
    assert_eq!(form("1e309", None), None);
}

#[test]
fn canonical_jsonld_writes_one_node_object_per_subject() {
    let triples = [
        triple(
            iri("http://e/s"),
            "http://e/q",
            literal("x", &format!("{XSD}string")),
        ),
        triple(iri("http://e/s"), "http://e/p", Term::Blank("b".into())),
    ];
    assert_eq!(
        canonical_text(&triples),
        "[\n  {\"@id\": \"http://e/s\", \"http://e/p\": [{\"@id\": \"_:b\"}], \"http://e/q\": [{\"@value\": \"x\"}]}\n]\n"
    );
    assert_eq!(canonical_text(&[]), "[]\n");
}

#[test]
fn the_jsonld_scopes_have_their_closed_form_counts() {
    assert_eq!(
        (
            JSONLD_BASES.len(),
            JSONLD_CONTEXTS.len(),
            JSONLD_BODIES.len(),
            JSONLD_EXTRA.len()
        ),
        (2, 20, 38, 9)
    );
    assert_eq!(jsonld_documents_count(), 2 * (20 * 38 + 9));
    let documents = jsonld_documents();
    assert_eq!(documents.len(), 1538);
    let read = documents
        .iter()
        .filter(|(base, text)| read_document(text, *base).is_ok())
        .count();
    assert_eq!(read, 628);
    assert_eq!(JSONLD_NUMERALS.len() * JSONLD_NUMBER_TYPES.len(), 148);
}
