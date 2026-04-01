//! FoLiA (Format for Linguistic Annotation) protocol definition.
//!
//! FoLiA is a rich XML-based format for linguistic annotation supporting
//! multiple annotation layers, provenance, and typed annotation sets.
//! Uses Group E theory: constrained multigraph + W-type + metadata.
//! Schema `colimit(ThGraph, ThConstraint, ThMulti)`,
//! instance `colimit(ThWType, ThMeta)`.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the `FoLiA` protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "folia".into(),
        schema_theory: "ThFoliaSchema".into(),
        instance_theory: "ThFoliaInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            // Structural
            "folia-document".into(),
            "text".into(),
            "division".into(),
            "paragraph".into(),
            "sentence".into(),
            "word".into(),
            // Subtoken
            "morpheme".into(),
            "phoneme".into(),
            // Inline annotation
            "pos-annotation".into(),
            "lemma-annotation".into(),
            "sense-annotation".into(),
            "lang-annotation".into(),
            "domain-annotation".into(),
            // Span annotation layers
            "syntax-layer".into(),
            "chunk-layer".into(),
            "dep-layer".into(),
            "entity-layer".into(),
            "semrole-layer".into(),
            "coref-layer".into(),
            "sentiment-layer".into(),
            "timesegment-layer".into(),
            "statement-layer".into(),
            "observation-layer".into(),
            // Subtoken annotation layers
            "morphology-layer".into(),
            "phonology-layer".into(),
            // Span annotation elements
            "entity".into(),
            "chunk".into(),
            "syntaxunit".into(),
            "dependency".into(),
            "semrole".into(),
            "coreferencechain".into(),
            "sentiment".into(),
            "timesegment".into(),
            "statement".into(),
            "observation".into(),
            // Provenance
            "processor".into(),
            // Primitives
            "string".into(),
            "integer".into(),
        ],
        constraint_sorts: vec![
            "xml-id".into(),
            "class".into(),
            "set".into(),
            "annotator".into(),
            "annotator-type".into(),
            "confidence".into(),
            "datetime".into(),
            "n".into(),
            // FoLiA v2 provenance
            "processor".into(),
            // Dependency role
            "deprel".into(),
        ],
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for `FoLiA`.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_multigraph_wtype_meta(registry, "ThFoliaSchema", "ThFoliaInstance");
}

/// Parse a JSON-based `FoLiA` schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
#[allow(clippy::too_many_lines)]
pub fn parse_folia(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
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
            .unwrap_or("folia-document");
        builder = builder.vertex(name, kind, None)?;

        // Constraints (xml-id, class, set, annotator, processor, etc.).
        if let Some(constraints) = def
            .get("constraints")
            .and_then(serde_json::Value::as_object)
        {
            for (sort, val) in constraints {
                if let Some(v) = val.as_str() {
                    builder = builder.constraint(name, sort, v);
                }
            }
        }

        // Containment children (structural hierarchy).
        if let Some(fields) = def.get("fields").and_then(serde_json::Value::as_object) {
            for (field_name, field_def) in fields {
                let field_id = format!("{name}.{field_name}");
                let field_kind = field_def
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("string");
                builder = builder.vertex(&field_id, field_kind, None)?;
                builder = builder.edge(name, &field_id, "contains", Some(field_name))?;

                if let Some(fc) = field_def
                    .get("constraints")
                    .and_then(serde_json::Value::as_object)
                {
                    for (sort, val) in fc {
                        if let Some(v) = val.as_str() {
                            builder = builder.constraint(&field_id, sort, v);
                        }
                    }
                }
            }
        }

        // Annotation-of edges (inline annotation → word/morpheme).
        if let Some(ann_of) = def
            .get("annotation-of")
            .and_then(serde_json::Value::as_array)
        {
            for (i, target) in ann_of.iter().enumerate() {
                if let Some(tgt) = target.as_str() {
                    let ref_id = format!("{name}:ann{i}");
                    builder = builder.vertex(&ref_id, "string", None)?;
                    builder = builder.edge(name, &ref_id, "annotation-of", Some(tgt))?;
                }
            }
        }

        // Span edges (entity/chunk/semrole/etc. → word+).
        if let Some(spans) = def.get("spans").and_then(serde_json::Value::as_array) {
            for (i, target) in spans.iter().enumerate() {
                if let Some(tgt) = target.as_str() {
                    let ref_id = format!("{name}:span{i}");
                    builder = builder.vertex(&ref_id, "string", None)?;
                    builder = builder.edge(name, &ref_id, "span", Some(tgt))?;
                }
            }
        }

        // Dependency head edge: the syntactic head word (<hd> in FoLiA).
        if let Some(hd) = def.get("dep-head").and_then(serde_json::Value::as_str) {
            let ref_id = format!("{name}:hd");
            builder = builder.vertex(&ref_id, "string", None)?;
            builder = builder.edge(name, &ref_id, "dep-head", Some(hd))?;
        }

        // Dependency dependent edge: the dependent word (<dep> in FoLiA).
        if let Some(dep_targets) = def
            .get("dep-dependent")
            .and_then(serde_json::Value::as_array)
        {
            for (i, target) in dep_targets.iter().enumerate() {
                if let Some(tgt) = target.as_str() {
                    let ref_id = format!("{name}:dep{i}");
                    builder = builder.vertex(&ref_id, "string", None)?;
                    builder = builder.edge(name, &ref_id, "dep-dependent", Some(tgt))?;
                }
            }
        }

        // Items (contained children by kind).
        if let Some(items) = def.get("items").and_then(serde_json::Value::as_array) {
            for (i, item) in items.iter().enumerate() {
                if let Some(item_kind) = item.as_str() {
                    let item_id = format!("{name}:item{i}");
                    builder = builder.vertex(&item_id, item_kind, None)?;
                    builder = builder.edge(name, &item_id, "contains", Some(item_kind))?;
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON `FoLiA` schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
#[allow(clippy::too_many_lines)]
pub fn emit_folia(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &[
        "contains",
        "annotation-of",
        "span",
        "dep-head",
        "dep-dependent",
    ];
    let roots = find_roots(schema, structural);

    let mut types = serde_json::Map::new();
    for root in &roots {
        let mut obj = serde_json::Map::new();
        obj.insert("kind".into(), serde_json::json!(root.kind));

        // Emit constraints.
        let cs = vertex_constraints(schema, &root.id);
        if !cs.is_empty() {
            let mut constraints = serde_json::Map::new();
            for c in &cs {
                constraints.insert(c.sort.to_string(), serde_json::json!(c.value));
            }
            obj.insert("constraints".into(), serde_json::Value::Object(constraints));
        }

        // Emit contains-edge children as fields.
        let children = children_by_edge(schema, &root.id, "contains");
        if !children.is_empty() {
            let mut fields = serde_json::Map::new();
            for (edge, child) in &children {
                let name = edge.name.as_deref().unwrap_or(&child.id);
                let mut field = serde_json::Map::new();
                field.insert("type".into(), serde_json::json!(child.kind));
                let fc = vertex_constraints(schema, &child.id);
                if !fc.is_empty() {
                    let mut fcs = serde_json::Map::new();
                    for c in &fc {
                        fcs.insert(c.sort.to_string(), serde_json::json!(c.value));
                    }
                    field.insert("constraints".into(), serde_json::Value::Object(fcs));
                }
                fields.insert(name.to_string(), serde_json::Value::Object(field));
            }
            obj.insert("fields".into(), serde_json::Value::Object(fields));
        }

        // Emit annotation-of edges.
        let ann_children = children_by_edge(schema, &root.id, "annotation-of");
        if !ann_children.is_empty() {
            let arr: Vec<serde_json::Value> = ann_children
                .iter()
                .filter_map(|(e, _)| e.name.as_deref().map(|n| serde_json::json!(n)))
                .collect();
            obj.insert("annotation-of".into(), serde_json::Value::Array(arr));
        }

        // Emit span edges.
        let span_children = children_by_edge(schema, &root.id, "span");
        if !span_children.is_empty() {
            let arr: Vec<serde_json::Value> = span_children
                .iter()
                .filter_map(|(e, _)| e.name.as_deref().map(|n| serde_json::json!(n)))
                .collect();
            obj.insert("spans".into(), serde_json::Value::Array(arr));
        }

        // Emit dep-head edge (FoLiA <hd> role, the syntactic head).
        let hd_children = children_by_edge(schema, &root.id, "dep-head");
        if let Some((e, _)) = hd_children.first() {
            if let Some(n) = e.name.as_deref() {
                obj.insert("dep-head".into(), serde_json::json!(n));
            }
        }

        // Emit dep-dependent edges (FoLiA <dep> role, the dependents).
        let dep_children = children_by_edge(schema, &root.id, "dep-dependent");
        if !dep_children.is_empty() {
            let arr: Vec<serde_json::Value> = dep_children
                .iter()
                .filter_map(|(e, _)| e.name.as_deref().map(|n| serde_json::json!(n)))
                .collect();
            obj.insert("dep-dependent".into(), serde_json::Value::Array(arr));
        }

        types.insert(root.id.to_string(), serde_json::Value::Object(obj));
    }

    Ok(serde_json::json!({ "types": types }))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "contains".into(),
            src_kinds: vec![
                "folia-document".into(),
                "text".into(),
                "division".into(),
                "paragraph".into(),
                "sentence".into(),
                // Annotation layers (span)
                "syntax-layer".into(),
                "chunk-layer".into(),
                "dep-layer".into(),
                "entity-layer".into(),
                "semrole-layer".into(),
                "coref-layer".into(),
                "sentiment-layer".into(),
                "timesegment-layer".into(),
                "statement-layer".into(),
                "observation-layer".into(),
                // Subtoken annotation layers
                "morphology-layer".into(),
                "phonology-layer".into(),
            ],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "annotation-of".into(),
            src_kinds: vec![
                "pos-annotation".into(),
                "lemma-annotation".into(),
                "sense-annotation".into(),
                "lang-annotation".into(),
                "domain-annotation".into(),
            ],
            tgt_kinds: vec![
                "word".into(),
                "morpheme".into(),
                "phoneme".into(),
                "string".into(),
            ],
        },
        EdgeRule {
            edge_kind: "span".into(),
            src_kinds: vec![
                "entity".into(),
                "chunk".into(),
                "syntaxunit".into(),
                "dependency".into(),
                "semrole".into(),
                "coreferencechain".into(),
                "sentiment".into(),
                "timesegment".into(),
                "statement".into(),
                "observation".into(),
            ],
            tgt_kinds: vec!["word".into(), "morpheme".into(), "string".into()],
        },
        // FoLiA dependency annotation: <dependency> contains both a <hd> (head)
        // and one or more <dep> (dependent) sub-elements, each pointing to words.
        EdgeRule {
            edge_kind: "dep-head".into(),
            src_kinds: vec!["dependency".into()],
            tgt_kinds: vec!["word".into(), "string".into()],
        },
        EdgeRule {
            edge_kind: "dep-dependent".into(),
            src_kinds: vec!["dependency".into()],
            tgt_kinds: vec!["word".into(), "string".into()],
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
        assert_eq!(p.name, "folia");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThFoliaSchema"));
        assert!(registry.contains_key("ThFoliaInstance"));
    }

    #[test]
    fn all_annotation_layers_present() {
        let p = protocol();
        let expected_layers = [
            "sentiment-layer",
            "semrole-layer",
            "coref-layer",
            "timesegment-layer",
            "statement-layer",
            "observation-layer",
            "phonology-layer",
            "morphology-layer",
        ];
        for layer in &expected_layers {
            assert!(
                p.obj_kinds.iter().any(|k| k == layer),
                "missing layer: {layer}"
            );
        }
    }

    #[test]
    fn annotation_kinds_present() {
        let p = protocol();
        let expected = [
            "lang-annotation",
            "domain-annotation",
            "phoneme",
            "processor",
        ];
        for kind in &expected {
            assert!(
                p.obj_kinds.iter().any(|k| k == kind),
                "missing obj_kind: {kind}"
            );
        }
    }

    #[test]
    fn processor_constraint_sort_present() {
        let p = protocol();
        assert!(
            p.constraint_sorts.iter().any(|s| s == "processor"),
            "missing constraint sort: processor"
        );
    }

    #[test]
    fn dep_edge_rules_distinct() {
        let rules = edge_rules();
        assert!(rules.iter().any(|r| r.edge_kind == "dep-head"));
        assert!(rules.iter().any(|r| r.edge_kind == "dep-dependent"));
        // The old monolithic "dep" rule must not exist.
        assert!(!rules.iter().any(|r| r.edge_kind == "dep"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "types": {
                "doc": {
                    "kind": "folia-document",
                    "constraints": {
                        "xml-id": "doc1"
                    },
                    "fields": {
                        "text": {"type": "text"}
                    }
                },
                "s1": {
                    "kind": "sentence",
                    "constraints": {
                        "xml-id": "s.1"
                    },
                    "items": ["word"]
                },
                "w1": {
                    "kind": "word",
                    "constraints": {
                        "xml-id": "w.1",
                        "class": "WORD"
                    }
                },
                "pos1": {
                    "kind": "pos-annotation",
                    "constraints": {
                        "class": "N",
                        "set": "cgn"
                    },
                    "annotation-of": ["w1"]
                }
            }
        });
        let schema = parse_folia(&json).expect("should parse");
        assert!(schema.has_vertex("doc"));
        assert!(schema.has_vertex("s1"));
        assert!(schema.has_vertex("w1"));
        assert!(schema.has_vertex("pos1"));
        let emitted = emit_folia(&schema).expect("emit");
        let s2 = parse_folia(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn parse_dep_with_head_and_dependent() {
        let json = serde_json::json!({
            "types": {
                "w1": {
                    "kind": "word",
                    "constraints": {"xml-id": "w.1"}
                },
                "w2": {
                    "kind": "word",
                    "constraints": {"xml-id": "w.2"}
                },
                "dep1": {
                    "kind": "dependency",
                    "constraints": {
                        "class": "su",
                        "set": "ud",
                        "processor": "proc1"
                    },
                    "dep-head": "w1",
                    "dep-dependent": ["w2"]
                }
            }
        });
        let schema = parse_folia(&json).expect("dep parse");
        assert!(schema.has_vertex("w1"));
        assert!(schema.has_vertex("w2"));
        assert!(schema.has_vertex("dep1"));

        // Verify that dep-head and dep-dependent edges exist.
        let out = schema.outgoing_edges("dep1");
        assert!(
            out.iter().any(|e| e.kind == "dep-head"),
            "dep-head edge missing"
        );
        assert!(
            out.iter().any(|e| e.kind == "dep-dependent"),
            "dep-dependent edge missing"
        );

        // Round-trip.
        let emitted = emit_folia(&schema).expect("emit dep");
        let s2 = parse_folia(&emitted).expect("re-parse dep");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn parse_sentiment_and_semrole() {
        let json = serde_json::json!({
            "types": {
                "w1": {"kind": "word", "constraints": {"xml-id": "w.1"}},
                "sent1": {
                    "kind": "sentiment",
                    "constraints": {
                        "class": "positive",
                        "set": "sentiment-set"
                    },
                    "spans": ["w1"]
                },
                "sr1": {
                    "kind": "semrole",
                    "constraints": {
                        "class": "ARG0",
                        "set": "pb"
                    },
                    "spans": ["w1"]
                }
            }
        });
        let schema = parse_folia(&json).expect("sentiment+semrole parse");
        assert!(schema.has_vertex("sent1"));
        assert!(schema.has_vertex("sr1"));
        let emitted = emit_folia(&schema).expect("emit");
        let s2 = parse_folia(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }
}
