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

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use panproto_gat::is_interstitial_text_sort;
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
    /// Per-grammar defaults for opaque external scanner tokens.
    cassette: Arc<dyn super::cassettes::GrammarCassette>,
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
        // Named-scope detection is a best-effort secondary feature. Some
        // vendored `tags.scm` files use capture names outside the
        // tree-sitter-tags vocabulary (e.g. C#'s `@module`, AL's helper
        // `@_test_attr`), which `TagsConfiguration` rejects. A grammar
        // must still register for parse/emit in that case, so fall back
        // to a no-op detector (which `(None, None)` constructs and cannot
        // fail) rather than dropping the whole grammar.
        let scope_detector = ScopeDetector::new(&language, tags_query, None)
            .or_else(|_| ScopeDetector::new(&language, None, None))?;

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
            cassette: super::cassettes::cassette_for(protocol_name),
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
        // The put-direction of the parse/emit dependent optic, dispatched
        // on whether the layout complement is present:
        //
        // * **Complement present** (a parsed / CST schema, or one edited
        //   in place by `panproto-io`'s `UnifiedCodec`): replay the layout
        //   fibre. `emit_from_schema` reconstructs bytes from the
        //   `start-byte` / `interstitial-N` / `literal-value` constraints
        //   sorted by source position — byte-faithful by construction.
        // * **Complement absent** (a by-construction / transpiled abstract
        //   schema that never carried a parse trace): there is nothing to
        //   replay, so fall back to the canonical section — the grammar
        //   walk in `emit_pretty` under the default `FormatPolicy`.
        //
        // This makes `emit` total over both worlds: the historical
        // reconstruction flow (replay) and the canonical de-novo flow are
        // the two branches of one review. Before, the abstract case
        // errored with "schema has no text fragments".
        if has_layout_complement(schema) {
            emit_from_schema(schema, &self.protocol_name)
        } else {
            self.emit_pretty_with_policy(schema, &FormatPolicy::default())
        }
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
        emit_pretty_inner(
            &self.protocol_name,
            schema,
            grammar,
            policy,
            Some(&*self.cassette),
        )
    }
}

/// Does `schema` carry the layout complement that `emit_from_schema`
/// replays? True iff some vertex records a `start-byte` anchor (every
/// parsed vertex has one; a by-construction / transpiled schema has
/// none). This is the dependent-optic dispatch in [`LanguageParser::emit`]:
/// present ⇒ replay the fibre, absent ⇒ canonical section.
fn has_layout_complement(schema: &Schema) -> bool {
    schema
        .constraints
        .values()
        .any(|cs| cs.iter().any(|c| c.sort.as_ref() == "start-byte"))
}

/// One recorded text fragment of the layout complement: the span it
/// occupied in the original source, paired with the text to write in its
/// place, plus the rank that breaks a tie between two fragments recording
/// the same span.
struct Fragment {
    /// Start byte in the original source.
    start: usize,
    /// End byte in the original source. This is the span the fragment
    /// *covers*, which is independent of `text`'s current length: an
    /// edited fragment writes more or fewer bytes than it consumed.
    end: usize,
    /// Rank among fragments sharing a span: a leaf's `literal-value` (0)
    /// supersedes the `interstitial-N` run recording the same bytes (1).
    /// Schema constraint maps iterate in an arbitrary order, so without
    /// this the winner of a tie would vary between processes.
    rank: u8,
    /// The text to write, which the injection path may have rewritten.
    text: String,
}

/// Reconstruct source text from a schema using interstitial text and leaf literals.
///
/// The walker stores two kinds of text data, each with the source span it came
/// from:
/// - `literal-value` on leaf nodes: identifiers, literals, keywords that are
///   named nodes, spanning the vertex's `start-byte` … `end-byte`
/// - `interstitial-N` on parent nodes: text between named children (keywords,
///   punctuation, whitespace, comments from anonymous/unnamed tokens), spanning
///   `interstitial-N-start-byte` … `interstitial-N-end-byte`
///
/// Replay walks the fragments in source order and writes each one whose recorded
/// span begins at or after the end of the last span written. Coverage is
/// therefore a question about the *original* spans alone; the rewritten text
/// decides only what bytes come out, never which fragments come out. A fragment
/// edited to be longer than the bytes it replaces consequently displaces nothing
/// that follows it, and the concatenation stays lossless both for an untouched
/// schema (`emit(parse(source))` = `source`) and for an edited one.
fn emit_from_schema(schema: &Schema, protocol: &str) -> Result<Vec<u8>, ParseError> {
    let mut fragments: Vec<Fragment> = Vec::new();

    for name in schema.vertices.keys() {
        let Some(constraints) = schema.constraints.get(name) else {
            continue;
        };
        // Index the vertex's constraints by sort once. A vertex with many
        // children carries one interstitial per gap, and looking each one's
        // span up by scanning the whole list again made replay quadratic in a
        // node's fan-out.
        let by_sort: HashMap<&str, &str> = constraints
            .iter()
            .map(|c| (c.sort.as_ref(), c.value.as_str()))
            .collect();
        let recorded_byte =
            |sort: &str| -> Option<usize> { by_sort.get(sort)?.parse::<usize>().ok() };

        // The leaf literal, spanning the vertex itself.
        if let (Some(start), Some(text)) = (
            recorded_byte("start-byte"),
            by_sort.get("literal-value").map(|t| (*t).to_owned()),
        ) {
            let end = recorded_byte("end-byte").unwrap_or(start + text.len());
            fragments.push(Fragment {
                start,
                end,
                rank: 0,
                text,
            });
        }

        // The interstitial runs between this vertex's named children.
        let mut span_sort = String::new();
        for c in constraints {
            let sort_str = c.sort.as_ref();
            if !is_interstitial_text_sort(sort_str) {
                continue;
            }
            span_sort.clear();
            span_sort.push_str(sort_str);
            span_sort.push_str("-start-byte");
            let Some(start) = recorded_byte(&span_sort) else {
                continue;
            };
            span_sort.truncate(sort_str.len());
            span_sort.push_str("-end-byte");
            let end = recorded_byte(&span_sort).unwrap_or(start + c.value.len());
            fragments.push(Fragment {
                start,
                end,
                rank: 1,
                text: c.value.clone(),
            });
        }
    }

    if fragments.is_empty() {
        return Err(ParseError::EmitFailed {
            protocol: protocol.to_owned(),
            reason: "schema has no text fragments".to_owned(),
        });
    }

    // Source order, widest span first, literal ahead of interstitial. The last
    // two keys make the order total, so replay does not depend on the constraint
    // map's iteration order.
    fragments.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then(b.end.cmp(&a.end))
            .then(a.rank.cmp(&b.rank))
    });

    // A leaf records its text twice — once as `literal-value` and once as the
    // trailing `interstitial-N` covering the same bytes — and a parent's
    // interstitial run can abut a child's span. Writing a fragment whose
    // recorded span starts before the end of the span already written would
    // duplicate those bytes, so skip it.
    let mut output = Vec::new();
    let mut covered = 0;
    for fragment in &fragments {
        if fragment.start >= covered {
            output.extend_from_slice(fragment.text.as_bytes());
            covered = fragment.end.max(fragment.start);
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
