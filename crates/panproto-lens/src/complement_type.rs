//! Complement type system for protolenses.
//!
//! Given a protolens η and schema S, compute the complement type
//! `ComplementType(η, S)`. This is a dependent type — the complement
//! varies with the schema the protolens is instantiated at.

use panproto_gat::Name;
use panproto_inst::value::Value;
use panproto_schema::{Protocol, Schema};
use serde::{Deserialize, Serialize};

use crate::protolens::{ComplementConstructor, Protolens, ProtolensChain};

/// Static specification of what data a complement will contain.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub enum ComplementKind {
    /// No complement needed — isomorphism.
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
pub struct CapturedField {
    /// Name of the captured element.
    pub element_name: Name,
    /// What kind: "sort" or "op".
    pub element_kind: String,
    /// Human-readable description.
    pub description: String,
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
            summary: "Identity transformation — no complement needed.".into(),
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
            let count = schema.vertices.values().filter(|v| v.kind == *sort).count();
            ComplementSpec {
                kind: ComplementKind::DataCaptured,
                forward_defaults: vec![],
                captured_data: vec![CapturedField {
                    element_name: sort.clone(),
                    element_kind: "sort".into(),
                    description: format!(
                        "Data for {count} vertices of kind '{sort}' will be captured in the complement.",
                    ),
                }],
                summary: format!("Drops sort '{sort}' — {count} vertices captured in complement.",),
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
                summary: format!("Drops operation '{op}' — {count} edges captured."),
            }
        }
        ComplementConstructor::AddedElement {
            element_name,
            element_kind,
        } => ComplementSpec {
            kind: ComplementKind::DefaultsRequired,
            forward_defaults: vec![DefaultRequirement {
                element_name: element_name.clone(),
                element_kind: element_kind.clone(),
                description: format!(
                    "Default value needed for added {element_kind} '{element_name}'.",
                ),
                suggested_default: None,
            }],
            captured_data: vec![],
            summary: format!("Adds {element_kind} '{element_name}' — default required."),
        },
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
            summary: format!("Value conversion via '{nat_trans_name}' — kernel captured."),
        },
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
        ComplementKind::Empty => "Lossless transformation — no complement needed.".into(),
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

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into(), "array".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
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
        assert!(spec.captured_data.len() == 1);
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
        assert!(spec.captured_data.len() == 1);
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
}
