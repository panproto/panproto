//! Complement type system for protolenses.
//!
//! Given a protolens η and schema S, compute the complement type
//! `ComplementType(η, S)`. This is a dependent type: the complement
//! varies with the schema the protolens is instantiated at.

use std::collections::BTreeSet;

use panproto_gat::Name;
use panproto_inst::value::Value;
use panproto_inst::{RelSchema, RelSchemaError};
use panproto_schema::{Protocol, Schema};
use serde::{Deserialize, Serialize};

use crate::LensError;
use crate::protolens::{ComplementConstructor, Protolens, ProtolensChain};

/// Static specification of what data a complement will contain.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplementSpec {
    /// Overall classification.
    pub kind: ComplementKind,
    /// What the user must supply for forward direction.
    pub forward_defaults: Vec<DefaultRequirement>,
    /// What data is captured in the complement for backward direction.
    pub captured_data: Vec<CapturedField>,
    /// Human-readable summary.
    pub summary: String,
}

/// Classification of a complement's role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComplementKind {
    /// No complement needed (isomorphism).
    Empty,
    /// Data captured in complement (lossy forward).
    DataCaptured,
    /// User must provide defaults (lossy backward).
    DefaultsRequired,
    /// Both.
    Mixed,
}

/// A default value that must be supplied for the forward direction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultRequirement {
    /// Name of the element needing a default.
    pub element_name: Name,
    /// What kind: "sort" or "op" or "equation".
    pub element_kind: String,
    /// Human-readable description.
    pub description: String,
    /// Suggested default if known.
    pub suggested_default: Option<Value>,
}

/// A field captured in the complement during the forward direction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedField {
    /// Name of the captured element.
    pub element_name: Name,
    /// What kind: "sort" or "op".
    pub element_kind: String,
    /// Human-readable description.
    pub description: String,
}

/// Graph-shaped residue retained by a complement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphResidueKind {
    /// Data attached directly to a graph node.
    Node,
    /// A node or field value.
    Value,
    /// An ordinary binary graph edge.
    Edge,
    /// A hyper-edge represented by its reified fan node and incident edges.
    ReifiedHyperedge,
}

/// Table-shaped residue retained by a complement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableResidueKind {
    /// A complete table row.
    Row,
    /// One cell value.
    Cell,
    /// A pair of row ids instantiating a foreign key.
    ForeignKeyPair,
}

/// Shape-neutral complement information plus its graph carrier classes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphComplementSpec {
    /// Existing shape-neutral complement specification.
    pub complement: ComplementSpec,
    /// Distinct graph carrier classes that the complement may retain.
    pub residue: Vec<GraphResidueKind>,
}

/// Shape-neutral complement information plus its table carrier classes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableComplementSpec {
    /// Existing shape-neutral complement specification.
    pub complement: ComplementSpec,
    /// Distinct table carrier classes that the complement may retain.
    pub residue: Vec<TableResidueKind>,
}

/// Compute a complement specification classified by graph carrier.
#[must_use]
pub fn graph_complement_spec_at(protolens: &Protolens, schema: &Schema) -> GraphComplementSpec {
    let mut residue = BTreeSet::new();
    graph_residue(&protolens.complement_constructor, schema, &mut residue);
    GraphComplementSpec {
        complement: complement_spec_at(protolens, schema),
        residue: residue.into_iter().collect(),
    }
}

/// Compute a complement specification classified by table carrier.
///
/// # Errors
///
/// Returns [`RelSchemaError`] when `schema` is not relation-representable
/// under `protocol`.
pub fn table_complement_spec_at(
    protolens: &Protolens,
    schema: &Schema,
    protocol: &Protocol,
) -> Result<TableComplementSpec, RelSchemaError> {
    let rel = RelSchema::new(schema, protocol)?;
    let mut residue = BTreeSet::new();
    table_residue(
        &protolens.complement_constructor,
        schema,
        &rel,
        &mut residue,
    );
    Ok(TableComplementSpec {
        complement: complement_spec_at(protolens, schema),
        residue: residue.into_iter().collect(),
    })
}

/// Compute a chain complement specification classified by graph carrier.
///
/// # Errors
///
/// Returns the first schema-transform or applicability error in chain order.
pub fn graph_chain_complement_spec(
    chain: &ProtolensChain,
    schema: &Schema,
    protocol: &Protocol,
) -> Result<GraphComplementSpec, LensError> {
    let mut residue = BTreeSet::new();
    let mut defaults = Vec::new();
    let mut captured = Vec::new();
    let mut current = schema.clone();
    for step in &chain.steps {
        step.check_applicability(&current).map_err(|reasons| {
            LensError::ProtolensError(format!(
                "protolens `{}` is not applicable while classifying complement: {}",
                step.name,
                reasons.join("; ")
            ))
        })?;
        graph_residue(&step.complement_constructor, &current, &mut residue);
        let spec = complement_spec_at(step, &current);
        defaults.extend(spec.forward_defaults);
        captured.extend(spec.captured_data);
        current = step.target_schema(&current, protocol)?;
    }
    Ok(GraphComplementSpec {
        complement: accumulated_spec(defaults, captured),
        residue: residue.into_iter().collect(),
    })
}

/// Compute a chain complement specification classified by table carrier.
///
/// # Errors
///
/// Returns the first relation-representation, schema-transform, or
/// applicability error in chain order.
pub fn table_chain_complement_spec(
    chain: &ProtolensChain,
    schema: &Schema,
    protocol: &Protocol,
) -> Result<TableComplementSpec, LensError> {
    let mut residue = BTreeSet::new();
    let mut defaults = Vec::new();
    let mut captured = Vec::new();
    let mut current = schema.clone();
    for step in &chain.steps {
        step.check_applicability(&current).map_err(|reasons| {
            LensError::ProtolensError(format!(
                "protolens `{}` is not applicable while classifying complement: {}",
                step.name,
                reasons.join("; ")
            ))
        })?;
        let rel = RelSchema::new(&current, protocol)
            .map_err(|error| LensError::Restrict(error.into()))?;
        table_residue(&step.complement_constructor, &current, &rel, &mut residue);
        let spec = complement_spec_at(step, &current);
        defaults.extend(spec.forward_defaults);
        captured.extend(spec.captured_data);
        current = step.target_schema(&current, protocol)?;
    }
    Ok(TableComplementSpec {
        complement: accumulated_spec(defaults, captured),
        residue: residue.into_iter().collect(),
    })
}

fn accumulated_spec(
    forward_defaults: Vec<DefaultRequirement>,
    captured_data: Vec<CapturedField>,
) -> ComplementSpec {
    let kind = classify(&forward_defaults, &captured_data);
    let summary = build_summary(&kind, &forward_defaults, &captured_data);
    ComplementSpec {
        kind,
        forward_defaults,
        captured_data,
        summary,
    }
}

/// Compute the complement spec for a single protolens at a specific schema.
#[must_use]
pub fn complement_spec_at(protolens: &Protolens, schema: &Schema) -> ComplementSpec {
    spec_from_constructor(&protolens.complement_constructor, schema)
}

/// Compute the complement spec for a protolens chain at a specific schema.
#[must_use]
pub fn chain_complement_spec(
    chain: &ProtolensChain,
    schema: &Schema,
    protocol: &Protocol,
) -> ComplementSpec {
    if chain.steps.is_empty() {
        return ComplementSpec {
            kind: ComplementKind::Empty,
            forward_defaults: vec![],
            captured_data: vec![],
            summary: "Identity transformation, no complement needed.".into(),
        };
    }

    let mut all_defaults = Vec::new();
    let mut all_captured = Vec::new();
    let mut current_schema = schema.clone();

    for step in &chain.steps {
        let spec = complement_spec_at(step, &current_schema);
        all_defaults.extend(spec.forward_defaults);
        all_captured.extend(spec.captured_data);
        if let Ok(next) = step.target_schema(&current_schema, protocol) {
            current_schema = next;
        }
    }

    let kind = classify(&all_defaults, &all_captured);
    let summary = build_summary(&kind, &all_defaults, &all_captured);

    ComplementSpec {
        kind,
        forward_defaults: all_defaults,
        captured_data: all_captured,
        summary,
    }
}

fn graph_residue(
    constructor: &ComplementConstructor,
    schema: &Schema,
    residue: &mut BTreeSet<GraphResidueKind>,
) {
    match constructor {
        ComplementConstructor::DroppedSortData { sort } => {
            residue.insert(GraphResidueKind::Node);
            let dropped_vertices = schema
                .vertices
                .iter()
                .filter_map(|(name, vertex)| (name == sort || vertex.kind == *sort).then_some(name))
                .collect::<BTreeSet<_>>();
            if schema.edges.keys().any(|edge| {
                dropped_vertices.contains(&edge.src) || dropped_vertices.contains(&edge.tgt)
            }) {
                residue.insert(GraphResidueKind::Edge);
            }
            if schema.hyper_edges.values().any(|edge| {
                edge.signature.values().any(|vertex| {
                    vertex == sort
                        || schema
                            .vertices
                            .get(vertex)
                            .is_some_and(|candidate| candidate.kind == *sort)
                })
            }) {
                residue.insert(GraphResidueKind::ReifiedHyperedge);
            }
        }
        ComplementConstructor::DroppedOpData { op } => {
            residue.insert(GraphResidueKind::Edge);
            if schema.hyper_edges.values().any(|edge| edge.kind == *op) {
                residue.insert(GraphResidueKind::ReifiedHyperedge);
            }
        }
        ComplementConstructor::DroppedEdge { .. } => {
            residue.insert(GraphResidueKind::Edge);
        }
        ComplementConstructor::NatTransKernel { .. } | ComplementConstructor::Enrichment { .. } => {
            residue.insert(GraphResidueKind::Value);
        }
        ComplementConstructor::CoercedSortData { class, .. } => {
            if class.needs_complement_storage() {
                residue.insert(GraphResidueKind::Value);
            }
        }
        ComplementConstructor::Composite(parts) => {
            for part in parts {
                graph_residue(part, schema, residue);
            }
        }
        ComplementConstructor::Scoped { inner, .. } => graph_residue(inner, schema, residue),
        ComplementConstructor::Empty | ComplementConstructor::AddedElement { .. } => {}
    }
}

fn table_residue(
    constructor: &ComplementConstructor,
    schema: &Schema,
    rel: &RelSchema,
    residue: &mut BTreeSet<TableResidueKind>,
) {
    match constructor {
        ComplementConstructor::DroppedSortData { sort } => {
            let mut dropped_tables = BTreeSet::new();
            for (name, vertex) in &schema.vertices {
                if name == sort || vertex.kind == *sort {
                    if rel.is_table(name) {
                        residue.insert(TableResidueKind::Row);
                        dropped_tables.insert(name);
                    } else {
                        residue.insert(TableResidueKind::Cell);
                    }
                }
            }
            if schema.edges.keys().any(|edge| {
                rel.is_relation(edge)
                    && (dropped_tables.contains(&edge.src) || dropped_tables.contains(&edge.tgt))
            }) {
                residue.insert(TableResidueKind::ForeignKeyPair);
            }
        }
        ComplementConstructor::DroppedOpData { op } => {
            for edge in schema.edges.keys().filter(|edge| edge.kind == *op) {
                residue.insert(if rel.is_relation(edge) {
                    TableResidueKind::ForeignKeyPair
                } else {
                    TableResidueKind::Cell
                });
            }
        }
        ComplementConstructor::DroppedEdge {
            src,
            tgt,
            edge_name,
            edge_kind,
        } => {
            for edge in schema.edges.keys().filter(|edge| {
                edge.src == *src
                    && edge.tgt == *tgt
                    && edge.name == *edge_name
                    && (edge_kind.is_empty() || edge.kind == *edge_kind)
            }) {
                residue.insert(if rel.is_relation(edge) {
                    TableResidueKind::ForeignKeyPair
                } else {
                    TableResidueKind::Cell
                });
            }
        }
        ComplementConstructor::NatTransKernel { .. } | ComplementConstructor::Enrichment { .. } => {
            residue.insert(TableResidueKind::Cell);
        }
        ComplementConstructor::CoercedSortData { class, .. } => {
            if class.needs_complement_storage() {
                residue.insert(TableResidueKind::Cell);
            }
        }
        ComplementConstructor::Composite(parts) => {
            for part in parts {
                table_residue(part, schema, rel, residue);
            }
        }
        ComplementConstructor::Scoped { inner, .. } => {
            table_residue(inner, schema, rel, residue);
        }
        ComplementConstructor::Empty | ComplementConstructor::AddedElement { .. } => {}
    }
}

fn spec_from_constructor(constructor: &ComplementConstructor, schema: &Schema) -> ComplementSpec {
    match constructor {
        ComplementConstructor::Empty => ComplementSpec {
            kind: ComplementKind::Empty,
            forward_defaults: vec![],
            captured_data: vec![],
            summary: "Lossless transformation.".into(),
        },
        ComplementConstructor::DroppedSortData { sort } => {
            // Count how many vertices of this sort exist in the schema.
            let count = schema
                .vertices
                .iter()
                .filter(|(name, vertex)| *name == sort || vertex.kind == *sort)
                .count();
            ComplementSpec {
                kind: ComplementKind::DataCaptured,
                forward_defaults: vec![],
                captured_data: vec![CapturedField {
                    element_name: sort.clone(),
                    element_kind: "sort".into(),
                    description: format!(
                        "Data for {count} vertices of kind '{sort}' will be captured in the complement."
                    ),
                }],
                summary: format!("Drops sort '{sort}': {count} vertices captured in complement."),
            }
        }
        ComplementConstructor::DroppedOpData { op } => {
            let count = schema.edges.keys().filter(|e| e.kind == *op).count();
            ComplementSpec {
                kind: ComplementKind::DataCaptured,
                forward_defaults: vec![],
                captured_data: vec![CapturedField {
                    element_name: op.clone(),
                    element_kind: "op".into(),
                    description: format!(
                        "{count} edges of kind '{op}' will be captured in the complement.",
                    ),
                }],
                summary: format!("Drops operation '{op}': {count} edges captured."),
            }
        }
        ComplementConstructor::DroppedEdge {
            src,
            tgt,
            edge_name,
            ..
        } => dropped_edge_spec(src, tgt, edge_name.as_ref()),
        ComplementConstructor::AddedElement {
            element_name,
            element_kind,
            default_value,
        } => added_element_spec(element_name, element_kind, default_value.as_ref()),
        ComplementConstructor::NatTransKernel { nat_trans_name } => ComplementSpec {
            kind: ComplementKind::DataCaptured,
            forward_defaults: vec![],
            captured_data: vec![CapturedField {
                element_name: nat_trans_name.clone(),
                element_kind: "nat_trans".into(),
                description: format!(
                    "Kernel of natural transformation '{nat_trans_name}' captured in complement.",
                ),
            }],
            summary: format!("Value conversion via '{nat_trans_name}': kernel captured."),
        },
        ComplementConstructor::CoercedSortData { sort, class } => {
            coerced_sort_spec(sort, *class, schema)
        }
        ComplementConstructor::Composite(parts) => {
            let mut all_defaults = Vec::new();
            let mut all_captured = Vec::new();
            for part in parts {
                let sub = spec_from_constructor(part, schema);
                all_defaults.extend(sub.forward_defaults);
                all_captured.extend(sub.captured_data);
            }
            let kind = classify(&all_defaults, &all_captured);
            let summary = build_summary(&kind, &all_defaults, &all_captured);
            ComplementSpec {
                kind,
                forward_defaults: all_defaults,
                captured_data: all_captured,
                summary,
            }
        }
        ComplementConstructor::Scoped { focus, inner } => {
            let inner_spec = spec_from_constructor(inner, schema);
            let kind = inner_spec.kind;
            ComplementSpec {
                kind,
                forward_defaults: inner_spec.forward_defaults,
                captured_data: inner_spec.captured_data,
                summary: format!("Scoped at '{focus}': {}", inner_spec.summary),
            }
        }
        ComplementConstructor::Enrichment { kind, enricher } => {
            enrichment_spec(*kind, enricher, schema)
        }
    }
}

/// Build a `ComplementSpec` for an enrichment-fibre complement.
fn enrichment_spec(
    kind: panproto_gat::EnrichmentKind,
    enricher: &std::sync::Arc<str>,
    schema: &Schema,
) -> ComplementSpec {
    let count = schema
        .constraints
        .values()
        .filter(|cs| cs.iter().any(|c| kind.is_member_sort(c.sort.as_ref())))
        .count();
    ComplementSpec {
        kind: ComplementKind::DataCaptured,
        forward_defaults: vec![],
        captured_data: vec![CapturedField {
            element_name: Name::from(format!("enrichment/{kind:?}/{enricher}")),
            element_kind: "enrichment".into(),
            description: format!(
                "{count} vertices carry constraints in the {kind:?} \
                 enrichment fibre; the registered driver \
                 '{enricher}' is responsible for materialising \
                 them in the put direction."
            ),
        }],
        summary: format!(
            "{kind:?} enrichment via driver '{enricher}'; \
             per-vertex fibre handled by the driver, not the \
             WInstance complement."
        ),
    }
}

/// Build a `ComplementSpec` for an added element requiring a default.
fn added_element_spec(
    element_name: &Name,
    element_kind: &str,
    default_value: Option<&panproto_inst::value::Value>,
) -> ComplementSpec {
    ComplementSpec {
        kind: ComplementKind::DefaultsRequired,
        forward_defaults: vec![DefaultRequirement {
            element_name: element_name.clone(),
            element_kind: element_kind.to_string(),
            description: format!("Default value needed for added {element_kind} '{element_name}'."),
            suggested_default: default_value.cloned(),
        }],
        captured_data: vec![],
        summary: format!("Adds {element_kind} '{element_name}': default required."),
    }
}

/// Build a `ComplementSpec` for a single dropped edge.
fn dropped_edge_spec(src: &Name, tgt: &Name, edge_name: Option<&Name>) -> ComplementSpec {
    let label = edge_name.map_or_else(|| "unnamed".to_string(), ToString::to_string);
    ComplementSpec {
        kind: ComplementKind::DataCaptured,
        forward_defaults: vec![],
        captured_data: vec![CapturedField {
            element_name: Name::from(format!("{src}--{label}-->{tgt}")),
            element_kind: "edge".into(),
            description: format!(
                "Single edge '{src} --({label})--> {tgt}' captured in complement."
            ),
        }],
        summary: format!("Drops edge '{src} --({label})--> {tgt}': captured in complement."),
    }
}

/// Build a `ComplementSpec` for a coerced sort.
fn coerced_sort_spec(
    sort: &Name,
    class: panproto_gat::CoercionClass,
    schema: &Schema,
) -> ComplementSpec {
    let count = schema.vertices.values().filter(|v| v.kind == *sort).count();
    let (kind, desc) = match class {
        panproto_gat::CoercionClass::Iso => (
            ComplementKind::Empty,
            format!("Isomorphic coercion on sort '{sort}' ({count} vertices)."),
        ),
        panproto_gat::CoercionClass::Retraction => (
            ComplementKind::DataCaptured,
            format!("Retraction coercion on sort '{sort}' ({count} vertices): residual captured."),
        ),
        panproto_gat::CoercionClass::Projection => (
            ComplementKind::Empty,
            format!(
                "Projection coercion on sort '{sort}' ({count} vertices): \
                 derived values re-computed by get, no complement storage needed."
            ),
        ),
        panproto_gat::CoercionClass::Opaque | _ => (
            ComplementKind::DataCaptured,
            format!(
                "Opaque coercion on sort '{sort}' ({count} vertices): original values captured."
            ),
        ),
    };
    ComplementSpec {
        kind,
        forward_defaults: vec![],
        // Only coercions that need complement storage (Retraction, Opaque)
        // produce captured data. Iso stores nothing (lossless). Projection
        // stores nothing (the derived value is re-computed by `get`
        // deterministically from the source fiber; the source data itself
        // survives via the tree structure's complement, not this coercion's).
        captured_data: if class.needs_complement_storage() {
            vec![CapturedField {
                element_name: sort.clone(),
                element_kind: "coerced_sort".into(),
                description: desc.clone(),
            }]
        } else {
            vec![]
        },
        summary: desc,
    }
}

/// Classify the complement kind from defaults and captured fields.
const fn classify(defaults: &[DefaultRequirement], captured: &[CapturedField]) -> ComplementKind {
    match (defaults.is_empty(), captured.is_empty()) {
        (true, true) => ComplementKind::Empty,
        (false, true) => ComplementKind::DefaultsRequired,
        (true, false) => ComplementKind::DataCaptured,
        (false, false) => ComplementKind::Mixed,
    }
}

fn build_summary(
    kind: &ComplementKind,
    defaults: &[DefaultRequirement],
    captured: &[CapturedField],
) -> String {
    match kind {
        ComplementKind::Empty => "Lossless transformation, no complement needed.".into(),
        ComplementKind::DefaultsRequired => format!(
            "{} default(s) required: {}",
            defaults.len(),
            defaults
                .iter()
                .map(|d| d.element_name.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ComplementKind::DataCaptured => format!(
            "{} field(s) captured in complement: {}",
            captured.len(),
            captured
                .iter()
                .map(|c| c.element_name.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ComplementKind::Mixed => format!(
            "{} default(s) required, {} field(s) captured in complement.",
            defaults.len(),
            captured.len()
        ),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::protolens::elementary;
    use crate::tests::three_node_schema;
    use panproto_inst::value::Value;
    use panproto_schema::{ScalarKind, SchemaBuilder};
    use std::collections::{BTreeMap, BTreeSet};

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into(), "array".into()],
            constraint_sorts: vec![],
            ..Default::default()
        }
    }

    /// Pin the JSON wire format. The TypeScript SDK declares this
    /// shape in `bindings/typescript/src/protolens.ts` (`ComplementSpec`,
    /// `ComplementKind`, `DefaultRequirement`, `CapturedField`): enum
    /// variants in `snake_case` (`"empty" | "data_captured" |
    /// "defaults_required" | "mixed"`), struct fields in `camelCase`
    /// (`forwardDefaults`, `capturedData`, `elementName`,
    /// `elementKind`, `suggestedDefault`). Without the
    /// `#[serde(rename_all = ...)]` attributes Rust would emit
    /// `"DataCaptured"` enum variants and `element_name` fields, and
    /// every TS consumer reaching for those fields through the typed
    /// API would see `undefined`.
    #[test]
    fn complement_spec_wire_format_matches_ts_sdk() {
        use serde_json::{Value as JsonValue, json};
        let spec = ComplementSpec {
            kind: ComplementKind::DataCaptured,
            forward_defaults: vec![DefaultRequirement {
                element_name: Name::from("field_a"),
                element_kind: "sort".to_owned(),
                description: "needs a default".to_owned(),
                suggested_default: None,
            }],
            captured_data: vec![CapturedField {
                element_name: Name::from("field_b"),
                element_kind: "op".to_owned(),
                description: "captured".to_owned(),
            }],
            summary: "mixed".to_owned(),
        };
        let value: JsonValue = serde_json::to_value(&spec).unwrap();
        assert_eq!(
            value,
            json!({
                "kind": "data_captured",
                "forwardDefaults": [{
                    "elementName": "field_a",
                    "elementKind": "sort",
                    "description": "needs a default",
                    "suggestedDefault": null,
                }],
                "capturedData": [{
                    "elementName": "field_b",
                    "elementKind": "op",
                    "description": "captured",
                }],
                "summary": "mixed",
            }),
            "ComplementSpec wire format must match the TS SDK"
        );
        // All four ComplementKind variants must wire as snake_case
        // to match the TS union type.
        for (variant, wire) in [
            (ComplementKind::Empty, "empty"),
            (ComplementKind::DataCaptured, "data_captured"),
            (ComplementKind::DefaultsRequired, "defaults_required"),
            (ComplementKind::Mixed, "mixed"),
        ] {
            assert_eq!(
                serde_json::to_value(&variant).unwrap(),
                JsonValue::String(wire.to_owned()),
                "ComplementKind::{variant:?} must serialize as {wire:?}"
            );
        }
    }

    #[test]
    fn rename_sort_has_empty_complement() {
        let schema = three_node_schema();
        let p = elementary::rename_sort("string", "text");
        let spec = complement_spec_at(&p, &schema);
        assert_eq!(spec.kind, ComplementKind::Empty);
        assert!(spec.forward_defaults.is_empty());
        assert!(spec.captured_data.is_empty());
    }

    #[test]
    fn drop_sort_captures_data() {
        let schema = three_node_schema();
        let p = elementary::drop_sort("string");
        let spec = complement_spec_at(&p, &schema);
        assert_eq!(spec.kind, ComplementKind::DataCaptured);
        assert_eq!(spec.captured_data.len(), 1);
        assert_eq!(&*spec.captured_data[0].element_name, "string");
    }

    #[test]
    fn add_sort_has_defaults_required_complement() {
        let schema = three_node_schema();
        let p = elementary::add_sort("tags", "array", Value::Null);
        let spec = complement_spec_at(&p, &schema);
        assert_eq!(spec.kind, ComplementKind::DefaultsRequired);
        assert_eq!(spec.forward_defaults.len(), 1);
        assert_eq!(&*spec.forward_defaults[0].element_name, "tags");
    }

    #[test]
    fn drop_op_captures_data() {
        let schema = three_node_schema();
        let p = elementary::drop_op("prop");
        let spec = complement_spec_at(&p, &schema);
        assert_eq!(spec.kind, ComplementKind::DataCaptured);
        assert_eq!(spec.captured_data.len(), 1);
        assert_eq!(&*spec.captured_data[0].element_name, "prop");
    }

    #[test]
    fn empty_chain_is_empty() {
        let schema = three_node_schema();
        let protocol = test_protocol();
        let chain = crate::protolens::ProtolensChain::new(vec![]);
        let spec = chain_complement_spec(&chain, &schema, &protocol);
        assert_eq!(spec.kind, ComplementKind::Empty);
    }

    #[test]
    fn chain_with_drop_has_data_captured() {
        let schema = three_node_schema();
        let protocol = test_protocol();
        let chain = crate::protolens::ProtolensChain::new(vec![elementary::drop_sort("string")]);
        let spec = chain_complement_spec(&chain, &schema, &protocol);
        assert_eq!(spec.kind, ComplementKind::DataCaptured);
    }

    #[test]
    fn chain_mixed() {
        let schema = three_node_schema();
        let protocol = test_protocol();
        // This chain has both adds (defaults required)
        // and drops (data captured).
        let chain = crate::protolens::ProtolensChain::new(vec![
            elementary::add_sort("tags", "array", Value::Null),
            elementary::drop_sort("string"),
        ]);
        let spec = chain_complement_spec(&chain, &schema, &protocol);
        assert_eq!(spec.kind, ComplementKind::Mixed);
    }

    #[test]
    fn summary_describes_complement() {
        let schema = three_node_schema();
        let p = elementary::drop_sort("string");
        let spec = complement_spec_at(&p, &schema);
        assert!(spec.summary.contains("string"));
    }

    #[test]
    fn graph_residue_names_edges_and_reified_hyperedges() {
        let protocol = test_protocol();
        let schema = SchemaBuilder::new(&protocol)
            .vertex("a", "object", None::<&str>)
            .unwrap()
            .vertex("b", "object", None::<&str>)
            .unwrap()
            .edge("a", "b", "rel", Some("child"))
            .unwrap()
            .hyper_edge(
                "fan",
                "rel",
                std::collections::HashMap::from([
                    ("parent".to_owned(), "a".to_owned()),
                    ("child".to_owned(), "b".to_owned()),
                ]),
                "parent",
            )
            .unwrap()
            .build()
            .unwrap();
        let spec = graph_complement_spec_at(&elementary::drop_op("rel"), &schema);
        assert_eq!(
            spec.residue,
            vec![GraphResidueKind::Edge, GraphResidueKind::ReifiedHyperedge]
        );

        let signature_vertex =
            graph_complement_spec_at(&elementary::drop_schema_vertex("b"), &schema);
        assert_eq!(
            signature_vertex.residue,
            vec![
                GraphResidueKind::Node,
                GraphResidueKind::Edge,
                GraphResidueKind::ReifiedHyperedge
            ]
        );
    }

    #[test]
    fn table_residue_names_rows_cells_and_foreign_key_pairs() {
        let mut protocol = test_protocol();
        protocol.value_kinds = BTreeMap::from([("string".into(), ScalarKind::Str)]);
        protocol.non_value_kinds = BTreeSet::from(["object".into()]);
        let schema = SchemaBuilder::new(&protocol)
            .vertex("post", "object", None::<&str>)
            .unwrap()
            .vertex("user", "object", None::<&str>)
            .unwrap()
            .vertex("post.title", "string", None::<&str>)
            .unwrap()
            .edge("post", "post.title", "prop", Some("title"))
            .unwrap()
            .edge("post", "user", "fk", Some("author"))
            .unwrap()
            .build()
            .unwrap();

        let row =
            table_complement_spec_at(&elementary::drop_sort("object"), &schema, &protocol).unwrap();
        assert_eq!(
            row.residue,
            vec![TableResidueKind::Row, TableResidueKind::ForeignKeyPair]
        );
        let cell =
            table_complement_spec_at(&elementary::drop_op("prop"), &schema, &protocol).unwrap();
        assert_eq!(cell.residue, vec![TableResidueKind::Cell]);
        let pair =
            table_complement_spec_at(&elementary::drop_op("fk"), &schema, &protocol).unwrap();
        assert_eq!(pair.residue, vec![TableResidueKind::ForeignKeyPair]);

        let concrete_row =
            table_complement_spec_at(&elementary::drop_schema_vertex("post"), &schema, &protocol)
                .unwrap();
        assert_eq!(
            concrete_row.residue,
            vec![TableResidueKind::Row, TableResidueKind::ForeignKeyPair]
        );
        let concrete_cell = table_complement_spec_at(
            &elementary::drop_schema_vertex("post.title"),
            &schema,
            &protocol,
        )
        .unwrap();
        assert_eq!(concrete_cell.residue, vec![TableResidueKind::Cell]);
    }

    #[test]
    fn shape_chain_classifiers_stop_at_the_first_invalid_step() {
        let schema = three_node_schema();
        let protocol = test_protocol();
        let chain = ProtolensChain::new(vec![elementary::drop_sort("missing")]);
        assert!(graph_chain_complement_spec(&chain, &schema, &protocol).is_err());
        assert!(table_chain_complement_spec(&chain, &schema, &protocol).is_err());
    }
}
