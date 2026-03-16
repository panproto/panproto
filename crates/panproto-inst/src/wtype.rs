//! W-type instance representation and the `wtype_restrict` pipeline.
//!
//! A [`WInstance`] is a tree-shaped data instance conforming to a schema.
//! The restrict operation (`wtype_restrict`) is a fused single-pass pipeline
//! that projects a W-type instance along a migration mapping.
//!
//! The pipeline fuses four concerns into one BFS traversal:
//! 1. Anchor survival check — does this node's schema vertex survive?
//! 2. Reachability — is this node reachable from the root?
//! 3. Ancestor contraction — who is the nearest surviving ancestor?
//! 4. Edge resolution — what edge connects the contracted arc?
//!
//! Fan reconstruction (step 5) runs as a separate pass since it operates
//! on the original fan list, not the BFS tree.
//!
//! The five individual step functions are retained for testing and debugging.

use std::collections::{HashMap, HashSet, VecDeque};

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};
use rustc_hash::{FxHashMap, FxHashSet};
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
    pub surviving_verts: HashSet<Name>,
    /// Edges that survive the migration.
    pub surviving_edges: HashSet<Edge>,
    /// Vertex remapping: source vertex ID to target vertex ID.
    pub vertex_remap: HashMap<Name, Name>,
    /// Edge remapping: source edge to target edge.
    pub edge_remap: HashMap<Edge, Edge>,
    /// Binary contraction resolver: (`src_anchor`, `tgt_anchor`) to resolved edge.
    pub resolver: HashMap<(Name, Name), Edge>,
    /// Hyper-edge contraction resolver.
    pub hyper_resolver: HashMap<Name, (Name, HashMap<Name, Name>)>,
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
    pub schema_root: Name,
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
        schema_root: Name,
    ) -> Self {
        let mut parent_map = HashMap::with_capacity(arcs.len());
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
    #[inline]
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the number of arcs.
    #[inline]
    #[must_use]
    pub fn arc_count(&self) -> usize {
        self.arcs.len()
    }

    /// Get a node by ID.
    #[inline]
    #[must_use]
    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Get the children of a node.
    #[inline]
    #[must_use]
    pub fn children(&self, id: u32) -> &[u32] {
        self.children_map.get(&id).map_or(&[], SmallVec::as_slice)
    }

    /// Get the parent of a node.
    #[inline]
    #[must_use]
    pub fn parent(&self, id: u32) -> Option<u32> {
        self.parent_map.get(&id).copied()
    }
}

// ---------------------------------------------------------------------------
// Step 1: Signature restriction (retained for testing)
// ---------------------------------------------------------------------------

/// Keep nodes whose anchor vertex is in the surviving vertex set.
#[must_use]
pub fn anchor_surviving(instance: &WInstance, surviving_verts: &HashSet<Name>) -> HashSet<u32> {
    instance
        .nodes
        .iter()
        .filter(|(_, node)| surviving_verts.contains(&node.anchor))
        .map(|(&id, _)| id)
        .collect()
}

// ---------------------------------------------------------------------------
// Step 2: Reachability BFS (retained for testing)
// ---------------------------------------------------------------------------

/// BFS from the root through anchor-surviving nodes.
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
// Step 3: Ancestor contraction with path compression (retained for testing)
// ---------------------------------------------------------------------------

/// For each surviving non-root node, find its nearest surviving ancestor.
///
/// Uses path compression: when we walk the parent chain for a node,
/// we cache the result for every intermediate node visited. Subsequent
/// queries hitting a cached node return in O(1). This gives O(n)
/// amortized complexity instead of O(n × depth).
#[must_use]
pub fn ancestor_contraction(instance: &WInstance, surviving: &HashSet<u32>) -> HashMap<u32, u32> {
    let mut cache: FxHashMap<u32, u32> = FxHashMap::default();
    let mut ancestors = HashMap::new();

    for &node_id in surviving {
        if node_id == instance.root {
            continue;
        }

        // Check cache first
        if let Some(&cached) = cache.get(&node_id) {
            ancestors.insert(node_id, cached);
            continue;
        }

        // Walk the parent chain, recording the path for compression
        let mut path = Vec::new();
        let mut current = node_id;
        let mut found_ancestor = None;

        while let Some(parent) = instance.parent(current) {
            if let Some(&cached) = cache.get(&parent) {
                found_ancestor = Some(cached);
                break;
            }
            if surviving.contains(&parent) {
                found_ancestor = Some(parent);
                break;
            }
            path.push(parent);
            current = parent;
        }

        // Path compression: cache the ancestor for all nodes on the path
        if let Some(ancestor) = found_ancestor {
            ancestors.insert(node_id, ancestor);
            cache.insert(node_id, ancestor);
            for &intermediate in &path {
                cache.insert(intermediate, ancestor);
            }
        }
    }
    ancestors
}

// ---------------------------------------------------------------------------
// Step 4: Edge resolution (retained for testing)
// ---------------------------------------------------------------------------

/// Resolve the edge for a contracted arc in the target schema.
///
/// Avoids allocating a `(String, String)` tuple for the resolver lookup
/// by checking the resolver with borrowed references.
///
/// # Errors
///
/// Returns `RestrictError::NoEdgeFound` if no edge exists, or
/// `RestrictError::AmbiguousEdge` if multiple edges exist without
/// a resolver entry.
pub fn resolve_edge(
    tgt_schema: &Schema,
    resolver: &HashMap<(Name, Name), Edge>,
    src_v: &str,
    tgt_v: &str,
) -> Result<Edge, RestrictError> {
    // Check resolver — avoid allocation by scanning for matching key
    for ((k_src, k_tgt), edge) in resolver {
        if k_src == src_v && k_tgt == tgt_v {
            return Ok(edge.clone());
        }
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
// Step 5: Fan reconstruction (retained for testing)
// ---------------------------------------------------------------------------

/// Reconstruct fans after restriction.
///
/// # Errors
///
/// Returns `RestrictError::FanReconstructionFailed` if a fan cannot
/// be validly reconstructed.
pub fn reconstruct_fans(
    instance: &WInstance,
    surviving: &FxHashSet<u32>,
    _ancestors: &FxHashMap<u32, u32>,
    migration: &CompiledMigration,
    _tgt_schema: &Schema,
) -> Result<Vec<Fan>, RestrictError> {
    let mut result = Vec::new();

    for fan in &instance.fans {
        if !surviving.contains(&fan.parent) {
            continue;
        }

        let surviving_children: HashMap<String, u32> = fan
            .children
            .iter()
            .filter(|(_, node_id)| surviving.contains(node_id))
            .map(|(label, node_id)| (label.clone(), *node_id))
            .collect();

        if surviving_children.is_empty() {
            continue;
        }

        if let Some((new_he_id, label_map)) =
            migration.hyper_resolver.get(fan.hyper_edge_id.as_str())
        {
            let mut new_children = HashMap::new();
            for (old_label, &node_id) in &surviving_children {
                let new_label = label_map
                    .get(old_label.as_str())
                    .map_or_else(|| old_label.clone(), std::string::ToString::to_string);
                new_children.insert(new_label, node_id);
            }
            result.push(Fan {
                hyper_edge_id: new_he_id.to_string(),
                parent: fan.parent,
                children: new_children,
            });
        } else {
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
// Main restrict function: fused single-pass pipeline
// ---------------------------------------------------------------------------

/// The restrict operation for W-type instances.
///
/// Executes a fused single-pass pipeline that combines anchor checking,
/// BFS reachability, ancestor contraction, and edge resolution into one
/// traversal. Fan reconstruction runs as a separate pass.
///
/// The fused approach visits each node at most once (O(n)) versus
/// the sequential 5-step approach which makes 3-4 passes.
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
    // Check root survives
    let root_node = instance
        .nodes
        .get(&instance.root)
        .ok_or(RestrictError::RootPruned)?;
    if !migration.surviving_verts.contains(&root_node.anchor) {
        return Err(RestrictError::RootPruned);
    }

    // Fused BFS: traverse the tree from root, tracking the nearest
    // surviving ancestor for each node as we go.
    //
    // For each node in the BFS:
    //   - If its anchor survives: it becomes part of the result.
    //     Its nearest surviving ancestor is used to build an arc.
    //     It becomes the "current surviving ancestor" for its subtree.
    //   - If its anchor does not survive: skip it, but continue BFS
    //     into its children (they might survive). Pass along the
    //     current surviving ancestor unchanged.

    let mut new_nodes: HashMap<u32, Node> = HashMap::new();
    let mut new_arcs: Vec<(u32, u32, Edge)> = Vec::new();
    let mut surviving_set: FxHashSet<u32> = FxHashSet::default();

    // Queue entries: (node_id, nearest_surviving_ancestor_id)
    let mut queue: VecDeque<(u32, Option<u32>)> = VecDeque::new();

    // Process root
    let mut root_node_cloned = root_node.clone();
    if let Some(remapped) = migration.vertex_remap.get(&root_node.anchor) {
        root_node_cloned.anchor.clone_from(remapped);
    }
    new_nodes.insert(instance.root, root_node_cloned);
    surviving_set.insert(instance.root);
    queue.push_back((instance.root, None));

    while let Some((current_id, ancestor_id)) = queue.pop_front() {
        let current_survives = surviving_set.contains(&current_id);
        // The ancestor for children: if current survives, it's the new ancestor;
        // otherwise, pass along the existing ancestor.
        let child_ancestor = if current_survives {
            Some(current_id)
        } else {
            ancestor_id
        };

        for &child_id in instance.children(current_id) {
            let Some(child_node) = instance.nodes.get(&child_id) else {
                continue;
            };

            if migration.surviving_verts.contains(&child_node.anchor) {
                // This child survives — add it to results
                surviving_set.insert(child_id);
                let mut new_node = child_node.clone();
                if let Some(remapped) = migration.vertex_remap.get(&child_node.anchor) {
                    new_node.anchor.clone_from(remapped);
                }
                new_nodes.insert(child_id, new_node.clone());

                // Build the arc from nearest surviving ancestor to this node
                if let Some(anc_id) = child_ancestor {
                    let anc_node = new_nodes.get(&anc_id).ok_or(RestrictError::RootPruned)?;
                    let edge = resolve_edge(
                        tgt_schema,
                        &migration.resolver,
                        &anc_node.anchor,
                        &new_node.anchor,
                    )?;
                    new_arcs.push((anc_id, child_id, edge));
                }
            }

            // Always continue BFS into children (non-surviving intermediate
            // nodes may have surviving descendants)
            queue.push_back((child_id, child_ancestor));
        }
    }

    // Step 5: Fan reconstruction (separate pass — operates on original fans)
    let fused_surviving = &surviving_set;
    let empty_ancestors = FxHashMap::default();
    let new_fans = reconstruct_fans(
        instance,
        fused_surviving,
        &empty_ancestors,
        migration,
        tgt_schema,
    )?;

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

// ---------------------------------------------------------------------------
// Left Kan extension (Σ_F) for W-type instances
// ---------------------------------------------------------------------------

/// Left Kan extension (`Sigma_F`) for W-type instances.
///
/// Pushes a W-type instance forward along a migration morphism.
/// Unlike [`wtype_restrict`] (which drops unmapped nodes), extend
/// maps all source nodes into the target schema, remapping anchors
/// and edges according to the compiled migration.
///
/// # Errors
///
/// Returns [`RestrictError`] if edge resolution fails or the root
/// cannot be mapped.
pub fn wtype_extend(
    instance: &WInstance,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
) -> Result<WInstance, RestrictError> {
    // Check root can be mapped
    let root_node = instance
        .nodes
        .get(&instance.root)
        .ok_or(RestrictError::RootPruned)?;

    let root_anchor = &root_node.anchor;
    if !migration.surviving_verts.contains(root_anchor)
        && !migration.vertex_remap.contains_key(root_anchor)
    {
        return Err(RestrictError::RootPruned);
    }

    // Build new nodes: remap anchors where applicable
    let mut new_nodes: HashMap<u32, Node> = HashMap::with_capacity(instance.nodes.len());
    for (&id, node) in &instance.nodes {
        let mut new_node = node.clone();
        if let Some(remapped) = migration.vertex_remap.get(&node.anchor) {
            new_node.anchor.clone_from(remapped);
        } else if !migration.surviving_verts.contains(&node.anchor) {
            // Node's anchor has no remap and doesn't survive — skip it
            continue;
        }
        new_nodes.insert(id, new_node);
    }

    // Build new arcs: remap edges where applicable
    let mut new_arcs: Vec<(u32, u32, Edge)> = Vec::with_capacity(instance.arcs.len());
    for &(parent, child, ref edge) in &instance.arcs {
        // Both endpoints must be in the new node set
        if !new_nodes.contains_key(&parent) || !new_nodes.contains_key(&child) {
            continue;
        }

        if let Some(new_edge) = migration.edge_remap.get(edge) {
            new_arcs.push((parent, child, new_edge.clone()));
        } else if migration.surviving_edges.contains(edge) {
            // Edge survives unchanged, but anchors may have been remapped.
            // Rebuild the edge with the remapped src/tgt vertex names.
            let parent_anchor = &new_nodes[&parent].anchor;
            let child_anchor = &new_nodes[&child].anchor;
            if edge.src == *parent_anchor && edge.tgt == *child_anchor {
                new_arcs.push((parent, child, edge.clone()));
            } else {
                // Anchors were remapped; resolve the edge in the target schema
                let resolved =
                    resolve_edge(tgt_schema, &migration.resolver, parent_anchor, child_anchor)?;
                new_arcs.push((parent, child, resolved));
            }
        } else {
            // Edge not in surviving_edges or edge_remap — try to resolve
            // from remapped anchors
            let parent_anchor = &new_nodes[&parent].anchor;
            let child_anchor = &new_nodes[&child].anchor;
            let resolved =
                resolve_edge(tgt_schema, &migration.resolver, parent_anchor, child_anchor)?;
            new_arcs.push((parent, child, resolved));
        }
    }

    // Handle fans similarly to restrict's reconstruct_fans
    let surviving_ids: FxHashSet<u32> = new_nodes.keys().copied().collect();
    let empty_ancestors = FxHashMap::default();
    let new_fans = reconstruct_fans(
        instance,
        &surviving_ids,
        &empty_ancestors,
        migration,
        tgt_schema,
    )?;

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
        nodes.insert(0, Node::new(0, panproto_gat::Name::from("post:body")));
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

        WInstance::new(
            nodes,
            arcs,
            vec![],
            0,
            panproto_gat::Name::from("post:body"),
        )
    }

    #[test]
    fn anchor_surviving_keeps_matching_nodes() {
        let inst = three_node_instance();
        let surviving_verts: HashSet<Name> = ["post:body", "post:body.text"]
            .iter()
            .map(|&s| Name::from(s))
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
        between.insert((Name::from("a"), Name::from("b")), smallvec![edge.clone()]);

        let schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
            edges: HashMap::new(),
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
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
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
        resolver.insert((Name::from("a"), Name::from("b")), resolved_edge.clone());

        let result = resolve_edge(&schema, &resolver, "a", "b");
        assert!(result.is_ok());
        assert_eq!(result.ok(), Some(resolved_edge));
    }

    // --- wtype_extend tests ---

    #[allow(clippy::unwrap_used)]
    fn make_test_schema(vertices: &[&str], edges: &[Edge]) -> Schema {
        use smallvec::smallvec;
        let mut between = HashMap::new();
        for edge in edges {
            between
                .entry((Name::from(&*edge.src), Name::from(&*edge.tgt)))
                .or_insert_with(|| smallvec![])
                .push(edge.clone());
        }
        Schema {
            protocol: "test".into(),
            vertices: vertices
                .iter()
                .map(|&v| {
                    (
                        Name::from(v),
                        panproto_schema::Vertex {
                            id: Name::from(v),
                            kind: Name::from("object"),
                            nsid: None,
                        },
                    )
                })
                .collect(),
            edges: HashMap::new(),
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
            between,
        }
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn extend_identity_migration() {
        let inst = three_node_instance();
        let edge_text = Edge {
            src: "post:body".into(),
            tgt: "post:body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let edge_time = Edge {
            src: "post:body".into(),
            tgt: "post:body.createdAt".into(),
            kind: "prop".into(),
            name: Some("createdAt".into()),
        };
        let surviving_edges = HashSet::from([edge_text.clone(), edge_time.clone()]);
        let schema = make_test_schema(
            &["post:body", "post:body.text", "post:body.createdAt"],
            &[edge_text, edge_time],
        );
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([
                Name::from("post:body"),
                Name::from("post:body.text"),
                Name::from("post:body.createdAt"),
            ]),
            surviving_edges,
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        };
        let result = wtype_extend(&inst, &schema, &migration).unwrap();
        assert_eq!(result.node_count(), 3);
        assert_eq!(result.arc_count(), 2);
        assert_eq!(result.schema_root, Name::from("post:body"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn extend_with_vertex_remap() {
        let inst = three_node_instance();
        let tgt_edge_text = Edge {
            src: "article:body".into(),
            tgt: "article:body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let tgt_edge_time = Edge {
            src: "article:body".into(),
            tgt: "article:body.createdAt".into(),
            kind: "prop".into(),
            name: Some("createdAt".into()),
        };
        let tgt_schema = make_test_schema(
            &[
                "article:body",
                "article:body.text",
                "article:body.createdAt",
            ],
            &[tgt_edge_text, tgt_edge_time],
        );
        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("post:body"), Name::from("article:body"));
        vertex_remap.insert(
            Name::from("post:body.text"),
            Name::from("article:body.text"),
        );
        vertex_remap.insert(
            Name::from("post:body.createdAt"),
            Name::from("article:body.createdAt"),
        );
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([
                Name::from("article:body"),
                Name::from("article:body.text"),
                Name::from("article:body.createdAt"),
            ]),
            surviving_edges: HashSet::new(),
            vertex_remap,
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        };
        let result = wtype_extend(&inst, &tgt_schema, &migration).unwrap();
        assert_eq!(result.node_count(), 3);
        assert_eq!(result.arc_count(), 2);
        assert_eq!(result.schema_root, Name::from("article:body"));
        assert_eq!(result.nodes[&0].anchor, Name::from("article:body"));
        assert_eq!(result.nodes[&1].anchor, Name::from("article:body.text"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn extend_with_edge_remap() {
        let inst = three_node_instance();
        let src_edge_text = Edge {
            src: "post:body".into(),
            tgt: "post:body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let new_edge_text = Edge {
            src: "post:body".into(),
            tgt: "post:body.text".into(),
            kind: "prop".into(),
            name: Some("content".into()),
        };
        let edge_time = Edge {
            src: "post:body".into(),
            tgt: "post:body.createdAt".into(),
            kind: "prop".into(),
            name: Some("createdAt".into()),
        };
        let surviving_edges = HashSet::from([edge_time.clone()]);
        let tgt_schema = make_test_schema(
            &["post:body", "post:body.text", "post:body.createdAt"],
            &[new_edge_text.clone(), edge_time],
        );
        let mut edge_remap = HashMap::new();
        edge_remap.insert(src_edge_text, new_edge_text);
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([
                Name::from("post:body"),
                Name::from("post:body.text"),
                Name::from("post:body.createdAt"),
            ]),
            surviving_edges,
            vertex_remap: HashMap::new(),
            edge_remap,
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        };
        let result = wtype_extend(&inst, &tgt_schema, &migration).unwrap();
        assert_eq!(result.arc_count(), 2);
        // Check that the remapped edge is used
        let text_arc = result.arcs.iter().find(|a| a.1 == 1).unwrap();
        assert_eq!(text_arc.2.name.as_deref(), Some("content"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn extend_preserves_structure() {
        let inst = three_node_instance();
        let edge_text = Edge {
            src: "post:body".into(),
            tgt: "post:body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let edge_time = Edge {
            src: "post:body".into(),
            tgt: "post:body.createdAt".into(),
            kind: "prop".into(),
            name: Some("createdAt".into()),
        };
        let surviving_edges = HashSet::from([edge_text.clone(), edge_time.clone()]);
        let schema = make_test_schema(
            &["post:body", "post:body.text", "post:body.createdAt"],
            &[edge_text, edge_time],
        );
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([
                Name::from("post:body"),
                Name::from("post:body.text"),
                Name::from("post:body.createdAt"),
            ]),
            surviving_edges,
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        };
        let result = wtype_extend(&inst, &schema, &migration).unwrap();
        // Verify parent/children maps are correctly rebuilt
        assert_eq!(result.parent(1), Some(0));
        assert_eq!(result.parent(2), Some(0));
        assert!(result.children(0).contains(&1));
        assert!(result.children(0).contains(&2));
        // Verify values are preserved
        assert!(result.nodes[&1].has_value());
        assert!(result.nodes[&2].has_value());
    }
}
