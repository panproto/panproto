//! Graph lens restoration, domain and presentation laws.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::{BTreeMap, HashMap};

use panproto_expr::Expr;
use panproto_inst::identity::{IdentityCarrier, IdentityField};
use panproto_inst::{
    CompiledMigration, EdgeId, Fan, FanDomain, FieldTransform, GEdge, GInstance, GraphComplement,
    HYPER_LABEL_KIND, HYPER_PARENT_KIND, IdentityError, Node, NodeShape, PutDomainError,
    SynthIndex, SynthKey, Value, ViewRef, WInstance,
};
use panproto_lens::graph_lens::{
    Aligner, Alignment, GraphComposite, IdAligner, SequentialGraphLens, align, check_aligner,
    check_put_domain, compose as compose_graph, contraction_witnesses, fresh_view_edge_id,
    fresh_view_node_id, get, put,
};
use panproto_lens::lens_shape::{GraphShape, LensShape};
use panproto_lens::{
    ComplementCompose, ErrorDetail, LawShape, Lens, LensError, LensPath, Mutation, MutationCases,
    SmallScope,
};
use panproto_schema::{Edge, Key, KeyPart, KeyRole, KeyScope, Protocol, Schema, SchemaBuilder};
use proptest::prelude::*;

fn relation() -> Edge {
    Edge {
        src: "a".into(),
        tgt: "b".into(),
        kind: "rel".into(),
        name: Some("b".into()),
    }
}

fn schema(include_b: bool) -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol)
        .vertex("a", "record", None)
        .expect("a");
    if include_b {
        builder = builder
            .vertex("b", "record", None)
            .expect("b")
            .edge("a", "b", "rel", Some("b"))
            .expect("edge");
    }
    builder.build().expect("schema")
}

fn identity_lens(schema: &Schema) -> Lens {
    Lens {
        compiled: CompiledMigration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.keys().cloned().collect(),
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    }
}

fn raw_complement_free_put(lens: &Lens, view: &GInstance) -> Result<GInstance, LensError> {
    let mut complement = GraphComplement::empty();
    complement.stamp(
        lens.identity(panproto_inst::ComplementShape::Graph),
        &lens.src_schema,
    );
    put(lens, view, &complement)
}

fn projection_lens() -> Lens {
    let source = schema(true);
    Lens {
        compiled: CompiledMigration {
            surviving_verts: std::iter::once("a".into()).collect(),
            ..CompiledMigration::default()
        },
        src_schema: source,
        tgt_schema: schema(false),
    }
}

fn source_graph() -> GInstance {
    GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "b"))
        .with_edge_id(EdgeId(4), 0, 1, relation())
        .with_value(0, Value::Str("kept".to_owned()))
        .with_value(1, Value::Str("dropped".to_owned()))
}

fn generated_source(values: &[i16]) -> GInstance {
    values
        .iter()
        .enumerate()
        .fold(GInstance::new(), |graph, (i, value)| {
            let base = u32::try_from(i).expect("small generated index") * 2;
            graph
                .with_node(Node::new(base, "a"))
                .with_node(Node::new(base + 1, "b"))
                .with_edge_id(EdgeId(100 + base), base, base + 1, relation())
                .with_value(base + 1, Value::Int(i64::from(*value)))
        })
}

fn edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "rel".into(),
        name: Some(name.into()),
    }
}

fn built_schema(vertices: &[&str], edges: &[Edge]) -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol);
    for vertex in vertices {
        builder = builder.vertex(vertex, "record", None).expect("vertex");
    }
    for edge in edges {
        builder = builder
            .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
            .expect("edge");
    }
    builder.build().expect("schema")
}

fn fanned_fixture() -> (Schema, GInstance) {
    let protocol = Protocol::default();
    let fan_schema = SchemaBuilder::new(&protocol)
        .vertex("r", "record", None)
        .expect("r")
        .vertex("c", "record", None)
        .expect("c")
        .edge("r", "c", "rel", Some("c"))
        .expect("edge")
        .hyper_edge(
            "h",
            "rel",
            HashMap::from([
                ("p".to_owned(), "r".to_owned()),
                ("x".to_owned(), "c".to_owned()),
                ("y".to_owned(), "c".to_owned()),
            ]),
            "p",
        )
        .expect("hyper edge")
        .hyper_edge(
            "h2",
            "rel",
            HashMap::from([
                ("q".to_owned(), "r".to_owned()),
                ("z".to_owned(), "c".to_owned()),
                ("y".to_owned(), "c".to_owned()),
            ]),
            "q",
        )
        .expect("target hyper edge")
        .build()
        .expect("schema");
    let relation = edge("r", "c", "c");
    let tree = WInstance::new(
        [
            (0, Node::new(0, "r")),
            (1, Node::new(1, "c")),
            (2, Node::new(2, "c")),
        ]
        .into(),
        vec![(0, 1, relation.clone()), (0, 2, relation)],
        vec![Fan {
            hyper_edge_id: "h".to_owned(),
            parent: 0,
            children: HashMap::from([("x".to_owned(), 1), ("y".to_owned(), 2)]),
        }],
        0,
        "r".into(),
    );
    let graph = GInstance::from_wtype_with_schema(&tree, &fan_schema).expect("graph");
    (fan_schema, graph)
}

#[test]
fn graph_get_put_restores_exactly_what_get_set_aside() {
    let lens = projection_lens();
    let source = source_graph();
    let (view, complement) = get(&lens, &source).expect("get");
    assert_eq!(view.node_count(), 1);
    assert!(complement.dropped_nodes.contains_key(&1));
    assert!(complement.dropped_edges.contains_key(&EdgeId(4)));
    let restored = put(&lens, &view, &complement).expect("put");
    assert!(GInstance::identical(&source, &restored));
}

#[test]
fn graph_put_without_complement_agrees_with_the_shape_classifier() {
    let schema = schema(false);
    let lens = identity_lens(&schema);
    let source = GInstance::new()
        .with_node(Node::new(7, "a"))
        .with_value(7, Value::Str("kept".to_owned()));
    let (view, complement) = get(&lens, &source).expect("get");
    assert!(complement.residue_is_trivial());
    assert!(panproto_lens::graph_lens::is_isomorphism(&lens));
    assert!(panproto_lens::graph_lens::obstruction_to_isomorphism(&lens).is_none());
    let restored = panproto_lens::graph_lens::put_without_complement(&lens, &view)
        .expect("complement-free put");
    assert!(GInstance::identical(&source, &restored));

    let projection = projection_lens();
    assert!(!panproto_lens::graph_lens::is_isomorphism(&projection));
    assert!(panproto_lens::graph_lens::obstruction_to_isomorphism(&projection).is_some());
}

#[test]
fn graph_obstructions_are_reported() {
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: std::iter::once("b".into()).collect(),
            ..CompiledMigration::default()
        },
        src_schema: built_schema(
            &["b", "c", "a"],
            &[edge("a", "b", "ab"), edge("b", "c", "bc")],
        ),
        tgt_schema: built_schema(&["b"], &[]),
    };

    assert!(!panproto_lens::graph_lens::is_isomorphism(&lens));
    assert_eq!(
        panproto_lens::graph_lens::obstruction_to_isomorphism(&lens).as_deref(),
        Some("vertex `a` does not survive, so a graph node over it cannot be reconstructed")
    );
}

#[test]
fn graph_isomorphism_rejects_an_open_field_rename_collision() {
    let schema = schema(false);
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: std::iter::once("a".into()).collect(),
            field_transforms: std::iter::once((
                "a".into(),
                vec![FieldTransform::RenameField {
                    old_key: "old".to_owned(),
                    new_key: "new".to_owned(),
                }],
            ))
            .collect(),
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema,
    };
    let mut node = Node::new(7, "a");
    node.extra_fields.insert("old".to_owned(), Value::Int(1));
    node.extra_fields.insert("new".to_owned(), Value::Int(2));
    let source = GInstance::new().with_node(node);
    let (view, complement) = get(&lens, &source).expect("get");
    assert!(!complement.residue_is_trivial());
    assert!(!panproto_lens::graph_lens::is_isomorphism(&lens));
    assert!(
        panproto_lens::graph_lens::obstruction_to_isomorphism(&lens)
            .is_some_and(|detail| detail.contains("overwrite"))
    );
    assert!(matches!(
        panproto_lens::graph_lens::put_without_complement(&lens, &view),
        Err(LensError::NotAnIsomorphism { .. })
    ));
}

#[test]
fn graph_isomorphism_is_classified_throughout_finite_scope() {
    let source_schema = schema(true);
    let identity = identity_lens(&source_schema);
    let scope = SmallScope {
        max_nodes: 2,
        max_edges: 2,
    };
    let sources = GraphShape::enumerate_small(&source_schema, scope, &());
    assert!(!sources.is_empty());
    assert!(panproto_lens::graph_lens::is_isomorphism(&identity));
    let identity_round_trips = sources.iter().all(|source| {
        let (view, _) = get(&identity, source).expect("identity get in finite scope");
        raw_complement_free_put(&identity, &view)
            .is_ok_and(|restored| GInstance::identical(source, &restored))
    });
    assert_eq!(
        panproto_lens::graph_lens::is_isomorphism(&identity),
        identity_round_trips
    );

    let projection = projection_lens();
    assert!(!panproto_lens::graph_lens::is_isomorphism(&projection));
    let projection_round_trips = sources.iter().all(|source| {
        let (view, _) = get(&projection, source).expect("projection get in finite scope");
        raw_complement_free_put(&projection, &view)
            .is_ok_and(|restored| GInstance::identical(source, &restored))
    });
    assert!(
        !projection_round_trips,
        "the finite scope has a dropped node"
    );
    assert_eq!(
        panproto_lens::graph_lens::is_isomorphism(&projection),
        projection_round_trips
    );

    let protocol = Protocol::default();
    let fan_schema = SchemaBuilder::new(&protocol)
        .vertex("r", "record", None)
        .expect("parent")
        .vertex("c", "record", None)
        .expect("child")
        .hyper_edge(
            "h",
            "rel",
            HashMap::from([
                ("p".to_owned(), "r".to_owned()),
                ("x".to_owned(), "c".to_owned()),
            ]),
            "p",
        )
        .expect("hyper edge")
        .build()
        .expect("fan schema");
    let fan_sources = GraphShape::enumerate_small(
        &fan_schema,
        SmallScope {
            max_nodes: 3,
            max_edges: 2,
        },
        &(),
    );
    assert!(
        fan_sources.iter().any(|source| {
            source
                .edges()
                .values()
                .any(|edge| edge.edge.kind.as_str() == HYPER_PARENT_KIND)
                && source
                    .edges()
                    .values()
                    .all(|edge| edge.edge.kind.as_str() != HYPER_LABEL_KIND)
        }),
        "finite graph scope must include a parent-only fan"
    );
    let fan_identity = identity_lens(&fan_schema);
    assert!(!panproto_lens::graph_lens::is_isomorphism(&fan_identity));
    let fan_round_trips = fan_sources.iter().all(|source| {
        let (view, _) = get(&fan_identity, source).expect("finite fan get");
        raw_complement_free_put(&fan_identity, &view)
            .is_ok_and(|restored| GInstance::identical(source, &restored))
    });
    assert!(!fan_round_trips, "the finite scope has a reified hyperedge");
    assert_eq!(
        panproto_lens::graph_lens::is_isomorphism(&fan_identity),
        fan_round_trips
    );
}

#[test]
fn graph_put_preserves_an_edit_to_a_kept_node() {
    let lens = projection_lens();
    let (mut view, complement) = get(&lens, &source_graph()).expect("get");
    view.node_mut(0)
        .expect("node")
        .extra_fields
        .insert("edited".to_owned(), Value::Bool(true));
    let restored = put(&lens, &view, &complement).expect("put");
    assert_eq!(
        restored.node(0).expect("node").extra_fields.get("edited"),
        Some(&Value::Bool(true))
    );
    assert!(restored.node(1).is_some());
}

#[test]
fn a_view_id_below_the_floor_is_admitted() {
    let schema = schema(false);
    let lens = identity_lens(&schema);
    let source = GInstance::new().with_node(Node::new(10, "a"));
    let (mut view, complement) = get(&lens, &source).expect("get");
    assert!(complement.id_floor.node > 1);
    view.insert_node(Node::new(1, "a")).expect("new node");
    let restored = put(&lens, &view, &complement).expect("put");
    assert!(restored.node(1).is_some());
}

#[test]
fn a_reused_view_id_is_an_edit_in_place() {
    let lens = projection_lens();
    let (mut view, complement) = get(&lens, &source_graph()).expect("get");
    view.node_mut(0).expect("node").value = Some(panproto_inst::FieldPresence::Present(
        Value::Str("edited".to_owned()),
    ));
    let restored = put(&lens, &view, &complement).expect("put");
    assert_eq!(
        restored.node(0).expect("same node").value,
        Some(panproto_inst::FieldPresence::Present(Value::Str(
            "edited".to_owned()
        )))
    );
    assert_eq!(restored.edge(EdgeId(4)).expect("hidden edge").src, 0);
}

#[test]
fn a_reinserted_id_does_not_recover_its_residue() {
    let lens = projection_lens();
    let (mut view, complement) = get(&lens, &source_graph()).expect("first get");
    view.remove_node(0).expect("delete kept node");
    let committed = put(&lens, &view, &complement).expect("commit delete");
    assert!(committed.node(0).is_none());
    assert!(committed.edge(EdgeId(4)).is_none());

    let (mut next_view, next_complement) = get(&lens, &committed).expect("second get");
    next_view
        .insert_node(Node::new(0, "a"))
        .expect("reinsert id");
    let reinserted = put(&lens, &next_view, &next_complement).expect("commit reinsert");
    assert!(reinserted.node(0).is_some());
    assert!(reinserted.edge(EdgeId(4)).is_none());
}

#[test]
fn an_expanded_graph_edge_round_trips_and_rejects_a_synthesized_edit() {
    let source_edge = edge("a", "b", "b");
    let first = edge("a", "m", "m");
    let second = edge("m", "b", "b");
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into_iter().collect(),
            expansion_path: std::iter::once((("a".into(), "b".into()), vec!["m".into()])).collect(),
            ..CompiledMigration::default()
        },
        src_schema: built_schema(&["a", "b"], std::slice::from_ref(&source_edge)),
        tgt_schema: built_schema(&["a", "m", "b"], &[first, second]),
    };
    let source = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "b"))
        .with_edge_id(EdgeId(4), 0, 1, source_edge);
    let (mut view, complement) = get(&lens, &source).expect("get");
    assert_eq!(complement.synthesized_nodes.len(), 1);
    assert_eq!(complement.synthesized_edges.len(), 2);
    assert!(GInstance::identical(
        &source,
        &put(&lens, &view, &complement).expect("put")
    ));
    let synthesized = *complement.synthesized_nodes.keys().next().expect("node");
    view.node_mut(synthesized).expect("synth node").anchor = "b".into();
    assert!(matches!(
        check_put_domain(&lens, &view, &complement),
        Err(errors) if errors.iter().any(|error| matches!(
            error,
            PutDomainError::SynthesizedEdited { elem: ViewRef::Node(id) } if *id == synthesized
        ))
    ));
}

#[test]
fn a_contracted_graph_path_round_trips_and_refuses_a_new_contracted_edge() {
    let ad = edge("a", "d", "d");
    let db = edge("d", "b", "b");
    let ab = edge("a", "b", "b");
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into_iter().collect(),
            ..CompiledMigration::default()
        },
        src_schema: built_schema(&["a", "d", "b"], &[ad.clone(), db.clone()]),
        tgt_schema: built_schema(&["a", "b"], std::slice::from_ref(&ab)),
    };
    let source = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "d"))
        .with_node(Node::new(2, "b"))
        .with_edge_id(EdgeId(4), 0, 1, ad)
        .with_edge_id(EdgeId(5), 1, 2, db);
    let (mut view, complement) = get(&lens, &source).expect("get");
    assert_eq!(complement.synthesized_edges.len(), 1);
    let witnesses = contraction_witnesses(&lens, &source).expect("witnesses");
    assert_eq!(witnesses.values().next(), Some(&vec![EdgeId(4), EdgeId(5)]));
    assert!(GInstance::identical(
        &source,
        &put(&lens, &view, &complement).expect("put")
    ));
    view.insert_edge(EdgeId(20), GEdge::new(0, 2, ab))
        .expect("new edge");
    assert!(matches!(
        check_put_domain(&lens, &view, &complement),
        Err(errors) if errors.iter().any(|error| matches!(
            error,
            PutDomainError::ContractedEdgeInserted { elem: ViewRef::Edge(EdgeId(20)) }
        ))
    ));
}

#[test]
fn a_direct_graph_preimage_is_not_a_contracted_edge_negative() {
    let ad = edge("a", "d", "d");
    let db = edge("d", "b", "b");
    let direct = edge("a", "b", "direct");
    let ab = edge("a", "b", "b");
    let target = built_schema(&["a", "b"], std::slice::from_ref(&ab));
    let mut compiled = identity_lens(&target).compiled;
    compiled.edge_remap.insert(direct.clone(), ab);
    let lens = Lens {
        compiled,
        src_schema: built_schema(&["a", "d", "b"], &[ad, db, direct]),
        tgt_schema: target,
    };
    let source = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(2, "b"));
    let (view, complement) = get(&lens, &source).expect("get");

    assert!(matches!(
        GraphShape::mutation_cases(
            &lens,
            &source,
            &view,
            &complement,
            &(),
            Mutation::AddContractedEdge,
        ),
        MutationCases::Skip(panproto_lens::SkipReason::NoCandidate)
    ));
}

#[test]
fn editing_a_contracted_edge_is_rejected() {
    let ad = edge("a", "d", "d");
    let db = edge("d", "b", "b");
    let ab = edge("a", "b", "b");
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into_iter().collect(),
            ..CompiledMigration::default()
        },
        src_schema: built_schema(&["a", "d", "b"], &[ad.clone(), db.clone()]),
        tgt_schema: built_schema(&["a", "b"], &[ab]),
    };
    let source = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "d"))
        .with_node(Node::new(2, "b"))
        .with_edge_id(EdgeId(4), 0, 1, ad)
        .with_edge_id(EdgeId(5), 1, 2, db);
    let (mut view, complement) = get(&lens, &source).expect("get");
    let id = *complement.synthesized_edges.keys().next().expect("edge");
    let mut edited = view.remove_edge(id).expect("edge");
    edited.position = Some(1);
    view.insert_edge(id, edited).expect("edited edge");
    assert!(matches!(
        check_put_domain(&lens, &view, &complement),
        Err(errors) if errors.iter().any(|error| matches!(
            error,
            PutDomainError::SynthesizedEdited { elem: ViewRef::Edge(found) } if *found == id
        ))
    ));
}

#[test]
fn removing_a_chain_endpoint_is_not_a_synthesized_edit() {
    let ad = edge("a", "d", "d");
    let db = edge("d", "b", "b");
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into_iter().collect(),
            ..CompiledMigration::default()
        },
        src_schema: built_schema(&["a", "d", "b"], &[ad.clone(), db.clone()]),
        tgt_schema: built_schema(&["a", "b"], &[edge("a", "b", "b")]),
    };
    let source = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "d"))
        .with_node(Node::new(2, "b"))
        .with_edge_id(EdgeId(4), 0, 1, ad)
        .with_edge_id(EdgeId(5), 1, 2, db);
    let (mut view, complement) = get(&lens, &source).expect("get");
    let synthesized = *complement.synthesized_edges.keys().next().expect("edge");
    view.remove_edge(synthesized).expect("remove derived edge");
    view.remove_node(2).expect("remove endpoint");
    if let Err(errors) = check_put_domain(&lens, &view, &complement) {
        assert!(
            !errors
                .iter()
                .any(|error| matches!(error, PutDomainError::SynthesizedEdited { .. }))
        );
    }
    let restored = put(&lens, &view, &complement).expect("put");
    assert!(restored.node(2).is_none());
    assert!(restored.edge(EdgeId(5)).is_none());
}

#[test]
fn graph_put_reports_an_id_collision_before_restoration() {
    let lens = projection_lens();
    let (mut view, complement) = get(&lens, &source_graph()).expect("get");
    view.insert_node(Node::new(1, "a")).expect("insert");
    assert!(matches!(
        check_put_domain(&lens, &view, &complement),
        Err(errors) if matches!(errors.first(), Some(PutDomainError::IdCollision {
            elem: ViewRef::Node(1)
        }))
    ));
    assert!(matches!(
        put(&lens, &view, &complement),
        Err(LensError::PutDomain(error))
            if matches!(*error, PutDomainError::IdCollision { elem: ViewRef::Node(1) })
    ));
}

#[test]
fn a_view_added_node_under_a_merged_anchor_is_ambiguous() {
    let source_schema = built_schema(&["left", "right"], &[]);
    let target_schema = built_schema(&["merged"], &[]);
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: std::iter::once("merged".into()).collect(),
            vertex_remap: [
                ("left".into(), "merged".into()),
                ("right".into(), "merged".into()),
            ]
            .into_iter()
            .collect(),
            ..CompiledMigration::default()
        },
        src_schema: source_schema,
        tgt_schema: target_schema,
    };
    let (mut view, complement) = get(&lens, &GInstance::new()).expect("get");
    view.insert_node(Node::new(7, "merged")).expect("node");
    assert!(matches!(
        check_put_domain(&lens, &view, &complement),
        Err(errors) if errors.iter().any(|error| matches!(
            error,
            PutDomainError::AmbiguousReverseAnchor { elem: ViewRef::Node(7), candidates }
                if candidates.iter().map(panproto_gat::Name::as_str).eq(["left", "right"])
        ))
    ));
}

#[test]
fn put_domain_errors_are_sorted_by_the_report_key() {
    let source_edge = edge("a", "b", "b");
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into_iter().collect(),
            expansion_path: std::iter::once((("a".into(), "b".into()), vec!["m".into()])).collect(),
            ..CompiledMigration::default()
        },
        src_schema: built_schema(&["a", "b"], std::slice::from_ref(&source_edge)),
        tgt_schema: built_schema(
            &["a", "m", "b"],
            &[edge("a", "m", "m"), edge("m", "b", "b")],
        ),
    };
    let source = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "b"))
        .with_edge_id(EdgeId(4), 0, 1, source_edge);
    let (mut view, complement) = get(&lens, &source).expect("get");
    let synthesized = *complement.synthesized_nodes.keys().next().expect("node");
    view.node_mut(synthesized).expect("node").value =
        Some(panproto_inst::FieldPresence::Present(Value::Bool(true)));
    view.insert_edge(EdgeId(4), GEdge::new(0, 1, relation()))
        .expect("colliding edge id");
    let errors = check_put_domain(&lens, &view, &complement).expect_err("violations");
    assert!(matches!(
        errors.first(),
        Some(PutDomainError::IdCollision { .. })
    ));
    assert!(matches!(
        errors.get(1),
        Some(PutDomainError::SynthesizedEdited { .. })
    ));
}

#[test]
fn graph_put_domain_reports_duplicate_keys_and_missing_required_cells() {
    let mut keyed_schema = schema(true);
    keyed_schema.declarations.keys.insert(
        "a".into(),
        vec![Key {
            id: "by-b".into(),
            parts: vec![KeyPart::Path(vec![relation()])],
            role: KeyRole::Primary,
            scope: KeyScope::Global,
        }],
    );
    let keyed_lens = identity_lens(&keyed_schema);
    let mut first = Node::new(0, "a");
    first
        .extra_fields
        .insert("b".to_owned(), Value::Str("same".to_owned()));
    let mut second = Node::new(1, "a");
    second
        .extra_fields
        .insert("b".to_owned(), Value::Str("same".to_owned()));
    let duplicate = GInstance::new().with_node(first).with_node(second);
    let (view, complement) = get(&keyed_lens, &duplicate).expect("get duplicate");
    assert!(!panproto_lens::graph_lens::is_isomorphism(&keyed_lens));
    assert!(
        panproto_lens::graph_lens::obstruction_to_isomorphism(&keyed_lens)
            .is_some_and(|detail| detail.contains("duplicate key values"))
    );
    assert!(matches!(
        raw_complement_free_put(&keyed_lens, &view),
        Err(LensError::PutDomain(error))
            if matches!(*error, PutDomainError::DuplicateKey { .. })
    ));
    assert!(matches!(
        check_put_domain(&keyed_lens, &view, &complement),
        Err(errors) if errors.iter().any(|error| matches!(
            error,
            PutDomainError::DuplicateKey { table, elems, .. }
                if table.as_str() == "a" && elems.len() == 2
        ))
    ));

    let mut required_schema = schema(true);
    required_schema
        .required
        .insert("a".into(), vec![relation()]);
    let required_lens = identity_lens(&required_schema);
    let missing = GInstance::new().with_node(Node::new(0, "a"));
    let (view, complement) = get(&required_lens, &missing).expect("get missing");
    assert!(!panproto_lens::graph_lens::is_isomorphism(&required_lens));
    assert!(
        panproto_lens::graph_lens::obstruction_to_isomorphism(&required_lens)
            .is_some_and(|detail| detail.contains("requires field `b`"))
    );
    assert!(matches!(
        raw_complement_free_put(&required_lens, &view),
        Err(LensError::PutDomain(error))
            if matches!(*error, PutDomainError::MissingRequiredCell { .. })
    ));
    assert!(matches!(
        check_put_domain(&required_lens, &view, &complement),
        Err(errors) if errors.iter().any(|error| matches!(
            error,
            PutDomainError::MissingRequiredCell { elem: ViewRef::Node(0), field }
                if field == "b"
        ))
    ));

    let mut defaulted_schema = required_schema;
    defaulted_schema
        .defaults
        .insert("b".into(), Expr::Var("default-b".into()));
    let defaulted_lens = identity_lens(&defaulted_schema);
    let missing = GInstance::new().with_node(Node::new(0, "a"));
    let (view, _) = get(&defaulted_lens, &missing).expect("get defaulted required field");
    assert!(panproto_lens::graph_lens::is_isomorphism(&defaulted_lens));
    assert!(
        raw_complement_free_put(&defaulted_lens, &view)
            .is_ok_and(|restored| GInstance::identical(&missing, &restored))
    );
}

#[test]
fn a_non_boolean_survival_predicate_is_a_put_domain_refusal() {
    let schema = schema(false);
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: std::iter::once("a".into()).collect(),
            conditional_survival: std::iter::once(("a".into(), Expr::Var("x".into()))).collect(),
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema,
    };
    let mut node = Node::new(0, "a");
    node.extra_fields.insert("x".to_owned(), Value::Bool(true));
    let source = GInstance::new().with_node(node);
    let (mut view, complement) = get(&lens, &source).expect("get");
    view.node_mut(0)
        .expect("node")
        .extra_fields
        .insert("x".to_owned(), Value::Int(7));
    assert!(matches!(
        check_put_domain(&lens, &view, &complement),
        Err(errors) if errors.iter().any(|error| matches!(
            error,
            PutDomainError::SurvivalPredicateRefused {
                elem: ViewRef::Node(0),
                refusal: panproto_inst::PredicateRefusal::NotBoolean { got }
            } if got == "int"
        ))
    ));
}

#[test]
fn a_restored_node_failing_its_survival_predicate_is_refused() {
    let schema = schema(false);
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: std::iter::once("a".into()).collect(),
            conditional_survival: std::iter::once(("a".into(), Expr::Var("keep".into()))).collect(),
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema,
    };
    let (mut view, complement) = get(&lens, &GInstance::new()).expect("get");
    let mut node = Node::new(3, "a");
    node.extra_fields
        .insert("keep".to_owned(), Value::Bool(false));
    view.insert_node(node).expect("node");
    assert!(matches!(
        check_put_domain(&lens, &view, &complement),
        Err(errors) if errors.iter().any(|error| matches!(
            error,
            PutDomainError::SurvivalPredicateFails { elem: ViewRef::Node(3) }
        ))
    ));
}

#[test]
fn a_put_that_breaks_a_fan_is_refused() {
    let (fan_schema, source) = fanned_fixture();
    let lens = identity_lens(&fan_schema);
    let (mut view, complement) = get(&lens, &source).expect("get");
    let parent = view
        .edges_sorted()
        .find(|(_, edge)| edge.edge.kind.as_str() == "hyper_parent")
        .map(|(id, _)| id)
        .expect("parent edge");
    view.remove_edge(parent).expect("break fan");
    assert!(matches!(
        check_put_domain(&lens, &view, &complement),
        Err(errors) if errors.iter().any(|error| matches!(
            error,
            PutDomainError::MalformedFan { .. }
        ))
    ));
}

#[test]
fn fresh_graph_ids_avoid_view_reserved_and_synthesized_ids() {
    let view = GInstance::new()
        .with_node(Node::new(2, "a"))
        .with_node(Node::new(3, "a"))
        .with_edge_id(EdgeId(7), 2, 3, relation());
    let mut complement = GraphComplement::empty();
    complement.id_floor.node = 1;
    complement.id_floor.edge = 5;
    complement.dropped_nodes.insert(4, Node::new(4, "a"));
    complement.synthesized_nodes.insert(
        5,
        SynthKey::Contract {
            src: 2,
            tgt: 3,
            edge: relation(),
        },
    );
    complement
        .dropped_edges
        .insert(EdgeId(8), GEdge::new(2, 3, relation()));
    complement.synthesized_edges.insert(
        EdgeId(9),
        SynthKey::Contract {
            src: 2,
            tgt: 3,
            edge: relation(),
        },
    );
    assert_eq!(fresh_view_node_id(&view, &complement).expect("node id"), 6);
    assert_eq!(
        fresh_view_edge_id(&view, &complement).expect("edge id"),
        EdgeId(10)
    );
}

#[test]
fn graph_complement_synth_index_lists_every_synthesized_object() {
    let key = SynthKey::Contract {
        src: 1,
        tgt: 2,
        edge: relation(),
    };
    let mut complement = GraphComplement::empty();
    complement.synthesized_nodes.insert(7, key.clone());
    complement.synthesized_edges.insert(EdgeId(8), key.clone());
    assert_eq!(
        complement.synth_index(),
        SynthIndex {
            nodes: BTreeMap::from([(7, key.clone())]),
            edges: BTreeMap::from([(EdgeId(8), key)]),
        }
    );
}

#[test]
fn graph_complement_merge_detects_conflicts_and_has_empty_identity() {
    let lens = projection_lens();
    let (_, complement) = get(&lens, &source_graph()).expect("get");
    let empty = {
        let mut empty = GraphComplement::empty();
        empty.stamp(
            lens.identity(panproto_inst::ComplementShape::Graph),
            &lens.src_schema,
        );
        empty
    };
    let left = complement.compose(&empty).expect("left identity");
    let right = empty.compose(&complement).expect("right identity");
    assert_eq!(left.dropped_edges, complement.dropped_edges);
    assert_eq!(right.dropped_edges, complement.dropped_edges);
    let mut conflict = complement.clone();
    conflict
        .dropped_edges
        .get_mut(&EdgeId(4))
        .expect("edge")
        .src = 9;
    assert!(matches!(
        complement.compose(&conflict),
        Err(LensError::ComplementConflict {
            kind: "dropped_edges",
            ..
        })
    ));
}

#[test]
fn an_identity_lens_maps_back_a_view_added_edge() {
    let schema = schema(true);
    let lens = identity_lens(&schema);
    let source = source_graph();
    let (mut view, complement) = get(&lens, &source).expect("get");
    view.insert_edge(EdgeId(10), GEdge::new(0, 1, relation()))
        .expect("insert edge");
    let restored = put(&lens, &view, &complement).expect("put");
    assert_eq!(restored.edge(EdgeId(10)).expect("edge").edge, relation());
}

#[test]
fn id_alignment_preserves_a_graph_and_obeys_the_self_law() {
    let graph = source_graph();
    check_aligner(&graph, &IdAligner).expect("self law");
    let aligned = align(&graph, &graph, &IdAligner).expect("align");
    assert!(GInstance::identical(&graph, &aligned));
}

#[test]
fn a_non_injective_alignment_is_refused() {
    struct Bad;
    impl Aligner for Bad {
        fn align(&self, _old: &GInstance, _new: &GInstance) -> Alignment {
            Alignment {
                nodes: BTreeMap::from([(0, 0), (1, 0)]),
                edges: BTreeMap::new(),
            }
        }
    }
    assert!(check_aligner(&source_graph(), &Bad).is_err());
}

#[test]
fn an_incomplete_self_alignment_is_refused() {
    struct Empty;
    impl Aligner for Empty {
        fn align(&self, _old: &GInstance, _new: &GInstance) -> Alignment {
            Alignment::default()
        }
    }
    assert!(check_aligner(&source_graph(), &Empty).is_err());
}

#[test]
fn a_relabelled_fan_edge_round_trips_through_its_recorded_source_label() {
    let (fan_schema, source) = fanned_fixture();
    let mut compiled = CompiledMigration {
        surviving_verts: fan_schema.vertices.keys().cloned().collect(),
        surviving_edges: fan_schema.edges.keys().cloned().collect(),
        ..CompiledMigration::default()
    };
    compiled.hyper_resolver.insert(
        ("h".into(), vec!["x".into(), "y".into()]),
        (
            "h2".into(),
            HashMap::from([("x".into(), "z".into()), ("y".into(), "y".into())]),
        ),
    );
    let lens = Lens {
        compiled,
        src_schema: fan_schema.clone(),
        tgt_schema: fan_schema,
    };
    let (view, complement) = get(&lens, &source).expect("get");
    assert!(
        view.edges()
            .values()
            .any(|edge| edge.edge.name.as_deref() == Some("z"))
    );
    assert!(view.nodes().values().any(|node| node.anchor == "h2"));
    assert_eq!(complement.original_edges.len(), 1);
    let restored = put(&lens, &view, &complement).expect("put");
    assert!(GInstance::identical(&source, &restored));
    assert!(!panproto_lens::graph_lens::is_isomorphism(&lens));
    assert!(matches!(
        panproto_lens::graph_lens::put_without_complement(&lens, &view),
        Err(LensError::NotAnIsomorphism { .. })
    ));
}

#[test]
fn a_parent_only_fan_prevents_global_graph_isomorphism() {
    let schema = SchemaBuilder::new(&Protocol::default())
        .vertex("r", "record", None)
        .expect("root")
        .hyper_edge(
            "h",
            "rel",
            HashMap::from([("p".to_owned(), "r".to_owned())]),
            "p",
        )
        .expect("hyper-edge")
        .build()
        .expect("schema");
    let lens = identity_lens(&schema);
    let source = GInstance::new()
        .with_node(Node::new(0, "r"))
        .with_node(Node::new(1, "h").with_shape(NodeShape::HyperEdge))
        .with_edge_id(
            EdgeId(4),
            1,
            0,
            Edge {
                src: "h".into(),
                tgt: "r".into(),
                kind: HYPER_PARENT_KIND.into(),
                name: Some("p".into()),
            },
        );
    assert!(FanDomain::check_with_schema(&source, &lens.src_schema).is_ok());
    let (view, complement) = get(&lens, &source).expect("get");
    assert!(!complement.residue_is_trivial());
    assert!(!panproto_lens::graph_lens::is_isomorphism(&lens));
    assert!(matches!(
        panproto_lens::graph_lens::put_without_complement(&lens, &view),
        Err(LensError::NotAnIsomorphism { .. })
    ));
}

#[test]
fn a_nested_path_transform_is_not_a_graph_isomorphism() {
    let mut lens = identity_lens(&schema(false));
    lens.compiled.field_transforms.insert(
        "a".into(),
        vec![FieldTransform::PathTransform {
            path: vec!["cell".to_owned()],
            inner: Box::new(FieldTransform::RenameField {
                old_key: "x".to_owned(),
                new_key: "y".to_owned(),
            }),
        }],
    );
    assert!(!panproto_lens::graph_lens::is_isomorphism(&lens));
    assert!(
        panproto_lens::graph_lens::obstruction_to_isomorphism(&lens)
            .is_some_and(|detail| detail.contains("nested path transform"))
    );
}

#[test]
fn a_graph_lens_path_round_trips() {
    let lens = identity_lens(&schema(true));
    let path = LensPath::single(lens);
    let source = source_graph();
    let (view, complement) = path.get_graph(&source).expect("path get");
    let restored = path.put_graph(&view, &complement).expect("path put");
    assert!(GInstance::identical(&source, &restored));
    assert_eq!(path.graph_normal_form().len(), 1);
}

#[test]
fn a_lens_reports_its_contraction_mode() {
    let identity = identity_lens(&schema(true));
    assert_eq!(
        identity.contraction_mode().expect("mode"),
        panproto_inst::ContractionMode::None
    );
    assert!(identity.edge_image().is_ok());
    assert!(identity.contraction_plan().is_ok());

    let contracting = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into_iter().collect(),
            ..CompiledMigration::default()
        },
        src_schema: built_schema(
            &["a", "d", "b"],
            &[edge("a", "d", "d"), edge("d", "b", "b")],
        ),
        tgt_schema: built_schema(&["a", "b"], &[edge("a", "b", "b")]),
    };
    assert_eq!(
        contracting.contraction_mode().expect("mode"),
        panproto_inst::ContractionMode::Declared
    );
}

#[test]
fn graph_composition_uses_the_fusion_conditions() {
    let schema = schema(true);
    let identity = identity_lens(&schema);
    assert!(matches!(
        compose_graph(&identity, &identity).expect("identity composition"),
        GraphComposite::Compiled(_)
    ));

    let middle = built_schema(
        &["a", "d", "b"],
        &[edge("a", "d", "d"), edge("d", "b", "b")],
    );
    let target = built_schema(&["a", "b"], &[edge("a", "b", "b")]);
    let first = identity_lens(&middle);
    let second = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into_iter().collect(),
            ..CompiledMigration::default()
        },
        src_schema: middle,
        tgt_schema: target,
    };
    assert!(matches!(
        compose_graph(&first, &second).expect("contracting composition"),
        GraphComposite::Sequential(_)
    ));
}

#[test]
fn graph_equivalence_matches_synthesized_objects_by_key() {
    let key = SynthKey::Contract {
        src: 0,
        tgt: 1,
        edge: relation(),
    };
    let a = source_graph().with_node(Node::new(10, "a"));
    let b = source_graph().with_node(Node::new(11, "a"));
    let ia = SynthIndex {
        nodes: BTreeMap::from([(10, key.clone())]),
        edges: BTreeMap::new(),
    };
    let ib = SynthIndex {
        nodes: BTreeMap::from([(11, key)]),
        edges: BTreeMap::new(),
    };
    assert!(GraphShape::equivalent(&a, &ia, &b, &ib));
    assert!(!GInstance::identical(&a, &b));
}

#[test]
fn graph_equivalence_compares_source_edge_positions() {
    let left = source_graph();
    let mut right = left.clone();
    let mut changed = right.remove_edge(EdgeId(4)).expect("edge");
    changed.position = Some(1);
    right
        .insert_edge(EdgeId(4), changed)
        .expect("positioned edge");
    assert!(!GraphShape::equivalent(
        &left,
        &SynthIndex::default(),
        &right,
        &SynthIndex::default(),
    ));
}

#[test]
fn congruence_witnesses_are_equivalent_but_not_identical() {
    let source_edge = edge("a", "b", "b");
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into_iter().collect(),
            expansion_path: std::iter::once((("a".into(), "b".into()), vec!["m".into()])).collect(),
            ..CompiledMigration::default()
        },
        src_schema: built_schema(&["a", "b"], std::slice::from_ref(&source_edge)),
        tgt_schema: built_schema(
            &["a", "m", "b"],
            &[edge("a", "m", "m"), edge("m", "b", "b")],
        ),
    };
    let source = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "b"))
        .with_edge_id(EdgeId(4), 0, 1, source_edge);
    let (view, complement) = get(&lens, &source).expect("get");
    let index = GraphShape::synth_index(&view, &complement);
    let witnesses = GraphShape::congruence_witnesses(&view, &index);
    assert_eq!(witnesses.len(), 1);
    let (witness, witness_index) = &witnesses[0];
    assert!(GraphShape::equivalent(
        &view,
        &index,
        witness,
        witness_index,
    ));
    assert!(!GInstance::identical(&view, witness));
}

#[test]
fn complement_divergence_ignores_derived_bookkeeping() {
    let key = SynthKey::Contract {
        src: 0,
        tgt: 1,
        edge: relation(),
    };
    let mut left = GraphComplement::empty();
    left.synthesized_nodes.insert(7, key.clone());
    left.id_floor.node = 8;
    let mut right = GraphComplement::empty();
    right.synthesized_nodes.insert(17, key);
    right.id_floor.node = 18;
    assert!(GraphShape::complement_divergence(&left, &right).is_none());
    right.dropped_nodes.insert(2, Node::new(2, "b"));
    assert!(GraphShape::complement_divergence(&left, &right).is_some());
}

#[test]
fn lens_identity_and_put_domain_errors_share_detail_variants() {
    let identity = LensError::Identity(IdentityError::Mismatch {
        field: IdentityField::Lens,
        expected: "a".to_owned(),
        found: "b".to_owned(),
    });
    assert!(matches!(
        identity.detail(),
        Some(ErrorDetail::IdentityMismatch { .. })
    ));
    let domain = LensError::PutDomain(Box::new(PutDomainError::IdCollision {
        elem: ViewRef::Node(3),
    }));
    assert!(matches!(
        domain.detail(),
        Some(ErrorDetail::PutDomain { .. })
    ));
}

proptest! {
    #[test]
    fn every_unchanged_view_edge_maps_back(values in prop::collection::vec(any::<i16>(), 0..12)) {
        let schema = schema(true);
        let lens = identity_lens(&schema);
        let source = generated_source(&values);
        let (view, complement) = get(&lens, &source).expect("get");
        let restored = put(&lens, &view, &complement).expect("put");
        prop_assert!(GInstance::identical(&source, &restored));
    }

    #[test]
    fn graph_complement_merge_is_a_partial_commutative_monoid(
        left_mask in prop::collection::vec(any::<bool>(), 0..16),
        right_mask in prop::collection::vec(any::<bool>(), 0..16),
        third_mask in prop::collection::vec(any::<bool>(), 0..16),
    ) {
        let lens = projection_lens();
        let complement = |mask: &[bool]| {
            let mut complement = GraphComplement::empty();
            for (index, keep) in mask.iter().copied().enumerate() {
                if keep {
                    let id = u32::try_from(index).expect("small generated index");
                    complement.dropped_nodes.insert(id, Node::new(id, "a"));
                }
            }
            complement.stamp(
                lens.identity(panproto_inst::ComplementShape::Graph),
                &lens.src_schema,
            );
            complement
        };
        let left = complement(&left_mask);
        let right = complement(&right_mask);
        let third = complement(&third_mask);
        let empty = complement(&[]);
        let lr = left.compose(&right).expect("compatible");
        let rl = right.compose(&left).expect("compatible");
        prop_assert!(GraphShape::complement_divergence(&lr, &rl).is_none());
        let l_rt = left.compose(&empty).expect("right identity");
        let l_lt = empty.compose(&left).expect("left identity");
        prop_assert!(GraphShape::complement_divergence(&left, &l_rt).is_none());
        prop_assert!(GraphShape::complement_divergence(&left, &l_lt).is_none());
        let lr_t = lr.compose(&third).expect("left associative form");
        let r_t = right.compose(&third).expect("right subexpression");
        let l_rt = left.compose(&r_t).expect("right associative form");
        prop_assert!(GraphShape::complement_divergence(&lr_t, &l_rt).is_none());
    }

    #[test]
    fn the_cascade_is_the_cartesian_lift_of_the_view_deletion(
        values in prop::collection::vec(any::<i16>(), 1..12),
        removed in prop::collection::vec(any::<bool>(), 1..12),
    ) {
        let lens = projection_lens();
        let source = generated_source(&values);
        let (mut view, complement) = get(&lens, &source).expect("get");
        let mut expected = source;
        for index in 0..values.len() {
            if removed.get(index).copied().unwrap_or(false) {
                let id = u32::try_from(index).expect("small generated index") * 2;
                let edge_id = EdgeId(100 + id);
                view.remove_node(id).expect("isolated view node");
                expected.remove_edge(edge_id).expect("source edge");
                expected.remove_node(id).expect("source node");
            }
        }
        let restored = put(&lens, &view, &complement).expect("put");
        prop_assert!(GInstance::identical(&expected, &restored));
    }

    #[test]
    fn compiled_and_sequential_graph_composites_agree(
        values in prop::collection::vec(any::<i16>(), 0..12),
    ) {
        let schema = schema(true);
        let first = identity_lens(&schema);
        let second = identity_lens(&schema);
        let compiled = compose_graph(&first, &second).expect("compose");
        let sequential = GraphComposite::Sequential(Box::new(SequentialGraphLens {
            first,
            second,
        }));
        let source = generated_source(&values);
        let (compiled_view, compiled_complement) = compiled.get(&source).expect("compiled get");
        let (sequential_view, sequential_complement) = sequential.get(&source).expect("sequential get");
        prop_assert!(GraphShape::equivalent(
            &compiled_view,
            &SynthIndex::default(),
            &sequential_view,
            &SynthIndex::default(),
        ));
        let compiled_source = compiled.put(&compiled_view, &compiled_complement).expect("compiled put");
        let sequential_source = sequential.put(&sequential_view, &sequential_complement).expect("sequential put");
        prop_assert!(GInstance::identical(&compiled_source, &sequential_source));
        prop_assert!(GInstance::identical(&source, &compiled_source));
    }

    #[test]
    fn graph_lens_path_laws(values in prop::collection::vec(any::<i16>(), 0..12)) {
        let source_schema = schema(true);
        let first = LensPath::single(identity_lens(&source_schema));
        let second = LensPath::single(projection_lens());
        let path = first.then(&second).expect("path");
        let source = generated_source(&values);
        let (view, complement) = path.get_graph(&source).expect("get");
        let restored = path.put_graph(&view, &complement).expect("put");
        prop_assert!(GInstance::identical(&source, &restored));
    }
}
