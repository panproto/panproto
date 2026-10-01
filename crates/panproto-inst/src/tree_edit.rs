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

/// A tree edit step that allocates ids, with the first edge id it takes.
struct Allocating {
    first_edge: EdgeId,
    step: TreeEdit,
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
            step: TreeEdit::InsertFan { fan },
        });
    }
    Ok(allocating)
}

/// The `DeleteFan` steps of the fans a delta deletes that no deleted tree
/// node names (those go with the node).
fn fan_delete_steps(
    tree: &WInstance,
    embedding: &TreeEmbedding,
    delta: &GraphDelta,
    deleted: &BTreeSet<u32>,
) -> Vec<TreeEdit> {
    let mut steps = Vec::new();
    let mut deleted_fans = BTreeSet::new();
    for (k, fan) in embedding.fans.iter().enumerate() {
        let gone = delta.del_nodes().contains_key(&fan.node)
            && !delta.add_edges().contains_key(&fan.parent_edge);
        let cascaded = tree.fans.get(k).is_some_and(|f| {
            deleted.contains(&f.parent) || f.children.values().any(|c| deleted.contains(c))
        });
        if gone
            && !cascaded
            && let Some(f) = tree.fans.get(k)
            && deleted_fans.insert(f.hyper_edge_id.clone())
        {
            steps.push(TreeEdit::DeleteFan {
                hyper_edge_id: Name::from(f.hyper_edge_id.as_str()),
            });
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
    /// deleted node `DeleteNode`, a new fan `InsertFan` and a deleted fan not
    /// named by a deleted node `DeleteFan`. The steps run in that order, the
    /// arc-adding and fan-adding steps by ascending first new edge id (the
    /// order in which their ids are allocated) and the deletions children
    /// first. The edit is returned only when it lowers back to `delta`
    /// exactly.
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
    /// from the delta does not lower back to it (ids not allocated as tree
    /// edits allocate them, or payloads a tree edit does not produce).
    /// Returns [`EditError::ApplyFailed`] when `tree` has no graph form
    /// under `embedding`.
    pub fn from_graph_delta(
        tree: &WInstance,
        embedding: &TreeEmbedding,
        delta: &GraphDelta,
    ) -> Result<Self, EditError> {
        let graph =
            GInstance::from_wtype_under(tree, embedding, None).map_err(|e| inst_failure(&e))?;
        let mut steps = attr_steps(delta)?;

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
        steps.extend(fan_delete_steps(tree, embedding, delta, &nodes.deleted));

        allocating.sort_by_key(|a| a.first_edge);
        // A tree node replaced under its own id is deleted before the step
        // that inserts its successor.
        let mut deletions: Vec<u32> = nodes.deleted.iter().copied().collect();
        deletions.sort_by_key(|id| (std::cmp::Reverse(depth(tree, *id)), *id));
        for Allocating { step, .. } in allocating {
            if let Self::InsertNode { child_id, .. } = &step
                && nodes.deleted.contains(child_id)
            {
                let id = *child_id;
                deletions.retain(|d| *d != id);
                steps.push(Self::DeleteNode { id });
            }
            steps.push(step);
        }
        steps.extend(deletions.into_iter().map(|id| Self::DeleteNode { id }));

        let edit = match steps.len() {
            0 => Self::Identity,
            1 => steps.remove(0),
            _ => Self::Sequence(steps),
        };
        let mut followed = embedding.clone();
        let lowered = GraphDelta::from_tree_edit(&graph, &mut followed, &edit).map_err(|err| {
            not_tree_edit(format!("the tree edit it reads as does not apply: {err}"))
        })?;
        if lowered != *delta {
            return Err(not_tree_edit(
                "the tree edit it reads as gives another delta: its ids or payloads are not those tree edits produce"
                    .to_owned(),
            ));
        }
        Ok(edit)
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod graph_tests {
    use std::collections::{BTreeMap, HashMap};

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
    fn lower(tree: &WInstance, edit: &TreeEdit) -> (GraphDelta, TreeEmbedding, GInstance) {
        let (graph, mut embedding) = GInstance::from_wtype_embedded(tree, None).unwrap();
        let mut followed = embedding.clone();
        let mut edited = tree.clone();
        followed.apply_edit(&mut edited, edit).unwrap();
        let delta = GraphDelta::from_tree_edit(&graph, &mut embedding, edit).unwrap();
        assert_eq!(embedding, followed);
        let mut reached = graph;
        delta.apply(&mut reached).unwrap();
        let expected = GInstance::from_wtype_under(&edited, &followed, None).unwrap();
        assert!(GInstance::identical(&reached, &expected));
        (delta, embedding, reached)
    }

    fn ids<K: Copy, V>(map: &BTreeMap<K, V>) -> Vec<K> {
        map.keys().copied().collect()
    }

    #[test]
    fn from_tree_edit_cases() {
        let tree = sentence();
        insert_and_delete_cases(&tree);
        contract_case();
        relabel_and_field_cases(&tree);
        move_and_fan_cases(&tree);
        sequence_case(&tree);
    }

    fn insert_and_delete_cases(tree: &WInstance) {
        // InsertNode: the node and an arc under the next edge id.
        let (d, emb, _) = lower(
            tree,
            &TreeEdit::InsertNode {
                parent: 1,
                child_id: 6,
                node: Node::new(6, "m"),
                edge: edge("w", "m"),
            },
        );
        assert_eq!(ids(d.add_nodes()), vec![6]);
        assert_eq!(d.add_edges()[&EdgeId(5)], GEdge::new(1, 6, edge("w", "m")));
        assert!(d.del_nodes().is_empty() && d.del_edges().is_empty() && d.attrs().is_empty());
        assert_eq!(emb.arcs[&6], EdgeId(5));

        // DeleteNode: the node, its in-arc and the fan naming it.
        let (d, emb, _) = lower(tree, &TreeEdit::DeleteNode { id: 4 });
        assert_eq!(ids(d.del_nodes()), vec![4, 5]);
        assert_eq!(
            ids(d.del_edges()),
            vec![EdgeId(0), EdgeId(2), EdgeId(3), EdgeId(4)]
        );
        assert!(d.add_nodes().is_empty() && d.add_edges().is_empty());
        assert!(emb.fans.is_empty());
    }

    fn contract_case() {
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
        let (d, _, _) = lower(&chain, &TreeEdit::ContractNode { id: 1 });
        assert_eq!(ids(d.del_nodes()), vec![1]);
        assert_eq!(ids(d.del_edges()), vec![EdgeId(0), EdgeId(1), EdgeId(2)]);
        assert_eq!(d.add_edges()[&EdgeId(3)], GEdge::new(0, 2, edge("r", "b")));
        assert_eq!(d.add_edges()[&EdgeId(4)], GEdge::new(0, 3, edge("r", "c")));
    }

    fn relabel_and_field_cases(tree: &WInstance) {
        // RelabelNode: an `Anchor` change, and the label change of the fan
        // edge reaching the node (it names its target's anchor).
        let (d, _, _) = lower(
            tree,
            &TreeEdit::RelabelNode {
                id: 1,
                new_anchor: "v".into(),
            },
        );
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
        let (d, _, _) = lower(tree, &set);
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
        set.apply(&mut with_field).unwrap();
        let (d, _, _) = lower(
            &with_field,
            &TreeEdit::RemoveField {
                node_id: 4,
                field: "n".into(),
            },
        );
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
    }

    fn move_and_fan_cases(tree: &WInstance) {
        // MoveSubtree: the old arc deleted, the new one under a fresh id.
        let (d, _, _) = lower(
            tree,
            &TreeEdit::MoveSubtree {
                node_id: 4,
                new_parent: 1,
                edge: edge("w", "w"),
            },
        );
        assert_eq!(ids(d.del_edges()), vec![EdgeId(0)]);
        assert_eq!(d.add_edges()[&EdgeId(5)], GEdge::new(1, 4, edge("w", "w")));
        assert_eq!(d.add_edges().len(), 1);

        // InsertFan and DeleteFan: the fan node with its edges.
        let (d, _, _) = lower(
            tree,
            &TreeEdit::InsertFan {
                fan: Fan::new("arg", 1).with_child("x", 4),
            },
        );
        assert_eq!(ids(d.add_nodes()), vec![6]);
        assert_eq!(ids(d.add_edges()), vec![EdgeId(5), EdgeId(6)]);
        let (d, _, _) = lower(
            tree,
            &TreeEdit::DeleteFan {
                hyper_edge_id: "rel".into(),
            },
        );
        assert_eq!(ids(d.del_nodes()), vec![5]);
        assert_eq!(ids(d.del_edges()), vec![EdgeId(2), EdgeId(3), EdgeId(4)]);
    }

    fn sequence_case(tree: &WInstance) {
        // Sequence: the composite of the steps' deltas.
        let insert = TreeEdit::InsertNode {
            parent: 0,
            child_id: 9,
            node: Node::new(9, "w"),
            edge: edge("s", "w"),
        };
        let seq = TreeEdit::Sequence(vec![insert.clone(), TreeEdit::DeleteNode { id: 9 }]);
        let (d, emb, _) = lower(tree, &seq);
        assert!(d.is_identity(), "an add then a delete of one node cancels");
        assert_eq!(emb.next_edge, 6, "the cancelled arc still took an id");
        let (first, mid, _) = lower(tree, &insert);
        let mut mid_tree = tree.clone();
        insert.apply(&mut mid_tree).unwrap();
        let mid_graph = GInstance::from_wtype_under(&mid_tree, &mid, None).unwrap();
        let mut follow = mid;
        let second =
            GraphDelta::from_tree_edit(&mid_graph, &mut follow, &TreeEdit::DeleteNode { id: 9 })
                .unwrap();
        assert_eq!(first.compose(&second).unwrap(), d);
    }

    #[test]
    fn from_tree_edit_joins_features_like_tree_apply() {
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
        );
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
    }

    #[test]
    fn from_tree_edit_allocates_ids_in_apply_order() {
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
        let (d, emb, _) = lower(&tree, &edit);
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
    }

    #[test]
    fn from_graph_delta_rejections() {
        let tree = sentence();
        let (graph, emb) = GInstance::from_wtype_embedded(&tree, None).unwrap();
        let reject = |delta: &GraphDelta, needle: &str| {
            let err = TreeEdit::from_graph_delta(&tree, &emb, delta).unwrap_err();
            let EditError::NotTreeEdit { reason } = &err else {
                panic!("expected NotTreeEdit, got {err:?}");
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
            .with_deleted_edge(EdgeId(1), graph.edge(EdgeId(1)).unwrap().clone());
        reject(&low, "is not above the existing arc");
        // A deleted node that keeps a child.
        let not_leaf = GraphDelta::identity().with_deleted_node(graph.node(0).unwrap().clone());
        reject(&not_leaf, "keeps its child");
        // A node that gains a parent without losing its own.
        let second_parent =
            GraphDelta::identity().with_added_edge(EdgeId(7), GEdge::new(1, 4, edge("w", "w")));
        reject(&second_parent, "without losing its own");
        // A node that loses its parent without gaining one.
        let orphan = GraphDelta::identity()
            .with_deleted_edge(EdgeId(0), graph.edge(EdgeId(0)).unwrap().clone());
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
            let delta = GraphDelta::identity().with_change(key, old, new).unwrap();
            reject(&delta, "a tree edit has no change of");
        }
        // An insertion under an id the tree edit would not allocate.
        let far = GraphDelta::identity()
            .with_added_node(Node::new(6, "w"))
            .with_added_edge(EdgeId(40), GEdge::new(0, 6, edge("s", "w")));
        reject(&far, "gives another delta");
        // The same insertion under the next edge id is a tree edit.
        let next = GraphDelta::identity()
            .with_added_node(Node::new(6, "w"))
            .with_added_edge(EdgeId(5), GEdge::new(0, 6, edge("s", "w")));
        assert!(matches!(
            TreeEdit::from_graph_delta(&tree, &emb, &next),
            Ok(TreeEdit::InsertNode { child_id: 6, .. })
        ));
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
    fn tree_edit_bytes_are_pinned() {
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
        let hex = writer
            .as_bytes()
            .iter()
            .fold(String::new(), |mut hex, byte| {
                use std::fmt::Write as _;
                write!(hex, "{byte:02x}").unwrap();
                hex
            });
        assert_eq!(hex, PINNED_BYTES);
        assert_eq!(edit.digest().to_hex(), PINNED_DIGEST);
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
                let id = |i: usize| u32::try_from(i * 3).unwrap();
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
            (0u8..4, 0u32..20, 0u32..20).prop_map(|(k, a, b)| Intent::Raw(k, a, b)),
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
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn from_tree_edit_commutes_with_embedding(
            tree in arb_tree(),
            intents in prop::collection::vec(intent(), 1..10),
        ) {
            let (mut graph, mut embedding) = GInstance::from_wtype_embedded(&tree, None).unwrap();
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
                        let expected = GInstance::from_wtype_under(&edited, &followed, None).unwrap();
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
            let (mut graph, mut embedding) = GInstance::from_wtype_embedded(&tree, None).unwrap();
            let mut tree = tree;
            for intent in &intents {
                let Some(edit) = resolve(&tree, &embedding, intent) else { continue };
                let mut followed = embedding.clone();
                let Ok(delta) = GraphDelta::from_tree_edit(&graph, &mut followed, &edit) else { continue };
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
                delta.apply(&mut reached).unwrap();
                edit.apply(&mut tree).unwrap();
                graph = reached;
                embedding = followed;
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
