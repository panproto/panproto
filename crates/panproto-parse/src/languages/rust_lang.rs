//! Rust full-AST parser and emitter.

use crate::languages::common::LanguageParser;
use crate::walker::WalkerConfig;

/// Full-AST parser for Rust source files.
pub struct RustParser {
    inner: LanguageParser,
}

impl RustParser {
    /// Create a new Rust parser with auto-derived theory.
    ///
    /// # Panics
    ///
    /// Panics if theory extraction from the embedded `NODE_TYPES` fails.
    #[must_use]
    pub fn new() -> Self {
        let config = WalkerConfig {
            extra_scope_kinds: vec![
                "impl_item".to_owned(),
                "trait_item".to_owned(),
                "mod_item".to_owned(),
                "closure_expression".to_owned(),
                "macro_definition".to_owned(),
            ],
            extra_block_kinds: vec![
                "match_block".to_owned(),
                "use_list".to_owned(),
                "field_declaration_list".to_owned(),
                "enum_variant_list".to_owned(),
            ],
            name_fields: vec!["name".to_owned(), "identifier".to_owned()],
            capture_comments: true,
            capture_formatting: true,
        };

        let inner = match LanguageParser::new(
            "rust",
            vec!["rs"],
            tree_sitter_rust::LANGUAGE,
            tree_sitter_rust::NODE_TYPES.as_bytes(),
            config,
        ) {
            Ok(v) => v,
            Err(e) => panic!("grammar theory extraction failed: {e}"),
        };

        Self { inner }
    }
}

impl Default for RustParser {
    fn default() -> Self {
        Self::new()
    }
}
impl crate::registry::AstParser for RustParser {
    fn protocol_name(&self) -> &str {
        self.inner.protocol_name()
    }
    fn parse(
        &self,
        source: &[u8],
        file_path: &str,
    ) -> Result<panproto_schema::Schema, crate::ParseError> {
        self.inner.parse(source, file_path)
    }
    fn emit(&self, schema: &panproto_schema::Schema) -> Result<Vec<u8>, crate::ParseError> {
        self.inner.emit(schema)
    }
    fn supported_extensions(&self) -> &[&str] {
        self.inner.supported_extensions()
    }
    fn theory_meta(&self) -> &crate::theory_extract::ExtractedTheoryMeta {
        self.inner.theory_meta()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::registry::AstParser;

    #[test]
    fn parse_rust_function() {
        let parser = RustParser::new();
        let source = br"
fn fibonacci(n: u64) -> u64 {
    match n {
        0 | 1 => n,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}
";
        let schema = parser.parse(source, "fib.rs").unwrap();
        assert!(
            schema.vertices.len() > 10,
            "got {} vertices",
            schema.vertices.len()
        );
    }

    #[test]
    fn parse_rust_struct_impl() {
        let parser = RustParser::new();
        let source = br#"
use std::fmt;

pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}
"#;
        let schema = parser.parse(source, "point.rs").unwrap();
        assert!(
            schema.vertices.len() > 30,
            "got {} vertices",
            schema.vertices.len()
        );
    }

    #[test]
    fn rust_theory_extraction() {
        let parser = RustParser::new();
        let meta = parser.theory_meta();
        assert!(
            meta.vertex_kinds.len() > 80,
            "expected 80+ vertex kinds, got {}",
            meta.vertex_kinds.len()
        );
    }

    #[test]
    fn emit_roundtrip_rust() {
        let parser = RustParser::new();
        let source = b"fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        let schema = parser.parse(source, "add.rs").unwrap();
        let emitted = parser.emit(&schema).unwrap();
        assert_eq!(
            std::str::from_utf8(&emitted).unwrap(),
            std::str::from_utf8(source).unwrap(),
            "Rust emit(parse(source)) should reproduce the original source"
        );
    }
}
