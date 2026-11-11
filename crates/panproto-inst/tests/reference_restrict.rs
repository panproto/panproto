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
    ElementOps, FInstance, Fan, FieldPresence, GEdge, GInstance, GraphComplement, IdFloor, Node,
    NodeShape, PairResolution, RestrictError, RestrictPlan, RowId, SynthKey, UnresolvedReason,
    Value, WInstance, collect_scalar_child_values, functor_restrict_with_plan,
    graph_restrict_with_plan, resolve_pair, wtype_restrict_with_plan,
};
use panproto_reference::enumerate::restrict_and_contraction::{
    ChildBindingSlot, ColumnState, ExpansionState, PairResolutionCase, PairState, TreeFloorCase,
    TreeRestrictCase, TreeRoute, child_binding_cases, column_fate_cases, pair_resolution_cases,
    release_graphs, release_schemas, survival_classifications, tree_restrict_cases,
};
use panproto_reference::lens::{GraphLens as RefGraphLens, tree_get as reference_tree_get};
use panproto_reference::migration::Migration as RefMigration;
use panproto_reference::model::{
    Fan as RefFan, GEdge as RefGEdge, Graph as RefGraph, Node as RefNode,
    NodeShape as RefNodeShape, SEdge as RefEdge, Tree as RefTree,
};
use panproto_reference::restrict::{
    self, ColumnFate as RefColumnFate, Complement as RefComplement, EdgeFate as RefEdgeFate,
    EngineError as RefEngineError, NoValueLayer, PairResolution as RefPairResolution,
    PlanError as RefPlanError, RestrictError as RefRestrictError, SynthKey as RefSynthKey,
    UnresolvedReason as RefUnresolvedReason,
};
use panproto_reference::schema::{HyperEdge as RefHyperEdge, Schema as RefSchema};
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
    for (id, hyper) in &schema.hyper_edges {
        builder = builder
            .hyper_edge(
                id,
                &hyper.kind,
                hyper
                    .signature
                    .iter()
                    .map(|(label, vertex)| (label.clone(), vertex.clone()))
                    .collect(),
                &hyper.parent_label,
            )
            .expect("reference hyper-edge");
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

/// The production value of a reference value. The restrict scopes hold
/// booleans, integers, float bit patterns, strings, byte strings, tokens,
/// links, explicit nulls, records, lists and labelled nulls; any other value
/// fails the test.
fn prod_value(value: &RefValue) -> Value {
    match value {
        RefValue::Bool(b) => Value::Bool(*b),
        RefValue::Int(i) => Value::Int(*i),
        RefValue::Float(bits) => Value::Float(f64::from_bits(*bits)),
        RefValue::Str(s) => Value::Str(s.clone()),
        RefValue::Bytes(b) => Value::Bytes(b.clone()),
        RefValue::CidLink(s) => Value::CidLink(s.clone()),
        RefValue::Token(s) => Value::Token(s.clone()),
        RefValue::Null => Value::Null,
        RefValue::Unknown(fields) => Value::Unknown(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), prod_value(v)))
                .collect(),
        ),
        RefValue::List(items) => Value::List(items.iter().map(prod_value).collect()),
        RefValue::LabeledNull(id) => Value::LabeledNull(*id),
        other => panic!("the restrict scopes hold no value {other:?}"),
    }
}

/// The reference value of a production value: the left inverse of
/// [`prod_value`] on its image.
fn ref_value(value: &Value) -> RefValue {
    match value {
        Value::Bool(b) => RefValue::Bool(*b),
        Value::Int(i) => RefValue::Int(*i),
        Value::Float(f) => RefValue::Float(f.to_bits()),
        Value::Str(s) => RefValue::Str(s.clone()),
        Value::Bytes(b) => RefValue::Bytes(b.clone()),
        Value::CidLink(s) => RefValue::CidLink(s.clone()),
        Value::Token(s) => RefValue::Token(s.clone()),
        Value::Null => RefValue::Null,
        Value::Unknown(fields) => RefValue::Unknown(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), ref_value(v)))
                .collect(),
        ),
        Value::List(items) => RefValue::List(items.iter().map(ref_value).collect()),
        Value::LabeledNull(id) => RefValue::LabeledNull(*id),
        other => panic!("the restrict scopes hold no value {other:?}"),
    }
}

fn prod_presence(presence: &RefPresence) -> FieldPresence {
    match presence {
        RefPresence::Present(value) => FieldPresence::Present(prod_value(value)),
        RefPresence::Null => FieldPresence::Null,
        RefPresence::Absent => FieldPresence::Absent,
    }
}

fn ref_presence(presence: &FieldPresence) -> RefPresence {
    match presence {
        FieldPresence::Present(value) => RefPresence::Present(ref_value(value)),
        FieldPresence::Null => RefPresence::Null,
        FieldPresence::Absent => RefPresence::Absent,
    }
}

fn ref_fields<'a>(
    fields: impl IntoIterator<Item = (&'a String, &'a Value)>,
) -> BTreeMap<String, RefValue> {
    fields
        .into_iter()
        .map(|(key, value)| (key.clone(), ref_value(value)))
        .collect()
}

const fn ref_shape(shape: &NodeShape) -> RefNodeShape {
    match shape {
        NodeShape::Plain => RefNodeShape::Plain,
        NodeShape::List => RefNodeShape::List,
        NodeShape::HyperEdge => RefNodeShape::HyperEdge,
        NodeShape::XmlElement { .. } | NodeShape::XmlTextSegment => {
            panic!("the restrict scopes hold no XML node")
        }
    }
}

const fn prod_shape(shape: &RefNodeShape) -> NodeShape {
    match shape {
        RefNodeShape::Plain => NodeShape::Plain,
        RefNodeShape::List => NodeShape::List,
        RefNodeShape::HyperEdge => NodeShape::HyperEdge,
        RefNodeShape::XmlElement(_) | RefNodeShape::XmlTextSegment => {
            panic!("the restrict scopes hold no XML node")
        }
    }
}

/// The reference node of a production node: id, anchor, value,
/// discriminator, fields, position, shape and annotations.
fn ref_node(node: &Node) -> RefNode {
    let mut out = RefNode::new(node.id, &node.anchor);
    out.value = node.value.as_ref().map(ref_presence);
    out.discriminator = node.discriminator.as_ref().map(ToString::to_string);
    out.fields = ref_fields(&node.extra_fields);
    out.position = node.position;
    out.shape = ref_shape(&node.shape);
    out.annotations = ref_fields(&node.annotations);
    out
}

/// The production node of a reference node; the inverse of [`ref_node`].
fn prod_node(node: &RefNode) -> Node {
    let mut out = Node::new(node.id, node.anchor.as_str());
    out.value = node.value.as_ref().map(prod_presence);
    out.discriminator = node.discriminator.as_deref().map(Into::into);
    out.extra_fields = node
        .fields
        .iter()
        .map(|(key, value)| (key.clone(), prod_value(value)))
        .collect();
    out.position = node.position;
    out.shape = prod_shape(&node.shape);
    out.annotations = node
        .annotations
        .iter()
        .map(|(key, value)| (key.clone(), prod_value(value)))
        .collect();
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
        fans: tree
            .fans
            .iter()
            .map(|fan| RefFan {
                parent: fan.parent,
                hyper_edge: fan.hyper_edge_id.clone(),
                children: fan
                    .children
                    .iter()
                    .map(|(label, child)| (label.clone(), *child))
                    .collect(),
            })
            .collect(),
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
        original_fields: complement
            .original_extra_fields
            .iter()
            .map(|(id, fields)| (*id, ref_fields(fields)))
            .collect(),
        original_values: complement
            .original_values
            .iter()
            .map(|(id, value)| (*id, value.as_ref().map(ref_presence)))
            .collect(),
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

/// The graph with every node carrying a value, a discriminator, a field and an
/// annotation derived from its id, so that the view and complement
/// comparisons observe node payloads.
fn decorated(graph: &RefGraph) -> RefGraph {
    let mut out = graph.clone();
    for node in out.nodes.values_mut() {
        let id = i64::from(node.id);
        node.value = Some(RefPresence::Present(RefValue::Int(id)));
        node.discriminator = Some(format!("d{id}"));
        node.fields
            .insert("f".to_owned(), RefValue::Str(format!("field {id}")));
        node.annotations
            .insert("note".to_owned(), RefValue::Bool(id % 2 == 0));
    }
    out
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
            for graph in release_graphs(&source)
                .into_iter()
                .flat_map(|graph| [decorated(&graph), graph])
            {
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
                        for (id, node) in &reference_view.nodes {
                            if let Some(original) = graph.nodes.get(id) {
                                assert_eq!(
                                    (&node.value, &node.fields, &node.annotations),
                                    (&original.value, &original.fields, &original.annotations),
                                    "a kept node keeps its payload without a value program"
                                );
                            }
                        }
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
    assert_eq!(compared, 2 * 12_366, "each graph bare and decorated");
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

fn prod_tree(tree: &RefTree) -> WInstance {
    let schema_root = tree.nodes[&tree.root].anchor.as_str();
    WInstance::new(
        tree.nodes
            .values()
            .map(|node| (node.id, prod_node(node)))
            .collect(),
        tree.arcs
            .iter()
            .map(|(parent, child, edge)| (*parent, *child, prod_edge(edge)))
            .collect(),
        tree.fans
            .iter()
            .map(|fan| Fan {
                hyper_edge_id: fan.hyper_edge.clone(),
                parent: fan.parent,
                children: fan.children.clone().into_iter().collect(),
            })
            .collect(),
        tree.root,
        schema_root.into(),
    )
}

/// The component of a graph view that hangs from `root`, read as a tree.
fn root_component(view: &GInstance, schema: &Schema, root: u32) -> RefTree {
    let trees = view.to_wtypes(schema).expect("the view is a forest");
    let tree = trees
        .iter()
        .find(|tree| tree.root == root)
        .expect("the root survives");
    ref_tree(tree)
}

/// The source trees of the Drop-between-survivors carrier over `a -> b -> c`.
fn chain_trees() -> Vec<RefTree> {
    let ab = RefEdge::new("a", "b", "prop", Some("ab"));
    let bc = RefEdge::new("b", "c", "prop", Some("bc"));
    let tree = |nodes: &[(u32, &str)], arcs: &[(u32, u32, &RefEdge)]| RefTree {
        nodes: nodes
            .iter()
            .map(|(id, anchor)| (*id, RefNode::new(*id, anchor)))
            .collect(),
        arcs: arcs
            .iter()
            .map(|(parent, child, edge)| (*parent, *child, (*edge).clone()))
            .collect(),
        fans: Vec::new(),
        root: 0,
    };
    vec![
        tree(&[(0, "a")], &[]),
        tree(&[(0, "a"), (1, "b")], &[(0, 1, &ab)]),
        tree(&[(0, "a"), (1, "b"), (2, "c")], &[(0, 1, &ab), (1, 2, &bc)]),
        tree(
            &[(0, "a"), (1, "b"), (2, "b"), (3, "c"), (4, "c")],
            &[(0, 1, &ab), (0, 2, &ab), (1, 3, &bc), (2, 4, &bc)],
        ),
    ]
}

/// Tree and graph get on every Keep/Drop assignment to the two arcs of
/// `a -> b -> c`, all three vertices surviving.
///
/// With no `Drop` between survivors the plan preserves tree shape and the tree
/// view is the graph view read as a tree. With one, the tree engine drops the
/// arc's subtree while the graph engine drops the edge alone: the tree view is
/// then exactly the root component of the graph view, and the reference tree
/// lens is undefined. Returns the number of shape-breaking observations.
fn drop_between_survivors_relates_tree_and_graph_views() -> usize {
    let source = RefSchema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("c", "record")
        .edge("a", "b", "prop", Some("ab"))
        .edge("b", "c", "prop", Some("bc"));
    let semantics = BTreeMap::new();
    let mut divergent = 0usize;
    let mut observations = 0usize;
    for keep_ab in [true, false] {
        for keep_bc in [true, false] {
            let mut target = RefSchema::default()
                .vertex("a", "record")
                .vertex("b", "record")
                .vertex("c", "record");
            if keep_ab {
                target = target.edge("a", "b", "prop", Some("ab"));
            }
            if keep_bc {
                target = target.edge("b", "c", "prop", Some("bc"));
            }
            let migration = RefMigration {
                surviving_verts: ["a", "b", "c"].into_iter().map(str::to_owned).collect(),
                ..RefMigration::default()
            };
            let reference_plan = restrict::compile(&source, &target, &migration).unwrap();
            let production_target = prod_schema(&target);
            let production_plan = RestrictPlan::compile(
                &prod_schema(&source),
                &production_target,
                &prod_migration(&migration),
            )
            .unwrap();
            let shape_preserving = keep_ab && keep_bc;
            assert_eq!(production_plan.preserves_tree_shape(), shape_preserving);
            let lens = RefGraphLens::new(
                &source,
                &target,
                &migration,
                &reference_plan,
                &semantics,
                &NoValueLayer,
            )
            .unwrap();
            for tree in chain_trees() {
                let source_graph = GInstance::from_wtype(&prod_tree(&tree)).unwrap();
                let (expected_graph, _) = restrict::get(
                    &source,
                    &target,
                    &migration,
                    &NoValueLayer,
                    &ref_graph(&source_graph),
                )
                .expect("reference graph get");
                let (graph_view, _) =
                    graph_restrict_with_plan(&source_graph, &production_plan).unwrap();
                assert_eq!(ref_graph(&graph_view), expected_graph, "{tree:?}");
                let (tree_view, _) =
                    wtype_restrict_with_plan(&prod_tree(&tree), &production_plan).unwrap();
                assert_eq!(
                    ref_tree(&tree_view),
                    root_component(&graph_view, &production_target, tree.root),
                    "tree view is the root component of the graph view: {tree:?}, keep ab {keep_ab}, keep bc {keep_bc}"
                );
                let reference_tree = reference_tree_get(&lens, &tree);
                if shape_preserving {
                    let (expected_tree, _, _) = reference_tree
                        .expect("shape-preserving plan")
                        .expect("reference tree get");
                    assert_eq!(ref_tree(&tree_view), expected_tree, "{tree:?}");
                } else {
                    assert!(reference_tree.is_none(), "reference tree lens is undefined");
                    if graph_view.nodes().len() != tree_view.nodes.len() {
                        divergent += 1;
                    }
                }
                observations += 1;
            }
        }
    }
    assert_eq!(observations, 16);
    divergent
}

/// The witness: `a -> b` dropped between survivors, `b -> c` kept, on
/// the tree `0 -> 1 -> 2`. The tree engine removes arc `1 -> 2` with the
/// subtree of `1`; the graph engine keeps edge `1 -> 2`.
fn drop_between_survivors_witness() {
    let source = RefSchema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("c", "record")
        .edge("a", "b", "prop", Some("ab"))
        .edge("b", "c", "prop", Some("bc"));
    let target = RefSchema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("c", "record")
        .edge("b", "c", "prop", Some("bc"));
    let migration = RefMigration {
        surviving_verts: ["a", "b", "c"].into_iter().map(str::to_owned).collect(),
        ..RefMigration::default()
    };
    let plan = RestrictPlan::compile(
        &prod_schema(&source),
        &prod_schema(&target),
        &prod_migration(&migration),
    )
    .unwrap();
    assert!(!plan.preserves_tree_shape());
    let tree = chain_trees().swap_remove(2);
    let (tree_view, _) = wtype_restrict_with_plan(&prod_tree(&tree), &plan).unwrap();
    assert_eq!(
        tree_view.nodes.keys().copied().collect::<BTreeSet<_>>(),
        BTreeSet::from([0])
    );
    assert_eq!(tree_view.arcs, []);
    let source_graph = GInstance::from_wtype(&prod_tree(&tree)).unwrap();
    let (graph_view, _) = graph_restrict_with_plan(&source_graph, &plan).unwrap();
    assert_eq!(
        graph_view.nodes().keys().copied().collect::<BTreeSet<_>>(),
        BTreeSet::from([0, 1, 2])
    );
    assert_eq!(
        graph_view
            .edges()
            .values()
            .map(|edge| (edge.src, edge.tgt))
            .collect::<Vec<_>>(),
        vec![(1, 2)]
    );
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

fn shadowed_child_binding_tree(shadowed: u8, duplicate: bool) -> RefTree {
    let mut tree = RefTree {
        nodes: BTreeMap::from([(0, RefNode::new(0, "r"))]),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 0,
    };
    for (bit, column, first) in [(1, "x", 1), (2, "y", 3)] {
        if shadowed & bit != 0 {
            tree.nodes
                .get_mut(&0)
                .unwrap()
                .fields
                .insert(column.to_owned(), RefValue::Int(40 + i64::from(first)));
        }
        for slot in 0..2 {
            let id = first + slot;
            let value = if duplicate { first } else { id };
            tree.nodes.insert(
                id,
                RefNode::new(id, "v").with_value(RefValue::Int(i64::from(value))),
            );
            tree.arcs
                .push((0, id, RefEdge::new("r", "v", "prop", Some(column))));
        }
    }
    tree
}

#[test]
fn scalar_child_bindings_respect_independent_parent_field_shadows() {
    for shadowed in 0..4 {
        for duplicate in [false, true] {
            let reference = shadowed_child_binding_tree(shadowed, duplicate);
            let tree = prod_tree(&reference);
            let graph = GInstance::from_wtype(&tree).unwrap();
            let mut pinned = BTreeMap::new();
            for (bit, column, first) in [(1, "x", 1), (2, "y", 3)] {
                if shadowed & bit == 0 {
                    pinned.insert(
                        column.to_owned(),
                        RefValue::List(vec![
                            RefValue::Int(first),
                            RefValue::Int(if duplicate { first } else { first + 1 }),
                        ]),
                    );
                }
            }
            let expected = restrict::scalar_child_bindings(&reference, 0);
            assert_eq!(
                expected, pinned,
                "shadow mask {shadowed}, duplicate={duplicate}"
            );
            assert_eq!(
                ref_fields(&collect_scalar_child_values(&tree, 0)),
                expected,
                "tree child bindings: shadow mask {shadowed}, duplicate={duplicate}"
            );
            pinned.extend(reference.nodes[&0].fields.clone());
            assert_eq!(
                ref_fields(&tree.attributes(0)),
                pinned,
                "own tree fields win: shadow mask {shadowed}, duplicate={duplicate}"
            );
            assert_eq!(
                ref_fields(&graph.attributes(0)),
                pinned,
                "own graph fields win: shadow mask {shadowed}, duplicate={duplicate}"
            );
        }
    }
}

#[test]
fn graph_scalar_child_bindings_follow_positions_then_edge_ids() {
    for (positions, order) in [
        ([None, None], [0, 1]),
        ([Some(1), Some(0)], [1, 0]),
        ([Some(0), Some(0)], [0, 1]),
        ([Some(0), None], [1, 0]),
        ([None, Some(0)], [0, 1]),
        ([Some(u32::MAX), Some(0)], [1, 0]),
    ] {
        let mut instances = repeated_child_instances();
        for (id, position) in positions.into_iter().enumerate() {
            instances
                .graph
                .set_edge_position(EdgeId(u32::try_from(id).unwrap()), position)
                .unwrap();
        }
        let expected_ids = order.map(EdgeId).to_vec();
        assert_eq!(
            instances.graph.ordered_out_edges(0),
            expected_ids,
            "positions={positions:?}"
        );
        let mut reference = ref_tree(&instances.tree);
        let source_arcs = reference.arcs.clone();
        reference.arcs = order
            .map(|id| source_arcs[usize::try_from(id).unwrap()].clone())
            .to_vec();
        let expected = restrict::scalar_child_bindings(&reference, 0);
        assert_eq!(
            expected.get("x"),
            Some(&RefValue::List(
                order.map(|id| RefValue::Int(i64::from(id + 1))).to_vec()
            )),
            "reference arc order pins positional graph order: {positions:?}"
        );
        assert_eq!(
            ref_fields(&instances.graph.attributes(0)),
            expected,
            "graph child binding order: {positions:?}"
        );
    }
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
        value_kinds: BTreeMap::from([(
            "int".to_owned(),
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        )]),
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    };
    let schema = SchemaBuilder::new(&protocol)
        .vertex("r", "record", None)
        .and_then(|builder| builder.vertex("v", "record", None))
        .and_then(|builder| builder.edge("r", "v", "prop", Some("x")))
        .and_then(SchemaBuilder::build)
        .unwrap();
    // The target declares the computed attribute `n` the migration writes;
    // the source has no such column for its rows to lack.
    let target = SchemaBuilder::new(&protocol)
        .vertex("r", "record", None)
        .and_then(|builder| builder.vertex("v", "record", None))
        .and_then(|builder| builder.vertex("r.n", "int", None))
        .and_then(|builder| builder.edge("r", "v", "prop", Some("x")))
        .and_then(|builder| builder.edge("r", "r.n", "prop", Some("n")))
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
    let graph_plan = RestrictPlan::compile(&schema, &target, &migration).unwrap();
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
        &target,
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
    let production = prod_tree(&reference);
    let expected = reference_tree_get(reference_lens, &reference)
        .expect("the bounded source stays in the tree-get shape domain");
    let actual = wtype_restrict_with_plan(&production, production_plan);
    match (expected, actual) {
        (Ok((view, complement, _)), Ok((actual_view, synthesis))) => {
            assert_eq!(ref_tree(&actual_view), view, "{case:?}");
            assert_tree_view_is_graph_view(&production, &actual_view, production_plan);
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

/// The tree engine's view equals the graph engine's view of the tree's graph
/// form, read back as a tree from the same root; and re-encoding the tree view
/// as a graph and reading it back keeps every node and the sibling sequence.
fn assert_tree_view_is_graph_view(source: &WInstance, tree_view: &WInstance, plan: &RestrictPlan) {
    assert!(plan.preserves_tree_shape());
    let source_graph = GInstance::from_wtype(source).unwrap();
    let (graph_view, _) = graph_restrict_with_plan(&source_graph, plan).unwrap();
    let read = graph_view.to_wtype(tree_view.root).unwrap();
    assert_eq!(
        ref_tree(&read),
        ref_tree(tree_view),
        "graph view read as a tree"
    );
    let reencoded = GInstance::from_wtype(tree_view)
        .unwrap()
        .to_wtype(tree_view.root)
        .unwrap();
    assert_eq!(
        ref_tree(&reencoded),
        ref_tree(tree_view),
        "tree view re-encoded"
    );
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
    assert_tree_view_is_graph_view(&production_witness, &actual_order, production_plan);
}

/// The schemas, migration and tree of the fan-floor carrier: a fan on
/// `a` labelling `d`, and an expansion of `a -> c` through `b`.
fn fan_floor_fixture() -> (RefSchema, RefSchema, RefMigration, RefTree) {
    let hyper = RefHyperEdge {
        kind: "fan".to_owned(),
        signature: BTreeMap::from([
            ("l".to_owned(), "d".to_owned()),
            ("p".to_owned(), "a".to_owned()),
        ]),
        parent_label: "p".to_owned(),
    };
    let mut source = RefSchema::default()
        .vertex("a", "record")
        .vertex("c", "record")
        .vertex("d", "record")
        .edge("a", "d", "prop", Some("keep"))
        .edge("a", "c", "prop", Some("expand"));
    source.hyper_edges.insert("h".to_owned(), hyper.clone());
    let mut target = RefSchema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("c", "record")
        .vertex("d", "record")
        .edge("a", "d", "prop", Some("keep"))
        .edge("a", "b", "prop", Some("ab"))
        .edge("b", "c", "prop", Some("bc"));
    target.hyper_edges.insert("h".to_owned(), hyper);
    let mut migration = RefMigration {
        surviving_verts: ["a", "c", "d"].into_iter().map(str::to_owned).collect(),
        ..RefMigration::default()
    };
    migration
        .expansion_path
        .insert(("a".to_owned(), "c".to_owned()), vec!["b".to_owned()]);
    let tree = RefTree {
        nodes: BTreeMap::from([
            (0, RefNode::new(0, "a")),
            (1, RefNode::new(1, "c")),
            (2, RefNode::new(2, "d")),
        ]),
        arcs: vec![
            (0, 1, RefEdge::new("a", "c", "prop", Some("expand"))),
            (0, 2, RefEdge::new("a", "d", "prop", Some("keep"))),
        ],
        fans: vec![RefFan {
            parent: 0,
            hyper_edge: "h".to_owned(),
            children: BTreeMap::from([("l".to_owned(), 2)]),
        }],
        root: 0,
    };
    (source, target, migration, tree)
}

/// A source with a fan plus an expansion: the fan node of the tree's graph
/// form takes the first free node id, so the tree floor counts it and the
/// expansion's synthesized node is allocated above it, in tree and graph get.
fn fan_carrier_counts_toward_the_tree_floor() {
    let (source, target, migration, tree) = fan_floor_fixture();
    let production_source_schema = prod_schema(&source);
    let production_plan = RestrictPlan::compile(
        &production_source_schema,
        &prod_schema(&target),
        &prod_migration(&migration),
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
    let (expected_view, expected_complement, _) = reference_tree_get(&reference_lens, &tree)
        .expect("valid fan source")
        .expect("reference fan source restricts");
    let production_source = prod_tree(&tree);
    let (view, synthesis) = wtype_restrict_with_plan(&production_source, &production_plan).unwrap();
    assert_eq!(ref_tree(&view), expected_view);
    assert_eq!(view.fans.len(), 1, "the fan survives");
    assert_eq!(expected_complement.floor.0, 4, "nodes 0..=2 and fan node 3");
    assert_eq!(synthesis.id_floor, expected_complement.floor.0);
    let keys = synthesis
        .synthesized_keys
        .iter()
        .map(|(id, key)| (*id, ref_synth_key(key)))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(keys, expected_complement.synthesized_nodes);
    assert_eq!(keys.keys().copied().collect::<Vec<_>>(), vec![4]);
    let (source_graph, _) =
        GInstance::from_wtype_embedded(&production_source, Some(&production_source_schema))
            .unwrap();
    assert!(source_graph.nodes().contains_key(&3), "fan node 3");
    let (expected_graph, expected_graph_complement) = restrict::get(
        &source,
        &target,
        &migration,
        &NoValueLayer,
        &ref_graph(&source_graph),
    )
    .expect("reference graph get with a fan");
    let (graph_view, graph_complement) =
        graph_restrict_with_plan(&source_graph, &production_plan).unwrap();
    assert_eq!(ref_graph(&graph_view), expected_graph);
    assert_eq!(ref_complement(&graph_complement), expected_graph_complement);
    assert_eq!(graph_complement.id_floor.node, synthesis.id_floor);
    assert_eq!(
        graph_complement
            .synthesized_nodes
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![4]
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
    fan_carrier_counts_toward_the_tree_floor();
    drop_and_unresolved_fates_run_through_graph_and_tree_engines();
    drop_between_survivors_witness();
    assert_eq!(drop_between_survivors_relates_tree_and_graph_views(), 8);
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

/// A replaced endpoint also needs its synthesized incident edge replaced.
#[test]
fn translated_replacement_closes_over_incident_contraction_edges() {
    use panproto_reference::edit::{GElem, GPayload, GraphDelta, apply, get_edit};
    let source_schema = RefSchema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("c", "record")
        .edge("a", "b", "e", None)
        .edge("b", "c", "e", None);
    let target_schema = RefSchema::default()
        .vertex("a", "record")
        .vertex("c", "record")
        .edge("a", "c", "e", None);
    let migration = RefMigration {
        surviving_verts: BTreeSet::from(["a".to_owned(), "c".to_owned()]),
        ..RefMigration::default()
    };
    let source = RefGraph {
        nodes: BTreeMap::from([
            (0, RefNode::new(0, "a")),
            (1, RefNode::new(1, "b")),
            (2, RefNode::new(2, "c")),
        ]),
        edges: BTreeMap::from([
            (
                0,
                RefGEdge {
                    src: 0,
                    tgt: 1,
                    label: RefEdge::new("a", "b", "e", None),
                    position: None,
                },
            ),
            (
                1,
                RefGEdge {
                    src: 1,
                    tgt: 2,
                    label: RefEdge::new("b", "c", "e", None),
                    position: None,
                },
            ),
        ]),
    };
    let get = move |graph: &RefGraph| {
        restrict::get(
            &source_schema,
            &target_schema,
            &migration,
            &NoValueLayer,
            graph,
        )
    };
    let (view, _) = get(&source).unwrap();
    assert_eq!(view.edges.keys().copied().collect::<Vec<_>>(), vec![2]);
    let replaced = BTreeMap::from([
        (GElem::Node(0), GPayload::Node(source.nodes[&0].clone())),
        (GElem::Edge(0), GPayload::Edge(source.edges[&0].clone())),
    ]);
    let input = GraphDelta {
        del: replaced.clone(),
        add: replaced,
        attrs: BTreeMap::new(),
    };
    assert_eq!(apply(&input, &source).unwrap(), source);
    let translated = get_edit(&get, &source, &input).unwrap();
    assert_eq!(
        apply(&translated.delta, &view).unwrap(),
        view,
        "the translated replacement must apply to the contraction view"
    );
    assert_eq!(
        translated.delta.del.keys().cloned().collect::<Vec<_>>(),
        vec![GElem::Node(0), GElem::Edge(2)]
    );
    assert_eq!(translated.delta.del, translated.delta.add);
    assert_eq!(
        translated.delta.compose(&GraphDelta::identity()).unwrap(),
        translated.delta
    );
    assert_eq!(
        GraphDelta::identity().compose(&translated.delta).unwrap(),
        translated.delta
    );
    let identity = get_edit(&get, &source, &GraphDelta::identity()).unwrap();
    assert_eq!(identity.delta, GraphDelta::identity());
}

struct VisibleFieldLayer;
impl restrict::ValueLayer for VisibleFieldLayer {
    fn canonical_semantics(&self) -> restrict::ValueLayerSemantics {
        restrict::ValueLayerSemantics {
            domain: "visible-bool",
            version: 1,
            canonical_payload: vec![],
        }
    }
    fn survives(&self, _: &RefGraph, node: &RefNode) -> Result<bool, restrict::PredicateRefusal> {
        Ok(node.fields.get("visible") == Some(&RefValue::Bool(true)))
    }
    fn transform(
        &self,
        graph: &RefGraph,
        node: &RefNode,
    ) -> Result<(BTreeMap<String, RefValue>, Option<RefPresence>), restrict::LayerRefusal> {
        NoValueLayer.transform(graph, node)
    }
    fn restore(
        &self,
        view: &RefNode,
        fields: Option<&BTreeMap<String, RefValue>>,
        value: Option<&Option<RefPresence>>,
    ) -> Result<(BTreeMap<String, RefValue>, Option<RefPresence>), restrict::PutFieldRefusal> {
        NoValueLayer.restore(view, fields, value)
    }
}

/// The reference restrict domain admits the same two-attribute survival loop.
#[test]
fn reference_survival_loop_is_a_replacement_after_translation() {
    use panproto_reference::edit::{self, GElem, GraphDelta};
    let schema = RefSchema::default()
        .vertex("r", "record")
        .vertex("a", "record")
        .edge("r", "a", "prop", Some("a"));
    let migration = RefMigration {
        surviving_verts: BTreeSet::from(["r".to_owned(), "a".to_owned()]),
        surviving_edges: schema.edges.clone(),
        conditional: BTreeSet::from(["a".to_owned()]),
        ..RefMigration::default()
    };
    let mut leaf = RefNode::new(1, "a");
    leaf.fields
        .insert("visible".to_owned(), RefValue::Bool(true));
    let before = RefGraph {
        nodes: BTreeMap::from([(0, RefNode::new(0, "r")), (1, leaf)]),
        edges: BTreeMap::from([(
            0,
            RefGEdge {
                src: 0,
                tgt: 1,
                label: RefEdge::new("r", "a", "prop", Some("a")),
                position: None,
            },
        )]),
    };
    let mut middle = before.clone();
    middle
        .nodes
        .get_mut(&1)
        .unwrap()
        .fields
        .insert("visible".to_owned(), RefValue::Bool(false));
    let down = edit::diff(&before, &middle);
    let up = edit::diff(&middle, &before);
    assert_eq!(down.compose(&up).unwrap(), GraphDelta::identity());
    let plan = restrict::compile(&schema, &schema, &migration).unwrap();
    let layer = VisibleFieldLayer;
    let lens = RefGraphLens::new(
        &schema,
        &schema,
        &migration,
        &plan,
        &BTreeMap::new(),
        &layer,
    )
    .unwrap();
    for state in [&before, &middle] {
        let (view, complement) = lens.get(state).unwrap();
        assert_eq!(lens.put(&view, &complement).unwrap(), *state);
    }
    let closure_schema = schema.clone();
    let closure_migration = migration.clone();
    let get = move |state: &RefGraph| {
        restrict::get(
            &closure_schema,
            &closure_schema,
            &closure_migration,
            &VisibleFieldLayer,
            state,
        )
    };
    let first = edit::get_edit(&get, &before, &down).unwrap();
    let second = edit::get_edit(&get, &middle, &up).unwrap();
    let sequential = first.delta.compose(&second.delta).unwrap();
    let direct = edit::get_edit(&get, &before, &GraphDelta::identity()).unwrap();
    assert!(sequential.del.contains_key(&GElem::Node(1)));
    assert!(sequential.add.contains_key(&GElem::Node(1)));
    assert_eq!(direct.delta, GraphDelta::identity());
    assert_ne!(sequential, direct.delta);
    let (view, _) = get(&before).unwrap();
    assert_eq!(edit::apply(&sequential, &view).unwrap(), view);
}

/// The existing common-image domain excludes merged named tree attributes.
/// Graph/table placement remains defined, while the tree reports the exact boundary refusal.
#[test]
fn column_fate_many_to_one_is_outside_the_common_tree_image() {
    let protocol = table_protocol();
    let source = SchemaBuilder::new(&protocol)
        .vertex("t", "record", None)
        .and_then(|b| b.vertex("t.a", "int", None))
        .and_then(|b| b.vertex("t.b", "int", None))
        .and_then(|b| b.edge("t", "t.a", "prop", Some("a")))
        .and_then(|b| b.edge("t", "t.b", "prop", Some("b")))
        .and_then(SchemaBuilder::build)
        .unwrap();
    let (target, _, mut migration, _, _) =
        column_case_target(&protocol, &[ColumnState::Rename, ColumnState::Drop]);
    let edge = |target: &str, name: &str| Edge {
        src: "t".into(),
        tgt: target.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    };
    migration
        .vertex_remap
        .insert("t.b".into(), "t.a_new".into());
    migration
        .edge_remap
        .insert(edge("t.b", "b"), edge("t.a_new", "a_new"));
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let plan =
        RestrictPlan::compile_with_protocols(&source, &target, &migration, protocols).unwrap();
    let table_plan =
        RestrictPlan::compile_for_tables(&source, &target, &migration, protocols).unwrap();
    let target_edge = edge("t.a_new", "a_new");
    let expected_fates = vec![
        CellFate::Rename("a".to_owned(), "a_new".to_owned()),
        CellFate::Rename("b".to_owned(), "a_new".to_owned()),
    ];
    for compiled in [&plan, &table_plan] {
        for (vertex, column) in [("t.a", "a"), ("t.b", "b")] {
            assert_eq!(
                compiled.edge_image.fate(&edge(vertex, column)),
                Some(&EdgeFate::Keep(target_edge.clone())),
            );
        }
        assert_eq!(
            compiled
                .boundary
                .as_ref()
                .and_then(|b| b.cells.as_ref())
                .and_then(|cells| cells.fates.get("t")),
            Some(&expected_fates),
        );
    }
    let table = FInstance::new()
        .with_rows(
            "t",
            [(
                RowId(0),
                HashMap::from([
                    ("a".to_owned(), Value::Int(1)),
                    ("b".to_owned(), Value::Int(2)),
                ]),
            )],
        )
        .unwrap();
    let (table_view, _) = functor_restrict_with_plan(&table, &table_plan).unwrap();
    let expected = HashMap::from([("a_new".to_owned(), Value::Int(2))]);
    assert_eq!(table_view.row(RowId(0)), Some(&expected));
    let (graph, _) = GInstance::from_finstance(&table).unwrap();
    let (graph_view, _) = graph_restrict_with_plan(&graph, &plan).unwrap();
    assert_eq!(graph_view.node(0).map(|n| &n.extra_fields), Some(&expected));
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
        vec![(0, 1, edge("t.a", "a")), (0, 2, edge("t.b", "b"))],
        Vec::new(),
        0,
        "t".into(),
    );
    let error = wtype_restrict_with_plan(&tree, &plan).unwrap_err();
    assert!(
        matches!(
            error.unlocated(),
            RestrictError::MultipleArcsOnSingleValuedEdge { parent: 0, edge: found, count: 2 }
                if found.as_ref() == &target_edge
        ),
        "the documented tree boundary must name the merged edge and both arcs: {error:?}"
    );
}

fn generated_structural_plan_agrees(source: &RefSchema, migration: &RefMigration) {
    let production_schema = prod_schema(source);
    let production_migration = prod_migration(migration);
    let reference_image = restrict::edge_image(source, source, migration).unwrap();
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

    let optimized = restrict::contraction_plan(source, source, migration).unwrap();
    let walked = restrict::contraction_plan_by_walks(source, source, migration);
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
}

fn generated_graph_restriction_agrees(
    source: &RefSchema,
    migration: &RefMigration,
    graph: &RefGraph,
) {
    let production_schema = prod_schema(source);
    let production_migration = prod_migration(migration);
    let production_plan = RestrictPlan::compile(
        &production_schema,
        &production_schema,
        &production_migration,
    )
    .unwrap();
    let reference = restrict::get(source, source, migration, &NoValueLayer, graph);
    let production = graph_restrict_with_plan(&prod_graph(graph), &production_plan);
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
            for (id, node) in &reference_view.nodes {
                if let Some(original) = graph.nodes.get(id) {
                    assert_eq!(
                        (&node.value, &node.fields, &node.annotations),
                        (&original.value, &original.fields, &original.annotations),
                        "a kept node keeps its payload without a value program"
                    );
                }
            }
            let surviving = reference_view.nodes.keys().copied().collect();
            let walked = restrict::realized_pairs_by_walks(graph, &surviving);
            let synthesized: BTreeSet<(u32, u32)> = reference_complement
                .synthesized_edges
                .values()
                .filter_map(|key| match key {
                    RefSynthKey::Contract { src, tgt, .. }
                    | RefSynthKey::ContractExpand { src, tgt, .. } => Some((*src, *tgt)),
                    RefSynthKey::Expand { .. } => None,
                })
                .collect();
            assert_eq!(synthesized, walked, "realized pair census for {graph:?}");
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
}

fn generated_column_fates_agree(states: [ColumnState; 2]) {
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
    let (target, reference_target, migration, reference_migration, target_columns) =
        column_case_target(&protocol, &states);
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
    assert_column_runtime(&protocol, &source, &target, &migration, &production, states);
}

fn generated_tree_restriction_agrees(case: &TreeRestrictCase) {
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
    tree_case_is_success(case, &reference_lens, &production_plan);
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::default())]

    /// Schemas and survival classes are independently sampled; edge presence,
    /// node fields and annotations are generated before either engine runs.
    #[test]
    fn randomized_restriction_plan_and_graph_differential(
        schema_index in 0_usize..57,
        classification in 0_usize..27,
        edge_bits in proptest::prelude::any::<u8>(),
        payloads in proptest::prelude::any::<[i64;3]>(),
    ) {
        let source = &release_schemas()[schema_index];
        let migration = &survival_classifications(source)[classification];
        generated_structural_plan_agrees(source, migration);
        let mut graph = RefGraph::default();
        for (index,anchor) in ["a","b","c"].iter().enumerate() {
            let id = u32::try_from(index).unwrap();
            let mut node = RefNode::new(id,anchor);
            node.fields.insert("payload".into(),RefValue::Int(payloads[index]));
            node.annotations.insert("annotation".into(),RefValue::Int(payloads[index]));
            graph.nodes.insert(id,node);
        }
        for (index,edge) in source.edges.iter().enumerate() {
            if edge_bits & (1_u8 << index) != 0 {
                let node = |anchor: &str| u32::try_from(["a","b","c"].iter().position(|candidate| *candidate==anchor).unwrap()).unwrap();
                graph.edges.insert(u32::try_from(index).unwrap(),RefGEdge { src:node(&edge.src),tgt:node(&edge.tgt),label:edge.clone(),position:Some(u32::try_from(index).unwrap()) });
            }
        }
        generated_graph_restriction_agrees(source,migration,&graph);
    }

    #[test]
    fn randomized_pair_resolution_differential(
        direct in 0_usize..5, first in 0_usize..5, second in 0_usize..5, expansion in 0_usize..3,
    ) {
        let case = PairResolutionCase { direct:PairState::ALL[direct], first:PairState::ALL[first], second:PairState::ALL[second], expansion:[ExpansionState::Absent,ExpansionState::Empty,ExpansionState::ViaC][expansion] };
        let (target,migration)=resolution_case(case);
        let expected=restrict::resolve_pair(&target,&migration,"a","b");
        let actual=resolve_pair(&prod_schema(&target),&prod_migration(&migration),&"a".into(),&"b".into());
        assert_eq!(prod_resolution(&actual),ref_resolution(&expected));
        if let RefPairResolution::Unresolved(source,target_anchor,_) = expected {
            let actual=panproto_inst::contraction_plan::failing_hop(&prod_schema(&target),&prod_migration(&migration),&"a".into(),&"b".into());
            assert_eq!((actual.0.to_string(),actual.1.to_string()),(source,target_anchor));
        }
    }

    #[test]
    fn randomized_column_fate_differential(left in 0_usize..3,right in 0_usize..3) {
        let states=[ColumnState::Drop,ColumnState::Keep,ColumnState::Rename];
        generated_column_fates_agree([states[left],states[right]]);
    }

    #[test]
    fn randomized_ordered_child_binding_differential(word in proptest::collection::vec(0_usize..8,0..9)) {
        let slots=[ChildBindingSlot::SelfX,ChildBindingSlot::SelfY,ChildBindingSlot::MissingX,ChildBindingSlot::ZeroX,ChildBindingSlot::OneX,ChildBindingSlot::MissingY,ChildBindingSlot::ZeroY,ChildBindingSlot::OneY];
        let word:Vec<_>=word.into_iter().map(|index|slots[index]).collect();
        assert_child_binding_word(&word);
    }

    #[test]
    fn randomized_tree_restriction_differential(routes in proptest::collection::vec(proptest::prelude::any::<bool>(),1..7),floor in 0_usize..5) {
        let case=TreeRestrictCase { routes:routes.into_iter().map(|expand|if expand {TreeRoute::Expand} else {TreeRoute::Keep}).collect(),floor:[TreeFloorCase::Compact,TreeFloorCase::Gapped,TreeFloorCase::LastFree,TreeFloorCase::Saturated,TreeFloorCase::AlreadyMax][floor] };
        generated_tree_restriction_agrees(&case);
    }
}

/// Segmented graph restriction against its reference (L-segments-11).
mod segmented {
    use super::{
        NoValueLayer, RefGEdge, RefGraph, RefMigration, RefSchema, RestrictPlan,
        graph_restrict_with_plan, prod_graph, prod_migration, prod_schema, production_error,
        ref_complement, ref_edge, ref_graph, ref_node, ref_synth_key, reference_error,
    };
    use panproto_inst::InstanceShapeKind;
    use panproto_inst::segment::{
        GraphComplementSegment, GraphSegment, IdRange, Partition, SegmentId, SegmentSpec,
        SegmentedRestrictError, graph_restrict_segmented, partition_graph,
    };
    use panproto_inst::{EdgeId, GEdge};
    use panproto_reference::enumerate::segments::{
        segmented_restrict_case_count, segmented_restrict_cases,
    };
    use panproto_reference::segment as reference;

    /// The production partition of a reference graph partition.
    pub fn prod_partition(partition: &reference::Partition) -> Partition {
        let range = |r: &reference::IdRange| IdRange { lo: r.lo, hi: r.hi };
        Partition::new(
            InstanceShapeKind::Graph,
            partition
                .specs
                .iter()
                .map(|spec| SegmentSpec {
                    id: SegmentId(spec.id.0),
                    nodes: spec.nodes.iter().map(range).collect(),
                    alloc_nodes: spec.alloc_nodes.as_ref().map(range),
                    alloc_edges: spec.alloc_edges.as_ref().map(range),
                })
                .collect(),
        )
    }

    fn ref_gedge(edge: &GEdge) -> RefGEdge {
        RefGEdge {
            src: edge.src,
            tgt: edge.tgt,
            label: ref_edge(&edge.edge),
            position: edge.position,
        }
    }

    fn ref_edges(
        edges: &std::collections::BTreeMap<EdgeId, GEdge>,
    ) -> std::collections::BTreeMap<u32, RefGEdge> {
        edges
            .iter()
            .map(|(id, edge)| (id.0, ref_gedge(edge)))
            .collect()
    }

    /// The reference form of a production view segment.
    pub fn ref_view(segment: &GraphSegment) -> reference::GraphSegment {
        let boundary = &segment.boundary;
        reference::GraphSegment {
            segment: reference::SegmentId(segment.segment.0),
            nodes: segment
                .nodes
                .iter()
                .map(|(id, node)| (*id, ref_node(node)))
                .collect(),
            edges: ref_edges(&segment.edges),
            boundary: reference::Boundary {
                ghosts: boundary
                    .ghosts
                    .iter()
                    .map(|(id, ghost)| {
                        (
                            *id,
                            reference::Ghost {
                                node: ghost.node,
                                owner: reference::SegmentId(ghost.owner.0),
                                anchor: ghost.anchor.to_string(),
                            },
                        )
                    })
                    .collect(),
                in_stubs: boundary
                    .in_stubs
                    .iter()
                    .map(|(elem, stub)| {
                        let panproto_inst::ViewRef::Edge(id) = elem else {
                            panic!("a graph in-stub is keyed by its edge: {elem:?}")
                        };
                        (
                            reference::ViewRef::Edge(id.0),
                            reference::InStub {
                                src: stub.src,
                                src_owner: reference::SegmentId(stub.src_owner.0),
                                src_anchor: stub.src_anchor.to_string(),
                                tgt: stub.tgt,
                                label: ref_edge(&stub.label),
                            },
                        )
                    })
                    .collect(),
            },
        }
    }

    /// The reference form of a production complement segment.
    pub fn ref_complement_segment(
        segment: &GraphComplementSegment,
    ) -> reference::GraphComplementSegment {
        let mut whole = panproto_inst::GraphComplement::empty();
        whole.dropped_nodes.clone_from(&segment.dropped_nodes);
        whole.dropped_edges.clone_from(&segment.dropped_edges);
        whole.original_edges.clone_from(&segment.original_edges);
        whole.original_anchors.clone_from(&segment.original_anchors);
        whole
            .original_extra_fields
            .clone_from(&segment.original_extra_fields);
        whole.original_values.clone_from(&segment.original_values);
        let as_ref = ref_complement(&whole);
        reference::GraphComplementSegment {
            dropped_nodes: as_ref.dropped_nodes,
            dropped_edges: as_ref.dropped_edges,
            original_edges: as_ref.original_edges,
            original_anchors: as_ref.original_anchors,
            original_fields: as_ref.original_fields,
            original_values: as_ref.original_values,
            synthesized_nodes: segment
                .synthesized_nodes
                .iter()
                .map(|(id, key)| (*id, ref_synth_key(key)))
                .collect(),
            synthesized_edges: segment
                .synthesized_edges
                .iter()
                .map(|(id, key)| (id.0, ref_synth_key(key)))
                .collect(),
            floors: reference::SegmentFloors {
                node: segment.floors.node,
                edge: segment.floors.edge,
            },
        }
    }

    /// The outcome of one segmented case.
    #[derive(Debug, PartialEq, Eq)]
    pub enum Outcome {
        /// Both sides gave segments, equal, assembling to whole-state restrict.
        Agreed,
        /// Both refused alike.
        Refused,
    }

    /// Run one case through production and reference and check the law:
    /// the outputs agree segment by segment, and the production output
    /// assembles to production whole-state restrict.
    pub fn check_case(
        source: &RefSchema,
        target: &RefSchema,
        migration: &RefMigration,
        plan: &RestrictPlan,
        graph: &RefGraph,
        partition: &reference::Partition,
    ) -> Outcome {
        let production_partition = prod_partition(partition);
        let whole = prod_graph(graph);
        let segments = partition_graph(&whole, &production_partition).expect("production split");
        let reference_segments =
            reference::partition_graph(graph, partition).expect("reference split");
        let production = graph_restrict_segmented(plan, &segments, &production_partition);
        let expected = reference::restrict_graph_segmented(
            source,
            target,
            migration,
            &NoValueLayer,
            &reference_segments,
            partition,
        );
        match (production, expected) {
            (Ok(output), Ok(expected)) => {
                assert_eq!(
                    output.views.len(),
                    expected.len(),
                    "{graph:?} {partition:?}"
                );
                for ((view, complement), (expected_view, expected_complement)) in
                    output.views.iter().zip(&output.complements).zip(&expected)
                {
                    assert_eq!(
                        &ref_view(view),
                        expected_view,
                        "view of {graph:?} under {partition:?}"
                    );
                    assert_eq!(
                        &ref_complement_segment(complement),
                        expected_complement,
                        "complement of {graph:?} under {partition:?}"
                    );
                }
                let (view, complement) = output
                    .assemble(&production_partition)
                    .expect("the view segments assemble");
                let (whole_view, whole_complement) =
                    graph_restrict_with_plan(&whole, plan).expect("whole-state restrict accepts");
                assert_eq!(
                    ref_graph(&view),
                    ref_graph(&whole_view),
                    "assembled view of {graph:?}"
                );
                assert_eq!(
                    ref_complement(&complement),
                    ref_complement(&whole_complement),
                    "assembled complement of {graph:?}"
                );
                Outcome::Agreed
            }
            (
                Err(SegmentedRestrictError::Restrict { segment, error }),
                Err(reference::SegmentedRestrictRefusal::Restrict {
                    segment: expected_segment,
                    error: expected,
                }),
            ) => {
                assert_eq!(
                    segment.0, expected_segment.0,
                    "refusing segment of {graph:?}"
                );
                assert_eq!(
                    production_error(&error),
                    reference_error(&expected),
                    "refusal of {graph:?}"
                );
                let whole_error =
                    graph_restrict_with_plan(&whole, plan).expect_err("whole-state refuses");
                assert_eq!(production_error(&error), production_error(&whole_error));
                Outcome::Refused
            }
            (
                Err(SegmentedRestrictError::Segment(error)),
                Err(reference::SegmentedRestrictRefusal::Segment(expected)),
            ) => {
                assert_eq!(
                    format!("{error:?}"),
                    format!("{expected:?}"),
                    "segment refusal of {graph:?}"
                );
                Outcome::Refused
            }
            (production, expected) => panic!(
                "segmented restrict disagrees on {graph:?} under {partition:?}: {production:?} / {expected:?}"
            ),
        }
    }

    #[test]
    fn segmented_restrict_agrees_with_the_reference_and_assembles_to_whole_state_restrict() {
        let mut counts = (0_u64, 0_u64);
        let mut plan: Option<(RefSchema, RefMigration, RestrictPlan)> = None;
        for case in segmented_restrict_cases() {
            let fresh =
                !matches!(&plan, Some((s, m, _)) if *s == case.schema && *m == case.migration);
            if fresh {
                let schema = prod_schema(&case.schema);
                let compiled =
                    RestrictPlan::compile(&schema, &schema, &prod_migration(&case.migration))
                        .expect("release plans compile");
                plan = Some((case.schema.clone(), case.migration.clone(), compiled));
            }
            let Some((_, _, compiled)) = &plan else {
                unreachable!("a plan was just compiled")
            };
            match check_case(
                &case.schema,
                &case.schema,
                &case.migration,
                compiled,
                &case.graph,
                &case.partition,
            ) {
                Outcome::Agreed => counts.0 += 1,
                Outcome::Refused => counts.1 += 1,
            }
        }
        assert_eq!(counts.0 + counts.1, segmented_restrict_case_count());
        assert_eq!(counts.0 + counts.1, 128_240);
        assert!(
            counts.0 > 0 && counts.1 > 0,
            "both outcomes occur: {counts:?}"
        );
    }
}

/// The checkpointed run of segmented graph restriction against the
/// reference runner: equal reports at every cancellation point, and a
/// resumed run that ends as the uninterrupted one.
mod segmented_runner {
    use super::segmented::{prod_partition, ref_view};
    use super::{
        NoValueLayer, RefGraph, RefMigration, RefSchema, RestrictPlan, production_error,
        reference_error,
    };
    use panproto_inst::identity::Digest;
    use panproto_inst::segment::{
        AnySegment, ArtifactKey, ArtifactKind, Budget, EdgeIdRun, GraphRestrictRun, ManifestHeader,
        ManifestRole, MemoryStore, Phase, RecordSource, RunSpec, RunStatus, SegmentId,
        SegmentManifest, SegmentRecord, SegmentRunReport, SegmentStore, SegmentedRestrictError,
        TaskKey, graph_restrict_run, partition_graph, read_emitted, seal_full,
    };
    use panproto_reference::segment as reference;
    use std::collections::BTreeMap;

    const SOURCE_SCHEMA: [u8; 32] = [1; 32];
    const VIEW_SCHEMA: [u8; 32] = [2; 32];
    const LENS: [u8; 32] = [3; 32];

    /// A report in a form both implementations project to.
    #[derive(Debug, PartialEq, Eq)]
    pub struct Seen {
        status: String,
        locality: String,
        budget: (u64, Option<u64>, u64, u32),
        progress: (String, u32, u32, u64, u64),
        peak: (u64, Option<u64>),
        complement: (u64, u64, Vec<(u32, u64, u64)>),
        tasks: (u32, u32),
        orphans: Vec<([u8; 32], String, Option<u32>, String)>,
        phases: Vec<(String, u32, u32, u64, Option<u64>)>,
    }

    fn seen_production(report: &SegmentRunReport<SegmentedRestrictError>) -> Seen {
        let status = match &report.status {
            RunStatus::Complete { manifest } => format!("complete {:?}", manifest.0),
            RunStatus::Incomplete { reason } => format!("incomplete {reason:?}"),
            RunStatus::Residual(residual) => format!("residual {residual:?}"),
            RunStatus::Failed(SegmentedRestrictError::Restrict { segment, error }) => {
                format!("restrict {} {}", segment.0, production_error(error))
            }
            RunStatus::Failed(SegmentedRestrictError::Segment(error)) => {
                format!("segment {error:?}")
            }
            RunStatus::Failed(other) => format!("other {other:?}"),
        };
        Seen {
            status,
            locality: format!("{:?}", report.locality),
            budget: (
                report.budget.working_set,
                report.budget.complement,
                report.budget.summary,
                report.budget.affected_segments,
            ),
            progress: (
                format!("{:?}", report.progress.phase),
                report.progress.tasks_done,
                report.progress.tasks_total,
                report.progress.elements_done,
                report.progress.elements_total,
            ),
            peak: (report.peak_estimated_bytes, report.observed_peak_bytes),
            complement: (
                report.complement.elements,
                report.complement.bytes,
                report
                    .complement
                    .per_segment
                    .iter()
                    .map(|(id, (elements, bytes))| (id.0, *elements, *bytes))
                    .collect(),
            ),
            tasks: (report.tasks_run, report.tasks_reused),
            orphans: report
                .orphans
                .iter()
                .map(|key| {
                    (
                        key.run.0,
                        format!("{:?}", key.task.phase),
                        key.task.segment.map(|segment| segment.0),
                        format!("{:?}", key.kind),
                    )
                })
                .collect(),
            phases: report
                .phases
                .iter()
                .map(|phase| {
                    (
                        format!("{:?}", phase.phase),
                        phase.tasks,
                        phase.reused,
                        phase.peak_estimated_bytes,
                        phase.wall_ms,
                    )
                })
                .collect(),
        }
    }

    fn seen_reference(
        report: &reference::SegmentRunReport<reference::SegmentedRestrictRefusal>,
    ) -> Seen {
        let status = match &report.status {
            reference::RunStatus::Complete { manifest } => format!("complete {:?}", manifest.0),
            reference::RunStatus::Incomplete { reason } => format!("incomplete {reason:?}"),
            reference::RunStatus::Residual(residual) => format!("residual {residual:?}"),
            reference::RunStatus::Failed(reference::SegmentedRestrictRefusal::Restrict {
                segment,
                error,
            }) => format!("restrict {} {}", segment.0, reference_error(error)),
            reference::RunStatus::Failed(reference::SegmentedRestrictRefusal::Segment(error)) => {
                format!("segment {error:?}")
            }
            reference::RunStatus::Failed(other) => format!("other {other:?}"),
        };
        Seen {
            status,
            locality: format!("{:?}", report.locality.get),
            budget: (
                report.budget.working_set,
                report.budget.complement,
                report.budget.summary,
                report.budget.affected_segments,
            ),
            progress: (
                format!("{:?}", report.progress.phase),
                report.progress.tasks_done,
                report.progress.tasks_total,
                report.progress.elements_done,
                report.progress.elements_total,
            ),
            peak: (report.peak_estimated_bytes, report.observed_peak_bytes),
            complement: (
                report.complement.elements,
                report.complement.bytes,
                report
                    .complement
                    .per_segment
                    .iter()
                    .map(|(id, (elements, bytes))| (id.0, *elements, *bytes))
                    .collect(),
            ),
            tasks: (report.tasks_run, report.tasks_reused),
            orphans: report
                .orphans
                .iter()
                .map(|key| {
                    (
                        key.run.0,
                        format!("{:?}", key.task.phase),
                        key.task.segment.map(|segment| segment.0),
                        format!("{:?}", key.kind),
                    )
                })
                .collect(),
            phases: report
                .phases
                .iter()
                .map(|phase| {
                    (
                        format!("{:?}", phase.phase),
                        phase.tasks,
                        phase.reused,
                        phase.peak_estimated_bytes,
                        phase.wall_ms,
                    )
                })
                .collect(),
        }
    }

    /// The production input: the source manifest and the segment bytes.
    pub struct Input {
        /// The source manifest.
        pub manifest: SegmentManifest,
        /// Each segment's canonical bytes.
        pub bytes: BTreeMap<SegmentId, Vec<u8>>,
        /// The production partition.
        pub partition: panproto_inst::segment::Partition,
    }

    /// Seal the production segments of `graph` under `partition`.
    pub fn input(graph: &RefGraph, partition: &reference::Partition) -> Input {
        let partition = prod_partition(partition);
        let whole = super::prod_graph(graph);
        let segments: Vec<AnySegment> = partition_graph(&whole, &partition)
            .expect("production split")
            .into_iter()
            .map(AnySegment::Graph)
            .collect();
        let schema = Digest(SOURCE_SCHEMA);
        let records: Vec<SegmentRecord> = segments
            .iter()
            .zip(partition.specs())
            .map(|(segment, spec)| SegmentRecord::of(segment, spec, &schema))
            .collect();
        let runs: Vec<EdgeIdRun<'_>> = segments
            .iter()
            .map(|segment| {
                (
                    segment.segment(),
                    Box::new(segment.edge_id_run().into_iter())
                        as Box<dyn Iterator<Item = panproto_inst::EdgeId>>,
                )
            })
            .collect();
        let sources = segments
            .iter()
            .map(|segment| Box::new(segment.records()) as Box<dyn RecordSource + '_>)
            .collect();
        let manifest = seal_full(
            records,
            ManifestHeader {
                shape: panproto_inst::InstanceShapeKind::Graph,
                schema,
                role: ManifestRole::Source,
                binding: None,
            },
            runs,
            sources,
        )
        .expect("the carrier seals");
        let bytes = segments
            .iter()
            .map(|segment| {
                (
                    segment.segment(),
                    segment
                        .canonical_bytes(&schema, &partition)
                        .expect("canonical bytes"),
                )
            })
            .collect();
        Input {
            manifest,
            bytes,
            partition,
        }
    }

    /// One production run of `plan` over `input`.
    pub fn production_run(
        plan: &RestrictPlan,
        input: &Input,
        budget: Budget,
        cancel: Option<u32>,
        store: &mut MemoryStore,
    ) -> GraphRestrictRun {
        graph_restrict_run(
            plan,
            &input.manifest,
            &input.bytes,
            RunSpec {
                lens: Digest(LENS),
                input: input.manifest.digest(),
                view_schema: Digest(VIEW_SCHEMA),
                cancel_after_tasks: cancel,
            },
            budget,
            store,
            &mut |_| {},
        )
    }

    /// The lens and segments of one reference case.
    pub struct Case<'a> {
        /// Source schema.
        pub source: &'a RefSchema,
        /// Target schema.
        pub target: &'a RefSchema,
        /// Migration.
        pub migration: &'a RefMigration,
        /// Production plan.
        pub plan: &'a RestrictPlan,
        /// Source graph.
        pub graph: &'a RefGraph,
        /// Partition.
        pub partition: &'a reference::Partition,
    }

    fn reference_run(
        case: &Case<'_>,
        segments: &[reference::GraphSegment],
        budget: Budget,
        cancel: Option<u32>,
        store: &mut reference::MemoryStore,
    ) -> reference::RestrictRun {
        reference::segmented_restrict_with_progress(
            reference::RestrictLens {
                src: case.source,
                tgt: case.target,
                m: case.migration,
                vl: &NoValueLayer,
            },
            segments,
            case.partition,
            reference::RestrictRunSpec {
                lens: reference::Digest(LENS),
                source_schema: reference::Digest(SOURCE_SCHEMA),
                view_schema: reference::Digest(VIEW_SCHEMA),
                binding: None,
                cancel_after_tasks: cancel,
            },
            reference::Budget {
                working_set: budget.working_set,
                complement: budget.complement,
                summary: budget.summary,
                affected_segments: budget.affected_segments,
            },
            store,
            &mut |_| {},
        )
    }

    /// The emitted view and complement bytes of every segment of a run.
    fn emitted(
        store: &MemoryStore,
        run: &GraphRestrictRun,
        input: &Input,
    ) -> Vec<(String, Vec<u8>)> {
        let run = run.run.expect("a run identity");
        input
            .partition
            .specs()
            .iter()
            .map(|spec| {
                let emitted =
                    read_emitted(store, run, &input.partition, spec.id).expect("emitted segment");
                (format!("{:?}", ref_view(&emitted.view)), emitted.complement)
            })
            .collect()
    }

    /// The emitted view and complement of every segment of a complete run,
    /// after checking them and the Emit artifacts against the reference
    /// run's.
    fn complete_outputs(
        store: &MemoryStore,
        baseline: &GraphRestrictRun,
        reference_store: &reference::MemoryStore,
        expected: &reference::RestrictRun,
        input: &Input,
        context: &str,
    ) -> Vec<(String, Vec<u8>)> {
        let emitted = emitted(store, baseline, input);
        let expected_outputs = expected.output.as_ref().expect("reference output");
        for ((view, complement), (expected_view, expected_complement)) in
            emitted.iter().zip(expected_outputs)
        {
            assert_eq!(view, &format!("{expected_view:?}"), "view of {context}");
            let expected_bytes = reference::canonical_complement_segment_bytes(
                &reference::AnyComplementSegment::Graph(expected_complement.clone()),
            )
            .expect("reference complement bytes");
            assert_eq!(complement, &expected_bytes, "complement of {context}");
        }
        assert_emit_artifacts_agree(store, baseline, reference_store, expected, input, context);
        emitted
    }

    /// Every segment's Emit artifact is the same bytes in the production
    /// and reference stores.
    fn assert_emit_artifacts_agree(
        store: &MemoryStore,
        run: &GraphRestrictRun,
        reference_store: &reference::MemoryStore,
        expected: &reference::RestrictRun,
        input: &Input,
        context: &str,
    ) {
        use reference::SegmentStore as _;
        let run = run.run.expect("a run identity");
        for spec in input.partition.specs() {
            let production = store
                .read(&ArtifactKey {
                    run,
                    task: TaskKey {
                        phase: Phase::Emit,
                        segment: Some(spec.id),
                    },
                    kind: ArtifactKind::ComplementSegment,
                })
                .unwrap_or_else(|never| match never {})
                .expect("a production Emit artifact");
            let reference = reference_store
                .read(&reference::ArtifactKey {
                    run: expected.run,
                    task: reference::TaskKey {
                        phase: reference::Phase::Emit,
                        segment: Some(reference::SegmentId(spec.id.0)),
                    },
                    kind: reference::ArtifactKind::ComplementSegment,
                })
                .unwrap_or_else(|never| match never {})
                .expect("a reference Emit artifact");
            assert_eq!(
                production, reference,
                "Emit artifact of segment {} of {context}",
                spec.id.0
            );
        }
    }

    /// What one case showed.
    #[derive(Debug, Default, PartialEq, Eq)]
    pub struct Counts {
        /// Complete runs.
        pub complete: u64,
        /// Refused runs.
        pub refused: u64,
        /// Declined runs.
        pub residual: u64,
        /// Cancellation points checked.
        pub cancellations: u64,
    }

    /// Run one case uninterrupted and from every cancellation point, through
    /// production and reference, and check the law.
    pub fn check_case(case: &Case<'_>, budget: Budget, counts: &mut Counts) {
        let input = input(case.graph, case.partition);
        let segments =
            reference::partition_graph(case.graph, case.partition).expect("reference split");
        let mut store = MemoryStore::default();
        let baseline = production_run(case.plan, &input, budget, None, &mut store);
        let mut reference_store = reference::MemoryStore::default();
        let expected = reference_run(case, &segments, budget, None, &mut reference_store);
        let context = format!("{:?} under {:?}", case.graph, case.partition);
        assert_eq!(
            seen_production(&baseline.report),
            seen_reference(&expected.report),
            "uninterrupted run of {context}"
        );
        assert_eq!(
            baseline.run.map(|run| run.0),
            Some(expected.run.0),
            "run identity of {context}"
        );
        let outputs = match &baseline.report.status {
            RunStatus::Complete { manifest } => {
                counts.complete += 1;
                let sealed = baseline
                    .manifest
                    .as_ref()
                    .expect("a complete run's manifest");
                assert_eq!(sealed.digest(), *manifest, "{context}");
                Some(complete_outputs(
                    &store,
                    &baseline,
                    &reference_store,
                    &expected,
                    &input,
                    &context,
                ))
            }
            RunStatus::Failed(_) => {
                counts.refused += 1;
                None
            }
            RunStatus::Residual(_) => {
                counts.residual += 1;
                None
            }
            RunStatus::Incomplete { reason } => panic!("{context} stopped: {reason:?}"),
        };
        let total = baseline.report.progress.tasks_total;
        let done = baseline.report.progress.tasks_done;
        for cancel in 0..=total {
            counts.cancellations += 1;
            let mut store = MemoryStore::default();
            let interrupted = production_run(case.plan, &input, budget, Some(cancel), &mut store);
            let mut reference_store = reference::MemoryStore::default();
            let expected =
                reference_run(case, &segments, budget, Some(cancel), &mut reference_store);
            assert_eq!(
                seen_production(&interrupted.report),
                seen_reference(&expected.report),
                "run of {context} cancelled after {cancel} tasks"
            );
            let resumed = production_run(case.plan, &input, budget, None, &mut store);
            let expected = reference_run(case, &segments, budget, None, &mut reference_store);
            assert_eq!(
                seen_production(&resumed.report),
                seen_reference(&expected.report),
                "run of {context} resumed after {cancel} tasks"
            );
            assert_eq!(
                resumed.report.tasks_reused,
                cancel.min(done),
                "reuse after {cancel} tasks of {context}"
            );
            assert_eq!(
                format!("{:?}", resumed.report.status),
                format!("{:?}", baseline.report.status),
                "{context}"
            );
            assert_eq!(resumed.manifest, baseline.manifest, "{context}");
            assert!(
                store
                    .staged_uncommitted(&resumed.run.expect("a run identity"))
                    .expect("memory store")
                    .is_empty(),
                "no staged artifact is left after resuming {context}"
            );
            if let Some(outputs) = &outputs {
                assert_eq!(&emitted(&store, &resumed, &input), outputs, "{context}");
            }
        }
    }

    /// Every `STRIDE`th case of the segmented restrict carrier.
    const STRIDE: usize = 97;

    #[test]
    fn a_checkpointed_restrict_run_matches_the_reference_runner_at_every_cancellation_point() {
        use super::{prod_migration, prod_schema};
        use panproto_reference::enumerate::segments::segmented_restrict_cases;
        let mut counts = Counts::default();
        let mut plan: Option<(RefSchema, RefMigration, RestrictPlan)> = None;
        for case in segmented_restrict_cases().step_by(STRIDE) {
            let fresh =
                !matches!(&plan, Some((s, m, _)) if *s == case.schema && *m == case.migration);
            if fresh {
                let schema = prod_schema(&case.schema);
                let compiled =
                    RestrictPlan::compile(&schema, &schema, &prod_migration(&case.migration))
                        .expect("release plans compile");
                plan = Some((case.schema.clone(), case.migration.clone(), compiled));
            }
            let Some((_, _, compiled)) = &plan else {
                unreachable!("a plan was just compiled")
            };
            check_case(
                &Case {
                    source: &case.schema,
                    target: &case.schema,
                    migration: &case.migration,
                    plan: compiled,
                    graph: &case.graph,
                    partition: &case.partition,
                },
                Budget::unlimited(),
                &mut counts,
            );
        }
        assert_eq!(
            counts.complete + counts.refused + counts.residual,
            128_240_u64.div_ceil(97)
        );
        assert!(
            counts.complete > 0 && counts.refused > 0,
            "both outcomes occur: {counts:?}"
        );
    }
}

/// Witnesses of segmented graph restriction beyond the release carrier:
/// hidden paths through several segments, fans and expansions crossing
/// segments, allocation refusals and the residual of a predicate.
mod segmented_witnesses {
    use super::segmented::{Outcome, check_case, prod_partition};
    use super::segmented_runner;
    use super::{
        RefEdge, RefGEdge, RefGraph, RefMigration, RefNode, RefSchema, RestrictPlan,
        fan_floor_fixture, prod_graph, prod_migration, prod_schema,
    };
    use panproto_inst::identity::Digest;
    use panproto_inst::locality::{GlobalReason, SegmentOp};
    use panproto_inst::segment::{
        AnySegment, ArtifactKey, Budget, MemoryStore, Phase, RunIncompleteReason, RunJournal,
        RunSpec, RunStatus, SegmentStore, TaskRecord, graph_restrict_run, read_emitted,
        verify_manifest, verify_segment,
    };
    use panproto_inst::segment::{
        IdKind, Residual, SegmentError, SegmentId, SegmentedRestrictError,
        graph_restrict_segmented, partition_graph,
    };
    use panproto_reference::enumerate::segments::{
        SEGMENTED_RESTRICT_ALLOCATION_BASE, allocated_splits,
    };
    use panproto_reference::segment as reference;

    /// A dropped vertex `c` with a loop, between surviving `a` and `b`.
    fn hidden_path_fixture() -> (RefSchema, RefMigration, RefGraph) {
        let schema = RefSchema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .vertex("c", "record")
            .edge("a", "b", "prop", Some("ab"))
            .edge("a", "c", "prop", Some("ac"))
            .edge("c", "c", "prop", Some("cc"))
            .edge("c", "b", "prop", Some("cb"));
        let migration = RefMigration {
            surviving_verts: ["a", "b"].into_iter().map(str::to_owned).collect(),
            surviving_edges: std::iter::once(RefEdge::new("a", "b", "prop", Some("ab"))).collect(),
            ..RefMigration::default()
        };
        let mut graph = RefGraph::default();
        for (id, anchor) in [(0, "a"), (1, "c"), (2, "c"), (3, "c"), (4, "b")] {
            graph.nodes.insert(id, RefNode::new(id, anchor));
        }
        let edges = [
            (0, 1, "a", "c", "ac"),
            (1, 2, "c", "c", "cc"),
            (2, 3, "c", "c", "cc"),
            (3, 1, "c", "c", "cc"),
            (3, 4, "c", "b", "cb"),
        ];
        for (id, (src, tgt, a, b, name)) in (0_u32..).zip(edges) {
            graph.edges.insert(
                id,
                RefGEdge {
                    src,
                    tgt,
                    label: RefEdge::new(a, b, "prop", Some(name)),
                    position: None,
                },
            );
        }
        (schema, migration, graph)
    }

    fn compile(source: &RefSchema, target: &RefSchema, migration: &RefMigration) -> RestrictPlan {
        RestrictPlan::compile(
            &prod_schema(source),
            &prod_schema(target),
            &prod_migration(migration),
        )
        .expect("the witness plan compiles")
    }

    /// The partition owning `owners[i]`'s nodes in segment `i`, with or
    /// without the carrier's allocation ranges, or with edge ranges of
    /// width `width` at the carrier base.
    fn partition(
        owners: &[&[u32]],
        node_width: Option<u32>,
        edge_width: Option<u32>,
    ) -> reference::Partition {
        reference::Partition {
            shape: reference::InstanceShapeKind::Graph,
            specs: owners
                .iter()
                .zip(0_u32..)
                .map(|(nodes, index)| {
                    let lo = SEGMENTED_RESTRICT_ALLOCATION_BASE + 10 * index;
                    let range = |width: u32| reference::IdRange {
                        lo,
                        hi: lo + width - 1,
                    };
                    let mut spec_nodes: Vec<reference::IdRange> = nodes
                        .iter()
                        .map(|id| reference::IdRange { lo: *id, hi: *id })
                        .collect();
                    let alloc_nodes = node_width.map(range);
                    spec_nodes.extend(alloc_nodes);
                    reference::SegmentSpec {
                        id: reference::SegmentId(index),
                        nodes: spec_nodes,
                        alloc_nodes,
                        alloc_edges: edge_width.map(range),
                    }
                })
                .collect(),
        }
    }

    #[test]
    fn a_hidden_path_through_three_segments_contracts_like_whole_state_restrict() {
        let (schema, migration, graph) = hidden_path_fixture();
        let plan = compile(&schema, &schema, &migration);
        let nodes: Vec<u32> = graph.nodes.keys().copied().collect();
        let splits = allocated_splits(&nodes);
        assert_eq!(splits.len(), 2_usize.pow(5) + 3_usize.pow(5));
        for split in &splits {
            assert_eq!(
                check_case(&schema, &schema, &migration, &plan, &graph, split),
                Outcome::Agreed,
                "{split:?}"
            );
        }
        // Node 0 in segment 0, the path leaving through 1 (segment 1), 2
        // (segment 2) and 3 (segment 0) to 4 (segment 1).
        let split = partition(&[&[0, 3], &[1, 4], &[2]], Some(10), Some(10));
        let production_partition = prod_partition(&split);
        let segments =
            partition_graph(&prod_graph(&graph), &production_partition).expect("production split");
        let output = graph_restrict_segmented(&plan, &segments, &production_partition)
            .expect("the hidden path restricts");
        let first = &output.views[0];
        let contracted: Vec<(u32, u32)> = first
            .edges
            .iter()
            .filter(|(id, _)| output.complements[0].synthesized_edges.contains_key(id))
            .map(|(_, edge)| (edge.src, edge.tgt))
            .collect();
        assert_eq!(
            contracted,
            vec![(0, 4)],
            "the pair is joined in its first node's segment"
        );
        assert_eq!(
            first.boundary.ghosts.get(&4).map(|g| g.owner),
            Some(SegmentId(1))
        );
        assert!(output.views[1].boundary.in_stubs.keys().any(|elem| {
            matches!(elem, panproto_inst::ViewRef::Edge(id) if id.0 >= SEGMENTED_RESTRICT_ALLOCATION_BASE)
        }));
    }

    #[test]
    fn fans_and_expansions_crossing_segments_restrict_like_whole_state_restrict() {
        let (source, target, migration, tree) = fan_floor_fixture();
        let (graph, _) = panproto_reference::encode::tree_to_graph(&tree, Some(&source))
            .expect("the fixture tree has a graph form");
        assert!(
            graph
                .nodes
                .values()
                .any(|n| n.shape == super::RefNodeShape::HyperEdge)
        );
        let plan = compile(&source, &target, &migration);
        let nodes: Vec<u32> = graph.nodes.keys().copied().collect();
        let mut agreed = 0;
        for split in allocated_splits(&nodes) {
            assert_eq!(
                check_case(&source, &target, &migration, &plan, &graph, &split),
                Outcome::Agreed,
                "{split:?}"
            );
            agreed += 1;
        }
        assert_eq!(agreed, 2_u32.pow(4) + 3_u32.pow(4));
    }

    #[test]
    fn allocation_refusals_agree_with_the_reference() {
        let (schema, migration, graph) = hidden_path_fixture();
        let plan = compile(&schema, &schema, &migration);
        let owners: [&[u32]; 2] = [&[0, 1, 2], &[3, 4]];
        // No edge range where the pair is joined.
        let none = partition(&owners, None, None);
        assert_eq!(
            check_case(&schema, &schema, &migration, &plan, &graph, &none),
            Outcome::Refused
        );
        let segments = partition_graph(&prod_graph(&graph), &prod_partition(&none)).expect("split");
        assert!(matches!(
            graph_restrict_segmented(&plan, &segments, &prod_partition(&none)),
            Err(SegmentedRestrictError::Segment(
                SegmentError::NoAllocationRange {
                    segment: SegmentId(0),
                    kind: IdKind::Edge
                }
            ))
        ));
        // An edge range below an edge id in use.
        let mut below = partition(&owners, Some(10), Some(10));
        below.specs[1].alloc_edges = Some(reference::IdRange { lo: 4, hi: 9 });
        assert_eq!(
            check_case(&schema, &schema, &migration, &plan, &graph, &below),
            Outcome::Refused
        );
        // An expansion of two edges into a one-id edge range.
        let (source, target, expanding, tree) = fan_floor_fixture();
        let (fan_graph, _) =
            panproto_reference::encode::tree_to_graph(&tree, Some(&source)).expect("graph form");
        let fan_plan = compile(&source, &target, &expanding);
        let all: Vec<u32> = fan_graph.nodes.keys().copied().collect();
        let narrow = partition(&[&all], Some(10), Some(1));
        assert_eq!(
            check_case(&source, &target, &expanding, &fan_plan, &fan_graph, &narrow),
            Outcome::Refused
        );
        let segments =
            partition_graph(&prod_graph(&fan_graph), &prod_partition(&narrow)).expect("split");
        assert!(matches!(
            graph_restrict_segmented(&fan_plan, &segments, &prod_partition(&narrow)),
            Err(SegmentedRestrictError::Segment(
                SegmentError::AllocationExhausted {
                    kind: IdKind::Edge,
                    ..
                }
            ))
        ));
    }

    #[test]
    fn a_survival_predicate_is_declined_as_a_global_expression_before_any_segment_is_read() {
        let (schema, mut migration, graph) = hidden_path_fixture();
        migration.conditional.insert("b".to_owned());
        let plan = compile(&schema, &schema, &migration);
        let split = partition(&[&[0, 1, 2], &[3, 4]], Some(10), Some(10));
        let segments =
            partition_graph(&prod_graph(&graph), &prod_partition(&split)).expect("split");
        let declined = graph_restrict_segmented(&plan, &segments, &prod_partition(&split));
        assert!(matches!(
            declined,
            Err(SegmentedRestrictError::Residual(Residual::Global {
                op: SegmentOp::Restrict,
                reason: GlobalReason::ExpressionDemand { ref vertex },
            })) if vertex.as_ref() == "b"
        ));
        // Declined before reading: an empty family is declined alike.
        assert!(matches!(
            graph_restrict_segmented(&plan, &[], &prod_partition(&split)),
            Err(SegmentedRestrictError::Residual(_))
        ));
        // The checkpointed run declines it before its first task.
        let input = segmented_runner::input(&graph, &split);
        let mut store = MemoryStore::default();
        let run =
            segmented_runner::production_run(&plan, &input, Budget::unlimited(), None, &mut store);
        assert!(matches!(
            run.report.status,
            RunStatus::Residual(Residual::Global {
                op: SegmentOp::Restrict,
                reason: GlobalReason::ExpressionDemand { ref vertex },
            }) if vertex.as_ref() == "b"
        ));
        assert_eq!(run.report.progress.tasks_done, 0);
    }

    #[test]
    fn a_checkpointed_run_declines_an_exceeded_budget_like_the_reference_runner() {
        let (schema, graph) = (hidden_path_fixture().0, hidden_path_fixture().2);
        let migration = keep_everything(&schema);
        let plan = compile(&schema, &schema, &migration);
        let split = partition(&[&[0, 3], &[1, 4], &[2]], Some(10), Some(10));
        let unlimited = Budget::unlimited();
        let budgets = [
            Budget {
                working_set: 0,
                ..unlimited
            },
            Budget {
                affected_segments: 2,
                ..unlimited
            },
            Budget {
                summary: 0,
                ..unlimited
            },
            Budget {
                complement: Some(0),
                ..unlimited
            },
            unlimited,
        ];
        let mut counts = segmented_runner::Counts::default();
        for budget in budgets {
            segmented_runner::check_case(
                &segmented_runner::Case {
                    source: &schema,
                    target: &schema,
                    migration: &migration,
                    plan: &plan,
                    graph: &graph,
                    partition: &split,
                },
                budget,
                &mut counts,
            );
        }
        assert_eq!((counts.residual, counts.complete), (4, 1), "{counts:?}");
    }

    /// The peaks of the Validate and Seal phases of a complete production
    /// run of `graph`, after checking the run against the reference runner.
    fn validate_and_seal_peaks(
        schema: &RefSchema,
        migration: &RefMigration,
        graph: &RefGraph,
        split: &reference::Partition,
    ) -> (u64, u64) {
        let plan = compile(schema, schema, migration);
        let mut counts = segmented_runner::Counts::default();
        let case = segmented_runner::Case {
            source: schema,
            target: schema,
            migration,
            plan: &plan,
            graph,
            partition: split,
        };
        segmented_runner::check_case(&case, Budget::unlimited(), &mut counts);
        assert_eq!(counts.complete, 1, "the run completes: {counts:?}");
        let input = segmented_runner::input(graph, split);
        let mut store = MemoryStore::default();
        let run =
            segmented_runner::production_run(&plan, &input, Budget::unlimited(), None, &mut store);
        let peak = |phase: Phase| {
            run.report
                .phases
                .iter()
                .find(|report| report.phase == phase)
                .map(|report| report.peak_estimated_bytes)
                .expect("the phase finished")
        };
        (peak(Phase::Validate), peak(Phase::Seal))
    }

    #[test]
    fn seal_holds_the_same_bytes_for_graphs_with_more_records_per_segment_while_the_segment_peak_grows()
     {
        let schema = RefSchema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .edge("a", "b", "prop", Some("ab"));
        let migration = keep_everything(&schema);
        let spec = |index: u32| reference::SegmentSpec {
            id: reference::SegmentId(index),
            nodes: vec![reference::IdRange {
                lo: 10 * index,
                hi: 10 * index + 9,
            }],
            alloc_nodes: None,
            alloc_edges: None,
        };
        let split = reference::Partition {
            shape: reference::InstanceShapeKind::Graph,
            specs: vec![spec(0), spec(1)],
        };
        // `width` nodes `a` in segment 0, `width` nodes `b` in segment 1,
        // and edge `i` from the `i`th `a` to the `i`th `b`: one cross peer
        // each way whatever the width.
        let graph = |width: u32| {
            let mut graph = RefGraph::default();
            for i in 0..width {
                graph.nodes.insert(i, RefNode::new(i, "a"));
                graph.nodes.insert(10 + i, RefNode::new(10 + i, "b"));
                graph.edges.insert(
                    i,
                    RefGEdge {
                        src: i,
                        tgt: 10 + i,
                        label: RefEdge::new("a", "b", "prop", Some("ab")),
                        position: None,
                    },
                );
            }
            graph
        };
        let (narrow_validate, narrow_seal) =
            validate_and_seal_peaks(&schema, &migration, &graph(1), &split);
        let (wide_validate, wide_seal) =
            validate_and_seal_peaks(&schema, &migration, &graph(3), &split);
        assert!(
            wide_validate > narrow_validate,
            "the segment peak grows: {narrow_validate} then {wide_validate}"
        );
        assert_eq!(narrow_seal, wide_seal);
        // Per segment: the 56-byte header and the record (id 4, one node
        // range 8 after its count 8, two absent allocation ranges 2, stats
        // 52, the greatest node 5, the greatest edge 5 or 1, two digests
        // 64, the outgoing and incoming cross maps 8 + 36 per peer each);
        // segment 0 adds one edge id 4 and its longest frame, an edge
        // (tag 1, id, endpoints 12, the label's three names 9 + 9 + 12,
        // its name 1 + 10, no position 1, after its length 8); segment 1
        // adds its longest frame, a node (tag 1, id 4, anchor 9, no value,
        // discriminator or position 3, no fields or annotations 16, plain
        // shape 1, after its length 8).
        let record = |max_edge: u64| 4 + 8 + 8 + 2 + 52 + 5 + max_edge + 64 + 8 + 8 + 36;
        let segment_0 = 56 + record(5) + 4 + (1 + 4 + 8 + 9 + 9 + 12 + 11 + 1 + 8);
        let segment_1 = 56 + record(1) + (1 + 4 + 9 + 3 + 16 + 1 + 8);
        assert_eq!(narrow_seal, segment_0 + segment_1);
    }

    /// Run a plan whose view synthesizes edges through the checkpointed
    /// runner, check it against the reference runner at every cancellation
    /// point and after resuming, and check that its manifest verifies and
    /// seals view segments holding a synthesized edge inside an edge
    /// allocation range.
    fn check_synthesizing_run(case: &segmented_runner::Case<'_>) {
        let mut counts = segmented_runner::Counts::default();
        segmented_runner::check_case(case, Budget::unlimited(), &mut counts);
        assert_eq!(counts.complete, 1, "the run completes: {counts:?}");
        let input = segmented_runner::input(case.graph, case.partition);
        let mut store = MemoryStore::default();
        let run = segmented_runner::production_run(
            case.plan,
            &input,
            Budget::unlimited(),
            None,
            &mut store,
        );
        let manifest = run.manifest.expect("a complete run's manifest");
        assert_eq!(verify_manifest(&manifest, manifest.digest()), Ok(()));
        let run = run.run.expect("a run identity");
        let mut synthesized = 0;
        for spec in input.partition.specs() {
            let emitted =
                read_emitted(&store, run, &input.partition, spec.id).expect("emitted segment");
            synthesized += emitted
                .view
                .edges
                .keys()
                .filter(|id| spec.alloc_edges.is_some_and(|range| range.contains(id.0)))
                .count();
            assert_eq!(
                verify_segment(&manifest, &AnySegment::Graph(emitted.view)),
                Ok(())
            );
        }
        assert!(
            synthesized > 0,
            "the view holds an edge id inside its holder's edge range"
        );
    }

    #[test]
    fn contracting_and_expanding_restrict_runs_seal_verify_and_resume_from_every_cancellation_point()
     {
        // A hidden path contracted across three segments.
        let (schema, migration, graph) = hidden_path_fixture();
        let plan = compile(&schema, &schema, &migration);
        let split = partition(&[&[0, 3], &[1, 4], &[2]], Some(10), Some(10));
        check_synthesizing_run(&segmented_runner::Case {
            source: &schema,
            target: &schema,
            migration: &migration,
            plan: &plan,
            graph: &graph,
            partition: &split,
        });
        // A fan expanded into a relation node and its edges, with the fan's
        // nodes split between two segments.
        let (source, target, expanding, tree) = fan_floor_fixture();
        let (fan_graph, _) =
            panproto_reference::encode::tree_to_graph(&tree, Some(&source)).expect("graph form");
        let fan_plan = compile(&source, &target, &expanding);
        let nodes: Vec<u32> = fan_graph.nodes.keys().copied().collect();
        let (even, odd): (Vec<u32>, Vec<u32>) = nodes.iter().partition(|id| *id % 2 == 0);
        let fan_split = partition(&[&even, &odd], Some(10), Some(10));
        check_synthesizing_run(&segmented_runner::Case {
            source: &source,
            target: &target,
            migration: &expanding,
            plan: &fan_plan,
            graph: &fan_graph,
            partition: &fan_split,
        });
    }

    /// The migration keeping every vertex and edge of `schema`.
    fn keep_everything(schema: &RefSchema) -> RefMigration {
        RefMigration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.iter().cloned().collect(),
            ..RefMigration::default()
        }
    }

    /// A store whose `fail_at`th commit fails, leaving its artifact staged.
    struct FailingStore {
        inner: MemoryStore,
        commits: usize,
        fail_at: Option<usize>,
    }

    #[derive(Debug)]
    struct CommitFailed;

    impl std::fmt::Display for CommitFailed {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("commit failed")
        }
    }

    impl std::error::Error for CommitFailed {}

    impl SegmentStore for FailingStore {
        type Error = CommitFailed;

        fn read(&self, key: &ArtifactKey) -> Result<Option<Vec<u8>>, Self::Error> {
            Ok(self.inner.read(key).unwrap_or_else(|never| match never {}))
        }

        fn read_at(
            &self,
            key: &ArtifactKey,
            offset: u64,
            len: u64,
        ) -> Result<Option<Vec<u8>>, Self::Error> {
            Ok(self
                .inner
                .read_at(key, offset, len)
                .unwrap_or_else(|never| match never {}))
        }

        fn stage(&mut self, key: &ArtifactKey, bytes: Vec<u8>) -> Result<(), Self::Error> {
            self.inner
                .stage(key, bytes)
                .unwrap_or_else(|never| match never {});
            Ok(())
        }

        fn commit(&mut self, key: &ArtifactKey, record: &TaskRecord) -> Result<(), Self::Error> {
            self.commits += 1;
            if Some(self.commits) == self.fail_at {
                return Err(CommitFailed);
            }
            self.inner
                .commit(key, record)
                .unwrap_or_else(|never| match never {});
            Ok(())
        }

        fn journal(&self, run: &Digest) -> Result<RunJournal, Self::Error> {
            Ok(self
                .inner
                .journal(run)
                .unwrap_or_else(|never| match never {}))
        }

        fn staged_uncommitted(&self, run: &Digest) -> Result<Vec<ArtifactKey>, Self::Error> {
            Ok(self
                .inner
                .staged_uncommitted(run)
                .unwrap_or_else(|never| match never {}))
        }
    }

    #[test]
    fn a_failed_commit_reports_its_staged_artifact_and_a_resumed_run_completes_it() {
        let (schema, graph) = (hidden_path_fixture().0, hidden_path_fixture().2);
        let migration = keep_everything(&schema);
        let plan = compile(&schema, &schema, &migration);
        let split = partition(&[&[0, 3], &[1, 4], &[2]], Some(10), Some(10));
        let input = segmented_runner::input(&graph, &split);
        let spec = RunSpec {
            lens: Digest([3; 32]),
            input: input.manifest.digest(),
            view_schema: Digest([2; 32]),
            cancel_after_tasks: None,
        };
        let mut baseline_store = MemoryStore::default();
        let baseline = graph_restrict_run(
            &plan,
            &input.manifest,
            &input.bytes,
            spec,
            Budget::unlimited(),
            &mut baseline_store,
            &mut |_| {},
        );
        assert!(
            matches!(baseline.report.status, RunStatus::Complete { .. }),
            "{:?}",
            baseline.report.status
        );
        let total = baseline.report.progress.tasks_total;
        for fail_at in 1..=total {
            let mut store = FailingStore {
                inner: MemoryStore::default(),
                commits: 0,
                fail_at: Some(usize::try_from(fail_at).unwrap_or(usize::MAX)),
            };
            let failed = graph_restrict_run(
                &plan,
                &input.manifest,
                &input.bytes,
                spec,
                Budget::unlimited(),
                &mut store,
                &mut |_| {},
            );
            assert!(
                matches!(
                    &failed.report.status,
                    RunStatus::Incomplete {
                        reason: RunIncompleteReason::Store { message }
                    } if message == "commit failed"
                ),
                "{:?}",
                failed.report.status
            );
            assert_eq!(failed.report.progress.tasks_done, fail_at - 1);
            assert_eq!(failed.report.orphans.len(), 1, "the failed task's artifact");
            store.fail_at = None;
            let resumed = graph_restrict_run(
                &plan,
                &input.manifest,
                &input.bytes,
                spec,
                Budget::unlimited(),
                &mut store,
                &mut |_| {},
            );
            assert_eq!(resumed.report.tasks_reused, fail_at - 1);
            assert!(resumed.report.orphans.is_empty());
            assert_eq!(resumed.manifest, baseline.manifest);
            assert_eq!(
                format!("{:?}", resumed.report.status),
                format!("{:?}", baseline.report.status)
            );
        }
    }
}

/// Generated segmented restricts against the reference: graphs of up to
/// eight nodes over a schema with a dropped loop, every survival choice,
/// and owners drawn from up to four segments.
mod segmented_randomized {
    use super::segmented::{Outcome, check_case};
    use super::{RefEdge, RefGEdge, RefGraph, RefMigration, RefNode, RefSchema, RestrictPlan};
    use super::{prod_migration, prod_schema};
    use panproto_reference::enumerate::segments::SEGMENTED_RESTRICT_ALLOCATION_BASE;
    use panproto_reference::segment as reference;
    use proptest::prelude::*;

    const ANCHORS: [&str; 3] = ["a", "b", "c"];

    fn schema() -> RefSchema {
        let mut schema = RefSchema::default();
        for anchor in ANCHORS {
            schema = schema.vertex(anchor, "record");
        }
        for src in ANCHORS {
            for tgt in ANCHORS {
                let name = format!("{src}{tgt}");
                schema = schema.edge(src, tgt, "prop", Some(&name));
            }
        }
        schema
    }

    /// A generated case: the schema, migration, plan, graph and partition,
    /// or `None` for a plan outside the compile domain.
    fn generated(
        anchors: &[usize],
        edges: &[(usize, usize)],
        owners: &[u32],
        segments: u32,
        surviving: u8,
    ) -> Option<(
        RefSchema,
        RefMigration,
        RestrictPlan,
        RefGraph,
        reference::Partition,
    )> {
        let schema = schema();
        let survivors: Vec<&str> = ANCHORS
            .iter()
            .enumerate()
            .filter(|(i, _)| surviving & (1 << i) != 0)
            .map(|(_, a)| *a)
            .collect();
        let migration = RefMigration {
            surviving_verts: survivors.iter().map(|a| (*a).to_owned()).collect(),
            surviving_edges: schema
                .edges
                .iter()
                .filter(|e| {
                    survivors.contains(&e.src.as_str()) && survivors.contains(&e.tgt.as_str())
                })
                .cloned()
                .collect(),
            ..RefMigration::default()
        };
        let production_schema = prod_schema(&schema);
        let plan = RestrictPlan::compile(
            &production_schema,
            &production_schema,
            &prod_migration(&migration),
        )
        .ok()?;
        let n = anchors.len();
        let mut graph = RefGraph::default();
        for (id, anchor) in (0_u32..).zip(anchors) {
            graph.nodes.insert(id, RefNode::new(id, ANCHORS[*anchor]));
        }
        for (id, (src, tgt)) in (0_u32..).zip(edges.iter().filter(|(s, t)| *s < n && *t < n)) {
            let (a, b) = (ANCHORS[anchors[*src]], ANCHORS[anchors[*tgt]]);
            let name = format!("{a}{b}");
            graph.edges.insert(
                id,
                RefGEdge {
                    src: u32::try_from(*src).unwrap_or(0),
                    tgt: u32::try_from(*tgt).unwrap_or(0),
                    label: RefEdge::new(a, b, "prop", Some(&name)),
                    position: None,
                },
            );
        }
        let partition = reference::Partition {
            shape: reference::InstanceShapeKind::Graph,
            specs: (0..segments)
                .map(|segment| {
                    let lo = SEGMENTED_RESTRICT_ALLOCATION_BASE + 10 * segment;
                    let range = reference::IdRange { lo, hi: lo + 9 };
                    let mut nodes: Vec<reference::IdRange> = (0_u32..)
                        .zip(owners)
                        .take(n)
                        .filter(|(_, owner)| **owner % segments == segment)
                        .map(|(id, _)| reference::IdRange { lo: id, hi: id })
                        .collect();
                    nodes.push(range);
                    reference::SegmentSpec {
                        id: reference::SegmentId(segment),
                        nodes,
                        alloc_nodes: Some(range),
                        alloc_edges: Some(range),
                    }
                })
                .collect(),
        };
        Some((schema, migration, plan, graph, partition))
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn segmented_restrict_matches_the_reference_on_generated_partitions(
            anchors in proptest::collection::vec(0_usize..3, 2..=8),
            edges in proptest::collection::vec((0_usize..8, 0_usize..8), 0..=14),
            owners in proptest::collection::vec(0_u32..4, 8),
            segments in 2_u32..=4,
            surviving in 0_u8..8,
        ) {
            let Some((schema, migration, plan, graph, partition)) =
                generated(&anchors, &edges, &owners, segments, surviving)
            else {
                return Err(TestCaseError::reject("plan outside the compile domain"));
            };
            let outcome = check_case(&schema, &schema, &migration, &plan, &graph, &partition);
            prop_assert!(matches!(outcome, Outcome::Agreed | Outcome::Refused));
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn a_checkpointed_restrict_run_matches_the_reference_runner_on_generated_partitions(
            anchors in proptest::collection::vec(0_usize..3, 2..=8),
            edges in proptest::collection::vec((0_usize..8, 0_usize..8), 0..=14),
            owners in proptest::collection::vec(0_u32..4, 8),
            segments in 2_u32..=4,
            surviving in 0_u8..8,
        ) {
            let Some((schema, migration, plan, graph, partition)) =
                generated(&anchors, &edges, &owners, segments, surviving)
            else {
                return Err(TestCaseError::reject("plan outside the compile domain"));
            };
            let mut counts = super::segmented_runner::Counts::default();
            super::segmented_runner::check_case(
                &super::segmented_runner::Case {
                    source: &schema,
                    target: &schema,
                    migration: &migration,
                    plan: &plan,
                    graph: &graph,
                    partition: &partition,
                },
                panproto_inst::segment::Budget::unlimited(),
                &mut counts,
            );
            prop_assert_eq!(counts.complete + counts.refused + counts.residual, 1);
        }
    }
}
