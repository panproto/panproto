//! W-type instance representation and the `wtype_restrict` pipeline.
//!
//! A [`WInstance`] is a tree-shaped data instance conforming to a schema.
//! The restrict operation (`wtype_restrict`) is a 5-step pipeline that
//! projects a W-type instance along a migration mapping.
//!
//! The five steps are independently testable functions:
//! 1. `anchor_surviving` — signature restriction
//! 2. `reachable_from_root` — BFS reachability
//! 3. `ancestor_contraction` — find nearest surviving ancestors
//! 4. `resolve_edge` — edge resolution for contracted arcs
//! 5. `reconstruct_fans` — hyper-edge fan reconstruction

use std::collections::{HashMap, HashSet, VecDeque};

use panproto_schema::{Edge, Schema};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::error::RestrictError;
use crate::fan::Fan;
use crate::metadata::Node;

/// A compiled migration specification (minimal version for panproto-inst).
///
/// The full `CompiledMigration` lives in `panproto-mig`. This type provides
/// the subset of fields that `wtype_restrict` and `functor_restrict` need.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompiledMigration {
    /// Vertices that survive the migration.
    pub surviving_verts: HashSet<String>,
    /// Edges that survive the migration.
    pub surviving_edges: HashSet<Edge>,
    /// Vertex remapping: source vertex ID to target vertex ID.
    pub vertex_remap: HashMap<String, String>,
    /// Edge remapping: source edge to target edge.
    pub edge_remap: HashMap<Edge, Edge>,
    /// Binary contraction resolver: (`src_anchor`, `tgt_anchor`) to resolved edge.
    pub resolver: HashMap<(String, String), Edge>,
    /// Hyper-edge contraction resolver.
    pub hyper_resolver: HashMap<String, (String, HashMap<String, String>)>,
}

/// A W-type instance: tree-shaped data conforming to a schema.
///
/// Nodes are anchored to schema vertices, connected by arcs that
/// correspond to schema edges. The tree is rooted at `root`.
/// Precomputed `parent_map` and `children_map` enable fast traversal.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WInstance {
    /// All nodes keyed by their numeric ID.
    pub nodes: HashMap<u32, Node>,
    /// Arcs: (`parent_id`, `child_id`, `schema_edge`).
    pub arcs: Vec<(u32, u32, Edge)>,
    /// Hyper-edge fans.
    pub fans: Vec<Fan>,
    /// Root node ID.
    pub root: u32,
    /// Schema vertex that the root node is anchored to.
    pub schema_root: String,
    /// Precomputed parent map: `child_id` -> `parent_id`.
    pub parent_map: HashMap<u32, u32>,
    /// Precomputed children map: `parent_id` -> child IDs.
    pub children_map: HashMap<u32, SmallVec<u32, 4>>,
}

impl WInstance {
    /// Build a new W-type instance, computing parent and children maps from arcs.
    #[must_use]
    pub fn new(
        nodes: HashMap<u32, Node>,
        arcs: Vec<(u32, u32, Edge)>,
        fans: Vec<Fan>,
        root: u32,
        schema_root: String,
    ) -> Self {
        let mut parent_map = HashMap::new();
        let mut children_map: HashMap<u32, SmallVec<u32, 4>> = HashMap::new();
        for &(parent, child, _) in &arcs {
            parent_map.insert(child, parent);
            children_map.entry(parent).or_default().push(child);
        }
        Self {
            nodes,
            arcs,
            fans,
            root,
            schema_root,
            parent_map,
            children_map,
        }
    }

    /// Returns the number of nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the number of arcs.
    #[must_use]
    pub fn arc_count(&self) -> usize {
        self.arcs.len()
    }

    /// Get a node by ID.
    #[must_use]
    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Get the children of a node.
    #[must_use]
    pub fn children(&self, id: u32) -> &[u32] {
        self.children_map.get(&id).map_or(&[], SmallVec::as_slice)
    }

    /// Get the parent of a node.
    #[must_use]
    pub fn parent(&self, id: u32) -> Option<u32> {
        self.parent_map.get(&id).copied()
    }
}

// ---------------------------------------------------------------------------
// Step 1: Signature restriction
// ---------------------------------------------------------------------------

/// Keep nodes whose anchor vertex is in the surviving vertex set.
///
/// This is the first step of the restrict pipeline: it selects the
/// subset of instance nodes whose schema anchor is retained by the
/// migration.
#[must_use]
pub fn anchor_surviving(instance: &WInstance, surviving_verts: &HashSet<String>) -> HashSet<u32> {
    instance
        .nodes
        .iter()
        .filter(|(_, node)| surviving_verts.contains(&node.anchor))
        .map(|(&id, _)| id)
        .collect()
}

// ---------------------------------------------------------------------------
// Step 2: Reachability BFS
// ---------------------------------------------------------------------------

/// BFS from the root through anchor-surviving nodes.
///
/// Starting from the root (if it is in `candidates`), traverse children
/// that are also in `candidates`. Returns the set of reachable node IDs.
#[must_use]
pub fn reachable_from_root(instance: &WInstance, candidates: &HashSet<u32>) -> HashSet<u32> {
    let mut reachable = HashSet::new();
    if !candidates.contains(&instance.root) {
        return reachable;
    }

    let mut queue = VecDeque::new();
    queue.push_back(instance.root);
    reachable.insert(instance.root);

    while let Some(current) = queue.pop_front() {
        for &child in instance.children(current) {
            if candidates.contains(&child) && reachable.insert(child) {
                queue.push_back(child);
            }
        }
    }

    reachable
}

// ---------------------------------------------------------------------------
// Step 3: Ancestor contraction
// ---------------------------------------------------------------------------

/// For each surviving non-root node, find its nearest surviving ancestor.
///
/// When intermediate nodes are pruned, children must be re-attached to
/// the nearest surviving ancestor. This function walks up the parent
/// chain for each surviving node to find that ancestor.
#[must_use]
pub fn ancestor_contraction(instance: &WInstance, surviving: &HashSet<u32>) -> HashMap<u32, u32> {
    let mut ancestors = HashMap::new();
    for &node_id in surviving {
        if node_id == instance.root {
            continue;
        }
        // Walk up to find nearest surviving ancestor, with visited set to guard against cycles
        let mut current = node_id;
        let mut visited = HashSet::new();
        visited.insert(node_id);
        while let Some(parent) = instance.parent(current) {
            if !visited.insert(parent) {
                // Cycle detected — stop walking
                break;
            }
            if surviving.contains(&parent) {
                ancestors.insert(node_id, parent);
                break;
            }
            current = parent;
        }
    }
    ancestors
}

// ---------------------------------------------------------------------------
// Step 4: Edge resolution
// ---------------------------------------------------------------------------

/// Resolve the edge for a contracted arc in the target schema.
///
/// Given a (source, target) vertex pair in the target schema, find the
/// unique edge. Consults the resolver first (for ambiguous cases), then
/// falls back to the unique-edge heuristic.
///
/// # Errors
///
/// Returns `RestrictError::NoEdgeFound` if no edge exists, or
/// `RestrictError::AmbiguousEdge` if multiple edges exist without
/// a resolver entry.
pub fn resolve_edge(
    tgt_schema: &Schema,
    resolver: &HashMap<(String, String), Edge>,
    src_v: &str,
    tgt_v: &str,
) -> Result<Edge, RestrictError> {
    // Check resolver first
    let key = (src_v.to_string(), tgt_v.to_string());
    if let Some(edge) = resolver.get(&key) {
        return Ok(edge.clone());
    }

    // Fall back to unique-edge lookup
    let candidates = tgt_schema.edges_between(src_v, tgt_v);
    match candidates.len() {
        0 => Err(RestrictError::NoEdgeFound {
            src: src_v.to_string(),
            tgt: tgt_v.to_string(),
        }),
        1 => Ok(candidates[0].clone()),
        n => Err(RestrictError::AmbiguousEdge {
            src: src_v.to_string(),
            tgt: tgt_v.to_string(),
            count: n,
        }),
    }
}

// ---------------------------------------------------------------------------
// Step 5: Fan reconstruction
// ---------------------------------------------------------------------------

/// Reconstruct fans after restriction.
///
/// For hyper-edge schemas, fans whose children are partially pruned
/// need to be rebuilt with only the surviving children. If the
/// hyper-resolver provides a mapping, use it; otherwise, build a
/// reduced fan from the surviving children.
///
/// # Errors
///
/// Returns `RestrictError::FanReconstructionFailed` if a fan cannot
/// be validly reconstructed.
pub fn reconstruct_fans(
    instance: &WInstance,
    surviving: &HashSet<u32>,
    _ancestors: &HashMap<u32, u32>,
    migration: &CompiledMigration,
    _tgt_schema: &Schema,
) -> Result<Vec<Fan>, RestrictError> {
    let mut result = Vec::new();

    for fan in &instance.fans {
        // Check if the parent survives
        if !surviving.contains(&fan.parent) {
            continue;
        }

        // Collect surviving children
        let surviving_children: HashMap<String, u32> = fan
            .children
            .iter()
            .filter(|(_, node_id)| surviving.contains(node_id))
            .map(|(label, node_id)| (label.clone(), *node_id))
            .collect();

        if surviving_children.is_empty() {
            continue;
        }

        // Check if the hyper-resolver has a mapping for this fan
        if let Some((new_he_id, label_map)) = migration.hyper_resolver.get(&fan.hyper_edge_id) {
            let mut new_children = HashMap::new();
            for (old_label, &node_id) in &surviving_children {
                let new_label = label_map
                    .get(old_label)
                    .cloned()
                    .unwrap_or_else(|| old_label.clone());
                new_children.insert(new_label, node_id);
            }
            result.push(Fan {
                hyper_edge_id: new_he_id.clone(),
                parent: fan.parent,
                children: new_children,
            });
        } else {
            // Keep original fan with surviving children only
            result.push(Fan {
                hyper_edge_id: fan.hyper_edge_id.clone(),
                parent: fan.parent,
                children: surviving_children,
            });
        }
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Main restrict function
// ---------------------------------------------------------------------------

/// The restrict operation for W-type instances.
///
/// This is the data migration algorithm for tree-structured data.
/// It executes a 5-step pipeline:
///
/// 1. **Signature restriction**: keep nodes whose anchor survives
/// 2. **Reachability BFS**: keep only nodes reachable from root
/// 3. **Ancestor contraction**: find nearest surviving ancestors
/// 4. **Edge resolution**: resolve edges for contracted arcs
/// 5. **Fan reconstruction**: rebuild hyper-edge fans
///
/// # Errors
///
/// Returns `RestrictError` if edge resolution fails or the root
/// is pruned during restriction.
pub fn wtype_restrict(
    instance: &WInstance,
    _src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
) -> Result<WInstance, RestrictError> {
    // Step 1: Signature restriction
    let candidates = anchor_surviving(instance, &migration.surviving_verts);

    // Step 2: Reachability BFS
    let surviving = reachable_from_root(instance, &candidates);

    if !surviving.contains(&instance.root) {
        return Err(RestrictError::RootPruned);
    }

    // Step 3: Ancestor contraction
    let ancestors = ancestor_contraction(instance, &surviving);

    // Step 4: Build new arcs via edge resolution
    let mut new_arcs = Vec::new();
    for (&child_id, &ancestor_id) in &ancestors {
        let child_node = instance
            .nodes
            .get(&child_id)
            .ok_or(RestrictError::RootPruned)?;
        let ancestor_node = instance
            .nodes
            .get(&ancestor_id)
            .ok_or(RestrictError::RootPruned)?;

        // Remap anchors through the migration
        let src_anchor = migration
            .vertex_remap
            .get(&ancestor_node.anchor)
            .cloned()
            .unwrap_or_else(|| ancestor_node.anchor.clone());
        let tgt_anchor = migration
            .vertex_remap
            .get(&child_node.anchor)
            .cloned()
            .unwrap_or_else(|| child_node.anchor.clone());

        let edge = resolve_edge(tgt_schema, &migration.resolver, &src_anchor, &tgt_anchor)?;
        new_arcs.push((ancestor_id, child_id, edge));
    }

    // Step 5: Fan reconstruction
    let new_fans = reconstruct_fans(instance, &surviving, &ancestors, migration, tgt_schema)?;

    // Build surviving nodes with remapped anchors
    let mut new_nodes = HashMap::new();
    for &node_id in &surviving {
        if let Some(node) = instance.nodes.get(&node_id) {
            let mut new_node = node.clone();
            if let Some(remapped) = migration.vertex_remap.get(&node.anchor) {
                new_node.anchor.clone_from(remapped);
            }
            new_nodes.insert(node_id, new_node);
        }
    }

    let new_schema_root = migration
        .vertex_remap
        .get(&instance.schema_root)
        .cloned()
        .unwrap_or_else(|| instance.schema_root.clone());

    Ok(WInstance::new(
        new_nodes,
        new_arcs,
        new_fans,
        instance.root,
        new_schema_root,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{FieldPresence, Value};

    /// Helper: build a simple 3-node instance (object with two string children).
    fn three_node_instance() -> WInstance {
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "post:body"));
        nodes.insert(
            1,
            Node::new(1, "post:body.text")
                .with_value(FieldPresence::Present(Value::Str("hello".into()))),
        );
        nodes.insert(
            2,
            Node::new(2, "post:body.createdAt")
                .with_value(FieldPresence::Present(Value::Str("2024-01-01".into()))),
        );

        let arcs = vec![
            (
                0,
                1,
                Edge {
                    src: "post:body".into(),
                    tgt: "post:body.text".into(),
                    kind: "prop".into(),
                    name: Some("text".into()),
                },
            ),
            (
                0,
                2,
                Edge {
                    src: "post:body".into(),
                    tgt: "post:body.createdAt".into(),
                    kind: "prop".into(),
                    name: Some("createdAt".into()),
                },
            ),
        ];

        WInstance::new(nodes, arcs, vec![], 0, "post:body".into())
    }

    #[test]
    fn anchor_surviving_keeps_matching_nodes() {
        let inst = three_node_instance();
        let surviving_verts: HashSet<String> = ["post:body", "post:body.text"]
            .iter()
            .map(|&s| s.into())
            .collect();

        let result = anchor_surviving(&inst, &surviving_verts);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&0));
        assert!(result.contains(&1));
        assert!(!result.contains(&2));
    }

    #[test]
    fn reachable_from_root_filters_disconnected() {
        let inst = three_node_instance();
        // Only nodes 0 and 2 survive anchoring, but 2 is reachable from 0
        let candidates: HashSet<u32> = [0, 2].iter().copied().collect();
        let reachable = reachable_from_root(&inst, &candidates);
        assert_eq!(reachable.len(), 2);
        assert!(reachable.contains(&0));
        assert!(reachable.contains(&2));
    }

    #[test]
    fn reachable_from_root_empty_when_root_missing() {
        let inst = three_node_instance();
        let candidates: HashSet<u32> = [1, 2].iter().copied().collect();
        let reachable = reachable_from_root(&inst, &candidates);
        assert!(reachable.is_empty());
    }

    #[test]
    fn ancestor_contraction_direct_parent() {
        let inst = three_node_instance();
        let surviving: HashSet<u32> = [0, 1, 2].iter().copied().collect();
        let ancestors = ancestor_contraction(&inst, &surviving);
        assert_eq!(ancestors.get(&1), Some(&0));
        assert_eq!(ancestors.get(&2), Some(&0));
    }

    #[test]
    fn resolve_edge_unique() {
        use smallvec::smallvec;
        let mut between = HashMap::new();
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        between.insert(("a".to_string(), "b".to_string()), smallvec![edge.clone()]);

        let schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between,
        };

        let resolver = HashMap::new();
        let result = resolve_edge(&schema, &resolver, "a", "b");
        assert!(result.is_ok());
        assert_eq!(result.ok(), Some(edge));
    }

    #[test]
    fn resolve_edge_uses_resolver() {
        let schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        };

        let resolved_edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("resolved".into()),
        };
        let mut resolver = HashMap::new();
        resolver.insert(("a".to_string(), "b".to_string()), resolved_edge.clone());

        let result = resolve_edge(&schema, &resolver, "a", "b");
        assert!(result.is_ok());
        assert_eq!(result.ok(), Some(resolved_edge));
    }
}
