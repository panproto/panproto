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
use panproto_reference::enumerate::lenses::{
    C11_FIELD, C11_VALUE, C11AddField, C11DropField, C11Layer, C11OpTerm, C11ReadField, C11Stage,
    C11SurvivalFalse,
};
use panproto_reference::harness::{Disagreement, agree};
use panproto_reference::lens::{GraphLens as RefGraphLens, LensError as RefLensError};
use panproto_reference::migration::Migration as RefMigration;
use panproto_reference::model::{
    GEdge as RefGEdge, Graph as RefGraph, Node as RefNode, NodeShape as RefNodeShape,
    SEdge as RefSEdge,
};
use panproto_reference::restrict::{
    Complement as RefComplement, EngineError, NoValueLayer, Plan, PlanError,
    RestrictError as RefRestrictError, SynthKey as RefSynthKey, ValueLayer,
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
        out.insert_node(prod_node(node))
            .expect("distinct reference node id");
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
    if *schema == RefSchema::default() {
        // The builder refuses a schema with no vertex; the empty schema is
        // the default value.
        return Schema::default();
    }
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
        surviving_verts: migration
            .surviving_verts
            .iter()
            .cloned()
            .map(Into::into)
            .collect(),
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
                (
                    (source.clone().into(), target.clone().into()),
                    prod_edge(edge),
                )
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
        hyper_resolver: migration
            .hyper_resolver
            .iter()
            .map(|((hyper, labels), (target, remap))| {
                (
                    (
                        hyper.clone().into(),
                        labels.iter().cloned().map(Into::into).collect(),
                    ),
                    (
                        target.clone().into(),
                        remap
                            .iter()
                            .map(|(from, to)| (from.clone().into(), to.clone().into()))
                            .collect(),
                    ),
                )
            })
            .collect(),
        ..CompiledMigration::default()
    }
}

/// The production lens of a C11 stage: the structural migration, plus the
/// production value layer its tag names.
fn production_stage(stage: &C11Stage) -> Lens {
    let mut structural = stage.migration.clone();
    structural.conditional.clear();
    structural.transformed.clear();
    let mut compiled = prod_migration(&structural);
    match stage.layer {
        C11Layer::None => {}
        C11Layer::SurvivalFalse => {
            for vertex in &stage.migration.conditional {
                compiled.conditional_survival.insert(
                    vertex.clone().into(),
                    panproto_expr::Expr::Lit(panproto_expr::Literal::Bool(false)),
                );
            }
        }
        C11Layer::AddField => {
            for vertex in &stage.migration.transformed {
                compiled.field_transforms.insert(
                    vertex.clone().into(),
                    vec![panproto_inst::FieldTransform::AddField {
                        key: C11_FIELD.to_owned(),
                        value: prod_value(&C11_VALUE),
                    }],
                );
            }
        }
        C11Layer::DropField => {
            for vertex in &stage.migration.transformed {
                compiled.field_transforms.insert(
                    vertex.clone().into(),
                    vec![panproto_inst::FieldTransform::DropField {
                        key: C11_FIELD.to_owned(),
                    }],
                );
            }
        }
        C11Layer::ReadField => {
            for vertex in &stage.migration.transformed {
                compiled.field_transforms.insert(
                    vertex.clone().into(),
                    vec![panproto_inst::FieldTransform::ApplyExpr {
                        key: C11_FIELD.to_owned(),
                        expr: panproto_expr::Expr::Var(C11_FIELD.into()),
                        inverse: Some(panproto_expr::Expr::Var(C11_FIELD.into())),
                        coercion_class: panproto_gat::CoercionClass::Iso,
                    }],
                );
            }
        }
        C11Layer::OpTerm => {
            for vertex in &stage.migration.transformed {
                compiled.op_term_assignments.insert(
                    vertex.clone().into(),
                    vec![panproto_inst::TermAssignment::Compute {
                        target: C11_FIELD.to_owned(),
                        scope: panproto_inst::TermScope::Row,
                        term: panproto_expr::Expr::Lit(panproto_expr::Literal::Int(0)),
                        inverse: None,
                        coercion_class: panproto_gat::CoercionClass::Projection,
                    }],
                );
            }
        }
    }
    Lens {
        compiled,
        src_schema: prod_schema(&stage.source),
        tgt_schema: prod_schema(&stage.target),
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
            candidates
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
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
            format!(
                "AmbiguousReverseAnchor {} {candidates:?}",
                ref_view_ref(elem)
            )
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
        RefLensError::PutDomainReport(found) => Reason::PutDomain(ref_put_domain_reason(&found[0])),
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
    layer: &'static dyn ValueLayer,
    production: Lens,
}

impl Pair {
    fn new(src: RefSchema, tgt: RefSchema, migration: RefMigration) -> Self {
        let production = Lens {
            compiled: prod_migration(&migration),
            src_schema: prod_schema(&src),
            tgt_schema: prod_schema(&tgt),
        };
        Self::with_production(src, tgt, migration, &NoValueLayer, production)
    }

    fn with_production(
        src: RefSchema,
        tgt: RefSchema,
        migration: RefMigration,
        layer: &'static dyn ValueLayer,
        production: Lens,
    ) -> Self {
        let plan = panproto_reference::restrict::compile(&src, &tgt, &migration);
        Self {
            src,
            tgt,
            migration,
            plan,
            layer,
            production,
        }
    }

    /// A C11 stage on both sides. The value layer is built on each side
    /// from the stage's layer tag: the production expression or transform,
    /// and the reference's caller-supplied layer.
    fn from_stage(stage: &C11Stage) -> Self {
        let layer: &'static dyn ValueLayer = match stage.layer {
            C11Layer::None => &NoValueLayer,
            C11Layer::SurvivalFalse => &C11SurvivalFalse,
            C11Layer::AddField => &C11AddField,
            C11Layer::DropField => &C11DropField,
            C11Layer::ReadField => &C11ReadField,
            C11Layer::OpTerm => &C11OpTerm,
        };
        Self::with_production(
            stage.source.clone(),
            stage.target.clone(),
            stage.migration.clone(),
            layer,
            production_stage(stage),
        )
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
                    self.layer,
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

/// [`agree`] on every case of every unit, spread over the machine's threads.
///
/// Each thread builds its own context once (the lens pairs hold value
/// layers that are not shared between threads), then repeatedly claims the
/// next block of units, expands each unit into its cases and runs [`agree`]
/// on them, so a disagreement is reported with its own case. Every thread
/// stops at the first disagreement any thread finds; of the disagreements
/// found, the one at the lowest unit is returned. Otherwise the result is
/// the number of cases compared, which is independent of the thread count.
fn agree_in_parallel<'a, U, C, I, P, R>(
    bound: &str,
    units: &'a [U],
    context: impl Fn() -> C + Sync,
    expand: impl Fn(&C, &'a U) -> Vec<I> + Sync,
    production: impl Fn(&C, &I) -> P + Sync,
    reference: impl Fn(&C, &I) -> R + Sync,
    eq: impl Fn(&P, &R) -> bool + Sync,
) -> Result<u64, Disagreement>
where
    U: Sync,
    I: std::fmt::Debug,
    P: std::fmt::Debug,
    R: std::fmt::Debug,
{
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    const BLOCK: usize = 32;
    let threads = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let outcomes: Vec<Result<u64, (usize, Disagreement)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let context = context();
                    let mut compared = 0_u64;
                    loop {
                        let start = next.fetch_add(BLOCK, Ordering::Relaxed);
                        if start >= units.len() || stop.load(Ordering::Relaxed) {
                            return Ok(compared);
                        }
                        for (offset, unit) in units[start..].iter().take(BLOCK).enumerate() {
                            match agree(
                                bound,
                                expand(&context, unit),
                                |case| production(&context, case),
                                |case| reference(&context, case),
                                &eq,
                            ) {
                                Ok(count) => compared += count,
                                Err(disagreement) => {
                                    stop.store(true, Ordering::Relaxed);
                                    return Err((start + offset, disagreement));
                                }
                            }
                        }
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("a comparison thread panicked"))
            .collect()
    });
    let mut compared = 0_u64;
    let mut first: Option<(usize, Disagreement)> = None;
    for outcome in outcomes {
        match outcome {
            Ok(count) => compared += count,
            Err((unit, disagreement)) => {
                if first.as_ref().is_none_or(|(at, _)| unit < *at) {
                    first = Some((unit, disagreement));
                }
            }
        }
    }
    first.map_or(Ok(compared), |(_, disagreement)| Err(disagreement))
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
                panproto_reference::lens::cascade_universal_property(lens, source, edited, restored)
            });
        (largest, universal)
    });
    (production, largest, universal)
}

#[test]
fn the_cascade_agrees_with_the_reference_on_the_gf5_scope() {
    let certificate = gf5_count_certificate();
    let mut sources = [0_usize; 2];
    for_each_gf5_source(|fixture, _| {
        sources[usize::from(fixture == Gf5Fixture::VisibleThenHidden)] += 1
    });
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
    let pair_of =
        |fixture: Gf5Fixture| &pairs[usize::from(fixture == Gf5Fixture::VisibleThenHidden)];
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
            production_get(
                &pairs[usize::from(*fixture == Gf5Fixture::VisibleThenHidden)],
                source,
            )
        },
        |(fixture, source)| {
            reference_get(
                &pairs[usize::from(*fixture == Gf5Fixture::VisibleThenHidden)],
                source,
            )
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

// ---------------------------------------------------------------------------
// Tables (T.4): get, strict put and refusals on the L.8 fixture scope, and
// the last-writer rule on the writer carrier
// ---------------------------------------------------------------------------

use panproto_inst::{BoundaryProtocols, FInstance, RowId, TableComplement};
use panproto_lens::table_lens;
use panproto_reference::enumerate::lenses::{
    TABLE_FIXTURE_TABLE_KIND, TABLE_FIXTURE_VALUE_KIND, TableFixture, WRITER_EDITS,
    WRITER_VALUE_KIND, WriterFixture, for_each_table_fixture, for_each_writer_fixture,
    table_fixture_count_certificate, table_view_edits, table_view_edits_count,
    writer_count_certificate,
};
use panproto_reference::model::Table as RefTable;
use panproto_reference::rel::RelSchema;
use panproto_reference::table::{
    DroppedRow as RefDroppedRow, TableCheckError, TableComplement as RefTableComplement,
    TableLens as RefTableLens, TablePlan, TablePlanError, TablePutError,
};
use panproto_schema::ScalarKind;

fn table_protocol(value_kind: &str, scalar: ScalarKind) -> Protocol {
    Protocol {
        value_kinds: BTreeMap::from([(value_kind.to_owned(), scalar)]),
        non_value_kinds: std::collections::BTreeSet::from([TABLE_FIXTURE_TABLE_KIND.to_owned()]),
        ..Protocol::default()
    }
}

fn ref_table(table: &FInstance) -> RefTable {
    let mut out = RefTable::default();
    for (name, _) in table.tables() {
        for (id, row) in table.rows(name) {
            out.rows.insert(id.0, (name.to_string(), ref_fields(row)));
        }
    }
    for (edge, pairs) in table.foreign_keys() {
        for (src, tgt) in pairs {
            out.pairs.insert((ref_edge(edge), src.0, tgt.0));
        }
    }
    out
}

fn prod_table(table: &RefTable) -> FInstance {
    let mut out = FInstance::new();
    for (id, (name, cells)) in &table.rows {
        out.insert_row(
            name.as_str(),
            RowId(*id),
            cells
                .iter()
                .map(|(key, value)| (key.clone(), prod_value(value)))
                .collect(),
        )
        .expect("distinct reference row id");
    }
    for (edge, src, tgt) in &table.pairs {
        out.insert_foreign_key(prod_edge(edge), RowId(*src), RowId(*tgt))
            .expect("reference pair between reference rows");
    }
    out
}

fn ref_table_complement(complement: &TableComplement) -> RefTableComplement {
    RefTableComplement {
        dropped_rows: complement
            .dropped_rows
            .iter()
            .map(|(id, dropped)| {
                (
                    id.0,
                    RefDroppedRow {
                        table: dropped.table.to_string(),
                        row: ref_fields(&dropped.row),
                    },
                )
            })
            .collect(),
        dropped_cells: complement
            .dropped_cells
            .iter()
            .map(|((id, column), value)| ((id.0, column.clone()), ref_value(value)))
            .collect(),
        dropped_fk_pairs: complement
            .dropped_fk_pairs
            .iter()
            .map(|(edge, pairs)| {
                (
                    ref_edge(edge),
                    pairs.iter().map(|(s, t)| (s.0, t.0)).collect(),
                )
            })
            .collect(),
        original_fk_edges: complement
            .original_fk_edges
            .iter()
            .map(|((edge, s, t), sources)| {
                (
                    (ref_edge(edge), s.0, t.0),
                    sources.iter().map(ref_edge).collect(),
                )
            })
            .collect(),
        original_tables: complement
            .original_tables
            .iter()
            .map(|(id, table)| (id.0, table.to_string()))
            .collect(),
        original_cells: complement
            .original_cells
            .iter()
            .map(|((id, column), value)| ((id.0, column.clone()), ref_value(value)))
            .collect(),
        synthesized_fk_pairs: complement
            .synthesized_fk_pairs
            .iter()
            .map(|(edge, s, t)| (ref_edge(edge), s.0, t.0))
            .collect(),
        id_floor: complement.id_floor,
        identity: None,
    }
}

fn ref_table_put_reason(error: &TablePutError) -> Reason {
    match error {
        TablePutError::Domain(found) => Reason::PutDomain(ref_put_domain_reason(found)),
        TablePutError::Identity(found) => Reason::Identity(format!("{found:?}")),
        other => Reason::Restrict(format!("reference table refusal {other:?}")),
    }
}

/// One table lens given as reference schemas and migration, on both sides.
struct TablePair {
    src: RefSchema,
    tgt: RefSchema,
    migration: RefMigration,
    boundaries: (RelSchema, RelSchema),
    plan: Result<TablePlan, TablePlanError>,
    production: Lens,
    protocol: Protocol,
}

impl TablePair {
    fn new(
        src: RefSchema,
        tgt: RefSchema,
        migration: RefMigration,
        value_kind: &str,
        scalar: ScalarKind,
    ) -> Self {
        let kinds = |kind: &str| std::collections::BTreeSet::from([kind.to_owned()]);
        let classify = |schema: &RefSchema| {
            panproto_reference::rel::classify(
                schema,
                &kinds(value_kind),
                &kinds(TABLE_FIXTURE_TABLE_KIND),
            )
            .expect("fixture schemas are relational")
        };
        let boundaries = (classify(&src), classify(&tgt));
        let plan = panproto_reference::table::compile_for_tables(
            &src,
            &tgt,
            &boundaries.0,
            &boundaries.1,
            &migration,
        );
        let production = Lens {
            compiled: prod_migration(&migration),
            src_schema: prod_schema(&src),
            tgt_schema: prod_schema(&tgt),
        };
        Self {
            src,
            tgt,
            migration,
            boundaries,
            plan,
            production,
            protocol: table_protocol(value_kind, scalar),
        }
    }

    fn protocols(&self) -> BoundaryProtocols<'_> {
        BoundaryProtocols {
            src: &self.protocol,
            tgt: &self.protocol,
        }
    }

    fn with_reference<T>(&self, run: impl FnOnce(&RefTableLens<'_>, &TablePlan) -> T) -> T {
        let plan = self.plan.as_ref().expect("fixture table plans compile");
        let lens = RefTableLens::new(
            &self.src,
            &self.tgt,
            panproto_reference::table::TableBoundaries::new(&self.boundaries.0, &self.boundaries.1),
            &self.migration,
            plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .expect("plan compiled for these boundaries");
        run(&lens, plan)
    }
}

type TableGetOutcome = Result<(RefTable, RefTableComplement), Reason>;

fn production_table_get(pair: &TablePair, source: &RefTable) -> TableGetOutcome {
    table_lens::get(&pair.production, &prod_table(source), pair.protocols())
        .map(|(view, complement)| (ref_table(&view), ref_table_complement(&complement)))
        .map_err(|error| lens_reason(&error))
}

fn reference_table_get(pair: &TablePair, source: &RefTable) -> TableGetOutcome {
    pair.with_reference(|lens, plan| {
        panproto_reference::table::get(
            &pair.src,
            &pair.tgt,
            &pair.migration,
            plan,
            &NoValueLayer,
            source,
        )
        .map(|(view, complement)| {
            assert_eq!(
                lens.get(source).map(|(bound, _)| bound),
                Ok(view.clone()),
                "the bound and free reference table gets agree"
            );
            (
                view,
                RefTableComplement {
                    identity: None,
                    ..complement
                },
            )
        })
        .map_err(|error| Reason::Restrict(ref_restrict_reason(&error)))
    })
}

#[derive(Debug, PartialEq, Eq)]
struct TablePutOutcome {
    put: Result<RefTable, Reason>,
    domain: Result<(), Vec<String>>,
}

fn production_table_put(pair: &TablePair, source: &RefTable, edited: &RefTable) -> TablePutOutcome {
    let (_, complement) = table_lens::get(&pair.production, &prod_table(source), pair.protocols())
        .expect("fixture gets are total");
    let view = prod_table(edited);
    TablePutOutcome {
        put: table_lens::put(&pair.production, &view, &complement, pair.protocols())
            .map(|restored| ref_table(&restored))
            .map_err(|error| lens_reason(&error)),
        domain: table_lens::check_put_domain(
            &pair.production,
            &view,
            &complement,
            pair.protocols(),
        )
        .map_err(|errors| errors.iter().map(put_domain_reason).collect()),
    }
}

fn reference_table_put(pair: &TablePair, source: &RefTable, edited: &RefTable) -> TablePutOutcome {
    pair.with_reference(|lens, _| {
        let (_, complement) = lens.get(source).expect("fixture gets are total");
        TablePutOutcome {
            put: RefTableLens::put(lens, edited, &complement)
                .map_err(|error| ref_table_put_reason(&error)),
            domain: match panproto_reference::table::check_put_domain(lens, edited, &complement) {
                Ok(()) => Ok(()),
                Err(TableCheckError::Domain { violations, .. }) => {
                    Err(violations.iter().map(ref_put_domain_reason).collect())
                }
                Err(other) => Err(vec![format!("reference table check {other:?}")]),
            },
        }
    })
}

fn fixture_pair(fixture: &TableFixture) -> TablePair {
    TablePair::new(
        fixture.source_schema(),
        fixture.target_schema(),
        fixture.migration(),
        TABLE_FIXTURE_VALUE_KIND,
        ScalarKind::Any,
    )
}

/// The table lens of every fixture shape, indexed by column count and
/// retained-column mask.
fn table_pairs() -> Vec<Vec<TablePair>> {
    (0..=2_usize)
        .map(|columns| {
            (0..(1_usize << columns))
                .map(|retained| {
                    fixture_pair(&TableFixture {
                        columns,
                        retained,
                        source: RefTable::default(),
                    })
                })
                .collect()
        })
        .collect()
}

#[test]
fn table_get_and_put_agree_with_the_reference_on_the_fixture_scope() {
    let certificate = table_fixture_count_certificate();
    let mut fixtures = Vec::new();
    for_each_table_fixture(|fixture| fixtures.push(fixture));
    assert_eq!(fixtures.len(), certificate.total);
    let pairs = table_pairs();
    let pair_of = |fixture: &TableFixture| &pairs[fixture.columns][fixture.retained];
    let bound = format!(
        "table fixtures: rows<=2, columns<=2, cell alphabet=7 (absence included), lenses=7, source/lens cases={}",
        certificate.total
    );
    println!("{bound}");
    let gets = assert_agree(agree_in_parallel(
        &bound,
        &fixtures,
        table_pairs,
        |_, fixture| vec![fixture],
        |pairs, fixture| {
            production_table_get(&pairs[fixture.columns][fixture.retained], &fixture.source)
        },
        |pairs, fixture| {
            reference_table_get(&pairs[fixture.columns][fixture.retained], &fixture.source)
        },
        |production, reference| production == reference,
    ));
    assert_eq!(gets, u64::try_from(certificate.total).unwrap());

    let mut cases = Vec::new();
    let mut expected_edits = 0_usize;
    for fixture in &fixtures {
        let (view, complement) =
            reference_table_get(pair_of(fixture), &fixture.source).expect("fixture gets are total");
        let columns: Vec<String> = (0..fixture.columns)
            .filter(|column| fixture.retained & (1 << column) != 0)
            .map(|column| format!("c{column}"))
            .collect();
        let reserved: Vec<u32> = complement.dropped_rows.keys().copied().collect();
        let edits = table_view_edits(&view, &columns, &reserved);
        expected_edits += table_view_edits_count(view.rows.len(), columns.len(), reserved.len());
        cases.push((fixture, view));
        for edited in edits {
            cases.push((fixture, edited));
        }
    }
    assert_eq!(cases.len(), certificate.total + expected_edits);
    let bound = format!(
        "{bound}; unedited views={}, one-step view edits={expected_edits}",
        certificate.total
    );
    println!("{bound}");
    let puts = assert_agree(agree_in_parallel(
        &bound,
        &cases,
        table_pairs,
        |_, case| vec![case],
        |pairs, (fixture, edited)| {
            production_table_put(
                &pairs[fixture.columns][fixture.retained],
                &fixture.source,
                edited,
            )
        },
        |pairs, (fixture, edited)| {
            reference_table_put(
                &pairs[fixture.columns][fixture.retained],
                &fixture.source,
                edited,
            )
        },
        |production, reference| production == reference,
    ));
    assert_eq!(puts, u64::try_from(cases.len()).unwrap());
}

fn writer_pair(fixture: &WriterFixture) -> TablePair {
    TablePair::new(
        fixture.source_schema(),
        fixture.target_schema(),
        fixture.migration(),
        WRITER_VALUE_KIND,
        ScalarKind::Int {
            bits: 64,
            signed: true,
        },
    )
}

#[test]
fn table_writers_agree_with_the_reference_on_the_writer_carrier() {
    let (mappings, rows, edits) = writer_count_certificate();
    let mut fixtures = Vec::new();
    for_each_writer_fixture(|fixture| fixtures.push(fixture));
    assert_eq!(fixtures.len(), rows);
    let mut cases = Vec::new();
    for fixture in &fixtures {
        for target in fixture.target_columns() {
            for edit in WRITER_EDITS {
                cases.push((fixture, target, edit));
            }
        }
    }
    assert_eq!(cases.len(), edits);
    let bound = format!(
        "table writer: source_columns=2, image_choices=3, mappings={mappings}, value_alphabet=3, source_rows={rows}, edit_alphabet=2, edited_views={edits}"
    );
    println!("{bound}");
    let compared = assert_agree(agree(
        &bound,
        cases.iter(),
        |(fixture, target, edit)| {
            let pair = writer_pair(fixture);
            let source = fixture.source();
            let (mut view, _) = production_table_get(&pair, &source).expect("writer get");
            view.rows
                .get_mut(&0)
                .unwrap()
                .1
                .insert((*target).to_owned(), RefValue::Int(*edit));
            production_table_put(&pair, &source, &view).put
        },
        |(fixture, target, edit)| {
            let pair = writer_pair(fixture);
            let source = fixture.source();
            pair.with_reference(|lens, plan| {
                let (mut view, complement) = lens.get(&source).expect("writer get");
                view.rows
                    .get_mut(&0)
                    .unwrap()
                    .1
                    .insert((*target).to_owned(), RefValue::Int(*edit));
                let restored = RefTableLens::put(lens, &view, &complement)
                    .map_err(|error| ref_table_put_reason(&error));
                // The last writer of the edited column is the column put writes.
                let writer = panproto_reference::table::TablePlan::writer(plan, "t", target)
                    .expect("an image column has a writer");
                let mut expected = source.clone();
                expected
                    .rows
                    .get_mut(&0)
                    .unwrap()
                    .1
                    .insert(writer, RefValue::Int(*edit));
                assert_eq!(
                    restored.as_ref(),
                    Ok(&expected),
                    "reference put writes the last writer"
                );
                restored
            })
        },
        |production, reference| production == reference,
    ));
    assert_eq!(compared, u64::try_from(edits).unwrap());
}

// ---------------------------------------------------------------------------
// The five law gates (L.8): production checker against the reference checker
// ---------------------------------------------------------------------------

use panproto_lens::LawViolation;
use panproto_reference::lens::{LawGateStatus, LawReport as RefLawReport};

/// The comparable part of a five-gate report: the first gate that does not
/// pass, or `None` when every gate passes. The two checkers draw their
/// Single-Step Bases independently (L.8 fixes the families, not one
/// alternate payload per coordinate), so basis sizes are not compared.
#[derive(Debug, PartialEq, Eq)]
struct LawOutcome {
    first_failure: Option<String>,
}

fn production_law_outcome(report: &Result<panproto_lens::LawReport, LawViolation>) -> LawOutcome {
    LawOutcome {
        first_failure: report.as_ref().err().map(|violation| {
            match violation {
                LawViolation::GetPut { .. } => "GetPut",
                LawViolation::PutGet { .. } | LawViolation::WeakPutGet { .. } => "PutGet",
                LawViolation::Totality { .. } => "Totality",
                LawViolation::PutTwice { .. } => "PutTwice",
                LawViolation::Congruence { .. } => "Congruence",
                LawViolation::Error(_) => "Refusal",
                other => panic!("non-gating violation from the five gates: {other:?}"),
            }
            .to_owned()
        }),
    }
}

fn reference_law_outcome<R: std::fmt::Debug>(report: &RefLawReport<R>) -> LawOutcome {
    LawOutcome {
        first_failure: report
            .gates
            .iter()
            .find(|gate| !matches!(gate.status, LawGateStatus::Passed { .. }))
            .map(|gate| match (&gate.status, gate.gate) {
                // Production reports an operation refused inside a gate as
                // `LawViolation::Error` without naming the gate, except
                // inside Totality, where the refusal is the violation.
                (
                    LawGateStatus::Failed(panproto_reference::lens::LawFailure::Refusal { .. }),
                    gate,
                ) if gate != panproto_reference::lens::LawGate::Totality => "Refusal".to_owned(),
                (_, gate) => format!("{gate:?}"),
            }),
    }
}

#[test]
fn graph_laws_agree_with_the_reference_on_the_gf5_sources() {
    let pairs = [
        gf5_pair(Gf5Fixture::DropTarget),
        gf5_pair(Gf5Fixture::VisibleThenHidden),
    ];
    let mut cases = Vec::new();
    for_each_gf5_source(|fixture, source| cases.push((fixture, source.clone())));
    let bound = format!("GF5 law reports: sources={}", cases.len());
    println!("{bound}");
    let compared = assert_agree(agree(
        &bound,
        cases.iter(),
        |(fixture, source)| {
            let pair = &pairs[usize::from(*fixture == Gf5Fixture::VisibleThenHidden)];
            production_law_outcome(&graph_lens::check_laws(
                &pair.production,
                &prod_graph(source),
            ))
        },
        |(fixture, source)| {
            let pair = &pairs[usize::from(*fixture == Gf5Fixture::VisibleThenHidden)];
            pair.with_reference(|lens| {
                reference_law_outcome(&panproto_reference::lens::check_graph_laws(
                    lens.expect("GF5 plans compile"),
                    source,
                ))
            })
        },
        |production, reference| production == reference,
    ));
    assert_eq!(compared, 133);
}

// ---------------------------------------------------------------------------
// The finite partial-lens carrier: the production five-gate checker against
// the reference checker on every lens and source
// ---------------------------------------------------------------------------

use panproto_inst::SynthIndex;
use panproto_lens::laws::{LawShape, Mutation, MutationCases, SmallScope};
use panproto_lens::{DerivedMap, LensShape, SkipReason};
use panproto_reference::enumerate::lenses::{
    PARTIAL_LENS_VIEWS, PartialLens, for_each_partial_lens, partial_lens_count_certificate,
    partial_lens_law_report,
};

/// The production law shape of one finite partial lens, which the context
/// carries. States, views and complements are the carrier's small numbers;
/// an undefined put is an operational refusal, never a domain refusal, so
/// the production Totality gate sees it.
struct PartialShape;

fn undefined_put(view: u8, complement: u8) -> LensError {
    LensError::ComplementMismatch {
        detail: format!("partial put undefined at view {view}, complement {complement}"),
    }
}

impl LensShape for PartialShape {
    type Instance = u8;
    type Complement = u8;
    type Context = PartialLens;

    fn get(_lens: &Lens, source: &u8, ctx: &PartialLens) -> Result<(u8, u8), LensError> {
        let (view, complement) = ctx.get_table()[usize::from(*source)];
        Ok((
            u8::try_from(view).unwrap(),
            u8::try_from(complement).unwrap(),
        ))
    }

    fn put(_lens: &Lens, view: &u8, complement: &u8, ctx: &PartialLens) -> Result<u8, LensError> {
        ctx.put(usize::from(*view), usize::from(*complement))
            .map(|source| u8::try_from(source).unwrap())
            .ok_or_else(|| undefined_put(*view, *complement))
    }

    fn check_put_domain(
        _lens: &Lens,
        _view: &u8,
        _complement: &u8,
        _ctx: &PartialLens,
    ) -> Result<(), Vec<PutDomainError>> {
        Ok(())
    }

    fn synth_index(_instance: &u8, _complement: &u8) -> SynthIndex {
        SynthIndex::default()
    }

    fn equivalent(a: &u8, _ia: &SynthIndex, b: &u8, _ib: &SynthIndex) -> bool {
        a == b
    }

    fn divergence(a: &u8, _ia: &SynthIndex, b: &u8, _ib: &SynthIndex) -> Option<String> {
        (a != b).then(|| format!("{a} != {b}"))
    }

    fn divergence_modulo_derived(
        a: &u8,
        ia: &SynthIndex,
        b: &u8,
        ib: &SynthIndex,
        _derived: &DerivedMap,
    ) -> Option<String> {
        Self::divergence(a, ia, b, ib)
    }

    fn complement_divergence(a: &u8, b: &u8) -> Option<String> {
        (a != b).then(|| format!("{a} != {b}"))
    }

    fn congruence_witnesses(_instance: &u8, _index: &SynthIndex) -> Vec<(u8, SynthIndex)> {
        Vec::new()
    }
}

impl LawShape for PartialShape {
    fn mutation_cases(
        _lens: &Lens,
        _source: &u8,
        view: &u8,
        _complement: &u8,
        _ctx: &PartialLens,
        mutation: Mutation,
    ) -> MutationCases<u8> {
        if mutation == Mutation::ValueEdit {
            MutationCases::Cases(vec![u8::try_from(PARTIAL_LENS_VIEWS - 1).unwrap() - view])
        } else {
            MutationCases::Skip(SkipReason::ShapeCannotExpress)
        }
    }

    fn identical(a: &u8, b: &u8) -> bool {
        a == b
    }

    fn congruent_states(_view: &u8, _complement: &u8) -> Vec<(u8, u8)> {
        Vec::new()
    }

    fn rekey_complement(complement: &u8, _from: &SynthIndex, _to: &SynthIndex) -> u8 {
        *complement
    }

    fn locality_divergence(
        _lens: &Lens,
        _source: &u8,
        _original_view: &u8,
        _edited_view: &u8,
        _restored: &u8,
        _complement: &u8,
        _ctx: &PartialLens,
    ) -> Option<String> {
        None
    }

    fn carrier_intersection(a: &u8, b: &u8) -> u8 {
        (*a).min(*b)
    }

    fn undo_view(original: &u8, edited: &u8) -> Option<u8> {
        (original != edited).then_some(*original)
    }

    fn enumerate_small(_schema: &Schema, _scope: SmallScope, _ctx: &PartialLens) -> Vec<u8> {
        Vec::new()
    }
}

fn empty_lens() -> Lens {
    let schema = prod_schema(&RefSchema::default().vertex("a", "obj"));
    Lens {
        compiled: CompiledMigration::default(),
        src_schema: schema.clone(),
        tgt_schema: schema,
    }
}

#[test]
fn partial_lens_laws_agree_with_the_reference_on_every_lens_and_source() {
    let mut cases = Vec::new();
    for_each_partial_lens(|lens| {
        for source in 0..2_u8 {
            cases.push((lens, source));
        }
    });
    let lenses = partial_lens_count_certificate();
    assert_eq!(cases.len(), 2 * lenses);
    let bound = format!(
        "partial lenses: sources=2, views=2, complements=3, injective total gets=30, put tables=729, lenses={lenses}, lens/source cases={}",
        cases.len()
    );
    println!("{bound}");
    let carrier = empty_lens();
    let compared = assert_agree(agree(
        &bound,
        cases.iter(),
        |(lens, source)| {
            production_law_outcome(&panproto_lens::laws::check_laws_report::<PartialShape>(
                &carrier, source, lens,
            ))
        },
        |(lens, source)| {
            reference_law_outcome(&partial_lens_law_report(lens, usize::from(*source)))
        },
        |production, reference| production == reference,
    ));
    assert_eq!(compared, 43_740);
}

// ---------------------------------------------------------------------------
// The C11 state scope: every stage, every state over its source schema and
// every one-step edit of its view
// ---------------------------------------------------------------------------

use panproto_reference::enumerate::lenses::{
    c11_fan_schema, c11_fan_state_count, c11_fan_states, c11_graph_edit_count, c11_graph_edits,
    c11_graph_state_count, c11_graph_states, c11_path_count_certificate, c11_stages, sigma2_count,
    sigma2_schemas,
};

/// The states a stage reads: the fan states for a stage over the fan
/// schema, otherwise the graph states over its source schema.
fn c11_sources(stage: &C11Stage) -> Vec<RefGraph> {
    if stage.source == c11_fan_schema() {
        c11_fan_states()
    } else {
        c11_graph_states(&stage.source)
    }
}

/// One case of the C11 graph scope: the get of a stage on a state, or the
/// put of one view (unedited or one-step edited) against that get's
/// complement. States are named by their index among the stage's sources.
#[derive(Debug)]
enum GraphCase<'a> {
    Get(usize, &'a RefGraph),
    Put(usize, &'a RefGraph, RefGraph),
}

/// The outcome of one graph case: a get outcome, or a put outcome on both
/// the module put and the strict lens put.
#[derive(Debug, PartialEq, Eq)]
enum GraphCaseOutcome {
    Get(GetOutcome),
    Put(Option<PutOutcome>),
}

/// The source states of every stage, each checked against its independent
/// count.
fn c11_stage_sources(stages: &[C11Stage]) -> Vec<Vec<RefGraph>> {
    stages
        .iter()
        .map(|stage| {
            let sources = c11_sources(stage);
            let certified = if stage.source == c11_fan_schema() {
                c11_fan_state_count()
            } else {
                c11_graph_state_count(&stage.source)
            };
            assert_eq!(sources.len(), certified, "stage {}", stage.name);
            sources
        })
        .collect()
}

/// The cases of one `(stage, state)` unit: the unedited view and its
/// one-step edits (when the reference get succeeds), then the get.
fn c11_graph_unit_cases<'a>(
    pairs: &[Pair],
    stages: &[C11Stage],
    stage: usize,
    source: &'a RefGraph,
) -> Vec<GraphCase<'a>> {
    let mut cases = Vec::new();
    if let Ok((view, complement)) = reference_get(&pairs[stage], source) {
        let edits = c11_graph_edits(&view, &stages[stage].target, &complement);
        assert_eq!(
            edits.len(),
            c11_graph_edit_count(&view, &stages[stage].target, &complement),
            "the per-view edit formula at stage {}",
            stages[stage].name
        );
        cases.push(GraphCase::Put(stage, source, view));
        cases.extend(
            edits
                .into_iter()
                .map(|edit| GraphCase::Put(stage, source, edit.view)),
        );
    }
    cases.push(GraphCase::Get(stage, source));
    cases
}

#[test]
#[ignore = "the amended C11 census awaits build verification"]
fn graph_lens_agrees_with_the_reference_exhaustively() {
    let schemas = sigma2_schemas();
    assert_eq!(schemas.len(), sigma2_count());
    let stages = c11_stages();
    let certificate = c11_path_count_certificate();
    assert_eq!(stages.len(), certificate.stages);
    let sources = c11_stage_sources(&stages);
    let units: Vec<(usize, usize)> = sources
        .iter()
        .enumerate()
        .flat_map(|(stage, states)| (0..states.len()).map(move |state| (stage, state)))
        .collect();
    assert_eq!(units.len(), certificate.single_stage_states);
    let bound = format!(
        "C11 graph scope: schemas={}, stages={}, states={}",
        schemas.len(),
        stages.len(),
        units.len(),
    );
    println!("{bound}");
    // Unedited views (one per state whose reference get succeeds) and
    // one-step edits, tallied as the units expand; each unit's edit count is
    // checked against the per-view formula.
    let views = std::sync::atomic::AtomicUsize::new(0);
    let edits = std::sync::atomic::AtomicUsize::new(0);
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &units,
        || stages.iter().map(Pair::from_stage).collect::<Vec<Pair>>(),
        |pairs, (stage, state)| {
            let cases = c11_graph_unit_cases(pairs, &stages, *stage, &sources[*stage][*state]);
            let viewed = usize::from(cases.len() > 1);
            views.fetch_add(viewed, std::sync::atomic::Ordering::Relaxed);
            edits.fetch_add(
                cases.len() - 1 - viewed,
                std::sync::atomic::Ordering::Relaxed,
            );
            cases
        },
        |pairs, case| match case {
            GraphCase::Get(stage, source) => {
                GraphCaseOutcome::Get(production_get(&pairs[*stage], source))
            }
            GraphCase::Put(stage, source, view) => {
                GraphCaseOutcome::Put(production_put(&pairs[*stage], source, view))
            }
        },
        |pairs, case| match case {
            GraphCase::Get(stage, source) => {
                GraphCaseOutcome::Get(reference_get(&pairs[*stage], source))
            }
            GraphCase::Put(stage, source, view) => {
                let module = reference_put(&pairs[*stage], source, view);
                assert_eq!(
                    module,
                    reference_strict_put(&pairs[*stage], source, view),
                    "the module put is the strict lens put"
                );
                GraphCaseOutcome::Put(module)
            }
        },
        |production, reference| production == reference,
    ));
    let (views, edits) = (views.into_inner(), edits.into_inner());
    let total = units.len() + views + edits;
    println!(
        "C11 graph cases: gets={}, unedited views={views}, one-step edits={edits}, cases={total}",
        units.len()
    );
    assert_eq!(compared, u64::try_from(total).unwrap());
}

// ---------------------------------------------------------------------------
// Trees: get, GetPut and the five-gate reports on the C11 tree states
// ---------------------------------------------------------------------------

use panproto_inst::{Fan, WInstance};
use panproto_reference::enumerate::lenses::{c11_tree_state_count, c11_tree_states};
use panproto_reference::model::{Fan as RefFan, Tree as RefTree};

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

fn prod_tree(tree: &RefTree) -> WInstance {
    let root_anchor = tree
        .nodes
        .get(&tree.root)
        .map(|node| node.anchor.clone())
        .unwrap_or_default();
    WInstance::new(
        tree.nodes
            .iter()
            .map(|(id, node)| (*id, prod_node(node)))
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
                children: fan
                    .children
                    .iter()
                    .map(|(label, child)| (label.clone(), *child))
                    .collect(),
            })
            .collect(),
        tree.root,
        root_anchor.into(),
    )
}

/// The tree complement coordinates that F.B (3) maps field for field
/// between a tree complement and the graph complement of the tree's graph
/// form: dropped non-fan nodes, dropped non-fan arcs as `(src, tgt, label)`
/// (ascending), recorded anchors, fields and values, synthesized keys and
/// the node floor.
#[derive(Debug, PartialEq, Eq)]
struct TreeResidue {
    dropped_nodes: BTreeMap<u32, RefNode>,
    dropped_arcs: Vec<(u32, u32, RefSEdge)>,
    original_anchors: BTreeMap<u32, String>,
    original_fields: BTreeMap<u32, BTreeMap<String, RefValue>>,
    original_values: BTreeMap<u32, Option<RefPresence>>,
    synthesized: BTreeMap<u32, RefSynthKey>,
    floor: u32,
}

fn production_tree_residue(complement: &panproto_lens::Complement) -> TreeResidue {
    let mut dropped_arcs: Vec<(u32, u32, RefSEdge)> = complement
        .dropped_arcs
        .iter()
        .map(|(src, tgt, edge)| (*src, *tgt, ref_edge(edge)))
        .collect();
    dropped_arcs.sort();
    TreeResidue {
        dropped_nodes: complement
            .dropped_nodes
            .iter()
            .map(|(id, node)| (*id, ref_node(node)))
            .collect(),
        dropped_arcs,
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
        synthesized: complement
            .synthesized_keys
            .iter()
            .map(|(id, key)| (*id, ref_synth_key(key)))
            .collect(),
        floor: complement.id_floor,
    }
}

fn reference_tree_residue(complement: &RefComplement) -> TreeResidue {
    let is_fan = |kind: &str| panproto_reference::restrict::is_fan_kind(kind);
    let mut dropped_arcs: Vec<(u32, u32, RefSEdge)> = complement
        .dropped_edges
        .values()
        .filter(|edge| !is_fan(&edge.label.kind))
        .map(|edge| (edge.src, edge.tgt, edge.label.clone()))
        .collect();
    dropped_arcs.sort();
    TreeResidue {
        dropped_nodes: complement
            .dropped_nodes
            .iter()
            .filter(|(_, node)| node.shape != RefNodeShape::HyperEdge)
            .map(|(id, node)| (*id, node.clone()))
            .collect(),
        dropped_arcs,
        original_anchors: complement.original_anchors.clone(),
        original_fields: complement.original_fields.clone(),
        original_values: complement.original_values.clone(),
        synthesized: complement.synthesized_nodes.clone(),
        floor: complement.floor.0,
    }
}

/// Tree get on both sides, with GetPut: the view, the mapped residue and
/// the put of the unedited view. `None` outside the reference tree lens's
/// domain (a stage dropping an edge between surviving vertices).
type TreeOutcome = Option<Result<(RefTree, TreeResidue, Result<RefTree, Reason>), Reason>>;

fn production_tree_case(pair: &Pair, source: &RefTree, inside: bool) -> TreeOutcome {
    if !inside {
        return None;
    }
    let tree = prod_tree(source);
    Some(
        panproto_lens::get(&pair.production, &tree)
            .map(|(view, complement)| {
                let restored = panproto_lens::put(&pair.production, &view, &complement)
                    .map(|restored| ref_tree(&restored))
                    .map_err(|error| lens_reason(&error));
                (
                    ref_tree(&view),
                    production_tree_residue(&complement),
                    restored,
                )
            })
            .map_err(|error| lens_reason(&error)),
    )
}

fn reference_tree_case(pair: &Pair, source: &RefTree) -> TreeOutcome {
    pair.with_reference(|lens| {
        let lens = lens.ok()?;
        Some(
            panproto_reference::lens::tree_get(lens, source)?
                .map(|(view, complement, embedding)| {
                    let restored =
                        panproto_reference::lens::tree_put(lens, &view, &embedding, &complement)
                            .map_err(|error| ref_lens_reason(&error));
                    (view, reference_tree_residue(&complement), restored)
                })
                .map_err(|error| Reason::Restrict(ref_restrict_reason(&error))),
        )
    })
}

#[test]
#[ignore = "the amended C11 census awaits build verification"]
fn tree_lens_agrees_with_the_reference_exhaustively() {
    let stages = c11_stages();
    let certificate = c11_path_count_certificate();
    let mut cases = Vec::new();
    let mut certified = 0_usize;
    for (index, stage) in stages.iter().enumerate() {
        if stage.source == c11_fan_schema() {
            continue;
        }
        let trees = c11_tree_states(&stage.source);
        certified += c11_tree_state_count(&stage.source);
        cases.extend(trees.into_iter().map(|tree| (index, tree)));
    }
    assert_eq!(cases.len(), certified);
    assert_eq!(certified, certificate.stage_tree_states);
    let bound = format!(
        "C11 tree scope: stages={}, trees of at most 3 nodes over each stage's source schema, (stage, tree) cases={}",
        certificate.tree_stages,
        cases.len()
    );
    println!("{bound}");
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &cases,
        || stages.iter().map(Pair::from_stage).collect::<Vec<Pair>>(),
        |_, case| vec![case],
        |pairs, (stage, tree)| {
            let inside = reference_tree_case(&pairs[*stage], tree).is_some();
            (
                production_tree_case(&pairs[*stage], tree, inside),
                inside.then(|| {
                    production_law_outcome(&panproto_lens::check_laws_report::<
                        panproto_lens::TreeShape,
                    >(
                        &pairs[*stage].production, &prod_tree(tree), &()
                    ))
                }),
            )
        },
        |pairs, (stage, tree)| {
            let outcome = reference_tree_case(&pairs[*stage], tree);
            let laws = outcome.as_ref().map(|_| {
                pairs[*stage].with_reference(|lens| {
                    reference_law_outcome(&panproto_reference::lens::check_tree_laws(
                        lens.expect("inside the reference domain the plan compiles"),
                        tree,
                    ))
                })
            });
            (outcome, laws)
        },
        |production, reference| production == reference,
    ));
    assert_eq!(compared, u64::try_from(cases.len()).unwrap());
}

// ---------------------------------------------------------------------------
// Lens paths: the free category on the stages, and fusion soundness (the
// production fused fold against the reference's unfused fold)
// ---------------------------------------------------------------------------

use panproto_lens::LensPath;
use panproto_reference::path::{LensPath as RefLensPath, PathError, PathStage, PathStageError};

fn ref_path_stage(index: usize, stage: &C11Stage) -> PathStage<usize> {
    PathStage::new(
        index,
        &stage.source,
        &stage.target,
        stage.name.as_bytes().to_vec(),
        stage.name.as_bytes().to_vec(),
    )
}

/// The reference path of a word of stage indices; `Err` when two adjacent
/// boundaries differ.
fn reference_word(stages: &[C11Stage], word: &[usize]) -> Result<RefLensPath<usize>, String> {
    let mut path: Option<RefLensPath<usize>> = None;
    for index in word {
        let next = RefLensPath::single(ref_path_stage(*index, &stages[*index]));
        path = Some(match path {
            None => next,
            Some(path) => RefLensPath::then(&path, &next).map_err(|error| format!("{error:?}"))?,
        });
    }
    path.ok_or_else(|| "empty word".to_owned())
}

/// The production path of a word, built by `then` from single stages.
fn production_word(stages: &[C11Stage], word: &[usize]) -> Result<LensPath, String> {
    let mut path: Option<LensPath> = None;
    for index in word {
        let next = LensPath::single(production_stage(&stages[*index]));
        path = Some(match path {
            None => next,
            Some(path) => path.then(&next).map_err(|error| format!("{error:?}"))?,
        });
    }
    path.ok_or_else(|| "empty word".to_owned())
}

/// A located path refusal: the reason and the atomic stages of the group
/// that refused.
type PathRefusal = (Reason, std::ops::Range<usize>);

fn production_path_refusal(error: &LensError) -> PathRefusal {
    match error {
        LensError::AtStage { at, error } => (lens_reason(error), at.stages.clone()),
        other => (lens_reason(other), 0..0),
    }
}

fn reference_path_refusal(error: &PathError<Reason>) -> PathRefusal {
    match error {
        PathError::AtStage { at, error } => match error.as_ref() {
            PathStageError::Operation(reason) => (reason.clone(), at.stages.clone()),
            other => (Reason::Identity(format!("{other:?}")), at.stages.clone()),
        },
        other => (Reason::Identity(format!("{other:?}")), 0..0),
    }
}

/// Two located refusals agree when their reasons agree and the reference's
/// atomic stage lies in the production's (possibly fused) group.
fn refusals_agree(production: &PathRefusal, reference: &PathRefusal) -> bool {
    production.0 == reference.0
        && production.1.start <= reference.1.start
        && reference.1.end <= production.1.end
}

#[derive(Debug)]
struct PathOutcome {
    get: Result<RefGraph, PathRefusal>,
    puts: Vec<Result<RefGraph, PathRefusal>>,
}

fn path_outcomes_agree(production: &PathOutcome, reference: &PathOutcome) -> bool {
    let same = |a: &Result<RefGraph, PathRefusal>, b: &Result<RefGraph, PathRefusal>| match (a, b) {
        (Ok(a), Ok(b)) => a == b,
        (Err(a), Err(b)) => refusals_agree(a, b),
        _ => false,
    };
    same(&production.get, &reference.get)
        && production.puts.len() == reference.puts.len()
        && production
            .puts
            .iter()
            .zip(&reference.puts)
            .all(|(a, b)| same(a, b))
}

/// The reference fold of a word on one source: get left to right through
/// [`panproto_reference::path::get_path`], then put right to left through
/// [`panproto_reference::path::put_path`] of the unedited view and of every
/// edited view in `views`.
fn reference_path_outcome(
    pairs: &[Pair],
    path: &RefLensPath<usize>,
    source: &RefGraph,
    views: &[RefGraph],
) -> PathOutcome {
    let get = panproto_reference::path::get_path(
        path,
        |_, index: &usize, input: &RefGraph| reference_get(&pairs[*index], input),
        source,
    );
    let Ok((view, complement)) = get else {
        let error = get.err().map(|error| reference_path_refusal(&error));
        return PathOutcome {
            get: Err(error.unwrap_or_else(|| (Reason::Identity("unreachable".into()), 0..0))),
            puts: Vec::new(),
        };
    };
    let put = |_: usize, index: &usize, input: &RefGraph, entry: &RefComplement| {
        pairs[*index].with_reference(|lens| {
            let lens = lens.map_err(Reason::Restrict)?;
            let mut stamped = entry.clone();
            stamped.identity = Some(*lens.identity());
            panproto_reference::lens::put(lens, input, &stamped)
                .map_err(|error| ref_lens_reason(&error))
        })
    };
    let puts = std::iter::once(&view)
        .chain(views)
        .map(|edited| {
            panproto_reference::path::put_path(path, &put, edited, &complement)
                .map_err(|error| reference_path_refusal(&error))
        })
        .collect();
    PathOutcome {
        get: Ok(view),
        puts,
    }
}

fn production_path_outcome(path: &LensPath, source: &RefGraph, views: &[RefGraph]) -> PathOutcome {
    match path.get_graph(&prod_graph(source)) {
        Err(error) => PathOutcome {
            get: Err(production_path_refusal(&error)),
            puts: Vec::new(),
        },
        Ok((view, complement)) => {
            let unedited = ref_graph(&view);
            let puts = std::iter::once(&unedited)
                .chain(views)
                .map(|edited| {
                    path.put_graph(&prod_graph(edited), &complement)
                        .map(|restored| ref_graph(&restored))
                        .map_err(|error| production_path_refusal(&error))
                })
                .collect();
            PathOutcome {
                get: Ok(unedited),
                puts,
            }
        }
    }
}

/// Every word of length 1 to 3 over the stage alphabet, by ascending length
/// and then lexicographic index order.
fn words(stages: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut layer: Vec<Vec<usize>> = vec![Vec::new()];
    for _ in 0..3 {
        layer = layer
            .iter()
            .flat_map(|prefix| {
                (0..stages).map(move |index| {
                    let mut word = prefix.clone();
                    word.push(index);
                    word
                })
            })
            .collect();
        out.extend(layer.iter().cloned());
    }
    out
}

fn c11_stage_index(stages: &[C11Stage], name: &str) -> usize {
    stages
        .iter()
        .position(|stage| stage.name == name)
        .unwrap_or_else(|| panic!("missing C11 stage {name}"))
}

fn c11_identity_index(stages: &[C11Stage], schema: &RefSchema) -> usize {
    stages
        .iter()
        .position(|stage| {
            stage.name.starts_with("Id") && stage.source == *schema && stage.target == *schema
        })
        .unwrap_or_else(|| panic!("missing C11 identity at {schema:?}"))
}

#[test]
fn the_c11_alphabet_witnesses_every_tree_fusion_condition() {
    let stages = c11_stages();
    let named = |name| c11_stage_index(&stages, name);
    let identity_after = |name| c11_identity_index(&stages, &stages[named(name)].target);
    let witnesses = [
        (
            0_u8,
            "admitted",
            c11_identity_index(&stages, &stages[named("T")].source),
            c11_identity_index(&stages, &stages[named("T")].source),
            true,
        ),
        (1, "contraction", named("C"), identity_after("C"), false),
        (2, "unbound field", named("DropF"), named("ReadF"), false),
        (3, "direct edge image", named("D"), named("Ext"), false),
        (4, "expansion", named("X"), identity_after("X"), false),
        (5, "value layers", named("Q"), named("T"), false),
        (6, "fan retarget", named("H"), named("Id_h"), false),
        (7, "plan refusal", named("B"), identity_after("B"), false),
    ];
    for (condition, label, first, second, fused) in witnesses {
        assert_eq!(
            stages[first].target, stages[second].source,
            "condition {condition} ({label}) must be boundary-matched"
        );
        let path = production_word(&stages, &[first, second]).expect("matching boundary");
        let groups = path.tree_normal_form();
        assert_eq!(
            groups.len(),
            if fused { 1 } else { 2 },
            "condition {condition} ({label}) at {} then {}",
            stages[first].name,
            stages[second].name
        );
        println!(
            "C11 fusion condition {condition} ({label}): {} then {}, fused={fused}",
            stages[first].name, stages[second].name
        );
    }
}

#[test]
#[ignore = "the amended C11 census awaits build verification"]
fn lens_paths_agree_with_the_reference_exhaustively() {
    let stages = c11_stages();
    let certificate = c11_path_count_certificate();
    let all = words(stages.len());
    assert_eq!(all.len(), certificate.words());
    let mut composable_words = Vec::new();
    let mut composable_by_length = [0_usize; 3];
    for word in &all {
        let production = production_word(&stages, word);
        let reference = reference_word(&stages, word);
        assert_eq!(
            production.is_ok(),
            reference.is_ok(),
            "composition refusal of {word:?}: {production:?} against {reference:?}"
        );
        if let (Ok(production), Ok(reference)) = (production, reference) {
            assert_eq!(production.len(), reference.len());
            composable_by_length[word.len() - 1] += 1;
            composable_words.push(word.clone());
        }
    }
    assert_eq!(composable_by_length, certificate.composable_by_length);
    // Each composable word, then the word composed with the identity on
    // either side, on both sides of the differential (the unit laws,
    // behaviourally). Built once per thread: the composites hold value
    // layers that are not shared between threads.
    let composites = || -> (Vec<Pair>, Vec<(Vec<usize>, LensPath, RefLensPath<usize>)>) {
        let mut out = Vec::new();
        for word in &composable_words {
            let production = production_word(&stages, word).unwrap();
            let reference = reference_word(&stages, word).unwrap();
            let first = &stages[word[0]];
            let last = &stages[word[word.len() - 1]];
            let left = (
                LensPath::identity(&prod_schema(&first.source))
                    .then(&production)
                    .unwrap(),
                RefLensPath::then(&RefLensPath::identity(&first.source), &reference).unwrap(),
            );
            let right = (
                production
                    .then(&LensPath::identity(&prod_schema(&last.target)))
                    .unwrap(),
                RefLensPath::then(&reference, &RefLensPath::identity(&last.target)).unwrap(),
            );
            out.push((word.clone(), production, reference));
            out.push((word.clone(), left.0, left.1));
            out.push((word.clone(), right.0, right.1));
        }
        (stages.iter().map(Pair::from_stage).collect(), out)
    };
    let sources = c11_stage_sources(&stages);
    let units: Vec<(usize, usize)> = (0..3 * composable_words.len())
        .flat_map(|index| {
            let first = composable_words[index / 3][0];
            (0..sources[first].len()).map(move |state| (index, state))
        })
        .collect();
    let bound = format!(
        "C11 paths: stages={}, words by length={:?}, words={}, composable by length={:?}, composable={}, with both identity composites={}, word-state pairs={}, evaluated path-state cases={}",
        stages.len(),
        certificate.words_by_length,
        all.len(),
        composable_by_length,
        composable_words.len(),
        3 * composable_words.len(),
        certificate.word_states(),
        units.len()
    );
    assert_eq!(composable_words.len(), certificate.composable());
    assert_eq!(units.len(), 3 * certificate.word_states());
    println!("{bound}");
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &units,
        composites,
        |(_, composites), (index, state)| {
            let word = composites[*index].0.clone();
            let source = &sources[word[0]][*state];
            vec![(*index, word, source)]
        },
        |(pairs, composites), (index, word, source)| {
            let (_, path, reference) = &composites[*index];
            let edits = edits_of_reference_view(pairs, &stages, word, reference, source);
            production_path_outcome(path, source, &edits)
        },
        |(pairs, composites), (index, word, source)| {
            let (_, _, reference) = &composites[*index];
            let edits = edits_of_reference_view(pairs, &stages, word, reference, source);
            reference_path_outcome(pairs, reference, source, &edits)
        },
        path_outcomes_agree,
    ));
    assert_eq!(compared, u64::try_from(units.len()).unwrap());
}

/// The one-step edits of a path's final view: the C11 edits of the last
/// stage's view against the last stage's complement, from the reference fold.
fn edits_of_reference_view(
    pairs: &[Pair],
    stages: &[C11Stage],
    word: &[usize],
    path: &RefLensPath<usize>,
    source: &RefGraph,
) -> Vec<RefGraph> {
    let get = panproto_reference::path::get_path(
        path,
        |_, index: &usize, input: &RefGraph| reference_get(&pairs[*index], input),
        source,
    );
    let Ok((view, complement)) = get else {
        return Vec::new();
    };
    let Some(last) = complement.entries().last() else {
        return Vec::new();
    };
    let target = &stages[word[word.len() - 1]].target;
    c11_graph_edits(&view, target, &last.complement)
        .into_iter()
        .map(|edit| edit.view)
        .collect()
}

#[test]
#[ignore = "the amended C11 census awaits build verification"]
fn lens_path_category_laws_agree_with_the_reference_exhaustively() {
    let stages = c11_stages();
    let schemas: Vec<RefSchema> = sigma2_schemas()
        .into_iter()
        .chain([c11_fan_schema()])
        .collect();
    let mut identities = 0_usize;
    let mut triples = 0_usize;
    let envelopes = |path: &LensPath| path.identity_envelope(panproto_inst::ComplementShape::Graph);
    for (index, stage) in stages.iter().enumerate() {
        let reference = RefLensPath::single(ref_path_stage(index, stage));
        let left = RefLensPath::then(&RefLensPath::identity(&stage.source), &reference);
        let right = RefLensPath::then(&reference, &RefLensPath::identity(&stage.target));
        assert_eq!(
            left.as_ref(),
            Ok(&reference),
            "reference left identity at {}",
            stage.name
        );
        assert_eq!(
            right.as_ref(),
            Ok(&reference),
            "reference right identity at {}",
            stage.name
        );
        let production = LensPath::single(production_stage(stage));
        let left = LensPath::identity(&prod_schema(&stage.source))
            .then(&production)
            .unwrap();
        let right = production
            .then(&LensPath::identity(&prod_schema(&stage.target)))
            .unwrap();
        assert_eq!(
            envelopes(&left),
            envelopes(&production),
            "production left identity at {}",
            stage.name
        );
        assert_eq!(
            envelopes(&right),
            envelopes(&production),
            "production right identity at {}",
            stage.name
        );
        identities += 1;
    }
    for schema in &schemas {
        let reference = RefLensPath::<usize>::identity(schema);
        assert_eq!(
            RefLensPath::then(&reference, &reference).as_ref(),
            Ok(&reference)
        );
        let production = LensPath::identity(&prod_schema(schema));
        let twice = production.then(&production).unwrap();
        assert!(twice.is_identity());
        assert_eq!(envelopes(&twice), envelopes(&production));
    }
    for p in 0..stages.len() {
        for q in 0..stages.len() {
            for r in 0..stages.len() {
                let single =
                    |index: usize| RefLensPath::single(ref_path_stage(index, &stages[index]));
                let left = RefLensPath::then(&single(p), &single(q))
                    .and_then(|pq| RefLensPath::then(&pq, &single(r)));
                let right = RefLensPath::then(&single(q), &single(r))
                    .and_then(|qr| RefLensPath::then(&single(p), &qr));
                assert_eq!(left, right, "reference associativity at {p} {q} {r}");
                let prod = |index: usize| LensPath::single(production_stage(&stages[index]));
                let left = prod(p).then(&prod(q)).and_then(|pq| pq.then(&prod(r)));
                let right = prod(q).then(&prod(r)).and_then(|qr| prod(p).then(&qr));
                match (left, right) {
                    (Ok(left), Ok(right)) => assert_eq!(envelopes(&left), envelopes(&right)),
                    (Err(left), Err(right)) => {
                        assert_eq!(format!("{left:?}"), format!("{right:?}"))
                    }
                    (left, right) => panic!(
                        "production associativity at {p} {q} {r}: {left:?} against {right:?}"
                    ),
                }
                triples += 1;
            }
        }
    }
    println!(
        "C11 path category: stages={}, identity schemas={}, unit checks={identities}, triples={triples}",
        stages.len(),
        schemas.len()
    );
    assert_eq!(triples, c11_path_count_certificate().words_by_length[2]);
}

// ---------------------------------------------------------------------------
// F.B squares: the production graph route and fast route against the
// reference's two routes
// ---------------------------------------------------------------------------

use panproto_reference::lens::{SquareResult, SquareRouteObservation};

/// One leg of a square in reference coordinates: get (view and complement
/// without identity) and the put of the unedited view.
#[derive(Debug, PartialEq, Eq)]
struct Leg {
    get: Result<(RefGraph, RefComplement), String>,
    put: Option<Result<RefGraph, String>>,
}

/// A square in reference coordinates: both legs, whether they commute, and
/// (on the reference side) whether a route is outside the exact common
/// domain. The production side cannot tell: its fast leg is the dispatcher,
/// which runs the graph route wherever the tree or table route refuses.
#[derive(Debug, PartialEq, Eq)]
struct SquareOutcome {
    graph: Leg,
    fast: Option<Leg>,
    commutes: bool,
    outside: bool,
}

fn reference_leg(route: &SquareRouteObservation) -> Leg {
    Leg {
        get: route
            .get
            .as_ref()
            .map(|(view, complement)| (view.clone(), without_identity(complement)))
            .map_err(|error| format!("{error:?}")),
        put: route
            .put
            .as_ref()
            .map(|put| put.clone().map_err(|error| format!("{error:?}"))),
    }
}

/// The refusals of the two legs are compared through their presence only:
/// the reference names a route refusal by its square error, the production
/// by its lens error, and both routes' reasons are compared exactly by the
/// get and put differentials above.
fn legs_agree(production: &Leg, reference: &Leg) -> bool {
    let get = match (&production.get, &reference.get) {
        (Ok(a), Ok(b)) => a == b,
        (Err(_), Err(_)) => true,
        _ => false,
    };
    let put = match (&production.put, &reference.put) {
        (Some(Ok(a)), Some(Ok(b))) => a == b,
        (Some(Err(_)), Some(Err(_))) | (None, None) => true,
        _ => false,
    };
    get && put
}

/// Whether a production fast leg is the dispatcher's fallback to the graph
/// route: the same get as the graph leg, and either no put (the dispatcher
/// declined the put) or the graph leg's put.
fn falls_back_to_the_graph_route(fast: &Leg, graph: &Leg) -> bool {
    fast.get == graph.get && (fast.put.is_none() || fast.put == graph.put)
}

/// Two squares agree when their graph legs agree and their commutation
/// verdicts are equal, and on the fast legs: inside the exact common domain
/// the two fast legs agree; outside it (the reference refuses a route) the
/// law makes no comparison, and the production dispatcher must instead
/// decline or fall back to the graph route.
fn squares_agree(production: &SquareOutcome, reference: &SquareOutcome) -> bool {
    let fast = match (&production.fast, &reference.fast) {
        (Some(a), Some(b)) if legs_agree(a, b) => true,
        (None, _) => !reference.commutes,
        (Some(a), _) => {
            reference.outside && falls_back_to_the_graph_route(a, &production.graph)
        }
    };
    legs_agree(&production.graph, &reference.graph)
        && fast
        && production.commutes == reference.commutes
}

fn production_graph_leg(lens: &Lens, graph: &GInstance) -> Leg {
    match graph_lens::get(lens, graph) {
        Err(error) => Leg {
            get: Err(format!("{error:?}")),
            put: None,
        },
        Ok((view, complement)) => Leg {
            get: Ok((ref_graph(&view), ref_complement(&complement))),
            put: Some(
                graph_lens::put(lens, &view, &complement)
                    .map(|restored| ref_graph(&restored))
                    .map_err(|error| format!("{error:?}")),
            ),
        },
    }
}

fn reference_square<O>(result: &SquareResult<O>, legs: impl Fn(&O) -> (Leg, Leg)) -> SquareOutcome {
    let (graph, fast) = legs(result.observation());
    SquareOutcome {
        graph,
        fast: Some(fast),
        commutes: result.commutes(),
        outside: matches!(result, SquareResult::Refused(_)),
    }
}

fn table_square_pair_graph(pair: &TablePair) -> Pair {
    Pair::new(pair.src.clone(), pair.tgt.clone(), pair.migration.clone())
}

#[test]
fn table_square_agrees_with_the_reference_on_the_fixture_scope() {
    let certificate = table_fixture_count_certificate();
    let mut fixtures = Vec::new();
    for_each_table_fixture(|fixture| fixtures.push(fixture));
    assert_eq!(fixtures.len(), certificate.total);
    let bound = format!(
        "table square: rows<=2, columns<=2, cell alphabet=7, lenses=7, source/lens cases={}",
        certificate.total
    );
    println!("{bound}");
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &fixtures,
        || (),
        |(), fixture| vec![fixture],
        |(), fixture| {
            let pair = fixture_pair(fixture);
            let (graph, _) = GInstance::from_finstance(&prod_table(&fixture.source)).unwrap();
            let graph_leg = production_graph_leg(&pair.production, &graph);
            let fast = graph_lens_via_table(&pair, &graph);
            let commutes = fast
                .as_ref()
                .is_some_and(|fast| legs_agree(fast, &graph_leg));
            SquareOutcome {
                graph: graph_leg,
                fast,
                commutes,
                outside: false,
            }
        },
        |(), fixture| {
            let pair = fixture_pair(fixture);
            let graph_pair = table_square_pair_graph(&pair);
            graph_pair.with_reference(|graph_lens| {
                let graph_lens = graph_lens.expect("fixture plans compile");
                pair.with_reference(|table_lens, _| {
                    let result = panproto_reference::lens::table_square(
                        graph_lens,
                        table_lens,
                        &fixture.source,
                    );
                    reference_square(&result, |observation| {
                        (
                            reference_leg(&observation.graph),
                            reference_leg(&observation.fast),
                        )
                    })
                })
            })
        },
        squares_agree,
    ));
    assert_eq!(compared, u64::try_from(certificate.total).unwrap());
}

/// The production table route of the square: get through the table lens
/// and back to graph coordinates, and the put of the unedited view.
fn graph_lens_via_table(pair: &TablePair, graph: &GInstance) -> Option<Leg> {
    let lens = &pair.production;
    match panproto_lens::graph_get_via_table(lens, graph, pair.protocols()) {
        Ok(None) => None,
        Err(error) => Some(Leg {
            get: Err(format!("{error:?}")),
            put: None,
        }),
        Ok(Some((view, complement))) => Some(Leg {
            get: Ok((ref_graph(&view), ref_complement(&complement))),
            put: match panproto_lens::graph_put_via_table(
                lens,
                &view,
                &complement,
                pair.protocols(),
            ) {
                Ok(None) => None,
                Ok(Some(restored)) => Some(Ok(ref_graph(&restored))),
                Err(error) => Some(Err(format!("{error:?}"))),
            },
        }),
    }
}

fn graph_lens_via_tree(lens: &Lens, graph: &GInstance) -> Option<Leg> {
    match panproto_lens::graph_get_via_tree(lens, graph) {
        Ok(None) => None,
        Err(error) => Some(Leg {
            get: Err(format!("{error:?}")),
            put: None,
        }),
        Ok(Some((view, complement))) => Some(Leg {
            get: Ok((ref_graph(&view), ref_complement(&complement))),
            put: match panproto_lens::graph_put_via_tree(lens, &view, &complement) {
                Ok(None) => None,
                Ok(Some(restored)) => Some(Ok(ref_graph(&restored))),
                Err(error) => Some(Err(format!("{error:?}"))),
            },
        }),
    }
}

#[test]
#[ignore = "the amended C11 census awaits build verification"]
fn graph_square_agrees_with_the_reference_exhaustively() {
    let stages = c11_stages();
    let mut cases = Vec::new();
    for (index, stage) in stages.iter().enumerate() {
        if stage.source == c11_fan_schema() {
            continue;
        }
        for tree in c11_tree_states(&stage.source) {
            cases.push((index, tree));
        }
    }
    let bound = format!("C11 graph square: (stage, tree) cases={}", cases.len());
    println!("{bound}");
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &cases,
        || stages.iter().map(Pair::from_stage).collect::<Vec<Pair>>(),
        |_, case| vec![case],
        |pairs, (stage, tree)| {
            let lens = &pairs[*stage].production;
            let graph = GInstance::from_wtype(&prod_tree(tree)).unwrap();
            let graph_leg = production_graph_leg(lens, &graph);
            let fast = graph_lens_via_tree(lens, &graph);
            let commutes = fast
                .as_ref()
                .is_some_and(|fast| legs_agree(fast, &graph_leg));
            SquareOutcome {
                graph: graph_leg,
                fast,
                commutes,
                outside: false,
            }
        },
        |pairs, (stage, tree)| {
            pairs[*stage].with_reference(|lens| {
                // A plan that does not compile refuses both routes' get.
                let lens = match lens {
                    Ok(lens) => lens,
                    Err(reason) => {
                        return SquareOutcome {
                            graph: Leg {
                                get: Err(reason),
                                put: None,
                            },
                            fast: None,
                            commutes: false,
                            outside: true,
                        };
                    }
                };
                let result = panproto_reference::lens::graph_square(lens, tree);
                reference_square(&result, |observation| {
                    (
                        reference_leg(&observation.graph),
                        reference_leg(&observation.fast),
                    )
                })
            })
        },
        squares_agree,
    ));
    assert_eq!(compared, u64::try_from(cases.len()).unwrap());
}

#[test]
fn table_laws_agree_with_the_reference_on_the_fixture_scope() {
    let certificate = table_fixture_count_certificate();
    let mut fixtures = Vec::new();
    for_each_table_fixture(|fixture| fixtures.push(fixture));
    assert_eq!(fixtures.len(), certificate.total);
    assert_eq!(certificate.total, 9_921);
    let bound = format!(
        "table law reports: rows<=2, columns<=2, cell alphabet=7, lenses=7, source/lens cases={}",
        certificate.total
    );
    println!("{bound}");
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &fixtures,
        table_pairs,
        |_, fixture| vec![fixture],
        |pairs, fixture| {
            let pair = &pairs[fixture.columns][fixture.retained];
            production_law_outcome(&table_lens::check_laws(
                &pair.production,
                &prod_table(&fixture.source),
                pair.protocols(),
            ))
        },
        |pairs, fixture| {
            pairs[fixture.columns][fixture.retained].with_reference(|lens, _| {
                reference_law_outcome(&panproto_reference::lens::check_table_laws(
                    lens,
                    &fixture.source,
                ))
            })
        },
        |production, reference| production == reference,
    ));
    assert_eq!(compared, 9_921);
}

// ---------------------------------------------------------------------------
// Protolenses: precondition gating, schema instantiation and instance
// application against the reference's bounded protolens
// ---------------------------------------------------------------------------

use panproto_lens::protolens::elementary;
use panproto_reference::lens::{Protolens as RefProtolens, ProtolensError as RefProtolensError};

/// The elementary protolens alphabet over the C11 schemas: drop and rename
/// the one vertex kind, and drop and rename the one edge name.
fn protolens_alphabet() -> Vec<panproto_lens::Protolens> {
    vec![
        elementary::drop_sort("obj"),
        elementary::rename_sort("obj", "thing"),
        elementary::drop_edge("obj", "obj", Some("e".into())),
        elementary::rename_edge_name("obj", "obj", "e", "g"),
    ]
}

fn ref_schema(schema: &Schema) -> RefSchema {
    let mut out = RefSchema::default();
    for (id, vertex) in &schema.vertices {
        out = out.vertex(id, &vertex.kind);
    }
    for edge in schema.edges.keys() {
        out.edges.insert(ref_edge(edge));
    }
    for (id, hyper) in &schema.hyper_edges {
        out.hyper_edges.insert(
            id.to_string(),
            panproto_reference::schema::HyperEdge {
                kind: hyper.kind.to_string(),
                signature: hyper
                    .signature
                    .iter()
                    .map(|(label, vertex)| (label.to_string(), vertex.to_string()))
                    .collect(),
                parent_label: hyper.parent_label.to_string(),
            },
        );
    }
    out
}

fn ref_migration(compiled: &CompiledMigration) -> RefMigration {
    RefMigration {
        surviving_verts: compiled
            .surviving_verts
            .iter()
            .map(ToString::to_string)
            .collect(),
        surviving_edges: compiled.surviving_edges.iter().map(ref_edge).collect(),
        vertex_remap: compiled
            .vertex_remap
            .iter()
            .map(|(from, to)| (from.to_string(), to.to_string()))
            .collect(),
        edge_remap: compiled
            .edge_remap
            .iter()
            .map(|(from, to)| (ref_edge(from), ref_edge(to)))
            .collect(),
        resolver: compiled
            .resolver
            .iter()
            .map(|((from, to), edge)| ((from.to_string(), to.to_string()), ref_edge(edge)))
            .collect(),
        expansion_path: compiled
            .expansion_path
            .iter()
            .map(|((from, to), path)| {
                (
                    (from.to_string(), to.to_string()),
                    path.iter().map(ToString::to_string).collect(),
                )
            })
            .collect(),
        conditional: compiled
            .conditional_survival
            .keys()
            .map(ToString::to_string)
            .collect(),
        transformed: compiled
            .field_transforms
            .iter()
            .filter(|(_, transforms)| !transforms.is_empty())
            .map(|(vertex, _)| vertex.to_string())
            .collect(),
        ..RefMigration::default()
    }
}

/// The observable outcome of applying one protolens at one schema to one
/// source: applicability, then the view and complement or the refusal.
#[derive(Debug, PartialEq, Eq)]
enum ProtolensOutcome {
    Inapplicable,
    Refused(String),
    Applied(RefGraph, RefComplement),
}

#[test]
#[ignore = "the amended C11 census awaits build verification"]
fn protolens_transforms_agree_with_the_reference_exhaustively() {
    let alphabet = protolens_alphabet();
    let schemas = sigma2_schemas();
    let states: Vec<Vec<RefGraph>> = schemas.iter().map(c11_graph_states).collect();
    let units: Vec<(usize, usize, usize)> = (0..alphabet.len())
        .flat_map(|index| {
            states.iter().enumerate().flat_map(move |(schema, sources)| {
                (0..sources.len()).map(move |state| (index, schema, state))
            })
        })
        .collect();
    assert_eq!(
        units.len(),
        alphabet.len()
            * schemas
                .iter()
                .map(c11_graph_state_count)
                .sum::<usize>()
    );
    let bound = format!(
        "C11 protolenses: protolenses={}, schemas={}, (protolens, schema, state) cases={}",
        alphabet.len(),
        schemas.len(),
        units.len()
    );
    println!("{bound}");
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &units,
        || (Protocol::default(), protolens_alphabet()),
        |_, (index, schema, state)| vec![(*index, &schemas[*schema], &states[*schema][*state])],
        |(protocol, alphabet), (index, schema, source)| {
            let schema = prod_schema(schema);
            match panproto_lens::graph_protolens::apply(
                &alphabet[*index],
                &schema,
                protocol,
                &prod_graph(source),
            ) {
                Ok(applied) => ProtolensOutcome::Applied(
                    ref_graph(&applied.view),
                    ref_complement(&applied.complement),
                ),
                Err(LensError::ProtolensError(_)) if !alphabet[*index].applicable_to(&schema) => {
                    ProtolensOutcome::Inapplicable
                }
                Err(error) => ProtolensOutcome::Refused(format!("{:?}", lens_reason(&error))),
            }
        },
        |(protocol, alphabet), (index, schema, source)| {
            let production_schema = prod_schema(schema);
            let protolens = &alphabet[*index];
            // The reference protolens at this schema: its precondition is
            // the production precondition's verdict at exactly this schema
            // (`Exact` when it holds, an unsatisfiable vertex requirement
            // otherwise), its target and migration the production
            // instantiation's.
            let Ok(lens) = protolens.instantiate(&production_schema, protocol) else {
                return ProtolensOutcome::Inapplicable;
            };
            let target = ref_schema(&lens.tgt_schema);
            let migration = ref_migration(&lens.compiled);
            let reference = if protolens.applicable_to(&production_schema) {
                RefProtolens::exact(schema, &target, &migration)
            } else {
                RefProtolens::new(
                    panproto_reference::lens::SchemaPrecondition::HasVertices(
                        ["\u{0}unsatisfiable".to_owned()].into(),
                    ),
                    target,
                    migration,
                )
            };
            let instantiated = panproto_reference::lens::protolens_schema(&reference, schema);
            assert_eq!(
                instantiated.as_ref().map(|lens| lens.source().clone()).ok(),
                protolens
                    .applicable_to(&production_schema)
                    .then(|| (*schema).clone())
                    .filter(|_| instantiated.is_ok()),
                "an instantiated reference protolens is bound to exactly its schema"
            );
            match panproto_reference::lens::protolens_instance(&reference, schema, source) {
                Ok(application) => ProtolensOutcome::Applied(
                    application.view,
                    without_identity(&application.complement),
                ),
                Err(RefProtolensError::FalsePrecondition { .. }) => ProtolensOutcome::Inapplicable,
                Err(RefProtolensError::Restrict(error)) => ProtolensOutcome::Refused(format!(
                    "{:?}",
                    Reason::Restrict(ref_restrict_reason(&error))
                )),
                Err(RefProtolensError::Plan(error)) => ProtolensOutcome::Refused(format!(
                    "{:?}",
                    Reason::Restrict(ref_restrict_reason(&RefRestrictError::Plan(error)))
                )),
            }
        },
        |production, reference| production == reference,
    ));
    assert_eq!(compared, u64::try_from(units.len()).unwrap());
}

/// The law-report case families of the L.8 scope.
#[derive(Debug)]
enum LawCase {
    Partial(PartialLens, u8),
    Table(TableFixture),
    Graph(usize, RefGraph),
    Tree(usize, RefTree),
}

#[test]
#[ignore = "the amended C11 census awaits build verification"]
fn lens_laws_agree_with_the_reference_exhaustively() {
    let stages = c11_stages();
    // The lens pairs of every family, built once per thread: they hold
    // value layers that are not shared between threads.
    let context = || {
        let pairs: Vec<Pair> = stages.iter().map(Pair::from_stage).collect();
        (pairs, table_pairs(), empty_lens())
    };
    let mut cases = Vec::new();
    for_each_partial_lens(|lens| {
        for source in 0..2_u8 {
            cases.push(LawCase::Partial(lens, source));
        }
    });
    let partial = cases.len();
    for_each_table_fixture(|fixture| cases.push(LawCase::Table(fixture)));
    let tables = cases.len() - partial;
    for (index, stage) in stages.iter().enumerate() {
        for source in c11_sources(stage) {
            cases.push(LawCase::Graph(index, source));
        }
        if stage.source != c11_fan_schema() {
            for tree in c11_tree_states(&stage.source) {
                cases.push(LawCase::Tree(index, tree));
            }
        }
    }
    assert_eq!(partial, 2 * partial_lens_count_certificate());
    assert_eq!(tables, table_fixture_count_certificate().total);
    let bound = format!(
        "L.8 law reports: partial lens/source cases={partial}, table fixtures={tables}, C11 graph and tree (stage, state) cases={}, total={}",
        cases.len() - partial - tables,
        cases.len()
    );
    println!("{bound}");
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &cases,
        context,
        |_, case| vec![case],
        |(pairs, table_pairs, carrier), case| match case {
            LawCase::Partial(lens, source) => production_law_outcome(
                &panproto_lens::laws::check_laws_report::<PartialShape>(carrier, source, lens),
            ),
            LawCase::Table(fixture) => {
                let pair = &table_pairs[fixture.columns][fixture.retained];
                production_law_outcome(&table_lens::check_laws(
                    &pair.production,
                    &prod_table(&fixture.source),
                    pair.protocols(),
                ))
            }
            LawCase::Graph(stage, source) => production_law_outcome(&graph_lens::check_laws(
                &pairs[*stage].production,
                &prod_graph(source),
            )),
            LawCase::Tree(stage, tree) => production_law_outcome(
                &panproto_lens::check_laws_report::<panproto_lens::TreeShape>(
                    &pairs[*stage].production,
                    &prod_tree(tree),
                    &(),
                ),
            ),
        },
        |(pairs, table_pairs, _), case| match case {
            LawCase::Partial(lens, source) => {
                reference_law_outcome(&partial_lens_law_report(lens, usize::from(*source)))
            }
            LawCase::Table(fixture) => table_pairs[fixture.columns][fixture.retained]
                .with_reference(|lens, _| {
                    reference_law_outcome(&panproto_reference::lens::check_table_laws(
                        lens,
                        &fixture.source,
                    ))
                }),
            LawCase::Graph(stage, source) => pairs[*stage].with_reference(|lens| match lens {
                Ok(lens) => {
                    let outcome = reference_law_outcome(
                        &panproto_reference::lens::check_graph_laws(lens, source),
                    );
                    // A passing report includes GetPut: the unedited view is in
                    // the put domain and both reference puts return the source.
                    if outcome.first_failure.is_none() {
                        let (view, complement) =
                            panproto_reference::lens::get(lens, source).expect("GetPut passed");
                        assert_eq!(
                            RefGraphLens::check_put_domain(lens, &view, &complement),
                            Ok(())
                        );
                        assert_eq!(
                            RefGraphLens::put(lens, &view, &complement).as_ref(),
                            Ok(source)
                        );
                        assert_eq!(
                            panproto_reference::lens::put(lens, &view, &complement).as_ref(),
                            Ok(source)
                        );
                    }
                    outcome
                }
                Err(_) => LawOutcome {
                    first_failure: Some("Refusal".to_owned()),
                },
            }),
            LawCase::Tree(stage, tree) => pairs[*stage].with_reference(|lens| match lens {
                Ok(lens) => {
                    reference_law_outcome(&panproto_reference::lens::check_tree_laws(lens, tree))
                }
                Err(_) => LawOutcome {
                    first_failure: Some("Refusal".to_owned()),
                },
            }),
        },
        |production, reference| production == reference,
    ));
    assert_eq!(compared, u64::try_from(cases.len()).unwrap());
}

// ---------------------------------------------------------------------------
// The property differential: random cases from every exhaustive family
// ---------------------------------------------------------------------------

/// Whether the amended C11 census has passed its build and count gates.
/// Until then the property test samples only the already verified families.
const C11_ACCEPTED: bool = false;

fn gf5_cases() -> &'static [(Gf5Fixture, RefGraph, RefGraph)] {
    static CASES: std::sync::OnceLock<Vec<(Gf5Fixture, RefGraph, RefGraph)>> =
        std::sync::OnceLock::new();
    CASES.get_or_init(|| {
        let mut cases = Vec::new();
        for_each_gf5_deletion_edit(|fixture, source, edited| {
            cases.push((fixture, source.clone(), edited.clone()));
        });
        cases
    })
}

fn table_fixtures() -> &'static [TableFixture] {
    static CASES: std::sync::OnceLock<Vec<TableFixture>> = std::sync::OnceLock::new();
    CASES.get_or_init(|| {
        let mut cases = Vec::new();
        for_each_table_fixture(|fixture| cases.push(fixture));
        cases
    })
}

fn partial_lenses() -> &'static [PartialLens] {
    static CASES: std::sync::OnceLock<Vec<PartialLens>> = std::sync::OnceLock::new();
    CASES.get_or_init(|| {
        let mut cases = Vec::new();
        for_each_partial_lens(|lens| cases.push(lens));
        cases
    })
}

fn c11_cases() -> &'static (Vec<C11Stage>, Vec<(usize, RefGraph)>, Vec<(usize, RefTree)>) {
    static CASES: std::sync::OnceLock<(
        Vec<C11Stage>,
        Vec<(usize, RefGraph)>,
        Vec<(usize, RefTree)>,
    )> = std::sync::OnceLock::new();
    CASES.get_or_init(|| {
        let stages = c11_stages();
        let mut graphs = Vec::new();
        let mut trees = Vec::new();
        for (index, stage) in stages.iter().enumerate() {
            graphs.extend(c11_sources(stage).into_iter().map(|g| (index, g)));
            if stage.source != c11_fan_schema() {
                trees.extend(
                    c11_tree_states(&stage.source)
                        .into_iter()
                        .map(|t| (index, t)),
                );
            }
        }
        (stages, graphs, trees)
    })
}

/// One random case of one family, compared on both sides.
fn property_case(family: u8, index: usize, edit: usize) -> Result<(), String> {
    let differ = |what: &str, production: String, reference: String| {
        if production == reference {
            Ok(())
        } else {
            Err(format!(
                "{what}: production {production} against reference {reference}"
            ))
        }
    };
    match family {
        0 => {
            let cases = gf5_cases();
            let (fixture, source, edited) = &cases[index % cases.len()];
            let pair = gf5_pair(*fixture);
            differ(
                "GF5 put",
                format!("{:?}", production_put(&pair, source, edited)),
                format!("{:?}", reference_put(&pair, source, edited)),
            )?;
            let (production, largest, universal) = gf5_cascade_outcome(&pair, source, edited);
            differ(
                "GF5 cascade",
                format!(
                    "{:?}",
                    (production.and_then(|outcome| outcome.put.ok()), universal)
                ),
                format!("{:?}", (largest, true)),
            )
        }
        1 => {
            let fixtures = table_fixtures();
            let fixture = &fixtures[index % fixtures.len()];
            let pair = fixture_pair(fixture);
            differ(
                "table get",
                format!("{:?}", production_table_get(&pair, &fixture.source)),
                format!("{:?}", reference_table_get(&pair, &fixture.source)),
            )?;
            let (view, complement) =
                reference_table_get(&pair, &fixture.source).map_err(|e| format!("{e:?}"))?;
            let columns: Vec<String> = (0..fixture.columns)
                .filter(|column| fixture.retained & (1 << column) != 0)
                .map(|column| format!("c{column}"))
                .collect();
            let reserved: Vec<u32> = complement.dropped_rows.keys().copied().collect();
            let mut views = vec![view.clone()];
            views.extend(table_view_edits(&view, &columns, &reserved));
            let edited = &views[edit % views.len()];
            differ(
                "table put",
                format!("{:?}", production_table_put(&pair, &fixture.source, edited)),
                format!("{:?}", reference_table_put(&pair, &fixture.source, edited)),
            )?;
            differ(
                "table laws",
                format!(
                    "{:?}",
                    production_law_outcome(&table_lens::check_laws(
                        &pair.production,
                        &prod_table(&fixture.source),
                        pair.protocols(),
                    ))
                ),
                format!(
                    "{:?}",
                    pair.with_reference(|lens, _| {
                        reference_law_outcome(&panproto_reference::lens::check_table_laws(
                            lens,
                            &fixture.source,
                        ))
                    })
                ),
            )
        }
        2 => {
            let lenses = partial_lenses();
            let lens = &lenses[index % lenses.len()];
            let source = u8::try_from(edit % 2).unwrap();
            differ(
                "partial-lens laws",
                format!(
                    "{:?}",
                    production_law_outcome(
                        &panproto_lens::laws::check_laws_report::<PartialShape>(
                            &empty_lens(),
                            &source,
                            lens,
                        )
                    )
                ),
                format!(
                    "{:?}",
                    reference_law_outcome(&partial_lens_law_report(lens, usize::from(source)))
                ),
            )
        }
        3 if C11_ACCEPTED => {
            let (stages, graphs, _) = c11_cases();
            let (stage, source) = &graphs[index % graphs.len()];
            let pair = Pair::from_stage(&stages[*stage]);
            differ(
                "C11 get",
                format!("{:?}", production_get(&pair, source)),
                format!("{:?}", reference_get(&pair, source)),
            )?;
            let Ok((view, complement)) = reference_get(&pair, source) else {
                return Ok(());
            };
            let mut views = vec![view.clone()];
            views.extend(
                c11_graph_edits(&view, &stages[*stage].target, &complement)
                    .into_iter()
                    .map(|e| e.view),
            );
            let edited = &views[edit % views.len()];
            differ(
                "C11 put",
                format!("{:?}", production_put(&pair, source, edited)),
                format!("{:?}", reference_strict_put(&pair, source, edited)),
            )
        }
        4 if C11_ACCEPTED => {
            let (stages, _, trees) = c11_cases();
            let (stage, tree) = &trees[index % trees.len()];
            let pair = Pair::from_stage(&stages[*stage]);
            let reference = reference_tree_case(&pair, tree);
            differ(
                "C11 tree",
                format!(
                    "{:?}",
                    production_tree_case(&pair, tree, reference.is_some())
                ),
                format!("{reference:?}"),
            )?;
            let production_square = {
                let graph =
                    GInstance::from_wtype(&prod_tree(tree)).map_err(|e| format!("{e:?}"))?;
                let graph_leg = production_graph_leg(&pair.production, &graph);
                let fast = graph_lens_via_tree(&pair.production, &graph);
                let commutes = fast
                    .as_ref()
                    .is_some_and(|fast| legs_agree(fast, &graph_leg));
                SquareOutcome {
                    graph: graph_leg,
                    fast,
                    commutes,
                    outside: false,
                }
            };
            let reference_square_outcome = pair.with_reference(|lens| {
                lens.ok().map(|lens| {
                    reference_square(&panproto_reference::lens::graph_square(lens, tree), |o| {
                        (reference_leg(&o.graph), reference_leg(&o.fast))
                    })
                })
            });
            match reference_square_outcome {
                Some(reference) if !squares_agree(&production_square, &reference) => Err(format!(
                    "graph square: production {production_square:?} against reference {reference:?}"
                )),
                _ => Ok(()),
            }
        }
        5 if C11_ACCEPTED => {
            let (stages, _, _) = c11_cases();
            let pairs: Vec<Pair> = stages.iter().map(Pair::from_stage).collect();
            let all = words(stages.len());
            let word = &all[index % all.len()];
            let production = production_word(stages, word);
            let reference = reference_word(stages, word);
            let (Ok(production), Ok(reference)) = (production, reference) else {
                return differ(
                    "path composition",
                    format!("{}", production_word(stages, word).is_ok()),
                    format!("{}", reference_word(stages, word).is_ok()),
                );
            };
            let sources = c11_sources(&stages[word[0]]);
            let source = &sources[edit % sources.len()];
            let edits = edits_of_reference_view(&pairs, stages, word, &reference, source);
            let p = production_path_outcome(&production, source, &edits);
            let r = reference_path_outcome(&pairs, &reference, source, &edits);
            if path_outcomes_agree(&p, &r) {
                Ok(())
            } else {
                Err(format!("path: production {p:?} against reference {r:?}"))
            }
        }
        6 => {
            let mut fixtures = Vec::new();
            for_each_writer_fixture(|fixture| fixtures.push(fixture));
            let fixture = &fixtures[index % fixtures.len()];
            let targets = fixture.target_columns();
            if targets.is_empty() {
                return Ok(());
            }
            let target = targets[edit % targets.len()];
            let value = WRITER_EDITS[(edit / targets.len()) % WRITER_EDITS.len()];
            let pair = writer_pair(fixture);
            let source = fixture.source();
            let (mut view, _) =
                production_table_get(&pair, &source).map_err(|e| format!("{e:?}"))?;
            view.rows
                .get_mut(&0)
                .unwrap()
                .1
                .insert(target.to_owned(), RefValue::Int(value));
            let production = production_table_put(&pair, &source, &view).put;
            let reference = pair.with_reference(|lens, plan| {
                let (_, complement) = lens.get(&source).expect("writer get");
                let writer = panproto_reference::table::TablePlan::writer(plan, "t", target)
                    .expect("an image column has a writer");
                let mut expected = source.clone();
                expected
                    .rows
                    .get_mut(&0)
                    .unwrap()
                    .1
                    .insert(writer, RefValue::Int(value));
                let restored = RefTableLens::put(lens, &view, &complement)
                    .map_err(|error| ref_table_put_reason(&error));
                assert_eq!(
                    restored.as_ref(),
                    Ok(&expected),
                    "reference put writes the last writer"
                );
                restored
            });
            differ(
                "table writer",
                format!("{production:?}"),
                format!("{reference:?}"),
            )
        }
        7 => {
            let fixtures = table_fixtures();
            let fixture = &fixtures[index % fixtures.len()];
            let pair = fixture_pair(fixture);
            let (graph, _) = GInstance::from_finstance(&prod_table(&fixture.source))
                .map_err(|e| format!("{e:?}"))?;
            let graph_leg = production_graph_leg(&pair.production, &graph);
            let fast = graph_lens_via_table(&pair, &graph);
            let commutes = fast
                .as_ref()
                .is_some_and(|fast| legs_agree(fast, &graph_leg));
            let production = SquareOutcome {
                graph: graph_leg,
                fast,
                commutes,
                outside: false,
            };
            let graph_pair = table_square_pair_graph(&pair);
            let reference = graph_pair.with_reference(|graph_lens| {
                let graph_lens = graph_lens.expect("fixture plans compile");
                pair.with_reference(|table_lens, _| {
                    reference_square(
                        &panproto_reference::lens::table_square(
                            graph_lens,
                            table_lens,
                            &fixture.source,
                        ),
                        |o| (reference_leg(&o.graph), reference_leg(&o.fast)),
                    )
                })
            });
            if squares_agree(&production, &reference) {
                Ok(())
            } else {
                Err(format!(
                    "table square: production {production:?} against reference {reference:?}"
                ))
            }
        }
        8 if C11_ACCEPTED => {
            let protocol = Protocol::default();
            let alphabet = protolens_alphabet();
            let schemas = sigma2_schemas();
            let protolens = &alphabet[index % alphabet.len()];
            let schema = &schemas[(index / alphabet.len()) % schemas.len()];
            let states = c11_graph_states(schema);
            let source = &states[edit % states.len()];
            let production_schema = prod_schema(schema);
            let production = panproto_lens::graph_protolens::apply(
                protolens,
                &production_schema,
                &protocol,
                &prod_graph(source),
            )
            .map(|applied| {
                (
                    ref_graph(&applied.view),
                    ref_complement(&applied.complement),
                )
            })
            .map_err(|_| ());
            let Ok(lens) = protolens.instantiate(&production_schema, &protocol) else {
                return Ok(());
            };
            let reference_protolens = RefProtolens::exact(
                schema,
                &ref_schema(&lens.tgt_schema),
                &ref_migration(&lens.compiled),
            );
            let instantiated =
                panproto_reference::lens::protolens_schema(&reference_protolens, schema);
            let reference =
                panproto_reference::lens::protolens_instance(&reference_protolens, schema, source)
                    .map(|application| {
                        (application.view, without_identity(&application.complement))
                    })
                    .map_err(|_| ());
            if !protolens.applicable_to(&production_schema) || instantiated.is_err() {
                return Ok(());
            }
            differ(
                "protolens instance",
                format!("{production:?}"),
                format!("{reference:?}"),
            )
        }
        _ => Ok(()),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::default())]
    #[test]
    fn randomized_lens_law_differential(
        family in 0_u8..9,
        index in any::<usize>(),
        edit in any::<usize>(),
    ) {
        if let Err(message) = property_case(family, index, edit) {
            prop_assert!(false, "{}", message);
        }
    }
}
