#![allow(missing_docs, clippy::expect_used)]

use std::collections::{HashMap, HashSet};

use divan::Bencher;
use panproto_inst::{CompiledMigration, parse_json, wtype_restrict};
use panproto_schema::{Edge, Schema, Vertex};

fn main() {
    divan::main();
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

fn test_schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
    let mut vert_map = HashMap::new();
    let mut edge_map = HashMap::new();
    let mut outgoing: HashMap<String, smallvec::SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<String, smallvec::SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(String, String), smallvec::SmallVec<Edge, 2>> = HashMap::new();

    for (id, kind) in vertices {
        vert_map.insert(
            id.to_string(),
            Vertex {
                id: id.to_string(),
                kind: kind.to_string(),
                nsid: None,
            },
        );
    }

    for edge in edges {
        edge_map.insert(edge.clone(), edge.kind.clone());
        outgoing
            .entry(edge.src.clone())
            .or_default()
            .push(edge.clone());
        incoming
            .entry(edge.tgt.clone())
            .or_default()
            .push(edge.clone());
        between
            .entry((edge.src.clone(), edge.tgt.clone()))
            .or_default()
            .push(edge.clone());
    }

    Schema {
        protocol: "test".into(),
        vertices: vert_map,
        edges: edge_map,
        hyper_edges: HashMap::new(),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        outgoing,
        incoming,
        between,
    }
}

// ---------------------------------------------------------------------------
// Benchmarks: parse_json
// ---------------------------------------------------------------------------

#[divan::bench(args = [3, 10, 50])]
fn parse_json_flat_fields(bencher: Bencher, n: usize) {
    // Schema: root object with n string properties
    let mut vert_strs: Vec<String> = Vec::new();
    let mut edges = Vec::new();

    vert_strs.push("root".into());
    for i in 0..n {
        vert_strs.push(format!("root.f{i}"));
    }

    // Build vertex refs from owned strings
    let vert_refs: Vec<(&str, &str)> = std::iter::once(("root", "object"))
        .chain(vert_strs[1..].iter().map(|s| (s.as_str(), "string")))
        .collect();

    for i in 0..n {
        edges.push(make_edge("root", &vert_strs[i + 1], &format!("f{i}")));
    }

    let schema = test_schema(&vert_refs, &edges);

    // Build JSON object with n fields
    let mut obj = serde_json::Map::new();
    for i in 0..n {
        obj.insert(
            format!("f{i}"),
            serde_json::Value::String(format!("val{i}")),
        );
    }
    let json_val = serde_json::Value::Object(obj);

    bencher.bench(|| parse_json(&schema, "root", &json_val));
}

// ---------------------------------------------------------------------------
// Benchmarks: wtype_restrict
// ---------------------------------------------------------------------------

#[divan::bench]
fn wtype_restrict_identity(bencher: Bencher) {
    let edge_text = make_edge("body", "body.text", "text");
    let edge_time = make_edge("body", "body.createdAt", "createdAt");

    let schema = test_schema(
        &[
            ("body", "object"),
            ("body.text", "string"),
            ("body.createdAt", "string"),
        ],
        &[edge_text.clone(), edge_time.clone()],
    );

    let json_val = serde_json::json!({
        "text": "hello world",
        "createdAt": "2024-01-01"
    });
    let instance = parse_json(&schema, "body", &json_val).expect("parse should succeed");

    let compiled = CompiledMigration {
        surviving_verts: HashSet::from([
            "body".into(),
            "body.text".into(),
            "body.createdAt".into(),
        ]),
        surviving_edges: HashSet::from([edge_text, edge_time]),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    };

    bencher.bench(|| wtype_restrict(&instance, &schema, &schema, &compiled));
}
