//! Classification of schema diffs into breaking vs. non-breaking changes.
//!
//! [`classify`] takes a [`SchemaDiff`] and a [`Protocol`] and determines
//! which changes are backward-incompatible (breaking) and which are safe
//! (non-breaking). The classification is protocol-aware: for example,
//! removing a vertex that serves as the target of a required edge is
//! always breaking.

use panproto_schema::Protocol;
use serde::{Deserialize, Serialize};

use crate::diff::{ConstraintChange, SchemaDiff};

/// The result of classifying a schema diff.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatReport {
    /// Changes that break backward compatibility.
    pub breaking: Vec<BreakingChange>,
    /// Changes that are safe for existing consumers.
    pub non_breaking: Vec<NonBreakingChange>,
    /// `true` if the migration is fully backward-compatible.
    pub compatible: bool,
}

/// A change that breaks backward compatibility.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum BreakingChange {
    /// A vertex was removed from the schema.
    RemovedVertex {
        /// The removed vertex ID.
        vertex_id: String,
    },

    /// An edge was removed from the schema.
    RemovedEdge {
        /// Source vertex ID.
        src: String,
        /// Target vertex ID.
        tgt: String,
        /// Edge kind.
        kind: String,
        /// Edge name, if present.
        name: Option<String>,
    },

    /// A vertex's kind changed.
    KindChanged {
        /// The vertex ID.
        vertex_id: String,
        /// The old kind.
        old_kind: String,
        /// The new kind.
        new_kind: String,
    },

    /// A constraint was tightened (made more restrictive).
    ConstraintTightened {
        /// The vertex ID.
        vertex_id: String,
        /// The constraint sort.
        sort: String,
        /// The old value.
        old_value: String,
        /// The new value.
        new_value: String,
    },

    /// A new constraint was added to an existing vertex.
    ConstraintAdded {
        /// The vertex ID.
        vertex_id: String,
        /// The constraint sort.
        sort: String,
        /// The constraint value.
        value: String,
    },

    /// A coproduct variant was removed (type error for existing data).
    RemovedVariant {
        /// The parent coproduct vertex ID.
        vertex_id: String,
        /// The removed variant ID.
        variant_id: String,
    },

    /// An ordered collection became unordered (lossy).
    OrderToUnordered {
        /// The edge that lost its ordering.
        edge: panproto_schema::Edge,
    },

    /// A recursion point was removed (breaks recursive types).
    RecursionBroken {
        /// The removed fixpoint marker ID.
        mu_id: String,
    },

    /// An edge's usage mode was tightened (e.g., structural → linear).
    LinearityTightened {
        /// The affected edge.
        edge: panproto_schema::Edge,
        /// The old usage mode.
        old_mode: panproto_schema::UsageMode,
        /// The new usage mode.
        new_mode: panproto_schema::UsageMode,
    },
}

/// A non-breaking (backward-compatible) change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum NonBreakingChange {
    /// A new vertex was added.
    AddedVertex {
        /// The added vertex ID.
        vertex_id: String,
    },

    /// A new edge was added.
    AddedEdge {
        /// Source vertex ID.
        src: String,
        /// Target vertex ID.
        tgt: String,
        /// Edge kind.
        kind: String,
        /// Edge name, if present.
        name: Option<String>,
    },

    /// A constraint was relaxed (made less restrictive).
    ConstraintRelaxed {
        /// The vertex ID.
        vertex_id: String,
        /// The constraint sort.
        sort: String,
        /// The old value.
        old_value: String,
        /// The new value.
        new_value: String,
    },

    /// A constraint was removed from a vertex.
    ConstraintRemoved {
        /// The vertex ID.
        vertex_id: String,
        /// The constraint sort.
        sort: String,
    },
}

/// Classify a [`SchemaDiff`] into breaking and non-breaking changes.
///
/// The classification depends on the protocol's constraint sorts and
/// edge rules to determine the severity of each change.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn classify(diff: &SchemaDiff, protocol: &Protocol) -> CompatReport {
    let mut breaking = Vec::new();
    let mut non_breaking = Vec::new();

    // Removed vertices are always breaking.
    for v in &diff.removed_vertices {
        breaking.push(BreakingChange::RemovedVertex {
            vertex_id: v.clone(),
        });
    }

    // Added vertices are non-breaking.
    for v in &diff.added_vertices {
        non_breaking.push(NonBreakingChange::AddedVertex {
            vertex_id: v.clone(),
        });
    }

    // Removed edges: breaking if the edge kind is governed by an edge rule
    // in the protocol (i.e., the protocol considers that edge kind
    // structurally significant). Edges with no matching rule are
    // non-breaking removals.
    for e in &diff.removed_edges {
        if protocol.find_edge_rule(&e.kind).is_some() {
            breaking.push(BreakingChange::RemovedEdge {
                src: e.src.clone(),
                tgt: e.tgt.clone(),
                kind: e.kind.clone(),
                name: e.name.clone(),
            });
        } else {
            non_breaking.push(NonBreakingChange::AddedEdge {
                src: e.src.clone(),
                tgt: e.tgt.clone(),
                kind: e.kind.clone(),
                name: e.name.clone(),
            });
        }
    }

    // Added edges are non-breaking.
    for e in &diff.added_edges {
        non_breaking.push(NonBreakingChange::AddedEdge {
            src: e.src.clone(),
            tgt: e.tgt.clone(),
            kind: e.kind.clone(),
            name: e.name.clone(),
        });
    }

    // Kind changes are always breaking.
    for kc in &diff.kind_changes {
        breaking.push(BreakingChange::KindChanged {
            vertex_id: kc.vertex_id.clone(),
            old_kind: kc.old_kind.clone(),
            new_kind: kc.new_kind.clone(),
        });
    }

    // Constraint changes — only classify constraints whose sort is
    // recognized by the protocol. Unknown sorts are silently ignored
    // (they are not part of this protocol's contract).
    for (vid, cdiff) in &diff.modified_constraints {
        // New constraints on existing vertices are breaking
        // (only for recognized sorts).
        for c in &cdiff.added {
            if protocol.constraint_sorts.iter().any(|s| s == &c.sort) {
                breaking.push(BreakingChange::ConstraintAdded {
                    vertex_id: vid.clone(),
                    sort: c.sort.clone(),
                    value: c.value.clone(),
                });
            }
        }

        // Removed constraints are non-breaking (relaxation)
        // (only for recognized sorts).
        for c in &cdiff.removed {
            if protocol.constraint_sorts.iter().any(|s| s == &c.sort) {
                non_breaking.push(NonBreakingChange::ConstraintRemoved {
                    vertex_id: vid.clone(),
                    sort: c.sort.clone(),
                });
            }
        }

        // Changed constraints: direction depends on the sort
        // (only for recognized sorts).
        for change in &cdiff.changed {
            if protocol.constraint_sorts.iter().any(|s| s == &change.sort) {
                classify_constraint_change(vid, change, &mut breaking, &mut non_breaking);
            }
        }
    }

    // --- Variant changes ---
    for v in &diff.removed_variants {
        breaking.push(BreakingChange::RemovedVariant {
            vertex_id: v.parent_vertex.clone(),
            variant_id: v.id.clone(),
        });
    }

    // --- Ordering changes ---
    for (edge, old_pos, new_pos) in &diff.order_changes {
        if old_pos.is_some() && new_pos.is_none() {
            breaking.push(BreakingChange::OrderToUnordered { edge: edge.clone() });
        }
    }

    // --- Recursion point changes ---
    for rp in &diff.removed_recursion_points {
        breaking.push(BreakingChange::RecursionBroken {
            mu_id: rp.mu_id.clone(),
        });
    }

    // --- Usage mode changes ---
    for (edge, old_mode, new_mode) in &diff.usage_mode_changes {
        // Tightening: Structural → Linear/Affine, or Affine → Linear
        let is_tightened = matches!(
            (old_mode, new_mode),
            (
                panproto_schema::UsageMode::Structural | panproto_schema::UsageMode::Affine,
                panproto_schema::UsageMode::Linear
            ) | (
                panproto_schema::UsageMode::Structural,
                panproto_schema::UsageMode::Affine
            )
        );
        if is_tightened {
            breaking.push(BreakingChange::LinearityTightened {
                edge: edge.clone(),
                old_mode: old_mode.clone(),
                new_mode: new_mode.clone(),
            });
        }
    }

    let compatible = breaking.is_empty();
    CompatReport {
        breaking,
        non_breaking,
        compatible,
    }
}

/// Determine whether a constraint value change is tightening or relaxing.
fn classify_constraint_change(
    vertex_id: &str,
    change: &ConstraintChange,
    breaking: &mut Vec<BreakingChange>,
    non_breaking: &mut Vec<NonBreakingChange>,
) {
    let is_tightened = is_constraint_tightened(&change.sort, &change.old_value, &change.new_value);

    if is_tightened {
        breaking.push(BreakingChange::ConstraintTightened {
            vertex_id: vertex_id.to_string(),
            sort: change.sort.clone(),
            old_value: change.old_value.clone(),
            new_value: change.new_value.clone(),
        });
    } else {
        non_breaking.push(NonBreakingChange::ConstraintRelaxed {
            vertex_id: vertex_id.to_string(),
            sort: change.sort.clone(),
            old_value: change.old_value.clone(),
            new_value: change.new_value.clone(),
        });
    }
}

/// Determine if a constraint value change is a tightening.
///
/// For upper-bound constraints (`maxLength`, `maximum`, etc.), a smaller
/// new value is tighter. For lower-bound constraints (`minLength`, `minimum`),
/// a larger new value is tighter. For all others, any change is
/// considered tightening.
fn is_constraint_tightened(sort: &str, old_val: &str, new_val: &str) -> bool {
    match sort {
        "maxLength" | "maxSize" | "maximum" | "maxGraphemes" => {
            let old_n: Result<i64, _> = old_val.parse();
            let new_n: Result<i64, _> = new_val.parse();
            if let (Ok(o), Ok(n)) = (old_n, new_n) {
                return n < o;
            }
            // Non-numeric: any change is tightening.
            true
        }
        "minLength" | "minimum" => {
            let old_n: Result<i64, _> = old_val.parse();
            let new_n: Result<i64, _> = new_val.parse();
            if let (Ok(o), Ok(n)) = (old_n, new_n) {
                return n > o;
            }
            true
        }
        _ => {
            // For unknown constraint sorts, any change is tightening.
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{ConstraintDiff, KindChange};
    use panproto_schema::{Edge, EdgeRule};

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![EdgeRule {
                edge_kind: "prop".into(),
                src_kinds: vec!["object".into()],
                tgt_kinds: vec![],
            }],
            obj_kinds: vec!["object".into()],
            constraint_sorts: vec!["maxLength".into()],
            ..Protocol::default()
        }
    }

    #[test]
    fn classify_removed_required_field_as_breaking() {
        let diff = SchemaDiff {
            removed_vertices: vec!["body.text".into()],
            removed_edges: vec![Edge {
                src: "body".into(),
                tgt: "body.text".into(),
                kind: "prop".into(),
                name: Some("text".into()),
            }],
            ..SchemaDiff::default()
        };

        let report = classify(&diff, &test_protocol());
        assert!(!report.compatible, "removing a vertex should be breaking");
        assert_eq!(report.breaking.len(), 2); // vertex + edge
    }

    #[test]
    fn classify_added_optional_field_as_non_breaking() {
        let diff = SchemaDiff {
            added_vertices: vec!["body.newField".into()],
            added_edges: vec![Edge {
                src: "body".into(),
                tgt: "body.newField".into(),
                kind: "prop".into(),
                name: Some("newField".into()),
            }],
            ..SchemaDiff::default()
        };

        let report = classify(&diff, &test_protocol());
        assert!(report.compatible, "adding a vertex should be non-breaking");
        assert_eq!(report.non_breaking.len(), 2); // vertex + edge
        assert!(report.breaking.is_empty());
    }

    #[test]
    fn classify_constraint_tightening_as_breaking() {
        let diff = SchemaDiff {
            modified_constraints: std::iter::once((
                "body.text".into(),
                ConstraintDiff {
                    added: vec![],
                    removed: vec![],
                    changed: vec![crate::diff::ConstraintChange {
                        sort: "maxLength".into(),
                        old_value: "3000".into(),
                        new_value: "300".into(),
                    }],
                },
            ))
            .collect(),
            ..SchemaDiff::default()
        };

        let report = classify(&diff, &test_protocol());
        assert!(
            !report.compatible,
            "tightening maxLength should be breaking"
        );
        assert!(
            report
                .breaking
                .iter()
                .any(|b| matches!(b, BreakingChange::ConstraintTightened { .. }))
        );
    }

    #[test]
    fn classify_constraint_relaxing_as_non_breaking() {
        let diff = SchemaDiff {
            modified_constraints: std::iter::once((
                "body.text".into(),
                ConstraintDiff {
                    added: vec![],
                    removed: vec![],
                    changed: vec![crate::diff::ConstraintChange {
                        sort: "maxLength".into(),
                        old_value: "300".into(),
                        new_value: "3000".into(),
                    }],
                },
            ))
            .collect(),
            ..SchemaDiff::default()
        };

        let report = classify(&diff, &test_protocol());
        assert!(
            report.compatible,
            "relaxing maxLength should be non-breaking"
        );
        assert!(
            report
                .non_breaking
                .iter()
                .any(|nb| matches!(nb, NonBreakingChange::ConstraintRelaxed { .. }))
        );
    }

    #[test]
    fn classify_kind_change_as_breaking() {
        let diff = SchemaDiff {
            kind_changes: vec![KindChange {
                vertex_id: "x".into(),
                old_kind: "string".into(),
                new_kind: "integer".into(),
            }],
            ..SchemaDiff::default()
        };

        let report = classify(&diff, &test_protocol());
        assert!(!report.compatible, "kind change should be breaking");
    }
}
