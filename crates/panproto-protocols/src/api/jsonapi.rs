//! JSON:API protocol definition.
//!
//! JSON:API uses a constrained multigraph schema theory
//! (`colimit(ThGraph, ThConstraint, ThMulti)`) and a W-type
//! instance theory (`ThWType`).
//!
//! Vertex kinds: resource-type, attribute, relationship,
//! string, integer, number, boolean, array, object.
//!
//! Edge kinds: prop, ref.

use std::collections::HashMap;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the JSON:API protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "jsonapi".into(),
        schema_theory: "ThJsonAPISchema".into(),
        instance_theory: "ThJsonAPIInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "resource-type".into(),
            "attribute".into(),
            "relationship".into(),
            "string".into(),
            "integer".into(),
            "number".into(),
            "boolean".into(),
            "array".into(),
            "object".into(),
        ],
        constraint_sorts: vec!["required".into()],
        has_order: true,
        has_recursion: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for JSON:API with a theory registry.
pub fn register_theories<S: ::std::hash::BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThJsonAPISchema",
        "ThJsonAPIInstance",
    );
}

/// Parse a JSON:API schema document into a [`Schema`].
///
/// Expects a JSON object describing resource types with their
/// attributes and relationships.
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing or schema construction fails.
pub fn parse_jsonapi(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let resources = json
        .get("resources")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ProtocolError::MissingField("resources".into()))?;

    for (res_name, res_def) in resources {
        let resource_id = format!("resource:{res_name}");
        builder = builder.vertex(&resource_id, "resource-type", None)?;

        // Attributes.
        if let Some(attrs) = res_def
            .get("attributes")
            .and_then(serde_json::Value::as_object)
        {
            let required_fields: Vec<&str> = res_def
                .get("required")
                .and_then(serde_json::Value::as_array)
                .map(|arr| arr.iter().filter_map(serde_json::Value::as_str).collect())
                .unwrap_or_default();

            for (attr_name, attr_def) in attrs {
                let attr_id = format!("{resource_id}.{attr_name}");
                let attr_type = attr_def
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("string");

                let kind = match attr_type {
                    "integer" => "integer",
                    "number" => "number",
                    "boolean" => "boolean",
                    "array" => "array",
                    "object" => "object",
                    _ => "string",
                };

                builder = builder.vertex(&attr_id, kind, None)?;
                builder = builder.edge(&resource_id, &attr_id, "prop", Some(attr_name))?;

                if required_fields.contains(&attr_name.as_str()) {
                    builder = builder.constraint(&attr_id, "required", "true");
                }
            }
        }

        // Relationships.
        if let Some(relationships) = res_def
            .get("relationships")
            .and_then(serde_json::Value::as_object)
        {
            for (relationship_name, relationship_def) in relationships {
                let relationship_id = format!("{resource_id}:rel:{relationship_name}");
                builder = builder.vertex(&relationship_id, "relationship", None)?;
                builder = builder.edge(
                    &resource_id,
                    &relationship_id,
                    "prop",
                    Some(relationship_name),
                )?;

                // Target resource type.
                if let Some(target) = relationship_def
                    .get("target")
                    .and_then(serde_json::Value::as_str)
                {
                    let target_id = format!("resource:{target}");
                    // Store target as a constraint since the target vertex may not exist yet.
                    builder = builder.constraint(&relationship_id, "required", &target_id);
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON:API schema document.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_jsonapi(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let mut resources = serde_json::Map::new();

    let roots = find_roots(schema, &["prop", "ref"]);

    for root in &roots {
        if root.kind != "resource-type" {
            continue;
        }
        let resource_name = root.id.strip_prefix("resource:").unwrap_or(&root.id);
        let mut resource_obj = serde_json::Map::new();
        let mut attrs = serde_json::Map::new();
        let mut relationships = serde_json::Map::new();
        let mut required_list = Vec::new();

        for (edge, child) in children_by_edge(schema, &root.id, "prop") {
            let name = edge.name.as_deref().unwrap_or("");
            if child.kind == "relationship" {
                let mut relationship_obj = serde_json::Map::new();
                if let Some(target_val) = constraint_value(schema, &child.id, "required") {
                    let target_name = target_val.strip_prefix("resource:").unwrap_or(target_val);
                    relationship_obj.insert(
                        "target".into(),
                        serde_json::Value::String(target_name.to_string()),
                    );
                }
                relationships.insert(
                    name.to_string(),
                    serde_json::Value::Object(relationship_obj),
                );
            } else {
                let type_name = match child.kind.as_str() {
                    "integer" | "number" | "boolean" | "array" | "object" => child.kind.as_str(),
                    _ => "string",
                };
                attrs.insert(name.to_string(), serde_json::json!({"type": type_name}));
                if constraint_value(schema, &child.id, "required") == Some("true") {
                    required_list.push(serde_json::Value::String(name.to_string()));
                }
            }
        }

        if !attrs.is_empty() {
            resource_obj.insert("attributes".into(), serde_json::Value::Object(attrs));
        }
        if !relationships.is_empty() {
            resource_obj.insert(
                "relationships".into(),
                serde_json::Value::Object(relationships),
            );
        }
        if !required_list.is_empty() {
            resource_obj.insert("required".into(), serde_json::Value::Array(required_list));
        }

        resources.insert(
            resource_name.to_string(),
            serde_json::Value::Object(resource_obj),
        );
    }

    let mut result = serde_json::Map::new();
    result.insert("resources".into(), serde_json::Value::Object(resources));

    Ok(serde_json::Value::Object(result))
}

/// Well-formedness rules for JSON:API edges.
fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["resource-type".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "ref".into(),
            src_kinds: vec!["relationship".into()],
            tgt_kinds: vec!["resource-type".into()],
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
        assert_eq!(p.name, "jsonapi");
        assert_eq!(p.schema_theory, "ThJsonAPISchema");
        assert_eq!(p.instance_theory, "ThJsonAPIInstance");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThJsonAPISchema"));
        assert!(registry.contains_key("ThJsonAPIInstance"));
    }

    #[test]
    fn parse_minimal() {
        let doc = serde_json::json!({
            "resources": {
                "articles": {
                    "attributes": {
                        "title": {"type": "string"},
                        "body": {"type": "string"}
                    },
                    "relationships": {
                        "author": {"target": "people"}
                    },
                    "required": ["title"]
                },
                "people": {
                    "attributes": {
                        "name": {"type": "string"}
                    }
                }
            }
        });
        let schema = parse_jsonapi(&doc).expect("should parse");
        assert!(schema.has_vertex("resource:articles"));
        assert!(schema.has_vertex("resource:people"));
        assert!(schema.has_vertex("resource:articles.title"));
    }

    #[test]
    fn emit_minimal() {
        let doc = serde_json::json!({
            "resources": {
                "posts": {
                    "attributes": {
                        "title": {"type": "string"}
                    }
                }
            }
        });
        let schema = parse_jsonapi(&doc).expect("should parse");
        let emitted = emit_jsonapi(&schema).expect("should emit");
        assert!(emitted.get("resources").is_some());
    }

    #[test]
    fn roundtrip() {
        let doc = serde_json::json!({
            "resources": {
                "users": {
                    "attributes": {
                        "name": {"type": "string"},
                        "age": {"type": "integer"}
                    }
                }
            }
        });
        let schema = parse_jsonapi(&doc).expect("parse");
        let emitted = emit_jsonapi(&schema).expect("emit");
        let schema2 = parse_jsonapi(&emitted).expect("re-parse");
        assert_eq!(schema.vertices.len(), schema2.vertices.len());
    }
}
