//! Provenance tracking for translated edits.
//!
//! An [`EditProvenance`] record captures the lineage of a translated
//! edit: which source edit produced it, which translation rules fired
//! during `get_edit`, and whether the translation was total (all
//! refinement constraints satisfied) or partial.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Provenance record for a translated edit.
///
/// Tracks which source edit produced this view edit, which translation
/// rules fired, and whether the translation was total or partial.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditProvenance {
    /// Description of the original source edit.
    pub source_edit_desc: String,
    /// Names of translation rules that fired during `get_edit`.
    pub rules_applied: Vec<Arc<str>>,
    /// Name of the complement policy that was consulted (if any).
    pub policy_consulted: Option<Arc<str>>,
    /// Whether the translation was total (all constraints satisfied).
    pub was_total: bool,
}

impl EditProvenance {
    /// Create a new provenance record with the given source description.
    #[must_use]
    pub const fn new(source_edit_desc: String) -> Self {
        Self {
            source_edit_desc,
            rules_applied: Vec::new(),
            policy_consulted: None,
            was_total: true,
        }
    }

    /// Record that a translation rule fired.
    pub fn record_rule(&mut self, rule: Arc<str>) {
        self.rules_applied.push(rule);
    }

    /// Record that a complement policy was consulted.
    pub fn record_policy(&mut self, policy: Arc<str>) {
        self.policy_consulted = Some(policy);
    }

    /// Mark the translation as partial (a refinement constraint failed).
    pub const fn mark_partial(&mut self) {
        self.was_total = false;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use panproto_gat::Name;
    use panproto_inst::TreeEdit;
    use panproto_schema::Protocol;

    use crate::edit_lens::EditLens;
    use crate::tests::{identity_lens, three_node_instance, three_node_schema};

    use super::EditProvenance;

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            schema_composition: None,
            instance_composition: None,
            edge_rules: vec![],
            obj_kinds: vec![],
            constraint_sorts: vec![],
            has_order: false,
            has_coproducts: false,
            has_recursion: false,
            has_causal: false,
            nominal_identity: false,
            has_defaults: false,
            has_coercions: false,
            has_mergers: false,
            has_policies: false,
        }
    }

    #[test]
    fn provenance_records_structural_remap() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        let edit = TreeEdit::SetField {
            node_id: 1,
            field: Name::from("text"),
            value: panproto_inst::Value::Str("updated".into()),
        };

        let (_translated, provenance) = edit_lens.get_edit_with_provenance(edit).unwrap();
        assert!(
            provenance
                .rules_applied
                .iter()
                .any(|r| r.as_ref() == "structural_remap"),
            "provenance should record structural_remap rule"
        );
    }

    #[test]
    fn provenance_identity_is_total() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        let edit = TreeEdit::SetField {
            node_id: 1,
            field: Name::from("text"),
            value: panproto_inst::Value::Str("hello".into()),
        };

        let (_translated, provenance) = edit_lens.get_edit_with_provenance(edit).unwrap();
        assert!(
            provenance.was_total,
            "identity lens translation should be total"
        );
    }

    #[test]
    fn provenance_serialization_round_trip() {
        let prov = EditProvenance {
            source_edit_desc: "SetField(1, text)".into(),
            rules_applied: vec![Arc::from("structural_remap"), Arc::from("field_text")],
            policy_consulted: Some(Arc::from("last_writer_wins")),
            was_total: true,
        };

        let json = serde_json::to_string(&prov).unwrap();
        let back: EditProvenance = serde_json::from_str(&json).unwrap();

        assert_eq!(back.source_edit_desc, prov.source_edit_desc);
        assert_eq!(back.rules_applied.len(), 2);
        assert_eq!(back.rules_applied[0].as_ref(), "structural_remap");
        assert_eq!(back.rules_applied[1].as_ref(), "field_text");
        assert_eq!(back.policy_consulted.as_deref(), Some("last_writer_wins"));
        assert!(back.was_total);
    }
}
