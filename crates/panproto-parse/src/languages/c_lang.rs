//! C full-AST parser and emitter.

use crate::languages::common::LanguageParser;
use crate::walker::WalkerConfig;

/// Full-AST parser for C source files.
pub struct CParser {
    inner: LanguageParser,
}

impl CParser {
    /// Create a new C parser with auto-derived theory.
    ///
    /// # Panics
    ///
    /// Panics if theory extraction from the embedded `NODE_TYPES` fails.
    #[must_use]
    pub fn new() -> Self {
        let config = WalkerConfig {
            extra_scope_kinds: vec![
                "preproc_function_def".to_owned(),
                "preproc_ifdef".to_owned(),
            ],
            extra_block_kinds: vec![
                "case_statement".to_owned(),
                "initializer_list".to_owned(),
                "preproc_params".to_owned(),
            ],
            name_fields: vec![
                "name".to_owned(),
                "identifier".to_owned(),
                "declarator".to_owned(),
            ],
            capture_comments: true,
            capture_formatting: true,
        };

        let inner = LanguageParser::new(
            "c",
            vec!["c", "h"],
            tree_sitter_c::LANGUAGE,
            tree_sitter_c::NODE_TYPES.as_bytes(),
            config,
        )
        .expect("C grammar theory extraction must not fail");

        Self { inner }
    }
}

impl crate::registry::AstParser for CParser {
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
    fn parse_c_function() {
        let parser = CParser::new();
        let source = br#"
#include <stdio.h>
#include <stdlib.h>

typedef struct {
    int x;
    int y;
} Point;

int distance_squared(const Point* a, const Point* b) {
    int dx = a->x - b->x;
    int dy = a->y - b->y;
    return dx * dx + dy * dy;
}

int main(int argc, char** argv) {
    Point p1 = {0, 0};
    Point p2 = {3, 4};
    printf("Distance squared: %d\n", distance_squared(&p1, &p2));
    return 0;
}
"#;
        let schema = parser.parse(source, "main.c").unwrap();
        assert!(schema.vertices.len() > 25, "got {} vertices", schema.vertices.len());
    }

    #[test]
    fn c_theory_extraction() {
        let parser = CParser::new();
        let meta = parser.theory_meta();
        assert!(meta.vertex_kinds.len() > 40, "expected 40+ vertex kinds, got {}", meta.vertex_kinds.len());
    }
}
