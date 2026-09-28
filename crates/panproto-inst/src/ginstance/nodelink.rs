//! The node-link JSON form of a graph instance.
//!
//! ```text
//! {"nodes": [Node, ...],
//!  "edges": [{"id": u32, "src": u32, "tgt": u32, "edge": Edge, "position": u32 | null}, ...]}
//! ```
//!
//! Both arrays are written in ascending id order. The form uses arrays rather
//! than id-keyed objects because a JSON object key is a string, so consumers
//! that read JSON into native maps (JavaScript among them) would otherwise see
//! the ids as strings. Files holding this form use the suffix `.nodelink.json`.

use panproto_schema::Edge;
use serde::{Deserialize, Serialize};

use super::{EdgeId, GEdge, GInstance};
use crate::error::InstError;
use crate::metadata::Node;

#[derive(Serialize)]
struct NodeLinkRef<'a> {
    nodes: Vec<&'a Node>,
    edges: Vec<EdgeRef<'a>>,
}

#[derive(Serialize)]
struct EdgeRef<'a> {
    id: EdgeId,
    src: u32,
    tgt: u32,
    edge: &'a Edge,
    position: Option<u32>,
}

#[derive(Deserialize)]
struct NodeLink {
    nodes: Vec<Node>,
    edges: Vec<NodeLinkEdge>,
}

#[derive(Deserialize)]
struct NodeLinkEdge {
    id: EdgeId,
    src: u32,
    tgt: u32,
    edge: Edge,
    #[serde(default)]
    position: Option<u32>,
}

/// The node-link JSON form of a graph instance, arrays in ascending id order.
///
/// A float that JSON cannot represent (`NaN` or an infinity) is written as
/// `null`, as `serde_json` writes it everywhere.
#[must_use]
pub fn to_node_link_json(graph: &GInstance) -> serde_json::Value {
    let mut nodes: Vec<&Node> = graph.nodes().values().collect();
    nodes.sort_unstable_by_key(|n| n.id);
    let edges = graph
        .edges_sorted()
        .map(|(id, e)| EdgeRef {
            id,
            src: e.src,
            tgt: e.tgt,
            edge: &e.edge,
            position: e.position,
        })
        .collect();
    match serde_json::to_value(NodeLinkRef { nodes, edges }) {
        Ok(value) => value,
        // Every map in a node is keyed by `String` and every other field is a
        // plain datum, which is exactly what `serde_json::Value` can hold.
        Err(err) => unreachable!("a graph instance always has a JSON form: {err}"),
    }
}

/// Read a graph instance from its node-link JSON form.
///
/// The arrays may list nodes and edges in any order, and an edge's
/// `position` may be omitted, meaning no position.
///
/// # Errors
///
/// Returns [`InstError::NodeLink`] when the document does not have the
/// node-link shape, [`InstError::DuplicateNode`] or
/// [`InstError::DuplicateEdge`] when an id is listed twice, and
/// [`InstError::DanglingEdge`] when an edge names a node the document does
/// not list.
pub fn from_node_link_json(value: &serde_json::Value) -> Result<GInstance, InstError> {
    let doc = NodeLink::deserialize(value).map_err(|e| InstError::NodeLink(e.to_string()))?;
    let mut graph = GInstance::new();
    for node in doc.nodes {
        graph.insert_node(node)?;
    }
    for e in doc.edges {
        graph.insert_edge(
            e.id,
            GEdge {
                src: e.src,
                tgt: e.tgt,
                edge: e.edge,
                position: e.position,
            },
        )?;
    }
    Ok(graph)
}
