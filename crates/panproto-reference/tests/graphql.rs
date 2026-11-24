//! The graphql reference on the page's own examples and scope.

use panproto_reference::enumerate::graphql::{
    CHOICES, accepted_count, choices, document, document_count, documents,
};
use panproto_reference::pages::graphql::{Refusal, canonical_text, read, render, string};

fn refusal(choice: [usize; 9]) -> Refusal {
    match read(&document(choice)) {
        Ok(reading) => panic!("{reading:?}"),
        Err(refusal) => refusal,
    }
}

#[test]
fn the_graphql_document_scope_has_its_closed_form_count() {
    assert_eq!(CHOICES.map(|(size, _)| size), [4, 3, 2, 4, 4, 4, 4, 2, 3]);
    let all = documents();
    assert_eq!(all.len(), document_count());
    assert_eq!(document_count(), 36_864);
    let read_count = all.iter().filter(|d| read(d).is_ok()).count();
    assert_eq!(read_count, accepted_count());
    assert_eq!(accepted_count(), 432);
}

#[test]
fn every_breaking_choice_is_refused_by_its_rule() {
    let base = [0; 9];
    let cases: [(usize, usize, &str, &str); 11] = [
        (0, 2, "Pet.__x", "ReservedName"),
        (0, 3, "Pet.age", "NotOutputType"),
        (1, 1, "Pet.id", "FieldNotCovariant"),
        (1, 2, "Pet", "ImplementsNonInterface"),
        (2, 1, "Result", "MemberNotObject"),
        (3, 3, "Query.search(filter:)", "ValueNotCoercible"),
        (4, 2, "Pet", "ArgumentMissing"),
        (4, 3, "Pet", "DirectiveLocation"),
        (5, 3, "Kind", "ExtensionKind"),
        (6, 3, "{schema}", "RootNotObject"),
        (8, 2, "Filter.self", "InputCycle"),
    ];
    for (dimension, index, path, rule) in cases {
        let mut choice = base;
        choice[dimension] = index;
        assert_eq!(
            refusal(choice),
            Refusal {
                path: path.to_owned(),
                rule
            },
            "{choice:?}"
        );
    }
}

#[test]
fn a_document_reads_as_its_vertices_edges_and_constraints() {
    let got = read(&document([1, 0, 0, 1, 1, 1, 2, 1, 1])).unwrap_or_else(|r| panic!("{r:?}"));
    assert_eq!(got.vertices["{schema}"], "schema");
    assert_eq!(got.vertices["Pet.owner(first:)"], "argument");
    assert_eq!(got.vertices["Filter"], "input-object");
    assert_eq!(got.vertices["ID"], "scalar");
    assert_eq!(
        got.edges["{schema}"],
        [
            (
                "root".to_owned(),
                "Query".to_owned(),
                Some("query".to_owned())
            ),
            (
                "root".to_owned(),
                "Pet".to_owned(),
                Some("mutation".to_owned())
            )
        ]
    );
    assert_eq!(
        got.constraints["Pet.owner"],
        [
            ("type".to_owned(), "Pet".to_owned()),
            ("directive".to_owned(), "@tag(name: \"o\")".to_owned()),
            ("deprecated".to_owned(), "Use \"keeper\".".to_owned()),
        ]
    );
    assert_eq!(
        got.constraints["Query.search(filter:)"][1],
        ("default".to_owned(), "{text: \"a\"}".to_owned())
    );
    assert_eq!(
        got.constraints["Filter"],
        [("one-of".to_owned(), "true".to_owned())]
    );
    assert_eq!(
        got.constraints["ID"],
        [("built-in".to_owned(), "true".to_owned())]
    );
}

#[test]
fn the_canonical_text_of_every_read_document_reads_as_it() {
    for choice in choices() {
        let doc = document(choice);
        let Some(text) = canonical_text(&doc) else {
            continue;
        };
        assert!(
            text.starts_with("schema {") || text.starts_with("\"S\"\nschema {"),
            "{text}"
        );
        assert!(render(&doc).contains("type Query"));
    }
}

#[test]
fn strings_take_the_first_form_whose_value_they_are() {
    assert_eq!(string("a b").as_deref(), Some("\"a b\""));
    assert_eq!(string("a\"b").as_deref(), Some("\"\"\"\na\"b\n\"\"\""));
    assert_eq!(string("  a\"").as_deref(), Some("\"\"\"  a\"\n\"\"\""));
    assert_eq!(string("a\"\"\"b"), None);
}
