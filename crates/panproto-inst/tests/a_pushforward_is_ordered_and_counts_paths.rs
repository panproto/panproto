//! A pushforward yields its targets in a fixed order, and a graph's
//! pushforward keeps one target per path.
//!
//! A table instance files its foreign keys in a `HashMap`, whose enumeration
//! order each map's hash seed decides. A pushforward that walks the keys in
//! that order lists the same targets in a different order from one instance to
//! the next, so a query that applies `limit` after a path step keeps a
//! different subset. A graph instance's pushforward counts paths: a node
//! reached twice is listed twice, and each copy is followed on the next step.
//! A query's path navigation does not: it keeps each node once per step, so
//! its work is bounded by the graph's size rather than by its number of walks.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::{
    ElementOps, FInstance, GInstance, InstanceQuery, Node, RowId, Value, execute_graph,
};
use panproto_schema::{Edge, Protocol, SchemaBuilder};

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

/// Three tables whose single rows each point at a different user: comments
/// is row 0, likes row 1, posts row 2, and the users rows 3, 4 and 5.
fn tables() -> FInstance {
    let row = |v: i64| HashMap::from([("k".to_owned(), Value::Int(v))]);
    FInstance::new()
        .with_table("comments", vec![row(1)])
        .and_then(|f| f.with_table("likes", vec![row(2)]))
        .and_then(|f| f.with_table("posts", vec![row(3)]))
        .and_then(|f| f.with_table("users", vec![row(10), row(11), row(12)]))
        .and_then(|f| f.with_foreign_key(author("posts"), [(RowId(2), RowId(3))]))
        .and_then(|f| f.with_foreign_key(author("comments"), [(RowId(0), RowId(5))]))
        .and_then(|f| f.with_foreign_key(author("likes"), [(RowId(1), RowId(4))]))
        .expect("the fixture is well formed")
}

#[test]
fn a_table_pushforward_lists_targets_in_foreign_key_order() {
    // Element ids are row ids.
    let sources = [0, 1, 2];
    // Keys in ascending edge order (comments, likes, posts), each key's pairs
    // in ascending order.
    let expected = vec![5, 4, 3];
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

#[test]
fn composed_graph_pushforwards_list_one_entry_per_walk() -> Result<(), Box<dyn std::error::Error>> {
    // A chain of diamonds: node 3i splits to 3i+1 and 3i+2, which both join
    // at 3i+3. Every diamond doubles the number of walks to the next join.
    const DIAMONDS: u32 = 12;
    let graph = diamond_chain(DIAMONDS);

    let path: Vec<Name> = (0..2 * DIAMONDS).map(|_| Name::from("link")).collect();
    let walks = path
        .iter()
        .fold(vec![0], |current, kind| graph.pushforward(&current, kind));
    assert_eq!(walks.len(), 1 << DIAMONDS, "one entry per walk");
    assert!(walks.iter().all(|&node| node == 3 * DIAMONDS));

    // A query over the same path matches the far end once, however many
    // walks reach it.
    let query = InstanceQuery {
        anchor: Name::from("n"),
        path,
        limit: Some(3),
        ..Default::default()
    };
    // Every node is a start; only the root reaches the far end in
    // `2 * DIAMONDS` steps.
    assert_eq!(query_answers(&query, &graph)?, vec![3 * DIAMONDS]);
    Ok(())
}

/// A chain of `diamonds` diamonds over one vertex `n`: node `3i` splits to
/// `3i + 1` and `3i + 2`, which both join at `3i + 3`.
fn diamond_chain(diamonds: u32) -> GInstance {
    let mut graph =
        (0..=3 * diamonds).fold(GInstance::new(), |g, id| g.with_node(Node::new(id, "n")));
    for i in 0..diamonds {
        let top = 3 * i;
        graph = graph
            .with_edge(top, top + 1, link())
            .with_edge(top, top + 2, link())
            .with_edge(top + 1, top + 3, link())
            .with_edge(top + 2, top + 3, link());
    }
    graph
}

/// The node ids a query over the one-vertex `n` schema matches, in order.
fn query_answers(
    query: &InstanceQuery,
    graph: &GInstance,
) -> Result<Vec<u32>, Box<dyn std::error::Error>> {
    let schema = SchemaBuilder::new(&Protocol::default())
        .vertex("n", "record", None)?
        .edge("n", "n", "link", None)?
        .build()?;
    Ok(execute_graph(query, graph, &schema)
        .into_iter()
        .map(|m| m.node_id)
        .collect())
}

#[test]
fn a_graph_query_matches_each_node_once_however_many_walks_reach_it()
-> Result<(), Box<dyn std::error::Error>> {
    // `2^16` walks reach the far end; the query lists it once.
    const DIAMONDS: u32 = 16;
    let query = InstanceQuery {
        anchor: Name::from("n"),
        path: (0..2 * DIAMONDS).map(|_| Name::from("link")).collect(),
        ..Default::default()
    };
    assert_eq!(
        query_answers(&query, &diamond_chain(DIAMONDS))?,
        vec![3 * DIAMONDS]
    );

    // Each step keeps the first occurrence of each node, in pushforward
    // order: from nodes 0, 1, 2 and 3, one step takes edges 0 to 3, reaching
    // 1, 2, 3 and 3 again.
    let one_step = InstanceQuery {
        anchor: Name::from("n"),
        path: vec![Name::from("link")],
        ..Default::default()
    };
    assert_eq!(query_answers(&one_step, &diamond_chain(1))?, vec![1, 2, 3]);
    Ok(())
}

#[test]
fn a_graph_query_over_a_long_path_takes_time_in_the_graph_size()
-> Result<(), Box<dyn std::error::Error>> {
    // `2^200` walks reach the far end, far more than could be listed; each
    // step holds at most one entry per node.
    const DIAMONDS: u32 = 200;
    let query = InstanceQuery {
        anchor: Name::from("n"),
        path: (0..2 * DIAMONDS).map(|_| Name::from("link")).collect(),
        limit: Some(1),
        ..Default::default()
    };
    assert_eq!(
        query_answers(&query, &diamond_chain(DIAMONDS))?,
        vec![3 * DIAMONDS]
    );

    // Two nodes linked both ways, one direction twice: a cycle whose walks
    // double every two steps.
    let cycle = GInstance::new()
        .with_node(Node::new(0, "n"))
        .with_node(Node::new(1, "n"))
        .with_edge(0, 1, link())
        .with_edge(0, 1, link())
        .with_edge(1, 0, link());
    let around = InstanceQuery {
        anchor: Name::from("n"),
        path: (0..400).map(|_| Name::from("link")).collect(),
        ..Default::default()
    };
    assert_eq!(query_answers(&around, &cycle)?, vec![1, 0]);
    Ok(())
}
