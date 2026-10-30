#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Relational graph Kan examples, refusals, and finite laws.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_inst::{
    AdjunctionError, BoundaryProtocols, CellLayout, CompiledMigration, EdgeId, EdgeLayout,
    EncodingSide, FInstance, FieldPresence, GInstance, GraphKanError, GraphKanHom, GraphLayout,
    MigrationFunctor, Node, NodeShape, NotEncodableReason, RelSchema, RelSchemaError,
    RestrictError, Row, RowId, Value, WInstance, decode_graph, encode_graph, g_counit, g_delta,
    g_delta_along, g_delta_with_provenance, g_pi, g_pi_along, g_pi_along_with_provenance,
    g_pi_counit, g_pi_restricted, g_pi_transpose_left, g_pi_transpose_right, g_pi_unit,
    g_pi_with_provenance, g_sigma, g_sigma_along, g_sigma_along_with_provenance,
    g_sigma_restricted, g_sigma_with_provenance, g_transpose_left, g_transpose_right, g_unit,
    graph_extend, graph_extend_engine, wtype_extend_engine, wtype_pi,
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
                    // A field is named by its edge, or by the edge's target
                    // vertex when the edge has no name.
                    let name = edge
                        .edge
                        .name
                        .as_ref()
                        .map_or_else(|| edge.edge.tgt.to_string(), ToString::to_string);
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

/// P1-O706's exact agreement domain includes both spellings of a list
/// element edge and, in particular, the element-vertex swap with no table
/// rename. These six fixtures are injective, column-covered and
/// transform-free; their two realized list edges also have distinct image
/// pairs. The oracle is `g_sigma`, whose implementation reaches the
/// presheaf Sigma through the relational encoding rather than the total
/// graph-extension engine.
#[test]
fn graph_sigma_agrees_with_total_graph_extend_on_its_exact_domain() {
    let fixtures = bijective_list_columns();
    assert_eq!(fixtures.len(), 6, "finite P1-O706 fixture count");
    for (case, fixture) in fixtures {
        let sigma = list_sigma(&fixture);
        let extend = graph_extend(
            &fixture.graph,
            &fixture.source,
            &fixture.target,
            &fixture.migration,
            BoundaryProtocols {
                src: &protocol(),
                tgt: &protocol(),
            },
        )
        .unwrap_or_else(|error| panic!("{case}: total graph extend refused: {error:?}"));
        assert!(
            GInstance::identical(&sigma, &extend),
            "{case}: sigma {sigma:?}, extend {extend:?}"
        );
    }
    println!(
        "P1-O706 finite bound: 6 list cases = 2 edge spellings x 3 rename/image classes; all compared"
    );
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
