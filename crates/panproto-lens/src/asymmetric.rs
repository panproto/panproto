//! Asymmetric lens operations: `get` and `put` with complement tracking.
//!
//! The `get` direction runs the restrict pipeline (forward migration) while
//! capturing everything that was discarded into a [`Complement`]. The `put`
//! direction restores the original source instance from a (possibly modified)
//! view plus the complement.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::{Fan, Node, WInstance, wtype_restrict};
use panproto_schema::Edge;
use serde::{Deserialize, Serialize};

use crate::Lens;
use crate::error::LensError;

/// The complement: data discarded by `get`, needed by `put` to restore the
/// original source instance.
///
/// When `get` projects a source instance to a target view, some nodes, arcs,
/// and structural decisions are lost. The complement records all of this so
/// that `put` can reconstruct the full source.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Complement {
    /// Nodes from the source that do not appear in the target view.
    pub dropped_nodes: HashMap<u32, Node>,
    /// Arcs from the source that do not appear in the target view.
    pub dropped_arcs: Vec<(u32, u32, Edge)>,
    /// Fans from the source whose parent or children were dropped during `get`.
    pub dropped_fans: Vec<Fan>,
    /// Resolver decisions made during ancestor contraction.
    pub contraction_choices: HashMap<(u32, u32), Edge>,
    /// Original parent mapping before contraction.
    pub original_parent: HashMap<u32, u32>,
    /// Fingerprint of the source schema at `get` time, used by `put` to
    /// validate that the complement matches the lens's source schema.
    #[serde(default)]
    pub source_fingerprint: u64,
    /// Pre-transform `extra_fields` for nodes that had `field_transforms` applied.
    /// Used by `put` to restore original field values.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub original_extra_fields: HashMap<u32, HashMap<String, panproto_inst::value::Value>>,
    /// Exact edge used for every arc in the view, keyed by `(parent_id, child_id)`.
    /// This makes `put` deterministic when the source schema has parallel edges
    /// between the same vertex pair, ensuring the cartesian lift is unique.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub arc_edges: HashMap<(u32, u32), Edge>,
}

impl Complement {
    /// Create an empty complement (no data discarded).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            dropped_nodes: HashMap::new(),
            dropped_arcs: Vec::new(),
            dropped_fans: Vec::new(),
            contraction_choices: HashMap::new(),
            original_parent: HashMap::new(),
            source_fingerprint: 0,
            original_extra_fields: HashMap::new(),
            arc_edges: HashMap::new(),
        }
    }

    /// Returns `true` if the complement is empty (lossless transformation).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.dropped_nodes.is_empty()
            && self.dropped_arcs.is_empty()
            && self.dropped_fans.is_empty()
            && self.contraction_choices.is_empty()
            && self.original_parent.is_empty()
            && self.original_extra_fields.is_empty()
            && self.arc_edges.is_empty()
    }
}

/// Compute a fingerprint of a schema for complement provenance validation.
fn schema_fingerprint(schema: &panproto_schema::Schema) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    let mut vert_names: Vec<&str> = schema.vertices.keys().map(|n| &**n).collect();
    vert_names.sort_unstable();
    for v in &vert_names {
        v.hash(&mut hasher);
    }
    schema.edges.len().hash(&mut hasher);
    hasher.finish()
}

/// Forward lens direction: restrict the source instance to the target view
/// and capture the complement.
///
/// This runs `wtype_restrict` and then computes the set difference between
/// the source and result to populate the complement.
///
/// # Errors
///
/// Returns `LensError::Restrict` if the underlying restrict pipeline fails.
pub fn get(lens: &Lens, instance: &WInstance) -> Result<(WInstance, Complement), LensError> {
    let view = wtype_restrict(instance, &lens.src_schema, &lens.tgt_schema, &lens.compiled)?;

    // Compute complement: everything in source not in view
    let mut dropped_nodes = HashMap::new();
    for (&id, node) in &instance.nodes {
        if !view.nodes.contains_key(&id) {
            dropped_nodes.insert(id, node.clone());
        }
    }

    let mut dropped_arcs = Vec::new();
    for arc in &instance.arcs {
        let (parent, child, _) = arc;
        if !view.nodes.contains_key(parent) || !view.nodes.contains_key(child) {
            dropped_arcs.push(arc.clone());
        }
    }

    // Capture contraction choices: for each arc in the view that connects
    // nodes that were not directly connected in the source, record the
    // resolver decision.
    let mut contraction_choices = HashMap::new();
    for (parent, child, edge) in &view.arcs {
        // Check if this arc existed in the original
        let was_direct = instance
            .arcs
            .iter()
            .any(|(p, c, _)| p == parent && c == child);
        if !was_direct {
            contraction_choices.insert((*parent, *child), edge.clone());
        }
    }

    // Capture original parent mapping for all surviving nodes
    let mut original_parent = HashMap::new();
    for &id in view.nodes.keys() {
        if let Some(&parent) = instance.parent_map.get(&id) {
            original_parent.insert(id, parent);
        }
    }

    // Capture fans whose parent or any children were dropped
    let mut dropped_fans = Vec::new();
    for fan in &instance.fans {
        let parent_survives = view.nodes.contains_key(&fan.parent);
        let all_children_survive = fan
            .children
            .values()
            .all(|node_id| view.nodes.contains_key(node_id));
        if !parent_survives || !all_children_survive {
            dropped_fans.push(fan.clone());
        }
    }

    // Capture pre-transform extra_fields for nodes that had field_transforms
    let mut original_extra_fields = HashMap::new();
    for &id in view.nodes.keys() {
        if let Some(source_node) = instance.nodes.get(&id) {
            if lens
                .compiled
                .field_transforms
                .contains_key(&source_node.anchor)
            {
                original_extra_fields.insert(id, source_node.extra_fields.clone());
            }
        }
    }

    // Record the exact edge for every arc in the source instance whose
    // parent and child both survive. This makes `put` deterministic when
    // the source schema has parallel edges between the same vertex pair.
    let mut arc_edges = HashMap::new();
    for (parent, child, edge) in &instance.arcs {
        if view.nodes.contains_key(parent) && view.nodes.contains_key(child) {
            arc_edges.insert((*parent, *child), edge.clone());
        }
    }

    let source_fingerprint = schema_fingerprint(&lens.src_schema);

    let complement = Complement {
        dropped_nodes,
        dropped_arcs,
        dropped_fans,
        contraction_choices,
        original_parent,
        source_fingerprint,
        original_extra_fields,
        arc_edges,
    };

    Ok((view, complement))
}

/// Backward lens direction: restore a source instance from a (possibly
/// modified) view and the complement.
///
/// The complement provides the dropped nodes and arcs; the view provides
/// the (potentially modified) surviving data. Together they reconstruct
/// the full source instance.
///
/// # Errors
///
/// Returns `LensError::ComplementMismatch` if the complement is inconsistent
/// with the view.
pub fn put(lens: &Lens, view: &WInstance, complement: &Complement) -> Result<WInstance, LensError> {
    // Validate complement provenance: the complement must have been produced
    // from the same source schema.
    if complement.source_fingerprint != 0 {
        let expected = schema_fingerprint(&lens.src_schema);
        if complement.source_fingerprint != expected {
            return Err(LensError::ComplementMismatch {
                detail: format!(
                    "source fingerprint mismatch: complement has {}, lens expects {}",
                    complement.source_fingerprint, expected
                ),
            });
        }
    }

    // Start with all nodes from the view (un-remap anchors back to source)
    let mut nodes = HashMap::new();
    let reverse_remap = build_reverse_remap(&lens.compiled.vertex_remap);

    for (&id, node) in &view.nodes {
        let mut restored_node = node.clone();
        if let Some(original_anchor) = reverse_remap.get(&node.anchor) {
            restored_node.anchor.clone_from(original_anchor);
        }
        // Restore original extra_fields if this node had field_transforms applied
        if let Some(original_fields) = complement.original_extra_fields.get(&id) {
            restored_node.extra_fields.clone_from(original_fields);
        }
        nodes.insert(id, restored_node);
    }

    // Re-insert dropped nodes from complement
    for (&id, node) in &complement.dropped_nodes {
        nodes.insert(id, node.clone());
    }

    // Rebuild arcs: start with original parent relationships for view nodes,
    // then add back dropped arcs
    let mut arcs = Vec::new();

    // For view nodes, use the original parent mapping to restore arcs
    for (&child_id, &original_parent) in &complement.original_parent {
        if !nodes.contains_key(&child_id) || child_id == view.root {
            continue;
        }
        // Find the original arc for this parent-child pair, consulting
        // contraction_choices for disambiguation when multiple edges exist.
        if let Some(arc) = find_original_arc(
            &lens.src_schema,
            &nodes,
            original_parent,
            child_id,
            &complement.contraction_choices,
            &complement.arc_edges,
        ) {
            arcs.push(arc);
        }
    }

    // Add dropped arcs back (they connect dropped nodes)
    for arc in &complement.dropped_arcs {
        let (parent, child, _) = arc;
        if nodes.contains_key(parent) && nodes.contains_key(child) {
            arcs.push(arc.clone());
        }
    }

    // Reconstruct fans: start with the view's fans (un-remapping vertex
    // references), then re-insert dropped fans from the complement whose
    // parent and children are all present in the restored node set.
    let mut fans: Vec<Fan> = view
        .fans
        .iter()
        .map(|fan| {
            let mut restored_fan = fan.clone();
            // Un-remap the hyper-edge ID if needed
            if let Some(original_he) = reverse_remap.get(fan.hyper_edge_id.as_str()) {
                restored_fan.hyper_edge_id = original_he.to_string();
            }
            restored_fan
        })
        .collect();

    // Re-insert dropped fans whose nodes are all present after restoration
    for fan in &complement.dropped_fans {
        let parent_present = nodes.contains_key(&fan.parent);
        let all_children_present = fan
            .children
            .values()
            .all(|node_id| nodes.contains_key(node_id));
        if parent_present && all_children_present {
            fans.push(fan.clone());
        }
    }

    let schema_root = reverse_remap
        .get(&view.schema_root)
        .cloned()
        .unwrap_or_else(|| view.schema_root.clone());
    // schema_root is Name, which WInstance::new accepts via Into<Name>

    Ok(WInstance::new(nodes, arcs, fans, view.root, schema_root))
}

/// Build a reverse mapping from target vertex IDs back to source vertex IDs.
fn build_reverse_remap(forward: &HashMap<Name, Name>) -> HashMap<Name, Name> {
    forward
        .iter()
        .map(|(k, v)| (v.clone(), k.clone()))
        .collect()
}

/// Find the original arc between a parent and child in the source schema.
///
/// Consults `arc_edges` (exact edge recorded during `get`) first, then
/// `contraction_choices`, then falls back to the source schema. When the
/// complement was produced by this module's `get`, `arc_edges` will always
/// contain the exact edge, making this function deterministic even when
/// the source schema has parallel edges.
fn find_original_arc(
    src_schema: &panproto_schema::Schema,
    nodes: &HashMap<u32, Node>,
    parent_id: u32,
    child_id: u32,
    contraction_choices: &HashMap<(u32, u32), Edge>,
    arc_edges: &HashMap<(u32, u32), Edge>,
) -> Option<(u32, u32, Edge)> {
    // Exact edge recorded during get: deterministic.
    if let Some(edge) = arc_edges.get(&(parent_id, child_id)) {
        return Some((parent_id, child_id, edge.clone()));
    }

    // Contraction choice: edges created by ancestor contraction.
    if let Some(edge) = contraction_choices.get(&(parent_id, child_id)) {
        return Some((parent_id, child_id, edge.clone()));
    }

    // Fallback: look up in the source schema (backward compat for old complements).
    let parent_node = nodes.get(&parent_id)?;
    let child_node = nodes.get(&child_id)?;

    let edges = src_schema.edges_between(&parent_node.anchor, &child_node.anchor);
    if edges.len() == 1 {
        Some((parent_id, child_id, edges[0].clone()))
    } else {
        edges.first().map(|e| (parent_id, child_id, e.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{identity_lens, three_node_instance, three_node_schema};

    #[test]
    fn get_with_identity_lens_produces_empty_complement() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        let (view, complement) =
            get(&lens, &instance).unwrap_or_else(|e| panic!("get failed: {e}"));
        assert_eq!(view.node_count(), instance.node_count());
        assert!(
            complement.dropped_nodes.is_empty(),
            "no nodes should be dropped by identity lens"
        );
    }

    #[test]
    fn get_then_put_round_trips_identity() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        let (view, complement) =
            get(&lens, &instance).unwrap_or_else(|e| panic!("get failed: {e}"));
        let restored = put(&lens, &view, &complement).unwrap_or_else(|e| panic!("put failed: {e}"));

        assert_eq!(
            restored.node_count(),
            instance.node_count(),
            "restored should have same node count"
        );
        assert_eq!(restored.root, instance.root, "restored root should match");
    }

    #[test]
    fn complement_is_empty_for_identity() {
        let complement = Complement::empty();
        assert!(complement.is_empty());
    }
}
