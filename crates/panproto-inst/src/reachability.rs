//! Incremental reachability index for W-type instances.
//!
//! Tracks which nodes are reachable from the root without recomputing
//! a full BFS on every edit. Used by the edit lens pipeline.

use std::collections::{HashMap, VecDeque};

use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::wtype::WInstance;

/// Incremental reachability index over a W-type instance tree.
///
/// Maintains a `reachable` set together with parent/children adjacency
/// maps so that edge insertions and deletions can update reachability
/// in time proportional to the affected subtree rather than the whole
/// instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReachabilityIndex {
    reachable: FxHashSet<u32>,
    children: HashMap<u32, SmallVec<u32, 4>>,
    parents: HashMap<u32, u32>,
    root: Option<u32>,
}

impl ReachabilityIndex {
    /// Build a reachability index from an existing W-type instance via BFS.
    #[must_use]
    pub fn from_instance(inst: &WInstance) -> Self {
        let mut reachable = FxHashSet::default();
        let mut children: HashMap<u32, SmallVec<u32, 4>> = HashMap::new();
        let mut parents: HashMap<u32, u32> = HashMap::new();

        // Copy adjacency from instance
        for &(parent, child, _) in &inst.arcs {
            children.entry(parent).or_default().push(child);
            parents.insert(child, parent);
        }

        // BFS from root
        let mut queue = VecDeque::new();
        queue.push_back(inst.root);
        reachable.insert(inst.root);

        while let Some(current) = queue.pop_front() {
            if let Some(kids) = children.get(&current) {
                for &child in kids {
                    if reachable.insert(child) {
                        queue.push_back(child);
                    }
                }
            }
        }

        Self {
            reachable,
            children,
            parents,
            root: Some(inst.root),
        }
    }

    /// Insert an edge from `parent` to `child`.
    ///
    /// If `parent` is reachable and `child` was not, BFS from `child` to mark
    /// the entire subtree reachable. Returns the list of newly reachable nodes.
    pub fn insert_edge(&mut self, parent: u32, child: u32) -> Vec<u32> {
        self.children.entry(parent).or_default().push(child);
        self.parents.insert(child, parent);

        if !self.reachable.contains(&parent) || self.reachable.contains(&child) {
            return Vec::new();
        }

        // BFS from child to mark subtree reachable
        let mut newly_reachable = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(child);

        while let Some(current) = queue.pop_front() {
            if self.reachable.insert(current) {
                newly_reachable.push(current);
                if let Some(kids) = self.children.get(&current) {
                    for &kid in kids {
                        if !self.reachable.contains(&kid) {
                            queue.push_back(kid);
                        }
                    }
                }
            }
        }

        newly_reachable
    }

    /// Delete the edge from `parent` to `child`.
    ///
    /// If `child` is no longer reachable via any path to root, BFS from `child`
    /// to mark the subtree unreachable. Returns the list of newly unreachable
    /// nodes. Handles the case where `child` has an alternate path to root.
    pub fn delete_edge(&mut self, parent: u32, child: u32) -> Vec<u32> {
        // Remove from adjacency
        if let Some(kids) = self.children.get_mut(&parent) {
            if let Some(pos) = kids.iter().position(|&k| k == child) {
                kids.remove(pos);
            }
        }

        // Only remove from parents if this was the recorded parent
        if self.parents.get(&child).copied() == Some(parent) {
            self.parents.remove(&child);
        }

        if !self.reachable.contains(&child) {
            return Vec::new();
        }

        // Check if child is still reachable via an alternate path.
        // Walk from child toward root using any remaining parent entry.
        if self.has_path_to_root(child) {
            return Vec::new();
        }

        // BFS from child to mark subtree unreachable
        let mut newly_unreachable = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(child);

        while let Some(current) = queue.pop_front() {
            if self.reachable.remove(&current) {
                newly_unreachable.push(current);
                if let Some(kids) = self.children.get(&current) {
                    for &kid in kids {
                        if self.reachable.contains(&kid) && !self.has_path_to_root(kid) {
                            queue.push_back(kid);
                        }
                    }
                }
            }
        }

        newly_unreachable
    }

    /// Check whether `node` is reachable from the root.
    #[must_use]
    pub fn is_reachable(&self, node: u32) -> bool {
        self.reachable.contains(&node)
    }

    /// Returns the root node (the one with no parent in the parents map).
    #[must_use]
    pub const fn root(&self) -> Option<u32> {
        self.root
    }

    /// Returns the parent of `node` according to the stored adjacency.
    #[must_use]
    pub fn parent_of(&self, node: u32) -> Option<u32> {
        self.parents.get(&node).copied()
    }

    /// Returns the children of `node` according to the stored adjacency.
    #[must_use]
    pub fn children_of(&self, node: u32) -> &[u32] {
        self.children
            .get(&node)
            .map_or(&[], smallvec::SmallVec::as_slice)
    }

    /// Check if a node can reach the root by walking up the parent chain
    /// or by doing a reverse BFS through all incoming edges.
    fn has_path_to_root(&self, node: u32) -> bool {
        let Some(root) = self.root else {
            return false;
        };
        if node == root {
            return true;
        }

        // Walk parent pointers, but also check all nodes that list `node`
        // as a child (since parents map only stores one parent).
        let mut visited = FxHashSet::default();
        let mut queue = VecDeque::new();
        queue.push_back(node);
        visited.insert(node);

        while let Some(current) = queue.pop_front() {
            // Check direct parent
            if let Some(&p) = self.parents.get(&current) {
                if p == root {
                    return true;
                }
                if self.reachable.contains(&p) && visited.insert(p) {
                    queue.push_back(p);
                }
            }
            // Check all potential parents (nodes whose children list includes current)
            for (&candidate, kids) in &self.children {
                if kids.contains(&current) && candidate != node && visited.insert(candidate) {
                    if candidate == root {
                        return true;
                    }
                    if self.reachable.contains(&candidate) {
                        queue.push_back(candidate);
                    }
                }
            }
        }

        false
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
mod tests {
    use std::collections::HashMap;

    use panproto_gat::Name;
    use panproto_schema::Edge;

    use crate::metadata::Node;
    use crate::wtype::WInstance;

    use super::*;

    fn test_node(id: u32, anchor: &str) -> Node {
        Node {
            id,
            anchor: Name::from(anchor),
            value: None,
            discriminator: None,
            extra_fields: HashMap::new(),
            position: None,
            annotations: HashMap::new(),
        }
    }

    /// Helper: build a minimal `WInstance` from a list of (parent, child) pairs.
    fn make_instance(root: u32, arcs: &[(u32, u32)]) -> WInstance {
        let mut nodes = HashMap::new();
        nodes.insert(root, test_node(root, "root"));
        for &(p, c) in arcs {
            nodes.entry(p).or_insert_with(|| test_node(p, "v"));
            nodes.entry(c).or_insert_with(|| test_node(c, "v"));
        }

        let edge = Edge {
            src: Name::from("a"),
            tgt: Name::from("b"),
            kind: Name::from("prop"),
            name: None,
        };

        let arcs_vec: Vec<(u32, u32, Edge)> =
            arcs.iter().map(|&(p, c)| (p, c, edge.clone())).collect();

        WInstance::new(nodes, arcs_vec, Vec::new(), root, Name::from("root"))
    }

    #[test]
    fn from_instance_marks_all_reachable() {
        // root(0) -> 1 -> 2
        let inst = make_instance(0, &[(0, 1), (1, 2)]);
        let idx = ReachabilityIndex::from_instance(&inst);

        assert!(idx.is_reachable(0));
        assert!(idx.is_reachable(1));
        assert!(idx.is_reachable(2));
        assert_eq!(idx.root(), Some(0));
    }

    #[test]
    fn delete_edge_marks_subtree_unreachable() {
        // root(0) -> 1 -> 2
        let inst = make_instance(0, &[(0, 1), (1, 2)]);
        let mut idx = ReachabilityIndex::from_instance(&inst);

        let unreachable = idx.delete_edge(0, 1);
        assert!(unreachable.contains(&1));
        assert!(unreachable.contains(&2));
        assert!(!idx.is_reachable(1));
        assert!(!idx.is_reachable(2));
        assert!(idx.is_reachable(0));
    }

    #[test]
    fn insert_edge_makes_unreachable_reachable() {
        // Start with root(0) -> 1 -> 2, then delete 0->1, then re-add 0->1
        let inst = make_instance(0, &[(0, 1), (1, 2)]);
        let mut idx = ReachabilityIndex::from_instance(&inst);

        idx.delete_edge(0, 1);
        assert!(!idx.is_reachable(1));
        assert!(!idx.is_reachable(2));

        let newly = idx.insert_edge(0, 1);
        assert!(newly.contains(&1));
        assert!(newly.contains(&2));
        assert!(idx.is_reachable(1));
        assert!(idx.is_reachable(2));
    }

    #[test]
    fn diamond_graph_one_path_removal() {
        // root(0) -> 1 -> 3, root(0) -> 2 -> 3
        // Deleting 0->1 should leave 3 reachable via 0->2->3
        let mut nodes = HashMap::new();
        for id in 0..=3 {
            nodes.insert(id, test_node(id, "v"));
        }
        let edge = Edge {
            src: Name::from("a"),
            tgt: Name::from("b"),
            kind: Name::from("prop"),
            name: None,
        };
        let arcs = vec![
            (0, 1, edge.clone()),
            (0, 2, edge.clone()),
            (1, 3, edge.clone()),
            (2, 3, edge),
        ];
        let inst = WInstance::new(nodes, arcs, Vec::new(), 0, Name::from("root"));
        let mut idx = ReachabilityIndex::from_instance(&inst);

        let unreachable = idx.delete_edge(1, 3);
        // 3 is still reachable via 2->3
        assert!(idx.is_reachable(3));
        assert!(!unreachable.contains(&3));

        // 1 is still reachable directly from root
        assert!(idx.is_reachable(1));
    }

    #[test]
    fn empty_instance() {
        // Single-node instance (just root)
        let inst = make_instance(0, &[]);
        let idx = ReachabilityIndex::from_instance(&inst);

        assert!(idx.is_reachable(0));
        assert!(!idx.is_reachable(1));
        assert_eq!(idx.root(), Some(0));
    }

    #[test]
    fn deep_chain() {
        // Chain: 0 -> 1 -> 2 -> ... -> 9
        let arcs: Vec<(u32, u32)> = (0..9).map(|i| (i, i + 1)).collect();
        let inst = make_instance(0, &arcs);
        let mut idx = ReachabilityIndex::from_instance(&inst);

        for i in 0..=9 {
            assert!(idx.is_reachable(i));
        }

        // Delete edge 4->5, nodes 5..=9 become unreachable
        let unreachable = idx.delete_edge(4, 5);
        for i in 0..=4 {
            assert!(idx.is_reachable(i));
        }
        for i in 5..=9 {
            assert!(!idx.is_reachable(i));
            assert!(unreachable.contains(&i));
        }
    }
}
