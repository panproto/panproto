//! `ATProto` protocol definition.
//!
//! The AT Protocol uses a constrained multigraph schema theory
//! (colimit of `ThGraph`, `ThConstraint`, `ThMulti`) and a W-type
//! instance theory with metadata (`ThWType + ThMeta`).
//!
//! Vertex kinds: record, object, array, union, string, integer, boolean,
//! bytes, cid-link, blob, unknown, token.
//!
//! Edge kinds: record-schema, prop, items, variant, ref, self-ref.

use std::collections::HashMap;

use panproto_gat::{Sort, Theory, colimit};
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::error::ProtocolError;
use crate::theories;

/// Returns the `ATProto` protocol definition.
///
/// Schema theory: `colimit(ThGraph, ThConstraint, ThMulti)`.
/// Instance theory: `ThWType + ThMeta`.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "atproto".into(),
        schema_theory: "ThATProtoSchema".into(),
        instance_theory: "ThATProtoInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "record".into(),
            "object".into(),
            "array".into(),
            "union".into(),
            "string".into(),
            "integer".into(),
            "boolean".into(),
            "bytes".into(),
            "cid-link".into(),
            "blob".into(),
            "unknown".into(),
            "token".into(),
            "query".into(),
            "procedure".into(),
            "subscription".into(),
            "ref".into(),
        ],
        constraint_sorts: vec![
            "minLength".into(),
            "maxLength".into(),
            "minimum".into(),
            "maximum".into(),
            "maxGraphemes".into(),
            "enum".into(),
            "const".into(),
            "default".into(),
            "closed".into(),
        ],
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for `ATProto` with a theory registry.
///
/// Registers `ThGraph`, `ThConstraint`, `ThMulti`, `ThWType`, `ThMeta`,
/// and the composed schema/instance theories.
pub fn register_theories<S: ::std::hash::BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    let th_graph = theories::th_graph();
    let th_constraint = theories::th_constraint();
    let th_multi = theories::th_multi();
    let th_wtype = theories::th_wtype();
    let th_meta = theories::th_meta();

    registry.insert("ThGraph".into(), th_graph.clone());
    registry.insert("ThConstraint".into(), th_constraint.clone());
    registry.insert("ThMulti".into(), th_multi.clone());
    registry.insert("ThWType".into(), th_wtype.clone());
    registry.insert("ThMeta".into(), th_meta.clone());

    // Compose schema theory via colimit.
    // Step 1: colimit(ThGraph, ThConstraint) over shared Vertex.
    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);

    if let Ok(gc) = colimit(&th_graph, &th_constraint, &shared_vertex) {
        // Step 2: colimit(gc, ThMulti) over shared {Vertex, Edge}.
        let shared_ve = Theory::new(
            "ThVertexEdge",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![],
            vec![],
        );
        if let Ok(mut schema_theory) = colimit(&gc, &th_multi, &shared_ve) {
            schema_theory.name = "ThATProtoSchema".into();
            registry.insert("ThATProtoSchema".into(), schema_theory);
        }
    }

    // Compose instance theory: colimit(ThWType, ThMeta) over shared Node.
    let shared_node = Theory::new("ThNode", vec![Sort::simple("Node")], vec![], vec![]);
    if let Ok(mut inst_theory) = colimit(&th_wtype, &th_meta, &shared_node) {
        inst_theory.name = "ThATProtoInstance".into();
        registry.insert("ThATProtoInstance".into(), inst_theory);
    }
}

/// Parse an `ATProto` lexicon JSON document into a [`Schema`].
///
/// Walks the `defs` object, creating vertices for each type definition
/// and edges for structural relationships (properties, array items,
/// union variants, references).
///
/// # Errors
///
/// Returns [`ProtocolError`] if the JSON is not a valid lexicon or
/// if schema construction fails.
pub fn parse_lexicon(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();

    let lexicon_id = json
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ProtocolError::MissingField("id".into()))?;

    let defs = json
        .get("defs")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ProtocolError::MissingField("defs".into()))?;

    let mut builder = SchemaBuilder::new(&proto);

    for (def_name, def_value) in defs {
        let def_type = def_value
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("object");

        let vertex_id = if def_name == "main" {
            lexicon_id.to_string()
        } else {
            format!("{lexicon_id}#{def_name}")
        };

        let kind = lexicon_type_to_kind(def_type);
        let nsid = if def_name == "main" {
            Some(lexicon_id)
        } else {
            None
        };

        builder = builder.vertex(&vertex_id, &kind, nsid)?;

        // Parse type-specific structure.
        match def_type {
            "record" => {
                builder = parse_record_def(builder, &vertex_id, def_value, lexicon_id)?;
            }
            "object" => {
                builder = parse_object_def(builder, &vertex_id, def_value, lexicon_id)?;
            }
            "string" | "integer" | "boolean" | "bytes" | "cid-link" | "blob" | "unknown"
            | "token" => {
                builder = parse_constraints(builder, &vertex_id, def_value);
            }
            "array" => {
                builder = parse_array_def(builder, &vertex_id, def_value, lexicon_id)?;
            }
            "union" => {
                builder = parse_union_def(builder, &vertex_id, def_value)?;
            }
            "query" | "procedure" | "subscription" => {
                builder = parse_query_procedure_def(builder, &vertex_id, def_value, lexicon_id)?;
            }
            other => {
                return Err(ProtocolError::Parse(format!(
                    "unrecognized Lexicon definition type: {other}"
                )));
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Map a lexicon type string to our vertex kind.
fn lexicon_type_to_kind(type_str: &str) -> String {
    match type_str {
        "record" => "record",
        "array" => "array",
        "union" => "union",
        "string" => "string",
        "integer" => "integer",
        "boolean" => "boolean",
        "bytes" => "bytes",
        "cid-link" => "cid-link",
        "blob" => "blob",
        "unknown" => "unknown",
        "token" => "token",
        "query" => "query",
        "procedure" => "procedure",
        "subscription" => "subscription",
        "ref" => "ref",
        _ => "object",
    }
    .to_string()
}

/// Parse a record definition, creating the record-schema edge and body object.
fn parse_record_def(
    mut builder: SchemaBuilder,
    record_id: &str,
    def: &serde_json::Value,
    lexicon_id: &str,
) -> Result<SchemaBuilder, ProtocolError> {
    // A record has a record body (the "record" sub-object).
    if let Some(record_body) = def.get("record") {
        let body_id = format!("{record_id}:body");
        builder = builder.vertex(&body_id, "object", None)?;
        builder = builder.edge(record_id, &body_id, "record-schema", None)?;
        builder = parse_object_def(builder, &body_id, record_body, lexicon_id)?;
    }
    Ok(builder)
}

/// Parse an object definition, creating property edges.
fn parse_object_def(
    mut builder: SchemaBuilder,
    object_id: &str,
    def: &serde_json::Value,
    lexicon_id: &str,
) -> Result<SchemaBuilder, ProtocolError> {
    if let Some(properties) = def.get("properties").and_then(serde_json::Value::as_object) {
        let required_fields: Vec<&str> = def
            .get("required")
            .and_then(serde_json::Value::as_array)
            .map(|arr| arr.iter().filter_map(serde_json::Value::as_str).collect())
            .unwrap_or_default();

        for (prop_name, prop_def) in properties {
            let prop_type = prop_def
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("string");

            let prop_vertex_id = format!("{object_id}.{prop_name}");
            let prop_kind = lexicon_type_to_kind(prop_type);

            builder = builder.vertex(&prop_vertex_id, &prop_kind, None)?;
            builder = builder.edge(object_id, &prop_vertex_id, "prop", Some(prop_name))?;

            // Mark as required if in required list.
            if required_fields.contains(&prop_name.as_str()) {
                let req_edge = panproto_schema::Edge {
                    src: object_id.to_string(),
                    tgt: prop_vertex_id.clone(),
                    kind: "prop".into(),
                    name: Some(prop_name.clone()),
                };
                builder = builder.required(object_id, vec![req_edge]);
            }

            // Parse nested structure.
            match prop_type {
                "object" => {
                    builder = parse_object_def(builder, &prop_vertex_id, prop_def, lexicon_id)?;
                }
                "array" => {
                    builder = parse_array_def(builder, &prop_vertex_id, prop_def, lexicon_id)?;
                }
                "union" => {
                    builder = parse_union_def(builder, &prop_vertex_id, prop_def)?;
                }
                "ref" => {
                    // Create a ref vertex and edge for the reference.
                    if let Some(ref_target) =
                        prop_def.get("ref").and_then(serde_json::Value::as_str)
                    {
                        builder = builder.constraint(&prop_vertex_id, "ref", ref_target);
                    }
                }
                _ => {
                    builder = parse_constraints(builder, &prop_vertex_id, prop_def);
                }
            }
        }
    }
    Ok(builder)
}

/// Parse an array definition, creating items edge.
fn parse_array_def(
    mut builder: SchemaBuilder,
    array_id: &str,
    def: &serde_json::Value,
    lexicon_id: &str,
) -> Result<SchemaBuilder, ProtocolError> {
    if let Some(items) = def.get("items") {
        let items_type = items
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("string");

        let items_id = format!("{array_id}:items");
        let items_kind = lexicon_type_to_kind(items_type);

        builder = builder.vertex(&items_id, &items_kind, None)?;
        builder = builder.edge(array_id, &items_id, "items", None)?;

        match items_type {
            "object" => {
                builder = parse_object_def(builder, &items_id, items, lexicon_id)?;
            }
            "union" => {
                builder = parse_union_def(builder, &items_id, items)?;
            }
            _ => {
                builder = parse_constraints(builder, &items_id, items);
            }
        }
    }
    Ok(builder)
}

/// Parse a union definition, creating variant edges.
fn parse_union_def(
    mut builder: SchemaBuilder,
    union_id: &str,
    def: &serde_json::Value,
) -> Result<SchemaBuilder, ProtocolError> {
    if let Some(refs) = def.get("refs").and_then(serde_json::Value::as_array) {
        for (i, ref_val) in refs.iter().enumerate() {
            if let Some(ref_str) = ref_val.as_str() {
                let variant_id = format!("{union_id}:variant{i}");
                // Union variants are modeled as object vertices when we cannot
                // resolve cross-lexicon refs at parse time.
                builder = builder.vertex(&variant_id, "object", None)?;
                builder = builder.edge(union_id, &variant_id, "variant", Some(ref_str))?;
            }
        }
    }
    Ok(builder)
}

/// Parse a query, procedure, or subscription definition.
///
/// These have optional `parameters` (input) and `output` sub-schemas.
fn parse_query_procedure_def(
    mut builder: SchemaBuilder,
    vertex_id: &str,
    def: &serde_json::Value,
    lexicon_id: &str,
) -> Result<SchemaBuilder, ProtocolError> {
    // Parse parameters (input schema).
    if let Some(params) = def.get("parameters") {
        let params_id = format!("{vertex_id}:params");
        let params_type = params
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("object");
        let params_kind = lexicon_type_to_kind(params_type);
        builder = builder.vertex(&params_id, &params_kind, None)?;
        builder = builder.edge(vertex_id, &params_id, "prop", Some("parameters"))?;
        if params_type == "object" {
            builder = parse_object_def(builder, &params_id, params, lexicon_id)?;
        } else {
            builder = parse_constraints(builder, &params_id, params);
        }
    }

    // Parse input schema (used by procedures).
    if let Some(input) = def.get("input") {
        if let Some(input_schema) = input.get("schema") {
            let input_id = format!("{vertex_id}:input");
            let input_type = input_schema
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("object");
            let input_kind = lexicon_type_to_kind(input_type);
            builder = builder.vertex(&input_id, &input_kind, None)?;
            builder = builder.edge(vertex_id, &input_id, "prop", Some("input"))?;
            if input_type == "object" {
                builder = parse_object_def(builder, &input_id, input_schema, lexicon_id)?;
            } else {
                builder = parse_constraints(builder, &input_id, input_schema);
            }
        }
    }

    // Parse output schema.
    if let Some(output) = def.get("output") {
        if let Some(output_schema) = output.get("schema") {
            let output_id = format!("{vertex_id}:output");
            let output_type = output_schema
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("object");
            let output_kind = lexicon_type_to_kind(output_type);
            builder = builder.vertex(&output_id, &output_kind, None)?;
            builder = builder.edge(vertex_id, &output_id, "prop", Some("output"))?;
            if output_type == "object" {
                builder = parse_object_def(builder, &output_id, output_schema, lexicon_id)?;
            } else {
                builder = parse_constraints(builder, &output_id, output_schema);
            }
        }
    }

    // Parse message (used by subscriptions).
    if let Some(message) = def.get("message") {
        if let Some(msg_schema) = message.get("schema") {
            let msg_id = format!("{vertex_id}:message");
            let msg_type = msg_schema
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("object");
            let msg_kind = lexicon_type_to_kind(msg_type);
            builder = builder.vertex(&msg_id, &msg_kind, None)?;
            builder = builder.edge(vertex_id, &msg_id, "prop", Some("message"))?;
            if msg_type == "object" {
                builder = parse_object_def(builder, &msg_id, msg_schema, lexicon_id)?;
            } else {
                builder = parse_constraints(builder, &msg_id, msg_schema);
            }
        }
    }

    Ok(builder)
}

/// Parse constraints from a type definition.
fn parse_constraints(
    mut builder: SchemaBuilder,
    vertex_id: &str,
    def: &serde_json::Value,
) -> SchemaBuilder {
    let constraint_fields = [
        "minLength",
        "maxLength",
        "minimum",
        "maximum",
        "maxGraphemes",
        "enum",
        "const",
        "default",
        "closed",
    ];

    for field in &constraint_fields {
        if let Some(value) = def.get(field) {
            let value_str = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Array(arr) => {
                    // For enum arrays, join values.
                    arr.iter()
                        .filter_map(serde_json::Value::as_str)
                        .collect::<Vec<_>>()
                        .join(",")
                }
                _ => value.to_string(),
            };
            builder = builder.constraint(vertex_id, field, &value_str);
        }
    }
    builder
}

/// Emit a [`Schema`] as an `ATProto` lexicon JSON value.
///
/// Reconstructs the lexicon document from the schema graph, including
/// the record body, properties, and constraints.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_lexicon(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    // Find the root record vertex (has an nsid).
    let root = schema
        .vertices
        .values()
        .find(|v| v.nsid.is_some())
        .ok_or_else(|| ProtocolError::Emit("no root vertex with nsid found".into()))?;

    let nsid = root.nsid.as_deref().unwrap_or(&root.id);

    let mut defs = serde_json::Map::new();

    // Build the main definition.
    let main_def = emit_lexicon_def(schema, root)?;
    defs.insert("main".to_string(), main_def);

    Ok(serde_json::json!({
        "lexicon": 1,
        "id": nsid,
        "defs": defs
    }))
}

/// Emit a single lexicon definition as a JSON value.
fn emit_lexicon_def(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
) -> Result<serde_json::Value, ProtocolError> {
    use crate::emit::{children_by_edge, vertex_constraints};

    match vertex.kind.as_str() {
        "record" => {
            let mut obj = serde_json::Map::new();
            obj.insert("type".to_string(), serde_json::json!("record"));

            // Walk record-schema edge to get body object.
            let body_edges = children_by_edge(schema, &vertex.id, "record-schema");
            if let Some((_, body_vertex)) = body_edges.first() {
                let body = emit_lexicon_object(schema, body_vertex)?;
                obj.insert("record".to_string(), body);
            }

            Ok(serde_json::Value::Object(obj))
        }
        "object" => emit_lexicon_object(schema, vertex),
        "array" => {
            let mut obj = serde_json::Map::new();
            obj.insert("type".to_string(), serde_json::json!("array"));

            let items_edges = children_by_edge(schema, &vertex.id, "items");
            if let Some((_, items_vertex)) = items_edges.first() {
                let items_val = emit_lexicon_def(schema, items_vertex)?;
                obj.insert("items".to_string(), items_val);
            }

            Ok(serde_json::Value::Object(obj))
        }
        "union" => {
            let mut obj = serde_json::Map::new();
            obj.insert("type".to_string(), serde_json::json!("union"));

            let variants = children_by_edge(schema, &vertex.id, "variant");
            let refs: Vec<serde_json::Value> = variants
                .iter()
                .filter_map(|(edge, _)| edge.name.as_deref().map(|n| serde_json::json!(n)))
                .collect();
            if !refs.is_empty() {
                obj.insert("refs".to_string(), serde_json::Value::Array(refs));
            }

            Ok(serde_json::Value::Object(obj))
        }
        _ => {
            // Scalar types: string, integer, boolean, etc.
            let mut obj = serde_json::Map::new();
            obj.insert("type".to_string(), serde_json::json!(vertex.kind.as_str()));

            // Add constraints.
            let constraints = vertex_constraints(schema, &vertex.id);
            for c in &constraints {
                let val = emit_constraint_value(c);
                obj.insert(c.sort.clone(), val);
            }

            Ok(serde_json::Value::Object(obj))
        }
    }
}

/// Emit an object definition with properties.
fn emit_lexicon_object(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
) -> Result<serde_json::Value, ProtocolError> {
    use crate::emit::children_by_edge;

    let mut obj = serde_json::Map::new();
    obj.insert("type".to_string(), serde_json::json!("object"));

    let props = children_by_edge(schema, &vertex.id, "prop");
    if !props.is_empty() {
        let mut properties = serde_json::Map::new();
        for (edge, prop_vertex) in &props {
            let prop_name = edge.name.as_deref().unwrap_or(&prop_vertex.id);
            let prop_val = emit_lexicon_def(schema, prop_vertex)?;
            properties.insert(prop_name.to_string(), prop_val);
        }
        obj.insert(
            "properties".to_string(),
            serde_json::Value::Object(properties),
        );
    }

    // Reconstruct required fields from schema.required.
    if let Some(req_edges) = schema.required.get(&vertex.id) {
        let required: Vec<serde_json::Value> = req_edges
            .iter()
            .filter_map(|e| e.name.as_deref().map(|n| serde_json::json!(n)))
            .collect();
        if !required.is_empty() {
            obj.insert("required".to_string(), serde_json::Value::Array(required));
        }
    }

    Ok(serde_json::Value::Object(obj))
}

/// Convert a constraint to a JSON value, using numbers where appropriate.
fn emit_constraint_value(c: &panproto_schema::Constraint) -> serde_json::Value {
    match c.sort.as_str() {
        "minLength" | "maxLength" | "minimum" | "maximum" | "maxGraphemes" => c
            .value
            .parse::<i64>()
            .map_or_else(|_| serde_json::json!(c.value), |n| serde_json::json!(n)),
        "closed" => c
            .value
            .parse::<bool>()
            .map_or_else(|_| serde_json::json!(c.value), |b| serde_json::json!(b)),
        "enum" => {
            let vals: Vec<serde_json::Value> = c
                .value
                .split(',')
                .map(|s| serde_json::json!(s.trim()))
                .collect();
            serde_json::Value::Array(vals)
        }
        _ => serde_json::json!(c.value),
    }
}

/// Well-formedness rules for `ATProto` edges.
fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "record-schema".into(),
            src_kinds: vec!["record".into()],
            tgt_kinds: vec!["object".into()],
        },
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec![
                "object".into(),
                "query".into(),
                "procedure".into(),
                "subscription".into(),
            ],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec!["array".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "variant".into(),
            src_kinds: vec!["union".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "ref".into(),
            src_kinds: vec![],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "self-ref".into(),
            src_kinds: vec![],
            tgt_kinds: vec![],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_creates_valid_definition() {
        let p = protocol();
        assert_eq!(p.name, "atproto");
        assert_eq!(p.schema_theory, "ThATProtoSchema");
        assert_eq!(p.instance_theory, "ThATProtoInstance");
        assert!(!p.edge_rules.is_empty());
        assert!(p.find_edge_rule("record-schema").is_some());
        assert!(p.find_edge_rule("prop").is_some());
        assert!(p.find_edge_rule("items").is_some());
        assert!(p.find_edge_rule("variant").is_some());
    }

    #[test]
    fn register_theories_adds_correct_theories() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);

        assert!(registry.contains_key("ThGraph"), "ThGraph missing");
        assert!(
            registry.contains_key("ThConstraint"),
            "ThConstraint missing"
        );
        assert!(registry.contains_key("ThMulti"), "ThMulti missing");
        assert!(registry.contains_key("ThWType"), "ThWType missing");
        assert!(registry.contains_key("ThMeta"), "ThMeta missing");
        assert!(
            registry.contains_key("ThATProtoSchema"),
            "ThATProtoSchema missing"
        );
        assert!(
            registry.contains_key("ThATProtoInstance"),
            "ThATProtoInstance missing"
        );

        // Verify schema theory has expected sorts.
        let schema_t = &registry["ThATProtoSchema"];
        assert!(schema_t.find_sort("Vertex").is_some());
        assert!(schema_t.find_sort("Edge").is_some());
        assert!(schema_t.find_sort("Constraint").is_some());
    }

    #[test]
    fn parse_simple_lexicon() {
        let lexicon = serde_json::json!({
            "lexicon": 1,
            "id": "app.bsky.feed.post",
            "defs": {
                "main": {
                    "type": "record",
                    "record": {
                        "type": "object",
                        "required": ["text", "createdAt"],
                        "properties": {
                            "text": {
                                "type": "string",
                                "maxLength": 3000,
                                "maxGraphemes": 300
                            },
                            "createdAt": {
                                "type": "string"
                            }
                        }
                    }
                }
            }
        });

        let schema = parse_lexicon(&lexicon);
        assert!(schema.is_ok(), "parse_lexicon should succeed: {schema:?}");
        let schema = schema.ok();
        let schema = schema.as_ref();

        // Should have: record vertex, body object, text string, createdAt string.
        assert!(
            schema.is_some_and(|s| s.has_vertex("app.bsky.feed.post")),
            "record vertex should exist"
        );
        assert!(
            schema.is_some_and(|s| s.has_vertex("app.bsky.feed.post:body")),
            "body object vertex should exist"
        );
        assert!(
            schema.is_some_and(|s| s.has_vertex("app.bsky.feed.post:body.text")),
            "text vertex should exist"
        );
        assert!(
            schema.is_some_and(|s| s.has_vertex("app.bsky.feed.post:body.createdAt")),
            "createdAt vertex should exist"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn emit_lexicon_roundtrip() {
        let lexicon = serde_json::json!({
            "lexicon": 1,
            "id": "app.bsky.feed.post",
            "defs": {
                "main": {
                    "type": "record",
                    "record": {
                        "type": "object",
                        "required": ["text", "createdAt"],
                        "properties": {
                            "text": {
                                "type": "string",
                                "maxLength": 3000,
                                "maxGraphemes": 300
                            },
                            "createdAt": {
                                "type": "string"
                            }
                        }
                    }
                }
            }
        });

        let schema1 = parse_lexicon(&lexicon).expect("first parse should succeed");
        let emitted = emit_lexicon(&schema1).expect("emit should succeed");
        let schema2 = parse_lexicon(&emitted).expect("re-parse should succeed");

        assert_eq!(
            schema1.vertex_count(),
            schema2.vertex_count(),
            "vertex counts should match after round-trip"
        );
        assert_eq!(
            schema1.edge_count(),
            schema2.edge_count(),
            "edge counts should match after round-trip"
        );
    }

    #[test]
    fn parse_lexicon_missing_id_fails() {
        let lexicon = serde_json::json!({
            "defs": {}
        });

        let result = parse_lexicon(&lexicon);
        assert!(result.is_err());
    }
}
