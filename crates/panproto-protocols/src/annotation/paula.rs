//! PAULA/Salt/ANNIS multi-layer corpus annotation protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.
//!
//! # Salt node and relation types modelled
//!
//! Node (vertex) kinds:
//! - `corpus`          — SCorpus: top-level corpus container
//! - `document`        — SDocument: individual document inside a corpus
//! - `text`            — STextualDS: primary text data source
//! - `token`           — SToken: basic tokenisation unit
//! - `span`            — SSpan: ordered set of tokens sharing annotations
//! - `struct-node`     — SStructure: hierarchical node (constituency, RST)
//! - `timeline`        — STimeline: virtual timeline for multi-layer alignment
//! - `media`           — SMedialDS: audio/video data source
//! - `annotation`      — SAnnotation: key-value label on any node or edge
//! - `meta-annotation` — SMetaAnnotation: corpus/document-level metadata
//! - `annotation-layer`— grouping layer for annotations (ANNIS layers)
//!
//! Edge (relation) kinds:
//! - `textual-relation`  — STextualRelation: token → text with start/end offsets
//! - `spanning-relation` — SSpanningRelation (spans edge): span → token
//! - `dominance`         — SDominanceRelation: struct-node → token | struct-node
//! - `points-to`         — SPointingRelation: anaphora, coref, discourse arcs
//! - `order`             — SOrderRelation: sequential ordering of tokens/spans
//! - `timeline-relation` — STimelineRelation: token/span → timeline
//! - `medial-relation`   — SMedialRelation: token/span → media data source
//! - `layer-of`          — layer → annotation membership
//! - `annotates`         — annotation → annotated node
//! - `prop` / `items`    — JSON schema structural edges

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the PAULA protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "paula".into(),
        schema_theory: "ThPaulaSchema".into(),
        instance_theory: "ThPaulaInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            // Salt corpus structure
            "corpus".into(),
            "document".into(),
            // Salt data sources
            "text".into(),
            "timeline".into(),
            "media".into(),
            // Salt annotation nodes
            "token".into(),
            "span".into(),
            "struct-node".into(),
            // Salt annotation kinds
            "annotation".into(),
            "meta-annotation".into(),
            "annotation-layer".into(),
            // Scalar value kinds
            "string".into(),
            "integer".into(),
        ],
        constraint_sorts: vec![
            // Annotation identity
            "name".into(),
            "value".into(),
            "namespace".into(),
            "type".into(),
            "layer".into(),
            // Corpus/document references
            "document-id".into(),
            "corpus-id".into(),
            // Character-offset anchoring (STextualRelation)
            "start".into(),
            "end".into(),
            // Temporal anchoring (STimelineRelation / SMedialRelation)
            "begin-time".into(),
            "end-time".into(),
        ],
        has_order: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for PAULA.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(registry, "ThPaulaSchema", "ThPaulaInstance");
}

/// Parse a JSON-based PAULA schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_paula_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
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
            .unwrap_or("corpus");
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

                if let Some(ns) = field_def
                    .get("namespace")
                    .and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(&field_id, "namespace", ns);
                }
                if let Some(layer) = field_def.get("layer").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(&field_id, "layer", layer);
                }
                // Character-offset constraints (STextualRelation)
                if let Some(start) = field_def.get("start").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(&field_id, "start", start);
                }
                if let Some(end) = field_def.get("end").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(&field_id, "end", end);
                }
                // Temporal constraints (STimelineRelation / SMedialRelation)
                if let Some(bt) = field_def
                    .get("begin-time")
                    .and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(&field_id, "begin-time", bt);
                }
                if let Some(et) = field_def
                    .get("end-time")
                    .and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(&field_id, "end-time", et);
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

/// Emit a [`Schema`] as a JSON PAULA schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_paula_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
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
                    match c.sort.as_str() {
                        "namespace" | "layer" | "start" | "end" | "begin-time" | "end-time" => {
                            field.insert(c.sort.clone(), serde_json::json!(c.value));
                        }
                        _ => {}
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
    // All annotation node kinds that can carry SAnnotation or be part of layers.
    let annotatable: Vec<String> = vec![
        "token".into(),
        "span".into(),
        "struct-node".into(),
        "annotation-layer".into(),
        "annotation".into(),
        "meta-annotation".into(),
    ];

    // All object kinds valid as src/tgt of the generic prop/items edges.
    let all_obj: Vec<String> = vec![
        "corpus".into(),
        "document".into(),
        "text".into(),
        "timeline".into(),
        "media".into(),
        "token".into(),
        "span".into(),
        "struct-node".into(),
        "annotation".into(),
        "meta-annotation".into(),
        "annotation-layer".into(),
    ];

    vec![
        // ── Structural JSON-schema edges ──────────────────────────────────────
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: all_obj.clone(),
            tgt_kinds: vec!["string".into(), "integer".into()],
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: all_obj.clone(),
            tgt_kinds: {
                let mut t = all_obj;
                t.push("string".into());
                t.push("integer".into());
                t
            },
        },
        // ── Salt textual anchoring ────────────────────────────────────────────
        // STextualRelation: connects a token to its STextualDS with character
        // offsets (start/end carried as constraint_sorts on the edge vertex).
        EdgeRule {
            edge_kind: "textual-relation".into(),
            src_kinds: vec!["token".into()],
            tgt_kinds: vec!["text".into()],
        },
        // ── Salt spanning relation ────────────────────────────────────────────
        // SSpanningRelation: span → token (replaces the old "spans" edge name).
        EdgeRule {
            edge_kind: "spans".into(),
            src_kinds: vec!["span".into()],
            tgt_kinds: vec!["token".into()],
        },
        // ── Salt dominance relation ───────────────────────────────────────────
        // SDominanceRelation: struct-node → token | struct-node.
        EdgeRule {
            edge_kind: "dominates".into(),
            src_kinds: vec!["struct-node".into()],
            tgt_kinds: vec!["token".into(), "struct-node".into()],
        },
        // ── Salt pointing relation ────────────────────────────────────────────
        // SPointingRelation: any of token, span, struct-node → any of the same.
        // Used for anaphora, coreference, RST relations, etc.
        EdgeRule {
            edge_kind: "points-to".into(),
            src_kinds: vec!["token".into(), "span".into(), "struct-node".into()],
            tgt_kinds: vec!["token".into(), "span".into(), "struct-node".into()],
        },
        // ── Salt order relation ───────────────────────────────────────────────
        // SOrderRelation: sequential ordering between tokens or spans.
        EdgeRule {
            edge_kind: "order".into(),
            src_kinds: vec!["token".into(), "span".into()],
            tgt_kinds: vec!["token".into(), "span".into()],
        },
        // ── Salt timeline relation ────────────────────────────────────────────
        // STimelineRelation: anchors tokens/spans to an STimeline position
        // (begin-time / end-time constraint_sorts).
        EdgeRule {
            edge_kind: "timeline-relation".into(),
            src_kinds: vec!["token".into(), "span".into()],
            tgt_kinds: vec!["timeline".into()],
        },
        // ── Salt medial relation ──────────────────────────────────────────────
        // SMedialRelation: anchors tokens/spans to an SMedialDS segment.
        EdgeRule {
            edge_kind: "medial-relation".into(),
            src_kinds: vec!["token".into(), "span".into()],
            tgt_kinds: vec!["media".into()],
        },
        // ── ANNIS layer membership ────────────────────────────────────────────
        EdgeRule {
            edge_kind: "layer-of".into(),
            src_kinds: vec!["annotation-layer".into()],
            tgt_kinds: annotatable,
        },
        // ── Salt annotation attachment ────────────────────────────────────────
        // SAnnotation / SMetaAnnotation can annotate tokens, spans, struct-nodes,
        // edges (represented here as their Salt proxy node kinds), and documents.
        EdgeRule {
            edge_kind: "annotates".into(),
            src_kinds: vec!["annotation".into(), "meta-annotation".into()],
            tgt_kinds: vec![
                "token".into(),
                "span".into(),
                "struct-node".into(),
                "document".into(),
                "corpus".into(),
            ],
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
        assert_eq!(p.name, "paula");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThPaulaSchema"));
        assert!(registry.contains_key("ThPaulaInstance"));
    }

    /// All Salt vertex kinds must be present.
    #[test]
    fn salt_vertex_kinds_present() {
        let p = protocol();
        for kind in &[
            "corpus",
            "document",
            "text",
            "timeline",
            "media",
            "token",
            "span",
            "struct-node",
            "annotation",
            "meta-annotation",
            "annotation-layer",
        ] {
            assert!(
                p.is_known_vertex_kind(kind),
                "Salt vertex kind '{kind}' must be recognized"
            );
        }
    }

    /// All Salt constraint sorts must be declared.
    #[test]
    fn salt_constraint_sorts_present() {
        let p = protocol();
        for sort in &[
            "name",
            "value",
            "namespace",
            "type",
            "layer",
            "document-id",
            "corpus-id",
            "start",
            "end",
            "begin-time",
            "end-time",
        ] {
            assert!(
                p.constraint_sorts.iter().any(|s| s == sort),
                "constraint sort '{sort}' must be declared"
            );
        }
    }

    /// textual-relation: only token→text.
    #[test]
    fn textual_relation_token_to_text() {
        let p = protocol();
        let rule = p
            .find_edge_rule("textual-relation")
            .expect("textual-relation rule must exist");
        assert_eq!(rule.src_kinds, vec!["token".to_string()]);
        assert_eq!(rule.tgt_kinds, vec!["text".to_string()]);
    }

    /// timeline-relation: token/span → timeline.
    #[test]
    fn timeline_relation_to_timeline() {
        let p = protocol();
        let rule = p
            .find_edge_rule("timeline-relation")
            .expect("timeline-relation rule must exist");
        assert!(rule.src_kinds.contains(&"token".to_string()));
        assert!(rule.src_kinds.contains(&"span".to_string()));
        assert_eq!(rule.tgt_kinds, vec!["timeline".to_string()]);
    }

    /// medial-relation: token/span → media.
    #[test]
    fn medial_relation_to_media() {
        let p = protocol();
        let rule = p
            .find_edge_rule("medial-relation")
            .expect("medial-relation rule must exist");
        assert!(rule.src_kinds.contains(&"token".to_string()));
        assert!(rule.src_kinds.contains(&"span".to_string()));
        assert_eq!(rule.tgt_kinds, vec!["media".to_string()]);
    }

    /// points-to must include struct-node in tgt_kinds (SPointingRelation spec).
    #[test]
    fn points_to_includes_struct_node() {
        let p = protocol();
        let rule = p
            .find_edge_rule("points-to")
            .expect("points-to rule must exist");
        assert!(
            rule.tgt_kinds.contains(&"struct-node".to_string()),
            "points-to tgt_kinds must include struct-node"
        );
        assert!(
            rule.src_kinds.contains(&"struct-node".to_string()),
            "points-to src_kinds must include struct-node"
        );
    }

    /// order edge must accept span src/tgt (virtual tokenisation via STimeline).
    #[test]
    fn order_relation_includes_span() {
        let p = protocol();
        let rule = p.find_edge_rule("order").expect("order rule must exist");
        assert!(rule.src_kinds.contains(&"span".to_string()));
        assert!(rule.tgt_kinds.contains(&"span".to_string()));
    }

    /// meta-annotation can annotate corpus and document (SMetaAnnotation spec).
    #[test]
    fn meta_annotation_annotates_corpus_and_document() {
        let p = protocol();
        let rule = p
            .find_edge_rule("annotates")
            .expect("annotates rule must exist");
        assert!(rule.src_kinds.contains(&"meta-annotation".to_string()));
        assert!(rule.tgt_kinds.contains(&"corpus".to_string()));
        assert!(rule.tgt_kinds.contains(&"document".to_string()));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "types": {
                "corpus": {
                    "kind": "corpus",
                    "fields": {
                        "name": {"type": "string", "namespace": "paula"}
                    },
                    "items": ["document"]
                }
            }
        });
        let schema = parse_paula_schema(&json).expect("should parse");
        assert!(schema.has_vertex("corpus"));
        let emitted = emit_paula_schema(&schema).expect("emit");
        let s2 = parse_paula_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    /// Round-trip a schema that uses character-offset constraint sorts.
    #[test]
    fn parse_and_emit_with_offsets() {
        let json = serde_json::json!({
            "types": {
                "tok1": {
                    "kind": "token",
                    "fields": {
                        "anchor": {"type": "string", "start": "0", "end": "5"}
                    }
                }
            }
        });
        let schema = parse_paula_schema(&json).expect("offset parse");
        assert!(schema.has_vertex("tok1"));
        let emitted = emit_paula_schema(&schema).expect("offset emit");
        let s2 = parse_paula_schema(&emitted).expect("offset re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    /// Round-trip a schema that uses temporal constraint sorts (STimeline).
    #[test]
    fn parse_and_emit_with_temporal() {
        let json = serde_json::json!({
            "types": {
                "span1": {
                    "kind": "span",
                    "fields": {
                        "segment": {
                            "type": "string",
                            "begin-time": "1.5",
                            "end-time": "2.0"
                        }
                    }
                }
            }
        });
        let schema = parse_paula_schema(&json).expect("temporal parse");
        assert!(schema.has_vertex("span1"));
        let emitted = emit_paula_schema(&schema).expect("temporal emit");
        let s2 = parse_paula_schema(&emitted).expect("temporal re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }
}
