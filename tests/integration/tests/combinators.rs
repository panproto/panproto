//! Integration tests for protolens combinators.
//!
//! Verifies that derived combinators (rename_field, remove_field, add_field,
//! hoist_field, pipeline) produce correct schema transformations and satisfy
//! lens laws on concrete instances.

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
    nodes.insert(
        root_id,
        Node::new(root_id, root_vertex),
    );
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
    WInstance::new(
        nodes,
        arcs,
        Vec::new(),
        root_id,
        Name::from(root_vertex),
    )
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

    let instance = make_instance("post", &[
        ("text", "text", "hello"),
        ("tags", "tags", "[\"rust\"]"),
    ]);

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

    let instance = make_instance("post", &[
        ("text", "text", "hello"),
        ("tags", "tags", "[\"rust\"]"),
    ]);
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
