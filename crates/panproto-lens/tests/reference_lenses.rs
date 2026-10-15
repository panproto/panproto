//! Differential tests of the tree, table and graph lenses, their paths and
//! squares against the executable reference semantics.
//!
//! Every test here converts production inputs and outputs to the models of
//! `panproto_reference` and compares the two sides exactly: views and
//! complements field by field (the identity envelope excepted, because the
//! two sides digest different migration encodings and each side checks its
//! own), restored sources exactly, and refusals through one typed reason
//! map. Inputs come from the reference enumerators; this file holds only
//! conversions, the reason map and assertions.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, HashMap};

use panproto_inst::{
    CompiledMigration, EdgeId, FieldPresence, GEdge, GInstance, GraphComplement, IdFloor, Node,
    NodeShape, PutDomainError, RestrictError, SynthKey, Value, ViewRef,
};
use panproto_lens::{Lens, LensError, graph_lens};
use panproto_reference::harness::{Disagreement, agree};
use panproto_reference::lens::{GraphLens as RefGraphLens, LensError as RefLensError};
use panproto_reference::migration::Migration as RefMigration;
use panproto_reference::model::{
    GEdge as RefGEdge, Graph as RefGraph, Node as RefNode, NodeShape as RefNodeShape,
    SEdge as RefSEdge,
};
use panproto_reference::restrict::{
    Complement as RefComplement, EngineError, NoValueLayer, Plan, PlanError,
    RestrictError as RefRestrictError, SynthKey as RefSynthKey,
};
use panproto_reference::schema::Schema as RefSchema;
use panproto_reference::value::{Presence as RefPresence, Value as RefValue};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};
use proptest::prelude::*;

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

fn ref_value(value: &Value) -> RefValue {
    match value {
        Value::Bool(b) => RefValue::Bool(*b),
        Value::Int(i) => RefValue::Int(*i),
        Value::Str(s) => RefValue::Str(s.clone()),
        Value::Null => RefValue::Null,
        other => panic!("value outside the lens differential alphabet: {other:?}"),
    }
}

fn prod_value(value: &RefValue) -> Value {
    match value {
        RefValue::Bool(b) => Value::Bool(*b),
        RefValue::Int(i) => Value::Int(*i),
        RefValue::Str(s) => Value::Str(s.clone()),
        RefValue::Null => Value::Null,
        other => panic!("value outside the lens differential alphabet: {other:?}"),
    }
}

fn ref_presence(presence: &FieldPresence) -> RefPresence {
    match presence {
        FieldPresence::Present(value) => RefPresence::Present(ref_value(value)),
        FieldPresence::Null => RefPresence::Null,
        FieldPresence::Absent => RefPresence::Absent,
    }
}

fn prod_presence(presence: &RefPresence) -> FieldPresence {
    match presence {
        RefPresence::Present(value) => FieldPresence::Present(prod_value(value)),
        RefPresence::Null => FieldPresence::Null,
        RefPresence::Absent => FieldPresence::Absent,
    }
}

fn ref_fields(fields: &HashMap<String, Value>) -> BTreeMap<String, RefValue> {
    fields
        .iter()
        .map(|(key, value)| (key.clone(), ref_value(value)))
        .collect()
}

fn ref_shape(shape: &NodeShape) -> RefNodeShape {
    match shape {
        NodeShape::Plain => RefNodeShape::Plain,
        NodeShape::List => RefNodeShape::List,
        NodeShape::XmlElement { tag } => RefNodeShape::XmlElement(tag.to_string()),
        NodeShape::XmlTextSegment => RefNodeShape::XmlTextSegment,
        NodeShape::HyperEdge => RefNodeShape::HyperEdge,
    }
}

fn prod_shape(shape: &RefNodeShape) -> NodeShape {
    match shape {
        RefNodeShape::Plain => NodeShape::Plain,
        RefNodeShape::List => NodeShape::List,
        RefNodeShape::XmlElement(tag) => NodeShape::XmlElement {
            tag: tag.as_str().into(),
        },
        RefNodeShape::XmlTextSegment => NodeShape::XmlTextSegment,
        RefNodeShape::HyperEdge => NodeShape::HyperEdge,
    }
}

fn ref_node(node: &Node) -> RefNode {
    RefNode {
        id: node.id,
        anchor: node.anchor.to_string(),
        value: node.value.as_ref().map(ref_presence),
        discriminator: node.discriminator.as_ref().map(ToString::to_string),
        fields: ref_fields(&node.extra_fields),
        position: node.position,
        shape: ref_shape(&node.shape),
        annotations: ref_fields(&node.annotations),
    }
}

fn prod_node(node: &RefNode) -> Node {
    let mut out = Node::new(node.id, node.anchor.as_str()).with_shape(prod_shape(&node.shape));
    out.value = node.value.as_ref().map(prod_presence);
    out.discriminator = node.discriminator.as_deref().map(Into::into);
    out.extra_fields = node
        .fields
        .iter()
        .map(|(key, value)| (key.clone(), prod_value(value)))
        .collect();
    out.position = node.position;
    out.annotations = node
        .annotations
        .iter()
        .map(|(key, value)| (key.clone(), prod_value(value)))
        .collect();
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

fn prod_gedge(edge: &RefGEdge) -> GEdge {
    GEdge::new(edge.src, edge.tgt, prod_edge(&edge.label)).with_position(edge.position)
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
        out.insert_node(prod_node(node)).expect("distinct reference node id");
    }
    for (id, edge) in &graph.edges {
        out.insert_edge(EdgeId(*id), prod_gedge(edge))
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
        other => panic!("resolution key outside the lens differential: {other:?}"),
    }
}

/// The graph complement on the reference model, with the identity envelope
/// left out (each side stamps and checks its own).
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
        original_fields: complement
            .original_extra_fields
            .iter()
            .map(|(id, fields)| {
                (
                    *id,
                    fields
                        .iter()
                        .map(|(key, value)| (key.clone(), ref_value(value)))
                        .collect(),
                )
            })
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

fn without_identity(complement: &RefComplement) -> RefComplement {
    RefComplement {
        identity: None,
        ..complement.clone()
    }
}

fn prod_schema(schema: &RefSchema) -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol);
    for (vertex, kind) in &schema.vertices {
        builder = builder.vertex(vertex, kind, None).expect("reference vertex");
    }
    for edge in &schema.edges {
        builder = builder
            .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
            .expect("reference edge");
    }
    for (id, hyper) in &schema.hyper_edges {
        let signature: HashMap<String, String> = hyper
            .signature
            .iter()
            .map(|(label, vertex)| (label.clone(), vertex.clone()))
            .collect();
        builder = builder
            .hyper_edge(id, &hyper.kind, signature, &hyper.parent_label)
            .expect("reference hyper-edge");
    }
    assert!(
        schema.constraints.is_empty()
            && schema.required.is_empty()
            && schema.orderings.is_empty()
            && schema.entries.is_empty()
            && schema.keys.is_empty()
            && schema.references.is_empty()
            && schema.target_sets.is_empty(),
        "schema declarations outside the lens differential alphabet"
    );
    builder.build().expect("reference schema builds")
}

fn prod_migration(migration: &RefMigration) -> CompiledMigration {
    assert!(
        migration.conditional.is_empty() && migration.transformed.is_empty(),
        "value-layer stages are converted by their own stage constructors"
    );
    CompiledMigration {
        surviving_verts: migration.surviving_verts.iter().cloned().map(Into::into).collect(),
        surviving_edges: migration.surviving_edges.iter().map(prod_edge).collect(),
        vertex_remap: migration
            .vertex_remap
            .iter()
            .map(|(source, target)| (source.clone().into(), target.clone().into()))
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
                ((source.clone().into(), target.clone().into()), prod_edge(edge))
            })
            .collect(),
        expansion_path: migration
            .expansion_path
            .iter()
            .map(|((source, target), path)| {
                (
                    (source.clone().into(), target.clone().into()),
                    path.iter().cloned().map(Into::into).collect(),
                )
            })
            .collect(),
        ..CompiledMigration::default()
    }
}

// ---------------------------------------------------------------------------
// One typed reason map for refusals
// ---------------------------------------------------------------------------

/// The adapter's common name for a refusal. Two refusals agree only when
/// both sides map to the same reason.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Reason {
    PutDomain(String),
    Restrict(String),
    Identity(String),
}

fn view_ref(elem: &ViewRef) -> String {
    match elem {
        ViewRef::Node(id) => format!("Node({id})"),
        ViewRef::Edge(id) => format!("Edge({})", id.0),
        ViewRef::Arc { parent, child } => format!("Arc({parent},{child})"),
        ViewRef::Row(id) => format!("Row({})", id.0),
        ViewRef::Pair { edge, src, tgt } => {
            format!("Pair({:?},{},{})", ref_edge(edge), src.0, tgt.0)
        }
    }
}

fn ref_view_ref(elem: &panproto_reference::lens::ViewRef) -> String {
    use panproto_reference::lens::ViewRef as R;
    match elem {
        R::Node(id) => format!("Node({id})"),
        R::Edge(id) => format!("Edge({id})"),
        R::Row(id) => format!("Row({id})"),
        R::Pair(edge, src, tgt) => format!("Pair({edge:?},{src},{tgt})"),
    }
}

fn put_domain_reason(error: &PutDomainError) -> String {
    match error {
        PutDomainError::IdCollision { elem } => format!("IdCollision {}", view_ref(elem)),
        PutDomainError::SynthesizedEdited { elem } => {
            format!("SynthesizedEdited {}", view_ref(elem))
        }
        PutDomainError::ContractedEdgeInserted { elem } => {
            format!("ContractedEdgeInserted {}", view_ref(elem))
        }
        PutDomainError::UnrepresentableViewEdge { elem } => {
            format!("UnrepresentableViewEdge {}", view_ref(elem))
        }
        PutDomainError::AmbiguousReverseAnchor { elem, candidates } => format!(
            "AmbiguousReverseAnchor {} {:?}",
            view_ref(elem),
            candidates.iter().map(ToString::to_string).collect::<Vec<_>>()
        ),
        PutDomainError::SurvivalPredicateFails { elem } => {
            format!("SurvivalPredicateFails {}", view_ref(elem))
        }
        PutDomainError::DerivedCellEdited { elem, field } => {
            format!("DerivedCellEdited {} {field}", view_ref(elem))
        }
        PutDomainError::TransformNotInvertible { elem, field } => {
            format!("TransformNotInvertible {} {field}", view_ref(elem))
        }
        PutDomainError::MissingRequiredCell { elem, field } => {
            format!("MissingRequiredCell {} {field}", view_ref(elem))
        }
        PutDomainError::MalformedFan { node, reason } => {
            format!("MalformedFan {node} {reason:?}")
        }
        other => panic!("put-domain refusal outside the lens differential: {other:?}"),
    }
}

fn ref_put_domain_reason(error: &panproto_reference::lens::PutDomainError) -> String {
    use panproto_reference::lens::PutDomainError as R;
    match error {
        R::IdCollision { elem } => format!("IdCollision {}", ref_view_ref(elem)),
        R::SynthesizedEdited { elem } => format!("SynthesizedEdited {}", ref_view_ref(elem)),
        R::ContractedEdgeInserted { elem } => {
            format!("ContractedEdgeInserted {}", ref_view_ref(elem))
        }
        R::UnrepresentableViewEdge { elem } => {
            format!("UnrepresentableViewEdge {}", ref_view_ref(elem))
        }
        R::AmbiguousReverseAnchor { elem, candidates } => {
            format!("AmbiguousReverseAnchor {} {candidates:?}", ref_view_ref(elem))
        }
        R::SurvivalPredicateFails { elem } => {
            format!("SurvivalPredicateFails {}", ref_view_ref(elem))
        }
        R::DerivedCellEdited { elem, field } => {
            format!("DerivedCellEdited {} {field}", ref_view_ref(elem))
        }
        R::TransformNotInvertible { elem, field } => {
            format!("TransformNotInvertible {} {field}", ref_view_ref(elem))
        }
        R::MissingRequiredCell { elem, field } => {
            format!("MissingRequiredCell {} {field}", ref_view_ref(elem))
        }
        R::MalformedFan { node, reason } => format!("MalformedFan {node} {reason:?}"),
        other => panic!("reference put-domain refusal outside the lens differential: {other:?}"),
    }
}

fn restrict_reason(error: &RestrictError) -> String {
    match error {
        RestrictError::At { error, .. } => restrict_reason(error),
        RestrictError::IncoherentEdgeImage { source_edge, .. } => {
            format!("IncoherentEdgeImage {:?}", ref_edge(source_edge))
        }
        RestrictError::NoEdgeFound { src, tgt } => format!("NoEdgeFound {src} {tgt}"),
        other => format!("production restrict refusal {other:?}"),
    }
}

fn ref_restrict_reason(error: &RefRestrictError) -> String {
    match error {
        RefRestrictError::At { error, .. } => match error {
            EngineError::IncoherentEdgeImage { label } => format!("IncoherentEdgeImage {label:?}"),
            EngineError::NoEdgeFound { src, tgt } => format!("NoEdgeFound {src} {tgt}"),
            other => format!("reference engine refusal {other:?}"),
        },
        RefRestrictError::Plan(PlanError::IncoherentEdgeImage { source_edge, .. }) => {
            format!("IncoherentEdgeImage {source_edge:?}")
        }
        other => format!("reference restrict refusal {other:?}"),
    }
}

fn lens_reason(error: &LensError) -> Reason {
    match error {
        LensError::PutDomain(found) => Reason::PutDomain(put_domain_reason(found)),
        LensError::Restrict(found) => Reason::Restrict(restrict_reason(found)),
        LensError::Identity(found) => Reason::Identity(format!("{found:?}")),
        other => panic!("lens refusal outside the lens differential: {other:?}"),
    }
}

fn ref_lens_reason(error: &RefLensError) -> Reason {
    match error {
        RefLensError::PutDomain(found) => Reason::PutDomain(ref_put_domain_reason(found)),
        RefLensError::PutDomainReport(found) => {
            Reason::PutDomain(ref_put_domain_reason(&found[0]))
        }
        RefLensError::Identity(found) => Reason::Identity(format!("{found:?}")),
        RefLensError::TreeOperational(found) => Reason::Restrict(format!("{found:?}")),
    }
}

// ---------------------------------------------------------------------------
// A lens on both sides
// ---------------------------------------------------------------------------

/// One lens given as reference schemas and migration, built on both sides.
struct Pair {
    src: RefSchema,
    tgt: RefSchema,
    migration: RefMigration,
    plan: Result<Plan, PlanError>,
    production: Lens,
}

impl Pair {
    fn new(src: RefSchema, tgt: RefSchema, migration: RefMigration) -> Self {
        let plan = panproto_reference::restrict::compile(&src, &tgt, &migration);
        let production = Lens {
            compiled: prod_migration(&migration),
            src_schema: prod_schema(&src),
            tgt_schema: prod_schema(&tgt),
        };
        Self {
            src,
            tgt,
            migration,
            plan,
            production,
        }
    }

    fn with_reference<T>(&self, run: impl FnOnce(Result<&RefGraphLens<'_>, String>) -> T) -> T {
        match &self.plan {
            Ok(plan) => {
                let lens = RefGraphLens::new(
                    &self.src,
                    &self.tgt,
                    &self.migration,
                    plan,
                    &BTreeMap::new(),
                    &NoValueLayer,
                )
                .expect("plan compiled for these schemas");
                run(Ok(&lens))
            }
            Err(error) => run(Err(ref_restrict_reason(&RefRestrictError::Plan(
                error.clone(),
            )))),
        }
    }
}

/// The observable outcome of graph get: the view and the complement, or a
/// refusal.
type GetOutcome = Result<(RefGraph, RefComplement), Reason>;

fn production_get(pair: &Pair, source: &RefGraph) -> GetOutcome {
    graph_lens::get(&pair.production, &prod_graph(source))
        .map(|(view, complement)| (ref_graph(&view), ref_complement(&complement)))
        .map_err(|error| lens_reason(&error))
}

fn reference_get(pair: &Pair, source: &RefGraph) -> GetOutcome {
    pair.with_reference(|lens| match lens {
        Ok(lens) => panproto_reference::lens::get(lens, source)
            .map(|(view, complement)| (view, without_identity(&complement)))
            .map_err(|error| Reason::Restrict(ref_restrict_reason(&error))),
        Err(reason) => Err(Reason::Restrict(reason)),
    })
}

/// The observable outcome of graph put of an edited view against the
/// complement each side produced for the same source: the restored source,
/// or a refusal, together with the domain predicate's full report.
#[derive(Debug, PartialEq, Eq)]
struct PutOutcome {
    put: Result<RefGraph, Reason>,
    domain: Result<(), Vec<String>>,
}

fn production_put(pair: &Pair, source: &RefGraph, edited: &RefGraph) -> Option<PutOutcome> {
    let (_, complement) = graph_lens::get(&pair.production, &prod_graph(source)).ok()?;
    let view = prod_graph(edited);
    Some(PutOutcome {
        put: graph_lens::put(&pair.production, &view, &complement)
            .map(|restored| ref_graph(&restored))
            .map_err(|error| lens_reason(&error)),
        domain: graph_lens::check_put_domain(&pair.production, &view, &complement)
            .map_err(|errors| errors.iter().map(put_domain_reason).collect()),
    })
}

fn reference_put(pair: &Pair, source: &RefGraph, edited: &RefGraph) -> Option<PutOutcome> {
    pair.with_reference(|lens| {
        let lens = lens.ok()?;
        let (_, complement) = panproto_reference::lens::get(lens, source).ok()?;
        Some(PutOutcome {
            put: panproto_reference::lens::put(lens, edited, &complement)
                .map_err(|error| ref_lens_reason(&error)),
            domain: RefGraphLens::check_put_domain(lens, edited, &complement)
                .map_err(|errors| errors.iter().map(ref_put_domain_reason).collect()),
        })
    })
}

/// Exact put comparison, also checking that the strict graph put of the
/// reference is `GraphLens::put` itself.
fn reference_strict_put(pair: &Pair, source: &RefGraph, edited: &RefGraph) -> Option<PutOutcome> {
    pair.with_reference(|lens| {
        let lens = lens.ok()?;
        let (_, complement) = panproto_reference::lens::get(lens, source).ok()?;
        Some(PutOutcome {
            put: RefGraphLens::put(lens, edited, &complement)
                .map_err(|error| ref_lens_reason(&error)),
            domain: RefGraphLens::check_put_domain(lens, edited, &complement)
                .map_err(|errors| errors.iter().map(ref_put_domain_reason).collect()),
        })
    })
}

fn assert_agree(result: Result<u64, Disagreement>) -> u64 {
    match result {
        Ok(count) => count,
        Err(disagreement) => panic!("{disagreement}"),
    }
}

// ---------------------------------------------------------------------------
// GF5: the Delta-fragment cascade (L.4) on its exact finite scope
// ---------------------------------------------------------------------------

use panproto_reference::enumerate::lenses::{
    Gf5Fixture, for_each_gf5_deletion_edit, for_each_gf5_source, gf5_count_certificate,
};

fn gf5_pair(fixture: Gf5Fixture) -> Pair {
    Pair::new(
        fixture.source_schema(),
        fixture.target_schema(),
        fixture.migration(),
    )
}

/// The GF5 cascade law, compared on both sides: production put equals the
/// reference put, and both equal the reference's largest exact preimage,
/// which satisfies the universal property.
fn gf5_cascade_outcome(
    pair: &Pair,
    source: &RefGraph,
    edited: &RefGraph,
) -> (Option<PutOutcome>, Option<RefGraph>, bool) {
    let production = production_put(pair, source, edited);
    let (largest, universal) = pair.with_reference(|lens| {
        let lens = lens.expect("GF5 plans compile");
        let largest = panproto_reference::lens::largest_exact_preimage(lens, source, edited);
        let universal = production
            .as_ref()
            .and_then(|outcome| outcome.put.as_ref().ok())
            .is_some_and(|restored| {
                panproto_reference::lens::cascade_universal_property(
                    lens, source, edited, restored,
                )
            });
        (largest, universal)
    });
    (production, largest, universal)
}

#[test]
fn the_cascade_agrees_with_the_reference_on_the_gf5_scope() {
    let certificate = gf5_count_certificate();
    let mut sources = [0_usize; 2];
    for_each_gf5_source(|fixture, _| sources[usize::from(fixture == Gf5Fixture::VisibleThenHidden)] += 1);
    assert_eq!(sources, certificate.sources);

    let mut cases = Vec::new();
    for_each_gf5_deletion_edit(|fixture, source, edited| {
        cases.push((fixture, source.clone(), edited.clone()));
    });
    let edits: usize = certificate.deletion_edits.iter().sum();
    assert_eq!(cases.len(), edits);
    let probes: usize = certificate.source_subinstance_probes.iter().sum();
    let pairs = [
        gf5_pair(Gf5Fixture::DropTarget),
        gf5_pair(Gf5Fixture::VisibleThenHidden),
    ];
    let pair_of = |fixture: Gf5Fixture| &pairs[usize::from(fixture == Gf5Fixture::VisibleThenHidden)];
    let bound = format!(
        "GF5: fixtures=2, nodes<=3, edges<=3, sources={}, deletion_edits={edits}, source_subinstance_probes={probes}",
        sources.iter().sum::<usize>()
    );
    println!("{bound}");

    let compared = assert_agree(agree(
        &bound,
        cases.iter(),
        |(fixture, source, edited)| production_put(pair_of(*fixture), source, edited),
        |(fixture, source, edited)| reference_put(pair_of(*fixture), source, edited),
        |production, reference| production == reference,
    ));
    assert_eq!(compared, u64::try_from(edits).unwrap());

    let cascade = assert_agree(agree(
        &bound,
        cases.iter(),
        |(fixture, source, edited)| {
            let (production, _, universal) = gf5_cascade_outcome(pair_of(*fixture), source, edited);
            (production.and_then(|outcome| outcome.put.ok()), universal)
        },
        |(fixture, source, edited)| {
            let (_, largest, _) = gf5_cascade_outcome(pair_of(*fixture), source, edited);
            (largest, true)
        },
        |production, reference| production == reference,
    ));
    assert_eq!(cascade, u64::try_from(edits).unwrap());
}

#[test]
fn graph_get_agrees_with_the_reference_on_the_gf5_sources() {
    let pairs = [
        gf5_pair(Gf5Fixture::DropTarget),
        gf5_pair(Gf5Fixture::VisibleThenHidden),
    ];
    let mut cases = Vec::new();
    for_each_gf5_source(|fixture, source| cases.push((fixture, source.clone())));
    let bound = format!("GF5 sources={}", cases.len());
    println!("{bound}");
    let compared = assert_agree(agree(
        &bound,
        cases.iter(),
        |(fixture, source)| {
            production_get(&pairs[usize::from(*fixture == Gf5Fixture::VisibleThenHidden)], source)
        },
        |(fixture, source)| {
            reference_get(&pairs[usize::from(*fixture == Gf5Fixture::VisibleThenHidden)], source)
        },
        |production, reference| production == reference,
    ));
    assert_eq!(compared, 133);
    // GetPut through the strict reference put on every GF5 source.
    for (fixture, source) in &cases {
        let pair = &pairs[usize::from(*fixture == Gf5Fixture::VisibleThenHidden)];
        let (view, _) = reference_get(pair, source).expect("GF5 get is total");
        let production = production_put(pair, source, &view).expect("production get");
        let reference = reference_strict_put(pair, source, &view).expect("reference get");
        assert_eq!(production, reference);
        assert_eq!(reference.put, Ok(source.clone()));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::default())]
    #[test]
    fn randomized_lens_law_differential(index in 0_usize..604) {
        let pairs = [
            gf5_pair(Gf5Fixture::DropTarget),
            gf5_pair(Gf5Fixture::VisibleThenHidden),
        ];
        let mut found = None;
        let mut at = 0;
        for_each_gf5_deletion_edit(|fixture, source, edited| {
            if at == index {
                found = Some((fixture, source.clone(), edited.clone()));
            }
            at += 1;
        });
        let (fixture, source, edited) = found.expect("index in scope");
        let pair = &pairs[usize::from(fixture == Gf5Fixture::VisibleThenHidden)];
        prop_assert_eq!(production_put(pair, &source, &edited), reference_put(pair, &source, &edited));
    }
}
