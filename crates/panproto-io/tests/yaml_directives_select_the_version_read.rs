//! The directives that open a YAML document decide whether the YAML 1.2
//! route reads it: `%YAML 1.2` and no directive are read, every other
//! version and a repeated `%YAML` directive are refused with the version
//! and offset, a `%TAG` directive is refused, and a reserved directive is
//! kept in the source bytes without being read.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_inst::value::{FieldPresence, Value};
use panproto_io::ParseInstanceError;
use panproto_io::unified_codec::UnifiedCodec;
use panproto_schema::{Protocol, Schema, SchemaBuilder};

fn open_schema() -> Schema {
    let proto = Protocol {
        name: "yaml".into(),
        schema_theory: "ThYamlSchema".into(),
        instance_theory: "ThYamlInstance".into(),
        ..Default::default()
    };
    SchemaBuilder::new(&proto)
        .vertex("root", "object", None)
        .expect("vertex")
        .build()
        .expect("schema")
}

#[test]
fn a_yaml_1_2_directive_and_a_reserved_directive_are_read_and_kept() {
    let schema = open_schema();
    let codec = UnifiedCodec::yaml("yaml").expect("YAML codec");
    for input in [
        "%YAML 1.2\n---\na: yes\n",
        "%YAML 1.2 # the version\n---\na: yes\n",
        "%FOO bar baz\n---\na: yes\n",
        "%FOO bar\n%YAML 1.2\n---\na: yes\n",
    ] {
        let (mut instance, complement) = codec
            .parse_wtype_preserving(&schema, input.as_bytes())
            .unwrap_or_else(|error| panic!("{input:?} was refused: {error}"));
        let node_id = instance
            .nodes
            .values()
            .find(|node| node.anchor.as_ref() == "root:a")
            .expect("the a value")
            .id;
        // Under the YAML 1.2 core schema `yes` is a string, not a boolean.
        assert_eq!(
            instance.nodes[&node_id].value,
            Some(FieldPresence::Present(Value::Str("yes".into()))),
            "{input:?}"
        );
        assert_eq!(
            codec
                .emit_wtype_preserving(&schema, &instance, &complement)
                .unwrap(),
            input.as_bytes(),
            "{input:?}"
        );
        // An edited string is written double-quoted.
        instance.nodes.get_mut(&node_id).expect("the a value").value =
            Some(FieldPresence::Present(Value::Str("later".into())));
        assert_eq!(
            String::from_utf8(
                codec
                    .emit_wtype_preserving(&schema, &instance, &complement)
                    .unwrap()
            )
            .unwrap(),
            input.replace("a: yes", "a: \"later\""),
            "{input:?}"
        );
    }
}

#[test]
fn every_other_yaml_version_a_repeated_version_and_a_tag_directive_are_refused() {
    let schema = open_schema();
    let codec = UnifiedCodec::yaml("yaml").expect("YAML codec");
    for (input, expected) in [
        (
            "%YAML 1.1\n---\na: yes\n",
            "YAML %YAML 1.1 directive at byte 0: this route reads YAML 1.2 only",
        ),
        (
            "%YAML 1.3\n---\na: 1\n",
            "YAML %YAML 1.3 directive at byte 0: this route reads YAML 1.2 only",
        ),
        (
            "%YAML 2.0\n---\na: 1\n",
            "YAML %YAML 2.0 directive at byte 0: this route reads YAML 1.2 only",
        ),
        (
            "%YAML 1.2\n%YAML 1.2\n---\na: 1\n",
            "YAML document repeats the %YAML directive at byte 10",
        ),
        (
            "%TAG !e! tag:example.com,2000:\n---\na: 1\n",
            "YAML %TAG directive at byte 0: tag handle definitions are not supported",
        ),
    ] {
        match codec.parse_wtype_preserving(&schema, input.as_bytes()) {
            Err(ParseInstanceError::Parse { message, .. }) => assert!(
                message.contains(expected),
                "{input:?}: {message:?} does not contain {expected:?}"
            ),
            other => panic!("{input:?} was not refused: {other:?}"),
        }
    }
}
