//! JSON Schema protocol definition and parser.
//!
//! JSON Schema uses the same constrained multigraph schema theory as
//! `ATProto`: `colimit(ThGraph, ThConstraint, ThMulti)`.
//! Instance theory: `ThWType`.
//!
//! The parser handles `$ref`, `allOf`, `oneOf`, `anyOf`, and standard
//! type/constraint keywords.

use std::collections::HashMap;

use panproto_gat::{Sort, Theory, colimit};
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::error::ProtocolError;
use crate::theories;

/// Returns the JSON Schema protocol definition.
///
/// Schema theory: `colimit(ThGraph, ThConstraint, ThMulti)`.
/// Instance theory: `ThWType`.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "json-schema".into(),
        schema_theory: "ThJsonSchemaSchema".into(),
        instance_theory: "ThJsonSchemaInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "object".into(),
            "array".into(),
            "string".into(),
            "integer".into(),
            "boolean".into(),
            "unknown".into(),
            "not".into(),
            "if".into(),
            "then".into(),
            "else".into(),
            "union".into(),
        ],
        constraint_sorts: vec![
            "type".into(),
            "minLength".into(),
            "maxLength".into(),
            "pattern".into(),
            "minimum".into(),
            "maximum".into(),
            "required".into(),
            "format".into(),
            "enum".into(),
            "const".into(),
            "additionalProperties".into(),
        ],
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for JSON Schema with a theory registry.
pub fn register_theories<S: ::std::hash::BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    let th_graph = theories::th_graph();
    let th_constraint = theories::th_constraint();
    let th_multi = theories::th_multi();
    let th_wtype = theories::th_wtype();

    // Schema theory: same composition as ATProto.
    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);

    if let Ok(gc) = colimit(&th_graph, &th_constraint, &shared_vertex) {
        let shared_ve = Theory::new(
            "ThVertexEdge",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![],
            vec![],
        );
        if let Ok(mut schema_theory) = colimit(&gc, &th_multi, &shared_ve) {
            schema_theory.name = "ThJsonSchemaSchema".into();
            registry.insert("ThJsonSchemaSchema".into(), schema_theory);
        }
    }

    // Instance theory is ThWType.
    let mut inst = th_wtype;
    inst.name = "ThJsonSchemaInstance".into();
    registry.insert("ThJsonSchemaInstance".into(), inst);
}

/// Parse a JSON Schema document into a [`Schema`].
///
/// Handles `type`, `properties`, `items`, `allOf`, `oneOf`, `anyOf`,
/// and `$ref` keywords. Nested schemas are flattened into vertices
/// and edges.
///
/// # Errors
///
/// Returns [`ProtocolError`] if the JSON is not a valid schema or
/// if construction fails.
pub fn parse_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut counter: usize = 0;
    let mut defs_map: HashMap<String, String> = HashMap::new();

    // Pre-walk `$defs` and `definitions` so that `$ref` resolution can find them.
    for defs_key in &["$defs", "definitions"] {
        if let Some(defs) = json.get(*defs_key).and_then(serde_json::Value::as_object) {
            for (def_name, def_schema) in defs {
                let def_id = format!("root:{defs_key}/{def_name}");
                builder = walk_schema(builder, def_schema, &def_id, &mut counter, &defs_map)?;
                let ref_path = format!("#/{defs_key}/{def_name}");
                defs_map.insert(ref_path, def_id);
            }
        }
    }

    builder = walk_schema(builder, json, "root", &mut counter, &defs_map)?;

    let schema = builder.build()?;
    Ok(schema)
}

/// Recursively walk a JSON Schema object.
#[allow(clippy::too_many_lines)]
fn walk_schema(
    mut builder: SchemaBuilder,
    schema: &serde_json::Value,
    current_id: &str,
    counter: &mut usize,
    defs_map: &HashMap<String, String>,
) -> Result<SchemaBuilder, ProtocolError> {
    // Handle `type` as array (e.g., `["string", "null"]`): create a union-like vertex.
    let type_val = schema.get("type");
    let kind = if let Some(serde_json::Value::Array(types)) = type_val {
        // Union of types: create a union vertex and sub-vertices for each type.
        builder = builder.vertex(current_id, "union", None)?;
        for (i, t) in types.iter().enumerate() {
            if let Some(t_str) = t.as_str() {
                let sub_id = format!("{current_id}:type{i}");
                let sub_kind = json_type_to_kind(t_str);
                builder = builder.vertex(&sub_id, &sub_kind, None)?;
                builder = builder.edge(current_id, &sub_id, "variant", Some(t_str))?;
            }
        }
        None // vertex already created
    } else {
        let type_str = type_val
            .and_then(serde_json::Value::as_str)
            .unwrap_or("object");
        let kind = json_type_to_kind(type_str);
        builder = builder.vertex(current_id, &kind, None)?;
        Some(kind)
    };
    let _ = kind; // used above for vertex creation

    // Add constraints.
    let constraint_fields = [
        "minLength",
        "maxLength",
        "pattern",
        "minimum",
        "maximum",
        "format",
    ];
    for field in &constraint_fields {
        if let Some(val) = schema.get(field) {
            let val_str = match val {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                _ => val.to_string(),
            };
            builder = builder.constraint(current_id, field, &val_str);
        }
    }

    // Handle enum constraint.
    if let Some(enum_val) = schema.get("enum").and_then(serde_json::Value::as_array) {
        let vals: Vec<String> = enum_val
            .iter()
            .map(|v| v.as_str().map_or_else(|| v.to_string(), String::from))
            .collect();
        builder = builder.constraint(current_id, "enum", &vals.join(","));
    }

    // Handle const constraint.
    if let Some(const_val) = schema.get("const") {
        let val_str = match const_val {
            serde_json::Value::String(s) => s.clone(),
            _ => const_val.to_string(),
        };
        builder = builder.constraint(current_id, "const", &val_str);
    }

    // Handle properties (object type).
    if let Some(properties) = schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
    {
        for (prop_name, prop_schema) in properties {
            let prop_id = format!("{current_id}.{prop_name}");
            builder = walk_schema(builder, prop_schema, &prop_id, counter, defs_map)?;
            builder = builder.edge(current_id, &prop_id, "prop", Some(prop_name))?;
        }
    }

    // Handle patternProperties.
    if let Some(pattern_props) = schema
        .get("patternProperties")
        .and_then(serde_json::Value::as_object)
    {
        for (pattern, prop_schema) in pattern_props {
            *counter += 1;
            let prop_id = format!("{current_id}:patternProp{counter}");
            builder = walk_schema(builder, prop_schema, &prop_id, counter, defs_map)?;
            builder = builder.edge(current_id, &prop_id, "pattern-prop", Some(pattern))?;
        }
    }

    // Handle additionalProperties.
    if let Some(additional) = schema.get("additionalProperties") {
        match additional {
            serde_json::Value::Bool(false) => {
                builder = builder.constraint(current_id, "additionalProperties", "false");
            }
            serde_json::Value::Object(_) => {
                *counter += 1;
                let add_id = format!("{current_id}:additionalProperties{counter}");
                builder = walk_schema(builder, additional, &add_id, counter, defs_map)?;
                builder =
                    builder.edge(current_id, &add_id, "prop", Some("additionalProperties"))?;
            }
            // true is the default; no constraint needed for other variants.
            _ => {}
        }
    }

    // Handle items (array type).
    if let Some(items) = schema.get("items") {
        let items_id = format!("{current_id}:items");
        builder = walk_schema(builder, items, &items_id, counter, defs_map)?;
        builder = builder.edge(current_id, &items_id, "items", None)?;
    }

    // Handle allOf / oneOf / anyOf as union-like structures.
    for combiner in &["allOf", "oneOf", "anyOf"] {
        if let Some(arr) = schema.get(*combiner).and_then(serde_json::Value::as_array) {
            for (i, sub_schema) in arr.iter().enumerate() {
                let sub_id = format!("{current_id}:{combiner}{i}");
                builder = walk_schema(builder, sub_schema, &sub_id, counter, defs_map)?;
                builder = builder.edge(current_id, &sub_id, "variant", Some(combiner))?;
            }
        }
    }

    // Handle `not`: recurse into the schema, creating a vertex with kind "not".
    if let Some(not_schema) = schema.get("not") {
        *counter += 1;
        let not_id = format!("{current_id}:not{counter}");
        builder = builder.vertex(&not_id, "not", None)?;
        // Walk the negated schema as a child of the not vertex.
        let negated_id = format!("{not_id}:negated");
        builder = walk_schema(builder, not_schema, &negated_id, counter, defs_map)?;
        builder = builder.edge(&not_id, &negated_id, "variant", Some("not"))?;
        builder = builder.edge(current_id, &not_id, "variant", Some("not"))?;
    }

    // Handle `if`/`then`/`else`: create vertices for each conditional branch.
    if let Some(if_schema) = schema.get("if") {
        *counter += 1;
        let if_id = format!("{current_id}:if{counter}");
        builder = builder.vertex(&if_id, "if", None)?;
        let if_cond_id = format!("{if_id}:cond");
        builder = walk_schema(builder, if_schema, &if_cond_id, counter, defs_map)?;
        builder = builder.edge(&if_id, &if_cond_id, "variant", Some("condition"))?;
        builder = builder.edge(current_id, &if_id, "variant", Some("if"))?;

        if let Some(then_schema) = schema.get("then") {
            let then_id = format!("{current_id}:then{counter}");
            builder = builder.vertex(&then_id, "then", None)?;
            let then_body_id = format!("{then_id}:body");
            builder = walk_schema(builder, then_schema, &then_body_id, counter, defs_map)?;
            builder = builder.edge(&then_id, &then_body_id, "variant", Some("body"))?;
            builder = builder.edge(current_id, &then_id, "variant", Some("then"))?;
        }

        if let Some(else_schema) = schema.get("else") {
            let else_id = format!("{current_id}:else{counter}");
            builder = builder.vertex(&else_id, "else", None)?;
            let else_body_id = format!("{else_id}:body");
            builder = walk_schema(builder, else_schema, &else_body_id, counter, defs_map)?;
            builder = builder.edge(&else_id, &else_body_id, "variant", Some("body"))?;
            builder = builder.edge(current_id, &else_id, "variant", Some("else"))?;
        }
    }

    // Handle $ref: resolve to a definition vertex or create a ref vertex.
    if let Some(ref_str) = schema.get("$ref").and_then(serde_json::Value::as_str) {
        if let Some(def_vertex_id) = defs_map.get(ref_str) {
            builder = builder.edge(current_id, def_vertex_id, "ref", Some(ref_str))?;
        } else {
            *counter += 1;
            let ref_id = format!("{current_id}:ref{counter}");
            builder = builder.vertex(&ref_id, "object", None)?;
            builder = builder.edge(current_id, &ref_id, "ref", Some(ref_str))?;
        }
    }

    Ok(builder)
}

/// Map JSON Schema type to vertex kind.
fn json_type_to_kind(type_str: &str) -> String {
    match type_str {
        "array" => "array",
        "string" => "string",
        "integer" | "number" => "integer",
        "boolean" => "boolean",
        "null" => "unknown",
        // "object" and all others default to "object".
        _ => "object",
    }
    .to_string()
}

/// Map a vertex kind back to a JSON Schema type keyword.
fn kind_to_json_type(kind: &str) -> &'static str {
    match kind {
        "string" => "string",
        "integer" => "integer",
        "boolean" => "boolean",
        "array" => "array",
        "unknown" => "null",
        _ => "object",
    }
}

/// Emit a [`Schema`] as a JSON Schema document.
///
/// Reconstructs the JSON Schema from the schema graph, including
/// properties, items, type keywords, and validation constraints.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    // The root vertex is "root".
    let root = schema
        .vertices
        .get("root")
        .ok_or_else(|| ProtocolError::Emit("no root vertex found".into()))?;

    emit_json_schema_vertex(schema, root)
}

/// Emit a single vertex as a JSON Schema value.
fn emit_json_schema_vertex(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
) -> Result<serde_json::Value, ProtocolError> {
    use crate::emit::{children_by_edge, vertex_constraints};

    let mut obj = serde_json::Map::new();

    // Handle union vertices (type array).
    if vertex.kind == "union" {
        let variants = children_by_edge(schema, &vertex.id, "variant");
        let types: Vec<serde_json::Value> = variants
            .iter()
            .map(|(_edge, v)| {
                let type_str = kind_to_json_type(&v.kind);
                serde_json::json!(type_str)
            })
            .collect();
        if !types.is_empty() {
            obj.insert("type".to_string(), serde_json::Value::Array(types));
        }
        return Ok(serde_json::Value::Object(obj));
    }

    // Set type keyword.
    let json_type = kind_to_json_type(&vertex.kind);
    obj.insert("type".to_string(), serde_json::json!(json_type));

    // Add constraints.
    let constraints = vertex_constraints(schema, &vertex.id);
    for c in &constraints {
        match c.sort.as_str() {
            "type" => {
                // Already handled above.
            }
            "minLength" | "maxLength" | "minimum" | "maximum" => {
                if let Ok(n) = c.value.parse::<i64>() {
                    obj.insert(c.sort.to_string(), serde_json::json!(n));
                } else {
                    obj.insert(c.sort.to_string(), serde_json::json!(c.value));
                }
            }
            "enum" => {
                let vals: Vec<serde_json::Value> = c
                    .value
                    .split(',')
                    .map(|s| serde_json::json!(s.trim()))
                    .collect();
                obj.insert("enum".to_string(), serde_json::Value::Array(vals));
            }
            "const" => {
                obj.insert("const".to_string(), serde_json::json!(c.value));
            }
            "additionalProperties" => {
                if c.value == "false" {
                    obj.insert("additionalProperties".to_string(), serde_json::json!(false));
                }
            }
            _ => {
                obj.insert(c.sort.to_string(), serde_json::json!(c.value));
            }
        }
    }

    // Handle properties (object type).
    let props = children_by_edge(schema, &vertex.id, "prop");
    if !props.is_empty() {
        let mut properties = serde_json::Map::new();
        for (edge, prop_vertex) in &props {
            let prop_name = edge.name.as_deref().unwrap_or(&prop_vertex.id);
            // Skip additionalProperties vertex (handled as constraint).
            if prop_name == "additionalProperties" {
                continue;
            }
            let prop_val = emit_json_schema_vertex(schema, prop_vertex)?;
            properties.insert(prop_name.to_string(), prop_val);
        }
        if !properties.is_empty() {
            obj.insert(
                "properties".to_string(),
                serde_json::Value::Object(properties),
            );
        }
    }

    // Handle items (array type).
    let items = children_by_edge(schema, &vertex.id, "items");
    if let Some((_, items_vertex)) = items.first() {
        let items_val = emit_json_schema_vertex(schema, items_vertex)?;
        obj.insert("items".to_string(), items_val);
    }

    // Handle variant edges for oneOf/anyOf/allOf.
    let variants = children_by_edge(schema, &vertex.id, "variant");
    if !variants.is_empty() {
        // Group variants by their edge name (combiner keyword).
        let mut combiners: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
        for (edge, variant_vertex) in &variants {
            let combiner = edge.name.as_deref().unwrap_or("oneOf");
            let val = emit_json_schema_vertex(schema, variant_vertex)?;
            combiners.entry(combiner.to_string()).or_default().push(val);
        }
        for (key, schemas) in &combiners {
            obj.insert(key.clone(), serde_json::Value::Array(schemas.clone()));
        }
    }

    Ok(serde_json::Value::Object(obj))
}

/// Well-formedness rules for JSON Schema edges.
fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["object".into()],
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
        EdgeRule {
            edge_kind: "pattern-prop".into(),
            src_kinds: vec!["object".into()],
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
        assert_eq!(p.name, "json-schema");
        assert_eq!(p.schema_theory, "ThJsonSchemaSchema");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThJsonSchemaSchema"));
        assert!(registry.contains_key("ThJsonSchemaInstance"));
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn emit_schema_roundtrip() {
        let schema_json = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "maxLength": 100
                },
                "age": {
                    "type": "integer",
                    "minimum": 0
                }
            }
        });

        let schema1 = parse_schema(&schema_json).expect("first parse should succeed");
        let emitted = emit_schema(&schema1).expect("emit should succeed");
        let schema2 = parse_schema(&emitted).expect("re-parse should succeed");

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
    fn parse_simple_json_schema() {
        let schema_json = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "maxLength": 100
                },
                "age": {
                    "type": "integer",
                    "minimum": 0
                }
            }
        });

        let schema = parse_schema(&schema_json);
        assert!(schema.is_ok(), "parse_schema should succeed: {schema:?}");
        let schema = schema.ok();
        let schema = schema.as_ref();

        assert!(schema.is_some_and(|s| s.has_vertex("root")));
        assert!(schema.is_some_and(|s| s.has_vertex("root.name")));
        assert!(schema.is_some_and(|s| s.has_vertex("root.age")));
    }
}
