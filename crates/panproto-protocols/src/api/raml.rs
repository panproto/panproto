//! RAML protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the RAML protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "raml".into(),
        schema_theory: "ThRamlSchema".into(),
        instance_theory: "ThRamlInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "resource".into(),
            "method".into(),
            "type".into(),
            "trait".into(),
            "string".into(),
            "integer".into(),
            "number".into(),
            "boolean".into(),
            "array".into(),
            "object".into(),
            "date".into(),
            "file".into(),
            "nil".into(),
        ],
        constraint_sorts: vec![
            "required".into(),
            "pattern".into(),
            "minLength".into(),
            "maxLength".into(),
            "minimum".into(),
            "maximum".into(),
            "enum".into(),
            "default".into(),
        ],
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for RAML.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(registry, "ThRamlSchema", "ThRamlInstance");
}

/// Parse a JSON-based RAML representation into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_raml_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut counter: usize = 0;

    // Parse types.
    if let Some(types) = json.get("types").and_then(serde_json::Value::as_object) {
        for (name, def) in types {
            builder = walk_raml_type(builder, def, name, &mut counter)?;
        }
    }

    // Parse resources.
    if let Some(resources) = json.get("resources").and_then(serde_json::Value::as_object) {
        for (path, res_def) in resources {
            let res_id = format!("resource:{path}");
            builder = builder.vertex(&res_id, "resource", None)?;

            if let Some(methods) = res_def
                .get("methods")
                .and_then(serde_json::Value::as_object)
            {
                for (method_name, method_def) in methods {
                    let method_id = format!("{res_id}:{method_name}");
                    builder = builder.vertex(&method_id, "method", None)?;
                    builder = builder.edge(&res_id, &method_id, "prop", Some(method_name))?;

                    if let Some(body) = method_def.get("body") {
                        counter += 1;
                        let body_id = format!("{method_id}:body{counter}");
                        builder = walk_raml_type(builder, body, &body_id, &mut counter)?;
                        builder = builder.edge(&method_id, &body_id, "prop", Some("body"))?;
                    }
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Walk a RAML type definition.
fn walk_raml_type(
    mut builder: SchemaBuilder,
    def: &serde_json::Value,
    current_id: &str,
    counter: &mut usize,
) -> Result<SchemaBuilder, ProtocolError> {
    let type_str = def
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("object");

    let kind = raml_type_to_kind(type_str);
    builder = builder.vertex(current_id, kind, None)?;

    // Constraints.
    for field in &[
        "pattern",
        "minLength",
        "maxLength",
        "minimum",
        "maximum",
        "default",
    ] {
        if let Some(val) = def.get(field) {
            let val_str = match val {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                _ => val.to_string(),
            };
            builder = builder.constraint(current_id, field, &val_str);
        }
    }

    if let Some(enum_val) = def.get("enum").and_then(serde_json::Value::as_array) {
        let vals: Vec<String> = enum_val
            .iter()
            .map(|v| v.as_str().map_or_else(|| v.to_string(), String::from))
            .collect();
        builder = builder.constraint(current_id, "enum", &vals.join(","));
    }

    // Properties.
    if let Some(properties) = def.get("properties").and_then(serde_json::Value::as_object) {
        for (prop_name, prop_def) in properties {
            let prop_id = format!("{current_id}.{prop_name}");
            builder = walk_raml_type(builder, prop_def, &prop_id, counter)?;
            builder = builder.edge(current_id, &prop_id, "prop", Some(prop_name))?;
        }
    }

    // Items.
    if let Some(items) = def.get("items") {
        let items_id = format!("{current_id}:items");
        builder = walk_raml_type(builder, items, &items_id, counter)?;
        builder = builder.edge(current_id, &items_id, "items", None)?;
    }

    Ok(builder)
}

/// Map RAML type to vertex kind.
fn raml_type_to_kind(t: &str) -> &'static str {
    match t {
        "string" => "string",
        "integer" => "integer",
        "number" => "number",
        "boolean" => "boolean",
        "array" => "array",
        "object" => "object",
        "date-only" | "time-only" | "datetime-only" | "datetime" => "date",
        "file" => "file",
        "nil" => "nil",
        _ => "type",
    }
}

/// Emit a [`Schema`] as a JSON RAML representation.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_raml_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &["prop", "items"];
    let roots = find_roots(schema, structural);

    let mut types = serde_json::Map::new();
    let mut resources = serde_json::Map::new();

    for root in &roots {
        if root.kind.as_str() == "resource" {
            let path = root.id.strip_prefix("resource:").unwrap_or(&root.id);
            let methods = children_by_edge(schema, &root.id, "prop");
            let mut methods_obj = serde_json::Map::new();
            for (edge, child) in &methods {
                let name = edge.name.as_deref().unwrap_or(&child.id);
                methods_obj.insert(name.to_string(), serde_json::json!({}));
            }
            resources.insert(
                path.to_string(),
                serde_json::json!({"methods": methods_obj}),
            );
        } else {
            let obj = emit_raml_type(schema, &root.id);
            types.insert(root.id.clone(), obj);
        }
    }

    let mut result = serde_json::Map::new();
    if !types.is_empty() {
        result.insert("types".into(), serde_json::Value::Object(types));
    }
    if !resources.is_empty() {
        result.insert("resources".into(), serde_json::Value::Object(resources));
    }

    Ok(serde_json::Value::Object(result))
}

/// Emit a single RAML type definition.
fn emit_raml_type(schema: &Schema, vertex_id: &str) -> serde_json::Value {
    let vertex = match schema.vertices.get(vertex_id) {
        Some(v) => v,
        None => return serde_json::json!({}),
    };

    let mut obj = serde_json::Map::new();
    obj.insert("type".into(), serde_json::json!(vertex.kind));

    for c in vertex_constraints(schema, vertex_id) {
        if let Ok(n) = c.value.parse::<i64>() {
            obj.insert(c.sort.clone(), serde_json::json!(n));
        } else {
            obj.insert(c.sort.clone(), serde_json::json!(c.value));
        }
    }

    let props = children_by_edge(schema, vertex_id, "prop");
    if !props.is_empty() {
        let mut properties = serde_json::Map::new();
        for (edge, child) in &props {
            let name = edge.name.as_deref().unwrap_or(&child.id);
            let val = emit_raml_type(schema, &child.id);
            properties.insert(name.to_string(), val);
        }
        obj.insert("properties".into(), serde_json::Value::Object(properties));
    }

    serde_json::Value::Object(obj)
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec![
                "resource".into(),
                "method".into(),
                "object".into(),
                "type".into(),
            ],
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
        assert_eq!(p.name, "raml");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThRamlSchema"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "types": {
                "User": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "age": {"type": "integer"}
                    }
                }
            },
            "resources": {
                "/users": {
                    "methods": {
                        "get": {}
                    }
                }
            }
        });
        let schema = parse_raml_schema(&json).expect("should parse");
        assert!(schema.has_vertex("User"));
        assert!(schema.has_vertex("resource:/users"));
        let emitted = emit_raml_schema(&schema).expect("emit");
        assert!(emitted.get("types").is_some());
    }

    #[test]
    fn roundtrip() {
        let json = serde_json::json!({
            "types": {
                "Item": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "integer"}
                    }
                }
            }
        });
        let s1 = parse_raml_schema(&json).expect("parse");
        let emitted = emit_raml_schema(&s1).expect("emit");
        let s2 = parse_raml_schema(&emitted).expect("re-parse");
        assert_eq!(s1.vertex_count(), s2.vertex_count());
    }
}
