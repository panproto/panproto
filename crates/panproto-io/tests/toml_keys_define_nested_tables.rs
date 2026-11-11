//! TOML dotted keys and table headers define nested tables under the
//! definition rules of TOML 1.0.0: a key and a table are defined once, a
//! dotted key extends only the tables its own table's dotted keys defined,
//! and a header may define a table a longer header created implicitly.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;

use panproto_inst::WInstance;
use panproto_inst::value::{FieldPresence, Value};
use panproto_io::ParseInstanceError;
use panproto_io::traits::InstanceParser;
use panproto_io::unified_codec::UnifiedCodec;
use panproto_schema::{Protocol, Schema, SchemaBuilder};

fn open_schema() -> Schema {
    let proto = Protocol {
        name: "toml".into(),
        schema_theory: "ThTomlSchema".into(),
        instance_theory: "ThTomlInstance".into(),
        ..Default::default()
    };
    SchemaBuilder::new(&proto)
        .vertex("root", "object", None)
        .expect("vertex")
        .build()
        .expect("schema")
}

fn codec() -> UnifiedCodec {
    UnifiedCodec::toml("toml").expect("TOML codec")
}

/// A tree of the scalars, lists and maps an instance holds, read from its
/// arcs alone, so a flattened dotted key and a nested table differ.
#[derive(Debug, PartialEq)]
enum Shape {
    Scalar(String),
    List(Vec<Self>),
    Map(BTreeMap<String, Self>),
}

fn scalar_text(value: &Value) -> String {
    match value {
        Value::Str(text) => format!("s:{text}"),
        Value::Int(number) => format!("i:{number}"),
        Value::Bool(flag) => format!("b:{flag}"),
        other => panic!("the corpus holds strings, integers and booleans only: {other:?}"),
    }
}

fn shape_of(instance: &WInstance, node_id: u32) -> Shape {
    let node = &instance.nodes[&node_id];
    if let Some(FieldPresence::Present(value)) = &node.value {
        return Shape::Scalar(scalar_text(value));
    }
    let arcs: Vec<_> = instance
        .arcs
        .iter()
        .filter(|(parent, _, _)| *parent == node_id)
        .collect();
    if node.shape == panproto_inst::NodeShape::List {
        return Shape::List(
            arcs.iter()
                .map(|(_, child, _)| shape_of(instance, *child))
                .collect(),
        );
    }
    Shape::Map(
        arcs.iter()
            .map(|(_, child, edge)| {
                (
                    edge.name.as_deref().expect("a named key").to_owned(),
                    shape_of(instance, *child),
                )
            })
            .collect(),
    )
}

/// The same tree as the `toml` crate reads the document.
fn shape_of_toml(value: &toml::Value) -> Shape {
    match value {
        toml::Value::String(text) => Shape::Scalar(format!("s:{text}")),
        toml::Value::Integer(number) => Shape::Scalar(format!("i:{number}")),
        toml::Value::Boolean(flag) => Shape::Scalar(format!("b:{flag}")),
        toml::Value::Array(items) => Shape::List(items.iter().map(shape_of_toml).collect()),
        toml::Value::Table(table) => Shape::Map(
            table
                .iter()
                .map(|(key, value)| (key.clone(), shape_of_toml(value)))
                .collect(),
        ),
        other => panic!("the corpus holds strings, integers and booleans only: {other:?}"),
    }
}

/// Documents valid under TOML 1.0.0 that use dotted keys, sub-table
/// headers, implicit tables and arrays of tables.
const VALID: &[&str] = &[
    "a.b = 1\n",
    "a.b.c = 1\na.b.d = 2\na.e = 3\n",
    "name = \"x\"\nsite.\"google.com\" = true\n",
    "\"a.b\" = 1\na.b = 2\n",
    "a . b = 1\n",
    "[fruit]\napple.color = \"red\"\napple.taste.sweet = true\n\n[fruit.apple.texture]\nsmooth = true\n",
    "[x.y.z.w]\nv = 1\n[x]\nu = 2\n",
    "[a.b]\nc = 1\n[a.d]\ne = 2\n",
    "[[products]]\nname = \"a\"\n[products.dims]\nw = 1\n[[products]]\nname = \"b\"\n",
    "[[fruits]]\nname = \"apple\"\n[[fruits.varieties]]\nname = \"red\"\n[[fruits.varieties]]\nname = \"green\"\n[[fruits]]\nname = \"banana\"\n",
    "[[a.b]]\nc = 1\n",
    "point = { x.y = 1, x.z = 2, w = 3 }\n",
    "[t]\nin = { a.b = 1 }\n",
    "a.b = [1, 2]\nc = [{ d.e = 1 }]\n",
];

#[test]
fn dotted_keys_and_headers_read_the_tables_the_toml_crate_reads() {
    let schema = open_schema();
    let codec = codec();
    for document in VALID {
        let (instance, complement) = codec
            .parse_wtype_preserving(&schema, document.as_bytes())
            .unwrap_or_else(|error| panic!("{document:?} was refused: {error}"));
        let expected = shape_of_toml(&toml::Value::Table(
            toml::from_str::<toml::Table>(document).expect("valid TOML"),
        ));
        assert_eq!(shape_of(&instance, instance.root), expected, "{document:?}");
        assert_eq!(
            codec
                .emit_wtype_preserving(&schema, &instance, &complement)
                .expect("unedited emission"),
            document.as_bytes(),
            "{document:?}"
        );
        let canonical = codec
            .parse_wtype(&schema, document.as_bytes())
            .expect("complement-free read");
        assert_eq!(
            shape_of(&canonical, canonical.root),
            expected,
            "{document:?}"
        );
    }
}

#[test]
fn a_value_under_a_dotted_key_is_edited_in_place() {
    let schema = open_schema();
    let codec = codec();
    let input = "# fruit\n[fruit]\napple.color = \"red\"  # note\napple.taste.sweet = true\n";
    let (mut instance, complement) = codec
        .parse_wtype_preserving(&schema, input.as_bytes())
        .unwrap();
    let node = instance
        .nodes
        .values_mut()
        .find(|node| node.anchor.as_ref() == "root:fruit:apple:color")
        .expect("the dotted key's value");
    node.value = Some(FieldPresence::Present(Value::Str("green".into())));
    let emitted = codec
        .emit_wtype_preserving(&schema, &instance, &complement)
        .unwrap();
    assert_eq!(
        String::from_utf8(emitted).unwrap(),
        input.replace("\"red\"", "\"green\"")
    );
}

#[test]
fn a_key_or_table_defined_twice_is_refused_with_the_key_and_offset() {
    let schema = open_schema();
    let codec = codec();
    for (document, expected) in [
        (
            "a = 1\na.b = 2\n",
            "TOML dotted key a.b at byte 6 extends key a",
        ),
        ("a.b = 1\na = 2\n", "duplicate TOML key a at byte 8"),
        ("a.b = 1\na.b = 2\n", "duplicate TOML key a.b at byte 8"),
        ("\"a\" = 1\na = 2\n", "duplicate TOML key a at byte 8"),
        ("a = { b = 1, b = 2 }\n", "duplicate TOML key b at byte 13"),
        (
            "[a]\nb = 1\n[a]\nc = 2\n",
            "TOML header [a] at byte 10 defines a table",
        ),
        (
            "[fruit]\napple.color = 1\n[fruit.apple]\n",
            "TOML header [fruit.apple] at byte 24 defines a table",
        ),
        (
            "[fruit]\napple.taste.sweet = true\n[fruit.apple.taste]\n",
            "TOML header [fruit.apple.taste] at byte 33 defines a table",
        ),
        (
            "a.b = 1\n[a]\n",
            "TOML header [a] at byte 8 defines a table",
        ),
        (
            "[a.b.c]\nz = 1\n[a]\nb.c.t = 1\n",
            "TOML dotted key b.c.t at byte 18 extends key b",
        ),
        (
            "[a.b]\nc = 1\n[a]\nb.d = 1\n",
            "TOML dotted key b.d at byte 16 extends key b",
        ),
        (
            "a = { b = 1 }\n[a.c]\n",
            "TOML header [a.c] at byte 14 extends key a",
        ),
        (
            "a = { b = 1 }\n[a]\n",
            "TOML header [a] at byte 14 defines a table",
        ),
        (
            "a = 1\n[a.b]\n",
            "TOML header [a.b] at byte 6 extends key a",
        ),
        (
            "a = [1]\n[[a]]\n",
            "TOML header [[a]] at byte 8 appends to a key",
        ),
        ("[[a]]\n[a]\n", "TOML header [a] at byte 6 defines a table"),
        (
            "[a]\n[[a]]\n",
            "TOML header [[a]] at byte 4 appends to a key",
        ),
    ] {
        match codec.parse_wtype_preserving(&schema, document.as_bytes()) {
            Err(ParseInstanceError::Parse { message, .. }) => {
                assert!(
                    message.contains(expected),
                    "{document:?}: {message:?} does not contain {expected:?}"
                );
            }
            other => panic!("{document:?} was not refused: {other:?}"),
        }
        assert!(
            toml::from_str::<toml::Table>(document).is_err(),
            "{document:?} is valid TOML"
        );
    }
}

#[test]
fn a_key_path_deeper_than_extraction_descends_is_refused() {
    let schema = open_schema();
    let codec = codec();
    let deep = vec!["k"; panproto_io::cst_extract::MAX_EXTRACT_DEPTH + 1].join(".");
    for document in [format!("{deep} = 1\n"), format!("[{deep}]\n")] {
        match codec.parse_wtype_preserving(&schema, document.as_bytes()) {
            Err(ParseInstanceError::Parse { message, .. }) => {
                assert!(message.contains("nesting exceeds"), "{message}");
            }
            other => panic!("a {deep:?} key path was not refused: {other:?}"),
        }
    }
}
