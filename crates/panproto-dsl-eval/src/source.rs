//! Source formats, and the location of a refused element in source text.

use crate::path::DocumentPath;

/// The surface syntax of a document's source text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceFormat {
    /// Nickel, evaluated with the DSL's bundled contract library.
    Nickel,
    /// JSON.
    Json,
    /// YAML.
    Yaml,
}

/// The byte range `(offset, length)` of the element at `path` in `source`.
///
/// Only JSON text is located: the range is the one
/// [`locate_json`](crate::path::locate_json) finds. YAML and Nickel text
/// give `None`, as does JSON text in which `path` names no value; a refusal
/// of such text keeps its path and carries no span.
#[must_use]
pub fn locate(source: &str, format: SourceFormat, path: &DocumentPath) -> Option<(usize, usize)> {
    match format {
        SourceFormat::Json => crate::path::locate_json(source, path),
        SourceFormat::Nickel | SourceFormat::Yaml => None,
    }
}

/// The byte range `(offset, length)` of a JSON syntax error in `source`:
/// the one byte at which the JSON reader stopped, or the empty range at
/// the end of the text.
#[must_use]
pub fn json_syntax_span(source: &str, error: &serde_json::Error) -> (usize, usize) {
    let line_start: usize = source
        .split_inclusive('\n')
        .take(error.line().saturating_sub(1))
        .map(str::len)
        .sum();
    // `serde_json` counts the bytes read on the line, so the byte it names
    // is the one before its column.
    let offset = (line_start + error.column().saturating_sub(1)).min(source.len());
    (offset, usize::from(offset < source.len()))
}
