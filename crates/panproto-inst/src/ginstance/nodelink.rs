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
//! Values are written as serde writes [`Value`], with one exception: JSON has
//! no number for a float that is `NaN` or infinite, so such a float is written
//! as `{"Float": "NaN"}`, `{"Float": "Infinity"}` or `{"Float": "-Infinity"}`,
//! and read back as that float. Every other value writes a number under
//! `"Float"`, so the strings cannot be mistaken for anything else.

use panproto_schema::Edge;
use serde::{Deserialize, Serialize};

use super::{EdgeId, GEdge, GInstance};
use crate::error::InstError;
use crate::metadata::Node;
use crate::value::{FieldPresence, Value};

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
    /// Nodes as JSON, decoded one by one once non-finite floats are read.
    nodes: Vec<serde_json::Value>,
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
/// A float that JSON cannot represent (`NaN` or an infinity) is written as a
/// string (see the [module documentation](self)), so that
/// [`from_node_link_json`] reads every graph back; a `NaN` comes back as the
/// standard `NaN`, whatever its payload bits were.
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
    let written = nodes.clone();
    let mut doc = match serde_json::to_value(NodeLinkRef { nodes, edges }) {
        Ok(value) => value,
        // Every map in a node is keyed by `String` and every other field is a
        // plain datum, which is exactly what `serde_json::Value` can hold.
        Err(err) => unreachable!("a graph instance always has a JSON form: {err}"),
    };
    if let Some(slots) = doc
        .get_mut("nodes")
        .and_then(serde_json::Value::as_array_mut)
    {
        for (node, json) in written.into_iter().zip(slots) {
            for_each_value_mut(node, json, mark_non_finite);
        }
    }
    doc
}

/// Pair each value a node holds with its place in the node's JSON form.
fn for_each_value_mut(
    node: &Node,
    json: &mut serde_json::Value,
    mut visit: impl FnMut(&Value, &mut serde_json::Value),
) {
    if let Some(FieldPresence::Present(value)) = &node.value
        && let Some(slot) = json.get_mut("value").and_then(|v| v.get_mut("Present"))
    {
        visit(value, slot);
    }
    for (field, map) in [
        ("extra_fields", &node.extra_fields),
        ("annotations", &node.annotations),
    ] {
        for (key, value) in map {
            if let Some(slot) = json.get_mut(field).and_then(|m| m.get_mut(key)) {
                visit(value, slot);
            }
        }
    }
}

/// The string a non-finite float is written as.
fn non_finite_name(x: f64) -> &'static str {
    if x.is_nan() {
        "NaN"
    } else if x > 0.0 {
        "Infinity"
    } else {
        "-Infinity"
    }
}

/// The non-finite float a string names.
fn non_finite_value(name: &str) -> Option<f64> {
    match name {
        "NaN" => Some(f64::NAN),
        "Infinity" => Some(f64::INFINITY),
        "-Infinity" => Some(f64::NEG_INFINITY),
        _ => None,
    }
}

/// Rewrite each non-finite float inside `value`'s JSON form as its string.
fn mark_non_finite(value: &Value, json: &mut serde_json::Value) {
    match value {
        Value::Float(x) if !x.is_finite() => {
            *json = serde_json::json!({ "Float": non_finite_name(*x) });
        }
        Value::List(items) => {
            if let Some(slots) = json
                .get_mut("List")
                .and_then(serde_json::Value::as_array_mut)
            {
                for (item, slot) in items.iter().zip(slots) {
                    mark_non_finite(item, slot);
                }
            }
        }
        Value::Unknown(fields) => {
            let slots = json.get_mut("Unknown");
            mark_fields(fields, slots);
        }
        Value::Opaque { fields, .. } => {
            let slots = json.get_mut("Opaque").and_then(|o| o.get_mut("fields"));
            mark_fields(fields, slots);
        }
        _ => {}
    }
}

/// [`mark_non_finite`] over the fields of a record value.
fn mark_fields(
    fields: &std::collections::HashMap<String, Value>,
    slots: Option<&mut serde_json::Value>,
) {
    let Some(slots) = slots else {
        return;
    };
    for (key, value) in fields {
        if let Some(slot) = slots.get_mut(key) {
            mark_non_finite(value, slot);
        }
    }
}

/// The value positions of a node's JSON form: its present value, then its
/// extra fields and annotations.
fn value_slots_mut(json: &mut serde_json::Value) -> Vec<&mut serde_json::Value> {
    let mut slots = Vec::new();
    let Some(object) = json.as_object_mut() else {
        return slots;
    };
    for (field, entry) in object.iter_mut() {
        match field.as_str() {
            "value" => {
                if let Some(present) = entry.get_mut("Present") {
                    slots.push(present);
                }
            }
            "extra_fields" | "annotations" => {
                if let Some(map) = entry.as_object_mut() {
                    slots.extend(map.values_mut());
                }
            }
            _ => {}
        }
    }
    slots
}

/// Replace each non-finite-float string inside a value's JSON form by `0.0`
/// so the form decodes, returning whether any was found.
///
/// # Errors
///
/// Returns [`InstError::NodeLink`] when a string under `"Float"` names no
/// non-finite float.
fn unmark_value(json: &mut serde_json::Value) -> Result<bool, InstError> {
    let Some(object) = json.as_object_mut() else {
        return Ok(false);
    };
    let mut found = false;
    for (variant, payload) in object.iter_mut() {
        match variant.as_str() {
            "Float" => {
                if let Some(name) = payload.as_str() {
                    if non_finite_value(name).is_none() {
                        return Err(InstError::NodeLink(format!(
                            "`{name}` is not a float; the strings allowed under \"Float\" are \"NaN\", \"Infinity\" and \"-Infinity\""
                        )));
                    }
                    *payload = serde_json::json!(0.0);
                    found = true;
                }
            }
            "List" => {
                if let Some(items) = payload.as_array_mut() {
                    for item in items {
                        found |= unmark_value(item)?;
                    }
                }
            }
            "Unknown" => {
                if let Some(fields) = payload.as_object_mut() {
                    for item in fields.values_mut() {
                        found |= unmark_value(item)?;
                    }
                }
            }
            "Opaque" => {
                if let Some(fields) = payload
                    .get_mut("fields")
                    .and_then(serde_json::Value::as_object_mut)
                {
                    for item in fields.values_mut() {
                        found |= unmark_value(item)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(found)
}

/// Put back into `value` each non-finite float its JSON form names.
fn restore_non_finite(value: &mut Value, json: &serde_json::Value) {
    match value {
        Value::Float(x) => {
            if let Some(restored) = json
                .get("Float")
                .and_then(serde_json::Value::as_str)
                .and_then(non_finite_value)
            {
                *x = restored;
            }
        }
        Value::List(items) => {
            if let Some(slots) = json.get("List").and_then(serde_json::Value::as_array) {
                for (item, slot) in items.iter_mut().zip(slots) {
                    restore_non_finite(item, slot);
                }
            }
        }
        Value::Unknown(fields) => {
            if let Some(slots) = json.get("Unknown") {
                restore_fields(fields, slots);
            }
        }
        Value::Opaque { fields, .. } => {
            if let Some(slots) = json.get("Opaque").and_then(|o| o.get("fields")) {
                restore_fields(fields, slots);
            }
        }
        _ => {}
    }
}

/// [`restore_non_finite`] over the fields of a record value.
fn restore_fields(
    fields: &mut std::collections::HashMap<String, Value>,
    slots: &serde_json::Value,
) {
    for (key, value) in fields {
        if let Some(slot) = slots.get(key) {
            restore_non_finite(value, slot);
        }
    }
}

/// Decode one node from its node-link JSON form.
///
/// A node holding no non-finite float decodes directly. One that holds some
/// fails that first attempt, since a string stands where a number belongs; it
/// is then decoded with those strings replaced and the floats put back.
fn decode_node(json: &serde_json::Value) -> Result<Node, InstError> {
    let direct_error = match Node::deserialize(json) {
        Ok(node) => return Ok(node),
        Err(err) => InstError::NodeLink(err.to_string()),
    };
    let mut clean = json.clone();
    let mut marked = false;
    for slot in value_slots_mut(&mut clean) {
        marked |= unmark_value(slot)?;
    }
    if !marked {
        return Err(direct_error);
    }
    let mut node = Node::deserialize(clean).map_err(|e| InstError::NodeLink(e.to_string()))?;
    if let Some(FieldPresence::Present(value)) = &mut node.value
        && let Some(slot) = json.get("value").and_then(|v| v.get("Present"))
    {
        restore_non_finite(value, slot);
    }
    for (field, map) in [
        ("extra_fields", &mut node.extra_fields),
        ("annotations", &mut node.annotations),
    ] {
        if let Some(slots) = json.get(field) {
            restore_fields(map, slots);
        }
    }
    Ok(node)
}

/// Read a graph instance from its node-link JSON form.
///
/// The arrays may list nodes and edges in any order, and an edge's
/// `position` may be omitted, meaning no position.
///
/// # Errors
///
/// Returns [`InstError::NodeLink`] when the document does not have the
/// node-link shape (a string under `"Float"` other than the three the module
/// documentation lists included), [`InstError::DuplicateNode`] or
/// [`InstError::DuplicateEdge`] when an id is listed twice, and
/// [`InstError::DanglingEdge`] when an edge names a node the document does
/// not list.
pub fn from_node_link_json(value: &serde_json::Value) -> Result<GInstance, InstError> {
    let doc = NodeLink::deserialize(value).map_err(|e| InstError::NodeLink(e.to_string()))?;
    let mut graph = GInstance::new();
    for node in doc.nodes {
        graph.insert_node(decode_node(&node)?)?;
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
