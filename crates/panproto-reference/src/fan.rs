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

/// Every violation of the fan rules at `node`, in this order.
///
/// For a node not of the fan shape, each reserved-kind out-edge; otherwise each in-edge, each
/// out-edge of no fan kind, the parent count, each duplicated label in label order, and with a
/// schema an undeclared hyper-edge or each label outside its signature.
#[must_use]
pub fn violations_at(graph: &Graph, node: u32, schema: Option<&Schema>) -> Vec<FanViolation> {
    let Some(fan) = graph.nodes.get(&node) else {
        return Vec::new();
    };
    let out: Vec<u32> = graph
        .edges
        .iter()
        .filter(|(_, edge)| edge.src == node)
        .map(|(id, _)| *id)
        .collect();
    if fan.shape != NodeShape::HyperEdge {
        return out
            .into_iter()
            .filter(|id| is_fan_kind(&graph.edges[id].label.kind))
            .map(|edge| FanViolation::ReservedKindOutsideFan { edge })
            .collect();
    }
    let mut found: Vec<FanViolation> = graph
        .edges
        .iter()
        .filter(|(_, edge)| edge.tgt == node)
        .map(|(id, _)| FanViolation::ForeignIncidentEdge { edge: *id })
        .collect();
    let mut parents = Vec::new();
    let mut labels: BTreeMap<Name, Vec<u32>> = BTreeMap::new();
    for id in out {
        let edge = &graph.edges[&id];
        match (edge.label.kind.as_str(), &edge.label.name) {
            (HYPER_PARENT, _) => parents.push(id),
            (HYPER_LABEL, Some(label)) => labels.entry(label.clone()).or_default().push(id),
            _ => found.push(FanViolation::ForeignIncidentEdge { edge: id }),
        }
    }
    match parents.len() {
        0 => found.push(FanViolation::MissingParent),
        1 => {}
        _ => found.push(FanViolation::MultipleParents { edges: parents }),
    }
    for (label, edges) in &labels {
        if edges.len() > 1 {
            found.push(FanViolation::DuplicateLabel {
                label: label.clone(),
                edges: edges.clone(),
            });
        }
    }
    if let Some(decl) = schema {
        match decl.hyper_edges.get(&fan.anchor) {
            None => found.push(FanViolation::UnknownHyperEdge {
                hyper_edge: fan.anchor.clone(),
            }),
            Some(hyper) => found.extend(
                labels
                    .keys()
                    .filter(|label| !hyper.signature.contains_key(*label))
                    .map(|label| FanViolation::UnknownLabel {
                        label: label.clone(),
                    }),
            ),
        }
    }
    found
}

/// The nodes that must be fans: every node of the fan shape and every
/// source of a reserved-kind edge, ascending.
#[must_use]
pub fn candidates(g: &Graph) -> Vec<u32> {
    g.nodes
        .keys()
        .copied()
        .filter(|n| {
            g.nodes[n].shape == NodeShape::HyperEdge
                || g.edges
                    .values()
                    .any(|e| e.src == *n && is_fan_kind(&e.label.kind))
        })
        .collect()
}

/// The first violation of the fan domain: the least offending node and its
/// first violation.
#[must_use]
pub fn first_violation(g: &Graph, schema: Option<&Schema>) -> Option<(u32, FanViolation)> {
    candidates(g).into_iter().find_map(|n| {
        violations_at(g, n, schema)
            .into_iter()
            .next()
            .map(|v| (n, v))
    })
}
