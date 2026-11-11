//! Trees as graph instances.
//!
//! [`GInstance::from_wtype_embedded`] lays a tree out as a graph and records
//! the correspondence as a [`TreeEmbedding`]; the way back is in the
//! `tree_shape` module.

use panproto_schema::Schema;

use super::embedding::{FanEmbedding, TreeEmbedding};
use super::reify;
use super::{EdgeId, GEdge, GInstance};
use crate::NotTreeReason;
use crate::error::InstError;
use crate::segment::{IdAllocator, IdKind, IdRange};
use crate::wtype::WInstance;
use crate::wtype_check;

/// Insert a tree's nodes into an empty graph, checking that each is filed
/// under its own id, and refuse reserved vocabulary among them.
fn tree_nodes(tree: &WInstance) -> Result<GInstance, InstError> {
    let mut graph = GInstance::new();
    let mut ids: Vec<u32> = tree.nodes.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let node = &tree.nodes[&id];
        if node.id != id {
            return Err(InstError::NodeIdMismatch {
                key: id,
                node_id: node.id,
            });
        }
        graph.insert_node(node.clone())?;
    }
    Ok(graph)
}

/// Refuse a second arc into one child: the embedding records one arc per
/// child.
fn refuse_second_parent(
    embedding: &TreeEmbedding,
    child: u32,
    id: EdgeId,
) -> Result<(), InstError> {
    match embedding.arcs.get(&child) {
        Some(&first) => Err(InstError::NotATree {
            reason: NotTreeReason::MultipleParents {
                node: child,
                edges: vec![first, id],
            },
        }),
        None => Ok(()),
    }
}

impl GInstance {
    /// The graph form of a tree, with the embedding relating the two.
    ///
    /// Node ids are preserved. Arc `i`, in `WInstance::arcs` order, becomes
    /// the edge `EdgeId(i)` with no position, so the tree's child order is the
    /// edge order. Each fan is reified as a fan node (see
    /// [`reify_fans`](crate::reify_fans)): fan `k`, in `WInstance::fans` order
    /// and duplicates included, gets node id `max_node_id + 1 + k`, and its
    /// edges follow every arc edge, the parent edge first and then the
    /// children in ascending label order. The parent edge is named by the
    /// parent label `schema` declares for the fan's hyper-edge, and is
    /// unnamed without a schema or a declaration. Ids are drawn from one
    /// [`IdAllocator`] per kind over the whole id space, whose floors, after
    /// this initial allocation, are the embedding's `next_node` and
    /// `next_edge`: `next_node` is one above every tree node and fan node
    /// id, the floor of the graph.
    ///
    /// Reserved fan vocabulary is invalid in a tree, so for a valid tree this
    /// fails only when the ids run out.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::NodeIdMismatch`] when a node is filed under
    /// another id, [`InstError::DanglingEdge`] when an arc or a fan names a
    /// node the tree does not hold, [`InstError::NotATree`] with
    /// [`NotTreeReason::MultipleParents`] when two arcs enter one child,
    /// [`InstError::ReservedVocabulary`] when a node has the fan shape or an
    /// arc has a reserved fan kind, and [`InstError::NodeIdsExhausted`] or
    /// [`InstError::EdgeIdsExhausted`] when no id below `u32::MAX` is left
    /// for an arc, a fan node or a fan edge. A tree holding the node id
    /// `u32::MAX` embeds with `next_node` at `u32::MAX`.
    pub fn from_wtype_embedded(
        tree: &WInstance,
        schema: Option<&Schema>,
    ) -> Result<(Self, TreeEmbedding), InstError> {
        let mut graph = tree_nodes(tree)?;
        wtype_check::check_reserved_vocabulary(tree)?;
        let mut edges = IdAllocator::new(IdRange::FULL);
        let mut embedding = TreeEmbedding::default();
        for (src, tgt, edge) in &tree.arcs {
            let candidate = EdgeId(edges.floor());
            for endpoint in [*src, *tgt] {
                if !tree.nodes.contains_key(&endpoint) {
                    return Err(InstError::DanglingEdge {
                        edge: candidate,
                        endpoint,
                    });
                }
            }
            refuse_second_parent(&embedding, *tgt, candidate)?;
            let id = EdgeId(edges.allocate().map_err(|e| e.whole_state(IdKind::Edge))?);
            debug_assert_eq!(id, candidate);
            graph.insert_edge(id, GEdge::new(*src, *tgt, edge.clone()))?;
            embedding.arcs.insert(*tgt, id);
        }
        let mut nodes = IdAllocator::from_source(IdRange::FULL, tree.nodes.keys().copied());
        let (graph, fans) = reify::reify_into(graph, &tree.fans, schema, &mut nodes, &mut edges)?;
        embedding.fans = fans.into_iter().map(FanEmbedding::from).collect();
        embedding.next_node = nodes.floor();
        embedding.next_edge = edges.floor();
        Ok((graph, embedding))
    }

    /// The graph form of a tree: the first projection of
    /// [`Self::from_wtype_embedded`] without a schema.
    ///
    /// # Errors
    ///
    /// As [`Self::from_wtype_embedded`].
    pub fn from_wtype(tree: &WInstance) -> Result<Self, InstError> {
        Self::from_wtype_embedded(tree, None).map(|(graph, _)| graph)
    }

    /// [`Self::from_wtype`], naming each fan's parent edge by the parent label
    /// `schema` declares for its hyper-edge.
    ///
    /// # Errors
    ///
    /// As [`Self::from_wtype_embedded`].
    pub fn from_wtype_with_schema(tree: &WInstance, schema: &Schema) -> Result<Self, InstError> {
        Self::from_wtype_embedded(tree, Some(schema)).map(|(graph, _)| graph)
    }

    /// The graph form of a tree under a given embedding: arcs take the edge
    /// ids `embedding.arcs` records and fans the ids `embedding.fans`
    /// records, in `WInstance::fans` order.
    ///
    /// This is the graph a tree edit leads to when the embedding follows the
    /// edit (see [`TreeEmbedding::apply_edit`]), and for the embedding
    /// [`Self::from_wtype_embedded`] returns it is that function's graph.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::DanglingArc`] when an arc's child has no
    /// recorded edge, [`InstError::NotATree`] with
    /// [`NotTreeReason::FanNotRepresentable`] when the embedding records a
    /// different number of fans or a fan with other labels, and otherwise
    /// the errors of [`Self::from_wtype_embedded`], including
    /// [`InstError::DuplicateEdge`] or [`InstError::DuplicateNode`] when the
    /// embedding reuses an id.
    pub fn from_wtype_under(
        tree: &WInstance,
        embedding: &TreeEmbedding,
        schema: Option<&Schema>,
    ) -> Result<Self, InstError> {
        let mut graph = tree_nodes(tree)?;
        wtype_check::check_reserved_vocabulary(tree)?;
        for (src, tgt, edge) in &tree.arcs {
            let id = *embedding.arcs.get(tgt).ok_or(InstError::DanglingArc {
                src: *src,
                tgt: *tgt,
            })?;
            graph.insert_edge(id, GEdge::new(*src, *tgt, edge.clone()))?;
        }
        let unfit = |node: u32| InstError::NotATree {
            reason: NotTreeReason::FanNotRepresentable { node },
        };
        if embedding.fans.len() != tree.fans.len() {
            let node = embedding
                .fans
                .get(tree.fans.len())
                .map_or_else(|| embedding.next_node, |f| f.node);
            return Err(unfit(node));
        }
        let tree_ids = graph
            .nodes()
            .keys()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        for (fan, ids) in tree.fans.iter().zip(&embedding.fans) {
            let labels_match = fan.children.len() == ids.child_edges.len()
                && fan.children.keys().all(|l| ids.child_edges.contains_key(l));
            if !labels_match {
                return Err(unfit(ids.node));
            }
            let mut edge_ids =
                std::iter::once(ids.parent_edge).chain(ids.child_edges.values().copied());
            reify::insert_fan(
                &mut graph,
                fan,
                schema,
                ids.node,
                &mut || edge_ids.next().ok_or(InstError::EdgeIdsExhausted),
                &|target| tree_ids.contains(&target),
            )?;
        }
        Ok(graph)
    }
}
