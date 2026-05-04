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
        panproto_gat::Name::from(root_vertex),
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
        node.discriminator = Some(panproto_gat::Name::from(disc.as_str()));
    }

    // Get outgoing edges from schema for this vertex
    let outgoing: Vec<Edge> = schema.outgoing_edges(vertex_id).to_vec();

    // Track which fields we've handled
    let mut handled_fields = std::collections::HashSet::new();

    for edge in &outgoing {
        let field_name = edge.name.as_deref().unwrap_or(&*edge.tgt);
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
///
/// The item edge is identified by the generic free-schema rule: a list
/// vertex has a single anonymous outgoing edge (`name == None`), which
/// is the item slot. This is protocol-agnostic and matches the rule in
/// [`is_list_vertex`] used by `to_json`; the two must agree so that
/// parse/serialize is a round-trip on list vertices regardless of which
/// string the protocol uses to name the edge kind ("items", "line-of",
/// "element", etc.).
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

    let outgoing: Vec<Edge> = schema.outgoing_edges(vertex_id).to_vec();
    // Prefer the first anonymous outgoing edge. In a well-formed list
    // vertex there is exactly one, but if the schema happens to carry
    // additional named edges we still pick the anonymous one as the
    // item slot (named edges would be field projections, not items).
    let item_edge = outgoing.iter().find(|e| e.name.is_none());

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
///
/// JSON arrays map to [`Value::List`] and JSON objects map to
/// [`Value::Unknown`]. The two branches are the categorical
/// constructors for the free JSON-like term algebra, so
/// `json_value_to_value` is a faithful embedding: every
/// `serde_json::Value` has a unique preimage (up to the numeric
/// `Int`/`Float` split dictated by the source) and survives a
/// `value_to_json` round trip.
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
        serde_json::Value::Array(arr) => Value::List(arr.iter().map(json_value_to_value).collect()),
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

    // Leaf node: return value directly
    if let Some(ref presence) = node.value {
        return match presence {
            FieldPresence::Present(val) => value_to_json(val),
            FieldPresence::Null | FieldPresence::Absent => serde_json::Value::Null,
        };
    }

    // List (ordered-collection) node. We treat a node as a list when
    // any of three signals fires:
    //
    // 1. The schema marks the anchor vertex as a list (its outgoing
    //    edges are nonempty and all anonymous), or
    // 2. The node carries the `$list` annotation set by the CST
    //    extractor at parse time (catches empty and singleton arrays
    //    that the instance-arc heuristic cannot distinguish from
    //    plain `{ "item": x }` objects), or
    // 3. The instance carries multiple outgoing arcs that share the
    //    same `(kind, name)` pair (recovers list shape when the
    //    annotation is absent, e.g. instances built by callers that
    //    bypass the CST extraction path).
    let list_via_schema = is_list_vertex(schema, &node.anchor);
    let list_via_annotation = node.is_list();
    let list_via_instance_arcs = is_list_via_instance_arcs(instance, node_id);
    // The schema-shape signal (`list_via_schema`) is a heuristic: it fires
    // whenever every outgoing edge is anonymous, which is also true of a
    // hand-built record whose author didn't supply edge names. Object-only
    // signals on the *node* (a discriminator or extra_fields populated by
    // the parser when the JSON was an object) are direct evidence the data
    // is map-shaped, so they veto the schema heuristic. The CST `$list`
    // annotation and the structural same-name-arcs signal are not vetoed:
    // both are positive evidence about the data, not the schema, and
    // cannot coexist with object content.
    let object_only_signals = !node.extra_fields.is_empty() || node.discriminator.is_some();
    let is_list =
        (list_via_schema && !object_only_signals) || list_via_annotation || list_via_instance_arcs;
    if is_list {
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
        map.insert("$type".to_string(), json!(&**disc));
    }

    // Add children as properties
    for &(parent, child, ref edge) in &instance.arcs {
        if parent == node_id {
            let field_name = edge.name.as_deref().unwrap_or(&*edge.tgt);
            map.insert(
                field_name.to_string(),
                node_to_json(schema, instance, child),
            );
        }
    }

    // Add extra fields. These are serialized AFTER children: when both
    // contain the same key, extra_fields take precedence. This is by
    // design: field transforms (`ComputeField` deriving from child
    // scalars, `ApplyExpr` transforming a child scalar value) write
    // results to extra_fields, and the transform output must be
    // authoritative over the original child value.
    for (key, val) in &node.extra_fields {
        map.insert(key.clone(), value_to_json(val));
    }

    serde_json::Value::Object(map)
}

/// Convert a `Value` to a `serde_json::Value`.
///
/// This is the right inverse of [`json_value_to_value`] on the image
/// of that map: if `v = json_value_to_value(j)`, then
/// `value_to_json(&v)` returns `j` up to the `Int`/`Float` normalization
/// performed on numeric literals. In particular, [`Value::List`]
/// round-trips to a JSON array and [`Value::Unknown`] to a JSON object.
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
        Value::List(items) => serde_json::Value::Array(items.iter().map(value_to_json).collect()),
    }
}

/// Decide whether a schema vertex should be rendered as a JSON list.
///
/// A vertex is a **list vertex** iff its outgoing edges in the schema
/// are nonempty and all anonymous (i.e. every edge has `name == None`).
/// Intuitively, a record sort has one projection per named field, while
/// a list sort has a single anonymous "item" edge (possibly repeated in
/// the instance). Anonymous edges therefore identify exactly the free
/// list / free monoid constructor in the schema theory, independent of
/// the protocol-specific spelling of the vertex kind.
///
/// This is the category-theoretically generic rule: it does not depend
/// on string-matching the vertex kind (`"array"`, `"list"`,
/// `"sequence"`, etc.) or on any particular protocol's edge-kind
/// convention. Any schema that encodes ordered collections via repeated
/// unnamed edges is covered.
fn is_list_vertex(schema: &Schema, vertex_id: &str) -> bool {
    let outgoing = schema.outgoing_edges(vertex_id);
    !outgoing.is_empty() && outgoing.iter().all(|e| e.name.is_none())
}

/// Detect a list-shaped node from its instance arcs.
///
/// Returns `true` when the node has at least two outgoing arcs that
/// all share the same `(kind, name)` pair. Two same-named children
/// cannot be expressed as a JSON object (duplicate keys are
/// disallowed), so the only consistent serialization is a JSON array.
/// In particular, this catches the synthetic `"item"` arcs that the
/// open-schema CST extractor emits for every array element.
fn is_list_via_instance_arcs(instance: &WInstance, node_id: u32) -> bool {
    let mut signature: Option<(panproto_gat::Name, Option<panproto_gat::Name>)> = None;
    let mut count = 0_usize;
    for &(parent, _, ref edge) in &instance.arcs {
        if parent != node_id {
            continue;
        }
        let key = (edge.kind.clone(), edge.name.clone());
        match &signature {
            Some(existing) if existing != &key => return false,
            Some(_) => {}
            None => signature = Some(key),
        }
        count += 1;
    }
    count >= 2
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_schema::{Protocol, SchemaBuilder};
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
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
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

        let inst = result.unwrap_or_else(|_| {
            WInstance::new(
                HashMap::new(),
                vec![],
                vec![],
                0,
                panproto_gat::Name::default(),
            )
        });
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
        let inst = inst.unwrap_or_else(|_| {
            WInstance::new(
                HashMap::new(),
                vec![],
                vec![],
                0,
                panproto_gat::Name::default(),
            )
        });

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

    #[test]
    fn parse_array_with_items_edge_kind() {
        // Protocols declare the array edge kind as "items" (plural);
        // the parser must match that spelling rather than the singular
        // "item", which is a plausible typo.
        let proto = Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into(), "array".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&proto)
            .vertex("root", "object", None::<&str>)
            .unwrap()
            .vertex("root.tags", "array", None::<&str>)
            .unwrap()
            .vertex("tag", "string", None::<&str>)
            .unwrap()
            .edge("root", "root.tags", "prop", Some("tags"))
            .unwrap()
            .edge("root.tags", "tag", "items", None::<&str>)
            .unwrap()
            .build()
            .unwrap();

        let json_val = json!({"tags": ["alpha", "beta", "gamma"]});
        let inst = parse_json(&schema, "root", &json_val).unwrap();

        let output = to_json(&schema, &inst);
        assert!(output["tags"].is_array());
        let tags = output["tags"].as_array().unwrap();
        assert_eq!(tags.len(), 3, "array elements should not be dropped");
        assert_eq!(tags[0], "alpha");
        assert_eq!(tags[1], "beta");
        assert_eq!(tags[2], "gamma");
    }

    // ── Tests for issue #27: generic list detection + Value::List ──────

    /// Build a minimal schema with an object vertex that carries a
    /// list-vertex as a property, where the list-vertex is distinguished
    /// only by having a single anonymous outgoing edge. The vertex kind
    /// is deliberately NOT named `"array"` to prove the generic rule is
    /// not relying on any particular protocol's kind string.
    fn list_schema_with_kind(list_vertex_kind: &str) -> Schema {
        let proto = Protocol {
            name: "generic".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into(), list_vertex_kind.into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        };
        SchemaBuilder::new(&proto)
            .vertex("root", "object", None::<&str>)
            .unwrap()
            .vertex("root.items", list_vertex_kind, None::<&str>)
            .unwrap()
            .vertex("item", "string", None::<&str>)
            .unwrap()
            .edge("root", "root.items", "prop", Some("items"))
            .unwrap()
            // Anonymous edge from list-vertex to element type:
            // this is what marks the vertex as a list.
            .edge("root.items", "item", "anonymous-edge-kind", None::<&str>)
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    fn to_json_emits_list_regardless_of_kind_string() {
        // The generic rule is "all outgoing edges are anonymous," not
        // "vertex.kind == array". Prove it by using a vertex kind
        // that isn't `"array"`: `"sequence"`, `"list"`, `"bag"`, etc.
        for kind in ["sequence", "list", "bag", "ordered-multi"] {
            let schema = list_schema_with_kind(kind);
            let input = json!({"items": ["alpha", "beta"]});
            let inst = parse_json(&schema, "root", &input).unwrap();
            let output = to_json(&schema, &inst);
            assert!(
                output["items"].is_array(),
                "kind={kind}: expected JSON array, got {}",
                output["items"]
            );
            assert_eq!(output["items"][0], "alpha", "kind={kind}");
            assert_eq!(output["items"][1], "beta", "kind={kind}");
        }
    }

    #[test]
    fn is_list_vertex_detects_by_anonymous_edges() {
        // All outgoing edges anonymous → list.
        let list_schema = list_schema_with_kind("whatever");
        assert!(
            is_list_vertex(&list_schema, "root.items"),
            "a vertex with only anonymous outgoing edges is a list vertex"
        );

        // Named outgoing edges → not a list (it's a record).
        assert!(
            !is_list_vertex(&list_schema, "root"),
            "a vertex with named outgoing edges is a record vertex, not a list"
        );

        // No outgoing edges → not a list (it's a leaf).
        assert!(
            !is_list_vertex(&list_schema, "item"),
            "a leaf vertex with no outgoing edges is not a list vertex"
        );
    }

    #[test]
    fn to_json_empty_list_vertex_renders_as_empty_json_array() {
        // A list-vertex with zero children must render as `[]`. The
        // schema-based rule for picking array vs object renderings has
        // to run before the child-count check, otherwise an empty list
        // renders as `{}`.
        let schema = list_schema_with_kind("collection");
        let input = json!({"items": []});
        let inst = parse_json(&schema, "root", &input).unwrap();
        let output = to_json(&schema, &inst);
        assert_eq!(output["items"], json!([]));
    }

    #[test]
    fn json_value_to_value_preserves_array_as_list() {
        // json_value_to_value is the embedding serde_json::Value ↪ Value;
        // it must map arrays to Value::List (not Value::Unknown with
        // stringly keys) or the embedding is not faithful.
        let input = json!([1, "two", true, null]);
        let v = json_value_to_value(&input);
        match v {
            Value::List(items) => {
                assert_eq!(items.len(), 4);
                assert_eq!(items[0], Value::Int(1));
                assert_eq!(items[1], Value::Str("two".into()));
                assert_eq!(items[2], Value::Bool(true));
                assert_eq!(items[3], Value::Null);
            }
            other => panic!("expected Value::List, got {other:?}"),
        }
    }

    #[test]
    fn value_to_json_renders_list_as_json_array() {
        let v = Value::List(vec![
            Value::Int(1),
            Value::Str("two".into()),
            Value::Bool(true),
            Value::Null,
        ]);
        let j = value_to_json(&v);
        assert_eq!(j, json!([1, "two", true, null]));
    }

    #[test]
    fn value_json_round_trip_is_faithful_for_arrays() {
        // The composition value_to_json ∘ json_value_to_value should be
        // the identity on the JSON subalgebra (up to the Int/Float
        // numeric normalization). Exercise it on nested arrays and
        // arrays-of-objects to confirm faithfulness.
        let cases = vec![
            json!([]),
            json!(["en"]),
            json!(["panproto", "atproto", "schemas"]),
            json!([[1, 2], [3, 4]]),
            json!([{"a": 1}, {"b": 2}]),
            json!({"tags": ["x", "y"]}),
            json!({"nested": {"tags": ["x", "y"]}}),
        ];
        for original in cases {
            let roundtrip = value_to_json(&json_value_to_value(&original));
            assert_eq!(
                roundtrip, original,
                "round trip should be faithful for {original}"
            );
        }
    }

    #[test]
    fn to_json_extra_field_array_round_trips_via_value_list() {
        // A field that lands in `extra_fields` (no matching schema
        // edge) carries a JSON array. The emitter must preserve the
        // array shape rather than flatten it to a stringly-keyed
        // object like `{"0": "en"}`.
        let schema = test_schema();
        let input = json!({
            "text": "Hello",
            "createdAt": "2024-01-15T12:00:00.000Z",
            "langs": ["en"],
            "tags": ["panproto", "atproto", "schemas"]
        });

        let inst = parse_json(&schema, "post:body", &input).unwrap();
        let output = to_json(&schema, &inst);

        // Schema-anchored fields survive as usual.
        assert_eq!(output["text"], "Hello");
        assert_eq!(output["createdAt"], "2024-01-15T12:00:00.000Z");

        // Extra-field arrays must come out as JSON arrays, not objects
        // with numeric string keys.
        assert!(
            output["langs"].is_array(),
            "langs should be a JSON array, got {}",
            output["langs"]
        );
        assert_eq!(output["langs"], json!(["en"]));

        assert!(
            output["tags"].is_array(),
            "tags should be a JSON array, got {}",
            output["tags"]
        );
        assert_eq!(output["tags"], json!(["panproto", "atproto", "schemas"]));
    }

    #[test]
    fn to_json_record_with_anonymous_edges_emits_extra_fields_not_empty_array() {
        // Regression for issues #54 and #55: a hand-built schema whose
        // record vertex happens to have only anonymous outgoing edges
        // (e.g. a Python caller using SchemaBuilder.edge(..., name=None))
        // was being classified as a list by the schema heuristic. The
        // parser correctly preserves unhandled JSON keys in
        // `extra_fields`, but the emitter then dropped them and rendered
        // the node as `[]`. Object-only signals on the node (extra_fields
        // populated, or a discriminator) must veto the schema heuristic.
        let proto = Protocol {
            name: "test".into(),
            schema_theory: "ThTestSchema".into(),
            instance_theory: "ThTestInstance".into(),
            edge_rules: vec![],
            obj_kinds: vec!["record".into(), "field".into(), "long".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&proto)
            .vertex("event", "record", Some("Event"))
            .unwrap()
            .vertex("event.tick", "field", Some("tick"))
            .unwrap()
            .vertex("event.tick:t", "long", None::<&str>)
            .unwrap()
            // Anonymous edges (name=None). This is what the user's
            // didactic-style hand-built schema looks like.
            .edge("event", "event.tick", "field-of", None::<&str>)
            .unwrap()
            .edge("event.tick", "event.tick:t", "type-of", None::<&str>)
            .unwrap()
            .build()
            .unwrap();

        let input = json!({"tick": 480});
        let inst = parse_json(&schema, "event", &input).unwrap();

        // The data is preserved on the parse side as an extra_field on
        // the root: there is no schema edge named "tick", so the parser
        // routes it to extra_fields rather than dropping it.
        let output = to_json(&schema, &inst);
        assert!(
            output.is_object(),
            "node with extra_fields must emit as an object, not an array; got {output}"
        );
        assert_eq!(
            output["tick"], 480,
            "extra_fields content must round-trip through to_json"
        );
    }
}
