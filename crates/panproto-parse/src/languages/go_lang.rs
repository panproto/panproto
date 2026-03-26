//! Go full-AST parser and emitter.

use crate::languages::common::LanguageParser;
use crate::walker::WalkerConfig;

/// Full-AST parser for Go source files.
pub struct GoParser {
    inner: LanguageParser,
}

impl GoParser {
    /// Create a new Go parser with auto-derived theory.
    ///
    /// # Panics
    ///
    /// Panics if theory extraction from the embedded `NODE_TYPES` fails.
    #[must_use]
    pub fn new() -> Self {
        let config = WalkerConfig {
            extra_scope_kinds: vec![
                "func_literal".to_owned(),
                "method_declaration".to_owned(),
                "type_declaration".to_owned(),
            ],
            extra_block_kinds: vec![
                "communication_case".to_owned(),
                "type_case".to_owned(),
                "expression_case".to_owned(),
                "default_case".to_owned(),
            ],
            name_fields: vec!["name".to_owned(), "identifier".to_owned()],
            capture_comments: true,
            capture_formatting: true,
        };

        let inner = match LanguageParser::new(
            "go",
            vec!["go"],
            tree_sitter_go::LANGUAGE,
            tree_sitter_go::NODE_TYPES.as_bytes(),
            config,
        ) {
            Ok(v) => v,
            Err(e) => panic!("grammar theory extraction failed: {e}"),
        };

        Self { inner }
    }
}

impl Default for GoParser {
    fn default() -> Self {
        Self::new()
    }
}
impl crate::registry::AstParser for GoParser {
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
    fn parse_go_function() {
        let parser = GoParser::new();
        let source = br#"
package main

import "fmt"

func fibonacci(n int) int {
    if n <= 1 {
        return n
    }
    return fibonacci(n-1) + fibonacci(n-2)
}

func main() {
    for i := 0; i < 10; i++ {
        fmt.Println(fibonacci(i))
    }
}
"#;
        let schema = parser.parse(source, "main.go").unwrap();
        assert!(
            schema.vertices.len() > 20,
            "got {} vertices",
            schema.vertices.len()
        );
    }

    #[test]
    fn go_theory_extraction() {
        let parser = GoParser::new();
        let meta = parser.theory_meta();
        assert!(
            meta.vertex_kinds.len() > 50,
            "expected 50+ vertex kinds, got {}",
            meta.vertex_kinds.len()
        );
    }
}
