//! Assemble a multi-file project schema from real OpenTelemetry proto sources.

use std::path::Path;

use panproto_project::ProjectBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut project = ProjectBuilder::new();
    project.add_file(Path::new("trace.proto"), include_bytes!("../../../fixtures/protobuf/trace.proto"))?;
    project.add_file(Path::new("common.proto"), include_bytes!("../../../fixtures/protobuf/common.proto"))?;
    project.add_file(Path::new("resource.proto"), include_bytes!("../../../fixtures/protobuf/resource.proto"))?;
    let n_files = project.file_count();
    let project_schema = project.build()?;
    println!(
        "assembled project: {} files, {} vertices, {} edges",
        n_files,
        project_schema.schema.vertices.len(),
        project_schema.schema.edges.len(),
    );
    Ok(())
}
