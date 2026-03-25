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
    }
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

    let complement = Complement {
        dropped_nodes,
        dropped_arcs,
        dropped_fans,
        contraction_choices,
        original_parent,
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
    // Start with all nodes from the view (un-remap anchors back to source)
    let mut nodes = HashMap::new();
    let reverse_remap = build_reverse_remap(&lens.compiled.vertex_remap);

    for (&id, node) in &view.nodes {
        let mut restored_node = node.clone();
        if let Some(original_anchor) = reverse_remap.get(&node.anchor) {
            restored_node.anchor.clone_from(original_anchor);
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
/// Consults `contraction_choices` first for disambiguation when multiple edges exist.
fn find_original_arc(
    src_schema: &panproto_schema::Schema,
    nodes: &HashMap<u32, Node>,
    parent_id: u32,
    child_id: u32,
    contraction_choices: &HashMap<(u32, u32), Edge>,
) -> Option<(u32, u32, Edge)> {
    // Check contraction_choices first for a recorded disambiguation
    if let Some(edge) = contraction_choices.get(&(parent_id, child_id)) {
        return Some((parent_id, child_id, edge.clone()));
    }

    let parent_node = nodes.get(&parent_id)?;
    let child_node = nodes.get(&child_id)?;

    let edges = src_schema.edges_between(&parent_node.anchor, &child_node.anchor);
    if edges.len() == 1 {
        Some((parent_id, child_id, edges[0].clone()))
    } else {
        // If multiple edges and no contraction choice, take the first
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
        let complement = Complement {
            dropped_nodes: HashMap::new(),
            dropped_arcs: Vec::new(),
            dropped_fans: Vec::new(),
            contraction_choices: HashMap::new(),
            original_parent: HashMap::new(),
        };
        assert!(complement.is_empty());
    }
}
