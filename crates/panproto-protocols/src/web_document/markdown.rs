//! Markdown document structure protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the Markdown protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "markdown".into(),
        schema_theory: "ThMarkdownSchema".into(),
        instance_theory: "ThMarkdownInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "document".into(),
            "heading".into(),
            "paragraph".into(),
            "list".into(),
            "list-item".into(),
            "code-block".into(),
            "blockquote".into(),
            "table".into(),
            "table-row".into(),
            "table-cell".into(),
            "link".into(),
            "image".into(),
            "emphasis".into(),
            "strong".into(),
            "code-span".into(),
            "html-block".into(),
            "thematic-break".into(),
            "front-matter".into(),
        ],
        constraint_sorts: vec![],
        has_order: true,
        has_recursion: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for Markdown.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThMarkdownSchema",
        "ThMarkdownInstance",
    );
}

/// Parse a JSON-based Markdown document schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_markdown_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let nodes = json
        .get("nodes")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ProtocolError::MissingField("nodes".into()))?;

    for (name, def) in nodes {
        let kind = def
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("document");
        builder = builder.vertex(name, kind, None)?;

        if let Some(children) = def.get("children").and_then(serde_json::Value::as_array) {
            for (i, child) in children.iter().enumerate() {
                if let Some(child_kind) = child.as_str() {
                    let child_id = format!("{name}:child{i}");
                    builder = builder.vertex(&child_id, child_kind, None)?;
                    builder = builder.edge(name, &child_id, "items", Some(child_kind))?;
                }
            }
        }

        if let Some(props) = def.get("properties").and_then(serde_json::Value::as_object) {
            for (prop_name, prop_def) in props {
                let prop_id = format!("{name}.{prop_name}");
                let prop_kind = prop_def
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("paragraph");
                builder = builder.vertex(&prop_id, prop_kind, None)?;
                builder = builder.edge(name, &prop_id, "prop", Some(prop_name))?;
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON Markdown schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_markdown_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &["prop", "items"];
    let roots = find_roots(schema, structural);

    let mut nodes = serde_json::Map::new();
    for root in &roots {
        let mut obj = serde_json::Map::new();
        obj.insert("kind".into(), serde_json::json!(root.kind));

        let items = children_by_edge(schema, &root.id, "items");
        if !items.is_empty() {
            let arr: Vec<serde_json::Value> = items
                .iter()
                .filter_map(|(e, _)| e.name.as_deref().map(|n| serde_json::json!(n)))
                .collect();
            obj.insert("children".into(), serde_json::Value::Array(arr));
        }

        let props = children_by_edge(schema, &root.id, "prop");
        if !props.is_empty() {
            let mut props_obj = serde_json::Map::new();
            for (edge, child) in &props {
                let prop_name = edge.name.as_deref().unwrap_or(&child.id);
                props_obj.insert(
                    prop_name.to_string(),
                    serde_json::json!({"kind": child.kind}),
                );
            }
            obj.insert("properties".into(), serde_json::Value::Object(props_obj));
        }

        nodes.insert(root.id.to_string(), serde_json::Value::Object(obj));
    }

    Ok(serde_json::json!({ "nodes": nodes }))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["document".into(), "heading".into(), "link".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec![
                "document".into(),
                "list".into(),
                "list-item".into(),
                "blockquote".into(),
                "table".into(),
                "table-row".into(),
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
        assert_eq!(p.name, "markdown");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThMarkdownSchema"));
        assert!(registry.contains_key("ThMarkdownInstance"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "nodes": {
                "doc": {
                    "kind": "document",
                    "children": ["heading", "paragraph", "list"]
                }
            }
        });
        let schema = parse_markdown_schema(&json).expect("should parse");
        assert!(schema.has_vertex("doc"));

        let emitted = emit_markdown_schema(&schema).expect("should emit");
        let s2 = parse_markdown_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }
}
