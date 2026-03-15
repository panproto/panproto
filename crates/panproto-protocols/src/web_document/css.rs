//! CSS property definitions protocol.
//!
//! Uses Group E theory: constrained multigraph + W-type + metadata.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the CSS protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "css".into(),
        schema_theory: "ThCssSchema".into(),
        instance_theory: "ThCssInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "property".into(),
            "value-type".into(),
            "at-rule".into(),
            "selector".into(),
            "length".into(),
            "color".into(),
            "percentage".into(),
            "number".into(),
            "string".into(),
            "keyword".into(),
            "function".into(),
            "url".into(),
            "image".into(),
            "time".into(),
            "angle".into(),
            "resolution".into(),
        ],
        constraint_sorts: vec![
            "inherited".into(),
            "initial".into(),
            "applies-to".into(),
            "computed".into(),
            "animatable".into(),
            "shorthand".into(),
        ],
        has_order: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for CSS.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_multigraph_wtype_meta(registry, "ThCssSchema", "ThCssInstance");
}

/// Parse a JSON-based CSS property schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_css_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let properties = json
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ProtocolError::MissingField("properties".into()))?;

    for (name, def) in properties {
        builder = builder.vertex(name, "property", None)?;

        for field in &[
            "inherited",
            "initial",
            "applies-to",
            "computed",
            "animatable",
            "shorthand",
        ] {
            if let Some(val) = def.get(field) {
                let val_str = match val {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => val.to_string(),
                };
                builder = builder.constraint(name, field, &val_str);
            }
        }

        // Parse value types.
        if let Some(values) = def.get("values").and_then(serde_json::Value::as_array) {
            for (i, val) in values.iter().enumerate() {
                if let Some(val_str) = val.as_str() {
                    let vt_id = format!("{name}:value{i}");
                    let vt_kind = css_value_kind(val_str);
                    builder = builder.vertex(&vt_id, vt_kind, None)?;
                    builder = builder.edge(name, &vt_id, "variant", Some(val_str))?;
                }
            }
        }

        // Parse sub-properties for shorthands.
        if let Some(sub_props) = def
            .get("sub-properties")
            .and_then(serde_json::Value::as_array)
        {
            for sub in sub_props.iter().filter_map(serde_json::Value::as_str) {
                let sp_id = format!("{name}:sub:{sub}");
                builder = builder.vertex(&sp_id, "property", None)?;
                builder = builder.edge(name, &sp_id, "prop", Some(sub))?;
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Map CSS value type name to vertex kind.
fn css_value_kind(value: &str) -> &'static str {
    match value {
        "length" | "em" | "rem" | "px" | "vh" | "vw" => "length",
        "color" | "rgb" | "hsl" | "hex" => "color",
        "percentage" | "%" => "percentage",
        "number" | "integer" => "number",
        "string" => "string",
        "url" => "url",
        "image" => "image",
        "time" | "s" | "ms" => "time",
        "angle" | "deg" | "rad" => "angle",
        "resolution" | "dpi" | "dpcm" | "dppx" => "resolution",
        _ => "keyword",
    }
}

/// Emit a [`Schema`] as a JSON CSS schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_css_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &["prop", "variant"];
    let roots = find_roots(schema, structural);

    let mut properties = serde_json::Map::new();
    for root in &roots {
        if root.kind != "property" {
            continue;
        }
        let mut obj = serde_json::Map::new();

        for c in vertex_constraints(schema, &root.id) {
            match c.sort.as_str() {
                "inherited" | "animatable" => {
                    if let Ok(b) = c.value.parse::<bool>() {
                        obj.insert(c.sort.to_string(), serde_json::json!(b));
                    } else {
                        obj.insert(c.sort.to_string(), serde_json::json!(c.value));
                    }
                }
                _ => {
                    obj.insert(c.sort.to_string(), serde_json::json!(c.value));
                }
            }
        }

        let variants = children_by_edge(schema, &root.id, "variant");
        if !variants.is_empty() {
            let values: Vec<serde_json::Value> = variants
                .iter()
                .filter_map(|(e, _)| e.name.as_deref().map(|n| serde_json::json!(n)))
                .collect();
            obj.insert("values".into(), serde_json::Value::Array(values));
        }

        let sub_props = children_by_edge(schema, &root.id, "prop");
        if !sub_props.is_empty() {
            let subs: Vec<serde_json::Value> = sub_props
                .iter()
                .filter_map(|(e, _)| e.name.as_deref().map(|n| serde_json::json!(n)))
                .collect();
            obj.insert("sub-properties".into(), serde_json::Value::Array(subs));
        }

        properties.insert(root.id.to_string(), serde_json::Value::Object(obj));
    }

    Ok(serde_json::json!({ "properties": properties }))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["property".into()],
            tgt_kinds: vec!["property".into()],
        },
        EdgeRule {
            edge_kind: "variant".into(),
            src_kinds: vec!["property".into()],
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
        assert_eq!(p.name, "css");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThCssSchema"));
        assert!(registry.contains_key("ThCssInstance"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "properties": {
                "color": {
                    "inherited": true,
                    "initial": "canvastext",
                    "values": ["color", "inherit"]
                },
                "margin": {
                    "inherited": false,
                    "shorthand": "true",
                    "sub-properties": ["margin-top", "margin-right"]
                }
            }
        });
        let schema = parse_css_schema(&json).expect("should parse");
        assert!(schema.has_vertex("color"));
        assert!(schema.has_vertex("margin"));

        let emitted = emit_css_schema(&schema).expect("should emit");
        assert!(emitted.get("properties").is_some());
    }

    #[test]
    fn roundtrip() {
        let json = serde_json::json!({
            "properties": {
                "display": {
                    "inherited": false,
                    "initial": "inline",
                    "values": ["block", "inline", "none"]
                }
            }
        });
        let s1 = parse_css_schema(&json).expect("parse");
        let emitted = emit_css_schema(&s1).expect("emit");
        let s2 = parse_css_schema(&emitted).expect("re-parse");
        assert_eq!(s1.vertex_count(), s2.vertex_count());
    }
}
