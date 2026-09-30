//! Independence and merge of two deltas of one state.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::canonical::{same_edge, same_node, same_val};
use super::compose::ElemRef;
use super::delta::{AttrKey, AttrVal, GraphDelta};
use crate::ginstance::{HYPER_LABEL_KIND, HYPER_PARENT_KIND};
use crate::metadata::NodeShape;

/// One way in which two deltas of the same state interfere.
///
/// The derived order sorts by variant in declaration order, then by
/// payload; [`GraphDelta::independence`] lists conflicts in that order.
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, thiserror::Error,
)]
pub enum EditConflict {
    /// Both deltas delete the element (a replacement deletes too).
    #[error("both deltas delete {elem:?}")]
    DeleteDelete {
        /// The element.
        elem: ElemRef,
    },
    /// One delta deletes an element the other uses: the other adds or
    /// replaces an edge with the deleted node as an endpoint, changes an
    /// attribution entry of the deleted element, or replaces it.
    #[error("one delta deletes {deleted:?}, which the other uses through {user:?}")]
    DeleteUse {
        /// The deleted element.
        deleted: ElemRef,
        /// The element of the other delta that uses it: the edge, the
        /// attribution entry, or the replaced element itself.
        user: ElemRef,
    },
    /// Both deltas change the same attribution entry.
    #[error("both deltas change {key:?}")]
    UpdateUpdate {
        /// The entry.
        key: AttrKey,
    },
    /// Both deltas add an element under the same id (a replacement adds
    /// too).
    #[error("both deltas add {elem:?}")]
    AddAdd {
        /// The element.
        elem: ElemRef,
    },
    /// One delta changes the anchor of a node and the other adds or
    /// replaces an edge incident to it, or changes the label of an edge that
    /// may be incident to it.
    #[error("one delta re-anchors node {node}, which the other meets through {user:?}")]
    AnchorUse {
        /// The re-anchored node.
        node: u32,
        /// The edge, or the label entry, of the other delta.
        user: ElemRef,
    },
    /// Both deltas change the fan structure at one node: each adds, deletes
    /// or replaces an edge incident to it, or changes its shape to or from
    /// the fan shape, and at least one of the two changes is a shape change
    /// or an edge of a reserved kind leaving the node. Together such changes can leave
    /// the fan domain although each alone does not (two `hyper_label` edges
    /// with one name, say). A label change to or from a reserved kind meets
    /// every such change of the other delta, since the edge's endpoints are
    /// state.
    #[error("both deltas change the fan structure, through {first:?} and {second:?}")]
    FanUse {
        /// The lesser of the two changed elements (an edge, or a `Meta` or
        /// `EdgeLabel` entry), so the conflict does not depend on which
        /// delta is `self`.
        first: ElemRef,
        /// The greater of the two changed elements.
        second: ElemRef,
    },
}

impl EditConflict {
    /// Whether the conflict is benign: a `DeleteDelete` whose two deleted
    /// payloads are equal, an `UpdateUpdate` whose two `new` values are
    /// equal, or an `AddAdd` whose two added payloads are equal. `a` and `b`
    /// are the two deltas the conflict was found between, in either order.
    /// Every other conflict is not benign.
    ///
    /// Only the part the variant names is compared. A replacement deletes
    /// too, so a delete and a replacement of one node with equal deleted
    /// payloads give a benign `DeleteDelete`, although one delta leaves the
    /// node deleted and the other adds a new node under its id; the pair
    /// also has the conflict `DeleteUse` on that node, which is not benign.
    ///
    /// A benign conflict still makes the deltas dependent: applying one
    /// after the other fails, since the second finds its element already
    /// deleted, added or changed.
    ///
    /// ```
    /// use panproto_inst::graph_edit::{EditConflict, ElemRef, GraphDelta, GraphDeltaParts};
    /// use panproto_inst::metadata::Node;
    ///
    /// let old = Node::new(0, "a");
    /// let mut delete = GraphDeltaParts::default();
    /// delete.del_nodes.insert(0, old.clone());
    /// let mut replace = delete.clone();
    /// replace.add_nodes.insert(0, Node::new(0, "z"));
    /// let (a, b) = (
    ///     GraphDelta::normalize(delete).unwrap(),
    ///     GraphDelta::normalize(replace).unwrap(),
    /// );
    /// let conflicts = a.independence(&b);
    /// let node = ElemRef::Node(0);
    /// assert_eq!(
    ///     conflicts,
    ///     vec![
    ///         EditConflict::DeleteDelete { elem: node.clone() },
    ///         EditConflict::DeleteUse { deleted: node.clone(), user: node },
    ///     ],
    /// );
    /// assert!(conflicts[0].is_benign(&a, &b));
    /// assert!(!conflicts[1].is_benign(&a, &b));
    /// ```
    #[must_use]
    pub fn is_benign(&self, a: &GraphDelta, b: &GraphDelta) -> bool {
        match self {
            Self::DeleteDelete { elem } => match elem {
                ElemRef::Node(n) => pair(a.del_nodes().get(n), b.del_nodes().get(n), same_node),
                ElemRef::Edge(e) => pair(a.del_edges().get(e), b.del_edges().get(e), same_edge),
                _ => false,
            },
            Self::AddAdd { elem } => match elem {
                ElemRef::Node(n) => pair(a.add_nodes().get(n), b.add_nodes().get(n), same_node),
                ElemRef::Edge(e) => pair(a.add_edges().get(e), b.add_edges().get(e), same_edge),
                _ => false,
            },
            Self::UpdateUpdate { key } => pair(
                a.attrs().get(key).map(|c| &c.new),
                b.attrs().get(key).map(|c| &c.new),
                same_val,
            ),
            Self::DeleteUse { .. } | Self::AnchorUse { .. } | Self::FanUse { .. } => false,
        }
    }
}

/// Whether both entries exist and are equal under `same`.
fn pair<T>(x: Option<&T>, y: Option<&T>, same: fn(&T, &T) -> bool) -> bool {
    matches!((x, y), (Some(x), Some(y)) if same(x, y))
}

/// The ways in which `deleter`'s deletions are used by `user`.
fn delete_use(deleter: &GraphDelta, user: &GraphDelta, out: &mut BTreeSet<EditConflict>) {
    for x in deleter.del_nodes().keys() {
        let gone = ElemRef::Node(*x);
        for (id, edge) in user.add_edges() {
            if edge.src == *x || edge.tgt == *x {
                out.insert(EditConflict::DeleteUse {
                    deleted: gone.clone(),
                    user: ElemRef::Edge(*id),
                });
            }
        }
        if user.del_nodes().contains_key(x) && user.add_nodes().contains_key(x) {
            out.insert(EditConflict::DeleteUse {
                deleted: gone.clone(),
                user: gone.clone(),
            });
        }
    }
    for e in deleter.del_edges().keys() {
        if user.del_edges().contains_key(e) && user.add_edges().contains_key(e) {
            out.insert(EditConflict::DeleteUse {
                deleted: ElemRef::Edge(*e),
                user: ElemRef::Edge(*e),
            });
        }
    }
    for key in user.attrs().keys() {
        let elem = key.element();
        let removed = match &elem {
            ElemRef::Node(n) => deleter.del_nodes().contains_key(n),
            ElemRef::Edge(e) => deleter.del_edges().contains_key(e),
            _ => false,
        };
        if removed {
            out.insert(EditConflict::DeleteUse {
                deleted: elem,
                user: ElemRef::Attr(key.clone()),
            });
        }
    }
}

/// The ways in which `anchorer`'s anchor changes meet `user`'s edges.
///
/// Whether a kept edge is incident to the re-anchored node is state, which
/// the representation does not hold, so every label change of `user` meets
/// every anchor change of `anchorer`.
fn anchor_use(anchorer: &GraphDelta, user: &GraphDelta, out: &mut BTreeSet<EditConflict>) {
    for key in anchorer.attrs().keys() {
        let AttrKey::Anchor { node } = key else {
            continue;
        };
        for (id, edge) in user.add_edges() {
            if edge.src == *node || edge.tgt == *node {
                out.insert(EditConflict::AnchorUse {
                    node: *node,
                    user: ElemRef::Edge(*id),
                });
            }
        }
        for label_key in user.attrs().keys() {
            if matches!(label_key, AttrKey::EdgeLabel { .. }) {
                out.insert(EditConflict::AnchorUse {
                    node: *node,
                    user: ElemRef::Attr(label_key.clone()),
                });
            }
        }
    }
}

/// Whether an attribution value is a label of a reserved fan kind.
fn reserved_label(value: &AttrVal) -> bool {
    matches!(value, AttrVal::Label(e) if e.kind == HYPER_PARENT_KIND || e.kind == HYPER_LABEL_KIND)
}

/// Whether an attribution value is metadata with the fan shape.
fn fan_shape(value: &AttrVal) -> bool {
    matches!(value, AttrVal::Meta(meta) if meta.shape == NodeShape::HyperEdge)
}

/// The changes a delta makes to the fan structure.
#[derive(Default)]
struct FanTouches {
    /// Per node, each changed element incident to it and whether the change
    /// is a shape change or a reserved-kind edge leaving the node.
    at: BTreeMap<u32, Vec<(ElemRef, bool)>>,
    /// Label changes to or from a reserved kind, whose endpoints are state.
    anywhere: Vec<ElemRef>,
}

impl FanTouches {
    fn of(delta: &GraphDelta) -> Self {
        let mut touches = Self::default();
        for (id, edge) in delta.del_edges().iter().chain(delta.add_edges()) {
            let reserved =
                edge.edge.kind == HYPER_PARENT_KIND || edge.edge.kind == HYPER_LABEL_KIND;
            // A fan is the source of its reserved edges; at the target the
            // edge is one more in-edge, which matters only to a fan there.
            for (node, reserved) in [(edge.src, reserved), (edge.tgt, false)] {
                touches
                    .at
                    .entry(node)
                    .or_default()
                    .push((ElemRef::Edge(*id), reserved));
            }
        }
        for (key, change) in delta.attrs() {
            match key {
                AttrKey::Meta { node } if fan_shape(&change.old) != fan_shape(&change.new) => {
                    touches
                        .at
                        .entry(*node)
                        .or_default()
                        .push((ElemRef::Attr(key.clone()), true));
                }
                AttrKey::EdgeLabel { .. }
                    if reserved_label(&change.old) || reserved_label(&change.new) =>
                {
                    touches.anywhere.push(ElemRef::Attr(key.clone()));
                }
                _ => {}
            }
        }
        touches
    }

    /// Every changed element, at any node.
    fn all(&self) -> impl Iterator<Item = &ElemRef> {
        self.at
            .values()
            .flatten()
            .map(|(elem, _)| elem)
            .chain(&self.anywhere)
    }
}

/// The fan conflicts between `a` and `b`.
fn fan_use(a: &GraphDelta, b: &GraphDelta, out: &mut BTreeSet<EditConflict>) {
    let (x, y) = (FanTouches::of(a), FanTouches::of(b));
    let mut pair = |p: &ElemRef, q: &ElemRef| {
        let (first, second) = if p <= q { (p, q) } else { (q, p) };
        out.insert(EditConflict::FanUse {
            first: first.clone(),
            second: second.clone(),
        });
    };
    for (node, mine) in &x.at {
        let Some(theirs) = y.at.get(node) else {
            continue;
        };
        for (p, p_reserved) in mine {
            for (q, q_reserved) in theirs {
                if *p_reserved || *q_reserved {
                    pair(p, q);
                }
            }
        }
    }
    for p in &x.anywhere {
        for q in y.all() {
            pair(p, q);
        }
    }
    for q in &y.anywhere {
        for p in x.at.values().flatten().map(|(elem, _)| elem) {
            pair(p, q);
        }
    }
}

impl GraphDelta {
    /// Every conflict between this delta and `other`, two deltas of the same
    /// state, in ascending order. The deltas are independent exactly when
    /// the list is empty; then, when both apply to a state `s`, applying one
    /// after the other in either order, or applying [`Self::merge`], gives
    /// the same result.
    ///
    /// Conflicts are found on the representation, without the state, so
    /// where the answer depends on the state they over-approximate: a label
    /// change meets every anchor change ([`EditConflict::AnchorUse`]), since
    /// whether the edge is incident to the node is state, and a label
    /// change to or from a reserved fan kind meets every fan change of the
    /// other delta ([`EditConflict::FanUse`]).
    #[must_use]
    pub fn independence(&self, other: &Self) -> Vec<EditConflict> {
        let mut out = BTreeSet::new();
        for n in self
            .del_nodes()
            .keys()
            .filter(|n| other.del_nodes().contains_key(n))
        {
            out.insert(EditConflict::DeleteDelete {
                elem: ElemRef::Node(*n),
            });
        }
        for e in self
            .del_edges()
            .keys()
            .filter(|e| other.del_edges().contains_key(e))
        {
            out.insert(EditConflict::DeleteDelete {
                elem: ElemRef::Edge(*e),
            });
        }
        for n in self
            .add_nodes()
            .keys()
            .filter(|n| other.add_nodes().contains_key(n))
        {
            out.insert(EditConflict::AddAdd {
                elem: ElemRef::Node(*n),
            });
        }
        for e in self
            .add_edges()
            .keys()
            .filter(|e| other.add_edges().contains_key(e))
        {
            out.insert(EditConflict::AddAdd {
                elem: ElemRef::Edge(*e),
            });
        }
        for key in self
            .attrs()
            .keys()
            .filter(|k| other.attrs().contains_key(*k))
        {
            out.insert(EditConflict::UpdateUpdate { key: key.clone() });
        }
        delete_use(self, other, &mut out);
        delete_use(other, self, &mut out);
        anchor_use(self, other, &mut out);
        anchor_use(other, self, &mut out);
        fan_use(self, other, &mut out);
        out.into_iter().collect()
    }

    /// The union of this delta and `other`, two independent deltas of the
    /// same state.
    ///
    /// # Errors
    ///
    /// Returns the first conflict of [`Self::independence`] when the two
    /// deltas are not independent, a benign one ([`EditConflict::is_benign`])
    /// included: merge is defined on independent deltas only.
    pub fn merge(&self, other: &Self) -> Result<Self, EditConflict> {
        if let Some(conflict) = self.independence(other).into_iter().next() {
            return Err(conflict);
        }
        let mut parts = self.to_parts();
        let theirs = other.to_parts();
        parts.del_nodes.extend(theirs.del_nodes);
        parts.del_edges.extend(theirs.del_edges);
        parts.add_nodes.extend(theirs.add_nodes);
        parts.add_edges.extend(theirs.add_edges);
        parts.attrs.extend(theirs.attrs);
        Ok(Self::from_valid_parts(parts))
    }
}
