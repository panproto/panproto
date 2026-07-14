//! Classification of schema diffs into breaking vs. non-breaking changes.
//!
//! [`classify`] takes a [`SchemaDiff`] and a [`Protocol`] and determines
//! which changes are backward-incompatible (breaking) and which are safe
//! (non-breaking). The classification is protocol-aware: for example,
//! removing a vertex that serves as the target of a required edge is
//! always breaking.
//!
//! # Classification rules
//!
//! The tri-state verdict is summarised by [`Classification`]:
//! *fully-compatible* (no changes in either direction),
//! *backward-compatible* (non-breaking changes only), or *breaking*
//! (at least one breaking change).
//!
//! Per category, the rules are:
//!
//! - **Vertices**: removals breaking, additions non-breaking. A detected
//!   rename (see [`RenamedVertex`](BreakingChange::RenamedVertex)) is a
//!   single breaking change that suppresses the removed/added pair.
//! - **Edges**: removals breaking when the edge kind is governed by a
//!   protocol edge rule, non-breaking otherwise; additions non-breaking.
//! - **Required edges**: additions and removals both breaking (a newly
//!   required edge rejects existing data; a removed requirement drops a
//!   guarantee consumers relied on).
//! - **Kind changes**: always breaking.
//! - **Constraints**: additions breaking, removals non-breaking, value
//!   changes tightening-breaking / relaxing-non-breaking. Sorts the
//!   protocol does not recognise fall through to the conservative
//!   tightening default rather than being dropped.
//! - **Variants**: removals, modifications, and additions all breaking
//!   (openness is not encoded in [`Protocol`], so additions default to
//!   the closed-union reading).
//! - **Orderings**: ordered-to-unordered, unordered-to-ordered, and
//!   in-place reorderings all breaking.
//! - **Recursion points**: additions, removals, and target
//!   modifications all breaking.
//! - **Usage modes**: tightening breaking, relaxing non-breaking.
//! - **NSIDs**: additions non-breaking, changes and removals breaking.
//! - **Hyper-edges / spans**: additions non-breaking, removals and
//!   signature modifications breaking.
//! - **Nominal identity**: any flip breaking in either direction.
//! - **Enrichments** (coercions, mergers, defaults, policies):
//!   additions non-breaking, removals and modifications breaking.
//!   [`classify_with_schemas`] layers the schema-level coercion class
//!   downgrade check on top.
//!
//! The [`classify`] function destructures [`SchemaDiff`] exhaustively so
//! that a newly added diff field is a compile error until it is given a
//! rule here; the [`UnclassifiedChange`](BreakingChange::UnclassifiedChange)
//! bucket is the conservative fail-closed fallback for any residual
//! sub-case that has no dedicated variant.

use panproto_schema::Protocol;
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};

use crate::diff::{ConstraintChange, SchemaDiff};

/// The tri-state compatibility verdict for a [`CompatReport`].
///
/// - [`FullyCompatible`](Classification::FullyCompatible): no breaking
///   and no non-breaking changes; the two schemas are equivalent for
///   compatibility purposes and round-trip in both directions.
/// - [`BackwardCompatible`](Classification::BackwardCompatible):
///   non-breaking changes only; existing data and consumers keep
///   working, but the reverse direction may not.
/// - [`Breaking`](Classification::Breaking): at least one breaking
///   change; existing data or consumers can be invalidated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Classification {
    /// No changes in either direction.
    FullyCompatible,
    /// Only non-breaking (backward-compatible) changes.
    BackwardCompatible,
    /// At least one breaking change. The default, so deserialising a
    /// legacy report that predates this field fails closed.
    #[default]
    Breaking,
}

/// The result of classifying a schema diff.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatReport {
    /// Changes that break backward compatibility.
    pub breaking: Vec<BreakingChange>,
    /// Changes that are safe for existing consumers.
    pub non_breaking: Vec<NonBreakingChange>,
    /// `true` if the migration is fully backward-compatible.
    pub compatible: bool,
    /// The tri-state compatibility verdict.
    #[serde(default)]
    pub classification: Classification,
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

    /// A required edge was added on an existing vertex (existing data
    /// lacking the edge becomes invalid).
    RequiredEdgeAdded {
        /// The vertex the requirement is attached to.
        vertex_id: String,
        /// Source vertex ID.
        src: String,
        /// Target vertex ID.
        tgt: String,
        /// Edge kind.
        kind: String,
        /// Edge name, if present.
        name: Option<String>,
    },

    /// A required edge was removed on an existing vertex (a guarantee
    /// consumers relied on is gone).
    RequiredEdgeRemoved {
        /// The vertex the requirement was attached to.
        vertex_id: String,
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

    /// A coproduct variant was added. Openness is not encoded in the
    /// protocol, so this defaults to the closed-union reading where
    /// existing consumers reject the unknown variant.
    AddedVariant {
        /// The parent coproduct vertex ID.
        vertex_id: String,
        /// The added variant ID.
        variant_id: String,
    },

    /// A coproduct variant was removed (type error for existing data).
    RemovedVariant {
        /// The parent coproduct vertex ID.
        vertex_id: String,
        /// The removed variant ID.
        variant_id: String,
    },

    /// A coproduct variant's tag changed.
    ModifiedVariant {
        /// The parent coproduct vertex ID.
        vertex_id: String,
        /// The variant ID.
        variant_id: String,
        /// The old tag.
        old_tag: Option<String>,
        /// The new tag.
        new_tag: Option<String>,
    },

    /// An ordered collection became unordered (lossy).
    OrderToUnordered {
        /// The edge that lost its ordering.
        edge: panproto_schema::Edge,
    },

    /// An unordered collection became ordered (consumers relying on set
    /// semantics can break).
    UnorderedToOrdered {
        /// The edge that gained an ordering.
        edge: panproto_schema::Edge,
    },

    /// A recursion point was added (the type became recursive).
    RecursionPointAdded {
        /// The added fixpoint marker ID.
        mu_id: String,
    },

    /// A recursion point was removed (breaks recursive types).
    RecursionBroken {
        /// The removed fixpoint marker ID.
        mu_id: String,
    },

    /// A recursion point's target vertex changed.
    RecursionPointModified {
        /// The fixpoint marker ID.
        mu_id: String,
        /// The old target vertex.
        old_target: String,
        /// The new target vertex.
        new_target: String,
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

    /// A vertex's NSID mapping changed.
    NsidChanged {
        /// The vertex ID.
        vertex_id: String,
        /// The old NSID.
        old_nsid: String,
        /// The new NSID.
        new_nsid: String,
    },

    /// A vertex's NSID mapping was removed.
    NsidRemoved {
        /// The vertex ID.
        vertex_id: String,
    },

    /// A hyper-edge was removed from the schema.
    HyperEdgeRemoved {
        /// The removed hyper-edge ID.
        id: String,
    },

    /// A hyper-edge's kind, signature, or parent label changed.
    HyperEdgeModified {
        /// The hyper-edge ID.
        id: String,
    },

    /// A span was removed from the schema.
    SpanRemoved {
        /// The removed span ID.
        id: String,
    },

    /// A span's left or right vertex changed.
    SpanModified {
        /// The span ID.
        id: String,
    },

    /// A vertex's nominal-identity flag flipped in either direction.
    NominalFlipped {
        /// The vertex ID.
        vertex_id: String,
        /// The old nominal flag.
        old_value: bool,
        /// The new nominal flag.
        new_value: bool,
    },

    /// An enrichment (coercion, merger, default, or policy) was removed.
    EnrichmentRemoved {
        /// The enrichment category (`"coercion"`, `"merger"`,
        /// `"default"`, or `"policy"`).
        category: String,
        /// The enrichment key (vertex ID, sort, or `"from -> to"` pair).
        key: String,
    },

    /// An enrichment (coercion, merger, default, or policy) was modified.
    EnrichmentModified {
        /// The enrichment category.
        category: String,
        /// The enrichment key.
        key: String,
    },

    /// A coercion's round-trip class was downgraded (e.g., Iso to Retraction).
    CoercionClassDowngraded {
        /// The source kind of the coercion.
        from_kind: String,
        /// The target kind of the coercion.
        to_kind: String,
        /// The old coercion class.
        old_class: String,
        /// The new coercion class.
        new_class: String,
    },

    /// A coercion was removed from the schema.
    ///
    /// Diff-level coercion removals are reported as
    /// [`EnrichmentRemoved`](BreakingChange::EnrichmentRemoved); this
    /// richer variant is retained for API stability.
    CoercionRemoved {
        /// The source kind of the removed coercion.
        from_kind: String,
        /// The target kind of the removed coercion.
        to_kind: String,
    },

    /// A vertex was renamed (old ID to new ID), detected from a
    /// removed/added pair.
    RenamedVertex {
        /// The old vertex ID.
        old_id: String,
        /// The new vertex ID.
        new_id: String,
    },

    /// A residual, non-empty diff sub-case with no dedicated variant.
    ///
    /// This is the conservative fail-closed bucket: any change routed
    /// here forces `compatible == false` rather than being dropped.
    UnclassifiedChange {
        /// A short label for the sub-case.
        category: String,
        /// The number of changes rolled into this bucket.
        count: usize,
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

    /// An edge was removed but its kind is not governed by any protocol
    /// edge rule, so it is considered non-breaking.
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

    /// A vertex gained an NSID mapping.
    AddedNsid {
        /// The vertex ID.
        vertex_id: String,
        /// The added NSID.
        nsid: String,
    },

    /// A hyper-edge was added to the schema.
    AddedHyperEdge {
        /// The added hyper-edge ID.
        id: String,
    },

    /// A span was added to the schema.
    AddedSpan {
        /// The added span ID.
        id: String,
    },

    /// An enrichment (coercion, merger, default, or policy) was added.
    EnrichmentAdded {
        /// The enrichment category.
        category: String,
        /// The enrichment key.
        key: String,
    },

    /// An edge's usage mode was relaxed (e.g., linear → structural).
    LinearityRelaxed {
        /// The affected edge.
        edge: panproto_schema::Edge,
        /// The old usage mode.
        old_mode: panproto_schema::UsageMode,
        /// The new usage mode.
        new_mode: panproto_schema::UsageMode,
    },
}

/// Classify a [`SchemaDiff`] into breaking and non-breaking changes.
///
/// The classification depends on the protocol's edge rules to determine
/// the severity of edge changes, and on the per-category rules
/// documented at the module level.
///
/// [`SchemaDiff`] is destructured exhaustively (no `..` rest pattern) so
/// that adding a diff field without a classification branch here is a
/// compile error; the fail-closed rule routes any residual sub-case
/// into [`UnclassifiedChange`](BreakingChange::UnclassifiedChange).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn classify(diff: &SchemaDiff, protocol: &Protocol) -> CompatReport {
    let mut breaking = Vec::new();
    let mut non_breaking = Vec::new();

    // Fail-closed exhaustive destructuring: a new SchemaDiff field must
    // gain a branch below or this stops compiling.
    let SchemaDiff {
        added_vertices,
        removed_vertices,
        kind_changes,
        added_edges,
        removed_edges,
        modified_constraints,
        added_hyper_edges,
        removed_hyper_edges,
        modified_hyper_edges,
        added_required,
        removed_required,
        added_nsids,
        removed_nsids,
        changed_nsids,
        added_variants,
        removed_variants,
        modified_variants,
        order_changes,
        added_recursion_points,
        removed_recursion_points,
        modified_recursion_points,
        usage_mode_changes,
        added_spans,
        removed_spans,
        modified_spans,
        nominal_changes,
        added_coercions,
        removed_coercions,
        modified_coercions,
        added_mergers,
        removed_mergers,
        modified_mergers,
        added_defaults,
        removed_defaults,
        modified_defaults,
        added_policies,
        removed_policies,
        modified_policies,
        renamed_vertices,
    } = diff;

    // --- Renames --- (record and suppress the removed/added pair)
    let mut renamed_old: FxHashSet<&str> = FxHashSet::default();
    let mut renamed_new: FxHashSet<&str> = FxHashSet::default();
    for (old_id, new_id) in renamed_vertices {
        renamed_old.insert(old_id.as_str());
        renamed_new.insert(new_id.as_str());
        breaking.push(BreakingChange::RenamedVertex {
            old_id: old_id.clone(),
            new_id: new_id.clone(),
        });
    }

    // --- Required edges --- (both directions breaking)
    let required_added: FxHashSet<&panproto_schema::Edge> =
        added_required.values().flatten().collect();
    let required_removed: FxHashSet<&panproto_schema::Edge> =
        removed_required.values().flatten().collect();
    for (vid, edges) in added_required {
        for e in edges {
            breaking.push(BreakingChange::RequiredEdgeAdded {
                vertex_id: vid.clone(),
                src: e.src.to_string(),
                tgt: e.tgt.to_string(),
                kind: e.kind.to_string(),
                name: e.name.as_ref().map(ToString::to_string),
            });
        }
    }
    for (vid, edges) in removed_required {
        for e in edges {
            breaking.push(BreakingChange::RequiredEdgeRemoved {
                vertex_id: vid.clone(),
                src: e.src.to_string(),
                tgt: e.tgt.to_string(),
                kind: e.kind.to_string(),
                name: e.name.as_ref().map(ToString::to_string),
            });
        }
    }

    // Removed vertices are breaking (unless part of a detected rename).
    for v in removed_vertices {
        if renamed_old.contains(v.as_str()) {
            continue;
        }
        breaking.push(BreakingChange::RemovedVertex {
            vertex_id: v.clone(),
        });
    }

    // Added vertices are non-breaking (unless part of a detected rename).
    for v in added_vertices {
        if renamed_new.contains(v.as_str()) {
            continue;
        }
        non_breaking.push(NonBreakingChange::AddedVertex {
            vertex_id: v.clone(),
        });
    }

    // Removed edges: breaking if the edge kind is governed by a protocol
    // edge rule. Edges also covered by a required-edge removal are
    // suppressed to avoid a duplicate breaking entry.
    for e in removed_edges {
        if required_removed.contains(e) {
            continue;
        }
        if protocol.find_edge_rule(&e.kind).is_some() {
            breaking.push(BreakingChange::RemovedEdge {
                src: e.src.to_string(),
                tgt: e.tgt.to_string(),
                kind: e.kind.to_string(),
                name: e.name.as_ref().map(ToString::to_string),
            });
        } else {
            non_breaking.push(NonBreakingChange::RemovedEdge {
                src: e.src.to_string(),
                tgt: e.tgt.to_string(),
                kind: e.kind.to_string(),
                name: e.name.as_ref().map(ToString::to_string),
            });
        }
    }

    // Added edges are non-breaking, unless the same edge is also a newly
    // required edge (already reported as breaking above).
    for e in added_edges {
        if required_added.contains(e) {
            continue;
        }
        non_breaking.push(NonBreakingChange::AddedEdge {
            src: e.src.to_string(),
            tgt: e.tgt.to_string(),
            kind: e.kind.to_string(),
            name: e.name.as_ref().map(ToString::to_string),
        });
    }

    // Kind changes are always breaking.
    for kc in kind_changes {
        breaking.push(BreakingChange::KindChanged {
            vertex_id: kc.vertex_id.clone(),
            old_kind: kc.old_kind.clone(),
            new_kind: kc.new_kind.clone(),
        });
    }

    // Constraint changes are classified for every sort. Unrecognised
    // sorts fall through to the conservative tightening default in
    // `is_constraint_tightened` rather than being dropped.
    for (vid, cdiff) in modified_constraints {
        for c in &cdiff.added {
            breaking.push(BreakingChange::ConstraintAdded {
                vertex_id: vid.clone(),
                sort: c.sort.to_string(),
                value: c.value.clone(),
            });
        }
        for c in &cdiff.removed {
            non_breaking.push(NonBreakingChange::ConstraintRemoved {
                vertex_id: vid.clone(),
                sort: c.sort.to_string(),
            });
        }
        for change in &cdiff.changed {
            classify_constraint_change(vid, change, &mut breaking, &mut non_breaking);
        }
    }

    // --- Variant changes ---
    for v in added_variants {
        breaking.push(BreakingChange::AddedVariant {
            vertex_id: v.parent_vertex.to_string(),
            variant_id: v.id.to_string(),
        });
    }
    for v in removed_variants {
        breaking.push(BreakingChange::RemovedVariant {
            vertex_id: v.parent_vertex.to_string(),
            variant_id: v.id.to_string(),
        });
    }
    for vc in modified_variants {
        breaking.push(BreakingChange::ModifiedVariant {
            vertex_id: vc.parent_vertex.clone(),
            variant_id: vc.id.clone(),
            old_tag: vc.old_tag.clone(),
            new_tag: vc.new_tag.clone(),
        });
    }

    // --- Ordering changes ---
    for (edge, old_pos, new_pos) in order_changes {
        match (old_pos.is_some(), new_pos.is_some()) {
            (true, false) => {
                breaking.push(BreakingChange::OrderToUnordered { edge: edge.clone() });
            }
            (false, true) => {
                breaking.push(BreakingChange::UnorderedToOrdered { edge: edge.clone() });
            }
            // Both positions present but different: an in-place reorder.
            // No dedicated variant, so route through the conservative
            // fail-closed bucket.
            _ => {
                breaking.push(BreakingChange::UnclassifiedChange {
                    category: "reordered_edge".to_string(),
                    count: 1,
                });
            }
        }
    }

    // --- Recursion point changes ---
    for rp in added_recursion_points {
        breaking.push(BreakingChange::RecursionPointAdded {
            mu_id: rp.mu_id.to_string(),
        });
    }
    for rp in removed_recursion_points {
        breaking.push(BreakingChange::RecursionBroken {
            mu_id: rp.mu_id.to_string(),
        });
    }
    for rpc in modified_recursion_points {
        breaking.push(BreakingChange::RecursionPointModified {
            mu_id: rpc.mu_id.clone(),
            old_target: rpc.old_target.clone(),
            new_target: rpc.new_target.clone(),
        });
    }

    // --- Usage mode changes ---
    for (edge, old_mode, new_mode) in usage_mode_changes {
        if is_usage_tightened(old_mode, new_mode) {
            breaking.push(BreakingChange::LinearityTightened {
                edge: edge.clone(),
                old_mode: old_mode.clone(),
                new_mode: new_mode.clone(),
            });
        } else {
            non_breaking.push(NonBreakingChange::LinearityRelaxed {
                edge: edge.clone(),
                old_mode: old_mode.clone(),
                new_mode: new_mode.clone(),
            });
        }
    }

    // --- NSID changes ---
    for (vid, nsid) in added_nsids {
        non_breaking.push(NonBreakingChange::AddedNsid {
            vertex_id: vid.clone(),
            nsid: nsid.clone(),
        });
    }
    for vid in removed_nsids {
        breaking.push(BreakingChange::NsidRemoved {
            vertex_id: vid.clone(),
        });
    }
    for (vid, old_nsid, new_nsid) in changed_nsids {
        breaking.push(BreakingChange::NsidChanged {
            vertex_id: vid.clone(),
            old_nsid: old_nsid.clone(),
            new_nsid: new_nsid.clone(),
        });
    }

    // --- Hyper-edge changes ---
    for id in added_hyper_edges {
        non_breaking.push(NonBreakingChange::AddedHyperEdge { id: id.clone() });
    }
    for id in removed_hyper_edges {
        breaking.push(BreakingChange::HyperEdgeRemoved { id: id.clone() });
    }
    for hec in modified_hyper_edges {
        breaking.push(BreakingChange::HyperEdgeModified { id: hec.id.clone() });
    }

    // --- Span changes ---
    for id in added_spans {
        non_breaking.push(NonBreakingChange::AddedSpan { id: id.clone() });
    }
    for id in removed_spans {
        breaking.push(BreakingChange::SpanRemoved { id: id.clone() });
    }
    for sc in modified_spans {
        breaking.push(BreakingChange::SpanModified { id: sc.id.clone() });
    }

    // --- Nominal identity changes ---
    for (vid, old_val, new_val) in nominal_changes {
        breaking.push(BreakingChange::NominalFlipped {
            vertex_id: vid.clone(),
            old_value: *old_val,
            new_value: *new_val,
        });
    }

    // --- Enrichment changes ---
    classify_enrichment(
        "coercion",
        added_coercions.iter().map(coercion_key),
        removed_coercions.iter().map(coercion_key),
        modified_coercions.iter().map(coercion_key),
        &mut breaking,
        &mut non_breaking,
    );
    classify_enrichment(
        "merger",
        added_mergers.iter().cloned(),
        removed_mergers.iter().cloned(),
        modified_mergers.iter().cloned(),
        &mut breaking,
        &mut non_breaking,
    );
    classify_enrichment(
        "default",
        added_defaults.iter().cloned(),
        removed_defaults.iter().cloned(),
        modified_defaults.iter().cloned(),
        &mut breaking,
        &mut non_breaking,
    );
    classify_enrichment(
        "policy",
        added_policies.iter().cloned(),
        removed_policies.iter().cloned(),
        modified_policies.iter().cloned(),
        &mut breaking,
        &mut non_breaking,
    );

    finish_report(breaking, non_breaking)
}

/// Classify a schema diff with access to the old and new schemas for
/// enrichment-level checks (coercion class downgrades).
///
/// This extends the basic [`classify`] with the schema-level coercion
/// class downgrade check, which is not derivable from the structural
/// diff alone. Diff-level coercion removals are already reported by
/// [`classify`] as [`EnrichmentRemoved`](BreakingChange::EnrichmentRemoved).
#[must_use]
pub fn classify_with_schemas(
    diff: &SchemaDiff,
    protocol: &Protocol,
    old_schema: &panproto_schema::Schema,
    new_schema: &panproto_schema::Schema,
) -> CompatReport {
    let mut report = classify(diff, protocol);

    // Check coercion class downgrades: if a coercion exists in both schemas
    // but the new class is strictly greater (more lossy) than the old class,
    // that is a breaking change.
    for (key, new_spec) in &new_schema.coercions {
        if let Some(old_spec) = old_schema.coercions.get(key) {
            if new_spec.class > old_spec.class {
                report
                    .breaking
                    .push(BreakingChange::CoercionClassDowngraded {
                        from_kind: key.0.to_string(),
                        to_kind: key.1.to_string(),
                        old_class: format!("{:?}", old_spec.class),
                        new_class: format!("{:?}", new_spec.class),
                    });
            }
        }
    }

    report.compatible = report.breaking.is_empty();
    report.classification = classify_verdict(&report.breaking, &report.non_breaking);
    report
}

/// Build the tri-state verdict from the breaking/non-breaking lists.
const fn classify_verdict(
    breaking: &[BreakingChange],
    non_breaking: &[NonBreakingChange],
) -> Classification {
    if !breaking.is_empty() {
        Classification::Breaking
    } else if non_breaking.is_empty() {
        Classification::FullyCompatible
    } else {
        Classification::BackwardCompatible
    }
}

/// Assemble a [`CompatReport`], deriving `compatible` and `classification`.
fn finish_report(
    breaking: Vec<BreakingChange>,
    non_breaking: Vec<NonBreakingChange>,
) -> CompatReport {
    let compatible = breaking.is_empty();
    let classification = classify_verdict(&breaking, &non_breaking);
    CompatReport {
        breaking,
        non_breaking,
        compatible,
        classification,
    }
}

/// Format a coercion `(from, to)` key as a display string.
fn coercion_key(key: &(String, String)) -> String {
    format!("{} -> {}", key.0, key.1)
}

/// Classify one enrichment category: additions non-breaking, removals
/// and modifications breaking.
fn classify_enrichment(
    category: &str,
    added: impl IntoIterator<Item = String>,
    removed: impl IntoIterator<Item = String>,
    modified: impl IntoIterator<Item = String>,
    breaking: &mut Vec<BreakingChange>,
    non_breaking: &mut Vec<NonBreakingChange>,
) {
    for key in added {
        non_breaking.push(NonBreakingChange::EnrichmentAdded {
            category: category.to_string(),
            key,
        });
    }
    for key in removed {
        breaking.push(BreakingChange::EnrichmentRemoved {
            category: category.to_string(),
            key,
        });
    }
    for key in modified {
        breaking.push(BreakingChange::EnrichmentModified {
            category: category.to_string(),
            key,
        });
    }
}

/// Determine whether a usage-mode change is a tightening.
///
/// Tightening restricts how an edge may be used: `Structural` → `Affine`
/// or `Linear`, or `Affine` → `Linear`. Everything else is a relaxation.
const fn is_usage_tightened(
    old_mode: &panproto_schema::UsageMode,
    new_mode: &panproto_schema::UsageMode,
) -> bool {
    use panproto_schema::UsageMode::{Affine, Linear, Structural};
    matches!(
        (old_mode, new_mode),
        (Structural | Affine, Linear) | (Structural, Affine)
    )
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
    use crate::diff::{
        ConstraintDiff, HyperEdgeChange, KindChange, RecursionPointChange, SpanChange,
        VariantChange,
    };
    use panproto_schema::{Constraint, Edge, EdgeRule, RecursionPoint, UsageMode, Variant};
    use std::collections::HashMap;

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

    fn edge(src: &str, tgt: &str, kind: &str, name: Option<&str>) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: kind.into(),
            name: name.map(Into::into),
        }
    }

    #[test]
    fn classify_removed_required_field_as_breaking() {
        let diff = SchemaDiff {
            removed_vertices: vec!["body.text".into()],
            removed_edges: vec![edge("body", "body.text", "prop", Some("text"))],
            ..SchemaDiff::default()
        };

        let report = classify(&diff, &test_protocol());
        assert!(!report.compatible, "removing a vertex should be breaking");
        assert_eq!(report.breaking.len(), 2); // vertex + edge
        assert_eq!(report.classification, Classification::Breaking);
    }

    #[test]
    fn classify_added_optional_field_as_non_breaking() {
        let diff = SchemaDiff {
            added_vertices: vec!["body.newField".into()],
            added_edges: vec![edge("body", "body.newField", "prop", Some("newField"))],
            ..SchemaDiff::default()
        };

        let report = classify(&diff, &test_protocol());
        assert!(report.compatible, "adding a vertex should be non-breaking");
        assert_eq!(report.non_breaking.len(), 2); // vertex + edge
        assert!(report.breaking.is_empty());
        assert_eq!(report.classification, Classification::BackwardCompatible);
    }

    #[test]
    fn classify_empty_diff_is_fully_compatible() {
        let report = classify(&SchemaDiff::default(), &test_protocol());
        assert!(report.compatible);
        assert!(report.breaking.is_empty());
        assert!(report.non_breaking.is_empty());
        assert_eq!(report.classification, Classification::FullyCompatible);
    }

    // -----------------------------------------------------------------------
    // Required edges
    // -----------------------------------------------------------------------

    #[test]
    fn classify_added_required_edge_as_breaking() {
        let e = edge("body", "body.text", "prop", Some("text"));
        let diff = SchemaDiff {
            added_required: HashMap::from([("body".into(), vec![e.clone()])]),
            // An added-and-required edge also shows up as an added edge.
            added_edges: vec![e],
            ..SchemaDiff::default()
        };

        let report = classify(&diff, &test_protocol());
        assert!(!report.compatible, "adding a required edge is breaking");
        assert!(
            report
                .breaking
                .iter()
                .any(|b| matches!(b, BreakingChange::RequiredEdgeAdded { .. }))
        );
        // The duplicate non-breaking AddedEdge is suppressed.
        assert!(
            !report
                .non_breaking
                .iter()
                .any(|nb| matches!(nb, NonBreakingChange::AddedEdge { .. }))
        );
    }

    #[test]
    fn classify_removed_required_edge_as_breaking() {
        let e = edge("body", "body.text", "prop", Some("text"));
        let diff = SchemaDiff {
            removed_required: HashMap::from([("body".into(), vec![e])]),
            ..SchemaDiff::default()
        };
        let report = classify(&diff, &test_protocol());
        assert!(!report.compatible);
        assert!(
            report
                .breaking
                .iter()
                .any(|b| matches!(b, BreakingChange::RequiredEdgeRemoved { .. }))
        );
    }

    // -----------------------------------------------------------------------
    // Variants
    // -----------------------------------------------------------------------

    #[test]
    fn classify_added_variant_as_breaking_under_unknown_openness() {
        let diff = SchemaDiff {
            added_variants: vec![Variant {
                id: "v2".into(),
                parent_vertex: "u".into(),
                tag: Some("b".into()),
            }],
            ..SchemaDiff::default()
        };
        let report = classify(&diff, &test_protocol());
        assert!(!report.compatible, "added variant defaults to breaking");
        assert!(
            report
                .breaking
                .iter()
                .any(|b| matches!(b, BreakingChange::AddedVariant { .. }))
        );
    }

    #[test]
    fn classify_modified_variant_as_breaking() {
        let diff = SchemaDiff {
            modified_variants: vec![VariantChange {
                id: "v1".into(),
                parent_vertex: "u".into(),
                old_tag: Some("a".into()),
                new_tag: Some("b".into()),
            }],
            ..SchemaDiff::default()
        };
        let report = classify(&diff, &test_protocol());
        assert!(!report.compatible, "modified variant is breaking");
        assert!(
            report
                .breaking
                .iter()
                .any(|b| matches!(b, BreakingChange::ModifiedVariant { .. }))
        );
    }

    // -----------------------------------------------------------------------
    // Constraints
    // -----------------------------------------------------------------------

    #[test]
    fn classify_constraint_tightening_as_breaking() {
        let diff = SchemaDiff {
            modified_constraints: std::iter::once((
                "body.text".into(),
                ConstraintDiff {
                    added: vec![],
                    removed: vec![],
                    changed: vec![ConstraintChange {
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
                    changed: vec![ConstraintChange {
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
    fn classify_unlisted_sort_constraint_change_as_breaking() {
        // 'customSort' is not in the protocol's constraint_sorts, but its
        // change must still be classified via the conservative
        // tightening default rather than dropped.
        let diff = SchemaDiff {
            modified_constraints: std::iter::once((
                "body.text".into(),
                ConstraintDiff {
                    added: vec![],
                    removed: vec![],
                    changed: vec![ConstraintChange {
                        sort: "customSort".into(),
                        old_value: "a".into(),
                        new_value: "b".into(),
                    }],
                },
            ))
            .collect(),
            ..SchemaDiff::default()
        };

        let report = classify(&diff, &test_protocol());
        assert!(
            !report.compatible,
            "a change on an unlisted constraint sort must be breaking"
        );
    }

    #[test]
    fn classify_added_constraint_on_unlisted_sort_as_breaking() {
        let diff = SchemaDiff {
            modified_constraints: std::iter::once((
                "body.text".into(),
                ConstraintDiff {
                    added: vec![Constraint {
                        sort: "customSort".into(),
                        value: "v".into(),
                    }],
                    removed: vec![],
                    changed: vec![],
                },
            ))
            .collect(),
            ..SchemaDiff::default()
        };
        let report = classify(&diff, &test_protocol());
        assert!(!report.compatible);
        assert!(
            report
                .breaking
                .iter()
                .any(|b| matches!(b, BreakingChange::ConstraintAdded { .. }))
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

    #[test]
    fn classify_removed_non_governed_edge_as_non_breaking() {
        let diff = SchemaDiff {
            removed_edges: vec![edge("body", "body.note", "annotation", Some("note"))],
            ..SchemaDiff::default()
        };

        let report = classify(&diff, &test_protocol());
        assert!(report.compatible);
        assert_eq!(report.non_breaking.len(), 1);
        assert!(report.non_breaking.iter().any(
            |nb| matches!(nb, NonBreakingChange::RemovedEdge { kind, .. } if kind == "annotation")
        ),);
    }

    #[test]
    fn classify_removed_governed_edge_as_breaking() {
        let diff = SchemaDiff {
            removed_edges: vec![edge("body", "body.text", "prop", Some("text"))],
            ..SchemaDiff::default()
        };

        let report = classify(&diff, &test_protocol());
        assert!(!report.compatible);
        assert_eq!(report.breaking.len(), 1);
        assert!(
            report
                .breaking
                .iter()
                .any(|b| matches!(b, BreakingChange::RemovedEdge { kind, .. } if kind == "prop"))
        );
    }

    // -----------------------------------------------------------------------
    // Per-category breaking/non-breaking coverage
    // -----------------------------------------------------------------------

    #[test]
    fn nsid_add_non_breaking_change_remove_breaking() {
        let added = SchemaDiff {
            added_nsids: HashMap::from([("a".into(), "com.example.thing".into())]),
            ..SchemaDiff::default()
        };
        assert!(classify(&added, &test_protocol()).compatible);

        let changed = SchemaDiff {
            changed_nsids: vec![("a".into(), "com.old".into(), "com.new".into())],
            ..SchemaDiff::default()
        };
        assert!(!classify(&changed, &test_protocol()).compatible);

        let removed = SchemaDiff {
            removed_nsids: vec!["a".into()],
            ..SchemaDiff::default()
        };
        assert!(!classify(&removed, &test_protocol()).compatible);
    }

    #[test]
    fn hyper_edge_add_non_breaking_remove_modify_breaking() {
        let added = SchemaDiff {
            added_hyper_edges: vec!["he1".into()],
            ..SchemaDiff::default()
        };
        assert!(classify(&added, &test_protocol()).compatible);

        let removed = SchemaDiff {
            removed_hyper_edges: vec!["he1".into()],
            ..SchemaDiff::default()
        };
        assert!(!classify(&removed, &test_protocol()).compatible);

        let modified = SchemaDiff {
            modified_hyper_edges: vec![HyperEdgeChange {
                id: "he1".into(),
                kind_change: Some(("join".into(), "merge".into())),
                signature_added: HashMap::new(),
                signature_removed: HashMap::new(),
                signature_changed: HashMap::new(),
                parent_label_change: None,
            }],
            ..SchemaDiff::default()
        };
        assert!(!classify(&modified, &test_protocol()).compatible);
    }

    #[test]
    fn span_add_non_breaking_remove_modify_breaking() {
        let added = SchemaDiff {
            added_spans: vec!["s1".into()],
            ..SchemaDiff::default()
        };
        assert!(classify(&added, &test_protocol()).compatible);

        let removed = SchemaDiff {
            removed_spans: vec!["s1".into()],
            ..SchemaDiff::default()
        };
        assert!(!classify(&removed, &test_protocol()).compatible);

        let modified = SchemaDiff {
            modified_spans: vec![SpanChange {
                id: "s1".into(),
                left_change: Some(("a".into(), "b".into())),
                right_change: None,
            }],
            ..SchemaDiff::default()
        };
        assert!(!classify(&modified, &test_protocol()).compatible);
    }

    #[test]
    fn nominal_flip_breaking_both_directions() {
        for (old, new) in [(false, true), (true, false)] {
            let diff = SchemaDiff {
                nominal_changes: vec![("a".into(), old, new)],
                ..SchemaDiff::default()
            };
            assert!(
                !classify(&diff, &test_protocol()).compatible,
                "nominal flip {old}->{new} must be breaking"
            );
        }
    }

    #[test]
    fn recursion_point_add_remove_modify_breaking() {
        let added = SchemaDiff {
            added_recursion_points: vec![RecursionPoint {
                mu_id: "m".into(),
                target_vertex: "t".into(),
            }],
            ..SchemaDiff::default()
        };
        assert!(!classify(&added, &test_protocol()).compatible);

        let removed = SchemaDiff {
            removed_recursion_points: vec![RecursionPoint {
                mu_id: "m".into(),
                target_vertex: "t".into(),
            }],
            ..SchemaDiff::default()
        };
        assert!(!classify(&removed, &test_protocol()).compatible);

        let modified = SchemaDiff {
            modified_recursion_points: vec![RecursionPointChange {
                mu_id: "m".into(),
                old_target: "a".into(),
                new_target: "b".into(),
            }],
            ..SchemaDiff::default()
        };
        assert!(!classify(&modified, &test_protocol()).compatible);
    }

    #[test]
    fn ordering_transitions_breaking() {
        let e = edge("a", "b", "prop", None);
        let to_unordered = SchemaDiff {
            order_changes: vec![(e.clone(), Some(0), None)],
            ..SchemaDiff::default()
        };
        assert!(!classify(&to_unordered, &test_protocol()).compatible);

        let to_ordered = SchemaDiff {
            order_changes: vec![(e.clone(), None, Some(0))],
            ..SchemaDiff::default()
        };
        let report = classify(&to_ordered, &test_protocol());
        assert!(!report.compatible);
        assert!(
            report
                .breaking
                .iter()
                .any(|b| matches!(b, BreakingChange::UnorderedToOrdered { .. }))
        );

        let reordered = SchemaDiff {
            order_changes: vec![(e, Some(0), Some(1))],
            ..SchemaDiff::default()
        };
        let report = classify(&reordered, &test_protocol());
        assert!(!report.compatible);
        assert!(
            report
                .breaking
                .iter()
                .any(|b| matches!(b, BreakingChange::UnclassifiedChange { .. }))
        );
    }

    #[test]
    fn usage_mode_tighten_breaking_relax_non_breaking() {
        let e = edge("a", "b", "prop", None);
        let tighten = SchemaDiff {
            usage_mode_changes: vec![(e.clone(), UsageMode::Structural, UsageMode::Linear)],
            ..SchemaDiff::default()
        };
        assert!(!classify(&tighten, &test_protocol()).compatible);

        let relax = SchemaDiff {
            usage_mode_changes: vec![(e, UsageMode::Linear, UsageMode::Structural)],
            ..SchemaDiff::default()
        };
        assert!(classify(&relax, &test_protocol()).compatible);
    }

    #[test]
    fn enrichment_add_non_breaking_remove_modify_breaking() {
        let added = SchemaDiff {
            added_coercions: vec![("a".into(), "b".into())],
            added_mergers: vec!["m".into()],
            added_defaults: vec!["d".into()],
            added_policies: vec!["p".into()],
            ..SchemaDiff::default()
        };
        assert!(classify(&added, &test_protocol()).compatible);

        let removed = SchemaDiff {
            removed_coercions: vec![("a".into(), "b".into())],
            ..SchemaDiff::default()
        };
        assert!(!classify(&removed, &test_protocol()).compatible);

        let modified = SchemaDiff {
            modified_policies: vec!["p".into()],
            ..SchemaDiff::default()
        };
        assert!(!classify(&modified, &test_protocol()).compatible);
    }

    /// Fail-closed guarantee: every diff category, populated
    /// alone, produces a classification (breaking for removals /
    /// modifications / tightenings, non-breaking for optional
    /// additions), never a silent "compatible" verdict on a non-empty
    /// diff.
    #[test]
    #[allow(clippy::too_many_lines)]
    fn fail_closed_every_category_is_classified() {
        let e = edge("a", "b", "prop", None);
        let breaking_cases: Vec<(&str, SchemaDiff)> = vec![
            (
                "removed_vertices",
                SchemaDiff {
                    removed_vertices: vec!["a".into()],
                    ..SchemaDiff::default()
                },
            ),
            (
                "kind_changes",
                SchemaDiff {
                    kind_changes: vec![KindChange {
                        vertex_id: "a".into(),
                        old_kind: "x".into(),
                        new_kind: "y".into(),
                    }],
                    ..SchemaDiff::default()
                },
            ),
            (
                "removed_hyper_edges",
                SchemaDiff {
                    removed_hyper_edges: vec!["he".into()],
                    ..SchemaDiff::default()
                },
            ),
            (
                "added_required",
                SchemaDiff {
                    added_required: HashMap::from([("a".into(), vec![e.clone()])]),
                    ..SchemaDiff::default()
                },
            ),
            (
                "removed_required",
                SchemaDiff {
                    removed_required: HashMap::from([("a".into(), vec![e.clone()])]),
                    ..SchemaDiff::default()
                },
            ),
            (
                "changed_nsids",
                SchemaDiff {
                    changed_nsids: vec![("a".into(), "x".into(), "y".into())],
                    ..SchemaDiff::default()
                },
            ),
            (
                "removed_nsids",
                SchemaDiff {
                    removed_nsids: vec!["a".into()],
                    ..SchemaDiff::default()
                },
            ),
            (
                "added_variants",
                SchemaDiff {
                    added_variants: vec![Variant {
                        id: "v".into(),
                        parent_vertex: "u".into(),
                        tag: None,
                    }],
                    ..SchemaDiff::default()
                },
            ),
            (
                "removed_variants",
                SchemaDiff {
                    removed_variants: vec![Variant {
                        id: "v".into(),
                        parent_vertex: "u".into(),
                        tag: None,
                    }],
                    ..SchemaDiff::default()
                },
            ),
            (
                "order_to_unordered",
                SchemaDiff {
                    order_changes: vec![(e.clone(), Some(0), None)],
                    ..SchemaDiff::default()
                },
            ),
            (
                "unordered_to_ordered",
                SchemaDiff {
                    order_changes: vec![(e, None, Some(0))],
                    ..SchemaDiff::default()
                },
            ),
            (
                "added_recursion_points",
                SchemaDiff {
                    added_recursion_points: vec![RecursionPoint {
                        mu_id: "m".into(),
                        target_vertex: "t".into(),
                    }],
                    ..SchemaDiff::default()
                },
            ),
            (
                "modified_recursion_points",
                SchemaDiff {
                    modified_recursion_points: vec![RecursionPointChange {
                        mu_id: "m".into(),
                        old_target: "a".into(),
                        new_target: "b".into(),
                    }],
                    ..SchemaDiff::default()
                },
            ),
            (
                "removed_spans",
                SchemaDiff {
                    removed_spans: vec!["s".into()],
                    ..SchemaDiff::default()
                },
            ),
            (
                "nominal_changes",
                SchemaDiff {
                    nominal_changes: vec![("a".into(), false, true)],
                    ..SchemaDiff::default()
                },
            ),
            (
                "removed_coercions",
                SchemaDiff {
                    removed_coercions: vec![("a".into(), "b".into())],
                    ..SchemaDiff::default()
                },
            ),
            (
                "removed_mergers",
                SchemaDiff {
                    removed_mergers: vec!["m".into()],
                    ..SchemaDiff::default()
                },
            ),
            (
                "removed_defaults",
                SchemaDiff {
                    removed_defaults: vec!["d".into()],
                    ..SchemaDiff::default()
                },
            ),
            (
                "removed_policies",
                SchemaDiff {
                    removed_policies: vec!["p".into()],
                    ..SchemaDiff::default()
                },
            ),
        ];

        for (label, diff) in &breaking_cases {
            let report = classify(diff, &test_protocol());
            assert!(
                !report.compatible,
                "category {label} must classify as breaking"
            );
            assert_eq!(report.classification, Classification::Breaking, "{label}");
        }

        // Optional additions are backward-compatible, not dropped.
        let non_breaking_cases: Vec<(&str, SchemaDiff)> = vec![
            (
                "added_nsids",
                SchemaDiff {
                    added_nsids: HashMap::from([("a".into(), "x".into())]),
                    ..SchemaDiff::default()
                },
            ),
            (
                "added_hyper_edges",
                SchemaDiff {
                    added_hyper_edges: vec!["he".into()],
                    ..SchemaDiff::default()
                },
            ),
            (
                "added_spans",
                SchemaDiff {
                    added_spans: vec!["s".into()],
                    ..SchemaDiff::default()
                },
            ),
            (
                "added_coercions",
                SchemaDiff {
                    added_coercions: vec![("a".into(), "b".into())],
                    ..SchemaDiff::default()
                },
            ),
            (
                "added_mergers",
                SchemaDiff {
                    added_mergers: vec!["m".into()],
                    ..SchemaDiff::default()
                },
            ),
            (
                "added_defaults",
                SchemaDiff {
                    added_defaults: vec!["d".into()],
                    ..SchemaDiff::default()
                },
            ),
            (
                "added_policies",
                SchemaDiff {
                    added_policies: vec!["p".into()],
                    ..SchemaDiff::default()
                },
            ),
        ];

        for (label, diff) in &non_breaking_cases {
            let report = classify(diff, &test_protocol());
            assert!(report.compatible, "category {label} should be non-breaking");
            assert_eq!(
                report.classification,
                Classification::BackwardCompatible,
                "{label}"
            );
            assert!(!report.non_breaking.is_empty(), "{label} produced no entry");
        }
    }

    // -----------------------------------------------------------------------
    // Renames
    // -----------------------------------------------------------------------

    #[test]
    fn classify_rename_suppresses_removed_added_pair() {
        let diff = SchemaDiff {
            removed_vertices: vec!["root.text".into()],
            added_vertices: vec!["root.body".into()],
            renamed_vertices: vec![("root.text".into(), "root.body".into())],
            ..SchemaDiff::default()
        };

        let report = classify(&diff, &test_protocol());
        assert_eq!(
            report.breaking.len(),
            1,
            "only the rename should be breaking"
        );
        assert!(report.breaking.iter().any(
            |b| matches!(b, BreakingChange::RenamedVertex { old_id, new_id }
                    if old_id == "root.text" && new_id == "root.body")
        ));
        assert!(
            !report
                .breaking
                .iter()
                .any(|b| matches!(b, BreakingChange::RemovedVertex { .. })),
            "the removed vertex must be suppressed"
        );
        assert!(
            report.non_breaking.is_empty(),
            "the added vertex must be suppressed"
        );
    }

    // -----------------------------------------------------------------------
    // Classification with schemas
    // -----------------------------------------------------------------------

    #[test]
    fn classify_with_schemas_sets_classification() {
        let diff = SchemaDiff {
            added_vertices: vec!["x".into()],
            ..SchemaDiff::default()
        };
        let schema = empty_schema();
        let report = classify_with_schemas(&diff, &test_protocol(), &schema, &schema);
        assert_eq!(report.classification, Classification::BackwardCompatible);
    }

    fn empty_schema() -> panproto_schema::Schema {
        panproto_schema::Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        }
    }
}
