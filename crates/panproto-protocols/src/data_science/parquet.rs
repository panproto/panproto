//! Apache Parquet schema protocol definition.
//!
//! Parquet uses a constrained hypergraph schema theory
//! (`colimit(ThHypergraph, ThConstraint)`) and a set-valued functor
//! instance theory (`ThFunctor`).

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the Parquet protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "parquet".into(),
        schema_theory: "ThParquetSchema".into(),
        instance_theory: "ThParquetInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "message".into(),
            "field".into(),
            "group".into(),
            "boolean".into(),
            "int32".into(),
            "int64".into(),
            "int96".into(),
            "float".into(),
            "double".into(),
            "binary".into(),
            "fixed-len-byte-array".into(),
            "list".into(),
            "map".into(),
        ],
        constraint_sorts: vec![
            "repetition".into(),
            "logical-type".into(),
            "converted-type".into(),
            "field-id".into(),
        ],
    }
}

/// Register the component GATs for Parquet with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_hypergraph_functor(registry, "ThParquetSchema", "ThParquetInstance");
}

/// Parse a Parquet schema JSON into a [`Schema`].
///
/// Expects a JSON object with a `name` (message name) and `fields` array.
/// Each field has `name`, `type`, and optional `repetition`, `logicalType`,
/// `convertedType`, `fieldId`, and `fields` (for groups).
///
/// # Errors
///
/// Returns [`ProtocolError`] if the JSON is invalid.
pub fn parse_parquet_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut he_counter: usize = 0;

    let msg_name = json
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("message");

    builder = builder.vertex(msg_name, "message", None)?;

    let fields = json
        .get("fields")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ProtocolError::MissingField("fields".into()))?;

    let sig = parse_fields(&mut builder, fields, msg_name)?;

    if !sig.is_empty() {
        let he_id = format!("he_{he_counter}");
        he_counter += 1;
        builder = builder.hyper_edge(&he_id, "message", sig, msg_name)?;
    }
    let _ = he_counter;

    let schema = builder.build()?;
    Ok(schema)
}

fn parse_fields(
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

        let field_type = field
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("binary");

        let field_id = format!("{parent_id}.{field_name}");
        let kind = parquet_type_to_kind(field_type);

        // Take ownership of builder temporarily.
        let mut b = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
        b = b.vertex(&field_id, &kind, None)?;

        // Add constraints.
        if let Some(rep) = field.get("repetition").and_then(|v| v.as_str()) {
            b = b.constraint(&field_id, "repetition", rep);
        }
        if let Some(lt) = field.get("logicalType").and_then(|v| v.as_str()) {
            b = b.constraint(&field_id, "logical-type", lt);
        }
        if let Some(ct) = field.get("convertedType").and_then(|v| v.as_str()) {
            b = b.constraint(&field_id, "converted-type", ct);
        }
        if let Some(fid) = field.get("fieldId") {
            let fid_str = match fid {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                _ => fid.to_string(),
            };
            b = b.constraint(&field_id, "field-id", &fid_str);
        }

        b = b.edge(parent_id, &field_id, "prop", Some(field_name))?;
        sig.insert(field_name.to_string(), field_id.clone());

        *builder = b;

        // Recurse into nested fields (groups).
        if let Some(sub_fields) = field.get("fields").and_then(|v| v.as_array()) {
            let _ = parse_fields(builder, sub_fields, &field_id)?;
        }
    }

    Ok(sig)
}

/// Emit a [`Schema`] as Parquet schema JSON.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_parquet_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let roots: Vec<_> = find_roots(schema, &["prop"]);
    let msg = roots
        .into_iter()
        .find(|v| v.kind == "message")
        .ok_or_else(|| ProtocolError::Emit("no message vertex found".into()))?;

    let fields = emit_fields(schema, &msg.id);

    Ok(serde_json::json!({
        "name": msg.id,
        "fields": fields
    }))
}

fn emit_fields(schema: &Schema, parent_id: &str) -> Vec<serde_json::Value> {
    let children = children_by_edge(schema, parent_id, "prop");
    let mut fields = Vec::new();

    for (edge, vertex) in &children {
        let field_name = edge.name.as_deref().unwrap_or(&vertex.id);
        let pq_type = kind_to_parquet_type(&vertex.kind);

        let mut field_obj = serde_json::json!({
            "name": field_name,
            "type": pq_type
        });

        if let Some(rep) = constraint_value(schema, &vertex.id, "repetition") {
            field_obj["repetition"] = serde_json::Value::String(rep.to_string());
        }
        if let Some(lt) = constraint_value(schema, &vertex.id, "logical-type") {
            field_obj["logicalType"] = serde_json::Value::String(lt.to_string());
        }
        if let Some(ct) = constraint_value(schema, &vertex.id, "converted-type") {
            field_obj["convertedType"] = serde_json::Value::String(ct.to_string());
        }
        if let Some(fid) = constraint_value(schema, &vertex.id, "field-id") {
            field_obj["fieldId"] = serde_json::Value::String(fid.to_string());
        }

        // Recurse for nested fields.
        let sub_fields = emit_fields(schema, &vertex.id);
        if !sub_fields.is_empty() {
            field_obj["fields"] = serde_json::Value::Array(sub_fields);
        }

        fields.push(field_obj);
    }

    fields
}

fn parquet_type_to_kind(type_str: &str) -> String {
    match type_str.to_uppercase().as_str() {
        "BOOLEAN" => "boolean",
        "INT32" => "int32",
        "INT64" => "int64",
        "INT96" => "int96",
        "FLOAT" => "float",
        "DOUBLE" => "double",
        "FIXED_LEN_BYTE_ARRAY" => "fixed-len-byte-array",
        "GROUP" => "group",
        "LIST" => "list",
        "MAP" => "map",
        "MESSAGE" => "message",
        _ => "binary",
    }
    .into()
}

#[allow(clippy::match_same_arms)]
fn kind_to_parquet_type(kind: &str) -> &'static str {
    match kind {
        "boolean" => "BOOLEAN",
        "int32" => "INT32",
        "int64" => "INT64",
        "int96" => "INT96",
        "float" => "FLOAT",
        "double" => "DOUBLE",
        "fixed-len-byte-array" => "FIXED_LEN_BYTE_ARRAY",
        "group" => "GROUP",
        "list" => "LIST",
        "map" => "MAP",
        "message" => "MESSAGE",
        _ => "BINARY",
    }
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![EdgeRule {
        edge_kind: "prop".into(),
        src_kinds: vec!["message".into(), "group".into()],
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
        assert_eq!(p.name, "parquet");
        assert_eq!(p.schema_theory, "ThParquetSchema");
        assert!(p.find_edge_rule("prop").is_some());
    }

    #[test]
    fn register_theories_adds_correct_theories() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThParquetSchema"));
        assert!(registry.contains_key("ThParquetInstance"));
    }

    #[test]
    fn parse_simple_schema() {
        let json = serde_json::json!({
            "name": "event",
            "fields": [
                { "name": "id", "type": "INT64", "repetition": "required" },
                { "name": "name", "type": "BINARY", "repetition": "optional", "logicalType": "STRING" },
                { "name": "score", "type": "DOUBLE", "repetition": "required" }
            ]
        });
        let schema = parse_parquet_schema(&json).expect("should parse");
        assert!(schema.has_vertex("event"));
        assert!(schema.has_vertex("event.id"));
        assert_eq!(schema.vertices.get("event.id").unwrap().kind, "int64");
        assert_eq!(schema.vertices.get("event.name").unwrap().kind, "binary");
    }

    #[test]
    fn parse_with_group() {
        let json = serde_json::json!({
            "name": "record",
            "fields": [
                {
                    "name": "address",
                    "type": "GROUP",
                    "repetition": "optional",
                    "fields": [
                        { "name": "street", "type": "BINARY", "logicalType": "STRING" },
                        { "name": "zip", "type": "INT32" }
                    ]
                }
            ]
        });
        let schema = parse_parquet_schema(&json).expect("should parse");
        assert!(schema.has_vertex("record.address"));
        assert!(schema.has_vertex("record.address.street"));
    }

    #[test]
    fn emit_roundtrip() {
        let json = serde_json::json!({
            "name": "msg",
            "fields": [
                { "name": "x", "type": "INT32", "repetition": "required" },
                { "name": "y", "type": "FLOAT" }
            ]
        });
        let schema = parse_parquet_schema(&json).expect("parse");
        let emitted = emit_parquet_schema(&schema).expect("emit");
        assert_eq!(emitted["name"], "msg");
        assert_eq!(emitted["fields"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn parse_missing_fields_fails() {
        let json = serde_json::json!({ "name": "broken" });
        let result = parse_parquet_schema(&json);
        assert!(result.is_err());
    }
}
