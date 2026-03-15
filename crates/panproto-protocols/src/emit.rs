//! Shared emit helpers for protocol serialization.
//!
//! These utilities help protocol modules convert a [`Schema`](panproto_schema::Schema) back into
//! native format text. They provide common operations like finding root
//! vertices, walking edges, and generating indented output.

use panproto_schema::{Constraint, Edge, Schema, Vertex};

/// Find root vertices (vertices with no incoming edges of the given structural kinds).
///
/// Returns vertices sorted by ID for deterministic output.
#[must_use]
pub fn find_roots<'a>(schema: &'a Schema, structural_edge_kinds: &[&str]) -> Vec<&'a Vertex> {
    let mut roots: Vec<&Vertex> = schema
        .vertices
        .values()
        .filter(|v| {
            let incoming = schema.incoming_edges(&v.id);
            !incoming
                .iter()
                .any(|e| structural_edge_kinds.contains(&&*e.kind))
        })
        .collect();
    roots.sort_by(|a, b| a.id.cmp(&b.id));
    roots
}

/// Get children of a vertex via a specific edge kind, sorted by edge name.
///
/// Returns pairs of (edge, target vertex) for each outgoing edge of the
/// specified kind.
#[must_use]
pub fn children_by_edge<'a>(
    schema: &'a Schema,
    parent: &str,
    edge_kind: &str,
) -> Vec<(&'a Edge, &'a Vertex)> {
    let mut children: Vec<(&Edge, &Vertex)> = schema
        .outgoing_edges(parent)
        .iter()
        .filter(|e| e.kind == edge_kind)
        .filter_map(|e| schema.vertices.get(&e.tgt).map(|v| (e, v)))
        .collect();
    children.sort_by(|a, b| {
        let a_name = a.0.name.as_deref().unwrap_or("");
        let b_name = b.0.name.as_deref().unwrap_or("");
        a_name.cmp(b_name)
    });
    children
}

/// Get a constraint value by sort for a vertex.
#[must_use]
pub fn constraint_value<'a>(schema: &'a Schema, vertex_id: &str, sort: &str) -> Option<&'a str> {
    schema
        .constraints
        .get(vertex_id)?
        .iter()
        .find(|c| c.sort == sort)
        .map(|c| c.value.as_str())
}

/// Get all constraints for a vertex.
#[must_use]
pub fn vertex_constraints<'a>(schema: &'a Schema, vertex_id: &str) -> Vec<&'a Constraint> {
    schema
        .constraints
        .get(vertex_id)
        .map(|cs| cs.iter().collect())
        .unwrap_or_default()
}

/// Find the target vertex of an outgoing edge with a given kind from a vertex.
#[must_use]
pub fn resolve_type<'a>(schema: &'a Schema, field_id: &str) -> Option<&'a Vertex> {
    schema
        .outgoing_edges(field_id)
        .iter()
        .find(|e| e.kind == "type-of")
        .and_then(|e| schema.vertices.get(&e.tgt))
}

/// An indented text writer for emitting nested format text.
///
/// Provides a simple API for building indented, line-oriented output
/// such as `.proto` files, GraphQL SDL, or TypeScript declarations.
pub struct IndentWriter {
    buf: String,
    level: usize,
    indent_str: &'static str,
}

impl IndentWriter {
    /// Create a new `IndentWriter` with the given indentation string.
    ///
    /// Common values: `"  "` (2 spaces), `"    "` (4 spaces), `"\t"`.
    #[must_use]
    pub const fn new(indent_str: &'static str) -> Self {
        Self {
            buf: String::new(),
            level: 0,
            indent_str,
        }
    }

    /// Increase the indentation level by one.
    pub const fn indent(&mut self) {
        self.level += 1;
    }

    /// Decrease the indentation level by one.
    pub const fn dedent(&mut self) {
        self.level = self.level.saturating_sub(1);
    }

    /// Write a line at the current indentation level.
    pub fn line(&mut self, s: &str) {
        for _ in 0..self.level {
            self.buf.push_str(self.indent_str);
        }
        self.buf.push_str(s);
        self.buf.push('\n');
    }

    /// Write a blank line.
    pub fn blank(&mut self) {
        self.buf.push('\n');
    }

    /// Write raw text without indentation.
    pub fn raw(&mut self, s: &str) {
        self.buf.push_str(s);
    }

    /// Consume the writer and return the built string.
    #[must_use]
    pub fn finish(self) -> String {
        self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indent_writer_basic() {
        let mut w = IndentWriter::new("  ");
        w.line("message Foo {");
        w.indent();
        w.line("string bar = 1;");
        w.dedent();
        w.line("}");
        let result = w.finish();
        assert_eq!(result, "message Foo {\n  string bar = 1;\n}\n");
    }
}
