//! NAF (NLP Annotation Format) protocol definition.
//!
//! NAF is a layered standoff format for NLP pipelines, representing
//! linguistic annotations across multiple layers (text, terms, deps,
//! chunks, entities, coreferences, SRL, opinions, time expressions,
//! factualities, constituency). Uses Group B theory: hypergraph + functor.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the NAF protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "naf".into(),
        schema_theory: "ThNafSchema".into(),
        instance_theory: "ThNafInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "naf-document".into(),
            "raw-text".into(),
            "text-layer".into(),
            "terms-layer".into(),
            "deps-layer".into(),
            "chunks-layer".into(),
            "entities-layer".into(),
            "coreferences-layer".into(),
            "srl-layer".into(),
            "opinion-layer".into(),
            "temporal-layer".into(),
            "factuality-layer".into(),
            "constituency-layer".into(),
            "word-form".into(),
            "term".into(),
            "dep".into(),
            "chunk".into(),
            "entity".into(),
            "coref".into(),
            "predicate".into(),
            "role".into(),
            "opinion".into(),
            "timex3".into(),
            "factuality".into(),
            "non-terminal".into(),
            "terminal".into(),
            "span".into(),
            "external-ref".into(),
            "string".into(),
            "integer".into(),
        ],
        constraint_sorts: vec![
            "id".into(),
            "type".into(),
            "lemma".into(),
            "pos".into(),
            "morphofeat".into(),
            "case".into(),
            "head".into(),
            "sentiment".into(),
            "resource".into(),
            "reference".into(),
            "confidence".into(),
            "offset".into(),
            "length".into(),
            "sent".into(),
            "polarity".into(),
            "value".into(),
            "uri".into(),
            "prediction".into(),
        ],
        has_order: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for NAF.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_hypergraph_functor(registry, "ThNafSchema", "ThNafInstance");
}

/// Parse a JSON-based NAF schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
#[allow(clippy::too_many_lines)]
pub fn parse_naf(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    // Parse document.
    if let Some(doc) = json.get("document").and_then(serde_json::Value::as_object) {
        let doc_id = doc
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("doc");
        builder = builder.vertex(doc_id, "naf-document", None)?;

        if let Some(id_val) = doc.get("id").and_then(serde_json::Value::as_str) {
            builder = builder.constraint(doc_id, "id", id_val);
        }
    }

    // Parse layers: two-pass approach.
    //
    // Pass 1: register all layer vertices, element vertices, and external-ref
    // vertices along with their constraints, plus the "contains" edges within
    // the document→layer→element hierarchy. These edges only reference vertices
    // that are registered in the same pass, so order does not matter.
    //
    // Pass 2: register cross-element reference edges (span-ref, head, dep-ref,
    // coref-ref, pred-ref, role-ref, ext-ref). By deferring these until all
    // element vertices exist, forward references across layers (e.g. a dep
    // referencing a term in another layer) are resolved without error.

    if let Some(layers) = json.get("layers").and_then(serde_json::Value::as_object) {
        // --- pass 1: vertices + containment edges ---
        for (layer_id, layer_def) in layers {
            let kind = layer_def
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("text-layer");
            builder = builder.vertex(layer_id, kind, None)?;

            // Connect layer to document.
            if let Some(doc) = json.get("document").and_then(serde_json::Value::as_object) {
                let doc_id = doc
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("doc");
                builder = builder.edge(doc_id, layer_id, "contains", Some(kind))?;
            }

            if let Some(elements) = layer_def
                .get("elements")
                .and_then(serde_json::Value::as_object)
            {
                for (elem_id, elem_def) in elements {
                    let elem_kind = elem_def
                        .get("kind")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("word-form");
                    builder = builder.vertex(elem_id, elem_kind, None)?;
                    builder = builder.edge(layer_id, elem_id, "contains", Some(elem_kind))?;

                    if let Some(attrs) =
                        elem_def.get("attrs").and_then(serde_json::Value::as_object)
                    {
                        for (sort, value) in attrs {
                            if let Some(v) = value.as_str() {
                                builder = builder.constraint(elem_id, sort, v);
                            }
                        }
                    }

                    // Register external-ref vertices inline so they exist in
                    // pass 2 when the ext-ref edges are added.
                    if let Some(ext_refs) = elem_def
                        .get("ext_refs")
                        .and_then(serde_json::Value::as_array)
                    {
                        for (i, ext) in ext_refs.iter().enumerate() {
                            if let Some(ext_obj) = ext.as_object() {
                                let ext_id = format!("{elem_id}:extref{i}");
                                builder = builder.vertex(&ext_id, "external-ref", None)?;
                                if let Some(resource) =
                                    ext_obj.get("resource").and_then(serde_json::Value::as_str)
                                {
                                    builder = builder.constraint(&ext_id, "resource", resource);
                                }
                                if let Some(reference) =
                                    ext_obj.get("reference").and_then(serde_json::Value::as_str)
                                {
                                    builder = builder.constraint(&ext_id, "reference", reference);
                                }
                                if let Some(confidence) = ext_obj
                                    .get("confidence")
                                    .and_then(serde_json::Value::as_str)
                                {
                                    builder = builder.constraint(&ext_id, "confidence", confidence);
                                }
                            }
                        }
                    }
                }
            }
        }

        // --- pass 2: cross-element reference edges ---
        for (_layer_id, layer_def) in layers {
            if let Some(elements) = layer_def
                .get("elements")
                .and_then(serde_json::Value::as_object)
            {
                for (elem_id, elem_def) in elements {
                    // span-ref (term/entity/chunk → word-form)
                    if let Some(spans) = elem_def
                        .get("span_refs")
                        .and_then(serde_json::Value::as_array)
                    {
                        for span_ref in spans {
                            if let Some(tgt) = span_ref.as_str() {
                                builder = builder.edge(elem_id, tgt, "span-ref", None)?;
                            }
                        }
                    }

                    // head (dep → term)
                    if let Some(head_ref) =
                        elem_def.get("head_ref").and_then(serde_json::Value::as_str)
                    {
                        builder = builder.edge(elem_id, head_ref, "head", None)?;
                    }

                    // dep-ref (dep → term)
                    if let Some(dep_ref) =
                        elem_def.get("dep_ref").and_then(serde_json::Value::as_str)
                    {
                        builder = builder.edge(elem_id, dep_ref, "dep-ref", None)?;
                    }

                    // coref-ref (coref → span)
                    if let Some(coref_refs) = elem_def
                        .get("coref_refs")
                        .and_then(serde_json::Value::as_array)
                    {
                        for cr in coref_refs {
                            if let Some(tgt) = cr.as_str() {
                                builder = builder.edge(elem_id, tgt, "coref-ref", None)?;
                            }
                        }
                    }

                    // pred-ref (predicate → span)
                    if let Some(pred_ref) =
                        elem_def.get("pred_ref").and_then(serde_json::Value::as_str)
                    {
                        builder = builder.edge(elem_id, pred_ref, "pred-ref", None)?;
                    }

                    // role-ref (role → span)
                    if let Some(role_ref) =
                        elem_def.get("role_ref").and_then(serde_json::Value::as_str)
                    {
                        builder = builder.edge(elem_id, role_ref, "role-ref", None)?;
                    }

                    // ext-ref (entity/term → external-ref)
                    if let Some(ext_refs) = elem_def
                        .get("ext_refs")
                        .and_then(serde_json::Value::as_array)
                    {
                        for (i, ext) in ext_refs.iter().enumerate() {
                            if ext.is_object() {
                                let ext_id = format!("{elem_id}:extref{i}");
                                builder = builder.edge(elem_id, &ext_id, "ext-ref", None)?;
                            }
                        }
                    }

                    // opinion-ref (opinion → term/span holder/target)
                    if let Some(opinion_refs) = elem_def
                        .get("opinion_refs")
                        .and_then(serde_json::Value::as_array)
                    {
                        for or_ref in opinion_refs {
                            if let Some(tgt) = or_ref.as_str() {
                                builder = builder.edge(elem_id, tgt, "opinion-ref", None)?;
                            }
                        }
                    }

                    // parent-child (non-terminal → non-terminal/terminal)
                    if let Some(children) = elem_def
                        .get("children")
                        .and_then(serde_json::Value::as_array)
                    {
                        for child_ref in children {
                            if let Some(tgt) = child_ref.as_str() {
                                builder = builder.edge(elem_id, tgt, "parent-child", None)?;
                            }
                        }
                    }
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON NAF representation.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
#[allow(clippy::too_many_lines)]
pub fn emit_naf(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &[
        "contains",
        "span-ref",
        "head",
        "dep-ref",
        "coref-ref",
        "pred-ref",
        "role-ref",
        "ext-ref",
        "opinion-ref",
        "parent-child",
    ];
    let roots = find_roots(schema, structural);

    let mut document = serde_json::Map::new();
    let mut layers = serde_json::Map::new();

    for root in &roots {
        if root.kind.as_str() == "naf-document" {
            let constraints = vertex_constraints(schema, &root.id);
            for c in &constraints {
                if c.sort == "id" {
                    document.insert("id".into(), serde_json::json!(c.value));
                }
            }

            // Find layers contained by this document, sorted in dependency
            // order so that base layers (text) are emitted before derived
            // layers (terms, deps) that reference their elements. This
            // ensures the parser can resolve all vertex references in a
            // single forward pass.
            let layer_order = |kind: &str| -> u8 {
                match kind {
                    "text-layer" => 0,
                    "terms-layer" => 1,
                    "chunks-layer" => 2,
                    "entities-layer" => 3,
                    "deps-layer" => 4,
                    "coreferences-layer" => 5,
                    "srl-layer" => 6,
                    "constituency-layer" => 7,
                    "opinion-layer" => 8,
                    "temporal-layer" => 9,
                    "factuality-layer" => 10,
                    _ => 11,
                }
            };
            let mut layer_children = children_by_edge(schema, &root.id, "contains");
            layer_children.sort_by_key(|(_, layer)| layer_order(layer.kind.as_str()));
            for (edge, layer) in &layer_children {
                let mut layer_obj = serde_json::Map::new();
                layer_obj.insert("kind".into(), serde_json::json!(layer.kind));

                let mut elements = serde_json::Map::new();
                let elem_children = children_by_edge(schema, &layer.id, "contains");
                for (_edge, elem) in &elem_children {
                    let mut elem_obj = serde_json::Map::new();
                    elem_obj.insert("kind".into(), serde_json::json!(elem.kind));

                    // Emit constraints as attrs.
                    let elem_constraints = vertex_constraints(schema, &elem.id);
                    if !elem_constraints.is_empty() {
                        let mut attrs = serde_json::Map::new();
                        for c in &elem_constraints {
                            attrs.insert(c.sort.to_string(), serde_json::json!(c.value));
                        }
                        elem_obj.insert("attrs".into(), serde_json::Value::Object(attrs));
                    }

                    // Emit span-ref edges.
                    let span_refs = children_by_edge(schema, &elem.id, "span-ref");
                    if !span_refs.is_empty() {
                        let arr: Vec<serde_json::Value> = span_refs
                            .iter()
                            .map(|(_, child)| serde_json::json!(child.id))
                            .collect();
                        elem_obj.insert("span_refs".into(), serde_json::Value::Array(arr));
                    }

                    // Emit head reference.
                    let head_refs = children_by_edge(schema, &elem.id, "head");
                    if let Some((_, child)) = head_refs.first() {
                        elem_obj.insert("head_ref".into(), serde_json::json!(child.id));
                    }

                    // Emit dep-ref.
                    let dep_refs = children_by_edge(schema, &elem.id, "dep-ref");
                    if let Some((_, child)) = dep_refs.first() {
                        elem_obj.insert("dep_ref".into(), serde_json::json!(child.id));
                    }

                    // Emit coref-ref edges.
                    let coref_refs = children_by_edge(schema, &elem.id, "coref-ref");
                    if !coref_refs.is_empty() {
                        let arr: Vec<serde_json::Value> = coref_refs
                            .iter()
                            .map(|(_, child)| serde_json::json!(child.id))
                            .collect();
                        elem_obj.insert("coref_refs".into(), serde_json::Value::Array(arr));
                    }

                    // Emit pred-ref.
                    let pred_refs = children_by_edge(schema, &elem.id, "pred-ref");
                    if let Some((_, child)) = pred_refs.first() {
                        elem_obj.insert("pred_ref".into(), serde_json::json!(child.id));
                    }

                    // Emit role-ref.
                    let role_refs = children_by_edge(schema, &elem.id, "role-ref");
                    if let Some((_, child)) = role_refs.first() {
                        elem_obj.insert("role_ref".into(), serde_json::json!(child.id));
                    }

                    // Emit ext-ref edges.
                    let ext_refs = children_by_edge(schema, &elem.id, "ext-ref");
                    if !ext_refs.is_empty() {
                        let arr: Vec<serde_json::Value> = ext_refs
                            .iter()
                            .map(|(_, child)| {
                                let mut ext_obj = serde_json::Map::new();
                                let ext_constraints = vertex_constraints(schema, &child.id);
                                for c in &ext_constraints {
                                    ext_obj.insert(c.sort.to_string(), serde_json::json!(c.value));
                                }
                                serde_json::Value::Object(ext_obj)
                            })
                            .collect();
                        elem_obj.insert("ext_refs".into(), serde_json::Value::Array(arr));
                    }

                    // Emit opinion-ref edges (opinion holder/target).
                    let opinion_refs = children_by_edge(schema, &elem.id, "opinion-ref");
                    if !opinion_refs.is_empty() {
                        let arr: Vec<serde_json::Value> = opinion_refs
                            .iter()
                            .map(|(_, child)| serde_json::json!(child.id))
                            .collect();
                        elem_obj.insert("opinion_refs".into(), serde_json::Value::Array(arr));
                    }

                    // Emit parent-child edges (constituency tree).
                    let children = children_by_edge(schema, &elem.id, "parent-child");
                    if !children.is_empty() {
                        let arr: Vec<serde_json::Value> = children
                            .iter()
                            .map(|(_, child)| serde_json::json!(child.id))
                            .collect();
                        elem_obj.insert("children".into(), serde_json::Value::Array(arr));
                    }

                    elements.insert(elem.id.to_string(), serde_json::Value::Object(elem_obj));
                }

                if !elements.is_empty() {
                    layer_obj.insert("elements".into(), serde_json::Value::Object(elements));
                }

                let _layer_name = edge.name.as_deref().unwrap_or(&layer.id);
                layers.insert(layer.id.to_string(), serde_json::Value::Object(layer_obj));
            }
        }
    }

    let mut result = serde_json::Map::new();
    if !document.is_empty() {
        result.insert("document".into(), serde_json::Value::Object(document));
    }
    if !layers.is_empty() {
        result.insert("layers".into(), serde_json::Value::Object(layers));
    }

    Ok(serde_json::Value::Object(result))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "contains".into(),
            src_kinds: vec![
                "naf-document".into(),
                "text-layer".into(),
                "terms-layer".into(),
                "deps-layer".into(),
                "chunks-layer".into(),
                "entities-layer".into(),
                "coreferences-layer".into(),
                "srl-layer".into(),
                "opinion-layer".into(),
                "temporal-layer".into(),
                "factuality-layer".into(),
                "constituency-layer".into(),
            ],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "span-ref".into(),
            src_kinds: vec![
                "term".into(),
                "entity".into(),
                "chunk".into(),
                "opinion".into(),
                "timex3".into(),
                "factuality".into(),
            ],
            tgt_kinds: vec!["word-form".into()],
        },
        EdgeRule {
            edge_kind: "head".into(),
            src_kinds: vec!["dep".into()],
            tgt_kinds: vec!["term".into()],
        },
        EdgeRule {
            edge_kind: "dep-ref".into(),
            src_kinds: vec!["dep".into()],
            tgt_kinds: vec!["term".into()],
        },
        EdgeRule {
            edge_kind: "coref-ref".into(),
            src_kinds: vec!["coref".into()],
            tgt_kinds: vec!["span".into()],
        },
        EdgeRule {
            edge_kind: "pred-ref".into(),
            src_kinds: vec!["predicate".into()],
            tgt_kinds: vec!["span".into()],
        },
        EdgeRule {
            edge_kind: "role-ref".into(),
            src_kinds: vec!["role".into()],
            tgt_kinds: vec!["span".into()],
        },
        EdgeRule {
            edge_kind: "ext-ref".into(),
            src_kinds: vec!["entity".into(), "term".into()],
            tgt_kinds: vec!["external-ref".into()],
        },
        EdgeRule {
            edge_kind: "opinion-ref".into(),
            src_kinds: vec!["opinion".into()],
            tgt_kinds: vec!["term".into(), "span".into()],
        },
        EdgeRule {
            edge_kind: "parent-child".into(),
            src_kinds: vec!["non-terminal".into()],
            tgt_kinds: vec!["non-terminal".into(), "terminal".into()],
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
        assert_eq!(p.name, "naf");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThNafSchema"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "document": {
                "id": "doc1"
            },
            "layers": {
                "text": {
                    "kind": "text-layer",
                    "elements": {
                        "w1": {
                            "kind": "word-form",
                            "attrs": {
                                "offset": "0",
                                "length": "5"
                            }
                        },
                        "w2": {
                            "kind": "word-form",
                            "attrs": {
                                "offset": "6",
                                "length": "4"
                            }
                        }
                    }
                },
                "terms": {
                    "kind": "terms-layer",
                    "elements": {
                        "t1": {
                            "kind": "term",
                            "attrs": {
                                "lemma": "hello",
                                "pos": "UH"
                            },
                            "span_refs": ["w1"],
                            "ext_refs": [
                                {"resource": "WordNet", "reference": "hello%1:10:00::"}
                            ]
                        }
                    }
                },
                "deps": {
                    "kind": "deps-layer",
                    "elements": {
                        "d1": {
                            "kind": "dep",
                            "attrs": {
                                "type": "ROOT"
                            },
                            "head_ref": "t1",
                            "dep_ref": "t1"
                        }
                    }
                }
            }
        });
        let schema = parse_naf(&json).expect("should parse");
        assert!(schema.has_vertex("doc1"));
        assert!(schema.has_vertex("w1"));
        assert!(schema.has_vertex("t1"));
        assert!(schema.has_vertex("d1"));
        let emitted = emit_naf(&schema).expect("emit");
        let s2 = parse_naf(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn opinion_layer_round_trip() {
        let json = serde_json::json!({
            "document": { "id": "doc2" },
            "layers": {
                "text": {
                    "kind": "text-layer",
                    "elements": {
                        "w1": { "kind": "word-form", "attrs": { "offset": "0", "length": "4" } }
                    }
                },
                "terms": {
                    "kind": "terms-layer",
                    "elements": {
                        "t1": { "kind": "term", "attrs": { "lemma": "good", "pos": "JJ" }, "span_refs": ["w1"] }
                    }
                },
                "opinions": {
                    "kind": "opinion-layer",
                    "elements": {
                        "op1": {
                            "kind": "opinion",
                            "attrs": { "polarity": "positive", "sent": "1" },
                            "opinion_refs": ["t1"]
                        }
                    }
                }
            }
        });
        let schema = parse_naf(&json).expect("should parse opinion layer");
        assert!(schema.has_vertex("op1"));
        let emitted = emit_naf(&schema).expect("emit");
        let s2 = parse_naf(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn temporal_layer_round_trip() {
        let json = serde_json::json!({
            "document": { "id": "doc3" },
            "layers": {
                "text": {
                    "kind": "text-layer",
                    "elements": {
                        "w1": { "kind": "word-form", "attrs": { "offset": "0", "length": "8" } }
                    }
                },
                "time": {
                    "kind": "temporal-layer",
                    "elements": {
                        "tmx1": {
                            "kind": "timex3",
                            "attrs": { "type": "DATE", "value": "2013-01-01" },
                            "span_refs": ["w1"]
                        }
                    }
                }
            }
        });
        let schema = parse_naf(&json).expect("should parse temporal layer");
        assert!(schema.has_vertex("tmx1"));
        let emitted = emit_naf(&schema).expect("emit");
        let s2 = parse_naf(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn factuality_layer_round_trip() {
        let json = serde_json::json!({
            "document": { "id": "doc4" },
            "layers": {
                "text": {
                    "kind": "text-layer",
                    "elements": {
                        "w1": { "kind": "word-form", "attrs": { "offset": "0", "length": "4" } }
                    }
                },
                "factualities": {
                    "kind": "factuality-layer",
                    "elements": {
                        "f1": {
                            "kind": "factuality",
                            "attrs": { "prediction": "CT+", "confidence": "0.9" },
                            "span_refs": ["w1"]
                        }
                    }
                }
            }
        });
        let schema = parse_naf(&json).expect("should parse factuality layer");
        assert!(schema.has_vertex("f1"));
        let emitted = emit_naf(&schema).expect("emit");
        let s2 = parse_naf(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn constituency_layer_round_trip() {
        let json = serde_json::json!({
            "document": { "id": "doc5" },
            "layers": {
                "text": {
                    "kind": "text-layer",
                    "elements": {
                        "w1": { "kind": "word-form", "attrs": { "offset": "0", "length": "3" } }
                    }
                },
                "terms": {
                    "kind": "terms-layer",
                    "elements": {
                        "t1": { "kind": "term", "attrs": { "lemma": "the", "pos": "DT" }, "span_refs": ["w1"] }
                    }
                },
                "constituency": {
                    "kind": "constituency-layer",
                    "elements": {
                        "nt1": {
                            "kind": "non-terminal",
                            "attrs": { "id": "nt1", "sent": "1" },
                            "children": ["term1"]
                        },
                        "term1": {
                            "kind": "terminal",
                            "attrs": { "id": "term1", "sent": "1" }
                        }
                    }
                }
            }
        });
        let schema = parse_naf(&json).expect("should parse constituency layer");
        assert!(schema.has_vertex("nt1"));
        assert!(schema.has_vertex("term1"));
        let emitted = emit_naf(&schema).expect("emit");
        let s2 = parse_naf(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn protocol_has_new_obj_kinds() {
        let p = protocol();
        let kinds = &p.obj_kinds;
        assert!(kinds.iter().any(|k| k == "opinion-layer"));
        assert!(kinds.iter().any(|k| k == "temporal-layer"));
        assert!(kinds.iter().any(|k| k == "factuality-layer"));
        assert!(kinds.iter().any(|k| k == "constituency-layer"));
        assert!(kinds.iter().any(|k| k == "opinion"));
        assert!(kinds.iter().any(|k| k == "timex3"));
        assert!(kinds.iter().any(|k| k == "factuality"));
        assert!(kinds.iter().any(|k| k == "non-terminal"));
        assert!(kinds.iter().any(|k| k == "terminal"));
    }

    #[test]
    fn protocol_has_new_constraint_sorts() {
        let p = protocol();
        let sorts = &p.constraint_sorts;
        assert!(sorts.iter().any(|s| s == "sent"));
        assert!(sorts.iter().any(|s| s == "polarity"));
        assert!(sorts.iter().any(|s| s == "value"));
        assert!(sorts.iter().any(|s| s == "uri"));
        assert!(sorts.iter().any(|s| s == "prediction"));
    }

    #[test]
    fn protocol_has_new_edge_kinds() {
        let p = protocol();
        let edge_kinds: Vec<&str> = p.edge_rules.iter().map(|r| r.edge_kind.as_str()).collect();
        assert!(edge_kinds.contains(&"opinion-ref"));
        assert!(edge_kinds.contains(&"parent-child"));
    }
}
