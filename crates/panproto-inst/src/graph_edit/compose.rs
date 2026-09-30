//! Inversion and composition of graph deltas.

use std::collections::{BTreeMap, BTreeSet};

use panproto_schema::Edge;
use serde::{Deserialize, Serialize};

use super::delta::{AttrChange, AttrKey, GraphDelta, GraphDeltaParts, Payload};
use crate::ginstance::EdgeId;
use crate::view_ref::RowId;

/// A reference to one element of a delta: a node, an edge, an attribution
/// entry, or (for table deltas) a row or a foreign-key pair.
///
/// The derived order sorts by variant in declaration order; the table
/// variants come last.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ElemRef {
    /// A node.
    Node(u32),
    /// An edge.
    Edge(EdgeId),
    /// An attribution entry.
    Attr(AttrKey),
    /// A table row.
    Row(RowId),
    /// A foreign-key pair.
    Pair {
        /// The foreign-key edge (boxed, so that a reference stays small; it
        /// serializes as the edge itself).
        edge: Box<Edge>,
        /// The source row.
        src: RowId,
        /// The target row.
        tgt: RowId,
    },
}

impl ElemRef {
    /// The graph element this reference names in the graph form of a
    /// table: a row is the node with its id, a pair is the edge `pair_edge`
    /// finds for it, a cell is the field of that name and a row's table is
    /// the node's anchor. Graph references are returned unchanged; `None`
    /// when `pair_edge` finds no edge for a pair.
    #[must_use]
    pub fn to_graph(
        &self,
        pair_edge: impl Fn(&Edge, RowId, RowId) -> Option<EdgeId>,
    ) -> Option<Self> {
        Some(match self {
            Self::Row(row) => Self::Node(row.0),
            Self::Pair { edge, src, tgt } => Self::Edge(pair_edge(edge, *src, *tgt)?),
            Self::Attr(AttrKey::Cell { row, column }) => Self::Attr(AttrKey::Field {
                node: row.0,
                field: column.clone(),
            }),
            Self::Attr(AttrKey::Table { row }) => Self::Attr(AttrKey::Anchor { node: row.0 }),
            other => other.clone(),
        })
    }
}

/// Why two deltas do not compose.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum ComposeError {
    /// The second delta's expectation of an element disagrees with what the
    /// first leaves: it deletes or changes an element the first deleted,
    /// adds one the first added or kept, or expects a payload or value the
    /// first did not produce.
    #[error("the deltas disagree on {elem:?}")]
    Mismatch {
        /// The element.
        elem: ElemRef,
    },
    /// An edit names an element whose payload only an instance holds, so it
    /// lowers to a delta only against one
    /// ([`GraphEdit::to_delta_in`](super::GraphEdit::to_delta_in)).
    #[error("{elem:?} needs the instance to lower to a delta")]
    NeedsState {
        /// The element.
        elem: ElemRef,
    },
}

/// What one delta does to one element.
enum Action<'a, P> {
    /// Nothing.
    None,
    /// Deletes it, with this payload.
    Del(&'a P),
    /// Adds it with this payload.
    Add(&'a P),
    /// Replaces the first payload with the second.
    Repl(&'a P, &'a P),
    /// Changes these attribution entries, in key order.
    Attrs(Vec<(&'a AttrKey, &'a AttrChange)>),
}

/// What the composite does to one element.
enum Out<P> {
    None,
    Del(P),
    Add(P),
    Repl(P, P),
    Attrs(Vec<(AttrKey, AttrChange)>),
}

/// The action of a delta on one element, given its delete and add payloads
/// and its attribution changes.
fn action<'a, P>(
    del: Option<&'a P>,
    add: Option<&'a P>,
    attrs: Vec<(&'a AttrKey, &'a AttrChange)>,
) -> Action<'a, P> {
    match (del, add) {
        (Some(p), Some(q)) => Action::Repl(p, q),
        (Some(p), None) => Action::Del(p),
        (None, Some(q)) => Action::Add(q),
        (None, None) if attrs.is_empty() => Action::None,
        (None, None) => Action::Attrs(attrs),
    }
}

/// Undo attribution changes on a payload that must show their `new`
/// values: each entry, in key order, must equal the change's `new` and is
/// set to its `old`.
fn unapply<P: Payload>(payload: &P, attrs: &[(&AttrKey, &AttrChange)]) -> Result<P, ComposeError> {
    let mut out = payload.clone();
    for (key, change) in attrs {
        match out.get(key) {
            Some(v) if super::canonical::same_val(&v, &change.new) => out.set(key, &change.old),
            _ => return Err(mismatch_attr(key)),
        }
    }
    Ok(out)
}

/// Apply attribution changes to a payload that must show their `old`
/// values: each entry, in key order, must equal the change's `old` and is
/// set to its `new`.
fn reapply<P: Payload>(payload: &P, attrs: &[(&AttrKey, &AttrChange)]) -> Result<P, ComposeError> {
    let mut out = payload.clone();
    for (key, change) in attrs {
        match out.get(key) {
            Some(v) if super::canonical::same_val(&v, &change.old) => out.set(key, &change.new),
            _ => return Err(mismatch_attr(key)),
        }
    }
    Ok(out)
}

fn mismatch_attr(key: &AttrKey) -> ComposeError {
    ComposeError::Mismatch {
        elem: ElemRef::Attr(key.clone()),
    }
}

/// Compose the actions of two deltas on one element, per the case table of
/// [`GraphDelta::compose`].
fn compose_action<P: Payload>(
    elem: &ElemRef,
    first: Action<'_, P>,
    next: Action<'_, P>,
) -> Result<Out<P>, ComposeError> {
    let mismatch = || ComposeError::Mismatch { elem: elem.clone() };
    Ok(match (first, next) {
        (Action::None, Action::None) => Out::None,
        (Action::None, Action::Del(r)) => Out::Del(r.clone()),
        (Action::None, Action::Add(r)) => Out::Add(r.clone()),
        (Action::None, Action::Repl(r, t)) => Out::Repl(r.clone(), t.clone()),
        (Action::None, Action::Attrs(a)) | (Action::Attrs(a), Action::None) => {
            Out::Attrs(owned(&a))
        }
        (Action::Del(p), Action::None) => Out::Del(p.clone()),
        (Action::Add(q), Action::None) => Out::Add(q.clone()),
        (Action::Repl(p, q), Action::None) => Out::Repl(p.clone(), q.clone()),
        (Action::Del(p), Action::Add(r)) => Out::Repl(p.clone(), r.clone()),
        (Action::Add(q), Action::Del(r)) if q.same(r) => Out::None,
        (Action::Add(q), Action::Repl(r, t)) if q.same(r) => Out::Add(t.clone()),
        (Action::Add(q), Action::Attrs(a)) => Out::Add(reapply(q, &a)?),
        (Action::Repl(p, q), Action::Del(r)) if q.same(r) => Out::Del(p.clone()),
        (Action::Repl(p, q), Action::Repl(r, t)) if q.same(r) => Out::Repl(p.clone(), t.clone()),
        (Action::Repl(p, q), Action::Attrs(a)) => Out::Repl(p.clone(), reapply(q, &a)?),
        (Action::Del(_), _)
        | (_, Action::Add(_))
        | (Action::Add(_) | Action::Repl(..), Action::Del(_) | Action::Repl(..)) => {
            return Err(mismatch());
        }
        (Action::Attrs(a), Action::Del(r)) => Out::Del(unapply(r, &a)?),
        (Action::Attrs(a), Action::Repl(r, t)) => Out::Repl(unapply(r, &a)?, t.clone()),
        (Action::Attrs(a), Action::Attrs(b)) => Out::Attrs(merge_attrs(&a, &b)?),
    })
}

fn owned(attrs: &[(&AttrKey, &AttrChange)]) -> Vec<(AttrKey, AttrChange)> {
    attrs
        .iter()
        .map(|(k, c)| ((*k).clone(), (*c).clone()))
        .collect()
}

/// Compose two attribution change lists of one element: a key both change
/// chains (`o -> n` then `n -> n'` gives `o -> n'`, dropped when `o == n'`);
/// a key one changes is kept.
fn merge_attrs(
    first: &[(&AttrKey, &AttrChange)],
    next: &[(&AttrKey, &AttrChange)],
) -> Result<Vec<(AttrKey, AttrChange)>, ComposeError> {
    let mut out: BTreeMap<AttrKey, AttrChange> = owned(first).into_iter().collect();
    for (key, second) in next {
        match out.remove(*key) {
            None => {
                out.insert((*key).clone(), (*second).clone());
            }
            Some(change) => {
                if !super::canonical::same_val(&change.new, &second.old) {
                    return Err(mismatch_attr(key));
                }
                out.insert(
                    (*key).clone(),
                    AttrChange {
                        old: change.old,
                        new: second.new.clone(),
                    },
                );
            }
        }
    }
    Ok(out.into_iter().collect())
}

/// Write one element's composite action into delete and add maps and the
/// attribution map.
fn emit<K: Ord + Copy, P>(
    out: Out<P>,
    id: K,
    del: &mut BTreeMap<K, P>,
    add: &mut BTreeMap<K, P>,
    attrs: &mut BTreeMap<AttrKey, AttrChange>,
) {
    match out {
        Out::None => {}
        Out::Del(p) => {
            del.insert(id, p);
        }
        Out::Add(q) => {
            add.insert(id, q);
        }
        Out::Repl(p, q) => {
            del.insert(id, p);
            add.insert(id, q);
        }
        Out::Attrs(changes) => attrs.extend(changes),
    }
}

/// The attribution changes of `delta` grouped by element.
fn attrs_by_element(delta: &GraphDelta) -> BTreeMap<ElemRef, Vec<(&AttrKey, &AttrChange)>> {
    let mut out: BTreeMap<ElemRef, Vec<(&AttrKey, &AttrChange)>> = BTreeMap::new();
    for (key, change) in delta.attrs() {
        out.entry(key.element()).or_default().push((key, change));
    }
    out
}

impl GraphDelta {
    /// The inverse delta: deletions and additions swap, and every
    /// attribution change's `old` and `new` swap. `invert(invert(d)) == d`.
    ///
    /// Applying a delta and then its inverse restores the instance: a deleted
    /// element comes back under its own id with its full payload.
    #[must_use]
    pub fn invert(&self) -> Self {
        let parts = self.to_parts();
        Self::from_valid_parts(GraphDeltaParts {
            del_nodes: parts.add_nodes,
            del_edges: parts.add_edges,
            add_nodes: parts.del_nodes,
            add_edges: parts.del_edges,
            attrs: parts
                .attrs
                .into_iter()
                .map(|(k, c)| {
                    (
                        k,
                        AttrChange {
                            old: c.new,
                            new: c.old,
                        },
                    )
                })
                .collect(),
        })
    }

    /// The net delta of `self` followed by `next`.
    ///
    /// Each element is composed on its own. Writing Del(p), Add(q),
    /// Repl(p -> q) and Attr(k: o -> n) for what a delta does to it, and
    /// q\[k\] for the entry `k` of payload q:
    ///
    /// | self \ next | Del(r) | Add(r) | Repl(r -> t) | Attr(k: o' -> n') |
    /// |---|---|---|---|---|
    /// | none | Del(r) | Add(r) | Repl(r -> t) | Attr(k: o' -> n') |
    /// | Del(p) | Mismatch | Repl(p -> r), never cancels | Mismatch | Mismatch |
    /// | Add(q) | r == q: cancels | Mismatch | r == q: Add(t) | o' == q\[k\]: Add(q\[k := n'\]) |
    /// | Repl(p -> q) | r == q: Del(p) | Mismatch | r == q: Repl(p -> t) | o' == q\[k\]: Repl(p -> q\[k := n'\]) |
    /// | Attr(k: o -> n) | r\[k\] == n: Del(r\[k := o\]) | Mismatch | r\[k\] == n: Repl(r\[k := o\] -> t) | same k: n == o' gives Attr(k: o -> n'), dropped if o == n'; other keys kept |
    ///
    /// A condition that fails is a [`ComposeError::Mismatch`] on the element,
    /// or on the attribution entry for a failing `k`. Several attribution
    /// changes of one element fold into a later Del or Repl payload in key
    /// order. Payloads and values compare by canonical bytes. Composing a
    /// delete with a later add of the same id is a replacement, never the
    /// identity; an add followed by a delete of the same payload cancels.
    ///
    /// [`GraphDelta::identity`] is a unit on both sides. Composition is
    /// associative on composable triples: when `a.compose(&b)` and
    /// `b.compose(&c)` are both defined, the two bracketings of `a`, `b`,
    /// `c` are both defined or both undefined, and equal when defined. On
    /// other triples one bracketing can be defined without the other, since
    /// the cancelling cell meets a mismatching one. With `add` and `del` an
    /// add and a delete of one node, and `attr` a change of that node's
    /// anchor, `attr.compose(&add)` is a mismatch while
    /// `attr.compose(&add.compose(&del)?)` is `attr`; and
    /// `add.compose(&del)?.compose(&attr)` is `attr` while `del.compose(&attr)`
    /// is a mismatch.
    ///
    /// # Errors
    ///
    /// Returns [`ComposeError::Mismatch`] naming the first element, in the
    /// order nodes then edges by ascending id, whose actions do not compose.
    pub fn compose(&self, next: &Self) -> Result<Self, ComposeError> {
        let first_attrs = attrs_by_element(self);
        let next_attrs = attrs_by_element(next);
        let mut out = GraphDeltaParts::default();

        let node_ids: BTreeSet<u32> = [self, next]
            .into_iter()
            .flat_map(|d| {
                d.del_nodes()
                    .keys()
                    .chain(d.add_nodes().keys())
                    .copied()
                    .chain(d.attrs().keys().filter_map(|k| match k.element() {
                        ElemRef::Node(n) => Some(n),
                        _ => None,
                    }))
            })
            .collect();
        for id in node_ids {
            let elem = ElemRef::Node(id);
            let a = action(
                self.del_nodes().get(&id),
                self.add_nodes().get(&id),
                first_attrs.get(&elem).cloned().unwrap_or_default(),
            );
            let b = action(
                next.del_nodes().get(&id),
                next.add_nodes().get(&id),
                next_attrs.get(&elem).cloned().unwrap_or_default(),
            );
            let composite = compose_action(&elem, a, b)?;
            emit(
                composite,
                id,
                &mut out.del_nodes,
                &mut out.add_nodes,
                &mut out.attrs,
            );
        }

        let edge_ids: BTreeSet<EdgeId> = [self, next]
            .into_iter()
            .flat_map(|d| {
                d.del_edges()
                    .keys()
                    .chain(d.add_edges().keys())
                    .copied()
                    .chain(d.attrs().keys().filter_map(|k| match k.element() {
                        ElemRef::Edge(e) => Some(e),
                        _ => None,
                    }))
            })
            .collect();
        for id in edge_ids {
            let elem = ElemRef::Edge(id);
            let a = action(
                self.del_edges().get(&id),
                self.add_edges().get(&id),
                first_attrs.get(&elem).cloned().unwrap_or_default(),
            );
            let b = action(
                next.del_edges().get(&id),
                next.add_edges().get(&id),
                next_attrs.get(&elem).cloned().unwrap_or_default(),
            );
            let composite = compose_action(&elem, a, b)?;
            emit(
                composite,
                id,
                &mut out.del_edges,
                &mut out.add_edges,
                &mut out.attrs,
            );
        }
        Ok(Self::from_valid_parts(out))
    }

    /// Every node id the delta mentions: deleted, added and attributed
    /// nodes, and the endpoints of deleted and added edges.
    #[must_use]
    pub fn touched_nodes(&self) -> BTreeSet<u32> {
        let mut ids: BTreeSet<u32> = self
            .del_nodes()
            .keys()
            .chain(self.add_nodes().keys())
            .copied()
            .collect();
        for edge in self.del_edges().values().chain(self.add_edges().values()) {
            ids.insert(edge.src);
            ids.insert(edge.tgt);
        }
        ids.extend(self.attrs().keys().filter_map(|k| match k.element() {
            ElemRef::Node(n) => Some(n),
            _ => None,
        }));
        ids
    }

    /// Every edge id the delta mentions: deleted, added and attributed
    /// edges.
    #[must_use]
    pub fn touched_edges(&self) -> BTreeSet<EdgeId> {
        let mut ids: BTreeSet<EdgeId> = self
            .del_edges()
            .keys()
            .chain(self.add_edges().keys())
            .copied()
            .collect();
        ids.extend(self.attrs().keys().filter_map(|k| match k.element() {
            ElemRef::Edge(e) => Some(e),
            _ => None,
        }));
        ids
    }
}
