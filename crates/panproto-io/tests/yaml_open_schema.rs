//! YAML extraction against a schema that declares no edges.
//!
//! An open schema is the normal case for reading a document whose shape is
//! not known in advance, and it is what the JSON extractor already handles by
//! synthesising one edge per child. The YAML extractor did not: a sequence
//! reaching a domain vertex with no item edge collected no children at all,
//! and a mapping folded its pairs into `extra_fields`, so neither produced a
//! node an edit could address. A document parsed that way looks empty.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::wtype::WInstance;
use panproto_io::unified_codec::UnifiedCodec;
use panproto_schema::{Protocol, Schema, SchemaBuilder};

/// A schema whose root vertex declares no outgoing edges.
fn bare_root_schema() -> Schema {
    let proto = Protocol {
        name: "test".into(),
        schema_theory: "ThtestSchema".into(),
        instance_theory: "ThtestInstance".into(),
        edge_rules: vec![],
        obj_kinds: vec![],
        constraint_sorts: vec![],
        ..Protocol::default()
    };
    SchemaBuilder::new(&proto)
        .vertex("root", "object", None)
        .expect("root vertex")
        .build()
        .expect("build schema")
}

fn parse(input: &[u8]) -> WInstance {
    let codec = UnifiedCodec::yaml("test").expect("yaml codec");
    let schema = bare_root_schema();
    codec
        .parse_wtype_preserving(&schema, input)
        .expect("parse")
        .0
}

/// The values of `root`'s children, in arc order.
fn child_values(inst: &WInstance) -> Vec<Option<FieldPresence>> {
    let mut children: Vec<u32> = inst
        .arcs
        .iter()
        .filter(|(p, _, _)| *p == inst.root)
        .map(|(_, c, _)| *c)
        .collect();
    children.sort_unstable();
    children
        .into_iter()
        .map(|id| inst.nodes[&id].value.clone())
        .collect()
}

#[test]
fn a_root_sequence_extracts_every_item() {
    let inst = parse(b"- 1\n- 2\n- 3\n");
    assert_eq!(
        child_values(&inst),
        vec![
            Some(FieldPresence::Present(Value::Int(1))),
            Some(FieldPresence::Present(Value::Int(2))),
            Some(FieldPresence::Present(Value::Int(3))),
        ]
    );
}

#[test]
fn a_root_mapping_extracts_every_pair_as_a_child() {
    let inst = parse(b"name: Alice\nvalue: 42\n");
    let mut keys: Vec<String> = inst
        .arcs
        .iter()
        .filter(|(p, _, _)| *p == inst.root)
        .filter_map(|(_, _, e)| e.name.as_ref().map(std::string::ToString::to_string))
        .collect();
    keys.sort();
    assert_eq!(keys, vec!["name".to_owned(), "value".to_owned()]);
    assert_eq!(
        child_values(&inst),
        vec![
            Some(FieldPresence::Present(Value::Str("Alice".into()))),
            Some(FieldPresence::Present(Value::Int(42))),
        ]
    );
}

#[test]
fn a_nested_sequence_of_mappings_keeps_its_depth() {
    let inst = parse(b"clients:\n  - name: a\n  - name: b\n");
    // root -> clients -> two items -> one `name` each.
    assert_eq!(inst.nodes.len(), 6, "instance: {inst:?}");
    let names: Vec<String> = inst
        .nodes
        .values()
        .filter_map(|n| match &n.value {
            Some(FieldPresence::Present(Value::Str(s))) => Some(s.clone()),
            _ => None,
        })
        .collect();
    let mut names = names;
    names.sort();
    assert_eq!(names, vec!["a".to_owned(), "b".to_owned()]);
}

#[test]
fn flow_collections_extract_like_their_block_forms() {
    let inst = parse(b"nums: [1, 2, 3]\n");
    let ints: Vec<i64> = inst
        .nodes
        .values()
        .filter_map(|n| match &n.value {
            Some(FieldPresence::Present(Value::Int(i))) => Some(*i),
            _ => None,
        })
        .collect();
    let mut ints = ints;
    ints.sort_unstable();
    assert_eq!(ints, vec![1, 2, 3]);
}
