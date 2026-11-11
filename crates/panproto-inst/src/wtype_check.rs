//! The validating constructor of [`WInstance`] and the reserved-vocabulary
//! predicate of trees.
//!
//! A tree carries its hyper-edges as [`Fan`] records beside its nodes and
//! arcs. Its graph form reifies each fan as a fan node of shape
//! [`NodeShape::HyperEdge`] with edges of the reserved kinds
//! [`HYPER_PARENT_KIND`] and [`HYPER_LABEL_KIND`]. A tree that already held
//! such a node or arc would be ambiguous in that graph form (the node would
//! read back as a fan), so this vocabulary is invalid in trees: with it
//! excluded, [`GInstance::from_wtype_embedded`](crate::GInstance::from_wtype_embedded)
//! is total over valid trees up to id exhaustion.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::Edge;

use crate::error::InstError;
use crate::fan::Fan;
use crate::ginstance::{HYPER_LABEL_KIND, HYPER_PARENT_KIND};
use crate::metadata::{Node, NodeShape};
use crate::view_ref::ViewRef;
use crate::wtype::WInstance;

/// Whether an edge kind is one of the reserved fan kinds.
#[must_use]
pub fn is_reserved_edge_kind(kind: &str) -> bool {
    kind == HYPER_PARENT_KIND || kind == HYPER_LABEL_KIND
}

/// Every place a tree holds reserved fan vocabulary, ascending in
/// [`ViewRef`] order.
///
/// That is each node of shape [`NodeShape::HyperEdge`]
/// (`ViewRef::Node`), then each arc whose edge kind is [`HYPER_PARENT_KIND`]
/// or [`HYPER_LABEL_KIND`] (`ViewRef::Arc`).
///
/// A tree has no fan nodes of its own (its fans are [`Fan`] records), so
/// every node of the fan shape is outside a fan. The result is empty exactly
/// when the tree is free of reserved vocabulary.
///
/// ```
/// use std::collections::HashMap;
///
/// use panproto_inst::{Node, NodeShape, ViewRef, WInstance, wtype_check};
///
/// let tree = WInstance::new(
///     HashMap::from([(0, Node::new(0, "doc").with_shape(NodeShape::HyperEdge))]),
///     vec![],
///     vec![],
///     0,
///     "doc".into(),
/// );
/// assert_eq!(wtype_check::reserved_vocabulary(&tree), vec![ViewRef::Node(0)]);
/// ```
#[must_use]
pub fn reserved_vocabulary(tree: &WInstance) -> Vec<ViewRef> {
    let mut found: Vec<ViewRef> = tree
        .nodes
        .iter()
        .filter(|(_, node)| node.shape == NodeShape::HyperEdge)
        .map(|(&id, _)| ViewRef::Node(id))
        .collect();
    found.extend(
        tree.arcs
            .iter()
            .filter(|(_, _, edge)| is_reserved_edge_kind(&edge.kind))
            .map(|&(parent, child, _)| ViewRef::Arc { parent, child }),
    );
    found.sort();
    found.dedup();
    found
}

/// Check that `tree` holds no reserved fan vocabulary.
///
/// # Errors
///
/// Returns [`InstError::ReservedVocabulary`] naming the least offending
/// element in [`ViewRef`] order (a node before any arc).
pub fn check_reserved_vocabulary(tree: &WInstance) -> Result<(), InstError> {
    reserved_vocabulary(tree)
        .into_iter()
        .next()
        .map_or(Ok(()), |at| Err(InstError::ReservedVocabulary { at }))
}

impl WInstance {
    /// Build a W-type instance as [`WInstance::new`] does, rejecting input
    /// that is not a valid tree of this representation.
    ///
    /// Checked, in this order: every node is filed under its own id; the
    /// root is among the nodes; every arc joins two nodes of the instance;
    /// and the tree holds no reserved fan vocabulary
    /// ([`reserved_vocabulary`]).
    ///
    /// ```
    /// use std::collections::HashMap;
    ///
    /// use panproto_inst::{InstError, Node, ViewRef, WInstance};
    /// use panproto_schema::Edge;
    ///
    /// let edge = Edge { src: "doc".into(), tgt: "p".into(), kind: "hyper_parent".into(), name: None };
    /// let refused = WInstance::try_new(
    ///     HashMap::from([(0, Node::new(0, "doc")), (1, Node::new(1, "p"))]),
    ///     vec![(0, 1, edge)],
    ///     vec![],
    ///     0,
    ///     "doc".into(),
    /// );
    /// assert!(matches!(
    ///     refused,
    ///     Err(InstError::ReservedVocabulary { at: ViewRef::Arc { parent: 0, child: 1 } })
    /// ));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`InstError::NodeIdMismatch`] for a node filed under another
    /// id, [`InstError::MissingRoot`] when `root` is not a node,
    /// [`InstError::DanglingArc`] for an arc naming an absent node, and
    /// [`InstError::ReservedVocabulary`] for reserved fan vocabulary.
    pub fn try_new(
        nodes: HashMap<u32, Node>,
        arcs: Vec<(u32, u32, Edge)>,
        fans: Vec<Fan>,
        root: u32,
        schema_root: Name,
    ) -> Result<Self, InstError> {
        let mut ids: Vec<u32> = nodes.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let node_id = nodes[&id].id;
            if node_id != id {
                return Err(InstError::NodeIdMismatch { key: id, node_id });
            }
        }
        if !nodes.contains_key(&root) {
            return Err(InstError::MissingRoot);
        }
        if let Some(&(src, tgt, _)) = arcs
            .iter()
            .find(|(src, tgt, _)| !nodes.contains_key(src) || !nodes.contains_key(tgt))
        {
            return Err(InstError::DanglingArc { src, tgt });
        }
        let tree = Self::new(nodes, arcs, fans, root, schema_root);
        check_reserved_vocabulary(&tree)?;
        Ok(tree)
    }
}
