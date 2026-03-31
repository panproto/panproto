//! Integration tests for protolens combinators.
//!
//! Verifies that derived combinators (`rename_field`, `remove_field`, `add_field`,
//! `hoist_field`, `pipeline`) produce correct schema transformations and satisfy
//! lens laws on concrete instances.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::missing_panics_doc
)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::value::FieldPresence;
use panproto_inst::{Node, Value, WInstance};
use panproto_lens::{check_laws, combinators, elementary, get, put};
use panproto_schema::{Edge, Protocol, Schema, Vertex};
use smallvec::SmallVec;

/// Build a schema from vertices and edges with a given protocol name.
fn make_schema(protocol: &str, verts: &[(&str, &str)], edge_list: &[Edge]) -> Schema {
    let mut vertices = HashMap::new();
    let mut edges = HashMap::new();
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

    for (id, kind) in verts {
        vertices.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
                nsid: None,
            },
        );
    }
    for edge in edge_list {
        edges.insert(edge.clone(), edge.kind.clone());
        outgoing
            .entry(edge.src.clone())
            .or_default()
            .push(edge.clone());
        incoming
            .entry(edge.tgt.clone())
            .or_default()
            .push(edge.clone());
        between
            .entry((edge.src.clone(), edge.tgt.clone()))
            .or_default()
            .push(edge.clone());
    }
    Schema {
        protocol: protocol.into(),
        vertices,
        edges,
        hyper_edges: HashMap::new(),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        coercions: HashMap::new(),
        mergers: HashMap::new(),
        defaults: HashMap::new(),
        policies: HashMap::new(),
        outgoing,
        incoming,
        between,
    }
}

fn prop_edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: Name::from(src),
        tgt: Name::from(tgt),
        kind: Name::from("prop"),
        name: Some(Name::from(name)),
    }
}

fn make_protocol() -> Protocol {
    Protocol {
        name: "test".into(),
        ..Default::default()
    }
}

fn make_instance(root_vertex: &str, children: &[(&str, &str, &str)]) -> WInstance {
    let mut nodes = HashMap::new();
    let mut arcs = Vec::new();
    let root_id = 0u32;
    nodes.insert(root_id, Node::new(root_id, root_vertex));
    for (i, (vertex_id, edge_name, value)) in children.iter().enumerate() {
        let child_id = (i + 1) as u32;
        let mut node = Node::new(child_id, *vertex_id);
        node.value = Some(FieldPresence::Present(Value::Str((*value).into())));
        nodes.insert(child_id, node);
        arcs.push((
            root_id,
            child_id,
            Edge {
                src: Name::from(root_vertex),
                tgt: Name::from(*vertex_id),
                kind: Name::from("prop"),
                name: Some(Name::from(*edge_name)),
            },
        ));
    }
    WInstance::new(nodes, arcs, Vec::new(), root_id, Name::from(root_vertex))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn rename_field_changes_edge_label() {
    let src = make_schema(
        "test",
        &[("post", "record"), ("text", "string")],
        &[prop_edge("post", "text", "text")],
    );
    let protocol = make_protocol();

    let chain = combinators::rename_field("post", "text", "text", "body");
    let lens = chain.instantiate(&src, &protocol).unwrap();

    // Verify target schema has the renamed edge label.
    let tgt = &lens.tgt_schema;
    let post_edges: Vec<_> = tgt.outgoing_edges("post").to_vec();
    assert_eq!(post_edges.len(), 1);
    assert_eq!(post_edges[0].name.as_deref(), Some("body"));
    // Vertex ID unchanged.
    assert!(tgt.vertices.contains_key("text"));
}

#[test]
fn rename_field_satisfies_lens_laws() {
    let src = make_schema(
        "test",
        &[("post", "record"), ("text", "string")],
        &[prop_edge("post", "text", "text")],
    );
    let protocol = make_protocol();

    let chain = combinators::rename_field("post", "text", "text", "body");
    let lens = chain.instantiate(&src, &protocol).unwrap();

    let instance = make_instance("post", &[("text", "text", "hello world")]);
    check_laws(&lens, &instance).unwrap();
}

#[test]
fn remove_field_drops_vertex() {
    let src = make_schema(
        "test",
        &[("post", "record"), ("text", "string"), ("tags", "array")],
        &[
            prop_edge("post", "text", "text"),
            prop_edge("post", "tags", "tags"),
        ],
    );
    let protocol = make_protocol();

    let chain = combinators::remove_field("tags");
    let lens = chain.instantiate(&src, &protocol).unwrap();

    let tgt = &lens.tgt_schema;
    assert!(!tgt.vertices.contains_key("tags"));
    assert!(tgt.vertices.contains_key("text"));
}

#[test]
fn remove_field_complement_captures_data() {
    let src = make_schema(
        "test",
        &[("post", "record"), ("text", "string"), ("tags", "array")],
        &[
            prop_edge("post", "text", "text"),
            prop_edge("post", "tags", "tags"),
        ],
    );
    let protocol = make_protocol();

    let chain = combinators::remove_field("tags");
    let lens = chain.instantiate(&src, &protocol).unwrap();

    let instance = make_instance(
        "post",
        &[("text", "text", "hello"), ("tags", "tags", "[\"rust\"]")],
    );

    let (view, complement) = get(&lens, &instance).unwrap();
    // View should not have the tags node.
    assert_eq!(view.node_count(), 2); // post + text
    // Complement should capture the dropped tags node.
    assert!(!complement.dropped_nodes.is_empty());
    // Round-trip: put should restore the original.
    let restored = put(&lens, &view, &complement).unwrap();
    assert_eq!(restored.node_count(), instance.node_count());
}

#[test]
fn pipeline_composes_multiple_steps() {
    let src = make_schema(
        "test",
        &[("post", "record"), ("text", "string"), ("tags", "array")],
        &[
            prop_edge("post", "text", "text"),
            prop_edge("post", "tags", "tags"),
        ],
    );
    let protocol = make_protocol();

    let chain = combinators::pipeline(vec![
        combinators::rename_field("post", "text", "text", "body"),
        combinators::remove_field("tags"),
    ]);
    let lens = chain.instantiate(&src, &protocol).unwrap();

    let tgt = &lens.tgt_schema;
    // text renamed to body (edge label), tags dropped.
    let post_edges: Vec<_> = tgt.outgoing_edges("post").to_vec();
    assert_eq!(post_edges.len(), 1);
    assert_eq!(post_edges[0].name.as_deref(), Some("body"));
    assert!(!tgt.vertices.contains_key("tags"));
}

#[test]
fn pipeline_satisfies_lens_laws() {
    let src = make_schema(
        "test",
        &[("post", "record"), ("text", "string"), ("tags", "array")],
        &[
            prop_edge("post", "text", "text"),
            prop_edge("post", "tags", "tags"),
        ],
    );
    let protocol = make_protocol();

    let chain = combinators::pipeline(vec![
        combinators::rename_field("post", "text", "text", "body"),
        combinators::remove_field("tags"),
    ]);
    let lens = chain.instantiate(&src, &protocol).unwrap();

    let instance = make_instance(
        "post",
        &[("text", "text", "hello"), ("tags", "tags", "[\"rust\"]")],
    );
    check_laws(&lens, &instance).unwrap();
}

#[test]
fn rename_edge_name_elementary_is_iso() {
    let src = make_schema(
        "test",
        &[("post", "record"), ("title", "string")],
        &[prop_edge("post", "title", "title")],
    );
    let protocol = make_protocol();

    let protolens = elementary::rename_edge_name("post", "title", "title", "name");
    let chain = panproto_lens::ProtolensChain::new(vec![protolens]);
    let lens = chain.instantiate(&src, &protocol).unwrap();

    // Verify the edge label changed.
    let tgt = &lens.tgt_schema;
    let edges: Vec<_> = tgt.outgoing_edges("post").to_vec();
    assert_eq!(edges[0].name.as_deref(), Some("name"));

    // Verify complement is empty (iso).
    let instance = make_instance("post", &[("title", "title", "My Post")]);
    let (_, complement) = get(&lens, &instance).unwrap();
    assert!(complement.dropped_nodes.is_empty());
    assert!(complement.dropped_arcs.is_empty());

    check_laws(&lens, &instance).unwrap();
}

#[test]
fn scoped_rename_applies_to_sub_schema() {
    // Schema: post → words (array) → word (object) → text (string)
    let src = make_schema(
        "test",
        &[
            ("post", "record"),
            ("words", "array"),
            ("word", "object"),
            ("word_text", "string"),
        ],
        &[
            prop_edge("post", "words", "words"),
            Edge {
                src: Name::from("words"),
                tgt: Name::from("word"),
                kind: Name::from("item"),
                name: Some(Name::from("item")),
            },
            prop_edge("word", "word_text", "text"),
        ],
    );
    let protocol = make_protocol();

    // Scope a rename inside the word sub-schema.
    let inner = elementary::rename_edge_name("word", "word_text", "text", "content");
    let scoped = elementary::scoped("word", inner);
    let chain = panproto_lens::ProtolensChain::new(vec![scoped]);
    let lens = chain.instantiate(&src, &protocol).unwrap();

    // The word's edge should be renamed.
    let tgt = &lens.tgt_schema;
    let word_edges: Vec<_> = tgt.outgoing_edges("word").to_vec();
    assert_eq!(word_edges.len(), 1);
    assert_eq!(word_edges[0].name.as_deref(), Some("content"));

    // The post-level edges should be unchanged.
    let post_edges: Vec<_> = tgt.outgoing_edges("post").to_vec();
    assert_eq!(post_edges.len(), 1);
    assert_eq!(post_edges[0].name.as_deref(), Some("words"));
}

// ---------------------------------------------------------------------------
// Property-based tests
// ---------------------------------------------------------------------------

#[allow(clippy::unwrap_used)]
mod property {
    use super::*;
    use panproto_inst::value::FieldPresence;
    use proptest::prelude::*;

    /// Generate a flat schema (root + N leaf children) with prop edges,
    /// a matching instance with random string values, and a protocol.
    fn arb_schema_and_instance() -> impl Strategy<Value = (Schema, WInstance, Protocol, Vec<String>)>
    {
        (2..=5usize).prop_flat_map(|n_children| {
            prop::collection::vec("[a-z]{1,8}".prop_map(String::from), n_children..=n_children)
                .prop_map(move |values| {
                    let field_names: Vec<String> =
                        (0..n_children).map(|i| format!("field{i}")).collect();
                    let mut edges = Vec::new();
                    let vert_data: Vec<(String, String)> =
                        std::iter::once(("root".to_owned(), "object".to_owned()))
                            .chain(field_names.iter().map(|n| (n.clone(), "string".to_owned())))
                            .collect();
                    let vert_refs: Vec<(&str, &str)> = vert_data
                        .iter()
                        .map(|(a, b)| (a.as_str(), b.as_str()))
                        .collect();
                    for name in &field_names {
                        edges.push(prop_edge("root", name, name));
                    }
                    let schema = make_schema("test", &vert_refs, &edges);

                    let mut nodes = HashMap::new();
                    nodes.insert(0, Node::new(0, "root"));
                    let mut arcs = Vec::new();
                    for (i, val) in values.iter().enumerate() {
                        let nid = u32::try_from(i + 1).unwrap();
                        let field = &field_names[i];
                        let mut node = Node::new(nid, field.as_str());
                        node.value = Some(FieldPresence::Present(Value::Str(val.clone())));
                        nodes.insert(nid, node);
                        arcs.push((
                            0,
                            nid,
                            Edge {
                                src: Name::from("root"),
                                tgt: Name::from(field.as_str()),
                                kind: Name::from("prop"),
                                name: Some(Name::from(field.as_str())),
                            },
                        ));
                    }
                    let instance = WInstance::new(nodes, arcs, Vec::new(), 0, Name::from("root"));
                    let protocol = make_protocol();
                    (schema, instance, protocol, field_names)
                })
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// Renaming any field satisfies GetPut and PutGet.
        #[test]
        fn rename_field_satisfies_laws_proptest(
            (schema, instance, protocol, field_names) in arb_schema_and_instance(),
            new_suffix in "[a-z]{1,4}",
        ) {
            // Pick the first field to rename.
            let field = &field_names[0];
            let new_name = format!("{field}_{new_suffix}");
            let chain = combinators::rename_field("root", field.as_str(), field.as_str(), new_name.as_str());
            let lens = chain.instantiate(&schema, &protocol).unwrap();
            check_laws(&lens, &instance).unwrap();
        }

        /// Removing any single field satisfies GetPut and PutGet.
        #[test]
        fn remove_field_satisfies_laws_proptest(
            (schema, instance, protocol, field_names) in arb_schema_and_instance(),
        ) {
            // Remove the last field (so root still has at least one child).
            let field = field_names.last().unwrap();
            let chain = combinators::remove_field(field.as_str());
            let lens = chain.instantiate(&schema, &protocol).unwrap();
            check_laws(&lens, &instance).unwrap();
        }

        /// A pipeline of rename + remove satisfies GetPut and PutGet.
        #[test]
        fn pipeline_satisfies_laws_proptest(
            (schema, instance, protocol, field_names) in arb_schema_and_instance(),
            new_suffix in "[a-z]{1,4}",
        ) {
            if field_names.len() < 2 {
                return Ok(());
            }
            let first = &field_names[0];
            let last = field_names.last().unwrap();
            let new_name = format!("{first}_{new_suffix}");
            let chain = combinators::pipeline(vec![
                combinators::rename_field("root", first.as_str(), first.as_str(), new_name.as_str()),
                combinators::remove_field(last.as_str()),
            ]);
            let lens = chain.instantiate(&schema, &protocol).unwrap();
            check_laws(&lens, &instance).unwrap();
        }

        /// Rename is an isomorphism: complement is always empty.
        #[test]
        fn rename_field_complement_is_empty_proptest(
            (schema, instance, protocol, field_names) in arb_schema_and_instance(),
            new_suffix in "[a-z]{1,4}",
        ) {
            let field = &field_names[0];
            let new_name = format!("{field}_{new_suffix}");
            let chain = combinators::rename_field("root", field.as_str(), field.as_str(), new_name.as_str());
            let lens = chain.instantiate(&schema, &protocol).unwrap();
            let (_, complement) = get(&lens, &instance).unwrap();
            prop_assert!(complement.dropped_nodes.is_empty());
            prop_assert!(complement.dropped_arcs.is_empty());
        }

        /// Remove then add back with default: get preserves node count minus one,
        /// and round-trip via complement restores the original.
        #[test]
        fn remove_get_put_roundtrip_proptest(
            (schema, instance, protocol, field_names) in arb_schema_and_instance(),
        ) {
            let field = field_names.last().unwrap();
            let chain = combinators::remove_field(field.as_str());
            let lens = chain.instantiate(&schema, &protocol).unwrap();
            let original_count = instance.node_count();

            let (view, complement) = get(&lens, &instance).unwrap();
            prop_assert_eq!(view.node_count(), original_count - 1);

            let restored = put(&lens, &view, &complement).unwrap();
            prop_assert_eq!(restored.node_count(), original_count);
        }
    }
}
