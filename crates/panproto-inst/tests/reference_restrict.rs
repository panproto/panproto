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
        BTreeMap, BTreeSet, Edge, FieldPresence, HashMap, IdFloor, NoValueLayer, Node, RefGEdge,
        RefGraph, RefMigration, RefSchema, RestrictPlan, SynthKey, Value, graph_restrict_with_plan,
        prod_graph, prod_migration, prod_schema, production_error, ref_complement, ref_edge,
        ref_graph, ref_node, ref_synth_key, reference_error,
    };
    use panproto_inst::InstanceShapeKind;
    use panproto_inst::segment::{
        GraphComplementSegment, GraphSegment, IdRange, Partition, SegmentError, SegmentId,
        SegmentSpec, SegmentedRestrict, SegmentedRestrictError, assemble_graph,
        graph_restrict_segmented, graph_restrict_segmented_held, partition_graph,
    };
    use panproto_inst::{EdgeId, GEdge};
    use panproto_reference::enumerate::segments::{
        SEGMENTED_RESTRICT_ALLOCATION_BASE, segmented_restrict_case_count, segmented_restrict_cases,
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

    /// A production segment refusal as the reference writes it, where an
    /// edge id is a bare `u32`.
    fn production_segment_error(error: &SegmentError) -> String {
        match error {
            SegmentError::EdgeIdOutsideAllocation { edge, segment } => format!(
                "EdgeIdOutsideAllocation {{ edge: {}, segment: {segment:?} }}",
                edge.0
            ),
            other => format!("{other:?}"),
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
        check_case_held((source, target, migration), plan, graph, partition, &[]).0
    }

    /// The persisting keys of a stably numbered output: how many kept the
    /// id their segment's held complement records, and how many took
    /// another because the id is outside the range or the segment holds it.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Persisting {
        /// Keys that kept their held id.
        pub kept: u64,
        /// Keys whose held id the segment's source holds or its range
        /// excludes.
        pub renumbered: u64,
    }

    /// Check that every synthesis key of `output` that segment `i`'s held
    /// complement records keeps the least id recorded for it when that id
    /// lies in the segment's range, is not `u32::MAX` and is no id of its
    /// kind the source segment holds; count the persisting keys.
    pub fn persisting_ids(
        output: &SegmentedRestrict,
        segments: &[GraphSegment],
        partition: &Partition,
        held: &[GraphComplementSegment],
    ) -> Persisting {
        let mut seen = Persisting::default();
        for (position, (complement, held)) in output.complements.iter().zip(held).enumerate() {
            let spec = &partition.specs()[position];
            let source = &segments[position];
            let least = |ids: Vec<(u32, &SynthKey)>, key: &SynthKey| {
                ids.into_iter()
                    .find_map(|(id, held_key)| (held_key == key).then_some(id))
            };
            let mut check = |id: u32, held_id: Option<u32>, range: Option<IdRange>, owned: bool| {
                let Some(held_id) = held_id else { return };
                let keeps = range.is_some_and(|r| r.lo <= held_id && held_id <= r.hi)
                    && held_id != u32::MAX
                    && !owned;
                if keeps {
                    assert_eq!(id, held_id, "a persisting key keeps its held id");
                    seen.kept += 1;
                } else {
                    seen.renumbered += 1;
                }
            };
            for (id, key) in &complement.synthesized_nodes {
                let held_id = least(
                    held.synthesized_nodes
                        .iter()
                        .map(|(id, key)| (*id, key))
                        .collect(),
                    key,
                );
                let owned = held_id.is_some_and(|h| source.nodes.contains_key(&h));
                check(*id, held_id, spec.alloc_nodes, owned);
            }
            for (id, key) in &complement.synthesized_edges {
                let held_id = least(
                    held.synthesized_edges
                        .iter()
                        .map(|(id, key)| (id.0, key))
                        .collect(),
                    key,
                );
                let owned = held_id.is_some_and(|h| source.edges.contains_key(&EdgeId(h)));
                check(id.0, held_id, spec.alloc_edges, owned);
            }
        }
        seen
    }

    /// [`check_case`] numbering stably against `held`, the production
    /// complement segments of an earlier restriction: production and
    /// reference agree segment by segment, the output assembles to
    /// whole-state restrict, and every persisting key keeps its held id
    /// where the rule allows ([`persisting_ids`]). Returns the outcome and,
    /// when both sides agree on an output, the production output and its
    /// persisting keys.
    pub fn check_case_held(
        (source, target, migration): (&RefSchema, &RefSchema, &RefMigration),
        plan: &RestrictPlan,
        graph: &RefGraph,
        partition: &reference::Partition,
        held: &[GraphComplementSegment],
    ) -> (Outcome, Option<(SegmentedRestrict, Persisting)>) {
        let production_partition = prod_partition(partition);
        let whole = prod_graph(graph);
        let segments = partition_graph(&whole, &production_partition).expect("production split");
        let reference_segments =
            reference::partition_graph(graph, partition).expect("reference split");
        let production =
            graph_restrict_segmented_held(plan, &segments, &production_partition, held);
        let reference_held: Vec<reference::GraphComplementSegment> =
            held.iter().map(ref_complement_segment).collect();
        let expected = reference::restrict_graph_segmented_held(
            source,
            target,
            migration,
            &NoValueLayer,
            &reference_segments,
            partition,
            &reference_held,
        );
        let outcome = match (production, expected) {
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
                let persisting = persisting_ids(&output, &segments, &production_partition, held);
                return (Outcome::Agreed, Some((output, persisting)));
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
                    production_segment_error(&error),
                    format!("{expected:?}"),
                    "segment refusal of {graph:?}"
                );
                Outcome::Refused
            }
            (production, expected) => panic!(
                "segmented restrict disagrees on {graph:?} under {partition:?}: {production:?} / {expected:?}"
            ),
        };
        (outcome, None)
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

    /// Every survival classification of `schema` without a conditional
    /// vertex, with its compiled plan.
    fn plans_of(schema: &RefSchema) -> Vec<(RefMigration, RestrictPlan)> {
        use panproto_reference::enumerate::restrict_and_contraction::survival_classifications;
        let compiled = prod_schema(schema);
        survival_classifications(schema)
            .into_iter()
            .filter(|migration| migration.conditional.is_empty())
            .map(|migration| {
                let plan = RestrictPlan::compile(&compiled, &compiled, &prod_migration(&migration))
                    .expect("release plans compile");
                (migration, plan)
            })
            .collect()
    }

    /// Every 97th case of the carrier whose segmented restrict completes:
    /// its view family, holding the edge ids its run allocated, is the
    /// source of a second segmented restrict under the same partition along
    /// every survival classification of the schema, which agrees with the
    /// reference and assembles to whole-state restrict of the glued view.
    #[test]
    fn a_segmented_restrict_of_a_segmented_view_agrees_with_the_reference_and_assembles_to_whole_state_restrict()
     {
        let mut plans: Option<(RefSchema, Vec<(RefMigration, RestrictPlan)>)> = None;
        let (mut first, mut agreed, mut refused, mut allocated) = (0_u64, 0_u64, 0_u64, 0_u64);
        for case in segmented_restrict_cases().step_by(97) {
            if !matches!(&plans, Some((schema, _)) if *schema == case.schema) {
                plans = Some((case.schema.clone(), plans_of(&case.schema)));
            }
            let Some((_, plans)) = &plans else {
                unreachable!("the plans were just compiled")
            };
            let partition = prod_partition(&case.partition);
            let segments =
                partition_graph(&prod_graph(&case.graph), &partition).expect("production split");
            let Some((_, plan)) = plans.iter().find(|(m, _)| *m == case.migration) else {
                unreachable!("the case's migration is a survival classification")
            };
            let Ok(output) = graph_restrict_segmented(plan, &segments, &partition) else {
                continue;
            };
            first += 1;
            let glued = assemble_graph(output.views.clone(), &partition).expect("the views glue");
            if glued
                .edges()
                .keys()
                .any(|id| id.0 >= SEGMENTED_RESTRICT_ALLOCATION_BASE)
            {
                allocated += 1;
            }
            let glued = ref_graph(&glued);
            for (migration, second) in plans {
                match check_case(
                    &case.schema,
                    &case.schema,
                    migration,
                    second,
                    &glued,
                    &case.partition,
                ) {
                    Outcome::Agreed => agreed += 1,
                    Outcome::Refused => refused += 1,
                }
            }
        }
        // A release schema has three vertices and no conditional one, so
        // eight survival classifications.
        assert_eq!(agreed + refused, 8 * first);
        assert!(
            first > 0 && allocated > 0 && agreed > 0,
            "first runs {first}, with an allocated edge id {allocated}, second runs agreed {agreed}, refused {refused}"
        );
    }

    /// Decode `bytes` in production and in the reference: both accept, the
    /// production complement re-encodes to `bytes` under `segment`, and the
    /// two decoded complements are equal.
    pub fn decodes_as_the_reference(bytes: &[u8], segment: SegmentId) -> GraphComplementSegment {
        let decoded = GraphComplementSegment::from_canonical_bytes(bytes, segment)
            .unwrap_or_else(|error| panic!("production decodes {bytes:?}: {error}"));
        assert_eq!(decoded.segment, segment);
        assert_eq!(
            decoded.canonical_bytes().expect("decoded bytes"),
            bytes,
            "round trip"
        );
        let reference::AnyComplementSegment::Graph(expected) =
            reference::decode_complement_segment_bytes(bytes).expect("reference decodes")
        else {
            panic!("the reference decodes a graph complement")
        };
        assert_eq!(ref_complement_segment(&decoded), expected);
        decoded
    }

    /// Both decoders refuse `bytes`, and the production refusal is a decode
    /// error of the complement segment artifact.
    pub fn both_refuse(bytes: &[u8], what: &str) {
        match GraphComplementSegment::from_canonical_bytes(bytes, SegmentId(0)) {
            Err(SegmentError::Decode {
                artifact: panproto_inst::segment::ArtifactKind::ComplementSegment,
                ..
            }) => {}
            other => panic!("production accepts {what}: {other:?}"),
        }
        assert!(
            reference::decode_complement_segment_bytes(bytes).is_err(),
            "the reference accepts {what}"
        );
    }

    /// Every proper prefix of `bytes`, and `bytes` followed by each byte
    /// value, is refused by both decoders.
    pub fn prefixes_and_trailing_bytes_are_refused(bytes: &[u8]) {
        for end in 0..bytes.len() {
            both_refuse(&bytes[..end], "a proper prefix");
        }
        for extra in [0_u8, 1, 2, 255] {
            let mut longer = bytes.to_vec();
            longer.push(extra);
            both_refuse(&longer, "a trailing byte");
        }
    }

    /// The index of the first byte at which `a` and `b` differ.
    fn first_difference(a: &[u8], b: &[u8]) -> usize {
        a.iter()
            .zip(b)
            .position(|(x, y)| x != y)
            .expect("the encodings differ")
    }

    /// A complement segment holding an entry of every kind: a dropped node
    /// with a value and fields, a dropped and an original edge, an original
    /// anchor, original fields, every value presence, and a synthesized key
    /// of each of the three kinds graph restrict synthesizes.
    fn every_entry_kind() -> GraphComplementSegment {
        let label = |name: Option<&str>| Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: name.map(Into::into),
        };
        let mut dropped = Node::new(3, "a");
        dropped.value = Some(FieldPresence::Present(Value::Int(-7)));
        dropped.extra_fields = HashMap::from([
            ("x".to_owned(), Value::Str("é".to_owned())),
            ("y".to_owned(), Value::Bool(true)),
        ]);
        dropped.position = Some(2);
        GraphComplementSegment {
            segment: SegmentId(1),
            dropped_nodes: BTreeMap::from([(3, dropped)]),
            dropped_edges: BTreeMap::from([(
                EdgeId(4),
                GEdge {
                    src: 3,
                    tgt: 9,
                    edge: label(Some("f")),
                    position: Some(0),
                },
            )]),
            original_edges: BTreeMap::from([(
                EdgeId(5),
                GEdge {
                    src: 7,
                    tgt: 8,
                    edge: label(None),
                    position: None,
                },
            )]),
            original_anchors: BTreeMap::from([(7, "b".into())]),
            original_extra_fields: BTreeMap::from([
                (7, BTreeMap::new()),
                (8, BTreeMap::from([("z".to_owned(), Value::Float(0.5))])),
            ]),
            original_values: BTreeMap::from([
                (7, None),
                (8, Some(FieldPresence::Present(Value::Str("v".to_owned())))),
                (10, Some(FieldPresence::Null)),
                (11, Some(FieldPresence::Absent)),
            ]),
            synthesized_nodes: BTreeMap::from([(
                20,
                SynthKey::ContractExpand {
                    src: 7,
                    tgt: 8,
                    hop: 1,
                },
            )]),
            synthesized_edges: BTreeMap::from([
                (
                    EdgeId(30),
                    SynthKey::Contract {
                        src: 7,
                        tgt: 8,
                        edge: label(Some("g")),
                    },
                ),
                (
                    EdgeId(31),
                    SynthKey::Expand {
                        source_edge: EdgeId(5),
                        hop: 258,
                    },
                ),
            ]),
            floors: IdFloor { node: 21, edge: 32 },
        }
    }

    /// The complement segment of every segment of every case of the carrier
    /// whose segmented restrict completes decodes from its canonical bytes
    /// to itself, as the reference decodes the same bytes; every proper
    /// prefix and every extension by one byte of each distinct encoding is
    /// refused by both decoders.
    #[test]
    fn a_complement_segment_decodes_from_its_canonical_bytes_as_the_reference_decodes_them() {
        let mut plan: Option<(RefSchema, RefMigration, RestrictPlan)> = None;
        let mut distinct = BTreeSet::new();
        let (mut decoded, mut expected, mut synthesized) = (0_u64, 0_u64, 0_u64);
        for case in segmented_restrict_cases() {
            if !matches!(&plan, Some((s, m, _)) if *s == case.schema && *m == case.migration) {
                let schema = prod_schema(&case.schema);
                let compiled =
                    RestrictPlan::compile(&schema, &schema, &prod_migration(&case.migration))
                        .expect("release plans compile");
                plan = Some((case.schema.clone(), case.migration.clone(), compiled));
            }
            let Some((_, _, compiled)) = &plan else {
                unreachable!("a plan was just compiled")
            };
            let partition = prod_partition(&case.partition);
            let segments =
                partition_graph(&prod_graph(&case.graph), &partition).expect("production split");
            let Ok(output) = graph_restrict_segmented(compiled, &segments, &partition) else {
                continue;
            };
            expected += u64::try_from(output.complements.len()).expect("segment count");
            for complement in &output.complements {
                let bytes = complement.canonical_bytes().expect("complement bytes");
                let back = decodes_as_the_reference(&bytes, complement.segment);
                assert_eq!(
                    ref_complement_segment(&back),
                    ref_complement_segment(complement)
                );
                if !complement.synthesized_edges.is_empty() {
                    synthesized += 1;
                }
                decoded += 1;
                distinct.insert(bytes);
            }
        }
        assert_eq!(decoded, expected);
        assert!(
            synthesized > 0 && distinct.len() > 1,
            "decoded {decoded}, synthesizing {synthesized}, distinct {}",
            distinct.len()
        );
        for bytes in &distinct {
            prefixes_and_trailing_bytes_are_refused(bytes);
        }
    }

    /// A complement segment with an entry of every kind round-trips through
    /// both decoders, and both refuse its proper prefixes, its extensions,
    /// another version, every shape tag but the graph's, every unknown value
    /// presence tag, every unknown synthesized-key tag, and a map whose keys
    /// do not strictly ascend.
    #[test]
    fn a_complement_segment_with_every_entry_kind_decodes_and_malformed_tags_are_refused() {
        let witness = every_entry_kind();
        let bytes = witness.canonical_bytes().expect("witness bytes");
        let back = decodes_as_the_reference(&bytes, witness.segment);
        assert_eq!(
            ref_complement_segment(&back),
            ref_complement_segment(&witness)
        );
        prefixes_and_trailing_bytes_are_refused(&bytes);

        let with = |edit: &dyn Fn(&mut GraphComplementSegment)| {
            let mut value = witness.clone();
            edit(&mut value);
            value.canonical_bytes().expect("edited bytes")
        };
        let mut other_version = bytes.clone();
        other_version[0] ^= 1;
        both_refuse(&other_version, "another format version");
        for shape in (0_u8..=255).filter(|shape| *shape != 2) {
            let mut other = bytes.clone();
            other[2] = shape;
            both_refuse(&other, "another shape tag");
        }
        let null = with(&|value| {
            value.original_values.insert(7, Some(FieldPresence::Null));
        });
        let presence = first_difference(&bytes, &null);
        assert_eq!((bytes[presence], null[presence]), (0, 2));
        for tag in 4_u8..=255 {
            let mut other = bytes.clone();
            other[presence] = tag;
            both_refuse(&other, "an unknown value presence tag");
        }
        let expand = with(&|value| {
            value.synthesized_nodes.insert(
                20,
                SynthKey::Expand {
                    source_edge: EdgeId(7),
                    hop: 1,
                },
            );
        });
        let key = first_difference(&bytes, &expand);
        assert_eq!((bytes[key], expand[key]), (2, 1));
        for tag in 3_u8..=255 {
            let mut other = bytes.clone();
            other[key] = tag;
            both_refuse(&other, "an unknown synthesized-key tag");
        }
        // The two synthesized edges are written as `30` and then `31`;
        // swapping their ids leaves the keys descending.
        let thirty = 30_u32.to_le_bytes();
        let at = bytes
            .windows(4)
            .rposition(|window| window == thirty)
            .expect("the first synthesized edge id");
        let mut descending = bytes;
        descending[at] = 32;
        both_refuse(&descending, "synthesized edge ids that do not ascend");
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
        SegmentManifest, SegmentRecord, SegmentRunReport, SegmentSource, SegmentStore,
        SegmentedRestrictError, TaskKey, graph_restrict_run, partition_graph, read_emitted,
        seal_full,
    };
    use panproto_reference::segment as reference;
    use std::collections::BTreeMap;

    /// The schema digest of a run's source segments.
    pub const SOURCE_SCHEMA: [u8; 32] = [1; 32];
    /// The schema digest of a run's view segments.
    pub const VIEW_SCHEMA: [u8; 32] = [2; 32];
    /// The lens identity of a run.
    pub const LENS: [u8; 32] = [3; 32];

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

    /// `seen` with its computed and reused task counts replaced by their
    /// sums, overall and in each phase: what carrying tasks over from an
    /// earlier run leaves unchanged.
    pub fn counts_summed(seen: Seen) -> Seen {
        Seen {
            tasks: (seen.tasks.0 + seen.tasks.1, 0),
            phases: seen
                .phases
                .into_iter()
                .map(|(phase, tasks, reused, peak, wall)| (phase, tasks + reused, 0, peak, wall))
                .collect(),
            ..seen
        }
    }

    /// The production report as both implementations project it.
    pub fn seen_production(report: &SegmentRunReport<SegmentedRestrictError>) -> Seen {
        let status = match &report.status {
            RunStatus::Failed(SegmentedRestrictError::Restrict { segment, error }) => {
                format!("restrict {} {}", segment.0, production_error(error))
            }
            RunStatus::Failed(SegmentedRestrictError::Segment(error)) => {
                format!("segment {error:?}")
            }
            RunStatus::Failed(other) => format!("other {other:?}"),
            _ => String::new(),
        };
        production_report(report, status)
    }

    /// The production report `report` as both implementations project it,
    /// a failed status written as `failed`.
    pub fn production_report<E>(report: &SegmentRunReport<E>, failed: String) -> Seen {
        let status = match &report.status {
            RunStatus::Complete { manifest } => format!("complete {:?}", manifest.0),
            RunStatus::Incomplete { reason } => format!("incomplete {reason:?}"),
            RunStatus::Residual(residual) => format!("residual {residual:?}"),
            RunStatus::Failed(_) => failed,
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

    /// The reference report as both implementations project it.
    pub fn seen_reference(
        report: &reference::SegmentRunReport<reference::SegmentedRestrictRefusal>,
    ) -> Seen {
        let status = match &report.status {
            reference::RunStatus::Failed(reference::SegmentedRestrictRefusal::Restrict {
                segment,
                error,
            }) => format!("restrict {} {}", segment.0, reference_error(error)),
            reference::RunStatus::Failed(reference::SegmentedRestrictRefusal::Segment(error)) => {
                format!("segment {error:?}")
            }
            reference::RunStatus::Failed(other) => format!("other {other:?}"),
            _ => String::new(),
        };
        reference_report(report, status, format!("{:?}", report.locality.get))
    }

    /// The reference report `report` as both implementations project it, a
    /// failed status written as `failed` and its locality as `locality`.
    pub fn reference_report<E>(
        report: &reference::SegmentRunReport<E>,
        failed: String,
        locality: String,
    ) -> Seen {
        let status = match &report.status {
            reference::RunStatus::Complete { manifest } => format!("complete {:?}", manifest.0),
            reference::RunStatus::Incomplete { reason } => format!("incomplete {reason:?}"),
            reference::RunStatus::Residual(residual) => format!("residual {residual:?}"),
            reference::RunStatus::Failed(_) => failed,
        };
        Seen {
            status,
            locality,
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
        bound_input(graph, partition, None)
    }

    /// Seal the production segments of `graph` under `partition` with the
    /// manifest binding `binding`.
    pub fn bound_input(
        graph: &RefGraph,
        partition: &reference::Partition,
        binding: Option<Digest>,
    ) -> Input {
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
                binding,
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

    /// One production run of `plan` over `input`, reading its segments from
    /// `source` and carrying tasks over from the earlier run over the input
    /// manifest `reuse` names.
    pub fn carrying_run<Src: SegmentSource>(
        plan: &RestrictPlan,
        input: &Input,
        source: &Src,
        cancel: Option<u32>,
        reuse: Option<Digest>,
        store: &mut MemoryStore,
    ) -> GraphRestrictRun {
        graph_restrict_run(
            plan,
            &input.manifest,
            source,
            RunSpec {
                lens: Digest(LENS),
                input: input.manifest.digest(),
                output_schema: Digest(VIEW_SCHEMA),
                cancel_after_tasks: cancel,
                reuse,
            },
            Budget::unlimited(),
            store,
            &mut |_| {},
        )
    }

    /// One reference run of `case` over `segments` sealed with the binding
    /// `binding`, carrying tasks over from the earlier run over the input
    /// manifest `reuse` names.
    pub fn reference_carrying_run(
        case: &Case<'_>,
        segments: &[reference::GraphSegment],
        binding: Option<Digest>,
        cancel: Option<u32>,
        reuse: Option<Digest>,
        store: &mut reference::MemoryStore,
    ) -> reference::RestrictRun {
        reference::segmented_restrict_with_progress(
            reference::RestrictLens {
                src: case.source,
                tgt: case.target,
                m: case.migration,
                vl: &NoValueLayer,
                held: None,
            },
            segments,
            case.partition,
            reference::RestrictRunSpec {
                lens: reference::Digest(LENS),
                source_schema: reference::Digest(SOURCE_SCHEMA),
                view_schema: reference::Digest(VIEW_SCHEMA),
                binding: binding.map(|digest| reference::Digest(digest.0)),
                cancel_after_tasks: cancel,
                reuse: reuse.map(|digest| reference::Digest(digest.0)),
            },
            reference::Budget {
                working_set: u64::MAX,
                complement: None,
                summary: u64::MAX,
                affected_segments: u32::MAX,
            },
            store,
            &mut |_| {},
        )
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
                output_schema: Digest(VIEW_SCHEMA),
                cancel_after_tasks: cancel,
                reuse: None,
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
                held: None,
            },
            segments,
            case.partition,
            reference::RestrictRunSpec {
                lens: reference::Digest(LENS),
                source_schema: reference::Digest(SOURCE_SCHEMA),
                view_schema: reference::Digest(VIEW_SCHEMA),
                binding: None,
                cancel_after_tasks: cancel,
                reuse: None,
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
    pub fn hidden_path_fixture() -> (RefSchema, RefMigration, RefGraph) {
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

    pub fn compile(
        source: &RefSchema,
        target: &RefSchema,
        migration: &RefMigration,
    ) -> RestrictPlan {
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
    pub fn partition(
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
        // An edge id at or above the least edge range that lies outside the
        // range of the segment holding it: edge 3 leaves node 3, which
        // segment 1 owns, and the least range is segment 0's.
        let mut outside = partition(&owners, Some(10), Some(10));
        outside.specs[0].alloc_edges = Some(reference::IdRange { lo: 3, hi: 9 });
        assert_eq!(
            check_case(&schema, &schema, &migration, &plan, &graph, &outside),
            Outcome::Refused
        );
        let segments =
            partition_graph(&prod_graph(&graph), &prod_partition(&outside)).expect("split");
        assert!(matches!(
            graph_restrict_segmented(&plan, &segments, &prod_partition(&outside)),
            Err(SegmentedRestrictError::Segment(
                SegmentError::EdgeIdOutsideAllocation {
                    edge: panproto_inst::EdgeId(3),
                    segment: SegmentId(1)
                }
            ))
        ));
        // An edge range starting at an edge id the same segment holds is
        // admitted, and allocation proceeds above it.
        let mut below = partition(&owners, Some(10), Some(10));
        below.specs[1].alloc_edges = Some(reference::IdRange { lo: 4, hi: 9 });
        assert_eq!(
            check_case(&schema, &schema, &migration, &plan, &graph, &below),
            Outcome::Agreed
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
        // 52, the greatest node 5, the greatest edge 5 or 1, two absent
        // tree counters 2, two digests 64, the outgoing and incoming cross
        // maps 8 + 36 per peer each);
        // segment 0 adds one edge id 4 and its longest frame, an edge
        // (tag 1, id, endpoints 12, the label's three names 9 + 9 + 12,
        // its name 1 + 10, no position 1, after its length 8); segment 1
        // adds its longest frame, a node (tag 1, id 4, anchor 9, no value,
        // discriminator or position 3, no fields or annotations 16, plain
        // shape 1, after its length 8).
        let record = |max_edge: u64| 4 + 8 + 8 + 2 + 52 + 5 + max_edge + 2 + 64 + 8 + 8 + 36;
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
    pub fn keep_everything(schema: &RefSchema) -> RefMigration {
        RefMigration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.iter().cloned().collect(),
            ..RefMigration::default()
        }
    }

    /// A store whose `fail_at`th commit fails, leaving its artifact staged.
    pub struct FailingStore {
        /// The store the commits that succeed reach.
        pub inner: MemoryStore,
        /// The commits so far.
        pub commits: usize,
        /// The commit that fails, counting from one.
        pub fail_at: Option<usize>,
    }

    /// The failure of a [`FailingStore`] commit.
    #[derive(Debug)]
    pub struct CommitFailed;

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

    /// The view segments a complete run emitted, in segment order.
    fn emitted_views(
        store: &MemoryStore,
        run: Digest,
        input: &segmented_runner::Input,
    ) -> Vec<panproto_inst::segment::GraphSegment> {
        input
            .partition
            .specs()
            .iter()
            .map(|spec| {
                read_emitted(store, run, &input.partition, spec.id)
                    .expect("emitted segment")
                    .view
            })
            .collect()
    }

    /// A contracting run's sealed view manifest, with its view segments as
    /// the segment source, is the input of a second checkpointed run that
    /// drops the surviving target: the second run completes, emits the
    /// output of segmented restrict on the first run's views, and assembles
    /// to whole-state restrict of their glued graph.
    #[test]
    fn the_sealed_view_of_a_checkpointed_run_is_the_input_of_a_second_run() {
        use panproto_inst::graph_restrict_with_plan;
        use panproto_inst::segment::{SegmentId, assemble_graph};
        use std::collections::BTreeMap;
        let (schema, first_migration, graph) = hidden_path_fixture();
        let first_plan = compile(&schema, &schema, &first_migration);
        let second_migration = RefMigration {
            surviving_verts: std::iter::once("a".to_owned()).collect(),
            ..RefMigration::default()
        };
        let second_plan = compile(&schema, &schema, &second_migration);
        let split = partition(&[&[0, 3], &[1, 4], &[2]], Some(10), Some(10));
        let input = segmented_runner::input(&graph, &split);
        let mut store = MemoryStore::default();
        let first = segmented_runner::production_run(
            &first_plan,
            &input,
            Budget::unlimited(),
            None,
            &mut store,
        );
        let manifest = first.manifest.expect("the first run completes");
        let views = emitted_views(&store, first.run.expect("a run identity"), &input);
        assert!(
            views
                .iter()
                .flat_map(|view| view.edges.keys())
                .any(|id| id.0 >= SEGMENTED_RESTRICT_ALLOCATION_BASE),
            "the first view holds an allocated edge id"
        );
        let view_schema = Digest(segmented_runner::VIEW_SCHEMA);
        let source: BTreeMap<SegmentId, Vec<u8>> = views
            .iter()
            .map(|view| {
                (
                    view.segment,
                    AnySegment::Graph(view.clone())
                        .canonical_bytes(&view_schema, &input.partition)
                        .expect("view bytes"),
                )
            })
            .collect();
        let mut second_store = MemoryStore::default();
        let second = graph_restrict_run(
            &second_plan,
            &manifest,
            &source,
            RunSpec {
                lens: Digest([4; 32]),
                input: manifest.digest(),
                output_schema: Digest([5; 32]),
                cancel_after_tasks: None,
                reuse: None,
            },
            Budget::unlimited(),
            &mut second_store,
            &mut |_| {},
        );
        let second_manifest = second.manifest.expect("the second run completes");
        assert_eq!(
            verify_manifest(&second_manifest, second_manifest.digest()),
            Ok(())
        );
        let expected = graph_restrict_segmented(&second_plan, &views, &input.partition)
            .expect("segmented restrict of the first views");
        let second_run = second.run.expect("a run identity");
        for (spec, (view, complement)) in input
            .partition
            .specs()
            .iter()
            .zip(expected.views.iter().zip(&expected.complements))
        {
            let emitted = read_emitted(&second_store, second_run, &input.partition, spec.id)
                .expect("emitted segment");
            assert_eq!(
                super::segmented::ref_view(&emitted.view),
                super::segmented::ref_view(view)
            );
            assert_eq!(
                emitted.complement,
                complement.canonical_bytes().expect("complement bytes")
            );
        }
        let glued = assemble_graph(views, &input.partition).expect("the first views glue");
        let (view, complement) = expected
            .assemble(&input.partition)
            .expect("the second views assemble");
        let (whole_view, whole_complement) =
            graph_restrict_with_plan(&glued, &second_plan).expect("whole-state restrict accepts");
        assert_eq!(super::ref_graph(&view), super::ref_graph(&whole_view));
        assert_eq!(
            super::ref_complement(&complement),
            super::ref_complement(&whole_complement)
        );
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
            output_schema: Digest([2; 32]),
            cancel_after_tasks: None,
            reuse: None,
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
            assert_eq!(resumed.report.orphans, Vec::<ArtifactKey>::new());
            assert_eq!(resumed.manifest, baseline.manifest);
            assert_eq!(
                format!("{:?}", resumed.report.status),
                format!("{:?}", baseline.report.status)
            );
        }
    }
}

/// Carrying tasks over from an earlier run over another input manifest:
/// every 97th case of the segmented restrict carrier, against an earlier
/// run over the same segments under another binding, complete and stopped
/// at every cancellation point, and against an earlier run over the family
/// without each one of its edges.
mod segmented_carry {
    use super::segmented_runner::{
        self, Case, Input, carrying_run, counts_summed, reference_carrying_run, seen_production,
        seen_reference,
    };
    use super::segmented_witnesses::{
        FailingStore, compile, hidden_path_fixture, keep_everything, partition,
    };
    use super::{RefGraph, RefMigration, RefSchema, RestrictPlan, prod_migration, prod_schema};
    use panproto_inst::identity::Digest;
    use panproto_inst::segment::{
        ArtifactKey, ArtifactKind, GraphRestrictRun, MemoryStore, MissingSegment, Phase, RunStatus,
        SegmentId, SegmentSource, SegmentStore, TaskKey,
    };
    use panproto_reference::segment as reference;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    /// The binding of the earlier manifest over the same segments.
    const BINDING: [u8; 32] = [9; 32];

    /// A source that records the segments read from it.
    pub struct CountingSource {
        bytes: BTreeMap<SegmentId, Vec<u8>>,
        /// The segments read, in order.
        pub reads: RefCell<Vec<SegmentId>>,
    }

    impl CountingSource {
        /// A source serving `bytes` that has served nothing yet.
        pub fn new(bytes: &BTreeMap<SegmentId, Vec<u8>>) -> Self {
            Self {
                bytes: bytes.clone(),
                reads: RefCell::new(Vec::new()),
            }
        }
    }

    impl SegmentSource for CountingSource {
        type Error = MissingSegment;

        fn read(&self, segment: SegmentId) -> Result<Vec<u8>, Self::Error> {
            self.reads.borrow_mut().push(segment);
            self.bytes.read(segment)
        }

        fn byte_len(&self, segment: SegmentId) -> Result<u64, Self::Error> {
            self.reads.borrow_mut().push(segment);
            self.bytes.byte_len(segment)
        }
    }

    /// What the pairs of one certificate showed.
    #[derive(Debug, Default)]
    struct Counts {
        /// Cases checked.
        cases: u64,
        /// Pairs of an earlier and a later run checked.
        pairs: u64,
        /// Pairs whose later run carried every task but Seal.
        carried_all: u64,
        /// Pairs whose later run carried some tasks and computed others
        /// besides Seal.
        carried_some: u64,
        /// Pairs whose later run carried nothing.
        carried_none: u64,
    }

    /// The earlier run of a pair: its graph, binding and cancellation point.
    struct Earlier<'a> {
        graph: &'a RefGraph,
        binding: Option<Digest>,
        cancel: Option<u32>,
    }

    /// Every committed task record and artifact of `run` in `store`.
    pub fn committed(store: &MemoryStore, run: &GraphRestrictRun) -> Vec<(TaskKey, Vec<u8>)> {
        let run = run.run.expect("a run identity");
        let journal = store.journal(&run).unwrap_or_else(|never| match never {});
        journal
            .tasks
            .iter()
            .map(|(task, record)| {
                let kind = match task.phase {
                    Phase::Validate => ArtifactKind::Segment,
                    Phase::Export => ArtifactKind::Export,
                    Phase::Summarize => ArtifactKind::Summary,
                    Phase::Assemble => ArtifactKind::Assembly,
                    Phase::Emit => ArtifactKind::ComplementSegment,
                    Phase::Seal => ArtifactKind::Manifest,
                };
                let bytes = store
                    .read(&ArtifactKey {
                        run,
                        task: *task,
                        kind,
                    })
                    .unwrap_or_else(|never| match never {})
                    .expect("a committed artifact");
                assert_eq!(
                    u64::try_from(bytes.len()).expect("length"),
                    record.bytes,
                    "{task}"
                );
                (*task, bytes)
            })
            .collect()
    }

    /// Run the earlier run, then the later run over `input` carrying tasks
    /// over from it, through production and reference, and check the law
    /// against the fresh run `fresh` committed in `fresh_store`. Returns the
    /// later run's carried count.
    fn check_pair(
        case: &Case<'_>,
        input: &Input,
        segments: &[reference::GraphSegment],
        (fresh, fresh_store): (&GraphRestrictRun, &MemoryStore),
        earlier: &Earlier<'_>,
    ) -> u32 {
        let earlier_input =
            segmented_runner::bound_input(earlier.graph, case.partition, earlier.binding);
        let earlier_segments = reference::partition_graph(earlier.graph, case.partition)
            .expect("reference split of the earlier graph");
        let context = format!(
            "{:?} under {:?} after {:?} stopped at {:?}",
            case.graph, case.partition, earlier.graph, earlier.cancel
        );
        let mut store = MemoryStore::default();
        let before = carrying_run(
            case.plan,
            &earlier_input,
            &earlier_input.bytes,
            earlier.cancel,
            None,
            &mut store,
        );
        let mut reference_store = reference::MemoryStore::default();
        let reference_before = reference_carrying_run(
            case,
            &earlier_segments,
            earlier.binding,
            earlier.cancel,
            None,
            &mut reference_store,
        );
        assert_eq!(
            before.run.map(|run| run.0),
            Some(reference_before.run.0),
            "earlier run identity of {context}"
        );
        let source = CountingSource::new(&input.bytes);
        let reuse = Some(earlier_input.manifest.digest());
        let later = carrying_run(case.plan, input, &source, None, reuse, &mut store);
        let expected =
            reference_carrying_run(case, segments, None, None, reuse, &mut reference_store);
        assert_eq!(
            seen_production(&later.report),
            seen_reference(&expected.report),
            "carrying run of {context}"
        );
        assert_eq!(
            counts_summed(seen_production(&later.report)),
            counts_summed(seen_production(&fresh.report)),
            "everything but the reuse counts of {context}"
        );
        assert_eq!(later.run, fresh.run, "{context}");
        assert_eq!(later.manifest, fresh.manifest, "{context}");
        assert_eq!(
            committed(&store, &later),
            committed(fresh_store, fresh),
            "committed artifacts of {context}"
        );
        assert_eq!(
            store.journal(&later.run.expect("a run identity")),
            fresh_store.journal(&fresh.run.expect("a run identity")),
            "journal of {context}"
        );
        let validated = |position: usize| {
            earlier
                .cancel
                .is_none_or(|cancel| position < usize::try_from(cancel).expect("count"))
        };
        let unread: Vec<SegmentId> = input
            .bytes
            .iter()
            .enumerate()
            .filter(|(position, (id, bytes))| {
                validated(*position) && earlier_input.bytes.get(id) == Some(bytes)
            })
            .map(|(_, (id, _))| *id)
            .collect();
        let read: Vec<SegmentId> = input
            .bytes
            .keys()
            .filter(|id| !unread.contains(id))
            .copied()
            .collect();
        assert_eq!(
            source.reads.borrow().clone(),
            read,
            "segments read by the carrying run of {context}"
        );
        later.report.tasks_reused
    }

    /// Check the carrying run of `case` after a complete earlier run over
    /// `earlier` under the same partition; returns the carried count.
    pub fn check_after(case: &Case<'_>, earlier: &RefGraph) -> u32 {
        let input = segmented_runner::input(case.graph, case.partition);
        let segments =
            reference::partition_graph(case.graph, case.partition).expect("reference split");
        let mut fresh_store = MemoryStore::default();
        let fresh = carrying_run(
            case.plan,
            &input,
            &input.bytes,
            None,
            None,
            &mut fresh_store,
        );
        check_pair(
            case,
            &input,
            &segments,
            (&fresh, &fresh_store),
            &Earlier {
                graph: earlier,
                binding: None,
                cancel: None,
            },
        )
    }

    /// Check one case against the earlier runs over the same segments under
    /// another binding and over its graph without each edge.
    fn check_carry(case: &Case<'_>, counts: &mut Counts) {
        let input = segmented_runner::input(case.graph, case.partition);
        let segments =
            reference::partition_graph(case.graph, case.partition).expect("reference split");
        let mut fresh_store = MemoryStore::default();
        let fresh = carrying_run(
            case.plan,
            &input,
            &input.bytes,
            None,
            None,
            &mut fresh_store,
        );
        assert_eq!(fresh.report.tasks_reused, 0);
        let total = fresh.report.progress.tasks_total;
        let done = fresh.report.progress.tasks_done;
        counts.cases += 1;
        let tally = |carried: u32, counts: &mut Counts| {
            counts.pairs += 1;
            if carried == 0 {
                counts.carried_none += 1;
            } else if carried == total - 1 {
                counts.carried_all += 1;
            } else {
                counts.carried_some += 1;
            }
        };
        for cancel in (0..=total).map(Some).chain([None]) {
            let earlier = Earlier {
                graph: case.graph,
                binding: Some(Digest(BINDING)),
                cancel,
            };
            let carried = check_pair(case, &input, &segments, (&fresh, &fresh_store), &earlier);
            let committed = cancel.unwrap_or(total).min(done);
            assert_eq!(
                carried,
                committed.min(total - 1),
                "the same segments carry every committed task but Seal, earlier run stopped at {cancel:?}"
            );
            tally(carried, counts);
        }
        for edge in case.graph.edges.keys() {
            let mut graph = case.graph.clone();
            graph.edges.remove(edge);
            let earlier = Earlier {
                graph: &graph,
                binding: None,
                cancel: None,
            };
            let carried = check_pair(case, &input, &segments, (&fresh, &fresh_store), &earlier);
            tally(carried, counts);
        }
    }

    /// Every `STRIDE`th case of the segmented restrict carrier.
    const STRIDE: usize = 97;

    #[test]
    fn carrying_tasks_over_changes_nothing_but_the_reuse_counts_on_every_97th_case() {
        use panproto_reference::enumerate::segments::segmented_restrict_cases;
        let mut counts = Counts::default();
        let mut plan: Option<(RefSchema, RefMigration, RestrictPlan)> = None;
        let mut expected_pairs = 0_u64;
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
            let segments = u64::try_from(case.partition.specs.len()).expect("count");
            let edges = u64::try_from(case.graph.edges.len()).expect("count");
            expected_pairs += (4 * segments + 3) + 1 + edges;
            check_carry(
                &Case {
                    source: &case.schema,
                    target: &case.schema,
                    migration: &case.migration,
                    plan: compiled,
                    graph: &case.graph,
                    partition: &case.partition,
                },
                &mut counts,
            );
        }
        assert_eq!(counts.cases, 128_240_u64.div_ceil(97));
        assert_eq!(counts.pairs, expected_pairs);
        assert_eq!(expected_pairs, 22_051);
        assert_eq!(
            (counts.carried_all, counts.carried_some, counts.carried_none),
            (3_489, 17_006, 1_556),
            "pairs carrying every task but Seal, some tasks, and none"
        );
    }

    #[test]
    fn a_carrying_run_stopped_by_a_failed_commit_at_each_task_resumes_to_the_uninterrupted_run() {
        let (schema, graph) = (hidden_path_fixture().0, hidden_path_fixture().2);
        let migration = keep_everything(&schema);
        let plan = compile(&schema, &schema, &migration);
        let split = partition(&[&[0, 3], &[1, 4], &[2]], Some(10), Some(10));
        let input = segmented_runner::input(&graph, &split);
        let earlier = segmented_runner::bound_input(&graph, &split, Some(Digest(BINDING)));
        let reuse = Some(earlier.manifest.digest());
        let mut baseline_store = MemoryStore::default();
        carrying_run(
            &plan,
            &earlier,
            &earlier.bytes,
            None,
            None,
            &mut baseline_store,
        );
        let baseline = carrying_run(
            &plan,
            &input,
            &input.bytes,
            None,
            reuse,
            &mut baseline_store,
        );
        assert!(
            matches!(baseline.report.status, RunStatus::Complete { .. }),
            "{:?}",
            baseline.report.status
        );
        let total = baseline.report.progress.tasks_total;
        assert_eq!(
            (baseline.report.tasks_run, baseline.report.tasks_reused),
            (1, total - 1),
            "every task but Seal is carried over"
        );
        for fail_at in 1..=total {
            let mut store = FailingStore {
                inner: MemoryStore::default(),
                commits: 0,
                fail_at: None,
            };
            graph_restrict(&plan, &earlier, None, &mut store);
            store.fail_at = Some(store.commits + usize::try_from(fail_at).expect("count"));
            let failed = graph_restrict(&plan, &input, reuse, &mut store);
            assert!(
                matches!(&failed.report.status, RunStatus::Incomplete { .. }),
                "{:?}",
                failed.report.status
            );
            assert_eq!(failed.report.progress.tasks_done, fail_at - 1);
            assert_eq!(failed.report.orphans.len(), 1, "the failed task's artifact");
            store.fail_at = None;
            let resumed = graph_restrict(&plan, &input, reuse, &mut store);
            assert_eq!(
                (resumed.report.tasks_run, resumed.report.tasks_reused),
                (1, total - 1),
                "a resumed carrying run after commit {fail_at}"
            );
            assert_eq!(resumed.report.orphans, Vec::<ArtifactKey>::new());
            assert_eq!(resumed.manifest, baseline.manifest);
            assert_eq!(
                format!("{:?}", resumed.report.status),
                format!("{:?}", baseline.report.status)
            );
        }
    }

    /// A run of `plan` over `input` in a failing store.
    fn graph_restrict(
        plan: &RestrictPlan,
        input: &Input,
        reuse: Option<Digest>,
        store: &mut FailingStore,
    ) -> GraphRestrictRun {
        use panproto_inst::segment::{Budget, RunSpec, graph_restrict_run};
        graph_restrict_run(
            plan,
            &input.manifest,
            &input.bytes,
            RunSpec {
                lens: Digest([3; 32]),
                input: input.manifest.digest(),
                output_schema: Digest([2; 32]),
                cancel_after_tasks: None,
                reuse,
            },
            Budget::unlimited(),
            store,
            &mut |_| {},
        )
    }
}

/// Stable numbering against held complement segments: segmented
/// restriction with the complements of an earlier restriction, in memory and
/// as a checkpointed run, against the reference.
mod segmented_held {
    use super::segmented::{
        Outcome, Persisting, check_case_held, prod_partition, ref_complement_segment, ref_view,
    };
    use super::segmented_carry::{CountingSource, committed};
    use super::segmented_runner::{
        self, Case, Input, LENS, SOURCE_SCHEMA, VIEW_SCHEMA, counts_summed, seen_production,
        seen_reference,
    };
    use super::segmented_witnesses::{FailingStore, compile, hidden_path_fixture, partition};
    use super::{
        NoValueLayer, RefGraph, RefMigration, RefSchema, RestrictPlan, prod_graph, prod_migration,
        prod_schema,
    };
    use panproto_inst::identity::Digest;
    use panproto_inst::segment::{
        ArtifactKey, Budget, GraphComplementSegment, GraphRestrict, GraphRestrictRun, MemoryStore,
        RunSpec, RunStatus, SegmentId, SegmentSource, SegmentStore, graph_restrict_segmented,
        graph_restrict_segmented_held, partition_graph, read_emitted, run_segmented,
    };
    use panproto_reference::segment as reference;
    use std::collections::BTreeMap;

    /// A checkpointed restriction of `plan` over `input` numbering stably
    /// against `held`, reading segments from `source`.
    fn held_run<Src: SegmentSource>(
        plan: &RestrictPlan,
        (input, source): (&Input, &Src),
        held: Option<&BTreeMap<SegmentId, Vec<u8>>>,
        (cancel, reuse): (Option<u32>, Option<Digest>),
        store: &mut impl SegmentStore,
    ) -> GraphRestrictRun {
        let held = held.map(|held| held as &dyn panproto_inst::segment::HeldComplements);
        run_segmented(
            &GraphRestrict { plan, held },
            &input.manifest,
            source,
            RunSpec {
                lens: Digest(LENS),
                input: input.manifest.digest(),
                output_schema: Digest(VIEW_SCHEMA),
                cancel_after_tasks: cancel,
                reuse,
            },
            Budget::unlimited(),
            store,
            &mut |_| {},
        )
    }

    /// The reference run of `case` over `segments` numbering stably against
    /// `held`.
    fn reference_held_run(
        case: &Case<'_>,
        segments: &[reference::GraphSegment],
        held: Option<&[reference::GraphComplementSegment]>,
        (cancel, reuse): (Option<u32>, Option<Digest>),
        store: &mut reference::MemoryStore,
    ) -> reference::RestrictRun {
        reference::segmented_restrict_with_progress(
            reference::RestrictLens {
                src: case.source,
                tgt: case.target,
                m: case.migration,
                vl: &NoValueLayer,
                held,
            },
            segments,
            case.partition,
            reference::RestrictRunSpec {
                lens: reference::Digest(LENS),
                source_schema: reference::Digest(SOURCE_SCHEMA),
                view_schema: reference::Digest(VIEW_SCHEMA),
                binding: None,
                cancel_after_tasks: cancel,
                reuse: reuse.map(|digest| reference::Digest(digest.0)),
            },
            reference::Budget {
                working_set: u64::MAX,
                complement: None,
                summary: u64::MAX,
                affected_segments: u32::MAX,
            },
            store,
            &mut |_| {},
        )
    }

    /// `graph` with an edge numbered at the lower bound of `spec`'s edge
    /// allocation range (the id the segment's first synthesized edge takes),
    /// from the least node the segment owns to the least node the schema
    /// gives it an edge to; `None` when the segment declares no edge range,
    /// owns no node, or the schema gives that node no edge.
    pub fn with_floor_edge(
        graph: &RefGraph,
        spec: &reference::SegmentSpec,
        schema: &RefSchema,
    ) -> Option<RefGraph> {
        let range = spec.alloc_edges?;
        let src = graph.nodes.values().find(|node| {
            spec.nodes
                .iter()
                .any(|r| r.lo <= node.id && node.id <= r.hi)
        })?;
        let (tgt, label) = graph.nodes.values().find_map(|tgt| {
            schema
                .edges
                .iter()
                .find(|edge| edge.src == src.anchor && edge.tgt == tgt.anchor)
                .map(|label| (tgt.id, label.clone()))
        })?;
        let mut later = graph.clone();
        later.edges.insert(
            range.lo,
            super::RefGEdge {
                src: src.id,
                tgt,
                label,
                position: None,
            },
        );
        Some(later)
    }

    /// The graphs a later restriction of `graph` reads against the held
    /// complements of a restriction of `graph` itself: `graph` without each
    /// edge, and `graph` with an edge at each segment's edge allocation
    /// floor ([`with_floor_edge`]).
    fn later_graphs(
        graph: &RefGraph,
        partition: &reference::Partition,
        schema: &RefSchema,
    ) -> Vec<RefGraph> {
        let mut graphs = Vec::new();
        for edge in graph.edges.keys() {
            let mut later = graph.clone();
            later.edges.remove(edge);
            graphs.push(later);
        }
        graphs.extend(
            partition
                .specs
                .iter()
                .filter_map(|spec| with_floor_edge(graph, spec, schema)),
        );
        graphs
    }

    /// The complement segments of a segmented restriction of `graph`
    /// numbered afresh, or `None` when it refuses.
    fn fresh_complements(
        plan: &RestrictPlan,
        graph: &RefGraph,
        partition: &reference::Partition,
    ) -> Option<Vec<GraphComplementSegment>> {
        let production = prod_partition(partition);
        let segments = partition_graph(&prod_graph(graph), &production).expect("production split");
        graph_restrict_segmented(plan, &segments, &production)
            .ok()
            .map(|output| output.complements)
    }

    /// The outcome of one in-memory check, with the agreed output.
    type Checked = (
        Outcome,
        Option<(panproto_inst::segment::SegmentedRestrict, Persisting)>,
    );

    /// What the in-memory certificate showed.
    #[derive(Debug, Default, PartialEq, Eq)]
    struct Counts {
        /// Cases checked.
        cases: u64,
        /// Cases whose first restriction completes.
        first: u64,
        /// Later restrictions against held complements.
        later: u64,
        /// Later restrictions both sides refuse alike.
        refused: u64,
        /// Persisting keys that kept and did not keep their held ids.
        persisting: Persisting,
        /// Later restrictions whose stable numbering differs from fresh
        /// numbering.
        differs: u64,
    }

    impl Counts {
        /// Tally one later restriction of `graph` against `held`.
        fn tally(
            &mut self,
            (outcome, output): Checked,
            fresh: Option<Vec<GraphComplementSegment>>,
        ) {
            self.later += 1;
            match (outcome, output) {
                (Outcome::Agreed, Some((output, persisting))) => {
                    self.persisting.kept += persisting.kept;
                    self.persisting.renumbered += persisting.renumbered;
                    let bytes = |complements: &[GraphComplementSegment]| -> Vec<Vec<u8>> {
                        complements
                            .iter()
                            .map(|c| c.canonical_bytes().expect("complement bytes"))
                            .collect()
                    };
                    let fresh = fresh.expect("fresh numbering accepts what stable numbering does");
                    if bytes(&output.complements) != bytes(&fresh) {
                        self.differs += 1;
                    }
                }
                (Outcome::Refused, None) => self.refused += 1,
                (outcome, _) => panic!("an agreed outcome carries its output: {outcome:?}"),
            }
        }
    }

    /// Every `STRIDE`th case of the segmented restrict carrier.
    const STRIDE: usize = 7;

    /// Every 7th case of the carrier whose restriction completes: restricted
    /// again against its own complements, the output is the first output
    /// byte for byte; restricted against them after removing each edge or
    /// adding an edge at each segment's edge allocation floor, and restricted
    /// against the complements of the graph without each edge, production
    /// agrees with the reference, assembles to whole-state restrict, and keeps
    /// every persisting key's held id where the rule allows.
    #[test]
    fn stable_numbering_agrees_with_the_reference_reproduces_an_unchanged_family_and_keeps_persisting_ids()
     {
        use panproto_reference::enumerate::segments::segmented_restrict_cases;
        let mut counts = Counts::default();
        let mut plan: Option<(RefSchema, RefMigration, RestrictPlan)> = None;
        for case in segmented_restrict_cases().step_by(STRIDE) {
            counts.cases += 1;
            if !matches!(&plan, Some((s, m, _)) if *s == case.schema && *m == case.migration) {
                let schema = prod_schema(&case.schema);
                let compiled =
                    RestrictPlan::compile(&schema, &schema, &prod_migration(&case.migration))
                        .expect("release plans compile");
                plan = Some((case.schema.clone(), case.migration.clone(), compiled));
            }
            let Some((_, _, compiled)) = &plan else {
                unreachable!("a plan was just compiled")
            };
            let Some(first) = fresh_complements(compiled, &case.graph, &case.partition) else {
                continue;
            };
            counts.first += 1;
            let schemas = (&case.schema, &case.schema, &case.migration);
            let production = prod_partition(&case.partition);
            let segments =
                partition_graph(&prod_graph(&case.graph), &production).expect("production split");
            let output = graph_restrict_segmented(compiled, &segments, &production)
                .expect("the first restriction completes");
            let (outcome, again) =
                check_case_held(schemas, compiled, &case.graph, &case.partition, &first);
            assert_eq!(outcome, Outcome::Agreed);
            let (again, persisting) = again.expect("an agreed output");
            assert_eq!(
                persisting.renumbered, 0,
                "an unchanged family renumbers nothing"
            );
            for (position, (view, complement)) in
                again.views.iter().zip(&again.complements).enumerate()
            {
                assert_eq!(ref_view(view), ref_view(&output.views[position]));
                assert_eq!(
                    complement.canonical_bytes().expect("bytes"),
                    output.complements[position]
                        .canonical_bytes()
                        .expect("bytes"),
                    "an unchanged family reproduces its complement"
                );
            }
            for later in later_graphs(&case.graph, &case.partition, &case.schema) {
                let fresh = fresh_complements(compiled, &later, &case.partition);
                counts.tally(
                    check_case_held(schemas, compiled, &later, &case.partition, &first),
                    fresh,
                );
            }
            for edge in case.graph.edges.keys() {
                let mut earlier = case.graph.clone();
                earlier.edges.remove(edge);
                let Some(held) = fresh_complements(compiled, &earlier, &case.partition) else {
                    continue;
                };
                let fresh = fresh_complements(compiled, &case.graph, &case.partition);
                counts.tally(
                    check_case_held(schemas, compiled, &case.graph, &case.partition, &held),
                    fresh,
                );
            }
        }
        assert_eq!(counts.cases, 128_240_u64.div_ceil(7));
        assert_eq!(
            counts,
            Counts {
                cases: 18_320,
                first: 16_280,
                later: 66_144,
                refused: 1_764,
                persisting: Persisting {
                    kept: 364,
                    renumbered: 216,
                },
                differs: 0,
            }
        );
    }

    /// The emitted complement bytes of every segment of a complete run.
    fn emitted_complements(
        store: &MemoryStore,
        run: &GraphRestrictRun,
        input: &Input,
    ) -> BTreeMap<SegmentId, Vec<u8>> {
        let run = run.run.expect("a run identity");
        input
            .partition
            .specs()
            .iter()
            .map(|spec| {
                let emitted =
                    read_emitted(store, run, &input.partition, spec.id).expect("emitted segment");
                (spec.id, emitted.complement)
            })
            .collect()
    }

    /// What the checkpointed certificate showed.
    #[derive(Debug, Default, PartialEq, Eq)]
    pub struct RunCounts {
        /// Cases checked.
        pub cases: u64,
        /// Cases whose first run completes.
        pub first: u64,
        /// Later runs against the first run's complements.
        pub later: u64,
        /// Later runs that complete.
        pub complete: u64,
        /// Cancellation points checked.
        pub cancellations: u64,
        /// Tasks the later runs carried over from the first.
        pub carried: u64,
    }

    /// A complete run over a case's graph in fresh stores, production and
    /// reference, with the complement segments it emitted.
    struct Held {
        input: Input,
        store: MemoryStore,
        reference_store: reference::MemoryStore,
        bytes: BTreeMap<SegmentId, Vec<u8>>,
        segments: Vec<GraphComplementSegment>,
        reference: Vec<reference::GraphComplementSegment>,
    }

    impl Held {
        /// Run `case` over its graph, holding nothing.
        fn of(case: &Case<'_>) -> Self {
            let earlier_input = segmented_runner::input(case.graph, case.partition);
            let earlier_segments =
                reference::partition_graph(case.graph, case.partition).expect("reference split");
            let mut store = MemoryStore::default();
            let earlier = segmented_runner::carrying_run(
                case.plan,
                &earlier_input,
                &earlier_input.bytes,
                None,
                None,
                &mut store,
            );
            let mut reference_store = reference::MemoryStore::default();
            reference_held_run(
                case,
                &earlier_segments,
                None,
                (None, None),
                &mut reference_store,
            );
            let held = emitted_complements(&store, &earlier, &earlier_input);
            let held_segments: Vec<GraphComplementSegment> = held
                .iter()
                .map(|(id, bytes)| {
                    GraphComplementSegment::from_canonical_bytes(bytes, *id).expect("held decodes")
                })
                .collect();
            let reference_held: Vec<reference::GraphComplementSegment> =
                held_segments.iter().map(ref_complement_segment).collect();
            Self {
                input: earlier_input,
                store,
                reference_store,
                bytes: held,
                segments: held_segments,
                reference: reference_held,
            }
        }
    }

    /// The later run of `case` over `later` holding the complements the
    /// earlier run over `case.graph` emitted, carrying tasks over from it:
    /// production and reference reports agree uninterrupted and from every
    /// cancellation point; the carrying run's journal, artifacts and manifest
    /// are those of a fresh run holding the same complements; it reads from
    /// its source the segments whose bytes changed; and a complete run emits
    /// the output of in-memory stable numbering.
    pub fn check_later_run(
        case: &Case<'_>,
        later: &RefGraph,
        cancellations: bool,
        counts: &mut RunCounts,
    ) {
        let Held {
            input: earlier_input,
            mut store,
            mut reference_store,
            bytes: held,
            segments: held_segments,
            reference: reference_held,
        } = Held::of(case);
        let later_case = Case {
            graph: later,
            ..*case
        };
        let input = segmented_runner::input(later, case.partition);
        let segments = reference::partition_graph(later, case.partition).expect("reference split");
        let context = format!(
            "{later:?} under {:?} after {:?}",
            case.partition, case.graph
        );
        let reuse = Some(earlier_input.manifest.digest());
        let source = CountingSource::new(&input.bytes);
        let carrying = held_run(
            case.plan,
            (&input, &source),
            Some(&held),
            (None, reuse),
            &mut store,
        );
        let expected = reference_held_run(
            &later_case,
            &segments,
            Some(&reference_held),
            (None, reuse),
            &mut reference_store,
        );
        assert_eq!(
            seen_production(&carrying.report),
            seen_reference(&expected.report),
            "carrying run of {context}"
        );
        counts.later += 1;
        counts.carried += u64::from(carrying.report.tasks_reused);
        let changed: Vec<SegmentId> = input
            .bytes
            .iter()
            .filter(|(id, bytes)| earlier_input.bytes.get(id) != Some(*bytes))
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(
            source.reads.borrow().clone(),
            changed,
            "segments read by {context}"
        );

        let mut fresh_store = MemoryStore::default();
        let fresh = held_run(
            case.plan,
            (&input, &input.bytes),
            Some(&held),
            (None, None),
            &mut fresh_store,
        );
        if later != case.graph {
            assert_eq!(
                counts_summed(seen_production(&carrying.report)),
                counts_summed(seen_production(&fresh.report)),
                "everything but the reuse counts of {context}"
            );
            assert_eq!(
                committed(&store, &carrying),
                committed(&fresh_store, &fresh),
                "{context}"
            );
            assert_eq!(
                store.journal(&carrying.run.expect("a run identity")),
                fresh_store.journal(&fresh.run.expect("a run identity")),
                "journal of {context}"
            );
        }
        assert_eq!(carrying.manifest, fresh.manifest, "{context}");
        if let RunStatus::Complete { .. } = fresh.report.status {
            counts.complete += 1;
            check_emitted(
                case,
                later,
                &held_segments,
                (&fresh_store, &fresh),
                &context,
            );
        }
        if cancellations {
            let held = (&held, reference_held.as_slice());
            check_cancellations(&later_case, (&input, &segments), held, &fresh, counts);
        }
    }

    /// The complete run `fresh` of `case.plan` over `later` holding
    /// `held_segments` emitted the in-memory stable numbering.
    fn check_emitted(
        case: &Case<'_>,
        later: &RefGraph,
        held_segments: &[GraphComplementSegment],
        (fresh_store, fresh): (&MemoryStore, &GraphRestrictRun),
        context: &str,
    ) {
        let production = prod_partition(case.partition);
        let source_segments =
            partition_graph(&prod_graph(later), &production).expect("production split");
        let output =
            graph_restrict_segmented_held(case.plan, &source_segments, &production, held_segments)
                .expect("in-memory stable numbering completes");
        let run = fresh.run.expect("a run identity");
        for (position, spec) in production.specs().iter().enumerate() {
            let emitted = read_emitted(fresh_store, run, &production, spec.id).expect("emitted");
            assert_eq!(ref_view(&emitted.view), ref_view(&output.views[position]));
            assert_eq!(
                emitted.complement,
                output.complements[position]
                    .canonical_bytes()
                    .expect("bytes"),
                "emitted complement of {context}"
            );
        }
    }

    /// The run of `case` over `input` holding `held`, stopped at every
    /// cancellation point and resumed, reports as the reference does and
    /// ends as the uninterrupted run `fresh`.
    fn check_cancellations(
        case: &Case<'_>,
        (input, segments): (&Input, &[reference::GraphSegment]),
        (held, reference_held): (
            &BTreeMap<SegmentId, Vec<u8>>,
            &[reference::GraphComplementSegment],
        ),
        fresh: &GraphRestrictRun,
        counts: &mut RunCounts,
    ) {
        let context = format!("{:?} under {:?}", case.graph, case.partition);
        let total = fresh.report.progress.tasks_total;
        for cancel in 0..=total {
            let mut stopped_store = MemoryStore::default();
            let stopped = held_run(
                case.plan,
                (input, &input.bytes),
                Some(held),
                (Some(cancel), None),
                &mut stopped_store,
            );
            let mut reference_stopped = reference::MemoryStore::default();
            let expected_stopped = reference_held_run(
                case,
                segments,
                Some(reference_held),
                (Some(cancel), None),
                &mut reference_stopped,
            );
            assert_eq!(
                seen_production(&stopped.report),
                seen_reference(&expected_stopped.report),
                "{context} stopped at {cancel}"
            );
            let resumed = held_run(
                case.plan,
                (input, &input.bytes),
                Some(held),
                (None, None),
                &mut stopped_store,
            );
            let expected_resumed = reference_held_run(
                case,
                segments,
                Some(reference_held),
                (None, None),
                &mut reference_stopped,
            );
            assert_eq!(
                seen_production(&resumed.report),
                seen_reference(&expected_resumed.report),
                "{context} resumed after {cancel}"
            );
            assert_eq!(
                resumed.manifest, fresh.manifest,
                "{context} resumed after {cancel}"
            );
            assert_eq!(resumed.report.orphans, Vec::<ArtifactKey>::new());
            counts.cancellations += 1;
        }
    }

    /// Every 97th case of the carrier whose checkpointed restriction
    /// completes: later runs over the same graph, the graph without each
    /// edge and the graph with an edge at each segment's allocation floor,
    /// holding the first run's emitted complements and carrying tasks over
    /// from it, agree with the reference runner, change nothing against a
    /// fresh run but the reuse counts, read only the changed segments, emit
    /// the in-memory stable numbering, and resume from every cancellation
    /// point (checked for the first changed graph of each case).
    #[test]
    fn a_checkpointed_restriction_holding_complements_matches_the_reference_runner_on_every_97th_case()
     {
        use panproto_reference::enumerate::segments::segmented_restrict_cases;
        let mut counts = RunCounts::default();
        let mut plan: Option<(RefSchema, RefMigration, RestrictPlan)> = None;
        for case in segmented_restrict_cases().step_by(97) {
            counts.cases += 1;
            if !matches!(&plan, Some((s, m, _)) if *s == case.schema && *m == case.migration) {
                let schema = prod_schema(&case.schema);
                let compiled =
                    RestrictPlan::compile(&schema, &schema, &prod_migration(&case.migration))
                        .expect("release plans compile");
                plan = Some((case.schema.clone(), case.migration.clone(), compiled));
            }
            let Some((_, _, compiled)) = &plan else {
                unreachable!("a plan was just compiled")
            };
            if fresh_complements(compiled, &case.graph, &case.partition).is_none() {
                continue;
            }
            counts.first += 1;
            let run_case = Case {
                source: &case.schema,
                target: &case.schema,
                migration: &case.migration,
                plan: compiled,
                graph: &case.graph,
                partition: &case.partition,
            };
            check_later_run(&run_case, &case.graph, false, &mut counts);
            for (position, later) in later_graphs(&case.graph, &case.partition, &case.schema)
                .iter()
                .enumerate()
            {
                check_later_run(&run_case, later, position == 0, &mut counts);
            }
        }
        assert_eq!(counts.cases, 128_240_u64.div_ceil(97));
        assert_eq!(
            counts,
            RunCounts {
                cases: 1_323,
                first: 1_163,
                later: 4_769,
                complete: 4_606,
                cancellations: 16_166,
                carried: 18_687,
            }
        );
    }

    /// Stable numbering keeps a persisting key's id and gives a new key the
    /// id a deleted chain freed, where fresh numbering shifts both: node 0
    /// contracts to the `b` nodes 4 and 5 through the dropped node 1, ids
    /// 100 and 101; after the path to 4 is cut and one to a new node 6
    /// added, the pair to 5 keeps 101 and the pair to 6 takes the freed 100,
    /// while fresh numbering gives them 100 and 101.
    #[test]
    fn stable_numbering_keeps_a_persisting_id_and_reuses_a_freed_one_where_fresh_numbering_shifts()
    {
        let (schema, migration, _) = hidden_path_fixture();
        let plan = compile(&schema, &schema, &migration);
        let split = partition(&[&[0, 1, 4, 5, 6]], Some(10), Some(10));
        let edge = |src: u32, tgt: u32, (a, b, name): (&str, &str, &str)| super::RefGEdge {
            src,
            tgt,
            label: super::RefEdge::new(a, b, "prop", Some(name)),
            position: None,
        };
        let mut earlier = RefGraph::default();
        for (id, anchor) in [(0, "a"), (1, "c"), (4, "b"), (5, "b")] {
            earlier.nodes.insert(id, super::RefNode::new(id, anchor));
        }
        earlier.edges.insert(0, edge(0, 1, ("a", "c", "ac")));
        earlier.edges.insert(1, edge(1, 4, ("c", "b", "cb")));
        earlier.edges.insert(2, edge(1, 5, ("c", "b", "cb")));
        let held = fresh_complements(&plan, &earlier, &split).expect("the earlier graph restricts");
        let pairs = |complement: &GraphComplementSegment| -> Vec<(u32, u32)> {
            complement
                .synthesized_edges
                .iter()
                .map(|(id, key)| match key {
                    panproto_inst::SynthKey::Contract { tgt, .. } => (id.0, *tgt),
                    other => panic!("a contracted pair: {other:?}"),
                })
                .collect()
        };
        assert_eq!(pairs(&held[0]), vec![(100, 4), (101, 5)]);
        let mut later = earlier.clone();
        later.edges.remove(&1);
        later.nodes.insert(6, super::RefNode::new(6, "b"));
        later.edges.insert(3, edge(1, 6, ("c", "b", "cb")));
        let (outcome, output) =
            check_case_held((&schema, &schema, &migration), &plan, &later, &split, &held);
        assert_eq!(outcome, Outcome::Agreed);
        let (output, persisting) = output.expect("an agreed output");
        assert_eq!(
            persisting,
            Persisting {
                kept: 1,
                renumbered: 0
            }
        );
        assert_eq!(pairs(&output.complements[0]), vec![(100, 6), (101, 5)]);
        let fresh = fresh_complements(&plan, &later, &split).expect("the later graph restricts");
        assert_eq!(pairs(&fresh[0]), vec![(100, 5), (101, 6)]);
    }

    /// Restricting `later` against `held_segments` agrees with the reference
    /// and renumbers segment 0's one persisting contracted pair from 100 to
    /// 101.
    fn assert_renumbered(
        (schema, migration, plan): (&RefSchema, &RefMigration, &RestrictPlan),
        later: &RefGraph,
        split: &reference::Partition,
        held_segments: &[GraphComplementSegment],
    ) {
        let (outcome, output) = check_case_held(
            (schema, schema, migration),
            plan,
            later,
            split,
            held_segments,
        );
        assert_eq!(outcome, Outcome::Agreed);
        let (output, persisting) = output.expect("an agreed output");
        assert_eq!(
            persisting,
            Persisting {
                kept: 0,
                renumbered: 1
            }
        );
        assert_eq!(
            output.complements[0]
                .synthesized_edges
                .keys()
                .map(|id| id.0)
                .collect::<Vec<_>>(),
            vec![101]
        );
    }

    /// The run of `plan` over `input` holding `held` and carrying tasks over
    /// from the run over `earlier`, stopped by a failed commit at each task,
    /// resumes to `baseline`.
    fn assert_failed_commits_resume(
        plan: &RestrictPlan,
        (earlier, input): (&Input, &Input),
        held: &BTreeMap<SegmentId, Vec<u8>>,
        baseline: &GraphRestrictRun,
    ) {
        let reuse = Some(earlier.manifest.digest());
        let total = baseline.report.progress.tasks_total;
        for fail_at in 1..=total {
            let mut store = FailingStore {
                inner: MemoryStore::default(),
                commits: 0,
                fail_at: None,
            };
            held_run(
                plan,
                (earlier, &earlier.bytes),
                None,
                (None, None),
                &mut store,
            );
            store.fail_at = Some(store.commits + usize::try_from(fail_at).expect("count"));
            let failed = held_run(
                plan,
                (input, &input.bytes),
                Some(held),
                (None, reuse),
                &mut store,
            );
            assert!(
                matches!(&failed.report.status, RunStatus::Incomplete { .. }),
                "{:?}",
                failed.report.status
            );
            assert_eq!(failed.report.progress.tasks_done, fail_at - 1);
            assert_eq!(failed.report.orphans.len(), 1, "the failed task's artifact");
            store.fail_at = None;
            let resumed = held_run(
                plan,
                (input, &input.bytes),
                Some(held),
                (None, reuse),
                &mut store,
            );
            assert_eq!(
                resumed.report.tasks_run + resumed.report.tasks_reused,
                total,
                "a resumed carrying run after commit {fail_at}"
            );
            assert_eq!(resumed.report.orphans, Vec::<ArtifactKey>::new());
            assert_eq!(resumed.manifest, baseline.manifest);
        }
    }

    /// A carrying run holding complements, stopped by a failed commit at
    /// each task, resumes to the uninterrupted carrying run, and a
    /// persisting key whose held id a new source node takes is renumbered
    /// to the least free id, as the reference numbers it.
    #[test]
    fn a_held_carrying_run_stopped_by_a_failed_commit_at_each_task_resumes_to_the_uninterrupted_run()
     {
        let (schema, migration, graph) = hidden_path_fixture();
        let plan = compile(&schema, &schema, &migration);
        let split = partition(&[&[0, 3], &[1, 4], &[2]], Some(10), Some(10));
        let earlier = segmented_runner::input(&graph, &split);
        let mut seed = MemoryStore::default();
        let first =
            segmented_runner::carrying_run(&plan, &earlier, &earlier.bytes, None, None, &mut seed);
        let held = emitted_complements(&seed, &first, &earlier);
        let held_segments: Vec<GraphComplementSegment> = held
            .iter()
            .map(|(id, bytes)| {
                GraphComplementSegment::from_canonical_bytes(bytes, *id).expect("decodes")
            })
            .collect();
        assert_eq!(
            held_segments[0]
                .synthesized_edges
                .keys()
                .map(|id| id.0)
                .collect::<Vec<_>>(),
            vec![100],
            "the contracted pair of node 0 takes the first id of segment 0's range"
        );
        // The later graph adds a dropped edge numbered 100 to segment 0, from
        // node 3 back to node 2 inside the hidden cycle: the contracted pair
        // persists, and must leave its held id 100 for the next free id.
        let mut later = graph;
        later.edges.insert(
            100,
            super::RefGEdge {
                src: 3,
                tgt: 2,
                label: super::RefEdge::new("c", "c", "prop", Some("cc")),
                position: None,
            },
        );
        assert_renumbered((&schema, &migration, &plan), &later, &split, &held_segments);
        let input = segmented_runner::input(&later, &split);
        let reuse = Some(earlier.manifest.digest());
        let mut baseline_store = MemoryStore::default();
        segmented_runner::carrying_run(
            &plan,
            &earlier,
            &earlier.bytes,
            None,
            None,
            &mut baseline_store,
        );
        let baseline = held_run(
            &plan,
            (&input, &input.bytes),
            Some(&held),
            (None, reuse),
            &mut baseline_store,
        );
        assert!(
            matches!(baseline.report.status, RunStatus::Complete { .. }),
            "{:?}",
            baseline.report.status
        );
        let total = baseline.report.progress.tasks_total;
        assert_eq!(total, 14);
        assert_eq!(
            (baseline.report.tasks_run, baseline.report.tasks_reused),
            (12, 2),
            "segment 0 gains the edge and segment 2 its in-stub, so only segment 1 carries Validate and Export"
        );
        assert_failed_commits_resume(&plan, (&earlier, &input), &held, &baseline);
    }
}

/// Generated segmented restricts against the reference: graphs of up to
/// eight nodes over a schema with a dropped loop, every survival choice,
/// and owners drawn from up to four segments.
mod segmented_randomized {
    use super::segmented::{
        Outcome, check_case, decodes_as_the_reference, prefixes_and_trailing_bytes_are_refused,
        prod_partition, ref_complement_segment,
    };
    use super::{RefEdge, RefGEdge, RefGraph, RefMigration, RefNode, RefSchema, RestrictPlan};
    use super::{prod_migration, prod_schema};
    use panproto_inst::segment::{graph_restrict_segmented, partition_graph};
    use panproto_reference::enumerate::segments::SEGMENTED_RESTRICT_ALLOCATION_BASE;
    use panproto_reference::segment as reference;
    use proptest::prelude::*;

    const ANCHORS: [&str; 3] = ["a", "b", "c"];

    /// The schema of the generated cases: three record vertices with every
    /// edge between them, loops included.
    pub fn schema() -> RefSchema {
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
    pub fn generated(
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
        let (graph, partition) = generated_graph(anchors, edges, owners, segments);
        Some((schema, migration, plan, graph, partition))
    }

    /// A generated graph over the anchors of [`schema`], each edge labelled
    /// by the schema edge between its endpoints' anchors, and a partition
    /// giving node `i` to segment `owners[i]` modulo `segments`, each segment
    /// with ten allocation ids.
    pub fn generated_graph(
        anchors: &[usize],
        edges: &[(usize, usize)],
        owners: &[u32],
        segments: u32,
    ) -> (RefGraph, reference::Partition) {
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
        (graph, partition)
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
        fn carrying_tasks_over_matches_the_reference_runner_on_generated_graph_pairs(
            anchors in proptest::collection::vec(0_usize..3, 2..=8),
            edges in proptest::collection::vec((0_usize..8, 0_usize..8), 0..=14),
            dropped in proptest::collection::vec(proptest::prelude::any::<bool>(), 14),
            owners in proptest::collection::vec(0_u32..4, 8),
            segments in 2_u32..=4,
            surviving in 0_u8..8,
        ) {
            let Some((schema, migration, plan, graph, partition)) =
                generated(&anchors, &edges, &owners, segments, surviving)
            else {
                return Err(TestCaseError::reject("plan outside the compile domain"));
            };
            let mut earlier = graph.clone();
            earlier
                .edges
                .retain(|id, _| !dropped[usize::try_from(*id).unwrap_or(0)]);
            super::segmented_carry::check_after(
                &super::segmented_runner::Case {
                    source: &schema,
                    target: &schema,
                    migration: &migration,
                    plan: &plan,
                    graph: &graph,
                    partition: &partition,
                },
                &earlier,
            );
        }

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

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn stable_numbering_matches_the_reference_on_generated_graph_pairs(
            anchors in proptest::collection::vec(0_usize..3, 2..=8),
            edges in proptest::collection::vec((0_usize..8, 0_usize..8), 0..=14),
            dropped in proptest::collection::vec(proptest::prelude::any::<bool>(), 14),
            floors in proptest::collection::vec(proptest::prelude::any::<bool>(), 4),
            owners in proptest::collection::vec(0_u32..4, 8),
            segments in 2_u32..=4,
            surviving in 0_u8..8,
        ) {
            let Some((schema, migration, plan, graph, partition)) =
                generated(&anchors, &edges, &owners, segments, surviving)
            else {
                return Err(TestCaseError::reject("plan outside the compile domain"));
            };
            let (earlier, later) = held_pair(&graph, &partition, &schema, &dropped, &floors);
            let production = prod_partition(&partition);
            let earlier_segments = partition_graph(&super::prod_graph(&earlier), &production)
                .expect("production split");
            let Ok(first) = graph_restrict_segmented(&plan, &earlier_segments, &production) else {
                return Err(TestCaseError::reject("the earlier restriction refuses"));
            };
            let (outcome, _) = super::segmented::check_case_held(
                (&schema, &schema, &migration),
                &plan,
                &later,
                &partition,
                &first.complements,
            );
            prop_assert!(matches!(outcome, Outcome::Agreed | Outcome::Refused));
        }
    }

    /// An earlier graph, `graph` without the edges `dropped` marks, and a
    /// later one, `graph` with an edge at the edge allocation floor of each
    /// segment `floors` marks.
    fn held_pair(
        graph: &RefGraph,
        partition: &reference::Partition,
        schema: &RefSchema,
        dropped: &[bool],
        floors: &[bool],
    ) -> (RefGraph, RefGraph) {
        let mut earlier = graph.clone();
        earlier
            .edges
            .retain(|id, _| !dropped[usize::try_from(*id).unwrap_or(0)]);
        let mut later = graph.clone();
        for (spec, floor) in partition.specs.iter().zip(floors) {
            if *floor
                && let Some(with) = super::segmented_held::with_floor_edge(&later, spec, schema)
            {
                later = with;
            }
        }
        (earlier, later)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn a_checkpointed_restriction_holding_complements_matches_the_reference_runner_on_generated_graph_pairs(
            anchors in proptest::collection::vec(0_usize..3, 2..=8),
            edges in proptest::collection::vec((0_usize..8, 0_usize..8), 0..=14),
            dropped in proptest::collection::vec(proptest::prelude::any::<bool>(), 14),
            floors in proptest::collection::vec(proptest::prelude::any::<bool>(), 4),
            owners in proptest::collection::vec(0_u32..4, 8),
            segments in 2_u32..=4,
            surviving in 0_u8..8,
        ) {
            let Some((schema, migration, plan, graph, partition)) =
                generated(&anchors, &edges, &owners, segments, surviving)
            else {
                return Err(TestCaseError::reject("plan outside the compile domain"));
            };
            let (earlier, later) = held_pair(&graph, &partition, &schema, &dropped, &floors);
            let production = prod_partition(&partition);
            let earlier_segments = partition_graph(&super::prod_graph(&earlier), &production)
                .expect("production split");
            if graph_restrict_segmented(&plan, &earlier_segments, &production).is_err() {
                return Err(TestCaseError::reject("the earlier restriction refuses"));
            }
            let mut counts = super::segmented_held::RunCounts::default();
            super::segmented_held::check_later_run(
                &super::segmented_runner::Case {
                    source: &schema,
                    target: &schema,
                    migration: &migration,
                    plan: &plan,
                    graph: &earlier,
                    partition: &partition,
                },
                &later,
                true,
                &mut counts,
            );
            prop_assert_eq!(counts.later, 1);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn complement_segments_decode_as_the_reference_decodes_them_on_generated_partitions(
            anchors in proptest::collection::vec(0_usize..3, 2..=8),
            edges in proptest::collection::vec((0_usize..8, 0_usize..8), 0..=14),
            owners in proptest::collection::vec(0_u32..4, 8),
            segments in 2_u32..=4,
            surviving in 0_u8..8,
        ) {
            let Some((_, _, plan, graph, partition)) =
                generated(&anchors, &edges, &owners, segments, surviving)
            else {
                return Err(TestCaseError::reject("plan outside the compile domain"));
            };
            let partition = prod_partition(&partition);
            let segments = partition_graph(&super::prod_graph(&graph), &partition)
                .expect("production split");
            let Ok(output) = graph_restrict_segmented(&plan, &segments, &partition) else {
                return Err(TestCaseError::reject("segmented restrict refuses"));
            };
            for complement in &output.complements {
                let bytes = complement.canonical_bytes().expect("complement bytes");
                let back = decodes_as_the_reference(&bytes, complement.segment);
                prop_assert_eq!(
                    ref_complement_segment(&back),
                    ref_complement_segment(complement)
                );
                prefixes_and_trailing_bytes_are_refused(&bytes);
            }
        }
    }
}

/// Segmented graph extension against the reference: the exhaustive
/// carrier, generated graphs, transform witnesses, and the checkpointed run.
mod segmented_extend {
    use super::segmented::{prod_partition, ref_view};
    use super::segmented_randomized::{generated_graph, schema as generated_schema};
    use super::{
        RefGraph, RefMigration, RefSchema, RestrictError, RestrictPlan, Value, prod_graph,
        prod_migration, prod_schema, ref_edge, ref_graph,
    };
    use panproto_inst::locality::{GlobalReason, Locality, SegmentOp};
    use panproto_inst::segment::{
        GraphSegment, Residual, SegmentedExtendError, assemble_graph, graph_extend_locality,
        graph_extend_segmented, partition_graph,
    };
    use panproto_inst::{
        BoundaryProtocols, FieldTransform, ViewRef, compile_extension_plan, graph_extend_engine,
        graph_extend_with_plan,
    };
    use panproto_reference::enumerate::segments::{
        extension_map, segmented_extend_case_count, segmented_extend_cases,
    };
    use panproto_reference::segment as reference;
    use panproto_schema::Protocol;

    /// The extension plan of `migration` from `source` to `target`.
    pub fn plan_of(
        source: &RefSchema,
        target: &RefSchema,
        migration: &RefMigration,
    ) -> RestrictPlan {
        let protocol = Protocol::default();
        compile_extension_plan(
            &prod_schema(source),
            &prod_schema(target),
            &prod_migration(migration),
            BoundaryProtocols {
                src: &protocol,
                tgt: &protocol,
            },
        )
        .expect("the carrier's extension plans compile")
    }

    /// A production extension refusal as the reference writes it.
    pub fn production_refusal(error: &RestrictError) -> String {
        match error {
            RestrictError::UnmappedAnchor { anchor, node_id } => {
                format!("anchor {anchor} {node_id}")
            }
            RestrictError::UnmappedEdge {
                edge,
                element: ViewRef::Edge(id),
            } => format!("edge {:?} {}", ref_edge(edge), id.0),
            other => format!("other {other:?}"),
        }
    }

    /// A reference extension refusal.
    pub fn reference_refusal(error: &reference::ExtendRefusal) -> String {
        match error {
            reference::ExtendRefusal::UnmappedAnchor { anchor, node } => {
                format!("anchor {anchor} {node}")
            }
            reference::ExtendRefusal::UnmappedEdge { edge, element } => {
                format!("edge {edge:?} {element}")
            }
        }
    }

    /// What one case showed.
    #[derive(Debug, PartialEq, Eq)]
    pub enum Outcome {
        /// Both sides extended alike, assembling to whole-state extension.
        Agreed,
        /// Both refused alike at an unmapped node.
        RefusedNode,
        /// Both refused alike at an unmapped edge.
        RefusedEdge,
    }

    /// Run one case through production and reference and check the law:
    /// the extended segments agree segment by segment, glue to production
    /// whole-state extension of the glued input, which agrees with the
    /// reference whole-state extension; or both refuse with the whole-state
    /// refusal in the same segment.
    pub fn check_case(
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
        let production = graph_extend_segmented(plan, &segments, &production_partition);
        let expected =
            reference::extend_graph_segmented(target, migration, &reference_segments, partition);
        let whole_state = graph_extend_with_plan(&whole, plan);
        match (production, expected) {
            (Ok(output), Ok(expected)) => {
                assert_eq!(output.len(), expected.len(), "{graph:?} {partition:?}");
                for (segment, expected) in output.iter().zip(&expected) {
                    assert_eq!(
                        &ref_view(segment),
                        expected,
                        "extended segment of {graph:?} under {partition:?}"
                    );
                }
                let glued =
                    assemble_graph(output, &production_partition).expect("the output glues");
                let whole_state = whole_state.expect("whole-state extension accepts");
                assert_eq!(ref_graph(&glued), ref_graph(&whole_state), "{graph:?}");
                assert_eq!(
                    ref_graph(&whole_state),
                    reference::extend_graph(target, migration, graph).expect("reference extends"),
                    "whole-state extension of {graph:?}"
                );
                Outcome::Agreed
            }
            (
                Err(SegmentedExtendError::Extend { segment, error }),
                Err(reference::SegmentedExtendRefusal::Extend {
                    segment: expected_segment,
                    error: expected,
                }),
            ) => {
                assert_eq!(
                    segment.0, expected_segment.0,
                    "refusing segment of {graph:?}"
                );
                assert_eq!(
                    production_refusal(&error),
                    reference_refusal(&expected),
                    "refusal of {graph:?}"
                );
                let whole_error = whole_state.expect_err("whole-state extension refuses");
                assert_eq!(production_refusal(&error), production_refusal(&whole_error));
                if matches!(error, RestrictError::UnmappedAnchor { .. }) {
                    Outcome::RefusedNode
                } else {
                    Outcome::RefusedEdge
                }
            }
            (production, expected) => panic!(
                "segmented extension disagrees on {graph:?} under {partition:?}: {production:?} / {expected:?}"
            ),
        }
    }

    #[test]
    fn segmented_extend_agrees_with_the_reference_and_assembles_to_whole_state_extend() {
        let mut counts = [0_u64; 3];
        let mut plan: Option<(RefSchema, RefMigration, RestrictPlan)> = None;
        for case in segmented_extend_cases() {
            if !matches!(&plan, Some((s, m, _)) if *s == case.source && *m == case.migration) {
                let compiled = plan_of(&case.source, &case.target, &case.migration);
                plan = Some((case.source.clone(), case.migration.clone(), compiled));
            }
            let Some((_, _, compiled)) = &plan else {
                unreachable!("a plan was just compiled")
            };
            let outcome = check_case(
                &case.target,
                &case.migration,
                compiled,
                &case.graph,
                &case.partition,
            );
            counts[outcome as usize] += 1;
        }
        println!("agreed, refused at a node, refused at an edge: {counts:?}");
        assert_eq!(counts.iter().sum::<u64>(), segmented_extend_case_count());
        assert_eq!(segmented_extend_case_count(), 432_810);
        assert!(
            counts.iter().all(|count| *count > 0),
            "every outcome occurs: {counts:?}"
        );
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(256))]

        #[test]
        fn segmented_extend_agrees_with_the_reference_on_generated_partitions(
            anchors in proptest::collection::vec(0_usize..3, 1..=8),
            edges in proptest::collection::vec((0_usize..8, 0_usize..8), 0..=12),
            owners in proptest::collection::vec(0_u32..4, 8),
            segments in 1_u32..=4,
            code in 0_u8..27,
        ) {
            let source = generated_schema();
            let (target, migration) = extension_map(&source, code);
            let plan = plan_of(&source, &target, &migration);
            let (graph, partition) = generated_graph(&anchors, &edges, &owners, segments);
            check_case(&target, &migration, &plan, &graph, &partition);
        }
    }

    /// The protocol of the transform witnesses: records and 64-bit integers.
    fn attribute_protocol() -> Protocol {
        Protocol {
            value_kinds: std::collections::BTreeMap::from([(
                "int".to_owned(),
                panproto_schema::ScalarKind::Int {
                    bits: 64,
                    signed: true,
                },
            )]),
            non_value_kinds: std::collections::BTreeSet::from(["record".to_owned()]),
            ..Protocol::default()
        }
    }

    /// A schema of three records joined in a cycle by the edges `ab`, `bc`
    /// and `ca`, named `vertices`, with an integer attribute of each name in
    /// `attributes` on the vertex it names.
    fn cycle_schema(vertices: [&str; 3], attributes: &[(&str, &str)]) -> panproto_schema::Schema {
        let protocol = attribute_protocol();
        let mut builder = panproto_schema::SchemaBuilder::new(&protocol);
        for vertex in vertices {
            builder = builder.vertex(vertex, "record", None).expect("vertex");
        }
        for ((src, tgt), name) in [(0, 1), (1, 2), (2, 0)].into_iter().zip(["ab", "bc", "ca"]) {
            builder = builder
                .edge(vertices[src], vertices[tgt], "prop", Some(name))
                .expect("edge");
        }
        for (vertex, field) in attributes {
            let attribute = format!("{vertex}.{field}");
            builder = builder
                .vertex(&attribute, "int", None)
                .and_then(|builder| builder.edge(vertex, &attribute, "prop", Some(field)))
                .expect("attribute");
        }
        builder.build().expect("schema")
    }

    /// The witness migration from `cycle_schema([a, b, c])` onto
    /// `cycle_schema([p, q, r])` with the value transforms `transforms`.
    fn cycle_migration(
        transforms: Vec<(&str, Vec<FieldTransform>)>,
    ) -> panproto_inst::CompiledMigration {
        let mut migration = panproto_inst::CompiledMigration::default();
        for (source, target) in [("a", "p"), ("b", "q"), ("c", "r")] {
            migration.vertex_remap.insert(source.into(), target.into());
        }
        for (vertex, list) in transforms {
            migration.field_transforms.insert(vertex.into(), list);
        }
        migration
    }

    /// The three-node cycle over `a`, `b`, `c`, each node in its own
    /// segment and carrying the field `x`, so every edge crosses segments.
    fn cycle_family() -> (panproto_inst::GInstance, panproto_inst::segment::Partition) {
        let anchors = ["a", "b", "c"];
        let mut graph = RefGraph::default();
        for (id, anchor) in (0_u32..).zip(anchors) {
            graph.nodes.insert(id, super::RefNode::new(id, anchor));
        }
        for (id, (src, tgt)) in (0_u32..).zip([(0_u32, 1_u32), (1, 2), (2, 0)]) {
            let (a, b) = (anchors[src as usize], anchors[tgt as usize]);
            graph.edges.insert(
                id,
                super::RefGEdge {
                    src,
                    tgt,
                    label: super::RefEdge::new(a, b, "prop", Some(&format!("{a}{b}"))),
                    position: None,
                },
            );
        }
        let (_, partition) = generated_graph(&[0, 1, 2], &[], &[0, 1, 2], 3);
        let mut whole = prod_graph(&graph);
        for id in 0..3 {
            if let Some(node) = whole.node_mut(id) {
                node.extra_fields
                    .insert("x".to_owned(), Value::Int(i64::from(id)));
            }
        }
        (whole, prod_partition(&partition))
    }

    #[test]
    fn field_transforms_that_evaluate_no_expression_extend_segment_by_segment_like_whole_state() {
        let protocol = attribute_protocol();
        let protocols = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };
        let source = cycle_schema(["a", "b", "c"], &[("a", "x"), ("b", "x")]);
        let target = cycle_schema(["p", "q", "r"], &[("p", "y"), ("q", "z")]);
        let migration = cycle_migration(vec![
            (
                "a",
                vec![FieldTransform::RenameField {
                    old_key: "x".to_owned(),
                    new_key: "y".to_owned(),
                }],
            ),
            (
                "b",
                vec![
                    FieldTransform::AddField {
                        key: "z".to_owned(),
                        value: Value::Int(7),
                    },
                    FieldTransform::DropField {
                        key: "x".to_owned(),
                    },
                ],
            ),
        ]);
        let plan = compile_extension_plan(&source, &target, &migration, protocols)
            .expect("the plan compiles");
        assert_eq!(graph_extend_locality(&plan), Locality::Local);
        let (whole, partition) = cycle_family();
        let segments = partition_graph(&whole, &partition).expect("split");
        let output = graph_extend_segmented(&plan, &segments, &partition).expect("extends");
        let glued = assemble_graph(output, &partition).expect("glues");
        let expected = graph_extend_engine(&whole, &source, &target, &migration, protocols)
            .expect("whole-state extension accepts");
        assert_eq!(ref_graph(&glued), ref_graph(&expected));
        let renamed = glued.node(0).expect("node 0");
        assert_eq!(renamed.anchor.as_ref(), "p");
        assert_eq!(renamed.extra_fields.get("y"), Some(&Value::Int(0)));
        let added = glued.node(1).expect("node 1");
        assert_eq!(added.extra_fields.get("z"), Some(&Value::Int(7)));
        assert_eq!(added.extra_fields.get("x"), None);
    }

    #[test]
    fn an_expression_transform_is_declined_as_a_global_expression_before_any_segment_is_read() {
        let protocol = attribute_protocol();
        let source = cycle_schema(["a", "b", "c"], &[("b", "x")]);
        let target = cycle_schema(["p", "q", "r"], &[("q", "x")]);
        let migration = cycle_migration(vec![(
            "b",
            vec![FieldTransform::ApplyExpr {
                key: "x".to_owned(),
                expr: panproto_expr::Expr::Var("x".into()),
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Iso,
            }],
        )]);
        let plan = compile_extension_plan(
            &source,
            &target,
            &migration,
            BoundaryProtocols {
                src: &protocol,
                tgt: &protocol,
            },
        )
        .expect("the plan compiles");
        let reason = GlobalReason::ExpressionDemand { vertex: "b".into() };
        assert_eq!(
            graph_extend_locality(&plan),
            Locality::Global(reason.clone())
        );
        let (whole, partition) = cycle_family();
        let segments = partition_graph(&whole, &partition).expect("split");
        // Segments out of order are no family: the residual comes before
        // any segment is checked or read.
        let unread: Vec<GraphSegment> = segments.iter().rev().cloned().collect();
        match graph_extend_segmented(&plan, &unread, &partition) {
            Err(SegmentedExtendError::Residual(Residual::Global { op, reason: found })) => {
                assert_eq!(op, SegmentOp::Extend);
                assert_eq!(found, reason);
            }
            other => panic!("an expression transform is declined: {other:?}"),
        }
    }
}

/// The checkpointed run of segmented graph extension against the reference
/// runner.
mod segmented_extend_run {
    use super::segmented_extend::{plan_of, production_refusal, reference_refusal};
    use super::segmented_randomized::{generated_graph, schema as generated_schema};
    use super::segmented_runner::{
        Input, SOURCE_SCHEMA, Seen, input, production_report, reference_report,
    };
    use super::segmented_witnesses::FailingStore;
    use super::{RefGraph, RefMigration, RefSchema, RestrictPlan};
    use panproto_inst::identity::Digest;
    use panproto_inst::segment::{
        AnySegment, ArtifactKey, Budget, GraphExtendRun, MemoryStore, Phase, Residual, Resource,
        RunIncompleteReason, RunJournal, RunSpec, RunStatus, SegmentRunReport, SegmentStore,
        SegmentedExtendError, graph_extend_run, graph_extend_segmented, partition_graph,
        read_emitted, verify_manifest,
    };
    use panproto_reference::enumerate::segments::{extension_map, segmented_extend_cases};
    use panproto_reference::segment as reference;

    /// The schema digest of a run's extended segments.
    const TARGET_SCHEMA: [u8; 32] = [24; 32];
    const LENS: [u8; 32] = [25; 32];

    fn seen_production(report: &SegmentRunReport<SegmentedExtendError>) -> Seen {
        let failed = match &report.status {
            RunStatus::Failed(SegmentedExtendError::Extend { segment, error }) => {
                format!("extend {} {}", segment.0, production_refusal(error))
            }
            RunStatus::Failed(SegmentedExtendError::Segment(error)) => format!("segment {error:?}"),
            RunStatus::Failed(other) => format!("other {other:?}"),
            _ => String::new(),
        };
        production_report(report, failed)
    }

    fn seen_reference(
        report: &reference::SegmentRunReport<reference::SegmentedExtendRefusal>,
    ) -> Seen {
        let failed = match &report.status {
            reference::RunStatus::Failed(reference::SegmentedExtendRefusal::Extend {
                segment,
                error,
            }) => format!("extend {} {}", segment.0, reference_refusal(error)),
            reference::RunStatus::Failed(reference::SegmentedExtendRefusal::Segment(error)) => {
                format!("segment {error:?}")
            }
            _ => String::new(),
        };
        reference_report(report, failed, format!("{:?}", report.locality.extend))
    }

    /// One extension of the carrier on both sides.
    struct Case<'a> {
        target: &'a RefSchema,
        migration: &'a RefMigration,
        plan: &'a RestrictPlan,
        input: Input,
        segments: Vec<reference::GraphSegment>,
        partition: &'a reference::Partition,
    }

    impl<'a> Case<'a> {
        fn new(
            target: &'a RefSchema,
            migration: &'a RefMigration,
            plan: &'a RestrictPlan,
            graph: &RefGraph,
            partition: &'a reference::Partition,
        ) -> Self {
            Self {
                target,
                migration,
                plan,
                input: input(graph, partition),
                segments: reference::partition_graph(graph, partition).expect("reference split"),
                partition,
            }
        }

        fn production<S: SegmentStore>(
            &self,
            budget: Budget,
            store: &mut S,
            cancel: Option<u32>,
        ) -> GraphExtendRun {
            graph_extend_run(
                self.plan,
                &self.input.manifest,
                &self.input.bytes,
                RunSpec {
                    lens: Digest(LENS),
                    input: self.input.manifest.digest(),
                    output_schema: Digest(TARGET_SCHEMA),
                    cancel_after_tasks: cancel,
                    reuse: None,
                },
                budget,
                store,
                &mut |_| {},
            )
        }

        fn reference(
            &self,
            budget: Budget,
            store: &mut reference::MemoryStore,
            cancel: Option<u32>,
        ) -> reference::ExtendRun {
            reference::segmented_extend_with_progress(
                reference::ExtendLens {
                    tgt: self.target,
                    m: self.migration,
                },
                &self.segments,
                self.partition,
                &reference::ExtendRunSpec {
                    lens: reference::Digest(LENS),
                    source_schema: reference::Digest(SOURCE_SCHEMA),
                    target_schema: reference::Digest(TARGET_SCHEMA),
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
    }

    /// What the run law showed.
    #[derive(Debug, Default)]
    struct Tally {
        complete: u64,
        refused: u64,
        declined: u64,
        cancellations: u64,
    }

    /// The journal of a run holds no Emit or Seal task.
    fn sealed_nothing(store: &MemoryStore, run: Digest) -> bool {
        let journal: RunJournal = store.journal(&run).unwrap_or_else(|never| match never {});
        journal
            .tasks
            .keys()
            .all(|task| !matches!(task.phase, Phase::Emit | Phase::Seal))
    }

    /// Check the run law on one extension under `budget`: the production
    /// report equals the reference runner's uninterrupted and, when
    /// `every_point`, at every cancellation point and after resuming; a
    /// complete run emits the reference's extended segments and those of
    /// in-memory segmented extension with no complement, under a manifest
    /// that verifies; a refused or declined run seals nothing.
    fn check_run(case: &Case<'_>, budget: Budget, every_point: bool, tally: &mut Tally) {
        let mut store = MemoryStore::default();
        let baseline = case.production(budget, &mut store, None);
        let mut reference_store = reference::MemoryStore::default();
        let expected = case.reference(budget, &mut reference_store, None);
        assert_eq!(
            seen_production(&baseline.report),
            seen_reference(&expected.report),
            "uninterrupted extension under {:?}",
            case.partition
        );
        let run = baseline.run.expect("a run identity");
        assert_eq!(run.0, expected.run.0, "run identity");
        match &baseline.report.status {
            RunStatus::Complete { manifest } => {
                tally.complete += 1;
                check_emitted(case, &store, &baseline, *manifest, &expected);
            }
            RunStatus::Failed(_) => {
                tally.refused += 1;
                assert!(
                    sealed_nothing(&store, run),
                    "a refused extension seals nothing"
                );
            }
            RunStatus::Residual(_) => {
                tally.declined += 1;
                assert!(
                    sealed_nothing(&store, run),
                    "a declined extension seals nothing"
                );
            }
            RunStatus::Incomplete { reason } => {
                panic!("an uninterrupted extension stopped: {reason:?}")
            }
        }
        if every_point {
            check_cancellations(case, budget, &baseline, run, tally);
        }
    }

    /// The emitted segments of the complete run `baseline` in `store`
    /// carry no complement and are the in-memory and reference extended
    /// segments, under a manifest with digest `manifest` that verifies.
    fn check_emitted(
        case: &Case<'_>,
        store: &MemoryStore,
        baseline: &GraphExtendRun,
        manifest: Digest,
        expected: &reference::ExtendRun,
    ) {
        let run = baseline.run.expect("a run identity");
        let sealed = baseline.manifest.as_ref().expect("a manifest");
        assert_eq!(sealed.digest(), manifest);
        assert_eq!(verify_manifest(sealed, manifest), Ok(()));
        let partition = &case.input.partition;
        let source = partition_graph(
            &super::prod_graph(
                &reference::assemble_graph(&case.segments, case.partition).expect("glued input"),
            ),
            partition,
        )
        .expect("split");
        let in_memory = graph_extend_segmented(case.plan, &source, partition).expect("extends");
        let schema = Digest(TARGET_SCHEMA);
        for ((spec, held), expected) in partition
            .specs()
            .iter()
            .zip(&in_memory)
            .zip(expected.output.as_ref().expect("output"))
        {
            let emitted = read_emitted(store, run, partition, spec.id).expect("an emitted segment");
            assert_eq!(
                emitted.complement,
                Vec::<u8>::new(),
                "extension emits no complement"
            );
            let bytes = AnySegment::Graph(emitted.view)
                .canonical_bytes(&schema, partition)
                .expect("extended bytes");
            assert_eq!(
                bytes,
                AnySegment::Graph(held.clone())
                    .canonical_bytes(&schema, partition)
                    .expect("in-memory bytes")
            );
            assert_eq!(
                bytes,
                reference::canonical_segment_bytes(
                    &reference::AnySegment::Graph(expected.clone()),
                    reference::Digest(TARGET_SCHEMA),
                    case.partition,
                )
                .expect("reference extended bytes"),
                "extended segment under {:?}",
                case.partition
            );
        }
    }

    /// Stopping the run of `case` at every cancellation point and resuming
    /// it reports what the reference runner reports, and ends as `baseline`.
    fn check_cancellations(
        case: &Case<'_>,
        budget: Budget,
        baseline: &GraphExtendRun,
        run: Digest,
        tally: &mut Tally,
    ) {
        let (total, done) = (
            baseline.report.progress.tasks_total,
            baseline.report.progress.tasks_done,
        );
        for cancel in 0..=total {
            tally.cancellations += 1;
            let mut store = MemoryStore::default();
            let interrupted = case.production(budget, &mut store, Some(cancel));
            let mut reference_store = reference::MemoryStore::default();
            let stopped = case.reference(budget, &mut reference_store, Some(cancel));
            assert_eq!(
                seen_production(&interrupted.report),
                seen_reference(&stopped.report),
                "extension cancelled after {cancel} tasks"
            );
            let resumed = case.production(budget, &mut store, None);
            let again = case.reference(budget, &mut reference_store, None);
            assert_eq!(
                seen_production(&resumed.report),
                seen_reference(&again.report),
                "extension resumed after {cancel} tasks"
            );
            assert_eq!(resumed.report.tasks_reused, cancel.min(done));
            assert_eq!(
                format!("{:?}", resumed.report.status),
                format!("{:?}", baseline.report.status)
            );
            assert_eq!(resumed.manifest, baseline.manifest);
            assert_eq!(
                store
                    .staged_uncommitted(&run)
                    .unwrap_or_else(|never| match never {}),
                Vec::<ArtifactKey>::new()
            );
        }
    }

    fn sweep(select: impl Fn(usize) -> bool) -> (usize, usize, Tally) {
        let mut tally = Tally::default();
        let (mut cases, mut selected) = (0, 0);
        let mut plan: Option<(RefSchema, RefMigration, RestrictPlan)> = None;
        for (position, case) in segmented_extend_cases().step_by(331).enumerate() {
            cases += 1;
            if !select(position) {
                continue;
            }
            selected += 1;
            if !matches!(&plan, Some((s, m, _)) if *s == case.source && *m == case.migration) {
                let compiled = plan_of(&case.source, &case.target, &case.migration);
                plan = Some((case.source.clone(), case.migration.clone(), compiled));
            }
            let Some((_, _, compiled)) = &plan else {
                unreachable!("a plan was just compiled")
            };
            let run = Case::new(
                &case.target,
                &case.migration,
                compiled,
                &case.graph,
                &case.partition,
            );
            check_run(&run, Budget::unlimited(), true, &mut tally);
        }
        (cases, selected, tally)
    }

    /// Every 331st case of the segmented extension carrier, run as a
    /// checkpointed extension: the report equals the reference runner's
    /// uninterrupted and at every cancellation point, a resumed run ends as
    /// the uninterrupted one, a complete run emits the reference's and
    /// in-memory segmented extension's segments, and a refused run seals
    /// nothing.
    #[test]
    fn segmented_extend_run_matches_the_reference_runner_at_every_cancellation_point() {
        let (cases, selected, tally) = sweep(|_| true);
        assert_eq!((cases, selected), (1_308, 1_308));
        println!("{tally:?}");
        assert!(
            tally.complete > 0 && tally.refused > 0 && tally.cancellations > 0,
            "{tally:?}"
        );
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(32))]

        #[test]
        fn segmented_extend_run_matches_the_reference_runner_on_generated_partitions(
            anchors in proptest::collection::vec(0_usize..3, 1..=6),
            edges in proptest::collection::vec((0_usize..6, 0_usize..6), 0..=8),
            owners in proptest::collection::vec(0_u32..4, 6),
            segments in 1_u32..=3,
            code in 0_u8..27,
        ) {
            let source = generated_schema();
            let (target, migration) = extension_map(&source, code);
            let plan = plan_of(&source, &target, &migration);
            let (graph, partition) = generated_graph(&anchors, &edges, &owners, segments);
            let case = Case::new(&target, &migration, &plan, &graph, &partition);
            check_run(&case, Budget::unlimited(), true, &mut Tally::default());
        }
    }

    /// The witness: `a` to `p`, `b` to `q` and `c` without an image, over
    /// three nodes at `a`, `b`, `a` in three segments joined by the edges
    /// `ab`, `ba` and `aa`, whose images `p` to `q`, `q` to `p` and `p` to
    /// `p` are each the one target edge between their endpoints; every edge
    /// crosses segments and the extension completes.
    fn witness() -> (RefSchema, RefMigration, RefGraph, reference::Partition) {
        let (target, migration) = extension_map(&generated_schema(), 1 + 2 * 3);
        let (graph, partition) =
            generated_graph(&[0, 1, 0], &[(0, 1), (1, 2), (2, 0)], &[0, 1, 2], 3);
        (target, migration, graph, partition)
    }

    #[test]
    fn an_extension_run_declines_each_exceeded_budget_like_the_reference() {
        let (target, migration, graph, partition) = witness();
        let plan = plan_of(&generated_schema(), &target, &migration);
        let case = Case::new(&target, &migration, &plan, &graph, &partition);
        let mut tally = Tally::default();
        check_run(&case, Budget::unlimited(), true, &mut tally);
        assert_eq!(tally.complete, 1, "the witness extends: {tally:?}");
        let segments = Budget {
            affected_segments: 2,
            ..Budget::unlimited()
        };
        let working_set = Budget {
            working_set: 0,
            ..Budget::unlimited()
        };
        let summary = Budget {
            summary: 2,
            ..Budget::unlimited()
        };
        let complement = Budget {
            complement: Some(0),
            ..Budget::unlimited()
        };
        for (budget, resource) in [
            (segments, Some(Resource::AffectedSegments)),
            (working_set, Some(Resource::WorkingSet)),
            (summary, Some(Resource::Summary)),
            (complement, None),
        ] {
            let mut tally = Tally::default();
            check_run(&case, budget, true, &mut tally);
            let mut store = MemoryStore::default();
            let run = case.production(budget, &mut store, None);
            match (resource, &run.report.status) {
                (
                    Some(resource),
                    RunStatus::Residual(Residual::BudgetExceeded {
                        resource: found, ..
                    }),
                ) => assert_eq!(*found, resource),
                (None, RunStatus::Complete { .. }) => {}
                (resource, status) => panic!("{resource:?} under {budget:?}: {status:?}"),
            }
        }
    }

    #[test]
    fn an_extension_run_stopped_by_a_failed_commit_at_any_task_resumes_to_the_uninterrupted_run() {
        let (target, migration, graph, partition) = witness();
        let plan = plan_of(&generated_schema(), &target, &migration);
        let case = Case::new(&target, &migration, &plan, &graph, &partition);
        let mut baseline_store = MemoryStore::default();
        let baseline = case.production(Budget::unlimited(), &mut baseline_store, None);
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
            let failed = case.production(Budget::unlimited(), &mut store, None);
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
            let resumed = case.production(Budget::unlimited(), &mut store, None);
            assert_eq!(resumed.report.tasks_reused, fail_at - 1);
            assert_eq!(resumed.report.orphans, Vec::<ArtifactKey>::new());
            assert_eq!(resumed.manifest, baseline.manifest);
            assert_eq!(
                format!("{:?}", resumed.report.status),
                format!("{:?}", baseline.report.status)
            );
        }
    }

    #[test]
    fn a_refused_extension_run_fails_at_assemble_with_the_whole_state_refusal_and_seals_nothing() {
        let source = generated_schema();
        // `c` has no image: node 2 is refused, in segment 2.
        let (target, migration) = extension_map(&source, 1 + 2 * 3);
        let plan = plan_of(&source, &target, &migration);
        let (graph, partition) = generated_graph(&[0, 1, 2], &[(0, 1), (2, 0)], &[0, 1, 2], 3);
        let case = Case::new(&target, &migration, &plan, &graph, &partition);
        let mut store = MemoryStore::default();
        let run = case.production(Budget::unlimited(), &mut store, None);
        match &run.report.status {
            RunStatus::Failed(SegmentedExtendError::Extend { segment, error }) => {
                assert_eq!(segment.0, 2);
                assert_eq!(production_refusal(error), "anchor c 2");
            }
            other => panic!("the extension is refused: {other:?}"),
        }
        assert_eq!(run.report.progress.phase, Phase::Summarize);
        assert_eq!(run.report.progress.tasks_done, 9);
        assert!(sealed_nothing(&store, run.run.expect("a run identity")));
    }
}
