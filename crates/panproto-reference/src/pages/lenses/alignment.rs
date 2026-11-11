//! Structural alignment of a tree view read from a serialization that keeps
//! no node ids.

use std::collections::BTreeMap;

use crate::model::{Name, SEdge, Tree};
use crate::restrict::{Complement, floor_of};

/// Definition: a view position.
///
/// The root of a view, by its anchor, or the child at `index` among the
/// children of `parent` on `edge`, counted from 0 in the view's arc order.
///
/// Book anchors: `L-lenses-11`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ViewPosition {
    /// The view's root.
    Root {
        /// The root's anchor.
        anchor: Name,
    },
    /// A child of `parent` on `edge`.
    Child {
        /// The parent's view id.
        parent: u32,
        /// The schema edge.
        edge: SEdge,
        /// The index among the parent's children on `edge`.
        index: u32,
    },
}

/// Definition: the view positions of a tree.
///
/// The root is at its anchor; every node an arc path from the root reaches
/// is at its parent, edge and index among the parent's children on that
/// edge, in arc order. Unreached nodes have no position.
///
/// Book anchors: `L-lenses-11`.
#[must_use]
pub fn tree_view_positions(view: &Tree) -> BTreeMap<u32, ViewPosition> {
    let mut positions = BTreeMap::new();
    let Some(root) = view.nodes.get(&view.root) else {
        return positions;
    };
    positions.insert(
        view.root,
        ViewPosition::Root {
            anchor: root.anchor.clone(),
        },
    );
    let mut frontier = vec![view.root];
    while let Some(parent) = frontier.pop() {
        let mut counts: BTreeMap<&SEdge, u32> = BTreeMap::new();
        for (_, child, edge) in view.arcs.iter().filter(|(p, _, _)| *p == parent) {
            let count = counts.entry(edge).or_insert(0);
            let index = *count;
            *count += 1;
            if positions.contains_key(child) {
                continue;
            }
            positions.insert(
                *child,
                ViewPosition::Child {
                    parent,
                    edge: edge.clone(),
                    index,
                },
            );
            frontier.push(*child);
        }
    }
    positions
}

/// Why a view cannot be aligned to recorded positions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewAlignmentRefusal {
    /// The view has a node and no positions are recorded.
    PositionsNotRecorded,
    /// The recorded positions name no single root, or one position twice.
    MalformedPositions,
    /// The view's root anchor is not the recorded root's.
    RootAnchorMismatch {
        /// The recorded anchor.
        recorded: Name,
        /// The view's anchor.
        found: Name,
    },
    /// No fresh node id is left.
    IdsExhausted,
}

/// Definition: aligning a tree view to recorded positions.
///
/// Walk `view` breadth first from its root, children in arc order. The root
/// takes the recorded root's id. A child takes the id recorded at its
/// position (its parent's new id, its edge, its index among the parent's
/// children on that edge) when its parent took a recorded id and that
/// position is recorded; otherwise it is fresh. Fresh nodes, then the nodes
/// the walk does not reach in ascending id order, take the ids
/// `f, f + 1, ...` where `f` is the greatest of the complement's node floor
/// and the floor of every recorded, dropped and synthesized node id. Arcs
/// and fans are renamed in place.
///
/// # Errors
///
/// The refusals of [`ViewAlignmentRefusal`], in its declaration order of
/// checks: no positions for a nonempty view, malformed positions, a root
/// anchor mismatch, and exhausted ids.
///
/// Book anchors: `L-lenses-11`.
pub fn align_tree_view(
    view: &Tree,
    positions: &BTreeMap<u32, ViewPosition>,
    complement: &Complement,
) -> Result<Tree, ViewAlignmentRefusal> {
    let Some(view_root) = view.nodes.get(&view.root) else {
        return Ok(view.clone());
    };
    if positions.is_empty() {
        return Err(ViewAlignmentRefusal::PositionsNotRecorded);
    }
    let roots: Vec<(u32, &Name)> = positions
        .iter()
        .filter_map(|(id, position)| match position {
            ViewPosition::Root { anchor } => Some((*id, anchor)),
            ViewPosition::Child { .. } => None,
        })
        .collect();
    let [(recorded_root, recorded_anchor)] = roots.as_slice() else {
        return Err(ViewAlignmentRefusal::MalformedPositions);
    };
    let mut at: BTreeMap<(u32, &SEdge, u32), u32> = BTreeMap::new();
    for (id, position) in positions {
        if let ViewPosition::Child {
            parent,
            edge,
            index,
        } = position
            && at.insert((*parent, edge, *index), *id).is_some()
        {
            return Err(ViewAlignmentRefusal::MalformedPositions);
        }
    }
    if **recorded_anchor != view_root.anchor {
        return Err(ViewAlignmentRefusal::RootAnchorMismatch {
            recorded: (*recorded_anchor).clone(),
            found: view_root.anchor.clone(),
        });
    }

    let (order, recorded) = walk(view, *recorded_root, &at);
    let mut fresh = complement.floor.0.max(floor_of(
        positions
            .keys()
            .chain(complement.dropped_nodes.keys())
            .chain(complement.synthesized_nodes.keys())
            .copied(),
    ));
    let mut rename: BTreeMap<u32, u32> = BTreeMap::new();
    for id in order {
        let new_id = if let Some(id) = recorded[&id] {
            id
        } else {
            if fresh == u32::MAX {
                return Err(ViewAlignmentRefusal::IdsExhausted);
            }
            fresh += 1;
            fresh - 1
        };
        rename.insert(id, new_id);
    }
    Ok(renamed_tree(view, &rename))
}

/// The breadth-first walk of `view` from its root, children in arc order,
/// then the unreached nodes in id order, with the recorded id each takes
/// (`None` for a fresh one): the root takes `root`, and a child the id
/// `at` records under its parent's recorded id, its edge and its index.
fn walk(
    view: &Tree,
    root: u32,
    at: &BTreeMap<(u32, &SEdge, u32), u32>,
) -> (Vec<u32>, BTreeMap<u32, Option<u32>>) {
    let mut order: Vec<u32> = vec![view.root];
    let mut recorded: BTreeMap<u32, Option<u32>> = BTreeMap::from([(view.root, Some(root))]);
    let mut next = 0;
    while next < order.len() {
        let parent = order[next];
        next += 1;
        let mut counts: BTreeMap<&SEdge, u32> = BTreeMap::new();
        for (_, child, edge) in view.arcs.iter().filter(|(p, _, _)| *p == parent) {
            if recorded.contains_key(child) || !view.nodes.contains_key(child) {
                continue;
            }
            let count = counts.entry(edge).or_insert(0);
            let index = *count;
            *count += 1;
            let id = recorded[&parent].and_then(|p| at.get(&(p, edge, index)).copied());
            recorded.insert(*child, id);
            order.push(*child);
        }
    }
    let unreached: Vec<u32> = view
        .nodes
        .keys()
        .copied()
        .filter(|id| !recorded.contains_key(id))
        .collect();
    for id in unreached {
        recorded.insert(id, None);
        order.push(id);
    }
    (order, recorded)
}

/// `view` with every node id renamed by `rename`, in its nodes, arcs, fans
/// and root; an id `rename` does not name is kept.
fn renamed_tree(view: &Tree, rename: &BTreeMap<u32, u32>) -> Tree {
    let renamed = |id: u32| rename.get(&id).copied().unwrap_or(id);
    Tree {
        nodes: view
            .nodes
            .iter()
            .map(|(id, node)| {
                let mut node = node.clone();
                node.id = renamed(*id);
                (node.id, node)
            })
            .collect(),
        arcs: view
            .arcs
            .iter()
            .map(|(p, c, e)| (renamed(*p), renamed(*c), e.clone()))
            .collect(),
        fans: view
            .fans
            .iter()
            .map(|fan| {
                let mut fan = fan.clone();
                fan.parent = renamed(fan.parent);
                for child in fan.children.values_mut() {
                    *child = renamed(*child);
                }
                fan
            })
            .collect(),
        root: renamed(view.root),
    }
}

/// The positions `positions` with every node id, key and parent alike,
/// renamed by `nodes`; an id `nodes` does not name is kept.
#[must_use]
pub(super) fn reindexed_positions(
    positions: &BTreeMap<u32, ViewPosition>,
    nodes: &BTreeMap<u32, u32>,
) -> BTreeMap<u32, ViewPosition> {
    let renamed = |id: u32| nodes.get(&id).copied().unwrap_or(id);
    positions
        .iter()
        .map(|(id, position)| {
            let position = match position {
                ViewPosition::Root { anchor } => ViewPosition::Root {
                    anchor: anchor.clone(),
                },
                ViewPosition::Child {
                    parent,
                    edge,
                    index,
                } => ViewPosition::Child {
                    parent: renamed(*parent),
                    edge: edge.clone(),
                    index: *index,
                },
            };
            (renamed(*id), position)
        })
        .collect()
}
