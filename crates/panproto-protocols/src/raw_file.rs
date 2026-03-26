//! Raw file protocol for non-code files.
//!
//! Handles files that don't have a language protocol (README.md, LICENSE,
//! .gitignore, images, Makefile, etc.) by representing them as ordered
//! sequences of lines (text) or single opaque chunks (binary).
//!
//! ## Theory composition
//!
//! ```text
//! ThRawFile = colimit(ThGraph, ThOrder, shared=ThVertexEdge)
//! ```
//!
//! ## Vertex kinds
//!
//! - `file`: the root vertex representing the entire file
//! - `line`: a single line of text (ordered via ThOrder)
//! - `chunk`: an opaque binary blob
//!
//! ## Edge rules
//!
//! - `line-of`: file → line (ordered)
//! - `chunk-of`: file → chunk
//!
//! ## Merge behavior
//!
//! Text files merge via pushout on ordered line sequences (the same algorithm
//! as all other ordered schemas). Binary files are opaque (whole-file replacement).

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::error::ProtocolError;
use crate::theories;

/// Returns the raw file protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "raw_file".into(),
        schema_theory: "ThRawFileSchema".into(),
        instance_theory: "ThRawFileInstance".into(),
        edge_rules: vec![
            EdgeRule {
                edge_kind: "line-of".into(),
                src_kinds: vec!["file".into()],
                tgt_kinds: vec!["line".into()],
            },
            EdgeRule {
                edge_kind: "chunk-of".into(),
                src_kinds: vec!["file".into()],
                tgt_kinds: vec!["chunk".into()],
            },
        ],
        obj_kinds: vec!["file".into(), "line".into(), "chunk".into()],
        constraint_sorts: vec![
            "mime-type".into(),
            "encoding".into(),
            "line-number".into(),
            "content".into(),
        ],
        has_order: true,
        has_coproducts: false,
        has_recursion: false,
        has_causal: false,
        nominal_identity: false,
        has_defaults: false,
        has_coercions: false,
        has_mergers: false,
        has_policies: false,
    }
}

/// Register the raw file theory pair.
///
/// Schema: `colimit(ThGraph, ThOrder, shared=ThVertexEdge)`.
/// Instance: `ThWType`.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThRawFileSchema",
        "ThRawFileInstance",
    );
}

/// Parse a text file into a raw file [`Schema`].
///
/// Each line becomes a `line` vertex connected to the root `file` vertex
/// via a `line-of` edge. Lines are ordered via positional indices.
///
/// # Errors
///
/// Returns [`ProtocolError`] if schema construction fails.
pub fn parse_text(input: &str, file_path: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    // Root file vertex.
    let file_id = file_path;
    builder = builder
        .vertex(file_id, "file", None)
        .map_err(|e| ProtocolError::Parse(format!("file vertex: {e}")))?;

    // Detect mime type from extension.
    let mime = mime_from_path(file_path);
    builder = builder.constraint(file_id, "mime-type", &mime);
    builder = builder.constraint(file_id, "encoding", "utf-8");

    // One line vertex per line.
    for (i, line_text) in input.lines().enumerate() {
        let line_id = format!("{file_id}::line_{i}");
        builder = builder
            .vertex(&line_id, "line", None)
            .map_err(|e| ProtocolError::Parse(format!("line {i}: {e}")))?;

        builder = builder
            .edge(file_id, &line_id, "line-of", None)
            .map_err(|e| ProtocolError::Parse(format!("line-of edge {i}: {e}")))?;

        builder = builder.constraint(&line_id, "content", line_text);
        builder = builder.constraint(&line_id, "line-number", &i.to_string());
    }

    builder
        .build()
        .map_err(|e| ProtocolError::Parse(format!("build: {e}")))
}

/// Parse a binary file into a raw file [`Schema`].
///
/// The entire file becomes a single `chunk` vertex connected to the root
/// `file` vertex via a `chunk-of` edge.
///
/// # Errors
///
/// Returns [`ProtocolError`] if schema construction fails.
pub fn parse_binary(file_path: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let file_id = file_path;
    builder = builder
        .vertex(file_id, "file", None)
        .map_err(|e| ProtocolError::Parse(format!("file vertex: {e}")))?;

    let mime = mime_from_path(file_path);
    builder = builder.constraint(file_id, "mime-type", &mime);
    builder = builder.constraint(file_id, "encoding", "binary");

    let chunk_id = format!("{file_id}::chunk_0");
    builder = builder
        .vertex(&chunk_id, "chunk", None)
        .map_err(|e| ProtocolError::Parse(format!("chunk vertex: {e}")))?;

    builder = builder
        .edge(file_id, &chunk_id, "chunk-of", None)
        .map_err(|e| ProtocolError::Parse(format!("chunk-of edge: {e}")))?;

    builder
        .build()
        .map_err(|e| ProtocolError::Parse(format!("build: {e}")))
}

/// Emit a raw file schema back to text.
///
/// Walks line vertices in order, joining them with newlines.
///
/// # Errors
///
/// Returns [`ProtocolError`] if the schema structure is invalid.
pub fn emit_text(schema: &Schema) -> Result<String, ProtocolError> {
    // Collect line vertices with their line numbers.
    let mut lines: Vec<(usize, String)> = Vec::new();

    for (name, vertex) in &schema.vertices {
        if vertex.kind.as_ref() == "line" {
            let line_num = schema
                .constraints
                .get(name)
                .and_then(|cs| {
                    cs.iter()
                        .find(|c| c.sort.as_ref() == "line-number")
                        .and_then(|c| c.value.parse::<usize>().ok())
                })
                .unwrap_or(lines.len());

            let content = schema
                .constraints
                .get(name)
                .and_then(|cs| {
                    cs.iter()
                        .find(|c| c.sort.as_ref() == "content")
                        .map(|c| c.value.clone())
                })
                .unwrap_or_default();

            lines.push((line_num, content));
        }
    }

    // Sort by line number.
    lines.sort_by_key(|(num, _)| *num);

    let text: Vec<&str> = lines.iter().map(|(_, content)| content.as_str()).collect();
    let mut result = text.join("\n");
    if !result.is_empty() {
        result.push('\n');
    }
    Ok(result)
}

/// Detect MIME type from file path extension.
fn mime_from_path(path: &str) -> String {
    // Only consider the part after the last dot as the extension.
    // If there's no dot, there's no extension.
    let ext = if path.contains('.') {
        path.rsplit('.').next().unwrap_or("")
    } else {
        ""
    };
    match ext.to_lowercase().as_str() {
        "md" | "markdown" => "text/markdown",
        "txt" => "text/plain",
        "json" => "application/json",
        "yaml" | "yml" => "text/yaml",
        "toml" => "text/toml",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        "wasm" => "application/wasm",
        "sh" | "bash" => "text/x-shellscript",
        "dockerfile" => "text/x-dockerfile",
        "makefile" => "text/x-makefile",
        "gitignore" => "text/plain",
        "env" => "text/plain",
        "lock" => "text/plain",
        "cfg" | "ini" => "text/plain",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "log" => "text/plain",
        _ => "application/octet-stream",
    }
    .to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_def() {
        let proto = protocol();
        assert_eq!(proto.name, "raw_file");
        assert_eq!(proto.obj_kinds.len(), 3);
        assert_eq!(proto.edge_rules.len(), 2);
        assert!(proto.has_order);
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThRawFileSchema"));
        assert!(registry.contains_key("ThRawFileInstance"));
    }

    #[test]
    fn parse_text_file() {
        let input = "Hello World\nSecond line\nThird line";
        let schema = parse_text(input, "README.md").unwrap();

        // 1 file + 3 lines = 4 vertices.
        assert_eq!(schema.vertices.len(), 4);

        // Check mime type constraint on file vertex.
        let file_name: panproto_gat::Name = "README.md".into();
        let constraints = schema.constraints.get(&file_name).unwrap();
        let mime = constraints
            .iter()
            .find(|c| c.sort.as_ref() == "mime-type")
            .unwrap();
        assert_eq!(mime.value, "text/markdown");
    }

    #[test]
    fn parse_and_emit_roundtrip() {
        let input = "line one\nline two\nline three\n";
        let schema = parse_text(input, "test.txt").unwrap();
        let output = emit_text(&schema).unwrap();
        assert_eq!(output, input);
    }

    #[test]
    fn parse_empty_file() {
        let input = "";
        let schema = parse_text(input, "empty.txt").unwrap();
        // Just the file vertex (no lines for empty input).
        assert_eq!(schema.vertices.len(), 1);
    }

    #[test]
    fn parse_binary_file() {
        let schema = parse_binary("image.png").unwrap();
        assert_eq!(schema.vertices.len(), 2); // file + chunk

        let file_name: panproto_gat::Name = "image.png".into();
        let constraints = schema.constraints.get(&file_name).unwrap();
        let mime = constraints
            .iter()
            .find(|c| c.sort.as_ref() == "mime-type")
            .unwrap();
        assert_eq!(mime.value, "image/png");

        let encoding = constraints
            .iter()
            .find(|c| c.sort.as_ref() == "encoding")
            .unwrap();
        assert_eq!(encoding.value, "binary");
    }

    #[test]
    fn mime_detection() {
        assert_eq!(mime_from_path("README.md"), "text/markdown");
        assert_eq!(mime_from_path("data.json"), "application/json");
        assert_eq!(mime_from_path("photo.jpg"), "image/jpeg");
        assert_eq!(mime_from_path("unknown.xyz"), "application/octet-stream");
        // "Dockerfile" has no extension; rsplit('.').next() returns "Dockerfile"
        // which doesn't match any known extension, so it's octet-stream.
        assert_eq!(mime_from_path("Dockerfile"), "application/octet-stream");
        assert_eq!(mime_from_path("app.dockerfile"), "text/x-dockerfile");
    }
}
