//! The key indexes validation keeps: for each key, the elements sharing
//! each key value.
//!
//! Whole-instance validation builds an index per key to decide `KeyUnique`
//! and `ReferenceResolves`; incremental validation maintains the same
//! indexes, so its memory is the open verdicts plus these buckets.

use std::collections::BTreeMap;

use panproto_gat::Name;
use panproto_schema::{Edge, Key, KeyPart, KeyScope};

use crate::identity::CanonicalSink;
use crate::value::Value;
use crate::view_ref::ViewRef;

use super::checks::InstanceView;

/// The name a key part is reported under: the labels of its path joined by
/// `/` (an unlabelled edge by its target vertex), or `position(<label>)` for
/// a position part.
pub fn part_name(part: &KeyPart) -> String {
    let label = |e: &Edge| {
        e.name
            .as_ref()
            .map_or_else(|| e.tgt.to_string(), Name::to_string)
    };
    match part {
        KeyPart::Path(path) => path.iter().map(label).collect::<Vec<_>>().join("/"),
        KeyPart::Position { within } => format!("position({})", label(within)),
    }
}

/// The values reached from `start` along `path`, in element order.
///
/// Each hop follows the out-edges labelled with the hop's edge. At the last
/// hop the value is the own value of each node reached; when no node is
/// reached there, the field named like the last edge (a node's extra field,
/// a row's cell) of each node before it is read instead. Absent and null
/// values are skipped.
pub fn path_values<'a>(view: InstanceView<'a>, start: &ViewRef, path: &[Edge]) -> Vec<&'a Value> {
    let Some((last, hops)) = path.split_last() else {
        return view.own_value(start).value().into_iter().collect();
    };
    let mut frontier = vec![start.clone()];
    for hop in hops {
        frontier = frontier
            .iter()
            .flat_map(|n| view.out_edges(n))
            .filter(|e| e.label == hop)
            .map(|e| e.tgt)
            .collect();
    }
    let reached: Vec<ViewRef> = frontier
        .iter()
        .flat_map(|n| view.out_edges(n))
        .filter(|e| e.label == last)
        .map(|e| e.tgt)
        .collect();
    if reached.is_empty() {
        let field = last
            .name
            .as_ref()
            .map_or_else(|| last.tgt.to_string(), Name::to_string);
        frontier
            .iter()
            .filter_map(|n| view.field(n, &field).value())
            .collect()
    } else {
        reached
            .iter()
            .filter_map(|n| view.own_value(n).value())
            .collect()
    }
}

/// The value of one key part of `element`: the first value along its path,
/// or its position among the targets of the ordered edge. `None` when the
/// part has no value, which SQL reads as NULL.
pub fn part_value(view: InstanceView<'_>, element: &ViewRef, part: &KeyPart) -> Option<Value> {
    match part {
        KeyPart::Path(path) => path_values(view, element, path)
            .first()
            .map(|v| (*v).clone()),
        KeyPart::Position { within } => view
            .position_of(element, within)
            .map(|p| Value::Int(i64::from(p))),
    }
}

/// The value of every part of `key` for `element`, or `None` when some part
/// has none (an element with a NULL key part takes no part in uniqueness).
pub fn key_values(view: InstanceView<'_>, element: &ViewRef, key: &Key) -> Option<Vec<Value>> {
    key.parts
        .iter()
        .map(|part| part_value(view, element, part))
        .collect()
}

/// The canonical bytes of a key value: each part's value through
/// [`CanonicalSink::value`], behind the part count.
pub fn key_bytes(values: &[Value]) -> Vec<u8> {
    let mut w = panproto_schema::CanonicalWriter::new();
    w.len(values.len());
    for v in values {
        w.value(v);
    }
    w.into_bytes()
}

/// The scope element of `element` under `scope`: for `Within(vertex)`, the
/// nearest node anchored at `vertex` reached backwards along in-edges (the
/// least such node when several are equally near), else `None`. Document
/// and global scopes are the whole instance, `None`.
pub fn scope_of(view: InstanceView<'_>, element: &ViewRef, scope: &KeyScope) -> Option<ViewRef> {
    let KeyScope::Within(vertex) = scope else {
        return None;
    };
    let mut seen = std::collections::BTreeSet::from([element.clone()]);
    let mut frontier = vec![element.clone()];
    while !frontier.is_empty() {
        let mut next: Vec<ViewRef> = frontier
            .iter()
            .flat_map(|n| view.in_edges(n))
            .map(|e| e.src)
            .filter(|n| seen.insert(n.clone()))
            .collect();
        next.sort();
        if let Some(found) = next.iter().find(|n| {
            view.node(n)
                .is_some_and(|rec| rec.anchor == vertex && !rec.fan)
        }) {
            return Some(found.clone());
        }
        frontier = next;
    }
    None
}

/// One key's index: the elements anchored at its vertex, bucketed by scope
/// and key value. Elements with a NULL part are left out.
#[derive(Clone, Debug, Default)]
pub struct KeyIndex {
    /// The elements of each `(scope, key bytes)` bucket, ascending.
    pub buckets: BTreeMap<(Option<ViewRef>, Vec<u8>), Vec<ViewRef>>,
    /// The elements of each key value, in any scope, ascending.
    pub by_value: BTreeMap<Vec<u8>, Vec<ViewRef>>,
    /// The key value of each indexed element.
    pub values: BTreeMap<ViewRef, Vec<Value>>,
}

impl KeyIndex {
    /// Index the elements of `vertex` by `key`, scoped when `scoped`.
    pub(crate) fn build(view: InstanceView<'_>, vertex: &Name, key: &Key, scoped: bool) -> Self {
        let mut index = Self::default();
        for element in view.anchored(vertex) {
            let Some(values) = key_values(view, &element, key) else {
                continue;
            };
            let scope = if scoped {
                scope_of(view, &element, &key.scope)
            } else {
                None
            };
            let bytes = key_bytes(&values);
            index
                .by_value
                .entry(bytes.clone())
                .or_default()
                .push(element.clone());
            index
                .buckets
                .entry((scope, bytes))
                .or_default()
                .push(element.clone());
            index.values.insert(element, values);
        }
        index
    }

    /// The elements whose key value has canonical bytes `bytes`, in any
    /// scope, ascending.
    pub(crate) fn lookup(&self, bytes: &[u8]) -> &[ViewRef] {
        self.by_value.get(bytes).map_or(&[], Vec::as_slice)
    }
}
