//! TimeML (ISO 24617-1) temporal annotation protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the `TimeML` protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "timeml".into(),
        schema_theory: "ThTimeMlSchema".into(),
        instance_theory: "ThTimeMlInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "document".into(),
            "event".into(),
            "timex3".into(),
            "signal".into(),
            "tlink".into(),
            "slink".into(),
            "alink".into(),
            "makeinstance".into(),
            "string".into(),
            "integer".into(),
            "date".into(),
        ],
        constraint_sorts: vec![
            "eid".into(),
            "eiid".into(),
            "class".into(),
            "tense".into(),
            "aspect".into(),
            "polarity".into(),
            "modality".into(),
            "pos".into(),
            "type".into(),
            "value".into(),
            "mod".into(),
            "temporal-function".into(),
            "function-in-document".into(),
            "begin-point".into(),
            "end-point".into(),
            "quant".into(),
            "freq".into(),
            "rel-type".into(),
            "signal-id".into(),
            "subordinate-signal".into(),
        ],
        has_order: true,
        has_causal: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for `TimeML`.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(registry, "ThTimeMlSchema", "ThTimeMlInstance");
}

/// Parse a JSON-based `TimeML` annotation into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
#[allow(clippy::too_many_lines)]
pub fn parse_timeml(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    // Pass 1: register all vertices and constraints.

    // Parse events.
    if let Some(events) = json.get("events").and_then(serde_json::Value::as_object) {
        for (id, def) in events {
            builder = builder.vertex(id, "event", None)?;

            if let Some(constraints) = def
                .get("constraints")
                .and_then(serde_json::Value::as_object)
            {
                for (sort, val) in constraints {
                    if let Some(v) = val.as_str() {
                        builder = builder.constraint(id, sort, v);
                    }
                }
            }
        }
    }

    // Parse timex3 expressions.
    if let Some(timexes) = json.get("timex3s").and_then(serde_json::Value::as_object) {
        for (id, def) in timexes {
            builder = builder.vertex(id, "timex3", None)?;

            if let Some(constraints) = def
                .get("constraints")
                .and_then(serde_json::Value::as_object)
            {
                for (sort, val) in constraints {
                    if let Some(v) = val.as_str() {
                        builder = builder.constraint(id, sort, v);
                    }
                }
            }
        }
    }

    // Parse signals.
    if let Some(signals) = json.get("signals").and_then(serde_json::Value::as_object) {
        for (id, def) in signals {
            builder = builder.vertex(id, "signal", None)?;

            if let Some(constraints) = def
                .get("constraints")
                .and_then(serde_json::Value::as_object)
            {
                for (sort, val) in constraints {
                    if let Some(v) = val.as_str() {
                        builder = builder.constraint(id, sort, v);
                    }
                }
            }
        }
    }

    // Parse tlinks (vertices + constraints only).
    if let Some(tlinks) = json.get("tlinks").and_then(serde_json::Value::as_object) {
        for (id, def) in tlinks {
            builder = builder.vertex(id, "tlink", None)?;

            if let Some(constraints) = def
                .get("constraints")
                .and_then(serde_json::Value::as_object)
            {
                for (sort, val) in constraints {
                    if let Some(v) = val.as_str() {
                        builder = builder.constraint(id, sort, v);
                    }
                }
            }
        }
    }

    // Parse slinks (vertices + constraints only).
    if let Some(slinks) = json.get("slinks").and_then(serde_json::Value::as_object) {
        for (id, def) in slinks {
            builder = builder.vertex(id, "slink", None)?;

            if let Some(constraints) = def
                .get("constraints")
                .and_then(serde_json::Value::as_object)
            {
                for (sort, val) in constraints {
                    if let Some(v) = val.as_str() {
                        builder = builder.constraint(id, sort, v);
                    }
                }
            }
        }
    }

    // Parse alinks (vertices + constraints only).
    if let Some(alinks) = json.get("alinks").and_then(serde_json::Value::as_object) {
        for (id, def) in alinks {
            builder = builder.vertex(id, "alink", None)?;

            if let Some(constraints) = def
                .get("constraints")
                .and_then(serde_json::Value::as_object)
            {
                for (sort, val) in constraints {
                    if let Some(v) = val.as_str() {
                        builder = builder.constraint(id, sort, v);
                    }
                }
            }
        }
    }

    // Parse makeinstances (vertices + constraints only).
    if let Some(instances) = json
        .get("makeinstances")
        .and_then(serde_json::Value::as_object)
    {
        for (id, def) in instances {
            builder = builder.vertex(id, "makeinstance", None)?;

            if let Some(constraints) = def
                .get("constraints")
                .and_then(serde_json::Value::as_object)
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

    // Tlink edges.
    if let Some(tlinks) = json.get("tlinks").and_then(serde_json::Value::as_object) {
        for (id, def) in tlinks {
            if let Some(source) = def.get("source").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, source, "source", None)?;
            }
            if let Some(target) = def.get("target").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, target, "target", None)?;
            }
            if let Some(signal) = def.get("signal").and_then(serde_json::Value::as_str) {
                builder = builder.edge(signal, id, "signal-of", None)?;
            }
        }
    }

    // Slink edges.
    if let Some(slinks) = json.get("slinks").and_then(serde_json::Value::as_object) {
        for (id, def) in slinks {
            if let Some(source) = def.get("source").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, source, "source", None)?;
            }
            if let Some(target) = def.get("target").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, target, "target", None)?;
            }
            if let Some(signal) = def.get("signal").and_then(serde_json::Value::as_str) {
                builder = builder.edge(signal, id, "signal-of", None)?;
            }
        }
    }

    // Alink edges.
    if let Some(alinks) = json.get("alinks").and_then(serde_json::Value::as_object) {
        for (id, def) in alinks {
            if let Some(source) = def.get("source").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, source, "source", None)?;
            }
            if let Some(target) = def.get("target").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, target, "target", None)?;
            }
            if let Some(signal) = def.get("signal").and_then(serde_json::Value::as_str) {
                builder = builder.edge(signal, id, "signal-of", None)?;
            }
        }
    }

    // Makeinstance edges.
    if let Some(instances) = json
        .get("makeinstances")
        .and_then(serde_json::Value::as_object)
    {
        for (id, def) in instances {
            if let Some(event) = def.get("event").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, event, "event-instance", None)?;
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON `TimeML` annotation representation.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
#[allow(clippy::too_many_lines)]
pub fn emit_timeml(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    // All cross-reference edges point between independent top-level entries,
    // so no edge kind causes a vertex to be excluded from roots.
    let structural: &[&str] = &[];
    let roots = find_roots(schema, structural);

    let mut events = serde_json::Map::new();
    let mut timex3s = serde_json::Map::new();
    let mut signals = serde_json::Map::new();
    let mut tlinks = serde_json::Map::new();
    let mut slinks = serde_json::Map::new();
    let mut alinks = serde_json::Map::new();
    let mut makeinstances = serde_json::Map::new();

    for root in &roots {
        let mut obj = serde_json::Map::new();
        let constraints = vertex_constraints(schema, &root.id);

        if !constraints.is_empty() {
            let mut cs = serde_json::Map::new();
            for c in &constraints {
                cs.insert(c.sort.to_string(), serde_json::json!(c.value));
            }
            obj.insert("constraints".into(), serde_json::Value::Object(cs));
        }

        match root.kind.as_str() {
            "event" => {
                events.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            "timex3" => {
                timex3s.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            "signal" => {
                signals.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            "tlink" => {
                let sources = children_by_edge(schema, &root.id, "source");
                if let Some((_edge, child)) = sources.first() {
                    obj.insert("source".into(), serde_json::json!(child.id));
                }
                let targets = children_by_edge(schema, &root.id, "target");
                if let Some((_edge, child)) = targets.first() {
                    obj.insert("target".into(), serde_json::json!(child.id));
                }
                // Find signal pointing to this tlink via signal-of.
                for v in schema.vertices.values() {
                    if v.kind == "signal" {
                        let sig_children = children_by_edge(schema, &v.id, "signal-of");
                        for (_edge, child) in &sig_children {
                            if child.id == root.id {
                                obj.insert("signal".into(), serde_json::json!(v.id));
                            }
                        }
                    }
                }
                tlinks.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            "slink" => {
                let sources = children_by_edge(schema, &root.id, "source");
                if let Some((_edge, child)) = sources.first() {
                    obj.insert("source".into(), serde_json::json!(child.id));
                }
                let targets = children_by_edge(schema, &root.id, "target");
                if let Some((_edge, child)) = targets.first() {
                    obj.insert("target".into(), serde_json::json!(child.id));
                }
                for v in schema.vertices.values() {
                    if v.kind == "signal" {
                        let sig_children = children_by_edge(schema, &v.id, "signal-of");
                        for (_edge, child) in &sig_children {
                            if child.id == root.id {
                                obj.insert("signal".into(), serde_json::json!(v.id));
                            }
                        }
                    }
                }
                slinks.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            "alink" => {
                let sources = children_by_edge(schema, &root.id, "source");
                if let Some((_edge, child)) = sources.first() {
                    obj.insert("source".into(), serde_json::json!(child.id));
                }
                let targets = children_by_edge(schema, &root.id, "target");
                if let Some((_edge, child)) = targets.first() {
                    obj.insert("target".into(), serde_json::json!(child.id));
                }
                for v in schema.vertices.values() {
                    if v.kind == "signal" {
                        let sig_children = children_by_edge(schema, &v.id, "signal-of");
                        for (_edge, child) in &sig_children {
                            if child.id == root.id {
                                obj.insert("signal".into(), serde_json::json!(v.id));
                            }
                        }
                    }
                }
                alinks.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            "makeinstance" => {
                let event_edges = children_by_edge(schema, &root.id, "event-instance");
                if let Some((_edge, child)) = event_edges.first() {
                    obj.insert("event".into(), serde_json::json!(child.id));
                }
                makeinstances.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            _ => {}
        }
    }

    let mut result = serde_json::Map::new();
    if !events.is_empty() {
        result.insert("events".into(), serde_json::Value::Object(events));
    }
    if !timex3s.is_empty() {
        result.insert("timex3s".into(), serde_json::Value::Object(timex3s));
    }
    if !signals.is_empty() {
        result.insert("signals".into(), serde_json::Value::Object(signals));
    }
    if !tlinks.is_empty() {
        result.insert("tlinks".into(), serde_json::Value::Object(tlinks));
    }
    if !slinks.is_empty() {
        result.insert("slinks".into(), serde_json::Value::Object(slinks));
    }
    if !alinks.is_empty() {
        result.insert("alinks".into(), serde_json::Value::Object(alinks));
    }
    if !makeinstances.is_empty() {
        result.insert(
            "makeinstances".into(),
            serde_json::Value::Object(makeinstances),
        );
    }

    Ok(serde_json::Value::Object(result))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "source".into(),
            src_kinds: vec!["tlink".into(), "slink".into(), "alink".into()],
            tgt_kinds: vec!["event".into(), "timex3".into(), "makeinstance".into()],
        },
        EdgeRule {
            edge_kind: "target".into(),
            src_kinds: vec!["tlink".into(), "slink".into(), "alink".into()],
            tgt_kinds: vec!["event".into(), "timex3".into(), "makeinstance".into()],
        },
        EdgeRule {
            edge_kind: "signal-of".into(),
            src_kinds: vec!["signal".into()],
            tgt_kinds: vec!["tlink".into(), "slink".into(), "alink".into()],
        },
        EdgeRule {
            edge_kind: "event-instance".into(),
            src_kinds: vec!["makeinstance".into()],
            tgt_kinds: vec!["event".into()],
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
        assert_eq!(p.name, "timeml");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThTimeMlSchema"));
        assert!(registry.contains_key("ThTimeMlInstance"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "events": {
                "e1": {
                    "constraints": {
                        "class": "OCCURRENCE",
                        "tense": "PAST",
                        "aspect": "PERFECTIVE"
                    }
                },
                "e2": {
                    "constraints": {
                        "class": "STATE"
                    }
                }
            },
            "timex3s": {
                "t1": {
                    "constraints": {
                        "type": "DATE",
                        "value": "1998-01-01"
                    }
                }
            },
            "signals": {
                "s1": {
                    "constraints": {
                        "value": "before"
                    }
                }
            },
            "tlinks": {
                "tl1": {
                    "source": "e1",
                    "target": "t1",
                    "signal": "s1",
                    "constraints": {
                        "rel-type": "BEFORE"
                    }
                }
            },
            "slinks": {
                "sl1": {
                    "source": "e1",
                    "target": "e2",
                    "constraints": {
                        "rel-type": "MODAL"
                    }
                }
            },
            "makeinstances": {
                "ei1": {
                    "event": "e1",
                    "constraints": {
                        "eiid": "ei1",
                        "tense": "PAST",
                        "aspect": "PERFECTIVE",
                        "polarity": "POS"
                    }
                }
            }
        });
        let schema = parse_timeml(&json).expect("should parse");
        assert!(schema.has_vertex("e1"));
        assert!(schema.has_vertex("e2"));
        assert!(schema.has_vertex("t1"));
        assert!(schema.has_vertex("s1"));
        assert!(schema.has_vertex("tl1"));
        assert!(schema.has_vertex("sl1"));
        assert!(schema.has_vertex("ei1"));
        let emitted = emit_timeml(&schema).expect("emit");
        let s2 = parse_timeml(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn timex3_mod_constraint_sort_present() {
        // Issue 1: TIMEX3 mod attribute (APPROX, START, MID, END, etc.) must be
        // a recognized constraint sort.
        let p = protocol();
        assert!(
            p.constraint_sorts.iter().any(|s| s == "mod"),
            "mod constraint sort must be present for TIMEX3"
        );
    }

    #[test]
    fn timex3_mod_roundtrips() {
        // Issue 1: parse and emit a TIMEX3 with a mod constraint.
        let json = serde_json::json!({
            "timex3s": {
                "t1": {
                    "constraints": {
                        "type": "DATE",
                        "value": "1998",
                        "mod": "APPROX"
                    }
                }
            }
        });
        let schema = parse_timeml(&json).expect("should parse timex3 with mod");
        assert!(schema.has_vertex("t1"));
        let emitted = emit_timeml(&schema).expect("emit");
        let t1_mod = emitted["timex3s"]["t1"]["constraints"]["mod"]
            .as_str()
            .expect("mod constraint must survive round-trip");
        assert_eq!(t1_mod, "APPROX");
    }

    #[test]
    fn alink_signal_of_wired() {
        // Issue 2: ALINKs can carry signalID; signal-of edges must be wired for alinks.
        let json = serde_json::json!({
            "events": {
                "e1": { "constraints": { "class": "OCCURRENCE" } },
                "e2": { "constraints": { "class": "OCCURRENCE" } }
            },
            "makeinstances": {
                "ei1": { "event": "e1", "constraints": { "eiid": "ei1" } },
                "ei2": { "event": "e2", "constraints": { "eiid": "ei2" } }
            },
            "signals": {
                "s1": { "constraints": { "value": "begin" } }
            },
            "alinks": {
                "al1": {
                    "source": "ei1",
                    "target": "ei2",
                    "signal": "s1",
                    "constraints": { "rel-type": "INITIATES" }
                }
            }
        });
        let schema = parse_timeml(&json).expect("should parse alink with signal");
        assert!(schema.has_vertex("al1"));
        assert!(schema.has_vertex("s1"));
        let emitted = emit_timeml(&schema).expect("emit");
        let signal_id = emitted["alinks"]["al1"]["signal"]
            .as_str()
            .expect("signal must be emitted for alink");
        assert_eq!(signal_id, "s1");
        // Round-trip.
        let s2 = parse_timeml(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn links_can_target_makeinstance() {
        // Issue 3: TLINKs reference event instances (makeinstance eiids), not raw events.
        // The source/target edge rules must accept makeinstance vertices.
        let p = protocol();
        let source_rule = p.find_edge_rule("source").expect("source edge rule");
        assert!(
            source_rule.tgt_kinds.iter().any(|k| k == "makeinstance"),
            "source tgt_kinds must include makeinstance"
        );
        let target_rule = p.find_edge_rule("target").expect("target edge rule");
        assert!(
            target_rule.tgt_kinds.iter().any(|k| k == "makeinstance"),
            "target tgt_kinds must include makeinstance"
        );
    }

    #[test]
    fn tlink_with_makeinstance_endpoints_roundtrips() {
        // Issue 3: parse a TLINK whose endpoints are makeinstance vertices.
        let json = serde_json::json!({
            "events": {
                "e1": { "constraints": { "class": "OCCURRENCE" } },
                "e2": { "constraints": { "class": "STATE" } }
            },
            "makeinstances": {
                "ei1": { "event": "e1", "constraints": { "eiid": "ei1", "tense": "PAST" } },
                "ei2": { "event": "e2", "constraints": { "eiid": "ei2", "tense": "PRESENT" } }
            },
            "tlinks": {
                "tl1": {
                    "source": "ei1",
                    "target": "ei2",
                    "constraints": { "rel-type": "BEFORE" }
                }
            }
        });
        let schema = parse_timeml(&json).expect("should parse tlink with makeinstance endpoints");
        assert!(schema.has_vertex("tl1"));
        assert!(schema.has_vertex("ei1"));
        assert!(schema.has_vertex("ei2"));
        let emitted = emit_timeml(&schema).expect("emit");
        let s2 = parse_timeml(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn signal_of_accepts_alink() {
        // Issue 2: signal-of edge rule must list alink as a valid target kind.
        let p = protocol();
        let rule = p.find_edge_rule("signal-of").expect("signal-of edge rule");
        assert!(
            rule.tgt_kinds.iter().any(|k| k == "alink"),
            "signal-of tgt_kinds must include alink"
        );
    }
}
