//! DOCX/Office Open XML protocol definition.
//!
//! Uses Group E theory: constrained multigraph + W-type + metadata.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the DOCX protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "docx".into(),
        schema_theory: "ThDocxSchema".into(),
        instance_theory: "ThDocxInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "document".into(),
            "body".into(),
            "paragraph".into(),
            "run".into(),
            "text".into(),
            "table".into(),
            "row".into(),
            "cell".into(),
            "section".into(),
            "header".into(),
            "footer".into(),
            "style".into(),
            "numbering".into(),
            "footnote".into(),
            "image".into(),
            "hyperlink".into(),
        ],
        constraint_sorts: vec![
            "required".into(),
            "style-type".into(),
            "numbering-format".into(),
        ],
        has_order: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for DOCX.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_multigraph_wtype_meta(registry, "ThDocxSchema", "ThDocxInstance");
}

/// Parse a JSON-based DOCX content model into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_docx_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let elements = json
        .get("elements")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ProtocolError::MissingField("elements".into()))?;

    for (name, def) in elements {
        let kind = def
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("document");
        builder = builder.vertex(name, kind, None)?;

        for field in &["required", "style-type", "numbering-format"] {
            if let Some(val) = def.get(field).and_then(serde_json::Value::as_str) {
                builder = builder.constraint(name, field, val);
            }
        }

        if let Some(children) = def.get("children").and_then(serde_json::Value::as_object) {
            for (child_name, child_def) in children {
                let child_id = format!("{name}.{child_name}");
                let child_kind = child_def
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("text");
                builder = builder.vertex(&child_id, child_kind, None)?;
                builder = builder.edge(name, &child_id, "prop", Some(child_name))?;

                for field in &["required", "style-type"] {
                    if let Some(val) = child_def.get(field).and_then(serde_json::Value::as_str) {
                        builder = builder.constraint(&child_id, field, val);
                    }
                }
            }
        }

        if let Some(items) = def.get("items").and_then(serde_json::Value::as_array) {
            for (i, item) in items.iter().enumerate() {
                if let Some(item_kind) = item.as_str() {
                    let item_id = format!("{name}:item{i}");
                    builder = builder.vertex(&item_id, item_kind, None)?;
                    builder = builder.edge(name, &item_id, "items", Some(item_kind))?;
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON DOCX schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_docx_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &["prop", "items"];
    let roots = find_roots(schema, structural);

    let mut elements = serde_json::Map::new();
    for root in &roots {
        let mut obj = serde_json::Map::new();
        obj.insert("kind".into(), serde_json::json!(root.kind));

        for c in vertex_constraints(schema, &root.id) {
            obj.insert(c.sort.to_string(), serde_json::json!(c.value));
        }

        let props = children_by_edge(schema, &root.id, "prop");
        if !props.is_empty() {
            let mut children = serde_json::Map::new();
            for (edge, child) in &props {
                let child_name = edge.name.as_deref().unwrap_or(&child.id);
                let mut child_obj = serde_json::Map::new();
                child_obj.insert("kind".into(), serde_json::json!(child.kind));
                for c in vertex_constraints(schema, &child.id) {
                    child_obj.insert(c.sort.to_string(), serde_json::json!(c.value));
                }
                children.insert(child_name.to_string(), serde_json::Value::Object(child_obj));
            }
            obj.insert("children".into(), serde_json::Value::Object(children));
        }

        let items = children_by_edge(schema, &root.id, "items");
        if !items.is_empty() {
            let arr: Vec<serde_json::Value> = items
                .iter()
                .filter_map(|(e, _)| e.name.as_deref().map(|n| serde_json::json!(n)))
                .collect();
            obj.insert("items".into(), serde_json::Value::Array(arr));
        }

        elements.insert(root.id.to_string(), serde_json::Value::Object(obj));
    }

    Ok(serde_json::json!({ "elements": elements }))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec![
                "document".into(),
                "body".into(),
                "paragraph".into(),
                "run".into(),
                "table".into(),
                "row".into(),
                "cell".into(),
                "section".into(),
            ],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec![
                "document".into(),
                "body".into(),
                "paragraph".into(),
                "table".into(),
            ],
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
        assert_eq!(p.name, "docx");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThDocxSchema"));
        assert!(registry.contains_key("ThDocxInstance"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "elements": {
                "document": {
                    "kind": "document",
                    "children": {
                        "body": {"kind": "body"}
                    },
                    "items": ["paragraph", "table"]
                }
            }
        });
        let schema = parse_docx_schema(&json).expect("should parse");
        assert!(schema.has_vertex("document"));
        assert!(schema.has_vertex("document.body"));

        let emitted = emit_docx_schema(&schema).expect("should emit");
        let s2 = parse_docx_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }
}
