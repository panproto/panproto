//! JSON parsing for W-type instances.
//!
//! Converts JSON data into a [`WInstance`] guided by a schema, and
//! serializes instances back to JSON. The parser recursively walks
//! the JSON structure, matching properties to schema edges.

use std::collections::HashMap;

use panproto_schema::{Edge, Schema};
use serde_json::json;

use crate::error::ParseError;
use crate::metadata::Node;
use crate::value::{FieldPresence, Value};
use crate::wtype::WInstance;

/// Accumulated state during JSON parsing.
struct ParseState {
    nodes: HashMap<u32, Node>,
    arcs: Vec<(u32, u32, Edge)>,
    next_id: u32,
}

impl ParseState {
    fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            arcs: Vec::new(),
            next_id: 0,
        }
    }

    const fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// Parse JSON into a W-type instance, guided by a schema.
///
/// The parser starts at `root_vertex` in the schema and recursively
/// walks the JSON structure, creating nodes for each schema vertex
/// encountered. Property edges guide which JSON fields become child
/// nodes.
///
/// # Errors
///
/// Returns `ParseError` if the JSON structure doesn't match the
/// schema or contains invalid values.
pub fn parse_json(
    schema: &Schema,
    root_vertex: &str,
    json_val: &serde_json::Value,
) -> Result<WInstance, ParseError> {
    if !schema.has_vertex(root_vertex) {
        return Err(ParseError::RootVertexNotFound(root_vertex.to_string()));
    }

    let mut state = ParseState::new();
    let root_id = state.alloc_id();

    walk_json(schema, root_vertex, json_val, root_id, &mut state, "$")?;

    Ok(WInstance::new(
        state.nodes,
        state.arcs,
        Vec::new(),
        root_id,
        root_vertex.to_string(),
    ))
}

/// Recursive JSON walker.
fn walk_json(
    schema: &Schema,
    vertex_id: &str,
    json_val: &serde_json::Value,
    node_id: u32,
    state: &mut ParseState,
    path: &str,
) -> Result<(), ParseError> {
    let _vertex = schema
        .vertex(vertex_id)
        .ok_or_else(|| ParseError::RootVertexNotFound(vertex_id.to_string()))?;

    match json_val {
        serde_json::Value::Object(map) => {
            parse_object(schema, vertex_id, map, node_id, state, path)?;
        }
        serde_json::Value::Array(arr) => {
            parse_array(schema, vertex_id, arr, node_id, state, path)?;
        }
        _ => {
            // Leaf value
            let value = json_to_field_presence(json_val);
            let node = Node::new(node_id, vertex_id).with_value(value);
            state.nodes.insert(node_id, node);
        }
    }

    Ok(())
}

/// Parse a JSON object into a node with children.
fn parse_object(
    schema: &Schema,
    vertex_id: &str,
    map: &serde_json::Map<String, serde_json::Value>,
    node_id: u32,
    state: &mut ParseState,
    path: &str,
) -> Result<(), ParseError> {
    let mut node = Node::new(node_id, vertex_id);

    // Check for discriminator ($type field)
    if let Some(serde_json::Value::String(disc)) = map.get("$type") {
        node.discriminator = Some(disc.clone());
    }

    // Get outgoing edges from schema for this vertex
    let outgoing: Vec<Edge> = schema.outgoing_edges(vertex_id).to_vec();

    // Track which fields we've handled
    let mut handled_fields = std::collections::HashSet::new();

    for edge in &outgoing {
        let field_name = edge.name.as_deref().unwrap_or(&edge.tgt);
        handled_fields.insert(field_name.to_string());

        if let Some(field_val) = map.get(field_name) {
            let child_id = state.alloc_id();
            let child_path = format!("{path}.{field_name}");
            walk_json(schema, &edge.tgt, field_val, child_id, state, &child_path)?;
            state.arcs.push((node_id, child_id, edge.clone()));
        }
    }

    // Preserve unhandled fields as extra_fields
    for (key, val) in map {
        if key == "$type" || handled_fields.contains(key.as_str()) {
            continue;
        }
        node.extra_fields
            .insert(key.clone(), json_value_to_value(val));
    }

    state.nodes.insert(node_id, node);
    Ok(())
}

/// Parse a JSON array into a node with item children.
fn parse_array(
    schema: &Schema,
    vertex_id: &str,
    arr: &[serde_json::Value],
    node_id: u32,
    state: &mut ParseState,
    path: &str,
) -> Result<(), ParseError> {
    let node = Node::new(node_id, vertex_id);
    state.nodes.insert(node_id, node);

    // Array items: look for outgoing "item" edges
    let outgoing: Vec<Edge> = schema.outgoing_edges(vertex_id).to_vec();
    let item_edge = outgoing
        .iter()
        .find(|e| e.kind == "item" || e.name.as_deref() == Some("item"));

    if let Some(edge) = item_edge {
        for (i, item) in arr.iter().enumerate() {
            let child_id = state.alloc_id();
            let child_path = format!("{path}[{i}]");
            walk_json(schema, &edge.tgt, item, child_id, state, &child_path)?;
            state.arcs.push((node_id, child_id, edge.clone()));
        }
    }
    Ok(())
}

/// Convert a JSON value to a `FieldPresence`.
fn json_to_field_presence(val: &serde_json::Value) -> FieldPresence {
    match val {
        serde_json::Value::Null => FieldPresence::Null,
        serde_json::Value::Bool(b) => FieldPresence::Present(Value::Bool(*b)),
        serde_json::Value::Number(n) => n.as_i64().map_or_else(
            || {
                n.as_f64().map_or_else(
                    || FieldPresence::Present(Value::Str(n.to_string())),
                    |f| FieldPresence::Present(Value::Float(f)),
                )
            },
            |i| FieldPresence::Present(Value::Int(i)),
        ),
        serde_json::Value::String(s) => FieldPresence::Present(Value::Str(s.clone())),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            FieldPresence::Present(json_value_to_value(val))
        }
    }
}

/// Convert a `serde_json::Value` to our `Value` type.
fn json_value_to_value(val: &serde_json::Value) -> Value {
    match val {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => n.as_i64().map_or_else(
            || {
                n.as_f64()
                    .map_or_else(|| Value::Str(n.to_string()), Value::Float)
            },
            Value::Int,
        ),
        serde_json::Value::String(s) => Value::Str(s.clone()),
        serde_json::Value::Array(arr) => {
            let mut fields = HashMap::new();
            for (i, item) in arr.iter().enumerate() {
                fields.insert(i.to_string(), json_value_to_value(item));
            }
            Value::Unknown(fields)
        }
        serde_json::Value::Object(map) => {
            let fields: HashMap<String, Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_value_to_value(v)))
                .collect();
            Value::Unknown(fields)
        }
    }
}

/// Serialize a W-type instance to JSON.
///
/// Reconstructs the JSON structure by walking the instance tree
/// from the root, using schema edges as property names.
#[must_use]
pub fn to_json(schema: &Schema, instance: &WInstance) -> serde_json::Value {
    node_to_json(schema, instance, instance.root)
}

/// Recursively convert a node to JSON.
fn node_to_json(schema: &Schema, instance: &WInstance, node_id: u32) -> serde_json::Value {
    let Some(node) = instance.node(node_id) else {
        return serde_json::Value::Null;
    };

    let vertex = schema.vertex(&node.anchor);
    let is_array_like = vertex.is_some_and(|v| v.kind == "array");

    // Leaf node: return value directly
    if let Some(ref presence) = node.value {
        return match presence {
            FieldPresence::Present(val) => value_to_json(val),
            FieldPresence::Null | FieldPresence::Absent => serde_json::Value::Null,
        };
    }

    // Array node
    if is_array_like {
        let children = instance.children(node_id);
        let items: Vec<serde_json::Value> = children
            .iter()
            .map(|&child_id| node_to_json(schema, instance, child_id))
            .collect();
        return serde_json::Value::Array(items);
    }

    // Object node: reconstruct as JSON object
    let mut map = serde_json::Map::new();

    // Add discriminator if present
    if let Some(ref disc) = node.discriminator {
        map.insert("$type".to_string(), json!(disc));
    }

    // Add children as properties
    for &(parent, child, ref edge) in &instance.arcs {
        if parent == node_id {
            let field_name = edge.name.as_deref().unwrap_or(&edge.tgt);
            map.insert(
                field_name.to_string(),
                node_to_json(schema, instance, child),
            );
        }
    }

    // Add extra fields
    for (key, val) in &node.extra_fields {
        map.insert(key.clone(), value_to_json(val));
    }

    serde_json::Value::Object(map)
}

/// Convert a `Value` to a `serde_json::Value`.
fn value_to_json(val: &Value) -> serde_json::Value {
    match val {
        Value::Bool(b) => json!(b),
        Value::Int(i) => json!(i),
        Value::Float(f) => json!(f),
        Value::Str(s) => json!(s),
        Value::Bytes(b) => serde_json::Value::String(base64_encode(b)),
        Value::CidLink(s) => json!({"$link": s}),
        Value::Blob { ref_, mime, size } => {
            json!({"$type": "blob", "ref": ref_, "mimeType": mime, "size": size})
        }
        Value::Token(t) => json!(t),
        Value::Null => serde_json::Value::Null,
        Value::Opaque { type_, fields } => {
            let mut map = serde_json::Map::new();
            map.insert("$type".to_string(), json!(type_));
            for (k, v) in fields {
                map.insert(k.clone(), value_to_json(v));
            }
            serde_json::Value::Object(map)
        }
        Value::Unknown(fields) => {
            let map: serde_json::Map<String, serde_json::Value> = fields
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
    }
}

/// Simple base64 encoding (no padding).
fn base64_encode(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(chunk.get(1).copied().unwrap_or_default());
        let b2 = u32::from(chunk.get(2).copied().unwrap_or_default());
        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::smallvec;

    /// Build a minimal schema for testing.
    fn test_schema() -> Schema {
        let mut vertices = HashMap::new();
        vertices.insert(
            "post:body".into(),
            panproto_schema::Vertex {
                id: "post:body".into(),
                kind: "object".into(),
                nsid: None,
            },
        );
        vertices.insert(
            "post:body.text".into(),
            panproto_schema::Vertex {
                id: "post:body.text".into(),
                kind: "string".into(),
                nsid: None,
            },
        );
        vertices.insert(
            "post:body.createdAt".into(),
            panproto_schema::Vertex {
                id: "post:body.createdAt".into(),
                kind: "string".into(),
                nsid: None,
            },
        );

        let text_edge = Edge {
            src: "post:body".into(),
            tgt: "post:body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let date_edge = Edge {
            src: "post:body".into(),
            tgt: "post:body.createdAt".into(),
            kind: "prop".into(),
            name: Some("createdAt".into()),
        };

        let mut edges = HashMap::new();
        edges.insert(text_edge.clone(), "prop".into());
        edges.insert(date_edge.clone(), "prop".into());

        let mut outgoing = HashMap::new();
        outgoing.insert(
            "post:body".into(),
            smallvec![text_edge.clone(), date_edge.clone()],
        );

        let mut incoming = HashMap::new();
        incoming.insert("post:body.text".into(), smallvec![text_edge.clone()]);
        incoming.insert("post:body.createdAt".into(), smallvec![date_edge.clone()]);

        let mut between = HashMap::new();
        between.insert(
            ("post:body".into(), "post:body.text".into()),
            smallvec![text_edge],
        );
        between.insert(
            ("post:body".into(), "post:body.createdAt".into()),
            smallvec![date_edge],
        );

        Schema {
            protocol: "test".into(),
            vertices,
            edges,
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            outgoing,
            incoming,
            between,
        }
    }

    #[test]
    fn parse_json_simple_object() {
        let schema = test_schema();
        let json_val = json!({
            "text": "hello world",
            "createdAt": "2024-01-01T00:00:00Z"
        });

        let result = parse_json(&schema, "post:body", &json_val);
        assert!(result.is_ok(), "parse failed: {result:?}");

        let inst = result
            .unwrap_or_else(|_| WInstance::new(HashMap::new(), vec![], vec![], 0, String::new()));
        assert_eq!(inst.node_count(), 3);
        assert_eq!(inst.arc_count(), 2);
    }

    #[test]
    fn json_round_trip() {
        let schema = test_schema();
        let json_val = json!({
            "text": "hello world",
            "createdAt": "2024-01-01T00:00:00Z"
        });

        let inst = parse_json(&schema, "post:body", &json_val);
        assert!(inst.is_ok());
        let inst = inst
            .unwrap_or_else(|_| WInstance::new(HashMap::new(), vec![], vec![], 0, String::new()));

        let output = to_json(&schema, &inst);
        assert!(output.is_object());
        assert_eq!(output["text"], "hello world");
        assert_eq!(output["createdAt"], "2024-01-01T00:00:00Z");
    }

    #[test]
    fn parse_json_missing_root_vertex() {
        let schema = test_schema();
        let json_val = json!({"text": "hello"});
        let result = parse_json(&schema, "nonexistent", &json_val);
        assert!(result.is_err());
    }
}
