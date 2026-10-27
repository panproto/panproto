#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The shape encodings are inverse on their images.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::encode::{self, CellLayout, EdgeLayout, GraphLayout, GraphSide};
use panproto_reference::encode::{DecodeTableError, TreeEncodeError};
use panproto_reference::enumerate::kan_extensions as bounded;
use panproto_reference::migration::Migration;
use panproto_reference::model::{Fan, GEdge, Graph, Node, NodeShape, SEdge, Table, Tree};
use panproto_reference::rel::{self, Conformance, NonConforming};
use panproto_reference::schema::{HyperEdge, Schema};
use panproto_reference::value::{Presence, Value};

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

// Existing tree nodes are not allocations. A fan-free tree may therefore
// hold `u32::MAX`; its saturated floor merely prevents a later fresh node.
// A tree one below it also encodes with the floor at `u32::MAX`, and an
// embedding tree apply accepts that presentation.
#[test]
fn a_fan_free_tree_preserves_the_reserved_maximum_at_a_saturated_floor() {
    let leaf_edge = SEdge::new("r", "leaf", "prop", Some("leaf"));
    let tree_with = |leaf: u32| Tree {
        nodes: BTreeMap::from([(0, Node::new(0, "r")), (leaf, Node::new(leaf, "leaf"))]),
        arcs: vec![(0, leaf, leaf_edge.clone())],
        fans: Vec::new(),
        root: 0,
    };
    let at_max = tree_with(u32::MAX);
    let (graph, embedding) = encode::tree_to_graph(&at_max, None).unwrap();
    assert_eq!(graph.nodes, at_max.nodes);
    assert_eq!(graph.edges.len(), 1);
    assert_eq!(
        embedding,
        encode::TreeEmbedding {
            arcs: BTreeMap::from([(u32::MAX, 0)]),
            fans: Vec::new(),
            next_node: u32::MAX,
            next_edge: 1,
        }
    );
    let alone = Tree {
        nodes: BTreeMap::from([(u32::MAX, Node::new(u32::MAX, "r"))]),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: u32::MAX,
    };
    let (graph, embedding) = encode::tree_to_graph(&alone, None).unwrap();
    assert_eq!(graph.nodes, alone.nodes);
    assert!(graph.edges.is_empty());
    assert_eq!(
        embedding,
        encode::TreeEmbedding {
            arcs: BTreeMap::new(),
            fans: Vec::new(),
            next_node: u32::MAX,
            next_edge: 0,
        }
    );

    let empty = Tree {
        nodes: BTreeMap::new(),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 0,
    };
    assert_eq!(
        encode::tree_to_graph(&empty, None),
        Ok((Graph::default(), encode::TreeEmbedding::default()))
    );

    let below = tree_with(u32::MAX - 1);
    let (graph, mut embedding) = encode::tree_to_graph(&below, None).unwrap();
    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(
        embedding,
        encode::TreeEmbedding {
            arcs: BTreeMap::from([(u32::MAX - 1, 0)]),
            fans: Vec::new(),
            next_node: u32::MAX,
            next_edge: 1,
        }
    );
    assert_eq!(
        panproto_reference::edit::apply_tree(
            &panproto_reference::edit::TreeEdit::Identity,
            &below,
            &mut embedding
        ),
        Ok(below.clone())
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
    assert_eq!(encode::graph_to_table(&g, &schema), None);
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
    let schema = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("a.x", "int")
        .edge("a", "a.x", "prop", Some("x"))
        .edge("a", "b", "fk", Some("f"));
    assert_eq!(encode::graph_to_table(&graph, &schema), Some(table.clone()));
    let side_schema = rel::classify(
        &schema,
        &BTreeSet::from(["int".to_owned()]),
        &BTreeSet::from(["record".to_owned()]),
    )
    .unwrap();
    let side = GraphSide::whole(&side_schema);
    let decoded = encode::decode_table(&table, &side, &GraphLayout::default()).unwrap();
    assert_eq!(encode::encode_graph(&decoded, &side).unwrap().0, table);
    let id = Migration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges,
        ..Migration::default()
    };
    let functor = rel::entity_functor(&side_schema, &side_schema, &id)
        .unwrap()
        .into_kan()
        .unwrap();
    assert_eq!(
        encode::g_sigma_along(&functor, &decoded),
        Ok(decoded.clone())
    );
    assert_eq!(encode::g_delta_along(&functor, &decoded), Ok(decoded));
}

#[test]
fn every_o7_labelled_graph_presentation_decodes_after_encoding_exactly() {
    let schema = bounded::encoding_schema();
    let classified = rel::classify(
        &schema,
        &BTreeSet::from(["value".to_owned(), "str".to_owned(), "int".to_owned()]),
        &BTreeSet::from(["record".to_owned()]),
    )
    .unwrap();
    let side = GraphSide::whole(&classified);
    let graphs = bounded::encoding_graph_presentations();
    assert_eq!(graphs.len(), 26_508);
    for graph in graphs {
        let (table, layout) = encode::encode_graph(&graph, &side).unwrap();
        assert_eq!(encode::decode_table(&table, &side, &layout), Ok(graph),);
    }
}

#[test]
fn graph_layout_round_trips_ids_positions_shapes_and_decorations_exactly() {
    let attribute = SEdge::new("a", "value", "prop", Some("x"));
    let item = SEdge::new("value", "leaf", "item", Some("item"));
    let relation = SEdge::new("a", "b", "fk", Some("f"));
    let side = GraphSide {
        tables: BTreeSet::from(["a".to_owned(), "b".to_owned()]),
        relations: BTreeSet::from([relation.clone()]),
        columns: BTreeMap::from([(
            "a".to_owned(),
            BTreeMap::from([("x".to_owned(), attribute.clone())]),
        )]),
        value_vertices: BTreeSet::from(["value".to_owned(), "leaf".to_owned()]),
        inner_edges: BTreeSet::from([item.clone()]),
    };
    let mut table_node = Node::new(10, "a");
    table_node.discriminator = Some("disc".to_owned());
    table_node
        .fields
        .insert("extra".to_owned(), Value::Bool(true));
    table_node.position = Some(12);
    table_node
        .annotations
        .insert("ann".to_owned(), Value::Int(2));
    let mut list = Node::new(20, "value");
    list.shape = NodeShape::List;
    list.annotations
        .insert("list".to_owned(), Value::Str("yes".to_owned()));
    let mut leaf = Node::new(21, "leaf");
    leaf.value = Some(Presence::Present(Value::Int(7)));
    leaf.discriminator = Some("leaf-disc".to_owned());
    let graph = Graph {
        nodes: BTreeMap::from([
            (10, table_node),
            (20, list),
            (21, leaf),
            (30, Node::new(30, "b")),
        ]),
        edges: BTreeMap::from([
            (
                7,
                GEdge {
                    src: 10,
                    tgt: 20,
                    label: attribute,
                    position: Some(3),
                },
            ),
            (
                8,
                GEdge {
                    src: 20,
                    tgt: 21,
                    label: item,
                    position: Some(0),
                },
            ),
            (
                9,
                GEdge {
                    src: 10,
                    tgt: 30,
                    label: relation,
                    position: Some(4),
                },
            ),
        ]),
    };
    let (table, layout) = encode::encode_graph(&graph, &side).unwrap();
    assert_eq!(table.rows[&10].1["x"], Value::List(vec![Value::Int(7)]));
    assert_eq!(encode::decode_table(&table, &side, &layout), Ok(graph));
}

#[test]
fn table_decode_refuses_nodes_before_edges_and_positions() {
    let x = SEdge::new("a", "vx", "prop", Some("x"));
    let y = SEdge::new("a", "vy", "prop", Some("y"));
    let side = GraphSide {
        tables: BTreeSet::from(["a".to_owned()]),
        columns: BTreeMap::from([(
            "a".to_owned(),
            BTreeMap::from([("x".to_owned(), x), ("y".to_owned(), y)]),
        )]),
        value_vertices: BTreeSet::from(["vx".to_owned(), "vy".to_owned()]),
        ..GraphSide::default()
    };
    let table = Table {
        rows: BTreeMap::from([(
            0,
            (
                "a".to_owned(),
                BTreeMap::from([
                    ("x".to_owned(), Value::Int(1)),
                    ("y".to_owned(), Value::Int(2)),
                ]),
            ),
        )]),
        pairs: BTreeSet::new(),
    };
    let layout = GraphLayout {
        reserved_node_ids: BTreeSet::from([u32::MAX - 2]),
        reserved_edge_ids: BTreeSet::from([u32::MAX]),
        ..GraphLayout::default()
    };
    assert_eq!(
        encode::decode_table(&table, &side, &layout),
        Err(DecodeTableError::NodeIdsExhausted)
    );
}

#[test]
fn created_edges_take_consecutive_positions_after_carried_edges() {
    let arrow = |column: &str| SEdge {
        src: "a".to_owned(),
        tgt: format!("v{column}"),
        kind: "prop".to_owned(),
        name: Some(column.to_owned()),
    };
    let side = GraphSide {
        tables: BTreeSet::from(["a".to_owned()]),
        columns: BTreeMap::from([(
            "a".to_owned(),
            ["x", "y", "z"]
                .into_iter()
                .map(|column| (column.to_owned(), arrow(column)))
                .collect(),
        )]),
        value_vertices: ["vx", "vy", "vz"].into_iter().map(str::to_owned).collect(),
        ..GraphSide::default()
    };
    let table = Table {
        rows: BTreeMap::from([(
            0,
            (
                "a".to_owned(),
                BTreeMap::from([
                    ("x".to_owned(), Value::Int(1)),
                    ("y".to_owned(), Value::Int(2)),
                    ("z".to_owned(), Value::Int(3)),
                ]),
            ),
        )]),
        pairs: BTreeSet::new(),
    };
    let carried = |position| CellLayout {
        root: 10,
        nodes: BTreeMap::from([(10, Node::new(10, "vx").with_value(Value::Int(1)))]),
        edges: BTreeMap::new(),
        attribute: EdgeLayout {
            id: Some(1),
            position: Some(position),
            has_source: true,
        },
    };
    let layout = |position| GraphLayout {
        cells: BTreeMap::from([((0, "x".to_owned()), carried(position))]),
        reserved_node_ids: BTreeSet::from([10]),
        reserved_edge_ids: BTreeSet::from([1]),
        ..GraphLayout::default()
    };
    let decoded = encode::decode_table(&table, &side, &layout(5)).unwrap();
    let mut positions: Vec<u32> = decoded
        .edges
        .values()
        .filter_map(|edge| edge.position)
        .collect();
    positions.sort_unstable();
    assert_eq!(positions, vec![5, 6, 7]);

    let one_created = table;
    let mut max_minus_one = layout(u32::MAX - 1);
    max_minus_one.cells.insert(
        (0, "z".to_owned()),
        CellLayout {
            root: 11,
            nodes: BTreeMap::from([(11, Node::new(11, "vz").with_value(Value::Int(3)))]),
            edges: BTreeMap::new(),
            attribute: EdgeLayout {
                id: Some(2),
                position: None,
                has_source: true,
            },
        },
    );
    max_minus_one.reserved_node_ids.insert(11);
    max_minus_one.reserved_edge_ids.insert(2);
    let decoded = encode::decode_table(&one_created, &side, &max_minus_one).unwrap();
    assert!(
        decoded
            .edges
            .values()
            .any(|edge| edge.position == Some(u32::MAX))
    );

    let mut at_max = max_minus_one;
    at_max
        .cells
        .get_mut(&(0, "x".to_owned()))
        .unwrap()
        .attribute
        .position = Some(u32::MAX);
    assert_eq!(
        encode::decode_table(&one_created, &side, &at_max),
        Err(DecodeTableError::EdgePositionsExhausted { node: 0 })
    );
}

#[test]
fn table_decode_ignores_a_cyclic_layout_instead_of_emitting_a_cyclic_cell() {
    let attribute = SEdge::new("a", "value", "prop", Some("x"));
    let inner = SEdge::new("value", "value", "item", Some("child"));
    let side = GraphSide {
        tables: BTreeSet::from(["a".to_owned()]),
        columns: BTreeMap::from([(
            "a".to_owned(),
            BTreeMap::from([("x".to_owned(), attribute)]),
        )]),
        value_vertices: BTreeSet::from(["value".to_owned()]),
        inner_edges: BTreeSet::from([inner.clone()]),
        ..GraphSide::default()
    };
    let table = Table {
        rows: BTreeMap::from([(
            0,
            (
                "a".to_owned(),
                BTreeMap::from([("x".to_owned(), Value::Int(1))]),
            ),
        )]),
        pairs: BTreeSet::new(),
    };
    let cyclic = CellLayout {
        root: 1,
        nodes: BTreeMap::from([(1, Node::new(1, "value"))]),
        edges: BTreeMap::from([(
            2,
            GEdge {
                src: 1,
                tgt: 1,
                label: inner,
                position: Some(0),
            },
        )]),
        attribute: EdgeLayout {
            id: Some(3),
            position: Some(9),
            has_source: true,
        },
    };
    let layout = GraphLayout {
        cells: BTreeMap::from([((0, "x".to_owned()), cyclic)]),
        ..GraphLayout::default()
    };
    let decoded = encode::decode_table(&table, &side, &layout).unwrap();
    assert_eq!(decoded.nodes.len(), 2);
    assert_eq!(decoded.edges.len(), 1);
    assert_eq!(encode::encode_graph(&decoded, &side).unwrap().0, table);
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
    let near = encode::decode_table(&table(u32::MAX - 2), &side, &GraphLayout::default()).unwrap();
    assert!(near.nodes.contains_key(&(u32::MAX - 1)));
    for reserved in [u32::MAX - 1, u32::MAX] {
        assert_eq!(
            encode::decode_table(&table(reserved), &side, &GraphLayout::default()),
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
        encode::decode_table(&dangling, &GraphSide::default(), &GraphLayout::default(),),
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
        encode::decode_table(
            &table("outside", BTreeMap::new()),
            &GraphSide::default(),
            &GraphLayout::default(),
        ),
        Err(nonconforming(0, None, Conformance::TableOutside))
    );
    assert_eq!(
        encode::decode_table(&table("a", BTreeMap::new()), &side, &GraphLayout::default(),),
        Err(nonconforming(0, Some("x"), Conformance::MissingCell))
    );
    assert_eq!(
        encode::decode_table(
            &table("b", BTreeMap::from([("unknown".to_owned(), Value::Int(1))]),),
            &side,
            &GraphLayout::default(),
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
            &GraphLayout::default(),
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
        encode::decode_table(&outside_pair, &side, &GraphLayout::default()),
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
        encode::decode_table(&incoherent, &side, &GraphLayout::default()),
        Err(nonconforming(
            1,
            None,
            Conformance::IncoherentPair(Box::new(relation)),
        ))
    );
}

#[test]
fn graph_kan_homs_and_transports_match_the_identity_table_adjunctions() {
    let schema = Schema::default()
        .vertex("a", "record")
        .vertex("a.x", "int")
        .edge("a", "a.x", "prop", Some("x"));
    let classified = rel::classify(
        &schema,
        &BTreeSet::from(["int".to_owned()]),
        &BTreeSet::from(["record".to_owned()]),
    )
    .unwrap();
    let migration = Migration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges,
        ..Migration::default()
    };
    let functor = rel::entity_functor(&classified, &classified, &migration)
        .unwrap()
        .into_kan()
        .unwrap();
    let table = Table {
        rows: BTreeMap::from([(
            5,
            (
                "a".to_owned(),
                BTreeMap::from([("x".to_owned(), Value::LabeledNull(4))]),
            ),
        )]),
        pairs: BTreeSet::new(),
    };
    let side = GraphSide::whole(&classified);
    let graph = encode::decode_table(&table, &side, &GraphLayout::default()).unwrap();
    let identity = encode::GraphKanHom::identity(&graph, &side).unwrap();
    assert_eq!(identity.node_map, BTreeMap::from([(5, 5)]));
    identity.check(&graph, &graph, &side).unwrap();
    assert!(identity.compose(&identity).unwrap().same_as(&identity));

    for transported in [
        encode::g_unit(&graph, &functor).unwrap(),
        encode::g_counit(&graph, &functor).unwrap(),
        encode::g_pi_unit(&graph, &functor, 4).unwrap(),
        encode::g_pi_counit(&graph, &functor, 4).unwrap(),
        encode::g_transpose_left(&identity, &graph, &graph, &functor).unwrap(),
        encode::g_transpose_right(&identity, &graph, &graph, &functor).unwrap(),
        encode::g_pi_transpose_left(&identity, &graph, &graph, &functor, 4).unwrap(),
        encode::g_pi_transpose_right(&identity, &graph, &graph, &functor, 4).unwrap(),
    ] {
        assert!(transported.same_as(&identity));
    }

    for operation in [
        encode::g_sigma_along_with_provenance(&functor, &graph)
            .map(|(graph, provenance)| (graph, provenance.nodes, provenance.edges)),
        encode::g_delta_along_with_provenance(&functor, &graph)
            .map(|(graph, provenance)| (graph, provenance.nodes, provenance.edges)),
        encode::g_pi_along_with_provenance(&functor, &graph, 4)
            .map(|(graph, provenance)| (graph, provenance.nodes, provenance.edges)),
    ] {
        let (output, nodes, edges) = operation.unwrap();
        assert_eq!(output, graph);
        for node in output.nodes.keys() {
            assert_eq!(nodes[node], BTreeSet::from([*node]));
        }
        for edge in output.edges.keys() {
            assert_eq!(edges[edge], BTreeSet::from([*edge]));
        }
    }
}

fn global_value_class_functor() -> (rel::RelFunctor, SEdge, SEdge, SEdge, SEdge) {
    let attribute_x = SEdge::new("r", "vx", "prop", Some("x"));
    let attribute_y = SEdge::new("r", "vy", "prop", Some("y"));
    let attribute_q = SEdge::new("r", "vq", "prop", Some("q"));
    let attribute_z = SEdge::new("r", "v", "prop", Some("z"));
    let attribute_w = SEdge::new("r", "v", "prop", Some("w"));
    let source = rel::RelSchema {
        tables: BTreeSet::from(["r".to_owned()]),
        columns: BTreeMap::from([(
            "r".to_owned(),
            BTreeMap::from([
                ("q".to_owned(), attribute_q.clone()),
                ("x".to_owned(), attribute_x.clone()),
                ("y".to_owned(), attribute_y.clone()),
            ]),
        )]),
        value_vertices: BTreeSet::from(["vq".to_owned(), "vx".to_owned(), "vy".to_owned()]),
        ..rel::RelSchema::default()
    };
    let target = rel::RelSchema {
        tables: BTreeSet::from(["r".to_owned()]),
        columns: BTreeMap::from([(
            "r".to_owned(),
            BTreeMap::from([
                ("w".to_owned(), attribute_w),
                ("z".to_owned(), attribute_z.clone()),
            ]),
        )]),
        value_vertices: BTreeSet::from(["v".to_owned()]),
        ..rel::RelSchema::default()
    };
    let functor = rel::RelFunctor {
        src: source,
        tgt: target,
        tables: BTreeMap::from([("r".to_owned(), "r".to_owned())]),
        relations: BTreeMap::new(),
        columns: BTreeMap::from([
            (("r".to_owned(), "q".to_owned()), "w".to_owned()),
            (("r".to_owned(), "x".to_owned()), "z".to_owned()),
            (("r".to_owned(), "y".to_owned()), "w".to_owned()),
        ]),
        inner_edges: BTreeMap::new(),
        value_vertices: BTreeMap::from([
            ("vq".to_owned(), "v".to_owned()),
            ("vx".to_owned(), "v".to_owned()),
            ("vy".to_owned(), "v".to_owned()),
        ]),
    };
    (functor, attribute_x, attribute_y, attribute_q, attribute_z)
}

#[test]
fn sigma_layout_uses_direct_slot_sources_and_records_their_origins() {
    let (functor, attribute_x, attribute_y, attribute_q, attribute_z) =
        global_value_class_functor();
    let attribute_w = functor.tgt.columns["r"]["w"].clone();
    let mut q_node = Node::new(12, "vq");
    q_node.value = Some(Presence::Present(Value::Int(1)));
    q_node
        .annotations
        .insert("origin".to_owned(), Value::Int(12));
    let graph = Graph {
        nodes: BTreeMap::from([
            (0, Node::new(0, "r")),
            (10, Node::new(10, "vx").with_value(Value::LabeledNull(0))),
            (11, Node::new(11, "vy").with_value(Value::LabeledNull(0))),
            (12, q_node),
        ]),
        edges: BTreeMap::from([
            (
                20,
                GEdge {
                    src: 0,
                    tgt: 10,
                    label: attribute_x,
                    position: None,
                },
            ),
            (
                21,
                GEdge {
                    src: 0,
                    tgt: 11,
                    label: attribute_y,
                    position: None,
                },
            ),
            (
                22,
                GEdge {
                    src: 0,
                    tgt: 12,
                    label: attribute_q,
                    position: None,
                },
            ),
        ]),
    };
    let (output, provenance) = encode::g_sigma_along_with_provenance(&functor, &graph).unwrap();
    let (table, _) = encode::encode_graph(&output, &GraphSide::whole(&functor.tgt)).unwrap();
    assert_eq!(table.rows[&0].1["z"], Value::Int(1));
    assert_eq!(table.rows[&0].1["w"], Value::Int(1));
    let (z_edge_id, z_edge) = output
        .edges
        .iter()
        .find(|(_, edge)| edge.label == attribute_z)
        .unwrap();
    assert!(!output.nodes[&z_edge.tgt].annotations.contains_key("origin"));
    assert_eq!(provenance.nodes[&z_edge.tgt], BTreeSet::from([10]));
    assert_eq!(provenance.edges[z_edge_id], BTreeSet::from([20]));

    let (w_edge_id, w_edge) = output
        .edges
        .iter()
        .find(|(_, edge)| edge.label == attribute_w)
        .unwrap();
    assert_eq!(
        output.nodes[&w_edge.tgt].annotations["origin"],
        Value::Int(12)
    );
    assert_eq!(provenance.nodes[&w_edge.tgt], BTreeSet::from([11, 12]));
    assert_eq!(provenance.edges[w_edge_id], BTreeSet::from([21, 22]));
}
