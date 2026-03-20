#![allow(missing_docs, clippy::expect_used)]

use std::collections::{HashMap, HashSet};

use divan::Bencher;
use panproto_gat::Name;
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{CompiledMigration, Node, WInstance};
use panproto_lens::{Lens, get, put};
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
    let mut outgoing: HashMap<Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), smallvec::SmallVec<Edge, 2>> = HashMap::new();

    for (id, kind) in vertices {
        vert_map.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
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
        coercions: HashMap::new(),
        mergers: HashMap::new(),
        defaults: HashMap::new(),
        policies: HashMap::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
    }
}

/// Build a wide schema: root object with `n` string children.
fn wide_schema(n: usize) -> (Schema, Vec<Edge>) {
    let mut vert_names: Vec<String> = vec!["root".into()];
    let mut edges = Vec::new();

    for i in 0..n {
        vert_names.push(format!("field{i}"));
        edges.push(make_edge("root", &format!("field{i}"), &format!("f{i}")));
    }

    let vert_refs: Vec<(&str, &str)> = std::iter::once(("root", "object"))
        .chain(vert_names[1..].iter().map(|s| (s.as_str(), "string")))
        .collect();

    (test_schema(&vert_refs, &edges), edges)
}

/// Build a `WInstance` for a wide schema.
fn wide_instance(n: usize, edges: &[Edge]) -> WInstance {
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "root"));
    for i in 0..n {
        let id = u32::try_from(i + 1).expect("n fits in u32");
        nodes.insert(
            id,
            Node::new(id, format!("field{i}"))
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

    WInstance::new(nodes, arcs, vec![], 0, Name::from("root"))
}

/// Build an identity lens for the given schema.
fn identity_lens(schema: &Schema) -> Lens {
    let surviving_verts = schema.vertices.keys().cloned().collect();
    let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();

    let compiled = CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
    };

    Lens {
        compiled,
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    }
}

/// Build a projection lens that keeps only the first `keep` fields.
fn projection_lens(schema: &Schema, edges: &[Edge], total: usize, keep: usize) -> Lens {
    let mut surviving_verts: HashSet<Name> = HashSet::new();
    surviving_verts.insert("root".into());
    for i in 0..keep.min(total) {
        surviving_verts.insert(Name::from(format!("field{i}")));
    }

    let surviving_edges: HashSet<Edge> = edges.iter().take(keep).cloned().collect();

    // Build target schema with only surviving fields
    let mut tgt_vert_refs: Vec<(&str, &str)> = vec![("root", "object")];
    let field_names: Vec<String> = (0..keep).map(|i| format!("field{i}")).collect();
    for name in &field_names {
        tgt_vert_refs.push((name.as_str(), "string"));
    }
    let tgt_schema = test_schema(&tgt_vert_refs, &edges[..keep]);

    let compiled = CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
    };

    Lens {
        compiled,
        src_schema: schema.clone(),
        tgt_schema,
    }
}

// ---------------------------------------------------------------------------
// Benchmarks: get with identity lens (no complement)
// ---------------------------------------------------------------------------

#[divan::bench(args = [5, 20, 50])]
fn get_identity_lens(bencher: Bencher, n: usize) {
    let (schema, edges) = wide_schema(n);
    let instance = wide_instance(n, &edges);
    let lens = identity_lens(&schema);

    bencher.bench(|| get(&lens, &instance));
}

// ---------------------------------------------------------------------------
// Benchmarks: get with projection (large complement)
// ---------------------------------------------------------------------------

#[divan::bench(args = [10, 50])]
fn get_projection_lens(bencher: Bencher, n: usize) {
    let (schema, edges) = wide_schema(n);
    let instance = wide_instance(n, &edges);
    // Keep only 2 of n fields, so complement is large
    let lens = projection_lens(&schema, &edges, n, 2);

    bencher.bench(|| get(&lens, &instance));
}

// ---------------------------------------------------------------------------
// Benchmarks: get then put round-trip
// ---------------------------------------------------------------------------

#[divan::bench(args = [5, 20, 50])]
fn get_then_put_round_trip(bencher: Bencher, n: usize) {
    let (schema, edges) = wide_schema(n);
    let instance = wide_instance(n, &edges);
    let lens = identity_lens(&schema);

    bencher.bench(|| {
        let (view, complement) = get(&lens, &instance).expect("get should succeed");
        put(&lens, &view, &complement).expect("put should succeed")
    });
}

// ---------------------------------------------------------------------------
// Benchmarks: get then put with projection (complement restoration)
// ---------------------------------------------------------------------------

#[divan::bench(args = [10, 50])]
fn get_then_put_projection(bencher: Bencher, n: usize) {
    let (schema, edges) = wide_schema(n);
    let instance = wide_instance(n, &edges);
    let lens = projection_lens(&schema, &edges, n, 2);

    bencher.bench(|| {
        let (view, complement) = get(&lens, &instance).expect("get should succeed");
        put(&lens, &view, &complement).expect("put should succeed")
    });
}
