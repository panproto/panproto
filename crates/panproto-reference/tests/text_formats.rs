//! Direct witnesses for the text-formats reference: TOML key resolution
//! on the examples of TOML 1.0.0 and YAML directive acceptance in source
//! order, with the bounded scopes' closed-form counts.

use panproto_reference::enumerate::text_formats::{
    toml_documents, toml_documents_count, yaml_directive_sequences,
    yaml_directive_sequences_count,
};
use panproto_reference::pages::text_formats::toml::{Item, KeyRefusal, Statement, resolve_keys};
use panproto_reference::pages::text_formats::yaml::{Directive, DirectiveRefusal, read_directives};

fn path(dotted: &str) -> Vec<String> {
    dotted.split('.').map(str::to_owned).collect()
}

fn pair(dotted: &str) -> Statement {
    Statement::Pair(path(dotted))
}

fn header(dotted: &str) -> Statement {
    Statement::Header(path(dotted))
}

fn array(dotted: &str) -> Statement {
    Statement::ArrayHeader(path(dotted))
}

fn table(entries: Vec<(&str, Item)>) -> Item {
    Item::Table(
        entries
            .into_iter()
            .map(|(key, item)| (key.to_owned(), item))
            .collect(),
    )
}

fn root(statements: &[Statement]) -> Item {
    match resolve_keys(statements) {
        Some(Ok(entries)) => Item::Table(entries),
        other => panic!("{statements:?} did not resolve: {other:?}"),
    }
}

fn refusal(statements: &[Statement]) -> (usize, KeyRefusal) {
    match resolve_keys(statements) {
        Some(Err(refusal)) => refusal,
        other => panic!("{statements:?} was not refused: {other:?}"),
    }
}

#[test]
fn the_bounded_scopes_have_their_closed_form_counts() {
    assert_eq!(toml_documents().len() as u64, toml_documents_count());
    assert_eq!(toml_documents_count(), 1 + 18 + 18 * 18 + 18 * 18 * 18);
    assert_eq!(
        yaml_directive_sequences().len() as u64,
        yaml_directive_sequences_count()
    );
    assert_eq!(yaml_directive_sequences_count(), 1 + 6 + 36 + 216);
    assert!(
        toml_documents()
            .iter()
            .all(|document| resolve_keys(document).is_some())
    );
}

#[test]
fn toml_key_resolution_follows_the_toml_examples() {
    // The dotted-key and sub-table examples of the TOML 1.0.0 Table section:
    // `[fruit] apple.color = ...; apple.taste.sweet = ...; [fruit.apple.texture]
    // smooth = ...`.
    assert_eq!(
        root(&[
            header("fruit"),
            pair("apple.color"),
            pair("apple.taste.sweet"),
            header("fruit.apple.texture"),
            pair("smooth"),
        ]),
        table(vec![(
            "fruit",
            table(vec![(
                "apple",
                table(vec![
                    ("color", Item::Value(1)),
                    ("taste", table(vec![("sweet", Item::Value(2))])),
                    ("texture", table(vec![("smooth", Item::Value(4))])),
                ]),
            )]),
        )])
    );
    // `[fruit.apple]` and `[fruit.apple.taste]` are invalid there.
    assert_eq!(
        refusal(&[header("fruit"), pair("apple.color"), header("fruit.apple")]),
        (2, KeyRefusal::TableRedefined)
    );
    assert_eq!(
        refusal(&[
            header("fruit"),
            pair("apple.taste.sweet"),
            header("fruit.apple.taste"),
        ]),
        (2, KeyRefusal::TableRedefined)
    );
    // A super-table may be defined after a sub-table created it.
    assert_eq!(
        root(&[header("x.y"), pair("v"), header("x"), pair("w")]),
        table(vec![(
            "x",
            table(vec![
                ("y", table(vec![("v", Item::Value(1))])),
                ("w", Item::Value(3)),
            ]),
        )])
    );
    // Arrays of tables: a sub-table header enters the last element.
    assert_eq!(
        root(&[
            array("fruits"),
            pair("name"),
            array("fruits.varieties"),
            pair("name"),
            array("fruits"),
            pair("name"),
        ]),
        table(vec![(
            "fruits",
            Item::Tables(vec![
                vec![
                    ("name".to_owned(), Item::Value(1)),
                    (
                        "varieties".to_owned(),
                        Item::Tables(vec![vec![("name".to_owned(), Item::Value(3))]]),
                    ),
                ],
                vec![("name".to_owned(), Item::Value(5))],
            ]),
        )])
    );
    assert_eq!(
        refusal(&[array("a"), header("a")]),
        (1, KeyRefusal::TableRedefined)
    );
    assert_eq!(
        refusal(&[header("a"), array("a")]),
        (1, KeyRefusal::ArrayConflict)
    );
    assert_eq!(
        refusal(&[pair("a"), pair("a.b")]),
        (1, KeyRefusal::DottedExtension)
    );
    assert_eq!(
        refusal(&[pair("a.b"), pair("a")]),
        (1, KeyRefusal::DuplicateKey)
    );
    assert_eq!(
        refusal(&[pair("a"), header("a.b")]),
        (1, KeyRefusal::HeaderExtendsValue)
    );
    // Dotted keys extend only the tables the same table's dotted keys made.
    assert_eq!(
        refusal(&[header("a.b"), pair("c"), header("a"), pair("b.d")]),
        (3, KeyRefusal::DottedExtension)
    );
    assert_eq!(resolve_keys(&[Statement::Pair(Vec::new())]), None);
}

#[test]
fn yaml_directives_are_refused_in_source_order() {
    const V11: Directive = Directive::Version { major: 1, minor: 1 };
    const V12: Directive = Directive::Version { major: 1, minor: 2 };
    const V20: Directive = Directive::Version { major: 2, minor: 0 };
    assert_eq!(read_directives(&[]), Ok(()));
    assert_eq!(read_directives(&[V12, Directive::Reserved]), Ok(()));
    assert_eq!(read_directives(&[V11]), Err((0, DirectiveRefusal::Version)));
    assert_eq!(read_directives(&[V20]), Err((0, DirectiveRefusal::Version)));
    assert_eq!(
        read_directives(&[V12, V11]),
        Err((1, DirectiveRefusal::Repeated))
    );
    assert_eq!(
        read_directives(&[V11, V12]),
        Err((0, DirectiveRefusal::Version))
    );
    assert_eq!(
        read_directives(&[Directive::Reserved, Directive::Tag, V11]),
        Err((1, DirectiveRefusal::Tag))
    );
}
