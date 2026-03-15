//! Ansible protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the Ansible protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "ansible".into(),
        schema_theory: "ThAnsibleSchema".into(),
        instance_theory: "ThAnsibleInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "playbook".into(),
            "play".into(),
            "task".into(),
            "role".into(),
            "variable".into(),
            "handler".into(),
            "string".into(),
            "integer".into(),
            "number".into(),
            "boolean".into(),
            "list".into(),
            "dict".into(),
        ],
        constraint_sorts: vec!["required".into(), "default".into()],
        has_order: true,
        has_recursion: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for Ansible.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThAnsibleSchema",
        "ThAnsibleInstance",
    );
}

/// Parse a JSON-based Ansible schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_ansible_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let modules = json
        .get("modules")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ProtocolError::MissingField("modules".into()))?;

    for (name, def) in modules {
        let kind = def
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("task");
        builder = builder.vertex(name, kind, None)?;

        if let Some(params) = def.get("parameters").and_then(serde_json::Value::as_object) {
            for (param_name, param_def) in params {
                let param_id = format!("{name}.{param_name}");
                let param_kind = param_def
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .map_or("string", ansible_type_to_kind);
                builder = builder.vertex(&param_id, param_kind, None)?;
                builder = builder.edge(name, &param_id, "prop", Some(param_name))?;

                if param_def
                    .get("required")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                {
                    builder = builder.constraint(&param_id, "required", "true");
                }
                if let Some(default) = param_def.get("default").and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(&param_id, "default", default);
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Map Ansible type to vertex kind.
fn ansible_type_to_kind(t: &str) -> &'static str {
    match t {
        "str" | "string" => "string",
        "int" | "integer" => "integer",
        "float" | "number" => "number",
        "bool" | "boolean" => "boolean",
        "list" => "list",
        "dict" => "dict",
        _ => "string",
    }
}

/// Emit a [`Schema`] as a JSON Ansible schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_ansible_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &["prop"];
    let roots = find_roots(schema, structural);

    let mut modules = serde_json::Map::new();
    for root in &roots {
        let mut obj = serde_json::Map::new();
        obj.insert("kind".into(), serde_json::json!(root.kind));

        let props = children_by_edge(schema, &root.id, "prop");
        if !props.is_empty() {
            let mut params = serde_json::Map::new();
            for (edge, child) in &props {
                let name = edge.name.as_deref().unwrap_or(&child.id);
                let mut param = serde_json::Map::new();
                param.insert("type".into(), serde_json::json!(child.kind));
                for c in vertex_constraints(schema, &child.id) {
                    if c.sort == "required" {
                        param.insert("required".into(), serde_json::json!(true));
                    } else {
                        param.insert(c.sort.to_string(), serde_json::json!(c.value));
                    }
                }
                params.insert(name.to_string(), serde_json::Value::Object(param));
            }
            obj.insert("parameters".into(), serde_json::Value::Object(params));
        }

        modules.insert(root.id.to_string(), serde_json::Value::Object(obj));
    }

    Ok(serde_json::json!({ "modules": modules }))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![EdgeRule {
        edge_kind: "prop".into(),
        src_kinds: vec![
            "playbook".into(),
            "play".into(),
            "task".into(),
            "role".into(),
        ],
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
        assert_eq!(p.name, "ansible");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThAnsibleSchema"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "modules": {
                "copy": {
                    "kind": "task",
                    "parameters": {
                        "src": {"type": "str", "required": true},
                        "dest": {"type": "str", "required": true},
                        "mode": {"type": "str", "default": "0644"}
                    }
                }
            }
        });
        let schema = parse_ansible_schema(&json).expect("should parse");
        assert!(schema.has_vertex("copy"));
        assert!(schema.has_vertex("copy.src"));
        let emitted = emit_ansible_schema(&schema).expect("emit");
        let s2 = parse_ansible_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }
}
