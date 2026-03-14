//! NIF (NLP Interchange Format) protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the NIF protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "nif".into(),
        schema_theory: "ThNifSchema".into(),
        instance_theory: "ThNifInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "context".into(),
            "string-context".into(),
            "word".into(),
            "sentence".into(),
            "phrase".into(),
            "structure".into(),
            "offset-based-string".into(),
            "title".into(),
            "paragraph".into(),
            "section".into(),
            "uri-scheme".into(),
            "string".into(),
            "integer".into(),
        ],
        constraint_sorts: vec![
            "begin-index".into(),
            "end-index".into(),
            "anchor-of".into(),
            "is-string".into(),
            "pred-lang".into(),
            "pos-tag".into(),
            "lemma".into(),
            "stem".into(),
            "sentiment-value".into(),
            "confidence".into(),
            "ta-ident-ref".into(),
            "ta-class-ref".into(),
        ],
        has_order: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for NIF.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(registry, "ThNifSchema", "ThNifInstance");
}

/// Parse a JSON-based NIF schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
#[allow(clippy::too_many_lines)]
pub fn parse_nif_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
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
            .unwrap_or("context");
        builder = builder.vertex(name, kind, None)?;

        if let Some(fields) = def.get("fields").and_then(serde_json::Value::as_object) {
            for (field_name, field_def) in fields {
                let field_id = format!("{name}.{field_name}");
                let field_kind = field_def
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("string");
                builder = builder.vertex(&field_id, field_kind, None)?;
                builder = builder.edge(name, &field_id, "sub-string", Some(field_name))?;

                // nif:OffsetBasedString / nif:String index constraints
                if let Some(begin) = field_def
                    .get("begin-index")
                    .and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(&field_id, "begin-index", begin);
                }
                if let Some(end) = field_def
                    .get("end-index")
                    .and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(&field_id, "end-index", end);
                }
                if let Some(anchor) = field_def
                    .get("anchor-of")
                    .and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(&field_id, "anchor-of", anchor);
                }
                if let Some(v) = field_def
                    .get("is-string")
                    .and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(&field_id, "is-string", v);
                }
                if let Some(v) = field_def
                    .get("pred-lang")
                    .and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(&field_id, "pred-lang", v);
                }

                // Lexical annotation constraints (nif:posTag, nif:lemma, nif:stem)
                if let Some(v) = field_def.get("pos-tag").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(&field_id, "pos-tag", v);
                }
                if let Some(v) = field_def.get("lemma").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(&field_id, "lemma", v);
                }
                if let Some(v) = field_def.get("stem").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(&field_id, "stem", v);
                }

                // Sentiment and named-entity annotation (nif:sentimentValue,
                // itsrdf:taConfidence, itsrdf:taIdentRef, itsrdf:taClassRef)
                if let Some(v) = field_def
                    .get("sentiment-value")
                    .and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(&field_id, "sentiment-value", v);
                }
                if let Some(v) = field_def
                    .get("confidence")
                    .and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(&field_id, "confidence", v);
                }
                if let Some(v) = field_def
                    .get("ta-ident-ref")
                    .and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(&field_id, "ta-ident-ref", v);
                }
                if let Some(v) = field_def
                    .get("ta-class-ref")
                    .and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(&field_id, "ta-class-ref", v);
                }
            }
        }

        if let Some(items) = def.get("items").and_then(serde_json::Value::as_array) {
            for (i, item) in items.iter().enumerate() {
                if let Some(item_kind) = item.as_str() {
                    let item_id = format!("{name}:item{i}");
                    builder = builder.vertex(&item_id, item_kind, None)?;
                    builder = builder.edge(name, &item_id, "sub-string", Some(item_kind))?;
                }
            }
        }

        // reference-context edges: each string links back to its nif:Context.
        if let Some(refs) = def
            .get("reference-context")
            .and_then(serde_json::Value::as_array)
        {
            for ref_val in refs {
                if let Some(ctx_id) = ref_val.as_str() {
                    builder = builder.edge(name, ctx_id, "reference-context", None)?;
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON NIF schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_nif_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    // Only "sub-string" is a true structural (containment) edge in the NIF
    // JSON representation.  "reference-context" points *up* to a context, so
    // it must not be used to exclude vertices from the root set.
    let structural = &["sub-string"];
    let roots = find_roots(schema, structural);

    let mut types = serde_json::Map::new();
    for root in &roots {
        let mut obj = serde_json::Map::new();
        obj.insert("kind".into(), serde_json::json!(root.kind));

        let props = children_by_edge(schema, &root.id, "sub-string");
        if !props.is_empty() {
            let mut fields = serde_json::Map::new();
            let mut item_arr: Vec<serde_json::Value> = Vec::new();

            for (edge, child) in &props {
                let edge_name = edge.name.as_deref().unwrap_or(&child.id);
                // If the edge name equals the child kind it was recorded as an
                // item (repeated by kind); otherwise it is a named field.
                if edge_name == child.kind {
                    item_arr.push(serde_json::json!(edge_name));
                } else {
                    let mut field = serde_json::Map::new();
                    field.insert("type".into(), serde_json::json!(child.kind));
                    for c in vertex_constraints(schema, &child.id) {
                        field.insert(c.sort.clone(), serde_json::json!(c.value));
                    }
                    fields.insert(edge_name.to_string(), serde_json::Value::Object(field));
                }
            }

            if !fields.is_empty() {
                obj.insert("fields".into(), serde_json::Value::Object(fields));
            }
            if !item_arr.is_empty() {
                obj.insert("items".into(), serde_json::Value::Array(item_arr));
            }
        }

        // Emit reference-context edges as an array of target IDs.
        let ref_ctx_edges = children_by_edge(schema, &root.id, "reference-context");
        if !ref_ctx_edges.is_empty() {
            let refs: Vec<serde_json::Value> = ref_ctx_edges
                .iter()
                .map(|(_, tgt)| serde_json::json!(tgt.id))
                .collect();
            obj.insert("reference-context".into(), serde_json::Value::Array(refs));
        }

        types.insert(root.id.clone(), serde_json::Value::Object(obj));
    }

    Ok(serde_json::json!({ "types": types }))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "sub-string".into(),
            src_kinds: vec![
                "context".into(),
                "string-context".into(),
                "sentence".into(),
                "paragraph".into(),
                "section".into(),
                "title".into(),
            ],
            tgt_kinds: vec![
                "context".into(),
                "string-context".into(),
                "word".into(),
                "sentence".into(),
                "phrase".into(),
                "structure".into(),
                "offset-based-string".into(),
                "title".into(),
                "paragraph".into(),
                "section".into(),
                "string".into(),
                "integer".into(),
            ],
        },
        EdgeRule {
            edge_kind: "reference-context".into(),
            src_kinds: vec![
                "word".into(),
                "sentence".into(),
                "phrase".into(),
                "structure".into(),
                "offset-based-string".into(),
                "title".into(),
                "paragraph".into(),
                "section".into(),
                "string-context".into(),
            ],
            tgt_kinds: vec!["context".into()],
        },
        EdgeRule {
            edge_kind: "word-of".into(),
            src_kinds: vec!["word".into()],
            tgt_kinds: vec!["sentence".into()],
        },
        EdgeRule {
            edge_kind: "annotation".into(),
            src_kinds: vec!["context".into()],
            tgt_kinds: vec!["context".into()],
        },
        EdgeRule {
            edge_kind: "source-url".into(),
            src_kinds: vec!["context".into()],
            tgt_kinds: vec!["uri-scheme".into()],
        },
        EdgeRule {
            edge_kind: "next-word".into(),
            src_kinds: vec!["word".into()],
            tgt_kinds: vec!["word".into()],
        },
        EdgeRule {
            edge_kind: "prev-sentence".into(),
            src_kinds: vec!["sentence".into()],
            tgt_kinds: vec!["sentence".into()],
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
        assert_eq!(p.name, "nif");
        // All expected constraint sorts present.
        for sort in &[
            "begin-index",
            "end-index",
            "anchor-of",
            "is-string",
            "pred-lang",
            "pos-tag",
            "lemma",
            "stem",
            "sentiment-value",
            "confidence",
            "ta-ident-ref",
            "ta-class-ref",
        ] {
            assert!(
                p.constraint_sorts.contains(&(*sort).to_string()),
                "missing constraint sort: {sort}"
            );
        }
        // reference-context must be an edge kind, not a constraint sort.
        assert!(
            !p.constraint_sorts
                .contains(&"reference-context".to_string()),
            "reference-context must not be a constraint sort"
        );
        assert!(p.find_edge_rule("reference-context").is_some());
        // section vertex kind present.
        assert!(p.obj_kinds.contains(&"section".to_string()));
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThNifSchema"));
        assert!(registry.contains_key("ThNifInstance"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "types": {
                "ctx1": {
                    "kind": "context",
                    "fields": {
                        "word1": {"type": "word", "begin-index": "0", "end-index": "5"}
                    },
                    "items": ["sentence"]
                }
            }
        });
        let schema = parse_nif_schema(&json).expect("should parse");
        assert!(schema.has_vertex("ctx1"));
        let emitted = emit_nif_schema(&schema).expect("emit");
        let s2 = parse_nif_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn parse_and_emit_lexical_constraints() {
        let json = serde_json::json!({
            "types": {
                "ctx2": {
                    "kind": "context",
                    "fields": {
                        "w1": {
                            "type": "word",
                            "begin-index": "0",
                            "end-index": "3",
                            "anchor-of": "run",
                            "pos-tag": "VBZ",
                            "lemma": "run",
                            "stem": "run",
                            "sentiment-value": "0.5",
                            "confidence": "0.9",
                            "ta-ident-ref": "http://dbpedia.org/resource/Run",
                            "ta-class-ref": "http://nerd.eurecom.fr/ontology#Person"
                        }
                    }
                }
            }
        });
        let schema = parse_nif_schema(&json).expect("should parse");
        assert!(schema.has_vertex("ctx2.w1"));
        // All lexical constraints round-trip through emit.
        let emitted = emit_nif_schema(&schema).expect("emit");
        let s2 = parse_nif_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
        // Verify at least one constraint survived.
        let types = emitted["types"].as_object().unwrap();
        let w1_field = &types["ctx2"]["fields"]["w1"];
        assert_eq!(w1_field["pos-tag"], "VBZ");
        assert_eq!(w1_field["lemma"], "run");
        assert_eq!(w1_field["stem"], "run");
        assert_eq!(w1_field["sentiment-value"], "0.5");
        assert_eq!(w1_field["confidence"], "0.9");
        assert_eq!(w1_field["ta-ident-ref"], "http://dbpedia.org/resource/Run");
        assert_eq!(
            w1_field["ta-class-ref"],
            "http://nerd.eurecom.fr/ontology#Person"
        );
    }

    #[test]
    fn parse_and_emit_reference_context() {
        // A sentence string declared as a top-level type pointing back to its
        // nif:Context via a reference-context edge.  The context is a separate
        // top-level entry so neither vertex is created twice.
        let json = serde_json::json!({
            "types": {
                "ctx3": {
                    "kind": "context"
                },
                "sent3": {
                    "kind": "sentence",
                    "reference-context": ["ctx3"]
                }
            }
        });
        let schema = parse_nif_schema(&json).expect("should parse");
        assert!(schema.has_vertex("ctx3"));
        assert!(schema.has_vertex("sent3"));
        // reference-context edge must exist from sent3 -> ctx3.
        let edges = schema.edges_between("sent3", "ctx3");
        assert!(
            edges.iter().any(|e| e.kind == "reference-context"),
            "expected reference-context edge from sent3 to ctx3"
        );
        // Emit and re-parse round-trip.
        let emitted = emit_nif_schema(&schema).expect("emit");
        let s2 = parse_nif_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn section_vertex_kind() {
        let json = serde_json::json!({
            "types": {
                "doc": {
                    "kind": "context",
                    "items": ["section"]
                }
            }
        });
        let schema = parse_nif_schema(&json).expect("should parse");
        // item vertex should be of kind "section".
        let section_vertex = schema.vertices.values().find(|v| v.kind == "section");
        assert!(section_vertex.is_some(), "expected a section vertex");
    }
}
