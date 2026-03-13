//! INI Schema protocol definition.
//!
//! INI Schema uses a constrained multigraph schema theory
//! (`colimit(ThGraph, ThConstraint, ThMulti)`) and a W-type
//! instance theory (`ThWType`).
//!
//! Provides a JSON-based schema definition for INI files
//! (sections with typed keys).
//!
//! Vertex kinds: section, key, string, integer, float, boolean.
//! Edge kinds: prop.

use std::collections::HashMap;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the INI Schema protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "ini-schema".into(),
        schema_theory: "ThIniSchemaSchema".into(),
        instance_theory: "ThIniSchemaInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "section".into(),
            "key".into(),
            "string".into(),
            "integer".into(),
            "float".into(),
            "boolean".into(),
        ],
        constraint_sorts: vec!["required".into(), "default".into(), "enum".into()],
    }
}

/// Register the component GATs for INI Schema with a theory registry.
pub fn register_theories<S: ::std::hash::BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThIniSchemaSchema",
        "ThIniSchemaInstance",
    );
}

/// Parse an INI schema JSON document into a [`Schema`].
///
/// Expects a JSON object with `sections` mapping section names to
/// their typed key definitions.
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing or schema construction fails.
pub fn parse_ini_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let sections = json
        .get("sections")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ProtocolError::MissingField("sections".into()))?;

    for (section_name, section_def) in sections {
        let section_id = format!("section:{section_name}");
        builder = builder.vertex(&section_id, "section", None)?;

        let keys = section_def
            .get("keys")
            .and_then(serde_json::Value::as_object);

        let required_fields: Vec<&str> = section_def
            .get("required")
            .and_then(serde_json::Value::as_array)
            .map(|arr| arr.iter().filter_map(serde_json::Value::as_str).collect())
            .unwrap_or_default();

        if let Some(keys) = keys {
            for (key_name, key_def) in keys {
                let key_id = format!("{section_id}.{key_name}");
                let key_type = key_def
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("string");

                let kind = ini_type_to_kind(key_type);
                builder = builder.vertex(&key_id, &kind, None)?;
                builder = builder.edge(&section_id, &key_id, "prop", Some(key_name))?;

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
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Map an INI type string to a vertex kind.
fn ini_type_to_kind(type_str: &str) -> String {
    match type_str {
        "integer" | "int" => "integer",
        "float" | "number" => "float",
        "boolean" | "bool" => "boolean",
        _ => "string",
    }
    .to_string()
}

/// Emit a [`Schema`] as an INI Schema JSON document.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_ini_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let mut sections = serde_json::Map::new();

    let roots = find_roots(schema, &["prop"]);

    for root in &roots {
        if root.kind != "section" {
            continue;
        }
        let section_name = root.id.strip_prefix("section:").unwrap_or(&root.id);
        let mut section_obj = serde_json::Map::new();
        let mut keys = serde_json::Map::new();
        let mut required_list = Vec::new();

        let children = children_by_edge(schema, &root.id, "prop");
        for (edge, child) in &children {
            let name = edge.name.as_deref().unwrap_or("");
            let mut key_obj = serde_json::Map::new();

            let type_name = match child.kind.as_str() {
                "integer" => "integer",
                "float" => "float",
                "boolean" => "boolean",
                _ => "string",
            };
            key_obj.insert("type".into(), serde_json::Value::String(type_name.into()));

            if let Some(val) = constraint_value(schema, &child.id, "default") {
                key_obj.insert("default".into(), serde_json::Value::String(val.to_string()));
            }

            if let Some(val) = constraint_value(schema, &child.id, "enum") {
                let enum_vals: Vec<serde_json::Value> = val
                    .split(',')
                    .map(|s| serde_json::Value::String(s.to_string()))
                    .collect();
                key_obj.insert("enum".into(), serde_json::Value::Array(enum_vals));
            }

            if constraint_value(schema, &child.id, "required") == Some("true") {
                required_list.push(serde_json::Value::String(name.to_string()));
            }

            keys.insert(name.to_string(), serde_json::Value::Object(key_obj));
        }

        section_obj.insert("keys".into(), serde_json::Value::Object(keys));
        if !required_list.is_empty() {
            section_obj.insert("required".into(), serde_json::Value::Array(required_list));
        }

        sections.insert(
            section_name.to_string(),
            serde_json::Value::Object(section_obj),
        );
    }

    let mut result = serde_json::Map::new();
    result.insert("sections".into(), serde_json::Value::Object(sections));

    Ok(serde_json::Value::Object(result))
}

/// Well-formedness rules for INI Schema edges.
fn edge_rules() -> Vec<EdgeRule> {
    vec![EdgeRule {
        edge_kind: "prop".into(),
        src_kinds: vec!["section".into()],
        tgt_kinds: vec![],
    }]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_def() {
        let p = protocol();
        assert_eq!(p.name, "ini-schema");
        assert_eq!(p.schema_theory, "ThIniSchemaSchema");
        assert_eq!(p.instance_theory, "ThIniSchemaInstance");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThIniSchemaSchema"));
        assert!(registry.contains_key("ThIniSchemaInstance"));
    }

    #[test]
    fn parse_minimal() {
        let doc = serde_json::json!({
            "sections": {
                "database": {
                    "keys": {
                        "host": {"type": "string"},
                        "port": {"type": "integer", "default": "5432"},
                        "ssl": {"type": "boolean"}
                    },
                    "required": ["host"]
                }
            }
        });
        let schema = parse_ini_schema(&doc).expect("should parse");
        assert!(schema.has_vertex("section:database"));
        assert!(schema.has_vertex("section:database.host"));
        assert!(schema.has_vertex("section:database.port"));
        assert!(schema.has_vertex("section:database.ssl"));
    }

    #[test]
    fn emit_minimal() {
        let doc = serde_json::json!({
            "sections": {
                "general": {
                    "keys": {
                        "debug": {"type": "boolean", "default": "false"}
                    }
                }
            }
        });
        let schema = parse_ini_schema(&doc).expect("should parse");
        let emitted = emit_ini_schema(&schema).expect("should emit");
        assert!(emitted.get("sections").is_some());
    }

    #[test]
    fn roundtrip() {
        let doc = serde_json::json!({
            "sections": {
                "logging": {
                    "keys": {
                        "level": {"type": "string", "enum": ["debug","info","warn","error"]},
                        "file": {"type": "string"}
                    }
                }
            }
        });
        let schema = parse_ini_schema(&doc).expect("parse");
        let emitted = emit_ini_schema(&schema).expect("emit");
        let schema2 = parse_ini_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertices.len(), schema2.vertices.len());
    }
}
