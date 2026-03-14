//! EDI X12 schema protocol definition.
//!
//! EDI X12 uses a constrained hypergraph schema theory
//! (`colimit(ThHypergraph, ThConstraint)`) and a set-valued functor
//! instance theory (`ThFunctor`).

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, constraint_value, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the EDI X12 protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "edi-x12".into(),
        schema_theory: "ThEdiX12Schema".into(),
        instance_theory: "ThEdiX12Instance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "transaction-set".into(),
            "segment".into(),
            "element".into(),
            "composite".into(),
            "data-element".into(),
            "string".into(),
            "numeric".into(),
            "decimal".into(),
            "date".into(),
            "time".into(),
            "id".into(),
        ],
        constraint_sorts: vec![
            "min-length".into(),
            "max-length".into(),
            "required".into(),
            "usage".into(),
            "repeat".into(),
        ],
        has_order: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for EDI X12 with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_hypergraph_functor(registry, "ThEdiX12Schema", "ThEdiX12Instance");
}

/// Parse an EDI X12 schema JSON into a [`Schema`].
///
/// Expects a JSON object with `transactionSet` (name and code) and
/// `segments` array containing elements.
///
/// # Errors
///
/// Returns [`ProtocolError`] if the JSON is invalid.
pub fn parse_edi_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut he_counter: usize = 0;

    let ts_name = json
        .get("transactionSet")
        .and_then(|v| v.as_str())
        .or_else(|| json.get("name").and_then(|v| v.as_str()))
        .unwrap_or("transaction-set");

    builder = builder.vertex(ts_name, "transaction-set", None)?;

    let segments = json
        .get("segments")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ProtocolError::MissingField("segments".into()))?;

    let mut ts_sig = HashMap::new();

    for segment in segments {
        let seg_id_str = segment
            .get("id")
            .and_then(|v| v.as_str())
            .or_else(|| segment.get("name").and_then(|v| v.as_str()))
            .ok_or_else(|| ProtocolError::MissingField("segment id".into()))?;

        let seg_id = format!("{ts_name}.{seg_id_str}");
        builder = builder.vertex(&seg_id, "segment", None)?;
        builder = builder.edge(ts_name, &seg_id, "prop", Some(seg_id_str))?;
        ts_sig.insert(seg_id_str.to_string(), seg_id.clone());

        // Segment-level constraints.
        if let Some(usage) = segment.get("usage").and_then(|v| v.as_str()) {
            builder = builder.constraint(&seg_id, "usage", usage);
        }
        if let Some(repeat) = segment.get("repeat") {
            builder = builder.constraint(&seg_id, "repeat", &json_val_to_string(repeat));
        }

        // Parse elements within the segment.
        if let Some(elements) = segment.get("elements").and_then(|v| v.as_array()) {
            let mut seg_sig = HashMap::new();

            for element in elements {
                let elem_ref = element
                    .get("ref")
                    .and_then(|v| v.as_str())
                    .or_else(|| element.get("name").and_then(|v| v.as_str()))
                    .ok_or_else(|| ProtocolError::MissingField("element ref".into()))?;

                let elem_id = format!("{seg_id}.{elem_ref}");
                let has_components = element
                    .get("components")
                    .and_then(|v| v.as_array())
                    .is_some_and(|a| !a.is_empty());
                let kind = if has_components {
                    "composite".to_string()
                } else {
                    let elem_type = element.get("type").and_then(|v| v.as_str()).unwrap_or("AN");
                    edi_type_to_kind(elem_type)
                };

                builder = builder.vertex(&elem_id, &kind, None)?;
                builder = builder.edge(&seg_id, &elem_id, "prop", Some(elem_ref))?;
                seg_sig.insert(elem_ref.to_string(), elem_id.clone());

                // Element constraints.
                if let Some(min_len) = element.get("minLength") {
                    builder =
                        builder.constraint(&elem_id, "min-length", &json_val_to_string(min_len));
                }
                if let Some(max_len) = element.get("maxLength") {
                    builder =
                        builder.constraint(&elem_id, "max-length", &json_val_to_string(max_len));
                }
                if let Some(req) = element.get("required").and_then(serde_json::Value::as_bool) {
                    if req {
                        builder = builder.constraint(&elem_id, "required", "true");
                    }
                }
                if let Some(usage) = element.get("usage").and_then(|v| v.as_str()) {
                    builder = builder.constraint(&elem_id, "usage", usage);
                }

                // Handle composite elements.
                if let Some(components) = element.get("components").and_then(|v| v.as_array()) {
                    for comp in components {
                        let comp_ref = comp
                            .get("ref")
                            .and_then(|v| v.as_str())
                            .or_else(|| comp.get("name").and_then(|v| v.as_str()))
                            .unwrap_or("comp");
                        let comp_id = format!("{elem_id}.{comp_ref}");
                        let comp_type = comp.get("type").and_then(|v| v.as_str()).unwrap_or("AN");
                        let comp_kind = edi_type_to_kind(comp_type);
                        builder = builder.vertex(&comp_id, &comp_kind, None)?;
                        builder = builder.edge(&elem_id, &comp_id, "prop", Some(comp_ref))?;
                    }
                }
            }

            if !seg_sig.is_empty() {
                let he_id = format!("he_{he_counter}");
                he_counter += 1;
                builder = builder.hyper_edge(&he_id, "segment", seg_sig, &seg_id)?;
            }
        }
    }

    if !ts_sig.is_empty() {
        let he_id = format!("he_{he_counter}");
        he_counter += 1;
        builder = builder.hyper_edge(&he_id, "transaction-set", ts_sig, ts_name)?;
    }
    let _ = he_counter;

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as EDI X12 schema JSON.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_edi_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let roots: Vec<_> = find_roots(schema, &["prop"]);
    let ts = roots
        .into_iter()
        .find(|v| v.kind == "transaction-set")
        .ok_or_else(|| ProtocolError::Emit("no transaction-set vertex found".into()))?;

    let seg_children = children_by_edge(schema, &ts.id, "prop");
    let mut segments = Vec::new();

    for (seg_edge, seg_vertex) in &seg_children {
        let seg_name = seg_edge.name.as_deref().unwrap_or(&seg_vertex.id);

        let mut seg_obj = serde_json::json!({ "id": seg_name });

        if let Some(usage) = constraint_value(schema, &seg_vertex.id, "usage") {
            seg_obj["usage"] = serde_json::Value::String(usage.to_string());
        }
        if let Some(repeat) = constraint_value(schema, &seg_vertex.id, "repeat") {
            seg_obj["repeat"] = serde_json::Value::String(repeat.to_string());
        }

        let elem_children = children_by_edge(schema, &seg_vertex.id, "prop");
        let mut elements = Vec::new();

        for (elem_edge, elem_vertex) in &elem_children {
            let elem_name = elem_edge.name.as_deref().unwrap_or(&elem_vertex.id);
            let edi_type = kind_to_edi_type(&elem_vertex.kind);

            let mut elem_obj = serde_json::json!({
                "ref": elem_name,
                "type": edi_type
            });

            let constraints = vertex_constraints(schema, &elem_vertex.id);
            for c in &constraints {
                match c.sort.as_str() {
                    "min-length" => {
                        elem_obj["minLength"] = serde_json::Value::String(c.value.clone());
                    }
                    "max-length" => {
                        elem_obj["maxLength"] = serde_json::Value::String(c.value.clone());
                    }
                    "required" if c.value == "true" => {
                        elem_obj["required"] = serde_json::Value::Bool(true);
                    }
                    "usage" => {
                        elem_obj["usage"] = serde_json::Value::String(c.value.clone());
                    }
                    _ => {}
                }
            }

            // Emit composite components.
            let comp_children = children_by_edge(schema, &elem_vertex.id, "prop");
            if !comp_children.is_empty() {
                let mut components = Vec::new();
                for (comp_edge, comp_vertex) in &comp_children {
                    let comp_name = comp_edge.name.as_deref().unwrap_or(&comp_vertex.id);
                    let comp_type = kind_to_edi_type(&comp_vertex.kind);
                    components.push(serde_json::json!({
                        "ref": comp_name,
                        "type": comp_type
                    }));
                }
                elem_obj["components"] = serde_json::Value::Array(components);
            }

            elements.push(elem_obj);
        }

        if !elements.is_empty() {
            seg_obj["elements"] = serde_json::Value::Array(elements);
        }

        segments.push(seg_obj);
    }

    Ok(serde_json::json!({
        "transactionSet": ts.id,
        "segments": segments
    }))
}

fn edi_type_to_kind(edi_type: &str) -> String {
    match edi_type.to_uppercase().as_str() {
        "N" | "N0" | "N1" | "N2" | "NUMERIC" => "numeric",
        "R" | "DECIMAL" => "decimal",
        "DT" | "DATE" => "date",
        "TM" | "TIME" => "time",
        "ID" => "id",
        "COMPOSITE" => "composite",
        _ => "string",
    }
    .into()
}

fn kind_to_edi_type(kind: &str) -> &'static str {
    match kind {
        "numeric" => "N",
        "decimal" => "R",
        "date" => "DT",
        "time" => "TM",
        "id" => "ID",
        "composite" => "COMPOSITE",
        _ => "AN",
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
        src_kinds: vec![
            "transaction-set".into(),
            "segment".into(),
            "composite".into(),
        ],
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
        assert_eq!(p.name, "edi-x12");
        assert_eq!(p.schema_theory, "ThEdiX12Schema");
        assert!(p.find_edge_rule("prop").is_some());
    }

    #[test]
    fn register_theories_adds_correct_theories() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThEdiX12Schema"));
        assert!(registry.contains_key("ThEdiX12Instance"));
    }

    #[test]
    fn parse_simple_transaction_set() {
        let json = serde_json::json!({
            "transactionSet": "810",
            "segments": [
                {
                    "id": "BIG",
                    "usage": "mandatory",
                    "elements": [
                        { "ref": "BIG01", "type": "DT", "minLength": "8", "maxLength": "8", "required": true },
                        { "ref": "BIG02", "type": "AN", "minLength": "1", "maxLength": "22" }
                    ]
                },
                {
                    "id": "NTE",
                    "usage": "optional",
                    "elements": [
                        { "ref": "NTE01", "type": "ID", "minLength": "3", "maxLength": "3" }
                    ]
                }
            ]
        });
        let schema = parse_edi_schema(&json).expect("should parse");
        assert!(schema.has_vertex("810"));
        assert!(schema.has_vertex("810.BIG"));
        assert!(schema.has_vertex("810.BIG.BIG01"));
        assert_eq!(schema.vertices.get("810.BIG.BIG01").unwrap().kind, "date");
        assert_eq!(schema.vertices.get("810.NTE.NTE01").unwrap().kind, "id");
    }

    #[test]
    fn parse_with_composite_elements() {
        let json = serde_json::json!({
            "transactionSet": "850",
            "segments": [
                {
                    "id": "PO1",
                    "elements": [
                        {
                            "ref": "PO101",
                            "type": "AN",
                            "components": [
                                { "ref": "C001-01", "type": "AN" },
                                { "ref": "C001-02", "type": "N" }
                            ]
                        }
                    ]
                }
            ]
        });
        let schema = parse_edi_schema(&json).expect("should parse");
        assert!(schema.has_vertex("850.PO1.PO101"));
        assert!(schema.has_vertex("850.PO1.PO101.C001-01"));
    }

    #[test]
    fn emit_roundtrip() {
        let json = serde_json::json!({
            "transactionSet": "997",
            "segments": [
                {
                    "id": "AK1",
                    "elements": [
                        { "ref": "AK101", "type": "ID" },
                        { "ref": "AK102", "type": "N" }
                    ]
                }
            ]
        });
        let schema = parse_edi_schema(&json).expect("parse");
        let emitted = emit_edi_schema(&schema).expect("emit");
        assert_eq!(emitted["transactionSet"], "997");
        assert_eq!(emitted["segments"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn parse_missing_segments_fails() {
        let json = serde_json::json!({ "transactionSet": "810" });
        let result = parse_edi_schema(&json);
        assert!(result.is_err());
    }
}
