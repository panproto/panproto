#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Relational graph Kan examples, refusals, and finite laws.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_inst::{
    AdjunctionError, BoundaryProtocols, CellLayout, CompiledMigration, EdgeId, EdgeLayout,
    EncodingSide, FInstance, FInstanceHom, FieldPresence, GInstance, GraphKanError, GraphKanHom,
    GraphLayout, MigrationFunctor, Node, NodeShape, NotEncodableReason, RelSchema, RelSchemaError,
    RestrictError, Row, RowId, Value, WInstance, decode_graph, encode_graph, g_counit, g_delta,
    g_delta_along, g_delta_along_with_provenance, g_delta_with_provenance, g_pi, g_pi_along,
    g_pi_along_with_provenance, g_pi_counit, g_pi_restricted, g_pi_transpose_left,
    g_pi_transpose_right, g_pi_unit, g_pi_with_provenance, g_sigma, g_sigma_along,
    g_sigma_along_with_provenance, g_sigma_restricted, g_sigma_with_provenance, g_transpose_left,
    g_transpose_right, g_unit, graph_extend, graph_extend_engine, wtype_extend_engine, wtype_pi,
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

/// An inner value edge; the empty name stands for an unnamed edge, whose
/// column name is its target vertex.
fn inner_edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        name: (!name.is_empty()).then(|| name.into()),
        ..edge(src, tgt, name)
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
                    Some((
                        oracle::name_of(&edge.edge),
                        fold(graph, edge.tgt, visiting)?,
                    ))
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
    let copies: BTreeSet<(Name, RowId)> = provenance.table.copies.values().cloned().collect();
    assert_eq!(
        copies,
        BTreeSet::from([(Name::from("A"), RowId(0)), (Name::from("B"), RowId(0))])
    );
    for (copy, (table, _)) in &provenance.table.copies {
        assert_eq!(delta.node(copy.0).unwrap().anchor, *table);
        assert_eq!(delta.out_edges(copy.0).len(), 1, "one cell per kept column");
    }
    let leaves: Vec<_> = delta
        .nodes()
        .values()
        .filter(|node| node.anchor == "str")
        .collect();
    assert_eq!(leaves.len(), 2);
    assert_ne!(leaves[0].id, leaves[1].id);
    let rows: Vec<u32> = provenance.table.copies.keys().map(|row| row.0).collect();
    assert!(
        leaves
            .iter()
            .all(|leaf| !rows.contains(&leaf.id) && leaf.id != 0),
        "each value subtree has its own id, distinct from every copy row"
    );
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

/// A one-column table `a` (renamed `A` when `rename` holds) whose column
/// `x` holds a list of ints, one element vertex per entry of `elements`,
/// each reached by an inner edge called `name` (empty for unnamed); a
/// migration sending element vertex `k` and its inner edge to element
/// vertex `image_of[k]` of the target and every other vertex and edge to
/// its namesake; and a graph whose `x` cell is the list `[1, 2, ...]` with
/// element `i` on element vertex `placements[i]`.
struct ListColumn {
    source: Schema,
    target: Schema,
    migration: CompiledMigration,
    functor: MigrationFunctor,
    graph: GInstance,
}

fn list_column(
    rename: bool,
    elements: &[&str],
    name: &str,
    image_of: &[usize],
    placements: &[usize],
) -> ListColumn {
    let side = |table: &str| {
        let value = format!("{table}.x");
        let element_vertices: Vec<String> = elements
            .iter()
            .map(|element| format!("{value}.{element}"))
            .collect();
        let column = edge(table, &value, "x");
        let inner: Vec<Edge> = element_vertices
            .iter()
            .map(|vertex| inner_edge(&value, vertex, name))
            .collect();
        let mut vertices = vec![(table.to_owned(), "record"), (value, "value")];
        vertices.extend(
            element_vertices
                .iter()
                .map(|vertex| (vertex.clone(), "int")),
        );
        let vertex_refs: Vec<(&str, &str)> = vertices
            .iter()
            .map(|(id, kind)| (id.as_str(), *kind))
            .collect();
        let mut edges = vec![column.clone()];
        edges.extend(inner.iter().cloned());
        (
            schema(&vertex_refs, &edges),
            column,
            inner,
            vertices.into_iter().map(|(id, _)| id).collect::<Vec<_>>(),
        )
    };
    let (source, x, source_inner, source_vertices) = side("a");
    let (target, big_x, target_inner, target_vertices) = side(if rename { "A" } else { "a" });
    // The first two vertices are the table and the column's value vertex;
    // element vertex `k` follows at `2 + k`.
    let vertex_image = |index: usize| {
        if index < 2 {
            index
        } else {
            2 + image_of[index - 2]
        }
    };
    let vertex_pairs: Vec<(&str, &str)> = source_vertices
        .iter()
        .enumerate()
        .map(|(index, from)| (from.as_str(), target_vertices[vertex_image(index)].as_str()))
        .collect();
    let mut edge_pairs = vec![(x.clone(), big_x)];
    edge_pairs.extend(
        source_inner
            .iter()
            .enumerate()
            .map(|(index, from)| (from.clone(), target_inner[image_of[index]].clone())),
    );
    let migration = migration(&vertex_pairs, &edge_pairs, &[], &[]);
    let functor =
        MigrationFunctor::new(&source, &protocol(), &target, &protocol(), &migration).unwrap();
    let mut graph = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "a.x").with_shape(NodeShape::List))
        .with_edge_id(EdgeId(10), 0, 1, x);
    for (index, &placement) in placements.iter().enumerate() {
        let node = u32::try_from(index).unwrap() + 2;
        let value = i64::try_from(index).unwrap() + 1;
        let element = &source_inner[placement];
        graph = graph
            .with_node(
                Node::new(node, element.tgt.clone())
                    .with_value(FieldPresence::Present(Value::Int(value))),
            )
            .with_edge_id(EdgeId(20 + node), 1, node, element.clone());
        graph
            .set_edge_position(EdgeId(20 + node), Some(u32::try_from(index).unwrap()))
            .unwrap();
    }
    ListColumn {
        source,
        target,
        migration,
        functor,
        graph,
    }
}

/// [`list_column`] along the identity map on element vertices.
fn list_column_fixture(
    rename: bool,
    elements: &[&str],
    name: &str,
    placements: &[usize],
) -> (MigrationFunctor, GInstance) {
    let identity: Vec<usize> = (0..elements.len()).collect();
    let fixture = list_column(rename, elements, name, &identity, placements);
    (fixture.functor, fixture.graph)
}

/// The element edges out of the value node at `column_vertex` in `graph`,
/// as (element vertex, position, value), ascending by position.
fn list_elements(graph: &GInstance, column_vertex: &str) -> Vec<(String, Option<u32>, Value)> {
    let list = graph
        .nodes()
        .values()
        .find(|node| node.anchor == column_vertex)
        .unwrap()
        .id;
    let mut elements: Vec<_> = graph
        .edges()
        .values()
        .filter(|edge| edge.src == list)
        .map(|edge| {
            let target = graph.node(edge.tgt).unwrap();
            let value = match target.value.as_ref() {
                Some(FieldPresence::Present(value)) => value.clone(),
                other => panic!("element without a value: {other:?}"),
            };
            (target.anchor.to_string(), edge.position, value)
        })
        .collect();
    elements.sort_by_key(|(_, position, _)| *position);
    elements
}

#[test]
fn graph_sigma_keeps_an_unnamed_list_layout_across_a_vertex_rename() {
    let (functor, graph) = list_column_fixture(true, &["item"], "", &[0, 0]);
    let sigma = g_sigma_along(&graph, &functor).unwrap();
    assert_eq!((sigma.node_count(), sigma.edge_count()), (4, 3));
    assert_eq!(
        list_elements(&sigma, "A.x"),
        vec![
            ("A.x.item".to_owned(), Some(0), Value::Int(1)),
            ("A.x.item".to_owned(), Some(1), Value::Int(2)),
        ]
    );
}

#[test]
fn graph_sigma_keeps_a_named_list_layout_across_a_vertex_rename() {
    let (functor, graph) = list_column_fixture(true, &["item"], "item", &[0, 0]);
    let sigma = g_sigma_along(&graph, &functor).unwrap();
    assert_eq!((sigma.node_count(), sigma.edge_count()), (4, 3));
    assert_eq!(
        list_elements(&sigma, "A.x"),
        vec![
            ("A.x.item".to_owned(), Some(0), Value::Int(1)),
            ("A.x.item".to_owned(), Some(1), Value::Int(2)),
        ]
    );
}

#[test]
fn graph_sigma_keeps_unnamed_list_elements_on_their_own_element_vertices() {
    let (functor, graph) = list_column_fixture(false, &["i", "j"], "", &[1, 0]);
    let sigma = g_sigma_along(&graph, &functor).unwrap();
    assert_eq!((sigma.node_count(), sigma.edge_count()), (4, 3));
    assert_eq!(
        list_elements(&sigma, "a.x"),
        vec![
            ("a.x.j".to_owned(), Some(0), Value::Int(1)),
            ("a.x.i".to_owned(), Some(1), Value::Int(2)),
        ]
    );
}

/// [`list_column`] with its migration cut down to the table `a`, its value
/// vertex `a.x` and its column `x`, so that it mentions neither the element
/// vertices nor the inner edges, and its functor rebuilt from that
/// migration.
fn without_element_remaps(mut fixture: ListColumn) -> ListColumn {
    fixture
        .migration
        .vertex_remap
        .retain(|vertex, _| vertex.matches('.').count() < 2);
    fixture
        .migration
        .edge_remap
        .retain(|edge, _| edge.src == "a");
    fixture.functor = MigrationFunctor::new(
        &fixture.source,
        &protocol(),
        &fixture.target,
        &protocol(),
        &fixture.migration,
    )
    .unwrap();
    fixture
}

#[test]
fn graph_sigma_decodes_a_list_whose_element_edges_have_no_image_as_a_leaf() {
    // The migration renames `a`, `a.x` and column `x` to `A`, `A.x` and `x`
    // and mentions no element vertex and no inner edge. An element edge
    // `a.x -> a.x.k` has no remap, and its target vertex no remap either, so
    // its image would be the target edge of its kind and name from `A.x` to
    // the namesake `a.x.k`, which the target lacks: it has no image, and the
    // cell is decoded as one leaf holding the list. With one element vertex
    // the only target edge of that kind and name leaving `A.x` ends at
    // `A.x.i`; being the only one does not make it an image.
    for elements in [&["i", "j"][..], &["i"][..]] {
        for name in ["", "item"] {
            let identity: Vec<usize> = (0..elements.len()).collect();
            let fixture = without_element_remaps(list_column(
                true,
                elements,
                name,
                &identity,
                &[elements.len() - 1, 0],
            ));
            let case = format!("elements {elements:?}, name {name:?}");
            for sigma in [
                g_sigma_along(&fixture.graph, &fixture.functor).unwrap(),
                list_sigma(&fixture),
            ] {
                assert_eq!(
                    (sigma.node_count(), sigma.edge_count()),
                    (2, 1),
                    "{case}: {sigma:?}"
                );
                let leaf = sigma
                    .nodes()
                    .values()
                    .find(|node| node.anchor == "A.x")
                    .unwrap();
                assert_eq!(
                    leaf.value,
                    Some(FieldPresence::Present(Value::List(vec![
                        Value::Int(1),
                        Value::Int(2)
                    ]))),
                    "{case}"
                );
            }
        }
    }
}

/// Every two-element list column of [`list_column`] whose migration is a
/// bijection on element vertices: unnamed and `item`-named element edges,
/// with and without a table rename, and the element swap without one.
fn bijective_list_columns() -> Vec<(String, ListColumn)> {
    let mut fixtures = Vec::new();
    for name in ["", "item"] {
        for (rename, image_of) in [(true, [0, 1]), (true, [1, 0]), (false, [1, 0])] {
            fixtures.push((
                format!("name {name:?}, rename {rename}, images {image_of:?}"),
                list_column(rename, &["i", "j"], name, &image_of, &[0, 1]),
            ));
        }
    }
    fixtures
}

/// The list `[1, 2]` of [`list_column`] with element `k` on the element
/// vertex its migration sends `k`'s source element vertex to, as
/// [`list_elements`] reads it.
fn moved_elements(fixture: &ListColumn) -> Vec<(String, Option<u32>, Value)> {
    ["a.x.i", "a.x.j"]
        .into_iter()
        .enumerate()
        .map(|(index, element)| {
            let image = fixture
                .migration
                .edge_remap
                .iter()
                .find(|(source, _)| source.src == "a.x" && source.tgt == element)
                .map(|(_, image)| image)
                .unwrap();
            (
                image.tgt.to_string(),
                Some(u32::try_from(index).unwrap()),
                Value::Int(i64::try_from(index).unwrap() + 1),
            )
        })
        .collect()
}

/// Whether [`list_column`]'s migration renames the table, so that the
/// fixture lies where total graph extension is checked against graph Sigma
/// here. The element swap that keeps the table's name lies in the same
/// agreement domain, but its extension is tested with the extend functions.
fn renames_the_table(fixture: &ListColumn) -> bool {
    fixture.target.vertices.contains_key("A")
}

/// The total graph extension of [`list_column`]'s graph along its
/// migration.
fn extended(fixture: &ListColumn) -> Result<GInstance, RestrictError> {
    graph_extend_engine(
        &fixture.graph,
        &fixture.source,
        &fixture.target,
        &fixture.migration,
        BoundaryProtocols {
            src: &protocol(),
            tgt: &protocol(),
        },
    )
}

/// Graph Sigma of [`list_column`]'s graph along its migration.
fn list_sigma(fixture: &ListColumn) -> GInstance {
    g_sigma(
        &fixture.graph,
        &fixture.source,
        &protocol(),
        &fixture.target,
        &protocol(),
        &fixture.migration,
    )
    .unwrap()
}

/// The value vertex of `x` in [`list_column`]'s target.
fn target_column_vertex(fixture: &ListColumn) -> &'static str {
    if renames_the_table(fixture) {
        "A.x"
    } else {
        "a.x"
    }
}

#[test]
fn graph_sigma_moves_list_elements_along_the_migrations_edge_map() {
    for (case, fixture) in bijective_list_columns() {
        let column = target_column_vertex(&fixture);
        let sigma = list_sigma(&fixture);
        assert_eq!(
            list_elements(&sigma, column),
            moved_elements(&fixture),
            "{case}"
        );
        for (form, along) in [
            (
                "g_sigma_along",
                g_sigma_along(&fixture.graph, &fixture.functor).unwrap(),
            ),
            (
                "g_sigma_restricted",
                g_sigma_restricted(&fixture.graph, &fixture.functor).unwrap(),
            ),
        ] {
            assert_eq!(
                list_elements(&along, column),
                moved_elements(&fixture),
                "{case}: {form}"
            );
            assert!(GInstance::identical(&along, &sigma), "{case}: {form}");
        }
        if renames_the_table(&fixture) {
            let extend = extended(&fixture).unwrap();
            assert!(
                GInstance::identical(&sigma, &extend),
                "{case}: sigma {sigma:?}, extend {extend:?}"
            );
            let validated = graph_extend(
                &fixture.graph,
                &fixture.source,
                &fixture.target,
                &fixture.migration,
                BoundaryProtocols {
                    src: &protocol(),
                    tgt: &protocol(),
                },
            )
            .unwrap();
            assert!(GInstance::identical(&sigma, &validated), "{case}");
        }
    }
}

#[test]
fn graph_pi_moves_list_elements_along_the_migrations_edge_map() {
    for (case, fixture) in bijective_list_columns() {
        let pi = g_pi(
            &fixture.graph,
            &fixture.source,
            &protocol(),
            &fixture.target,
            &protocol(),
            &fixture.migration,
            32,
        )
        .unwrap();
        let column = target_column_vertex(&fixture);
        assert_eq!(
            list_elements(&pi, column),
            moved_elements(&fixture),
            "{case}"
        );
        assert!(
            GInstance::identical(&pi, &list_sigma(&fixture)),
            "{case}: pi {pi:?}"
        );
        for (form, along) in [
            (
                "g_pi_along",
                g_pi_along(&fixture.graph, &fixture.functor, 32).unwrap(),
            ),
            (
                "g_pi_restricted",
                g_pi_restricted(&fixture.graph, &fixture.functor, 32).unwrap(),
            ),
        ] {
            assert_eq!(
                list_elements(&along, column),
                moved_elements(&fixture),
                "{case}: {form}"
            );
            assert!(GInstance::identical(&along, &pi), "{case}: {form}");
        }
    }
}

#[test]
fn graph_delta_moves_list_elements_back_along_the_migrations_edge_map() {
    for (case, fixture) in bijective_list_columns() {
        let sigma = list_sigma(&fixture);
        let delta = g_delta(
            &sigma,
            &fixture.source,
            &protocol(),
            &fixture.target,
            &protocol(),
            &fixture.migration,
        )
        .unwrap();
        assert!(
            GInstance::identical(&delta, &fixture.graph),
            "{case}: delta {delta:?}, input {:?}",
            fixture.graph
        );
        let along = g_delta_along(&sigma, &fixture.functor).unwrap();
        assert_eq!(
            list_elements(&along, "a.x"),
            list_elements(&fixture.graph, "a.x"),
            "{case}"
        );
        assert!(GInstance::identical(&along, &fixture.graph), "{case}");
        if renames_the_table(&fixture) {
            let back = g_delta_along(&extended(&fixture).unwrap(), &fixture.functor).unwrap();
            assert!(GInstance::identical(&back, &fixture.graph), "{case}");
        }
    }
}

#[test]
fn graph_kan_unit_and_counit_check_along_the_element_swap() {
    // The swap of the two element vertices without a table rename, along the
    // one functor the graph Kan functions and the adjunction maps both take.
    for (case, fixture) in bijective_list_columns() {
        if renames_the_table(&fixture) {
            continue;
        }
        let functor = &fixture.functor;
        let sigma = g_sigma_along(&fixture.graph, functor).unwrap();
        assert_eq!(
            list_elements(&sigma, "a.x"),
            moved_elements(&fixture),
            "{case}"
        );
        let delta_sigma = g_delta_along(&sigma, functor).unwrap();
        assert!(
            GInstance::identical(&delta_sigma, &fixture.graph),
            "{case}: {delta_sigma:?}"
        );
        let unit = g_unit(&fixture.graph, functor).unwrap();
        unit.check(&fixture.graph, &delta_sigma, EncodingSide::Domain(functor))
            .unwrap();
        assert_eq!(unit.node_map, BTreeMap::from([(0, 0)]), "{case}");
        let sigma_delta = g_sigma_along(&g_delta_along(&sigma, functor).unwrap(), functor).unwrap();
        assert!(
            GInstance::identical(&sigma_delta, &sigma),
            "{case}: {sigma_delta:?}"
        );
        let counit = g_counit(&sigma, functor).unwrap();
        counit
            .check(&sigma_delta, &sigma, EncodingSide::Whole(functor.target()))
            .unwrap();
        assert_eq!(counit.node_map, BTreeMap::from([(0, 0)]), "{case}");
    }
}

#[test]
fn graph_kan_moves_list_elements_along_inner_edge_remaps_alone() {
    // The migration remaps the element edges but not their target vertices,
    // so only the edge remaps say where each element goes.
    for (case, mut fixture) in bijective_list_columns() {
        fixture
            .migration
            .vertex_remap
            .retain(|vertex, _| vertex.matches('.').count() < 2);
        assert_eq!(fixture.migration.vertex_remap.len(), 2, "{case}");
        let functor = MigrationFunctor::new(
            &fixture.source,
            &protocol(),
            &fixture.target,
            &protocol(),
            &fixture.migration,
        )
        .unwrap();
        let sigma = g_sigma_along(&fixture.graph, &functor).unwrap();
        let column = target_column_vertex(&fixture);
        assert_eq!(
            list_elements(&sigma, column),
            moved_elements(&fixture),
            "{case}"
        );
        let pi = g_pi_along(&fixture.graph, &functor, 32).unwrap();
        assert!(GInstance::identical(&pi, &sigma), "{case}");
        let delta = g_delta_along(&sigma, &functor).unwrap();
        assert!(GInstance::identical(&delta, &fixture.graph), "{case}");
    }
}

#[test]
fn graph_delta_decodes_a_list_whose_element_edge_has_two_preimages_as_a_leaf() {
    // Both source element edges go to the one target element edge, so a
    // target element edge has two preimages in the copy's fibre and the
    // copied cell has no layout to take.
    let fixture = list_column(true, &["i", "j"], "", &[0, 0], &[0, 1]);
    let functor = &fixture.functor;
    let sigma = g_sigma_along(&fixture.graph, functor).unwrap();
    assert_eq!(
        list_elements(&sigma, "A.x"),
        vec![
            ("A.x.i".to_owned(), Some(0), Value::Int(1)),
            ("A.x.i".to_owned(), Some(1), Value::Int(2)),
        ]
    );
    let delta = g_delta_along(&sigma, functor).unwrap();
    assert_eq!((delta.node_count(), delta.edge_count()), (2, 1));
    let leaf = delta
        .nodes()
        .values()
        .find(|node| node.anchor == "a.x")
        .unwrap();
    assert_eq!(
        leaf.value,
        Some(FieldPresence::Present(Value::List(vec![
            Value::Int(1),
            Value::Int(2)
        ])))
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

/// Which schema-taking graph Kan form a precedence check runs.
#[derive(Clone, Copy, Debug)]
enum SchemaForm {
    Sigma,
    SigmaWithProvenance,
    Pi,
    PiWithProvenance,
}

impl SchemaForm {
    const ALL: [Self; 4] = [
        Self::Sigma,
        Self::SigmaWithProvenance,
        Self::Pi,
        Self::PiWithProvenance,
    ];

    const fn is_sigma(self) -> bool {
        matches!(self, Self::Sigma | Self::SigmaWithProvenance)
    }

    fn run(
        self,
        graph: &GInstance,
        source: &Schema,
        target: &Schema,
        map: &CompiledMigration,
    ) -> Result<GInstance, GraphKanError> {
        let p = protocol();
        match self {
            Self::Sigma => g_sigma(graph, source, &p, target, &p, map),
            Self::SigmaWithProvenance => {
                g_sigma_with_provenance(graph, source, &p, target, &p, map).map(|(g, _)| g)
            }
            Self::Pi => g_pi(graph, source, &p, target, &p, map, 8),
            Self::PiWithProvenance => {
                g_pi_with_provenance(graph, source, &p, target, &p, map, 8).map(|(g, _)| g)
            }
        }
    }
}

/// The schemas of the Sigma and Pi precedence checks: a source `a{x, y}`, a
/// target `A{x, z}`, a migration sending both `x` and `y` to `x` (Sigma
/// clashes when they hold different constants) and one sending only `x` to
/// `x` (Pi refuses the target column `z` outside its image).
struct PrecedenceFixture {
    x: Edge,
    y: Edge,
    source: Schema,
    target: Schema,
    clashing: CompiledMigration,
    partial: CompiledMigration,
}

fn precedence_fixture() -> PrecedenceFixture {
    let x = edge("a", "a.x", "x");
    let y = edge("a", "a.y", "y");
    let source = schema(
        &[("a", "record"), ("a.x", "int"), ("a.y", "int")],
        &[x.clone(), y.clone()],
    );
    let target_x = edge("A", "A.x", "x");
    let target_z = edge("A", "A.z", "z");
    let target = schema(
        &[("A", "record"), ("A.x", "int"), ("A.z", "int")],
        &[target_x.clone(), target_z],
    );
    let clashing = migration(
        &[("a", "A"), ("a.x", "A.x"), ("a.y", "A.x")],
        &[(x.clone(), target_x.clone()), (y.clone(), target_x.clone())],
        &[],
        &[],
    );
    let partial = migration(
        &[("a", "A"), ("a.x", "A.x")],
        &[(x.clone(), target_x)],
        &[],
        &[],
    );
    PrecedenceFixture {
        x,
        y,
        source,
        target,
        clashing,
        partial,
    }
}

/// A relation `f: b -> c` carried to `f: B -> C`, with a second target
/// relation `g: B -> C` outside the image, which Pi fills with all pairs.
fn relation_outside_image() -> (Edge, Schema, Schema, CompiledMigration) {
    let f = edge("b", "c", "f");
    let target_f = edge("B", "C", "f");
    let target_g = edge("B", "C", "g");
    let related = schema(
        &[("b", "record"), ("c", "record")],
        std::slice::from_ref(&f),
    );
    let related_target = schema(
        &[("B", "record"), ("C", "record")],
        &[target_f.clone(), target_g],
    );
    let relating = migration(
        &[("b", "B"), ("c", "C")],
        &[(f.clone(), target_f)],
        &[],
        &[],
    );
    (f, related, related_target, relating)
}

/// Spec ruling S3 (11) for the schema-taking Sigma and Pi, in each form
/// (with and without provenance): an input wrong at every later stage is
/// refused at the earliest one, in the order classification (source before
/// target), functor, encoding, table.
#[test]
fn graph_sigma_and_pi_error_precedence_is_explicit() {
    let unclassified_source = schema(&[("source", "unknown")], &[]);
    let unclassified_target = schema(&[("target", "unknown")], &[]);
    let PrecedenceFixture {
        x,
        y,
        source,
        target,
        clashing,
        partial,
    } = precedence_fixture();
    let bad_map = migration(&[("a", "missing")], &[], &[], &[]);
    let at_ceiling = |first: Value, second: Value| {
        GInstance::new()
            .with_node(Node::new(0, "a"))
            .with_node(Node::new(1, "a.x").with_value(FieldPresence::Present(first)))
            .with_node(Node::new(u32::MAX, "a.y").with_value(FieldPresence::Present(second)))
            .with_edge_id(EdgeId(0), 0, 1, x.clone())
            .with_edge_id(EdgeId(u32::MAX), 0, u32::MAX, y.clone())
    };
    let mut fan = at_ceiling(Value::Int(1), Value::Int(2));
    fan.node_mut(0).unwrap().shape = NodeShape::HyperEdge;

    for form in SchemaForm::ALL {
        let table_map = if form.is_sigma() { &clashing } else { &partial };
        assert!(
            matches!(
                form.run(&fan, &unclassified_source, &unclassified_target, &bad_map),
                Err(GraphKanError::RelSchema(RelSchemaError::UnclassifiedKind { vertex, .. }))
                    if vertex == "source"
            ),
            "{form:?}"
        );
        assert!(
            matches!(
                form.run(&fan, &source, &unclassified_target, &bad_map),
                Err(GraphKanError::RelSchema(RelSchemaError::UnclassifiedKind { vertex, .. }))
                    if vertex == "target"
            ),
            "{form:?}"
        );
        assert!(
            matches!(
                form.run(&fan, &source, &target, &bad_map),
                Err(GraphKanError::Functor(_))
            ),
            "{form:?}"
        );
        assert!(
            matches!(
                form.run(&fan, &source, &target, table_map),
                Err(GraphKanError::NotEncodable {
                    reason: NotEncodableReason::FanNode { node: 0 }
                })
            ),
            "{form:?}"
        );
        let refused = form.run(
            &at_ceiling(Value::Int(1), Value::Int(2)),
            &source,
            &target,
            table_map,
        );
        if form.is_sigma() {
            assert!(
                matches!(
                    refused,
                    Err(GraphKanError::Adjunction(
                        AdjunctionError::SigmaConstantClash { .. }
                    ))
                ),
                "{form:?}: {refused:?}"
            );
        } else {
            assert!(
                matches!(
                    refused,
                    Err(GraphKanError::Adjunction(AdjunctionError::Pi(
                        RestrictError::AttributeOutsideImage { ref table, ref column }
                    ))) if table == "A" && column == "z"
                ),
                "{form:?}: {refused:?}"
            );
        }
    }
}

/// Spec ruling S3 (11), last stage: the schema-taking Sigma and Pi, in each
/// form, refuse with the decoding error when every earlier stage succeeds
/// and the output needs an id above `u32::MAX`.
#[test]
fn graph_sigma_and_pi_refuse_id_exhaustion_last() {
    let PrecedenceFixture {
        x,
        y,
        source,
        target,
        partial,
        ..
    } = precedence_fixture();
    // Decoding: Sigma along `partial` writes a fresh labelled null at the
    // target column `z`, whose value node needs an id above `u32::MAX`; Pi
    // along a relation outside the image writes the all-pairs relation,
    // whose edge needs an id above `u32::MAX`.
    let node_ceiling = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(u32::MAX, "a.x").with_value(FieldPresence::Present(Value::Int(1))))
        .with_node(Node::new(1, "a.y").with_value(FieldPresence::Present(Value::Int(2))))
        .with_edge_id(EdgeId(0), 0, u32::MAX, x)
        .with_edge_id(EdgeId(1), 0, 1, y);
    let (f, related, related_target, relating) = relation_outside_image();
    let edge_ceiling = GInstance::new()
        .with_node(Node::new(0, "b"))
        .with_node(Node::new(1, "c"))
        .with_edge_id(EdgeId(u32::MAX), 0, 1, f);
    for form in SchemaForm::ALL {
        let decoded = if form.is_sigma() {
            form.run(&node_ceiling, &source, &target, &partial)
        } else {
            form.run(&edge_ceiling, &related, &related_target, &relating)
        };
        assert!(
            matches!(
                (form.is_sigma(), &decoded),
                (true, Err(GraphKanError::NodeIdsExhausted))
                    | (false, Err(GraphKanError::EdgeIdsExhausted))
            ),
            "{form:?}: {decoded:?}"
        );
    }
}

/// The schema-taking Sigma and Pi, in each form, succeed on an input wrong
/// at no stage and agree with the form along the checked functor.
#[test]
fn graph_sigma_and_pi_from_schemas_agree_with_their_forms_along_the_functor() {
    let PrecedenceFixture {
        x,
        y,
        source,
        target,
        clashing,
        ..
    } = precedence_fixture();
    let (f, related, related_target, relating) = relation_outside_image();
    let plain = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "a.x").with_value(FieldPresence::Present(Value::Int(1))))
        .with_node(Node::new(2, "a.y").with_value(FieldPresence::Present(Value::Int(1))))
        .with_edge_id(EdgeId(3), 0, 1, x)
        .with_edge_id(EdgeId(4), 0, 2, y);
    let functor =
        MigrationFunctor::new(&source, &protocol(), &target, &protocol(), &clashing).unwrap();
    let along_sigma = g_sigma_along(&plain, &functor).unwrap();
    assert_eq!(along_sigma.node_count(), 3);
    let related_graph = GInstance::new()
        .with_node(Node::new(0, "b"))
        .with_node(Node::new(1, "c"))
        .with_edge_id(EdgeId(5), 0, 1, f);
    let related_functor = MigrationFunctor::new(
        &related,
        &protocol(),
        &related_target,
        &protocol(),
        &relating,
    )
    .unwrap();
    let along_pi = g_pi_along(&related_graph, &related_functor, 8).unwrap();
    assert_eq!(along_pi.edge_count(), 2);
    for form in SchemaForm::ALL {
        if form.is_sigma() {
            let got = form.run(&plain, &source, &target, &clashing).unwrap();
            assert!(GInstance::identical(&got, &along_sigma), "{form:?}");
        } else {
            let got = form
                .run(&related_graph, &related, &related_target, &relating)
                .unwrap();
            assert!(GInstance::identical(&got, &along_pi), "{form:?}");
        }
    }
}

/// A source `a{x, w}` and `q` whose kept part is `a{x}`: the table `q` and
/// the column `w` are dropped. Returns the functor, the schemas and
/// migration it was built from, a whole-source graph with a node at `q` and
/// a value node at `a.w`, and that graph restricted to the kept part.
fn restriction_scenario() -> (
    MigrationFunctor,
    (Schema, Schema, CompiledMigration),
    GInstance,
    GInstance,
) {
    let x = edge("a", "a.x", "x");
    let w = edge("a", "a.w", "w");
    let target_x = edge("A", "A.x", "x");
    let source = schema(
        &[
            ("a", "record"),
            ("a.x", "int"),
            ("a.w", "int"),
            ("q", "record"),
        ],
        &[x.clone(), w.clone()],
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
        .with_node(Node::new(2, "a.w").with_value(FieldPresence::Present(Value::Int(9))))
        .with_node(Node::new(7, "q"))
        .with_edge_id(EdgeId(3), 0, 1, x.clone())
        .with_edge_id(EdgeId(4), 0, 2, w);
    let kept = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "a.x").with_value(FieldPresence::Present(Value::Int(4))))
        .with_edge_id(EdgeId(3), 0, 1, x);
    (functor, (source, target, migration), whole, kept)
}

#[test]
fn graph_restriction_commutes_with_table_domain_restriction_on_fixture() {
    let (functor, _, whole, kept) = restriction_scenario();
    let (whole_tables, _) = encode_graph(&whole, EncodingSide::Whole(functor.source())).unwrap();
    assert_eq!(whole_tables.rows("q").len(), 1);
    assert_eq!(
        whole_tables.rows("a")[0].1.get("w"),
        Some(&Value::Int(9)),
        "the whole table carries the dropped column"
    );
    let restricted = panproto_inst::f_restrict_to_domain(&whole_tables, &functor).unwrap();
    let (kept_tables, _) = encode_graph(&kept, EncodingSide::Domain(&functor)).unwrap();
    assert_eq!(restricted, kept_tables);
}

/// Sigma and Pi restrict, rather than refuse, a whole-source graph with a
/// node at a dropped table and a value node at a dropped column: every form
/// (along a functor, from schemas, with and without provenance) gives on the
/// whole graph exactly what it gives on the kept part.
#[test]
fn graph_sigma_and_pi_restrict_elements_outside_the_kept_part() {
    let (functor, (source, target, migration), whole, kept) = restriction_scenario();
    let sigma = g_sigma_along(&whole, &functor).unwrap();
    let pi = g_pi_along(&whole, &functor, 32).unwrap();
    for output in [&sigma, &pi] {
        assert_eq!(output.node_count(), 2);
        assert!(
            output
                .nodes()
                .values()
                .all(|node| node.anchor == "A" || node.anchor == "A.x")
        );
        assert!(
            output
                .nodes()
                .values()
                .all(|node| node.id != 2 && node.id != 7)
        );
    }
    assert!(GInstance::identical(
        &sigma,
        &g_sigma_restricted(&kept, &functor).unwrap()
    ));
    assert!(GInstance::identical(
        &pi,
        &g_pi_restricted(&kept, &functor, 32).unwrap()
    ));
    let from_schemas = g_sigma(
        &whole,
        &source,
        &protocol(),
        &target,
        &protocol(),
        &migration,
    )
    .unwrap();
    assert!(GInstance::identical(&sigma, &from_schemas));
    let (with_provenance, provenance) = g_sigma_with_provenance(
        &whole,
        &source,
        &protocol(),
        &target,
        &protocol(),
        &migration,
    )
    .unwrap();
    assert!(GInstance::identical(&sigma, &with_provenance));
    assert_eq!(
        provenance,
        g_sigma_along_with_provenance(&whole, &functor).unwrap().1
    );
    assert!(
        provenance
            .nodes
            .values()
            .flatten()
            .all(|input| *input != 2 && *input != 7)
    );
    let from_schemas = g_pi(
        &whole,
        &source,
        &protocol(),
        &target,
        &protocol(),
        &migration,
        32,
    )
    .unwrap();
    assert!(GInstance::identical(&pi, &from_schemas));
    let (with_provenance, provenance) = g_pi_with_provenance(
        &whole,
        &source,
        &protocol(),
        &target,
        &protocol(),
        &migration,
        32,
    )
    .unwrap();
    assert!(GInstance::identical(&pi, &with_provenance));
    assert_eq!(
        provenance,
        g_pi_along_with_provenance(&whole, &functor, 32).unwrap().1
    );
    assert!(
        provenance
            .nodes
            .values()
            .flatten()
            .all(|input| *input != 2 && *input != 7)
    );
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
        &wtype_extend_engine(
            &tree,
            &source,
            &target_with_column,
            &migration,
            BoundaryProtocols {
                src: &protocol(),
                tgt: &protocol(),
            },
        )
        .unwrap(),
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

    /// The five-pass domain, both halves: a graph of the encoding family
    /// with at most three table nodes, changed by up to three mutations
    /// (most of which leave `Enc(X)`), is accepted by `encode_graph` exactly
    /// when the oracle's passes accept it, and is otherwise refused with the
    /// oracle's first reason; an accepted graph encodes to the oracle's table
    /// and decodes back identically.
    #[test]
    fn encoding_accepts_exactly_the_five_pass_domain(
        base in any::<proptest::sample::Index>(),
        mutations in proptest::collection::vec((0..MUTATIONS, 0usize..16), 0..=3),
    ) {
        static BASES: std::sync::OnceLock<Vec<GInstance>> = std::sync::OnceLock::new();
        let bases = BASES.get_or_init(|| encoding_graphs(3));
        let mut graph = bases[base.index(bases.len())].clone();
        for (kind, pick) in mutations {
            graph = mutate(&graph, kind, pick);
        }
        assert_encoding_matches_the_oracle(&graph).ok();
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
        let (functor, _, mut whole, mut kept) = restriction_scenario();
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
            &wtype_extend_engine(
                &tree,
                &source,
                &target,
                &migration,
                BoundaryProtocols {
                    src: &protocol(),
                    tgt: &protocol(),
                },
            )
            .unwrap(),
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

/// Brute-force oracles for the encoding, the graph Kan homomorphisms and the
/// graph Kan adjunctions.
///
/// Nothing in this module calls `encode_graph`, `decode_graph`, a `g_*`
/// function or `GraphKanHom::check`: each side is written out by hand from
/// its test schema, membership of `Enc(X)` is decided by the five passes as
/// the contract states them, cells are read by a recursive fold written here,
/// and homomorphisms are enumerated from the graphs alone. The tests compare
/// the code under test with these answers.
///
/// Every enumerator here produces labelled instances: nodes carry the ids
/// `0..n` in table order, so two instances that differ only by a renaming of
/// ids are distinct when their ids differ, and every count asserted against an
/// enumerator is a labelled count, given by a closed formula over labelled
/// structures (never a count of isomorphism classes).
mod oracle {
    use std::collections::{BTreeMap, BTreeSet};

    use panproto_gat::Name;
    use panproto_inst::{
        EdgeId, FInstance, FieldPresence, GEdge, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND,
        Node, NodeShape, NotEncodableReason, Row, RowId, Value,
    };
    use panproto_schema::{Edge, Schema};

    use super::{edge, inner_edge, schema};

    /// A test schema written out by hand: its tables in order, each with its
    /// columns `(column, kind of the column's value vertex)`, its relations,
    /// and its inner value edges `(source vertex, target vertex, name, target
    /// kind)`, an empty name standing for an unnamed edge. The value vertex of column `c` of table `t` is `t.c`.
    #[derive(Clone, Debug, Default)]
    pub struct Spec {
        pub tables: Vec<(&'static str, Vec<(&'static str, &'static str)>)>,
        pub relations: Vec<Edge>,
        pub inner: Vec<(&'static str, &'static str, &'static str, &'static str)>,
    }

    impl Spec {
        /// The attribute arrow of column `column` of `table`.
        pub fn column_edge(table: &str, column: &str) -> Edge {
            edge(table, &format!("{table}.{column}"), column)
        }

        /// The inner value edges as schema edges.
        pub fn inner_edges(&self) -> Vec<Edge> {
            self.inner
                .iter()
                .map(|(src, tgt, name, _)| inner_edge(src, tgt, name))
                .collect()
        }

        /// The schema the code under test classifies.
        pub fn schema(&self) -> Schema {
            let mut vertices: Vec<(String, &str)> = Vec::new();
            let mut edges = Vec::new();
            for (table, columns) in &self.tables {
                vertices.push(((*table).to_owned(), "record"));
                for (column, kind) in columns {
                    vertices.push((format!("{table}.{column}"), kind));
                    edges.push(Self::column_edge(table, column));
                }
            }
            for (_, tgt, _, kind) in &self.inner {
                if vertices.iter().all(|(id, _)| id != tgt) {
                    vertices.push(((*tgt).to_owned(), kind));
                }
            }
            edges.extend(self.relations.iter().cloned());
            edges.extend(self.inner_edges());
            let named: Vec<(&str, &str)> = vertices
                .iter()
                .map(|(id, kind)| (id.as_str(), *kind))
                .collect();
            schema(&named, &edges)
        }

        /// The side the oracle reads, built from this description alone.
        pub fn side(&self) -> Side {
            let mut side = Side::default();
            for (table, columns) in &self.tables {
                let mut arrows = BTreeMap::new();
                for (column, _) in columns {
                    let arrow = Self::column_edge(table, column);
                    side.value_vertices.insert(arrow.tgt.clone());
                    arrows.insert((*column).to_owned(), arrow);
                }
                side.tables.insert(Name::from(*table), arrows);
            }
            side.relations = self.relations.iter().cloned().collect();
            for inner in self.inner_edges() {
                side.value_vertices.insert(inner.src.clone());
                side.value_vertices.insert(inner.tgt.clone());
                side.inner.insert(inner);
            }
            side
        }
    }

    /// The objects and arrows of one encoding side.
    #[derive(Clone, Debug, Default)]
    pub struct Side {
        pub tables: BTreeMap<Name, BTreeMap<String, Edge>>,
        pub relations: BTreeSet<Edge>,
        pub value_vertices: BTreeSet<Name>,
        pub inner: BTreeSet<Edge>,
    }

    impl Side {
        fn is_attribute(&self, label: &Edge) -> bool {
            self.tables
                .get(&label.src)
                .is_some_and(|columns| columns.values().any(|arrow| arrow == label))
        }

        fn has_label(&self, label: &Edge) -> bool {
            self.relations.contains(label) || self.is_attribute(label) || self.inner.contains(label)
        }
    }

    fn in_edges(graph: &GInstance, node: u32) -> Vec<EdgeId> {
        let mut edges: Vec<EdgeId> = graph
            .edges()
            .iter()
            .filter(|(_, edge)| edge.tgt == node)
            .map(|(id, _)| *id)
            .collect();
        edges.sort_unstable();
        edges
    }

    fn out_edges(graph: &GInstance, node: u32) -> Vec<(EdgeId, &GEdge)> {
        let mut edges: Vec<(EdgeId, &GEdge)> = graph
            .edges()
            .iter()
            .filter(|(_, edge)| edge.src == node)
            .map(|(id, edge)| (*id, edge))
            .collect();
        edges.sort_unstable_by_key(|(id, _)| *id);
        edges
    }

    fn sorted_nodes(graph: &GInstance) -> Vec<&Node> {
        let mut nodes: Vec<&Node> = graph.nodes().values().collect();
        nodes.sort_unstable_by_key(|node| node.id);
        nodes
    }

    fn sorted_edges(graph: &GInstance) -> Vec<(EdgeId, &GEdge)> {
        let mut edges: Vec<(EdgeId, &GEdge)> = graph.edges().iter().map(|(i, e)| (*i, e)).collect();
        edges.sort_unstable_by_key(|(id, _)| *id);
        edges
    }

    /// Whether `value` holds a labelled null anywhere, itself included.
    pub fn any_null(value: &Value) -> bool {
        match value {
            Value::LabeledNull(_) => true,
            Value::List(items) => items.iter().any(any_null),
            Value::Unknown(fields) | Value::Opaque { fields, .. } => fields.values().any(any_null),
            _ => false,
        }
    }

    /// Whether `value` holds a labelled null strictly below its top level.
    pub fn nested_null(value: &Value) -> bool {
        match value {
            Value::List(items) => items.iter().any(any_null),
            Value::Unknown(fields) | Value::Opaque { fields, .. } => fields.values().any(any_null),
            _ => false,
        }
    }

    fn is_fan(graph: &GInstance, node: &Node) -> bool {
        node.shape == NodeShape::HyperEdge
            || out_edges(graph, node.id).iter().any(|(_, edge)| {
                edge.edge.kind.as_ref() == HYPER_PARENT_KIND
                    || edge.edge.kind.as_ref() == HYPER_LABEL_KIND
            })
    }

    fn pass_nodes(graph: &GInstance, side: &Side) -> Result<(), NotEncodableReason> {
        for node in sorted_nodes(graph) {
            if is_fan(graph, node) {
                return Err(NotEncodableReason::FanNode { node: node.id });
            }
            if !side.tables.contains_key(&node.anchor)
                && !side.value_vertices.contains(&node.anchor)
            {
                return Err(NotEncodableReason::AnchorOutsideSide { node: node.id });
            }
        }
        Ok(())
    }

    fn pass_edges(graph: &GInstance, side: &Side) -> Result<(), NotEncodableReason> {
        for (id, edge) in sorted_edges(graph) {
            if !side.has_label(&edge.edge) {
                return Err(NotEncodableReason::EdgeOutsideSide { edge: id });
            }
            let anchor = |node: u32| graph.node(node).map(|n| n.anchor.clone());
            if anchor(edge.src) != Some(edge.edge.src.clone())
                || anchor(edge.tgt) != Some(edge.edge.tgt.clone())
            {
                return Err(NotEncodableReason::IncoherentEndpoints { edge: id });
            }
        }
        Ok(())
    }

    fn pass_parallel(graph: &GInstance) -> Result<(), NotEncodableReason> {
        let mut groups: BTreeMap<(Edge, u32, u32), Vec<EdgeId>> = BTreeMap::new();
        for (id, edge) in sorted_edges(graph) {
            groups
                .entry((edge.edge.clone(), edge.src, edge.tgt))
                .or_default()
                .push(id);
        }
        let mut parallel: Vec<Vec<EdgeId>> = groups
            .into_values()
            .filter(|group| group.len() > 1)
            .collect();
        parallel.sort_unstable_by_key(|group| group[0]);
        parallel.into_iter().next().map_or(Ok(()), |edges| {
            Err(NotEncodableReason::ParallelEdges { edges })
        })
    }

    fn pass_cells(graph: &GInstance, side: &Side) -> Result<(), NotEncodableReason> {
        for node in sorted_nodes(graph) {
            let Some(columns) = side.tables.get(&node.anchor) else {
                continue;
            };
            for (column, arrow) in columns {
                let edges: Vec<EdgeId> = out_edges(graph, node.id)
                    .into_iter()
                    .filter(|(_, edge)| edge.edge == *arrow)
                    .map(|(id, _)| id)
                    .collect();
                match edges.len() {
                    0 if node.extra_fields.contains_key(column) => {
                        return Err(NotEncodableReason::CellAsExtraField {
                            node: node.id,
                            column: column.clone(),
                        });
                    }
                    0 => {
                        return Err(NotEncodableReason::MissingCell {
                            node: node.id,
                            column: column.clone(),
                        });
                    }
                    1 => {}
                    _ => {
                        return Err(NotEncodableReason::RepeatedCell {
                            node: node.id,
                            column: column.clone(),
                            edges,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Whether following in-edges up from `node` (the least in-edge at each
    /// step) reaches a table node before it revisits a node or stops.
    fn rooted(graph: &GInstance, side: &Side, node: u32) -> bool {
        let mut seen = BTreeSet::new();
        let mut current = node;
        loop {
            if !seen.insert(current) {
                return false;
            }
            let Some(first) = in_edges(graph, current).first().copied() else {
                return false;
            };
            let parent = graph.edges()[&first].src;
            let Some(anchor) = graph.node(parent).map(|n| &n.anchor) else {
                return false;
            };
            if side.tables.contains_key(anchor) {
                return true;
            }
            current = parent;
        }
    }

    /// The name a record's inner edge gives its field, as Spec ruling S3 (1)
    /// (b) and (c) define it: the edge's name, or its target vertex when it
    /// has none.
    pub fn name_of(edge: &Edge) -> String {
        edge.name
            .as_ref()
            .map_or_else(|| edge.tgt.to_string(), ToString::to_string)
    }

    fn check_value_node(
        graph: &GInstance,
        side: &Side,
        node: &Node,
    ) -> Result<(), NotEncodableReason> {
        let incoming = in_edges(graph, node.id);
        match incoming.len() {
            0 => return Err(NotEncodableReason::UnrootedValue { node: node.id }),
            1 => {}
            _ => {
                return Err(NotEncodableReason::SharedValue {
                    node: node.id,
                    edges: incoming,
                });
            }
        }
        if !rooted(graph, side, node.id) {
            return Err(NotEncodableReason::UnrootedValue { node: node.id });
        }
        let below_top = side.inner.contains(&graph.edges()[&incoming[0]].edge);
        let outgoing = out_edges(graph, node.id);
        if outgoing.is_empty() {
            return match &node.value {
                Some(FieldPresence::Present(value)) => {
                    if nested_null(value) || (below_top && any_null(value)) {
                        Err(NotEncodableReason::NestedLabeledNull { node: node.id })
                    } else {
                        Ok(())
                    }
                }
                Some(FieldPresence::Null) => Ok(()),
                None | Some(FieldPresence::Absent) => {
                    Err(NotEncodableReason::MissingValue { node: node.id })
                }
            };
        }
        let positions: Vec<Option<u32>> = outgoing.iter().map(|(_, e)| e.position).collect();
        let names: BTreeSet<String> = outgoing.iter().map(|(_, e)| name_of(&e.edge)).collect();
        let record = positions.iter().all(Option::is_none) && names.len() == outgoing.len();
        let list = positions.iter().all(Option::is_some)
            && positions.iter().collect::<BTreeSet<_>>().len() == outgoing.len();
        if node.value.is_some() || !(record || list) {
            return Err(NotEncodableReason::InnerValueShape { node: node.id });
        }
        Ok(())
    }

    /// Membership of `Enc(X)`: the five passes in order, each returning its
    /// least offender.
    pub fn classify(graph: &GInstance, side: &Side) -> Result<(), NotEncodableReason> {
        pass_nodes(graph, side)?;
        pass_edges(graph, side)?;
        pass_parallel(graph)?;
        pass_cells(graph, side)?;
        for node in sorted_nodes(graph) {
            if side.value_vertices.contains(&node.anchor) {
                check_value_node(graph, side, node)?;
            }
        }
        Ok(())
    }

    /// The cell a value node folds to: its value at a leaf, a list of its
    /// children's cells by ascending position when its out-edges carry
    /// positions, and a record keyed by the [`name_of`] each edge otherwise.
    pub fn fold(graph: &GInstance, node: u32) -> Value {
        let outgoing = out_edges(graph, node);
        if outgoing.is_empty() {
            return match &graph.nodes()[&node].value {
                Some(FieldPresence::Present(value)) => value.clone(),
                _ => Value::Null,
            };
        }
        if outgoing.iter().all(|(_, e)| e.position.is_some()) {
            let mut items: Vec<(u32, u32)> = outgoing
                .iter()
                .map(|(_, e)| (e.position.unwrap_or(0), e.tgt))
                .collect();
            items.sort_unstable();
            return Value::List(items.into_iter().map(|(_, t)| fold(graph, t)).collect());
        }
        Value::Unknown(
            outgoing
                .iter()
                .map(|(_, e)| (name_of(&e.edge), fold(graph, e.tgt)))
                .collect(),
        )
    }

    /// The relational content of an encodable graph, read from the graph
    /// alone: each table node with its table and its folded cells, and each
    /// relation edge as a labelled pair.
    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct Content {
        pub rows: BTreeMap<u32, (Name, BTreeMap<String, Value>)>,
        pub pairs: BTreeSet<(Edge, u32, u32)>,
    }

    impl Content {
        /// The content of `graph`, which must lie in `Enc(side)`.
        pub fn of_graph(graph: &GInstance, side: &Side) -> Self {
            let mut content = Self::default();
            for node in sorted_nodes(graph) {
                let Some(columns) = side.tables.get(&node.anchor) else {
                    continue;
                };
                let cells = columns
                    .iter()
                    .filter_map(|(column, arrow)| {
                        out_edges(graph, node.id)
                            .into_iter()
                            .find(|(_, e)| e.edge == *arrow)
                            .map(|(_, e)| (column.clone(), fold(graph, e.tgt)))
                    })
                    .collect();
                content.rows.insert(node.id, (node.anchor.clone(), cells));
            }
            for (_, e) in sorted_edges(graph) {
                if side.relations.contains(&e.edge) {
                    content.pairs.insert((e.edge.clone(), e.src, e.tgt));
                }
            }
            content
        }

        /// The content of a table instance.
        pub fn of_table(table: &FInstance) -> Self {
            let mut content = Self::default();
            for id in table.row_ids() {
                let Some(name) = table.table_of(id) else {
                    continue;
                };
                let cells = table
                    .row(id)
                    .map(|row| row.iter().map(|(c, v)| (c.clone(), v.clone())).collect())
                    .unwrap_or_default();
                content.rows.insert(id.0, (name.clone(), cells));
            }
            for (relation, pairs) in table.foreign_keys() {
                for (src, tgt) in pairs {
                    content.pairs.insert((relation.clone(), src.0, tgt.0));
                }
            }
            content
        }

        /// The table instance with this content: one row per table node,
        /// keyed by the node's id, and one pair per relation edge.
        pub fn table(&self) -> FInstance {
            let mut table = FInstance::new();
            for (id, (name, cells)) in &self.rows {
                let row: Row = cells.iter().map(|(c, v)| (c.clone(), v.clone())).collect();
                table
                    .insert_row(name.clone(), RowId(*id), row)
                    .unwrap_or_else(|error| panic!("{error}"));
            }
            for (relation, src, tgt) in &self.pairs {
                table
                    .insert_foreign_key(relation.clone(), RowId(*src), RowId(*tgt))
                    .unwrap_or_else(|error| panic!("{error}"));
            }
            table
        }

        /// The labelled nulls held at the top level of a cell.
        pub fn nulls(&self) -> BTreeSet<u64> {
            self.rows
                .values()
                .flat_map(|(_, cells)| cells.values())
                .filter_map(|value| match value {
                    Value::LabeledNull(null) => Some(*null),
                    _ => None,
                })
                .collect()
        }
    }

    /// The canonical bytes of a value, the equality every oracle uses.
    pub fn bytes(value: &Value) -> Vec<u8> {
        value.canonical_bytes()
    }

    /// Whether `table` conforms to `side`: every row lies at a table of the
    /// side with exactly its columns, no cell holds a nested labelled null,
    /// and every pair lies at a relation of the side between rows of its two
    /// tables. Returns the first violation found, in words.
    pub fn conformance(table: &FInstance, side: &Side) -> Result<(), String> {
        for id in table.row_ids() {
            let name = table.table_of(id).cloned().unwrap_or_default();
            let Some(columns) = side.tables.get(&name) else {
                return Err(format!("row {} lies at {name}, outside the side", id.0));
            };
            let row = table.row(id).cloned().unwrap_or_default();
            let held: BTreeSet<&String> = row.keys().collect();
            let wanted: BTreeSet<&String> = columns.keys().collect();
            if held != wanted {
                return Err(format!("row {} holds {held:?}, not {wanted:?}", id.0));
            }
            if let Some((column, _)) = row.iter().find(|(_, value)| nested_null(value)) {
                return Err(format!("row {} holds a nested null at {column}", id.0));
            }
        }
        for (relation, pairs) in table.foreign_keys() {
            if pairs.is_empty() {
                continue;
            }
            if !side.relations.contains(relation) {
                return Err(format!("pairs at {relation:?}, outside the side"));
            }
            for (src, tgt) in pairs {
                if table.table_of(*src) != Some(&relation.src)
                    || table.table_of(*tgt) != Some(&relation.tgt)
                {
                    return Err(format!("pair ({}, {}) at {relation:?}", src.0, tgt.0));
                }
            }
        }
        Ok(())
    }

    /// A homomorphism between contents: the image of every table node of the
    /// domain, and the value of every labelled null of the domain that does
    /// not go to itself, by canonical bytes.
    #[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
    pub struct Hom {
        pub nodes: BTreeMap<u32, u32>,
        pub nulls: BTreeMap<u64, Vec<u8>>,
    }

    impl Hom {
        /// The identity on `content`.
        pub fn identity(content: &Content) -> Self {
            Self {
                nodes: content.rows.keys().map(|id| (*id, *id)).collect(),
                nulls: BTreeMap::new(),
            }
        }

        /// A hom with node map `nodes` and the null map `nulls` restricted to
        /// the nulls of `domain`, without identity entries.
        pub fn normalised(
            nodes: &BTreeMap<u32, u32>,
            nulls: &BTreeMap<u64, Value>,
            domain: &Content,
        ) -> Self {
            let held = domain.nulls();
            Self {
                nodes: nodes
                    .iter()
                    .filter(|(node, _)| domain.rows.contains_key(node))
                    .map(|(a, b)| (*a, *b))
                    .collect(),
                nulls: nulls
                    .iter()
                    .filter(|(null, value)| {
                        held.contains(null) && bytes(value) != bytes(&Value::LabeledNull(**null))
                    })
                    .map(|(null, value)| (*null, bytes(value)))
                    .collect(),
            }
        }

        /// `self: A -> B` followed by `other: B -> C`, where `domain` is `A`:
        /// node maps compose, and each null of `A` goes where `other` sends
        /// its image under `self` (a constant stays a constant).
        pub fn then(&self, other: &Self, domain: &Content) -> Self {
            let nodes = self
                .nodes
                .iter()
                .filter_map(|(a, b)| other.nodes.get(b).map(|c| (*a, *c)))
                .collect();
            let mut nulls = BTreeMap::new();
            for null in domain.nulls() {
                let own = bytes(&Value::LabeledNull(null));
                let middle = self
                    .nulls
                    .get(&null)
                    .cloned()
                    .unwrap_or_else(|| own.clone());
                let image = other
                    .nulls
                    .iter()
                    .find(|(m, _)| bytes(&Value::LabeledNull(**m)) == middle)
                    .map_or(middle, |(_, value)| value.clone());
                if image != own {
                    nulls.insert(null, image);
                }
            }
            Self { nodes, nulls }
        }
    }

    /// Extend `nulls` by sending the cells of `cells` to those of `image`:
    /// the key sets must be equal, the first occurrence of a null fixes its
    /// value, and every later occurrence and every constant must agree by
    /// canonical bytes.
    fn extend_nulls(
        cells: &BTreeMap<String, Value>,
        image: &BTreeMap<String, Value>,
        nulls: &mut BTreeMap<u64, Vec<u8>>,
    ) -> bool {
        if cells.len() != image.len() {
            return false;
        }
        for (column, value) in cells {
            let Some(target) = image.get(column) else {
                return false;
            };
            let target = bytes(target);
            match value {
                Value::LabeledNull(null) => match nulls.get(null) {
                    Some(fixed) if *fixed != target => return false,
                    Some(_) => {}
                    None => {
                        nulls.insert(*null, target);
                    }
                },
                other if bytes(other) != target => return false,
                _ => {}
            }
        }
        true
    }

    fn pairs_kept(dom: &Content, cod: &Content, nodes: &BTreeMap<u32, u32>, row: u32) -> bool {
        dom.pairs
            .iter()
            .filter(|(_, src, tgt)| *src == row || *tgt == row)
            .all(
                |(relation, src, tgt)| match (nodes.get(src), nodes.get(tgt)) {
                    (Some(a), Some(b)) => cod.pairs.contains(&(relation.clone(), *a, *b)),
                    _ => true,
                },
            )
    }

    fn search(
        dom: &Content,
        cod: &Content,
        order: &[u32],
        partial: &mut (BTreeMap<u32, u32>, BTreeMap<u64, Vec<u8>>),
        out: &mut Vec<Hom>,
    ) {
        let Some((&row, rest)) = order.split_first() else {
            let nulls = partial
                .1
                .iter()
                .filter(|(null, value)| **value != bytes(&Value::LabeledNull(**null)))
                .map(|(null, value)| (*null, value.clone()))
                .collect();
            out.push(Hom {
                nodes: partial.0.clone(),
                nulls,
            });
            return;
        };
        let (table, cells) = &dom.rows[&row];
        for (candidate, (image_table, image_cells)) in &cod.rows {
            if image_table != table {
                continue;
            }
            let mut nulls = partial.1.clone();
            if !extend_nulls(cells, image_cells, &mut nulls) {
                continue;
            }
            partial.0.insert(row, *candidate);
            if pairs_kept(dom, cod, &partial.0, row) {
                let saved = std::mem::replace(&mut partial.1, nulls);
                search(dom, cod, rest, partial, out);
                partial.1 = saved;
            }
            partial.0.remove(&row);
        }
    }

    /// Every homomorphism `dom -> cod`: every function on table nodes that
    /// keeps each node's table, keeps its cells (nulls fixed at their first
    /// occurrence) and sends relation pairs to relation pairs. Nodes are
    /// assigned in ascending id, and a partial map that already fails is not
    /// extended, which gives exactly the maps a scan of every complete map
    /// accepts.
    pub fn homs(dom: &Content, cod: &Content) -> Vec<Hom> {
        let order: Vec<u32> = dom.rows.keys().copied().collect();
        let mut out = Vec::new();
        search(
            dom,
            cod,
            &order,
            &mut (BTreeMap::new(), BTreeMap::new()),
            &mut out,
        );
        out
    }

    /// Whether `hom` is a homomorphism `dom -> cod`, read off the definition.
    pub fn is_hom(dom: &Content, cod: &Content, hom: &Hom) -> bool {
        if hom.nodes.keys().copied().collect::<BTreeSet<_>>()
            != dom.rows.keys().copied().collect::<BTreeSet<_>>()
        {
            return false;
        }
        let image = |value: &Value| match value {
            Value::LabeledNull(null) => {
                hom.nulls.get(null).cloned().unwrap_or_else(|| bytes(value))
            }
            other => bytes(other),
        };
        dom.rows.iter().all(|(row, (table, cells))| {
            cod.rows
                .get(&hom.nodes[row])
                .is_some_and(|(image_table, image_cells)| {
                    image_table == table
                        && cells.len() == image_cells.len()
                        && cells.iter().all(|(column, value)| {
                            image_cells
                                .get(column)
                                .is_some_and(|target| bytes(target) == image(value))
                        })
                })
        }) && dom.pairs.iter().all(|(relation, src, tgt)| {
            cod.pairs
                .contains(&(relation.clone(), hom.nodes[src], hom.nodes[tgt]))
        })
    }

    /// Whether `a` and `b` are isomorphic: some homomorphism `a -> b` is a
    /// bijection on table nodes and sends the nulls of `a` bijectively onto
    /// those of `b`, and its inverse is a homomorphism `b -> a`.
    pub fn isomorphic(a: &Content, b: &Content) -> bool {
        if a.rows.len() != b.rows.len() || a.pairs.len() != b.pairs.len() {
            return false;
        }
        let b_nulls = b.nulls();
        homs(a, b).into_iter().any(|hom| {
            let inverse_nodes: BTreeMap<u32, u32> =
                hom.nodes.iter().map(|(x, y)| (*y, *x)).collect();
            if inverse_nodes.len() != hom.nodes.len() {
                return false;
            }
            let mut inverse_nulls = BTreeMap::new();
            for null in a.nulls() {
                let image = hom
                    .nulls
                    .get(&null)
                    .cloned()
                    .unwrap_or_else(|| bytes(&Value::LabeledNull(null)));
                let Some(target) = b_nulls
                    .iter()
                    .find(|n| bytes(&Value::LabeledNull(**n)) == image)
                else {
                    return false;
                };
                if inverse_nulls
                    .insert(*target, bytes(&Value::LabeledNull(null)))
                    .is_some()
                {
                    return false;
                }
            }
            if inverse_nulls.len() != b_nulls.len() {
                return false;
            }
            inverse_nulls.retain(|n, v| *v != bytes(&Value::LabeledNull(*n)));
            is_hom(
                b,
                a,
                &Hom {
                    nodes: inverse_nodes,
                    nulls: inverse_nulls,
                },
            )
        })
    }

    /// The graph form of one cell: a leaf value node, or a value node whose
    /// children are leaves along named inner edges, unpositioned for a record
    /// and positioned `0, 1, ..` for a list. A child is `(edge name, target
    /// vertex, value)`, an empty name standing for an unnamed edge.
    #[derive(Clone, Debug)]
    pub enum Form {
        Leaf(FieldPresence),
        Record(Vec<(&'static str, &'static str, Value)>),
        List(Vec<(&'static str, &'static str, Value)>),
    }

    /// Every element of the cartesian product of `choices`, the first
    /// coordinate varying slowest.
    pub fn product<T: Clone>(choices: &[Vec<T>]) -> Vec<Vec<T>> {
        let mut out: Vec<Vec<T>> = vec![Vec::new()];
        for options in choices {
            out = out
                .into_iter()
                .flat_map(|prefix| {
                    options.iter().map(move |option| {
                        let mut next = prefix.clone();
                        next.push(option.clone());
                        next
                    })
                })
                .collect();
        }
        out
    }

    /// Every vector of per-table node counts with sum at most `max`.
    pub fn compositions(tables: usize, max: usize) -> Vec<Vec<usize>> {
        product(&vec![(0..=max).collect::<Vec<_>>(); tables])
            .into_iter()
            .filter(|counts| counts.iter().sum::<usize>() <= max)
            .collect()
    }

    fn add_cell(
        graph: &mut GInstance,
        next: &mut (u32, u32),
        owner: u32,
        arrow: &Edge,
        form: &Form,
    ) {
        let root = next.0;
        next.0 += 1;
        let (value, children, positioned) = match form {
            Form::Leaf(presence) => (Some(presence.clone()), &[][..], false),
            Form::Record(children) => (None, &children[..], false),
            Form::List(children) => (None, &children[..], true),
        };
        let mut node = Node::new(root, arrow.tgt.clone());
        node.value = value;
        graph.insert_node(node).unwrap_or_else(|e| panic!("{e}"));
        graph
            .insert_edge(EdgeId(next.1), GEdge::new(owner, root, arrow.clone()))
            .unwrap_or_else(|e| panic!("{e}"));
        next.1 += 1;
        for (position, (name, vertex, value)) in children.iter().enumerate() {
            let child = next.0;
            next.0 += 1;
            graph
                .insert_node(
                    Node::new(child, *vertex).with_value(FieldPresence::Present(value.clone())),
                )
                .unwrap_or_else(|e| panic!("{e}"));
            let label = inner_edge(&arrow.tgt, vertex, name);
            let position = positioned.then(|| u32::try_from(position).unwrap_or(u32::MAX));
            graph
                .insert_edge(
                    EdgeId(next.1),
                    GEdge::new(root, child, label).with_position(position),
                )
                .unwrap_or_else(|e| panic!("{e}"));
            next.1 += 1;
        }
    }

    /// Every labelled graph of `Enc(spec)` with at most `max` table nodes:
    /// the table nodes carry ids `0..n` in table order, every cell takes
    /// every form `forms(table, column)` lists, and every relation takes
    /// every subset of the pairs between its tables' nodes. Value nodes and
    /// edges take ids in order after them.
    pub fn every_graph(
        spec: &Spec,
        forms: &dyn Fn(&str, &str) -> Vec<Form>,
        max: usize,
    ) -> Vec<GInstance> {
        let mut out = Vec::new();
        for counts in compositions(spec.tables.len(), max) {
            let mut slots: Vec<(u32, &str, &str)> = Vec::new();
            let mut owners: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
            let mut id = 0u32;
            for ((table, columns), count) in spec.tables.iter().zip(&counts) {
                for _ in 0..*count {
                    owners.entry(table).or_default().push(id);
                    for (column, _) in columns {
                        slots.push((id, table, column));
                    }
                    id += 1;
                }
            }
            let nodes = id;
            let cell_choices: Vec<Vec<Form>> = slots.iter().map(|(_, t, c)| forms(t, c)).collect();
            let pair_choices: Vec<Vec<Vec<(u32, u32)>>> = spec
                .relations
                .iter()
                .map(|relation| {
                    let none = Vec::new();
                    let possible: Vec<(u32, u32)> = product(&[
                        owners.get(relation.src.as_ref()).unwrap_or(&none).clone(),
                        owners.get(relation.tgt.as_ref()).unwrap_or(&none).clone(),
                    ])
                    .into_iter()
                    .map(|pair| (pair[0], pair[1]))
                    .collect();
                    (0u32..(1u32 << possible.len()))
                        .map(|mask| {
                            possible
                                .iter()
                                .enumerate()
                                .filter(|(k, _)| mask & (1 << k) != 0)
                                .map(|(_, pair)| *pair)
                                .collect()
                        })
                        .collect()
                })
                .collect();
            for cells in product(&cell_choices) {
                for pairs in product(&pair_choices) {
                    let mut graph = GInstance::new();
                    for (table, _) in &spec.tables {
                        for node in owners.get(table).into_iter().flatten() {
                            graph
                                .insert_node(Node::new(*node, *table))
                                .unwrap_or_else(|e| panic!("{e}"));
                        }
                    }
                    let mut next = (nodes, 0u32);
                    for ((owner, table, column), form) in slots.iter().zip(&cells) {
                        add_cell(
                            &mut graph,
                            &mut next,
                            *owner,
                            &Spec::column_edge(table, column),
                            form,
                        );
                    }
                    for (relation, chosen) in spec.relations.iter().zip(&pairs) {
                        for (src, tgt) in chosen {
                            graph
                                .insert_edge(
                                    EdgeId(next.1),
                                    GEdge::new(*src, *tgt, relation.clone()),
                                )
                                .unwrap_or_else(|e| panic!("{e}"));
                            next.1 += 1;
                        }
                    }
                    out.push(graph);
                }
            }
        }
        out
    }

    /// The labelled count of [`every_graph`], by the closed formula: the sum,
    /// over every vector `k` of per-table node counts with `|k| <= max`, of
    /// `prod_t (prod_c |forms(t, c)|)^(k_t) * 2^(sum over relations e: s -> t
    /// of k_s * k_t)`.
    pub fn labelled_count(spec: &Spec, forms: &dyn Fn(&str, &str) -> usize, max: usize) -> usize {
        let index = |name: &str| spec.tables.iter().position(|(t, _)| *t == name);
        compositions(spec.tables.len(), max)
            .into_iter()
            .map(|counts| {
                let cells: usize = spec
                    .tables
                    .iter()
                    .zip(&counts)
                    .map(|((table, columns), k)| {
                        columns
                            .iter()
                            .map(|(column, _)| forms(table, column))
                            .product::<usize>()
                            .pow(u32::try_from(*k).unwrap_or(u32::MAX))
                    })
                    .product();
                let pairs: usize = spec
                    .relations
                    .iter()
                    .map(|r| {
                        index(r.src.as_ref()).map_or(0, |s| counts[s])
                            * index(r.tgt.as_ref()).map_or(0, |t| counts[t])
                    })
                    .sum();
                cells << pairs
            })
            .sum()
    }

    /// `graph` with every node id `n` renamed to `node(n)` and every edge id
    /// `e` to `edge(e)`.
    pub fn relabel(
        graph: &GInstance,
        node: impl Fn(u32) -> u32,
        edge: impl Fn(u32) -> u32,
    ) -> GInstance {
        let mut out = GInstance::new();
        for old in sorted_nodes(graph) {
            let mut renamed = old.clone();
            renamed.id = node(old.id);
            out.insert_node(renamed).unwrap_or_else(|e| panic!("{e}"));
        }
        for (id, old) in sorted_edges(graph) {
            let renamed = GEdge::new(node(old.src), node(old.tgt), old.edge.clone())
                .with_position(old.position);
            out.insert_edge(EdgeId(edge(id.0)), renamed)
                .unwrap_or_else(|e| panic!("{e}"));
        }
        out
    }

    /// `graph` with a decoration on every node (discriminator, position,
    /// annotation and an extra field that names no column) and a position on
    /// every edge that is not an inner edge.
    pub fn decorate(graph: &GInstance, side: &Side) -> GInstance {
        let mut out = GInstance::new();
        for old in sorted_nodes(graph) {
            let mut node = old.clone();
            node.discriminator = Some(node.anchor.clone());
            node.position = Some(node.id % 5);
            node.annotations
                .insert("seen".into(), Value::Int(i64::from(node.id)));
            node.extra_fields
                .insert("note".into(), Value::Str(format!("n{}", node.id)));
            out.insert_node(node).unwrap_or_else(|e| panic!("{e}"));
        }
        for (id, old) in sorted_edges(graph) {
            let position = if side.inner.contains(&old.edge) {
                old.position
            } else {
                Some(id.0 + 2)
            };
            let renamed = GEdge::new(old.src, old.tgt, old.edge.clone()).with_position(position);
            out.insert_edge(id, renamed)
                .unwrap_or_else(|e| panic!("{e}"));
        }
        out
    }

    /// The restriction `rho G` of a whole-source graph to the kept side: the
    /// nodes at a table or value vertex of `kept`, and the edges between
    /// them whose label is an arrow of `kept`. A value node at a dropped
    /// column and a node at a dropped table go, with every edge they meet.
    pub fn restrict(graph: &GInstance, kept: &Side) -> GInstance {
        let mut out = GInstance::new();
        for node in sorted_nodes(graph) {
            if kept.tables.contains_key(&node.anchor) || kept.value_vertices.contains(&node.anchor)
            {
                out.insert_node(node.clone())
                    .unwrap_or_else(|e| panic!("{e}"));
            }
        }
        for (id, e) in sorted_edges(graph) {
            if kept.has_label(&e.edge) && out.node(e.src).is_some() && out.node(e.tgt).is_some() {
                out.insert_edge(id, e.clone())
                    .unwrap_or_else(|e| panic!("{e}"));
            }
        }
        out
    }

    /// A functor `F: S' -> T` given by its maps on kept tables, kept columns
    /// `(table, column) -> column` and kept relations.
    #[derive(Clone, Debug, Default)]
    pub struct Map {
        pub tables: BTreeMap<Name, Name>,
        pub columns: BTreeMap<(Name, String), String>,
        pub relations: BTreeMap<Edge, Edge>,
    }

    /// The Delta oracle: for each table node `r` of `target` and each table
    /// `s` of `kept` that `map` sends to `r`'s table, one copy of `r` at `s`
    /// holding, at each column `c` of `s`, the cell of `r` at `F(c)`; and for
    /// each kept relation `e` and each pair of `F(e)`, the pair of copies at
    /// `e`'s endpoints. Returns the content and each copy's `(s, r)`.
    pub fn delta(
        target: &Content,
        map: &Map,
        kept: &Side,
    ) -> (Content, BTreeMap<u32, (Name, u32)>) {
        let mut content = Content::default();
        let mut copies = BTreeMap::new();
        let mut index = BTreeMap::new();
        let mut next = 0u32;
        for (row, (table, cells)) in &target.rows {
            for (source, columns) in &kept.tables {
                if map.tables.get(source) != Some(table) {
                    continue;
                }
                let held = columns
                    .keys()
                    .map(|column| {
                        let image = &map.columns[&(source.clone(), column.clone())];
                        (column.clone(), cells[image].clone())
                    })
                    .collect();
                content.rows.insert(next, (source.clone(), held));
                copies.insert(next, (source.clone(), *row));
                index.insert((source.clone(), *row), next);
                next += 1;
            }
        }
        for relation in &kept.relations {
            let image = &map.relations[relation];
            for (label, src, tgt) in &target.pairs {
                if label != image {
                    continue;
                }
                if let (Some(a), Some(b)) = (
                    index.get(&(relation.src.clone(), *src)),
                    index.get(&(relation.tgt.clone(), *tgt)),
                ) {
                    content.pairs.insert((relation.clone(), *a, *b));
                }
            }
        }
        (content, copies)
    }

    /// Delta on a homomorphism `k: J1 -> J2`, given each copy's `(s, r)` in
    /// `Delta J1` and in `Delta J2`: the copy of `r` at `s` goes to the copy
    /// of `k(r)` at `s`, and the nulls go where `k` sends them.
    pub fn delta_on_hom(
        hom: &Hom,
        from: &BTreeMap<u32, (Name, u32)>,
        to: &BTreeMap<u32, (Name, u32)>,
    ) -> Hom {
        let index: BTreeMap<(Name, u32), u32> =
            to.iter().map(|(copy, key)| (key.clone(), *copy)).collect();
        Hom {
            nodes: from
                .iter()
                .map(|(copy, (table, row))| (*copy, index[&(table.clone(), hom.nodes[row])]))
                .collect(),
            nulls: hom.nulls.clone(),
        }
    }
}

/// Run `check` on every element of `items`, spread over the machine's
/// threads.
fn par<T: Sync>(items: &[T], check: impl Fn(&T) + Sync) {
    let threads = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
    let chunk = items.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        for part in items.chunks(chunk) {
            let check = &check;
            scope.spawn(move || part.iter().for_each(check));
        }
    });
}

/// The encoding family: a table `a` with one structured column `x`, a table
/// `b` without columns and a relation `f: a -> b`.
fn encoding_spec() -> oracle::Spec {
    oracle::Spec {
        tables: vec![("a", vec![("x", "value")]), ("b", vec![])],
        relations: vec![edge("a", "b", "f")],
        inner: vec![
            ("a.x", "a.x.k", "k", "str"),
            ("a.x", "a.x.item", "item", "int"),
        ],
    }
}

/// The seven graph forms of the encoding family's cell: a constant leaf,
/// both null forms, a record subtree, a list subtree, a leaf holding the
/// list that subtree folds to, and a top-level labelled null.
fn encoding_forms() -> Vec<oracle::Form> {
    use oracle::Form;
    vec![
        Form::Leaf(FieldPresence::Present(Value::Int(0))),
        Form::Leaf(FieldPresence::Null),
        Form::Leaf(FieldPresence::Present(Value::Null)),
        Form::Record(vec![("k", "a.x.k", Value::Str("s".into()))]),
        Form::List(vec![
            ("item", "a.x.item", Value::Int(0)),
            ("item", "a.x.item", Value::Int(1)),
        ]),
        Form::Leaf(FieldPresence::Present(Value::List(vec![
            Value::Int(0),
            Value::Int(1),
        ]))),
        Form::Leaf(FieldPresence::Present(Value::LabeledNull(0))),
    ]
}

/// Every graph of the encoding family with at most four table nodes, in four
/// presentations each: as enumerated, with node and edge ids reversed and
/// spread out (value nodes then precede table nodes), and both of these
/// decorated.
fn encoding_graphs(max: usize) -> Vec<GInstance> {
    let spec = encoding_spec();
    let side = spec.side();
    let plain = oracle::every_graph(&spec, &|_, _| encoding_forms(), max);
    let mut out = Vec::with_capacity(plain.len() * 4);
    for graph in plain {
        let top_node = graph.max_node_id().unwrap_or(0);
        let top_edge = graph.max_edge_id().map_or(0, |edge| edge.0);
        let spread = oracle::relabel(
            &graph,
            |node| 3 * (top_node - node) + 1,
            |edge| 5 * (top_edge - edge) + 2,
        );
        out.push(oracle::decorate(&graph, &side));
        out.push(oracle::decorate(&spread, &side));
        out.push(graph);
        out.push(spread);
    }
    out
}

#[test]
fn the_oracle_enumerators_give_their_labelled_counts() {
    let spec = encoding_spec();
    let count = oracle::every_graph(&spec, &|_, _| encoding_forms(), 4).len();
    assert_eq!(count, oracle::labelled_count(&spec, &|_, _| 7, 4));
    // sum over k + m <= 4 of 7^k * 2^(k m), written out.
    let by_hand: usize = (0..=4usize)
        .flat_map(|k| (0..=4 - k).map(move |m| (k, m)))
        .map(|(k, m)| 7usize.pow(u32::try_from(k).unwrap()) << (k * m))
        .sum();
    assert_eq!(count, by_hand);
    assert_eq!(count, 6627);
    for (spec, cells, expected) in [(relation_spec(), 1, 51), (shared_column_spec(), 2, 129)] {
        let forms = |_: &str, _: &str| cells;
        let enumerated = oracle::every_graph(&spec, &|_, _| leaf_forms(&small_cells(cells)), 4);
        assert_eq!(enumerated.len(), oracle::labelled_count(&spec, &forms, 4));
        assert_eq!(enumerated.len(), expected);
    }
}

/// Encoding law one, read against the oracle: every enumerated graph lies in
/// `Enc(X)`, `E` gives the table the oracle reads off the graph, and `D`
/// after `E` rebuilds the graph identically.
#[test]
fn decoding_inverts_encoding_on_every_small_graph() {
    let spec = encoding_spec();
    let side = spec.side();
    let rel = RelSchema::new(&spec.schema(), &protocol()).unwrap();
    let graphs = encoding_graphs(4);
    assert_eq!(graphs.len(), 4 * 6627);
    par(&graphs, |graph| {
        assert_eq!(oracle::classify(graph, &side), Ok(()), "{graph:?}");
        let (tables, layout) = encode_graph(graph, EncodingSide::Whole(&rel)).unwrap();
        assert_eq!(
            tables,
            oracle::Content::of_graph(graph, &side).table(),
            "{graph:?}"
        );
        let decoded = decode_graph(&tables, EncodingSide::Whole(&rel), &layout).unwrap();
        assert!(
            GInstance::identical(graph, &decoded),
            "{graph:?} -> {decoded:?}"
        );
    });
}

/// The first `count` cells of the family alphabet: `0`, `1`, then the
/// labelled null `0`.
fn small_cells(count: usize) -> Vec<Value> {
    [Value::Int(0), Value::Int(1), Value::LabeledNull(0)]
        .into_iter()
        .take(count)
        .collect()
}

/// One leaf form per value.
fn leaf_forms(cells: &[Value]) -> Vec<oracle::Form> {
    cells
        .iter()
        .map(|value| oracle::Form::Leaf(FieldPresence::Present(value.clone())))
        .collect()
}

/// Two tables without columns and one relation between them.
fn relation_spec() -> oracle::Spec {
    oracle::Spec {
        tables: vec![("a", vec![]), ("b", vec![])],
        relations: vec![edge("a", "b", "f")],
        inner: vec![],
    }
}

/// Two tables with one integer column each.
fn shared_column_spec() -> oracle::Spec {
    oracle::Spec {
        tables: vec![("a", vec![("x", "int")]), ("b", vec![("x", "int")])],
        relations: vec![],
        inner: vec![],
    }
}

/// Every conforming table of the encoding family with at most four rows,
/// its cells drawn from a constant, `Null`, a record, a list and a labelled
/// null, read through the oracle from the graphs that hold them as leaves.
fn encoding_tables() -> Vec<FInstance> {
    let spec = encoding_spec();
    let side = spec.side();
    let cells = vec![
        oracle::Form::Leaf(FieldPresence::Present(Value::Int(0))),
        oracle::Form::Leaf(FieldPresence::Null),
        oracle::Form::Leaf(FieldPresence::Present(Value::Unknown(HashMap::from([(
            "k".to_owned(),
            Value::Str("s".into()),
        )])))),
        oracle::Form::Leaf(FieldPresence::Present(Value::List(vec![
            Value::Int(0),
            Value::Int(1),
        ]))),
        oracle::Form::Leaf(FieldPresence::Present(Value::LabeledNull(0))),
    ];
    oracle::every_graph(&spec, &|_, _| cells.clone(), 4)
        .iter()
        .map(|graph| oracle::Content::of_graph(graph, &side).table())
        .collect()
}

/// The layouts the second encoding law is run with: the record-free layout
/// and the layout of every graph of the encoding family with at most two
/// table nodes, as enumerated and decorated with spread ids.
fn encoding_layouts() -> Vec<GraphLayout> {
    let spec = encoding_spec();
    let rel = RelSchema::new(&spec.schema(), &protocol()).unwrap();
    let mut layouts = vec![GraphLayout::default()];
    for (index, graph) in encoding_graphs(2).iter().enumerate() {
        if index % 4 == 1 || index % 4 == 2 {
            layouts.push(encode_graph(graph, EncodingSide::Whole(&rel)).unwrap().1);
        }
    }
    layouts
}

/// Encoding law two, read against the oracle: for every conforming table and
/// every layout of the family, `D` lands in `Enc(X)` and the table component
/// of `E` after `D` is the table.
#[test]
fn encoding_inverts_decoding_on_every_small_table_and_layout() {
    let spec = encoding_spec();
    let side = spec.side();
    let rel = RelSchema::new(&spec.schema(), &protocol()).unwrap();
    let tables = encoding_tables();
    assert_eq!(tables.len(), oracle::labelled_count(&spec, &|_, _| 5, 4));
    let layouts = encoding_layouts();
    assert_eq!(layouts.len(), 1 + 2 * 73);
    par(&tables, |table| {
        assert_eq!(oracle::conformance(table, &side), Ok(()));
        for layout in &layouts {
            let graph = decode_graph(table, EncodingSide::Whole(&rel), layout).unwrap();
            assert_eq!(oracle::classify(&graph, &side), Ok(()), "{graph:?}");
            assert_eq!(oracle::Content::of_graph(&graph, &side).table(), *table);
            let (again, _) = encode_graph(&graph, EncodingSide::Whole(&rel)).unwrap();
            assert_eq!(again, *table);
        }
    });
}

/// Layout isomorphism: for every conforming table `T` of the family and
/// every ordered pair of layouts `L1`, `L2` of it (the record-free layout
/// and every layout of the family), the identity on table nodes is a
/// homomorphism `D(T, L1) -> D(T, L2)`, both by `GraphKanHom::check` and by
/// the oracle. Layouts that decode `T` to the same graph are checked once.
/// So graphs that differ only in layout are isomorphic, each pair checked
/// directly, and the layout selection of the Kan functions is a choice of
/// representative.
#[test]
fn graphs_that_differ_only_in_layout_are_isomorphic() {
    let spec = encoding_spec();
    let side = spec.side();
    let rel = RelSchema::new(&spec.schema(), &protocol()).unwrap();
    let tables = encoding_tables();
    let layouts = encoding_layouts();
    let differing = std::sync::atomic::AtomicUsize::new(0);
    let both_laid_out = std::sync::atomic::AtomicUsize::new(0);
    par(&tables, |table| {
        // Each distinct decoding, with whether a non-default layout gave it.
        let mut decodings: Vec<(GInstance, bool)> = Vec::new();
        for (index, layout) in layouts.iter().enumerate() {
            let graph = decode_graph(table, EncodingSide::Whole(&rel), layout).unwrap();
            match decodings
                .iter_mut()
                .find(|(seen, _)| GInstance::identical(seen, &graph))
            {
                Some((_, laid_out)) => *laid_out |= index > 0,
                None => decodings.push((graph, index > 0)),
            }
        }
        if decodings.len() > 1 {
            differing.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let identity = GraphKanHom {
            node_map: table
                .row_ids()
                .into_iter()
                .map(|row| (row.0, row.0))
                .collect(),
            null_map: BTreeMap::new(),
        };
        let contents: Vec<oracle::Content> = decodings
            .iter()
            .map(|(graph, _)| oracle::Content::of_graph(graph, &side))
            .collect();
        let oracle_identity = oracle::Hom::identity(&contents[0]);
        for (i, (first, first_laid_out)) in decodings.iter().enumerate() {
            for (j, (second, second_laid_out)) in decodings.iter().enumerate() {
                if i != j && *first_laid_out && *second_laid_out {
                    both_laid_out.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                identity
                    .check(first, second, EncodingSide::Whole(&rel))
                    .unwrap();
                assert!(oracle::is_hom(&contents[i], &contents[j], &oracle_identity));
            }
        }
    });
    assert!(differing.into_inner() > 0, "some layout changes the graph");
    assert!(
        both_laid_out.into_inner() > 0,
        "some pair of distinct graphs comes from two non-default layouts"
    );
}

/// A table `p` with one structured column `x` holding a record `{k}`.
fn record_spec() -> oracle::Spec {
    oracle::Spec {
        tables: vec![("p", vec![("x", "value")])],
        relations: vec![],
        inner: vec![("p.x", "p.x.k", "k", "str")],
    }
}

fn record_value() -> Value {
    Value::Unknown(HashMap::from([("k".to_owned(), Value::Str("s".into()))]))
}

/// The hom families, each over at most four table nodes: the record table
/// over two of its cell forms at a time (a constant and a labelled null, two
/// labelled nulls, a constant and a record subtree, a record subtree and a
/// leaf holding the record it folds to), and the relation family.
fn hom_families() -> Vec<(&'static str, oracle::Spec, Vec<oracle::Form>)> {
    use oracle::Form;
    let constant = Form::Leaf(FieldPresence::Present(Value::Int(0)));
    let null = |n| Form::Leaf(FieldPresence::Present(Value::LabeledNull(n)));
    let subtree = Form::Record(vec![("k", "p.x.k", Value::Str("s".into()))]);
    let leaf = Form::Leaf(FieldPresence::Present(record_value()));
    vec![
        (
            "constant and null",
            record_spec(),
            vec![constant.clone(), null(0)],
        ),
        ("two nulls", record_spec(), vec![null(0), null(1)]),
        (
            "constant and record subtree",
            record_spec(),
            vec![constant, subtree.clone()],
        ),
        (
            "record subtree and record leaf",
            record_spec(),
            vec![subtree, leaf],
        ),
        ("relation", relation_spec(), vec![]),
    ]
}

/// Every map from the rows of `dom` to the rows of `cod` that keeps each
/// row's table.
fn table_respecting_maps(dom: &FInstance, cod: &FInstance) -> Vec<BTreeMap<u32, u32>> {
    let rows = dom.row_ids();
    let choices: Vec<Vec<u32>> = rows
        .iter()
        .map(|row| {
            cod.row_ids()
                .into_iter()
                .filter(|image| cod.table_of(*image) == dom.table_of(*row))
                .map(|image| image.0)
                .collect()
        })
        .collect();
    oracle::product(&choices)
        .into_iter()
        .map(|images| rows.iter().map(|row| row.0).zip(images).collect())
        .collect()
}

/// Every `FInstanceHom` from `dom` to `cod` that `FInstanceHom::check`
/// accepts, over every table-respecting row map and every null map sending
/// each null of `dom` to itself or to a cell value of `cod`.
fn table_homs(dom: &FInstance, cod: &FInstance) -> BTreeSet<oracle::Hom> {
    let dom_content = oracle::Content::of_table(dom);
    let cod_content = oracle::Content::of_table(cod);
    let values: Vec<Value> = cod_content
        .rows
        .values()
        .flat_map(|(_, cells)| cells.values().cloned())
        .collect();
    let nulls: Vec<u64> = dom_content.nulls().into_iter().collect();
    let null_choices: Vec<Vec<Value>> = nulls
        .iter()
        .map(|null| {
            std::iter::once(Value::LabeledNull(*null))
                .chain(values.iter().cloned())
                .collect()
        })
        .collect();
    let mut out = BTreeSet::new();
    for rows in table_respecting_maps(dom, cod) {
        for images in oracle::product(&null_choices) {
            let null_map: BTreeMap<u64, Value> = nulls.iter().copied().zip(images).collect();
            let hom = FInstanceHom::new(rows.iter().map(|(a, b)| (RowId(*a), RowId(*b))).collect())
                .with_null_map(null_map.clone());
            if hom.check(dom, cod).is_ok() {
                out.insert(oracle::Hom::normalised(&rows, &null_map, &dom_content));
            }
        }
    }
    out
}

/// The null map a node map fixes, nulls taken at their first occurrence in
/// `(node, column)` order; a null whose cell has no image is left out.
fn first_occurrence_nulls(
    dom: &oracle::Content,
    cod: &oracle::Content,
    nodes: &BTreeMap<u32, u32>,
) -> BTreeMap<u64, Value> {
    let mut nulls = BTreeMap::new();
    for (row, (_, cells)) in &dom.rows {
        let Some((_, image)) = nodes.get(row).and_then(|target| cod.rows.get(target)) else {
            continue;
        };
        for (column, value) in cells {
            if let (Value::LabeledNull(null), Some(target)) = (value, image.get(column)) {
                nulls.entry(*null).or_insert_with(|| target.clone());
            }
        }
    }
    nulls
}

/// Full faithfulness of `E`, against the oracle: for every pair of graphs of
/// each hom family with at most four table nodes, the graph homomorphisms
/// the oracle enumerates from the graphs alone are exactly the
/// `FInstanceHom`s between their encodings (`E(h)` keeps the node and null
/// maps), and `GraphKanHom::check` accepts a table-respecting node map, with
/// the null map its first occurrences fix, exactly when the oracle does.
#[test]
fn graph_kan_homs_are_exactly_the_table_homs_of_the_encodings() {
    for (name, spec, forms) in hom_families() {
        let side = spec.side();
        let rel = RelSchema::new(&spec.schema(), &protocol()).unwrap();
        let graphs = oracle::every_graph(&spec, &|_, _| forms.clone(), 4);
        let count = forms.len().max(1);
        assert_eq!(
            graphs.len(),
            oracle::labelled_count(&spec, &|_, _| count, 4),
            "{name}"
        );
        let prepared: Vec<(GInstance, oracle::Content, FInstance)> = graphs
            .into_iter()
            .map(|graph| {
                let content = oracle::Content::of_graph(&graph, &side);
                let (table, _) = encode_graph(&graph, EncodingSide::Whole(&rel)).unwrap();
                (graph, content, table)
            })
            .collect();
        let homs_found = std::sync::atomic::AtomicUsize::new(0);
        par(&prepared, |(g, g_content, g_table)| {
            for (h, h_content, h_table) in &prepared {
                let found: BTreeSet<oracle::Hom> =
                    oracle::homs(g_content, h_content).into_iter().collect();
                homs_found.fetch_add(found.len(), std::sync::atomic::Ordering::Relaxed);
                assert_eq!(
                    found,
                    table_homs(g_table, h_table),
                    "{name}: {g:?} -> {h:?}"
                );
                for nodes in table_respecting_maps(g_table, h_table) {
                    let null_map = first_occurrence_nulls(g_content, h_content, &nodes);
                    let candidate = oracle::Hom::normalised(&nodes, &null_map, g_content);
                    let hom = GraphKanHom {
                        node_map: nodes,
                        null_map,
                    };
                    assert_eq!(
                        hom.check(g, h, EncodingSide::Whole(&rel)).is_ok(),
                        found.contains(&candidate),
                        "{name}: {hom:?} on {g:?} -> {h:?}"
                    );
                }
            }
        });
        assert!(homs_found.into_inner() > 0, "{name}");
    }
}

/// Identities and composites of `GraphKanHom`, against the oracle: on every
/// graph of each hom family with at most four table nodes
/// `GraphKanHom::identity` is the oracle's identity, and for every three
/// graphs `G`, `H`, `K` of the family with at most three table nodes each and
/// every oracle homomorphisms `h: G -> H` and `k: H -> K`, `h.compose(k)` is
/// the oracle's composite and a homomorphism `G -> K`.
#[test]
fn graph_kan_hom_identities_and_composites_match_the_oracle() {
    for (name, spec, forms) in hom_families() {
        let side = spec.side();
        let rel = RelSchema::new(&spec.schema(), &protocol()).unwrap();
        let graphs = |max| -> Vec<(GInstance, oracle::Content)> {
            oracle::every_graph(&spec, &|_, _| forms.clone(), max)
                .into_iter()
                .map(|graph| {
                    let content = oracle::Content::of_graph(&graph, &side);
                    (graph, content)
                })
                .collect()
        };
        par(&graphs(4), |(g, g_content)| {
            let identity = GraphKanHom::identity(g, &EncodingSide::Whole(&rel)).unwrap();
            assert_eq!(
                oracle::Hom::normalised(&identity.node_map, &identity.null_map, g_content),
                oracle::Hom::identity(g_content),
                "{name}: {g:?}"
            );
        });
        let small = graphs(3);
        let composites = std::sync::atomic::AtomicUsize::new(0);
        par(&small, |(g, g_content)| {
            for (h, h_content) in &small {
                for first in oracle::homs(g_content, h_content) {
                    let first_kan = to_kan(&first, h_content);
                    for (k, k_content) in &small {
                        for second in oracle::homs(h_content, k_content) {
                            let composite = first_kan.compose(&to_kan(&second, k_content)).unwrap();
                            let composite = oracle::Hom::normalised(
                                &composite.node_map,
                                &composite.null_map,
                                g_content,
                            );
                            assert_eq!(
                                composite,
                                first.then(&second, g_content),
                                "{name}: {g:?} -> {h:?} -> {k:?}"
                            );
                            assert!(oracle::is_hom(g_content, k_content, &composite));
                            composites.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                }
            }
        });
        assert!(composites.into_inner() > 0, "{name}");
    }
}

/// The `GraphKanHom` of an oracle homomorphism into `cod`: its node map, and
/// each null's value read back from the cell of `cod` with those canonical
/// bytes.
fn to_kan(hom: &oracle::Hom, cod: &oracle::Content) -> GraphKanHom {
    let values: Vec<Value> = cod
        .rows
        .values()
        .flat_map(|(_, cells)| cells.values().cloned())
        .collect();
    GraphKanHom {
        node_map: hom.nodes.clone(),
        null_map: hom
            .nulls
            .iter()
            .map(|(null, bytes)| {
                let value = values
                    .iter()
                    .find(|value| oracle::bytes(value) == *bytes)
                    .cloned()
                    .unwrap_or_else(|| panic!("null {null} goes to no cell of the codomain"));
                (*null, value)
            })
            .collect(),
    }
}

/// One functor of the adjunction family: the whole source `S`, its kept part
/// `S'` and the target `T` written out by hand, the map `F: S' -> T`, the
/// cell alphabet, and whether `F` is attribute-surjective (so that `Pi`
/// exists).
struct KanCase {
    name: &'static str,
    source: oracle::Spec,
    kept: oracle::Spec,
    target: oracle::Spec,
    map: oracle::Map,
    cells: Vec<Value>,
    pi: bool,
}

/// The largest product a family Pi may build.
const FAMILY_PI_MAX: usize = 81;

impl KanCase {
    fn new(
        name: &'static str,
        (source, kept, target): (oracle::Spec, oracle::Spec, oracle::Spec),
        tables: &[(&str, &str)],
        columns: &[(&str, &str, &str)],
        relations: &[(Edge, Edge)],
    ) -> Self {
        let map = oracle::Map {
            tables: tables
                .iter()
                .map(|(s, t)| (Name::from(*s), Name::from(*t)))
                .collect(),
            columns: columns
                .iter()
                .map(|(s, c, t)| ((Name::from(*s), (*c).to_owned()), (*t).to_owned()))
                .collect(),
            relations: relations.iter().cloned().collect(),
        };
        Self {
            name,
            source,
            kept,
            target,
            map,
            cells: small_cells(2),
            pi: true,
        }
    }

    fn with_cells(mut self, cells: Vec<Value>) -> Self {
        self.cells = cells;
        self
    }

    const fn without_pi(mut self) -> Self {
        self.pi = false;
        self
    }

    /// The migration the code under test receives: each kept table and
    /// column vertex to its image, each kept column arrow and relation to
    /// its image.
    fn migration(&self) -> CompiledMigration {
        let mut vertices = Vec::new();
        let mut edges = Vec::new();
        for (source, target) in &self.map.tables {
            vertices.push((source.to_string(), target.to_string()));
        }
        for ((table, column), image) in &self.map.columns {
            let target = &self.map.tables[table];
            vertices.push((format!("{table}.{column}"), format!("{target}.{image}")));
            edges.push((
                oracle::Spec::column_edge(table, column),
                oracle::Spec::column_edge(target, image),
            ));
        }
        edges.extend(
            self.map
                .relations
                .iter()
                .map(|(a, b)| (a.clone(), b.clone())),
        );
        let named: Vec<(&str, &str)> = vertices
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        migration(&named, &edges, &[], &[])
    }

    fn functor(&self) -> MigrationFunctor {
        MigrationFunctor::new(
            &self.source.schema(),
            &protocol(),
            &self.target.schema(),
            &protocol(),
            &self.migration(),
        )
        .unwrap_or_else(|error| panic!("{}: {error}", self.name))
    }

    fn graphs(&self, spec: &oracle::Spec) -> Vec<GInstance> {
        let forms = leaf_forms(&self.cells);
        oracle::every_graph(spec, &|_, _| forms.clone(), 4)
    }
}

const fn spec(
    tables: Vec<(&'static str, Vec<(&'static str, &'static str)>)>,
    relations: Vec<Edge>,
) -> oracle::Spec {
    oracle::Spec {
        tables,
        relations,
        inner: vec![],
    }
}

/// The adjunction family: [`merge_cases`] and [`column_cases`].
fn kan_family() -> Vec<KanCase> {
    let mut cases = merge_cases();
    cases.extend(column_cases());
    cases
}

/// The merges of the family: two tables with a shared column (over
/// constants, and over a constant and a labelled null), tables carrying two
/// relations to one, and two parallel relations.
fn merge_cases() -> Vec<KanCase> {
    let merge = || {
        (
            shared_column_spec(),
            shared_column_spec(),
            spec(vec![("t", vec![("x", "int")])], vec![]),
        )
    };
    let (f, h) = (edge("a", "c", "f"), edge("b", "c", "h"));
    let relations = spec(
        vec![("a", vec![]), ("b", vec![]), ("c", vec![])],
        vec![f.clone(), h.clone()],
    );
    let g = edge("A", "C", "g");
    let (f2, h2, g2) = (
        edge("a", "b", "f"),
        edge("a", "b", "h"),
        edge("a", "b", "g"),
    );
    let parallel = spec(
        vec![("a", vec![]), ("b", vec![])],
        vec![f2.clone(), h2.clone()],
    );
    vec![
        KanCase::new(
            "a merge with a shared column",
            merge(),
            &[("a", "t"), ("b", "t")],
            &[("a", "x", "x"), ("b", "x", "x")],
            &[],
        ),
        KanCase::new(
            "a merge with a shared column over a null",
            merge(),
            &[("a", "t"), ("b", "t")],
            &[("a", "x", "x"), ("b", "x", "x")],
            &[],
        )
        .with_cells(vec![Value::Int(0), Value::LabeledNull(0)]),
        KanCase::new(
            "a merge of tables carrying two relations to one",
            (
                relations.clone(),
                relations,
                spec(vec![("A", vec![]), ("C", vec![])], vec![g.clone()]),
            ),
            &[("a", "A"), ("b", "A"), ("c", "C")],
            &[],
            &[(f, g.clone()), (h, g)],
        ),
        KanCase::new(
            "two parallel relations merged",
            (
                parallel.clone(),
                parallel,
                spec(vec![("a", vec![]), ("b", vec![])], vec![g2.clone()]),
            ),
            &[("a", "a"), ("b", "b")],
            &[],
            &[(f2, g2.clone()), (h2, g2)],
        ),
    ]
}

/// The column cases of the family: a target column outside the image (Pi
/// refuses), two columns sent to one (Sigma may clash), and a source table
/// and column outside the kept part.
fn column_cases() -> Vec<KanCase> {
    let one_column = || spec(vec![("a", vec![("x", "int")])], vec![]);
    let two_columns = spec(vec![("t", vec![("x", "int"), ("y", "int")])], vec![]);
    let dropped = spec(
        vec![("a", vec![("x", "int"), ("w", "int")]), ("q", vec![])],
        vec![],
    );
    vec![
        KanCase::new(
            "a target column outside the image",
            (
                one_column(),
                one_column(),
                spec(vec![("a", vec![("x", "int"), ("y", "int")])], vec![]),
            ),
            &[("a", "a")],
            &[("a", "x", "x")],
            &[],
        )
        .without_pi(),
        KanCase::new(
            "two columns sent to one",
            (
                two_columns.clone(),
                two_columns,
                spec(vec![("t", vec![("z", "int")])], vec![]),
            ),
            &[("t", "t")],
            &[("t", "x", "z"), ("t", "y", "z")],
            &[],
        ),
        KanCase::new(
            "a source table and column outside the kept part",
            (
                dropped,
                one_column(),
                spec(vec![("A", vec![("x", "int")])], vec![]),
            ),
            &[("a", "A")],
            &[("a", "x", "x")],
            &[],
        ),
    ]
}

/// What the adjunction checks read about one target graph `H`.
struct TargetFacts {
    graph: GInstance,
    content: oracle::Content,
    delta: GInstance,
    delta_content: oracle::Content,
    copies: BTreeMap<u32, (Name, u32)>,
    naive_delta: oracle::Content,
}

fn copies_of(provenance: &panproto_inst::GraphDeltaProvenance) -> BTreeMap<u32, (Name, u32)> {
    provenance
        .table
        .copies
        .iter()
        .map(|(copy, (table, row))| (copy.0, (table.clone(), row.0)))
        .collect()
}

/// `g_delta` on `H`, checked against the Delta oracle: it lies in
/// `Enc(S')`, it is isomorphic to the oracle's Delta, and each copy its
/// provenance names holds exactly the oracle's cells and pairs for that copy.
fn target_facts(case: &KanCase, functor: &MigrationFunctor, graph: GInstance) -> TargetFacts {
    let (target_side, kept_side) = (case.target.side(), case.kept.side());
    assert_eq!(oracle::classify(&graph, &target_side), Ok(()));
    let content = oracle::Content::of_graph(&graph, &target_side);
    let (delta, provenance) = g_delta_along_with_provenance(&graph, functor).unwrap();
    assert_eq!(
        oracle::classify(&delta, &kept_side),
        Ok(()),
        "{}: {delta:?}",
        case.name
    );
    let delta_content = oracle::Content::of_graph(&delta, &kept_side);
    let (naive_delta, naive_copies) = oracle::delta(&content, &case.map, &kept_side);
    assert!(
        oracle::isomorphic(&delta_content, &naive_delta),
        "{}: {graph:?}",
        case.name
    );
    let copies = copies_of(&provenance);
    let naive_index: BTreeMap<(Name, u32), u32> = naive_copies
        .iter()
        .map(|(copy, key)| (key.clone(), *copy))
        .collect();
    assert_eq!(copies.len(), naive_copies.len());
    let rename = |copy: &u32| naive_index[&copies[copy]];
    for (copy, row) in &delta_content.rows {
        assert_eq!(naive_delta.rows[&rename(copy)], *row, "{}", case.name);
    }
    let renamed: BTreeSet<(Edge, u32, u32)> = delta_content
        .pairs
        .iter()
        .map(|(e, a, b)| (e.clone(), rename(a), rename(b)))
        .collect();
    assert_eq!(renamed, naive_delta.pairs, "{}", case.name);
    TargetFacts {
        graph,
        content,
        delta,
        delta_content,
        copies,
        naive_delta,
    }
}

fn normal(hom: &GraphKanHom, domain: &oracle::Content) -> oracle::Hom {
    oracle::Hom::normalised(&hom.node_map, &hom.null_map, domain)
}

fn kan_identity(content: &oracle::Content) -> GraphKanHom {
    to_kan(&oracle::Hom::identity(content), content)
}

/// The counit of Sigma -| Delta at `H` and both triangle identities through
/// it: the counit is a homomorphism `Sigma Delta H -> H`, its transpose is
/// the identity of `Delta H`, and `Delta(counit) . unit_(Delta H)` is that
/// identity too.
fn sigma_triangles_at_target(case: &KanCase, functor: &MigrationFunctor, facts: &TargetFacts) {
    let (target_side, kept_side) = (case.target.side(), case.kept.side());
    let sigma_delta = g_sigma_restricted(&facts.delta, functor).unwrap();
    let sigma_delta_content = oracle::Content::of_graph(&sigma_delta, &target_side);
    let counit = g_counit(&facts.graph, functor).unwrap();
    let counit = normal(&counit, &sigma_delta_content);
    assert!(
        oracle::is_hom(&sigma_delta_content, &facts.content, &counit),
        "{}",
        case.name
    );
    let identity = oracle::Hom::identity(&facts.delta_content);
    let transpose = g_transpose_left(
        &to_kan(&counit, &facts.content),
        &facts.delta,
        &facts.graph,
        functor,
    )
    .unwrap();
    assert_eq!(
        normal(&transpose, &facts.delta_content),
        identity,
        "{}",
        case.name
    );

    let (delta_sigma_delta, provenance) =
        g_delta_along_with_provenance(&sigma_delta, functor).unwrap();
    let delta_sigma_delta_content = oracle::Content::of_graph(&delta_sigma_delta, &kept_side);
    let unit = normal(
        &g_unit(&facts.delta, functor).unwrap(),
        &facts.delta_content,
    );
    assert!(oracle::is_hom(
        &facts.delta_content,
        &delta_sigma_delta_content,
        &unit
    ));
    let delta_counit = oracle::delta_on_hom(&counit, &copies_of(&provenance), &facts.copies);
    assert!(oracle::is_hom(
        &delta_sigma_delta_content,
        &facts.delta_content,
        &delta_counit
    ));
    assert_eq!(
        unit.then(&delta_counit, &facts.delta_content),
        identity,
        "{}",
        case.name
    );
}

/// The unit of Delta -| Pi at `H` and both triangle identities through it:
/// the unit is a homomorphism `H -> Pi Delta H`, its transpose is the
/// identity of `Delta H`, and `counit_(Delta H) . Delta(unit)` is that
/// identity too.
fn pi_triangles_at_target(case: &KanCase, functor: &MigrationFunctor, facts: &TargetFacts) {
    let (target_side, kept_side) = (case.target.side(), case.kept.side());
    let pi_delta = g_pi_restricted(&facts.delta, functor, FAMILY_PI_MAX).unwrap();
    let pi_delta_content = oracle::Content::of_graph(&pi_delta, &target_side);
    let unit = normal(
        &g_pi_unit(&facts.graph, functor, FAMILY_PI_MAX).unwrap(),
        &facts.content,
    );
    assert!(
        oracle::is_hom(&facts.content, &pi_delta_content, &unit),
        "{}",
        case.name
    );
    let identity = oracle::Hom::identity(&facts.delta_content);
    let transpose = g_pi_transpose_right(
        &to_kan(&unit, &pi_delta_content),
        &facts.graph,
        &facts.delta,
        functor,
        FAMILY_PI_MAX,
    )
    .unwrap();
    assert_eq!(
        normal(&transpose, &facts.delta_content),
        identity,
        "{}",
        case.name
    );

    let (delta_pi_delta, provenance) = g_delta_along_with_provenance(&pi_delta, functor).unwrap();
    let delta_pi_delta_content = oracle::Content::of_graph(&delta_pi_delta, &kept_side);
    let delta_unit = oracle::delta_on_hom(&unit, &facts.copies, &copies_of(&provenance));
    assert!(oracle::is_hom(
        &facts.delta_content,
        &delta_pi_delta_content,
        &delta_unit
    ));
    let counit = normal(
        &g_pi_counit(&facts.delta, functor, FAMILY_PI_MAX).unwrap(),
        &delta_pi_delta_content,
    );
    assert!(oracle::is_hom(
        &delta_pi_delta_content,
        &facts.delta_content,
        &counit
    ));
    assert_eq!(
        delta_unit.then(&counit, &facts.delta_content),
        identity,
        "{}",
        case.name
    );
}

/// Sigma -| Delta at a source graph `G` against every target: when Sigma
/// succeeds, its unit is a homomorphism `G -> Delta Sigma G`, its transpose
/// is the identity of `Sigma G` and the transpose of that identity is the
/// unit; and for every `H` the oracle's `Hom(Sigma G, H)`, `Hom(G, Delta H)`
/// and `Hom(G, Delta_oracle H)` have one size, the transpose sends the first
/// injectively into the second and the inverse transpose undoes it. When
/// Sigma refuses with a clash of constants, `Hom(G, Delta H)` is empty for
/// every `H`. Returns whether Sigma clashed.
fn sigma_at_source(
    case: &KanCase,
    functor: &MigrationFunctor,
    graph: &GInstance,
    targets: &[TargetFacts],
) -> bool {
    let (target_side, kept_side) = (case.target.side(), case.kept.side());
    let content = oracle::Content::of_graph(graph, &kept_side);
    let sigma = match g_sigma_restricted(graph, functor) {
        Ok(sigma) => sigma,
        Err(GraphKanError::Adjunction(AdjunctionError::SigmaConstantClash { .. })) => {
            for target in targets {
                assert!(
                    oracle::homs(&content, &target.naive_delta).is_empty(),
                    "{}",
                    case.name
                );
                assert!(
                    oracle::homs(&content, &target.delta_content).is_empty(),
                    "{}",
                    case.name
                );
            }
            return true;
        }
        Err(other) => panic!("{}: {other}", case.name),
    };
    assert_eq!(
        oracle::classify(&sigma, &target_side),
        Ok(()),
        "{}",
        case.name
    );
    let sigma_content = oracle::Content::of_graph(&sigma, &target_side);
    let delta_sigma = g_delta_along(&sigma, functor).unwrap();
    let delta_sigma_content = oracle::Content::of_graph(&delta_sigma, &kept_side);
    let unit = normal(&g_unit(graph, functor).unwrap(), &content);
    assert!(
        oracle::is_hom(&content, &delta_sigma_content, &unit),
        "{}",
        case.name
    );
    let back =
        g_transpose_right(&to_kan(&unit, &delta_sigma_content), graph, &sigma, functor).unwrap();
    assert_eq!(
        normal(&back, &sigma_content),
        oracle::Hom::identity(&sigma_content)
    );
    let forth = g_transpose_left(&kan_identity(&sigma_content), graph, &sigma, functor).unwrap();
    assert_eq!(normal(&forth, &content), unit, "{}", case.name);

    for target in targets {
        let left = oracle::homs(&sigma_content, &target.content);
        let right: BTreeSet<oracle::Hom> = oracle::homs(&content, &target.delta_content)
            .into_iter()
            .collect();
        assert_eq!(
            left.len(),
            right.len(),
            "{}: {graph:?} / {:?}",
            case.name,
            target.graph
        );
        assert_eq!(
            left.len(),
            oracle::homs(&content, &target.naive_delta).len()
        );
        let mut images = BTreeSet::new();
        for hom in &left {
            let transpose =
                g_transpose_left(&to_kan(hom, &target.content), graph, &target.graph, functor)
                    .unwrap();
            let image = normal(&transpose, &content);
            assert!(
                right.contains(&image),
                "{}: {hom:?} -> {image:?}",
                case.name
            );
            let back = g_transpose_right(&transpose, graph, &target.graph, functor).unwrap();
            assert_eq!(normal(&back, &sigma_content), *hom, "{}", case.name);
            images.insert(image);
        }
        assert_eq!(images.len(), left.len(), "{}", case.name);
    }
    false
}

/// Delta -| Pi at a source graph `G` against every target: Pi's counit is a
/// homomorphism `Delta Pi G -> G`, its transpose is the identity of `Pi G`
/// and the inverse transpose of that identity is the counit; and for every
/// `H` the oracle's `Hom(Delta H, G)`, `Hom(Delta_oracle H, G)` and
/// `Hom(H, Pi G)` have one size, the transpose sends the first injectively
/// into the last and the inverse transpose undoes it. Without
/// attribute-surjectivity Pi refuses with `AttributeOutsideImage`.
fn pi_at_source(
    case: &KanCase,
    functor: &MigrationFunctor,
    graph: &GInstance,
    targets: &[TargetFacts],
) {
    let (target_side, kept_side) = (case.target.side(), case.kept.side());
    let content = oracle::Content::of_graph(graph, &kept_side);
    if !case.pi {
        assert!(matches!(
            g_pi_restricted(graph, functor, FAMILY_PI_MAX),
            Err(GraphKanError::Adjunction(AdjunctionError::Pi(
                RestrictError::AttributeOutsideImage { .. }
            )))
        ));
        return;
    }
    let pi = g_pi_restricted(graph, functor, FAMILY_PI_MAX).unwrap();
    assert_eq!(oracle::classify(&pi, &target_side), Ok(()), "{}", case.name);
    let pi_content = oracle::Content::of_graph(&pi, &target_side);
    let delta_pi = g_delta_along(&pi, functor).unwrap();
    let delta_pi_content = oracle::Content::of_graph(&delta_pi, &kept_side);
    let counit = normal(
        &g_pi_counit(graph, functor, FAMILY_PI_MAX).unwrap(),
        &delta_pi_content,
    );
    assert!(
        oracle::is_hom(&delta_pi_content, &content, &counit),
        "{}",
        case.name
    );
    let forth = g_pi_transpose_left(
        &to_kan(&counit, &content),
        &pi,
        graph,
        functor,
        FAMILY_PI_MAX,
    )
    .unwrap();
    assert_eq!(
        normal(&forth, &pi_content),
        oracle::Hom::identity(&pi_content)
    );
    let back = g_pi_transpose_right(
        &kan_identity(&pi_content),
        &pi,
        graph,
        functor,
        FAMILY_PI_MAX,
    )
    .unwrap();
    assert_eq!(normal(&back, &delta_pi_content), counit, "{}", case.name);

    for target in targets {
        let left = oracle::homs(&target.delta_content, &content);
        let right: BTreeSet<oracle::Hom> = oracle::homs(&target.content, &pi_content)
            .into_iter()
            .collect();
        assert_eq!(
            left.len(),
            right.len(),
            "{}: {graph:?} / {:?}",
            case.name,
            target.graph
        );
        assert_eq!(
            left.len(),
            oracle::homs(&target.naive_delta, &content).len()
        );
        let mut images = BTreeSet::new();
        for hom in &left {
            let transpose = g_pi_transpose_left(
                &to_kan(hom, &content),
                &target.graph,
                graph,
                functor,
                FAMILY_PI_MAX,
            )
            .unwrap();
            let image = normal(&transpose, &target.content);
            assert!(
                right.contains(&image),
                "{}: {hom:?} -> {image:?}",
                case.name
            );
            let back =
                g_pi_transpose_right(&transpose, &target.graph, graph, functor, FAMILY_PI_MAX)
                    .unwrap();
            assert_eq!(normal(&back, &target.delta_content), *hom, "{}", case.name);
            images.insert(image);
        }
        assert_eq!(images.len(), left.len(), "{}", case.name);
    }
}

/// Both adjunction transports, the units, counits and triangle identities,
/// on every source graph of `Enc(S')` and every target graph of `Enc(T)`
/// with at most four table nodes, for the family functor named `name`,
/// against the brute-force oracle.
fn adjunctions_hold_on_every_small_instance(name: &str) {
    let case = kan_family()
        .into_iter()
        .find(|case| case.name == name)
        .unwrap_or_else(|| panic!("no family functor {name}"));
    let functor = case.functor();
    let sources = case.graphs(&case.kept);
    let targets: Vec<GInstance> = case.graphs(&case.target);
    let cells = case.cells.len();
    let forms = |_: &str, _: &str| cells;
    assert_eq!(sources.len(), oracle::labelled_count(&case.kept, &forms, 4));
    assert_eq!(
        targets.len(),
        oracle::labelled_count(&case.target, &forms, 4)
    );
    let facts: Vec<TargetFacts> = targets
        .into_iter()
        .map(|graph| target_facts(&case, &functor, graph))
        .collect();
    par(&facts, |target| {
        sigma_triangles_at_target(&case, &functor, target);
        if case.pi {
            pi_triangles_at_target(&case, &functor, target);
        }
    });
    let clashes = std::sync::atomic::AtomicUsize::new(0);
    par(&sources, |graph| {
        assert_eq!(oracle::classify(graph, &case.kept.side()), Ok(()));
        if sigma_at_source(&case, &functor, graph, &facts) {
            clashes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        pi_at_source(&case, &functor, graph, &facts);
    });
    assert_eq!(
        clashes.into_inner() > 0,
        case.name == "two columns sent to one",
        "{}",
        case.name
    );
}

#[test]
fn graph_kan_adjunctions_hold_for_a_merge_with_a_shared_column() {
    adjunctions_hold_on_every_small_instance("a merge with a shared column");
}

#[test]
fn graph_kan_adjunctions_hold_for_a_merge_with_a_shared_column_over_a_null() {
    adjunctions_hold_on_every_small_instance("a merge with a shared column over a null");
}

#[test]
fn graph_kan_adjunctions_hold_for_a_merge_of_tables_carrying_two_relations_to_one() {
    adjunctions_hold_on_every_small_instance("a merge of tables carrying two relations to one");
}

#[test]
fn graph_kan_adjunctions_hold_for_two_parallel_relations_merged() {
    adjunctions_hold_on_every_small_instance("two parallel relations merged");
}

#[test]
fn graph_kan_adjunctions_hold_for_a_target_column_outside_the_image() {
    adjunctions_hold_on_every_small_instance("a target column outside the image");
}

#[test]
fn graph_kan_adjunctions_hold_for_two_columns_sent_to_one() {
    adjunctions_hold_on_every_small_instance("two columns sent to one");
}

#[test]
fn graph_kan_adjunctions_hold_for_a_source_table_and_column_outside_the_kept_part() {
    adjunctions_hold_on_every_small_instance("a source table and column outside the kept part");
}

/// Spec ruling S3 (4) on every whole-source graph of the family functor
/// with a dropped table and a dropped column, at most four table nodes:
/// the oracle restriction `rho G` (computed from the graph and the kept
/// side alone) lies in `Enc(S')`, `E_S'(rho G) = f_restrict_to_domain(E_S
/// G)`, and Sigma and Pi of `G` (along the functor and from the schemas)
/// are exactly Sigma and Pi of `rho G`, so no element at a dropped table or
/// column makes either refuse.
#[test]
fn graph_sigma_and_pi_restrict_every_small_whole_source_graph() {
    let case = kan_family()
        .into_iter()
        .find(|case| case.name == "a source table and column outside the kept part")
        .unwrap();
    let functor = case.functor();
    let whole_side = case.source.side();
    let kept_side = case.kept.side();
    let wholes = case.graphs(&case.source);
    let cells = case.cells.len();
    assert_eq!(
        wholes.len(),
        oracle::labelled_count(&case.source, &|_: &str, _: &str| cells, 4)
    );
    let (source, target, migration) =
        (case.source.schema(), case.target.schema(), case.migration());
    let both_dropped = std::sync::atomic::AtomicUsize::new(0);
    par(&wholes, |graph| {
        assert_eq!(oracle::classify(graph, &whole_side), Ok(()));
        let kept = oracle::restrict(graph, &kept_side);
        assert_eq!(oracle::classify(&kept, &kept_side), Ok(()));
        let anchors: BTreeSet<&str> = graph
            .nodes()
            .values()
            .map(|node| node.anchor.as_ref())
            .collect();
        if anchors.contains("q") && anchors.contains("a.w") {
            both_dropped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let (whole_tables, _) = encode_graph(graph, EncodingSide::Whole(functor.source())).unwrap();
        let (kept_tables, _) = encode_graph(&kept, EncodingSide::Domain(&functor)).unwrap();
        assert_eq!(
            kept_tables,
            oracle::Content::of_graph(&kept, &kept_side).table()
        );
        assert_eq!(
            panproto_inst::f_restrict_to_domain(&whole_tables, &functor).unwrap(),
            kept_tables,
            "{graph:?}"
        );
        let sigma = g_sigma_along(graph, &functor).unwrap();
        assert!(GInstance::identical(
            &sigma,
            &g_sigma_restricted(&kept, &functor).unwrap()
        ));
        assert!(GInstance::identical(
            &sigma,
            &g_sigma(
                graph,
                &source,
                &protocol(),
                &target,
                &protocol(),
                &migration
            )
            .unwrap()
        ));
        let pi = g_pi_along(graph, &functor, FAMILY_PI_MAX).unwrap();
        assert!(GInstance::identical(
            &pi,
            &g_pi_restricted(&kept, &functor, FAMILY_PI_MAX).unwrap()
        ));
        assert!(GInstance::identical(
            &pi,
            &g_pi(
                graph,
                &source,
                &protocol(),
                &target,
                &protocol(),
                &migration,
                FAMILY_PI_MAX
            )
            .unwrap()
        ));
    });
    assert!(both_dropped.into_inner() > 0);
}

/// The prerequisite of `E D = id` in the transport: on every instance the
/// adjunction oracles enumerate (the whole source `S`, its kept part `S'`
/// and the target `T` of every family functor, at most four table nodes),
/// every output of `f_sigma`, `f_sigma_restricted`, `f_delta`,
/// `functor_pi_along` and `functor_pi_restricted` conforms to its side,
/// with no nested labelled null, as the oracle reads conformance. The only
/// refusals are a clash of constants in Sigma and `AttributeOutsideImage` in
/// Pi, each exactly where the family expects it.
#[test]
fn every_table_kan_output_on_the_oracle_instances_conforms() {
    use panproto_inst::{
        f_delta, f_sigma, f_sigma_restricted, functor_pi_along, functor_pi_restricted,
    };
    for case in kan_family() {
        let functor = case.functor();
        let (target_side, kept_side) = (case.target.side(), case.kept.side());
        let tables = |spec: &oracle::Spec| -> Vec<FInstance> {
            let side = spec.side();
            case.graphs(spec)
                .iter()
                .map(|graph| oracle::Content::of_graph(graph, &side).table())
                .collect()
        };
        let sigma_ok = |result: Result<FInstance, AdjunctionError>| match result {
            Ok(table) => assert_eq!(
                oracle::conformance(&table, &target_side),
                Ok(()),
                "{}",
                case.name
            ),
            Err(AdjunctionError::SigmaConstantClash { .. }) => {
                assert_eq!(case.name, "two columns sent to one");
            }
            Err(other) => panic!("{}: {other}", case.name),
        };
        let pi_ok = |result: Result<FInstance, AdjunctionError>| match result {
            Ok(table) => {
                assert!(case.pi, "{}", case.name);
                assert_eq!(
                    oracle::conformance(&table, &target_side),
                    Ok(()),
                    "{}",
                    case.name
                );
            }
            Err(AdjunctionError::Pi(RestrictError::AttributeOutsideImage { .. })) => {
                assert!(!case.pi);
            }
            Err(other) => panic!("{}: {other}", case.name),
        };
        let whole = tables(&case.source);
        par(&whole, |instance| {
            sigma_ok(f_sigma(instance, &functor));
            pi_ok(functor_pi_along(instance, &functor, FAMILY_PI_MAX));
        });
        let kept = tables(&case.kept);
        par(&kept, |instance| {
            sigma_ok(f_sigma_restricted(instance, &functor).map(|(table, _)| table));
            pi_ok(functor_pi_restricted(instance, &functor, FAMILY_PI_MAX).map(|(table, _)| table));
        });
        let targets = tables(&case.target);
        par(&targets, |instance| {
            let delta = f_delta(instance, &functor).unwrap();
            assert_eq!(
                oracle::conformance(&delta, &kept_side),
                Ok(()),
                "{}",
                case.name
            );
        });
    }
}

/// The number of mutation kinds [`mutate`] knows.
const MUTATIONS: u8 = 18;

/// The ids a mutation reads: the node and edge `pick` selects (modulo their
/// counts), the node after that node, and a fresh node and edge id.
struct Picks {
    node: Option<u32>,
    other: Option<u32>,
    edge: Option<EdgeId>,
    fresh_node: u32,
    fresh_edge: EdgeId,
}

impl Picks {
    fn of(graph: &GInstance, pick: usize) -> Self {
        let mut nodes: Vec<u32> = graph.nodes().keys().copied().collect();
        nodes.sort_unstable();
        let mut edges: Vec<EdgeId> = graph.edges().keys().copied().collect();
        edges.sort_unstable();
        Self {
            node: nodes.get(pick % nodes.len().max(1)).copied(),
            other: nodes.get((pick + 1) % nodes.len().max(1)).copied(),
            edge: edges.get(pick % edges.len().max(1)).copied(),
            fresh_node: graph.max_node_id().map_or(0, |id| id + 1),
            fresh_edge: EdgeId(graph.max_edge_id().map_or(0, |id| id.0 + 1)),
        }
    }
}

fn anchor_of(graph: &GInstance, id: u32) -> String {
    graph
        .node(id)
        .map(|n| n.anchor.to_string())
        .unwrap_or_default()
}

/// The mutations that add nodes or edges: a fan edge, a node outside the
/// side, an edge outside the side, an edge with swapped endpoints, a
/// duplicated edge, a second cell, an orphan value node and a second record
/// field of one name.
fn mutate_by_adding(out: &mut GInstance, graph: &GInstance, kind: u8, at: &Picks) {
    let add = |out: &mut GInstance, src: u32, tgt: u32, label: Edge| {
        out.insert_edge(at.fresh_edge, panproto_inst::GEdge::new(src, tgt, label))
            .unwrap();
    };
    let leaf = |anchor: &str, value: Value| {
        Node::new(at.fresh_node, anchor).with_value(FieldPresence::Present(value))
    };
    match (kind, at.node) {
        (1, Some(n)) => {
            out.insert_node(Node::new(at.fresh_node, "a")).unwrap();
            let label = Edge {
                src: anchor_of(graph, n).into(),
                tgt: "a".into(),
                kind: panproto_inst::HYPER_PARENT_KIND.into(),
                name: None,
            };
            add(out, n, at.fresh_node, label);
        }
        (2, _) => out
            .insert_node(Node::new(at.fresh_node, "outside"))
            .unwrap(),
        (3, Some(n)) => {
            let m = at.other.unwrap_or(n);
            add(
                out,
                n,
                m,
                edge(&anchor_of(graph, n), &anchor_of(graph, m), "nowhere"),
            );
        }
        (4, Some(n)) => add(out, at.other.unwrap_or(n), n, edge("a", "b", "f")),
        (5, _) => {
            if let Some(e) = at.edge {
                out.insert_edge(at.fresh_edge, graph.edge(e).unwrap().clone())
                    .unwrap();
            }
        }
        (8, Some(n)) if anchor_of(graph, n) == "a" => {
            out.insert_node(leaf("a.x", Value::Int(5))).unwrap();
            add(out, n, at.fresh_node, edge("a", "a.x", "x"));
        }
        (10, _) => out.insert_node(leaf("a.x", Value::Int(5))).unwrap(),
        (17, Some(n)) if anchor_of(graph, n) == "a.x" => {
            out.insert_node(leaf("a.x.k", Value::Str("t".into())))
                .unwrap();
            add(out, n, at.fresh_node, edge("a.x", "a.x.k", "k"));
        }
        _ => {}
    }
}

/// The mutations that change what is there: a fan shape, a removed edge
/// (with or without an extra field of its name), an edge moved onto another
/// edge's value node of the same label, a cleared, absent or inner
/// value, a toggled position, and a nested or top-level labelled null.
fn mutate_in_place(out: &mut GInstance, graph: &GInstance, kind: u8, at: &Picks) {
    let set_value = |out: &mut GInstance, n: u32, value: Option<FieldPresence>| {
        out.node_mut(n).unwrap().value = value;
    };
    match (kind, at.node, at.edge) {
        (0, Some(n), _) => out.node_mut(n).unwrap().shape = NodeShape::HyperEdge,
        (6, _, Some(e)) => {
            out.remove_edge(e).unwrap();
        }
        (7, _, Some(e)) => {
            let removed = out.remove_edge(e).unwrap();
            let name = removed.edge.name.map(|n| n.to_string()).unwrap_or_default();
            out.node_mut(removed.src)
                .unwrap()
                .extra_fields
                .insert(name, Value::Int(9));
        }
        (9, _, Some(e)) => {
            let moved = graph.edge(e).unwrap();
            let shared = graph
                .edges_sorted()
                .find(|(_, other)| other.edge == moved.edge && other.tgt != moved.tgt)
                .map(|(_, other)| other.tgt);
            if let Some(target) = shared {
                out.set_endpoints(e, moved.src, target).unwrap();
            }
        }
        (11, Some(n), _) => set_value(out, n, None),
        (12, Some(n), _) => set_value(out, n, Some(FieldPresence::Absent)),
        (13, Some(n), _) => set_value(out, n, Some(FieldPresence::Present(Value::Int(0)))),
        (14, _, Some(e)) => {
            let toggled = if graph.edge(e).unwrap().position.is_some() {
                None
            } else {
                Some(7)
            };
            out.set_edge_position(e, toggled).unwrap();
        }
        (15, Some(n), _) => {
            let nested = Value::List(vec![Value::LabeledNull(3)]);
            set_value(out, n, Some(FieldPresence::Present(nested)));
        }
        (16, Some(n), _) => set_value(out, n, Some(FieldPresence::Present(Value::LabeledNull(3)))),
        _ => {}
    }
}

/// `graph` changed by mutation `kind` at the node or edge `pick` selects.
/// Most kinds move the graph out of `Enc(X)`; some keep it inside (a list
/// position toggled on a one-child record, a null at the top level of a
/// cell), and the tests compare the code with the oracle either way.
fn mutate(graph: &GInstance, kind: u8, pick: usize) -> GInstance {
    let at = Picks::of(graph, pick);
    let mut out = graph.clone();
    mutate_by_adding(&mut out, graph, kind, &at);
    mutate_in_place(&mut out, graph, kind, &at);
    out
}

/// The code's answer for `graph` on the encoding family, as the oracle
/// states it: `Ok` with the oracle's table, or the oracle's first reason.
fn assert_encoding_matches_the_oracle(graph: &GInstance) -> Result<(), NotEncodableReason> {
    assert_encoding_matches_the_oracle_on(&encoding_spec(), graph)
}

fn assert_encoding_matches_the_oracle_on(
    spec: &oracle::Spec,
    graph: &GInstance,
) -> Result<(), NotEncodableReason> {
    let side = spec.side();
    let rel = RelSchema::new(&spec.schema(), &protocol()).unwrap();
    let expected = oracle::classify(graph, &side);
    match (encode_graph(graph, EncodingSide::Whole(&rel)), &expected) {
        (Ok((tables, layout)), Ok(())) => {
            assert_eq!(
                tables,
                oracle::Content::of_graph(graph, &side).table(),
                "{graph:?}"
            );
            let decoded = decode_graph(&tables, EncodingSide::Whole(&rel), &layout).unwrap();
            assert!(GInstance::identical(graph, &decoded), "{graph:?}");
        }
        (Err(GraphKanError::NotEncodable { reason }), Err(want)) => {
            assert_eq!(&reason, want, "{graph:?}");
        }
        (got, want) => panic!("encode_graph gave {got:?}, the oracle {want:?}, on {graph:?}"),
    }
    expected
}

/// The unnamed-edge family: a table `a` with one structured column `x`
/// whose inner edges to `a.x.k` and `a.x.item` carry no name, so that their
/// column names are their target vertices.
fn unnamed_inner_spec() -> oracle::Spec {
    oracle::Spec {
        tables: vec![("a", vec![("x", "value")])],
        relations: vec![],
        inner: vec![("a.x", "a.x.k", "", "str"), ("a.x", "a.x.item", "", "int")],
    }
}

/// Spec ruling S3 (1) (b), (c) names a record's inner edges by the column
/// name of the edge, the target vertex when the edge has no name. On every
/// graph of the unnamed-edge family with at most two table nodes (a record
/// over both targets, a record repeating one target, a list, and a leaf),
/// the encoding agrees with the oracle, and the graph-only fold of every
/// accepted cell is the cell the encoding reads.
#[test]
fn encoding_names_unnamed_inner_edges_by_their_target() {
    use oracle::Form;
    let spec = unnamed_inner_spec();
    let side = spec.side();
    let forms = vec![
        Form::Record(vec![
            ("", "a.x.k", Value::Str("s".into())),
            ("", "a.x.item", Value::Int(1)),
        ]),
        Form::Record(vec![
            ("", "a.x.k", Value::Str("s".into())),
            ("", "a.x.k", Value::Str("t".into())),
        ]),
        Form::List(vec![
            ("", "a.x.item", Value::Int(0)),
            ("", "a.x.item", Value::Int(1)),
        ]),
        Form::Leaf(FieldPresence::Present(Value::Int(0))),
    ];
    let graphs = oracle::every_graph(&spec, &|_, _| forms.clone(), 2);
    assert_eq!(graphs.len(), 1 + 4 + 16);
    let mut accepted = 0;
    let mut refused = 0;
    for graph in &graphs {
        if assert_encoding_matches_the_oracle_on(&spec, graph).is_ok() {
            accepted += 1;
            for node in graph.nodes().values() {
                if node.anchor == "a" {
                    let cell = graph
                        .out_edges(node.id)
                        .iter()
                        .map(|id| graph.edge(*id).unwrap().tgt)
                        .next()
                        .unwrap();
                    assert_eq!(
                        naive_fold(graph, cell),
                        Some(oracle::fold(graph, cell)),
                        "{graph:?}"
                    );
                }
            }
        } else {
            refused += 1;
        }
    }
    let record = &graphs[1];
    let (tables, _) = encode_graph(
        record,
        EncodingSide::Whole(&RelSchema::new(&spec.schema(), &protocol()).unwrap()),
    )
    .unwrap();
    assert_eq!(
        tables.rows("a")[0].1["x"],
        Value::Unknown(HashMap::from([
            ("a.x.k".to_owned(), Value::Str("s".into())),
            ("a.x.item".to_owned(), Value::Int(1)),
        ]))
    );
    assert_eq!(oracle::Content::of_graph(record, &side).table(), tables);
    assert!(accepted > 0 && refused > 0, "{accepted} {refused}");
}

const fn reason_name(reason: &NotEncodableReason) -> &'static str {
    match reason {
        NotEncodableReason::FanNode { .. } => "FanNode",
        NotEncodableReason::AnchorOutsideSide { .. } => "AnchorOutsideSide",
        NotEncodableReason::EdgeOutsideSide { .. } => "EdgeOutsideSide",
        NotEncodableReason::IncoherentEndpoints { .. } => "IncoherentEndpoints",
        NotEncodableReason::ParallelEdges { .. } => "ParallelEdges",
        NotEncodableReason::MissingCell { .. } => "MissingCell",
        NotEncodableReason::CellAsExtraField { .. } => "CellAsExtraField",
        NotEncodableReason::RepeatedCell { .. } => "RepeatedCell",
        NotEncodableReason::SharedValue { .. } => "SharedValue",
        NotEncodableReason::UnrootedValue { .. } => "UnrootedValue",
        NotEncodableReason::MissingValue { .. } => "MissingValue",
        NotEncodableReason::InnerValueShape { .. } => "InnerValueShape",
        NotEncodableReason::NestedLabeledNull { .. } => "NestedLabeledNull",
        _ => "an unlisted reason",
    }
}

/// The refusal half of the encoding domain, exhaustively: every single
/// mutation of every graph of the encoding family with at most two table
/// nodes, at each of its first eight picks, is accepted or refused by
/// `encode_graph` exactly as the oracle's five passes decide, with the
/// oracle's first reason; and the mutations reach every `NotEncodableReason`.
#[test]
fn every_not_encodable_reason_is_reached_and_matches_the_oracle() {
    let reached = std::sync::Mutex::new(BTreeSet::new());
    let accepted = std::sync::atomic::AtomicUsize::new(0);
    par(&encoding_graphs(2), |graph| {
        for kind in 0..MUTATIONS {
            for pick in 0..8 {
                match assert_encoding_matches_the_oracle(&mutate(graph, kind, pick)) {
                    Ok(()) => {
                        accepted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(reason) => {
                        reached.lock().unwrap().insert(reason_name(&reason));
                    }
                }
            }
        }
    });
    let every: BTreeSet<&str> = [
        "FanNode",
        "AnchorOutsideSide",
        "EdgeOutsideSide",
        "IncoherentEndpoints",
        "ParallelEdges",
        "MissingCell",
        "CellAsExtraField",
        "RepeatedCell",
        "SharedValue",
        "UnrootedValue",
        "MissingValue",
        "InnerValueShape",
        "NestedLabeledNull",
    ]
    .into_iter()
    .collect();
    assert_eq!(reached.into_inner().unwrap(), every);
    assert!(accepted.into_inner() > 0);
}

/// A hand-computed Pi fibre product: along the merge of `a{x}` and `b{x}`
/// into `t{x}`, Pi of `a#0{x=0}, a#1{x=1}, b#2{x=0}, b#3{x=0}` holds one
/// `t` row per pair `(a row, b row)` that agrees on `x`, that is `(0, 2)` and
/// `(0, 3)`, each with `x = 0`, and its provenance names both rows of each
/// pair.
#[test]
fn graph_pi_keeps_the_agreeing_pairs_of_a_merge() {
    let case = kan_family()
        .into_iter()
        .find(|case| case.name == "a merge with a shared column")
        .unwrap();
    let functor = case.functor();
    let x = |table: &str| oracle::Spec::column_edge(table, "x");
    let leaf = |id: u32, anchor: &str, value: i64| {
        Node::new(id, anchor).with_value(FieldPresence::Present(Value::Int(value)))
    };
    let graph = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "a"))
        .with_node(Node::new(2, "b"))
        .with_node(Node::new(3, "b"))
        .with_node(leaf(4, "a.x", 0))
        .with_node(leaf(5, "a.x", 1))
        .with_node(leaf(6, "b.x", 0))
        .with_node(leaf(7, "b.x", 0))
        .with_edge_id(EdgeId(0), 0, 4, x("a"))
        .with_edge_id(EdgeId(1), 1, 5, x("a"))
        .with_edge_id(EdgeId(2), 2, 6, x("b"))
        .with_edge_id(EdgeId(3), 3, 7, x("b"));
    let (pi, provenance) = g_pi_along_with_provenance(&graph, &functor, FAMILY_PI_MAX).unwrap();
    let tuples: BTreeSet<Vec<RowId>> = provenance
        .table
        .tuples
        .values()
        .map(|(_, tuple)| tuple.clone())
        .collect();
    assert_eq!(
        tuples,
        BTreeSet::from([vec![RowId(0), RowId(2)], vec![RowId(0), RowId(3)]])
    );
    let content = oracle::Content::of_graph(&pi, &case.target.side());
    assert_eq!(content.rows.len(), 2);
    for (row, (table, cells)) in &content.rows {
        assert_eq!(table, "t");
        assert_eq!(cells["x"], Value::Int(0));
        let (_, tuple) = &provenance.table.tuples[&RowId(*row)];
        let sources: BTreeSet<u32> = tuple.iter().map(|row| row.0).collect();
        assert_eq!(provenance.nodes[row], sources);
    }
}
