//! BSON Schema protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the BSON protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "bson".into(),
        schema_theory: "ThBsonSchema".into(),
        instance_theory: "ThBsonInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "object".into(),
            "array".into(),
            "string".into(),
            "int".into(),
            "long".into(),
            "double".into(),
            "decimal128".into(),
            "boolean".into(),
            "date".into(),
            "timestamp".into(),
            "objectid".into(),
            "binary".into(),
            "regex".into(),
            "null".into(),
        ],
        constraint_sorts: vec![
            "required".into(),
            "minLength".into(),
            "maxLength".into(),
            "minimum".into(),
            "maximum".into(),
        ],
    }
}

/// Register the component GATs for BSON.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(registry, "ThBsonSchema", "ThBsonInstance");
}

/// Parse a JSON-based BSON schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_bson_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut counter: usize = 0;

    builder = walk_bson_schema(builder, json, "root", &mut counter)?;

    let schema = builder.build()?;
    Ok(schema)
}

/// Walk a BSON schema value recursively.
fn walk_bson_schema(
    mut builder: SchemaBuilder,
    schema: &serde_json::Value,
    current_id: &str,
    counter: &mut usize,
) -> Result<SchemaBuilder, ProtocolError> {
    let bson_type = schema
        .get("bsonType")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("object");

    let kind = bson_type_to_kind(bson_type);
    builder = builder.vertex(current_id, kind, None)?;

    // Constraints.
    for field in &["minLength", "maxLength", "minimum", "maximum"] {
        if let Some(val) = schema.get(field) {
            let val_str = match val {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                _ => val.to_string(),
            };
            builder = builder.constraint(current_id, field, &val_str);
        }
    }

    // Properties.
    if let Some(properties) = schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
    {
        for (prop_name, prop_schema) in properties {
            let prop_id = format!("{current_id}.{prop_name}");
            builder = walk_bson_schema(builder, prop_schema, &prop_id, counter)?;
            builder = builder.edge(current_id, &prop_id, "prop", Some(prop_name))?;
        }
    }

    // Items.
    if let Some(items) = schema.get("items") {
        let items_id = format!("{current_id}:items");
        builder = walk_bson_schema(builder, items, &items_id, counter)?;
        builder = builder.edge(current_id, &items_id, "items", None)?;
    }

    Ok(builder)
}

/// Map BSON type to vertex kind.
fn bson_type_to_kind(bson_type: &str) -> &'static str {
    match bson_type {
        "object" => "object",
        "array" => "array",
        "string" => "string",
        "int" => "int",
        "long" => "long",
        "double" => "double",
        "decimal" | "decimal128" => "decimal128",
        "bool" | "boolean" => "boolean",
        "date" => "date",
        "timestamp" => "timestamp",
        "objectId" | "objectid" => "objectid",
        "binData" | "binary" => "binary",
        "regex" => "regex",
        "null" => "null",
        _ => "object",
    }
}

/// Map vertex kind to BSON type string.
fn kind_to_bson_type(kind: &str) -> &'static str {
    match kind {
        "object" => "object",
        "array" => "array",
        "string" => "string",
        "int" => "int",
        "long" => "long",
        "double" => "double",
        "decimal128" => "decimal128",
        "boolean" => "bool",
        "date" => "date",
        "timestamp" => "timestamp",
        "objectid" => "objectId",
        "binary" => "binData",
        "regex" => "regex",
        "null" => "null",
        _ => "object",
    }
}

/// Emit a [`Schema`] as a JSON BSON schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_bson_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let root = schema
        .vertices
        .get("root")
        .ok_or_else(|| ProtocolError::Emit("no root vertex found".into()))?;

    emit_bson_vertex(schema, root)
}

/// Emit a single BSON vertex.
fn emit_bson_vertex(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
) -> Result<serde_json::Value, ProtocolError> {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "bsonType".into(),
        serde_json::json!(kind_to_bson_type(&vertex.kind)),
    );

    for c in vertex_constraints(schema, &vertex.id) {
        if let Ok(n) = c.value.parse::<i64>() {
            obj.insert(c.sort.clone(), serde_json::json!(n));
        } else {
            obj.insert(c.sort.clone(), serde_json::json!(c.value));
        }
    }

    let props = children_by_edge(schema, &vertex.id, "prop");
    if !props.is_empty() {
        let mut properties = serde_json::Map::new();
        for (edge, child) in &props {
            let name = edge.name.as_deref().unwrap_or(&child.id);
            let val = emit_bson_vertex(schema, child)?;
            properties.insert(name.to_string(), val);
        }
        obj.insert("properties".into(), serde_json::Value::Object(properties));
    }

    let items = children_by_edge(schema, &vertex.id, "items");
    if let Some((_, child)) = items.first() {
        let val = emit_bson_vertex(schema, child)?;
        obj.insert("items".into(), val);
    }

    Ok(serde_json::Value::Object(obj))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["object".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec!["array".into()],
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
        assert_eq!(p.name, "bson");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThBsonSchema"));
        assert!(registry.contains_key("ThBsonInstance"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "bsonType": "object",
            "properties": {
                "name": {"bsonType": "string", "maxLength": 100},
                "age": {"bsonType": "int"}
            }
        });
        let schema = parse_bson_schema(&json).expect("should parse");
        assert!(schema.has_vertex("root"));
        assert!(schema.has_vertex("root.name"));

        let emitted = emit_bson_schema(&schema).expect("should emit");
        let s2 = parse_bson_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }
}
