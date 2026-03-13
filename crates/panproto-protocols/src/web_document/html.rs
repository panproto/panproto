//! HTML content model protocol definition.
//!
//! Uses Group E theory: constrained multigraph + W-type + metadata.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the HTML protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "html".into(),
        schema_theory: "ThHtmlSchema".into(),
        instance_theory: "ThHtmlInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "element".into(),
            "attribute".into(),
            "content-model".into(),
            "text".into(),
            "flow".into(),
            "phrasing".into(),
            "heading".into(),
            "sectioning".into(),
            "embedded".into(),
            "interactive".into(),
            "metadata".into(),
            "void".into(),
        ],
        constraint_sorts: vec![
            "required".into(),
            "deprecated".into(),
            "global".into(),
            "boolean-attr".into(),
            "content-categories".into(),
        ],
    }
}

/// Register the component GATs for HTML.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_multigraph_wtype_meta(registry, "ThHtmlSchema", "ThHtmlInstance");
}

/// Parse a JSON-based HTML element schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_html_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
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
            .unwrap_or("element");
        builder = builder.vertex(name, kind, None)?;

        // Parse attributes.
        if let Some(attrs) = def.get("attributes").and_then(serde_json::Value::as_object) {
            for (attr_name, attr_def) in attrs {
                let attr_id = format!("{name}.{attr_name}");
                builder = builder.vertex(&attr_id, "attribute", None)?;
                builder = builder.edge(name, &attr_id, "prop", Some(attr_name))?;

                if attr_def
                    .get("required")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                {
                    builder = builder.constraint(&attr_id, "required", "true");
                }
                if attr_def
                    .get("deprecated")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                {
                    builder = builder.constraint(&attr_id, "deprecated", "true");
                }
                if attr_def.get("global").and_then(serde_json::Value::as_bool) == Some(true) {
                    builder = builder.constraint(&attr_id, "global", "true");
                }
                if attr_def
                    .get("boolean-attr")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                {
                    builder = builder.constraint(&attr_id, "boolean-attr", "true");
                }
            }
        }

        // Parse content model children.
        if let Some(children) = def.get("children").and_then(serde_json::Value::as_array) {
            for child_name in children.iter().filter_map(serde_json::Value::as_str) {
                let cm_id = format!("{name}:child:{child_name}");
                builder = builder.vertex(&cm_id, "content-model", None)?;
                builder = builder.edge(name, &cm_id, "items", Some(child_name))?;
            }
        }

        // Content categories constraint.
        if let Some(cats) = def
            .get("content-categories")
            .and_then(serde_json::Value::as_str)
        {
            builder = builder.constraint(name, "content-categories", cats);
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON HTML schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_html_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &["prop", "items", "variant"];
    let roots = find_roots(schema, structural);

    let mut elements = serde_json::Map::new();
    for root in &roots {
        let mut obj = serde_json::Map::new();
        obj.insert("kind".into(), serde_json::json!(root.kind));

        let attrs = children_by_edge(schema, &root.id, "prop");
        if !attrs.is_empty() {
            let mut attrs_obj = serde_json::Map::new();
            for (edge, attr_v) in &attrs {
                let attr_name = edge.name.as_deref().unwrap_or(&attr_v.id);
                let mut single_attr = serde_json::Map::new();
                for c in vertex_constraints(schema, &attr_v.id) {
                    match c.sort.as_str() {
                        "required" | "deprecated" | "global" | "boolean-attr" => {
                            single_attr.insert(c.sort.clone(), serde_json::json!(true));
                        }
                        _ => {
                            single_attr.insert(c.sort.clone(), serde_json::json!(c.value));
                        }
                    }
                }
                attrs_obj.insert(
                    attr_name.to_string(),
                    serde_json::Value::Object(single_attr),
                );
            }
            obj.insert("attributes".into(), serde_json::Value::Object(attrs_obj));
        }

        let items = children_by_edge(schema, &root.id, "items");
        if !items.is_empty() {
            let children: Vec<serde_json::Value> = items
                .iter()
                .filter_map(|(e, _)| e.name.as_deref().map(|n| serde_json::json!(n)))
                .collect();
            obj.insert("children".into(), serde_json::Value::Array(children));
        }

        for c in vertex_constraints(schema, &root.id) {
            if c.sort == "content-categories" {
                obj.insert("content-categories".into(), serde_json::json!(c.value));
            }
        }

        elements.insert(root.id.clone(), serde_json::Value::Object(obj));
    }

    Ok(serde_json::json!({ "elements": elements }))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["element".into(), "void".into()],
            tgt_kinds: vec!["attribute".into()],
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec!["element".into()],
            tgt_kinds: vec!["content-model".into()],
        },
        EdgeRule {
            edge_kind: "variant".into(),
            src_kinds: vec![],
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
        assert_eq!(p.name, "html");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThHtmlSchema"));
        assert!(registry.contains_key("ThHtmlInstance"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "elements": {
                "div": {
                    "kind": "element",
                    "attributes": {
                        "id": {"global": true},
                        "class": {"global": true}
                    },
                    "children": ["p", "span"],
                    "content-categories": "flow"
                },
                "input": {
                    "kind": "void",
                    "attributes": {
                        "type": {"required": true},
                        "value": {}
                    }
                }
            }
        });
        let schema = parse_html_schema(&json).expect("should parse");
        assert!(schema.has_vertex("div"));
        assert!(schema.has_vertex("input"));
        assert!(schema.has_vertex("div.id"));

        let emitted = emit_html_schema(&schema).expect("should emit");
        assert!(emitted.get("elements").is_some());
    }

    #[test]
    fn roundtrip() {
        let json = serde_json::json!({
            "elements": {
                "p": {
                    "kind": "element",
                    "attributes": {
                        "class": {"global": true}
                    }
                }
            }
        });
        let s1 = parse_html_schema(&json).expect("parse");
        let emitted = emit_html_schema(&s1).expect("emit");
        let s2 = parse_html_schema(&emitted).expect("re-parse");
        assert_eq!(s1.vertex_count(), s2.vertex_count());
    }
}
