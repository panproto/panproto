//! Parse a real OpenTelemetry `trace.proto` into a full-AST schema.

use std::path::Path;

use panproto_parse::ParserRegistry;

const TRACE_PROTO: &[u8] = include_bytes!("../../../fixtures/protobuf/trace.proto");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry = ParserRegistry::new();
    let schema = registry.parse_file(Path::new("trace.proto"), TRACE_PROTO)?;
    println!(
        "parsed trace.proto: {} vertices, {} edges",
        schema.vertices.len(),
        schema.edges.len()
    );
    Ok(())
}
