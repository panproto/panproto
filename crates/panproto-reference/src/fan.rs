//! The fan domain: reified hyper-edge occurrences in graphs.
//!
//! A fan node is well formed iff it has exactly one out-edge of kind
//! `hyper_parent`, at most one out-edge of kind `hyper_label` per label, no
//! other out-edge, and no in-edge. The fan domain is the set of graphs in
//! which every node of the hyper-edge shape, and every source of a reserved
//! edge kind, is a well-formed fan; with a schema, a fan's hyper-edge must
//! be declared and its labels must be in its signature.

use std::collections::BTreeMap;

use crate::model::{Graph, Name, NodeShape};
use crate::restrict::{HYPER_LABEL, HYPER_PARENT, is_fan_kind};
use crate::schema::Schema;

/// A violation of the fan domain at one node.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FanViolation {
    /// No parent edge.
    MissingParent,
    /// Several parent edges.
    MultipleParents {
        /// Their ids.
        edges: Vec<u32>,
    },
    /// Several child edges with one label.
    DuplicateLabel {
        /// The label.
        label: Name,
        /// Their ids.
        edges: Vec<u32>,
    },
    /// A label outside the hyper-edge's signature.
    UnknownLabel {
        /// The label.
        label: Name,
    },
    /// A reserved-kind edge out of a node that is not of the fan shape.
    ReservedKindOutsideFan {
        /// The edge.
        edge: u32,
    },
    /// An in-edge, or an out-edge of no fan kind (or an unnamed child edge).
    ForeignIncidentEdge {
        /// The edge.
        edge: u32,
    },
    /// The fan's hyper-edge is not declared.
    UnknownHyperEdge {
        /// The hyper-edge.
        hyper_edge: Name,
    },
}

/// Every violation of the fan rules at `node`, in this order: for a node not
/// of the fan shape, each reserved-kind out-edge; otherwise each in-edge,
/// each out-edge of no fan kind, the parent count, each duplicated label in
/// label order, and with a schema an undeclared hyper-edge or each label
/// outside its signature.
#[must_use]
pub fn violations_at(g: &Graph, node: u32, schema: Option<&Schema>) -> Vec<FanViolation> {
    let Some(n) = g.nodes.get(&node) else { return Vec::new() };
    let out: Vec<u32> = g.edges.iter().filter(|(_, e)| e.src == node).map(|(i, _)| *i).collect();
    if n.shape != NodeShape::HyperEdge {
        return out
            .into_iter()
            .filter(|i| is_fan_kind(&g.edges[i].label.kind))
            .map(|edge| FanViolation::ReservedKindOutsideFan { edge })
            .collect();
    }
    let mut v: Vec<FanViolation> = g
        .edges
        .iter()
        .filter(|(_, e)| e.tgt == node)
        .map(|(i, _)| FanViolation::ForeignIncidentEdge { edge: *i })
        .collect();
    let mut parents = Vec::new();
    let mut labels: BTreeMap<Name, Vec<u32>> = BTreeMap::new();
    for i in out {
        let e = &g.edges[&i];
        match (e.label.kind.as_str(), &e.label.name) {
            (HYPER_PARENT, _) => parents.push(i),
            (HYPER_LABEL, Some(l)) => labels.entry(l.clone()).or_default().push(i),
            _ => v.push(FanViolation::ForeignIncidentEdge { edge: i }),
        }
    }
    match parents.len() {
        0 => v.push(FanViolation::MissingParent),
        1 => {}
        _ => v.push(FanViolation::MultipleParents { edges: parents }),
    }
    for (label, edges) in &labels {
        if edges.len() > 1 {
            v.push(FanViolation::DuplicateLabel { label: label.clone(), edges: edges.clone() });
        }
    }
    if let Some(s) = schema {
        match s.hyper_edges.get(&n.anchor) {
            None => v.push(FanViolation::UnknownHyperEdge { hyper_edge: n.anchor.clone() }),
            Some(h) => v.extend(labels.keys().filter(|l| !h.signature.contains_key(*l)).map(|l| FanViolation::UnknownLabel { label: l.clone() })),
        }
    }
    v
}

/// The nodes that must be fans: every node of the fan shape and every
/// source of a reserved-kind edge, ascending.
#[must_use]
pub fn candidates(g: &Graph) -> Vec<u32> {
    g.nodes
        .keys()
        .copied()
        .filter(|n| g.nodes[n].shape == NodeShape::HyperEdge || g.edges.values().any(|e| e.src == *n && is_fan_kind(&e.label.kind)))
        .collect()
}

/// The first violation of the fan domain: the least offending node and its
/// first violation.
#[must_use]
pub fn first_violation(g: &Graph, schema: Option<&Schema>) -> Option<(u32, FanViolation)> {
    candidates(g).into_iter().find_map(|n| violations_at(g, n, schema).into_iter().next().map(|v| (n, v)))
}
