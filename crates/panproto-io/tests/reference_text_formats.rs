//! Production text routes against the text-formats reference: TOML key
//! resolution and YAML directive acceptance, compared outcome for outcome
//! on the bounded scopes and on generated longer documents.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{NodeShape, WInstance};
use panproto_io::ParseInstanceError;
use panproto_io::unified_codec::UnifiedCodec;
use panproto_reference::enumerate::text_formats::{toml_documents, yaml_directive_sequences};
use panproto_reference::pages::text_formats::toml::{Item, KeyRefusal, Statement, resolve_keys};
use panproto_reference::pages::text_formats::yaml::{Directive, DirectiveRefusal, read_directives};
use panproto_schema::{Protocol, Schema, SchemaBuilder};
use proptest::prelude::*;

fn open_schema(name: &str) -> Schema {
    let proto = Protocol {
        name: name.into(),
        schema_theory: format!("Th{name}Schema"),
        instance_theory: format!("Th{name}Instance"),
        ..Default::default()
    };
    SchemaBuilder::new(&proto)
        .vertex("root", "object", None)
        .expect("vertex")
        .build()
        .expect("schema")
}

/// The TOML lines of `statements`, one per statement, with the byte offset
/// at which each starts. A pair's value is its statement index.
fn render_toml(statements: &[Statement]) -> (String, Vec<usize>) {
    let mut text = String::new();
    let mut starts = Vec::new();
    for (index, statement) in statements.iter().enumerate() {
        starts.push(text.len());
        let line = match statement {
            Statement::Pair(keys) => format!("{} = {index}\n", keys.join(".")),
            Statement::Header(keys) => format!("[{}]\n", keys.join(".")),
            Statement::ArrayHeader(keys) => format!("[[{}]]\n", keys.join(".")),
        };
        text.push_str(&line);
    }
    (text, starts)
}

fn table_of(instance: &WInstance, node_id: u32) -> Vec<(String, Item)> {
    instance
        .arcs
        .iter()
        .filter(|(parent, _, _)| *parent == node_id)
        .map(|(_, child, edge)| {
            (
                edge.name.as_deref().expect("a keyed arc").to_owned(),
                item_of(instance, *child),
            )
        })
        .collect()
}

fn item_of(instance: &WInstance, node_id: u32) -> Item {
    let node = &instance.nodes[&node_id];
    match &node.value {
        Some(FieldPresence::Present(Value::Int(index))) => {
            Item::Value(usize::try_from(*index).expect("a statement index"))
        }
        Some(other) => panic!("unexpected value {other:?}"),
        None if node.shape == NodeShape::List => Item::Tables(
            instance
                .arcs
                .iter()
                .filter(|(parent, _, _)| *parent == node_id)
                .map(|(_, child, _)| table_of(instance, *child))
                .collect(),
        ),
        None => Item::Table(table_of(instance, node_id)),
    }
}

/// The refusal class a production message names.
fn refusal_class(message: &str) -> KeyRefusal {
    if message.contains("duplicate TOML key") {
        KeyRefusal::DuplicateKey
    } else if message.contains("TOML dotted key") {
        KeyRefusal::DottedExtension
    } else if message.contains("which holds a value rather than a table") {
        KeyRefusal::HeaderExtendsValue
    } else if message.contains("defines a table that is already defined") {
        KeyRefusal::TableRedefined
    } else if message.contains("appends to a key that is not an array of tables") {
        KeyRefusal::ArrayConflict
    } else {
        panic!("an unclassified TOML refusal: {message}")
    }
}

/// The byte offset a production message names.
fn named_offset(message: &str) -> usize {
    let (_, tail) = message.split_once(" at byte ").expect("a byte offset");
    tail.split(|character: char| !character.is_ascii_digit())
        .next()
        .and_then(|digits| digits.parse().ok())
        .expect("a decimal offset")
}

fn production_toml(
    codec: &UnifiedCodec,
    schema: &Schema,
    statements: &[Statement],
) -> Result<Vec<(String, Item)>, (usize, KeyRefusal)> {
    let (text, starts) = render_toml(statements);
    match codec.parse_wtype_preserving(schema, text.as_bytes()) {
        Ok((instance, complement)) => {
            assert_eq!(
                codec
                    .emit_wtype_preserving(schema, &instance, &complement)
                    .expect("unedited emission"),
                text.as_bytes(),
                "{text:?}"
            );
            Ok(table_of(&instance, instance.root))
        }
        Err(ParseInstanceError::Parse { message, .. }) => {
            let offset = named_offset(&message);
            let index = starts
                .iter()
                .position(|start| *start == offset)
                .unwrap_or_else(|| panic!("{message} names no statement start of {text:?}"));
            Err((index, refusal_class(&message)))
        }
        Err(other) => panic!("{text:?}: unexpected error {other:?}"),
    }
}

fn assert_toml_agrees(codec: &UnifiedCodec, schema: &Schema, statements: &[Statement]) {
    let expected = resolve_keys(statements).expect("every statement names a key");
    assert_eq!(
        production_toml(codec, schema, statements),
        expected,
        "{:?}",
        render_toml(statements).0
    );
}

#[test]
fn toml_key_resolution_matches_the_reference_on_every_bounded_document() {
    let codec = UnifiedCodec::toml("toml").expect("TOML codec");
    let schema = open_schema("toml");
    let documents = toml_documents();
    assert_eq!(documents.len(), 6175);
    for statements in &documents {
        assert_toml_agrees(&codec, &schema, statements);
    }
}

fn statement_strategy() -> impl Strategy<Value = Statement> {
    let path = prop::collection::vec(prop::sample::select(vec!["a", "b", "c"]), 1..=3)
        .prop_map(|keys| keys.into_iter().map(str::to_owned).collect::<Vec<_>>());
    (0_u8..3, path).prop_map(|(form, keys)| match form {
        0 => Statement::Pair(keys),
        1 => Statement::Header(keys),
        _ => Statement::ArrayHeader(keys),
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn randomized_toml_key_resolution_matches_the_reference(
        statements in prop::collection::vec(statement_strategy(), 0..=8)
    ) {
        let codec = UnifiedCodec::toml("toml").expect("TOML codec");
        let schema = open_schema("toml");
        assert_toml_agrees(&codec, &schema, &statements);
    }
}

fn render_directive(directive: Directive) -> String {
    match directive {
        Directive::Version { major, minor } => format!("%YAML {major}.{minor}\n"),
        Directive::Tag => "%TAG !e! tag:example.com,2000:\n".to_owned(),
        Directive::Reserved => "%FOO bar\n".to_owned(),
    }
}

#[test]
fn yaml_directive_acceptance_matches_the_reference_on_every_bounded_sequence() {
    let codec = UnifiedCodec::yaml("yaml").expect("YAML codec");
    let schema = open_schema("yaml");
    let sequences = yaml_directive_sequences();
    assert_eq!(sequences.len(), 259);
    for directives in &sequences {
        let mut text = String::new();
        let mut starts = Vec::new();
        for directive in directives {
            starts.push(text.len());
            text.push_str(&render_directive(*directive));
        }
        text.push_str("---\na: 1\n");
        let production = match codec.parse_wtype_preserving(&schema, text.as_bytes()) {
            Ok((instance, complement)) => {
                assert_eq!(
                    codec
                        .emit_wtype_preserving(&schema, &instance, &complement)
                        .expect("unedited emission"),
                    text.as_bytes(),
                    "{text:?}"
                );
                Ok(())
            }
            Err(ParseInstanceError::Parse { message, .. }) => {
                let offset = named_offset(&message);
                let index = starts
                    .iter()
                    .position(|start| *start == offset)
                    .unwrap_or_else(|| panic!("{message} names no directive of {text:?}"));
                let class = if message.contains("repeats the %YAML directive") {
                    DirectiveRefusal::Repeated
                } else if message.contains("this route reads YAML 1.2 only") {
                    DirectiveRefusal::Version
                } else if message.contains("%TAG directive") {
                    DirectiveRefusal::Tag
                } else {
                    panic!("an unclassified YAML refusal: {message}")
                };
                Err((index, class))
            }
            Err(other) => panic!("{text:?}: unexpected error {other:?}"),
        };
        assert_eq!(production, read_directives(directives), "{text:?}");
    }
}

/// A production scalar as the reference value it must equal.
fn reference_value(presence: Option<&FieldPresence>) -> panproto_reference::value::Value {
    use panproto_reference::value::Value as R;
    match presence {
        None | Some(FieldPresence::Null | FieldPresence::Present(Value::Null)) => R::Null,
        Some(FieldPresence::Present(Value::Int(number))) => R::Int(*number),
        Some(FieldPresence::Present(Value::Str(text))) => R::Str(text.clone()),
        Some(FieldPresence::Present(Value::Bool(flag))) => R::Bool(*flag),
        other => panic!("a value outside the bounded scope: {other:?}"),
    }
}

mod toml_whole {
    use super::*;
    use panproto_reference::pages::text_formats::toml::{DocumentRefusal, Read, read_document};

    fn table(instance: &WInstance, node_id: u32) -> Vec<(String, Read)> {
        instance
            .arcs
            .iter()
            .filter(|(parent, _, _)| *parent == node_id)
            .map(|(_, child, edge)| {
                (
                    edge.name.as_deref().expect("a keyed arc").to_owned(),
                    item(instance, *child),
                )
            })
            .collect()
    }

    fn item(instance: &WInstance, node_id: u32) -> Read {
        let node = &instance.nodes[&node_id];
        if node.value.is_some() {
            return Read::Value(reference_value(node.value.as_ref()));
        }
        if node.shape == NodeShape::List {
            return Read::Tables(
                instance
                    .arcs
                    .iter()
                    .filter(|(parent, _, _)| *parent == node_id)
                    .map(|(_, child, _)| table(instance, *child))
                    .collect(),
            );
        }
        Read::Table(table(instance, node_id))
    }

    /// The outcome both sides are compared on: the read tables, a key
    /// refusal with its statement and class, or a scalar refusal.
    #[derive(Debug, PartialEq)]
    enum Outcome {
        Read(Vec<(String, Read)>),
        Key(usize, KeyRefusal),
        Scalar,
    }

    fn render(document: &[(Statement, Vec<u8>)]) -> (String, Vec<usize>) {
        let mut text = String::new();
        let mut starts = Vec::new();
        for (statement, token) in document {
            starts.push(text.len());
            let line = match statement {
                Statement::Pair(keys) => {
                    format!("{} = {}\n", keys.join("."), String::from_utf8_lossy(token))
                }
                Statement::Header(keys) => format!("[{}]\n", keys.join(".")),
                Statement::ArrayHeader(keys) => format!("[[{}]]\n", keys.join(".")),
            };
            text.push_str(&line);
        }
        (text, starts)
    }

    pub fn assert_agrees(document: &[(Statement, Vec<u8>)]) {
        let codec = UnifiedCodec::toml("toml").expect("TOML codec");
        let schema = open_schema("toml");
        let statements: Vec<Statement> = document.iter().map(|(s, _)| s.clone()).collect();
        let tokens: Vec<&[u8]> = document.iter().map(|(_, t)| t.as_slice()).collect();
        let expected = match read_document(&statements, &tokens).expect("keys named") {
            Ok(read) => Outcome::Read(read),
            Err(DocumentRefusal::Key(index, class)) => Outcome::Key(index, class),
            Err(DocumentRefusal::Scalar(..)) => Outcome::Scalar,
        };
        let (text, starts) = render(document);
        let actual = match codec.parse_wtype_preserving(&schema, text.as_bytes()) {
            Ok((instance, complement)) => {
                assert_eq!(
                    codec
                        .emit_wtype_preserving(&schema, &instance, &complement)
                        .expect("unedited emission"),
                    text.as_bytes(),
                    "{text:?}"
                );
                Outcome::Read(table(&instance, instance.root))
            }
            Err(ParseInstanceError::Parse { message, .. })
                if message.contains(" at byte ")
                    && (message.contains("TOML dotted key")
                        || message.contains("duplicate TOML key")
                        || message.contains("TOML header")) =>
            {
                let offset = named_offset(&message);
                let index = starts
                    .iter()
                    .position(|start| *start == offset)
                    .expect("a statement");
                Outcome::Key(index, refusal_class(&message))
            }
            Err(_) => Outcome::Scalar,
        };
        // A document with both a refused scalar and a refused key is refused
        // on either side; which refusal each names first is not compared.
        let both_refuse = |outcome: &Outcome| !matches!(outcome, Outcome::Read(_));
        let has_bad_token = tokens.iter().zip(&statements).any(|(token, statement)| {
            matches!(statement, Statement::Pair(_))
                && panproto_reference::value_io::parse(
                    panproto_reference::value_io::Route::Toml,
                    token,
                )
                .is_err()
        });
        if has_bad_token && both_refuse(&expected) {
            assert!(both_refuse(&actual), "{text:?}: {actual:?}");
        } else {
            assert_eq!(actual, expected, "{text:?}");
        }
    }
}

#[test]
fn toml_documents_read_as_the_reference_on_every_bounded_document() {
    let documents = panproto_reference::enumerate::text_formats::toml_token_documents();
    assert_eq!(documents.len(), 2353);
    for document in &documents {
        toml_whole::assert_agrees(document);
    }
}

mod yaml_whole {
    use super::*;
    use panproto_reference::pages::text_formats::yaml::{Read, read_block_document, render_block};

    fn read(instance: &WInstance, node_id: u32) -> Read {
        let node = &instance.nodes[&node_id];
        let children: Vec<_> = instance
            .arcs
            .iter()
            .filter(|(parent, _, _)| *parent == node_id)
            .collect();
        if node.value.is_some() {
            return Read::Value(reference_value(node.value.as_ref()));
        }
        if node.shape == NodeShape::List {
            return Read::Sequence(
                children
                    .iter()
                    .map(|(_, child, _)| read(instance, *child))
                    .collect(),
            );
        }
        Read::Mapping(
            children
                .iter()
                .map(|(_, child, edge)| {
                    (
                        edge.name.as_deref().expect("a keyed arc").to_owned(),
                        read(instance, *child),
                    )
                })
                .collect(),
        )
    }

    pub fn assert_agrees(document: &panproto_reference::pages::text_formats::yaml::Node) {
        let codec = UnifiedCodec::yaml("yaml").expect("YAML codec");
        let schema = open_schema("yaml");
        let text = render_block(document);
        let expected = read_block_document(document).expect("every token reads");
        let (instance, complement) = codec
            .parse_wtype_preserving(&schema, text.as_bytes())
            .unwrap_or_else(|error| panic!("{text:?} was refused: {error}"));
        assert_eq!(read(&instance, instance.root), expected, "{text:?}");
        assert_eq!(
            codec
                .emit_wtype_preserving(&schema, &instance, &complement)
                .expect("unedited emission"),
            text.as_bytes(),
            "{text:?}"
        );
    }
}

#[test]
fn yaml_block_documents_read_as_the_reference_on_every_bounded_document() {
    let documents = panproto_reference::enumerate::text_formats::yaml_block_documents();
    assert_eq!(documents.len(), 13_271);
    for document in &documents {
        yaml_whole::assert_agrees(document);
    }
}
