//! UIMA/CAS (Unstructured Information Management Architecture) protocol definition.
//!
//! Uses Group D theory: typed graph + W-type.
//!
//! Models UIMA type systems with full CAS type hierarchy. Vertex kinds:
//!
//! - `type-system`: root container for a type system descriptor
//! - `type-description`: a named CAS type (e.g. `uima.tcas.Annotation`)
//! - `feature-description`: a named feature on a type
//! - `cas`: a CAS instance (holds sofas, indices, and feature structures)
//! - `sofa`: Subject of Analysis — the artifact being annotated
//! - `fs-index`: a feature-structure index (for efficient FS retrieval)
//! - `annotation-base`: base annotation with sofa reference but no span
//! - `annotation`: span annotation with `begin`/`end` offsets
//! - `document-annotation`: whole-document annotation (one per sofa)
//! - `string`: string-typed feature value
//! - `integer`: integer-typed feature value
//! - `float`: float-typed feature value
//! - `boolean`: boolean-typed feature value
//! - `array`: array collection (StringArray, IntegerArray, FSArray, …)
//! - `fs-ref`: feature-structure reference (typed pointer)
//! - `string-list`: linked-list of strings (`StringList` / `EmptyStringList`)
//! - `integer-list`: linked-list of integers (`IntegerList` / `EmptyIntegerList`)
//!
//! Edge kinds:
//!
//! - `extends`: type inheritance (child → parent)
//! - `feature`: type owns a feature (type → feature-description)
//! - `sofa-of`: sofa contains annotation-bases (sofa → annotation-base)
//! - `index-of`: fs-index indexes a type (fs-index → type-description)
//! - `contains`: cas holds sofas and fs-indices (cas → sofa | fs-index)
//!
//! Constraint sorts: `name`, `range-type`, `element-type`, `multi-ref`,
//! `sofa-num`, `mime-type`, `begin`, `end`.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the UIMA/CAS protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "uima-cas".into(),
        schema_theory: "ThUimaSchema".into(),
        instance_theory: "ThUimaInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "type-system".into(),
            "type-description".into(),
            "feature-description".into(),
            "cas".into(),
            "sofa".into(),
            "fs-index".into(),
            "annotation-base".into(),
            "annotation".into(),
            "document-annotation".into(),
            "string".into(),
            "integer".into(),
            "float".into(),
            "boolean".into(),
            "array".into(),
            "fs-ref".into(),
            "string-list".into(),
            "integer-list".into(),
        ],
        constraint_sorts: vec![
            "name".into(),
            "range-type".into(),
            "element-type".into(),
            "multi-ref".into(),
            "sofa-num".into(),
            "mime-type".into(),
            "begin".into(),
            "end".into(),
        ],
    }
}

/// Register the component GATs for UIMA/CAS.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_typed_graph_wtype(registry, "ThUimaSchema", "ThUimaInstance");
}

/// Parse a JSON-based UIMA/CAS schema into a [`Schema`].
///
/// Expected JSON shape:
///
/// ```json
/// {
///   "types": {
///     "MyType": {
///       "kind": "type-description",
///       "name": "com.example.MyType",
///       "extends": "Annotation",
///       "features": {
///         "pos": { "type": "string", "range-type": "uima.cas.String" }
///       }
///     },
///     "MySofa": {
///       "kind": "sofa",
///       "sofa-num": "1",
///       "mime-type": "text/plain",
///       "sofa-of": "MyAnnotation"
///     },
///     "MyCas": {
///       "kind": "cas",
///       "contains": ["MySofa", "MyIndex"]
///     },
///     "MyIndex": {
///       "kind": "fs-index",
///       "index-of": "MyType"
///     }
///   }
/// }
/// ```
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_uima_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let types = json
        .get("types")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ProtocolError::MissingField("types".into()))?;

    // First pass: create all vertices so edges can reference them.
    for (name, def) in types {
        let kind = def
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("type-description");
        builder = builder.vertex(name, kind, None)?;

        // Constraints valid on most vertex kinds.
        if let Some(c_name) = def.get("name").and_then(serde_json::Value::as_str) {
            builder = builder.constraint(name, "name", c_name);
        }
        if let Some(rt) = def.get("range-type").and_then(serde_json::Value::as_str) {
            builder = builder.constraint(name, "range-type", rt);
        }
        if let Some(et) = def.get("element-type").and_then(serde_json::Value::as_str) {
            builder = builder.constraint(name, "element-type", et);
        }
        if let Some(mr) = def.get("multi-ref").and_then(serde_json::Value::as_str) {
            builder = builder.constraint(name, "multi-ref", mr);
        }
        if let Some(sn) = def.get("sofa-num").and_then(serde_json::Value::as_str) {
            builder = builder.constraint(name, "sofa-num", sn);
        }
        if let Some(mt) = def.get("mime-type").and_then(serde_json::Value::as_str) {
            builder = builder.constraint(name, "mime-type", mt);
        }
        // Span offsets: valid on annotation and document-annotation vertices.
        if let Some(begin) = def.get("begin").and_then(serde_json::Value::as_str) {
            builder = builder.constraint(name, "begin", begin);
        }
        if let Some(end) = def.get("end").and_then(serde_json::Value::as_str) {
            builder = builder.constraint(name, "end", end);
        }

        // Feature sub-vertices are created here so they exist for the edge pass.
        if let Some(features) = def.get("features").and_then(serde_json::Value::as_object) {
            for (feat_name, feat_def) in features {
                let feat_id = format!("{name}.{feat_name}");
                let feat_kind = feat_def
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("feature-description");
                builder = builder.vertex(&feat_id, feat_kind, None)?;

                if let Some(rt) =
                    feat_def.get("range-type").and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(&feat_id, "range-type", rt);
                }
            }
        }
    }

    // Second pass: create edges now that all vertices exist.
    for (name, def) in types {
        // Type inheritance: child → parent.
        if let Some(parent) = def.get("extends").and_then(serde_json::Value::as_str) {
            if types.contains_key(parent) {
                builder = builder.edge(name, parent, "extends", None)?;
            }
        }

        // Feature edges: type → feature-description.
        if let Some(features) = def.get("features").and_then(serde_json::Value::as_object) {
            for (feat_name, _) in features {
                let feat_id = format!("{name}.{feat_name}");
                builder = builder.edge(name, &feat_id, "feature", Some(feat_name))?;
            }
        }

        // sofa-of: sofa → annotation-base (sofa contains its annotations).
        // The "sofa-of" key is on the *sofa* vertex and names the annotation-base it covers.
        if let Some(ann) = def.get("sofa-of").and_then(serde_json::Value::as_str) {
            if types.contains_key(ann) {
                builder = builder.edge(name, ann, "sofa-of", None)?;
            }
        }

        // index-of: fs-index → type-description.
        if let Some(idx_type) = def.get("index-of").and_then(serde_json::Value::as_str) {
            if types.contains_key(idx_type) {
                builder = builder.edge(name, idx_type, "index-of", None)?;
            }
        }

        // contains: cas → sofa | fs-index (CAS owns its sofas and indices).
        if let Some(children) = def.get("contains").and_then(serde_json::Value::as_array) {
            for child_val in children {
                if let Some(child_id) = child_val.as_str() {
                    if types.contains_key(child_id) {
                        builder = builder.edge(name, child_id, "contains", None)?;
                    }
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON UIMA/CAS schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_uima_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    // Only feature sub-vertices are structurally nested inside their parent's
    // "features" block; all other edge kinds ("extends", "sofa-of", "index-of",
    // "contains") are references between top-level entries, so they must not
    // suppress roots.
    let roots = find_roots(schema, &["feature"]);

    let mut types = serde_json::Map::new();
    for root in &roots {
        let mut obj = serde_json::Map::new();
        obj.insert("kind".into(), serde_json::json!(root.kind));

        // Emit constraints.
        for c in vertex_constraints(schema, &root.id) {
            obj.insert(c.sort.clone(), serde_json::json!(c.value));
        }

        // Emit extends edges (type → parent).
        let extends = children_by_edge(schema, &root.id, "extends");
        if let Some((_, parent)) = extends.first() {
            obj.insert("extends".into(), serde_json::json!(parent.id));
        }

        // Emit features (nested under the type).
        let features = children_by_edge(schema, &root.id, "feature");
        if !features.is_empty() {
            let mut feats = serde_json::Map::new();
            for (edge, child) in &features {
                let feat_name = edge.name.as_deref().unwrap_or(&child.id);
                let mut feat = serde_json::Map::new();
                feat.insert("type".into(), serde_json::json!(child.kind));
                for c in vertex_constraints(schema, &child.id) {
                    feat.insert(c.sort.clone(), serde_json::json!(c.value));
                }
                feats.insert(feat_name.to_string(), serde_json::Value::Object(feat));
            }
            obj.insert("features".into(), serde_json::Value::Object(feats));
        }

        // Emit sofa-of edges (sofa → annotation-base).
        let sofa_of = children_by_edge(schema, &root.id, "sofa-of");
        if let Some((_, ann)) = sofa_of.first() {
            obj.insert("sofa-of".into(), serde_json::json!(ann.id));
        }

        // Emit index-of edges (fs-index → type-description).
        let index_of = children_by_edge(schema, &root.id, "index-of");
        if let Some((_, idx_type)) = index_of.first() {
            obj.insert("index-of".into(), serde_json::json!(idx_type.id));
        }

        // Emit contains edges (cas → sofa | fs-index).
        let contained = children_by_edge(schema, &root.id, "contains");
        if !contained.is_empty() {
            let ids: Vec<serde_json::Value> = contained
                .iter()
                .map(|(_, v)| serde_json::json!(v.id))
                .collect();
            obj.insert("contains".into(), serde_json::Value::Array(ids));
        }

        types.insert(root.id.clone(), serde_json::Value::Object(obj));
    }

    Ok(serde_json::json!({ "types": types }))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        // Type inheritance: any type-description may extend another.
        EdgeRule {
            edge_kind: "extends".into(),
            src_kinds: vec!["type-description".into()],
            tgt_kinds: vec!["type-description".into()],
        },
        // Feature ownership: a type-description owns feature-descriptions
        // (or typed primitives/composites as feature nodes).
        EdgeRule {
            edge_kind: "feature".into(),
            src_kinds: vec!["type-description".into()],
            tgt_kinds: vec![
                "feature-description".into(),
                "string".into(),
                "integer".into(),
                "float".into(),
                "boolean".into(),
                "array".into(),
                "fs-ref".into(),
                "string-list".into(),
                "integer-list".into(),
            ],
        },
        // sofa-of: a sofa covers annotation-base, annotation, or document-annotation vertices.
        // Direction: sofa → annotation-base (the sofa is the substrate for annotations).
        EdgeRule {
            edge_kind: "sofa-of".into(),
            src_kinds: vec!["sofa".into()],
            tgt_kinds: vec![
                "annotation-base".into(),
                "annotation".into(),
                "document-annotation".into(),
            ],
        },
        // index-of: an fs-index indexes a type-description.
        EdgeRule {
            edge_kind: "index-of".into(),
            src_kinds: vec!["fs-index".into()],
            tgt_kinds: vec!["type-description".into()],
        },
        // contains: a CAS instance holds sofas and fs-indices.
        EdgeRule {
            edge_kind: "contains".into(),
            src_kinds: vec!["cas".into()],
            tgt_kinds: vec!["sofa".into(), "fs-index".into()],
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
        assert_eq!(p.name, "uima-cas");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThUimaSchema"));
        assert!(registry.contains_key("ThUimaInstance"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "types": {
                "Annotation": {
                    "kind": "type-description",
                    "name": "uima.tcas.Annotation",
                    "features": {
                        "begin": {"type": "integer", "range-type": "uima.cas.Integer"},
                        "end": {"type": "integer", "range-type": "uima.cas.Integer"}
                    }
                },
                "Token": {
                    "kind": "type-description",
                    "extends": "Annotation",
                    "features": {
                        "pos": {"type": "string"}
                    }
                }
            }
        });
        let schema = parse_uima_schema(&json).expect("should parse");
        assert!(schema.has_vertex("Annotation"));
        assert!(schema.has_vertex("Token"));
        assert!(schema.has_vertex("Annotation.begin"));
        let emitted = emit_uima_schema(&schema).expect("emit");
        let s2 = parse_uima_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn protocol_has_annotation_base_kind() {
        let p = protocol();
        assert!(
            p.obj_kinds.iter().any(|k| k == "annotation-base"),
            "protocol must include 'annotation-base' vertex kind"
        );
    }

    #[test]
    fn protocol_has_document_annotation_kind() {
        let p = protocol();
        assert!(
            p.obj_kinds.iter().any(|k| k == "document-annotation"),
            "protocol must include 'document-annotation' vertex kind"
        );
    }

    #[test]
    fn protocol_has_string_list_and_integer_list_kinds() {
        let p = protocol();
        assert!(
            p.obj_kinds.iter().any(|k| k == "string-list"),
            "protocol must include 'string-list' vertex kind"
        );
        assert!(
            p.obj_kinds.iter().any(|k| k == "integer-list"),
            "protocol must include 'integer-list' vertex kind"
        );
    }

    #[test]
    fn protocol_has_begin_end_constraint_sorts() {
        let p = protocol();
        assert!(
            p.constraint_sorts.iter().any(|s| s == "begin"),
            "protocol must include 'begin' constraint sort"
        );
        assert!(
            p.constraint_sorts.iter().any(|s| s == "end"),
            "protocol must include 'end' constraint sort"
        );
    }

    #[test]
    fn begin_end_constraints_roundtrip() {
        let json = serde_json::json!({
            "types": {
                "Span": {
                    "kind": "annotation",
                    "begin": "0",
                    "end": "10"
                }
            }
        });
        let schema = parse_uima_schema(&json).expect("should parse span with begin/end");
        let constraints: Vec<_> = schema
            .constraints
            .get("Span")
            .map(|cs| cs.iter().collect())
            .unwrap_or_default();
        let has_begin = constraints.iter().any(|c| c.sort == "begin" && c.value == "0");
        let has_end = constraints.iter().any(|c| c.sort == "end" && c.value == "10");
        assert!(has_begin, "Span must have a 'begin' constraint with value '0'");
        assert!(has_end, "Span must have an 'end' constraint with value '10'");

        let emitted = emit_uima_schema(&schema).expect("emit");
        assert_eq!(emitted["types"]["Span"]["begin"].as_str().unwrap(), "0");
        assert_eq!(emitted["types"]["Span"]["end"].as_str().unwrap(), "10");
    }

    #[test]
    fn sofa_of_edge_direction_is_sofa_to_annotation() {
        // "sofa-of" is on the *sofa* vertex and points to the annotation-base.
        let json = serde_json::json!({
            "types": {
                "MySofa": {
                    "kind": "sofa",
                    "sofa-num": "1",
                    "sofa-of": "MyAnnotation"
                },
                "MyAnnotation": {
                    "kind": "annotation-base"
                }
            }
        });
        let schema = parse_uima_schema(&json).expect("should parse sofa-of");
        // Edge must go FROM MySofa TO MyAnnotation.
        let has_edge = schema
            .outgoing_edges("MySofa")
            .iter()
            .any(|e| e.kind == "sofa-of" && e.tgt == "MyAnnotation");
        assert!(has_edge, "sofa-of edge must go from sofa to annotation-base");
    }

    #[test]
    fn cas_connected_via_contains_edges() {
        // A CAS vertex must be reachable via contains edges to its sofas and indices.
        let json = serde_json::json!({
            "types": {
                "MyCas": {
                    "kind": "cas",
                    "contains": ["MySofa", "MyIndex"]
                },
                "MySofa": {
                    "kind": "sofa",
                    "sofa-num": "1"
                },
                "MyIndex": {
                    "kind": "fs-index"
                }
            }
        });
        let schema = parse_uima_schema(&json).expect("should parse cas with contains");
        let contains_sofa = schema
            .outgoing_edges("MyCas")
            .iter()
            .any(|e| e.kind == "contains" && e.tgt == "MySofa");
        let contains_index = schema
            .outgoing_edges("MyCas")
            .iter()
            .any(|e| e.kind == "contains" && e.tgt == "MyIndex");
        assert!(contains_sofa, "cas must have contains edge to sofa");
        assert!(contains_index, "cas must have contains edge to fs-index");

        // Emit and re-parse: cas must remain connected.
        let emitted = emit_uima_schema(&schema).expect("emit");
        let s2 = parse_uima_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn index_of_edge_on_fs_index_node() {
        // index-of must originate from the fs-index vertex, not from the indexed type.
        let json = serde_json::json!({
            "types": {
                "TokenIndex": {
                    "kind": "fs-index",
                    "index-of": "Token"
                },
                "Token": {
                    "kind": "type-description"
                }
            }
        });
        let schema = parse_uima_schema(&json).expect("should parse index-of");
        let has_edge = schema
            .outgoing_edges("TokenIndex")
            .iter()
            .any(|e| e.kind == "index-of" && e.tgt == "Token");
        assert!(has_edge, "index-of edge must originate from fs-index");
        // Token must NOT have an outgoing index-of edge.
        let token_has_edge = schema
            .outgoing_edges("Token")
            .iter()
            .any(|e| e.kind == "index-of");
        assert!(!token_has_edge, "type-description must not have outgoing index-of edges");
    }

    #[test]
    fn string_list_and_integer_list_as_feature_targets() {
        // string-list and integer-list may be used as feature target kinds.
        let json = serde_json::json!({
            "types": {
                "Doc": {
                    "kind": "type-description",
                    "features": {
                        "words": {"type": "string-list"},
                        "counts": {"type": "integer-list"}
                    }
                }
            }
        });
        let schema = parse_uima_schema(&json).expect("should parse list features");
        assert!(schema.has_vertex("Doc.words"));
        assert_eq!(schema.vertices.get("Doc.words").unwrap().kind, "string-list");
        assert!(schema.has_vertex("Doc.counts"));
        assert_eq!(schema.vertices.get("Doc.counts").unwrap().kind, "integer-list");
    }

    #[test]
    fn document_annotation_as_sofa_of_target() {
        // A sofa-of edge may target a document-annotation.
        let json = serde_json::json!({
            "types": {
                "DocSofa": {
                    "kind": "sofa",
                    "sofa-of": "DocAnn"
                },
                "DocAnn": {
                    "kind": "document-annotation",
                    "begin": "0",
                    "end": "100"
                }
            }
        });
        let schema = parse_uima_schema(&json).expect("should parse document-annotation");
        let has_edge = schema
            .outgoing_edges("DocSofa")
            .iter()
            .any(|e| e.kind == "sofa-of" && e.tgt == "DocAnn");
        assert!(has_edge, "sofa-of edge to document-annotation must exist");
    }
}
