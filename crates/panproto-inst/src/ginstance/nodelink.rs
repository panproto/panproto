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
//!
//! Values are written as serde writes [`Value`] in a human-readable format.
//! JSON has no number for a float that is `NaN` or infinite, so such a float
//! is written under `"Float"` as one of the strings `"NaN"`, `"-NaN"`,
//! `"Infinity"` and `"-Infinity"`, or `"NaN:0x"` followed by the sixteen
//! lowercase hexadecimal digits of the bits of any other `NaN`, and read back
//! bit for bit. Every finite float writes a number under `"Float"`, so the
//! strings cannot be mistaken for anything else.

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

/// The node-link JSON form of a graph instance, arrays in ascending id order
/// and the keys of every object in ascending order.
///
/// The keys are ordered because a `serde_json::Value` lists them in insertion
/// order when the build includes `serde_json`'s `preserve_order` feature and
/// in ascending order otherwise; inserting them in ascending order makes the
/// form, and every text written from it, the same in both builds.
///
/// Values are written as serde writes [`Value`](crate::value::Value) in a
/// human-readable format, so a float JSON cannot represent is written under
/// `"Float"` as `"NaN"`, `"-NaN"`, `"Infinity"`, `"-Infinity"`, or `"NaN:0x"`
/// followed by the sixteen lowercase hexadecimal digits of any other `NaN`'s
/// bits, and [`from_node_link_json`] reads every graph back with every float
/// bit intact, the sign and payload of a `NaN` included.
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
        Ok(value) => with_ascending_keys(value),
        // Every map in a graph is keyed by `String`, every float has a
        // human-readable spelling, and every other field is a plain datum,
        // which is exactly what `serde_json::Value` can hold.
        Err(err) => unreachable!("a graph instance always has a JSON form: {err}"),
    }
}

/// `json` with the keys of every object inserted in ascending order.
fn with_ascending_keys(json: serde_json::Value) -> serde_json::Value {
    match json {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<(String, serde_json::Value)> = map.into_iter().collect();
            entries.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
            serde_json::Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, with_ascending_keys(value)))
                    .collect(),
            )
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(with_ascending_keys).collect())
        }
        other => other,
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
/// node-link shape (a string under `"Float"` other than the spellings
/// [`to_node_link_json`] writes included), [`InstError::DuplicateNode`] or
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
