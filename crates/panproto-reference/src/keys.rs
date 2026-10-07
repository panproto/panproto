//! Key values of graph elements and the key buckets of a scope.
//!
//! The value of a key path `[e1, ..., ek]` at an element is read along its
//! out-edges: each hop follows the out-edges carrying that label, in edge
//! order, and the value is the first non-null own value met at the last
//! hop; an element with no out-edge for the last hop reads its field named
//! by that edge (the field fallback). A null or absent part is SQL's NULL:
//! the element then has no key value and joins no bucket. A `Within(v)` key
//! is scoped by the nearest ancestor anchored at v (walking back one edge at
//! a time, the least of the nearest); an element with no such ancestor is
//! outside the key.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Graph, NodeShape, SEdge};
use crate::schema::{Key, KeyPart, KeyScope};
use crate::value::{Presence, Value};

fn own(g: &Graph, n: u32) -> Option<Value> {
    match g.nodes.get(&n)?.value.as_ref()? {
        Presence::Present(v) if *v != Value::Null => Some(v.clone()),
        _ => None,
    }
}

/// The value of a key path at node `n`, or `None` (NULL).
#[must_use]
pub fn path_value(g: &Graph, n: u32, path: &[SEdge]) -> Option<Value> {
    let (last, hops) = path.split_last()?;
    let mut frontier = vec![n];
    for hop in hops {
        frontier = frontier
            .iter()
            .flat_map(|x| {
                g.edges
                    .values()
                    .filter(move |e| e.src == *x && e.label == *hop)
                    .map(|e| e.tgt)
            })
            .collect();
    }
    frontier.into_iter().find_map(|x| {
        let targets: Vec<u32> = g
            .edges
            .values()
            .filter(|e| e.src == x && e.label == *last)
            .map(|e| e.tgt)
            .collect();
        if targets.is_empty() {
            g.nodes
                .get(&x)?
                .fields
                .get(last.column())
                .filter(|v| **v != Value::Null)
                .cloned()
        } else {
            targets.into_iter().find_map(|t| own(g, t))
        }
    })
}

/// The key value of `n` under `key`.
///
/// One value per part, or `None` when a part is NULL. A position part is the index of `n` among the
/// targets of its ordered edge out of `n`'s parent, by position then edge id.
#[must_use]
pub fn key_value(g: &Graph, n: u32, key: &Key) -> Option<Vec<Value>> {
    key.parts
        .iter()
        .map(|p| match p {
            KeyPart::Path(path) => path_value(g, n, path),
            KeyPart::Position { within } => {
                let parent = g
                    .edges
                    .values()
                    .find(|e| e.tgt == n && e.label == *within)?
                    .src;
                let mut sibs: Vec<(Option<u32>, u32, u32)> = g
                    .edges
                    .iter()
                    .filter(|(_, e)| e.src == parent && e.label == *within)
                    .map(|(i, e)| (e.position, *i, e.tgt))
                    .collect();
                sibs.sort();
                sibs.iter()
                    .position(|s| s.2 == n)
                    .and_then(|i| i64::try_from(i).ok())
                    .map(Value::Int)
            }
        })
        .collect()
}

/// The nearest ancestors of `n` anchored at `v` (walking back one edge at a
/// time over non-fan nodes); the least of them, or `None`.
#[must_use]
pub fn nearest_ancestor(g: &Graph, n: u32, v: &str) -> Option<u32> {
    let mut seen = BTreeSet::from([n]);
    let mut level = vec![n];
    while !level.is_empty() {
        let mut next: Vec<u32> = level
            .iter()
            .flat_map(|x| g.edges.values().filter(move |e| e.tgt == *x).map(|e| e.src))
            .filter(|p| seen.insert(*p))
            .collect();
        next.sort_unstable();
        if let Some(a) = next.iter().find(|x| {
            g.nodes
                .get(x)
                .is_some_and(|m| m.anchor == v && m.shape != NodeShape::HyperEdge)
        }) {
            return Some(*a);
        }
        level = next;
    }
    None
}

/// Key buckets: (scope, canonical key bytes) to (key value, members).
pub type Buckets = BTreeMap<(Option<u32>, Vec<u8>), (Vec<Value>, Vec<u32>)>;

/// The buckets of a key over the non-fan nodes anchored at `vertex`.
///
/// Each bucket key is the scope (the nearest ancestor for `Within`, else none) and the canonical
/// bytes of the key value; elements with a NULL part are left out, and so (separately returned) are
/// `Within` elements with no ancestor.
#[must_use]
pub fn buckets(g: &Graph, vertex: &str, key: &Key) -> (Buckets, Vec<u32>) {
    let mut out = Buckets::new();
    let mut outside = Vec::new();
    for (id, n) in &g.nodes {
        if n.anchor != vertex || n.shape == NodeShape::HyperEdge {
            continue;
        }
        let Some(values) = key_value(g, *id, key) else {
            continue;
        };
        let scope = match &key.scope {
            KeyScope::Within(v) => {
                let Some(a) = nearest_ancestor(g, *id, v) else {
                    outside.push(*id);
                    continue;
                };
                Some(a)
            }
            KeyScope::Global | KeyScope::Document => None,
        };
        let bytes: Vec<u8> = values
            .iter()
            .flat_map(|v| crate::canonical::value_bytes(v).unwrap_or_default())
            .collect();
        out.entry((scope, bytes))
            .or_insert_with(|| (values, Vec::new()))
            .1
            .push(*id);
    }
    (out, outside)
}
