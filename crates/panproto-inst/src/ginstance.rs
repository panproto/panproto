//! Graph-shaped instance representation.
//!
//! A [`GInstance`] is the most general instance form: a directed graph
//! of nodes and edges with no distinguished root and cycles allowed.
//! Both [`WInstance`](crate::WInstance) (trees) and
//! [`FInstance`](crate::FInstance) (tables) are special cases.
//!
//! This is the natural instance theory for knowledge graphs (RDF,
//! OWL, JSON-LD), property graphs (Neo4j), and dependency graphs.

use std::collections::HashMap;

use panproto_schema::Edge;
use serde::{Deserialize, Serialize};

use crate::error::RestrictError;
use crate::metadata::Node;
use crate::value::Value;
use crate::wtype::CompiledMigration;

/// A graph-shaped instance: the most general instance form.
///
/// Unlike [`WInstance`](crate::WInstance), `GInstance` has no
/// distinguished root and cycles are allowed. Unlike
/// [`FInstance`](crate::FInstance), it is not bipartite.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GInstance {
    /// All nodes keyed by their numeric ID.
    pub nodes: HashMap<u32, Node>,
    /// Directed edges: `(src_node_id, tgt_node_id, schema_edge)`.
    pub edges: Vec<(u32, u32, Edge)>,
    /// Node values (leaf data).
    pub values: HashMap<u32, Value>,
}

impl GInstance {
    /// Create a new empty graph instance.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            values: HashMap::new(),
        }
    }

    /// Add a node.
    #[must_use]
    pub fn with_node(mut self, node: Node) -> Self {
        self.nodes.insert(node.id, node);
        self
    }

    /// Add an edge.
    #[must_use]
    pub fn with_edge(mut self, src: u32, tgt: u32, edge: Edge) -> Self {
        self.edges.push((src, tgt, edge));
        self
    }

    /// Add a value for a node.
    #[must_use]
    pub fn with_value(mut self, node_id: u32, value: Value) -> Self {
        self.values.insert(node_id, value);
        self
    }

    /// Returns the number of nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the number of edges.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

impl Default for GInstance {
    fn default() -> Self {
        Self::new()
    }
}

/// Restrict a graph instance along a compiled migration.
///
/// For each node whose anchor survives the migration, the node is
/// kept and its anchor is remapped. Edges whose endpoints both
/// survive and whose schema edge survives are kept and remapped.
///
/// # Errors
///
/// Returns an error if the restrict pipeline fails.
pub fn graph_restrict(
    instance: &GInstance,
    migration: &CompiledMigration,
) -> Result<GInstance, RestrictError> {
    let mut new_nodes = HashMap::new();
    let mut new_edges = Vec::new();
    let mut new_values = HashMap::new();

    // Keep nodes whose anchor survives.
    for (id, node) in &instance.nodes {
        if let Some(new_anchor) = migration.vertex_remap.get(&node.anchor) {
            let mut new_node = node.clone();
            new_anchor.clone_into(&mut new_node.anchor);
            new_nodes.insert(*id, new_node);

            if let Some(value) = instance.values.get(id) {
                new_values.insert(*id, value.clone());
            }
        }
    }

    // Keep edges whose endpoints both survive and schema edge survives.
    for (src, tgt, edge) in &instance.edges {
        if new_nodes.contains_key(src) && new_nodes.contains_key(tgt) {
            if let Some(new_edge) = migration.edge_remap.get(edge) {
                new_edges.push((*src, *tgt, new_edge.clone()));
            } else if migration.surviving_edges.contains(edge) {
                new_edges.push((*src, *tgt, edge.clone()));
            }
        }
    }

    Ok(GInstance {
        nodes: new_nodes,
        edges: new_edges,
        values: new_values,
    })
}

/// Extend (left Kan extension, `Sigma_F`) a graph instance along a migration.
///
/// Maps all source nodes into the target schema by remapping anchors
/// and edges according to the compiled migration. Nodes whose anchor
/// has no remap and is not in `surviving_verts` are dropped.
///
/// # Errors
///
/// Returns [`RestrictError`] if the extend pipeline fails.
pub fn graph_extend(
    instance: &GInstance,
    migration: &CompiledMigration,
) -> Result<GInstance, RestrictError> {
    let mut new_nodes = HashMap::new();
    let mut new_edges = Vec::new();
    let mut new_values = HashMap::new();

    // Remap all node anchors via vertex_remap; skip nodes whose anchor
    // has no remap and is not in surviving_verts.
    for (&id, node) in &instance.nodes {
        let mut new_node = node.clone();
        if let Some(remapped) = migration.vertex_remap.get(&node.anchor) {
            new_node.anchor.clone_from(remapped);
        } else if !migration.surviving_verts.contains(&node.anchor) {
            continue;
        }
        new_nodes.insert(id, new_node);

        // Copy values for surviving nodes
        if let Some(value) = instance.values.get(&id) {
            new_values.insert(id, value.clone());
        }
    }

    // Remap edges: check edge_remap first, then surviving_edges.
    for (src, tgt, edge) in &instance.edges {
        if !new_nodes.contains_key(src) || !new_nodes.contains_key(tgt) {
            continue;
        }

        if let Some(new_edge) = migration.edge_remap.get(edge) {
            new_edges.push((*src, *tgt, new_edge.clone()));
        } else if migration.surviving_edges.contains(edge) {
            new_edges.push((*src, *tgt, edge.clone()));
        }
    }

    Ok(GInstance {
        nodes: new_nodes,
        edges: new_edges,
        values: new_values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_graph_instance() {
        let g = GInstance::new();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn build_graph_instance() {
        let g = GInstance::new()
            .with_node(Node::new(0, "person"))
            .with_node(Node::new(1, "person"))
            .with_edge(
                0,
                1,
                Edge {
                    src: "person".into(),
                    tgt: "person".into(),
                    kind: "knows".into(),
                    name: None,
                },
            )
            .with_value(0, Value::Str("Alice".into()))
            .with_value(1, Value::Str("Bob".into()));

        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 1);
        assert_eq!(g.values.len(), 2);
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn graph_restrict_drops_unmapped_nodes() {
        let g = GInstance::new()
            .with_node(Node::new(0, "a"))
            .with_node(Node::new(1, "b"))
            .with_node(Node::new(2, "c"));

        let migration = CompiledMigration {
            surviving_verts: std::iter::once("a_new".into()).collect(),
            surviving_edges: std::collections::HashSet::new(),
            vertex_remap: std::iter::once(("a".into(), "a_new".into())).collect(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
        };

        let restricted = graph_restrict(&g, &migration).expect("graph_restrict should succeed");
        assert_eq!(restricted.node_count(), 1);
        assert_eq!(restricted.nodes[&0].anchor, "a_new");
    }
}
