#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Relational graph Kan examples, refusals, and finite laws.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_inst::{
    AdjunctionError, CellLayout, CompiledMigration, EdgeId, EdgeLayout, EncodingSide, FInstance,
    FieldPresence, GInstance, GraphKanError, GraphKanHom, GraphLayout, MigrationFunctor, Node,
    NodeShape, NotEncodableReason, RelSchema, RelSchemaError, RestrictError, Row, RowId, Value,
    WInstance, decode_graph, encode_graph, g_counit, g_delta, g_delta_along,
    g_delta_with_provenance, g_pi, g_pi_along, g_pi_along_with_provenance, g_pi_counit,
    g_pi_restricted, g_pi_transpose_left, g_pi_transpose_right, g_pi_unit, g_sigma_along,
    g_sigma_along_with_provenance, g_sigma_restricted, g_transpose_left, g_transpose_right, g_unit,
    wtype_extend_engine, wtype_pi,
};
use panproto_schema::{Edge, Protocol, ScalarKind, Schema, Vertex};
use proptest::prelude::*;
use smallvec::SmallVec;

fn edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

fn schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut edge_map = HashMap::new();
    for edge in edges {
        between
            .entry((edge.src.clone(), edge.tgt.clone()))
            .or_default()
            .push(edge.clone());
        outgoing
            .entry(edge.src.clone())
            .or_default()
            .push(edge.clone());
        incoming
            .entry(edge.tgt.clone())
            .or_default()
            .push(edge.clone());
        edge_map.insert(edge.clone(), edge.kind.clone());
    }
    Schema {
        protocol: "test".into(),
        vertices: vertices
            .iter()
            .map(|&(id, kind)| {
                (
                    Name::from(id),
                    Vertex {
                        id: id.into(),
                        kind: kind.into(),
                        nsid: None,
                    },
                )
            })
            .collect(),
        edges: edge_map,
        outgoing,
        incoming,
        between,
        ..Schema::default()
    }
}

fn protocol() -> Protocol {
    Protocol {
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        value_kinds: BTreeMap::from([
            ("str".to_owned(), ScalarKind::Str),
            (
                "int".to_owned(),
                ScalarKind::Int {
                    bits: 64,
                    signed: true,
                },
            ),
            ("value".to_owned(), ScalarKind::Any),
        ]),
        ..Protocol::default()
    }
}

fn migration(
    vertices: &[(&str, &str)],
    edges: &[(Edge, Edge)],
    surviving_vertices: &[&str],
    surviving_edges: &[Edge],
) -> CompiledMigration {
    CompiledMigration {
        vertex_remap: vertices
            .iter()
            .map(|(source, target)| (Name::from(*source), Name::from(*target)))
            .collect(),
        edge_remap: edges.iter().cloned().collect(),
        surviving_verts: surviving_vertices
            .iter()
            .map(|id| Name::from(*id))
            .collect(),
        surviving_edges: surviving_edges.iter().cloned().collect(),
        ..CompiledMigration::default()
    }
}

fn row(cells: &[(&str, Value)]) -> Row {
    cells
        .iter()
        .map(|(column, value)| ((*column).to_owned(), value.clone()))
        .collect()
}

fn one_cell_schema() -> (Schema, Edge) {
    let name = edge("person", "person.name", "name");
    (
        schema(
            &[("person", "record"), ("person.name", "str")],
            std::slice::from_ref(&name),
        ),
        name,
    )
}

fn one_cell_graph(value: Value) -> GInstance {
    let (_, name) = one_cell_schema();
    GInstance::new()
        .with_node(Node::new(0, "person"))
        .with_node(Node::new(1, "person.name").with_value(FieldPresence::Present(value)))
        .with_edge_id(EdgeId(2), 0, 1, name)
}

fn reason(graph: &GInstance, schema: &Schema) -> NotEncodableReason {
    let rel = RelSchema::new(schema, &protocol()).unwrap();
    match encode_graph(graph, EncodingSide::Whole(&rel)).unwrap_err() {
        GraphKanError::NotEncodable { reason } => reason,
        other => panic!("expected an encoding refusal, got {other:?}"),
    }
}

fn naive_fold(graph: &GInstance, root: u32) -> Option<Value> {
    fn fold(graph: &GInstance, node: u32, visiting: &mut BTreeSet<u32>) -> Option<Value> {
        if !visiting.insert(node) {
            return None;
        }
        let current = graph.node(node)?;
        let mut outgoing: Vec<_> = graph
            .out_edges(node)
            .iter()
            .filter_map(|id| graph.edge(*id).map(|edge| (*id, edge)))
            .collect();
        let value = if outgoing.is_empty() {
            match current.value.as_ref()? {
                FieldPresence::Present(value) => value.clone(),
                FieldPresence::Null => Value::Null,
                FieldPresence::Absent => return None,
            }
        } else if outgoing.iter().all(|(_, edge)| edge.position.is_some()) {
            outgoing.sort_unstable_by_key(|(id, edge)| (edge.position, *id));
            Value::List(
                outgoing
                    .into_iter()
                    .map(|(_, edge)| fold(graph, edge.tgt, visiting))
                    .collect::<Option<Vec<_>>>()?,
            )
        } else {
            let fields = outgoing
                .into_iter()
                .map(|(_, edge)| {
                    let name = edge.edge.name.as_ref()?.as_ref().to_owned();
                    Some((name, fold(graph, edge.tgt, visiting)?))
                })
                .collect::<Option<HashMap<_, _>>>()?;
            Value::Unknown(fields)
        };
        visiting.remove(&node);
        Some(value)
    }
    fold(graph, root, &mut BTreeSet::new())
}

#[test]
fn decoding_inverts_encoding_with_structured_layout() {
    let item = edge("person.tags", "person.tag", "item");
    let tags = edge("person", "person.tags", "tags");
    let schema = schema(
        &[
            ("person", "record"),
            ("person.tags", "value"),
            ("person.tag", "str"),
        ],
        &[tags.clone(), item.clone()],
    );
    let mut table = Node::new(7, "person");
    table.discriminator = Some("person".into());
    table.position = Some(9);
    table.annotations.insert("source".into(), Value::Int(3));
    table
        .extra_fields
        .insert("opaque".into(), Value::Bool(true));
    let mut list = Node::new(3, "person.tags").with_shape(NodeShape::List);
    list.annotations
        .insert("layout".into(), Value::Str("kept".into()));
    let graph = GInstance::new()
        .with_node(table)
        .with_node(list)
        .with_node(
            Node::new(11, "person.tag").with_value(FieldPresence::Present(Value::Str("a".into()))),
        )
        .with_node(
            Node::new(12, "person.tag").with_value(FieldPresence::Present(Value::Str("b".into()))),
        )
        .with_edge_id(EdgeId(20), 7, 3, tags)
        .with_edge_id(EdgeId(21), 3, 11, item.clone())
        .with_edge_id(EdgeId(22), 3, 12, item);
    let mut graph = graph;
    graph.set_edge_position(EdgeId(20), Some(4)).unwrap();
    graph.set_edge_position(EdgeId(21), Some(0)).unwrap();
    graph.set_edge_position(EdgeId(22), Some(1)).unwrap();
    let rel = RelSchema::new(&schema, &protocol()).unwrap();
    let (tables, layout) = encode_graph(&graph, EncodingSide::Whole(&rel)).unwrap();
    let decoded = decode_graph(&tables, EncodingSide::Whole(&rel), &layout).unwrap();
    assert!(GInstance::identical(&graph, &decoded));
}

#[test]
fn encoding_inverts_decoding_with_record_free_layout() {
    let (schema, _) = one_cell_schema();
    let rel = RelSchema::new(&schema, &protocol()).unwrap();
    let value = Value::Unknown(HashMap::from([
        ("first".to_owned(), Value::Int(1)),
        (
            "second".to_owned(),
            Value::List(vec![Value::Bool(false), Value::Null]),
        ),
    ]));
    let tables = FInstance::new()
        .with_rows("person", [(RowId(8), row(&[("name", value)]))])
        .unwrap();
    let graph = decode_graph(&tables, EncodingSide::Whole(&rel), &GraphLayout::default()).unwrap();
    let (again, _) = encode_graph(&graph, EncodingSide::Whole(&rel)).unwrap();
    assert_eq!(again, tables);
}

#[test]
fn encoding_pass_one_orders_fans_before_unknown_anchors() {
    let (schema, _) = one_cell_schema();
    let mut fan = Node::new(0, "outside");
    fan.shape = NodeShape::HyperEdge;
    let graph = GInstance::new().with_node(fan);
    assert_eq!(
        reason(&graph, &schema),
        NotEncodableReason::FanNode { node: 0 }
    );

    let graph = GInstance::new().with_node(Node::new(2, "outside"));
    assert_eq!(
        reason(&graph, &schema),
        NotEncodableReason::AnchorOutsideSide { node: 2 }
    );
}

#[test]
fn encoding_pass_two_orders_unknown_labels_before_bad_endpoints() {
    let (schema, name) = one_cell_schema();
    let outside = edge("person", "person.name", "outside");
    let graph = GInstance::new()
        .with_node(Node::new(0, "person"))
        .with_node(
            Node::new(1, "person.name").with_value(FieldPresence::Present(Value::Str("a".into()))),
        )
        .with_edge_id(EdgeId(3), 0, 1, outside);
    assert_eq!(
        reason(&graph, &schema),
        NotEncodableReason::EdgeOutsideSide { edge: EdgeId(3) }
    );

    let graph = GInstance::new()
        .with_node(Node::new(0, "person.name"))
        .with_node(Node::new(1, "person"))
        .with_edge_id(EdgeId(4), 0, 1, name);
    assert_eq!(
        reason(&graph, &schema),
        NotEncodableReason::IncoherentEndpoints { edge: EdgeId(4) }
    );
}

#[test]
fn parallel_edges_are_not_graph_kan_encodable() {
    let relation = edge("a", "b", "f");
    let relation_schema = schema(
        &[("a", "record"), ("b", "record")],
        std::slice::from_ref(&relation),
    );
    let graph = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "b"))
        .with_edge_id(EdgeId(4), 0, 1, relation.clone())
        .with_edge_id(EdgeId(7), 0, 1, relation);
    assert_eq!(
        reason(&graph, &relation_schema),
        NotEncodableReason::ParallelEdges {
            edges: vec![EdgeId(4), EdgeId(7)]
        }
    );

    let first = edge("a", "b", "z");
    let later = edge("a", "b", "a");
    let schema = schema(
        &[("a", "record"), ("b", "record")],
        &[first.clone(), later.clone()],
    );
    let graph = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "b"))
        .with_edge_id(EdgeId(1), 0, 1, first.clone())
        .with_edge_id(EdgeId(2), 0, 1, first)
        .with_edge_id(EdgeId(8), 0, 1, later.clone())
        .with_edge_id(EdgeId(9), 0, 1, later);
    assert_eq!(
        reason(&graph, &schema),
        NotEncodableReason::ParallelEdges {
            edges: vec![EdgeId(1), EdgeId(2)]
        }
    );
}

#[test]
fn encoding_pass_four_distinguishes_missing_extra_and_repeated_cells() {
    let (schema, name) = one_cell_schema();
    let missing = GInstance::new().with_node(Node::new(0, "person"));
    assert_eq!(
        reason(&missing, &schema),
        NotEncodableReason::MissingCell {
            node: 0,
            column: "name".into()
        }
    );
    let extra = GInstance::new()
        .with_node(Node::new(0, "person").with_extra_field("name", Value::Str("a".into())));
    assert_eq!(
        reason(&extra, &schema),
        NotEncodableReason::CellAsExtraField {
            node: 0,
            column: "name".into()
        }
    );
    let repeated = GInstance::new()
        .with_node(Node::new(0, "person"))
        .with_node(
            Node::new(1, "person.name").with_value(FieldPresence::Present(Value::Str("a".into()))),
        )
        .with_node(
            Node::new(2, "person.name").with_value(FieldPresence::Present(Value::Str("b".into()))),
        )
        .with_edge_id(EdgeId(2), 0, 1, name.clone())
        .with_edge_id(EdgeId(3), 0, 2, name);
    assert_eq!(
        reason(&repeated, &schema),
        NotEncodableReason::RepeatedCell {
            node: 0,
            column: "name".into(),
            edges: vec![EdgeId(2), EdgeId(3)]
        }
    );
}

#[test]
fn encoding_pass_five_names_shared_unrooted_missing_and_nested_values() {
    let (cell_schema, name) = one_cell_schema();
    let shared = GInstance::new()
        .with_node(Node::new(0, "person"))
        .with_node(Node::new(2, "person"))
        .with_node(
            Node::new(1, "person.name").with_value(FieldPresence::Present(Value::Str("a".into()))),
        )
        .with_edge_id(EdgeId(4), 0, 1, name.clone())
        .with_edge_id(EdgeId(5), 2, 1, name.clone());
    assert_eq!(
        reason(&shared, &cell_schema),
        NotEncodableReason::SharedValue {
            node: 1,
            edges: vec![EdgeId(4), EdgeId(5)]
        }
    );

    let unrooted = GInstance::new().with_node(
        Node::new(1, "person.name").with_value(FieldPresence::Present(Value::Str("a".into()))),
    );
    assert_eq!(
        reason(&unrooted, &cell_schema),
        NotEncodableReason::UnrootedValue { node: 1 }
    );

    let missing = GInstance::new()
        .with_node(Node::new(0, "person"))
        .with_node(Node::new(1, "person.name"))
        .with_edge_id(EdgeId(2), 0, 1, name.clone());
    assert_eq!(
        reason(&missing, &cell_schema),
        NotEncodableReason::MissingValue { node: 1 }
    );
    let absent = GInstance::new()
        .with_node(Node::new(0, "person"))
        .with_node(Node::new(1, "person.name").with_value(FieldPresence::Absent))
        .with_edge_id(EdgeId(2), 0, 1, name.clone());
    assert_eq!(
        reason(&absent, &cell_schema),
        NotEncodableReason::MissingValue { node: 1 }
    );

    let nested = GInstance::new()
        .with_node(Node::new(0, "person"))
        .with_node(
            Node::new(1, "person.name").with_value(FieldPresence::Present(Value::List(vec![
                Value::LabeledNull(4),
            ]))),
        )
        .with_edge_id(EdgeId(2), 0, 1, name.clone());
    assert_eq!(
        reason(&nested, &cell_schema),
        NotEncodableReason::NestedLabeledNull { node: 1 }
    );

    let loop_edge = edge("person.name", "person.name", "next");
    let cyclic_schema = schema(
        &[("person", "record"), ("person.name", "str")],
        &[name.clone(), loop_edge.clone()],
    );
    let cycle = GInstance::new()
        .with_node(Node::new(0, "person"))
        .with_node(Node::new(1, "person.name"))
        .with_node(Node::new(2, "person.name"))
        .with_node(
            Node::new(3, "person.name")
                .with_value(FieldPresence::Present(Value::Str("rooted".into()))),
        )
        .with_edge_id(EdgeId(2), 0, 3, name)
        .with_edge_id(EdgeId(4), 1, 2, loop_edge.clone())
        .with_edge_id(EdgeId(5), 2, 1, loop_edge);
    assert_eq!(
        reason(&cycle, &cyclic_schema),
        NotEncodableReason::UnrootedValue { node: 1 }
    );
}

#[test]
fn encoding_pass_five_refuses_incoherent_inner_value_shapes() {
    let values = edge("person", "person.values", "values");
    let item = edge("person.values", "person.item", "item");
    let schema = schema(
        &[
            ("person", "record"),
            ("person.values", "value"),
            ("person.item", "str"),
        ],
        &[values.clone(), item.clone()],
    );
    let graph = GInstance::new()
        .with_node(Node::new(0, "person"))
        .with_node(Node::new(1, "person.values"))
        .with_node(
            Node::new(2, "person.item").with_value(FieldPresence::Present(Value::Str("a".into()))),
        )
        .with_node(
            Node::new(3, "person.item").with_value(FieldPresence::Present(Value::Str("b".into()))),
        )
        .with_edge_id(EdgeId(4), 0, 1, values)
        .with_edge_id(EdgeId(5), 1, 2, item.clone())
        .with_edge_id(EdgeId(6), 1, 3, item);
    let mut graph = graph;
    graph.set_edge_position(EdgeId(5), Some(0)).unwrap();
    assert_eq!(
        reason(&graph, &schema),
        NotEncodableReason::InnerValueShape { node: 1 }
    );

    let nested = GInstance::new()
        .with_node(Node::new(0, "person"))
        .with_node(Node::new(1, "person.values"))
        .with_node(
            Node::new(2, "person.item").with_value(FieldPresence::Present(Value::LabeledNull(9))),
        )
        .with_edge_id(EdgeId(4), 0, 1, edge("person", "person.values", "values"))
        .with_edge_id(
            EdgeId(5),
            1,
            2,
            edge("person.values", "person.item", "item"),
        );
    assert_eq!(
        reason(&nested, &schema),
        NotEncodableReason::NestedLabeledNull { node: 2 }
    );
}

#[test]
fn graph_kan_hom_check_agrees_with_the_graph_only_oracle() {
    let (cell_schema, _) = one_cell_schema();
    let rel = RelSchema::new(&cell_schema, &protocol()).unwrap();
    let ann = one_cell_graph(Value::Str("Ann".into()));
    let ann_again = one_cell_graph(Value::Str("Ann".into()));
    let bob = one_cell_graph(Value::Str("Bob".into()));
    let hom = GraphKanHom {
        node_map: BTreeMap::from([(0, 0)]),
        null_map: BTreeMap::new(),
    };
    assert!(
        hom.check(&ann, &ann_again, EncodingSide::Whole(&rel))
            .is_ok()
    );
    assert!(matches!(
        hom.check(&ann, &bob, EncodingSide::Whole(&rel)),
        Err(GraphKanError::Hom(
            panproto_inst::HomError::AttributeMismatch { src: 0, tgt: 0 }
        ))
    ));

    let name = edge("p", "p.name", "name");
    let friend = edge("p", "p", "friend");
    let oracle_schema = schema(
        &[("p", "record"), ("p.name", "str")],
        &[name.clone(), friend.clone()],
    );
    let oracle_rel = RelSchema::new(&oracle_schema, &protocol()).unwrap();
    let domain = GInstance::new()
        .with_node(Node::new(0, "p"))
        .with_node(
            Node::new(1, "p.name").with_value(FieldPresence::Present(Value::Str("Ann".into()))),
        )
        .with_node(Node::new(2, "p"))
        .with_node(
            Node::new(3, "p.name").with_value(FieldPresence::Present(Value::Str("Bob".into()))),
        )
        .with_edge_id(EdgeId(10), 0, 1, name.clone())
        .with_edge_id(EdgeId(11), 2, 3, name.clone())
        .with_edge_id(EdgeId(12), 0, 2, friend.clone());
    let codomain = GInstance::new()
        .with_node(Node::new(7, "p"))
        .with_node(
            Node::new(8, "p.name").with_value(FieldPresence::Present(Value::Str("Ann".into()))),
        )
        .with_node(Node::new(9, "p"))
        .with_node(
            Node::new(10, "p.name").with_value(FieldPresence::Present(Value::Str("Bob".into()))),
        )
        .with_edge_id(EdgeId(20), 7, 8, name.clone())
        .with_edge_id(EdgeId(21), 9, 10, name.clone())
        .with_edge_id(EdgeId(22), 7, 9, friend.clone());
    let candidates = [7, 9]
        .into_iter()
        .flat_map(|left| {
            [7, 9].into_iter().map(move |right| GraphKanHom {
                node_map: BTreeMap::from([(0, left), (2, right)]),
                null_map: BTreeMap::new(),
            })
        })
        .collect::<Vec<_>>();
    let direct_oracle = |candidate: &GraphKanHom| {
        [0, 2].into_iter().all(|source| {
            let Some(&target) = candidate.node_map.get(&source) else {
                return false;
            };
            let source_cell = domain
                .out_edges(source)
                .iter()
                .find_map(|id| (domain.edge(*id)?.edge == name).then_some(domain.edge(*id)?.tgt));
            let target_cell = codomain.out_edges(target).iter().find_map(|id| {
                (codomain.edge(*id)?.edge == name).then_some(codomain.edge(*id)?.tgt)
            });
            source_cell
                .zip(target_cell)
                .is_some_and(|(source, target)| {
                    naive_fold(&domain, source) == naive_fold(&codomain, target)
                })
        }) && codomain.edges().values().any(|edge| {
            edge.edge == friend
                && candidate.node_map.get(&0) == Some(&edge.src)
                && candidate.node_map.get(&2) == Some(&edge.tgt)
        })
    };
    assert_eq!(
        candidates.iter().filter(|hom| direct_oracle(hom)).count(),
        1
    );
    for candidate in candidates {
        assert_eq!(
            direct_oracle(&candidate),
            candidate
                .check(&domain, &codomain, EncodingSide::Whole(&oracle_rel))
                .is_ok()
        );
    }
}

fn merge_tables_functor() -> (MigrationFunctor, GInstance, GInstance) {
    let source = schema(&[("a", "record"), ("b", "record")], &[]);
    let target = schema(&[("t", "record")], &[]);
    let migration = migration(&[("a", "t"), ("b", "t")], &[], &[], &[]);
    let functor =
        MigrationFunctor::new(&source, &protocol(), &target, &protocol(), &migration).unwrap();
    let source_graph = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "b"));
    let target_graph = GInstance::new()
        .with_node(Node::new(7, "t"))
        .with_node(Node::new(9, "t"));
    (functor, source_graph, target_graph)
}

#[test]
fn graph_sigma_is_left_adjoint_to_graph_delta_on_merge_fixture() {
    let (functor, source, target) = merge_tables_functor();
    let sigma = g_sigma_along(&source, &functor).unwrap();
    let delta = g_delta_along(&target, &functor).unwrap();
    let unit = g_unit(&source, &functor).unwrap();
    let counit = g_counit(&target, &functor).unwrap();
    unit.check(
        &source,
        &g_delta_along(&sigma, &functor).unwrap(),
        EncodingSide::Domain(&functor),
    )
    .unwrap();
    counit
        .check(
            &g_sigma_along(&delta, &functor).unwrap(),
            &target,
            EncodingSide::Whole(functor.target()),
        )
        .unwrap();

    for left in [7, 9] {
        for right in [7, 9] {
            let hom = GraphKanHom {
                node_map: BTreeMap::from([(0, left), (1, right)]),
                null_map: BTreeMap::new(),
            };
            hom.check(&sigma, &target, EncodingSide::Whole(functor.target()))
                .unwrap();
            let transpose = g_transpose_left(&hom, &source, &target, &functor).unwrap();
            transpose
                .check(&source, &delta, EncodingSide::Domain(&functor))
                .unwrap();
            let round = g_transpose_right(&transpose, &source, &target, &functor).unwrap();
            assert!(round.same_as(&hom));
        }
    }
}

#[test]
fn graph_delta_is_left_adjoint_to_graph_pi_on_merge_fixture() {
    let (functor, _, target) = merge_tables_functor();
    let source = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "a"))
        .with_node(Node::new(2, "b"));
    let delta = g_delta_along(&target, &functor).unwrap();
    let pi = g_pi_along(&source, &functor, 32).unwrap();
    let unit = g_pi_unit(&target, &functor, 32).unwrap();
    let counit = g_pi_counit(&source, &functor, 32).unwrap();
    unit.check(
        &target,
        &g_pi_along(&delta, &functor, 32).unwrap(),
        EncodingSide::Whole(functor.target()),
    )
    .unwrap();
    counit
        .check(
            &g_delta_along(&pi, &functor).unwrap(),
            &source,
            EncodingSide::Domain(&functor),
        )
        .unwrap();

    for image in [0, 1] {
        let hom = GraphKanHom {
            node_map: delta
                .nodes()
                .iter()
                .map(|(node, value)| (*node, if value.anchor == "a" { image } else { 2 }))
                .collect(),
            null_map: BTreeMap::new(),
        };
        hom.check(&delta, &source, EncodingSide::Domain(&functor))
            .unwrap();
        let transpose = g_pi_transpose_left(&hom, &target, &source, &functor, 32).unwrap();
        transpose
            .check(&target, &pi, EncodingSide::Whole(functor.target()))
            .unwrap();
        let round = g_pi_transpose_right(&transpose, &target, &source, &functor, 32).unwrap();
        assert!(round.same_as(&hom));
    }
}

#[test]
fn graph_delta_row_ids_do_not_collide_with_value_nodes() {
    let a_name = edge("A", "A.name", "name");
    let b_name = edge("B", "B.name", "name");
    let p_name = edge("P", "P.name", "name");
    let src = schema(
        &[
            ("A", "record"),
            ("B", "record"),
            ("A.name", "str"),
            ("B.name", "str"),
        ],
        &[a_name.clone(), b_name.clone()],
    );
    let tgt = schema(
        &[("P", "record"), ("P.name", "str")],
        std::slice::from_ref(&p_name),
    );
    let migration = migration(
        &[
            ("A", "P"),
            ("B", "P"),
            ("A.name", "P.name"),
            ("B.name", "P.name"),
        ],
        &[(a_name, p_name.clone()), (b_name, p_name.clone())],
        &[],
        &[],
    );
    let graph = GInstance::new()
        .with_node(Node::new(0, "P"))
        .with_node(
            Node::new(1, "P.name").with_value(FieldPresence::Present(Value::Str("Ann".into()))),
        )
        .with_edge_id(EdgeId(0), 0, 1, p_name);
    let (delta, provenance) =
        g_delta_with_provenance(&graph, &src, &protocol(), &tgt, &protocol(), &migration).unwrap();
    assert_eq!(provenance.table.copies.len(), 2);
    assert!(delta.node(0).is_some() && delta.node(1).is_some());
    let value_nodes: Vec<u32> = delta
        .nodes()
        .values()
        .filter(|node| node.anchor == "A.name" || node.anchor == "B.name")
        .map(|node| node.id)
        .collect();
    assert_eq!(value_nodes.len(), 2);
    assert!(value_nodes.into_iter().all(|id| id > 1));
}

#[test]
fn graph_delta_copies_a_node_per_fibre_vertex_including_shared_value_vertices() {
    let a_name = edge("A", "str", "name");
    let b_name = edge("B", "str", "name");
    let p_name = edge("P", "str", "name");
    let src = schema(
        &[("A", "record"), ("B", "record"), ("str", "str")],
        &[a_name.clone(), b_name.clone()],
    );
    let tgt = schema(
        &[("P", "record"), ("str", "str")],
        std::slice::from_ref(&p_name),
    );
    let migration = migration(
        &[("A", "P"), ("B", "P"), ("str", "str")],
        &[(a_name, p_name.clone()), (b_name, p_name.clone())],
        &[],
        &[],
    );
    let graph = GInstance::new()
        .with_node(Node::new(0, "P"))
        .with_node(Node::new(1, "str").with_value(FieldPresence::Present(Value::Str("Ann".into()))))
        .with_edge_id(EdgeId(0), 0, 1, p_name);
    let (delta, provenance) =
        g_delta_with_provenance(&graph, &src, &protocol(), &tgt, &protocol(), &migration).unwrap();
    let leaves: Vec<_> = delta
        .nodes()
        .values()
        .filter(|node| node.anchor == "str")
        .collect();
    assert_eq!(leaves.len(), 2);
    assert_ne!(leaves[0].id, leaves[1].id);
    assert!(
        leaves
            .iter()
            .all(|node| { node.value == Some(FieldPresence::Present(Value::Str("Ann".into()))) })
    );
    assert!(
        provenance.nodes.values().all(|contributors| {
            contributors.is_empty() || contributors.iter().all(|id| *id <= 1)
        })
    );
}

#[test]
fn graph_sigma_reflects_merged_relations_to_one_pair() {
    let f1 = edge("a", "b", "f1");
    let f2 = edge("a", "b", "f2");
    let f = edge("a", "b", "f");
    let src = schema(
        &[("a", "record"), ("b", "record")],
        &[f1.clone(), f2.clone()],
    );
    let tgt = schema(
        &[("a", "record"), ("b", "record")],
        std::slice::from_ref(&f),
    );
    let migration = migration(
        &[],
        &[(f1.clone(), f.clone()), (f2.clone(), f)],
        &["a", "b"],
        &[],
    );
    let graph = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "b"))
        .with_edge_id(EdgeId(3), 0, 1, f1)
        .with_edge_id(EdgeId(8), 0, 1, f2);
    let functor =
        panproto_inst::MigrationFunctor::new(&src, &protocol(), &tgt, &protocol(), &migration)
            .unwrap();
    let (sigma, provenance) = g_sigma_along_with_provenance(&graph, &functor).unwrap();
    assert_eq!(sigma.edge_count(), 1);
    let contributors = provenance.edges.values().next().unwrap();
    assert_eq!(contributors, &BTreeSet::from([EdgeId(3), EdgeId(8)]));
}

#[test]
fn graph_kan_provenance_names_every_contributor() {
    let x = edge("a", "a.x", "x");
    let y = edge("a", "a.y", "y");
    let z = edge("A", "A.z", "z");
    let source = schema(
        &[("a", "record"), ("a.x", "int"), ("a.y", "int")],
        &[x.clone(), y.clone()],
    );
    let target = schema(&[("A", "record"), ("A.z", "int")], std::slice::from_ref(&z));
    let migration = migration(
        &[("a", "A"), ("a.x", "A.z"), ("a.y", "A.z")],
        &[(x.clone(), z.clone()), (y.clone(), z)],
        &[],
        &[],
    );
    let functor =
        MigrationFunctor::new(&source, &protocol(), &target, &protocol(), &migration).unwrap();
    let graph = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "a.x").with_value(FieldPresence::Present(Value::Int(7))))
        .with_node(Node::new(2, "a.y").with_value(FieldPresence::Present(Value::Int(7))))
        .with_edge_id(EdgeId(3), 0, 1, x)
        .with_edge_id(EdgeId(4), 0, 2, y);
    let (sigma, provenance) = g_sigma_along_with_provenance(&graph, &functor).unwrap();

    assert_eq!(
        provenance.nodes.keys().copied().collect::<BTreeSet<_>>(),
        sigma.nodes().keys().copied().collect()
    );
    assert_eq!(
        provenance.edges.keys().copied().collect::<BTreeSet<_>>(),
        sigma.edges().keys().copied().collect()
    );
    let table = sigma
        .nodes()
        .values()
        .find(|node| node.anchor == "A")
        .unwrap();
    let value = sigma
        .nodes()
        .values()
        .find(|node| node.anchor == "A.z")
        .unwrap();
    let attribute = *sigma.edges().keys().next().unwrap();
    assert_eq!(provenance.nodes[&table.id], BTreeSet::from([0]));
    assert_eq!(provenance.nodes[&value.id], BTreeSet::from([1, 2]));
    assert_eq!(
        provenance.edges[&attribute],
        BTreeSet::from([EdgeId(3), EdgeId(4)])
    );
}

#[test]
fn a_graph_sigma_clash_has_no_hom_into_any_delta_candidate() {
    let x = edge("a", "a.x", "x");
    let y = edge("a", "a.y", "y");
    let z = edge("A", "A.z", "z");
    let source = schema(
        &[("a", "record"), ("a.x", "int"), ("a.y", "int")],
        &[x.clone(), y.clone()],
    );
    let target = schema(&[("A", "record"), ("A.z", "int")], std::slice::from_ref(&z));
    let migration = migration(
        &[("a", "A"), ("a.x", "A.z"), ("a.y", "A.z")],
        &[(x.clone(), z.clone()), (y.clone(), z.clone())],
        &[],
        &[],
    );
    let functor =
        MigrationFunctor::new(&source, &protocol(), &target, &protocol(), &migration).unwrap();
    let graph = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "a.x").with_value(FieldPresence::Present(Value::Int(1))))
        .with_node(Node::new(2, "a.y").with_value(FieldPresence::Present(Value::Int(2))))
        .with_edge_id(EdgeId(3), 0, 1, x)
        .with_edge_id(EdgeId(4), 0, 2, y);
    assert!(matches!(
        g_sigma_along(&graph, &functor),
        Err(GraphKanError::Adjunction(
            AdjunctionError::SigmaConstantClash { .. }
        ))
    ));

    let target_graph = GInstance::new()
        .with_node(Node::new(7, "A"))
        .with_node(Node::new(8, "A.z").with_value(FieldPresence::Present(Value::Int(1))))
        .with_node(Node::new(9, "A"))
        .with_node(Node::new(10, "A.z").with_value(FieldPresence::Present(Value::Int(2))))
        .with_edge_id(EdgeId(11), 7, 8, z.clone())
        .with_edge_id(EdgeId(12), 9, 10, z);
    let delta = g_delta_along(&target_graph, &functor).unwrap();
    for image in [7, 9] {
        let hom = GraphKanHom {
            node_map: BTreeMap::from([(0, image)]),
            null_map: BTreeMap::new(),
        };
        assert!(
            hom.check(&graph, &delta, EncodingSide::Domain(&functor))
                .is_err()
        );
    }
}

#[test]
fn graph_pi_creates_the_all_pairs_relation() {
    let source = schema(&[("a", "record"), ("b", "record")], &[]);
    let relation = edge("A", "B", "f");
    let target = schema(
        &[("A", "record"), ("B", "record")],
        std::slice::from_ref(&relation),
    );
    let migration = migration(&[("a", "A"), ("b", "B")], &[], &[], &[]);
    let graph = GInstance::new()
        .with_node(Node::new(2, "a"))
        .with_node(Node::new(5, "b"));
    let functor = panproto_inst::MigrationFunctor::new(
        &source,
        &protocol(),
        &target,
        &protocol(),
        &migration,
    )
    .unwrap();
    let (pi, provenance) = g_pi_along_with_provenance(&graph, &functor, 10).unwrap();
    assert_eq!(pi.node_count(), 2);
    assert_eq!(pi.edge_count(), 1);
    let created = *pi.edges().keys().next().unwrap();
    assert!(provenance.edges[&created].is_empty());
}

fn two_cell_schema() -> (Schema, Edge, Edge) {
    let x = edge("row", "row.x", "x");
    let y = edge("row", "row.y", "y");
    (
        schema(
            &[("row", "record"), ("row.x", "int"), ("row.y", "int")],
            &[x.clone(), y.clone()],
        ),
        x,
        y,
    )
}

fn positioned_layout(position: u32, x: &Edge) -> GraphLayout {
    GraphLayout {
        cells: BTreeMap::from([(
            (RowId(0), "x".to_owned()),
            CellLayout {
                root: 1,
                attribute: EdgeLayout {
                    id: Some(EdgeId(4)),
                    position: Some(position),
                    has_source: true,
                },
                nodes: BTreeMap::from([(
                    1,
                    Node::new(1, x.tgt.clone()).with_value(FieldPresence::Present(Value::Int(1))),
                )]),
                edges: BTreeMap::new(),
            },
        )]),
        reserved_node_ids: BTreeSet::from([0, 1]),
        reserved_edge_ids: BTreeSet::from([EdgeId(4)]),
        ..GraphLayout::default()
    }
}

#[test]
fn created_edges_follow_positioned_carried_edges() {
    let (schema, x, y) = two_cell_schema();
    let rel = RelSchema::new(&schema, &protocol()).unwrap();
    let tables = FInstance::new()
        .with_rows(
            "row",
            [(RowId(0), row(&[("x", Value::Int(1)), ("y", Value::Int(2))]))],
        )
        .unwrap();
    let graph = decode_graph(
        &tables,
        EncodingSide::Whole(&rel),
        &positioned_layout(4, &x),
    )
    .unwrap();
    let x_edge = graph
        .edges()
        .values()
        .find(|candidate| candidate.edge == x)
        .unwrap();
    let y_edge = graph
        .edges()
        .values()
        .find(|candidate| candidate.edge == y)
        .unwrap();
    assert_eq!(x_edge.position, Some(4));
    assert_eq!(y_edge.position, Some(5));
}

#[test]
fn decoding_refuses_node_edge_and_position_exhaustion_in_order() {
    let (cell_schema, _) = one_cell_schema();
    let cell_rel = RelSchema::new(&cell_schema, &protocol()).unwrap();
    let cells = FInstance::new()
        .with_rows("person", [(RowId(0), row(&[("name", Value::Int(1))]))])
        .unwrap();
    let node_floor = GraphLayout {
        reserved_node_ids: BTreeSet::from([u32::MAX]),
        ..GraphLayout::default()
    };
    assert!(matches!(
        decode_graph(&cells, EncodingSide::Whole(&cell_rel), &node_floor),
        Err(GraphKanError::NodeIdsExhausted)
    ));

    let relation = edge("a", "b", "f");
    let relation_schema = schema(
        &[("a", "record"), ("b", "record")],
        std::slice::from_ref(&relation),
    );
    let relation_rel = RelSchema::new(&relation_schema, &protocol()).unwrap();
    let pairs = FInstance::new()
        .with_rows("a", [(RowId(0), Row::new())])
        .and_then(|tables| tables.with_rows("b", [(RowId(1), Row::new())]))
        .and_then(|tables| tables.with_foreign_key(relation.clone(), [(RowId(0), RowId(1))]))
        .unwrap();
    let edge_floor = GraphLayout {
        reserved_edge_ids: BTreeSet::from([EdgeId(u32::MAX)]),
        ..GraphLayout::default()
    };
    assert!(matches!(
        decode_graph(&pairs, EncodingSide::Whole(&relation_rel), &edge_floor),
        Err(GraphKanError::EdgeIdsExhausted)
    ));

    let (two_schema, x, _) = two_cell_schema();
    let two_rel = RelSchema::new(&two_schema, &protocol()).unwrap();
    let two_cells = FInstance::new()
        .with_rows(
            "row",
            [(RowId(0), row(&[("x", Value::Int(1)), ("y", Value::Int(2))]))],
        )
        .unwrap();
    assert!(matches!(
        decode_graph(
            &two_cells,
            EncodingSide::Whole(&two_rel),
            &positioned_layout(u32::MAX, &x)
        ),
        Err(GraphKanError::EdgePositionsExhausted { node: 0 })
    ));
}

#[test]
fn graph_kan_error_precedence_and_delta_boundaries_are_explicit() {
    let unclassified_source = schema(&[("source", "unknown")], &[]);
    let unclassified_target = schema(&[("target", "unknown")], &[]);
    let empty = GInstance::new();
    assert!(matches!(
        g_delta(
            &empty,
            &unclassified_source,
            &protocol(),
            &unclassified_target,
            &protocol(),
            &CompiledMigration::default(),
        ),
        Err(GraphKanError::RelSchema(RelSchemaError::UnclassifiedKind {
            vertex,
            ..
        })) if vertex == "source"
    ));

    let source = schema(&[("a", "record")], &[]);
    assert!(matches!(
        g_delta(
            &empty,
            &source,
            &protocol(),
            &unclassified_target,
            &protocol(),
            &CompiledMigration::default(),
        ),
        Err(GraphKanError::RelSchema(RelSchemaError::UnclassifiedKind {
            vertex,
            ..
        })) if vertex == "target"
    ));

    let target = schema(&[("A", "record")], &[]);
    let bad_map = migration(&[("a", "missing")], &[], &[], &[]);
    assert!(matches!(
        g_delta(&empty, &source, &protocol(), &target, &protocol(), &bad_map,),
        Err(GraphKanError::Functor(_))
    ));

    let map = migration(&[("a", "A")], &[], &[], &[]);
    let fan = GInstance::new().with_node(Node::new(0, "A").with_shape(NodeShape::HyperEdge));
    assert!(matches!(
        g_delta(&fan, &source, &protocol(), &target, &protocol(), &map,),
        Err(GraphKanError::NotEncodable {
            reason: NotEncodableReason::FanNode { node: 0 }
        })
    ));

    let target_with_outside = schema(&[("A", "record"), ("Q", "record")], &[]);
    let graph = GInstance::new()
        .with_node(Node::new(4, "A"))
        .with_node(Node::new(8, "Q"));
    let delta = g_delta(
        &graph,
        &source,
        &protocol(),
        &target_with_outside,
        &protocol(),
        &map,
    )
    .unwrap();
    assert_eq!(delta.node_count(), 1);
    assert_eq!(delta.node(4).unwrap().anchor, "a");

    let target_column = edge("A", "A.x", "x");
    let target_with_column = schema(
        &[("A", "record"), ("A.x", "int")],
        std::slice::from_ref(&target_column),
    );
    let source_graph = GInstance::new().with_node(Node::new(0, "a"));
    assert!(matches!(
        g_pi(
            &source_graph,
            &source,
            &protocol(),
            &target_with_column,
            &protocol(),
            &map,
            8,
        ),
        Err(GraphKanError::Adjunction(AdjunctionError::Pi(
            RestrictError::AttributeOutsideImage { table, column }
        ))) if table == "A" && column == "x"
    ));
}

fn restriction_scenario() -> (MigrationFunctor, GInstance, GInstance) {
    let x = edge("a", "a.x", "x");
    let target_x = edge("A", "A.x", "x");
    let source = schema(
        &[("a", "record"), ("a.x", "int"), ("q", "record")],
        std::slice::from_ref(&x),
    );
    let target = schema(
        &[("A", "record"), ("A.x", "int")],
        std::slice::from_ref(&target_x),
    );
    let migration = migration(
        &[("a", "A"), ("a.x", "A.x")],
        &[(x.clone(), target_x)],
        &[],
        &[],
    );
    let functor =
        MigrationFunctor::new(&source, &protocol(), &target, &protocol(), &migration).unwrap();
    let whole = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "a.x").with_value(FieldPresence::Present(Value::Int(4))))
        .with_node(Node::new(7, "q"))
        .with_edge_id(EdgeId(3), 0, 1, x.clone());
    let kept = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "a.x").with_value(FieldPresence::Present(Value::Int(4))))
        .with_edge_id(EdgeId(3), 0, 1, x);
    (functor, whole, kept)
}

#[test]
fn graph_restriction_commutes_with_table_domain_restriction_on_fixture() {
    let (functor, whole, kept) = restriction_scenario();
    let (whole_tables, _) = encode_graph(&whole, EncodingSide::Whole(functor.source())).unwrap();
    let restricted = panproto_inst::f_restrict_to_domain(&whole_tables, &functor).unwrap();
    let (kept_tables, _) = encode_graph(&kept, EncodingSide::Domain(&functor)).unwrap();
    assert_eq!(restricted, kept_tables);
}

#[test]
fn graph_sigma_and_pi_restrict_elements_outside_the_kept_part() {
    let (functor, whole, _) = restriction_scenario();
    let sigma = g_sigma_along(&whole, &functor).unwrap();
    let pi = g_pi_along(&whole, &functor, 32).unwrap();
    assert!(sigma.nodes().values().all(|node| node.anchor != "q"));
    assert!(pi.nodes().values().all(|node| node.anchor != "q"));
    assert_eq!(sigma.node_count(), 2);
    assert_eq!(pi.node_count(), 2);
}

#[test]
fn tree_kan_boundary_cases_are_not_claimed_as_agreements() {
    let source_x = edge("a", "a.x", "x");
    let target_x = edge("A", "A.x", "x");
    let target_y = edge("A", "A.y", "y");
    let source = schema(
        &[("a", "record"), ("a.x", "int")],
        std::slice::from_ref(&source_x),
    );
    let target_with_column = schema(
        &[("A", "record"), ("A.x", "int"), ("A.y", "int")],
        &[target_x.clone(), target_y],
    );
    let migration = migration(
        &[("a", "A"), ("a.x", "A.x")],
        &[(source_x.clone(), target_x.clone())],
        &[],
        &[],
    );
    let functor = MigrationFunctor::new(
        &source,
        &protocol(),
        &target_with_column,
        &protocol(),
        &migration,
    )
    .unwrap();
    let tree = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "a")),
            (
                1,
                Node::new(1, "a.x").with_value(FieldPresence::Present(Value::Int(4))),
            ),
        ]),
        vec![(0, 1, source_x)],
        vec![],
        0,
        "a".into(),
    );
    let graph = GInstance::from_wtype(&tree).unwrap();
    let sigma = g_sigma_restricted(&graph, &functor).unwrap();
    assert!(sigma.nodes().values().any(|node| {
        node.anchor == "A.y"
            && matches!(
                node.value,
                Some(FieldPresence::Present(Value::LabeledNull(_)))
            )
    }));
    let tree_sigma = GInstance::from_wtype(
        &wtype_extend_engine(&tree, &target_with_column, &migration).unwrap(),
    )
    .unwrap();
    assert!(tree_sigma.nodes().values().all(|node| node.anchor != "A.y"));
    assert!(matches!(
        g_pi_restricted(&graph, &functor, 32),
        Err(GraphKanError::Adjunction(AdjunctionError::Pi(
            RestrictError::AttributeOutsideImage { table, column }
        ))) if table == "A" && column == "y"
    ));

    let target_with_table = schema(
        &[("A", "record"), ("A.x", "int"), ("Q", "record")],
        std::slice::from_ref(&target_x),
    );
    let functor = MigrationFunctor::new(
        &source,
        &protocol(),
        &target_with_table,
        &protocol(),
        &migration,
    )
    .unwrap();
    let graph_pi = g_pi_restricted(&graph, &functor, 32).unwrap();
    assert!(graph_pi.nodes().values().any(|node| node.anchor == "Q"));
    let tree_pi =
        GInstance::from_wtype(&wtype_pi(&tree, &target_with_table, &migration, 32).unwrap())
            .unwrap();
    assert!(tree_pi.nodes().values().all(|node| node.anchor != "Q"));
}

proptest! {
    #[test]
    fn decoding_inverts_encoding(
        value in any::<i64>(),
        node_position in proptest::option::of(any::<u32>()),
        edge_position in proptest::option::of(any::<u32>()),
        list_shape in any::<bool>(),
    ) {
        let (schema, name) = one_cell_schema();
        let rel = RelSchema::new(&schema, &protocol()).unwrap();
        let mut table = Node::new(7, "person");
        table.discriminator = Some("person".into());
        table.position = node_position;
        table.annotations.insert("generated".into(), Value::Int(value));
        table.extra_fields.insert("opaque".into(), Value::Bool(list_shape));
        let mut cell = Node::new(11, "person.name")
            .with_value(FieldPresence::Present(Value::Int(value)));
        if list_shape {
            cell.shape = NodeShape::List;
        }
        let mut graph = GInstance::new()
            .with_node(table)
            .with_node(cell)
            .with_edge_id(EdgeId(20), 7, 11, name);
        graph.set_edge_position(EdgeId(20), edge_position).unwrap();
        let (tables, layout) = encode_graph(&graph, EncodingSide::Whole(&rel)).unwrap();
        let decoded = decode_graph(&tables, EncodingSide::Whole(&rel), &layout).unwrap();
        prop_assert!(GInstance::identical(&graph, &decoded));
    }

    #[test]
    fn encoding_inverts_decoding(value in any::<i64>(), flag in any::<bool>()) {
        let (schema, _) = one_cell_schema();
        let rel = RelSchema::new(&schema, &protocol()).unwrap();
        let structured = Value::Unknown(HashMap::from([
            ("value".to_owned(), Value::Int(value)),
            ("flags".to_owned(), Value::List(vec![Value::Bool(flag), Value::Null])),
        ]));
        let tables = FInstance::new()
            .with_rows("person", [(RowId(8), row(&[("name", structured)]))])
            .unwrap();
        let graph = decode_graph(
            &tables,
            EncodingSide::Whole(&rel),
            &GraphLayout::default(),
        ).unwrap();
        let (again, _) = encode_graph(&graph, EncodingSide::Whole(&rel)).unwrap();
        prop_assert_eq!(again, tables);
    }

    #[test]
    fn encoding_accepts_exactly_the_five_pass_domain(id in 0u32..100, value in any::<i64>()) {
        let (schema, name) = one_cell_schema();
        let rel = RelSchema::new(&schema, &protocol()).unwrap();
        let graph = GInstance::new()
            .with_node(Node::new(id, "person"))
            .with_node(Node::new(id + 100, "person.name").with_value(FieldPresence::Present(Value::Int(value))))
            .with_edge_id(EdgeId(id + 200), id, id + 100, name);
        let (tables, layout) = encode_graph(&graph, EncodingSide::Whole(&rel)).unwrap();
        let decoded = decode_graph(&tables, EncodingSide::Whole(&rel), &layout).unwrap();
        prop_assert!(GInstance::identical(&graph, &decoded));
    }

    #[test]
    fn graph_kan_hom_identities_and_composition_are_functorial(value in any::<i64>()) {
        let (schema, _) = one_cell_schema();
        let rel = RelSchema::new(&schema, &protocol()).unwrap();
        let graph = one_cell_graph(Value::Int(value));
        let identity = GraphKanHom::identity(&graph, &EncodingSide::Whole(&rel)).unwrap();
        prop_assert!(identity.check(&graph, &graph, EncodingSide::Whole(&rel)).is_ok());
        let composite = identity.compose(&identity).unwrap();
        prop_assert!(composite.same_as(&identity));
    }

    #[test]
    fn graph_restriction_commutes_with_table_domain_restriction(value in any::<i64>()) {
        let (functor, mut whole, mut kept) = restriction_scenario();
        whole.node_mut(1).unwrap().value = Some(FieldPresence::Present(Value::Int(value)));
        kept.node_mut(1).unwrap().value = Some(FieldPresence::Present(Value::Int(value)));
        let (whole_tables, _) = encode_graph(&whole, EncodingSide::Whole(functor.source())).unwrap();
        let restricted = panproto_inst::f_restrict_to_domain(&whole_tables, &functor).unwrap();
        let (kept_tables, _) = encode_graph(&kept, EncodingSide::Domain(&functor)).unwrap();
        prop_assert_eq!(restricted, kept_tables);
    }

    #[test]
    fn graph_sigma_is_left_adjoint_to_graph_delta(left in 10u32..100, choice in any::<bool>()) {
        let (functor, source, _) = merge_tables_functor();
        let right = left + 100;
        let target = GInstance::new()
            .with_node(Node::new(left, "t"))
            .with_node(Node::new(right, "t"));
        let sigma = g_sigma_along(&source, &functor).unwrap();
        let delta = g_delta_along(&target, &functor).unwrap();
        let image = if choice { left } else { right };
        let hom = GraphKanHom {
            node_map: BTreeMap::from([(0, image), (1, right)]),
            null_map: BTreeMap::new(),
        };
        hom.check(&sigma, &target, EncodingSide::Whole(functor.target())).unwrap();
        let transpose = g_transpose_left(&hom, &source, &target, &functor).unwrap();
        transpose.check(&source, &delta, EncodingSide::Domain(&functor)).unwrap();
        let round = g_transpose_right(&transpose, &source, &target, &functor).unwrap();
        prop_assert!(round.same_as(&hom));
    }

    #[test]
    fn graph_delta_is_left_adjoint_to_graph_pi(choice in any::<bool>()) {
        let (functor, _, target) = merge_tables_functor();
        let source = GInstance::new()
            .with_node(Node::new(0, "a"))
            .with_node(Node::new(1, "a"))
            .with_node(Node::new(2, "b"));
        let delta = g_delta_along(&target, &functor).unwrap();
        let pi = g_pi_along(&source, &functor, 32).unwrap();
        let image = u32::from(choice);
        let hom = GraphKanHom {
            node_map: delta
                .nodes()
                .iter()
                .map(|(node, value)| (*node, if value.anchor == "a" { image } else { 2 }))
                .collect(),
            null_map: BTreeMap::new(),
        };
        hom.check(&delta, &source, EncodingSide::Domain(&functor)).unwrap();
        let transpose = g_pi_transpose_left(&hom, &target, &source, &functor, 32).unwrap();
        transpose.check(&target, &pi, EncodingSide::Whole(functor.target())).unwrap();
        let round = g_pi_transpose_right(&transpose, &target, &source, &functor, 32).unwrap();
        prop_assert!(round.same_as(&hom));
    }

    #[test]
    fn graph_delta_agrees_on_injective_maps(id in 0u32..100, value in any::<i64>()) {
        let source_edge = edge("a", "a.x", "x");
        let target_edge = edge("A", "A.x", "x");
        let source = schema(
            &[("a", "record"), ("a.x", "int")],
            std::slice::from_ref(&source_edge),
        );
        let target = schema(
            &[("A", "record"), ("A.x", "int")],
            std::slice::from_ref(&target_edge),
        );
        let migration = migration(
            &[("a", "A"), ("a.x", "A.x")],
            &[(source_edge.clone(), target_edge.clone())],
            &[],
            &[],
        );
        let functor = MigrationFunctor::new(
            &source,
            &protocol(),
            &target,
            &protocol(),
            &migration,
        ).unwrap();
        let graph = GInstance::new()
            .with_node(Node::new(id, "A"))
            .with_node(Node::new(id + 100, "A.x").with_value(FieldPresence::Present(Value::Int(value))))
            .with_edge_id(EdgeId(id + 200), id, id + 100, target_edge);
        let expected = GInstance::new()
            .with_node(Node::new(id, "a"))
            .with_node(Node::new(id + 100, "a.x").with_value(FieldPresence::Present(Value::Int(value))))
            .with_edge_id(EdgeId(id + 200), id, id + 100, source_edge);
        let actual = g_delta_along(&graph, &functor).unwrap();
        prop_assert!(
            GInstance::identical(&actual, &expected),
            "actual={actual:?}; expected={expected:?}"
        );
    }

    #[test]
    fn graph_kan_agrees_with_the_tree_kan_on_trees(value in any::<i64>()) {
        let source_edge = edge("a", "a.object", "object");
        let source_name = edge("a.object", "a.name", "name");
        let source_count = edge("a.object", "a.count", "count");
        let target_edge = edge("A", "A.object", "object");
        let target_name = edge("A.object", "A.name", "name");
        let target_count = edge("A.object", "A.count", "count");
        let source = schema(
            &[
                ("a", "record"),
                ("a.object", "value"),
                ("a.name", "str"),
                ("a.count", "int"),
            ],
            &[source_edge.clone(), source_name.clone(), source_count.clone()],
        );
        let target = schema(
            &[
                ("A", "record"),
                ("A.object", "value"),
                ("A.name", "str"),
                ("A.count", "int"),
            ],
            &[target_edge.clone(), target_name.clone(), target_count.clone()],
        );
        let migration = migration(
            &[
                ("a", "A"),
                ("a.object", "A.object"),
                ("a.name", "A.name"),
                ("a.count", "A.count"),
            ],
            &[
                (source_edge.clone(), target_edge),
                (source_name.clone(), target_name),
                (source_count.clone(), target_count),
            ],
            &[],
            &[],
        );
        let functor = MigrationFunctor::new(
            &source,
            &protocol(),
            &target,
            &protocol(),
            &migration,
        ).unwrap();
        let nodes = HashMap::from([
            (0, Node::new(0, "a")),
            (2, Node::new(2, "a.object")),
            (
                3,
                Node::new(3, "a.name")
                    .with_value(FieldPresence::Present(Value::Str("item".into()))),
            ),
            (
                5,
                Node::new(5, "a.count")
                    .with_value(FieldPresence::Present(Value::Int(value))),
            ),
        ]);
        let tree = WInstance::new(
            nodes,
            vec![
                (0, 2, source_edge),
                (2, 3, source_name),
                (2, 5, source_count),
            ],
            vec![],
            0,
            "a".into(),
        );
        let graph = GInstance::from_wtype(&tree).unwrap();
        let graph_sigma = g_sigma_restricted(&graph, &functor).unwrap();
        let tree_sigma = GInstance::from_wtype(
            &wtype_extend_engine(&tree, &target, &migration).unwrap(),
        ).unwrap();
        prop_assert!(
            GInstance::identical(&graph_sigma, &tree_sigma),
            "graph sigma={graph_sigma:?}; tree sigma={tree_sigma:?}"
        );
        let graph_pi = g_pi_restricted(&graph, &functor, 32).unwrap();
        let tree_pi = GInstance::from_wtype(&wtype_pi(&tree, &target, &migration, 32).unwrap()).unwrap();
        prop_assert!(GInstance::identical(&graph_pi, &tree_pi));
    }

    #[test]
    fn graph_kan_ids_and_positions_are_deterministic(value in any::<i64>()) {
        let (schema, name) = one_cell_schema();
        let rel = RelSchema::new(&schema, &protocol()).unwrap();
        let first = GInstance::new()
            .with_node(Node::new(0, "person"))
            .with_node(Node::new(1, "person.name").with_value(FieldPresence::Present(Value::Int(value))))
            .with_edge_id(EdgeId(2), 0, 1, name.clone());
        let second = GInstance::new()
            .with_node(Node::new(1, "person.name").with_value(FieldPresence::Present(Value::Int(value))))
            .with_node(Node::new(0, "person"))
            .with_edge_id(EdgeId(2), 0, 1, name);
        let (first_tables, first_layout) = encode_graph(&first, EncodingSide::Whole(&rel)).unwrap();
        let (second_tables, second_layout) = encode_graph(&second, EncodingSide::Whole(&rel)).unwrap();
        prop_assert_eq!(&first_tables, &second_tables);
        let first_decoded = decode_graph(&first_tables, EncodingSide::Whole(&rel), &first_layout).unwrap();
        let second_decoded = decode_graph(&second_tables, EncodingSide::Whole(&rel), &second_layout).unwrap();
        prop_assert!(GInstance::identical(&first_decoded, &second_decoded));
    }
}
