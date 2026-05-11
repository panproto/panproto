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

    /// Render a by-construction [`Schema`] (one with no parse-recovered
    /// byte positions or interstitials) to source bytes.
    ///
    /// Unlike [`emit`](Self::emit), which reconstructs source from
    /// byte-position fragments stored on the schema during `parse`,
    /// `emit_pretty` walks tree-sitter `grammar.json` production rules
    /// to render schemas built from scratch via `SchemaBuilder`.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::EmitFailed`] when the language has no
    /// vendored `grammar.json`, when a vertex's kind is not a grammar
    /// rule, or when a required field has no corresponding schema edge.
    fn emit_pretty(&self, schema: &Schema) -> Result<Vec<u8>, ParseError> {
        let _ = schema;
        Err(ParseError::EmitFailed {
            protocol: self.protocol_name().to_owned(),
            reason: format!(
                "emit_pretty not implemented for protocol '{}'",
                self.protocol_name()
            ),
        })
    }
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
    /// Create a new registry populated with all enabled language parsers.
    ///
    /// With the `grammars` feature (default), this populates the registry from
    /// `panproto-grammars`, which provides up to 248 tree-sitter languages.
    /// Without the `grammars` feature, this returns an empty registry; call
    /// [`register`](Self::register) to add parsers manually using individual
    /// grammar crates.
    #[must_use]
    pub fn new() -> Self {
        let mut registry = Self {
            parsers: FxHashMap::default(),
            extension_map: FxHashMap::default(),
        };

        #[cfg(feature = "grammars")]
        for grammar in panproto_grammars::grammars() {
            let config = crate::languages::walker_configs::walker_config_for(grammar.name);
            match crate::languages::common::LanguageParser::from_language_with_grammar_json(
                grammar.name,
                grammar.extensions.to_vec(),
                grammar.language,
                grammar.node_types,
                grammar.tags_query,
                config,
                grammar.grammar_json,
            ) {
                Ok(p) => registry.register(Box::new(p)),
                Err(err) => {
                    let _ = err;
                    #[cfg(debug_assertions)]
                    eprintln!(
                        "warning: grammar '{}' theory extraction failed: {err}",
                        grammar.name
                    );
                }
            }
        }

        registry
    }

    /// Register a parser implementation.
    pub fn register(&mut self, parser: Box<dyn AstParser>) {
        let name = parser.protocol_name().to_owned();
        for ext in parser.supported_extensions() {
            self.extension_map.insert((*ext).to_owned(), name.clone());
        }
        self.parsers.insert(name, parser);
    }

    /// Register a tree-sitter language as a full-AST parser.
    ///
    /// Used by `panproto-grammars-*` companion crates that ship grammars
    /// outside the default `panproto-grammars` build. The byte-slice
    /// arguments must outlive this registry; the canonical pattern is
    /// for the companion to bake the data into `&'static` rodata at
    /// compile time and pass references that are valid for the process
    /// lifetime.
    ///
    /// `walker_config` is looked up by `name` from the bundled per-language
    /// configuration table. Languages without a tailored configuration
    /// fall back to the default walker config.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] if theory extraction from `node_types_json`
    /// fails or if the tags query rejects compilation.
    pub fn register_external_grammar(
        &mut self,
        name: &'static str,
        extensions: Vec<&'static str>,
        language: tree_sitter::Language,
        node_types_json: &'static [u8],
        tags_query: Option<&'static str>,
        grammar_json: Option<&'static [u8]>,
    ) -> Result<(), crate::error::ParseError> {
        let config = crate::languages::walker_configs::walker_config_for(name);
        let parser = crate::languages::common::LanguageParser::from_language_with_grammar_json(
            name,
            extensions,
            language,
            node_types_json,
            tags_query,
            config,
            grammar_json,
        )?;
        self.register(Box::new(parser));
        Ok(())
    }

    /// Owned-data variant of [`register_external_grammar`](Self::register_external_grammar).
    ///
    /// Accepts `String` / `Vec<u8>` rather than `&'static` references. The
    /// caller is presumed not to have process-lifetime rodata available
    /// (typical dev-time use: bytes read from disk via the Python binding's
    /// override hook). To match the trait's `'static` lifetime requirement
    /// the inputs are leaked into the heap; the leak is one-time per
    /// override.
    ///
    /// This is the registration primitive for grammar-author workflows
    /// where a grammar's `parser.c` / `grammar.json` / `node-types.json`
    /// are evolving outside the panproto release cadence. Production
    /// builds should continue to use [`register_external_grammar`] with
    /// `'static` data baked into the binary at compile time.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] if theory extraction or tags-query
    /// compilation fails.
    pub fn register_external_grammar_owned(
        &mut self,
        name: String,
        extensions: Vec<String>,
        language: tree_sitter::Language,
        node_types_json: Vec<u8>,
        tags_query: Option<String>,
        grammar_json: Option<Vec<u8>>,
    ) -> Result<(), crate::error::ParseError> {
        let name_static: &'static str = Box::leak(name.into_boxed_str());
        let extensions_static: Vec<&'static str> = extensions
            .into_iter()
            .map(|s| Box::leak(s.into_boxed_str()) as &'static str)
            .collect();
        let node_types_static: &'static [u8] = Box::leak(node_types_json.into_boxed_slice());
        let tags_query_static: Option<&'static str> =
            tags_query.map(|s| Box::leak(s.into_boxed_str()) as &'static str);
        let grammar_json_static: Option<&'static [u8]> =
            grammar_json.map(|v| Box::leak(v.into_boxed_slice()) as &'static [u8]);

        self.register_external_grammar(
            name_static,
            extensions_static,
            language,
            node_types_static,
            tags_query_static,
            grammar_json_static,
        )
    }

    /// Remove a registration by protocol name.
    ///
    /// Drops the parser and any extension mappings that pointed at it.
    /// Returns `true` if a parser was removed, `false` if no such
    /// registration existed. Primarily intended for grammar-author
    /// workflows where a registered grammar is being replaced by a
    /// freshly-compiled version mid-process.
    pub fn unregister(&mut self, name: &str) -> bool {
        let removed = self.parsers.remove(name).is_some();
        if removed {
            self.extension_map.retain(|_, v| v != name);
        }
        removed
    }

    /// Override a registered grammar with new owned data.
    ///
    /// Equivalent to [`unregister`](Self::unregister) followed by
    /// [`register_external_grammar_owned`](Self::register_external_grammar_owned),
    /// and intended for the same grammar-author dev workflow. Any
    /// extension mappings previously bound to `name` are replaced by
    /// the new `extensions`.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] if theory extraction or tags-query
    /// compilation fails on the new grammar; in that case the prior
    /// registration is already gone.
    pub fn override_grammar(
        &mut self,
        name: String,
        extensions: Vec<String>,
        language: tree_sitter::Language,
        node_types_json: Vec<u8>,
        tags_query: Option<String>,
        grammar_json: Option<Vec<u8>>,
    ) -> Result<(), crate::error::ParseError> {
        self.unregister(&name);
        self.register_external_grammar_owned(
            name,
            extensions,
            language,
            node_types_json,
            tags_query,
            grammar_json,
        )
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
        let protocol = self
            .detect_language(path)
            .ok_or_else(|| ParseError::UnknownLanguage {
                extension: path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_owned(),
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
        let parser = self
            .parsers
            .get(protocol)
            .ok_or_else(|| ParseError::UnknownLanguage {
                extension: protocol.to_owned(),
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
        let parser = self
            .parsers
            .get(protocol)
            .ok_or_else(|| ParseError::UnknownLanguage {
                extension: protocol.to_owned(),
            })?;

        parser.emit(schema)
    }

    /// Render a by-construction schema using the named protocol.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::UnknownLanguage`] if the protocol is not
    /// registered, or [`ParseError::EmitFailed`] from the underlying
    /// parser's `emit_pretty`.
    pub fn emit_pretty_with_protocol(
        &self,
        protocol: &str,
        schema: &Schema,
    ) -> Result<Vec<u8>, ParseError> {
        let parser = self
            .parsers
            .get(protocol)
            .ok_or_else(|| ParseError::UnknownLanguage {
                extension: protocol.to_owned(),
            })?;

        parser.emit_pretty(schema)
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

    /// O(1) lookup: is a parser already registered for `protocol`?
    ///
    /// Useful for dedup at the registration boundary. The umbrella
    /// `panproto-grammars-all` companion pack overlaps with both the
    /// built-in core grammars and every per-group pack; callers can
    /// short-circuit before re-registering rather than scanning
    /// `protocol_names()` linearly.
    #[must_use]
    pub fn has_parser(&self, protocol: &str) -> bool {
        self.parsers.contains_key(protocol)
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
