#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Differential test of the plan-taking graph restrict against the
//! executable reference, on a scope that holds fans well formed and
//! malformed.
//!
//! Inputs come from the reference enumerators: every graph of up to three
//! nodes anchored at `a`, `b` or `h`, of the plain or the fan shape, with
//! up to two edges carrying the parent label, the child label or the plain
//! edge `a -e-> b`, under every vertex migration of the schema to itself.
//! Both engines must agree exactly: a view and a complement field by field
//! (the identity envelope excepted, which neither engine stamps), or the
//! same refusal with the same payload. On a graph outside the fan domain
//! that refusal must be the malformed fan the reference fan domain names
//! first.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use panproto_inst::{
    CompiledMigration, EdgeId, FanViolation, GEdge, GInstance, GraphComplement, IdFloor, Node,
    NodeShape, RestrictError, RestrictPlan, SynthKey, graph_restrict_with_plan,
};
use panproto_reference::enumerate::{GraphScope, MigrationScope, graphs, graphs_count, migrations};
use panproto_reference::fan::{self, FanViolation as RefFanViolation};
use panproto_reference::migration::Migration as RefMigration;
use panproto_reference::model::{
    GEdge as RefGEdge, Graph as RefGraph, Node as RefNode, NodeShape as RefNodeShape,
    SEdge as RefSEdge,
};
use panproto_reference::restrict::{
    self, Complement as RefComplement, EngineError, NoValueLayer, Plan,
    RestrictError as RefRestrictError, SynthKey as RefSynthKey,
};
use panproto_reference::schema::{HyperEdge, Schema as RefSchema};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

// ---------------------------------------------------------------------------
// Conversions between production and reference models
// ---------------------------------------------------------------------------

fn ref_edge(edge: &Edge) -> RefSEdge {
    RefSEdge::new(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
}

fn prod_edge(edge: &RefSEdge) -> Edge {
    Edge {
        src: edge.src.as_str().into(),
        tgt: edge.tgt.as_str().into(),
        kind: edge.kind.as_str().into(),
        name: edge.name.as_deref().map(Into::into),
    }
}

fn ref_shape(shape: &NodeShape) -> RefNodeShape {
    match shape {
        NodeShape::Plain => RefNodeShape::Plain,
        NodeShape::HyperEdge => RefNodeShape::HyperEdge,
        other => panic!("node shape outside the differential scope: {other:?}"),
    }
}

fn prod_shape(shape: &RefNodeShape) -> NodeShape {
    match shape {
        RefNodeShape::Plain => NodeShape::Plain,
        RefNodeShape::HyperEdge => NodeShape::HyperEdge,
        other => panic!("node shape outside the differential scope: {other:?}"),
    }
}

/// A production node on the reference model. The scope holds no values,
/// fields, discriminators or annotations, and no engine adds one.
fn ref_node(node: &Node) -> RefNode {
    assert!(
        node.value.is_none()
            && node.discriminator.is_none()
            && node.extra_fields.is_empty()
            && node.annotations.is_empty(),
        "node outside the differential scope: {node:?}"
    );
    let mut out = RefNode::new(node.id, &node.anchor);
    out.position = node.position;
    out.shape = ref_shape(&node.shape);
    out
}

fn prod_node(node: &RefNode) -> Node {
    assert!(
        node.value.is_none()
            && node.discriminator.is_none()
            && node.fields.is_empty()
            && node.annotations.is_empty(),
        "node outside the differential scope: {node:?}"
    );
    let mut out = Node::new(node.id, node.anchor.as_str()).with_shape(prod_shape(&node.shape));
    out.position = node.position;
    out
}

fn ref_gedge(edge: &GEdge) -> RefGEdge {
    RefGEdge {
        src: edge.src,
        tgt: edge.tgt,
        label: ref_edge(&edge.edge),
        position: edge.position,
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
            .map(|(id, edge)| (id.0, ref_gedge(edge)))
            .collect(),
    }
}

fn prod_graph(graph: &RefGraph) -> GInstance {
    let mut out = GInstance::new();
    for node in graph.nodes.values() {
        out.insert_node(prod_node(node))
            .expect("distinct reference node id");
    }
    for (id, edge) in &graph.edges {
        out.insert_edge(
            EdgeId(*id),
            GEdge::new(edge.src, edge.tgt, prod_edge(&edge.label)).with_position(edge.position),
        )
        .expect("reference edge between reference nodes");
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
        other => panic!("synthesized key outside the differential scope: {other:?}"),
    }
}

/// The graph complement on the reference model, the identity envelope left
/// out: the plan-taking engine leaves it unstamped, as the reference does.
fn ref_complement(complement: &GraphComplement) -> RefComplement {
    assert!(
        complement.original_extra_fields.is_empty() && complement.original_values.is_empty(),
        "complement outside the differential scope: {complement:?}"
    );
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
            .map(|(id, edge)| (id.0, ref_gedge(edge)))
            .collect(),
        original_edges: complement
            .original_edges
            .iter()
            .map(|(id, edge)| (id.0, ref_gedge(edge)))
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
    builder.build().expect("reference schema builds")
}

/// A vertex migration of the scope on the production side.
fn prod_migration(migration: &RefMigration) -> CompiledMigration {
    assert!(
        migration.edge_remap.is_empty()
            && migration.resolver.is_empty()
            && migration.expansion_path.is_empty()
            && migration.hyper_resolver.is_empty()
            && migration.conditional.is_empty()
            && migration.transformed.is_empty(),
        "migration outside the differential scope: {migration:?}"
    );
    CompiledMigration {
        surviving_verts: migration
            .surviving_verts
            .iter()
            .map(|v| v.as_str().into())
            .collect(),
        surviving_edges: migration.surviving_edges.iter().map(prod_edge).collect(),
        vertex_remap: migration
            .vertex_remap
            .iter()
            .map(|(source, target)| (source.as_str().into(), target.as_str().into()))
            .collect(),
        ..CompiledMigration::default()
    }
}

// ---------------------------------------------------------------------------
// One refusal vocabulary for both engines
// ---------------------------------------------------------------------------

/// A production fan violation in reference coordinates.
fn fan_reason(reason: &FanViolation) -> RefFanViolation {
    let ids = |edges: &[EdgeId]| edges.iter().map(|edge| edge.0).collect::<Vec<u32>>();
    match reason {
        FanViolation::MissingParent => RefFanViolation::MissingParent,
        FanViolation::MultipleParents { edges } => {
            RefFanViolation::MultipleParents { edges: ids(edges) }
        }
        FanViolation::DuplicateLabel { label, edges } => RefFanViolation::DuplicateLabel {
            label: label.to_string(),
            edges: ids(edges),
        },
        FanViolation::UnknownLabel { label } => RefFanViolation::UnknownLabel {
            label: label.to_string(),
        },
        FanViolation::ReservedKindOutsideFan { edge } => {
            RefFanViolation::ReservedKindOutsideFan { edge: edge.0 }
        }
        FanViolation::ForeignIncidentEdge { edge } => {
            RefFanViolation::ForeignIncidentEdge { edge: edge.0 }
        }
        FanViolation::UnknownHyperEdge { hyper_edge } => RefFanViolation::UnknownHyperEdge {
            hyper_edge: hyper_edge.to_string(),
        },
        other @ FanViolation::MissingChild { .. } => {
            panic!("fan violation outside the differential scope: {other:?}")
        }
    }
}

/// A refusal both engines can state.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Refusal {
    /// The input is outside the fan domain.
    MalformedFan { node: u32, reason: RefFanViolation },
    /// An error located at `(stage, id, id2)`.
    At {
        stage: u8,
        id: u32,
        id2: u32,
        error: String,
    },
}

fn production_refusal(error: &RestrictError) -> Refusal {
    match error {
        RestrictError::MalformedFan { node, reason } => Refusal::MalformedFan {
            node: *node,
            reason: fan_reason(reason),
        },
        RestrictError::At { key, error } => Refusal::At {
            stage: key.stage,
            id: key.id,
            id2: key.id2,
            error: match &**error {
                RestrictError::IncoherentEdgeImage { source_edge, .. } => {
                    format!("IncoherentEdgeImage {:?}", ref_edge(source_edge))
                }
                RestrictError::NoEdgeFound { src, tgt } => format!("NoEdgeFound {src} {tgt}"),
                RestrictError::AmbiguousEdge { src, tgt, count } => {
                    format!("AmbiguousEdge {src} {tgt} {count}")
                }
                other => panic!("production refusal outside the differential scope: {other:?}"),
            },
        },
        other => panic!("production refusal outside the differential scope: {other:?}"),
    }
}

fn reference_refusal(error: &RefRestrictError) -> Refusal {
    match error {
        RefRestrictError::MalformedFan { node, reason } => Refusal::MalformedFan {
            node: *node,
            reason: reason.clone(),
        },
        RefRestrictError::At { key, error } => Refusal::At {
            stage: key.stage,
            id: key.id,
            id2: key.id2,
            error: match error {
                EngineError::IncoherentEdgeImage { label } => {
                    format!("IncoherentEdgeImage {label:?}")
                }
                EngineError::NoEdgeFound { src, tgt } => format!("NoEdgeFound {src} {tgt}"),
                EngineError::AmbiguousEdge { src, tgt, count } => {
                    format!("AmbiguousEdge {src} {tgt} {count}")
                }
                other => panic!("reference refusal outside the differential scope: {other:?}"),
            },
        },
        other => panic!("reference refusal outside the differential scope: {other:?}"),
    }
}

type Outcome = Result<(RefGraph, RefComplement), Refusal>;

// ---------------------------------------------------------------------------
// The scope
// ---------------------------------------------------------------------------

/// Vertices `a` and `b`, the edge `a -e-> b`, and the hyper-edge `h` with
/// signature `{p -> a, l -> b}` and parent label `p`.
fn fan_schema() -> RefSchema {
    let mut schema = RefSchema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .edge("a", "b", "prop", Some("e"));
    schema.hyper_edges.insert(
        "h".to_owned(),
        HyperEdge {
            kind: "fan".to_owned(),
            signature: BTreeMap::from([
                ("l".to_owned(), "b".to_owned()),
                ("p".to_owned(), "a".to_owned()),
            ]),
            parent_label: "p".to_owned(),
        },
    );
    schema
}

fn labels() -> [RefSEdge; 3] {
    [
        RefSEdge::new("h", "a", restrict::HYPER_PARENT, Some("p")),
        RefSEdge::new("h", "b", restrict::HYPER_LABEL, Some("l")),
        RefSEdge::new("a", "b", "prop", Some("e")),
    ]
}

/// A graph of the scope from its nodes (`(anchor, fan shape)`, ids from 0)
/// and edges (`(src, tgt, label index)`, ids from 0).
fn graph(nodes: &[(&str, bool)], edges: &[(u32, u32, usize)]) -> RefGraph {
    let labels = labels();
    let mut g = RefGraph::default();
    for (id, (anchor, fan)) in (0u32..).zip(nodes) {
        let mut node = RefNode::new(id, anchor);
        if *fan {
            node.shape = RefNodeShape::HyperEdge;
        }
        g.nodes.insert(id, node);
    }
    for (id, (src, tgt, label)) in (0u32..).zip(edges) {
        g.edges.insert(
            id,
            RefGEdge {
                src: *src,
                tgt: *tgt,
                label: labels[*label].clone(),
                position: None,
            },
        );
    }
    g
}

/// The sources outside the fan domain on which the two engines disagreed
/// before the domain was fixed (label 0 is the parent label, 1 the child
/// label).
fn recorded_witnesses() -> Vec<RefGraph> {
    vec![
        graph(
            &[("a", false), ("a", false), ("a", false)],
            &[(2, 0, 0), (2, 0, 1)],
        ),
        graph(&[("a", false), ("a", false)], &[(1, 1, 1), (0, 0, 1)]),
        graph(
            &[("a", false), ("b", false), ("h", true)],
            &[(0, 2, 1), (0, 0, 1)],
        ),
        graph(
            &[("a", false), ("b", false), ("h", true)],
            &[(2, 2, 0), (1, 0, 0)],
        ),
        graph(&[("a", false), ("a", false)], &[(0, 0, 0), (1, 1, 1)]),
        graph(&[("a", false), ("a", false)], &[(0, 1, 1), (0, 1, 0)]),
        graph(&[("b", false)], &[(0, 0, 0), (0, 0, 1)]),
    ]
}

/// One migration of the scope, compiled on both sides.
struct Pair {
    src: RefSchema,
    migration: RefMigration,
    reference: Plan,
    production: RestrictPlan,
}

fn production_outcome(pair: &Pair, source: &RefGraph) -> Outcome {
    graph_restrict_with_plan(&prod_graph(source), &pair.production)
        .map(|(view, complement)| (ref_graph(&view), ref_complement(&complement)))
        .map_err(|error| production_refusal(&error))
}

fn reference_outcome(pair: &Pair, source: &RefGraph) -> Outcome {
    restrict::get_with_plan(
        &pair.src,
        &pair.src,
        &pair.migration,
        &pair.reference,
        &NoValueLayer,
        source,
    )
    .map_err(|error| reference_refusal(&error))
}

/// The counts of one run of the differential.
#[derive(Debug, Default)]
struct Census {
    compared: AtomicUsize,
    outside: AtomicUsize,
    refused_inside: AtomicUsize,
}

#[test]
fn graph_restrict_agrees_with_the_reference_inside_and_outside_the_fan_domain() {
    let src = fan_schema();
    let prod_src = prod_schema(&src);
    let mut pairs = Vec::new();
    let mut plan_refusals = 0usize;
    for migration in migrations(&src, &src, MigrationScope::VERTICES) {
        let reference = restrict::compile(&src, &src, &migration);
        let production = RestrictPlan::compile(&prod_src, &prod_src, &prod_migration(&migration));
        match (reference, production) {
            (Ok(reference), Ok(production)) => pairs.push(Pair {
                src: src.clone(),
                migration,
                reference,
                production,
            }),
            (Err(_), Err(_)) => plan_refusals += 1,
            (reference, production) => panic!(
                "the plans disagree on {migration:?}: reference {reference:?}, production {production:?}"
            ),
        }
    }
    let labels = labels();
    let scope = GraphScope {
        anchors: &["a", "b", "h"],
        values: &[],
        shapes: &[RefNodeShape::Plain, RefNodeShape::HyperEdge],
        labels: &labels,
        positions: &[None],
        max_nodes: 3,
        max_edges: 2,
    };
    let sources = graphs(&scope);
    assert_eq!(
        u64::try_from(sources.len()).unwrap(),
        graphs_count(3, 0, 2, 3, 1, 3, 2)
    );
    let census = Census::default();
    for witness in recorded_witnesses() {
        assert!(
            sources.contains(&witness),
            "{witness:?} is not in the scope"
        );
    }
    let workers = std::thread::available_parallelism().map_or(1, |n| usize::from(n).min(4));
    let chunk = sources.len().div_ceil(workers);
    std::thread::scope(|threads| {
        for part in sources.chunks(chunk) {
            let (pairs, census) = (&pairs, &census);
            threads.spawn(move || {
                for source in part {
                    let first = fan::first_violation(source, None);
                    for pair in pairs {
                        let production = production_outcome(pair, source);
                        let reference = reference_outcome(pair, source);
                        assert_eq!(
                            production, reference,
                            "the engines disagree on {source:?} under {:?}",
                            pair.migration
                        );
                        match &first {
                            Some((node, reason)) => {
                                assert_eq!(
                                    reference,
                                    Err(Refusal::MalformedFan {
                                        node: *node,
                                        reason: reason.clone(),
                                    }),
                                    "{source:?}"
                                );
                                census.outside.fetch_add(1, Ordering::Relaxed);
                            }
                            None => {
                                if reference.is_err() {
                                    census.refused_inside.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        }
                        census.compared.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });
        }
    });
    let compared = census.compared.load(Ordering::Relaxed);
    let outside = census.outside.load(Ordering::Relaxed);
    let refused_inside = census.refused_inside.load(Ordering::Relaxed);
    println!(
        "restrict differential: {} sources, {} plans compiled ({plan_refusals} refused on both sides), \
         {compared} cases compared, {outside} outside the fan domain (each refused as a malformed fan \
         by both engines), {refused_inside} inside it refused alike",
        sources.len(),
        pairs.len(),
    );
    assert_eq!(compared, sources.len() * pairs.len());
    assert!(outside > 0 && compared > outside);
}
