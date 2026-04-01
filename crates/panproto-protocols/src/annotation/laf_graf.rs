//! LAF/GrAF (Linguistic Annotation Framework / Graph Annotation Framework)
//! protocol definition (ISO 24612).
//!
//! Uses Group A theory: constrained multigraph + W-type. LAF is explicitly a
//! directed multigraph: multiple edges may exist between the same pair of
//! nodes, and a node may carry multiple annotations.
//!
//! Edge kinds:
//! - `node-of`: node → region (node is grounded in a region)
//! - `region-anchor`: region → anchor (region bounded by anchors)
//! - `edge-link`: edge-vertex → node (source/target endpoints)
//! - `annotates-node`: annotation → node/edge-vertex (annotation targets)
//! - `annotation-fs`: annotation → feature-structure (associated FS)
//! - `feature-of`: feature → feature-structure (membership)

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the LAF/GrAF protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "laf-graf".into(),
        schema_theory: "ThLafGrafSchema".into(),
        instance_theory: "ThLafGrafInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "annotation".into(),
            "node".into(),
            "edge".into(),
            "region".into(),
            "anchor".into(),
            "feature-structure".into(),
            "feature".into(),
            "string".into(),
            "integer".into(),
            "float".into(),
        ],
        constraint_sorts: vec![
            "id".into(),
            "type".into(),
            "value".into(),
            "from".into(),
            "to".into(),
            "label".into(),
        ],
        has_order: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for LAF/GrAF.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThLafGrafSchema",
        "ThLafGrafInstance",
    );
}

/// Parse a JSON-based LAF/GrAF annotation into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
#[allow(clippy::too_many_lines)]
pub fn parse_laf_graf(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    // Pass 1: register all vertices (and their constraints) across every
    // section before touching any edges. This allows cross-section forward
    // references (e.g. a node referencing a region that appears later in the
    // JSON) to resolve without error.

    if let Some(nodes) = json.get("nodes").and_then(serde_json::Value::as_object) {
        for (id, def) in nodes {
            builder = builder.vertex(id, "node", None)?;
            if let Some(cs) = def
                .get("constraints")
                .and_then(serde_json::Value::as_object)
            {
                for (sort, val) in cs {
                    if let Some(v) = val.as_str() {
                        builder = builder.constraint(id, sort, v);
                    }
                }
            }
        }
    }

    if let Some(regions) = json.get("regions").and_then(serde_json::Value::as_object) {
        for (id, def) in regions {
            builder = builder.vertex(id, "region", None)?;
            if let Some(cs) = def
                .get("constraints")
                .and_then(serde_json::Value::as_object)
            {
                for (sort, val) in cs {
                    if let Some(v) = val.as_str() {
                        builder = builder.constraint(id, sort, v);
                    }
                }
            }
        }
    }

    if let Some(anchors) = json.get("anchors").and_then(serde_json::Value::as_object) {
        for (id, def) in anchors {
            builder = builder.vertex(id, "anchor", None)?;
            if let Some(cs) = def
                .get("constraints")
                .and_then(serde_json::Value::as_object)
            {
                for (sort, val) in cs {
                    if let Some(v) = val.as_str() {
                        builder = builder.constraint(id, sort, v);
                    }
                }
            }
        }
    }

    if let Some(edges) = json.get("edges").and_then(serde_json::Value::as_object) {
        for (id, def) in edges {
            builder = builder.vertex(id, "edge", None)?;
            if let Some(cs) = def
                .get("constraints")
                .and_then(serde_json::Value::as_object)
            {
                for (sort, val) in cs {
                    if let Some(v) = val.as_str() {
                        builder = builder.constraint(id, sort, v);
                    }
                }
            }
        }
    }

    if let Some(annotations) = json
        .get("annotations")
        .and_then(serde_json::Value::as_object)
    {
        for (id, def) in annotations {
            builder = builder.vertex(id, "annotation", None)?;
            if let Some(cs) = def
                .get("constraints")
                .and_then(serde_json::Value::as_object)
            {
                for (sort, val) in cs {
                    if let Some(v) = val.as_str() {
                        builder = builder.constraint(id, sort, v);
                    }
                }
            }
            // Register inline feature-structure vertex and its feature vertices.
            if let Some(features) = def.get("features").and_then(serde_json::Value::as_object) {
                let fs_id = format!("{id}:fs");
                builder = builder.vertex(&fs_id, "feature-structure", None)?;
                for (fname, fval) in features {
                    let feat_id = format!("{fs_id}.{fname}");
                    builder = builder.vertex(&feat_id, "feature", None)?;
                    if let Some(v) = fval.as_str() {
                        builder = builder.constraint(&feat_id, "value", v);
                    }
                }
            }
        }
    }

    if let Some(fstructs) = json
        .get("feature_structures")
        .and_then(serde_json::Value::as_object)
    {
        for (id, def) in fstructs {
            builder = builder.vertex(id, "feature-structure", None)?;
            if let Some(cs) = def
                .get("constraints")
                .and_then(serde_json::Value::as_object)
            {
                for (sort, val) in cs {
                    if let Some(v) = val.as_str() {
                        builder = builder.constraint(id, sort, v);
                    }
                }
            }
            if let Some(features) = def.get("features").and_then(serde_json::Value::as_object) {
                for (fname, fval) in features {
                    let feat_id = format!("{id}.{fname}");
                    builder = builder.vertex(&feat_id, "feature", None)?;
                    if let Some(v) = fval.as_str() {
                        builder = builder.constraint(&feat_id, "value", v);
                    }
                }
            }
        }
    }

    // Pass 2: register all edges now that every vertex exists.

    if let Some(nodes) = json.get("nodes").and_then(serde_json::Value::as_object) {
        for (id, def) in nodes {
            if let Some(region) = def.get("region").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, region, "node-of", None)?;
            }
        }
    }

    if let Some(regions) = json.get("regions").and_then(serde_json::Value::as_object) {
        for (id, def) in regions {
            if let Some(anchors) = def.get("anchors").and_then(serde_json::Value::as_array) {
                for (i, anchor) in anchors.iter().enumerate() {
                    if let Some(anchor_id) = anchor.as_str() {
                        builder = builder.edge(
                            id,
                            anchor_id,
                            "region-anchor",
                            Some(&format!("anchor{i}")),
                        )?;
                    }
                }
            }
        }
    }

    if let Some(edges) = json.get("edges").and_then(serde_json::Value::as_object) {
        for (id, def) in edges {
            if let Some(from) = def.get("from").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, from, "edge-link", Some("from"))?;
            }
            if let Some(to) = def.get("to").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, to, "edge-link", Some("to"))?;
            }
        }
    }

    if let Some(annotations) = json
        .get("annotations")
        .and_then(serde_json::Value::as_object)
    {
        for (id, def) in annotations {
            // `annotates-node`: annotation → node or edge-vertex (the target
            // being annotated). Distinct from the feature-structure link.
            if let Some(target) = def.get("target").and_then(serde_json::Value::as_str) {
                builder = builder.edge(id, target, "annotates-node", None)?;
            }
            // `annotation-fs`: annotation → feature-structure (the associated
            // feature structure). Kept separate from `annotates-node` because
            // they represent two orthogonal relationships per ISO 24612.
            if let Some(features) = def.get("features").and_then(serde_json::Value::as_object) {
                let fs_id = format!("{id}:fs");
                builder = builder.edge(id, &fs_id, "annotation-fs", None)?;
                for (fname, _fval) in features {
                    let feat_id = format!("{fs_id}.{fname}");
                    builder = builder.edge(&feat_id, &fs_id, "feature-of", Some(fname))?;
                }
            }
        }
    }

    if let Some(fstructs) = json
        .get("feature_structures")
        .and_then(serde_json::Value::as_object)
    {
        for (id, def) in fstructs {
            if let Some(features) = def.get("features").and_then(serde_json::Value::as_object) {
                for (fname, _fval) in features {
                    let feat_id = format!("{id}.{fname}");
                    builder = builder.edge(&feat_id, id, "feature-of", Some(fname))?;
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON LAF/GrAF annotation representation.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
#[allow(clippy::too_many_lines)]
pub fn emit_laf_graf(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    // Only "feature-of" creates true nesting: feature-structure vertices are
    // always inlined and should not appear as independent top-level entries.
    // Edges like "node-of", "region-anchor", "edge-link", "annotates-node",
    // and "annotation-fs" are cross-references between independently-emitted
    // sections, so their targets must remain top-level roots.
    let structural = &["feature-of"];
    let roots = find_roots(schema, structural);

    let mut nodes = serde_json::Map::new();
    let mut regions = serde_json::Map::new();
    let mut anchors = serde_json::Map::new();
    let mut edges = serde_json::Map::new();
    let mut annotations = serde_json::Map::new();
    let mut feature_structures = serde_json::Map::new();

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
            "node" => {
                let region_edges = children_by_edge(schema, &root.id, "node-of");
                if let Some((_edge, child)) = region_edges.first() {
                    obj.insert("region".into(), serde_json::json!(child.id));
                }
                nodes.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            "region" => {
                let anchor_edges = children_by_edge(schema, &root.id, "region-anchor");
                if !anchor_edges.is_empty() {
                    let arr: Vec<serde_json::Value> = anchor_edges
                        .iter()
                        .map(|(_edge, child)| serde_json::json!(child.id))
                        .collect();
                    obj.insert("anchors".into(), serde_json::Value::Array(arr));
                }
                regions.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            "anchor" => {
                anchors.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            "edge" => {
                let links = children_by_edge(schema, &root.id, "edge-link");
                for (edge, child) in &links {
                    let name = edge.name.as_deref().unwrap_or("");
                    match name {
                        "from" => {
                            obj.insert("from".into(), serde_json::json!(child.id));
                        }
                        "to" => {
                            obj.insert("to".into(), serde_json::json!(child.id));
                        }
                        _ => {}
                    }
                }
                edges.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            "annotation" => {
                // `annotates-node`: recover the annotated node/edge-vertex.
                let targets = children_by_edge(schema, &root.id, "annotates-node");
                if let Some((_edge, child)) = targets.first() {
                    obj.insert("target".into(), serde_json::json!(child.id));
                }
                // `annotation-fs`: recover the associated feature structure and
                // inline its features.
                let fs_links = children_by_edge(schema, &root.id, "annotation-fs");
                if let Some((_edge, fs_child)) = fs_links.first() {
                    let mut features = serde_json::Map::new();
                    for v in schema.vertices.values() {
                        if v.kind == "feature" {
                            let fof = children_by_edge(schema, &v.id, "feature-of");
                            for (fe, fc) in &fof {
                                if fc.id == fs_child.id {
                                    let fname = fe.name.as_deref().unwrap_or(&v.id);
                                    let fcs = vertex_constraints(schema, &v.id);
                                    let fval = fcs
                                        .iter()
                                        .find(|c| c.sort == "value")
                                        .map_or("", |c| c.value.as_str());
                                    features.insert(fname.to_string(), serde_json::json!(fval));
                                }
                            }
                        }
                    }
                    if !features.is_empty() {
                        obj.insert("features".into(), serde_json::Value::Object(features));
                    }
                }
                annotations.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            "feature-structure" => {
                let mut features = serde_json::Map::new();
                for v in schema.vertices.values() {
                    if v.kind == "feature" {
                        let fof = children_by_edge(schema, &v.id, "feature-of");
                        for (fe, fc) in &fof {
                            if fc.id == root.id {
                                let fname = fe.name.as_deref().unwrap_or(&v.id);
                                let fcs = vertex_constraints(schema, &v.id);
                                let fval = fcs
                                    .iter()
                                    .find(|c| c.sort == "value")
                                    .map_or("", |c| c.value.as_str());
                                features.insert(fname.to_string(), serde_json::json!(fval));
                            }
                        }
                    }
                }
                if !features.is_empty() {
                    obj.insert("features".into(), serde_json::Value::Object(features));
                }
                feature_structures.insert(root.id.to_string(), serde_json::Value::Object(obj));
            }
            _ => {}
        }
    }

    let mut result = serde_json::Map::new();
    if !nodes.is_empty() {
        result.insert("nodes".into(), serde_json::Value::Object(nodes));
    }
    if !regions.is_empty() {
        result.insert("regions".into(), serde_json::Value::Object(regions));
    }
    if !anchors.is_empty() {
        result.insert("anchors".into(), serde_json::Value::Object(anchors));
    }
    if !edges.is_empty() {
        result.insert("edges".into(), serde_json::Value::Object(edges));
    }
    if !annotations.is_empty() {
        result.insert("annotations".into(), serde_json::Value::Object(annotations));
    }
    if !feature_structures.is_empty() {
        result.insert(
            "feature_structures".into(),
            serde_json::Value::Object(feature_structures),
        );
    }

    Ok(serde_json::Value::Object(result))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "node-of".into(),
            src_kinds: vec!["node".into()],
            tgt_kinds: vec!["region".into()],
        },
        EdgeRule {
            edge_kind: "edge-link".into(),
            src_kinds: vec!["edge".into()],
            tgt_kinds: vec!["node".into()],
        },
        // `annotates-node`: the annotation-to-target relationship, which
        // node or edge-vertex this annotation describes.
        EdgeRule {
            edge_kind: "annotates-node".into(),
            src_kinds: vec!["annotation".into()],
            tgt_kinds: vec!["node".into(), "edge".into()],
        },
        // `annotation-fs`: the annotation-to-feature-structure relationship;
        // the feature structure that encodes the annotation's content.
        // Distinct from `annotates-node` per ISO 24612 §5.
        EdgeRule {
            edge_kind: "annotation-fs".into(),
            src_kinds: vec!["annotation".into()],
            tgt_kinds: vec!["feature-structure".into()],
        },
        EdgeRule {
            edge_kind: "feature-of".into(),
            src_kinds: vec!["feature".into()],
            tgt_kinds: vec!["feature-structure".into()],
        },
        EdgeRule {
            edge_kind: "region-anchor".into(),
            src_kinds: vec!["region".into()],
            tgt_kinds: vec!["anchor".into()],
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
        assert_eq!(p.name, "laf-graf");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThLafGrafSchema"));
        assert!(registry.contains_key("ThLafGrafInstance"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "anchors": {
                "a1": {
                    "constraints": {
                        "type": "char",
                        "value": "0"
                    }
                },
                "a2": {
                    "constraints": {
                        "type": "char",
                        "value": "5"
                    }
                }
            },
            "regions": {
                "r1": {
                    "anchors": ["a1", "a2"]
                }
            },
            "nodes": {
                "n1": {
                    "region": "r1"
                },
                "n2": {
                    "constraints": {
                        "id": "n2"
                    }
                }
            },
            "edges": {
                "e1": {
                    "from": "n1",
                    "to": "n2",
                    "constraints": {
                        "label": "dep"
                    }
                }
            },
            "annotations": {
                "ann1": {
                    "target": "n1",
                    "features": {
                        "pos": "NN",
                        "lemma": "cat"
                    }
                }
            }
        });
        let schema = parse_laf_graf(&json).expect("should parse");
        assert!(schema.has_vertex("a1"));
        assert!(schema.has_vertex("r1"));
        assert!(schema.has_vertex("n1"));
        assert!(schema.has_vertex("e1"));
        assert!(schema.has_vertex("ann1"));
        let emitted = emit_laf_graf(&schema).expect("emit");
        let s2 = parse_laf_graf(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn multigraph_multiple_annotations_per_node() {
        // LAF is a multigraph: a single node can carry multiple annotations.
        let json = serde_json::json!({
            "anchors": {
                "a1": { "constraints": { "type": "char", "value": "0" } },
                "a2": { "constraints": { "type": "char", "value": "3" } }
            },
            "regions": { "r1": { "anchors": ["a1", "a2"] } },
            "nodes": { "n1": { "region": "r1" } },
            "annotations": {
                "ann_pos": {
                    "target": "n1",
                    "features": { "pos": "NN" }
                },
                "ann_ner": {
                    "target": "n1",
                    "features": { "ner": "PER" }
                }
            }
        });
        let schema = parse_laf_graf(&json).expect("should parse");
        // Both annotations must be present even though they share the same target node.
        assert!(schema.has_vertex("ann_pos"));
        assert!(schema.has_vertex("ann_ner"));
    }

    #[test]
    fn annotation_fs_and_annotates_node_are_distinct_edge_kinds() {
        // annotates-node and annotation-fs must be separate edge kinds per ISO 24612.
        let rules = edge_rules();
        let has_annotates_node = rules.iter().any(|r| r.edge_kind == "annotates-node");
        let has_annotation_fs = rules.iter().any(|r| r.edge_kind == "annotation-fs");
        let has_legacy = rules.iter().any(|r| r.edge_kind == "annotation-of");
        assert!(has_annotates_node, "annotates-node edge kind must exist");
        assert!(has_annotation_fs, "annotation-fs edge kind must exist");
        assert!(
            !has_legacy,
            "annotation-of must not exist (replaced by distinct kinds)"
        );
    }

    #[test]
    fn no_graph_vertex_kind_in_obj_kinds() {
        // The `graph` vertex kind was declared but never used; it is now removed.
        let p = protocol();
        assert!(
            !p.obj_kinds.iter().any(|k| k == "graph"),
            "unused `graph` vertex kind must not appear in obj_kinds"
        );
    }
}
