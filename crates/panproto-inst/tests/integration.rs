//! Integration tests for panproto-inst.
//!
//! These tests correspond to the four test cases specified in ENGINEERING.md:
//! 1. 3-node W-type instance -> validate -> round-trip JSON
//! 2. Recursive schema (threadViewPost): 11-node instance -> restrict drops array+union -> 3 nodes
//! 3. Fan instance: 4-ary hyperedge, drop 1 child -> fan reconstruction produces valid 3-ary fan
//! 4. Set-valued functor: two tables with FK -> restrict drops one table -> verify precomposition

#![allow(clippy::implicit_hasher)]

use std::collections::{HashMap, HashSet};

use panproto_gat::Name;
use panproto_inst::{
    CompiledMigration, FInstance, Fan, FieldPresence, Node, Value, WInstance, anchor_surviving,
    functor_restrict, parse_json, reachable_from_root, reconstruct_fans, to_json, validate_wtype,
    wtype_restrict,
};
use panproto_schema::{Edge, HyperEdge, Schema, Vertex};
use smallvec::smallvec;

// ---------------------------------------------------------------------------
// Helper: build a minimal 3-vertex schema (object + two strings)
// ---------------------------------------------------------------------------
fn simple_schema() -> Schema {
    let mut vertices = HashMap::new();
    vertices.insert(
        "obj".into(),
        Vertex {
            id: "obj".into(),
            kind: "object".into(),
            nsid: None,
        },
    );
    vertices.insert(
        "name".into(),
        Vertex {
            id: "name".into(),
            kind: "string".into(),
            nsid: None,
        },
    );
    vertices.insert(
        "desc".into(),
        Vertex {
            id: "desc".into(),
            kind: "string".into(),
            nsid: None,
        },
    );

    let e1 = Edge {
        src: "obj".into(),
        tgt: "name".into(),
        kind: "prop".into(),
        name: Some("name".into()),
    };
    let e2 = Edge {
        src: "obj".into(),
        tgt: "desc".into(),
        kind: "prop".into(),
        name: Some("desc".into()),
    };

    let mut edges = HashMap::new();
    edges.insert(e1.clone(), "prop".into());
    edges.insert(e2.clone(), "prop".into());

    let mut outgoing = HashMap::new();
    outgoing.insert("obj".into(), smallvec![e1.clone(), e2.clone()]);

    let mut incoming = HashMap::new();
    incoming.insert("name".into(), smallvec![e1.clone()]);
    incoming.insert("desc".into(), smallvec![e2.clone()]);

    let mut between = HashMap::new();
    between.insert(("obj".into(), "name".into()), smallvec![e1]);
    between.insert(("obj".into(), "desc".into()), smallvec![e2]);

    Schema {
        protocol: "test".into(),
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
        outgoing,
        incoming,
        between,
    }
}

// ---------------------------------------------------------------------------
// Test 1: 3-node W-type instance -> validate -> round-trip JSON
// ---------------------------------------------------------------------------
#[test]
fn test_3_node_validate_and_round_trip() {
    let schema = simple_schema();

    // Build instance
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "obj"));
    nodes.insert(
        1,
        Node::new(1, "name").with_value(FieldPresence::Present(Value::Str("Alice".into()))),
    );
    nodes.insert(
        2,
        Node::new(2, "desc").with_value(FieldPresence::Present(Value::Str("A person".into()))),
    );

    let arcs = vec![
        (
            0,
            1,
            Edge {
                src: "obj".into(),
                tgt: "name".into(),
                kind: "prop".into(),
                name: Some("name".into()),
            },
        ),
        (
            0,
            2,
            Edge {
                src: "obj".into(),
                tgt: "desc".into(),
                kind: "prop".into(),
                name: Some("desc".into()),
            },
        ),
    ];

    let inst = WInstance::new(nodes, arcs, vec![], 0, Name::from("obj"));

    // Validate
    let errors = validate_wtype(&schema, &inst);
    assert!(errors.is_empty(), "validation errors: {errors:?}");

    // Serialize to JSON
    let json_out = to_json(&schema, &inst);
    assert!(json_out.is_object());
    assert_eq!(json_out["name"], "Alice");
    assert_eq!(json_out["desc"], "A person");

    // Parse back
    let parsed = parse_json(&schema, "obj", &json_out);
    assert!(parsed.is_ok(), "parse failed: {parsed:?}");
    let parsed = parsed
        .unwrap_or_else(|_| WInstance::new(HashMap::new(), vec![], vec![], 0, Name::default()));

    // Verify round-trip fidelity
    assert_eq!(parsed.node_count(), 3, "expected 3 nodes after round-trip");
    assert_eq!(parsed.arc_count(), 2, "expected 2 arcs after round-trip");

    // Re-serialize and compare
    let json_rt = to_json(&schema, &parsed);
    assert_eq!(json_rt["name"], "Alice");
    assert_eq!(json_rt["desc"], "A person");
}

// ---------------------------------------------------------------------------
// Test 2: Recursive schema (threadViewPost):
//         11-node instance -> restrict drops array+union -> 3 nodes survive
// ---------------------------------------------------------------------------
#[allow(clippy::too_many_lines)]
fn thread_view_post_schema() -> (Schema, Schema) {
    // Source schema: threadViewPost with nested structure
    // Vertices: post, body, text, createdAt, replies(array), reply_item(union),
    //           thread_view(ref), author, displayName, avatar, handle
    let verts = [
        ("post", "record"),
        ("body", "object"),
        ("text", "string"),
        ("createdAt", "string"),
        ("replies", "array"),
        ("reply_item", "union"),
        ("thread_view", "ref"),
        ("author", "object"),
        ("displayName", "string"),
        ("avatar", "string"),
        ("handle", "string"),
    ];

    let mut vertices = HashMap::new();
    for (id, kind) in &verts {
        vertices.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
                nsid: None,
            },
        );
    }

    let edge_defs = [
        ("post", "body", "record-schema", None),
        ("body", "text", "prop", Some("text")),
        ("body", "createdAt", "prop", Some("createdAt")),
        ("body", "replies", "prop", Some("replies")),
        ("body", "author", "prop", Some("author")),
        ("replies", "reply_item", "item", Some("item")),
        (
            "reply_item",
            "thread_view",
            "variant",
            Some("threadViewPost"),
        ),
        ("author", "displayName", "prop", Some("displayName")),
        ("author", "avatar", "prop", Some("avatar")),
        ("author", "handle", "prop", Some("handle")),
    ];

    let mut edges = HashMap::new();
    let mut outgoing: HashMap<Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), smallvec::SmallVec<Edge, 2>> = HashMap::new();

    for &(src, tgt, kind, name) in &edge_defs {
        let edge = Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: kind.into(),
            name: name.map(Into::into),
        };
        edges.insert(edge.clone(), kind.into());
        outgoing.entry(src.into()).or_default().push(edge.clone());
        incoming.entry(tgt.into()).or_default().push(edge.clone());
        between
            .entry((src.into(), tgt.into()))
            .or_default()
            .push(edge);
    }

    let src_schema = Schema {
        protocol: "test".into(),
        vertices: vertices.clone(),
        edges: edges.clone(),
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
        outgoing: outgoing.clone(),
        incoming: incoming.clone(),
        between: between.clone(),
    };

    // Target schema: only post, body, text survive
    let tgt_verts: HashMap<Name, Vertex> = ["post", "body", "text"]
        .iter()
        .filter_map(|id| vertices.get(*id).map(|v| (Name::from(*id), v.clone())))
        .collect();

    let tgt_edges_list: Vec<Edge> = [
        ("post", "body", "record-schema", None),
        ("body", "text", "prop", Some("text")),
    ]
    .iter()
    .map(|&(src, tgt, kind, name)| Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: kind.into(),
        name: name.map(Into::into),
    })
    .collect();

    let mut tgt_edges = HashMap::new();
    let mut tgt_outgoing: HashMap<Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
    let mut tgt_incoming: HashMap<Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
    let mut tgt_between: HashMap<(Name, Name), smallvec::SmallVec<Edge, 2>> = HashMap::new();

    for edge in &tgt_edges_list {
        tgt_edges.insert(edge.clone(), edge.kind.clone());
        tgt_outgoing
            .entry(edge.src.clone())
            .or_default()
            .push(edge.clone());
        tgt_incoming
            .entry(edge.tgt.clone())
            .or_default()
            .push(edge.clone());
        tgt_between
            .entry((edge.src.clone(), edge.tgt.clone()))
            .or_default()
            .push(edge.clone());
    }

    let tgt_schema = Schema {
        protocol: "test".into(),
        vertices: tgt_verts,
        edges: tgt_edges,
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
        outgoing: tgt_outgoing,
        incoming: tgt_incoming,
        between: tgt_between,
    };

    (src_schema, tgt_schema)
}

#[test]
#[allow(clippy::too_many_lines)]
fn test_recursive_schema_restrict_drops_to_3_nodes() {
    let (src_schema, tgt_schema) = thread_view_post_schema();

    // Build an 11-node instance
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "post"));
    nodes.insert(1, Node::new(1, "body"));
    nodes.insert(
        2,
        Node::new(2, "text").with_value(FieldPresence::Present(Value::Str("hello".into()))),
    );
    nodes.insert(
        3,
        Node::new(3, "createdAt")
            .with_value(FieldPresence::Present(Value::Str("2024-01-01".into()))),
    );
    nodes.insert(4, Node::new(4, "replies"));
    nodes.insert(
        5,
        Node::new(5, "reply_item").with_discriminator("threadViewPost"),
    );
    nodes.insert(6, Node::new(6, "thread_view"));
    nodes.insert(7, Node::new(7, "author"));
    nodes.insert(
        8,
        Node::new(8, "displayName").with_value(FieldPresence::Present(Value::Str("Bob".into()))),
    );
    nodes.insert(
        9,
        Node::new(9, "avatar")
            .with_value(FieldPresence::Present(Value::Str("https://img.jpg".into()))),
    );
    nodes.insert(
        10,
        Node::new(10, "handle")
            .with_value(FieldPresence::Present(Value::Str("bob.bsky.social".into()))),
    );

    let arcs = vec![
        (
            0,
            1,
            Edge {
                src: "post".into(),
                tgt: "body".into(),
                kind: "record-schema".into(),
                name: None,
            },
        ),
        (
            1,
            2,
            Edge {
                src: "body".into(),
                tgt: "text".into(),
                kind: "prop".into(),
                name: Some("text".into()),
            },
        ),
        (
            1,
            3,
            Edge {
                src: "body".into(),
                tgt: "createdAt".into(),
                kind: "prop".into(),
                name: Some("createdAt".into()),
            },
        ),
        (
            1,
            4,
            Edge {
                src: "body".into(),
                tgt: "replies".into(),
                kind: "prop".into(),
                name: Some("replies".into()),
            },
        ),
        (
            1,
            7,
            Edge {
                src: "body".into(),
                tgt: "author".into(),
                kind: "prop".into(),
                name: Some("author".into()),
            },
        ),
        (
            4,
            5,
            Edge {
                src: "replies".into(),
                tgt: "reply_item".into(),
                kind: "item".into(),
                name: Some("item".into()),
            },
        ),
        (
            5,
            6,
            Edge {
                src: "reply_item".into(),
                tgt: "thread_view".into(),
                kind: "variant".into(),
                name: Some("threadViewPost".into()),
            },
        ),
        (
            7,
            8,
            Edge {
                src: "author".into(),
                tgt: "displayName".into(),
                kind: "prop".into(),
                name: Some("displayName".into()),
            },
        ),
        (
            7,
            9,
            Edge {
                src: "author".into(),
                tgt: "avatar".into(),
                kind: "prop".into(),
                name: Some("avatar".into()),
            },
        ),
        (
            7,
            10,
            Edge {
                src: "author".into(),
                tgt: "handle".into(),
                kind: "prop".into(),
                name: Some("handle".into()),
            },
        ),
    ];

    let inst = WInstance::new(nodes, arcs, vec![], 0, Name::from("post"));
    assert_eq!(inst.node_count(), 11, "should start with 11 nodes");

    // Migration: only post, body, text survive
    let surviving_verts: HashSet<Name> = ["post", "body", "text"]
        .iter()
        .map(|s| Name::from(*s))
        .collect();

    // Step 1: anchor surviving
    let candidates = anchor_surviving(&inst, &surviving_verts);
    assert_eq!(
        candidates.len(),
        3,
        "3 nodes should have surviving anchors: {candidates:?}"
    );

    // Step 2: reachability
    let reachable = reachable_from_root(&inst, &candidates);
    assert_eq!(
        reachable.len(),
        3,
        "3 nodes should be reachable from root: {reachable:?}"
    );
    assert!(reachable.contains(&0), "root should be reachable");
    assert!(reachable.contains(&1), "body should be reachable");
    assert!(reachable.contains(&2), "text should be reachable");

    // Full restrict
    let migration = CompiledMigration {
        surviving_verts,
        surviving_edges: HashSet::new(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    };

    let result = wtype_restrict(&inst, &src_schema, &tgt_schema, &migration);
    assert!(result.is_ok(), "restrict failed: {result:?}");
    let restricted = result
        .unwrap_or_else(|_| WInstance::new(HashMap::new(), vec![], vec![], 0, Name::default()));
    assert_eq!(
        restricted.node_count(),
        3,
        "restricted instance should have 3 nodes"
    );
}

// ---------------------------------------------------------------------------
// Test 3: Fan instance: 4-ary hyperedge, drop 1 child -> valid 3-ary fan
// ---------------------------------------------------------------------------
#[test]
fn test_fan_reconstruction_4_to_3() {
    // Schema with a 4-ary hyperedge (like a SQL FK)
    let mut vertices = HashMap::new();
    for id in ["table", "col1", "col2", "col3", "col4"] {
        vertices.insert(
            Name::from(id),
            Vertex {
                id: Name::from(id),
                kind: "node".into(),
                nsid: None,
            },
        );
    }

    let he = HyperEdge {
        id: "fk1".into(),
        kind: "foreign_key".into(),
        signature: HashMap::from([
            ("table".into(), "table".into()),
            ("col1".into(), "col1".into()),
            ("col2".into(), "col2".into()),
            ("col3".into(), "col3".into()),
        ]),
        parent_label: "table".into(),
    };

    let mut hyper_edges = HashMap::new();
    hyper_edges.insert("fk1".into(), he);

    let schema = Schema {
        protocol: "test".into(),
        vertices,
        edges: HashMap::new(),
        hyper_edges,
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        outgoing: HashMap::new(),
        incoming: HashMap::new(),
        between: HashMap::new(),
    };

    // Build instance with fan
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "table"));
    nodes.insert(
        1,
        Node::new(1, "col1").with_value(FieldPresence::Present(Value::Str("a".into()))),
    );
    nodes.insert(
        2,
        Node::new(2, "col2").with_value(FieldPresence::Present(Value::Str("b".into()))),
    );
    nodes.insert(
        3,
        Node::new(3, "col3").with_value(FieldPresence::Present(Value::Str("c".into()))),
    );
    nodes.insert(
        4,
        Node::new(4, "col4").with_value(FieldPresence::Present(Value::Str("d".into()))),
    );

    let fan = Fan::new("fk1", 0)
        .with_child("col1", 1)
        .with_child("col2", 2)
        .with_child("col3", 3)
        .with_child("col4", 4);

    let inst = WInstance::new(nodes, vec![], vec![fan], 0, "table".into());

    // Surviving: table, col1, col2, col3 (drop col4)
    let surviving: rustc_hash::FxHashSet<u32> = [0, 1, 2, 3].into_iter().collect();
    let ancestors = rustc_hash::FxHashMap::default();

    let migration = CompiledMigration {
        surviving_verts: HashSet::from([
            "table".into(),
            "col1".into(),
            "col2".into(),
            "col3".into(),
        ]),
        surviving_edges: HashSet::new(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    };

    let result = reconstruct_fans(&inst, &surviving, &ancestors, &migration, &schema);
    assert!(result.is_ok(), "fan reconstruction failed: {result:?}");
    let fans = result.unwrap_or_default();
    assert_eq!(fans.len(), 1, "should produce exactly 1 fan");
    assert_eq!(fans[0].arity(), 3, "reconstructed fan should be 3-ary");
    assert!(
        fans[0].child("col1").is_some(),
        "col1 should survive in fan"
    );
    assert!(
        fans[0].child("col2").is_some(),
        "col2 should survive in fan"
    );
    assert!(
        fans[0].child("col3").is_some(),
        "col3 should survive in fan"
    );
    assert!(fans[0].child("col4").is_none(), "col4 should not be in fan");
}

// ---------------------------------------------------------------------------
// Test 4: Set-valued functor: two tables with FK -> restrict drops one table
// ---------------------------------------------------------------------------
#[test]
fn test_functor_restrict_precomposition() {
    let mut users_rows = vec![];
    let mut row1 = HashMap::new();
    row1.insert("id".to_string(), Value::Int(1));
    row1.insert("name".to_string(), Value::Str("Alice".into()));
    users_rows.push(row1);

    let mut row2 = HashMap::new();
    row2.insert("id".to_string(), Value::Int(2));
    row2.insert("name".to_string(), Value::Str("Bob".into()));
    users_rows.push(row2);

    let mut posts_rows = vec![];
    let mut post1 = HashMap::new();
    post1.insert("id".to_string(), Value::Int(100));
    post1.insert("title".to_string(), Value::Str("Hello World".into()));
    post1.insert("author_id".to_string(), Value::Int(1));
    posts_rows.push(post1);

    let fk_edge = Edge {
        src: "posts".into(),
        tgt: "users".into(),
        kind: "fk".into(),
        name: Some("author".into()),
    };

    let inst = FInstance::new()
        .with_table("users", users_rows)
        .with_table("posts", posts_rows)
        .with_foreign_key(fk_edge, vec![(0, 0)]);

    assert_eq!(inst.table_count(), 2);
    assert_eq!(inst.row_count("users"), 2);
    assert_eq!(inst.row_count("posts"), 1);

    // Migration: drop "posts", keep "users" only
    let migration = CompiledMigration {
        surviving_verts: HashSet::from([Name::from("users")]),
        surviving_edges: HashSet::new(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    };

    let result = functor_restrict(&inst, &migration);
    assert!(result.is_ok(), "functor_restrict failed: {result:?}");
    let restricted = result.unwrap_or_else(|_| FInstance::new());

    // Verify: only "users" table survives
    assert_eq!(restricted.table_count(), 1, "should have 1 table");
    assert!(
        restricted.tables.contains_key("users"),
        "users table should survive"
    );
    assert!(
        !restricted.tables.contains_key("posts"),
        "posts table should be dropped"
    );

    // Verify: foreign keys dropped (since posts is gone)
    assert!(
        restricted.foreign_keys.is_empty(),
        "foreign keys should be empty after dropping posts"
    );

    // Verify: users data preserved
    assert_eq!(restricted.row_count("users"), 2, "users should have 2 rows");
}
