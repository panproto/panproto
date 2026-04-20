//! Assemble a multi-file project from real OpenTelemetry .proto sources.

#![allow(clippy::expect_used)]

use std::path::Path;

use panproto_project::ProjectBuilder;

fn main() {
    divan::main();
}

const TRACE: &[u8] = include_bytes!("../../../fixtures/protobuf/trace.proto");
const COMMON: &[u8] = include_bytes!("../../../fixtures/protobuf/common.proto");
const RESOURCE: &[u8] = include_bytes!("../../../fixtures/protobuf/resource.proto");

#[divan::bench]
fn assemble_opentelemetry_protos(bencher: divan::Bencher) {
    bencher.bench_local(|| {
        let mut project = ProjectBuilder::new();
        project
            .add_file(Path::new("trace.proto"), TRACE)
            .expect("trace");
        project
            .add_file(Path::new("common.proto"), COMMON)
            .expect("common");
        project
            .add_file(Path::new("resource.proto"), RESOURCE)
            .expect("resource");
        project.build()
    });
}
