//! The delta between two graph instances.

use std::collections::BTreeSet;

use super::canonical::same_val;
use super::delta::{AttrChange, AttrKey, GraphDelta, GraphDeltaParts, MalformedDelta, Payload};
use crate::ginstance::GInstance;
use crate::metadata::Node;

/// The attribution keys of a node pair: value, every field either node has
/// (ascending), metadata and anchor.
fn node_keys(id: u32, a: &Node, b: &Node) -> Vec<AttrKey> {
    let fields: BTreeSet<&String> = a.extra_fields.keys().chain(b.extra_fields.keys()).collect();
    let mut keys = vec![AttrKey::Value { node: id }];
    keys.extend(fields.into_iter().map(|field| AttrKey::Field {
        node: id,
        field: field.clone(),
    }));
    keys.push(AttrKey::Meta { node: id });
    keys.push(AttrKey::Anchor { node: id });
    keys
}

/// Record the changed entries of `keys` between two payloads.
fn changes<P: Payload>(keys: Vec<AttrKey>, a: &P, b: &P, parts: &mut GraphDeltaParts) {
    for key in keys {
        if let (Some(old), Some(new)) = (a.get(&key), b.get(&key))
            && !same_val(&old, &new)
        {
            parts.attrs.insert(key, AttrChange { old, new });
        }
    }
}

impl GraphDelta {
    /// The delta from `a` to `b`, keyed by node id and [`EdgeId`](crate::EdgeId).
    ///
    /// A node or edge only in `a` is deleted and one only in `b` is added. A
    /// node in both keeps its id, and every differing value, field,
    /// metadata or anchor becomes an attribution change. An edge in both
    /// whose endpoints differ is replaced; otherwise a differing label or
    /// position becomes an attribution change. No replacement has equal
    /// payloads.
    ///
    /// `diff(a, a)` is the identity, and `diff(a, b).apply(a)` gives `b`
    /// whenever `b` is in the [`FanDomain`](crate::FanDomain) (the differ of
    /// Hofmann et al. 2012, Section 7, stated). The source `a` need not be in
    /// the fan domain. For `b` outside it the diff is still computed, but
    /// [`GraphDelta::apply`] rejects it with
    /// [`EditError::MalformedFan`](crate::EditError::MalformedFan) and leaves
    /// `a` unchanged, since every applied result is in the fan domain.
    ///
    /// ```
    /// use panproto_inst::graph_edit::GraphDelta;
    /// use panproto_inst::{GInstance, Node};
    ///
    /// let a = GInstance::new().with_node(Node::new(0, "person"));
    /// let b = GInstance::new().with_node(Node::new(0, "agent"));
    /// let mut c = a.clone();
    /// GraphDelta::diff(&a, &b).apply(&mut c).unwrap();
    /// assert!(GInstance::identical(&c, &b));
    /// assert!(GraphDelta::diff(&a, &a).is_identity());
    ///
    /// // A `hyper_parent` edge out of a node that is not a fan puts `d`
    /// // outside the fan domain: its diff is rejected, the reverse applies.
    /// use panproto_inst::{EditError, EdgeId, FanDomain, HYPER_PARENT_KIND};
    /// use panproto_schema::Edge;
    /// let parent = Edge {
    ///     src: "person".into(),
    ///     tgt: "person".into(),
    ///     kind: HYPER_PARENT_KIND.into(),
    ///     name: None,
    /// };
    /// let pair = a.clone().with_node(Node::new(1, "person"));
    /// let d = pair.clone().with_edge_id(EdgeId(9), 0, 1, parent);
    /// assert!(!FanDomain::contains(&d));
    /// let mut c = pair.clone();
    /// let err = GraphDelta::diff(&pair, &d).apply(&mut c).unwrap_err();
    /// assert!(matches!(err, EditError::MalformedFan { node: 0, .. }));
    /// assert!(GInstance::identical(&c, &pair));
    /// let mut c = d.clone();
    /// GraphDelta::diff(&d, &pair).apply(&mut c).unwrap();
    /// assert!(GInstance::identical(&c, &pair));
    /// ```
    #[must_use]
    pub fn diff(a: &GInstance, b: &GInstance) -> Self {
        let mut parts = GraphDeltaParts::default();
        let mut ids: Vec<u32> = a.nodes().keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let Some(old) = a.node(id) else { continue };
            match b.node(id) {
                None => {
                    parts.del_nodes.insert(id, old.clone());
                }
                Some(new) => changes(node_keys(id, old, new), old, new, &mut parts),
            }
        }
        for (id, node) in b.nodes() {
            if a.node(*id).is_none() {
                parts.add_nodes.insert(*id, node.clone());
            }
        }
        for (id, old) in a.edges_sorted() {
            match b.edge(id) {
                None => {
                    parts.del_edges.insert(id, old.clone());
                }
                Some(new) if (old.src, old.tgt) != (new.src, new.tgt) => {
                    parts.del_edges.insert(id, old.clone());
                    parts.add_edges.insert(id, new.clone());
                }
                Some(new) => changes(
                    vec![
                        AttrKey::EdgeLabel { edge: id },
                        AttrKey::EdgePosition { edge: id },
                    ],
                    old,
                    new,
                    &mut parts,
                ),
            }
        }
        for (id, edge) in b.edges_sorted() {
            if a.edge(id).is_none() {
                parts.add_edges.insert(id, edge.clone());
            }
        }
        Self::from_valid_parts(parts)
    }

    /// The incidence-closed replacement differ from `before` to `after`
    /// keeping the replacements of `input`.
    ///
    /// It is [`Self::diff`] with every element `input` replaces (an id in
    /// both of its structural parts) that both states hold made a
    /// replacement from its `before` payload to its `after` payload, even
    /// when the two are equal, and then closed under incidence: every edge of
    /// `before` incident to a deleted node is deleted, and every edge of
    /// `after` incident to an added node is added, each with its counterpart
    /// in the other state when that state holds the edge's id. A state diff
    /// cannot recover a replacement with equal payloads, while canonical
    /// normal form keeps it. The closure reads only edges the two states
    /// hold, so the states may be parts of two larger states that hold every
    /// edge incident to a node the result changes. The result applies to
    /// `before` with result `after` when both are valid graphs.
    ///
    /// # Errors
    ///
    /// [`MalformedDelta`] when the closed parts are not well sorted, which a
    /// pair of graphs never gives.
    pub fn diff_keeping_replacements(
        before: &GInstance,
        after: &GInstance,
        input: &Self,
    ) -> Result<Self, MalformedDelta> {
        let mut parts = Self::diff(before, after).into_parts();
        for id in input
            .del_nodes()
            .keys()
            .filter(|id| input.add_nodes().contains_key(id))
        {
            if let (Some(old), Some(new)) = (before.node(*id), after.node(*id)) {
                parts.del_nodes.insert(*id, old.clone());
                parts.add_nodes.insert(*id, new.clone());
            }
        }
        for id in input
            .del_edges()
            .keys()
            .filter(|id| input.add_edges().contains_key(id))
        {
            if let (Some(old), Some(new)) = (before.edge(*id), after.edge(*id)) {
                parts.del_edges.insert(*id, old.clone());
                parts.add_edges.insert(*id, new.clone());
            }
        }
        for (id, old) in before.edges_sorted() {
            if parts.del_nodes.contains_key(&old.src) || parts.del_nodes.contains_key(&old.tgt) {
                parts.del_edges.insert(id, old.clone());
                if let Some(new) = after.edge(id) {
                    parts.add_edges.insert(id, new.clone());
                }
            }
        }
        for (id, new) in after.edges_sorted() {
            if parts.add_nodes.contains_key(&new.src) || parts.add_nodes.contains_key(&new.tgt) {
                parts.add_edges.insert(id, new.clone());
                if let Some(old) = before.edge(id) {
                    parts.del_edges.insert(id, old.clone());
                }
            }
        }
        Self::normalize(parts)
    }
}
