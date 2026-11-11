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
