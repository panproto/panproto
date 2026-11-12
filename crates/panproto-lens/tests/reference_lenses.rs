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

mod support;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_inst::identity::{ComplementShape, IdentityEnvelope as ProductionIdentityEnvelope};
use panproto_inst::{
    CompiledMigration, EdgeId, FieldPresence, GEdge, GInstance, GraphComplement, IdFloor, Node,
    NodeShape, PutDomainError, RestrictError, SynthKey, Value, ViewRef,
};
use panproto_lens::{Lens, LensError, graph_lens};
use panproto_reference::enumerate::lenses::{
    BOUNDED_LENS_FIELD, BOUNDED_LENS_VALUE, BoundedLensAddField, BoundedLensDropField,
    BoundedLensLayer, BoundedLensOpTerm, BoundedLensReadField, BoundedLensStage,
    BoundedLensSurvivalFalse, GraphPutCascadeLawCase, GraphPutDomainFamily, GraphPutLawCase,
    GraphTreeRouteLawCase, LensEquivalenceCase, LensLawCertificateCase, PathBoundaryWitness,
    PathLawCarrier, PathLawCase, ProtolensInstanceCase, ProtolensSchemaCase,
    TableFusionBoundaryWitness, TreeFusionBoundaryWitness, TreePutLawCase,
    for_each_bounded_lens_tree_square_case, for_each_graph_put_cascade_law_case,
    for_each_graph_put_law_case, for_each_graph_tree_route_law_case,
    for_each_lens_equivalence_case, for_each_lens_law_case, for_each_path_law_case,
    for_each_protolens_instance_case, for_each_protolens_schema_case, for_each_tree_put_law_case,
    graph_put_cascade_law_case_count_certificate, graph_put_law_case_count_certificate,
    graph_tree_route_law_case_count_certificate,
    graph_tree_route_law_observation_count_certificate, lens_law_case_count_certificate,
    path_law_case_count_certificate, protolens_instance_count_certificate,
    protolens_schema_case_count_certificate, tree_put_law_case_count_certificate,
};
use panproto_reference::enumerate::restrict_and_contraction::{
    TreeFloorCase, TreeRestrictCase, TreeRoute, tree_restrict_cases,
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
use panproto_schema::{Edge, Key, KeyPart, KeyRole, KeyScope, Protocol, Schema, SchemaBuilder};
use proptest::prelude::*;
use support::graph_put_contract::{
    assert_graph_put_contract_case, assert_randomized_graph_put_contract,
};
use support::tree_validation::{assert_randomized_tree_validation, assert_tree_validation_case};

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
        Value::Float(f) => RefValue::Float(f.to_bits()),
        Value::Str(s) => RefValue::Str(s.clone()),
        Value::Null => RefValue::Null,
        Value::Temporal(temporal) => ref_offset_only_temporal(ref_offset_minutes(temporal)),
        Value::List(items) => RefValue::List(items.iter().map(ref_value).collect()),
        other => panic!("value outside the lens differential alphabet: {other:?}"),
    }
}

fn prod_value(value: &RefValue) -> Value {
    match value {
        RefValue::Bool(b) => Value::Bool(*b),
        RefValue::Int(i) => Value::Int(*i),
        RefValue::Float(bits) => Value::Float(f64::from_bits(*bits)),
        RefValue::Str(s) => Value::Str(s.clone()),
        RefValue::Null => Value::Null,
        RefValue::Temporal(temporal) => Value::Temporal(prod_ill_formed_temporal(temporal)),
        RefValue::List(items) => Value::List(items.iter().map(prod_value).collect()),
        other => panic!("value outside the lens differential alphabet: {other:?}"),
    }
}

/// A temporal with neither a date nor a time and a fixed offset of
/// `minutes`, which is well formed only below a whole day: at a day or more
/// it has no canonical bytes on either side.
const fn ref_offset_only_temporal(minutes: i16) -> RefValue {
    use panproto_reference::temporal::{Offset, OffsetForm, Temporal};
    RefValue::Temporal(Temporal {
        date: None,
        time: None,
        separator: None,
        offset: Offset::Fixed(minutes, OffsetForm::Extended),
        infinity: None,
    })
}

/// The production carrier of a reference offset-only temporal, the one
/// temporal shape of the lens differential alphabet.
fn prod_ill_formed_temporal(
    temporal: &panproto_reference::temporal::Temporal,
) -> panproto_expr::exact::Temporal {
    use panproto_expr::exact::{ExtendedTemporal, Offset, Temporal};
    use panproto_reference::temporal::{Offset as RefOffset, OffsetForm, Temporal as RefTemporal};
    match temporal {
        RefTemporal {
            date: None,
            time: None,
            separator: None,
            offset: RefOffset::Fixed(minutes, OffsetForm::Extended),
            infinity: None,
        } => Temporal::Extended(ExtendedTemporal {
            date: None,
            time: None,
            separator: None,
            offset: Offset::Fixed(*minutes),
            infinity: None,
        }),
        other => panic!("temporal outside the lens differential alphabet: {other:?}"),
    }
}

fn ref_offset_minutes(temporal: &panproto_expr::exact::Temporal) -> i16 {
    use panproto_expr::exact::{ExtendedTemporal, Offset, Temporal};
    match temporal {
        Temporal::Extended(ExtendedTemporal {
            date: None,
            time: None,
            separator: None,
            offset: Offset::Fixed(minutes),
            infinity: None,
        }) => *minutes,
        other => panic!("temporal outside the lens differential alphabet: {other:?}"),
    }
}

#[test]
fn float_values_cross_the_differential_alphabet_bit_exactly() {
    for bits in panproto_reference::enumerate::lenses::LAW_EQUIVALENCE_FLOAT_WITNESSES {
        let value = Value::Float(f64::from_bits(bits));
        assert_eq!(ref_value(&value), RefValue::Float(bits));
        let Value::Float(back) = prod_value(&ref_value(&value)) else {
            panic!("a float converts back to a float");
        };
        assert_eq!(back.to_bits(), bits);
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
        migration.conditional.is_empty()
            && migration.transformed.is_empty()
            && migration.output_fields.is_empty(),
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

/// The production lens of a bounded lens stage: the structural migration, plus the
/// production value layer its tag names.
fn production_stage(stage: &BoundedLensStage) -> Lens {
    let mut structural = stage.migration.clone();
    structural.conditional.clear();
    structural.transformed.clear();
    structural.output_fields.clear();
    let mut compiled = prod_migration(&structural);
    match stage.layer {
        BoundedLensLayer::None => {}
        BoundedLensLayer::SurvivalFalse => {
            for vertex in &stage.migration.conditional {
                compiled.conditional_survival.insert(
                    vertex.clone().into(),
                    panproto_expr::Expr::Lit(panproto_expr::Literal::Bool(false)),
                );
            }
        }
        BoundedLensLayer::AddField => {
            for vertex in &stage.migration.transformed {
                compiled.field_transforms.insert(
                    vertex.clone().into(),
                    vec![panproto_inst::FieldTransform::AddField {
                        key: BOUNDED_LENS_FIELD.to_owned(),
                        value: prod_value(&BOUNDED_LENS_VALUE),
                    }],
                );
            }
        }
        BoundedLensLayer::DropField => {
            for vertex in &stage.migration.transformed {
                compiled.field_transforms.insert(
                    vertex.clone().into(),
                    vec![panproto_inst::FieldTransform::DropField {
                        key: BOUNDED_LENS_FIELD.to_owned(),
                    }],
                );
            }
        }
        BoundedLensLayer::ReadField => {
            for vertex in &stage.migration.transformed {
                compiled.field_transforms.insert(
                    vertex.clone().into(),
                    vec![panproto_inst::FieldTransform::ApplyExpr {
                        key: BOUNDED_LENS_FIELD.to_owned(),
                        expr: panproto_expr::Expr::Var(BOUNDED_LENS_FIELD.into()),
                        inverse: Some(panproto_expr::Expr::Var(BOUNDED_LENS_FIELD.into())),
                        coercion_class: panproto_gat::CoercionClass::Iso,
                    }],
                );
            }
        }
        BoundedLensLayer::OpTerm => {
            for vertex in &stage.migration.transformed {
                compiled.op_term_assignments.insert(
                    vertex.clone().into(),
                    vec![panproto_inst::TermAssignment::Compute {
                        target: BOUNDED_LENS_FIELD.to_owned(),
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
        R::Arc { parent, child } => format!("Arc({parent},{child})"),
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
        PutDomainError::KeyRefused { table, key, reason } => {
            format!("KeyRefused {table} {key} {}", key_refusal_reason(reason))
        }
        PutDomainError::PlanCompilation { detail } => {
            format!("PlanCompilation {detail}")
        }
    }
}

fn key_refusal_reason(reason: &panproto_inst::KeyRefusal) -> String {
    use panproto_inst::KeyRefusal;
    match reason {
        KeyRefusal::UnsupportedCollation { collation } => {
            format!("UnsupportedCollation {collation}")
        }
        KeyRefusal::EqualityArity { expected, found } => {
            format!("EqualityArity {expected} {found}")
        }
        KeyRefusal::InvalidNumericRepresentation { kind } => {
            format!("InvalidNumericRepresentation {kind}")
        }
        KeyRefusal::NoCanonicalBytes => "NoCanonicalBytes".to_owned(),
    }
}

fn ref_key_refusal_reason(reason: &panproto_reference::keys::KeyingError) -> String {
    use panproto_reference::keys::KeyingError;
    match reason {
        KeyingError::UnsupportedCollation(collation) => {
            format!("UnsupportedCollation {collation}")
        }
        KeyingError::EqualityArity { expected, found } => {
            format!("EqualityArity {expected} {found}")
        }
        KeyingError::NoCanonicalBytes => "NoCanonicalBytes".to_owned(),
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
        R::KeyRefused { table, key, reason } => {
            format!(
                "KeyRefused {table} {key} {}",
                ref_key_refusal_reason(reason)
            )
        }
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
        RestrictError::MultipleArcsOnSingleValuedEdge {
            parent,
            edge,
            count,
        } => format!(
            "MultipleArcsOnSingleValuedEdge {parent} {:?} {count}",
            ref_edge(edge)
        ),
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
        RestrictError::SourceDeclarations(errors) => {
            format!("SourceDeclarations {}", declaration_reasons(errors))
        }
        RestrictError::TargetDeclarations(errors) => {
            format!("TargetDeclarations {}", declaration_reasons(errors))
        }
        RestrictError::UndeclaredTransformField {
            source_vertex,
            target_vertex,
            field,
        } => {
            format!("UndeclaredTransformField {source_vertex} {target_vertex} {field}")
        }
        other => format!("production restrict refusal {other:?}"),
    }
}

fn declaration_reasons(errors: &[panproto_schema::KeyRefError]) -> String {
    use panproto_schema::KeyRefError as E;
    errors
        .iter()
        .map(|error| match error {
            E::EqualityArity {
                vertex,
                key,
                expected,
                found,
            } => format!("EqualityArity {vertex} {key} {expected} {found}"),
            E::SortKeyNotOrdered { key, vertex } => format!("SortKeyNotOrdered {vertex} {key}"),
            E::DuplicateKeyId { vertex, key } => format!("DuplicateKeyId {vertex} {key}"),
            other => format!("production declaration error {other:?}"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn ref_declaration_reasons(errors: &[panproto_reference::schema::DeclarationError]) -> String {
    use panproto_reference::schema::DeclarationError as E;
    errors
        .iter()
        .map(|error| match error {
            E::EqualityArity {
                vertex,
                key,
                expected,
                found,
            } => format!("EqualityArity {vertex} {key} {expected} {found}"),
            E::SortKeyNotOrdered { key, vertex } => format!("SortKeyNotOrdered {vertex} {key}"),
            E::DuplicateKeyId { vertex, key } => format!("DuplicateKeyId {vertex} {key}"),
            other => format!("reference declaration error {other:?}"),
        })
        .collect::<Vec<_>>()
        .join(", ")
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
        RefRestrictError::MultipleArcsOnSingleValuedEdge {
            parent,
            edge,
            count,
        } => format!("MultipleArcsOnSingleValuedEdge {parent} {edge:?} {count}"),
        RefRestrictError::MalformedFan { node, reason } => {
            format!("MalformedFan {node} {}", ref_fan_reason(reason))
        }
        RefRestrictError::Plan(PlanError::SourceDeclarations(errors)) => {
            format!("SourceDeclarations {}", ref_declaration_reasons(errors))
        }
        RefRestrictError::Plan(PlanError::TargetDeclarations(errors)) => {
            format!("TargetDeclarations {}", ref_declaration_reasons(errors))
        }
        RefRestrictError::Plan(PlanError::UndeclaredTransformField {
            source_vertex,
            target_vertex,
            field,
        }) => format!("UndeclaredTransformField {source_vertex} {target_vertex} {field}"),
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

    /// A bounded lens stage on both sides. The value layer is built on each side
    /// from the stage's layer tag: the production expression or transform,
    /// and the reference's caller-supplied layer.
    fn from_stage(stage: &BoundedLensStage) -> Self {
        let layer: &'static dyn ValueLayer = match stage.layer {
            BoundedLensLayer::None => &NoValueLayer,
            BoundedLensLayer::SurvivalFalse => &BoundedLensSurvivalFalse,
            BoundedLensLayer::AddField => &BoundedLensAddField,
            BoundedLensLayer::DropField => &BoundedLensDropField,
            BoundedLensLayer::ReadField => &BoundedLensReadField,
            BoundedLensLayer::OpTerm => &BoundedLensOpTerm,
        };
        let production = production_stage(stage);
        assert_eq!(
            stage.migration.output_fields,
            reference_output_fields(&production.compiled)
        );
        Self::with_production(
            stage.source.clone(),
            stage.target.clone(),
            stage.migration.clone(),
            layer,
            production,
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
/// on them, so a disagreement is reported with its own case. A thread checks
/// the shared stop flag before claiming a block. Once claimed, it completes
/// that block unless it finds its own disagreement. Thus every unit below the
/// lowest disagreement is known to have completed. Otherwise every unit
/// completes and the result is the number of cases compared, which is
/// independent of the thread count.
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

/// A parallel comparison unit's state: not yet claimed by a worker.
const UNCLAIMED: u8 = 0;
/// A parallel comparison unit's state: every case agreed or lay in the gap.
const COMPLETED: u8 = 2;
/// A parallel comparison unit's state: a case disagreed.
const MISMATCH: u8 = 3;

/// One worker's result: its census, or the unit and disagreement at which it
/// stopped.
type ParallelOutcome = Result<Census, (usize, Disagreement)>;

/// The number of comparison workers: `PANPROTO_TEST_WORKERS` when it is a
/// positive integer, else the available parallelism (4 when unknown).
fn comparison_threads() -> usize {
    std::env::var("PANPROTO_TEST_WORKERS")
        .ok()
        .and_then(|workers| workers.parse::<usize>().ok())
        .filter(|workers| *workers > 0)
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, std::num::NonZero::get))
}

/// The summed census of the workers that finished, with its witnesses sorted
/// and truncated, and the lowest-unit disagreement of those that stopped.
fn merge_parallel_outcomes(
    outcomes: Vec<ParallelOutcome>,
) -> (Census, Option<(usize, Disagreement)>) {
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
    (total, first)
}

/// Check the unit states a parallel comparison left behind and return its
/// verdict: on success every unit completed and no mismatch was recorded; on
/// a disagreement its unit is the lowest recorded mismatch and every unit
/// below it completed.
fn audit_parallel_units(
    states: &[std::sync::atomic::AtomicU8],
    recorded_lowest: usize,
    total: Census,
    first: Option<(usize, Disagreement)>,
) -> Result<Census, Disagreement> {
    use std::sync::atomic::Ordering;
    match first {
        None => {
            assert_eq!(
                recorded_lowest,
                usize::MAX,
                "a mismatch was recorded without a disagreement outcome"
            );
            for (unit, state) in states.iter().enumerate() {
                assert_eq!(
                    state.load(Ordering::Acquire),
                    COMPLETED,
                    "successful parallel comparison did not complete unit {unit}"
                );
            }
            Ok(total)
        }
        Some((unit, disagreement)) => {
            assert_eq!(
                recorded_lowest, unit,
                "reported disagreement is not the lowest recorded mismatch"
            );
            assert_eq!(
                states[unit].load(Ordering::Acquire),
                MISMATCH,
                "lowest disagreement unit was not marked as a mismatch"
            );
            for (prefix, state) in states[..unit].iter().enumerate() {
                assert_eq!(
                    state.load(Ordering::Acquire),
                    COMPLETED,
                    "unit {prefix} below the lowest disagreement did not complete"
                );
            }
            Err(disagreement)
        }
    }
}

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
    use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

    const BLOCK: usize = 32;
    const CLAIMED: u8 = 1;

    let (production, reference) = (&sides.0, &sides.1);
    let threads = comparison_threads();
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let lowest_mismatch = AtomicUsize::new(usize::MAX);
    let states: Vec<_> = (0..units.len()).map(|_| AtomicU8::new(UNCLAIMED)).collect();
    let outcomes: Vec<ParallelOutcome> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let context = context();
                    let mut census = Census::default();
                    loop {
                        if stop.load(Ordering::Acquire) {
                            return Ok(census);
                        }
                        let start = next.fetch_add(BLOCK, Ordering::Relaxed);
                        if start >= units.len() {
                            return Ok(census);
                        }
                        let end = (start + BLOCK).min(units.len());
                        for (offset, state) in states[start..end].iter().enumerate() {
                            assert_eq!(
                                state.compare_exchange(
                                    UNCLAIMED,
                                    CLAIMED,
                                    Ordering::AcqRel,
                                    Ordering::Acquire
                                ),
                                Ok(UNCLAIMED),
                                "parallel comparison unit {} was claimed twice",
                                start + offset
                            );
                        }
                        for (offset, unit) in units[start..end].iter().enumerate() {
                            let unit_index = start + offset;
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
                                states[unit_index].store(MISMATCH, Ordering::Release);
                                lowest_mismatch.fetch_min(unit_index, Ordering::AcqRel);
                                stop.store(true, Ordering::Release);
                                return Err((unit_index, disagreement));
                            }
                            states[unit_index].store(COMPLETED, Ordering::Release);
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
    let (total, first) = merge_parallel_outcomes(outcomes);
    audit_parallel_units(
        &states,
        lowest_mismatch.load(Ordering::Acquire),
        total,
        first,
    )
}

// ---------------------------------------------------------------------------
// The Delta-fragment cascade (L-lenses-4) on its exact finite scope
// ---------------------------------------------------------------------------

use panproto_reference::enumerate::lenses::{
    DeltaFragmentFixture, delta_fragment_count_certificate, for_each_delta_fragment_deletion_edit,
    for_each_delta_fragment_source,
};

fn delta_fragment_pair(fixture: DeltaFragmentFixture) -> Pair {
    Pair::new(
        fixture.source_schema(),
        fixture.target_schema(),
        fixture.migration(),
    )
}

/// The Delta-fragment cascade law, compared on both sides: production put equals the
/// reference put, and both equal the reference's largest exact preimage,
/// which satisfies the universal property.
fn delta_fragment_cascade_outcome(
    pair: &Pair,
    source: &RefGraph,
    edited: &RefGraph,
) -> (Option<PutOutcome>, Option<RefGraph>, bool) {
    let production = production_put(pair, source, edited);
    let (largest, universal) = pair.with_reference(|lens| {
        let lens = lens.expect("Delta-fragment plans compile");
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

fn assert_graph_put_domain_outcome(
    pair: &Pair,
    source: &RefGraph,
    edited: &RefGraph,
    family: GraphPutDomainFamily,
) -> u16 {
    let production = production_put(pair, source, edited)
        .unwrap_or_else(|| panic!("production get refused the {family:?} witness"));
    let reference = reference_strict_put(pair, source, edited)
        .unwrap_or_else(|| panic!("reference get refused the {family:?} witness"));
    assert_eq!(production, reference, "graph domain family {family:?}");
    let reasons = reference
        .domain
        .expect_err("the named graph-domain witness must refuse");
    assert!(
        reasons
            .iter()
            .any(|reason| reason.starts_with(family.name())),
        "the {family:?} witness returned {reasons:?}"
    );
    graph_put_domain_family_bit(family.name())
}

fn conditional_graph_pair() -> Pair {
    let schema = panproto_reference::enumerate::lenses::bounded_lens_schema(&["a"], &[]);
    let migration = RefMigration {
        surviving_verts: ["a".to_owned()].into(),
        conditional: ["a".to_owned()].into(),
        ..RefMigration::default()
    };
    let production_schema = prod_schema(&schema);
    let production = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into()].into(),
            conditional_survival: [("a".into(), panproto_expr::Expr::Var("keep".into()))].into(),
            ..CompiledMigration::default()
        },
        src_schema: production_schema.clone(),
        tgt_schema: production_schema,
    };
    Pair::with_production(
        schema.clone(),
        schema,
        migration,
        &ConditionalGraphPredicate,
        production,
    )
}

fn assert_graph_predicate_domain_families() -> u16 {
    let pair = conditional_graph_pair();
    let mut node = RefNode::new(0, "a");
    node.fields.insert("keep".to_owned(), RefValue::Bool(true));
    let source = RefGraph {
        nodes: [(0, node)].into(),
        edges: BTreeMap::new(),
    };
    let (view, _) = reference_get(&pair, &source).expect("the predicate source survives");

    let mut fails = view.clone();
    fails
        .nodes
        .get_mut(&0)
        .expect("predicate node")
        .fields
        .insert("keep".to_owned(), RefValue::Bool(false));
    let mut observed = assert_graph_put_domain_outcome(
        &pair,
        &source,
        &fails,
        GraphPutDomainFamily::SurvivalPredicateFails,
    );

    let mut refuses = view;
    refuses
        .nodes
        .get_mut(&0)
        .expect("predicate node")
        .fields
        .insert("keep".to_owned(), RefValue::Int(1));
    observed |= assert_graph_put_domain_outcome(
        &pair,
        &source,
        &refuses,
        GraphPutDomainFamily::SurvivalPredicateRefused,
    );
    observed
}

fn assert_graph_noninvertible_transform_domain_family() -> u16 {
    let schema = panproto_reference::enumerate::lenses::bounded_lens_schema(&["a"], &[])
        .vertex("x_value", "int")
        .edge("a", "x_value", "prop", Some("x"));
    let migration = RefMigration {
        surviving_verts: ["a".to_owned()].into(),
        transformed: ["a".to_owned()].into(),
        output_fields: [("a".to_owned(), vec!["x".to_owned()])].into(),
        ..RefMigration::default()
    };
    let production_schema = prod_schema(&schema);
    let production = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into()].into(),
            field_transforms: [(
                "a".into(),
                vec![panproto_inst::FieldTransform::ApplyExpr {
                    key: "x".to_owned(),
                    expr: panproto_expr::Expr::Builtin(
                        panproto_expr::BuiltinOp::Abs,
                        vec![panproto_expr::Expr::Var("x".into())],
                    ),
                    inverse: None,
                    coercion_class: panproto_gat::CoercionClass::Projection,
                }],
            )]
            .into(),
            ..CompiledMigration::default()
        },
        src_schema: production_schema.clone(),
        tgt_schema: production_schema,
    };
    let pair = Pair::with_production(
        schema.clone(),
        schema,
        migration,
        &NonInvertibleTableTransform,
        production,
    );
    let mut node = RefNode::new(0, "a");
    node.fields.insert("x".to_owned(), RefValue::Int(-1));
    let source = RefGraph {
        nodes: [(0, node)].into(),
        edges: BTreeMap::new(),
    };
    let (mut edited, _) = reference_get(&pair, &source).expect("the absolute-value get succeeds");
    edited
        .nodes
        .get_mut(&0)
        .expect("transform node")
        .fields
        .insert("x".to_owned(), RefValue::Int(2));
    assert_graph_put_domain_outcome(
        &pair,
        &source,
        &edited,
        GraphPutDomainFamily::TransformNotInvertible,
    )
}

fn assert_graph_missing_required_cell_domain_family() -> u16 {
    let required_edge = RefSEdge::new("a", "b", "rel", Some("b"));
    let mut reference_required = RefSchema::default()
        .vertex("a", "obj")
        .vertex("b", "obj")
        .edge("a", "b", "rel", Some("b"));
    reference_required
        .required
        .insert("a".to_owned(), vec![required_edge.clone()]);
    let mut production_required = SchemaBuilder::new(&Protocol::default())
        .vertex("a", "obj", None)
        .expect("a")
        .vertex("b", "obj", None)
        .expect("b")
        .edge("a", "b", "rel", Some("b"))
        .expect("a-b")
        .build()
        .expect("required schema");
    production_required
        .required
        .insert("a".into(), vec![prod_edge(&required_edge)]);
    let required_migration = reference_carrier_migration(&reference_required);
    let required_pair = Pair::with_production(
        reference_required.clone(),
        reference_required,
        required_migration,
        &NoValueLayer,
        Lens {
            compiled: CompiledMigration {
                surviving_verts: ["a".into(), "b".into()].into(),
                surviving_edges: [prod_edge(&required_edge)].into(),
                ..CompiledMigration::default()
            },
            src_schema: production_required.clone(),
            tgt_schema: production_required,
        },
    );
    let required_source = RefGraph {
        nodes: [(0, RefNode::new(0, "a"))].into(),
        edges: BTreeMap::new(),
    };
    assert_graph_put_domain_outcome(
        &required_pair,
        &required_source,
        &required_source,
        GraphPutDomainFamily::MissingRequiredCell,
    )
}

fn assert_graph_duplicate_key_domain_family() -> u16 {
    let mut reference_keyed =
        panproto_reference::enumerate::lenses::bounded_lens_schema(&["a"], &[]);
    reference_keyed.keys.insert(
        "a".to_owned(),
        vec![panproto_reference::schema::Key {
            id: "by-value".to_owned(),
            parts: vec![panproto_reference::schema::KeyPart::Path(Vec::new())],
            role: panproto_reference::schema::KeyRole::Primary,
            scope: panproto_reference::schema::KeyScope::Global,
            equality: Vec::new(),
        }],
    );
    let mut production_keyed = SchemaBuilder::new(&Protocol::default())
        .vertex("a", "obj", None)
        .expect("a")
        .build()
        .expect("keyed schema");
    production_keyed.declarations.keys.insert(
        "a".into(),
        vec![Key {
            id: "by-value".into(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::Primary,
            scope: KeyScope::Global,
            equality: vec![],
        }],
    );
    let keyed_migration = reference_carrier_migration(&reference_keyed);
    let keyed_pair = Pair::with_production(
        reference_keyed.clone(),
        reference_keyed,
        keyed_migration,
        &NoValueLayer,
        Lens {
            compiled: CompiledMigration {
                surviving_verts: ["a".into()].into(),
                ..CompiledMigration::default()
            },
            src_schema: production_keyed.clone(),
            tgt_schema: production_keyed,
        },
    );
    let duplicate_value = Some(RefPresence::Present(RefValue::Str("same".to_owned())));
    let mut first = RefNode::new(0, "a");
    first.value.clone_from(&duplicate_value);
    let mut second = RefNode::new(1, "a");
    second.value = duplicate_value;
    let keyed_source = RefGraph {
        nodes: [(0, first), (1, second)].into(),
        edges: BTreeMap::new(),
    };
    assert_graph_put_domain_outcome(
        &keyed_pair,
        &keyed_source,
        &keyed_source,
        GraphPutDomainFamily::DuplicateKey,
    )
}

fn prod_key_equality(
    equality: &panproto_reference::keys::KeyEquality,
) -> panproto_schema::KeyEquality {
    use panproto_reference::keys::{
        KeyEquality as R, NumericPrimitive as RefPrimitive, SqlCollation as RefCollation,
        SqlPad as RefPad,
    };
    use panproto_schema::{KeyEquality, NumericPrimitive, SqlCollation, SqlPad};
    match equality {
        R::Representation => KeyEquality::Representation,
        R::NumericValue => KeyEquality::NumericValue,
        R::NumericValueWithinKind(primitive) => {
            KeyEquality::NumericValueWithinKind(match primitive {
                RefPrimitive::Decimal => NumericPrimitive::Decimal,
                RefPrimitive::Float(width) => NumericPrimitive::Float(*width),
                RefPrimitive::Any => NumericPrimitive::Any,
            })
        }
        R::Sql { collation, pad } => KeyEquality::Sql {
            collation: match collation {
                RefCollation::Binary => SqlCollation::Binary,
                RefCollation::AsciiCaseInsensitive => SqlCollation::AsciiCaseInsensitive,
                RefCollation::Named(name) => SqlCollation::Named(name.as_str().into()),
            },
            pad: match pad {
                RefPad::NoPad => SqlPad::NoPad,
                RefPad::PadSpace => SqlPad::PadSpace,
            },
        },
        R::XmlNonCdata => KeyEquality::XmlNonCdata,
    }
}

fn prod_key(key: &panproto_reference::schema::Key) -> Key {
    use panproto_reference::schema::{
        KeyPart as RefPart, KeyRole as RefRole, KeyScope as RefScope,
    };
    Key {
        id: key.id.as_str().into(),
        parts: key
            .parts
            .iter()
            .map(|part| match part {
                RefPart::Path(path) => KeyPart::Path(path.iter().map(prod_edge).collect()),
                RefPart::Position { within } => KeyPart::Position {
                    within: prod_edge(within),
                },
            })
            .collect(),
        role: match key.role {
            RefRole::Primary => KeyRole::Primary,
            RefRole::Unique => KeyRole::Unique,
            RefRole::Identifier => KeyRole::Identifier,
            RefRole::SortKey => KeyRole::SortKey,
            RefRole::Index => KeyRole::Index,
        },
        scope: match &key.scope {
            RefScope::Document => KeyScope::Document,
            RefScope::Global => KeyScope::Global,
            RefScope::Within(vertex) => KeyScope::Within(vertex.as_str().into()),
        },
        equality: key.equality.iter().map(prod_key_equality).collect(),
    }
}

/// An identity lens over the bounded lens schema on `vertices` and `edges` whose
/// vertex `a` declares `keys`, built on both sides.
fn keyed_graph_pair(
    vertices: &[&str],
    edges: &[(&str, &str)],
    keys: Vec<panproto_reference::schema::Key>,
) -> Pair {
    let mut reference = panproto_reference::enumerate::lenses::bounded_lens_schema(vertices, edges);
    let mut production = prod_schema(&reference);
    production
        .declarations
        .keys
        .insert("a".into(), keys.iter().map(prod_key).collect());
    reference.keys.insert("a".to_owned(), keys);
    let migration = reference_carrier_migration(&reference);
    let lens = Lens {
        compiled: prod_migration(&migration),
        src_schema: production.clone(),
        tgt_schema: production,
    };
    Pair::with_production(reference.clone(), reference, migration, &NoValueLayer, lens)
}

fn value_key(
    id: &str,
    role: panproto_reference::schema::KeyRole,
    scope: panproto_reference::schema::KeyScope,
    equality: Vec<panproto_reference::keys::KeyEquality>,
) -> panproto_reference::schema::Key {
    panproto_reference::schema::Key {
        id: id.to_owned(),
        parts: vec![panproto_reference::schema::KeyPart::Path(Vec::new())],
        role,
        scope,
        equality,
    }
}

/// Nodes `0..` anchored at `a`, carrying `values` in order (`None` for a
/// node without a value).
fn valued_a_nodes(values: &[Option<RefValue>]) -> RefGraph {
    let nodes = (0_u32..)
        .zip(values)
        .map(|(id, value)| {
            let mut node = RefNode::new(id, "a");
            node.value = value.clone().map(RefPresence::Present);
            (id, node)
        })
        .collect();
    RefGraph {
        nodes,
        edges: BTreeMap::new(),
    }
}

/// The graph put-domain outcome on both sides, asserted equal; returns the
/// shared domain report.
fn agreed_graph_put_domain(pair: &Pair, source: &RefGraph, what: &str) -> Result<(), Vec<String>> {
    let production = production_put(pair, source, source)
        .unwrap_or_else(|| panic!("production get refused the {what} witness"));
    let reference = reference_strict_put(pair, source, source)
        .unwrap_or_else(|| panic!("reference get refused the {what} witness"));
    assert_eq!(production, reference, "{what}");
    reference.domain
}

/// A declared key whose restored value has no
/// canonical bytes is a typed put-domain refusal in production and in the
/// reference alike, under every equality declaration, and the refusal takes
/// the key's place in the report instead of its duplicate-key verdicts.
#[test]
fn graph_put_refuses_key_values_without_canonical_bytes_like_the_reference() {
    use panproto_reference::keys::{KeyEquality as Eq, NumericPrimitive, SqlCollation, SqlPad};
    use panproto_reference::schema::{KeyRole, KeyScope};
    let equalities = [
        Vec::new(),
        vec![Eq::Representation],
        vec![Eq::NumericValue],
        vec![Eq::NumericValueWithinKind(NumericPrimitive::Any)],
        vec![Eq::Sql {
            collation: SqlCollation::Binary,
            pad: SqlPad::PadSpace,
        }],
        vec![Eq::XmlNonCdata],
    ];
    let same = Some(RefValue::Str("same".to_owned()));
    let witnesses = [
        // Two distinct values without canonical bytes.
        vec![
            Some(ref_offset_only_temporal(1440)),
            Some(ref_offset_only_temporal(1441)),
        ],
        // One value without canonical bytes among a duplicated pair: the
        // key is undecided, so the pair is not reported.
        vec![same.clone(), Some(ref_offset_only_temporal(-1440)), same],
        // A value without canonical bytes nested in a list.
        vec![
            Some(RefValue::List(vec![
                RefValue::Int(1),
                ref_offset_only_temporal(2000),
            ])),
            Some(RefValue::Int(1)),
        ],
    ];
    for equality in &equalities {
        for values in &witnesses {
            let pair = keyed_graph_pair(
                &["a"],
                &[],
                vec![value_key(
                    "by-value",
                    KeyRole::Unique,
                    KeyScope::Global,
                    equality.clone(),
                )],
            );
            let what = format!("{equality:?} over {values:?}");
            let report = agreed_graph_put_domain(&pair, &valued_a_nodes(values), &what)
                .expect_err("a key value without canonical bytes must refuse");
            assert_eq!(
                report,
                ["KeyRefused a by-value NoCanonicalBytes".to_owned()],
                "{what}"
            );
        }
    }
}

/// The graph key clause agrees with the reference beyond representation
/// keys: declared equality, non-unique roles (an ill-formed sort key is
/// refused when the plan compiles), a `Null` value read as SQL NULL, the
/// primary-key presence rule and elements outside a scoped key's domain.
#[test]
fn graph_put_key_clause_agrees_with_the_reference_on_declared_semantics() {
    use panproto_reference::keys::KeyEquality as Eq;
    use panproto_reference::schema::{KeyRole, KeyScope};
    let one = vec![
        Some(RefValue::Int(1)),
        Some(RefValue::Float(1.0_f64.to_bits())),
    ];
    let same = vec![
        Some(RefValue::Str("same".to_owned())),
        Some(RefValue::Str("same".to_owned())),
    ];
    let cases = [
        (
            "numeric equality",
            value_key(
                "k",
                KeyRole::Unique,
                KeyScope::Global,
                vec![Eq::NumericValue],
            ),
            one,
            Vec::new(),
        ),
        (
            "index role",
            value_key("k", KeyRole::Index, KeyScope::Global, Vec::new()),
            same.clone(),
            Vec::new(),
        ),
        (
            "sort-key role without an ordering",
            value_key("k", KeyRole::SortKey, KeyScope::Global, Vec::new()),
            same.clone(),
            Vec::new(),
        ),
        (
            "null value",
            value_key("k", KeyRole::Unique, KeyScope::Global, Vec::new()),
            vec![Some(RefValue::Null), Some(RefValue::Null)],
            Vec::new(),
        ),
        (
            "primary key without a value",
            value_key("k", KeyRole::Primary, KeyScope::Global, Vec::new()),
            vec![Some(RefValue::Int(1)), None],
            Vec::new(),
        ),
        (
            "scoped key outside its domain",
            value_key(
                "k",
                KeyRole::Unique,
                KeyScope::Within("b".to_owned()),
                Vec::new(),
            ),
            same,
            vec![("b", "a")],
        ),
    ];
    let mut disagreements = Vec::new();
    for (what, key, values, edges) in cases {
        let pair = keyed_graph_pair(&["a", "b"], &edges, vec![key]);
        let source = valued_a_nodes(&values);
        let production = production_put(&pair, &source, &source);
        let reference = reference_strict_put(&pair, &source, &source);
        if production != reference {
            disagreements.push(format!("{what}: {production:?} != {reference:?}"));
        }
    }
    assert!(disagreements.is_empty(), "{disagreements:#?}");
}

/// A lens whose source schema's declarations are ill formed has no plan on
/// either side: get refuses with the source declaration errors, in the
/// reference's order and before any edge-image refusal, and the target's
/// are reported only when the source's are well formed.
#[test]
fn ill_formed_key_declarations_refuse_the_plan_like_the_reference() {
    use panproto_reference::keys::KeyEquality as Eq;
    use panproto_reference::schema::{KeyRole, KeyScope};
    let sort_key = value_key("sorted", KeyRole::SortKey, KeyScope::Global, Vec::new());
    let mut arity = value_key("arity", KeyRole::Unique, KeyScope::Global, Vec::new());
    arity.equality = vec![Eq::Representation, Eq::NumericValue];
    let source = valued_a_nodes(&[Some(RefValue::Int(1))]);
    let cases = [
        (
            vec![sort_key.clone()],
            "SourceDeclarations SortKeyNotOrdered a sorted",
        ),
        (
            vec![arity.clone()],
            "SourceDeclarations EqualityArity a arity 1 2",
        ),
        (
            vec![arity, sort_key],
            "SourceDeclarations EqualityArity a arity 1 2, SortKeyNotOrdered a sorted",
        ),
    ];
    for (keys, expected) in cases {
        let pair = keyed_graph_pair(&["a"], &[], keys);
        let production = production_get(&pair, &source);
        let reference = reference_get(&pair, &source);
        assert_eq!(production, reference, "{expected}");
        assert_eq!(production, Err(Reason::Restrict(expected.to_owned())));
    }

    // Source declarations well formed, target's not: the target's errors.
    let mut target = panproto_reference::enumerate::lenses::bounded_lens_schema(&["a"], &[]);
    target.keys.insert(
        "a".to_owned(),
        vec![value_key(
            "sorted",
            KeyRole::SortKey,
            KeyScope::Global,
            Vec::new(),
        )],
    );
    let reference_source = panproto_reference::enumerate::lenses::bounded_lens_schema(&["a"], &[]);
    let mut production_target = prod_schema(
        &panproto_reference::enumerate::lenses::bounded_lens_schema(&["a"], &[]),
    );
    production_target
        .declarations
        .keys
        .insert("a".into(), target.keys["a"].iter().map(prod_key).collect());
    let migration = reference_carrier_migration(&reference_source);
    let lens = Lens {
        compiled: prod_migration(&migration),
        src_schema: prod_schema(&reference_source),
        tgt_schema: production_target,
    };
    let pair = Pair::with_production(reference_source, target, migration, &NoValueLayer, lens);
    let production = production_get(&pair, &source);
    assert_eq!(production, reference_get(&pair, &source));
    assert_eq!(
        production,
        Err(Reason::Restrict(
            "TargetDeclarations SortKeyNotOrdered a sorted".to_owned()
        ))
    );
}

fn assert_graph_malformed_fan_domain_family() -> u16 {
    let fan_schema = bounded_lens_fan_schema();
    let fan_pair = Pair::new(
        fan_schema.clone(),
        fan_schema.clone(),
        reference_carrier_migration(&fan_schema),
    );
    let labels = panproto_reference::enumerate::lenses::bounded_lens_fan_labels();
    let mut hyper = RefNode::new(0, "h");
    hyper.shape = RefNodeShape::HyperEdge;
    let fan_source = RefGraph {
        nodes: BTreeMap::from([
            (0, hyper),
            (1, RefNode::new(1, "a")),
            (2, RefNode::new(2, "b")),
        ]),
        edges: BTreeMap::from([
            (
                0,
                RefGEdge {
                    src: 0,
                    tgt: 1,
                    label: labels[0].clone(),
                    position: None,
                },
            ),
            (
                1,
                RefGEdge {
                    src: 0,
                    tgt: 2,
                    label: labels[1].clone(),
                    position: None,
                },
            ),
        ]),
    };
    let mut malformed = fan_source.clone();
    malformed.edges.remove(&0);
    assert_graph_put_domain_outcome(
        &fan_pair,
        &fan_source,
        &malformed,
        GraphPutDomainFamily::MalformedFan,
    )
}

fn assert_graph_validation_domain_families() -> u16 {
    assert_graph_missing_required_cell_domain_family()
        | assert_graph_duplicate_key_domain_family()
        | assert_graph_malformed_fan_domain_family()
}

#[test]
fn graph_validation_domain_boundaries_agree_with_the_reference() {
    let observed = assert_graph_validation_domain_families();
    let expected = [
        GraphPutDomainFamily::DuplicateKey,
        GraphPutDomainFamily::MissingRequiredCell,
        GraphPutDomainFamily::MalformedFan,
    ]
    .into_iter()
    .fold(0_u16, |bits, family| {
        bits | graph_put_domain_family_bit(family.name())
    });
    assert_eq!(observed, expected);
}

fn assert_graph_put_domain_family(family: GraphPutDomainFamily) -> u16 {
    let expected = graph_put_domain_family_bit(family.name());
    let observed = match family {
        GraphPutDomainFamily::SurvivalPredicateFails
        | GraphPutDomainFamily::SurvivalPredicateRefused => {
            assert_graph_predicate_domain_families()
        }
        GraphPutDomainFamily::TransformNotInvertible => {
            assert_graph_noninvertible_transform_domain_family()
        }
        GraphPutDomainFamily::DuplicateKey
        | GraphPutDomainFamily::MissingRequiredCell
        | GraphPutDomainFamily::MalformedFan => assert_graph_validation_domain_families(),
        _ => {
            let (_, stage, source, edited) = bounded_lens_graph_domain_witnesses()
                .iter()
                .find(|(candidate, _, _, _)| *candidate == family)
                .unwrap_or_else(|| panic!("the witness search records no {family:?} witness"));
            let pair = Pair::from_stage(stage);
            assert_graph_put_domain_outcome(&pair, source, edited, family)
        }
    };
    assert_eq!(
        observed & expected,
        expected,
        "the {family:?} dispatcher did not exercise its named refusal"
    );
    expected
}

#[test]
fn the_cascade_agrees_with_the_reference_on_the_delta_fragment_scope() {
    let certificate = delta_fragment_count_certificate();
    let mut sources = [0_usize; 2];
    for_each_delta_fragment_source(|fixture, _| {
        sources[usize::from(fixture == DeltaFragmentFixture::VisibleThenHidden)] += 1;
    });
    assert_eq!(sources, certificate.sources);

    let mut cases = Vec::new();
    for_each_delta_fragment_deletion_edit(|fixture, source, edited| {
        cases.push((fixture, source.clone(), edited.clone()));
    });
    let edits: usize = certificate.deletion_edits.iter().sum();
    assert_eq!(cases.len(), edits);
    let probes: usize = certificate.source_subinstance_probes.iter().sum();
    let pairs = [
        delta_fragment_pair(DeltaFragmentFixture::DropTarget),
        delta_fragment_pair(DeltaFragmentFixture::VisibleThenHidden),
    ];
    let pair_of = |fixture: DeltaFragmentFixture| {
        &pairs[usize::from(fixture == DeltaFragmentFixture::VisibleThenHidden)]
    };
    let bound = format!(
        "Delta fragment: fixtures=2, nodes<=3, edges<=3, sources={}, deletion_edits={edits}, source_subinstance_probes={probes}",
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
            let (production, _, universal) =
                delta_fragment_cascade_outcome(pair_of(*fixture), source, edited);
            (production.and_then(|outcome| outcome.put.ok()), universal)
        },
        |(fixture, source, edited)| {
            let (_, largest, _) = delta_fragment_cascade_outcome(pair_of(*fixture), source, edited);
            (largest, true)
        },
        |production, reference| production == reference,
    ));
    assert_eq!(cascade, u64::try_from(edits).unwrap());
}

#[test]
fn graph_put_domain_and_cascade_law_certificate() {
    let mut observed_domains = 0_u16;
    let mut domain_cases = 0_usize;
    let mut carriers = Vec::new();
    let mut cascades = Vec::new();
    let mut cases = 0_usize;
    for_each_graph_put_cascade_law_case(|case| {
        cases += 1;
        match case {
            GraphPutCascadeLawCase::Domain(family) => {
                domain_cases += 1;
                observed_domains |= assert_graph_put_domain_family(family);
            }
            GraphPutCascadeLawCase::Carrier { stage, source } => {
                carriers.push((stage, source));
            }
            GraphPutCascadeLawCase::Cascade {
                fixture,
                source,
                edited,
            } => cascades.push((fixture, source, edited)),
        }
    });
    assert_eq!(cases, graph_put_cascade_law_case_count_certificate());
    assert_eq!(cases, 145_013);
    assert_eq!(carriers.len(), 144_397);
    assert_eq!(cascades.len(), 604);
    assert_eq!(domain_cases, GraphPutDomainFamily::ALL.len());
    let expected_domains = GraphPutDomainFamily::ALL
        .into_iter()
        .fold(0_u16, |bits, family| {
            bits | graph_put_domain_family_bit(family.name())
        });
    assert_eq!(
        observed_domains, expected_domains,
        "the graph-domain variants did not dispatch every named family"
    );

    let _ = graph_lens_agrees_with_the_reference_on_cases(&carriers);

    let pairs = [
        delta_fragment_pair(DeltaFragmentFixture::DropTarget),
        delta_fragment_pair(DeltaFragmentFixture::VisibleThenHidden),
    ];
    for (fixture, source, edited) in &cascades {
        let pair = &pairs[usize::from(*fixture == DeltaFragmentFixture::VisibleThenHidden)];
        assert_eq!(
            production_put(pair, source, edited),
            reference_strict_put(pair, source, edited),
            "Delta-fragment put at {fixture:?}"
        );
        let (production, largest, universal) = delta_fragment_cascade_outcome(pair, source, edited);
        assert_eq!(
            production.and_then(|outcome| outcome.put.ok()),
            largest,
            "largest exact preimage at {fixture:?}"
        );
        assert!(universal, "cascade universal property at {fixture:?}");
    }
}

#[test]
fn graph_get_agrees_with_the_reference_on_the_delta_fragment_sources() {
    let pairs = [
        delta_fragment_pair(DeltaFragmentFixture::DropTarget),
        delta_fragment_pair(DeltaFragmentFixture::VisibleThenHidden),
    ];
    let mut cases = Vec::new();
    for_each_delta_fragment_source(|fixture, source| cases.push((fixture, source.clone())));
    let bound = format!("Delta-fragment sources={}", cases.len());
    println!("{bound}");
    let compared = assert_agree(agree(
        &bound,
        cases.iter(),
        |(fixture, source)| {
            production_get(
                &pairs[usize::from(*fixture == DeltaFragmentFixture::VisibleThenHidden)],
                source,
            )
        },
        |(fixture, source)| {
            reference_get(
                &pairs[usize::from(*fixture == DeltaFragmentFixture::VisibleThenHidden)],
                source,
            )
        },
        |production, reference| production == reference,
    ));
    assert_eq!(compared, 133);
    // GetPut through the strict reference put on every Delta-fragment source.
    for (fixture, source) in &cases {
        let pair = &pairs[usize::from(*fixture == DeltaFragmentFixture::VisibleThenHidden)];
        let (view, _) = reference_get(pair, source).expect("Delta-fragment get is total");
        let production = production_put(pair, source, &view).expect("production get");
        let reference = reference_strict_put(pair, source, &view).expect("reference get");
        assert_eq!(production, reference);
        assert_eq!(reference.put, Ok(source.clone()));
    }
}

// ---------------------------------------------------------------------------
// Tables: get, strict put and refusals on the L-lenses-8 fixture scope, and
// the last-writer rule on the writer carrier
// ---------------------------------------------------------------------------

use panproto_inst::{BoundaryProtocols, FInstance, RowId, TableComplement};
use panproto_lens::table_lens;
use panproto_reference::enumerate::lenses::{
    DerivedWriterFixture, TABLE_FIXTURE_TABLE_KIND, TABLE_FIXTURE_VALUE_KIND,
    TableComplementConstruction, TableComplementLawCase, TableFixture, TableFixturePutCase,
    TableGetPutLawCase, TableGetRefusalCase, TablePairPutCase, TablePutRefusalCase,
    TableWriterEditCase, WRITER_EDITS, WRITER_VALUE_KIND, WriterFixture,
    derived_writer_count_certificate, for_each_table_complement_construction,
    for_each_table_complement_law_case, for_each_table_fixture, for_each_table_get_put_law_case,
    for_each_writer_fixture, table_complement_construction_count_certificate,
    table_complement_law_count_certificate, table_fixture_count_certificate,
    table_fixture_get_put_count_certificate, table_get_put_law_count_certificate,
    table_get_refusal_count_certificate, table_pair_put_count_certificate,
    table_put_refusal_count_certificate, table_view_edits, table_view_edits_count,
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
        original_absent_cells: complement
            .original_absent_cells
            .iter()
            .map(|(id, column)| (id.0, column.clone()))
            .collect(),
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

    fn from_stage(stage: &BoundedLensStage) -> Self {
        let layer: &'static dyn ValueLayer = match stage.layer {
            BoundedLensLayer::None => &NoValueLayer,
            BoundedLensLayer::SurvivalFalse => &BoundedLensSurvivalFalse,
            BoundedLensLayer::AddField => &BoundedLensAddField,
            BoundedLensLayer::DropField => &BoundedLensDropField,
            BoundedLensLayer::ReadField => &BoundedLensReadField,
            BoundedLensLayer::OpTerm => &BoundedLensOpTerm,
        };
        let kinds = std::collections::BTreeSet::from(["obj".to_owned()]);
        let value_kinds: BTreeMap<String, ScalarKind> = stage
            .source
            .vertices
            .values()
            .chain(stage.target.vertices.values())
            .filter(|kind| kind.as_str() == "int")
            .map(|kind| {
                (
                    kind.clone(),
                    ScalarKind::Int {
                        bits: 64,
                        signed: true,
                    },
                )
            })
            .collect();
        let values = value_kinds.keys().cloned().collect();
        let classify =
            |schema: &RefSchema| panproto_reference::rel::classify(schema, &values, &kinds);
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
                value_kinds,
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
            // The reference compiles the entity functor before the restrict
            // plan, so an incoherent kept relation is reported by the functor;
            // production reports the same edge and image from its plan.
            TablePlanError::Functor(error) => match error.as_ref() {
                panproto_reference::rel::FunctorError::IncoherentEdgeImage { edge, image } => {
                    Reason::Restrict(format!("IncoherentEdgeImage {edge:?} {image:?}"))
                }
                other => Reason::Restrict(format!("reference table functor refusal {other:?}")),
            },
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
            pair.layer,
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

struct ConditionalGraphPredicate;

impl ValueLayer for ConditionalGraphPredicate {
    fn canonical_semantics(&self) -> ValueLayerSemantics {
        ValueLayerSemantics {
            domain: "panproto-reference/graph-put-domain/conditional-predicate",
            version: 1,
            canonical_payload: Vec::new(),
        }
    }

    fn survives(&self, _: &RefGraph, node: &RefNode) -> Result<bool, RefPredicateRefusal> {
        match node.fields.get("keep") {
            Some(RefValue::Bool(value)) => Ok(*value),
            Some(value) => Err(RefPredicateRefusal::NotBoolean {
                got: match value {
                    RefValue::Int(_) => "int",
                    RefValue::Str(_) => "string",
                    RefValue::Null => "null",
                    _ => "other",
                }
                .to_owned(),
            }),
            None => Ok(false),
        }
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

fn assert_table_complement_field_order() {
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
    assert_eq!(production_field_order, RefTableComplement::FIELD_ORDER[..9]);
    let extended = TableComplement {
        original_absent_cells: BTreeSet::from([(RowId(0), "f".to_owned())]),
        ..TableComplement::empty()
    };
    let extended = serde_json::to_value(extended).expect("absence residue is serializable");
    assert_eq!(
        extended
            .as_object()
            .expect("a complement is an object")
            .len(),
        10
    );
    assert_eq!(
        extended["original_absent_cells"],
        serde_json::json!([[0, "f"]])
    );
}

fn assert_table_fixture_complement(fixture: &TableFixture) {
    let pair = fixture_pair(fixture);
    let production = production_table_get(&pair, &fixture.source)
        .expect("the production projection fixture gets");
    let reference =
        reference_table_get(&pair, &fixture.source).expect("the reference projection fixture gets");
    assert_eq!(production, reference, "table projection complement");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TableComplementCoordinate {
    DroppedRows,
    DroppedCells,
    DroppedPairs,
    OriginalEdges,
    OriginalTables,
    OriginalCells,
    SynthesizedPairs,
}

#[derive(Debug, Default)]
struct TableComplementCoordinateCensus {
    observed: BTreeSet<TableComplementCoordinate>,
}

impl TableComplementCoordinateCensus {
    const ALL: [TableComplementCoordinate; 7] = [
        TableComplementCoordinate::DroppedRows,
        TableComplementCoordinate::DroppedCells,
        TableComplementCoordinate::DroppedPairs,
        TableComplementCoordinate::OriginalEdges,
        TableComplementCoordinate::OriginalTables,
        TableComplementCoordinate::OriginalCells,
        TableComplementCoordinate::SynthesizedPairs,
    ];

    fn observe(&mut self, complement: &RefTableComplement) {
        let coordinates = [
            (
                TableComplementCoordinate::DroppedRows,
                !complement.dropped_rows.is_empty(),
            ),
            (
                TableComplementCoordinate::DroppedCells,
                !complement.dropped_cells.is_empty(),
            ),
            (
                TableComplementCoordinate::DroppedPairs,
                !complement.dropped_fk_pairs.is_empty(),
            ),
            (
                TableComplementCoordinate::OriginalEdges,
                !complement.original_fk_edges.is_empty(),
            ),
            (
                TableComplementCoordinate::OriginalTables,
                !complement.original_tables.is_empty(),
            ),
            (
                TableComplementCoordinate::OriginalCells,
                !complement.original_cells.is_empty(),
            ),
            (
                TableComplementCoordinate::SynthesizedPairs,
                !complement.synthesized_fk_pairs.is_empty(),
            ),
        ];
        self.observed.extend(
            coordinates
                .into_iter()
                .filter_map(|(coordinate, present)| present.then_some(coordinate)),
        );
    }

    fn merge(&mut self, other: Self) {
        self.observed.extend(other.observed);
    }

    fn assert_complete(self) {
        assert_eq!(self.observed, BTreeSet::from(Self::ALL));
    }
}

fn assert_table_complement_construction(
    construction: &TableComplementConstruction,
) -> TableComplementCoordinateCensus {
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
    let mut census = TableComplementCoordinateCensus::default();
    census.observe(&reference.1);

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
    census
}

fn table_refusal_pair(case: TableGetRefusalCase) -> TablePair {
    let fixture = case.fixture();
    let mut structural = fixture.migration.clone();
    structural.conditional.clear();
    structural.transformed.clear();
    structural.output_fields.clear();
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

fn assert_table_fixture_put_case(case: &TableFixturePutCase) {
    let pair = fixture_pair(&case.fixture);
    let production_get = production_table_get(&pair, &case.fixture.source)
        .expect("the projection fixture gets in production");
    let reference_get = reference_table_get(&pair, &case.fixture.source)
        .expect("the projection fixture gets in the reference");
    assert_eq!(production_get, reference_get);
    assert_eq!(
        production_table_put(&pair, &case.fixture.source, &case.view),
        reference_table_put(&pair, &case.fixture.source, &case.view),
        "projection put for an enumerated view"
    );
}

fn assert_table_writer_edit_case(case: &TableWriterEditCase) {
    let pair = writer_pair(&case.fixture);
    let source = case.fixture.source();
    let (mut production_view, _) =
        production_table_get(&pair, &source).expect("production writer get");
    production_view
        .rows
        .get_mut(&0)
        .expect("writer fixture row zero")
        .1
        .insert(case.target.to_owned(), RefValue::Int(case.edit));
    let production = production_table_put(&pair, &source, &production_view).put;
    let reference = pair.with_reference(|lens, plan| {
        let (mut view, complement) = lens.get(&source).expect("reference writer get");
        view.rows
            .get_mut(&0)
            .expect("writer fixture row zero")
            .1
            .insert(case.target.to_owned(), RefValue::Int(case.edit));
        let writer = case
            .fixture
            .expected_writer(case.target)
            .expect("an image column has a certified writer")
            .to_owned();
        assert_eq!(
            panproto_reference::table::TablePlan::writer(plan, "t", case.target),
            Some(writer.clone()),
            "the reference plan realizes the enumerated last writer"
        );
        let mut expected = source.clone();
        expected
            .rows
            .get_mut(&0)
            .expect("writer fixture row zero")
            .1
            .insert(writer, RefValue::Int(case.edit));
        let restored = RefTableLens::put(lens, &view, &complement)
            .map_err(|error| ref_table_put_reason(&error));
        assert_eq!(restored.as_ref(), Ok(&expected));
        restored
    });
    assert_eq!(production, reference, "last-writer edit {case:?}");
}

fn assert_derived_writer_case(fixture: DerivedWriterFixture) {
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
    migration
        .output_fields
        .insert("t".to_owned(), vec!["x".to_owned()]);
    let mut structural = migration.clone();
    structural.transformed.clear();
    structural.output_fields.clear();
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
    let mut field_orders = 0;
    let mut constructions = 0;
    let mut refusals = 0;
    let mut coordinates = TableComplementCoordinateCensus::default();
    for_each_table_complement_law_case(|case| {
        cases += 1;
        match case {
            TableComplementLawCase::Fixture(fixture) => {
                fixtures += 1;
                assert_table_fixture_complement(&fixture);
            }
            TableComplementLawCase::FieldOrder => {
                field_orders += 1;
                assert_table_complement_field_order();
            }
            TableComplementLawCase::Construction(construction) => {
                constructions += 1;
                let observed = assert_table_complement_construction(&construction);
                coordinates.merge(observed);
            }
            TableComplementLawCase::Refusal(case) => {
                refusals += 1;
                assert_table_get_refusal_case(case);
            }
        }
    });
    assert_eq!(cases, table_complement_law_count_certificate());
    assert_eq!(fixtures, table_fixture_count_certificate().total);
    assert_eq!(field_orders, 1);
    assert_eq!(
        constructions,
        table_complement_construction_count_certificate()
    );
    assert_eq!(refusals, table_get_refusal_count_certificate());
    assert_eq!(cases, 9_938);
    coordinates.assert_complete();
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
            TableGetPutLawCase::Fixture(case) => {
                fixtures += 1;
                assert_table_fixture_put_case(&case);
            }
            TableGetPutLawCase::Writer(case) => {
                writers += 1;
                assert_table_writer_edit_case(&case);
            }
            TableGetPutLawCase::Derived(case) => {
                derived += 1;
                assert_derived_writer_case(case);
            }
            TableGetPutLawCase::Pair(case) => {
                pairs += 1;
                assert_table_pair_put_case(case);
            }
            TableGetPutLawCase::Refusal(case) => {
                refusals += 1;
                assert_table_put_refusal_case(case);
            }
        }
    });
    assert_eq!(cases, table_get_put_law_count_certificate());
    assert_eq!(fixtures, table_fixture_get_put_count_certificate());
    assert_eq!(writers, writer_count_certificate().2);
    assert_eq!(derived, derived_writer_count_certificate());
    assert_eq!(pairs, table_pair_put_count_certificate());
    assert_eq!(refusals, table_put_refusal_count_certificate());
    assert_eq!(cases, 156_704);
}

// ---------------------------------------------------------------------------
// The five law gates (L-lenses-8): production checker against the reference checker
// ---------------------------------------------------------------------------

use panproto_lens::LawViolation;
use panproto_reference::lens::{LawGateStatus, LawReport as RefLawReport};

/// The comparable part of a native five-gate report: the first gate that does
/// not pass, or `None` when every gate passes. L-lenses-8 retains this own-basis
/// comparison and separately reruns both checkers on one family-labelled
/// union of their independently generated candidates.
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
fn graph_laws_agree_with_the_reference_on_the_delta_fragment_sources() {
    let pairs = [
        delta_fragment_pair(DeltaFragmentFixture::DropTarget),
        delta_fragment_pair(DeltaFragmentFixture::VisibleThenHidden),
    ];
    let mut cases = Vec::new();
    for_each_delta_fragment_source(|fixture, source| cases.push((fixture, source.clone())));
    let bound = format!("Delta-fragment law reports: sources={}", cases.len());
    println!("{bound}");
    let compared = assert_agree(agree(
        &bound,
        cases.iter(),
        |(fixture, source)| {
            let pair = &pairs[usize::from(*fixture == DeltaFragmentFixture::VisibleThenHidden)];
            production_law_outcome(&graph_lens::check_laws(
                &pair.production,
                &prod_graph(source),
            ))
        },
        |(fixture, source)| {
            let pair = &pairs[usize::from(*fixture == DeltaFragmentFixture::VisibleThenHidden)];
            pair.with_reference(|lens| {
                reference_law_outcome(&panproto_reference::lens::check_graph_laws(
                    lens.expect("Delta-fragment plans compile"),
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
// The bounded lens state scope: every stage, every state over its source schema and
// every one-step edit of its view
// ---------------------------------------------------------------------------

use panproto_reference::enumerate::lenses::{
    bounded_lens_fan_schema, bounded_lens_fan_state_count, bounded_lens_fan_states,
    bounded_lens_graph_edit_count, bounded_lens_graph_edits, bounded_lens_graph_state_count,
    bounded_lens_graph_states, bounded_lens_path_count_certificate, bounded_lens_stages,
    bounded_lens_table_state_count, bounded_lens_table_states,
    bounded_lens_tree_square_count_certificate, bounded_lens_tree_state_count, sigma2_count,
    sigma2_schemas,
};

/// The states a stage reads: the fan states for a stage over the fan
/// schema, otherwise the graph states over its source schema.
fn bounded_lens_sources(stage: &BoundedLensStage) -> Vec<RefGraph> {
    if stage.source == bounded_lens_fan_schema() {
        bounded_lens_fan_states()
    } else {
        bounded_lens_graph_states(&stage.source)
    }
}

/// One case of the bounded lens graph scope: the get of a stage on a state, or the
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
fn bounded_lens_stage_sources(stages: &[BoundedLensStage]) -> Vec<Vec<RefGraph>> {
    stages
        .iter()
        .map(|stage| {
            let sources = bounded_lens_sources(stage);
            let certified = if stage.source == bounded_lens_fan_schema() {
                bounded_lens_fan_state_count()
            } else {
                bounded_lens_graph_state_count(&stage.source)
            };
            assert_eq!(sources.len(), certified, "stage {}", stage.name);
            sources
        })
        .collect()
}

/// The cases of one `(stage, state)` unit: the unedited view and its
/// one-step edits (when the reference get succeeds), then the get.
fn bounded_lens_graph_unit_cases<'a>(
    pairs: &[Pair],
    stages: &[BoundedLensStage],
    stage: usize,
    source: &'a RefGraph,
) -> Vec<GraphCase<'a>> {
    let mut cases = Vec::new();
    if let Ok((view, complement)) = reference_get(&pairs[stage], source) {
        let edits = bounded_lens_graph_edits(&view, &stages[stage].target, &complement);
        assert_eq!(
            edits.len(),
            bounded_lens_graph_edit_count(&view, &stages[stage].target, &complement),
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

fn graph_put_domain_family_bit(reason: &str) -> u16 {
    GraphPutDomainFamily::ALL
        .into_iter()
        .position(|family| reason.starts_with(family.name()))
        .map_or(0, |index| 1 << index)
}

fn graph_lens_agrees_with_the_reference_on_cases(units: &[(usize, RefGraph)]) -> u16 {
    let schemas = sigma2_schemas();
    assert_eq!(schemas.len(), sigma2_count());
    let stages = bounded_lens_stages();
    let certificate = bounded_lens_path_count_certificate();
    assert_eq!(stages.len(), certificate.stages);
    assert_eq!(units.len(), certificate.single_stage_states);
    let bound = format!(
        "bounded lens graph scope: schemas={}, stages={}, states={}",
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
    let observed_domain_families = std::sync::atomic::AtomicU16::new(0);
    let compared = assert_agree(agree_in_parallel(
        &bound,
        units,
        || stages.iter().map(Pair::from_stage).collect::<Vec<Pair>>(),
        |pairs, (stage, source)| {
            let cases = bounded_lens_graph_unit_cases(pairs, &stages, *stage, source);
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
                if let Some(PutOutcome {
                    domain: Err(reasons),
                    ..
                }) = &module
                {
                    for reason in reasons {
                        observed_domain_families.fetch_or(
                            graph_put_domain_family_bit(reason),
                            std::sync::atomic::Ordering::Relaxed,
                        );
                    }
                }
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
        "bounded lens graph cases: gets={}, unedited views={views}, one-step edits={edits}, cases={total}",
        units.len()
    );
    assert_eq!(compared, u64::try_from(total).unwrap());
    observed_domain_families.into_inner()
}

// ---------------------------------------------------------------------------
// Trees: get, GetPut and the five-gate reports on the bounded lens tree states
// ---------------------------------------------------------------------------

use panproto_inst::{Fan, WInstance};
use panproto_reference::enumerate::lenses::bounded_lens_tree_states;
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

fn tree_synth_index_pair() -> Pair {
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
    Pair::new(source, target, migration)
}

fn tree_synth_source(case: &TreeRestrictCase) -> RefTree {
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
    let mut source = RefTree {
        nodes: BTreeMap::from([(root, RefNode::new(root, "a"))]),
        arcs: Vec::new(),
        fans: Vec::new(),
        root,
    };
    for (&id, route) in ids.iter().zip(&case.routes) {
        let (anchor, edge) = match route {
            TreeRoute::Keep => ("d", RefSEdge::new("a", "d", "prop", Some("keep"))),
            TreeRoute::Expand => ("c", RefSEdge::new("a", "c", "prop", Some("expand"))),
        };
        source.nodes.insert(id, RefNode::new(id, anchor));
        source.arcs.push((root, id, edge));
    }
    source
}

fn assert_tree_synth_success(
    pair: &Pair,
    label: &str,
    reference_source: &RefTree,
    expected_view: &RefTree,
    expected_complement: &RefComplement,
    actual_view: &panproto_inst::WInstance,
    actual_complement: &panproto_inst::Complement,
) -> BTreeSet<&'static str> {
    assert_eq!(&ref_tree(actual_view), expected_view, "{label}");
    let production_source = prod_tree(reference_source);
    let (source_graph, source_embedding) =
        GInstance::from_wtype_embedded(&production_source, Some(&pair.production.src_schema))
            .unwrap();
    let (graph_view, graph_complement) = graph_lens::get(&pair.production, &source_graph).unwrap();
    let (tree_from_graph, view_embedding) = graph_view
        .to_wtype_embedded(&pair.production.tgt_schema)
        .unwrap();
    assert_eq!(
        ref_tree(&tree_from_graph),
        ref_tree(actual_view),
        "graph/tree view for {label}"
    );
    let converted_complement = panproto_lens::tree_complement_to_graph(
        &pair.production,
        &source_graph,
        &graph_view,
        actual_complement,
        &source_embedding,
        &view_embedding,
    )
    .unwrap();
    assert_eq!(
        ref_complement(&converted_complement),
        ref_complement(&graph_complement),
        "converted graph complement for {label}"
    );
    assert_eq!(
        converted_complement.identity, graph_complement.identity,
        "converted graph identity for {label}"
    );
    assert_eq!(
        ref_complement(&converted_complement),
        without_identity(expected_complement),
        "converted reference complement for {label}"
    );
    let tree_index = converted_complement.synth_index();
    let graph_index = graph_complement.synth_index();
    assert_eq!(tree_index, graph_index, "full synth index for {label}");
    assert_eq!(
        tree_index
            .nodes
            .iter()
            .map(|(id, key)| (*id, ref_synth_key(key)))
            .collect::<BTreeMap<_, _>>(),
        expected_complement.synthesized_nodes,
        "synthesized nodes for {label}"
    );
    assert_eq!(
        tree_index
            .edges
            .iter()
            .map(|(id, key)| (id.0, ref_synth_key(key)))
            .collect::<BTreeMap<_, _>>(),
        expected_complement.synthesized_edges,
        "synthesized edges for {label}"
    );
    assert_eq!(
        graph_complement.id_floor,
        IdFloor {
            node: expected_complement.floor.0,
            edge: expected_complement.floor.1,
        },
        "graph floors for {label}"
    );
    assert_eq!(
        (actual_complement.id_floor, source_embedding.next_edge),
        expected_complement.floor,
        "tree node floor and source-embedding edge floor for {label}"
    );
    tree_index
        .nodes
        .values()
        .chain(tree_index.edges.values())
        .filter_map(|key| match key {
            SynthKey::Contract { .. } => Some("contract"),
            SynthKey::Expand { .. } => Some("expand"),
            SynthKey::ContractExpand { .. } => Some("contract-expand"),
            SynthKey::ResolutionReference { .. } | SynthKey::ResolutionSpan { .. } => None,
        })
        .collect()
}

fn tree_synth_case_succeeds(
    pair: &Pair,
    reference: &RefGraphLens<'_>,
    case: &TreeRestrictCase,
) -> bool {
    let reference_source = tree_synth_source(case);
    let production_source = prod_tree(&reference_source);
    let expected = panproto_reference::lens::tree_get(reference, &reference_source)
        .expect("the carrier tree has a graph form");
    let actual = panproto_lens::get(&pair.production, &production_source);
    match (expected, actual) {
        (Ok((expected_view, expected_complement, _)), Ok((actual_view, actual_complement))) => {
            let label = format!("{case:?}");
            let _ = assert_tree_synth_success(
                pair,
                &label,
                &reference_source,
                &expected_view,
                &expected_complement,
                &actual_view,
                &actual_complement,
            );
            true
        }
        (
            Err(RefRestrictError::At {
                error: EngineError::IdsExhausted { nodes: true },
                ..
            }),
            Err(LensError::Restrict(RestrictError::IdsExhausted {
                kind: panproto_inst::segment::IdKind::Node,
            })),
        ) => false,
        (expected, actual) => {
            panic!("tree synthesis mismatch for {case:?}: {expected:?} / {actual:?}")
        }
    }
}

fn contraction_synth_pair(expand: bool) -> Pair {
    let source = RefSchema::default()
        .vertex("a", "record")
        .vertex("x", "record")
        .vertex("c", "record")
        .edge("a", "x", "prop", Some("ax"))
        .edge("x", "c", "prop", Some("xc"));
    let target = if expand {
        RefSchema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .vertex("c", "record")
            .edge("a", "b", "prop", Some("ab"))
            .edge("b", "c", "prop", Some("bc"))
    } else {
        RefSchema::default()
            .vertex("a", "record")
            .vertex("c", "record")
            .edge("a", "c", "prop", Some("ac"))
    };
    let mut migration = RefMigration {
        surviving_verts: ["a", "c"].into_iter().map(str::to_owned).collect(),
        ..RefMigration::default()
    };
    if expand {
        migration
            .expansion_path
            .insert(("a".to_owned(), "c".to_owned()), vec!["b".to_owned()]);
    }
    Pair::new(source, target, migration)
}

fn contraction_synth_source(floor: TreeFloorCase) -> RefTree {
    let [root, interior, target] = match floor {
        TreeFloorCase::Compact => [0, 1, 2],
        TreeFloorCase::Gapped => [2, 10, 20],
        TreeFloorCase::LastFree => [0, 1, u32::MAX - 2],
        TreeFloorCase::Saturated | TreeFloorCase::AlreadyMax => {
            panic!("requested a refusing floor for a successful synthesis witness")
        }
    };
    RefTree {
        nodes: BTreeMap::from([
            (root, RefNode::new(root, "a")),
            (interior, RefNode::new(interior, "x")),
            (target, RefNode::new(target, "c")),
        ]),
        arcs: vec![
            (root, interior, RefSEdge::new("a", "x", "prop", Some("ax"))),
            (
                interior,
                target,
                RefSEdge::new("x", "c", "prop", Some("xc")),
            ),
        ],
        fans: Vec::new(),
        root,
    }
}

fn tree_synth_variants(pair: &Pair, source: &RefTree, label: &str) -> BTreeSet<&'static str> {
    pair.with_reference(|reference| {
        let reference = reference.expect("the structural synthesis lens compiles");
        let (expected_view, expected_complement, _) =
            panproto_reference::lens::tree_get(reference, source)
                .expect("the source has a graph form")
                .expect("the structural synthesis succeeds");
        let (actual_view, actual_complement) =
            panproto_lens::get(&pair.production, &prod_tree(source))
                .expect("production structural synthesis succeeds");
        assert_tree_synth_success(
            pair,
            label,
            source,
            &expected_view,
            &expected_complement,
            &actual_view,
            &actual_complement,
        )
    })
}

#[test]
fn tree_restrict_synth_index_matches_graph_and_reference_on_the_70_case_carrier() {
    let pair = tree_synth_index_pair();
    let cases = tree_restrict_cases();

    pair.with_reference(|reference| {
        let reference = reference.expect("the release tree lens compiles");
        let mut successful = 0usize;
        for case in &cases {
            if tree_synth_case_succeeds(&pair, reference, case) {
                successful += 1;
            }
        }
        assert_eq!(cases.len(), 70);
        assert_eq!(successful, 43);
        assert_eq!(cases.len() - successful, 27);
    });
}

#[test]
fn tree_restrict_synth_index_covers_all_structural_keys_and_successful_floors() {
    let expansion_pair = tree_synth_index_pair();
    let expansion_source = tree_synth_source(&TreeRestrictCase {
        routes: vec![TreeRoute::Expand],
        floor: TreeFloorCase::Compact,
    });
    let mut variants = tree_synth_variants(&expansion_pair, &expansion_source, "direct expansion");

    let contract_pair = contraction_synth_pair(false);
    variants.extend(tree_synth_variants(
        &contract_pair,
        &contraction_synth_source(TreeFloorCase::Compact),
        "direct contraction",
    ));

    let contract_expand_pair = contraction_synth_pair(true);
    for floor in [
        TreeFloorCase::Compact,
        TreeFloorCase::Gapped,
        TreeFloorCase::LastFree,
    ] {
        variants.extend(tree_synth_variants(
            &contract_expand_pair,
            &contraction_synth_source(floor),
            &format!("contract expansion at {floor:?}"),
        ));
    }
    assert_eq!(
        variants,
        BTreeSet::from(["contract", "contract-expand", "expand"])
    );
}

/// Tree get on both sides, with `GetPut`: the view, the mapped residue and
/// the put of the unedited view. `None` outside the reference tree lens's
/// domain (a stage dropping an edge between surviving vertices).
type TreeOutcome = Option<Result<(RefTree, TreeResidue, Result<RefTree, Reason>), Reason>>;

#[derive(Debug, PartialEq, Eq)]
struct TreePutOutcome {
    put: Result<RefTree, Reason>,
    domain: Result<(), Vec<String>>,
}

fn production_tree_put(pair: &Pair, source: &RefTree, edited: &RefTree) -> Option<TreePutOutcome> {
    let (_, complement) = panproto_lens::get(&pair.production, &prod_tree(source)).ok()?;
    let view = prod_tree(edited);
    Some(TreePutOutcome {
        put: panproto_lens::put(&pair.production, &view, &complement)
            .map(|restored| ref_tree(&restored))
            .map_err(|error| lens_reason(&error)),
        domain: panproto_lens::asymmetric::check_put_domain(&pair.production, &view, &complement)
            .map_err(|errors| errors.iter().map(put_domain_reason).collect()),
    })
}

fn reference_tree_put(pair: &Pair, source: &RefTree, edited: &RefTree) -> Option<TreePutOutcome> {
    pair.with_reference(|lens| {
        let lens = lens.ok()?;
        let (_, complement) = RefTreeShape::get(lens, source, &()).ok()?;
        Some(TreePutOutcome {
            put: RefTreeShape::put(lens, edited, &complement, &())
                .map_err(|error| reference_tree_law_reason(&error)),
            domain: RefTreeShape::check_put_domain(lens, edited, &complement, &()).map_err(
                |error| match error {
                    panproto_reference::lens::ReferenceLawError::PutDomain(errors) => {
                        errors.iter().map(ref_put_domain_reason).collect()
                    }
                    other => panic!("unexpected reference domain refusal: {other:?}"),
                },
            ),
        })
    })
}

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
        let lens = match lens {
            Ok(lens) => lens,
            Err(error) => return Some(Err(Reason::Restrict(error))),
        };
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

fn tree_lens_agrees_with_the_reference_on_cases(cases: &[(usize, RefTree)]) {
    let stages = bounded_lens_stages();
    let certificate = bounded_lens_path_count_certificate();
    assert_eq!(cases.len(), certificate.stage_tree_states);
    let bound = format!(
        "bounded lens tree scope: stages={}, trees of at most 3 nodes over each stage's source schema, (stage, tree) cases={}",
        certificate.tree_stages,
        cases.len()
    );
    println!("{bound}");
    let compared = assert_agree(agree_in_parallel(
        &bound,
        cases,
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
                    lens.map_or_else(
                        |_| refused_law_outcome(),
                        |lens| {
                            reference_law_outcome(&panproto_reference::lens::check_tree_laws(
                                lens, tree,
                            ))
                        },
                    )
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

const BOUNDED_LENS_PROTOCOL: &str = "";

fn ref_path_stage(index: usize, stage: &BoundedLensStage) -> PathStage<usize> {
    let migration = panproto_reference::table::canonical_migration_bytes(&stage.migration);
    let mut identity = u64::try_from(migration.len())
        .expect("migration descriptor length")
        .to_le_bytes()
        .to_vec();
    identity.extend_from_slice(&migration);
    identity.extend_from_slice(
        &u64::try_from(stage.name.len())
            .expect("stage-name length")
            .to_le_bytes(),
    );
    identity.extend_from_slice(stage.name.as_bytes());
    PathStage::new_with_protocols(
        index,
        &stage.source,
        BOUNDED_LENS_PROTOCOL,
        &stage.target,
        BOUNDED_LENS_PROTOCOL,
        identity.clone(),
        identity,
    )
}

/// The reference path of a word of stage indices; `Err` when two adjacent
/// boundaries differ.
fn reference_word(
    stages: &[BoundedLensStage],
    word: &[usize],
) -> Result<RefLensPath<usize>, String> {
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
fn production_word(stages: &[BoundedLensStage], word: &[usize]) -> Result<LensPath, String> {
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
fn consecutive_bounded_lens_value_layers_remain_separate_normal_groups() {
    let stages = bounded_lens_stages();
    let transform = bounded_lens_stage_index(&stages, "T");
    let path = production_word(&stages, &[transform, transform]).expect("T then T composes");
    let ranges = path
        .tree_normal_form()
        .into_iter()
        .map(|group| group.stages)
        .collect::<Vec<_>>();
    assert_eq!(ranges, [0..1, 1..2]);
}

/// An identity followed by a transform that writes an undeclared field stays
/// two groups, since that transform's plan refuses at compile; the same
/// transform with its field declared fuses with the identity.
#[test]
fn undeclared_transform_stages_never_join_a_fused_group() {
    let stages = bounded_lens_stages();
    let identity = bounded_lens_stage_index(&stages, "Id1");
    for name in ["T", "ReadF", "Op"] {
        let transform = bounded_lens_stage_index(&stages, name);
        let path =
            production_word(&stages, &[identity, transform]).expect("Id1 meets each transform");
        let ranges = path
            .graph_normal_form()
            .into_iter()
            .map(|group| group.stages)
            .collect::<Vec<_>>();
        assert_eq!(ranges, [0..1, 1..2], "Id1 then {name}");
    }
    let declared = panproto_reference::enumerate::lenses::declared_transform_stages();
    let identity = bounded_lens_stage_index(&declared, "DeclaredId");
    for name in ["DeclaredDefault", "DeclaredRead", "DeclaredCompute"] {
        let transform = bounded_lens_stage_index(&declared, name);
        let path = production_word(&declared, &[identity, transform])
            .expect("DeclaredId meets each transform");
        let ranges = path
            .graph_normal_form()
            .into_iter()
            .map(|group| group.stages)
            .collect::<Vec<_>>();
        assert_eq!(ranges.len(), 1, "DeclaredId then {name} formed one group");
        assert_eq!(ranges[0], 0..2, "DeclaredId then {name}");
    }
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
                    RefTreeShape::put(lens, &restored, &complements[at], &())
                        .map_err(|error| reference_tree_law_reason(&error))
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

fn bounded_lens_stage_index(stages: &[BoundedLensStage], name: &str) -> usize {
    stages
        .iter()
        .position(|stage| stage.name == name)
        .unwrap_or_else(|| panic!("missing bounded lens stage {name}"))
}

fn bounded_lens_identity_index(stages: &[BoundedLensStage], schema: &RefSchema) -> usize {
    stages
        .iter()
        .position(|stage| {
            stage.name.starts_with("Id") && stage.source == *schema && stage.target == *schema
        })
        .unwrap_or_else(|| panic!("missing bounded lens identity at {schema:?}"))
}

fn the_bounded_lens_alphabet_witnesses_every_tree_fusion_condition() {
    let stages = bounded_lens_stages();
    let named = |name| bounded_lens_stage_index(&stages, name);
    let identity_after = |name| bounded_lens_identity_index(&stages, &stages[named(name)].target);
    let witnesses = [
        (
            0_u8,
            "admitted",
            bounded_lens_identity_index(&stages, &stages[named("T")].source),
            bounded_lens_identity_index(&stages, &stages[named("T")].source),
            true,
        ),
        (1, "contraction", named("C"), identity_after("C"), false),
        (
            2,
            "former unbound-field case; ReadF output declaration refuses",
            named("DropF"),
            named("ReadF"),
            false,
        ),
        (3, "direct edge image", named("D"), named("Ext"), false),
        (4, "expansion", named("X"), identity_after("X"), false),
        (
            5,
            "value-layer pair with a T output declaration refusal",
            named("Q"),
            named("T"),
            false,
        ),
        (6, "fan retarget", named("H"), named("Id_h"), false),
        (7, "plan refusal", named("B"), identity_after("B"), false),
        (
            8,
            "operational identity domain",
            bounded_lens_identity_index(&stages, &stages[named("D")].source),
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
            "bounded lens fusion condition {condition} ({label}): {} then {}, fused={fused}",
            stages[first].name, stages[second].name
        );
    }
}

fn path_context(stages: &[BoundedLensStage], words: &[Vec<usize>]) -> PathContext {
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
            RefLensPath::then(
                &RefLensPath::identity_with_protocol(&first.source, BOUNDED_LENS_PROTOCOL),
                &reference,
            )
            .unwrap(),
        );
        let right = (
            production
                .then(&LensPath::identity(&prod_schema(&last.target)))
                .unwrap(),
            RefLensPath::then(
                &reference,
                &RefLensPath::identity_with_protocol(&last.target, BOUNDED_LENS_PROTOCOL),
            )
            .unwrap(),
        );
        composites.push((word.clone(), production, reference));
        composites.push((word.clone(), left.0, left.1));
        composites.push((word.clone(), right.0, right.1));
    }
    (stages.iter().map(Pair::from_stage).collect(), composites)
}

fn lens_paths_agree_with_the_reference_exhaustively() {
    let stages = bounded_lens_stages();
    let certificate = bounded_lens_path_count_certificate();
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
    let sources = bounded_lens_stage_sources(&stages);
    let units: Vec<(usize, usize)> = (0..3 * composable_words.len())
        .flat_map(|index| {
            let first = composable_words[index / 3][0];
            (0..sources[first].len()).map(move |state| (index, state))
        })
        .collect();
    let bound = format!(
        "bounded lens paths: stages={}, words by length={:?}, words={}, composable by length={:?}, composable={}, with both identity composites={}, word-state pairs={}, evaluated path-state cases={}",
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
        "bounded lens paths final SSB: edits={edits}, admitted-positive families={:?}, refused-positive families={:?}, designated-negative families={:?}",
        edit_families.admitted_positive.named(),
        edit_families.refused_positive.named(),
        edit_families.designated_negative.named()
    );
    assert_eq!(compared, u64::try_from(units.len()).unwrap());
}

/// The anchor no bounded lens schema declares.
const UNDECLARED_ANCHOR: &str = "\u{1}undeclared";

/// `source` with one more node, at an anchor its schema does not declare,
/// under the next free node id.
fn with_undeclared_anchor(source: &RefGraph) -> RefGraph {
    let mut augmented = source.clone();
    let id = source.nodes.keys().next_back().map_or(0, |last| last + 1);
    augmented.nodes.insert(
        id,
        panproto_reference::model::Node::new(id, UNDECLARED_ANCHOR),
    );
    augmented
}

/// `source` with one more edge, a self-loop on its least node under a label
/// its schema does not declare and the next free edge id; `None` for a
/// source without nodes.
fn with_undeclared_label(source: &RefGraph) -> Option<RefGraph> {
    let (&node, carrier) = source.nodes.iter().next()?;
    let mut augmented = source.clone();
    let id = source.edges.keys().next_back().map_or(0, |last| last + 1);
    augmented.edges.insert(
        id,
        panproto_reference::model::GEdge {
            src: node,
            tgt: node,
            label: RefSEdge::new(
                &carrier.anchor,
                &carrier.anchor,
                "prop",
                Some(UNDECLARED_ANCHOR),
            ),
            position: None,
        },
    );
    Some(augmented)
}

/// The undeclared-source census: every composite of the path differential
/// whose graph normal form fuses a group, on every certified source state
/// of its first stage, with one undeclared anchor added and, when the state
/// has a node, with one undeclared edge label added.
///
/// On these sources the first stage of a fused pair would drop the
/// undeclared node as residue or refuse the undeclared label, so the fused
/// group must run its stages in sequence. The comparison is the one of
/// [`lens_paths_agree_with_the_reference_exhaustively`], against the
/// atomic reference fold, with every edit of the final-stage basis.
///
/// A pair fuses only when both stage plans compile. The stages `T`,
/// `ReadF` and `Op` write the field [`BOUNDED_LENS_FIELD`], which schema `a`
/// does not declare, so their plans refuse at compile and every source with
/// them; no group contains one, and the 42 composable words in which `Id1`
/// directly precedes one of them and no other adjacent pair fuses lie
/// outside this census.
/// Their refusals are compared by the path differential and pinned by
/// `undeclared_transform_stages_keep_every_original_source_as_a_refusal`.
fn fused_graph_groups_agree_with_the_reference_on_undeclared_sources() {
    let stages = bounded_lens_stages();
    let composable_words = composable_bounded_lens_words(&stages);
    let composites = || path_context(&stages, &composable_words);
    let (_, built) = composites();
    let fused: Vec<usize> = built
        .iter()
        .enumerate()
        .filter(|(_, (_, path, _))| {
            path.graph_normal_form()
                .iter()
                .any(|group| group.stages.len() > 1)
        })
        .map(|(index, _)| index)
        .collect();
    drop(built);
    let sources = bounded_lens_stage_sources(&stages);
    let mut anchors = 0_usize;
    let mut labels = 0_usize;
    let units: Vec<(usize, RefGraph)> = fused
        .iter()
        .flat_map(|&index| {
            let first = composable_words[index / 3][0];
            sources[first].iter().flat_map(move |source| {
                std::iter::once(with_undeclared_anchor(source))
                    .chain(with_undeclared_label(source))
                    .map(move |augmented| (index, augmented))
            })
        })
        .collect();
    for (_, source) in &units {
        if source
            .nodes
            .values()
            .any(|node| node.anchor == UNDECLARED_ANCHOR)
        {
            anchors += 1;
        } else {
            labels += 1;
        }
    }
    let source_states: usize = fused
        .iter()
        .map(|&index| sources[composable_words[index / 3][0]].len())
        .sum();
    let bound = format!(
        "bounded lens undeclared sources: fused composites={}, source states={source_states}, undeclared-anchor sources={anchors}, undeclared-label sources={labels}, cases={}",
        fused.len(),
        units.len()
    );
    println!("{bound}");
    assert_eq!(anchors, source_states);
    assert_eq!(anchors + labels, units.len());
    assert_eq!(fused.len(), 258);
    assert_eq!(source_states, 752_958);
    assert_eq!(labels, 752_700);
    assert_eq!(units.len(), 1_505_658);
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &units,
        composites,
        |(pairs, composites), (index, source)| {
            let word = composites[*index].0.clone();
            let (edits, _) = edits_of_reference_view(pairs, &word, source);
            vec![PathCase {
                index: *index,
                source: source.clone(),
                edits,
            }]
        },
        |(_, composites): &PathContext, case: &PathCase| {
            let (word, path, _) = &composites[case.index];
            assert_undeclared_groups_run_in_sequence(&stages, word, path, &case.source);
            production_path_outcome(path, &case.source, &case.edits)
        },
        |(pairs, composites): &PathContext, case: &PathCase| {
            let (_, _, reference) = &composites[case.index];
            reference_path_outcome(pairs, reference, &case.source, &case.edits)
        },
        path_outcomes_agree,
    ));
    assert_eq!(compared, u64::try_from(units.len()).unwrap());
}

/// The first multi-stage group of `path` records the form the independent
/// declared-source predicate selects: one complement per atomic stage on an
/// undeclared source, one fused complement otherwise.
fn assert_undeclared_groups_run_in_sequence(
    stages: &[BoundedLensStage],
    word: &[usize],
    path: &LensPath,
    source: &RefGraph,
) {
    let groups = path.graph_normal_form();
    let Some((group_index, group)) = groups
        .iter()
        .enumerate()
        .find(|(_, group)| group.stages.len() > 1)
    else {
        return;
    };
    // Only the first group's source is the path source; a later group's
    // input is an intermediate view, whose form follows from its own source.
    if group_index != 0 {
        return;
    }
    let declared = panproto_reference::path::declared_graph_source(&stages[word[0]].source, source);
    if let Ok((_, complement)) = path.get_graph(&prod_graph(source)) {
        let sequential = matches!(
            complement.entries()[group_index].complement,
            panproto_lens::GroupComplement::Sequential(_)
        );
        assert_eq!(
            sequential, !declared,
            "group {:?} of {word:?} on {source:?}",
            group.stages
        );
    }
}

fn composable_bounded_lens_words(stages: &[BoundedLensStage]) -> Vec<Vec<usize>> {
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
    admitted_positive: EditCensus,
    refused_positive: EditCensus,
    designated_negative: EditCensus,
}

impl ClassifiedEditCensus {
    const fn add_admitted_positive(&mut self, mutation: RefMutation) {
        self.admitted_positive.add(mutation, 1);
    }

    const fn add_refused_positive(&mut self, mutation: RefMutation) {
        self.refused_positive.add(mutation, 1);
    }

    const fn add_designated_negative(&mut self, mutation: RefMutation) {
        self.designated_negative.add(mutation, 1);
    }

    const fn add_positive(&mut self, mutation: RefMutation, admitted: bool) {
        if admitted {
            self.add_admitted_positive(mutation);
        } else {
            self.add_refused_positive(mutation);
        }
    }

    const fn add_reference_basis(
        &mut self,
        mutation: RefMutation,
        partition: ReferenceBasisPartition,
    ) {
        match partition {
            ReferenceBasisPartition::AdmittedPositive => {
                self.add_admitted_positive(mutation);
            }
            ReferenceBasisPartition::RefusedPositive => {
                self.add_refused_positive(mutation);
            }
            ReferenceBasisPartition::DesignatedNegative => {
                self.add_designated_negative(mutation);
            }
        }
    }

    fn total(&self) -> usize {
        self.admitted_positive.total()
            + self.refused_positive.total()
            + self.designated_negative.total()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReferenceBasisPartition {
    AdmittedPositive,
    RefusedPositive,
    DesignatedNegative,
}

/// Classify one generated reference-basis candidate by the predicate's
/// observed result. Positive basis cases remain part of the carrier when a
/// stricter dynamic predicate refuses them; only designated negative cases
/// carry an exact-refusal certificate.
fn classify_reference_basis_case<R, C>(
    expected: &DomainExpectation<R>,
    observed: Result<(), R>,
    context: C,
) -> ReferenceBasisPartition
where
    R: std::fmt::Debug + PartialEq,
    C: FnOnce() -> String,
{
    match (expected, observed) {
        (DomainExpectation::Admitted, Ok(())) => ReferenceBasisPartition::AdmittedPositive,
        (DomainExpectation::Admitted, Err(_)) => ReferenceBasisPartition::RefusedPositive,
        (DomainExpectation::Refused(expected), Err(observed)) => {
            assert_eq!(
                &observed,
                expected,
                "designated reference-basis refusal differs for {}",
                context()
            );
            ReferenceBasisPartition::DesignatedNegative
        }
        (DomainExpectation::Refused(expected), Ok(())) => panic!(
            "designated reference-basis negative was admitted for {}: expected {expected:?}",
            context()
        ),
    }
}

fn assert_fan_parent_refusal_remains_in_strict_carrier(stages: &[BoundedLensStage]) {
    let identity = bounded_lens_stage_index(stages, "Id_h");
    let pair = Pair::from_stage(&stages[identity]);
    let labels = panproto_reference::enumerate::lenses::bounded_lens_fan_labels();
    let mut hyper = RefNode::new(0, "h");
    hyper.shape = RefNodeShape::HyperEdge;
    let source = RefGraph {
        nodes: BTreeMap::from([
            (0, hyper),
            (1, RefNode::new(1, "a")),
            (2, RefNode::new(2, "b")),
        ]),
        edges: BTreeMap::from([
            (
                0,
                RefGEdge {
                    src: 0,
                    tgt: 1,
                    label: labels[0].clone(),
                    position: None,
                },
            ),
            (
                1,
                RefGEdge {
                    src: 0,
                    tgt: 2,
                    label: labels[1].clone(),
                    position: None,
                },
            ),
        ]),
    };
    pair.with_reference(|lens| {
        let lens = lens.expect("Id_h compiles");
        let (view, complement) = RefGraphShape::get(lens, &source, &()).expect("Id_h gets");
        let basis = RefGraphShape::single_step_basis(lens, &source, &view, &complement, &());
        let case = basis
            .iter()
            .find(|case| {
                case.mutation == RefMutation::RemoveNode
                    && !case.view.nodes.contains_key(&1)
                    && case.view.nodes.contains_key(&2)
            })
            .expect("the Id_h basis retains deletion of the fan parent");
        assert!(matches!(case.domain, DomainExpectation::Admitted));
        let observed = RefGraphShape::check_put_domain(lens, &case.view, &complement, &());
        let Err(panproto_reference::lens::ReferenceLawError::PutDomain(errors)) = &observed else {
            panic!("fan-parent deletion did not receive a put-domain refusal: {observed:?}");
        };
        assert_eq!(
            errors.iter().map(ref_put_domain_reason).collect::<Vec<_>>(),
            vec!["MalformedFan 0 MissingParent"]
        );
        assert_eq!(
            classify_reference_basis_case(&case.domain, observed, || {
                "Id_h fan-parent deletion".to_owned()
            }),
            ReferenceBasisPartition::RefusedPositive
        );
        let pairs: Vec<_> = stages.iter().map(Pair::from_stage).collect();
        let (edits, census) = edits_of_reference_view(&pairs, &[identity], &source);
        assert!(
            edits.contains(&case.view),
            "the strict path carrier dropped a predicate-refused positive proposal"
        );
        assert!(
            census.refused_positive.families[ref_mutation_index(RefMutation::RemoveNode)] > 0,
            "the strict path census did not classify the fan-parent deletion as refused"
        );
    });
}

fn assert_derived_refusal_remains_in_strict_carrier(stages: &[BoundedLensStage]) {
    let transform = bounded_lens_stage_index(stages, "T");
    assert_eq!(
        Pair::from_stage(&stages[transform]).plan,
        Err(PlanError::UndeclaredTransformField {
            source_vertex: "a".to_owned(),
            target_vertex: "a".to_owned(),
            field: BOUNDED_LENS_FIELD.to_owned(),
        })
    );
    let stages = panproto_reference::enumerate::lenses::declared_transform_stages();
    let transform = bounded_lens_stage_index(&stages, "DeclaredDefault");
    let pair = Pair::from_stage(&stages[transform]);
    let source = RefGraph {
        nodes: BTreeMap::from([(0, RefNode::new(0, "a"))]),
        edges: BTreeMap::new(),
    };
    pair.with_reference(|lens| {
        let lens = lens.expect("the declared default compiles");
        let (view, complement) =
            RefGraphShape::get(lens, &source, &()).expect("the declared default gets");
        let basis = RefGraphShape::single_step_basis(lens, &source, &view, &complement, &());
        let case = basis
            .iter()
            .find(|case| case.mutation == RefMutation::FieldEdit)
            .expect("the declared default basis retains the derived-field edit");
        assert!(matches!(case.domain, DomainExpectation::Admitted));
        let observed = RefGraphShape::check_put_domain(lens, &case.view, &complement, &());
        let Err(panproto_reference::lens::ReferenceLawError::PutDomain(errors)) = &observed else {
            panic!("derived-field edit did not receive a put-domain refusal: {observed:?}");
        };
        assert_eq!(
            errors.iter().map(ref_put_domain_reason).collect::<Vec<_>>(),
            vec!["DerivedCellEdited Node(0) f"]
        );
        assert_eq!(
            classify_reference_basis_case(&case.domain, observed, || {
                "declared default derived-field edit".to_owned()
            }),
            ReferenceBasisPartition::RefusedPositive
        );
        let pairs: Vec<_> = stages.iter().map(Pair::from_stage).collect();
        let (edits, census) = edits_of_reference_view(&pairs, &[transform], &source);
        assert!(
            edits.contains(&case.view),
            "the strict path carrier dropped a predicate-refused derived-field proposal"
        );
        assert!(
            census.refused_positive.families[ref_mutation_index(RefMutation::FieldEdit)] > 0,
            "the strict path census did not classify the derived-field edit as refused"
        );
    });
}

#[test]
fn positive_reference_basis_refusals_remain_in_the_strict_carrier() {
    let stages = bounded_lens_stages();
    assert_fan_parent_refusal_remains_in_strict_carrier(&stages);
    assert_derived_refusal_remains_in_strict_carrier(&stages);
}

#[test]
fn designated_reference_basis_negatives_require_ordered_exact_refusals() {
    let expected = DomainExpectation::Refused(vec!["first", "second"]);
    assert_eq!(
        classify_reference_basis_case(&expected, Err(vec!["first", "second"]), || {
            "ordered exact refusal".to_owned()
        }),
        ReferenceBasisPartition::DesignatedNegative
    );
    assert!(
        std::panic::catch_unwind(|| {
            classify_reference_basis_case(&expected, Err(vec!["second", "first"]), || {
                "reordered refusal".to_owned()
            });
        })
        .is_err()
    );
    assert!(
        std::panic::catch_unwind(|| {
            classify_reference_basis_case(&expected, Ok(()), || {
                "admitted designated negative".to_owned()
            });
        })
        .is_err()
    );
}

#[test]
fn projection_table_path_partitions_positive_and_negative_basis_cases() {
    let stages = bounded_lens_stages();
    let projection = bounded_lens_stage_index(&stages, "P");
    assert_eq!(
        projection, 17,
        "P remains path stage 17 in the bounded lens alphabet"
    );
    let pairs: Vec<_> = stages.iter().map(TablePair::from_stage).collect();
    let source = RefTable {
        rows: BTreeMap::from([
            (0, ("a".to_owned(), BTreeMap::new())),
            (1, ("b".to_owned(), BTreeMap::new())),
        ]),
        pairs: BTreeSet::new(),
    };

    pairs[projection].with_reference(|lens, _| {
        let (view, complement) = RefTableShape::get(lens, &source, &()).expect("P gets the table");
        let basis = RefTableShape::single_step_basis(lens, &source, &view, &complement, &());
        let reserved = basis
            .iter()
            .find(|case| case.mutation == RefMutation::ReservedIdReuse)
            .expect("P carries the dropped row as a designated negative");
        let DomainExpectation::Refused(panproto_reference::lens::TableLawError::Domain(
            TableCheckError::Domain {
                violations,
                unresolved_collations,
            },
        )) = &reserved.domain
        else {
            panic!("P's reserved-row case lacks an exact domain certificate");
        };
        assert_eq!(
            violations
                .iter()
                .map(ref_put_domain_reason)
                .collect::<Vec<_>>(),
            vec!["IdCollision Row(1)"],
            "the designated negative carries the exact ordered row-residue refusal"
        );
        assert_eq!(
            unresolved_collations,
            &BTreeSet::new(),
            "the row-residue refusal has no named-collation component"
        );

        let mut without_row_residue = complement;
        assert!(
            without_row_residue.dropped_rows.remove(&1).is_some(),
            "the counterfactual removes the exact dropped row"
        );
        assert_eq!(
            RefTableShape::check_put_domain(lens, &reserved.view, &without_row_residue, &(),),
            Ok(()),
            "the retained reserved-id candidate is admitted once its row residue is removed"
        );
        assert!(
            !RefTableShape::single_step_basis(lens, &source, &view, &without_row_residue, &(),)
                .iter()
                .any(|case| case.mutation == RefMutation::ReservedIdReuse),
            "removing the dropped-row residue removes its reserved-id counterfactual"
        );
    });

    let (edits, census) = reference_table_path_edits(&pairs, &[projection], &source);
    assert_eq!(edits.len(), 4, "P's focused strict carrier is frozen");
    assert_eq!(
        census.total(),
        edits.len(),
        "the three census buckets partition the strict carrier"
    );
    assert_eq!(
        census.admitted_positive.families,
        [0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        "P admits exactly one AddNode and one RemoveNode proposal"
    );
    assert_eq!(
        census.refused_positive.families,
        [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        "P refuses exactly the manual ValueEdit proposal"
    );
    assert_eq!(
        census.designated_negative.families,
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0],
        "P designates exactly one ReservedIdReuse negative"
    );
}

#[derive(Default)]
struct AtomicClassifiedEditCensus {
    admitted_positive: AtomicEditCensus,
    refused_positive: AtomicEditCensus,
    designated_negative: AtomicEditCensus,
}

impl AtomicClassifiedEditCensus {
    fn add(&self, census: &ClassifiedEditCensus) {
        self.admitted_positive.add(&census.admitted_positive);
        self.refused_positive.add(&census.refused_positive);
        self.designated_negative.add(&census.designated_negative);
    }

    fn into_inner(self) -> ClassifiedEditCensus {
        ClassifiedEditCensus {
            admitted_positive: self.admitted_positive.into_inner(),
            refused_positive: self.refused_positive.into_inner(),
            designated_negative: self.designated_negative.into_inner(),
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
            let mut candidate = view.clone();
            candidate.nodes.remove(id);
            candidate.arcs.retain(|(_, child, _)| child != id);
            let admitted = pair.with_reference(|lens| {
                lens.is_ok_and(|lens| {
                    RefTreeShape::check_put_domain(lens, &candidate, complement, &()).is_ok()
                })
            });
            census.add(RefMutation::RemoveNode, usize::from(admitted));
            census.add(RefMutation::RemoveEdge, usize::from(admitted));
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct ClassifiedTablePositive {
    mutation: RefMutation,
    view: RefTable,
    admitted: bool,
}

fn observe_table_positive(
    lens: &RefTableLens<'_>,
    complement: &RefTableComplement,
    mutation: RefMutation,
    view: RefTable,
) -> ClassifiedTablePositive {
    let admitted = RefTableShape::check_put_domain(lens, &view, complement, &()).is_ok();
    ClassifiedTablePositive {
        mutation,
        view,
        admitted,
    }
}

fn independent_fresh_table_row_id(view: &RefTable, complement: &RefTableComplement) -> Option<u32> {
    let mut candidate = complement.id_floor;
    loop {
        if candidate == u32::MAX {
            return None;
        }
        if !view.rows.contains_key(&candidate) && !complement.dropped_rows.contains_key(&candidate)
        {
            return Some(candidate);
        }
        candidate = candidate.checked_add(1)?;
    }
}

/// Independently construct the complete positive table basis, followed by
/// the intentional bounded lens-field extension. Each candidate carries the observed
/// dynamic-domain classification, so equality below is extensional rather
/// than merely an equality of per-family counts.
fn independent_table_positive_basis(
    pair: &TablePair,
    lens: &RefTableLens<'_>,
    view: &RefTable,
    complement: &RefTableComplement,
) -> Vec<ClassifiedTablePositive> {
    let mut candidates = Vec::new();
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
            candidates.push(observe_table_positive(
                lens,
                complement,
                RefMutation::ValueEdit,
                candidate,
            ));
        }
    }
    if let Some(fresh) = independent_fresh_table_row_id(view, complement) {
        for table in &pair.boundaries.1.tables {
            let mut candidate = view.clone();
            candidate
                .rows
                .insert(fresh, (table.clone(), BTreeMap::new()));
            candidates.push(observe_table_positive(
                lens,
                complement,
                RefMutation::AddNode,
                candidate,
            ));
        }
    }
    for id in view.rows.keys() {
        let mut candidate = view.clone();
        candidate.rows.remove(id);
        candidate
            .pairs
            .retain(|(_, src, tgt)| src != id && tgt != id);
        candidates.push(observe_table_positive(
            lens,
            complement,
            RefMutation::RemoveNode,
            candidate,
        ));
    }

    for (id, (table, cells)) in &view.rows {
        if pair
            .boundaries
            .1
            .columns
            .get(table)
            .is_some_and(|columns| columns.contains_key(BOUNDED_LENS_FIELD))
        {
            continue;
        }
        let mut candidate = view.clone();
        let mut row = cells.clone();
        if row.contains_key(BOUNDED_LENS_FIELD) {
            row.remove(BOUNDED_LENS_FIELD);
        } else {
            row.insert(BOUNDED_LENS_FIELD.to_owned(), BOUNDED_LENS_VALUE);
        }
        candidate.rows.insert(*id, (table.clone(), row));
        if candidates.iter().any(|existing| existing.view == candidate) {
            continue;
        }
        candidates.push(observe_table_positive(
            lens,
            complement,
            RefMutation::ValueEdit,
            candidate,
        ));
    }
    candidates
}

fn table_positive_census(candidates: &[ClassifiedTablePositive]) -> ClassifiedEditCensus {
    let mut census = ClassifiedEditCensus::default();
    for candidate in candidates {
        census.add_positive(candidate.mutation, candidate.admitted);
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
        let basis = RefTreeShape::single_step_basis(lens, &final_source, &view, &complement, &());
        let mut basis_census = ClassifiedEditCensus::default();
        for case in &basis {
            let observed = RefTreeShape::check_put_domain(lens, &case.view, &complement, &());
            let partition = classify_reference_basis_case(&case.domain, observed, || {
                format!("final-tree path {word:?}, basis ordinal {}", case.ordinal)
            });
            basis_census.add_reference_basis(case.mutation, partition);
        }
        assert_eq!(
            basis_census.admitted_positive,
            tree_edit_census(&pairs[last], &view, &complement),
            "independent admitted final-tree edit census for path {word:?}"
        );
        let edits = basis.into_iter().map(|case| case.view).collect();
        Some((edits, basis_census))
    })
}

fn reference_table_final_edits(
    pair: &TablePair,
    lens: &RefTableLens<'_>,
    word: &[usize],
    final_source: &RefTable,
    view: &RefTable,
    complement: &RefTableComplement,
) -> (Vec<RefTable>, ClassifiedEditCensus) {
    let basis = RefTableShape::single_step_basis(lens, final_source, view, complement, &());
    let mut basis_census = ClassifiedEditCensus::default();
    let mut positive_basis = Vec::new();
    let mut designated_negatives = Vec::new();
    for case in &basis {
        let observed = RefTableShape::check_put_domain(lens, &case.view, complement, &());
        let partition = classify_reference_basis_case(&case.domain, observed, || {
            format!("final-table path {word:?}, basis ordinal {}", case.ordinal)
        });
        basis_census.add_reference_basis(case.mutation, partition);
        match partition {
            ReferenceBasisPartition::AdmittedPositive
            | ReferenceBasisPartition::RefusedPositive => {
                positive_basis.push(ClassifiedTablePositive {
                    mutation: case.mutation,
                    view: case.view.clone(),
                    admitted: partition == ReferenceBasisPartition::AdmittedPositive,
                });
            }
            ReferenceBasisPartition::DesignatedNegative => {
                designated_negatives.push((case.mutation, case.view.clone()));
            }
        }
    }
    let mut edits: Vec<RefTable> = basis.into_iter().map(|case| case.view).collect();
    for (id, (table, cells)) in &view.rows {
        let mut candidate = view.clone();
        let mut row = cells.clone();
        if row.contains_key(BOUNDED_LENS_FIELD) {
            row.remove(BOUNDED_LENS_FIELD);
        } else {
            row.insert(BOUNDED_LENS_FIELD.to_owned(), BOUNDED_LENS_VALUE);
        }
        candidate.rows.insert(*id, (table.clone(), row));
        assert!(
            !designated_negatives
                .iter()
                .any(|(_, negative)| negative == &candidate),
            "manual bounded lens-field positive collides with a designated negative for path {word:?}"
        );
        if positive_basis
            .iter()
            .any(|existing| existing.view == candidate)
        {
            continue;
        }
        let admitted = RefTableShape::check_put_domain(lens, &candidate, complement, &()).is_ok();
        basis_census.add_positive(RefMutation::ValueEdit, admitted);
        positive_basis.push(ClassifiedTablePositive {
            mutation: RefMutation::ValueEdit,
            view: candidate.clone(),
            admitted,
        });
        edits.push(candidate);
    }
    let independent = independent_table_positive_basis(pair, lens, view, complement);
    assert_eq!(
        positive_basis, independent,
        "independent extensional positive final-table basis for path {word:?}"
    );
    let independent_census = table_positive_census(&independent);
    assert_eq!(
        basis_census.admitted_positive, independent_census.admitted_positive,
        "independent admitted-positive final-table edit census for path {word:?}"
    );
    assert_eq!(
        basis_census.refused_positive, independent_census.refused_positive,
        "independent refused-positive final-table edit census for path {word:?}"
    );
    assert_eq!(
        independent_census.designated_negative.total(),
        0,
        "the independent table edit census contains only positive proposals"
    );
    assert_eq!(
        designated_negatives.len(),
        basis_census.designated_negative.total(),
        "every designated final-table negative has one ordered exact-refusal certificate for path {word:?}"
    );
    assert_eq!(edits.len(), basis_census.total());
    (edits, basis_census)
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
            reference_table_final_edits(&pairs[last], lens, word, &final_source, &view, &complement)
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
    let stages = bounded_lens_stages();
    let certificate = bounded_lens_path_count_certificate();
    let composable = composable_bounded_lens_words(&stages);
    assert_eq!(composable.len(), certificate.composable());
    let mut units = Vec::new();
    let mut by_length = [0_usize; 3];
    for word in &composable {
        if stages[word[0]].source == bounded_lens_fan_schema() {
            continue;
        }
        for source in bounded_lens_tree_states(&stages[word[0]].source) {
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
    println!(
        "L16 tree fusion: bases={}, inside={}, outside={}, final edits={}, admitted-positive families={:?}, refused-positive families={:?}, designated-negative families={:?}",
        units.len(),
        units.len() - outside,
        outside,
        edits,
        edit_families.admitted_positive.named(),
        edit_families.refused_positive.named(),
        edit_families.designated_negative.named()
    );
    assert_eq!(
        outside, 920,
        "the production-only tree-path consistency scope is part of the certificate"
    );
    assert_eq!(edit_families.total(), edits);
    assert_eq!(compared, u64::try_from(units.len()).unwrap());
}

#[derive(Clone, Debug)]
struct TableFusionCase {
    word: Vec<usize>,
    source: RefTable,
    edits: Vec<RefTable>,
}

fn table_path_normal_forms_match_independent_atomic_folds() {
    let stages = bounded_lens_stages();
    let certificate = bounded_lens_path_count_certificate();
    let composable = composable_bounded_lens_words(&stages);
    let mut units = Vec::new();
    let mut by_length = [0_usize; 3];
    for word in &composable {
        if stages[word[0]].source == bounded_lens_fan_schema() {
            continue;
        }
        for source in bounded_lens_table_states(&stages[word[0]].source) {
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
    println!(
        "L16 table fusion: bases={}, final edits={}, admitted-positive families={:?}, refused-positive families={:?}, designated-negative families={:?}",
        units.len(),
        edits,
        edit_families.admitted_positive.named(),
        edit_families.refused_positive.named(),
        edit_families.designated_negative.named()
    );
    // A word through `T`, `ReadF` or `Op` refuses at compile, since each
    // writes a field schema `a` does not declare, so it contributes a refusal
    // outcome and no final-stage edits to these families.
    assert_eq!(
        edit_families.admitted_positive.families,
        [0, 0, 18_598, 27_218, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        "the admitted table-path mutation-family census is part of the certificate"
    );
    assert_eq!(
        edit_families.refused_positive.families,
        [27_218, 0, 242, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        "the refused table-path mutation-family census is part of the certificate"
    );
    assert_eq!(
        edit_families.designated_negative.families,
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 168, 0, 573, 271, 0],
        "the designated-negative table-path mutation-family census is part of the certificate"
    );
    assert_eq!(compared, u64::try_from(units.len()).unwrap());
}

/// Direct entry point for rebinding the exhaustive table-path basis
/// partition without rerunning the graph, tree, or category certificates.
#[test]
#[ignore = "exhaustive table-path subcertificate; run explicitly when rebinding its census"]
fn table_path_basis_partition_certificate_exhaustive() {
    table_path_normal_forms_match_independent_atomic_folds();
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
        let basis = RefGraphShape::single_step_basis(lens, &final_source, &view, &complement, &());
        let mut census = ClassifiedEditCensus::default();
        for case in &basis {
            let observed = RefGraphShape::check_put_domain(lens, &case.view, &complement, &());
            let partition = classify_reference_basis_case(&case.domain, observed, || {
                format!("final-graph path {word:?}, basis ordinal {}", case.ordinal)
            });
            census.add_reference_basis(case.mutation, partition);
        }
        (basis.into_iter().map(|case| case.view).collect(), census)
    })
}

fn lens_path_category_laws_agree_with_the_reference_exhaustively() {
    let stages = bounded_lens_stages();
    let schemas: Vec<RefSchema> = sigma2_schemas()
        .into_iter()
        .chain([bounded_lens_fan_schema()])
        .collect();
    let mut identities = 0_usize;
    let mut triples = 0_usize;
    let envelopes = |path: &LensPath| path.identity_envelope(panproto_inst::ComplementShape::Graph);
    for (index, stage) in stages.iter().enumerate() {
        let reference = RefLensPath::single(ref_path_stage(index, stage));
        let left = RefLensPath::then(
            &RefLensPath::identity_with_protocol(&stage.source, BOUNDED_LENS_PROTOCOL),
            &reference,
        );
        let right = RefLensPath::then(
            &reference,
            &RefLensPath::identity_with_protocol(&stage.target, BOUNDED_LENS_PROTOCOL),
        );
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
        "bounded lens path category: stages={}, identity schemas={}, unit checks={identities}, triples={triples}",
        stages.len(),
        schemas.len()
    );
    assert_eq!(
        triples,
        bounded_lens_path_count_certificate().words_by_length[2]
    );
}

struct PathPropertyCarrier {
    stages: Vec<BoundedLensStage>,
    words: Vec<Vec<usize>>,
    mismatched_triples: Vec<[usize; 3]>,
    graphs: Vec<Vec<RefGraph>>,
    trees: Vec<Vec<RefTree>>,
    tables: Vec<Vec<RefTable>>,
    tree_units: Vec<(usize, usize)>,
}

fn path_property_carrier() -> &'static PathPropertyCarrier {
    static CARRIER: std::sync::OnceLock<PathPropertyCarrier> = std::sync::OnceLock::new();
    CARRIER.get_or_init(|| {
        let stages = bounded_lens_stages();
        let words = composable_bounded_lens_words(&stages);
        let mut mismatched_triples = Vec::new();
        for first in 0..stages.len() {
            for second in 0..stages.len() {
                for third in 0..stages.len() {
                    if stages[first].target != stages[second].source
                        || stages[second].target != stages[third].source
                    {
                        mismatched_triples.push([first, second, third]);
                    }
                }
            }
        }
        let graphs = stages.iter().map(bounded_lens_sources).collect();
        let trees: Vec<Vec<RefTree>> = stages
            .iter()
            .map(|stage| {
                if stage.source == bounded_lens_fan_schema() {
                    Vec::new()
                } else {
                    bounded_lens_tree_states(&stage.source)
                }
            })
            .collect();
        let tables: Vec<Vec<RefTable>> = stages
            .iter()
            .map(|stage| {
                if stage.source == bounded_lens_fan_schema() {
                    Vec::new()
                } else {
                    bounded_lens_table_states(&stage.source)
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
            mismatched_triples,
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

    let tree_sources = &carrier.trees[word[0]];
    if carrier.stages[word[0]].source != bounded_lens_fan_schema() && !tree_sources.is_empty() {
        let tree_source = &tree_sources[source_index % tree_sources.len()];
        if let Some((tree_edits, _)) = reference_tree_path_edits(&pairs, word, tree_source) {
            let tree_edits = select_one(&tree_edits, edit_index);
            let tree_reference =
                reference_tree_path_outcome(&pairs, word, tree_source, &tree_edits);
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
    }

    let table_pairs: Vec<TablePair> = carrier.stages.iter().map(TablePair::from_stage).collect();
    let table_sources = &carrier.tables[word[0]];
    if table_sources.is_empty() {
        return;
    }
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
    source: usize,
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
    assert_path_on_all_carriers(carrier, word, &[production], &reference, source, edit);
}

fn assert_randomized_path_associativity(case: usize, source: usize, edit: usize) {
    let carrier = path_property_carrier();
    let mismatched = carrier.mismatched_triples[case % carrier.mismatched_triples.len()];
    assert_boundary_mismatched_associativity(&carrier.stages, mismatched);

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
    assert_path_on_all_carriers(carrier, word, &[left, right], &reference, source, edit);
}

fn assert_boundary_mismatched_associativity(stages: &[BoundedLensStage], triple: [usize; 3]) {
    let [first, second, third] = triple;
    assert!(
        stages[first].target != stages[second].source
            || stages[second].target != stages[third].source,
        "the property boundary triple must contain a composition mismatch"
    );

    let reference_single =
        |index: usize| RefLensPath::single(ref_path_stage(index, &stages[index]));
    let reference_left = RefLensPath::then(&reference_single(first), &reference_single(second))
        .and_then(|path| RefLensPath::then(&path, &reference_single(third)));
    let reference_right = RefLensPath::then(&reference_single(second), &reference_single(third))
        .and_then(|path| RefLensPath::then(&reference_single(first), &path));
    assert_eq!(
        reference_left, reference_right,
        "reference associativity differs at mismatched triple {triple:?}"
    );
    assert!(matches!(
        reference_left,
        Err(PathError::CompositionMismatch)
    ));

    let production_single = |index: usize| LensPath::single(production_stage(&stages[index]));
    let production_left = production_single(first)
        .then(&production_single(second))
        .and_then(|path| path.then(&production_single(third)));
    let production_right = production_single(second)
        .then(&production_single(third))
        .and_then(|path| production_single(first).then(&path));
    assert!(matches!(
        production_left,
        Err(LensError::CompositionMismatch)
    ));
    assert!(matches!(
        production_right,
        Err(LensError::CompositionMismatch)
    ));
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

fn certificate_identity_lens(schema: &Schema) -> Lens {
    certificate_mapped_lens(
        schema,
        schema,
        schema
            .edges
            .keys()
            .cloned()
            .map(|edge| (edge.clone(), edge)),
    )
}

fn assert_validation_annotation_path_boundary(plain: &Schema) {
    let reference_plain = ref_schema(plain);
    let edge = certificate_edge_named(plain, "a");
    let mut annotated = plain.clone();
    annotated.required.insert("row".into(), vec![edge.clone()]);
    assert_ne!(plain.required, annotated.required);
    let production = LensPath::single(certificate_identity_lens(plain))
        .then(&LensPath::single(certificate_identity_lens(&annotated)))
        .expect("validation annotations are outside the production path boundary");
    assert_eq!(production.len(), 2);

    let mut reference_annotated = reference_plain.clone();
    reference_annotated
        .required
        .insert("row".to_owned(), vec![ref_edge(&edge)]);
    assert_ne!(reference_plain.required, reference_annotated.required);
    let reference = RefLensPath::single(PathStage::new_with_protocols(
        0_usize,
        &reference_plain,
        &plain.protocol,
        &reference_plain,
        &plain.protocol,
        vec![0],
        vec![0],
    ))
    .then(&RefLensPath::single(PathStage::new_with_protocols(
        1_usize,
        &reference_annotated,
        &annotated.protocol,
        &reference_annotated,
        &annotated.protocol,
        vec![1],
        vec![1],
    )))
    .expect("validation annotations are outside the reference path boundary");
    assert_eq!(reference.len(), 2);
}

fn assert_protocol_mismatch_path_boundary(plain: Schema) {
    let reference_plain = ref_schema(&plain);
    let mut protocol_left = plain.clone();
    protocol_left.protocol = "boundary-left".into();
    let mut protocol_right = plain;
    protocol_right.protocol = "boundary-right".into();
    let production = LensPath::single(certificate_identity_lens(&protocol_left)).then(
        &LensPath::single(certificate_identity_lens(&protocol_right)),
    );
    assert!(matches!(production, Err(LensError::CompositionMismatch)));

    let reference = RefLensPath::single(PathStage::new_with_protocols(
        0_usize,
        &reference_plain,
        &protocol_left.protocol,
        &reference_plain,
        &protocol_left.protocol,
        vec![2],
        vec![2],
    ))
    .then(&RefLensPath::single(PathStage::new_with_protocols(
        1_usize,
        &reference_plain,
        &protocol_right.protocol,
        &reference_plain,
        &protocol_right.protocol,
        vec![3],
        vec![3],
    )));
    assert!(matches!(reference, Err(PathError::CompositionMismatch)));
}

fn assert_structural_path_boundary_mismatch(left: &Schema, right: &Schema, coordinate: &str) {
    let production = LensPath::single(certificate_identity_lens(left))
        .then(&LensPath::single(certificate_identity_lens(right)));
    assert!(
        matches!(production, Err(LensError::CompositionMismatch)),
        "production accepted a {coordinate}-key boundary mismatch"
    );

    let reference_left = ref_schema(left);
    let reference_right = ref_schema(right);
    let reference = RefLensPath::single(PathStage::new_with_protocols(
        0_usize,
        &reference_left,
        &left.protocol,
        &reference_left,
        &left.protocol,
        vec![4],
        vec![4],
    ))
    .then(&RefLensPath::single(PathStage::new_with_protocols(
        1_usize,
        &reference_right,
        &right.protocol,
        &reference_right,
        &right.protocol,
        vec![5],
        vec![5],
    )));
    assert!(
        matches!(reference, Err(PathError::CompositionMismatch)),
        "reference accepted a {coordinate}-key boundary mismatch"
    );
}

fn assert_structural_key_mismatch_path_boundary(protocol: &Protocol, plain: &Schema) {
    let vertex_wider = SchemaBuilder::new(protocol)
        .vertex("row", "record", None)
        .expect("the row vertex is valid")
        .vertex("value", "value", None)
        .expect("the value vertex is valid")
        .vertex("extra", "record", None)
        .expect("the extra vertex is valid")
        .edge("row", "value", "property", Some("a"))
        .expect("the column is valid")
        .build()
        .expect("the vertex-mismatch schema is valid");
    assert_structural_path_boundary_mismatch(plain, &vertex_wider, "vertex");

    let edge_wider = certificate_column_schema(protocol, "value", &["a", "b"]);
    assert_structural_path_boundary_mismatch(plain, &edge_wider, "edge");

    let hyper_protocol = Protocol {
        non_value_kinds: ["record".to_owned()].into(),
        ..Protocol::default()
    };
    let hyper_plain = SchemaBuilder::new(&hyper_protocol)
        .vertex("parent", "record", None)
        .expect("the parent vertex is valid")
        .vertex("child", "record", None)
        .expect("the child vertex is valid")
        .build()
        .expect("the hyper-edge base schema is valid");
    let hyper_wider = SchemaBuilder::new(&hyper_protocol)
        .vertex("parent", "record", None)
        .expect("the parent vertex is valid")
        .vertex("child", "record", None)
        .expect("the child vertex is valid")
        .hyper_edge(
            "fan",
            "relation",
            HashMap::from([
                ("parent".to_owned(), "parent".to_owned()),
                ("child".to_owned(), "child".to_owned()),
            ]),
            "parent",
        )
        .expect("the hyper-edge is valid")
        .build()
        .expect("the hyper-edge mismatch schema is valid");
    assert_structural_path_boundary_mismatch(&hyper_plain, &hyper_wider, "hyper-edge");
}

/// Check one enumerated witness for the quotient used at a path boundary.
fn assert_path_boundary_quotient_witness(witness: PathBoundaryWitness) {
    let protocol = certificate_column_protocol(ScalarKind::Int {
        bits: 64,
        signed: true,
    });
    let plain = certificate_column_schema(&protocol, "value", &["a"]);
    match witness {
        PathBoundaryWitness::ValidationAnnotations => {
            assert_validation_annotation_path_boundary(&plain);
        }
        PathBoundaryWitness::ProtocolMismatch => assert_protocol_mismatch_path_boundary(plain),
        PathBoundaryWitness::StructuralKeyMismatch => {
            assert_structural_key_mismatch_path_boundary(&protocol, &plain);
        }
    }
}

#[test]
fn path_boundary_quotient_witnesses_agree() {
    let mut table_boundaries = 0;
    let mut path_boundaries = 0;
    let mut tree_boundaries = 0;
    for_each_path_law_case(|case| match case {
        PathLawCase::TableFusionBoundary(witness) => {
            assert_table_fusion_boundary_witness(witness);
            table_boundaries += 1;
        }
        PathLawCase::PathBoundary(witness) => {
            assert_path_boundary_quotient_witness(witness);
            path_boundaries += 1;
        }
        PathLawCase::TreeFusionBoundary(witness) => {
            assert_tree_fusion_boundary_witness(witness);
            tree_boundaries += 1;
        }
        _ => {}
    });
    assert_eq!(table_boundaries, 4);
    assert_eq!(path_boundaries, 3);
    assert_eq!(tree_boundaries, 8);
}

fn certificate_vertex_schema(protocol: &Protocol, vertices: &[&str]) -> Schema {
    let mut builder = SchemaBuilder::new(protocol);
    for vertex in vertices {
        builder = builder
            .vertex(vertex, "obj", None)
            .expect("the fusion-witness vertex is valid");
    }
    builder.build().expect("the fusion-witness schema is valid")
}

fn assert_tree_fusion_guard(first: Lens, second: Lens, label: &str) {
    let path = LensPath::single(first)
        .then(&LensPath::single(second))
        .expect("the fusion witness has a shared boundary");
    let groups = path.tree_normal_form();
    assert_eq!(
        groups.len(),
        2,
        "the {label} witness must retain both atomic stages"
    );
}

fn assert_target_only_reverse_carrier_guard(protocol: &Protocol) {
    // A target-only carrier gives the second atomic reverse no source
    // preimage. An identity first stage isolates the target-coverage guard.
    let middle = certificate_vertex_schema(protocol, &["a"]);
    let target = certificate_vertex_schema(protocol, &["a", "fresh"]);
    let first = certificate_identity_lens(&middle);
    let second = Lens {
        compiled: CompiledMigration {
            surviving_verts: target.vertices.keys().cloned().collect(),
            ..CompiledMigration::default()
        },
        src_schema: middle,
        tgt_schema: target,
    };
    assert_tree_fusion_guard(first, second, "target-only reverse carrier");
}

fn assert_target_only_direct_edge_guard(protocol: &Protocol) {
    // Vertex identity does not imply coverage of a target-only direct edge.
    let middle = certificate_vertex_schema(protocol, &["a", "b"]);
    let target = SchemaBuilder::new(protocol)
        .vertex("a", "obj", None)
        .expect("a is valid")
        .vertex("b", "obj", None)
        .expect("b is valid")
        .edge("a", "b", "prop", Some("fresh"))
        .expect("the target-only edge is valid")
        .build()
        .expect("the target-only edge schema is valid");
    let first = certificate_identity_lens(&middle);
    let second = Lens {
        compiled: CompiledMigration {
            surviving_verts: target.vertices.keys().cloned().collect(),
            surviving_edges: target.edges.keys().cloned().collect(),
            ..CompiledMigration::default()
        },
        src_schema: middle,
        tgt_schema: target,
    };
    assert_tree_fusion_guard(first, second, "target-only direct edge");
}

fn assert_staged_error_precedence_guard(protocol: &Protocol) {
    let source = certificate_vertex_schema(protocol, &["kept", "dropped"]);
    let middle = certificate_vertex_schema(protocol, &["kept"]);
    let first = Lens {
        compiled: CompiledMigration {
            surviving_verts: middle.vertices.keys().cloned().collect(),
            ..CompiledMigration::default()
        },
        src_schema: source,
        tgt_schema: middle.clone(),
    };
    let second = certificate_identity_lens(&middle);
    assert_tree_fusion_guard(first, second, "staged error precedence");
}

fn assert_second_middle_representation_validation_guard(protocol: &Protocol) {
    // Ordinary path composition equates the middle carrier, but the second
    // atomic put validates its own source representation. A required-cell
    // obligation there would otherwise be erased by fusion. The first
    // representation is covered by the first lens's isomorphism classifier.
    let base = SchemaBuilder::new(protocol)
        .vertex("a", "obj", None)
        .expect("a is valid")
        .vertex("b", "obj", None)
        .expect("b is valid")
        .edge("a", "b", "prop", Some("f"))
        .expect("f is valid")
        .build()
        .expect("the middle-validation schema is valid");
    let required_edge = certificate_edge_named(&base, "f");
    let mut constrained = base.clone();
    constrained.required.insert("a".into(), vec![required_edge]);
    let compiled = || CompiledMigration {
        surviving_verts: base.vertices.keys().cloned().collect(),
        surviving_edges: base.edges.keys().cloned().collect(),
        ..CompiledMigration::default()
    };
    assert_tree_fusion_guard(
        Lens {
            compiled: compiled(),
            src_schema: base.clone(),
            tgt_schema: base.clone(),
        },
        Lens {
            compiled: compiled(),
            src_schema: constrained,
            tgt_schema: base,
        },
        "second middle-representation validation",
    );
}

fn assert_outside_schema_edited_anchor_guard(protocol: &Protocol) {
    // Raw remaps outside declared carriers participate in the identity-
    // extended reverse map and therefore must also block fusion.
    let schema = |vertex: &str| certificate_vertex_schema(protocol, &[vertex]);
    let a = schema("a");
    let b = schema("b");
    let c = schema("c");
    let relabel = |source: &Schema, target: &Schema, from: &str, to: &str| Lens {
        compiled: CompiledMigration {
            surviving_verts: target.vertices.keys().cloned().collect(),
            vertex_remap: [(from.into(), to.into())].into(),
            ..CompiledMigration::default()
        },
        src_schema: source.clone(),
        tgt_schema: target.clone(),
    };
    assert_tree_fusion_guard(
        relabel(&a, &b, "a", "b"),
        relabel(&b, &c, "b", "c"),
        "outside-schema edited anchor",
    );
}

fn assert_extraneous_reverse_map_entry_guard(protocol: &Protocol) {
    let schema = certificate_vertex_schema(protocol, &["a"]);
    let raw_remap = |from: &str, to: &str| Lens {
        compiled: CompiledMigration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            vertex_remap: [(from.into(), to.into())].into(),
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    };
    assert_tree_fusion_guard(
        raw_remap("x", "y"),
        raw_remap("y", "z"),
        "extraneous reverse-map entry",
    );
}

fn assert_undeclared_value_layer_anchor_guard(protocol: &Protocol) {
    let schema = certificate_vertex_schema(protocol, &["a"]);
    let first = certificate_identity_lens(&schema);
    let mut second = certificate_identity_lens(&schema);
    second.compiled.field_transforms.insert(
        "outside".into(),
        vec![panproto_inst::FieldTransform::RenameField {
            old_key: "old".to_owned(),
            new_key: "new".to_owned(),
        }],
    );
    assert_tree_fusion_guard(first, second, "undeclared value-layer anchor");
}

fn assert_tree_fusion_guard_witness(witness: TreeFusionBoundaryWitness) {
    let protocol = Protocol::default();
    match witness {
        TreeFusionBoundaryWitness::StructuralIdentityBeforeValueLayer => {
            panic!("positive tree-fusion witness was sent to the refusal dispatcher");
        }
        TreeFusionBoundaryWitness::TargetOnlyReverseCarrier => {
            assert_target_only_reverse_carrier_guard(&protocol);
        }
        TreeFusionBoundaryWitness::TargetOnlyDirectEdge => {
            assert_target_only_direct_edge_guard(&protocol);
        }
        TreeFusionBoundaryWitness::StagedErrorPrecedence => {
            assert_staged_error_precedence_guard(&protocol);
        }
        TreeFusionBoundaryWitness::SecondMiddleRepresentationValidation => {
            assert_second_middle_representation_validation_guard(&protocol);
        }
        TreeFusionBoundaryWitness::OutsideSchemaEditedAnchor => {
            assert_outside_schema_edited_anchor_guard(&protocol);
        }
        TreeFusionBoundaryWitness::ExtraneousReverseMapEntry => {
            assert_extraneous_reverse_map_entry_guard(&protocol);
        }
        TreeFusionBoundaryWitness::UndeclaredValueLayerAnchor => {
            assert_undeclared_value_layer_anchor_guard(&protocol);
        }
    }
}

fn reference_rel_schema(schema: &Schema, protocol: &Protocol) -> Option<RelSchema> {
    let value_kinds = protocol.value_kinds.keys().cloned().collect();
    panproto_reference::rel::classify(&ref_schema(schema), &value_kinds, &protocol.non_value_kinds)
        .ok()
}

fn duplicate_lens(lens: &Lens) -> Lens {
    Lens {
        compiled: lens.compiled.clone(),
        src_schema: lens.src_schema.clone(),
        tgt_schema: lens.tgt_schema.clone(),
    }
}

fn reference_functors_compose(
    first: &panproto_reference::rel::RelFunctor,
    second: &panproto_reference::rel::RelFunctor,
    candidate: &panproto_reference::rel::RelFunctor,
) -> bool {
    let tables = first.src.tables.iter().all(|source| {
        first
            .tables
            .get(source)
            .and_then(|middle| second.tables.get(middle))
            == candidate.tables.get(source)
    });
    let relations = first.src.relations.iter().all(|source| {
        first
            .relations
            .get(source)
            .and_then(|middle| second.relations.get(middle))
            == candidate.relations.get(source)
    });
    let columns = first.src.columns.iter().all(|(source_table, columns)| {
        columns.keys().all(|source_column| {
            let source_key = (source_table.clone(), source_column.clone());
            let sequential = first.columns.get(&source_key).and_then(|middle_column| {
                let middle_table = first.tables.get(source_table)?;
                second
                    .columns
                    .get(&(middle_table.clone(), middle_column.clone()))
            });
            sequential == candidate.columns.get(&source_key)
        })
    });
    tables && relations && columns
}

fn reference_columns_are_injective(functor: &panproto_reference::rel::RelFunctor) -> bool {
    let mut images = BTreeSet::new();
    functor
        .columns
        .iter()
        .all(|((table, _), target)| images.insert((table.clone(), target.clone())))
}

fn reference_required_value_free(
    schema: &Schema,
    protocol: &Protocol,
    boundary: &RelSchema,
) -> bool {
    let field_vertices: BTreeSet<_> = boundary
        .columns
        .values()
        .flat_map(BTreeMap::values)
        .map(|edge| edge.tgt.as_str())
        .collect();
    !schema.constraints.iter().any(|(vertex, constraints)| {
        field_vertices.contains(vertex.as_ref())
            && constraints.iter().any(|constraint| {
                protocol
                    .constraint_semantics
                    .get(constraint.sort.as_ref())
                    .is_some_and(|semantics| {
                        matches!(
                            semantics.parse_value(&constraint.sort, &constraint.value),
                            Ok(panproto_schema::ConstraintValue::Required(true))
                        )
                    })
            })
    })
}

/// One stage of a table fusion as the reference sees it: the relational
/// boundaries of its schemas, its migration and, when everything compiles,
/// its table plan.
struct RefTableStagePlan {
    source: Option<RelSchema>,
    target: Option<RelSchema>,
    migration: RefMigration,
    plan: Option<TablePlan>,
}

fn reference_table_stage_plan(
    lens: &Lens,
    source_protocol: &Protocol,
    target_protocol: &Protocol,
) -> RefTableStagePlan {
    let source = reference_rel_schema(&lens.src_schema, source_protocol);
    let target = reference_rel_schema(&lens.tgt_schema, target_protocol);
    let migration = ref_migration(&lens.compiled);
    let plan = source
        .as_ref()
        .zip(target.as_ref())
        .and_then(|(source, target)| {
            panproto_reference::table::compile_for_tables(
                &ref_schema(&lens.src_schema),
                &ref_schema(&lens.tgt_schema),
                source,
                target,
                &migration,
            )
            .ok()
        });
    RefTableStagePlan {
        source,
        target,
        migration,
        plan,
    }
}

/// Whether a compiled stage is a unit bijection: the reference table lens it
/// builds has no obstruction to being an isomorphism.
fn reference_table_stage_is_unit_bijection(lens: &Lens, stage: &RefTableStagePlan) -> bool {
    stage.plan.as_ref().is_some_and(|plan| {
        let source_schema = ref_schema(&lens.src_schema);
        let target_schema = ref_schema(&lens.tgt_schema);
        let source_boundary = stage
            .source
            .as_ref()
            .expect("a compiled plan has a source boundary");
        let target_boundary = stage
            .target
            .as_ref()
            .expect("a compiled plan has a target boundary");
        RefTableLens::new(
            &source_schema,
            &target_schema,
            panproto_reference::table::TableBoundaries::new(source_boundary, target_boundary),
            &stage.migration,
            plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .is_ok_and(|lens| panproto_reference::table::obstruction_to_isomorphism(&lens).is_none())
    })
}

fn reference_table_fusion_premises(
    first: &Lens,
    second: &Lens,
    candidate: &Lens,
    protocols: [&Protocol; 3],
) -> panproto_reference::path::TableFusionPremises {
    let first_stage = reference_table_stage_plan(first, protocols[0], protocols[1]);
    let second_stage = reference_table_stage_plan(second, protocols[1], protocols[2]);
    let candidate_stage = reference_table_stage_plan(candidate, protocols[0], protocols[2]);
    let (first_plan, second_plan, candidate_plan) =
        (&first_stage.plan, &second_stage.plan, &candidate_stage.plan);
    let plans_compile = first_plan.is_some() && second_plan.is_some() && candidate_plan.is_some();
    let first_is_unit_bijection = reference_table_stage_is_unit_bijection(first, &first_stage);
    let middle_boundaries_equal = first_plan
        .as_ref()
        .zip(second_plan.as_ref())
        .is_some_and(|(first, second)| first.target_boundary() == second.source_boundary());
    let second_source_validation_free = second_plan.as_ref().is_some_and(|plan| {
        reference_required_value_free(&second.src_schema, protocols[1], plan.source_boundary())
    });
    let functors_compose = first_plan
        .as_ref()
        .zip(second_plan.as_ref())
        .zip(candidate_plan.as_ref())
        .is_some_and(|((first, second), candidate)| {
            reference_functors_compose(first.functor(), second.functor(), candidate.functor())
        });
    let second_columns_injective = second_plan
        .as_ref()
        .is_some_and(|plan| reference_columns_are_injective(plan.functor()));

    panproto_reference::path::TableFusionPremises {
        first_stage_structurally_pure: (first
            .compiled
            .field_transforms
            .values()
            .all(Vec::is_empty)
            && first
                .compiled
                .op_term_assignments
                .values()
                .all(Vec::is_empty))
        .into(),
        plans_compile: plans_compile.into(),
        first_is_unit_bijection: first_is_unit_bijection.into(),
        middle_boundaries_equal: middle_boundaries_equal.into(),
        second_source_validation_free: second_source_validation_free.into(),
        functors_compose: functors_compose.into(),
        second_columns_injective: second_columns_injective.into(),
    }
}

fn reference_table_fusion_classification(
    first: &Lens,
    second: &Lens,
    candidate: &Lens,
    protocols: [&Protocol; 3],
) -> Result<(), panproto_reference::path::TableFusionObstruction> {
    panproto_reference::path::classify_table_fusion(reference_table_fusion_premises(
        first, second, candidate, protocols,
    ))
}

fn assert_table_fusion_decision(
    first: &Lens,
    second: &Lens,
    protocols: [&Protocol; 3],
    expected: Result<(), panproto_reference::path::TableFusionObstruction>,
) {
    let path = LensPath::single(duplicate_lens(first))
        .then(&LensPath::single(duplicate_lens(second)))
        .expect("the table-fusion fixture has a shared middle schema");
    let tree_groups = path.tree_normal_form();
    assert_eq!(
        tree_groups.len(),
        1,
        "a normalization differential requires a separately certified structural candidate"
    );
    let reference =
        reference_table_fusion_classification(first, second, &tree_groups[0].lens, protocols);
    assert_eq!(reference, expected, "the independent premise classifier");
    let production_protocols = protocols.map(|protocol| (*protocol).clone());
    let production = path
        .table_normal_form(&production_protocols)
        .expect("the fusion fixture has three boundary protocols")
        .len()
        == 1;
    assert_eq!(
        production,
        reference.is_ok(),
        "production table fusion must agree with the independent premise classifier"
    );
}

fn assert_admitted_table_fusion_behavior(
    first: &Lens,
    second: &Lens,
    protocols: &[Protocol; 3],
    source: &FInstance,
) {
    assert_table_fusion_decision(
        first,
        second,
        [&protocols[0], &protocols[1], &protocols[2]],
        Ok(()),
    );
    let path = LensPath::single(duplicate_lens(first))
        .then(&LensPath::single(duplicate_lens(second)))
        .expect("the admitted fixture has a shared middle schema");
    let (fused_view, fused_complement) = path
        .get_table(source, protocols)
        .expect("the admitted fused get succeeds");
    let (middle, first_complement) = table_lens::get(
        first,
        source,
        BoundaryProtocols {
            src: &protocols[0],
            tgt: &protocols[1],
        },
    )
    .expect("the first atomic get succeeds");
    let (atomic_view, second_complement) = table_lens::get(
        second,
        &middle,
        BoundaryProtocols {
            src: &protocols[1],
            tgt: &protocols[2],
        },
    )
    .expect("the second atomic get succeeds");
    assert_eq!(fused_view, atomic_view, "fused and atomic get");

    let fused_source = path
        .put_table(&fused_view, &fused_complement, protocols)
        .expect("the admitted fused put succeeds");
    let atomic_middle = table_lens::put(
        second,
        &atomic_view,
        &second_complement,
        BoundaryProtocols {
            src: &protocols[1],
            tgt: &protocols[2],
        },
    )
    .expect("the second atomic put succeeds");
    let atomic_source = table_lens::put(
        first,
        &atomic_middle,
        &first_complement,
        BoundaryProtocols {
            src: &protocols[0],
            tgt: &protocols[1],
        },
    )
    .expect("the first atomic put succeeds");
    assert_eq!(fused_source, atomic_source, "fused and atomic put");
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
    let candidate = certificate_mapped_lens(
        &source,
        &target,
        [
            (
                certificate_edge_named(&source, "a"),
                certificate_edge_named(&target, "q"),
            ),
            (
                certificate_edge_named(&source, "z"),
                certificate_edge_named(&target, "q"),
            ),
        ],
    );
    assert_eq!(
        reference_table_fusion_classification(
            &first,
            &second,
            &candidate,
            [&protocol, &protocol, &protocol],
        ),
        Err(panproto_reference::path::TableFusionObstruction::ColumnCollision),
        "the independent table-specific classifier must reject the noninjective column map",
    );
    let path = LensPath::single(duplicate_lens(&first))
        .then(&LensPath::single(duplicate_lens(&second)))
        .expect("the collision fixture has a shared middle schema");
    assert_eq!(
        path.tree_normal_form().len(),
        2,
        "the upstream structural-identity predicate makes the downstream column guard redundant",
    );

    let admitted_schema = certificate_column_schema(&protocol, "value", &["a", "z"]);
    let admitted_first = certificate_identity_lens(&admitted_schema);
    let admitted_second = certificate_identity_lens(&admitted_schema);
    let mut instance = FInstance::new();
    instance
        .insert_row(
            "row",
            RowId(0),
            HashMap::from([
                ("a".to_owned(), Value::Int(1)),
                ("z".to_owned(), Value::Int(2)),
            ]),
        )
        .expect("the admitted collision control row is distinct");
    assert_admitted_table_fusion_behavior(
        &admitted_first,
        &admitted_second,
        &[protocol.clone(), protocol.clone(), protocol],
        &instance,
    );
}

fn table_fusion_protocol_drift_witness() {
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
    let protocols = [source_protocol, middle_protocol.clone(), middle_protocol];
    let mut present = FInstance::new();
    present
        .insert_row(
            "row",
            RowId(0),
            HashMap::from([("f".to_owned(), Value::Bool(true))]),
        )
        .expect("the admitted drift row is distinct");
    assert_admitted_table_fusion_behavior(&first, &second, &protocols, &present);

    // A genuinely absent cell reaches the predicate as
    // absent in both executions, so both refuse with the same typed error.
    let mut absent = FInstance::new();
    absent
        .insert_row("row", RowId(0), HashMap::new())
        .expect("the absent-cell drift row is distinct");
    let path = LensPath::single(duplicate_lens(&first))
        .then(&LensPath::single(duplicate_lens(&second)))
        .expect("the drift fixture has a shared middle schema");
    let fused = path
        .get_table(&absent, &protocols)
        .expect_err("the fused predicate reads an absent cell");
    let (middle, _) = table_lens::get(
        &first,
        &absent,
        BoundaryProtocols {
            src: &protocols[0],
            tgt: &protocols[1],
        },
    )
    .expect("the structural first stage keeps the row");
    let atomic = table_lens::get(
        &second,
        &middle,
        BoundaryProtocols {
            src: &protocols[1],
            tgt: &protocols[2],
        },
    )
    .expect_err("the atomic predicate reads an absent cell");
    assert!(
        matches!(
            &atomic,
            LensError::Restrict(RestrictError::SurvivalPredicateFailed {
                node: 0,
                source: panproto_expr::ExprError::UnboundVariable(field),
            }) if field == "f"
        ),
        "the atomic refusal is the absent-cell predicate failure: {atomic:?}"
    );
    let LensError::AtStage { at, error } = fused else {
        panic!("a path refusal is located at its group: {fused:?}");
    };
    assert_eq!((at.group, at.stages), (0, 0..2), "one fused group");
    assert_eq!(
        format!("{error:?}"),
        format!("{atomic:?}"),
        "the fused group reports the atomic refusal"
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
    assert_table_fusion_decision(
        &first,
        &second,
        [&protocol, &protocol, &protocol],
        Err(panproto_reference::path::TableFusionObstruction::RequiredValueValidation),
    );
    let admitted_first = certificate_mapped_lens(&schema, &schema, std::iter::empty());
    let admitted_second = certificate_mapped_lens(&schema, &schema, std::iter::empty());
    let mut instance = FInstance::new();
    instance
        .insert_row(
            "row",
            RowId(0),
            HashMap::from([("f".to_owned(), Value::Int(1))]),
        )
        .expect("the admitted validation control row is distinct");
    assert_admitted_table_fusion_behavior(
        &admitted_first,
        &admitted_second,
        &[protocol.clone(), protocol.clone(), protocol],
        &instance,
    );
}

/// A target-only coordinate (a table, foreign key or column only the
/// target declares) obstructs table isomorphism, the independent classifier
/// rejects fusing past it, and the schema-level path keeps two groups.
fn assert_static_table_isomorphism_boundary(
    protocol: &Protocol,
    source: &Schema,
    target: &Schema,
    coordinate: &str,
) {
    let first = certificate_mapped_lens(source, target, std::iter::empty());
    let boundaries = BoundaryProtocols {
        src: protocol,
        tgt: protocol,
    };
    assert!(!panproto_lens::table_lens::is_isomorphism(
        &first, boundaries
    ));
    let obstruction = panproto_lens::table_lens::obstruction_to_isomorphism(&first, boundaries)
        .expect("the target-only coordinate must obstruct table isomorphism");
    assert!(
        obstruction.contains(coordinate),
        "the {coordinate} subcase reported the wrong obstruction: {obstruction}"
    );

    let second = certificate_mapped_lens(target, target, std::iter::empty());
    assert_eq!(
        reference_table_fusion_classification(
            &first,
            &second,
            &first,
            [protocol, protocol, protocol],
        ),
        Err(panproto_reference::path::TableFusionObstruction::FirstStageNotUnitBijection),
        "the independent table classifier must reject the {coordinate} boundary",
    );
    let path = LensPath::single(duplicate_lens(&first))
        .then(&LensPath::single(second))
        .expect("the target-only fixture has a shared middle schema");
    assert_eq!(
        path.tree_normal_form().len(),
        2,
        "the schema-level {coordinate} fixture is structurally precluded before table fusion",
    );
}

fn table_fusion_target_only_carrier_witness() {
    let relational_protocol = Protocol {
        non_value_kinds: ["record".to_owned()].into(),
        ..Protocol::default()
    };
    let table_source = SchemaBuilder::new(&relational_protocol)
        .vertex("row", "record", None)
        .expect("the source row is valid")
        .build()
        .expect("the source schema is valid");
    let table_target = SchemaBuilder::new(&relational_protocol)
        .vertex("row", "record", None)
        .expect("the middle row is valid")
        .vertex("target_only", "record", None)
        .expect("the target-only table is valid")
        .build()
        .expect("the target table schema is valid");
    assert_static_table_isomorphism_boundary(
        &relational_protocol,
        &table_source,
        &table_target,
        "target table `target_only`",
    );

    let relation_source = SchemaBuilder::new(&relational_protocol)
        .vertex("parent", "record", None)
        .expect("the parent table is valid")
        .vertex("child", "record", None)
        .expect("the child table is valid")
        .build()
        .expect("the relation source schema is valid");
    let relation_target = SchemaBuilder::new(&relational_protocol)
        .vertex("parent", "record", None)
        .expect("the parent table is valid")
        .vertex("child", "record", None)
        .expect("the child table is valid")
        .edge("child", "parent", "foreign-key", Some("parent"))
        .expect("the target-only relation is valid")
        .build()
        .expect("the relation target schema is valid");
    assert_static_table_isomorphism_boundary(
        &relational_protocol,
        &relation_source,
        &relation_target,
        "target foreign key",
    );

    let column_protocol = certificate_column_protocol(ScalarKind::Int {
        bits: 64,
        signed: true,
    });
    let column_source = certificate_column_schema(&column_protocol, "value", &[]);
    let column_target = certificate_column_schema(&column_protocol, "value", &["target_only"]);
    assert_static_table_isomorphism_boundary(
        &column_protocol,
        &column_source,
        &column_target,
        "target column `target_only`",
    );

    assert_protocol_drift_table_fusion_witness();
}

/// A carrier that is a value column under the source protocol but a table
/// under the target protocol makes the first stage no unit bijection, while
/// the same lens under one protocol fuses.
fn assert_protocol_drift_table_fusion_witness() {
    let source_protocol = Protocol {
        value_kinds: BTreeMap::from([(
            "aux".to_owned(),
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        )]),
        non_value_kinds: ["record".to_owned()].into(),
        ..Protocol::default()
    };
    let target_protocol = Protocol {
        non_value_kinds: ["aux".to_owned(), "record".to_owned()].into(),
        ..Protocol::default()
    };
    let protocol_drift_schema = SchemaBuilder::new(&source_protocol)
        .vertex("row", "record", None)
        .expect("the row is valid")
        .vertex("target_only", "aux", None)
        .expect("the protocol-dependent carrier is valid")
        .build()
        .expect("the protocol-drift schema is valid");
    let drift_first = certificate_identity_lens(&protocol_drift_schema);
    let drift_second = certificate_identity_lens(&protocol_drift_schema);
    assert_table_fusion_decision(
        &drift_first,
        &drift_second,
        [&source_protocol, &target_protocol, &target_protocol],
        Err(panproto_reference::path::TableFusionObstruction::FirstStageNotUnitBijection),
    );

    assert_admitted_table_fusion_behavior(
        &certificate_identity_lens(&protocol_drift_schema),
        &certificate_identity_lens(&protocol_drift_schema),
        &[
            source_protocol.clone(),
            source_protocol.clone(),
            source_protocol,
        ],
        &FInstance::new(),
    );
}

fn assert_tree_fusion_boundary_witness(witness: TreeFusionBoundaryWitness) {
    match witness {
        TreeFusionBoundaryWitness::StructuralIdentityBeforeValueLayer => {
            let stages = bounded_lens_stages();
            let predicate = bounded_lens_stage_index(&stages, "Q");
            let identity = stages
                .iter()
                .position(|stage| {
                    stage.name.starts_with("Id")
                        && stage.source == stages[predicate].source
                        && stage.target == stages[predicate].source
                })
                .expect("the bounded lens alphabet contains Id_a");
            let word = [identity, predicate];
            let path = production_word(&stages, &word).expect("Id_a then Q composes");
            let groups = path.tree_normal_form();
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].stages, 0..2);

            let pairs: Vec<Pair> = stages.iter().map(Pair::from_stage).collect();
            let sources = bounded_lens_tree_states(&stages[identity].source);
            assert_ne!(sources, []);
            for source in &sources {
                let fused = production_tree_path_outcome(&path, source, &[]);
                let atomic = reference_tree_path_outcome(&pairs, &word, source, &[]);
                assert!(
                    shape_path_outcomes_agree(&fused, &atomic),
                    "Id_a ; Q fused tree get/put differs from the atomic reference on {source:?}"
                );
            }
        }
        witness => assert_tree_fusion_guard_witness(witness),
    }
}

const TABLE_FUSION_BOUNDARY_WITNESSES: [TableFusionBoundaryWitness; 4] = [
    TableFusionBoundaryWitness::ColumnMapInjectivity,
    TableFusionBoundaryWitness::ProtocolDrift,
    TableFusionBoundaryWitness::RequiredValueValidation,
    TableFusionBoundaryWitness::TargetOnlyRelationalCarrier,
];

const PATH_BOUNDARY_WITNESSES: [PathBoundaryWitness; 3] = [
    PathBoundaryWitness::ValidationAnnotations,
    PathBoundaryWitness::ProtocolMismatch,
    PathBoundaryWitness::StructuralKeyMismatch,
];

const TREE_FUSION_BOUNDARY_WITNESSES: [TreeFusionBoundaryWitness; 8] = [
    TreeFusionBoundaryWitness::StructuralIdentityBeforeValueLayer,
    TreeFusionBoundaryWitness::TargetOnlyReverseCarrier,
    TreeFusionBoundaryWitness::TargetOnlyDirectEdge,
    TreeFusionBoundaryWitness::StagedErrorPrecedence,
    TreeFusionBoundaryWitness::SecondMiddleRepresentationValidation,
    TreeFusionBoundaryWitness::OutsideSchemaEditedAnchor,
    TreeFusionBoundaryWitness::ExtraneousReverseMapEntry,
    TreeFusionBoundaryWitness::UndeclaredValueLayerAnchor,
];

fn assert_table_fusion_boundary_witness(witness: TableFusionBoundaryWitness) {
    match witness {
        TableFusionBoundaryWitness::ColumnMapInjectivity => {
            table_fusion_column_injectivity_witness();
        }
        TableFusionBoundaryWitness::ProtocolDrift => {
            table_fusion_protocol_drift_witness();
        }
        TableFusionBoundaryWitness::RequiredValueValidation => {
            table_fusion_required_value_witness();
        }
        TableFusionBoundaryWitness::TargetOnlyRelationalCarrier => {
            table_fusion_target_only_carrier_witness();
        }
    }
}

fn assert_all_named_path_boundaries() {
    for witness in TABLE_FUSION_BOUNDARY_WITNESSES {
        assert_table_fusion_boundary_witness(witness);
    }
    for witness in PATH_BOUNDARY_WITNESSES {
        assert_path_boundary_quotient_witness(witness);
    }
    for witness in TREE_FUSION_BOUNDARY_WITNESSES {
        assert_tree_fusion_boundary_witness(witness);
    }
}

#[test]
fn named_table_fusion_boundaries_agree_with_the_reference() {
    for witness in TABLE_FUSION_BOUNDARY_WITNESSES {
        assert_table_fusion_boundary_witness(witness);
    }
}

/// A bounded-memory, independently derived cursor over the exact path-law
/// manifest. State cardinalities are computed once at construction time, so
/// validating the two-million-case operational suffix does not rebuild a
/// graph, tree or table census for each payload.
struct ExpectedPathLawCases {
    stages: Vec<BoundedLensStage>,
    words: Vec<Vec<usize>>,
    graph_source_counts: Vec<usize>,
    tree_source_counts: Vec<usize>,
    table_source_counts: Vec<usize>,
    unit_ordinal: usize,
    associativity_ordinal: usize,
    operational_word: usize,
    operational_offset: usize,
    boundary_ordinal: usize,
}

impl ExpectedPathLawCases {
    fn new() -> Self {
        let stages = bounded_lens_stages();
        let words = composable_bounded_lens_words(&stages);
        let graph_source_counts = stages
            .iter()
            .map(|stage| {
                if stage.source == bounded_lens_fan_schema() {
                    bounded_lens_fan_state_count()
                } else {
                    bounded_lens_graph_state_count(&stage.source)
                }
            })
            .collect();
        let tree_source_counts = stages
            .iter()
            .map(|stage| {
                if stage.source == bounded_lens_fan_schema() {
                    0
                } else {
                    bounded_lens_tree_state_count(&stage.source)
                }
            })
            .collect();
        let table_source_counts = stages
            .iter()
            .map(|stage| {
                if stage.source == bounded_lens_fan_schema() {
                    0
                } else {
                    bounded_lens_table_state_count(&stage.source)
                }
            })
            .collect();
        Self {
            stages,
            words,
            graph_source_counts,
            tree_source_counts,
            table_source_counts,
            unit_ordinal: 0,
            associativity_ordinal: 0,
            operational_word: 0,
            operational_offset: 0,
            boundary_ordinal: 0,
        }
    }

    fn next(&mut self) -> Option<PathLawCase> {
        if self.unit_ordinal < 2 * self.words.len() {
            let ordinal = self.unit_ordinal;
            self.unit_ordinal += 1;
            let word = self.words[ordinal / 2].clone();
            return Some(if ordinal.is_multiple_of(2) {
                PathLawCase::LeftIdentity { word }
            } else {
                PathLawCase::RightIdentity { word }
            });
        }

        let stage_count = self.stages.len();
        let associativity_cases = stage_count.pow(3);
        if self.associativity_ordinal < associativity_cases {
            let ordinal = self.associativity_ordinal;
            self.associativity_ordinal += 1;
            return Some(PathLawCase::Associativity {
                stages: [
                    ordinal / stage_count.pow(2),
                    (ordinal / stage_count) % stage_count,
                    ordinal % stage_count,
                ],
            });
        }

        while self.operational_word < self.words.len() {
            let word = &self.words[self.operational_word];
            let first = word[0];
            let graph_cases = 3 * self.graph_source_counts[first];
            let tree_cases = self.tree_source_counts[first];
            let table_cases = self.table_source_counts[first];
            let cases_for_word = graph_cases + tree_cases + table_cases;
            if self.operational_offset == cases_for_word {
                self.operational_word += 1;
                self.operational_offset = 0;
                continue;
            }

            let offset = self.operational_offset;
            self.operational_offset += 1;
            let (form, carrier, source) = if offset < graph_cases {
                let form = match offset % 3 {
                    0 => panproto_reference::enumerate::lenses::PathLawForm::Sequential,
                    1 => panproto_reference::enumerate::lenses::PathLawForm::LeftIdentity,
                    _ => panproto_reference::enumerate::lenses::PathLawForm::RightIdentity,
                };
                (form, PathLawCarrier::Graph, offset / 3)
            } else if offset < graph_cases + tree_cases {
                (
                    panproto_reference::enumerate::lenses::PathLawForm::Sequential,
                    PathLawCarrier::Tree,
                    offset - graph_cases,
                )
            } else {
                (
                    panproto_reference::enumerate::lenses::PathLawForm::Sequential,
                    PathLawCarrier::Table,
                    offset - graph_cases - tree_cases,
                )
            };
            return Some(PathLawCase::Operational {
                form,
                carrier,
                word: word.clone(),
                source,
            });
        }

        let ordinal = self.boundary_ordinal;
        self.boundary_ordinal += 1;
        if ordinal < TABLE_FUSION_BOUNDARY_WITNESSES.len() {
            return Some(PathLawCase::TableFusionBoundary(
                TABLE_FUSION_BOUNDARY_WITNESSES[ordinal],
            ));
        }
        let ordinal = ordinal - TABLE_FUSION_BOUNDARY_WITNESSES.len();
        if ordinal < PATH_BOUNDARY_WITNESSES.len() {
            return Some(PathLawCase::PathBoundary(PATH_BOUNDARY_WITNESSES[ordinal]));
        }
        let ordinal = ordinal - PATH_BOUNDARY_WITNESSES.len();
        TREE_FUSION_BOUNDARY_WITNESSES
            .get(ordinal)
            .copied()
            .map(PathLawCase::TreeFusionBoundary)
    }
}

fn path_certificate_manifest_is_exact() {
    let mut expected = ExpectedPathLawCases::new();
    let mut cases = 0_usize;
    let mut variants = [0_usize; 7];
    for_each_path_law_case(|case| {
        let expected_case = expected
            .next()
            .expect("the enumerator emitted a case beyond the exact manifest");
        assert_eq!(
            case, expected_case,
            "path-law payload differs at manifest ordinal {cases}"
        );
        cases += 1;
        match case {
            PathLawCase::LeftIdentity { .. } => variants[0] += 1,
            PathLawCase::RightIdentity { .. } => variants[1] += 1,
            PathLawCase::Associativity { .. } => variants[2] += 1,
            PathLawCase::Operational { .. } => variants[3] += 1,
            PathLawCase::TableFusionBoundary(witness) => {
                variants[4] += 1;
                assert_table_fusion_boundary_witness(witness);
            }
            PathLawCase::PathBoundary(witness) => {
                variants[5] += 1;
                assert_path_boundary_quotient_witness(witness);
            }
            PathLawCase::TreeFusionBoundary(witness) => {
                variants[6] += 1;
                assert_tree_fusion_boundary_witness(witness);
            }
        }
    });
    assert!(
        expected.next().is_none(),
        "the enumerator stopped before the exact manifest was exhausted"
    );
    assert_eq!(cases, path_law_case_count_certificate());
    assert_eq!(cases, 2_003_096);
    assert_eq!(
        variants,
        [537, 537, 29_791, 1_972_216, 4, 3, 8],
        "every path-law variant has its independently pinned cardinality"
    );
}

/// One directly runnable certificate for the path category and its graph,
/// tree, and table operational semantics.
#[test]
fn lens_path_law_certificate() {
    path_certificate_manifest_is_exact();
    lens_path_category_laws_agree_with_the_reference_exhaustively();
    the_bounded_lens_alphabet_witnesses_every_tree_fusion_condition();
    lens_paths_agree_with_the_reference_exhaustively();
    fused_graph_groups_agree_with_the_reference_on_undeclared_sources();
    tree_path_normal_forms_match_independent_atomic_folds();
    table_path_normal_forms_match_independent_atomic_folds();
}

#[test]
fn empty_schema_path_properties_skip_only_the_empty_tree_carrier() {
    use panproto_reference::enumerate::lenses::PathLawForm;

    let carrier = path_property_carrier();
    assert_eq!(carrier.stages[0].name, "Id0");
    assert_eq!(carrier.graphs[0].len(), 1);
    assert_eq!(carrier.trees[0], []);
    assert_eq!(carrier.tables[0].len(), 1);

    assert_randomized_path_form(PathLawForm::LeftIdentity, 0, 0, 0);
    assert_randomized_path_form(PathLawForm::RightIdentity, 0, 0, 0);
    assert_randomized_path_form(PathLawForm::Sequential, 0, 0, 0);
    assert_randomized_path_associativity(0, 0, 0);
}

proptest! {
    #![proptest_config(ProptestConfig::default())]
    #[test]
    fn randomized_path_left_identity_differential(
        case in any::<usize>(),
        source in any::<usize>(),
        edit in any::<usize>(),
    ) {
        assert_randomized_path_form(
            panproto_reference::enumerate::lenses::PathLawForm::LeftIdentity,
            case,
            source,
            edit,
        );
    }

    #[test]
    fn randomized_path_right_identity_differential(
        case in any::<usize>(),
        source in any::<usize>(),
        edit in any::<usize>(),
    ) {
        assert_randomized_path_form(
            panproto_reference::enumerate::lenses::PathLawForm::RightIdentity,
            case,
            source,
            edit,
        );
    }

    #[test]
    fn randomized_path_associativity_differential(
        case in any::<usize>(),
        source in any::<usize>(),
        edit in any::<usize>(),
    ) {
        assert_randomized_path_associativity(case, source, edit);
    }

    #[test]
    fn randomized_path_fusion_differential(
        case in any::<usize>(),
        source in any::<usize>(),
        edit in any::<usize>(),
    ) {
        assert_all_named_path_boundaries();
        assert_randomized_path_form(
            panproto_reference::enumerate::lenses::PathLawForm::Sequential,
            case,
            source,
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
        assert_false_precondition_protolens_refusal();
        assert_overlong_protolens_expansion_refusal();
        assert_malformed_fan_protolens_application_refusal();
    }
}

fn randomized_table_complement_case(index: usize) {
    let fixture_count = table_fixture_count_certificate().total;
    let field_order_count = 1;
    let construction_count = table_complement_construction_count_certificate();
    let refusal_count = table_get_refusal_count_certificate();
    let total = table_complement_law_count_certificate();
    assert_eq!(
        total,
        fixture_count + field_order_count + construction_count + refusal_count
    );
    assert_eq!(total, 9_938);

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
    assert_eq!(constructions.len(), construction_count);
    for construction in &constructions {
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
            "table-complement construction {}",
            construction.name
        );
    }
    assert_table_complement_field_order();
    for refusal in TableGetRefusalCase::ALL {
        assert_table_get_refusal_case(refusal);
    }
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
    for derived in 0..derived_count {
        randomized_derived_writer_case(derived);
    }
    for pair in TablePairPutCase::ALL {
        assert_table_pair_put_case(pair);
    }
    for refusal in TablePutRefusalCase::ALL {
        assert_table_put_refusal_case(refusal);
    }
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
fn empty_table_does_not_relax_the_graph_neutral_plan_boundary() {
    let fixture = TableFixture {
        columns: 2,
        retained: 0,
        source: RefTable::default(),
    };
    let pair = fixture_pair(&fixture);
    let (graph, _) = GInstance::from_finstance(&prod_table(&fixture.source)).unwrap();
    assert!(graph_lens_via_table(&pair, &graph, None).is_none());

    let graph_pair = table_square_pair_graph(&pair);
    graph_pair.with_reference(|graph_lens| {
        let graph_lens = graph_lens.expect("the fixture graph plan compiles");
        pair.with_reference(|table_lens, _| {
            let result =
                panproto_reference::lens::table_square(graph_lens, table_lens, &fixture.source);
            let SquareResult::Refused(observation) = result else {
                panic!("a column drop remains outside the exact domain on an empty source");
            };
            assert!(observation.graph.get.is_ok());
            assert!(matches!(
                observation.fast.get,
                Err(SquareError::OutsideExactDomain(
                    "the table cell plan is not graph-neutral"
                ))
            ));
            assert_eq!(observation.fast.put, None);

            let edited = RefTable {
                rows: [(0, ("row".to_owned(), BTreeMap::new()))].into(),
                ..RefTable::default()
            };
            let result = panproto_reference::lens::table_put_square(
                graph_lens,
                table_lens,
                &fixture.source,
                &edited,
            );
            let SquareResult::Refused(observation) = result else {
                panic!("an edited column-drop square remains outside the exact domain");
            };
            assert!(observation.graph.get.is_ok());
            assert!(matches!(observation.graph.put, Some(Ok(_))));
            assert!(matches!(
                observation.fast.get,
                Err(SquareError::OutsideExactDomain(
                    "the table cell plan is not graph-neutral"
                ))
            ));
            assert_eq!(observation.fast.put, None);
        });
    });
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

fn assert_direct_graph_route_operations(pair: &Pair, case: &TreeSquareCase) {
    let (source, _) = panproto_reference::encode::tree_to_graph(&case.source, None)
        .expect("the declared nonfan square source embeds");
    let expected_get = reference_get(pair, &source);
    assert_eq!(production_get(pair, &source), expected_get);
    let Ok((view, _)) = expected_get else { return };
    let edited = match &case.edited {
        None => view,
        Some(tree) => {
            let Ok((graph, _)) = panproto_reference::encode::tree_to_graph(tree, None) else {
                return;
            };
            graph
        }
    };
    let expected_put = reference_put(pair, &source, &edited);
    assert_eq!(production_put(pair, &source, &edited), expected_put);
    assert_eq!(reference_strict_put(pair, &source, &edited), expected_put);
}

fn reference_tree_square(pair: &Pair, case: &TreeSquareCase) -> SquareOutcome {
    assert_direct_graph_route_operations(pair, case);
    pair.with_reference(|lens| {
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
                    .expect("an edited square is constructed only after a successful tree get");
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
}

fn graph_square_units(_stages: &[BoundedLensStage]) -> Vec<(usize, RefTree)> {
    let mut units = Vec::new();
    for_each_bounded_lens_tree_square_case(|case| units.push((case.stage, case.source)));
    units
}

fn assert_graph_square_carrier_metadata(stages: &[BoundedLensStage], units: &[(usize, RefTree)]) {
    let certificate = bounded_lens_tree_square_count_certificate();
    let path_certificate = bounded_lens_path_count_certificate();
    let schemas = sigma2_schemas();
    let nonfan_stages = stages
        .iter()
        .filter(|stage| stage.source != bounded_lens_fan_schema())
        .count();
    let schema_tree_states: usize = schemas
        .iter()
        .map(|schema| bounded_lens_tree_states(schema).len())
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
    let certificate = bounded_lens_tree_square_count_certificate();
    assert_eq!(edit_families.total(), edits);
    println!(
        "bounded lens graph/tree square: bases={bases}, edits={edits}, admitted-positive families={:?}, refused-positive families={:?}, designated-negative families={:?}, total={}",
        edit_families.admitted_positive.named(),
        edit_families.refused_positive.named(),
        edit_families.designated_negative.named(),
        bases + edits
    );
    assert_eq!(
        edit_families.admitted_positive.families,
        [
            6_360, 0, 6_504, 3_232, 56, 6_504, 3_232, 1_728, 0, 1_152, 0, 0, 0, 0, 0, 0, 0,
        ],
        "the admitted graph/tree-square mutation-family census is part of the certificate"
    );
    assert_eq!(
        edit_families.refused_positive.families,
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        "the refused graph/tree-square mutation-family census is part of the certificate"
    );
    assert_eq!(
        edit_families.designated_negative.families,
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 48, 0, 48, 0, 0],
        "the designated-negative graph/tree-square mutation-family census is part of the certificate"
    );
    assert_eq!(edits, certificate.edited_views);
    assert_eq!(bases + edits, certificate.observations);
    assert_eq!(compared, u64::try_from(certificate.observations).unwrap());
}

fn graph_square_bound() -> String {
    let bound = "bounded lens graph/tree square: every one-step tree SSB view".to_owned();
    println!("{bound}");
    bound
}

fn assert_graph_square_agrees_with_the_reference_exhaustively(units: &[(usize, RefTree)]) {
    let stages = bounded_lens_stages();
    assert_graph_square_carrier_metadata(&stages, units);
    let bound = graph_square_bound();
    let edits = std::sync::atomic::AtomicUsize::new(0);
    let edit_families = AtomicClassifiedEditCensus::default();
    let compared = assert_agree(agree_in_parallel(
        &bound,
        units,
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
                    let partition = classify_reference_basis_case(&case.domain, observed, || {
                        format!("tree square stage {stage}, basis ordinal {}", case.ordinal)
                    });
                    basis_census.add_reference_basis(case.mutation, partition);
                }
                assert_eq!(
                    basis_census.admitted_positive,
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
        |pairs, case| reference_tree_square(&pairs[case.stage], case),
        squares_agree,
    ));
    let edits = edits.into_inner();
    let edit_families = edit_families.into_inner();
    assert_graph_square_census(units.len(), edits, &edit_families, compared);
}

#[test]
fn graph_square_agrees_with_the_reference_exhaustively() {
    let stages = bounded_lens_stages();
    let units = graph_square_units(&stages);
    assert_graph_square_agrees_with_the_reference_exhaustively(&units);
}

#[test]
fn graph_tree_route_law_certificate() {
    let mut squares = Vec::new();
    let mut validations = 0_usize;
    for_each_graph_tree_route_law_case(|case| match case {
        GraphTreeRouteLawCase::Square(case) => squares.push((case.stage, case.source)),
        GraphTreeRouteLawCase::Validation(case) => {
            validations += 1;
            assert_tree_validation_case(case);
        }
    });
    assert_eq!(squares.len(), 2_548);
    assert_eq!(validations, 2);
    assert_eq!(
        squares.len() + validations,
        graph_tree_route_law_case_count_certificate()
    );
    assert_eq!(
        bounded_lens_tree_square_count_certificate().observations + validations,
        graph_tree_route_law_observation_count_certificate()
    );
    assert_graph_square_agrees_with_the_reference_exhaustively(&squares);
}

#[test]
fn graph_square_falls_back_before_a_distinct_tree_refusal() {
    let stages = bounded_lens_stages();
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
                panproto_reference::enumerate::lenses::bounded_lens_edge("a", "b"),
            ),
            (
                2,
                1,
                panproto_reference::enumerate::lenses::bounded_lens_edge("b", "a"),
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
            panproto_reference::enumerate::lenses::bounded_lens_edge("a", "a"),
        ));

        assert_eq!(
            RefTreeShape::check_put_domain(lens, &view, &complement, &()),
            Err(panproto_reference::lens::ReferenceLawError::PutDomain(
                vec![
                    panproto_reference::lens::PutDomainError::SynthesizedEdited {
                        elem: panproto_reference::lens::ViewRef::Arc {
                            parent: 0,
                            child: 1,
                        },
                    },
                    panproto_reference::lens::PutDomainError::ContractedEdgeInserted {
                        elem: panproto_reference::lens::ViewRef::Arc {
                            parent: 0,
                            child: 3,
                        },
                    },
                ],
            )),
            "the tree law route reports the complete ordered native-arc refusal before the square projects its first graph-coordinate refusal"
        );

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

fn assert_randomized_graph_square_case(case_index: usize, edit_index: usize) {
    let carrier = path_property_carrier();
    // Keep the generated base fixed. A refusing get or an empty edit basis is
    // itself an observation; scanning forward would bias the property carrier.
    let (stage, source) = carrier.tree_units[case_index % carrier.tree_units.len()];
    let pair = Pair::from_stage(&carrier.stages[stage]);
    let tree = &carrier.trees[stage][source];

    let base = TreeSquareCase {
        stage,
        source: tree.clone(),
        edited: None,
    };
    let production = production_tree_square(&pair, &base);
    let reference = reference_tree_square(&pair, &base);
    assert!(
        squares_agree(&production, &reference),
        "unedited graph/tree square differs at stage {stage}, source {source}"
    );

    let basis = pair.with_reference(|lens| {
        let lens = lens.ok()?;
        let (view, complement) = RefTreeShape::get(lens, tree, &()).ok()?;
        let basis = RefTreeShape::single_step_basis(lens, tree, &view, &complement, &());
        (!basis.is_empty()).then_some(basis)
    });
    let Some(basis) = basis else {
        return;
    };
    let edit = edit_index % basis.len();
    assert_eq!(
        production_tree_put(&pair, tree, &basis[edit].view),
        reference_tree_put(&pair, tree, &basis[edit].view),
        "native tree put/domain differs at stage {stage}, source {source}, edit {edit}"
    );
    let edited = TreeSquareCase {
        stage,
        source: tree.clone(),
        edited: Some(basis[edit].view.clone()),
    };
    let production = production_tree_square(&pair, &edited);
    let reference = reference_tree_square(&pair, &edited);
    assert!(
        squares_agree(&production, &reference),
        "edited graph/tree square differs at stage {stage}, source {source}, edit {edit}"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::default())]

    #[test]
    fn randomized_graph_tree_route_law_differential(
        pair in (any::<i16>(), any::<i16>())
            .prop_filter("source keys differ", |(left, right)| left != right),
        case in any::<usize>(),
        edit in any::<usize>(),
    ) {
        let (left, right) = pair;
        assert_tree_validation_case(
            panproto_reference::enumerate::lenses::TreeValidationCase::RequiredDeletion {
                elem: 0,
                field: "b",
            },
        );
        assert_tree_validation_case(
            panproto_reference::enumerate::lenses::TreeValidationCase::DuplicateKeyAfterEdit {
                table: "a",
                field: "b",
                repeated_value: i64::from(left),
                replaced_value: i64::from(right),
                elems: [1, 2],
            },
        );
        assert_randomized_graph_square_case(case, edit);
    }

    #[test]
    fn randomized_graph_square_differential(
        case in any::<usize>(),
        edit in any::<usize>(),
    ) {
        assert_randomized_graph_square_case(case, edit);
    }
}

fn table_laws_agree_with_the_reference_on_cases(fixtures: &[TableFixture]) {
    let certificate = table_fixture_count_certificate();
    assert_eq!(fixtures.len(), certificate.total);
    assert_eq!(certificate.total, 9_921);
    let bound = format!(
        "table law reports: rows<=2, columns<=2, cell alphabet=7, lenses=7, source/lens cases={}",
        certificate.total
    );
    println!("{bound}");
    let compared = assert_agree(agree_in_parallel(
        &bound,
        fixtures,
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
fn graph_put_law_certificate() {
    let mut contracts = 0_usize;
    let mut carriers = Vec::new();
    for_each_graph_put_law_case(|case| match case {
        GraphPutLawCase::Contract(case) => {
            contracts += 1;
            assert_graph_put_contract_case(case);
        }
        GraphPutLawCase::Carrier { stage, source } => carriers.push((stage, source)),
    });
    assert_eq!(contracts, 21);
    assert_eq!(carriers.len(), 144_397);
    assert_eq!(
        contracts + carriers.len(),
        graph_put_law_case_count_certificate()
    );
    graph_lens_agrees_with_the_reference_on_cases(&carriers);
}

#[test]
fn tree_put_law_certificate() {
    let mut validations = 0_usize;
    let mut carriers = Vec::new();
    for_each_tree_put_law_case(|case| match case {
        TreePutLawCase::Validation(case) => {
            validations += 1;
            assert_tree_validation_case(case);
        }
        TreePutLawCase::Carrier { stage, source } => carriers.push((stage, source)),
    });
    assert_eq!(validations, 3);
    assert_eq!(carriers.len(), 2_548);
    assert_eq!(
        validations + carriers.len(),
        tree_put_law_case_count_certificate()
    );
    tree_lens_agrees_with_the_reference_on_cases(&carriers);
    assert_graph_square_agrees_with_the_reference_exhaustively(&carriers);
}

// ---------------------------------------------------------------------------
// Serialized views: alignment to the recorded positions, then put
// ---------------------------------------------------------------------------

/// `tree` numbered as a reader that keeps no node ids numbers it: breadth
/// first from the root, children in arc order, from 0, every node no arc
/// reaches after them in id order. Arc order and fans are kept.
fn renumbered(tree: &RefTree) -> RefTree {
    let mut order = vec![tree.root];
    let mut next = 0;
    while next < order.len() {
        let parent = order[next];
        next += 1;
        for (_, child, _) in tree.arcs.iter().filter(|(p, _, _)| *p == parent) {
            if !order.contains(child) {
                order.push(*child);
            }
        }
    }
    let unreached: Vec<u32> = tree
        .nodes
        .keys()
        .copied()
        .filter(|id| !order.contains(id))
        .collect();
    order.extend(unreached);
    let rename: BTreeMap<u32, u32> = order
        .iter()
        .enumerate()
        .map(|(new, old)| (*old, u32::try_from(new).expect("small tree")))
        .collect();
    let id = |old: u32| rename.get(&old).copied().unwrap_or(old);
    RefTree {
        nodes: tree
            .nodes
            .values()
            .map(|node| {
                let mut node = node.clone();
                node.id = id(node.id);
                (node.id, node)
            })
            .collect(),
        arcs: tree
            .arcs
            .iter()
            .map(|(p, c, e)| (id(*p), id(*c), e.clone()))
            .collect(),
        fans: tree
            .fans
            .iter()
            .map(|fan| RefFan {
                parent: id(fan.parent),
                hyper_edge: fan.hyper_edge.clone(),
                children: fan
                    .children
                    .iter()
                    .map(|(l, c)| (l.clone(), id(*c)))
                    .collect(),
            })
            .collect(),
        root: id(tree.root),
    }
}

/// The serialized views of a get view `view`, each renumbered as a reader
/// numbers it: `view` itself; `view` with its last arc's child removed
/// when that child is a leaf in no fan; and `view` with a copy of that
/// leaf added under the same parent and edge after it.
fn serialized_views(view: &RefTree) -> Vec<RefTree> {
    let mut views = vec![renumbered(view)];
    let fanned: BTreeSet<u32> = view
        .fans
        .iter()
        .flat_map(|fan| std::iter::once(fan.parent).chain(fan.children.values().copied()))
        .collect();
    if let Some((parent, leaf, edge)) = view.arcs.last().cloned()
        && !view.arcs.iter().any(|(p, _, _)| *p == leaf)
        && !fanned.contains(&leaf)
    {
        let mut deleted = view.clone();
        deleted.arcs.pop();
        deleted.nodes.remove(&leaf);
        views.push(renumbered(&deleted));
        let mut inserted = view.clone();
        let fresh = view.nodes.keys().max().copied().unwrap_or(0) + 1;
        let mut copy = view.nodes[&leaf].clone();
        copy.id = fresh;
        inserted.nodes.insert(fresh, copy);
        inserted.arcs.push((parent, fresh, edge));
        views.push(renumbered(&inserted));
    }
    views
}

/// The observable outcome of a serialized put: the positions get recorded,
/// and for each serialized view its alignment and the aligned view's put
/// and put-domain report.
#[derive(Debug, PartialEq, Eq)]
struct AlignedPutOutcome {
    positions: BTreeMap<u32, String>,
    puts: Vec<(Result<RefTree, String>, Option<TreePutOutcome>)>,
}

fn production_position(position: &panproto_inst::ViewPosition) -> String {
    match position {
        panproto_inst::ViewPosition::Root { anchor } => format!("root {anchor}"),
        panproto_inst::ViewPosition::Child {
            parent,
            edge,
            index,
        } => format!("child {parent} {:?} {index}", ref_edge(edge)),
    }
}

fn reference_position(position: &panproto_reference::lens::ViewPosition) -> String {
    match position {
        panproto_reference::lens::ViewPosition::Root { anchor } => format!("root {anchor}"),
        panproto_reference::lens::ViewPosition::Child {
            parent,
            edge,
            index,
        } => format!("child {parent} {edge:?} {index}"),
    }
}

fn production_alignment_reason(error: &LensError) -> String {
    match error {
        LensError::ViewAlignment(panproto_lens::ViewAlignmentError::PositionsNotRecorded) => {
            "positions not recorded".to_owned()
        }
        LensError::ViewAlignment(panproto_lens::ViewAlignmentError::MalformedPositions {
            ..
        }) => "malformed positions".to_owned(),
        LensError::ViewAlignment(panproto_lens::ViewAlignmentError::RootAnchorMismatch {
            recorded,
            found,
        }) => format!("root anchor {recorded} {found}"),
        LensError::ViewAlignment(panproto_lens::ViewAlignmentError::IdsExhausted) => {
            "ids exhausted".to_owned()
        }
        other => format!("{other:?}"),
    }
}

fn reference_alignment_reason(error: &panproto_reference::lens::ViewAlignmentRefusal) -> String {
    use panproto_reference::lens::ViewAlignmentRefusal as Refusal;
    match error {
        Refusal::PositionsNotRecorded => "positions not recorded".to_owned(),
        Refusal::MalformedPositions => "malformed positions".to_owned(),
        Refusal::RootAnchorMismatch { recorded, found } => {
            format!("root anchor {recorded} {found}")
        }
        Refusal::IdsExhausted => "ids exhausted".to_owned(),
    }
}

fn production_aligned_put(pair: &Pair, source: &RefTree) -> Option<AlignedPutOutcome> {
    let (view, complement) = panproto_lens::get(&pair.production, &prod_tree(source)).ok()?;
    let positions = complement
        .view_positions
        .iter()
        .map(|(id, position)| (*id, production_position(position)))
        .collect();
    let puts = serialized_views(&ref_tree(&view))
        .iter()
        .map(|shown| {
            match panproto_lens::align_view_to_complement(
                &pair.production,
                &prod_tree(shown),
                &complement,
            ) {
                Ok(aligned) => {
                    let put = Some(TreePutOutcome {
                        put: panproto_lens::put(&pair.production, &aligned, &complement)
                            .map(|restored| ref_tree(&restored))
                            .map_err(|error| lens_reason(&error)),
                        domain: panproto_lens::asymmetric::check_put_domain(
                            &pair.production,
                            &aligned,
                            &complement,
                        )
                        .map_err(|errors| errors.iter().map(put_domain_reason).collect()),
                    });
                    (Ok(ref_tree(&aligned)), put)
                }
                Err(error) => (Err(production_alignment_reason(&error)), None),
            }
        })
        .collect();
    Some(AlignedPutOutcome { positions, puts })
}

fn reference_aligned_put(pair: &Pair, source: &RefTree) -> Option<AlignedPutOutcome> {
    pair.with_reference(|lens| {
        let lens = lens.ok()?;
        let (view, complement) = RefTreeShape::get(lens, source, &()).ok()?;
        let positions = complement
            .view_positions
            .iter()
            .map(|(id, position)| (*id, reference_position(position)))
            .collect();
        let puts = serialized_views(&view)
            .iter()
            .map(|shown| {
                match panproto_reference::lens::align_tree_view(
                    shown,
                    &complement.view_positions,
                    &complement.complement,
                ) {
                    Ok(aligned) => {
                        let put = Some(TreePutOutcome {
                            put: RefTreeShape::put(lens, &aligned, &complement, &())
                                .map_err(|error| reference_tree_law_reason(&error)),
                            domain: RefTreeShape::check_put_domain(
                                lens,
                                &aligned,
                                &complement,
                                &(),
                            )
                            .map_err(|error| match error {
                                panproto_reference::lens::ReferenceLawError::PutDomain(errors) => {
                                    errors.iter().map(ref_put_domain_reason).collect()
                                }
                                other => panic!("unexpected reference domain refusal: {other:?}"),
                            }),
                        });
                        (Ok(aligned), put)
                    }
                    Err(error) => (Err(reference_alignment_reason(&error)), None),
                }
            })
            .collect();
        Some(AlignedPutOutcome { positions, puts })
    })
}

/// On every tree put-law carrier, production and the reference record the
/// same view positions at get, align the same serialized views (the view
/// renumbered as a reader numbers it, with its last leaf deleted, and with
/// a copy of that leaf inserted) to the same trees, and put the aligned
/// views alike, refusals and put-domain reports included.
#[test]
fn tree_aligned_put_agrees_with_the_reference() {
    let mut carriers = Vec::new();
    for_each_tree_put_law_case(|case| {
        if let TreePutLawCase::Carrier { stage, source } = case {
            carriers.push((stage, source));
        }
    });
    assert_eq!(carriers.len(), 2_548);
    let stages = bounded_lens_stages();
    let bound = format!(
        "aligned tree put: (stage, tree) carriers={}, three serialized views each at most",
        carriers.len()
    );
    println!("{bound}");
    let edited = std::sync::atomic::AtomicUsize::new(0);
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &carriers,
        || stages.iter().map(Pair::from_stage).collect::<Vec<Pair>>(),
        |_, case| vec![case],
        |pairs, (stage, tree)| {
            // Outside the reference tree domain (a stage dropping an edge
            // between surviving vertices) there is no tree put to compare.
            let inside = reference_aligned_put(&pairs[*stage], tree).is_some();
            let outcome = inside
                .then(|| production_aligned_put(&pairs[*stage], tree))
                .flatten();
            if let Some(outcome) = &outcome {
                edited.fetch_add(
                    outcome.puts.len().saturating_sub(1),
                    std::sync::atomic::Ordering::Relaxed,
                );
            }
            outcome
        },
        |pairs, (stage, tree)| reference_aligned_put(&pairs[*stage], tree),
        |production, reference| production == reference,
    ));
    assert_eq!(compared, u64::try_from(carriers.len()).unwrap());
    let edited = edited.into_inner();
    println!("aligned tree put: edited serialized views={edited}");
    assert!(edited > 0, "some carrier has a leaf to delete and copy");
}

#[test]
fn lens_equivalence_law_certificate() {
    let mut bases = [0_usize; 3];
    let mut graphs = Vec::new();
    let mut trees = Vec::new();
    let mut tables = Vec::new();
    for_each_lens_equivalence_case(|case| match case {
        LensEquivalenceCase::Graph { stage, source } => {
            bases[0] += 1;
            graphs.push((stage, source));
        }
        LensEquivalenceCase::Tree { stage, source } => {
            bases[1] += 1;
            trees.push((stage, source));
        }
        LensEquivalenceCase::Table(fixture) => {
            bases[2] += 1;
            tables.push(fixture);
        }
    });
    assert_eq!(bases, [144_397, 2_548, 9_921]);
    assert_eq!(graphs.len() + trees.len() + tables.len(), 156_866);
    graph_lens_agrees_with_the_reference_on_cases(&graphs);
    tree_lens_agrees_with_the_reference_on_cases(&trees);
    table_laws_agree_with_the_reference_on_cases(&tables);
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
            detail: "the L-lenses-19 precedence witness must not evaluate this transform"
                .to_owned(),
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
                detail: "the one-shot L-lenses-19 schema driver was invoked twice".to_owned(),
            })
    }
}

fn protolens_add_enrichment(
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

fn protolens_expansion_source() -> Schema {
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

fn protolens_overlong_expansion_target() -> Schema {
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

/// The elementary protolens alphabet over the bounded lens schemas: drop and rename
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

/// The four bounded lens protolenses constructed without consulting production's
/// applicability verdict, target schema, compiled migration or plan.
fn reference_protolens(index: usize, schema: &RefSchema) -> RefProtolens {
    match index {
        0 => {
            // Dropping a sort removes every vertex whose id or kind is the
            // name, and its precondition admits exactly those schemas.
            let applies = schema
                .vertices
                .iter()
                .any(|(id, kind)| id == "obj" || kind == "obj");
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
            .chain(
                compiled
                    .op_term_assignments
                    .iter()
                    .filter(|(_, assignments)| !assignments.is_empty())
                    .map(|(vertex, _)| vertex.to_string()),
            )
            .collect(),
        output_fields: reference_output_fields(compiled),
        ..RefMigration::default()
    }
}

#[test]
fn undeclared_transform_stages_keep_every_original_source_as_a_refusal() {
    let stages = bounded_lens_stages();
    assert_eq!(stages.len(), 31);
    let mut cases = [0_usize; 3];
    for name in ["T", "ReadF", "Op"] {
        let stage = &stages[bounded_lens_stage_index(&stages, name)];
        let pair = Pair::from_stage(stage);
        let expected = Reason::Restrict("UndeclaredTransformField a a f".to_owned());
        let plan_error = PlanError::UndeclaredTransformField {
            source_vertex: "a".to_owned(),
            target_vertex: "a".to_owned(),
            field: "f".to_owned(),
        };
        assert_eq!(
            panproto_reference::restrict::compile(&stage.source, &stage.target, &stage.migration),
            Err(plan_error.clone())
        );
        assert_eq!(pair.plan, Err(plan_error.clone()));
        for source in bounded_lens_sources(stage) {
            assert_eq!(reference_get(&pair, &source), Err(expected.clone()));
            assert_eq!(production_get(&pair, &source), Err(expected.clone()));
            cases[0] += 1;
        }
        for source in bounded_lens_tree_states(&stage.source) {
            assert_eq!(
                reference_tree_case(&pair, &source),
                Some(Err(expected.clone()))
            );
            assert_eq!(
                production_tree_case(&pair, &source, true),
                Some(Err(expected.clone()))
            );
            cases[1] += 1;
        }
        let table = TablePair::from_stage(stage);
        assert_eq!(
            panproto_reference::table::compile_for_tables(
                &table.src,
                &table.tgt,
                &table.boundaries.0,
                &table.boundaries.1,
                &table.migration,
            ),
            Err(TablePlanError::Plan(Box::new(plan_error)))
        );
        for source in bounded_lens_table_states(&stage.source) {
            assert_eq!(table.try_with_reference(|_, _| ()), Err(expected.clone()));
            assert_eq!(production_table_get(&table, &source), Err(expected.clone()));
            cases[2] += 1;
        }
    }
    assert_eq!(cases, [45, 6, 45]);
    println!(
        "undeclared transform source refusals: graph={}, tree={}, table={}, total={}",
        cases[0],
        cases[1],
        cases[2],
        cases.iter().sum::<usize>()
    );
}

#[test]
fn declared_transform_absence_complements_separate_the_default_collision() {
    let stages = panproto_reference::enumerate::lenses::declared_transform_stages();
    let pair = TablePair::from_stage(&stages[bounded_lens_stage_index(&stages, "DeclaredDefault")]);
    let mut views = Vec::new();
    let mut complements = Vec::new();
    for source_value in [None, Some(0)] {
        let source = RefTable {
            rows: BTreeMap::from([(
                0,
                (
                    "a".to_owned(),
                    source_value
                        .map(|value| ("f".to_owned(), RefValue::Int(value)))
                        .into_iter()
                        .collect(),
                ),
            )]),
            pairs: BTreeSet::new(),
        };
        let expected = reference_table_get(&pair, &source).expect("finite reference get");
        assert_eq!(production_table_get(&pair, &source), Ok(expected.clone()));
        let markers = if source_value.is_none() {
            BTreeSet::from([(0, "f".to_owned())])
        } else {
            BTreeSet::new()
        };
        assert_eq!(expected.1.original_absent_cells, markers);
        let expected_put = reference_table_put(&pair, &source, &expected.0);
        assert_eq!(expected_put.put, Ok(source.clone()));
        assert_eq!(
            production_table_put(&pair, &source, &expected.0),
            expected_put
        );
        views.push(expected.0);
        complements.push(expected.1);
    }
    assert_eq!(views[0], views[1]);
    assert_ne!(complements[0], complements[1]);
    assert_table_complement_field_order();
}

#[test]
fn declared_transform_absence_composition_matches_independent_union() {
    use panproto_lens::ComplementCompose;
    let stages = panproto_reference::enumerate::lenses::declared_transform_stages();
    let pair = TablePair::from_stage(&stages[bounded_lens_stage_index(&stages, "DeclaredDefault")]);
    let source = RefTable {
        rows: BTreeMap::from([(0, ("a".to_owned(), BTreeMap::new()))]),
        pairs: BTreeSet::new(),
    };
    let (_, left) = table_lens::get(&pair.production, &prod_table(&source), pair.protocols())
        .expect("default inserts f");
    let mut right = left.clone();
    right.original_absent_cells = BTreeSet::from([(RowId(1), "f".to_owned())]);
    right.id_floor = 2;
    let mut expected = ref_table_complement(&left);
    expected
        .original_absent_cells
        .extend(ref_table_complement(&right).original_absent_cells);
    expected.id_floor = 2;
    for (one, two) in [(&left, &right), (&right, &left)] {
        assert_eq!(
            ref_table_complement(&one.compose(two).expect("compatible absence union")),
            expected
        );
    }
    let mut present = TableComplement {
        identity: left.identity.clone(),
        ..TableComplement::empty()
    };
    present
        .original_cells
        .insert((RowId(0), "f".to_owned()), Value::Int(0));
    assert!(matches!(
        left.compose(&present),
        Err(LensError::ComplementConflict { .. })
    ));
    assert!(matches!(
        present.compose(&left),
        Err(LensError::ComplementConflict { .. })
    ));
    let mut malformed = left;
    malformed.original_cells = present.original_cells;
    assert!(matches!(
        malformed.compose(&TableComplement::empty()),
        Err(LensError::ComplementConflict { .. })
    ));
    assert!(matches!(
        TableComplement::empty().compose(&malformed),
        Err(LensError::ComplementConflict { .. })
    ));
}

/// The declared-field carrier's root with its attribute `f` held by the
/// child `value` rather than by the root's own fields.
fn declared_attribute_on_child(value: i64) -> RefTree {
    let mut child = RefNode::new(1, "value");
    child.value = Some(RefPresence::Present(RefValue::Int(value)));
    RefTree {
        nodes: BTreeMap::from([(0, RefNode::new(0, "a")), (1, child)]),
        arcs: vec![(
            0,
            1,
            RefSEdge::new("a", "value", "prop", Some(BOUNDED_LENS_FIELD)),
        )],
        fans: Vec::new(),
        root: 0,
    }
}

#[test]
fn declared_read_of_a_child_attribute_writes_it_back_to_the_child() {
    let stages = panproto_reference::enumerate::lenses::declared_transform_stages();
    let pair = Pair::from_stage(&stages[bounded_lens_stage_index(&stages, "DeclaredRead")]);
    for value in [0, 1] {
        let source = declared_attribute_on_child(value);
        let production = production_tree_case(&pair, &source, true);
        assert_eq!(
            production,
            reference_tree_case(&pair, &source),
            "f = {value}"
        );
        let Some(Ok((view, _, restored))) = production else {
            panic!("the read admits the child attribute f = {value}");
        };
        // The attribute stays where it lives: the child holds the read
        // value and the root gains no second value for `f`.
        assert!(view.nodes[&0].fields.is_empty(), "{view:?}");
        assert_eq!(view, source);
        assert_eq!(restored, Ok(source.clone()));
        // A view showing the child attribute edited is put back onto the
        // child, and get of that put shows the edit.
        let edited = declared_attribute_on_child(1 - value);
        let put = production_tree_put(&pair, &source, &edited);
        assert_eq!(put, reference_tree_put(&pair, &source, &edited));
        let restored = put
            .and_then(|outcome| outcome.put.ok())
            .unwrap_or_else(|| panic!("put of the edited child attribute f = {value}"));
        assert_eq!(restored, edited);
    }
}

/// A default or computed field presented as a child on the edge it names,
/// the way `parse_json` reads a serialized view, is a child the source never
/// held: tree put refuses it in production exactly as in the reference.
/// Folding the child back into its node's field recovers the view `get`
/// wrote, which both engines put back to the source.
#[test]
fn a_derived_field_presented_as_a_child_is_refused_like_the_reference() {
    let stages = panproto_reference::enumerate::lenses::declared_transform_stages();
    for name in ["DeclaredDefault", "DeclaredCompute"] {
        let pair = Pair::from_stage(&stages[bounded_lens_stage_index(&stages, name)]);
        let source = RefTree {
            nodes: BTreeMap::from([(0, RefNode::new(0, "a"))]),
            arcs: Vec::new(),
            fans: Vec::new(),
            root: 0,
        };
        let production = production_tree_case(&pair, &source, true);
        assert_eq!(production, reference_tree_case(&pair, &source), "{name}");
        let Some(Ok((view, _, restored))) = production else {
            panic!("{name}: get admits the bare root");
        };
        assert_eq!(restored, Ok(source.clone()), "{name}");
        let value = view.nodes[&0]
            .fields
            .get(BOUNDED_LENS_FIELD)
            .unwrap_or_else(|| panic!("{name}: get writes `{BOUNDED_LENS_FIELD}` on the root"))
            .clone();

        let mut child_form = view.clone();
        child_form
            .nodes
            .get_mut(&0)
            .expect("root")
            .fields
            .remove(BOUNDED_LENS_FIELD);
        let mut child = RefNode::new(1, "value");
        child.value = Some(RefPresence::Present(value));
        child_form.nodes.insert(1, child);
        child_form.arcs.push((
            0,
            1,
            RefSEdge::new("a", "value", "prop", Some(BOUNDED_LENS_FIELD)),
        ));

        let production = production_tree_put(&pair, &source, &child_form).expect("production get");
        let reference = reference_tree_put(&pair, &source, &child_form).expect("reference get");
        let refusal = format!("DerivedCellEdited Node(0) {BOUNDED_LENS_FIELD}");
        assert_eq!(
            reference.put,
            Err(Reason::PutDomain(refusal.clone())),
            "{name}"
        );
        assert_eq!(reference.domain, Err(vec![refusal]), "{name}");
        assert_eq!(production, reference, "{name}");

        let (_, complement) =
            panproto_lens::get(&pair.production, &prod_tree(&source)).expect("production get");
        let folded = panproto_lens::fold_computed_children_into_fields(
            &pair.production,
            &prod_tree(&child_form),
            &complement,
        );
        assert_eq!(
            ref_tree(&folded),
            view,
            "{name}: the fold recovers the view"
        );
        let production = production_tree_put(&pair, &source, &ref_tree(&folded));
        assert_eq!(
            production,
            reference_tree_put(&pair, &source, &view),
            "{name}"
        );
        assert_eq!(
            production.and_then(|outcome| outcome.put.ok()),
            Some(source.clone()),
            "{name}"
        );
    }
}

/// A source `r` with sibling fields `a` and `b` of vertex `s`, the target
/// keeping only `a`; the edge map sends `b` to `a` as well.
fn sibling_merge_pair() -> Pair {
    let source = RefSchema::default()
        .vertex("r", "record")
        .vertex("s", "record")
        .edge("r", "s", "prop", Some("a"))
        .edge("r", "s", "prop", Some("b"));
    let target = RefSchema::default()
        .vertex("r", "record")
        .vertex("s", "record")
        .edge("r", "s", "prop", Some("a"));
    let mut migration = RefMigration {
        surviving_verts: ["r", "s"].into_iter().map(str::to_owned).collect(),
        ..RefMigration::default()
    };
    migration.edge_remap.insert(
        RefSEdge::new("r", "s", "prop", Some("b")),
        RefSEdge::new("r", "s", "prop", Some("a")),
    );
    Pair::new(source, target, migration)
}

/// A source `r` holding `leaf` directly and through the dropped wrapper `m`,
/// the target keeping `r -leaf-> leaf` alone: contraction flattens the
/// nested `leaf` onto the edge the direct one keeps.
fn flattened_field_pair() -> Pair {
    let source = RefSchema::default()
        .vertex("r", "record")
        .vertex("m", "record")
        .vertex("leaf", "record")
        .edge("r", "m", "prop", Some("m"))
        .edge("m", "leaf", "prop", Some("leaf"))
        .edge("r", "leaf", "prop", Some("leaf"));
    let target = RefSchema::default()
        .vertex("r", "record")
        .vertex("leaf", "record")
        .edge("r", "leaf", "prop", Some("leaf"));
    let migration = RefMigration {
        surviving_verts: ["r", "leaf"].into_iter().map(str::to_owned).collect(),
        ..RefMigration::default()
    };
    Pair::new(source, target, migration)
}

fn ref_tree_of(nodes: &[(u32, &str)], arcs: &[(u32, u32, RefSEdge)]) -> RefTree {
    RefTree {
        nodes: nodes
            .iter()
            .map(|(id, anchor)| (*id, RefNode::new(*id, anchor)))
            .collect(),
        arcs: arcs.to_vec(),
        fans: Vec::new(),
        root: 0,
    }
}

#[test]
fn two_values_on_a_single_valued_edge_are_refused_on_both_sides() {
    let field = |src: &str, tgt: &str, name: &str| RefSEdge::new(src, tgt, "prop", Some(name));
    let refused = |parent: u32, edge: &RefSEdge, count: usize| {
        Some(Err(Reason::Restrict(format!(
            "MultipleArcsOnSingleValuedEdge {parent} {edge:?} {count}"
        ))))
    };

    // Sibling fields merged by the edge map.
    let merge = sibling_merge_pair();
    let siblings = ref_tree_of(
        &[(0, "r"), (1, "s"), (2, "s")],
        &[(0, 1, field("r", "s", "a")), (0, 2, field("r", "s", "b"))],
    );
    let expected = refused(0, &field("r", "s", "a"), 2);
    assert_eq!(production_tree_case(&merge, &siblings, true), expected);
    assert_eq!(reference_tree_case(&merge, &siblings), expected);
    // Arcs of the one source edge `a` keep their multiplicity.
    let repeated = ref_tree_of(
        &[(0, "r"), (1, "s"), (2, "s")],
        &[(0, 1, field("r", "s", "a")), (0, 2, field("r", "s", "a"))],
    );
    let production = production_tree_case(&merge, &repeated, true);
    assert!(matches!(production, Some(Ok(_))), "{production:?}");
    assert_eq!(production, reference_tree_case(&merge, &repeated));

    // A direct field and a flattened nested field joined by contraction.
    let flatten = flattened_field_pair();
    let joined = ref_tree_of(
        &[(0, "r"), (1, "leaf"), (2, "m"), (3, "leaf")],
        &[
            (0, 1, field("r", "leaf", "leaf")),
            (0, 2, field("r", "m", "m")),
            (2, 3, field("m", "leaf", "leaf")),
        ],
    );
    let expected = refused(0, &field("r", "leaf", "leaf"), 2);
    assert_eq!(production_tree_case(&flatten, &joined, true), expected);
    assert_eq!(reference_tree_case(&flatten, &joined), expected);
    // The nested field alone flattens onto the edge.
    let nested = ref_tree_of(
        &[(0, "r"), (2, "m"), (3, "leaf")],
        &[
            (0, 2, field("r", "m", "m")),
            (2, 3, field("m", "leaf", "leaf")),
        ],
    );
    let production = production_tree_case(&flatten, &nested, true);
    assert!(matches!(production, Some(Ok(_))), "{production:?}");
    assert_eq!(production, reference_tree_case(&flatten, &nested));

    // The graph route holds the same arcs as parallel edges on both sides.
    let graph = RefGraph {
        nodes: siblings.nodes.clone(),
        edges: siblings
            .arcs
            .iter()
            .zip(0_u32..)
            .map(|((src, tgt, label), id)| {
                (
                    id,
                    RefGEdge {
                        src: *src,
                        tgt: *tgt,
                        label: label.clone(),
                        position: None,
                    },
                )
            })
            .collect(),
    };
    let production = production_get(&merge, &graph);
    assert_eq!(production, reference_get(&merge, &graph));
    let Ok((view, _)) = production else {
        panic!("the graph route refuses parallel edges: {production:?}");
    };
    let label = field("r", "s", "a");
    assert_eq!(
        view.edges
            .values()
            .filter(|edge| edge.src == 0 && edge.label == label)
            .count(),
        2
    );
}

#[test]
fn declared_transform_atomic_laws_agree_with_the_reference() {
    use panproto_reference::enumerate::lenses::{
        declared_transform_count_certificate, declared_transform_graphs, declared_transform_stages,
        declared_transform_tables, declared_transform_trees,
    };
    let stages = declared_transform_stages();
    let mut cases = [0_usize; 3];
    for stage in &stages {
        let pair = Pair::from_stage(stage);
        assert!(pair.plan.is_ok(), "{} compiles", stage.name);
        for source in declared_transform_graphs() {
            assert_eq!(
                production_get(&pair, &source),
                reference_get(&pair, &source),
                "{} graph get {source:?}",
                stage.name
            );
            let candidates = graph_candidates(&pair, &source);
            assert_eq!(
                graph_common_laws(&pair, &source, &candidates, true),
                graph_common_laws(&pair, &source, &candidates, false),
                "{} graph laws {source:?}",
                stage.name
            );
            cases[0] += 1;
        }
        for source in declared_transform_trees() {
            assert_eq!(
                production_tree_case(&pair, &source, true),
                reference_tree_case(&pair, &source),
                "{} tree get/put {source:?}",
                stage.name
            );
            let candidates = tree_candidates(&pair, &source);
            assert_eq!(
                tree_common_laws(&pair, &source, &candidates, true),
                tree_common_laws(&pair, &source, &candidates, false),
                "{} tree laws {source:?}",
                stage.name
            );
            cases[1] += 1;
        }
        let table = TablePair::from_stage(stage);
        assert!(table.plan.is_ok(), "{} table compiles", stage.name);
        for source in declared_transform_tables() {
            let candidates = table_candidates(&table, &source);
            assert_eq!(
                table_common_laws(&table, &source, &candidates, true),
                table_common_laws(&table, &source, &candidates, false),
                "{} table laws {source:?}",
                stage.name
            );
            cases[2] += 1;
        }
    }
    assert_eq!(cases, [65, 15, 65]);
    assert_eq!(
        cases.iter().sum::<usize>(),
        declared_transform_count_certificate().atomic_cases
    );
    println!(
        "declared transform atomic laws: graph={}, tree={}, table={}, total={}",
        cases[0],
        cases[1],
        cases[2],
        cases.iter().sum::<usize>()
    );
}

#[test]
fn declared_transform_wrappers_preserve_their_active_field_domain() {
    use panproto_expr::{Expr, Literal};
    use panproto_inst::{CaseBranch, FieldTransform};
    use panproto_reference::enumerate::lenses::declared_transform_stages;

    let stage = declared_transform_stages()
        .into_iter()
        .find(|stage| stage.layer == BoundedLensLayer::AddField)
        .expect("the separate declared carrier includes default insertion");
    let default = FieldTransform::AddField {
        key: BOUNDED_LENS_FIELD.to_owned(),
        value: Value::Int(0),
    };
    let wrappers = [
        (
            "empty path",
            true,
            FieldTransform::PathTransform {
                path: Vec::new(),
                inner: Box::new(default.clone()),
            },
        ),
        (
            "true case",
            true,
            FieldTransform::Case {
                branches: vec![CaseBranch {
                    predicate: Expr::Lit(Literal::Bool(true)),
                    transforms: vec![default.clone()],
                }],
            },
        ),
        (
            "false case",
            false,
            FieldTransform::Case {
                branches: vec![CaseBranch {
                    predicate: Expr::Lit(Literal::Bool(false)),
                    transforms: vec![default],
                }],
            },
        ),
    ];
    let mut cases = [0_usize; 2];
    for (name, active, transform) in wrappers {
        let mut production = production_stage(&stage);
        production
            .compiled
            .field_transforms
            .insert("a".into(), vec![transform]);
        assert_eq!(
            reference_output_fields(&production.compiled),
            stage.migration.output_fields,
            "inactive branches still contribute static output declarations"
        );
        let layer: &'static dyn ValueLayer = if active {
            &BoundedLensAddField
        } else {
            &NoValueLayer
        };
        let pair = Pair::with_production(
            stage.source.clone(),
            stage.target.clone(),
            stage.migration.clone(),
            layer,
            production,
        );
        assert!(pair.plan.is_ok(), "{name} has a declared output");
        assert_default_wrapper_edits(&pair, name, &mut cases);
    }
    assert_eq!(cases, [81, 81]);
    println!(
        "declared transform wrapper edits: graph={}, tree={}, total={}",
        cases[0],
        cases[1],
        cases.iter().sum::<usize>()
    );
}

fn assert_default_wrapper_edits(pair: &Pair, name: &str, cases: &mut [usize; 2]) {
    for source_field in [None, Some(0), Some(1)] {
        let mut node = RefNode::new(0, "a");
        node.fields.insert("other".to_owned(), RefValue::Int(0));
        if let Some(value) = source_field {
            node.fields
                .insert(BOUNDED_LENS_FIELD.to_owned(), RefValue::Int(value));
        }
        let source = RefGraph {
            nodes: [(0, node)].into(),
            edges: BTreeMap::new(),
        };
        let tree = RefTree {
            root: 0,
            nodes: source.nodes.clone(),
            arcs: Vec::new(),
            fans: Vec::new(),
        };
        assert_eq!(
            production_get(pair, &source),
            reference_get(pair, &source),
            "{name} graph get with source f={source_field:?}"
        );
        assert_eq!(
            production_tree_case(pair, &tree, true),
            reference_tree_case(pair, &tree),
            "{name} tree get with source f={source_field:?}"
        );
        for edited_field in [None, Some(0), Some(1)] {
            for other_field in [None, Some(0), Some(1)] {
                let mut edited = source.clone();
                let fields = &mut edited.nodes.get_mut(&0).expect("one source node").fields;
                fields.clear();
                for (key, value) in [(BOUNDED_LENS_FIELD, edited_field), ("other", other_field)] {
                    if let Some(value) = value {
                        fields.insert(key.to_owned(), RefValue::Int(value));
                    }
                }
                let expected = reference_put(pair, &source, &edited)
                    .expect("the reference wrapper has a successful get");
                assert_eq!(
                    production_put(pair, &source, &edited),
                    Some(expected),
                    "{name} graph source f={source_field:?}, edit f={edited_field:?}, other={other_field:?}"
                );
                cases[0] += 1;
                let edited_tree = RefTree {
                    nodes: edited.nodes,
                    ..tree.clone()
                };
                let expected = reference_tree_put(pair, &tree, &edited_tree)
                    .expect("the reference wrapper has a successful tree get");
                assert_eq!(
                    production_tree_put(pair, &tree, &edited_tree),
                    Some(expected),
                    "{name} tree source f={source_field:?}, edit f={edited_field:?}, other={other_field:?}"
                );
                cases[1] += 1;
            }
        }
    }
}

fn conditional_default_stage() -> (BoundedLensStage, Lens) {
    use panproto_expr::{BuiltinOp, Expr, Literal};
    use panproto_inst::{CaseBranch, FieldTransform};

    let schema = RefSchema::default()
        .vertex("a", "obj")
        .vertex("f-value", "int")
        .vertex("g-value", "int")
        .vertex("hidden-value", "int")
        .edge("a", "f-value", "prop", Some("f"))
        .edge("a", "g-value", "prop", Some("g"))
        .edge("a", "hidden-value", "prop", Some("hidden"));
    let stage = BoundedLensStage {
        name: "ConditionalDefaultWithResidue".to_owned(),
        source: schema.clone(),
        target: schema.clone(),
        migration: RefMigration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges,
            transformed: ["a".to_owned()].into(),
            output_fields: [("a".to_owned(), vec![BOUNDED_LENS_FIELD.to_owned()])].into(),
            ..RefMigration::default()
        },
        layer: BoundedLensLayer::AddField,
    };
    let mut production = production_stage(&stage);
    production.compiled.field_transforms.insert(
        "a".into(),
        vec![
            FieldTransform::DropField {
                key: "hidden".to_owned(),
            },
            FieldTransform::Case {
                branches: vec![CaseBranch {
                    predicate: Expr::Builtin(
                        BuiltinOp::Eq,
                        vec![Expr::Var("g".into()), Expr::Lit(Literal::Int(0))],
                    ),
                    transforms: vec![FieldTransform::AddField {
                        key: BOUNDED_LENS_FIELD.to_owned(),
                        value: Value::Int(0),
                    }],
                }],
            },
        ],
    );
    assert_eq!(
        reference_output_fields(&production.compiled),
        stage.migration.output_fields
    );
    (stage, production)
}

#[test]
fn declared_transform_conditional_defaults_track_actual_writes() {
    let (stage, production) = conditional_default_stage();
    let mut table_pair = TablePair::from_stage(&stage);
    table_pair.production = duplicate_lens(&production);
    table_pair.layer = &ConditionalDefaultWithResidue;
    let pair = Pair::with_production(
        stage.source,
        stage.target,
        stage.migration,
        &ConditionalDefaultWithResidue,
        production,
    );
    let fixtures = [
        ("inactive branch", 1, Some(7), 1, Some(8), true),
        ("existing default", 0, Some(7), 0, Some(8), true),
        (
            "switch with an existing field",
            1,
            Some(7),
            0,
            Some(8),
            true,
        ),
        ("baseline default disappears", 0, None, 1, Some(0), false),
        ("replay default appears", 1, None, 0, None, false),
    ];
    let mut accepted = [0_usize; 3];
    let mut refused = [0_usize; 2];
    for (name, source_g, source_f, edited_g, edited_f, admitted) in fixtures {
        let (source, edited) = conditional_default_graphs(source_g, source_f, edited_g, edited_f);
        assert_eq!(
            production_get(&pair, &source),
            reference_get(&pair, &source),
            "{name}"
        );
        let graph_outcome = reference_put(&pair, &source, &edited).expect("successful graph get");
        assert_eq!(
            production_put(&pair, &source, &edited).expect("successful production graph get"),
            graph_outcome,
            "{name} graph put and domain"
        );
        let tree = RefTree {
            root: 0,
            nodes: source.nodes.clone(),
            arcs: Vec::new(),
            fans: Vec::new(),
        };
        let edited_tree = RefTree {
            nodes: edited.nodes.clone(),
            ..tree.clone()
        };
        assert_eq!(
            production_tree_case(&pair, &tree, true),
            reference_tree_case(&pair, &tree),
            "{name} tree get"
        );
        let tree_outcome =
            reference_tree_put(&pair, &tree, &edited_tree).expect("successful tree get");
        assert_eq!(
            production_tree_put(&pair, &tree, &edited_tree)
                .expect("successful production tree get"),
            tree_outcome,
            "{name} tree put and domain"
        );
        if admitted {
            assert_conditional_default_graph_tree_round_trip(
                name,
                &pair,
                &edited,
                tree,
                &edited_tree,
                &graph_outcome,
                &tree_outcome,
            );
            accepted[0] += 1;
            accepted[1] += 1;

            assert_conditional_default_table_round_trip(name, &table_pair, &source, &edited);
            accepted[2] += 1;
        } else {
            assert_conditional_default_refusal(name, &graph_outcome, &tree_outcome);
            refused[0] += 1;
            refused[1] += 1;
        }
    }
    assert_eq!(accepted, [3, 3, 3]);
    assert_eq!(refused, [2, 2]);
    println!(
        "conditional default edits: graph=5, tree=5, table=3, accepted=9, refused=4, total=13"
    );
}

fn assert_conditional_default_refusal(
    name: &str,
    graph_outcome: &PutOutcome,
    tree_outcome: &TreePutOutcome,
) {
    let reason = "DerivedCellEdited Node(0) f".to_owned();
    assert_eq!(graph_outcome.domain, Err(vec![reason.clone()]), "{name}");
    assert_eq!(tree_outcome.domain, Err(vec![reason.clone()]), "{name}");
    assert_eq!(
        graph_outcome.put,
        Err(Reason::PutDomain(reason.clone())),
        "{name}"
    );
    assert_eq!(tree_outcome.put, Err(Reason::PutDomain(reason)), "{name}");
}

fn conditional_default_graphs(
    source_g: i64,
    source_f: Option<i64>,
    edited_g: i64,
    edited_f: Option<i64>,
) -> (RefGraph, RefGraph) {
    let mut node = RefNode::new(0, "a");
    node.fields.insert("g".to_owned(), RefValue::Int(source_g));
    node.fields.insert("hidden".to_owned(), RefValue::Int(9));
    if let Some(value) = source_f {
        node.fields
            .insert(BOUNDED_LENS_FIELD.to_owned(), RefValue::Int(value));
    }
    let source = RefGraph {
        nodes: [(0, node)].into(),
        edges: BTreeMap::new(),
    };
    let mut edited = source.clone();
    let fields = &mut edited.nodes.get_mut(&0).expect("one source node").fields;
    fields.remove("hidden");
    fields.insert("g".to_owned(), RefValue::Int(edited_g));
    fields.remove(BOUNDED_LENS_FIELD);
    if let Some(value) = edited_f {
        fields.insert(BOUNDED_LENS_FIELD.to_owned(), RefValue::Int(value));
    }
    (source, edited)
}

fn assert_conditional_default_graph_tree_round_trip(
    name: &str,
    pair: &Pair,
    edited: &RefGraph,
    tree: RefTree,
    edited_tree: &RefTree,
    graph_outcome: &PutOutcome,
    tree_outcome: &TreePutOutcome,
) {
    assert_eq!(graph_outcome.domain, Ok(()), "{name}");
    assert_eq!(tree_outcome.domain, Ok(()), "{name}");
    let mut restored = edited.clone();
    restored
        .nodes
        .get_mut(&0)
        .expect("one restored node")
        .fields
        .insert("hidden".to_owned(), RefValue::Int(9));
    assert_eq!(graph_outcome.put, Ok(restored.clone()), "{name}");
    assert_eq!(
        production_get(pair, &restored).unwrap().0,
        *edited,
        "{name}"
    );
    assert_eq!(reference_get(pair, &restored).unwrap().0, *edited, "{name}");
    let restored_tree = RefTree {
        nodes: restored.nodes,
        ..tree
    };
    assert_eq!(tree_outcome.put, Ok(restored_tree.clone()), "{name}");
    let production_view = panproto_lens::get(&pair.production, &prod_tree(&restored_tree))
        .expect("restored tree re-gets")
        .0;
    assert_eq!(ref_tree(&production_view), *edited_tree, "{name}");
    let reference_view = pair.with_reference(|lens| {
        RefTreeShape::get(lens.expect("compiled reference lens"), &restored_tree, &())
            .expect("restored reference tree re-gets")
            .0
    });
    assert_eq!(reference_view, *edited_tree, "{name}");
}

fn assert_conditional_default_table_round_trip(
    name: &str,
    table_pair: &TablePair,
    source: &RefGraph,
    edited: &RefGraph,
) {
    let table_source = RefTable {
        rows: [(0, ("a".to_owned(), source.nodes[&0].fields.clone()))].into(),
        pairs: std::collections::BTreeSet::new(),
    };
    let table_edit = RefTable {
        rows: [(0, ("a".to_owned(), edited.nodes[&0].fields.clone()))].into(),
        pairs: std::collections::BTreeSet::new(),
    };
    assert_eq!(
        production_table_get(table_pair, &table_source),
        reference_table_get(table_pair, &table_source),
        "{name} table get"
    );
    let table_outcome = reference_table_put(table_pair, &table_source, &table_edit);
    assert_eq!(
        production_table_put(table_pair, &table_source, &table_edit),
        table_outcome,
        "{name} table put and domain"
    );
    assert_eq!(table_outcome.domain, Ok(()), "{name}");
    let mut table_restored = table_edit.clone();
    table_restored
        .rows
        .get_mut(&0)
        .expect("one restored row")
        .1
        .insert("hidden".to_owned(), RefValue::Int(9));
    assert_eq!(table_outcome.put, Ok(table_restored.clone()), "{name}");
    assert_eq!(
        production_table_get(table_pair, &table_restored).unwrap().0,
        table_edit,
        "{name} production table re-get"
    );
    assert_eq!(
        reference_table_get(table_pair, &table_restored).unwrap().0,
        table_edit,
        "{name} reference table re-get"
    );
}

/// Absent cells remain in the public finite-map get domain. Both observations
/// execute before comparison, separately from the total-row put witnesses.
#[test]
fn declared_transform_conditional_absent_cells_agree_at_table_get() {
    let (stage, production) = conditional_default_stage();
    let mut pair = TablePair::from_stage(&stage);
    pair.production = production;
    pair.layer = &ConditionalDefaultWithResidue;
    let sources: Vec<_> = [0, 1]
        .into_iter()
        .map(|source_g| RefTable {
            rows: [(
                0,
                (
                    "a".to_owned(),
                    [
                        ("g".to_owned(), RefValue::Int(source_g)),
                        ("hidden".to_owned(), RefValue::Int(9)),
                    ]
                    .into(),
                ),
            )]
            .into(),
            pairs: std::collections::BTreeSet::new(),
        })
        .collect();
    let production: Vec<_> = sources
        .iter()
        .map(|source| production_table_get(&pair, source))
        .collect();
    let reference: Vec<_> = sources
        .iter()
        .map(|source| reference_table_get(&pair, source))
        .collect();
    assert_eq!(sources.len(), 2);
    println!("conditional absent-cell table gets: sources=2");
    assert_eq!(production, reference, "absent f with g=0 and g=1");
}

/// Fixed independent layer for Drop(hidden); Case(g=0, Default(f,0)).
/// Its baseline and replay inspect actual default insertion, not the set
/// of fields named by either syntactic branch.
struct ConditionalDefaultWithResidue;

impl ConditionalDefaultWithResidue {
    fn inserts_default(fields: &BTreeMap<String, RefValue>) -> bool {
        fields.get("g") == Some(&RefValue::Int(0)) && !fields.contains_key(BOUNDED_LENS_FIELD)
    }
}

impl ValueLayer for ConditionalDefaultWithResidue {
    fn canonical_semantics(&self) -> ValueLayerSemantics {
        ValueLayerSemantics {
            domain: "panproto-reference/conditional-default-with-residue",
            version: 1,
            canonical_payload: b"a:drop(hidden);if(g=0):default(f,0);baseline-pullback-replay"
                .to_vec(),
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
        if node.anchor == "a" {
            fields.remove("hidden");
            if Self::inserts_default(&fields) {
                fields.insert(BOUNDED_LENS_FIELD.to_owned(), RefValue::Int(0));
            }
        }
        Ok((fields, node.value.clone()))
    }

    fn restore(
        &self,
        view: &RefNode,
        original_fields: Option<&BTreeMap<String, RefValue>>,
        _: Option<&Option<RefPresence>>,
    ) -> Result<(BTreeMap<String, RefValue>, Option<RefPresence>), PutFieldRefusal> {
        if view.anchor != "a" {
            return Ok((view.fields.clone(), view.value.clone()));
        }
        if view.fields.contains_key("hidden") {
            return Err(PutFieldRefusal::TransformNotInvertible {
                field: "hidden".to_owned(),
            });
        }
        let baseline = original_fields.unwrap_or(&view.fields);
        let baseline_writes_f = Self::inserts_default(baseline);
        let mut restored = view.fields.clone();
        if let Some(hidden) = baseline.get("hidden") {
            restored.insert("hidden".to_owned(), hidden.clone());
        }
        if baseline_writes_f {
            restored.remove(BOUNDED_LENS_FIELD);
        }
        let replay_writes_f = Self::inserts_default(&restored);
        let replay_f = if replay_writes_f {
            Some(&BOUNDED_LENS_VALUE)
        } else {
            restored.get(BOUNDED_LENS_FIELD)
        };
        if (baseline_writes_f || replay_writes_f) && replay_f != view.fields.get(BOUNDED_LENS_FIELD)
        {
            return Err(PutFieldRefusal::DerivedCellEdited {
                field: BOUNDED_LENS_FIELD.to_owned(),
            });
        }
        Ok((restored, view.value.clone()))
    }
}

#[test]
fn declared_transform_attribute_edges_agree_with_the_reference() {
    for (target, name) in [
        ("value", Some(BOUNDED_LENS_FIELD)),
        (BOUNDED_LENS_FIELD, None),
    ] {
        let schema = RefSchema::default()
            .vertex("a", "obj")
            .vertex(target, "int")
            .edge("a", target, "attribute", name);
        let stage = BoundedLensStage {
            name: "DeclaredAttribute".to_owned(),
            source: schema.clone(),
            target: schema.clone(),
            migration: RefMigration {
                surviving_verts: schema.vertices.keys().cloned().collect(),
                surviving_edges: schema.edges.clone(),
                transformed: ["a".to_owned()].into(),
                output_fields: [("a".to_owned(), vec![BOUNDED_LENS_FIELD.to_owned()])].into(),
                ..RefMigration::default()
            },
            layer: BoundedLensLayer::AddField,
        };
        let pair = Pair::from_stage(&stage);
        let reference_plan =
            panproto_reference::restrict::compile(&stage.source, &stage.target, &stage.migration)
                .expect("any-kind leaf attributes and unnamed fallback attributes declare outputs");
        assert_eq!(pair.plan, Ok(reference_plan));
        let source = RefGraph {
            nodes: [(0, RefNode::new(0, "a"))].into(),
            edges: BTreeMap::new(),
        };
        let expected = reference_get(&pair, &source).expect("declared output compiles");
        assert_eq!(production_get(&pair, &source), Ok(expected.clone()));
        assert_eq!(
            production_put(&pair, &source, &expected.0),
            reference_put(&pair, &source, &expected.0),
            "edge name {name:?}"
        );
    }
}

#[test]
fn declared_transform_table_functor_refusal_precedes_output_refusal() {
    let stage = BoundedLensStage {
        name: "DeclaredOutputCompetingRefusals".to_owned(),
        source: RefSchema::default().vertex("a", "obj"),
        target: RefSchema::default().vertex("value", "int"),
        migration: RefMigration {
            surviving_verts: ["value".to_owned()].into(),
            vertex_remap: [("a".to_owned(), "value".to_owned())].into(),
            transformed: ["a".to_owned()].into(),
            output_fields: [("a".to_owned(), vec![BOUNDED_LENS_FIELD.to_owned()])].into(),
            ..RefMigration::default()
        },
        layer: BoundedLensLayer::AddField,
    };
    let pair = TablePair::from_stage(&stage);
    assert_eq!(
        panproto_reference::restrict::compile(&stage.source, &stage.target, &stage.migration),
        Err(PlanError::UndeclaredTransformField {
            source_vertex: "a".to_owned(),
            target_vertex: "value".to_owned(),
            field: BOUNDED_LENS_FIELD.to_owned(),
        })
    );
    let expected = TablePlanError::Functor(Box::new(
        panproto_reference::rel::FunctorError::VertexImageNotATable {
            vertex: "a".to_owned(),
            image: "value".to_owned(),
        },
    ));
    assert_eq!(
        panproto_reference::table::compile_for_tables(
            &pair.src,
            &pair.tgt,
            &pair.boundaries.0,
            &pair.boundaries.1,
            &pair.migration,
        ),
        Err(expected.clone())
    );
    assert_eq!(pair.plan, Err(expected));
    let error = panproto_inst::RestrictPlan::compile_for_tables(
        &pair.production.src_schema,
        &pair.production.tgt_schema,
        &pair.production.compiled,
        pair.protocols(),
    )
    .expect_err("entity functor refusal precedes the undeclared output");
    assert!(matches!(
        error,
        RestrictError::VertexImageNotATable { vertex, image }
            if vertex == "a" && image == "value"
    ));
    let get_error = panproto_lens::table_lens::get(
        &pair.production,
        &prod_table(&RefTable::default()),
        pair.protocols(),
    )
    .expect_err("table get preserves the entity functor refusal on an empty source");
    assert!(matches!(
        get_error,
        LensError::Restrict(RestrictError::VertexImageNotATable { vertex, image })
            if vertex == "a" && image == "value"
    ));
}

#[test]
fn declared_transform_paths_agree_with_the_reference() {
    use panproto_reference::enumerate::lenses::{
        declared_transform_count_certificate, declared_transform_graphs, declared_transform_stages,
        declared_transform_tables, declared_transform_trees,
    };
    let stages = declared_transform_stages();
    let pairs: Vec<_> = stages.iter().map(Pair::from_stage).collect();
    let tables: Vec<_> = stages.iter().map(TablePair::from_stage).collect();
    let graph_sources = declared_transform_graphs();
    let tree_sources = declared_transform_trees();
    let table_sources = declared_transform_tables();
    let paths = words(stages.len());
    let certificate = declared_transform_count_certificate();
    assert_eq!(paths.len(), certificate.words);
    let mut cases = [0_usize; 3];
    for word in &paths {
        let reference = reference_word(&stages, word).expect("declared stage boundaries agree");
        let production = production_word(&stages, word).expect("declared stage boundaries agree");
        for source in &graph_sources {
            let expected = reference_path_outcome(&pairs, &reference, source, &graph_sources);
            let actual = production_path_outcome(&production, source, &graph_sources);
            assert!(
                path_outcomes_agree(&actual, &expected),
                "graph path {word:?}, source {source:?}: actual {actual:?}, expected {expected:?}"
            );
            cases[0] += 1;
        }
        for source in &tree_sources {
            let expected = reference_tree_path_outcome(&pairs, word, source, &tree_sources);
            let actual = production_tree_path_outcome(&production, source, &tree_sources);
            assert!(
                shape_path_outcomes_agree(&actual, &expected),
                "tree path {word:?}, source {source:?}: actual {actual:?}, expected {expected:?}"
            );
            cases[1] += 1;
        }
        for source in &table_sources {
            let expected = reference_table_path_outcome(&tables, word, source, &table_sources);
            let actual = production_table_path_outcome(
                &production,
                &tables[0].protocol,
                source,
                &table_sources,
            );
            assert!(
                shape_path_outcomes_agree(&actual, &expected),
                "table path {word:?}, source {source:?}: actual {actual:?}, expected {expected:?}"
            );
            cases[2] += 1;
        }
    }
    assert_eq!(cases, [2_015, 465, 2_015]);
    assert_eq!(cases.iter().sum::<usize>(), certificate.path_cases);
    println!(
        "declared transform paths: words={}, graph={}, tree={}, table={}, total={}",
        paths.len(),
        cases[0],
        cases[1],
        cases[2],
        cases.iter().sum::<usize>()
    );
}

/// Successful atomic plans retain both guarded sequential pairs and the
/// admitted identity/read control, compared with independent atomic folds.
#[test]
fn declared_transform_fusion_boundaries_preserve_atomic_behavior() {
    use panproto_reference::enumerate::lenses::{
        declared_transform_graphs, declared_transform_stages, declared_transform_trees,
    };

    let stages = declared_transform_stages();
    let pairs: Vec<_> = stages.iter().map(Pair::from_stage).collect();
    let graph_sources = declared_transform_graphs();
    let tree_sources = declared_transform_trees();
    let id = bounded_lens_stage_index(&stages, "DeclaredId");
    let default = bounded_lens_stage_index(&stages, "DeclaredDefault");
    let drop = bounded_lens_stage_index(&stages, "DeclaredDrop");
    let read = bounded_lens_stage_index(&stages, "DeclaredRead");
    let witnesses = [
        ("dropped field before read", [drop, read], 2),
        ("value layers on both stages", [default, default], 2),
        ("structural identity before read", [id, read], 1),
    ];
    let mut cases = [0_usize; 2];
    for (name, word, groups) in witnesses {
        let first = &stages[word[0]];
        let second = &stages[word[1]];
        assert_eq!(
            first.target, second.source,
            "{name} has compatible boundaries"
        );
        for index in word {
            let stage = &stages[index];
            assert!(
                panproto_reference::restrict::compile(
                    &stage.source,
                    &stage.target,
                    &stage.migration,
                )
                .is_ok(),
                "{name}: {} reference stage compiles",
                stage.name
            );
            let lens = &pairs[index].production;
            assert!(
                panproto_inst::RestrictPlan::compile(
                    &lens.src_schema,
                    &lens.tgt_schema,
                    &lens.compiled,
                )
                .is_ok(),
                "{name}: {} production stage compiles",
                stage.name
            );
        }
        let production = production_word(&stages, &word).expect("declared boundaries agree");
        let reference = reference_word(&stages, &word).expect("declared boundaries agree");
        // These observations concern the public normal form. Drop also
        // violates the first-stage Unit/value-layer premise, so its missing
        // read does not isolate a private unbound-field composition guard.
        assert_eq!(production.tree_normal_form().len(), groups, "{name}");
        for source in &graph_sources {
            let expected = reference_path_outcome(&pairs, &reference, source, &graph_sources);
            let actual = production_path_outcome(&production, source, &graph_sources);
            assert!(
                path_outcomes_agree(&actual, &expected),
                "{name} graph source {source:?}: actual {actual:?}, expected {expected:?}"
            );
            cases[0] += 1;
        }
        for source in &tree_sources {
            let expected = reference_tree_path_outcome(&pairs, &word, source, &tree_sources);
            let actual = production_tree_path_outcome(&production, source, &tree_sources);
            assert!(
                shape_path_outcomes_agree(&actual, &expected),
                "{name} tree source {source:?}: actual {actual:?}, expected {expected:?}"
            );
            cases[1] += 1;
        }
    }
    assert_eq!(cases, [39, 9]);
    println!("declared transform fusion boundaries: graph=39, tree=9, total=48");
}

/// Decode possible writes from public syntax into the reference migration.
/// Declaration checking belongs to the independent reference compiler.
fn reference_output_fields(compiled: &CompiledMigration) -> BTreeMap<String, Vec<String>> {
    fn fields(transform: &panproto_inst::FieldTransform, writes: &mut Vec<String>) {
        use panproto_inst::FieldTransform as F;
        match transform {
            F::RenameField { new_key, .. } => writes.push(new_key.clone()),
            F::AddField { key, .. } => writes.push(key.clone()),
            F::ApplyExpr { key, .. } => {
                if key != "__value__" {
                    writes.push(key.clone());
                }
            }
            F::ComputeField { target_key, .. } => writes.push(target_key.clone()),
            F::MapReferences { field, .. } => writes.push(field.clone()),
            F::PathTransform { path, inner } => match path.first() {
                Some(field) => writes.push(field.clone()),
                None => fields(inner, writes),
            },
            F::Case { branches } => {
                for branch in branches {
                    for transform in &branch.transforms {
                        fields(transform, writes);
                    }
                }
            }
            F::DropField { .. } | F::KeepFields { .. } => {}
        }
    }
    fn terms(assignment: &panproto_inst::TermAssignment, writes: &mut Vec<String>) {
        use panproto_inst::{TermAssignment as T, TermScope};
        match assignment {
            T::Compute { target, scope, .. } => {
                if !matches!(scope, TermScope::Field) || target != "__value__" {
                    writes.push(target.clone());
                }
            }
            T::Rename { new, .. } => writes.push(new.clone()),
            T::Default { key, .. } => writes.push(key.clone()),
            T::MapReferences { field, .. } => writes.push(field.clone()),
            T::AtPath { path, inner } => match path.first() {
                Some(field) => writes.push(field.clone()),
                None => terms(inner, writes),
            },
            T::Case { branches } => {
                for branch in branches {
                    for assignment in &branch.assignments {
                        terms(assignment, writes);
                    }
                }
            }
            T::Drop { .. } | T::Keep { .. } => {}
        }
    }
    let mut out = BTreeMap::<String, Vec<String>>::new();
    for (anchor, transforms) in &compiled.field_transforms {
        for transform in transforms {
            fields(transform, out.entry(anchor.to_string()).or_default());
        }
    }
    for (anchor, assignments) in &compiled.op_term_assignments {
        for assignment in assignments {
            terms(assignment, out.entry(anchor.to_string()).or_default());
        }
    }
    out.retain(|_, writes| !writes.is_empty());
    out
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

fn assert_false_precondition_protolens_refusal() {
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
            source: protolens_add_enrichment("l19-source-must-not-run", false_precondition),
            target: protolens_add_enrichment(
                "l19-target-must-not-run",
                TheoryConstraint::Unconstrained,
            ),
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
        assert!(matches!(
            panproto_lens::graph_protolens::apply(
                &inapplicable,
                &empty_schema,
                &Protocol::default(),
                &GInstance::new(),
            ),
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
                precondition: reference_precondition.clone(),
                found_schema: panproto_reference::schema::canonical_bytes(&reference_empty),
            })
        );
        assert_eq!(
            panproto_reference::lens::protolens_instance(
                &reference_inapplicable,
                &reference_empty,
                &RefGraph::default(),
            ),
            Err(RefProtolensError::FalsePrecondition {
                precondition: reference_precondition,
                found_schema: panproto_reference::schema::canonical_bytes(&reference_empty),
            })
        );
    });
}

fn assert_overlong_protolens_expansion_refusal() {
    static VERIFIED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    VERIFIED.get_or_init(|| {
        use std::sync::atomic::Ordering;

        use panproto_gat::{TheoryConstraint, TheoryEndofunctor, TheoryTransform};
        use panproto_lens::{ComplementConstructor, Protolens};

    let source = protolens_expansion_source();
    let target = protolens_overlong_expansion_target();
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
        target: protolens_add_enrichment("l19-overlong-expansion", TheoryConstraint::Unconstrained),
        complement_constructor: ComplementConstructor::Empty,
    };
    let production_error = panproto_lens::graph_protolens::apply(
        &overlong,
        &source,
        &Protocol::default(),
        &GInstance::new(),
    )
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
    let reference_error = panproto_reference::lens::protolens_instance(
        &reference_overlong,
        &reference_source,
        &RefGraph::default(),
    )
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
            "L-lenses-19 refusals: candidates=2, false-precondition clauses=2, transform calls before false-precondition refusal=0, plan refusals=1, expansion intermediates=65536, expansion hops=65537"
        );
    });
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
    output_fields: BTreeMap<String, Vec<String>>,
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
        output_fields: migration.output_fields.clone(),
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
    let schema = panproto_reference::enumerate::lenses::bounded_lens_schema(&["a"], &[("a", "a")]);
    let plain = reference_carrier_migration(&schema);
    let mut redundant = plain.clone();
    redundant.resolver.insert(
        ("a".to_owned(), "a".to_owned()),
        panproto_reference::enumerate::lenses::bounded_lens_edge("a", "a"),
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
            ProtolensSchemaCase::FalsePrecondition => {
                false_preconditions += 1;
                assert_false_precondition_protolens_refusal();
            }
            ProtolensSchemaCase::ExpansionTooLong => {
                expansion_refusals += 1;
                assert_overlong_protolens_expansion_refusal();
            }
        }
    });
    assert_eq!(certificate_cases, protolens_schema_case_count_certificate());
    assert_eq!(elementary_cases, 64);
    assert_eq!(false_preconditions, 1);
    assert_eq!(expansion_refusals, 1);
    certificate_cases
}

#[test]
fn protolens_schema_law_certificate() {
    let alphabet = protolens_alphabet();
    let schemas = sigma2_schemas();
    let cases = assert_protolens_schema_scope(&alphabet, &schemas, &Protocol::default());
    assert_eq!(cases, 66);
}

fn protolens_instance_bound(
    protolenses: usize,
    schemas: usize,
    elementary_cases: usize,
    refusal_witnesses: usize,
) -> String {
    let bound = format!(
        "bounded lens protolens instances: protolenses={protolenses}, schemas={schemas}, elementary cases={elementary_cases}, named application refusals={refusal_witnesses}, total={}",
        elementary_cases + refusal_witnesses,
    );
    println!("{bound}");
    bound
}

fn assert_malformed_fan_protolens_application_refusal() {
    use panproto_gat::{TheoryConstraint, TheoryEndofunctor, TheoryTransform};
    use panproto_lens::{ComplementConstructor, Protolens};

    let schema = bounded_lens_fan_schema();
    let mut hyper = RefNode::new(0, "h");
    hyper.shape = RefNodeShape::HyperEdge;
    let labels = panproto_reference::enumerate::lenses::bounded_lens_fan_labels();
    let source = RefGraph {
        nodes: BTreeMap::from([(0, hyper), (2, RefNode::new(2, "b"))]),
        edges: BTreeMap::from([(
            0,
            RefGEdge {
                src: 0,
                tgt: 2,
                label: labels[1].clone(),
                position: None,
            },
        )]),
    };
    let identity_endofunctor = || TheoryEndofunctor {
        name: "l20-malformed-fan-identity".into(),
        precondition: TheoryConstraint::Unconstrained,
        transform: TheoryTransform::Identity,
    };
    let production = Protolens {
        name: "l20-malformed-fan".into(),
        source: identity_endofunctor(),
        target: identity_endofunctor(),
        complement_constructor: ComplementConstructor::Empty,
    };
    let production_error = panproto_lens::graph_protolens::apply(
        &production,
        &prod_schema(&schema),
        &Protocol::default(),
        &prod_graph(&source),
    )
    .expect_err("a malformed fan must refuse after protolens instantiation");
    let production_reason = lens_reason(&production_error);

    let migration = reference_carrier_migration(&schema);
    let reference = RefProtolens::exact(&schema, &schema, &migration);
    let reference_error =
        panproto_reference::lens::protolens_instance(&reference, &schema, &source)
            .expect_err("the reference application must refuse the same malformed fan");
    let reference_reason = match reference_error {
        RefProtolensError::Restrict(error) => Reason::Restrict(ref_restrict_reason(&error)),
        other => panic!("malformed fan returned a non-restrict refusal: {other:?}"),
    };
    assert_eq!(production_reason, reference_reason);
    assert!(matches!(
        production_reason,
        Reason::Restrict(ref reason) if reason.starts_with("MalformedFan 0 ")
    ));
}

type ProtolensInstanceUnit = (usize, usize, usize);
type ProtolensInstanceContext = (Protocol, Vec<panproto_lens::Protolens>);
type ProtolensComparisonCase<'a> = (usize, &'a RefSchema, &'a RefGraph);

fn collect_protolens_instance_units() -> (Vec<ProtolensInstanceUnit>, usize) {
    let mut units = Vec::new();
    let mut refusal_witnesses = 0_usize;
    for_each_protolens_instance_case(|case| match case {
        ProtolensInstanceCase::Elementary {
            protolens,
            schema,
            state,
        } => units.push((protolens, schema, state)),
        ProtolensInstanceCase::FalsePrecondition => {
            refusal_witnesses += 1;
            assert_false_precondition_protolens_refusal();
        }
        ProtolensInstanceCase::ExpansionTooLong => {
            refusal_witnesses += 1;
            assert_overlong_protolens_expansion_refusal();
        }
        ProtolensInstanceCase::MalformedFan => {
            refusal_witnesses += 1;
            assert_malformed_fan_protolens_application_refusal();
        }
    });
    (units, refusal_witnesses)
}

fn production_protolens_instance_outcome(
    context: &ProtolensInstanceContext,
    case: &ProtolensComparisonCase<'_>,
) -> ProtolensOutcome {
    let (protocol, alphabet) = context;
    let &(index, schema, source) = case;
    let production_schema = prod_schema(schema);
    let applicable = assert_protolens_schema_agrees(index, &alphabet[index], schema, protocol);
    match panproto_lens::graph_protolens::apply(
        &alphabet[index],
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
}

fn reference_protolens_instance_outcome(
    _context: &ProtolensInstanceContext,
    case: &ProtolensComparisonCase<'_>,
) -> ProtolensOutcome {
    let &(index, schema, source) = case;
    let reference = reference_protolens(index, schema);
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
}

fn assert_protolens_instance_units_agree(
    bound: &str,
    units: &[ProtolensInstanceUnit],
    schemas: &[RefSchema],
    states: &[Vec<RefGraph>],
) -> u64 {
    assert_agree(agree_in_parallel(
        bound,
        units,
        || (Protocol::default(), protolens_alphabet()),
        |_, unit| {
            let &(index, schema, state) = unit;
            vec![(index, &schemas[schema], &states[schema][state])]
        },
        production_protolens_instance_outcome,
        reference_protolens_instance_outcome,
        |production, reference| production == reference,
    ))
}

#[test]
fn protolens_transforms_agree_with_the_reference_exhaustively() {
    let alphabet = protolens_alphabet();
    let schemas = sigma2_schemas();
    let states: Vec<Vec<RefGraph>> = schemas.iter().map(bounded_lens_graph_states).collect();
    let (units, refusal_witnesses) = collect_protolens_instance_units();
    let certificate = protolens_instance_count_certificate();
    assert_eq!(certificate.protolenses, alphabet.len());
    assert_eq!(certificate.schemas, schemas.len());
    assert_eq!(
        units.len(),
        alphabet.len()
            * schemas
                .iter()
                .map(bounded_lens_graph_state_count)
                .sum::<usize>()
    );
    assert_eq!(units.len(), certificate.elementary_cases);
    assert_eq!(refusal_witnesses, certificate.refusal_witnesses);
    assert_eq!(units.len() + refusal_witnesses, certificate.total);
    assert_eq!(certificate.total, 464_835);
    let bound = protolens_instance_bound(
        alphabet.len(),
        schemas.len(),
        units.len(),
        refusal_witnesses,
    );
    let compared = assert_protolens_instance_units_agree(&bound, &units, &schemas, &states);
    assert_eq!(compared, u64::try_from(units.len()).unwrap());
}

fn generated_protolens_schema(vertex_mask: u8, kind_mask: u8, edge_mask: u32) -> RefSchema {
    const VERTICES: [&str; 3] = ["a", "b", "c"];
    let mut schema = RefSchema::default();
    for (index, vertex) in VERTICES.into_iter().enumerate() {
        if vertex_mask & (1 << index) != 0 {
            let kind = if kind_mask & (1 << index) != 0 {
                "obj"
            } else {
                "other"
            };
            schema = schema.vertex(vertex, kind);
        }
    }
    let present = schema.vertices.keys().cloned().collect::<Vec<_>>();
    let mut ordinal = 0_u32;
    for src in &present {
        for tgt in &present {
            for name in ["e", "f"] {
                if edge_mask & (1 << ordinal) != 0 {
                    schema
                        .edges
                        .insert(RefSEdge::new(src, tgt, "rel", Some(name)));
                }
                ordinal += 1;
            }
        }
    }
    schema
}

#[test]
fn parallel_edge_resolver_does_not_relabel_a_dropped_protolens_edge() {
    let dropped = RefSEdge::new("a", "a", "rel", Some("e"));
    let retained = RefSEdge::new("a", "a", "rel", Some("f"));
    let source = RefSchema::default()
        .vertex("a", "obj")
        .edge("a", "a", "rel", Some("e"))
        .edge("a", "a", "rel", Some("f"));
    let production_source = prod_schema(&source);
    let lens = elementary::drop_edge("a", "a", Some("e".into()))
        .instantiate_checked(&production_source, &Protocol::default())
        .expect("the drop-edge protolens applies to the parallel-edge schema");

    let production_plan =
        panproto_inst::RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
            .expect("production compiles the instantiated drop-edge migration");
    let reference_target = ref_schema(&lens.tgt_schema);
    let reference_migration = ref_migration(&lens.compiled);
    let endpoint = ("a".to_owned(), "a".to_owned());
    assert_eq!(
        reference_migration.resolver.get(&endpoint),
        None,
        "the resolver resolves no endpoint pair that a source edge joins",
    );
    let reference_plan =
        panproto_reference::restrict::compile(&source, &reference_target, &reference_migration)
            .expect("the reference compiles the production migration");

    let production_dropped = prod_edge(&dropped);
    let production_retained = prod_edge(&retained);
    let production_keep = panproto_inst::EdgeFate::Keep(production_retained.clone());
    assert_eq!(
        production_plan.edge_image.fate(&production_dropped),
        Some(&panproto_inst::EdgeFate::Drop),
        "an endpoint resolver cannot identify one of two parallel source edges",
    );
    assert_eq!(
        production_plan.edge_image.fate(&production_retained),
        Some(&production_keep),
        "the retained parallel edge keeps its complete identity",
    );

    let reference_keep = panproto_reference::restrict::EdgeFate::Keep(retained.clone());
    assert_eq!(
        reference_plan.fates().get(&dropped),
        Some(&panproto_reference::restrict::EdgeFate::Drop),
        "the reference must not reinterpret the dropped edge as the resolver edge",
    );
    assert_eq!(
        reference_plan.fates().get(&retained),
        Some(&reference_keep),
        "the reference keeps the retained parallel edge",
    );
}

#[test]
fn drop_sort_does_not_invent_a_parallel_contraction_resolver() {
    let source = RefSchema::default()
        .vertex("a", "obj")
        .vertex("b", "other")
        .edge("b", "a", "rel", Some("ba"))
        .edge("a", "b", "rel", Some("ab"))
        .edge("b", "b", "rel", Some("e"))
        .edge("b", "b", "rel", Some("f"));
    let production_source = prod_schema(&source);
    let protolens = elementary::drop_sort("obj");
    assert_protolens_schema_agrees(0, &protolens, &source, &Protocol::default());

    let lens = protolens
        .instantiate_checked(&production_source, &Protocol::default())
        .expect("drop-sort applies to the obj vertex");
    let endpoint = ("b".into(), "b".into());
    assert!(
        !lens.compiled.resolver.contains_key(&endpoint),
        "an inferred endpoint resolver must not choose between parallel target edges",
    );
    let plan =
        panproto_inst::RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
            .expect("the ambiguous pair remains a valid, partial restrict plan");
    let pair = plan
        .contraction
        .pair(&endpoint.0, &endpoint.1)
        .expect("the path through the dropped vertex creates a b-to-b contraction pair");
    assert_eq!(
        pair.resolution,
        panproto_inst::PairResolution::Unresolved(panproto_inst::UnresolvedReason::Ambiguous {
            count: 2
        },),
    );
}

proptest! {
    #![proptest_config(ProptestConfig::default())]
    #[test]
    fn randomized_protolens_schema_differential(
        vertex_mask in 0_u8..8,
        kind_mask in 0_u8..8,
        edge_mask in any::<u32>(),
    ) {
        let schema = generated_protolens_schema(vertex_mask, kind_mask, edge_mask);
        let alphabet = protolens_alphabet();
        let protocol = Protocol::default();
        for (index, protolens) in alphabet.iter().enumerate() {
            assert_protolens_schema_agrees(index, protolens, &schema, &protocol);
        }
        assert_false_precondition_protolens_refusal();
        assert_overlong_protolens_expansion_refusal();
    }
}

/// The law outcome recorded on both sides for a tree outside the reference
/// tree lens's domain, where the tree law report is not compared.
fn outside_the_tree_domain() -> LawOutcome {
    LawOutcome {
        first_failure: Some("OutsideTreeDomain".to_owned()),
    }
}

/// The per-thread context of the L-lenses-8 differential: the bounded lens lens pairs, the
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
    mutation: usize,
    view: I,
    positive_reference: bool,
}

fn push_candidate<I: PartialEq>(
    candidates: &mut Vec<Candidate<I>>,
    mutation: usize,
    view: I,
    positive_reference: bool,
) {
    if let Some(candidate) = candidates
        .iter_mut()
        .find(|candidate| candidate.mutation == mutation && candidate.view == view)
    {
        candidate.positive_reference |= positive_reference;
    } else {
        candidates.push(Candidate {
            mutation,
            view,
            positive_reference,
        });
    }
}

/// Partition one exact-presentation union into mutation families in one pass.
/// The per-family order is the order in which the union first presented each
/// candidate, matching the earlier family-filter construction exactly.
fn partition_common_views<I>(
    candidate_count: usize,
    candidates: impl IntoIterator<Item = (usize, I)>,
) -> [Vec<I>; EDIT_FAMILIES] {
    let mut families: [Vec<I>; EDIT_FAMILIES] = std::array::from_fn(|_| Vec::new());
    let mut observed = 0_usize;
    for (family, view) in candidates {
        assert!(
            family < EDIT_FAMILIES,
            "common-basis mutation family {family} is outside the {EDIT_FAMILIES}-family contract"
        );
        families[family].push(view);
        observed += 1;
    }
    assert_eq!(
        observed, candidate_count,
        "single-pass family partition must preserve every common-basis candidate"
    );
    assert_eq!(
        families.iter().map(Vec::len).sum::<usize>(),
        candidate_count,
        "single-pass family counts must sum to the exact common-basis size"
    );
    families
}

#[test]
fn single_pass_common_family_partition_is_exact() {
    let families = partition_common_views(
        6,
        [(2, 'a'), (0, 'b'), (2, 'c'), (16, 'd'), (0, 'e'), (7, 'f')],
    );
    assert_eq!(families[0], ['b', 'e']);
    assert_eq!(families[2], ['a', 'c']);
    assert_eq!(families[7], ['f']);
    assert_eq!(families[16], ['d']);
    assert_eq!(families.iter().map(Vec::len).sum::<usize>(), 6);
}

/// A production-law context whose root source uses the independently formed
/// union basis. Recursive classification steps retain the production shape's
/// own basis; the compared payload retains the five gates at the prepared
/// root.
struct CommonProductionContext<S: LawShape> {
    inner: S::Context,
    root: S::Instance,
    families: [Vec<S::Instance>; EDIT_FAMILIES],
}

struct CommonProductionShape<S>(std::marker::PhantomData<S>);

impl<S: LawShape> panproto_lens::LensShape for CommonProductionShape<S> {
    type Instance = S::Instance;
    type Complement = S::Complement;
    type Context = CommonProductionContext<S>;

    fn get(
        lens: &Lens,
        source: &Self::Instance,
        context: &Self::Context,
    ) -> Result<(Self::Instance, Self::Complement), LensError> {
        S::get(lens, source, &context.inner)
    }

    fn put(
        lens: &Lens,
        view: &Self::Instance,
        complement: &Self::Complement,
        context: &Self::Context,
    ) -> Result<Self::Instance, LensError> {
        S::put(lens, view, complement, &context.inner)
    }

    fn check_put_domain(
        lens: &Lens,
        view: &Self::Instance,
        complement: &Self::Complement,
        context: &Self::Context,
    ) -> Result<(), Vec<PutDomainError>> {
        S::check_put_domain(lens, view, complement, &context.inner)
    }

    fn synth_index(
        instance: &Self::Instance,
        complement: &Self::Complement,
    ) -> panproto_inst::SynthIndex {
        S::synth_index(instance, complement)
    }

    fn equivalent(
        left: &Self::Instance,
        left_index: &panproto_inst::SynthIndex,
        right: &Self::Instance,
        right_index: &panproto_inst::SynthIndex,
    ) -> bool {
        S::equivalent(left, left_index, right, right_index)
    }

    fn divergence(
        left: &Self::Instance,
        left_index: &panproto_inst::SynthIndex,
        right: &Self::Instance,
        right_index: &panproto_inst::SynthIndex,
    ) -> Option<String> {
        S::divergence(left, left_index, right, right_index)
    }

    fn divergence_modulo_derived(
        left: &Self::Instance,
        left_index: &panproto_inst::SynthIndex,
        right: &Self::Instance,
        right_index: &panproto_inst::SynthIndex,
        derived: &panproto_lens::DerivedMap,
    ) -> Option<String> {
        S::divergence_modulo_derived(left, left_index, right, right_index, derived)
    }

    fn complement_divergence(left: &Self::Complement, right: &Self::Complement) -> Option<String> {
        S::complement_divergence(left, right)
    }

    fn congruence_witnesses(
        instance: &Self::Instance,
        index: &panproto_inst::SynthIndex,
    ) -> Vec<(Self::Instance, panproto_inst::SynthIndex)> {
        S::congruence_witnesses(instance, index)
    }
}

impl<S: LawShape> LawShape for CommonProductionShape<S> {
    fn mutation_cases(
        lens: &Lens,
        source: &Self::Instance,
        view: &Self::Instance,
        complement: &Self::Complement,
        context: &Self::Context,
        mutation: Mutation,
    ) -> MutationCases<Self::Instance> {
        if S::identical(source, &context.root) {
            let family = Mutation::ALL
                .iter()
                .position(|candidate| *candidate == mutation)
                .expect("Mutation::ALL contains the requested family");
            MutationCases::Cases(context.families[family].clone())
        } else {
            S::mutation_cases(lens, source, view, complement, &context.inner, mutation)
        }
    }

    fn negative_mutation_cases(
        _: &Lens,
        _: &Self::Instance,
        _: &Self::Instance,
        _: &Self::Complement,
        _: &Self::Context,
        _: Mutation,
    ) -> Vec<panproto_lens::NegativeMutationCase<Self::Instance>> {
        Vec::new()
    }

    fn identical(left: &Self::Instance, right: &Self::Instance) -> bool {
        S::identical(left, right)
    }

    fn congruent_states(
        view: &Self::Instance,
        complement: &Self::Complement,
    ) -> Vec<(Self::Instance, Self::Complement)> {
        S::congruent_states(view, complement)
    }

    fn rekey_complement(
        complement: &Self::Complement,
        from: &panproto_inst::SynthIndex,
        to: &panproto_inst::SynthIndex,
    ) -> Self::Complement {
        S::rekey_complement(complement, from, to)
    }

    fn locality_divergence(
        lens: &Lens,
        source: &Self::Instance,
        original_view: &Self::Instance,
        edited_view: &Self::Instance,
        restored: &Self::Instance,
        complement: &Self::Complement,
        context: &Self::Context,
    ) -> Option<String> {
        S::locality_divergence(
            lens,
            source,
            original_view,
            edited_view,
            restored,
            complement,
            &context.inner,
        )
    }

    fn carrier_intersection(left: &Self::Instance, right: &Self::Instance) -> Self::Instance {
        S::carrier_intersection(left, right)
    }

    fn undo_view(original: &Self::Instance, edited: &Self::Instance) -> Option<Self::Instance> {
        S::undo_view(original, edited)
    }

    fn enumerate_small(
        schema: &Schema,
        scope: SmallScope,
        context: &Self::Context,
    ) -> Vec<Self::Instance> {
        S::enumerate_small(schema, scope, &context.inner)
    }
}

struct CommonReferenceContext<S, L>
where
    S: RefLawShape<L>,
{
    inner: S::Context,
    root: S::Instance,
    basis: Vec<panproto_reference::lens::BasisCase<S::Instance, S::Refusal>>,
    marker: std::marker::PhantomData<L>,
}

struct CommonReferenceShape<S>(std::marker::PhantomData<S>);

impl<S: panproto_reference::lens::LensShape> panproto_reference::lens::LensShape
    for CommonReferenceShape<S>
{
    type Instance = S::Instance;
    type Complement = S::Complement;

    fn identical(left: &Self::Instance, right: &Self::Instance) -> bool {
        S::identical(left, right)
    }

    fn complements_identical(left: &Self::Complement, right: &Self::Complement) -> bool {
        S::complements_identical(left, right)
    }

    fn synth_index(
        instance: &Self::Instance,
        complement: &Self::Complement,
    ) -> panproto_reference::lens::SynthIndex {
        S::synth_index(instance, complement)
    }

    fn equivalent(
        left: &Self::Instance,
        left_index: &panproto_reference::lens::SynthIndex,
        right: &Self::Instance,
        right_index: &panproto_reference::lens::SynthIndex,
    ) -> bool {
        S::equivalent(left, left_index, right, right_index)
    }

    fn divergence(
        left: &Self::Instance,
        left_index: &panproto_reference::lens::SynthIndex,
        right: &Self::Instance,
        right_index: &panproto_reference::lens::SynthIndex,
    ) -> Option<panproto_reference::lens::Divergence> {
        S::divergence(left, left_index, right, right_index)
    }

    fn complement_divergence(
        left: &Self::Complement,
        right: &Self::Complement,
    ) -> Option<panproto_reference::lens::Divergence> {
        S::complement_divergence(left, right)
    }
}

impl<S, L> RefLawShape<L> for CommonReferenceShape<S>
where
    S: RefLawShape<L>,
{
    type Refusal = S::Refusal;
    type Context = CommonReferenceContext<S, L>;

    fn get(
        lens: &L,
        source: &Self::Instance,
        context: &Self::Context,
    ) -> Result<(Self::Instance, Self::Complement), Self::Refusal> {
        S::get(lens, source, &context.inner)
    }

    fn put(
        lens: &L,
        view: &Self::Instance,
        complement: &Self::Complement,
        context: &Self::Context,
    ) -> Result<Self::Instance, Self::Refusal> {
        S::put(lens, view, complement, &context.inner)
    }

    fn check_put_domain(
        lens: &L,
        view: &Self::Instance,
        complement: &Self::Complement,
        context: &Self::Context,
    ) -> Result<(), Self::Refusal> {
        S::check_put_domain(lens, view, complement, &context.inner)
    }

    fn first_put_refusal(refusal: &Self::Refusal) -> Option<Self::Refusal> {
        S::first_put_refusal(refusal)
    }

    fn single_step_basis(
        lens: &L,
        source: &Self::Instance,
        view: &Self::Instance,
        complement: &Self::Complement,
        context: &Self::Context,
    ) -> Vec<panproto_reference::lens::BasisCase<Self::Instance, Self::Refusal>> {
        if S::identical(source, &context.root) {
            context.basis.clone()
        } else {
            S::single_step_basis(lens, source, view, complement, &context.inner)
        }
    }

    fn put_get_mode() -> panproto_reference::lens::PutGetMode {
        S::put_get_mode()
    }

    fn rekey_complement(
        complement: &Self::Complement,
        observed_view: &Self::Instance,
        edited_view: &Self::Instance,
        producing_complement: &Self::Complement,
    ) -> Self::Complement {
        S::rekey_complement(complement, observed_view, edited_view, producing_complement)
    }

    fn congruence_cases(
        lens: &L,
        view: &Self::Instance,
        complement: &Self::Complement,
        context: &Self::Context,
    ) -> Vec<panproto_reference::lens::CongruenceCase<Self::Instance, Self::Complement>> {
        S::congruence_cases(lens, view, complement, &context.inner)
    }

    fn locality_divergence(
        lens: &L,
        source: &Self::Instance,
        original_view: &Self::Instance,
        edited_view: &Self::Instance,
        restored: &Self::Instance,
        complement: &Self::Complement,
        context: &Self::Context,
    ) -> Option<panproto_reference::lens::Divergence> {
        S::locality_divergence(
            lens,
            source,
            original_view,
            edited_view,
            restored,
            complement,
            &context.inner,
        )
    }
}

#[derive(Debug, PartialEq, Eq)]
struct CommonBasisLawOutcome {
    gates: LawOutcome,
    family_counts: [usize; EDIT_FAMILIES],
    admitted: Option<usize>,
}

fn production_common_law_outcome<S: LawShape>(
    lens: &Lens,
    root: &S::Instance,
    inner: S::Context,
    families: [Vec<S::Instance>; EDIT_FAMILIES],
) -> CommonBasisLawOutcome {
    let family_counts = std::array::from_fn(|family| families[family].len());
    let context = CommonProductionContext::<S> {
        inner,
        root: root.clone(),
        families,
    };
    let report =
        panproto_lens::laws::check_laws_report::<CommonProductionShape<S>>(lens, root, &context);
    let admitted = report.as_ref().ok().map(|report| {
        report
            .ran
            .iter()
            .map(|family| usize::try_from(family.in_domain).expect("family count fits usize"))
            .sum()
    });
    CommonBasisLawOutcome {
        gates: production_law_outcome(&report),
        family_counts,
        admitted,
    }
}

fn reference_common_law_report<S, L>(
    lens: &L,
    root: &S::Instance,
    inner: S::Context,
    families: [Vec<S::Instance>; EDIT_FAMILIES],
) -> (RefLawReport<S::Refusal>, [usize; EDIT_FAMILIES])
where
    S: RefLawShape<L>,
{
    let family_counts = std::array::from_fn(|family| families[family].len());
    let basis = families
        .into_iter()
        .enumerate()
        .flat_map(|(family, views)| {
            views
                .into_iter()
                .map(move |view| (REF_MUTATIONS[family], view))
        })
        .enumerate()
        .map(
            |(ordinal, (mutation, view))| panproto_reference::lens::BasisCase {
                ordinal,
                mutation,
                view,
                domain: DomainExpectation::Admitted,
            },
        )
        .collect();
    let context = CommonReferenceContext::<S, L> {
        inner,
        root: root.clone(),
        basis,
        marker: std::marker::PhantomData,
    };
    let report = panproto_reference::lens::check_laws_report::<CommonReferenceShape<S>, L>(
        lens, root, &context,
    );
    let observed = std::array::from_fn(|family| {
        report
            .basis_families
            .get(&REF_MUTATIONS[family])
            .copied()
            .unwrap_or_default()
    });
    assert_eq!(
        observed, family_counts,
        "the reference five-gate checker must consume the common basis family by family"
    );
    (report, family_counts)
}

fn reference_common_law_outcome<S, L>(
    lens: &L,
    root: &S::Instance,
    inner: S::Context,
    families: [Vec<S::Instance>; EDIT_FAMILIES],
) -> CommonBasisLawOutcome
where
    S: RefLawShape<L>,
    S::Refusal: std::fmt::Debug,
{
    let (report, family_counts) = reference_common_law_report::<S, L>(lens, root, inner, families);
    let gates = reference_law_outcome(&report);
    let admitted = gates.first_failure.is_none().then_some(report.admitted);
    CommonBasisLawOutcome {
        gates,
        family_counts,
        admitted,
    }
}

fn graph_common_laws(
    pair: &Pair,
    source: &RefGraph,
    candidates: &[Candidate<RefGraph>],
    production: bool,
) -> Option<CommonBasisLawOutcome> {
    if pair.with_reference(|lens| lens.is_err()) {
        return None;
    }
    let reference_families = partition_common_views(
        candidates.len(),
        candidates
            .iter()
            .map(|candidate| (candidate.mutation, candidate.view.clone())),
    );
    if production {
        let production_families = reference_families
            .map(|views| views.into_iter().map(|view| prod_graph(&view)).collect());
        Some(production_common_law_outcome::<panproto_lens::GraphShape>(
            &pair.production,
            &prod_graph(source),
            (),
            production_families,
        ))
    } else {
        Some(pair.with_reference(|lens| {
            let lens = lens.expect("the common-basis graph checker requires a compiled lens");
            reference_common_law_outcome::<RefGraphShape, _>(lens, source, (), reference_families)
        }))
    }
}

fn tree_common_laws(
    pair: &Pair,
    source: &RefTree,
    candidates: &[Candidate<RefTree>],
    production: bool,
) -> Option<CommonBasisLawOutcome> {
    if pair.with_reference(|lens| lens.is_err()) {
        return None;
    }
    let reference_families = partition_common_views(
        candidates.len(),
        candidates
            .iter()
            .map(|candidate| (candidate.mutation, candidate.view.clone())),
    );
    if production {
        let production_families = reference_families
            .map(|views| views.into_iter().map(|view| prod_tree(&view)).collect());
        Some(production_common_law_outcome::<panproto_lens::TreeShape>(
            &pair.production,
            &prod_tree(source),
            (),
            production_families,
        ))
    } else {
        Some(pair.with_reference(|lens| {
            let lens = lens.expect("the common-basis tree checker requires a compiled lens");
            reference_common_law_outcome::<RefTreeShape, _>(lens, source, (), reference_families)
        }))
    }
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
            for case in RefGraphShape::single_step_basis(lens, source, &view, &complement, &()) {
                push_candidate(
                    &mut out,
                    ref_mutation_index(case.mutation),
                    case.view,
                    matches!(case.domain, DomainExpectation::Admitted),
                );
            }
        }
    });
    let production_source = prod_graph(source);
    if let Ok((view, complement)) = graph_lens::get(&pair.production, &production_source) {
        for (family, mutation) in Mutation::ALL.into_iter().enumerate() {
            match <panproto_lens::GraphShape as LawShape>::mutation_cases(
                &pair.production,
                &production_source,
                &view,
                &complement,
                &(),
                mutation,
            ) {
                MutationCases::Cases(views) => {
                    for view in views {
                        push_candidate(&mut out, family, ref_graph(&view), false);
                    }
                }
                MutationCases::Skip(_) => {}
            }
            for case in <panproto_lens::GraphShape as LawShape>::negative_mutation_cases(
                &pair.production,
                &production_source,
                &view,
                &complement,
                &(),
                mutation,
            ) {
                push_candidate(&mut out, family, ref_graph(&case.view), false);
            }
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
            for case in RefTreeShape::single_step_basis(lens, source, &view, &complement, &()) {
                push_candidate(
                    &mut out,
                    ref_mutation_index(case.mutation),
                    case.view,
                    matches!(case.domain, DomainExpectation::Admitted),
                );
            }
        }
    });
    let production_source = prod_tree(source);
    if let Ok((view, complement)) = panproto_lens::get(&pair.production, &production_source) {
        for (family, mutation) in Mutation::ALL.into_iter().enumerate() {
            match <panproto_lens::TreeShape as LawShape>::mutation_cases(
                &pair.production,
                &production_source,
                &view,
                &complement,
                &(),
                mutation,
            ) {
                MutationCases::Cases(views) => {
                    for view in views {
                        push_candidate(&mut out, family, ref_tree(&view), false);
                    }
                }
                MutationCases::Skip(_) => {}
            }
            for case in <panproto_lens::TreeShape as LawShape>::negative_mutation_cases(
                &pair.production,
                &production_source,
                &view,
                &complement,
                &(),
                mutation,
            ) {
                push_candidate(&mut out, family, ref_tree(&case.view), false);
            }
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
    mutation: usize,
    view: RefTable,
    positive_reference: bool,
    declared_negative: bool,
}

fn push_table_candidate(
    candidates: &mut Vec<TableCandidate>,
    mutation: usize,
    view: RefTable,
    positive_reference: bool,
    declared_negative: bool,
) {
    if let Some(candidate) = candidates
        .iter_mut()
        .find(|candidate| candidate.mutation == mutation && candidate.view == view)
    {
        candidate.positive_reference |= positive_reference;
        candidate.declared_negative |= declared_negative;
        return;
    }
    candidates.push(TableCandidate {
        mutation,
        view,
        positive_reference,
        declared_negative,
    });
}

/// The deterministic union of the independent reference and production table
/// bases. Every positive proposal is retained even when its own side's exact
/// predicate refuses it; deliberately negative candidates are likewise
/// retained so that the differential checks the complete admission partition.
fn table_candidates(pair: &TablePair, source: &RefTable) -> Vec<TableCandidate> {
    let mut candidates = Vec::new();
    pair.with_reference(|lens, _| {
        if let Ok((view, complement)) = RefTableShape::get(lens, source, &()) {
            for case in RefTableShape::single_step_basis(lens, source, &view, &complement, &()) {
                match case.domain {
                    DomainExpectation::Admitted => {
                        push_table_candidate(
                            &mut candidates,
                            ref_mutation_index(case.mutation),
                            case.view,
                            true,
                            false,
                        );
                    }
                    DomainExpectation::Refused(_) => push_table_candidate(
                        &mut candidates,
                        ref_mutation_index(case.mutation),
                        case.view,
                        false,
                        true,
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
        for (family, mutation) in Mutation::ALL.into_iter().enumerate() {
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
                        push_table_candidate(
                            &mut candidates,
                            family,
                            ref_table(&candidate),
                            false,
                            false,
                        );
                    }
                }
                MutationCases::Skip(_) => {}
            }
            for case in <panproto_lens::TableShape as LawShape>::negative_mutation_cases(
                &pair.production,
                &production_source,
                &view,
                &complement,
                &context,
                mutation,
            ) {
                push_table_candidate(&mut candidates, family, ref_table(&case.view), false, true);
            }
        }
    }
    candidates
}

fn table_common_laws(
    pair: &TablePair,
    source: &RefTable,
    candidates: &[TableCandidate],
    production: bool,
) -> CommonBasisLawOutcome {
    let reference_families = partition_common_views(
        candidates.len(),
        candidates
            .iter()
            .map(|candidate| (candidate.mutation, candidate.view.clone())),
    );
    if production {
        let production_families = reference_families
            .map(|views| views.into_iter().map(|view| prod_table(&view)).collect());
        production_common_law_outcome::<panproto_lens::TableShape>(
            &pair.production,
            &prod_table(source),
            panproto_lens::TableContext {
                src: pair.protocol.clone(),
                tgt: pair.protocol.clone(),
            },
            production_families,
        )
    } else {
        pair.with_reference(|lens, _| {
            reference_common_law_outcome::<RefTableShape, _>(lens, source, (), reference_families)
        })
    }
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
        if candidate.declared_negative {
            assert_observed_table_negative(Mutation::ALL[candidate.mutation], verdict);
            counts[candidate.mutation] += 1;
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

struct CommonBasisTally {
    families: [std::sync::atomic::AtomicUsize; EDIT_FAMILIES],
}

impl Default for CommonBasisTally {
    fn default() -> Self {
        Self {
            families: std::array::from_fn(|_| std::sync::atomic::AtomicUsize::new(0)),
        }
    }
}

impl CommonBasisTally {
    fn add(&self, counts: &[usize; EDIT_FAMILIES]) {
        use std::sync::atomic::Ordering;
        for (total, count) in self.families.iter().zip(counts) {
            total.fetch_add(*count, Ordering::Relaxed);
        }
    }

    fn snapshot(&self) -> [usize; EDIT_FAMILIES] {
        use std::sync::atomic::Ordering;
        std::array::from_fn(|family| self.families[family].load(Ordering::Relaxed))
    }
}

impl Default for AdmissionTally {
    fn default() -> Self {
        Self {
            candidates: std::sync::atomic::AtomicUsize::new(0),
            admitted: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

/// The law-report case families of the L-lenses-8 scope.
#[derive(Debug)]
enum LawCase {
    Partial(PartialLens, u8),
    Table(TableFixture),
    Graph(usize, RefGraph),
    Tree(usize, RefTree),
}

/// One common exact-presentation union prepared once for both sides.
enum PreparedCommonBasis {
    None,
    Graph(Vec<Candidate<RefGraph>>),
    Tree(Vec<Candidate<RefTree>>),
    Table(Vec<TableCandidate>),
}

impl PreparedCommonBasis {
    const fn len(&self) -> Option<usize> {
        match self {
            Self::None => None,
            Self::Graph(candidates) => Some(candidates.len()),
            Self::Tree(candidates) => Some(candidates.len()),
            Self::Table(candidates) => Some(candidates.len()),
        }
    }

    fn family_counts(&self) -> Option<[usize; EDIT_FAMILIES]> {
        let mut counts = [0_usize; EDIT_FAMILIES];
        match self {
            Self::None => return None,
            Self::Graph(candidates) => {
                for candidate in candidates {
                    counts[candidate.mutation] += 1;
                }
            }
            Self::Tree(candidates) => {
                for candidate in candidates {
                    counts[candidate.mutation] += 1;
                }
            }
            Self::Table(candidates) => {
                for candidate in candidates {
                    counts[candidate.mutation] += 1;
                }
            }
        }
        Some(counts)
    }
}

struct PreparedLawCase<'a> {
    case: &'a LawCase,
    common: PreparedCommonBasis,
}

impl std::fmt::Debug for PreparedLawCase<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.case, formatter)
    }
}

fn prepare_law_case<'a>(context: &LawContext, case: &'a LawCase) -> PreparedLawCase<'a> {
    let (pairs, table_pairs, _) = context;
    let common = match case {
        LawCase::Graph(stage, source) => {
            PreparedCommonBasis::Graph(graph_candidates(&pairs[*stage], source))
        }
        LawCase::Tree(stage, tree) if reference_tree_case(&pairs[*stage], tree).is_some() => {
            PreparedCommonBasis::Tree(tree_candidates(&pairs[*stage], tree))
        }
        LawCase::Table(fixture) => {
            let pair = &table_pairs[fixture.columns][fixture.retained];
            PreparedCommonBasis::Table(table_candidates(pair, &fixture.source))
        }
        LawCase::Partial(_, _) | LawCase::Tree(_, _) => PreparedCommonBasis::None,
    };
    PreparedLawCase { case, common }
}

/// Admission and full five-gate observations over one independently formed
/// common basis. The immutable union is formed once, then both sides feed it
/// to their own admission predicate and full five-gate checker.
fn common_basis_evidence(
    pairs: &[Pair],
    table_pairs: &[Vec<TablePair>],
    prepared: &PreparedLawCase<'_>,
    production: bool,
    admission_tally: &AdmissionTally,
    basis_tally: &CommonBasisTally,
) -> (Option<AdmissionCensus>, Option<CommonBasisLawOutcome>) {
    let evidence = match (prepared.case, &prepared.common) {
        (LawCase::Graph(stage, source), PreparedCommonBasis::Graph(candidates)) => {
            let pair = &pairs[*stage];
            let census = if production {
                production_graph_admissions(&pairs[*stage], source, candidates)
            } else {
                reference_graph_admissions(&pairs[*stage], source, candidates)
            };
            let laws = graph_common_laws(pair, source, candidates, production);
            (census, laws)
        }
        (LawCase::Tree(stage, tree), PreparedCommonBasis::Tree(candidates)) => {
            let pair = &pairs[*stage];
            let census = if production {
                production_tree_admissions(pair, tree, candidates)
            } else {
                reference_tree_admissions(pair, tree, candidates)
            };
            let laws = tree_common_laws(pair, tree, candidates, production);
            (census, laws)
        }
        (LawCase::Table(fixture), PreparedCommonBasis::Table(candidates)) => {
            let pair = &table_pairs[fixture.columns][fixture.retained];
            let census = if production {
                production_table_admissions(pair, &fixture.source, candidates)
            } else {
                reference_table_admissions(pair, &fixture.source, candidates)
            };
            if !production && let Some(census) = &census {
                for (candidate, verdict) in candidates.iter().zip(&census.verdicts) {
                    if candidate.declared_negative {
                        assert_observed_table_negative(Mutation::ALL[candidate.mutation], verdict);
                    }
                }
            }
            let laws = Some(table_common_laws(
                pair,
                &fixture.source,
                candidates,
                production,
            ));
            (census, laws)
        }
        (LawCase::Partial(_, _) | LawCase::Tree(_, _), PreparedCommonBasis::None) => (None, None),
        _ => panic!("prepared L-lenses-8 case and common-basis carrier disagree"),
    };
    if let (Some(candidate_count), Some(census)) = (prepared.common.len(), &evidence.0) {
        assert_eq!(
            census.verdicts.len(),
            candidate_count,
            "the admission census must classify every common-basis candidate exactly once"
        );
    }
    if let (Some(expected), Some(outcome)) = (prepared.common.family_counts(), &evidence.1) {
        assert_eq!(
            outcome.family_counts, expected,
            "the full checker must preserve the prepared common-basis family counts"
        );
    }
    if !production && let Some(census) = &evidence.0 {
        use std::sync::atomic::Ordering;
        admission_tally
            .candidates
            .fetch_add(census.verdicts.len(), Ordering::Relaxed);
        admission_tally
            .admitted
            .fetch_add(census.admitted, Ordering::Relaxed);
    }
    if !production && let Some(outcome) = &evidence.1 {
        basis_tally.add(&outcome.family_counts);
    }
    evidence
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

fn id_h_empty_basis_refusal_witnesses() -> [RefGraph; 2] {
    let label = RefSEdge::new("h", "a", "hyper_parent", Some("p"));
    let graph = |node| RefGraph {
        nodes: BTreeMap::from([(0, node)]),
        edges: BTreeMap::from([(
            0,
            RefGEdge {
                src: 0,
                tgt: 0,
                label: label.clone(),
                position: None,
            },
        )]),
    };

    let mut hyper = RefNode::new(0, "h");
    hyper.shape = RefNodeShape::HyperEdge;
    [graph(hyper), graph(RefNode::new(0, "a"))]
}

#[test]
fn empty_common_basis_refusals_normalize_identically() {
    let stages = bounded_lens_stages();
    let stage = bounded_lens_stage_index(&stages, "Id_h");
    assert_eq!(stage, 16, "Id_h remains the seventeenth bounded lens stage");
    let pair = Pair::from_stage(&stages[stage]);

    for source in id_h_empty_basis_refusal_witnesses() {
        let production_native = production_law_outcome(&graph_lens::check_laws(
            &pair.production,
            &prod_graph(&source),
        ));
        let reference_native = reference_graph_law_case(&pair, &source);
        assert_eq!(production_native, refused_law_outcome());
        assert_eq!(production_native, reference_native);

        let candidates = graph_candidates(&pair, &source);
        assert!(
            candidates.is_empty(),
            "the refused Id_h source has no mutation basis"
        );
        let production_common = graph_common_laws(&pair, &source, &candidates, true)
            .expect("Id_h has an independently compiled reference lens");
        let reference_common = graph_common_laws(&pair, &source, &candidates, false)
            .expect("Id_h has an independently compiled reference lens");
        assert_eq!(production_common.gates, refused_law_outcome());
        assert_eq!(production_common.family_counts, [0; EDIT_FAMILIES]);
        assert_eq!(production_common.admitted, None);
        assert_eq!(production_common, reference_common);
    }
}

fn stage_c_cycle_witnesses() -> [RefGraph; 2] {
    let forward = RefSEdge::new("a", "b", "prop", Some("e"));
    let backward = RefSEdge::new("b", "a", "prop", Some("e"));
    let graph = |parallel| {
        let mut edges = BTreeMap::from([
            (
                0,
                RefGEdge {
                    src: 1,
                    tgt: 0,
                    label: forward.clone(),
                    position: None,
                },
            ),
            (
                1,
                RefGEdge {
                    src: 0,
                    tgt: 1,
                    label: backward.clone(),
                    position: None,
                },
            ),
        ]);
        if parallel {
            edges.insert(
                2,
                RefGEdge {
                    src: 0,
                    tgt: 1,
                    label: backward.clone(),
                    position: None,
                },
            );
        }
        RefGraph {
            nodes: BTreeMap::from([(0, RefNode::new(0, "b")), (1, RefNode::new(1, "a"))]),
            edges,
        }
    };
    [graph(false), graph(true)]
}

/// The two isomorphic three-node stage-C states that expose a disagreement
/// between the production and reference common-basis law checkers.
fn stage_c_three_node_common_basis_witnesses() -> [RefGraph; 2] {
    let forward = RefSEdge::new("a", "b", "prop", Some("e"));
    let backward = RefSEdge::new("b", "a", "prop", Some("e"));
    let graph = |first_a, second_a| RefGraph {
        nodes: BTreeMap::from([
            (0, RefNode::new(0, "b")),
            (1, RefNode::new(1, "a")),
            (2, RefNode::new(2, "a")),
        ]),
        edges: BTreeMap::from([
            (
                0,
                RefGEdge {
                    src: first_a,
                    tgt: 0,
                    label: forward.clone(),
                    position: None,
                },
            ),
            (
                1,
                RefGEdge {
                    src: 0,
                    tgt: first_a,
                    label: backward.clone(),
                    position: None,
                },
            ),
            (
                2,
                RefGEdge {
                    src: 0,
                    tgt: second_a,
                    label: backward.clone(),
                    position: None,
                },
            ),
        ]),
    };
    [graph(1, 2), graph(2, 1)]
}

#[test]
fn stage_c_three_node_common_basis_laws_agree() {
    let stages = bounded_lens_stages();
    let stage = bounded_lens_stage_index(&stages, "C");
    assert_eq!(stage, 21, "C remains the twenty-second bounded lens stage");
    let pair = Pair::from_stage(&stages[stage]);
    let expected_counts = [2, 4, 1, 2, 0, 0, 0, 0, 0, 2, 0, 0, 2, 2, 4, 0, 0];

    let outcomes: Vec<_> = stage_c_three_node_common_basis_witnesses()
        .into_iter()
        .map(|source| {
            let candidates = graph_candidates(&pair, &source);
            let reference_families = partition_common_views(
                candidates.len(),
                candidates
                    .iter()
                    .map(|candidate| (candidate.mutation, candidate.view.clone())),
            );
            let ordered_reference_basis: Vec<_> = reference_families
                .iter()
                .enumerate()
                .flat_map(|(family, views)| {
                    views
                        .iter()
                        .map(move |view| (Mutation::ALL[family], view))
                })
                .collect();
            let reference_report = pair.with_reference(|lens| {
                let lens = lens.expect("stage C compiles in the independent reference");
                reference_common_law_report::<RefGraphShape, _>(
                    lens,
                    &source,
                    (),
                    reference_families.clone(),
                )
                .0
            });
            assert_eq!(reference_report.admitted, 11);
            let put_twice = reference_report
                .gates
                .iter()
                .find(|gate| gate.gate == panproto_reference::lens::LawGate::PutTwice)
                .expect("the five-gate report contains PutTwice");
            match &put_twice.status {
                LawGateStatus::Passed { cases } => assert_eq!(*cases, 11),
                LawGateStatus::Failed(panproto_reference::lens::LawFailure::Refusal {
                    case: Some(case),
                    operation,
                    refusal,
                }) => {
                    let (mutation, view) = ordered_reference_basis
                        .get(*case)
                        .expect("the failure ordinal names a common-basis candidate");
                    panic!(
                        "stage-C common-basis PutTwice refused: case={case}, mutation={mutation:?}, operation={operation:?}, refusal={refusal:#?}, view={view:#?}"
                    );
                }
                status => panic!("stage-C common-basis PutTwice failed: {status:#?}"),
            }
            let production_admission = production_graph_admissions(&pair, &source, &candidates)
                .expect("stage C gets on the production side");
            let reference_admission = reference_graph_admissions(&pair, &source, &candidates)
                .expect("stage C gets on the reference side");
            assert_eq!(production_admission, reference_admission);
            assert_eq!(production_admission.admitted, 3);

            let production = graph_common_laws(&pair, &source, &candidates, true)
                .expect("stage C compiles on both sides");
            let reference = graph_common_laws(&pair, &source, &candidates, false)
                .expect("stage C compiles on both sides");
            assert_eq!(production.family_counts, expected_counts);
            assert_eq!(reference.family_counts, expected_counts);
            (production, reference)
        })
        .collect();

    assert_eq!(outcomes.len(), 2);
    assert_eq!(outcomes[0].0, outcomes[0].1);
    assert_eq!(outcomes[1].0, outcomes[1].1);
}

#[derive(Debug)]
struct GraphPutGetTrace {
    mutation: &'static str,
    edited_view: RefGraph,
    original_view: RefGraph,
    original_complement: RefComplement,
    restored_source: RefGraph,
    observed_view: RefGraph,
    observed_complement: RefComplement,
    expected_index: String,
    observed_index: String,
    equivalent: bool,
    divergence: Option<String>,
}

fn production_graph_put_get_trace(
    pair: &Pair,
    source: &RefGraph,
    candidate: &Candidate<RefGraph>,
) -> GraphPutGetTrace {
    let source = prod_graph(source);
    let (original_view, complement) = graph_lens::get(&pair.production, &source)
        .expect("the stage-C witness is in the production get domain");
    let edited_view = prod_graph(&candidate.view);
    assert_eq!(
        graph_lens::check_put_domain(&pair.production, &edited_view, &complement),
        Ok(()),
        "the trace is only defined for an admitted production candidate"
    );
    let restored_source = graph_lens::put(&pair.production, &edited_view, &complement)
        .expect("an admitted production candidate is total");
    let (observed_view, observed_complement) = graph_lens::get(&pair.production, &restored_source)
        .expect("the restored production source regets");
    let expected_index = <panproto_lens::GraphShape as panproto_lens::LensShape>::synth_index(
        &edited_view,
        &complement,
    );
    let observed_index = <panproto_lens::GraphShape as panproto_lens::LensShape>::synth_index(
        &observed_view,
        &observed_complement,
    );
    let equivalent = <panproto_lens::GraphShape as panproto_lens::LensShape>::equivalent(
        &edited_view,
        &expected_index,
        &observed_view,
        &observed_index,
    );
    let divergence = <panproto_lens::GraphShape as panproto_lens::LensShape>::divergence(
        &edited_view,
        &expected_index,
        &observed_view,
        &observed_index,
    );
    GraphPutGetTrace {
        mutation: Mutation::ALL[candidate.mutation].as_str(),
        edited_view: candidate.view.clone(),
        original_view: ref_graph(&original_view),
        original_complement: ref_complement(&complement),
        restored_source: ref_graph(&restored_source),
        observed_view: ref_graph(&observed_view),
        observed_complement: ref_complement(&observed_complement),
        expected_index: format!("{expected_index:?}"),
        observed_index: format!("{observed_index:?}"),
        equivalent,
        divergence,
    }
}

fn reference_graph_put_get_trace(
    pair: &Pair,
    source: &RefGraph,
    candidate: &Candidate<RefGraph>,
) -> GraphPutGetTrace {
    pair.with_reference(|lens| {
        let lens = lens.expect("stage C compiles in the independent reference");
        let (original_view, complement) = panproto_reference::lens::get(lens, source)
            .expect("the stage-C witness is in the reference get domain");
        assert_eq!(
            RefGraphLens::check_put_domain(lens, &candidate.view, &complement),
            Ok(()),
            "the trace is only defined for an admitted reference candidate"
        );
        let restored_source = RefGraphLens::put(lens, &candidate.view, &complement)
            .expect("an admitted reference candidate is total");
        let (observed_view, observed_complement) =
            panproto_reference::lens::get(lens, &restored_source)
                .expect("the restored reference source regets");
        let expected_index = <RefGraphShape as panproto_reference::lens::LensShape>::synth_index(
            &candidate.view,
            &complement,
        );
        let observed_index = <RefGraphShape as panproto_reference::lens::LensShape>::synth_index(
            &observed_view,
            &observed_complement,
        );
        let equivalent = <RefGraphShape as panproto_reference::lens::LensShape>::equivalent(
            &candidate.view,
            &expected_index,
            &observed_view,
            &observed_index,
        );
        let divergence = <RefGraphShape as panproto_reference::lens::LensShape>::divergence(
            &candidate.view,
            &expected_index,
            &observed_view,
            &observed_index,
        )
        .map(|difference| format!("{difference:?}"));
        GraphPutGetTrace {
            mutation: Mutation::ALL[candidate.mutation].as_str(),
            edited_view: candidate.view.clone(),
            original_view,
            original_complement: without_identity(&complement),
            restored_source,
            observed_view,
            observed_complement: without_identity(&observed_complement),
            expected_index: format!("{expected_index:?}"),
            observed_index: format!("{observed_index:?}"),
            equivalent,
            divergence,
        }
    })
}

#[test]
fn stage_c_common_basis_put_get_traces_agree() {
    let stages = bounded_lens_stages();
    let stage = bounded_lens_stage_index(&stages, "C");
    assert_eq!(stage, 21, "C remains the twenty-second bounded lens stage");
    let pair = Pair::from_stage(&stages[stage]);

    for source in stage_c_cycle_witnesses() {
        let production_native = production_law_outcome(&graph_lens::check_laws(
            &pair.production,
            &prod_graph(&source),
        ));
        let reference_native = reference_graph_law_case(&pair, &source);
        assert!(production_native.first_failure.is_none());
        assert_eq!(production_native, reference_native);
        let candidates = graph_candidates(&pair, &source);
        assert_eq!(
            PreparedCommonBasis::Graph(graph_candidates(&pair, &source)).family_counts(),
            Some([1, 2, 1, 1, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 1, 0, 0])
        );
        let reference_report = pair.with_reference(|lens| {
            let lens = lens.expect("stage C compiles in the independent reference");
            let families = partition_common_views(
                candidates.len(),
                candidates
                    .iter()
                    .map(|candidate| (candidate.mutation, candidate.view.clone())),
            );
            reference_common_law_report::<RefGraphShape, _>(lens, &source, (), families).0
        });
        assert_eq!(reference_report.admitted, 6);
        assert_eq!(
            reference_report
                .gates
                .iter()
                .find(|gate| gate.gate == panproto_reference::lens::LawGate::PutGet)
                .map(|gate| &gate.status),
            Some(&LawGateStatus::Passed { cases: 6 })
        );
        assert_eq!(
            reference_report
                .gates
                .iter()
                .find(|gate| gate.gate == panproto_reference::lens::LawGate::Congruence)
                .map(|gate| &gate.status),
            Some(&LawGateStatus::Passed { cases: 1 })
        );
        let production_admissions = production_graph_admissions(&pair, &source, &candidates)
            .expect("stage C gets on the production side");
        let reference_admissions = reference_graph_admissions(&pair, &source, &candidates)
            .expect("stage C gets on the reference side");
        assert_eq!(production_admissions, reference_admissions);
        assert_eq!(
            production_admissions
                .verdicts
                .iter()
                .filter(|verdict| verdict.is_admitted())
                .count(),
            6
        );

        let remove_node_family = Mutation::ALL
            .iter()
            .position(|mutation| *mutation == Mutation::RemoveNode)
            .expect("Mutation::ALL contains RemoveNode");
        let admitted_remove_node: Vec<_> = candidates
            .iter()
            .zip(&production_admissions.verdicts)
            .filter(|(candidate, verdict)| {
                candidate.mutation == remove_node_family && verdict.is_admitted()
            })
            .map(|(candidate, _)| candidate)
            .collect();
        let [removed] = admitted_remove_node.as_slice() else {
            panic!("each stage-C cycle witness has exactly one admitted node removal")
        };
        assert!(removed.view.nodes.is_empty());
        assert!(removed.view.edges.is_empty());
        pair.with_reference(|lens| {
            let lens = lens.expect("stage C compiles in the independent reference");
            let (original_view, complement) = panproto_reference::lens::get(lens, &source)
                .expect("the stage-C witness is in the reference get domain");
            let original_index =
                <RefGraphShape as panproto_reference::lens::LensShape>::synth_index(
                    &original_view,
                    &complement,
                );
            let deleted_index = <RefGraphShape as panproto_reference::lens::LensShape>::synth_index(
                &removed.view,
                &complement,
            );
            assert_eq!(original_index.nodes.len(), 0);
            assert_eq!(original_index.edges.len(), 1);
            assert!(deleted_index.nodes.is_empty());
            assert!(deleted_index.edges.is_empty());
        });

        assert_stage_c_put_get_traces(&pair, &source, &candidates, &production_admissions.verdicts);
    }
}

/// Every admitted stage-C candidate's `PutGet` trace agrees field for field,
/// with nonempty synthesized indexes and a divergence exactly when the
/// verdict is not equivalence.
fn assert_stage_c_put_get_traces(
    pair: &Pair,
    source: &RefGraph,
    candidates: &[Candidate<RefGraph>],
    verdicts: &[Admission],
) {
    for (candidate, verdict) in candidates.iter().zip(verdicts) {
        if !verdict.is_admitted() {
            continue;
        }
        let production = production_graph_put_get_trace(pair, source, candidate);
        let reference = reference_graph_put_get_trace(pair, source, candidate);
        assert_eq!(production.mutation, reference.mutation);
        assert_eq!(production.edited_view, reference.edited_view);
        assert_eq!(production.original_view, reference.original_view);
        assert_eq!(
            production.original_complement,
            reference.original_complement
        );
        assert_eq!(production.restored_source, reference.restored_source);
        assert_eq!(production.observed_view, reference.observed_view);
        assert_eq!(
            production.observed_complement,
            reference.observed_complement
        );
        assert_eq!(
            production.equivalent, reference.equivalent,
            "stage-C common-basis PutGet verdict differs\nproduction={production:#?}\nreference={reference:#?}"
        );
        assert_ne!(production.expected_index, "");
        assert_ne!(reference.expected_index, "");
        assert_ne!(production.observed_index, "");
        assert_ne!(reference.observed_index, "");
        assert_eq!(production.divergence.is_none(), production.equivalent);
        assert_eq!(reference.divergence.is_none(), reference.equivalent);
    }
}

/// The environment variable that selects one shard of the bounded lens law
/// sweep.
const LAW_SHARD_VARIABLE: &str = "PANPROTO_TEST_SHARD";

/// One shard of a sweep split `count` ways: the cases whose position in the
/// full enumeration is congruent to `index` modulo `count`. The `count`
/// shards with indexes `0` to `count - 1` partition the cases, and taking
/// residues rather than contiguous blocks spreads every case family evenly
/// over the shards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LawShard {
    index: usize,
    count: usize,
}

impl LawShard {
    /// Parse `index/count`: two unsigned decimal numerals with
    /// `0 <= index < count`.
    fn parse(value: &str) -> Result<Self, String> {
        let numeral = |part: &str, role: &str| {
            if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(format!(
                    "the shard {role} {part:?} is not an unsigned decimal numeral"
                ));
            }
            part.parse::<usize>()
                .map_err(|error| format!("the shard {role} {part:?} does not fit: {error}"))
        };
        let (index, count) = value
            .split_once('/')
            .ok_or_else(|| format!("{value:?} is not of the form index/count"))?;
        let (index, count) = (numeral(index, "index")?, numeral(count, "count")?);
        if count == 0 {
            return Err(format!("{value:?} has a shard count of zero"));
        }
        if index >= count {
            return Err(format!(
                "{value:?} has shard index {index}, which is not below the shard count {count}"
            ));
        }
        Ok(Self { index, count })
    }

    /// The shard named by [`LAW_SHARD_VARIABLE`], or `None` when the variable
    /// is unset. A value that is not a well-formed shard panics, so a
    /// misconfigured job fails instead of checking the wrong cases.
    fn from_environment() -> Option<Self> {
        match std::env::var(LAW_SHARD_VARIABLE) {
            Err(std::env::VarError::NotPresent) => None,
            Err(std::env::VarError::NotUnicode(value)) => {
                panic!(
                    "{LAW_SHARD_VARIABLE}={} is not valid Unicode",
                    value.display()
                )
            }
            Ok(value) => Some(Self::parse(&value).unwrap_or_else(|error| {
                panic!("{LAW_SHARD_VARIABLE} must be index/count with index < count: {error}")
            })),
        }
    }

    /// Whether the case at `position` of the full enumeration is in this
    /// shard.
    const fn contains(self, position: usize) -> bool {
        position % self.count == self.index
    }

    /// The number of the first `total` positions that are in this shard.
    const fn share(self, total: usize) -> usize {
        total.saturating_sub(self.index).div_ceil(self.count)
    }
}

#[test]
fn law_shard_accepts_only_an_index_below_its_count() {
    assert_eq!(LawShard::parse("0/1"), Ok(LawShard { index: 0, count: 1 }));
    assert_eq!(
        LawShard::parse("7/12"),
        Ok(LawShard {
            index: 7,
            count: 12
        })
    );
    for malformed in [
        "",
        "3",
        "/4",
        "3/",
        "4/4",
        "5/4",
        "0/0",
        "-1/4",
        "+1/4",
        " 1/4",
        "1/4 ",
        "1/2/3",
        "a/4",
        "1/99999999999999999999999",
    ] {
        assert!(
            LawShard::parse(malformed).is_err(),
            "{malformed:?} must not parse as a shard"
        );
    }
    for count in 1..=9 {
        for total in 0..=40 {
            let shares: usize = (0..count)
                .map(|index| {
                    let shard = LawShard { index, count };
                    let selected = (0..total)
                        .filter(|position| shard.contains(*position))
                        .count();
                    assert_eq!(shard.share(total), selected);
                    selected
                })
                .sum();
            assert_eq!(shares, total);
        }
    }
}

/// The per-family candidate counts of the prepared common basis of every
/// case, summed in parallel with one context per worker.
fn common_basis_family_census<C: Sync>(
    cases: &[C],
    context: impl Fn() -> LawContext + Sync,
    prepare: impl Fn(&LawContext, &C) -> Option<[usize; EDIT_FAMILIES]> + Sync,
) -> [usize; EDIT_FAMILIES] {
    use std::sync::atomic::{AtomicUsize, Ordering};

    const BLOCK: usize = 64;
    let next = AtomicUsize::new(0);
    let partials: Vec<[usize; EDIT_FAMILIES]> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..comparison_threads())
            .map(|_| {
                scope.spawn(|| {
                    let context = context();
                    let mut counts = [0_usize; EDIT_FAMILIES];
                    loop {
                        let start = next.fetch_add(BLOCK, Ordering::Relaxed);
                        if start >= cases.len() {
                            return counts;
                        }
                        for case in &cases[start..(start + BLOCK).min(cases.len())] {
                            if let Some(case_counts) = prepare(&context, case) {
                                for (total, count) in counts.iter_mut().zip(case_counts) {
                                    *total += count;
                                }
                            }
                        }
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .expect("a common-basis census worker panicked")
            })
            .collect()
    });
    let mut total = [0_usize; EDIT_FAMILIES];
    for partial in partials {
        for (total, count) in total.iter_mut().zip(partial) {
            *total += count;
        }
    }
    total
}

/// Production and the reference agree on the law report of every case of
/// the bounded lens scope, and on the admission census and full five-gate
/// outcome of every case's common basis.
///
/// With [`LAW_SHARD_VARIABLE`] unset the test compares every case. With it
/// set to `i/n` the test still enumerates the full scope and asserts its
/// census, then compares only the cases of [`LawShard`] `i` of `n`, and
/// asserts that it compared exactly that shard's share. The one whole-run
/// assertion, [`assert_bounded_common_basis_families`], holds of the
/// per-case family counts, which each comparison asserts equal to the counts
/// of the prepared basis. A shard therefore prepares the common basis of
/// every case of the scope (not only its own), makes that assertion of the
/// full census, and asserts that the family counts its comparisons tallied
/// equal the prepared counts of its own cases. Running
/// the `n` shards `0/n` to `n-1/n` thus compares every case once and checks
/// every assertion of the unsharded run; the admission census is printed
/// per shard, and the shards' printed counts sum to the unsharded totals.
#[test]
fn lens_laws_agree_with_the_reference_exhaustively() {
    let stages = bounded_lens_stages();
    // The lens pairs of every family, built once per thread: they hold
    // value layers that are not shared between threads.
    let context = || {
        let pairs: Vec<Pair> = stages.iter().map(Pair::from_stage).collect();
        (pairs, table_pairs(), empty_lens())
    };
    let mut cases = Vec::new();
    for_each_lens_law_case(|case| match case {
        LensLawCertificateCase::Partial { lens, source } => {
            cases.push(LawCase::Partial(lens, source));
        }
        LensLawCertificateCase::Concrete(LensEquivalenceCase::Table(fixture)) => {
            cases.push(LawCase::Table(fixture));
        }
        LensLawCertificateCase::Concrete(LensEquivalenceCase::Graph { stage, source }) => {
            cases.push(LawCase::Graph(stage, source));
        }
        LensLawCertificateCase::Concrete(LensEquivalenceCase::Tree { stage, source }) => {
            cases.push(LawCase::Tree(stage, source));
        }
    });
    let partial = cases
        .iter()
        .filter(|case| matches!(case, LawCase::Partial(_, _)))
        .count();
    let tables = cases
        .iter()
        .filter(|case| matches!(case, LawCase::Table(_)))
        .count();
    assert_eq!(cases.len(), lens_law_case_count_certificate());
    assert_eq!(cases.len(), 200_606);
    assert_eq!(partial, 2 * partial_lens_count_certificate());
    assert_eq!(tables, table_fixture_count_certificate().total);
    let bound = format!(
        "L-lenses-8 law reports: partial lens/source cases={partial}, table fixtures={tables}, bounded lens graph and tree (stage, state) cases={}, total={}",
        cases.len() - partial - tables,
        cases.len()
    );
    println!("{bound}");
    let shard = LawShard::from_environment();
    let selected = law_shard_cases(shard, &cases);
    let (compared, tally, common_tally) = compare_law_cases(&bound, &selected, context);
    println!(
        "L-lenses-8 admission census (graph, tree and table families): candidates={}, admitted positive reference-basis candidates={}",
        tally.candidates.load(std::sync::atomic::Ordering::Relaxed),
        tally.admitted.load(std::sync::atomic::Ordering::Relaxed),
    );
    let common_counts = common_tally.snapshot();
    println!("L-lenses-8 common five-gate basis by mutation family: {common_counts:?}");
    let Some(shard) = shard else {
        assert_bounded_common_basis_families(&common_counts);
        assert_eq!(compared, 200_606);
        return;
    };
    assert_eq!(
        u64::try_from(shard.share(cases.len())).expect("a case count fits in u64"),
        compared,
        "shard {}/{} must compare exactly its share of the cases",
        shard.index,
        shard.count
    );
    assert_law_shard_common_basis(&cases, &selected, common_counts, context);
}

/// The cases of `shard` in enumeration order, or every case when there is
/// no shard, after checking that there are exactly the shard's share of them.
fn law_shard_cases(shard: Option<LawShard>, cases: &[LawCase]) -> Vec<&LawCase> {
    let selected: Vec<&LawCase> = cases
        .iter()
        .enumerate()
        .filter(|(position, _)| shard.is_none_or(|shard| shard.contains(*position)))
        .map(|(_, case)| case)
        .collect();
    let Some(shard) = shard else {
        assert_eq!(selected.len(), cases.len());
        return selected;
    };
    assert_eq!(selected.len(), shard.share(cases.len()));
    println!(
        "L-lenses-8 shard {}/{}: comparing {} of {} cases",
        shard.index,
        shard.count,
        selected.len(),
        cases.len()
    );
    selected
}

/// A shard's common-basis assertions: the five-gate family counts its
/// comparisons tallied equal the prepared common basis of its own cases, and
/// every mutation family occurs in the prepared common basis of the full
/// scope.
fn assert_law_shard_common_basis(
    cases: &[LawCase],
    selected: &[&LawCase],
    tallied: [usize; EDIT_FAMILIES],
    context: impl Fn() -> LawContext + Sync + Copy,
) {
    let shard_basis = common_basis_family_census(selected, context, |context, case| {
        prepare_law_case(context, case).common.family_counts()
    });
    assert_eq!(
        tallied, shard_basis,
        "the shard's tallied five-gate family counts must equal its prepared common basis"
    );
    let full_basis = common_basis_family_census(cases, context, |context, case| {
        prepare_law_case(context, case).common.family_counts()
    });
    println!(
        "L-lenses-8 prepared common basis of the full scope by mutation family: {full_basis:?}"
    );
    assert_bounded_common_basis_families(&full_basis);
}

/// Compare production with the reference on every case of `cases` as the
/// L-lenses-8 sweep does: the law report of each case, then the admission
/// census and full five-gate outcome of its prepared common basis. Returns
/// the number of cases compared with the reference side's admission and
/// common-basis family tallies.
fn compare_law_cases(
    bound: &str,
    cases: &[&LawCase],
    context: impl Fn() -> LawContext + Sync,
) -> (u64, AdmissionTally, CommonBasisTally) {
    let tally = AdmissionTally::default();
    let common_tally = CommonBasisTally::default();
    let compared = assert_agree(agree_in_parallel(
        bound,
        cases,
        context,
        |context, case| vec![prepare_law_case(context, case)],
        |context: &LawContext, prepared: &PreparedLawCase<'_>| {
            let (pairs, table_pairs, _) = context;
            (
                production_law_case(context, prepared.case),
                common_basis_evidence(pairs, table_pairs, prepared, true, &tally, &common_tally),
            )
        },
        |context: &LawContext, prepared: &PreparedLawCase<'_>| {
            let (pairs, table_pairs, _) = context;
            (
                reference_law_case(context, prepared.case),
                common_basis_evidence(pairs, table_pairs, prepared, false, &tally, &common_tally),
            )
        },
        |production, reference| production == reference,
    ));
    (compared, tally, common_tally)
}

/// The position of the edge-relabel family in [`Mutation::ALL`], which the
/// reference's [`ref_mutation_index`] assigns to the same family.
fn relabel_edge_family() -> usize {
    let family = Mutation::ALL
        .iter()
        .position(|mutation| *mutation == Mutation::RelabelEdge)
        .expect("Mutation::ALL contains the edge-relabel family");
    assert_eq!(family, ref_mutation_index(RefMutation::RelabelEdge));
    family
}

/// The family assertion of the bounded scope's common basis: every mutation
/// family occurs in it except edge relabelling, which occurs zero times.
///
/// A relabel gives a view edge another target-schema edge with the same
/// endpoints, and no target schema of the bounded carrier has two distinct
/// edges between the same pair of vertices, so neither side proposes one.
/// `relabel_edge_laws_agree_with_the_reference` compares that family on a
/// parallel-edge carrier instead. The count is asserted to be zero rather
/// than left unchecked, so a carrier change that starts producing relabels
/// fails here and the exemption is revisited.
fn assert_bounded_common_basis_families(counts: &[usize; EDIT_FAMILIES]) {
    let relabel = relabel_edge_family();
    for (family, count) in counts.iter().enumerate() {
        if family == relabel {
            assert_eq!(
                *count, 0,
                "the bounded carrier has no parallel target edges, so its common basis has no edge relabels"
            );
        } else {
            assert!(
                *count > 0,
                "mutation family {} must occur in the independently formed common basis",
                Mutation::ALL[family].as_str()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Edge relabelling: the law differential on a parallel-edge carrier
// ---------------------------------------------------------------------------

use panproto_reference::enumerate::lenses::{BOUNDED_LENS_EDGE_KIND, BOUNDED_LENS_VERTEX_KIND};

/// The edge kind of the relabel carrier's table schemas.
const RELABEL_TABLE_EDGE_KIND: &str = "foreign-key";

/// The schema on vertices `a` and `b` of `kind` with one `edge_kind` edge
/// from `a` to `b` named by each of `names`.
fn relabel_edge_schema(kind: &str, edge_kind: &str, names: &[&str]) -> RefSchema {
    names.iter().fold(
        RefSchema::default().vertex("a", kind).vertex("b", kind),
        |schema, name| schema.edge("a", "b", edge_kind, Some(name)),
    )
}

/// The lenses of the relabel carrier over edges of `edge_kind`, as
/// `(source, target, migration)`; every target schema has the parallel
/// edges `x` and `y` from `a` to `b`. The first lens is the identity. The
/// second contracts the path `a -> m -> b` of a source with a third vertex
/// `m` into `y`, so `y` is synthesized and has no direct source preimage.
/// With `merge`, a third lens merges a third parallel source edge `z` into
/// `y`, so `y` has two direct source preimages; the table plan refuses such
/// a merge, so the table carrier omits it.
fn relabel_edge_lenses(
    kind: &str,
    edge_kind: &str,
    merge: bool,
) -> Vec<(RefSchema, RefSchema, RefMigration)> {
    let edge = |name| RefSEdge::new("a", "b", edge_kind, Some(name));
    let parallel = relabel_edge_schema(kind, edge_kind, &["x", "y"]);
    let mut lenses = vec![
        (
            parallel.clone(),
            parallel.clone(),
            RefMigration {
                surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
                surviving_edges: parallel.edges.clone(),
                ..RefMigration::default()
            },
        ),
        (
            relabel_edge_schema(kind, edge_kind, &["x"])
                .vertex("m", kind)
                .edge("a", "m", edge_kind, Some("left"))
                .edge("m", "b", edge_kind, Some("right")),
            parallel.clone(),
            RefMigration {
                surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
                surviving_edges: [edge("x")].into(),
                resolver: [(("a".to_owned(), "b".to_owned()), edge("y"))].into(),
                ..RefMigration::default()
            },
        ),
    ];
    if merge {
        lenses.push((
            relabel_edge_schema(kind, edge_kind, &["x", "y", "z"]),
            parallel,
            RefMigration {
                surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
                surviving_edges: [edge("x"), edge("y"), edge("z")].into(),
                edge_remap: [(edge("z"), edge("y"))].into(),
                ..RefMigration::default()
            },
        ));
    }
    lenses
}

/// The relabel carrier's law context: the graph and tree lenses of
/// [`relabel_edge_lenses`] over bounded lens kinds, then the table lenses
/// over table kinds as the one row of table pairs.
fn relabel_edge_law_context() -> LawContext {
    let pairs = relabel_edge_lenses(BOUNDED_LENS_VERTEX_KIND, BOUNDED_LENS_EDGE_KIND, true)
        .into_iter()
        .map(|(source, target, migration)| Pair::new(source, target, migration))
        .collect();
    let tables = relabel_edge_lenses(TABLE_FIXTURE_TABLE_KIND, RELABEL_TABLE_EDGE_KIND, false)
        .into_iter()
        .map(|(source, target, migration)| {
            TablePair::new(
                source,
                target,
                migration,
                TABLE_FIXTURE_VALUE_KIND,
                ScalarKind::Any,
            )
        })
        .collect();
    (pairs, vec![tables], empty_lens())
}

/// The relabel carrier's law cases. For each graph lens, the graph states
/// and tree states of at most two nodes over its source schema. For each
/// table lens, the tables with one row per source vertex, row ids in vertex
/// order, related by each set of the source schema's edges. A table case's
/// `columns` and `retained` index the context's table pairs, as they index
/// the fixture pairs in the bounded sweep's context.
fn relabel_edge_law_cases(context: &LawContext) -> Vec<LawCase> {
    let (pairs, table_pairs, _) = context;
    let mut cases = Vec::new();
    for (stage, pair) in pairs.iter().enumerate() {
        cases.extend(
            bounded_lens_graph_states(&pair.src)
                .into_iter()
                .filter(|graph| graph.nodes.len() <= 2)
                .map(|graph| LawCase::Graph(stage, graph)),
        );
        cases.extend(
            bounded_lens_tree_states(&pair.src)
                .into_iter()
                .filter(|tree| tree.nodes.len() <= 2)
                .map(|tree| LawCase::Tree(stage, tree)),
        );
    }
    for (retained, pair) in table_pairs[0].iter().enumerate() {
        let rows: BTreeMap<&str, u32> = pair
            .src
            .vertices
            .keys()
            .zip(0..)
            .map(|(vertex, id)| (vertex.as_str(), id))
            .collect();
        let edges: Vec<&RefSEdge> = pair.src.edges.iter().collect();
        for subset in 0..1_usize << edges.len() {
            let source = RefTable {
                rows: rows
                    .iter()
                    .map(|(vertex, id)| (*id, ((*vertex).to_owned(), BTreeMap::new())))
                    .collect(),
                pairs: edges
                    .iter()
                    .enumerate()
                    .filter(|(bit, _)| subset & (1 << bit) != 0)
                    .map(|(_, edge)| {
                        (
                            (*edge).clone(),
                            rows[edge.src.as_str()],
                            rows[edge.tgt.as_str()],
                        )
                    })
                    .collect(),
            };
            cases.push(LawCase::Table(TableFixture {
                columns: 0,
                retained,
                source,
            }));
        }
    }
    cases
}

impl PreparedCommonBasis {
    /// The mutation family of each candidate, in candidate order.
    fn mutations(&self) -> Vec<usize> {
        match self {
            Self::None => Vec::new(),
            Self::Graph(candidates) => candidates.iter().map(|c| c.mutation).collect(),
            Self::Tree(candidates) => candidates.iter().map(|c| c.mutation).collect(),
            Self::Table(candidates) => candidates.iter().map(|c| c.mutation).collect(),
        }
    }
}

/// The relabel carrier's edge relabels by shape and lens: how many
/// candidates of the common basis the put domain admits, how many it
/// refuses, and the variants of the refusals.
type RelabelEdgeCensus = BTreeMap<(&'static str, usize), (usize, usize, BTreeSet<String>)>;

/// The [`RelabelEdgeCensus`] of `cases`, from the reference's verdicts on
/// the common basis of each case, where `relabel` is the edge-relabel
/// family. The differential asserts that production's verdicts are equal.
fn relabel_edge_census(
    context: &LawContext,
    cases: &[LawCase],
    relabel: usize,
) -> RelabelEdgeCensus {
    let (pairs, table_pairs, _) = context;
    let mut census = RelabelEdgeCensus::new();
    for case in cases {
        let (shape, lens) = match case {
            LawCase::Graph(stage, _) => ("graph", *stage),
            LawCase::Tree(stage, _) => ("tree", *stage),
            LawCase::Table(fixture) => ("table", fixture.retained),
            LawCase::Partial(_, _) => unreachable!("the relabel carrier has no partial lenses"),
        };
        let prepared = prepare_law_case(context, case);
        let (admissions, _) = common_basis_evidence(
            pairs,
            table_pairs,
            &prepared,
            false,
            &AdmissionTally::default(),
            &CommonBasisTally::default(),
        );
        let Some(admissions) = admissions else {
            continue;
        };
        for (mutation, verdict) in prepared
            .common
            .mutations()
            .into_iter()
            .zip(&admissions.verdicts)
        {
            if mutation != relabel {
                continue;
            }
            let entry = census.entry((shape, lens)).or_default();
            let violations = match verdict {
                Admission::Admitted
                | Admission::Table {
                    domain: TableDomainVerdict::Admitted,
                    raw: TableRawVerdict::Admitted,
                } => {
                    entry.0 += 1;
                    continue;
                }
                Admission::Refused(violations)
                | Admission::Table {
                    domain: TableDomainVerdict::Refused { violations, .. },
                    raw: TableRawVerdict::Domain(_),
                } => violations,
                Admission::Table { .. } => {
                    panic!("a table relabel verdict is not a domain verdict: {verdict:?}")
                }
            };
            entry.1 += 1;
            entry.2.extend(violations.iter().map(|violation| {
                violation
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            }));
        }
    }
    census
}

/// Production and the reference agree on the law report, admission census
/// and full five-gate common-basis outcome of every case of a carrier whose
/// target schemas have two distinct edges between the same vertices. The
/// bounded sweep's carrier has no such schema, so this is where the two
/// implementations are compared on edge relabels.
///
/// For each shape (graph, tree, table) and lens the test pins the number of
/// edge-relabel candidates of the common basis that the put domain admits
/// and refuses, and the refusal variants. Under the identity every relabel
/// is admitted. Under the contraction a relabel of `x` to `y` is refused
/// with `ContractedEdgeInserted`, since `y` arises only by contraction.
/// Under the merge a relabel to `x` is admitted, since `x` has one direct
/// source preimage, and a relabel to `y` is refused with
/// `UnrepresentableViewEdge`, since `y` has two.
#[test]
fn relabel_edge_laws_agree_with_the_reference() {
    let relabel = relabel_edge_family();
    let context = relabel_edge_law_context();
    let cases = relabel_edge_law_cases(&context);
    let selected: Vec<&LawCase> = cases.iter().collect();
    let bound = format!(
        "L-lenses-8 parallel-edge relabel carrier: cases={}",
        cases.len()
    );
    println!("{bound}");
    let (compared, tally, common_tally) =
        compare_law_cases(&bound, &selected, relabel_edge_law_context);
    assert_eq!(
        compared,
        u64::try_from(cases.len()).expect("a case count fits in u64")
    );
    println!(
        "L-lenses-8 relabel carrier admission census: candidates={}, admitted positive reference-basis candidates={}",
        tally.candidates.load(std::sync::atomic::Ordering::Relaxed),
        tally.admitted.load(std::sync::atomic::Ordering::Relaxed),
    );

    let census = relabel_edge_census(&context, &cases, relabel);
    println!(
        "L-lenses-8 relabel carrier edge relabels by (shape, lens): (admitted, refused, refusal variants): {census:?}"
    );
    let variants = |variant: &str| BTreeSet::from([variant.to_owned()]);
    let contracted = "ContractedEdgeInserted";
    let unrepresentable = "UnrepresentableViewEdge";
    assert_eq!(
        census,
        BTreeMap::from([
            (("graph", 0), (272, 0, BTreeSet::new())),
            (("graph", 1), (0, 48, variants(contracted))),
            (("graph", 2), (544, 272, variants(unrepresentable))),
            (("table", 0), (2, 0, BTreeSet::new())),
            (("table", 1), (0, 3, variants(contracted))),
            (("tree", 0), (16, 0, BTreeSet::new())),
            (("tree", 1), (0, 8, variants(contracted))),
            (("tree", 2), (16, 8, variants(unrepresentable))),
        ])
    );
    assert_eq!(
        common_tally.snapshot()[relabel],
        census
            .values()
            .map(|(admitted, refused, _)| admitted + refused)
            .sum::<usize>(),
        "the five-gate tally must count every edge-relabel candidate of the common basis"
    );
}

// ---------------------------------------------------------------------------
// The property differential: random cases from every exhaustive family
// ---------------------------------------------------------------------------

/// The amended bounded lens census has passed its independent count and dynamic gates.
const BOUNDED_LENS_ACCEPTED: bool = true;

fn delta_fragment_cases() -> &'static [(DeltaFragmentFixture, RefGraph, RefGraph)] {
    static CASES: std::sync::OnceLock<Vec<(DeltaFragmentFixture, RefGraph, RefGraph)>> =
        std::sync::OnceLock::new();
    CASES.get_or_init(|| {
        let mut cases = Vec::new();
        for_each_delta_fragment_deletion_edit(|fixture, source, edited| {
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

type BoundedLensCases = (
    Vec<BoundedLensStage>,
    Vec<(usize, RefGraph)>,
    Vec<(usize, RefTree)>,
);

fn bounded_lens_cases() -> &'static BoundedLensCases {
    static CASES: std::sync::OnceLock<BoundedLensCases> = std::sync::OnceLock::new();
    CASES.get_or_init(|| {
        let stages = bounded_lens_stages();
        let mut graphs = Vec::new();
        let mut trees = Vec::new();
        for (index, stage) in stages.iter().enumerate() {
            graphs.extend(bounded_lens_sources(stage).into_iter().map(|g| (index, g)));
            if stage.source != bounded_lens_fan_schema() {
                trees.extend(
                    bounded_lens_tree_states(&stage.source)
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

fn delta_fragment_property_case(index: usize) -> Result<(), String> {
    let cases = delta_fragment_cases();
    let (fixture, source, edited) = &cases[index % cases.len()];
    let pair = delta_fragment_pair(*fixture);
    property_differ(
        "Delta-fragment put",
        format!("{:?}", production_put(&pair, source, edited)),
        format!("{:?}", reference_put(&pair, source, edited)),
    )?;
    let (production, largest, universal) = delta_fragment_cascade_outcome(&pair, source, edited);
    property_differ(
        "Delta-fragment cascade",
        format!(
            "{:?}",
            (production.and_then(|outcome| outcome.put.ok()), universal)
        ),
        format!("{:?}", (largest, true)),
    )
}

type GraphDomainWitness = (GraphPutDomainFamily, BoundedLensStage, RefGraph, RefGraph);

/// One reusable witness of each put-domain family that has no dedicated
/// construction, found once per test process.
///
/// The structural families come from the bounded lens carrier, in stage,
/// state and edit order. `DerivedCellEdited` comes from the declared-attribute
/// transform carrier: the bounded carrier's value-layer stages write an
/// undeclared field and refuse at compilation, so their edits never reach a
/// derived cell. The search result is cached even when a family has no
/// witness, so the missing family is reported at once by every caller
/// instead of being searched for again.
fn bounded_lens_graph_domain_witnesses() -> &'static [GraphDomainWitness] {
    static WITNESSES: std::sync::OnceLock<Result<Vec<GraphDomainWitness>, String>> =
        std::sync::OnceLock::new();
    let found = WITNESSES.get_or_init(|| {
        let focused = [
            GraphPutDomainFamily::SurvivalPredicateFails,
            GraphPutDomainFamily::SurvivalPredicateRefused,
            GraphPutDomainFamily::TransformNotInvertible,
            GraphPutDomainFamily::DuplicateKey,
            GraphPutDomainFamily::MissingRequiredCell,
            GraphPutDomainFamily::MalformedFan,
        ]
        .into_iter()
        .fold(0_u16, |bits, family| {
            bits | graph_put_domain_family_bit(family.name())
        });
        let declared = graph_put_domain_family_bit(GraphPutDomainFamily::DerivedCellEdited.name());
        let structural = GraphPutDomainFamily::ALL
            .into_iter()
            .fold(0_u16, |bits, family| {
                bits | graph_put_domain_family_bit(family.name())
            })
            & !focused
            & !declared;
        let mut witnesses = Vec::new();
        let mut found = graph_domain_witnesses_on(
            &bounded_lens_stages(),
            bounded_lens_sources,
            structural,
            &mut witnesses,
        );
        found |= graph_domain_witnesses_on(
            &panproto_reference::enumerate::lenses::declared_transform_stages(),
            |_| panproto_reference::enumerate::lenses::declared_transform_graphs(),
            declared,
            &mut witnesses,
        );
        let wanted = structural | declared;
        if found == wanted {
            Ok(witnesses)
        } else {
            let missing = GraphPutDomainFamily::ALL
                .into_iter()
                .filter(|family| {
                    let bit = graph_put_domain_family_bit(family.name());
                    wanted & bit != 0 && found & bit == 0
                })
                .map(GraphPutDomainFamily::name)
                .collect::<Vec<_>>();
            Err(format!(
                "no reusable graph put-domain witness for {missing:?}: no one-step edit of a carrier view is refused with that family"
            ))
        }
    });
    match found {
        Ok(witnesses) => witnesses,
        Err(message) => panic!("{message}"),
    }
}

/// Push onto `witnesses` the first reference strict-put witness, over
/// `stages`, the states `sources` gives each stage and every one-step edit
/// of their views, of each family in `wanted`; return the families found.
fn graph_domain_witnesses_on(
    stages: &[BoundedLensStage],
    sources: impl Fn(&BoundedLensStage) -> Vec<RefGraph>,
    wanted: u16,
    witnesses: &mut Vec<GraphDomainWitness>,
) -> u16 {
    let mut found = 0_u16;
    for stage in stages {
        let pair = Pair::from_stage(stage);
        for source in sources(stage) {
            let Ok((view, complement)) = reference_get(&pair, &source) else {
                continue;
            };
            for edited in bounded_lens_graph_edits(&view, &stage.target, &complement)
                .into_iter()
                .map(|case| case.view)
            {
                let Some(PutOutcome {
                    domain: Err(reasons),
                    ..
                }) = reference_strict_put(&pair, &source, &edited)
                else {
                    continue;
                };
                for family in GraphPutDomainFamily::ALL {
                    let bit = graph_put_domain_family_bit(family.name());
                    if wanted & bit != 0
                        && found & bit == 0
                        && reasons
                            .iter()
                            .any(|reason| reason.starts_with(family.name()))
                    {
                        found |= bit;
                        witnesses.push((family, stage.clone(), source.clone(), edited.clone()));
                    }
                }
                if found == wanted {
                    return found;
                }
            }
        }
    }
    found
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
    let (stages, graphs, _) = bounded_lens_cases();
    let (stage, source) = &graphs[index % graphs.len()];
    let pair = Pair::from_stage(&stages[*stage]);
    property_differ(
        "bounded lens graph law report",
        format!(
            "{:?}",
            production_law_outcome(&graph_lens::check_laws(
                &pair.production,
                &prod_graph(source),
            ))
        ),
        format!("{:?}", reference_graph_law_case(&pair, source)),
    )?;
    property_differ(
        "bounded lens get",
        format!("{:?}", production_get(&pair, source)),
        format!("{:?}", reference_get(&pair, source)),
    )?;
    let Ok((view, complement)) = reference_get(&pair, source) else {
        return Ok(());
    };
    let mut views = vec![view.clone()];
    views.extend(
        bounded_lens_graph_edits(&view, &stages[*stage].target, &complement)
            .into_iter()
            .map(|candidate| candidate.view),
    );
    let edited = &views[edit % views.len()];
    property_differ(
        "bounded lens put",
        format!("{:?}", production_put(&pair, source, edited)),
        format!("{:?}", reference_strict_put(&pair, source, edited)),
    )
}

fn tree_property_case(index: usize) -> Result<(), String> {
    let (stages, _, trees) = bounded_lens_cases();
    let (stage, tree) = &trees[index % trees.len()];
    let pair = Pair::from_stage(&stages[*stage]);
    let reference = reference_tree_case(&pair, tree);
    let production_laws = reference
        .as_ref()
        .map_or_else(outside_the_tree_domain, |_| {
            production_law_outcome(
                &panproto_lens::check_laws_report::<panproto_lens::TreeShape>(
                    &pair.production,
                    &prod_tree(tree),
                    &(),
                ),
            )
        });
    let reference_laws = reference
        .as_ref()
        .map_or_else(outside_the_tree_domain, |_| {
            pair.with_reference(|lens| {
                lens.map_or_else(
                    |_| refused_law_outcome(),
                    |lens| {
                        reference_law_outcome(&panproto_reference::lens::check_tree_laws(
                            lens, tree,
                        ))
                    },
                )
            })
        });
    property_differ(
        "bounded lens tree law report",
        format!("{production_laws:?}"),
        format!("{reference_laws:?}"),
    )?;
    property_differ(
        "bounded lens tree",
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
    let (stages, _, _) = bounded_lens_cases();
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
    let sources = bounded_lens_sources(&stages[word[0]]);
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
    let states = bounded_lens_graph_states(schema);
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
        0 => delta_fragment_property_case(index),
        1 => table_property_case(index, edit),
        2 => partial_lens_property_case(index, edit),
        3 if BOUNDED_LENS_ACCEPTED => graph_property_case(index, edit),
        4 if BOUNDED_LENS_ACCEPTED => tree_property_case(index),
        5 if BOUNDED_LENS_ACCEPTED => path_property_case(index, edit),
        6 => table_writer_property_case(index, edit),
        7 => table_square_property_case(index),
        8 if BOUNDED_LENS_ACCEPTED => protolens_instance_property_case(index, edit),
        _ => Ok(()),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::default())]

    #[test]
    fn randomized_graph_put_law_differential(
        floor in 0_u16..=4_000,
        taken in proptest::collection::vec(0_u16..=4_000, 0..8),
        dropped_nodes in proptest::collection::vec(0_u16..=4_000, 0..8),
        dropped_edges in proptest::collection::vec(0_u16..=4_000, 0..8),
        synthesized_nodes in proptest::collection::vec(0_u16..=4_000, 0..8),
        synthesized_edges in proptest::collection::vec(0_u16..=4_000, 0..8),
        below_delta in any::<u16>(),
        above_delta in 1_u16..=4_000,
        missing_suffix in "[a-z0-9]{1,12}",
        carrier in any::<usize>(),
        edit in any::<usize>(),
    ) {
        assert_randomized_graph_put_contract(
            u32::from(floor),
            taken.into_iter().map(u32::from).collect::<BTreeSet<_>>(),
            dropped_nodes
                .into_iter()
                .map(u32::from)
                .collect::<BTreeSet<_>>(),
            dropped_edges
                .into_iter()
                .map(u32::from)
                .collect::<BTreeSet<_>>(),
            synthesized_nodes
                .into_iter()
                .map(u32::from)
                .collect::<BTreeSet<_>>(),
            synthesized_edges
                .into_iter()
                .map(u32::from)
                .collect::<BTreeSet<_>>(),
            below_delta,
            above_delta,
            &missing_suffix,
        );
        if let Err(message) = graph_property_case(carrier, edit) {
            prop_assert!(false, "{}", message);
        }
    }

    #[test]
    fn randomized_graph_put_domain_and_cascade_differential(
        carrier in any::<usize>(),
        edit in any::<usize>(),
    ) {
        if let Err(message) = delta_fragment_property_case(carrier) {
            prop_assert!(false, "{message}");
        }
        if let Err(message) = graph_property_case(carrier, edit) {
            prop_assert!(false, "{message}");
        }
        let observed = GraphPutDomainFamily::ALL
            .into_iter()
            .fold(0_u16, |bits, family| {
                bits | assert_graph_put_domain_family(family)
            });
        let expected = GraphPutDomainFamily::ALL
            .into_iter()
            .fold(0_u16, |bits, family| {
                bits | graph_put_domain_family_bit(family.name())
            });
        prop_assert_eq!(observed, expected);
    }

    #[test]
    fn randomized_tree_put_law_differential(
        pair in (any::<i16>(), any::<i16>())
            .prop_filter("source keys differ", |(left, right)| left != right),
        id_seed in 0_u16..=10_000,
        required_field in prop_oneof![Just("b"), Just("child")],
        malformed_x in any::<bool>(),
        carrier in any::<usize>(),
        edit in any::<usize>(),
    ) {
        let (left, right) = pair;
        assert_randomized_tree_validation(
            i64::from(left),
            i64::from(right),
            u32::from(id_seed) * 4,
            required_field,
            if malformed_x { "x" } else { "y" },
        );
        if let Err(message) = tree_property_case(carrier) {
            prop_assert!(false, "{}", message);
        }
        assert_randomized_graph_square_case(carrier, edit);
    }

    #[test]
    fn randomized_lens_law_differential(
        additional_family in 0_u8..5,
        index in any::<usize>(),
        edit in any::<usize>(),
    ) {
        for (offset, family) in [1_u8, 2, 3, 4].into_iter().enumerate() {
            if let Err(message) = property_case(
                family,
                index.rotate_left(u32::try_from(offset * 7).unwrap()),
                edit.rotate_right(u32::try_from(offset * 5).unwrap()),
            ) {
                prop_assert!(false, "L-lenses-8 family {family}: {message}");
            }
        }
        let family = [0_u8, 5, 6, 7, 8][usize::from(additional_family)];
        if let Err(message) = property_case(family, index, edit) {
            prop_assert!(false, "additional family {family}: {message}");
        }
    }
}

/// The stage-C slice of the exhaustive law comparison on trees: on every bounded lens
/// tree case of the contraction stage, both sides report the same five-gate
/// outcome over the common basis. The chain `a -> b -> a` contracts to one
/// view arc with a synthesized identity; removing the arc's child from the
/// view leaves that identity naming nothing, and strict `PutGet` must compare
/// only the synthesized identities of elements the views hold.
#[test]
fn stage_c_tree_common_basis_laws_agree_with_the_reference() {
    let stages = bounded_lens_stages();
    let stage_c = bounded_lens_stage_index(&stages, "C");
    let pair = Pair::from_stage(&stages[stage_c]);
    let mut cases = Vec::new();
    for_each_lens_law_case(|case| {
        if let LensLawCertificateCase::Concrete(LensEquivalenceCase::Tree { stage, source }) = case
            && stage == stage_c
        {
            cases.push(source);
        }
    });
    let chain = |values: [Option<i64>; 3]| {
        let node = |id: u32, anchor: &str, value: Option<i64>| {
            let mut node = RefNode::new(id, anchor);
            node.value = value.map(|value| {
                panproto_reference::value::Presence::Present(panproto_reference::value::Value::Int(
                    value,
                ))
            });
            (id, node)
        };
        RefTree {
            nodes: BTreeMap::from([
                node(0, "a", values[0]),
                node(1, "b", values[1]),
                node(2, "a", values[2]),
            ]),
            arcs: vec![
                (0, 1, RefSEdge::new("a", "b", "prop", Some("e"))),
                (1, 2, RefSEdge::new("b", "a", "prop", Some("e"))),
            ],
            fans: Vec::new(),
            root: 0,
        }
    };
    for pinned in [chain([None, None, None]), chain([Some(0), None, None])] {
        assert!(cases.contains(&pinned), "{pinned:?} is a stage-C case");
    }
    let mut compared = 0;
    for tree in &cases {
        if reference_tree_case(&pair, tree).is_none() {
            continue;
        }
        let candidates = tree_candidates(&pair, tree);
        let production = tree_common_laws(&pair, tree, &candidates, true);
        let reference = tree_common_laws(&pair, tree, &candidates, false);
        assert_eq!(production, reference, "{tree:?}");
        compared += 1;
    }
    println!("stage C trees {}, compared {compared}", cases.len());
    assert!(compared > 0);
}

/// A cell inserted at a column the source table lacks has no writer, so the
/// put domain reports it as derived before any value layer is consulted,
/// even when the row's transform drops that same field.
#[test]
fn writerless_cell_under_a_dropping_transform_is_derived() {
    use panproto_reference::enumerate::lenses::bounded_lens_stages;

    let stage = bounded_lens_stages()
        .into_iter()
        .find(|stage| stage.layer == BoundedLensLayer::DropField)
        .expect("the stage alphabet includes the dropping transform");
    let pair = TablePair::from_stage(&stage);
    let source = RefTable {
        rows: BTreeMap::from([(0, ("a".to_owned(), BTreeMap::new()))]),
        pairs: BTreeSet::new(),
    };
    let mut edited = source.clone();
    edited
        .rows
        .get_mut(&0)
        .expect("row zero")
        .1
        .insert(BOUNDED_LENS_FIELD.to_owned(), BOUNDED_LENS_VALUE);
    let reference = reference_table_put(&pair, &source, &edited);
    assert_eq!(
        reference.domain,
        Err(vec![format!(
            "DerivedCellEdited Row(0) {BOUNDED_LENS_FIELD}"
        )])
    );
    assert_eq!(production_table_put(&pair, &source, &edited), reference);
}

// ---------------------------------------------------------------------------
// Transformed tables: put through a value layer agrees with the reference on
// derived transform outputs, in-place inverses and the inserted-row floor
// ---------------------------------------------------------------------------

mod transformed_tables {
    use super::*;
    use panproto_expr::{BuiltinOp, Expr, Literal};
    use panproto_gat::CoercionClass;
    use panproto_inst::FieldTransform;

    fn schema(columns: &[&str]) -> RefSchema {
        let mut schema = RefSchema::default().vertex("t", TABLE_FIXTURE_TABLE_KIND);
        for column in columns {
            schema = schema
                .vertex(&format!("t.{column}"), WRITER_VALUE_KIND)
                .edge("t", &format!("t.{column}"), "prop", Some(column));
        }
        schema
    }

    fn column(name: &str) -> Edge {
        Edge {
            src: "t".into(),
            tgt: format!("t.{name}").into(),
            kind: "prop".into(),
            name: Some(name.into()),
        }
    }

    /// The table pair of a production migration whose transforms the
    /// reference sees through `layer`.
    fn pair(
        source: &[&str],
        target: &[&str],
        compiled: CompiledMigration,
        layer: &'static dyn ValueLayer,
    ) -> TablePair {
        let migration = ref_migration(&compiled);
        let mut structural = migration.clone();
        structural.conditional.clear();
        structural.transformed.clear();
        structural.output_fields.clear();
        let mut pair = TablePair::new(
            schema(source),
            schema(target),
            structural,
            WRITER_VALUE_KIND,
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        );
        pair.plan = panproto_reference::table::compile_for_tables(
            &pair.src,
            &pair.tgt,
            &pair.boundaries.0,
            &pair.boundaries.1,
            &migration,
        );
        pair.migration = migration;
        pair.production = Lens {
            compiled,
            src_schema: prod_schema(&schema(source)),
            tgt_schema: prod_schema(&schema(target)),
        };
        pair.layer = layer;
        pair
    }

    fn one_row(cells: &[(&str, i64)]) -> RefTable {
        let mut table = RefTable::default();
        table.rows.insert(
            0,
            (
                "t".to_owned(),
                cells
                    .iter()
                    .map(|(key, value)| ((*key).to_owned(), RefValue::Int(*value)))
                    .collect(),
            ),
        );
        table
    }

    fn agreed_put(pair: &TablePair, source: &RefTable, view: &RefTable) -> TablePutOutcome {
        let production = production_table_put(pair, source, view);
        let reference = reference_table_put(pair, source, view);
        assert_eq!(production, reference);
        production
    }

    const fn layer_semantics(domain: &'static str) -> ValueLayerSemantics {
        ValueLayerSemantics {
            domain,
            version: 1,
            canonical_payload: Vec::new(),
        }
    }

    fn int(fields: &BTreeMap<String, RefValue>, key: &str) -> Option<i64> {
        match fields.get(key) {
            Some(RefValue::Int(value)) => Some(*value),
            _ => None,
        }
    }

    /// `RenameField old -> new` on table `t`.
    struct RenameOldToNew;

    impl ValueLayer for RenameOldToNew {
        fn canonical_semantics(&self) -> ValueLayerSemantics {
            layer_semantics("panproto-lens/table-test/rename-old-new")
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
            if let Some(value) = fields.remove("old") {
                fields.insert("new".to_owned(), value);
            }
            Ok((fields, node.value.clone()))
        }

        fn restore(
            &self,
            view: &RefNode,
            _: Option<&BTreeMap<String, RefValue>>,
            _: Option<&Option<RefPresence>>,
        ) -> Result<(BTreeMap<String, RefValue>, Option<RefPresence>), PutFieldRefusal> {
            let mut fields = view.fields.clone();
            if let Some(value) = fields.remove("new") {
                fields.insert("old".to_owned(), value);
            }
            Ok((fields, view.value.clone()))
        }
    }

    /// `ComputeField a := a + 1` with inverse `a - 1` on table `t`.
    struct Successor;

    impl ValueLayer for Successor {
        fn canonical_semantics(&self) -> ValueLayerSemantics {
            layer_semantics("panproto-lens/table-test/successor")
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
            let Some(value) = int(&fields, "a") else {
                return Err(LayerRefusal {
                    kind: "expression".to_owned(),
                    detail: "unbound variable: a".to_owned(),
                });
            };
            fields.insert("a".to_owned(), RefValue::Int(value + 1));
            Ok((fields, node.value.clone()))
        }

        fn restore(
            &self,
            view: &RefNode,
            _: Option<&BTreeMap<String, RefValue>>,
            _: Option<&Option<RefPresence>>,
        ) -> Result<(BTreeMap<String, RefValue>, Option<RefPresence>), PutFieldRefusal> {
            let mut fields = view.fields.clone();
            if let Some(value) = int(&fields, "a") {
                fields.insert("a".to_owned(), RefValue::Int(value - 1));
            }
            Ok((fields, view.value.clone()))
        }
    }

    /// `ComputeField b := a + 1` with inverse `b - 1`, then `DropField a`,
    /// on table `t`.
    struct SuccessorIntoB;

    impl ValueLayer for SuccessorIntoB {
        fn canonical_semantics(&self) -> ValueLayerSemantics {
            layer_semantics("panproto-lens/table-test/successor-into-b")
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
            let Some(value) = int(&fields, "a") else {
                return Err(LayerRefusal {
                    kind: "expression".to_owned(),
                    detail: "unbound variable: a".to_owned(),
                });
            };
            fields.insert("b".to_owned(), RefValue::Int(value + 1));
            fields.remove("a");
            Ok((fields, node.value.clone()))
        }

        fn restore(
            &self,
            view: &RefNode,
            _: Option<&BTreeMap<String, RefValue>>,
            _: Option<&Option<RefPresence>>,
        ) -> Result<(BTreeMap<String, RefValue>, Option<RefPresence>), PutFieldRefusal> {
            let mut fields = view.fields.clone();
            if let Some(value) = int(&fields, "b") {
                fields.remove("b");
                fields.insert("a".to_owned(), RefValue::Int(value - 1));
            }
            Ok((fields, view.value.clone()))
        }
    }

    /// `ComputeField b := a + 1` with inverse `b - 1` on table `t`, keeping
    /// `a`.
    struct SuccessorBesideA;

    impl ValueLayer for SuccessorBesideA {
        fn canonical_semantics(&self) -> ValueLayerSemantics {
            layer_semantics("panproto-lens/table-test/successor-beside-a")
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
            let Some(value) = int(&fields, "a") else {
                return Err(LayerRefusal {
                    kind: "expression".to_owned(),
                    detail: "unbound variable: a".to_owned(),
                });
            };
            fields.insert("b".to_owned(), RefValue::Int(value + 1));
            Ok((fields, node.value.clone()))
        }

        fn restore(
            &self,
            view: &RefNode,
            _: Option<&BTreeMap<String, RefValue>>,
            _: Option<&Option<RefPresence>>,
        ) -> Result<(BTreeMap<String, RefValue>, Option<RefPresence>), PutFieldRefusal> {
            let mut fields = view.fields.clone();
            if let Some(value) = int(&fields, "b") {
                fields.remove("b");
                fields.insert("a".to_owned(), RefValue::Int(value - 1));
            }
            Ok((fields, view.value.clone()))
        }
    }

    /// `ApplyExpr a := 7` with the non-inverting inverse `8`: a changed
    /// coordinate goes through the inverse, an unchanged one keeps its
    /// recorded original.
    struct ConstantSeven;

    impl ValueLayer for ConstantSeven {
        fn canonical_semantics(&self) -> ValueLayerSemantics {
            layer_semantics("panproto-lens/table-test/constant-seven")
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
            if fields.contains_key("a") {
                fields.insert("a".to_owned(), RefValue::Int(7));
            }
            Ok((fields, node.value.clone()))
        }

        fn restore(
            &self,
            view: &RefNode,
            original: Option<&BTreeMap<String, RefValue>>,
            _: Option<&Option<RefPresence>>,
        ) -> Result<(BTreeMap<String, RefValue>, Option<RefPresence>), PutFieldRefusal> {
            let mut fields = view.fields.clone();
            if fields.contains_key("a") {
                let kept = original
                    .and_then(|original| original.get("a"))
                    .filter(|_| int(&fields, "a") == Some(7))
                    .cloned();
                fields.insert("a".to_owned(), kept.unwrap_or(RefValue::Int(8)));
            }
            Ok((fields, view.value.clone()))
        }
    }

    #[test]
    fn a_transform_only_rename_output_is_a_derived_cell() {
        let mut compiled = CompiledMigration {
            surviving_verts: std::iter::once("t".into()).collect(),
            ..CompiledMigration::default()
        };
        compiled.field_transforms.insert(
            "t".into(),
            vec![FieldTransform::RenameField {
                old_key: "old".to_owned(),
                new_key: "new".to_owned(),
            }],
        );
        let pair = pair(&["old"], &["new"], compiled, &RenameOldToNew);
        let source = one_row(&[("old", 1)]);
        assert_eq!(
            agreed_put(&pair, &source, &one_row(&[("new", 1)])).put,
            Ok(source.clone())
        );
        assert_eq!(
            agreed_put(&pair, &source, &one_row(&[("new", 2)])).domain,
            Err(vec!["DerivedCellEdited Row(0) new".to_owned()])
        );
        assert!(!table_lens::is_isomorphism(
            &pair.production,
            pair.protocols()
        ));
    }

    #[test]
    fn an_edited_in_place_computation_pulls_back_through_its_inverse() {
        let mut compiled = CompiledMigration {
            surviving_verts: ["t", "t.a"].into_iter().map(Into::into).collect(),
            surviving_edges: [column("a")].into(),
            ..CompiledMigration::default()
        };
        let step = |op, operand: &str| {
            Expr::Builtin(
                op,
                vec![Expr::Var(operand.into()), Expr::Lit(Literal::Int(1))],
            )
        };
        compiled.field_transforms.insert(
            "t".into(),
            vec![FieldTransform::ComputeField {
                target_key: "a".to_owned(),
                expr: step(BuiltinOp::Add, "a"),
                inverse: Some(step(BuiltinOp::Sub, "a")),
                coercion_class: CoercionClass::Iso,
            }],
        );
        let pair = pair(&["a"], &["a"], compiled, &Successor);
        let source = one_row(&[("a", 1)]);
        assert_eq!(
            agreed_put(&pair, &source, &one_row(&[("a", 2)])).put,
            Ok(source)
        );
        assert_eq!(
            agreed_put(&pair, &one_row(&[("a", 1)]), &one_row(&[("a", 5)])).put,
            Ok(one_row(&[("a", 4)]))
        );
    }

    /// A declared transform's output at a target column is that column's
    /// value: the rename `a -> b` places the source cell at `b`, and the
    /// computed `b := a + 1` then overwrites it.
    #[test]
    fn a_transform_output_overwrites_the_renamed_cell_at_its_column() {
        let mut compiled = CompiledMigration {
            surviving_verts: ["t", "t.b"].into_iter().map(Into::into).collect(),
            surviving_edges: [column("b")].into(),
            vertex_remap: HashMap::from([("t.a".into(), "t.b".into())]),
            edge_remap: HashMap::from([(column("a"), column("b"))]),
            ..CompiledMigration::default()
        };
        let step = |op, operand: &str| {
            Expr::Builtin(
                op,
                vec![Expr::Var(operand.into()), Expr::Lit(Literal::Int(1))],
            )
        };
        compiled.field_transforms.insert(
            "t".into(),
            vec![
                FieldTransform::ComputeField {
                    target_key: "b".to_owned(),
                    expr: step(BuiltinOp::Add, "a"),
                    inverse: Some(step(BuiltinOp::Sub, "b")),
                    coercion_class: CoercionClass::Iso,
                },
                FieldTransform::DropField {
                    key: "a".to_owned(),
                },
            ],
        );
        let pair = pair(&["a"], &["b"], compiled, &SuccessorIntoB);
        let source = one_row(&[("a", 1)]);
        let production = production_table_get(&pair, &source);
        let reference = reference_table_get(&pair, &source);
        assert_eq!(
            reference.as_ref().map(|(view, _)| view),
            Ok(&one_row(&[("b", 2)]))
        );
        assert_eq!(production, reference);
    }

    /// The output wins without a drop of its renamed source: the rename
    /// `a -> b` moves `a`'s cell onto `b`, and the computed `b := a + 1`,
    /// evaluated on the source row, then overwrites it.
    #[test]
    fn a_transform_output_overwrites_a_renamed_cell_it_does_not_drop() {
        let mut compiled = CompiledMigration {
            surviving_verts: ["t", "t.b"].into_iter().map(Into::into).collect(),
            surviving_edges: [column("b")].into(),
            vertex_remap: HashMap::from([("t.a".into(), "t.b".into())]),
            edge_remap: HashMap::from([(column("a"), column("b"))]),
            ..CompiledMigration::default()
        };
        let step = |op, operand: &str| {
            Expr::Builtin(
                op,
                vec![Expr::Var(operand.into()), Expr::Lit(Literal::Int(1))],
            )
        };
        compiled.field_transforms.insert(
            "t".into(),
            vec![FieldTransform::ComputeField {
                target_key: "b".to_owned(),
                expr: step(BuiltinOp::Add, "a"),
                inverse: Some(step(BuiltinOp::Sub, "b")),
                coercion_class: CoercionClass::Iso,
            }],
        );
        let pair = pair(&["a"], &["b"], compiled, &SuccessorBesideA);
        let source = one_row(&[("a", 1)]);
        let production = production_table_get(&pair, &source);
        let reference = reference_table_get(&pair, &source);
        assert_eq!(
            reference.as_ref().map(|(view, _)| view),
            Ok(&one_row(&[("b", 2)]))
        );
        assert_eq!(production, reference);
        // Put compares a view against the same placement, so the unedited
        // view restores the source.
        assert_eq!(
            agreed_put(&pair, &source, &one_row(&[("b", 2)])).put,
            Ok(source.clone())
        );
        // Both engines refuse an edit of the output column `b` alike: the
        // rename writes it back to the source column `a`, and the computed
        // `b` of the restored source does not reproduce it.
        let edited = one_row(&[("b", 5)]);
        let refusal = "TransformNotInvertible Row(0) b".to_owned();
        let reference = reference_table_put(&pair, &source, &edited);
        assert_eq!(reference.domain, Err(vec![refusal.clone()]));
        assert_eq!(reference.put, Err(Reason::PutDomain(refusal)));
        assert_eq!(production_table_put(&pair, &source, &edited), reference);
    }

    /// The graph engine applies no cell fates to a node's fields: under the
    /// rename `a -> b` and the computed `b := a + 1`, the view node keeps the
    /// source field `a` beside the computed `b`, in production as in the
    /// reference.
    #[test]
    fn a_graph_transform_output_sits_beside_the_field_a_rename_does_not_move() {
        let mut compiled = CompiledMigration {
            surviving_verts: ["t", "t.b"].into_iter().map(Into::into).collect(),
            surviving_edges: [column("b")].into(),
            vertex_remap: HashMap::from([("t.a".into(), "t.b".into())]),
            edge_remap: HashMap::from([(column("a"), column("b"))]),
            ..CompiledMigration::default()
        };
        let step = |op, operand: &str| {
            Expr::Builtin(
                op,
                vec![Expr::Var(operand.into()), Expr::Lit(Literal::Int(1))],
            )
        };
        compiled.field_transforms.insert(
            "t".into(),
            vec![FieldTransform::ComputeField {
                target_key: "b".to_owned(),
                expr: step(BuiltinOp::Add, "a"),
                inverse: Some(step(BuiltinOp::Sub, "b")),
                coercion_class: CoercionClass::Iso,
            }],
        );
        let migration = ref_migration(&compiled);
        let production = Lens {
            compiled,
            src_schema: prod_schema(&schema(&["a"])),
            tgt_schema: prod_schema(&schema(&["b"])),
        };
        let pair = Pair::with_production(
            schema(&["a"]),
            schema(&["b"]),
            migration,
            &SuccessorBesideA,
            production,
        );
        let mut node = RefNode::new(0, "t");
        node.fields.insert("a".to_owned(), RefValue::Int(1));
        let source = RefGraph {
            nodes: BTreeMap::from([(0, node)]),
            ..RefGraph::default()
        };
        let production = production_get(&pair, &source);
        let reference = reference_get(&pair, &source);
        assert_eq!(
            reference
                .as_ref()
                .map(|(view, _)| view.nodes[&0].fields.clone()),
            Ok(BTreeMap::from([
                ("a".to_owned(), RefValue::Int(1)),
                ("b".to_owned(), RefValue::Int(2)),
            ]))
        );
        assert_eq!(production, reference);
    }

    #[test]
    fn the_id_floor_separates_inserted_rows_from_source_rows() {
        let mut compiled = CompiledMigration {
            surviving_verts: ["t", "t.a"].into_iter().map(Into::into).collect(),
            surviving_edges: [column("a")].into(),
            ..CompiledMigration::default()
        };
        compiled.field_transforms.insert(
            "t".into(),
            vec![FieldTransform::ApplyExpr {
                key: "a".to_owned(),
                expr: Expr::Lit(Literal::Int(7)),
                inverse: Some(Expr::Lit(Literal::Int(8))),
                coercion_class: CoercionClass::Projection,
            }],
        );
        let pair = pair(&["a"], &["a"], compiled, &ConstantSeven);
        let view = one_row(&[("a", 7)]);
        // From an empty source the floor is 0, so row 0 is inserted and its
        // cell is pulled back through the inverse.
        assert_eq!(
            agreed_put(&pair, &RefTable::default(), &view).put,
            Ok(one_row(&[("a", 8)]))
        );
        // From the one-row source the floor is 1, so row 0 is a source row
        // and its unedited view restores the source.
        assert_eq!(agreed_put(&pair, &view, &view).put, Ok(view.clone()));
    }
}

// ---------------------------------------------------------------------------
// Graph edit lenses (L-edits-9)
// ---------------------------------------------------------------------------

mod graph_edit_lens_differential {
    use panproto_inst::GraphDelta;
    use panproto_inst::graph_edit::{AttrChange, AttrKey, AttrVal, GraphDeltaParts, NodeMeta};
    use panproto_lens::{GraphEditLens, ShapeEditError};
    use panproto_reference::edit::{
        self as ref_edit, GraphDelta as RefGraphDelta, GraphEditError, GraphEditState,
    };
    use panproto_reference::enumerate::edits::{
        graph_edit_lens_letter_count, graph_edit_lens_letters,
    };
    use panproto_reference::enumerate::lenses::bounded_lens_stages;

    use super::*;

    fn ref_meta(meta: &NodeMeta) -> ref_edit::NodeMeta {
        ref_edit::NodeMeta {
            discriminator: meta.discriminator.as_deref().map(str::to_owned),
            position: meta.position,
            shape: ref_shape(&meta.shape),
            annotations: meta
                .annotations
                .iter()
                .map(|(name, value)| (name.clone(), ref_value(value)))
                .collect(),
        }
    }

    fn prod_meta(meta: &ref_edit::NodeMeta) -> NodeMeta {
        NodeMeta {
            discriminator: meta.discriminator.as_deref().map(Into::into),
            position: meta.position,
            shape: prod_shape(&meta.shape),
            annotations: meta
                .annotations
                .iter()
                .map(|(name, value)| (name.clone(), prod_value(value)))
                .collect(),
        }
    }

    fn ref_attr_key(key: &AttrKey) -> ref_edit::AttrKey {
        match key {
            AttrKey::Value { node } => ref_edit::AttrKey::Value(*node),
            AttrKey::Field { node, field } => ref_edit::AttrKey::Field(*node, field.clone()),
            AttrKey::Meta { node } => ref_edit::AttrKey::Meta(*node),
            AttrKey::Anchor { node } => ref_edit::AttrKey::Anchor(*node),
            AttrKey::EdgeLabel { edge } => ref_edit::AttrKey::EdgeLabel(edge.0),
            AttrKey::EdgePosition { edge } => ref_edit::AttrKey::EdgePosition(edge.0),
            other => panic!("table key in a graph delta: {other:?}"),
        }
    }

    fn prod_attr_key(key: &ref_edit::AttrKey) -> AttrKey {
        match key {
            ref_edit::AttrKey::Value(node) => AttrKey::Value { node: *node },
            ref_edit::AttrKey::Field(node, field) => AttrKey::Field {
                node: *node,
                field: field.clone(),
            },
            ref_edit::AttrKey::Meta(node) => AttrKey::Meta { node: *node },
            ref_edit::AttrKey::Anchor(node) => AttrKey::Anchor { node: *node },
            ref_edit::AttrKey::EdgeLabel(edge) => AttrKey::EdgeLabel {
                edge: EdgeId(*edge),
            },
            ref_edit::AttrKey::EdgePosition(edge) => AttrKey::EdgePosition {
                edge: EdgeId(*edge),
            },
        }
    }

    fn ref_attr_val(value: &AttrVal) -> ref_edit::AttrVal {
        match value {
            AttrVal::Presence(value) => {
                ref_edit::AttrVal::Presence(value.as_ref().map(ref_presence))
            }
            AttrVal::Field(value) => ref_edit::AttrVal::Field(value.as_ref().map(ref_value)),
            AttrVal::Meta(value) => ref_edit::AttrVal::Meta(ref_meta(value)),
            AttrVal::Anchor(value) => ref_edit::AttrVal::Anchor(value.to_string()),
            AttrVal::Label(value) => ref_edit::AttrVal::Label(ref_edge(value)),
            AttrVal::Position(value) => ref_edit::AttrVal::Position(*value),
        }
    }

    fn prod_attr_val(value: &ref_edit::AttrVal) -> AttrVal {
        match value {
            ref_edit::AttrVal::Presence(value) => {
                AttrVal::Presence(value.as_ref().map(prod_presence))
            }
            ref_edit::AttrVal::Field(value) => AttrVal::Field(value.as_ref().map(prod_value)),
            ref_edit::AttrVal::Meta(value) => AttrVal::Meta(prod_meta(value)),
            ref_edit::AttrVal::Anchor(value) => AttrVal::Anchor(value.as_str().into()),
            ref_edit::AttrVal::Label(value) => AttrVal::Label(prod_edge(value)),
            ref_edit::AttrVal::Position(value) => AttrVal::Position(*value),
        }
    }

    pub fn prod_delta(delta: &RefGraphDelta) -> GraphDelta {
        let mut parts = GraphDeltaParts::default();
        for (element, payload) in &delta.del {
            match (element, payload) {
                (ref_edit::GElem::Node(id), ref_edit::GPayload::Node(node)) => {
                    parts.del_nodes.insert(*id, prod_node(node));
                }
                (ref_edit::GElem::Edge(id), ref_edit::GPayload::Edge(edge)) => {
                    parts.del_edges.insert(EdgeId(*id), prod_gedge(edge));
                }
                other => panic!("cross-sort reference deletion: {other:?}"),
            }
        }
        for (element, payload) in &delta.add {
            match (element, payload) {
                (ref_edit::GElem::Node(id), ref_edit::GPayload::Node(node)) => {
                    parts.add_nodes.insert(*id, prod_node(node));
                }
                (ref_edit::GElem::Edge(id), ref_edit::GPayload::Edge(edge)) => {
                    parts.add_edges.insert(EdgeId(*id), prod_gedge(edge));
                }
                other => panic!("cross-sort reference addition: {other:?}"),
            }
        }
        parts.attrs = delta
            .attrs
            .iter()
            .map(|(key, (old, new))| {
                (
                    prod_attr_key(key),
                    AttrChange {
                        old: prod_attr_val(old),
                        new: prod_attr_val(new),
                    },
                )
            })
            .collect();
        GraphDelta::normalize(parts).expect("a carrier letter is normal")
    }

    pub fn ref_delta(delta: &GraphDelta) -> RefGraphDelta {
        let mut out = RefGraphDelta::identity();
        for (id, node) in delta.del_nodes() {
            out.del.insert(
                ref_edit::GElem::Node(*id),
                ref_edit::GPayload::Node(ref_node(node)),
            );
        }
        for (id, edge) in delta.del_edges() {
            out.del.insert(
                ref_edit::GElem::Edge(id.0),
                ref_edit::GPayload::Edge(ref_gedge(edge)),
            );
        }
        for (id, node) in delta.add_nodes() {
            out.add.insert(
                ref_edit::GElem::Node(*id),
                ref_edit::GPayload::Node(ref_node(node)),
            );
        }
        for (id, edge) in delta.add_edges() {
            out.add.insert(
                ref_edit::GElem::Edge(id.0),
                ref_edit::GPayload::Edge(ref_gedge(edge)),
            );
        }
        out.attrs = delta
            .attrs()
            .iter()
            .map(|(key, change)| {
                (
                    ref_attr_key(key),
                    (ref_attr_val(&change.old), ref_attr_val(&change.new)),
                )
            })
            .collect();
        out
    }

    /// The observable outcome of one edit: the translated delta with the
    /// next source, view and complement, or a refusal by class and reason.
    #[derive(Debug, PartialEq)]
    enum Outcome {
        Admitted(Box<Admitted>),
        Refused(String),
    }

    #[derive(Debug, PartialEq)]
    struct Admitted {
        delta: RefGraphDelta,
        source: RefGraph,
        view: RefGraph,
        complement: RefComplement,
    }

    fn production_refusal(error: &ShapeEditError) -> String {
        match error {
            ShapeEditError::Apply(_) => "apply".to_owned(),
            ShapeEditError::Get(error) => format!("get {:?}", lens_reason(error)),
            ShapeEditError::Put(error) => format!("put {:?}", lens_reason(error)),
            ShapeEditError::IdsExhausted { .. } => "ids exhausted".to_owned(),
            ShapeEditError::PutGetMismatch => "put-get mismatch".to_owned(),
            other => format!("production refusal outside the carrier: {other:?}"),
        }
    }

    fn reference_refusal(error: &GraphEditError<RefRestrictError, RefLensError>) -> String {
        match error {
            GraphEditError::Apply(_) => "apply".to_owned(),
            GraphEditError::Get(error) => {
                format!("get {:?}", Reason::Restrict(ref_restrict_reason(error)))
            }
            GraphEditError::Put(error) => format!("put {:?}", ref_lens_reason(error)),
            GraphEditError::IdsExhausted => "ids exhausted".to_owned(),
            GraphEditError::PutGetMismatch => "put-get mismatch".to_owned(),
        }
    }

    fn production_state(lens: &GraphEditLens) -> (RefGraph, RefGraph, RefComplement) {
        (
            ref_graph(lens.source()),
            ref_graph(lens.view()),
            ref_complement(lens.complement()),
        )
    }

    /// Translate `letter` in direction `put` on both sides, assert that the
    /// outcomes agree and that a refusal leaves the production lens as it
    /// was, and return the two advanced states when the edit is admitted.
    fn step(
        lens: &RefGraphLens<'_>,
        production: &GraphEditLens,
        state: &GraphEditState,
        put: bool,
        letter: &RefGraphDelta,
    ) -> Option<(GraphEditLens, GraphEditState)> {
        let get = |source: &RefGraph| panproto_reference::lens::get(lens, source);
        let put_oracle = |view: &RefGraph, complement: &RefComplement| lens.put(view, complement);
        let reference = if put {
            ref_edit::graph_edit_put(&get, &put_oracle, state, letter)
        } else {
            ref_edit::graph_edit_get::<RefRestrictError, RefLensError>(&get, state, letter)
        };
        let mut advanced = production.clone();
        let edit = prod_delta(letter);
        let produced = if put {
            advanced.put_edit(&edit)
        } else {
            advanced.get_edit(&edit)
        };
        let (production_outcome, translated) = match &produced {
            Ok(step) => {
                let (source, view, complement) = production_state(&advanced);
                let delta = if put {
                    &step.source_delta
                } else {
                    &step.view_delta
                };
                (
                    Outcome::Admitted(Box::new(Admitted {
                        delta: ref_delta(delta),
                        source,
                        view,
                        complement,
                    })),
                    true,
                )
            }
            Err(error) => {
                assert_eq!(
                    production_state(&advanced),
                    production_state(production),
                    "a refused edit changed the lens: {letter:?}"
                );
                (Outcome::Refused(production_refusal(error)), false)
            }
        };
        let reference_outcome = match &reference {
            Ok((delta, next)) => Outcome::Admitted(Box::new(Admitted {
                delta: delta.clone(),
                source: next.source.clone(),
                view: next.view.clone(),
                complement: without_identity(&next.complement),
            })),
            Err(error) => Outcome::Refused(reference_refusal(error)),
        };
        assert_eq!(
            production_outcome,
            reference_outcome,
            "graph edit lens {} of {letter:?} from {:?}",
            if put { "put" } else { "get" },
            state.source
        );
        match reference {
            Ok((_, next)) if translated => Some((advanced, next)),
            _ => None,
        }
    }

    /// Bind both sides to `source`, or `None` when graph get refuses it.
    fn bind(
        pair: &Pair,
        lens: &RefGraphLens<'_>,
        source: &RefGraph,
    ) -> Option<(GraphEditLens, GraphEditState)> {
        let (view, complement) = panproto_reference::lens::get(lens, source).ok()?;
        let production = Lens {
            compiled: pair.production.compiled.clone(),
            src_schema: pair.production.src_schema.clone(),
            tgt_schema: pair.production.tgt_schema.clone(),
        };
        let production = GraphEditLens::new(production, prod_graph(source))
            .expect("production get accepts what reference get accepts");
        let state = GraphEditState {
            source: source.clone(),
            view,
            complement,
        };
        assert_eq!(ref_graph(production.view()), state.view);
        assert_eq!(
            ref_complement(production.complement()),
            without_identity(&state.complement)
        );
        Some((production, state))
    }

    /// The edits one sweep compared.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct Counts {
        /// Bound bases: those whose reference get succeeds.
        bound: usize,
        /// Get letters, one per letter of every bound base's source.
        get_letters: usize,
        /// Put letters, one per letter of every bound base's view.
        put_letters: usize,
        /// Inverses translated after an admitted get letter.
        inverses: usize,
    }

    impl std::ops::AddAssign for Counts {
        fn add_assign(&mut self, other: Self) {
            self.bound += other.bound;
            self.get_letters += other.get_letters;
            self.put_letters += other.put_letters;
            self.inverses += other.inverses;
        }
    }

    /// The closed-form letter counts of one carrier base: the get letters
    /// of its source and the put letters of its reference view, without
    /// building a letter or running a production lens.
    fn closed_form(pair: &Pair, stage: &BoundedLensStage, source: &RefGraph) -> Counts {
        pair.with_reference(|lens| {
            let Ok(lens) = lens else {
                return Counts::default();
            };
            let Ok((view, _)) = panproto_reference::lens::get(lens, source) else {
                return Counts::default();
            };
            Counts {
                bound: 1,
                get_letters: graph_edit_lens_letter_count(&stage.source, source),
                put_letters: graph_edit_lens_letter_count(&stage.target, &view),
                inverses: 0,
            }
        })
    }

    /// Every one-letter get and put edit of one carrier base, and the
    /// inverse of each admitted get edit after it.
    fn unit(pair: &Pair, stage: &BoundedLensStage, source: &RefGraph) -> Counts {
        pair.with_reference(|lens| {
            let Ok(lens) = lens else {
                return Counts::default();
            };
            let Some((edit_lens, held)) = bind(pair, lens, source) else {
                return Counts::default();
            };
            let mut counts = Counts {
                bound: 1,
                ..Counts::default()
            };
            for letter in graph_edit_lens_letters(
                &stage.source,
                source,
                BOUNDED_LENS_FIELD,
                &BOUNDED_LENS_VALUE,
            ) {
                counts.get_letters += 1;
                if let Some((advanced, next)) = step(lens, &edit_lens, &held, false, &letter) {
                    counts.inverses += 1;
                    step(lens, &advanced, &next, false, &letter.invert());
                }
            }
            for letter in graph_edit_lens_letters(
                &stage.target,
                &held.view,
                BOUNDED_LENS_FIELD,
                &BOUNDED_LENS_VALUE,
            ) {
                counts.put_letters += 1;
                step(lens, &edit_lens, &held, true, &letter);
            }
            counts
        })
    }

    /// Run `per_base` on every graph base of the bounded lens carrier that
    /// `select` admits, by position; returns the bases, the selected bases
    /// and the summed counts.
    fn sweep(
        select: impl Fn(usize, &RefGraph) -> bool,
        per_base: impl Fn(&Pair, &BoundedLensStage, &RefGraph) -> Counts,
    ) -> (usize, usize, Counts) {
        let stages = bounded_lens_stages();
        let pairs: Vec<Pair> = stages.iter().map(Pair::from_stage).collect();
        let (mut bases, mut selected, mut counts) = (0, 0, Counts::default());
        for_each_lens_equivalence_case(|case| {
            if let LensEquivalenceCase::Graph { stage, source } = case {
                if select(bases, &source) {
                    selected += 1;
                    let observed = per_base(&pairs[stage], &stages[stage], &source);
                    if observed.bound == 1 {
                        let expected = closed_form(&pairs[stage], &stages[stage], &source);
                        assert_eq!(
                            (observed.get_letters, observed.put_letters),
                            (expected.get_letters, expected.put_letters),
                            "letters of stage {stage} at {source:?}"
                        );
                    }
                    counts += observed;
                }
                bases += 1;
            }
        });
        (bases, selected, counts)
    }

    /// The closed-form census of the whole carrier: bound bases, get
    /// letters and put letters, summed from [`graph_edit_lens_letter_count`]
    /// over every base whose reference get succeeds.
    const GRAPH_EDIT_LENS_CENSUS: (usize, usize, usize) = (124_009, 5_300_310, 5_221_561);

    #[test]
    fn graph_edit_lens_letter_census_is_the_closed_form_over_the_carrier() {
        let (bases, selected, counts) = sweep(|_, _| true, closed_form);
        println!("graph edit lens census: bases={bases}, counts={counts:?}");
        assert_eq!((bases, selected), (144_397, 144_397));
        assert_eq!(
            (counts.bound, counts.get_letters, counts.put_letters),
            GRAPH_EDIT_LENS_CENSUS
        );
    }

    #[test]
    fn graph_edit_lens_agrees_with_the_reference_on_every_two_node_base() {
        let (bases, selected, counts) = sweep(|_, source| source.nodes.len() <= 2, unit);
        println!(
            "graph edit lens two-node scope: bases={bases}, selected={selected}, counts={counts:?}"
        );
        assert_eq!(bases, 144_397);
        assert!(counts.bound > 0 && counts.get_letters > counts.bound);
    }

    #[test]
    fn graph_edit_lens_law_certificate() {
        let shard = LawShard::from_environment();
        let (bases, selected, counts) = sweep(
            |position, _| shard.is_none_or(|shard| shard.contains(position)),
            unit,
        );
        println!(
            "graph edit lens carrier: bases={bases}, shard={shard:?}, selected={selected}, counts={counts:?}"
        );
        assert_eq!(bases, 144_397);
        assert_eq!(selected, shard.map_or(bases, |shard| shard.share(bases)));
        if shard.is_none() {
            assert_eq!(
                (counts.bound, counts.get_letters, counts.put_letters),
                GRAPH_EDIT_LENS_CENSUS
            );
        }
    }

    /// A graph over the plain vertices of `stage` built from generated
    /// choices: one node per entry of `nodes`, and each entry of `edges`
    /// that names a schema edge between the anchors of its two nodes.
    fn generated_source(
        stage: &BoundedLensStage,
        nodes: &[u8],
        edges: &[(u8, u8, u8)],
    ) -> Option<RefGraph> {
        let vertices: Vec<&String> = stage.source.vertices.keys().collect();
        if vertices.is_empty() {
            return None;
        }
        let mut graph = RefGraph::default();
        for (id, choice) in nodes.iter().enumerate() {
            let id = u32::try_from(id).ok()?;
            let anchor = vertices[usize::from(*choice) % vertices.len()];
            graph.nodes.insert(id, RefNode::new(id, anchor));
        }
        let count = nodes.len();
        for (index, (src, tgt, label)) in edges.iter().enumerate() {
            let src = u32::try_from(usize::from(*src) % count).ok()?;
            let tgt = u32::try_from(usize::from(*tgt) % count).ok()?;
            let joining: Vec<&RefSEdge> = stage
                .source
                .edges
                .iter()
                .filter(|edge| {
                    edge.src == graph.nodes[&src].anchor && edge.tgt == graph.nodes[&tgt].anchor
                })
                .collect();
            if joining.is_empty() {
                continue;
            }
            let label = joining[usize::from(*label) % joining.len()].clone();
            graph.edges.insert(
                u32::try_from(index).ok()?,
                RefGEdge {
                    src,
                    tgt,
                    label,
                    position: None,
                },
            );
        }
        Some(graph)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn randomized_graph_edit_lens_differential(
            stage in 0usize..1024,
            nodes in prop::collection::vec(any::<u8>(), 1..6),
            edges in prop::collection::vec((any::<u8>(), any::<u8>(), any::<u8>()), 0..8),
            word in prop::collection::vec((any::<bool>(), any::<usize>()), 1..6),
        ) {
            let stages = bounded_lens_stages();
            let stage = &stages[stage % stages.len()];
            let pair = Pair::from_stage(stage);
            let Some(source) = generated_source(stage, &nodes, &edges) else {
                return Ok(());
            };
            pair.with_reference(|lens| {
                let Ok(lens) = lens else { return };
                let Some((mut production, mut state)) = bind(&pair, lens, &source) else {
                    return;
                };
                for (put, choice) in &word {
                    let (schema, graph) = if *put {
                        (&stage.target, &state.view)
                    } else {
                        (&stage.source, &state.source)
                    };
                    let letters = graph_edit_lens_letters(
                        schema,
                        graph,
                        BOUNDED_LENS_FIELD,
                        &BOUNDED_LENS_VALUE,
                    );
                    if letters.is_empty() {
                        continue;
                    }
                    let letter = &letters[choice % letters.len()];
                    if let Some((advanced, next)) = step(lens, &production, &state, *put, letter) {
                        production = advanced;
                        state = next;
                    }
                }
            });
        }
    }
}

/// Segmented graph get: the checkpointed run of a graph lens's restrict
/// over a sealed segment family, against the reference definition of
/// segmented restriction and whole-state get of the glued graph.
mod segmented_graph_get {
    use std::collections::BTreeMap;

    use panproto_inst::InstanceShapeKind;
    use panproto_inst::identity::{ComplementShape, Digest};
    use panproto_inst::segment::{
        AnySegment, Budget, EdgeIdRun, GraphComplementSegment, GraphRestrictRun, IdRange,
        ManifestHeader, ManifestRole, MemoryStore, Partition, RecordSource, Residual,
        RunIncompleteReason, RunStatus, SegmentId, SegmentManifest, SegmentRecord, SegmentSpec,
        SegmentedRestrict, partition_graph, read_emitted, seal_full,
    };
    use panproto_lens::{Lens, graph_lens};
    use panproto_reference::enumerate::segments::{
        SEGMENTED_RESTRICT_ALLOCATION_BASE, segmented_restrict_cases,
    };
    use panproto_reference::restrict::NoValueLayer;
    use panproto_reference::segment as reference;

    use super::{
        RefGraph, RefMigration, RefSchema, prod_graph, prod_migration, prod_schema, ref_complement,
        ref_graph,
    };

    /// The schema digest of a sealed source family.
    pub const SOURCE_SCHEMA: [u8; 32] = [7; 32];

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

    /// A sealed source family: its manifest, the segments' canonical bytes
    /// and the production partition.
    pub struct Input {
        /// The source manifest.
        pub manifest: SegmentManifest,
        /// The segments' canonical bytes.
        pub bytes: BTreeMap<SegmentId, Vec<u8>>,
        /// The production partition.
        pub partition: Partition,
    }

    /// Seal the production segments of `graph` under `partition`.
    pub fn input(graph: &RefGraph, partition: &reference::Partition) -> Input {
        let partition = prod_partition(partition);
        let segments: Vec<AnySegment> = partition_graph(&prod_graph(graph), &partition)
            .expect("production split")
            .into_iter()
            .map(AnySegment::Graph)
            .collect();
        let schema = Digest(SOURCE_SCHEMA);
        let records = segments
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
                shape: InstanceShapeKind::Graph,
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

    pub fn lens_of(schema: &RefSchema, migration: &RefMigration) -> Lens {
        let schema = prod_schema(schema);
        Lens {
            compiled: prod_migration(migration),
            src_schema: schema.clone(),
            tgt_schema: schema,
        }
    }

    fn run(
        lens: &Lens,
        input: &Input,
        budget: Budget,
        store: &mut MemoryStore,
        cancel: Option<u32>,
    ) -> GraphRestrictRun {
        graph_lens::get_segmented(
            lens,
            &input.manifest,
            input.manifest.digest(),
            &input.bytes,
            budget,
            store,
            cancel,
        )
        .expect("the lens's plan compiles")
    }

    /// The emitted view and complement segments of a complete run, the
    /// complements decoded from their canonical bytes.
    fn emitted(store: &MemoryStore, run: &GraphRestrictRun, input: &Input) -> SegmentedRestrict {
        let identity = run.run.expect("a run identity");
        let (mut views, mut complements) = (Vec::new(), Vec::new());
        for spec in input.partition.specs() {
            let emitted =
                read_emitted(store, identity, &input.partition, spec.id).expect("emitted segment");
            complements.push(
                GraphComplementSegment::from_canonical_bytes(&emitted.complement, spec.id)
                    .expect("the emitted complement decodes"),
            );
            views.push(emitted.view);
        }
        SegmentedRestrict { views, complements }
    }

    /// The outcome of one case.
    #[derive(Debug, PartialEq, Eq)]
    enum Outcome {
        Complete,
        Refused,
    }

    /// Check the law on one case: the complete run's segments are, byte for
    /// byte, the reference definition's, they assemble to whole-state get,
    /// and every cancellation point resumes to the uninterrupted run.
    fn check_case(
        schema: &RefSchema,
        migration: &RefMigration,
        graph: &RefGraph,
        partition: &reference::Partition,
    ) -> Outcome {
        let lens = lens_of(schema, migration);
        let input = input(graph, partition);
        let mut store = MemoryStore::default();
        let baseline = run(&lens, &input, Budget::unlimited(), &mut store, None);
        let reference_segments =
            reference::partition_graph(graph, partition).expect("reference split");
        let expected = reference::restrict_graph_segmented(
            schema,
            schema,
            migration,
            &NoValueLayer,
            &reference_segments,
            partition,
        );
        let whole = graph_lens::get(&lens, &prod_graph(graph));
        let outcome = match (&baseline.report.status, expected) {
            (RunStatus::Complete { manifest }, Ok(expected)) => {
                assert_eq!(
                    Some(*manifest),
                    baseline.manifest.as_ref().map(SegmentManifest::digest)
                );
                let output = emitted(&store, &baseline, &input);
                let view_schema =
                    reference::Digest(lens.identity(ComplementShape::Graph).target_schema.0);
                for ((view, complement), (expected_view, expected_complement)) in
                    output.views.iter().zip(&output.complements).zip(&expected)
                {
                    assert_eq!(
                        AnySegment::Graph(view.clone())
                            .canonical_bytes(&Digest(view_schema.0), &input.partition)
                            .expect("view bytes"),
                        reference::canonical_segment_bytes(
                            &reference::AnySegment::Graph(expected_view.clone()),
                            view_schema,
                            partition,
                        )
                        .expect("reference view bytes"),
                        "view segment of {graph:?} under {partition:?}"
                    );
                    assert_eq!(
                        complement.canonical_bytes().expect("complement bytes"),
                        reference::canonical_complement_segment_bytes(
                            &reference::AnyComplementSegment::Graph(expected_complement.clone()),
                        )
                        .expect("reference complement bytes"),
                        "complement segment of {graph:?} under {partition:?}"
                    );
                }
                let (view, complement) = output
                    .assemble(&input.partition)
                    .expect("the emitted segments assemble");
                let (whole_view, whole_complement) = whole.expect("whole-state get accepts");
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
                Outcome::Complete
            }
            (RunStatus::Failed(_), Err(reference::SegmentedRestrictRefusal::Restrict { .. })) => {
                assert!(whole.is_err(), "whole-state get refuses {graph:?}");
                Outcome::Refused
            }
            (status, expected) => {
                panic!(
                    "segmented get disagrees on {graph:?} under {partition:?}: {status:?} / {expected:?}"
                )
            }
        };
        check_resumption(&lens, &input, &baseline);
        outcome
    }

    /// Stopping the run of `lens` over `input` at every cancellation point
    /// and resuming it against the same store ends as `baseline`, the
    /// uninterrupted run, reusing every task the stopped run committed.
    fn check_resumption(lens: &Lens, input: &Input, baseline: &GraphRestrictRun) {
        let finished = baseline.report.progress.tasks_done;
        for cancel in 0..=baseline.report.progress.tasks_total {
            let mut store = MemoryStore::default();
            let stopped = run(lens, input, Budget::unlimited(), &mut store, Some(cancel));
            assert_eq!(stopped.run, baseline.run, "run identity");
            if cancel < finished {
                assert!(
                    matches!(
                        stopped.report.status,
                        RunStatus::Incomplete {
                            reason: RunIncompleteReason::Cancelled
                        }
                    ),
                    "{:?}",
                    stopped.report.status
                );
            }
            let resumed = run(lens, input, Budget::unlimited(), &mut store, None);
            assert_eq!(resumed.report.tasks_reused, cancel.min(finished));
            assert_eq!(
                format!("{:?}", resumed.report.status),
                format!("{:?}", baseline.report.status)
            );
            assert_eq!(
                resumed.manifest.as_ref().map(SegmentManifest::digest),
                baseline.manifest.as_ref().map(SegmentManifest::digest)
            );
            assert_eq!(resumed.report.orphans, Vec::new());
        }
    }

    /// Every 97th case of the segmented restriction carrier, read as a lens
    /// over its schema: the segmented get agrees with the reference
    /// definition segment by segment, assembles to whole-state get, and
    /// resumes from every cancellation point.
    #[test]
    fn segmented_graph_get_assembles_to_whole_state_get_and_resumes_from_every_cancellation_point()
    {
        let (mut complete, mut refused) = (0_u64, 0_u64);
        for case in segmented_restrict_cases().step_by(97) {
            match check_case(&case.schema, &case.migration, &case.graph, &case.partition) {
                Outcome::Complete => complete += 1,
                Outcome::Refused => refused += 1,
            }
        }
        assert_eq!(complete + refused, 1_323);
        assert!(
            complete > 0 && refused > 0,
            "{complete} complete, {refused} refused"
        );
    }

    /// A budget admitting fewer segments than the partition holds declines
    /// the segmented get before any task, and a lens whose restrict carries
    /// a survival predicate is declined with a global residual.
    #[test]
    fn segmented_graph_get_declines_an_exceeded_budget_and_an_expression_before_any_task() {
        let case = segmented_restrict_cases()
            .find(|case| case.partition.specs.len() == 3)
            .expect("a three-segment case");
        let lens = lens_of(&case.schema, &case.migration);
        let input = input(&case.graph, &case.partition);
        let mut store = MemoryStore::default();
        let budget = Budget {
            affected_segments: 2,
            ..Budget::unlimited()
        };
        let declined = run(&lens, &input, budget, &mut store, None);
        assert!(
            matches!(
                declined.report.status,
                RunStatus::Residual(Residual::BudgetExceeded { .. })
            ),
            "{:?}",
            declined.report.status
        );
        assert_eq!(declined.report.progress.tasks_done, 0);

        let mut predicated = lens;
        let vertex = predicated
            .src_schema
            .vertices
            .keys()
            .next()
            .cloned()
            .expect("a vertex");
        predicated.compiled.conditional_survival.insert(
            vertex,
            panproto_expr::Expr::Lit(panproto_expr::Literal::Bool(true)),
        );
        let mut store = MemoryStore::default();
        let global = run(&predicated, &input, Budget::unlimited(), &mut store, None);
        assert!(
            matches!(
                global.report.status,
                RunStatus::Residual(Residual::Global { .. })
            ),
            "{:?}",
            global.report.status
        );
        assert_eq!(global.report.progress.tasks_done, 0);
    }

    pub const ANCHORS: [&str; 3] = ["a", "b", "c"];

    /// A generated case over a three-vertex schema with every edge and loop:
    /// the schema, a survival classification, a graph and a partition giving
    /// each node to one of `segments` segments with its own allocation range.
    pub fn generated(
        anchors: &[usize],
        edges: &[(usize, usize)],
        owners: &[u32],
        segments: u32,
        surviving: u8,
    ) -> (RefSchema, RefMigration, RefGraph, reference::Partition) {
        let mut schema = RefSchema::default();
        for anchor in ANCHORS {
            schema = schema.vertex(anchor, "record");
        }
        for src in ANCHORS {
            for tgt in ANCHORS {
                schema = schema.edge(src, tgt, "prop", Some(&format!("{src}{tgt}")));
            }
        }
        let survivors: Vec<&str> = (0..3)
            .filter(|i| surviving & (1 << i) != 0)
            .map(|i| ANCHORS[i])
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
        let n = anchors.len();
        let mut graph = RefGraph::default();
        for (id, anchor) in (0_u32..).zip(anchors) {
            graph
                .nodes
                .insert(id, super::RefNode::new(id, ANCHORS[*anchor]));
        }
        for (id, (src, tgt)) in (0_u32..).zip(edges.iter().filter(|(s, t)| *s < n && *t < n)) {
            let (a, b) = (ANCHORS[anchors[*src]], ANCHORS[anchors[*tgt]]);
            graph.edges.insert(
                id,
                super::RefGEdge {
                    src: u32::try_from(*src).expect("a small index"),
                    tgt: u32::try_from(*tgt).expect("a small index"),
                    label: super::RefSEdge::new(a, b, "prop", Some(&format!("{a}{b}"))),
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
        (schema, migration, graph, partition)
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

        #[test]
        fn segmented_graph_get_matches_the_reference_and_resumes_on_generated_partitions(
            anchors in proptest::collection::vec(0_usize..3, 2..=8),
            edges in proptest::collection::vec((0_usize..8, 0_usize..8), 0..=14),
            owners in proptest::collection::vec(0_u32..4, 8),
            segments in 2_u32..=4,
            surviving in 0_u8..8,
        ) {
            let (schema, migration, graph, partition) =
                generated(&anchors, &edges, &owners, segments, surviving);
            let outcome = check_case(&schema, &migration, &graph, &partition);
            proptest::prop_assert!(matches!(outcome, Outcome::Complete | Outcome::Refused));
        }
    }
}

mod segmented_graph_put {
    use std::collections::BTreeMap;

    use panproto_inst::identity::{ComplementShape, Digest, IdentityCarrier};
    use panproto_inst::locality::{GlobalReason, Locality, SummaryKind};
    use panproto_inst::segment::{
        AnySegment, GraphComplementSegment, GraphSegment, Partition, Residual, SegmentError,
        SegmentedRestrict, assemble_graph, graph_restrict_segmented, partition_graph,
    };
    use panproto_inst::{GraphComplement, PutDomainError, RestrictPlan, ViewRef};
    use panproto_lens::graph_lens::{self, SegmentedPutError};
    use panproto_lens::{Lens, LensError};
    use panproto_reference::enumerate::segments::segmented_restrict_cases;
    use panproto_reference::restrict::NoValueLayer;
    use panproto_reference::segment as reference;

    use super::segmented_graph_get::{generated, lens_of, prod_partition};
    use super::{
        RefGraph, RefGraphLens, RefMigration, RefSchema, lens_reason, prod_graph, ref_graph,
        ref_lens_reason,
    };

    const SCHEMA: Digest = Digest([9; 32]);

    /// The complement segments joined into one complement, unrenumbered and
    /// stamped with the lens's graph identity, as whole-state get stamps it.
    fn joined(complements: &[GraphComplementSegment], lens: &Lens) -> GraphComplement {
        let mut whole = GraphComplement::empty();
        for part in complements {
            whole.dropped_nodes.extend(part.dropped_nodes.clone());
            whole.dropped_edges.extend(part.dropped_edges.clone());
            whole.original_edges.extend(part.original_edges.clone());
            whole.original_anchors.extend(part.original_anchors.clone());
            whole
                .original_extra_fields
                .extend(part.original_extra_fields.clone());
            whole.original_values.extend(part.original_values.clone());
            whole
                .synthesized_nodes
                .extend(part.synthesized_nodes.clone());
            whole
                .synthesized_edges
                .extend(part.synthesized_edges.clone());
        }
        whole.stamp(lens.identity(ComplementShape::Graph), &lens.src_schema);
        whole
    }

    fn segment_bytes(segment: &GraphSegment, partition: &Partition) -> Vec<u8> {
        AnySegment::Graph(segment.clone())
            .canonical_bytes(&SCHEMA, partition)
            .expect("segment bytes")
    }

    fn reference_bytes(
        segment: &reference::GraphSegment,
        partition: &reference::Partition,
    ) -> Vec<u8> {
        reference::canonical_segment_bytes(
            &reference::AnySegment::Graph(segment.clone()),
            reference::Digest(SCHEMA.0),
            partition,
        )
        .expect("reference segment bytes")
    }

    /// The view of a segmented get assembled, its synthesized elements
    /// numbered as whole-state get numbers them.
    fn assembled_view(
        views: Vec<GraphSegment>,
        complements: Vec<GraphComplementSegment>,
        partition: &Partition,
    ) -> RefGraph {
        let (view, _) = SegmentedRestrict { views, complements }
            .assemble(partition)
            .expect("the view segments assemble");
        ref_graph(&view)
    }

    /// The edits of the carrier: the identity, every node and every edge
    /// removed, every edge retargeted at every other node, an edge added
    /// between every ordered pair of nodes in different segments, every
    /// node's anchor changed to every other vertex, and a node added to
    /// every segment.
    pub fn edits(view: &RefGraph, partition: &reference::Partition) -> Vec<RefGraph> {
        let owner = |node: u32| partition.owner_of_node(node);
        let label = |src: &str, tgt: &str| {
            super::RefSEdge::new(src, tgt, "prop", Some(&format!("{src}{tgt}")))
        };
        let anchor = |graph: &RefGraph, node: u32| graph.nodes[&node].anchor.clone();
        let mut out = vec![view.clone()];
        for node in view.nodes.keys() {
            let mut edited = view.clone();
            edited.nodes.remove(node);
            edited
                .edges
                .retain(|_, edge| edge.src != *node && edge.tgt != *node);
            out.push(edited);
        }
        for id in view.edges.keys() {
            let mut edited = view.clone();
            edited.edges.remove(id);
            out.push(edited);
        }
        for (id, edge) in &view.edges {
            for target in view.nodes.keys().filter(|target| **target != edge.tgt) {
                let mut edited = view.clone();
                let moved = edited.edges.get_mut(id).expect("the edge");
                moved.tgt = *target;
                moved.label = label(&anchor(view, edge.src), &anchor(view, *target));
                out.push(edited);
            }
        }
        for src in view.nodes.keys() {
            for tgt in view.nodes.keys() {
                let Some(holder) = owner(*src).filter(|holder| owner(*tgt) != Some(*holder)) else {
                    continue;
                };
                let Some(range) = partition.spec(holder).and_then(|spec| spec.alloc_edges) else {
                    continue;
                };
                let mut edited = view.clone();
                edited.edges.insert(
                    range.lo + 5,
                    super::RefGEdge {
                        src: *src,
                        tgt: *tgt,
                        label: label(&anchor(view, *src), &anchor(view, *tgt)),
                        position: None,
                    },
                );
                out.push(edited);
            }
        }
        for node in view.nodes.keys() {
            for vertex in super::segmented_graph_get::ANCHORS {
                if vertex == anchor(view, *node) {
                    continue;
                }
                let mut edited = view.clone();
                edited.nodes.get_mut(node).expect("the node").anchor = vertex.into();
                out.push(edited);
            }
        }
        for spec in &partition.specs {
            let Some(range) = spec.alloc_nodes else {
                continue;
            };
            let mut edited = view.clone();
            edited
                .nodes
                .insert(range.lo + 5, super::RefNode::new(range.lo + 5, "a"));
            out.push(edited);
        }
        out
    }

    /// What one case gave.
    #[derive(Default)]
    struct Tally {
        restored: u64,
        refused: u64,
        crossing: u64,
        get_refused: u64,
    }

    /// Check one case: the segmented get's view and every edit of it put
    /// back segment by segment agree with the reference definition and,
    /// glued, with whole-state put of the glued view; the unedited view
    /// restores the source segments exactly, and a restored source's
    /// segmented get assembles to the edited view exactly when whole-state
    /// get of whole-state put does.
    fn check_case(
        schema: &RefSchema,
        migration: &RefMigration,
        graph: &RefGraph,
        partition: &reference::Partition,
        tally: &mut Tally,
    ) {
        let lens = lens_of(schema, migration);
        let prod_p = prod_partition(partition);
        let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
            .expect("the carrier compiles");
        let source = partition_graph(&prod_graph(graph), &prod_p).expect("production split");
        let Ok(got) = graph_restrict_segmented(&plan, &source, &prod_p) else {
            return;
        };
        let (reference_views, reference_complements) =
            reference_get(schema, migration, graph, partition);
        let reference_plan = panproto_reference::restrict::compile(schema, schema, migration)
            .expect("the reference plan compiles");
        let reference_lens = RefGraphLens::new(
            schema,
            schema,
            migration,
            &reference_plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .expect("the reference lens");
        let whole_complement = joined(&got.complements, &lens);
        let view = reference::assemble_graph(&reference_views, partition).expect("glued view");

        for (at, edited) in edits(&view, partition).into_iter().enumerate() {
            let views = partition_graph(&prod_graph(&edited), &prod_p).expect("edited split");
            let reference_edited =
                reference::partition_graph(&edited, partition).expect("reference edited split");
            let production = graph_lens::put_segmented(&lens, &views, &got.complements, &prod_p);
            let expected = reference::put_graph_segmented(
                &reference_lens,
                &reference_edited,
                &reference_complements,
                partition,
            );
            let whole = graph_lens::put(&lens, &prod_graph(&edited), &whole_complement);
            match (&production, &whole) {
                (Ok(restored), Ok(whole)) => {
                    let glued = assemble_graph(restored.iter().cloned(), &prod_p)
                        .expect("the restored segments glue");
                    assert_eq!(
                        ref_graph(&glued),
                        ref_graph(whole),
                        "glued put of {edited:?}"
                    );
                    let Ok(expected) = &expected else {
                        panic!("the reference refuses {edited:?}: {expected:?}");
                    };
                    for (segment, expected) in restored.iter().zip(expected) {
                        assert_eq!(
                            segment_bytes(segment, &prod_p),
                            reference_bytes(expected, partition),
                            "restored segment of {edited:?} under {partition:?}"
                        );
                    }
                    if at == 0 {
                        for (segment, original) in restored.iter().zip(&source) {
                            assert_eq!(
                                segment_bytes(segment, &prod_p),
                                segment_bytes(original, &prod_p),
                                "GetPut of {graph:?} under {partition:?}"
                            );
                        }
                    }
                    check_put_get(
                        &lens,
                        &plan,
                        restored,
                        &views,
                        &got.complements,
                        &prod_p,
                        whole,
                    );
                    if views
                        .iter()
                        .any(|segment| !segment.boundary.ghosts.is_empty())
                    {
                        tally.crossing += 1;
                    }
                    tally.restored += 1;
                }
                (Err(SegmentedPutError::Lens(error)), Err(whole)) => {
                    assert_ne!(at, 0, "GetPut refused on {graph:?}");
                    check_refusal(error, whole, &expected, &edited, partition, tally);
                }
                outcomes => panic!(
                    "segmented put disagrees with whole-state put on {edited:?} under {partition:?}: {outcomes:?}"
                ),
            }
        }
    }

    /// The segment-get of the reference definition: each segment's view and
    /// complement.
    pub fn reference_get(
        schema: &RefSchema,
        migration: &RefMigration,
        graph: &RefGraph,
        partition: &reference::Partition,
    ) -> (
        Vec<reference::GraphSegment>,
        Vec<reference::GraphComplementSegment>,
    ) {
        let source = reference::partition_graph(graph, partition).expect("reference split");
        reference::restrict_graph_segmented(
            schema,
            schema,
            migration,
            &NoValueLayer,
            &source,
            partition,
        )
        .expect("the reference restricts what production restricts")
        .into_iter()
        .unzip()
    }

    /// A refused segmented put: whole-state put refuses with the same
    /// reason, and so does the reference definition, a view whose restored
    /// source get refuses included.
    fn check_refusal(
        error: &LensError,
        whole: &LensError,
        expected: &Result<Vec<reference::GraphSegment>, reference::SegmentedPutRefusal>,
        edited: &RefGraph,
        partition: &reference::Partition,
        tally: &mut Tally,
    ) {
        assert_eq!(lens_reason(error), lens_reason(whole), "{edited:?}");
        match expected {
            Err(reference::SegmentedPutRefusal::Put(expected)) => {
                assert_eq!(lens_reason(error), ref_lens_reason(expected), "{edited:?}");
                tally.refused += 1;
                if refused_by_the_next_get(error) {
                    tally.get_refused += 1;
                }
            }
            expected => panic!(
                "the reference disagrees on {edited:?} under {partition:?}: {error:?} / {expected:?}"
            ),
        }
    }

    /// Whether a refusal is the clause (5) refusal of a view whose restored
    /// source get refuses with something other than a predicate's verdict.
    pub fn refused_by_the_next_get(error: &LensError) -> bool {
        matches!(
            error,
            LensError::PutDomain(found) if matches!(
                **found,
                PutDomainError::SurvivalPredicateRefused {
                    refusal: panproto_inst::PredicateRefusal::EvaluationFailed { .. },
                    ..
                }
            )
        )
    }

    /// `PutGet` over segments: the segmented get of a restored family
    /// assembles to whole-state get of the glued restored source, and to
    /// the edited view exactly when whole-state get of whole-state put does.
    fn check_put_get(
        lens: &Lens,
        plan: &RestrictPlan,
        restored: &[GraphSegment],
        views: &[GraphSegment],
        complements: &[GraphComplementSegment],
        partition: &Partition,
        whole: &panproto_inst::GInstance,
    ) {
        let again = graph_restrict_segmented(plan, restored, partition)
            .expect("a restored family restricts again");
        let segmented = assembled_view(again.views, again.complements, partition);
        let (whole_view, _) = graph_lens::get(lens, whole).expect("whole-state get");
        assert_eq!(
            segmented,
            ref_graph(&whole_view),
            "segmented get of the put"
        );
        let edited = assembled_view(views.to_vec(), complements.to_vec(), partition);
        assert_eq!(
            segmented == edited,
            ref_graph(&whole_view) == edited,
            "PutGet"
        );
    }

    /// Run [`check_case`] on the cases of every 97th case of the segmented
    /// restriction carrier whose position among them `select` admits, and
    /// return how many there are and how many were selected.
    fn sweep(select: impl Fn(usize) -> bool) -> (usize, usize, Tally) {
        let mut tally = Tally::default();
        let (mut cases, mut selected) = (0, 0);
        for (position, case) in segmented_restrict_cases().step_by(97).enumerate() {
            cases += 1;
            if !select(position) {
                continue;
            }
            selected += 1;
            check_case(
                &case.schema,
                &case.migration,
                &case.graph,
                &case.partition,
                &mut tally,
            );
        }
        (cases, selected, tally)
    }

    fn assert_every_outcome(tally: &Tally) {
        assert!(
            tally.restored > 0 && tally.refused > 0 && tally.crossing > 0 && tally.get_refused > 0,
            "{} restored, {} refused, {} crossing, {} refused by the next get",
            tally.restored,
            tally.refused,
            tally.crossing,
            tally.get_refused
        );
    }

    /// Every 97th case of the segmented restriction carrier, read as a lens
    /// over its schema, with every edit of its segmented view: segmented put
    /// agrees with the reference definition segment by segment and, glued,
    /// with whole-state put of the glued view, refusing exactly when it
    /// refuses and with the same violation; `GetPut` and `PutGet` hold over
    /// segments where they hold whole. `PANPROTO_TEST_SHARD=i/n` restricts
    /// the check to the cases at positions `i` modulo `n`.
    #[test]
    fn segmented_graph_put_agrees_with_the_reference_and_assembles_to_whole_state_put() {
        let shard = super::LawShard::from_environment();
        let (cases, selected, tally) =
            sweep(|position| shard.is_none_or(|shard| shard.contains(position)));
        assert_eq!(cases, 1_323);
        assert_eq!(selected, shard.map_or(cases, |shard| shard.share(cases)));
        assert_every_outcome(&tally);
    }

    /// The eighth of the certificate's cases at positions divisible by eight,
    /// checked on every run.
    #[test]
    fn segmented_graph_put_slice_agrees_with_the_reference_on_every_eighth_case() {
        let (cases, selected, tally) = sweep(|position| position % 8 == 0);
        assert_eq!((cases, selected), (1_323, 166));
        assert_every_outcome(&tally);
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

        #[test]
        fn segmented_graph_put_matches_the_reference_on_generated_partitions(
            anchors in proptest::collection::vec(0_usize..3, 2..=8),
            edges in proptest::collection::vec((0_usize..8, 0_usize..8), 0..=14),
            owners in proptest::collection::vec(0_u32..4, 8),
            segments in 2_u32..=4,
            surviving in 0_u8..8,
        ) {
            let (schema, migration, graph, partition) =
                generated(&anchors, &edges, &owners, segments, surviving);
            check_case(&schema, &migration, &graph, &partition, &mut Tally::default());
        }
    }

    /// A three-segment graph whose dropped middle node hides a path from
    /// the first segment to the third: the segmented view holds the
    /// contracted edge, and an edit that removes the third segment's node
    /// and adds an edge from it back to the first is put back segment by
    /// segment like whole-state put.
    #[test]
    fn a_put_crossing_three_segments_restores_like_whole_state_put() {
        let (schema, migration, graph, partition) = generated(
            &[0, 1, 2, 0],
            &[(0, 1), (1, 2), (3, 0)],
            &[0, 1, 2, 2, 0, 0, 0, 0],
            3,
            0b101,
        );
        let lens = lens_of(&schema, &migration);
        let prod_p = prod_partition(&partition);
        let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
            .expect("the lens compiles");
        let source = partition_graph(&prod_graph(&graph), &prod_p).expect("split");
        let got = graph_restrict_segmented(&plan, &source, &prod_p).expect("segmented get");
        assert_eq!(
            got.complements
                .iter()
                .map(|complement| complement.synthesized_edges.len())
                .collect::<Vec<_>>(),
            vec![1, 0, 0],
            "the contracted edge is synthesized in the first segment"
        );
        let restored = graph_lens::put_segmented(&lens, &got.views, &got.complements, &prod_p)
            .expect("GetPut");
        for (segment, original) in restored.iter().zip(&source) {
            assert_eq!(
                segment_bytes(segment, &prod_p),
                segment_bytes(original, &prod_p)
            );
        }
        let mut tally = Tally::default();
        check_case(&schema, &migration, &graph, &partition, &mut tally);
        assert!(tally.crossing > 0 && tally.refused > 0 && tally.restored > 0);
    }

    /// A view edge whose id a complement in another segment records as a
    /// dropped edge collides, and two violations in different segments
    /// refuse the put with the lesser of them, as whole-state put does.
    #[test]
    fn a_refused_put_returns_the_least_violation_of_every_segment() {
        let (schema, migration, graph, partition) = generated(
            &[0, 1, 2],
            &[(0, 1), (1, 2), (2, 0)],
            &[0, 1, 0, 0, 0, 0, 0, 0],
            2,
            0b101,
        );
        let lens = lens_of(&schema, &migration);
        let prod_p = prod_partition(&partition);
        let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
            .expect("the lens compiles");
        let source = partition_graph(&prod_graph(&graph), &prod_p).expect("split");
        let got = graph_restrict_segmented(&plan, &source, &prod_p).expect("segmented get");
        let dropped: Vec<(u32, u32)> = got
            .complements
            .iter()
            .flat_map(|complement| {
                complement
                    .dropped_edges
                    .keys()
                    .map(|id| (complement.segment.0, id.0))
            })
            .collect();
        assert_eq!(dropped, vec![(0, 0), (1, 1)]);

        // Node 2 (segment 0) gets an anchor no source vertex maps to, and a
        // new view edge in segment 0 reuses the id of segment 1's dropped
        // edge 1: the collision is the lesser violation.
        let view = assemble_graph(got.views.iter().cloned(), &prod_p).expect("glue");
        let mut edited = ref_graph(&view);
        edited.nodes.get_mut(&2).expect("node 2").anchor = "b".into();
        edited.edges.insert(
            1,
            super::RefGEdge {
                src: 0,
                tgt: 2,
                label: super::RefSEdge::new("a", "c", "prop", Some("ac")),
                position: None,
            },
        );
        let views = partition_graph(&prod_graph(&edited), &prod_p).expect("edited split");
        let error = graph_lens::put_segmented(&lens, &views, &got.complements, &prod_p)
            .expect_err("the put is refused");
        assert!(
            matches!(
                &error,
                SegmentedPutError::Lens(LensError::PutDomain(found))
                    if **found == PutDomainError::IdCollision { elem: ViewRef::Edge(panproto_inst::EdgeId(1)) }
            ),
            "{error:?}"
        );
        let whole = graph_lens::put(
            &lens,
            &prod_graph(&edited),
            &joined(&got.complements, &lens),
        )
        .expect_err("whole-state put refuses");
        let SegmentedPutError::Lens(error) = error else {
            unreachable!("matched above")
        };
        assert_eq!(lens_reason(&error), lens_reason(&whole));

        // Without the colliding edge, the edge from node 2 no longer maps
        // back: its source's view anchor is the image of no source vertex
        // with an edge of its label.
        edited.edges.remove(&1);
        let views = partition_graph(&prod_graph(&edited), &prod_p).expect("edited split");
        let error = graph_lens::put_segmented(&lens, &views, &got.complements, &prod_p)
            .expect_err("the put is refused");
        assert!(
            matches!(
                &error,
                SegmentedPutError::Lens(LensError::PutDomain(found))
                    if **found == PutDomainError::UnrepresentableViewEdge { elem: ViewRef::Edge(panproto_inst::EdgeId(2)) }
            ),
            "{error:?}"
        );
        let whole = graph_lens::put(
            &lens,
            &prod_graph(&edited),
            &joined(&got.complements, &lens),
        )
        .expect_err("whole-state put refuses");
        let SegmentedPutError::Lens(error) = error else {
            unreachable!("matched above")
        };
        assert_eq!(lens_reason(&error), lens_reason(&whole));
    }

    /// The locality of put: a plain lens merges sorted runs, a survival
    /// predicate and a key on a path of two edges are global, and a segmented put
    /// declines a global lens before reading any segment and refuses a
    /// complement entry filed under a segment that does not own it.
    #[test]
    fn put_locality_classes_and_input_refusals() {
        let (schema, migration, graph, partition) =
            generated(&[0, 1], &[(0, 1)], &[0, 1, 0, 0, 0, 0, 0, 0], 2, 0b001);
        let lens = lens_of(&schema, &migration);
        assert_eq!(
            graph_lens::graph_put_locality(&lens).expect("compiles"),
            Locality::Summary(SummaryKind::SortedMerge)
        );
        let prod_p = prod_partition(&partition);
        let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
            .expect("the lens compiles");
        let source = partition_graph(&prod_graph(&graph), &prod_p).expect("split");
        let got = graph_restrict_segmented(&plan, &source, &prod_p).expect("segmented get");

        let mut keyed = lens_of(&schema, &migration);
        keyed.src_schema.declarations.keys.insert(
            "a".into(),
            vec![panproto_schema::Key {
                id: "order".into(),
                parts: vec![panproto_schema::KeyPart::Path(
                    ["aa", "ab"]
                        .into_iter()
                        .map(|name| panproto_schema::Edge {
                            src: "a".into(),
                            tgt: name[1..].into(),
                            kind: "prop".into(),
                            name: Some(name.into()),
                        })
                        .collect(),
                )],
                role: panproto_schema::KeyRole::Unique,
                scope: panproto_schema::KeyScope::Global,
                equality: Vec::new(),
            }],
        );
        let demand = Locality::Global(GlobalReason::KeyDemand {
            vertex: "a".into(),
            key: "order".into(),
        });
        assert_eq!(
            graph_lens::graph_put_locality(&keyed).expect("compiles"),
            demand
        );
        assert!(matches!(
            graph_lens::put_segmented(&keyed, &got.views, &got.complements, &prod_p),
            Err(SegmentedPutError::Residual(Residual::Global { .. }))
        ));

        let mut predicated = lens_of(&schema, &migration);
        predicated.compiled.conditional_survival.insert(
            "a".into(),
            panproto_expr::Expr::Lit(panproto_expr::Literal::Bool(true)),
        );
        assert!(matches!(
            graph_lens::graph_put_locality(&predicated).expect("compiles"),
            Locality::Global(GlobalReason::ExpressionDemand { .. })
        ));

        let mut misplaced = got.complements;
        let moved = misplaced[1]
            .dropped_nodes
            .pop_first()
            .expect("segment 1 drops node 1");
        misplaced[0].dropped_nodes.insert(moved.0, moved.1);
        assert!(
            matches!(
                graph_lens::put_segmented(&lens, &got.views, &misplaced, &prod_p),
                Err(SegmentedPutError::Segment(SegmentError::WrongOwner { .. }))
            ),
            "a dropped node filed under a segment that does not own it"
        );
    }
}

/// Segmented graph put as a checkpointed run: the production run against
/// the reference runner, at every cancellation point and under budgets.
mod segmented_graph_put_run {
    use std::collections::BTreeMap;

    use panproto_inst::RestrictPlan;
    use panproto_inst::identity::{ComplementShape, Digest};
    use panproto_inst::segment::{
        AnySegment, ArtifactKey, Budget, EdgeIdRun, GraphSegment, ManifestHeader, ManifestRole,
        MemoryStore, Partition, Phase, RecordSource, Residual, Resource, RunIncompleteReason,
        RunJournal, RunStatus, SegmentId, SegmentManifest, SegmentRecord, SegmentRunReport,
        SegmentStore, TaskRecord, graph_restrict_segmented, partition_graph, read_emitted,
        seal_full,
    };
    use panproto_lens::Lens;
    use panproto_lens::graph_lens::{self, GraphPutRun, HeldView, SegmentedPutError};
    use panproto_reference::enumerate::segments::segmented_restrict_cases;
    use panproto_reference::restrict::NoValueLayer;
    use panproto_reference::segment as reference;

    use super::segmented_graph_get::{generated, lens_of, prod_partition};
    use super::segmented_graph_put::{edits, reference_get, refused_by_the_next_get};
    use super::{
        RefGraph, RefGraphLens, RefMigration, RefSchema, lens_reason, prod_graph, ref_lens_reason,
    };

    const VIEW_SCHEMA: [u8; 32] = [11; 32];

    /// A report in a form both implementations project to.
    #[derive(Debug, PartialEq, Eq)]
    struct Seen {
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

    fn seen_production(report: &SegmentRunReport<SegmentedPutError>) -> Seen {
        let status = match &report.status {
            RunStatus::Complete { manifest } => format!("complete {:?}", manifest.0),
            RunStatus::Incomplete { reason } => format!("incomplete {reason:?}"),
            RunStatus::Residual(residual) => format!("residual {residual:?}"),
            RunStatus::Failed(SegmentedPutError::Lens(error)) => {
                format!("put {:?}", lens_reason(error))
            }
            RunStatus::Failed(SegmentedPutError::Segment(error)) => format!("segment {error:?}"),
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
        report: &reference::SegmentRunReport<reference::SegmentedPutRefusal>,
    ) -> Seen {
        let status = match &report.status {
            reference::RunStatus::Complete { manifest } => format!("complete {:?}", manifest.0),
            reference::RunStatus::Incomplete { reason } => format!("incomplete {reason:?}"),
            reference::RunStatus::Residual(residual) => format!("residual {residual:?}"),
            reference::RunStatus::Failed(reference::SegmentedPutRefusal::Put(error)) => {
                format!("put {:?}", ref_lens_reason(error))
            }
            reference::RunStatus::Failed(reference::SegmentedPutRefusal::Segment(error)) => {
                format!("segment {error:?}")
            }
        };
        Seen {
            status,
            locality: format!("{:?}", report.locality.put),
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

    /// A view held as segments: its sealed manifest, each segment's bytes
    /// and each segment's complement bytes.
    struct Held {
        manifest: SegmentManifest,
        views: BTreeMap<SegmentId, Vec<u8>>,
        complements: BTreeMap<SegmentId, Vec<u8>>,
    }

    impl Held {
        fn of(
            segments: &[GraphSegment],
            complements: &[panproto_inst::segment::GraphComplementSegment],
            partition: &Partition,
        ) -> Self {
            let schema = Digest(VIEW_SCHEMA);
            let any: Vec<AnySegment> = segments.iter().cloned().map(AnySegment::Graph).collect();
            let records = any
                .iter()
                .zip(partition.specs())
                .map(|(segment, spec)| SegmentRecord::of(segment, spec, &schema))
                .collect();
            let runs: Vec<EdgeIdRun<'_>> = any
                .iter()
                .map(|segment| {
                    (
                        segment.segment(),
                        Box::new(segment.edge_id_run().into_iter())
                            as Box<dyn Iterator<Item = panproto_inst::EdgeId>>,
                    )
                })
                .collect();
            let sources = any
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
            .expect("the view seals");
            Self {
                manifest,
                views: any
                    .iter()
                    .map(|segment| {
                        (
                            segment.segment(),
                            segment
                                .canonical_bytes(&schema, partition)
                                .expect("view bytes"),
                        )
                    })
                    .collect(),
                complements: complements
                    .iter()
                    .map(|complement| {
                        (
                            complement.segment,
                            complement.canonical_bytes().expect("complement bytes"),
                        )
                    })
                    .collect(),
            }
        }

        fn input(
            &self,
        ) -> HeldView<'_, BTreeMap<SegmentId, Vec<u8>>, BTreeMap<SegmentId, Vec<u8>>> {
            HeldView {
                manifest: &self.manifest,
                input: self.manifest.digest(),
                views: &self.views,
                complements: &self.complements,
            }
        }
    }

    /// One put of the carrier on both sides.
    struct Case<'a> {
        lens: &'a Lens,
        reference_lens: &'a RefGraphLens<'a>,
        held: &'a Held,
        reference_views: &'a [reference::GraphSegment],
        reference_complements: &'a [reference::GraphComplementSegment],
        partition: &'a reference::Partition,
    }

    impl Case<'_> {
        fn production<S: SegmentStore>(
            &self,
            budget: Budget,
            store: &mut S,
            cancel: Option<u32>,
        ) -> GraphPutRun {
            graph_lens::put_segmented_run(self.lens, &self.held.input(), budget, store, cancel)
                .expect("the carrier's plan compiles")
        }

        fn reference(
            &self,
            budget: Budget,
            store: &mut reference::MemoryStore,
            cancel: Option<u32>,
        ) -> reference::PutRun {
            let identity = self.lens.identity(ComplementShape::Graph);
            reference::segmented_put_with_progress(
                self.reference_lens,
                (self.reference_views, self.reference_complements),
                self.partition,
                &reference::PutRunSpec {
                    lens: reference::Digest(identity.lens.0),
                    view_schema: reference::Digest(VIEW_SCHEMA),
                    view_role: reference::ManifestRole::Source,
                    source_schema: reference::Digest(identity.source_schema.0),
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
        get_refused: u64,
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

    /// Check the run law on one put under `budget`: the production report
    /// equals the reference runner's uninterrupted and at every
    /// cancellation point, a resumed run ends as the uninterrupted one
    /// reusing every task the stopped run committed, a complete run emits
    /// the restored segments of in-memory segmented put and of the
    /// reference, and a refused or declined run seals nothing.
    fn check_put(case: &Case<'_>, budget: Budget, every_point: bool, tally: &mut Tally) {
        let mut store = MemoryStore::default();
        let baseline = case.production(budget, &mut store, None);
        let mut reference_store = reference::MemoryStore::default();
        let expected = case.reference(budget, &mut reference_store, None);
        assert_eq!(
            seen_production(&baseline.report),
            seen_reference(&expected.report),
            "uninterrupted put under {:?}",
            case.partition
        );
        let run = baseline.run.expect("a run identity");
        assert_eq!(run.0, expected.run.0, "run identity");
        match &baseline.report.status {
            RunStatus::Complete { manifest } => {
                tally.complete += 1;
                assert_eq!(
                    baseline.manifest.as_ref().map(SegmentManifest::digest),
                    Some(*manifest)
                );
                let partition = prod_partition(case.partition);
                let restored: Vec<GraphSegment> = partition
                    .specs()
                    .iter()
                    .map(|spec| {
                        let emitted = read_emitted(&store, run, &partition, spec.id)
                            .expect("an emitted segment");
                        assert_eq!(
                            emitted.complement,
                            Vec::<u8>::new(),
                            "put emits no complement"
                        );
                        emitted.view
                    })
                    .collect();
                let schema = case.lens.identity(ComplementShape::Graph).source_schema;
                for (segment, expected) in restored
                    .iter()
                    .zip(expected.output.as_ref().expect("output"))
                {
                    assert_eq!(
                        AnySegment::Graph(segment.clone())
                            .canonical_bytes(&schema, &partition)
                            .expect("restored bytes"),
                        reference::canonical_segment_bytes(
                            &reference::AnySegment::Graph(expected.clone()),
                            reference::Digest(schema.0),
                            case.partition,
                        )
                        .expect("reference restored bytes"),
                        "restored segment under {:?}",
                        case.partition
                    );
                }
            }
            RunStatus::Failed(error) => {
                tally.refused += 1;
                if matches!(error, SegmentedPutError::Lens(error) if refused_by_the_next_get(error))
                {
                    tally.get_refused += 1;
                }
                assert!(sealed_nothing(&store, run), "a refused put seals nothing");
            }
            RunStatus::Residual(_) => {
                tally.declined += 1;
                assert!(sealed_nothing(&store, run), "a declined put seals nothing");
            }
            RunStatus::Incomplete { reason } => panic!("an uninterrupted put stopped: {reason:?}"),
        }
        if every_point {
            check_cancellations(case, budget, &baseline, run, tally);
        }
    }

    /// Stopping the put of `case` at every cancellation point and resuming
    /// it reports what the reference runner reports, and ends as `baseline`.
    fn check_cancellations(
        case: &Case<'_>,
        budget: Budget,
        baseline: &GraphPutRun,
        run: Digest,
        tally: &mut Tally,
    ) {
        let total = baseline.report.progress.tasks_total;
        let done = baseline.report.progress.tasks_done;
        for cancel in 0..=total {
            tally.cancellations += 1;
            let mut store = MemoryStore::default();
            let interrupted = case.production(budget, &mut store, Some(cancel));
            let mut reference_store = reference::MemoryStore::default();
            let stopped = case.reference(budget, &mut reference_store, Some(cancel));
            assert_eq!(
                seen_production(&interrupted.report),
                seen_reference(&stopped.report),
                "put cancelled after {cancel} tasks"
            );
            let resumed = case.production(budget, &mut store, None);
            let again = case.reference(budget, &mut reference_store, None);
            assert_eq!(
                seen_production(&resumed.report),
                seen_reference(&again.report),
                "put resumed after {cancel} tasks"
            );
            assert_eq!(resumed.report.tasks_reused, cancel.min(done));
            assert_eq!(
                format!("{:?}", resumed.report.status),
                format!("{:?}", baseline.report.status)
            );
            assert_eq!(resumed.manifest, baseline.manifest);
            assert_eq!(
                store.staged_uncommitted(&run).expect("the journal reads"),
                Vec::<ArtifactKey>::new()
            );
        }
    }

    /// The puts of one carrier case: its segmented get's view and every
    /// edit of it, against the get's complement segments.
    fn check_case(
        schema: &RefSchema,
        migration: &RefMigration,
        graph: &RefGraph,
        partition: &reference::Partition,
        every_point: bool,
        tally: &mut Tally,
    ) {
        let lens = lens_of(schema, migration);
        let prod_p = prod_partition(partition);
        let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
            .expect("the carrier compiles");
        let source = partition_graph(&prod_graph(graph), &prod_p).expect("production split");
        let Ok(got) = graph_restrict_segmented(&plan, &source, &prod_p) else {
            return;
        };
        let (reference_views, reference_complements) =
            reference_get(schema, migration, graph, partition);
        let reference_plan = panproto_reference::restrict::compile(schema, schema, migration)
            .expect("the reference plan compiles");
        let reference_lens = RefGraphLens::new(
            schema,
            schema,
            migration,
            &reference_plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .expect("the reference lens");
        let view = reference::assemble_graph(&reference_views, partition).expect("glued view");
        for edited in edits(&view, partition) {
            let views = partition_graph(&prod_graph(&edited), &prod_p).expect("edited split");
            let reference_edited =
                reference::partition_graph(&edited, partition).expect("reference edited split");
            let held = Held::of(&views, &got.complements, &prod_p);
            let case = Case {
                lens: &lens,
                reference_lens: &reference_lens,
                held: &held,
                reference_views: &reference_edited,
                reference_complements: &reference_complements,
                partition,
            };
            check_put(&case, Budget::unlimited(), every_point, tally);
            let in_memory = graph_lens::put_segmented(&lens, &views, &got.complements, &prod_p);
            let mut store = MemoryStore::default();
            let run = case.production(Budget::unlimited(), &mut store, None);
            match (&run.report.status, &in_memory) {
                (RunStatus::Complete { .. }, Ok(_)) => {}
                (
                    RunStatus::Failed(SegmentedPutError::Lens(run)),
                    Err(SegmentedPutError::Lens(held)),
                ) => {
                    assert_eq!(lens_reason(run), lens_reason(held), "{edited:?}");
                }
                outcomes => {
                    panic!("the run and in-memory put disagree on {edited:?}: {outcomes:?}")
                }
            }
        }
    }

    fn sweep(select: impl Fn(usize) -> bool, every_point: bool) -> (usize, usize, Tally) {
        let mut tally = Tally::default();
        let (mut cases, mut selected) = (0, 0);
        for (position, case) in segmented_restrict_cases().step_by(97).enumerate() {
            cases += 1;
            if !select(position) {
                continue;
            }
            selected += 1;
            check_case(
                &case.schema,
                &case.migration,
                &case.graph,
                &case.partition,
                every_point,
                &mut tally,
            );
        }
        (cases, selected, tally)
    }

    /// Every 97th case of the segmented restriction carrier with every edit
    /// of its segmented view, put back as a checkpointed run: the report
    /// equals the reference runner's uninterrupted and at every
    /// cancellation point, a resumed run ends as the uninterrupted one, a
    /// complete run emits the reference's restored segments and agrees with
    /// in-memory segmented put, and a refused put seals nothing.
    /// `PANPROTO_TEST_SHARD=i/n` restricts the check to the cases at
    /// positions `i` modulo `n`.
    #[test]
    fn segmented_graph_put_run_matches_the_reference_runner_at_every_cancellation_point() {
        let shard = super::LawShard::from_environment();
        let (cases, selected, tally) = sweep(
            |position| shard.is_none_or(|shard| shard.contains(position)),
            true,
        );
        assert_eq!(cases, 1_323);
        assert_eq!(selected, shard.map_or(cases, |shard| shard.share(cases)));
        assert!(
            tally.complete > 0
                && tally.refused > 0
                && tally.get_refused > 0
                && tally.cancellations > 0,
            "{tally:?}"
        );
    }

    /// The certificate's cases at positions divisible by 64, uninterrupted
    /// and from every cancellation point, checked on every run.
    #[test]
    fn segmented_graph_put_run_slice_matches_the_reference_runner() {
        let (cases, selected, tally) = sweep(|position| position % 64 == 0, true);
        assert_eq!((cases, selected), (1_323, 21));
        assert!(tally.complete > 0 && tally.refused > 0, "{tally:?}");
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(16))]

        #[test]
        fn segmented_graph_put_run_matches_the_reference_runner_on_generated_partitions(
            anchors in proptest::collection::vec(0_usize..3, 2..=6),
            edges in proptest::collection::vec((0_usize..6, 0_usize..6), 0..=8),
            owners in proptest::collection::vec(0_u32..4, 6),
            segments in 2_u32..=3,
            surviving in 0_u8..8,
        ) {
            let (schema, migration, graph, partition) =
                generated(&anchors, &edges, &owners, segments, surviving);
            check_case(&schema, &migration, &graph, &partition, false, &mut Tally::default());
        }
    }

    /// The witness case: a three-segment carrier case with an edge crossing
    /// segments, its unedited view held with its get's complements.
    fn witness(tally: &mut Tally, check: &mut dyn FnMut(&Case<'_>, &mut Tally)) {
        let case = segmented_restrict_cases()
            .find(|case| {
                case.partition.specs.len() == 3
                    && case.graph.edges.len() >= 2
                    && case.migration.surviving_verts.len() == 3
            })
            .expect("a three-segment case keeping every vertex");
        let lens = lens_of(&case.schema, &case.migration);
        let prod_p = prod_partition(&case.partition);
        let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
            .expect("the carrier compiles");
        let source = partition_graph(&prod_graph(&case.graph), &prod_p).expect("split");
        let got = graph_restrict_segmented(&plan, &source, &prod_p).expect("get");
        let (reference_views, reference_complements) =
            reference_get(&case.schema, &case.migration, &case.graph, &case.partition);
        let reference_plan =
            panproto_reference::restrict::compile(&case.schema, &case.schema, &case.migration)
                .expect("the reference plan");
        let reference_lens = RefGraphLens::new(
            &case.schema,
            &case.schema,
            &case.migration,
            &reference_plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .expect("the reference lens");
        let held = Held::of(&got.views, &got.complements, &prod_p);
        check(
            &Case {
                lens: &lens,
                reference_lens: &reference_lens,
                held: &held,
                reference_views: &reference_views,
                reference_complements: &reference_complements,
                partition: &case.partition,
            },
            tally,
        );
    }

    /// Each budget below what a put needs declines it where the reference
    /// runner declines it, with equal reports at every cancellation point:
    /// the number of segments and the working set (a view segment's estimate
    /// plus its complement's length) before any task, the summary bytes at
    /// Assemble; a complement budget of zero bytes completes, since put
    /// emits no complement.
    #[test]
    fn a_put_run_declines_each_exceeded_budget_like_the_reference() {
        let mut tally = Tally::default();
        witness(&mut tally, &mut |case, tally| {
            let mut store = MemoryStore::default();
            let full = case.production(Budget::unlimited(), &mut store, None);
            let RunStatus::Complete { .. } = full.report.status else {
                panic!("the witness put completes: {:?}", full.report.status);
            };
            let peak = full.report.peak_estimated_bytes;
            let summaries = {
                let mut store = MemoryStore::default();
                let probe = case.production(
                    Budget {
                        summary: 0,
                        ..Budget::unlimited()
                    },
                    &mut store,
                    None,
                );
                match probe.report.status {
                    RunStatus::Residual(Residual::BudgetExceeded {
                        resource: Resource::Summary,
                        needed,
                        ..
                    }) => needed,
                    other => panic!("a zero summary budget declines at Assemble: {other:?}"),
                }
            };
            for (budget, resource) in [
                (
                    Budget {
                        affected_segments: 2,
                        ..Budget::unlimited()
                    },
                    Some(Resource::AffectedSegments),
                ),
                (
                    Budget {
                        working_set: peak - 1,
                        ..Budget::unlimited()
                    },
                    Some(Resource::WorkingSet),
                ),
                (
                    Budget {
                        summary: summaries - 1,
                        ..Budget::unlimited()
                    },
                    Some(Resource::Summary),
                ),
                (
                    Budget {
                        summary: summaries,
                        working_set: peak,
                        complement: Some(0),
                        ..Budget::unlimited()
                    },
                    None,
                ),
            ] {
                let mut store = MemoryStore::default();
                let run = case.production(budget, &mut store, None);
                match (resource, &run.report.status) {
                    (
                        Some(expected),
                        RunStatus::Residual(Residual::BudgetExceeded { resource, .. }),
                    ) => {
                        assert_eq!(*resource, expected);
                    }
                    (None, RunStatus::Complete { .. }) => {}
                    (_, status) => panic!("{budget:?} ends as {status:?}"),
                }
                check_put(case, budget, true, tally);
            }
        });
        assert_eq!((tally.complete, tally.declined), (1, 3), "{tally:?}");
    }

    /// A store whose commit fails at one commit, leaving that artifact
    /// staged.
    #[derive(Default)]
    struct FailingStore {
        inner: MemoryStore,
        fail_at: u32,
        commits: u32,
    }

    #[derive(Debug)]
    struct CommitRefused;

    impl std::fmt::Display for CommitRefused {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("the commit is refused")
        }
    }

    impl std::error::Error for CommitRefused {}

    impl SegmentStore for FailingStore {
        type Error = CommitRefused;

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
            let at = self.commits;
            self.commits += 1;
            if at == self.fail_at {
                return Err(CommitRefused);
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

    /// A commit that fails at any task stops the put with a store failure,
    /// reports the staged artifact of that task as its one orphan, and a
    /// run resumed against the store reuses every committed task and ends
    /// as the uninterrupted run.
    #[test]
    fn a_put_run_stopped_by_a_failed_commit_at_any_task_resumes_to_the_uninterrupted_run() {
        let mut tally = Tally::default();
        witness(&mut tally, &mut |case, tally| {
            let mut store = MemoryStore::default();
            let baseline = case.production(Budget::unlimited(), &mut store, None);
            let total = baseline.report.progress.tasks_total;
            assert_eq!(baseline.report.progress.tasks_done, total);
            for fail_at in 0..total {
                let mut failing = FailingStore {
                    fail_at,
                    ..FailingStore::default()
                };
                let stopped = case.production(Budget::unlimited(), &mut failing, None);
                assert!(
                    matches!(
                        &stopped.report.status,
                        RunStatus::Incomplete {
                            reason: RunIncompleteReason::Store { message }
                        } if message == "the commit is refused"
                    ),
                    "{:?}",
                    stopped.report.status
                );
                assert_eq!(stopped.report.progress.tasks_done, fail_at);
                assert_eq!(
                    stopped.report.orphans.len(),
                    1,
                    "the failed task's artifact"
                );
                let mut store = failing.inner;
                let resumed = case.production(Budget::unlimited(), &mut store, None);
                assert_eq!(resumed.report.tasks_reused, fail_at);
                assert_eq!(resumed.manifest, baseline.manifest);
                assert_eq!(
                    format!("{:?}", resumed.report.status),
                    format!("{:?}", baseline.report.status)
                );
                assert_eq!(resumed.report.orphans, Vec::<ArtifactKey>::new());
                tally.cancellations += 1;
            }
        });
        assert!(tally.cancellations >= 14, "{tally:?}");
    }
}

/// The segmented graph edit lens against its reference definition and
/// against the whole-state graph edit lens, by renaming synthesized ids by
/// synthesis key.
mod segmented_graph_edit_lens {
    use std::collections::{BTreeMap, BTreeSet};

    use panproto_inst::identity::ComplementShape;
    use panproto_inst::identity::Digest;
    use panproto_inst::segment::{
        AnySegment, ArtifactKey, Budget, FamilyIndex, GRAPH_APPLY_OP, MemoryStore, MissingSegment,
        Phase, Residual, RunJournal, SegmentId, SegmentStore, TaskKey, TaskRecord, read_emitted,
    };
    use panproto_lens::{Lens, SegmentedGraph, SegmentedGraphEditLens, ShapeEditError};
    use panproto_reference::edit::{
        self as ref_edit, GElem, GPayload, GraphDelta as RefGraphDelta,
    };
    use panproto_reference::enumerate::edits::{
        graph_edit_lens_letter_count, graph_edit_lens_letters,
    };
    use panproto_reference::enumerate::segments::segmented_restrict_cases;
    use panproto_reference::restrict::NoValueLayer;
    use panproto_reference::segment as reference;

    use super::graph_edit_lens_differential::{prod_delta, ref_delta};
    use super::segmented_graph_get::{Input, SOURCE_SCHEMA, input, lens_of};
    use super::{
        BOUNDED_LENS_FIELD, BOUNDED_LENS_VALUE, RefComplement, RefGraph, RefMigration,
        RefRestrictError, RefSchema,
    };

    /// The bound lenses of a case.
    type Bound = SegmentedGraphEditLens<BTreeMap<SegmentId, Vec<u8>>, MemoryStore>;

    /// `partition` with the ids from `from` to 99 given to its last segment,
    /// so a node an edit adds has an owner.
    pub fn with_spare(partition: &reference::Partition, from: u32) -> reference::Partition {
        let mut spared = partition.clone();
        if let Some(last) = spared.specs.last_mut() {
            last.nodes.push(reference::IdRange { lo: from, hi: 99 });
            last.nodes.sort_by_key(|range| range.lo);
        }
        spared
    }

    /// The lens, the restriction and the partition of one case.
    pub struct Case<'a> {
        /// The schema, source and target alike.
        pub schema: &'a RefSchema,
        /// The migration.
        pub migration: &'a RefMigration,
        /// The partition, with spare ids.
        pub partition: &'a reference::Partition,
    }

    impl Case<'_> {
        fn restriction(&self) -> reference::EditRestriction<'_> {
            reference::EditRestriction {
                src: self.schema,
                tgt: self.schema,
                m: self.migration,
                vl: &NoValueLayer,
            }
        }

        pub fn lens(&self) -> Lens {
            lens_of(self.schema, self.migration)
        }

        fn whole_get(
            &self,
            graph: &RefGraph,
        ) -> Result<(RefGraph, RefComplement), RefRestrictError> {
            panproto_reference::restrict::get(
                self.schema,
                self.schema,
                self.migration,
                &NoValueLayer,
                graph,
            )
        }
    }

    /// Every segment's canonical bytes in the production lens: source
    /// segments under the source schema, view segments under the view
    /// schema, and complement segments.
    pub fn production_state<St: SegmentStore>(
        lens: &SegmentedGraphEditLens<BTreeMap<SegmentId, Vec<u8>>, St>,
        view_schema: Digest,
    ) -> Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        let state = lens.segmented_state();
        let partition = state.view_manifest().partition().expect("a partition");
        partition
            .specs()
            .iter()
            .map(|spec| {
                let source = state.source_segment(spec.id).expect("a source segment");
                let emitted = read_emitted(&*state.store(), state.get_run(), &partition, spec.id)
                    .expect("an emitted segment");
                let view = AnySegment::Graph(emitted.view)
                    .canonical_bytes(&view_schema, &partition)
                    .expect("view bytes");
                (source, view, emitted.complement)
            })
            .collect()
    }

    /// The same bytes of a reference state.
    pub fn reference_state(
        state: &reference::SegmentedEditState,
        partition: &reference::Partition,
        view_schema: panproto_inst::identity::Digest,
    ) -> Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        let bytes = |segment: &reference::GraphSegment, schema: [u8; 32]| {
            reference::canonical_segment_bytes(
                &reference::AnySegment::Graph(segment.clone()),
                reference::Digest(schema),
                partition,
            )
            .expect("reference segment bytes")
        };
        state
            .source
            .iter()
            .zip(&state.views)
            .zip(&state.complements)
            .map(|((source, view), complement)| {
                (
                    bytes(source, SOURCE_SCHEMA),
                    bytes(view, view_schema.0),
                    reference::canonical_complement_segment_bytes(
                        &reference::AnyComplementSegment::Graph(complement.clone()),
                    )
                    .expect("reference complement bytes"),
                )
            })
            .collect()
    }

    /// The renaming of synthesized ids by synthesis key from a whole-state
    /// complement to segmented complement segments, nodes then edges.
    type Renaming = (BTreeMap<u32, u32>, BTreeMap<u32, u32>);

    fn renaming(whole: &RefComplement, segments: &[reference::GraphComplementSegment]) -> Renaming {
        let by_key = |pick: &dyn Fn(
            &reference::GraphComplementSegment,
        )
            -> &BTreeMap<u32, panproto_reference::restrict::SynthKey>| {
            segments
                .iter()
                .flat_map(|segment| pick(segment).iter().map(|(id, key)| (key.clone(), *id)))
                .collect::<BTreeMap<_, _>>()
        };
        let nodes = by_key(&|segment| &segment.synthesized_nodes);
        let edges = by_key(&|segment| &segment.synthesized_edges);
        (
            whole
                .synthesized_nodes
                .iter()
                .map(|(id, key)| (*id, nodes[key]))
                .collect(),
            whole
                .synthesized_edges
                .iter()
                .map(|(id, key)| (*id, edges[key]))
                .collect(),
        )
    }

    fn renamed_graph(graph: &RefGraph, (nodes, edges): &Renaming) -> RefGraph {
        let node = |id: u32| nodes.get(&id).copied().unwrap_or(id);
        let mut out = RefGraph::default();
        for (id, held) in &graph.nodes {
            let mut held = held.clone();
            held.id = node(*id);
            out.nodes.insert(held.id, held);
        }
        for (id, edge) in &graph.edges {
            let mut edge = edge.clone();
            edge.src = node(edge.src);
            edge.tgt = node(edge.tgt);
            out.edges
                .insert(edges.get(id).copied().unwrap_or(*id), edge);
        }
        out
    }

    fn renamed_delta(delta: &RefGraphDelta, (nodes, edges): &Renaming) -> RefGraphDelta {
        let node = |id: u32| nodes.get(&id).copied().unwrap_or(id);
        let edge = |id: u32| edges.get(&id).copied().unwrap_or(id);
        let element = |elem: &GElem, payload: &GPayload| match (elem, payload) {
            (GElem::Node(id), GPayload::Node(held)) => {
                let mut held = held.clone();
                held.id = node(*id);
                (GElem::Node(node(*id)), GPayload::Node(held))
            }
            (GElem::Edge(id), GPayload::Edge(held)) => {
                let mut held = held.clone();
                held.src = node(held.src);
                held.tgt = node(held.tgt);
                (GElem::Edge(edge(*id)), GPayload::Edge(held))
            }
            other => panic!("a cross-sort element: {other:?}"),
        };
        let mut out = RefGraphDelta::identity();
        out.del = delta.del.iter().map(|(x, p)| element(x, p)).collect();
        out.add = delta.add.iter().map(|(x, p)| element(x, p)).collect();
        out.attrs = delta
            .attrs
            .iter()
            .map(|(key, change)| {
                let key = match key {
                    ref_edit::AttrKey::Value(n) => ref_edit::AttrKey::Value(node(*n)),
                    ref_edit::AttrKey::Field(n, f) => ref_edit::AttrKey::Field(node(*n), f.clone()),
                    ref_edit::AttrKey::Meta(n) => ref_edit::AttrKey::Meta(node(*n)),
                    ref_edit::AttrKey::Anchor(n) => ref_edit::AttrKey::Anchor(node(*n)),
                    ref_edit::AttrKey::EdgeLabel(e) => ref_edit::AttrKey::EdgeLabel(edge(*e)),
                    ref_edit::AttrKey::EdgePosition(e) => ref_edit::AttrKey::EdgePosition(edge(*e)),
                };
                (key, change.clone())
            })
            .collect();
        out
    }

    /// The entries of a complement other than its synthesized elements and
    /// floors: dropped nodes and edges and recorded originals.
    type Entries = (
        BTreeMap<u32, super::RefNode>,
        BTreeMap<u32, super::RefGEdge>,
        BTreeMap<u32, super::RefGEdge>,
        BTreeMap<u32, String>,
        BTreeMap<u32, BTreeMap<String, super::RefValue>>,
        BTreeMap<u32, Option<super::RefPresence>>,
    );

    fn whole_entries(complement: &RefComplement) -> Entries {
        (
            complement.dropped_nodes.clone(),
            complement.dropped_edges.clone(),
            complement.original_edges.clone(),
            complement.original_anchors.clone(),
            complement.original_fields.clone(),
            complement.original_values.clone(),
        )
    }

    fn joined_entries(segments: &[reference::GraphComplementSegment]) -> Entries {
        let mut out = Entries::default();
        for segment in segments {
            out.0.extend(segment.dropped_nodes.clone());
            out.1.extend(segment.dropped_edges.clone());
            out.2.extend(segment.original_edges.clone());
            out.3.extend(segment.original_anchors.clone());
            out.4.extend(segment.original_fields.clone());
            out.5.extend(segment.original_values.clone());
        }
        out
    }

    /// Whether the union of two renamings of one kind is a bijection: a
    /// function that is injective.
    fn bijective(a: &BTreeMap<u32, u32>, b: &BTreeMap<u32, u32>) -> bool {
        let mut union = a.clone();
        for (id, to) in b {
            if union.get(id).is_some_and(|held| held != to) {
                return false;
            }
            union.insert(*id, *to);
        }
        let images: BTreeSet<u32> = union.values().copied().collect();
        images.len() == union.len()
    }

    /// What a sweep showed.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Counts {
        /// Cases bound on both sides.
        pub bound: u64,
        /// Cases whose segmented get refuses on both sides.
        pub unbound: u64,
        /// Letters translated.
        pub letters: u64,
        /// Letters admitted on both sides.
        pub admitted: u64,
        /// Letters refused alike.
        pub refused: u64,
        /// Admitted letters whose renamings before and after form one
        /// bijection, with the segmented delta the whole-state delta renamed.
        pub single_renaming: u64,
        /// Admitted letters whose renamings do not.
        pub two_renamings: u64,
        /// Letters the segmented lens refuses for its layout while the
        /// whole-state lens admits them.
        pub layout_refused: u64,
    }

    fn production_refusal(error: &ShapeEditError) -> String {
        match error {
            ShapeEditError::Apply(_) => "apply".to_owned(),
            ShapeEditError::Get(_) => "get".to_owned(),
            ShapeEditError::Segment(_) => "segment".to_owned(),
            other => format!("outside the carrier: {other:?}"),
        }
    }

    fn reference_refusal(error: &reference::SegmentedEditRefusal) -> String {
        match error {
            reference::SegmentedEditRefusal::Apply(reference::SegmentedApplyRefusal::Edit(_)) => {
                "apply".to_owned()
            }
            reference::SegmentedEditRefusal::Put(reference::SegmentedPutRefusal::Put(_)) => {
                "put".to_owned()
            }
            reference::SegmentedEditRefusal::PutGetMismatch => "put-get mismatch".to_owned(),
            reference::SegmentedEditRefusal::Put(reference::SegmentedPutRefusal::Segment(_)) => {
                "segment".to_owned()
            }
            reference::SegmentedEditRefusal::Restrict(
                reference::SegmentedRestrictRefusal::Restrict { .. }
                | reference::SegmentedRestrictRefusal::Unlocated(_),
            ) => "get".to_owned(),
            reference::SegmentedEditRefusal::Apply(reference::SegmentedApplyRefusal::Segment(
                _,
            ))
            | reference::SegmentedEditRefusal::Restrict(
                reference::SegmentedRestrictRefusal::Segment(_),
            )
            | reference::SegmentedEditRefusal::Segment(_) => "segment".to_owned(),
        }
    }

    /// The segments the runs of an admitted edit read, given the lens
    /// `before` it and `after` it: the edit run's read set in segment order,
    /// then every segment whose source record changed, each but those whose
    /// Validate task the store already journaled under the same run, which
    /// the run reuses rather than reads.
    fn expected_reads(
        before: &Bound,
        after: &Bound,
        delta: &panproto_inst::GraphDelta,
    ) -> Vec<SegmentId> {
        let (old, new) = (before.segmented_state(), after.segmented_state());
        let (source, edited) = (old.source_manifest(), new.source_manifest());
        let partition = source.partition().expect("a partition");
        let journaled = |run: Digest, segment: SegmentId| {
            old.store()
                .journal(&run)
                .expect("a journal")
                .tasks
                .contains_key(&TaskKey {
                    phase: Phase::Validate,
                    segment: Some(segment),
                })
        };
        let apply = RunJournal::run_digest(
            &delta.digest(),
            &source.digest(),
            &partition,
            GRAPH_APPLY_OP,
        );
        let mut reads: Vec<SegmentId> = FamilyIndex::of_manifest(source)
            .expect("an index")
            .read_set(delta)
            .into_iter()
            .filter(|segment| !journaled(apply, *segment))
            .collect();
        reads.extend(
            edited
                .segments
                .iter()
                .filter(|record| source.record(record.id) != Some(*record))
                .map(|record| record.id)
                .filter(|segment| !journaled(new.get_run(), *segment)),
        );
        reads
    }

    /// Translate one source letter through a clone of the bound lens and the
    /// reference, check the outcome and the law against the whole-state
    /// lens, and return the advanced lens and states when admitted.
    pub fn step(
        case: &Case<'_>,
        (production, state, whole): (
            &Bound,
            &reference::SegmentedEditState,
            &ref_edit::GraphEditState,
        ),
        letter: &RefGraphDelta,
        counts: &mut Counts,
    ) -> Option<(
        Bound,
        reference::SegmentedEditState,
        ref_edit::GraphEditState,
    )> {
        let view_schema = case.lens().identity(ComplementShape::Graph).target_schema;
        counts.letters += 1;
        let mut advanced = production.clone();
        let edit = prod_delta(letter);
        let produced = advanced.get_edit(&edit);
        let expected =
            reference::segmented_edit_get(case.restriction(), state, case.partition, letter);
        let whole_get = |graph: &RefGraph| case.whole_get(graph);
        let whole_next =
            ref_edit::graph_edit_get::<RefRestrictError, ()>(&whole_get, whole, letter);
        match (produced, expected) {
            (Ok(step), Ok((delta, next))) => {
                counts.admitted += 1;
                assert_eq!(
                    ref_delta(&step.view_delta),
                    delta,
                    "view delta of {letter:?}"
                );
                assert_eq!(
                    production_state(&advanced, view_schema),
                    reference_state(&next, case.partition, view_schema),
                    "segments after {letter:?}"
                );
                assert_eq!(
                    advanced.segmented_state().reads(),
                    expected_reads(production, &advanced, &edit),
                    "segments read by {letter:?}"
                );
                let (whole_delta, whole_after) =
                    whole_next.expect("the whole-state lens admits it");
                let before = renaming(&whole.complement, &state.complements);
                let after = renaming(&whole_after.complement, &next.complements);
                let glued =
                    reference::assemble_graph(&next.views, case.partition).expect("views glue");
                assert_eq!(
                    renamed_graph(&whole_after.view, &after),
                    glued,
                    "view after {letter:?}"
                );
                assert_eq!(
                    joined_entries(&next.complements),
                    whole_entries(&whole_after.complement),
                    "complement after {letter:?}"
                );
                assert_eq!(step.source_delta, edit, "the source delta is the input");
                if bijective(&before.0, &after.0) && bijective(&before.1, &after.1) {
                    counts.single_renaming += 1;
                    let mut union = before;
                    union.0.extend(after.0.clone());
                    union.1.extend(after.1);
                    assert_eq!(
                        renamed_delta(&whole_delta, &union),
                        delta,
                        "renamed delta of {letter:?}"
                    );
                } else {
                    counts.two_renamings += 1;
                }
                Some((advanced, next, whole_after))
            }
            (Err(error), Err(refusal)) => {
                counts.refused += 1;
                assert_eq!(
                    production_refusal(&error),
                    reference_refusal(&refusal),
                    "{letter:?}"
                );
                assert_eq!(
                    production_state(&advanced, view_schema),
                    production_state(production, view_schema),
                    "a refused edit changed a segment"
                );
                assert_eq!(
                    advanced.segmented_state().source_manifest(),
                    production.segmented_state().source_manifest()
                );
                assert_eq!(
                    advanced.segmented_state().get_run(),
                    production.segmented_state().get_run()
                );
                assert_eq!(advanced.sequence(), production.sequence());
                if matches!(error, ShapeEditError::Segment(_)) && whole_next.is_ok() {
                    counts.layout_refused += 1;
                } else {
                    assert!(
                        whole_next.is_err(),
                        "the whole-state lens refuses {letter:?}"
                    );
                }
                None
            }
            (produced, expected) => panic!(
                "segmented edit lens disagrees on {letter:?}: {:?} / {expected:?}",
                produced.map(|step| step.view_delta)
            ),
        }
    }

    /// Bind both sides and the whole-state lens to `graph`, or `None` when
    /// segmented get refuses it (on both sides alike).
    pub fn bind(
        case: &Case<'_>,
        graph: &RefGraph,
    ) -> Option<(
        Bound,
        reference::SegmentedEditState,
        ref_edit::GraphEditState,
    )> {
        let Input {
            manifest, bytes, ..
        } = input(graph, case.partition);
        let segments = reference::partition_graph(graph, case.partition).expect("reference split");
        let production = Bound::segmented(
            case.lens(),
            Budget::unlimited(),
            SegmentedGraph {
                input: manifest.digest(),
                manifest,
                segments: bytes,
                store: MemoryStore::default(),
            },
        );
        let expected =
            reference::bind_segmented_edit(case.restriction(), &segments, case.partition);
        let (production, state) = match (production, expected) {
            (Ok(production), Ok(state)) => (production, state),
            (Err(ShapeEditError::Get(_)), Err(_)) => return None,
            (production, expected) => panic!(
                "segmented bind disagrees on {graph:?}: {:?} / {expected:?}",
                production.map(|_| ())
            ),
        };
        let view_schema = case.lens().identity(ComplementShape::Graph).target_schema;
        assert_eq!(
            production_state(&production, view_schema),
            reference_state(&state, case.partition, view_schema)
        );
        let (view, complement) = case.whole_get(graph).expect("whole-state get accepts");
        Some((
            production,
            state,
            ref_edit::GraphEditState {
                source: graph.clone(),
                view,
                complement,
            },
        ))
    }

    /// Bind one case and translate every source letter of its graph, and
    /// the inverse of each admitted one after it.
    pub fn check_case(case: &Case<'_>, graph: &RefGraph, counts: &mut Counts) {
        let Some((production, state, whole)) = bind(case, graph) else {
            counts.unbound += 1;
            return;
        };
        counts.bound += 1;
        let letters =
            graph_edit_lens_letters(case.schema, graph, BOUNDED_LENS_FIELD, &BOUNDED_LENS_VALUE);
        assert_eq!(
            letters.len(),
            graph_edit_lens_letter_count(case.schema, graph)
        );
        for letter in &letters {
            if let Some((advanced, next, whole_next)) =
                step(case, (&production, &state, &whole), letter, counts)
            {
                step(
                    case,
                    (&advanced, &next, &whole_next),
                    &letter.invert(),
                    counts,
                );
            }
        }
    }

    #[test]
    fn segmented_graph_edit_lens_agrees_with_the_reference_and_the_whole_state_lens_on_every_97th_case()
     {
        let mut counts = Counts::default();
        let mut cases = 0_u64;
        for case in segmented_restrict_cases().step_by(97) {
            cases += 1;
            let partition = with_spare(&case.partition, 3);
            check_case(
                &Case {
                    schema: &case.schema,
                    migration: &case.migration,
                    partition: &partition,
                },
                &case.graph,
                &mut counts,
            );
        }
        assert_eq!(cases, 128_240_u64.div_ceil(97));
        assert_eq!(
            counts,
            Counts {
                bound: 1_163,
                unbound: 160,
                letters: 59_115,
                admitted: 53_782,
                refused: 5_333,
                single_renaming: 53_734,
                two_renamings: 48,
                layout_refused: 0,
            }
        );
    }

    #[test]
    fn a_lens_with_an_expression_is_declined_when_bound() {
        let case = segmented_restrict_cases().next().expect("a case");
        let partition = with_spare(&case.partition, 3);
        let mut lens = lens_of(&case.schema, &case.migration);
        let vertex = lens
            .src_schema
            .vertices
            .keys()
            .next()
            .cloned()
            .expect("a vertex");
        lens.compiled.conditional_survival.insert(
            vertex,
            panproto_expr::Expr::Lit(panproto_expr::Literal::Bool(true)),
        );
        let Input {
            manifest, bytes, ..
        } = input(&case.graph, &partition);
        let bound = Bound::segmented(
            lens,
            Budget::unlimited(),
            SegmentedGraph {
                input: manifest.digest(),
                manifest,
                segments: bytes,
                store: MemoryStore::default(),
            },
        );
        assert!(
            matches!(
                bound,
                Err(ShapeEditError::Residual(Residual::Global { .. }))
            ),
            "{:?}",
            bound.map(|_| ())
        );
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

        #[test]
        fn segmented_graph_edit_lens_agrees_with_the_reference_on_generated_words(
            anchors in proptest::collection::vec(0_usize..3, 2..=8),
            edges in proptest::collection::vec((0_usize..8, 0_usize..8), 0..=14),
            owners in proptest::collection::vec(0_u32..4, 8),
            segments in 2_u32..=4,
            surviving in 0_u8..8,
            word in proptest::collection::vec(proptest::prelude::any::<usize>(), 1..6),
        ) {
            let (schema, migration, graph, partition) =
                super::segmented_graph_get::generated(&anchors, &edges, &owners, segments, surviving);
            let from = u32::try_from(anchors.len()).expect("a small count");
            let partition = with_spare(&partition, from);
            let case = Case { schema: &schema, migration: &migration, partition: &partition };
            let Some((mut production, mut state, mut whole)) = bind(&case, &graph) else {
                return Ok(());
            };
            let mut counts = Counts::default();
            for choice in &word {
                let source = reference::assemble_graph(&state.source, &partition).expect("the source glues");
                let letters = graph_edit_lens_letters(&schema, &source, BOUNDED_LENS_FIELD, &BOUNDED_LENS_VALUE);
                let letter = &letters[choice % letters.len()];
                if let Some(next) = step(&case, (&production, &state, &whole), letter, &mut counts) {
                    (production, state, whole) = next;
                }
            }
            proptest::prop_assert_eq!(counts.letters, u64::try_from(word.len()).expect("a count"));
        }
    }

    /// A store whose `fail_at`th commit, counted by `commits`, fails.
    #[derive(Clone, Default)]
    struct FailingStore {
        inner: MemoryStore,
        commits: std::rc::Rc<std::cell::Cell<usize>>,
        fail_at: std::rc::Rc<std::cell::Cell<Option<usize>>>,
    }

    impl SegmentStore for FailingStore {
        type Error = MissingSegment;

        fn read(&self, key: &ArtifactKey) -> Result<Option<Vec<u8>>, MissingSegment> {
            Ok(self.inner.read(key).unwrap_or_else(|never| match never {}))
        }

        fn read_at(
            &self,
            key: &ArtifactKey,
            offset: u64,
            len: u64,
        ) -> Result<Option<Vec<u8>>, MissingSegment> {
            Ok(self
                .inner
                .read_at(key, offset, len)
                .unwrap_or_else(|never| match never {}))
        }

        fn stage(&mut self, key: &ArtifactKey, bytes: Vec<u8>) -> Result<(), MissingSegment> {
            self.inner
                .stage(key, bytes)
                .unwrap_or_else(|never| match never {});
            Ok(())
        }

        fn commit(&mut self, key: &ArtifactKey, record: &TaskRecord) -> Result<(), MissingSegment> {
            let count = self.commits.get() + 1;
            self.commits.set(count);
            if self.fail_at.get() == Some(count) {
                return Err(MissingSegment(SegmentId(u32::MAX)));
            }
            self.inner
                .commit(key, record)
                .unwrap_or_else(|never| match never {});
            Ok(())
        }

        fn journal(&self, run: &Digest) -> Result<RunJournal, MissingSegment> {
            Ok(self
                .inner
                .journal(run)
                .unwrap_or_else(|never| match never {}))
        }

        fn staged_uncommitted(&self, run: &Digest) -> Result<Vec<ArtifactKey>, MissingSegment> {
            Ok(self
                .inner
                .staged_uncommitted(run)
                .unwrap_or_else(|never| match never {}))
        }
    }

    /// An edit whose commit fails at each of its tasks leaves the lens and
    /// every segment as they were and reports the stopped run; translating
    /// the same edit again resumes it and ends where the uninterrupted edit
    /// ends, with nothing left staged.
    #[test]
    fn an_edit_stopped_by_a_failed_commit_at_each_task_resumes_to_the_uninterrupted_edit() {
        let (schema, migration, graph, partition) = super::segmented_graph_get::generated(
            &[0, 2, 2, 2, 1],
            &[(0, 1), (1, 2), (2, 3), (3, 1), (3, 4)],
            &[0, 1, 2, 0, 1],
            3,
            0b011,
        );
        let partition = with_spare(&partition, 5);
        let case = Case {
            schema: &schema,
            migration: &migration,
            partition: &partition,
        };
        let Input {
            manifest, bytes, ..
        } = input(&graph, &partition);
        let store = FailingStore::default();
        let (commits, fail_at) = (store.commits.clone(), store.fail_at.clone());
        let bound = SegmentedGraphEditLens::segmented(
            case.lens(),
            Budget::unlimited(),
            SegmentedGraph {
                input: manifest.digest(),
                manifest,
                segments: bytes,
                store,
            },
        )
        .expect("the case binds");
        let edit =
            graph_edit_lens_letters(&schema, &graph, BOUNDED_LENS_FIELD, &BOUNDED_LENS_VALUE)
                .iter()
                .map(prod_delta)
                .find(|edit| !edit.del_edges().is_empty())
                .expect("an edge deletion");
        let mut baseline = bound.clone();
        let before = commits.get();
        let expected = baseline
            .get_edit(&edit)
            .expect("the edge deletion is admitted");
        let total = commits.get() - before;
        assert_eq!(
            total, 28,
            "the edit run commits 4k + 2 tasks for its read set of k = 3 segments, and the restriction 4n + 2 for its n = 3"
        );
        let view_schema = case.lens().identity(ComplementShape::Graph).target_schema;
        for failing in 1..=total {
            let mut lens = bound.clone();
            fail_at.set(Some(commits.get() + failing));
            let stopped = lens.get_edit(&edit);
            assert!(
                matches!(stopped, Err(ShapeEditError::Incomplete(_))),
                "{:?}",
                stopped.map(|step| step.view_delta)
            );
            assert_eq!(
                production_state(&lens, view_schema),
                production_state(&bound, view_schema)
            );
            assert_eq!(
                lens.segmented_state().get_run(),
                bound.segmented_state().get_run()
            );
            assert_eq!(lens.sequence(), 0);
            fail_at.set(None);
            let resumed = lens.get_edit(&edit).expect("the retried edit is admitted");
            assert_eq!(resumed.view_delta, expected.view_delta);
            assert_eq!(
                production_state(&lens, view_schema),
                production_state(&baseline, view_schema)
            );
            let run = lens.segmented_state().get_run();
            assert_eq!(
                lens.segmented_state()
                    .store()
                    .staged_uncommitted(&run)
                    .expect("the journal reads"),
                Vec::<ArtifactKey>::new()
            );
        }
    }

    /// What the put sweep showed.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct PutCounts {
        /// Cases bound on both sides.
        pub bound: u64,
        /// View letters translated.
        pub letters: u64,
        /// Letters admitted on both sides.
        pub admitted: u64,
        /// Letters refused alike.
        pub refused: u64,
        /// Admitted letters compared with the whole-state lens through the
        /// renaming before the edit.
        pub whole: u64,
    }

    fn production_put_refusal(error: &ShapeEditError) -> String {
        match error {
            ShapeEditError::Put(_) => "put".to_owned(),
            ShapeEditError::PutGetMismatch => "put-get mismatch".to_owned(),
            other => production_refusal(other),
        }
    }

    /// `letter` with every synthesized id the renaming `rho` gives back
    /// renamed to its whole-state id, when no other id the letter names is
    /// a whole-state synthesized id; `None` otherwise.
    fn whole_letter(letter: &RefGraphDelta, rho: &Renaming) -> Option<RefGraphDelta> {
        let invert = |map: &BTreeMap<u32, u32>| -> BTreeMap<u32, u32> {
            map.iter().map(|(from, to)| (*to, *from)).collect()
        };
        let back = (invert(&rho.0), invert(&rho.1));
        let named = |elem: &GElem| match elem {
            GElem::Node(id) => rho.0.contains_key(id) && !back.0.contains_key(id),
            GElem::Edge(id) => rho.1.contains_key(id) && !back.1.contains_key(id),
        };
        let clash = letter.del.keys().chain(letter.add.keys()).any(named)
            || letter.attrs.keys().any(|key| match key {
                ref_edit::AttrKey::EdgeLabel(e) | ref_edit::AttrKey::EdgePosition(e) => {
                    rho.1.contains_key(e) && !back.1.contains_key(e)
                }
                ref_edit::AttrKey::Value(n)
                | ref_edit::AttrKey::Field(n, _)
                | ref_edit::AttrKey::Meta(n)
                | ref_edit::AttrKey::Anchor(n) => rho.0.contains_key(n) && !back.0.contains_key(n),
            });
        (!clash).then(|| renamed_delta(letter, &back))
    }

    /// Translate one view letter through a clone of the bound lens and the
    /// reference, and, when the letter names whole-state ids through the
    /// renaming before the edit, through the whole-state lens.
    fn step_put(
        case: &Case<'_>,
        reference_lens: &super::RefGraphLens<'_>,
        (production, state, whole): (
            &Bound,
            &reference::SegmentedEditState,
            &ref_edit::GraphEditState,
        ),
        letter: &RefGraphDelta,
        counts: &mut PutCounts,
    ) {
        let view_schema = case.lens().identity(ComplementShape::Graph).target_schema;
        counts.letters += 1;
        let mut advanced = production.clone();
        let edit = prod_delta(letter);
        let produced = advanced.put_edit(&edit);
        let expected = reference::segmented_edit_put(
            case.restriction(),
            reference_lens,
            state,
            case.partition,
            letter,
        );
        match (produced, expected) {
            (Ok(step), Ok((delta, next))) => {
                counts.admitted += 1;
                assert_eq!(
                    ref_delta(&step.source_delta),
                    delta,
                    "source delta of {letter:?}"
                );
                assert_eq!(step.view_delta, edit, "the view delta is the input");
                assert_eq!(
                    production_state(&advanced, view_schema),
                    reference_state(&next, case.partition, view_schema),
                    "segments after {letter:?}"
                );
                let before = renaming(&whole.complement, &state.complements);
                let Some(whole_edit) = whole_letter(letter, &before) else {
                    return;
                };
                let get = |graph: &RefGraph| reference_lens.get(graph);
                let put = |view: &RefGraph, complement: &RefComplement| {
                    reference_lens.put(view, complement)
                };
                if let Ok((whole_delta, whole_next)) =
                    ref_edit::graph_edit_put(&get, &put, whole, &whole_edit)
                {
                    counts.whole += 1;
                    assert_eq!(whole_delta, delta, "whole-state source delta of {letter:?}");
                    let after = renaming(&whole_next.complement, &next.complements);
                    assert_eq!(
                        renamed_graph(&whole_next.view, &after),
                        reference::assemble_graph(&next.views, case.partition).expect("views glue"),
                        "view after {letter:?}"
                    );
                }
            }
            (Err(error), Err(refusal)) => {
                counts.refused += 1;
                assert_eq!(
                    production_put_refusal(&error),
                    reference_refusal(&refusal),
                    "{letter:?}"
                );
                assert_eq!(
                    production_state(&advanced, view_schema),
                    production_state(production, view_schema),
                    "a refused put changed a segment"
                );
                assert_eq!(
                    advanced.segmented_state().get_run(),
                    production.segmented_state().get_run()
                );
                assert_eq!(advanced.sequence(), production.sequence());
            }
            (produced, expected) => panic!(
                "segmented put disagrees on {letter:?}: {:?} / {expected:?}",
                produced.map(|step| step.source_delta)
            ),
        }
    }

    /// Bind one case and put back every view letter of its glued view.
    pub fn check_puts(case: &Case<'_>, graph: &RefGraph, counts: &mut PutCounts) {
        let Some((production, state, _)) = bind(case, graph) else {
            return;
        };
        counts.bound += 1;
        let plan = panproto_reference::restrict::compile(case.schema, case.schema, case.migration)
            .expect("the reference plan compiles");
        let reference_lens = super::RefGraphLens::new(
            case.schema,
            case.schema,
            case.migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .expect("the reference lens");
        let (view, complement) = reference_lens.get(graph).expect("whole-state get");
        let whole = ref_edit::GraphEditState {
            source: graph.clone(),
            view,
            complement,
        };
        let glued = reference::assemble_graph(&state.views, case.partition).expect("views glue");
        for letter in
            graph_edit_lens_letters(case.schema, &glued, BOUNDED_LENS_FIELD, &BOUNDED_LENS_VALUE)
        {
            step_put(
                case,
                &reference_lens,
                (&production, &state, &whole),
                &letter,
                counts,
            );
        }
    }

    #[test]
    fn segmented_graph_edit_lens_puts_view_edits_as_the_reference_on_every_97th_case() {
        let mut counts = PutCounts::default();
        for case in segmented_restrict_cases().step_by(97) {
            let partition = with_spare(&case.partition, 3);
            let edit_case = Case {
                schema: &case.schema,
                migration: &case.migration,
                partition: &partition,
            };
            check_puts(&edit_case, &case.graph, &mut counts);
        }
        assert_eq!(
            counts,
            PutCounts {
                bound: 1_163,
                letters: 17_565,
                admitted: 9_273,
                refused: 8_292,
                whole: 9_273,
            }
        );
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

        #[test]
        fn segmented_graph_edit_lens_puts_view_edits_as_the_reference_on_generated_cases(
            anchors in proptest::collection::vec(0_usize..3, 2..=8),
            edges in proptest::collection::vec((0_usize..8, 0_usize..8), 0..=14),
            owners in proptest::collection::vec(0_u32..4, 8),
            segments in 2_u32..=4,
            surviving in 0_u8..8,
        ) {
            let (schema, migration, graph, partition) =
                super::segmented_graph_get::generated(&anchors, &edges, &owners, segments, surviving);
            let from = u32::try_from(anchors.len()).expect("a small count");
            let partition = with_spare(&partition, from);
            let case = Case { schema: &schema, migration: &migration, partition: &partition };
            let mut counts = PutCounts::default();
            check_puts(&case, &graph, &mut counts);
            proptest::prop_assert_eq!(counts.letters, counts.admitted + counts.refused);
        }
    }

    /// An edit put back whose commit fails at each task of its four runs
    /// leaves the lens and every segment as they were; putting the same
    /// edit again resumes the runs and ends where the uninterrupted put
    /// ends.
    #[test]
    fn a_put_stopped_by_a_failed_commit_at_each_task_resumes_to_the_uninterrupted_put() {
        let (schema, migration, graph, partition) = super::segmented_graph_get::generated(
            &[0, 2, 2, 2, 1],
            &[(0, 1), (1, 2), (2, 3), (3, 1), (3, 4)],
            &[0, 1, 2, 0, 1],
            3,
            0b011,
        );
        let partition = with_spare(&partition, 5);
        let case = Case {
            schema: &schema,
            migration: &migration,
            partition: &partition,
        };
        let Input {
            manifest, bytes, ..
        } = input(&graph, &partition);
        let store = FailingStore::default();
        let (commits, fail_at) = (store.commits.clone(), store.fail_at.clone());
        let bound = SegmentedGraphEditLens::segmented(
            case.lens(),
            Budget::unlimited(),
            SegmentedGraph {
                input: manifest.digest(),
                manifest,
                segments: bytes,
                store,
            },
        )
        .expect("the case binds");
        let view_schema = case.lens().identity(ComplementShape::Graph).target_schema;
        let state = bound.segmented_state();
        let view_partition = state.view_manifest().partition().expect("a partition");
        let mut view = panproto_inst::GInstance::new();
        let mut held_edges = BTreeMap::new();
        for spec in view_partition.specs() {
            let emitted = read_emitted(&*state.store(), state.get_run(), &view_partition, spec.id)
                .expect("an emitted segment");
            for node in emitted.view.nodes.into_values() {
                view = view.with_node(node);
            }
            held_edges.extend(emitted.view.edges);
        }
        for (id, edge) in held_edges {
            view.insert_edge(id, edge).expect("a view edge");
        }
        let edit = graph_edit_lens_letters(
            &schema,
            &super::ref_graph(&view),
            BOUNDED_LENS_FIELD,
            &BOUNDED_LENS_VALUE,
        )
        .iter()
        .map(prod_delta)
        .find(|edit| !edit.del_edges().is_empty())
        .expect("a view edge deletion");
        let mut baseline = bound.clone();
        let before = commits.get();
        let expected = baseline
            .put_edit(&edit)
            .expect("the view edge deletion is put back");
        let total = commits.get() - before;
        assert_eq!(
            total, 52,
            "the four runs of the put commit each of their tasks once"
        );
        for failing in 1..=total {
            let mut lens = bound.clone();
            fail_at.set(Some(commits.get() + failing));
            let stopped = lens.put_edit(&edit);
            assert!(
                matches!(stopped, Err(ShapeEditError::Incomplete(_))),
                "{:?}",
                stopped.map(|step| step.source_delta)
            );
            assert_eq!(
                production_state(&lens, view_schema),
                production_state(&bound, view_schema)
            );
            assert_eq!(lens.sequence(), 0);
            fail_at.set(None);
            let resumed = lens.put_edit(&edit).expect("the retried put is admitted");
            assert_eq!(resumed.source_delta, expected.source_delta);
            assert_eq!(
                production_state(&lens, view_schema),
                production_state(&baseline, view_schema)
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Elementary protolens kinds: instantiation, get, put and their refusals
// against the reference's elementary kinds
// ---------------------------------------------------------------------------

use panproto_reference::enumerate::lenses::{
    ELEMENTARY_SOURCE_BOUND, ELEMENTARY_VIEW_BOUND, elementary_protolens_alphabet,
    elementary_protolens_count_certificate, elementary_protolens_schemas, elementary_view_schema,
    graph_state_count_up_to, graph_states_up_to,
};
use panproto_reference::lens::{ElementaryProtolens, ElementaryRefusal};

/// The production protolens of each entry of
/// [`elementary_protolens_alphabet`], built with the constructors the lens
/// DSL's steps compile to.
fn production_elementary_alphabet() -> Vec<panproto_lens::Protolens> {
    use std::sync::Arc;
    let swap = |left: &str, right: &str| {
        HashMap::from([
            (Arc::<str>::from(left), Arc::<str>::from(right)),
            (Arc::<str>::from(right), Arc::<str>::from(left)),
        ])
    };
    let equation = panproto_gat::Equation {
        name: Arc::from("eq"),
        lhs: panproto_gat::Term::Var(Arc::from("v")),
        rhs: panproto_gat::Term::Var(Arc::from("v")),
    };
    vec![
        elementary::rename_sort("s", "u"),
        elementary::rename_sort("s", "t"),
        elementary::rename_op("p", "r"),
        elementary::rename_op("p", "q"),
        elementary::rename_edge_name("a", "b", "x", "z"),
        elementary::rename_edge_name("a", "b", "x", "y"),
        elementary::pullback(panproto_gat::TheoryMorphism::new(
            "swap",
            "domain",
            "codomain",
            swap("s", "t"),
            swap("p", "q"),
        )),
        elementary::add_equation(equation),
        elementary::drop_equation("eq"),
        elementary::drop_sort("t"),
        elementary::drop_sort("a"),
        elementary::drop_op("p"),
        elementary::drop_edge("a", "b", Some("x".into())),
        elementary::add_sort("c", "t", Value::Null),
        elementary::add_edge("a", "b", "w", "p"),
        elementary::add_op("o", "a", "b", "o"),
    ]
}

/// The observable result of applying an elementary protolens to a source:
/// the typed category of a refusal before instantiation, or the target
/// schema with the view and complement, or a refusal of get.
#[derive(Debug, PartialEq, Eq)]
enum ElementaryApplication {
    Inapplicable,
    IdentifiesEdges,
    Applied(Box<(RefSchema, RefGraph, RefComplement)>),
    Refused(String),
}

const IDENTIFIES_EDGES: &str = "renaming operations sends the edges";

fn production_elementary_application(
    protolens: &panproto_lens::Protolens,
    schema: &RefSchema,
    source: &RefGraph,
) -> ElementaryApplication {
    match panproto_lens::graph_protolens::apply(
        protolens,
        &prod_schema(schema),
        &Protocol::default(),
        &prod_graph(source),
    ) {
        Ok(applied) => ElementaryApplication::Applied(Box::new((
            ref_schema(&applied.lens.tgt_schema),
            ref_graph(&applied.view),
            ref_complement(&applied.complement),
        ))),
        Err(LensError::ProtolensInapplicable { .. }) => ElementaryApplication::Inapplicable,
        Err(LensError::ProtolensError(message)) if message.starts_with(IDENTIFIES_EDGES) => {
            ElementaryApplication::IdentifiesEdges
        }
        Err(error) => ElementaryApplication::Refused(format!("{:?}", lens_reason(&error))),
    }
}

fn reference_elementary_application(
    kind: &ElementaryProtolens,
    schema: &RefSchema,
    source: &RefGraph,
) -> ElementaryApplication {
    if !kind.precondition().holds(schema) {
        return ElementaryApplication::Inapplicable;
    }
    let protolens = match kind.at(schema) {
        Ok(protolens) => protolens,
        Err(ElementaryRefusal::IdentifiesEdges { .. }) => {
            return ElementaryApplication::IdentifiesEdges;
        }
    };
    match panproto_reference::lens::protolens_instance(&protolens, schema, source) {
        Ok(application) => ElementaryApplication::Applied(Box::new((
            application.lens.target().clone(),
            application.view,
            without_identity(&application.complement),
        ))),
        Err(RefProtolensError::FalsePrecondition { .. }) => ElementaryApplication::Inapplicable,
        Err(RefProtolensError::Restrict(error)) => ElementaryApplication::Refused(format!(
            "{:?}",
            Reason::Restrict(ref_restrict_reason(&error))
        )),
        Err(RefProtolensError::Plan(error)) => ElementaryApplication::Refused(format!(
            "{:?}",
            Reason::Restrict(ref_restrict_reason(&RefRestrictError::Plan(error)))
        )),
    }
}

/// One elementary case: a kind, a schema and a source, with a view to put
/// back or none for the application itself.
#[derive(Clone, Copy, Debug)]
struct ElementaryCase {
    kind: usize,
    schema: usize,
    source: usize,
    view: Option<usize>,
}

/// Per-thread state of the elementary certificate: the two alphabets, the
/// carrier, and for each instantiated `(kind, schema)` its lens pair and put
/// views.
struct ElementaryContext {
    production: Vec<panproto_lens::Protolens>,
    reference: Vec<ElementaryProtolens>,
    schemas: Vec<RefSchema>,
    sources: Vec<Vec<RefGraph>>,
    pairs: BTreeMap<(usize, usize), (Pair, Vec<RefGraph>)>,
}

impl ElementaryContext {
    fn new() -> Self {
        let production = production_elementary_alphabet();
        let reference = elementary_protolens_alphabet();
        let schemas = elementary_protolens_schemas();
        let (nodes, edges) = ELEMENTARY_SOURCE_BOUND;
        let sources = schemas
            .iter()
            .map(|schema| graph_states_up_to(schema, nodes, edges))
            .collect();
        let mut pairs = BTreeMap::new();
        for (kind, reference_kind) in reference.iter().enumerate() {
            for (index, schema) in schemas.iter().enumerate() {
                if !reference_kind.precondition().holds(schema) {
                    continue;
                }
                let Ok((target, migration)) = reference_kind.target_and_migration(schema) else {
                    continue;
                };
                let lens = production[kind]
                    .instantiate_checked(&prod_schema(schema), &Protocol::default())
                    .unwrap_or_else(|error| {
                        panic!("kind {kind} instantiates at schema {index} in production: {error}")
                    });
                let (view_nodes, view_edges) = ELEMENTARY_VIEW_BOUND;
                let views = graph_states_up_to(
                    &elementary_view_schema(schema, &target),
                    view_nodes,
                    view_edges,
                );
                let pair =
                    Pair::with_production(schema.clone(), target, migration, &NoValueLayer, lens);
                pairs.insert((kind, index), (pair, views));
            }
        }
        Self {
            production,
            reference,
            schemas,
            sources,
            pairs,
        }
    }

    fn expand(&self, unit: &(usize, usize, usize)) -> Vec<ElementaryCase> {
        let &(kind, schema, source) = unit;
        let mut cases = vec![ElementaryCase {
            kind,
            schema,
            source,
            view: None,
        }];
        if let Some((_, views)) = self.pairs.get(&(kind, schema)) {
            cases.extend((0..views.len()).map(|view| ElementaryCase {
                kind,
                schema,
                source,
                view: Some(view),
            }));
        }
        cases
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ElementaryObservation {
    Application(ElementaryApplication),
    Put(Box<Option<PutOutcome>>),
}

fn production_elementary_case(
    context: &ElementaryContext,
    case: &ElementaryCase,
) -> ElementaryObservation {
    let source = &context.sources[case.schema][case.source];
    case.view.map_or_else(
        || {
            ElementaryObservation::Application(production_elementary_application(
                &context.production[case.kind],
                &context.schemas[case.schema],
                source,
            ))
        },
        |view| {
            let (pair, views) = &context.pairs[&(case.kind, case.schema)];
            ElementaryObservation::Put(Box::new(production_put(pair, source, &views[view])))
        },
    )
}

fn reference_elementary_case(
    context: &ElementaryContext,
    case: &ElementaryCase,
) -> ElementaryObservation {
    let source = &context.sources[case.schema][case.source];
    case.view.map_or_else(
        || {
            ElementaryObservation::Application(reference_elementary_application(
                &context.reference[case.kind],
                &context.schemas[case.schema],
                source,
            ))
        },
        |view| {
            let (pair, views) = &context.pairs[&(case.kind, case.schema)];
            ElementaryObservation::Put(Box::new(reference_put(pair, source, &views[view])))
        },
    )
}

#[test]
fn elementary_protolens_kinds_agree_with_the_reference_exhaustively() {
    let certificate = elementary_protolens_count_certificate();
    assert_eq!(certificate.kinds, 16);
    assert_eq!(certificate.schemas, 14);
    assert_eq!(certificate.source_states, 678);
    assert_eq!(certificate.get_cases, 10_848);
    assert_eq!(certificate.inapplicable, 19);
    assert_eq!(certificate.identifying, 2);
    assert_eq!(certificate.instantiated, 203);
    assert_eq!(certificate.put_cases, 388_847);

    let context = ElementaryContext::new();
    assert_eq!(context.production.len(), certificate.kinds);
    assert_eq!(context.pairs.len(), certificate.instantiated);
    let (nodes, edges) = ELEMENTARY_SOURCE_BOUND;
    for (schema, sources) in context.schemas.iter().zip(&context.sources) {
        assert_eq!(sources.len(), graph_state_count_up_to(schema, nodes, edges));
    }
    let units: Vec<(usize, usize, usize)> = (0..certificate.kinds)
        .flat_map(|kind| {
            context
                .sources
                .iter()
                .enumerate()
                .flat_map(move |(schema, sources)| {
                    (0..sources.len()).map(move |source| (kind, schema, source))
                })
        })
        .collect();
    assert_eq!(units.len(), certificate.get_cases);
    let bound = format!(
        "L-lenses-21 elementary protolens kinds: kinds={}, schemas={}, source_states={}, \
         get_cases={}, inapplicable={}, identifying={}, instantiated={}, put_cases={}",
        certificate.kinds,
        certificate.schemas,
        certificate.source_states,
        certificate.get_cases,
        certificate.inapplicable,
        certificate.identifying,
        certificate.instantiated,
        certificate.put_cases
    );
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &units,
        ElementaryContext::new,
        ElementaryContext::expand,
        production_elementary_case,
        reference_elementary_case,
        |production, reference| production == reference,
    ));
    assert_eq!(
        compared,
        u64::try_from(certificate.get_cases + certificate.put_cases).unwrap()
    );
}

/// A random graph over `schema` with at most four nodes and four edges,
/// each edge a typed candidate between its nodes' anchors.
fn random_elementary_graph(schema: &RefSchema, seed: u64) -> RefGraph {
    let mut state = seed;
    let mut next = |bound: usize| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(state >> 33).unwrap() % bound.max(1)
    };
    let vertices: Vec<&String> = schema.vertices.keys().collect();
    let mut graph = RefGraph::default();
    for id in 0..u32::try_from(next(5)).unwrap() {
        let node = RefNode::new(id, vertices[next(vertices.len())]);
        let node = if next(2) == 1 {
            node.with_value(BOUNDED_LENS_VALUE)
        } else {
            node
        };
        graph.nodes.insert(id, node);
    }
    let mut candidates = Vec::new();
    for (src, src_node) in &graph.nodes {
        for (tgt, tgt_node) in &graph.nodes {
            for label in &schema.edges {
                if label.src == src_node.anchor && label.tgt == tgt_node.anchor {
                    candidates.push((*src, *tgt, label.clone()));
                }
            }
        }
    }
    if !candidates.is_empty() {
        for id in 0..u32::try_from(next(5)).unwrap() {
            let (src, tgt, label) = candidates[next(candidates.len())].clone();
            graph.edges.insert(
                id,
                RefGEdge {
                    src,
                    tgt,
                    label,
                    position: None,
                },
            );
        }
    }
    graph
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]
    #[test]
    fn randomized_elementary_protolens_differential(
        kind in 0..16_usize,
        schema in 0..14_usize,
        source_seed in any::<u64>(),
        view_seed in any::<u64>(),
    ) {
        let production = production_elementary_alphabet();
        let reference = elementary_protolens_alphabet();
        let schemas = elementary_protolens_schemas();
        let schema_ref = &schemas[schema];
        let source = random_elementary_graph(schema_ref, source_seed);
        let production_application =
            production_elementary_application(&production[kind], schema_ref, &source);
        let reference_application =
            reference_elementary_application(&reference[kind], schema_ref, &source);
        prop_assert_eq!(&production_application, &reference_application);
        if let ElementaryApplication::Applied(applied) = reference_application {
            let (target, view, _) = *applied;
            let (_, migration) = reference[kind]
                .target_and_migration(schema_ref)
                .expect("an applied kind has a target");
            let lens = production[kind]
                .instantiate_checked(&prod_schema(schema_ref), &Protocol::default())
                .expect("an applied kind instantiates");
            let edited =
                random_elementary_graph(&elementary_view_schema(schema_ref, &target), view_seed);
            let pair =
                Pair::with_production(schema_ref.clone(), target, migration, &NoValueLayer, lens);
            for put_view in [&view, &edited] {
                prop_assert_eq!(
                    production_put(&pair, &source, put_view),
                    reference_put(&pair, &source, put_view)
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Fused chains, scoped protolenses and sequential composites against the
// reference's protolens terms
// ---------------------------------------------------------------------------

use panproto_reference::enumerate::lenses::{
    COMPOSITE_SOURCE_BOUND, FUSED_SOURCE_BOUND, composite_protolens_count_certificate,
    composite_protolens_pairs, fused_protolens_alphabet, fused_protolens_count_certificate,
    fused_protolens_layer, fused_protolens_schemas, put_views_of,
};
use panproto_reference::lens::{
    CompositeInstantiationError, CompositeLens, CompositePutError, ProtolensTerm, TermRefusal,
};

/// The production chain of each entry of [`fused_protolens_alphabet`], built
/// with the combinators the lens DSL's steps compile to.
fn production_fused_alphabet() -> Vec<panproto_lens::ProtolensChain> {
    use panproto_lens::ProtolensChain;
    use panproto_lens::protolens::combinators;
    let hoist = combinators::hoist_field("p", "i", "c");
    let fused_hoist = hoist.fuse().expect("the hoist chain fuses");
    let scoped = |focus: &str, inner: panproto_lens::Protolens| {
        ProtolensChain::new(vec![combinators::map_items(focus, inner)])
    };
    vec![
        combinators::add_field("p", "f", "integer", Value::Int(0)),
        hoist,
        combinators::nest_field("p", "c", "m", "object", "prop", Some("c".into()), "m", "c"),
        scoped("i", elementary::drop_sort("c")),
        scoped("i", elementary::rename_sort("string", "text")),
        scoped("i", elementary::rename_edge_name("i", "c", "c", "d")),
        scoped("i", elementary::drop_edge("i", "c", Some("c".into()))),
        scoped("i", elementary::add_sort("z", "string", Value::Null)),
        scoped("p", fused_hoist),
        scoped("c", elementary::drop_sort("c")),
    ]
}

/// The production chain of each pair of [`composite_protolens_pairs`]: the
/// two elementary protolenses as one chain.
fn production_pair_chains() -> Vec<panproto_lens::ProtolensChain> {
    let alphabet = production_elementary_alphabet();
    composite_protolens_pairs()
        .into_iter()
        .map(|(first, second)| {
            panproto_lens::ProtolensChain::new(vec![
                alphabet[first].clone(),
                alphabet[second].clone(),
            ])
        })
        .collect()
}

/// The reference term of each pair of [`composite_protolens_pairs`], fused.
fn reference_pair_terms() -> Vec<ProtolensTerm> {
    let alphabet = elementary_protolens_alphabet();
    composite_protolens_pairs()
        .into_iter()
        .map(|(first, second)| {
            ProtolensTerm::Fused(vec![
                ProtolensTerm::Elementary(alphabet[first].clone()),
                ProtolensTerm::Elementary(alphabet[second].clone()),
            ])
        })
        .collect()
}

/// The category of a production refusal before instantiation: a
/// relabelling that identifies two edges, or any other failed precondition.
fn production_instantiation_refusal(reasons: &str) -> ElementaryApplication {
    if reasons.contains(IDENTIFIES_EDGES) {
        ElementaryApplication::IdentifiesEdges
    } else {
        ElementaryApplication::Inapplicable
    }
}

fn production_term_application(
    chain: &panproto_lens::ProtolensChain,
    schema: &RefSchema,
    source: &RefGraph,
) -> ElementaryApplication {
    match panproto_lens::graph_protolens::apply_chain(
        chain,
        &prod_schema(schema),
        &Protocol::default(),
        &prod_graph(source),
    ) {
        Ok(applied) => ElementaryApplication::Applied(Box::new((
            ref_schema(&applied.lens.tgt_schema),
            ref_graph(&applied.view),
            ref_complement(&applied.complement),
        ))),
        Err(LensError::ProtolensInapplicable { reasons }) => {
            production_instantiation_refusal(&reasons.join("; "))
        }
        Err(error) => ElementaryApplication::Refused(format!("{:?}", lens_reason(&error))),
    }
}

/// The reference's application of a term: its refusal category, or the
/// target, view and complement of get through the graph lens of its
/// protolens with the term's value layer.
fn reference_term_application(
    term: &ProtolensTerm,
    layer: &'static dyn ValueLayer,
    schema: &RefSchema,
    source: &RefGraph,
) -> ElementaryApplication {
    let protolens = match term.at(schema) {
        Ok(protolens) => protolens,
        Err(TermRefusal::Inapplicable) => return ElementaryApplication::Inapplicable,
        Err(TermRefusal::Elementary(_)) => return ElementaryApplication::IdentifiesEdges,
    };
    let plan = match panproto_reference::restrict::compile(
        schema,
        &protolens.target,
        &protolens.migration,
    ) {
        Ok(plan) => plan,
        Err(error) => {
            return ElementaryApplication::Refused(format!(
                "{:?}",
                Reason::Restrict(ref_restrict_reason(&RefRestrictError::Plan(error)))
            ));
        }
    };
    let lens = RefGraphLens::new(
        schema,
        &protolens.target,
        &protolens.migration,
        &plan,
        &BTreeMap::new(),
        layer,
    )
    .expect("a plan binds the schemas it was compiled for");
    match lens.get(source) {
        Ok((view, complement)) => ElementaryApplication::Applied(Box::new((
            protolens.target.clone(),
            view,
            without_identity(&complement),
        ))),
        Err(error) => ElementaryApplication::Refused(format!(
            "{:?}",
            Reason::Restrict(ref_restrict_reason(&error))
        )),
    }
}

/// One term case: a term, a schema and a source, with a view to put back or
/// none for the application itself.
#[derive(Clone, Debug)]
struct TermCase {
    kind: usize,
    schema: usize,
    source: usize,
    view: Option<RefGraph>,
}

/// Per-thread state of a term certificate: the two alphabets, the carrier,
/// and for each instantiated `(kind, schema)` its lens pair.
struct TermContext {
    production: Vec<panproto_lens::ProtolensChain>,
    reference: Vec<ProtolensTerm>,
    schemas: Vec<RefSchema>,
    sources: Vec<Vec<RefGraph>>,
    pairs: BTreeMap<(usize, usize), Pair>,
}

impl TermContext {
    fn new(
        production: Vec<panproto_lens::ProtolensChain>,
        reference: Vec<ProtolensTerm>,
        schemas: Vec<RefSchema>,
        (nodes, edges): (usize, usize),
    ) -> Self {
        let sources = schemas
            .iter()
            .map(|schema| graph_states_up_to(schema, nodes, edges))
            .collect();
        let mut pairs = BTreeMap::new();
        for (kind, term) in reference.iter().enumerate() {
            for (index, schema) in schemas.iter().enumerate() {
                let Ok(protolens) = term.at(schema) else {
                    continue;
                };
                let Ok(lens) =
                    production[kind].instantiate(&prod_schema(schema), &Protocol::default())
                else {
                    continue;
                };
                let pair = Pair::with_production(
                    schema.clone(),
                    protolens.target,
                    protolens.migration,
                    fused_protolens_layer(term),
                    lens,
                );
                pairs.insert((kind, index), pair);
            }
        }
        Self {
            production,
            reference,
            schemas,
            sources,
            pairs,
        }
    }

    fn fused() -> Self {
        Self::new(
            production_fused_alphabet(),
            fused_protolens_alphabet(),
            fused_protolens_schemas(),
            FUSED_SOURCE_BOUND,
        )
    }

    fn fused_pairs() -> Self {
        Self::new(
            production_pair_chains(),
            reference_pair_terms(),
            elementary_protolens_schemas(),
            COMPOSITE_SOURCE_BOUND,
        )
    }

    /// The units of the carrier: every `(kind, schema, source)`.
    fn units(&self) -> Vec<(usize, usize, usize)> {
        (0..self.reference.len())
            .flat_map(|kind| {
                self.sources
                    .iter()
                    .enumerate()
                    .flat_map(move |(schema, sources)| {
                        (0..sources.len()).map(move |source| (kind, schema, source))
                    })
            })
            .collect()
    }

    /// The application case, then a put case for the reference view and
    /// each of its one-step edits.
    fn expand(&self, unit: &(usize, usize, usize)) -> Vec<TermCase> {
        let &(kind, schema, source) = unit;
        let mut cases = vec![TermCase {
            kind,
            schema,
            source,
            view: None,
        }];
        let term = &self.reference[kind];
        if let ElementaryApplication::Applied(applied) = reference_term_application(
            term,
            fused_protolens_layer(term),
            &self.schemas[schema],
            &self.sources[schema][source],
        ) {
            let (target, view, complement) = *applied;
            cases.extend(
                put_views_of(&view, &target, &complement)
                    .into_iter()
                    .map(|view| TermCase {
                        kind,
                        schema,
                        source,
                        view: Some(view),
                    }),
            );
        }
        cases
    }
}

fn production_term_case(context: &TermContext, case: &TermCase) -> ElementaryObservation {
    let source = &context.sources[case.schema][case.source];
    case.view.as_ref().map_or_else(
        || {
            ElementaryObservation::Application(production_term_application(
                &context.production[case.kind],
                &context.schemas[case.schema],
                source,
            ))
        },
        |view| {
            ElementaryObservation::Put(Box::new(
                context
                    .pairs
                    .get(&(case.kind, case.schema))
                    .and_then(|pair| production_put(pair, source, view)),
            ))
        },
    )
}

fn reference_term_case(context: &TermContext, case: &TermCase) -> ElementaryObservation {
    let source = &context.sources[case.schema][case.source];
    case.view.as_ref().map_or_else(
        || {
            let term = &context.reference[case.kind];
            ElementaryObservation::Application(reference_term_application(
                term,
                fused_protolens_layer(term),
                &context.schemas[case.schema],
                source,
            ))
        },
        |view| {
            ElementaryObservation::Put(Box::new(
                context
                    .pairs
                    .get(&(case.kind, case.schema))
                    .and_then(|pair| reference_put(pair, source, view)),
            ))
        },
    )
}

#[test]
fn fused_and_scoped_protolenses_agree_with_the_reference_exhaustively() {
    let certificate = fused_protolens_count_certificate();
    assert_eq!(certificate.kinds, 10);
    assert_eq!(certificate.schemas, 8);
    assert_eq!(certificate.source_states, 7448);
    assert_eq!(certificate.get_cases, 74_480);
    assert_eq!(certificate.inapplicable, 0);
    assert_eq!(certificate.identifying, 0);
    assert_eq!(certificate.instantiated, 80);
    assert_eq!(certificate.get_refusals, 192);
    assert_eq!(certificate.put_cases, 1_534_344);

    let context = TermContext::fused();
    assert_eq!(context.production.len(), certificate.kinds);
    let (nodes, edges) = FUSED_SOURCE_BOUND;
    for (schema, sources) in context.schemas.iter().zip(&context.sources) {
        assert_eq!(sources.len(), graph_state_count_up_to(schema, nodes, edges));
    }
    let units = context.units();
    assert_eq!(units.len(), certificate.get_cases);
    let bound =
        format!("L-lenses-22 and L-lenses-23 fused and scoped protolenses: {certificate:?}");
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &units,
        TermContext::fused,
        TermContext::expand,
        production_term_case,
        reference_term_case,
        |production, reference| production == reference,
    ));
    assert_eq!(
        compared,
        u64::try_from(certificate.get_cases + certificate.put_cases).unwrap()
    );
}

/// The observable result of a sequential composite: the category of a
/// refusal before instantiation, or the target, the view and, when the
/// production path keeps one complement per part, the complements, or a
/// refusal of get.
#[derive(Debug, PartialEq, Eq)]
enum CompositeApplication {
    Inapplicable,
    IdentifiesEdges,
    Applied(Box<(RefSchema, RefGraph, Option<Vec<RefComplement>>)>),
    Refused(String),
}

/// The production composite of elementary protolenses at a schema, as the
/// lens DSL instantiates compiled stages: each chain at the running schema,
/// the lenses joined into a path.
fn production_composite(
    parts: &[&panproto_lens::Protolens],
    schema: &RefSchema,
) -> Result<panproto_lens::LensPath, CompositeApplication> {
    let protocol = Protocol::default();
    let refuse = |error: LensError| match error {
        LensError::ProtolensError(message) => production_composite_refusal(&message),
        other => CompositeApplication::Refused(format!("{:?}", lens_reason(&other))),
    };
    let mut path: Option<panproto_lens::LensPath> = None;
    let source = prod_schema(schema);
    for part in parts {
        let running = path
            .as_ref()
            .map_or(&source, panproto_lens::LensPath::tgt_schema);
        let lens = panproto_lens::ProtolensChain::new(vec![(*part).clone()])
            .instantiate(running, &protocol)
            .map_err(refuse)?;
        let stage = panproto_lens::LensPath::single(lens);
        path = Some(match path {
            Some(path) => path.then(&stage).map_err(refuse)?,
            None => stage,
        });
    }
    Ok(path.unwrap_or_else(|| panproto_lens::LensPath::identity(&source)))
}

fn production_composite_refusal(message: &str) -> CompositeApplication {
    match production_instantiation_refusal(message) {
        ElementaryApplication::IdentifiesEdges => CompositeApplication::IdentifiesEdges,
        _ => CompositeApplication::Inapplicable,
    }
}

/// The inner refusal of a located path error.
fn path_reason(error: &LensError) -> Reason {
    match error {
        LensError::AtStage { error, .. } => lens_reason(error),
        other => lens_reason(other),
    }
}

fn production_composite_application(
    path: &Result<panproto_lens::LensPath, CompositeApplication>,
    source: &RefGraph,
) -> CompositeApplication {
    let path = match path {
        Ok(path) => path,
        Err(CompositeApplication::Inapplicable) => return CompositeApplication::Inapplicable,
        Err(CompositeApplication::IdentifiesEdges) => {
            return CompositeApplication::IdentifiesEdges;
        }
        Err(other) => return CompositeApplication::Refused(format!("{other:?}")),
    };
    match path.get_graph(&prod_graph(source)) {
        Ok((view, complement)) => {
            let complements: Vec<RefComplement> = complement
                .entries()
                .iter()
                .flat_map(|entry| entry.complement.complements())
                .map(ref_complement)
                .collect();
            CompositeApplication::Applied(Box::new((
                ref_schema(path.tgt_schema()),
                ref_graph(&view),
                (complements.len() == path.stages().len()).then_some(complements),
            )))
        }
        Err(error) => CompositeApplication::Refused(format!("{:?}", path_reason(&error))),
    }
}

fn reference_composite_application(
    composite: &Result<CompositeLens, CompositeApplication>,
    source: &RefGraph,
) -> CompositeApplication {
    let composite = match composite {
        Ok(composite) => composite,
        Err(CompositeApplication::Inapplicable) => return CompositeApplication::Inapplicable,
        Err(CompositeApplication::IdentifiesEdges) => {
            return CompositeApplication::IdentifiesEdges;
        }
        Err(other) => return CompositeApplication::Refused(format!("{other:?}")),
    };
    match composite.get(source) {
        Ok((view, complements)) => CompositeApplication::Applied(Box::new((
            composite.target().cloned().unwrap_or_default(),
            view,
            Some(complements.iter().map(without_identity).collect()),
        ))),
        Err(refusal) => CompositeApplication::Refused(format!(
            "{:?}",
            Reason::Restrict(ref_restrict_reason(&refusal.error))
        )),
    }
}

fn reference_composite(
    parts: &[&ElementaryProtolens],
    schema: &RefSchema,
) -> Result<CompositeLens, CompositeApplication> {
    let parts: Vec<ProtolensTerm> = parts
        .iter()
        .map(|kind| ProtolensTerm::Elementary((*kind).clone()))
        .collect();
    CompositeLens::instantiate(&parts, schema).map_err(|refusal| match refusal.error {
        CompositeInstantiationError::Term(TermRefusal::Inapplicable) => {
            CompositeApplication::Inapplicable
        }
        CompositeInstantiationError::Term(TermRefusal::Elementary(_)) => {
            CompositeApplication::IdentifiesEdges
        }
        CompositeInstantiationError::Plan(error) => CompositeApplication::Refused(format!(
            "{:?}",
            Reason::Restrict(ref_restrict_reason(&RefRestrictError::Plan(error)))
        )),
    })
}

fn production_composite_put(
    path: &panproto_lens::LensPath,
    source: &RefGraph,
    edited: &RefGraph,
) -> Option<PutOutcome> {
    let (_, complement) = path.get_graph(&prod_graph(source)).ok()?;
    let view = prod_graph(edited);
    Some(PutOutcome {
        put: path
            .put_graph(&view, &complement)
            .map(|restored| ref_graph(&restored))
            .map_err(|error| path_reason(&error)),
        domain: path
            .check_graph_put_domain(&view, &complement)
            .map_err(|(_, errors)| errors.iter().map(put_domain_reason).collect()),
    })
}

fn reference_composite_put(
    composite: &CompositeLens,
    source: &RefGraph,
    edited: &RefGraph,
) -> Option<PutOutcome> {
    let (_, complements) = composite.get(source).ok()?;
    Some(PutOutcome {
        put: composite
            .put(edited, &complements)
            .map_err(|refusal| match refusal.error {
                CompositePutError::Lens(error) => ref_lens_reason(&error),
                other => Reason::Identity(format!("{other:?}")),
            }),
        domain: composite
            .check_put_domain(edited, &complements)
            .map_err(|refusal| refusal.error.iter().map(ref_put_domain_reason).collect()),
    })
}

/// Per-thread state of the sequential composite certificate: each pair at
/// each schema instantiated on both sides.
struct CompositeContext {
    schemas: Vec<RefSchema>,
    sources: Vec<Vec<RefGraph>>,
    production: Vec<Vec<Result<panproto_lens::LensPath, CompositeApplication>>>,
    reference: Vec<Vec<Result<CompositeLens, CompositeApplication>>>,
}

impl CompositeContext {
    fn new() -> Self {
        let production_alphabet = production_elementary_alphabet();
        let reference_alphabet = elementary_protolens_alphabet();
        let schemas = elementary_protolens_schemas();
        let (nodes, edges) = COMPOSITE_SOURCE_BOUND;
        let sources = schemas
            .iter()
            .map(|schema| graph_states_up_to(schema, nodes, edges))
            .collect();
        let pairs = composite_protolens_pairs();
        let production = pairs
            .iter()
            .map(|&(first, second)| {
                schemas
                    .iter()
                    .map(|schema| {
                        production_composite(
                            &[&production_alphabet[first], &production_alphabet[second]],
                            schema,
                        )
                    })
                    .collect()
            })
            .collect();
        let reference = pairs
            .iter()
            .map(|&(first, second)| {
                schemas
                    .iter()
                    .map(|schema| {
                        reference_composite(
                            &[&reference_alphabet[first], &reference_alphabet[second]],
                            schema,
                        )
                    })
                    .collect()
            })
            .collect();
        Self {
            schemas,
            sources,
            production,
            reference,
        }
    }

    fn units(&self) -> Vec<(usize, usize, usize)> {
        (0..self.reference.len())
            .flat_map(|pair| {
                self.sources
                    .iter()
                    .enumerate()
                    .flat_map(move |(schema, sources)| {
                        (0..sources.len()).map(move |source| (pair, schema, source))
                    })
            })
            .collect()
    }

    /// The application case, then a put case for the reference view and
    /// each of its one-step edits over the second part's target against
    /// its complement.
    fn expand(&self, unit: &(usize, usize, usize)) -> Vec<TermCase> {
        let &(kind, schema, source) = unit;
        let mut cases = vec![TermCase {
            kind,
            schema,
            source,
            view: None,
        }];
        if let Ok(composite) = &self.reference[kind][schema]
            && let Ok((view, complements)) = composite.get(&self.sources[schema][source])
            && let Some(target) = composite.target()
        {
            cases.extend(
                put_views_of(&view, target, &complements[complements.len() - 1])
                    .into_iter()
                    .map(|view| TermCase {
                        kind,
                        schema,
                        source,
                        view: Some(view),
                    }),
            );
        }
        cases
    }
}

#[derive(Debug, PartialEq, Eq)]
enum CompositeObservation {
    Application(CompositeApplication),
    Put(Box<Option<PutOutcome>>),
}

fn production_composite_case(context: &CompositeContext, case: &TermCase) -> CompositeObservation {
    let source = &context.sources[case.schema][case.source];
    let path = &context.production[case.kind][case.schema];
    case.view.as_ref().map_or_else(
        || CompositeObservation::Application(production_composite_application(path, source)),
        |view| {
            CompositeObservation::Put(Box::new(
                path.as_ref()
                    .ok()
                    .and_then(|path| production_composite_put(path, source, view)),
            ))
        },
    )
}

fn reference_composite_case(context: &CompositeContext, case: &TermCase) -> CompositeObservation {
    let source = &context.sources[case.schema][case.source];
    let composite = &context.reference[case.kind][case.schema];
    case.view.as_ref().map_or_else(
        || CompositeObservation::Application(reference_composite_application(composite, source)),
        |view| {
            CompositeObservation::Put(Box::new(
                composite
                    .as_ref()
                    .ok()
                    .and_then(|composite| reference_composite_put(composite, source, view)),
            ))
        },
    )
}

/// Production and reference composite observations agree: equal, except
/// that a production path that fused the two parts into one group keeps one
/// complement for both, which is then compared through put alone.
fn composite_observations_agree(
    production: &CompositeObservation,
    reference: &CompositeObservation,
) -> bool {
    match (production, reference) {
        (
            CompositeObservation::Application(CompositeApplication::Applied(production)),
            CompositeObservation::Application(CompositeApplication::Applied(reference)),
        ) => {
            let (production_target, production_view, production_complements) = &**production;
            let (reference_target, reference_view, reference_complements) = &**reference;
            production_target == reference_target
                && production_view == reference_view
                && production_complements
                    .as_ref()
                    .is_none_or(|complements| Some(complements) == reference_complements.as_ref())
        }
        _ => production == reference,
    }
}

#[test]
fn sequential_composites_of_elementary_protolenses_agree_with_the_reference_exhaustively() {
    let certificate = composite_protolens_count_certificate().sequential;
    assert_eq!(certificate.kinds, 256);
    assert_eq!(certificate.schemas, 14);
    assert_eq!(certificate.source_states, 438);
    assert_eq!(certificate.get_cases, 112_128);
    assert_eq!(certificate.inapplicable, 777);
    assert_eq!(certificate.identifying, 54);
    assert_eq!(certificate.instantiated, 2753);
    assert_eq!(certificate.get_refusals, 0);
    assert_eq!(certificate.put_cases, 953_227);

    let context = CompositeContext::new();
    let (nodes, edges) = COMPOSITE_SOURCE_BOUND;
    for (schema, sources) in context.schemas.iter().zip(&context.sources) {
        assert_eq!(sources.len(), graph_state_count_up_to(schema, nodes, edges));
    }
    let units = context.units();
    assert_eq!(units.len(), certificate.get_cases);
    let bound = format!("L-lenses-24 sequential composites: {certificate:?}");
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &units,
        CompositeContext::new,
        CompositeContext::expand,
        production_composite_case,
        reference_composite_case,
        composite_observations_agree,
    ));
    assert_eq!(
        compared,
        u64::try_from(certificate.get_cases + certificate.put_cases).unwrap()
    );
}

#[test]
fn fused_two_step_chains_of_elementary_protolenses_agree_with_the_reference_exhaustively() {
    let certificate = composite_protolens_count_certificate().fused;
    assert_eq!(certificate.kinds, 256);
    assert_eq!(certificate.schemas, 14);
    assert_eq!(certificate.source_states, 438);
    assert_eq!(certificate.get_cases, 112_128);
    assert_eq!(certificate.inapplicable, 777);
    assert_eq!(certificate.identifying, 54);
    assert_eq!(certificate.instantiated, 2753);
    assert_eq!(certificate.get_refusals, 0);
    assert_eq!(certificate.put_cases, 961_969);

    let context = TermContext::fused_pairs();
    let units = context.units();
    assert_eq!(units.len(), certificate.get_cases);
    let bound = format!("L-lenses-22 fused two-step chains: {certificate:?}");
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &units,
        TermContext::fused_pairs,
        TermContext::expand,
        production_term_case,
        reference_term_case,
        |production, reference| production == reference,
    ));
    assert_eq!(
        compared,
        u64::try_from(certificate.get_cases + certificate.put_cases).unwrap()
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]
    #[test]
    fn randomized_fused_and_scoped_protolens_differential(
        kind in 0..10_usize,
        schema in 0..8_usize,
        source_seed in any::<u64>(),
        view_seed in any::<u64>(),
    ) {
        let production = production_fused_alphabet();
        let reference = fused_protolens_alphabet();
        let schemas = fused_protolens_schemas();
        let schema_ref = &schemas[schema];
        let term = &reference[kind];
        let layer = fused_protolens_layer(term);
        let source = random_elementary_graph(schema_ref, source_seed);
        let production_application =
            production_term_application(&production[kind], schema_ref, &source);
        let reference_application =
            reference_term_application(term, layer, schema_ref, &source);
        prop_assert_eq!(&production_application, &reference_application);
        if let ElementaryApplication::Applied(applied) = reference_application {
            let (target, view, _) = *applied;
            let protolens = term.at(schema_ref).expect("an applied term has a protolens");
            let lens = production[kind]
                .instantiate(&prod_schema(schema_ref), &Protocol::default())
                .expect("an applied chain instantiates");
            let edited =
                random_elementary_graph(&elementary_view_schema(schema_ref, &target), view_seed);
            let pair = Pair::with_production(
                schema_ref.clone(),
                protolens.target,
                protolens.migration,
                layer,
                lens,
            );
            for put_view in [&view, &edited] {
                prop_assert_eq!(
                    production_put(&pair, &source, put_view),
                    reference_put(&pair, &source, put_view)
                );
            }
        }
    }

    #[test]
    fn randomized_sequential_composite_differential(
        steps in proptest::collection::vec(0..16_usize, 1..=5),
        schema in 0..14_usize,
        source_seed in any::<u64>(),
        view_seed in any::<u64>(),
    ) {
        let production_alphabet = production_elementary_alphabet();
        let reference_alphabet = elementary_protolens_alphabet();
        let schemas = elementary_protolens_schemas();
        let schema_ref = &schemas[schema];
        let production_parts: Vec<&panproto_lens::Protolens> =
            steps.iter().map(|step| &production_alphabet[*step]).collect();
        let reference_parts: Vec<&ElementaryProtolens> =
            steps.iter().map(|step| &reference_alphabet[*step]).collect();
        let path = production_composite(&production_parts, schema_ref);
        let composite = reference_composite(&reference_parts, schema_ref);
        let source = random_elementary_graph(schema_ref, source_seed);
        let production_application = production_composite_application(&path, &source);
        let reference_application = reference_composite_application(&composite, &source);
        prop_assert!(
            composite_observations_agree(
                &CompositeObservation::Application(production_application),
                &CompositeObservation::Application(reference_application),
            ),
            "the composite applications differ"
        );
        if let (Ok(path), Ok(composite)) = (&path, &composite)
            && let Ok((view, _)) = composite.get(&source)
        {
            let target = composite.target().expect("a composite has a target");
            let edited =
                random_elementary_graph(&elementary_view_schema(schema_ref, target), view_seed);
            for put_view in [&view, &edited] {
                prop_assert_eq!(
                    production_composite_put(path, &source, put_view),
                    reference_composite_put(composite, &source, put_view)
                );
            }
        }
    }
}

/// Segmented table get against its reference definition and whole-state
/// table get.
mod segmented_table_get {
    use std::collections::{BTreeMap, BTreeSet};

    use panproto_inst::InstanceShapeKind;
    use panproto_inst::identity::{ComplementShape, Digest};
    use panproto_inst::segment::{
        AnySegment, Budget, EdgeIdRun, IdRange, ManifestHeader, ManifestRole, MemoryStore,
        Partition, RecordSource, Residual, RunIncompleteReason, RunStatus, SegmentId,
        SegmentManifest, SegmentRecord, SegmentSpec, SegmentedRestrictError, SegmentedRun,
        TableComplementSegment, assemble_table, partition_table, read_emitted_segment, seal_full,
    };
    use panproto_lens::table_lens;
    use panproto_reference::enumerate::segments::{
        segmented_table_restrict_partitions, segmented_table_restrict_table_count,
        segmented_table_restrict_tables,
    };
    use panproto_reference::segment as reference;
    use panproto_schema::ScalarKind;
    use proptest::prelude::*;

    use super::{
        Reason, RefMigration, RefSEdge, RefSchema, RefTable, TablePair, lens_reason, prod_table,
        ref_restrict_reason, ref_table, ref_table_complement, restrict_reason,
    };

    /// The schema digest of a sealed source family.
    pub const SOURCE_SCHEMA: [u8; 32] = [11; 32];

    /// The lenses of the carrier, by name.
    pub const LENSES: [&str; 6] = [
        "identity",
        "drop b, contract to aa",
        "drop b, no contraction label",
        "drop ba",
        "drop x",
        "drop a, contract to bb",
    ];

    fn relation(src: &str, tgt: &str, name: &str) -> RefSEdge {
        RefSEdge::new(src, tgt, "relation", Some(name))
    }

    /// The schema with tables `a` (column `x`) and `b` (column `y`) as
    /// `tables` lists, and the relations `relations` lists.
    fn schema(tables: &[&str], relations: &[(&str, &str, &str)]) -> RefSchema {
        let mut schema = RefSchema::default();
        for table in tables {
            let column = if *table == "a" { "x" } else { "y" };
            let vertex = format!("{table}.{column}");
            schema = schema.vertex(table, "record").vertex(&vertex, "int").edge(
                table,
                &vertex,
                "prop",
                Some(column),
            );
        }
        for (src, tgt, name) in relations {
            schema.edges.insert(relation(src, tgt, name));
        }
        schema
    }

    /// The lens of `LENSES[index]`: from tables `a` and `b` with the
    /// relations `ab` and `ba` to the target it names, keeping everything
    /// the target holds.
    pub fn lens(index: usize) -> TablePair {
        let source = schema(&["a", "b"], &[("a", "b", "ab"), ("b", "a", "ba")]);
        let target = match index {
            0 => source.clone(),
            1 => schema(&["a"], &[("a", "a", "aa")]),
            2 => schema(&["a"], &[]),
            3 => schema(&["a", "b"], &[("a", "b", "ab")]),
            4 => schema(&["b"], &[("a", "b", "ab"), ("b", "a", "ba")]).vertex("a", "record"),
            _ => schema(&["b"], &[("b", "b", "bb")]),
        };
        let migration = RefMigration {
            surviving_verts: target.vertices.keys().cloned().collect(),
            surviving_edges: target.edges.iter().cloned().collect(),
            ..RefMigration::default()
        };
        TablePair::new(
            source,
            target,
            migration,
            "int",
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        )
    }

    /// The production partition of a reference table partition.
    pub fn prod_partition(partition: &reference::Partition) -> Partition {
        let range = |r: &reference::IdRange| IdRange { lo: r.lo, hi: r.hi };
        Partition::new(
            InstanceShapeKind::Table,
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

    /// A sealed source family: its manifest, the segments' canonical bytes
    /// and the production partition.
    pub struct Input {
        /// The source manifest.
        pub manifest: SegmentManifest,
        /// The segments' canonical bytes.
        pub bytes: BTreeMap<SegmentId, Vec<u8>>,
        /// The production partition.
        pub partition: Partition,
    }

    /// Seal the production segments of `table` under `partition`.
    pub fn input(table: &RefTable, partition: &reference::Partition) -> Input {
        let partition = prod_partition(partition);
        let segments: Vec<AnySegment> = partition_table(&prod_table(table), &partition)
            .expect("production split")
            .into_iter()
            .map(AnySegment::Table)
            .collect();
        let schema = Digest(SOURCE_SCHEMA);
        let records = segments
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
                shape: InstanceShapeKind::Table,
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

    pub fn run(
        pair: &TablePair,
        input: &Input,
        budget: Budget,
        store: &mut MemoryStore,
        cancel: Option<u32>,
    ) -> SegmentedRun<SegmentedRestrictError> {
        table_lens::get_segmented(
            &pair.production,
            pair.protocols(),
            (&input.manifest, input.manifest.digest()),
            &input.bytes,
            budget,
            store,
            cancel,
        )
        .expect("the lens's table plan compiles")
    }

    /// The emitted view and complement segments of a complete run.
    pub fn emitted(
        store: &MemoryStore,
        run: &SegmentedRun<SegmentedRestrictError>,
        partition: &Partition,
    ) -> Vec<(AnySegment, Vec<u8>)> {
        let identity = run.run.expect("a run identity");
        partition
            .specs()
            .iter()
            .map(|spec| {
                read_emitted_segment(store, identity, partition, spec.id).expect("emitted segment")
            })
            .collect()
    }

    /// The outcome of one case.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Counts {
        pub complete: u64,
        pub refused: u64,
        pub contracted: u64,
    }

    /// Compare a complete run's emitted segments with the reference's byte
    /// for byte, and return them decoded.
    fn compare_segments(
        pair: &TablePair,
        (table, partition): (&RefTable, &reference::Partition),
        input: &Input,
        store: &MemoryStore,
        baseline: &SegmentedRun<SegmentedRestrictError>,
        expected: &[(reference::TableSegment, reference::TableComplementSegment)],
    ) -> (
        Vec<panproto_inst::segment::TableSegment>,
        Vec<TableComplementSegment>,
    ) {
        let view_schema = reference::Digest(
            pair.production
                .identity(ComplementShape::Functor)
                .target_schema
                .0,
        );
        let output = emitted(store, baseline, &input.partition);
        let mut views = Vec::new();
        let mut complements = Vec::new();
        for ((spec, (view, complement)), (expected_view, expected_complement)) in
            input.partition.specs().iter().zip(&output).zip(expected)
        {
            assert_eq!(
                view.canonical_bytes(&Digest(view_schema.0), &input.partition)
                    .expect("view bytes"),
                reference::canonical_segment_bytes(
                    &reference::AnySegment::Table(expected_view.clone()),
                    view_schema,
                    partition,
                )
                .expect("reference view bytes"),
                "view segment of {table:?} under {partition:?} by {:?}",
                pair.production.compiled
            );
            assert_eq!(
                *complement,
                reference::canonical_complement_segment_bytes(
                    &reference::AnyComplementSegment::Table(expected_complement.clone()),
                )
                .expect("reference complement bytes"),
                "complement segment of {table:?} under {partition:?}"
            );
            let decoded = TableComplementSegment::from_canonical_bytes(complement, spec.id)
                .expect("the emitted complement decodes");
            assert_eq!(decoded.canonical_bytes(), *complement);
            let AnySegment::Table(view) = view.clone() else {
                panic!("a table view segment");
            };
            views.push(view);
            complements.push(decoded);
        }
        (views, complements)
    }

    /// Check the law on one case: a complete run's segments are, byte for
    /// byte, the reference definition's, and glued they are whole-state
    /// table get; a refusal is whole-state get's refusal.
    pub fn check_case(
        pair: &TablePair,
        table: &RefTable,
        partition: &reference::Partition,
        counts: &mut Counts,
    ) -> SegmentedRun<SegmentedRestrictError> {
        let input = input(table, partition);
        let mut store = MemoryStore::default();
        let baseline = run(pair, &input, Budget::unlimited(), &mut store, None);
        let reference_segments =
            reference::partition_table(table, partition).expect("reference split");
        let expected = pair.with_reference(|lens, _| {
            reference::restrict_table_segmented(lens, &reference_segments, partition)
        });
        let whole = table_lens::get(&pair.production, &prod_table(table), pair.protocols());
        match (&baseline.report.status, expected) {
            (RunStatus::Complete { manifest }, Ok(expected)) => {
                assert_eq!(
                    Some(*manifest),
                    baseline.manifest.as_ref().map(SegmentManifest::digest)
                );
                let (views, complements) = compare_segments(
                    pair,
                    (table, partition),
                    &input,
                    &store,
                    &baseline,
                    &expected,
                );
                let view = assemble_table(views, &input.partition).expect("the views assemble");
                let complement = TableComplementSegment::glue(&complements);
                let (whole_view, whole_complement) = whole.expect("whole-state get accepts");
                assert_eq!(ref_table(&view), ref_table(&whole_view));
                assert_eq!(
                    ref_table_complement(&complement),
                    ref_table_complement(&whole_complement)
                );
                counts.complete += 1;
                if !whole_complement.synthesized_fk_pairs.is_empty() {
                    counts.contracted += 1;
                }
            }
            (
                RunStatus::Failed(SegmentedRestrictError::Restrict { segment, error }),
                Err(reference::SegmentedTableRestrictRefusal::Restrict(expected)),
            ) => {
                // The reference locates a contraction refusal at its pair;
                // the table engine locates none, and the segmented run names
                // the segment owning the pair's source row instead.
                let (expected, located) = match &expected {
                    super::RefRestrictError::At { key, error } => {
                        (super::ref_engine_reason(error), Some(*key))
                    }
                    other => (ref_restrict_reason(other), None),
                };
                assert_eq!(
                    Reason::Restrict(restrict_reason(error)),
                    Reason::Restrict(expected.clone()),
                    "refusal of {table:?} under {partition:?}"
                );
                assert_eq!(
                    whole.map(|_| ()).map_err(|refusal| lens_reason(&refusal)),
                    Err(Reason::Restrict(expected))
                );
                if let Some(key) = located {
                    assert_eq!(
                        partition.owner_of_node(key.id).map(|owner| owner.0),
                        Some(segment.0),
                        "the refusing segment owns the contracted pair's source row"
                    );
                }
                assert!(
                    input
                        .partition
                        .specs()
                        .iter()
                        .any(|spec| spec.id == *segment)
                );
                counts.refused += 1;
            }
            (status, expected) => panic!(
                "segmented table get disagrees on {table:?} under {partition:?}: {status:?} / {expected:?}"
            ),
        }
        baseline
    }

    /// Stopping the run at every cancellation point and resuming it against
    /// the same store ends as `baseline`, reusing every committed task.
    fn check_resumption(
        pair: &TablePair,
        input: &Input,
        baseline: &SegmentedRun<SegmentedRestrictError>,
    ) -> u64 {
        let finished = baseline.report.progress.tasks_done;
        let mut points = 0;
        for cancel in 0..=baseline.report.progress.tasks_total {
            points += 1;
            let mut store = MemoryStore::default();
            let stopped = run(pair, input, Budget::unlimited(), &mut store, Some(cancel));
            assert_eq!(stopped.run, baseline.run, "run identity");
            if cancel < finished {
                assert!(
                    matches!(
                        stopped.report.status,
                        RunStatus::Incomplete {
                            reason: RunIncompleteReason::Cancelled
                        }
                    ),
                    "{:?}",
                    stopped.report.status
                );
            }
            let resumed = run(pair, input, Budget::unlimited(), &mut store, None);
            assert_eq!(resumed.report.tasks_reused, cancel.min(finished));
            assert_eq!(
                format!("{:?}", resumed.report.status),
                format!("{:?}", baseline.report.status)
            );
            assert_eq!(
                resumed.manifest.as_ref().map(SegmentManifest::digest),
                baseline.manifest.as_ref().map(SegmentManifest::digest)
            );
        }
        points
    }

    /// The carrier census: tables, partitions and lenses.
    #[test]
    fn segmented_table_get_carrier_census_is_the_closed_form() {
        assert_eq!(
            u64::try_from(segmented_table_restrict_tables().len()).expect("count"),
            segmented_table_restrict_table_count()
        );
        assert_eq!(segmented_table_restrict_table_count(), 741);
        assert_eq!(segmented_table_restrict_partitions().len(), 15);
        let distinct: BTreeSet<String> = segmented_table_restrict_tables()
            .iter()
            .map(|table| format!("{table:?}"))
            .collect();
        assert_eq!(distinct.len(), 741);
    }

    /// Every case of the carrier: every table under every partition and
    /// every lens agrees with the reference definition segment by segment
    /// and glues to whole-state table get, or refuses as it does.
    #[test]
    fn segmented_table_get_agrees_with_the_reference_on_every_case() {
        let tables = segmented_table_restrict_tables();
        let partitions = segmented_table_restrict_partitions();
        let mut counts = Counts::default();
        for index in 0..LENSES.len() {
            let pair = lens(index);
            for table in &tables {
                for partition in &partitions {
                    check_case(&pair, table, partition, &mut counts);
                }
            }
        }
        println!("segmented table get: {counts:?}");
        assert_eq!(counts.complete + counts.refused, 741 * 15 * 6);
        assert_eq!(counts, SEGMENTED_TABLE_GET_COUNTS);
    }

    /// The pinned outcome census of the exhaustive differential, derived
    /// apart from it. Contraction realizes a pair exactly in a table holding
    /// a chain of two pairs through a row of the dropped table: `a -ab-> m
    /// -ba-> a` when `b` is dropped, `b -ba-> m -ab-> b` when `a` is. With
    /// `k_a` and `k_b` rows in the two tables there are `k_b k_a^2` chains of
    /// the first kind (the end rows may coincide), each a table of the
    /// carrier; summed over the assignments, `180`, and `180` of the second
    /// kind by the symmetry exchanging the tables. The lens dropping `b`
    /// with no `a`-to-`a` label refuses the first `180`; the lenses
    /// contracting to `aa` and `bb` contract the first and second `180`. So,
    /// under `15` partitions: `180 * 15 = 2,700` refused, `360 * 15 = 5,400`
    /// contracted, and `741 * 15 * 6 - 2,700 = 63,990` complete.
    const SEGMENTED_TABLE_GET_COUNTS: Counts = Counts {
        complete: 63_990,
        refused: 2_700,
        contracted: 5_400,
    };

    /// Every 97th case of the carrier, stopped at each cancellation point and
    /// resumed, ends as the uninterrupted run.
    #[test]
    fn segmented_table_get_resumes_from_every_cancellation_point() {
        let tables = segmented_table_restrict_tables();
        let partitions = segmented_table_restrict_partitions();
        let mut cases = 0_u64;
        let mut points = 0_u64;
        let mut counts = Counts::default();
        let mut index = 0_usize;
        for lens_index in 0..LENSES.len() {
            let pair = lens(lens_index);
            for table in &tables {
                for partition in &partitions {
                    index += 1;
                    if !index.is_multiple_of(97) {
                        continue;
                    }
                    let baseline = check_case(&pair, table, partition, &mut counts);
                    points += check_resumption(&pair, &input(table, partition), &baseline);
                    cases += 1;
                }
            }
        }
        println!("segmented table get resumption: cases={cases} points={points}");
        assert_eq!(cases, 741 * 15 * 6 / 97);
    }

    /// A lens whose survival carries a predicate is declined with a global
    /// residual before any task.
    #[test]
    fn segmented_table_get_declines_an_expression_before_any_task() {
        let mut pair = lens(0);
        pair.production.compiled.conditional_survival.insert(
            "a".into(),
            panproto_expr::Expr::Lit(panproto_expr::Literal::Bool(true)),
        );
        let tables = segmented_table_restrict_tables();
        let table = tables.last().expect("a table");
        let partitions = segmented_table_restrict_partitions();
        let input = input(table, &partitions[partitions.len() - 1]);
        let mut store = MemoryStore::default();
        let declined = run(&pair, &input, Budget::unlimited(), &mut store, None);
        assert!(
            matches!(
                declined.report.status,
                RunStatus::Residual(Residual::Global { .. })
            ),
            "{:?}",
            declined.report.status
        );
        assert_eq!(declined.report.progress.tasks_done, 0);
    }

    pub fn generated_table(rows: &[u8], pairs: &[(u8, u8)]) -> RefTable {
        let mut table = RefTable::default();
        for (id, kind) in (0_u32..).zip(rows) {
            match kind % 3 {
                1 => {
                    table.rows.insert(
                        id,
                        (
                            "a".to_owned(),
                            BTreeMap::from([(
                                "x".to_owned(),
                                panproto_reference::value::Value::Int(i64::from(id % 2)),
                            )]),
                        ),
                    );
                }
                2 => {
                    table.rows.insert(id, ("b".to_owned(), BTreeMap::new()));
                }
                _ => {}
            }
        }
        for (src, tgt) in pairs {
            let (src, tgt) = (u32::from(*src), u32::from(*tgt));
            let (Some((a, _)), Some((b, _))) = (table.rows.get(&src), table.rows.get(&tgt)) else {
                continue;
            };
            if a != b {
                let name = if a == "a" { "ab" } else { "ba" };
                table.pairs.insert((relation(a, b, name), src, tgt));
            }
        }
        table
    }

    pub fn generated_partition(owners: &[u8]) -> reference::Partition {
        let segments = owners
            .iter()
            .map(|owner| u32::from(*owner % 3))
            .max()
            .unwrap_or(0)
            + 1;
        reference::Partition {
            shape: reference::InstanceShapeKind::Table,
            specs: (0..segments)
                .map(|segment| reference::SegmentSpec {
                    id: reference::SegmentId(segment),
                    nodes: (0_u32..)
                        .zip(owners)
                        .filter(|(_, owner)| u32::from(**owner % 3) == segment)
                        .map(|(id, _)| reference::IdRange { lo: id, hi: id })
                        .collect(),
                    alloc_nodes: None,
                    alloc_edges: None,
                })
                .collect(),
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Generated tables of up to eight rows with up to twelve pairs,
        /// under generated partitions into up to three segments (a segment
        /// may own no row), by every lens of the carrier.
        #[test]
        fn segmented_table_get_agrees_with_the_reference_on_generated_tables(
            rows in prop::collection::vec(0_u8..3, 1..9),
            pairs in prop::collection::vec((0_u8..8, 0_u8..8), 0..12),
            owners in prop::collection::vec(0_u8..3, 8),
            lens_index in 0_usize..LENSES.len(),
        ) {
            let table = generated_table(&rows, &pairs);
            let partition = generated_partition(&owners[..rows.len()]);
            let mut counts = Counts::default();
            check_case(&lens(lens_index), &table, &partition, &mut counts);
        }
    }
}

/// Segmented table put against its reference definition and whole-state
/// table put.
mod segmented_table_put {
    use std::collections::BTreeMap;

    use panproto_inst::identity::{ComplementShape, Digest, IdentityCarrier};
    use panproto_inst::locality::{GlobalReason, Locality};
    use panproto_inst::segment::{
        AnySegment, Budget, EdgeIdRun, ManifestHeader, ManifestRole, MemoryStore, Partition,
        RecordSource, RunIncompleteReason, RunStatus, SegmentId, SegmentManifest, SegmentRecord,
        SegmentedRun, TableComplementSegment, TableSegment, assemble_table, partition_table,
        read_emitted_segment, seal_full, table_restrict_segmented,
    };
    use panproto_inst::segment::{Residual, SegmentError};
    use panproto_inst::{RestrictPlan, RowId};
    use panproto_lens::graph_lens::{HeldView, SegmentedPutError};
    use panproto_lens::table_lens;
    use panproto_reference::enumerate::segments::{
        segmented_table_put_partitions, segmented_table_put_table_count, segmented_table_put_tables,
    };
    use panproto_reference::segment as reference;
    use panproto_reference::value::Value as RefValue;
    use proptest::prelude::*;

    use super::segmented_table_get::{
        LENSES, generated_partition, generated_table, lens, prod_partition,
    };
    use super::{
        Reason, RefSEdge, RefTable, TablePair, lens_reason, prod_table, ref_table,
        ref_table_put_reason,
    };

    /// The schema digest the edited views are sealed under.
    const VIEW_SCHEMA: [u8; 32] = [13; 32];

    /// A view held as segments: its sealed manifest, each segment's bytes
    /// and each segment's complement bytes.
    struct Held {
        manifest: SegmentManifest,
        views: BTreeMap<SegmentId, Vec<u8>>,
        complements: BTreeMap<SegmentId, Vec<u8>>,
    }

    impl Held {
        fn of(
            segments: &[TableSegment],
            complements: &[TableComplementSegment],
            partition: &Partition,
        ) -> Self {
            let schema = Digest(VIEW_SCHEMA);
            let any: Vec<AnySegment> = segments.iter().cloned().map(AnySegment::Table).collect();
            let records = any
                .iter()
                .zip(partition.specs())
                .map(|(segment, spec)| SegmentRecord::of(segment, spec, &schema))
                .collect();
            let runs: Vec<EdgeIdRun<'_>> = any
                .iter()
                .map(|segment| {
                    (
                        segment.segment(),
                        Box::new(segment.edge_id_run().into_iter())
                            as Box<dyn Iterator<Item = panproto_inst::EdgeId>>,
                    )
                })
                .collect();
            let sources = any
                .iter()
                .map(|segment| Box::new(segment.records()) as Box<dyn RecordSource + '_>)
                .collect();
            let manifest = seal_full(
                records,
                ManifestHeader {
                    shape: panproto_inst::InstanceShapeKind::Table,
                    schema,
                    role: ManifestRole::Source,
                    binding: None,
                },
                runs,
                sources,
            )
            .expect("the view seals");
            Self {
                manifest,
                views: any
                    .iter()
                    .map(|segment| {
                        (
                            segment.segment(),
                            segment
                                .canonical_bytes(&schema, partition)
                                .expect("view bytes"),
                        )
                    })
                    .collect(),
                complements: complements
                    .iter()
                    .map(|complement| (complement.segment, complement.canonical_bytes()))
                    .collect(),
            }
        }

        fn input(
            &self,
        ) -> HeldView<'_, BTreeMap<SegmentId, Vec<u8>>, BTreeMap<SegmentId, Vec<u8>>> {
            HeldView {
                manifest: &self.manifest,
                input: self.manifest.digest(),
                views: &self.views,
                complements: &self.complements,
            }
        }
    }

    fn run(
        pair: &TablePair,
        held: &Held,
        store: &mut MemoryStore,
        cancel: Option<u32>,
    ) -> SegmentedRun<SegmentedPutError> {
        table_lens::put_segmented(
            &pair.production,
            pair.protocols(),
            &held.input(),
            Budget::unlimited(),
            store,
            cancel,
        )
        .expect("the lens's table plan compiles")
    }

    /// The column of a table of the carrier.
    pub fn column(table: &str) -> &'static str {
        if table == "a" { "x" } else { "y" }
    }

    /// The view tables of a lens of the carrier with whether each has its
    /// column, and its view relations.
    pub fn view_schema(pair: &TablePair) -> (Vec<(String, bool)>, Vec<RefSEdge>) {
        let tables = ["a", "b"]
            .into_iter()
            .filter(|table| pair.tgt.vertices.contains_key(*table))
            .map(|table| {
                let has = pair
                    .tgt
                    .vertices
                    .contains_key(&format!("{table}.{}", column(table)));
                (table.to_owned(), has)
            })
            .collect();
        let relations = pair
            .tgt
            .edges
            .iter()
            .filter(|edge| edge.kind == "relation")
            .cloned()
            .collect();
        (tables, relations)
    }

    /// The kind of an edit of the carrier.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub enum EditKind {
        /// The view unchanged.
        Identity,
        /// A row removed with its pairs.
        DeleteRow,
        /// A row's column set to `1`.
        SetCell,
        /// A pair removed.
        DeletePair,
        /// A pair of a view relation added between two rows.
        AddPair,
        /// A row without cells added at an id the view does not hold.
        InsertRow,
    }

    /// The edits of the carrier of `view` by `pair`: the identity; every
    /// row removed with its pairs; every row's column, where its view table
    /// has one, set to `1`; every pair removed; every absent pair of a view
    /// relation added between rows of its tables; and a row without cells
    /// of each view table added at each id `partition` owns that the view
    /// does not hold.
    pub fn edits(
        pair: &TablePair,
        view: &RefTable,
        partition: &reference::Partition,
    ) -> Vec<(EditKind, RefTable)> {
        let (tables, relations) = view_schema(pair);
        let mut out = vec![(EditKind::Identity, view.clone())];
        for id in view.rows.keys() {
            let mut edited = view.clone();
            edited.rows.remove(id);
            edited.pairs.retain(|(_, src, tgt)| src != id && tgt != id);
            out.push((EditKind::DeleteRow, edited));
        }
        for (id, (table, _)) in &view.rows {
            if tables.iter().any(|(t, has)| t == table && *has) {
                let mut edited = view.clone();
                edited
                    .rows
                    .get_mut(id)
                    .expect("the row")
                    .1
                    .insert(column(table).to_owned(), RefValue::Int(1));
                out.push((EditKind::SetCell, edited));
            }
        }
        for added in &view.pairs {
            let mut edited = view.clone();
            edited.pairs.remove(added);
            out.push((EditKind::DeletePair, edited));
        }
        for relation in &relations {
            for (src, (a, _)) in &view.rows {
                for (tgt, (b, _)) in &view.rows {
                    let candidate = (relation.clone(), *src, *tgt);
                    if *a == relation.src && *b == relation.tgt && !view.pairs.contains(&candidate)
                    {
                        let mut edited = view.clone();
                        edited.pairs.insert(candidate);
                        out.push((EditKind::AddPair, edited));
                    }
                }
            }
        }
        let owned: std::collections::BTreeSet<u32> = partition
            .specs
            .iter()
            .flat_map(|spec| spec.nodes.iter().flat_map(|range| range.lo..=range.hi))
            .collect();
        for id in owned.into_iter().filter(|id| !view.rows.contains_key(id)) {
            for (table, _) in &tables {
                let mut edited = view.clone();
                edited.rows.insert(id, (table.clone(), BTreeMap::new()));
                out.push((EditKind::InsertRow, edited));
            }
        }
        out
    }

    /// What the carrier showed, by edit kind and outcome.
    #[derive(Debug, Default, PartialEq, Eq)]
    pub struct Census {
        /// The puts restored, by edit kind.
        pub restored: BTreeMap<EditKind, u64>,
        /// The puts refused, by edit kind and refusal.
        pub refused: BTreeMap<(EditKind, String), u64>,
        /// The puts whose restored family crosses a segment boundary.
        pub crossing: u64,
        /// The cases whose get refuses, put nothing.
        pub get_refused: u64,
    }

    /// The name of a refusal's put-domain variant.
    fn variant(reason: &Reason) -> String {
        let text = format!("{reason:?}");
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .nth(1)
            .unwrap_or("")
            .to_owned()
    }

    /// One case's get on both sides.
    pub struct Got {
        pub views: Vec<TableSegment>,
        pub complements: Vec<TableComplementSegment>,
        pub reference_complements: Vec<reference::TableComplementSegment>,
        pub view: RefTable,
    }

    /// The segmented get of `table` under `partition`, on both sides, or
    /// `None` when it refuses (on both sides, checked by L-segments-32).
    pub fn got(
        pair: &TablePair,
        table: &RefTable,
        partition: &reference::Partition,
    ) -> Option<Got> {
        let prod_p = prod_partition(partition);
        let plan = RestrictPlan::compile_for_tables(
            &pair.production.src_schema,
            &pair.production.tgt_schema,
            &pair.production.compiled,
            pair.protocols(),
        )
        .expect("the carrier's plans compile");
        let source = partition_table(&prod_table(table), &prod_p).expect("production split");
        let got = table_restrict_segmented(&plan, &source, &prod_p);
        let reference_source = reference::partition_table(table, partition).expect("split");
        let expected = pair.with_reference(|lens, _| {
            reference::restrict_table_segmented(lens, &reference_source, partition)
        });
        match (got, expected) {
            (Ok(got), Ok(expected)) => {
                let (views, complements): (Vec<_>, Vec<_>) = got.into_iter().unzip();
                let (reference_views, reference_complements): (Vec<_>, Vec<_>) =
                    expected.into_iter().unzip();
                let view = reference::assemble_table(&reference_views, partition).expect("glue");
                Some(Got {
                    views,
                    complements,
                    reference_complements,
                    view,
                })
            }
            (Err(_), Err(_)) => None,
            (got, expected) => {
                panic!("in-memory segmented get disagrees: {got:?} / {expected:?}")
            }
        }
    }

    /// Whole-state table put of `edited` against the joined complement
    /// segments of `got`, stamped as whole-state get stamps them.
    fn whole_put(
        pair: &TablePair,
        got: &Got,
        edited: &RefTable,
    ) -> Result<panproto_inst::FInstance, panproto_lens::LensError> {
        let mut joined = TableComplementSegment::glue(&got.complements);
        joined.stamp(
            pair.production.identity(ComplementShape::Functor),
            &pair.production.src_schema,
        );
        table_lens::put(
            &pair.production,
            &prod_table(edited),
            &joined,
            pair.protocols(),
        )
    }

    /// The restored segments a complete run emitted, each compared byte for
    /// byte with the reference definition's.
    fn restored_segments(
        pair: &TablePair,
        partition: &reference::Partition,
        store: &MemoryStore,
        baseline: &SegmentedRun<SegmentedPutError>,
        expected: &[reference::TableSegment],
    ) -> Vec<TableSegment> {
        let prod_p = prod_partition(partition);
        let schema = pair
            .production
            .identity(ComplementShape::Functor)
            .source_schema;
        let identity = baseline.run.expect("a run identity");
        let mut restored = Vec::new();
        for (spec, expected) in prod_p.specs().iter().zip(expected) {
            let (segment, complement) = read_emitted_segment(store, identity, &prod_p, spec.id)
                .expect("an emitted segment");
            assert!(complement.is_empty(), "put emits no complement");
            assert_eq!(
                segment
                    .canonical_bytes(&schema, &prod_p)
                    .expect("restored bytes"),
                reference::canonical_segment_bytes(
                    &reference::AnySegment::Table(expected.clone()),
                    reference::Digest(schema.0),
                    partition,
                )
                .expect("reference restored bytes"),
                "restored segment under {partition:?} by {:?}",
                pair.production.compiled
            );
            let AnySegment::Table(segment) = segment else {
                panic!("a table segment");
            };
            restored.push(segment);
        }
        restored
    }

    /// Check one put: the run's restored segments are, byte for byte, the
    /// reference definition's, and glued they are whole-state table put of
    /// the glued view; a refusal is whole-state put's and the reference's,
    /// and seals nothing.
    pub fn check_put(
        pair: &TablePair,
        (source, partition): (&RefTable, &reference::Partition),
        got: &Got,
        (kind, edited): (EditKind, &RefTable),
        census: &mut Census,
    ) -> SegmentedRun<SegmentedPutError> {
        let prod_p = prod_partition(partition);
        let views = partition_table(&prod_table(edited), &prod_p).expect("edited split");
        if kind == EditKind::Identity {
            assert_eq!(
                views, got.views,
                "the in-memory get lays the view out by owner"
            );
        }
        let held = Held::of(&views, &got.complements, &prod_p);
        let mut store = MemoryStore::default();
        let baseline = run(pair, &held, &mut store, None);
        let reference_views = reference::partition_table(edited, partition).expect("split");
        let expected = pair.with_reference(|lens, _| {
            reference::put_table_segmented(
                lens,
                &reference_views,
                &got.reference_complements,
                partition,
            )
        });
        let whole = whole_put(pair, got, edited);
        match (&baseline.report.status, &expected, &whole) {
            (RunStatus::Complete { manifest }, Ok(expected), Ok(whole)) => {
                assert_eq!(
                    Some(*manifest),
                    baseline.manifest.as_ref().map(SegmentManifest::digest)
                );
                let restored = restored_segments(pair, partition, &store, &baseline, expected);
                if restored
                    .iter()
                    .any(|segment| !segment.boundary.ghosts.is_empty())
                {
                    census.crossing += 1;
                }
                let glued = assemble_table(restored, &prod_p).expect("the restored segments glue");
                assert_eq!(
                    ref_table(&glued),
                    ref_table(whole),
                    "glued put of {edited:?}"
                );
                if kind == EditKind::Identity {
                    assert_eq!(ref_table(&glued), *source, "GetPut under {partition:?}");
                }
                *census.restored.entry(kind).or_default() += 1;
            }
            (RunStatus::Failed(SegmentedPutError::Lens(error)), Err(expected), Err(whole)) => {
                let reason = lens_reason(error);
                assert_eq!(reason, lens_reason(whole), "{edited:?} under {partition:?}");
                match expected {
                    reference::SegmentedTablePutRefusal::Put(expected) => {
                        assert_eq!(reason, ref_table_put_reason(expected), "{edited:?}");
                    }
                    other @ reference::SegmentedTablePutRefusal::Segment(_) => {
                        assert_ne!(kind, EditKind::Identity, "GetPut refused");
                        panic!("the reference refuses the layout of {edited:?}: {other:?}")
                    }
                }
                let identity = baseline.run.expect("a run identity");
                assert!(
                    read_emitted_segment(&store, identity, &prod_p, SegmentId(0)).is_err(),
                    "a refused put emits nothing"
                );
                assert!(baseline.manifest.is_none(), "a refused put seals nothing");
                *census.refused.entry((kind, variant(&reason))).or_default() += 1;
            }
            outcomes => panic!(
                "segmented table put disagrees on {edited:?} under {partition:?} by {:?}: {outcomes:?}",
                pair.production.compiled
            ),
        }
        baseline
    }

    /// Stopping the put at every cancellation point and resuming it against
    /// the same store ends as `baseline`, reusing every committed task.
    fn check_resumption(
        pair: &TablePair,
        partition: &reference::Partition,
        got: &Got,
        edited: &RefTable,
        baseline: &SegmentedRun<SegmentedPutError>,
    ) -> u64 {
        let prod_p = prod_partition(partition);
        let views = partition_table(&prod_table(edited), &prod_p).expect("edited split");
        let held = Held::of(&views, &got.complements, &prod_p);
        let finished = baseline.report.progress.tasks_done;
        let mut points = 0;
        for cancel in 0..=baseline.report.progress.tasks_total {
            points += 1;
            let mut store = MemoryStore::default();
            let stopped = run(pair, &held, &mut store, Some(cancel));
            assert_eq!(stopped.run, baseline.run, "run identity");
            if cancel < finished {
                assert!(
                    matches!(
                        stopped.report.status,
                        RunStatus::Incomplete {
                            reason: RunIncompleteReason::Cancelled
                        }
                    ),
                    "{:?}",
                    stopped.report.status
                );
            }
            let resumed = run(pair, &held, &mut store, None);
            assert_eq!(resumed.report.tasks_reused, cancel.min(finished));
            assert_eq!(
                format!("{:?}", resumed.report.status),
                format!("{:?}", baseline.report.status)
            );
            assert_eq!(
                resumed.manifest.as_ref().map(SegmentManifest::digest),
                baseline.manifest.as_ref().map(SegmentManifest::digest)
            );
        }
        points
    }

    /// Every put of the carrier: each table under each partition by each
    /// lens, its view and every edit of it; `visit` is given each put and
    /// its position among them.
    fn sweep(
        mut visit: impl FnMut(
            usize,
            &TablePair,
            (&RefTable, &reference::Partition),
            &Got,
            EditKind,
            &RefTable,
        ),
    ) -> Census {
        let tables = segmented_table_put_tables();
        let partitions = segmented_table_put_partitions();
        let mut census = Census::default();
        let mut position = 0;
        for index in 0..LENSES.len() {
            let pair = lens(index);
            for table in &tables {
                for partition in &partitions {
                    let Some(got) = got(&pair, table, partition) else {
                        census.get_refused += 1;
                        continue;
                    };
                    for (kind, edited) in edits(&pair, &got.view, partition) {
                        visit(position, &pair, (table, partition), &got, kind, &edited);
                        position += 1;
                    }
                }
            }
        }
        census
    }

    /// The carrier census: tables and partitions.
    #[test]
    fn segmented_table_put_carrier_census_is_the_closed_form() {
        assert_eq!(
            u64::try_from(segmented_table_put_tables().len()).expect("count"),
            segmented_table_put_table_count()
        );
        assert_eq!(segmented_table_put_table_count(), 105);
        assert_eq!(segmented_table_put_partitions().len(), 5);
    }

    /// Every put of the carrier agrees with the reference definition
    /// segment by segment and glues to whole-state table put, or refuses as
    /// it does, and the unedited view restores the source.
    #[test]
    fn segmented_table_put_agrees_with_the_reference_on_every_case() {
        let mut census = Census::default();
        let tally = sweep(|_, pair, case, got, kind, edited| {
            check_put(pair, case, got, (kind, edited), &mut census);
        });
        census.get_refused = tally.get_refused;
        println!("segmented table put: {census:#?}");
        assert!(
            census.crossing > 0,
            "some restored family crosses a boundary"
        );
        census.crossing = 0;
        assert_eq!(census, derived_census());
    }

    /// The census of the exhaustive differential, derived apart from both
    /// implementations from the carrier's tables and the lenses' meaning.
    ///
    /// Write `ab` and `ba` for the pairs of the two keys and a *chain* from
    /// `a` to `a'` for pairs `a -ab-> m -ba-> a'`. The view of each lens is
    /// read off the source: the identity and dropping `x` keep every row
    /// and pair (dropping `x` leaves `a` without a column); dropping `ba`
    /// keeps every row and the `ab` pairs; dropping `b` keeps the rows of
    /// `a` with one `aa` pair per chain, or, with no `aa` label, refuses
    /// get on a table holding a chain and otherwise keeps no pair; dropping
    /// `a` is the mirror image with `bb`. Then the identity, a removed row
    /// and a set cell always restore; a removed pair is refused as an edited
    /// synthesized pair exactly when contraction made it; an added pair is
    /// refused as an inserted contracted pair exactly when its relation is
    /// a contraction label; and an added row is refused as an id collision
    /// exactly when its id holds a row the lens drops. Every count is the
    /// same under each of the five partitions.
    fn derived_census() -> Census {
        let mut census = Census::default();
        for lens_index in 0..LENSES.len() {
            for table in segmented_table_put_tables() {
                match LensView::of(lens_index, &table) {
                    Some(view) => view.tally(&mut census),
                    None => census.get_refused += 5,
                }
            }
        }
        census
    }

    /// What the census reads of a lens's view of one table: whether each
    /// view row's table has a column, the kept and synthesized pairs, each
    /// view relation as (source rows, target rows, pairs held, whether it
    /// is a contraction label), the number of rows the lens drops, and the
    /// number of view tables.
    struct LensView {
        columns: Vec<bool>,
        kept: u64,
        synthesized: u64,
        relations: Vec<(u64, u64, u64, bool)>,
        dropped: u64,
        tables: u64,
    }

    impl LensView {
        /// The view of `table` by lens `lens_index`, or `None` when its get
        /// refuses.
        fn of(lens_index: usize, table: &RefTable) -> Option<Self> {
            let rows_of = |name: &str| -> u64 {
                table.rows.values().filter(|(t, _)| t == name).count() as u64
            };
            let pairs_of = |name: &str| -> Vec<(u32, u32)> {
                table
                    .pairs
                    .iter()
                    .filter(|(edge, _, _)| edge.name.as_deref() == Some(name))
                    .map(|(_, s, t)| (*s, *t))
                    .collect()
            };
            let chains = |first: &str, second: &str| -> u64 {
                let mut out = std::collections::BTreeSet::new();
                for (s, m) in pairs_of(first) {
                    for (m2, t) in pairs_of(second) {
                        if m == m2 {
                            out.insert((s, t));
                        }
                    }
                }
                out.len() as u64
            };
            let (a, b) = (rows_of("a"), rows_of("b"));
            let (ab, ba) = (pairs_of("ab").len() as u64, pairs_of("ba").len() as u64);
            let rows = |n: u64, has: bool| vec![has; usize::try_from(n).unwrap_or(0)];
            let both = vec![(a, b, ab, false), (b, a, ba, false)];
            Some(match lens_index {
                0 => Self {
                    columns: rows(a + b, true),
                    kept: ab + ba,
                    synthesized: 0,
                    relations: both,
                    dropped: 0,
                    tables: 2,
                },
                1 => Self {
                    columns: rows(a, true),
                    kept: 0,
                    synthesized: chains("ab", "ba"),
                    relations: vec![(a, a, chains("ab", "ba"), true)],
                    dropped: b,
                    tables: 1,
                },
                2 if chains("ab", "ba") > 0 => return None,
                2 => Self {
                    columns: rows(a, true),
                    kept: 0,
                    synthesized: 0,
                    relations: Vec::new(),
                    dropped: b,
                    tables: 1,
                },
                3 => Self {
                    columns: rows(a + b, true),
                    kept: ab,
                    synthesized: 0,
                    relations: vec![(a, b, ab, false)],
                    dropped: 0,
                    tables: 2,
                },
                4 => Self {
                    columns: [rows(a, false), rows(b, true)].concat(),
                    kept: ab + ba,
                    synthesized: 0,
                    relations: both,
                    dropped: 0,
                    tables: 2,
                },
                _ => Self {
                    columns: rows(b, true),
                    kept: 0,
                    synthesized: chains("ba", "ab"),
                    relations: vec![(b, b, chains("ba", "ab"), true)],
                    dropped: a,
                    tables: 1,
                },
            })
        }

        /// Add this view's puts, under each of the five partitions.
        fn tally(&self, census: &mut Census) {
            let mut restored = |kind: EditKind, n: u64| {
                if n > 0 {
                    *census.restored.entry(kind).or_default() += 5 * n;
                }
            };
            let r = self.columns.len() as u64;
            restored(EditKind::Identity, 1);
            restored(EditKind::DeleteRow, r);
            restored(
                EditKind::SetCell,
                self.columns.iter().filter(|has| **has).count() as u64,
            );
            restored(EditKind::DeletePair, self.kept);
            let mut refused = Vec::new();
            refused.push((EditKind::DeletePair, "SynthesizedEdited", self.synthesized));
            for (sources, targets, held, contraction) in &self.relations {
                let n = sources * targets - held;
                if *contraction {
                    refused.push((EditKind::AddPair, "ContractedEdgeInserted", n));
                } else {
                    restored(EditKind::AddPair, n);
                }
            }
            restored(EditKind::InsertRow, (3 - r - self.dropped) * self.tables);
            refused.push((
                EditKind::InsertRow,
                "IdCollision",
                self.dropped * self.tables,
            ));
            for (kind, why, n) in refused {
                if n > 0 {
                    *census.refused.entry((kind, why.to_owned())).or_default() += 5 * n;
                }
            }
        }
    }

    /// Every 23rd put of the carrier, stopped at each cancellation point and
    /// resumed, ends as the uninterrupted run.
    #[test]
    fn segmented_table_put_resumes_from_every_cancellation_point() {
        let mut census = Census::default();
        let (mut cases, mut points) = (0_u64, 0_u64);
        sweep(|position, pair, case, got, kind, edited| {
            if position % 23 != 0 {
                return;
            }
            let baseline = check_put(pair, case, got, (kind, edited), &mut census);
            points += check_resumption(pair, case.1, got, edited, &baseline);
            cases += 1;
        });
        println!("segmented table put resumption: cases={cases} points={points}");
        assert_eq!(cases, 24_045_u64.div_ceil(23));
        assert!(points > cases);
    }

    /// A source schema with a uniqueness key and a lens with a survival
    /// predicate are declined before any task, and a complement entry filed
    /// under a segment that does not own its row is refused at Validate.
    #[test]
    fn segmented_table_put_declines_keys_and_expressions_and_refuses_a_misfiled_entry() {
        let partitions = segmented_table_put_partitions();
        let partition = partitions.last().expect("a partition");
        assert_eq!(partition.specs.len(), 3);
        let tables = segmented_table_put_tables();
        let table = tables
            .iter()
            .find(|table| {
                table.rows.get(&0).is_some_and(|(name, _)| name == "a")
                    && table.rows.get(&1).is_some_and(|(name, _)| name == "b")
            })
            .expect("a table with a row of a at 0 and of b at 1");
        let pair = lens(1);
        let got = got(&pair, table, partition).expect("the get accepts");
        let prod_p = prod_partition(partition);
        let views = partition_table(&prod_table(&got.view), &prod_p).expect("split");
        assert!(!matches!(
            table_lens::table_put_locality(&pair.production, pair.protocols()).expect("compiles"),
            Locality::Global(_)
        ));

        let mut keyed = lens(1);
        keyed.production.src_schema.declarations.keys.insert(
            "a".into(),
            vec![panproto_schema::Key {
                id: "by_x".into(),
                parts: vec![panproto_schema::KeyPart::Path(vec![
                    panproto_schema::Edge {
                        src: "a".into(),
                        tgt: "a.x".into(),
                        kind: "prop".into(),
                        name: Some("x".into()),
                    },
                ])],
                role: panproto_schema::KeyRole::Unique,
                scope: panproto_schema::KeyScope::Global,
                equality: Vec::new(),
            }],
        );
        assert_eq!(
            table_lens::table_put_locality(&keyed.production, keyed.protocols()).expect("compiles"),
            Locality::Global(GlobalReason::KeyDemand {
                vertex: "a".into(),
                key: "by_x".into(),
            })
        );
        let mut predicated = lens(1);
        predicated.production.compiled.conditional_survival.insert(
            "a".into(),
            panproto_expr::Expr::Lit(panproto_expr::Literal::Bool(true)),
        );
        let held = Held::of(&views, &got.complements, &prod_p);
        for declined in [&keyed, &predicated] {
            let mut store = MemoryStore::default();
            let outcome = run(declined, &held, &mut store, None);
            assert!(
                matches!(
                    outcome.report.status,
                    RunStatus::Residual(Residual::Global { .. })
                ),
                "{:?}",
                outcome.report.status
            );
            assert_eq!(outcome.report.progress.tasks_done, 0);
        }

        let mut misplaced = got.complements;
        let moved = misplaced[1]
            .dropped_rows
            .pop_first()
            .expect("segment 1 drops row 1");
        misplaced[0].dropped_rows.insert(moved.0, moved.1);
        let held = Held::of(&views, &misplaced, &prod_p);
        let mut store = MemoryStore::default();
        let outcome = run(&pair, &held, &mut store, None);
        assert!(
            matches!(
                outcome.report.status,
                RunStatus::Failed(SegmentedPutError::Segment(SegmentError::WrongOwner {
                    elem: panproto_inst::ViewRef::Row(RowId(1)),
                    ..
                }))
            ),
            "{:?}",
            outcome.report.status
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        /// Generated tables of up to eight rows with up to twelve pairs,
        /// under generated partitions into up to three segments, by every
        /// lens of the carrier, with one generated edit of the view.
        #[test]
        fn segmented_table_put_agrees_with_the_reference_on_generated_tables(
            rows in prop::collection::vec(0_u8..3, 1..9),
            pairs in prop::collection::vec((0_u8..8, 0_u8..8), 0..12),
            owners in prop::collection::vec(0_u8..3, 8),
            lens_index in 0_usize..LENSES.len(),
            choice in 0_usize..1_000,
        ) {
            let table = generated_table(&rows, &pairs);
            let partition = generated_partition(&owners[..rows.len()]);
            let pair = lens(lens_index);
            if let Some(got) = got(&pair, &table, &partition) {
                let edits = edits(&pair, &got.view, &partition);
                let (kind, edited) = &edits[choice % edits.len()];
                check_put(&pair, (&table, &partition), &got, (*kind, edited), &mut Census::default());
            }
        }
    }
}

/// The segmented table edit lens against its reference definition and the
/// whole-state table edit lens.
mod segmented_table_edit_lens {
    use std::collections::BTreeMap;

    use panproto_inst::RowId;
    use panproto_inst::identity::Digest;
    use panproto_inst::segment::{
        AnySegment, ArtifactKey, Budget, EdgeIdRun, ManifestHeader, ManifestRole, MemoryStore,
        MissingSegment, Partition, RecordSource, RunJournal, SegmentId, SegmentManifest,
        SegmentRecord, SegmentStore, TaskRecord, partition_table, read_emitted_segment, seal_full,
    };
    use panproto_inst::table_delta::{CellChange, TableChange, TableDelta, TableDeltaParts};
    use panproto_lens::{
        SegmentedTable, SegmentedTableContext, SegmentedTableEditLens, ShapeEditError,
        TableContext, TableEditLens,
    };
    use panproto_reference::edit::{TElem, TKey, TPayload, TVal, TableDelta as RefTableDelta};
    use panproto_reference::enumerate::edits::{TableLetterSchema, table_edit_lens_letters};
    use panproto_reference::enumerate::segments::{
        segmented_table_put_partitions, segmented_table_put_tables,
    };
    use panproto_reference::segment as reference;
    use panproto_reference::value::Value as RefValue;

    use super::segmented_table_get::{LENSES, SOURCE_SCHEMA, lens, prod_partition};
    use super::segmented_table_put::{column, view_schema};
    use super::{
        RefSEdge, RefTable, TablePair, prod_edge, prod_table, prod_value, ref_edge, ref_table,
        ref_value,
    };

    type Lens<St> = SegmentedTableEditLens<BTreeMap<SegmentId, Vec<u8>>, St>;
    type Bound = Lens<MemoryStore>;

    const VALUES: [RefValue; 2] = [RefValue::Int(0), RefValue::Int(1)];

    /// A carrier partition with the identifiers `3` to `9` given to its
    /// last segment, so a row an edit adds at the fresh identifier has an
    /// owner.
    pub fn edit_partition(partition: &reference::Partition) -> reference::Partition {
        let mut partition = partition.clone();
        if let Some(last) = partition.specs.last_mut() {
            last.nodes.push(reference::IdRange { lo: 3, hi: 9 });
        }
        partition
    }

    /// The production delta of a reference table delta.
    pub fn production_delta(delta: &RefTableDelta) -> TableDelta {
        let mut parts = TableDeltaParts::default();
        for (side, map) in [(true, &delta.del), (false, &delta.add)] {
            for (element, payload) in map {
                match (element, payload) {
                    (TElem::Row(id), TPayload::Row(table, cells)) => {
                        let row = (
                            table.as_str().into(),
                            cells
                                .iter()
                                .map(|(column, value)| (column.clone(), prod_value(value)))
                                .collect(),
                        );
                        if side {
                            parts.del_rows.insert(RowId(*id), row);
                        } else {
                            parts.add_rows.insert(RowId(*id), row);
                        }
                    }
                    (TElem::Pair(edge, src, tgt), TPayload::Pair) => {
                        let pair = (prod_edge(edge), RowId(*src), RowId(*tgt));
                        if side {
                            parts.del_pairs.insert(pair);
                        } else {
                            parts.add_pairs.insert(pair);
                        }
                    }
                    other => panic!("cross-sort reference table element: {other:?}"),
                }
            }
        }
        for (key, (old, new)) in &delta.attrs {
            match (key, old, new) {
                (TKey::Cell(row, column), TVal::Cell(old), TVal::Cell(new)) => {
                    parts.cells.insert(
                        (RowId(*row), column.clone()),
                        CellChange {
                            old: old.as_ref().map(prod_value),
                            new: new.as_ref().map(prod_value),
                        },
                    );
                }
                (TKey::Table(row), TVal::Table(old), TVal::Table(new)) => {
                    parts.tables.insert(
                        RowId(*row),
                        TableChange {
                            old: old.as_str().into(),
                            new: new.as_str().into(),
                        },
                    );
                }
                other => panic!("cross-sort reference table attribute: {other:?}"),
            }
        }
        TableDelta::normalize(parts)
    }

    /// The reference delta of a production table delta.
    pub fn reference_delta(delta: &TableDelta) -> RefTableDelta {
        let mut out = RefTableDelta::identity();
        let row = |table: &panproto_gat::Name, cells: &panproto_inst::Row| {
            TPayload::Row(
                table.to_string(),
                cells
                    .iter()
                    .map(|(column, value)| (column.clone(), ref_value(value)))
                    .collect(),
            )
        };
        for (id, (table, cells)) in delta.del_rows() {
            out.del.insert(TElem::Row(id.0), row(table, cells));
        }
        for (id, (table, cells)) in delta.add_rows() {
            out.add.insert(TElem::Row(id.0), row(table, cells));
        }
        for (edge, src, tgt) in delta.del_pairs() {
            out.del
                .insert(TElem::Pair(ref_edge(edge), src.0, tgt.0), TPayload::Pair);
        }
        for (edge, src, tgt) in delta.add_pairs() {
            out.add
                .insert(TElem::Pair(ref_edge(edge), src.0, tgt.0), TPayload::Pair);
        }
        for ((row, column), change) in delta.cells() {
            out.attrs.insert(
                TKey::Cell(row.0, column.clone()),
                (
                    TVal::Cell(change.old.as_ref().map(ref_value)),
                    TVal::Cell(change.new.as_ref().map(ref_value)),
                ),
            );
        }
        for (row, change) in delta.tables() {
            out.attrs.insert(
                TKey::Table(row.0),
                (
                    TVal::Table(change.old.to_string()),
                    TVal::Table(change.new.to_string()),
                ),
            );
        }
        out
    }

    /// The one-letter edits of the table edit lens carrier at `table` over
    /// the tables and relations `(tables, relations)`.
    pub fn letters(
        (tables, relations): &(Vec<(String, bool)>, Vec<RefSEdge>),
        table: &RefTable,
    ) -> Vec<RefTableDelta> {
        let columns: Vec<(String, Vec<&str>)> = tables
            .iter()
            .map(|(name, has)| {
                let columns = if *has { vec![column(name)] } else { Vec::new() };
                (name.clone(), columns)
            })
            .collect();
        let tables: Vec<(&str, &[&str])> = columns
            .iter()
            .map(|(name, columns)| (name.as_str(), columns.as_slice()))
            .collect();
        table_edit_lens_letters(
            TableLetterSchema {
                tables: &tables,
                relations,
                values: &VALUES,
            },
            table,
        )
    }

    /// The source letter schema: tables `a` and `b` with their columns and
    /// the relations `ab` and `ba`.
    pub fn source_schema(pair: &TablePair) -> (Vec<(String, bool)>, Vec<RefSEdge>) {
        let tables = vec![("a".to_owned(), true), ("b".to_owned(), true)];
        let relations = pair
            .src
            .edges
            .iter()
            .filter(|edge| edge.kind == "relation")
            .cloned()
            .collect();
        (tables, relations)
    }

    /// A sealed source family with its segment bytes.
    pub struct Sealed {
        pub manifest: SegmentManifest,
        pub bytes: BTreeMap<SegmentId, Vec<u8>>,
        pub partition: Partition,
    }

    /// Seal the production segments of `table` under `partition`.
    pub fn sealed(table: &RefTable, partition: &reference::Partition) -> Sealed {
        let partition = prod_partition(partition);
        let segments: Vec<AnySegment> = partition_table(&prod_table(table), &partition)
            .expect("production split")
            .into_iter()
            .map(AnySegment::Table)
            .collect();
        let schema = Digest(SOURCE_SCHEMA);
        let records = segments
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
                shape: panproto_inst::InstanceShapeKind::Table,
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
        Sealed {
            manifest,
            bytes,
            partition,
        }
    }

    /// An owned copy of the production lens of `pair`.
    fn owned_lens(pair: &TablePair) -> panproto_lens::Lens {
        panproto_lens::Lens {
            compiled: pair.production.compiled.clone(),
            src_schema: pair.production.src_schema.clone(),
            tgt_schema: pair.production.tgt_schema.clone(),
        }
    }

    fn context(pair: &TablePair) -> SegmentedTableContext {
        SegmentedTableContext {
            budget: Budget::unlimited(),
            tables: TableContext {
                src: pair.protocol.clone(),
                tgt: pair.protocol.clone(),
            },
        }
    }

    /// Bind the lens of `pair` to `sealed`, committing to `store`.
    fn bind<St: SegmentStore>(
        pair: &TablePair,
        sealed: &Sealed,
        store: St,
    ) -> Result<Lens<St>, ShapeEditError> {
        SegmentedTableEditLens::segmented_table(
            owned_lens(pair),
            context(pair),
            SegmentedTable {
                manifest: sealed.manifest.clone(),
                input: sealed.manifest.digest(),
                segments: sealed.bytes.clone(),
                store,
            },
        )
    }

    /// The segments a run emitted, as reference values glued.
    fn glued_view<St: SegmentStore>(lens: &Lens<St>, partition: &Partition) -> RefTable {
        let state = lens.segmented_table_state();
        let store = state.store();
        let segments = partition
            .specs()
            .iter()
            .map(|spec| {
                let (segment, _) =
                    read_emitted_segment(&*store, state.get_run(), partition, spec.id)
                        .expect("a view segment");
                let AnySegment::Table(segment) = segment else {
                    panic!("a table segment");
                };
                segment
            })
            .collect::<Vec<_>>();
        ref_table(
            &panproto_inst::segment::assemble_table(segments, partition).expect("the view glues"),
        )
    }

    /// The glued source a lens holds.
    fn glued_source<St: SegmentStore>(lens: &Lens<St>, partition: &Partition) -> RefTable {
        let state = lens.segmented_table_state();
        let segments = partition
            .specs()
            .iter()
            .map(|spec| {
                let bytes = state.source_segment(spec.id).expect("a source segment");
                let AnySegment::Table(segment) =
                    AnySegment::from_canonical_bytes(&bytes, partition)
                        .expect("decodes")
                        .0
                else {
                    panic!("a table segment");
                };
                segment
            })
            .collect::<Vec<_>>();
        ref_table(
            &panproto_inst::segment::assemble_table(segments, partition).expect("the source glues"),
        )
    }

    /// The independent count of [`letters`].
    fn letter_count(
        (tables, relations): &(Vec<(String, bool)>, Vec<RefSEdge>),
        table: &RefTable,
    ) -> u64 {
        let columns: Vec<(String, Vec<&str>)> = tables
            .iter()
            .map(|(name, has)| {
                let columns = if *has { vec![column(name)] } else { Vec::new() };
                (name.clone(), columns)
            })
            .collect();
        let tables: Vec<(&str, &[&str])> = columns
            .iter()
            .map(|(name, columns)| (name.as_str(), columns.as_slice()))
            .collect();
        u64::try_from(
            panproto_reference::enumerate::edits::table_edit_lens_letter_count(
                TableLetterSchema {
                    tables: &tables,
                    relations,
                    values: &VALUES,
                },
                table,
            ),
        )
        .expect("a count")
    }

    /// Per-segment narrowing of an admitted source letter: the lens read,
    /// in order, the segments the reference's read set of the letter names
    /// and then the source segments the reference's edit changed, and every
    /// source segment outside the read set keeps its record. Returns the
    /// number of segments read.
    fn check_narrowing(
        lens: &Bound,
        before: &SegmentManifest,
        (state, next): (
            &reference::SegmentedTableEditState,
            &reference::SegmentedTableEditState,
        ),
        letter: &RefTableDelta,
        partition: &reference::Partition,
    ) -> u64 {
        let read_set = reference::table_read_set(letter, partition);
        let mut expected: Vec<SegmentId> = read_set.iter().map(|id| SegmentId(id.0)).collect();
        expected.extend(
            state
                .source
                .iter()
                .zip(&next.source)
                .filter(|(old, new)| old != new)
                .map(|(old, _)| SegmentId(old.segment.0)),
        );
        let held = lens.segmented_table_state();
        assert_eq!(
            held.reads(),
            expected.as_slice(),
            "the segments {letter:?} reads"
        );
        for record in &held.source_manifest().segments {
            if !read_set.contains(&reference::SegmentId(record.id.0)) {
                assert_eq!(
                    before.record(record.id).map(|old| old.content),
                    Some(record.content),
                    "an unaffected segment keeps its record under {letter:?}"
                );
            }
        }
        u64::try_from(expected.len()).expect("a count")
    }

    /// What the certificate showed.
    #[derive(Debug, Default, PartialEq, Eq)]
    pub struct Tally {
        pub bound: u64,
        pub bind_refused: u64,
        pub source_admitted: u64,
        pub source_refused: u64,
        pub view_admitted: u64,
        pub view_refused: u64,
        /// Source letters by the independent count of the letter carrier.
        pub source_letters: u64,
        /// View letters by the independent count of the letter carrier.
        pub view_letters: u64,
        /// Segments the admitted source letters read.
        pub reads: u64,
    }

    /// Check one carrier case: bind, then translate every source letter and
    /// put back every view letter, each on a freshly bound lens, against the
    /// reference definition and the whole-state table edit lens.
    pub fn check_case(
        pair: &TablePair,
        table: &RefTable,
        partition: &reference::Partition,
        tally: &mut Tally,
    ) {
        check_letters(pair, table, &edit_partition(partition), &|_| true, tally);
    }

    /// Check the letters of one case that `select` admits by position,
    /// source letters first, under `partition` as given.
    pub fn check_letters(
        pair: &TablePair,
        table: &RefTable,
        partition: &reference::Partition,
        select: &dyn Fn(usize) -> bool,
        tally: &mut Tally,
    ) {
        let partition = partition.clone();
        let sealed = sealed(table, &partition);
        let source_segments = reference::partition_table(table, &partition).expect("split");
        let state = pair.with_reference(|lens, _| {
            reference::bind_segmented_table_edit(lens, &source_segments, &partition)
        });
        let mut store = MemoryStore::default();
        let bound = bind(pair, &sealed, std::mem::take(&mut store));
        let (bound, state) = match (bound, state) {
            (Ok(bound), Ok(state)) => (bound, state),
            (Err(_), Err(_)) => {
                tally.bind_refused += 1;
                return;
            }
            outcomes => panic!("binding disagrees on {table:?}: {outcomes:?}"),
        };
        tally.bound += 1;
        let base_store = bound.segmented_table_state().store().clone();
        let view = reference::assemble_table(&state.views, &partition).expect("glued view");
        assert_eq!(
            glued_view(&bound, &sealed.partition),
            view,
            "the bound view"
        );
        let fresh = || bind(pair, &sealed, base_store.clone()).expect("rebinding");
        let whole = || {
            TableEditLens::new(owned_lens(pair), prod_table(table), context(pair).tables)
                .expect("whole-state binding")
        };
        tally.source_letters += letter_count(&source_schema(pair), table);
        tally.view_letters += letter_count(&view_schema(pair), &view);
        let source_letters = letters(&source_schema(pair), table);
        let offset = source_letters.len();
        for (at, letter) in source_letters.into_iter().enumerate() {
            if !select(at) {
                continue;
            }
            let mut lens = fresh();
            let before = lens.segmented_table_state().source_manifest().clone();
            let production = lens.get_edit(&production_delta(&letter)).map_err(drop);
            let expected = pair.with_reference(|reference_lens, _| {
                reference::segmented_table_edit_get(reference_lens, &state, &partition, &letter)
                    .map_err(drop)
            });
            let whole_state = whole().get_edit(&production_delta(&letter));
            match (&production, &expected) {
                (Ok(step), Ok((delta, next))) => {
                    assert_eq!(
                        reference_delta(&step.view_delta),
                        *delta,
                        "view delta of {letter:?}"
                    );
                    let whole_state = whole_state.expect("the whole-state lens admits it");
                    assert_eq!(
                        step.view_delta, whole_state.view_delta,
                        "whole-state view delta"
                    );
                    assert_eq!(
                        glued_view(&lens, &sealed.partition),
                        reference::assemble_table(&next.views, &partition).expect("glue")
                    );
                    assert_eq!(
                        glued_source(&lens, &sealed.partition),
                        reference::assemble_table(&next.source, &partition).expect("glue")
                    );
                    tally.reads +=
                        check_narrowing(&lens, &before, (&state, next), &letter, &partition);
                    tally.source_admitted += 1;
                }
                (Err(()), Err(())) => {
                    assert_eq!(
                        glued_view(&lens, &sealed.partition),
                        view,
                        "a refusal changes nothing"
                    );
                    tally.source_refused += 1;
                }
                outcomes => panic!(
                    "get_edit disagrees on {letter:?} at {table:?} under {partition:?}: {outcomes:?}"
                ),
            }
        }
        for (at, letter) in letters(&view_schema(pair), &view).into_iter().enumerate() {
            if select(offset + at) {
                let case = Bound3 {
                    pair,
                    sealed: &sealed,
                    partition: &partition,
                    state: &state,
                    view: &view,
                };
                check_view_letter(&case, fresh(), whole(), &letter, tally);
            }
        }
    }

    /// One bound case as both sides hold it.
    struct Bound3<'a> {
        pair: &'a TablePair,
        sealed: &'a Sealed,
        partition: &'a reference::Partition,
        state: &'a reference::SegmentedTableEditState,
        view: &'a RefTable,
    }

    /// Put back one view letter on a freshly bound lens against the
    /// reference and the whole-state lens.
    fn check_view_letter(
        case: &Bound3<'_>,
        mut lens: Bound,
        mut whole: TableEditLens,
        letter: &RefTableDelta,
        tally: &mut Tally,
    ) {
        let Bound3 {
            pair,
            sealed,
            partition,
            state,
            view,
        } = case;
        let production = lens.put_edit(&production_delta(letter)).map_err(drop);
        let expected = pair.with_reference(|reference_lens, _| {
            reference::segmented_table_edit_put(reference_lens, state, partition, letter)
                .map_err(drop)
        });
        match (&production, &expected) {
            (Ok(step), Ok((delta, next))) => {
                assert_eq!(
                    reference_delta(&step.source_delta),
                    *delta,
                    "source delta of {letter:?}"
                );
                let whole_state = whole
                    .put_edit(&production_delta(letter))
                    .expect("the whole-state lens admits it");
                assert_eq!(
                    step.source_delta, whole_state.source_delta,
                    "whole-state source delta"
                );
                assert_eq!(
                    glued_view(&lens, &sealed.partition),
                    reference::assemble_table(&next.views, partition).expect("glue")
                );
                assert_eq!(
                    glued_source(&lens, &sealed.partition),
                    reference::assemble_table(&next.source, partition).expect("glue")
                );
                tally.view_admitted += 1;
            }
            (Err(()), Err(())) => {
                assert_eq!(
                    glued_view(&lens, &sealed.partition),
                    **view,
                    "a refusal changes nothing"
                );
                tally.view_refused += 1;
            }
            outcomes => panic!(
                "put_edit disagrees on {letter:?} at {view:?} under {partition:?}: {outcomes:?}"
            ),
        }
    }

    /// Run [`check_case`] on the carrier cases whose position `select`
    /// admits: the number of cases, of selected cases, and the tally.
    fn sweep(select: impl Fn(usize) -> bool) -> (usize, usize, Tally) {
        let tables = segmented_table_put_tables();
        let partitions = segmented_table_put_partitions();
        let mut tally = Tally::default();
        let (mut cases, mut selected) = (0, 0);
        for index in 0..LENSES.len() {
            let pair = lens(index);
            for table in &tables {
                for partition in &partitions {
                    cases += 1;
                    if select(cases - 1) {
                        selected += 1;
                        check_case(&pair, table, partition, &mut tally);
                    }
                }
            }
        }
        (cases, selected, tally)
    }

    /// The pinned tally of the whole certificate. The binding counts are
    /// those of the segmented table put carrier (L-segments-34), and the
    /// letter counts are the independent letter counts of the carrier, both
    /// asserted beside it; the split into admitted and refused letters is
    /// the run's.
    const SEGMENTED_TABLE_EDIT_TALLY: Tally = Tally {
        bound: 3_030,
        bind_refused: 120,
        source_admitted: 40_530,
        source_refused: 11_580,
        view_admitted: 28_115,
        view_refused: 10_945,
        source_letters: 52_110,
        view_letters: 39_060,
        reads: READS,
    };

    /// The segments the admitted source letters read, each letter checked
    /// against its reference read set and changed segments.
    const READS: u64 = 93_576;

    /// Every case of the segmented table put carrier, bound under its
    /// partition with the identifiers `3` to `9` given to the last segment:
    /// every source letter translated and every view letter put back agrees
    /// with the reference definition and the whole-state table edit lens, a
    /// refused letter changes nothing, and an admitted source letter reads
    /// exactly its read set and the segments it changed.
    /// `PANPROTO_TEST_SHARD=i/n` restricts the check to the cases at
    /// positions `i` modulo `n`.
    #[test]
    fn segmented_table_edit_lens_agrees_with_the_reference_and_the_whole_state_lens_on_every_case()
    {
        let shard = super::LawShard::from_environment();
        let (cases, selected, tally) =
            sweep(|position| shard.is_none_or(|shard| shard.contains(position)));
        println!("segmented table edit lens: {tally:?}");
        assert_eq!(cases, 3_150);
        assert_eq!(selected, shard.map_or(cases, |shard| shard.share(cases)));
        assert_eq!(
            tally.source_letters,
            tally.source_admitted + tally.source_refused
        );
        assert_eq!(tally.view_letters, tally.view_admitted + tally.view_refused);
        if shard.is_none() {
            assert_eq!(tally, SEGMENTED_TABLE_EDIT_TALLY);
        }
    }

    /// The eighth of the certificate's cases at positions divisible by
    /// eight, checked on every run.
    #[test]
    fn segmented_table_edit_lens_slice_agrees_with_the_reference_on_every_eighth_case() {
        let (cases, selected, tally) = sweep(|position| position % 8 == 0);
        assert_eq!((cases, selected), (3_150, 394));
        assert!(tally.source_admitted > 0 && tally.source_refused > 0);
        assert!(tally.view_admitted > 0 && tally.view_refused > 0);
    }

    /// A store whose `fail_at`th commit, counted by `commits`, fails.
    #[derive(Clone, Default)]
    struct FailingStore {
        inner: MemoryStore,
        commits: std::rc::Rc<std::cell::Cell<usize>>,
        fail_at: std::rc::Rc<std::cell::Cell<Option<usize>>>,
    }

    impl SegmentStore for FailingStore {
        type Error = MissingSegment;

        fn read(&self, key: &ArtifactKey) -> Result<Option<Vec<u8>>, MissingSegment> {
            Ok(self.inner.read(key).unwrap_or_else(|never| match never {}))
        }

        fn read_at(
            &self,
            key: &ArtifactKey,
            offset: u64,
            len: u64,
        ) -> Result<Option<Vec<u8>>, MissingSegment> {
            Ok(self
                .inner
                .read_at(key, offset, len)
                .unwrap_or_else(|never| match never {}))
        }

        fn stage(&mut self, key: &ArtifactKey, bytes: Vec<u8>) -> Result<(), MissingSegment> {
            self.inner
                .stage(key, bytes)
                .unwrap_or_else(|never| match never {});
            Ok(())
        }

        fn commit(&mut self, key: &ArtifactKey, record: &TaskRecord) -> Result<(), MissingSegment> {
            let count = self.commits.get() + 1;
            self.commits.set(count);
            if self.fail_at.get() == Some(count) {
                return Err(MissingSegment(SegmentId(u32::MAX)));
            }
            self.inner
                .commit(key, record)
                .unwrap_or_else(|never| match never {});
            Ok(())
        }

        fn journal(&self, run: &Digest) -> Result<RunJournal, MissingSegment> {
            Ok(self
                .inner
                .journal(run)
                .unwrap_or_else(|never| match never {}))
        }

        fn staged_uncommitted(&self, run: &Digest) -> Result<Vec<ArtifactKey>, MissingSegment> {
            Ok(self
                .inner
                .staged_uncommitted(run)
                .unwrap_or_else(|never| match never {}))
        }
    }

    /// A three-segment table whose dropped row in the middle segment joins
    /// rows of the other two by contraction, bound by the lens dropping `b`:
    /// a source edit and a view edit, each stopped by a failed commit at
    /// every task of its runs, leave the lens unchanged and, retried, end as
    /// the uninterrupted edit, with nothing left staged.
    #[test]
    fn a_table_edit_stopped_by_a_failed_commit_at_each_task_resumes_to_the_uninterrupted_edit() {
        let pair = lens(1);
        let partitions = segmented_table_put_partitions();
        let partition = edit_partition(partitions.last().expect("a partition"));
        let mut table = RefTable::default();
        for (id, name) in [(0, "a"), (1, "b"), (2, "a")] {
            let cells = if name == "a" {
                BTreeMap::from([("x".to_owned(), RefValue::Int(0))])
            } else {
                BTreeMap::new()
            };
            table.rows.insert(id, (name.to_owned(), cells));
        }
        table
            .pairs
            .insert((RefSEdge::new("a", "b", "relation", Some("ab")), 0, 1));
        table
            .pairs
            .insert((RefSEdge::new("b", "a", "relation", Some("ba")), 1, 2));
        let sealed = sealed(&table, &partition);
        let source_edit = letters(&source_schema(&pair), &table)
            .into_iter()
            .find(|letter| letter.del.keys().any(|key| matches!(key, TElem::Pair(..))))
            .map(|letter| production_delta(&letter))
            .expect("a pair deletion");
        let bound = bind(&pair, &sealed, MemoryStore::default()).expect("binds");
        let view = glued_view(&bound, &sealed.partition);
        let view_edit = letters(&view_schema(&pair), &view)
            .into_iter()
            .find(|letter| letter.attrs.keys().any(|key| matches!(key, TKey::Cell(..))))
            .map(|letter| production_delta(&letter))
            .expect("a cell change");
        let base = bound.segmented_table_state().store().clone();
        for put in [false, true] {
            let edit = if put { &view_edit } else { &source_edit };
            let step = |lens: &mut Lens<FailingStore>| {
                if put {
                    lens.put_edit(edit)
                } else {
                    lens.get_edit(edit)
                }
            };
            let store = FailingStore {
                inner: base.clone(),
                ..FailingStore::default()
            };
            let (commits, fail_at) = (store.commits.clone(), store.fail_at.clone());
            let mut uninterrupted = bind(&pair, &sealed, store.clone()).expect("rebinds");
            let before = commits.get();
            let expected = step(&mut uninterrupted).expect("the edit is admitted");
            let total = commits.get() - before;
            assert!(total > 0, "the edit commits tasks");
            for failing in 1..=total {
                let mut lens = bind(
                    &pair,
                    &sealed,
                    FailingStore {
                        inner: base.clone(),
                        commits: commits.clone(),
                        fail_at: fail_at.clone(),
                    },
                )
                .expect("rebinds");
                fail_at.set(Some(commits.get() + failing));
                let stopped = step(&mut lens);
                assert!(
                    matches!(stopped, Err(ShapeEditError::Incomplete(_))),
                    "{:?}",
                    stopped.map(|step| step.view_delta)
                );
                assert_eq!(glued_view(&lens, &sealed.partition), view);
                assert_eq!(glued_source(&lens, &sealed.partition), table);
                assert_eq!(lens.sequence(), 0);
                fail_at.set(None);
                let resumed = step(&mut lens).expect("the retried edit is admitted");
                assert_eq!(resumed.view_delta, expected.view_delta);
                assert_eq!(resumed.source_delta, expected.source_delta);
                assert_eq!(
                    glued_view(&lens, &sealed.partition),
                    glued_view(&uninterrupted, &sealed.partition)
                );
                assert_eq!(
                    glued_source(&lens, &sealed.partition),
                    glued_source(&uninterrupted, &sealed.partition)
                );
                let run = lens.segmented_table_state().get_run();
                assert_eq!(
                    lens.segmented_table_state()
                        .store()
                        .staged_uncommitted(&run)
                        .expect("the journal reads"),
                    Vec::<ArtifactKey>::new()
                );
            }
        }
    }

    /// A lens whose restriction carries a survival predicate is declined
    /// when bound.
    #[test]
    fn a_table_lens_with_an_expression_is_declined_when_bound() {
        let mut pair = lens(0);
        pair.production.compiled.conditional_survival.insert(
            "a".into(),
            panproto_expr::Expr::Lit(panproto_expr::Literal::Bool(true)),
        );
        let partitions = segmented_table_put_partitions();
        let tables = segmented_table_put_tables();
        let sealed = sealed(tables.last().expect("a table"), &partitions[0]);
        assert!(matches!(
            bind(&pair, &sealed, MemoryStore::default()),
            Err(ShapeEditError::Residual(
                panproto_inst::segment::Residual::Global { .. }
            ))
        ));
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

        /// Generated tables of up to eight rows with up to twelve pairs,
        /// under generated partitions into up to three segments, by every
        /// lens of the carrier: one generated source letter and one
        /// generated view letter against the reference and the whole-state
        /// lens.
        #[test]
        fn segmented_table_edit_lens_agrees_with_the_reference_on_generated_tables(
            rows in proptest::collection::vec(0_u8..3, 1..9),
            pairs in proptest::collection::vec((0_u8..8, 0_u8..8), 0..12),
            owners in proptest::collection::vec(0_u8..3, 8),
            lens_index in 0_usize..LENSES.len(),
            choice in (0_usize..64, 0_usize..64),
        ) {
            let table = super::segmented_table_get::generated_table(&rows, &pairs);
            let partition =
                super::segmented_table_get::generated_partition(&owners[..rows.len()]);
            let pair = lens(lens_index);
            let sources = letters(&source_schema(&pair), &table).len();
            let mut tally = Tally::default();
            check_letters(
                &pair,
                &table,
                &partition,
                &|at| at == choice.0 % sources || (at >= sources && at - sources == choice.1),
                &mut tally,
            );
        }
    }
}

mod segment_collection {
    use std::collections::{BTreeMap, BTreeSet};

    use panproto_inst::identity::Digest;
    use panproto_inst::segment::{
        ArtifactKey, ArtifactKind, Budget, CollectableStore, MemoryStore, Phase, RunStatus,
        SegmentId, SegmentManifest, SegmentStore, Stored, TaskKey, TaskRecord, collect_garbage,
        read_emitted_segment,
    };
    use panproto_reference::enumerate::segments::{
        segmented_table_restrict_partitions, segmented_table_restrict_tables,
    };
    use panproto_reference::segment as reference;

    use super::segmented_table_get::{LENSES, input, lens, run};

    /// The three runs of the exhaustive carrier.
    const RUNS: [Digest; 3] = [Digest([1; 32]), Digest([2; 32]), Digest([3; 32])];

    /// The two artifacts each run of the carrier may store.
    const KEYS: [(TaskKey, ArtifactKind); 2] = [
        (
            TaskKey {
                phase: Phase::Validate,
                segment: Some(SegmentId(0)),
            },
            ArtifactKind::Segment,
        ),
        (
            TaskKey {
                phase: Phase::Assemble,
                segment: None,
            },
            ArtifactKind::Assembly,
        ),
    ];

    const fn reference_phase(phase: Phase) -> reference::Phase {
        match phase {
            Phase::Validate => reference::Phase::Validate,
            Phase::Export => reference::Phase::Export,
            Phase::Summarize => reference::Phase::Summarize,
            Phase::Assemble => reference::Phase::Assemble,
            Phase::Emit => reference::Phase::Emit,
            Phase::Seal => reference::Phase::Seal,
        }
    }

    const fn reference_kind(kind: ArtifactKind) -> reference::ArtifactKind {
        match kind {
            ArtifactKind::Segment => reference::ArtifactKind::Segment,
            ArtifactKind::ComplementSegment => reference::ArtifactKind::ComplementSegment,
            ArtifactKind::Export => reference::ArtifactKind::Export,
            ArtifactKind::Summary => reference::ArtifactKind::Summary,
            ArtifactKind::Assembly => reference::ArtifactKind::Assembly,
            ArtifactKind::Manifest => reference::ArtifactKind::Manifest,
            ArtifactKind::EdgeIdRun => reference::ArtifactKind::EdgeIdRun,
        }
    }

    fn reference_task(task: TaskKey) -> reference::TaskKey {
        reference::TaskKey {
            phase: reference_phase(task.phase),
            segment: task.segment.map(|segment| reference::SegmentId(segment.0)),
        }
    }

    fn reference_key(key: &ArtifactKey) -> reference::ArtifactKey {
        reference::ArtifactKey {
            run: reference::Digest(key.run.0),
            task: reference_task(key.task),
            kind: reference_kind(key.kind),
        }
    }

    /// What `store` holds, in the reference's terms.
    fn contents(store: &MemoryStore) -> reference::StoreContents {
        let Ok(stored) = store.stored();
        let Ok(journaled) = store.journaled();
        reference::StoreContents {
            artifacts: stored
                .iter()
                .map(|(key, state)| {
                    (
                        reference_key(key),
                        match state {
                            Stored::Staged => reference::StoredState::Staged,
                            Stored::Committed => reference::StoredState::Committed,
                        },
                    )
                })
                .collect(),
            journals: journaled
                .iter()
                .map(|run| {
                    let Ok(journal) = store.journal(run);
                    (
                        reference::Digest(run.0),
                        journal.tasks.keys().copied().map(reference_task).collect(),
                    )
                })
                .collect(),
        }
    }

    /// The store of carrier code `code`: for each run and key, two bits, the
    /// first committing the artifact and the second staging it (again).
    fn store_of(code: u32) -> MemoryStore {
        let mut store = MemoryStore::default();
        let mut bits = code;
        for (r, run) in RUNS.iter().enumerate() {
            for (k, (task, kind)) in KEYS.iter().enumerate() {
                let key = ArtifactKey {
                    run: *run,
                    task: *task,
                    kind: *kind,
                };
                let tag = u8::try_from(r * 2 + k).expect("small");
                if bits & 1 == 1 {
                    let Ok(()) = store.stage(&key, vec![tag]);
                    let Ok(()) = store.commit(
                        &key,
                        &TaskRecord {
                            inputs: Digest([tag; 32]),
                            output: Digest([tag; 32]),
                            bytes: 1,
                        },
                    );
                }
                if bits & 2 == 2 {
                    let Ok(()) = store.stage(&key, vec![tag, tag]);
                }
                bits >>= 2;
            }
        }
        store
    }

    /// Every store of the carrier under every set of roots: collection
    /// leaves exactly the reference's contents, reports what it removed,
    /// leaves every reachable artifact's bytes, and a second collection
    /// removes nothing.
    #[test]
    fn collection_keeps_exactly_what_the_roots_reach_on_every_bounded_store() {
        let mut cases = 0_u64;
        let mut removed = 0_u64;
        for code in 0..(1_u32 << 12) {
            for mask in 0..8_u32 {
                let roots: BTreeSet<Digest> = RUNS
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| mask & (1 << i) != 0)
                    .map(|(_, run)| *run)
                    .collect();
                let reference_roots: BTreeSet<reference::Digest> =
                    roots.iter().map(|run| reference::Digest(run.0)).collect();
                let mut store = store_of(code);
                let before = contents(&store);
                let reads: BTreeMap<ArtifactKey, Option<Vec<u8>>> = KEYS
                    .iter()
                    .flat_map(|(task, kind)| {
                        RUNS.iter().map(move |run| ArtifactKey {
                            run: *run,
                            task: *task,
                            kind: *kind,
                        })
                    })
                    .map(|key| {
                        let Ok(bytes) = store.read(&key);
                        (key, bytes)
                    })
                    .collect();
                let Ok(collection) = collect_garbage(&mut store, &roots);
                let after = contents(&store);
                let expected = reference::collected(&before, &reference_roots);
                assert_eq!(after, expected, "store {code:#x} under roots {mask:#b}");
                assert_eq!(
                    collection
                        .journals
                        .iter()
                        .map(|run| reference::Digest(run.0))
                        .collect::<BTreeSet<_>>(),
                    before
                        .journals
                        .keys()
                        .filter(|run| !expected.journals.contains_key(*run))
                        .copied()
                        .collect::<BTreeSet<_>>()
                );
                assert_eq!(
                    u64::try_from(collection.removed.len()).expect("count"),
                    u64::try_from(before.artifacts.len() - after.artifacts.len()).expect("count")
                );
                assert_eq!(collection.kept, after.artifacts.len());
                for (key, bytes) in &reads {
                    let Ok(now) = store.read(key);
                    if roots.contains(&key.run) {
                        assert_eq!(&now, bytes, "a reachable artifact keeps its bytes");
                    } else {
                        assert_eq!(now, None);
                    }
                }
                let Ok(again) = collect_garbage(&mut store, &roots);
                assert!(again.journals.is_empty() && again.removed.is_empty());
                cases += 1;
                removed += u64::try_from(collection.removed.len()).expect("count");
            }
        }
        println!("segment collection: cases={cases} removed={removed}");
        assert_eq!(cases, COLLECTION_CASES);
        assert_eq!(removed, COLLECTION_REMOVED);
    }

    /// The carrier's case count: `4^2` states of the two keys of each of
    /// three runs, under each of the `2^3` sets of roots.
    const COLLECTION_CASES: u64 = 16 * 16 * 16 * 8;

    /// The artifacts the carrier's collections remove, derived apart from
    /// it: a key in one of its four states holds `0`, `1`, `1` or `2`
    /// artifacts (none, staged, committed, both), `4` over the states; under
    /// a root its staged copy is removed (`2` over the states), and otherwise
    /// all are (`4`). Each of the `3 * 2 = 6` keys, over the `4^5` states of
    /// the other keys and the `4` root sets containing its run and the `4`
    /// that do not: `6 * 4^5 * (4 * 2 + 4 * 4) = 147,456`.
    const COLLECTION_REMOVED: u64 = 6 * 1024 * (4 * 2 + 4 * 4);

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(256))]

        /// Generated sequences of stages and commits over three runs and two
        /// artifacts each, under generated roots, collect to the reference's
        /// contents.
        #[test]
        fn collection_keeps_exactly_what_the_roots_reach_on_generated_stores(
            ops in proptest::collection::vec((0_usize..3, 0_usize..2, proptest::bool::ANY, 0_u8..4), 0..24),
            mask in 0_u32..8,
        ) {
            let mut store = MemoryStore::default();
            for (run, key, commit, byte) in ops {
                let (task, kind) = KEYS[key];
                let key = ArtifactKey { run: RUNS[run], task, kind };
                if commit {
                    let Ok(()) = store.commit(
                        &key,
                        &TaskRecord {
                            inputs: Digest([byte; 32]),
                            output: Digest([byte; 32]),
                            bytes: 1,
                        },
                    );
                } else {
                    let Ok(()) = store.stage(&key, vec![byte]);
                }
            }
            let roots: BTreeSet<Digest> = RUNS
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, run)| *run)
                .collect();
            let reference_roots: BTreeSet<reference::Digest> =
                roots.iter().map(|run| reference::Digest(run.0)).collect();
            let before = contents(&store);
            let Ok(_) = collect_garbage(&mut store, &roots);
            proptest::prop_assert_eq!(contents(&store), reference::collected(&before, &reference_roots));
        }
    }

    /// A run stopped at each cancellation point, beside a complete run of
    /// another lens, resumes after collection from its own run alone to
    /// the uninterrupted run, reusing every task it committed, while the
    /// other run's journal and artifacts are gone; collected from no root,
    /// it recomputes every task and ends the same.
    #[test]
    fn a_stopped_run_resumes_after_collection_from_its_own_run() {
        let tables = segmented_table_restrict_tables();
        let partitions = segmented_table_restrict_partitions();
        let (mut cases, mut points) = (0_u64, 0_u64);
        let mut index = 0_usize;
        for lens_index in 0..LENSES.len() {
            let pair = lens(lens_index);
            let other = lens((lens_index + 1) % LENSES.len());
            for table in &tables {
                for partition in &partitions {
                    index += 1;
                    if !index.is_multiple_of(389) {
                        continue;
                    }
                    cases += 1;
                    let input = input(table, partition);
                    let mut fresh = MemoryStore::default();
                    let baseline = run(&pair, &input, Budget::unlimited(), &mut fresh, None);
                    let finished = baseline.report.progress.tasks_done;
                    for cancel in 0..=baseline.report.progress.tasks_total {
                        points += 1;
                        for keep in [true, false] {
                            let mut store = MemoryStore::default();
                            let stopped =
                                run(&pair, &input, Budget::unlimited(), &mut store, Some(cancel));
                            let junk = run(&other, &input, Budget::unlimited(), &mut store, None);
                            let roots: BTreeSet<Digest> = if keep {
                                stopped.run.into_iter().collect()
                            } else {
                                BTreeSet::new()
                            };
                            let Ok(_) = collect_garbage(&mut store, &roots);
                            if let Some(junk) = junk.run {
                                let Ok(journal) = store.journal(&junk);
                                assert!(journal.tasks.is_empty());
                            }
                            let resumed = run(&pair, &input, Budget::unlimited(), &mut store, None);
                            assert_eq!(
                                resumed.report.tasks_reused,
                                if keep { cancel.min(finished) } else { 0 }
                            );
                            assert_eq!(
                                format!("{:?}", resumed.report.status),
                                format!("{:?}", baseline.report.status)
                            );
                            assert_eq!(
                                resumed.manifest.as_ref().map(SegmentManifest::digest),
                                baseline.manifest.as_ref().map(SegmentManifest::digest)
                            );
                            if let (RunStatus::Complete { .. }, Some(identity)) =
                                (&resumed.report.status, resumed.run)
                            {
                                let Ok(_) =
                                    collect_garbage(&mut store, &BTreeSet::from([identity]));
                                for spec in input.partition.specs() {
                                    assert_eq!(
                                        read_emitted_segment(
                                            &store,
                                            identity,
                                            &input.partition,
                                            spec.id
                                        )
                                        .expect("a root's emitted segment survives")
                                        .1,
                                        read_emitted_segment(
                                            &fresh,
                                            baseline.run.expect("a run"),
                                            &input.partition,
                                            spec.id
                                        )
                                        .expect("emitted")
                                        .1
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        println!("collection resumption: cases={cases} points={points}");
        assert_eq!(cases, 741 * 15 * 6 / 389);
    }
}

/// The segmented graph edit lens after its store is collected from its live
/// runs translates as the uncollected lens does.
mod segmented_graph_edit_lens_collection {
    use std::collections::BTreeSet;

    use panproto_inst::identity::ComplementShape;
    use panproto_inst::segment::CollectableStore;
    use panproto_reference::enumerate::edits::graph_edit_lens_letters;
    use panproto_reference::enumerate::segments::segmented_restrict_cases;

    use super::graph_edit_lens_differential::prod_delta;
    use super::segmented_graph_edit_lens::{Case, bind, production_state, with_spare};
    use super::{BOUNDED_LENS_FIELD, BOUNDED_LENS_VALUE};

    /// Every 97th case of the segmented restriction carrier, bound, takes
    /// up to three admitted source letters in turn on two copies of the
    /// lens, one collected after every step: each step translates alike,
    /// every segment of the two is byte for byte the same, and the collected
    /// store holds journals and artifacts of the live runs only.
    #[test]
    fn a_collected_segmented_graph_edit_lens_translates_as_the_uncollected_one() {
        let (mut cases, mut steps) = (0_u64, 0_u64);
        for case in segmented_restrict_cases().step_by(97) {
            let partition = with_spare(&case.partition, 3);
            let edit_case = Case {
                schema: &case.schema,
                migration: &case.migration,
                partition: &partition,
            };
            let Some((production, _, _)) = bind(&edit_case, &case.graph) else {
                continue;
            };
            cases += 1;
            let view_schema = edit_case
                .lens()
                .identity(ComplementShape::Graph)
                .target_schema;
            let (mut kept, mut collected) = (production.clone(), production);
            let letters = graph_edit_lens_letters(
                &case.schema,
                &case.graph,
                BOUNDED_LENS_FIELD,
                &BOUNDED_LENS_VALUE,
            );
            let mut taken = 0;
            for letter in &letters {
                if taken == 3 {
                    break;
                }
                let edit = prod_delta(letter);
                let mut trial = kept.clone();
                let Ok(expected) = trial.get_edit(&edit) else {
                    continue;
                };
                let produced = collected
                    .get_edit(&edit)
                    .expect("the collected lens admits what the uncollected one does");
                assert_eq!(produced, expected);
                kept = trial;
                collected
                    .segmented_state()
                    .collect_garbage()
                    .expect("the memory store collects");
                assert_eq!(
                    production_state(&collected, view_schema),
                    production_state(&kept, view_schema)
                );
                let state = collected.segmented_state();
                let live = state.live_runs();
                let store = state.store();
                let Ok(journaled) = store.journaled();
                let Ok(stored) = store.stored();
                assert!(journaled.iter().all(|run| live.contains(run)));
                assert!(
                    stored
                        .iter()
                        .map(|(key, _)| key.run)
                        .collect::<BTreeSet<_>>()
                        .is_subset(&live)
                );
                taken += 1;
                steps += 1;
            }
        }
        println!("collected edit lens: cases={cases} steps={steps}");
        assert_eq!(cases, 1_163);
        assert!(steps > 0);
    }
}
