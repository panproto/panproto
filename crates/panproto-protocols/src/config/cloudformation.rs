//! AWS CloudFormation protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the `CloudFormation` protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "cloudformation".into(),
        schema_theory: "ThCfnSchema".into(),
        instance_theory: "ThCfnInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "resource-type".into(),
            "property".into(),
            "string".into(),
            "integer".into(),
            "number".into(),
            "boolean".into(),
            "array".into(),
            "object".into(),
            "timestamp".into(),
        ],
        constraint_sorts: vec!["required".into(), "default".into(), "allowed-values".into()],
    }
}

/// Register the component GATs for `CloudFormation`.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(registry, "ThCfnSchema", "ThCfnInstance");
}

/// Parse a JSON-based `CloudFormation` resource schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_cfn_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let resources = json
        .get("resourceTypes")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ProtocolError::MissingField("resourceTypes".into()))?;

    for (name, def) in resources {
        builder = builder.vertex(name, "resource-type", None)?;

        if let Some(props) = def.get("properties").and_then(serde_json::Value::as_object) {
            for (prop_name, prop_def) in props {
                let prop_id = format!("{name}.{prop_name}");
                let kind = prop_def
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .map_or("string", cfn_type_to_kind);
                builder = builder.vertex(&prop_id, kind, None)?;
                builder = builder.edge(name, &prop_id, "prop", Some(prop_name))?;

                if prop_def
                    .get("required")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                {
                    builder = builder.constraint(&prop_id, "required", "true");
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Map `CloudFormation` type to vertex kind.
fn cfn_type_to_kind(t: &str) -> &'static str {
    match t {
        "String" | "string" => "string",
        "Integer" | "integer" => "integer",
        "Number" | "number" | "Double" => "number",
        "Boolean" | "boolean" => "boolean",
        "List" | "array" => "array",
        "Map" | "object" | "Json" => "object",
        "Timestamp" | "timestamp" => "timestamp",
        _ => "property",
    }
}

/// Emit a [`Schema`] as a JSON `CloudFormation` schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_cfn_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &["prop"];
    let roots = find_roots(schema, structural);

    let mut resource_types = serde_json::Map::new();
    for root in &roots {
        if root.kind != "resource-type" {
            continue;
        }
        let mut obj = serde_json::Map::new();
        let props = children_by_edge(schema, &root.id, "prop");
        if !props.is_empty() {
            let mut props_obj = serde_json::Map::new();
            for (edge, child) in &props {
                let name = edge.name.as_deref().unwrap_or(&child.id);
                let mut single_prop = serde_json::Map::new();
                single_prop.insert("type".into(), serde_json::json!(child.kind));
                for c in vertex_constraints(schema, &child.id) {
                    if c.sort == "required" {
                        single_prop.insert("required".into(), serde_json::json!(true));
                    } else {
                        single_prop.insert(c.sort.clone(), serde_json::json!(c.value));
                    }
                }
                props_obj.insert(name.to_string(), serde_json::Value::Object(single_prop));
            }
            obj.insert("properties".into(), serde_json::Value::Object(props_obj));
        }
        resource_types.insert(root.id.clone(), serde_json::Value::Object(obj));
    }

    Ok(serde_json::json!({ "resourceTypes": resource_types }))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![EdgeRule {
        edge_kind: "prop".into(),
        src_kinds: vec!["resource-type".into(), "object".into()],
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
        assert_eq!(p.name, "cloudformation");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThCfnSchema"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "resourceTypes": {
                "AWS::EC2::Instance": {
                    "properties": {
                        "InstanceType": {"type": "String", "required": true},
                        "ImageId": {"type": "String"}
                    }
                }
            }
        });
        let schema = parse_cfn_schema(&json).expect("should parse");
        assert!(schema.has_vertex("AWS::EC2::Instance"));
        let emitted = emit_cfn_schema(&schema).expect("emit");
        let s2 = parse_cfn_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }
}
