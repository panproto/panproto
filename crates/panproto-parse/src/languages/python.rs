//! Python full-AST parser and emitter.

use crate::languages::common::LanguageParser;
use crate::walker::WalkerConfig;

/// Full-AST parser for Python source files.
pub struct PythonParser {
    inner: LanguageParser,
}

impl PythonParser {
    /// Create a new Python parser with auto-derived theory.
    ///
    /// # Panics
    ///
    /// Panics if theory extraction from the embedded `NODE_TYPES` fails.
    #[must_use]
    pub fn new() -> Self {
        let config = WalkerConfig {
            extra_scope_kinds: vec![
                "decorated_definition".to_owned(),
                "lambda".to_owned(),
                "list_comprehension".to_owned(),
                "dictionary_comprehension".to_owned(),
                "set_comprehension".to_owned(),
                "generator_expression".to_owned(),
            ],
            extra_block_kinds: vec![
                "argument_list".to_owned(),
                "expression_list".to_owned(),
                "pattern_list".to_owned(),
            ],
            name_fields: vec!["name".to_owned(), "identifier".to_owned()],
            capture_comments: true,
            capture_formatting: true,
        };

        let inner = match LanguageParser::new(
            "python",
            vec!["py", "pyi"],
            tree_sitter_python::LANGUAGE,
            tree_sitter_python::NODE_TYPES.as_bytes(),
            config,
        ) {
            Ok(v) => v,
            Err(e) => panic!("grammar theory extraction failed: {e}"),
        };

        Self { inner }
    }
}

impl Default for PythonParser {
    fn default() -> Self {
        Self::new()
    }
}
impl crate::registry::AstParser for PythonParser {
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
    fn parse_python_function() {
        let parser = PythonParser::new();
        let source = br"
def fibonacci(n: int) -> int:
    if n <= 1:
        return n
    return fibonacci(n - 1) + fibonacci(n - 2)
";
        let schema = parser.parse(source, "fib.py").unwrap();
        assert!(
            schema.vertices.len() > 10,
            "got {} vertices",
            schema.vertices.len()
        );
    }

    #[test]
    fn parse_python_class() {
        let parser = PythonParser::new();
        let source = br#"
from dataclasses import dataclass
from typing import Optional

@dataclass
class User:
    name: str
    age: int
    email: Optional[str] = None

    def greet(self) -> str:
        return f"Hello, I'm {self.name}"

    @staticmethod
    def create(name: str, age: int) -> "User":
        return User(name=name, age=age)
"#;
        let schema = parser.parse(source, "user.py").unwrap();
        assert!(
            schema.vertices.len() > 20,
            "got {} vertices",
            schema.vertices.len()
        );
    }

    #[test]
    fn parse_python_comprehensions() {
        let parser = PythonParser::new();
        let source = br"
squares = [x**2 for x in range(10) if x % 2 == 0]
mapping = {k: v for k, v in items.items() if v is not None}

async def process(items):
    results = await asyncio.gather(*[fetch(item) for item in items])
    return results
";
        let schema = parser.parse(source, "comp.py").unwrap();
        assert!(
            schema.vertices.len() > 15,
            "got {} vertices",
            schema.vertices.len()
        );
    }

    #[test]
    fn python_theory_extraction() {
        let parser = PythonParser::new();
        let meta = parser.theory_meta();
        assert!(
            meta.vertex_kinds.len() > 50,
            "expected 50+ vertex kinds, got {}",
            meta.vertex_kinds.len()
        );
        assert!(
            meta.edge_kinds.len() > 20,
            "expected 20+ edge kinds, got {}",
            meta.edge_kinds.len()
        );
    }

    #[test]
    fn emit_roundtrip_python() {
        let parser = PythonParser::new();
        let source = b"def add(a, b):\n    return a + b\n";
        let schema = parser.parse(source, "add.py").unwrap();
        let emitted = parser.emit(&schema).unwrap();
        assert_eq!(
            std::str::from_utf8(&emitted).unwrap(),
            std::str::from_utf8(source).unwrap(),
            "Python emit(parse(source)) should reproduce the original source"
        );
    }
}
