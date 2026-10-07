#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The shape encodings are inverse on their images.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::encode::{self, GraphSide};
use panproto_reference::migration::Migration;
use panproto_reference::model::{Fan, Node, SEdge, Table, Tree};
use panproto_reference::rel;
use panproto_reference::schema::{HyperEdge, Schema};
use panproto_reference::value::Value;

#[test]
fn a_tree_with_a_fan_round_trips_through_its_graph() {
    let mut nodes = BTreeMap::new();
    for (id, a) in [(0, "r"), (2, "a"), (5, "b")] {
        nodes.insert(id, Node::new(id, a));
    }
    let tree = Tree {
        nodes,
        arcs: vec![
            (0, 2, SEdge::new("r", "a", "prop", Some("a"))),
            (0, 5, SEdge::new("r", "b", "prop", Some("b"))),
        ],
        fans: vec![Fan {
            parent: 0,
            hyper_edge: "h".into(),
            children: [("x".to_owned(), 2), ("y".to_owned(), 5)].into(),
        }],
        root: 0,
    };
    let schema = Schema {
        hyper_edges: [(
            "h".to_owned(),
            HyperEdge {
                kind: "k".into(),
                signature: BTreeMap::new(),
                parent_label: "p".into(),
            },
        )]
        .into(),
        ..Schema::default()
    };
    let (g, emb) = encode::tree_to_graph(&tree, Some(&schema)).unwrap();
    assert_eq!(
        emb.fans,
        vec![(6, 2, [("x".to_owned(), 3), ("y".to_owned(), 4)].into())]
    );
    assert_eq!(g.edges[&2].label.name.as_deref(), Some("p"));
    assert_eq!(encode::graph_to_tree(&g, 0), Some(tree));
}

#[test]
fn a_dangling_edge_endpoint_is_a_typed_refusal_of_the_encoding() {
    // Edge 0 runs from node 0 to node 5, which the graph does not hold.
    let schema = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .edge("a", "b", "fk", Some("f"));
    let side = GraphSide::whole(
        &rel::classify(
            &schema,
            &BTreeSet::from(["int".to_owned()]),
            &BTreeSet::from(["record".to_owned()]),
        )
        .unwrap(),
    );
    let mut g = panproto_reference::model::Graph::default();
    g.nodes.insert(0, Node::new(0, "a"));
    g.edges.insert(
        0,
        panproto_reference::model::GEdge {
            src: 0,
            tgt: 5,
            label: SEdge::new("a", "b", "fk", Some("f")),
            position: None,
        },
    );
    let refused = encode::encode_graph(&g, &side);
    assert_eq!(format!("{refused:?}"), "Err(DanglingEdge { edge: 0 })");
}

#[test]
fn a_table_round_trips_through_its_graph_and_its_encoding() {
    let mut table = Table::default();
    table
        .rows
        .insert(3, ("a".into(), [("x".to_owned(), Value::Int(1))].into()));
    table.rows.insert(7, ("b".into(), BTreeMap::new()));
    table
        .pairs
        .insert((SEdge::new("a", "b", "fk", Some("f")), 3, 7));
    let (graph, emb) = encode::table_to_graph(&table);
    assert_eq!(emb.len(), 1);
    assert_eq!(encode::graph_to_table(&graph), Some(table.clone()));
    let schema = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("a.x", "int")
        .edge("a", "a.x", "prop", Some("x"))
        .edge("a", "b", "fk", Some("f"));
    let side_schema = rel::classify(
        &schema,
        &BTreeSet::from(["int".to_owned()]),
        &BTreeSet::from(["record".to_owned()]),
    )
    .unwrap();
    let side = GraphSide::whole(&side_schema);
    let decoded = encode::decode_table(&table, &side);
    assert_eq!(encode::encode_graph(&decoded, &side), Ok(table.clone()));
    let id = Migration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges,
        ..Migration::default()
    };
    let functor = rel::entity_functor(&side_schema, &side_schema, &id).unwrap();
    assert_eq!(encode::g_sigma(&functor, &decoded), Ok(decoded.clone()));
    assert_eq!(encode::g_delta(&functor, &decoded), Ok(decoded));
}
