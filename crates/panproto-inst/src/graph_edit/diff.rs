//! The delta between two graph instances.

use std::collections::BTreeSet;

use super::canonical::same_val;
use super::delta::{AttrChange, AttrKey, GraphDelta, GraphDeltaParts, Payload};
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
    /// `diff(a, b).apply(a)` gives `b` and `diff(a, a)` is the identity
    /// (the differ of Hofmann et al. 2012, Section 7, stated).
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
}
