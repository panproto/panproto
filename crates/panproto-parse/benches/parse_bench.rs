//! Parse benchmarks against real protobuf source (OpenTelemetry trace.proto).

#![allow(clippy::expect_used)]

use std::path::Path;

use panproto_parse::ParserRegistry;

fn main() {
    divan::main();
}

const TRACE_PROTO: &[u8] = include_bytes!("../../../fixtures/protobuf/trace.proto");
const DESCRIPTOR_PROTO: &[u8] = include_bytes!("../../../fixtures/protobuf/descriptor.proto");

#[divan::bench]
fn parse_opentelemetry_trace_proto(bencher: divan::Bencher) {
    let registry = ParserRegistry::new();
    let path = Path::new("trace.proto");
    bencher.bench(|| registry.parse_file(path, TRACE_PROTO));
}

#[divan::bench]
fn parse_google_descriptor_proto(bencher: divan::Bencher) {
    let registry = ParserRegistry::new();
    let path = Path::new("descriptor.proto");
    bencher.bench(|| registry.parse_file(path, DESCRIPTOR_PROTO));
}
