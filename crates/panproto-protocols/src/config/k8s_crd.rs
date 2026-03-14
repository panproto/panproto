//! Kubernetes CRD protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the K8s CRD protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "k8s-crd".into(),
        schema_theory: "ThK8sCrdSchema".into(),
        instance_theory: "ThK8sCrdInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "custom-resource".into(),
            "version".into(),
            "schema-object".into(),
            "field".into(),
            "string".into(),
            "integer".into(),
            "number".into(),
            "boolean".into(),
            "array".into(),
            "object".into(),
        ],
        constraint_sorts: vec!["required".into(), "format".into()],
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for K8s CRD.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(registry, "ThK8sCrdSchema", "ThK8sCrdInstance");
}

/// Parse a JSON-based K8s CRD schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_k8s_crd_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut counter: usize = 0;

    let name = json
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ProtocolError::MissingField("name".into()))?;

    builder = builder.vertex(name, "custom-resource", None)?;

    if let Some(versions) = json.get("versions").and_then(serde_json::Value::as_array) {
        for ver in versions.iter() {
            let ver_name = ver
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("v1");
            let ver_id = format!("{name}:{ver_name}");
            builder = builder.vertex(&ver_id, "version", None)?;
            builder = builder.edge(name, &ver_id, "prop", Some(ver_name))?;

            if let Some(schema_val) = ver.get("schema") {
                builder = walk_k8s_schema(builder, schema_val, &ver_id, &mut counter)?;
            }
        }
    }

    if let Some(schema_val) = json.get("schema") {
        builder = walk_k8s_schema(builder, schema_val, name, &mut counter)?;
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Walk a K8s CRD openAPIV3Schema recursively.
fn walk_k8s_schema(
    mut builder: SchemaBuilder,
    schema: &serde_json::Value,
    parent_id: &str,
    counter: &mut usize,
) -> Result<SchemaBuilder, ProtocolError> {
    if let Some(properties) = schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
    {
        for (prop_name, prop_def) in properties {
            let prop_id = format!("{parent_id}.{prop_name}");
            let type_str = prop_def
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("string");
            let kind = k8s_type_to_kind(type_str);
            builder = builder.vertex(&prop_id, kind, None)?;
            builder = builder.edge(parent_id, &prop_id, "prop", Some(prop_name))?;

            // Recurse into nested objects.
            if type_str == "object" {
                builder = walk_k8s_schema(builder, prop_def, &prop_id, counter)?;
            }

            // Handle array items.
            if type_str == "array" {
                if let Some(items) = prop_def.get("items") {
                    *counter += 1;
                    let items_id = format!("{prop_id}:items{counter}");
                    let items_type = items
                        .get("type")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("string");
                    let items_kind = k8s_type_to_kind(items_type);
                    builder = builder.vertex(&items_id, items_kind, None)?;
                    builder = builder.edge(&prop_id, &items_id, "items", None)?;
                    if items_type == "object" {
                        builder = walk_k8s_schema(builder, items, &items_id, counter)?;
                    }
                }
            }
        }
    }

    Ok(builder)
}

/// Map K8s type to vertex kind.
fn k8s_type_to_kind(type_str: &str) -> &'static str {
    match type_str {
        "string" => "string",
        "integer" => "integer",
        "number" => "number",
        "boolean" => "boolean",
        "array" => "array",
        "object" => "object",
        _ => "field",
    }
}

/// Emit a [`Schema`] as a JSON K8s CRD schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_k8s_crd_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &["prop", "items"];
    let roots = find_roots(schema, structural);

    let root = roots
        .iter()
        .find(|v| v.kind == "custom-resource")
        .ok_or_else(|| ProtocolError::Emit("no custom-resource root".into()))?;

    let mut result = serde_json::Map::new();
    result.insert("name".into(), serde_json::json!(root.id));

    let versions = children_by_edge(schema, &root.id, "prop");
    let ver_arr: Vec<serde_json::Value> = versions
        .iter()
        .filter(|(_, v)| v.kind == "version")
        .map(|(e, v)| {
            let ver_name = e.name.as_deref().unwrap_or(&v.id);
            let schema_obj = emit_k8s_schema_obj(schema, &v.id);
            serde_json::json!({
                "name": ver_name,
                "schema": schema_obj,
            })
        })
        .collect();

    if !ver_arr.is_empty() {
        result.insert("versions".into(), serde_json::Value::Array(ver_arr));
    }

    Ok(serde_json::Value::Object(result))
}

/// Emit a schema object with properties.
fn emit_k8s_schema_obj(schema: &Schema, vertex_id: &str) -> serde_json::Value {
    let props = children_by_edge(schema, vertex_id, "prop");
    if props.is_empty() {
        return serde_json::json!({});
    }

    let mut properties = serde_json::Map::new();
    for (edge, child) in &props {
        let name = edge.name.as_deref().unwrap_or(&child.id);
        let mut obj = serde_json::Map::new();
        obj.insert("type".into(), serde_json::json!(child.kind));

        if child.kind == "object" {
            let nested = emit_k8s_schema_obj(schema, &child.id);
            if let serde_json::Value::Object(nested_obj) = &nested {
                if let Some(nested_props) = nested_obj.get("properties") {
                    obj.insert("properties".into(), nested_props.clone());
                }
            }
        }

        properties.insert(name.to_string(), serde_json::Value::Object(obj));
    }

    serde_json::json!({ "properties": properties })
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec![
                "custom-resource".into(),
                "version".into(),
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
    ]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_def() {
        let p = protocol();
        assert_eq!(p.name, "k8s-crd");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThK8sCrdSchema"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "name": "MyResource",
            "versions": [{
                "name": "v1",
                "schema": {
                    "properties": {
                        "spec": {"type": "object", "properties": {
                            "replicas": {"type": "integer"}
                        }},
                        "status": {"type": "string"}
                    }
                }
            }]
        });
        let schema = parse_k8s_crd_schema(&json).expect("should parse");
        assert!(schema.has_vertex("MyResource"));
        let emitted = emit_k8s_crd_schema(&schema).expect("emit");
        assert!(emitted.get("name").is_some());
    }
}
