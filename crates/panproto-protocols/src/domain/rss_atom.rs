//! RSS/Atom feed schema protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the RSS/Atom protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "rss-atom".into(),
        schema_theory: "ThRssAtomSchema".into(),
        instance_theory: "ThRssAtomInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "feed".into(),
            "entry".into(),
            "author".into(),
            "category".into(),
            "link".into(),
            "content".into(),
            "summary".into(),
            "title".into(),
            "published".into(),
            "updated".into(),
            "id".into(),
            "string".into(),
            "date".into(),
            "uri".into(),
        ],
        constraint_sorts: vec!["required".into(), "format".into()],
    }
}

/// Register the component GATs for RSS/Atom.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThRssAtomSchema",
        "ThRssAtomInstance",
    );
}

/// Parse a JSON-based RSS/Atom schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_rss_atom_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let types = json
        .get("types")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ProtocolError::MissingField("types".into()))?;

    for (name, def) in types {
        let kind = def
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("feed");
        builder = builder.vertex(name, kind, None)?;

        if let Some(fields) = def.get("fields").and_then(serde_json::Value::as_object) {
            for (field_name, field_def) in fields {
                let field_id = format!("{name}.{field_name}");
                let field_kind = field_def
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("string");
                builder = builder.vertex(&field_id, field_kind, None)?;
                builder = builder.edge(name, &field_id, "prop", Some(field_name))?;

                if field_def
                    .get("required")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                {
                    builder = builder.constraint(&field_id, "required", "true");
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

/// Emit a [`Schema`] as a JSON RSS/Atom schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_rss_atom_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &["prop", "items"];
    let roots = find_roots(schema, structural);

    let mut types = serde_json::Map::new();
    for root in &roots {
        let mut obj = serde_json::Map::new();
        obj.insert("kind".into(), serde_json::json!(root.kind));

        let props = children_by_edge(schema, &root.id, "prop");
        if !props.is_empty() {
            let mut fields = serde_json::Map::new();
            for (edge, child) in &props {
                let name = edge.name.as_deref().unwrap_or(&child.id);
                let mut field = serde_json::Map::new();
                field.insert("type".into(), serde_json::json!(child.kind));
                for c in vertex_constraints(schema, &child.id) {
                    if c.sort == "required" {
                        field.insert("required".into(), serde_json::json!(true));
                    }
                }
                fields.insert(name.to_string(), serde_json::Value::Object(field));
            }
            obj.insert("fields".into(), serde_json::Value::Object(fields));
        }

        let items = children_by_edge(schema, &root.id, "items");
        if !items.is_empty() {
            let arr: Vec<serde_json::Value> = items
                .iter()
                .filter_map(|(e, _)| e.name.as_deref().map(|n| serde_json::json!(n)))
                .collect();
            obj.insert("items".into(), serde_json::Value::Array(arr));
        }

        types.insert(root.id.clone(), serde_json::Value::Object(obj));
    }

    Ok(serde_json::json!({ "types": types }))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["feed".into(), "entry".into(), "author".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec!["feed".into()],
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
        assert_eq!(p.name, "rss-atom");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThRssAtomSchema"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "types": {
                "feed": {
                    "kind": "feed",
                    "fields": {
                        "title": {"type": "string", "required": true}
                    },
                    "items": ["entry"]
                }
            }
        });
        let schema = parse_rss_atom_schema(&json).expect("should parse");
        assert!(schema.has_vertex("feed"));
        let emitted = emit_rss_atom_schema(&schema).expect("emit");
        let s2 = parse_rss_atom_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }
}
