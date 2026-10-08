#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The shape encodings are inverse on their images.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::encode::{self, GraphSide};
use panproto_reference::encode::{DecodeTableError, TreeEncodeError};
use panproto_reference::migration::Migration;
use panproto_reference::model::{Fan, Node, SEdge, Table, Tree};
use panproto_reference::rel::{self, Conformance, NonConforming};
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
    assert_eq!(
        encode::graph_to_tree(&g, 0, &encode::schema_parent_name(Some(&schema))),
        Ok(tree)
    );
}

#[test]
fn tree_fan_nodes_use_max_minus_one_but_never_the_reserved_maximum() {
    let fan = Fan {
        parent: 0,
        hyper_edge: "h".to_owned(),
        children: BTreeMap::new(),
    };
    let near = Tree {
        nodes: BTreeMap::from([
            (0, Node::new(0, "r")),
            (u32::MAX - 2, Node::new(u32::MAX - 2, "leaf")),
        ]),
        arcs: Vec::new(),
        fans: vec![fan.clone()],
        root: 0,
    };
    let (graph, embedding) = encode::tree_to_graph(&near, None).unwrap();
    assert!(graph.nodes.contains_key(&(u32::MAX - 1)));
    assert!(!graph.nodes.contains_key(&u32::MAX));
    assert_eq!(embedding.next_node, u32::MAX);

    let at_max = Tree {
        nodes: BTreeMap::from([(u32::MAX, Node::new(u32::MAX, "r"))]),
        arcs: Vec::new(),
        fans: vec![Fan {
            parent: u32::MAX,
            ..fan
        }],
        root: u32::MAX,
    };
    assert_eq!(
        encode::tree_to_graph(&at_max, None),
        Err(TreeEncodeError::NodeIdsExhausted)
    );
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
    assert_eq!(encode::graph_to_table(&g), None);
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
    let (graph, emb) = encode::table_to_graph(&table).unwrap();
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
    let decoded = encode::decode_table(&table, &side).unwrap();
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

#[test]
fn table_decode_uses_max_minus_one_but_never_the_reserved_maximum() {
    let arrow = SEdge::new("a", "value", "prop", Some("x"));
    let side = GraphSide {
        tables: BTreeSet::from(["a".to_owned()]),
        columns: BTreeMap::from([("a".to_owned(), BTreeMap::from([("x".to_owned(), arrow)]))]),
        ..GraphSide::default()
    };
    let table = |row| Table {
        rows: BTreeMap::from([(
            row,
            (
                "a".to_owned(),
                BTreeMap::from([("x".to_owned(), Value::Int(1))]),
            ),
        )]),
        pairs: BTreeSet::new(),
    };
    let near = encode::decode_table(&table(u32::MAX - 2), &side).unwrap();
    assert!(near.nodes.contains_key(&(u32::MAX - 1)));
    for reserved in [u32::MAX - 1, u32::MAX] {
        assert_eq!(
            encode::decode_table(&table(reserved), &side),
            Err(DecodeTableError::NodeIdsExhausted)
        );
    }
}

#[test]
fn table_graph_bridges_refuse_a_dangling_pair() {
    let dangling = Table {
        rows: BTreeMap::from([(0, ("a".to_owned(), BTreeMap::new()))]),
        pairs: BTreeSet::from([(SEdge::new("a", "b", "fk", Some("f")), 0, 1)]),
    };
    let pair = dangling.pairs.iter().next().unwrap().clone();
    assert_eq!(
        encode::table_to_graph(&dangling),
        Err(encode::TableEncodeError::MalformedTable {
            pair: Box::new(pair),
        })
    );
    assert_eq!(
        encode::decode_table(&dangling, &GraphSide::default()),
        Err(DecodeTableError::MalformedTable)
    );
}

fn nonconforming(row: u32, column: Option<&str>, reason: Conformance) -> DecodeTableError {
    DecodeTableError::NonConforming(Box::new(NonConforming {
        row,
        column: column.map(str::to_owned),
        reason,
    }))
}

#[test]
fn table_decode_refuses_every_relational_side_violation() {
    let column = SEdge::new("a", "value", "prop", Some("x"));
    let relation = SEdge::new("a", "b", "fk", Some("f"));
    let side = GraphSide {
        tables: BTreeSet::from(["a".to_owned(), "b".to_owned()]),
        relations: BTreeSet::from([relation.clone()]),
        columns: BTreeMap::from([
            ("a".to_owned(), BTreeMap::from([("x".to_owned(), column)])),
            ("b".to_owned(), BTreeMap::new()),
        ]),
        value_vertices: BTreeSet::from(["value".to_owned()]),
        inner_edges: BTreeSet::new(),
    };
    let table = |table: &str, cells: BTreeMap<String, Value>| Table {
        rows: BTreeMap::from([(0, (table.to_owned(), cells))]),
        pairs: BTreeSet::new(),
    };

    assert_eq!(
        encode::decode_table(&table("outside", BTreeMap::new()), &GraphSide::default()),
        Err(nonconforming(0, None, Conformance::TableOutside))
    );
    assert_eq!(
        encode::decode_table(&table("a", BTreeMap::new()), &side),
        Err(nonconforming(0, Some("x"), Conformance::MissingCell))
    );
    assert_eq!(
        encode::decode_table(
            &table("b", BTreeMap::from([("unknown".to_owned(), Value::Int(1))]),),
            &side,
        ),
        Err(nonconforming(0, Some("unknown"), Conformance::StrayCell))
    );
    assert_eq!(
        encode::decode_table(
            &table(
                "a",
                BTreeMap::from([("x".to_owned(), Value::List(vec![Value::LabeledNull(7)]),)]),
            ),
            &side,
        ),
        Err(nonconforming(0, Some("x"), Conformance::NestedLabeledNull,))
    );

    let rows = BTreeMap::from([
        (
            0,
            (
                "a".to_owned(),
                BTreeMap::from([("x".to_owned(), Value::Int(1))]),
            ),
        ),
        (1, ("b".to_owned(), BTreeMap::new())),
    ]);
    let outside = SEdge::new("a", "b", "fk", Some("outside"));
    let outside_pair = Table {
        rows: rows.clone(),
        pairs: BTreeSet::from([(outside.clone(), 0, 1)]),
    };
    assert_eq!(
        encode::decode_table(&outside_pair, &side),
        Err(nonconforming(
            0,
            None,
            Conformance::RelationOutside(Box::new(outside)),
        ))
    );
    let incoherent = Table {
        rows,
        pairs: BTreeSet::from([(relation.clone(), 1, 0)]),
    };
    assert_eq!(
        encode::decode_table(&incoherent, &side),
        Err(nonconforming(
            1,
            None,
            Conformance::IncoherentPair(Box::new(relation)),
        ))
    );
}
