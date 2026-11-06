//! Structural alignment of a serialized tree view to its complement.
//!
//! A tree complement keys its residue by the node ids `get` gave the view.
//! A serialized view (JSON, or any format without node ids) loses those
//! ids, and a reader such as [`panproto_inst::parse_json`] numbers the
//! nodes afresh. Put against the complement then reads a residue entry as
//! belonging to whatever node now holds its id: a dropped child restored
//! under the wrong parent, or not at all, while put reports success.
//!
//! [`align_view_to_complement`] gives each node of such a view back the id
//! `get` gave the node at the same structural position, as
//! [`Complement::view_positions`] records it, and gives every node at a
//! position `get` did not produce a fresh id. Core [`put`](crate::put)
//! takes an id-aligned view and is unchanged; a serialized put entry point
//! parses, aligns, folds computed fields
//! ([`fold_computed_children_into_fields`](crate::fold_computed_children_into_fields))
//! and then puts.

use std::collections::{BTreeMap, HashMap};

use panproto_gat::Name;
use panproto_inst::identity::{ComplementShape, IdentityPolicy};
use panproto_inst::{Complement, IdAllocator, IdRange, ViewPosition, WInstance};
use panproto_schema::Edge;

use crate::Lens;
use crate::error::LensError;

/// Why a view cannot be aligned to a complement's recorded positions.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ViewAlignmentError {
    /// The view holds nodes, but the complement records no view positions:
    /// it was written before positions were recorded. Put such a complement
    /// only with the view `get` returned, ids and all.
    #[error("the complement records no view positions, so the view cannot be aligned to it")]
    PositionsNotRecorded,

    /// The complement records no root, more than one root, or two nodes at
    /// one position, so it names no single node for some position.
    #[error("the complement's view positions are malformed: {detail}")]
    MalformedPositions {
        /// What is wrong with them.
        detail: String,
    },

    /// The view's root is anchored at a vertex other than the one the
    /// complement records for the root of the view `get` returned.
    #[error("the view's root is anchored at `{found}`, but get produced a root at `{recorded}`")]
    RootAnchorMismatch {
        /// The root anchor the complement records.
        recorded: Name,
        /// The root anchor of the view.
        found: Name,
    },

    /// No node id at or above the complement's floor is left for a node
    /// the view adds.
    #[error("no node id above the complement's floor is left for a node the view adds")]
    IdsExhausted,
}

/// `view`, a tree view read from a serialization that does not keep node
/// ids, with each node given the id `get` gave the node at the same
/// structural position.
///
/// A node's position is its parent, its edge from the parent, and its index
/// among the parent's children on that edge in the view's arc order; the
/// root's is the root ([`ViewPosition`]). Walking `view` from its root, a
/// node takes the id [`Complement::view_positions`] records at its position
/// when its parent took the recorded id of its parent; every other node
/// (an insertion, or a node under one) takes a fresh id. Fresh ids follow
/// the id-floor rule of
/// [`fresh_view_node_id`](panproto_inst::complement::fresh_view_node_id):
/// they start at or above [`Complement::id_floor`] and above every recorded
/// view, dropped and synthesized id, and are handed out in ascending order
/// as the walk meets the nodes, children in arc order. A node no arc
/// reaches from the root takes a fresh id after them, in ascending order of
/// its id in `view`. A recorded node whose position the view does not
/// fill is absent from the result, which `put` reads as its deletion.
///
/// Arcs keep their order and edges, fans their hyper-edge and labels;
/// every node id in them, and each node's own `id`, is renamed. A view
/// `get` returned, renumbered in any way that keeps its arcs' order, aligns
/// back to exactly that view.
///
/// `complement` must be bound to `lens` as [`put`](crate::put) requires,
/// since positions recorded by another lens's `get` say nothing about this
/// view.
///
/// # Errors
///
/// Returns [`LensError::Identity`] when `complement` is not bound to
/// `lens` under [`IdentityPolicy::AllowLegacy`], the policy of
/// [`put`](crate::put). Returns [`LensError::ViewAlignment`] with
///
/// * [`ViewAlignmentError::PositionsNotRecorded`] when `view` holds a node
///   and `complement` records no positions;
/// * [`ViewAlignmentError::MalformedPositions`] when the recorded positions
///   name no single root, or two nodes share one position;
/// * [`ViewAlignmentError::RootAnchorMismatch`] when `view`'s root is
///   anchored elsewhere than the recorded root; and
/// * [`ViewAlignmentError::IdsExhausted`] when fresh ids run out.
pub fn align_view_to_complement(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
) -> Result<WInstance, LensError> {
    crate::identity::verify_with_identity(
        complement,
        lens,
        &lens.identity(ComplementShape::WType),
        IdentityPolicy::AllowLegacy,
    )?;
    align(view, complement).map_err(LensError::ViewAlignment)
}

fn align(view: &WInstance, complement: &Complement) -> Result<WInstance, ViewAlignmentError> {
    let Some(view_root) = view.nodes.get(&view.root) else {
        return Ok(view.clone());
    };
    let positions = &complement.view_positions;
    if positions.is_empty() {
        return Err(ViewAlignmentError::PositionsNotRecorded);
    }
    let (recorded_root, recorded_anchor, at) = index_positions(positions)?;
    if *recorded_anchor != view_root.anchor {
        return Err(ViewAlignmentError::RootAnchorMismatch {
            recorded: recorded_anchor.clone(),
            found: view_root.anchor.clone(),
        });
    }

    let mut children: HashMap<u32, Vec<(u32, &Edge)>> = HashMap::new();
    for (parent, child, edge) in &view.arcs {
        children.entry(*parent).or_default().push((*child, edge));
    }

    // Walk from the root, children in arc order. `None` marks a node that
    // takes a fresh id; the walk order is the order fresh ids are handed out.
    let mut assigned: HashMap<u32, Option<u32>> = HashMap::with_capacity(view.nodes.len());
    let mut walk: Vec<u32> = Vec::with_capacity(view.nodes.len());
    assigned.insert(view.root, Some(recorded_root));
    walk.push(view.root);
    let mut next = 0;
    while let Some(&parent) = walk.get(next) {
        next += 1;
        let parent_id = assigned.get(&parent).copied().flatten();
        let mut counts: HashMap<&Edge, u32> = HashMap::new();
        for &(child, edge) in children.get(&parent).into_iter().flatten() {
            if assigned.contains_key(&child) || !view.nodes.contains_key(&child) {
                continue;
            }
            let count = counts.entry(edge).or_insert(0);
            let index = *count;
            *count = count.saturating_add(1);
            let recorded =
                parent_id.and_then(|parent_id| at.get(&(parent_id, edge, index)).copied());
            assigned.insert(child, recorded);
            walk.push(child);
        }
    }
    let mut unreached: Vec<u32> = view
        .nodes
        .keys()
        .copied()
        .filter(|id| !assigned.contains_key(id))
        .collect();
    unreached.sort_unstable();
    walk.extend(unreached.iter().copied());
    for id in unreached {
        assigned.insert(id, None);
    }

    let mut ids = fresh_ids(complement);
    let mut rename: HashMap<u32, u32> = HashMap::with_capacity(walk.len());
    for id in walk {
        let new_id = match assigned.get(&id).copied().flatten() {
            Some(recorded) => recorded,
            None => ids
                .allocate()
                .map_err(|_| ViewAlignmentError::IdsExhausted)?,
        };
        rename.insert(id, new_id);
    }

    let renamed = |id: u32| rename.get(&id).copied().unwrap_or(id);
    let nodes = view
        .nodes
        .iter()
        .map(|(&id, node)| {
            let mut node = node.clone();
            node.id = renamed(id);
            (node.id, node)
        })
        .collect();
    let arcs = view
        .arcs
        .iter()
        .map(|(parent, child, edge)| (renamed(*parent), renamed(*child), edge.clone()))
        .collect();
    let fans = view
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
        .collect();
    Ok(WInstance::new(
        nodes,
        arcs,
        fans,
        renamed(view.root),
        view.schema_root.clone(),
    ))
}

/// A recorded position's key: parent id, edge and index.
type PositionKey<'a> = (u32, &'a Edge, u32);

/// The recorded root, its anchor, and the node at every recorded child
/// position.
fn index_positions(
    positions: &BTreeMap<u32, ViewPosition>,
) -> Result<(u32, &Name, HashMap<PositionKey<'_>, u32>), ViewAlignmentError> {
    let mut root = None;
    let mut at = HashMap::with_capacity(positions.len());
    for (&id, position) in positions {
        match position {
            ViewPosition::Root { anchor } => {
                if let Some((other, _)) = root.replace((id, anchor)) {
                    return Err(ViewAlignmentError::MalformedPositions {
                        detail: format!("nodes {other} and {id} are both recorded as the root"),
                    });
                }
            }
            ViewPosition::Child {
                parent,
                edge,
                index,
            } => {
                if let Some(other) = at.insert((*parent, edge, *index), id) {
                    return Err(ViewAlignmentError::MalformedPositions {
                        detail: format!(
                            "nodes {other} and {id} are both recorded at index {index} under {parent}"
                        ),
                    });
                }
            }
        }
    }
    let (root, anchor) = root.ok_or_else(|| ViewAlignmentError::MalformedPositions {
        detail: "no node is recorded as the root".to_owned(),
    })?;
    Ok((root, anchor, at))
}

/// The allocator fresh view ids are drawn from: at or above the
/// complement's floor and above every view, dropped and synthesized id it
/// records, and, on a complement that predates the floor, every other id
/// it records.
fn fresh_ids(complement: &Complement) -> IdAllocator {
    let mut ids = IdAllocator::from_floor(IdRange::FULL, complement.id_floor);
    let taken = complement
        .view_positions
        .keys()
        .chain(complement.dropped_nodes.keys())
        .chain(complement.synthesized_nodes.iter())
        .chain(complement.synthesized_keys.keys())
        .copied();
    for id in taken {
        ids.observe(id);
    }
    if complement.id_floor == 0 {
        let recorded = complement
            .original_parent
            .iter()
            .flat_map(|(&child, &parent)| [child, parent])
            .chain(
                complement
                    .arc_order
                    .iter()
                    .flat_map(|&pair| <[u32; 2]>::from(pair)),
            )
            .chain(complement.original_anchors.keys().copied())
            .chain(
                complement
                    .dropped_arcs
                    .iter()
                    .flat_map(|(p, c, _)| [*p, *c]),
            );
        for id in recorded {
            ids.observe(id);
        }
    }
    ids
}
