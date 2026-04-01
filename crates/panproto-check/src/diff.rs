//! Structural diffing of two schemas.
//!
//! [`diff`] compares an old and new schema, producing a [`SchemaDiff`]
//! that records every added, removed, or modified element. The diff is
//! purely structural; it does not yet classify changes as breaking or
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
    // --- Vertices ---
    /// Vertex IDs present in the new schema but absent from the old.
    pub added_vertices: Vec<String>,
    /// Vertex IDs present in the old schema but absent from the new.
    pub removed_vertices: Vec<String>,
    /// Vertices whose `kind` changed between old and new.
    pub kind_changes: Vec<KindChange>,

    // --- Edges ---
    /// Edges present in the new schema but absent from the old.
    pub added_edges: Vec<Edge>,
    /// Edges present in the old schema but absent from the new.
    pub removed_edges: Vec<Edge>,

    // --- Constraints ---
    /// Constraints that changed between old and new, keyed by vertex ID.
    pub modified_constraints: HashMap<String, ConstraintDiff>,

    // --- Hyper-edges ---
    /// Hyper-edge IDs added in the new schema.
    pub added_hyper_edges: Vec<String>,
    /// Hyper-edge IDs removed from the old schema.
    pub removed_hyper_edges: Vec<String>,
    /// Hyper-edges whose kind, signature, or parent label changed.
    pub modified_hyper_edges: Vec<HyperEdgeChange>,

    // --- Required edges ---
    /// Per-vertex: required edges added in the new schema.
    pub added_required: HashMap<String, Vec<Edge>>,
    /// Per-vertex: required edges removed from the old schema.
    pub removed_required: HashMap<String, Vec<Edge>>,

    // --- NSIDs ---
    /// Vertex-to-NSID mappings added in the new schema.
    pub added_nsids: HashMap<String, String>,
    /// Vertex IDs whose NSID mapping was removed.
    pub removed_nsids: Vec<String>,
    /// NSID mappings that changed: `(vertex_id, old_nsid, new_nsid)`.
    pub changed_nsids: Vec<(String, String, String)>,

    // --- Variants ---
    /// Variants added in the new schema.
    pub added_variants: Vec<Variant>,
    /// Variants removed from the old schema.
    pub removed_variants: Vec<Variant>,
    /// Variants whose tag changed (same ID, different tag).
    pub modified_variants: Vec<VariantChange>,

    // --- Orderings ---
    /// Edge ordering changes: `(edge, old_position, new_position)`.
    pub order_changes: Vec<(Edge, Option<u32>, Option<u32>)>,

    // --- Recursion points ---
    /// Recursion points added in the new schema.
    pub added_recursion_points: Vec<RecursionPoint>,
    /// Recursion points removed from the old schema.
    pub removed_recursion_points: Vec<RecursionPoint>,
    /// Recursion points whose target vertex changed.
    pub modified_recursion_points: Vec<RecursionPointChange>,

    // --- Usage modes ---
    /// Usage mode changes: `(edge, old_mode, new_mode)`.
    pub usage_mode_changes: Vec<(Edge, UsageMode, UsageMode)>,

    // --- Spans ---
    /// Span IDs added in the new schema.
    pub added_spans: Vec<String>,
    /// Span IDs removed from the old schema.
    pub removed_spans: Vec<String>,
    /// Spans whose left or right vertex changed.
    pub modified_spans: Vec<SpanChange>,

    // --- Nominal ---
    /// Nominal flag changes: `(vertex_id, old_value, new_value)`.
    pub nominal_changes: Vec<(String, bool, bool)>,

    // --- Enrichment maps ---
    /// Coercion keys `(source_kind, target_kind)` added in the new schema.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added_coercions: Vec<(String, String)>,
    /// Coercion keys `(source_kind, target_kind)` removed from the old schema.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_coercions: Vec<(String, String)>,
    /// Coercion keys `(source_kind, target_kind)` whose expression changed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modified_coercions: Vec<(String, String)>,

    /// Merger keys (vertex ID) added in the new schema.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added_mergers: Vec<String>,
    /// Merger keys (vertex ID) removed from the old schema.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_mergers: Vec<String>,
    /// Merger keys (vertex ID) whose expression changed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modified_mergers: Vec<String>,

    /// Default keys (vertex ID) added in the new schema.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added_defaults: Vec<String>,
    /// Default keys (vertex ID) removed from the old schema.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_defaults: Vec<String>,
    /// Default keys (vertex ID) whose expression changed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modified_defaults: Vec<String>,

    /// Policy keys (sort name) added in the new schema.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added_policies: Vec<String>,
    /// Policy keys (sort name) removed from the old schema.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_policies: Vec<String>,
    /// Policy keys (sort name) whose expression changed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modified_policies: Vec<String>,
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

/// Records changes to a hyper-edge's kind, signature, or parent label.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HyperEdgeChange {
    /// The hyper-edge ID.
    pub id: String,
    /// Kind change: `(old_kind, new_kind)`, or `None` if unchanged.
    pub kind_change: Option<(String, String)>,
    /// Signature labels added: label → `vertex_id`.
    pub signature_added: HashMap<String, String>,
    /// Signature labels removed: label → `vertex_id`.
    pub signature_removed: HashMap<String, String>,
    /// Signature labels whose vertex changed: label → (`old_vid`, `new_vid`).
    pub signature_changed: HashMap<String, (String, String)>,
    /// Parent label change: `(old, new)`, or `None` if unchanged.
    pub parent_label_change: Option<(String, String)>,
}

/// Records a variant whose tag changed between schema versions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantChange {
    /// The variant ID.
    pub id: String,
    /// The parent coproduct vertex ID.
    pub parent_vertex: String,
    /// The old tag.
    pub old_tag: Option<String>,
    /// The new tag.
    pub new_tag: Option<String>,
}

/// Records a recursion point whose target vertex changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecursionPointChange {
    /// The fixpoint marker vertex ID.
    pub mu_id: String,
    /// The old target vertex.
    pub old_target: String,
    /// The new target vertex.
    pub new_target: String,
}

/// Records a span whose left or right vertex changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpanChange {
    /// The span ID.
    pub id: String,
    /// Left vertex change: `(old, new)`, or `None` if unchanged.
    pub left_change: Option<(String, String)>,
    /// Right vertex change: `(old, new)`, or `None` if unchanged.
    pub right_change: Option<(String, String)>,
}

/// Compute a structural diff between two schemas.
///
/// Compares every schema field: vertices, edges, constraints, hyper-edges,
/// required edges, NSIDs, variants, orderings, recursion points, usage modes,
/// spans, and nominal flags.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn diff(old: &Schema, new: &Schema) -> SchemaDiff {
    let mut result = SchemaDiff::default();

    // --- Vertices ---
    let old_verts: FxHashSet<&panproto_gat::Name> = old.vertices.keys().collect();
    let new_verts: FxHashSet<&panproto_gat::Name> = new.vertices.keys().collect();

    for v in &new_verts {
        if !old_verts.contains(*v) {
            result.added_vertices.push(v.to_string());
        }
    }
    for v in &old_verts {
        if !new_verts.contains(*v) {
            result.removed_vertices.push(v.to_string());
        }
    }
    result.added_vertices.sort();
    result.removed_vertices.sort();

    // --- Kind changes ---
    for v in old_verts.intersection(&new_verts) {
        if let (Some(old_v), Some(new_v)) = (old.vertices.get(*v), new.vertices.get(*v)) {
            if old_v.kind != new_v.kind {
                result.kind_changes.push(KindChange {
                    vertex_id: v.to_string(),
                    old_kind: old_v.kind.to_string(),
                    new_kind: new_v.kind.to_string(),
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
    let all_constraint_vids: FxHashSet<&panproto_gat::Name> = old
        .constraints
        .keys()
        .chain(new.constraints.keys())
        .collect();

    for vid in all_constraint_vids {
        let old_cs = old.constraints.get(vid).cloned().unwrap_or_default();
        let new_cs = new.constraints.get(vid).cloned().unwrap_or_default();

        let cdiff = diff_constraints(&old_cs, &new_cs);
        if !cdiff.added.is_empty() || !cdiff.removed.is_empty() || !cdiff.changed.is_empty() {
            result.modified_constraints.insert(vid.to_string(), cdiff);
        }
    }

    // --- Hyper-edges ---
    diff_hyper_edges(old, new, &mut result);

    // --- Required edges ---
    diff_required(old, new, &mut result);

    // --- NSIDs ---
    diff_nsids(old, new, &mut result);

    // --- Variants ---
    diff_variants(old, new, &mut result);

    // --- Orderings ---
    let all_order_edges: FxHashSet<&Edge> =
        old.orderings.keys().chain(new.orderings.keys()).collect();
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
    diff_recursion_points(old, new, &mut result);

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

    // --- Spans ---
    diff_spans(old, new, &mut result);

    // --- Nominal ---
    diff_nominal(old, new, &mut result);

    // --- Enrichment maps ---
    diff_coercions(old, new, &mut result);
    diff_name_keyed_exprs(
        &old.mergers,
        &new.mergers,
        &mut result.added_mergers,
        &mut result.removed_mergers,
        &mut result.modified_mergers,
    );
    diff_name_keyed_exprs(
        &old.defaults,
        &new.defaults,
        &mut result.added_defaults,
        &mut result.removed_defaults,
        &mut result.modified_defaults,
    );
    diff_name_keyed_exprs(
        &old.policies,
        &new.policies,
        &mut result.added_policies,
        &mut result.removed_policies,
        &mut result.modified_policies,
    );

    result
}

// ---------------------------------------------------------------------------
// Per-field diff helpers
// ---------------------------------------------------------------------------

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

/// Diff hyper-edges: additions, removals, and modifications.
fn diff_hyper_edges(old: &Schema, new: &Schema, result: &mut SchemaDiff) {
    let old_ids: FxHashSet<&panproto_gat::Name> = old.hyper_edges.keys().collect();
    let new_ids: FxHashSet<&panproto_gat::Name> = new.hyper_edges.keys().collect();

    for id in &new_ids {
        if !old_ids.contains(*id) {
            result.added_hyper_edges.push(id.to_string());
        }
    }
    for id in &old_ids {
        if !new_ids.contains(*id) {
            result.removed_hyper_edges.push(id.to_string());
        }
    }
    result.added_hyper_edges.sort();
    result.removed_hyper_edges.sort();

    // Modifications on surviving hyper-edges.
    for id in old_ids.intersection(&new_ids) {
        let old_he = &old.hyper_edges[*id];
        let new_he = &new.hyper_edges[*id];

        if old_he == new_he {
            continue;
        }

        let kind_change = if old_he.kind == new_he.kind {
            None
        } else {
            Some((old_he.kind.to_string(), new_he.kind.to_string()))
        };

        let parent_label_change = if old_he.parent_label == new_he.parent_label {
            None
        } else {
            Some((
                old_he.parent_label.to_string(),
                new_he.parent_label.to_string(),
            ))
        };

        let mut sig_added = HashMap::new();
        let mut sig_removed = HashMap::new();
        let mut sig_changed = HashMap::new();

        for (label, new_vid) in &new_he.signature {
            match old_he.signature.get(label) {
                Some(old_vid) if old_vid != new_vid => {
                    sig_changed.insert(
                        label.to_string(),
                        (old_vid.to_string(), new_vid.to_string()),
                    );
                }
                None => {
                    sig_added.insert(label.to_string(), new_vid.to_string());
                }
                _ => {}
            }
        }
        for (label, old_vid) in &old_he.signature {
            if !new_he.signature.contains_key(label) {
                sig_removed.insert(label.to_string(), old_vid.to_string());
            }
        }

        result.modified_hyper_edges.push(HyperEdgeChange {
            id: id.to_string(),
            kind_change,
            signature_added: sig_added,
            signature_removed: sig_removed,
            signature_changed: sig_changed,
            parent_label_change,
        });
    }
    result.modified_hyper_edges.sort_by(|a, b| a.id.cmp(&b.id));
}

/// Diff required-edge maps.
fn diff_required(old: &Schema, new: &Schema, result: &mut SchemaDiff) {
    let all_vids: FxHashSet<&panproto_gat::Name> =
        old.required.keys().chain(new.required.keys()).collect();

    for vid in all_vids {
        let old_edges: FxHashSet<&Edge> = old
            .required
            .get(vid)
            .map(|v| v.iter().collect())
            .unwrap_or_default();
        let new_edges: FxHashSet<&Edge> = new
            .required
            .get(vid)
            .map(|v| v.iter().collect())
            .unwrap_or_default();

        let added: Vec<Edge> = new_edges
            .difference(&old_edges)
            .map(|e| (*e).clone())
            .collect();
        let removed: Vec<Edge> = old_edges
            .difference(&new_edges)
            .map(|e| (*e).clone())
            .collect();

        if !added.is_empty() {
            result.added_required.insert(vid.to_string(), added);
        }
        if !removed.is_empty() {
            result.removed_required.insert(vid.to_string(), removed);
        }
    }
}

/// Diff NSID maps.
fn diff_nsids(old: &Schema, new: &Schema, result: &mut SchemaDiff) {
    for (vid, new_nsid) in &new.nsids {
        match old.nsids.get(vid) {
            Some(old_nsid) if old_nsid != new_nsid => {
                result.changed_nsids.push((
                    vid.to_string(),
                    old_nsid.to_string(),
                    new_nsid.to_string(),
                ));
            }
            None => {
                result
                    .added_nsids
                    .insert(vid.to_string(), new_nsid.to_string());
            }
            _ => {}
        }
    }
    for vid in old.nsids.keys() {
        if !new.nsids.contains_key(vid) {
            result.removed_nsids.push(vid.to_string());
        }
    }
    result.removed_nsids.sort();
    result.changed_nsids.sort_by(|a, b| a.0.cmp(&b.0));
}

/// Diff variants: additions, removals, and tag modifications.
fn diff_variants(old: &Schema, new: &Schema, result: &mut SchemaDiff) {
    // Build a flat lookup: (parent_vertex, variant_id) → Variant
    let mut old_flat: HashMap<(&str, &str), &Variant> = HashMap::new();
    let mut new_flat: HashMap<(&str, &str), &Variant> = HashMap::new();

    for (parent, variants) in &old.variants {
        for v in variants {
            old_flat.insert((parent.as_str(), v.id.as_str()), v);
        }
    }
    for (parent, variants) in &new.variants {
        for v in variants {
            new_flat.insert((parent.as_str(), v.id.as_str()), v);
        }
    }

    // Additions and modifications.
    for (&(parent, vid), new_v) in &new_flat {
        match old_flat.get(&(parent, vid)) {
            Some(old_v) => {
                if old_v.tag != new_v.tag {
                    result.modified_variants.push(VariantChange {
                        id: vid.to_string(),
                        parent_vertex: parent.to_string(),
                        old_tag: old_v.tag.as_ref().map(ToString::to_string),
                        new_tag: new_v.tag.as_ref().map(ToString::to_string),
                    });
                }
            }
            None => {
                result.added_variants.push((*new_v).clone());
            }
        }
    }

    // Removals.
    for (&(parent, vid), old_v) in &old_flat {
        if !new_flat.contains_key(&(parent, vid)) {
            result.removed_variants.push((*old_v).clone());
        }
    }

    result
        .modified_variants
        .sort_by(|a, b| (&a.parent_vertex, &a.id).cmp(&(&b.parent_vertex, &b.id)));
}

/// Diff recursion points: additions, removals, and target changes.
fn diff_recursion_points(old: &Schema, new: &Schema, result: &mut SchemaDiff) {
    for (id, new_rp) in &new.recursion_points {
        match old.recursion_points.get(id) {
            Some(old_rp) => {
                if old_rp.target_vertex != new_rp.target_vertex {
                    result.modified_recursion_points.push(RecursionPointChange {
                        mu_id: id.to_string(),
                        old_target: old_rp.target_vertex.to_string(),
                        new_target: new_rp.target_vertex.to_string(),
                    });
                }
            }
            None => {
                result.added_recursion_points.push(new_rp.clone());
            }
        }
    }
    for (id, old_rp) in &old.recursion_points {
        if !new.recursion_points.contains_key(id) {
            result.removed_recursion_points.push(old_rp.clone());
        }
    }
}

/// Diff spans: additions, removals, and left/right changes.
fn diff_spans(old: &Schema, new: &Schema, result: &mut SchemaDiff) {
    let old_ids: FxHashSet<&panproto_gat::Name> = old.spans.keys().collect();
    let new_ids: FxHashSet<&panproto_gat::Name> = new.spans.keys().collect();

    for id in &new_ids {
        if !old_ids.contains(*id) {
            result.added_spans.push(id.to_string());
        }
    }
    for id in &old_ids {
        if !new_ids.contains(*id) {
            result.removed_spans.push(id.to_string());
        }
    }
    result.added_spans.sort();
    result.removed_spans.sort();

    for id in old_ids.intersection(&new_ids) {
        let old_span = &old.spans[*id];
        let new_span = &new.spans[*id];

        if old_span == new_span {
            continue;
        }

        let left_change = if old_span.left == new_span.left {
            None
        } else {
            Some((old_span.left.to_string(), new_span.left.to_string()))
        };
        let right_change = if old_span.right == new_span.right {
            None
        } else {
            Some((old_span.right.to_string(), new_span.right.to_string()))
        };

        result.modified_spans.push(SpanChange {
            id: id.to_string(),
            left_change,
            right_change,
        });
    }
    result.modified_spans.sort_by(|a, b| a.id.cmp(&b.id));
}

/// Diff nominal flags.
fn diff_nominal(old: &Schema, new: &Schema, result: &mut SchemaDiff) {
    let all_vids: FxHashSet<&panproto_gat::Name> =
        old.nominal.keys().chain(new.nominal.keys()).collect();

    for vid in all_vids {
        let old_val = old.nominal.get(vid).copied().unwrap_or(false);
        let new_val = new.nominal.get(vid).copied().unwrap_or(false);
        if old_val != new_val {
            result
                .nominal_changes
                .push((vid.to_string(), old_val, new_val));
        }
    }
    result.nominal_changes.sort_by(|a, b| a.0.cmp(&b.0));
}

/// Diff coercion maps: keyed by `(Name, Name)`.
fn diff_coercions(old: &Schema, new: &Schema, result: &mut SchemaDiff) {
    for (key, new_expr) in &new.coercions {
        match old.coercions.get(key) {
            Some(old_expr) => {
                if old_expr != new_expr {
                    result
                        .modified_coercions
                        .push((key.0.to_string(), key.1.to_string()));
                }
            }
            None => {
                result
                    .added_coercions
                    .push((key.0.to_string(), key.1.to_string()));
            }
        }
    }
    for key in old.coercions.keys() {
        if !new.coercions.contains_key(key) {
            result
                .removed_coercions
                .push((key.0.to_string(), key.1.to_string()));
        }
    }
    result.added_coercions.sort();
    result.removed_coercions.sort();
    result.modified_coercions.sort();
}

/// Diff `HashMap<Name, V>` maps where `V: PartialEq` (mergers, defaults, policies).
fn diff_name_keyed_exprs<V: PartialEq>(
    old: &std::collections::HashMap<panproto_gat::Name, V>,
    new: &std::collections::HashMap<panproto_gat::Name, V>,
    added: &mut Vec<String>,
    removed: &mut Vec<String>,
    modified: &mut Vec<String>,
) {
    for (key, new_expr) in new {
        match old.get(key) {
            Some(old_expr) => {
                if old_expr != new_expr {
                    modified.push(key.to_string());
                }
            }
            None => {
                added.push(key.to_string());
            }
        }
    }
    for key in old.keys() {
        if !new.contains_key(key) {
            removed.push(key.to_string());
        }
    }
    added.sort();
    removed.sort();
    modified.sort();
}

impl SchemaDiff {
    /// Returns `true` if this diff contains no changes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added_vertices.is_empty()
            && self.removed_vertices.is_empty()
            && self.kind_changes.is_empty()
            && self.added_edges.is_empty()
            && self.removed_edges.is_empty()
            && self.modified_constraints.is_empty()
            && self.added_hyper_edges.is_empty()
            && self.removed_hyper_edges.is_empty()
            && self.modified_hyper_edges.is_empty()
            && self.added_required.is_empty()
            && self.removed_required.is_empty()
            && self.added_nsids.is_empty()
            && self.removed_nsids.is_empty()
            && self.changed_nsids.is_empty()
            && self.added_variants.is_empty()
            && self.removed_variants.is_empty()
            && self.modified_variants.is_empty()
            && self.order_changes.is_empty()
            && self.added_recursion_points.is_empty()
            && self.removed_recursion_points.is_empty()
            && self.modified_recursion_points.is_empty()
            && self.usage_mode_changes.is_empty()
            && self.added_spans.is_empty()
            && self.removed_spans.is_empty()
            && self.modified_spans.is_empty()
            && self.nominal_changes.is_empty()
            && self.added_coercions.is_empty()
            && self.removed_coercions.is_empty()
            && self.modified_coercions.is_empty()
            && self.added_mergers.is_empty()
            && self.removed_mergers.is_empty()
            && self.modified_mergers.is_empty()
            && self.added_defaults.is_empty()
            && self.removed_defaults.is_empty()
            && self.modified_defaults.is_empty()
            && self.added_policies.is_empty()
            && self.removed_policies.is_empty()
            && self.modified_policies.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use panproto_gat::Name;
    use panproto_schema::{HyperEdge, RecursionPoint, Span, Variant, Vertex};
    use smallvec::SmallVec;
    use std::collections::HashMap;

    /// Helper to build a minimal test schema.
    fn test_schema(
        vertices: &[(&str, &str)],
        edges: &[Edge],
        constraints: HashMap<Name, Vec<Constraint>>,
    ) -> Schema {
        let mut vert_map = HashMap::new();
        let mut edge_map = HashMap::new();
        let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

        for (id, kind) in vertices {
            vert_map.insert(
                Name::from(*id),
                Vertex {
                    id: Name::from(*id),
                    kind: Name::from(*kind),
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
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing,
            incoming,
            between,
        }
    }

    /// Build a schema with additional extended fields set.
    fn test_schema_ext(base: Schema, f: impl FnOnce(&mut Schema)) -> Schema {
        let mut s = base;
        f(&mut s);
        s
    }

    // -----------------------------------------------------------------------
    // Existing tests (preserved)
    // -----------------------------------------------------------------------

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
            Name::from("x"),
            vec![Constraint {
                sort: "maxLength".into(),
                value: "3000".into(),
            }],
        )]);
        let new_constraints = HashMap::from([(
            Name::from("x"),
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

    // -----------------------------------------------------------------------
    // Hyper-edge diff tests
    // -----------------------------------------------------------------------

    #[test]
    fn diff_hyper_edge_added() {
        let base = test_schema(&[("a", "object")], &[], HashMap::new());
        let new = test_schema_ext(base.clone(), |s| {
            s.hyper_edges.insert(
                "he1".into(),
                HyperEdge {
                    id: "he1".into(),
                    kind: "join".into(),
                    signature: HashMap::from([("left".into(), "a".into())]),
                    parent_label: "left".into(),
                },
            );
        });
        let d = diff(&base, &new);
        assert_eq!(d.added_hyper_edges, vec!["he1"]);
        assert!(d.removed_hyper_edges.is_empty());
        assert!(!d.is_empty());
    }

    #[test]
    fn diff_hyper_edge_removed() {
        let old = test_schema_ext(test_schema(&[("a", "object")], &[], HashMap::new()), |s| {
            s.hyper_edges.insert(
                "he1".into(),
                HyperEdge {
                    id: "he1".into(),
                    kind: "join".into(),
                    signature: HashMap::from([("left".into(), "a".into())]),
                    parent_label: "left".into(),
                },
            );
        });
        let new = test_schema(&[("a", "object")], &[], HashMap::new());
        let d = diff(&old, &new);
        assert_eq!(d.removed_hyper_edges, vec!["he1"]);
    }

    #[test]
    fn diff_hyper_edge_modified_kind() {
        let he = HyperEdge {
            id: "he1".into(),
            kind: "join".into(),
            signature: HashMap::from([("left".into(), "a".into())]),
            parent_label: "left".into(),
        };
        let old = test_schema_ext(test_schema(&[("a", "object")], &[], HashMap::new()), |s| {
            s.hyper_edges.insert("he1".into(), he.clone());
        });
        let new = test_schema_ext(test_schema(&[("a", "object")], &[], HashMap::new()), |s| {
            let mut he2 = he.clone();
            he2.kind = "merge".into();
            s.hyper_edges.insert("he1".into(), he2);
        });
        let d = diff(&old, &new);
        assert_eq!(d.modified_hyper_edges.len(), 1);
        assert_eq!(
            d.modified_hyper_edges[0].kind_change,
            Some(("join".into(), "merge".into()))
        );
    }

    #[test]
    fn diff_hyper_edge_modified_signature() {
        let old = test_schema_ext(
            test_schema(&[("a", "object"), ("b", "string")], &[], HashMap::new()),
            |s| {
                s.hyper_edges.insert(
                    "he1".into(),
                    HyperEdge {
                        id: "he1".into(),
                        kind: "join".into(),
                        signature: HashMap::from([("left".into(), "a".into())]),
                        parent_label: "left".into(),
                    },
                );
            },
        );
        let new = test_schema_ext(
            test_schema(&[("a", "object"), ("b", "string")], &[], HashMap::new()),
            |s| {
                s.hyper_edges.insert(
                    "he1".into(),
                    HyperEdge {
                        id: "he1".into(),
                        kind: "join".into(),
                        signature: HashMap::from([
                            ("left".into(), "a".into()),
                            ("right".into(), "b".into()),
                        ]),
                        parent_label: "left".into(),
                    },
                );
            },
        );
        let d = diff(&old, &new);
        assert_eq!(d.modified_hyper_edges.len(), 1);
        assert_eq!(
            d.modified_hyper_edges[0].signature_added.get("right"),
            Some(&"b".to_string())
        );
    }

    // -----------------------------------------------------------------------
    // Required-edge diff tests
    // -----------------------------------------------------------------------

    #[test]
    fn diff_required_edge_added() {
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        let base = test_schema(
            &[("a", "object"), ("b", "string")],
            std::slice::from_ref(&edge),
            HashMap::new(),
        );
        let new = test_schema_ext(base.clone(), |s| {
            s.required.insert("a".into(), vec![edge.clone()]);
        });
        let d = diff(&base, &new);
        assert_eq!(d.added_required.len(), 1);
        assert_eq!(d.added_required["a"].len(), 1);
    }

    #[test]
    fn diff_required_edge_removed() {
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        let old = test_schema_ext(
            test_schema(
                &[("a", "object"), ("b", "string")],
                std::slice::from_ref(&edge),
                HashMap::new(),
            ),
            |s| {
                s.required.insert("a".into(), vec![edge.clone()]);
            },
        );
        let new = test_schema(&[("a", "object"), ("b", "string")], &[edge], HashMap::new());
        let d = diff(&old, &new);
        assert_eq!(d.removed_required.len(), 1);
        assert_eq!(d.removed_required["a"].len(), 1);
    }

    // -----------------------------------------------------------------------
    // NSID diff tests
    // -----------------------------------------------------------------------

    #[test]
    fn diff_nsid_added() {
        let base = test_schema(&[("a", "object")], &[], HashMap::new());
        let new = test_schema_ext(base.clone(), |s| {
            s.nsids.insert("a".into(), "com.example.thing".into());
        });
        let d = diff(&base, &new);
        assert_eq!(
            d.added_nsids.get("a"),
            Some(&"com.example.thing".to_string())
        );
    }

    #[test]
    fn diff_nsid_removed() {
        let old = test_schema_ext(test_schema(&[("a", "object")], &[], HashMap::new()), |s| {
            s.nsids.insert("a".into(), "com.example.thing".into());
        });
        let new = test_schema(&[("a", "object")], &[], HashMap::new());
        let d = diff(&old, &new);
        assert_eq!(d.removed_nsids, vec!["a"]);
    }

    #[test]
    fn diff_nsid_changed() {
        let old = test_schema_ext(test_schema(&[("a", "object")], &[], HashMap::new()), |s| {
            s.nsids.insert("a".into(), "com.example.old".into());
        });
        let new = test_schema_ext(test_schema(&[("a", "object")], &[], HashMap::new()), |s| {
            s.nsids.insert("a".into(), "com.example.new".into());
        });
        let d = diff(&old, &new);
        assert_eq!(d.changed_nsids.len(), 1);
        assert_eq!(
            d.changed_nsids[0],
            (
                "a".into(),
                "com.example.old".into(),
                "com.example.new".into()
            )
        );
    }

    // -----------------------------------------------------------------------
    // Variant diff tests
    // -----------------------------------------------------------------------

    #[test]
    fn diff_variant_tag_modified() {
        let old = test_schema_ext(test_schema(&[("u", "union")], &[], HashMap::new()), |s| {
            s.variants.insert(
                "u".into(),
                vec![Variant {
                    id: "v1".into(),
                    parent_vertex: "u".into(),
                    tag: Some("a".into()),
                }],
            );
        });
        let new = test_schema_ext(test_schema(&[("u", "union")], &[], HashMap::new()), |s| {
            s.variants.insert(
                "u".into(),
                vec![Variant {
                    id: "v1".into(),
                    parent_vertex: "u".into(),
                    tag: Some("b".into()),
                }],
            );
        });
        let d = diff(&old, &new);
        assert!(d.added_variants.is_empty());
        assert!(d.removed_variants.is_empty());
        assert_eq!(d.modified_variants.len(), 1);
        assert_eq!(d.modified_variants[0].old_tag, Some("a".into()));
        assert_eq!(d.modified_variants[0].new_tag, Some("b".into()));
    }

    // -----------------------------------------------------------------------
    // Recursion point diff tests
    // -----------------------------------------------------------------------

    #[test]
    fn diff_recursion_point_target_modified() {
        let old = test_schema_ext(
            test_schema(
                &[("a", "object"), ("b", "string"), ("c", "integer")],
                &[],
                HashMap::new(),
            ),
            |s| {
                s.recursion_points.insert(
                    "mu1".into(),
                    RecursionPoint {
                        mu_id: "mu1".into(),
                        target_vertex: "b".into(),
                    },
                );
            },
        );
        let new = test_schema_ext(
            test_schema(
                &[("a", "object"), ("b", "string"), ("c", "integer")],
                &[],
                HashMap::new(),
            ),
            |s| {
                s.recursion_points.insert(
                    "mu1".into(),
                    RecursionPoint {
                        mu_id: "mu1".into(),
                        target_vertex: "c".into(),
                    },
                );
            },
        );
        let d = diff(&old, &new);
        assert!(d.added_recursion_points.is_empty());
        assert!(d.removed_recursion_points.is_empty());
        assert_eq!(d.modified_recursion_points.len(), 1);
        assert_eq!(d.modified_recursion_points[0].old_target, "b");
        assert_eq!(d.modified_recursion_points[0].new_target, "c");
    }

    // -----------------------------------------------------------------------
    // Span diff tests
    // -----------------------------------------------------------------------

    #[test]
    fn diff_span_added() {
        let base = test_schema(&[("a", "object"), ("b", "string")], &[], HashMap::new());
        let new = test_schema_ext(base.clone(), |s| {
            s.spans.insert(
                "s1".into(),
                Span {
                    id: "s1".into(),
                    left: "a".into(),
                    right: "b".into(),
                },
            );
        });
        let d = diff(&base, &new);
        assert_eq!(d.added_spans, vec!["s1"]);
    }

    #[test]
    fn diff_span_modified() {
        let old = test_schema_ext(
            test_schema(
                &[("a", "object"), ("b", "string"), ("c", "integer")],
                &[],
                HashMap::new(),
            ),
            |s| {
                s.spans.insert(
                    "s1".into(),
                    Span {
                        id: "s1".into(),
                        left: "a".into(),
                        right: "b".into(),
                    },
                );
            },
        );
        let new = test_schema_ext(
            test_schema(
                &[("a", "object"), ("b", "string"), ("c", "integer")],
                &[],
                HashMap::new(),
            ),
            |s| {
                s.spans.insert(
                    "s1".into(),
                    Span {
                        id: "s1".into(),
                        left: "a".into(),
                        right: "c".into(),
                    },
                );
            },
        );
        let d = diff(&old, &new);
        assert_eq!(d.modified_spans.len(), 1);
        assert_eq!(
            d.modified_spans[0].right_change,
            Some(("b".into(), "c".into()))
        );
        assert_eq!(d.modified_spans[0].left_change, None);
    }

    // -----------------------------------------------------------------------
    // Nominal diff tests
    // -----------------------------------------------------------------------

    #[test]
    fn diff_nominal_changed() {
        let old = test_schema_ext(test_schema(&[("a", "object")], &[], HashMap::new()), |s| {
            s.nominal.insert("a".into(), false);
        });
        let new = test_schema_ext(test_schema(&[("a", "object")], &[], HashMap::new()), |s| {
            s.nominal.insert("a".into(), true);
        });
        let d = diff(&old, &new);
        assert_eq!(d.nominal_changes.len(), 1);
        assert_eq!(d.nominal_changes[0], ("a".into(), false, true));
    }

    // -----------------------------------------------------------------------
    // is_empty comprehensive test
    // -----------------------------------------------------------------------

    #[test]
    #[allow(clippy::too_many_lines)]
    fn is_empty_false_for_each_field() {
        let base = test_schema(&[("a", "object")], &[], HashMap::new());

        // Each of these should make is_empty() return false.
        let cases: Vec<SchemaDiff> = vec![
            SchemaDiff {
                added_vertices: vec!["x".into()],
                ..Default::default()
            },
            SchemaDiff {
                removed_vertices: vec!["x".into()],
                ..Default::default()
            },
            SchemaDiff {
                kind_changes: vec![KindChange {
                    vertex_id: "a".into(),
                    old_kind: "x".into(),
                    new_kind: "y".into(),
                }],
                ..Default::default()
            },
            SchemaDiff {
                added_edges: vec![Edge {
                    src: "a".into(),
                    tgt: "b".into(),
                    kind: "p".into(),
                    name: None,
                }],
                ..Default::default()
            },
            SchemaDiff {
                removed_edges: vec![Edge {
                    src: "a".into(),
                    tgt: "b".into(),
                    kind: "p".into(),
                    name: None,
                }],
                ..Default::default()
            },
            SchemaDiff {
                modified_constraints: HashMap::from([(
                    "a".into(),
                    ConstraintDiff {
                        added: vec![Constraint {
                            sort: "s".into(),
                            value: "v".into(),
                        }],
                        removed: vec![],
                        changed: vec![],
                    },
                )]),
                ..Default::default()
            },
            SchemaDiff {
                added_hyper_edges: vec!["he".into()],
                ..Default::default()
            },
            SchemaDiff {
                removed_hyper_edges: vec!["he".into()],
                ..Default::default()
            },
            SchemaDiff {
                modified_hyper_edges: vec![HyperEdgeChange {
                    id: "he".into(),
                    kind_change: None,
                    signature_added: HashMap::new(),
                    signature_removed: HashMap::new(),
                    signature_changed: HashMap::new(),
                    parent_label_change: Some(("a".into(), "b".into())),
                }],
                ..Default::default()
            },
            SchemaDiff {
                added_required: HashMap::from([("a".into(), vec![])]),
                ..Default::default()
            },
            SchemaDiff {
                removed_required: HashMap::from([("a".into(), vec![])]),
                ..Default::default()
            },
            SchemaDiff {
                added_nsids: HashMap::from([("a".into(), "x".into())]),
                ..Default::default()
            },
            SchemaDiff {
                removed_nsids: vec!["a".into()],
                ..Default::default()
            },
            SchemaDiff {
                changed_nsids: vec![("a".into(), "x".into(), "y".into())],
                ..Default::default()
            },
            SchemaDiff {
                added_variants: vec![Variant {
                    id: "v".into(),
                    parent_vertex: "u".into(),
                    tag: None,
                }],
                ..Default::default()
            },
            SchemaDiff {
                removed_variants: vec![Variant {
                    id: "v".into(),
                    parent_vertex: "u".into(),
                    tag: None,
                }],
                ..Default::default()
            },
            SchemaDiff {
                modified_variants: vec![VariantChange {
                    id: "v".into(),
                    parent_vertex: "u".into(),
                    old_tag: None,
                    new_tag: Some("t".into()),
                }],
                ..Default::default()
            },
            SchemaDiff {
                order_changes: vec![(
                    Edge {
                        src: "a".into(),
                        tgt: "b".into(),
                        kind: "p".into(),
                        name: None,
                    },
                    Some(0),
                    Some(1),
                )],
                ..Default::default()
            },
            SchemaDiff {
                added_recursion_points: vec![RecursionPoint {
                    mu_id: "m".into(),
                    target_vertex: "t".into(),
                }],
                ..Default::default()
            },
            SchemaDiff {
                removed_recursion_points: vec![RecursionPoint {
                    mu_id: "m".into(),
                    target_vertex: "t".into(),
                }],
                ..Default::default()
            },
            SchemaDiff {
                modified_recursion_points: vec![RecursionPointChange {
                    mu_id: "m".into(),
                    old_target: "a".into(),
                    new_target: "b".into(),
                }],
                ..Default::default()
            },
            SchemaDiff {
                usage_mode_changes: vec![(
                    Edge {
                        src: "a".into(),
                        tgt: "b".into(),
                        kind: "p".into(),
                        name: None,
                    },
                    UsageMode::Structural,
                    UsageMode::Linear,
                )],
                ..Default::default()
            },
            SchemaDiff {
                added_spans: vec!["s".into()],
                ..Default::default()
            },
            SchemaDiff {
                removed_spans: vec!["s".into()],
                ..Default::default()
            },
            SchemaDiff {
                modified_spans: vec![SpanChange {
                    id: "s".into(),
                    left_change: Some(("a".into(), "b".into())),
                    right_change: None,
                }],
                ..Default::default()
            },
            SchemaDiff {
                nominal_changes: vec![("a".into(), false, true)],
                ..Default::default()
            },
            SchemaDiff {
                added_coercions: vec![("a".into(), "b".into())],
                ..Default::default()
            },
            SchemaDiff {
                removed_coercions: vec![("a".into(), "b".into())],
                ..Default::default()
            },
            SchemaDiff {
                modified_coercions: vec![("a".into(), "b".into())],
                ..Default::default()
            },
            SchemaDiff {
                added_mergers: vec!["a".into()],
                ..Default::default()
            },
            SchemaDiff {
                removed_mergers: vec!["a".into()],
                ..Default::default()
            },
            SchemaDiff {
                modified_mergers: vec!["a".into()],
                ..Default::default()
            },
            SchemaDiff {
                added_defaults: vec!["a".into()],
                ..Default::default()
            },
            SchemaDiff {
                removed_defaults: vec!["a".into()],
                ..Default::default()
            },
            SchemaDiff {
                modified_defaults: vec!["a".into()],
                ..Default::default()
            },
            SchemaDiff {
                added_policies: vec!["a".into()],
                ..Default::default()
            },
            SchemaDiff {
                removed_policies: vec!["a".into()],
                ..Default::default()
            },
            SchemaDiff {
                modified_policies: vec!["a".into()],
                ..Default::default()
            },
        ];

        let _ = base; // suppress unused warning
        for (i, d) in cases.iter().enumerate() {
            assert!(!d.is_empty(), "case {i} should not be empty: {d:?}");
        }
    }
}
