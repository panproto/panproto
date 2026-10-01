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

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_schema::Edge;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::edit_error::EditError;
use crate::fan::Fan;
use crate::ginstance::{
    EdgeId, GEdge, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND, TreeEmbedding, write_node_record,
};
use crate::graph_edit::{AttrKey, AttrVal, GraphDelta};
use crate::identity::{CanonicalSink, Digest, TREE_EDIT_CONTEXT};
use crate::metadata::{Node, NodeShape};
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
        /// The new node data, which must carry `child_id` as its id.
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
    ///
    /// `produce` replaces the primary node and must carry its id. Each
    /// joined node is deleted with its in-arc, and its children move to the
    /// primary: in `joined` order, and for each joined node in the order of
    /// its child arcs, each child keeps its arc's schema edge with the
    /// source set to `produce`'s anchor, appended after every other arc.
    /// Fans that name a joined node are deleted.
    JoinFeatures {
        /// The primary node that absorbs the others.
        primary: u32,
        /// Nodes to merge into the primary, without repeats; neither the
        /// primary, nor the root, nor an ancestor of the primary.
        joined: Vec<u32>,
        /// The resulting merged node data, with the primary's id.
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
    if node.id != child_id {
        return Err(EditError::ApplyFailed(format!(
            "the inserted node {} is filed under {child_id}",
            node.id
        )));
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
    if id == instance.root {
        return Err(EditError::ApplyFailed(
            "cannot delete the root node".to_owned(),
        ));
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
    // Prevent cycles: a node cannot be its own parent, and `new_parent`
    // must not be a descendant of `node_id`.
    //
    // The self-parent case needs its own test. `is_descendant` walks up
    // from `new_parent`, so it starts at the node's parent and never
    // considers the node itself; with `node_id == new_parent` it reports
    // false on an acyclic instance and the move then writes
    // `parent_map[node_id] = node_id`. That self-loop is not merely
    // wrong, it is unwalkable: every later ancestor traversal through
    // the node spins forever.
    if node_id == new_parent || is_descendant(instance, new_parent, node_id) {
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
///
/// The walk is bounded by the node count, so a `parent_map` that already
/// carries a cycle answers `false` rather than looping. Nothing in this
/// module can now introduce such a cycle, but the map is part of a
/// deserialized `WInstance` and reaches here from hosts across the FFI
/// boundary; a predicate that hangs on malformed input is a denial of
/// service rather than a wrong answer, and a chain longer than every
/// node in the instance has necessarily revisited one.
fn is_descendant(instance: &WInstance, candidate: u32, ancestor: u32) -> bool {
    let mut current = candidate;
    for _ in 0..instance.parent_map.len() {
        let Some(&parent) = instance.parent_map.get(&current) else {
            return false;
        };
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
    if produce.id != primary {
        return Err(EditError::ApplyFailed(format!(
            "the product of joining into node {primary} carries the id {}",
            produce.id
        )));
    }
    let mut seen = std::collections::HashSet::with_capacity(joined.len());
    for &jid in joined {
        if !instance.nodes.contains_key(&jid) {
            return Err(EditError::NodeNotFound(jid));
        }
        if jid == primary {
            return Err(EditError::ApplyFailed(format!(
                "node {primary} cannot be joined into itself"
            )));
        }
        if jid == instance.root {
            return Err(EditError::ApplyFailed(format!(
                "the root node {jid} cannot be joined into node {primary}"
            )));
        }
        if !seen.insert(jid) {
            return Err(EditError::ApplyFailed(format!(
                "node {jid} is joined into node {primary} twice"
            )));
        }
        // A joined ancestor of the primary would hand the primary to itself.
        if is_descendant(instance, primary, jid) {
            return Err(EditError::CycleDetected {
                node_id: jid,
                new_parent: primary,
            });
        }
    }

    // Replace the primary node with the merged produce.
    instance.nodes.insert(primary, produce.clone());

    for &jid in joined {
        // The joined node's child arcs, in arc order, become arcs of the
        // primary appended after every other arc.
        let mut moved = Vec::new();
        instance.arcs.retain(|(parent, child, edge)| {
            if *child == jid {
                return false;
            }
            if *parent == jid {
                moved.push((*child, edge.clone()));
                return false;
            }
            true
        });
        if let Some(parent_id) = instance.parent_map.remove(&jid)
            && let Some(siblings) = instance.children_map.get_mut(&parent_id)
        {
            siblings.retain(|c| *c != jid);
        }
        instance.children_map.remove(&jid);
        instance.nodes.remove(&jid);
        for (child, mut edge) in moved {
            edge.src.clone_from(&produce.anchor);
            instance.arcs.push((primary, child, edge));
            instance.parent_map.insert(child, primary);
            instance
                .children_map
                .entry(primary)
                .or_default()
                .push(child);
        }
    }

    // Remove fans referencing joined nodes.
    instance.fans.retain(|f| {
        !joined.contains(&f.parent) && !f.children.values().any(|c| joined.contains(c))
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Canonical bytes
// ---------------------------------------------------------------------------

impl TreeEdit {
    /// The steps of this edit in normal form: nested sequences spliced in
    /// order and every `Identity` removed. A single edit other than a
    /// sequence is its own one step; the identity has none.
    #[must_use]
    pub fn steps(&self) -> Vec<&Self> {
        fn walk<'a>(edit: &'a TreeEdit, out: &mut Vec<&'a TreeEdit>) {
            match edit {
                TreeEdit::Identity => {}
                TreeEdit::Sequence(steps) => steps.iter().for_each(|s| walk(s, out)),
                step => out.push(step),
            }
        }
        let mut out = Vec::new();
        walk(self, &mut out);
        out
    }

    /// The digest of this edit: its canonical bytes
    /// ([`canonical_tree_edit_bytes`]) under
    /// [`TREE_EDIT_CONTEXT`], computed by
    /// streaming the steps into the hasher. An edit and the one-step
    /// sequence of it have one digest.
    ///
    /// ```
    /// use panproto_inst::TreeEdit;
    /// use panproto_inst::identity::{Digest, TREE_EDIT_CONTEXT};
    /// use panproto_inst::tree_edit::canonical_tree_edit_bytes;
    /// use panproto_schema::CanonicalWriter;
    ///
    /// let edit = TreeEdit::DeleteNode { id: 3 };
    /// let mut writer = CanonicalWriter::new();
    /// canonical_tree_edit_bytes(&edit, &mut writer);
    /// assert_eq!(edit.digest(), Digest::derive(TREE_EDIT_CONTEXT, writer.as_bytes()));
    /// assert_eq!(edit.digest(), TreeEdit::Sequence(vec![edit.clone()]).digest());
    /// ```
    #[must_use]
    pub fn digest(&self) -> Digest {
        let mut hasher = Digest::hasher(TREE_EDIT_CONTEXT);
        canonical_tree_edit_bytes(self, &mut hasher);
        hasher.finalize()
    }
}

/// Write the canonical bytes of a tree edit into `sink`.
///
/// The edit is first brought into normal form ([`TreeEdit::steps`]): nested
/// sequences spliced in order and every `Identity` removed, so an edit `e`
/// and `Sequence[e]` have one encoding, and the identity is the empty
/// sequence.
///
/// ```text
/// header = step count                       (u64)
/// step   = tag: u8, then the fields          (in sequence order)
/// ```
///
/// The tag is the variant's position in declaration order (`1`
/// `InsertNode` to `10` `JoinFeatures`; `0` `Identity` and `11` `Sequence`
/// never occur). Fields follow in declaration order: node ids as `u32`, a
/// node as its node record ([`write_node_record`]), a schema edge through
/// [`CanonicalSink::edge`], a name or field name through
/// [`CanonicalSink::name`], a value through [`CanonicalSink::value`], a fan
/// as its hyper-edge id, its parent and its children (count, then each label
/// and child in ascending label order), and the joined nodes of
/// `JoinFeatures` as a count and then each id in the edit's order.
///
/// ```
/// use panproto_inst::TreeEdit;
/// use panproto_inst::tree_edit::canonical_tree_edit_bytes;
/// use panproto_schema::CanonicalWriter;
///
/// let mut writer = CanonicalWriter::new();
/// canonical_tree_edit_bytes(&TreeEdit::Identity, &mut writer);
/// // No steps.
/// assert_eq!(writer.into_bytes(), vec![0u8; 8]);
/// ```
pub fn canonical_tree_edit_bytes<S: CanonicalSink + ?Sized>(edit: &TreeEdit, sink: &mut S) {
    let steps = edit.steps();
    sink.len(steps.len());
    for step in steps {
        write_step(sink, step);
    }
}

/// Write one step of a tree edit in normal form.
fn write_step<S: CanonicalSink + ?Sized>(sink: &mut S, step: &TreeEdit) {
    match step {
        TreeEdit::Identity => sink.tag(0),
        TreeEdit::InsertNode {
            parent,
            child_id,
            node,
            edge,
        } => {
            sink.tag(1);
            sink.u32(*parent);
            sink.u32(*child_id);
            write_node_record(sink, node);
            sink.edge(edge);
        }
        TreeEdit::DeleteNode { id } => {
            sink.tag(2);
            sink.u32(*id);
        }
        TreeEdit::ContractNode { id } => {
            sink.tag(3);
            sink.u32(*id);
        }
        TreeEdit::RelabelNode { id, new_anchor } => {
            sink.tag(4);
            sink.u32(*id);
            sink.name(new_anchor);
        }
        TreeEdit::SetField {
            node_id,
            field,
            value,
        } => {
            sink.tag(5);
            sink.u32(*node_id);
            sink.name(field);
            sink.value(value);
        }
        TreeEdit::RemoveField { node_id, field } => {
            sink.tag(6);
            sink.u32(*node_id);
            sink.name(field);
        }
        TreeEdit::MoveSubtree {
            node_id,
            new_parent,
            edge,
        } => {
            sink.tag(7);
            sink.u32(*node_id);
            sink.u32(*new_parent);
            sink.edge(edge);
        }
        TreeEdit::InsertFan { fan } => {
            sink.tag(8);
            sink.str(&fan.hyper_edge_id);
            sink.u32(fan.parent);
            let children: std::collections::BTreeMap<&String, &u32> = fan.children.iter().collect();
            sink.len(children.len());
            for (label, child) in children {
                sink.str(label);
                sink.u32(*child);
            }
        }
        TreeEdit::DeleteFan { hyper_edge_id } => {
            sink.tag(9);
            sink.name(hyper_edge_id);
        }
        TreeEdit::JoinFeatures {
            primary,
            joined,
            produce,
        } => {
            sink.tag(10);
            sink.u32(*primary);
            sink.len(joined.len());
            for id in joined {
                sink.u32(*id);
            }
            write_node_record(sink, produce);
        }
        TreeEdit::Sequence(steps) => {
            sink.tag(11);
            sink.len(steps.len());
            for inner in steps {
                write_step(sink, inner);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tree edits and graph deltas
// ---------------------------------------------------------------------------

fn not_graph_tree(reason: impl std::fmt::Display) -> EditError {
    EditError::ApplyFailed(format!(
        "the graph is not the tree its embedding embeds: {reason}"
    ))
}

fn inst_failure(err: &crate::error::InstError) -> EditError {
    EditError::ApplyFailed(err.to_string())
}

/// The names of the fan parent edges of `graph`, by edge id: the graph form
/// of a tree names a fan's parent edge by the label its schema declares,
/// which the tree does not hold.
fn parent_names(graph: &GInstance, embedding: &TreeEmbedding) -> BTreeMap<EdgeId, Option<Name>> {
    embedding
        .fans
        .iter()
        .filter_map(|fan| {
            graph
                .edge(fan.parent_edge)
                .map(|edge| (fan.parent_edge, edge.edge.name.clone()))
        })
        .collect()
}

/// The fan node ids of an embedding.
fn fan_nodes(embedding: &TreeEmbedding) -> BTreeSet<u32> {
    embedding.fans.iter().map(|fan| fan.node).collect()
}

/// The tree `graph` is when read through `embedding`: every node but the
/// fan nodes, the arcs the embedding records in ascending edge id, and the
/// fans in the embedding's order. Its root is the one node no arc enters;
/// its schema root is the root's anchor.
fn read_tree(graph: &GInstance, embedding: &TreeEmbedding) -> Result<WInstance, EditError> {
    let fans = fan_nodes(embedding);
    let nodes: HashMap<u32, Node> = graph
        .nodes()
        .iter()
        .filter(|(id, _)| !fans.contains(id))
        .map(|(id, node)| (*id, node.clone()))
        .collect();
    let mut arc_ids: Vec<(EdgeId, u32)> = embedding.arcs.iter().map(|(c, e)| (*e, *c)).collect();
    arc_ids.sort_unstable();
    let mut arcs = Vec::with_capacity(arc_ids.len());
    for (id, child) in arc_ids {
        let edge = graph
            .edge(id)
            .ok_or_else(|| not_graph_tree(format!("arc edge {id} is missing")))?;
        if edge.tgt != child {
            return Err(not_graph_tree(format!(
                "arc edge {id} does not enter node {child}"
            )));
        }
        arcs.push((edge.src, child, edge.edge.clone()));
    }
    let mut tree_fans = Vec::with_capacity(embedding.fans.len());
    for fan in &embedding.fans {
        let node = graph
            .node(fan.node)
            .ok_or_else(|| not_graph_tree(format!("fan node {} is missing", fan.node)))?;
        let target = |id: EdgeId| {
            graph
                .edge(id)
                .map(|e| e.tgt)
                .ok_or_else(|| not_graph_tree(format!("fan edge {id} is missing")))
        };
        let mut children = HashMap::with_capacity(fan.child_edges.len());
        for (label, id) in &fan.child_edges {
            children.insert(label.clone(), target(*id)?);
        }
        tree_fans.push(Fan {
            hyper_edge_id: node.anchor.to_string(),
            parent: target(fan.parent_edge)?,
            children,
        });
    }
    let mut roots = nodes.keys().filter(|id| !embedding.arcs.contains_key(id));
    let (Some(&root), None) = (roots.next(), roots.next()) else {
        return Err(not_graph_tree("it does not have exactly one root"));
    };
    let schema_root = nodes[&root].anchor.clone();
    Ok(WInstance::new(nodes, arcs, tree_fans, root, schema_root))
}

/// The graph form of `tree` under `embedding`, with the fan parent edges
/// named as `names` records them.
fn graph_under(
    tree: &WInstance,
    embedding: &TreeEmbedding,
    names: &BTreeMap<EdgeId, Option<Name>>,
) -> Result<GInstance, EditError> {
    let mut graph =
        GInstance::from_wtype_under(tree, embedding, None).map_err(|e| inst_failure(&e))?;
    for fan in &embedding.fans {
        if let (Some(Some(name)), Some(edge)) =
            (names.get(&fan.parent_edge), graph.edge(fan.parent_edge))
        {
            let mut label = edge.edge.clone();
            label.name = Some(name.clone());
            graph
                .relabel_edge(fan.parent_edge, label)
                .map_err(|e| inst_failure(&e))?;
        }
    }
    Ok(graph)
}

/// The delta of one tree edit step between the graph forms `before` and
/// `after`: their diff, except that a node id that is a fan node on one side
/// and a tree node on the other (a fan relocated away from an id a tree node
/// takes) is a replacement, since it names two different elements.
fn step_delta(
    before: &GInstance,
    after: &GInstance,
    fans_before: &BTreeSet<u32>,
    fans_after: &BTreeSet<u32>,
) -> Result<GraphDelta, EditError> {
    let mut parts = GraphDelta::diff(before, after).into_parts();
    for id in fans_before.symmetric_difference(fans_after) {
        if let (Some(old), Some(new)) = (before.node(*id), after.node(*id)) {
            parts
                .attrs
                .retain(|key, _| key.element() != crate::graph_edit::ElemRef::Node(*id));
            parts.del_nodes.insert(*id, old.clone());
            parts.add_nodes.insert(*id, new.clone());
        }
    }
    GraphDelta::normalize(parts).map_err(|e| EditError::ApplyFailed(e.to_string()))
}

impl GraphDelta {
    /// The graph delta of a tree edit: with `W` the tree `graph` is when
    /// read through `embedding`, applying the result to `graph` gives the
    /// graph form of `edit` applied to `W`, under the embedding as it
    /// follows the edit ([`TreeEmbedding::apply_edit`]), which `embedding`
    /// becomes.
    ///
    /// Each step lowers to the change of the graph form it makes, and a
    /// sequence to the composite of its steps ([`GraphDelta::compose`]).
    /// `InsertNode` adds the node and an arc with a fresh edge id;
    /// `DeleteNode` deletes the node, its in-arc and the fans that name it;
    /// `ContractNode` deletes the node, its in-arc, its out-arcs and the fans
    /// that name it and adds the re-parented arcs with fresh ids and the
    /// labels tree apply gives them; `RelabelNode` is an `Anchor` change (and
    /// a label change of each fan edge that reaches the node, whose label
    /// names its target's anchor); `SetField` and `RemoveField` are `Field`
    /// changes with the old value read from the state; `MoveSubtree` deletes
    /// the old arc and adds the new one with a fresh id; `InsertFan` and
    /// `DeleteFan` add and delete fan nodes with their edges; `JoinFeatures`
    /// deletes the joined nodes and their arcs, adds the re-parented arcs
    /// with fresh ids (in joined order, then each joined node's child arc
    /// order), and changes the primary's `Anchor`, `Value`, `Meta` and
    /// `Field` entries to those of the product. A tree node taking the id of
    /// a fan node relocates the fan node: that id is replaced, and the fan
    /// node and its edges move to the fan's new node id. Fresh ids come from
    /// `embedding.next_edge` and `embedding.next_node` in apply order. A fan
    /// parent edge keeps the name `graph` gives it; a new fan's parent edge
    /// is unnamed.
    ///
    /// The cost is that of rebuilding the graph form once per step.
    ///
    /// # Errors
    ///
    /// Returns [`EditError::ApplyFailed`] when `graph` is not the graph form
    /// of a tree under `embedding` (up to the names of fan parent edges),
    /// and otherwise the error of the first step that does not apply to the
    /// tree, as [`TreeEmbedding::apply_edit`] reports it. On error
    /// `embedding` is unchanged.
    pub fn from_tree_edit(
        graph: &GInstance,
        embedding: &mut TreeEmbedding,
        edit: &TreeEdit,
    ) -> Result<Self, EditError> {
        let names = parent_names(graph, embedding);
        let mut tree = read_tree(graph, embedding)?;
        let mut current = graph_under(&tree, embedding, &names)?;
        if !GInstance::identical(&current, graph) {
            return Err(not_graph_tree(
                "its nodes and edges are not those of the tree read through it",
            ));
        }
        let mut followed = embedding.clone();
        let mut net = Self::identity();
        for step in edit.steps() {
            let fans_before = fan_nodes(&followed);
            followed.apply_edit(&mut tree, step)?;
            let stepped = graph_under(&tree, &followed, &names)?;
            let delta = step_delta(&current, &stepped, &fans_before, &fan_nodes(&followed))?;
            net = net.compose(&delta)?;
            current = stepped;
        }
        *embedding = followed;
        Ok(net)
    }
}

const fn not_tree_edit(reason: String) -> EditError {
    EditError::NotTreeEdit { reason }
}

/// Whether an edge is a fan edge: of a reserved kind, or leaving a fan node.
fn is_fan_edge(edge: &GEdge, fan_nodes: &BTreeSet<u32>) -> bool {
    edge.edge.kind == HYPER_PARENT_KIND
        || edge.edge.kind == HYPER_LABEL_KIND
        || fan_nodes.contains(&edge.src)
}

/// The depth of a node in `tree`: the number of arcs above it.
fn depth(tree: &WInstance, id: u32) -> usize {
    let mut depth = 0;
    let mut current = id;
    while let Some(&parent) = tree.parent_map.get(&current) {
        depth += 1;
        current = parent;
        if depth > tree.parent_map.len() {
            break;
        }
    }
    depth
}

/// A tree edit step that allocates ids, with the first edge id it takes
/// and, for a new fan, the node id the delta gives the fan and the highest
/// node id under which the step may insert it (one below the lowest node
/// id the delta gives a fan inserted after it).
#[derive(Clone)]
struct Allocating {
    first_edge: EdgeId,
    fan: Option<(u32, u32)>,
    step: TreeEdit,
}

/// The tree edit a graph delta reads as, built step by step on the tree
/// it applies to, so that each id-allocating step can first spend the ids
/// below those it must take.
///
/// A delta whose edit ran steps that a later step undid (a move superseded
/// by another move, an insertion deleted again, a fan inserted and
/// deleted, a fan moved to a fresh node id by a tree node that took its
/// id) keeps the ids those steps took unused: its arcs and fans take ids
/// above them. The steps added here spend such ids without changing the
/// delta. An edge id is spent by placing the node an `InsertNode` or
/// `MoveSubtree` places under other parents first; by moving away a node
/// the delta moves under the parent it has; by a scratch node inserted
/// under the root and deleted again; or by moving a node the delta deletes.
/// A node id is spent by a scratch node under it. A fan reaches the node
/// id the delta gives it by a tree node (the delta's own, inserted early,
/// or a scratch one) taking its id. A new node a step names before the
/// step that places it is inserted early, and a node in the way of a move
/// under its own descendant is moved out first.
#[derive(Clone)]
struct Replay<'a> {
    delta: &'a GraphDelta,
    tree: WInstance,
    embedding: TreeEmbedding,
    steps: Vec<TreeEdit>,
    /// The fans the delta places under node ids no step of the edit has
    /// given them yet, as `(node, parent edge)`, in ascending node order.
    relocations: Vec<(u32, EdgeId)>,
    /// The node ids under which the new fans still to be inserted should
    /// go, in the order of their steps.
    upcoming: Vec<u32>,
    /// The scratch nodes in the tree. They stay until the edit's last
    /// deletions, so that a later move of a node under the parent it has
    /// does not find its arc the last one (which would keep its id), and
    /// they hold no other node.
    held: BTreeSet<u32>,
    /// The tree nodes the delta deletes and adds again under the same id
    /// whose old node is still in the tree.
    replaced: BTreeSet<u32>,
    /// Steps that go right before the next id-allocating step that adds a
    /// fan, once the ids below its own are spent.
    just_before: Vec<TreeEdit>,
    /// The tree nodes the delta deletes, with which a fan they name may go
    /// before a tree node takes its node id.
    clear_fans: BTreeSet<u32>,
    /// Whether fan deletions go last: a fan a tree node takes the id of
    /// then moves on, unless that would take a node id a fan still to be
    /// placed needs.
    fans_last: bool,
    /// The `DeleteFan` steps still to make, when fan deletions go last.
    late: Vec<TreeEdit>,
    /// The tree nodes deleted to clear a fan's node id.
    cleared: BTreeSet<u32>,
    /// The first edge id of the id-allocating step being made.
    first_edge: EdgeId,
}

impl<'a> Replay<'a> {
    fn new(
        tree: &WInstance,
        embedding: &TreeEmbedding,
        delta: &'a GraphDelta,
        mut relocations: Vec<(u32, EdgeId)>,
    ) -> Self {
        relocations.sort_unstable();
        let replaced = delta
            .del_nodes()
            .keys()
            .copied()
            .filter(|id| {
                tree.nodes.contains_key(id)
                    && delta
                        .add_nodes()
                        .get(id)
                        .is_some_and(|node| node.shape != NodeShape::HyperEdge)
            })
            .collect();
        Self {
            delta,
            tree: tree.clone(),
            embedding: embedding.clone(),
            steps: Vec::new(),
            relocations,
            upcoming: Vec::new(),
            held: BTreeSet::new(),
            replaced,
            just_before: Vec::new(),
            clear_fans: BTreeSet::new(),
            fans_last: false,
            late: Vec::new(),
            cleared: BTreeSet::new(),
            first_edge: EdgeId(embedding.next_edge),
        }
    }

    /// Before a tree node goes in under the node id `id` of a fan the
    /// delta deletes, delete the fan with a deleted tree node that names
    /// it, so that the insertion does not move it on: a leaf by a
    /// `DeleteNode`, or else the node with the fewest children by a
    /// `ContractNode`, whose arcs re-parenting them take edge ids below the
    /// current step's that are still unspent.
    fn clear_fan_id(&mut self, id: u32) -> Result<(), EditError> {
        let Some(k) = self.embedding.fans.iter().position(|f| f.node == id) else {
            return Ok(());
        };
        if self
            .delta
            .add_edges()
            .contains_key(&self.embedding.fans[k].parent_edge)
        {
            return Ok(());
        }
        let Some(fan) = self.tree.fans.get(k) else {
            return Ok(());
        };
        if self.fans_last {
            // Moved on, the fan would take the node id after the
            // insertion's; it goes now only when a fan still to be placed
            // needs that id or one below it.
            let moved_to = self.embedding.next_node.max(id.saturating_add(1));
            let moves = self.relocations.iter().filter_map(|(node, parent_edge)| {
                self.embedding
                    .fans
                    .iter()
                    .any(|f| f.parent_edge == *parent_edge && f.node != *node)
                    .then_some(*node)
            });
            let wanted = self.upcoming.first().copied().into_iter().chain(moves).min();
            if wanted.is_none_or(|w| moved_to < w) {
                return Ok(());
            }
            let name = fan.hyper_edge_id.as_str();
            if let Some(j) = self.late.iter().position(|step| {
                matches!(step, TreeEdit::DeleteFan { hyper_edge_id } if hyper_edge_id.as_ref() == name)
            }) {
                let step = self.late.remove(j);
                return self.push(step);
            }
        }
        let deleted = &self.clear_fans;
        let unspent = self.first_edge.0.saturating_sub(self.embedding.next_edge);
        let children = |n: &u32| self.tree.children_map.get(n).map_or(0, |c| c.len());
        let Some(node) = std::iter::once(fan.parent)
            .chain(fan.children.values().copied())
            .filter(|n| {
                deleted.contains(n)
                    && self.tree.nodes.contains_key(n)
                    && u32::try_from(children(n)).is_ok_and(|c| c <= unspent)
            })
            .min_by_key(|n| (children(n), *n))
        else {
            return Ok(());
        };
        let step = if children(&node) == 0 {
            TreeEdit::DeleteNode { id: node }
        } else {
            TreeEdit::ContractNode { id: node }
        };
        self.push(step)?;
        self.replaced.remove(&node);
        self.cleared.insert(node);
        Ok(())
    }

    /// Delete the old node under `id` when the delta replaces it and it is
    /// still in the tree, so that its successor can go in.
    fn delete_replaced(&mut self, id: u32) -> Result<(), EditError> {
        if self.replaced.remove(&id) {
            self.push(TreeEdit::DeleteNode { id })?;
        }
        Ok(())
    }

    /// Move the fans the delta places under node ids below `below` to
    /// those ids, for each fan that exists and is elsewhere: a scratch
    /// node under the fan's node id takes its place, which moves the fan to
    /// the next node id, and is deleted again. When the next node id is
    /// below the fan's id in the delta, a scratch node under the id below
    /// that raises it first.
    fn relocate_below(&mut self, below: u32) -> Result<(), EditError> {
        let pending = std::mem::take(&mut self.relocations);
        for (node, parent_edge) in pending {
            let current = self
                .embedding
                .fans
                .iter()
                .find(|fan| fan.parent_edge == parent_edge)
                .map(|fan| fan.node);
            match current {
                Some(current) if node < below => {
                    if current != node {
                        if node > self.embedding.next_node {
                            self.push_scratch(node - 1)?;
                        }
                        self.take_fan_id(current)?;
                    }
                }
                _ => self.relocations.push((node, parent_edge)),
            }
        }
        Ok(())
    }

    /// While edge ids below `first_edge` are unspent, move a fan the delta
    /// places under the next node id there, by a scratch node under its
    /// node id (which spends one edge id and no node id beyond it). When
    /// the step that follows inserts the tree node `inserts` under the
    /// next node id, a fan the delta places one above it moves to the next
    /// node id instead, for that insertion to move it on, and so does a fan
    /// the delta places above the next node id when the delta adds a tree
    /// node under that id later.
    fn relocate_into_gap(
        &mut self,
        first_edge: EdgeId,
        inserts: Option<u32>,
        hop_ahead: bool,
    ) -> Result<(), EditError> {
        while self.embedding.next_edge < first_edge.0 {
            let next = self.embedding.next_node;
            let hop = inserts
                .filter(|k| *k == next)
                .and_then(|k| k.checked_add(1));
            // A tree node the delta adds under the next node id, not yet
            // inserted, moves on a fan put there when it is inserted.
            let inserted_later = hop_ahead
                && !self.tree.nodes.contains_key(&next)
                && self
                    .delta
                    .add_nodes()
                    .get(&next)
                    .is_some_and(|node| node.shape != NodeShape::HyperEdge);
            let movable = self.relocations.iter().position(|(node, parent_edge)| {
                (*node == next || Some(*node) == hop || (inserted_later && *node > next))
                    && self
                        .embedding
                        .fans
                        .iter()
                        .any(|fan| fan.parent_edge == *parent_edge && fan.node != next)
            });
            let Some(k) = movable else {
                // Hopping, a fan the delta places above the next node id
                // moves up one id per scratch node taking its id, to the id
                // below the one the step's own insertion then moves it to,
                // or else to its own.
                if hop_ahead && let Some(current) = self.chain_hop(inserts) {
                    self.take_fan_id(current)?;
                    continue;
                }
                return Ok(());
            };
            let (node, parent_edge) = self.relocations[k];
            let Some(current) = self
                .embedding
                .fans
                .iter()
                .find(|fan| fan.parent_edge == parent_edge)
                .map(|fan| fan.node)
            else {
                return Ok(());
            };
            if node != next {
                // A hop: the insertion that follows finishes the move.
                return self.take_fan_id(current);
            }
            self.relocations.remove(k);
            self.take_fan_id(current)?;
        }
        Ok(())
    }

    /// The node id of the fan to move up one id towards the node id the
    /// delta gives it: the fan with the lowest such id above the next node
    /// id, while the next node id is below that id, or below the id under
    /// it when the step that follows inserts the tree node `inserts` there.
    fn chain_hop(&self, inserts: Option<u32>) -> Option<u32> {
        let next = self.embedding.next_node;
        let (target, current) = self
            .relocations
            .iter()
            .filter_map(|(node, parent_edge)| {
                let current = self
                    .embedding
                    .fans
                    .iter()
                    .find(|fan| fan.parent_edge == *parent_edge)?
                    .node;
                (current != *node && *node > next && current < next).then_some((*node, current))
            })
            .min()?;
        let end = if inserts == target.checked_sub(1) {
            target - 1
        } else {
            target
        };
        (next < end).then_some(current)
    }

    /// Put a tree node under the fan node id `id`, which moves the fan to
    /// the next node id: the node the delta adds under that id, inserted
    /// early, or else a scratch node deleted again.
    fn take_fan_id(&mut self, id: u32) -> Result<(), EditError> {
        if self
            .delta
            .add_nodes()
            .get(&id)
            .is_some_and(|node| node.shape != NodeShape::HyperEdge)
        {
            self.insert_early(id, None)?;
            if self.has_node(id) {
                return Ok(());
            }
        }
        self.push_scratch(id)
    }

    /// Whether the arc or fan `allocating` adds took the edge id the delta
    /// gives it.
    fn placed_as_read(&self, allocating: &Allocating) -> bool {
        match &allocating.step {
            TreeEdit::InsertNode { child_id: node, .. }
            | TreeEdit::MoveSubtree { node_id: node, .. } => {
                self.embedding.arcs.get(node) == Some(&allocating.first_edge)
            }
            TreeEdit::InsertFan { .. } => self
                .embedding
                .fans
                .iter()
                .any(|fan| fan.parent_edge == allocating.first_edge),
            _ => true,
        }
    }

    /// Apply `step` to the tree and follow it with the embedding. A
    /// scratch node under the id of a node `step` inserts goes first.
    fn push(&mut self, step: TreeEdit) -> Result<(), EditError> {
        if let TreeEdit::InsertNode { child_id, .. } = &step
            && self.held.remove(child_id)
        {
            self.push(TreeEdit::DeleteNode { id: *child_id })?;
        }
        self.embedding
            .apply_edit(&mut self.tree, &step)
            .map_err(|err| {
                not_tree_edit(format!("the tree edit it reads as does not apply: {err}"))
            })?;
        self.steps.push(step);
        Ok(())
    }

    /// Push an id-allocating step, after the steps that spend the ids
    /// below those it takes.
    fn push_allocating(&mut self, allocating: Allocating, choice: Choice) -> Result<(), EditError> {
        self.first_edge = allocating.first_edge;
        self.push_allocating_inner(allocating, choice)
    }

    fn push_allocating_inner(
        &mut self,
        allocating: Allocating,
        choice: Choice,
    ) -> Result<(), EditError> {
        let Choice {
            raise,
            eager,
            hop,
            early,
            stay,
        } = choice;
        let Allocating {
            first_edge,
            fan,
            step,
        } = allocating;
        // The new nodes the step names go in first.
        let named: Vec<u32> = match &step {
            TreeEdit::InsertNode { parent, .. } => vec![*parent],
            TreeEdit::MoveSubtree { new_parent, .. } => vec![*new_parent],
            TreeEdit::InsertFan { fan } => std::iter::once(fan.parent)
                .chain(fan.children.values().copied())
                .collect(),
            _ => Vec::new(),
        };
        let moved = match &step {
            TreeEdit::MoveSubtree { node_id, .. } => Some(*node_id),
            _ => None,
        };
        for id in named {
            self.insert_early(id, moved)?;
        }
        if let TreeEdit::MoveSubtree {
            node_id,
            new_parent,
            ..
        } = &step
        {
            self.clear_path(*node_id, *new_parent)?;
        }
        // The fans the delta places under ids this step raises the next
        // node id past move there first.
        let raises_past = match &step {
            TreeEdit::InsertNode { child_id, .. } => child_id.saturating_add(1),
            TreeEdit::InsertFan { .. } => fan
                .filter(|_| raise)
                .map_or(self.embedding.next_node, |(node, ceiling)| {
                    node.min(ceiling).max(self.embedding.next_node)
                })
                .saturating_add(1),
            _ => 0,
        };
        self.relocate_below(raises_past)?;
        // A new fan goes under its id in the delta when no fan inserted
        // after it has a lower one; otherwise as high as those allow, and a
        // relocation moves it on. A scratch node raises the next node id.
        if let Some((node, ceiling)) = fan {
            let target = node.min(ceiling);
            if raise
                && target > self.embedding.next_node
                && self.embedding.next_edge < first_edge.0
            {
                self.push_scratch(target - 1)?;
            }
            if !self.upcoming.is_empty() {
                self.upcoming.remove(0);
            }
        }
        let inserts = match &step {
            TreeEdit::InsertNode { child_id, .. } => Some(*child_id),
            _ => None,
        };
        if eager {
            self.relocate_into_gap(first_edge, inserts, hop)?;
        }
        // Before another step, an unspent edge id first raises the next
        // node id to the one the next new fan or pending fan move needs.
        if raise && fan.is_none() && self.embedding.next_edge < first_edge.0 {
            let next = self.embedding.next_node;
            let moves = self.relocations.iter().filter_map(|(node, parent_edge)| {
                self.embedding
                    .fans
                    .iter()
                    .any(|f| f.parent_edge == *parent_edge && f.node != *node)
                    .then_some(*node)
            });
            let wanted = self.upcoming.first().copied().into_iter().chain(moves).min();
            if let Some(wanted) = wanted
                && wanted > next
                && !matches!(&step, TreeEdit::InsertNode { child_id, .. } if *child_id >= wanted - 1)
            {
                self.push_scratch(wanted - 1)?;
                if eager {
                    self.relocate_into_gap(first_edge, inserts, hop)?;
                }
            }
        }
        // A new node already in (inserted early, or to take a fan's id)
        // moves to its place instead.
        let step = match step {
            TreeEdit::InsertNode {
                parent,
                child_id,
                edge,
                ..
            } if self.has_node(child_id) => TreeEdit::MoveSubtree {
                node_id: child_id,
                new_parent: parent,
                edge,
            },
            other => other,
        };
        if let TreeEdit::MoveSubtree {
            node_id,
            new_parent,
            ..
        } = &step
        {
            self.clear_path(*node_id, *new_parent)?;
        }
        // Spend the ids below the step's first edge id: by placing the node
        // it places under other parents first when the tree has enough of
        // them, and otherwise one id at a time by other means until it has.
        let own = match &step {
            TreeEdit::InsertNode { child_id: id, .. } | TreeEdit::MoveSubtree { node_id: id, .. } => {
                Some(*id)
            }
            _ => None,
        };
        loop {
            let gap = first_edge.0.saturating_sub(self.embedding.next_edge);
            // Inserting the delta's new nodes early comes first when chosen.
            if early
                && gap > 0
                && let Some(id) = self.pending_new_node(own)
            {
                let before = self.embedding.next_edge;
                self.insert_early(id, own)?;
                if self.embedding.next_edge > before {
                    continue;
                }
            }
            if gap > 0
                && let Some(placed) = self.placed(&step, gap)
            {
                return placed.into_iter().try_for_each(|s| self.push(s));
            }
            if gap == 1 && self.keeps_its_arc(&step) {
                // A move under the parent the node has keeps its arc when
                // that arc is the last one; a new node of the delta inserted
                // early, or a scratch node, with its arc after it makes the
                // move give it a fresh one.
                if early && let Some(id) = self.pending_new_node(own) {
                    self.insert_early(id, own)?;
                    if self.has_node(id) {
                        return self.push(step);
                    }
                }
                if let Some(id) = self.free_id() {
                    self.push_scratch_insert(id)?;
                    self.push(step)?;
                    return self.push(TreeEdit::DeleteNode { id });
                }
            }
            if gap == 0 || !self.spend_edge(early, stay, own)? {
                for before in std::mem::take(&mut self.just_before) {
                    self.push(before)?;
                }
                return self.push(step);
            }
        }
    }

    /// Whether `step` moves a node under the parent it has while its arc
    /// is above every other arc, so that the arc keeps its id.
    fn keeps_its_arc(&self, step: &TreeEdit) -> bool {
        let TreeEdit::MoveSubtree {
            node_id,
            new_parent,
            ..
        } = step
        else {
            return false;
        };
        let Some(own) = self.embedding.arcs.get(node_id) else {
            return false;
        };
        self.tree.parent_map.get(node_id) == Some(new_parent)
            && self
                .embedding
                .arcs
                .iter()
                .all(|(child, id)| child == node_id || id < own)
    }

    /// The id of a tree node the delta adds that is not in the tree yet (or
    /// whose old node, which it replaces, still is), other than `own`: the
    /// lowest that a fan holds, whose insertion moves the fan on, or else
    /// the lowest.
    fn pending_new_node(&self, own: Option<u32>) -> Option<u32> {
        let fans = fan_nodes(&self.embedding);
        self.delta
            .add_nodes()
            .values()
            .filter(|node| node.shape != NodeShape::HyperEdge)
            .map(|node| node.id)
            .filter(|id| {
                Some(*id) != own
                    && (!self.has_node(*id) || self.replaced.contains(id))
            })
            .min_by_key(|id| (!fans.contains(id), *id))
    }

    /// A node id below the next one that no node of the tree, no fan and
    /// no node the delta adds or deletes holds, the highest there is; or
    /// else the highest scratch node's id, which a scratch node can take
    /// again once that one is deleted.
    fn free_id(&self) -> Option<u32> {
        let fan_nodes = fan_nodes(&self.embedding);
        (0..self.embedding.next_node)
            .rev()
            .find(|id| {
                !self.tree.nodes.contains_key(id)
                    && !fan_nodes.contains(id)
                    && !self.delta.add_nodes().contains_key(id)
                    && !self.delta.del_nodes().contains_key(id)
            })
            .or_else(|| self.held.last().copied())
    }

    /// The steps that place the node `step` inserts or moves under
    /// `gap` other parents in turn before its own, each placement taking an
    /// edge id: an insertion goes under the first and moves through the
    /// others, a move moves through all of them. `None` when the step
    /// places no node or the tree has too few parents for it.
    fn placed(&self, step: &TreeEdit, gap: u32) -> Option<Vec<TreeEdit>> {
        let (node_id, current, target, edge, inserted) = match step {
            TreeEdit::MoveSubtree {
                node_id,
                new_parent,
                edge,
            } => (
                *node_id,
                self.tree.parent_map.get(node_id).copied(),
                *new_parent,
                edge,
                None,
            ),
            TreeEdit::InsertNode {
                parent,
                child_id,
                node,
                edge,
            } => (*child_id, None, *parent, edge, Some(node)),
            _ => return None,
        };
        let mut parents = self.placements(node_id, current, target, gap)?.into_iter();
        let mut steps = Vec::new();
        if let Some(node) = inserted {
            steps.push(TreeEdit::InsertNode {
                parent: parents.next()?,
                child_id: node_id,
                node: node.clone(),
                edge: edge.clone(),
            });
        }
        steps.extend(parents.map(|new_parent| TreeEdit::MoveSubtree {
            node_id,
            new_parent,
            edge: edge.clone(),
        }));
        Some(steps)
    }

    /// The parents under which to place `node` one after another so that
    /// the placements take `gap` edge ids before the one under `target`,
    /// which is last: `gap + 1` parents, each other than the one before it
    /// (the first other than `current`, the parent the node has now), so
    /// that each placement gives the node a fresh arc. A parent is a node of
    /// the tree outside the node's subtree. `None` when the tree has too few
    /// such nodes.
    fn placements(
        &self,
        node: u32,
        current: Option<u32>,
        target: u32,
        gap: u32,
    ) -> Option<Vec<u32>> {
        let mut candidates: Vec<u32> = self
            .tree
            .nodes
            .keys()
            .copied()
            .filter(|q| {
                *q != node && !self.held.contains(q) && !is_descendant(&self.tree, *q, node)
            })
            .collect();
        candidates.sort_unstable();
        let count = usize::try_from(gap).ok()?.checked_add(1)?;
        let mut parents = vec![target; count];
        for i in (0..count - 1).rev() {
            let next = parents[i + 1];
            let before = if i == 0 { current } else { None };
            parents[i] = candidates
                .iter()
                .copied()
                .find(|q| *q != next && Some(*q) != before)?;
        }
        if count == 1 || current == Some(parents[0]) {
            return None;
        }
        Some(parents)
    }

    /// Before a move of `node` under its own descendant `parent`, move the
    /// highest node on the path between them that the delta places
    /// elsewhere out of `node`'s subtree, under a node other than the
    /// parent the delta gives it, so that the step that places it there
    /// later is a move that gives it a fresh arc.
    fn clear_path(&mut self, node: u32, parent: u32) -> Result<(), EditError> {
        if !is_descendant(&self.tree, parent, node) {
            return Ok(());
        }
        let mut highest = None;
        let mut current = parent;
        while current != node {
            if let Some(arc) = self.final_arc(current) {
                highest = Some((current, arc));
            }
            let Some(&up) = self.tree.parent_map.get(&current) else {
                return Ok(());
            };
            current = up;
        }
        let Some((moved, (final_parent, edge))) = highest else {
            return Ok(());
        };
        let holders = self.holders();
        let outside = |q: &u32| *q != node && !is_descendant(&self.tree, *q, node);
        // The parent the delta gives the moved node is the last resort: a
        // move back under it still gives a fresh arc when arcs added after
        // this one follow it.
        let Some(holder) = std::iter::once(self.tree.root)
            .chain(holders)
            .find(|q| *q != final_parent && self.tree.nodes.contains_key(q) && outside(q))
            .or_else(|| {
                (self.tree.nodes.contains_key(&final_parent) && outside(&final_parent))
                    .then_some(final_parent)
            })
        else {
            return Ok(());
        };
        self.push(TreeEdit::MoveSubtree {
            node_id: moved,
            new_parent: holder,
            edge,
        })
    }

    /// The parent `node` has once the delta is applied: the source of the
    /// tree arc the delta adds into it, or else its parent in the tree when
    /// the delta keeps its arc.
    fn final_parent(&self, node: u32) -> Option<u32> {
        if let Some((parent, _)) = self.final_arc(node) {
            return Some(parent);
        }
        let arc = self.embedding.arcs.get(&node)?;
        if self.delta.del_edges().contains_key(arc) {
            return None;
        }
        self.tree.parent_map.get(&node).copied()
    }

    /// The tree nodes the delta places below `node`, in the tree it leaves.
    fn final_descendants(&self, node: u32) -> Vec<u32> {
        let mut nodes: Vec<u32> = self.tree.nodes.keys().copied().collect();
        nodes.sort_unstable();
        let limit = nodes.len().saturating_add(self.delta.add_nodes().len());
        nodes
            .into_iter()
            .filter(|&start| {
                let mut current = start;
                for _ in 0..limit {
                    match self.final_parent(current) {
                        Some(parent) if parent == node => return true,
                        Some(parent) => current = parent,
                        None => return false,
                    }
                }
                false
            })
            .collect()
    }

    /// The parent and schema edge of the tree arc the delta adds into
    /// `node`, when it adds one.
    fn final_arc(&self, node: u32) -> Option<(u32, Edge)> {
        self.delta
            .add_edges()
            .values()
            .find(|e| {
                e.tgt == node
                    && e.edge.kind != HYPER_PARENT_KIND
                    && e.edge.kind != HYPER_LABEL_KIND
                    && !self
                        .delta
                        .add_nodes()
                        .get(&e.src)
                        .is_some_and(|n| n.shape == NodeShape::HyperEdge)
            })
            .map(|e| (e.src, e.edge.clone()))
    }

    /// Insert the new node `id` before the step that inserts it, when a
    /// step before that one names it: under a node other than the parent
    /// the delta gives it, so that the step that places it there later is
    /// a move that gives it a fresh arc, and outside the subtree of
    /// `moved`, the node a move puts under it.
    fn insert_early(&mut self, id: u32, moved: Option<u32>) -> Result<(), EditError> {
        let Some((parent, edge)) = self.final_arc(id) else {
            return Ok(());
        };
        // A node the delta replaces under its own id goes first.
        if self.delta.add_nodes().contains_key(&id) {
            self.delete_replaced(id)?;
        }
        if self.has_node(id) {
            return Ok(());
        }
        self.clear_fan_id(id)?;
        let Some(node) = self.delta.add_nodes().get(&id).cloned() else {
            return Ok(());
        };
        let holders = self.holders();
        let root = self.tree.root;
        let below = self.final_descendants(id);
        // Outside the subtree of the node the step moves under it and of
        // every node the delta places below it, and under the parent the
        // delta gives it only as a last resort.
        let usable = |q: &u32| {
            self.tree.nodes.contains_key(q)
                && moved.is_none_or(|m| *q != m && !is_descendant(&self.tree, *q, m))
                && !below
                    .iter()
                    .any(|d| *q == *d || is_descendant(&self.tree, *q, *d))
        };
        let Some(holder) = std::iter::once(root)
            .chain(holders)
            .find(|q| *q != parent && usable(q))
            .or_else(|| usable(&parent).then_some(parent))
        else {
            return Ok(());
        };
        self.push(TreeEdit::InsertNode {
            parent: holder,
            child_id: id,
            node,
            edge,
        })
    }

    /// Insert a scratch node under the root with id `id`, deleted with
    /// the edit's last deletions, which spends one edge id and raises the
    /// next node id past `id`.
    fn push_scratch(&mut self, id: u32) -> Result<(), EditError> {
        if !self.tree.nodes.contains_key(&self.tree.root) {
            return Ok(());
        }
        self.push_scratch_insert(id)?;
        self.held.insert(id);
        Ok(())
    }

    /// Whether the tree has a node under `id` other than a scratch node.
    fn has_node(&self, id: u32) -> bool {
        self.tree.nodes.contains_key(&id) && !self.held.contains(&id)
    }

    /// Delete the scratch nodes still in the tree.
    fn delete_scratch(&mut self) -> Result<(), EditError> {
        for id in std::mem::take(&mut self.held) {
            self.push(TreeEdit::DeleteNode { id })?;
        }
        Ok(())
    }

    /// Insert a scratch node under the root with id `id`, when the tree
    /// has its root.
    fn push_scratch_insert(&mut self, id: u32) -> Result<(), EditError> {
        let root = self.tree.root;
        let Some(anchor) = self.tree.nodes.get(&root).map(|n| n.anchor.clone()) else {
            return Ok(());
        };
        self.push(TreeEdit::InsertNode {
            parent: root,
            child_id: id,
            node: Node::new(id, anchor.clone()),
            edge: Edge {
                src: anchor.clone(),
                tgt: anchor,
                kind: "prop".into(),
                name: None,
            },
        })
    }

    /// Spend one edge id without raising the next node id, by the first of
    /// these that applies: a move of a node the delta moves under the
    /// parent it has (whose own move would otherwise keep its arc) to
    /// another parent; a scratch node under a free id below the next node
    /// id; a move of a node the delta deletes to another parent. Returns
    /// whether an id was spent.
    fn spend_edge(&mut self, early: bool, stay: bool, own: Option<u32>) -> Result<bool, EditError> {
        if early && let Some(id) = self.pending_new_node(own) {
            let before = self.embedding.next_edge;
            self.insert_early(id, own)?;
            if self.embedding.next_edge > before {
                return Ok(true);
            }
        }
        let mut nodes: Vec<u32> = self.tree.nodes.keys().copied().collect();
        nodes.sort_unstable();
        let staying = nodes.iter().copied().find_map(|id| {
            let (final_parent, edge) = self.final_arc(id)?;
            let arc = self.embedding.arcs.get(&id)?;
            let unmoved = self.delta.del_edges().contains_key(arc);
            (unmoved && self.tree.parent_map.get(&id) == Some(&final_parent))
                .then_some(())?;
            let holder = self.holder(id, final_parent)?;
            Some(TreeEdit::MoveSubtree {
                node_id: id,
                new_parent: holder,
                edge,
            })
        });
        if stay && let Some(step) = staying {
            return self.push(step).map(|()| true);
        }
        if let Some(id) = self.free_id() {
            self.push_scratch(id)?;
            return Ok(true);
        }
        // A node the delta adds again under its id is doomed only while
        // its old node is in the tree.
        let doomed = self.delta.del_nodes().keys().find_map(|id| {
            if self.delta.add_nodes().contains_key(id) && !self.replaced.contains(id) {
                return None;
            }
            let parent = *self.tree.parent_map.get(id)?;
            let edge = self
                .tree
                .arcs
                .iter()
                .find(|(p, c, _)| *p == parent && c == id)
                .map(|(_, _, e)| e.clone())?;
            Some(TreeEdit::MoveSubtree {
                node_id: *id,
                new_parent: self.holder(*id, parent)?,
                edge,
            })
        });
        match doomed {
            Some(step) => self.push(step).map(|()| true),
            None => Ok(false),
        }
    }

    /// The nodes of the tree other than scratch nodes, in ascending id.
    fn holders(&self) -> Vec<u32> {
        let mut holders: Vec<u32> = self
            .tree
            .nodes
            .keys()
            .copied()
            .filter(|q| !self.held.contains(q))
            .collect();
        holders.sort_unstable();
        holders
    }

    /// A node of the tree outside `node`'s subtree other than `avoid`, the
    /// root first and then by ascending id.
    fn holder(&self, node: u32, avoid: u32) -> Option<u32> {
        let holders = self.holders();
        std::iter::once(self.tree.root).chain(holders).find(|q| {
            *q != avoid
                && *q != node
                && self.tree.nodes.contains_key(q)
                && !is_descendant(&self.tree, *q, node)
        })
    }
}

/// The steps the entry changes of a graph delta read as: a `RelabelNode`
/// for each `Anchor` change, then a `SetField` or `RemoveField` for each
/// `Field` change.
fn attr_steps(delta: &GraphDelta) -> Result<Vec<TreeEdit>, EditError> {
    let mut relabels = Vec::new();
    let mut fields = Vec::new();
    for (key, change) in delta.attrs() {
        match (key, &change.new) {
            (AttrKey::Anchor { node }, AttrVal::Anchor(anchor)) => {
                relabels.push(TreeEdit::RelabelNode {
                    id: *node,
                    new_anchor: anchor.clone(),
                });
            }
            (AttrKey::Field { node, field }, AttrVal::Field(new)) => {
                let field = Name::from(field.as_str());
                fields.push(new.as_ref().map_or_else(
                    || TreeEdit::RemoveField {
                        node_id: *node,
                        field: field.clone(),
                    },
                    |value| TreeEdit::SetField {
                        node_id: *node,
                        field: field.clone(),
                        value: value.clone(),
                    },
                ));
            }
            (AttrKey::Field { .. }, _) => {}
            (other, _) => {
                return Err(not_tree_edit(format!(
                    "a tree edit has no change of {other:?}"
                )));
            }
        }
    }
    relabels.extend(fields);
    Ok(relabels)
}

/// The fan node ids after `delta`: the fan nodes before it that it neither
/// deletes nor replaces by a tree node, and the fan nodes it adds.
fn fans_after(fans_before: &BTreeSet<u32>, delta: &GraphDelta) -> BTreeSet<u32> {
    let mut after: BTreeSet<u32> = fans_before
        .iter()
        .copied()
        .filter(|id| {
            delta.add_nodes().get(id).map_or_else(
                || !delta.del_nodes().contains_key(id),
                |node| node.shape == NodeShape::HyperEdge,
            )
        })
        .collect();
    after.extend(
        delta
            .add_nodes()
            .values()
            .filter(|node| node.shape == NodeShape::HyperEdge)
            .map(|node| node.id),
    );
    after
}

/// The tree nodes a delta deletes and adds.
struct TreeNodeChanges {
    deleted: BTreeSet<u32>,
    added: BTreeSet<u32>,
}

/// The steps the arcs a delta adds read as, `InsertNode` for an arc into a
/// new node and `MoveSubtree` for an arc into a kept node that loses its
/// own, after checking that the added arcs are appended and that every
/// re-parent is a move.
fn arc_steps(
    embedding: &TreeEmbedding,
    delta: &GraphDelta,
    fans_after: &BTreeSet<u32>,
    nodes: &TreeNodeChanges,
) -> Result<Vec<Allocating>, EditError> {
    let child_of: BTreeMap<EdgeId, u32> = embedding.arcs.iter().map(|(c, e)| (*e, *c)).collect();
    let top_arc = embedding.arcs.values().max().copied();
    let added_arcs: Vec<(EdgeId, &GEdge)> = delta
        .add_edges()
        .iter()
        .filter(|(_, edge)| !is_fan_edge(edge, fans_after))
        .map(|(id, edge)| (*id, edge))
        .collect();
    for (id, _) in &added_arcs {
        if let Some(top) = top_arc
            && *id <= top
        {
            return Err(not_tree_edit(format!(
                "the added arc {id} is not above the existing arc {top}"
            )));
        }
    }
    let lost_parent: BTreeSet<u32> = delta
        .del_edges()
        .keys()
        .filter_map(|id| child_of.get(id).copied())
        .filter(|child| !nodes.deleted.contains(child))
        .collect();
    let mut allocating = Vec::new();
    let mut gained = BTreeSet::new();
    for (id, edge) in &added_arcs {
        let child = edge.tgt;
        if !gained.insert(child) {
            return Err(not_tree_edit(format!("node {child} gains two parent arcs")));
        }
        let step = if nodes.added.contains(&child) {
            TreeEdit::InsertNode {
                parent: edge.src,
                child_id: child,
                node: delta.add_nodes()[&child].clone(),
                edge: edge.edge.clone(),
            }
        } else if lost_parent.contains(&child) {
            TreeEdit::MoveSubtree {
                node_id: child,
                new_parent: edge.src,
                edge: edge.edge.clone(),
            }
        } else {
            return Err(not_tree_edit(format!(
                "node {child} gains a parent arc without losing its own, which no move expresses"
            )));
        };
        allocating.push(Allocating {
            first_edge: *id,
            fan: None,
            step,
        });
    }
    if let Some(orphan) = lost_parent.iter().find(|c| !gained.contains(c)) {
        return Err(not_tree_edit(format!(
            "node {orphan} loses its parent arc without gaining one"
        )));
    }
    if let Some(rootless) = nodes.added.iter().find(|c| !gained.contains(c)) {
        return Err(not_tree_edit(format!(
            "the added node {rootless} has no parent arc"
        )));
    }
    Ok(allocating)
}

/// Check that every tree node a delta deletes is a leaf once the delta's
/// own arc deletions are made.
fn check_deleted_leaves(
    tree: &WInstance,
    embedding: &TreeEmbedding,
    delta: &GraphDelta,
    deleted: &BTreeSet<u32>,
) -> Result<(), EditError> {
    for id in deleted {
        let kept_child = tree
            .children_map
            .get(id)
            .into_iter()
            .flatten()
            .find(|child| {
                embedding
                    .arcs
                    .get(child)
                    .is_none_or(|arc| !delta.del_edges().contains_key(arc))
            });
        if let Some(child) = kept_child {
            return Err(not_tree_edit(format!(
                "the deleted node {id} keeps its child {child} after the delta's arc deletions"
            )));
        }
    }
    Ok(())
}

/// The fan nodes a delta adds, new or relocated, as `(node, parent edge)`.
fn relocations(delta: &GraphDelta) -> Vec<(u32, EdgeId)> {
    delta
        .add_edges()
        .iter()
        .filter(|(_, edge)| {
            edge.edge.kind == HYPER_PARENT_KIND
                && delta
                    .add_nodes()
                    .get(&edge.src)
                    .is_some_and(|node| node.shape == NodeShape::HyperEdge)
        })
        .map(|(id, edge)| (edge.src, *id))
        .collect()
}

/// The `InsertFan` steps of the fan nodes a delta adds, other than fan
/// nodes relocated to a new id (which need no step).
fn fan_insert_steps(delta: &GraphDelta) -> Result<Vec<Allocating>, EditError> {
    let mut allocating = Vec::new();
    for node in delta.add_nodes().values() {
        if node.shape != NodeShape::HyperEdge {
            continue;
        }
        let out: Vec<(EdgeId, &GEdge)> = delta
            .add_edges()
            .iter()
            .filter(|(_, edge)| edge.src == node.id)
            .map(|(id, edge)| (*id, edge))
            .collect();
        if out.iter().any(|(id, _)| delta.del_edges().contains_key(id)) {
            continue;
        }
        let mut fan = Fan::new(node.anchor.to_string(), 0);
        for (_, edge) in &out {
            if edge.edge.kind == HYPER_PARENT_KIND {
                fan.parent = edge.tgt;
            } else if let Some(label) = &edge.edge.name {
                fan.children.insert(label.to_string(), edge.tgt);
            }
        }
        let first_edge = out
            .first()
            .map(|(id, _)| *id)
            .ok_or_else(|| not_tree_edit(format!("the added fan node {} has no edges", node.id)))?;
        allocating.push(Allocating {
            first_edge,
            fan: Some((node.id, u32::MAX)),
            step: TreeEdit::InsertFan { fan },
        });
    }
    Ok(allocating)
}

/// The `DeleteFan` steps of the fans a delta deletes: one for each name
/// whose fans the delta all deletes, and one for each other deleted fan
/// that no deleted tree node names (those go with the node). Each comes
/// with whether every fan it deletes is named by a deleted tree node: such
/// a fan may also go with that node, later, after tree nodes taking its
/// node id have moved it on.
fn fan_delete_steps(
    tree: &WInstance,
    embedding: &TreeEmbedding,
    delta: &GraphDelta,
    deleted: &BTreeSet<u32>,
) -> Vec<(TreeEdit, bool)> {
    let gone: Vec<bool> = embedding
        .fans
        .iter()
        .map(|fan| {
            delta.del_nodes().contains_key(&fan.node)
                && !delta.add_edges().contains_key(&fan.parent_edge)
        })
        .collect();
    let all_gone = |name: &str| {
        tree.fans
            .iter()
            .zip(&gone)
            .all(|(f, g)| f.hyper_edge_id != name || *g)
    };
    let cascaded = |f: &Fan| {
        deleted.contains(&f.parent) || f.children.values().any(|c| deleted.contains(c))
    };
    let mut steps = Vec::new();
    let mut deleted_fans = BTreeSet::new();
    for (f, is_gone) in tree.fans.iter().zip(&gone) {
        if *is_gone
            && (!cascaded(f) || all_gone(&f.hyper_edge_id))
            && deleted_fans.insert(f.hyper_edge_id.clone())
        {
            let all_cascaded = tree
                .fans
                .iter()
                .all(|g| g.hyper_edge_id != f.hyper_edge_id || cascaded(g));
            steps.push((
                TreeEdit::DeleteFan {
                    hyper_edge_id: Name::from(f.hyper_edge_id.as_str()),
                },
                all_cascaded,
            ));
        }
    }
    steps
}

impl TreeEdit {
    /// The tree edit of a graph delta, when there is one: the edit `e` that
    /// [`GraphDelta::from_tree_edit`] lowers to `delta` from `g` and
    /// `embedding`, where `g` is the graph form of `tree` under `embedding`
    /// (its fan parent edges unnamed).
    ///
    /// The edit is built from the delta: `Anchor` changes become
    /// `RelabelNode`, `Field` changes `SetField` or `RemoveField`, an added
    /// arc into a new node `InsertNode` and into a kept node `MoveSubtree`, a
    /// deleted node `DeleteNode` (a node deleted and added again under its
    /// id is deleted before it goes back in), a new fan `InsertFan`, and a
    /// deleted fan `DeleteFan` when no deleted node names it or the delta
    /// deletes every fan of its name. The steps run in that order, the
    /// arc-adding and fan-adding steps by ascending first new edge id (the
    /// order in which their ids are allocated) and the deletions children
    /// first; a deleted fan, and a deleted leaf no added edge uses, may go
    /// before the arc-adding steps or after them (the orders are tried in
    /// turn). Ids the delta skips (those an edit took for arcs, nodes or
    /// fans that a later step of it removed again, or for a fan that a tree
    /// node taking its id moved on) are spent by steps that leave the delta
    /// unchanged: placements of a node under other parents before its own,
    /// scratch nodes inserted under the root and deleted again, and moves
    /// of nodes the delta moves or deletes; a fan reaches the node id the
    /// delta gives it through a tree node taking its id. The edit is
    /// returned only when it lowers back to `delta` exactly, so the
    /// embedding it leaves has the arcs and fans of the one any edit
    /// lowering to `delta` leaves (its id counters may differ, since the
    /// delta does not record the ids an edit spends after its last new arc
    /// or fan).
    ///
    /// # Errors
    ///
    /// Returns [`EditError::NotTreeEdit`] when the delta has no tree edit:
    /// when it changes an edge label, an edge position, a node's value or a
    /// node's metadata (a tree edit changes none of them); when an added
    /// arc's id is not above every existing arc id (tree edits append arcs);
    /// when a deleted node is not a leaf once the delta's own arc deletions
    /// are made; when a node loses its parent arc without gaining one, gains
    /// a parent arc without losing its old one, or is added without one (a
    /// re-parent a `MoveSubtree` does not express); and when the edit built
    /// from the delta does not lower back to it (a fan whose node id no
    /// spending of ids reaches, or payloads a tree edit does not produce).
    /// Returns [`EditError::ApplyFailed`] when `tree` has no graph form
    /// under `embedding`.
    pub fn from_graph_delta(
        tree: &WInstance,
        embedding: &TreeEmbedding,
        delta: &GraphDelta,
    ) -> Result<Self, EditError> {
        let graph =
            GInstance::from_wtype_under(tree, embedding, None).map_err(|e| inst_failure(&e))?;
        let attrs = attr_steps(delta)?;

        let fans_before = fan_nodes(embedding);
        let fans_after = fans_after(&fans_before, delta);
        let nodes = TreeNodeChanges {
            deleted: delta
                .del_nodes()
                .keys()
                .copied()
                .filter(|id| !fans_before.contains(id))
                .collect(),
            added: delta
                .add_nodes()
                .values()
                .filter(|node| node.shape != NodeShape::HyperEdge)
                .map(|node| node.id)
                .collect(),
        };
        let mut allocating = arc_steps(embedding, delta, &fans_after, &nodes)?;
        check_deleted_leaves(tree, embedding, delta, &nodes.deleted)?;
        allocating.extend(fan_insert_steps(delta)?);
        let fan_deletes = fan_delete_steps(tree, embedding, delta, &nodes.deleted);

        allocating.sort_by_key(|a| a.first_edge);
        let mut lowest_later = u32::MAX;
        for allocating in allocating.iter_mut().rev() {
            if let Some((node, ceiling)) = &mut allocating.fan {
                *ceiling = lowest_later.saturating_sub(1);
                lowest_later = lowest_later.min(*node);
            }
        }
        let read = ReadSteps {
            attrs,
            fan_deletes,
            allocating,
            deleted: nodes.deleted,
        };
        // A fan the delta deletes goes before the steps that add or after
        // them, and so does a tree node it deletes that is a leaf those
        // steps do not use, with the fans that name it: a tree node that
        // takes a fan's node id while the fan lives moves the fan to the
        // next node id, which the delta's later fans may or may not have
        // seen spent. The orders are tried in turn.
        let first = read.lower_checked(&graph, tree, embedding, delta, Order::FanDeletesFirst);
        let Err(err) = first else {
            return first;
        };
        for order in [Order::DeletesFirst, Order::DeletesLast] {
            if let Ok(edit) = read.lower_checked(&graph, tree, embedding, delta, order) {
                return Ok(edit);
            }
        }
        Err(err)
    }
}

/// Delete each node of `deletions` that is now a leaf of the replayed
/// tree, other than its root, and that no added edge of the delta uses.
fn delete_unused_leaves(
    replay: &mut Replay<'_>,
    deletions: &mut Vec<u32>,
    used: &BTreeSet<u32>,
) -> Result<(), EditError> {
    deletions.retain(|id| !replay.cleared.contains(id));
    while let Some(k) = deletions.iter().position(|id| {
        !used.contains(id)
            && *id != replay.tree.root
            && replay
                .tree
                .children_map
                .get(id)
                .is_none_or(|c| c.is_empty())
    }) {
        let id = deletions.remove(k);
        replay.push(TreeEdit::DeleteNode { id })?;
    }
    Ok(())
}

/// Where the deletions of the tree edit a graph delta reads as go
/// relative to its id-allocating steps.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Order {
    /// Fan deletions first, tree node deletions last.
    FanDeletesFirst,
    /// Fan deletions and the deletions of unused leaves first.
    DeletesFirst,
    /// Every deletion last, except that a fan deletion goes before the
    /// first new fan of its name.
    DeletesLast,
}

/// The parts of the tree edit a graph delta reads as, before they are
/// put in order.
struct ReadSteps {
    attrs: Vec<TreeEdit>,
    fan_deletes: Vec<(TreeEdit, bool)>,
    allocating: Vec<Allocating>,
    deleted: BTreeSet<u32>,
}

impl ReadSteps {
    /// The tree edit these parts make, with the deletions placed as
    /// `order` says, provided it lowers to `delta`.
    fn lower_checked(
        &self,
        graph: &GInstance,
        tree: &WInstance,
        embedding: &TreeEmbedding,
        delta: &GraphDelta,
        order: Order,
    ) -> Result<TreeEdit, EditError> {
        let mut replay = Replay::new(tree, embedding, delta, relocations(delta));
        replay.upcoming = self
            .allocating
            .iter()
            .filter_map(|a| a.fan.map(|(node, ceiling)| node.min(ceiling)))
            .collect();
        replay.clear_fans = self.deleted.clone();
        if order == Order::DeletesLast {
            replay.fans_last = true;
            // A fan that goes with a deleted node goes with it, last.
            replay.late = self
                .fan_deletes
                .iter()
                .filter(|(_, cascaded)| !cascaded)
                .map(|(step, _)| step.clone())
                .collect();
        }
        for step in &self.attrs {
            replay.push(step.clone())?;
        }
        if order != Order::DeletesLast {
            for (step, _) in &self.fan_deletes {
                replay.push(step.clone())?;
            }
        }
        // A tree node replaced under its own id is deleted before the step
        // that inserts its successor.
        let mut deletions: Vec<u32> = self.deleted.iter().copied().collect();
        deletions.sort_by_key(|id| (std::cmp::Reverse(depth(tree, *id)), *id));
        let used: BTreeSet<u32> = delta
            .add_edges()
            .values()
            .flat_map(|e| [e.src, e.tgt])
            .collect();
        if order == Order::DeletesFirst {
            delete_unused_leaves(&mut replay, &mut deletions, &used)?;
        }
        let search = Search {
            read: self,
            order,
            used,
            graph,
            embedding,
            delta,
            budget: std::cell::Cell::new(SEARCH_BUDGET),
        };
        search
            .run(
                SearchState { replay, deletions },
                0,
            )
            .ok_or_else(|| {
                not_tree_edit(
                    "no tree edit lowers to it: its ids or payloads are not those tree edits produce"
                        .to_owned(),
                )
            })
    }
}

/// How an id-allocating step spends the ids below its own.
#[derive(Clone, Copy)]
struct Choice {
    /// Raise the next node id first: to the node id the delta gives a new
    /// fan the step inserts, or, before another step, to the one the next
    /// new fan or pending fan move needs.
    raise: bool,
    /// Move fans to the node ids the delta gives them as soon as the next
    /// node id reaches those ids.
    eager: bool,
    /// Moving fans eagerly, also move a fan to the next node id when a
    /// tree node the delta adds later takes that id and moves it on.
    hop: bool,
    /// Spend edge ids by inserting the tree nodes the delta adds early,
    /// before other means.
    early: bool,
    /// Spend edge ids first by moving away a node the delta moves under
    /// the parent it has.
    stay: bool,
}

/// The choices the search tries for each step, in order.
const CHOICES: [Choice; 24] = [
    Choice {
        raise: true,
        eager: true,
        hop: false,
        early: false,
        stay: true,
    },
    Choice {
        raise: false,
        eager: true,
        hop: false,
        early: false,
        stay: true,
    },
    Choice {
        raise: true,
        eager: true,
        hop: true,
        early: false,
        stay: true,
    },
    Choice {
        raise: false,
        eager: true,
        hop: true,
        early: false,
        stay: true,
    },
    Choice {
        raise: true,
        eager: false,
        hop: false,
        early: false,
        stay: true,
    },
    Choice {
        raise: false,
        eager: false,
        hop: false,
        early: false,
        stay: true,
    },
    Choice {
        raise: true,
        eager: true,
        hop: false,
        early: true,
        stay: true,
    },
    Choice {
        raise: false,
        eager: true,
        hop: false,
        early: true,
        stay: true,
    },
    Choice {
        raise: true,
        eager: true,
        hop: true,
        early: true,
        stay: true,
    },
    Choice {
        raise: false,
        eager: true,
        hop: true,
        early: true,
        stay: true,
    },
    Choice {
        raise: true,
        eager: false,
        hop: false,
        early: true,
        stay: true,
    },
    Choice {
        raise: false,
        eager: false,
        hop: false,
        early: true,
        stay: true,
    },
    Choice {
        raise: true,
        eager: true,
        hop: false,
        early: false,
        stay: false,
    },
    Choice {
        raise: false,
        eager: true,
        hop: false,
        early: false,
        stay: false,
    },
    Choice {
        raise: true,
        eager: true,
        hop: true,
        early: false,
        stay: false,
    },
    Choice {
        raise: false,
        eager: true,
        hop: true,
        early: false,
        stay: false,
    },
    Choice {
        raise: true,
        eager: false,
        hop: false,
        early: false,
        stay: false,
    },
    Choice {
        raise: false,
        eager: false,
        hop: false,
        early: false,
        stay: false,
    },
    Choice {
        raise: true,
        eager: true,
        hop: false,
        early: true,
        stay: false,
    },
    Choice {
        raise: false,
        eager: true,
        hop: false,
        early: true,
        stay: false,
    },
    Choice {
        raise: true,
        eager: true,
        hop: true,
        early: true,
        stay: false,
    },
    Choice {
        raise: false,
        eager: true,
        hop: true,
        early: true,
        stay: false,
    },
    Choice {
        raise: true,
        eager: false,
        hop: false,
        early: true,
        stay: false,
    },
    Choice {
        raise: false,
        eager: false,
        hop: false,
        early: true,
        stay: false,
    },
];

/// How many partial edits the search for the tree edit of a graph delta
/// builds before it gives up.
const SEARCH_BUDGET: usize = 4096;

/// The search for the tree edit of a graph delta: the id-allocating steps
/// in turn, each made with each [`Choice`] of how to spend the ids below
/// its own, keeping only the partial edits whose arcs and fans take the
/// edge ids the delta gives them.
struct Search<'s, 'a> {
    read: &'s ReadSteps,
    order: Order,
    used: BTreeSet<u32>,
    graph: &'s GInstance,
    embedding: &'s TreeEmbedding,
    delta: &'a GraphDelta,
    budget: std::cell::Cell<usize>,
}

/// A partial edit of the search: the replay so far and the node
/// deletions still to make.
#[derive(Clone)]
struct SearchState<'a> {
    replay: Replay<'a>,
    deletions: Vec<u32>,
}

impl<'a> Search<'_, 'a> {
    /// The first complete edit that lowers to the delta, from `state` with
    /// the id-allocating steps from `index` on still to make.
    fn run(&self, state: SearchState<'a>, index: usize) -> Option<TreeEdit> {
        let Some(allocating) = self.read.allocating.get(index) else {
            return self.finish(state);
        };
        // Choices that make the same steps lead to the same search below.
        let mut tried = Vec::new();
        for choice in CHOICES {
            let left = self.budget.get().checked_sub(1)?;
            self.budget.set(left);
            let mut next = state.clone();
            if self.advance(&mut next, allocating, choice).is_err()
                || !next.replay.placed_as_read(allocating)
            {
                continue;
            }
            let made = TreeEdit::Sequence(next.replay.steps[state.replay.steps.len()..].to_vec())
                .digest();
            if tried.contains(&made) {
                continue;
            }
            tried.push(made);
            if let Some(edit) = self.run(next, index + 1) {
                return Some(edit);
            }
        }
        None
    }

    /// Make one id-allocating step, with the deletions that go before it
    /// and, deleting unused leaves first, those that go after it.
    fn advance(
        &self,
        state: &mut SearchState<'a>,
        allocating: &Allocating,
        choice: Choice,
    ) -> Result<(), EditError> {
        // A late fan deletion still goes before a new fan of its name,
        // which it would delete too.
        if let TreeEdit::InsertFan { fan } = &allocating.step {
            let (now, later): (Vec<TreeEdit>, Vec<TreeEdit>) =
                std::mem::take(&mut state.replay.late).into_iter().partition(|step| {
                    matches!(step, TreeEdit::DeleteFan { hyper_edge_id }
                        if hyper_edge_id.as_ref() == fan.hyper_edge_id.as_str())
                });
            state.replay.late = later;
            // Once the ids below the new fan's are spent: until then the old
            // fans may still be moved on by tree nodes taking their ids.
            state.replay.just_before = now;
        }
        if let TreeEdit::InsertNode { child_id, .. } = &allocating.step {
            if self.read.deleted.contains(child_id) {
                let id = *child_id;
                state.deletions.retain(|d| *d != id);
                state.replay.delete_replaced(id)?;
            }
            state.replay.first_edge = allocating.first_edge;
            state.replay.clear_fan_id(*child_id)?;
        }
        state.replay.push_allocating(allocating.clone(), choice)?;
        if self.order == Order::DeletesFirst {
            delete_unused_leaves(&mut state.replay, &mut state.deletions, &self.used)?;
        }
        Ok(())
    }

    /// The complete edit of `state`, when it lowers to the delta.
    fn finish(&self, state: SearchState<'a>) -> Option<TreeEdit> {
        let SearchState {
            mut replay,
            deletions,
        } = state;
        for step in std::mem::take(&mut replay.late) {
            replay.push(step).ok()?;
        }
        replay.relocate_below(u32::MAX).ok()?;
        replay.delete_scratch().ok()?;
        for id in deletions {
            if !replay.cleared.contains(&id) {
                replay.push(TreeEdit::DeleteNode { id }).ok()?;
            }
        }
        let mut steps = replay.steps;
        let edit = match steps.len() {
            0 => TreeEdit::Identity,
            1 => steps.remove(0),
            _ => TreeEdit::Sequence(steps),
        };
        let mut followed = self.embedding.clone();
        let lowered = GraphDelta::from_tree_edit(self.graph, &mut followed, &edit).ok()?;
        (lowered == *self.delta).then_some(edit)
    }
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

    use super::{EditError, is_descendant};

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
    fn move_subtree_refuses_to_make_a_node_its_own_parent() {
        // `is_descendant` walks up from `new_parent`, so it never
        // considers the node itself and reported no cycle here. The move
        // then wrote `parent_map[1] = 1`, and every later ancestor walk
        // through node 1 ran forever.
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(1, Node::new(1, "a"));
        let arcs = vec![(0, 1, sample_edge("root", "a"))];
        let mut inst = WInstance::new(nodes, arcs, vec![], 0, Name::from("root"));

        let edit = TreeEdit::MoveSubtree {
            node_id: 1,
            new_parent: 1,
            edge: sample_edge("a", "a"),
        };

        assert!(matches!(
            edit.apply(&mut inst),
            Err(EditError::CycleDetected {
                node_id: 1,
                new_parent: 1
            })
        ));
        assert_eq!(inst.parent_map[&1], 0, "the instance is left as it was");
    }

    #[test]
    fn ancestor_walk_terminates_on_a_cyclic_parent_map() {
        // A `WInstance` can arrive already carrying a cycle: it is
        // deserialized, and hosts hand one across the FFI boundary. The
        // walk is bounded by the node count so such an instance answers
        // rather than hanging.
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(1, Node::new(1, "a"));
        nodes.insert(2, Node::new(2, "b"));
        let arcs = vec![
            (0, 1, sample_edge("root", "a")),
            (1, 2, sample_edge("a", "b")),
        ];
        let mut inst = WInstance::new(nodes, arcs, vec![], 0, Name::from("root"));

        // Close the chain by hand: 1 -> 2 -> 1.
        inst.parent_map.insert(1, 2);

        assert!(!is_descendant(&inst, 2, 99));
        assert!(is_descendant(&inst, 2, 1));
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

    /// r(0) -> a(1) -> c(3) and r -> a(2): joining node 1 into node 2 hands
    /// node 1's child 3 to node 2, keeping its edge with the source set to
    /// node 2's anchor, so the result is still a tree.
    #[test]
    fn joining_features_reparents_the_children_of_joined_nodes() {
        let nodes = HashMap::from([
            (0, Node::new(0, "r")),
            (1, Node::new(1, "a")),
            (2, Node::new(2, "a")),
            (3, Node::new(3, "c")),
        ]);
        let arcs = vec![
            (0, 1, sample_edge("r", "a")),
            (1, 3, sample_edge("a", "c")),
            (0, 2, sample_edge("r", "a")),
        ];
        let mut inst = WInstance::new(nodes, arcs, vec![], 0, Name::from("r"));
        TreeEdit::JoinFeatures {
            primary: 2,
            joined: vec![1],
            produce: Node::new(2, "a"),
        }
        .apply(&mut inst)
        .unwrap();

        assert_eq!(inst.parent_map.get(&3), Some(&2));
        assert!(!inst.parent_map.contains_key(&1));
        assert_eq!(inst.children_map[&2].as_slice(), &[3]);
        assert_eq!(
            inst.arcs,
            vec![(0, 2, sample_edge("r", "a")), (2, 3, sample_edge("a", "c")),]
        );
        let graph = crate::GInstance::from_wtype(&inst).unwrap();
        let back = graph.to_wtype(0).unwrap();
        assert_eq!(back.arcs, inst.arcs);
        assert_eq!(back.parent_map, inst.parent_map);
    }

    /// The product of a join replaces the primary node, so it must carry
    /// the primary's id; anything else is refused and changes nothing.
    #[test]
    fn joining_features_rejects_a_foreign_product() {
        let mut inst = two_node_instance();
        inst.nodes.insert(2, Node::new(2, "child"));
        inst.arcs.push((0, 2, sample_edge("root", "child")));
        let mut inst = WInstance::new(
            inst.nodes.clone(),
            inst.arcs.clone(),
            vec![],
            0,
            Name::from("root"),
        );
        let before = inst.clone();
        let err = TreeEdit::JoinFeatures {
            primary: 1,
            joined: vec![2],
            produce: Node::new(7, "child"),
        }
        .apply(&mut inst)
        .unwrap_err();
        assert!(matches!(err, EditError::ApplyFailed(_)), "{err:?}");
        assert_eq!(inst.nodes.len(), before.nodes.len());
        assert_eq!(inst.arcs, before.arcs);
        assert!(!inst.nodes.contains_key(&7));
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

#[cfg(test)]
mod graph_tests {
    use std::collections::{BTreeMap, HashMap};
    use std::error::Error;

    use panproto_gat::Name;
    use panproto_schema::{CanonicalWriter, Edge};
    use proptest::prelude::*;

    use super::{TreeEdit, canonical_tree_edit_bytes};
    use crate::edit_error::EditError;
    use crate::fan::Fan;
    use crate::ginstance::{EdgeId, GEdge, GInstance, TreeEmbedding};
    use crate::graph_edit::{AttrChange, AttrKey, AttrVal, GraphDelta};
    use crate::metadata::Node;
    use crate::value::Value;
    use crate::wtype::WInstance;

    type TestResult = Result<(), Box<dyn Error>>;

    /// A proptest failure carrying `err`.
    fn fail(err: impl std::fmt::Debug) -> TestCaseError {
        TestCaseError::fail(format!("{err:?}"))
    }

    fn edge(src: &str, tgt: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "prop".into(),
            name: None,
        }
    }

    /// s(0) with words 1 and 4 and a fan `rel` over them; arcs 0 -> 4 (edge
    /// 0) and 0 -> 1 (edge 1); the fan is node 5 with edges 2 (parent), 3
    /// (agent) and 4 (theme).
    fn sentence() -> WInstance {
        let nodes = HashMap::from([
            (0, Node::new(0, "s")),
            (1, Node::new(1, "w")),
            (4, Node::new(4, "w")),
        ]);
        let arcs = vec![(0, 4, edge("s", "w")), (0, 1, edge("s", "w"))];
        let fan = Fan::new("rel", 0)
            .with_child("theme", 4)
            .with_child("agent", 1);
        WInstance::new(nodes, arcs, vec![fan], 0, "s".into())
    }

    /// The graph delta of `edit` on `tree`, the embedding it leaves, and the
    /// graph it reaches, checked against the graph form of the edited tree.
    fn lower(
        tree: &WInstance,
        edit: &TreeEdit,
    ) -> Result<(GraphDelta, TreeEmbedding, GInstance), Box<dyn Error>> {
        let (graph, mut embedding) = GInstance::from_wtype_embedded(tree, None)?;
        let mut followed = embedding.clone();
        let mut edited = tree.clone();
        followed.apply_edit(&mut edited, edit)?;
        let delta = GraphDelta::from_tree_edit(&graph, &mut embedding, edit)?;
        assert_eq!(embedding, followed);
        let mut reached = graph;
        delta.apply(&mut reached)?;
        let expected = GInstance::from_wtype_under(&edited, &followed, None)?;
        assert!(GInstance::identical(&reached, &expected));
        Ok((delta, embedding, reached))
    }

    fn ids<K: Copy, V>(map: &BTreeMap<K, V>) -> Vec<K> {
        map.keys().copied().collect()
    }

    #[test]
    fn from_tree_edit_cases() -> TestResult {
        let tree = sentence();
        insert_and_delete_cases(&tree)?;
        contract_case()?;
        relabel_and_field_cases(&tree)?;
        move_and_fan_cases(&tree)?;
        sequence_case(&tree)
    }

    fn insert_and_delete_cases(tree: &WInstance) -> TestResult {
        // InsertNode: the node and an arc under the next edge id.
        let (d, emb, _) = lower(
            tree,
            &TreeEdit::InsertNode {
                parent: 1,
                child_id: 6,
                node: Node::new(6, "m"),
                edge: edge("w", "m"),
            },
        )?;
        assert_eq!(ids(d.add_nodes()), vec![6]);
        assert_eq!(d.add_edges()[&EdgeId(5)], GEdge::new(1, 6, edge("w", "m")));
        assert!(d.del_nodes().is_empty() && d.del_edges().is_empty() && d.attrs().is_empty());
        assert_eq!(emb.arcs[&6], EdgeId(5));

        // DeleteNode: the node, its in-arc and the fan naming it.
        let (d, emb, _) = lower(tree, &TreeEdit::DeleteNode { id: 4 })?;
        assert_eq!(ids(d.del_nodes()), vec![4, 5]);
        assert_eq!(
            ids(d.del_edges()),
            vec![EdgeId(0), EdgeId(2), EdgeId(3), EdgeId(4)]
        );
        assert!(d.add_nodes().is_empty() && d.add_edges().is_empty());
        assert!(emb.fans.is_empty());
        Ok(())
    }

    fn contract_case() -> TestResult {
        // ContractNode: node 1 of r(0) -> a(1) -> {b(2), c(3)}.
        let chain = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "r")),
                (1, Node::new(1, "a")),
                (2, Node::new(2, "b")),
                (3, Node::new(3, "c")),
            ]),
            vec![
                (0, 1, edge("r", "a")),
                (1, 2, edge("a", "b")),
                (1, 3, edge("a", "c")),
            ],
            vec![],
            0,
            "r".into(),
        );
        let (d, _, _) = lower(&chain, &TreeEdit::ContractNode { id: 1 })?;
        assert_eq!(ids(d.del_nodes()), vec![1]);
        assert_eq!(ids(d.del_edges()), vec![EdgeId(0), EdgeId(1), EdgeId(2)]);
        assert_eq!(d.add_edges()[&EdgeId(3)], GEdge::new(0, 2, edge("r", "b")));
        assert_eq!(d.add_edges()[&EdgeId(4)], GEdge::new(0, 3, edge("r", "c")));
        Ok(())
    }

    fn relabel_and_field_cases(tree: &WInstance) -> TestResult {
        // RelabelNode: an `Anchor` change, and the label change of the fan
        // edge reaching the node (it names its target's anchor).
        let (d, _, _) = lower(
            tree,
            &TreeEdit::RelabelNode {
                id: 1,
                new_anchor: "v".into(),
            },
        )?;
        assert_eq!(
            d.attrs()[&AttrKey::Anchor { node: 1 }],
            AttrChange {
                old: AttrVal::Anchor("w".into()),
                new: AttrVal::Anchor("v".into())
            }
        );
        assert!(
            d.attrs()
                .contains_key(&AttrKey::EdgeLabel { edge: EdgeId(3) })
        );
        assert_eq!(d.attrs().len(), 2);

        // SetField and RemoveField: `Field` changes with the old value read
        // from the state.
        let set = TreeEdit::SetField {
            node_id: 4,
            field: "n".into(),
            value: Value::Int(2),
        };
        let (d, _, _) = lower(tree, &set)?;
        assert_eq!(
            d.attrs()[&AttrKey::Field {
                node: 4,
                field: "n".into()
            }],
            AttrChange {
                old: AttrVal::Field(None),
                new: AttrVal::Field(Some(Value::Int(2)))
            }
        );
        let mut with_field = tree.clone();
        set.apply(&mut with_field)?;
        let (d, _, _) = lower(
            &with_field,
            &TreeEdit::RemoveField {
                node_id: 4,
                field: "n".into(),
            },
        )?;
        assert_eq!(
            d.attrs()[&AttrKey::Field {
                node: 4,
                field: "n".into()
            }],
            AttrChange {
                old: AttrVal::Field(Some(Value::Int(2))),
                new: AttrVal::Field(None)
            }
        );
        Ok(())
    }

    fn move_and_fan_cases(tree: &WInstance) -> TestResult {
        // MoveSubtree: the old arc deleted, the new one under a fresh id.
        let (d, _, _) = lower(
            tree,
            &TreeEdit::MoveSubtree {
                node_id: 4,
                new_parent: 1,
                edge: edge("w", "w"),
            },
        )?;
        assert_eq!(ids(d.del_edges()), vec![EdgeId(0)]);
        assert_eq!(d.add_edges()[&EdgeId(5)], GEdge::new(1, 4, edge("w", "w")));
        assert_eq!(d.add_edges().len(), 1);

        // InsertFan and DeleteFan: the fan node with its edges.
        let (d, _, _) = lower(
            tree,
            &TreeEdit::InsertFan {
                fan: Fan::new("arg", 1).with_child("x", 4),
            },
        )?;
        assert_eq!(ids(d.add_nodes()), vec![6]);
        assert_eq!(ids(d.add_edges()), vec![EdgeId(5), EdgeId(6)]);
        let (d, _, _) = lower(
            tree,
            &TreeEdit::DeleteFan {
                hyper_edge_id: "rel".into(),
            },
        )?;
        assert_eq!(ids(d.del_nodes()), vec![5]);
        assert_eq!(ids(d.del_edges()), vec![EdgeId(2), EdgeId(3), EdgeId(4)]);
        Ok(())
    }

    fn sequence_case(tree: &WInstance) -> TestResult {
        // Sequence: the composite of the steps' deltas.
        let insert = TreeEdit::InsertNode {
            parent: 0,
            child_id: 9,
            node: Node::new(9, "w"),
            edge: edge("s", "w"),
        };
        let seq = TreeEdit::Sequence(vec![insert.clone(), TreeEdit::DeleteNode { id: 9 }]);
        let (d, emb, _) = lower(tree, &seq)?;
        assert!(d.is_identity(), "an add then a delete of one node cancels");
        assert_eq!(emb.next_edge, 6, "the cancelled arc still took an id");
        let (first, mid, _) = lower(tree, &insert)?;
        let mut mid_tree = tree.clone();
        insert.apply(&mut mid_tree)?;
        let mid_graph = GInstance::from_wtype_under(&mid_tree, &mid, None)?;
        let mut follow = mid;
        let second =
            GraphDelta::from_tree_edit(&mid_graph, &mut follow, &TreeEdit::DeleteNode { id: 9 })
                ?;
        assert_eq!(first.compose(&second)?, d);
        Ok(())
    }

    #[test]
    fn from_tree_edit_joins_features_like_tree_apply() -> TestResult {
        // r(0) -> a(1) -> c(3), r -> a(2): join 1 into 2 with a product
        // that renames the primary and sets a field.
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "r")),
                (1, Node::new(1, "a")),
                (2, Node::new(2, "a")),
                (3, Node::new(3, "c")),
            ]),
            vec![
                (0, 1, edge("r", "a")),
                (1, 3, edge("a", "c")),
                (0, 2, edge("r", "a")),
            ],
            vec![],
            0,
            "r".into(),
        );
        let mut produce = Node::new(2, "b");
        produce.extra_fields.insert("k".into(), Value::Int(1));
        let (d, emb, graph) = lower(
            &tree,
            &TreeEdit::JoinFeatures {
                primary: 2,
                joined: vec![1],
                produce,
            },
        )?;
        assert_eq!(ids(d.del_nodes()), vec![1]);
        assert_eq!(ids(d.del_edges()), vec![EdgeId(0), EdgeId(1)]);
        // Node 3 keeps its edge, with the source set to the product's anchor,
        // under the next edge id.
        assert_eq!(d.add_edges()[&EdgeId(3)], GEdge::new(2, 3, edge("b", "c")));
        assert_eq!(emb.arcs[&3], EdgeId(3));
        assert_eq!(
            d.attrs().keys().cloned().collect::<Vec<_>>(),
            vec![
                AttrKey::Field {
                    node: 2,
                    field: "k".into()
                },
                AttrKey::Anchor { node: 2 },
            ]
        );
        assert!(graph.to_wtype(0).is_ok(), "the result is a tree");
        Ok(())
    }

    #[test]
    fn from_tree_edit_allocates_ids_in_apply_order() -> TestResult {
        let tree = sentence();
        let edit = TreeEdit::Sequence(vec![
            TreeEdit::InsertFan {
                fan: Fan::new("arg", 0).with_child("y", 1).with_child("x", 4),
            },
            TreeEdit::MoveSubtree {
                node_id: 1,
                new_parent: 4,
                edge: edge("w", "w"),
            },
            TreeEdit::InsertNode {
                parent: 4,
                child_id: 6,
                node: Node::new(6, "w"),
                edge: edge("w", "w"),
            },
        ]);
        let (d, emb, _) = lower(&tree, &edit)?;
        // next_node was 6 and next_edge 5: the new fan takes node 6 and edges
        // 5 (parent), 6 (x) and 7 (y); the move takes 8; the inserted node
        // takes id 6 from the fan, which moves to node 7, and its arc takes 9.
        assert_eq!(emb.fans[1].parent_edge, EdgeId(5));
        assert_eq!(
            emb.fans[1].child_edges,
            BTreeMap::from([("x".to_owned(), EdgeId(6)), ("y".to_owned(), EdgeId(7))])
        );
        assert_eq!(emb.arcs[&1], EdgeId(8));
        assert_eq!(emb.arcs[&6], EdgeId(9));
        assert_eq!(emb.fans[1].node, 7);
        assert_eq!((emb.next_node, emb.next_edge), (8, 10));
        assert_eq!(
            ids(d.add_edges()),
            vec![EdgeId(5), EdgeId(6), EdgeId(7), EdgeId(8), EdgeId(9)]
        );
        assert_eq!(ids(d.add_nodes()), vec![6, 7]);
        Ok(())
    }

    #[test]
    fn from_graph_delta_rejections() -> TestResult {
        let tree = sentence();
        let (graph, emb) = GInstance::from_wtype_embedded(&tree, None)?;
        let reject = |delta: &GraphDelta, needle: &str| {
            let result = TreeEdit::from_graph_delta(&tree, &emb, delta);
            let Err(EditError::NotTreeEdit { reason }) = &result else {
                panic!("expected NotTreeEdit, got {result:?}");
            };
            assert!(
                reason.contains(needle),
                "{reason:?} does not say {needle:?}"
            );
        };
        // An added arc below an existing arc id: tree edits append.
        let low = GraphDelta::identity()
            .with_added_node(Node::new(6, "w"))
            .with_added_edge(EdgeId(1), GEdge::new(0, 6, edge("s", "w")))
            .with_deleted_edge(EdgeId(1), graph.edge(EdgeId(1)).ok_or("no edge 1")?.clone());
        reject(&low, "is not above the existing arc");
        // A deleted node that keeps a child.
        let not_leaf = GraphDelta::identity().with_deleted_node(graph.node(0).ok_or("no node 0")?.clone());
        reject(&not_leaf, "keeps its child");
        // A node that gains a parent without losing its own.
        let second_parent =
            GraphDelta::identity().with_added_edge(EdgeId(7), GEdge::new(1, 4, edge("w", "w")));
        reject(&second_parent, "without losing its own");
        // A node that loses its parent without gaining one.
        let orphan = GraphDelta::identity()
            .with_deleted_edge(EdgeId(0), graph.edge(EdgeId(0)).ok_or("no edge 0")?.clone());
        reject(&orphan, "loses its parent arc without gaining one");
        // Label, position, value and metadata changes.
        for key in [
            AttrKey::EdgeLabel { edge: EdgeId(0) },
            AttrKey::EdgePosition { edge: EdgeId(0) },
            AttrKey::Value { node: 1 },
            AttrKey::Meta { node: 1 },
        ] {
            let (old, new) = match &key {
                AttrKey::EdgeLabel { .. } => (
                    AttrVal::Label(edge("s", "w")),
                    AttrVal::Label(edge("s", "x")),
                ),
                AttrKey::EdgePosition { .. } => {
                    (AttrVal::Position(None), AttrVal::Position(Some(1)))
                }
                AttrKey::Value { .. } => (
                    AttrVal::Presence(None),
                    AttrVal::Presence(Some(crate::value::FieldPresence::Null)),
                ),
                _ => (
                    AttrVal::Meta(crate::graph_edit::NodeMeta::default()),
                    AttrVal::Meta(crate::graph_edit::NodeMeta {
                        position: Some(2),
                        ..crate::graph_edit::NodeMeta::default()
                    }),
                ),
            };
            let delta = GraphDelta::identity().with_change(key, old, new)?;
            reject(&delta, "a tree edit has no change of");
        }
        // An insertion under an arc id far above the next one is a tree
        // edit too: one that spends the ids below it first.
        let far = GraphDelta::identity()
            .with_added_node(Node::new(6, "w"))
            .with_added_edge(EdgeId(40), GEdge::new(0, 6, edge("s", "w")));
        let read = TreeEdit::from_graph_delta(&tree, &emb, &far)?;
        let mut again = emb.clone();
        assert_eq!(
            GraphDelta::from_tree_edit(&graph, &mut again, &read)?,
            far
        );
        // The same insertion under the next edge id is a tree edit.
        let next = GraphDelta::identity()
            .with_added_node(Node::new(6, "w"))
            .with_added_edge(EdgeId(5), GEdge::new(0, 6, edge("s", "w")));
        assert!(matches!(
            TreeEdit::from_graph_delta(&tree, &emb, &next),
            Ok(TreeEdit::InsertNode { child_id: 6, .. })
        ));
        Ok(())
    }

    /// doc(0) -> para(1) -> word(2) -> word(3), doc(0) -> para(4) ->
    /// word(5), and a fan `rel` over para 1 with children 2 and 5.
    fn paragraphs() -> WInstance {
        let nodes = HashMap::from([
            (0, Node::new(0, "doc")),
            (1, Node::new(1, "para")),
            (2, Node::new(2, "word")),
            (3, Node::new(3, "word")),
            (4, Node::new(4, "para")),
            (5, Node::new(5, "word")),
        ]);
        let arcs = vec![
            (0, 1, edge("doc", "para")),
            (1, 2, edge("para", "word")),
            (2, 3, edge("word", "word")),
            (0, 4, edge("doc", "para")),
            (4, 5, edge("para", "word")),
        ];
        let fan = Fan::new("rel", 1)
            .with_child("a", 2)
            .with_child("b", 5);
        WInstance::new(nodes, arcs, vec![fan], 0, "doc".into())
    }

    /// Lower `edit` on `tree`, read the delta back as a tree edit and lower
    /// that again: the delta must come back unchanged.
    fn assert_round_trips(tree: &WInstance, edit: &TreeEdit) -> TestResult {
        let (graph, emb) = GInstance::from_wtype_embedded(tree, None)?;
        let mut followed = emb.clone();
        let delta = GraphDelta::from_tree_edit(&graph, &mut followed, edit)?;
        let read = TreeEdit::from_graph_delta(tree, &emb, &delta)
            .map_err(|e| format!("{edit:?} lowers to a delta read as no tree edit: {e}"))?;
        let mut again = emb.clone();
        let lowered = GraphDelta::from_tree_edit(&graph, &mut again, &read)?;
        assert_eq!(lowered, delta, "{edit:?} read back as {read:?}");
        assert_eq!(again.arcs, followed.arcs);
        assert_eq!(again.fans, followed.fans);
        Ok(())
    }

    fn move_edit(tree: &WInstance, node_id: u32, new_parent: u32) -> TreeEdit {
        TreeEdit::MoveSubtree {
            node_id,
            new_parent,
            edge: Edge {
                src: tree.nodes[&new_parent].anchor.clone(),
                tgt: tree.nodes[&node_id].anchor.clone(),
                kind: "prop".into(),
                name: None,
            },
        }
    }

    #[test]
    fn from_graph_delta_reads_sequences_that_spend_ids() -> TestResult {
        let tree = paragraphs();
        // A move superseded by a second move of the same node: the delta's
        // arc skips the id the first move took.
        assert_round_trips(
            &tree,
            &TreeEdit::Sequence(vec![move_edit(&tree, 4, 3), move_edit(&tree, 4, 0)]),
        )?;
        // An insertion cancelled by a deletion before a move.
        assert_round_trips(
            &tree,
            &TreeEdit::Sequence(vec![
                insert_edit(&tree, 5, 9, "word"),
                TreeEdit::DeleteNode { id: 9 },
                move_edit(&tree, 4, 3),
            ]),
        )?;
        // A move of a node that is then deleted, before a new fan: the fan's
        // edges skip the id the move took.
        assert_round_trips(
            &tree,
            &TreeEdit::Sequence(vec![
                move_edit(&tree, 3, 0),
                TreeEdit::DeleteNode { id: 3 },
                TreeEdit::InsertFan {
                    fan: Fan::new("arg", 0).with_child("x", 4),
                },
            ]),
        )?;
        // A fan inserted and deleted before another fan: the second fan's
        // node and edges skip those of the first.
        assert_round_trips(
            &tree,
            &TreeEdit::Sequence(vec![
                TreeEdit::InsertFan {
                    fan: Fan::new("tmp", 0).with_child("x", 4).with_child("y", 2),
                },
                TreeEdit::DeleteFan {
                    hyper_edge_id: "tmp".into(),
                },
                TreeEdit::InsertFan {
                    fan: Fan::new("arg", 0).with_child("x", 4),
                },
            ]),
        )?;
        // An insertion under a high id that is then deleted, before a new
        // fan: the fan's node id is above the deleted node's.
        assert_round_trips(
            &tree,
            &TreeEdit::Sequence(vec![
                insert_edit(&tree, 0, 30, "word"),
                TreeEdit::DeleteNode { id: 30 },
                TreeEdit::InsertFan {
                    fan: Fan::new("arg", 0).with_child("x", 4),
                },
            ]),
        )?;
        // An insertion followed by moves of the inserted node.
        assert_round_trips(
            &tree,
            &TreeEdit::Sequence(vec![
                insert_edit(&tree, 0, 9, "word"),
                TreeEdit::MoveSubtree {
                    node_id: 9,
                    new_parent: 3,
                    edge: edge("word", "word"),
                },
                TreeEdit::MoveSubtree {
                    node_id: 9,
                    new_parent: 5,
                    edge: edge("word", "word"),
                },
            ]),
        )?;
        // A node moved out of the way of a move under its own descendant,
        // then moved back under its old parent.
        assert_round_trips(
            &tree,
            &TreeEdit::Sequence(vec![
                move_edit(&tree, 3, 0),
                TreeEdit::MoveSubtree {
                    node_id: 2,
                    new_parent: 3,
                    edge: edge("word", "word"),
                },
                TreeEdit::MoveSubtree {
                    node_id: 3,
                    new_parent: 0,
                    edge: edge("doc", "word"),
                },
            ]),
        )?;
        // Controls: the move alone, and the move before the cancelled
        // insertion.
        assert_round_trips(&tree, &move_edit(&tree, 4, 3))?;
        assert_round_trips(
            &tree,
            &TreeEdit::Sequence(vec![
                move_edit(&tree, 4, 3),
                insert_edit(&tree, 5, 9, "word"),
                TreeEdit::DeleteNode { id: 9 },
            ]),
        )
    }

    /// Two fans moved to fresh node ids by insertions that take their ids,
    /// after a fan inserted and deleted again: the delta skips node and edge
    /// ids that the edit read from it spends before it inserts node 8.
    #[test]
    fn from_graph_delta_reads_fans_moved_by_insertions() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "doc")),
                (3, Node::new(3, "doc")),
                (6, Node::new(6, "doc")),
            ]),
            vec![(0, 3, edge("doc", "doc")), (0, 6, edge("doc", "doc"))],
            vec![
                Fan::new("rel", 0).with_child("a", 6),
                Fan::new("rel", 0).with_child("a", 0),
            ],
            0,
            "doc".into(),
        );
        assert_round_trips(
            &tree,
            &TreeEdit::Sequence(vec![
                TreeEdit::InsertFan {
                    fan: Fan::new("arg", 3).with_child("a", 0),
                },
                TreeEdit::ContractNode { id: 3 },
                insert_edit(&tree, 0, 7, "doc"),
                insert_edit(&tree, 0, 8, "doc"),
                TreeEdit::DeleteNode { id: 7 },
                insert_edit(&tree, 0, 9, "doc"),
            ]),
        )
    }

    /// A fan moved twice by insertions that take its id, the first of
    /// them deleted again, around two new fans.
    #[test]
    fn from_graph_delta_reads_a_fan_moved_through_a_deleted_insertion() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([(0, Node::new(0, "doc")), (3, Node::new(3, "doc"))]),
            vec![(0, 3, edge("doc", "doc"))],
            vec![Fan::new("rel", 0).with_child("a", 0)],
            0,
            "doc".into(),
        );
        assert_round_trips(
            &tree,
            &TreeEdit::Sequence(vec![
                TreeEdit::DeleteNode { id: 3 },
                insert_edit(&tree, 0, 4, "doc"),
                TreeEdit::InsertFan {
                    fan: Fan::new("arg", 0).with_child("a", 0),
                },
                TreeEdit::DeleteNode { id: 4 },
                insert_edit(&tree, 0, 5, "doc"),
                TreeEdit::InsertFan {
                    fan: Fan::new("arg", 0).with_child("a", 0),
                },
            ]),
        )
    }

    /// A move under a new node that is then moved back under its parent:
    /// the new node goes in early, beside the moved node, not under it.
    #[test]
    fn from_graph_delta_reads_a_move_under_a_node_inserted_after_it() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([(0, Node::new(0, "doc")), (3, Node::new(3, "doc"))]),
            vec![(0, 3, edge("doc", "doc"))],
            vec![],
            0,
            "doc".into(),
        );
        assert_round_trips(
            &tree,
            &TreeEdit::Sequence(vec![
                insert_edit(&tree, 0, 4, "doc"),
                TreeEdit::MoveSubtree {
                    node_id: 3,
                    new_parent: 4,
                    edge: edge("doc", "doc"),
                },
                TreeEdit::MoveSubtree {
                    node_id: 4,
                    new_parent: 0,
                    edge: edge("doc", "doc"),
                },
            ]),
        )
    }

    /// A new node inserted before a move of another node under its own
    /// parent, given a child, and moved under its own parent too.
    #[test]
    fn from_graph_delta_reads_a_node_inserted_before_a_move() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "doc")),
                (3, Node::new(3, "doc")),
                (6, Node::new(6, "doc")),
            ]),
            vec![(0, 3, edge("doc", "doc")), (0, 6, edge("doc", "doc"))],
            vec![],
            0,
            "doc".into(),
        );
        let to = |node_id: u32, new_parent: u32| TreeEdit::MoveSubtree {
            node_id,
            new_parent,
            edge: edge("doc", "doc"),
        };
        assert_round_trips(
            &tree,
            &TreeEdit::Sequence(vec![
                insert_edit(&tree, 0, 13, "doc"),
                to(3, 0),
                TreeEdit::InsertNode {
                    parent: 13,
                    child_id: 14,
                    node: Node::new(14, "doc"),
                    edge: edge("doc", "doc"),
                },
                to(13, 0),
            ]),
        )
    }

    /// A node inserted under a node that then moves under its own parent
    /// twice, around a new fan: the new node goes in early to spend the
    /// node id below the fan's.
    #[test]
    fn from_graph_delta_reads_a_node_inserted_before_a_fan() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([(0, Node::new(0, "doc")), (3, Node::new(3, "doc"))]),
            vec![(0, 3, edge("doc", "doc"))],
            vec![],
            0,
            "doc".into(),
        );
        let to = |node_id: u32, new_parent: u32| TreeEdit::MoveSubtree {
            node_id,
            new_parent,
            edge: edge("doc", "doc"),
        };
        assert_round_trips(
            &tree,
            &TreeEdit::Sequence(vec![
                insert_edit(&tree, 3, 4, "doc"),
                to(3, 0),
                TreeEdit::InsertFan {
                    fan: Fan::new("arg", 0).with_child("a", 4),
                },
                to(4, 3),
                to(3, 0),
            ]),
        )
    }

    #[test]
    fn delete_node_refuses_the_root() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([(0, Node::new(0, "doc"))]),
            vec![],
            vec![],
            0,
            "doc".into(),
        );
        let delete = TreeEdit::DeleteNode { id: 0 };
        let mut applied = tree.clone();
        let tree_result = delete.apply(&mut applied);
        assert!(
            matches!(&tree_result, Err(EditError::ApplyFailed(reason)) if reason.contains("root")),
            "{tree_result:?}"
        );
        assert!(applied.nodes.contains_key(&0), "a refused deletion leaves the tree");
        let (graph, mut emb) = GInstance::from_wtype_embedded(&tree, None)?;
        let graph_result = GraphDelta::from_tree_edit(&graph, &mut emb, &delete);
        assert_eq!(graph_result.map(|_| ()), tree_result);
        Ok(())
    }

    #[test]
    fn insert_node_refuses_a_node_filed_under_another_id() -> TestResult {
        let tree = paragraphs();
        let misfiled = TreeEdit::InsertNode {
            parent: 5,
            child_id: 9,
            node: Node::new(8, "word"),
            edge: edge("word", "word"),
        };
        let mut applied = tree.clone();
        let tree_result = misfiled.apply(&mut applied);
        assert!(
            matches!(&tree_result, Err(EditError::ApplyFailed(reason)) if reason.contains("node 8")),
            "{tree_result:?}"
        );
        assert!(
            !applied.nodes.contains_key(&9) && applied.nodes.len() == tree.nodes.len(),
            "a refused insertion leaves the tree"
        );
        assert_eq!(applied.arcs, tree.arcs);
        let (graph, mut emb) = GInstance::from_wtype_embedded(&tree, None)?;
        let before = emb.clone();
        let graph_result = GraphDelta::from_tree_edit(&graph, &mut emb, &misfiled);
        assert_eq!(graph_result.map(|_| ()), tree_result);
        assert_eq!(emb, before);
        Ok(())
    }

    #[test]
    fn a_tree_edit_and_its_singleton_sequence_encode_identically() {
        let edit = TreeEdit::JoinFeatures {
            primary: 2,
            joined: vec![1, 3],
            produce: Node::new(2, "a"),
        };
        let bytes = |e: &TreeEdit| {
            let mut writer = CanonicalWriter::new();
            canonical_tree_edit_bytes(e, &mut writer);
            writer.into_bytes()
        };
        let single = TreeEdit::Sequence(vec![edit.clone()]);
        let nested = TreeEdit::Sequence(vec![
            TreeEdit::Identity,
            TreeEdit::Sequence(vec![edit.clone(), TreeEdit::Identity]),
        ]);
        assert_eq!(bytes(&edit), bytes(&single));
        assert_eq!(bytes(&edit), bytes(&nested));
        assert_eq!(edit.digest(), nested.digest());
        assert_eq!(
            bytes(&TreeEdit::Identity),
            bytes(&TreeEdit::Sequence(vec![]))
        );
        let two = TreeEdit::Sequence(vec![edit.clone(), TreeEdit::DeleteNode { id: 3 }]);
        let regrouped = TreeEdit::Sequence(vec![
            TreeEdit::Sequence(vec![edit]),
            TreeEdit::DeleteNode { id: 3 },
        ]);
        assert_eq!(bytes(&two), bytes(&regrouped));
        assert_ne!(bytes(&two), bytes(&TreeEdit::DeleteNode { id: 3 }));
    }

    /// The canonical bytes of one step of each variant, pinned.
    #[test]
    fn tree_edit_bytes_are_pinned() -> TestResult {
        let edit = TreeEdit::Sequence(vec![
            TreeEdit::InsertNode {
                parent: 0,
                child_id: 6,
                node: Node::new(6, "w"),
                edge: edge("s", "w"),
            },
            TreeEdit::DeleteNode { id: 1 },
            TreeEdit::ContractNode { id: 2 },
            TreeEdit::RelabelNode {
                id: 3,
                new_anchor: "v".into(),
            },
            TreeEdit::SetField {
                node_id: 4,
                field: "f".into(),
                value: Value::Float(-0.0),
            },
            TreeEdit::RemoveField {
                node_id: 4,
                field: "g".into(),
            },
            TreeEdit::MoveSubtree {
                node_id: 4,
                new_parent: 0,
                edge: edge("s", "w"),
            },
            TreeEdit::InsertFan {
                fan: Fan::new("rel", 0).with_child("b", 4).with_child("a", 1),
            },
            TreeEdit::DeleteFan {
                hyper_edge_id: "rel".into(),
            },
            TreeEdit::JoinFeatures {
                primary: 2,
                joined: vec![3, 1],
                produce: Node::new(2, "a"),
            },
        ]);
        let mut writer = CanonicalWriter::new();
        canonical_tree_edit_bytes(&edit, &mut writer);
        let hex: String = writer
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(hex, PINNED_BYTES);
        assert_eq!(edit.digest().to_hex(), PINNED_DIGEST);
        Ok(())
    }

    /// A node deleted and inserted again under its own id, given a child,
    /// then re-parented by a contraction: the delta replaces the node, and
    /// its child's arc comes before the node's own new arc.
    #[test]
    fn from_graph_delta_reads_a_replaced_node_with_a_child_older_than_its_arc() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "doc")),
                (3, Node::new(3, "doc")),
                (6, Node::new(6, "doc")),
                (9, Node::new(9, "doc")),
            ]),
            vec![
                (0, 3, edge("doc", "doc")),
                (0, 6, edge("doc", "doc")),
                (0, 9, edge("doc", "doc")),
            ],
            vec![],
            0,
            "doc".into(),
        );
        let mut produce = Node::new(0, "doc");
        produce.anchor = "joined".into();
        let edit = TreeEdit::Sequence(vec![
            TreeEdit::DeleteNode { id: 6 },
            TreeEdit::JoinFeatures {
                primary: 0,
                joined: vec![9],
                produce,
            },
            insert_edit(&tree, 3, 6, "doc"),
            TreeEdit::InsertNode {
                parent: 6,
                child_id: 7,
                node: Node::new(7, "para"),
                edge: edge("doc", "para"),
            },
            TreeEdit::ContractNode { id: 3 },
        ]);
        assert_round_trips(&tree, &edit)
    }

    /// A tree node inserted under the node id of a fan deleted before it,
    /// then a new fan and a contraction: the delta replaces the fan node by
    /// the tree node.
    #[test]
    fn from_graph_delta_reads_a_tree_node_under_a_deleted_fan_id() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "doc")),
                (3, Node::new(3, "doc")),
                (6, Node::new(6, "doc")),
                (9, Node::new(9, "doc")),
            ]),
            vec![
                (0, 3, edge("doc", "doc")),
                (0, 6, edge("doc", "doc")),
                (3, 9, edge("doc", "doc")),
            ],
            vec![Fan::new("rel", 0).with_child("a", 3)],
            0,
            "doc".into(),
        );
        let edit = TreeEdit::Sequence(vec![
            TreeEdit::DeleteFan {
                hyper_edge_id: "rel".into(),
            },
            insert_edit(&tree, 0, 10, "doc"),
            TreeEdit::InsertFan {
                fan: Fan::new("arg", 0).with_child("a", 6),
            },
            TreeEdit::ContractNode { id: 3 },
        ]);
        assert_round_trips(&tree, &edit)
    }

    /// A node inserted under a node that then moves, given a child and
    /// moved under the root: the early insertion that spends the skipped
    /// edge id must not go under the node it is later given as a child.
    #[test]
    fn from_graph_delta_reads_an_insertion_that_later_takes_a_sibling() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "doc")),
                (3, Node::new(3, "doc")),
                (6, Node::new(6, "doc")),
                (9, Node::new(9, "doc")),
                (12, Node::new(12, "doc")),
            ]),
            vec![
                (0, 3, edge("doc", "doc")),
                (0, 6, edge("doc", "doc")),
                (0, 9, edge("doc", "doc")),
                (0, 12, edge("doc", "doc")),
            ],
            vec![],
            0,
            "doc".into(),
        );
        let edit = TreeEdit::Sequence(vec![
            insert_edit(&tree, 6, 13, "doc"),
            move_edit(&tree, 6, 9),
            TreeEdit::MoveSubtree {
                node_id: 3,
                new_parent: 13,
                edge: edge("doc", "doc"),
            },
            TreeEdit::MoveSubtree {
                node_id: 13,
                new_parent: 0,
                edge: edge("doc", "doc"),
            },
        ]);
        assert_round_trips(&tree, &edit)
    }

    /// A contraction that deletes a fan sharing its name with a fan that
    /// stays, then an insertion under the deleted fan's node id: the fan
    /// goes with the contracted node before the insertion, whose skipped
    /// edge id the contraction's re-parented arc takes.
    #[test]
    fn from_graph_delta_reads_a_contraction_freeing_a_fan_id() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "doc")),
                (3, Node::new(3, "doc")),
                (6, Node::new(6, "doc")),
                (9, Node::new(9, "doc")),
            ]),
            vec![
                (0, 3, edge("doc", "doc")),
                (0, 6, edge("doc", "doc")),
                (3, 9, edge("doc", "doc")),
            ],
            vec![
                Fan::new("arg", 0).with_child("a", 3),
                Fan::new("arg", 0).with_child("a", 0),
            ],
            0,
            "doc".into(),
        );
        let edit = TreeEdit::Sequence(vec![
            TreeEdit::SetField {
                node_id: 0,
                field: "f".into(),
                value: Value::Int(0),
            },
            TreeEdit::ContractNode { id: 3 },
            insert_edit(&tree, 0, 10, "doc"),
            move_edit(&tree, 9, 0),
            TreeEdit::InsertFan {
                fan: Fan::new("arg", 0).with_child("a", 0),
            },
        ]);
        assert_round_trips(&tree, &edit)
    }

    /// A tree node inserted under a fan's node id, which moves the fan on,
    /// then a join of the node the fan names and two new fans.
    #[test]
    fn from_graph_delta_reads_a_join_after_a_fan_is_moved_on() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "doc")),
                (3, Node::new(3, "doc")),
                (6, Node::new(6, "doc")),
            ]),
            vec![(0, 3, edge("doc", "doc")), (0, 6, edge("doc", "doc"))],
            vec![Fan::new("rel", 0).with_child("a", 3)],
            0,
            "doc".into(),
        );
        let edit = TreeEdit::Sequence(vec![
            insert_edit(&tree, 6, 7, "doc"),
            TreeEdit::JoinFeatures {
                primary: 6,
                joined: vec![3],
                produce: Node::new(6, "doc"),
            },
            TreeEdit::InsertFan {
                fan: Fan::new("arg", 0).with_child("a", 0),
            },
            TreeEdit::InsertFan {
                fan: Fan::new("arg", 0).with_child("a", 0),
            },
        ]);
        assert_round_trips(&tree, &edit)
    }

    /// A tree node inserted under a fan's node id, which moves the fan on,
    /// then two new fans of the old fan's name, the second naming the new
    /// node, and a contraction that deletes the old fan.
    #[test]
    fn from_graph_delta_reads_new_fans_after_an_old_fan_of_their_name_moves_on() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "doc")),
                (3, Node::new(3, "doc")),
                (6, Node::new(6, "doc")),
            ]),
            vec![(0, 3, edge("doc", "doc")), (3, 6, edge("doc", "doc"))],
            vec![Fan::new("arg", 0).with_child("a", 3)],
            0,
            "doc".into(),
        );
        let edit = TreeEdit::Sequence(vec![
            insert_edit(&tree, 3, 7, "doc"),
            TreeEdit::InsertFan {
                fan: Fan::new("arg", 0).with_child("a", 6),
            },
            TreeEdit::InsertFan {
                fan: Fan::new("arg", 6).with_child("a", 7),
            },
            TreeEdit::ContractNode { id: 3 },
        ]);
        assert_round_trips(&tree, &edit)
    }

    /// A contraction deleting a fan that shares its name with a fan that
    /// stays, an insertion under the deleted fan's node id, a deletion, a
    /// new fan naming the inserted node and a contraction re-parenting it.
    #[test]
    fn from_graph_delta_reads_an_insertion_under_a_fan_id_freed_by_a_contraction() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "doc")),
                (3, Node::new(3, "doc")),
                (6, Node::new(6, "doc")),
                (9, Node::new(9, "doc")),
            ]),
            vec![
                (0, 3, edge("doc", "doc")),
                (0, 6, edge("doc", "doc")),
                (0, 9, edge("doc", "doc")),
            ],
            vec![
                Fan::new("rel", 0).with_child("a", 6),
                Fan::new("rel", 0).with_child("a", 0),
            ],
            0,
            "doc".into(),
        );
        let edit = TreeEdit::Sequence(vec![
            TreeEdit::ContractNode { id: 6 },
            insert_edit(&tree, 3, 10, "doc"),
            TreeEdit::DeleteNode { id: 9 },
            TreeEdit::InsertFan {
                fan: Fan::new("arg", 0).with_child("a", 10),
            },
            TreeEdit::ContractNode { id: 3 },
        ]);
        assert_round_trips(&tree, &edit)
    }

    /// A deletion of the parent a fan names, an insertion under the fan's
    /// node id, a deletion, a new fan and a contraction: the fan goes with
    /// its deleted parent, not with the contracted node it also names.
    #[test]
    fn from_graph_delta_reads_a_fan_freed_by_its_deleted_parent() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "doc")),
                (3, Node::new(3, "doc")),
                (6, Node::new(6, "doc")),
                (9, Node::new(9, "doc")),
            ]),
            vec![
                (0, 3, edge("doc", "doc")),
                (0, 6, edge("doc", "doc")),
                (3, 9, edge("doc", "doc")),
            ],
            vec![
                Fan::new("arg", 6).with_child("a", 3),
                Fan::new("arg", 0).with_child("a", 0),
            ],
            0,
            "doc".into(),
        );
        let edit = TreeEdit::Sequence(vec![
            TreeEdit::DeleteNode { id: 6 },
            insert_edit(&tree, 3, 10, "doc"),
            TreeEdit::DeleteNode { id: 9 },
            TreeEdit::InsertFan {
                fan: Fan::new("arg", 0).with_child("a", 10),
            },
            TreeEdit::ContractNode { id: 3 },
        ]);
        assert_round_trips(&tree, &edit)
    }

    /// Two fans of one name, the first moved on by a tree node taking its
    /// id, then both deleted, a tree node under the second's id and two new
    /// fans: the fans go between the two insertions.
    #[test]
    fn from_graph_delta_reads_fans_deleted_between_insertions_under_their_ids() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([(0, Node::new(0, "doc")), (3, Node::new(3, "doc"))]),
            vec![(0, 3, edge("doc", "doc"))],
            vec![
                Fan::new("rel", 0).with_child("a", 0),
                Fan::new("rel", 0).with_child("a", 0),
            ],
            0,
            "doc".into(),
        );
        let edit = TreeEdit::Sequence(vec![
            insert_edit(&tree, 0, 4, "doc"),
            TreeEdit::DeleteFan {
                hyper_edge_id: "rel".into(),
            },
            insert_edit(&tree, 0, 5, "doc"),
            TreeEdit::InsertFan {
                fan: Fan::new("arg", 0).with_child("a", 0),
            },
            TreeEdit::InsertFan {
                fan: Fan::new("arg", 0).with_child("a", 0),
            },
        ]);
        assert_round_trips(&tree, &edit)
    }

    /// A new fan, an insertion under the node id of a fan whose named
    /// node is deleted after it, a second new fan and an insertion under
    /// the node id of a fan that stays: the first old fan moves on before
    /// it goes, and the second moves on and stays.
    #[test]
    fn from_graph_delta_reads_fans_moved_on_before_one_goes() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "doc")),
                (3, Node::new(3, "doc")),
                (6, Node::new(6, "doc")),
                (9, Node::new(9, "doc")),
                (12, Node::new(12, "doc")),
                (15, Node::new(15, "doc")),
            ]),
            vec![
                (0, 3, edge("doc", "doc")),
                (0, 6, edge("doc", "doc")),
                (0, 9, edge("doc", "doc")),
                (3, 12, edge("doc", "doc")),
                (6, 15, edge("doc", "doc")),
            ],
            vec![
                Fan::new("rel", 0).with_child("a", 15),
                Fan::new("rel", 0).with_child("a", 0),
            ],
            0,
            "doc".into(),
        );
        let edit = TreeEdit::Sequence(vec![
            TreeEdit::InsertFan {
                fan: Fan::new("arg", 0).with_child("a", 0),
            },
            insert_edit(&tree, 0, 16, "doc"),
            TreeEdit::DeleteNode { id: 15 },
            TreeEdit::InsertFan {
                fan: Fan::new("arg", 0).with_child("a", 0),
            },
            insert_edit(&tree, 0, 17, "doc"),
        ]);
        assert_round_trips(&tree, &edit)
    }

    /// An insertion deleted again after a new fan and a move of its parent
    /// under the parent it has: the move takes a fresh arc only because the
    /// inserted node's arc follows it, so the node spending that edge id
    /// stays until the move is made.
    #[test]
    fn from_graph_delta_reads_a_move_in_place_after_a_cancelled_insertion() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([(0, Node::new(0, "doc")), (3, Node::new(3, "doc"))]),
            vec![(0, 3, edge("doc", "doc"))],
            vec![],
            0,
            "doc".into(),
        );
        let edit = TreeEdit::Sequence(vec![
            insert_edit(&tree, 3, 4, "doc"),
            TreeEdit::InsertFan {
                fan: Fan::new("arg", 0).with_child("a", 0),
            },
            move_edit(&tree, 3, 0),
            TreeEdit::DeleteNode { id: 4 },
        ]);
        assert_round_trips(&tree, &edit)
    }

    /// A new fan moved on three times by tree nodes taking its node id,
    /// two of them deleted again, before a second new fan.
    #[test]
    fn from_graph_delta_reads_a_new_fan_moved_on_three_times() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([(0, Node::new(0, "doc")), (3, Node::new(3, "doc"))]),
            vec![(0, 3, edge("doc", "doc"))],
            vec![],
            0,
            "doc".into(),
        );
        let edit = TreeEdit::Sequence(vec![
            TreeEdit::InsertFan {
                fan: Fan::new("arg", 0).with_child("a", 0),
            },
            insert_edit(&tree, 0, 4, "doc"),
            TreeEdit::JoinFeatures {
                primary: 0,
                joined: vec![4],
                produce: Node::new(0, "doc"),
            },
            insert_edit(&tree, 0, 5, "doc"),
            TreeEdit::DeleteNode { id: 5 },
            insert_edit(&tree, 0, 6, "doc"),
            TreeEdit::InsertFan {
                fan: Fan::new("arg", 0).with_child("a", 0),
            },
        ]);
        assert_round_trips(&tree, &edit)
    }

    /// A new fan and an old one, each moved on by a tree node taking its
    /// node id, around a join of the node the old fan names, before a
    /// second new fan.
    #[test]
    fn from_graph_delta_reads_fans_moved_on_around_a_join() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "doc")),
                (3, Node::new(3, "doc")),
                (6, Node::new(6, "doc")),
            ]),
            vec![(0, 3, edge("doc", "doc")), (0, 6, edge("doc", "doc"))],
            vec![Fan::new("arg", 0).with_child("a", 6)],
            0,
            "doc".into(),
        );
        let edit = TreeEdit::Sequence(vec![
            TreeEdit::InsertFan {
                fan: Fan::new("arg", 0).with_child("a", 0),
            },
            insert_edit(&tree, 0, 7, "doc"),
            TreeEdit::JoinFeatures {
                primary: 0,
                joined: vec![6],
                produce: Node::new(0, "doc"),
            },
            insert_edit(&tree, 0, 8, "doc"),
            TreeEdit::InsertFan {
                fan: Fan::new("arg", 0).with_child("a", 0),
            },
        ]);
        assert_round_trips(&tree, &edit)
    }

    /// A new fan deleted again, a second new fan, an insertion that moves
    /// an old fan on, a third new fan and an insertion under the deleted
    /// fan's node id.
    #[test]
    fn from_graph_delta_reads_an_insertion_under_a_cancelled_fan_id() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([(0, Node::new(0, "doc")), (3, Node::new(3, "doc"))]),
            vec![(0, 3, edge("doc", "doc"))],
            vec![Fan::new("rel", 0).with_child("a", 0)],
            0,
            "doc".into(),
        );
        let arg = || TreeEdit::InsertFan {
            fan: Fan::new("arg", 0).with_child("a", 0),
        };
        let edit = TreeEdit::Sequence(vec![
            arg(),
            TreeEdit::DeleteFan {
                hyper_edge_id: "arg".into(),
            },
            arg(),
            insert_edit(&tree, 0, 4, "doc"),
            arg(),
            insert_edit(&tree, 0, 5, "doc"),
        ]);
        assert_round_trips(&tree, &edit)
    }

    /// Insertions under the node ids of two fans in turn, one fan moved on
    /// twice more by insertions under the ids it moves to and then deleted
    /// by a contraction that re-parents the first inserted node: that node
    /// goes in early, under the fan's id, before a node with a lower id.
    #[test]
    fn from_graph_delta_reads_insertions_chasing_a_fan() -> TestResult {
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "doc")),
                (3, Node::new(3, "doc")),
                (6, Node::new(6, "doc")),
            ]),
            vec![(0, 3, edge("doc", "doc")), (0, 6, edge("doc", "doc"))],
            vec![
                Fan::new("rel", 0).with_child("a", 3),
                Fan::new("rel", 0).with_child("a", 0),
            ],
            0,
            "doc".into(),
        );
        let edit = TreeEdit::Sequence(vec![
            insert_edit(&tree, 3, 7, "doc"),
            insert_edit(&tree, 0, 8, "doc"),
            insert_edit(&tree, 6, 9, "doc"),
            TreeEdit::InsertNode {
                parent: 7,
                child_id: 11,
                node: Node::new(11, "doc"),
                edge: edge("doc", "doc"),
            },
            insert_edit(&tree, 6, 4, "doc"),
            TreeEdit::ContractNode { id: 3 },
        ]);
        assert_round_trips(&tree, &edit)
    }

    const ANCHORS: [&str; 3] = ["doc", "para", "word"];

    /// A tree over nodes whose ids are spread out, each node below one of
    /// the nodes before it, with fans over its nodes.
    fn arb_tree() -> impl Strategy<Value = WInstance> {
        (2usize..7)
            .prop_flat_map(|n| {
                (
                    prop::collection::vec(any::<prop::sample::Index>(), n),
                    prop::collection::vec(0..ANCHORS.len(), n),
                    prop::collection::vec(prop::option::of(-2_i64..2), n),
                    prop::collection::vec(
                        (
                            any::<prop::sample::Index>(),
                            prop::collection::btree_map(
                                prop::sample::select(vec!["a", "b", "c"]),
                                any::<prop::sample::Index>(),
                                1..3,
                            ),
                            prop::sample::select(vec!["rel", "arg"]),
                        ),
                        0..3,
                    ),
                )
            })
            .prop_map(|(parents, anchors, fields, fans)| {
                let n = parents.len();
                // Trees have a handful of nodes, so the ids fit.
                let id = |i: usize| u32::try_from(i * 3).unwrap_or(u32::MAX);
                let mut nodes = HashMap::new();
                for i in 0..n {
                    let mut node = Node::new(id(i), ANCHORS[anchors[i]]);
                    if let Some(v) = fields[i] {
                        node.extra_fields.insert("f".into(), Value::Int(v));
                    }
                    nodes.insert(id(i), node);
                }
                let arcs = (1..n)
                    .map(|i| {
                        let p = parents[i].index(i);
                        (id(p), id(i), edge(ANCHORS[anchors[p]], ANCHORS[anchors[i]]))
                    })
                    .collect();
                let fans = fans
                    .into_iter()
                    .map(|(parent, children, name)| {
                        let mut fan = Fan::new(name, id(parent.index(n)));
                        for (label, child) in children {
                            fan.children.insert(label.to_owned(), id(child.index(n)));
                        }
                        fan
                    })
                    .collect();
                WInstance::new(nodes, arcs, fans, 0, ANCHORS[anchors[0]].into())
            })
    }

    /// One step of a generated edit script, resolved against the tree it
    /// applies to; `Raw` names ids at random and may not apply.
    #[derive(Clone, Debug)]
    enum Intent {
        Insert(prop::sample::Index, usize, bool),
        Delete(prop::sample::Index),
        Contract(prop::sample::Index),
        Relabel(prop::sample::Index, usize),
        Set(prop::sample::Index, i64),
        Remove(prop::sample::Index),
        Move(prop::sample::Index, prop::sample::Index),
        AddFan(prop::sample::Index, prop::sample::Index),
        DropFan(prop::sample::Index),
        Join(prop::sample::Index, prop::sample::Index, bool),
        Raw(u8, u32, u32),
    }

    fn intent() -> impl Strategy<Value = Intent> {
        let ix = any::<prop::sample::Index>;
        prop_oneof![
            (ix(), 0..ANCHORS.len(), any::<bool>()).prop_map(|(p, a, f)| Intent::Insert(p, a, f)),
            ix().prop_map(Intent::Delete),
            ix().prop_map(Intent::Contract),
            (ix(), 0..ANCHORS.len()).prop_map(|(n, a)| Intent::Relabel(n, a)),
            (ix(), -2_i64..2).prop_map(|(n, v)| Intent::Set(n, v)),
            ix().prop_map(Intent::Remove),
            (ix(), ix()).prop_map(|(n, p)| Intent::Move(n, p)),
            (ix(), ix()).prop_map(|(p, c)| Intent::AddFan(p, c)),
            ix().prop_map(Intent::DropFan),
            (ix(), ix(), any::<bool>()).prop_map(|(p, j, r)| Intent::Join(p, j, r)),
            (0u8..5, 0u32..20, 0u32..20).prop_map(|(k, a, b)| Intent::Raw(k, a, b)),
        ]
    }

    /// Whether `a` is `b` or an ancestor of it.
    fn above(tree: &WInstance, a: u32, b: u32) -> bool {
        a == b || super::is_descendant(tree, b, a)
    }

    /// An insertion of a node with `anchor` under `parent`.
    fn insert_edit(tree: &WInstance, parent: u32, child_id: u32, anchor: &str) -> TreeEdit {
        TreeEdit::InsertNode {
            parent,
            child_id,
            node: Node::new(child_id, anchor),
            edge: Edge {
                src: tree.nodes[&parent].anchor.clone(),
                tgt: anchor.into(),
                kind: "prop".into(),
                name: None,
            },
        }
    }

    /// An edit naming ids at random, which may not apply.
    fn raw_edit(kind: u8, a: u32, b: u32) -> TreeEdit {
        match kind {
            0 => TreeEdit::DeleteNode { id: a },
            1 => TreeEdit::MoveSubtree {
                node_id: a,
                new_parent: b,
                edge: edge("doc", "doc"),
            },
            2 => TreeEdit::ContractNode { id: a },
            3 => TreeEdit::InsertNode {
                parent: a,
                child_id: b,
                node: Node::new(b + 1, "doc"),
                edge: edge("doc", "doc"),
            },
            _ => TreeEdit::InsertNode {
                parent: a,
                child_id: b,
                node: Node::new(b, "doc"),
                edge: edge("doc", "doc"),
            },
        }
    }

    /// The edit an intent names in `tree` under `embedding`, when there is
    /// one.
    fn resolve(tree: &WInstance, embedding: &TreeEmbedding, intent: &Intent) -> Option<TreeEdit> {
        let mut ids: Vec<u32> = tree.nodes.keys().copied().collect();
        ids.sort_unstable();
        let non_root: Vec<u32> = ids.iter().copied().filter(|i| *i != tree.root).collect();
        let pick =
            |from: &[u32], at: &prop::sample::Index| (!from.is_empty()).then(|| *at.get(from));
        let anchor = |id: u32| tree.nodes[&id].anchor.clone();
        Some(match intent {
            Intent::Insert(p, a, take_fan) => {
                let child_id = match embedding.fans.first() {
                    Some(fan) if *take_fan => fan.node,
                    _ => ids.last().copied().unwrap_or(0) + 1,
                };
                insert_edit(tree, pick(&ids, p)?, child_id, ANCHORS[*a])
            }
            Intent::Delete(n) => {
                let leaves: Vec<u32> = non_root
                    .iter()
                    .copied()
                    .filter(|i| {
                        tree.children_map
                            .get(i)
                            .is_none_or(smallvec::SmallVec::is_empty)
                    })
                    .collect();
                TreeEdit::DeleteNode {
                    id: pick(&leaves, n)?,
                }
            }
            Intent::Contract(n) => TreeEdit::ContractNode {
                id: pick(&non_root, n)?,
            },
            Intent::Relabel(n, a) => TreeEdit::RelabelNode {
                id: pick(&ids, n)?,
                new_anchor: ANCHORS[*a].into(),
            },
            Intent::Set(n, v) => TreeEdit::SetField {
                node_id: pick(&ids, n)?,
                field: "f".into(),
                value: Value::Int(*v),
            },
            Intent::Remove(n) => TreeEdit::RemoveField {
                node_id: pick(&ids, n)?,
                field: "f".into(),
            },
            Intent::Move(n, p) => {
                let node_id = pick(&non_root, n)?;
                let parents: Vec<u32> = ids
                    .iter()
                    .copied()
                    .filter(|q| !above(tree, node_id, *q))
                    .collect();
                let new_parent = pick(&parents, p)?;
                TreeEdit::MoveSubtree {
                    node_id,
                    new_parent,
                    edge: Edge {
                        src: anchor(new_parent),
                        tgt: anchor(node_id),
                        kind: "prop".into(),
                        name: None,
                    },
                }
            }
            Intent::AddFan(p, c) => TreeEdit::InsertFan {
                fan: Fan::new("arg", pick(&ids, p)?).with_child("a", pick(&ids, c)?),
            },
            Intent::DropFan(k) => {
                let names: Vec<String> =
                    tree.fans.iter().map(|f| f.hyper_edge_id.clone()).collect();
                if names.is_empty() {
                    return None;
                }
                TreeEdit::DeleteFan {
                    hyper_edge_id: Name::from(k.get(&names).as_str()),
                }
            }
            Intent::Join(p, j, renamed) => {
                let primary = pick(&ids, p)?;
                let joinable: Vec<u32> = non_root
                    .iter()
                    .copied()
                    .filter(|q| !above(tree, *q, primary))
                    .collect();
                let joined = pick(&joinable, j)?;
                let mut produce = tree.nodes[&primary].clone();
                if *renamed {
                    produce.anchor = "joined".into();
                }
                TreeEdit::JoinFeatures {
                    primary,
                    joined: vec![joined],
                    produce,
                }
            }
            Intent::Raw(kind, a, b) => raw_edit(*kind, *a, *b),
        })
    }

    /// Whether a graph delta changes an entry no tree edit changes.
    fn outside_tree_edits(delta: &GraphDelta) -> bool {
        delta.attrs().keys().any(|key| {
            matches!(
                key,
                AttrKey::EdgeLabel { .. }
                    | AttrKey::EdgePosition { .. }
                    | AttrKey::Value { .. }
                    | AttrKey::Meta { .. }
            )
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20000))]

        #[test]
        fn from_tree_edit_commutes_with_embedding(
            tree in arb_tree(),
            intents in prop::collection::vec(intent(), 1..10),
        ) {
            let (mut graph, mut embedding) = GInstance::from_wtype_embedded(&tree, None).map_err(fail)?;
            let mut tree = tree;
            let start = (graph.clone(), embedding.clone(), tree.clone());
            let mut script = Vec::new();
            for intent in &intents {
                let Some(edit) = resolve(&tree, &embedding, intent) else { continue };
                let mut lowered = embedding.clone();
                let delta = GraphDelta::from_tree_edit(&graph, &mut lowered, &edit);
                let mut followed = embedding.clone();
                let mut edited = tree.clone();
                match followed.apply_edit(&mut edited, &edit) {
                    Ok(()) => {
                        let delta = delta.map_err(|e| TestCaseError::fail(format!("{edit:?}: {e}")))?;
                        prop_assert_eq!(&lowered, &followed);
                        let mut reached = graph.clone();
                        delta.apply(&mut reached).map_err(|e| TestCaseError::fail(format!("{e}")))?;
                        let expected = GInstance::from_wtype_under(&edited, &followed, None).map_err(fail)?;
                        prop_assert!(GInstance::identical(&reached, &expected), "{:?}", edit);
                        graph = reached;
                        embedding = followed;
                        tree = edited;
                        script.push(edit);
                    }
                    Err(err) => {
                        prop_assert_eq!(delta, Err(err));
                        prop_assert_eq!(&lowered, &embedding, "a failing edit leaves the embedding");
                    }
                }
            }
            // The whole script as one sequence: the composite of its steps.
            let (start_graph, mut start_embedding, _) = start;
            let whole = GraphDelta::from_tree_edit(&start_graph, &mut start_embedding, &TreeEdit::Sequence(script))
                .map_err(|e| TestCaseError::fail(format!("{e}")))?;
            prop_assert_eq!(&start_embedding, &embedding);
            let mut reached = start_graph;
            whole.apply(&mut reached).map_err(|e| TestCaseError::fail(format!("{e}")))?;
            prop_assert!(GInstance::identical(&reached, &graph));
        }

        #[test]
        fn tree_edit_round_trips_through_graph_delta(
            tree in arb_tree(),
            intents in prop::collection::vec(intent(), 1..8),
        ) {
            let (mut graph, mut embedding) = GInstance::from_wtype_embedded(&tree, None).map_err(fail)?;
            let start = (tree.clone(), graph.clone(), embedding.clone());
            let mut tree = tree;
            let mut script = Vec::new();
            for intent in &intents {
                let Some(edit) = resolve(&tree, &embedding, intent) else { continue };
                let mut followed = embedding.clone();
                let Ok(delta) = GraphDelta::from_tree_edit(&graph, &mut followed, &edit) else { continue };
                script.push(edit.clone());
                match TreeEdit::from_graph_delta(&tree, &embedding, &delta) {
                    Ok(read) => {
                        let mut again = embedding.clone();
                        let lowered = GraphDelta::from_tree_edit(&graph, &mut again, &read)
                            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
                        prop_assert_eq!(&lowered, &delta);
                        prop_assert_eq!(&again, &followed);
                    }
                    Err(err) => {
                        prop_assert!(
                            outside_tree_edits(&delta),
                            "{:?} lowers to a delta with no tree edit: {}", edit, err
                        );
                        let is_not_tree_edit = matches!(err, EditError::NotTreeEdit { .. });
                        prop_assert!(is_not_tree_edit, "{:?}", err);
                    }
                }
                let mut reached = graph.clone();
                delta.apply(&mut reached).map_err(fail)?;
                edit.apply(&mut tree).map_err(fail)?;
                graph = reached;
                embedding = followed;
            }
            // The whole script as one sequence, whose delta may skip the ids
            // of arcs, nodes and fans that later steps remove again.
            let (start_tree, start_graph, start_embedding) = start;
            let whole_edit = TreeEdit::Sequence(script);
            let mut followed = start_embedding.clone();
            let whole = GraphDelta::from_tree_edit(&start_graph, &mut followed, &whole_edit)
                .map_err(|e| TestCaseError::fail(format!("{e}")))?;
            match TreeEdit::from_graph_delta(&start_tree, &start_embedding, &whole) {
                Ok(read) => {
                    let mut again = start_embedding.clone();
                    let lowered = GraphDelta::from_tree_edit(&start_graph, &mut again, &read)
                        .map_err(|e| TestCaseError::fail(format!("{e}")))?;
                    prop_assert_eq!(&lowered, &whole, "{:?} read back as {:?}", whole_edit, read);
                    prop_assert_eq!(&again.arcs, &followed.arcs);
                    prop_assert_eq!(&again.fans, &followed.fans);
                }
                Err(err) => {
                    prop_assert!(
                        outside_tree_edits(&whole),
                        "{:?} lowers to a delta with no tree edit: {}", whole_edit, err
                    );
                    let is_not_tree_edit = matches!(err, EditError::NotTreeEdit { .. });
                    prop_assert!(is_not_tree_edit, "{:?}", err);
                }
            }
        }
    }

    const PINNED_BYTES: &str = concat!(
        "0a000000000000000100000000060000000600000001000000000000007700000000000000000000",
        "00000000000000000000010000000000000073010000000000000077040000000000000070726f70",
        "00020100000003020000000403000000010000000000000076050400000001000000000000006602",
        "00000000000000800604000000010000000000000067070400000000000000010000000000000073",
        "010000000000000077040000000000000070726f700008030000000000000072656c000000000200",
        "00000000000001000000000000006101000000010000000000000062040000000903000000000000",
        "0072656c0a0200000002000000000000000300000001000000020000000100000000000000610000",
        "000000000000000000000000000000000000",
    );
    const PINNED_DIGEST: &str = "719aa916f820295b4b169d0bff86a0bbfddd45bc574dd951e0fcc375b73c20fc";
}
