//! `MongoDB` Schema Validation protocol definition.
//!
//! `MongoDB` uses a constrained multigraph schema theory
//! (`colimit(ThGraph, ThConstraint, ThMulti)`) and a W-type
//! instance theory (`ThWType`).
//!
//! Vertex kinds: collection, field, object, array, string, int, long,
//! double, decimal, bool, date, timestamp, objectId, binary, regex, null.
//!
//! Edge kinds: prop, items, variant.

use std::collections::HashMap;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the `MongoDB` protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "mongodb".into(),
        schema_theory: "ThMongoDBSchema".into(),
        instance_theory: "ThMongoDBInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "collection".into(),
            "field".into(),
            "object".into(),
            "array".into(),
            "string".into(),
            "int".into(),
            "long".into(),
            "double".into(),
            "decimal".into(),
            "bool".into(),
            "date".into(),
            "timestamp".into(),
            "objectId".into(),
            "binary".into(),
            "regex".into(),
            "null".into(),
        ],
        constraint_sorts: vec![
            "required".into(),
            "bsonType".into(),
            "enum".into(),
            "minimum".into(),
            "maximum".into(),
            "minLength".into(),
            "maxLength".into(),
            "pattern".into(),
            "description".into(),
        ],
    }
}

/// Register the component GATs for `MongoDB` with a theory registry.
pub fn register_theories<S: ::std::hash::BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThMongoDBSchema",
        "ThMongoDBInstance",
    );
}

/// Parse a `MongoDB` JSON Schema validation document into a [`Schema`].
///
/// Expects a JSON object with a `$jsonSchema` key at the top level,
/// or the schema body directly (bsonType-based).
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing or schema construction fails.
pub fn parse_mongodb_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    // Support both `{$jsonSchema: ...}` wrapper and direct schema body.
    let schema_body = json
        .get("$jsonSchema")
        .or_else(|| json.get("validator").and_then(|v| v.get("$jsonSchema")))
        .unwrap_or(json);

    let collection_name = json
        .get("collection")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("root");

    let collection_id = format!("collection:{collection_name}");
    builder = builder.vertex(&collection_id, "collection", None)?;

    if let Some(desc) = schema_body
        .get("description")
        .and_then(serde_json::Value::as_str)
    {
        builder = builder.constraint(&collection_id, "description", desc);
    }

    // Walk the schema body.
    builder = walk_bson_schema(builder, schema_body, &collection_id)?;

    let schema = builder.build()?;
    Ok(schema)
}

/// Recursively walk a `MongoDB` JSON Schema validation object.
fn walk_bson_schema(
    mut builder: SchemaBuilder,
    schema: &serde_json::Value,
    parent_id: &str,
) -> Result<SchemaBuilder, ProtocolError> {
    let required_fields: Vec<&str> = schema
        .get("required")
        .and_then(serde_json::Value::as_array)
        .map(|arr| arr.iter().filter_map(serde_json::Value::as_str).collect())
        .unwrap_or_default();

    if let Some(properties) = schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
    {
        for (prop_name, prop_schema) in properties {
            let prop_id = format!("{parent_id}.{prop_name}");

            let bson_type = prop_schema
                .get("bsonType")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("object");

            let kind = bson_type_to_kind(bson_type);
            builder = builder.vertex(&prop_id, &kind, None)?;
            builder = builder.edge(parent_id, &prop_id, "prop", Some(prop_name))?;

            if required_fields.contains(&prop_name.as_str()) {
                builder = builder.constraint(&prop_id, "required", "true");
            }

            // Add constraints.
            for field in &["minimum", "maximum", "minLength", "maxLength", "pattern"] {
                if let Some(val) = prop_schema.get(field) {
                    let val_str = match val {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Number(n) => n.to_string(),
                        _ => val.to_string(),
                    };
                    builder = builder.constraint(&prop_id, field, &val_str);
                }
            }

            if let Some(desc) = prop_schema
                .get("description")
                .and_then(serde_json::Value::as_str)
            {
                builder = builder.constraint(&prop_id, "description", desc);
            }

            if let Some(enum_val) = prop_schema
                .get("enum")
                .and_then(serde_json::Value::as_array)
            {
                let vals: Vec<String> = enum_val
                    .iter()
                    .map(|v| v.as_str().map_or_else(|| v.to_string(), String::from))
                    .collect();
                builder = builder.constraint(&prop_id, "enum", &vals.join(","));
            }

            // Recurse into nested objects.
            if bson_type == "object" {
                builder = walk_bson_schema(builder, prop_schema, &prop_id)?;
            }

            // Handle array items.
            if bson_type == "array" {
                if let Some(items) = prop_schema.get("items") {
                    let items_id = format!("{prop_id}:items");
                    let items_type = items
                        .get("bsonType")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("object");
                    let items_kind = bson_type_to_kind(items_type);
                    builder = builder.vertex(&items_id, &items_kind, None)?;
                    builder = builder.edge(&prop_id, &items_id, "items", None)?;

                    if items_type == "object" {
                        builder = walk_bson_schema(builder, items, &items_id)?;
                    }
                }
            }

            // Handle bsonType arrays (union types).
            if let Some(serde_json::Value::Array(types)) = prop_schema.get("bsonType") {
                // Already created with the first type; add variant edges for the rest.
                for (i, t) in types.iter().enumerate() {
                    if let Some(t_str) = t.as_str() {
                        if i > 0 {
                            let variant_id = format!("{prop_id}:variant{i}");
                            let variant_kind = bson_type_to_kind(t_str);
                            builder = builder.vertex(&variant_id, &variant_kind, None)?;
                            builder =
                                builder.edge(&prop_id, &variant_id, "variant", Some(t_str))?;
                        }
                    }
                }
            }
        }
    }

    Ok(builder)
}

/// Map a BSON type string to a vertex kind.
fn bson_type_to_kind(bson_type: &str) -> String {
    match bson_type {
        "string" => "string",
        "int" => "int",
        "long" => "long",
        "double" => "double",
        "decimal" => "decimal",
        "bool" => "bool",
        "date" => "date",
        "timestamp" => "timestamp",
        "objectId" => "objectId",
        "binary" | "binData" => "binary",
        "regex" => "regex",
        "null" => "null",
        "array" => "array",
        _ => "object",
    }
    .to_string()
}

/// Emit a [`Schema`] as a `MongoDB` JSON Schema validation document.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_mongodb_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let roots = find_roots(schema, &["prop", "items", "variant"]);

    // Find the collection root.
    let collection_root = roots
        .iter()
        .find(|v| v.kind == "collection")
        .ok_or_else(|| ProtocolError::Emit("no collection vertex found".into()))?;

    let collection_name = collection_root
        .id
        .strip_prefix("collection:")
        .unwrap_or(&collection_root.id);

    let json_schema = emit_bson_object(schema, &collection_root.id);

    let mut result = serde_json::Map::new();
    result.insert(
        "collection".into(),
        serde_json::Value::String(collection_name.to_string()),
    );
    result.insert("$jsonSchema".into(), json_schema);

    Ok(serde_json::Value::Object(result))
}

/// Emit a BSON schema object from a vertex and its children.
fn emit_bson_object(schema: &Schema, vertex_id: &str) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "bsonType".into(),
        serde_json::Value::String("object".into()),
    );

    let children = children_by_edge(schema, vertex_id, "prop");
    if children.is_empty() {
        return serde_json::Value::Object(obj);
    }

    let mut properties = serde_json::Map::new();
    let mut required_list = Vec::new();

    for (edge, child) in &children {
        let name = edge.name.as_deref().unwrap_or("");
        let mut prop_obj = serde_json::Map::new();

        let bson_type = match child.kind.as_str() {
            "string" => "string",
            "int" => "int",
            "long" => "long",
            "double" => "double",
            "decimal" => "decimal",
            "bool" => "bool",
            "date" => "date",
            "timestamp" => "timestamp",
            "objectId" => "objectId",
            "binary" => "binary",
            "regex" => "regex",
            "null" => "null",
            "array" => "array",
            _ => "object",
        };
        prop_obj.insert(
            "bsonType".into(),
            serde_json::Value::String(bson_type.into()),
        );

        if constraint_value(schema, &child.id, "required") == Some("true") {
            required_list.push(serde_json::Value::String(name.to_string()));
        }

        for field in &["minimum", "maximum", "minLength", "maxLength", "pattern"] {
            if let Some(val) = constraint_value(schema, &child.id, field) {
                if let Ok(n) = val.parse::<f64>() {
                    prop_obj.insert((*field).into(), serde_json::json!(n));
                } else {
                    prop_obj.insert((*field).into(), serde_json::Value::String(val.to_string()));
                }
            }
        }

        if let Some(desc) = constraint_value(schema, &child.id, "description") {
            prop_obj.insert(
                "description".into(),
                serde_json::Value::String(desc.to_string()),
            );
        }

        // Nested object.
        if bson_type == "object" {
            let nested = emit_bson_object(schema, &child.id);
            if let Some(nested_obj) = nested.as_object() {
                if let Some(nested_props) = nested_obj.get("properties") {
                    prop_obj.insert("properties".into(), nested_props.clone());
                }
            }
        }

        properties.insert(name.to_string(), serde_json::Value::Object(prop_obj));
    }

    obj.insert("properties".into(), serde_json::Value::Object(properties));
    if !required_list.is_empty() {
        obj.insert("required".into(), serde_json::Value::Array(required_list));
    }

    serde_json::Value::Object(obj)
}

/// Well-formedness rules for `MongoDB` edges.
fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["collection".into(), "object".into()],
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
        assert_eq!(p.name, "mongodb");
        assert_eq!(p.schema_theory, "ThMongoDBSchema");
        assert_eq!(p.instance_theory, "ThMongoDBInstance");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThMongoDBSchema"));
        assert!(registry.contains_key("ThMongoDBInstance"));
    }

    #[test]
    fn parse_minimal() {
        let doc = serde_json::json!({
            "collection": "users",
            "$jsonSchema": {
                "bsonType": "object",
                "required": ["name", "email"],
                "properties": {
                    "name": {
                        "bsonType": "string",
                        "description": "User name",
                        "maxLength": 100
                    },
                    "email": {
                        "bsonType": "string"
                    },
                    "age": {
                        "bsonType": "int",
                        "minimum": 0,
                        "maximum": 150
                    }
                }
            }
        });
        let schema = parse_mongodb_schema(&doc).expect("should parse");
        assert!(schema.has_vertex("collection:users"));
        assert!(schema.has_vertex("collection:users.name"));
        assert!(schema.has_vertex("collection:users.email"));
        assert!(schema.has_vertex("collection:users.age"));
    }

    #[test]
    fn emit_minimal() {
        let doc = serde_json::json!({
            "collection": "items",
            "$jsonSchema": {
                "bsonType": "object",
                "properties": {
                    "title": {"bsonType": "string"}
                }
            }
        });
        let schema = parse_mongodb_schema(&doc).expect("should parse");
        let emitted = emit_mongodb_schema(&schema).expect("should emit");
        assert!(emitted.get("$jsonSchema").is_some());
        assert_eq!(
            emitted.get("collection").and_then(|v| v.as_str()),
            Some("items")
        );
    }

    #[test]
    fn roundtrip() {
        let doc = serde_json::json!({
            "collection": "products",
            "$jsonSchema": {
                "bsonType": "object",
                "properties": {
                    "name": {"bsonType": "string"},
                    "price": {"bsonType": "double"}
                }
            }
        });
        let schema = parse_mongodb_schema(&doc).expect("parse");
        let emitted = emit_mongodb_schema(&schema).expect("emit");
        let schema2 = parse_mongodb_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertices.len(), schema2.vertices.len());
    }
}
