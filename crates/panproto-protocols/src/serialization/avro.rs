//! Apache Avro protocol definition.
//!
//! Avro uses a simple constrained graph schema theory
//! (`colimit(ThSimpleGraph, ThConstraint)` + `ThFlat`).
//!
//! Vertex kinds: record, field, enum, enum-symbol, array, map, union, fixed,
//!               string, int, long, float, double, boolean, bytes, null.
//! Edge kinds: field-of, type-of, variant-of.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the Avro protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "avro".into(),
        schema_theory: "ThAvroSchema".into(),
        instance_theory: "ThAvroInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "record".into(),
            "field".into(),
            "enum".into(),
            "enum-symbol".into(),
            "array".into(),
            "map".into(),
            "union".into(),
            "fixed".into(),
            "string".into(),
            "int".into(),
            "long".into(),
            "float".into(),
            "double".into(),
            "boolean".into(),
            "bytes".into(),
            "null".into(),
        ],
        constraint_sorts: vec![
            "order".into(),
            "default".into(),
            "doc".into(),
            "aliases".into(),
            "namespace".into(),
        ],
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for Avro with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_simple_graph_flat(registry, "ThAvroSchema", "ThAvroInstance");
}

/// Intermediate representation of a parsed field for two-pass resolution.
struct FieldInfo {
    field_id: String,
    type_name: String,
}

/// Parse an Avro schema (`.avsc` JSON) into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if the JSON cannot be parsed as valid Avro.
pub fn parse_avsc(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto_def = protocol();
    let mut builder = SchemaBuilder::new(&proto_def);
    let mut vertex_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut field_infos: Vec<FieldInfo> = Vec::new();

    parse_type(&mut builder, json, "", &mut vertex_ids, &mut field_infos)?;

    // Pass 2: Resolve type-of edges for fields referencing named types.
    for info in &field_infos {
        if vertex_ids.contains(&info.type_name) {
            builder = builder.edge(&info.field_id, &info.type_name, "type-of", None)?;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Parse a type definition from a JSON value.
fn parse_type(
    builder: &mut SchemaBuilder,
    value: &serde_json::Value,
    prefix: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
    field_infos: &mut Vec<FieldInfo>,
) -> Result<(), ProtocolError> {
    match value {
        serde_json::Value::Object(obj) => {
            parse_type_object(builder, obj, prefix, vertex_ids, field_infos)?;
        }
        serde_json::Value::Array(items) => {
            // Union type: parse each member.
            for item in items {
                parse_type(builder, item, prefix, vertex_ids, field_infos)?;
            }
        }
        // Primitive type references (strings) and other values are handled by the caller.
        _ => {}
    }

    Ok(())
}

/// Parse an object-type Avro definition (record, enum, array, map).
fn parse_type_object(
    builder: &mut SchemaBuilder,
    obj: &serde_json::Map<String, serde_json::Value>,
    prefix: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
    field_infos: &mut Vec<FieldInfo>,
) -> Result<(), ProtocolError> {
    let type_val = obj
        .get("type")
        .ok_or_else(|| ProtocolError::MissingField("type".into()))?;

    match type_val.as_str() {
        Some("record") => parse_record(builder, obj, prefix, vertex_ids, field_infos),
        Some("enum") => parse_avro_enum(builder, obj, prefix, vertex_ids),
        Some("array") => {
            if let Some(items) = obj.get("items") {
                parse_type(builder, items, prefix, vertex_ids, field_infos)?;
            }
            Ok(())
        }
        Some("map") => {
            if let Some(values) = obj.get("values") {
                parse_type(builder, values, prefix, vertex_ids, field_infos)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Parse an Avro record definition.
fn parse_record(
    builder: &mut SchemaBuilder,
    obj: &serde_json::Map<String, serde_json::Value>,
    prefix: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
    field_infos: &mut Vec<FieldInfo>,
) -> Result<(), ProtocolError> {
    let name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ProtocolError::MissingField("name".into()))?;

    let record_id = if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    };

    let taken = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
    let mut b = taken.vertex(&record_id, "record", None)?;
    vertex_ids.insert(record_id.clone());

    if let Some(doc) = obj.get("doc").and_then(|v| v.as_str()) {
        b = b.constraint(&record_id, "doc", doc);
    }
    if let Some(ns) = obj.get("namespace").and_then(|v| v.as_str()) {
        b = b.constraint(&record_id, "namespace", ns);
    }

    if let Some(serde_json::Value::Array(fields)) = obj.get("fields") {
        for field in fields {
            let field_obj = field
                .as_object()
                .ok_or_else(|| ProtocolError::Parse("field must be object".into()))?;
            let field_name = field_obj
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ProtocolError::MissingField("field name".into()))?;

            let field_id = format!("{record_id}.{field_name}");
            b = b.vertex(&field_id, "field", None)?;
            vertex_ids.insert(field_id.clone());
            b = b.edge(&record_id, &field_id, "field-of", Some(field_name))?;

            if let Some(default) = field_obj.get("default") {
                b = b.constraint(&field_id, "default", &default.to_string());
            }
            if let Some(doc) = field_obj.get("doc").and_then(|v| v.as_str()) {
                b = b.constraint(&field_id, "doc", doc);
            }
            if let Some(order) = field_obj.get("order").and_then(|v| v.as_str()) {
                b = b.constraint(&field_id, "order", order);
            }

            if let Some(field_type) = field_obj.get("type") {
                let type_name = avro_type_name(field_type);
                if !type_name.is_empty() {
                    field_infos.push(FieldInfo {
                        field_id: field_id.clone(),
                        type_name,
                    });
                }
                *builder = b;
                parse_type(builder, field_type, &record_id, vertex_ids, field_infos)?;
                b = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
            }
        }
    }

    *builder = b;
    Ok(())
}

/// Parse an Avro enum definition.
fn parse_avro_enum(
    builder: &mut SchemaBuilder,
    obj: &serde_json::Map<String, serde_json::Value>,
    prefix: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(), ProtocolError> {
    let name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ProtocolError::MissingField("name".into()))?;

    let enum_id = if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    };

    let taken = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
    let mut b = taken.vertex(&enum_id, "enum", None)?;
    vertex_ids.insert(enum_id.clone());

    if let Some(doc) = obj.get("doc").and_then(|v| v.as_str()) {
        b = b.constraint(&enum_id, "doc", doc);
    }

    if let Some(serde_json::Value::Array(symbols)) = obj.get("symbols") {
        for sym in symbols {
            if let Some(sym_name) = sym.as_str() {
                let sym_id = format!("{enum_id}.{sym_name}");
                b = b.vertex(&sym_id, "enum-symbol", None)?;
                vertex_ids.insert(sym_id.clone());
                b = b.edge(&enum_id, &sym_id, "variant-of", Some(sym_name))?;
            }
        }
    }

    *builder = b;
    Ok(())
}

/// Extract the type name string from an Avro type value.
fn avro_type_name(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(obj) => obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

/// Emit an Avro schema (`.avsc` JSON) from a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if the schema cannot be serialized.
pub fn emit_avsc(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let roots = find_roots(schema, &["field-of", "variant-of", "type-of"]);

    if roots.len() == 1 {
        emit_vertex(schema, &roots[0].id)
    } else {
        // Multiple roots: emit as a union (array).
        let mut items = Vec::new();
        for root in &roots {
            items.push(emit_vertex(schema, &root.id)?);
        }
        Ok(serde_json::Value::Array(items))
    }
}

/// Emit a single vertex as a JSON value.
fn emit_vertex(schema: &Schema, vertex_id: &str) -> Result<serde_json::Value, ProtocolError> {
    let vertex = schema
        .vertices
        .get(vertex_id)
        .ok_or_else(|| ProtocolError::Emit(format!("vertex not found: {vertex_id}")))?;

    match vertex.kind.as_str() {
        "record" => {
            let mut obj = serde_json::Map::new();
            obj.insert("type".into(), serde_json::Value::String("record".into()));
            obj.insert(
                "name".into(),
                serde_json::Value::String(short_name(vertex_id)),
            );

            if let Some(ns) = constraint_value(schema, vertex_id, "namespace") {
                obj.insert("namespace".into(), serde_json::Value::String(ns.into()));
            }
            if let Some(doc) = constraint_value(schema, vertex_id, "doc") {
                obj.insert("doc".into(), serde_json::Value::String(doc.into()));
            }

            let fields = children_by_edge(schema, vertex_id, "field-of");
            let mut field_arr = Vec::new();
            for (edge, field_vertex) in &fields {
                let mut field_obj = serde_json::Map::new();
                let name = edge.name.as_deref().unwrap_or(&field_vertex.id);
                field_obj.insert("name".into(), serde_json::Value::String(name.into()));

                // Determine type.
                let type_children = children_by_edge(schema, &field_vertex.id, "type-of");
                if let Some((_, type_vertex)) = type_children.first() {
                    field_obj.insert("type".into(), emit_vertex(schema, &type_vertex.id)?);
                } else {
                    field_obj.insert("type".into(), serde_json::Value::String("string".into()));
                }

                if let Some(default) = constraint_value(schema, &field_vertex.id, "default") {
                    if let Ok(parsed) = serde_json::from_str(default) {
                        field_obj.insert("default".into(), parsed);
                    }
                }
                if let Some(doc) = constraint_value(schema, &field_vertex.id, "doc") {
                    field_obj.insert("doc".into(), serde_json::Value::String(doc.into()));
                }

                field_arr.push(serde_json::Value::Object(field_obj));
            }
            obj.insert("fields".into(), serde_json::Value::Array(field_arr));

            Ok(serde_json::Value::Object(obj))
        }
        "enum" => {
            let mut obj = serde_json::Map::new();
            obj.insert("type".into(), serde_json::Value::String("enum".into()));
            obj.insert(
                "name".into(),
                serde_json::Value::String(short_name(vertex_id)),
            );

            let variants = children_by_edge(schema, vertex_id, "variant-of");
            let symbols: Vec<serde_json::Value> = variants
                .iter()
                .map(|(edge, _)| {
                    serde_json::Value::String(edge.name.as_deref().unwrap_or("UNKNOWN").to_string())
                })
                .collect();
            obj.insert("symbols".into(), serde_json::Value::Array(symbols));

            Ok(serde_json::Value::Object(obj))
        }
        kind => Ok(serde_json::Value::String(kind.into())),
    }
}

/// Extract the short name from a dotted vertex ID.
fn short_name(vertex_id: &str) -> String {
    vertex_id
        .rsplit('.')
        .next()
        .unwrap_or(vertex_id)
        .to_string()
}

/// Well-formedness rules for Avro edges.
fn edge_rules() -> Vec<EdgeRule> {
    let all_types: Vec<String> = vec![
        "field", "record", "enum", "array", "map", "union", "fixed", "string", "int", "long",
        "float", "double", "boolean", "bytes", "null",
    ]
    .into_iter()
    .map(Into::into)
    .collect();

    vec![
        EdgeRule {
            edge_kind: "field-of".into(),
            src_kinds: vec!["record".into()],
            tgt_kinds: vec!["field".into()],
        },
        EdgeRule {
            edge_kind: "type-of".into(),
            src_kinds: vec!["field".into()],
            tgt_kinds: all_types,
        },
        EdgeRule {
            edge_kind: "variant-of".into(),
            src_kinds: vec!["enum".into()],
            tgt_kinds: vec!["enum-symbol".into()],
        },
    ]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_creates_valid_definition() {
        let p = protocol();
        assert_eq!(p.name, "avro");
        assert_eq!(p.schema_theory, "ThAvroSchema");
        assert_eq!(p.instance_theory, "ThAvroInstance");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThAvroSchema"));
        assert!(registry.contains_key("ThAvroInstance"));
    }

    #[test]
    fn parse_minimal() {
        let json: serde_json::Value = serde_json::json!({
            "type": "record",
            "name": "Person",
            "fields": [
                {"name": "name", "type": "string"},
                {"name": "age", "type": "int"}
            ]
        });

        let schema = parse_avsc(&json).expect("should parse");
        assert!(schema.has_vertex("Person"));
        assert!(schema.has_vertex("Person.name"));
        assert!(schema.has_vertex("Person.age"));
    }

    #[test]
    fn emit_minimal() {
        let json: serde_json::Value = serde_json::json!({
            "type": "record",
            "name": "Person",
            "fields": [
                {"name": "name", "type": "string"},
                {"name": "age", "type": "int"}
            ]
        });

        let schema = parse_avsc(&json).expect("should parse");
        let emitted = emit_avsc(&schema).expect("should emit");
        assert!(emitted.is_object());
        let obj = emitted.as_object().unwrap();
        assert_eq!(obj.get("type").unwrap(), "record");
        assert_eq!(obj.get("name").unwrap(), "Person");
    }

    #[test]
    fn roundtrip() {
        let json: serde_json::Value = serde_json::json!({
            "type": "record",
            "name": "Event",
            "fields": [
                {"name": "id", "type": "string"},
                {"name": "count", "type": "int"}
            ]
        });

        let schema1 = parse_avsc(&json).expect("parse 1");
        let emitted = emit_avsc(&schema1).expect("emit");
        let schema2 = parse_avsc(&emitted).expect("parse 2");

        assert_eq!(schema1.vertex_count(), schema2.vertex_count());
        assert!(schema2.has_vertex("Event"));
        assert!(schema2.has_vertex("Event.id"));
        assert!(schema2.has_vertex("Event.count"));
    }
}
