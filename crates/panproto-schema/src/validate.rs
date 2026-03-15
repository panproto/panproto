//! Schema validation against a protocol's rules.
//!
//! The [`validate`] function checks a finished [`Schema`] against a
//! [`Protocol`]'s edge rules, vertex kinds, and constraint sorts,
//! returning a list of all violations found.

use crate::error::ValidationError;
use crate::protocol::Protocol;
use crate::schema::Schema;

/// Validate a schema against a protocol's structural rules.
///
/// Checks:
/// 1. All vertex kinds are recognized by the protocol.
/// 2. All edges satisfy the protocol's edge rules.
/// 3. All constraint sorts are recognized by the protocol.
/// 4. All required edges reference vertices that exist.
///
/// Returns an empty vector if the schema is valid.
#[must_use]
pub fn validate(schema: &Schema, protocol: &Protocol) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // 1. Check vertex kinds.
    if !protocol.obj_kinds.is_empty() || !protocol.edge_rules.is_empty() {
        for (id, vertex) in &schema.vertices {
            if !protocol.is_known_vertex_kind(&vertex.kind) {
                errors.push(ValidationError::InvalidVertexKind {
                    vertex: id.to_string(),
                    kind: vertex.kind.to_string(),
                });
            }
        }
    }

    // 2. Check edge rules.
    for edge in schema.edges.keys() {
        if let Some(rule) = protocol.find_edge_rule(&edge.kind) {
            // Check source kind.
            if let Some(src_vertex) = schema.vertices.get(&edge.src) {
                if !rule.src_kinds.is_empty()
                    && !rule.src_kinds.iter().any(|k| k == src_vertex.kind.as_ref())
                {
                    errors.push(ValidationError::InvalidEdge {
                        src: edge.src.to_string(),
                        tgt: edge.tgt.to_string(),
                        kind: edge.kind.to_string(),
                        reason: format!(
                            "source kind '{}' not in permitted: {:?}",
                            src_vertex.kind, rule.src_kinds
                        ),
                    });
                }
            }
            // Check target kind.
            if let Some(tgt_vertex) = schema.vertices.get(&edge.tgt) {
                if !rule.tgt_kinds.is_empty()
                    && !rule.tgt_kinds.iter().any(|k| k == tgt_vertex.kind.as_ref())
                {
                    errors.push(ValidationError::InvalidEdge {
                        src: edge.src.to_string(),
                        tgt: edge.tgt.to_string(),
                        kind: edge.kind.to_string(),
                        reason: format!(
                            "target kind '{}' not in permitted: {:?}",
                            tgt_vertex.kind, rule.tgt_kinds
                        ),
                    });
                }
            }
        } else if !protocol.edge_rules.is_empty() {
            errors.push(ValidationError::InvalidEdge {
                src: edge.src.to_string(),
                tgt: edge.tgt.to_string(),
                kind: edge.kind.to_string(),
                reason: format!("unknown edge kind '{}'", edge.kind),
            });
        }
    }

    // 3. Check constraint sorts.
    if !protocol.constraint_sorts.is_empty() {
        for (vertex_id, constraints) in &schema.constraints {
            for constraint in constraints {
                if !protocol
                    .constraint_sorts
                    .iter()
                    .any(|s| s == constraint.sort.as_ref())
                {
                    errors.push(ValidationError::InvalidConstraintSort {
                        vertex: vertex_id.to_string(),
                        sort: constraint.sort.to_string(),
                    });
                }
            }
        }
    }

    // 4. Check required edges reference existing vertices.
    for (vertex_id, required_edges) in &schema.required {
        for req_edge in required_edges {
            if !schema.vertices.contains_key(&req_edge.src)
                || !schema.vertices.contains_key(&req_edge.tgt)
            {
                errors.push(ValidationError::DanglingRequiredEdge {
                    vertex: vertex_id.to_string(),
                    edge: format!("{} -> {} ({})", req_edge.src, req_edge.tgt, req_edge.kind),
                });
            }
        }
    }

    errors
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::builder::SchemaBuilder;
    use crate::protocol::{EdgeRule, Protocol};

    fn atproto_protocol() -> Protocol {
        Protocol {
            name: "atproto".to_owned(),
            schema_theory: "ThATProtoSchema".to_owned(),
            instance_theory: "ThWType".to_owned(),
            edge_rules: vec![
                EdgeRule {
                    edge_kind: "record-schema".to_owned(),
                    src_kinds: vec!["record".to_owned()],
                    tgt_kinds: vec!["object".to_owned()],
                },
                EdgeRule {
                    edge_kind: "prop".to_owned(),
                    src_kinds: vec!["object".to_owned()],
                    tgt_kinds: vec![
                        "string".to_owned(),
                        "integer".to_owned(),
                        "object".to_owned(),
                        "boolean".to_owned(),
                    ],
                },
            ],
            obj_kinds: vec![
                "record".to_owned(),
                "object".to_owned(),
                "string".to_owned(),
                "integer".to_owned(),
                "boolean".to_owned(),
            ],
            constraint_sorts: vec![
                "maxLength".to_owned(),
                "minLength".to_owned(),
                "format".to_owned(),
            ],
            ..Protocol::default()
        }
    }

    #[test]
    fn valid_schema_passes() {
        let proto = atproto_protocol();
        let schema = SchemaBuilder::new(&proto)
            .vertex("post", "record", Some("app.bsky.feed.post"))
            .expect("vertex")
            .vertex("post:body", "object", None)
            .expect("vertex")
            .vertex("post:body.text", "string", None)
            .expect("vertex")
            .edge("post", "post:body", "record-schema", None)
            .expect("edge")
            .edge("post:body", "post:body.text", "prop", Some("text"))
            .expect("edge")
            .constraint("post:body.text", "maxLength", "3000")
            .build()
            .expect("build");

        let errors = validate(&schema, &proto);
        assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
    }

    #[test]
    fn invalid_constraint_sort_detected() {
        let proto = atproto_protocol();
        let schema = SchemaBuilder::new(&proto)
            .vertex("v", "string", None)
            .expect("vertex")
            .constraint("v", "nonexistent_sort", "value")
            .build()
            .expect("build");

        let errors = validate(&schema, &proto);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ValidationError::InvalidConstraintSort { .. })),
            "expected InvalidConstraintSort, got: {errors:?}"
        );
    }
}
