//! `AsyncAPI` protocol definition.
//!
//! `AsyncAPI` uses a constrained multigraph schema theory
//! (`colimit(ThGraph, ThConstraint, ThMulti)`) and a W-type
//! instance theory (`ThWType`).
//!
//! Vertex kinds: channel, operation, message, schema-object, server,
//! string, integer, number, boolean, array, object.
//!
//! Edge kinds: prop, items, variant, ref.

use std::collections::HashMap;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the `AsyncAPI` protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "asyncapi".into(),
        schema_theory: "ThAsyncAPISchema".into(),
        instance_theory: "ThAsyncAPIInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "channel".into(),
            "operation".into(),
            "message".into(),
            "schema-object".into(),
            "server".into(),
            "string".into(),
            "integer".into(),
            "number".into(),
            "boolean".into(),
            "array".into(),
            "object".into(),
        ],
        constraint_sorts: vec![
            "required".into(),
            "format".into(),
            "enum".into(),
            "default".into(),
        ],
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        has_causal: true,
        nominal_identity: true,
    }
}

/// Register the component GATs for `AsyncAPI` with a theory registry.
pub fn register_theories<S: ::std::hash::BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThAsyncAPISchema",
        "ThAsyncAPIInstance",
    );
}

/// Parse an `AsyncAPI` JSON document into a [`Schema`].
///
/// Walks servers, channels, operations, messages, and schemas.
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing or schema construction fails.
pub fn parse_asyncapi(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut counter: usize = 0;

    // Servers.
    if let Some(servers) = json.get("servers").and_then(serde_json::Value::as_object) {
        for (server_name, _server_obj) in servers {
            let server_id = format!("server:{server_name}");
            builder = builder.vertex(&server_id, "server", None)?;
        }
    }

    // Walk channels.
    if let Some(channels) = json.get("channels").and_then(serde_json::Value::as_object) {
        for (channel_name, channel_obj) in channels {
            let channel_id = format!("channel:{channel_name}");
            builder = builder.vertex(&channel_id, "channel", None)?;

            // AsyncAPI v2: subscribe/publish operations.
            for op_type in &["subscribe", "publish"] {
                if let Some(op) = channel_obj.get(*op_type) {
                    let op_id = format!("{channel_id}:{op_type}");
                    builder = builder.vertex(&op_id, "operation", None)?;
                    builder = builder.edge(&channel_id, &op_id, "prop", Some(op_type))?;

                    // Message.
                    if let Some(message) = op.get("message") {
                        let msg_id = format!("{op_id}:message");
                        builder = builder.vertex(&msg_id, "message", None)?;
                        builder = builder.edge(&op_id, &msg_id, "prop", Some("message"))?;

                        // Message payload schema.
                        if let Some(payload) = message.get("payload") {
                            let payload_id = format!("{msg_id}:payload");
                            builder = walk_schema(builder, payload, &payload_id, &mut counter)?;
                            builder =
                                builder.edge(&msg_id, &payload_id, "prop", Some("payload"))?;
                        }
                    }
                }
            }

            // AsyncAPI v3: messages at channel level.
            if let Some(messages) = channel_obj
                .get("messages")
                .and_then(serde_json::Value::as_object)
            {
                for (msg_name, msg_obj) in messages {
                    let msg_id = format!("{channel_id}:msg:{msg_name}");
                    builder = builder.vertex(&msg_id, "message", None)?;
                    builder = builder.edge(&channel_id, &msg_id, "prop", Some(msg_name))?;

                    if let Some(payload) = msg_obj.get("payload") {
                        let payload_id = format!("{msg_id}:payload");
                        builder = walk_schema(builder, payload, &payload_id, &mut counter)?;
                        builder = builder.edge(&msg_id, &payload_id, "prop", Some("payload"))?;
                    }
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Recursively walk a JSON Schema object within an `AsyncAPI` spec.
fn walk_schema(
    mut builder: SchemaBuilder,
    schema: &serde_json::Value,
    current_id: &str,
    counter: &mut usize,
) -> Result<SchemaBuilder, ProtocolError> {
    let type_str = schema
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("object");

    let kind = match type_str {
        "string" => "string",
        "integer" => "integer",
        "number" => "number",
        "boolean" => "boolean",
        "array" => "array",
        _ => "object",
    };

    builder = builder.vertex(current_id, kind, None)?;

    // Constraints.
    if let Some(fmt) = schema.get("format").and_then(serde_json::Value::as_str) {
        builder = builder.constraint(current_id, "format", fmt);
    }
    if let Some(default_val) = schema.get("default") {
        let val_str = match default_val {
            serde_json::Value::String(s) => s.clone(),
            _ => default_val.to_string(),
        };
        builder = builder.constraint(current_id, "default", &val_str);
    }
    if let Some(enum_val) = schema.get("enum").and_then(serde_json::Value::as_array) {
        let vals: Vec<String> = enum_val
            .iter()
            .map(|v| v.as_str().map_or_else(|| v.to_string(), String::from))
            .collect();
        builder = builder.constraint(current_id, "enum", &vals.join(","));
    }

    // Properties.
    if let Some(properties) = schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
    {
        let required_fields: Vec<&str> = schema
            .get("required")
            .and_then(serde_json::Value::as_array)
            .map(|arr| arr.iter().filter_map(serde_json::Value::as_str).collect())
            .unwrap_or_default();

        for (prop_name, prop_schema) in properties {
            let prop_id = format!("{current_id}.{prop_name}");
            builder = walk_schema(builder, prop_schema, &prop_id, counter)?;
            builder = builder.edge(current_id, &prop_id, "prop", Some(prop_name))?;
            if required_fields.contains(&prop_name.as_str()) {
                builder = builder.constraint(&prop_id, "required", "true");
            }
        }
    }

    // Items.
    if let Some(items) = schema.get("items") {
        let items_id = format!("{current_id}:items");
        builder = walk_schema(builder, items, &items_id, counter)?;
        builder = builder.edge(current_id, &items_id, "items", None)?;
    }

    // oneOf / anyOf.
    for combiner in &["oneOf", "anyOf"] {
        if let Some(arr) = schema.get(*combiner).and_then(serde_json::Value::as_array) {
            for (i, sub_schema) in arr.iter().enumerate() {
                *counter += 1;
                let sub_id = format!("{current_id}:{combiner}{i}_{counter}");
                builder = walk_schema(builder, sub_schema, &sub_id, counter)?;
                builder = builder.edge(current_id, &sub_id, "variant", Some(combiner))?;
            }
        }
    }

    Ok(builder)
}

/// Emit a [`Schema`] as an `AsyncAPI` JSON document.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_asyncapi(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let mut channels = serde_json::Map::new();
    let mut servers = serde_json::Map::new();

    let roots = find_roots(schema, &["prop", "items", "variant", "ref"]);

    for root in &roots {
        match root.kind.as_str() {
            "channel" => {
                let channel_name = root.id.strip_prefix("channel:").unwrap_or(&root.id);
                let mut channel_obj = serde_json::Map::new();

                for (edge, child) in children_by_edge(schema, &root.id, "prop") {
                    if child.kind == "operation" {
                        let op_name = edge.name.as_deref().unwrap_or("subscribe");
                        let mut op_obj = serde_json::Map::new();

                        for (msg_edge, msg_v) in children_by_edge(schema, &child.id, "prop") {
                            if msg_v.kind == "message" {
                                let mut msg_obj = serde_json::Map::new();
                                for (p_edge, _p_v) in children_by_edge(schema, &msg_v.id, "prop") {
                                    if p_edge.name.as_deref() == Some("payload") {
                                        msg_obj.insert(
                                            "payload".into(),
                                            emit_schema_value(schema, &p_edge.tgt),
                                        );
                                    }
                                }
                                op_obj.insert(
                                    msg_edge.name.as_deref().unwrap_or("message").to_string(),
                                    serde_json::Value::Object(msg_obj),
                                );
                            }
                        }

                        channel_obj.insert(op_name.to_string(), serde_json::Value::Object(op_obj));
                    }
                }

                channels.insert(
                    channel_name.to_string(),
                    serde_json::Value::Object(channel_obj),
                );
            }
            "server" => {
                let server_name = root.id.strip_prefix("server:").unwrap_or(&root.id);
                servers.insert(
                    server_name.to_string(),
                    serde_json::json!({"protocol": "mqtt"}),
                );
            }
            _ => {}
        }
    }

    let mut result = serde_json::Map::new();
    result.insert("asyncapi".into(), serde_json::Value::String("2.6.0".into()));
    result.insert(
        "info".into(),
        serde_json::json!({"title": "Generated", "version": "1.0.0"}),
    );
    if !servers.is_empty() {
        result.insert("servers".into(), serde_json::Value::Object(servers));
    }
    result.insert("channels".into(), serde_json::Value::Object(channels));

    Ok(serde_json::Value::Object(result))
}

/// Emit a schema vertex as a JSON Schema value.
fn emit_schema_value(schema: &Schema, vertex_id: &str) -> serde_json::Value {
    let Some(vertex) = schema.vertices.get(vertex_id) else {
        return serde_json::Value::Object(serde_json::Map::new());
    };

    let mut obj = serde_json::Map::new();

    let type_str = match vertex.kind.as_str() {
        "string" => Some("string"),
        "integer" => Some("integer"),
        "number" => Some("number"),
        "boolean" => Some("boolean"),
        "array" => Some("array"),
        "object" | "schema-object" => Some("object"),
        _ => None,
    };
    if let Some(t) = type_str {
        obj.insert("type".into(), serde_json::Value::String(t.into()));
    }

    if let Some(fmt) = constraint_value(schema, vertex_id, "format") {
        obj.insert("format".into(), serde_json::Value::String(fmt.to_string()));
    }

    // Properties.
    let props = children_by_edge(schema, vertex_id, "prop");
    if !props.is_empty() {
        let mut properties = serde_json::Map::new();
        for (edge, _child) in &props {
            let name = edge.name.as_deref().unwrap_or("");
            properties.insert(name.to_string(), emit_schema_value(schema, &edge.tgt));
        }
        obj.insert("properties".into(), serde_json::Value::Object(properties));
    }

    // Items.
    let items = children_by_edge(schema, vertex_id, "items");
    if let Some((edge, _)) = items.first() {
        obj.insert("items".into(), emit_schema_value(schema, &edge.tgt));
    }

    serde_json::Value::Object(obj)
}

/// Well-formedness rules for `AsyncAPI` edges.
fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec![
                "channel".into(),
                "operation".into(),
                "message".into(),
                "object".into(),
                "schema-object".into(),
            ],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec!["array".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "variant".into(),
            src_kinds: vec![],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "ref".into(),
            src_kinds: vec![],
            tgt_kinds: vec![],
        },
    ]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_def() {
        let p = protocol();
        assert_eq!(p.name, "asyncapi");
        assert_eq!(p.schema_theory, "ThAsyncAPISchema");
        assert_eq!(p.instance_theory, "ThAsyncAPIInstance");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThAsyncAPISchema"));
        assert!(registry.contains_key("ThAsyncAPIInstance"));
    }

    #[test]
    fn parse_minimal() {
        let doc = serde_json::json!({
            "asyncapi": "2.6.0",
            "info": {"title": "Test", "version": "1.0.0"},
            "channels": {
                "user/signedup": {
                    "subscribe": {
                        "message": {
                            "payload": {
                                "type": "object",
                                "properties": {
                                    "email": {"type": "string"}
                                }
                            }
                        }
                    }
                }
            }
        });
        let schema = parse_asyncapi(&doc).expect("should parse");
        assert!(schema.has_vertex("channel:user/signedup"));
        assert!(schema.has_vertex("channel:user/signedup:subscribe"));
    }

    #[test]
    fn emit_minimal() {
        let doc = serde_json::json!({
            "asyncapi": "2.6.0",
            "info": {"title": "Test", "version": "1.0.0"},
            "channels": {
                "events": {
                    "subscribe": {
                        "message": {
                            "payload": {"type": "string"}
                        }
                    }
                }
            }
        });
        let schema = parse_asyncapi(&doc).expect("should parse");
        let emitted = emit_asyncapi(&schema).expect("should emit");
        assert!(emitted.get("channels").is_some());
    }

    #[test]
    fn roundtrip() {
        let doc = serde_json::json!({
            "asyncapi": "2.6.0",
            "info": {"title": "Test", "version": "1.0.0"},
            "channels": {
                "events": {
                    "publish": {
                        "message": {
                            "payload": {"type": "integer"}
                        }
                    }
                }
            }
        });
        let schema = parse_asyncapi(&doc).expect("parse");
        let emitted = emit_asyncapi(&schema).expect("emit");
        let schema2 = parse_asyncapi(&emitted).expect("re-parse");
        assert_eq!(schema.vertices.len(), schema2.vertices.len());
    }
}
