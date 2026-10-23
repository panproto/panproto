//! Differential tests of the tree, table and graph lenses, their paths and
//! squares against the executable reference semantics.
//!
//! Every test here converts production inputs and outputs to the models of
//! `panproto_reference` and compares the two sides exactly: views and
//! complements field by field, restored sources exactly, and refusals through
//! one typed reason map. Tests that claim identity binding compare the two
//! envelope implementations field by field; the remaining differentials omit
//! the envelope after separately exercising each implementation's binding
//! checks. Inputs come from the reference enumerators; this file holds only
//! conversions, the reason map and assertions.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_inst::identity::{ComplementShape, IdentityEnvelope as ProductionIdentityEnvelope};
use panproto_inst::{
    CompiledMigration, EdgeId, FieldPresence, GEdge, GInstance, GraphComplement, IdFloor, Node,
    NodeShape, PutDomainError, RestrictError, SynthKey, Value, ViewRef,
};
use panproto_lens::{Lens, LensError, graph_lens};
use panproto_reference::enumerate::lenses::{
    C11_FIELD, C11_VALUE, C11AddField, C11DropField, C11Layer, C11OpTerm, C11ReadField, C11Stage,
    C11SurvivalFalse, LensEquivalenceCase, PathLawCase, ProtolensSchemaCase,
    TableFusionBoundaryWitness, for_each_lens_equivalence_case, for_each_path_law_case,
    for_each_protolens_schema_case, path_law_case_count_certificate,
    protolens_schema_case_count_certificate,
};
use panproto_reference::harness::{Disagreement, agree};
use panproto_reference::lens::{GraphLens as RefGraphLens, LensError as RefLensError};
use panproto_reference::migration::Migration as RefMigration;
use panproto_reference::model::{
    GEdge as RefGEdge, Graph as RefGraph, Node as RefNode, NodeShape as RefNodeShape,
    SEdge as RefSEdge,
};
use panproto_reference::pages::representation::canonical::{
    ComplementShape as RefComplementShape, IdentityEnvelope as RefIdentityEnvelope,
};
use panproto_reference::restrict::{
    Complement as RefComplement, EngineError, LayerRefusal, NoValueLayer, Plan, PlanError,
    PredicateRefusal as RefPredicateRefusal, PutFieldRefusal, RestrictError as RefRestrictError,
    SynthKey as RefSynthKey, ValueLayer, ValueLayerSemantics,
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

fn ref_identity(identity: &ProductionIdentityEnvelope) -> RefIdentityEnvelope {
    assert_eq!(
        identity.version, 2,
        "the reference models envelope version 2"
    );
    let shape = match identity.complement_format.shape {
        ComplementShape::WType => RefComplementShape::WType,
        ComplementShape::Graph => RefComplementShape::Graph,
        ComplementShape::Functor => RefComplementShape::Functor,
        ComplementShape::Resolution => RefComplementShape::Resolution,
    };
    RefIdentityEnvelope {
        source_schema: identity.source_schema.0,
        target_schema: identity.target_schema.0,
        lens: identity.lens.0,
        shape,
        format_version: identity.complement_format.version,
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

/// A production fan violation in reference coordinates: edge ids as plain
/// numbers.
fn fan_reason(reason: &panproto_inst::FanViolation) -> String {
    use panproto_inst::FanViolation as F;
    let ids =
        |edges: &[panproto_inst::EdgeId]| edges.iter().map(|edge| edge.0).collect::<Vec<u32>>();
    match reason {
        F::MissingParent => "MissingParent".to_owned(),
        F::MultipleParents { edges } => format!("MultipleParents {:?}", ids(edges)),
        F::DuplicateLabel { label, edges } => format!("DuplicateLabel {label} {:?}", ids(edges)),
        F::UnknownLabel { label } => format!("UnknownLabel {label}"),
        F::ReservedKindOutsideFan { edge } => format!("ReservedKindOutsideFan {}", edge.0),
        F::ForeignIncidentEdge { edge } => format!("ForeignIncidentEdge {}", edge.0),
        F::UnknownHyperEdge { hyper_edge } => format!("UnknownHyperEdge {hyper_edge}"),
        other @ F::MissingChild { .. } => {
            panic!("fan violation outside the lens differential: {other:?}")
        }
    }
}

/// A reference fan violation in the same coordinates as [`fan_reason`].
fn ref_fan_reason(reason: &panproto_reference::fan::FanViolation) -> String {
    use panproto_reference::fan::FanViolation as F;
    match reason {
        F::MissingParent => "MissingParent".to_owned(),
        F::MultipleParents { edges } => format!("MultipleParents {edges:?}"),
        F::DuplicateLabel { label, edges } => format!("DuplicateLabel {label} {edges:?}"),
        F::UnknownLabel { label } => format!("UnknownLabel {label}"),
        F::ReservedKindOutsideFan { edge } => format!("ReservedKindOutsideFan {edge}"),
        F::ForeignIncidentEdge { edge } => format!("ForeignIncidentEdge {edge}"),
        F::UnknownHyperEdge { hyper_edge } => format!("UnknownHyperEdge {hyper_edge}"),
    }
}

fn bytes_reason(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(encoded, "{byte:02x}").expect("writing to a String is infallible");
    }
    encoded
}

fn value_reason(value: &Value) -> String {
    bytes_reason(&value.canonical_bytes())
}

fn ref_value_reason(value: &RefValue) -> String {
    panproto_reference::pages::values::canonical::value_bytes(value).map_or_else(
        || format!("reference noncanonical value {value:?}"),
        |bytes| bytes_reason(&bytes),
    )
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
        PutDomainError::DuplicateKey { table, key, elems } => format!(
            "DuplicateKey {table} {:?} {:?}",
            key.iter()
                .map(|(column, value)| (column.clone(), value_reason(value)))
                .collect::<Vec<_>>(),
            elems.iter().map(view_ref).collect::<Vec<_>>()
        ),
        PutDomainError::MissingRequiredCell { elem, field } => {
            format!("MissingRequiredCell {} {field}", view_ref(elem))
        }
        PutDomainError::MalformedFan { node, reason } => {
            format!("MalformedFan {node} {}", fan_reason(reason))
        }
        PutDomainError::SurvivalPredicateRefused { elem, refusal } => format!(
            "SurvivalPredicateRefused {} {}",
            view_ref(elem),
            predicate_refusal_reason(refusal)
        ),
        PutDomainError::PlanCompilation { detail } => {
            format!("PlanCompilation {detail}")
        }
    }
}

fn ref_put_domain_reason(error: &panproto_reference::lens::PutDomainError) -> String {
    ref_put_domain_reason_with(error, &ref_view_ref)
}

fn ref_put_domain_reason_with(
    error: &panproto_reference::lens::PutDomainError,
    elem_reason: &impl Fn(&panproto_reference::lens::ViewRef) -> String,
) -> String {
    use panproto_reference::lens::PutDomainError as R;
    match error {
        R::IdCollision { elem } => format!("IdCollision {}", elem_reason(elem)),
        R::SynthesizedEdited { elem } => format!("SynthesizedEdited {}", elem_reason(elem)),
        R::ContractedEdgeInserted { elem } => {
            format!("ContractedEdgeInserted {}", elem_reason(elem))
        }
        R::UnrepresentableViewEdge { elem } => {
            format!("UnrepresentableViewEdge {}", elem_reason(elem))
        }
        R::AmbiguousReverseAnchor { elem, candidates } => {
            format!(
                "AmbiguousReverseAnchor {} {candidates:?}",
                elem_reason(elem)
            )
        }
        R::SurvivalPredicateFails { elem } => {
            format!("SurvivalPredicateFails {}", elem_reason(elem))
        }
        R::DerivedCellEdited { elem, field } => {
            format!("DerivedCellEdited {} {field}", elem_reason(elem))
        }
        R::TransformNotInvertible { elem, field } => {
            format!("TransformNotInvertible {} {field}", elem_reason(elem))
        }
        R::DuplicateKey { table, key, elems } => {
            let key = key
                .iter()
                .map(|(column, value)| (column.clone(), ref_value_reason(value)))
                .collect::<Vec<_>>();
            format!(
                "DuplicateKey {table} {key:?} {:?}",
                elems.iter().map(elem_reason).collect::<Vec<_>>()
            )
        }
        R::MissingRequiredCell { elem, field } => {
            format!("MissingRequiredCell {} {field}", elem_reason(elem))
        }
        R::MalformedFan { node, reason } => {
            format!("MalformedFan {node} {}", ref_fan_reason(reason))
        }
        R::SurvivalPredicateRefused { elem, refusal } => format!(
            "SurvivalPredicateRefused {} {}",
            elem_reason(elem),
            ref_predicate_refusal_reason(refusal)
        ),
    }
}

fn predicate_refusal_reason(error: &panproto_inst::PredicateRefusal) -> String {
    use panproto_inst::PredicateRefusal as P;

    match error {
        P::AmbiguousChildField { field } => format!("AmbiguousChildField {field}"),
        P::StructuralChildNotATree { field } => format!("StructuralChildNotATree {field}"),
        P::ValueNotExpressible { field, value_type } => {
            format!("ValueNotExpressible {field} {value_type}")
        }
        P::EvaluationFailed { message } => format!("EvaluationFailed {message:?}"),
        P::NotBoolean { got } => format!("NotBoolean {got}"),
    }
}

fn ref_predicate_refusal_reason(error: &panproto_reference::restrict::PredicateRefusal) -> String {
    use panproto_reference::restrict::PredicateRefusal as P;

    match error {
        P::AmbiguousChildField { field } => format!("AmbiguousChildField {field}"),
        P::StructuralChildNotATree { field } => format!("StructuralChildNotATree {field}"),
        P::ValueNotExpressible { field, value_type } => {
            format!("ValueNotExpressible {field} {value_type}")
        }
        P::EvaluationFailed { message } => format!("EvaluationFailed {message:?}"),
        P::NotBoolean { got } => format!("NotBoolean {got}"),
    }
}

fn restrict_reason(error: &RestrictError) -> String {
    match error {
        RestrictError::At { key, error } => format!(
            "At {} {} {} {}",
            key.stage,
            key.id,
            key.id2,
            restrict_reason(error)
        ),
        RestrictError::IncoherentEdgeImage { source_edge, image } => format!(
            "IncoherentEdgeImage {:?} {:?}",
            ref_edge(source_edge),
            ref_edge(image)
        ),
        RestrictError::IncoherentPairImage { pair, image } => format!(
            "IncoherentPairImage ({},{}) {:?}",
            pair.0,
            pair.1,
            ref_edge(image)
        ),
        RestrictError::NoEdgeFound { src, tgt } => format!("NoEdgeFound {src} {tgt}"),
        RestrictError::AmbiguousEdge { src, tgt, count } => {
            format!("AmbiguousEdge {src} {tgt} {count}")
        }
        RestrictError::FanReconstructionFailed {
            hyper_edge_id,
            detail,
        } => format!("FanReconstructionFailed {hyper_edge_id} {detail:?}"),
        RestrictError::IdsExhausted { kind } => format!("IdsExhausted {kind}"),
        RestrictError::ExpansionTooLong { src, tgt, hops } => {
            format!("ExpansionTooLong {src} {tgt} {hops}")
        }
        // The production refusal names no root: a tree has exactly one.
        RestrictError::RootPruned => "RootPruned".to_owned(),
        RestrictError::MalformedFan { node, reason } => {
            format!("MalformedFan {node} {}", fan_reason(reason))
        }
        RestrictError::NonConforming {
            row,
            column,
            reason,
        } => {
            use panproto_inst::NonConformance as N;

            let reason = match reason {
                N::TableOutside { table } => format!("TableOutside {table}"),
                N::MissingCell => "MissingCell".to_owned(),
                N::StrayCell => "StrayCell".to_owned(),
                N::NestedLabeledNull => "NestedLabeledNull".to_owned(),
                N::RelationOutside { edge } => {
                    format!("RelationOutside {:?}", ref_edge(edge))
                }
                other => format!("production nonconformance {other:?}"),
            };
            format!("NonConforming {} {column:?} {reason}", row.0)
        }
        other => format!("production restrict refusal {other:?}"),
    }
}

fn ref_nonconforming_reason(
    error: &panproto_reference::rel::NonConforming,
    source: &RefTable,
) -> String {
    use panproto_reference::rel::Conformance as C;

    let reason = match &error.reason {
        C::TableOutside => source.rows.get(&error.row).map_or_else(
            || "TableOutside <missing-row>".to_owned(),
            |(table, _)| format!("TableOutside {table}"),
        ),
        C::MissingCell => "MissingCell".to_owned(),
        C::StrayCell => "StrayCell".to_owned(),
        C::NestedLabeledNull => "NestedLabeledNull".to_owned(),
        C::RelationOutside(edge) => format!("RelationOutside {edge:?}"),
        C::IncoherentPair(edge) => format!("IncoherentPair {edge:?}"),
    };
    format!("NonConforming {} {:?} {reason}", error.row, error.column)
}

/// The source-shape check performed by production's finite-map table adapter.
///
/// The lower table engine requires total rows, but the public lens first fills
/// absent declared cells with typed scratch values and removes their output
/// coordinates after restriction. Consequently, only an unknown table or a
/// stray cell is a source-shape refusal here. Nested labelled nulls and pair
/// endpoint coherence belong to other validation layers and are deliberately
/// not part of this check.
fn reference_table_get_preflight(
    source: &RefTable,
    boundary: &RelSchema,
) -> Result<(), Box<panproto_reference::rel::NonConforming>> {
    use panproto_reference::rel::{Conformance, NonConforming};

    for (row, (table, cells)) in &source.rows {
        if !boundary.tables.contains(table) {
            return Err(Box::new(NonConforming {
                row: *row,
                column: None,
                reason: Conformance::TableOutside,
            }));
        }
        let columns = boundary.columns.get(table);
        if let Some(column) = cells
            .keys()
            .find(|column| columns.is_none_or(|columns| !columns.contains_key(*column)))
        {
            return Err(Box::new(NonConforming {
                row: *row,
                column: Some(column.clone()),
                reason: Conformance::StrayCell,
            }));
        }
    }
    Ok(())
}

fn ref_restrict_reason(error: &RefRestrictError) -> String {
    match error {
        RefRestrictError::At { key, error } => format!(
            "At {} {} {} {}",
            key.stage,
            key.id,
            key.id2,
            ref_engine_reason(error)
        ),
        RefRestrictError::Plan(PlanError::IncoherentEdgeImage { source_edge, image }) => {
            format!("IncoherentEdgeImage {source_edge:?} {image:?}")
        }
        RefRestrictError::Plan(PlanError::IncoherentPairImage { pair, image }) => {
            format!("IncoherentPairImage ({},{}) {image:?}", pair.0, pair.1)
        }
        RefRestrictError::Plan(PlanError::ExpansionTooLong { src, tgt, hops }) => {
            format!("ExpansionTooLong {src} {tgt} {hops}")
        }
        RefRestrictError::RootPruned { .. } => "RootPruned".to_owned(),
        RefRestrictError::MalformedFan { node, reason } => {
            format!("MalformedFan {node} {}", ref_fan_reason(reason))
        }
        other @ RefRestrictError::Plan(_) => format!("reference restrict refusal {other:?}"),
    }
}

fn ref_engine_reason(error: &EngineError) -> String {
    match error {
        EngineError::NoEdgeFound { src, tgt } => format!("NoEdgeFound {src} {tgt}"),
        EngineError::AmbiguousEdge { src, tgt, count } => {
            format!("AmbiguousEdge {src} {tgt} {count}")
        }
        // The production engine carries the input label in both fields of
        // `IncoherentEdgeImage`; retain both coordinates in the common form.
        EngineError::IncoherentEdgeImage { label } => {
            format!("IncoherentEdgeImage {label:?} {label:?}")
        }
        EngineError::FanReconstructionFailed { hyper_edge, label } => format!(
            "FanReconstructionFailed {hyper_edge} {:?}",
            format!("two children are relabelled `{label}`")
        ),
        EngineError::IdsExhausted { nodes } => {
            format!("IdsExhausted {}", if *nodes { "node" } else { "edge" })
        }
        EngineError::ExpansionTooLong { src, tgt, hops } => {
            format!("ExpansionTooLong {src} {tgt} {hops}")
        }
        other => format!("reference engine refusal {other:?}"),
    }
}

fn assert_put_domain_reason_maps_retain_payloads() {
    let production_duplicate = PutDomainError::DuplicateKey {
        table: "T".into(),
        key: vec![("k".to_owned(), Value::Int(7))],
        elems: vec![ViewRef::Node(2), ViewRef::Node(3)],
    };
    let reference_duplicate = panproto_reference::lens::PutDomainError::DuplicateKey {
        table: "T".to_owned(),
        key: vec![("k".to_owned(), RefValue::Int(7))],
        elems: vec![
            panproto_reference::lens::ViewRef::Node(2),
            panproto_reference::lens::ViewRef::Node(3),
        ],
    };
    assert_eq!(
        put_domain_reason(&production_duplicate),
        ref_put_domain_reason(&reference_duplicate),
        "duplicate-key payloads have one common exact representation"
    );

    let reference_derived = panproto_reference::lens::PutDomainError::DerivedCellEdited {
        elem: panproto_reference::lens::ViewRef::Node(4),
        field: "derived".to_owned(),
    };
    let one_error_report = SquareError::PutDomainReport(vec![reference_duplicate.clone()]);
    let two_error_report =
        SquareError::PutDomainReport(vec![reference_duplicate.clone(), reference_derived.clone()]);
    let reversed_report =
        SquareError::PutDomainReport(vec![reference_derived, reference_duplicate]);
    assert_ne!(
        reference_square_reason(&one_error_report),
        reference_square_reason(&two_error_report),
        "square-reason normalization retains every put-domain report entry"
    );
    assert_ne!(
        reference_square_reason(&two_error_report),
        reference_square_reason(&reversed_report),
        "square-reason normalization retains put-domain report order"
    );

    let production_predicate = PutDomainError::SurvivalPredicateRefused {
        elem: ViewRef::Node(4),
        refusal: panproto_inst::PredicateRefusal::NotBoolean {
            got: "int".to_owned(),
        },
    };
    let reference_predicate = panproto_reference::lens::PutDomainError::SurvivalPredicateRefused {
        elem: panproto_reference::lens::ViewRef::Node(4),
        refusal: panproto_reference::restrict::PredicateRefusal::NotBoolean {
            got: "int".to_owned(),
        },
    };
    assert_eq!(
        put_domain_reason(&production_predicate),
        ref_put_domain_reason(&reference_predicate),
        "predicate-refusal payloads have one common exact representation"
    );
}

fn assert_restrict_reason_maps_retain_payloads_and_locations() {
    let production_outside = RestrictError::NonConforming {
        row: panproto_inst::RowId(7),
        column: None,
        reason: panproto_inst::NonConformance::TableOutside {
            table: "outside".into(),
        },
    };
    let reference_outside = panproto_reference::rel::NonConforming {
        row: 7,
        column: None,
        reason: panproto_reference::rel::Conformance::TableOutside,
    };
    let mut outside_source = RefTable::default();
    outside_source
        .rows
        .insert(7, ("outside".to_owned(), BTreeMap::new()));
    assert_eq!(
        restrict_reason(&production_outside),
        ref_nonconforming_reason(&reference_outside, &outside_source),
        "table-outside normalization retains the offending table"
    );

    let production_ambiguous = RestrictError::At {
        key: panproto_inst::segment::ErrorKey {
            stage: 2,
            id: 5,
            id2: 8,
            kind: 1,
        },
        error: Box::new(RestrictError::AmbiguousEdge {
            src: "A".to_owned(),
            tgt: "B".to_owned(),
            count: 2,
        }),
    };
    let reference_ambiguous = RefRestrictError::At {
        key: panproto_reference::restrict::ErrorKey {
            stage: 2,
            id: 5,
            id2: 8,
        },
        error: EngineError::AmbiguousEdge {
            src: "A".to_owned(),
            tgt: "B".to_owned(),
            count: 2,
        },
    };
    assert_eq!(
        restrict_reason(&production_ambiguous),
        ref_restrict_reason(&reference_ambiguous),
        "located ambiguous-edge refusals have one common exact representation"
    );

    let other_location = RestrictError::At {
        key: panproto_inst::segment::ErrorKey {
            stage: 2,
            id: 5,
            id2: 9,
            kind: 1,
        },
        error: Box::new(RestrictError::AmbiguousEdge {
            src: "A".to_owned(),
            tgt: "B".to_owned(),
            count: 2,
        }),
    };
    assert_ne!(
        restrict_reason(&production_ambiguous),
        restrict_reason(&other_location),
        "the common reason retains every semantic ErrorKey coordinate"
    );

    let source_edge = RefSEdge::new("A", "B", "k", Some("source"));
    let image_edge = RefSEdge::new("A1", "B1", "k", Some("image"));
    let production_incoherent = RestrictError::IncoherentEdgeImage {
        source_edge: Box::new(prod_edge(&source_edge)),
        image: Box::new(prod_edge(&image_edge)),
    };
    let reference_incoherent = RefRestrictError::Plan(PlanError::IncoherentEdgeImage {
        source_edge: Box::new(source_edge.clone()),
        image: Box::new(image_edge),
    });
    assert_eq!(
        restrict_reason(&production_incoherent),
        ref_restrict_reason(&reference_incoherent),
        "incoherent-edge normalization retains both source and image"
    );
    let other_image = RestrictError::IncoherentEdgeImage {
        source_edge: Box::new(prod_edge(&source_edge)),
        image: Box::new(prod_edge(&RefSEdge::new("A1", "B1", "k", Some("other")))),
    };
    assert_ne!(
        restrict_reason(&production_incoherent),
        restrict_reason(&other_image),
        "incoherent-edge normalization is injective in the image"
    );
}

#[test]
fn refusal_reason_maps_retain_shared_payloads_and_locations() {
    assert_put_domain_reason_maps_retain_payloads();
    assert_restrict_reason_maps_retain_payloads_and_locations();
}

#[test]
fn table_get_preflight_matches_the_sparse_finite_map_contract() {
    let column = RefSEdge::new("a", "value", "attribute", Some("x"));
    let relation = RefSEdge::new("a", "b", "relation", Some("ab"));
    let mut boundary = RelSchema::default();
    boundary.tables.extend(["a".to_owned(), "b".to_owned()]);
    boundary.relations.insert(relation.clone());
    boundary
        .columns
        .insert("a".to_owned(), BTreeMap::from([("x".to_owned(), column)]));

    let nested_null = RefValue::Unknown(BTreeMap::from([(
        "nested".to_owned(),
        RefValue::LabeledNull(1),
    )]));
    let mut source = RefTable::default();
    source.rows.insert(1, ("a".to_owned(), BTreeMap::new()));
    source.rows.insert(
        2,
        (
            "a".to_owned(),
            BTreeMap::from([("x".to_owned(), nested_null)]),
        ),
    );
    source.pairs.insert((relation, 1, 2));

    assert_eq!(reference_table_get_preflight(&source, &boundary), Ok(()));
    assert!(
        panproto_reference::rel::conform(&source, &boundary.side()).is_err(),
        "the generic relational validator is intentionally stronger than table get"
    );

    source
        .rows
        .get_mut(&1)
        .unwrap()
        .1
        .insert("stray".to_owned(), RefValue::Int(0));
    let stray = reference_table_get_preflight(&source, &boundary).unwrap_err();
    assert_eq!(
        ref_nonconforming_reason(&stray, &source),
        "NonConforming 1 Some(\"stray\") StrayCell"
    );

    source
        .rows
        .insert(0, ("outside".to_owned(), BTreeMap::new()));
    let outside = reference_table_get_preflight(&source, &boundary).unwrap_err();
    assert_eq!(
        ref_nonconforming_reason(&outside, &source),
        "NonConforming 0 None TableOutside outside"
    );
}

fn lens_reason(error: &LensError) -> Reason {
    match error {
        LensError::PutDomain(found) => Reason::PutDomain(put_domain_reason(found)),
        LensError::Restrict(found) => Reason::Restrict(restrict_reason(found)),
        LensError::Identity(found) => Reason::Identity(format!("{found:?}")),
        // Keep exhaustive differentials total. An operational family outside
        // the normalized contract must disagree visibly, not abort the run.
        other => Reason::Identity(format!("production operational refusal {other:?}")),
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

fn assert_agree<T>(result: Result<T, Disagreement>) -> T {
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
    let census = agree_in_parallel_outside(
        bound,
        units,
        context,
        expand,
        &(production, reference),
        eq,
        |_, _| false,
    )?;
    Ok(census.compared)
}

/// The cases a comparison visited: all of them, and among them the cases in
/// a recorded specification gap whose two sides differ, with the first few
/// as witnesses.
#[derive(Debug, Default)]
struct Census {
    compared: u64,
    gap: u64,
    witnesses: Vec<String>,
}

/// How many specification-gap witnesses a census keeps.
const GAP_WITNESSES: usize = 3;

/// [`agree_in_parallel`] with one recorded specification gap: a case for
/// which `in_gap` holds and whose two sides differ is counted (and the first
/// [`GAP_WITNESSES`] kept) instead of failing; every other difference is a
/// disagreement exactly as in [`agree_in_parallel`]. Cases in the gap whose
/// two sides agree are counted as agreeing.
/// `sides` is the production and the reference side, in that order.
fn agree_in_parallel_outside<'a, U, C, I, P, R, FP, FR>(
    bound: &str,
    units: &'a [U],
    context: impl Fn() -> C + Sync,
    expand: impl Fn(&C, &'a U) -> Vec<I> + Sync,
    sides: &(FP, FR),
    eq: impl Fn(&P, &R) -> bool + Sync,
    in_gap: impl Fn(&C, &I) -> bool + Sync,
) -> Result<Census, Disagreement>
where
    U: Sync,
    I: std::fmt::Debug,
    P: std::fmt::Debug,
    R: std::fmt::Debug,
    FP: Fn(&C, &I) -> P + Sync,
    FR: Fn(&C, &I) -> R + Sync,
{
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    const BLOCK: usize = 32;
    type Outcome = Result<Census, (usize, Disagreement)>;

    let (production, reference) = (&sides.0, &sides.1);
    let threads = std::env::var("PANPROTO_TEST_WORKERS")
        .ok()
        .and_then(|workers| workers.parse::<usize>().ok())
        .filter(|workers| *workers > 0)
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, std::num::NonZero::get));
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let outcomes: Vec<Outcome> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let context = context();
                    let mut census = Census::default();
                    loop {
                        let start = next.fetch_add(BLOCK, Ordering::Relaxed);
                        if start >= units.len() || stop.load(Ordering::Relaxed) {
                            return Ok(census);
                        }
                        for (offset, unit) in units[start..].iter().take(BLOCK).enumerate() {
                            for case in expand(&context, unit) {
                                let (p, r) = (production(&context, &case), reference(&context, &case));
                                census.compared += 1;
                                if eq(&p, &r) {
                                    continue;
                                }
                                if in_gap(&context, &case) {
                                    census.gap += 1;
                                    if census.witnesses.len() < GAP_WITNESSES {
                                        census.witnesses.push(format!(
                                            "input: {case:?}\n  production: {p:?}\n  reference: {r:?}"
                                        ));
                                    }
                                    continue;
                                }
                                let disagreement = Disagreement {
                                    bound: bound.to_owned(),
                                    input: format!("{case:?}"),
                                    production: format!("{p:?}"),
                                    reference: format!("{r:?}"),
                                };
                                println!("{disagreement}");
                                stop.store(true, Ordering::Relaxed);
                                return Err((start + offset, disagreement));
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
    let mut total = Census::default();
    let mut first: Option<(usize, Disagreement)> = None;
    for outcome in outcomes {
        match outcome {
            Ok(census) => {
                total.compared += census.compared;
                total.gap += census.gap;
                total.witnesses.extend(census.witnesses);
            }
            Err((unit, disagreement)) => {
                if first.as_ref().is_none_or(|(at, _)| unit < *at) {
                    first = Some((unit, disagreement));
                }
            }
        }
    }
    total.witnesses.sort();
    total.witnesses.truncate(GAP_WITNESSES);
    first.map_or(Ok(total), |(_, disagreement)| Err(disagreement))
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
        sources[usize::from(fixture == Gf5Fixture::VisibleThenHidden)] += 1;
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
    TABLE_FIXTURE_TABLE_KIND, TABLE_FIXTURE_VALUE_KIND, TableComplementLawCase, TableFixture,
    TableGetPutLawCase, TableGetRefusalCase, TablePairPutCase, TablePutRefusalCase, WRITER_EDITS,
    WRITER_VALUE_KIND, WriterFixture, derived_writer_count_certificate,
    for_each_derived_writer_fixture, for_each_table_complement_construction,
    for_each_table_complement_law_case, for_each_table_fixture, for_each_table_get_put_law_case,
    for_each_writer_fixture, last_explicit_writer_count_certificate,
    table_complement_construction_count_certificate, table_complement_law_count_certificate,
    table_fixture_count_certificate, table_fixture_get_put_count_certificate,
    table_get_put_law_count_certificate, table_get_refusal_count_certificate,
    table_pair_put_count_certificate, table_put_refusal_count_certificate, table_view_edits,
    table_view_edits_count, writer_count_certificate,
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
    layer: &'static dyn ValueLayer,
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
            layer: &NoValueLayer,
        }
    }

    fn from_stage(stage: &C11Stage) -> Self {
        let layer: &'static dyn ValueLayer = match stage.layer {
            C11Layer::None => &NoValueLayer,
            C11Layer::SurvivalFalse => &C11SurvivalFalse,
            C11Layer::AddField => &C11AddField,
            C11Layer::DropField => &C11DropField,
            C11Layer::ReadField => &C11ReadField,
            C11Layer::OpTerm => &C11OpTerm,
        };
        let kinds = std::collections::BTreeSet::from(["obj".to_owned()]);
        let classify = |schema: &RefSchema| {
            panproto_reference::rel::classify(schema, &std::collections::BTreeSet::new(), &kinds)
        };
        let source_boundary = classify(&stage.source);
        let target_boundary = classify(&stage.target);
        let plan = match (&source_boundary, &target_boundary) {
            (Ok(source), Ok(target)) => panproto_reference::table::compile_for_tables(
                &stage.source,
                &stage.target,
                source,
                target,
                &stage.migration,
            ),
            _ => Err(TablePlanError::NotTableRepresentable),
        };
        let boundaries = (
            source_boundary.unwrap_or_default(),
            target_boundary.unwrap_or_default(),
        );
        Self {
            src: stage.source.clone(),
            tgt: stage.target.clone(),
            migration: stage.migration.clone(),
            boundaries,
            plan,
            production: production_stage(stage),
            protocol: Protocol {
                non_value_kinds: kinds,
                ..Protocol::default()
            },
            layer,
        }
    }

    const fn protocols(&self) -> BoundaryProtocols<'_> {
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
            self.layer,
        )
        .expect("plan compiled for these boundaries");
        run(&lens, plan)
    }

    fn try_with_reference<T>(
        &self,
        run: impl FnOnce(&RefTableLens<'_>, &TablePlan) -> T,
    ) -> Result<T, Reason> {
        let plan = self.plan.as_ref().map_err(|error| match error {
            TablePlanError::Plan(error) => Reason::Restrict(ref_restrict_reason(
                &RefRestrictError::Plan((**error).clone()),
            )),
            other => Reason::Restrict(format!("reference table plan refusal {other:?}")),
        })?;
        let lens = RefTableLens::new(
            &self.src,
            &self.tgt,
            panproto_reference::table::TableBoundaries::new(&self.boundaries.0, &self.boundaries.1),
            &self.migration,
            plan,
            &BTreeMap::new(),
            self.layer,
        )
        .map_err(|error| Reason::Restrict(format!("reference table lens refusal {error:?}")))?;
        Ok(run(&lens, plan))
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

struct RefusingTablePredicate;

impl ValueLayer for RefusingTablePredicate {
    fn canonical_semantics(&self) -> ValueLayerSemantics {
        ValueLayerSemantics {
            domain: "panproto-reference/table-law/refusing-predicate",
            version: 1,
            canonical_payload: Vec::new(),
        }
    }

    fn survives(&self, _: &RefGraph, _: &RefNode) -> Result<bool, RefPredicateRefusal> {
        Err(RefPredicateRefusal::NotBoolean {
            got: "int".to_owned(),
        })
    }

    fn transform(
        &self,
        _: &RefGraph,
        node: &RefNode,
    ) -> Result<(BTreeMap<String, RefValue>, Option<RefPresence>), LayerRefusal> {
        Ok((node.fields.clone(), node.value.clone()))
    }

    fn restore(
        &self,
        view: &RefNode,
        original_fields: Option<&BTreeMap<String, RefValue>>,
        original_value: Option<&Option<RefPresence>>,
    ) -> Result<(BTreeMap<String, RefValue>, Option<RefPresence>), PutFieldRefusal> {
        Ok((
            original_fields
                .cloned()
                .unwrap_or_else(|| view.fields.clone()),
            original_value
                .cloned()
                .unwrap_or_else(|| view.value.clone()),
        ))
    }
}

struct RefusingTableTransform;

impl ValueLayer for RefusingTableTransform {
    fn canonical_semantics(&self) -> ValueLayerSemantics {
        ValueLayerSemantics {
            domain: "panproto-reference/table-law/refusing-transform",
            version: 1,
            canonical_payload: Vec::new(),
        }
    }

    fn survives(&self, _: &RefGraph, _: &RefNode) -> Result<bool, RefPredicateRefusal> {
        Ok(true)
    }

    fn transform(
        &self,
        _: &RefGraph,
        _: &RefNode,
    ) -> Result<(BTreeMap<String, RefValue>, Option<RefPresence>), LayerRefusal> {
        Err(LayerRefusal {
            kind: "expression".to_owned(),
            detail: "missing variable".to_owned(),
        })
    }

    fn restore(
        &self,
        view: &RefNode,
        original_fields: Option<&BTreeMap<String, RefValue>>,
        original_value: Option<&Option<RefPresence>>,
    ) -> Result<(BTreeMap<String, RefValue>, Option<RefPresence>), PutFieldRefusal> {
        Ok((
            original_fields
                .cloned()
                .unwrap_or_else(|| view.fields.clone()),
            original_value
                .cloned()
                .unwrap_or_else(|| view.value.clone()),
        ))
    }
}

struct NonInvertibleTableTransform;

impl ValueLayer for NonInvertibleTableTransform {
    fn canonical_semantics(&self) -> ValueLayerSemantics {
        ValueLayerSemantics {
            domain: "panproto-reference/table-law/noninvertible-absolute-value",
            version: 1,
            canonical_payload: b"field=x\0operation=abs".to_vec(),
        }
    }

    fn survives(&self, _: &RefGraph, _: &RefNode) -> Result<bool, RefPredicateRefusal> {
        Ok(true)
    }

    fn transform(
        &self,
        _: &RefGraph,
        node: &RefNode,
    ) -> Result<(BTreeMap<String, RefValue>, Option<RefPresence>), LayerRefusal> {
        let mut fields = node.fields.clone();
        if let Some(RefValue::Int(value)) = fields.get("x") {
            fields.insert("x".to_owned(), RefValue::Int(value.abs()));
        }
        Ok((fields, node.value.clone()))
    }

    fn restore(
        &self,
        _: &RefNode,
        _: Option<&BTreeMap<String, RefValue>>,
        _: Option<&Option<RefPresence>>,
    ) -> Result<(BTreeMap<String, RefValue>, Option<RefPresence>), PutFieldRefusal> {
        Err(PutFieldRefusal::TransformNotInvertible {
            field: "x".to_owned(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableGetRefusalKind {
    NotTableRepresentable,
    SurvivalRulesDisagree,
    IncoherentEdgeImage,
    NoEdgeFound,
    AmbiguousEdge,
    IncoherentPairImage,
    NonConforming,
    PredicateRefusal,
    TransformRefusal,
}

impl From<TableGetRefusalCase> for TableGetRefusalKind {
    fn from(case: TableGetRefusalCase) -> Self {
        match case {
            TableGetRefusalCase::NotTableRepresentable => Self::NotTableRepresentable,
            TableGetRefusalCase::SurvivalRulesDisagree => Self::SurvivalRulesDisagree,
            TableGetRefusalCase::IncoherentEdgeImage => Self::IncoherentEdgeImage,
            TableGetRefusalCase::NoEdgeFound => Self::NoEdgeFound,
            TableGetRefusalCase::AmbiguousEdge => Self::AmbiguousEdge,
            TableGetRefusalCase::IncoherentPairImage => Self::IncoherentPairImage,
            TableGetRefusalCase::NonConforming => Self::NonConforming,
            TableGetRefusalCase::PredicateRefusal => Self::PredicateRefusal,
            TableGetRefusalCase::TransformRefusal => Self::TransformRefusal,
        }
    }
}

fn production_table_refusal_kind(error: &RestrictError) -> TableGetRefusalKind {
    if let RestrictError::At { error, .. } = error {
        return production_table_refusal_kind(error);
    }
    match error {
        RestrictError::NotTableRepresentable { .. } => TableGetRefusalKind::NotTableRepresentable,
        RestrictError::SurvivalRulesDisagree { .. } => TableGetRefusalKind::SurvivalRulesDisagree,
        RestrictError::IncoherentEdgeImage { .. } => TableGetRefusalKind::IncoherentEdgeImage,
        RestrictError::NoEdgeFound { .. } => TableGetRefusalKind::NoEdgeFound,
        RestrictError::AmbiguousEdge { .. } => TableGetRefusalKind::AmbiguousEdge,
        RestrictError::IncoherentPairImage { .. } => TableGetRefusalKind::IncoherentPairImage,
        RestrictError::NonConforming { .. } => TableGetRefusalKind::NonConforming,
        RestrictError::SurvivalPredicateFailed { .. }
        | RestrictError::AmbiguousChildField { .. }
        | RestrictError::StructuralChildNotATree { .. }
        | RestrictError::ValueNotExpressible { .. } => TableGetRefusalKind::PredicateRefusal,
        RestrictError::FieldTransformFailed { .. } => TableGetRefusalKind::TransformRefusal,
        other => panic!("unexpected production table-get refusal: {other:?}"),
    }
}

fn reference_table_plan_refusal_kind(error: &TablePlanError) -> TableGetRefusalKind {
    match error {
        TablePlanError::NotTableRepresentable => TableGetRefusalKind::NotTableRepresentable,
        TablePlanError::SurvivalRulesDisagree(_) => TableGetRefusalKind::SurvivalRulesDisagree,
        TablePlanError::Plan(error) => match error.as_ref() {
            PlanError::IncoherentEdgeImage { .. } => TableGetRefusalKind::IncoherentEdgeImage,
            PlanError::IncoherentPairImage { .. } => TableGetRefusalKind::IncoherentPairImage,
            other => panic!("unexpected reference table-plan refusal: {other:?}"),
        },
        TablePlanError::Functor(error) => match error.as_ref() {
            panproto_reference::rel::FunctorError::IncoherentEdgeImage { .. } => {
                TableGetRefusalKind::IncoherentEdgeImage
            }
            other => panic!("unexpected reference table-functor refusal: {other:?}"),
        },
        other => panic!("unexpected reference table-plan refusal: {other:?}"),
    }
}

fn reference_table_engine_refusal_kind(error: &RefRestrictError) -> TableGetRefusalKind {
    match error {
        RefRestrictError::At { error, .. } => match error {
            EngineError::NoEdgeFound { .. } => TableGetRefusalKind::NoEdgeFound,
            EngineError::AmbiguousEdge { .. } => TableGetRefusalKind::AmbiguousEdge,
            EngineError::Predicate(_) => TableGetRefusalKind::PredicateRefusal,
            EngineError::Layer(_) => TableGetRefusalKind::TransformRefusal,
            other => panic!("unexpected reference table-engine refusal: {other:?}"),
        },
        RefRestrictError::Plan(error) => match error {
            PlanError::IncoherentEdgeImage { .. } => TableGetRefusalKind::IncoherentEdgeImage,
            PlanError::IncoherentPairImage { .. } => TableGetRefusalKind::IncoherentPairImage,
            other => panic!("unexpected reference table plan refusal: {other:?}"),
        },
        other => panic!("unexpected reference table-get refusal: {other:?}"),
    }
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

fn table_complement_certificate_is_exact() {
    let encoded = serde_json::to_string(&TableComplement::empty())
        .expect("the production complement is serializable");
    let production_field_order: Vec<&str> = encoded
        .strip_prefix('{')
        .and_then(|body| body.strip_suffix('}'))
        .expect("a complement serializes as one JSON object")
        .split(',')
        .map(|field| {
            field
                .split_once(':')
                .expect("each serialized complement field has a value")
                .0
                .trim_matches('"')
        })
        .collect();
    assert_eq!(production_field_order, RefTableComplement::FIELD_ORDER);

    let mut constructions = Vec::new();
    for_each_table_complement_construction(|construction| constructions.push(construction));
    assert_eq!(
        constructions.len(),
        table_complement_construction_count_certificate()
    );

    let mut saw_dropped_rows = false;
    let mut saw_dropped_cells = false;
    let mut saw_dropped_pairs = false;
    let mut saw_original_edges = false;
    let mut saw_original_tables = false;
    let mut saw_original_cells = false;
    let mut saw_synthesized_pairs = false;
    for construction in &constructions {
        let name = construction.name;
        let pair = TablePair::new(
            construction.source_schema.clone(),
            construction.target_schema.clone(),
            construction.migration.clone(),
            WRITER_VALUE_KIND,
            ScalarKind::Any,
        );
        let source = &construction.source;
        let production = production_table_get(&pair, source)
            .unwrap_or_else(|error| panic!("production get refused {name}: {error:?}"));
        let reference = reference_table_get(&pair, source)
            .unwrap_or_else(|error| panic!("reference get refused {name}: {error:?}"));
        assert_eq!(production, reference, "table complement at {name}");
        saw_dropped_rows |= !reference.1.dropped_rows.is_empty();
        saw_dropped_cells |= !reference.1.dropped_cells.is_empty();
        saw_dropped_pairs |= !reference.1.dropped_fk_pairs.is_empty();
        saw_original_edges |= !reference.1.original_fk_edges.is_empty();
        saw_original_tables |= !reference.1.original_tables.is_empty();
        saw_original_cells |= !reference.1.original_cells.is_empty();
        saw_synthesized_pairs |= !reference.1.synthesized_fk_pairs.is_empty();

        let production_identity =
            table_lens::get(&pair.production, &prod_table(source), pair.protocols())
                .expect("the production construction gets")
                .1
                .identity
                .expect("production table get stamps an identity");
        pair.with_reference(|lens, _| {
            let reference_identity = lens
                .get(source)
                .expect("the reference construction gets")
                .1
                .identity
                .expect("reference table get stamps an identity");
            assert_eq!(
                production_semantic_envelope(&pair.production, &production_identity),
                reference_semantic_envelope(
                    &pair.src,
                    &pair.tgt,
                    &pair.migration,
                    &reference_identity,
                ),
                "semantic table identity at {name}"
            );
        });

        let production_put = production_table_put(&pair, source, &reference.0);
        let reference_put = reference_table_put(&pair, source, &reference.0);
        assert_eq!(production_put, reference_put, "unedited put at {name}");
        assert_eq!(reference_put.put, Ok(source.clone()), "GetPut at {name}");
    }
    assert!(saw_dropped_rows);
    assert!(saw_dropped_cells);
    assert!(saw_dropped_pairs);
    assert!(saw_original_edges);
    assert!(saw_original_tables);
    assert!(saw_original_cells);
    assert!(saw_synthesized_pairs);
}

fn table_refusal_pair(case: TableGetRefusalCase) -> TablePair {
    let fixture = case.fixture();
    let mut structural = fixture.migration.clone();
    structural.conditional.clear();
    structural.transformed.clear();
    let mut pair = TablePair::new(
        fixture.source_schema,
        fixture.target_schema,
        structural.clone(),
        WRITER_VALUE_KIND,
        ScalarKind::Int {
            bits: 64,
            signed: true,
        },
    );
    pair.migration = fixture.migration;
    pair.plan = panproto_reference::table::compile_for_tables(
        &pair.src,
        &pair.tgt,
        &pair.boundaries.0,
        &pair.boundaries.1,
        &pair.migration,
    );
    pair.production.compiled = prod_migration(&structural);
    match case {
        TableGetRefusalCase::PredicateRefusal => {
            pair.production
                .compiled
                .conditional_survival
                .insert("t".into(), panproto_expr::Expr::Var("x".into()));
            pair.layer = &RefusingTablePredicate;
        }
        TableGetRefusalCase::TransformRefusal => {
            pair.production.compiled.field_transforms.insert(
                "t".into(),
                vec![panproto_inst::FieldTransform::ApplyExpr {
                    key: "x".to_owned(),
                    expr: panproto_expr::Expr::Var("missing".into()),
                    inverse: None,
                    coercion_class: panproto_gat::CoercionClass::Projection,
                }],
            );
            pair.layer = &RefusingTableTransform;
        }
        _ => {}
    }
    pair
}

fn assert_table_get_refusal_case(case: TableGetRefusalCase) {
    let expected = TableGetRefusalKind::from(case);
    let fixture = case.fixture();
    let pair = table_refusal_pair(case);
    let production = table_lens::get(
        &pair.production,
        &prod_table(&fixture.source),
        pair.protocols(),
    )
    .unwrap_err();
    let LensError::Restrict(production) = production else {
        panic!(
            "production table get returned the wrong refusal for {}: {production:?}",
            case.name()
        );
    };
    assert_eq!(
        production_table_refusal_kind(&production),
        expected,
        "production refusal for {}",
        case.name()
    );

    let reference = if case == TableGetRefusalCase::NonConforming {
        reference_table_get_preflight(&fixture.source, &pair.boundaries.0)
            .map_or(TableGetRefusalKind::NonConforming, |()| {
                panic!("reference preflight accepted {}", case.name())
            })
    } else if let Err(error) = &pair.plan {
        reference_table_plan_refusal_kind(error)
    } else {
        pair.with_reference(|lens, _| {
            let error = lens
                .get(&fixture.source)
                .expect_err("the named reference table-get case must refuse");
            reference_table_engine_refusal_kind(&error)
        })
    };
    assert_eq!(reference, expected, "reference refusal for {}", case.name());
}

fn table_get_refusals_are_typed_and_complete() {
    let mut refusals = Vec::new();
    panproto_reference::enumerate::lenses::for_each_table_get_refusal(|case| {
        refusals.push(case);
    });
    assert_eq!(refusals.len(), table_get_refusal_count_certificate());
    for case in refusals {
        assert_table_get_refusal_case(case);
    }
}

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
    let last_explicit_cases = cases
        .iter()
        .filter(|(fixture, target, _)| {
            fixture
                .images
                .iter()
                .filter(|image| **image == Some(*target))
                .count()
                == 2
        })
        .count();
    assert_eq!(
        last_explicit_cases,
        last_explicit_writer_count_certificate()
    );
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
                let writer = fixture
                    .expected_writer(target)
                    .expect("an image column has a certified writer")
                    .to_owned();
                assert_eq!(
                    panproto_reference::table::TablePlan::writer(plan, "t", target),
                    Some(writer.clone()),
                    "the reference implementation realizes the certified writer"
                );
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

fn target_only_columns_refuse_with_exact_derived_cell_errors() {
    let mut fixtures = Vec::new();
    for_each_derived_writer_fixture(|fixture| fixtures.push(fixture));
    assert_eq!(fixtures.len(), derived_writer_count_certificate());

    for fixture in fixtures {
        let pair = TablePair::new(
            fixture.source_schema(),
            fixture.target_schema(),
            fixture.migration(),
            WRITER_VALUE_KIND,
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        );
        let source = fixture.source();
        let (production_view, _) =
            production_table_get(&pair, &source).expect("production derived-column get");
        let (reference_view, _) =
            reference_table_get(&pair, &source).expect("reference derived-column get");
        assert_eq!(production_view, reference_view);
        let mut edited = reference_view;
        edited
            .rows
            .get_mut(&0)
            .expect("fixture view has row zero")
            .1
            .insert("z".to_owned(), RefValue::Int(fixture.edit));

        let production = production_table_put(&pair, &source, &edited);
        let reference = reference_table_put(&pair, &source, &edited);
        let expected = TablePutOutcome {
            put: Err(Reason::PutDomain("DerivedCellEdited Row(0) z".to_owned())),
            domain: Err(vec!["DerivedCellEdited Row(0) z".to_owned()]),
        };
        assert_eq!(production, reference);
        assert_eq!(reference, expected);
    }
}

fn assert_table_pair_put_case(case: TablePairPutCase) {
    let fixture = case.fixture();
    let pair = TablePair::new(
        fixture.source_schema,
        fixture.target_schema,
        fixture.migration,
        WRITER_VALUE_KIND,
        ScalarKind::Any,
    );
    let (production_view, production_complement) = table_lens::get(
        &pair.production,
        &prod_table(&fixture.source),
        pair.protocols(),
    )
    .expect("production pair fixture gets");
    let (mut view, reference_complement) =
        reference_table_get(&pair, &fixture.source).expect("reference pair fixture gets");
    assert_eq!(ref_table(&production_view), view);
    assert_eq!(
        ref_table_complement(&production_complement),
        reference_complement
    );

    let image_pair = (fixture.image.clone(), 0, 2);
    assert_eq!(
        reference_complement
            .synthesized_fk_pairs
            .contains(&image_pair),
        case.has_chain(),
        "synthesized-pair classification for {case:?}"
    );
    if case.has_direct() {
        assert!(
            !reference_complement
                .original_fk_edges
                .contains_key(&image_pair),
            "the uniquely recoverable direct origin needs no residue"
        );
    }

    if case.view_has_pair() {
        view.pairs.insert(image_pair.clone());
    } else {
        view.pairs.remove(&image_pair);
    }
    let production = production_table_put(&pair, &fixture.source, &view);
    let reference = reference_table_put(&pair, &fixture.source, &view);
    assert_eq!(production.put, reference.put, "pair put at {case:?}");
    assert_eq!(
        production.domain, reference.domain,
        "complete pair-domain report at {case:?}"
    );
    match case {
        TablePairPutCase::InsertContracted => assert!(matches!(
            &reference.put,
            Err(Reason::PutDomain(reason)) if reason.starts_with("ContractedEdgeInserted ")
        )),
        TablePairPutCase::SynthesizedRemoved | TablePairPutCase::TwoOriginRemoved => {
            assert!(matches!(
                &reference.put,
                Err(Reason::PutDomain(reason)) if reason.starts_with("SynthesizedEdited ")
            ));
        }
        TablePairPutCase::SynthesizedUnedited | TablePairPutCase::TwoOriginUnedited => {
            assert_eq!(reference.put, Ok(fixture.source));
        }
    }
}

#[test]
fn table_pair_puts_agree_with_the_reference() {
    assert_eq!(
        TablePairPutCase::ALL.len(),
        table_pair_put_count_certificate()
    );
    for case in TablePairPutCase::ALL {
        assert_table_pair_put_case(case);
    }
}

fn named_table_construction(
    name: &str,
) -> panproto_reference::enumerate::lenses::TableComplementConstruction {
    let mut found = None;
    for_each_table_complement_construction(|construction| {
        if construction.name == name {
            found = Some(construction);
        }
    });
    found.unwrap_or_else(|| panic!("missing named table construction {name}"))
}

fn assert_table_put_identity_refusal() {
    let fixture = TableFixture {
        columns: 0,
        retained: 0,
        source: RefTable::default(),
    };
    let pair = fixture_pair(&fixture);
    let (production_view, mut production_complement) = table_lens::get(
        &pair.production,
        &prod_table(&fixture.source),
        pair.protocols(),
    )
    .expect("the identity-refusal fixture gets in production");
    let (reference_view, mut reference_complement) = pair.with_reference(|lens, _| {
        lens.get(&fixture.source)
            .expect("the identity-refusal fixture gets in the reference")
    });
    assert_eq!(ref_table(&production_view), reference_view);

    production_complement
        .identity
        .as_mut()
        .expect("production get stamps an identity")
        .lens
        .0[0] ^= 1;
    reference_complement
        .identity
        .as_mut()
        .expect("reference get stamps an identity")
        .lens[0] ^= 1;

    let production = table_lens::put(
        &pair.production,
        &production_view,
        &production_complement,
        pair.protocols(),
    )
    .expect_err("production must reject a foreign table complement");
    let reference = pair.with_reference(|lens, _| {
        RefTableLens::put(lens, &reference_view, &reference_complement)
            .expect_err("the reference must reject a foreign table complement")
    });
    let LensError::Identity(production) = production else {
        panic!("production returned a non-identity refusal: {production:?}");
    };
    let TablePutError::Identity(reference) = reference else {
        panic!("reference returned a non-identity refusal: {reference:?}");
    };
    for reason in [
        production_square_identity_reason(&production),
        reference_table_square_identity_reason(&reference),
    ] {
        assert!(matches!(
            reason,
            SquareIdentityReason::Mismatch {
                ref field,
                ref expected,
                ref found,
            } if field == "lens" && expected != found
        ));
    }
}

fn assert_table_put_id_collision_refusal() {
    let construction = named_table_construction("dropped table row");
    let pair = TablePair::new(
        construction.source_schema,
        construction.target_schema,
        construction.migration,
        WRITER_VALUE_KIND,
        ScalarKind::Any,
    );
    let mut view = reference_table_get(&pair, &construction.source)
        .expect("the id-collision fixture gets")
        .0;
    view.rows.insert(0, ("a".to_owned(), BTreeMap::new()));
    let production = production_table_put(&pair, &construction.source, &view);
    let reference = reference_table_put(&pair, &construction.source, &view);
    let expected = TablePutOutcome {
        put: Err(Reason::PutDomain("IdCollision Row(0)".to_owned())),
        domain: Err(vec!["IdCollision Row(0)".to_owned()]),
    };
    assert_eq!(production, reference);
    assert_eq!(reference, expected);
}

fn assert_table_put_ambiguous_anchor_refusal() {
    let construction = named_table_construction("merged table origins");
    let pair = TablePair::new(
        construction.source_schema,
        construction.target_schema,
        construction.migration,
        WRITER_VALUE_KIND,
        ScalarKind::Any,
    );
    let mut view = reference_table_get(&pair, &construction.source)
        .expect("the ambiguous-anchor fixture gets")
        .0;
    view.rows.insert(2, ("t".to_owned(), BTreeMap::new()));
    let production = production_table_put(&pair, &construction.source, &view);
    let reference = reference_table_put(&pair, &construction.source, &view);
    let expected_reason = "AmbiguousReverseAnchor Row(2) [\"a\", \"b\"]".to_owned();
    let expected = TablePutOutcome {
        put: Err(Reason::PutDomain(expected_reason.clone())),
        domain: Err(vec![expected_reason]),
    };
    assert_eq!(production, reference);
    assert_eq!(reference, expected);
}

fn assert_table_put_noninvertible_transform_refusal() {
    let fixture = WriterFixture {
        images: [Some("x"), None],
        values: [-1, 0],
    };
    let mut migration = fixture.migration();
    migration.transformed.insert("t".to_owned());
    let mut structural = migration.clone();
    structural.transformed.clear();
    let mut pair = TablePair::new(
        fixture.source_schema(),
        fixture.target_schema(),
        structural,
        WRITER_VALUE_KIND,
        ScalarKind::Int {
            bits: 64,
            signed: true,
        },
    );
    pair.migration = migration;
    pair.plan = panproto_reference::table::compile_for_tables(
        &pair.src,
        &pair.tgt,
        &pair.boundaries.0,
        &pair.boundaries.1,
        &pair.migration,
    );
    pair.layer = &NonInvertibleTableTransform;
    pair.production.compiled.field_transforms.insert(
        "t".into(),
        vec![panproto_inst::FieldTransform::ApplyExpr {
            key: "x".to_owned(),
            expr: panproto_expr::Expr::Builtin(
                panproto_expr::BuiltinOp::Abs,
                vec![panproto_expr::Expr::Var("x".into())],
            ),
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Projection,
        }],
    );
    let source = fixture.source();
    let mut view = pair.with_reference(|lens, _| {
        lens.get(&source)
            .expect("the noninvertible-transform fixture gets")
            .0
    });
    view.rows
        .get_mut(&0)
        .expect("the fixture has row zero")
        .1
        .insert("x".to_owned(), RefValue::Int(2));
    let production = production_table_put(&pair, &source, &view);
    let reference = reference_table_put(&pair, &source, &view);
    let expected = TablePutOutcome {
        put: Err(Reason::PutDomain(
            "TransformNotInvertible Row(0) x".to_owned(),
        )),
        domain: Err(vec!["TransformNotInvertible Row(0) x".to_owned()]),
    };
    assert_eq!(production, reference);
    assert_eq!(reference, expected);
}

fn assert_table_put_refusal_case(case: TablePutRefusalCase) {
    match case {
        TablePutRefusalCase::Identity => assert_table_put_identity_refusal(),
        TablePutRefusalCase::IdCollision => assert_table_put_id_collision_refusal(),
        TablePutRefusalCase::AmbiguousReverseAnchor => {
            assert_table_put_ambiguous_anchor_refusal();
        }
        TablePutRefusalCase::TransformNotInvertible => {
            assert_table_put_noninvertible_transform_refusal();
        }
    }
}

#[test]
fn table_put_refusals_are_typed_and_complete() {
    assert_eq!(
        TablePutRefusalCase::ALL.len(),
        table_put_refusal_count_certificate()
    );
    for case in TablePutRefusalCase::ALL {
        assert_table_put_refusal_case(case);
    }
}

#[test]
fn table_complement_law_certificate() {
    let mut cases = 0;
    let mut fixtures = 0;
    let mut constructions = 0;
    let mut refusals = 0;
    for_each_table_complement_law_case(|case| {
        cases += 1;
        match case {
            TableComplementLawCase::Fixture(_) => fixtures += 1,
            TableComplementLawCase::Construction(_) => constructions += 1,
            TableComplementLawCase::Refusal(_) => refusals += 1,
        }
    });
    assert_eq!(cases, table_complement_law_count_certificate());
    assert_eq!(fixtures, table_fixture_count_certificate().total);
    assert_eq!(
        constructions,
        table_complement_construction_count_certificate()
    );
    assert_eq!(refusals, table_get_refusal_count_certificate());
    assert_eq!(cases, 9_937);
    table_get_and_put_agree_with_the_reference_on_the_fixture_scope();
    table_complement_certificate_is_exact();
    table_get_refusals_are_typed_and_complete();
}

#[test]
fn table_get_put_law_certificate() {
    let mut cases = 0;
    let mut fixtures = 0;
    let mut writers = 0;
    let mut derived = 0;
    let mut pairs = 0;
    let mut refusals = 0;
    for_each_table_get_put_law_case(|case| {
        cases += 1;
        match case {
            TableGetPutLawCase::Fixture(_) => fixtures += 1,
            TableGetPutLawCase::Writer(_) => writers += 1,
            TableGetPutLawCase::Derived(_) => derived += 1,
            TableGetPutLawCase::Pair(_) => pairs += 1,
            TableGetPutLawCase::Refusal(_) => refusals += 1,
        }
    });
    assert_eq!(cases, table_get_put_law_count_certificate());
    assert_eq!(fixtures, table_fixture_get_put_count_certificate());
    assert_eq!(writers, writer_count_certificate().2);
    assert_eq!(derived, derived_writer_count_certificate());
    assert_eq!(pairs, table_pair_put_count_certificate());
    assert_eq!(refusals, table_put_refusal_count_certificate());
    assert_eq!(cases, 156_704);
    table_get_and_put_agree_with_the_reference_on_the_fixture_scope();
    table_writers_agree_with_the_reference_on_the_writer_carrier();
    target_only_columns_refuse_with_exact_derived_cell_errors();
    table_pair_puts_agree_with_the_reference();
    table_put_refusals_are_typed_and_complete();
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
    c11_graph_state_count, c11_graph_states, c11_path_count_certificate, c11_stages,
    c11_table_states, c11_tree_square_count_certificate, sigma2_count, sigma2_schemas,
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
    Get(Box<GetOutcome>),
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
        |pairs: &Vec<Pair>, case: &GraphCase<'_>| match case {
            GraphCase::Get(stage, source) => {
                GraphCaseOutcome::Get(Box::new(production_get(&pairs[*stage], source)))
            }
            GraphCase::Put(stage, source, view) => {
                GraphCaseOutcome::Put(production_put(&pairs[*stage], source, view))
            }
        },
        |pairs: &Vec<Pair>, case: &GraphCase<'_>| match case {
            GraphCase::Get(stage, source) => {
                GraphCaseOutcome::Get(Box::new(reference_get(&pairs[*stage], source)))
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
    contraction_choices: BTreeMap<(u32, u32), RefSEdge>,
    arc_order: Vec<(u32, u32)>,
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
        contraction_choices: complement
            .contraction_choices
            .iter()
            .map(|(pair, edge)| (*pair, ref_edge(edge)))
            .collect(),
        arc_order: complement.arc_order.clone(),
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

fn reference_tree_residue(
    source: &RefTree,
    view: &RefTree,
    complement: &RefComplement,
) -> TreeResidue {
    let is_fan = |kind: &str| panproto_reference::restrict::is_fan_kind(kind);
    let mut dropped_arcs: Vec<(u32, u32, RefSEdge)> = complement
        .dropped_edges
        .values()
        .filter(|edge| !is_fan(&edge.label.kind))
        .map(|edge| (edge.src, edge.tgt, edge.label.clone()))
        .collect();
    dropped_arcs.sort();
    let source_pairs: std::collections::BTreeSet<(u32, u32)> = source
        .arcs
        .iter()
        .map(|(parent, child, _)| (*parent, *child))
        .collect();
    TreeResidue {
        dropped_nodes: complement
            .dropped_nodes
            .iter()
            .filter(|(_, node)| node.shape != RefNodeShape::HyperEdge)
            .map(|(id, node)| (*id, node.clone()))
            .collect(),
        dropped_arcs,
        contraction_choices: view
            .arcs
            .iter()
            .filter(|(parent, child, _)| !source_pairs.contains(&(*parent, *child)))
            .map(|(parent, child, edge)| ((*parent, *child), edge.clone()))
            .collect(),
        arc_order: source
            .arcs
            .iter()
            .map(|(parent, child, _)| (*parent, *child))
            .collect(),
        original_anchors: complement.original_anchors.clone(),
        original_fields: complement.original_fields.clone(),
        original_values: complement.original_values.clone(),
        synthesized: complement.synthesized_nodes.clone(),
        floor: complement.floor.0,
    }
}

/// Tree get on both sides, with `GetPut`: the view, the mapped residue and
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
                assert_eq!(
                    complement.identity.as_ref(),
                    Some(&pair.production.identity(ComplementShape::WType)),
                    "tree get binds the exact production WType identity"
                );
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
                    assert_eq!(
                        complement.identity,
                        Some(panproto_reference::lens::graph_lens_identity(
                            &pair.src,
                            &pair.tgt,
                            &pair.migration,
                            RefComplementShape::WType,
                        )),
                        "tree get binds the exact reference WType identity"
                    );
                    let restored =
                        panproto_reference::lens::tree_put(lens, &view, &embedding, &complement)
                            .map_err(|error| ref_lens_reason(&error));
                    (
                        view.clone(),
                        reference_tree_residue(source, &view, &complement),
                        restored,
                    )
                })
                .map_err(|error| Reason::Restrict(ref_restrict_reason(&error))),
        )
    })
}

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

#[test]
fn consecutive_c11_value_layers_remain_separate_normal_groups() {
    let stages = c11_stages();
    let transform = c11_stage_index(&stages, "T");
    let path = production_word(&stages, &[transform, transform]).expect("T then T composes");
    let ranges = path
        .tree_normal_form()
        .into_iter()
        .map(|group| group.stages)
        .collect::<Vec<_>>();
    assert_eq!(ranges, [0..1, 1..2]);
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
            other @ PathStageError::IdentityMismatch { .. } => {
                (Reason::Identity(format!("{other:?}")), at.stages.clone())
            }
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

#[derive(Debug)]
struct ShapePathOutcome<I> {
    get: Result<I, PathRefusal>,
    puts: Vec<Result<I, PathRefusal>>,
}

fn shape_path_outcomes_agree<I: PartialEq>(
    production: &ShapePathOutcome<I>,
    reference: &ShapePathOutcome<I>,
) -> bool {
    let same = |a: &Result<I, PathRefusal>, b: &Result<I, PathRefusal>| match (a, b) {
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
            .all(|(production, reference)| same(production, reference))
}

fn reference_tree_law_reason(error: &panproto_reference::lens::ReferenceLawError) -> Reason {
    use panproto_reference::lens::ReferenceLawError as Error;
    match error {
        Error::Restrict(error) => Reason::Restrict(ref_restrict_reason(error)),
        Error::Identity(error) => Reason::Identity(format!("{error:?}")),
        Error::PutDomain(errors) => errors.first().map_or_else(
            || Reason::PutDomain("empty put-domain report".to_owned()),
            |error| Reason::PutDomain(ref_put_domain_reason(error)),
        ),
        Error::TreeOperational(error) => Reason::Restrict(format!("{error:?}")),
        Error::OutsideTreeDomain => Reason::Restrict("OutsideTreeDomain".to_owned()),
    }
}

fn ref_tree_view_ref(
    elem: &panproto_reference::lens::ViewRef,
    view: &RefTree,
    complement: &panproto_reference::lens::TreeLawComplement,
) -> String {
    use panproto_reference::lens::ViewRef as R;

    let R::Edge(edge) = elem else {
        return ref_view_ref(elem);
    };
    let durable_child = complement
        .embedding
        .arcs
        .iter()
        .find_map(|(child, candidate)| (*candidate == *edge).then_some(*child));
    let replayed_arc = durable_child
        .and_then(|child| {
            view.arcs
                .iter()
                .find(|(_, candidate, _)| *candidate == child)
                .map(|(parent, child, _)| (*parent, *child))
        })
        .or_else(|| ref_tree_fresh_arc(*edge, view, complement));
    let Some((view_parent, child)) = replayed_arc.or_else(|| {
        durable_child.map(|child| {
            let parent = view
                .arcs
                .iter()
                .find_map(|(parent, candidate, _)| (*candidate == child).then_some(*parent))
                .unwrap_or(child);
            (parent, child)
        })
    }) else {
        return ref_view_ref(elem);
    };
    let synthesized_parent = complement
        .complement
        .synthesized_edges
        .get(edge)
        .and_then(|key| match key {
            RefSynthKey::Contract { src, .. } | RefSynthKey::ContractExpand { src, hop: 0, .. } => {
                Some(*src)
            }
            RefSynthKey::Expand {
                source_edge,
                hop: 0,
            } => complement
                .source_arcs
                .get(source_edge)
                .map(|(parent, _)| *parent),
            RefSynthKey::Expand { source_edge, hop } => {
                let predecessor = RefSynthKey::Expand {
                    source_edge: *source_edge,
                    hop: *hop - 1,
                };
                complement
                    .complement
                    .synthesized_nodes
                    .iter()
                    .find_map(|(node, key)| (key == &predecessor).then_some(*node))
            }
            RefSynthKey::ContractExpand { src, tgt, hop } => {
                let predecessor = RefSynthKey::ContractExpand {
                    src: *src,
                    tgt: *tgt,
                    hop: *hop - 1,
                };
                complement
                    .complement
                    .synthesized_nodes
                    .iter()
                    .find_map(|(node, key)| (key == &predecessor).then_some(*node))
            }
        });
    let parent = synthesized_parent.unwrap_or(view_parent);
    format!("Arc({parent},{child})")
}

fn ref_tree_fresh_arc(
    wanted: u32,
    view: &RefTree,
    complement: &panproto_reference::lens::TreeLawComplement,
) -> Option<(u32, u32)> {
    let graph = &complement.complement;
    let existing = complement
        .embedding
        .arcs
        .values()
        .copied()
        .chain(graph.dropped_edges.keys().copied())
        .chain(graph.synthesized_edges.keys().copied())
        .chain(
            complement
                .embedding
                .fans
                .iter()
                .flat_map(|(_, parent, children)| {
                    std::iter::once(*parent).chain(children.values().copied())
                }),
        )
        .max();
    let reserved = graph
        .floor
        .1
        .max(complement.embedding.next_edge)
        .checked_sub(1);
    let mut next = existing
        .into_iter()
        .chain(reserved)
        .max()
        .map_or(Some(0), |id| id.checked_add(1))
        .filter(|id| *id != u32::MAX);
    let mut used = BTreeSet::new();
    for (parent, child, _) in &view.arcs {
        let id = complement
            .embedding
            .arcs
            .get(child)
            .copied()
            .filter(|id| used.insert(*id))
            .or_else(|| {
                let id = next?;
                next = id.checked_add(1).filter(|next| *next != u32::MAX);
                used.insert(id);
                Some(id)
            })?;
        if id == wanted {
            return Some((*parent, *child));
        }
    }
    None
}

#[test]
fn tree_refusal_coordinates_recover_every_missing_synthesized_hop() {
    use panproto_reference::lens::ViewRef as R;

    let mut view = RefTree {
        nodes: BTreeMap::new(),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 0,
    };
    let mut complement = panproto_reference::lens::TreeLawComplement::default();
    complement.source_arcs.insert(3, (10, 11));
    complement.complement.synthesized_nodes.insert(
        20,
        RefSynthKey::Expand {
            source_edge: 3,
            hop: 0,
        },
    );
    complement.complement.synthesized_nodes.insert(
        30,
        RefSynthKey::ContractExpand {
            src: 5,
            tgt: 9,
            hop: 0,
        },
    );
    let synthesized = [
        (
            40,
            20,
            RefSynthKey::Expand {
                source_edge: 3,
                hop: 0,
            },
        ),
        (
            41,
            11,
            RefSynthKey::Expand {
                source_edge: 3,
                hop: 1,
            },
        ),
        (
            42,
            30,
            RefSynthKey::ContractExpand {
                src: 5,
                tgt: 9,
                hop: 0,
            },
        ),
        (
            43,
            9,
            RefSynthKey::ContractExpand {
                src: 5,
                tgt: 9,
                hop: 1,
            },
        ),
        (
            44,
            8,
            RefSynthKey::Contract {
                src: 7,
                tgt: 8,
                edge: RefSEdge::new("a", "b", "e", None),
            },
        ),
    ];
    for (edge, child, key) in synthesized {
        complement.embedding.arcs.insert(child, edge);
        complement.complement.synthesized_edges.insert(edge, key);
    }

    for (edge, expected) in [
        (40, "Arc(10,20)"),
        (41, "Arc(20,11)"),
        (42, "Arc(5,30)"),
        (43, "Arc(30,9)"),
        (44, "Arc(7,8)"),
    ] {
        assert_eq!(
            ref_tree_view_ref(&R::Edge(edge), &view, &complement),
            expected,
            "synthesized edge {edge} must retain its tree carrier coordinate after removal"
        );
    }

    view.arcs.push((
        99,
        20,
        RefSEdge::new("outside", "outside", "reparented", None),
    ));
    assert_eq!(
        ref_tree_view_ref(&R::Edge(40), &view, &complement),
        "Arc(10,20)",
        "a reparented synthesized child still names the original synthesized arc"
    );

    assert_fresh_tree_arc_uses_replayed_carrier_allocation();
}

fn assert_fresh_tree_arc_uses_replayed_carrier_allocation() {
    use panproto_reference::lens::ViewRef as R;

    let fresh_view = RefTree {
        nodes: BTreeMap::from([(0, RefNode::new(0, "a")), (1, RefNode::new(1, "a"))]),
        arcs: vec![(0, 1, RefSEdge::new("a", "a", "e", None))],
        fans: Vec::new(),
        root: 0,
    };
    assert_eq!(
        ref_tree_view_ref(
            &R::Edge(0),
            &fresh_view,
            &panproto_reference::lens::TreeLawComplement::default(),
        ),
        "Arc(0,1)",
        "a newly inserted tree arc is named through replayed carrier allocation"
    );
}

fn reference_tree_put_law_reason(
    error: &panproto_reference::lens::ReferenceLawError,
    view: &RefTree,
    complement: &panproto_reference::lens::TreeLawComplement,
) -> Reason {
    match error {
        panproto_reference::lens::ReferenceLawError::PutDomain(errors) => {
            errors.first().map_or_else(
                || Reason::PutDomain("empty put-domain report".to_owned()),
                |error| {
                    Reason::PutDomain(ref_put_domain_reason_with(error, &|elem| {
                        ref_tree_view_ref(elem, view, complement)
                    }))
                },
            )
        }
        _ => reference_tree_law_reason(error),
    }
}

fn reference_table_law_reason(error: &panproto_reference::lens::TableLawError) -> Reason {
    use panproto_reference::lens::TableLawError as Error;
    match error {
        Error::Restrict(error) => Reason::Restrict(ref_restrict_reason(error)),
        Error::Put(error) => ref_table_put_reason(error),
        Error::Domain(TableCheckError::Domain { violations, .. }) => {
            violations.first().map_or_else(
                || Reason::PutDomain("empty put-domain report".to_owned()),
                |error| Reason::PutDomain(ref_put_domain_reason(error)),
            )
        }
        Error::Domain(error) => Reason::Restrict(format!("reference table check {error:?}")),
    }
}

fn reference_tree_path_outcome(
    pairs: &[Pair],
    word: &[usize],
    source: &RefTree,
    edits: &[RefTree],
) -> ShapePathOutcome<RefTree> {
    let mut view = source.clone();
    let mut complements = Vec::with_capacity(word.len());
    for (at, stage) in word.iter().copied().enumerate() {
        let get = pairs[stage].with_reference(|lens| {
            let lens = lens.map_err(Reason::Restrict)?;
            RefTreeShape::get(lens, &view, &()).map_err(|error| reference_tree_law_reason(&error))
        });
        match get {
            Ok((next, complement)) => {
                view = next;
                complements.push(complement);
            }
            Err(reason) => {
                return ShapePathOutcome {
                    get: Err((reason, at..at + 1)),
                    puts: Vec::new(),
                };
            }
        }
    }
    let puts = std::iter::once(&view)
        .chain(edits)
        .map(|edited| {
            let mut restored = edited.clone();
            for at in (0..word.len()).rev() {
                let stage = word[at];
                let put = pairs[stage].with_reference(|lens| {
                    let lens = lens.map_err(Reason::Restrict)?;
                    RefTreeShape::put(lens, &restored, &complements[at], &()).map_err(|error| {
                        reference_tree_put_law_reason(&error, &restored, &complements[at])
                    })
                });
                match put {
                    Ok(next) => restored = next,
                    Err(reason) => return Err((reason, at..at + 1)),
                }
            }
            Ok(restored)
        })
        .collect();
    ShapePathOutcome {
        get: Ok(view),
        puts,
    }
}

fn production_tree_path_outcome(
    path: &LensPath,
    source: &RefTree,
    edits: &[RefTree],
) -> ShapePathOutcome<RefTree> {
    match path.get(&prod_tree(source)) {
        Err(error) => ShapePathOutcome {
            get: Err(production_path_refusal(&error)),
            puts: Vec::new(),
        },
        Ok((view, complement)) => {
            let unedited = ref_tree(&view);
            let puts = std::iter::once(&unedited)
                .chain(edits)
                .map(|edited| {
                    path.put(&prod_tree(edited), &complement)
                        .map(|restored| ref_tree(&restored))
                        .map_err(|error| production_path_refusal(&error))
                })
                .collect();
            ShapePathOutcome {
                get: Ok(unedited),
                puts,
            }
        }
    }
}

/// The production tree fold without normal-form fusion. This supplies an
/// independent production-side comparison on sources that the reference
/// tree semantics deliberately excludes from its common graph/tree domain.
fn production_atomic_tree_path_outcome(
    pairs: &[Pair],
    word: &[usize],
    source: &RefTree,
    edits: &[RefTree],
) -> ShapePathOutcome<RefTree> {
    let mut view = prod_tree(source);
    let mut complements = Vec::with_capacity(word.len());
    for (at, stage) in word.iter().copied().enumerate() {
        match panproto_lens::get(&pairs[stage].production, &view) {
            Ok((next, complement)) => {
                view = next;
                complements.push(complement);
            }
            Err(error) => {
                return ShapePathOutcome {
                    get: Err((lens_reason(&error), at..at + 1)),
                    puts: Vec::new(),
                };
            }
        }
    }
    let unedited = ref_tree(&view);
    let puts = std::iter::once(&unedited)
        .chain(edits)
        .map(|edited| {
            let mut restored = prod_tree(edited);
            for at in (0..word.len()).rev() {
                let stage = word[at];
                match panproto_lens::put(&pairs[stage].production, &restored, &complements[at]) {
                    Ok(next) => restored = next,
                    Err(error) => return Err((lens_reason(&error), at..at + 1)),
                }
            }
            Ok(ref_tree(&restored))
        })
        .collect();
    ShapePathOutcome {
        get: Ok(unedited),
        puts,
    }
}

fn reference_table_path_outcome(
    pairs: &[TablePair],
    word: &[usize],
    source: &RefTable,
    edits: &[RefTable],
) -> ShapePathOutcome<RefTable> {
    let mut view = source.clone();
    let mut complements = Vec::with_capacity(word.len());
    for (at, stage) in word.iter().copied().enumerate() {
        let get = pairs[stage]
            .try_with_reference(|lens, _| {
                reference_table_get_preflight(&view, &pairs[stage].boundaries.0)
                    .map_err(|error| Reason::Restrict(ref_nonconforming_reason(&error, &view)))?;
                RefTableShape::get(lens, &view, &())
                    .map_err(|error| reference_table_law_reason(&error))
            })
            .and_then(|result| result);
        match get {
            Ok((next, complement)) => {
                view = next;
                complements.push(complement);
            }
            Err(reason) => {
                return ShapePathOutcome {
                    get: Err((reason, at..at + 1)),
                    puts: Vec::new(),
                };
            }
        }
    }
    let puts = std::iter::once(&view)
        .chain(edits)
        .map(|edited| {
            let mut restored = edited.clone();
            for at in (0..word.len()).rev() {
                let stage = word[at];
                let put = pairs[stage]
                    .try_with_reference(|lens, _| {
                        RefTableShape::put(lens, &restored, &complements[at], &())
                            .map_err(|error| reference_table_law_reason(&error))
                    })
                    .and_then(|result| result);
                match put {
                    Ok(next) => restored = next,
                    Err(reason) => return Err((reason, at..at + 1)),
                }
            }
            Ok(restored)
        })
        .collect();
    ShapePathOutcome {
        get: Ok(view),
        puts,
    }
}

fn production_table_path_outcome(
    path: &LensPath,
    protocol: &Protocol,
    source: &RefTable,
    edits: &[RefTable],
) -> ShapePathOutcome<RefTable> {
    let protocols = vec![protocol.clone(); path.len() + 1];
    match path.get_table(&prod_table(source), &protocols) {
        Err(error) => ShapePathOutcome {
            get: Err(production_path_refusal(&error)),
            puts: Vec::new(),
        },
        Ok((view, complement)) => {
            let unedited = ref_table(&view);
            let puts = std::iter::once(&unedited)
                .chain(edits)
                .map(|edited| {
                    path.put_table(&prod_table(edited), &complement, &protocols)
                        .map(|restored| ref_table(&restored))
                        .map_err(|error| production_path_refusal(&error))
                })
                .collect();
            ShapePathOutcome {
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
        (
            8,
            "operational identity domain",
            c11_identity_index(&stages, &stages[named("D")].source),
            named("D"),
            false,
        ),
        (
            9,
            "fan identity residue",
            named("Id_h"),
            named("Id_h"),
            false,
        ),
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

fn path_context(stages: &[C11Stage], words: &[Vec<usize>]) -> PathContext {
    let mut composites = Vec::new();
    for word in words {
        let production = production_word(stages, word).unwrap();
        let reference = reference_word(stages, word).unwrap();
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
        composites.push((word.clone(), production, reference));
        composites.push((word.clone(), left.0, left.1));
        composites.push((word.clone(), right.0, right.1));
    }
    (stages.iter().map(Pair::from_stage).collect(), composites)
}

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
    let composites = || path_context(&stages, &composable_words);
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
    assert_eq!(units.len(), 1_855_809);
    println!("{bound}");
    let edits = std::sync::atomic::AtomicUsize::new(0);
    let edit_families = AtomicClassifiedEditCensus::default();
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &units,
        composites,
        |(pairs, composites), (index, state)| {
            let word = composites[*index].0.clone();
            let source = &sources[word[0]][*state];
            let (final_edits, census) = edits_of_reference_view(pairs, &word, source);
            edits.fetch_add(final_edits.len(), std::sync::atomic::Ordering::Relaxed);
            edit_families.add(&census);
            vec![PathCase {
                index: *index,
                source: source.clone(),
                edits: final_edits,
            }]
        },
        |(_, composites): &PathContext, case: &PathCase| {
            let (_, path, _) = &composites[case.index];
            production_path_outcome(path, &case.source, &case.edits)
        },
        |(pairs, composites): &PathContext, case: &PathCase| {
            let (_, _, reference) = &composites[case.index];
            reference_path_outcome(pairs, reference, &case.source, &case.edits)
        },
        path_outcomes_agree,
    ));
    let edit_families = edit_families.into_inner();
    let edits = edits.into_inner();
    assert_eq!(edit_families.total(), edits);
    println!(
        "C11 paths final SSB: edits={edits}, admitted families={:?}, refused families={:?}",
        edit_families.admitted.named(),
        edit_families.refused.named()
    );
    assert_eq!(compared, u64::try_from(units.len()).unwrap());
}

fn composable_c11_words(stages: &[C11Stage]) -> Vec<Vec<usize>> {
    words(stages.len())
        .into_iter()
        .filter(|word| {
            word.windows(2)
                .all(|pair| stages[pair[0]].target == stages[pair[1]].source)
        })
        .collect()
}

const EDIT_FAMILIES: usize = 17;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct EditCensus {
    families: [usize; EDIT_FAMILIES],
}

impl EditCensus {
    const fn add(&mut self, mutation: RefMutation, count: usize) {
        self.families[ref_mutation_index(mutation)] += count;
    }

    fn total(&self) -> usize {
        self.families.iter().sum()
    }

    fn named(&self) -> Vec<(RefMutation, usize)> {
        REF_MUTATIONS
            .into_iter()
            .zip(self.families)
            .filter(|(_, count)| *count != 0)
            .collect()
    }
}

struct AtomicEditCensus {
    families: [std::sync::atomic::AtomicUsize; EDIT_FAMILIES],
}

impl Default for AtomicEditCensus {
    fn default() -> Self {
        Self {
            families: std::array::from_fn(|_| std::sync::atomic::AtomicUsize::new(0)),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ClassifiedEditCensus {
    admitted: EditCensus,
    refused: EditCensus,
}

impl ClassifiedEditCensus {
    const fn add(&mut self, mutation: RefMutation, admitted: bool) {
        if admitted {
            self.admitted.add(mutation, 1);
        } else {
            self.refused.add(mutation, 1);
        }
    }

    fn total(&self) -> usize {
        self.admitted.total() + self.refused.total()
    }
}

#[derive(Default)]
struct AtomicClassifiedEditCensus {
    admitted: AtomicEditCensus,
    refused: AtomicEditCensus,
}

impl AtomicClassifiedEditCensus {
    fn add(&self, census: &ClassifiedEditCensus) {
        self.admitted.add(&census.admitted);
        self.refused.add(&census.refused);
    }

    fn into_inner(self) -> ClassifiedEditCensus {
        ClassifiedEditCensus {
            admitted: self.admitted.into_inner(),
            refused: self.refused.into_inner(),
        }
    }
}

impl AtomicEditCensus {
    fn add(&self, census: &EditCensus) {
        for (target, count) in self.families.iter().zip(census.families) {
            target.fetch_add(count, std::sync::atomic::Ordering::Relaxed);
        }
    }

    fn into_inner(self) -> EditCensus {
        EditCensus {
            families: self
                .families
                .map(std::sync::atomic::AtomicUsize::into_inner),
        }
    }
}

const REF_MUTATIONS: [RefMutation; EDIT_FAMILIES] = [
    RefMutation::ValueEdit,
    RefMutation::FieldEdit,
    RefMutation::AddNode,
    RefMutation::RemoveNode,
    RefMutation::RelabelNode,
    RefMutation::AddEdge,
    RefMutation::RemoveEdge,
    RefMutation::Reconnect,
    RefMutation::RelabelEdge,
    RefMutation::Reorder,
    RefMutation::AddParallelEdge,
    RefMutation::AddSelfLoop,
    RefMutation::RemoveSynthesized,
    RefMutation::EditSynthesized,
    RefMutation::AddContractedEdge,
    RefMutation::ReservedIdReuse,
    RefMutation::MalformedFan,
];

const fn ref_mutation_index(mutation: RefMutation) -> usize {
    match mutation {
        RefMutation::ValueEdit => 0,
        RefMutation::FieldEdit => 1,
        RefMutation::AddNode => 2,
        RefMutation::RemoveNode => 3,
        RefMutation::RelabelNode => 4,
        RefMutation::AddEdge => 5,
        RefMutation::RemoveEdge => 6,
        RefMutation::Reconnect => 7,
        RefMutation::RelabelEdge => 8,
        RefMutation::Reorder => 9,
        RefMutation::AddParallelEdge => 10,
        RefMutation::AddSelfLoop => 11,
        RefMutation::RemoveSynthesized => 12,
        RefMutation::EditSynthesized => 13,
        RefMutation::AddContractedEdge => 14,
        RefMutation::ReservedIdReuse => 15,
        RefMutation::MalformedFan => 16,
    }
}

fn direct_edge_image(pair: &Pair, edge: &RefSEdge) -> Option<RefSEdge> {
    if !(pair.migration.survives(&edge.src) && pair.migration.survives(&edge.tgt)) {
        return None;
    }
    if let Some(image) = pair.migration.edge_remap.get(edge) {
        return Some(image.clone());
    }
    let src = pair.migration.vertex_image(&edge.src);
    let tgt = pair.migration.vertex_image(&edge.tgt);
    pair.tgt
        .edges
        .iter()
        .find(|candidate| {
            candidate.src == src
                && candidate.tgt == tgt
                && candidate.kind == edge.kind
                && candidate.name == edge.name
        })
        .cloned()
        .or_else(|| {
            pair.migration
                .resolver
                .get(&(src.to_owned(), tgt.to_owned()))
                .cloned()
        })
}

fn uniquely_representable_anchor(pair: &Pair, anchor: &str) -> bool {
    pair.src
        .vertices
        .keys()
        .filter(|source| {
            pair.migration.survives(source)
                && pair.migration.vertex_image(source.as_str()) == anchor
        })
        .count()
        == 1
}

fn uniquely_representable_edge(pair: &Pair, edge: &RefSEdge) -> bool {
    pair.src
        .edges
        .iter()
        .filter(|source| direct_edge_image(pair, source).as_ref() == Some(edge))
        .count()
        == 1
}

fn tree_arc_is_synthesized(
    child: u32,
    complement: &panproto_reference::lens::TreeLawComplement,
) -> bool {
    complement
        .embedding
        .arcs
        .get(&child)
        .is_some_and(|edge| complement.complement.synthesized_edges.contains_key(edge))
}

fn tree_node_touches_synthesized_arc(
    view: &RefTree,
    complement: &panproto_reference::lens::TreeLawComplement,
    node: u32,
) -> bool {
    view.arcs.iter().any(|(parent, child, _)| {
        (*parent == node || *child == node) && tree_arc_is_synthesized(*child, complement)
    })
}

fn tree_is_descendant(view: &RefTree, ancestor: u32, candidate: u32) -> bool {
    let mut frontier = vec![ancestor];
    while let Some(parent) = frontier.pop() {
        for (_, child, _) in view.arcs.iter().filter(|(from, _, _)| *from == parent) {
            if *child == candidate {
                return true;
            }
            frontier.push(*child);
        }
    }
    false
}

fn add_tree_node_edits(
    pair: &Pair,
    view: &RefTree,
    complement: &panproto_reference::lens::TreeLawComplement,
) -> EditCensus {
    let graph = &complement.complement;
    let ordinary = |id: &u32, node: &RefNode| {
        !graph.synthesized_nodes.contains_key(id) && node.shape != RefNodeShape::HyperEdge
    };
    let mut census = EditCensus::default();
    for (id, node) in view.nodes.iter().filter(|(id, node)| ordinary(id, node)) {
        census.add(RefMutation::ValueEdit, 1);
        for (field, value) in &node.fields {
            let mut candidate = view.clone();
            candidate
                .nodes
                .get_mut(id)
                .expect("the edited node comes from the view")
                .fields
                .insert(
                    field.clone(),
                    match value {
                        RefValue::Bool(value) => RefValue::Bool(!value),
                        _ => RefValue::Bool(false),
                    },
                );
            let admitted = pair.with_reference(|lens| {
                lens.is_ok_and(|lens| {
                    RefTreeShape::check_put_domain(lens, &candidate, complement, &()).is_ok()
                })
            });
            census.add(RefMutation::FieldEdit, usize::from(admitted));
        }

        let removable = *id != view.root
            && !view.arcs.iter().any(|(parent, _, _)| parent == id)
            && !tree_node_touches_synthesized_arc(view, complement, *id)
            && !view
                .fans
                .iter()
                .any(|fan| fan.parent == *id || fan.children.values().any(|child| child == id));
        if removable {
            census.add(RefMutation::RemoveNode, 1);
            census.add(RefMutation::RemoveEdge, 1);
        }

        if !tree_node_touches_synthesized_arc(view, complement, *id) {
            let relabels = pair
                .tgt
                .vertices
                .keys()
                .filter(|anchor| {
                    *anchor != &node.anchor
                        && uniquely_representable_anchor(pair, anchor)
                        && view.arcs.iter().all(|(parent, child, label)| {
                            (*parent != *id || label.src == anchor.as_str())
                                && (*child != *id || label.tgt == anchor.as_str())
                        })
                })
                .count();
            census.add(RefMutation::RelabelNode, relabels);
        }
    }
    census
}

/// A structural count of the admitted tree edit families. This deliberately
/// does not call the reference Single-Step Basis generator.
fn tree_edit_census(
    pair: &Pair,
    view: &RefTree,
    complement: &panproto_reference::lens::TreeLawComplement,
) -> EditCensus {
    let graph = &complement.complement;
    let mut census = add_tree_node_edits(pair, view, complement);

    let has_fresh_node = panproto_reference::restrict::floor_of(
        view.nodes.keys().chain(graph.dropped_nodes.keys()).copied(),
    ) != u32::MAX;
    if has_fresh_node {
        let insertions: usize = pair
            .tgt
            .edges
            .iter()
            .filter(|edge| uniquely_representable_edge(pair, edge))
            .map(|edge| {
                view.nodes
                    .values()
                    .filter(|node| node.anchor == edge.src)
                    .count()
            })
            .sum();
        census.add(RefMutation::AddNode, insertions);
        census.add(RefMutation::AddEdge, insertions);
    }

    for (parent, child, label) in &view.arcs {
        if tree_arc_is_synthesized(*child, complement) {
            continue;
        }
        let reconnects = view
            .nodes
            .iter()
            .filter(|(next_parent, node)| {
                let distinct_endpoint = *next_parent != parent && *next_parent != child;
                let matching_source = node.anchor == label.src;
                let preserves_acyclicity = !tree_is_descendant(view, *child, **next_parent);
                distinct_endpoint && matching_source && preserves_acyclicity
            })
            .count();
        census.add(RefMutation::Reconnect, reconnects);
        census.add(
            RefMutation::RelabelEdge,
            pair.tgt
                .edges
                .iter()
                .filter(|replacement| {
                    *replacement != label
                        && replacement.src == label.src
                        && replacement.tgt == label.tgt
                        && uniquely_representable_edge(pair, replacement)
                })
                .count(),
        );
    }

    let mut by_parent: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (parent, child, _) in &view.arcs {
        by_parent.entry(*parent).or_default().push(*child);
    }
    let reorders = by_parent
        .values()
        .map(|children| {
            children
                .windows(2)
                .filter(|pair| {
                    !tree_arc_is_synthesized(pair[0], complement)
                        && !tree_arc_is_synthesized(pair[1], complement)
                })
                .count()
        })
        .sum();
    census.add(RefMutation::Reorder, reorders);
    census
}

fn table_edit_census(
    pair: &TablePair,
    view: &RefTable,
    complement: &RefTableComplement,
) -> ClassifiedEditCensus {
    let mut census = ClassifiedEditCensus::default();
    let admitted = |candidate: &RefTable| {
        pair.with_reference(|lens, _| {
            RefTableShape::check_put_domain(lens, candidate, complement, &()).is_ok()
        })
    };
    let mut record = |mutation, candidate: &RefTable| {
        census.add(mutation, admitted(candidate));
    };
    for (id, (table, cells)) in &view.rows {
        for column in pair
            .boundaries
            .1
            .columns
            .get(table)
            .into_iter()
            .flat_map(BTreeMap::keys)
        {
            let mut candidate = view.clone();
            let mut row = cells.clone();
            let alternate = match cells.get(column) {
                Some(RefValue::Bool(value)) => RefValue::Bool(!value),
                _ => RefValue::Bool(false),
            };
            row.insert(column.clone(), alternate);
            candidate.rows.insert(*id, (table.clone(), row));
            record(RefMutation::ValueEdit, &candidate);
        }
        if !pair
            .boundaries
            .1
            .columns
            .get(table)
            .is_some_and(|columns| columns.contains_key(C11_FIELD))
        {
            let mut candidate = view.clone();
            let mut row = cells.clone();
            if row.contains_key(C11_FIELD) {
                row.remove(C11_FIELD);
            } else {
                row.insert(C11_FIELD.to_owned(), C11_VALUE);
            }
            candidate.rows.insert(*id, (table.clone(), row));
            record(RefMutation::ValueEdit, &candidate);
        }
    }
    let has_fresh_row = panproto_reference::restrict::floor_of(
        view.rows
            .keys()
            .chain(complement.dropped_rows.keys())
            .copied(),
    ) != u32::MAX;
    if has_fresh_row {
        let fresh = panproto_reference::restrict::floor_of(
            view.rows
                .keys()
                .chain(complement.dropped_rows.keys())
                .copied(),
        );
        for table in pair.boundaries.1.columns.keys() {
            let mut candidate = view.clone();
            candidate
                .rows
                .insert(fresh, (table.clone(), BTreeMap::new()));
            record(RefMutation::AddNode, &candidate);
        }
    }
    for id in view.rows.keys() {
        let mut candidate = view.clone();
        candidate.rows.remove(id);
        candidate
            .pairs
            .retain(|(_, src, tgt)| src != id && tgt != id);
        record(RefMutation::RemoveNode, &candidate);
    }
    census
}

fn reference_tree_path_edits(
    pairs: &[Pair],
    word: &[usize],
    source: &RefTree,
) -> Option<(Vec<RefTree>, ClassifiedEditCensus)> {
    enum Step {
        Next(RefTree, Box<panproto_reference::lens::TreeLawComplement>),
        Outside,
        Refused,
    }

    let mut view = source.clone();
    let mut final_complement = None;
    let mut final_source = source.clone();
    for stage in word.iter().copied() {
        let stage_source = view.clone();
        let step = pairs[stage].with_reference(|lens| {
            let Ok(lens) = lens else {
                return Step::Refused;
            };
            match RefTreeShape::get(lens, &view, &()) {
                Ok((next, complement)) => Step::Next(next, Box::new(complement)),
                Err(panproto_reference::lens::ReferenceLawError::OutsideTreeDomain) => {
                    Step::Outside
                }
                Err(_) => Step::Refused,
            }
        });
        match step {
            Step::Next(next, complement) => {
                final_source = stage_source;
                view = next;
                final_complement = Some(*complement);
            }
            Step::Outside => return None,
            Step::Refused => {
                return Some((Vec::new(), ClassifiedEditCensus::default()));
            }
        }
    }
    let last = *word.last().expect("a path word is non-empty");
    let complement = final_complement.expect("a path word has a final complement");
    pairs[last].with_reference(|lens| {
        let Ok(lens) = lens else {
            return Some((Vec::new(), ClassifiedEditCensus::default()));
        };
        let basis =
            RefTreeShape::single_step_basis(lens, &final_source, &view, &complement, &());
        let mut basis_census = ClassifiedEditCensus::default();
        for case in &basis {
            let observed = RefTreeShape::check_put_domain(lens, &case.view, &complement, &());
            match (&case.domain, observed) {
                (DomainExpectation::Admitted, Ok(())) => {
                    basis_census.add(case.mutation, true);
                }
                (DomainExpectation::Refused(expected), Err(observed)) => {
                    assert_eq!(
                        &observed, expected,
                        "declared final-tree refusal for path {word:?}"
                    );
                    basis_census.add(case.mutation, false);
                }
                (expected, observed) => panic!(
                    "final-tree domain mismatch for path {word:?}: expected {expected:?}, observed {observed:?}"
                ),
            }
        }
        assert_eq!(
            basis_census.admitted,
            tree_edit_census(&pairs[last], &view, &complement),
            "independent admitted final-tree edit census for path {word:?}"
        );
        let edits = basis.into_iter().map(|case| case.view).collect();
        Some((edits, basis_census))
    })
}

fn reference_table_path_edits(
    pairs: &[TablePair],
    word: &[usize],
    source: &RefTable,
) -> (Vec<RefTable>, ClassifiedEditCensus) {
    let mut view = source.clone();
    let mut final_complement = None;
    let mut final_source = source.clone();
    for stage in word.iter().copied() {
        let stage_source = view.clone();
        let get = pairs[stage]
            .try_with_reference(|lens, _| {
                reference_table_get_preflight(&view, &pairs[stage].boundaries.0).ok()?;
                RefTableShape::get(lens, &view, &()).ok()
            })
            .ok()
            .flatten();
        let Some((next, complement)) = get else {
            return (Vec::new(), ClassifiedEditCensus::default());
        };
        final_source = stage_source;
        view = next;
        final_complement = Some(complement);
    }
    let last = *word.last().expect("a path word is non-empty");
    let complement = final_complement.expect("a path word has a final complement");
    pairs[last]
        .try_with_reference(|lens, _| {
            let basis =
                RefTableShape::single_step_basis(lens, &final_source, &view, &complement, &());
            let mut basis_census = ClassifiedEditCensus::default();
            for case in &basis {
                let observed = RefTableShape::check_put_domain(lens, &case.view, &complement, &());
                match (&case.domain, observed) {
                    (DomainExpectation::Admitted, Ok(())) => {
                        basis_census.add(case.mutation, true);
                    }
                    (DomainExpectation::Refused(expected), Err(observed)) => {
                        assert_eq!(
                            &observed, expected,
                            "declared final-table refusal for path {word:?}"
                        );
                        basis_census.add(case.mutation, false);
                    }
                    (expected, observed) => panic!(
                        "final-table domain mismatch for path {word:?}: expected {expected:?}, observed {observed:?}"
                    ),
                }
            }
            let mut edits: Vec<RefTable> = basis.into_iter().map(|case| case.view).collect();
            for (id, (table, cells)) in &view.rows {
                let mut candidate = view.clone();
                let mut row = cells.clone();
                if row.contains_key(C11_FIELD) {
                    row.remove(C11_FIELD);
                } else {
                    row.insert(C11_FIELD.to_owned(), C11_VALUE);
                }
                candidate.rows.insert(*id, (table.clone(), row));
                if edits.contains(&candidate) {
                    continue;
                }
                let admitted = RefTableShape::check_put_domain(
                    lens,
                    &candidate,
                    &complement,
                    &(),
                )
                .is_ok();
                basis_census.add(RefMutation::ValueEdit, admitted);
                edits.push(candidate);
            }
            assert_eq!(
                basis_census,
                table_edit_census(&pairs[last], &view, &complement),
                "independent final-table edit census for path {word:?}"
            );
            assert_eq!(edits.len(), basis_census.total());
            (edits, basis_census)
        })
        .unwrap_or_else(|_| (Vec::new(), ClassifiedEditCensus::default()))
}

#[derive(Clone, Debug)]
struct TreeFusionCase {
    word: Vec<usize>,
    source: RefTree,
    edits: Vec<RefTree>,
    inside: bool,
}

fn tree_path_normal_forms_match_independent_atomic_folds() {
    let stages = c11_stages();
    let certificate = c11_path_count_certificate();
    let composable = composable_c11_words(&stages);
    assert_eq!(composable.len(), certificate.composable());
    let mut units = Vec::new();
    let mut by_length = [0_usize; 3];
    for word in &composable {
        if stages[word[0]].source == c11_fan_schema() {
            continue;
        }
        for source in c11_tree_states(&stages[word[0]].source) {
            by_length[word.len() - 1] += 1;
            units.push((word.clone(), source));
        }
    }
    assert_eq!(by_length, certificate.tree_word_states_by_length);
    assert_eq!(units.len(), certificate.tree_word_states());
    assert_eq!(units.len(), 15_596);
    let edits = std::sync::atomic::AtomicUsize::new(0);
    let outside = std::sync::atomic::AtomicUsize::new(0);
    let edit_families = AtomicClassifiedEditCensus::default();
    let bound = format!(
        "L16 tree fusion: base path-states={}, by length={by_length:?}",
        units.len()
    );
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &units,
        || stages.iter().map(Pair::from_stage).collect::<Vec<_>>(),
        |pairs, (word, source)| {
            let prepared = reference_tree_path_edits(pairs, word, source);
            let inside = prepared.is_some();
            let (final_edits, census) = prepared.unwrap_or_default();
            if !inside {
                outside.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            edits.fetch_add(final_edits.len(), std::sync::atomic::Ordering::Relaxed);
            edit_families.add(&census);
            vec![TreeFusionCase {
                word: word.clone(),
                source: source.clone(),
                edits: final_edits,
                inside,
            }]
        },
        |_, case| {
            let path = production_word(&stages, &case.word).expect("boundary-matched word");
            production_tree_path_outcome(&path, &case.source, &case.edits)
        },
        |pairs, case| {
            if case.inside {
                reference_tree_path_outcome(pairs, &case.word, &case.source, &case.edits)
            } else {
                production_atomic_tree_path_outcome(pairs, &case.word, &case.source, &case.edits)
            }
        },
        shape_path_outcomes_agree,
    ));
    let edit_families = edit_families.into_inner();
    let edits = edits.into_inner();
    let outside = outside.into_inner();
    assert_eq!(
        outside, 920,
        "the production-only tree-path consistency scope is part of the certificate"
    );
    assert_eq!(edit_families.total(), edits);
    println!(
        "L16 tree fusion: bases={}, inside={}, outside={}, final edits={}, admitted families={:?}, refused families={:?}",
        units.len(),
        units.len() - outside,
        outside,
        edits,
        edit_families.admitted.named(),
        edit_families.refused.named()
    );
    assert_eq!(compared, u64::try_from(units.len()).unwrap());
}

#[derive(Clone, Debug)]
struct TableFusionCase {
    word: Vec<usize>,
    source: RefTable,
    edits: Vec<RefTable>,
}

fn table_path_normal_forms_match_independent_atomic_folds() {
    let stages = c11_stages();
    let certificate = c11_path_count_certificate();
    let composable = composable_c11_words(&stages);
    let mut units = Vec::new();
    let mut by_length = [0_usize; 3];
    for word in &composable {
        if stages[word[0]].source == c11_fan_schema() {
            continue;
        }
        for source in c11_table_states(&stages[word[0]].source) {
            by_length[word.len() - 1] += 1;
            units.push((word.clone(), source));
        }
    }
    assert_eq!(by_length, certificate.table_word_states_by_length);
    assert_eq!(units.len(), certificate.table_word_states());
    assert_eq!(units.len(), 100_811);
    assert!(units.len() < 1_000_000);
    let edits = std::sync::atomic::AtomicUsize::new(0);
    let edit_families = AtomicClassifiedEditCensus::default();
    let bound = format!(
        "L16 table fusion: base path-states={}, by length={by_length:?}",
        units.len()
    );
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &units,
        || stages.iter().map(TablePair::from_stage).collect::<Vec<_>>(),
        |table_pairs, (word, source)| {
            let (final_edits, census) = reference_table_path_edits(table_pairs, word, source);
            edits.fetch_add(final_edits.len(), std::sync::atomic::Ordering::Relaxed);
            edit_families.add(&census);
            vec![TableFusionCase {
                word: word.clone(),
                source: source.clone(),
                edits: final_edits,
            }]
        },
        |table_pairs, case| {
            let path = production_word(&stages, &case.word).expect("boundary-matched word");
            production_table_path_outcome(
                &path,
                &table_pairs[case.word[0]].protocol,
                &case.source,
                &case.edits,
            )
        },
        |table_pairs, case| {
            reference_table_path_outcome(table_pairs, &case.word, &case.source, &case.edits)
        },
        shape_path_outcomes_agree,
    ));
    let edit_families = edit_families.into_inner();
    let edits = edits.into_inner();
    assert_eq!(edit_families.total(), edits);
    assert_eq!(
        edit_families.admitted.families,
        [0, 0, 19_258, 28_445, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        "the admitted table-path mutation-family census is part of the certificate"
    );
    assert_eq!(
        edit_families.refused.families,
        [28_445, 0, 1_035, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        "the refused table-path mutation-family census is part of the certificate"
    );
    println!(
        "L16 table fusion: bases={}, final edits={}, admitted families={:?}, refused families={:?}",
        units.len(),
        edits,
        edit_families.admitted.named(),
        edit_families.refused.named()
    );
    assert_eq!(compared, u64::try_from(units.len()).unwrap());
}

/// The per-thread context of the path differential: the lens pairs and the
/// composites (each composable word, then with the identity on either side).
type PathContext = (Vec<Pair>, Vec<(Vec<usize>, LensPath, RefLensPath<usize>)>);

/// One path-state case: the composite's index, its word, source, and the
/// complete final-stage Single-Step Basis in reference coordinates.
#[derive(Debug)]
struct PathCase {
    index: usize,
    source: RefGraph,
    edits: Vec<RefGraph>,
}

/// The complete final-stage Single-Step Basis, evaluated against the final
/// stage's source, view, and complement from the reference fold.
fn edits_of_reference_view(
    pairs: &[Pair],
    word: &[usize],
    source: &RefGraph,
) -> (Vec<RefGraph>, ClassifiedEditCensus) {
    let mut view = source.clone();
    let mut final_source = source.clone();
    let mut final_complement = None;
    for stage in word.iter().copied() {
        let stage_source = view.clone();
        let get = pairs[stage].with_reference(|lens| {
            let lens = lens.ok()?;
            RefGraphShape::get(lens, &view, &()).ok()
        });
        let Some((next, complement)) = get else {
            return (Vec::new(), ClassifiedEditCensus::default());
        };
        final_source = stage_source;
        view = next;
        final_complement = Some(complement);
    }
    let last = *word.last().expect("a path word is non-empty");
    let complement = final_complement.expect("a path word has a final complement");
    pairs[last].with_reference(|lens| {
        let Ok(lens) = lens else {
            return (Vec::new(), ClassifiedEditCensus::default());
        };
        let basis =
            RefGraphShape::single_step_basis(lens, &final_source, &view, &complement, &());
        let mut census = ClassifiedEditCensus::default();
        for case in &basis {
            let observed = RefGraphShape::check_put_domain(lens, &case.view, &complement, &());
            match (&case.domain, observed) {
                (DomainExpectation::Admitted, Ok(())) => census.add(case.mutation, true),
                (DomainExpectation::Refused(expected), Err(observed)) => {
                    assert_eq!(
                        &observed, expected,
                        "declared final-graph refusal for path {word:?}"
                    );
                    census.add(case.mutation, false);
                }
                (expected, observed) => panic!(
                    "final-graph domain mismatch for path {word:?}: expected {expected:?}, observed {observed:?}"
                ),
            }
        }
        (
            basis.into_iter().map(|case| case.view).collect(),
            census,
        )
    })
}

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
                        assert_eq!(format!("{left:?}"), format!("{right:?}"));
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

struct PathPropertyCarrier {
    stages: Vec<C11Stage>,
    words: Vec<Vec<usize>>,
    graphs: Vec<Vec<RefGraph>>,
    trees: Vec<Vec<RefTree>>,
    tables: Vec<Vec<RefTable>>,
    tree_units: Vec<(usize, usize)>,
}

fn path_property_carrier() -> &'static PathPropertyCarrier {
    static CARRIER: std::sync::OnceLock<PathPropertyCarrier> = std::sync::OnceLock::new();
    CARRIER.get_or_init(|| {
        let stages = c11_stages();
        let words = composable_c11_words(&stages);
        let graphs = stages.iter().map(c11_sources).collect();
        let trees: Vec<Vec<RefTree>> = stages
            .iter()
            .map(|stage| {
                if stage.source == c11_fan_schema() {
                    Vec::new()
                } else {
                    c11_tree_states(&stage.source)
                }
            })
            .collect();
        let tables: Vec<Vec<RefTable>> = stages
            .iter()
            .map(|stage| {
                if stage.source == c11_fan_schema() {
                    Vec::new()
                } else {
                    c11_table_states(&stage.source)
                }
            })
            .collect();
        let tree_units = trees
            .iter()
            .enumerate()
            .flat_map(|(stage, sources)| (0..sources.len()).map(move |source| (stage, source)))
            .collect();
        PathPropertyCarrier {
            stages,
            words,
            graphs,
            trees,
            tables,
            tree_units,
        }
    })
}

fn select_one<I: Clone>(cases: &[I], index: usize) -> Vec<I> {
    cases
        .get(index % cases.len().max(1))
        .cloned()
        .into_iter()
        .collect()
}

fn assert_path_on_all_carriers(
    carrier: &PathPropertyCarrier,
    word: &[usize],
    production_paths: &[LensPath],
    reference: &RefLensPath<usize>,
    source_index: usize,
    edit_index: usize,
) {
    let pairs: Vec<Pair> = carrier.stages.iter().map(Pair::from_stage).collect();
    let graph_sources = &carrier.graphs[word[0]];
    let graph_source = &graph_sources[source_index % graph_sources.len()];
    let (graph_edits, _) = edits_of_reference_view(&pairs, word, graph_source);
    let graph_edits = select_one(&graph_edits, edit_index);
    let graph_reference = reference_path_outcome(&pairs, reference, graph_source, &graph_edits);
    for production in production_paths {
        assert!(
            path_outcomes_agree(
                &production_path_outcome(production, graph_source, &graph_edits),
                &graph_reference,
            ),
            "graph path property differs for word {word:?}"
        );
    }

    if carrier.stages[word[0]].source == c11_fan_schema() {
        return;
    }
    let tree_sources = &carrier.trees[word[0]];
    let tree_source = &tree_sources[source_index % tree_sources.len()];
    if let Some((tree_edits, _)) = reference_tree_path_edits(&pairs, word, tree_source) {
        let tree_edits = select_one(&tree_edits, edit_index);
        let tree_reference = reference_tree_path_outcome(&pairs, word, tree_source, &tree_edits);
        for production in production_paths {
            assert!(
                shape_path_outcomes_agree(
                    &production_tree_path_outcome(production, tree_source, &tree_edits),
                    &tree_reference,
                ),
                "common-domain tree path property differs for word {word:?}"
            );
        }
    }

    let table_pairs: Vec<TablePair> = carrier.stages.iter().map(TablePair::from_stage).collect();
    let table_sources = &carrier.tables[word[0]];
    let table_source = &table_sources[source_index % table_sources.len()];
    let (table_edits, _) = reference_table_path_edits(&table_pairs, word, table_source);
    let table_edits = select_one(&table_edits, edit_index);
    let table_reference =
        reference_table_path_outcome(&table_pairs, word, table_source, &table_edits);
    for production in production_paths {
        assert!(
            shape_path_outcomes_agree(
                &production_table_path_outcome(
                    production,
                    &table_pairs[word[0]].protocol,
                    table_source,
                    &table_edits,
                ),
                &table_reference,
            ),
            "table path property differs for word {word:?}"
        );
    }
}

fn assert_randomized_path_form(
    form: panproto_reference::enumerate::lenses::PathLawForm,
    case: usize,
    edit: usize,
) {
    use panproto_reference::enumerate::lenses::PathLawForm;

    let carrier = path_property_carrier();
    let word = &carrier.words[case % carrier.words.len()];
    let atomic = production_word(&carrier.stages, word).expect("a certified word composes");
    let first = &carrier.stages[word[0]];
    let last = &carrier.stages[*word.last().expect("a path word is nonempty")];
    let production = match form {
        PathLawForm::Sequential => atomic,
        PathLawForm::LeftIdentity => LensPath::identity(&prod_schema(&first.source))
            .then(&atomic)
            .expect("left identity boundary matches"),
        PathLawForm::RightIdentity => atomic
            .then(&LensPath::identity(&prod_schema(&last.target)))
            .expect("right identity boundary matches"),
    };
    let reference = reference_word(&carrier.stages, word).expect("a certified word composes");
    assert_path_on_all_carriers(carrier, word, &[production], &reference, case, edit);
}

fn assert_randomized_path_associativity(case: usize, edit: usize) {
    let carrier = path_property_carrier();
    let triples: Vec<&Vec<usize>> = carrier
        .words
        .iter()
        .filter(|word| word.len() == 3)
        .collect();
    let word = triples[case % triples.len()];
    let single = |index: usize| LensPath::single(production_stage(&carrier.stages[index]));
    let left = single(word[0])
        .then(&single(word[1]))
        .and_then(|path| path.then(&single(word[2])))
        .expect("a certified triple composes under left bracketing");
    let right = single(word[1])
        .then(&single(word[2]))
        .and_then(|path| single(word[0]).then(&path))
        .expect("a certified triple composes under right bracketing");
    assert_eq!(
        left.identity_envelope(ComplementShape::Graph),
        right.identity_envelope(ComplementShape::Graph),
        "both associative bracketings retain the same atomic envelopes"
    );
    let reference = reference_word(&carrier.stages, word).expect("a certified word composes");
    assert_path_on_all_carriers(carrier, word, &[left, right], &reference, case, edit);
}

fn certificate_column_protocol(scalar: ScalarKind) -> Protocol {
    Protocol {
        value_kinds: BTreeMap::from([("value".to_owned(), scalar)]),
        non_value_kinds: ["record".to_owned()].into(),
        ..Protocol::default()
    }
}

fn certificate_column_schema(protocol: &Protocol, value_kind: &str, columns: &[&str]) -> Schema {
    let mut builder = SchemaBuilder::new(protocol)
        .vertex("row", "record", None)
        .expect("the boundary row is valid")
        .vertex("value", value_kind, None)
        .expect("the boundary value is valid");
    for column in columns {
        builder = builder
            .edge("row", "value", "property", Some(*column))
            .expect("the boundary column is valid");
    }
    builder.build().expect("the boundary schema is valid")
}

fn certificate_edge_named(schema: &Schema, name: &str) -> Edge {
    schema
        .edges
        .keys()
        .find(|edge| edge.name.as_deref() == Some(name))
        .cloned()
        .expect("the named boundary column exists")
}

fn certificate_mapped_lens(
    source: &Schema,
    target: &Schema,
    edge_map: impl IntoIterator<Item = (Edge, Edge)>,
) -> Lens {
    Lens {
        compiled: CompiledMigration {
            surviving_verts: target.vertices.keys().cloned().collect(),
            surviving_edges: target.edges.keys().cloned().collect(),
            edge_remap: edge_map.into_iter().collect(),
            ..CompiledMigration::default()
        },
        src_schema: source.clone(),
        tgt_schema: target.clone(),
    }
}

fn table_fusion_column_injectivity_witness() {
    let protocol = certificate_column_protocol(ScalarKind::Int {
        bits: 64,
        signed: true,
    });
    let source = certificate_column_schema(&protocol, "value", &["a", "z"]);
    let middle = certificate_column_schema(&protocol, "value", &["x", "y"]);
    let target = certificate_column_schema(&protocol, "value", &["q"]);
    let first = certificate_mapped_lens(
        &source,
        &middle,
        [
            (
                certificate_edge_named(&source, "a"),
                certificate_edge_named(&middle, "y"),
            ),
            (
                certificate_edge_named(&source, "z"),
                certificate_edge_named(&middle, "x"),
            ),
        ],
    );
    let second = certificate_mapped_lens(
        &middle,
        &target,
        [
            (
                certificate_edge_named(&middle, "x"),
                certificate_edge_named(&target, "q"),
            ),
            (
                certificate_edge_named(&middle, "y"),
                certificate_edge_named(&target, "q"),
            ),
        ],
    );
    let path = LensPath::single(first)
        .then(&LensPath::single(second))
        .expect("the collision witness has a shared middle schema");
    assert_eq!(
        path.table_normal_form(&[protocol.clone(), protocol.clone(), protocol])
            .unwrap()
            .len(),
        2
    );
}

fn table_fusion_placeholder_witness() {
    let source_protocol = certificate_column_protocol(ScalarKind::Bool);
    let middle_protocol = certificate_column_protocol(ScalarKind::Int {
        bits: 64,
        signed: true,
    });
    let schema = certificate_column_schema(&source_protocol, "value", &["f"]);
    let first = certificate_mapped_lens(&schema, &schema, std::iter::empty());
    let mut second = certificate_mapped_lens(&schema, &schema, std::iter::empty());
    second
        .compiled
        .conditional_survival
        .insert("row".into(), panproto_expr::Expr::Var("f".into()));
    let path = LensPath::single(first)
        .then(&LensPath::single(second))
        .expect("the placeholder witness has a shared middle schema");
    assert_eq!(
        path.table_normal_form(&[source_protocol, middle_protocol.clone(), middle_protocol,])
            .unwrap()
            .len(),
        2
    );
}

fn table_fusion_required_value_witness() {
    use panproto_gat::Name;
    use panproto_schema::{Constraint, ConstraintSemantics};

    let mut protocol = certificate_column_protocol(ScalarKind::Int {
        bits: 64,
        signed: true,
    });
    protocol.constraint_sorts.push("required".to_owned());
    protocol
        .constraint_semantics
        .insert("required".to_owned(), ConstraintSemantics::RequiredValue);
    let schema = certificate_column_schema(&protocol, "value", &["f"]);
    let mut required_middle = schema.clone();
    required_middle.constraints.insert(
        Name::from("value"),
        vec![Constraint {
            sort: Name::from("required"),
            value: "true".to_owned(),
        }],
    );
    let first = certificate_mapped_lens(&schema, &required_middle, std::iter::empty());
    let second = certificate_mapped_lens(&required_middle, &schema, std::iter::empty());
    let path = LensPath::single(first)
        .then(&LensPath::single(second))
        .expect("the validation witness compares equal middle carriers");
    assert_eq!(
        path.table_normal_form(&[protocol.clone(), protocol.clone(), protocol])
            .unwrap()
            .len(),
        2
    );
}

fn table_fusion_target_only_carrier_witness() {
    let protocol = Protocol {
        non_value_kinds: ["record".to_owned()].into(),
        ..Protocol::default()
    };
    let source = SchemaBuilder::new(&protocol)
        .vertex("row", "record", None)
        .expect("the source row is valid")
        .build()
        .expect("the source schema is valid");
    let middle = SchemaBuilder::new(&protocol)
        .vertex("row", "record", None)
        .expect("the middle row is valid")
        .vertex("target_only", "record", None)
        .expect("the target-only table is valid")
        .build()
        .expect("the middle schema is valid");
    let first = certificate_mapped_lens(&source, &middle, std::iter::empty());
    assert!(!panproto_lens::table_lens::is_isomorphism(
        &first,
        BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        }
    ));
    let second = certificate_mapped_lens(&middle, &middle, std::iter::empty());
    let path = LensPath::single(first)
        .then(&LensPath::single(second))
        .expect("the target-only witness has a shared middle schema");
    assert_eq!(
        path.table_normal_form(&[protocol.clone(), protocol.clone(), protocol])
            .unwrap()
            .len(),
        2
    );
}

fn path_certificate_manifest_is_exact() {
    let mut cases = 0_usize;
    let mut boundaries = Vec::new();
    for_each_path_law_case(|case| {
        cases += 1;
        if let PathLawCase::TableFusionBoundary(witness) = case {
            boundaries.push(witness);
        }
    });
    assert_eq!(cases, path_law_case_count_certificate());
    assert_eq!(cases, 2_003_085);
    assert_eq!(
        boundaries,
        [
            TableFusionBoundaryWitness::ColumnMapInjectivity,
            TableFusionBoundaryWitness::TypedPlaceholder,
            TableFusionBoundaryWitness::RequiredValueValidation,
            TableFusionBoundaryWitness::TargetOnlyRelationalCarrier,
        ]
    );
    table_fusion_column_injectivity_witness();
    table_fusion_placeholder_witness();
    table_fusion_required_value_witness();
    table_fusion_target_only_carrier_witness();
}

/// One directly runnable certificate for the path category and its graph,
/// tree, and table operational semantics.
#[test]
fn lens_path_law_certificate() {
    path_certificate_manifest_is_exact();
    lens_path_category_laws_agree_with_the_reference_exhaustively();
    the_c11_alphabet_witnesses_every_tree_fusion_condition();
    lens_paths_agree_with_the_reference_exhaustively();
    tree_path_normal_forms_match_independent_atomic_folds();
    table_path_normal_forms_match_independent_atomic_folds();
}

proptest! {
    #![proptest_config(ProptestConfig::default())]
    #[test]
    fn randomized_path_left_identity_differential(case in any::<usize>(), edit in any::<usize>()) {
        assert_randomized_path_form(
            panproto_reference::enumerate::lenses::PathLawForm::LeftIdentity,
            case,
            edit,
        );
    }

    #[test]
    fn randomized_path_right_identity_differential(case in any::<usize>(), edit in any::<usize>()) {
        assert_randomized_path_form(
            panproto_reference::enumerate::lenses::PathLawForm::RightIdentity,
            case,
            edit,
        );
    }

    #[test]
    fn randomized_path_associativity_differential(case in any::<usize>(), edit in any::<usize>()) {
        assert_randomized_path_associativity(case, edit);
    }

    #[test]
    fn randomized_path_fusion_differential(case in any::<usize>(), edit in any::<usize>()) {
        assert_randomized_path_form(
            panproto_reference::enumerate::lenses::PathLawForm::Sequential,
            case,
            edit,
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn randomized_protolens_instance_differential(
        schema_seed in any::<usize>(),
        state_seed in any::<usize>(),
    ) {
        const ELEMENTARY_PROTOLENSES: usize = 4;
        for protolens in 0..ELEMENTARY_PROTOLENSES {
            let index = schema_seed
                .wrapping_mul(ELEMENTARY_PROTOLENSES)
                .wrapping_add(protolens);
            if let Err(message) = property_case(8, index, state_seed) {
                prop_assert!(false, "elementary protolens {protolens}: {message}");
            }
        }
    }
}

fn randomized_table_complement_case(index: usize) {
    let fixture_count = table_fixture_count_certificate().total;
    let construction_count = table_complement_construction_count_certificate();
    let refusal_count = table_get_refusal_count_certificate();
    let total = table_complement_law_count_certificate();
    assert_eq!(total, fixture_count + construction_count + refusal_count);
    assert_eq!(total, 9_937);

    let fixture_index = index % fixture_count;
    let fixture = &table_fixtures()[fixture_index];
    let pair = fixture_pair(fixture);
    assert_eq!(
        production_table_get(&pair, &fixture.source),
        reference_table_get(&pair, &fixture.source),
        "randomized table-complement fixture {fixture_index}"
    );

    let mut constructions = Vec::new();
    for_each_table_complement_construction(|construction| constructions.push(construction));
    let construction = &constructions[index.rotate_left(17) % construction_count];
    let pair = TablePair::new(
        construction.source_schema.clone(),
        construction.target_schema.clone(),
        construction.migration.clone(),
        WRITER_VALUE_KIND,
        ScalarKind::Any,
    );
    assert_eq!(
        production_table_get(&pair, &construction.source),
        reference_table_get(&pair, &construction.source),
        "randomized complement construction {}",
        construction.name
    );

    assert_table_get_refusal_case(TableGetRefusalCase::ALL[index.rotate_left(33) % refusal_count]);
}

fn assert_randomized_projection_observation(mut index: usize) {
    let projection_count = table_fixture_get_put_count_certificate();
    assert!(index < projection_count);
    let mut selected = None;
    for_each_table_fixture(|fixture| {
        if selected.is_some() {
            return;
        }
        let retained_columns = fixture.retained.count_ones() as usize;
        let observations =
            1 + table_view_edits_count(fixture.source.rows.len(), retained_columns, 0);
        if index < observations {
            let pair = fixture_pair(&fixture);
            let (view, complement) =
                reference_table_get(&pair, &fixture.source).expect("projection fixtures get");
            assert!(complement.dropped_rows.is_empty());
            let columns: Vec<String> = (0..fixture.columns)
                .filter(|column| fixture.retained & (1 << column) != 0)
                .map(|column| format!("c{column}"))
                .collect();
            let selected_view = if index == 0 {
                view
            } else {
                table_view_edits(&view, &columns, &[])
                    .into_iter()
                    .nth(index - 1)
                    .expect("the projection observation index is in range")
            };
            selected = Some((fixture, selected_view));
        } else {
            index -= observations;
        }
    });
    let (fixture, view) = selected.expect("the projection count covers every observation index");
    let pair = fixture_pair(&fixture);
    assert_eq!(
        production_table_put(&pair, &fixture.source, &view),
        reference_table_put(&pair, &fixture.source, &view),
        "randomized projection observation"
    );
}

fn assert_randomized_writer_observation(mut index: usize) {
    assert!(index < writer_count_certificate().2);
    let mut selected = None;
    for_each_writer_fixture(|fixture| {
        for target in fixture.target_columns() {
            for edit in WRITER_EDITS {
                if selected.is_none() {
                    if index == 0 {
                        selected = Some((fixture.clone(), target, edit));
                    } else {
                        index -= 1;
                    }
                }
            }
        }
    });
    let (fixture, target, edit) =
        selected.expect("the writer count covers every observation index");
    let pair = writer_pair(&fixture);
    let source = fixture.source();
    let (mut view, _) = reference_table_get(&pair, &source).expect("writer fixture gets");
    view.rows
        .get_mut(&0)
        .expect("writer fixture has row zero")
        .1
        .insert(target.to_owned(), RefValue::Int(edit));
    assert_eq!(
        production_table_put(&pair, &source, &view),
        reference_table_put(&pair, &source, &view),
        "randomized writer observation"
    );
}

fn randomized_table_get_put_case(index: usize) {
    let projection_count = table_fixture_get_put_count_certificate();
    let writer_count = writer_count_certificate().2;
    let derived_count = derived_writer_count_certificate();
    let pair_count = table_pair_put_count_certificate();
    let refusal_count = table_put_refusal_count_certificate();
    let total = table_get_put_law_count_certificate();
    assert_eq!(projection_count, 156_513);
    assert_eq!(writer_count, 180);
    assert_eq!(derived_count, 2);
    assert_eq!(pair_count, 5);
    assert_eq!(refusal_count, 4);
    assert_eq!(
        total,
        projection_count + writer_count + derived_count + pair_count + refusal_count
    );
    assert_eq!(total, 156_704);

    assert_randomized_projection_observation(index % projection_count);
    assert_randomized_writer_observation(index.rotate_left(13) % writer_count);
    randomized_derived_writer_case(index.rotate_left(27) % derived_count);
    assert_table_pair_put_case(TablePairPutCase::ALL[index.rotate_left(41) % pair_count]);
    assert_table_put_refusal_case(TablePutRefusalCase::ALL[index.rotate_left(53) % refusal_count]);
}

fn randomized_derived_writer_case(index: usize) {
    let edit = WRITER_EDITS[index % WRITER_EDITS.len()];
    let fixture = panproto_reference::enumerate::lenses::DerivedWriterFixture { edit };
    let pair = TablePair::new(
        fixture.source_schema(),
        fixture.target_schema(),
        fixture.migration(),
        WRITER_VALUE_KIND,
        ScalarKind::Int {
            bits: 64,
            signed: true,
        },
    );
    let source = fixture.source();
    let (mut view, _) = reference_table_get(&pair, &source).expect("derived fixture gets");
    view.rows
        .get_mut(&0)
        .expect("derived fixture has row zero")
        .1
        .insert("z".to_owned(), RefValue::Int(edit));
    assert_eq!(
        production_table_put(&pair, &source, &view),
        reference_table_put(&pair, &source, &view)
    );
}

proptest! {
    #![proptest_config(ProptestConfig::default())]

    #[test]
    fn randomized_table_complement_law_differential(index in any::<usize>()) {
        randomized_table_complement_case(index);
    }

    #[test]
    fn randomized_table_get_put_law_differential(index in any::<usize>()) {
        randomized_table_get_put_case(index);
    }
}

// ---------------------------------------------------------------------------
// F.B squares: the production graph route and fast route against the
// reference's two routes
// ---------------------------------------------------------------------------

use panproto_reference::lens::{SquareError, SquareResult, SquareRouteObservation};

/// The identity-level part of a square refusal. Reference envelopes carry
/// both complete endpoints while production reports the first differing
/// coordinate, so the shared contract names the mathematical refusal class.
#[derive(Clone, Debug, PartialEq, Eq)]
enum SquareIdentityReason {
    Mismatch {
        field: String,
        expected: String,
        found: String,
    },
    UnboundComplement,
    LegacyComplementRejected(String),
    UnsupportedEnvelopeVersion(u16),
    Undecodable(String),
    UnexpectedReference(String),
}

/// A route refusal in common production/reference coordinates.
///
/// Only variants with a proved cross-implementation correspondence share a
/// constructor. Unexpected implementation-specific failures remain on
/// distinct sides, so an exhaustive differential cannot accept them merely
/// because both routes failed.
#[derive(Clone, Debug, PartialEq, Eq)]
enum SquareReason {
    Restrict(String),
    PutDomain(String),
    PutDomainReport(Vec<String>),
    Identity(SquareIdentityReason),
    TreeEncoding(String),
    TableEncoding(String),
    TreeOperational(String),
    TableOperational(String),
    NamedCollationUnresolved(Vec<String>),
    NotAnIsomorphism(String),
    OutsideExactDomain(String),
    UnexpectedProduction(String),
}

fn production_square_identity_reason(error: &panproto_inst::IdentityError) -> SquareIdentityReason {
    use panproto_inst::IdentityError as I;

    match error {
        I::Mismatch {
            field,
            expected,
            found,
        } => SquareIdentityReason::Mismatch {
            field: field.to_string(),
            expected: expected.clone(),
            found: found.clone(),
        },
        I::UnboundComplement => SquareIdentityReason::UnboundComplement,
        I::LegacyComplementRejected { reason } => {
            SquareIdentityReason::LegacyComplementRejected(format!("{reason:?}"))
        }
        I::UnsupportedEnvelopeVersion { found } => {
            SquareIdentityReason::UnsupportedEnvelopeVersion(*found)
        }
        I::Undecodable(detail) => SquareIdentityReason::Undecodable(detail.clone()),
    }
}

fn reference_identity_digest(bytes: &[u8; 32]) -> String {
    use std::fmt::Write as _;

    let mut encoded = String::with_capacity(64);
    for byte in bytes {
        write!(encoded, "{byte:02x}").expect("writing to a String is infallible");
    }
    encoded
}

fn reference_identity_format(identity: &RefIdentityEnvelope) -> String {
    let shape = match identity.shape {
        RefComplementShape::WType => "wtype",
        RefComplementShape::Graph => "graph",
        RefComplementShape::Functor => "functor",
        RefComplementShape::Resolution => "resolution",
    };
    format!("{shape}/{}", identity.format_version)
}

fn reference_square_identity_mismatch(
    expected: &RefIdentityEnvelope,
    found: &RefIdentityEnvelope,
) -> SquareIdentityReason {
    let mismatch = |field: &str, expected: String, found: String| SquareIdentityReason::Mismatch {
        field: field.to_owned(),
        expected,
        found,
    };
    if expected.source_schema != found.source_schema {
        return mismatch(
            "source_schema",
            reference_identity_digest(&expected.source_schema),
            reference_identity_digest(&found.source_schema),
        );
    }
    if expected.target_schema != found.target_schema {
        return mismatch(
            "target_schema",
            reference_identity_digest(&expected.target_schema),
            reference_identity_digest(&found.target_schema),
        );
    }
    if expected.lens != found.lens {
        return mismatch(
            "lens",
            reference_identity_digest(&expected.lens),
            reference_identity_digest(&found.lens),
        );
    }
    if expected.shape != found.shape || expected.format_version != found.format_version {
        return mismatch(
            "format",
            reference_identity_format(expected),
            reference_identity_format(found),
        );
    }
    SquareIdentityReason::UnexpectedReference("mismatch reported for equal envelopes".to_owned())
}

fn reference_graph_square_identity_reason(
    error: &panproto_reference::lens::IdentityError,
) -> SquareIdentityReason {
    use panproto_reference::lens::IdentityError as I;

    match error {
        I::Mismatch { expected, found } => reference_square_identity_mismatch(expected, found),
        I::UnboundComplement => SquareIdentityReason::UnboundComplement,
    }
}

fn reference_table_square_identity_reason(
    error: &panproto_reference::table::IdentityError,
) -> SquareIdentityReason {
    use panproto_reference::table::IdentityError as I;

    match error {
        I::Mismatch { expected, found } => reference_square_identity_mismatch(expected, found),
        I::UnboundComplement => SquareIdentityReason::UnboundComplement,
    }
}

fn production_square_reason(error: &LensError) -> SquareReason {
    match error {
        LensError::Restrict(error) => SquareReason::Restrict(restrict_reason(error)),
        LensError::PutDomain(error) => SquareReason::PutDomain(put_domain_reason(error)),
        LensError::Identity(error) => {
            SquareReason::Identity(production_square_identity_reason(error))
        }
        LensError::NotAnIsomorphism { detail } => SquareReason::NotAnIsomorphism(detail.clone()),
        other => SquareReason::UnexpectedProduction(format!("{other:?}")),
    }
}

fn reference_square_reason(error: &SquareError) -> SquareReason {
    match error {
        SquareError::TreeEncoding(error) => SquareReason::TreeEncoding(format!("{error:?}")),
        SquareError::TableEncoding(error) => SquareReason::TableEncoding(format!("{error:?}")),
        SquareError::Restrict(error) => SquareReason::Restrict(ref_restrict_reason(error)),
        SquareError::PutDomain(error) => SquareReason::PutDomain(ref_put_domain_reason(error)),
        SquareError::PutDomainReport(errors) => {
            SquareReason::PutDomainReport(errors.iter().map(ref_put_domain_reason).collect())
        }
        SquareError::GraphIdentity(error) => {
            SquareReason::Identity(reference_graph_square_identity_reason(error))
        }
        SquareError::TreeOperational(error) => SquareReason::TreeOperational(format!("{error:?}")),
        SquareError::TableIdentity(error) => {
            SquareReason::Identity(reference_table_square_identity_reason(error))
        }
        SquareError::TableOperational(error) => {
            SquareReason::TableOperational(format!("{error:?}"))
        }
        SquareError::NamedCollationUnresolved { names } => {
            SquareReason::NamedCollationUnresolved(names.iter().cloned().collect())
        }
        SquareError::NotAnIsomorphism { detail } => SquareReason::NotAnIsomorphism(detail.clone()),
        SquareError::OutsideExactDomain(detail) => {
            SquareReason::OutsideExactDomain((*detail).to_owned())
        }
    }
}

/// One leg of a square in reference coordinates: get (view and complement
/// without identity) and the put of the unedited view.
#[derive(Debug, PartialEq, Eq)]
struct Leg {
    get: Result<(RefGraph, RefComplement), SquareReason>,
    put: Option<Result<RefGraph, SquareReason>>,
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
            .map_err(reference_square_reason),
        put: route
            .put
            .as_ref()
            .map(|put| put.clone().map_err(|error| reference_square_reason(&error))),
    }
}

/// Compare complete normalized observations, including the exact typed
/// reason for every get or put refusal.
fn legs_agree(production: &Leg, reference: &Leg) -> bool {
    production == reference
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
    if !legs_agree(&production.graph, &reference.graph) {
        return false;
    }
    if reference.outside {
        return production
            .fast
            .as_ref()
            .is_none_or(|fast| falls_back_to_the_graph_route(fast, &production.graph));
    }
    if !reference.commutes || !production.commutes {
        return false;
    }
    match (&production.fast, &reference.fast) {
        (Some(production), Some(reference)) => legs_agree(production, reference),
        _ => false,
    }
}

fn production_graph_leg(lens: &Lens, graph: &GInstance, edited: Option<&GInstance>) -> Leg {
    match graph_lens::get(lens, graph) {
        Err(error) => Leg {
            get: Err(production_square_reason(&error)),
            put: None,
        },
        Ok((view, complement)) => Leg {
            get: Ok((ref_graph(&view), ref_complement(&complement))),
            put: Some(
                graph_lens::put(lens, edited.unwrap_or(&view), &complement)
                    .map(|restored| ref_graph(&restored))
                    .map_err(|error| production_square_reason(&error)),
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

#[derive(Clone, Debug)]
struct TableSquareCase {
    fixture: TableFixture,
    edited: Option<RefTable>,
}

fn production_table_square(pair: &TablePair, case: &TableSquareCase) -> SquareOutcome {
    let (graph, _) = GInstance::from_finstance(&prod_table(&case.fixture.source)).unwrap();
    let edited = case.edited.as_ref().map(|edited| {
        let (view, _) = graph_lens::get(&pair.production, &graph).unwrap();
        let (_, mut embedding) = view
            .try_to_finstance(&pair.production.tgt_schema)
            .expect("the graph view is table-shaped");
        GInstance::from_finstance_along(&prod_table(edited), &mut embedding)
            .expect("an admitted edited table has a durable graph presentation")
    });
    let graph_leg = production_graph_leg(&pair.production, &graph, edited.as_ref());
    let fast = graph_lens_via_table(pair, &graph, edited.as_ref());
    // Within production, compare exact errors as well as successful values.
    // The cross-implementation reason map is certified by the get/put
    // differentials above.
    let commutes = fast.as_ref() == Some(&graph_leg);
    SquareOutcome {
        graph: graph_leg,
        fast,
        commutes,
        outside: false,
    }
}

#[test]
fn table_square_agrees_with_the_reference_on_the_fixture_scope() {
    let certificate = table_fixture_count_certificate();
    let mut fixtures = Vec::new();
    for_each_table_fixture(|fixture| fixtures.push(fixture));
    assert_eq!(fixtures.len(), certificate.total);
    let bound = "table square: 9,921 fixtures and every one-step edited view".to_owned();
    println!("{bound}");
    let edits = std::sync::atomic::AtomicUsize::new(0);
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &fixtures,
        || (),
        |(), fixture| {
            let pair = fixture_pair(fixture);
            let (view, complement) = reference_table_get(&pair, &fixture.source)
                .expect("the fixture table get succeeds");
            let columns: Vec<String> = (0..fixture.columns)
                .filter(|column| fixture.retained & (1 << column) != 0)
                .map(|column| format!("c{column}"))
                .collect();
            let reserved: Vec<u32> = complement.dropped_rows.keys().copied().collect();
            let edited_views = table_view_edits(&view, &columns, &reserved);
            assert_eq!(
                edited_views.len(),
                table_view_edits_count(view.rows.len(), columns.len(), reserved.len()),
                "independent table edit formula"
            );
            edits.fetch_add(edited_views.len(), std::sync::atomic::Ordering::Relaxed);
            std::iter::once(TableSquareCase {
                fixture: fixture.clone(),
                edited: None,
            })
            .chain(edited_views.into_iter().map(|edited| TableSquareCase {
                fixture: fixture.clone(),
                edited: Some(edited),
            }))
            .collect()
        },
        |(), case| {
            let pair = fixture_pair(&case.fixture);
            production_table_square(&pair, case)
        },
        |(), case| {
            let pair = fixture_pair(&case.fixture);
            let graph_pair = table_square_pair_graph(&pair);
            graph_pair.with_reference(|graph_lens| {
                let graph_lens = graph_lens.expect("fixture plans compile");
                pair.with_reference(|table_lens, _| {
                    let result = case.edited.as_ref().map_or_else(
                        || {
                            panproto_reference::lens::table_square(
                                graph_lens,
                                table_lens,
                                &case.fixture.source,
                            )
                        },
                        |edited| {
                            panproto_reference::lens::table_put_square(
                                graph_lens,
                                table_lens,
                                &case.fixture.source,
                                edited,
                            )
                        },
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
    let edits = edits.into_inner();
    assert_eq!(edits, 146_592);
    assert_eq!(compared, 156_513);
}

/// The production table route of the square: get through the table lens
/// and back to graph coordinates, and the put of the unedited view.
fn graph_lens_via_table(
    pair: &TablePair,
    graph: &GInstance,
    edited: Option<&GInstance>,
) -> Option<Leg> {
    let lens = &pair.production;
    match panproto_lens::graph_get_via_table(lens, graph, pair.protocols()) {
        Ok(None) => None,
        Err(error) => Some(Leg {
            get: Err(production_square_reason(&error)),
            put: None,
        }),
        Ok(Some((view, complement))) => Some(Leg {
            get: Ok((ref_graph(&view), ref_complement(&complement))),
            put: match panproto_lens::graph_put_via_table(
                lens,
                edited.unwrap_or(&view),
                &complement,
                pair.protocols(),
            ) {
                Ok(None) => None,
                Ok(Some(restored)) => Some(Ok(ref_graph(&restored))),
                Err(error) => Some(Err(production_square_reason(&error))),
            },
        }),
    }
}

fn graph_lens_via_tree(lens: &Lens, graph: &GInstance, edited: Option<&GInstance>) -> Option<Leg> {
    match panproto_lens::graph_get_via_tree(lens, graph) {
        Ok(None) => None,
        Err(error) => Some(Leg {
            get: Err(production_square_reason(&error)),
            put: None,
        }),
        Ok(Some((view, complement))) => Some(Leg {
            get: Ok((ref_graph(&view), ref_complement(&complement))),
            put: match panproto_lens::graph_put_via_tree(lens, edited.unwrap_or(&view), &complement)
            {
                Ok(None) => None,
                Ok(Some(restored)) => Some(Ok(ref_graph(&restored))),
                Err(error) => Some(Err(production_square_reason(&error))),
            },
        }),
    }
}

#[derive(Clone, Debug)]
struct TreeSquareCase {
    stage: usize,
    source: RefTree,
    edited: Option<RefTree>,
}

fn production_tree_square(pair: &Pair, case: &TreeSquareCase) -> SquareOutcome {
    let lens = &pair.production;
    let source = prod_tree(&case.source);
    let graph = GInstance::from_wtype(&source).unwrap();
    let edited_graph = case.edited.as_ref().map(|edited| {
        let (view, _) = graph_lens::get(lens, &graph).unwrap();
        let (_, mut embedding) = view
            .to_wtype_embedded(&lens.tgt_schema)
            .expect("the graph view is tree-shaped");
        for &(_, child, _) in &edited.arcs {
            if !embedding.arcs.contains_key(&child) {
                let edge = embedding.allocate_edge().expect("bounded edge id");
                embedding.arcs.insert(child, edge);
            }
        }
        GInstance::from_wtype_under(&prod_tree(edited), &embedding, Some(&lens.tgt_schema))
            .expect("an admitted edited tree has a durable graph presentation")
    });
    let graph_leg = production_graph_leg(lens, &graph, edited_graph.as_ref());
    let fast = graph_lens_via_tree(lens, &graph, edited_graph.as_ref());
    let exact_tree_order = match (
        panproto_lens::get(lens, &source),
        graph_lens::get(lens, &graph),
    ) {
        (Ok((tree_view, tree_complement)), Ok((graph_view, graph_complement))) => {
            let tree_put_view = case.edited.as_ref().map_or(tree_view, prod_tree);
            let graph_put_view = edited_graph.as_ref().unwrap_or(&graph_view);
            match (
                panproto_lens::put(lens, &tree_put_view, &tree_complement),
                graph_lens::put(lens, graph_put_view, &graph_complement),
            ) {
                (Ok(tree_source), Ok(graph_source)) => graph_source
                    .to_wtype(tree_source.root)
                    .is_ok_and(|graph_tree| ref_tree(&graph_tree) == ref_tree(&tree_source)),
                (Err(_), Err(_)) => true,
                _ => false,
            }
        }
        (Err(tree_error), Err(graph_error)) => {
            production_square_reason(&tree_error) == production_square_reason(&graph_error)
        }
        _ => false,
    };
    // Within production, compare exact errors as well as successful values;
    // the native-tree equality additionally observes child order.
    let commutes = fast.as_ref() == Some(&graph_leg) && exact_tree_order;
    SquareOutcome {
        graph: graph_leg,
        fast,
        commutes,
        outside: false,
    }
}

fn graph_square_units(stages: &[C11Stage]) -> Vec<(usize, RefTree)> {
    let mut units = Vec::new();
    for (index, stage) in stages.iter().enumerate() {
        if stage.source != c11_fan_schema() {
            units.extend(
                c11_tree_states(&stage.source)
                    .into_iter()
                    .map(|tree| (index, tree)),
            );
        }
    }
    units
}

fn assert_graph_square_carrier_metadata(stages: &[C11Stage], units: &[(usize, RefTree)]) {
    let certificate = c11_tree_square_count_certificate();
    let path_certificate = c11_path_count_certificate();
    let schemas = sigma2_schemas();
    let nonfan_stages = stages
        .iter()
        .filter(|stage| stage.source != c11_fan_schema())
        .count();
    let schema_tree_states: usize = schemas
        .iter()
        .map(|schema| c11_tree_states(schema).len())
        .sum();
    assert_eq!(schemas.len(), sigma2_count());
    assert_eq!(schemas.len(), 16);
    assert_eq!(nonfan_stages, 29);
    assert_eq!(path_certificate.tree_stages, nonfan_stages);
    assert_eq!(schema_tree_states, 2_020);
    assert_eq!(certificate.bases, path_certificate.stage_tree_states);
    assert_eq!(units.len(), certificate.bases);
}

fn assert_graph_square_census(
    bases: usize,
    edits: usize,
    edit_families: &ClassifiedEditCensus,
    compared: u64,
) {
    let certificate = c11_tree_square_count_certificate();
    assert_eq!(edit_families.total(), edits);
    assert_eq!(edits, certificate.edited_views);
    assert_eq!(bases + edits, certificate.observations);
    println!(
        "C11 graph/tree square: bases={bases}, edits={edits}, admitted families={:?}, refused families={:?}, total={}",
        edit_families.admitted.named(),
        edit_families.refused.named(),
        bases + edits
    );
    assert_eq!(compared, u64::try_from(certificate.observations).unwrap());
}

fn graph_square_bound() -> String {
    let bound = "C11 graph/tree square: every one-step tree SSB view".to_owned();
    println!("{bound}");
    bound
}

#[test]
fn graph_square_agrees_with_the_reference_exhaustively() {
    let stages = c11_stages();
    let units = graph_square_units(&stages);
    assert_graph_square_carrier_metadata(&stages, &units);
    let bound = graph_square_bound();
    let edits = std::sync::atomic::AtomicUsize::new(0);
    let edit_families = AtomicClassifiedEditCensus::default();
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &units,
        || stages.iter().map(Pair::from_stage).collect::<Vec<Pair>>(),
        |pairs, (stage, tree)| {
            let mut cases = vec![TreeSquareCase {
                stage: *stage,
                source: tree.clone(),
                edited: None,
            }];
            pairs[*stage].with_reference(|lens| {
                let Ok(lens) = lens else { return };
                let Ok((view, complement)) = RefTreeShape::get(lens, tree, &()) else {
                    return;
                };
                let basis = RefTreeShape::single_step_basis(lens, tree, &view, &complement, &());
                let mut basis_census = ClassifiedEditCensus::default();
                for case in &basis {
                    let observed =
                        RefTreeShape::check_put_domain(lens, &case.view, &complement, &());
                    match (&case.domain, observed) {
                        (DomainExpectation::Admitted, Ok(())) => {
                            basis_census.add(case.mutation, true);
                        }
                        (DomainExpectation::Refused(expected), Err(observed)) => {
                            assert_eq!(
                                &observed, expected,
                                "declared tree-square refusal at stage {stage}"
                            );
                            basis_census.add(case.mutation, false);
                        }
                        (expected, observed) => panic!(
                            "tree-square domain mismatch at stage {stage}: expected {expected:?}, observed {observed:?}"
                        ),
                    }
                }
                assert_eq!(
                    basis_census.admitted,
                    tree_edit_census(&pairs[*stage], &view, &complement),
                    "independent admitted tree-square edit census at stage {stage}"
                );
                edit_families.add(&basis_census);
                edits.fetch_add(basis.len(), std::sync::atomic::Ordering::Relaxed);
                cases.extend(basis.into_iter().map(|case| TreeSquareCase {
                    stage: *stage,
                    source: tree.clone(),
                    edited: Some(case.view),
                }));
            });
            cases
        },
        |pairs, case| production_tree_square(&pairs[case.stage], case),
        |pairs, case| {
            pairs[case.stage].with_reference(|lens| {
                // A plan that does not compile refuses both routes' get.
                let lens = match lens {
                    Ok(lens) => lens,
                    Err(reason) => {
                        return SquareOutcome {
                            graph: Leg {
                                get: Err(SquareReason::Restrict(reason)),
                                put: None,
                            },
                            fast: None,
                            commutes: false,
                            outside: true,
                        };
                    }
                };
                let result = case.edited.as_ref().map_or_else(
                    || panproto_reference::lens::graph_square(lens, &case.source),
                    |edited| {
                        let (_, complement) = RefTreeShape::get(lens, &case.source, &())
                            .expect("case expansion required a successful tree get");
                        panproto_reference::lens::graph_put_square(
                            lens,
                            &case.source,
                            edited,
                            &complement.embedding,
                        )
                    },
                );
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
    let edits = edits.into_inner();
    let edit_families = edit_families.into_inner();
    assert_graph_square_census(units.len(), edits, &edit_families, compared);
}

#[test]
fn graph_square_falls_back_before_a_distinct_tree_refusal() {
    let stages = c11_stages();
    let stage = &stages[21];
    assert_eq!(stage.name, "C");
    let pair = Pair::from_stage(stage);
    let source = RefTree {
        nodes: BTreeMap::from([
            (0, RefNode::new(0, "a")),
            (1, RefNode::new(1, "a")),
            (2, RefNode::new(2, "b")),
        ]),
        arcs: vec![
            (
                0,
                2,
                panproto_reference::enumerate::lenses::c11_edge("a", "b"),
            ),
            (
                2,
                1,
                panproto_reference::enumerate::lenses::c11_edge("b", "a"),
            ),
        ],
        fans: Vec::new(),
        root: 0,
    };
    let edited = pair.with_reference(|lens| {
        let lens = lens.expect("stage C compiles");
        let (mut view, complement) =
            RefTreeShape::get(lens, &source, &()).expect("the contraction has a tree view");
        assert_eq!(complement.embedding.arcs.get(&1), Some(&2));
        view.nodes.insert(3, RefNode::new(3, "a"));
        view.arcs.push((
            0,
            3,
            panproto_reference::enumerate::lenses::c11_edge("a", "a"),
        ));

        let result =
            panproto_reference::lens::graph_put_square(lens, &source, &view, &complement.embedding);
        let SquareResult::Commutes(observation) = result else {
            panic!("graph-error fallback did not commute: {result:?}");
        };
        let expected = Some(Err(SquareError::PutDomain(
            panproto_reference::lens::PutDomainError::ContractedEdgeInserted {
                elem: panproto_reference::lens::ViewRef::Edge(3),
            },
        )));
        assert_eq!(observation.graph.put, expected);
        assert_eq!(observation.fast.put, expected);
        assert_eq!(
            observation.fast_tree_put,
            Some(Err(SquareError::PutDomain(
                panproto_reference::lens::PutDomainError::ContractedEdgeInserted {
                    elem: panproto_reference::lens::ViewRef::Edge(3),
                },
            )))
        );
        view
    });

    let production = production_tree_square(
        &pair,
        &TreeSquareCase {
            stage: 21,
            source,
            edited: Some(edited),
        },
    );
    let expected = Some(Err(SquareReason::PutDomain(
        "ContractedEdgeInserted Edge(3)".to_owned(),
    )));
    assert!(production.commutes);
    assert_eq!(production.graph.put, expected);
    assert_eq!(production.fast.expect("tree dispatch get").put, expected);
}

fn assert_randomized_edited_graph_square(case_index: usize, edit_index: usize) {
    let carrier = path_property_carrier();
    for offset in 0..carrier.tree_units.len() {
        let (stage, source) = carrier.tree_units[(case_index + offset) % carrier.tree_units.len()];
        let pair = Pair::from_stage(&carrier.stages[stage]);
        let tree = &carrier.trees[stage][source];
        let basis = pair.with_reference(|lens| {
            let lens = lens.ok()?;
            let (view, complement) = RefTreeShape::get(lens, tree, &()).ok()?;
            let basis = RefTreeShape::single_step_basis(lens, tree, &view, &complement, &());
            (!basis.is_empty()).then_some(basis)
        });
        let Some(basis) = basis else {
            continue;
        };
        let edited = basis[edit_index % basis.len()].view.clone();
        let square_case = TreeSquareCase {
            stage,
            source: tree.clone(),
            edited: Some(edited.clone()),
        };
        let production = production_tree_square(&pair, &square_case);
        let reference = pair.with_reference(|lens| {
            let lens = lens.expect("the selected edited-square plan compiles");
            let (_, complement) = RefTreeShape::get(lens, tree, &())
                .expect("the selected edited-square base has a successful get");
            reference_square(
                &panproto_reference::lens::graph_put_square(
                    lens,
                    tree,
                    &edited,
                    &complement.embedding,
                ),
                |observation| {
                    (
                        reference_leg(&observation.graph),
                        reference_leg(&observation.fast),
                    )
                },
            )
        });
        assert!(
            squares_agree(&production, &reference),
            "edited graph/tree square differs at stage {stage}, source {source}, edit {}",
            edit_index % basis.len()
        );
        return;
    }
    panic!("the bounded graph/tree-square carrier contains no genuine edited view");
}

proptest! {
    #![proptest_config(ProptestConfig::default())]
    #[test]
    fn randomized_graph_square_differential(
        case in any::<usize>(),
        edit in any::<usize>(),
    ) {
        assert_randomized_edited_graph_square(case, edit);
    }
}

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

#[test]
fn lens_equivalence_law_certificate() {
    let mut bases = [0_usize; 3];
    for_each_lens_equivalence_case(|case| match case {
        LensEquivalenceCase::Graph { .. } => bases[0] += 1,
        LensEquivalenceCase::Tree { .. } => bases[1] += 1,
        LensEquivalenceCase::Table(_) => bases[2] += 1,
    });
    assert_eq!(bases, [144_397, 2_548, 9_921]);
    graph_lens_agrees_with_the_reference_exhaustively();
    tree_lens_agrees_with_the_reference_exhaustively();
    table_laws_agree_with_the_reference_on_the_fixture_scope();
}

// ---------------------------------------------------------------------------
// Protolenses: precondition gating, schema instantiation and instance
// application against the reference's bounded protolens
// ---------------------------------------------------------------------------

use panproto_lens::protolens::elementary;
use panproto_reference::lens::{Protolens as RefProtolens, ProtolensError as RefProtolensError};

const L19_OVERLONG_EXPANSION_INTERMEDIATES: usize = 65_536;

struct CountingRejectingEnricher {
    calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    name: &'static str,
}

impl panproto_lens::enrichment_registry::LayoutEnricher for CountingRejectingEnricher {
    fn enrich(
        &self,
        _schema: &Schema,
        _policy: &panproto_gat::LayoutPolicySpec,
    ) -> Result<Schema, LensError> {
        self.calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Err(LensError::EnrichmentSynthesisFailed {
            kind: panproto_gat::EnrichmentKind::Layout,
            enricher: self.name.to_owned(),
            detail: "the L.19 precedence witness must not evaluate this transform".to_owned(),
        })
    }
}

struct OneShotSchemaEnricher {
    target: std::sync::Mutex<Option<Schema>>,
    calls: std::sync::atomic::AtomicUsize,
}

impl panproto_lens::enrichment_registry::LayoutEnricher for OneShotSchemaEnricher {
    fn enrich(
        &self,
        _schema: &Schema,
        _policy: &panproto_gat::LayoutPolicySpec,
    ) -> Result<Schema, LensError> {
        self.calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.target
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .ok_or_else(|| LensError::EnrichmentSynthesisFailed {
                kind: panproto_gat::EnrichmentKind::Layout,
                enricher: "l19-overlong-expansion".to_owned(),
                detail: "the one-shot L.19 schema driver was invoked twice".to_owned(),
            })
    }
}

fn l19_add_enrichment(
    name: &'static str,
    precondition: panproto_gat::TheoryConstraint,
) -> panproto_gat::TheoryEndofunctor {
    panproto_gat::TheoryEndofunctor {
        name: name.into(),
        precondition,
        transform: panproto_gat::TheoryTransform::AddEnrichment {
            kind: panproto_gat::EnrichmentKind::Layout,
            enricher: name.into(),
            policy: panproto_gat::LayoutPolicySpec::default(),
        },
    }
}

fn l19_expansion_source() -> Schema {
    SchemaBuilder::new(&Protocol::default())
        .vertex("a", "obj", None)
        .expect("open protocol accepts a")
        .vertex("b", "obj", None)
        .expect("open protocol accepts b")
        .edge("a", "b", "source", Some("ab"))
        .expect("the direct source edge is well formed")
        .build()
        .expect("the direct-edge source schema is valid")
}

fn l19_overlong_expansion_target() -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol)
        .vertex("a", "obj", None)
        .expect("open protocol accepts a")
        .vertex("b", "obj", None)
        .expect("open protocol accepts b");
    let mut previous = "a".to_owned();
    for index in 0..L19_OVERLONG_EXPANSION_INTERMEDIATES {
        let next = format!("m{index}");
        builder = builder
            .vertex(&next, "obj", None)
            .expect("every expansion intermediate has a fresh name")
            .edge(&previous, &next, "hop", Some("h"))
            .expect("each expansion hop joins declared vertices");
        previous = next;
    }
    builder
        .edge(&previous, "b", "hop", Some("h"))
        .expect("the final expansion hop reaches b")
        .build()
        .expect("the overlong expansion target is a valid schema")
}

/// The elementary protolens alphabet over the C11 schemas: drop and rename
/// the one vertex kind, and drop and rename one concrete edge name.
fn protolens_alphabet() -> Vec<panproto_lens::Protolens> {
    vec![
        elementary::drop_sort("obj"),
        elementary::rename_sort("obj", "thing"),
        elementary::drop_edge("a", "a", Some("e".into())),
        elementary::rename_edge_name("a", "a", "e", "g"),
    ]
}

fn reference_precondition(
    applies: bool,
    schema: &RefSchema,
) -> panproto_reference::lens::SchemaPrecondition {
    use panproto_reference::lens::SchemaPrecondition;
    if applies {
        SchemaPrecondition::Exact(panproto_reference::schema::canonical_bytes(schema))
    } else {
        SchemaPrecondition::HasVertices(["\u{0}unsatisfiable".to_owned()].into())
    }
}

fn reference_carrier_migration(target: &RefSchema) -> RefMigration {
    RefMigration {
        surviving_verts: target.vertices.keys().cloned().collect(),
        surviving_edges: target.edges.clone(),
        ..RefMigration::default()
    }
}

/// The four C11 protolenses constructed without consulting production's
/// applicability verdict, target schema, compiled migration or plan.
fn reference_protolens(index: usize, schema: &RefSchema) -> RefProtolens {
    match index {
        0 => {
            let applies = schema.vertices.values().any(|kind| kind == "obj");
            let mut target = schema.clone();
            let dropped = target
                .vertices
                .iter()
                .filter(|(id, kind)| id.as_str() == "obj" || kind.as_str() == "obj")
                .map(|(id, _)| id.clone())
                .collect::<std::collections::BTreeSet<_>>();
            target.vertices.retain(|id, _| !dropped.contains(id));
            target
                .edges
                .retain(|edge| !dropped.contains(&edge.src) && !dropped.contains(&edge.tgt));
            target.hyper_edges.retain(|_, hyper| {
                !hyper
                    .signature
                    .values()
                    .any(|vertex| dropped.contains(vertex))
            });
            let migration = reference_carrier_migration(&target);
            RefProtolens::new(reference_precondition(applies, schema), target, migration)
        }
        1 => {
            let applies = schema.vertices.values().any(|kind| kind == "obj");
            let mut target = schema.clone();
            for kind in target.vertices.values_mut() {
                if kind == "obj" {
                    "thing".clone_into(kind);
                }
            }
            let migration = reference_carrier_migration(&target);
            RefProtolens::new(reference_precondition(applies, schema), target, migration)
        }
        2 => {
            let applies = true;
            let mut target = schema.clone();
            target.edges.retain(|edge| {
                !(edge.src == "a" && edge.tgt == "a" && edge.name.as_deref() == Some("e"))
            });
            let migration = reference_carrier_migration(&target);
            RefProtolens::new(reference_precondition(applies, schema), target, migration)
        }
        3 => {
            let applies = true;
            let mut target = schema.clone();
            let mut edge_remap = BTreeMap::new();
            target.edges = schema
                .edges
                .iter()
                .map(|edge| {
                    if edge.src == "a" && edge.tgt == "a" && edge.name.as_deref() == Some("e") {
                        let mut renamed = edge.clone();
                        renamed.name = Some("g".to_owned());
                        edge_remap.insert(edge.clone(), renamed.clone());
                        renamed
                    } else {
                        edge.clone()
                    }
                })
                .collect();
            let mut migration = reference_carrier_migration(&target);
            for (source, target) in &edge_remap {
                migration.surviving_edges.insert(source.clone());
                migration.surviving_edges.insert(target.clone());
            }
            migration.edge_remap = edge_remap;
            RefProtolens::new(reference_precondition(applies, schema), target, migration)
        }
        _ => panic!("protolens index outside the four-operation alphabet: {index}"),
    }
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

fn assert_protolens_schema_agrees(
    index: usize,
    protolens: &panproto_lens::Protolens,
    schema: &RefSchema,
    protocol: &Protocol,
) -> bool {
    let production_schema = prod_schema(schema);
    let reference = reference_protolens(index, schema);
    let reference_applicable = reference.applicable_to(schema);
    assert_eq!(
        protolens.applicable_to(&production_schema),
        reference_applicable,
        "independent precondition at protolens {index} and schema {schema:?}"
    );
    let production = protolens.instantiate_checked(&production_schema, protocol);
    let reference_instantiated = panproto_reference::lens::protolens_schema(&reference, schema);
    match (production, reference_instantiated) {
        (Ok(production), Ok(reference_instantiated)) => {
            assert!(reference_applicable);
            assert_eq!(ref_schema(&production.src_schema), *schema);
            assert_eq!(ref_schema(&production.tgt_schema), reference.target);
            assert_eq!(
                semantic_migration(
                    schema,
                    &reference.target,
                    &ref_migration(&production.compiled),
                ),
                semantic_migration(schema, &reference.target, &reference.migration),
                "independently compiled migration semantics at protolens {index} and schema {schema:?}",
            );
            assert_eq!(reference_instantiated.source(), schema);
            assert_eq!(reference_instantiated.target(), &reference.target);
            true
        }
        (
            Err(LensError::ProtolensInapplicable { reasons }),
            Err(RefProtolensError::FalsePrecondition { .. }),
        ) => {
            assert!(!reference_applicable);
            assert!(
                !reasons.is_empty(),
                "a false production precondition must retain its failed clauses"
            );
            false
        }
        (Err(LensError::Restrict(production)), Err(RefProtolensError::Plan(reference))) => {
            assert!(reference_applicable);
            assert_eq!(
                restrict_reason(&production),
                ref_restrict_reason(&RefRestrictError::Plan(reference)),
                "plan-compilation refusal at protolens {index} and schema {schema:?}"
            );
            true
        }
        (production, reference) => panic!(
            "schema instantiation differs at protolens {index} and schema {schema:?}: production {production:?}, reference {reference:?}"
        ),
    }
}

fn checked_protolens_refusals_match_the_reference_with_exact_precedence() {
    static VERIFIED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    VERIFIED.get_or_init(|| {
        use std::sync::atomic::Ordering;

        use panproto_gat::TheoryConstraint;
        use panproto_lens::{ComplementConstructor, Protolens};
        use panproto_reference::lens::SchemaPrecondition;

        let source_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let target_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        panproto_lens::enrichment_registry::register_enricher(
            panproto_gat::EnrichmentKind::Layout,
            "l19-source-must-not-run",
            std::sync::Arc::new(CountingRejectingEnricher {
                calls: std::sync::Arc::clone(&source_calls),
                name: "l19-source-must-not-run",
            }),
        );
        panproto_lens::enrichment_registry::register_enricher(
            panproto_gat::EnrichmentKind::Layout,
            "l19-target-must-not-run",
            std::sync::Arc::new(CountingRejectingEnricher {
                calls: std::sync::Arc::clone(&target_calls),
                name: "l19-target-must-not-run",
            }),
        );
        let false_precondition = TheoryConstraint::All(vec![
            TheoryConstraint::HasSort("missing-first".into()),
            TheoryConstraint::HasOp("missing-second".into()),
        ]);
        let inapplicable = Protolens {
            name: "l19-false-precondition".into(),
            source: l19_add_enrichment("l19-source-must-not-run", false_precondition),
            target: l19_add_enrichment("l19-target-must-not-run", TheoryConstraint::Unconstrained),
            complement_constructor: ComplementConstructor::Empty,
        };
        let empty_schema = Schema::default();
        let expected_reasons = vec![
            "Schema has no vertex of kind 'missing-first'.".to_owned(),
            "Schema has no edge of kind 'missing-second'.".to_owned(),
        ];
        assert!(matches!(
            inapplicable.instantiate_checked(&empty_schema, &Protocol::default()),
            Err(LensError::ProtolensInapplicable { reasons }) if reasons == expected_reasons
        ));
        assert_eq!(source_calls.load(Ordering::Relaxed), 0);
        assert_eq!(target_calls.load(Ordering::Relaxed), 0);

        let reference_empty = RefSchema::default();
        let missing_edge = RefSEdge::new("missing", "missing", "missing-second", None);
        let reference_precondition = SchemaPrecondition::All(vec![
            SchemaPrecondition::HasVertices(["missing-first".to_owned()].into()),
            SchemaPrecondition::HasEdges([missing_edge].into()),
        ]);
        let reference_inapplicable = RefProtolens::new(
            reference_precondition.clone(),
            RefSchema::default(),
            RefMigration::default(),
        );
        assert_eq!(
            panproto_reference::lens::protolens_schema(&reference_inapplicable, &reference_empty),
            Err(RefProtolensError::FalsePrecondition {
                precondition: reference_precondition,
                found_schema: panproto_reference::schema::canonical_bytes(&reference_empty),
            })
        );

        assert_overlong_protolens_expansion_refusal();
    });
}

fn assert_overlong_protolens_expansion_refusal() {
    use std::sync::atomic::Ordering;

    use panproto_gat::{TheoryConstraint, TheoryEndofunctor, TheoryTransform};
    use panproto_lens::{ComplementConstructor, Protolens};

    let source = l19_expansion_source();
    let target = l19_overlong_expansion_target();
    assert_eq!(source.vertices.len(), 2);
    assert_eq!(source.edges.len(), 1);
    assert_eq!(
        target.vertices.len(),
        L19_OVERLONG_EXPANSION_INTERMEDIATES + 2
    );
    assert_eq!(target.edges.len(), L19_OVERLONG_EXPANSION_INTERMEDIATES + 1);
    let reference_source = ref_schema(&source);
    let reference_target = ref_schema(&target);
    let overlong_driver = std::sync::Arc::new(OneShotSchemaEnricher {
        target: std::sync::Mutex::new(Some(target)),
        calls: std::sync::atomic::AtomicUsize::new(0),
    });
    panproto_lens::enrichment_registry::register_enricher(
        panproto_gat::EnrichmentKind::Layout,
        "l19-overlong-expansion",
        overlong_driver.clone(),
    );
    let overlong = Protolens {
        name: "l19-overlong-expansion".into(),
        source: TheoryEndofunctor {
            name: "identity".into(),
            precondition: TheoryConstraint::Unconstrained,
            transform: TheoryTransform::Identity,
        },
        target: l19_add_enrichment("l19-overlong-expansion", TheoryConstraint::Unconstrained),
        complement_constructor: ComplementConstructor::Empty,
    };
    let production_error = overlong
        .instantiate_checked(&source, &Protocol::default())
        .expect_err("a 65537-hop expansion cannot be numbered by a synthesized key");
    let LensError::Restrict(production_plan_error) = production_error else {
        panic!("checked instantiation returned the wrong refusal: {production_error:?}");
    };
    assert!(matches!(
        &production_plan_error,
        RestrictError::ExpansionTooLong { src, tgt, hops }
            if src == "a" && tgt == "b" && *hops == 65_537
    ));
    assert_eq!(overlong_driver.calls.load(Ordering::Relaxed), 1);

    let reference_migration = RefMigration {
        surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
        expansion_path: [(
            ("a".to_owned(), "b".to_owned()),
            (0..L19_OVERLONG_EXPANSION_INTERMEDIATES)
                .map(|index| format!("m{index}"))
                .collect(),
        )]
        .into(),
        ..RefMigration::default()
    };
    let reference_overlong =
        RefProtolens::exact(&reference_source, &reference_target, &reference_migration);
    let reference_error =
        panproto_reference::lens::protolens_schema(&reference_overlong, &reference_source)
            .expect_err("the independent compiler must refuse the same overlong expansion");
    let RefProtolensError::Plan(reference_plan_error) = reference_error else {
        panic!("reference instantiation returned the wrong refusal: {reference_error:?}");
    };
    assert_eq!(
        reference_plan_error,
        PlanError::ExpansionTooLong {
            src: "a".to_owned(),
            tgt: "b".to_owned(),
            hops: 65_537,
        }
    );
    let production_reason = restrict_reason(&production_plan_error);
    let reference_reason = ref_restrict_reason(&RefRestrictError::Plan(reference_plan_error));
    assert_eq!(production_reason, "ExpansionTooLong a b 65537");
    assert_eq!(production_reason, reference_reason);
    println!(
        "L.19 refusals: candidates=2, false-precondition clauses=2, transform calls before false-precondition refusal=0, plan refusals=1, expansion intermediates=65536, expansion hops=65537"
    );
}

/// The observable outcome of applying one protolens at one schema to one
/// source: applicability, then the view and complement or the refusal.
#[derive(Debug, PartialEq, Eq)]
enum ProtolensOutcome {
    Inapplicable,
    Refused(String),
    Applied(RefGraph, Box<RefComplement>, Box<SemanticEnvelope>),
}

/// Cross-implementation envelope semantics. Raw digests deliberately stay
/// local to their canonical encodings; the differential compares the exact
/// boundaries, migration, shape and complement format they certify.
#[derive(Debug, PartialEq, Eq)]
struct SemanticEnvelope {
    source: RefSchema,
    target: RefSchema,
    migration: SemanticMigration,
    shape: RefComplementShape,
    format_version: u16,
}

/// The observable relation compiled from a migration. Syntactic resolver
/// entries are absent: a resolver that repeats the unique target edge has
/// the same edge fates and contraction resolutions as the omitted entry.
type SemanticHyperResolver =
    BTreeMap<(String, BTreeSet<String>), (String, BTreeMap<String, String>)>;

#[derive(Debug, PartialEq, Eq)]
struct SemanticMigration {
    vertices: BTreeMap<String, Option<String>>,
    edges: BTreeMap<RefSEdge, panproto_reference::restrict::EdgeFate>,
    contractions: BTreeMap<(String, String), panproto_reference::restrict::PlannedPair>,
    hyper_resolver: SemanticHyperResolver,
    conditional: std::collections::BTreeSet<String>,
    transformed: std::collections::BTreeSet<String>,
}

fn semantic_migration(
    source: &RefSchema,
    target: &RefSchema,
    migration: &RefMigration,
) -> SemanticMigration {
    let plan = panproto_reference::restrict::compile(source, target, migration)
        .expect("an instantiated protolens has a compiled migration");
    SemanticMigration {
        vertices: source
            .vertices
            .keys()
            .map(|vertex| {
                (
                    vertex.clone(),
                    migration
                        .survives(vertex)
                        .then(|| migration.vertex_image(vertex).to_owned()),
                )
            })
            .collect(),
        edges: plan.fates().clone(),
        contractions: plan.pairs().clone(),
        hyper_resolver: migration.hyper_resolver.clone(),
        conditional: migration.conditional.clone(),
        transformed: migration.transformed.clone(),
    }
}

fn production_semantic_envelope(
    lens: &Lens,
    identity: &ProductionIdentityEnvelope,
) -> SemanticEnvelope {
    let source = ref_schema(&lens.src_schema);
    let target = ref_schema(&lens.tgt_schema);
    let migration = ref_migration(&lens.compiled);
    SemanticEnvelope {
        migration: semantic_migration(&source, &target, &migration),
        source,
        target,
        shape: ref_identity(identity).shape,
        format_version: identity.complement_format.version,
    }
}

fn reference_semantic_envelope(
    source: &RefSchema,
    target: &RefSchema,
    migration: &RefMigration,
    identity: &RefIdentityEnvelope,
) -> SemanticEnvelope {
    SemanticEnvelope {
        source: source.clone(),
        target: target.clone(),
        migration: semantic_migration(source, target, migration),
        shape: identity.shape,
        format_version: identity.format_version,
    }
}

#[test]
fn a_redundant_unique_edge_resolver_has_the_same_semantic_migration() {
    let schema = panproto_reference::enumerate::lenses::c11_schema(&["a"], &[("a", "a")]);
    let plain = reference_carrier_migration(&schema);
    let mut redundant = plain.clone();
    redundant.resolver.insert(
        ("a".to_owned(), "a".to_owned()),
        panproto_reference::enumerate::lenses::c11_edge("a", "a"),
    );
    assert_ne!(plain, redundant, "the syntax differs at the resolver");
    assert_eq!(
        semantic_migration(&schema, &schema, &plain),
        semantic_migration(&schema, &schema, &redundant),
        "the unique target edge gives the same fate and pair resolution",
    );
}

fn assert_protolens_schema_scope(
    alphabet: &[panproto_lens::Protolens],
    schemas: &[RefSchema],
    protocol: &Protocol,
) -> usize {
    let mut certificate_cases = 0;
    let mut elementary_cases = 0;
    let mut false_preconditions = 0;
    let mut expansion_refusals = 0;
    for_each_protolens_schema_case(|case| {
        certificate_cases += 1;
        match case {
            ProtolensSchemaCase::Elementary { protolens, schema } => {
                elementary_cases += 1;
                assert_protolens_schema_agrees(
                    protolens,
                    &alphabet[protolens],
                    &schemas[schema],
                    protocol,
                );
            }
            ProtolensSchemaCase::FalsePrecondition => false_preconditions += 1,
            ProtolensSchemaCase::ExpansionTooLong => expansion_refusals += 1,
        }
    });
    assert_eq!(certificate_cases, protolens_schema_case_count_certificate());
    assert_eq!(elementary_cases, 64);
    assert_eq!(false_preconditions, 1);
    assert_eq!(expansion_refusals, 1);
    checked_protolens_refusals_match_the_reference_with_exact_precedence();
    certificate_cases
}

fn protolens_instance_bound(
    protolenses: usize,
    schemas: usize,
    schema_cases: usize,
    certificate_cases: usize,
    state_cases: usize,
) -> String {
    let bound = format!(
        "C11 protolenses: protolenses={protolenses}, schemas={schemas}, schema cases={schema_cases}, L.19 certificate cases={certificate_cases}, (protolens, schema, state) cases={state_cases}"
    );
    println!("{bound}");
    bound
}

fn protolens_state_units(
    protolenses: usize,
    states: &[Vec<RefGraph>],
) -> Vec<(usize, usize, usize)> {
    (0..protolenses)
        .flat_map(|protolens| {
            states
                .iter()
                .enumerate()
                .flat_map(move |(schema, sources)| {
                    (0..sources.len()).map(move |state| (protolens, schema, state))
                })
        })
        .collect()
}

#[test]
fn protolens_transforms_agree_with_the_reference_exhaustively() {
    let alphabet = protolens_alphabet();
    let schemas = sigma2_schemas();
    let protocol = Protocol::default();
    let certificate_cases = assert_protolens_schema_scope(&alphabet, &schemas, &protocol);

    let states: Vec<Vec<RefGraph>> = schemas.iter().map(c11_graph_states).collect();
    let units = protolens_state_units(alphabet.len(), &states);
    let schema_cases = alphabet.len() * schemas.len();
    assert_eq!(schema_cases, 64);
    assert_eq!(
        units.len(),
        alphabet.len() * schemas.iter().map(c11_graph_state_count).sum::<usize>()
    );
    assert_eq!(units.len(), 464_832);
    let bound = protolens_instance_bound(
        alphabet.len(),
        schemas.len(),
        schema_cases,
        certificate_cases,
        units.len(),
    );
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &units,
        || (Protocol::default(), protolens_alphabet()),
        |_, (index, schema, state)| vec![(*index, &schemas[*schema], &states[*schema][*state])],
        |(protocol, alphabet), (index, schema, source)| {
            let production_schema = prod_schema(schema);
            let applicable =
                assert_protolens_schema_agrees(*index, &alphabet[*index], schema, protocol);
            match panproto_lens::graph_protolens::apply(
                &alphabet[*index],
                &production_schema,
                protocol,
                &prod_graph(source),
            ) {
                Ok(applied) => {
                    assert!(applicable);
                    let identity = applied.lens.identity(ComplementShape::Graph);
                    assert_eq!(applied.complement.identity.as_ref(), Some(&identity));
                    ProtolensOutcome::Applied(
                        ref_graph(&applied.view),
                        Box::new(ref_complement(&applied.complement)),
                        Box::new(production_semantic_envelope(&applied.lens, &identity)),
                    )
                }
                Err(LensError::ProtolensInapplicable { .. }) if !applicable => {
                    ProtolensOutcome::Inapplicable
                }
                Err(error) => ProtolensOutcome::Refused(format!("{:?}", lens_reason(&error))),
            }
        },
        |_, (index, schema, source)| {
            let reference = reference_protolens(*index, schema);
            let instantiated = panproto_reference::lens::protolens_schema(&reference, schema);
            assert_eq!(
                instantiated.as_ref().map(|lens| lens.source().clone()).ok(),
                reference
                    .applicable_to(schema)
                    .then(|| (*schema).clone())
                    .filter(|_| instantiated.is_ok()),
                "an instantiated reference protolens is bound to exactly its schema"
            );
            match panproto_reference::lens::protolens_instance(&reference, schema, source) {
                Ok(application) => {
                    let identity = *application
                        .lens
                        .as_graph_lens()
                        .expect("an instantiated reference graph lens")
                        .identity();
                    assert_eq!(application.complement.identity, Some(identity));
                    ProtolensOutcome::Applied(
                        application.view,
                        Box::new(without_identity(&application.complement)),
                        Box::new(reference_semantic_envelope(
                            schema,
                            &reference.target,
                            &reference.migration,
                            &identity,
                        )),
                    )
                }
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

fn assert_randomized_protolens_schema_case(case_index: usize) {
    let mut cases = Vec::new();
    for_each_protolens_schema_case(|case| cases.push(case));
    assert_eq!(cases.len(), 66);
    match cases[case_index % cases.len()] {
        ProtolensSchemaCase::Elementary { protolens, schema } => {
            let alphabet = protolens_alphabet();
            let schemas = sigma2_schemas();
            assert_protolens_schema_agrees(
                protolens,
                &alphabet[protolens],
                &schemas[schema],
                &Protocol::default(),
            );
        }
        ProtolensSchemaCase::FalsePrecondition | ProtolensSchemaCase::ExpansionTooLong => {
            checked_protolens_refusals_match_the_reference_with_exact_precedence();
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::default())]
    #[test]
    fn randomized_protolens_schema_differential(rotation in 0_usize..66) {
        for offset in 0..protolens_schema_case_count_certificate() {
            assert_randomized_protolens_schema_case(rotation + offset);
        }
    }
}

/// The law outcome recorded on both sides for a tree outside the reference
/// tree lens's domain, where the tree law report is not compared.
fn outside_the_tree_domain() -> LawOutcome {
    LawOutcome {
        first_failure: Some("OutsideTreeDomain".to_owned()),
    }
}

/// The per-thread context of the L.8 differential: the C11 lens pairs, the
/// table lens pairs and the partial-lens carrier.
type LawContext = (Vec<Pair>, Vec<Vec<TablePair>>, Lens);

// ---------------------------------------------------------------------------
// Admission: both put-domain predicates on every Single-Step Basis candidate
// ---------------------------------------------------------------------------

use panproto_reference::lens::{
    DomainExpectation, GraphShape as RefGraphShape, LawShape as RefLawShape,
    Mutation as RefMutation, TableShape as RefTableShape, TreeShape as RefTreeShape,
};

/// The complete normalized table-domain outcome. Named collations remain
/// alongside known violations: retaining one must never erase the other.
#[derive(Clone, Debug, PartialEq, Eq)]
enum TableDomainVerdict {
    Admitted,
    Refused {
        violations: Vec<String>,
        unresolved_collations: BTreeSet<String>,
    },
    Operational(String),
}

/// The raw table put's result. This is intentionally separate from the
/// complete checker report because raw put returns only its first known
/// violation, before any unresolved named-collation result.
#[derive(Clone, Debug, PartialEq, Eq)]
enum TableRawVerdict {
    Admitted,
    Domain(String),
    NamedCollationUnresolved(BTreeSet<String>),
    Operational(String),
}

/// One candidate's admission. Graph and tree candidates carry their complete
/// ordered refusal vectors. Table candidates additionally carry raw-put
/// precedence and unresolved-collation evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Admission {
    Admitted,
    Refused(Vec<String>),
    Table {
        domain: TableDomainVerdict,
        raw: TableRawVerdict,
    },
}

impl Admission {
    const fn is_admitted(&self) -> bool {
        matches!(
            self,
            Self::Admitted
                | Self::Table {
                    domain: TableDomainVerdict::Admitted,
                    raw: TableRawVerdict::Admitted,
                }
        )
    }
}

/// The admission census of one source: each candidate's verdict, in the
/// order the reference basis and then the production basis list them, and
/// the number of positive reference-basis candidates admitted.
#[derive(Debug, PartialEq, Eq)]
struct AdmissionCensus {
    verdicts: Vec<Admission>,
    admitted: usize,
}

/// One Single-Step Basis candidate in reference coordinates, and whether it
/// is a positive candidate of the reference basis.
struct Candidate<I> {
    view: I,
    positive_reference: bool,
}

/// The positive reference-basis candidates a verdict list admits: the
/// in-domain mutations of the reference law report, counted from each side's
/// own predicate.
fn admitted_positive<I>(candidates: &[Candidate<I>], verdicts: &[Admission]) -> usize {
    candidates
        .iter()
        .zip(verdicts)
        .filter(|(candidate, verdict)| candidate.positive_reference && verdict.is_admitted())
        .count()
}

/// Every candidate of both graph bases at `source`: the reference basis,
/// then the production basis.
fn graph_candidates(pair: &Pair, source: &RefGraph) -> Vec<Candidate<RefGraph>> {
    let mut out = Vec::new();
    pair.with_reference(|lens| {
        if let Ok(lens) = lens
            && let Ok((view, complement)) = RefGraphShape::get(lens, source, &())
        {
            out.extend(
                RefGraphShape::single_step_basis(lens, source, &view, &complement, &())
                    .into_iter()
                    .map(|case| Candidate {
                        positive_reference: matches!(case.domain, DomainExpectation::Admitted),
                        view: case.view,
                    }),
            );
        }
    });
    let production_source = prod_graph(source);
    if let Ok((view, complement)) = graph_lens::get(&pair.production, &production_source) {
        for mutation in Mutation::ALL {
            match <panproto_lens::GraphShape as LawShape>::mutation_cases(
                &pair.production,
                &production_source,
                &view,
                &complement,
                &(),
                mutation,
            ) {
                MutationCases::Cases(views) => out.extend(views.iter().map(|view| Candidate {
                    view: ref_graph(view),
                    positive_reference: false,
                })),
                MutationCases::Skip(_) => {}
            }
            out.extend(
                <panproto_lens::GraphShape as LawShape>::negative_mutation_cases(
                    &pair.production,
                    &production_source,
                    &view,
                    &complement,
                    &(),
                    mutation,
                )
                .iter()
                .map(|case| Candidate {
                    view: ref_graph(&case.view),
                    positive_reference: false,
                }),
            );
        }
    }
    out
}

/// Production's admission census of a graph source, `None` when get refuses.
fn production_graph_admissions(
    pair: &Pair,
    source: &RefGraph,
    candidates: &[Candidate<RefGraph>],
) -> Option<AdmissionCensus> {
    let (_, complement) = graph_lens::get(&pair.production, &prod_graph(source)).ok()?;
    let verdicts: Vec<Admission> = candidates
        .iter()
        .map(|candidate| {
            graph_lens::check_put_domain(
                &pair.production,
                &prod_graph(&candidate.view),
                &complement,
            )
            .map_or_else(
                |errors| Admission::Refused(errors.iter().map(put_domain_reason).collect()),
                |()| Admission::Admitted,
            )
        })
        .collect();
    let admitted = admitted_positive(candidates, &verdicts);
    Some(AdmissionCensus { verdicts, admitted })
}

/// The reference's admission census of a graph source; `None` when get
/// refuses.
fn reference_graph_admissions(
    pair: &Pair,
    source: &RefGraph,
    candidates: &[Candidate<RefGraph>],
) -> Option<AdmissionCensus> {
    pair.with_reference(|lens| {
        let lens = lens.ok()?;
        let (_, complement) = panproto_reference::lens::get(lens, source).ok()?;
        let verdicts: Vec<Admission> = candidates
            .iter()
            .map(|candidate| {
                RefGraphLens::check_put_domain(lens, &candidate.view, &complement).map_or_else(
                    |errors| Admission::Refused(errors.iter().map(ref_put_domain_reason).collect()),
                    |()| Admission::Admitted,
                )
            })
            .collect();
        let admitted = admitted_positive(candidates, &verdicts);
        Some(AdmissionCensus { verdicts, admitted })
    })
}

/// Every candidate of both tree bases at `source`: the reference basis,
/// then the production basis.
fn tree_candidates(pair: &Pair, source: &RefTree) -> Vec<Candidate<RefTree>> {
    let mut out = Vec::new();
    pair.with_reference(|lens| {
        if let Ok(lens) = lens
            && let Ok((view, complement)) = RefTreeShape::get(lens, source, &())
        {
            out.extend(
                RefTreeShape::single_step_basis(lens, source, &view, &complement, &())
                    .into_iter()
                    .map(|case| Candidate {
                        positive_reference: matches!(case.domain, DomainExpectation::Admitted),
                        view: case.view,
                    }),
            );
        }
    });
    let production_source = prod_tree(source);
    if let Ok((view, complement)) = panproto_lens::get(&pair.production, &production_source) {
        for mutation in Mutation::ALL {
            match <panproto_lens::TreeShape as LawShape>::mutation_cases(
                &pair.production,
                &production_source,
                &view,
                &complement,
                &(),
                mutation,
            ) {
                MutationCases::Cases(views) => out.extend(views.iter().map(|view| Candidate {
                    view: ref_tree(view),
                    positive_reference: false,
                })),
                MutationCases::Skip(_) => {}
            }
            out.extend(
                <panproto_lens::TreeShape as LawShape>::negative_mutation_cases(
                    &pair.production,
                    &production_source,
                    &view,
                    &complement,
                    &(),
                    mutation,
                )
                .iter()
                .map(|case| Candidate {
                    view: ref_tree(&case.view),
                    positive_reference: false,
                }),
            );
        }
    }
    out
}

/// Production's admission census of a tree source, `None` when get refuses.
fn production_tree_admissions(
    pair: &Pair,
    source: &RefTree,
    candidates: &[Candidate<RefTree>],
) -> Option<AdmissionCensus> {
    let (_, complement) = panproto_lens::get(&pair.production, &prod_tree(source)).ok()?;
    let verdicts: Vec<Admission> = candidates
        .iter()
        .map(|candidate| {
            panproto_lens::asymmetric::check_put_domain(
                &pair.production,
                &prod_tree(&candidate.view),
                &complement,
            )
            .map_or_else(
                |errors| Admission::Refused(errors.iter().map(put_domain_reason).collect()),
                |()| Admission::Admitted,
            )
        })
        .collect();
    let admitted = admitted_positive(candidates, &verdicts);
    Some(AdmissionCensus { verdicts, admitted })
}

/// The reference's admission census of a tree source; `None` when get
/// refuses.
fn reference_tree_admissions(
    pair: &Pair,
    source: &RefTree,
    candidates: &[Candidate<RefTree>],
) -> Option<AdmissionCensus> {
    pair.with_reference(|lens| {
        let lens = lens.ok()?;
        let (_, complement) = RefTreeShape::get(lens, source, &()).ok()?;
        let verdicts: Vec<Admission> = candidates
            .iter()
            .map(|candidate| {
                RefTreeShape::check_put_domain(lens, &candidate.view, &complement, &()).map_or_else(
                    |error| match error {
                        panproto_reference::lens::ReferenceLawError::PutDomain(errors) => {
                            Admission::Refused(errors.iter().map(ref_put_domain_reason).collect())
                        }
                        other => Admission::Refused(vec![format!("{other:?}")]),
                    },
                    |()| Admission::Admitted,
                )
            })
            .collect();
        let admitted = admitted_positive(candidates, &verdicts);
        Some(AdmissionCensus { verdicts, admitted })
    })
}

/// One table candidate in reference coordinates. A candidate may be shared
/// by the two independently generated bases, so origin flags and negative
/// families are merged when its exact table presentation is deduplicated.
struct TableCandidate {
    view: RefTable,
    positive_reference: bool,
    negative_families: BTreeSet<usize>,
}

fn push_table_candidate(
    candidates: &mut Vec<TableCandidate>,
    view: RefTable,
    positive_reference: bool,
    negative_family: Option<usize>,
) {
    if let Some(candidate) = candidates
        .iter_mut()
        .find(|candidate| candidate.view == view)
    {
        candidate.positive_reference |= positive_reference;
        if let Some(family) = negative_family {
            candidate.negative_families.insert(family);
        }
        return;
    }
    candidates.push(TableCandidate {
        view,
        positive_reference,
        negative_families: negative_family.into_iter().collect(),
    });
}

/// The deterministic union of the independent reference and production table
/// bases. Positive candidates are retained only when their own side's exact
/// predicate admits them; deliberately negative candidates are retained
/// unconditionally so that the differential checks their full refusal.
fn table_candidates(pair: &TablePair, source: &RefTable) -> Vec<TableCandidate> {
    let mut candidates = Vec::new();
    pair.with_reference(|lens, _| {
        if let Ok((view, complement)) = RefTableShape::get(lens, source, &()) {
            for case in RefTableShape::single_step_basis(lens, source, &view, &complement, &()) {
                match case.domain {
                    DomainExpectation::Admitted => {
                        if RefTableShape::check_put_domain(lens, &case.view, &complement, &())
                            .is_ok()
                        {
                            push_table_candidate(&mut candidates, case.view, true, None);
                        }
                    }
                    DomainExpectation::Refused(_) => push_table_candidate(
                        &mut candidates,
                        case.view,
                        false,
                        Some(ref_mutation_index(case.mutation)),
                    ),
                }
            }
        }
    });

    let production_source = prod_table(source);
    if let Ok((view, complement)) =
        table_lens::get(&pair.production, &production_source, pair.protocols())
    {
        let context = panproto_lens::TableContext {
            src: pair.protocol.clone(),
            tgt: pair.protocol.clone(),
        };
        for mutation in Mutation::ALL {
            match <panproto_lens::TableShape as LawShape>::mutation_cases(
                &pair.production,
                &production_source,
                &view,
                &complement,
                &context,
                mutation,
            ) {
                MutationCases::Cases(views) => {
                    for candidate in views {
                        if table_lens::check_put_domain(
                            &pair.production,
                            &candidate,
                            &complement,
                            pair.protocols(),
                        )
                        .is_ok()
                        {
                            push_table_candidate(
                                &mut candidates,
                                ref_table(&candidate),
                                false,
                                None,
                            );
                        }
                    }
                }
                MutationCases::Skip(_) => {}
            }
            let family = Mutation::ALL
                .iter()
                .position(|candidate| *candidate == mutation)
                .expect("Mutation::ALL contains the iterated mutation");
            for case in <panproto_lens::TableShape as LawShape>::negative_mutation_cases(
                &pair.production,
                &production_source,
                &view,
                &complement,
                &context,
                mutation,
            ) {
                push_table_candidate(&mut candidates, ref_table(&case.view), false, Some(family));
            }
        }
    }
    candidates
}

fn production_table_domain_verdict(result: Result<(), Vec<PutDomainError>>) -> TableDomainVerdict {
    result.map_or_else(
        |violations| TableDomainVerdict::Refused {
            violations: violations.iter().map(put_domain_reason).collect(),
            unresolved_collations: BTreeSet::new(),
        },
        |()| TableDomainVerdict::Admitted,
    )
}

fn reference_table_domain_verdict(result: Result<(), TableCheckError>) -> TableDomainVerdict {
    match result {
        Ok(()) => TableDomainVerdict::Admitted,
        Err(TableCheckError::Domain {
            violations,
            unresolved_collations,
        }) => TableDomainVerdict::Refused {
            violations: violations.iter().map(ref_put_domain_reason).collect(),
            unresolved_collations,
        },
        Err(TableCheckError::NamedCollationUnresolved { names }) => TableDomainVerdict::Refused {
            violations: Vec::new(),
            unresolved_collations: names,
        },
        Err(TableCheckError::Operational(error)) => {
            TableDomainVerdict::Operational(format!("{error:?}"))
        }
    }
}

fn production_table_raw_verdict(result: Result<FInstance, LensError>) -> TableRawVerdict {
    match result {
        Ok(_) => TableRawVerdict::Admitted,
        Err(LensError::PutDomain(error)) => {
            TableRawVerdict::Domain(put_domain_reason(error.as_ref()))
        }
        Err(error) => TableRawVerdict::Operational(format!("{error:?}")),
    }
}

fn reference_table_raw_verdict(result: Result<RefTable, TablePutError>) -> TableRawVerdict {
    match result {
        Ok(_) => TableRawVerdict::Admitted,
        Err(TablePutError::Domain(error)) => TableRawVerdict::Domain(ref_put_domain_reason(&error)),
        Err(TablePutError::NamedCollationUnresolved { names }) => {
            TableRawVerdict::NamedCollationUnresolved(names)
        }
        Err(error) => TableRawVerdict::Operational(format!("{error:?}")),
    }
}

fn production_table_admissions(
    pair: &TablePair,
    source: &RefTable,
    candidates: &[TableCandidate],
) -> Option<AdmissionCensus> {
    let (_, complement) =
        table_lens::get(&pair.production, &prod_table(source), pair.protocols()).ok()?;
    let verdicts: Vec<_> = candidates
        .iter()
        .map(|candidate| {
            let view = prod_table(&candidate.view);
            Admission::Table {
                domain: production_table_domain_verdict(table_lens::check_put_domain(
                    &pair.production,
                    &view,
                    &complement,
                    pair.protocols(),
                )),
                raw: production_table_raw_verdict(table_lens::put(
                    &pair.production,
                    &view,
                    &complement,
                    pair.protocols(),
                )),
            }
        })
        .collect();
    let admitted = candidates
        .iter()
        .zip(&verdicts)
        .filter(|(candidate, verdict)| candidate.positive_reference && verdict.is_admitted())
        .count();
    Some(AdmissionCensus { verdicts, admitted })
}

fn reference_table_admissions(
    pair: &TablePair,
    source: &RefTable,
    candidates: &[TableCandidate],
) -> Option<AdmissionCensus> {
    pair.with_reference(|lens, _| {
        let (_, complement) = lens.get(source).ok()?;
        let verdicts: Vec<_> = candidates
            .iter()
            .map(|candidate| Admission::Table {
                domain: reference_table_domain_verdict(
                    panproto_reference::table::check_put_domain(lens, &candidate.view, &complement),
                ),
                raw: reference_table_raw_verdict(RefTableLens::put(
                    lens,
                    &candidate.view,
                    &complement,
                )),
            })
            .collect();
        let admitted = candidates
            .iter()
            .zip(&verdicts)
            .filter(|(candidate, verdict)| candidate.positive_reference && verdict.is_admitted())
            .count();
        Some(AdmissionCensus { verdicts, admitted })
    })
}

fn assert_observed_table_negative(mutation: Mutation, verdict: &Admission) {
    let Admission::Table {
        domain:
            TableDomainVerdict::Refused {
                violations,
                unresolved_collations: _,
            },
        raw: TableRawVerdict::Domain(raw),
    } = verdict
    else {
        panic!(
            "declared table negative family {} was not refused by both the complete checker and raw put: {verdict:?}",
            mutation.as_str()
        );
    };
    let first = violations.first().unwrap_or_else(|| {
        panic!(
            "declared table negative family {} had no known domain violation",
            mutation.as_str()
        )
    });
    assert_eq!(
        raw,
        first,
        "declared table negative family {} raw put did not return the first ordered violation",
        mutation.as_str()
    );
}

#[test]
fn table_admission_normalization_preserves_complete_reports_and_raw_precedence() {
    use panproto_reference::lens::{PutDomainError as RefPutDomainError, ViewRef as RefViewRef};

    let collision = RefPutDomainError::IdCollision {
        elem: RefViewRef::Row(2),
    };
    let synthesized = RefPutDomainError::SynthesizedEdited {
        elem: RefViewRef::Row(5),
    };
    let unresolved = BTreeSet::from(["case_fold".to_owned(), "locale".to_owned()]);
    let complete = reference_table_domain_verdict(Err(TableCheckError::Domain {
        violations: vec![collision.clone(), synthesized.clone()],
        unresolved_collations: unresolved.clone(),
    }));
    assert_eq!(
        complete,
        TableDomainVerdict::Refused {
            violations: vec![
                ref_put_domain_reason(&collision),
                ref_put_domain_reason(&synthesized),
            ],
            unresolved_collations: unresolved.clone(),
        },
        "a complete table report retains both ordered violations and unresolved collations"
    );
    assert_eq!(
        reference_table_raw_verdict(Err(TablePutError::Domain(collision.clone()))),
        TableRawVerdict::Domain(ref_put_domain_reason(&collision)),
        "raw put returns the first known violation before unresolved collations"
    );
    assert_eq!(
        reference_table_domain_verdict(Err(TableCheckError::NamedCollationUnresolved {
            names: unresolved.clone(),
        })),
        TableDomainVerdict::Refused {
            violations: Vec::new(),
            unresolved_collations: unresolved.clone(),
        },
        "an unresolved-only checker result is not mistaken for admission"
    );
    assert_eq!(
        reference_table_raw_verdict(Err(TablePutError::NamedCollationUnresolved {
            names: unresolved.clone(),
        })),
        TableRawVerdict::NamedCollationUnresolved(unresolved)
    );

    let report = |violations| {
        reference_table_domain_verdict(Err(TableCheckError::Domain {
            violations,
            unresolved_collations: BTreeSet::new(),
        }))
    };
    let exact = report(vec![collision.clone(), synthesized.clone()]);
    assert_ne!(exact, report(vec![collision.clone()]), "missing refusal");
    assert_ne!(
        exact,
        report(vec![
            collision.clone(),
            synthesized.clone(),
            RefPutDomainError::ContractedEdgeInserted {
                elem: RefViewRef::Row(7),
            },
        ]),
        "extra refusal"
    );
    assert_ne!(
        exact,
        report(vec![synthesized.clone(), collision.clone()]),
        "reordered refusal"
    );
    assert_ne!(
        exact,
        report(vec![collision.clone(), collision]),
        "duplicate refusal"
    );
    assert_ne!(
        exact,
        report(vec![
            RefPutDomainError::IdCollision {
                elem: RefViewRef::Row(3),
            },
            synthesized,
        ]),
        "wrong-coordinate refusal"
    );
}

fn table_admission_fixture_census(pair: &TablePair, source: &RefTable) -> [usize; EDIT_FAMILIES] {
    let candidates = table_candidates(pair, source);
    let production = production_table_admissions(pair, source, &candidates)
        .expect("the production fixture get must succeed");
    let reference = reference_table_admissions(pair, source, &candidates)
        .expect("the reference fixture get must succeed");
    assert_eq!(
        production, reference,
        "the complete table admission census must agree on the dedicated fixture"
    );
    let mut counts = [0_usize; EDIT_FAMILIES];
    for (candidate, verdict) in candidates.into_iter().zip(&reference.verdicts) {
        for family in candidate.negative_families {
            assert_observed_table_negative(Mutation::ALL[family], verdict);
            counts[family] += 1;
        }
    }
    counts
}

#[test]
fn table_negative_admission_families_are_nonvacuous() {
    let dropped = named_table_construction("dropped table row");
    let dropped_pair = TablePair::new(
        dropped.source_schema,
        dropped.target_schema,
        dropped.migration,
        TABLE_FIXTURE_VALUE_KIND,
        ScalarKind::Any,
    );

    let contracted = TablePairPutCase::InsertContracted.fixture();
    let contracted_pair = TablePair::new(
        contracted.source_schema,
        contracted.target_schema,
        contracted.migration,
        TABLE_FIXTURE_VALUE_KIND,
        ScalarKind::Any,
    );

    let mut synthesized = TablePairPutCase::SynthesizedRemoved.fixture();
    let direct = RefSEdge::new("a", "b", "foreign-key", Some("direct"));
    synthesized.source_schema.edges.insert(direct.clone());
    synthesized.migration.surviving_edges.insert(direct.clone());
    synthesized
        .migration
        .edge_remap
        .insert(direct, synthesized.image.clone());
    synthesized
        .source
        .rows
        .insert(3, ("a".to_owned(), BTreeMap::new()));
    synthesized
        .source
        .rows
        .insert(4, ("b".to_owned(), BTreeMap::new()));
    let synthesized_pair = TablePair::new(
        synthesized.source_schema,
        synthesized.target_schema,
        synthesized.migration,
        TABLE_FIXTURE_VALUE_KIND,
        ScalarKind::Any,
    );

    let mut total = [0_usize; EDIT_FAMILIES];
    for counts in [
        table_admission_fixture_census(&dropped_pair, &dropped.source),
        table_admission_fixture_census(&contracted_pair, &contracted.source),
        table_admission_fixture_census(&synthesized_pair, &synthesized.source),
    ] {
        for (sum, count) in total.iter_mut().zip(counts) {
            *sum += count;
        }
    }
    for mutation in [
        Mutation::RemoveSynthesized,
        Mutation::EditSynthesized,
        Mutation::AddContractedEdge,
        Mutation::ReservedIdReuse,
    ] {
        let family = Mutation::ALL
            .iter()
            .position(|candidate| *candidate == mutation)
            .expect("Mutation::ALL contains each table negative family");
        assert!(
            total[family] > 0,
            "table negative family {} must have a differential admission witness",
            mutation.as_str()
        );
    }
}

/// Running totals of the admission census over the reference side.
struct AdmissionTally {
    candidates: std::sync::atomic::AtomicUsize,
    admitted: std::sync::atomic::AtomicUsize,
}

impl Default for AdmissionTally {
    fn default() -> Self {
        Self {
            candidates: std::sync::atomic::AtomicUsize::new(0),
            admitted: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

/// The admission census of one L.8 case on one side: graph and table cases,
/// and tree cases inside the reference tree lens's domain. The reference side
/// adds the deduplicated union to `tally`.
fn law_admissions(
    pairs: &[Pair],
    table_pairs: &[Vec<TablePair>],
    case: &LawCase,
    production: bool,
    tally: &AdmissionTally,
) -> Option<AdmissionCensus> {
    let census = match case {
        LawCase::Graph(stage, source) => {
            let candidates = graph_candidates(&pairs[*stage], source);
            if production {
                production_graph_admissions(&pairs[*stage], source, &candidates)
            } else {
                reference_graph_admissions(&pairs[*stage], source, &candidates)
            }
        }
        LawCase::Tree(stage, tree) if reference_tree_case(&pairs[*stage], tree).is_some() => {
            let candidates = tree_candidates(&pairs[*stage], tree);
            if production {
                production_tree_admissions(&pairs[*stage], tree, &candidates)
            } else {
                reference_tree_admissions(&pairs[*stage], tree, &candidates)
            }
        }
        LawCase::Table(fixture) => {
            let pair = &table_pairs[fixture.columns][fixture.retained];
            let candidates = table_candidates(pair, &fixture.source);
            let census = if production {
                production_table_admissions(pair, &fixture.source, &candidates)
            } else {
                reference_table_admissions(pair, &fixture.source, &candidates)
            };
            if !production && let Some(census) = &census {
                for (candidate, verdict) in candidates.iter().zip(&census.verdicts) {
                    for &family in &candidate.negative_families {
                        assert_observed_table_negative(Mutation::ALL[family], verdict);
                    }
                }
            }
            census
        }
        _ => None,
    };
    if !production && let Some(census) = &census {
        use std::sync::atomic::Ordering;
        tally
            .candidates
            .fetch_add(census.verdicts.len(), Ordering::Relaxed);
        tally.admitted.fetch_add(census.admitted, Ordering::Relaxed);
    }
    census
}

/// The law-report case families of the L.8 scope.
#[derive(Debug)]
enum LawCase {
    Partial(PartialLens, u8),
    Table(TableFixture),
    Graph(usize, RefGraph),
    Tree(usize, RefTree),
}

fn refused_law_outcome() -> LawOutcome {
    LawOutcome {
        first_failure: Some("Refusal".to_owned()),
    }
}

fn production_law_case(context: &LawContext, case: &LawCase) -> LawOutcome {
    let (pairs, table_pairs, carrier) = context;
    match case {
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
        LawCase::Tree(stage, tree) if reference_tree_case(&pairs[*stage], tree).is_none() => {
            outside_the_tree_domain()
        }
        LawCase::Tree(stage, tree) => production_law_outcome(&panproto_lens::check_laws_report::<
            panproto_lens::TreeShape,
        >(
            &pairs[*stage].production,
            &prod_tree(tree),
            &(),
        )),
    }
}

fn reference_graph_law_case(pair: &Pair, source: &RefGraph) -> LawOutcome {
    pair.with_reference(|lens| {
        lens.map_or_else(
            |_| refused_law_outcome(),
            |lens| {
                let outcome = reference_law_outcome(&panproto_reference::lens::check_graph_laws(
                    lens, source,
                ));
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
            },
        )
    })
}

fn reference_law_case(context: &LawContext, case: &LawCase) -> LawOutcome {
    let (pairs, table_pairs, _) = context;
    match case {
        LawCase::Partial(lens, source) => {
            reference_law_outcome(&partial_lens_law_report(lens, usize::from(*source)))
        }
        LawCase::Table(fixture) => {
            table_pairs[fixture.columns][fixture.retained].with_reference(|lens, _| {
                reference_law_outcome(&panproto_reference::lens::check_table_laws(
                    lens,
                    &fixture.source,
                ))
            })
        }
        LawCase::Graph(stage, source) => reference_graph_law_case(&pairs[*stage], source),
        LawCase::Tree(stage, tree) if reference_tree_case(&pairs[*stage], tree).is_none() => {
            outside_the_tree_domain()
        }
        LawCase::Tree(stage, tree) => pairs[*stage].with_reference(|lens| {
            lens.map_or_else(
                |_| refused_law_outcome(),
                |lens| {
                    reference_law_outcome(&panproto_reference::lens::check_tree_laws(lens, tree))
                },
            )
        }),
    }
}

#[test]
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
    let tally = AdmissionTally::default();
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &cases,
        context,
        |_, case| vec![case],
        |context: &LawContext, case: &&LawCase| {
            let (pairs, table_pairs, _) = context;
            (
                production_law_case(context, case),
                law_admissions(pairs, table_pairs, case, true, &tally),
            )
        },
        |context: &LawContext, case: &&LawCase| {
            let (pairs, table_pairs, _) = context;
            (
                reference_law_case(context, case),
                law_admissions(pairs, table_pairs, case, false, &tally),
            )
        },
        |production, reference| production == reference,
    ));
    println!(
        "L.8 admission census (graph, tree and table families): candidates={}, admitted positive reference-basis candidates={}",
        tally.candidates.load(std::sync::atomic::Ordering::Relaxed),
        tally.admitted.load(std::sync::atomic::Ordering::Relaxed),
    );
    assert_eq!(compared, u64::try_from(cases.len()).unwrap());
}

// ---------------------------------------------------------------------------
// The property differential: random cases from every exhaustive family
// ---------------------------------------------------------------------------

/// The amended C11 census has passed its independent count and dynamic gates.
const C11_ACCEPTED: bool = true;

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

type C11Cases = (Vec<C11Stage>, Vec<(usize, RefGraph)>, Vec<(usize, RefTree)>);

fn c11_cases() -> &'static C11Cases {
    static CASES: std::sync::OnceLock<C11Cases> = std::sync::OnceLock::new();
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

fn property_differ(
    what: &str,
    production: impl Into<String>,
    reference: impl Into<String>,
) -> Result<(), String> {
    let production = production.into();
    let reference = reference.into();
    if production == reference {
        Ok(())
    } else {
        Err(format!(
            "{what}: production {production} against reference {reference}"
        ))
    }
}

fn gf5_property_case(index: usize) -> Result<(), String> {
    let cases = gf5_cases();
    let (fixture, source, edited) = &cases[index % cases.len()];
    let pair = gf5_pair(*fixture);
    property_differ(
        "GF5 put",
        format!("{:?}", production_put(&pair, source, edited)),
        format!("{:?}", reference_put(&pair, source, edited)),
    )?;
    let (production, largest, universal) = gf5_cascade_outcome(&pair, source, edited);
    property_differ(
        "GF5 cascade",
        format!(
            "{:?}",
            (production.and_then(|outcome| outcome.put.ok()), universal)
        ),
        format!("{:?}", (largest, true)),
    )
}

fn table_property_case(index: usize, edit: usize) -> Result<(), String> {
    let fixtures = table_fixtures();
    let fixture = &fixtures[index % fixtures.len()];
    let pair = fixture_pair(fixture);
    property_differ(
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
    property_differ(
        "table put",
        format!("{:?}", production_table_put(&pair, &fixture.source, edited)),
        format!("{:?}", reference_table_put(&pair, &fixture.source, edited)),
    )?;
    property_differ(
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

fn partial_lens_property_case(index: usize, edit: usize) -> Result<(), String> {
    let lenses = partial_lenses();
    let lens = &lenses[index % lenses.len()];
    let source = u8::try_from(edit % 2).unwrap();
    property_differ(
        "partial-lens laws",
        format!(
            "{:?}",
            production_law_outcome(&panproto_lens::laws::check_laws_report::<PartialShape>(
                &empty_lens(),
                &source,
                lens,
            ))
        ),
        format!(
            "{:?}",
            reference_law_outcome(&partial_lens_law_report(lens, usize::from(source)))
        ),
    )
}

fn graph_property_case(index: usize, edit: usize) -> Result<(), String> {
    let (stages, graphs, _) = c11_cases();
    let (stage, source) = &graphs[index % graphs.len()];
    let pair = Pair::from_stage(&stages[*stage]);
    property_differ(
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
            .map(|candidate| candidate.view),
    );
    let edited = &views[edit % views.len()];
    property_differ(
        "C11 put",
        format!("{:?}", production_put(&pair, source, edited)),
        format!("{:?}", reference_strict_put(&pair, source, edited)),
    )
}

fn tree_property_case(index: usize) -> Result<(), String> {
    let (stages, _, trees) = c11_cases();
    let (stage, tree) = &trees[index % trees.len()];
    let pair = Pair::from_stage(&stages[*stage]);
    let reference = reference_tree_case(&pair, tree);
    property_differ(
        "C11 tree",
        format!(
            "{:?}",
            production_tree_case(&pair, tree, reference.is_some())
        ),
        format!("{reference:?}"),
    )?;
    let graph = GInstance::from_wtype(&prod_tree(tree)).map_err(|error| format!("{error:?}"))?;
    let graph_leg = production_graph_leg(&pair.production, &graph, None);
    let fast = graph_lens_via_tree(&pair.production, &graph, None);
    let commutes = fast
        .as_ref()
        .is_some_and(|fast| legs_agree(fast, &graph_leg));
    let production = SquareOutcome {
        graph: graph_leg,
        fast,
        commutes,
        outside: false,
    };
    let reference = pair.with_reference(|lens| {
        lens.ok().map(|lens| {
            reference_square(
                &panproto_reference::lens::graph_square(lens, tree),
                |outcome| (reference_leg(&outcome.graph), reference_leg(&outcome.fast)),
            )
        })
    });
    match reference {
        Some(reference) if !squares_agree(&production, &reference) => Err(format!(
            "graph square: production {production:?} against reference {reference:?}"
        )),
        _ => Ok(()),
    }
}

fn path_property_case(index: usize, edit: usize) -> Result<(), String> {
    let (stages, _, _) = c11_cases();
    let pairs: Vec<Pair> = stages.iter().map(Pair::from_stage).collect();
    let all = words(stages.len());
    let word = &all[index % all.len()];
    let production = production_word(stages, word);
    let reference = reference_word(stages, word);
    let (Ok(production), Ok(reference)) = (production, reference) else {
        return property_differ(
            "path composition",
            format!("{}", production_word(stages, word).is_ok()),
            format!("{}", reference_word(stages, word).is_ok()),
        );
    };
    let sources = c11_sources(&stages[word[0]]);
    let source = &sources[edit % sources.len()];
    let (edits, _) = edits_of_reference_view(&pairs, word, source);
    let production = production_path_outcome(&production, source, &edits);
    let reference = reference_path_outcome(&pairs, &reference, source, &edits);
    if path_outcomes_agree(&production, &reference) {
        Ok(())
    } else {
        Err(format!(
            "path: production {production:?} against reference {reference:?}"
        ))
    }
}

fn table_writer_property_case(index: usize, edit: usize) -> Result<(), String> {
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
        production_table_get(&pair, &source).map_err(|error| format!("{error:?}"))?;
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
    property_differ(
        "table writer",
        format!("{production:?}"),
        format!("{reference:?}"),
    )
}

fn table_square_property_case(index: usize) -> Result<(), String> {
    let fixtures = table_fixtures();
    let fixture = &fixtures[index % fixtures.len()];
    let pair = fixture_pair(fixture);
    let (graph, _) = GInstance::from_finstance(&prod_table(&fixture.source))
        .map_err(|error| format!("{error:?}"))?;
    let graph_leg = production_graph_leg(&pair.production, &graph, None);
    let fast = graph_lens_via_table(&pair, &graph, None);
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
                &panproto_reference::lens::table_square(graph_lens, table_lens, &fixture.source),
                |outcome| (reference_leg(&outcome.graph), reference_leg(&outcome.fast)),
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

fn protolens_instance_property_case(index: usize, edit: usize) -> Result<(), String> {
    let protocol = Protocol::default();
    let alphabet = protolens_alphabet();
    let schemas = sigma2_schemas();
    let family = index % alphabet.len();
    let protolens = &alphabet[family];
    let schema = &schemas[(index / alphabet.len()) % schemas.len()];
    let states = c11_graph_states(schema);
    let source = &states[edit % states.len()];
    let production_schema = prod_schema(schema);
    let applicable = assert_protolens_schema_agrees(family, protolens, schema, &protocol);
    let production = match panproto_lens::graph_protolens::apply(
        protolens,
        &production_schema,
        &protocol,
        &prod_graph(source),
    ) {
        Ok(applied) => {
            assert!(applicable);
            let identity = applied.lens.identity(ComplementShape::Graph);
            assert_eq!(applied.complement.identity.as_ref(), Some(&identity));
            ProtolensOutcome::Applied(
                ref_graph(&applied.view),
                Box::new(ref_complement(&applied.complement)),
                Box::new(production_semantic_envelope(&applied.lens, &identity)),
            )
        }
        Err(LensError::ProtolensInapplicable { .. }) if !applicable => {
            ProtolensOutcome::Inapplicable
        }
        Err(error) => ProtolensOutcome::Refused(format!("{:?}", lens_reason(&error))),
    };
    let reference_protolens = reference_protolens(family, schema);
    let reference =
        match panproto_reference::lens::protolens_instance(&reference_protolens, schema, source) {
            Ok(application) => {
                let identity = *application
                    .lens
                    .as_graph_lens()
                    .expect("an instantiated reference graph lens")
                    .identity();
                assert_eq!(application.complement.identity, Some(identity));
                ProtolensOutcome::Applied(
                    application.view,
                    Box::new(without_identity(&application.complement)),
                    Box::new(reference_semantic_envelope(
                        schema,
                        &reference_protolens.target,
                        &reference_protolens.migration,
                        &identity,
                    )),
                )
            }
            Err(RefProtolensError::FalsePrecondition { .. }) => ProtolensOutcome::Inapplicable,
            Err(RefProtolensError::Restrict(error)) => ProtolensOutcome::Refused(format!(
                "{:?}",
                Reason::Restrict(ref_restrict_reason(&error))
            )),
            Err(RefProtolensError::Plan(error)) => ProtolensOutcome::Refused(format!(
                "{:?}",
                Reason::Restrict(ref_restrict_reason(&RefRestrictError::Plan(error)))
            )),
        };
    property_differ(
        "protolens instance",
        format!("{production:?}"),
        format!("{reference:?}"),
    )
}

/// One random case of one family, compared on both sides.
fn property_case(family: u8, index: usize, edit: usize) -> Result<(), String> {
    match family {
        0 => gf5_property_case(index),
        1 => table_property_case(index, edit),
        2 => partial_lens_property_case(index, edit),
        3 if C11_ACCEPTED => graph_property_case(index, edit),
        4 if C11_ACCEPTED => tree_property_case(index),
        5 if C11_ACCEPTED => path_property_case(index, edit),
        6 => table_writer_property_case(index, edit),
        7 => table_square_property_case(index),
        8 if C11_ACCEPTED => protolens_instance_property_case(index, edit),
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
