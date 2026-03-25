//! Edit lens law verification.
//!
//! Checks the two Hofmann-Pierce-Wagner edit lens laws:
//!
//! - **Consistency**: translating a source edit and applying it to the
//!   view gives the same result as applying the edit to the source and
//!   then doing a whole-state `get`.
//! - **Complement coherence**: the complement state after `get_edit` is
//!   consistent with the complement that would result from a whole-state
//!   `get` on the edited source.

use std::fmt;

use panproto_inst::{TreeEdit, WInstance};

use crate::Lens;
use crate::edit_error::EditLensError;
use crate::edit_lens::EditLens;

/// A violation of an edit lens law.
#[derive(Debug)]
#[non_exhaustive]
pub enum EditLawViolation {
    /// Consistency law violation.
    Consistency {
        /// Description of the mismatch.
        detail: String,
    },
    /// Complement coherence law violation.
    ComplementCoherence {
        /// Description of the mismatch.
        detail: String,
    },
    /// An error occurred during law checking.
    Error(EditLensError),
}

impl fmt::Display for EditLawViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Consistency { detail } => write!(f, "Consistency law violated: {detail}"),
            Self::ComplementCoherence { detail } => {
                write!(f, "Complement coherence violated: {detail}")
            }
            Self::Error(e) => write!(f, "error during law check: {e}"),
        }
    }
}

impl std::error::Error for EditLawViolation {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Error(e) => Some(e),
            _ => None,
        }
    }
}

/// Check the Consistency law for an edit lens.
///
/// Verifies that translating `edit` through the lens and applying the
/// result to the current view produces the same view as applying `edit`
/// to the source and then doing a whole-state `get`.
///
/// # Errors
///
/// Returns [`EditLawViolation`] if the law is violated or an error occurs.
pub fn check_edit_consistency(
    lens: &mut EditLens,
    edit: &TreeEdit,
    source: &WInstance,
) -> Result<(), EditLawViolation> {
    // Path 1: translate edit, apply to view.
    let mut lens_clone = clone_edit_lens(lens);
    let view_edit = lens_clone
        .get_edit(edit.clone())
        .map_err(EditLawViolation::Error)?;

    // Get current view via whole-state get.
    let state_lens = Lens {
        compiled: lens.compiled.clone(),
        src_schema: lens.src_schema.clone(),
        tgt_schema: lens.tgt_schema.clone(),
    };
    let (mut view, _) = crate::get(&state_lens, source)
        .map_err(|e| EditLawViolation::Error(EditLensError::TranslationFailed(e.to_string())))?;
    view_edit
        .apply(&mut view)
        .map_err(|e| EditLawViolation::Error(EditLensError::EditApply(e)))?;

    // Path 2: apply edit to source, then whole-state get.
    let mut edited_source = source.clone();
    edit.apply(&mut edited_source)
        .map_err(|e| EditLawViolation::Error(EditLensError::EditApply(e)))?;
    let (view2, _) = crate::get(&state_lens, &edited_source)
        .map_err(|e| EditLawViolation::Error(EditLensError::TranslationFailed(e.to_string())))?;

    // Compare views.
    if view.node_count() != view2.node_count() {
        return Err(EditLawViolation::Consistency {
            detail: format!(
                "node count mismatch: translate-then-apply={}, apply-then-get={}",
                view.node_count(),
                view2.node_count()
            ),
        });
    }

    for (id, node) in &view.nodes {
        if let Some(node2) = view2.nodes.get(id) {
            if node.anchor != node2.anchor {
                return Err(EditLawViolation::Consistency {
                    detail: format!(
                        "anchor mismatch at node {id}: {a1} vs {a2}",
                        a1 = node.anchor,
                        a2 = node2.anchor,
                    ),
                });
            }
        } else {
            return Err(EditLawViolation::Consistency {
                detail: format!("node {id} present in path 1 but not path 2"),
            });
        }
    }

    Ok(())
}

/// Check the Complement coherence law for an edit lens.
///
/// Verifies that the complement state after `get_edit` matches the
/// complement that would result from a whole-state `get` on the edited
/// source.
///
/// # Errors
///
/// Returns [`EditLawViolation`] if the law is violated or an error occurs.
pub fn check_complement_coherence(
    lens: &mut EditLens,
    edit: &TreeEdit,
    source: &WInstance,
) -> Result<(), EditLawViolation> {
    // Path 1: get_edit on the lens.
    let mut lens_clone = clone_edit_lens(lens);
    let _ = lens_clone
        .get_edit(edit.clone())
        .map_err(EditLawViolation::Error)?;

    // Path 2: apply edit to source, then whole-state get.
    let mut edited_source = source.clone();
    edit.apply(&mut edited_source)
        .map_err(|e| EditLawViolation::Error(EditLensError::EditApply(e)))?;

    let state_lens = Lens {
        compiled: lens.compiled.clone(),
        src_schema: lens.src_schema.clone(),
        tgt_schema: lens.tgt_schema.clone(),
    };
    let (_, complement2) = crate::get(&state_lens, &edited_source)
        .map_err(|e| EditLawViolation::Error(EditLensError::TranslationFailed(e.to_string())))?;

    // Compare complements.
    let c1 = &lens_clone.complement;
    if c1.dropped_nodes.len() != complement2.dropped_nodes.len() {
        return Err(EditLawViolation::ComplementCoherence {
            detail: format!(
                "dropped_nodes count mismatch: edit_lens={}, whole_state={}",
                c1.dropped_nodes.len(),
                complement2.dropped_nodes.len()
            ),
        });
    }

    Ok(())
}

/// Clone an `EditLens` for law checking (needs all fields).
fn clone_edit_lens(lens: &EditLens) -> EditLens {
    EditLens {
        compiled: lens.compiled.clone(),
        src_schema: lens.src_schema.clone(),
        tgt_schema: lens.tgt_schema.clone(),
        complement: lens.complement.clone(),
        protocol: lens.protocol.clone(),
        reverse_vertex_remap: lens.reverse_vertex_remap.clone(),
        reverse_edge_remap: lens.reverse_edge_remap.clone(),
        pipeline: lens.pipeline.clone(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use panproto_gat::Name;
    use panproto_inst::{TreeEdit, Value};
    use panproto_schema::Protocol;

    use crate::edit_lens::EditLens;
    use crate::tests::{identity_lens, three_node_instance, three_node_schema};

    use super::*;

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
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
    fn consistency_identity_lens() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        let edit = TreeEdit::SetField {
            node_id: 1,
            field: Name::from("text"),
            value: Value::Str("changed".into()),
        };

        let result = check_edit_consistency(&mut edit_lens, &edit, &instance);
        assert!(result.is_ok(), "consistency should hold: {result:?}");
    }

    #[test]
    fn complement_coherence_identity_lens() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        let edit = TreeEdit::SetField {
            node_id: 1,
            field: Name::from("text"),
            value: Value::Str("changed".into()),
        };

        let result = check_complement_coherence(&mut edit_lens, &edit, &instance);
        assert!(result.is_ok(), "coherence should hold: {result:?}");
    }
}
