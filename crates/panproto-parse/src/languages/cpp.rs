//! C++ full-AST parser and emitter.

use crate::languages::common::LanguageParser;
use crate::walker::WalkerConfig;

/// Full-AST parser for C++ source files.
pub struct CppParser {
    inner: LanguageParser,
}

impl CppParser {
    /// Create a new C++ parser with auto-derived theory.
    ///
    /// # Panics
    ///
    /// Panics if theory extraction from the embedded `NODE_TYPES` fails.
    #[must_use]
    pub fn new() -> Self {
        let config = WalkerConfig {
            extra_scope_kinds: vec![
                "template_declaration".to_owned(),
                "namespace_definition".to_owned(),
                "lambda_expression".to_owned(),
                "concept_definition".to_owned(),
            ],
            extra_block_kinds: vec![
                "case_statement".to_owned(),
                "initializer_list".to_owned(),
                "template_argument_list".to_owned(),
                "base_class_clause".to_owned(),
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
            "cpp",
            vec!["cpp", "cc", "cxx", "hpp", "hh", "hxx"],
            tree_sitter_cpp::LANGUAGE,
            tree_sitter_cpp::NODE_TYPES.as_bytes(),
            config,
        )
        .expect("C++ grammar theory extraction must not fail");

        Self { inner }
    }
}

impl crate::registry::AstParser for CppParser {
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
    fn parse_cpp_class() {
        let parser = CppParser::new();
        let source = br#"
#include <string>
#include <vector>
#include <memory>

template<typename T>
class Stack {
public:
    void push(T value) {
        data_.push_back(std::move(value));
    }

    T pop() {
        T value = std::move(data_.back());
        data_.pop_back();
        return value;
    }

    bool empty() const {
        return data_.empty();
    }

private:
    std::vector<T> data_;
};

int main() {
    auto stack = std::make_unique<Stack<int>>();
    stack->push(42);
    return stack->pop();
}
"#;
        let schema = parser.parse(source, "stack.cpp").unwrap();
        assert!(schema.vertices.len() > 30, "got {} vertices", schema.vertices.len());
    }

    #[test]
    fn cpp_theory_extraction() {
        let parser = CppParser::new();
        let meta = parser.theory_meta();
        assert!(meta.vertex_kinds.len() > 60, "expected 60+ vertex kinds, got {}", meta.vertex_kinds.len());
    }
}
