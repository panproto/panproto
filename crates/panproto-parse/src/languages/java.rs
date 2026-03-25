//! Java full-AST parser and emitter.

use crate::languages::common::LanguageParser;
use crate::walker::WalkerConfig;

/// Full-AST parser for Java source files.
pub struct JavaParser {
    inner: LanguageParser,
}

impl JavaParser {
    /// Create a new Java parser with auto-derived theory.
    ///
    /// # Panics
    ///
    /// Panics if theory extraction from the embedded `NODE_TYPES` fails.
    #[must_use]
    pub fn new() -> Self {
        let config = WalkerConfig {
            extra_scope_kinds: vec![
                "record_declaration".to_owned(),
                "annotation_type_declaration".to_owned(),
                "lambda_expression".to_owned(),
                "constructor_declaration".to_owned(),
            ],
            extra_block_kinds: vec![
                "switch_block".to_owned(),
                "annotation_argument_list".to_owned(),
                "element_value_array_initializer".to_owned(),
            ],
            name_fields: vec!["name".to_owned(), "identifier".to_owned()],
            capture_comments: true,
            capture_formatting: true,
        };

        let inner = LanguageParser::new(
            "java",
            vec!["java"],
            tree_sitter_java::LANGUAGE,
            tree_sitter_java::NODE_TYPES.as_bytes(),
            config,
        )
        .expect("Java grammar theory extraction must not fail");

        Self { inner }
    }
}

impl crate::registry::AstParser for JavaParser {
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
    fn parse_java_class() {
        let parser = JavaParser::new();
        let source = br#"
public class Calculator {
    private int memory;

    public Calculator() {
        this.memory = 0;
    }

    public int add(int a, int b) {
        return a + b;
    }

    public void store(int value) {
        this.memory = value;
    }
}
"#;
        let schema = parser.parse(source, "Calculator.java").unwrap();
        assert!(schema.vertices.len() > 20, "got {} vertices", schema.vertices.len());
    }

    #[test]
    fn java_theory_extraction() {
        let parser = JavaParser::new();
        let meta = parser.theory_meta();
        assert!(meta.vertex_kinds.len() > 60, "expected 60+ vertex kinds, got {}", meta.vertex_kinds.len());
    }
}
