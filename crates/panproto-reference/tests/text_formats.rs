//! Direct witnesses for the text-formats reference: TOML key resolution
//! on the examples of TOML 1.0.0 and YAML directive acceptance in source
//! order, with the bounded scopes' closed-form counts.

use panproto_reference::enumerate::text_formats::{
    toml_documents, toml_documents_count, yaml_directive_sequences, yaml_directive_sequences_count,
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

#[test]
fn whole_document_scopes_have_their_closed_form_counts() {
    use panproto_reference::enumerate::text_formats::{
        toml_token_documents, toml_token_documents_count, yaml_block_documents,
        yaml_block_documents_count,
    };
    assert_eq!(
        toml_token_documents().len() as u64,
        toml_token_documents_count()
    );
    assert_eq!(toml_token_documents_count(), 1 + 48 + 48 * 48);
    assert_eq!(
        yaml_block_documents().len() as u64,
        yaml_block_documents_count()
    );
    assert_eq!(yaml_block_documents_count(), 13_271);
}

#[test]
fn toml_documents_read_their_tables_and_scalars() {
    use panproto_reference::pages::text_formats::toml::{DocumentRefusal, Read, read_document};
    use panproto_reference::value::Value;
    let statements = [pair("a.b"), header("c"), pair("d")];
    let tokens: [&[u8]; 3] = [b"1", b"", b"'x'"];
    assert_eq!(
        read_document(&statements, &tokens),
        Some(Ok(vec![
            (
                "a".to_owned(),
                Read::Table(vec![("b".to_owned(), Read::Value(Value::Int(1)))])
            ),
            (
                "c".to_owned(),
                Read::Table(vec![("d".to_owned(), Read::Value(Value::Str("x".into())))])
            ),
        ]))
    );
    assert!(matches!(
        read_document(&[pair("a"), pair("b")], &[b"01", b"1"]),
        Some(Err(DocumentRefusal::Scalar(0, _)))
    ));
    assert_eq!(
        read_document(&[pair("a"), pair("a")], &[b"01", b"1"]),
        Some(Err(DocumentRefusal::Key(1, KeyRefusal::DuplicateKey)))
    );
}

#[test]
fn yaml_block_documents_render_and_read_as_written() {
    use panproto_reference::pages::text_formats::yaml::{
        Node, Read, read_block_document, render_block,
    };
    use panproto_reference::value::Value;
    let document = Node::Mapping(vec![
        ("a".to_owned(), Node::Scalar("yes".to_owned())),
        (
            "b".to_owned(),
            Node::Sequence(vec![
                Node::Scalar(String::new()),
                Node::Mapping(vec![("a".to_owned(), Node::Scalar("-1".to_owned()))]),
                Node::Sequence(Vec::new()),
            ]),
        ),
    ]);
    assert_eq!(
        render_block(&document),
        "a: yes\nb:\n  -\n  -\n    a: -1\n  - []\n"
    );
    assert_eq!(
        read_block_document(&document),
        Ok(Read::Mapping(vec![
            ("a".to_owned(), Read::Value(Value::Str("yes".into()))),
            (
                "b".to_owned(),
                Read::Sequence(vec![
                    Read::Value(Value::Null),
                    Read::Mapping(vec![("a".to_owned(), Read::Value(Value::Int(-1)))]),
                    Read::Sequence(Vec::new()),
                ])
            ),
        ]))
    );
}
