//! A pushforward yields its targets in a fixed order, and a graph's
//! pushforward keeps one target per path.
//!
//! A table instance files its foreign keys in a `HashMap`, whose enumeration
//! order each map's hash seed decides. A pushforward that walks the keys in
//! that order lists the same targets in a different order from one instance to
//! the next, so a query that applies `limit` after a path step keeps a
//! different subset. A graph instance's pushforward counts paths: a node
//! reached twice is listed twice, and each copy is followed on the next step.

use std::collections::HashMap;

use panproto_inst::{ElementOps, FInstance, GInstance, Node, Value, encode_finstance_id};
use panproto_schema::Edge;

/// How many independently built instances each order is compared across.
const INSTANCES: usize = 64;

fn author(src: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: "users".into(),
        kind: "author".into(),
        name: None,
    }
}

/// Three tables whose single rows each point at a different user.
fn tables() -> FInstance {
    let row = |v: i64| HashMap::from([("k".to_owned(), Value::Int(v))]);
    FInstance::new()
        .with_table("comments", vec![row(1)])
        .with_table("likes", vec![row(2)])
        .with_table("posts", vec![row(3)])
        .with_table("users", vec![row(10), row(11), row(12)])
        .with_foreign_key(author("posts"), vec![(0, 0)])
        .with_foreign_key(author("comments"), vec![(0, 2)])
        .with_foreign_key(author("likes"), vec![(0, 1)])
}

#[test]
fn a_table_pushforward_lists_targets_in_foreign_key_order() {
    // Tables in ascending name order: comments 0, likes 1, posts 2, users 3.
    let sources = [
        encode_finstance_id(0, 0),
        encode_finstance_id(1, 0),
        encode_finstance_id(2, 0),
    ];
    // Keys in ascending edge order (comments, likes, posts), each key's pairs
    // in stored order.
    let expected = vec![
        encode_finstance_id(3, 2),
        encode_finstance_id(3, 1),
        encode_finstance_id(3, 0),
    ];
    for attempt in 0..INSTANCES {
        let targets = tables().pushforward(&sources, &"author".into());
        assert_eq!(targets, expected, "instance {attempt}");
    }
}

fn link() -> Edge {
    Edge {
        src: "n".into(),
        tgt: "n".into(),
        kind: "link".into(),
        name: None,
    }
}

#[test]
fn a_graph_pushforward_keeps_one_target_per_path() {
    // Two parallel edges 0 -> 1, then one edge 1 -> 2.
    let graph = (0..3)
        .fold(GInstance::new(), |g, id| g.with_node(Node::new(id, "n")))
        .with_edge(0, 1, link())
        .with_edge(0, 1, link())
        .with_edge(1, 2, link());

    let one = graph.pushforward(&[0], &"link".into());
    assert_eq!(one, vec![1, 1], "one target per parallel edge");

    let two = graph.pushforward(&one, &"link".into());
    assert_eq!(two, vec![2, 2], "each path to node 1 continues to node 2");

    assert_eq!(
        graph.pushforward(&[1, 0, 1], &"link".into()),
        vec![1, 1, 2, 2],
        "ascending edge id, each edge once per occurrence of its source"
    );
}
