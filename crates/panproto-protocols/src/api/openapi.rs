//! `OpenAPI`/Swagger protocol definition.
//!
//! `OpenAPI` uses a constrained multigraph schema theory
//! (`colimit(ThGraph, ThConstraint, ThMulti)`) and a W-type
//! instance theory (`ThWType`).
//!
//! Vertex kinds: path, operation, parameter, request-body, response,
//! schema-object, header, string, integer, number, boolean, array, object.
//!
//! Edge kinds: prop, items, variant, ref.

use std::collections::HashMap;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the `OpenAPI` protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "openapi".into(),
        schema_theory: "ThOpenAPISchema".into(),
        instance_theory: "ThOpenAPIInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "path".into(),
            "operation".into(),
            "parameter".into(),
            "request-body".into(),
            "response".into(),
            "schema-object".into(),
            "header".into(),
            "string".into(),
            "integer".into(),
            "number".into(),
            "boolean".into(),
            "array".into(),
            "object".into(),
        ],
        constraint_sorts: vec![
            "required".into(),
            "format".into(),
            "enum".into(),
            "default".into(),
            "minimum".into(),
            "maximum".into(),
            "pattern".into(),
            "minLength".into(),
            "maxLength".into(),
            "minItems".into(),
            "maxItems".into(),
            "deprecated".into(),
        ],
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for `OpenAPI` with a theory registry.
pub fn register_theories<S: ::std::hash::BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThOpenAPISchema",
        "ThOpenAPIInstance",
    );
}

/// Parse an `OpenAPI` JSON document into a [`Schema`].
///
/// Walks paths, operations, parameters, request bodies, responses,
/// and schemas to produce a flat vertex/edge graph.
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing or schema construction fails.
#[allow(clippy::too_many_lines)]
pub fn parse_openapi(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut counter: usize = 0;

    // Pre-walk components/schemas so $ref resolution can find them.
    let mut defs_map: HashMap<String, String> = HashMap::new();
    if let Some(schemas) = json
        .pointer("/components/schemas")
        .and_then(serde_json::Value::as_object)
    {
        for (name, schema_val) in schemas {
            let schema_id = format!("components/schemas/{name}");
            builder = walk_schema(builder, schema_val, &schema_id, &mut counter)?;
            let ref_path = format!("#/components/schemas/{name}");
            defs_map.insert(ref_path, schema_id);
        }
    }

    // Walk paths.
    if let Some(paths) = json.get("paths").and_then(serde_json::Value::as_object) {
        for (path_str, path_item) in paths {
            let path_id = format!("path:{path_str}");
            builder = builder.vertex(&path_id, "path", None)?;
            builder = parse_path_item(builder, path_item, &path_id, &mut counter, &defs_map)?;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Parse a single path item, walking HTTP methods.
fn parse_path_item(
    mut builder: SchemaBuilder,
    path_item: &serde_json::Value,
    path_id: &str,
    counter: &mut usize,
    defs_map: &HashMap<String, String>,
) -> Result<SchemaBuilder, ProtocolError> {
    for method in &[
        "get", "post", "put", "delete", "patch", "options", "head", "trace",
    ] {
        if let Some(op) = path_item.get(*method) {
            let op_id = format!("{path_id}:{method}");
            builder = builder.vertex(&op_id, "operation", None)?;
            builder = builder.edge(path_id, &op_id, "prop", Some(method))?;

            if op.get("deprecated").and_then(serde_json::Value::as_bool) == Some(true) {
                builder = builder.constraint(&op_id, "deprecated", "true");
            }

            builder = parse_operation(builder, op, &op_id, counter, defs_map)?;
        }
    }
    Ok(builder)
}

/// Parse an operation's parameters, request body, and responses.
fn parse_operation(
    mut builder: SchemaBuilder,
    op: &serde_json::Value,
    op_id: &str,
    counter: &mut usize,
    defs_map: &HashMap<String, String>,
) -> Result<SchemaBuilder, ProtocolError> {
    // Parameters.
    if let Some(params) = op.get("parameters").and_then(serde_json::Value::as_array) {
        for (i, param) in params.iter().enumerate() {
            let param_name = param
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown");
            let param_id = format!("{op_id}:param{i}");
            builder = builder.vertex(&param_id, "parameter", None)?;
            builder = builder.edge(op_id, &param_id, "prop", Some(param_name))?;

            if param.get("required").and_then(serde_json::Value::as_bool) == Some(true) {
                builder = builder.constraint(&param_id, "required", "true");
            }

            if let Some(schema_val) = param.get("schema") {
                let s_id = format!("{param_id}:schema");
                builder = walk_schema_or_ref(builder, schema_val, &s_id, counter, defs_map)?;
                builder = builder.edge(&param_id, &s_id, "prop", Some("schema"))?;
            }
        }
    }

    // Request body.
    if let Some(req_body) = op.get("requestBody") {
        let rb_id = format!("{op_id}:requestBody");
        builder = builder.vertex(&rb_id, "request-body", None)?;
        builder = builder.edge(op_id, &rb_id, "prop", Some("requestBody"))?;

        if let Some(content) = req_body
            .get("content")
            .and_then(serde_json::Value::as_object)
        {
            for (media_type, media_obj) in content {
                if let Some(schema_val) = media_obj.get("schema") {
                    let s_id = format!("{rb_id}:{media_type}");
                    builder = walk_schema_or_ref(builder, schema_val, &s_id, counter, defs_map)?;
                    builder = builder.edge(&rb_id, &s_id, "prop", Some(media_type))?;
                }
            }
        }
    }

    // Responses.
    if let Some(responses) = op.get("responses").and_then(serde_json::Value::as_object) {
        for (status, resp) in responses {
            let resp_id = format!("{op_id}:resp{status}");
            builder = builder.vertex(&resp_id, "response", None)?;
            builder = builder.edge(op_id, &resp_id, "prop", Some(status))?;

            if let Some(content) = resp.get("content").and_then(serde_json::Value::as_object) {
                for (media_type, media_obj) in content {
                    if let Some(schema_val) = media_obj.get("schema") {
                        let s_id = format!("{resp_id}:{media_type}");
                        builder =
                            walk_schema_or_ref(builder, schema_val, &s_id, counter, defs_map)?;
                        builder = builder.edge(&resp_id, &s_id, "prop", Some(media_type))?;
                    }
                }
            }

            if let Some(headers) = resp.get("headers").and_then(serde_json::Value::as_object) {
                for (hdr_name, _hdr_obj) in headers {
                    let hdr_id = format!("{resp_id}:hdr:{hdr_name}");
                    builder = builder.vertex(&hdr_id, "header", None)?;
                    builder = builder.edge(&resp_id, &hdr_id, "prop", Some(hdr_name))?;
                }
            }
        }
    }

    Ok(builder)
}

/// Walk a schema value, resolving `$ref` if present.
fn walk_schema_or_ref(
    builder: SchemaBuilder,
    schema: &serde_json::Value,
    current_id: &str,
    counter: &mut usize,
    defs_map: &HashMap<String, String>,
) -> Result<SchemaBuilder, ProtocolError> {
    if let Some(ref_str) = schema.get("$ref").and_then(serde_json::Value::as_str) {
        let mut b = builder.vertex(current_id, "schema-object", None)?;
        if let Some(def_id) = defs_map.get(ref_str) {
            b = b.edge(current_id, def_id, "ref", Some(ref_str))?;
        }
        Ok(b)
    } else {
        walk_schema(builder, schema, current_id, counter)
    }
}

/// Recursively walk a JSON Schema object within an `OpenAPI` spec.
fn walk_schema(
    mut builder: SchemaBuilder,
    schema: &serde_json::Value,
    current_id: &str,
    counter: &mut usize,
) -> Result<SchemaBuilder, ProtocolError> {
    let type_str = schema
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("object");

    let kind = match type_str {
        "string" => "string",
        "integer" => "integer",
        "number" => "number",
        "boolean" => "boolean",
        "array" => "array",
        _ => "object",
    };

    builder = builder.vertex(current_id, kind, None)?;

    // Add constraints.
    for field in &[
        "format",
        "minimum",
        "maximum",
        "pattern",
        "minLength",
        "maxLength",
        "minItems",
        "maxItems",
    ] {
        if let Some(val) = schema.get(field) {
            let val_str = match val {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                _ => val.to_string(),
            };
            builder = builder.constraint(current_id, field, &val_str);
        }
    }

    if let Some(enum_val) = schema.get("enum").and_then(serde_json::Value::as_array) {
        let vals: Vec<String> = enum_val
            .iter()
            .map(|v| v.as_str().map_or_else(|| v.to_string(), String::from))
            .collect();
        builder = builder.constraint(current_id, "enum", &vals.join(","));
    }

    if let Some(default_val) = schema.get("default") {
        let val_str = match default_val {
            serde_json::Value::String(s) => s.clone(),
            _ => default_val.to_string(),
        };
        builder = builder.constraint(current_id, "default", &val_str);
    }

    // Properties.
    if let Some(properties) = schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
    {
        let required_fields: Vec<&str> = schema
            .get("required")
            .and_then(serde_json::Value::as_array)
            .map(|arr| arr.iter().filter_map(serde_json::Value::as_str).collect())
            .unwrap_or_default();

        for (prop_name, prop_schema) in properties {
            let prop_id = format!("{current_id}.{prop_name}");
            builder = walk_schema(builder, prop_schema, &prop_id, counter)?;
            builder = builder.edge(current_id, &prop_id, "prop", Some(prop_name))?;
            if required_fields.contains(&prop_name.as_str()) {
                builder = builder.constraint(&prop_id, "required", "true");
            }
        }
    }

    // Items.
    if let Some(items) = schema.get("items") {
        let items_id = format!("{current_id}:items");
        builder = walk_schema(builder, items, &items_id, counter)?;
        builder = builder.edge(current_id, &items_id, "items", None)?;
    }

    // oneOf / anyOf / allOf.
    for combiner in &["oneOf", "anyOf", "allOf"] {
        if let Some(arr) = schema.get(*combiner).and_then(serde_json::Value::as_array) {
            for (i, sub_schema) in arr.iter().enumerate() {
                *counter += 1;
                let sub_id = format!("{current_id}:{combiner}{i}_{counter}");
                builder = walk_schema(builder, sub_schema, &sub_id, counter)?;
                builder = builder.edge(current_id, &sub_id, "variant", Some(combiner))?;
            }
        }
    }

    Ok(builder)
}

/// Emit a [`Schema`] as an `OpenAPI` JSON document.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_openapi(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let mut paths = serde_json::Map::new();
    let mut component_schemas = serde_json::Map::new();

    let roots = find_roots(schema, &["prop", "items", "variant", "ref"]);

    for root in &roots {
        if root.kind == "path" {
            let path_name = root.id.strip_prefix("path:").unwrap_or(&root.id);
            let mut path_obj = serde_json::Map::new();

            for (edge, op_vertex) in children_by_edge(schema, &root.id, "prop") {
                if op_vertex.kind == "operation" {
                    let method = edge.name.as_deref().unwrap_or("get");
                    let op_obj = emit_operation(schema, &op_vertex.id);
                    path_obj.insert(method.to_string(), op_obj);
                }
            }

            paths.insert(path_name.to_string(), serde_json::Value::Object(path_obj));
        } else {
            let schema_obj = emit_schema_value(schema, &root.id);
            let name = root
                .id
                .strip_prefix("components/schemas/")
                .unwrap_or(&root.id);
            component_schemas.insert(name.to_string(), schema_obj);
        }
    }

    let mut result = serde_json::Map::new();
    result.insert("openapi".into(), serde_json::Value::String("3.0.0".into()));
    result.insert(
        "info".into(),
        serde_json::json!({"title": "Generated", "version": "1.0.0"}),
    );
    result.insert("paths".into(), serde_json::Value::Object(paths));

    if !component_schemas.is_empty() {
        let mut components = serde_json::Map::new();
        components.insert(
            "schemas".into(),
            serde_json::Value::Object(component_schemas),
        );
        result.insert("components".into(), serde_json::Value::Object(components));
    }

    Ok(serde_json::Value::Object(result))
}

/// Emit an operation vertex as a JSON object.
fn emit_operation(schema: &Schema, op_id: &str) -> serde_json::Value {
    let mut obj = serde_json::Map::new();

    if constraint_value(schema, op_id, "deprecated") == Some("true") {
        obj.insert("deprecated".into(), serde_json::Value::Bool(true));
    }

    let children = children_by_edge(schema, op_id, "prop");

    // Parameters.
    let params: Vec<serde_json::Value> = children
        .iter()
        .filter(|(_, v)| v.kind == "parameter")
        .map(|(edge, v)| {
            let mut p = serde_json::Map::new();
            p.insert(
                "name".into(),
                serde_json::Value::String(edge.name.as_deref().unwrap_or("unknown").to_string()),
            );
            p.insert("in".into(), serde_json::Value::String("query".into()));
            if constraint_value(schema, &v.id, "required") == Some("true") {
                p.insert("required".into(), serde_json::Value::Bool(true));
            }
            serde_json::Value::Object(p)
        })
        .collect();
    if !params.is_empty() {
        obj.insert("parameters".into(), serde_json::Value::Array(params));
    }

    // Responses.
    let responses: Vec<_> = children
        .iter()
        .filter(|(_, v)| v.kind == "response")
        .collect();
    if !responses.is_empty() {
        let mut resp_obj = serde_json::Map::new();
        for (edge, _v) in &responses {
            let status = edge.name.as_deref().unwrap_or("200");
            let mut r = serde_json::Map::new();
            r.insert(
                "description".into(),
                serde_json::Value::String(String::new()),
            );
            resp_obj.insert(status.to_string(), serde_json::Value::Object(r));
        }
        obj.insert("responses".into(), serde_json::Value::Object(resp_obj));
    }

    serde_json::Value::Object(obj)
}

/// Emit a schema vertex as a JSON Schema value.
fn emit_schema_value(schema: &Schema, vertex_id: &str) -> serde_json::Value {
    let Some(vertex) = schema.vertices.get(vertex_id) else {
        return serde_json::Value::Object(serde_json::Map::new());
    };

    let mut obj = serde_json::Map::new();

    let type_str = match vertex.kind.as_str() {
        "string" => Some("string"),
        "integer" => Some("integer"),
        "number" => Some("number"),
        "boolean" => Some("boolean"),
        "array" => Some("array"),
        "object" | "schema-object" => Some("object"),
        _ => None,
    };

    if let Some(t) = type_str {
        obj.insert("type".into(), serde_json::Value::String(t.into()));
    }

    for field in &[
        "format",
        "minimum",
        "maximum",
        "pattern",
        "minLength",
        "maxLength",
        "minItems",
        "maxItems",
    ] {
        if let Some(val) = constraint_value(schema, vertex_id, field) {
            if let Ok(n) = val.parse::<f64>() {
                obj.insert((*field).into(), serde_json::json!(n));
            } else {
                obj.insert((*field).into(), serde_json::Value::String(val.to_string()));
            }
        }
    }

    // Properties.
    let props = children_by_edge(schema, vertex_id, "prop");
    if !props.is_empty() {
        let mut properties = serde_json::Map::new();
        let mut required_list = Vec::new();
        for (edge, _child) in &props {
            let name = edge.name.as_deref().unwrap_or("");
            let child_schema = emit_schema_value(schema, &edge.tgt);
            properties.insert(name.to_string(), child_schema);
            if constraint_value(schema, &edge.tgt, "required") == Some("true") {
                required_list.push(serde_json::Value::String(name.to_string()));
            }
        }
        obj.insert("properties".into(), serde_json::Value::Object(properties));
        if !required_list.is_empty() {
            obj.insert("required".into(), serde_json::Value::Array(required_list));
        }
    }

    // Items.
    let items = children_by_edge(schema, vertex_id, "items");
    if let Some((edge, _)) = items.first() {
        let items_schema = emit_schema_value(schema, &edge.tgt);
        obj.insert("items".into(), items_schema);
    }

    serde_json::Value::Object(obj)
}

/// Well-formedness rules for `OpenAPI` edges.
fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec![
                "path".into(),
                "operation".into(),
                "parameter".into(),
                "request-body".into(),
                "response".into(),
                "object".into(),
                "schema-object".into(),
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
            src_kinds: vec![],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "ref".into(),
            src_kinds: vec![],
            tgt_kinds: vec![],
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
        assert_eq!(p.name, "openapi");
        assert_eq!(p.schema_theory, "ThOpenAPISchema");
        assert_eq!(p.instance_theory, "ThOpenAPIInstance");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThOpenAPISchema"));
        assert!(registry.contains_key("ThOpenAPIInstance"));
    }

    #[test]
    fn parse_minimal() {
        let doc = serde_json::json!({
            "openapi": "3.0.0",
            "info": {"title": "Test", "version": "1.0.0"},
            "paths": {
                "/users": {
                    "get": {
                        "parameters": [
                            {"name": "limit", "in": "query", "schema": {"type": "integer"}}
                        ],
                        "responses": {
                            "200": {
                                "description": "OK",
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "type": "array",
                                            "items": {"type": "string"}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
        let schema = parse_openapi(&doc).expect("should parse");
        assert!(schema.has_vertex("path:/users"));
        assert!(schema.has_vertex("path:/users:get"));
    }

    #[test]
    fn emit_minimal() {
        let doc = serde_json::json!({
            "openapi": "3.0.0",
            "info": {"title": "Test", "version": "1.0.0"},
            "paths": {
                "/pets": {
                    "get": {
                        "responses": {
                            "200": {"description": "OK"}
                        }
                    }
                }
            }
        });
        let schema = parse_openapi(&doc).expect("should parse");
        let emitted = emit_openapi(&schema).expect("should emit");
        assert!(emitted.get("paths").is_some());
    }

    #[test]
    fn roundtrip() {
        let doc = serde_json::json!({
            "openapi": "3.0.0",
            "info": {"title": "Test", "version": "1.0.0"},
            "paths": {
                "/items": {
                    "get": {
                        "responses": {
                            "200": {"description": "OK"}
                        }
                    }
                }
            }
        });
        let schema = parse_openapi(&doc).expect("parse");
        let emitted = emit_openapi(&schema).expect("emit");
        let schema2 = parse_openapi(&emitted).expect("re-parse");
        assert_eq!(schema.vertices.len(), schema2.vertices.len());
    }
}
