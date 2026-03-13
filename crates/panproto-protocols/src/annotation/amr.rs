//! AMR (Abstract Meaning Representation) protocol definition.
//!
//! Uses Group D theory: typed graph + W-type.
//!
//! AMR models sentence meaning as a rooted, directed graph. Reentrancy is
//! allowed: a variable may appear as the target of multiple edges, making the
//! structure a general directed graph (not necessarily acyclic). Vertex kinds:
//! `amr-graph`, `concept`, `frame`, `string`, `number`.
//! Core edge kinds: `instance`, `arg0`–`arg5`, `op`, `mod`, `name`, `quant`,
//! `time`, `location`, `manner`, `purpose`, `cause`, `condition`, `concession`,
//! `part-of`, `poss`, `topic`, `beneficiary`, `degree`, `duration`,
//! `frequency`, `instrument`, `medium`, `source`, `destination`, `direction`,
//! `accompanier`, `consist-of`, `example`, `polarity`, `mode`, `wiki`,
//! `domain`, `age`, `range`, `scale`, `unit`, `value`, `ord`, `subevent`,
//! `path`, `extent`, `li`, `polite`, `relation`.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the AMR protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "amr".into(),
        schema_theory: "ThAmrSchema".into(),
        instance_theory: "ThAmrInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "amr-graph".into(),
            "concept".into(),
            "frame".into(),
            "string".into(),
            "number".into(),
        ],
        constraint_sorts: vec![
            "value".into(),
            "alignment".into(),
            "wiki".into(),
            "polarity".into(),
            "mode".into(),
        ],
    }
}

/// Register the component GATs for AMR.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_typed_graph_wtype(registry, "ThAmrSchema", "ThAmrInstance");
}

/// All core AMR relation edge kinds, in the order they are emitted.
///
/// Named ARG edges are listed individually (arg0–arg5) to preserve the
/// semantic argument numbering. A generic `relation` edge kind covers any
/// non-core relation not listed here.
const RELATION_EDGES: &[&str] = &[
    "instance",
    "arg0",
    "arg1",
    "arg2",
    "arg3",
    "arg4",
    "arg5",
    "op",
    "mod",
    "name",
    "quant",
    "time",
    "location",
    "manner",
    "purpose",
    "cause",
    "condition",
    "concession",
    "part-of",
    "poss",
    "topic",
    "beneficiary",
    "degree",
    "duration",
    "frequency",
    "instrument",
    "medium",
    "source",
    "destination",
    "direction",
    "accompanier",
    "consist-of",
    "example",
    "polarity",
    "mode",
    "domain",
    "age",
    "range",
    "scale",
    "unit",
    "value",
    "ord",
    "subevent",
    "path",
    "extent",
    "li",
    "polite",
    "relation",
];

/// Parse a JSON-based AMR schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_amr_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let types = json
        .get("types")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ProtocolError::MissingField("types".into()))?;

    // First pass: create all vertices.
    for (name, def) in types {
        let kind = def
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("concept");
        builder = builder.vertex(name, kind, None)?;

        if let Some(val) = def.get("value").and_then(serde_json::Value::as_str) {
            builder = builder.constraint(name, "value", val);
        }
        if let Some(align) = def.get("alignment").and_then(serde_json::Value::as_str) {
            builder = builder.constraint(name, "alignment", align);
        }
        if let Some(wiki) = def.get("wiki").and_then(serde_json::Value::as_str) {
            builder = builder.constraint(name, "wiki", wiki);
        }
        if let Some(pol) = def.get("polarity").and_then(serde_json::Value::as_str) {
            builder = builder.constraint(name, "polarity", pol);
        }
        if let Some(mode) = def.get("mode").and_then(serde_json::Value::as_str) {
            builder = builder.constraint(name, "mode", mode);
        }
    }

    // Second pass: create edges (all targets must already exist as vertices).
    for (name, def) in types {
        for edge_kind in RELATION_EDGES {
            if let Some(targets) = def.get(*edge_kind) {
                match targets {
                    serde_json::Value::String(tgt) => {
                        if types.contains_key(tgt.as_str()) {
                            builder = builder.edge(name, tgt, edge_kind, None)?;
                        }
                    }
                    serde_json::Value::Array(arr) => {
                        for (i, item) in arr.iter().enumerate() {
                            if let Some(tgt) = item.as_str() {
                                if types.contains_key(tgt) {
                                    let label = format!("{edge_kind}.{i}");
                                    builder = builder.edge(name, tgt, edge_kind, Some(&label))?;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON AMR schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_amr_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let mut all_vertices: Vec<&panproto_schema::Vertex> = schema.vertices.values().collect();
    all_vertices.sort_by(|a, b| a.id.cmp(&b.id));

    let mut types = serde_json::Map::new();
    for vertex in &all_vertices {
        let mut obj = serde_json::Map::new();
        obj.insert("kind".into(), serde_json::json!(vertex.kind));

        // Emit constraints.
        for c in vertex_constraints(schema, &vertex.id) {
            obj.insert(c.sort.clone(), serde_json::json!(c.value));
        }

        // Emit relation edges.
        for edge_kind in RELATION_EDGES {
            let children = children_by_edge(schema, &vertex.id, edge_kind);
            match children.len() {
                0 => {}
                1 => {
                    obj.insert(
                        (*edge_kind).to_string(),
                        serde_json::json!(children[0].1.id),
                    );
                }
                _ => {
                    let arr: Vec<serde_json::Value> = children
                        .iter()
                        .map(|(_, child)| serde_json::json!(child.id))
                        .collect();
                    obj.insert((*edge_kind).to_string(), serde_json::Value::Array(arr));
                }
            }
        }

        types.insert(vertex.id.clone(), serde_json::Value::Object(obj));
    }

    Ok(serde_json::json!({ "types": types }))
}

#[allow(clippy::too_many_lines)]
fn edge_rules() -> Vec<EdgeRule> {
    // Vertex kinds that can serve as concept/frame nodes (AMR concept nodes).
    let concept_kinds: Vec<String> = vec!["concept".into(), "frame".into()];
    // Vertex kinds that can serve as any AMR node (concept, frame, or leaf value).
    let any_node: Vec<String> = vec![
        "concept".into(),
        "frame".into(),
        "string".into(),
        "number".into(),
    ];
    // Source is always a concept or frame node in AMR.
    let src_kinds: Vec<String> = vec!["concept".into(), "frame".into()];

    let mut rules = vec![
        EdgeRule {
            edge_kind: "instance".into(),
            src_kinds: vec!["concept".into(), "frame".into()],
            tgt_kinds: concept_kinds,
        },
        EdgeRule {
            edge_kind: "op".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "mod".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "name".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "quant".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "time".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "location".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "manner".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "purpose".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "cause".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "condition".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "concession".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "part-of".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "poss".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "topic".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "beneficiary".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "degree".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "duration".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "frequency".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "instrument".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "medium".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "source".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "destination".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "direction".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "accompanier".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "consist-of".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "example".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "polarity".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "mode".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "domain".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "age".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "range".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "scale".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "unit".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "value".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "ord".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "subevent".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "path".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "extent".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "li".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        EdgeRule {
            edge_kind: "polite".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
        // Generic catch-all for inverse roles, prep-*, and other non-core relations.
        EdgeRule {
            edge_kind: "relation".into(),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        },
    ];

    // Numbered ARG edges: :ARG0 through :ARG5.
    for n in 0..=5_u8 {
        rules.push(EdgeRule {
            edge_kind: format!("arg{n}"),
            src_kinds: src_kinds.clone(),
            tgt_kinds: any_node.clone(),
        });
    }

    rules
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_def() {
        let p = protocol();
        assert_eq!(p.name, "amr");
        assert_eq!(p.schema_theory, "ThAmrSchema");
        assert_eq!(p.instance_theory, "ThAmrInstance");
        // Core vertex kinds.
        assert!(p.obj_kinds.contains(&"concept".to_string()));
        assert!(p.obj_kinds.contains(&"frame".to_string()));
        assert!(p.obj_kinds.contains(&"string".to_string()));
        assert!(p.obj_kinds.contains(&"number".to_string()));
        // Hallucinated kinds must be absent.
        assert!(!p.obj_kinds.contains(&"role-set".to_string()));
        assert!(!p.obj_kinds.contains(&"boolean".to_string()));
        assert!(!p.obj_kinds.contains(&"attribute".to_string()));
        assert!(!p.obj_kinds.contains(&"instance".to_string()));
        // Numbered ARG edges.
        assert!(p.find_edge_rule("arg0").is_some());
        assert!(p.find_edge_rule("arg1").is_some());
        assert!(p.find_edge_rule("arg2").is_some());
        // Corrected relations.
        assert!(p.find_edge_rule("part-of").is_some());
        assert!(p.find_edge_rule("mod").is_some());
        assert!(p.find_edge_rule("location").is_some());
        assert!(p.find_edge_rule("time").is_some());
        assert!(p.find_edge_rule("manner").is_some());
        assert!(p.find_edge_rule("purpose").is_some());
        assert!(p.find_edge_rule("condition").is_some());
        assert!(p.find_edge_rule("poss").is_some());
        assert!(p.find_edge_rule("beneficiary").is_some());
        assert!(p.find_edge_rule("topic").is_some());
        assert!(p.find_edge_rule("degree").is_some());
        assert!(p.find_edge_rule("consist-of").is_some());
        // Hallucinated `part` (wrong) should not appear as a rule.
        assert!(p.find_edge_rule("part").is_none());
        // Constraint sorts.
        assert!(p.constraint_sorts.contains(&"polarity".to_string()));
        assert!(p.constraint_sorts.contains(&"mode".to_string()));
        assert!(!p.constraint_sorts.contains(&"frame-id".to_string()));
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThAmrSchema"));
        assert!(registry.contains_key("ThAmrInstance"));
    }

    #[test]
    fn parse_and_emit() {
        // Represents: (w / want-01 :ARG0 (b / boy))
        // AMR allows reentrancy: `b` could appear as target of multiple edges.
        let json = serde_json::json!({
            "types": {
                "g": {
                    "kind": "amr-graph"
                },
                "boy": {
                    "kind": "concept",
                    "value": "boy"
                },
                "want-01": {
                    "kind": "frame"
                },
                "b": {
                    "kind": "concept",
                    "instance": "boy",
                    "alignment": "0-1"
                },
                "w": {
                    "kind": "frame",
                    "instance": "want-01",
                    "arg0": "b"
                }
            }
        });
        let schema = parse_amr_schema(&json).expect("should parse");
        assert!(schema.has_vertex("b"));
        assert!(schema.has_vertex("w"));
        assert!(schema.has_vertex("boy"));
        assert!(schema.has_vertex("want-01"));
        assert_eq!(schema.vertices.get("b").unwrap().kind, "concept");
        assert_eq!(schema.vertices.get("w").unwrap().kind, "frame");
        let emitted = emit_amr_schema(&schema).expect("emit");
        let s2 = parse_amr_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }
}
