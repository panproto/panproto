//! Parser registry mapping protocol names to full-AST parser implementations.

use std::path::Path;
use std::sync::Arc;

use panproto_schema::{AbstractSchema, DecoratedSchema, Schema};
use rustc_hash::FxHashMap;

use crate::error::ParseError;
use crate::layout_policy::LayoutPolicy;
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
        self.emit_pretty_with_policy(schema, &crate::emit_pretty::FormatPolicy::default())
    }

    /// Render a by-construction [`Schema`] under a caller-supplied
    /// [`FormatPolicy`](crate::emit_pretty::FormatPolicy).
    ///
    /// The policy governs every configurable aspect of the rendered
    /// output: separator between glued tokens, newline byte sequence,
    /// indent width, line-break and indent-open/close token sets. The
    /// default policy (used by [`emit_pretty`](Self::emit_pretty))
    /// targets syntactic validity with ASCII conventions; callers
    /// supplying their own policy can pin idiomatic formatting.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::EmitFailed`] when the language has no
    /// vendored `grammar.json`, when a vertex's kind is not a grammar
    /// rule, or when a required field has no corresponding schema edge.
    fn emit_pretty_with_policy(
        &self,
        schema: &Schema,
        policy: &crate::emit_pretty::FormatPolicy,
    ) -> Result<Vec<u8>, ParseError> {
        let _ = (schema, policy);
        Err(ParseError::EmitFailed {
            protocol: self.protocol_name().to_owned(),
            reason: format!(
                "emit_pretty_with_policy not implemented for protocol '{}'",
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
    ///
    /// Held by `Arc` (not `Box`) so the same handle can be shared with
    /// the layout-enrichment registry without re-wrapping at every
    /// lookup. Registration installs both: the parser into `parsers`
    /// and a thin adapter into the lens crate's enrichment registry.
    parsers: FxHashMap<String, Arc<dyn AstParser>>,
    /// Extension → protocol name mapping.
    extension_map: FxHashMap<String, String>,
}

impl ParserRegistry {
    /// Create a new registry populated with all enabled language parsers.
    ///
    /// With the `grammars` feature (default), this populates the registry from
    /// `panproto-grammars`, which provides up to 259 tree-sitter languages.
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
    ///
    /// In addition to keying the parser by its protocol name, this
    /// installs a [`LayoutEnricher`](panproto_lens::enrichment_registry::LayoutEnricher)
    /// adapter into the global enrichment registry so that a
    /// `parse_emit_protolens(protocol, …)` instantiation finds a
    /// synthesis driver without any further wiring.
    pub fn register(&mut self, parser: Box<dyn AstParser>) {
        let name = parser.protocol_name().to_owned();
        for ext in parser.supported_extensions() {
            self.extension_map.insert((*ext).to_owned(), name.clone());
        }
        let arc: Arc<dyn AstParser> = Arc::from(parser);
        crate::decorate::register_layout_enricher(Arc::clone(&arc));
        self.parsers.insert(name, arc);
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
    /// builds should continue to use [`register_external_grammar`](Self::register_external_grammar) with
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

    /// Report the test-verification status of `emit_pretty` for a
    /// given protocol.
    ///
    /// The status is a programmatic check that downstream tooling
    /// (e.g. quivers, schema-migration pipelines) can use to refuse
    /// emit on protocols whose fixed-point law has never been
    /// exercised by panproto's test suite. The three tiers are:
    ///
    /// * [`EmitVerificationStatus::Verified`] — the protocol has an
    ///   explicit fixed-point or roundtrip test in panproto's suite.
    ///   `emit_pretty(parse(emit_pretty(s))) == emit_pretty(s)` is
    ///   known to hold on representative source.
    /// * [`EmitVerificationStatus::Generic`] — the protocol is
    ///   registered (a tree-sitter grammar is vendored) and the
    ///   generic dispatch path applies, but no per-language test
    ///   asserts emit correctness. Output is structurally derived
    ///   from `grammar.json` + the universal cassette layer and is
    ///   likely correct, but unverified.
    /// * [`EmitVerificationStatus::Unsupported`] — the protocol is
    ///   not registered, OR is registered but no `grammar.json` was
    ///   vendored at build time. `emit_pretty` will return
    ///   [`ParseError::EmitFailed`].
    #[must_use]
    pub fn emit_verification_status(&self, protocol: &str) -> EmitVerificationStatus {
        if !self.parsers.contains_key(protocol) {
            return EmitVerificationStatus::Unsupported;
        }
        if VERIFIED_EMIT_PROTOCOLS.binary_search(&protocol).is_ok() {
            EmitVerificationStatus::Verified
        } else {
            EmitVerificationStatus::Generic
        }
    }
}

/// Programmatic verification tier for [`ParserRegistry::emit_verification_status`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmitVerificationStatus {
    /// `emit_pretty` for this protocol has a test in panproto's suite
    /// asserting the fixed-point law on representative source.
    Verified,
    /// The protocol is registered and the generic dispatch path
    /// applies, but no per-language test asserts emit correctness.
    Generic,
    /// The protocol is not registered, or its grammar lacks the
    /// vendored `grammar.json` that `emit_pretty` requires.
    Unsupported,
}

/// Protocols whose `emit_pretty` is verified to a bar that justifies
/// downstream trust. A protocol qualifies on one of two bases:
///
/// 1. **Corpus-verified** — every entry in the grammar author's own
///    `test/corpus/` round-trips under the full oracle (byte fixed point +
///    vertex-kind multiset + edge-shape multiset), checked by the strict
///    `emit_corpus_audit` test. This is the strong bar: the corpus exercises
///    the whole grammar, not one hand-written sample.
/// 2. **Backend-verified** — a quivers transpile backend (`python`, `stan`,
///    `bugs`, `jags`, `julia`, `scheme`, `javascript`) covered by dedicated
///    emit regression tests for the construct surface quivers actually emits.
///    These are pinned by `emit_verification_status` tests as a downstream
///    contract; bringing each to full corpus-pass (basis 1) is tracked work.
///
/// A single hand-written round-trip sample is NOT sufficient: an earlier
/// expansion to 149 protocols on minimal samples was reverted after a corpus
/// audit showed most failed their own grammar's test corpus.
///
/// Names MUST be kept in sorted order so the binary-search lookup in
/// [`ParserRegistry::emit_verification_status`] works.
const VERIFIED_EMIT_PROTOCOLS: &[&str] = &[
    "arduino",
    "bass",
    "bugs",
    "chuck",
    "elisp",
    "fidl",
    "firrtl",
    "go",
    "graphql",
    "gstlaunch",
    "html",
    "jags",
    "janet",
    "javascript",
    "json",
    "julia",
    "python",
    "qmldir",
    "rego",
    "scheme",
    "sparql",
    "stan",
    "ungrammar",
];

impl ParserRegistry {
    /// Decorate an [`AbstractSchema`] with the layout enrichment
    /// fibre required by `emit_pretty_with_protocol` and friends.
    ///
    /// This is the put-direction of the parse / decorate / emit lens
    /// at `protocol`. The implementation routes through the same
    /// grammar walker as `emit_pretty` followed by `parse`, so the
    /// resulting [`DecoratedSchema`] carries a complete layout fibre
    /// recovered by the parse-side walker — `start-byte`, `end-byte`,
    /// every `interstitial-N`, `chose-alt-fingerprint`, and
    /// `chose-alt-child-kinds`.
    ///
    /// The section law holds up to kind- and edge-multiset
    /// equivalence: `forget_layout(decorate(a)) ≅ a` modulo vertex-id
    /// renaming. Grammars where parsing consolidates tokens that the
    /// emitter rendered as separate sequences (e.g. lilypond's `c'4`
    /// re-parses to a single note) do not preserve a one-to-one
    /// vertex correspondence, so the result's vertex IDs are always
    /// freshly minted by the parser.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::UnknownLanguage`] when `protocol` is not
    /// registered, [`ParseError::SchemaConstruction`] when the
    /// abstract schema was built for a different protocol than
    /// `protocol`, [`ParseError::EmitFailed`] when the grammar walker
    /// cannot render the abstract schema (missing `grammar.json`,
    /// vertex kind not a rule), or any other parser error if the
    /// re-parse step rejects the canonical bytes (a regression in the
    /// parse/emit pipeline, not a user bug).
    pub fn decorate(
        &self,
        protocol: &str,
        abstract_schema: &AbstractSchema,
        policy: &LayoutPolicy,
    ) -> Result<DecoratedSchema, ParseError> {
        let parser = self
            .parsers
            .get(protocol)
            .ok_or_else(|| ParseError::UnknownLanguage {
                extension: protocol.to_owned(),
            })?;
        // `decorate_with_parser` enforces the protocol-match invariant
        // between the parser and the abstract schema, so no extra guard
        // is needed here.
        crate::decorate::decorate_with_parser(parser.as_ref(), abstract_schema, policy)
    }

    /// Render an [`AbstractSchema`] to canonical source bytes under
    /// `policy`.
    ///
    /// Implementation note: this is exactly the first emit step of
    /// [`decorate`](Self::decorate) — `decorate` then re-parses to
    /// recover the layout fibre, but if all the caller wants is the
    /// bytes, the re-parse is wasted work. Going through
    /// `emit_pretty_with_policy` directly preserves every field of
    /// `policy` in the output (`separator`, `newline`, `indent_width`,
    /// `line_break_after`, `indent_open` / `indent_close`).
    ///
    /// # Errors
    ///
    /// See [`decorate`](Self::decorate).
    pub fn pretty_with_protocol(
        &self,
        protocol: &str,
        abstract_schema: &AbstractSchema,
        policy: &LayoutPolicy,
    ) -> Result<Vec<u8>, ParseError> {
        let parser = self
            .parsers
            .get(protocol)
            .ok_or_else(|| ParseError::UnknownLanguage {
                extension: protocol.to_owned(),
            })?;
        check_protocol_match(
            protocol,
            abstract_schema.as_schema(),
            "pretty_with_protocol",
        )?;
        parser.emit_pretty_with_policy(abstract_schema.as_schema(), policy)
    }

    /// Return the canonical [`Protolens`](panproto_lens::Protolens)
    /// describing the parse / decorate / emit relationship at
    /// `protocol`.
    ///
    /// The protolens encodes the schema-level structure of the
    /// relationship: source-side strips the layout enrichment fibre,
    /// target-side adds it via the registered
    /// [`LayoutEnricher`](panproto_lens::enrichment_registry::LayoutEnricher).
    /// It composes with the rest of the `panproto-lens` protolens
    /// algebra for chain-law reasoning. The operational entry points
    /// for running the relationship on real schemas are
    /// [`decorate`](Self::decorate),
    /// [`pretty_with_protocol`](Self::pretty_with_protocol), and
    /// [`emit_pretty_with_protocol`](Self::emit_pretty_with_protocol).
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::UnknownLanguage`] when `protocol` is not
    /// registered.
    pub fn parse_emit_protolens(
        &self,
        protocol: &str,
        policy: &LayoutPolicy,
    ) -> Result<panproto_lens::Protolens, ParseError> {
        if !self.parsers.contains_key(protocol) {
            return Err(ParseError::UnknownLanguage {
                extension: protocol.to_owned(),
            });
        }
        Ok(crate::parse_emit_protolens::parse_emit_protolens(
            protocol, policy,
        ))
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

/// Guard against running parser-tied operations on a schema built
/// for a different protocol. Catches the user-visible error of
/// passing (say) a JSON schema to a Python parser before the
/// underlying grammar walker would surface it as an opaque rule
/// mismatch.
fn check_protocol_match(
    expected: &str,
    schema: &Schema,
    operation: &'static str,
) -> Result<(), ParseError> {
    if schema.protocol == expected {
        Ok(())
    } else {
        Err(ParseError::SchemaConstruction {
            reason: format!(
                "{operation}: protocol mismatch — registry called with '{expected}' but \
                 schema carries protocol '{}'",
                schema.protocol,
            ),
        })
    }
}
