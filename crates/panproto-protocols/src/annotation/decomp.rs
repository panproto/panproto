//! Universal Decompositional Semantics (UDS / Decomp) protocol definition.
//!
//! UDS graphs are directed acyclic semantic graphs with real-valued node and
//! edge attributes, built on top of Universal Dependencies syntax trees.
//! Every graph is a unified multi-domain DiGraph whose nodes and edges each
//! carry a `domain` and `type` label plus annotation-subspace attributes.
//!
//! Uses Group A theory: `register_constrained_multigraph_wtype`.

use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, constraint_value, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

// ── Annotation subspace constants ────────────────────────────────────────────

const FACTUAL: &str = "factual";

const PRED_PARTICULAR: &str = "pred-particular";
const PRED_DYNAMIC: &str = "pred-dynamic";
const PRED_HYPOTHETICAL: &str = "pred-hypothetical";

const ARG_PARTICULAR: &str = "arg-particular";
const ARG_KIND: &str = "arg-kind";
const ARG_ABSTRACT: &str = "arg-abstract";

/// Protorole properties on semantics-dep edges (predicate → argument).
const PROTOROLE_PROPERTIES: &[&str] = &[
    "awareness",
    "change_of_location",
    "change_of_possession",
    "change_of_state",
    "existed_before",
    "existed_after",
    "existed_during",
    "instigation",
    "location",
    "manner",
    "partitive",
    "purpose",
    "sentient",
    "time",
    "volition",
    "was_for_benefit",
    "was_used",
    "change_of_state_continuous",
];

/// Event-structure subspace properties on predicates.
const EVENT_STRUCTURE_PROPERTIES: &[&str] = &[
    "distributive",
    "dynamic",
    "natural_parts",
    "part_similarity",
    "telic",
];

/// Duration granularities for the time subspace.
const TIME_GRANULARITIES: &[&str] = &[
    "dur-seconds",
    "dur-minutes",
    "dur-hours",
    "dur-days",
    "dur-weeks",
    "dur-months",
    "dur-years",
    "dur-decades",
    "dur-centuries",
    "instant",
    "forever",
];

/// Wordsense supersense properties on argument nodes (26 items).
const WORDSENSE_PROPERTIES: &[&str] = &[
    "supersense-noun.act",
    "supersense-noun.animal",
    "supersense-noun.artifact",
    "supersense-noun.attribute",
    "supersense-noun.body",
    "supersense-noun.cognition",
    "supersense-noun.communication",
    "supersense-noun.event",
    "supersense-noun.feeling",
    "supersense-noun.food",
    "supersense-noun.group",
    "supersense-noun.location",
    "supersense-noun.motive",
    "supersense-noun.object",
    "supersense-noun.person",
    "supersense-noun.phenomenon",
    "supersense-noun.plant",
    "supersense-noun.possession",
    "supersense-noun.process",
    "supersense-noun.quantity",
    "supersense-noun.relation",
    "supersense-noun.shape",
    "supersense-noun.state",
    "supersense-noun.substance",
    "supersense-noun.time",
    "supersense-noun.tops",
];

// ─────────────────────────────────────────────────────────────────────────────

/// Returns the Decomp/UDS protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "decomp".into(),
        schema_theory: "ThDecompSchema".into(),
        instance_theory: "ThDecompInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            // Containment hierarchy
            "corpus".into(),
            "document".into(),
            "sentence".into(),
            // Syntax layer
            "token".into(),
            // Semantics layer
            "predicate".into(),
            "argument".into(),
            // Scalar leaf types for annotation values
            "string".into(),
            "integer".into(),
            "float".into(),
            "boolean".into(),
        ],
        constraint_sorts: vec![
            // Node identity / syntax
            "domain".into(),
            "type".into(),
            "position".into(),
            "form".into(),
            "lemma".into(),
            "upos".into(),
            "xpos".into(),
            "deprel".into(),
            // UDS provenance
            "frompredpatt".into(),
            // Annotation value pair
            "value".into(),
            "confidence".into(),
            // Annotation subspace and property keys
            "subspace".into(),
            "property".into(),
        ],
        has_order: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for Decomp/UDS with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(registry, "ThDecompSchema", "ThDecompInstance");
}

// ── Parse ─────────────────────────────────────────────────────────────────────

/// Parse a JSON-serialised UDS graph into a [`Schema`].
///
/// The expected JSON layout mirrors the Decomp toolkit's serialisation:
///
/// ```json
/// {
///   "corpus_id": "ewt",
///   "documents": {
///     "doc-1": {
///       "sentences": {
///         "sent-1": {
///           "syntax": {
///             "tokens": {
///               "1": {"form":"The","lemma":"the","upos":"DET","xpos":"DT","deprel":"det"}
///             }
///           },
///           "semantics": {
///             "predicates": {
///               "pred-1-1": {
///                 "domain": "semantics", "type": "predicate",
///                 "frompredpatt": true,
///                 "head_token": "1", "span_tokens": ["1"],
///                 "factuality":      {"factual":         {"value": 0.9, "confidence": 1.0}},
///                 "genericity":      {"pred-particular": {"value": 0.8, "confidence": 1.0}},
///                 "time":            {"dur-seconds":     {"value": 0.1, "confidence": 0.5}},
///                 "event_structure": {"telic":           {"value": 0.7, "confidence": 1.0}}
///               }
///             },
///             "arguments": {
///               "arg-1-1": {
///                 "domain": "semantics", "type": "argument",
///                 "head_token": "2", "span_tokens": ["2"],
///                 "genericity": {"arg-particular": {"value": 0.9, "confidence": 1.0}},
///                 "wordsense":  {"supersense-noun.person": {"value": 0.8, "confidence": 1.0}}
///               }
///             },
///             "edges": {
///               "pred-1-1$$arg-1-1": {
///                 "protoroles": {"awareness": {"value": 0.9, "confidence": 1.0}}
///               }
///             }
///           }
///         }
///       }
///     }
///   }
/// }
/// ```
///
/// # Errors
///
/// Returns [`ProtocolError`] if the JSON structure cannot be parsed.
#[allow(clippy::too_many_lines)]
pub fn parse_decomp(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    // Track vertex IDs so helper functions can check token existence before linking.
    let mut known: HashSet<String> = HashSet::new();

    // ── Corpus root vertex ────────────────────────────────────────────────
    let corpus_id = json
        .get("corpus_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("corpus")
        .to_string();

    builder = builder
        .vertex(&corpus_id, "corpus", None)
        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
    known.insert(corpus_id.clone());
    builder = builder.constraint(&corpus_id, "domain", "root");
    builder = builder.constraint(&corpus_id, "type", "corpus");

    // ── Documents ─────────────────────────────────────────────────────────
    let documents = json
        .get("documents")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ProtocolError::MissingField("documents".into()))?;

    for (doc_key, doc_val) in documents {
        let doc_vid = format!("{corpus_id}.{doc_key}");
        builder = builder
            .vertex(&doc_vid, "document", None)
            .map_err(|e| ProtocolError::Parse(e.to_string()))?;
        known.insert(doc_vid.clone());
        builder = builder.constraint(&doc_vid, "domain", "document");
        builder = builder.constraint(&doc_vid, "type", "document");
        builder = builder
            .edge(&corpus_id, &doc_vid, "contains", Some(doc_key))
            .map_err(|e| ProtocolError::Parse(e.to_string()))?;

        // ── Sentences ─────────────────────────────────────────────────────
        let sentences = doc_val
            .get("sentences")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| ProtocolError::MissingField(format!("{doc_key}.sentences")))?;

        for (sent_key, sent_val) in sentences {
            let sent_vid = format!("{doc_vid}.{sent_key}");
            builder = builder
                .vertex(&sent_vid, "sentence", None)
                .map_err(|e| ProtocolError::Parse(e.to_string()))?;
            known.insert(sent_vid.clone());
            builder = builder.constraint(&sent_vid, "domain", "syntax");
            builder = builder.constraint(&sent_vid, "type", "sentence");
            builder = builder
                .edge(&doc_vid, &sent_vid, "contains", Some(sent_key))
                .map_err(|e| ProtocolError::Parse(e.to_string()))?;

            builder = parse_syntax_tokens(builder, sent_val, &sent_vid, &mut known)
                .map_err(|e| ProtocolError::Parse(e.to_string()))?;

            builder = parse_semantics(builder, sent_val, &sent_vid, &known)
                .map_err(|e| ProtocolError::Parse(e.to_string()))?;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Parse the `syntax.tokens` sub-object of a sentence.
fn parse_syntax_tokens(
    mut builder: SchemaBuilder,
    sent_val: &serde_json::Value,
    sent_vid: &str,
    known: &mut HashSet<String>,
) -> Result<SchemaBuilder, panproto_schema::SchemaError> {
    let Some(tokens) = sent_val
        .pointer("/syntax/tokens")
        .and_then(serde_json::Value::as_object)
    else {
        return Ok(builder);
    };

    for (pos_str, tok_val) in tokens {
        let tok_vid = format!("{sent_vid}.tok_{pos_str}");
        builder = builder.vertex(&tok_vid, "token", None)?;
        known.insert(tok_vid.clone());
        builder = builder.constraint(&tok_vid, "domain", "syntax");
        builder = builder.constraint(&tok_vid, "type", "token");
        builder = builder.constraint(&tok_vid, "position", pos_str);
        builder = builder.edge(sent_vid, &tok_vid, "syntax-dep", Some(pos_str))?;

        for field in &["form", "lemma", "upos", "xpos", "deprel"] {
            if let Some(v) = tok_val.get(field).and_then(serde_json::Value::as_str) {
                builder = builder.constraint(&tok_vid, field, v);
            }
        }
    }

    Ok(builder)
}

/// Parse the `semantics` sub-object of a sentence (predicates, arguments, edges).
#[allow(clippy::too_many_lines)]
fn parse_semantics(
    mut builder: SchemaBuilder,
    sent_val: &serde_json::Value,
    sent_vid: &str,
    known: &HashSet<String>,
) -> Result<SchemaBuilder, panproto_schema::SchemaError> {
    let Some(sem) = sent_val
        .get("semantics")
        .and_then(serde_json::Value::as_object)
    else {
        return Ok(builder);
    };

    // Track semantics-layer vertex IDs to resolve edges between them.
    let mut sem_known: HashSet<String> = HashSet::new();

    // ── Predicates ────────────────────────────────────────────────────────
    if let Some(preds) = sem.get("predicates").and_then(serde_json::Value::as_object) {
        for (pred_key, pred_val) in preds {
            let pred_vid = format!("{sent_vid}.{pred_key}");
            builder = builder.vertex(&pred_vid, "predicate", None)?;
            sem_known.insert(pred_vid.clone());
            builder = builder.constraint(&pred_vid, "domain", "semantics");
            builder = builder.constraint(&pred_vid, "type", "predicate");
            builder = builder.edge(sent_vid, &pred_vid, "contains", Some(pred_key))?;

            if let Some(fp) = pred_val.get("frompredpatt") {
                let fp_str = if fp.as_bool().unwrap_or(false) {
                    "true"
                } else {
                    "false"
                };
                builder = builder.constraint(&pred_vid, "frompredpatt", fp_str);
            }

            // Interface head edge
            if let Some(head_pos) = pred_val
                .get("head_token")
                .and_then(serde_json::Value::as_str)
            {
                let tok_vid = format!("{sent_vid}.tok_{head_pos}");
                if known.contains(&tok_vid) {
                    builder = builder.edge(&pred_vid, &tok_vid, "head", Some(head_pos))?;
                }
            }

            // Interface nonhead edges (deduplicated)
            if let Some(span_arr) = pred_val
                .get("span_tokens")
                .and_then(serde_json::Value::as_array)
            {
                let mut added: HashSet<String> = HashSet::new();
                for tok_pos in span_arr.iter().filter_map(serde_json::Value::as_str) {
                    let tok_vid = format!("{sent_vid}.tok_{tok_pos}");
                    if known.contains(&tok_vid) && added.insert(tok_vid.clone()) {
                        builder = builder.edge(&pred_vid, &tok_vid, "nonhead", Some(tok_pos))?;
                    }
                }
            }

            builder = parse_subspace(builder, pred_val, "factuality", &[FACTUAL], &pred_vid)?;
            builder = parse_subspace(
                builder,
                pred_val,
                "genericity",
                &[PRED_PARTICULAR, PRED_DYNAMIC, PRED_HYPOTHETICAL],
                &pred_vid,
            )?;
            builder = parse_subspace(builder, pred_val, "time", TIME_GRANULARITIES, &pred_vid)?;
            builder = parse_subspace(
                builder,
                pred_val,
                "event_structure",
                EVENT_STRUCTURE_PROPERTIES,
                &pred_vid,
            )?;
        }
    }

    // ── Arguments ─────────────────────────────────────────────────────────
    if let Some(args) = sem.get("arguments").and_then(serde_json::Value::as_object) {
        for (arg_key, arg_val) in args {
            let arg_vid = format!("{sent_vid}.{arg_key}");
            builder = builder.vertex(&arg_vid, "argument", None)?;
            sem_known.insert(arg_vid.clone());
            builder = builder.constraint(&arg_vid, "domain", "semantics");
            builder = builder.constraint(&arg_vid, "type", "argument");
            builder = builder.edge(sent_vid, &arg_vid, "contains", Some(arg_key))?;

            if let Some(head_pos) = arg_val
                .get("head_token")
                .and_then(serde_json::Value::as_str)
            {
                let tok_vid = format!("{sent_vid}.tok_{head_pos}");
                if known.contains(&tok_vid) {
                    builder = builder.edge(&arg_vid, &tok_vid, "head", Some(head_pos))?;
                }
            }

            if let Some(span_arr) = arg_val
                .get("span_tokens")
                .and_then(serde_json::Value::as_array)
            {
                let mut added: HashSet<String> = HashSet::new();
                for tok_pos in span_arr.iter().filter_map(serde_json::Value::as_str) {
                    let tok_vid = format!("{sent_vid}.tok_{tok_pos}");
                    if known.contains(&tok_vid) && added.insert(tok_vid.clone()) {
                        builder = builder.edge(&arg_vid, &tok_vid, "nonhead", Some(tok_pos))?;
                    }
                }
            }

            builder = parse_subspace(
                builder,
                arg_val,
                "genericity",
                &[ARG_PARTICULAR, ARG_KIND, ARG_ABSTRACT],
                &arg_vid,
            )?;
            builder = parse_subspace(
                builder,
                arg_val,
                "wordsense",
                WORDSENSE_PROPERTIES,
                &arg_vid,
            )?;
        }
    }

    // ── Semantics dependency edges (pred → arg) with protoroles ───────────
    if let Some(edges) = sem.get("edges").and_then(serde_json::Value::as_object) {
        for (edge_key, edge_val) in edges {
            let Some((pred_key, arg_key)) = edge_key.split_once("$$") else {
                continue;
            };
            let pred_vid = format!("{sent_vid}.{pred_key}");
            let arg_vid = format!("{sent_vid}.{arg_key}");
            if !sem_known.contains(&pred_vid) || !sem_known.contains(&arg_vid) {
                continue;
            }
            builder = builder.edge(&pred_vid, &arg_vid, "sem-dep", Some(edge_key))?;

            // Protorole annotations as float prop vertices on the predicate.
            if let Some(protoroles) = edge_val
                .get("protoroles")
                .and_then(serde_json::Value::as_object)
            {
                for prop in PROTOROLE_PROPERTIES {
                    if let Some(ann) = protoroles.get(*prop) {
                        let prop_vid = format!("{pred_vid}.pr.{arg_key}.{prop}");
                        builder = builder.vertex(&prop_vid, "float", None)?;
                        builder = builder.constraint(&prop_vid, "subspace", "protoroles");
                        builder = builder.constraint(&prop_vid, "property", prop);
                        if let Some(v) = ann.get("value").and_then(serde_json::Value::as_f64) {
                            builder = builder.constraint(&prop_vid, "value", &v.to_string());
                        }
                        if let Some(c) = ann.get("confidence").and_then(serde_json::Value::as_f64) {
                            builder = builder.constraint(&prop_vid, "confidence", &c.to_string());
                        }
                        builder = builder.edge(&pred_vid, &prop_vid, "prop", Some(prop))?;
                    }
                }
            }

            // Event-structure mereology on edges (e.g. pred1_contains_pred2).
            if let Some(event_struct) = edge_val
                .get("event_structure")
                .and_then(serde_json::Value::as_object)
            {
                for (mero_key, ann) in event_struct {
                    let mero_vid = format!("{pred_vid}.es.{arg_key}.{mero_key}");
                    builder = builder.vertex(&mero_vid, "boolean", None)?;
                    builder = builder.constraint(&mero_vid, "subspace", "event_structure");
                    builder = builder.constraint(&mero_vid, "property", mero_key);
                    if let Some(v) = ann.get("value").and_then(serde_json::Value::as_f64) {
                        builder = builder.constraint(&mero_vid, "value", &v.to_string());
                    }
                    if let Some(c) = ann.get("confidence").and_then(serde_json::Value::as_f64) {
                        builder = builder.constraint(&mero_vid, "confidence", &c.to_string());
                    }
                    builder =
                        builder.edge(&pred_vid, &mero_vid, "prop", Some(mero_key.as_str()))?;
                }
            }
        }
    }

    Ok(builder)
}

/// Parse one annotation subspace object and attach `float` prop vertices.
fn parse_subspace(
    mut builder: SchemaBuilder,
    node_val: &serde_json::Value,
    subspace: &str,
    known_props: &[&str],
    parent_vid: &str,
) -> Result<SchemaBuilder, panproto_schema::SchemaError> {
    let Some(subspace_obj) = node_val
        .get(subspace)
        .and_then(serde_json::Value::as_object)
    else {
        return Ok(builder);
    };

    for prop in known_props {
        if let Some(ann) = subspace_obj.get(*prop) {
            let prop_vid = format!("{parent_vid}.{subspace}.{prop}");
            builder = builder.vertex(&prop_vid, "float", None)?;
            builder = builder.constraint(&prop_vid, "subspace", subspace);
            builder = builder.constraint(&prop_vid, "property", prop);
            if let Some(v) = ann.get("value").and_then(serde_json::Value::as_f64) {
                builder = builder.constraint(&prop_vid, "value", &v.to_string());
            }
            if let Some(c) = ann.get("confidence").and_then(serde_json::Value::as_f64) {
                builder = builder.constraint(&prop_vid, "confidence", &c.to_string());
            }
            builder = builder.edge(parent_vid, &prop_vid, "prop", Some(prop))?;
        }
    }

    Ok(builder)
}

// ── Emit ──────────────────────────────────────────────────────────────────────

/// Emit a [`Schema`] back to its JSON UDS representation.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialised.
#[allow(clippy::too_many_lines)]
pub fn emit_decomp(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let corpus = schema
        .vertices
        .values()
        .find(|v| v.kind == "corpus")
        .ok_or_else(|| ProtocolError::Emit("no corpus vertex found".into()))?;

    let corpus_id = corpus.id.clone();
    let mut documents_map = serde_json::Map::new();

    for (_doc_edge, doc_vertex) in children_by_edge(schema, &corpus_id, "contains") {
        let mut sentences_map = serde_json::Map::new();

        for (_sent_edge, sent_vertex) in children_by_edge(schema, &doc_vertex.id, "contains") {
            let sent_json = emit_sentence(schema, &sent_vertex.id);
            let sent_key = sent_vertex.id.rsplit('.').next().unwrap_or(&sent_vertex.id);
            sentences_map.insert(sent_key.to_string(), sent_json);
        }

        let doc_key = doc_vertex.id.rsplit('.').next().unwrap_or(&doc_vertex.id);
        documents_map.insert(
            doc_key.to_string(),
            serde_json::json!({ "sentences": sentences_map }),
        );
    }

    Ok(serde_json::json!({
        "corpus_id": corpus_id,
        "documents": documents_map,
    }))
}

/// Emit a single sentence vertex as a JSON object.
fn emit_sentence(schema: &Schema, sent_vid: &str) -> serde_json::Value {
    // ── Syntax tokens ─────────────────────────────────────────────────────
    let mut tokens_map = serde_json::Map::new();
    for (_edge, tok_vertex) in children_by_edge(schema, sent_vid, "syntax-dep") {
        let mut tok_obj = serde_json::Map::new();
        for sort in &["form", "lemma", "upos", "xpos", "deprel"] {
            if let Some(v) = constraint_value(schema, &tok_vertex.id, sort) {
                tok_obj.insert((*sort).to_string(), serde_json::json!(v));
            }
        }
        let pos = constraint_value(schema, &tok_vertex.id, "position").unwrap_or(&tok_vertex.id);
        tokens_map.insert(pos.to_string(), serde_json::Value::Object(tok_obj));
    }

    // ── Semantics ─────────────────────────────────────────────────────────
    let mut preds_map = serde_json::Map::new();
    let mut args_map = serde_json::Map::new();
    let mut edges_map = serde_json::Map::new();

    for (_edge, child) in children_by_edge(schema, sent_vid, "contains") {
        match child.kind.as_str() {
            "predicate" => {
                let pred_key = child.id.rsplit('.').next().unwrap_or(&child.id);
                preds_map.insert(
                    pred_key.to_string(),
                    emit_sem_node(schema, &child.id, "predicate"),
                );

                // Collect sem-dep edges originating from this predicate.
                for dep_edge in schema
                    .outgoing_edges(&child.id)
                    .iter()
                    .filter(|e| e.kind == "sem-dep")
                {
                    let arg_vid = &dep_edge.tgt;
                    let arg_key = arg_vid.rsplit('.').next().unwrap_or(arg_vid.as_str());
                    let edge_key = dep_edge
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("{pred_key}$${arg_key}"));

                    // Protoroles: prop children of pred scoped to this arg.
                    let mut protoroles_map = serde_json::Map::new();
                    for (_prop_edge, prop_vertex) in children_by_edge(schema, &child.id, "prop") {
                        if constraint_value(schema, &prop_vertex.id, "subspace")
                            != Some("protoroles")
                        {
                            continue;
                        }
                        if !prop_vertex.id.contains(arg_key) {
                            continue;
                        }
                        if let Some(pname) = constraint_value(schema, &prop_vertex.id, "property") {
                            protoroles_map.insert(
                                pname.to_string(),
                                emit_annotation(schema, &prop_vertex.id),
                            );
                        }
                    }

                    let mut edge_obj = serde_json::Map::new();
                    if !protoroles_map.is_empty() {
                        edge_obj.insert(
                            "protoroles".into(),
                            serde_json::Value::Object(protoroles_map),
                        );
                    }
                    edges_map.insert(edge_key, serde_json::Value::Object(edge_obj));
                }
            }
            "argument" => {
                let arg_key = child.id.rsplit('.').next().unwrap_or(&child.id);
                args_map.insert(
                    arg_key.to_string(),
                    emit_sem_node(schema, &child.id, "argument"),
                );
            }
            _ => {}
        }
    }

    let mut sem_obj = serde_json::Map::new();
    if !preds_map.is_empty() {
        sem_obj.insert("predicates".into(), serde_json::Value::Object(preds_map));
    }
    if !args_map.is_empty() {
        sem_obj.insert("arguments".into(), serde_json::Value::Object(args_map));
    }
    if !edges_map.is_empty() {
        sem_obj.insert("edges".into(), serde_json::Value::Object(edges_map));
    }

    serde_json::json!({
        "syntax": { "tokens": tokens_map },
        "semantics": sem_obj,
    })
}

/// Emit a semantics predicate or argument node as a JSON object.
fn emit_sem_node(schema: &Schema, node_vid: &str, sem_type: &str) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert("domain".into(), serde_json::json!("semantics"));
    obj.insert("type".into(), serde_json::json!(sem_type));

    if let Some(fp) = constraint_value(schema, node_vid, "frompredpatt") {
        obj.insert("frompredpatt".into(), serde_json::json!(fp == "true"));
    }

    // head_token
    if let Some(head_edge) = schema
        .outgoing_edges(node_vid)
        .iter()
        .find(|e| e.kind == "head")
    {
        if let Some(pos) = &head_edge.name {
            obj.insert("head_token".into(), serde_json::json!(pos));
        }
    }

    // span_tokens
    let nonhead: Vec<_> = schema
        .outgoing_edges(node_vid)
        .iter()
        .filter(|e| e.kind == "nonhead")
        .collect();
    if !nonhead.is_empty() {
        let span: Vec<serde_json::Value> = nonhead
            .iter()
            .filter_map(|e| e.name.as_deref().map(|n| serde_json::json!(n)))
            .collect();
        obj.insert("span_tokens".into(), serde_json::Value::Array(span));
    }

    // Annotation subspaces from prop children (excluding protoroles).
    let mut subspaces: HashMap<String, serde_json::Map<String, serde_json::Value>> = HashMap::new();
    for (_prop_edge, prop_vertex) in children_by_edge(schema, node_vid, "prop") {
        let sub = constraint_value(schema, &prop_vertex.id, "subspace");
        let prop_name = constraint_value(schema, &prop_vertex.id, "property");
        if sub == Some("protoroles") {
            continue;
        }
        if let (Some(sub_str), Some(prop_str)) = (sub, prop_name) {
            let ann = emit_annotation(schema, &prop_vertex.id);
            subspaces
                .entry(sub_str.to_string())
                .or_default()
                .insert(prop_str.to_string(), ann);
        }
    }
    for (sub, props) in subspaces {
        obj.insert(sub, serde_json::Value::Object(props));
    }

    serde_json::Value::Object(obj)
}

/// Build `{"value": f64, "confidence": f64}` from a vertex's constraints.
fn emit_annotation(schema: &Schema, vertex_id: &str) -> serde_json::Value {
    let mut ann = serde_json::Map::new();
    for c in vertex_constraints(schema, vertex_id) {
        if c.sort == "value" || c.sort == "confidence" {
            if let Ok(f) = c.value.parse::<f64>() {
                ann.insert(c.sort.clone(), serde_json::json!(f));
            }
        }
    }
    serde_json::Value::Object(ann)
}

// ── Edge rules ────────────────────────────────────────────────────────────────

fn edge_rules() -> Vec<EdgeRule> {
    let sem_kinds = || vec!["predicate".to_string(), "argument".to_string()];
    let scalar_kinds = || {
        vec![
            "string".to_string(),
            "integer".to_string(),
            "float".to_string(),
            "boolean".to_string(),
        ]
    };

    vec![
        // Containment: corpus → document → sentence; sentence → pred / arg
        EdgeRule {
            edge_kind: "contains".into(),
            src_kinds: vec!["corpus".into(), "document".into(), "sentence".into()],
            tgt_kinds: vec![
                "document".into(),
                "sentence".into(),
                "predicate".into(),
                "argument".into(),
            ],
        },
        // Syntax dependency: sentence → token (root) or token → token
        EdgeRule {
            edge_kind: "syntax-dep".into(),
            src_kinds: vec!["sentence".into(), "token".into()],
            tgt_kinds: vec!["token".into()],
        },
        // Interface head: semantics node → head syntax token
        EdgeRule {
            edge_kind: "head".into(),
            src_kinds: sem_kinds(),
            tgt_kinds: vec!["token".into()],
        },
        // Interface nonhead: semantics node → span syntax token
        EdgeRule {
            edge_kind: "nonhead".into(),
            src_kinds: sem_kinds(),
            tgt_kinds: vec!["token".into()],
        },
        // Semantics dependency: predicate → argument with protorole annotations
        EdgeRule {
            edge_kind: "sem-dep".into(),
            src_kinds: vec!["predicate".into()],
            tgt_kinds: vec!["argument".into()],
        },
        // Semantics head: argument → predicate (realization)
        EdgeRule {
            edge_kind: "sem-head".into(),
            src_kinds: vec!["argument".into()],
            tgt_kinds: vec!["predicate".into()],
        },
        // Sub-argument structural edges
        EdgeRule {
            edge_kind: "sub-argument".into(),
            src_kinds: vec!["argument".into()],
            tgt_kinds: vec!["argument".into()],
        },
        // Sub-predicate structural edges
        EdgeRule {
            edge_kind: "sub-predicate".into(),
            src_kinds: vec!["predicate".into()],
            tgt_kinds: vec!["predicate".into()],
        },
        // Document relation: cross-sentence edges
        EdgeRule {
            edge_kind: "doc-relation".into(),
            src_kinds: sem_kinds(),
            tgt_kinds: sem_kinds(),
        },
        // Annotation property leaf edges
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: sem_kinds(),
            tgt_kinds: scalar_kinds(),
        },
        // Items: ordered membership edges
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: [sem_kinds(), vec!["sentence".into()]].concat(),
            tgt_kinds: [
                sem_kinds(),
                scalar_kinds(),
                vec!["token".into(), "sentence".into()],
            ]
            .concat(),
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
        assert_eq!(p.name, "decomp");
        assert_eq!(p.schema_theory, "ThDecompSchema");
        assert_eq!(p.instance_theory, "ThDecompInstance");

        for kind in &[
            "contains",
            "syntax-dep",
            "head",
            "nonhead",
            "sem-dep",
            "sem-head",
            "sub-argument",
            "sub-predicate",
            "doc-relation",
            "prop",
            "items",
        ] {
            assert!(
                p.find_edge_rule(kind).is_some(),
                "missing edge rule for '{kind}'"
            );
        }

        for kind in &[
            "corpus",
            "document",
            "sentence",
            "token",
            "predicate",
            "argument",
            "string",
            "integer",
            "float",
            "boolean",
        ] {
            assert!(p.is_known_vertex_kind(kind), "unknown vertex kind '{kind}'");
        }

        for sort in &[
            "domain",
            "type",
            "position",
            "form",
            "lemma",
            "upos",
            "xpos",
            "deprel",
            "frompredpatt",
            "value",
            "confidence",
            "subspace",
            "property",
        ] {
            assert!(
                p.constraint_sorts.iter().any(|s| s == sort),
                "missing constraint sort '{sort}'"
            );
        }
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThDecompSchema"));
        assert!(registry.contains_key("ThDecompInstance"));
        assert!(registry.contains_key("ThGraph"));
        assert!(registry.contains_key("ThConstraint"));
        assert!(registry.contains_key("ThMulti"));
    }

    fn minimal_json() -> serde_json::Value {
        serde_json::json!({
            "corpus_id": "test-corpus",
            "documents": {
                "doc-1": {
                    "sentences": {
                        "sent-1": {
                            "syntax": {
                                "tokens": {
                                    "1": {
                                        "form": "The",
                                        "lemma": "the",
                                        "upos": "DET",
                                        "xpos": "DT",
                                        "deprel": "det"
                                    },
                                    "2": {
                                        "form": "cat",
                                        "lemma": "cat",
                                        "upos": "NOUN",
                                        "xpos": "NN",
                                        "deprel": "nsubj"
                                    }
                                }
                            },
                            "semantics": {
                                "predicates": {
                                    "pred-1-1": {
                                        "domain": "semantics",
                                        "type": "predicate",
                                        "frompredpatt": true,
                                        "head_token": "2",
                                        "span_tokens": ["2"],
                                        "factuality": {
                                            "factual": {"value": 0.9, "confidence": 1.0}
                                        },
                                        "genericity": {
                                            "pred-particular": {"value": 0.8, "confidence": 1.0}
                                        },
                                        "time": {
                                            "dur-seconds": {"value": 0.1, "confidence": 0.5}
                                        },
                                        "event_structure": {
                                            "telic": {"value": 0.7, "confidence": 1.0}
                                        }
                                    }
                                },
                                "arguments": {
                                    "arg-1-1": {
                                        "domain": "semantics",
                                        "type": "argument",
                                        "head_token": "1",
                                        "span_tokens": ["1"],
                                        "genericity": {
                                            "arg-particular": {"value": 0.9, "confidence": 1.0}
                                        },
                                        "wordsense": {
                                            "supersense-noun.person": {
                                                "value": 0.8,
                                                "confidence": 1.0
                                            }
                                        }
                                    }
                                },
                                "edges": {
                                    "pred-1-1$$arg-1-1": {
                                        "protoroles": {
                                            "awareness": {"value": 0.85, "confidence": 1.0},
                                            "instigation": {"value": 0.6, "confidence": 0.8}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn parse_and_emit() {
        let json = minimal_json();
        let schema = parse_decomp(&json).expect("should parse");

        // ── Structural vertices ──────────────────────────────────────────
        assert!(schema.has_vertex("test-corpus"), "missing corpus");
        assert_eq!(schema.vertices["test-corpus"].kind, "corpus");

        assert!(schema.has_vertex("test-corpus.doc-1"), "missing document");
        assert_eq!(schema.vertices["test-corpus.doc-1"].kind, "document");

        let sent_vid = "test-corpus.doc-1.sent-1";
        assert!(schema.has_vertex(sent_vid), "missing sentence");
        assert_eq!(schema.vertices[sent_vid].kind, "sentence");

        // ── Tokens ──────────────────────────────────────────────────────
        let tok1 = format!("{sent_vid}.tok_1");
        let tok2 = format!("{sent_vid}.tok_2");
        assert!(schema.has_vertex(&tok1), "missing tok_1");
        assert!(schema.has_vertex(&tok2), "missing tok_2");
        assert_eq!(schema.vertices[&tok1].kind, "token");
        assert_eq!(
            constraint_value(&schema, &tok1, "form"),
            Some("The"),
            "tok_1 form"
        );
        assert_eq!(
            constraint_value(&schema, &tok2, "upos"),
            Some("NOUN"),
            "tok_2 upos"
        );

        // ── Predicate ───────────────────────────────────────────────────
        let pred_vid = format!("{sent_vid}.pred-1-1");
        assert!(schema.has_vertex(&pred_vid), "missing predicate");
        assert_eq!(schema.vertices[&pred_vid].kind, "predicate");
        assert_eq!(
            constraint_value(&schema, &pred_vid, "frompredpatt"),
            Some("true")
        );

        // ── Argument ────────────────────────────────────────────────────
        let arg_vid = format!("{sent_vid}.arg-1-1");
        assert!(schema.has_vertex(&arg_vid), "missing argument");
        assert_eq!(schema.vertices[&arg_vid].kind, "argument");

        // ── sem-dep edge ────────────────────────────────────────────────
        let dep_count = schema
            .outgoing_edges(&pred_vid)
            .iter()
            .filter(|e| e.kind == "sem-dep")
            .count();
        assert_eq!(dep_count, 1, "expected 1 sem-dep edge");

        // ── Annotation subspace prop vertices ────────────────────────────
        let factual_vid = format!("{pred_vid}.factuality.factual");
        assert!(
            schema.has_vertex(&factual_vid),
            "missing factuality.factual"
        );
        assert_eq!(schema.vertices[&factual_vid].kind, "float");
        assert_eq!(
            constraint_value(&schema, &factual_vid, "value"),
            Some("0.9")
        );
        assert_eq!(
            constraint_value(&schema, &factual_vid, "confidence"),
            Some("1")
        );

        let telic_vid = format!("{pred_vid}.event_structure.telic");
        assert!(schema.has_vertex(&telic_vid), "missing telic");

        let arg_gen_vid = format!("{arg_vid}.genericity.arg-particular");
        assert!(schema.has_vertex(&arg_gen_vid), "missing arg genericity");

        // ── Protorole prop vertices ──────────────────────────────────────
        let pr_aware_vid = format!("{pred_vid}.pr.arg-1-1.awareness");
        assert!(
            schema.has_vertex(&pr_aware_vid),
            "missing protorole awareness"
        );
        assert_eq!(
            constraint_value(&schema, &pr_aware_vid, "subspace"),
            Some("protoroles")
        );
        assert_eq!(
            constraint_value(&schema, &pr_aware_vid, "property"),
            Some("awareness")
        );
        assert_eq!(
            constraint_value(&schema, &pr_aware_vid, "value"),
            Some("0.85")
        );

        // ── Interface edges ──────────────────────────────────────────────
        let pred_head_count = schema
            .outgoing_edges(&pred_vid)
            .iter()
            .filter(|e| e.kind == "head")
            .count();
        assert_eq!(pred_head_count, 1, "predicate should have 1 head edge");

        let arg_head_count = schema
            .outgoing_edges(&arg_vid)
            .iter()
            .filter(|e| e.kind == "head")
            .count();
        assert_eq!(arg_head_count, 1, "argument should have 1 head edge");

        // ── Roundtrip ────────────────────────────────────────────────────
        let emitted = emit_decomp(&schema).expect("should emit");
        let schema2 = parse_decomp(&emitted).expect("should re-parse");
        assert_eq!(
            schema.vertex_count(),
            schema2.vertex_count(),
            "vertex count mismatch on roundtrip"
        );
    }
}
