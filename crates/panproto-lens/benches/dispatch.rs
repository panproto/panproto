#![allow(missing_docs, clippy::expect_used)]

use std::collections::{BTreeSet, HashMap, HashSet};

use divan::{Bencher, black_box};
use panproto_inst::{BoundaryProtocols, CompiledMigration, EdgeId, GInstance, Node};
use panproto_lens::{
    Lens, graph_get_via_table, graph_get_via_tree, graph_put_via_table, graph_put_via_tree,
};
use panproto_schema::{Edge, Protocol, SchemaBuilder};

fn main() {
    divan::main();
}

fn fixture(size: usize) -> (Lens, GInstance) {
    let edge = Edge {
        src: "node".into(),
        tgt: "node".into(),
        kind: "next".into(),
        name: None,
    };
    let schema = SchemaBuilder::new(&Protocol::default())
        .vertex("node", "record", None)
        .and_then(|builder| builder.edge("node", "node", "next", None))
        .and_then(SchemaBuilder::build)
        .expect("the benchmark schema is valid");
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: HashSet::from(["node".into()]),
            surviving_edges: HashSet::from([edge.clone()]),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        },
        src_schema: schema.clone(),
        tgt_schema: schema,
    };
    let mut graph = GInstance::new();
    let size = u32::try_from(size).expect("benchmark size fits in u32");
    for id in 0..size {
        graph = graph.with_node(Node::new(id, "node"));
        if id > 0 {
            graph = graph.with_edge_id(EdgeId(id - 1), id - 1, id, edge.clone());
        }
    }
    (lens, graph)
}

fn table_fixture(size: usize) -> (Lens, GInstance, Protocol) {
    let protocol = Protocol {
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    };
    let schema = SchemaBuilder::new(&protocol)
        .vertex("row", "record", None)
        .and_then(SchemaBuilder::build)
        .expect("the benchmark schema is valid");
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: HashSet::from(["row".into()]),
            surviving_edges: HashSet::new(),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        },
        src_schema: schema.clone(),
        tgt_schema: schema,
    };
    let mut graph = GInstance::new();
    let size = u32::try_from(size).expect("benchmark size fits in u32");
    for id in 0..size {
        graph = graph.with_node(Node::new(id, "row"));
    }
    (lens, graph, protocol)
}

#[divan::bench(args = [1_000usize, 10_000usize])]
fn graph_get(bencher: Bencher<'_, '_>, size: usize) {
    let (lens, graph) = fixture(size);
    bencher.bench(|| {
        panproto_lens::graph_lens::get(black_box(&lens), black_box(&graph))
            .expect("benchmark graph get")
    });
}

#[divan::bench(args = [1_000usize, 10_000usize])]
fn tree_get_with_conversions(bencher: Bencher<'_, '_>, size: usize) {
    let (lens, graph) = fixture(size);
    bencher.bench(|| {
        graph_get_via_tree(black_box(&lens), black_box(&graph))
            .expect("benchmark tree bridge get")
            .expect("benchmark source is tree-shaped")
    });
}

#[divan::bench(args = [1_000usize, 10_000usize])]
fn graph_put(bencher: Bencher<'_, '_>, size: usize) {
    let (lens, graph) = fixture(size);
    let (view, complement) =
        panproto_lens::graph_lens::get(&lens, &graph).expect("benchmark graph get");
    bencher.bench(|| {
        panproto_lens::graph_lens::put(black_box(&lens), black_box(&view), black_box(&complement))
            .expect("benchmark graph put")
    });
}

#[divan::bench(args = [1_000usize, 10_000usize])]
fn tree_put_with_conversions(bencher: Bencher<'_, '_>, size: usize) {
    let (lens, graph) = fixture(size);
    let (view, complement) =
        panproto_lens::graph_lens::get(&lens, &graph).expect("benchmark graph get");
    bencher.bench(|| {
        graph_put_via_tree(black_box(&lens), black_box(&view), black_box(&complement))
            .expect("benchmark tree bridge put")
            .expect("benchmark view is tree-shaped")
    });
}

#[divan::bench(args = [1_000usize, 10_000usize])]
fn table_graph_get(bencher: Bencher<'_, '_>, size: usize) {
    let (lens, graph, _) = table_fixture(size);
    bencher.bench(|| {
        panproto_lens::graph_lens::get(black_box(&lens), black_box(&graph))
            .expect("benchmark graph get")
    });
}

#[divan::bench(args = [1_000usize, 10_000usize])]
fn table_get_with_conversions(bencher: Bencher<'_, '_>, size: usize) {
    let (lens, graph, protocol) = table_fixture(size);
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    bencher.bench(|| {
        graph_get_via_table(black_box(&lens), black_box(&graph), protocols)
            .expect("benchmark table bridge get")
            .expect("benchmark source is table-shaped")
    });
}

#[divan::bench(args = [1_000usize, 10_000usize])]
fn table_graph_put(bencher: Bencher<'_, '_>, size: usize) {
    let (lens, graph, _) = table_fixture(size);
    let (view, complement) =
        panproto_lens::graph_lens::get(&lens, &graph).expect("benchmark graph get");
    bencher.bench(|| {
        panproto_lens::graph_lens::put(black_box(&lens), black_box(&view), black_box(&complement))
            .expect("benchmark graph put")
    });
}

#[divan::bench(args = [1_000usize, 10_000usize])]
fn table_put_with_conversions(bencher: Bencher<'_, '_>, size: usize) {
    let (lens, graph, protocol) = table_fixture(size);
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let (view, complement) =
        panproto_lens::graph_lens::get(&lens, &graph).expect("benchmark graph get");
    bencher.bench(|| {
        graph_put_via_table(
            black_box(&lens),
            black_box(&view),
            black_box(&complement),
            protocols,
        )
        .expect("benchmark table bridge put")
        .expect("benchmark view is table-shaped")
    });
}
