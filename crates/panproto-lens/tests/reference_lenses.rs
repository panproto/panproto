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
    C11_FIELD, C11_VALUE, C11AddField, C11DropField, C11Layer, C11OpTerm, C11ReadField, C11Stage,
    C11SurvivalFalse, GraphPutCascadeLawCase, GraphPutDomainFamily, GraphPutLawCase,
    GraphTreeRouteLawCase, LensEquivalenceCase, LensLawCertificateCase, PathBoundaryWitness,
    PathLawCarrier, PathLawCase, ProtolensInstanceCase, ProtolensSchemaCase,
    TableFusionBoundaryWitness, TreeFusionBoundaryWitness, TreePutLawCase,
    for_each_c11_tree_square_case, for_each_graph_put_cascade_law_case,
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
            format!("KeyRefused {table} {key} {}", ref_key_refusal_reason(reason))
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
    const UNCLAIMED: u8 = 0;
    const CLAIMED: u8 = 1;
    const COMPLETED: u8 = 2;
    const MISMATCH: u8 = 3;
    type Outcome = Result<Census, (usize, Disagreement)>;

    let (production, reference) = (&sides.0, &sides.1);
    let threads = std::env::var("PANPROTO_TEST_WORKERS")
        .ok()
        .and_then(|workers| workers.parse::<usize>().ok())
        .filter(|workers| *workers > 0)
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, std::num::NonZero::get));
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let lowest_mismatch = AtomicUsize::new(usize::MAX);
    let states: Vec<_> = (0..units.len()).map(|_| AtomicU8::new(UNCLAIMED)).collect();
    let outcomes: Vec<Outcome> = std::thread::scope(|scope| {
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
    let recorded_lowest = lowest_mismatch.load(Ordering::Acquire);
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
    let schema = panproto_reference::enumerate::lenses::c11_schema(&["a"], &[]);
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
    let schema = panproto_reference::enumerate::lenses::c11_schema(&["a"], &[]);
    let migration = RefMigration {
        surviving_verts: ["a".to_owned()].into(),
        transformed: ["a".to_owned()].into(),
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
    let mut reference_keyed = panproto_reference::enumerate::lenses::c11_schema(&["a"], &[]);
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

fn prod_key_equality(equality: &panproto_reference::keys::KeyEquality) -> panproto_schema::KeyEquality {
    use panproto_reference::keys::{
        KeyEquality as R, NumericPrimitive as RefPrimitive, SqlCollation as RefCollation,
        SqlPad as RefPad,
    };
    use panproto_schema::{KeyEquality, NumericPrimitive, SqlCollation, SqlPad};
    match equality {
        R::Representation => KeyEquality::Representation,
        R::NumericValue => KeyEquality::NumericValue,
        R::NumericValueWithinKind(primitive) => KeyEquality::NumericValueWithinKind(match primitive {
            RefPrimitive::Decimal => NumericPrimitive::Decimal,
            RefPrimitive::Float(width) => NumericPrimitive::Float(*width),
            RefPrimitive::Any => NumericPrimitive::Any,
        }),
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
    use panproto_reference::schema::{KeyPart as RefPart, KeyRole as RefRole, KeyScope as RefScope};
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

/// An identity lens over the C11 schema on `vertices` and `edges` whose
/// vertex `a` declares `keys`, built on both sides.
fn keyed_graph_pair(
    vertices: &[&str],
    edges: &[(&str, &str)],
    keys: Vec<panproto_reference::schema::Key>,
) -> Pair {
    let mut reference = panproto_reference::enumerate::lenses::c11_schema(vertices, edges);
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

/// Amendment-level contract: a declared key whose restored value has no
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
/// keys: declared equality, non-unique roles, the primary-key presence rule
/// and elements outside a scoped key's domain.
#[test]
fn graph_put_key_clause_agrees_with_the_reference_on_declared_semantics() {
    use panproto_reference::keys::KeyEquality as Eq;
    use panproto_reference::schema::{KeyRole, KeyScope};
    let one = vec![Some(RefValue::Int(1)), Some(RefValue::Float(1.0_f64.to_bits()))];
    let same = vec![
        Some(RefValue::Str("same".to_owned())),
        Some(RefValue::Str("same".to_owned())),
    ];
    let cases = [
        (
            "numeric equality",
            value_key("k", KeyRole::Unique, KeyScope::Global, vec![Eq::NumericValue]),
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
            "sort-key role",
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
            value_key("k", KeyRole::Unique, KeyScope::Within("b".to_owned()), Vec::new()),
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

fn assert_graph_malformed_fan_domain_family() -> u16 {
    let fan_schema = c11_fan_schema();
    let fan_pair = Pair::new(
        fan_schema.clone(),
        fan_schema.clone(),
        reference_carrier_migration(&fan_schema),
    );
    let labels = panproto_reference::enumerate::lenses::c11_fan_labels();
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
            let stages = c11_stages();
            let (_, stage, source, edited) = c11_graph_domain_witnesses()
                .iter()
                .find(|(candidate, _, _, _)| *candidate == family)
                .unwrap_or_else(|| panic!("the C11 carrier has no {family:?} witness"));
            let pair = Pair::from_stage(&stages[*stage]);
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
        gf5_pair(Gf5Fixture::DropTarget),
        gf5_pair(Gf5Fixture::VisibleThenHidden),
    ];
    for (fixture, source, edited) in &cascades {
        let pair = &pairs[usize::from(*fixture == Gf5Fixture::VisibleThenHidden)];
        assert_eq!(
            production_put(pair, source, edited),
            reference_strict_put(pair, source, edited),
            "Delta-fragment put at {fixture:?}"
        );
        let (production, largest, universal) = gf5_cascade_outcome(pair, source, edited);
        assert_eq!(
            production.and_then(|outcome| outcome.put.ok()),
            largest,
            "largest exact preimage at {fixture:?}"
        );
        assert!(universal, "cascade universal property at {fixture:?}");
    }
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
    assert_eq!(production_field_order, RefTableComplement::FIELD_ORDER);
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
// The five law gates (L.8): production checker against the reference checker
// ---------------------------------------------------------------------------

use panproto_lens::LawViolation;
use panproto_reference::lens::{LawGateStatus, LawReport as RefLawReport};

/// The comparable part of a native five-gate report: the first gate that does
/// not pass, or `None` when every gate passes. L.8 retains this own-basis
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
    c11_table_state_count, c11_table_states, c11_tree_square_count_certificate,
    c11_tree_state_count, sigma2_count, sigma2_schemas,
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

fn graph_put_domain_family_bit(reason: &str) -> u16 {
    GraphPutDomainFamily::ALL
        .into_iter()
        .position(|family| reason.starts_with(family.name()))
        .map_or(0, |index| 1 << index)
}

fn graph_lens_agrees_with_the_reference_on_cases(units: &[(usize, RefGraph)]) -> u16 {
    let schemas = sigma2_schemas();
    assert_eq!(schemas.len(), sigma2_count());
    let stages = c11_stages();
    let certificate = c11_path_count_certificate();
    assert_eq!(stages.len(), certificate.stages);
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
    let observed_domain_families = std::sync::atomic::AtomicU16::new(0);
    let compared = assert_agree(agree_in_parallel(
        &bound,
        units,
        || stages.iter().map(Pair::from_stage).collect::<Vec<Pair>>(),
        |pairs, (stage, source)| {
            let cases = c11_graph_unit_cases(pairs, &stages, *stage, source);
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
        "C11 graph cases: gets={}, unedited views={views}, one-step edits={edits}, cases={total}",
        units.len()
    );
    assert_eq!(compared, u64::try_from(total).unwrap());
    observed_domain_families.into_inner()
}

// ---------------------------------------------------------------------------
// Trees: get, GetPut and the five-gate reports on the C11 tree states
// ---------------------------------------------------------------------------

use panproto_inst::{Fan, WInstance};
use panproto_reference::enumerate::lenses::c11_tree_states;
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

fn tree_lens_agrees_with_the_reference_on_cases(cases: &[(usize, RefTree)]) {
    let stages = c11_stages();
    let certificate = c11_path_count_certificate();
    assert_eq!(cases.len(), certificate.stage_tree_states);
    let bound = format!(
        "C11 tree scope: stages={}, trees of at most 3 nodes over each stage's source schema, (stage, tree) cases={}",
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

const C11_PROTOCOL: &str = "";

fn ref_path_stage(index: usize, stage: &C11Stage) -> PathStage<usize> {
    PathStage::new_with_protocols(
        index,
        &stage.source,
        C11_PROTOCOL,
        &stage.target,
        C11_PROTOCOL,
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
            RefLensPath::then(
                &RefLensPath::identity_with_protocol(&first.source, C11_PROTOCOL),
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
                &RefLensPath::identity_with_protocol(&last.target, C11_PROTOCOL),
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
        "C11 paths final SSB: edits={edits}, admitted-positive families={:?}, refused-positive families={:?}, designated-negative families={:?}",
        edit_families.admitted_positive.named(),
        edit_families.refused_positive.named(),
        edit_families.designated_negative.named()
    );
    assert_eq!(compared, u64::try_from(units.len()).unwrap());
}

/// The anchor no C11 schema declares.
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
fn fused_graph_groups_agree_with_the_reference_on_undeclared_sources() {
    let stages = c11_stages();
    let composable_words = composable_c11_words(&stages);
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
    let sources = c11_stage_sources(&stages);
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
    let states: usize = fused
        .iter()
        .map(|&index| sources[composable_words[index / 3][0]].len())
        .sum();
    let bound = format!(
        "C11 undeclared sources: fused composites={}, source states={states}, undeclared-anchor sources={anchors}, undeclared-label sources={labels}, cases={}",
        fused.len(),
        units.len()
    );
    assert_eq!(anchors, states);
    assert_eq!(anchors + labels, units.len());
    assert_eq!(fused.len(), 384);
    assert_eq!(states, 761_742);
    assert_eq!(labels, 761_358);
    assert_eq!(units.len(), 1_523_100);
    println!("{bound}");
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
    stages: &[C11Stage],
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

fn assert_fan_parent_refusal_remains_in_strict_carrier(stages: &[C11Stage]) {
    let identity = c11_stage_index(stages, "Id_h");
    let pair = Pair::from_stage(&stages[identity]);
    let labels = panproto_reference::enumerate::lenses::c11_fan_labels();
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

fn assert_derived_refusal_remains_in_strict_carrier(stages: &[C11Stage]) {
    let transform = c11_stage_index(stages, "T");
    let pair = Pair::from_stage(&stages[transform]);
    let source = RefGraph {
        nodes: BTreeMap::from([(0, RefNode::new(0, "a"))]),
        edges: BTreeMap::new(),
    };
    pair.with_reference(|lens| {
        let lens = lens.expect("T compiles");
        let (view, complement) = RefGraphShape::get(lens, &source, &()).expect("T gets");
        let basis = RefGraphShape::single_step_basis(lens, &source, &view, &complement, &());
        let case = basis
            .iter()
            .find(|case| case.mutation == RefMutation::FieldEdit)
            .expect("the T basis retains the derived-field edit");
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
                "T derived-field edit".to_owned()
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
    let stages = c11_stages();
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
    let stages = c11_stages();
    let projection = c11_stage_index(&stages, "P");
    assert_eq!(
        projection, 17,
        "P remains path stage 17 in the C11 alphabet"
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
/// the intentional C11-field extension. Each candidate carries the observed
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
            .is_some_and(|columns| columns.contains_key(C11_FIELD))
        {
            continue;
        }
        let mut candidate = view.clone();
        let mut row = cells.clone();
        if row.contains_key(C11_FIELD) {
            row.remove(C11_FIELD);
        } else {
            row.insert(C11_FIELD.to_owned(), C11_VALUE);
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
        if row.contains_key(C11_FIELD) {
            row.remove(C11_FIELD);
        } else {
            row.insert(C11_FIELD.to_owned(), C11_VALUE);
        }
        candidate.rows.insert(*id, (table.clone(), row));
        assert!(
            !designated_negatives
                .iter()
                .any(|(_, negative)| negative == &candidate),
            "manual C11-field positive collides with a designated negative for path {word:?}"
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
        "L16 tree fusion: bases={}, inside={}, outside={}, final edits={}, admitted-positive families={:?}, refused-positive families={:?}, designated-negative families={:?}",
        units.len(),
        units.len() - outside,
        outside,
        edits,
        edit_families.admitted_positive.named(),
        edit_families.refused_positive.named(),
        edit_families.designated_negative.named()
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
        edit_families.admitted_positive.families,
        [0, 0, 19_258, 28_445, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        "the admitted table-path mutation-family census is part of the certificate"
    );
    assert_eq!(
        edit_families.refused_positive.families,
        [28_445, 0, 1_035, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        "the refused table-path mutation-family census is part of the certificate"
    );
    assert_eq!(
        edit_families.designated_negative.families,
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 168, 0, 573, 271, 0],
        "the designated-negative table-path mutation-family census is part of the certificate"
    );
    println!(
        "L16 table fusion: bases={}, final edits={}, admitted-positive families={:?}, refused-positive families={:?}, designated-negative families={:?}",
        units.len(),
        edits,
        edit_families.admitted_positive.named(),
        edit_families.refused_positive.named(),
        edit_families.designated_negative.named()
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
        let left = RefLensPath::then(
            &RefLensPath::identity_with_protocol(&stage.source, C11_PROTOCOL),
            &reference,
        );
        let right = RefLensPath::then(
            &reference,
            &RefLensPath::identity_with_protocol(&stage.target, C11_PROTOCOL),
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
        "C11 path category: stages={}, identity schemas={}, unit checks={identities}, triples={triples}",
        stages.len(),
        schemas.len()
    );
    assert_eq!(triples, c11_path_count_certificate().words_by_length[2]);
}

struct PathPropertyCarrier {
    stages: Vec<C11Stage>,
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
        let stages = c11_stages();
        let words = composable_c11_words(&stages);
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
    if carrier.stages[word[0]].source != c11_fan_schema() && !tree_sources.is_empty() {
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

fn assert_boundary_mismatched_associativity(stages: &[C11Stage], triple: [usize; 3]) {
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

fn reference_table_fusion_premises(
    first: &Lens,
    second: &Lens,
    candidate: &Lens,
    protocols: [&Protocol; 3],
) -> panproto_reference::path::TableFusionPremises {
    let first_source = reference_rel_schema(&first.src_schema, protocols[0]);
    let first_target = reference_rel_schema(&first.tgt_schema, protocols[1]);
    let second_source = reference_rel_schema(&second.src_schema, protocols[1]);
    let second_target = reference_rel_schema(&second.tgt_schema, protocols[2]);
    let candidate_source = reference_rel_schema(&candidate.src_schema, protocols[0]);
    let candidate_target = reference_rel_schema(&candidate.tgt_schema, protocols[2]);

    let first_migration = ref_migration(&first.compiled);
    let second_migration = ref_migration(&second.compiled);
    let candidate_migration = ref_migration(&candidate.compiled);
    let first_plan =
        first_source
            .as_ref()
            .zip(first_target.as_ref())
            .and_then(|(source, target)| {
                panproto_reference::table::compile_for_tables(
                    &ref_schema(&first.src_schema),
                    &ref_schema(&first.tgt_schema),
                    source,
                    target,
                    &first_migration,
                )
                .ok()
            });
    let second_plan =
        second_source
            .as_ref()
            .zip(second_target.as_ref())
            .and_then(|(source, target)| {
                panproto_reference::table::compile_for_tables(
                    &ref_schema(&second.src_schema),
                    &ref_schema(&second.tgt_schema),
                    source,
                    target,
                    &second_migration,
                )
                .ok()
            });
    let candidate_plan = candidate_source
        .as_ref()
        .zip(candidate_target.as_ref())
        .and_then(|(source, target)| {
            panproto_reference::table::compile_for_tables(
                &ref_schema(&candidate.src_schema),
                &ref_schema(&candidate.tgt_schema),
                source,
                target,
                &candidate_migration,
            )
            .ok()
        });
    let plans_compile = first_plan.is_some() && second_plan.is_some() && candidate_plan.is_some();

    let first_is_unit_bijection = first_plan.as_ref().is_some_and(|plan| {
        let source_schema = ref_schema(&first.src_schema);
        let target_schema = ref_schema(&first.tgt_schema);
        let source_boundary = first_source
            .as_ref()
            .expect("a compiled plan has a source boundary");
        let target_boundary = first_target
            .as_ref()
            .expect("a compiled plan has a target boundary");
        RefTableLens::new(
            &source_schema,
            &target_schema,
            panproto_reference::table::TableBoundaries::new(source_boundary, target_boundary),
            &first_migration,
            plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .ok()
        .is_some_and(|lens| panproto_reference::table::obstruction_to_isomorphism(&lens).is_none())
    });
    let middle_boundaries_equal = first_plan
        .as_ref()
        .zip(second_plan.as_ref())
        .is_some_and(|(first, second)| first.target_boundary() == second.source_boundary());
    let second_source_validation_free = second_plan.as_ref().is_some_and(|plan| {
        reference_required_value_free(&second.src_schema, protocols[1], plan.source_boundary())
    });
    let second_has_value_layer = !second.compiled.conditional_survival.is_empty()
        || second
            .compiled
            .field_transforms
            .values()
            .any(|transforms| !transforms.is_empty())
        || second
            .compiled
            .op_term_assignments
            .values()
            .any(|assignments| !assignments.is_empty());
    let typed_placeholders_agree = !second_has_value_layer
        || (protocols[0] == protocols[1]
            && first_plan
                .as_ref()
                .is_some_and(|plan| plan.source_boundary() == plan.target_boundary()));
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
            .all(|transforms| transforms.is_empty())
            && first
                .compiled
                .op_term_assignments
                .values()
                .all(|assignments| assignments.is_empty()))
        .into(),
        plans_compile: plans_compile.into(),
        first_is_unit_bijection: first_is_unit_bijection.into(),
        middle_boundaries_equal: middle_boundaries_equal.into(),
        second_source_validation_free: second_source_validation_free.into(),
        typed_placeholders_agree: typed_placeholders_agree.into(),
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
    first: Lens,
    second: Lens,
    protocols: [Protocol; 3],
    source: &FInstance,
) {
    assert_table_fusion_decision(
        &first,
        &second,
        [&protocols[0], &protocols[1], &protocols[2]],
        Ok(()),
    );
    let path = LensPath::single(duplicate_lens(&first))
        .then(&LensPath::single(duplicate_lens(&second)))
        .expect("the admitted fixture has a shared middle schema");
    let (fused_view, fused_complement) = path
        .get_table(source, &protocols)
        .expect("the admitted fused get succeeds");
    let (middle, first_complement) = table_lens::get(
        &first,
        source,
        BoundaryProtocols {
            src: &protocols[0],
            tgt: &protocols[1],
        },
    )
    .expect("the first atomic get succeeds");
    let (atomic_view, second_complement) = table_lens::get(
        &second,
        &middle,
        BoundaryProtocols {
            src: &protocols[1],
            tgt: &protocols[2],
        },
    )
    .expect("the second atomic get succeeds");
    assert_eq!(fused_view, atomic_view, "fused and atomic get");

    let fused_source = path
        .put_table(&fused_view, &fused_complement, &protocols)
        .expect("the admitted fused put succeeds");
    let atomic_middle = table_lens::put(
        &second,
        &atomic_view,
        &second_complement,
        BoundaryProtocols {
            src: &protocols[1],
            tgt: &protocols[2],
        },
    )
    .expect("the second atomic put succeeds");
    let atomic_source = table_lens::put(
        &first,
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
        admitted_first,
        admitted_second,
        [protocol.clone(), protocol.clone(), protocol],
        &instance,
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
    assert_table_fusion_decision(
        &first,
        &second,
        [&source_protocol, &middle_protocol, &middle_protocol],
        Err(panproto_reference::path::TableFusionObstruction::TypedPlaceholderMismatch),
    );
    let mut instance = FInstance::new();
    instance
        .insert_row(
            "row",
            RowId(0),
            HashMap::from([("f".to_owned(), Value::Bool(true))]),
        )
        .expect("the admitted placeholder control row is distinct");
    assert_admitted_table_fusion_behavior(
        first,
        second,
        [
            source_protocol.clone(),
            source_protocol.clone(),
            source_protocol,
        ],
        &instance,
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
        admitted_first,
        admitted_second,
        [protocol.clone(), protocol.clone(), protocol],
        &instance,
    );
}

fn table_fusion_target_only_carrier_witness() {
    let assert_static_isomorphism_boundary =
        |protocol: &Protocol, source: &Schema, target: &Schema, coordinate: &str| {
            let first = certificate_mapped_lens(source, target, std::iter::empty());
            let boundaries = BoundaryProtocols {
                src: protocol,
                tgt: protocol,
            };
            assert!(!panproto_lens::table_lens::is_isomorphism(
                &first, boundaries
            ));
            let obstruction =
                panproto_lens::table_lens::obstruction_to_isomorphism(&first, boundaries)
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
        };

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
    assert_static_isomorphism_boundary(
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
    assert_static_isomorphism_boundary(
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
    assert_static_isomorphism_boundary(
        &column_protocol,
        &column_source,
        &column_target,
        "target column `target_only`",
    );

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
        certificate_identity_lens(&protocol_drift_schema),
        certificate_identity_lens(&protocol_drift_schema),
        [
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
            let stages = c11_stages();
            let predicate = c11_stage_index(&stages, "Q");
            let identity = stages
                .iter()
                .position(|stage| {
                    stage.name.starts_with("Id")
                        && stage.source == stages[predicate].source
                        && stage.target == stages[predicate].source
                })
                .expect("the C11 alphabet contains Id_a");
            let word = [identity, predicate];
            let path = production_word(&stages, &word).expect("Id_a then Q composes");
            let groups = path.tree_normal_form();
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].stages, 0..2);

            let pairs: Vec<Pair> = stages.iter().map(Pair::from_stage).collect();
            let sources = c11_tree_states(&stages[identity].source);
            assert!(!sources.is_empty());
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
    TableFusionBoundaryWitness::TypedPlaceholder,
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
        TableFusionBoundaryWitness::TypedPlaceholder => {
            table_fusion_placeholder_witness();
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
    stages: Vec<C11Stage>,
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
        let stages = c11_stages();
        let words = composable_c11_words(&stages);
        let graph_source_counts = stages
            .iter()
            .map(|stage| {
                if stage.source == c11_fan_schema() {
                    c11_fan_state_count()
                } else {
                    c11_graph_state_count(&stage.source)
                }
            })
            .collect();
        let tree_source_counts = stages
            .iter()
            .map(|stage| {
                if stage.source == c11_fan_schema() {
                    0
                } else {
                    c11_tree_state_count(&stage.source)
                }
            })
            .collect();
        let table_source_counts = stages
            .iter()
            .map(|stage| {
                if stage.source == c11_fan_schema() {
                    0
                } else {
                    c11_table_state_count(&stage.source)
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
            return Some(if ordinal % 2 == 0 {
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
    the_c11_alphabet_witnesses_every_tree_fusion_condition();
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
    assert!(carrier.trees[0].is_empty());
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

fn reference_tree_square(pair: &Pair, case: &TreeSquareCase) -> SquareOutcome {
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

fn graph_square_units(_stages: &[C11Stage]) -> Vec<(usize, RefTree)> {
    let mut units = Vec::new();
    for_each_c11_tree_square_case(|case| units.push((case.stage, case.source)));
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
    assert_eq!(
        edit_families.admitted_positive.families,
        [
            6_366, 0, 6_504, 3_232, 56, 6_504, 3_232, 1_728, 0, 1_152, 0, 0, 0, 0, 0, 0, 0,
        ],
        "the admitted graph/tree-square mutation-family census is part of the certificate"
    );
    assert_eq!(
        edit_families.refused_positive.families,
        [0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        "the refused graph/tree-square mutation-family census is part of the certificate"
    );
    assert_eq!(
        edit_families.designated_negative.families,
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 48, 0, 48, 0, 0],
        "the designated-negative graph/tree-square mutation-family census is part of the certificate"
    );
    println!(
        "C11 graph/tree square: bases={bases}, edits={edits}, admitted-positive families={:?}, refused-positive families={:?}, designated-negative families={:?}, total={}",
        edit_families.admitted_positive.named(),
        edit_families.refused_positive.named(),
        edit_families.designated_negative.named(),
        bases + edits
    );
    assert_eq!(edits, certificate.edited_views);
    assert_eq!(bases + edits, certificate.observations);
    assert_eq!(compared, u64::try_from(certificate.observations).unwrap());
}

fn graph_square_bound() -> String {
    let bound = "C11 graph/tree square: every one-step tree SSB view".to_owned();
    println!("{bound}");
    bound
}

fn assert_graph_square_agrees_with_the_reference_exhaustively(units: &[(usize, RefTree)]) {
    let stages = c11_stages();
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
    let stages = c11_stages();
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
        c11_tree_square_count_certificate().observations + validations,
        graph_tree_route_law_observation_count_certificate()
    );
    assert_graph_square_agrees_with_the_reference_exhaustively(&squares);
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
            "L.19 refusals: candidates=2, false-precondition clauses=2, transform calls before false-precondition refusal=0, plan refusals=1, expansion intermediates=65536, expansion hops=65537"
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
        "C11 protolens instances: protolenses={protolenses}, schemas={schemas}, elementary cases={elementary_cases}, named application refusals={refusal_witnesses}, total={}",
        elementary_cases + refusal_witnesses,
    );
    println!("{bound}");
    bound
}

fn assert_malformed_fan_protolens_application_refusal() {
    use panproto_gat::{TheoryConstraint, TheoryEndofunctor, TheoryTransform};
    use panproto_lens::{ComplementConstructor, Protolens};

    let schema = c11_fan_schema();
    let mut hyper = RefNode::new(0, "h");
    hyper.shape = RefNodeShape::HyperEdge;
    let labels = panproto_reference::enumerate::lenses::c11_fan_labels();
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
    let states: Vec<Vec<RefGraph>> = schemas.iter().map(c11_graph_states).collect();
    let (units, refusal_witnesses) = collect_protolens_instance_units();
    let certificate = protolens_instance_count_certificate();
    assert_eq!(certificate.protolenses, alphabet.len());
    assert_eq!(certificate.schemas, schemas.len());
    assert_eq!(
        units.len(),
        alphabet.len() * schemas.iter().map(c11_graph_state_count).sum::<usize>()
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
        Some(&retained),
        "the fixture must exercise the endpoint-resolver fallback",
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
    root: S::Instance,
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
        panproto_lens::laws::check_laws_report::<CommonProductionShape<S>>(lens, &root, &context);
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
    root: S::Instance,
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
        lens, &root, &context,
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
    root: S::Instance,
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
            prod_graph(source),
            (),
            production_families,
        ))
    } else {
        Some(pair.with_reference(|lens| {
            let lens = lens.expect("the common-basis graph checker requires a compiled lens");
            reference_common_law_outcome::<RefGraphShape, _>(
                lens,
                source.clone(),
                (),
                reference_families,
            )
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
            prod_tree(source),
            (),
            production_families,
        ))
    } else {
        Some(pair.with_reference(|lens| {
            let lens = lens.expect("the common-basis tree checker requires a compiled lens");
            reference_common_law_outcome::<RefTreeShape, _>(
                lens,
                source.clone(),
                (),
                reference_families,
            )
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
            prod_table(source),
            panproto_lens::TableContext {
                src: pair.protocol.clone(),
                tgt: pair.protocol.clone(),
            },
            production_families,
        )
    } else {
        pair.with_reference(|lens, _| {
            reference_common_law_outcome::<RefTableShape, _>(
                lens,
                source.clone(),
                (),
                reference_families,
            )
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

/// The law-report case families of the L.8 scope.
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
    fn len(&self) -> Option<usize> {
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
        _ => panic!("prepared L.8 case and common-basis carrier disagree"),
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
    let stages = c11_stages();
    let stage = c11_stage_index(&stages, "Id_h");
    assert_eq!(stage, 16, "Id_h remains the seventeenth C11 stage");
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
    let stages = c11_stages();
    let stage = c11_stage_index(&stages, "C");
    assert_eq!(stage, 21, "C remains the twenty-second C11 stage");
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
                    source.clone(),
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
    let stages = c11_stages();
    let stage = c11_stage_index(&stages, "C");
    assert_eq!(stage, 21, "C remains the twenty-second C11 stage");
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
            reference_common_law_report::<RefGraphShape, _>(lens, source.clone(), (), families).0
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

        for (candidate, verdict) in candidates.iter().zip(&production_admissions.verdicts) {
            if !verdict.is_admitted() {
                continue;
            }
            let production = production_graph_put_get_trace(&pair, &source, candidate);
            let reference = reference_graph_put_get_trace(&pair, &source, candidate);
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
            assert!(!production.expected_index.is_empty());
            assert!(!reference.expected_index.is_empty());
            assert!(!production.observed_index.is_empty());
            assert!(!reference.observed_index.is_empty());
            assert_eq!(production.divergence.is_none(), production.equivalent);
            assert_eq!(reference.divergence.is_none(), reference.equivalent);
        }
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
        "L.8 law reports: partial lens/source cases={partial}, table fixtures={tables}, C11 graph and tree (stage, state) cases={}, total={}",
        cases.len() - partial - tables,
        cases.len()
    );
    println!("{bound}");
    let tally = AdmissionTally::default();
    let common_tally = CommonBasisTally::default();
    let compared = assert_agree(agree_in_parallel(
        &bound,
        &cases,
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
    println!(
        "L.8 admission census (graph, tree and table families): candidates={}, admitted positive reference-basis candidates={}",
        tally.candidates.load(std::sync::atomic::Ordering::Relaxed),
        tally.admitted.load(std::sync::atomic::Ordering::Relaxed),
    );
    let common_counts = common_tally.snapshot();
    println!("L.8 common five-gate basis by mutation family: {common_counts:?}");
    assert!(
        common_counts.iter().all(|count| *count > 0),
        "every mutation family must occur in the independently formed common basis"
    );
    assert_eq!(compared, 200_606);
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

type GraphDomainWitness = (GraphPutDomainFamily, usize, RefGraph, RefGraph);

fn c11_graph_domain_witnesses() -> &'static [GraphDomainWitness] {
    static WITNESSES: std::sync::OnceLock<Vec<GraphDomainWitness>> = std::sync::OnceLock::new();
    WITNESSES.get_or_init(|| {
        let stages = c11_stages();
        let pairs = stages.iter().map(Pair::from_stage).collect::<Vec<_>>();
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
        let wanted = GraphPutDomainFamily::ALL
            .into_iter()
            .fold(0_u16, |bits, family| {
                bits | graph_put_domain_family_bit(family.name())
            })
            & !focused;
        let mut found = 0_u16;
        let mut witnesses = Vec::new();
        'stages: for (stage, descriptor) in stages.iter().enumerate() {
            for source in c11_sources(descriptor) {
                let Ok((view, complement)) = reference_get(&pairs[stage], &source) else {
                    continue;
                };
                for edited in c11_graph_edits(&view, &descriptor.target, &complement)
                    .into_iter()
                    .map(|case| case.view)
                {
                    let Some(PutOutcome {
                        domain: Err(reasons),
                        ..
                    }) = reference_strict_put(&pairs[stage], &source, &edited)
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
                            witnesses.push((family, stage, source.clone(), edited.clone()));
                        }
                    }
                    if found == wanted {
                        break 'stages;
                    }
                }
            }
        }
        assert_eq!(
            found, wanted,
            "the C11 carrier lacks a reusable domain witness"
        );
        witnesses
    })
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
        "C11 graph law report",
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
        "C11 tree law report",
        format!("{production_laws:?}"),
        format!("{reference_laws:?}"),
    )?;
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
        if let Err(message) = gf5_property_case(carrier) {
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
                prop_assert!(false, "L.8 family {family}: {message}");
            }
        }
        let family = [0_u8, 5, 6, 7, 8][usize::from(additional_family)];
        if let Err(message) = property_case(family, index, edit) {
            prop_assert!(false, "additional family {family}: {message}");
        }
    }
}
