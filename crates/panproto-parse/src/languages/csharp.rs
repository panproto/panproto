//! C# full-AST parser and emitter.

use crate::languages::common::LanguageParser;
use crate::walker::WalkerConfig;

/// Full-AST parser for C# source files.
pub struct CSharpParser {
    inner: LanguageParser,
}

impl CSharpParser {
    /// Create a new C# parser with auto-derived theory.
    ///
    /// # Panics
    ///
    /// Panics if theory extraction from the embedded `NODE_TYPES` fails.
    #[must_use]
    pub fn new() -> Self {
        let config = WalkerConfig {
            extra_scope_kinds: vec![
                "record_declaration".to_owned(),
                "namespace_declaration".to_owned(),
                "lambda_expression".to_owned(),
                "local_function_statement".to_owned(),
                "property_declaration".to_owned(),
            ],
            extra_block_kinds: vec![
                "switch_section".to_owned(),
                "accessor_list".to_owned(),
                "attribute_list".to_owned(),
            ],
            name_fields: vec!["name".to_owned(), "identifier".to_owned()],
            capture_comments: true,
            capture_formatting: true,
        };

        let inner = LanguageParser::new(
            "csharp",
            vec!["cs"],
            tree_sitter_c_sharp::LANGUAGE,
            tree_sitter_c_sharp::NODE_TYPES.as_bytes(),
            config,
        )
        .expect("C# grammar theory extraction must not fail");

        Self { inner }
    }
}

impl crate::registry::AstParser for CSharpParser {
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
    fn parse_csharp_class() {
        let parser = CSharpParser::new();
        let source = br#"
using System;

namespace MyApp
{
    public class Calculator
    {
        public int Add(int a, int b) => a + b;

        public async Task<string> FetchAsync(string url)
        {
            var client = new HttpClient();
            return await client.GetStringAsync(url);
        }
    }
}
"#;
        let schema = parser.parse(source, "Calculator.cs").unwrap();
        assert!(schema.vertices.len() > 20, "got {} vertices", schema.vertices.len());
    }

    #[test]
    fn csharp_theory_extraction() {
        let parser = CSharpParser::new();
        let meta = parser.theory_meta();
        assert!(meta.vertex_kinds.len() > 80, "expected 80+ vertex kinds, got {}", meta.vertex_kinds.len());
    }
}
