//! Common language parser implementation shared by all tree-sitter-based parsers.
//!
//! Since the generic [`AstWalker`](crate::walker::AstWalker) handles all languages
//! uniformly (the node kind IS the vertex kind, the field name IS the edge kind),
//! per-language parsers are thin wrappers that provide:
//!
//! 1. The tree-sitter Language object
//! 2. The embedded `NODE_TYPES` JSON
//! 3. Language-specific [`WalkerConfig`](crate::walker::WalkerConfig) overrides
//! 4. File extension mapping

use std::sync::{Mutex, OnceLock};

use panproto_schema::{Protocol, Schema};

use crate::emit_pretty::{FormatPolicy, Grammar as EmitGrammar, emit_pretty as emit_pretty_inner};
use crate::error::ParseError;
use crate::registry::AstParser;
use crate::scope_detector::ScopeDetector;
use crate::theory_extract::{ExtractedTheoryMeta, extract_theory_from_node_types};
use crate::walker::{AstWalker, WalkerConfig};

/// A generic language parser built from a tree-sitter grammar.
///
/// This struct is the shared implementation behind all language parsers.
/// Each language constructs one with its specific grammar, node types,
/// tags query, and config.
pub struct LanguageParser {
    /// The protocol name (e.g. `"typescript"`, `"python"`).
    protocol_name: String,
    /// File extensions this language handles.
    extensions: Vec<&'static str>,
    /// The resolved tree-sitter language.
    language: tree_sitter::Language,
    /// The grammar's bundled `tags.scm`, if any (for named-scope detection).
    tags_query: Option<&'static str>,
    /// Project-level tags-query override (concatenated in front of
    /// `tags_query` when constructing the [`ScopeDetector`]).
    project_tags_override: Option<String>,
    /// The auto-derived theory metadata.
    theory_meta: ExtractedTheoryMeta,
    /// The panproto protocol definition (used for `SchemaBuilder` validation).
    protocol: Protocol,
    /// Per-language walker configuration.
    walker_config: WalkerConfig,
    /// A reusable [`ScopeDetector`] for this language.
    ///
    /// Held behind a `Mutex` because `parse()` on [`AstParser`] takes `&self`
    /// but the detector's `TagsContext` (and internal `QueryCursor`) need
    /// `&mut` access during a tags query run. A single parser instance is
    /// typically used serially; contention here is rare.
    scope_detector: Mutex<ScopeDetector>,
    /// Raw `grammar.json` bytes for the de-novo emit walker. `None`
    /// when the upstream grammar does not ship `grammar.json` and
    /// `tools/fetch-grammar-json.py` could not regenerate one.
    grammar_json: Option<&'static [u8]>,
    /// Raw `node-types.json` bytes for augmenting the Grammar's subtype
    /// closure with parser-produced child kinds not in grammar.json.
    node_types_json_for_emit: Option<Vec<u8>>,
    /// Lazily-parsed grammar. Populated on first call to `emit_pretty`.
    grammar_cache: OnceLock<Result<EmitGrammar, ParseError>>,
}

impl LanguageParser {
    /// Create a new language parser from a pre-constructed [`Language`](tree_sitter::Language).
    ///
    /// `tags_query` is the grammar's `queries/tags.scm` content, usually
    /// sourced from [`panproto_grammars::Grammar::tags_query`]; pass `None`
    /// if the grammar does not ship one.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] if theory extraction from `node_types_json`
    /// fails, or if the grammar's tags query fails to compile.
    pub fn from_language(
        protocol_name: &str,
        extensions: Vec<&'static str>,
        language: tree_sitter::Language,
        node_types_json: &[u8],
        tags_query: Option<&'static str>,
        walker_config: WalkerConfig,
    ) -> Result<Self, ParseError> {
        Self::from_language_with_grammar_json(
            protocol_name,
            extensions,
            language,
            node_types_json,
            tags_query,
            walker_config,
            None,
        )
    }

    /// Construct a `LanguageParser` with vendored `grammar.json` bytes
    /// for de-novo emission via [`AstParser::emit_pretty`].
    ///
    /// `grammar_json` should come from
    /// [`panproto_grammars::Grammar::grammar_json`]; pass `None` to
    /// signal that the language has no production-rule table available.
    /// Without it, `emit_pretty` returns
    /// [`ParseError::EmitFailed`] with a `grammar.json missing` reason.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] if theory extraction from
    /// `node_types_json` fails or if the tags query rejects compilation.
    pub fn from_language_with_grammar_json(
        protocol_name: &str,
        extensions: Vec<&'static str>,
        language: tree_sitter::Language,
        node_types_json: &[u8],
        tags_query: Option<&'static str>,
        walker_config: WalkerConfig,
        grammar_json: Option<&'static [u8]>,
    ) -> Result<Self, ParseError> {
        let theory_name = format!("Th{}FullAST", capitalize_first(protocol_name));
        let theory_meta = extract_theory_from_node_types(&theory_name, node_types_json)?;
        let protocol = build_full_ast_protocol(protocol_name, &theory_name);
        let scope_detector = ScopeDetector::new(&language, tags_query, None)?;

        Ok(Self {
            protocol_name: protocol_name.to_owned(),
            extensions,
            language,
            tags_query,
            project_tags_override: None,
            theory_meta,
            protocol,
            walker_config,
            scope_detector: Mutex::new(scope_detector),
            grammar_json,
            node_types_json_for_emit: Some(node_types_json.to_vec()),
            grammar_cache: OnceLock::new(),
        })
    }

    /// Install a project-level tags-query override.
    ///
    /// The override string is concatenated in front of the grammar's
    /// bundled `tags.scm` when the detector is rebuilt. Tree-sitter unions
    /// all patterns, so overrides augment the defaults without replacing
    /// them. Pass `None` to clear an existing override.
    ///
    /// Typical source: `panproto.toml`'s `[parse.tags.<lang>] path = "..."`.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::ScopeQueryCompile`] if the combined query
    /// fails to compile against this language.
    pub fn set_tags_override(&mut self, override_query: Option<String>) -> Result<(), ParseError> {
        let detector =
            ScopeDetector::new(&self.language, self.tags_query, override_query.as_deref())?;
        self.project_tags_override = override_query;
        if let Ok(mut guard) = self.scope_detector.lock() {
            *guard = detector;
        }
        Ok(())
    }
}

impl AstParser for LanguageParser {
    fn protocol_name(&self) -> &str {
        &self.protocol_name
    }

    fn parse(&self, source: &[u8], file_path: &str) -> Result<Schema, ParseError> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&self.language)
            .map_err(|e| ParseError::TreeSitterParse {
                path: format!("{file_path}: set_language failed: {e}"),
            })?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| ParseError::TreeSitterParse {
                path: format!("{file_path}: parse returned None (timeout or cancellation)"),
            })?;

        // Build the walker (which runs the tags query once via the
        // detector) inside the guard scope, then drop the guard before
        // walking the tree. The scope map is copied into the walker, so
        // the detector lock is no longer needed past that point.
        let walker = {
            let mut detector_guard =
                self.scope_detector
                    .lock()
                    .map_err(|_| ParseError::SchemaConstruction {
                        reason: "scope-detector mutex poisoned".to_owned(),
                    })?;
            AstWalker::new(
                source,
                &self.theory_meta,
                &self.protocol,
                self.walker_config.clone(),
                Some(&mut *detector_guard),
            )
        };

        walker.walk(&tree, file_path)
    }

    fn emit(&self, schema: &Schema) -> Result<Vec<u8>, ParseError> {
        // Reconstruct source text from the schema's structural information.
        //
        // The walker stores two types of text constraints:
        // 1. `literal-value` on leaf nodes: the source text of identifiers, literals, etc.
        // 2. `interstitial-N` on parent nodes: the text between named children, which
        //    contains keywords, punctuation, whitespace, and comments.
        //
        // The emitter walks the schema tree depth-first, interleaving interstitial text
        // with child emissions to reconstruct the full source.
        emit_from_schema(schema, &self.protocol_name)
    }

    fn supported_extensions(&self) -> &[&str] {
        &self.extensions
    }

    fn theory_meta(&self) -> &ExtractedTheoryMeta {
        &self.theory_meta
    }

    fn emit_pretty_with_policy(
        &self,
        schema: &Schema,
        policy: &FormatPolicy,
    ) -> Result<Vec<u8>, ParseError> {
        let bytes = self.grammar_json.ok_or_else(|| ParseError::EmitFailed {
            protocol: self.protocol_name.clone(),
            reason: "grammar.json not vendored for this protocol; \
                     run tools/fetch-grammar-json.py to populate it"
                .to_owned(),
        })?;
        let nt = self.node_types_json_for_emit.as_deref();
        let cached = self.grammar_cache.get_or_init(|| {
            EmitGrammar::from_bytes_with_node_types(&self.protocol_name, bytes, nt)
        });
        let grammar = match cached {
            Ok(g) => g,
            Err(e) => {
                return Err(ParseError::EmitFailed {
                    protocol: self.protocol_name.clone(),
                    reason: format!("grammar.json parse failed: {e}"),
                });
            }
        };
        emit_pretty_inner(&self.protocol_name, schema, grammar, policy)
    }
}

/// Reconstruct source text from a schema using interstitial text and leaf literals.
///
/// The walker stores two types of text data:
/// - `literal-value` on leaf nodes: identifiers, literals, keywords that are named nodes
/// - `interstitial-N` on parent nodes: text between named children (keywords, punctuation,
///   whitespace, comments from anonymous/unnamed tokens)
///
/// The emitter reconstructs source by collecting ALL text fragments (both interstitials
/// and leaf literals) and sorting them by their byte position in the original source.
/// This produces exact round-trip fidelity: `emit(parse(source))` = `source`.
fn emit_from_schema(schema: &Schema, protocol: &str) -> Result<Vec<u8>, ParseError> {
    // Collect all text fragments with their byte positions.
    // Each fragment is (start_byte, text).
    let mut fragments: Vec<(usize, String)> = Vec::new();

    for name in schema.vertices.keys() {
        if let Some(constraints) = schema.constraints.get(name) {
            // Get start-byte for this vertex.
            let start_byte = constraints
                .iter()
                .find(|c| c.sort.as_ref() == "start-byte")
                .and_then(|c| c.value.parse::<usize>().ok());

            // Collect literal-value from leaf nodes.
            let literal = constraints
                .iter()
                .find(|c| c.sort.as_ref() == "literal-value")
                .map(|c| c.value.clone());

            if let (Some(start), Some(text)) = (start_byte, literal) {
                fragments.push((start, text));
            }

            // Collect interstitial text fragments.
            // Each interstitial has a byte position derived from its parent and index.
            for c in constraints {
                let sort_str = c.sort.as_ref();
                if sort_str.starts_with("interstitial-") {
                    // The interstitial's position is encoded in a companion constraint.
                    // We stored interstitial-N-start-byte alongside interstitial-N.
                    let pos_sort = format!("{sort_str}-start-byte");
                    let pos = constraints
                        .iter()
                        .find(|c2| c2.sort.as_ref() == pos_sort.as_str())
                        .and_then(|c2| c2.value.parse::<usize>().ok());

                    if let Some(p) = pos {
                        fragments.push((p, c.value.clone()));
                    }
                }
            }
        }
    }

    if fragments.is_empty() {
        return Err(ParseError::EmitFailed {
            protocol: protocol.to_owned(),
            reason: "schema has no text fragments".to_owned(),
        });
    }

    // Sort by byte position and concatenate.
    fragments.sort_by_key(|(pos, _)| *pos);

    // Deduplicate overlapping fragments (parent interstitials may overlap with
    // child literals). Keep the first fragment at each position.
    let mut output = Vec::new();
    let mut cursor = 0;

    for (pos, text) in &fragments {
        if *pos >= cursor {
            output.extend_from_slice(text.as_bytes());
            cursor = pos + text.len();
        }
    }

    Ok(output)
}

/// Build the standard Protocol for a full-AST language parser.
///
/// Shared by `LanguageParser::new` and `LanguageParser::from_language`
/// to avoid duplicating the constraint sorts and flag definitions.
fn build_full_ast_protocol(protocol_name: &str, theory_name: &str) -> Protocol {
    Protocol {
        name: protocol_name.into(),
        schema_theory: theory_name.into(),
        instance_theory: format!("{theory_name}Instance"),
        schema_composition: None,
        instance_composition: None,
        obj_kinds: vec![],
        edge_rules: vec![],
        constraint_sorts: vec![
            "literal-value".into(),
            "literal-type".into(),
            "operator".into(),
            "visibility".into(),
            "mutability".into(),
            "async".into(),
            "static".into(),
            "generator".into(),
            "comment".into(),
            "indent".into(),
            "trailing-comma".into(),
            "semicolon".into(),
            "blank-lines-before".into(),
            "start-byte".into(),
            "end-byte".into(),
        ],
        has_order: true,
        has_coproducts: false,
        has_recursion: true,
        has_causal: false,
        nominal_identity: false,
        has_defaults: false,
        has_coercions: false,
        has_mergers: false,
        has_policies: false,
    }
}

/// Capitalize the first letter of a string.
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}
