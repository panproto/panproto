#![allow(missing_docs, clippy::expect_used, clippy::cast_possible_truncation)]

use std::collections::{HashMap, HashSet};

use divan::Bencher;
use panproto_gat::Name;
use panproto_inst::Value;
use panproto_inst::value::FieldPresence;
use panproto_inst::{CompiledMigration, Node, WInstance};
use panproto_mig::{Migration, check_existence, compile, compose, lift_wtype};
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

/// Builds a linear chain schema: root -> v1 -> v2 -> ... -> vN.
fn chain_schema(n: usize) -> (Schema, Vec<Edge>) {
    let mut vert_strs: Vec<String> = vec!["root".to_owned()];
    let mut edges = Vec::new();

    for i in 0..n {
        let name = format!("v{i}");
        vert_strs.push(name);
    }

    // We need &str references, so collect them
    let vert_refs: Vec<(&str, &str)> = std::iter::once(("root", "object"))
        .chain(vert_strs[1..].iter().map(|s| (s.as_str(), "string")))
        .collect();

    for i in 0..n {
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

/// Build an identity `WInstance` for a chain schema of size n.
fn chain_instance(n: usize, edges: &[Edge]) -> WInstance {
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "root"));
    for i in 0..n {
        let id = (i + 1) as u32;
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
            let parent = if i == 0 { 0 } else { i as u32 };
            (parent, (i + 1) as u32, e.clone())
        })
        .collect();

    WInstance::new(nodes, arcs, vec![], 0, Name::from("root"))
}

fn identity_compiled(n: usize, edges: &[Edge]) -> CompiledMigration {
    let mut surviving_verts: HashSet<Name> = HashSet::new();
    surviving_verts.insert("root".into());
    for i in 0..n {
        surviving_verts.insert(Name::from(format!("v{i}")));
    }

    CompiledMigration {
        surviving_verts,
        surviving_edges: edges.iter().cloned().collect(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
    }
}

fn projection_compiled(n: usize, keep: usize, edges: &[Edge]) -> CompiledMigration {
    let mut surviving_verts: HashSet<Name> = HashSet::new();
    surviving_verts.insert("root".into());
    for i in 0..keep.min(n) {
        surviving_verts.insert(Name::from(format!("v{i}")));
    }

    let surviving_edges: HashSet<Edge> = edges.iter().take(keep).cloned().collect();

    CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
    }
}

fn chain_vertex_ids(n: usize) -> Vec<Name> {
    let mut ids: Vec<Name> = vec!["root".into()];
    for i in 0..n {
        ids.push(Name::from(format!("v{i}")));
    }
    ids
}

fn make_protocol() -> panproto_schema::Protocol {
    panproto_schema::Protocol {
        name: "test".into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![],
        obj_kinds: vec!["object".into()],
        constraint_sorts: vec![],
        has_order: false,
        has_coproducts: false,
        has_recursion: false,
        has_causal: false,
        nominal_identity: false,
        ..panproto_schema::Protocol::default()
    }
}

// ---------------------------------------------------------------------------
// Benchmarks: lift_wtype
// ---------------------------------------------------------------------------

#[divan::bench]
fn lift_wtype_simple(bencher: Bencher) {
    let (schema, edges) = chain_schema(5);
    let instance = chain_instance(5, &edges);
    let compiled = identity_compiled(5, &edges);

    bencher.bench(|| lift_wtype(&compiled, &schema, &schema, &instance));
}

#[divan::bench]
fn lift_wtype_contraction(bencher: Bencher) {
    // Keep only root + first 2 of 10 vertices (drop 8)
    let (_schema, edges) = chain_schema(10);
    let instance = chain_instance(10, &edges);
    let compiled = projection_compiled(10, 2, &edges);

    // Build target schema with only the surviving vertices
    let tgt_schema = test_schema(
        &[("root", "object"), ("v0", "string"), ("v1", "string")],
        &edges[..2],
    );

    bencher.bench(|| lift_wtype(&compiled, &tgt_schema, &tgt_schema, &instance));
}

#[divan::bench(args = [100, 500])]
fn lift_wtype_at_scale(bencher: Bencher, n: usize) {
    let (schema, edges) = chain_schema(n);
    let instance = chain_instance(n, &edges);
    let compiled = identity_compiled(n, &edges);

    bencher.bench(|| lift_wtype(&compiled, &schema, &schema, &instance));
}

// ---------------------------------------------------------------------------
// Benchmarks: check_existence
// ---------------------------------------------------------------------------

#[divan::bench(args = [10, 100])]
fn check_existence_n_vertices(bencher: Bencher, n: usize) {
    let (schema, edges) = chain_schema(n);
    let vertex_ids = chain_vertex_ids(n);
    let migration = Migration::identity(&vertex_ids, &edges);
    let protocol = make_protocol();
    let registry = HashMap::new();

    bencher.bench(|| check_existence(&protocol, &schema, &schema, &migration, &registry));
}

// ---------------------------------------------------------------------------
// Benchmarks: compile
// ---------------------------------------------------------------------------

#[divan::bench(args = [10, 100])]
fn compile_n_vertices(bencher: Bencher, n: usize) {
    let (schema, edges) = chain_schema(n);
    let vertex_ids = chain_vertex_ids(n);
    let migration = Migration::identity(&vertex_ids, &edges);

    bencher.bench(|| compile(&schema, &schema, &migration));
}

// ---------------------------------------------------------------------------
// Benchmarks: compose two migrations
// ---------------------------------------------------------------------------

#[divan::bench(args = [10, 50, 100])]
fn compose_two_identity(bencher: Bencher, n: usize) {
    let (_schema, edges) = chain_schema(n);
    let vertex_ids = chain_vertex_ids(n);
    let m = Migration::identity(&vertex_ids, &edges);

    bencher.bench(|| compose(&m, &m));
}

#[divan::bench(args = [10, 50])]
fn compose_chain_of_5(bencher: Bencher, n: usize) {
    let (_schema, edges) = chain_schema(n);
    let vertex_ids = chain_vertex_ids(n);
    let m = Migration::identity(&vertex_ids, &edges);

    bencher.bench(|| {
        let m12 = compose(&m, &m).expect("compose 1-2");
        let m123 = compose(&m12, &m).expect("compose 1-3");
        let m1234 = compose(&m123, &m).expect("compose 1-4");
        compose(&m1234, &m).expect("compose 1-5")
    });
}

// ---------------------------------------------------------------------------
// Benchmarks: compose with renaming
// ---------------------------------------------------------------------------

#[divan::bench(args = [10, 50])]
fn compose_with_renaming(bencher: Bencher, n: usize) {
    let (_schema, edges) = chain_schema(n);
    let vertex_ids = chain_vertex_ids(n);

    // First migration: identity
    let m1 = Migration::identity(&vertex_ids, &edges);

    // Second migration: rename each vertex v{i} -> r{i}
    let mut vertex_map: HashMap<Name, Name> = HashMap::new();
    vertex_map.insert("root".into(), "root".into());
    let mut edge_map = HashMap::new();
    for i in 0..n {
        vertex_map.insert(Name::from(format!("v{i}")), Name::from(format!("r{i}")));
    }
    for edge in &edges {
        let new_src = vertex_map
            .get(&edge.src)
            .cloned()
            .unwrap_or_else(|| edge.src.clone());
        let new_tgt = vertex_map
            .get(&edge.tgt)
            .cloned()
            .unwrap_or_else(|| edge.tgt.clone());
        let new_edge = Edge {
            src: new_src,
            tgt: new_tgt,
            kind: edge.kind.clone(),
            name: edge.name.clone(),
        };
        edge_map.insert(edge.clone(), new_edge);
    }

    let m2 = Migration {
        vertex_map,
        edge_map,
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        expr_resolvers: HashMap::new(),
    };

    bencher.bench(|| compose(&m1, &m2));
}
