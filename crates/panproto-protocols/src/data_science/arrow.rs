//! Apache Arrow schema protocol definition.
//!
//! Arrow uses a constrained hypergraph schema theory
//! (`colimit(ThHypergraph, ThConstraint)`) and a set-valued functor
//! instance theory (`ThFunctor`).

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the Arrow protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "arrow".into(),
        schema_theory: "ThArrowSchema".into(),
        instance_theory: "ThArrowInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "schema".into(),
            "field".into(),
            "null".into(),
            "bool".into(),
            "int8".into(),
            "int16".into(),
            "int32".into(),
            "int64".into(),
            "uint8".into(),
            "uint16".into(),
            "uint32".into(),
            "uint64".into(),
            "float16".into(),
            "float32".into(),
            "float64".into(),
            "decimal128".into(),
            "decimal256".into(),
            "date32".into(),
            "date64".into(),
            "time32".into(),
            "time64".into(),
            "timestamp".into(),
            "duration".into(),
            "binary".into(),
            "utf8".into(),
            "large-binary".into(),
            "large-utf8".into(),
            "list".into(),
            "struct-type".into(),
            "map".into(),
            "union".into(),
            "dictionary".into(),
        ],
        constraint_sorts: vec![
            "nullable".into(),
            "metadata".into(),
            "timezone".into(),
            "unit".into(),
            "precision".into(),
            "scale".into(),
            "bit-width".into(),
        ],
    }
}

/// Register the component GATs for Arrow with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_hypergraph_functor(registry, "ThArrowSchema", "ThArrowInstance");
}

/// Parse an Arrow schema JSON into a [`Schema`].
///
/// Expects a JSON object with a `fields` array. Each field has `name`,
/// `type` (object or string), `nullable`, and optional `children`.
///
/// # Errors
///
/// Returns [`ProtocolError`] if the JSON is invalid.
pub fn parse_arrow_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut he_counter: usize = 0;

    let schema_name = json
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("schema");

    builder = builder.vertex(schema_name, "schema", None)?;

    let fields = json
        .get("fields")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ProtocolError::MissingField("fields".into()))?;

    let sig = parse_arrow_fields(&mut builder, fields, schema_name)?;

    if !sig.is_empty() {
        let he_id = format!("he_{he_counter}");
        he_counter += 1;
        builder = builder.hyper_edge(&he_id, "schema", sig, schema_name)?;
    }
    let _ = he_counter;

    // Schema-level metadata.
    if let Some(metadata) = json.get("metadata").and_then(|v| v.as_object()) {
        let meta_str: Vec<String> = metadata
            .iter()
            .map(|(k, v)| format!("{k}={}", v.as_str().unwrap_or("")))
            .collect();
        if !meta_str.is_empty() {
            builder = builder.constraint(schema_name, "metadata", &meta_str.join(","));
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

fn parse_arrow_fields(
    builder: &mut SchemaBuilder,
    fields: &[serde_json::Value],
    parent_id: &str,
) -> Result<HashMap<String, String>, ProtocolError> {
    let mut sig = HashMap::new();

    for field in fields {
        let field_name = field
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ProtocolError::MissingField("field name".into()))?;

        let kind = resolve_arrow_type(field);
        let field_id = format!("{parent_id}.{field_name}");

        let mut b = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
        b = b.vertex(&field_id, &kind, None)?;

        // Nullable constraint.
        if let Some(nullable) = field.get("nullable").and_then(serde_json::Value::as_bool) {
            b = b.constraint(&field_id, "nullable", &nullable.to_string());
        }

        // Type-specific constraints.
        if let Some(type_obj) = field.get("type").and_then(|v| v.as_object()) {
            if let Some(tz) = type_obj.get("timezone").and_then(|v| v.as_str()) {
                b = b.constraint(&field_id, "timezone", tz);
            }
            if let Some(unit) = type_obj.get("unit").and_then(|v| v.as_str()) {
                b = b.constraint(&field_id, "unit", unit);
            }
            if let Some(precision) = type_obj.get("precision") {
                b = b.constraint(&field_id, "precision", &json_val_to_string(precision));
            }
            if let Some(scale) = type_obj.get("scale") {
                b = b.constraint(&field_id, "scale", &json_val_to_string(scale));
            }
            if let Some(bw) = type_obj.get("bitWidth") {
                b = b.constraint(&field_id, "bit-width", &json_val_to_string(bw));
            }
        }

        // Field-level metadata.
        if let Some(metadata) = field.get("metadata").and_then(|v| v.as_object()) {
            let meta_str: Vec<String> = metadata
                .iter()
                .map(|(k, v)| format!("{k}={}", v.as_str().unwrap_or("")))
                .collect();
            if !meta_str.is_empty() {
                b = b.constraint(&field_id, "metadata", &meta_str.join(","));
            }
        }

        b = b.edge(parent_id, &field_id, "prop", Some(field_name))?;
        sig.insert(field_name.to_string(), field_id.clone());

        *builder = b;

        // Recurse into children.
        if let Some(children) = field.get("children").and_then(|v| v.as_array()) {
            let _ = parse_arrow_fields(builder, children, &field_id)?;
        }
    }

    Ok(sig)
}

fn resolve_arrow_type(field: &serde_json::Value) -> String {
    if let Some(type_obj) = field.get("type") {
        if let Some(type_str) = type_obj.as_str() {
            return arrow_type_to_kind(type_str);
        }
        if let Some(name) = type_obj.get("name").and_then(|v| v.as_str()) {
            return arrow_type_to_kind(name);
        }
    }
    "utf8".into()
}

fn arrow_type_to_kind(type_str: &str) -> String {
    match type_str.to_lowercase().as_str() {
        "null" => "null",
        "bool" | "boolean" => "bool",
        "int8" => "int8",
        "int16" => "int16",
        "int32" | "int" => "int32",
        "int64" => "int64",
        "uint8" => "uint8",
        "uint16" => "uint16",
        "uint32" => "uint32",
        "uint64" => "uint64",
        "float16" | "halffloat" => "float16",
        "float32" | "float" => "float32",
        "float64" | "double" => "float64",
        "decimal128" | "decimal" => "decimal128",
        "decimal256" => "decimal256",
        "date32" | "date" => "date32",
        "date64" => "date64",
        "time32" | "time" => "time32",
        "time64" => "time64",
        "timestamp" => "timestamp",
        "duration" => "duration",
        "binary" => "binary",
        "large-binary" | "largebinary" => "large-binary",
        "large-utf8" | "largeutf8" | "largestring" => "large-utf8",
        "list" => "list",
        "struct" | "struct-type" => "struct-type",
        "map" => "map",
        "union" => "union",
        "dictionary" => "dictionary",
        _ => "utf8",
    }
    .into()
}

#[allow(clippy::match_same_arms)]
fn kind_to_arrow_type(kind: &str) -> &'static str {
    match kind {
        "null" => "null",
        "bool" => "bool",
        "int8" => "int8",
        "int16" => "int16",
        "int32" => "int32",
        "int64" => "int64",
        "uint8" => "uint8",
        "uint16" => "uint16",
        "uint32" => "uint32",
        "uint64" => "uint64",
        "float16" => "float16",
        "float32" => "float32",
        "float64" => "float64",
        "decimal128" => "decimal128",
        "decimal256" => "decimal256",
        "date32" => "date32",
        "date64" => "date64",
        "time32" => "time32",
        "time64" => "time64",
        "timestamp" => "timestamp",
        "duration" => "duration",
        "binary" => "binary",
        "large-binary" => "large-binary",
        "large-utf8" => "large-utf8",
        "list" => "list",
        "struct-type" => "struct",
        "map" => "map",
        "union" => "union",
        "dictionary" => "dictionary",
        _ => "utf8",
    }
}

fn json_val_to_string(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => val.to_string(),
    }
}

/// Emit a [`Schema`] as Arrow schema JSON.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_arrow_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let roots: Vec<_> = find_roots(schema, &["prop"]);
    let root = roots
        .into_iter()
        .find(|v| v.kind == "schema")
        .ok_or_else(|| ProtocolError::Emit("no schema vertex found".into()))?;

    let fields = emit_arrow_fields(schema, &root.id);

    let mut result = serde_json::json!({
        "name": root.id,
        "fields": fields
    });

    if let Some(meta) = constraint_value(schema, &root.id, "metadata") {
        let meta_obj: serde_json::Map<String, serde_json::Value> = meta
            .split(',')
            .filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                let k = parts.next()?.to_string();
                let v = parts.next().unwrap_or("").to_string();
                Some((k, serde_json::Value::String(v)))
            })
            .collect();
        result["metadata"] = serde_json::Value::Object(meta_obj);
    }

    Ok(result)
}

fn emit_arrow_fields(schema: &Schema, parent_id: &str) -> Vec<serde_json::Value> {
    let children = children_by_edge(schema, parent_id, "prop");
    let mut fields = Vec::new();

    for (edge, vertex) in &children {
        let field_name = edge.name.as_deref().unwrap_or(&vertex.id);
        let arrow_type = kind_to_arrow_type(&vertex.kind);

        let mut field_obj = serde_json::json!({
            "name": field_name,
            "type": { "name": arrow_type }
        });

        if let Some(nullable) = constraint_value(schema, &vertex.id, "nullable") {
            field_obj["nullable"] = serde_json::Value::Bool(nullable == "true");
        }
        if let Some(tz) = constraint_value(schema, &vertex.id, "timezone") {
            field_obj["type"]["timezone"] = serde_json::Value::String(tz.to_string());
        }
        if let Some(unit) = constraint_value(schema, &vertex.id, "unit") {
            field_obj["type"]["unit"] = serde_json::Value::String(unit.to_string());
        }
        if let Some(prec) = constraint_value(schema, &vertex.id, "precision") {
            field_obj["type"]["precision"] = serde_json::Value::String(prec.to_string());
        }
        if let Some(scale) = constraint_value(schema, &vertex.id, "scale") {
            field_obj["type"]["scale"] = serde_json::Value::String(scale.to_string());
        }
        if let Some(bw) = constraint_value(schema, &vertex.id, "bit-width") {
            field_obj["type"]["bitWidth"] = serde_json::Value::String(bw.to_string());
        }

        let sub_fields = emit_arrow_fields(schema, &vertex.id);
        if !sub_fields.is_empty() {
            field_obj["children"] = serde_json::Value::Array(sub_fields);
        }

        fields.push(field_obj);
    }

    fields
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![EdgeRule {
        edge_kind: "prop".into(),
        src_kinds: vec!["schema".into(), "struct-type".into(), "list".into()],
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
        assert_eq!(p.name, "arrow");
        assert_eq!(p.schema_theory, "ThArrowSchema");
        assert!(p.find_edge_rule("prop").is_some());
    }

    #[test]
    fn register_theories_adds_correct_theories() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThArrowSchema"));
        assert!(registry.contains_key("ThArrowInstance"));
    }

    #[test]
    fn parse_simple_schema() {
        let json = serde_json::json!({
            "fields": [
                { "name": "id", "type": { "name": "int64" }, "nullable": false },
                { "name": "name", "type": { "name": "utf8" }, "nullable": true },
                { "name": "score", "type": { "name": "float64" }, "nullable": true }
            ]
        });
        let schema = parse_arrow_schema(&json).expect("should parse");
        assert!(schema.has_vertex("schema"));
        assert!(schema.has_vertex("schema.id"));
        assert_eq!(schema.vertices.get("schema.id").unwrap().kind, "int64");
        assert_eq!(schema.vertices.get("schema.name").unwrap().kind, "utf8");
    }

    #[test]
    fn parse_with_children() {
        let json = serde_json::json!({
            "name": "record",
            "fields": [
                {
                    "name": "address",
                    "type": { "name": "struct" },
                    "nullable": true,
                    "children": [
                        { "name": "street", "type": { "name": "utf8" }, "nullable": true },
                        { "name": "zip", "type": { "name": "int32" }, "nullable": false }
                    ]
                }
            ]
        });
        let schema = parse_arrow_schema(&json).expect("should parse");
        assert!(schema.has_vertex("record.address"));
        assert!(schema.has_vertex("record.address.street"));
    }

    #[test]
    fn parse_timestamp_with_timezone() {
        let json = serde_json::json!({
            "fields": [
                {
                    "name": "ts",
                    "type": { "name": "timestamp", "timezone": "UTC", "unit": "MICROSECOND" },
                    "nullable": false
                }
            ]
        });
        let schema = parse_arrow_schema(&json).expect("should parse");
        assert!(schema.has_vertex("schema.ts"));
        assert_eq!(schema.vertices.get("schema.ts").unwrap().kind, "timestamp");
    }

    #[test]
    fn emit_roundtrip() {
        let json = serde_json::json!({
            "fields": [
                { "name": "x", "type": { "name": "int32" }, "nullable": false },
                { "name": "y", "type": { "name": "float64" }, "nullable": true }
            ]
        });
        let schema = parse_arrow_schema(&json).expect("parse");
        let emitted = emit_arrow_schema(&schema).expect("emit");
        assert_eq!(emitted["fields"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn parse_missing_fields_fails() {
        let json = serde_json::json!({ "name": "broken" });
        let result = parse_arrow_schema(&json);
        assert!(result.is_err());
    }
}
