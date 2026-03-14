//! ELAN/Praat time-aligned annotation protocol definition.
//!
//! Covers both the ELAN Annotation Format (EAF XML) and the Praat TextGrid
//! format. ELAN uses `ANNOTATION_DOCUMENT` as the root, with `TIER` children
//! containing either `ALIGNABLE_ANNOTATION` (references two `TIME_SLOT`s via
//! `TIME_SLOT_REF1`/`TIME_SLOT_REF2`) or `REF_ANNOTATION` (references a
//! parent annotation). Praat TextGrid uses a root `text-grid` with
//! `interval-tier` or `point-tier` children holding `interval` or `point`
//! leaves respectively.
//!
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

/// Returns the ELAN/Praat protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "elan".into(),
        schema_theory: "ThElanSchema".into(),
        instance_theory: "ThElanInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            // ELAN (EAF) kinds
            "annotation-document".into(),
            "tier".into(),
            "alignable-annotation".into(),
            "ref-annotation".into(),
            "time-slot".into(),
            "linguistic-type".into(),
            "controlled-vocabulary".into(),
            "cv-entry".into(),
            "locale".into(),
            // Praat TextGrid kinds
            "text-grid".into(),
            "interval-tier".into(),
            "point-tier".into(),
            "interval".into(),
            "point".into(),
            // Scalar kinds shared by both
            "string".into(),
            "integer".into(),
            "float".into(),
        ],
        constraint_sorts: vec![
            // ELAN constraint sorts
            "time-value".into(),
            "annotation-value".into(),
            "tier-id".into(),
            "participant".into(),
            "annotator".into(),
            "linguistic-type-id".into(),
            "stereotype".into(),
            // Praat constraint sorts
            "xmin".into(),
            "xmax".into(),
            "text".into(),
            "mark".into(),
        ],
        has_order: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for ELAN.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_multigraph_wtype_meta(registry, "ThElanSchema", "ThElanInstance");
}

/// Parse a JSON-based ELAN schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_elan(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
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
            .unwrap_or("annotation-document");
        builder = builder.vertex(name, kind, None)?;

        // Constraints from top-level attributes.
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

        // Nested fields as child vertices with prop edges.
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

        // Typed references (time-ref, type-ref, cv-ref, parent-ref).
        if let Some(refs) = def.get("refs").and_then(serde_json::Value::as_array) {
            for (i, r) in refs.iter().enumerate() {
                if let (Some(edge_kind), Some(target)) = (
                    r.get("edge").and_then(|v| v.as_str()),
                    r.get("target").and_then(|v| v.as_str()),
                ) {
                    let ref_id = format!("{name}:ref{i}");
                    let ref_kind = r
                        .get("kind")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("string");
                    builder = builder.vertex(&ref_id, ref_kind, None)?;
                    builder = builder.edge(name, &ref_id, edge_kind, Some(target))?;
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

/// Emit a [`Schema`] as a JSON ELAN schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_elan(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &["contains", "time-ref", "type-ref", "cv-ref", "parent-ref"];
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
                constraints.insert(c.sort.clone(), serde_json::json!(c.value));
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
                        fcs.insert(c.sort.clone(), serde_json::json!(c.value));
                    }
                    field.insert("constraints".into(), serde_json::Value::Object(fcs));
                }
                fields.insert(name.to_string(), serde_json::Value::Object(field));
            }
            obj.insert("fields".into(), serde_json::Value::Object(fields));
        }

        // Emit ref edges.
        let ref_kinds = ["time-ref", "type-ref", "cv-ref", "parent-ref"];
        let mut refs = Vec::new();
        for rk in &ref_kinds {
            let ref_children = children_by_edge(schema, &root.id, rk);
            for (edge, child) in &ref_children {
                let target = edge.name.as_deref().unwrap_or(&child.id);
                refs.push(serde_json::json!({
                    "edge": rk,
                    "target": target,
                    "kind": child.kind,
                }));
            }
        }
        if !refs.is_empty() {
            obj.insert("refs".into(), serde_json::Value::Array(refs));
        }

        types.insert(root.id.clone(), serde_json::Value::Object(obj));
    }

    Ok(serde_json::json!({ "types": types }))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        // ELAN structural containment: document → tiers/time-slots/vocabularies,
        // tier → annotations, vocabulary → entries.
        EdgeRule {
            edge_kind: "contains".into(),
            src_kinds: vec![
                "annotation-document".into(),
                "tier".into(),
                "controlled-vocabulary".into(),
                // Praat TextGrid containment: grid → tiers, tiers → leaves.
                "text-grid".into(),
                "interval-tier".into(),
                "point-tier".into(),
            ],
            tgt_kinds: vec![],
        },
        // Only alignable-annotation has direct time-slot references; ref-annotation
        // inherits timing through its parent chain.
        EdgeRule {
            edge_kind: "time-ref".into(),
            src_kinds: vec!["alignable-annotation".into()],
            tgt_kinds: vec!["time-slot".into()],
        },
        EdgeRule {
            edge_kind: "type-ref".into(),
            src_kinds: vec!["tier".into()],
            tgt_kinds: vec!["linguistic-type".into()],
        },
        EdgeRule {
            edge_kind: "cv-ref".into(),
            src_kinds: vec!["tier".into()],
            tgt_kinds: vec!["controlled-vocabulary".into()],
        },
        // ref-annotation references another annotation as its parent; the parent
        // can be either an alignable-annotation or another ref-annotation.
        EdgeRule {
            edge_kind: "parent-ref".into(),
            src_kinds: vec!["ref-annotation".into()],
            tgt_kinds: vec!["alignable-annotation".into(), "ref-annotation".into()],
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
        assert_eq!(p.name, "elan");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThElanSchema"));
        assert!(registry.contains_key("ThElanInstance"));
    }

    /// ELAN: abstract `annotation` kind is not in obj_kinds or edge rules.
    #[test]
    fn abstract_annotation_kind_absent() {
        let p = protocol();
        assert!(
            !p.obj_kinds.iter().any(|k| k == "annotation"),
            "abstract 'annotation' kind must not appear in obj_kinds"
        );
        for rule in &p.edge_rules {
            assert!(
                !rule.src_kinds.iter().any(|k| k == "annotation"),
                "abstract 'annotation' kind must not appear in src_kinds of '{}'",
                rule.edge_kind
            );
            assert!(
                !rule.tgt_kinds.iter().any(|k| k == "annotation"),
                "abstract 'annotation' kind must not appear in tgt_kinds of '{}'",
                rule.edge_kind
            );
        }
    }

    /// Only `alignable-annotation` may be the source of a `time-ref` edge.
    #[test]
    fn time_ref_src_only_alignable() {
        let p = protocol();
        let rule = p
            .find_edge_rule("time-ref")
            .expect("time-ref rule must exist");
        assert_eq!(
            rule.src_kinds,
            vec!["alignable-annotation".to_string()],
            "time-ref src_kinds must be exactly [alignable-annotation]"
        );
    }

    /// `parent-ref` target must be concrete annotation kinds only.
    #[test]
    fn parent_ref_tgt_concrete() {
        let p = protocol();
        let rule = p
            .find_edge_rule("parent-ref")
            .expect("parent-ref rule must exist");
        assert!(
            rule.tgt_kinds.contains(&"alignable-annotation".to_string()),
            "parent-ref tgt_kinds must include alignable-annotation"
        );
        assert!(
            rule.tgt_kinds.contains(&"ref-annotation".to_string()),
            "parent-ref tgt_kinds must include ref-annotation"
        );
        assert!(
            !rule.tgt_kinds.iter().any(|k| k == "annotation"),
            "parent-ref tgt_kinds must not include abstract annotation"
        );
    }

    /// Praat vertex kinds must all be present as recognized kinds.
    #[test]
    fn praat_kinds_present() {
        let p = protocol();
        for kind in &[
            "text-grid",
            "interval-tier",
            "point-tier",
            "interval",
            "point",
        ] {
            assert!(
                p.is_known_vertex_kind(kind),
                "Praat kind '{kind}' must be recognized"
            );
        }
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "types": {
                "doc": {
                    "kind": "annotation-document",
                    "fields": {
                        "main-tier": {
                            "type": "tier",
                            "constraints": {
                                "tier-id": "default"
                            }
                        }
                    }
                },
                "ts1": {
                    "kind": "time-slot",
                    "constraints": {
                        "time-value": "0"
                    }
                },
                "ts2": {
                    "kind": "time-slot",
                    "constraints": {
                        "time-value": "1000"
                    }
                },
                "ann1": {
                    "kind": "alignable-annotation",
                    "constraints": {
                        "annotation-value": "hello"
                    },
                    "refs": [
                        {"edge": "time-ref", "target": "ts1", "kind": "time-slot"},
                        {"edge": "time-ref", "target": "ts2", "kind": "time-slot"}
                    ]
                }
            }
        });
        let schema = parse_elan(&json).expect("should parse");
        assert!(schema.has_vertex("doc"));
        assert!(schema.has_vertex("ts1"));
        assert!(schema.has_vertex("ann1"));
        let emitted = emit_elan(&schema).expect("emit");
        let s2 = parse_elan(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    /// Praat TextGrid round-trip: parse a minimal text-grid schema and re-emit.
    #[test]
    fn praat_text_grid_round_trip() {
        let json = serde_json::json!({
            "types": {
                "tg": {
                    "kind": "text-grid",
                    "constraints": {
                        "xmin": "0.0",
                        "xmax": "3.5"
                    }
                },
                "tier1": {
                    "kind": "interval-tier",
                    "constraints": {
                        "xmin": "0.0",
                        "xmax": "3.5"
                    }
                },
                "itvl1": {
                    "kind": "interval",
                    "constraints": {
                        "xmin": "0.0",
                        "xmax": "1.2",
                        "text": "hello"
                    }
                },
                "ptier1": {
                    "kind": "point-tier",
                    "constraints": {
                        "xmin": "0.0",
                        "xmax": "3.5"
                    }
                },
                "pt1": {
                    "kind": "point",
                    "constraints": {
                        "xmin": "2.1",
                        "mark": "boundary"
                    }
                }
            }
        });
        let schema = parse_elan(&json).expect("praat parse");
        assert!(schema.has_vertex("tg"));
        assert!(schema.has_vertex("tier1"));
        assert!(schema.has_vertex("itvl1"));
        assert!(schema.has_vertex("ptier1"));
        assert!(schema.has_vertex("pt1"));
        let emitted = emit_elan(&schema).expect("praat emit");
        let s2 = parse_elan(&emitted).expect("praat re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    /// ref-annotation may reference another ref-annotation as parent (chained refs).
    #[test]
    fn ref_annotation_parent_ref_chained() {
        let json = serde_json::json!({
            "types": {
                "ts1": { "kind": "time-slot", "constraints": { "time-value": "0" } },
                "ts2": { "kind": "time-slot", "constraints": { "time-value": "500" } },
                "ann_align": {
                    "kind": "alignable-annotation",
                    "constraints": { "annotation-value": "base" },
                    "refs": [
                        { "edge": "time-ref", "target": "ts1", "kind": "time-slot" },
                        { "edge": "time-ref", "target": "ts2", "kind": "time-slot" }
                    ]
                },
                "ann_ref1": {
                    "kind": "ref-annotation",
                    "constraints": { "annotation-value": "child" },
                    "refs": [
                        { "edge": "parent-ref", "target": "ann_align", "kind": "alignable-annotation" }
                    ]
                },
                "ann_ref2": {
                    "kind": "ref-annotation",
                    "constraints": { "annotation-value": "grandchild" },
                    "refs": [
                        { "edge": "parent-ref", "target": "ann_ref1", "kind": "ref-annotation" }
                    ]
                }
            }
        });
        let schema = parse_elan(&json).expect("chained ref parse");
        assert!(schema.has_vertex("ann_ref2"));
    }
}
