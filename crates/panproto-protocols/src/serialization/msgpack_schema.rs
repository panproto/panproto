//! MessagePack Schema protocol definition.
//!
//! MessagePack Schema uses a constrained multigraph + W-type theory (Group A):
//! `colimit(ThGraph, ThConstraint, ThMulti)` + `ThWType`.
//!
//! Vertex kinds: object, field, array, string, integer, boolean, float,
//!               binary, timestamp, extension, nil, union.
//! Edge kinds: prop, items, variant.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the `MessagePack` Schema protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "msgpack-schema".into(),
        schema_theory: "ThMsgPackSchemaSchema".into(),
        instance_theory: "ThMsgPackSchemaInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "object".into(),
            "field".into(),
            "array".into(),
            "string".into(),
            "integer".into(),
            "boolean".into(),
            "float".into(),
            "binary".into(),
            "timestamp".into(),
            "extension".into(),
            "nil".into(),
            "union".into(),
        ],
        constraint_sorts: vec!["required".into(), "default".into()],
        has_order: true,
        has_recursion: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for `MessagePack` Schema with a theory registry.
///
/// Uses Group A (constrained multigraph + W-type).
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThMsgPackSchemaSchema",
        "ThMsgPackSchemaInstance",
    );
}

/// Parse a `MessagePack` Schema (JSON) into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if the JSON cannot be parsed as valid `MessagePack` Schema.
pub fn parse_msgpack_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto_def = protocol();
    let mut builder = SchemaBuilder::new(&proto_def);
    let mut counter: usize = 0;

    parse_schema_node(&mut builder, json, "", &mut counter)?;

    let schema = builder.build()?;
    Ok(schema)
}

/// Parse a single schema node recursively.
fn parse_schema_node(
    builder: &mut SchemaBuilder,
    value: &serde_json::Value,
    prefix: &str,
    counter: &mut usize,
) -> Result<String, ProtocolError> {
    match value {
        serde_json::Value::Object(obj) => {
            let type_val = obj.get("type").and_then(|v| v.as_str()).unwrap_or("object");

            let kind = msgpack_type_to_kind(type_val);

            let node_id = if prefix.is_empty() {
                "root".to_string()
            } else {
                format!("{prefix}:{counter}")
            };
            *counter += 1;

            let taken = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
            let mut b = taken.vertex(&node_id, kind, None)?;

            match kind {
                "object" => {
                    if let Some(serde_json::Value::Object(props)) = obj.get("properties") {
                        let required_fields: Vec<String> = obj
                            .get("required")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();

                        for (prop_name, prop_schema) in props {
                            let field_id = format!("{node_id}.{prop_name}");
                            b = b.vertex(&field_id, "field", None)?;
                            b = b.edge(&node_id, &field_id, "prop", Some(prop_name))?;

                            if required_fields.contains(prop_name) {
                                b = b.constraint(&field_id, "required", "true");
                            }

                            if let Some(default) = prop_schema.get("default") {
                                b = b.constraint(&field_id, "default", &default.to_string());
                            }

                            // Parse nested type.
                            *builder = b;
                            let child_id =
                                parse_schema_node(builder, prop_schema, &field_id, counter)?;
                            b = std::mem::replace(builder, SchemaBuilder::new(&protocol()));

                            if !child_id.is_empty() {
                                b = b.edge(&field_id, &child_id, "items", None)?;
                            }
                        }
                    }
                }
                "array" => {
                    if let Some(items_schema) = obj.get("items") {
                        *builder = b;
                        let child_id = parse_schema_node(builder, items_schema, &node_id, counter)?;
                        b = std::mem::replace(builder, SchemaBuilder::new(&protocol()));

                        if !child_id.is_empty() {
                            b = b.edge(&node_id, &child_id, "items", None)?;
                        }
                    }
                }
                "union" => {
                    if let Some(serde_json::Value::Array(variants)) = obj.get("oneOf") {
                        for variant_schema in variants {
                            *builder = b;
                            let child_id =
                                parse_schema_node(builder, variant_schema, &node_id, counter)?;
                            b = std::mem::replace(builder, SchemaBuilder::new(&protocol()));

                            if !child_id.is_empty() {
                                b = b.edge(&node_id, &child_id, "variant", None)?;
                            }
                        }
                    }
                }
                _ => {}
            }

            *builder = b;
            Ok(node_id)
        }
        serde_json::Value::String(s) => {
            let kind = msgpack_type_to_kind(s);
            let node_id = if prefix.is_empty() {
                kind.to_string()
            } else {
                format!("{prefix}:{counter}")
            };
            *counter += 1;

            let taken = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
            let b = taken.vertex(&node_id, kind, None)?;
            *builder = b;
            Ok(node_id)
        }
        _ => Ok(String::new()),
    }
}

/// Map `MessagePack` type names to vertex kinds.
fn msgpack_type_to_kind(type_name: &str) -> &'static str {
    match type_name {
        "array" => "array",
        "string" | "str" => "string",
        "integer" | "int" => "integer",
        "boolean" | "bool" => "boolean",
        "float" | "number" => "float",
        "binary" | "bin" => "binary",
        "timestamp" => "timestamp",
        "extension" | "ext" => "extension",
        "nil" | "null" => "nil",
        "union" => "union",
        // "object", "map", and any other type default to "object".
        _ => "object",
    }
}

/// Emit a `MessagePack` Schema (JSON) from a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if the schema cannot be serialized.
pub fn emit_msgpack_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let roots = find_roots(schema, &["prop", "items", "variant"]);

    if roots.len() == 1 {
        emit_node(schema, &roots[0].id)
    } else if roots.is_empty() {
        Err(ProtocolError::Emit("no root vertices found".into()))
    } else {
        // Multiple roots: emit first root.
        emit_node(schema, &roots[0].id)
    }
}

/// Emit a single schema node as JSON.
fn emit_node(schema: &Schema, vertex_id: &str) -> Result<serde_json::Value, ProtocolError> {
    let vertex = schema
        .vertices
        .get(vertex_id)
        .ok_or_else(|| ProtocolError::Emit(format!("vertex not found: {vertex_id}")))?;

    match vertex.kind.as_str() {
        "object" => {
            let mut obj = serde_json::Map::new();
            obj.insert("type".into(), serde_json::Value::String("object".into()));

            let props = children_by_edge(schema, vertex_id, "prop");
            if !props.is_empty() {
                let mut properties = serde_json::Map::new();
                let mut required_list = Vec::new();

                for (edge, field_vertex) in &props {
                    let name = edge.name.as_deref().unwrap_or(&field_vertex.id);

                    // Get the items edge to find the type.
                    let items = children_by_edge(schema, &field_vertex.id, "items");
                    let field_schema = if let Some((_, type_vertex)) = items.first() {
                        emit_node(schema, &type_vertex.id)?
                    } else {
                        serde_json::Value::Object(serde_json::Map::new())
                    };

                    properties.insert(name.to_string(), field_schema);

                    if constraint_value(schema, &field_vertex.id, "required").is_some() {
                        required_list.push(serde_json::Value::String(name.to_string()));
                    }
                }

                obj.insert("properties".into(), serde_json::Value::Object(properties));

                if !required_list.is_empty() {
                    obj.insert("required".into(), serde_json::Value::Array(required_list));
                }
            }

            Ok(serde_json::Value::Object(obj))
        }
        "array" => {
            let mut obj = serde_json::Map::new();
            obj.insert("type".into(), serde_json::Value::String("array".into()));

            let items = children_by_edge(schema, vertex_id, "items");
            if let Some((_, type_vertex)) = items.first() {
                obj.insert("items".into(), emit_node(schema, &type_vertex.id)?);
            }

            Ok(serde_json::Value::Object(obj))
        }
        kind => {
            let mut obj = serde_json::Map::new();
            obj.insert("type".into(), serde_json::Value::String(kind.into()));
            Ok(serde_json::Value::Object(obj))
        }
    }
}

/// Well-formedness rules for `MessagePack` Schema edges.
fn edge_rules() -> Vec<EdgeRule> {
    let all_types: Vec<String> = vec![
        "object",
        "field",
        "array",
        "string",
        "integer",
        "boolean",
        "float",
        "binary",
        "timestamp",
        "extension",
        "nil",
        "union",
    ]
    .into_iter()
    .map(Into::into)
    .collect();

    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["object".into()],
            tgt_kinds: vec!["field".into()],
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec!["array".into(), "field".into()],
            tgt_kinds: all_types.clone(),
        },
        EdgeRule {
            edge_kind: "variant".into(),
            src_kinds: vec!["union".into()],
            tgt_kinds: all_types,
        },
    ]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_creates_valid_definition() {
        let p = protocol();
        assert_eq!(p.name, "msgpack-schema");
        assert_eq!(p.schema_theory, "ThMsgPackSchemaSchema");
        assert_eq!(p.instance_theory, "ThMsgPackSchemaInstance");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThMsgPackSchemaSchema"));
        assert!(registry.contains_key("ThMsgPackSchemaInstance"));
    }

    #[test]
    fn parse_minimal() {
        let json: serde_json::Value = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            },
            "required": ["name"]
        });

        let schema = parse_msgpack_schema(&json).expect("should parse");
        assert!(schema.has_vertex("root"));
        assert!(schema.has_vertex("root.name"));
        assert!(schema.has_vertex("root.age"));
    }

    #[test]
    fn emit_minimal() {
        let json: serde_json::Value = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            },
            "required": ["name"]
        });

        let schema = parse_msgpack_schema(&json).expect("should parse");
        let emitted = emit_msgpack_schema(&schema).expect("should emit");
        assert!(emitted.is_object());
        let obj = emitted.as_object().unwrap();
        assert_eq!(obj.get("type").unwrap(), "object");
        assert!(obj.contains_key("properties"));
    }

    #[test]
    fn roundtrip() {
        let json: serde_json::Value = serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "count": {"type": "integer"}
            },
            "required": ["id"]
        });

        let schema1 = parse_msgpack_schema(&json).expect("parse 1");
        let emitted = emit_msgpack_schema(&schema1).expect("emit");
        let schema2 = parse_msgpack_schema(&emitted).expect("parse 2");

        assert_eq!(schema1.vertex_count(), schema2.vertex_count());
        assert!(schema2.has_vertex("root"));
    }
}
