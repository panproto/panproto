#![allow(missing_docs, clippy::expect_used)]

use std::collections::{HashMap, HashSet};

use divan::Bencher;
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{
    CompiledMigration, FInstance, Node, WInstance, functor_extend, functor_restrict, parse_json,
    wtype_restrict,
};
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
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
    }
}

/// Build a deep tree schema: root -> v0 -> v1 -> ... -> v(n-1)
fn deep_tree_schema(depth: usize) -> (Schema, Vec<Edge>) {
    let mut vert_strs: Vec<String> = vec!["root".into()];
    let mut edges = Vec::new();

    for i in 0..depth {
        vert_strs.push(format!("v{i}"));
    }

    let vert_refs: Vec<(&str, &str)> = std::iter::once(("root", "object"))
        .chain(vert_strs[1..].iter().map(|s| (s.as_str(), "string")))
        .collect();

    for i in 0..depth {
        let src = if i == 0 {
            "root".to_string()
        } else {
            format!("v{}", i - 1)
        };
        let tgt = format!("v{i}");
        edges.push(make_edge(&src, &tgt, &format!("e{i}")));
    }

    (test_schema(&vert_refs, &edges), edges)
}

/// Build a wide tree schema: root with `width` children
fn wide_tree_schema(width: usize) -> (Schema, Vec<Edge>) {
    let mut vert_strs: Vec<String> = vec!["root".into()];
    let mut edges = Vec::new();

    for i in 0..width {
        vert_strs.push(format!("child{i}"));
    }

    let vert_refs: Vec<(&str, &str)> = std::iter::once(("root", "object"))
        .chain(vert_strs[1..].iter().map(|s| (s.as_str(), "string")))
        .collect();

    for i in 0..width {
        edges.push(make_edge("root", &format!("child{i}"), &format!("f{i}")));
    }

    (test_schema(&vert_refs, &edges), edges)
}

/// Build a `WInstance` for a deep tree.
fn deep_tree_instance(depth: usize, edges: &[Edge]) -> WInstance {
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "root"));
    for i in 0..depth {
        let id = u32::try_from(i + 1).expect("depth fits in u32");
        nodes.insert(
            id,
            Node::new(id, format!("v{i}"))
                .with_value(FieldPresence::Present(Value::Str(format!("val{i}")))),
        );
    }

    let arcs: Vec<(u32, u32, Edge)> = edges
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let parent = if i == 0 {
                0
            } else {
                u32::try_from(i).expect("i fits in u32")
            };
            let child = u32::try_from(i + 1).expect("i+1 fits in u32");
            (parent, child, e.clone())
        })
        .collect();

    WInstance::new(nodes, arcs, vec![], 0, "root".into())
}

/// Build a `WInstance` for a wide tree.
fn wide_tree_instance(width: usize, edges: &[Edge]) -> WInstance {
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "root"));
    for i in 0..width {
        let id = u32::try_from(i + 1).expect("width fits in u32");
        nodes.insert(
            id,
            Node::new(id, format!("child{i}"))
                .with_value(FieldPresence::Present(Value::Str(format!("val{i}")))),
        );
    }

    let arcs: Vec<(u32, u32, Edge)> = edges
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let child = u32::try_from(i + 1).expect("i+1 fits in u32");
            (0, child, e.clone())
        })
        .collect();

    WInstance::new(nodes, arcs, vec![], 0, "root".into())
}

fn identity_compiled(vertices: &[String], edges: &[Edge]) -> CompiledMigration {
    CompiledMigration {
        surviving_verts: vertices.iter().cloned().collect(),
        surviving_edges: edges.iter().cloned().collect(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
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

#[divan::bench(args = [3, 10, 20])]
fn parse_json_nested_objects(bencher: Bencher, depth: usize) {
    // Schema: root.a.b.c... nested objects, each with one string leaf
    let mut vert_strs: Vec<String> = vec!["root".into()];
    let mut edges = Vec::new();

    for i in 0..depth {
        let parent = vert_strs.last().expect("non-empty").clone();
        let child = format!("{parent}.n{i}");
        vert_strs.push(child);
    }
    // Add a leaf at the deepest level
    let deepest = vert_strs.last().expect("non-empty").clone();
    let leaf = format!("{deepest}.leaf");
    vert_strs.push(leaf);

    // Vertex refs: root = object, intermediate = object, leaf = string
    let vert_refs: Vec<(&str, &str)> = vert_strs
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let kind = if i == vert_strs.len() - 1 {
                "string"
            } else {
                "object"
            };
            (s.as_str(), kind)
        })
        .collect();

    // Edges: each level to the next
    for i in 0..vert_strs.len() - 1 {
        let name = if i < depth {
            format!("n{i}")
        } else {
            "leaf".to_string()
        };
        edges.push(make_edge(&vert_strs[i], &vert_strs[i + 1], &name));
    }

    let schema = test_schema(&vert_refs, &edges);

    // Build nested JSON
    let mut json_val = serde_json::Value::String("leaf_value".into());
    // Wrap in objects bottom-up
    json_val = {
        let mut obj = serde_json::Map::new();
        obj.insert("leaf".into(), json_val);
        serde_json::Value::Object(obj)
    };
    for i in (0..depth).rev() {
        let mut obj = serde_json::Map::new();
        obj.insert(format!("n{i}"), json_val);
        json_val = serde_json::Value::Object(obj);
    }

    bencher.bench(|| parse_json(&schema, "root", &json_val));
}

// ---------------------------------------------------------------------------
// Benchmarks: wtype_restrict — deep tree
// ---------------------------------------------------------------------------

#[divan::bench(args = [10, 50, 100])]
fn wtype_restrict_deep_tree(bencher: Bencher, depth: usize) {
    let (schema, edges) = deep_tree_schema(depth);
    let instance = deep_tree_instance(depth, &edges);

    let mut verts: Vec<String> = vec!["root".into()];
    for i in 0..depth {
        verts.push(format!("v{i}"));
    }
    let compiled = identity_compiled(&verts, &edges);

    bencher.bench(|| wtype_restrict(&instance, &schema, &schema, &compiled));
}

// ---------------------------------------------------------------------------
// Benchmarks: wtype_restrict — wide tree
// ---------------------------------------------------------------------------

#[divan::bench(args = [10, 50])]
fn wtype_restrict_wide_tree(bencher: Bencher, width: usize) {
    let (schema, edges) = wide_tree_schema(width);
    let instance = wide_tree_instance(width, &edges);

    let mut verts: Vec<String> = vec!["root".into()];
    for i in 0..width {
        verts.push(format!("child{i}"));
    }
    let compiled = identity_compiled(&verts, &edges);

    bencher.bench(|| wtype_restrict(&instance, &schema, &schema, &compiled));
}

// ---------------------------------------------------------------------------
// Benchmarks: wtype_restrict — with contraction
// ---------------------------------------------------------------------------

#[divan::bench(args = [10, 50])]
fn wtype_restrict_contraction(bencher: Bencher, depth: usize) {
    // Deep tree of `depth` nodes, keep only root + first 2
    let (_schema, edges) = deep_tree_schema(depth);
    let instance = deep_tree_instance(depth, &edges);

    let keep = 2.min(depth);
    let mut surviving_verts = HashSet::new();
    surviving_verts.insert("root".into());
    for i in 0..keep {
        surviving_verts.insert(format!("v{i}"));
    }

    let surviving_edges: HashSet<Edge> = edges.iter().take(keep).cloned().collect();

    // Build target schema with only surviving vertices
    let mut tgt_verts: Vec<(&str, &str)> = vec![("root", "object")];
    let vert_names: Vec<String> = (0..keep).map(|i| format!("v{i}")).collect();
    for name in &vert_names {
        tgt_verts.push((name.as_str(), "string"));
    }
    let tgt_schema = test_schema(&tgt_verts, &edges[..keep]);

    let compiled = CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    };

    bencher.bench(|| wtype_restrict(&instance, &tgt_schema, &tgt_schema, &compiled));
}

// ---------------------------------------------------------------------------
// Benchmarks: wtype_restrict — identity (existing, kept for comparison)
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

// ---------------------------------------------------------------------------
// Benchmarks: functor_restrict
// ---------------------------------------------------------------------------

#[divan::bench(args = [5, 20, 50])]
fn functor_restrict_n_tables(bencher: Bencher, n: usize) {
    // Build an FInstance with n tables, each having 10 rows
    let mut inst = FInstance::new();
    for i in 0..n {
        let rows: Vec<HashMap<String, Value>> = (0..10)
            .map(|j| {
                let mut row = HashMap::new();
                row.insert("col".into(), Value::Str(format!("v{i}_{j}")));
                row
            })
            .collect();
        inst = inst.with_table(format!("t{i}"), rows);
    }

    // Identity migration keeping all tables
    let surviving_verts: HashSet<String> = (0..n).map(|i| format!("t{i}")).collect();
    let compiled = CompiledMigration {
        surviving_verts,
        surviving_edges: HashSet::new(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    };

    bencher.bench(|| functor_restrict(&inst, &compiled));
}

// ---------------------------------------------------------------------------
// Benchmarks: functor_extend
// ---------------------------------------------------------------------------

#[divan::bench(args = [5, 20, 50])]
fn functor_extend_n_tables(bencher: Bencher, n: usize) {
    let mut inst = FInstance::new();
    for i in 0..n {
        let rows: Vec<HashMap<String, Value>> = (0..10)
            .map(|j| {
                let mut row = HashMap::new();
                row.insert("col".into(), Value::Str(format!("v{i}_{j}")));
                row
            })
            .collect();
        inst = inst.with_table(format!("t{i}"), rows);
    }

    // Identity migration (no remap, all survive)
    let surviving_verts: HashSet<String> = (0..n).map(|i| format!("t{i}")).collect();
    let compiled = CompiledMigration {
        surviving_verts,
        surviving_edges: HashSet::new(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    };

    bencher.bench(|| functor_extend(&inst, &compiled));
}
