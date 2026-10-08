//! Graph lens restoration, domain and presentation laws.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_inst::identity::{IdentityCarrier, IdentityField, LegacyRejectReason};
use panproto_inst::{
    BoundaryProtocols, CompiledMigration, EdgeId, Fan, FanDomain, FidelityReason, FieldTransform,
    GEdge, GInstance, GraphComplement, HYPER_LABEL_KIND, HYPER_PARENT_KIND, IdentityError, Node,
    NodeShape, PutDomainError, RestrictError, SynthIndex, SynthKey, Value, ViewRef, WInstance,
};
use panproto_lens::graph_lens::{
    Aligner, Alignment, GraphComposite, IdAligner, SequentialGraphLens, align, check_aligner,
    check_put_domain, compose as compose_graph, contraction_witnesses, fresh_view_edge_id,
    fresh_view_node_id, get, get_with_protocols, put,
};
use panproto_lens::lens_shape::{GraphShape, LensShape};
use panproto_lens::{
    ComplementCompose, ErrorDetail, LawShape, Lens, LensError, LensPath, Mutation, MutationCases,
    SmallScope,
};
use panproto_schema::{
    Edge, Key, KeyPart, KeyRole, KeyScope, Protocol, ScalarKind, Schema, SchemaBuilder,
};
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

#[test]
fn graph_law_wrappers_propagate_plan_errors() {
    let source_schema = schema(true);
    let mut lens = identity_lens(&source_schema);
    lens.compiled
        .edge_remap
        .insert(relation(), edge("a", "a", "wrong"));
    let source = source_graph();

    assert!(panproto_lens::graph_lens::check_get_put(&lens, &source).is_err());
    assert!(panproto_lens::graph_lens::check_put_get(&lens, &source).is_err());
    assert!(panproto_lens::graph_lens::check_put_twice(&lens, &source).is_err());
    assert!(panproto_lens::graph_lens::check_weak_put_get(&lens, &source).is_err());
    assert!(panproto_lens::graph_lens::check_put_injectivity(&lens, &source).is_err());
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

fn graph_is_subinstance(candidate: &GInstance, source: &GInstance) -> bool {
    candidate.nodes().iter().all(|(id, node)| {
        source.node(*id).is_some_and(|source_node| {
            GInstance::identical(
                &GInstance::new().with_node(node.clone()),
                &GInstance::new().with_node(source_node.clone()),
            )
        })
    }) && candidate
        .edges()
        .iter()
        .all(|(id, edge)| source.edge(*id) == Some(edge))
}

fn graph_subinstances(source: &GInstance) -> Vec<GInstance> {
    let nodes: Vec<_> = source.nodes().iter().collect();
    let edges: Vec<_> = source.edges().iter().collect();
    let mut result = Vec::new();
    for node_mask in 0..(1_usize << nodes.len()) {
        let mut base = GInstance::new();
        let kept: BTreeSet<u32> = nodes
            .iter()
            .enumerate()
            .filter_map(|(index, (id, _))| ((node_mask >> index) & 1 == 1).then_some(**id))
            .collect();
        for (id, node) in &nodes {
            if kept.contains(id) {
                base.insert_node((*node).clone())
                    .expect("distinct source node");
            }
        }
        let eligible: Vec<_> = edges
            .iter()
            .filter(|(_, edge)| kept.contains(&edge.src) && kept.contains(&edge.tgt))
            .collect();
        for edge_mask in 0..(1_usize << eligible.len()) {
            let mut graph = base.clone();
            for (index, (id, edge)) in eligible.iter().enumerate() {
                if (edge_mask >> index) & 1 == 1 {
                    graph
                        .insert_edge(**id, (*edge).clone())
                        .expect("distinct source edge");
                }
            }
            result.push(graph);
        }
    }
    result
}

/// Independent Delta-fragment restriction used only as the GF5 oracle.
/// It deliberately calls no restrict, lens, put-domain, or cascade code.
fn reference_delta_restrict(lens: &Lens, source: &GInstance) -> GInstance {
    let mut view = GInstance::new();
    for node in source.nodes().values() {
        if lens.compiled.surviving_verts.contains(&node.anchor)
            && lens.tgt_schema.vertices.contains_key(&node.anchor)
        {
            view.insert_node(node.clone()).expect("distinct node");
        }
    }
    for (id, edge) in source.edges() {
        if view.node(edge.src).is_some()
            && view.node(edge.tgt).is_some()
            && lens.compiled.surviving_edges.contains(&edge.edge)
            && lens.tgt_schema.edges.contains_key(&edge.edge)
        {
            view.insert_edge(*id, edge.clone()).expect("distinct edge");
        }
    }
    view
}

fn reference_largest_delta_source(
    lens: &Lens,
    source: &GInstance,
    edited_view: &GInstance,
) -> (GInstance, Vec<GInstance>) {
    let subinstances = graph_subinstances(source);
    let exact: Vec<_> = subinstances
        .iter()
        .filter(|candidate| {
            GInstance::identical(&reference_delta_restrict(lens, candidate), edited_view)
        })
        .cloned()
        .collect();
    let largest = exact
        .iter()
        .find(|candidate| {
            exact
                .iter()
                .all(|other| graph_is_subinstance(other, candidate))
        })
        .expect("the edited view has a largest source preimage")
        .clone();
    (largest, subinstances)
}

fn assert_delta_cascade_oracle(lens: &Lens, source: &GInstance, edited_view: &GInstance) {
    let (_, complement) = get(lens, source).expect("production get");
    let actual = put(lens, edited_view, &complement).expect("production put");
    let (expected, subinstances) = reference_largest_delta_source(lens, source, edited_view);
    assert!(
        GInstance::identical(&actual, &expected),
        "cascade did not return the independently enumerated largest preimage"
    );
    assert!(graph_is_subinstance(&actual, source));
    for candidate in subinstances {
        let restriction = reference_delta_restrict(lens, &candidate);
        if graph_is_subinstance(&restriction, edited_view) {
            assert!(
                graph_is_subinstance(&candidate, &actual),
                "universal-property witness was not contained in put output"
            );
        }
    }
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

fn partially_pruned_fan_fixture() -> (Lens, WInstance) {
    let protocol = Protocol::default();
    let left = edge("parent", "left", "left");
    let right = edge("parent", "right", "right");
    let source_schema = SchemaBuilder::new(&protocol)
        .vertex("parent", "record", None)
        .expect("parent")
        .vertex("left", "record", None)
        .expect("left")
        .vertex("right", "record", None)
        .expect("right")
        .edge("parent", "left", "rel", Some("left"))
        .expect("left edge")
        .edge("parent", "right", "rel", Some("right"))
        .expect("right edge")
        .hyper_edge(
            "h",
            "rel",
            HashMap::from([
                ("p".to_owned(), "parent".to_owned()),
                ("x".to_owned(), "left".to_owned()),
                ("y".to_owned(), "right".to_owned()),
            ]),
            "p",
        )
        .expect("source fan")
        .build()
        .expect("source schema");
    let target_schema = SchemaBuilder::new(&protocol)
        .vertex("parent", "record", None)
        .expect("parent")
        .vertex("left", "record", None)
        .expect("left")
        .edge("parent", "left", "rel", Some("left"))
        .expect("left edge")
        .hyper_edge(
            "h-left",
            "rel",
            HashMap::from([
                ("p".to_owned(), "parent".to_owned()),
                ("z".to_owned(), "left".to_owned()),
            ]),
            "p",
        )
        .expect("target fan")
        .build()
        .expect("target schema");
    let mut compiled = CompiledMigration {
        surviving_verts: ["parent".into(), "left".into()].into_iter().collect(),
        surviving_edges: std::iter::once(left.clone()).collect(),
        edge_remap: HashMap::from([(left.clone(), left.clone())]),
        ..CompiledMigration::default()
    };
    compiled.hyper_resolver.insert(
        ("h".into(), vec!["x".into(), "y".into()]),
        ("h-left".into(), HashMap::from([("x".into(), "z".into())])),
    );
    let source = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "parent")),
            (1, Node::new(1, "left")),
            (2, Node::new(2, "right")),
        ]),
        vec![(0, 1, left), (0, 2, right)],
        vec![Fan::new("h", 0).with_child("x", 1).with_child("y", 2)],
        0,
        "parent".into(),
    );
    (
        Lens {
            compiled,
            src_schema: source_schema,
            tgt_schema: target_schema,
        },
        source,
    )
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
fn editing_a_non_invertible_transform_output_is_refused() {
    let schema = schema(false);
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: std::iter::once("a".into()).collect(),
            field_transforms: HashMap::from([(
                "a".into(),
                vec![FieldTransform::ApplyExpr {
                    key: "n".to_owned(),
                    expr: Expr::Builtin(
                        BuiltinOp::Add,
                        vec![Expr::Var("n".into()), Expr::Lit(Literal::Int(1))],
                    ),
                    inverse: None,
                    coercion_class: panproto_gat::CoercionClass::Opaque,
                }],
            )]),
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema,
    };
    let mut node = Node::new(0, "a");
    node.extra_fields.insert("n".to_owned(), Value::Int(1));
    let (mut view, complement) = get(&lens, &GInstance::new().with_node(node)).expect("get");
    view.node_mut(0)
        .expect("node")
        .extra_fields
        .insert("n".to_owned(), Value::Int(7));

    assert!(matches!(
        check_put_domain(&lens, &view, &complement),
        Err(errors) if errors.contains(&PutDomainError::TransformNotInvertible {
            elem: ViewRef::Node(0),
            field: "n".to_owned(),
        })
    ));
}

#[test]
fn editing_a_derived_cell_is_refused() {
    let schema = schema(false);
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: std::iter::once("a".into()).collect(),
            field_transforms: HashMap::from([(
                "a".into(),
                vec![FieldTransform::ComputeField {
                    target_key: "double".to_owned(),
                    expr: Expr::Builtin(
                        BuiltinOp::Add,
                        vec![Expr::Var("n".into()), Expr::Var("n".into())],
                    ),
                    inverse: None,
                    coercion_class: panproto_gat::CoercionClass::Opaque,
                }],
            )]),
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema,
    };
    let mut node = Node::new(0, "a");
    node.extra_fields.insert("n".to_owned(), Value::Int(2));
    let (mut view, complement) = get(&lens, &GInstance::new().with_node(node)).expect("get");
    view.node_mut(0)
        .expect("node")
        .extra_fields
        .insert("double".to_owned(), Value::Int(9));

    assert!(matches!(
        check_put_domain(&lens, &view, &complement),
        Err(errors) if errors.contains(&PutDomainError::DerivedCellEdited {
            elem: ViewRef::Node(0),
            field: "double".to_owned(),
        })
    ));
}

fn duplicate_key_errors(role: KeyRole, scope: KeyScope) -> Vec<PutDomainError> {
    let mut keyed_schema = schema(false);
    keyed_schema.declarations.keys.insert(
        "a".into(),
        vec![Key {
            id: "by-value".into(),
            parts: vec![KeyPart::Path(Vec::new())],
            role,
            scope,
        }],
    );
    let lens = identity_lens(&keyed_schema);
    let duplicate = GInstance::new()
        .with_node(
            Node::new(0, "a").with_value(panproto_inst::FieldPresence::Present(Value::Str(
                "same".to_owned(),
            ))),
        )
        .with_node(
            Node::new(1, "a").with_value(panproto_inst::FieldPresence::Present(Value::Str(
                "same".to_owned(),
            ))),
        );
    let (view, complement) = get(&lens, &duplicate).expect("get");
    check_put_domain(&lens, &view, &complement).expect_err("duplicate key")
}

#[test]
fn a_restored_duplicate_is_refused_for_each_key_role() {
    for role in [KeyRole::Primary, KeyRole::Unique, KeyRole::Identifier] {
        assert!(duplicate_key_errors(role, KeyScope::Global).iter().any(
            |error| matches!(error, PutDomainError::DuplicateKey { elems, .. } if elems.len() == 2)
        ));
    }
}

#[test]
fn a_document_key_is_unique_over_the_document() {
    assert!(duplicate_key_errors(KeyRole::Unique, KeyScope::Document)
        .iter()
        .any(|error| matches!(error, PutDomainError::DuplicateKey { elems, .. } if elems.len() == 2)));
}

#[test]
fn a_within_key_is_unique_under_its_ancestor() {
    let contains = edge("p", "a", "items");
    let mut keyed_schema = built_schema(&["p", "a"], std::slice::from_ref(&contains));
    keyed_schema.declarations.keys.insert(
        "a".into(),
        vec![Key {
            id: "by-value".into(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::Unique,
            scope: KeyScope::Within("p".into()),
        }],
    );
    let lens = identity_lens(&keyed_schema);
    let value = || panproto_inst::FieldPresence::Present(Value::Str("same".to_owned()));
    let source = GInstance::new()
        .with_node(Node::new(0, "p"))
        .with_node(Node::new(1, "p"))
        .with_node(Node::new(2, "a").with_value(value()))
        .with_node(Node::new(3, "a").with_value(value()))
        .with_node(Node::new(4, "a").with_value(value()))
        .with_edge_id(EdgeId(10), 0, 2, contains.clone())
        .with_edge_id(EdgeId(11), 1, 3, contains.clone())
        .with_edge_id(EdgeId(12), 0, 4, contains);
    let (view, complement) = get(&lens, &source).expect("get");
    let errors = check_put_domain(&lens, &view, &complement).expect_err("same-parent duplicate");
    let duplicate = errors
        .iter()
        .find_map(|error| match error {
            PutDomainError::DuplicateKey { elems, .. } => Some(elems),
            _ => None,
        })
        .expect("duplicate key");
    assert_eq!(duplicate, &vec![ViewRef::Node(2), ViewRef::Node(4)]);
}

fn binary32_column_lens(output: f64) -> (Lens, Protocol) {
    let protocol = Protocol {
        value_kinds: BTreeMap::from([("float32".to_owned(), ScalarKind::Float { bits: 32 })]),
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    };
    let column = Edge {
        src: "row".into(),
        tgt: "row.x".into(),
        kind: "column".into(),
        name: Some("x".into()),
    };
    let schema = SchemaBuilder::new(&protocol)
        .vertex("row", "record", None)
        .expect("row")
        .vertex("row.x", "float32", None)
        .expect("value vertex")
        .edge(
            &column.src,
            &column.tgt,
            &column.kind,
            column.name.as_deref(),
        )
        .expect("column")
        .build()
        .expect("schema");
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.keys().cloned().collect(),
            field_transforms: HashMap::from([(
                "row".into(),
                vec![FieldTransform::ComputeField {
                    target_key: "x".to_owned(),
                    expr: Expr::Lit(Literal::Float(output)),
                    inverse: None,
                    coercion_class: panproto_gat::CoercionClass::Opaque,
                }],
            )]),
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema,
    };
    (lens, protocol)
}

#[test]
fn a_float_that_is_not_a_binary32_is_refused_by_get() {
    let (lens, protocol) = binary32_column_lens(0.1);
    let source = GInstance::new().with_node(Node::new(0, "row"));
    let error = get_with_protocols(
        &lens,
        &source,
        BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        },
    )
    .expect_err("0.1 is not exactly binary32");
    assert!(
        matches!(
            &error,
            LensError::Restrict(restrict)
                if matches!(
                    restrict.unlocated(),
                    RestrictError::ValueFidelity { node: 0, error }
                        if error.site.vertex.as_str() == "row.x"
                            && error.reason == FidelityReason::PrecisionExceeded { max: 9 }
                )
        ),
        "{error:?}"
    );
}

#[test]
fn an_exact_binary32_cell_is_restored_by_put() {
    let exact = f64::from(0.1_f32);
    let (lens, protocol) = binary32_column_lens(exact);
    let mut row = Node::new(0, "row");
    row.extra_fields.insert("x".to_owned(), Value::Int(7));
    let source = GInstance::new().with_node(row);
    let (view, complement) = get_with_protocols(
        &lens,
        &source,
        BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        },
    )
    .expect("exact binary32 get");
    assert_eq!(
        view.node(0).expect("row").extra_fields.get("x"),
        Some(&Value::Float(exact))
    );
    let restored = put(&lens, &view, &complement).expect("put");
    assert!(GInstance::identical(&source, &restored));
}

#[test]
fn the_base_graph_get_remains_boundary_free() {
    let (lens, _) = binary32_column_lens(0.1);
    let source = GInstance::new().with_node(Node::new(0, "row"));
    let (view, _) = get(&lens, &source).expect("protocol-free get");
    assert_eq!(
        view.node(0).expect("row").extra_fields.get("x"),
        Some(&Value::Float(0.1))
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

fn assert_cross_shape_agreement(case: &str, lens: &Lens, tree: &WInstance, graph: &GInstance) {
    let (tree_view, tree_complement) = panproto_lens::get(lens, tree).expect("tree get");
    let (graph_view, graph_complement) = get(lens, graph).expect("graph get");
    assert_eq!(
        lens.is_isomorphism(),
        panproto_lens::graph_lens::is_isomorphism(lens),
        "case {case}"
    );
    assert_eq!(
        tree_complement.residue_is_trivial(),
        graph_complement.residue_is_trivial(),
        "case {case}"
    );
    assert_eq!(
        panproto_lens::put_without_complement(lens, &tree_view).is_ok(),
        panproto_lens::graph_lens::put_without_complement(lens, &graph_view).is_ok(),
        "case {case}"
    );
    let tree_restored = panproto_lens::put(lens, &tree_view, &tree_complement).expect("tree put");
    let tree_restored_graph = GInstance::from_wtype_with_schema(&tree_restored, &lens.src_schema)
        .expect("restored tree graph form");
    let graph_restored = put(lens, &graph_view, &graph_complement).expect("graph put");
    assert!(
        GInstance::identical(graph, &tree_restored_graph),
        "tree GetPut case {case}"
    );
    assert!(
        GInstance::identical(graph, &graph_restored),
        "graph GetPut case {case}"
    );
}

#[test]
fn residue_classification_agrees_across_shapes() {
    // (a) Contraction bookkeeping is derivable, while the dropped interior
    // node is genuine residue in both shapes.
    let am = edge("a", "m", "m");
    let mb = edge("m", "b", "b");
    let ab = edge("a", "b", "b");
    let contraction = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into_iter().collect(),
            resolver: HashMap::from([(("a".into(), "b".into()), ab.clone())]),
            ..CompiledMigration::default()
        },
        src_schema: built_schema(&["a", "m", "b"], &[am.clone(), mb.clone()]),
        tgt_schema: built_schema(&["a", "b"], std::slice::from_ref(&ab)),
    };
    let contraction_tree = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "a")),
            (1, Node::new(1, "m")),
            (2, Node::new(2, "b")),
        ]),
        vec![(0, 1, am), (1, 2, mb)],
        vec![],
        0,
        "a".into(),
    );
    let contraction_graph =
        GInstance::from_wtype_with_schema(&contraction_tree, &contraction.src_schema)
            .expect("graph form");
    assert_cross_shape_agreement("a", &contraction, &contraction_tree, &contraction_graph);

    // (b) An injective vertex and edge rename loses no information.
    let source_edge = edge("a", "b", "b");
    let target_edge = edge("x", "y", "b");
    let rename = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["x".into(), "y".into()].into_iter().collect(),
            surviving_edges: std::iter::once(target_edge.clone()).collect(),
            vertex_remap: HashMap::from([("a".into(), "x".into()), ("b".into(), "y".into())]),
            edge_remap: HashMap::from([(source_edge.clone(), target_edge)]),
            ..CompiledMigration::default()
        },
        src_schema: built_schema(&["a", "b"], std::slice::from_ref(&source_edge)),
        tgt_schema: built_schema(&["x", "y"], &[edge("x", "y", "b")]),
    };
    let rename_tree = WInstance::new(
        HashMap::from([(0, Node::new(0, "a")), (1, Node::new(1, "b"))]),
        vec![(0, 1, source_edge)],
        vec![],
        0,
        "a".into(),
    );
    let rename_graph =
        GInstance::from_wtype_with_schema(&rename_tree, &rename.src_schema).expect("graph form");
    assert_cross_shape_agreement("b", &rename, &rename_tree, &rename_graph);

    // (c) Merging parallel edge labels records which source label carried
    // each edge and therefore is not complement-free in either shape.
    let sx = edge("a", "b", "x");
    let sy = edge("a", "b", "y");
    let tx = edge("a", "b", "x");
    let merge = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into_iter().collect(),
            surviving_edges: std::iter::once(tx.clone()).collect(),
            edge_remap: HashMap::from([(sx.clone(), tx.clone()), (sy.clone(), tx.clone())]),
            ..CompiledMigration::default()
        },
        src_schema: built_schema(&["a", "b"], &[sx.clone(), sy.clone()]),
        tgt_schema: built_schema(&["a", "b"], std::slice::from_ref(&tx)),
    };
    let merge_tree = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "a")),
            (1, Node::new(1, "b")),
            (2, Node::new(2, "b")),
        ]),
        vec![(0, 1, sx), (0, 2, sy)],
        vec![],
        0,
        "a".into(),
    );
    let merge_graph =
        GInstance::from_wtype_with_schema(&merge_tree, &merge.src_schema).expect("graph form");
    assert_cross_shape_agreement("c", &merge, &merge_tree, &merge_graph);

    // (d) A fan relabel is recorded residue in both shapes.
    let (fan_schema, fan_graph) = fanned_fixture();
    let fan_tree = fan_graph.try_to_wtype(&fan_schema).expect("tree form");
    let mut fan_compiled = CompiledMigration {
        surviving_verts: fan_schema.vertices.keys().cloned().collect(),
        surviving_edges: fan_schema.edges.keys().cloned().collect(),
        ..CompiledMigration::default()
    };
    fan_compiled.hyper_resolver.insert(
        ("h".into(), vec!["x".into(), "y".into()]),
        (
            "h2".into(),
            HashMap::from([("x".into(), "z".into()), ("y".into(), "y".into())]),
        ),
    );
    let fan_lens = Lens {
        compiled: fan_compiled,
        src_schema: fan_schema.clone(),
        tgt_schema: fan_schema,
    };
    assert_cross_shape_agreement("d", &fan_lens, &fan_tree, &fan_graph);
}

#[test]
fn a_child_pruned_partial_fan_is_not_duplicated_by_put() {
    let (lens, source) = partially_pruned_fan_fixture();
    let (view, complement) = panproto_lens::get(&lens, &source).expect("get");
    assert_eq!(view.fans.len(), 1);
    assert_eq!(view.fans[0].hyper_edge_id, "h-left");
    assert_eq!(view.fans[0].children, HashMap::from([("z".to_owned(), 1)]));
    assert_eq!(complement.dropped_fans, source.fans);

    let restored = panproto_lens::put(&lens, &view, &complement).expect("put");
    assert_eq!(restored.fans, source.fans);
}

#[test]
fn a_recorded_fan_is_not_restored_without_all_its_nodes() {
    let (lens, source) = partially_pruned_fan_fixture();
    let (view, mut complement) = panproto_lens::get(&lens, &source).expect("get");
    complement.dropped_nodes.remove(&2);
    complement.dropped_arcs.retain(|(_, child, _)| *child != 2);

    let restored = panproto_lens::put(&lens, &view, &complement).expect("put");
    assert_eq!(restored.fans, vec![Fan::new("h", 0).with_child("x", 1)]);
    assert!(restored.fans.iter().all(|fan| {
        restored.nodes.contains_key(&fan.parent)
            && fan
                .children
                .values()
                .all(|child| restored.nodes.contains_key(child))
    }));
}

#[test]
fn an_unchanged_fan_leaves_tree_residue_trivial() {
    let (schema, graph) = fanned_fixture();
    let source = graph.try_to_wtype(&schema).expect("tree form");
    let lens = identity_lens(&schema);
    let (view, complement) = panproto_lens::get(&lens, &source).expect("get");
    assert!(complement.dropped_fans.is_empty());
    assert!(complement.residue_is_trivial());
    let restored = panproto_lens::put(&lens, &view, &complement).expect("put");
    assert_eq!(restored.fans, source.fans);
}

#[test]
fn a_fan_id_is_not_reversed_through_the_vertex_remap() {
    let protocol = Protocol::default();
    let relation = edge("parent", "child", "child");
    let source_schema = SchemaBuilder::new(&protocol)
        .vertex("parent", "record", None)
        .expect("parent")
        .vertex("child", "record", None)
        .expect("child")
        .vertex("source-vertex", "record", None)
        .expect("source vertex")
        .edge("parent", "child", "rel", Some("child"))
        .expect("edge")
        .hyper_edge(
            "target-hyper",
            "rel",
            HashMap::from([
                ("p".to_owned(), "parent".to_owned()),
                ("x".to_owned(), "child".to_owned()),
            ]),
            "p",
        )
        .expect("source fan")
        .build()
        .expect("source schema");
    let target_schema = SchemaBuilder::new(&protocol)
        .vertex("parent", "record", None)
        .expect("parent")
        .vertex("child", "record", None)
        .expect("child")
        .vertex("target-hyper", "record", None)
        .expect("target vertex")
        .edge("parent", "child", "rel", Some("child"))
        .expect("edge")
        .hyper_edge(
            "target-hyper",
            "rel",
            HashMap::from([
                ("p".to_owned(), "parent".to_owned()),
                ("x".to_owned(), "child".to_owned()),
            ]),
            "p",
        )
        .expect("target fan")
        .build()
        .expect("target schema");
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["parent".into(), "child".into(), "target-hyper".into()]
                .into_iter()
                .collect(),
            surviving_edges: std::iter::once(relation.clone()).collect(),
            vertex_remap: HashMap::from([("source-vertex".into(), "target-hyper".into())]),
            edge_remap: HashMap::from([(relation.clone(), relation.clone())]),
            ..CompiledMigration::default()
        },
        src_schema: source_schema,
        tgt_schema: target_schema,
    };
    let source = WInstance::new(
        HashMap::from([(0, Node::new(0, "parent")), (1, Node::new(1, "child"))]),
        vec![(0, 1, relation)],
        vec![Fan::new("target-hyper", 0).with_child("x", 1)],
        0,
        "parent".into(),
    );

    let (view, complement) = panproto_lens::get(&lens, &source).expect("get");
    assert!(complement.dropped_fans.is_empty());
    let restored = panproto_lens::put(&lens, &view, &complement).expect("put");
    assert_eq!(restored.fans, source.fans);
}

#[test]
fn a_new_resolved_fan_uses_the_least_exact_backward_entry() {
    let protocol = Protocol::default();
    let relation = edge("parent", "child", "child");
    let source_schema = SchemaBuilder::new(&protocol)
        .vertex("parent", "record", None)
        .expect("parent")
        .vertex("child", "record", None)
        .expect("child")
        .edge("parent", "child", "rel", Some("child"))
        .expect("edge")
        .hyper_edge(
            "a-source",
            "rel",
            HashMap::from([
                ("p".to_owned(), "parent".to_owned()),
                ("a".to_owned(), "child".to_owned()),
                ("b".to_owned(), "child".to_owned()),
            ]),
            "p",
        )
        .expect("first source fan")
        .hyper_edge(
            "b-source",
            "rel",
            HashMap::from([
                ("p".to_owned(), "parent".to_owned()),
                ("y".to_owned(), "child".to_owned()),
            ]),
            "p",
        )
        .expect("second source fan")
        .build()
        .expect("source schema");
    let target_schema = SchemaBuilder::new(&protocol)
        .vertex("parent", "record", None)
        .expect("parent")
        .vertex("child", "record", None)
        .expect("child")
        .edge("parent", "child", "rel", Some("child"))
        .expect("edge")
        .hyper_edge(
            "target",
            "rel",
            HashMap::from([
                ("p".to_owned(), "parent".to_owned()),
                ("z".to_owned(), "child".to_owned()),
            ]),
            "p",
        )
        .expect("target fan")
        .build()
        .expect("target schema");
    for _ in 0..128 {
        let mut compiled = CompiledMigration {
            surviving_verts: ["parent".into(), "child".into()].into_iter().collect(),
            surviving_edges: std::iter::once(relation.clone()).collect(),
            edge_remap: HashMap::from([(relation.clone(), relation.clone())]),
            ..CompiledMigration::default()
        };
        compiled.hyper_resolver.insert(
            ("a-source".into(), vec!["a".into(), "b".into()]),
            (
                "target".into(),
                HashMap::from([("b".into(), "z".into()), ("a".into(), "z".into())]),
            ),
        );
        compiled.hyper_resolver.insert(
            ("b-source".into(), vec!["y".into()]),
            ("target".into(), HashMap::from([("y".into(), "z".into())])),
        );
        let lens = Lens {
            compiled,
            src_schema: source_schema.clone(),
            tgt_schema: target_schema.clone(),
        };
        let source = WInstance::new(
            HashMap::from([(0, Node::new(0, "parent")), (1, Node::new(1, "child"))]),
            vec![(0, 1, relation.clone())],
            vec![],
            0,
            "parent".into(),
        );
        let (mut view, complement) = panproto_lens::get(&lens, &source).expect("get");
        view.fans.push(Fan::new("target", 0).with_child("z", 1));

        let restored = panproto_lens::put(&lens, &view, &complement).expect("put");
        assert_eq!(
            restored.fans,
            vec![Fan::new("a-source", 0).with_child("a", 1)]
        );
    }
}

#[test]
fn an_unnamed_fan_parent_edge_stays_unnamed_through_graph_get_and_put() {
    let (schema, mut source) = fanned_fixture();
    let lens = identity_lens(&schema);
    let parent_id = source
        .edges_sorted()
        .find(|(_, edge)| edge.edge.kind.as_str() == HYPER_PARENT_KIND)
        .map(|(id, _)| id)
        .expect("parent edge");
    let mut parent = source.remove_edge(parent_id).expect("parent edge");
    parent.edge.name = None;
    source.insert_edge(parent_id, parent).expect("parent edge");
    assert!(FanDomain::check_with_schema(&source, &schema).is_ok());
    let (view, complement) = get(&lens, &source).expect("get");
    let view_parent = view
        .edges()
        .values()
        .find(|edge| edge.edge.kind.as_str() == HYPER_PARENT_KIND)
        .expect("parent edge");
    assert_eq!(view_parent.edge.name, None);
    let restored = put(&lens, &view, &complement).expect("put");
    let restored_parent = restored
        .edges()
        .values()
        .find(|edge| edge.edge.kind.as_str() == HYPER_PARENT_KIND)
        .expect("parent edge");
    assert_eq!(restored_parent.edge.name, None);
    assert!(GInstance::identical(&source, &restored));
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

fn assert_graph_composition_is_sequential(first: &Lens, second: &Lens) {
    assert!(matches!(
        compose_graph(first, second).expect("matching boundary"),
        GraphComposite::Sequential(_)
    ));
}

#[test]
fn a_contracting_pair_composes_sequentially() {
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
    assert_graph_composition_is_sequential(&first, &second);
}

#[test]
fn migrations_that_do_not_compose_stay_sequential() {
    let schema = schema(false);
    let mut first = identity_lens(&schema);
    first.compiled.field_transforms.insert(
        "a".into(),
        vec![FieldTransform::DropField {
            key: "lost".to_owned(),
        }],
    );
    let mut second = identity_lens(&schema);
    second.compiled.field_transforms.insert(
        "a".into(),
        vec![FieldTransform::ApplyExpr {
            key: "out".to_owned(),
            expr: Expr::Var("lost".into()),
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Opaque,
        }],
    );
    assert_graph_composition_is_sequential(&first, &second);
}

#[test]
fn disagreeing_direct_edge_images_stay_sequential() {
    let source_edge = edge("a", "b", "source");
    let middle_edge = edge("a", "b", "middle");
    let final_edge = source_edge.clone();
    let source = built_schema(&["a", "b"], std::slice::from_ref(&source_edge));
    let middle = built_schema(&["a", "b"], std::slice::from_ref(&middle_edge));
    let target = built_schema(&["a", "b"], std::slice::from_ref(&final_edge));
    let first = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into_iter().collect(),
            surviving_edges: std::iter::once(middle_edge.clone()).collect(),
            edge_remap: HashMap::from([(source_edge, middle_edge)]),
            ..CompiledMigration::default()
        },
        src_schema: source,
        tgt_schema: middle.clone(),
    };
    let second = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into_iter().collect(),
            ..CompiledMigration::default()
        },
        src_schema: middle,
        tgt_schema: target,
    };
    assert_graph_composition_is_sequential(&first, &second);
}

#[test]
fn an_expanding_plan_composes_sequentially() {
    let source_edge = edge("a", "b", "b");
    let source = built_schema(&["a", "b"], std::slice::from_ref(&source_edge));
    let target = built_schema(
        &["a", "m", "b"],
        &[edge("a", "m", "m"), edge("m", "b", "b")],
    );
    let first = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into_iter().collect(),
            expansion_path: HashMap::from([(("a".into(), "b".into()), vec!["m".into()])]),
            ..CompiledMigration::default()
        },
        src_schema: source,
        tgt_schema: target.clone(),
    };
    let second = identity_lens(&target);
    assert_graph_composition_is_sequential(&first, &second);
}

#[test]
fn two_value_layers_compose_sequentially() {
    let schema = schema(false);
    let with_layer = |field: &str| {
        let mut lens = identity_lens(&schema);
        lens.compiled.field_transforms.insert(
            "a".into(),
            vec![FieldTransform::AddField {
                key: field.to_owned(),
                value: Value::Int(0),
            }],
        );
        lens
    };
    assert_graph_composition_is_sequential(&with_layer("x"), &with_layer("y"));
}

#[test]
fn a_hyper_resolver_composes_sequentially() {
    let schema = schema(false);
    let mut first = identity_lens(&schema);
    first.compiled.hyper_resolver.insert(
        ("h".into(), vec!["x".into()]),
        ("h2".into(), HashMap::from([("x".into(), "y".into())])),
    );
    assert_graph_composition_is_sequential(&first, &identity_lens(&schema));
}

#[test]
fn a_plan_that_does_not_compile_stays_sequential() {
    let schema = schema(true);
    let mut first = identity_lens(&schema);
    first.compiled.edge_remap.insert(
        relation(),
        Edge {
            src: "a".into(),
            tgt: "a".into(),
            kind: "rel".into(),
            name: Some("bad".into()),
        },
    );
    assert_graph_composition_is_sequential(&first, &identity_lens(&schema));
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
    let unbound = LensError::Identity(IdentityError::UnboundComplement);
    assert_eq!(unbound.detail(), Some(ErrorDetail::UnboundComplement {}));
    let domain = LensError::PutDomain(Box::new(PutDomainError::IdCollision {
        elem: ViewRef::Node(3),
    }));
    assert!(matches!(
        domain.detail(),
        Some(ErrorDetail::PutDomain { .. })
    ));

    let legacy = LensError::Identity(IdentityError::LegacyComplementRejected {
        reason: LegacyRejectReason::FingerprintMismatch,
    });
    assert!(
        legacy.detail().is_none(),
        "the typed source error has no fingerprint, so detail must not invent one"
    );
    assert!(matches!(
        legacy,
        LensError::Identity(IdentityError::LegacyComplementRejected {
            reason: LegacyRejectReason::FingerprintMismatch
        })
    ));
}

fn delta_fixture_lenses() -> Vec<Lens> {
    let first = projection_lens();

    let visible = edge("a", "c", "visible");
    let hidden = edge("c", "b", "hidden");
    let source = built_schema(&["a", "b", "c"], &[visible.clone(), hidden]);
    let target = built_schema(&["a", "c"], std::slice::from_ref(&visible));
    let second = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "c".into()].into_iter().collect(),
            surviving_edges: std::iter::once(visible).collect(),
            ..CompiledMigration::default()
        },
        src_schema: source,
        tgt_schema: target,
    };
    vec![first, second]
}

fn every_graph_at_bound(schema: &Schema, max_nodes: usize, max_edges: usize) -> Vec<GInstance> {
    let mut anchors: Vec<_> = schema.vertices.keys().cloned().collect();
    anchors.sort_unstable();
    let mut schema_edges: Vec<_> = schema.edges.keys().cloned().collect();
    schema_edges.sort_unstable();
    let mut result = Vec::new();
    for node_count in 0..=max_nodes {
        let assignments = anchors
            .len()
            .pow(u32::try_from(node_count).expect("small bound"));
        for mut assignment in 0..assignments {
            let mut graph = GInstance::new();
            let mut node_anchors = Vec::with_capacity(node_count);
            for id in 0..node_count {
                let anchor = anchors[assignment % anchors.len()].clone();
                assignment /= anchors.len();
                node_anchors.push(anchor.clone());
                graph
                    .insert_node(Node::new(u32::try_from(id).expect("small bound"), anchor))
                    .expect("distinct node");
            }
            let mut candidates = Vec::new();
            for src in 0..node_count {
                for tgt in 0..node_count {
                    for edge in &schema_edges {
                        if edge.src == node_anchors[src] && edge.tgt == node_anchors[tgt] {
                            candidates.push((
                                u32::try_from(src).expect("small bound"),
                                u32::try_from(tgt).expect("small bound"),
                                edge.clone(),
                            ));
                        }
                    }
                }
            }
            for edge_mask in 0..(1_usize << candidates.len()) {
                if usize::try_from(edge_mask.count_ones()).expect("edge count") > max_edges {
                    continue;
                }
                let mut instance = graph.clone();
                for (index, (src, tgt, edge)) in candidates.iter().enumerate() {
                    if (edge_mask >> index) & 1 == 1 {
                        instance
                            .insert_edge(
                                EdgeId(u32::try_from(index).expect("small bound")),
                                GEdge::new(*src, *tgt, edge.clone()),
                            )
                            .expect("distinct edge");
                    }
                }
                result.push(instance);
            }
        }
    }
    result
}

#[test]
fn the_cascade_is_the_largest_subinstance_on_every_small_instance() {
    const MAX_NODES: usize = 3;
    const MAX_EDGES: usize = 3;
    let mut source_count = 0_usize;
    let mut edit_count = 0_usize;
    let mut candidate_count = 0_usize;
    let fixtures = delta_fixture_lenses();
    for lens in &fixtures {
        for source in every_graph_at_bound(&lens.src_schema, MAX_NODES, MAX_EDGES) {
            source_count += 1;
            let view = reference_delta_restrict(lens, &source);
            for edited_view in graph_subinstances(&view) {
                edit_count += 1;
                candidate_count += graph_subinstances(&source).len();
                assert_delta_cascade_oracle(lens, &source, &edited_view);
            }
        }
    }
    eprintln!(
        "GF5 exhaustive bound: schemas={}, nodes<={MAX_NODES}, edges<={MAX_EDGES}, sources={source_count}, deletion_edits={edit_count}, source_subinstances={candidate_count}",
        fixtures.len()
    );
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
    fn the_cascade_is_the_largest_subinstance_with_the_edited_view(
        values in prop::collection::vec(any::<i16>(), 1..4),
        removed in prop::collection::vec(any::<bool>(), 1..4),
    ) {
        let lens = projection_lens();
        let source = generated_source(&values);
        let (mut view, complement) = get(&lens, &source).expect("get");
        for index in 0..values.len() {
            if removed.get(index).copied().unwrap_or(false) {
                let id = u32::try_from(index).expect("small generated index") * 2;
                view.remove_node(id).expect("isolated view node");
            }
        }
        let restored = put(&lens, &view, &complement).expect("put");
        let (expected, candidates) = reference_largest_delta_source(&lens, &source, &view);
        prop_assert!(GInstance::identical(&expected, &restored));
        for candidate in candidates {
            let restriction = reference_delta_restrict(&lens, &candidate);
            if graph_is_subinstance(&restriction, &view) {
                prop_assert!(graph_is_subinstance(&candidate, &restored));
            }
        }
    }

    #[test]
    fn every_fusion_preserves_sequential_behaviour_on_graphs(
        values in prop::collection::vec(any::<i16>(), 0..12),
    ) {
        let schema = schema(true);
        let source = generated_source(&values);
        let mut first_with_layer = identity_lens(&schema);
        first_with_layer.compiled.field_transforms.insert(
            "a".into(),
            vec![FieldTransform::RenameField {
                old_key: "old".to_owned(),
                new_key: "new".to_owned(),
            }],
        );
        let mut second_with_layer = identity_lens(&schema);
        second_with_layer.compiled.field_transforms.insert(
            "a".into(),
            vec![FieldTransform::RenameField {
                old_key: "old".to_owned(),
                new_key: "new".to_owned(),
            }],
        );
        let pairs = [
            (identity_lens(&schema), identity_lens(&schema)),
            (first_with_layer, identity_lens(&schema)),
            (identity_lens(&schema), second_with_layer),
        ];
        for (first, second) in pairs {
            let compiled = compose_graph(&first, &second).expect("compose");
            prop_assert!(matches!(compiled, GraphComposite::Compiled(_)));
            let sequential = GraphComposite::Sequential(Box::new(SequentialGraphLens {
                first,
                second,
            }));
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
