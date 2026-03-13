//! ISO-Space (ISO 24617-7) spatial annotation protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the ISO-Space protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "iso-space".into(),
        schema_theory: "ThIsoSpaceSchema".into(),
        instance_theory: "ThIsoSpaceInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "document".into(),
            "place".into(),
            "path".into(),
            "spatial-entity".into(),
            "non-motion-event".into(),
            "motion".into(),
            "motion-signal".into(),
            "spatial-signal".into(),
            "measure".into(),
            "spatial-relation".into(),
            "q-s-link".into(),
            "o-link".into(),
            "m-link".into(),
            "measure-link".into(),
            "metalink".into(),
            "string".into(),
            "integer".into(),
            "float".into(),
        ],
        constraint_sorts: vec![
            "type".into(),
            "form".into(),
            "mod".into(),
            "countable".into(),
            "dimensionality".into(),
            "gquant".into(),
            "scopes".into(),
            "dcl".into(),
            "domain".into(),
            "lat-long".into(),
            "elevation".into(),
            "value".into(),
            "direction".into(),
            "distance".into(),
            "gazref".into(),
        ],
    }
}

/// Register the component GATs for ISO-Space.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThIsoSpaceSchema",
        "ThIsoSpaceInstance",
    );
}

/// Parse a JSON-based ISO-Space annotation into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
#[allow(clippy::too_many_lines)]
pub fn parse_iso_space(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    // Pass 1: register all vertices and constraints.

    // Parse spatial entities (places, paths, spatial-entities, etc.).
    if let Some(entities) = json.get("entities").and_then(serde_json::Value::as_object) {
        for (id, def) in entities {
            let kind = def
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("spatial-entity");
            builder = builder.vertex(id, kind, None)?;

            if let Some(constraints) =
                def.get("constraints").and_then(serde_json::Value::as_object)
            {
                for (sort, val) in constraints {
                    if let Some(v) = val.as_str() {
                        builder = builder.constraint(id, sort, v);
                    }
                }
            }
        }
    }

    // Parse motions (vertices + constraints only).
    if let Some(motions) = json.get("motions").and_then(serde_json::Value::as_object) {
        for (id, def) in motions {
            builder = builder.vertex(id, "motion", None)?;

            if let Some(constraints) =
                def.get("constraints").and_then(serde_json::Value::as_object)
            {
                for (sort, val) in constraints {
                    if let Some(v) = val.as_str() {
                        builder = builder.constraint(id, sort, v);
                    }
                }
            }
        }
    }

    // Parse signals (spatial-signal, motion-signal).
    if let Some(signals) = json.get("signals").and_then(serde_json::Value::as_object) {
        for (id, def) in signals {
            let kind = def
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("spatial-signal");
            builder = builder.vertex(id, kind, None)?;

            if let Some(constraints) =
                def.get("constraints").and_then(serde_json::Value::as_object)
            {
                for (sort, val) in constraints {
                    if let Some(v) = val.as_str() {
                        builder = builder.constraint(id, sort, v);
                    }
                }
            }
        }
    }

    // Parse spatial relations (vertices + constraints only).
    if let Some(relations) = json.get("relations").and_then(serde_json::Value::as_object) {
        for (id, def) in relations {
            let kind = def
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("spatial-relation");
            builder = builder.vertex(id, kind, None)?;

            if let Some(constraints) =
                def.get("constraints").and_then(serde_json::Value::as_object)
            {
                for (sort, val) in constraints {
                    if let Some(v) = val.as_str() {
                        builder = builder.constraint(id, sort, v);
                    }
                }
            }
        }
    }

    // Pass 2: register all edges (all vertices now exist).

    // Motion edges.
    if let Some(motions) = json.get("motions").and_then(serde_json::Value::as_object) {
        for (id, def) in motions {
            if let Some(mover) = def.get("mover").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, mover, "mover", None)?;
            }
            if let Some(goal) = def.get("goal").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, goal, "goal", None)?;
            }
            if let Some(source) = def.get("source").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, source, "source", None)?;
            }
            if let Some(path) = def.get("path").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, path, "path", None)?;
            }
        }
    }

    // Relation edges.
    if let Some(relations) = json.get("relations").and_then(serde_json::Value::as_object) {
        for (id, def) in relations {
            if let Some(figure) = def.get("figure").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, figure, "figure", None)?;
            }
            if let Some(ground) = def.get("ground").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, ground, "ground", None)?;
            }
            if let Some(trigger) = def.get("trigger").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, trigger, "trigger", None)?;
            }
        }
    }

    // Signal-of links (motion-signal→motion).
    if let Some(signal_links) = json.get("signal_links").and_then(serde_json::Value::as_object) {
        for (id, def) in signal_links {
            if let (Some(signal), Some(target)) = (
                def.get("signal").and_then(serde_json::Value::as_str),
                def.get("target").and_then(serde_json::Value::as_str),
            ) {
                builder = builder.edge(signal, target, "signal-of", Some(id))?;
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON ISO-Space annotation representation.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_iso_space(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    // All cross-reference edges point between independent top-level entries,
    // so no edge kind causes a vertex to be excluded from roots.
    let structural: &[&str] = &[];
    let roots = find_roots(schema, structural);

    let mut entities = serde_json::Map::new();
    let mut motions = serde_json::Map::new();
    let mut signals = serde_json::Map::new();
    let mut relations = serde_json::Map::new();

    for root in &roots {
        let mut obj = serde_json::Map::new();
        let constraints = vertex_constraints(schema, &root.id);

        match root.kind.as_str() {
            "place" | "path" | "spatial-entity" | "non-motion-event" | "measure" | "document" => {
                obj.insert("kind".into(), serde_json::json!(root.kind));
                if !constraints.is_empty() {
                    let mut cs = serde_json::Map::new();
                    for c in &constraints {
                        cs.insert(c.sort.clone(), serde_json::json!(c.value));
                    }
                    obj.insert("constraints".into(), serde_json::Value::Object(cs));
                }
                entities.insert(root.id.clone(), serde_json::Value::Object(obj));
            }
            "motion" => {
                if !constraints.is_empty() {
                    let mut cs = serde_json::Map::new();
                    for c in &constraints {
                        cs.insert(c.sort.clone(), serde_json::json!(c.value));
                    }
                    obj.insert("constraints".into(), serde_json::Value::Object(cs));
                }
                let movers = children_by_edge(schema, &root.id, "mover");
                if let Some((_edge, child)) = movers.first() {
                    obj.insert("mover".into(), serde_json::json!(child.id));
                }
                let goals = children_by_edge(schema, &root.id, "goal");
                if let Some((_edge, child)) = goals.first() {
                    obj.insert("goal".into(), serde_json::json!(child.id));
                }
                let sources = children_by_edge(schema, &root.id, "source");
                if let Some((_edge, child)) = sources.first() {
                    obj.insert("source".into(), serde_json::json!(child.id));
                }
                let paths = children_by_edge(schema, &root.id, "path");
                if let Some((_edge, child)) = paths.first() {
                    obj.insert("path".into(), serde_json::json!(child.id));
                }
                motions.insert(root.id.clone(), serde_json::Value::Object(obj));
            }
            "spatial-signal" | "motion-signal" => {
                obj.insert("kind".into(), serde_json::json!(root.kind));
                if !constraints.is_empty() {
                    let mut cs = serde_json::Map::new();
                    for c in &constraints {
                        cs.insert(c.sort.clone(), serde_json::json!(c.value));
                    }
                    obj.insert("constraints".into(), serde_json::Value::Object(cs));
                }
                signals.insert(root.id.clone(), serde_json::Value::Object(obj));
            }
            "spatial-relation" | "q-s-link" | "o-link" | "m-link" | "measure-link" | "metalink" => {
                obj.insert("kind".into(), serde_json::json!(root.kind));
                if !constraints.is_empty() {
                    let mut cs = serde_json::Map::new();
                    for c in &constraints {
                        cs.insert(c.sort.clone(), serde_json::json!(c.value));
                    }
                    obj.insert("constraints".into(), serde_json::Value::Object(cs));
                }
                let figures = children_by_edge(schema, &root.id, "figure");
                if let Some((_edge, child)) = figures.first() {
                    obj.insert("figure".into(), serde_json::json!(child.id));
                }
                let grounds = children_by_edge(schema, &root.id, "ground");
                if let Some((_edge, child)) = grounds.first() {
                    obj.insert("ground".into(), serde_json::json!(child.id));
                }
                let triggers = children_by_edge(schema, &root.id, "trigger");
                if let Some((_edge, child)) = triggers.first() {
                    obj.insert("trigger".into(), serde_json::json!(child.id));
                }
                relations.insert(root.id.clone(), serde_json::Value::Object(obj));
            }
            _ => {}
        }
    }

    let mut result = serde_json::Map::new();
    if !entities.is_empty() {
        result.insert("entities".into(), serde_json::Value::Object(entities));
    }
    if !motions.is_empty() {
        result.insert("motions".into(), serde_json::Value::Object(motions));
    }
    if !signals.is_empty() {
        result.insert("signals".into(), serde_json::Value::Object(signals));
    }
    if !relations.is_empty() {
        result.insert("relations".into(), serde_json::Value::Object(relations));
    }

    Ok(serde_json::Value::Object(result))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "trigger".into(),
            src_kinds: vec![
                "spatial-relation".into(),
                "q-s-link".into(),
                "o-link".into(),
                "m-link".into(),
                "measure-link".into(),
                "metalink".into(),
            ],
            tgt_kinds: vec!["spatial-signal".into()],
        },
        EdgeRule {
            edge_kind: "figure".into(),
            src_kinds: vec![
                "spatial-relation".into(),
                "q-s-link".into(),
                "o-link".into(),
                "m-link".into(),
                "measure-link".into(),
            ],
            tgt_kinds: vec![
                "spatial-entity".into(),
                "place".into(),
                "path".into(),
                "motion".into(),
                "non-motion-event".into(),
            ],
        },
        EdgeRule {
            edge_kind: "ground".into(),
            src_kinds: vec![
                "spatial-relation".into(),
                "q-s-link".into(),
                "o-link".into(),
                "m-link".into(),
                "measure-link".into(),
            ],
            tgt_kinds: vec!["place".into(), "path".into(), "spatial-entity".into()],
        },
        EdgeRule {
            edge_kind: "mover".into(),
            src_kinds: vec!["motion".into()],
            tgt_kinds: vec!["spatial-entity".into()],
        },
        EdgeRule {
            edge_kind: "goal".into(),
            src_kinds: vec!["motion".into()],
            tgt_kinds: vec!["place".into()],
        },
        EdgeRule {
            edge_kind: "source".into(),
            src_kinds: vec!["motion".into()],
            tgt_kinds: vec!["place".into()],
        },
        EdgeRule {
            edge_kind: "path".into(),
            src_kinds: vec!["motion".into()],
            tgt_kinds: vec!["path".into()],
        },
        EdgeRule {
            edge_kind: "signal-of".into(),
            src_kinds: vec!["motion-signal".into()],
            tgt_kinds: vec!["motion".into()],
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
        assert_eq!(p.name, "iso-space");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThIsoSpaceSchema"));
        assert!(registry.contains_key("ThIsoSpaceInstance"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "entities": {
                "pl1": {
                    "kind": "place",
                    "constraints": {
                        "type": "city",
                        "form": "nom"
                    }
                },
                "se1": {
                    "kind": "spatial-entity",
                    "constraints": {
                        "dimensionality": "2d"
                    }
                }
            },
            "motions": {
                "m1": {
                    "mover": "se1",
                    "goal": "pl1",
                    "constraints": {
                        "type": "manner"
                    }
                }
            },
            "signals": {
                "ss1": {
                    "kind": "spatial-signal",
                    "constraints": {
                        "value": "in"
                    }
                }
            },
            "relations": {
                "qsl1": {
                    "kind": "q-s-link",
                    "figure": "se1",
                    "ground": "pl1",
                    "trigger": "ss1",
                    "constraints": {
                        "type": "in"
                    }
                }
            }
        });
        let schema = parse_iso_space(&json).expect("should parse");
        assert!(schema.has_vertex("pl1"));
        assert!(schema.has_vertex("se1"));
        assert!(schema.has_vertex("m1"));
        assert!(schema.has_vertex("ss1"));
        assert!(schema.has_vertex("qsl1"));
        let emitted = emit_iso_space(&schema).expect("emit");
        let s2 = parse_iso_space(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn measure_link_vertex_kind_present() {
        let p = protocol();
        assert!(
            p.obj_kinds.iter().any(|k| k == "measure-link"),
            "measure-link must be a recognized vertex kind"
        );
    }

    #[test]
    fn figure_accepts_place_and_path() {
        let p = protocol();
        let rule = p.find_edge_rule("figure").expect("figure edge rule");
        assert!(
            rule.tgt_kinds.iter().any(|k| k == "place"),
            "figure tgt_kinds must include place"
        );
        assert!(
            rule.tgt_kinds.iter().any(|k| k == "path"),
            "figure tgt_kinds must include path"
        );
    }

    #[test]
    fn motion_path_role_not_midpoint() {
        let p = protocol();
        assert!(
            p.find_edge_rule("path").is_some(),
            "path edge rule must exist for MOTION path role"
        );
        assert!(
            p.find_edge_rule("midpoint").is_none(),
            "midpoint edge rule must not exist; ISO-Space uses path"
        );
    }

    #[test]
    fn gazref_constraint_sort_present() {
        let p = protocol();
        assert!(
            p.constraint_sorts.iter().any(|s| s == "gazref"),
            "gazref constraint sort must be present"
        );
    }

    #[test]
    fn parse_motion_with_path_role() {
        let json = serde_json::json!({
            "entities": {
                "pl1": { "kind": "place" },
                "pa1": { "kind": "path" },
                "se1": { "kind": "spatial-entity" }
            },
            "motions": {
                "m1": {
                    "mover": "se1",
                    "goal": "pl1",
                    "path": "pa1",
                    "constraints": { "type": "manner" }
                }
            }
        });
        let schema = parse_iso_space(&json).expect("should parse motion with path role");
        assert!(schema.has_vertex("m1"));
        assert!(schema.has_vertex("pa1"));
        let emitted = emit_iso_space(&schema).expect("emit");
        let s2 = parse_iso_space(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn parse_measure_link() {
        let json = serde_json::json!({
            "entities": {
                "pl1": { "kind": "place" },
                "se1": { "kind": "spatial-entity", "constraints": { "gazref": "geo:Q84" } }
            },
            "signals": {
                "ss1": { "kind": "spatial-signal", "constraints": { "value": "near" } }
            },
            "relations": {
                "ml1": {
                    "kind": "measure-link",
                    "figure": "se1",
                    "ground": "pl1",
                    "trigger": "ss1",
                    "constraints": { "distance": "2km" }
                }
            }
        });
        let schema = parse_iso_space(&json).expect("should parse measure-link");
        assert!(schema.has_vertex("ml1"));
        let emitted = emit_iso_space(&schema).expect("emit");
        let s2 = parse_iso_space(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }
}
