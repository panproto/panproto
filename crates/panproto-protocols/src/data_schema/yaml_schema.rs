//! YAML Schema protocol definition.
//!
//! YAML Schema uses a constrained multigraph schema theory
//! (`colimit(ThGraph, ThConstraint, ThMulti)`) and a W-type
//! instance theory (`ThWType`).
//!
//! This is a JSON Schema subset tailored for YAML, with support
//! for anchors and tags.
//!
//! Vertex kinds: object, array, string, integer, number, boolean, null, anchor, tag.
//! Edge kinds: prop, items, variant.

use std::collections::HashMap;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the YAML Schema protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "yaml-schema".into(),
        schema_theory: "ThYamlSchemaSchema".into(),
        instance_theory: "ThYamlSchemaInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "object".into(),
            "array".into(),
            "string".into(),
            "integer".into(),
            "number".into(),
            "boolean".into(),
            "null".into(),
            "anchor".into(),
            "tag".into(),
        ],
        constraint_sorts: vec![
            "required".into(),
            "type".into(),
            "enum".into(),
            "default".into(),
            "pattern".into(),
            "minLength".into(),
            "maxLength".into(),
        ],
    }
}

/// Register the component GATs for YAML Schema with a theory registry.
pub fn register_theories<S: ::std::hash::BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThYamlSchemaSchema",
        "ThYamlSchemaInstance",
    );
}

/// Parse a YAML Schema JSON document into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing or schema construction fails.
pub fn parse_yaml_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut counter: usize = 0;

    builder = walk_yaml_schema(builder, json, "root", &mut counter)?;

    let schema = builder.build()?;
    Ok(schema)
}

/// Recursively walk a YAML schema object.
fn walk_yaml_schema(
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
        "null" => "null",
        "array" => "array",
        _ => "object",
    };

    builder = builder.vertex(current_id, kind, None)?;

    // Constraints.
    for field in &["pattern", "minLength", "maxLength"] {
        if let Some(val) = schema.get(field) {
            let val_str = match val {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                _ => val.to_string(),
            };
            builder = builder.constraint(current_id, field, &val_str);
        }
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
            builder = walk_yaml_schema(builder, prop_schema, &prop_id, counter)?;
            builder = builder.edge(current_id, &prop_id, "prop", Some(prop_name))?;
            if required_fields.contains(&prop_name.as_str()) {
                builder = builder.constraint(&prop_id, "required", "true");
            }
        }
    }

    // Items.
    if let Some(items) = schema.get("items") {
        let items_id = format!("{current_id}:items");
        builder = walk_yaml_schema(builder, items, &items_id, counter)?;
        builder = builder.edge(current_id, &items_id, "items", None)?;
    }

    // YAML-specific: anchors.
    if let Some(anchors) = schema.get("anchors").and_then(serde_json::Value::as_object) {
        for (anchor_name, anchor_schema) in anchors {
            *counter += 1;
            let anchor_id = format!("{current_id}:anchor:{anchor_name}");
            builder = builder.vertex(&anchor_id, "anchor", None)?;
            builder = builder.edge(current_id, &anchor_id, "prop", Some(anchor_name))?;

            if anchor_schema.is_object() {
                let inner_id = format!("{anchor_id}:value");
                builder = walk_yaml_schema(builder, anchor_schema, &inner_id, counter)?;
                builder = builder.edge(&anchor_id, &inner_id, "prop", Some("value"))?;
            }
        }
    }

    // YAML-specific: tags.
    if let Some(tags) = schema.get("tags").and_then(serde_json::Value::as_array) {
        for (i, tag_val) in tags.iter().enumerate() {
            if let Some(tag_str) = tag_val.as_str() {
                *counter += 1;
                let tag_id = format!("{current_id}:tag{i}_{counter}");
                builder = builder.vertex(&tag_id, "tag", None)?;
                builder = builder.edge(current_id, &tag_id, "variant", Some(tag_str))?;
            }
        }
    }

    // oneOf / anyOf.
    for combiner in &["oneOf", "anyOf"] {
        if let Some(arr) = schema.get(*combiner).and_then(serde_json::Value::as_array) {
            for (i, sub_schema) in arr.iter().enumerate() {
                *counter += 1;
                let sub_id = format!("{current_id}:{combiner}{i}_{counter}");
                builder = walk_yaml_schema(builder, sub_schema, &sub_id, counter)?;
                builder = builder.edge(current_id, &sub_id, "variant", Some(combiner))?;
            }
        }
    }

    Ok(builder)
}

/// Emit a [`Schema`] as a YAML Schema JSON document.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_yaml_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let roots = find_roots(schema, &["prop", "items", "variant"]);

    roots.first().map_or_else(
        || Err(ProtocolError::Emit("no root vertex found".into())),
        |root| Ok(emit_yaml_vertex(schema, &root.id)),
    )
}

/// Emit a single vertex as a YAML schema JSON object.
fn emit_yaml_vertex(schema: &Schema, vertex_id: &str) -> serde_json::Value {
    let Some(vertex) = schema.vertices.get(vertex_id) else {
        return serde_json::Value::Object(serde_json::Map::new());
    };

    let mut obj = serde_json::Map::new();

    let type_str = match vertex.kind.as_str() {
        "string" => Some("string"),
        "integer" => Some("integer"),
        "number" => Some("number"),
        "boolean" => Some("boolean"),
        "null" => Some("null"),
        "array" => Some("array"),
        "object" => Some("object"),
        _ => None,
    };
    if let Some(t) = type_str {
        obj.insert("type".into(), serde_json::Value::String(t.into()));
    }

    for field in &["pattern", "minLength", "maxLength", "default"] {
        if let Some(val) = constraint_value(schema, vertex_id, field) {
            if let Ok(n) = val.parse::<f64>() {
                obj.insert((*field).into(), serde_json::json!(n));
            } else {
                obj.insert((*field).into(), serde_json::Value::String(val.to_string()));
            }
        }
    }

    // Properties.
    let props = children_by_edge(schema, vertex_id, "prop");
    if !props.is_empty() {
        let mut properties = serde_json::Map::new();
        let mut required_list = Vec::new();
        for (edge, _child) in &props {
            let name = edge.name.as_deref().unwrap_or("");
            properties.insert(name.to_string(), emit_yaml_vertex(schema, &edge.tgt));
            if constraint_value(schema, &edge.tgt, "required") == Some("true") {
                required_list.push(serde_json::Value::String(name.to_string()));
            }
        }
        obj.insert("properties".into(), serde_json::Value::Object(properties));
        if !required_list.is_empty() {
            obj.insert("required".into(), serde_json::Value::Array(required_list));
        }
    }

    // Items.
    let items = children_by_edge(schema, vertex_id, "items");
    if let Some((edge, _)) = items.first() {
        obj.insert("items".into(), emit_yaml_vertex(schema, &edge.tgt));
    }

    serde_json::Value::Object(obj)
}

/// Well-formedness rules for YAML Schema edges.
fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["object".into(), "anchor".into()],
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
    ]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_def() {
        let p = protocol();
        assert_eq!(p.name, "yaml-schema");
        assert_eq!(p.schema_theory, "ThYamlSchemaSchema");
        assert_eq!(p.instance_theory, "ThYamlSchemaInstance");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThYamlSchemaSchema"));
        assert!(registry.contains_key("ThYamlSchemaInstance"));
    }

    #[test]
    fn parse_minimal() {
        let doc = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "maxLength": 50},
                "tags": {
                    "type": "array",
                    "items": {"type": "string"}
                }
            },
            "required": ["name"]
        });
        let schema = parse_yaml_schema(&doc).expect("should parse");
        assert!(schema.has_vertex("root"));
        assert!(schema.has_vertex("root.name"));
        assert!(schema.has_vertex("root.tags"));
    }

    #[test]
    fn emit_minimal() {
        let doc = serde_json::json!({
            "type": "object",
            "properties": {
                "key": {"type": "string"}
            }
        });
        let schema = parse_yaml_schema(&doc).expect("should parse");
        let emitted = emit_yaml_schema(&schema).expect("should emit");
        assert_eq!(emitted.get("type").and_then(|v| v.as_str()), Some("object"));
    }

    #[test]
    fn roundtrip() {
        let doc = serde_json::json!({
            "type": "object",
            "properties": {
                "host": {"type": "string"},
                "port": {"type": "integer"}
            }
        });
        let schema = parse_yaml_schema(&doc).expect("parse");
        let emitted = emit_yaml_schema(&schema).expect("emit");
        let schema2 = parse_yaml_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertices.len(), schema2.vertices.len());
    }
}
