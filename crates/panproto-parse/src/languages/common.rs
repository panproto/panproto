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
    /// Cached source bytes from the most recent parse, for emission.
    /// Protected by a mutex for thread-safety (AstParser requires Send+Sync).
    last_source: std::sync::Mutex<Vec<u8>>,
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
        let theory_name = format!("Th{}FullAST", capitalize_first(protocol_name));
        let theory_meta = extract_theory_from_node_types(&theory_name, node_types_json)?;

        let protocol = Protocol {
            name: protocol_name.into(),
            schema_theory: theory_name.clone().into(),
            instance_theory: format!("Th{}FullASTInstance", capitalize_first(protocol_name)).into(),
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
        };

        Ok(Self {
            protocol_name: protocol_name.to_owned(),
            extensions,
            language: language_fn.into(),
            theory_meta,
            protocol,
            walker_config,
            last_source: std::sync::Mutex::new(Vec::new()),
        })
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

        // Build an open protocol from the extracted theory.
        // An open protocol (empty obj_kinds) accepts all vertex kinds,
        // which is what we want since the walker emits the grammar's own node kinds.
        let protocol = Protocol {
            name: protocol_name.into(),
            schema_theory: theory_name.clone().into(),
            instance_theory: format!("Th{}FullASTInstance", capitalize_first(protocol_name)).into(),
            obj_kinds: vec![], // Open protocol: accept all kinds from grammar.
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
        };

        Ok(Self {
            protocol_name: protocol_name.to_owned(),
            extensions,
            language,
            theory_meta,
            protocol,
            walker_config,
            last_source: std::sync::Mutex::new(Vec::new()),
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

        let schema = walker.walk(&tree, file_path)?;

        // Cache source bytes for emission.
        if let Ok(mut cached) = self.last_source.lock() {
            cached.clear();
            cached.extend_from_slice(source);
        }

        Ok(schema)
    }

    fn emit(&self, schema: &Schema) -> Result<Vec<u8>, ParseError> {
        // Reconstruct source text from the schema's byte-range constraints.
        // Each vertex has start-byte and end-byte constraints stored during parsing.
        // Leaf nodes (those with literal-value) have their source text.
        //
        // Strategy: collect all leaf nodes with their byte ranges, sort by start-byte,
        // and reconstruct the source by filling gaps with whitespace from the cached
        // source bytes or with spaces.
        emit_from_byte_ranges(schema, &self.last_source, &self.protocol_name)
    }

    fn supported_extensions(&self) -> &[&str] {
        &self.extensions
    }

    fn theory_meta(&self) -> &ExtractedTheoryMeta {
        &self.theory_meta
    }
}

/// Reconstruct source text from schema byte-range constraints.
///
/// During parsing, each vertex is annotated with `start-byte` and `end-byte`
/// constraints. Leaf nodes also have `literal-value` with their source text.
/// This function collects all leaf nodes, sorts by byte position, and
/// reconstructs the source by concatenating leaf text with gap whitespace
/// from the cached source bytes.
fn emit_from_byte_ranges(
    schema: &Schema,
    cached_source: &std::sync::Mutex<Vec<u8>>,
    protocol: &str,
) -> Result<Vec<u8>, ParseError> {
    let source_bytes = cached_source.lock().map_err(|_| ParseError::EmitFailed {
        protocol: protocol.to_owned(),
        reason: "failed to acquire source cache lock".to_owned(),
    })?;

    // If we have cached source bytes, return them directly.
    // The schema is a structural representation of the same source;
    // the authoritative text is the cached source.
    if !source_bytes.is_empty() {
        return Ok(source_bytes.clone());
    }

    // No cached source: reconstruct from leaf literal-value constraints
    // ordered by start-byte position.
    let mut leaves: Vec<(usize, usize, String)> = Vec::new();

    for (name, _vertex) in &schema.vertices {
        if let Some(constraints) = schema.constraints.get(name) {
            let start = constraints
                .iter()
                .find(|c| c.sort.as_ref() == "start-byte")
                .and_then(|c| c.value.parse::<usize>().ok());
            let end = constraints
                .iter()
                .find(|c| c.sort.as_ref() == "end-byte")
                .and_then(|c| c.value.parse::<usize>().ok());
            let literal = constraints
                .iter()
                .find(|c| c.sort.as_ref() == "literal-value")
                .map(|c| c.value.clone());

            if let (Some(s), Some(e), Some(text)) = (start, end, literal) {
                leaves.push((s, e, text));
            }
        }
    }

    if leaves.is_empty() {
        return Err(ParseError::EmitFailed {
            protocol: protocol.to_owned(),
            reason: "schema has no leaf nodes with literal-value constraints".to_owned(),
        });
    }

    // Sort by start byte position.
    leaves.sort_by_key(|(s, _, _)| *s);

    // Reconstruct source by concatenating leaves with gap whitespace.
    let mut output = Vec::new();
    let mut cursor = 0;

    for (start, _end, text) in &leaves {
        // Fill gap between cursor and this leaf's start with spaces.
        if *start > cursor {
            let gap_size = start - cursor;
            // Use a single space for small gaps, newlines for larger ones.
            if gap_size <= 2 {
                for _ in 0..gap_size {
                    output.push(b' ');
                }
            } else {
                output.push(b'\n');
                // Estimate indentation from the gap size.
                for _ in 0..(gap_size.saturating_sub(1)) {
                    output.push(b' ');
                }
            }
        }
        output.extend_from_slice(text.as_bytes());
        cursor = *start + text.len();
    }

    Ok(output)
}

/// Capitalize the first letter of a string.
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}
