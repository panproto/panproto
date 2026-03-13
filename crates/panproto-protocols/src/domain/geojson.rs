//! GeoJSON schema protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the `GeoJSON` protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "geojson".into(),
        schema_theory: "ThGeoJsonSchema".into(),
        instance_theory: "ThGeoJsonInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "feature-type".into(),
            "property".into(),
            "geometry".into(),
            "point".into(),
            "multipoint".into(),
            "linestring".into(),
            "multilinestring".into(),
            "polygon".into(),
            "multipolygon".into(),
            "geometry-collection".into(),
            "string".into(),
            "number".into(),
            "boolean".into(),
            "null".into(),
            "array".into(),
            "object".into(),
        ],
        constraint_sorts: vec!["required".into()],
    }
}

/// Register the component GATs for `GeoJSON`.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThGeoJsonSchema",
        "ThGeoJsonInstance",
    );
}

/// Parse a JSON-based `GeoJSON` schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_geojson_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let features = json
        .get("featureTypes")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ProtocolError::MissingField("featureTypes".into()))?;

    for (name, def) in features {
        builder = builder.vertex(name, "feature-type", None)?;

        if let Some(geom_type) = def.get("geometry").and_then(serde_json::Value::as_str) {
            let geom_id = format!("{name}:geometry");
            let kind = geojson_geometry_kind(geom_type);
            builder = builder.vertex(&geom_id, kind, None)?;
            builder = builder.edge(name, &geom_id, "prop", Some("geometry"))?;
        }

        if let Some(props) = def.get("properties").and_then(serde_json::Value::as_object) {
            for (prop_name, prop_def) in props {
                let prop_id = format!("{name}.{prop_name}");
                let kind = prop_def
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .map_or("string", geojson_prop_kind);
                builder = builder.vertex(&prop_id, kind, None)?;
                builder = builder.edge(name, &prop_id, "prop", Some(prop_name))?;
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Map geometry type name to kind.
fn geojson_geometry_kind(geom: &str) -> &'static str {
    match geom {
        "Point" | "point" => "point",
        "MultiPoint" | "multipoint" => "multipoint",
        "LineString" | "linestring" => "linestring",
        "MultiLineString" | "multilinestring" => "multilinestring",
        "Polygon" | "polygon" => "polygon",
        "MultiPolygon" | "multipolygon" => "multipolygon",
        "GeometryCollection" | "geometrycollection" => "geometry-collection",
        _ => "geometry",
    }
}

/// Map property type to kind.
fn geojson_prop_kind(t: &str) -> &'static str {
    match t {
        "string" => "string",
        "number" | "integer" | "float" => "number",
        "boolean" => "boolean",
        "null" => "null",
        "array" => "array",
        "object" => "object",
        _ => "string",
    }
}

/// Emit a [`Schema`] as a JSON `GeoJSON` schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_geojson_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &["prop"];
    let roots = find_roots(schema, structural);

    let mut feature_types = serde_json::Map::new();
    for root in &roots {
        if root.kind != "feature-type" {
            continue;
        }
        let mut obj = serde_json::Map::new();
        let children = children_by_edge(schema, &root.id, "prop");

        let mut props_obj = serde_json::Map::new();
        for (edge, child) in &children {
            let name = edge.name.as_deref().unwrap_or(&child.id);
            if name == "geometry" {
                obj.insert("geometry".into(), serde_json::json!(child.kind));
            } else {
                props_obj.insert(name.to_string(), serde_json::json!({"type": child.kind}));
            }
        }

        if !props_obj.is_empty() {
            obj.insert("properties".into(), serde_json::Value::Object(props_obj));
        }

        feature_types.insert(root.id.clone(), serde_json::Value::Object(obj));
    }

    Ok(serde_json::json!({ "featureTypes": feature_types }))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![EdgeRule {
        edge_kind: "prop".into(),
        src_kinds: vec!["feature-type".into()],
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
        assert_eq!(p.name, "geojson");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThGeoJsonSchema"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "featureTypes": {
                "building": {
                    "geometry": "Polygon",
                    "properties": {
                        "name": {"type": "string"},
                        "height": {"type": "number"}
                    }
                }
            }
        });
        let schema = parse_geojson_schema(&json).expect("should parse");
        assert!(schema.has_vertex("building"));
        let emitted = emit_geojson_schema(&schema).expect("emit");
        let s2 = parse_geojson_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }
}
