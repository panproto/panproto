//! The recorded correspondence between a tree and its graph form.
//!
//! A tree names its arcs by `(parent, child)` and keeps its fans in a side
//! list; its graph form names every edge by an [`EdgeId`] and reifies every
//! fan as a node. A [`TreeEmbedding`] records which graph id each arc and
//! each fan received, so that the two forms can be translated into each other
//! exactly, and it is threaded through tree edits so that the ids stay
//! stable while the tree changes.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use super::EdgeId;
use super::reify::FanIds;
use crate::edit_error::EditError;
use crate::error::InstError;
use crate::fan::Fan;
use crate::segment::{IdAllocator, IdKind, IdRange};
use crate::tree_edit::TreeEdit;
use crate::view_ref::ViewRef;
use crate::wtype::WInstance;

/// The graph ids of one reified fan of a tree.
///
/// Serialized as a struct with named fields.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FanEmbedding {
    /// The fan node.
    pub node: u32,
    /// The edge from the fan node to the fan's parent.
    pub parent_edge: EdgeId,
    /// The edge from the fan node to each labelled child, by label.
    pub child_edges: BTreeMap<String, EdgeId>,
}

impl From<FanIds> for FanEmbedding {
    fn from(ids: FanIds) -> Self {
        Self {
            node: ids.node,
            parent_edge: ids.parent_edge,
            child_edges: ids.child_edges,
        }
    }
}

/// How a tree sits inside its graph form.
///
/// A tree node keeps its id in the graph. The arc into child `c` is the
/// graph edge `arcs[c]`; fan `k` of the tree (in `WInstance::fans` order) is
/// the fan node and edges `fans[k]`. `next_node` and `next_edge` are the
/// floors from which the embedding allocates the ids of new arcs, new fans
/// and relocated fan nodes; they never decrease. Serialized as a struct with
/// named fields.
///
/// The ids of an embedding are a presentation of the tree, like the ids of
/// synthesized objects: two embeddings of one tree may differ, and a tree's
/// own identity (its canonical bytes) never includes them. They matter where
/// a tree is compared with or translated into a graph: [`Self::map_ref`]
/// sends a tree's element references to the graph's.
///
/// ```
/// use std::collections::HashMap;
///
/// use panproto_inst::{EdgeId, Fan, GInstance, Node, ViewRef, WInstance};
/// use panproto_schema::Edge;
///
/// let arc = Edge { src: "s".into(), tgt: "w".into(), kind: "prop".into(), name: None };
/// let tree = WInstance::new(
///     HashMap::from([(0, Node::new(0, "s")), (1, Node::new(1, "w"))]),
///     vec![(0, 1, arc)],
///     vec![Fan::new("rel", 0).with_child("arg", 1)],
///     0,
///     "s".into(),
/// );
/// let (graph, embedding) = GInstance::from_wtype_embedded(&tree, None)?;
/// assert_eq!(embedding.arcs[&1], EdgeId(0));
/// assert_eq!(embedding.fans[0].node, 2);
/// assert_eq!((embedding.next_node, embedding.next_edge), (3, 3));
/// assert_eq!(
///     embedding.map_ref(&ViewRef::Arc { parent: 0, child: 1 }),
///     Some(ViewRef::Edge(EdgeId(0)))
/// );
/// assert_eq!(graph.edge_count(), 3);
/// # Ok::<(), panproto_inst::InstError>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TreeEmbedding {
    /// The graph edge of the arc into each child, by child node id.
    pub arcs: BTreeMap<u32, EdgeId>,
    /// The graph ids of each fan, in the tree's fan order.
    pub fans: Vec<FanEmbedding>,
    /// The least node id not yet used by the tree or its fan nodes.
    pub next_node: u32,
    /// The least edge id not yet used by the tree's arcs or fan edges.
    pub next_edge: u32,
}

/// An allocator resuming from a stored counter over the whole id space.
fn resume(floor: u32) -> IdAllocator {
    IdAllocator::from_floor(IdRange::FULL, floor)
}

/// Draw one id and return it with the counter that follows it. The id
/// `u32::MAX`, after which no counter can be stored, is never drawn.
fn draw(counter: u32, kind: IdKind) -> Result<(u32, u32), InstError> {
    let mut allocator = resume(counter);
    let id = allocator.allocate().map_err(|e| e.whole_state(kind))?;
    Ok((id, allocator.floor()))
}

impl TreeEmbedding {
    /// Translate a tree's element reference into its graph form's.
    ///
    /// `Node(n)` is `Node(n)`, since tree nodes keep their ids;
    /// `Arc { parent, child }` is `Edge(arcs[child])`. An `Edge` reference
    /// is already a graph reference and is returned unchanged. A table
    /// reference (`Row`, `Pair`), or an arc into a child the embedding does
    /// not record, has no image.
    #[must_use]
    pub fn map_ref(&self, reference: &ViewRef) -> Option<ViewRef> {
        match reference {
            ViewRef::Node(n) => Some(ViewRef::Node(*n)),
            ViewRef::Edge(e) => Some(ViewRef::Edge(*e)),
            ViewRef::Arc { child, .. } => self.arcs.get(child).map(|e| ViewRef::Edge(*e)),
            ViewRef::Row(_) | ViewRef::Pair { .. } => None,
        }
    }

    /// Allocate a fresh edge id from `next_edge`.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::EdgeIdsExhausted`] when no edge id below
    /// `u32::MAX` is left.
    pub fn allocate_edge(&mut self) -> Result<EdgeId, InstError> {
        let (id, next) = draw(self.next_edge, IdKind::Edge)?;
        self.next_edge = next;
        Ok(EdgeId(id))
    }

    /// Allocate a fresh node id from `next_node`.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::NodeIdsExhausted`] when no node id below
    /// `u32::MAX` is left.
    pub fn allocate_node(&mut self) -> Result<u32, InstError> {
        let (id, next) = draw(self.next_node, IdKind::Node)?;
        self.next_node = next;
        Ok(id)
    }

    /// Raise `next_node` past `id`, leaving it alone when it is already
    /// above. The counter saturates at `u32::MAX`: observing that id
    /// records the floor `u32::MAX`, against which allocation fails.
    pub fn observe_node(&mut self, id: u32) {
        let mut allocator = resume(self.next_node);
        allocator.observe(id);
        self.next_node = allocator.floor();
    }

    /// The ids of a new fan: the fan node from `next_node`, then the parent
    /// edge and the child edges, in ascending label order, from `next_edge`.
    fn allocate_fan(&mut self, fan: &Fan) -> Result<FanEmbedding, InstError> {
        let node = self.allocate_node()?;
        let parent_edge = self.allocate_edge()?;
        let mut labels: Vec<&String> = fan.children.keys().collect();
        labels.sort_unstable();
        let mut child_edges = BTreeMap::new();
        for label in labels {
            child_edges.insert(label.clone(), self.allocate_edge()?);
        }
        Ok(FanEmbedding {
            node,
            parent_edge,
            child_edges,
        })
    }

    /// Follow one tree edit step: `before` is the tree this embedding
    /// embeds, `after` the tree the step left.
    ///
    /// Every current tree edit removes arcs and fans and appends new ones, so
    /// the arcs of `after` are some arcs of `before`, in their old order,
    /// followed by the arcs the step appended. This keeps the edge id of an
    /// arc of `after` whose child had the same parent in `before`, as long as
    /// that id is above every id kept or given before it in `after`'s arc
    /// order; every other arc (one the step created or re-parented) gets a
    /// fresh id from `next_edge`, in `after`'s arc order. Ascending edge id is
    /// therefore always arc order.
    ///
    /// A fan of `after` that is the next unmatched fan of `before`, in order,
    /// keeps its ids; any other fan gets a fresh fan node and fresh edges. A
    /// fan node whose id a tree node of `after` now holds is relocated to a
    /// fresh node id and keeps its edge ids. `next_node` is first raised past
    /// every tree node id of `after`, then the relocated and new fans take
    /// their node ids in `after`'s fan order.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::NodeIdsExhausted`] or
    /// [`InstError::EdgeIdsExhausted`] when the counters run out, leaving the
    /// embedding as it was.
    pub fn observe_edit(&mut self, before: &WInstance, after: &WInstance) -> Result<(), InstError> {
        let saved = self.clone();
        let result = self.observe_edit_inner(before, after);
        if result.is_err() {
            *self = saved;
        }
        result
    }

    fn observe_edit_inner(
        &mut self,
        before: &WInstance,
        after: &WInstance,
    ) -> Result<(), InstError> {
        for &id in after.nodes.keys() {
            self.observe_node(id);
        }

        let old_parent: HashMap<u32, u32> = before.arcs.iter().map(|&(p, c, _)| (c, p)).collect();
        let mut arcs = BTreeMap::new();
        let mut last: Option<EdgeId> = None;
        for &(parent, child, _) in &after.arcs {
            let kept = self.arcs.get(&child).copied().filter(|id| {
                old_parent.get(&child) == Some(&parent) && last.is_none_or(|l| *id > l)
            });
            let id = match kept {
                Some(id) => id,
                None => self.allocate_edge()?,
            };
            arcs.insert(child, id);
            last = Some(id);
        }
        self.arcs = arcs;

        let old_fans = std::mem::take(&mut self.fans);
        let mut next_old = 0;
        let mut fans = Vec::with_capacity(after.fans.len());
        for fan in &after.fans {
            let matched =
                (next_old..before.fans.len().min(old_fans.len())).find(|&j| before.fans[j] == *fan);
            let embedding = match matched {
                Some(j) => {
                    next_old = j + 1;
                    let mut kept = old_fans[j].clone();
                    if after.nodes.contains_key(&kept.node) {
                        kept.node = self.allocate_node()?;
                    }
                    kept
                }
                None => self.allocate_fan(fan)?,
            };
            fans.push(embedding);
        }
        self.fans = fans;
        Ok(())
    }

    /// Apply a tree edit to `tree`, which this embedding embeds, and follow
    /// it step by step with [`Self::observe_edit`], so that fresh ids are
    /// allocated in the order [`TreeEdit::apply`] appends arcs and fans.
    ///
    /// # Errors
    ///
    /// Returns the step's [`EditError`] when a step does not apply, and
    /// [`EditError::ApplyFailed`] when the embedding runs out of ids. The
    /// steps before the failing one stay applied and the embedding follows
    /// them; the failing step leaves the tree as it was before that step.
    pub fn apply_edit(&mut self, tree: &mut WInstance, edit: &TreeEdit) -> Result<(), EditError> {
        match edit {
            TreeEdit::Identity => Ok(()),
            TreeEdit::Sequence(steps) => steps.iter().try_for_each(|s| self.apply_edit(tree, s)),
            step => {
                let before = tree.clone();
                let followed = step.apply(tree).and_then(|()| {
                    self.observe_edit(&before, tree)
                        .map_err(|e| EditError::ApplyFailed(e.to_string()))
                });
                if followed.is_err() {
                    *tree = before;
                }
                followed
            }
        }
    }
}
