//! Three-way schema merge via pushout construction.
//!
//! Given a base schema and two divergent schemas ("ours" and "theirs"),
//! computes the categorical pushout — the universal schema receiving
//! morphisms from both sides that agree on the shared base.
//!
//! The merge is structural, not textual — it operates on the schema
//! graph (vertices, edges, constraints, hyper-edges, etc.) rather than
//! on serialized text. There is no "ours wins" tie-breaking: when both
//! sides modify the same element differently, a [`MergeConflict`] is
//! reported and the base value is retained in the merged schema.

use panproto_check::diff::{self, SchemaDiff};
use panproto_mig::Migration;
use panproto_schema::{Constraint, Edge, Schema, Span, UsageMode, Variant, Vertex};
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::collections::HashMap;

use crate::auto_mig;

/// Options controlling merge behavior.
#[derive(Clone, Debug, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct MergeOptions {
    /// Merge but don't auto-commit; leave result staged.
    pub no_commit: bool,
    /// Fail if the merge cannot be resolved as a fast-forward.
    pub ff_only: bool,
    /// Create a merge commit even for fast-forward merges.
    pub no_ff: bool,
    /// Squash all commits into a single change (no merge commit).
    pub squash: bool,
    /// Custom merge commit message.
    pub message: Option<String>,
}

/// The result of a three-way merge.
#[derive(Clone, Debug)]
pub struct MergeResult {
    /// The merged schema. When conflicts exist, conflicted elements
    /// retain their base values.
    pub merged_schema: Schema,
    /// Any conflicts detected during the merge.
    pub conflicts: Vec<MergeConflict>,
    /// Migration from "ours" schema to the merged schema.
    pub migration_from_ours: Migration,
    /// Migration from "theirs" schema to the merged schema.
    pub migration_from_theirs: Migration,
}

/// A conflict detected during three-way merge.
///
/// Each variant corresponds to a case where the pushout does not exist
/// cleanly because both sides made incompatible changes to the same
/// schema element.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MergeConflict {
    // --- Vertices ---
    /// Both branches modified the same vertex's kind differently.
    BothModifiedVertex {
        /// The ID of the vertex both sides modified.
        vertex_id: String,
        /// The kind our branch changed the vertex to.
        ours_kind: String,
        /// The kind their branch changed the vertex to.
        theirs_kind: String,
    },

    /// Both branches added a vertex with the same ID but different kinds.
    BothAddedVertexDifferently {
        /// The ID of the vertex both sides added.
        vertex_id: String,
        /// The kind our branch used.
        ours_kind: String,
        /// The kind their branch used.
        theirs_kind: String,
    },

    /// One branch deleted a vertex that the other modified.
    DeleteModifyVertex {
        /// The ID of the contested vertex.
        vertex_id: String,
        /// Which side performed the deletion.
        deleted_by: Side,
    },

    // --- Edges ---
    /// One branch deleted an edge that the other still has.
    DeleteModifyEdge {
        /// The contested edge.
        edge: Edge,
        /// Which side performed the deletion.
        deleted_by: Side,
    },

    // --- Constraints ---
    /// Both branches modified the same constraint differently.
    BothModifiedConstraint {
        /// The vertex the constraint belongs to.
        vertex_id: String,
        /// The constraint sort.
        sort: String,
        /// The value our branch set.
        ours_value: String,
        /// The value their branch set.
        theirs_value: String,
    },

    /// Both branches added the same constraint sort with different values.
    BothAddedConstraintDifferently {
        /// The vertex the constraint belongs to.
        vertex_id: String,
        /// The constraint sort.
        sort: String,
        /// The value our branch added.
        ours_value: String,
        /// The value their branch added.
        theirs_value: String,
    },

    /// One branch removed a constraint that the other modified.
    DeleteModifyConstraint {
        /// The vertex the constraint belongs to.
        vertex_id: String,
        /// The constraint sort.
        sort: String,
        /// Which side performed the deletion.
        deleted_by: Side,
    },

    // --- Hyper-edges ---
    /// One branch deleted a hyper-edge that the other modified.
    DeleteModifyHyperEdge {
        /// The ID of the contested hyper-edge.
        hyper_edge_id: String,
        /// Which side performed the deletion.
        deleted_by: Side,
    },

    /// Both branches modified the same hyper-edge differently.
    BothModifiedHyperEdge {
        /// The ID of the contested hyper-edge.
        hyper_edge_id: String,
    },

    /// Both branches added a hyper-edge with the same ID but different content.
    BothAddedHyperEdgeDifferently {
        /// The ID of the contested hyper-edge.
        hyper_edge_id: String,
    },

    // --- Variants ---
    /// One branch removed a variant that the other modified.
    DeleteModifyVariant {
        /// The ID of the contested variant.
        variant_id: String,
        /// The parent vertex of the variant.
        parent_vertex: String,
        /// Which side performed the deletion.
        deleted_by: Side,
    },

    /// Both branches modified the same variant's tag differently.
    BothModifiedVariant {
        /// The ID of the contested variant.
        variant_id: String,
        /// The parent vertex of the variant.
        parent_vertex: String,
        /// The tag our branch set (if any).
        ours_tag: Option<String>,
        /// The tag their branch set (if any).
        theirs_tag: Option<String>,
    },

    // --- Orderings ---
    /// Both branches changed the same edge's ordering differently.
    BothModifiedOrdering {
        /// The contested edge.
        edge: Edge,
        /// The position our branch set (if any).
        ours_position: Option<u32>,
        /// The position their branch set (if any).
        theirs_position: Option<u32>,
    },

    // --- Recursion points ---
    /// Both branches modified the same recursion point's target differently.
    BothModifiedRecursionPoint {
        /// The mu binder ID of the recursion point.
        mu_id: String,
        /// The target vertex our branch set.
        ours_target: String,
        /// The target vertex their branch set.
        theirs_target: String,
    },

    /// One branch removed a recursion point that the other modified.
    DeleteModifyRecursionPoint {
        /// The mu binder ID of the recursion point.
        mu_id: String,
        /// Which side performed the deletion.
        deleted_by: Side,
    },

    // --- Usage modes ---
    /// Both branches changed the same edge's usage mode differently.
    BothModifiedUsageMode {
        /// The contested edge.
        edge: Edge,
        /// The mode our branch set.
        ours_mode: UsageMode,
        /// The mode their branch set.
        theirs_mode: UsageMode,
    },

    // --- NSIDs ---
    /// Both branches changed the same vertex's NSID differently.
    BothModifiedNsid {
        /// The ID of the contested vertex.
        vertex_id: String,
        /// The NSID our branch set.
        ours_nsid: String,
        /// The NSID their branch set.
        theirs_nsid: String,
    },

    /// One branch removed an NSID that the other modified.
    DeleteModifyNsid {
        /// The ID of the contested vertex.
        vertex_id: String,
        /// Which side performed the deletion.
        deleted_by: Side,
    },

    // --- Required ---
    /// Both branches changed the same vertex's required edges differently.
    BothModifiedRequired {
        /// The ID of the contested vertex.
        vertex_id: String,
    },

    /// One branch removed required edges that the other modified.
    DeleteModifyRequired {
        /// The ID of the contested vertex.
        vertex_id: String,
        /// Which side performed the deletion.
        deleted_by: Side,
    },

    // --- Nominal ---
    /// Both branches changed the same vertex's nominal flag differently.
    BothModifiedNominal {
        /// The ID of the contested vertex.
        vertex_id: String,
        /// The nominal value our branch set.
        ours_value: bool,
        /// The nominal value their branch set.
        theirs_value: bool,
    },

    // --- Spans ---
    /// Both branches modified the same span differently.
    BothModifiedSpan {
        /// The ID of the contested span.
        span_id: String,
    },

    /// One branch removed a span that the other modified.
    DeleteModifySpan {
        /// The ID of the contested span.
        span_id: String,
        /// Which side performed the deletion.
        deleted_by: Side,
    },

    /// Both branches added a span with the same ID but different content.
    BothAddedSpanDifferently {
        /// The ID of the contested span.
        span_id: String,
    },
}

/// Which side of the merge performed an operation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Side {
    /// Our branch.
    Ours,
    /// Their branch.
    Theirs,
}

// ===========================================================================
// Core pushout merge
// ===========================================================================

/// Perform a three-way merge of schemas via pushout construction.
///
/// # Algorithm
///
/// 1. Compute `diff(base, ours)` and `diff(base, theirs)`.
/// 2. For every schema field, classify each element's fate on each
///    side (unchanged / added / removed / modified).
/// 3. Apply the pushout rule: one-sided changes are accepted;
///    identical changes are deduplicated; incompatible changes are
///    conflicts (base value retained).
/// 4. Rebuild precomputed indices.
/// 5. Derive migrations from ours → merged and theirs → merged.
///
/// The merge is **commutative**: swapping ours and theirs produces an
/// identical `merged_schema` (with `Side` labels swapped in conflicts).
#[must_use]
pub fn three_way_merge(base: &Schema, ours: &Schema, theirs: &Schema) -> MergeResult {
    let diff_ours = diff::diff(base, ours);
    let diff_theirs = diff::diff(base, theirs);
    let mut conflicts = Vec::new();

    // -- Vertices --
    let vertices = merge_vertices(base, ours, theirs, &diff_ours, &diff_theirs, &mut conflicts);

    // -- Edges --
    let edges = merge_edges(base, ours, theirs, &diff_ours, &diff_theirs, &mut conflicts);

    // -- Constraints --
    let constraints =
        merge_constraints(base, ours, theirs, &diff_ours, &diff_theirs, &mut conflicts);

    // -- Hyper-edges --
    let hyper_edges = merge_keyed_eq(
        &base.hyper_edges,
        &ours.hyper_edges,
        &theirs.hyper_edges,
        &fxset_from_iter(diff_ours.added_hyper_edges.iter()),
        &fxset_from_iter(diff_ours.removed_hyper_edges.iter()),
        &fxset_from_iter(diff_ours.modified_hyper_edges.iter().map(|c| &c.id)),
        &fxset_from_iter(diff_theirs.added_hyper_edges.iter()),
        &fxset_from_iter(diff_theirs.removed_hyper_edges.iter()),
        &fxset_from_iter(diff_theirs.modified_hyper_edges.iter().map(|c| &c.id)),
        &mut conflicts,
        |k, case| match case {
            ConflictCase::BothAddedDifferently => MergeConflict::BothAddedHyperEdgeDifferently {
                hyper_edge_id: k.clone(),
            },
            ConflictCase::BothModifiedDifferently => MergeConflict::BothModifiedHyperEdge {
                hyper_edge_id: k.clone(),
            },
            ConflictCase::DeleteModify(side) => MergeConflict::DeleteModifyHyperEdge {
                hyper_edge_id: k.clone(),
                deleted_by: side,
            },
        },
    );

    // -- Required --
    let required = merge_required(base, ours, theirs, &diff_ours, &diff_theirs, &mut conflicts);

    // -- NSIDs --
    let nsids = merge_nsids(base, ours, theirs, &diff_ours, &diff_theirs, &mut conflicts);

    // -- Variants --
    let variants = merge_variants(base, ours, theirs, &diff_ours, &diff_theirs, &mut conflicts);

    // -- Orderings --
    let orderings = merge_orderings(base, ours, theirs, &diff_ours, &diff_theirs, &mut conflicts);

    // -- Recursion points --
    let recursion_points =
        merge_recursion_points(base, ours, theirs, &diff_ours, &diff_theirs, &mut conflicts);

    // -- Usage modes --
    let usage_modes =
        merge_usage_modes(base, ours, theirs, &diff_ours, &diff_theirs, &mut conflicts);

    // -- Spans --
    let spans = merge_spans(base, ours, theirs, &diff_ours, &diff_theirs, &mut conflicts);

    // -- Nominal --
    let nominal = merge_nominal(base, ours, theirs, &diff_ours, &diff_theirs, &mut conflicts);

    // Rebuild precomputed indices.
    let mut outgoing: HashMap<String, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<String, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(String, String), SmallVec<Edge, 2>> = HashMap::new();

    for edge in edges.keys() {
        outgoing
            .entry(edge.src.clone())
            .or_default()
            .push(edge.clone());
        incoming
            .entry(edge.tgt.clone())
            .or_default()
            .push(edge.clone());
        between
            .entry((edge.src.clone(), edge.tgt.clone()))
            .or_default()
            .push(edge.clone());
    }

    let merged_schema = Schema {
        protocol: base.protocol.clone(),
        vertices,
        edges,
        hyper_edges,
        constraints,
        required,
        nsids,
        variants,
        orderings,
        recursion_points,
        spans,
        usage_modes,
        nominal,
        outgoing,
        incoming,
        between,
    };

    // Derive migrations.
    let diff_ours_to_merged = diff::diff(ours, &merged_schema);
    let diff_theirs_to_merged = diff::diff(theirs, &merged_schema);
    let migration_from_ours =
        auto_mig::derive_migration(ours, &merged_schema, &diff_ours_to_merged);
    let migration_from_theirs =
        auto_mig::derive_migration(theirs, &merged_schema, &diff_theirs_to_merged);

    MergeResult {
        merged_schema,
        conflicts,
        migration_from_ours,
        migration_from_theirs,
    }
}

// ===========================================================================
// Generic pushout helper for keyed maps with PartialEq values
// ===========================================================================

enum ConflictCase {
    BothAddedDifferently,
    BothModifiedDifferently,
    DeleteModify(Side),
}

/// Generic pushout merge for `HashMap<String, V>` where values support `PartialEq`.
///
/// Given sets of added/removed/modified keys from each diff, applies the
/// nine-case pushout rule.
#[allow(clippy::too_many_arguments)]
fn merge_keyed_eq<V: Clone + PartialEq>(
    base: &HashMap<String, V>,
    ours: &HashMap<String, V>,
    theirs: &HashMap<String, V>,
    ours_added: &FxHashSet<&String>,
    ours_removed: &FxHashSet<&String>,
    ours_modified: &FxHashSet<&String>,
    theirs_added: &FxHashSet<&String>,
    theirs_removed: &FxHashSet<&String>,
    theirs_modified: &FxHashSet<&String>,
    conflicts: &mut Vec<MergeConflict>,
    make_conflict: impl Fn(&String, ConflictCase) -> MergeConflict,
) -> HashMap<String, V> {
    let mut result: HashMap<String, V> = HashMap::new();

    // All keys across all three schemas.
    let all_keys: FxHashSet<&String> = base
        .keys()
        .chain(ours.keys())
        .chain(theirs.keys())
        .collect();

    for key in all_keys {
        let in_base = base.contains_key(key);
        let in_ours = ours.contains_key(key);
        let in_theirs = theirs.contains_key(key);

        let ours_fate = element_fate(
            in_base,
            in_ours,
            ours_added.contains(key),
            ours_removed.contains(key),
            ours_modified.contains(key),
        );
        let theirs_fate = element_fate(
            in_base,
            in_theirs,
            theirs_added.contains(key),
            theirs_removed.contains(key),
            theirs_modified.contains(key),
        );

        match (ours_fate, theirs_fate) {
            (Fate::Unchanged, Fate::Unchanged) => {
                if let Some(v) = base.get(key) {
                    result.insert(key.clone(), v.clone());
                }
            }
            (Fate::Unchanged, Fate::Added | Fate::Modified) => {
                if let Some(v) = theirs.get(key) {
                    result.insert(key.clone(), v.clone());
                }
            }
            (Fate::Added | Fate::Modified, Fate::Unchanged) => {
                if let Some(v) = ours.get(key) {
                    result.insert(key.clone(), v.clone());
                }
            }
            (Fate::Unchanged | Fate::Removed, Fate::Removed) | (Fate::Removed, Fate::Unchanged) => {
                // Removed — don't include.
            }
            (Fate::Added, Fate::Added) => {
                // Both added — check if identical.
                let ours_val = ours.get(key);
                let theirs_val = theirs.get(key);
                if ours_val == theirs_val {
                    if let Some(v) = ours_val {
                        result.insert(key.clone(), v.clone());
                    }
                } else {
                    conflicts.push(make_conflict(key, ConflictCase::BothAddedDifferently));
                    // No base value; don't include.
                }
            }
            (Fate::Modified | Fate::Added, Fate::Modified) | (Fate::Modified, Fate::Added) => {
                let ours_val = ours.get(key);
                let theirs_val = theirs.get(key);
                if ours_val == theirs_val {
                    if let Some(v) = ours_val {
                        result.insert(key.clone(), v.clone());
                    }
                } else {
                    conflicts.push(make_conflict(key, ConflictCase::BothModifiedDifferently));
                    if let Some(v) = base.get(key) {
                        result.insert(key.clone(), v.clone());
                    }
                }
            }
            (Fate::Removed, Fate::Modified | Fate::Added) => {
                conflicts.push(make_conflict(key, ConflictCase::DeleteModify(Side::Ours)));
                if let Some(v) = base.get(key) {
                    result.insert(key.clone(), v.clone());
                }
            }
            (Fate::Modified | Fate::Added, Fate::Removed) => {
                conflicts.push(make_conflict(key, ConflictCase::DeleteModify(Side::Theirs)));
                if let Some(v) = base.get(key) {
                    result.insert(key.clone(), v.clone());
                }
            }
        }
    }

    result
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fate {
    Unchanged,
    Added,
    Removed,
    Modified,
}

#[allow(clippy::fn_params_excessive_bools)]
const fn element_fate(
    in_base: bool,
    in_schema: bool,
    is_added: bool,
    is_removed: bool,
    is_modified: bool,
) -> Fate {
    if is_added {
        return Fate::Added;
    }
    if is_removed {
        return Fate::Removed;
    }
    if is_modified {
        return Fate::Modified;
    }
    if !in_base && in_schema {
        return Fate::Added;
    }
    if in_base && !in_schema {
        return Fate::Removed;
    }
    Fate::Unchanged
}

fn fxset_from_iter<'a, I: Iterator<Item = &'a String>>(iter: I) -> FxHashSet<&'a String> {
    iter.collect()
}

// ===========================================================================
// Per-field merge implementations
// ===========================================================================

#[allow(clippy::too_many_lines)]
fn merge_vertices(
    base: &Schema,
    ours: &Schema,
    theirs: &Schema,
    diff_ours: &SchemaDiff,
    diff_theirs: &SchemaDiff,
    conflicts: &mut Vec<MergeConflict>,
) -> HashMap<String, Vertex> {
    let mut result: HashMap<String, Vertex> = HashMap::new();

    let ours_added: FxHashSet<&str> = diff_ours
        .added_vertices
        .iter()
        .map(String::as_str)
        .collect();
    let theirs_added: FxHashSet<&str> = diff_theirs
        .added_vertices
        .iter()
        .map(String::as_str)
        .collect();
    let ours_removed: FxHashSet<&str> = diff_ours
        .removed_vertices
        .iter()
        .map(String::as_str)
        .collect();
    let theirs_removed: FxHashSet<&str> = diff_theirs
        .removed_vertices
        .iter()
        .map(String::as_str)
        .collect();
    let ours_kind_changed: FxHashSet<&str> = diff_ours
        .kind_changes
        .iter()
        .map(|kc| kc.vertex_id.as_str())
        .collect();
    let theirs_kind_changed: FxHashSet<&str> = diff_theirs
        .kind_changes
        .iter()
        .map(|kc| kc.vertex_id.as_str())
        .collect();

    // Process base vertices.
    for (vid, base_v) in &base.vertices {
        let o_removed = ours_removed.contains(vid.as_str());
        let t_removed = theirs_removed.contains(vid.as_str());
        let o_modified = ours_kind_changed.contains(vid.as_str());
        let t_modified = theirs_kind_changed.contains(vid.as_str());

        match (o_removed, t_removed, o_modified, t_modified) {
            // Both removed, or one removed while the other unchanged.
            (true, true, _, _) | (true, false, _, false) | (false, true, false, _) => {}
            // Ours removed, theirs modified → conflict.
            (true, false, _, true) => {
                conflicts.push(MergeConflict::DeleteModifyVertex {
                    vertex_id: vid.clone(),
                    deleted_by: Side::Ours,
                });
                result.insert(vid.clone(), base_v.clone());
            }
            // Theirs removed, ours modified → conflict.
            (false, true, true, _) => {
                conflicts.push(MergeConflict::DeleteModifyVertex {
                    vertex_id: vid.clone(),
                    deleted_by: Side::Theirs,
                });
                result.insert(vid.clone(), base_v.clone());
            }
            // Both modified.
            (false, false, true, true) => {
                let ours_v = &ours.vertices[vid];
                let theirs_v = &theirs.vertices[vid];
                if ours_v.kind == theirs_v.kind {
                    result.insert(vid.clone(), ours_v.clone());
                } else {
                    conflicts.push(MergeConflict::BothModifiedVertex {
                        vertex_id: vid.clone(),
                        ours_kind: ours_v.kind.clone(),
                        theirs_kind: theirs_v.kind.clone(),
                    });
                    result.insert(vid.clone(), base_v.clone());
                }
            }
            // Only ours modified.
            (false, false, true, false) => {
                result.insert(vid.clone(), ours.vertices[vid].clone());
            }
            // Only theirs modified.
            (false, false, false, true) => {
                result.insert(vid.clone(), theirs.vertices[vid].clone());
            }
            // Neither modified nor removed.
            (false, false, false, false) => {
                result.insert(vid.clone(), base_v.clone());
            }
        }
    }

    // Process vertices added by ours.
    for vid in &diff_ours.added_vertices {
        if theirs_added.contains(vid.as_str()) {
            // Both added — check if identical.
            let ours_v = &ours.vertices[vid];
            let theirs_v = &theirs.vertices[vid];
            if ours_v == theirs_v {
                result.insert(vid.clone(), ours_v.clone());
            } else {
                conflicts.push(MergeConflict::BothAddedVertexDifferently {
                    vertex_id: vid.clone(),
                    ours_kind: ours_v.kind.clone(),
                    theirs_kind: theirs_v.kind.clone(),
                });
                // No base value; don't include.
            }
        } else {
            result.insert(vid.clone(), ours.vertices[vid].clone());
        }
    }

    // Process vertices added by theirs only.
    for vid in &diff_theirs.added_vertices {
        if !ours_added.contains(vid.as_str()) {
            result.insert(vid.clone(), theirs.vertices[vid].clone());
        }
    }

    result
}

fn merge_edges(
    base: &Schema,
    _ours: &Schema,
    _theirs: &Schema,
    diff_ours: &SchemaDiff,
    diff_theirs: &SchemaDiff,
    _conflicts: &mut Vec<MergeConflict>,
) -> HashMap<Edge, String> {
    let mut result: HashMap<Edge, String> = HashMap::new();

    let ours_removed: FxHashSet<&Edge> = diff_ours.removed_edges.iter().collect();
    let theirs_removed: FxHashSet<&Edge> = diff_theirs.removed_edges.iter().collect();

    // Base edges.
    for (edge, kind) in &base.edges {
        let o_removed = ours_removed.contains(edge);
        let t_removed = theirs_removed.contains(edge);

        match (o_removed, t_removed) {
            // Any removal means the edge is gone.
            (true, _) | (_, true) => {}
            (false, false) => {
                result.insert(edge.clone(), kind.clone());
            }
        }
    }

    // Edges added by ours.
    for edge in &diff_ours.added_edges {
        result
            .entry(edge.clone())
            .or_insert_with(|| edge.kind.clone());
    }
    // Edges added by theirs.
    for edge in &diff_theirs.added_edges {
        result
            .entry(edge.clone())
            .or_insert_with(|| edge.kind.clone());
    }

    result
}

fn merge_constraints(
    base: &Schema,
    ours: &Schema,
    theirs: &Schema,
    diff_ours: &SchemaDiff,
    diff_theirs: &SchemaDiff,
    conflicts: &mut Vec<MergeConflict>,
) -> HashMap<String, Vec<Constraint>> {
    let mut result: HashMap<String, Vec<Constraint>> = HashMap::new();

    let all_vids: FxHashSet<&String> = base
        .constraints
        .keys()
        .chain(ours.constraints.keys())
        .chain(theirs.constraints.keys())
        .collect();

    for vid in all_vids {
        let base_cs = base.constraints.get(vid).cloned().unwrap_or_default();
        let ours_cs = ours.constraints.get(vid).cloned().unwrap_or_default();
        let theirs_cs = theirs.constraints.get(vid).cloned().unwrap_or_default();

        let ours_cdiff = diff_ours.modified_constraints.get(vid);
        let theirs_cdiff = diff_theirs.modified_constraints.get(vid);

        match (ours_cdiff, theirs_cdiff) {
            (None, None) => {
                // Neither side changed constraints on this vertex.
                if !base_cs.is_empty() {
                    result.insert(vid.clone(), base_cs);
                }
            }
            (Some(_), None) => {
                // Only ours changed — take ours.
                if !ours_cs.is_empty() {
                    result.insert(vid.clone(), ours_cs);
                }
            }
            (None, Some(_)) => {
                // Only theirs changed — take theirs.
                if !theirs_cs.is_empty() {
                    result.insert(vid.clone(), theirs_cs);
                }
            }
            (Some(od), Some(td)) => {
                // Both changed — merge per-sort.
                let merged_cs =
                    merge_constraint_sorts(vid, &base_cs, &ours_cs, &theirs_cs, od, td, conflicts);
                if !merged_cs.is_empty() {
                    result.insert(vid.clone(), merged_cs);
                }
            }
        }
    }

    result
}

/// Merge constraints per-sort when both sides changed constraints on the same vertex.
#[allow(clippy::too_many_lines)]
fn merge_constraint_sorts(
    vid: &str,
    base_cs: &[Constraint],
    ours_cs: &[Constraint],
    theirs_cs: &[Constraint],
    ours_cdiff: &panproto_check::diff::ConstraintDiff,
    theirs_cdiff: &panproto_check::diff::ConstraintDiff,
    conflicts: &mut Vec<MergeConflict>,
) -> Vec<Constraint> {
    let base_by_sort: HashMap<&str, &Constraint> =
        base_cs.iter().map(|c| (c.sort.as_str(), c)).collect();
    let ours_by_sort: HashMap<&str, &Constraint> =
        ours_cs.iter().map(|c| (c.sort.as_str(), c)).collect();
    let theirs_by_sort: HashMap<&str, &Constraint> =
        theirs_cs.iter().map(|c| (c.sort.as_str(), c)).collect();

    // Build change lookups.
    let ours_added_sorts: FxHashSet<&str> =
        ours_cdiff.added.iter().map(|c| c.sort.as_str()).collect();
    let ours_removed_sorts: FxHashSet<&str> =
        ours_cdiff.removed.iter().map(|c| c.sort.as_str()).collect();
    let ours_changed_sorts: FxHashSet<&str> =
        ours_cdiff.changed.iter().map(|c| c.sort.as_str()).collect();
    let theirs_added_sorts: FxHashSet<&str> =
        theirs_cdiff.added.iter().map(|c| c.sort.as_str()).collect();
    let theirs_removed_sorts: FxHashSet<&str> = theirs_cdiff
        .removed
        .iter()
        .map(|c| c.sort.as_str())
        .collect();
    let theirs_changed_sorts: FxHashSet<&str> = theirs_cdiff
        .changed
        .iter()
        .map(|c| c.sort.as_str())
        .collect();

    // Collect all sorts.
    let all_sorts: FxHashSet<&str> = base_by_sort
        .keys()
        .copied()
        .chain(ours_by_sort.keys().copied())
        .chain(theirs_by_sort.keys().copied())
        .collect();

    let mut merged = Vec::new();

    for sort in all_sorts {
        let in_base = base_by_sort.contains_key(sort);
        let o_added = ours_added_sorts.contains(sort);
        let o_removed = ours_removed_sorts.contains(sort);
        let o_changed = ours_changed_sorts.contains(sort);
        let t_added = theirs_added_sorts.contains(sort);
        let t_removed = theirs_removed_sorts.contains(sort);
        let t_changed = theirs_changed_sorts.contains(sort);

        let o_fate = constraint_fate(in_base, o_added, o_removed, o_changed);
        let t_fate = constraint_fate(in_base, t_added, t_removed, t_changed);

        match (o_fate, t_fate) {
            (Fate::Unchanged, Fate::Added | Fate::Modified) => {
                if let Some(c) = theirs_by_sort.get(sort) {
                    merged.push((*c).clone());
                }
            }
            (Fate::Added | Fate::Modified, Fate::Unchanged) => {
                if let Some(c) = ours_by_sort.get(sort) {
                    merged.push((*c).clone());
                }
            }
            (Fate::Unchanged | Fate::Removed, Fate::Removed) | (Fate::Removed, Fate::Unchanged) => {
                // Removed.
            }
            (Fate::Added, Fate::Added) => {
                let ov = ours_by_sort.get(sort);
                let tv = theirs_by_sort.get(sort);
                if ov == tv {
                    if let Some(c) = ov {
                        merged.push((*c).clone());
                    }
                } else {
                    conflicts.push(MergeConflict::BothAddedConstraintDifferently {
                        vertex_id: vid.to_string(),
                        sort: sort.to_string(),
                        ours_value: ov.map(|c| c.value.clone()).unwrap_or_default(),
                        theirs_value: tv.map(|c| c.value.clone()).unwrap_or_default(),
                    });
                }
            }
            (Fate::Modified, Fate::Modified) => {
                let ov = ours_by_sort.get(sort);
                let tv = theirs_by_sort.get(sort);
                if ov == tv {
                    if let Some(c) = ov {
                        merged.push((*c).clone());
                    }
                } else {
                    conflicts.push(MergeConflict::BothModifiedConstraint {
                        vertex_id: vid.to_string(),
                        sort: sort.to_string(),
                        ours_value: ov.map(|c| c.value.clone()).unwrap_or_default(),
                        theirs_value: tv.map(|c| c.value.clone()).unwrap_or_default(),
                    });
                    if let Some(c) = base_by_sort.get(sort) {
                        merged.push((*c).clone());
                    }
                }
            }
            (Fate::Removed, Fate::Modified | Fate::Added) => {
                conflicts.push(MergeConflict::DeleteModifyConstraint {
                    vertex_id: vid.to_string(),
                    sort: sort.to_string(),
                    deleted_by: Side::Ours,
                });
                if let Some(c) = base_by_sort.get(sort) {
                    merged.push((*c).clone());
                }
            }
            (Fate::Modified | Fate::Added, Fate::Removed) => {
                conflicts.push(MergeConflict::DeleteModifyConstraint {
                    vertex_id: vid.to_string(),
                    sort: sort.to_string(),
                    deleted_by: Side::Theirs,
                });
                if let Some(c) = base_by_sort.get(sort) {
                    merged.push((*c).clone());
                }
            }
            // Unchanged on both sides, or impossible combos — retain base.
            _ => {
                if let Some(c) = base_by_sort.get(sort) {
                    merged.push((*c).clone());
                }
            }
        }
    }

    merged.sort();
    merged
}

#[allow(clippy::fn_params_excessive_bools)]
const fn constraint_fate(in_base: bool, added: bool, removed: bool, changed: bool) -> Fate {
    if added {
        return Fate::Added;
    }
    if removed {
        return Fate::Removed;
    }
    if changed {
        return Fate::Modified;
    }
    if !in_base {
        return Fate::Added;
    }
    Fate::Unchanged
}

fn merge_required(
    base: &Schema,
    ours: &Schema,
    theirs: &Schema,
    diff_ours: &SchemaDiff,
    diff_theirs: &SchemaDiff,
    conflicts: &mut Vec<MergeConflict>,
) -> HashMap<String, Vec<Edge>> {
    let all_vids: FxHashSet<&String> = base
        .required
        .keys()
        .chain(ours.required.keys())
        .chain(theirs.required.keys())
        .collect();

    let mut result = HashMap::new();

    for vid in all_vids {
        let base_val = base.required.get(vid);
        let ours_val = ours.required.get(vid);
        let theirs_val = theirs.required.get(vid);

        let o_changed = diff_ours.added_required.contains_key(vid)
            || diff_ours.removed_required.contains_key(vid);
        let t_changed = diff_theirs.added_required.contains_key(vid)
            || diff_theirs.removed_required.contains_key(vid);

        let merged_val = match (o_changed, t_changed) {
            (false, false) => base_val.cloned(),
            (true, false) => ours_val.cloned(),
            (false, true) => theirs_val.cloned(),
            (true, true) => {
                if ours_val == theirs_val {
                    ours_val.cloned()
                } else {
                    conflicts.push(MergeConflict::BothModifiedRequired {
                        vertex_id: vid.clone(),
                    });
                    base_val.cloned()
                }
            }
        };

        if let Some(v) = merged_val {
            if !v.is_empty() {
                result.insert(vid.clone(), v);
            }
        }
    }

    result
}

#[allow(clippy::too_many_lines)]
fn merge_variants(
    base: &Schema,
    ours: &Schema,
    theirs: &Schema,
    diff_ours: &SchemaDiff,
    diff_theirs: &SchemaDiff,
    conflicts: &mut Vec<MergeConflict>,
) -> HashMap<String, Vec<Variant>> {
    // Flatten to (parent, id) → Variant for each.
    let base_flat = flatten_variants(&base.variants);
    let ours_flat = flatten_variants(&ours.variants);
    let theirs_flat = flatten_variants(&theirs.variants);

    let ours_added: FxHashSet<(&str, &str)> = diff_ours
        .added_variants
        .iter()
        .map(|v| (v.parent_vertex.as_str(), v.id.as_str()))
        .collect();
    let ours_removed: FxHashSet<(&str, &str)> = diff_ours
        .removed_variants
        .iter()
        .map(|v| (v.parent_vertex.as_str(), v.id.as_str()))
        .collect();
    let ours_modified: FxHashSet<(&str, &str)> = diff_ours
        .modified_variants
        .iter()
        .map(|v| (v.parent_vertex.as_str(), v.id.as_str()))
        .collect();
    let theirs_added: FxHashSet<(&str, &str)> = diff_theirs
        .added_variants
        .iter()
        .map(|v| (v.parent_vertex.as_str(), v.id.as_str()))
        .collect();
    let theirs_removed: FxHashSet<(&str, &str)> = diff_theirs
        .removed_variants
        .iter()
        .map(|v| (v.parent_vertex.as_str(), v.id.as_str()))
        .collect();
    let theirs_modified: FxHashSet<(&str, &str)> = diff_theirs
        .modified_variants
        .iter()
        .map(|v| (v.parent_vertex.as_str(), v.id.as_str()))
        .collect();

    let all_keys: FxHashSet<(&str, &str)> = base_flat
        .keys()
        .copied()
        .chain(ours_flat.keys().copied())
        .chain(theirs_flat.keys().copied())
        .collect();

    let mut merged_flat: HashMap<(&str, &str), &Variant> = HashMap::new();

    for key in all_keys {
        let in_base = base_flat.contains_key(&key);
        let o_a = ours_added.contains(&key);
        let o_r = ours_removed.contains(&key);
        let o_m = ours_modified.contains(&key);
        let t_a = theirs_added.contains(&key);
        let t_r = theirs_removed.contains(&key);
        let t_m = theirs_modified.contains(&key);

        let o_fate = element_fate(in_base, ours_flat.contains_key(&key), o_a, o_r, o_m);
        let t_fate = element_fate(in_base, theirs_flat.contains_key(&key), t_a, t_r, t_m);

        match (o_fate, t_fate) {
            (Fate::Unchanged, Fate::Added | Fate::Modified) => {
                if let Some(v) = theirs_flat.get(&key) {
                    merged_flat.insert(key, v);
                }
            }
            (Fate::Added | Fate::Modified, Fate::Unchanged) => {
                if let Some(v) = ours_flat.get(&key) {
                    merged_flat.insert(key, v);
                }
            }
            (Fate::Unchanged | Fate::Removed, Fate::Removed) | (Fate::Removed, Fate::Unchanged) => {
            }
            (Fate::Added, Fate::Added) | (Fate::Modified, Fate::Modified) => {
                let ov = ours_flat.get(&key);
                let tv = theirs_flat.get(&key);
                if ov == tv {
                    if let Some(v) = ov {
                        merged_flat.insert(key, v);
                    }
                } else {
                    conflicts.push(MergeConflict::BothModifiedVariant {
                        variant_id: key.1.to_string(),
                        parent_vertex: key.0.to_string(),
                        ours_tag: ov.and_then(|v| v.tag.clone()),
                        theirs_tag: tv.and_then(|v| v.tag.clone()),
                    });
                    if let Some(v) = base_flat.get(&key) {
                        merged_flat.insert(key, v);
                    }
                }
            }
            (Fate::Removed, Fate::Modified | Fate::Added) => {
                conflicts.push(MergeConflict::DeleteModifyVariant {
                    variant_id: key.1.to_string(),
                    parent_vertex: key.0.to_string(),
                    deleted_by: Side::Ours,
                });
                if let Some(v) = base_flat.get(&key) {
                    merged_flat.insert(key, v);
                }
            }
            (Fate::Modified | Fate::Added, Fate::Removed) => {
                conflicts.push(MergeConflict::DeleteModifyVariant {
                    variant_id: key.1.to_string(),
                    parent_vertex: key.0.to_string(),
                    deleted_by: Side::Theirs,
                });
                if let Some(v) = base_flat.get(&key) {
                    merged_flat.insert(key, v);
                }
            }
            // Unchanged on both sides, or impossible combos — retain base.
            _ => {
                if let Some(v) = base_flat.get(&key) {
                    merged_flat.insert(key, v);
                }
            }
        }
    }

    // Unflatten.
    let mut result: HashMap<String, Vec<Variant>> = HashMap::new();
    for ((parent, _), variant) in merged_flat {
        result
            .entry(parent.to_string())
            .or_default()
            .push((*variant).clone());
    }
    result
}

fn flatten_variants(variants: &HashMap<String, Vec<Variant>>) -> HashMap<(&str, &str), &Variant> {
    let mut flat: HashMap<(&str, &str), &Variant> = HashMap::new();
    for (parent, vs) in variants {
        for v in vs {
            flat.insert((parent.as_str(), v.id.as_str()), v);
        }
    }
    flat
}

fn merge_orderings(
    base: &Schema,
    ours: &Schema,
    theirs: &Schema,
    diff_ours: &SchemaDiff,
    diff_theirs: &SchemaDiff,
    conflicts: &mut Vec<MergeConflict>,
) -> HashMap<Edge, u32> {
    let ours_changed: FxHashSet<&Edge> =
        diff_ours.order_changes.iter().map(|(e, _, _)| e).collect();
    let theirs_changed: FxHashSet<&Edge> = diff_theirs
        .order_changes
        .iter()
        .map(|(e, _, _)| e)
        .collect();

    let all_edges: FxHashSet<&Edge> = base
        .orderings
        .keys()
        .chain(ours.orderings.keys())
        .chain(theirs.orderings.keys())
        .collect();

    let mut result = HashMap::new();

    for edge in all_edges {
        let o_changed = ours_changed.contains(edge);
        let t_changed = theirs_changed.contains(edge);

        let base_pos = base.orderings.get(edge).copied();
        let ours_pos = ours.orderings.get(edge).copied();
        let theirs_pos = theirs.orderings.get(edge).copied();

        let merged_pos = match (o_changed, t_changed) {
            (false, false) => base_pos,
            (true, false) => ours_pos,
            (false, true) => theirs_pos,
            (true, true) => {
                if ours_pos == theirs_pos {
                    ours_pos
                } else {
                    conflicts.push(MergeConflict::BothModifiedOrdering {
                        edge: edge.clone(),
                        ours_position: ours_pos,
                        theirs_position: theirs_pos,
                    });
                    base_pos
                }
            }
        };

        if let Some(pos) = merged_pos {
            result.insert(edge.clone(), pos);
        }
    }

    result
}

fn merge_recursion_points(
    base: &Schema,
    ours: &Schema,
    theirs: &Schema,
    diff_ours: &SchemaDiff,
    diff_theirs: &SchemaDiff,
    conflicts: &mut Vec<MergeConflict>,
) -> HashMap<String, panproto_schema::RecursionPoint> {
    let ours_added: FxHashSet<&str> = diff_ours
        .added_recursion_points
        .iter()
        .map(|r| r.mu_id.as_str())
        .collect();
    let ours_removed: FxHashSet<&str> = diff_ours
        .removed_recursion_points
        .iter()
        .map(|r| r.mu_id.as_str())
        .collect();
    let ours_modified: FxHashSet<&str> = diff_ours
        .modified_recursion_points
        .iter()
        .map(|r| r.mu_id.as_str())
        .collect();
    let theirs_added: FxHashSet<&str> = diff_theirs
        .added_recursion_points
        .iter()
        .map(|r| r.mu_id.as_str())
        .collect();
    let theirs_removed: FxHashSet<&str> = diff_theirs
        .removed_recursion_points
        .iter()
        .map(|r| r.mu_id.as_str())
        .collect();
    let theirs_modified: FxHashSet<&str> = diff_theirs
        .modified_recursion_points
        .iter()
        .map(|r| r.mu_id.as_str())
        .collect();

    merge_keyed_eq(
        &base.recursion_points,
        &ours.recursion_points,
        &theirs.recursion_points,
        &ours_added
            .iter()
            .filter_map(|s| ours.recursion_points.get_key_value(*s).map(|(k, _)| k))
            .collect(),
        &ours_removed
            .iter()
            .filter_map(|s| base.recursion_points.get_key_value(*s).map(|(k, _)| k))
            .collect(),
        &ours_modified
            .iter()
            .filter_map(|s| base.recursion_points.get_key_value(*s).map(|(k, _)| k))
            .collect(),
        &theirs_added
            .iter()
            .filter_map(|s| theirs.recursion_points.get_key_value(*s).map(|(k, _)| k))
            .collect(),
        &theirs_removed
            .iter()
            .filter_map(|s| base.recursion_points.get_key_value(*s).map(|(k, _)| k))
            .collect(),
        &theirs_modified
            .iter()
            .filter_map(|s| base.recursion_points.get_key_value(*s).map(|(k, _)| k))
            .collect(),
        conflicts,
        |k, case| match case {
            ConflictCase::BothAddedDifferently | ConflictCase::BothModifiedDifferently => {
                let ours_rp = ours.recursion_points.get(k);
                let theirs_rp = theirs.recursion_points.get(k);
                MergeConflict::BothModifiedRecursionPoint {
                    mu_id: k.clone(),
                    ours_target: ours_rp.map(|r| r.target_vertex.clone()).unwrap_or_default(),
                    theirs_target: theirs_rp
                        .map(|r| r.target_vertex.clone())
                        .unwrap_or_default(),
                }
            }
            ConflictCase::DeleteModify(side) => MergeConflict::DeleteModifyRecursionPoint {
                mu_id: k.clone(),
                deleted_by: side,
            },
        },
    )
}

fn merge_usage_modes(
    base: &Schema,
    ours: &Schema,
    theirs: &Schema,
    diff_ours: &SchemaDiff,
    diff_theirs: &SchemaDiff,
    conflicts: &mut Vec<MergeConflict>,
) -> HashMap<Edge, UsageMode> {
    let ours_changed: FxHashSet<&Edge> = diff_ours
        .usage_mode_changes
        .iter()
        .map(|(e, _, _)| e)
        .collect();
    let theirs_changed: FxHashSet<&Edge> = diff_theirs
        .usage_mode_changes
        .iter()
        .map(|(e, _, _)| e)
        .collect();

    let all_edges: FxHashSet<&Edge> = base
        .usage_modes
        .keys()
        .chain(ours.usage_modes.keys())
        .chain(theirs.usage_modes.keys())
        .collect();

    let mut result = HashMap::new();

    for edge in all_edges {
        let o_changed = ours_changed.contains(edge);
        let t_changed = theirs_changed.contains(edge);

        let base_mode = base.usage_modes.get(edge).cloned();
        let ours_mode = ours.usage_modes.get(edge).cloned();
        let theirs_mode = theirs.usage_modes.get(edge).cloned();

        let merged_mode = match (o_changed, t_changed) {
            (false, false) => base_mode,
            (true, false) => ours_mode,
            (false, true) => theirs_mode,
            (true, true) => {
                if ours_mode == theirs_mode {
                    ours_mode
                } else {
                    conflicts.push(MergeConflict::BothModifiedUsageMode {
                        edge: edge.clone(),
                        ours_mode: ours_mode.clone().unwrap_or_default(),
                        theirs_mode: theirs_mode.clone().unwrap_or_default(),
                    });
                    base_mode
                }
            }
        };

        if let Some(mode) = merged_mode {
            result.insert(edge.clone(), mode);
        }
    }

    result
}

fn merge_spans(
    base: &Schema,
    ours: &Schema,
    theirs: &Schema,
    diff_ours: &SchemaDiff,
    diff_theirs: &SchemaDiff,
    conflicts: &mut Vec<MergeConflict>,
) -> HashMap<String, Span> {
    let ours_added: FxHashSet<&String> = fxset_from_iter(diff_ours.added_spans.iter());
    let ours_removed: FxHashSet<&String> = fxset_from_iter(diff_ours.removed_spans.iter());
    let ours_modified: FxHashSet<&String> =
        fxset_from_iter(diff_ours.modified_spans.iter().map(|s| &s.id));
    let theirs_added: FxHashSet<&String> = fxset_from_iter(diff_theirs.added_spans.iter());
    let theirs_removed: FxHashSet<&String> = fxset_from_iter(diff_theirs.removed_spans.iter());
    let theirs_modified: FxHashSet<&String> =
        fxset_from_iter(diff_theirs.modified_spans.iter().map(|s| &s.id));

    merge_keyed_eq(
        &base.spans,
        &ours.spans,
        &theirs.spans,
        &ours_added,
        &ours_removed,
        &ours_modified,
        &theirs_added,
        &theirs_removed,
        &theirs_modified,
        conflicts,
        |k, case| match case {
            ConflictCase::BothAddedDifferently => {
                MergeConflict::BothAddedSpanDifferently { span_id: k.clone() }
            }
            ConflictCase::BothModifiedDifferently => {
                MergeConflict::BothModifiedSpan { span_id: k.clone() }
            }
            ConflictCase::DeleteModify(side) => MergeConflict::DeleteModifySpan {
                span_id: k.clone(),
                deleted_by: side,
            },
        },
    )
}

fn merge_nominal(
    base: &Schema,
    ours: &Schema,
    theirs: &Schema,
    diff_ours: &SchemaDiff,
    diff_theirs: &SchemaDiff,
    conflicts: &mut Vec<MergeConflict>,
) -> HashMap<String, bool> {
    let ours_changed: FxHashSet<&str> = diff_ours
        .nominal_changes
        .iter()
        .map(|(v, _, _)| v.as_str())
        .collect();
    let theirs_changed: FxHashSet<&str> = diff_theirs
        .nominal_changes
        .iter()
        .map(|(v, _, _)| v.as_str())
        .collect();

    let all_vids: FxHashSet<&String> = base
        .nominal
        .keys()
        .chain(ours.nominal.keys())
        .chain(theirs.nominal.keys())
        .collect();

    let mut result = HashMap::new();

    for vid in all_vids {
        let o_changed = ours_changed.contains(vid.as_str());
        let t_changed = theirs_changed.contains(vid.as_str());

        let base_val = base.nominal.get(vid).copied();
        let ours_val = ours.nominal.get(vid).copied();
        let theirs_val = theirs.nominal.get(vid).copied();

        let merged_val = match (o_changed, t_changed) {
            (false, false) => base_val,
            (true, false) => ours_val,
            (false, true) => theirs_val,
            (true, true) => {
                if ours_val == theirs_val {
                    ours_val
                } else {
                    conflicts.push(MergeConflict::BothModifiedNominal {
                        vertex_id: vid.clone(),
                        ours_value: ours_val.unwrap_or(false),
                        theirs_value: theirs_val.unwrap_or(false),
                    });
                    base_val
                }
            }
        };

        if let Some(val) = merged_val {
            result.insert(vid.clone(), val);
        }
    }

    result
}

#[allow(clippy::too_many_lines)]
fn merge_nsids(
    base: &Schema,
    ours: &Schema,
    theirs: &Schema,
    diff_ours: &SchemaDiff,
    diff_theirs: &SchemaDiff,
    conflicts: &mut Vec<MergeConflict>,
) -> HashMap<String, String> {
    let ours_changed: FxHashSet<&str> = diff_ours
        .changed_nsids
        .iter()
        .map(|(v, _, _)| v.as_str())
        .collect();
    let ours_added: FxHashSet<&str> = diff_ours.added_nsids.keys().map(String::as_str).collect();
    let ours_removed: FxHashSet<&str> =
        diff_ours.removed_nsids.iter().map(String::as_str).collect();
    let theirs_changed: FxHashSet<&str> = diff_theirs
        .changed_nsids
        .iter()
        .map(|(v, _, _)| v.as_str())
        .collect();
    let theirs_added: FxHashSet<&str> =
        diff_theirs.added_nsids.keys().map(String::as_str).collect();
    let theirs_removed: FxHashSet<&str> = diff_theirs
        .removed_nsids
        .iter()
        .map(String::as_str)
        .collect();

    let all_vids: FxHashSet<&String> = base
        .nsids
        .keys()
        .chain(ours.nsids.keys())
        .chain(theirs.nsids.keys())
        .collect();

    let mut result = HashMap::new();

    for vid in all_vids {
        let in_base = base.nsids.contains_key(vid);
        let o_a = ours_added.contains(vid.as_str());
        let o_r = ours_removed.contains(vid.as_str());
        let o_m = ours_changed.contains(vid.as_str());
        let t_a = theirs_added.contains(vid.as_str());
        let t_r = theirs_removed.contains(vid.as_str());
        let t_m = theirs_changed.contains(vid.as_str());

        let o_fate = if o_a {
            Fate::Added
        } else if o_r {
            Fate::Removed
        } else if o_m {
            Fate::Modified
        } else if !in_base && ours.nsids.contains_key(vid) {
            Fate::Added
        } else if in_base && !ours.nsids.contains_key(vid) {
            Fate::Removed
        } else {
            Fate::Unchanged
        };
        let t_fate = if t_a {
            Fate::Added
        } else if t_r {
            Fate::Removed
        } else if t_m {
            Fate::Modified
        } else if !in_base && theirs.nsids.contains_key(vid) {
            Fate::Added
        } else if in_base && !theirs.nsids.contains_key(vid) {
            Fate::Removed
        } else {
            Fate::Unchanged
        };

        match (o_fate, t_fate) {
            (Fate::Unchanged, Fate::Added | Fate::Modified) => {
                if let Some(v) = theirs.nsids.get(vid) {
                    result.insert(vid.clone(), v.clone());
                }
            }
            (Fate::Added | Fate::Modified, Fate::Unchanged) => {
                if let Some(v) = ours.nsids.get(vid) {
                    result.insert(vid.clone(), v.clone());
                }
            }
            (Fate::Unchanged | Fate::Removed, Fate::Removed) | (Fate::Removed, Fate::Unchanged) => {
            }
            (Fate::Added, Fate::Added) | (Fate::Modified, Fate::Modified) => {
                let ov = ours.nsids.get(vid);
                let tv = theirs.nsids.get(vid);
                if ov == tv {
                    if let Some(v) = ov {
                        result.insert(vid.clone(), v.clone());
                    }
                } else {
                    conflicts.push(MergeConflict::BothModifiedNsid {
                        vertex_id: vid.clone(),
                        ours_nsid: ov.cloned().unwrap_or_default(),
                        theirs_nsid: tv.cloned().unwrap_or_default(),
                    });
                    if let Some(v) = base.nsids.get(vid) {
                        result.insert(vid.clone(), v.clone());
                    }
                }
            }
            (Fate::Removed, Fate::Modified | Fate::Added) => {
                conflicts.push(MergeConflict::DeleteModifyNsid {
                    vertex_id: vid.clone(),
                    deleted_by: Side::Ours,
                });
                if let Some(v) = base.nsids.get(vid) {
                    result.insert(vid.clone(), v.clone());
                }
            }
            (Fate::Modified | Fate::Added, Fate::Removed) => {
                conflicts.push(MergeConflict::DeleteModifyNsid {
                    vertex_id: vid.clone(),
                    deleted_by: Side::Theirs,
                });
                if let Some(v) = base.nsids.get(vid) {
                    result.insert(vid.clone(), v.clone());
                }
            }
            // Unchanged on both sides, or impossible combos — retain base.
            _ => {
                if let Some(v) = base.nsids.get(vid) {
                    result.insert(vid.clone(), v.clone());
                }
            }
        }
    }

    result
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use panproto_schema::{HyperEdge, Variant};

    fn make_schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
        let mut vert_map = HashMap::new();
        let mut edge_map = HashMap::new();

        for (id, kind) in vertices {
            vert_map.insert(
                id.to_string(),
                Vertex {
                    id: id.to_string(),
                    kind: kind.to_string(),
                    nsid: None,
                },
            );
        }
        for edge in edges {
            edge_map.insert(edge.clone(), edge.kind.clone());
        }

        Schema {
            protocol: "test".into(),
            vertices: vert_map,
            edges: edge_map,
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        }
    }

    fn with_ext(base: Schema, f: impl FnOnce(&mut Schema)) -> Schema {
        let mut s = base;
        f(&mut s);
        s
    }

    // =======================================================================
    // A. Pushout property tests
    // =======================================================================

    #[test]
    fn commutativity_no_conflicts() -> Result<(), Box<dyn std::error::Error>> {
        let base = make_schema(&[("a", "object")], &[]);
        let ours = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let theirs = make_schema(&[("a", "object"), ("c", "integer")], &[]);

        let r1 = three_way_merge(&base, &ours, &theirs);
        let r2 = three_way_merge(&base, &theirs, &ours);

        assert_eq!(
            r1.merged_schema.vertices.len(),
            r2.merged_schema.vertices.len()
        );
        for (id, v1) in &r1.merged_schema.vertices {
            let v2 = r2
                .merged_schema
                .vertices
                .get(id)
                .ok_or("vertex missing in swapped merge")?;
            assert_eq!(v1, v2, "vertex {id} differs between merge directions");
        }
        assert!(r1.conflicts.is_empty());
        assert!(r2.conflicts.is_empty());
        Ok(())
    }

    #[test]
    fn commutativity_with_conflicts() {
        let base = make_schema(&[("a", "object")], &[]);
        let ours = make_schema(&[("a", "string")], &[]);
        let theirs = make_schema(&[("a", "integer")], &[]);

        let r1 = three_way_merge(&base, &ours, &theirs);
        let r2 = three_way_merge(&base, &theirs, &ours);

        // Merged schemas must be identical (base value for conflicted element).
        assert_eq!(r1.merged_schema.vertices["a"].kind, "object");
        assert_eq!(r2.merged_schema.vertices["a"].kind, "object");
        assert_eq!(r1.conflicts.len(), r2.conflicts.len());
    }

    #[test]
    fn left_identity() {
        let base = make_schema(&[("a", "object")], &[]);
        let x = make_schema(&[("a", "object"), ("b", "string")], &[]);

        let r = three_way_merge(&base, &base, &x);
        assert!(r.conflicts.is_empty());
        assert_eq!(r.merged_schema.vertices.len(), 2);
        assert!(r.merged_schema.vertices.contains_key("b"));
    }

    #[test]
    fn right_identity() {
        let base = make_schema(&[("a", "object")], &[]);
        let x = make_schema(&[("a", "object"), ("b", "string")], &[]);

        let r = three_way_merge(&base, &x, &base);
        assert!(r.conflicts.is_empty());
        assert_eq!(r.merged_schema.vertices.len(), 2);
        assert!(r.merged_schema.vertices.contains_key("b"));
    }

    #[test]
    fn idempotence() {
        let base = make_schema(&[("a", "object")], &[]);
        let x = make_schema(&[("a", "object"), ("b", "string")], &[]);

        let r = three_way_merge(&base, &x, &x);
        assert!(r.conflicts.is_empty());
        assert_eq!(r.merged_schema.vertices.len(), 2);
    }

    // =======================================================================
    // B. Vertex tests
    // =======================================================================

    #[test]
    fn merge_non_overlapping_additions() {
        let base = make_schema(&[("a", "object")], &[]);
        let ours = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let theirs = make_schema(&[("a", "object"), ("c", "integer")], &[]);

        let result = three_way_merge(&base, &ours, &theirs);
        assert!(result.conflicts.is_empty());
        assert_eq!(result.merged_schema.vertices.len(), 3);
    }

    #[test]
    fn merge_same_addition() {
        let base = make_schema(&[("a", "object")], &[]);
        let ours = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let theirs = make_schema(&[("a", "object"), ("b", "string")], &[]);

        let result = three_way_merge(&base, &ours, &theirs);
        assert!(result.conflicts.is_empty());
        assert_eq!(result.merged_schema.vertices.len(), 2);
    }

    #[test]
    fn merge_both_modify_same_vertex_kind_differently() {
        let base = make_schema(&[("a", "object")], &[]);
        let ours = make_schema(&[("a", "string")], &[]);
        let theirs = make_schema(&[("a", "integer")], &[]);

        let result = three_way_merge(&base, &ours, &theirs);
        assert_eq!(result.conflicts.len(), 1);
        assert!(matches!(
            &result.conflicts[0],
            MergeConflict::BothModifiedVertex { .. }
        ));
        // Base value retained — not ours.
        assert_eq!(result.merged_schema.vertices["a"].kind, "object");
    }

    #[test]
    fn merge_both_modify_same_vertex_kind_same_way() {
        let base = make_schema(&[("a", "object")], &[]);
        let ours = make_schema(&[("a", "string")], &[]);
        let theirs = make_schema(&[("a", "string")], &[]);

        let result = three_way_merge(&base, &ours, &theirs);
        assert!(result.conflicts.is_empty());
        assert_eq!(result.merged_schema.vertices["a"].kind, "string");
    }

    #[test]
    fn merge_both_add_vertex_differently() {
        let base = make_schema(&[("a", "object")], &[]);
        let ours = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let theirs = make_schema(&[("a", "object"), ("b", "integer")], &[]);

        let result = three_way_merge(&base, &ours, &theirs);
        assert_eq!(result.conflicts.len(), 1);
        assert!(matches!(
            &result.conflicts[0],
            MergeConflict::BothAddedVertexDifferently { .. }
        ));
        // Not in merged (no base value).
        assert!(!result.merged_schema.vertices.contains_key("b"));
    }

    // =======================================================================
    // C. Bug regression: false DeleteModifyVertex
    // =======================================================================

    #[test]
    fn regression_one_side_removes_vertex_other_unchanged() {
        let base = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let ours = make_schema(&[("a", "object")], &[]); // removed b
        let theirs = make_schema(&[("a", "object"), ("b", "string")], &[]); // unchanged

        let result = three_way_merge(&base, &ours, &theirs);
        assert!(
            result.conflicts.is_empty(),
            "should be clean removal, got {:?}",
            result.conflicts
        );
        assert!(!result.merged_schema.vertices.contains_key("b"));
    }

    #[test]
    fn regression_one_side_removes_other_modifies() {
        let base = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let ours = make_schema(&[("a", "object")], &[]); // removed b
        let theirs = make_schema(&[("a", "object"), ("b", "integer")], &[]); // changed b's kind

        let result = three_way_merge(&base, &ours, &theirs);
        assert_eq!(result.conflicts.len(), 1);
        assert!(matches!(
            &result.conflicts[0],
            MergeConflict::DeleteModifyVertex {
                deleted_by: Side::Ours,
                ..
            }
        ));
    }

    // =======================================================================
    // D. Constraint tests
    // =======================================================================

    #[test]
    fn merge_constraint_conflict() {
        let mut base = make_schema(&[("x", "string")], &[]);
        base.constraints.insert(
            "x".into(),
            vec![Constraint {
                sort: "maxLength".into(),
                value: "3000".into(),
            }],
        );

        let mut ours = base.clone();
        ours.constraints.insert(
            "x".into(),
            vec![Constraint {
                sort: "maxLength".into(),
                value: "300".into(),
            }],
        );

        let mut theirs = base.clone();
        theirs.constraints.insert(
            "x".into(),
            vec![Constraint {
                sort: "maxLength".into(),
                value: "5000".into(),
            }],
        );

        let result = three_way_merge(&base, &ours, &theirs);
        assert_eq!(result.conflicts.len(), 1);
        assert!(matches!(
            &result.conflicts[0],
            MergeConflict::BothModifiedConstraint { .. }
        ));
        // Base value retained.
        assert_eq!(result.merged_schema.constraints["x"][0].value, "3000");
    }

    // =======================================================================
    // E. Edge tests
    // =======================================================================

    #[test]
    fn merge_edge_additions_from_both() {
        let base = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let edge_ours = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        let edge_theirs = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("y".into()),
        };
        let ours = make_schema(&[("a", "object"), ("b", "string")], &[edge_ours]);
        let theirs = make_schema(&[("a", "object"), ("b", "string")], &[edge_theirs]);

        let result = three_way_merge(&base, &ours, &theirs);
        assert!(result.conflicts.is_empty());
        assert_eq!(result.merged_schema.edges.len(), 2);
    }

    // =======================================================================
    // F. Orderings regression
    // =======================================================================

    #[test]
    fn regression_orderings_theirs_change_not_dropped() {
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: None,
        };
        let base = with_ext(
            make_schema(
                &[("a", "object"), ("b", "string")],
                std::slice::from_ref(&edge),
            ),
            |s| {
                s.orderings.insert(edge.clone(), 0);
            },
        );
        let ours = base.clone(); // unchanged
        let theirs = with_ext(
            make_schema(
                &[("a", "object"), ("b", "string")],
                std::slice::from_ref(&edge),
            ),
            |s| {
                s.orderings.insert(edge.clone(), 1);
            },
        );

        let result = three_way_merge(&base, &ours, &theirs);
        assert!(result.conflicts.is_empty());
        assert_eq!(result.merged_schema.orderings[&edge], 1);
    }

    // =======================================================================
    // G. Hyper-edge tests
    // =======================================================================

    #[test]
    fn merge_hyper_edge_one_side_removes() {
        let he = HyperEdge {
            id: "he1".into(),
            kind: "join".into(),
            signature: HashMap::new(),
            parent_label: "left".into(),
        };
        let base = with_ext(make_schema(&[("a", "object")], &[]), |s| {
            s.hyper_edges.insert("he1".into(), he.clone());
        });
        let ours = make_schema(&[("a", "object")], &[]); // removed he1
        let theirs = base.clone(); // unchanged

        let result = three_way_merge(&base, &ours, &theirs);
        assert!(result.conflicts.is_empty());
        assert!(!result.merged_schema.hyper_edges.contains_key("he1"));
    }

    // =======================================================================
    // H. Span tests
    // =======================================================================

    #[test]
    fn regression_spans_preserved_when_unchanged() {
        let base = with_ext(make_schema(&[("a", "object"), ("b", "string")], &[]), |s| {
            s.spans.insert(
                "s1".into(),
                Span {
                    id: "s1".into(),
                    left: "a".into(),
                    right: "b".into(),
                },
            );
        });
        let ours = base.clone();
        let theirs = base.clone();

        let result = three_way_merge(&base, &ours, &theirs);
        assert!(result.conflicts.is_empty());
        assert!(result.merged_schema.spans.contains_key("s1"));
    }

    // =======================================================================
    // I. Nominal tests
    // =======================================================================

    #[test]
    fn regression_nominal_change_propagated() {
        let base = with_ext(make_schema(&[("a", "object")], &[]), |s| {
            s.nominal.insert("a".into(), false);
        });
        let ours = with_ext(make_schema(&[("a", "object")], &[]), |s| {
            s.nominal.insert("a".into(), true);
        });
        let theirs = base.clone();

        let result = three_way_merge(&base, &ours, &theirs);
        assert!(result.conflicts.is_empty());
        assert_eq!(result.merged_schema.nominal.get("a"), Some(&true));
    }

    // =======================================================================
    // J. Usage mode tests
    // =======================================================================

    #[test]
    fn merge_usage_mode_one_side_changes() {
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: None,
        };
        let base = with_ext(
            make_schema(
                &[("a", "object"), ("b", "string")],
                std::slice::from_ref(&edge),
            ),
            |s| {
                s.usage_modes.insert(edge.clone(), UsageMode::Structural);
            },
        );
        let ours = base.clone();
        let theirs = with_ext(
            make_schema(
                &[("a", "object"), ("b", "string")],
                std::slice::from_ref(&edge),
            ),
            |s| {
                s.usage_modes.insert(edge.clone(), UsageMode::Linear);
            },
        );

        let result = three_way_merge(&base, &ours, &theirs);
        assert!(result.conflicts.is_empty());
        assert_eq!(result.merged_schema.usage_modes[&edge], UsageMode::Linear);
    }

    #[test]
    fn merge_usage_mode_both_change_differently() {
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: None,
        };
        let base = with_ext(
            make_schema(
                &[("a", "object"), ("b", "string")],
                std::slice::from_ref(&edge),
            ),
            |s| {
                s.usage_modes.insert(edge.clone(), UsageMode::Structural);
            },
        );
        let ours = with_ext(
            make_schema(
                &[("a", "object"), ("b", "string")],
                std::slice::from_ref(&edge),
            ),
            |s| {
                s.usage_modes.insert(edge.clone(), UsageMode::Linear);
            },
        );
        let theirs = with_ext(
            make_schema(
                &[("a", "object"), ("b", "string")],
                std::slice::from_ref(&edge),
            ),
            |s| {
                s.usage_modes.insert(edge.clone(), UsageMode::Affine);
            },
        );

        let result = three_way_merge(&base, &ours, &theirs);
        assert_eq!(result.conflicts.len(), 1);
        assert!(matches!(
            &result.conflicts[0],
            MergeConflict::BothModifiedUsageMode { .. }
        ));
        // Base retained.
        assert_eq!(
            result.merged_schema.usage_modes[&edge],
            UsageMode::Structural
        );
    }

    // =======================================================================
    // K. Variant tests
    // =======================================================================

    #[test]
    fn merge_variant_one_side_removes() {
        let base = with_ext(make_schema(&[("u", "union")], &[]), |s| {
            s.variants.insert(
                "u".into(),
                vec![Variant {
                    id: "v1".into(),
                    parent_vertex: "u".into(),
                    tag: Some("a".into()),
                }],
            );
        });
        let ours = make_schema(&[("u", "union")], &[]); // removed variant
        let theirs = base.clone();

        let result = three_way_merge(&base, &ours, &theirs);
        assert!(result.conflicts.is_empty());
        let merged_variants = result.merged_schema.variants.get("u");
        assert!(merged_variants.is_none() || merged_variants.is_some_and(Vec::is_empty));
    }

    // =======================================================================
    // L. Migration validity test
    // =======================================================================

    #[test]
    fn migration_maps_surviving_vertices() {
        let base = make_schema(&[("a", "object")], &[]);
        let ours = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let theirs = make_schema(&[("a", "object"), ("c", "integer")], &[]);

        let result = three_way_merge(&base, &ours, &theirs);
        // migration_from_ours should map ours' vertices to merged vertices.
        for vid in ours.vertices.keys() {
            if result.merged_schema.vertices.contains_key(vid) {
                assert!(
                    result.migration_from_ours.vertex_map.contains_key(vid),
                    "vertex {vid} in ours and merged but not in migration_from_ours"
                );
            }
        }
        for vid in theirs.vertices.keys() {
            if result.merged_schema.vertices.contains_key(vid) {
                assert!(
                    result.migration_from_theirs.vertex_map.contains_key(vid),
                    "vertex {vid} in theirs and merged but not in migration_from_theirs"
                );
            }
        }
    }
}
