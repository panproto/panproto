//! Common language parser implementation shared by all tree-sitter-based parsers.
//!
//! Since the generic [`AstWalker`] handles all languages uniformly (the node kind
//! IS the vertex kind, the field name IS the edge kind), per-language parsers are
//! thin wrappers that provide:
//!
//! 1. The tree-sitter Language object
//! 2. The embedded `NODE_TYPES` JSON
//! 3. Language-specific [`WalkerConfig`] overrides
//! 4. File extension mapping

use panproto_schema::{Protocol, Schema};

use crate::error::ParseError;
use crate::registry::AstParser;
use crate::theory_extract::{ExtractedTheoryMeta, extract_theory_from_node_types};
use crate::walker::{AstWalker, WalkerConfig};

/// A generic language parser built from a tree-sitter grammar.
///
/// This struct is the shared implementation behind all 10 language parsers.
/// Each language constructs one with its specific grammar, node types, and config.
pub struct LanguageParser {
    /// The protocol name (e.g. `"typescript"`, `"python"`).
    protocol_name: String,
    /// File extensions this language handles.
    extensions: Vec<&'static str>,
    /// The resolved tree-sitter language.
    language: tree_sitter::Language,
    /// The auto-derived theory metadata.
    theory_meta: ExtractedTheoryMeta,
    /// The panproto protocol definition (used for SchemaBuilder validation).
    protocol: Protocol,
    /// Per-language walker configuration.
    walker_config: WalkerConfig,
}

impl LanguageParser {
    /// Create a new language parser from a [`LanguageFn`](tree_sitter_language::LanguageFn).
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] if theory extraction from `node_types_json` fails.
    pub fn new(
        protocol_name: &str,
        extensions: Vec<&'static str>,
        language_fn: tree_sitter_language::LanguageFn,
        node_types_json: &[u8],
        walker_config: WalkerConfig,
    ) -> Result<Self, ParseError> {
        Self::from_language(protocol_name, extensions, language_fn.into(), node_types_json, walker_config)
    }

    /// Create a new language parser from a pre-constructed [`Language`](tree_sitter::Language).
    ///
    /// Used by grammar crates (like Kotlin) that expose a `fn language() -> Language`
    /// instead of a `LANGUAGE` constant.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] if theory extraction from `node_types_json` fails.
    pub fn from_language(
        protocol_name: &str,
        extensions: Vec<&'static str>,
        language: tree_sitter::Language,
        node_types_json: &[u8],
        walker_config: WalkerConfig,
    ) -> Result<Self, ParseError> {
        let theory_name = format!("Th{}FullAST", capitalize_first(protocol_name));
        let theory_meta = extract_theory_from_node_types(&theory_name, node_types_json)?;
        let protocol = build_full_ast_protocol(protocol_name, &theory_name);

        Ok(Self {
            protocol_name: protocol_name.to_owned(),
            extensions,
            language,
            theory_meta,
            protocol,
            walker_config,
        })
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

        let walker = AstWalker::new(
            source,
            &self.theory_meta,
            &self.protocol,
            self.walker_config.clone(),
        );

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

    for (name, _vertex) in &schema.vertices {
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
        instance_theory: format!("{theory_name}Instance").into(),
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
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}
