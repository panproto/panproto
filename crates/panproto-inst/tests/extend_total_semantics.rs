#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Total extension has one elementwise domain in trees, graphs and tables.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_inst::{
    BoundaryProtocols, CompiledMigration, EdgeId, FInstance, GInstance, MigrationFunctor, Node,
    RestrictError, RowId, ViewRef, WInstance, f_sigma, functor_extend, functor_extend_partial,
    graph_extend, graph_extend_partial, wtype_extend, wtype_extend_partial,
};
use panproto_schema::{Edge, Protocol, ScalarKind, Schema, SchemaBuilder};
use proptest::prelude::*;

fn protocol() -> Protocol {
    Protocol {
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    }
}

fn edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "fk".into(),
        name: Some(name.into()),
    }
}

fn schema(vertices: &[&str], edges: &[Edge]) -> Schema {
    let protocol = protocol();
    let mut builder = SchemaBuilder::new(&protocol);
    for vertex in vertices {
        builder = builder.vertex(vertex, "record", None).unwrap();
    }
    for edge in edges {
        builder = builder
            .edge(
                edge.src.as_str(),
                edge.tgt.as_str(),
                edge.kind.as_str(),
                edge.name.as_deref(),
            )
            .unwrap();
    }
    builder.build().unwrap()
}

fn migration(vertices: &[(&str, &str)], edges: &[(Edge, Edge)]) -> CompiledMigration {
    CompiledMigration {
        surviving_verts: vertices
            .iter()
            .map(|(_, target)| (*target).into())
            .collect(),
        vertex_remap: vertices
            .iter()
            .map(|(source, target)| ((*source).into(), (*target).into()))
            .collect(),
        edge_remap: edges.iter().cloned().collect(),
        ..CompiledMigration::default()
    }
}

fn assert_exact_agreement(first: u32, second: u32) {
    let source_edge = edge("a", "b", "r");
    let target_edge = edge("A", "B", "R");
    let source_schema = schema(&["a", "b"], std::slice::from_ref(&source_edge));
    let target_schema = schema(&["A", "B"], std::slice::from_ref(&target_edge));
    let migration = migration(
        &[("a", "A"), ("b", "B")],
        &[(source_edge.clone(), target_edge)],
    );
    let protocol = protocol();
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };

    let tree = WInstance::new(
        HashMap::from([
            (first, Node::new(first, "a")),
            (second, Node::new(second, "b")),
        ]),
        vec![(first, second, source_edge.clone())],
        vec![],
        first,
        "a".into(),
    );
    let (source_graph, tree_embedding) = GInstance::from_wtype_embedded(&tree, None).unwrap();
    let table = FInstance::new()
        .with_rows("a", [(RowId(first), HashMap::new())])
        .and_then(|table| table.with_rows("b", [(RowId(second), HashMap::new())]))
        .and_then(|table| {
            table.with_foreign_key(source_edge.clone(), [(RowId(first), RowId(second))])
        })
        .unwrap();

    let tree_result =
        wtype_extend(&tree, &source_schema, &target_schema, &migration, protocols).unwrap();
    let graph_result = graph_extend(
        &source_graph,
        &source_schema,
        &target_schema,
        &migration,
        protocols,
    )
    .unwrap();
    let table_result = functor_extend(
        &table,
        &source_schema,
        &target_schema,
        &migration,
        protocols,
    )
    .unwrap();
    let table_sigma = f_sigma(
        &table,
        &MigrationFunctor::new(
            &source_schema,
            &protocol,
            &target_schema,
            &protocol,
            &migration,
        )
        .unwrap(),
    )
    .unwrap();

    let tree_graph = GInstance::from_wtype_under(&tree_result, &tree_embedding, None).unwrap();
    let (table_graph, _) = GInstance::from_finstance(&table_result).unwrap();
    assert_eq!(table_result, table_sigma);
    assert!(GInstance::identical(&tree_graph, &graph_result));
    assert!(GInstance::identical(&table_graph, &graph_result));
}

#[test]
fn extends_agree_on_the_exact_domain() {
    assert_exact_agreement(10, 20);
}

#[test]
fn table_extend_is_table_sigma_on_the_exact_domain() {
    assert_exact_agreement(11, 29);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn extends_agree_on_the_exact_domain_for_arbitrary_distinct_ids(
        first in any::<u16>(),
        second in any::<u16>(),
    ) {
        prop_assume!(first != second);
        assert_exact_agreement(u32::from(first), u32::from(second));
    }
}

#[test]
fn an_extend_refuses_an_element_without_an_image_in_every_shape() {
    let source_edge = edge("a", "x", "r");
    let source_schema = schema(&["a", "x"], std::slice::from_ref(&source_edge));
    let target_schema = schema(&["a"], &[]);
    let migration = CompiledMigration {
        surviving_verts: ["a".into()].into(),
        ..CompiledMigration::default()
    };
    let protocol = protocol();
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let tree = WInstance::new(
        HashMap::from([(10, Node::new(10, "a")), (20, Node::new(20, "x"))]),
        vec![(10, 20, source_edge.clone())],
        vec![],
        10,
        "a".into(),
    );
    let graph = GInstance::new()
        .with_node(Node::new(10, "a"))
        .with_node(Node::new(20, "x"))
        .with_edge_id(EdgeId(7), 10, 20, source_edge.clone());
    let table = FInstance::new()
        .with_rows("a", [(RowId(10), HashMap::new())])
        .and_then(|table| table.with_rows("x", [(RowId(20), HashMap::new())]))
        .and_then(|table| table.with_foreign_key(source_edge, [(RowId(10), RowId(20))]))
        .unwrap();

    for error in [
        wtype_extend(&tree, &source_schema, &target_schema, &migration, protocols).unwrap_err(),
        graph_extend(
            &graph,
            &source_schema,
            &target_schema,
            &migration,
            protocols,
        )
        .unwrap_err(),
        functor_extend(
            &table,
            &source_schema,
            &target_schema,
            &migration,
            protocols,
        )
        .unwrap_err(),
    ] {
        assert!(
            matches!(error, RestrictError::UnmappedAnchor { node_id: 20, .. }),
            "{error:?}"
        );
    }
}

#[test]
fn partial_extends_report_every_omitted_element() {
    let source_edge = edge("a", "x", "r");
    let source_schema = schema(&["a", "x"], std::slice::from_ref(&source_edge));
    let target_schema = schema(&["a"], &[]);
    let migration = CompiledMigration {
        surviving_verts: ["a".into()].into(),
        ..CompiledMigration::default()
    };
    let protocol = protocol();
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let tree = WInstance::new(
        HashMap::from([(10, Node::new(10, "a")), (20, Node::new(20, "x"))]),
        vec![(10, 20, source_edge.clone())],
        vec![],
        10,
        "a".into(),
    );
    let graph = GInstance::new()
        .with_node(Node::new(10, "a"))
        .with_node(Node::new(20, "x"))
        .with_edge_id(EdgeId(7), 10, 20, source_edge.clone());
    let table = FInstance::new()
        .with_rows("a", [(RowId(10), HashMap::new())])
        .and_then(|table| table.with_rows("x", [(RowId(20), HashMap::new())]))
        .and_then(|table| table.with_foreign_key(source_edge.clone(), [(RowId(10), RowId(20))]))
        .unwrap();

    let (tree_out, tree_dropped) =
        wtype_extend_partial(&tree, &source_schema, &target_schema, &migration, protocols).unwrap();
    assert_eq!(tree_out.nodes.keys().copied().collect::<Vec<_>>(), [10]);
    assert_eq!(
        tree_dropped,
        [
            ViewRef::Node(20),
            ViewRef::Arc {
                parent: 10,
                child: 20,
            },
        ]
    );

    let (graph_out, graph_dropped) = graph_extend_partial(
        &graph,
        &source_schema,
        &target_schema,
        &migration,
        protocols,
    )
    .unwrap();
    assert_eq!(graph_out.nodes().keys().copied().collect::<Vec<_>>(), [10]);
    assert_eq!(graph_dropped, [ViewRef::Node(20), ViewRef::Edge(EdgeId(7))]);

    let (table_out, table_dropped) = functor_extend_partial(
        &table,
        &source_schema,
        &target_schema,
        &migration,
        protocols,
    )
    .unwrap();
    assert_eq!(table_out.row_ids(), vec![RowId(10)]);
    assert_eq!(
        table_dropped,
        [
            ViewRef::Row(RowId(20)),
            ViewRef::Pair {
                edge: source_edge,
                src: RowId(10),
                tgt: RowId(20),
            },
        ]
    );
}

#[test]
fn partial_table_extension_reports_and_omits_an_unmapped_cell() {
    let protocol = Protocol {
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        value_kinds: BTreeMap::from([("value".to_owned(), ScalarKind::Any)]),
        ..Protocol::default()
    };
    let source_schema = SchemaBuilder::new(&protocol)
        .vertex("a", "record", None)
        .and_then(|builder| builder.vertex("a.value", "value", None))
        .and_then(|builder| builder.edge("a", "a.value", "prop", Some("value")))
        .and_then(SchemaBuilder::build)
        .unwrap();
    let target_schema = SchemaBuilder::new(&protocol)
        .vertex("a", "record", None)
        .and_then(SchemaBuilder::build)
        .unwrap();
    let migration = CompiledMigration {
        surviving_verts: ["a".into()].into(),
        ..CompiledMigration::default()
    };
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let table = FInstance::new()
        .with_rows(
            "a",
            [(
                RowId(10),
                HashMap::from([("value".to_owned(), panproto_inst::Value::Int(7))]),
            )],
        )
        .unwrap();

    assert!(matches!(
        functor_extend(
            &table,
            &source_schema,
            &target_schema,
            &migration,
            protocols,
        ),
        Err(RestrictError::UnmappedEdge {
            element: ViewRef::Row(RowId(10)),
            ..
        })
    ));
    let (extended, omitted) = functor_extend_partial(
        &table,
        &source_schema,
        &target_schema,
        &migration,
        protocols,
    )
    .unwrap();
    assert_eq!(extended.row(RowId(10)), Some(&HashMap::new()));
    assert_eq!(omitted, [ViewRef::Row(RowId(10))]);
}

#[test]
fn an_extend_keeps_distinct_element_ids() {
    let source_edge = edge("a", "b", "r");
    let target_edge = edge("m", "m", "r");
    let source_schema = schema(&["a", "b"], std::slice::from_ref(&source_edge));
    let target_schema = schema(&["m"], std::slice::from_ref(&target_edge));
    let migration = migration(
        &[("a", "m"), ("b", "m")],
        &[(source_edge.clone(), target_edge)],
    );
    let protocol = protocol();
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let tree = WInstance::new(
        HashMap::from([(10, Node::new(10, "a")), (20, Node::new(20, "b"))]),
        vec![(10, 20, source_edge.clone())],
        vec![],
        10,
        "a".into(),
    );
    let graph = GInstance::new()
        .with_node(Node::new(10, "a"))
        .with_node(Node::new(20, "b"))
        .with_edge_id(EdgeId(7), 10, 20, source_edge.clone());
    let table = FInstance::new()
        .with_rows("a", [(RowId(10), HashMap::new())])
        .and_then(|table| table.with_rows("b", [(RowId(20), HashMap::new())]))
        .and_then(|table| table.with_foreign_key(source_edge, [(RowId(10), RowId(20))]))
        .unwrap();

    let tree_out =
        wtype_extend(&tree, &source_schema, &target_schema, &migration, protocols).unwrap();
    let graph_out = graph_extend(
        &graph,
        &source_schema,
        &target_schema,
        &migration,
        protocols,
    )
    .unwrap();
    let table_out = functor_extend(
        &table,
        &source_schema,
        &target_schema,
        &migration,
        protocols,
    )
    .unwrap();
    let mut tree_ids: Vec<u32> = tree_out.nodes.keys().copied().collect();
    tree_ids.sort_unstable();
    let mut graph_ids: Vec<u32> = graph_out.nodes().keys().copied().collect();
    graph_ids.sort_unstable();
    assert_eq!(tree_ids, [10, 20]);
    assert_eq!(graph_ids, [10, 20]);
    assert_eq!(table_out.row_ids(), vec![RowId(10), RowId(20)]);
}

#[test]
fn an_extend_of_a_non_tree_input_takes_the_graph_path() {
    let forward = edge("a", "b", "forward");
    let back = edge("b", "a", "back");
    let source_schema = schema(&["a", "b"], &[forward.clone(), back.clone()]);
    let target_schema = source_schema.clone();
    let migration = migration(
        &[("a", "a"), ("b", "b")],
        &[
            (forward.clone(), forward.clone()),
            (back.clone(), back.clone()),
        ],
    );
    let protocol = protocol();
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let source = GInstance::new()
        .with_node(Node::new(3, "a"))
        .with_node(Node::new(5, "b"))
        .with_edge_id(EdgeId(7), 3, 5, forward)
        .with_edge_id(EdgeId(9), 5, 3, back);

    assert!(source.try_to_wtype(&source_schema).is_err());
    let extended = graph_extend(
        &source,
        &source_schema,
        &target_schema,
        &migration,
        protocols,
    )
    .unwrap();
    assert!(GInstance::identical(&extended, &source));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn an_s3_8_extend_of_a_set_valued_source_has_no_realized_pair_collision(
        drawn in proptest::collection::btree_set((0_u8..8, 0_u8..8), 0..32),
    ) {
        let source_edge = edge("a", "b", "r");
        let target_edge = edge("A", "B", "R");
        let source_schema = schema(&["a", "b"], std::slice::from_ref(&source_edge));
        let target_schema = schema(&["A", "B"], std::slice::from_ref(&target_edge));
        let migration = migration(
            &[("a", "A"), ("b", "B")],
            &[(source_edge.clone(), target_edge.clone())],
        );
        let protocol = protocol();
        let protocols = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };
        let left: BTreeSet<RowId> = drawn
            .iter()
            .map(|&(src, _)| RowId(u32::from(src)))
            .collect();
        let right: BTreeSet<RowId> = drawn
            .iter()
            .map(|&(_, tgt)| RowId(256 + u32::from(tgt)))
            .collect();
        let pairs: Vec<(RowId, RowId)> = drawn
            .iter()
            .map(|&(src, tgt)| (RowId(u32::from(src)), RowId(256 + u32::from(tgt))))
            .collect();
        let source = FInstance::new()
            .with_rows("a", left.iter().copied().map(|id| (id, HashMap::new())))
            .and_then(|table| {
                table.with_rows("b", right.iter().copied().map(|id| (id, HashMap::new())))
            })
            .and_then(|table| table.with_foreign_key(source_edge, pairs.clone()))
            .unwrap();

        let extended = functor_extend(
            &source,
            &source_schema,
            &target_schema,
            &migration,
            protocols,
        )
        .unwrap();
        prop_assert_eq!(extended.pairs(&target_edge).len(), drawn.len());
        for pair in pairs {
            prop_assert!(extended.contains_pair(&target_edge, pair.0, pair.1));
        }
    }
}

#[test]
fn table_pairs_coalesce_where_graph_edges_retain_identity() {
    let left = edge("a", "b", "left");
    let right = edge("a", "b", "right");
    let image = edge("A", "B", "joined");
    let source_schema = schema(&["a", "b"], &[left.clone(), right.clone()]);
    let target_schema = schema(&["A", "B"], std::slice::from_ref(&image));
    let migration = migration(
        &[("a", "A"), ("b", "B")],
        &[
            (left.clone(), image.clone()),
            (right.clone(), image.clone()),
        ],
    );
    let protocol = protocol();
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let graph = GInstance::new()
        .with_node(Node::new(10, "a"))
        .with_node(Node::new(20, "b"))
        .with_edge_id(EdgeId(7), 10, 20, left.clone())
        .with_edge_id(EdgeId(8), 10, 20, right.clone());
    let table = FInstance::new()
        .with_rows("a", [(RowId(10), HashMap::new())])
        .and_then(|table| table.with_rows("b", [(RowId(20), HashMap::new())]))
        .and_then(|table| table.with_foreign_key(left, [(RowId(10), RowId(20))]))
        .and_then(|table| table.with_foreign_key(right, [(RowId(10), RowId(20))]))
        .unwrap();

    let graph_out = graph_extend(
        &graph,
        &source_schema,
        &target_schema,
        &migration,
        protocols,
    )
    .unwrap();
    let table_out = functor_extend(
        &table,
        &source_schema,
        &target_schema,
        &migration,
        protocols,
    )
    .unwrap();
    assert_eq!(graph_out.edge_count(), 2);
    assert_eq!(table_out.pairs(&image).len(), 1);
}
