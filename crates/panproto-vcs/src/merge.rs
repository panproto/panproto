//! Three-way schema merge with conflict detection.
//!
//! Given a base schema and two divergent schemas ("ours" and "theirs"),
//! computes a merged schema by applying the union of non-conflicting
//! changes. Conflicting modifications are reported as [`MergeConflict`]s.
//!
//! The merge is structural, not textual — it operates on the schema
//! graph (vertices, edges, constraints) rather than on serialized text.

use std::collections::HashMap;

use panproto_check::diff::{self, SchemaDiff};
use panproto_mig::Migration;
use panproto_schema::{Constraint, Edge, Schema, Vertex};
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::auto_mig;

/// The result of a three-way merge.
#[derive(Clone, Debug)]
pub struct MergeResult {
    /// The merged schema (may contain conflict tie-breaks).
    pub merged_schema: Schema,
    /// Any conflicts detected during the merge.
    pub conflicts: Vec<MergeConflict>,
    /// Migration from "ours" schema to the merged schema.
    pub migration_from_ours: Migration,
    /// Migration from "theirs" schema to the merged schema.
    pub migration_from_theirs: Migration,
}

/// A conflict detected during three-way merge.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MergeConflict {
    /// Both branches modified the same vertex's kind differently.
    BothModifiedVertex {
        /// The vertex ID.
        vertex_id: String,
        /// The kind in "ours".
        ours_kind: String,
        /// The kind in "theirs".
        theirs_kind: String,
    },

    /// Both branches modified the same constraint differently.
    BothModifiedConstraint {
        /// The vertex ID the constraint is on.
        vertex_id: String,
        /// The constraint sort.
        sort: String,
        /// The value in "ours".
        ours_value: String,
        /// The value in "theirs".
        theirs_value: String,
    },

    /// One branch deleted a vertex that the other modified.
    DeleteModifyVertex {
        /// The vertex ID.
        vertex_id: String,
        /// Which side deleted it.
        deleted_by: Side,
    },

    /// One branch deleted an edge that the other modified.
    DeleteModifyEdge {
        /// The edge.
        edge: Edge,
        /// Which side deleted it.
        deleted_by: Side,
    },
}

/// Which side of the merge performed an operation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Side {
    /// Our branch.
    Ours,
    /// Their branch.
    Theirs,
}

/// Perform a three-way merge of schemas.
///
/// # Algorithm
///
/// 1. Compute `diff(base, ours)` and `diff(base, theirs)`.
/// 2. Start with a copy of the base schema.
/// 3. Apply non-conflicting additions and removals from both sides.
/// 4. For overlapping modifications, detect conflicts. Tie-break: ours wins.
/// 5. Rebuild precomputed indices for the merged schema.
/// 6. Derive migrations from ours → merged and theirs → merged.
#[must_use]
pub fn three_way_merge(base: &Schema, ours: &Schema, theirs: &Schema) -> MergeResult {
    let diff_ours = diff::diff(base, ours);
    let diff_theirs = diff::diff(base, theirs);
    let mut conflicts = Vec::new();

    // Start with base's data.
    let mut vertices = base.vertices.clone();
    let mut edges = base.edges.clone();
    let mut hyper_edges = base.hyper_edges.clone();
    let mut constraints = base.constraints.clone();
    let mut required = base.required.clone();
    let mut nsids = base.nsids.clone();

    // -- Vertices --
    merge_vertices(
        &diff_ours,
        &diff_theirs,
        ours,
        theirs,
        &mut vertices,
        &mut conflicts,
    );

    // -- Edges --
    merge_edges(&diff_ours, &diff_theirs, &mut edges, &mut conflicts);

    // -- Constraints --
    merge_constraints(
        &diff_ours,
        &diff_theirs,
        ours,
        theirs,
        &mut constraints,
        &mut conflicts,
    );

    // -- Hyper-edges: union additions, apply removals --
    for (id, he) in &ours.hyper_edges {
        if !base.hyper_edges.contains_key(id) {
            hyper_edges.insert(id.clone(), he.clone());
        }
    }
    for (id, he) in &theirs.hyper_edges {
        if !base.hyper_edges.contains_key(id) {
            hyper_edges.entry(id.clone()).or_insert_with(|| he.clone());
        }
    }

    // -- Required: union --
    for (vid, reqs) in &ours.required {
        if !base.required.contains_key(vid) {
            required.insert(vid.clone(), reqs.clone());
        }
    }
    for (vid, reqs) in &theirs.required {
        if !base.required.contains_key(vid) {
            required.entry(vid.clone()).or_insert_with(|| reqs.clone());
        }
    }

    // -- NSIDs: union --
    for (vid, nsid) in &ours.nsids {
        if !base.nsids.contains_key(vid) {
            nsids.insert(vid.clone(), nsid.clone());
        }
    }
    for (vid, nsid) in &theirs.nsids {
        if !base.nsids.contains_key(vid) {
            nsids.entry(vid.clone()).or_insert_with(|| nsid.clone());
        }
    }

    // Remove vertices/edges that were deleted by either side.
    // (Already handled in merge_vertices/merge_edges.)

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

    // Merge new schema elements (union of additions from both sides).
    let mut variants = base.variants.clone();
    for (k, v) in &ours.variants {
        if !base.variants.contains_key(k) {
            variants.insert(k.clone(), v.clone());
        }
    }
    for (k, v) in &theirs.variants {
        if !base.variants.contains_key(k) {
            variants.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }

    let mut orderings = base.orderings.clone();
    for (k, v) in &ours.orderings {
        orderings.insert(k.clone(), *v);
    }
    for (k, v) in &theirs.orderings {
        orderings.entry(k.clone()).or_insert(*v);
    }

    let mut recursion_points = base.recursion_points.clone();
    for (k, v) in &ours.recursion_points {
        recursion_points.insert(k.clone(), v.clone());
    }
    for (k, v) in &theirs.recursion_points {
        recursion_points.entry(k.clone()).or_insert_with(|| v.clone());
    }

    let mut usage_modes = base.usage_modes.clone();
    for (k, v) in &ours.usage_modes {
        usage_modes.insert(k.clone(), v.clone());
    }
    for (k, v) in &theirs.usage_modes {
        usage_modes.entry(k.clone()).or_insert_with(|| v.clone());
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
        spans: HashMap::new(),
        usage_modes,
        nominal: base.nominal.clone(),
        outgoing,
        incoming,
        between,
    };

    // Derive migrations.
    let diff_ours_to_merged = diff::diff(ours, &merged_schema);
    let diff_theirs_to_merged = diff::diff(theirs, &merged_schema);
    let migration_from_ours = auto_mig::derive_migration(ours, &merged_schema, &diff_ours_to_merged);
    let migration_from_theirs =
        auto_mig::derive_migration(theirs, &merged_schema, &diff_theirs_to_merged);

    MergeResult {
        merged_schema,
        conflicts,
        migration_from_ours,
        migration_from_theirs,
    }
}

fn merge_vertices(
    diff_ours: &SchemaDiff,
    diff_theirs: &SchemaDiff,
    ours: &Schema,
    theirs: &Schema,
    vertices: &mut HashMap<String, Vertex>,
    conflicts: &mut Vec<MergeConflict>,
) {
    let ours_added: FxHashSet<&str> = diff_ours.added_vertices.iter().map(String::as_str).collect();
    let theirs_added: FxHashSet<&str> = diff_theirs.added_vertices.iter().map(String::as_str).collect();
    let ours_removed: FxHashSet<&str> = diff_ours.removed_vertices.iter().map(String::as_str).collect();
    let theirs_removed: FxHashSet<&str> = diff_theirs.removed_vertices.iter().map(String::as_str).collect();

    // Apply additions from ours.
    for vid in &diff_ours.added_vertices {
        if let Some(v) = ours.vertices.get(vid) {
            vertices.insert(vid.clone(), v.clone());
        }
    }
    // Apply additions from theirs (skip if already added by ours).
    for vid in &diff_theirs.added_vertices {
        if let Some(v) = theirs.vertices.get(vid) {
            vertices.entry(vid.clone()).or_insert_with(|| v.clone());
        }
    }

    // Apply removals.
    for vid in &diff_ours.removed_vertices {
        if theirs_added.contains(vid.as_str()) || theirs.vertices.get(vid).is_some_and(|v| {
            // Check if theirs modified it (kind change).
            !ours.vertices.contains_key(vid) || ours.vertices[vid].kind != v.kind
        }) {
            // Theirs modified/added something ours deleted.
            if !theirs_removed.contains(vid.as_str()) {
                conflicts.push(MergeConflict::DeleteModifyVertex {
                    vertex_id: vid.clone(),
                    deleted_by: Side::Ours,
                });
            }
        }
        vertices.remove(vid);
    }
    for vid in &diff_theirs.removed_vertices {
        if ours_added.contains(vid.as_str()) {
            // Already handled above.
        } else if ours.vertices.contains_key(vid) && !ours_removed.contains(vid.as_str()) {
            // Ours still has this vertex and didn't remove it — check if ours modified it.
            let base_kind = vertices.get(vid).map(|v| v.kind.as_str());
            let ours_kind = ours.vertices.get(vid).map(|v| v.kind.as_str());
            if base_kind != ours_kind {
                conflicts.push(MergeConflict::DeleteModifyVertex {
                    vertex_id: vid.clone(),
                    deleted_by: Side::Theirs,
                });
            }
        }
        vertices.remove(vid);
    }

    // Apply kind changes.
    let ours_kind_changes: HashMap<&str, (&str, &str)> = diff_ours
        .kind_changes
        .iter()
        .map(|kc| (kc.vertex_id.as_str(), (kc.old_kind.as_str(), kc.new_kind.as_str())))
        .collect();
    let theirs_kind_changes: HashMap<&str, (&str, &str)> = diff_theirs
        .kind_changes
        .iter()
        .map(|kc| (kc.vertex_id.as_str(), (kc.old_kind.as_str(), kc.new_kind.as_str())))
        .collect();

    for (vid, (_, new_kind)) in &ours_kind_changes {
        if let Some((_, theirs_new)) = theirs_kind_changes.get(vid) {
            if new_kind != theirs_new {
                conflicts.push(MergeConflict::BothModifiedVertex {
                    vertex_id: vid.to_string(),
                    ours_kind: new_kind.to_string(),
                    theirs_kind: theirs_new.to_string(),
                });
            }
        }
        // Apply ours' kind change (ours wins tie-break).
        if let Some(v) = vertices.get_mut(*vid) {
            v.kind = new_kind.to_string();
        }
    }
    for (vid, (_, new_kind)) in &theirs_kind_changes {
        if ours_kind_changes.contains_key(vid) {
            continue; // Already handled above (ours wins).
        }
        if let Some(v) = vertices.get_mut(*vid) {
            v.kind = new_kind.to_string();
        }
    }
}

fn merge_edges(
    diff_ours: &SchemaDiff,
    diff_theirs: &SchemaDiff,
    edges: &mut HashMap<Edge, String>,
    conflicts: &mut Vec<MergeConflict>,
) {
    let ours_removed: FxHashSet<&Edge> = diff_ours.removed_edges.iter().collect();
    let theirs_removed: FxHashSet<&Edge> = diff_theirs.removed_edges.iter().collect();

    // Add edges from ours.
    for edge in &diff_ours.added_edges {
        edges.insert(edge.clone(), edge.kind.clone());
    }
    // Add edges from theirs (skip duplicates).
    for edge in &diff_theirs.added_edges {
        edges.entry(edge.clone()).or_insert_with(|| edge.kind.clone());
    }

    // Remove edges.
    for edge in &diff_ours.removed_edges {
        if !theirs_removed.contains(edge) && diff_theirs.added_edges.contains(edge) {
            conflicts.push(MergeConflict::DeleteModifyEdge {
                edge: (*edge).clone(),
                deleted_by: Side::Ours,
            });
        }
        edges.remove(edge);
    }
    for edge in &diff_theirs.removed_edges {
        if !ours_removed.contains(edge) && diff_ours.added_edges.contains(edge) {
            conflicts.push(MergeConflict::DeleteModifyEdge {
                edge: (*edge).clone(),
                deleted_by: Side::Theirs,
            });
        }
        edges.remove(edge);
    }
}

fn merge_constraints(
    diff_ours: &SchemaDiff,
    diff_theirs: &SchemaDiff,
    ours: &Schema,
    theirs: &Schema,
    constraints: &mut HashMap<String, Vec<Constraint>>,
    conflicts: &mut Vec<MergeConflict>,
) {
    // Collect all modified vertices.
    let all_modified: FxHashSet<&str> = diff_ours
        .modified_constraints
        .keys()
        .chain(diff_theirs.modified_constraints.keys())
        .map(String::as_str)
        .collect();

    for vid in all_modified {
        let ours_diff = diff_ours.modified_constraints.get(vid);
        let theirs_diff = diff_theirs.modified_constraints.get(vid);

        // Use ours' constraints as the base to apply changes to.
        let merged = match (ours_diff, theirs_diff) {
            (Some(_), None) => {
                // Only ours changed — use ours' constraints.
                ours.constraints.get(vid).cloned().unwrap_or_default()
            }
            (None, Some(_)) => {
                // Only theirs changed — use theirs' constraints.
                theirs.constraints.get(vid).cloned().unwrap_or_default()
            }
            (Some(od), Some(td)) => {
                // Both changed — check for conflicts on the same sort.
                for oc in &od.changed {
                    for tc in &td.changed {
                        if oc.sort == tc.sort && oc.new_value != tc.new_value {
                            conflicts.push(MergeConflict::BothModifiedConstraint {
                                vertex_id: vid.to_string(),
                                sort: oc.sort.clone(),
                                ours_value: oc.new_value.clone(),
                                theirs_value: tc.new_value.clone(),
                            });
                        }
                    }
                }
                // Ours wins — use ours' constraints.
                ours.constraints.get(vid).cloned().unwrap_or_default()
            }
            (None, None) => continue,
        };
        constraints.insert(vid.to_string(), merged);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn merge_non_overlapping_additions() {
        let base = make_schema(&[("a", "object")], &[]);
        let ours = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let theirs = make_schema(&[("a", "object"), ("c", "integer")], &[]);

        let result = three_way_merge(&base, &ours, &theirs);
        assert!(result.conflicts.is_empty());
        assert_eq!(result.merged_schema.vertices.len(), 3);
        assert!(result.merged_schema.vertices.contains_key("a"));
        assert!(result.merged_schema.vertices.contains_key("b"));
        assert!(result.merged_schema.vertices.contains_key("c"));
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
            MergeConflict::BothModifiedVertex {
                vertex_id,
                ours_kind,
                theirs_kind,
            } if vertex_id == "a" && ours_kind == "string" && theirs_kind == "integer"
        ));
        // Ours wins tie-break.
        assert_eq!(result.merged_schema.vertices["a"].kind, "string");
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
    fn merge_one_side_removes_vertex() {
        let base = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let ours = make_schema(&[("a", "object")], &[]); // removed b
        let theirs = make_schema(&[("a", "object"), ("b", "string")], &[]);

        let result = three_way_merge(&base, &ours, &theirs);
        // b was removed by ours, theirs didn't modify it — clean.
        assert!(!result.merged_schema.vertices.contains_key("b"));
    }

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
            MergeConflict::BothModifiedConstraint {
                vertex_id,
                sort,
                ours_value,
                theirs_value,
            } if vertex_id == "x" && sort == "maxLength" && ours_value == "300" && theirs_value == "5000"
        ));
    }

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
        let ours = make_schema(
            &[("a", "object"), ("b", "string")],
            &[edge_ours.clone()],
        );
        let theirs = make_schema(
            &[("a", "object"), ("b", "string")],
            &[edge_theirs.clone()],
        );

        let result = three_way_merge(&base, &ours, &theirs);
        assert!(result.conflicts.is_empty());
        assert_eq!(result.merged_schema.edges.len(), 2);
        assert!(result.merged_schema.edges.contains_key(&edge_ours));
        assert!(result.merged_schema.edges.contains_key(&edge_theirs));
    }

    #[test]
    fn merge_commutativity_no_conflicts() {
        let base = make_schema(&[("a", "object")], &[]);
        let ours = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let theirs = make_schema(&[("a", "object"), ("c", "integer")], &[]);

        let r1 = three_way_merge(&base, &ours, &theirs);
        let r2 = three_way_merge(&base, &theirs, &ours);

        // Both should produce the same merged schema.
        assert_eq!(r1.merged_schema.vertices.len(), r2.merged_schema.vertices.len());
        for (id, v1) in &r1.merged_schema.vertices {
            let v2 = r2.merged_schema.vertices.get(id).unwrap();
            assert_eq!(v1.kind, v2.kind);
        }
    }
}
