//! TypeScript full-AST parser and emitter.

use crate::languages::common::LanguageParser;
use crate::walker::WalkerConfig;

/// Full-AST parser for TypeScript source files.
pub struct TypeScriptParser {
    inner: LanguageParser,
}

impl TypeScriptParser {
    /// Create a new TypeScript parser with auto-derived theory.
    ///
    /// # Panics
    ///
    /// Panics if theory extraction from the embedded `NODE_TYPES` fails,
    /// which would indicate a corrupted grammar crate.
    #[must_use]
    pub fn new() -> Self {
        let config = WalkerConfig {
            extra_scope_kinds: vec![
                "arrow_function".to_owned(),
                "generator_function_declaration".to_owned(),
                "type_alias_declaration".to_owned(),
            ],
            extra_block_kinds: vec!["switch_body".to_owned(), "template_string".to_owned()],
            name_fields: vec!["name".to_owned(), "identifier".to_owned()],
            capture_comments: true,
            capture_formatting: true,
        };

        let inner = match LanguageParser::new(
            "typescript",
            vec!["ts"],
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            tree_sitter_typescript::TYPESCRIPT_NODE_TYPES.as_bytes(),
            config,
        ) {
            Ok(v) => v,
            Err(e) => panic!("grammar theory extraction failed: {e}"),
        };

        Self { inner }
    }
}

impl Default for TypeScriptParser {
    fn default() -> Self {
        Self::new()
    }
}
impl crate::registry::AstParser for TypeScriptParser {
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

/// Full-AST parser for TSX source files.
pub struct TsxParser {
    inner: LanguageParser,
}

impl TsxParser {
    /// Create a new TSX parser with auto-derived theory.
    ///
    /// # Panics
    ///
    /// Panics if theory extraction from the embedded `NODE_TYPES` fails.
    #[must_use]
    pub fn new() -> Self {
        let config = WalkerConfig {
            extra_scope_kinds: vec![
                "arrow_function".to_owned(),
                "jsx_element".to_owned(),
                "jsx_self_closing_element".to_owned(),
            ],
            extra_block_kinds: vec!["switch_body".to_owned(), "jsx_expression".to_owned()],
            name_fields: vec!["name".to_owned(), "identifier".to_owned()],
            capture_comments: true,
            capture_formatting: true,
        };

        let inner = match LanguageParser::new(
            "tsx",
            vec!["tsx"],
            tree_sitter_typescript::LANGUAGE_TSX,
            tree_sitter_typescript::TSX_NODE_TYPES.as_bytes(),
            config,
        ) {
            Ok(v) => v,
            Err(e) => panic!("grammar theory extraction failed: {e}"),
        };

        Self { inner }
    }
}

impl Default for TsxParser {
    fn default() -> Self {
        Self::new()
    }
}
impl crate::registry::AstParser for TsxParser {
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
    fn parse_typescript_function() {
        let parser = TypeScriptParser::new();
        let source = br#"
function greet(name: string): string {
    return "Hello, " + name;
}

const add = (a: number, b: number): number => a + b;
"#;
        let schema = parser.parse(source, "test.ts").unwrap();
        assert!(
            schema.vertices.len() > 10,
            "expected rich AST, got {} vertices",
            schema.vertices.len()
        );
    }

    #[test]
    fn parse_typescript_class() {
        let parser = TypeScriptParser::new();
        let source = br"
class User {
    private name: string;
    private age: number;

    constructor(name: string, age: number) {
        this.name = name;
        this.age = age;
    }

    greet(): string {
        return `Hello, I'm ${this.name}`;
    }
}
";
        let schema = parser.parse(source, "user.ts").unwrap();
        assert!(
            schema.vertices.len() > 20,
            "expected rich class AST, got {} vertices",
            schema.vertices.len()
        );
    }

    #[test]
    fn parse_typescript_interface_and_enum() {
        let parser = TypeScriptParser::new();
        let source = br#"
interface Shape {
    area(): number;
    perimeter(): number;
}

enum Color {
    Red = "red",
    Green = "green",
    Blue = "blue",
}

type Result<T> = { ok: true; value: T } | { ok: false; error: string };
"#;
        let schema = parser.parse(source, "types.ts").unwrap();
        assert!(
            schema.vertices.len() > 15,
            "expected rich type AST, got {} vertices",
            schema.vertices.len()
        );
    }

    #[test]
    fn parse_typescript_control_flow() {
        let parser = TypeScriptParser::new();
        let source = br"
function process(items: string[]): void {
    for (const item of items) {
        if (item.length > 5) {
            console.log(item);
        } else {
            continue;
        }
    }

    try {
        doSomething();
    } catch (e) {
        handleError(e);
    } finally {
        cleanup();
    }
}
";
        let schema = parser.parse(source, "flow.ts").unwrap();
        assert!(
            schema.vertices.len() > 25,
            "expected rich control flow AST, got {} vertices",
            schema.vertices.len()
        );
    }

    #[test]
    fn parse_typescript_imports() {
        let parser = TypeScriptParser::new();
        let source = br"
import { useState, useEffect } from 'react';
import type { User } from './types';

export function App(): JSX.Element {
    const [count, setCount] = useState(0);
    return count;
}
";
        let schema = parser.parse(source, "app.ts").unwrap();
        assert!(
            schema.vertices.len() > 10,
            "expected import/export AST, got {} vertices",
            schema.vertices.len()
        );
    }

    #[test]
    fn typescript_theory_extraction() {
        let parser = TypeScriptParser::new();
        let meta = parser.theory_meta();
        // TypeScript grammar has ~180+ named node types.
        assert!(
            meta.vertex_kinds.len() > 100,
            "expected 100+ vertex kinds, got {}",
            meta.vertex_kinds.len()
        );
        // TypeScript grammar has ~60+ field names.
        assert!(
            meta.edge_kinds.len() > 30,
            "expected 30+ edge kinds, got {}",
            meta.edge_kinds.len()
        );
    }

    #[test]
    fn parse_tsx_jsx() {
        let parser = TsxParser::new();
        let source = br#"
function App() {
    return (
        <div className="app">
            <h1>Hello World</h1>
            <button onClick={() => alert('clicked')}>Click me</button>
        </div>
    );
}
"#;
        let schema = parser.parse(source, "app.tsx").unwrap();
        assert!(
            schema.vertices.len() > 15,
            "expected rich JSX AST, got {} vertices",
            schema.vertices.len()
        );
    }

    #[test]
    fn emit_roundtrip_typescript() {
        let parser = TypeScriptParser::new();
        let source = b"function add(a: number, b: number): number {\n    return a + b;\n}\n";
        let schema = parser.parse(source, "add.ts").unwrap();
        let emitted = parser.emit(&schema).unwrap();
        assert_eq!(
            std::str::from_utf8(&emitted).unwrap(),
            std::str::from_utf8(source).unwrap(),
            "emit(parse(source)) should reproduce the original source"
        );
    }

    #[test]
    fn emit_roundtrip_complex() {
        let parser = TypeScriptParser::new();
        let source = br"interface Shape {
    area(): number;
}

class Circle implements Shape {
    constructor(private radius: number) {}

    area(): number {
        return Math.PI * this.radius * this.radius;
    }
}
";
        let schema = parser.parse(source, "shape.ts").unwrap();
        let emitted = parser.emit(&schema).unwrap();
        assert_eq!(
            std::str::from_utf8(&emitted).unwrap(),
            std::str::from_utf8(source).unwrap(),
            "complex TypeScript should round-trip through parse/emit"
        );
    }
}
