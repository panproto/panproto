//! Theory DSL parse benchmarks.

#![allow(clippy::expect_used)]

use panproto_theory_dsl::eval;

fn main() {
    divan::main();
}

/// Declarative JSON spec for `ThGraph` — the graph theory at the heart of
/// AT Protocol's schema theory (see `panproto-protocols::theories::th_graph`).
const TH_GRAPH_JSON: &str = r#"{
  "id": "dev.panproto.theories.th-graph",
  "description": "Graph theory: vertices, edges, source and target",
  "theory": "ThGraph",
  "sorts": [{"name": "Vertex"}, {"name": "Edge"}]
}"#;

#[divan::bench]
fn parse_thgraph_json(bencher: divan::Bencher) {
    bencher.bench(|| eval::eval_json(TH_GRAPH_JSON));
}
