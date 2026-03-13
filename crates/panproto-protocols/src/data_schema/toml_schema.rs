//! TOML Schema protocol definition.
//!
//! TOML Schema uses a constrained multigraph schema theory
//! (`colimit(ThGraph, ThConstraint, ThMulti)`) and a W-type
//! instance theory (`ThWType`).
//!
//! Provides a JSON-based schema definition for TOML structures
//! (tables, arrays-of-tables, typed keys).
//!
//! Vertex kinds: table, array-of-tables, key, string, integer, float,
//! boolean, datetime, date, time, array, inline-table.
//!
//! Edge kinds: prop, items.

use std::collections::HashMap;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the TOML Schema protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "toml-schema".into(),
        schema_theory: "ThTomlSchemaSchema".into(),
        instance_theory: "ThTomlSchemaInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "table".into(),
            "array-of-tables".into(),
            "key".into(),
            "string".into(),
            "integer".into(),
            "float".into(),
            "boolean".into(),
            "datetime".into(),
            "date".into(),
            "time".into(),
            "array".into(),
            "inline-table".into(),
        ],
        constraint_sorts: vec!["required".into(), "default".into(), "enum".into()],
    }
}

/// Register the component GATs for TOML Schema with a theory registry.
pub fn register_theories<S: ::std::hash::BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThTomlSchemaSchema",
        "ThTomlSchemaInstance",
    );
}

/// Parse a TOML schema JSON document into a [`Schema`].
///
/// Expects a JSON object describing TOML tables and their typed keys.
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing or schema construction fails.
pub fn parse_toml_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let tables = json
        .get("tables")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ProtocolError::MissingField("tables".into()))?;

    for (table_name, table_def) in tables {
        let table_id = format!("table:{table_name}");
        builder = builder.vertex(&table_id, "table", None)?;

        builder = walk_toml_table(builder, table_def, &table_id)?;
    }

    // Arrays of tables.
    if let Some(aot) = json
        .get("arrays_of_tables")
        .and_then(serde_json::Value::as_object)
    {
        for (aot_name, aot_def) in aot {
            let aot_id = format!("aot:{aot_name}");
            builder = builder.vertex(&aot_id, "array-of-tables", None)?;

            // Each array-of-tables entry has an items schema.
            if let Some(items_def) = aot_def.get("items") {
                let items_id = format!("{aot_id}:items");
                builder = builder.vertex(&items_id, "table", None)?;
                builder = builder.edge(&aot_id, &items_id, "items", None)?;
                builder = walk_toml_table(builder, items_def, &items_id)?;
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Walk a TOML table definition, creating key vertices and prop edges.
fn walk_toml_table(
    mut builder: SchemaBuilder,
    table_def: &serde_json::Value,
    parent_id: &str,
) -> Result<SchemaBuilder, ProtocolError> {
    let keys = table_def.get("keys").and_then(serde_json::Value::as_object);

    let required_fields: Vec<&str> = table_def
        .get("required")
        .and_then(serde_json::Value::as_array)
        .map(|arr| arr.iter().filter_map(serde_json::Value::as_str).collect())
        .unwrap_or_default();

    if let Some(keys) = keys {
        for (key_name, key_def) in keys {
            let key_id = format!("{parent_id}.{key_name}");
            let key_type = key_def
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("string");

            let kind = toml_type_to_kind(key_type);
            builder = builder.vertex(&key_id, &kind, None)?;
            builder = builder.edge(parent_id, &key_id, "prop", Some(key_name))?;

            if required_fields.contains(&key_name.as_str()) {
                builder = builder.constraint(&key_id, "required", "true");
            }

            if let Some(default_val) = key_def.get("default") {
                let val_str = match default_val {
                    serde_json::Value::String(s) => s.clone(),
                    _ => default_val.to_string(),
                };
                builder = builder.constraint(&key_id, "default", &val_str);
            }

            if let Some(enum_val) = key_def.get("enum").and_then(serde_json::Value::as_array) {
                let vals: Vec<String> = enum_val
                    .iter()
                    .map(|v| v.as_str().map_or_else(|| v.to_string(), String::from))
                    .collect();
                builder = builder.constraint(&key_id, "enum", &vals.join(","));
            }

            // Nested table (inline-table).
            if key_type == "inline-table" || key_type == "table" {
                builder = walk_toml_table(builder, key_def, &key_id)?;
            }

            // Array items.
            if key_type == "array" {
                if let Some(items) = key_def.get("items") {
                    let items_id = format!("{key_id}:items");
                    let items_type = items
                        .get("type")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("string");
                    let items_kind = toml_type_to_kind(items_type);
                    builder = builder.vertex(&items_id, &items_kind, None)?;
                    builder = builder.edge(&key_id, &items_id, "items", None)?;
                }
            }
        }
    }

    Ok(builder)
}

/// Map a TOML type string to a vertex kind.
fn toml_type_to_kind(type_str: &str) -> String {
    match type_str {
        "integer" => "integer",
        "float" => "float",
        "boolean" | "bool" => "boolean",
        "datetime" | "offset-datetime" | "local-datetime" => "datetime",
        "date" | "local-date" => "date",
        "time" | "local-time" => "time",
        "array" => "array",
        "inline-table" => "inline-table",
        "table" => "table",
        _ => "string",
    }
    .to_string()
}

/// Emit a [`Schema`] as a TOML Schema JSON document.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_toml_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let mut tables = serde_json::Map::new();
    let mut arrays_of_tables = serde_json::Map::new();

    let roots = find_roots(schema, &["prop", "items"]);

    for root in &roots {
        match root.kind.as_str() {
            "table" => {
                let table_name = root.id.strip_prefix("table:").unwrap_or(&root.id);
                tables.insert(table_name.to_string(), emit_toml_table(schema, &root.id));
            }
            "array-of-tables" => {
                let aot_name = root.id.strip_prefix("aot:").unwrap_or(&root.id);
                let mut aot_obj = serde_json::Map::new();
                let items = children_by_edge(schema, &root.id, "items");
                if let Some((edge, _)) = items.first() {
                    aot_obj.insert("items".into(), emit_toml_table(schema, &edge.tgt));
                }
                arrays_of_tables.insert(aot_name.to_string(), serde_json::Value::Object(aot_obj));
            }
            _ => {}
        }
    }

    let mut result = serde_json::Map::new();
    result.insert("tables".into(), serde_json::Value::Object(tables));
    if !arrays_of_tables.is_empty() {
        result.insert(
            "arrays_of_tables".into(),
            serde_json::Value::Object(arrays_of_tables),
        );
    }

    Ok(serde_json::Value::Object(result))
}

/// Emit a TOML table vertex as a JSON object.
fn emit_toml_table(schema: &Schema, vertex_id: &str) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    let mut keys = serde_json::Map::new();
    let mut required_list = Vec::new();

    let children = children_by_edge(schema, vertex_id, "prop");
    for (edge, child) in &children {
        let name = edge.name.as_deref().unwrap_or("");
        let mut key_obj = serde_json::Map::new();

        let type_name = match child.kind.as_str() {
            "integer" => "integer",
            "float" => "float",
            "boolean" => "boolean",
            "datetime" => "datetime",
            "date" => "date",
            "time" => "time",
            "array" => "array",
            "inline-table" => "inline-table",
            "table" => "table",
            _ => "string",
        };
        key_obj.insert("type".into(), serde_json::Value::String(type_name.into()));

        if let Some(val) = constraint_value(schema, &child.id, "default") {
            key_obj.insert("default".into(), serde_json::Value::String(val.to_string()));
        }

        if constraint_value(schema, &child.id, "required") == Some("true") {
            required_list.push(serde_json::Value::String(name.to_string()));
        }

        keys.insert(name.to_string(), serde_json::Value::Object(key_obj));
    }

    obj.insert("keys".into(), serde_json::Value::Object(keys));
    if !required_list.is_empty() {
        obj.insert("required".into(), serde_json::Value::Array(required_list));
    }

    serde_json::Value::Object(obj)
}

/// Well-formedness rules for TOML Schema edges.
fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["table".into(), "inline-table".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec!["array".into(), "array-of-tables".into()],
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
        assert_eq!(p.name, "toml-schema");
        assert_eq!(p.schema_theory, "ThTomlSchemaSchema");
        assert_eq!(p.instance_theory, "ThTomlSchemaInstance");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThTomlSchemaSchema"));
        assert!(registry.contains_key("ThTomlSchemaInstance"));
    }

    #[test]
    fn parse_minimal() {
        let doc = serde_json::json!({
            "tables": {
                "package": {
                    "keys": {
                        "name": {"type": "string"},
                        "version": {"type": "string"},
                        "edition": {"type": "string", "default": "2021"}
                    },
                    "required": ["name", "version"]
                }
            }
        });
        let schema = parse_toml_schema(&doc).expect("should parse");
        assert!(schema.has_vertex("table:package"));
        assert!(schema.has_vertex("table:package.name"));
        assert!(schema.has_vertex("table:package.version"));
    }

    #[test]
    fn emit_minimal() {
        let doc = serde_json::json!({
            "tables": {
                "server": {
                    "keys": {
                        "host": {"type": "string"},
                        "port": {"type": "integer"}
                    }
                }
            }
        });
        let schema = parse_toml_schema(&doc).expect("should parse");
        let emitted = emit_toml_schema(&schema).expect("should emit");
        assert!(emitted.get("tables").is_some());
    }

    #[test]
    fn roundtrip() {
        let doc = serde_json::json!({
            "tables": {
                "config": {
                    "keys": {
                        "debug": {"type": "boolean"},
                        "level": {"type": "integer"}
                    }
                }
            }
        });
        let schema = parse_toml_schema(&doc).expect("parse");
        let emitted = emit_toml_schema(&schema).expect("emit");
        let schema2 = parse_toml_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertices.len(), schema2.vertices.len());
    }
}
