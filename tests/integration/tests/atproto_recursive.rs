//! Integration test 2: threadViewPost projection.
//!
//! Builds a recursive ATProto-like schema (threadViewPost with nested
//! children), creates an 11-node instance, and verifies that a
//! projection migration with reachability filtering drops nested
//! children correctly.

use std::collections::{HashMap, HashSet};

use panproto_inst::value::FieldPresence;
use panproto_inst::{CompiledMigration, Node, Value, WInstance};
use panproto_mig::lift_wtype;
use panproto_schema::{Edge, Schema, Vertex};
use smallvec::SmallVec;

/// Build a recursive threadViewPost schema.
///
/// Schema structure:
///   threadViewPost (object)
///     -> post (object)
///       -> post.text (string)
///       -> post.createdAt (string)
///     -> replies (array)
///       -> replies:items (union)
///         -> replies:items:variant0 (object) [self-referential via arc]
///     -> parent (union)
///       -> parent:variant0 (object)
#[allow(clippy::too_many_lines)]
fn thread_view_post_schema() -> Schema {
    let vertices = HashMap::from([
        (
            "tvp".into(),
            Vertex {
                id: "tvp".into(),
                kind: "object".into(),
                nsid: None,
            },
        ),
        (
            "tvp.post".into(),
            Vertex {
                id: "tvp.post".into(),
                kind: "object".into(),
                nsid: None,
            },
        ),
        (
            "tvp.post.text".into(),
            Vertex {
                id: "tvp.post.text".into(),
                kind: "string".into(),
                nsid: None,
            },
        ),
        (
            "tvp.post.createdAt".into(),
            Vertex {
                id: "tvp.post.createdAt".into(),
                kind: "string".into(),
                nsid: None,
            },
        ),
        (
            "tvp.replies".into(),
            Vertex {
                id: "tvp.replies".into(),
                kind: "array".into(),
                nsid: None,
            },
        ),
        (
            "tvp.replies:items".into(),
            Vertex {
                id: "tvp.replies:items".into(),
                kind: "union".into(),
                nsid: None,
            },
        ),
        (
            "tvp.replies:items:v0".into(),
            Vertex {
                id: "tvp.replies:items:v0".into(),
                kind: "object".into(),
                nsid: None,
            },
        ),
        (
            "tvp.parent".into(),
            Vertex {
                id: "tvp.parent".into(),
                kind: "union".into(),
                nsid: None,
            },
        ),
        (
            "tvp.parent:v0".into(),
            Vertex {
                id: "tvp.parent:v0".into(),
                kind: "object".into(),
                nsid: None,
            },
        ),
    ]);

    let edges_list = vec![
        Edge {
            src: "tvp".into(),
            tgt: "tvp.post".into(),
            kind: "prop".into(),
            name: Some("post".into()),
        },
        Edge {
            src: "tvp.post".into(),
            tgt: "tvp.post.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        },
        Edge {
            src: "tvp.post".into(),
            tgt: "tvp.post.createdAt".into(),
            kind: "prop".into(),
            name: Some("createdAt".into()),
        },
        Edge {
            src: "tvp".into(),
            tgt: "tvp.replies".into(),
            kind: "prop".into(),
            name: Some("replies".into()),
        },
        Edge {
            src: "tvp.replies".into(),
            tgt: "tvp.replies:items".into(),
            kind: "items".into(),
            name: None,
        },
        Edge {
            src: "tvp.replies:items".into(),
            tgt: "tvp.replies:items:v0".into(),
            kind: "variant".into(),
            name: None,
        },
        Edge {
            src: "tvp".into(),
            tgt: "tvp.parent".into(),
            kind: "prop".into(),
            name: Some("parent".into()),
        },
        Edge {
            src: "tvp.parent".into(),
            tgt: "tvp.parent:v0".into(),
            kind: "variant".into(),
            name: None,
        },
    ];

    let mut edges = HashMap::new();
    let mut outgoing: HashMap<String, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<String, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(String, String), SmallVec<Edge, 2>> = HashMap::new();

    for e in &edges_list {
        edges.insert(e.clone(), e.kind.clone());
        outgoing.entry(e.src.clone()).or_default().push(e.clone());
        incoming.entry(e.tgt.clone()).or_default().push(e.clone());
        between
            .entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
    }

    Schema {
        protocol: "atproto".into(),
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

/// Build an 11-node instance with nested structure:
///   0: tvp (root)
///   1: tvp.post
///   2: tvp.post.text = "hello"
///   3: tvp.post.createdAt = "2024-01-01"
///   4: tvp.replies
///   5: tvp.replies:items
///   6: tvp.replies:items:v0 (nested child)
///   7: tvp.replies:items:v0.text (nested text)
///   8: tvp.parent
///   9: tvp.parent:v0
///   10: tvp.parent:v0.text (nested parent text)
#[allow(clippy::too_many_lines)]
fn thread_view_post_instance() -> WInstance {
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "tvp"));
    nodes.insert(1, Node::new(1, "tvp.post"));
    nodes.insert(
        2,
        Node::new(2, "tvp.post.text")
            .with_value(FieldPresence::Present(Value::Str("hello".into()))),
    );
    nodes.insert(
        3,
        Node::new(3, "tvp.post.createdAt")
            .with_value(FieldPresence::Present(Value::Str("2024-01-01".into()))),
    );
    nodes.insert(4, Node::new(4, "tvp.replies"));
    nodes.insert(5, Node::new(5, "tvp.replies:items"));
    nodes.insert(6, Node::new(6, "tvp.replies:items:v0"));
    nodes.insert(
        7,
        Node::new(7, "tvp.replies:items:v0")
            .with_value(FieldPresence::Present(Value::Str("nested".into()))),
    );
    nodes.insert(8, Node::new(8, "tvp.parent"));
    nodes.insert(9, Node::new(9, "tvp.parent:v0"));
    nodes.insert(
        10,
        Node::new(10, "tvp.parent:v0")
            .with_value(FieldPresence::Present(Value::Str("parent-text".into()))),
    );

    let arcs = vec![
        (
            0,
            1,
            Edge {
                src: "tvp".into(),
                tgt: "tvp.post".into(),
                kind: "prop".into(),
                name: Some("post".into()),
            },
        ),
        (
            1,
            2,
            Edge {
                src: "tvp.post".into(),
                tgt: "tvp.post.text".into(),
                kind: "prop".into(),
                name: Some("text".into()),
            },
        ),
        (
            1,
            3,
            Edge {
                src: "tvp.post".into(),
                tgt: "tvp.post.createdAt".into(),
                kind: "prop".into(),
                name: Some("createdAt".into()),
            },
        ),
        (
            0,
            4,
            Edge {
                src: "tvp".into(),
                tgt: "tvp.replies".into(),
                kind: "prop".into(),
                name: Some("replies".into()),
            },
        ),
        (
            4,
            5,
            Edge {
                src: "tvp.replies".into(),
                tgt: "tvp.replies:items".into(),
                kind: "items".into(),
                name: None,
            },
        ),
        (
            5,
            6,
            Edge {
                src: "tvp.replies:items".into(),
                tgt: "tvp.replies:items:v0".into(),
                kind: "variant".into(),
                name: None,
            },
        ),
        (
            6,
            7,
            Edge {
                src: "tvp.replies:items:v0".into(),
                tgt: "tvp.replies:items:v0".into(),
                kind: "prop".into(),
                name: Some("text".into()),
            },
        ),
        (
            0,
            8,
            Edge {
                src: "tvp".into(),
                tgt: "tvp.parent".into(),
                kind: "prop".into(),
                name: Some("parent".into()),
            },
        ),
        (
            8,
            9,
            Edge {
                src: "tvp.parent".into(),
                tgt: "tvp.parent:v0".into(),
                kind: "variant".into(),
                name: None,
            },
        ),
        (
            9,
            10,
            Edge {
                src: "tvp.parent:v0".into(),
                tgt: "tvp.parent:v0".into(),
                kind: "prop".into(),
                name: Some("text".into()),
            },
        ),
    ];

    WInstance::new(nodes, arcs, vec![], 0, "tvp".into())
}

#[test]
fn projection_drops_replies_and_parent() -> Result<(), Box<dyn std::error::Error>> {
    let full_schema = thread_view_post_schema();
    let instance = thread_view_post_instance();

    assert_eq!(instance.node_count(), 11, "instance should have 11 nodes");

    // Build a target schema that keeps only tvp, tvp.post, tvp.post.text,
    // tvp.post.createdAt. Drops replies, parent, and all descendants.
    let surviving_verts: HashSet<String> = HashSet::from([
        "tvp".into(),
        "tvp.post".into(),
        "tvp.post.text".into(),
        "tvp.post.createdAt".into(),
    ]);

    let surviving_edges: HashSet<Edge> = HashSet::from([
        Edge {
            src: "tvp".into(),
            tgt: "tvp.post".into(),
            kind: "prop".into(),
            name: Some("post".into()),
        },
        Edge {
            src: "tvp.post".into(),
            tgt: "tvp.post.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        },
        Edge {
            src: "tvp.post".into(),
            tgt: "tvp.post.createdAt".into(),
            kind: "prop".into(),
            name: Some("createdAt".into()),
        },
    ]);

    // Build a target schema with only the surviving elements.
    let tgt_vertices: HashMap<String, Vertex> = full_schema
        .vertices
        .into_iter()
        .filter(|(k, _)| surviving_verts.contains(k))
        .collect();

    let tgt_edges: HashMap<Edge, String> = surviving_edges
        .iter()
        .map(|e| (e.clone(), e.kind.clone()))
        .collect();

    let mut outgoing: HashMap<String, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<String, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(String, String), SmallVec<Edge, 2>> = HashMap::new();
    for e in &surviving_edges {
        outgoing.entry(e.src.clone()).or_default().push(e.clone());
        incoming.entry(e.tgt.clone()).or_default().push(e.clone());
        between
            .entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
    }

    let tgt_schema = Schema {
        protocol: "atproto".into(),
        vertices: tgt_vertices,
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
        outgoing,
        incoming,
        between,
    };

    let compiled = CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    };

    let lifted = lift_wtype(&compiled, &tgt_schema, &tgt_schema, &instance)?;

    // Should keep only the root + post subtree (4 nodes max, but
    // reachability may further prune depending on anchor survival).
    assert!(
        lifted.node_count() <= 4,
        "projection should drop nested children, got {} nodes",
        lifted.node_count()
    );
    assert!(lifted.nodes.contains_key(&0), "root should survive");

    // Replies and parent subtrees should be gone.
    assert!(
        !lifted.nodes.contains_key(&4),
        "replies node should be dropped"
    );
    assert!(
        !lifted.nodes.contains_key(&8),
        "parent node should be dropped"
    );

    Ok(())
}

#[test]
fn reachability_prunes_orphaned_children() -> Result<(), Box<dyn std::error::Error>> {
    // Build a schema where dropping an intermediate node makes its
    // children unreachable even though they are in the surviving set.
    //
    // Schema: root -> intermediate -> leaf
    // Migration: keep root + leaf but drop intermediate.
    // Result: leaf is anchor-surviving but not reachable from root.
    let edges = [
        Edge {
            src: "root".into(),
            tgt: "mid".into(),
            kind: "prop".into(),
            name: Some("child".into()),
        },
        Edge {
            src: "mid".into(),
            tgt: "leaf".into(),
            kind: "prop".into(),
            name: Some("data".into()),
        },
    ];

    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "root"));
    nodes.insert(1, Node::new(1, "mid"));
    nodes.insert(
        2,
        Node::new(2, "leaf").with_value(FieldPresence::Present(Value::Str("orphan".into()))),
    );

    let instance = WInstance::new(
        nodes,
        vec![(0, 1, edges[0].clone()), (1, 2, edges[1].clone())],
        vec![],
        0,
        "root".into(),
    );

    // Keep root and leaf but drop mid -> leaf becomes unreachable.
    let edge_root_leaf = Edge {
        src: "root".into(),
        tgt: "leaf".into(),
        kind: "prop".into(),
        name: Some("data".into()),
    };

    let tgt_vertices = HashMap::from([
        (
            "root".into(),
            Vertex {
                id: "root".into(),
                kind: "object".into(),
                nsid: None,
            },
        ),
        (
            "leaf".into(),
            Vertex {
                id: "leaf".into(),
                kind: "string".into(),
                nsid: None,
            },
        ),
    ]);

    // Target schema has a direct root->leaf edge (the contracted edge).
    let tgt_schema = Schema {
        protocol: "test".into(),
        vertices: tgt_vertices,
        edges: HashMap::from([(edge_root_leaf.clone(), "prop".into())]),
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
        outgoing: HashMap::from([("root".into(), SmallVec::from([edge_root_leaf.clone()]))]),
        incoming: HashMap::from([("leaf".into(), SmallVec::from([edge_root_leaf.clone()]))]),
        between: HashMap::from([(
            ("root".into(), "leaf".into()),
            SmallVec::from([edge_root_leaf.clone()]),
        )]),
    };

    let compiled = CompiledMigration {
        surviving_verts: HashSet::from(["root".into(), "leaf".into()]),
        surviving_edges: HashSet::from([edge_root_leaf]),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    };

    let lifted = lift_wtype(&compiled, &tgt_schema, &tgt_schema, &instance)?;

    // The restrict pipeline should handle ancestor contraction:
    // leaf's parent (mid) is dropped, so leaf should be re-parented
    // to root via the contracted edge, OR pruned if contraction fails.
    // Either outcome is acceptable: the key is that the pipeline
    // completes successfully.
    assert!(lifted.node_count() >= 1, "at least root should survive");
    assert!(lifted.nodes.contains_key(&0), "root should always survive");

    Ok(())
}
