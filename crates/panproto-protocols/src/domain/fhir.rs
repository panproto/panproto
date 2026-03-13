//! FHIR StructureDefinition protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the FHIR protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "fhir".into(),
        schema_theory: "ThFhirSchema".into(),
        instance_theory: "ThFhirInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "resource".into(),
            "element".into(),
            "backbone-element".into(),
            "extension".into(),
            "string".into(),
            "integer".into(),
            "decimal".into(),
            "boolean".into(),
            "uri".into(),
            "url".into(),
            "canonical".into(),
            "date".into(),
            "dateTime".into(),
            "instant".into(),
            "time".into(),
            "code".into(),
            "id".into(),
            "oid".into(),
            "uuid".into(),
            "markdown".into(),
            "base64Binary".into(),
            "coding".into(),
            "codeable-concept".into(),
            "reference".into(),
            "identifier".into(),
            "period".into(),
            "quantity".into(),
        ],
        constraint_sorts: vec!["min".into(), "max".into(), "binding".into()],
    }
}

/// Register the component GATs for FHIR.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(registry, "ThFhirSchema", "ThFhirInstance");
}

/// Parse a JSON-based FHIR `StructureDefinition` into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_fhir_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let resource_name = json
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ProtocolError::MissingField("name".into()))?;

    builder = builder.vertex(resource_name, "resource", None)?;

    if let Some(elements) = json.get("elements").and_then(serde_json::Value::as_object) {
        for (elem_name, elem_def) in elements {
            let elem_id = format!("{resource_name}.{elem_name}");
            let kind = elem_def
                .get("type")
                .and_then(serde_json::Value::as_str)
                .map_or("element", fhir_type_to_kind);
            builder = builder.vertex(&elem_id, kind, None)?;
            builder = builder.edge(resource_name, &elem_id, "prop", Some(elem_name))?;

            if let Some(min) = elem_def.get("min").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(&elem_id, "min", min);
            }
            if let Some(max) = elem_def.get("max").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(&elem_id, "max", max);
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Map FHIR type to vertex kind.
fn fhir_type_to_kind(type_str: &str) -> &'static str {
    match type_str {
        "string" => "string",
        "integer" | "positiveInt" | "unsignedInt" => "integer",
        "decimal" => "decimal",
        "boolean" => "boolean",
        "uri" => "uri",
        "url" => "url",
        "canonical" => "canonical",
        "date" => "date",
        "dateTime" => "dateTime",
        "instant" => "instant",
        "time" => "time",
        "code" => "code",
        "id" => "id",
        "oid" => "oid",
        "uuid" => "uuid",
        "markdown" => "markdown",
        "base64Binary" => "base64Binary",
        "Coding" => "coding",
        "CodeableConcept" => "codeable-concept",
        "Reference" => "reference",
        "Identifier" => "identifier",
        "Period" => "period",
        "Quantity" => "quantity",
        "BackboneElement" => "backbone-element",
        "Extension" => "extension",
        _ => "element",
    }
}

/// Emit a [`Schema`] as a JSON FHIR schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_fhir_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &["prop"];
    let roots = find_roots(schema, structural);

    let resource = roots
        .first()
        .ok_or_else(|| ProtocolError::Emit("no root resource".into()))?;

    let mut elements = serde_json::Map::new();
    for (edge, child) in children_by_edge(schema, &resource.id, "prop") {
        let name = edge.name.as_deref().unwrap_or(&child.id);
        let mut elem = serde_json::Map::new();
        elem.insert("type".into(), serde_json::json!(child.kind));
        for c in vertex_constraints(schema, &child.id) {
            elem.insert(c.sort.clone(), serde_json::json!(c.value));
        }
        elements.insert(name.to_string(), serde_json::Value::Object(elem));
    }

    Ok(serde_json::json!({
        "name": resource.id,
        "elements": elements
    }))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![EdgeRule {
        edge_kind: "prop".into(),
        src_kinds: vec!["resource".into(), "backbone-element".into()],
        tgt_kinds: vec![],
    }]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_def() {
        let p = protocol();
        assert_eq!(p.name, "fhir");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThFhirSchema"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "name": "Patient",
            "elements": {
                "name": {"type": "string", "min": "1", "max": "*"},
                "birthDate": {"type": "date"}
            }
        });
        let schema = parse_fhir_schema(&json).expect("should parse");
        assert!(schema.has_vertex("Patient"));
        assert!(schema.has_vertex("Patient.name"));
        let emitted = emit_fhir_schema(&schema).expect("emit");
        let s2 = parse_fhir_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }
}
