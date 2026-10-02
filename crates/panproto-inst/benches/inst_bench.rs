#![allow(missing_docs, clippy::expect_used)]

use std::collections::{HashMap, HashSet};

use divan::Bencher;
use panproto_gat::Name;
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{
    BoundaryProtocols, CompiledMigration, FInstance, Node, RestrictPlan, WInstance,
    functor_extend_engine, functor_restrict_with_plan, parse_json, wtype_restrict_with_plan,
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
        entries: Vec::new(),
        outgoing,
        incoming,
        between,
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        coercions: HashMap::new(),
        mergers: HashMap::new(),
        defaults: HashMap::new(),
        policies: HashMap::new(),
        ..Default::default()
    }
}

/// Build a deep tree schema: root -> v0 -> v1 -> ... -> v(n-1)
fn deep_tree_schema(depth: usize) -> (Schema, Vec<Edge>) {
    let mut vert_strs: Vec<String> = vec!["root".to_owned()];
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

/// Builds a wide tree schema: root with `width` children.
fn wide_tree_schema(width: usize) -> (Schema, Vec<Edge>) {
    let mut vert_strs: Vec<String> = vec!["root".to_owned()];
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

    WInstance::new(nodes, arcs, vec![], 0, Name::from("root"))
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

    WInstance::new(nodes, arcs, vec![], 0, Name::from("root"))
}

fn identity_compiled(vertices: &[Name], edges: &[Edge]) -> CompiledMigration {
    CompiledMigration {
        surviving_verts: vertices.iter().cloned().collect(),
        surviving_edges: edges.iter().cloned().collect(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
        op_term_assignments: HashMap::new(),
        expansion_path: HashMap::new(),
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
// Benchmarks: wtype_restrict, deep tree
// ---------------------------------------------------------------------------

#[divan::bench(args = [10, 50, 100])]
fn wtype_restrict_deep_tree(bencher: Bencher, depth: usize) {
    let (schema, edges) = deep_tree_schema(depth);
    let instance = deep_tree_instance(depth, &edges);

    let mut verts: Vec<Name> = vec!["root".into()];
    for i in 0..depth {
        verts.push(Name::from(format!("v{i}")));
    }
    let compiled = identity_compiled(&verts, &edges);

    let plan = RestrictPlan::compile(&schema, &schema, &compiled).expect("the plan compiles");
    bencher.bench(|| wtype_restrict_with_plan(&instance, &plan));
}

// ---------------------------------------------------------------------------
// Benchmarks: wtype_restrict, wide tree
// ---------------------------------------------------------------------------

#[divan::bench(args = [10, 50])]
fn wtype_restrict_wide_tree(bencher: Bencher, width: usize) {
    let (schema, edges) = wide_tree_schema(width);
    let instance = wide_tree_instance(width, &edges);

    let mut verts: Vec<Name> = vec!["root".into()];
    for i in 0..width {
        verts.push(Name::from(format!("child{i}")));
    }
    let compiled = identity_compiled(&verts, &edges);

    let plan = RestrictPlan::compile(&schema, &schema, &compiled).expect("the plan compiles");
    bencher.bench(|| wtype_restrict_with_plan(&instance, &plan));
}

// ---------------------------------------------------------------------------
// Benchmarks: wtype_restrict, with contraction
// ---------------------------------------------------------------------------

#[divan::bench(args = [10, 50])]
fn wtype_restrict_contraction(bencher: Bencher, depth: usize) {
    // Deep tree of `depth` nodes, keep only root + first 2
    let (_schema, edges) = deep_tree_schema(depth);
    let instance = deep_tree_instance(depth, &edges);

    let keep = 2.min(depth);
    let mut surviving_verts: HashSet<Name> = HashSet::new();
    surviving_verts.insert("root".into());
    for i in 0..keep {
        surviving_verts.insert(Name::from(format!("v{i}")));
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
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
        op_term_assignments: HashMap::new(),
        expansion_path: HashMap::new(),
    };

    let plan =
        RestrictPlan::compile(&tgt_schema, &tgt_schema, &compiled).expect("the plan compiles");
    bencher.bench(|| wtype_restrict_with_plan(&instance, &plan));
}

// ---------------------------------------------------------------------------
// Benchmarks: wtype_restrict, identity (existing, kept for comparison)
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
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
        op_term_assignments: HashMap::new(),
        expansion_path: HashMap::new(),
    };

    let plan = RestrictPlan::compile(&schema, &schema, &compiled).expect("the plan compiles");
    bencher.bench(|| wtype_restrict_with_plan(&instance, &plan));
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
        inst = inst
            .with_table(format!("t{i}"), rows)
            .expect("the ids are fresh");
    }

    // Identity migration keeping all tables and their column, over a schema
    // whose protocol classifies `t{i}` as tables and `col` as a column.
    let mut vertices: Vec<(String, &str)> = Vec::new();
    let mut columns: Vec<Edge> = Vec::new();
    for i in 0..n {
        vertices.push((format!("t{i}"), "record"));
        vertices.push((format!("t{i}.col"), "string"));
        columns.push(make_edge(&format!("t{i}"), &format!("t{i}.col"), "col"));
    }
    let named: Vec<(&str, &str)> = vertices.iter().map(|(v, k)| (v.as_str(), *k)).collect();
    let schema = test_schema(&named, &columns);
    let protocol = panproto_schema::Protocol {
        value_kinds: std::collections::BTreeMap::from([(
            "string".to_owned(),
            panproto_schema::ScalarKind::Str,
        )]),
        non_value_kinds: std::collections::BTreeSet::from(["record".to_owned()]),
        ..panproto_schema::Protocol::default()
    };
    let compiled = CompiledMigration {
        surviving_verts: vertices
            .iter()
            .map(|(v, _)| Name::from(v.as_str()))
            .collect(),
        surviving_edges: columns.iter().cloned().collect(),
        ..CompiledMigration::default()
    };
    let plan = RestrictPlan::compile_for_tables(
        &schema,
        &schema,
        &compiled,
        BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        },
    )
    .expect("the plan compiles");

    bencher.bench(|| functor_restrict_with_plan(&inst, &plan));
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
        inst = inst
            .with_table(format!("t{i}"), rows)
            .expect("the ids are fresh");
    }

    // Identity migration (no remap, all survive)
    let surviving_verts: HashSet<Name> = (0..n).map(|i| Name::from(format!("t{i}"))).collect();
    let compiled = CompiledMigration {
        surviving_verts,
        surviving_edges: HashSet::new(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
        op_term_assignments: HashMap::new(),
        expansion_path: HashMap::new(),
    };

    bencher.bench(|| functor_extend_engine(&inst, &compiled));
}

// ---------------------------------------------------------------------------
// Graph instances
// ---------------------------------------------------------------------------

fn link() -> Edge {
    make_edge("item", "item", "link")
}

/// A graph of `n` nodes and no edges.
fn graph_nodes(n: u32) -> panproto_inst::GInstance {
    let mut graph = panproto_inst::GInstance::new();
    for id in 0..n {
        graph
            .insert_node(Node::new(id, "item"))
            .expect("each id is fresh");
    }
    graph
}

/// A chain of `n` nodes, each with one value and one extra field, under
/// fresh edge ids.
fn graph_chain(n: u32) -> panproto_inst::GInstance {
    let mut graph = panproto_inst::GInstance::new();
    for id in 0..n {
        let mut node = Node::new(id, "item");
        node.value = Some(FieldPresence::Present(Value::Int(i64::from(id))));
        node.extra_fields
            .insert("label".into(), Value::Str(format!("n{id}")));
        graph.insert_node(node).expect("each id is fresh");
    }
    for id in 1..n {
        graph
            .add_edge(id - 1, id, link())
            .expect("both endpoints exist");
    }
    graph
}

/// A tree of `n` nodes with no fans: node `i > 0` hangs under `i / 2`.
fn fanless_tree(n: u32) -> WInstance {
    let nodes: HashMap<u32, Node> = (0..n).map(|id| (id, Node::new(id, "item"))).collect();
    let arcs = (1..n).map(|id| (id / 2, id, link())).collect();
    WInstance::new(nodes, arcs, vec![], 0, "item".into())
}

/// Adding `n` edges one at a time. Each allocation must cost O(1), so the
/// loop is linear in `n`.
#[divan::bench(args = [1_000, 4_000, 16_000])]
fn add_edge_in_a_loop(bencher: Bencher, n: u32) {
    bencher
        .with_inputs(|| graph_nodes(n))
        .bench_local_values(|mut graph| {
            for id in 0..n {
                graph
                    .add_edge(id, (id + 1) % n, link())
                    .expect("both endpoints exist");
            }
            graph
        });
}

/// Encoding a graph as `MessagePack`, which serializes by reference.
#[divan::bench(args = [1_000, 10_000, 100_000])]
fn encode_graph_msgpack(bencher: Bencher, n: u32) {
    let graph = graph_chain(n);
    bencher.bench_local(|| rmp_serde::to_vec(divan::black_box(&graph)).expect("encodes"));
}

/// Reifying no fans into a graph taken by value: the graph is checked for
/// reserved vocabulary and handed back without a copy. The graph each
/// iteration consumes is cloned outside the timed region.
#[divan::bench(args = [1_000, 10_000, 100_000])]
fn reify_without_fans(bencher: Bencher, n: u32) {
    let graph = panproto_inst::GInstance::from_wtype(&fanless_tree(n)).expect("a tree embeds");
    bencher
        .with_inputs(|| graph.clone())
        .bench_local_values(|g| panproto_inst::reify_fans_owned(g, &[], None).expect("reifies"));
}

/// Reifying no fans into a graph taken by reference: the returned graph is
/// the one copy a by-reference call must make.
#[divan::bench(args = [1_000, 10_000, 100_000])]
fn reify_without_fans_by_reference(bencher: Bencher, n: u32) {
    let graph = panproto_inst::GInstance::from_wtype(&fanless_tree(n)).expect("a tree embeds");
    bencher.bench_local(|| {
        panproto_inst::reify_fans(divan::black_box(&graph), &[], None).expect("reifies")
    });
}

/// Unreifying a graph with no fan nodes, taken by value: the graph is
/// checked against the fan domain and handed back without a copy.
#[divan::bench(args = [1_000, 10_000, 100_000])]
fn unreify_without_fans(bencher: Bencher, n: u32) {
    let graph = panproto_inst::GInstance::from_wtype(&fanless_tree(n)).expect("a tree embeds");
    bencher
        .with_inputs(|| graph.clone())
        .bench_local_values(|g| panproto_inst::unreify_fans_owned(g).expect("unreifies"));
}

/// Unreifying a graph with no fan nodes, taken by reference: the returned
/// rest is the one copy a by-reference call must make.
#[divan::bench(args = [1_000, 10_000, 100_000])]
fn unreify_without_fans_by_reference(bencher: Bencher, n: u32) {
    let graph = panproto_inst::GInstance::from_wtype(&fanless_tree(n)).expect("a tree embeds");
    bencher
        .bench_local(|| panproto_inst::unreify_fans(divan::black_box(&graph)).expect("unreifies"));
}

/// The graph form of a tree with no fans: nothing is reified, so the graph
/// is built once and not copied.
#[divan::bench(args = [1_000, 10_000, 100_000])]
fn tree_to_graph_without_fans(bencher: Bencher, n: u32) {
    let tree = fanless_tree(n);
    bencher.bench_local(|| {
        panproto_inst::GInstance::from_wtype(divan::black_box(&tree)).expect("a tree embeds")
    });
}

/// The tree form of a graph with no fan nodes: the graph is read in place,
/// without being copied first.
#[divan::bench(args = [1_000, 10_000, 100_000])]
fn graph_to_tree_without_fans(bencher: Bencher, n: u32) {
    let graph = panproto_inst::GInstance::from_wtype(&fanless_tree(n)).expect("a tree embeds");
    bencher.bench_local(|| divan::black_box(&graph).to_wtype(0).expect("a tree"));
}

/// Deciding the tree shape of a tree-shaped graph, over growing inputs: the
/// time grows linearly with the number of nodes and edges.
#[divan::bench(args = [1_000, 10_000, 100_000])]
fn tree_shape_linear(bencher: Bencher, n: u32) {
    let graph = panproto_inst::GInstance::from_wtype(&fanless_tree(n)).expect("a tree embeds");
    let schema = Schema::default();
    bencher.bench_local(|| divan::black_box(&graph).tree_shape(&schema));
}

/// Alternately removing the edge with the largest id and adding an edge,
/// `n` times, over a chain of `n` nodes. Each step must cost at most a
/// logarithm of the edge count, so the loop is near linear in `n`.
#[divan::bench(args = [2_000, 8_000, 32_000], sample_count = 10)]
fn add_edge_after_removing_the_largest(bencher: Bencher, n: u32) {
    let graph = graph_chain(n);
    bencher
        .with_inputs(|| graph.clone())
        .bench_local_values(|mut graph| {
            let mut largest = graph.max_edge_id().expect("the chain has edges");
            for _ in 0..n {
                let edge = graph.remove_edge(largest).expect("the edge exists");
                largest = graph
                    .add_edge(edge.src, edge.tgt, edge.edge)
                    .expect("both endpoints exist");
            }
            graph
        });
}

/// The graph form of a tree with one fan per node but the root: each fan's
/// targets are checked in constant time, so the time grows linearly with
/// the number of fans.
#[divan::bench(args = [8_000, 32_000, 128_000], sample_count = 10)]
fn tree_to_graph_with_fans(bencher: Bencher, n: u32) {
    let mut tree = fanless_tree(n);
    tree.fans = (1..n)
        .map(|id| panproto_inst::Fan {
            hyper_edge_id: "rel".into(),
            parent: id / 2,
            children: HashMap::from([("arg".to_owned(), id)]),
        })
        .collect();
    bencher.bench_local(|| {
        panproto_inst::GInstance::from_wtype_embedded(divan::black_box(&tree), None)
            .expect("a tree embeds")
    });
}

// ---------------------------------------------------------------------------
// Restrict plans, the graph engine and queries
// ---------------------------------------------------------------------------

/// A schema whose two surviving vertices `s` and `t` are joined only through
/// a dropped cycle of `n` vertices, each entered from `s` and leaving to
/// `t`, with the target edge `s -> t` the contraction resolves to.
fn dropped_region_schemas(n: usize) -> (Schema, Schema, CompiledMigration) {
    let protocol = panproto_schema::Protocol::default();
    let mut src = panproto_schema::SchemaBuilder::new(&protocol);
    for v in ["s", "t"] {
        src = src.vertex(v, "record", None).expect("fresh vertex");
    }
    for i in 0..n {
        src = src
            .vertex(&format!("d{i}"), "record", None)
            .expect("fresh vertex");
    }
    for i in 0..n {
        let d = format!("d{i}");
        let next = format!("d{}", (i + 1) % n);
        src = src
            .edge("s", &d, "prop", Some(&format!("in{i}")))
            .and_then(|b| b.edge(&d, &next, "prop", Some(&format!("next{i}"))))
            .and_then(|b| b.edge(&d, "t", "prop", Some(&format!("out{i}"))))
            .expect("fresh edges");
    }
    let tgt = panproto_schema::SchemaBuilder::new(&protocol)
        .vertex("s", "record", None)
        .and_then(|b| b.vertex("t", "record", None))
        .and_then(|b| b.edge("s", "t", "prop", Some("t")))
        .and_then(panproto_schema::SchemaBuilder::build)
        .expect("target schema");
    let migration = CompiledMigration {
        surviving_verts: ["s", "t"].into_iter().map(Name::from).collect(),
        ..CompiledMigration::default()
    };
    (src.build().expect("source schema"), tgt, migration)
}

/// Compiling a restrict plan over a dropped region of `n` vertices and
/// `3n` edges, which is cyclic and so unbounded. Compile is
/// `O(V (V + E))` and enumerates no path (the region holds exponentially
/// many), so doubling `n` at most quadruples the time.
#[divan::bench(args = [50, 100, 200, 400], sample_count = 20)]
fn contraction_plan_compile_large_dropped_region(bencher: Bencher, n: usize) {
    let (src, tgt, migration) = dropped_region_schemas(n);
    bencher.bench_local(|| {
        RestrictPlan::compile(divan::black_box(&src), &tgt, &migration).expect("compiles")
    });
}

/// A chain of `n` diamonds over one anchor: `n` merge points joined by
/// `2^n` paths.
fn diamond_chain(n: u32) -> panproto_inst::GInstance {
    let c = make_edge("r", "r", "c");
    let mut graph = panproto_inst::GInstance::new();
    for id in 0..=3 * n {
        graph = graph.with_node(Node::new(id, "r"));
    }
    for i in 0..n {
        let top = 3 * i;
        for mid in [top + 1, top + 2] {
            graph = graph
                .with_edge(top, mid, c.clone())
                .with_edge(mid, top + 3, c.clone());
        }
    }
    graph
}

/// A query of `2n` steps over [`diamond_chain`]: path steps map sets to
/// sets, so the cost is `O(steps (V + E))` although the paths number `2^n`.
#[divan::bench(args = [50, 100, 200], sample_count = 20)]
fn query_diamond_chain(bencher: Bencher, n: u32) {
    let graph = diamond_chain(n);
    let schema = panproto_schema::SchemaBuilder::new(&panproto_schema::Protocol::default())
        .vertex("r", "record", None)
        .and_then(|b| b.edge("r", "r", "prop", Some("c")))
        .and_then(panproto_schema::SchemaBuilder::build)
        .expect("schema");
    let steps = usize::try_from(2 * n).expect("small");
    let query = panproto_inst::InstanceQuery {
        anchor: Name::from("r"),
        path: vec!["c".into(); steps],
        ..Default::default()
    };
    bencher.bench_local(|| {
        panproto_inst::execute_graph(divan::black_box(&query), &graph, &schema).expect("answers")
    });
}

/// One restrict over a tree-shaped input of `n` nodes, half of them
/// dropped, for the tree engine on the tree and the graph engine on its
/// graph form.
fn halved_tree(n: u32) -> (WInstance, RestrictPlan) {
    let keep_edge = make_edge("item", "item", "next");
    let drop_edge = make_edge("item", "gone", "gone");
    let mut nodes: HashMap<u32, Node> = HashMap::new();
    let mut arcs = Vec::new();
    for id in 0..n {
        let dropped = id % 2 == 1;
        nodes.insert(id, Node::new(id, if dropped { "gone" } else { "item" }));
        if id > 0 {
            let parent = (id - 1) / 2 * 2;
            arcs.push((
                parent,
                id,
                if dropped {
                    drop_edge.clone()
                } else {
                    keep_edge.clone()
                },
            ));
        }
    }
    let protocol = panproto_schema::Protocol::default();
    let src = panproto_schema::SchemaBuilder::new(&protocol)
        .vertex("item", "record", None)
        .and_then(|b| b.vertex("gone", "record", None))
        .and_then(|b| b.edge("item", "item", "prop", Some("next")))
        .and_then(|b| b.edge("item", "gone", "prop", Some("gone")))
        .and_then(panproto_schema::SchemaBuilder::build)
        .expect("source schema");
    let tgt = panproto_schema::SchemaBuilder::new(&protocol)
        .vertex("item", "record", None)
        .and_then(|b| b.edge("item", "item", "prop", Some("next")))
        .and_then(panproto_schema::SchemaBuilder::build)
        .expect("target schema");
    let migration = CompiledMigration {
        surviving_verts: std::iter::once(Name::from("item")).collect(),
        ..CompiledMigration::default()
    };
    let plan = RestrictPlan::compile(&src, &tgt, &migration).expect("compiles");
    (WInstance::new(nodes, arcs, vec![], 0, "item".into()), plan)
}

#[divan::bench(args = [1_000, 10_000])]
fn restrict_tree_engine_on_halved_tree(bencher: Bencher, n: u32) {
    let (tree, plan) = halved_tree(n);
    bencher.bench_local(|| wtype_restrict_with_plan(divan::black_box(&tree), &plan).expect("ok"));
}

#[divan::bench(args = [1_000, 10_000])]
fn restrict_graph_engine_on_halved_tree(bencher: Bencher, n: u32) {
    let (tree, plan) = halved_tree(n);
    let graph = panproto_inst::GInstance::from_wtype(&tree).expect("a tree embeds");
    bencher.bench_local(|| {
        panproto_inst::graph_restrict_with_plan(divan::black_box(&graph), &plan).expect("ok")
    });
}
