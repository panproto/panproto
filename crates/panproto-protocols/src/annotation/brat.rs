//! brat standoff annotation protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the brat standoff annotation protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "brat".into(),
        schema_theory: "ThBratSchema".into(),
        instance_theory: "ThBratInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "text-bound".into(),
            "relation".into(),
            "event".into(),
            "attribute".into(),
            "normalization".into(),
            "note".into(),
            "equivalence".into(),
            "entity-type".into(),
            "relation-type".into(),
            "event-type".into(),
            "argument-role".into(),
            "string".into(),
            "integer".into(),
            "boolean".into(),
        ],
        constraint_sorts: vec![
            "offset-start".into(),
            "offset-end".into(),
            "value".into(),
            "type".into(),
            "source-db".into(),
            "source-id".into(),
            "text".into(),
        ],
        has_order: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for brat.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(registry, "ThBratSchema", "ThBratInstance");
}

/// Parse a JSON-based brat annotation schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
#[allow(clippy::too_many_lines)]
pub fn parse_brat(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    // Parse text-bound annotations (T annotations).
    if let Some(textbounds) = json
        .get("textbounds")
        .and_then(serde_json::Value::as_object)
    {
        for (id, def) in textbounds {
            let kind = def
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("text-bound");
            builder = builder.vertex(id, kind, None)?;

            if let Some(start) = def.get("start") {
                builder = builder.constraint(id, "offset-start", &start.to_string());
            }
            if let Some(end) = def.get("end") {
                builder = builder.constraint(id, "offset-end", &end.to_string());
            }
            if let Some(text) = def.get("text").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(id, "text", text);
            }
            if let Some(etype) = def.get("type").and_then(serde_json::Value::as_str) {
                let type_id = format!("{id}:type");
                builder = builder.vertex(&type_id, "entity-type", None)?;
                builder = builder.constraint(&type_id, "value", etype);
                builder = builder.edge(id, &type_id, "has-type", Some(etype))?;
            }
        }
    }

    // Parse relations (R annotations).
    if let Some(relations) = json.get("relations").and_then(serde_json::Value::as_object) {
        for (id, def) in relations {
            builder = builder.vertex(id, "relation", None)?;

            if let Some(rtype) = def.get("type").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(id, "type", rtype);
            }
            if let Some(args) = def.get("args").and_then(serde_json::Value::as_object) {
                for (role, target) in args {
                    if let Some(tgt) = target.as_str() {
                        let role_id = format!("{id}:role:{role}");
                        builder = builder.vertex(&role_id, "argument-role", None)?;
                        builder = builder.constraint(&role_id, "value", role);
                        builder = builder.edge(id, tgt, "rel-arg", Some(role))?;
                    }
                }
            }
        }
    }

    // Parse events (E annotations).
    if let Some(events) = json.get("events").and_then(serde_json::Value::as_object) {
        for (id, def) in events {
            builder = builder.vertex(id, "event", None)?;

            if let Some(trigger) = def.get("trigger").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, trigger, "arg", Some("trigger"))?;
            }
            if let Some(etype) = def.get("type").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(id, "type", etype);
            }
            if let Some(args) = def.get("args").and_then(serde_json::Value::as_object) {
                for (role, target) in args {
                    if let Some(tgt) = target.as_str() {
                        builder = builder.edge(id, tgt, "arg", Some(role))?;
                    }
                }
            }
        }
    }

    // Parse attributes (A/M annotations).
    if let Some(attributes) = json
        .get("attributes")
        .and_then(serde_json::Value::as_object)
    {
        for (id, def) in attributes {
            builder = builder.vertex(id, "attribute", None)?;

            if let Some(atype) = def.get("type").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(id, "type", atype);
            }
            if let Some(value) = def.get("value").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(id, "value", value);
            }
            if let Some(target) = def.get("target").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, target, "attr-of", None)?;
            }
        }
    }

    // Parse normalizations (N annotations).
    if let Some(normalizations) = json
        .get("normalizations")
        .and_then(serde_json::Value::as_object)
    {
        for (id, def) in normalizations {
            builder = builder.vertex(id, "normalization", None)?;

            if let Some(source_db) = def.get("source_db").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(id, "source-db", source_db);
            }
            if let Some(source_id) = def.get("source_id").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(id, "source-id", source_id);
            }
            if let Some(target) = def.get("target").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, target, "norm-of", None)?;
            }
        }
    }

    // Parse notes (# annotations).
    if let Some(notes) = json.get("notes").and_then(serde_json::Value::as_object) {
        for (id, def) in notes {
            builder = builder.vertex(id, "note", None)?;

            if let Some(value) = def.get("value").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(id, "value", value);
            }
            // Note target is stored as an edge, not as a constraint.
            if let Some(target) = def.get("target").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, target, "note-of", None)?;
            }
        }
    }

    // Parse equivalence sets (* annotations).
    if let Some(equivalences) = json
        .get("equivalences")
        .and_then(serde_json::Value::as_array)
    {
        for (idx, members_val) in equivalences.iter().enumerate() {
            let equiv_id = format!("Equiv{idx}");
            builder = builder.vertex(&equiv_id, "equivalence", None)?;

            if let Some(members) = members_val.as_array() {
                for member in members {
                    if let Some(tgt) = member.as_str() {
                        builder = builder.edge(&equiv_id, tgt, "equiv-member", None)?;
                    }
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON brat annotation representation.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
#[allow(clippy::too_many_lines)]
pub fn emit_brat(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &["has-type"];
    let roots = find_roots(schema, structural);

    let mut textbounds = serde_json::Map::new();
    let mut relations = serde_json::Map::new();
    let mut events = serde_json::Map::new();
    let mut attributes = serde_json::Map::new();
    let mut normalizations = serde_json::Map::new();
    let mut notes = serde_json::Map::new();
    let mut equivalences: Vec<serde_json::Value> = Vec::new();

    for root in &roots {
        let mut obj = serde_json::Map::new();
        let constraints = vertex_constraints(schema, &root.id);

        match root.kind.as_str() {
            "text-bound" => {
                obj.insert("kind".into(), serde_json::json!("text-bound"));
                for c in &constraints {
                    match c.sort.as_str() {
                        "offset-start" => {
                            obj.insert("start".into(), serde_json::json!(c.value));
                        }
                        "offset-end" => {
                            obj.insert("end".into(), serde_json::json!(c.value));
                        }
                        "text" => {
                            obj.insert("text".into(), serde_json::json!(c.value));
                        }
                        _ => {}
                    }
                }
                let type_children = children_by_edge(schema, &root.id, "has-type");
                if let Some((edge, _child)) = type_children.first() {
                    if let Some(name) = &edge.name {
                        obj.insert("type".into(), serde_json::json!(name));
                    }
                }
                textbounds.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            "relation" => {
                for c in &constraints {
                    if c.sort == "type" {
                        obj.insert("type".into(), serde_json::json!(c.value));
                    }
                }
                let rel_args = children_by_edge(schema, &root.id, "rel-arg");
                if !rel_args.is_empty() {
                    let mut args = serde_json::Map::new();
                    for (edge, child) in &rel_args {
                        let role = edge.name.as_deref().unwrap_or(&child.id);
                        args.insert(role.to_string(), serde_json::json!(child.id));
                    }
                    obj.insert("args".into(), serde_json::Value::Object(args));
                }
                relations.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            "event" => {
                for c in &constraints {
                    if c.sort == "type" {
                        obj.insert("type".into(), serde_json::json!(c.value));
                    }
                }
                let event_args = children_by_edge(schema, &root.id, "arg");
                let mut args = serde_json::Map::new();
                for (edge, child) in &event_args {
                    let role = edge.name.as_deref().unwrap_or(&child.id);
                    if role == "trigger" {
                        obj.insert("trigger".into(), serde_json::json!(child.id));
                    } else {
                        args.insert(role.to_string(), serde_json::json!(child.id));
                    }
                }
                if !args.is_empty() {
                    obj.insert("args".into(), serde_json::Value::Object(args));
                }
                events.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            "attribute" => {
                for c in &constraints {
                    match c.sort.as_str() {
                        "type" => {
                            obj.insert("type".into(), serde_json::json!(c.value));
                        }
                        "value" => {
                            obj.insert("value".into(), serde_json::json!(c.value));
                        }
                        _ => {}
                    }
                }
                let targets = children_by_edge(schema, &root.id, "attr-of");
                if let Some((_edge, child)) = targets.first() {
                    obj.insert("target".into(), serde_json::json!(child.id));
                }
                attributes.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            "normalization" => {
                for c in &constraints {
                    match c.sort.as_str() {
                        "source-db" => {
                            obj.insert("source_db".into(), serde_json::json!(c.value));
                        }
                        "source-id" => {
                            obj.insert("source_id".into(), serde_json::json!(c.value));
                        }
                        _ => {}
                    }
                }
                let targets = children_by_edge(schema, &root.id, "norm-of");
                if let Some((_edge, child)) = targets.first() {
                    obj.insert("target".into(), serde_json::json!(child.id));
                }
                normalizations.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            "note" => {
                for c in &constraints {
                    if c.sort == "value" {
                        obj.insert("value".into(), serde_json::json!(c.value));
                    }
                }
                // Target recovered from "note-of" edge.
                let targets = children_by_edge(schema, &root.id, "note-of");
                if let Some((_edge, child)) = targets.first() {
                    obj.insert("target".into(), serde_json::json!(child.id));
                }
                notes.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            "equivalence" => {
                let members = children_by_edge(schema, &root.id, "equiv-member");
                let member_ids: Vec<serde_json::Value> = members
                    .iter()
                    .map(|(_e, v)| serde_json::json!(v.id))
                    .collect();
                equivalences.push(serde_json::Value::Array(member_ids));
            }
            _ => {}
        }
    }

    let mut result = serde_json::Map::new();
    if !textbounds.is_empty() {
        result.insert("textbounds".into(), serde_json::Value::Object(textbounds));
    }
    if !relations.is_empty() {
        result.insert("relations".into(), serde_json::Value::Object(relations));
    }
    if !events.is_empty() {
        result.insert("events".into(), serde_json::Value::Object(events));
    }
    if !attributes.is_empty() {
        result.insert("attributes".into(), serde_json::Value::Object(attributes));
    }
    if !normalizations.is_empty() {
        result.insert(
            "normalizations".into(),
            serde_json::Value::Object(normalizations),
        );
    }
    if !notes.is_empty() {
        result.insert("notes".into(), serde_json::Value::Object(notes));
    }
    if !equivalences.is_empty() {
        result.insert(
            "equivalences".into(),
            serde_json::Value::Array(equivalences),
        );
    }

    Ok(serde_json::Value::Object(result))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "arg".into(),
            src_kinds: vec!["event".into()],
            tgt_kinds: vec!["text-bound".into(), "event".into()],
        },
        EdgeRule {
            edge_kind: "rel-arg".into(),
            src_kinds: vec!["relation".into()],
            tgt_kinds: vec!["text-bound".into(), "event".into()],
        },
        EdgeRule {
            edge_kind: "attr-of".into(),
            src_kinds: vec!["attribute".into()],
            tgt_kinds: vec!["text-bound".into(), "event".into()],
        },
        EdgeRule {
            edge_kind: "norm-of".into(),
            src_kinds: vec!["normalization".into()],
            tgt_kinds: vec!["text-bound".into(), "event".into()],
        },
        EdgeRule {
            edge_kind: "note-of".into(),
            src_kinds: vec!["note".into()],
            tgt_kinds: vec!["text-bound".into(), "event".into(), "relation".into()],
        },
        EdgeRule {
            edge_kind: "equiv-member".into(),
            src_kinds: vec!["equivalence".into()],
            tgt_kinds: vec!["text-bound".into(), "event".into(), "relation".into()],
        },
        EdgeRule {
            edge_kind: "has-type".into(),
            src_kinds: vec!["text-bound".into()],
            tgt_kinds: vec!["entity-type".into()],
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
        assert_eq!(p.name, "brat");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThBratSchema"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "textbounds": {
                "T1": {
                    "kind": "text-bound",
                    "type": "Protein",
                    "start": 0,
                    "end": 5
                },
                "T2": {
                    "kind": "text-bound",
                    "type": "Protein",
                    "start": 10,
                    "end": 15
                }
            },
            "events": {
                "E1": {
                    "type": "Binding",
                    "trigger": "T1",
                    "args": {
                        "Theme": "T2"
                    }
                }
            },
            "attributes": {
                "A1": {
                    "type": "Negation",
                    "target": "E1"
                }
            },
            "normalizations": {
                "N1": {
                    "source_db": "UniProt",
                    "source_id": "P12345",
                    "target": "T1"
                }
            }
        });
        let schema = parse_brat(&json).expect("should parse");
        assert!(schema.has_vertex("T1"));
        assert!(schema.has_vertex("E1"));
        assert!(schema.has_vertex("A1"));
        assert!(schema.has_vertex("N1"));
        let emitted = emit_brat(&schema).expect("emit");
        let s2 = parse_brat(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn equivalence_roundtrip() {
        let json = serde_json::json!({
            "textbounds": {
                "T1": { "kind": "text-bound", "type": "Protein", "start": 0, "end": 3 },
                "T2": { "kind": "text-bound", "type": "Protein", "start": 5, "end": 8 },
                "T3": { "kind": "text-bound", "type": "Protein", "start": 10, "end": 13 }
            },
            "equivalences": [
                ["T1", "T2", "T3"]
            ]
        });
        let schema = parse_brat(&json).expect("should parse equivalence");
        assert!(schema.has_vertex("Equiv0"));
        let emitted = emit_brat(&schema).expect("emit");
        assert!(emitted.get("equivalences").is_some());
        let s2 = parse_brat(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn note_uses_edge_not_constraint() {
        let json = serde_json::json!({
            "textbounds": {
                "T1": { "kind": "text-bound", "type": "Protein", "start": 0, "end": 5 }
            },
            "notes": {
                "X1": { "value": "This is p53 protein", "target": "T1" }
            }
        });
        let schema = parse_brat(&json).expect("should parse note");
        assert!(schema.has_vertex("X1"));
        // The target must be stored as a note-of edge, not as a "type" constraint.
        let has_note_of_edge = schema
            .outgoing_edges("X1")
            .iter()
            .any(|e| e.kind == "note-of" && e.tgt == "T1");
        assert!(has_note_of_edge, "note-of edge from X1 to T1 must exist");
        let emitted = emit_brat(&schema).expect("emit");
        let note_obj = emitted["notes"]["X1"].as_object().unwrap();
        assert_eq!(note_obj["target"].as_str().unwrap(), "T1");
        assert_eq!(note_obj["value"].as_str().unwrap(), "This is p53 protein");
        // Must not have a "type" key in the emitted note.
        assert!(
            !note_obj.contains_key("type"),
            "emitted note must not have a 'type' field"
        );
    }

    #[test]
    fn nested_event_arg() {
        // brat allows events as arguments of other events.
        let json = serde_json::json!({
            "textbounds": {
                "T1": { "kind": "text-bound", "type": "Gene", "start": 0, "end": 4 },
                "T2": { "kind": "text-bound", "type": "Gene", "start": 6, "end": 10 },
                "T3": { "kind": "text-bound", "type": "Gene", "start": 12, "end": 16 }
            },
            "events": {
                "E1": {
                    "type": "Regulation",
                    "trigger": "T1",
                    "args": { "Theme": "T2" }
                },
                "E2": {
                    "type": "Positive_regulation",
                    "trigger": "T3",
                    "args": { "Theme": "E1" }
                }
            }
        });
        let schema = parse_brat(&json).expect("should parse nested event arg");
        assert!(schema.has_vertex("E1"));
        assert!(schema.has_vertex("E2"));
        let has_nested = schema
            .outgoing_edges("E2")
            .iter()
            .any(|e| e.kind == "arg" && e.tgt == "E1");
        assert!(has_nested, "E2 must have arg edge to E1");
    }

    #[test]
    fn normalization_targets_event() {
        let json = serde_json::json!({
            "textbounds": {
                "T1": { "kind": "text-bound", "type": "Gene", "start": 0, "end": 4 }
            },
            "events": {
                "E1": { "type": "Binding", "trigger": "T1", "args": {} }
            },
            "normalizations": {
                "N1": { "source_db": "GO", "source_id": "GO:0005488", "target": "E1" }
            }
        });
        let schema = parse_brat(&json).expect("should parse norm targeting event");
        let has_edge = schema
            .outgoing_edges("N1")
            .iter()
            .any(|e| e.kind == "norm-of" && e.tgt == "E1");
        assert!(has_edge, "norm-of edge from N1 to E1 must exist");
    }

    #[test]
    fn text_constraint_roundtrip() {
        let json = serde_json::json!({
            "textbounds": {
                "T1": {
                    "kind": "text-bound",
                    "type": "Protein",
                    "start": 0,
                    "end": 3,
                    "text": "p53"
                }
            }
        });
        let schema = parse_brat(&json).expect("should parse text constraint");
        let constraints: Vec<_> = schema
            .constraints
            .get("T1")
            .map(|cs| cs.iter().collect())
            .unwrap_or_default();
        let has_text = constraints
            .iter()
            .any(|c| c.sort == "text" && c.value == "p53");
        assert!(
            has_text,
            "T1 must have a 'text' constraint with value 'p53'"
        );
        let emitted = emit_brat(&schema).expect("emit");
        assert_eq!(emitted["textbounds"]["T1"]["text"].as_str().unwrap(), "p53");
    }

    #[test]
    fn protocol_has_equivalence_kind() {
        let p = protocol();
        assert!(
            p.obj_kinds.iter().any(|k| k == "equivalence"),
            "protocol must include 'equivalence' vertex kind"
        );
    }

    #[test]
    fn protocol_has_text_constraint_sort() {
        let p = protocol();
        assert!(
            p.constraint_sorts.iter().any(|s| s == "text"),
            "protocol must include 'text' constraint sort"
        );
    }
}
