//! vCard/iCalendar schema protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the vCard/iCal protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "vcard-ical".into(),
        schema_theory: "ThVcardIcalSchema".into(),
        instance_theory: "ThVcardIcalInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "card".into(),
            "event".into(),
            "property".into(),
            "parameter".into(),
            "text".into(),
            "date".into(),
            "datetime".into(),
            "uri".into(),
            "integer".into(),
            "float".into(),
            "boolean".into(),
            "utc-offset".into(),
            "duration".into(),
            "geo".into(),
        ],
        constraint_sorts: vec!["required".into(), "cardinality".into()],
    }
}

/// Register the component GATs for vCard/iCal.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThVcardIcalSchema",
        "ThVcardIcalInstance",
    );
}

/// Parse a JSON-based vCard/iCalendar schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_vcard_ical_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let types = json
        .get("types")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ProtocolError::MissingField("types".into()))?;

    for (name, def) in types {
        let kind = def
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("card");
        builder = builder.vertex(name, kind, None)?;

        if let Some(props) = def.get("properties").and_then(serde_json::Value::as_object) {
            for (prop_name, prop_def) in props {
                let prop_id = format!("{name}.{prop_name}");
                let prop_kind = prop_def
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("text");
                builder = builder.vertex(&prop_id, prop_kind, None)?;
                builder = builder.edge(name, &prop_id, "prop", Some(prop_name))?;

                if prop_def
                    .get("required")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                {
                    builder = builder.constraint(&prop_id, "required", "true");
                }
                if let Some(card) = prop_def
                    .get("cardinality")
                    .and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(&prop_id, "cardinality", card);
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON vCard/iCal schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_vcard_ical_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &["prop"];
    let roots = find_roots(schema, structural);

    let mut types = serde_json::Map::new();
    for root in &roots {
        let mut obj = serde_json::Map::new();
        obj.insert("kind".into(), serde_json::json!(root.kind));

        let props = children_by_edge(schema, &root.id, "prop");
        if !props.is_empty() {
            let mut props_obj = serde_json::Map::new();
            for (edge, child) in &props {
                let name = edge.name.as_deref().unwrap_or(&child.id);
                let mut single_prop = serde_json::Map::new();
                single_prop.insert("type".into(), serde_json::json!(child.kind));
                for c in vertex_constraints(schema, &child.id) {
                    if c.sort == "required" {
                        single_prop.insert("required".into(), serde_json::json!(true));
                    } else {
                        single_prop.insert(c.sort.clone(), serde_json::json!(c.value));
                    }
                }
                props_obj.insert(name.to_string(), serde_json::Value::Object(single_prop));
            }
            obj.insert("properties".into(), serde_json::Value::Object(props_obj));
        }

        types.insert(root.id.clone(), serde_json::Value::Object(obj));
    }

    Ok(serde_json::json!({ "types": types }))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![EdgeRule {
        edge_kind: "prop".into(),
        src_kinds: vec!["card".into(), "event".into()],
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
        assert_eq!(p.name, "vcard-ical");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThVcardIcalSchema"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "types": {
                "contact": {
                    "kind": "card",
                    "properties": {
                        "fn": {"type": "text", "required": true},
                        "email": {"type": "text", "cardinality": "*"}
                    }
                }
            }
        });
        let schema = parse_vcard_ical_schema(&json).expect("should parse");
        assert!(schema.has_vertex("contact"));
        let emitted = emit_vcard_ical_schema(&schema).expect("emit");
        let s2 = parse_vcard_ical_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }
}
