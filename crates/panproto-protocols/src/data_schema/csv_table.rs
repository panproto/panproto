//! CSV/Table Schema protocol definition (Frictionless Data).
//!
//! Table Schema uses a constrained hypergraph schema theory
//! (`colimit(ThHypergraph, ThConstraint)`) and a set-valued functor
//! instance theory (`ThFunctor`).

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, constraint_value, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the Table Schema protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "table-schema".into(),
        schema_theory: "ThTableSchemaSchema".into(),
        instance_theory: "ThTableSchemaInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "table".into(),
            "field".into(),
            "string".into(),
            "integer".into(),
            "number".into(),
            "boolean".into(),
            "date".into(),
            "datetime".into(),
            "time".into(),
            "year".into(),
            "yearmonth".into(),
            "duration".into(),
            "geopoint".into(),
            "geojson".into(),
            "array".into(),
            "object".into(),
        ],
        constraint_sorts: vec![
            "required".into(),
            "unique".into(),
            "format".into(),
            "minimum".into(),
            "maximum".into(),
            "minLength".into(),
            "maxLength".into(),
            "pattern".into(),
            "enum".into(),
        ],
    }
}

/// Register the component GATs for Table Schema with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_hypergraph_functor(registry, "ThTableSchemaSchema", "ThTableSchemaInstance");
}

/// Parse a Frictionless Data Table Schema JSON into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if the JSON is invalid.
pub fn parse_table_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut he_counter: usize = 0;

    // The root may be a table descriptor or a schema object directly.
    let schema_obj = if json.get("fields").is_some() {
        json
    } else if let Some(s) = json.get("schema") {
        s
    } else {
        return Err(ProtocolError::MissingField("fields".into()));
    };

    let table_name = json.get("name").and_then(|v| v.as_str()).unwrap_or("table");

    builder = builder.vertex(table_name, "table", None)?;

    let fields = schema_obj
        .get("fields")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ProtocolError::MissingField("fields".into()))?;

    let mut sig = HashMap::new();

    for field in fields {
        let field_name = field
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ProtocolError::MissingField("field name".into()))?;

        let field_type = field
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("string");

        let field_id = format!("{table_name}.{field_name}");
        let kind = table_type_to_kind(field_type);
        builder = builder.vertex(&field_id, &kind, None)?;
        builder = builder.edge(table_name, &field_id, "prop", Some(field_name))?;
        sig.insert(field_name.to_string(), field_id.clone());

        // Parse format constraint.
        if let Some(fmt) = field.get("format").and_then(|v| v.as_str()) {
            builder = builder.constraint(&field_id, "format", fmt);
        }

        // Parse constraints block.
        if let Some(constraints) = field.get("constraints").and_then(|v| v.as_object()) {
            if constraints
                .get("required")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                builder = builder.constraint(&field_id, "required", "true");
            }
            if constraints
                .get("unique")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                builder = builder.constraint(&field_id, "unique", "true");
            }
            if let Some(min) = constraints.get("minimum") {
                builder = builder.constraint(&field_id, "minimum", &json_val_to_string(min));
            }
            if let Some(max) = constraints.get("maximum") {
                builder = builder.constraint(&field_id, "maximum", &json_val_to_string(max));
            }
            if let Some(min_len) = constraints.get("minLength") {
                builder = builder.constraint(&field_id, "minLength", &json_val_to_string(min_len));
            }
            if let Some(max_len) = constraints.get("maxLength") {
                builder = builder.constraint(&field_id, "maxLength", &json_val_to_string(max_len));
            }
            if let Some(pattern) = constraints.get("pattern").and_then(|v| v.as_str()) {
                builder = builder.constraint(&field_id, "pattern", pattern);
            }
            if let Some(enum_vals) = constraints.get("enum").and_then(|v| v.as_array()) {
                let vals: Vec<String> = enum_vals
                    .iter()
                    .map(|v| v.as_str().map_or_else(|| v.to_string(), String::from))
                    .collect();
                builder = builder.constraint(&field_id, "enum", &vals.join(","));
            }
        }
    }

    if !sig.is_empty() {
        let he_id = format!("he_{he_counter}");
        he_counter += 1;
        builder = builder.hyper_edge(&he_id, "table", sig, table_name)?;
    }
    let _ = he_counter;

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as Frictionless Data Table Schema JSON.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_table_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let tables: Vec<_> = find_roots(schema, &["prop"]);
    let table = tables
        .into_iter()
        .find(|v| v.kind == "table")
        .ok_or_else(|| ProtocolError::Emit("no table vertex found".into()))?;

    let children = children_by_edge(schema, &table.id, "prop");
    let mut fields = Vec::new();

    for (edge, vertex) in &children {
        let field_name = edge.name.as_deref().unwrap_or(&vertex.id);
        let field_type = kind_to_table_type(&vertex.kind);

        let mut field_obj = serde_json::json!({
            "name": field_name,
            "type": field_type
        });

        if let Some(fmt) = constraint_value(schema, &vertex.id, "format") {
            field_obj["format"] = serde_json::Value::String(fmt.to_string());
        }

        let mut constraints_obj = serde_json::Map::new();
        let constraints = vertex_constraints(schema, &vertex.id);
        for c in &constraints {
            match c.sort.as_str() {
                "required" if c.value == "true" => {
                    constraints_obj.insert("required".into(), serde_json::Value::Bool(true));
                }
                "unique" if c.value == "true" => {
                    constraints_obj.insert("unique".into(), serde_json::Value::Bool(true));
                }
                "minimum" | "maximum" | "minLength" | "maxLength" => {
                    let val = c.value.parse::<f64>().map_or_else(
                        |_| serde_json::Value::String(c.value.clone()),
                        |n| serde_json::json!(n),
                    );
                    constraints_obj.insert(c.sort.clone(), val);
                }
                "pattern" => {
                    constraints_obj
                        .insert("pattern".into(), serde_json::Value::String(c.value.clone()));
                }
                "enum" => {
                    let vals: Vec<serde_json::Value> = c
                        .value
                        .split(',')
                        .map(|s| serde_json::Value::String(s.to_string()))
                        .collect();
                    constraints_obj.insert("enum".into(), serde_json::Value::Array(vals));
                }
                _ => {}
            }
        }
        if !constraints_obj.is_empty() {
            field_obj["constraints"] = serde_json::Value::Object(constraints_obj);
        }

        fields.push(field_obj);
    }

    Ok(serde_json::json!({
        "name": table.id,
        "fields": fields
    }))
}

fn table_type_to_kind(type_str: &str) -> String {
    match type_str {
        "integer" | "number" | "boolean" | "date" | "datetime" | "time" | "year" | "yearmonth"
        | "duration" | "geopoint" | "geojson" | "array" | "object" => type_str,
        _ => "string",
    }
    .into()
}

fn kind_to_table_type(kind: &str) -> &'static str {
    match kind {
        "integer" => "integer",
        "number" => "number",
        "boolean" => "boolean",
        "date" => "date",
        "datetime" => "datetime",
        "time" => "time",
        "year" => "year",
        "yearmonth" => "yearmonth",
        "duration" => "duration",
        "geopoint" => "geopoint",
        "geojson" => "geojson",
        "array" => "array",
        "object" => "object",
        _ => "string",
    }
}

fn json_val_to_string(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => val.to_string(),
    }
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![EdgeRule {
        edge_kind: "prop".into(),
        src_kinds: vec!["table".into()],
        tgt_kinds: vec![],
    }]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_creates_valid_definition() {
        let p = protocol();
        assert_eq!(p.name, "table-schema");
        assert_eq!(p.schema_theory, "ThTableSchemaSchema");
        assert!(p.find_edge_rule("prop").is_some());
    }

    #[test]
    fn register_theories_adds_correct_theories() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThTableSchemaSchema"));
        assert!(registry.contains_key("ThTableSchemaInstance"));
    }

    #[test]
    fn parse_simple_table_schema() {
        let json = serde_json::json!({
            "name": "people",
            "fields": [
                { "name": "name", "type": "string" },
                { "name": "age", "type": "integer", "constraints": { "required": true, "minimum": 0 } },
                { "name": "email", "type": "string", "constraints": { "unique": true, "pattern": ".*@.*" } }
            ]
        });
        let schema = parse_table_schema(&json).expect("should parse");
        assert!(schema.has_vertex("people"));
        assert!(schema.has_vertex("people.name"));
        assert_eq!(schema.vertices.get("people.age").unwrap().kind, "integer");
    }

    #[test]
    fn parse_with_format() {
        let json = serde_json::json!({
            "name": "dates",
            "fields": [
                { "name": "birthday", "type": "date", "format": "%Y-%m-%d" }
            ]
        });
        let schema = parse_table_schema(&json).expect("should parse");
        assert!(schema.has_vertex("dates.birthday"));
    }

    #[test]
    fn emit_roundtrip() {
        let json = serde_json::json!({
            "name": "items",
            "fields": [
                { "name": "id", "type": "integer" },
                { "name": "label", "type": "string" }
            ]
        });
        let schema = parse_table_schema(&json).expect("parse");
        let emitted = emit_table_schema(&schema).expect("emit");
        assert_eq!(emitted["name"], "items");
        assert_eq!(emitted["fields"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn parse_missing_fields_fails() {
        let json = serde_json::json!({ "name": "broken" });
        let result = parse_table_schema(&json);
        assert!(result.is_err());
    }
}
