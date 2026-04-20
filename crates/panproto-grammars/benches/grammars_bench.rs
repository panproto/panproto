//! Grammar loading + tree-sitter parse benchmarks on real protobuf source.

#![allow(clippy::expect_used)]

fn main() {
    divan::main();
}

const TRACE_PROTO: &[u8] = include_bytes!("../../../fixtures/protobuf/trace.proto");

#[divan::bench]
fn load_grammars_registry(bencher: divan::Bencher) {
    bencher.bench(panproto_grammars::grammars);
}

#[divan::bench]
fn parse_trace_proto_via_tree_sitter(bencher: divan::Bencher) {
    let grammars = panproto_grammars::grammars();
    let grammar = grammars
        .iter()
        .find(|g| g.name == "protobuf")
        .expect("protobuf grammar available");
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&grammar.language).expect("lang");
    bencher.bench_local(|| parser.parse(TRACE_PROTO, None).expect("parse"));
}
