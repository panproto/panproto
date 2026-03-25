//! Parser registry mapping protocol names to full-AST parser implementations.

use std::path::Path;

use panproto_schema::Schema;
use rustc_hash::FxHashMap;

use crate::error::ParseError;
use crate::theory_extract::ExtractedTheoryMeta;

/// A full-AST parser and emitter for a specific programming language.
///
/// Each implementation wraps a tree-sitter grammar and its auto-derived theory,
/// providing parse (source → Schema) and emit (Schema → source) operations.
pub trait AstParser: Send + Sync {
    /// The panproto protocol name (e.g. `"typescript"`, `"python"`).
    fn protocol_name(&self) -> &str;

    /// Parse source code into a full-AST [`Schema`].
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] if tree-sitter parsing fails or schema construction fails.
    fn parse(&self, source: &[u8], file_path: &str) -> Result<Schema, ParseError>;

    /// Emit a [`Schema`] back to source code bytes.
    ///
    /// The emitter walks the schema graph top-down, using formatting constraints
    /// (comment, indent, blank-lines-before) to reproduce the original formatting.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::EmitFailed`] if emission fails.
    fn emit(&self, schema: &Schema) -> Result<Vec<u8>, ParseError>;

    /// File extensions this parser handles (e.g. `["ts", "tsx"]`).
    fn supported_extensions(&self) -> &[&str];

    /// The auto-derived theory metadata for this language.
    fn theory_meta(&self) -> &ExtractedTheoryMeta;
}

/// Registry of all full-AST parsers, keyed by protocol name.
///
/// Provides language detection by file extension and dispatches parse/emit
/// operations to the appropriate language parser.
pub struct ParserRegistry {
    /// Parsers keyed by protocol name.
    parsers: FxHashMap<String, Box<dyn AstParser>>,
    /// Extension → protocol name mapping.
    extension_map: FxHashMap<String, String>,
}

impl ParserRegistry {
    /// Create a new registry populated with all built-in language parsers.
    #[must_use]
    pub fn new() -> Self {
        let mut registry = Self {
            parsers: FxHashMap::default(),
            extension_map: FxHashMap::default(),
        };

        // Register all 10 language parsers.
        registry.register(Box::new(crate::languages::typescript::TypeScriptParser::new()));
        registry.register(Box::new(crate::languages::python::PythonParser::new()));
        registry.register(Box::new(crate::languages::rust_lang::RustParser::new()));
        registry.register(Box::new(crate::languages::java::JavaParser::new()));
        registry.register(Box::new(crate::languages::go_lang::GoParser::new()));
        registry.register(Box::new(crate::languages::swift::SwiftParser::new()));
        registry.register(Box::new(crate::languages::kotlin::KotlinParser::new()));
        registry.register(Box::new(crate::languages::csharp::CSharpParser::new()));
        registry.register(Box::new(crate::languages::c_lang::CParser::new()));
        registry.register(Box::new(crate::languages::cpp::CppParser::new()));

        registry
    }

    /// Register a parser implementation.
    pub fn register(&mut self, parser: Box<dyn AstParser>) {
        let name = parser.protocol_name().to_owned();
        for ext in parser.supported_extensions() {
            self.extension_map
                .insert((*ext).to_owned(), name.clone());
        }
        self.parsers.insert(name, parser);
    }

    /// Detect the language protocol for a file path by its extension.
    ///
    /// Returns `None` if the extension is not recognized (caller should
    /// fall back to the `raw_file` protocol).
    #[must_use]
    pub fn detect_language(&self, path: &Path) -> Option<&str> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| self.extension_map.get(ext))
            .map(String::as_str)
    }

    /// Parse a file by detecting its language from the file path.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::UnknownLanguage`] if the file extension is not recognized.
    /// Returns other [`ParseError`] variants if parsing fails.
    pub fn parse_file(&self, path: &Path, content: &[u8]) -> Result<Schema, ParseError> {
        let protocol = self.detect_language(path).ok_or_else(|| {
            ParseError::UnknownLanguage {
                extension: path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_owned(),
            }
        })?;

        self.parse_with_protocol(protocol, content, &path.display().to_string())
    }

    /// Parse source code with a specific protocol name.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::UnknownLanguage`] if the protocol is not registered.
    pub fn parse_with_protocol(
        &self,
        protocol: &str,
        content: &[u8],
        file_path: &str,
    ) -> Result<Schema, ParseError> {
        let parser = self.parsers.get(protocol).ok_or_else(|| {
            ParseError::UnknownLanguage {
                extension: protocol.to_owned(),
            }
        })?;

        parser.parse(content, file_path)
    }

    /// Emit a schema back to source code bytes using the specified protocol.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::UnknownLanguage`] if the protocol is not registered.
    pub fn emit_with_protocol(
        &self,
        protocol: &str,
        schema: &Schema,
    ) -> Result<Vec<u8>, ParseError> {
        let parser = self.parsers.get(protocol).ok_or_else(|| {
            ParseError::UnknownLanguage {
                extension: protocol.to_owned(),
            }
        })?;

        parser.emit(schema)
    }

    /// Get the theory metadata for a specific protocol.
    #[must_use]
    pub fn theory_meta(&self, protocol: &str) -> Option<&ExtractedTheoryMeta> {
        self.parsers.get(protocol).map(|p| p.theory_meta())
    }

    /// List all registered protocol names.
    pub fn protocol_names(&self) -> impl Iterator<Item = &str> {
        self.parsers.keys().map(String::as_str)
    }

    /// Get the number of registered parsers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.parsers.len()
    }

    /// Check if the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parsers.is_empty()
    }
}

impl Default for ParserRegistry {
    fn default() -> Self {
        Self::new()
    }
}
