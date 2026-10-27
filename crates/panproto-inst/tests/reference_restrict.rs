#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Release-scope differentials for restrict and contraction.
//!
//! The optimized production algorithms are compared with the deliberately
//! small, ordered reference carrier. The contraction region is checked a
//! second time by explicit walk enumeration, so agreement between two graph
//! searches is not accepted as its own proof.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_inst::{
    BoundaryProtocols, CellFate, CompiledMigration, ContractionPlan, EdgeFate, EdgeId, EdgeImage,
    ElementOps, FInstance, FieldPresence, GEdge, GInstance, GraphComplement, IdFloor, Node,
    PairResolution, RestrictError, RestrictPlan, RowId, SynthKey, UnresolvedReason, Value,
    WInstance, collect_scalar_child_values, functor_restrict_with_plan, graph_restrict_with_plan,
    resolve_pair, wtype_restrict_with_plan,
};
use panproto_reference::enumerate::restrict_and_contraction::{
    ChildBindingSlot, ColumnState, ExpansionState, PairResolutionCase, PairState, TreeFloorCase,
    TreeRestrictCase, TreeRoute, child_binding_cases, column_fate_cases, pair_resolution_cases,
    release_graphs, release_schemas, survival_classifications, tree_restrict_cases,
};
use panproto_reference::lens::{GraphLens as RefGraphLens, tree_get as reference_tree_get};
use panproto_reference::migration::Migration as RefMigration;
use panproto_reference::model::{
    GEdge as RefGEdge, Graph as RefGraph, Node as RefNode, SEdge as RefEdge, Tree as RefTree,
};
use panproto_reference::restrict::{
    self, ColumnFate as RefColumnFate, Complement as RefComplement, EdgeFate as RefEdgeFate,
    EngineError as RefEngineError, NoValueLayer, PairResolution as RefPairResolution,
    PlanError as RefPlanError, RestrictError as RefRestrictError, SynthKey as RefSynthKey,
    UnresolvedReason as RefUnresolvedReason,
};
use panproto_reference::schema::Schema as RefSchema;
use panproto_reference::value::{Presence as RefPresence, Value as RefValue};
use panproto_schema::{Edge, Protocol, ScalarKind, Schema, SchemaBuilder};

fn prod_edge(edge: &RefEdge) -> Edge {
    Edge {
        src: edge.src.as_str().into(),
        tgt: edge.tgt.as_str().into(),
        kind: edge.kind.as_str().into(),
        name: edge.name.as_deref().map(Into::into),
    }
}

fn ref_edge(edge: &Edge) -> RefEdge {
    RefEdge::new(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
}

fn prod_schema(schema: &RefSchema) -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol);
    for (vertex, kind) in &schema.vertices {
        builder = builder
            .vertex(vertex, kind, None)
            .expect("reference vertex");
    }
    for edge in &schema.edges {
        builder = builder
            .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
            .expect("reference edge");
    }
    builder.build().expect("reference schema")
}

fn prod_migration(migration: &RefMigration) -> CompiledMigration {
    let mut out = CompiledMigration {
        surviving_verts: migration
            .surviving_verts
            .iter()
            .map(|vertex| vertex.as_str().into())
            .collect(),
        surviving_edges: migration.surviving_edges.iter().map(prod_edge).collect(),
        vertex_remap: migration
            .vertex_remap
            .iter()
            .map(|(source, target)| (source.as_str().into(), target.as_str().into()))
            .collect(),
        edge_remap: migration
            .edge_remap
            .iter()
            .map(|(source, target)| (prod_edge(source), prod_edge(target)))
            .collect(),
        resolver: migration
            .resolver
            .iter()
            .map(|((source, target), edge)| {
                (
                    (source.as_str().into(), target.as_str().into()),
                    prod_edge(edge),
                )
            })
            .collect(),
        expansion_path: migration
            .expansion_path
            .iter()
            .map(|((source, target), path)| {
                (
                    (source.as_str().into(), target.as_str().into()),
                    path.iter().map(|anchor| anchor.as_str().into()).collect(),
                )
            })
            .collect(),
        ..CompiledMigration::default()
    };
    for vertex in &migration.conditional {
        out.conditional_survival
            .insert(vertex.as_str().into(), Expr::Lit(Literal::Bool(true)));
    }
    assert!(
        migration.hyper_resolver.is_empty() && migration.transformed.is_empty(),
        "release structural scope has no fan or value program"
    );
    out
}

fn edge_text(edge: &RefEdge) -> String {
    format!("{}>{}:{}:{:?}", edge.src, edge.tgt, edge.kind, edge.name)
}

fn ref_reason(reason: &RefUnresolvedReason) -> String {
    match reason {
        RefUnresolvedReason::NoEdge => "none".to_owned(),
        RefUnresolvedReason::Ambiguous { count } => format!("ambiguous:{count}"),
    }
}

fn prod_reason(reason: &UnresolvedReason) -> String {
    match reason {
        UnresolvedReason::NoEdge => "none".to_owned(),
        UnresolvedReason::Ambiguous { count } => format!("ambiguous:{count}"),
    }
}

fn ref_fate(fate: &RefEdgeFate) -> String {
    match fate {
        RefEdgeFate::Keep(edge) => format!("keep:{}", edge_text(edge)),
        RefEdgeFate::Expand {
            intermediates,
            labels,
        } => format!(
            "expand:{intermediates:?}:{}",
            labels.iter().map(edge_text).collect::<Vec<_>>().join("|")
        ),
        RefEdgeFate::Drop => "drop".to_owned(),
        RefEdgeFate::Unresolved(source, target, reason) => {
            format!("unresolved:{source}>{target}:{}", ref_reason(reason))
        }
    }
}

fn ref_fate_without_hop(fate: &RefEdgeFate) -> String {
    match fate {
        RefEdgeFate::Unresolved(_, _, reason) => {
            format!("unresolved:*:*:{}", ref_reason(reason))
        }
        other => ref_fate(other),
    }
}

fn prod_fate(fate: &EdgeFate) -> String {
    match fate {
        EdgeFate::Keep(edge) => format!("keep:{}", edge_text(&ref_edge(edge))),
        EdgeFate::Expand {
            intermediates,
            labels,
        } => format!(
            "expand:{:?}:{}",
            intermediates
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            labels
                .iter()
                .map(|edge| edge_text(&ref_edge(edge)))
                .collect::<Vec<_>>()
                .join("|")
        ),
        EdgeFate::Drop => "drop".to_owned(),
        EdgeFate::Unresolved(reason) => format!("unresolved:*:*:{}", prod_reason(reason)),
    }
}

fn ref_resolution(resolution: &RefPairResolution) -> String {
    match resolution {
        RefPairResolution::Label(edge) => format!("label:{}", edge_text(edge)),
        RefPairResolution::Expand {
            intermediates,
            labels,
        } => format!(
            "expand:{intermediates:?}:{}",
            labels.iter().map(edge_text).collect::<Vec<_>>().join("|")
        ),
        RefPairResolution::Unresolved(_, _, reason) => {
            format!("unresolved:{}", ref_reason(reason))
        }
    }
}

fn prod_resolution(resolution: &PairResolution) -> String {
    match resolution {
        PairResolution::Label(edge) => format!("label:{}", edge_text(&ref_edge(edge))),
        PairResolution::Expand {
            intermediates,
            labels,
        } => format!(
            "expand:{:?}:{}",
            intermediates
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            labels
                .iter()
                .map(|edge| edge_text(&ref_edge(edge)))
                .collect::<Vec<_>>()
                .join("|")
        ),
        PairResolution::Unresolved(reason) => format!("unresolved:{}", prod_reason(reason)),
    }
}

fn assert_single_edge_image(
    source: &RefSchema,
    target: &RefSchema,
    migration: &RefMigration,
    expected: &str,
) {
    let source_edge = source.edges.iter().next().expect("one source edge");
    assert_eq!(source.edges.len(), 1);
    let reference = restrict::edge_image(target, source, migration).expect("reference image");
    let production = EdgeImage::compile(
        &prod_schema(source),
        &prod_schema(target),
        &prod_migration(migration),
    )
    .expect("production image");
    assert_eq!(ref_fate_without_hop(&reference[source_edge]), expected);
    assert_eq!(
        prod_fate(production.fate(&prod_edge(source_edge)).unwrap()),
        expected
    );
}

fn remapped_edge_image_fixture() -> (RefSchema, RefEdge, RefMigration) {
    let source_edge = RefEdge::new("a", "b", "old", Some("source"));
    let source = RefSchema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .edge("a", "b", "old", Some("source"));
    let migration = RefMigration {
        surviving_verts: ["x", "y"].into_iter().map(str::to_owned).collect(),
        vertex_remap: BTreeMap::from([
            ("a".to_owned(), "x".to_owned()),
            ("b".to_owned(), "y".to_owned()),
        ]),
        ..RefMigration::default()
    };
    (source, source_edge, migration)
}

fn direct_edge_image_branch_cases() {
    let (source, source_edge, migration) = remapped_edge_image_fixture();
    let explicit = RefEdge::new("x", "y", "mapped", Some("explicit"));
    let explicit_target = RefSchema::default()
        .vertex("x", "record")
        .vertex("y", "record")
        .edge("x", "y", "mapped", Some("explicit"));
    let mut explicit_migration = migration.clone();
    explicit_migration
        .edge_remap
        .insert(source_edge, explicit.clone());
    assert_single_edge_image(
        &source,
        &explicit_target,
        &explicit_migration,
        &format!("keep:{}", edge_text(&explicit)),
    );

    let fallback = RefEdge::new("x", "y", "old", Some("source"));
    let fallback_target = RefSchema::default()
        .vertex("x", "record")
        .vertex("y", "record")
        .edge("x", "y", "old", Some("source"));
    assert_single_edge_image(
        &source,
        &fallback_target,
        &migration,
        &format!("keep:{}", edge_text(&fallback)),
    );

    let resolved = RefEdge::new("x", "y", "chosen", Some("resolver"));
    let resolver_target = RefSchema::default()
        .vertex("x", "record")
        .vertex("y", "record")
        .edge("x", "y", "chosen", Some("resolver"));
    let mut resolver_migration = migration;
    resolver_migration
        .resolver
        .insert(("x".to_owned(), "y".to_owned()), resolved.clone());
    assert_single_edge_image(
        &source,
        &resolver_target,
        &resolver_migration,
        &format!("keep:{}", edge_text(&resolved)),
    );
}

fn expansion_edge_image_branch_cases() {
    let (source, source_edge, migration) = remapped_edge_image_fixture();
    let first = RefEdge::new("x", "z", "hop", Some("first"));
    let second = RefEdge::new("z", "y", "hop", Some("second"));
    let expansion_target = RefSchema::default()
        .vertex("x", "record")
        .vertex("y", "record")
        .vertex("z", "record")
        .edge("x", "z", "hop", Some("first"))
        .edge("z", "y", "hop", Some("second"));
    let mut expansion_migration = migration.clone();
    expansion_migration
        .expansion_path
        .insert(("x".to_owned(), "y".to_owned()), vec!["z".to_owned()]);
    assert_single_edge_image(
        &source,
        &expansion_target,
        &expansion_migration,
        &format!(
            "expand:{:?}:{}|{}",
            vec!["z".to_owned()],
            edge_text(&first),
            edge_text(&second)
        ),
    );

    let unresolved_target = RefSchema::default()
        .vertex("x", "record")
        .vertex("y", "record")
        .vertex("z", "record")
        .edge("x", "z", "hop", Some("first"));
    assert_single_edge_image(
        &source,
        &unresolved_target,
        &expansion_migration,
        "unresolved:*:*:none",
    );
    let unresolved = restrict::edge_image(&unresolved_target, &source, &expansion_migration)
        .expect("unresolved hops are plan data");
    assert!(matches!(
        unresolved.get(&source_edge),
        Some(RefEdgeFate::Unresolved(src, tgt, RefUnresolvedReason::NoEdge))
            if src == "z" && tgt == "y"
    ));
    let failing = panproto_inst::contraction_plan::failing_hop(
        &prod_schema(&unresolved_target),
        &prod_migration(&expansion_migration),
        &"x".into(),
        &"y".into(),
    );
    assert_eq!((failing.0.as_ref(), failing.1.as_ref()), ("z", "y"));

    let absent = RefEdge::new("x", "y", "mapped", Some("absent"));
    let mut incoherent_migration = migration;
    incoherent_migration
        .edge_remap
        .insert(source_edge.clone(), absent.clone());
    let reference = restrict::edge_image(&expansion_target, &source, &incoherent_migration);
    assert!(matches!(
        reference,
        Err(RefPlanError::IncoherentEdgeImage { source_edge: found, image })
            if *found == source_edge && *image == absent
    ));
    let production = EdgeImage::compile(
        &prod_schema(&source),
        &prod_schema(&expansion_target),
        &prod_migration(&incoherent_migration),
    );
    assert!(matches!(
        production,
        Err(RestrictError::IncoherentEdgeImage { source_edge: found, image })
            if *found == prod_edge(&source_edge) && *image == prod_edge(&absent)
    ));
}

fn edge_image_branch_cases() {
    direct_edge_image_branch_cases();
    expansion_edge_image_branch_cases();
}

#[test]
fn edge_images_and_contraction_regions_agree_on_the_complete_release_scope() {
    let mut cases = 0usize;
    for source in release_schemas() {
        let production_schema = prod_schema(&source);
        for migration in survival_classifications(&source) {
            let production_migration = prod_migration(&migration);

            let reference_image = restrict::edge_image(&source, &source, &migration).unwrap();
            let production_image = EdgeImage::compile(
                &production_schema,
                &production_schema,
                &production_migration,
            )
            .unwrap();
            assert_eq!(reference_image.len(), production_image.fates.len());
            for (edge, expected) in &reference_image {
                let actual = production_image
                    .fate(&prod_edge(edge))
                    .expect("total production edge image");
                let expected = ref_fate_without_hop(expected);
                let actual = prod_fate(actual);
                assert_eq!(actual, expected, "edge {edge:?} under {migration:?}");
            }

            let optimized = restrict::contraction_plan(&source, &source, &migration).unwrap();
            let walked = restrict::contraction_plan_by_walks(&source, &source, &migration);
            assert_eq!(optimized, walked, "walk oracle under {migration:?}");
            let production = ContractionPlan::compile(
                &production_schema,
                &production_schema,
                &production_migration,
            )
            .unwrap();
            assert_eq!(optimized.len(), production.pairs.len());
            for (key, expected) in &optimized {
                let actual = production
                    .pairs
                    .get(&(key.0.as_str().into(), key.1.as_str().into()))
                    .expect("same pair domain");
                assert_eq!(
                    actual.target,
                    (
                        expected.target.0.as_str().into(),
                        expected.target.1.as_str().into()
                    )
                );
                assert_eq!(
                    actual.region.iter().map(ref_edge).collect::<BTreeSet<_>>(),
                    expected.region
                );
                assert_eq!(actual.bounded, expected.bounded);
                assert_eq!(
                    prod_resolution(&actual.resolution),
                    ref_resolution(&expected.resolution)
                );
            }
            cases += 1;
        }
    }
    assert_eq!(cases, 57 * 27);
    edge_image_branch_cases();
}

fn add_pair_state(
    mut schema: RefSchema,
    migration: &mut RefMigration,
    src: &str,
    tgt: &str,
    stem: &str,
    state: PairState,
) -> RefSchema {
    for index in 0..state.edge_count() {
        schema = schema.edge(src, tgt, "prop", Some(&format!("{stem}{index}")));
    }
    if state.has_resolver() {
        migration.resolver.insert(
            (src.to_owned(), tgt.to_owned()),
            RefEdge::new(src, tgt, "prop", Some(&format!("{stem}0"))),
        );
    }
    schema
}

fn resolution_case(case: PairResolutionCase) -> (RefSchema, RefMigration) {
    let mut migration = RefMigration::default();
    let mut schema = RefSchema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("c", "record");
    schema = add_pair_state(schema, &mut migration, "a", "b", "ab", case.direct);
    schema = add_pair_state(schema, &mut migration, "a", "c", "ac", case.first);
    schema = add_pair_state(schema, &mut migration, "c", "b", "cb", case.second);
    match case.expansion {
        ExpansionState::Absent => {}
        ExpansionState::Empty => {
            migration
                .expansion_path
                .insert(("a".to_owned(), "b".to_owned()), Vec::new());
        }
        ExpansionState::ViaC => {
            migration
                .expansion_path
                .insert(("a".to_owned(), "b".to_owned()), vec!["c".to_owned()]);
        }
    }
    (schema, migration)
}

#[test]
fn pair_resolution_precedence_and_first_failure_agree() {
    let cases = pair_resolution_cases();
    let mut outcomes = BTreeMap::<String, usize>::new();
    for case in &cases {
        let (target, migration) = resolution_case(*case);
        let production_target = prod_schema(&target);
        let production_migration = prod_migration(&migration);
        let reference = restrict::resolve_pair(&target, &migration, "a", "b");
        let production = resolve_pair(
            &production_target,
            &production_migration,
            &"a".into(),
            &"b".into(),
        );
        let expected = ref_resolution(&reference);
        *outcomes
            .entry(expected.split(':').next().unwrap().to_owned())
            .or_default() += 1;
        assert_eq!(prod_resolution(&production), expected, "{case:?}");
        if let RefPairResolution::Unresolved(source, target, _) = &reference {
            let actual = panproto_inst::contraction_plan::failing_hop(
                &production_target,
                &production_migration,
                &"a".into(),
                &"b".into(),
            );
            assert_eq!(
                (actual.0.to_string(), actual.1.to_string()),
                (source.clone(), target.clone()),
                "first failing hop for {case:?}"
            );
        }
    }
    assert_eq!(cases.len(), 375);
    assert_eq!(
        outcomes,
        BTreeMap::from([
            ("expand".to_owned(), 18),
            ("label".to_owned(), 225),
            ("unresolved".to_owned(), 132)
        ])
    );
}

fn ref_node(node: &Node) -> RefNode {
    let mut out = RefNode::new(node.id, &node.anchor);
    out.position = node.position;
    out
}

fn prod_node(node: &RefNode) -> Node {
    let mut out = Node::new(node.id, node.anchor.as_str());
    out.position = node.position;
    out
}

fn ref_tree(tree: &WInstance) -> RefTree {
    RefTree {
        nodes: tree
            .nodes
            .iter()
            .map(|(id, node)| (*id, ref_node(node)))
            .collect(),
        arcs: tree
            .arcs
            .iter()
            .map(|(parent, child, edge)| (*parent, *child, ref_edge(edge)))
            .collect(),
        fans: Vec::new(),
        root: tree.root,
    }
}

fn ref_graph(graph: &GInstance) -> RefGraph {
    RefGraph {
        nodes: graph
            .nodes()
            .iter()
            .map(|(id, node)| (*id, ref_node(node)))
            .collect(),
        edges: graph
            .edges()
            .iter()
            .map(|(id, edge)| {
                (
                    id.0,
                    RefGEdge {
                        src: edge.src,
                        tgt: edge.tgt,
                        label: ref_edge(&edge.edge),
                        position: edge.position,
                    },
                )
            })
            .collect(),
    }
}

fn prod_graph(graph: &RefGraph) -> GInstance {
    let mut out = GInstance::new();
    for node in graph.nodes.values() {
        out.insert_node(prod_node(node)).unwrap();
    }
    for (id, edge) in &graph.edges {
        out.insert_edge(
            EdgeId(*id),
            GEdge::new(edge.src, edge.tgt, prod_edge(&edge.label)).with_position(edge.position),
        )
        .unwrap();
    }
    out
}

fn ref_synth_key(key: &SynthKey) -> RefSynthKey {
    match key {
        SynthKey::Contract { src, tgt, edge } => RefSynthKey::Contract {
            src: *src,
            tgt: *tgt,
            edge: ref_edge(edge),
        },
        SynthKey::Expand { source_edge, hop } => RefSynthKey::Expand {
            source_edge: source_edge.0,
            hop: *hop,
        },
        SynthKey::ContractExpand { src, tgt, hop } => RefSynthKey::ContractExpand {
            src: *src,
            tgt: *tgt,
            hop: *hop,
        },
        other => panic!("key outside restrict scope: {other:?}"),
    }
}

fn ref_complement(complement: &GraphComplement) -> RefComplement {
    let IdFloor { node, edge } = complement.id_floor;
    RefComplement {
        dropped_nodes: complement
            .dropped_nodes
            .iter()
            .map(|(id, node)| (*id, ref_node(node)))
            .collect(),
        dropped_edges: complement
            .dropped_edges
            .iter()
            .map(|(id, edge)| {
                (
                    id.0,
                    RefGEdge {
                        src: edge.src,
                        tgt: edge.tgt,
                        label: ref_edge(&edge.edge),
                        position: edge.position,
                    },
                )
            })
            .collect(),
        original_edges: complement
            .original_edges
            .iter()
            .map(|(id, edge)| {
                (
                    id.0,
                    RefGEdge {
                        src: edge.src,
                        tgt: edge.tgt,
                        label: ref_edge(&edge.edge),
                        position: edge.position,
                    },
                )
            })
            .collect(),
        original_anchors: complement
            .original_anchors
            .iter()
            .map(|(id, anchor)| (*id, anchor.to_string()))
            .collect(),
        original_fields: BTreeMap::new(),
        original_values: BTreeMap::new(),
        synthesized_nodes: complement
            .synthesized_nodes
            .iter()
            .map(|(id, key)| (*id, ref_synth_key(key)))
            .collect(),
        synthesized_edges: complement
            .synthesized_edges
            .iter()
            .map(|(id, key)| (id.0, ref_synth_key(key)))
            .collect(),
        floor: (node, edge),
        identity: None,
    }
}

fn production_error(error: &RestrictError) -> String {
    match error {
        RestrictError::At { key, error } => {
            format!(
                "at:{}:{}:{}:{}",
                key.stage,
                key.id,
                key.id2,
                production_error(error)
            )
        }
        RestrictError::NoEdgeFound { src, tgt } => format!("none:{src}>{tgt}"),
        RestrictError::AmbiguousEdge { src, tgt, count } => {
            format!("ambiguous:{src}>{tgt}:{count}")
        }
        RestrictError::IncoherentEdgeImage { source_edge, .. } => {
            format!("incoherent:{}", edge_text(&ref_edge(source_edge)))
        }
        other => format!("other:{other:?}"),
    }
}

fn reference_error(error: &RefRestrictError) -> String {
    match error {
        RefRestrictError::At { key, error } => {
            let payload = reference_engine_error(error);
            format!("at:{}:{}:{}:{payload}", key.stage, key.id, key.id2)
        }
        other => format!("other:{other:?}"),
    }
}

fn reference_engine_error(error: &RefEngineError) -> String {
    match error {
        RefEngineError::NoEdgeFound { src, tgt } => format!("none:{src}>{tgt}"),
        RefEngineError::AmbiguousEdge { src, tgt, count } => {
            format!("ambiguous:{src}>{tgt}:{count}")
        }
        RefEngineError::IncoherentEdgeImage { label } => {
            format!("incoherent:{}", edge_text(label))
        }
        other => format!("other:{other:?}"),
    }
}

fn reference_unlocated_error(error: &RefRestrictError) -> String {
    match error {
        RefRestrictError::At { error, .. } => reference_engine_error(error),
        other => format!("other:{other:?}"),
    }
}

#[test]
fn graph_get_and_covered_reachability_agree_on_three_node_carriers() {
    let mut compared = 0usize;
    let mut successful = 0usize;
    for source in release_schemas() {
        let production_schema = prod_schema(&source);
        for migration in survival_classifications(&source) {
            let production_migration = prod_migration(&migration);
            let production_plan = RestrictPlan::compile(
                &production_schema,
                &production_schema,
                &production_migration,
            )
            .unwrap();
            for graph in release_graphs(&source) {
                let reference = restrict::get(&source, &source, &migration, &NoValueLayer, &graph);
                let production = graph_restrict_with_plan(&prod_graph(&graph), &production_plan);
                match (reference, production) {
                    (Ok((reference_view, reference_complement)), Ok((view, complement))) => {
                        assert_eq!(
                            ref_graph(&view),
                            reference_view,
                            "{graph:?} under {migration:?}"
                        );
                        assert_eq!(
                            ref_complement(&complement),
                            reference_complement,
                            "complement for {graph:?} under {migration:?}"
                        );
                        let surviving = reference_view.nodes.keys().copied().collect();
                        let walked = restrict::realized_pairs_by_walks(&graph, &surviving);
                        let synthesized: BTreeSet<(u32, u32)> = reference_complement
                            .synthesized_edges
                            .values()
                            .filter_map(|key| match key {
                                RefSynthKey::Contract { src, tgt, .. }
                                | RefSynthKey::ContractExpand { src, tgt, .. } => {
                                    Some((*src, *tgt))
                                }
                                RefSynthKey::Expand { .. } => None,
                            })
                            .collect();
                        assert_eq!(synthesized, walked, "realized pair census for {graph:?}");
                        successful += 1;
                    }
                    (Err(reference), Err(production)) => assert_eq!(
                        production_error(&production),
                        reference_error(&reference),
                        "refusal for {graph:?} under {migration:?}"
                    ),
                    (reference, production) => panic!(
                        "engines disagree for {graph:?} under {migration:?}: {reference:?} / {production:?}"
                    ),
                }
                compared += 1;
            }
        }
    }
    assert_eq!(compared, 12_366);
    assert!(successful > 0);
}

struct DirectFateFixture {
    source: RefSchema,
    tree: RefTree,
    graph: RefGraph,
    production_tree: WInstance,
}

fn direct_fate_fixture() -> DirectFateFixture {
    let edge = RefEdge::new("a", "b", "prop", Some("child"));
    let source = RefSchema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .edge("a", "b", "prop", Some("child"));
    let tree = RefTree {
        nodes: BTreeMap::from([(0, RefNode::new(0, "a")), (1, RefNode::new(1, "b"))]),
        arcs: vec![(0, 1, edge.clone())],
        fans: Vec::new(),
        root: 0,
    };
    let graph = RefGraph {
        nodes: tree.nodes.clone(),
        edges: BTreeMap::from([(
            0,
            RefGEdge {
                src: 0,
                tgt: 1,
                label: edge.clone(),
                position: None,
            },
        )]),
    };
    let production_tree = WInstance::new(
        tree.nodes
            .values()
            .map(|node| (node.id, prod_node(node)))
            .collect(),
        vec![(0, 1, prod_edge(&edge))],
        Vec::new(),
        0,
        "a".into(),
    );
    DirectFateFixture {
        source,
        tree,
        graph,
        production_tree,
    }
}

fn drop_fate_runs_through_graph_and_tree_engines(fixture: &DirectFateFixture) {
    let drop_target = RefSchema::default().vertex("a", "record");
    let drop_migration = RefMigration {
        surviving_verts: BTreeSet::from(["a".to_owned()]),
        ..RefMigration::default()
    };
    let drop_reference_plan =
        restrict::compile(&fixture.source, &drop_target, &drop_migration).unwrap();
    let drop_production_plan = RestrictPlan::compile(
        &prod_schema(&fixture.source),
        &prod_schema(&drop_target),
        &prod_migration(&drop_migration),
    )
    .unwrap();
    let semantics = BTreeMap::new();
    let drop_lens = RefGraphLens::new(
        &fixture.source,
        &drop_target,
        &drop_migration,
        &drop_reference_plan,
        &semantics,
        &NoValueLayer,
    )
    .unwrap();
    let (expected_tree, _, _) = reference_tree_get(&drop_lens, &fixture.tree)
        .expect("valid source tree")
        .expect("drop case restricts");
    let (actual_tree, _) =
        wtype_restrict_with_plan(&fixture.production_tree, &drop_production_plan)
            .expect("production drop case restricts");
    assert_eq!(ref_tree(&actual_tree), expected_tree);
    let (expected_graph, _) = restrict::get(
        &fixture.source,
        &drop_target,
        &drop_migration,
        &NoValueLayer,
        &fixture.graph,
    )
    .expect("reference graph drop");
    let (actual_graph, _) =
        graph_restrict_with_plan(&prod_graph(&fixture.graph), &drop_production_plan)
            .expect("production graph drop");
    assert_eq!(ref_graph(&actual_graph), expected_graph);
}

fn unresolved_fate_runs_through_graph_and_tree_engines(fixture: &DirectFateFixture) {
    let unresolved_target = RefSchema::default()
        .vertex("a", "record")
        .vertex("b", "record");
    let unresolved_migration = RefMigration {
        surviving_verts: BTreeSet::from(["a".to_owned(), "b".to_owned()]),
        expansion_path: BTreeMap::from([(("a".to_owned(), "b".to_owned()), Vec::new())]),
        ..RefMigration::default()
    };
    let unresolved_reference_plan =
        restrict::compile(&fixture.source, &unresolved_target, &unresolved_migration).unwrap();
    let unresolved_production_plan = RestrictPlan::compile(
        &prod_schema(&fixture.source),
        &prod_schema(&unresolved_target),
        &prod_migration(&unresolved_migration),
    )
    .unwrap();
    let semantics = BTreeMap::new();
    let unresolved_lens = RefGraphLens::new(
        &fixture.source,
        &unresolved_target,
        &unresolved_migration,
        &unresolved_reference_plan,
        &semantics,
        &NoValueLayer,
    )
    .unwrap();
    let expected_tree_error = reference_tree_get(&unresolved_lens, &fixture.tree)
        .expect("valid source tree")
        .expect_err("unresolved tree edge refuses");
    let actual_tree_error =
        wtype_restrict_with_plan(&fixture.production_tree, &unresolved_production_plan)
            .expect_err("production unresolved tree edge refuses");
    assert_eq!(
        production_error(&actual_tree_error),
        reference_unlocated_error(&expected_tree_error)
    );
    let expected_graph_error = restrict::get(
        &fixture.source,
        &unresolved_target,
        &unresolved_migration,
        &NoValueLayer,
        &fixture.graph,
    )
    .expect_err("unresolved graph edge refuses");
    let actual_graph_error =
        graph_restrict_with_plan(&prod_graph(&fixture.graph), &unresolved_production_plan)
            .expect_err("production unresolved graph edge refuses");
    assert_eq!(
        production_error(&actual_graph_error),
        reference_error(&expected_graph_error)
    );
}

fn drop_and_unresolved_fates_run_through_graph_and_tree_engines() {
    let fixture = direct_fate_fixture();
    drop_fate_runs_through_graph_and_tree_engines(&fixture);
    unresolved_fate_runs_through_graph_and_tree_engines(&fixture);
}

fn assert_child_binding_word(word: &[ChildBindingSlot]) {
    let mut reference = RefTree {
        nodes: BTreeMap::from([(0, RefNode::new(0, "p"))]),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 0,
    };
    for (index, slot) in word.iter().enumerate() {
        let (column, value, self_loop) = match slot {
            ChildBindingSlot::SelfX => ("x", None, true),
            ChildBindingSlot::SelfY => ("y", None, true),
            ChildBindingSlot::MissingX => ("x", None, false),
            ChildBindingSlot::ZeroX => ("x", Some(0), false),
            ChildBindingSlot::OneX => ("x", Some(1), false),
            ChildBindingSlot::MissingY => ("y", None, false),
            ChildBindingSlot::ZeroY => ("y", Some(0), false),
            ChildBindingSlot::OneY => ("y", Some(1), false),
        };
        let child = if self_loop {
            0
        } else {
            u32::try_from(index + 1).unwrap()
        };
        if !self_loop {
            let node = value.map_or_else(
                || RefNode::new(child, "v"),
                |value| RefNode::new(child, "v").with_value(RefValue::Int(value)),
            );
            reference.nodes.insert(child, node);
        }
        reference
            .arcs
            .push((0, child, RefEdge::new("p", "v", "prop", Some(column))));
    }
    let production = WInstance::new(
        reference
            .nodes
            .iter()
            .map(|(id, node)| {
                let mut production = Node::new(*id, node.anchor.as_str());
                if let Some(RefPresence::Present(RefValue::Int(value))) = &node.value {
                    production.value = Some(FieldPresence::Present(Value::Int(*value)));
                }
                (*id, production)
            })
            .collect(),
        reference
            .arcs
            .iter()
            .map(|(parent, child, edge)| (*parent, *child, prod_edge(edge)))
            .collect(),
        Vec::new(),
        0,
        "p".into(),
    );
    let expected = restrict::scalar_child_bindings(&reference, 0);
    let actual: BTreeMap<String, RefValue> = collect_scalar_child_values(&production, 0)
        .into_iter()
        .map(|(column, value)| {
            let value = match value {
                Value::Int(value) => RefValue::Int(value),
                Value::List(values) => RefValue::List(
                    values
                        .into_iter()
                        .map(|value| match value {
                            Value::Int(value) => RefValue::Int(value),
                            other => panic!("unexpected bounded scalar {other:?}"),
                        })
                        .collect(),
                ),
                other => panic!("unexpected bounded binding {other:?}"),
            };
            (column, value)
        })
        .collect();
    assert_eq!(actual, expected, "{word:?}");
}

struct RepeatedChildInstances {
    tree: WInstance,
    graph: GInstance,
    table: FInstance,
    table_graph: GInstance,
}

fn repeated_child_instances() -> RepeatedChildInstances {
    let repeated_edge = Edge {
        src: "r".into(),
        tgt: "v".into(),
        kind: "prop".into(),
        name: Some("x".into()),
    };
    let repeated_tree = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "r")),
            (
                1,
                Node::new(1, "v").with_value(FieldPresence::Present(Value::Int(1))),
            ),
            (
                2,
                Node::new(2, "v").with_value(FieldPresence::Present(Value::Int(2))),
            ),
        ]),
        vec![(0, 1, repeated_edge.clone()), (0, 2, repeated_edge.clone())],
        Vec::new(),
        0,
        "r".into(),
    );
    let graph = GInstance::from_wtype(&repeated_tree).unwrap();
    let table = FInstance::new()
        .with_rows("r", [(RowId(0), HashMap::new())])
        .and_then(|table| {
            table.with_rows(
                "v",
                [(RowId(1), HashMap::new()), (RowId(2), HashMap::new())],
            )
        })
        .and_then(|table| {
            table.with_foreign_key(
                repeated_edge.clone(),
                [(RowId(0), RowId(1)), (RowId(0), RowId(2))],
            )
        })
        .unwrap();
    let (table_graph, _) = GInstance::from_finstance(&table).unwrap();
    RepeatedChildInstances {
        tree: repeated_tree,
        graph,
        table,
        table_graph,
    }
}

fn assert_repeated_child_attributes(instances: &RepeatedChildInstances) {
    let scalar_word = Value::List(vec![Value::Int(1), Value::Int(2)]);
    assert_eq!(
        collect_scalar_child_values(&instances.tree, 0).get("x"),
        Some(&scalar_word)
    );
    assert_eq!(instances.graph.attributes(0).get("x"), Some(&scalar_word));
    let structural_word = Value::List(Vec::new());
    assert_eq!(
        instances.table.attributes(0).get("x"),
        Some(&structural_word)
    );
    assert_eq!(
        instances.table_graph.attributes(0).get("x"),
        Some(&structural_word)
    );
}

fn assert_repeated_child_ambiguity(instances: &RepeatedChildInstances) {
    let protocol = Protocol {
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    };
    let schema = SchemaBuilder::new(&protocol)
        .vertex("r", "record", None)
        .and_then(|builder| builder.vertex("v", "record", None))
        .and_then(|builder| builder.edge("r", "v", "prop", Some("x")))
        .and_then(SchemaBuilder::build)
        .unwrap();
    let mut migration = CompiledMigration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.keys().cloned().collect(),
        ..CompiledMigration::default()
    };
    migration.add_computed_field(
        "r",
        "n",
        Expr::Builtin(BuiltinOp::Length, vec![Expr::Var("x".into())]),
    );
    let graph_plan = RestrictPlan::compile(&schema, &schema, &migration).unwrap();
    let ambiguous = |error: &RestrictError| {
        matches!(
            error.unlocated(),
            RestrictError::AmbiguousChildField { node: 0, field } if field == "x"
        )
    };
    let error = wtype_restrict_with_plan(&instances.tree, &graph_plan).unwrap_err();
    assert!(ambiguous(&error), "{error:?}");
    let error = graph_restrict_with_plan(&instances.graph, &graph_plan).unwrap_err();
    assert!(ambiguous(&error), "{error:?}");
    let table_plan = RestrictPlan::compile_for_tables(
        &schema,
        &schema,
        &migration,
        BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        },
    )
    .unwrap();
    let error = functor_restrict_with_plan(&instances.table, &table_plan).unwrap_err();
    assert!(ambiguous(&error), "{error:?}");
    let error = graph_restrict_with_plan(&instances.table_graph, &graph_plan).unwrap_err();
    assert!(ambiguous(&error), "{error:?}");
}

#[test]
fn repeated_child_bindings_agree_and_preserve_order_and_duplicates() {
    let cases = child_binding_cases();
    for word in &cases {
        assert_child_binding_word(word);
    }
    assert_eq!(cases.len(), 585);

    let instances = repeated_child_instances();
    assert_repeated_child_attributes(&instances);
    assert_repeated_child_ambiguity(&instances);
}

fn table_protocol() -> Protocol {
    Protocol {
        value_kinds: BTreeMap::from([(
            "int".to_owned(),
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        )]),
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    }
}

fn ref_column_fate_text(fate: &RefColumnFate) -> String {
    match fate {
        RefColumnFate::Drop(column) => format!("drop:{column}"),
        RefColumnFate::Rename(source, target) => format!("rename:{source}:{target}"),
    }
}

fn prod_column_fate_text(fate: &CellFate) -> String {
    match fate {
        CellFate::Drop(column) => format!("drop:{column}"),
        CellFate::Rename(source, target) => format!("rename:{source}:{target}"),
    }
}

fn column_case_target(
    protocol: &Protocol,
    states: &[ColumnState],
) -> (
    Schema,
    RefSchema,
    CompiledMigration,
    RefMigration,
    BTreeMap<RefEdge, String>,
) {
    let mut target_builder = SchemaBuilder::new(protocol)
        .vertex("t", "record", None)
        .unwrap();
    let mut reference_target = RefSchema::default().vertex("t", "record");
    let mut migration = CompiledMigration::default();
    migration.surviving_verts.insert("t".into());
    let mut reference_migration = RefMigration {
        surviving_verts: BTreeSet::from(["t".to_owned()]),
        ..RefMigration::default()
    };
    let mut target_columns = BTreeMap::new();

    for (index, state) in states.iter().enumerate() {
        let source_name = if index == 0 { "a" } else { "b" };
        let source_vertex = format!("t.{source_name}");
        let source_edge = RefEdge::new("t", &source_vertex, "prop", Some(source_name));
        if *state == ColumnState::Drop {
            continue;
        }
        let target_name = if *state == ColumnState::Keep {
            source_name.to_owned()
        } else {
            format!("{source_name}_new")
        };
        let target_vertex = format!("t.{target_name}");
        let target_edge = RefEdge::new("t", &target_vertex, "prop", Some(&target_name));
        target_builder = target_builder
            .vertex(&target_vertex, "int", None)
            .unwrap()
            .edge("t", &target_vertex, "prop", Some(&target_name))
            .unwrap();
        reference_target = reference_target.vertex(&target_vertex, "int").edge(
            "t",
            &target_vertex,
            "prop",
            Some(&target_name),
        );
        migration
            .surviving_verts
            .insert(target_vertex.as_str().into());
        reference_migration
            .surviving_verts
            .insert(target_vertex.clone());
        target_columns.insert(target_edge.clone(), target_name.clone());
        if *state == ColumnState::Rename {
            migration
                .vertex_remap
                .insert(source_vertex.as_str().into(), target_vertex.as_str().into());
            migration
                .edge_remap
                .insert(prod_edge(&source_edge), prod_edge(&target_edge));
            reference_migration
                .vertex_remap
                .insert(source_vertex, target_vertex);
            reference_migration
                .edge_remap
                .insert(source_edge, target_edge);
        } else {
            migration.surviving_edges.insert(prod_edge(&source_edge));
            reference_migration.surviving_edges.insert(source_edge);
        }
    }

    (
        target_builder.build().unwrap(),
        reference_target,
        migration,
        reference_migration,
        target_columns,
    )
}

fn assert_column_runtime(
    protocol: &Protocol,
    source: &Schema,
    target: &Schema,
    migration: &CompiledMigration,
    production: &RestrictPlan,
    states: [ColumnState; 2],
) {
    let source_cells = HashMap::from([
        ("a".to_owned(), Value::Int(1)),
        ("b".to_owned(), Value::Int(2)),
    ]);
    let expected_cells: HashMap<String, Value> = states
        .iter()
        .enumerate()
        .filter_map(|(index, state)| {
            if *state == ColumnState::Drop {
                return None;
            }
            let source = if index == 0 { "a" } else { "b" };
            let target = if *state == ColumnState::Rename {
                format!("{source}_new")
            } else {
                source.to_owned()
            };
            Some((target, Value::Int(i64::try_from(index + 1).unwrap())))
        })
        .collect();
    let table = FInstance::new()
        .with_rows("t", [(RowId(0), source_cells)])
        .unwrap();
    let table_plan = RestrictPlan::compile_for_tables(
        source,
        target,
        migration,
        BoundaryProtocols {
            src: protocol,
            tgt: protocol,
        },
    )
    .unwrap();
    let (table_view, _) = functor_restrict_with_plan(&table, &table_plan).unwrap();
    assert_eq!(
        table_view.row(RowId(0)),
        Some(&expected_cells),
        "{states:?}"
    );

    let (table_graph, _) = GInstance::from_finstance(&table).unwrap();
    let (graph_view, _) = graph_restrict_with_plan(&table_graph, production).unwrap();
    assert_eq!(
        graph_view.node(0).map(|node| &node.extra_fields),
        Some(&expected_cells),
        "{states:?}"
    );

    let source_edge = |target: &str, name: &str| Edge {
        src: "t".into(),
        tgt: target.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    };
    let tree = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "t")),
            (
                1,
                Node::new(1, "t.a").with_value(FieldPresence::Present(Value::Int(1))),
            ),
            (
                2,
                Node::new(2, "t.b").with_value(FieldPresence::Present(Value::Int(2))),
            ),
        ]),
        vec![
            (0, 1, source_edge("t.a", "a")),
            (0, 2, source_edge("t.b", "b")),
        ],
        Vec::new(),
        0,
        "t".into(),
    );
    let (tree_view, _) = wtype_restrict_with_plan(&tree, production).unwrap();
    let actual_tree_cells = collect_scalar_child_values(&tree_view, 0);
    assert_eq!(actual_tree_cells, expected_cells, "{states:?}");
}

#[test]
fn column_fates_are_derived_from_the_same_schema_plan() {
    let protocol = table_protocol();
    let source = SchemaBuilder::new(&protocol)
        .vertex("t", "record", None)
        .and_then(|builder| builder.vertex("t.a", "int", None))
        .and_then(|builder| builder.vertex("t.b", "int", None))
        .and_then(|builder| builder.edge("t", "t.a", "prop", Some("a")))
        .and_then(|builder| builder.edge("t", "t.b", "prop", Some("b")))
        .and_then(SchemaBuilder::build)
        .unwrap();
    let reference_source = RefSchema::default()
        .vertex("t", "record")
        .vertex("t.a", "int")
        .vertex("t.b", "int")
        .edge("t", "t.a", "prop", Some("a"))
        .edge("t", "t.b", "prop", Some("b"));
    let source_columns = BTreeMap::from([
        (RefEdge::new("t", "t.a", "prop", Some("a")), "a".to_owned()),
        (RefEdge::new("t", "t.b", "prop", Some("b")), "b".to_owned()),
    ]);
    let cases = column_fate_cases();
    for states in &cases {
        let (target, reference_target, migration, reference_migration, target_columns) =
            column_case_target(&protocol, states);
        let production = RestrictPlan::compile_with_protocols(
            &source,
            &target,
            &migration,
            BoundaryProtocols {
                src: &protocol,
                tgt: &protocol,
            },
        )
        .unwrap();
        let reference_plan =
            restrict::compile(&reference_source, &reference_target, &reference_migration).unwrap();
        let expected = restrict::column_fates(
            &reference_plan,
            &reference_migration,
            &source_columns,
            &target_columns,
        );
        let expected = expected
            .get("t")
            .into_iter()
            .flatten()
            .map(ref_column_fate_text)
            .collect::<Vec<_>>();
        let actual = production
            .boundary
            .as_ref()
            .and_then(|boundary| boundary.cells.as_ref())
            .and_then(|cells| cells.fates.get("t"))
            .into_iter()
            .flatten()
            .map(prod_column_fate_text)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{states:?}");
        assert_column_runtime(
            &protocol,
            &source,
            &target,
            &migration,
            &production,
            *states,
        );
    }
    assert_eq!(cases.len(), 9);
}

fn reference_tree_for_case(case: &TreeRestrictCase) -> RefTree {
    let root = match case.floor {
        TreeFloorCase::Gapped => 2,
        _ => 0,
    };
    let mut ids = match case.floor {
        TreeFloorCase::Compact => (1..=case.routes.len())
            .map(|id| u32::try_from(id).unwrap())
            .collect::<Vec<_>>(),
        TreeFloorCase::Gapped => (0..case.routes.len())
            .map(|index| 10 + u32::try_from(index).unwrap() * 3)
            .collect(),
        TreeFloorCase::LastFree | TreeFloorCase::Saturated | TreeFloorCase::AlreadyMax => (1
            ..=case.routes.len())
            .map(|id| u32::try_from(id).unwrap())
            .collect(),
    };
    match case.floor {
        TreeFloorCase::LastFree => *ids.last_mut().unwrap() = u32::MAX - 2,
        TreeFloorCase::Saturated => *ids.last_mut().unwrap() = u32::MAX - 1,
        TreeFloorCase::AlreadyMax => *ids.last_mut().unwrap() = u32::MAX,
        TreeFloorCase::Compact | TreeFloorCase::Gapped => {}
    }
    let mut tree = RefTree {
        nodes: BTreeMap::from([(root, RefNode::new(root, "a"))]),
        arcs: Vec::new(),
        fans: Vec::new(),
        root,
    };
    for (&id, route) in ids.iter().zip(&case.routes) {
        let (anchor, edge) = match route {
            TreeRoute::Keep => ("d", RefEdge::new("a", "d", "prop", Some("keep"))),
            TreeRoute::Expand => ("c", RefEdge::new("a", "c", "prop", Some("expand"))),
        };
        tree.nodes.insert(id, RefNode::new(id, anchor));
        tree.arcs.push((root, id, edge));
    }
    tree
}

fn tree_case_is_success(
    case: &TreeRestrictCase,
    reference_lens: &RefGraphLens<'_>,
    production_plan: &RestrictPlan,
) -> bool {
    let reference = reference_tree_for_case(case);
    let production = WInstance::new(
        reference
            .nodes
            .values()
            .map(|node| (node.id, prod_node(node)))
            .collect(),
        reference
            .arcs
            .iter()
            .map(|(parent, child, edge)| (*parent, *child, prod_edge(edge)))
            .collect(),
        Vec::new(),
        reference.root,
        "a".into(),
    );
    let expected = reference_tree_get(reference_lens, &reference)
        .expect("the bounded source stays in the tree-get shape domain");
    let actual = wtype_restrict_with_plan(&production, production_plan);
    match (expected, actual) {
        (Ok((view, complement, _)), Ok((actual_view, synthesis))) => {
            assert_eq!(ref_tree(&actual_view), view, "{case:?}");
            assert_eq!(synthesis.id_floor, complement.floor.0, "{case:?}");
            assert_eq!(
                synthesis
                    .synthesized_keys
                    .iter()
                    .map(|(id, key)| (*id, ref_synth_key(key)))
                    .collect::<BTreeMap<_, _>>(),
                complement.synthesized_nodes,
                "{case:?}"
            );
            true
        }
        (
            Err(RefRestrictError::At {
                error: RefEngineError::IdsExhausted { nodes: true },
                ..
            }),
            Err(RestrictError::IdsExhausted {
                kind: panproto_inst::segment::IdKind::Node,
            }),
        ) => false,
        (expected, actual) => {
            panic!("tree engines disagree for {case:?}: {expected:?} / {actual:?}")
        }
    }
}

fn assert_keep_before_synthesis(reference_lens: &RefGraphLens<'_>, production_plan: &RestrictPlan) {
    let order_witness = RefTree {
        nodes: BTreeMap::from([
            (0, RefNode::new(0, "a")),
            (1, RefNode::new(1, "c")),
            (2, RefNode::new(2, "d")),
        ]),
        arcs: vec![
            (0, 1, RefEdge::new("a", "c", "prop", Some("expand"))),
            (0, 2, RefEdge::new("a", "d", "prop", Some("keep"))),
        ],
        fans: Vec::new(),
        root: 0,
    };
    let production_witness = WInstance::new(
        order_witness
            .nodes
            .values()
            .map(|node| (node.id, prod_node(node)))
            .collect(),
        order_witness
            .arcs
            .iter()
            .map(|(parent, child, edge)| (*parent, *child, prod_edge(edge)))
            .collect(),
        Vec::new(),
        0,
        "a".into(),
    );
    let (expected_order, _, _) = reference_tree_get(reference_lens, &order_witness)
        .expect("valid order witness")
        .expect("reference order witness restricts");
    let (actual_order, _) = wtype_restrict_with_plan(&production_witness, production_plan).unwrap();
    assert_eq!(ref_tree(&actual_order), expected_order);
    assert_eq!(
        actual_order
            .arcs
            .iter()
            .filter(|(parent, _, _)| *parent == 0)
            .map(|(_, child, _)| *child)
            .collect::<Vec<_>>(),
        vec![2, 3],
        "kept siblings precede synthesized siblings, which follow SynthKey order"
    );
}

#[test]
fn tree_get_uses_the_graph_plan_keys_floor_and_sibling_order() {
    let source = RefSchema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("c", "record")
        .vertex("d", "record")
        .edge("a", "d", "prop", Some("keep"))
        .edge("a", "c", "prop", Some("expand"));
    let target = RefSchema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("c", "record")
        .vertex("d", "record")
        .edge("a", "d", "prop", Some("keep"))
        .edge("a", "b", "prop", Some("ab"))
        .edge("b", "c", "prop", Some("bc"));
    let mut migration = RefMigration {
        surviving_verts: ["a", "c", "d"].into_iter().map(str::to_owned).collect(),
        ..RefMigration::default()
    };
    migration
        .expansion_path
        .insert(("a".to_owned(), "c".to_owned()), vec!["b".to_owned()]);
    let production_schema = prod_schema(&source);
    let production_target = prod_schema(&target);
    let production_migration = prod_migration(&migration);
    let production_plan = RestrictPlan::compile(
        &production_schema,
        &production_target,
        &production_migration,
    )
    .unwrap();
    let reference_plan = restrict::compile(&source, &target, &migration).unwrap();
    let semantics = BTreeMap::new();
    let reference_lens = RefGraphLens::new(
        &source,
        &target,
        &migration,
        &reference_plan,
        &semantics,
        &NoValueLayer,
    )
    .unwrap();
    let cases = tree_restrict_cases();
    let mut successful = 0usize;
    let mut exhausted = 0usize;
    for case in &cases {
        if tree_case_is_success(case, &reference_lens, &production_plan) {
            successful += 1;
        } else {
            exhausted += 1;
        }
    }
    assert_eq!(cases.len(), 70);
    assert_eq!(successful, 43);
    assert_eq!(exhausted, 27);
    assert_keep_before_synthesis(&reference_lens, &production_plan);
    drop_and_unresolved_fates_run_through_graph_and_tree_engines();
}

#[test]
fn reference_restore_vocabulary_covers_tree_put_domain_refusals() {
    assert_ne!(
        std::mem::discriminant(&restrict::PutFieldRefusal::TransformNotInvertible {
            field: "x".to_owned(),
        }),
        std::mem::discriminant(&restrict::PutFieldRefusal::DerivedCellEdited {
            field: "x".to_owned(),
        })
    );
    assert!(matches!(
        restrict::PredicateRefusal::AmbiguousChildField {
            field: "x".to_owned()
        },
        restrict::PredicateRefusal::AmbiguousChildField { .. }
    ));
}
