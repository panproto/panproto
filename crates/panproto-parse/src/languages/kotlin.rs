//! Kotlin full-AST parser and emitter.
//!
//! The Kotlin tree-sitter grammar crate (`tree-sitter-kotlin` 0.3.x) depends on
//! `tree-sitter` 0.20, which has an incompatible `Language` type with tree-sitter 0.24.
//! Theory extraction from `NODE_TYPES` works correctly. Runtime parsing requires the
//! grammar crate to be updated to use `tree-sitter-language`.

use crate::error::ParseError;
use crate::registry::AstParser;
use crate::theory_extract::{ExtractedTheoryMeta, extract_theory_from_node_types};

/// Full-AST parser for Kotlin source files.
///
/// Theory extraction works (from `NODE_TYPES` JSON). Runtime parsing is unavailable
/// until `tree-sitter-kotlin` is updated from tree-sitter 0.20 to use
/// `tree-sitter-language` for version-independent grammar loading.
pub struct KotlinParser {
    theory_meta: ExtractedTheoryMeta,
}

impl KotlinParser {
    /// Create a new Kotlin parser with auto-derived theory.
    ///
    /// # Panics
    ///
    /// Panics if theory extraction from the embedded `NODE_TYPES` fails.
    #[must_use]
    pub fn new() -> Self {
        let theory_meta = match extract_theory_from_node_types(
            "ThKotlinFullAST",
            include_bytes!("kotlin-node-types.json"),
        ) {
            Ok(meta) => meta,
            Err(e) => panic!("Kotlin grammar theory extraction failed: {e}"),
        };

        Self { theory_meta }
    }
}

impl Default for KotlinParser {
    fn default() -> Self {
        Self::new()
    }
}

impl AstParser for KotlinParser {
    fn protocol_name(&self) -> &'static str {
        "kotlin"
    }

    fn parse(
        &self,
        _source: &[u8],
        file_path: &str,
    ) -> Result<panproto_schema::Schema, ParseError> {
        Err(ParseError::TreeSitterParse {
            path: format!(
                "{file_path}: tree-sitter-kotlin 0.3.x depends on tree-sitter 0.20, \
                 incompatible with tree-sitter 0.24; awaiting grammar crate update to \
                 tree-sitter-language"
            ),
        })
    }

    fn emit(&self, _schema: &panproto_schema::Schema) -> Result<Vec<u8>, ParseError> {
        Err(ParseError::EmitFailed {
            protocol: "kotlin".to_owned(),
            reason: "tree-sitter-kotlin 0.3.x is incompatible; awaiting grammar crate update"
                .to_owned(),
        })
    }

    fn supported_extensions(&self) -> &[&str] {
        &["kt", "kts"]
    }

    fn theory_meta(&self) -> &ExtractedTheoryMeta {
        &self.theory_meta
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn kotlin_theory_extraction() {
        let parser = KotlinParser::new();
        let meta = parser.theory_meta();
        assert!(
            meta.vertex_kinds.len() > 40,
            "expected 40+ vertex kinds, got {}",
            meta.vertex_kinds.len()
        );
        // Kotlin's NODE_TYPES has fewer explicit fields than other languages.
        assert!(
            !meta.edge_kinds.is_empty(),
            "expected at least one edge kind, got {}",
            meta.edge_kinds.len()
        );
    }

    #[test]
    fn kotlin_parse_returns_error() {
        let parser = KotlinParser::new();
        let result = parser.parse(b"fun main() {}", "test.kt");
        assert!(result.is_err());
    }
}
