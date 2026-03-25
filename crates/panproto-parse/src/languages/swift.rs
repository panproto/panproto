//! Swift full-AST parser and emitter.

use crate::languages::common::LanguageParser;
use crate::walker::WalkerConfig;

/// Full-AST parser for Swift source files.
pub struct SwiftParser {
    inner: LanguageParser,
}

impl SwiftParser {
    /// Create a new Swift parser with auto-derived theory.
    ///
    /// # Panics
    ///
    /// Panics if theory extraction from the embedded `NODE_TYPES` fails.
    #[must_use]
    pub fn new() -> Self {
        let config = WalkerConfig {
            extra_scope_kinds: vec![
                "protocol_declaration".to_owned(),
                "extension_declaration".to_owned(),
                "closure_expression".to_owned(),
            ],
            extra_block_kinds: vec![
                "switch_entry".to_owned(),
            ],
            name_fields: vec!["name".to_owned(), "identifier".to_owned()],
            capture_comments: true,
            capture_formatting: true,
        };

        let inner = LanguageParser::new(
            "swift",
            vec!["swift"],
            tree_sitter_swift::LANGUAGE,
            tree_sitter_swift::NODE_TYPES.as_bytes(),
            config,
        )
        .expect("Swift grammar theory extraction must not fail");

        Self { inner }
    }
}

impl crate::registry::AstParser for SwiftParser {
    fn protocol_name(&self) -> &str { self.inner.protocol_name() }
    fn parse(&self, source: &[u8], file_path: &str) -> Result<panproto_schema::Schema, crate::ParseError> { self.inner.parse(source, file_path) }
    fn emit(&self, schema: &panproto_schema::Schema) -> Result<Vec<u8>, crate::ParseError> { self.inner.emit(schema) }
    fn supported_extensions(&self) -> &[&str] { self.inner.supported_extensions() }
    fn theory_meta(&self) -> &crate::theory_extract::ExtractedTheoryMeta { self.inner.theory_meta() }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::registry::AstParser;

    #[test]
    fn parse_swift_function() {
        let parser = SwiftParser::new();
        let source = br#"
func greet(name: String) -> String {
    return "Hello, " + name
}
"#;
        let schema = parser.parse(source, "greet.swift").unwrap();
        assert!(schema.vertices.len() > 5, "got {} vertices", schema.vertices.len());
    }

    #[test]
    fn swift_theory_extraction() {
        let parser = SwiftParser::new();
        let meta = parser.theory_meta();
        assert!(meta.vertex_kinds.len() > 40, "expected 40+ vertex kinds, got {}", meta.vertex_kinds.len());
    }
}
