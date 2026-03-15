//! W3C Web Annotation Data Model protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the W3C Web Annotation protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "web-annotation".into(),
        schema_theory: "ThWebAnnotationSchema".into(),
        instance_theory: "ThWebAnnotationInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "annotation".into(),
            "body".into(),
            "target".into(),
            "selector".into(),
            "text-body".into(),
            "text-position-selector".into(),
            "text-quote-selector".into(),
            "fragment-selector".into(),
            "css-selector".into(),
            "xpath-selector".into(),
            "range-selector".into(),
            "svg-selector".into(),
            "data-position-selector".into(),
            "time-selector".into(),
            "specific-resource".into(),
            "choice".into(),
            "agent".into(),
            "person".into(),
            "organization".into(),
            "software".into(),
            "string".into(),
            "integer".into(),
            "uri".into(),
        ],
        constraint_sorts: vec![
            "type".into(),
            "format".into(),
            "language".into(),
            "processing-language".into(),
            "text-direction".into(),
            "value".into(),
            "body-value".into(),
            "exact".into(),
            "prefix".into(),
            "suffix".into(),
            "start".into(),
            "end".into(),
            "conforms-to".into(),
            "motivation".into(),
            "purpose".into(),
            "created".into(),
            "modified".into(),
            "generated".into(),
            "rights".into(),
            "canonical".into(),
            "via".into(),
            "accessibility".into(),
        ],
        has_order: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for W3C Web Annotation.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThWebAnnotationSchema",
        "ThWebAnnotationInstance",
    );
}

/// All structural edge kinds used in the Web Annotation model.
const STRUCTURAL_EDGES: &[&str] = &[
    "has-body",
    "has-target",
    "has-selector",
    "has-source",
    "has-item",
    "refined-by",
    "creator",
    "generator",
    "has-field",
];

/// All constraint sorts that the parser reads from field definitions.
const FIELD_CONSTRAINT_SORTS: &[&str] = &[
    "type",
    "format",
    "language",
    "processing-language",
    "text-direction",
    "motivation",
    "purpose",
    "value",
    "body-value",
    "exact",
    "prefix",
    "suffix",
    "start",
    "end",
    "created",
    "modified",
    "generated",
    "rights",
    "canonical",
    "via",
    "accessibility",
    "conforms-to",
];

/// Map a field name to its edge kind, given the source vertex kind for context.
fn edge_kind_for_field(field_name: &str, src_kind: &str) -> &'static str {
    match field_name {
        "body" => "has-body",
        "target" => "has-target",
        "selector" => "has-selector",
        "source" => "has-source",
        "creator" => "creator",
        "generator" => "generator",
        _ => {
            // For items arrays the caller uses "has-item"; for plain
            // unknown fields on any vertex we fall back to "has-field".
            let _ = src_kind; // reserved for future context-sensitive dispatch
            "has-field"
        }
    }
}

/// Parse a JSON-based Web Annotation schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_web_annotation_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
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
            .unwrap_or("annotation");
        builder = builder.vertex(name, kind, None)?;

        if let Some(fields) = def.get("fields").and_then(serde_json::Value::as_object) {
            for (field_name, field_def) in fields {
                let field_id = format!("{name}.{field_name}");
                let field_kind = field_def
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("string");
                builder = builder.vertex(&field_id, field_kind, None)?;

                let edge_kind = edge_kind_for_field(field_name.as_str(), kind);
                builder = builder.edge(name, &field_id, edge_kind, Some(field_name))?;

                for sort in FIELD_CONSTRAINT_SORTS {
                    if let Some(val) = field_def.get(*sort).and_then(serde_json::Value::as_str) {
                        builder = builder.constraint(&field_id, sort, val);
                    }
                }

                // bodyValue: a plain-string value on the annotation itself
                if field_name == "bodyValue" {
                    if let Some(val) = field_def.get("value").and_then(serde_json::Value::as_str) {
                        builder = builder.constraint(name, "body-value", val);
                    }
                }
            }
        }

        // `items` is used by Choice bodies; dispatch per-item based on
        // the source vertex kind rather than always using "has-body".
        if let Some(items) = def.get("items").and_then(serde_json::Value::as_array) {
            for (i, item) in items.iter().enumerate() {
                if let Some(item_kind) = item.as_str() {
                    let item_id = format!("{name}:item{i}");
                    builder = builder.vertex(&item_id, item_kind, None)?;
                    // Choice vertices hold alternatives via has-item;
                    // annotation vertices accumulate bodies via has-body.
                    let items_edge = if kind == "choice" {
                        "has-item"
                    } else {
                        "has-body"
                    };
                    builder = builder.edge(name, &item_id, items_edge, Some(item_kind))?;
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON Web Annotation schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_web_annotation_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let roots = find_roots(schema, STRUCTURAL_EDGES);

    let mut types = serde_json::Map::new();
    for root in &roots {
        let mut obj = serde_json::Map::new();
        obj.insert("kind".into(), serde_json::json!(root.kind));

        let mut fields = serde_json::Map::new();
        for edge_kind in STRUCTURAL_EDGES {
            let children = children_by_edge(schema, &root.id, edge_kind);
            for (edge, child) in &children {
                let name = edge.name.as_deref().unwrap_or(&child.id);
                let mut field = serde_json::Map::new();
                field.insert("type".into(), serde_json::json!(child.kind));
                for c in vertex_constraints(schema, &child.id) {
                    field.insert(c.sort.to_string(), serde_json::json!(c.value));
                }
                fields.insert(name.to_string(), serde_json::Value::Object(field));
            }
        }
        if !fields.is_empty() {
            obj.insert("fields".into(), serde_json::Value::Object(fields));
        }

        // Emit bodyValue constraint on the annotation vertex itself.
        for c in vertex_constraints(schema, &root.id) {
            if c.sort == "body-value" {
                obj.insert("bodyValue".into(), serde_json::json!(c.value));
            }
        }

        types.insert(root.id.to_string(), serde_json::Value::Object(obj));
    }

    Ok(serde_json::json!({ "types": types }))
}

fn edge_rules() -> Vec<EdgeRule> {
    let all_selector_kinds: Vec<String> = vec![
        "selector".into(),
        "text-position-selector".into(),
        "text-quote-selector".into(),
        "fragment-selector".into(),
        "css-selector".into(),
        "xpath-selector".into(),
        "range-selector".into(),
        "svg-selector".into(),
        "data-position-selector".into(),
        "time-selector".into(),
    ];

    let all_agent_kinds: Vec<String> = vec![
        "agent".into(),
        "person".into(),
        "organization".into(),
        "software".into(),
    ];

    let all_body_kinds: Vec<String> = vec![
        "body".into(),
        "text-body".into(),
        "specific-resource".into(),
        "choice".into(),
    ];

    let all_target_kinds: Vec<String> =
        vec!["target".into(), "specific-resource".into(), "uri".into()];

    vec![
        EdgeRule {
            edge_kind: "has-body".into(),
            src_kinds: vec!["annotation".into()],
            tgt_kinds: all_body_kinds.clone(),
        },
        EdgeRule {
            edge_kind: "has-target".into(),
            src_kinds: vec!["annotation".into()],
            tgt_kinds: all_target_kinds,
        },
        EdgeRule {
            edge_kind: "has-selector".into(),
            src_kinds: vec!["specific-resource".into()],
            tgt_kinds: all_selector_kinds.clone(),
        },
        EdgeRule {
            edge_kind: "has-source".into(),
            src_kinds: vec!["specific-resource".into()],
            tgt_kinds: vec!["uri".into(), "string".into()],
        },
        EdgeRule {
            edge_kind: "has-item".into(),
            src_kinds: vec!["choice".into()],
            tgt_kinds: all_body_kinds,
        },
        EdgeRule {
            edge_kind: "refined-by".into(),
            src_kinds: all_selector_kinds.clone(),
            tgt_kinds: all_selector_kinds,
        },
        EdgeRule {
            edge_kind: "creator".into(),
            src_kinds: vec![
                "annotation".into(),
                "text-body".into(),
                "specific-resource".into(),
            ],
            tgt_kinds: all_agent_kinds.clone(),
        },
        EdgeRule {
            edge_kind: "generator".into(),
            src_kinds: vec!["annotation".into()],
            tgt_kinds: all_agent_kinds,
        },
        EdgeRule {
            edge_kind: "has-field".into(),
            src_kinds: vec![
                "annotation".into(),
                "body".into(),
                "target".into(),
                "text-body".into(),
                "specific-resource".into(),
                "choice".into(),
                "agent".into(),
                "person".into(),
                "organization".into(),
                "software".into(),
                "selector".into(),
                "text-position-selector".into(),
                "text-quote-selector".into(),
                "fragment-selector".into(),
                "css-selector".into(),
                "xpath-selector".into(),
                "range-selector".into(),
                "svg-selector".into(),
                "data-position-selector".into(),
                "time-selector".into(),
            ],
            tgt_kinds: vec!["string".into(), "integer".into(), "uri".into()],
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
        assert_eq!(p.name, "web-annotation");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThWebAnnotationSchema"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "types": {
                "anno1": {
                    "kind": "annotation",
                    "fields": {
                        "body": {"type": "text-body", "value": "Hello world"},
                        "target": {"type": "specific-resource", "format": "text/html"}
                    }
                }
            }
        });
        let schema = parse_web_annotation_schema(&json).expect("should parse");
        assert!(schema.has_vertex("anno1"));
        let emitted = emit_web_annotation_schema(&schema).expect("emit");
        let s2 = parse_web_annotation_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn new_vertex_kinds_present() {
        let p = protocol();
        for kind in &[
            "choice",
            "time-selector",
            "person",
            "organization",
            "software",
        ] {
            assert!(
                p.obj_kinds.iter().any(|k| k == kind),
                "missing obj_kind: {kind}"
            );
        }
    }

    #[test]
    fn new_constraint_sorts_present() {
        let p = protocol();
        for sort in &[
            "generated",
            "purpose",
            "accessibility",
            "rights",
            "canonical",
            "via",
            "body-value",
        ] {
            assert!(
                p.constraint_sorts.iter().any(|s| s == sort),
                "missing constraint_sort: {sort}"
            );
        }
    }

    #[test]
    fn choice_items_use_has_item_edge() {
        let json = serde_json::json!({
            "types": {
                "my-choice": {
                    "kind": "choice",
                    "items": ["text-body", "specific-resource"]
                },
                "anno2": {
                    "kind": "annotation",
                    "fields": {
                        "body": {"type": "choice"}
                    }
                }
            }
        });
        let schema = parse_web_annotation_schema(&json).expect("should parse choice");
        // Choice vertex should have has-item outgoing edges, not has-body.
        let outgoing = schema.outgoing_edges("my-choice");
        assert!(
            outgoing.iter().any(|e| e.kind == "has-item"),
            "expected has-item edges from choice vertex"
        );
        assert!(
            outgoing.iter().all(|e| e.kind != "has-body"),
            "choice vertex must not emit has-body edges for items"
        );
    }

    #[test]
    fn unknown_field_uses_has_field_edge() {
        let json = serde_json::json!({
            "types": {
                "anno3": {
                    "kind": "annotation",
                    "fields": {
                        "canonical": {"type": "uri"},
                        "via": {"type": "uri"}
                    }
                }
            }
        });
        let schema = parse_web_annotation_schema(&json).expect("should parse unknown fields");
        let outgoing = schema.outgoing_edges("anno3");
        assert!(
            outgoing.iter().all(|e| e.kind == "has-field"),
            "unknown fields should produce has-field edges, got: {:?}",
            outgoing.iter().map(|e| &e.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn body_value_constraint_round_trips() {
        let json = serde_json::json!({
            "types": {
                "anno4": {
                    "kind": "annotation",
                    "fields": {
                        "bodyValue": {"type": "string", "value": "tagging note"}
                    }
                }
            }
        });
        let schema = parse_web_annotation_schema(&json).expect("should parse bodyValue");
        let constraints = schema
            .constraints
            .get("anno4")
            .expect("constraints on anno4");
        assert!(
            constraints
                .iter()
                .any(|c| c.sort == "body-value" && c.value == "tagging note"),
            "body-value constraint not set on annotation vertex"
        );
    }

    #[test]
    fn motivation_and_generated_constraints() {
        let json = serde_json::json!({
            "types": {
                "anno5": {
                    "kind": "annotation",
                    "fields": {
                        "body": {
                            "type": "text-body",
                            "motivation": "commenting",
                            "generated": "2024-01-01T00:00:00Z"
                        }
                    }
                }
            }
        });
        let schema = parse_web_annotation_schema(&json).expect("should parse motivation+generated");
        let constraints = schema
            .constraints
            .get("anno5.body")
            .expect("constraints on body field");
        assert!(constraints.iter().any(|c| c.sort == "motivation"));
        assert!(constraints.iter().any(|c| c.sort == "generated"));
    }
}
