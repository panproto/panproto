//! CLI commands for full-AST parsing and project assembly.

use std::path::Path;

use miette::{Context, IntoDiagnostic, Result};
use panproto_parse::ParserRegistry;
use panproto_project::ProjectBuilder;

/// Parse a single source file and display its schema.
pub fn cmd_parse_file(file_path: &Path, verbose: bool) -> Result<()> {
    let content = std::fs::read(file_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read {}", file_path.display()))?;

    let registry = ParserRegistry::new();

    let language = registry
        .detect_language(file_path)
        .unwrap_or("raw_file");

    if verbose {
        eprintln!("Detected language: {language}");
        eprintln!("Parsing {}...", file_path.display());
    }

    let schema = registry
        .parse_file(file_path, &content)
        .into_diagnostic()
        .wrap_err("parse failed")?;

    println!(
        "Parsed {} ({language}): {} vertices, {} edges",
        file_path.display(),
        schema.vertices.len(),
        schema.edges.len(),
    );

    if verbose {
        for (name, vertex) in &schema.vertices {
            println!("  vertex {}: kind={}", name, vertex.kind);
        }
    }

    Ok(())
}

/// Parse all files in a directory into a unified project schema.
pub fn cmd_parse_project(dir_path: &Path, verbose: bool) -> Result<()> {
    if verbose {
        eprintln!("Scanning {}...", dir_path.display());
    }

    let mut builder = ProjectBuilder::new();
    builder
        .add_directory(dir_path)
        .into_diagnostic()
        .wrap_err("failed to scan directory")?;

    if verbose {
        eprintln!("Found {} files", builder.file_count());
    }

    let project = builder
        .build()
        .into_diagnostic()
        .wrap_err("project assembly failed")?;

    println!(
        "Project schema: {} files, {} vertices, {} edges",
        project.file_map.len(),
        project.schema.vertices.len(),
        project.schema.edges.len(),
    );

    for (path, protocol) in &project.protocol_map {
        println!("  {}: {protocol}", path.display());
    }

    Ok(())
}

/// Emit a parsed schema back to source text.
pub fn cmd_emit(file_path: &Path, verbose: bool) -> Result<()> {
    let content = std::fs::read(file_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read {}", file_path.display()))?;

    let registry = ParserRegistry::new();

    let protocol = registry
        .detect_language(file_path)
        .ok_or_else(|| miette::miette!("unknown language for {}", file_path.display()))?;

    if verbose {
        eprintln!("Parsing {file_path:?} as {protocol}...");
    }

    let schema = registry
        .parse_file(file_path, &content)
        .into_diagnostic()
        .wrap_err("parse failed")?;

    let emitted = registry
        .emit_with_protocol(protocol, &schema)
        .into_diagnostic()
        .wrap_err("emit failed")?;

    std::io::Write::write_all(&mut std::io::stdout(), &emitted)
        .into_diagnostic()
        .wrap_err("failed to write output")?;

    Ok(())
}
