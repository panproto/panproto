//! UCCA (Universal Conceptual Cognitive Annotation) protocol definition.
//!
//! UCCA is a semantic annotation scheme based on Basic Linguistic Theory,
//! with a foundational layer providing categories like Scene, Participant,
//! Process, State, etc. Uses Group D theory: typed graph + W-type with
//! interfaces.
//!
//! # Node vs. edge categories
//!
//! In UCCA, **edge labels** carry semantic category information — not nodes.
//! A node's functional role (Participant, Process, State, etc.) is determined
//! by the category of its incoming edge from its parent. Nodes are therefore
//! generic: `node` (non-terminal) or `terminal` (leaf corresponding to a
//! surface token). Edge categories (A, P, S, D, C, E, N, R, H, L, F, G,
//! Q, U, T) are stored as the `name` field on `edge` and `remote` edges.
//!
//! # Edge kinds
//!
//! - `contains`: structural containment (passage → layer, layer → node/terminal)
//! - `edge`: primary directed edge between nodes, `name` carries the UCCA
//!   category letter (A, P, S, H, D, C, E, N, R, L, F, G, Q, U, T)
//! - `remote`: re-entrant (secondary) edge — same category vocabulary as `edge`
//! - `implicit`: edge to an implicit node (no surface realization)

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the UCCA protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "ucca".into(),
        schema_theory: "ThUccaSchema".into(),
        instance_theory: "ThUccaInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "passage".into(),
            "layer".into(),
            "node".into(),
            "terminal".into(),
            "string".into(),
            "integer".into(),
        ],
        constraint_sorts: vec![
            "id".into(),
            "tag".into(),
            "type".into(),
            "paragraph".into(),
            "is-remote".into(),
            "is-implicit".into(),
            "position".into(),
            "text".into(),
        ],
        has_order: true,
        has_coproducts: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for UCCA.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_typed_graph_wtype(registry, "ThUccaSchema", "ThUccaInstance");
}

/// Parse a JSON-based UCCA schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
#[allow(clippy::too_many_lines)]
pub fn parse_ucca(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    // Pass 1: register all vertices and constraints.

    // Parse passage.
    if let Some(passage) = json.get("passage").and_then(serde_json::Value::as_object) {
        let passage_id = passage
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("passage0");
        builder = builder.vertex(passage_id, "passage", None)?;

        if let Some(id_val) = passage.get("id").and_then(serde_json::Value::as_str) {
            builder = builder.constraint(passage_id, "id", id_val);
        }
    }

    // Parse layers (vertices + constraints only).
    if let Some(layers) = json.get("layers").and_then(serde_json::Value::as_object) {
        for (layer_id, layer_def) in layers {
            // Layers are always kind "layer" regardless of what the JSON says,
            // because layer sub-types (L0, L1) are captured via the "type"
            // constraint rather than as vertex kinds.
            builder = builder.vertex(layer_id, "layer", None)?;

            if let Some(attrs) = layer_def
                .get("attrs")
                .and_then(serde_json::Value::as_object)
            {
                for (sort, value) in attrs {
                    if let Some(v) = value.as_str() {
                        builder = builder.constraint(layer_id, sort, v);
                    }
                }
            }
        }
    }

    // Parse nodes (vertices + constraints only).
    // In UCCA all non-terminal nodes have kind "node"; terminals have kind "terminal".
    // Semantic roles come from incoming edge categories, not from the node kind.
    if let Some(nodes) = json.get("nodes").and_then(serde_json::Value::as_object) {
        for (node_id, node_def) in nodes {
            let raw_kind = node_def
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("node");
            // Normalise legacy/wrong semantic-role kinds to either "node" or
            // "terminal".  Anything already valid passes through unchanged.
            let kind = match raw_kind {
                "terminal" => "terminal",
                _ => "node",
            };
            builder = builder.vertex(node_id, kind, None)?;

            if let Some(attrs) = node_def.get("attrs").and_then(serde_json::Value::as_object) {
                for (sort, value) in attrs {
                    // Skip "category" — categories belong on edges, not nodes.
                    if sort == "category" {
                        continue;
                    }
                    if let Some(v) = value.as_str() {
                        builder = builder.constraint(node_id, sort, v);
                    }
                }
            }
        }
    }

    // Pass 2: register all edges (all vertices now exist).

    // Layer edges: connect passage → layer.
    if let Some(layers) = json.get("layers").and_then(serde_json::Value::as_object) {
        for (layer_id, _layer_def) in layers {
            if let Some(passage) = json.get("passage").and_then(serde_json::Value::as_object) {
                let passage_id = passage
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("passage0");
                builder = builder.edge(passage_id, layer_id, "contains", None)?;
            }
        }
    }

    // Node edges.
    if let Some(nodes) = json.get("nodes").and_then(serde_json::Value::as_object) {
        for (node_id, node_def) in nodes {
            // Containment: layer → node.
            if let Some(layer) = node_def.get("layer").and_then(serde_json::Value::as_str) {
                builder = builder.edge(layer, node_id, "contains", None)?;
            }

            // Primary and remote edges (node → node), with UCCA category as name.
            if let Some(edges) = node_def.get("edges").and_then(serde_json::Value::as_array) {
                for edge_def in edges {
                    if let Some(edge_obj) = edge_def.as_object() {
                        let target = edge_obj
                            .get("target")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("");
                        let category = edge_obj.get("category").and_then(serde_json::Value::as_str);

                        if !target.is_empty() {
                            let is_remote = edge_obj
                                .get("remote")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(false);

                            if is_remote {
                                builder = builder.edge(node_id, target, "remote", category)?;
                            } else {
                                builder = builder.edge(node_id, target, "edge", category)?;
                            }
                        }
                    }
                }
            }

            // Implicit edges: node → implicit node (no surface realization).
            if let Some(implicit_targets) = node_def
                .get("implicit")
                .and_then(serde_json::Value::as_array)
            {
                for imp in implicit_targets {
                    if let Some(tgt) = imp.as_str() {
                        builder = builder.edge(node_id, tgt, "implicit", None)?;
                    }
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON UCCA representation.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_ucca(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    // Only "contains" establishes the true parent→child hierarchy
    // (passage→layer→node/terminal).
    // "edge", "remote", and "implicit" are intra-layer node-to-node references.
    let structural = &["contains"];
    let roots = find_roots(schema, structural);

    let mut passage_obj = serde_json::Map::new();
    let mut layers = serde_json::Map::new();
    let mut nodes = serde_json::Map::new();

    for root in &roots {
        let constraints = vertex_constraints(schema, &root.id);

        if root.kind.as_str() == "passage" {
            for c in &constraints {
                if c.sort == "id" {
                    passage_obj.insert("id".into(), serde_json::json!(c.value));
                }
            }

            // Find layers contained by passage.
            let layer_children = children_by_edge(schema, &root.id, "contains");
            for (_edge, layer) in &layer_children {
                let mut layer_obj = serde_json::Map::new();

                let layer_constraints = vertex_constraints(schema, &layer.id);
                if !layer_constraints.is_empty() {
                    let mut attrs = serde_json::Map::new();
                    for c in &layer_constraints {
                        attrs.insert(c.sort.clone(), serde_json::json!(c.value));
                    }
                    layer_obj.insert("attrs".into(), serde_json::Value::Object(attrs));
                }

                layers.insert(layer.id.clone(), serde_json::Value::Object(layer_obj));

                // Find nodes contained by this layer.
                let node_children = children_by_edge(schema, &layer.id, "contains");
                for (_edge, node) in &node_children {
                    emit_node(schema, node, &layer.id, &mut nodes);
                }
            }
        }
    }

    let mut result = serde_json::Map::new();
    if !passage_obj.is_empty() {
        result.insert("passage".into(), serde_json::Value::Object(passage_obj));
    }
    if !layers.is_empty() {
        result.insert("layers".into(), serde_json::Value::Object(layers));
    }
    if !nodes.is_empty() {
        result.insert("nodes".into(), serde_json::Value::Object(nodes));
    }

    Ok(serde_json::Value::Object(result))
}

/// Emit a single node into the nodes map.
fn emit_node(
    schema: &Schema,
    node: &panproto_schema::Vertex,
    layer_id: &str,
    nodes: &mut serde_json::Map<String, serde_json::Value>,
) {
    let mut node_obj = serde_json::Map::new();
    node_obj.insert("kind".into(), serde_json::json!(node.kind));
    node_obj.insert("layer".into(), serde_json::json!(layer_id));

    // Emit constraints as attrs.
    let node_constraints = vertex_constraints(schema, &node.id);
    if !node_constraints.is_empty() {
        let mut attrs = serde_json::Map::new();
        for c in &node_constraints {
            attrs.insert(c.sort.clone(), serde_json::json!(c.value));
        }
        node_obj.insert("attrs".into(), serde_json::Value::Object(attrs));
    }

    // Emit primary and remote edges.  The UCCA category is stored in edge.name.
    let edge_children = children_by_edge(schema, &node.id, "edge");
    let remote_children = children_by_edge(schema, &node.id, "remote");
    if !edge_children.is_empty() || !remote_children.is_empty() {
        let mut edges = Vec::new();
        for (edge, child) in &edge_children {
            let mut edge_obj = serde_json::Map::new();
            edge_obj.insert("target".into(), serde_json::json!(child.id));
            if let Some(cat) = &edge.name {
                edge_obj.insert("category".into(), serde_json::json!(cat));
            }
            edges.push(serde_json::Value::Object(edge_obj));
        }
        for (edge, child) in &remote_children {
            let mut edge_obj = serde_json::Map::new();
            edge_obj.insert("target".into(), serde_json::json!(child.id));
            edge_obj.insert("remote".into(), serde_json::json!(true));
            if let Some(cat) = &edge.name {
                edge_obj.insert("category".into(), serde_json::json!(cat));
            }
            edges.push(serde_json::Value::Object(edge_obj));
        }
        node_obj.insert("edges".into(), serde_json::Value::Array(edges));
    }

    // Emit implicit edges.
    let implicit_children = children_by_edge(schema, &node.id, "implicit");
    if !implicit_children.is_empty() {
        let arr: Vec<serde_json::Value> = implicit_children
            .iter()
            .map(|(_, child)| serde_json::json!(child.id))
            .collect();
        node_obj.insert("implicit".into(), serde_json::Value::Array(arr));
    }

    nodes.insert(node.id.clone(), serde_json::Value::Object(node_obj));
}

#[allow(clippy::too_many_lines)]
fn edge_rules() -> Vec<EdgeRule> {
    // In UCCA, any non-terminal node can be a source for primary, remote, and
    // implicit edges.  Empty src_kinds / tgt_kinds means "any kind is allowed".
    vec![
        // Structural containment: passage → layer, layer → node/terminal.
        EdgeRule {
            edge_kind: "contains".into(),
            src_kinds: vec!["passage".into(), "layer".into()],
            tgt_kinds: vec![],
        },
        // Primary directed edge between nodes.  The UCCA category (A, P, S, H,
        // D, C, E, N, R, L, F, G, Q, U, T) is stored in edge.name.
        // Any node (including terminals as sources of sub-categorisation) may
        // participate, so src_kinds and tgt_kinds are left empty.
        EdgeRule {
            edge_kind: "edge".into(),
            src_kinds: vec![],
            tgt_kinds: vec![],
        },
        // Re-entrant (remote / secondary) edge.  Any non-terminal node can be
        // the source of a remote edge; the target can be any node or terminal.
        EdgeRule {
            edge_kind: "remote".into(),
            src_kinds: vec![],
            tgt_kinds: vec![],
        },
        // Implicit edge: points from a node to an implicit child node that has
        // no surface text realization.  Any node may have implicit children.
        EdgeRule {
            edge_kind: "implicit".into(),
            src_kinds: vec![],
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
        assert_eq!(p.name, "ucca");
        assert_eq!(p.schema_theory, "ThUccaSchema");
        assert_eq!(p.instance_theory, "ThUccaInstance");

        // Correct vertex kinds: passage, layer, node, terminal only.
        assert!(p.obj_kinds.contains(&"passage".to_string()));
        assert!(p.obj_kinds.contains(&"layer".to_string()));
        assert!(p.obj_kinds.contains(&"node".to_string()));
        assert!(p.obj_kinds.contains(&"terminal".to_string()));

        // Semantic-role names must NOT appear as vertex kinds.
        assert!(!p.obj_kinds.contains(&"participant".to_string()));
        assert!(!p.obj_kinds.contains(&"process".to_string()));
        assert!(!p.obj_kinds.contains(&"state".to_string()));
        assert!(!p.obj_kinds.contains(&"parallel-scene".to_string()));
        assert!(!p.obj_kinds.contains(&"scene".to_string()));
        assert!(!p.obj_kinds.contains(&"adverbial".to_string()));
        assert!(!p.obj_kinds.contains(&"elaborator".to_string()));
        assert!(!p.obj_kinds.contains(&"center".to_string()));
        assert!(!p.obj_kinds.contains(&"linker".to_string()));

        // Edge rules.
        assert!(p.find_edge_rule("contains").is_some());
        assert!(p.find_edge_rule("edge").is_some());
        assert!(p.find_edge_rule("remote").is_some());
        assert!(p.find_edge_rule("implicit").is_some());

        // "terminal-of" was removed (caused containment cycles).
        assert!(p.find_edge_rule("terminal-of").is_none());

        // Constraint sorts: position and text present, category absent.
        assert!(p.constraint_sorts.contains(&"position".to_string()));
        assert!(p.constraint_sorts.contains(&"text".to_string()));
        assert!(!p.constraint_sorts.contains(&"category".to_string()));
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThUccaSchema"));
        assert!(registry.contains_key("ThUccaInstance"));
    }

    #[test]
    fn parse_and_emit() {
        // Minimal UCCA graph:
        //   passage p1
        //   layer L0 (foundational)
        //   node 1.1 (scene-level node)
        //     --edge(P)--> node 1.2  (process)
        //     --edge(A)--> node 1.3  (participant)
        //     --remote(A)--> node 1.4 (re-entrant participant)
        //   terminal t1
        //
        // Semantic roles are expressed via edge categories, not node kinds.
        let json = serde_json::json!({
            "passage": {
                "id": "p1"
            },
            "layers": {
                "L0": {
                    "attrs": {
                        "type": "foundational"
                    }
                }
            },
            "nodes": {
                "1.1": {
                    "kind": "node",
                    "layer": "L0",
                    "attrs": {
                        "id": "1.1"
                    },
                    "edges": [
                        {"target": "1.2", "category": "P"},
                        {"target": "1.3", "category": "A"},
                        {"target": "1.4", "category": "A", "remote": true}
                    ]
                },
                "1.2": {
                    "kind": "node",
                    "layer": "L0",
                    "attrs": {
                        "id": "1.2"
                    }
                },
                "1.3": {
                    "kind": "node",
                    "layer": "L0",
                    "attrs": {
                        "id": "1.3"
                    }
                },
                "1.4": {
                    "kind": "node",
                    "layer": "L0",
                    "attrs": {
                        "id": "1.4"
                    }
                },
                "t1": {
                    "kind": "terminal",
                    "layer": "L0",
                    "attrs": {
                        "text": "runs",
                        "position": "1"
                    }
                }
            }
        });
        let schema = parse_ucca(&json).expect("should parse");
        assert!(schema.has_vertex("p1"));
        assert!(schema.has_vertex("L0"));
        assert!(schema.has_vertex("1.1"));
        assert!(schema.has_vertex("t1"));

        // All non-terminal nodes must have kind "node".
        assert_eq!(schema.vertices.get("1.1").unwrap().kind, "node");
        assert_eq!(schema.vertices.get("1.2").unwrap().kind, "node");
        assert_eq!(schema.vertices.get("t1").unwrap().kind, "terminal");

        // Edge categories live on edges, not nodes.
        let outgoing_1_1 = schema.outgoing_edges("1.1");
        let primary_edges: Vec<_> = outgoing_1_1.iter().filter(|e| e.kind == "edge").collect();
        assert_eq!(primary_edges.len(), 2);
        let categories: Vec<_> = primary_edges
            .iter()
            .filter_map(|e| e.name.as_deref())
            .collect();
        assert!(categories.contains(&"P"));
        assert!(categories.contains(&"A"));

        let remote_edges: Vec<_> = outgoing_1_1.iter().filter(|e| e.kind == "remote").collect();
        assert_eq!(remote_edges.len(), 1);
        assert_eq!(remote_edges[0].name.as_deref(), Some("A"));

        let emitted = emit_ucca(&schema).expect("emit");
        let s2 = parse_ucca(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
        assert_eq!(schema.edge_count(), s2.edge_count());
    }

    #[test]
    fn legacy_kind_normalised_to_node() {
        // Old data that uses semantic-role names as node kinds should be
        // normalised to "node" on parse.
        let json = serde_json::json!({
            "passage": { "id": "p2" },
            "layers": {
                "L0": { "attrs": { "type": "foundational" } }
            },
            "nodes": {
                "n1": {
                    "kind": "participant",
                    "layer": "L0"
                },
                "n2": {
                    "kind": "process",
                    "layer": "L0"
                }
            }
        });
        let schema = parse_ucca(&json).expect("should parse legacy kinds");
        assert_eq!(schema.vertices.get("n1").unwrap().kind, "node");
        assert_eq!(schema.vertices.get("n2").unwrap().kind, "node");
    }

    #[test]
    fn implicit_edge_any_node() {
        // Any node can be the source of an implicit edge.
        let json = serde_json::json!({
            "passage": { "id": "p3" },
            "layers": {
                "L0": { "attrs": {} }
            },
            "nodes": {
                "parent": {
                    "kind": "node",
                    "layer": "L0",
                    "implicit": ["child"]
                },
                "child": {
                    "kind": "node",
                    "layer": "L0"
                }
            }
        });
        let schema = parse_ucca(&json).expect("implicit edge from any node");
        let out = schema.outgoing_edges("parent");
        let implicit: Vec<_> = out.iter().filter(|e| e.kind == "implicit").collect();
        assert_eq!(implicit.len(), 1);
        assert_eq!(implicit[0].tgt, "child");
    }
}
