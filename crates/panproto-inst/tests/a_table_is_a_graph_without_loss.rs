#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A table instance is a graph instance without loss: rows are nodes, pairs
//! are edges, and a table-shaped graph reads back as its table.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_inst::functor::Row;
use panproto_inst::value::FieldPresence;
use panproto_inst::{
    EdgeId, FInstance, GEdge, GInstance, InstError, Node, NodeShape, NotTableReason, RowId,
    TableEmbedding, TableShapeReport, Value, ViewRef, write_edge_record, write_node_record,
};
use panproto_schema::{
    CanonicalWriter, Cardinality, Edge, OnUnresolved, RefSource, Reference, ResolveMode, Schema,
};
use proptest::prelude::*;

fn author() -> Edge {
    Edge {
        src: "post".into(),
        tgt: "user".into(),
        kind: "fk".into(),
        name: Some("author".into()),
    }
}

fn cell(v: i64) -> Row {
    HashMap::from([("n".to_owned(), Value::Int(v))])
}

/// Users 4 and 1, posts 9 and 6; post 9 by user 4, post 6 by users 1
/// and 4.
fn tables() -> FInstance {
    FInstance::new()
        .with_rows("user", [(RowId(4), cell(40)), (RowId(1), cell(10))])
        .and_then(|f| f.with_rows("post", [(RowId(9), cell(90)), (RowId(6), cell(60))]))
        .and_then(|f| {
            f.with_foreign_key(
                author(),
                [
                    (RowId(9), RowId(4)),
                    (RowId(6), RowId(4)),
                    (RowId(6), RowId(1)),
                ],
            )
        })
        .unwrap()
}

fn schema() -> Schema {
    let mut schema = Schema::default();
    schema.edges.insert(author(), Name::from("author"));
    schema
}

/// The schema with `author` declared functional, and required unless
/// `nullable`.
fn declaring(nullable: bool) -> Schema {
    let mut schema = schema();
    schema.declarations.references.insert(
        Name::from("author"),
        Reference {
            id: Name::from("author"),
            holder: Name::from("post"),
            source: RefSource::KeyValue { parts: Vec::new() },
            target_vertex: Name::from("user"),
            target_key: None,
            edge: Some(author()),
            cardinality: Cardinality::One,
            nullable,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
        },
    );
    schema
}

#[test]
fn from_finstance_maps_rows_to_nodes_with_cells_as_fields() {
    let (graph, _) = GInstance::from_finstance(&tables()).unwrap();
    assert_eq!(graph.node_count(), 4);
    for id in [1, 4, 6, 9] {
        let node = graph.node(id).unwrap();
        let mut expected = Node::new(id, if id < 5 { "user" } else { "post" });
        expected.extra_fields = cell(i64::from(id) * 10);
        assert_eq!(
            serde_json::to_value(node).unwrap(),
            serde_json::to_value(&expected).unwrap(),
            "row {id} is node {id} with default node data"
        );
    }
}

#[test]
fn from_finstance_maps_pairs_to_labelled_edges() {
    let (graph, embedding) = GInstance::from_finstance(&tables()).unwrap();
    assert_eq!(graph.edge_count(), 3);
    for ((edge, src, tgt), id) in &embedding.pairs {
        assert_eq!(
            graph.edge(*id),
            Some(&GEdge::new(src.0, tgt.0, edge.clone())),
            "each pair is an unpositioned edge labelled with its foreign key"
        );
    }
}

#[test]
fn the_initial_table_encoding_allocates_edges_in_pair_order() {
    let (_, embedding) = GInstance::from_finstance(&tables()).unwrap();
    let order: Vec<((RowId, RowId), EdgeId)> = embedding
        .pairs
        .iter()
        .map(|((_, s, t), id)| ((*s, *t), *id))
        .collect();
    assert_eq!(
        order,
        vec![
            ((RowId(6), RowId(1)), EdgeId(0)),
            ((RowId(6), RowId(4)), EdgeId(1)),
            ((RowId(9), RowId(4)), EdgeId(2)),
        ]
    );
    assert_eq!(embedding.next_edge, 3);
}

#[test]
fn table_map_ref_sends_rows_and_pairs_to_graph_refs() {
    let (_, embedding) = GInstance::from_finstance(&tables()).unwrap();
    assert_eq!(
        embedding.map_ref(&ViewRef::Row(RowId(9))),
        Some(ViewRef::Node(9))
    );
    assert_eq!(
        embedding.map_ref(&ViewRef::Pair {
            edge: author(),
            src: RowId(6),
            tgt: RowId(4),
        }),
        Some(ViewRef::Edge(EdgeId(1)))
    );
    assert_eq!(
        embedding.map_ref(&ViewRef::Pair {
            edge: author(),
            src: RowId(9),
            tgt: RowId(1),
        }),
        None,
        "a pair the embedding does not record has no image"
    );
    assert_eq!(
        embedding.map_ref(&ViewRef::Arc {
            parent: 0,
            child: 1
        }),
        None
    );
    assert_eq!(embedding.map_ref(&ViewRef::Node(3)), Some(ViewRef::Node(3)));
}

#[test]
fn a_table_embedding_allocates_through_an_id_allocator() {
    let mut embedding = TableEmbedding {
        pairs: BTreeMap::new(),
        next_edge: 7,
    };
    assert_eq!(embedding.allocate_edge().unwrap(), EdgeId(7));
    assert_eq!(embedding.next_edge, 8);
    let id = embedding.edge_for(&author(), RowId(1), RowId(2)).unwrap();
    assert_eq!(id, EdgeId(8));
    assert_eq!(
        embedding.edge_for(&author(), RowId(1), RowId(2)).unwrap(),
        id
    );
    assert_eq!(embedding.next_edge, 9, "a recorded pair allocates nothing");
    let mut full = TableEmbedding {
        pairs: BTreeMap::new(),
        next_edge: u32::MAX,
    };
    assert!(matches!(
        full.allocate_edge(),
        Err(InstError::EdgeIdsExhausted)
    ));
    assert_eq!(
        full.next_edge,
        u32::MAX,
        "a failed allocation leaves the floor"
    );
    let json = serde_json::to_string(&embedding).unwrap();
    assert_eq!(
        serde_json::from_str::<TableEmbedding>(&json).unwrap(),
        embedding
    );
}

/// The embedding JSON of one recorded pair `(fk t->t, 0, 1)` with the
/// given edge id and floor.
fn one_pair_embedding_json(id: u32, floor: u32) -> String {
    format!(
        r#"{{"pairs":[[[{{"src":"t","tgt":"t","kind":"fk","name":null}},0,1],{id}]],"next_edge":{floor}}}"#
    )
}

#[test]
fn an_embedding_whose_floor_is_not_above_its_edges_is_refused_on_decode() {
    let ok = serde_json::from_str::<TableEmbedding>(&one_pair_embedding_json(0, 1)).unwrap();
    assert_eq!(ok.next_edge, 1);
    let error = serde_json::from_str::<TableEmbedding>(&one_pair_embedding_json(0, 0))
        .expect_err("a floor at the recorded edge would hand it out again");
    assert!(error.to_string().contains("edge 0"), "{error}");
    let error = serde_json::from_str::<TableEmbedding>(&one_pair_embedding_json(5, 3))
        .expect_err("a floor below the recorded edge");
    assert!(error.to_string().contains("edge 5"), "{error}");
    // The saturated floor holds the largest id.
    let max = u32::MAX;
    serde_json::from_str::<TableEmbedding>(&one_pair_embedding_json(max, max)).unwrap();
}

#[test]
fn an_embedding_sending_two_pairs_to_one_edge_is_refused() {
    let t = Edge {
        src: "t".into(),
        tgt: "t".into(),
        kind: "fk".into(),
        name: None,
    };
    let shared = TableEmbedding {
        pairs: BTreeMap::from([
            ((t.clone(), RowId(0), RowId(1)), EdgeId(0)),
            ((t.clone(), RowId(0), RowId(2)), EdgeId(0)),
        ]),
        next_edge: 1,
    };
    assert!(matches!(
        shared.check(),
        Err(InstError::DuplicateEdge(EdgeId(0)))
    ));
    let json = serde_json::to_string(&shared).unwrap();
    assert!(serde_json::from_str::<TableEmbedding>(&json).is_err());

    // A hand-built embedding with a stale floor is refused before it can
    // hand a recorded edge id to a second pair.
    let mut stale = TableEmbedding {
        pairs: BTreeMap::from([((t.clone(), RowId(0), RowId(1)), EdgeId(0))]),
        next_edge: 0,
    };
    assert!(matches!(
        stale.check(),
        Err(InstError::DuplicateEdge(EdgeId(0)))
    ));
    let tables = FInstance::new()
        .with_rows("t", [(RowId(0), cell(0)), (RowId(2), cell(2))])
        .and_then(|f| f.with_foreign_key(t, [(RowId(0), RowId(2))]))
        .unwrap();
    assert!(matches!(
        GInstance::from_finstance_along(&tables, &mut stale),
        Err(InstError::DuplicateEdge(EdgeId(0)))
    ));
    assert_eq!(
        stale.pairs.len(),
        1,
        "a refused embedding is left as it was"
    );
}

#[test]
fn the_graph_form_of_a_table_is_a_table() {
    let (graph, embedding) = GInstance::from_finstance(&tables()).unwrap();
    assert_eq!(graph.table_shape(&schema()), TableShapeReport::Table);
    let (back, read_embedding) = graph.try_to_finstance(&schema()).unwrap();
    assert_eq!(back, tables());
    assert_eq!(read_embedding, embedding);
}

#[test]
fn table_shape_names_the_first_violation() {
    let (graph, _) = GInstance::from_finstance(&tables()).unwrap();
    let reason = |g: &GInstance, s: &Schema| match g.table_shape(s) {
        TableShapeReport::NotTable { reason } => reason,
        TableShapeReport::Table => panic!("expected a violation"),
    };

    let mut valued = graph.clone();
    valued.node_mut(4).unwrap().value = Some(FieldPresence::Present(Value::Int(1)));
    assert_eq!(
        reason(&valued, &schema()),
        NotTableReason::NodeDataNotRepresentable { node: 4 }
    );

    let mut nested = graph.clone();
    nested
        .node_mut(6)
        .unwrap()
        .extra_fields
        .insert("tags".into(), Value::List(vec![Value::Int(1)]));
    assert_eq!(
        reason(&nested, &schema()),
        NotTableReason::NestedCell {
            node: 6,
            field: "tags".into()
        }
    );

    let fan = graph
        .clone()
        .with_node(Node::new(20, "rel").with_shape(NodeShape::HyperEdge));
    assert_eq!(
        reason(&fan, &schema()),
        NotTableReason::FanNode { node: 20 }
    );

    let mut positioned = graph.clone();
    positioned.set_edge_position(EdgeId(2), Some(0)).unwrap();
    assert_eq!(
        reason(&positioned, &schema()),
        NotTableReason::EdgePositioned { edge: EdgeId(2) }
    );

    assert_eq!(
        reason(&graph, &Schema::default()),
        NotTableReason::EdgeNotForeignKey { edge: EdgeId(0) }
    );

    let backwards = graph.clone().with_edge_id(EdgeId(10), 4, 9, author());
    assert_eq!(
        reason(&backwards, &schema()),
        NotTableReason::IncoherentEndpoints { edge: EdgeId(10) }
    );

    let parallel = graph.clone().with_edge_id(EdgeId(11), 9, 4, author());
    assert_eq!(
        reason(&parallel, &schema()),
        NotTableReason::ParallelForeignKey {
            edges: vec![EdgeId(2), EdgeId(11)]
        }
    );

    assert_eq!(
        reason(&graph, &declaring(true)),
        NotTableReason::MultipleLifts {
            node: 6,
            edge: author(),
            edges: vec![EdgeId(0), EdgeId(1)],
        }
    );

    let mut single = tables();
    assert!(single.remove_foreign_key(&author(), RowId(6), RowId(1)));
    assert!(single.remove_foreign_key(&author(), RowId(9), RowId(4)));
    let (graph, _) = GInstance::from_finstance(&single).unwrap();
    assert_eq!(graph.table_shape(&declaring(true)), TableShapeReport::Table);
    assert_eq!(
        reason(&graph, &declaring(false)),
        NotTableReason::MissingLift {
            node: 9,
            edge: author()
        }
    );
    assert!(matches!(
        graph.try_to_finstance(&declaring(false)),
        Err(InstError::NotATable {
            reason: NotTableReason::MissingLift { node: 9, .. }
        })
    ));
}

#[test]
fn renumbering_positions_changes_no_digest_field_but_position() {
    // Two graphs that differ only in the positions their edges carry:
    // the canonical bytes differ only inside the position fields, since
    // no derived positional code is ever written.
    let (graph, _) = GInstance::from_finstance(&tables()).unwrap();
    let mut first = graph.clone();
    let mut second = graph;
    for (k, id) in [EdgeId(0), EdgeId(1), EdgeId(2)].into_iter().enumerate() {
        let k = u32::try_from(k).unwrap();
        first.set_edge_position(id, Some(k)).unwrap();
        second.set_edge_position(id, Some(100 + k)).unwrap();
    }
    let a = panproto_inst::canonical_ginstance_bytes(&first);
    let b = panproto_inst::canonical_ginstance_bytes(&second);
    assert_eq!(a.len(), b.len());
    // The byte ranges of the position fields: the last four bytes of
    // each edge record, located by writing the records one by one.
    let mut offset = 16;
    let mut ids: Vec<u32> = first.nodes().keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let mut w = CanonicalWriter::new();
        write_node_record(&mut w, &first.nodes()[&id]);
        offset += w.into_bytes().len();
    }
    let mut positions = Vec::new();
    for (id, edge) in first.edges_sorted() {
        let mut w = CanonicalWriter::new();
        write_edge_record(&mut w, id, edge);
        offset += w.into_bytes().len();
        positions.push(offset - 4..offset);
    }
    assert_eq!(offset, a.len());
    for (i, (x, y)) in a.iter().zip(&b).enumerate() {
        if x != y {
            assert!(
                positions.iter().any(|r| r.contains(&i)),
                "byte {i} differs outside a position field"
            );
        }
    }
    assert_ne!(first.digest(), second.digest());
    // A table carries no position at all: its bytes and digest are the
    // same however its rows and pairs were inserted.
    let mut reordered = FInstance::new();
    for (id, table, row) in [
        (RowId(6), "post", cell(60)),
        (RowId(1), "user", cell(10)),
        (RowId(9), "post", cell(90)),
        (RowId(4), "user", cell(40)),
    ] {
        reordered.insert_row(table, id, row).unwrap();
    }
    for (s, t) in [(9, 4), (6, 1), (6, 4)] {
        reordered
            .insert_foreign_key(author(), RowId(s), RowId(t))
            .unwrap();
    }
    assert_eq!(reordered.digest(), tables().digest());
}

/// A graph whose edge ids are not the ones the initial encoding would
/// allocate still round-trips through its table when the table's graph
/// form is taken along the embedding read off the graph.
#[test]
fn a_table_shaped_graph_with_arbitrary_edge_ids_round_trips() {
    let graph = GInstance::new()
        .with_node(Node::new(1, "user"))
        .with_node(Node::new(2, "post"))
        .with_edge_id(EdgeId(5), 2, 1, author());
    assert_eq!(graph.table_shape(&schema()), TableShapeReport::Table);
    let (tables, embedding) = graph.try_to_finstance(&schema()).unwrap();
    assert_eq!(embedding.next_edge, 6);
    let mut along = embedding.clone();
    let again = GInstance::from_finstance_along(&tables, &mut along).unwrap();
    assert!(
        GInstance::identical(&again, &graph),
        "{again:?} != {graph:?}"
    );
    assert_eq!(
        along, embedding,
        "every pair was recorded, so nothing was allocated"
    );
}

/// The largest edge id is an edge id like any other in a graph: a
/// table-shaped graph holding `EdgeId(u32::MAX)` reads back as its table and
/// round-trips. The embedding it yields has its floor saturated at
/// `u32::MAX`, from which nothing is allocated, so a new pair is refused
/// rather than given an id already in use.
#[test]
fn a_table_shaped_graph_holding_the_largest_edge_id_round_trips() {
    let graph = GInstance::new()
        .with_node(Node::new(1, "user"))
        .with_node(Node::new(2, "post"))
        .with_node(Node::new(3, "post"))
        .with_edge_id(EdgeId(u32::MAX), 2, 1, author());
    assert_eq!(graph.table_shape(&schema()), TableShapeReport::Table);
    let (tables, embedding) = graph
        .try_to_finstance(&schema())
        .expect("a graph table_shape accepts reads back as its table");
    assert_eq!(embedding.next_edge, u32::MAX);
    assert_eq!(
        embedding.pairs.get(&(author(), RowId(2), RowId(1))),
        Some(&EdgeId(u32::MAX))
    );
    let mut along = embedding.clone();
    let again = GInstance::from_finstance_along(&tables, &mut along).unwrap();
    assert!(
        GInstance::identical(&again, &graph),
        "{again:?} != {graph:?}"
    );
    assert_eq!(along, embedding);
    assert!(
        matches!(
            along.edge_for(&author(), RowId(3), RowId(1)),
            Err(InstError::EdgeIdsExhausted)
        ),
        "the floor is saturated, so no edge id is left"
    );
    assert_eq!(along, embedding, "a refused allocation changes nothing");

    // One below the largest id: the floor lands on `u32::MAX` exactly, and
    // the round trip still holds.
    let below = GInstance::new()
        .with_node(Node::new(1, "user"))
        .with_node(Node::new(2, "post"))
        .with_edge_id(EdgeId(u32::MAX - 1), 2, 1, author());
    let (tables, embedding) = below.try_to_finstance(&schema()).unwrap();
    assert_eq!(embedding.next_edge, u32::MAX);
    let mut along = embedding.clone();
    let again = GInstance::from_finstance_along(&tables, &mut along).unwrap();
    assert!(GInstance::identical(&again, &below));
    assert_eq!(
        along, embedding,
        "every pair is recorded, so nothing is allocated"
    );
}

/// Deciding the table shape does work per node and per edge only: a
/// schema declaring many required foreign keys out of tables the graph
/// does not populate costs nothing per node.
#[test]
fn table_shape_is_linear_in_the_graph() {
    let mut schema = Schema::default();
    for i in 0..2000 {
        let name = Name::from(format!("r{i}"));
        let edge = Edge {
            src: Name::from(format!("s{i}")),
            tgt: "t".into(),
            kind: "fk".into(),
            name: Some(name.clone()),
        };
        schema.edges.insert(edge.clone(), name.clone());
        schema.declarations.references.insert(
            name.clone(),
            Reference {
                id: name,
                holder: edge.src.clone(),
                source: RefSource::KeyValue { parts: Vec::new() },
                target_vertex: "t".into(),
                target_key: None,
                edge: Some(edge),
                cardinality: Cardinality::One,
                nullable: false,
                on_unresolved: OnUnresolved::Error,
                mode: ResolveMode::AddEdge,
            },
        );
    }
    let mut graph = GInstance::new();
    for id in 0..20_000 {
        graph.insert_node(Node::new(id, "t")).unwrap();
    }
    let start = std::time::Instant::now();
    assert_eq!(graph.table_shape(&schema), TableShapeReport::Table);
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "table_shape took {elapsed:?}"
    );
}

/// A table instance over `user` and `post` with arbitrary distinct row
/// ids and arbitrary pairs.
fn arb_tables() -> impl Strategy<Value = FInstance> {
    (
        proptest::collection::btree_set(0_u32..64, 0..12),
        proptest::collection::vec(any::<bool>(), 12),
        proptest::collection::vec((any::<u8>(), any::<u8>()), 0..16),
    )
        .prop_map(|(ids, is_post, links)| {
            let mut f = FInstance::new();
            for (k, id) in ids.iter().enumerate() {
                let table = if is_post[k] { "post" } else { "user" };
                f.insert_row(table, RowId(*id), cell(i64::from(*id)))
                    .unwrap();
            }
            let posts = f
                .table("post")
                .map(panproto_inst::Table::ids)
                .unwrap_or_default();
            let users = f
                .table("user")
                .map(panproto_inst::Table::ids)
                .unwrap_or_default();
            if !posts.is_empty() && !users.is_empty() {
                for (a, b) in links {
                    let src = posts[usize::from(a) % posts.len()];
                    let tgt = users[usize::from(b) % users.len()];
                    if !f.contains_pair(&author(), src, tgt) {
                        f.insert_foreign_key(author(), src, tgt).unwrap();
                    }
                }
            }
            f
        })
}

/// A table-shaped graph over `user` and `post` whose edges carry arbitrary
/// distinct ids, not the ones the initial encoding allocates.
fn arb_table_graph() -> impl Strategy<Value = GInstance> {
    (arb_tables(), proptest::collection::vec(0_u32..1000, 16)).prop_map(|(f, ids)| {
        let mut graph = GInstance::new();
        for (table, rows) in f.tables() {
            for (id, row) in rows.rows() {
                let mut node = Node::new(id.0, table.clone());
                node.extra_fields.clone_from(row);
                graph.insert_node(node).unwrap();
            }
        }
        let mut used = BTreeSet::new();
        for (k, (edge, src, tgt)) in f.all_pairs().into_iter().enumerate() {
            // Distinct ids: a drawn id already used is moved past the
            // largest drawn so far.
            let mut id = ids[k % ids.len()];
            if !used.insert(id) {
                id = used.last().copied().unwrap_or(0) + 1;
                used.insert(id);
            }
            graph
                .insert_edge(EdgeId(id), GEdge::new(src.0, tgt.0, edge))
                .unwrap();
        }
        graph
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn the_table_graph_round_trip_is_a_bijection(f in arb_tables()) {
        let (graph, embedding) = GInstance::from_finstance(&f).unwrap();
        prop_assert!(embedding.check().is_ok());
        // Rows correspond one to one with nodes, pairs with edge ids.
        let nodes: Vec<u32> = {
            let mut ids: Vec<u32> = graph.nodes().keys().copied().collect();
            ids.sort_unstable();
            ids
        };
        prop_assert_eq!(nodes, f.row_ids().into_iter().map(|r| r.0).collect::<Vec<_>>());
        let pairs: Vec<ForeignKeyPairOf> = embedding.pairs.keys().cloned().collect();
        prop_assert_eq!(pairs, f.all_pairs());
        let edge_ids: BTreeSet<EdgeId> = embedding.pairs.values().copied().collect();
        prop_assert_eq!(edge_ids.len(), graph.edge_count());
        let (back, back_embedding) = graph.try_to_finstance(&schema()).unwrap();
        prop_assert_eq!(back, f);
        prop_assert_eq!(back_embedding, embedding);
    }

    #[test]
    fn a_table_shaped_graph_round_trips_through_its_table(graph in arb_table_graph()) {
        prop_assert_eq!(graph.table_shape(&schema()), TableShapeReport::Table);
        let (tables, embedding) = graph.try_to_finstance(&schema()).unwrap();
        let mut along = embedding.clone();
        let again = GInstance::from_finstance_along(&tables, &mut along).unwrap();
        prop_assert!(GInstance::identical(&again, &graph));
        prop_assert_eq!(along, embedding);
    }

    #[test]
    fn table_embedding_next_edge_never_decreases(
        f in arb_tables(),
        extra in proptest::collection::vec((0_u32..64, 0_u32..64), 0..12),
    ) {
        let (_, mut embedding) = GInstance::from_finstance(&f).unwrap();
        let mut floor = embedding.next_edge;
        for (s, t) in extra {
            let before = embedding.pairs.len();
            let id = embedding.edge_for(&author(), RowId(s), RowId(t)).unwrap();
            prop_assert!(embedding.next_edge >= floor);
            if embedding.pairs.len() > before {
                prop_assert_eq!(id, EdgeId(floor));
            }
            floor = embedding.next_edge;
        }
    }
}

type ForeignKeyPairOf = panproto_inst::ForeignKeyPair;

fn post_edge(tgt: &str, kind: &str, name: &str) -> Edge {
    Edge {
        src: "post".into(),
        tgt: tgt.into(),
        kind: kind.into(),
        name: Some(name.into()),
    }
}

/// A table instance over `user` and `post` with arbitrary distinct row ids,
/// string and integer cells, and arbitrary pairs of three foreign keys, two
/// of which (`author` and `editor`) share the kind `fk`.
fn arb_linked_tables() -> impl Strategy<Value = FInstance> {
    (
        proptest::collection::btree_set(0_u32..48, 0..14),
        proptest::collection::vec(any::<bool>(), 14),
        proptest::collection::vec((0_u8..3, any::<u8>(), any::<u8>()), 0..24),
    )
        .prop_map(|(ids, is_post, links)| {
            let mut f = FInstance::new();
            for (k, id) in ids.iter().enumerate() {
                let table = if is_post[k] { "post" } else { "user" };
                let mut row = cell(i64::from(*id));
                row.insert("s".to_owned(), Value::Str(format!("r{id}")));
                f.insert_row(table, RowId(*id), row).unwrap();
            }
            let ids_of = |f: &FInstance, t: &str| {
                f.table(t)
                    .map(panproto_inst::Table::ids)
                    .unwrap_or_default()
            };
            let (posts, users) = (ids_of(&f, "post"), ids_of(&f, "user"));
            for (which, a, b) in links {
                let (edge, targets) = match which {
                    0 => (post_edge("user", "fk", "author"), &users),
                    1 => (post_edge("user", "fk", "editor"), &users),
                    _ => (post_edge("post", "reply", "reply"), &posts),
                };
                if posts.is_empty() || targets.is_empty() {
                    continue;
                }
                let src = posts[usize::from(a) % posts.len()];
                let tgt = targets[usize::from(b) % targets.len()];
                if !f.contains_pair(&edge, src, tgt) {
                    f.insert_foreign_key(edge, src, tgt).unwrap();
                }
            }
            f
        })
}

/// An expression environment as its bindings in ascending name.
fn bindings(env: &panproto_expr::Env) -> Vec<(String, panproto_expr::Literal)> {
    let mut all: Vec<(String, panproto_expr::Literal)> = env
        .iter()
        .map(|(name, value)| (name.to_string(), value.clone()))
        .collect();
    all.sort_by(|a, b| a.0.cmp(&b.0));
    all
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Row ids are node ids, so every element operation reads the same on a
    /// table and on its graph form: fibres, pushforwards (which count paths
    /// on both), stalks, attributes, sorts, values and graph builtins.
    #[test]
    fn element_operations_agree_on_a_table_and_its_graph_form(
        f in arb_linked_tables(),
        elements in proptest::collection::vec(0_u32..50, 0..12),
    ) {
        use panproto_expr::{BuiltinOp, Literal};
        use panproto_inst::ElementOps;

        let (graph, _) = GInstance::from_finstance(&f).unwrap();
        for vertex in ["user", "post", "none"] {
            let vertex = Name::from(vertex);
            prop_assert_eq!(f.fiber(&vertex), graph.fiber(&vertex));
        }
        for kind in ["fk", "reply", "none"] {
            let kind = Name::from(kind);
            prop_assert_eq!(
                f.pushforward(&elements, &kind),
                graph.pushforward(&elements, &kind),
                "pushforward along {} of {:?}", kind, elements
            );
        }
        for id in 0_u32..50 {
            prop_assert_eq!(f.sort(id), graph.sort(id));
            prop_assert_eq!(f.element_value(id), graph.element_value(id));
            prop_assert_eq!(f.attributes(id), graph.attributes(id));
            prop_assert_eq!(bindings(&f.stalk(id)), bindings(&graph.stalk(id)), "stalk of {}", id);
            let node = Literal::Int(i64::from(id));
            for kind in ["fk", "reply", "none"] {
                let with_kind = [node.clone(), Literal::Str(kind.into())];
                for op in [BuiltinOp::Edge, BuiltinOp::Edges, BuiltinOp::HasEdge] {
                    prop_assert_eq!(
                        f.eval_graph_builtin(op, &with_kind, None).unwrap(),
                        graph.eval_graph_builtin(op, &with_kind, None).unwrap(),
                        "{:?} of {} along {}", op, id, kind
                    );
                }
            }
            for op in [BuiltinOp::Children, BuiltinOp::EdgeCount, BuiltinOp::Anchor] {
                prop_assert_eq!(
                    f.eval_graph_builtin(op, std::slice::from_ref(&node), None).unwrap(),
                    graph.eval_graph_builtin(op, std::slice::from_ref(&node), None).unwrap(),
                    "{:?} of {}", op, id
                );
            }
        }
    }
}

/// The two paths from post 1 to user 0 (along `author` and along `editor`,
/// both of kind `fk`) each reach it, on the table as on its graph form, and a
/// source listed twice is followed twice.
#[test]
fn a_table_pushforward_counts_paths_as_its_graph_form_does() {
    use panproto_inst::ElementOps;

    let f = FInstance::new()
        .with_rows("user", [(RowId(0), cell(0))])
        .and_then(|f| f.with_rows("post", [(RowId(1), cell(1)), (RowId(2), cell(2))]))
        .and_then(|f| {
            f.with_foreign_key(
                post_edge("user", "fk", "author"),
                [(RowId(1), RowId(0)), (RowId(2), RowId(0))],
            )
        })
        .and_then(|f| f.with_foreign_key(post_edge("user", "fk", "editor"), [(RowId(1), RowId(0))]))
        .unwrap();
    let (graph, _) = GInstance::from_finstance(&f).unwrap();
    let kind = Name::from("fk");
    assert_eq!(f.pushforward(&[1, 2], &kind), vec![0, 0, 0]);
    assert_eq!(f.pushforward(&[2, 2], &kind), vec![0, 0]);
    assert_eq!(
        f.pushforward(&[1, 2], &kind),
        graph.pushforward(&[1, 2], &kind)
    );
    assert_eq!(
        f.pushforward(&[2, 2], &kind),
        graph.pushforward(&[2, 2], &kind)
    );
}

/// A table over `user` whose rows may hold, besides the flat cell `n`, a
/// list cell `tags` and a nested record cell `meta`.
fn arb_tables_with_nested_cells() -> impl Strategy<Value = FInstance> {
    proptest::collection::btree_map(0_u32..32, (any::<bool>(), any::<bool>()), 0..10).prop_map(
        |rows| {
            let mut f = FInstance::new();
            for (id, (list, record)) in rows {
                let mut row = cell(i64::from(id));
                if list {
                    row.insert("tags".to_owned(), Value::List(vec![Value::Int(1)]));
                }
                if record {
                    row.insert(
                        "meta".to_owned(),
                        Value::Unknown(HashMap::from([("k".to_owned(), Value::Int(2))])),
                    );
                }
                f.insert_row("user", RowId(id), row).unwrap();
            }
            f
        },
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// A cell may hold a list or a nested record, and the graph form keeps
    /// it as a node field, so rows still correspond one to one with nodes.
    /// But a node field holding one is not a flat cell, so such a graph is
    /// not table-shaped: the round trip back to the table holds exactly when
    /// every cell is flat, and otherwise fails naming the least row holding
    /// a nested cell and its least such column.
    #[test]
    fn the_table_round_trip_holds_exactly_on_flat_cells(f in arb_tables_with_nested_cells()) {
        let (graph, _) = GInstance::from_finstance(&f).unwrap();
        let nodes: BTreeSet<u32> = graph.nodes().keys().copied().collect();
        let rows: BTreeSet<u32> = f.row_ids().into_iter().map(|r| r.0).collect();
        prop_assert_eq!(nodes, rows);
        let nested = f.row_ids().into_iter().find_map(|id| {
            let row = f.row(id)?;
            ["meta", "tags"]
                .into_iter()
                .find(|column| row.contains_key(*column))
                .map(|column| (id.0, column.to_owned()))
        });
        match (graph.try_to_finstance(&Schema::default()), nested) {
            (Ok((back, _)), None) => prop_assert_eq!(back, f),
            (Err(InstError::NotATable { reason }), Some((node, field))) => {
                prop_assert_eq!(reason, NotTableReason::NestedCell { node, field });
            }
            (other, expected) => {
                prop_assert!(false, "read back {:?}, expected a nested cell at {:?}", other.map(|_| ()), expected);
            }
        }
    }
}
