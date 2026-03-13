//! Open Document Format protocol definition.
//!
//! Uses Group E theory: constrained multigraph + W-type + metadata.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the ODF protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "odf".into(),
        schema_theory: "ThOdfSchema".into(),
        instance_theory: "ThOdfInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "document".into(),
            "body".into(),
            "paragraph".into(),
            "span".into(),
            "table".into(),
            "row".into(),
            "cell".into(),
            "frame".into(),
            "section".into(),
            "list".into(),
            "list-item".into(),
            "drawing".into(),
            "style".into(),
            "master-page".into(),
        ],
        constraint_sorts: vec!["style-family".into(), "master-page-name".into()],
    }
}

/// Register the component GATs for ODF.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_multigraph_wtype_meta(registry, "ThOdfSchema", "ThOdfInstance");
}

/// Parse a JSON-based ODF schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_odf_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
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

        if let Some(v) = def.get("style-family").and_then(serde_json::Value::as_str) {
            builder = builder.constraint(name, "style-family", v);
        }
        if let Some(v) = def
            .get("master-page-name")
            .and_then(serde_json::Value::as_str)
        {
            builder = builder.constraint(name, "master-page-name", v);
        }

        if let Some(children) = def.get("children").and_then(serde_json::Value::as_object) {
            for (child_name, child_def) in children {
                let child_id = format!("{name}.{child_name}");
                let child_kind = child_def
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("paragraph");
                builder = builder.vertex(&child_id, child_kind, None)?;
                builder = builder.edge(name, &child_id, "prop", Some(child_name))?;
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

/// Emit a [`Schema`] as a JSON ODF schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_odf_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &["prop", "items"];
    let roots = find_roots(schema, structural);

    let mut elements = serde_json::Map::new();
    for root in &roots {
        let mut obj = serde_json::Map::new();
        obj.insert("kind".into(), serde_json::json!(root.kind));

        for c in vertex_constraints(schema, &root.id) {
            obj.insert(c.sort.clone(), serde_json::json!(c.value));
        }

        let props = children_by_edge(schema, &root.id, "prop");
        if !props.is_empty() {
            let mut children = serde_json::Map::new();
            for (edge, child) in &props {
                let child_name = edge.name.as_deref().unwrap_or(&child.id);
                let mut child_obj = serde_json::Map::new();
                child_obj.insert("kind".into(), serde_json::json!(child.kind));
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

        elements.insert(root.id.clone(), serde_json::Value::Object(obj));
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
                "table".into(),
                "section".into(),
                "list".into(),
            ],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec!["document".into(), "body".into(), "list".into()],
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
        assert_eq!(p.name, "odf");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThOdfSchema"));
        assert!(registry.contains_key("ThOdfInstance"));
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
                },
                "heading-style": {
                    "kind": "style",
                    "style-family": "paragraph"
                }
            }
        });
        let schema = parse_odf_schema(&json).expect("should parse");
        assert!(schema.has_vertex("document"));

        let emitted = emit_odf_schema(&schema).expect("should emit");
        let s2 = parse_odf_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }
}
