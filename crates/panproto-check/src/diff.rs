//! Structural diffing of two schemas.
//!
//! [`diff`] compares an old and new schema, producing a [`SchemaDiff`]
//! that records every added, removed, or modified element. The diff is
//! purely structural -- it does not yet classify changes as breaking or
//! non-breaking (that is handled by [`crate::classify()`]).

use std::collections::HashMap;

use panproto_schema::{Constraint, Edge, RecursionPoint, Schema, UsageMode, Variant};
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};

/// A structural diff between two schemas.
///
/// Each field captures a specific category of change between the old
/// and new schema revisions.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaDiff {
    /// Vertex IDs present in the new schema but absent from the old.
    pub added_vertices: Vec<String>,
    /// Vertex IDs present in the old schema but absent from the new.
    pub removed_vertices: Vec<String>,
    /// Edges present in the new schema but absent from the old.
    pub added_edges: Vec<Edge>,
    /// Edges present in the old schema but absent from the new.
    pub removed_edges: Vec<Edge>,
    /// Constraints that changed between old and new, keyed by vertex ID.
    pub modified_constraints: HashMap<String, ConstraintDiff>,
    /// Vertices whose `kind` changed between old and new.
    pub kind_changes: Vec<KindChange>,

    /// Variants added in the new schema.
    pub added_variants: Vec<Variant>,
    /// Variants removed from the old schema.
    pub removed_variants: Vec<Variant>,
    /// Edge ordering changes: `(edge, old_position, new_position)`.
    pub order_changes: Vec<(Edge, Option<u32>, Option<u32>)>,
    /// Recursion points added in the new schema.
    pub added_recursion_points: Vec<RecursionPoint>,
    /// Recursion points removed from the old schema.
    pub removed_recursion_points: Vec<RecursionPoint>,
    /// Usage mode changes: `(edge, old_mode, new_mode)`.
    pub usage_mode_changes: Vec<(Edge, UsageMode, UsageMode)>,
}

/// Describes how constraints on a single vertex changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstraintDiff {
    /// Constraints added in the new schema.
    pub added: Vec<Constraint>,
    /// Constraints removed from the old schema.
    pub removed: Vec<Constraint>,
    /// Constraints whose value changed.
    pub changed: Vec<ConstraintChange>,
}

/// A single constraint that changed its value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstraintChange {
    /// The constraint sort (e.g., `"maxLength"`).
    pub sort: String,
    /// The value in the old schema.
    pub old_value: String,
    /// The value in the new schema.
    pub new_value: String,
}

/// Records a vertex whose kind changed between schema versions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KindChange {
    /// The vertex ID.
    pub vertex_id: String,
    /// The kind in the old schema.
    pub old_kind: String,
    /// The kind in the new schema.
    pub new_kind: String,
}

/// Compute a structural diff between two schemas.
///
/// Compares vertices, edges, constraints, and vertex kinds. The diff
/// is symmetric with respect to additions/removals.
#[must_use]
pub fn diff(old: &Schema, new: &Schema) -> SchemaDiff {
    let mut result = SchemaDiff::default();

    // --- Vertices ---
    let old_verts: FxHashSet<&String> = old.vertices.keys().collect();
    let new_verts: FxHashSet<&String> = new.vertices.keys().collect();

    for v in &new_verts {
        if !old_verts.contains(*v) {
            result.added_vertices.push((*v).clone());
        }
    }
    for v in &old_verts {
        if !new_verts.contains(*v) {
            result.removed_vertices.push((*v).clone());
        }
    }

    // Sort for deterministic output.
    result.added_vertices.sort();
    result.removed_vertices.sort();

    // --- Kind changes ---
    for v in old_verts.intersection(&new_verts) {
        if let (Some(old_v), Some(new_v)) = (old.vertices.get(*v), new.vertices.get(*v)) {
            if old_v.kind != new_v.kind {
                result.kind_changes.push(KindChange {
                    vertex_id: (*v).clone(),
                    old_kind: old_v.kind.clone(),
                    new_kind: new_v.kind.clone(),
                });
            }
        }
    }
    result
        .kind_changes
        .sort_by(|a, b| a.vertex_id.cmp(&b.vertex_id));

    // --- Edges ---
    let old_edges: FxHashSet<&Edge> = old.edges.keys().collect();
    let new_edges: FxHashSet<&Edge> = new.edges.keys().collect();

    for e in &new_edges {
        if !old_edges.contains(*e) {
            result.added_edges.push((*e).clone());
        }
    }
    for e in &old_edges {
        if !new_edges.contains(*e) {
            result.removed_edges.push((*e).clone());
        }
    }
    result.added_edges.sort();
    result.removed_edges.sort();

    // --- Constraints ---
    let all_vertex_ids: FxHashSet<&String> = old
        .constraints
        .keys()
        .chain(new.constraints.keys())
        .collect();

    for vid in all_vertex_ids {
        let old_cs = old.constraints.get(vid).cloned().unwrap_or_default();
        let new_cs = new.constraints.get(vid).cloned().unwrap_or_default();

        let cdiff = diff_constraints(&old_cs, &new_cs);
        if !cdiff.added.is_empty() || !cdiff.removed.is_empty() || !cdiff.changed.is_empty() {
            result.modified_constraints.insert(vid.clone(), cdiff);
        }
    }

    // --- Variants ---
    for (parent, new_variants) in &new.variants {
        let old_variants = old.variants.get(parent).cloned().unwrap_or_default();
        let old_ids: FxHashSet<&str> = old_variants.iter().map(|v| v.id.as_str()).collect();
        for v in new_variants {
            if !old_ids.contains(v.id.as_str()) {
                result.added_variants.push(v.clone());
            }
        }
    }
    for (parent, old_variants) in &old.variants {
        let new_variants = new.variants.get(parent).cloned().unwrap_or_default();
        let new_ids: FxHashSet<&str> = new_variants.iter().map(|v| v.id.as_str()).collect();
        for v in old_variants {
            if !new_ids.contains(v.id.as_str()) {
                result.removed_variants.push(v.clone());
            }
        }
    }

    // --- Orderings ---
    let all_order_edges: FxHashSet<&Edge> = old
        .orderings
        .keys()
        .chain(new.orderings.keys())
        .collect();
    for edge in all_order_edges {
        let old_pos = old.orderings.get(edge).copied();
        let new_pos = new.orderings.get(edge).copied();
        if old_pos != new_pos {
            result
                .order_changes
                .push(((*edge).clone(), old_pos, new_pos));
        }
    }

    // --- Recursion Points ---
    for (id, rp) in &new.recursion_points {
        if !old.recursion_points.contains_key(id) {
            result.added_recursion_points.push(rp.clone());
        }
    }
    for (id, rp) in &old.recursion_points {
        if !new.recursion_points.contains_key(id) {
            result.removed_recursion_points.push(rp.clone());
        }
    }

    // --- Usage Modes ---
    let all_usage_edges: FxHashSet<&Edge> = old
        .usage_modes
        .keys()
        .chain(new.usage_modes.keys())
        .collect();
    for edge in all_usage_edges {
        let old_mode = old.usage_modes.get(edge).cloned().unwrap_or_default();
        let new_mode = new.usage_modes.get(edge).cloned().unwrap_or_default();
        if old_mode != new_mode {
            result
                .usage_mode_changes
                .push(((*edge).clone(), old_mode, new_mode));
        }
    }

    result
}

/// Diff two constraint lists for a single vertex.
fn diff_constraints(old: &[Constraint], new: &[Constraint]) -> ConstraintDiff {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();

    let old_by_sort: HashMap<&str, &Constraint> =
        old.iter().map(|c| (c.sort.as_str(), c)).collect();
    let new_by_sort: HashMap<&str, &Constraint> =
        new.iter().map(|c| (c.sort.as_str(), c)).collect();

    for (sort, nc) in &new_by_sort {
        match old_by_sort.get(sort) {
            Some(oc) if oc.value != nc.value => {
                changed.push(ConstraintChange {
                    sort: sort.to_string(),
                    old_value: oc.value.clone(),
                    new_value: nc.value.clone(),
                });
            }
            None => {
                added.push((*nc).clone());
            }
            _ => {}
        }
    }

    for (sort, oc) in &old_by_sort {
        if !new_by_sort.contains_key(sort) {
            removed.push((*oc).clone());
        }
    }

    ConstraintDiff {
        added,
        removed,
        changed,
    }
}

/// Returns `true` if the diff represents no changes.
impl SchemaDiff {
    /// Returns `true` if this diff contains no changes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added_vertices.is_empty()
            && self.removed_vertices.is_empty()
            && self.added_edges.is_empty()
            && self.removed_edges.is_empty()
            && self.modified_constraints.is_empty()
            && self.kind_changes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use panproto_schema::Vertex;
    use smallvec::SmallVec;
    use std::collections::HashMap;

    /// Helper to build a minimal test schema.
    fn test_schema(
        vertices: &[(&str, &str)],
        edges: &[Edge],
        constraints: HashMap<String, Vec<Constraint>>,
    ) -> Schema {
        let mut vert_map = HashMap::new();
        let mut edge_map = HashMap::new();
        let mut outgoing: HashMap<String, SmallVec<Edge, 4>> = HashMap::new();
        let mut incoming: HashMap<String, SmallVec<Edge, 4>> = HashMap::new();
        let mut between: HashMap<(String, String), SmallVec<Edge, 2>> = HashMap::new();

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

        Schema {
            protocol: "test".into(),
            vertices: vert_map,
            edges: edge_map,
            hyper_edges: HashMap::new(),
            constraints,
            required: HashMap::new(),
            nsids: HashMap::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            outgoing,
            incoming,
            between,
        }
    }

    #[test]
    fn diff_added_and_removed_vertices() {
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: None,
        };
        let old = test_schema(&[("a", "object"), ("b", "string")], &[edge], HashMap::new());
        let new = test_schema(&[("a", "object"), ("c", "integer")], &[], HashMap::new());

        let d = diff(&old, &new);
        assert_eq!(d.added_vertices, vec!["c"]);
        assert_eq!(d.removed_vertices, vec!["b"]);
        assert_eq!(d.removed_edges.len(), 1);
    }

    #[test]
    fn diff_kind_change() {
        let old = test_schema(&[("x", "string")], &[], HashMap::new());
        let new = test_schema(&[("x", "integer")], &[], HashMap::new());

        let d = diff(&old, &new);
        assert_eq!(d.kind_changes.len(), 1);
        assert_eq!(d.kind_changes[0].old_kind, "string");
        assert_eq!(d.kind_changes[0].new_kind, "integer");
    }

    #[test]
    fn diff_constraint_changed() {
        let old_constraints = HashMap::from([(
            "x".to_string(),
            vec![Constraint {
                sort: "maxLength".into(),
                value: "3000".into(),
            }],
        )]);
        let new_constraints = HashMap::from([(
            "x".to_string(),
            vec![Constraint {
                sort: "maxLength".into(),
                value: "300".into(),
            }],
        )]);

        let old = test_schema(&[("x", "string")], &[], old_constraints);
        let new = test_schema(&[("x", "string")], &[], new_constraints);

        let d = diff(&old, &new);
        assert!(d.modified_constraints.contains_key("x"));
        let cdiff = &d.modified_constraints["x"];
        assert_eq!(cdiff.changed.len(), 1);
        assert_eq!(cdiff.changed[0].old_value, "3000");
        assert_eq!(cdiff.changed[0].new_value, "300");
    }

    #[test]
    fn empty_diff_for_identical_schemas() {
        let s = test_schema(&[("a", "object")], &[], HashMap::new());
        let d = diff(&s, &s);
        assert!(d.is_empty());
    }
}
