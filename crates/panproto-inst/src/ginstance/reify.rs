//! Hyper-edge fans as nodes of a graph instance.
//!
//! A tree carries its n-ary relations as [`Fan`]s beside its arcs. A graph has
//! no such side table, so a fan becomes a node of its own, the *fan node*:
//!
//! - its anchor is the hyper-edge id and its shape is [`NodeShape::HyperEdge`];
//! - one edge, of kind [`HYPER_PARENT_KIND`], runs from it to the fan's parent;
//! - one edge per labelled child, of kind [`HYPER_LABEL_KIND`] and named by the
//!   label, runs from it to that child.
//!
//! [`reify_fans`] adds fan nodes to a graph and [`unreify_fans`] takes them out
//! again; on the image of [`reify_fans`] the two are exact inverses.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};

use super::{EdgeId, GEdge, GInstance};
use crate::error::InstError;
use crate::fan::Fan;
use crate::metadata::{Node, NodeShape};

/// Edge kind of the edge from a fan node to the fan's parent.
pub const HYPER_PARENT_KIND: &str = "hyper_parent";

/// Edge kind of the edge from a fan node to one of its labelled children.
pub const HYPER_LABEL_KIND: &str = "hyper_label";

/// Add one fan node per fan to a copy of `graph`.
///
/// Fan `k` of `fans` gets node id `max_node_id + 1 + k` (`k` from `0` when the
/// graph has no nodes), so duplicate fans become distinct fan nodes. Edge ids
/// continue above the graph's largest: fan by fan, the parent edge first and
/// then the child edges in ascending label order.
///
/// Each edge is labelled with a schema-shaped [`Edge`] whose `src` is the
/// hyper-edge id and whose `tgt` is the anchor of the node it reaches. The
/// parent edge is named by the hyper-edge's `parent_label` when `schema`
/// declares the hyper-edge, and is unnamed otherwise; child edges are named by
/// their label.
///
/// # Errors
///
/// Returns [`InstError::DanglingEdge`] when a fan names a node `graph` does
/// not hold (a fan node added here is not one of them), and [`InstError::NodeIdsExhausted`] or
/// [`InstError::EdgeIdsExhausted`] when the id space above the graph's
/// largest ids is too small.
pub fn reify_fans(
    graph: &GInstance,
    fans: &[Fan],
    schema: Option<&Schema>,
) -> Result<GInstance, InstError> {
    let mut result = graph.clone();
    let mut next_node = graph.max_node_id().map_or(Some(0), |m| m.checked_add(1));
    let mut next_edge = graph
        .max_edge_id()
        .map_or(Some(0), |EdgeId(m)| m.checked_add(1));

    for fan in fans {
        let fan_id = next_node.ok_or(InstError::NodeIdsExhausted)?;
        next_node = fan_id.checked_add(1);
        let hyper_edge = Name::from(fan.hyper_edge_id.as_str());
        result
            .insert_node(Node::new(fan_id, hyper_edge.clone()).with_shape(NodeShape::HyperEdge))?;

        let parent_label = schema
            .and_then(|s| s.hyper_edges.get(&hyper_edge))
            .map(|h| h.parent_label.clone());
        let mut targets: Vec<(&str, Option<Name>, u32)> =
            vec![(HYPER_PARENT_KIND, parent_label, fan.parent)];
        let mut labels: Vec<(&String, &u32)> = fan.children.iter().collect();
        labels.sort_unstable();
        targets.extend(
            labels
                .into_iter()
                .map(|(label, &child)| (HYPER_LABEL_KIND, Some(Name::from(label.as_str())), child)),
        );

        for (kind, name, target) in targets {
            let raw = next_edge.ok_or(InstError::EdgeIdsExhausted)?;
            next_edge = raw.checked_add(1);
            let id = EdgeId(raw);
            let anchor = graph
                .node(target)
                .ok_or(InstError::DanglingEdge {
                    edge: id,
                    endpoint: target,
                })?
                .anchor
                .clone();
            let label = Edge {
                src: hyper_edge.clone(),
                tgt: anchor,
                kind: Name::from(kind),
                name,
            };
            result.insert_edge(id, GEdge::new(fan_id, target, label))?;
        }
    }
    Ok(result)
}

/// The fan a node reifies, if it is a well-formed fan node.
///
/// A fan node has shape [`NodeShape::HyperEdge`], no incoming edges, exactly
/// one outgoing [`HYPER_PARENT_KIND`] edge, and otherwise only named
/// [`HYPER_LABEL_KIND`] edges with distinct names. Its edges are returned with
/// the fan.
pub(super) fn fan_of(graph: &GInstance, node: &Node) -> Option<(Fan, Vec<EdgeId>)> {
    if node.shape != NodeShape::HyperEdge || !graph.in_edges(node.id).is_empty() {
        return None;
    }
    let mut parent = None;
    let mut children: HashMap<String, u32> = HashMap::new();
    for &id in graph.out_edges(node.id) {
        let edge = &graph.edges[&id];
        if edge.edge.kind == HYPER_PARENT_KIND {
            if parent.replace(edge.tgt).is_some() {
                return None;
            }
        } else if edge.edge.kind == HYPER_LABEL_KIND {
            let label = edge.edge.name.as_ref()?.to_string();
            if children.insert(label, edge.tgt).is_some() {
                return None;
            }
        } else {
            return None;
        }
    }
    let fan = Fan {
        hyper_edge_id: node.anchor.to_string(),
        parent: parent?,
        children,
    };
    Some((fan, graph.out_edges(node.id).to_vec()))
}

/// Take every fan node out of a graph, returning the rest and the fans.
///
/// Fans are listed in ascending fan-node id, which is the order
/// [`reify_fans`] allocates them in. A node of shape [`NodeShape::HyperEdge`]
/// that does not have the fan-node form described in the module
/// documentation is left in the graph as an ordinary node.
///
/// Fan nodes are recognised by that form alone. So this inverts
/// [`reify_fans`] exactly, duplicate fans included, whenever the graph given
/// to [`reify_fans`] has no node of fan-node form of its own; such a node
/// would be taken out as a fan too. In the graph form of a tree only the root
/// can have that form, which [`GInstance::to_wtype`] keeps:
/// [`GInstance::from_wtype`] refuses a tree any other node of which has it.
#[must_use]
pub fn unreify_fans(graph: &GInstance) -> (GInstance, Vec<Fan>) {
    unreify_fans_keeping(graph, None)
}

/// [`unreify_fans`], leaving the node `keep` in place whatever its form.
pub(super) fn unreify_fans_keeping(graph: &GInstance, keep: Option<u32>) -> (GInstance, Vec<Fan>) {
    let mut ids: Vec<u32> = graph
        .nodes()
        .values()
        .filter(|n| n.shape == NodeShape::HyperEdge && Some(n.id) != keep)
        .map(|n| n.id)
        .collect();
    ids.sort_unstable();

    let mut rest = graph.clone();
    let mut fans = Vec::new();
    for id in ids {
        let Some((fan, edges)) = graph.node(id).and_then(|node| fan_of(graph, node)) else {
            continue;
        };
        for edge in edges {
            // The edge is one of this node's out-edges, so it is present.
            let _ = rest.remove_edge(edge);
        }
        // The fan node has no incoming edges and its outgoing ones are gone.
        let _ = rest.remove_node(id);
        fans.push(fan);
    }
    (rest, fans)
}
