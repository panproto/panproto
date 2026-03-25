//! Edit algebra for W-type instances (model of `ThEditableStructure`).
//!
//! A [`TreeEdit`] is an element of the edit monoid for tree-shaped
//! instances. The monoid operations are [`TreeEdit::identity`],
//! [`TreeEdit::compose`], and [`TreeEdit::apply`] (the partial monoid
//! action on [`WInstance`]).
//!
//! Each variant corresponds to a primitive mutation on a W-type tree:
//! inserting or deleting nodes, relabeling anchors, setting or removing
//! fields, moving subtrees, and manipulating hyper-edge fans.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::Edge;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::edit_error::EditError;
use crate::fan::Fan;
use crate::metadata::Node;
use crate::value::Value;
use crate::wtype::WInstance;

/// A model of `ThEditableStructure` for W-type instances.
///
/// The edit monoid is free on these generators, quotiented by the
/// equations `apply(identity(), s) = s` and
/// `apply(compose(e1, e2), s) = apply(e2, apply(e1, s))`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TreeEdit {
    /// The monoid identity: no change.
    Identity,

    /// Insert a new child node under an existing parent.
    InsertNode {
        /// Parent node ID (must exist).
        parent: u32,
        /// ID for the new child node (must not already exist).
        child_id: u32,
        /// The new node data.
        node: Node,
        /// The edge connecting parent to child.
        edge: Edge,
    },

    /// Delete a leaf node (a node with no children).
    DeleteNode {
        /// Node ID to delete.
        id: u32,
    },

    /// Contract a non-leaf node: remove it and reattach its children
    /// to its parent.
    ContractNode {
        /// Node ID to contract.
        id: u32,
    },

    /// Change a node's anchor vertex.
    RelabelNode {
        /// Node ID to relabel.
        id: u32,
        /// The new anchor vertex.
        new_anchor: Name,
    },

    /// Set or update a field in a node's `extra_fields`.
    SetField {
        /// Node ID.
        node_id: u32,
        /// Field name.
        field: Name,
        /// New value.
        value: Value,
    },

    /// Remove a field from a node's `extra_fields`.
    RemoveField {
        /// Node ID.
        node_id: u32,
        /// Field name to remove.
        field: Name,
    },

    /// Move a subtree to a new parent.
    MoveSubtree {
        /// Root of the subtree to move.
        node_id: u32,
        /// New parent node ID.
        new_parent: u32,
        /// Edge connecting new parent to the moved node.
        edge: Edge,
    },

    /// Insert a new hyper-edge fan.
    InsertFan {
        /// The fan to insert.
        fan: Fan,
    },

    /// Delete a hyper-edge fan by its hyper-edge ID.
    DeleteFan {
        /// The hyper-edge ID of the fan to remove.
        hyper_edge_id: Name,
    },

    /// Merge multiple nodes into one via spatial join
    /// (co-located annotations merge into a single node).
    JoinFeatures {
        /// The primary node that absorbs the others.
        primary: u32,
        /// Nodes to merge into the primary.
        joined: Vec<u32>,
        /// The resulting merged node data.
        produce: Node,
    },

    /// A sequence of edits applied in order.
    Sequence(Vec<Self>),
}

impl TreeEdit {
    /// The monoid identity element.
    #[must_use]
    pub const fn identity() -> Self {
        Self::Identity
    }

    /// Monoid multiplication: compose two edits into a sequence.
    ///
    /// Nested sequences are flattened and identity elements are elided.
    #[must_use]
    pub fn compose(self, other: Self) -> Self {
        let mut steps = Vec::new();
        flatten_into(&mut steps, self);
        flatten_into(&mut steps, other);
        match steps.len() {
            0 => Self::Identity,
            1 => steps.into_iter().next().unwrap_or(Self::Identity),
            _ => Self::Sequence(steps),
        }
    }

    /// Returns `true` if this edit is the identity (no-op).
    #[must_use]
    pub fn is_identity(&self) -> bool {
        match self {
            Self::Identity => true,
            Self::Sequence(steps) => steps.iter().all(Self::is_identity),
            _ => false,
        }
    }

    /// Apply this edit to a W-type instance, mutating it in place.
    ///
    /// This is the partial monoid action `E × S → S`. The action is
    /// partial because some edits fail on some states (e.g., deleting
    /// a nonexistent node).
    ///
    /// # Errors
    ///
    /// Returns [`EditError`] if the edit cannot be applied to the
    /// current instance state.
    pub fn apply(&self, instance: &mut WInstance) -> Result<(), EditError> {
        match self {
            Self::Identity => Ok(()),

            Self::InsertNode {
                parent,
                child_id,
                node,
                edge,
            } => apply_insert_node(instance, *parent, *child_id, node, edge),

            Self::DeleteNode { id } => apply_delete_node(instance, *id),

            Self::ContractNode { id } => apply_contract_node(instance, *id),

            Self::RelabelNode { id, new_anchor } => {
                let n = instance
                    .nodes
                    .get_mut(id)
                    .ok_or(EditError::NodeNotFound(*id))?;
                n.anchor = new_anchor.clone();
                Ok(())
            }

            Self::SetField {
                node_id,
                field,
                value,
            } => {
                let n = instance
                    .nodes
                    .get_mut(node_id)
                    .ok_or(EditError::NodeNotFound(*node_id))?;
                n.extra_fields.insert(field.to_string(), value.clone());
                Ok(())
            }

            Self::RemoveField { node_id, field } => {
                let n = instance
                    .nodes
                    .get_mut(node_id)
                    .ok_or(EditError::NodeNotFound(*node_id))?;
                n.extra_fields.remove(field.as_ref());
                Ok(())
            }

            Self::MoveSubtree {
                node_id,
                new_parent,
                edge,
            } => apply_move_subtree(instance, *node_id, *new_parent, edge),

            Self::InsertFan { fan } => {
                instance.fans.push(fan.clone());
                Ok(())
            }

            Self::DeleteFan { hyper_edge_id } => {
                let id_str = hyper_edge_id.as_ref();
                let before = instance.fans.len();
                instance.fans.retain(|f| f.hyper_edge_id != id_str);
                if instance.fans.len() == before {
                    return Err(EditError::FanNotFound(id_str.to_owned()));
                }
                Ok(())
            }

            Self::JoinFeatures {
                primary,
                joined,
                produce,
            } => apply_join_features(instance, *primary, joined, produce),

            Self::Sequence(steps) => {
                for step in steps {
                    step.apply(instance)?;
                }
                Ok(())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Flatten nested sequences and strip identities.
fn flatten_into(out: &mut Vec<TreeEdit>, edit: TreeEdit) {
    match edit {
        TreeEdit::Identity => {}
        TreeEdit::Sequence(steps) => {
            for step in steps {
                flatten_into(out, step);
            }
        }
        other => out.push(other),
    }
}

fn apply_insert_node(
    instance: &mut WInstance,
    parent: u32,
    child_id: u32,
    node: &Node,
    edge: &Edge,
) -> Result<(), EditError> {
    if !instance.nodes.contains_key(&parent) {
        return Err(EditError::ParentNotFound(parent));
    }
    if instance.nodes.contains_key(&child_id) {
        return Err(EditError::DuplicateNodeId(child_id));
    }
    instance.nodes.insert(child_id, node.clone());
    let arc = (parent, child_id, edge.clone());
    instance.arcs.push(arc);
    instance.parent_map.insert(child_id, parent);
    instance
        .children_map
        .entry(parent)
        .or_default()
        .push(child_id);
    Ok(())
}

fn apply_delete_node(instance: &mut WInstance, id: u32) -> Result<(), EditError> {
    if !instance.nodes.contains_key(&id) {
        return Err(EditError::NodeNotFound(id));
    }
    // Only allow deleting leaf nodes (no children).
    let children = instance.children_map.get(&id);
    if children.is_some_and(|c| !c.is_empty()) {
        return Err(EditError::ApplyFailed(format!(
            "cannot delete non-leaf node {id}; use ContractNode to remove and reattach children"
        )));
    }
    instance.nodes.remove(&id);
    // Remove arcs pointing to this node.
    instance.arcs.retain(|&(_, child, _)| child != id);
    // Remove from parent's children map.
    if let Some(&parent_id) = instance.parent_map.get(&id) {
        if let Some(siblings) = instance.children_map.get_mut(&parent_id) {
            siblings.retain(|&c| c != id);
        }
    }
    instance.parent_map.remove(&id);
    instance.children_map.remove(&id);
    // Remove any fans referencing this node.
    instance
        .fans
        .retain(|f| f.parent != id && !f.children.values().any(|&c| c == id));
    Ok(())
}

fn apply_contract_node(instance: &mut WInstance, id: u32) -> Result<(), EditError> {
    if !instance.nodes.contains_key(&id) {
        return Err(EditError::NodeNotFound(id));
    }
    if id == instance.root {
        return Err(EditError::ApplyFailed(
            "cannot contract the root node".to_owned(),
        ));
    }
    let parent_id = instance
        .parent_map
        .get(&id)
        .copied()
        .ok_or_else(|| EditError::ApplyFailed(format!("node {id} has no parent")))?;

    // Collect this node's children.
    let children: SmallVec<u32, 4> = instance.children_map.get(&id).cloned().unwrap_or_default();

    // Collect the original child edges before removal (for re-parenting).
    let child_edges: HashMap<u32, Edge> = instance
        .arcs
        .iter()
        .filter(|&&(p, _, _)| p == id)
        .map(|&(_, c, ref e)| (c, e.clone()))
        .collect();

    // Find the edge from parent to this node.
    let parent_edge: Option<Edge> = instance
        .arcs
        .iter()
        .find(|&&(p, c, _)| p == parent_id && c == id)
        .map(|(_, _, e)| e.clone());

    // Remove arcs from parent to this node and from this node to children.
    instance
        .arcs
        .retain(|&(p, c, _)| !((p == parent_id && c == id) || (p == id)));

    // Reattach children to parent using the original child edge (with src
    // updated to the parent's anchor) or the parent edge as fallback.
    for &child_id in &children {
        let edge = if let Some(mut child_edge) = child_edges.get(&child_id).cloned() {
            // Use the original edge from contracted node to child, updating src.
            if let Some(parent_node) = instance.nodes.get(&parent_id) {
                child_edge.src.clone_from(&parent_node.anchor);
            }
            child_edge
        } else if let Some(pe) = parent_edge.clone() {
            // Fall back to the edge from parent to contracted node, updating tgt.
            let mut e = pe;
            if let Some(child_node) = instance.nodes.get(&child_id) {
                e.tgt.clone_from(&child_node.anchor);
            }
            e
        } else {
            // Last resort: construct from actual node anchors.
            let src = instance
                .nodes
                .get(&parent_id)
                .map_or_else(|| Name::from(&*id.to_string()), |n| n.anchor.clone());
            let tgt = instance
                .nodes
                .get(&child_id)
                .map_or_else(|| Name::from(&*child_id.to_string()), |n| n.anchor.clone());
            Edge {
                src,
                tgt,
                kind: "prop".into(),
                name: None,
            }
        };
        instance.arcs.push((parent_id, child_id, edge));
        instance.parent_map.insert(child_id, parent_id);
        instance
            .children_map
            .entry(parent_id)
            .or_default()
            .push(child_id);
    }

    // Remove the contracted node.
    instance.nodes.remove(&id);
    instance.parent_map.remove(&id);
    if let Some(siblings) = instance.children_map.get_mut(&parent_id) {
        siblings.retain(|&c| c != id);
    }
    instance.children_map.remove(&id);

    // Clean up fans referencing the contracted node.
    instance
        .fans
        .retain(|f| f.parent != id && !f.children.values().any(|&c| c == id));

    Ok(())
}

fn apply_move_subtree(
    instance: &mut WInstance,
    node_id: u32,
    new_parent: u32,
    edge: &Edge,
) -> Result<(), EditError> {
    if !instance.nodes.contains_key(&node_id) {
        return Err(EditError::NodeNotFound(node_id));
    }
    if !instance.nodes.contains_key(&new_parent) {
        return Err(EditError::ParentNotFound(new_parent));
    }
    // Prevent cycles: new_parent must not be a descendant of node_id.
    if is_descendant(instance, new_parent, node_id) {
        return Err(EditError::CycleDetected {
            node_id,
            new_parent,
        });
    }

    // Remove old parent arc.
    let old_parent = instance.parent_map.get(&node_id).copied();
    instance.arcs.retain(|&(_, child, _)| child != node_id);
    if let Some(old_p) = old_parent {
        if let Some(siblings) = instance.children_map.get_mut(&old_p) {
            siblings.retain(|&c| c != node_id);
        }
    }

    // Insert new parent arc.
    instance.arcs.push((new_parent, node_id, edge.clone()));
    instance.parent_map.insert(node_id, new_parent);
    instance
        .children_map
        .entry(new_parent)
        .or_default()
        .push(node_id);

    Ok(())
}

/// Check if `candidate` is a descendant of `ancestor` in the instance tree.
fn is_descendant(instance: &WInstance, candidate: u32, ancestor: u32) -> bool {
    let mut current = candidate;
    while let Some(&parent) = instance.parent_map.get(&current) {
        if parent == ancestor {
            return true;
        }
        current = parent;
    }
    false
}

fn apply_join_features(
    instance: &mut WInstance,
    primary: u32,
    joined: &[u32],
    produce: &Node,
) -> Result<(), EditError> {
    if !instance.nodes.contains_key(&primary) {
        return Err(EditError::NodeNotFound(primary));
    }
    for &jid in joined {
        if !instance.nodes.contains_key(&jid) {
            return Err(EditError::NodeNotFound(jid));
        }
    }

    // Replace the primary node with the merged produce.
    instance.nodes.insert(primary, produce.clone());

    // Delete the joined nodes (they are absorbed).
    for &jid in joined {
        // Remove arcs.
        instance.arcs.retain(|&(_, child, _)| child != jid);
        instance.arcs.retain(|&(parent, _, _)| parent != jid);
        // Clean up maps.
        if let Some(&parent_id) = instance.parent_map.get(&jid) {
            if let Some(siblings) = instance.children_map.get_mut(&parent_id) {
                siblings.retain(|&c| c != jid);
            }
        }
        instance.nodes.remove(&jid);
        instance.parent_map.remove(&jid);
        instance.children_map.remove(&jid);
    }

    // Remove fans referencing joined nodes.
    instance.fans.retain(|f| {
        !joined.contains(&f.parent) && !f.children.values().any(|c| joined.contains(c))
    });

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::redundant_clone)]
mod tests {
    use std::collections::HashMap;

    use panproto_gat::Name;
    use panproto_schema::Edge;

    use crate::metadata::Node;
    use crate::value::Value;
    use crate::wtype::WInstance;

    use super::TreeEdit;

    fn sample_edge(src: &str, tgt: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "prop".into(),
            name: None,
        }
    }

    fn two_node_instance() -> WInstance {
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(1, Node::new(1, "child"));
        let arcs = vec![(0, 1, sample_edge("root", "child"))];
        WInstance::new(nodes, arcs, vec![], 0, Name::from("root"))
    }

    #[test]
    fn identity_is_noop() {
        let mut inst = two_node_instance();
        let before = inst.nodes.len();
        TreeEdit::identity().apply(&mut inst).unwrap();
        assert_eq!(inst.nodes.len(), before);
    }

    #[test]
    fn insert_then_delete_is_identity() {
        let mut inst = two_node_instance();
        let original_count = inst.nodes.len();
        let node = Node::new(99, "new_child");
        let edge = sample_edge("root", "new_child");

        let insert = TreeEdit::InsertNode {
            parent: 0,
            child_id: 99,
            node,
            edge,
        };
        let delete = TreeEdit::DeleteNode { id: 99 };
        let composed = insert.compose(delete);
        composed.apply(&mut inst).unwrap();

        assert_eq!(inst.nodes.len(), original_count);
    }

    #[test]
    fn set_field_updates_value() {
        let mut inst = two_node_instance();
        let edit = TreeEdit::SetField {
            node_id: 1,
            field: Name::from("color"),
            value: Value::Str("blue".into()),
        };
        edit.apply(&mut inst).unwrap();
        assert_eq!(
            inst.nodes[&1].extra_fields.get("color"),
            Some(&Value::Str("blue".into()))
        );
    }

    #[test]
    fn remove_field() {
        let mut inst = two_node_instance();
        inst.nodes
            .get_mut(&1)
            .unwrap()
            .extra_fields
            .insert("color".into(), Value::Str("red".into()));

        let edit = TreeEdit::RemoveField {
            node_id: 1,
            field: Name::from("color"),
        };
        edit.apply(&mut inst).unwrap();
        assert!(!inst.nodes[&1].extra_fields.contains_key("color"));
    }

    #[test]
    fn relabel_node() {
        let mut inst = two_node_instance();
        let edit = TreeEdit::RelabelNode {
            id: 1,
            new_anchor: Name::from("renamed_child"),
        };
        edit.apply(&mut inst).unwrap();
        assert_eq!(inst.nodes[&1].anchor, Name::from("renamed_child"));
    }

    #[test]
    fn move_subtree_reparents() {
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(1, Node::new(1, "a"));
        nodes.insert(2, Node::new(2, "b"));
        let arcs = vec![
            (0, 1, sample_edge("root", "a")),
            (0, 2, sample_edge("root", "b")),
        ];
        let mut inst = WInstance::new(nodes, arcs, vec![], 0, Name::from("root"));

        let edit = TreeEdit::MoveSubtree {
            node_id: 2,
            new_parent: 1,
            edge: sample_edge("a", "b"),
        };
        edit.apply(&mut inst).unwrap();

        assert_eq!(inst.parent_map[&2], 1);
        assert!(inst.children_map[&1].contains(&2));
    }

    #[test]
    fn contract_node_reattaches_children() {
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(1, Node::new(1, "middle"));
        nodes.insert(2, Node::new(2, "leaf"));
        let arcs = vec![
            (0, 1, sample_edge("root", "middle")),
            (1, 2, sample_edge("middle", "leaf")),
        ];
        let mut inst = WInstance::new(nodes, arcs, vec![], 0, Name::from("root"));

        let edit = TreeEdit::ContractNode { id: 1 };
        edit.apply(&mut inst).unwrap();

        assert!(!inst.nodes.contains_key(&1));
        assert_eq!(inst.parent_map[&2], 0);
        assert!(inst.children_map[&0].contains(&2));
    }

    #[test]
    fn sequence_flattening() {
        let e1 = TreeEdit::Identity;
        let e2 = TreeEdit::Sequence(vec![TreeEdit::Identity, TreeEdit::Identity]);
        let composed = e1.compose(e2);
        assert!(composed.is_identity());
    }

    #[test]
    fn monoid_associativity() {
        let node_a = Node::new(10, "a");
        let node_b = Node::new(11, "b");
        let edge_a = sample_edge("root", "a");
        let edge_b = sample_edge("root", "b");

        let e1 = TreeEdit::InsertNode {
            parent: 0,
            child_id: 10,
            node: node_a.clone(),
            edge: edge_a.clone(),
        };
        let e2 = TreeEdit::InsertNode {
            parent: 0,
            child_id: 11,
            node: node_b.clone(),
            edge: edge_b.clone(),
        };
        let e3 = TreeEdit::DeleteNode { id: 10 };

        // (e1 . e2) . e3
        let left = e1.clone().compose(e2.clone()).compose(e3.clone());
        // e1 . (e2 . e3)
        let right = e1.compose(e2.compose(e3));

        let mut inst_l = two_node_instance();
        let mut inst_r = two_node_instance();
        left.apply(&mut inst_l).unwrap();
        right.apply(&mut inst_r).unwrap();

        assert_eq!(inst_l.nodes.len(), inst_r.nodes.len());
        for (id, node) in &inst_l.nodes {
            let other = inst_r.nodes.get(id).expect("node should exist in both");
            assert_eq!(node.anchor, other.anchor);
        }
    }

    #[test]
    fn monoid_identity_law() {
        let edit = TreeEdit::SetField {
            node_id: 1,
            field: Name::from("x"),
            value: Value::Int(42),
        };

        let mut inst1 = two_node_instance();
        let mut inst2 = two_node_instance();

        // apply(compose(identity, e), s) == apply(e, s)
        TreeEdit::identity()
            .compose(edit.clone())
            .apply(&mut inst1)
            .unwrap();
        edit.apply(&mut inst2).unwrap();

        assert_eq!(
            inst1.nodes[&1].extra_fields.get("x"),
            inst2.nodes[&1].extra_fields.get("x"),
        );
    }

    #[test]
    fn delete_nonexistent_node_fails() {
        let mut inst = two_node_instance();
        let err = TreeEdit::DeleteNode { id: 999 }.apply(&mut inst);
        assert!(err.is_err());
    }

    #[test]
    fn insert_duplicate_id_fails() {
        let mut inst = two_node_instance();
        let edit = TreeEdit::InsertNode {
            parent: 0,
            child_id: 1, // already exists
            node: Node::new(1, "dup"),
            edge: sample_edge("root", "dup"),
        };
        assert!(edit.apply(&mut inst).is_err());
    }

    #[test]
    fn move_creates_cycle_fails() {
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(1, Node::new(1, "child"));
        nodes.insert(2, Node::new(2, "grandchild"));
        let arcs = vec![
            (0, 1, sample_edge("root", "child")),
            (1, 2, sample_edge("child", "grandchild")),
        ];
        let mut inst = WInstance::new(nodes, arcs, vec![], 0, Name::from("root"));

        // Moving node 0 (root) under node 2 (grandchild) would create a cycle,
        // but root has no parent so MoveSubtree would try to reattach.
        // Instead, move node 1 under node 2 (its own child).
        let edit = TreeEdit::MoveSubtree {
            node_id: 1,
            new_parent: 2,
            edge: sample_edge("grandchild", "child"),
        };
        assert!(edit.apply(&mut inst).is_err());
    }
}
