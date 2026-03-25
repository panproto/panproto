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
        // Emit walks the schema graph top-down, reconstructing source text.
        // This uses the schema's structural information (vertex kinds, edge kinds,
        // constraints) to produce syntactically valid output.
        emit_schema_as_source(schema, &self.protocol_name)
    }

    fn supported_extensions(&self) -> &[&str] {
        &self.extensions
    }

    fn theory_meta(&self) -> &ExtractedTheoryMeta {
        &self.theory_meta
    }
}

/// Emit a schema back to source text by walking the graph top-down.
///
/// This is a generic emitter that works for any language by using the schema's
/// structural information. It produces output that captures the AST structure,
/// though formatting fidelity depends on the constraints captured during parsing.
fn emit_schema_as_source(schema: &Schema, protocol: &str) -> Result<Vec<u8>, ParseError> {
    let mut output = Vec::new();
    let mut visited = rustc_hash::FxHashSet::default();

    // Find root vertices (those that have no incoming edges).
    let roots: Vec<_> = schema
        .vertices
        .keys()
        .filter(|v| {
            schema
                .incoming
                .get(*v)
                .map_or(true, smallvec::SmallVec::is_empty)
        })
        .collect();

    for root in &roots {
        emit_vertex(schema, root, &mut output, &mut visited, 0);
    }

    if output.is_empty() {
        return Err(ParseError::EmitFailed {
            protocol: protocol.to_owned(),
            reason: "schema produced no output".to_owned(),
        });
    }

    Ok(output)
}

/// Recursively emit a vertex and its children.
fn emit_vertex(
    schema: &Schema,
    vertex_id: &panproto_gat::Name,
    output: &mut Vec<u8>,
    visited: &mut rustc_hash::FxHashSet<panproto_gat::Name>,
    depth: usize,
) {
    if !visited.insert(vertex_id.clone()) {
        return; // Already visited (cycles).
    }

    let _vertex = match schema.vertices.get(vertex_id) {
        Some(v) => v,
        None => return,
    };

    // Check for blank-lines-before constraint.
    if let Some(constraints) = schema.constraints.get(vertex_id) {
        for c in constraints {
            if c.sort.as_ref() == "blank-lines-before" {
                if let Ok(n) = c.value.parse::<usize>() {
                    for _ in 0..n {
                        output.push(b'\n');
                    }
                }
            }
        }
    }

    // Check for comment constraint.
    if let Some(constraints) = schema.constraints.get(vertex_id) {
        for c in constraints {
            if c.sort.as_ref() == "comment" {
                emit_indent(output, depth);
                output.extend_from_slice(c.value.as_bytes());
                output.push(b'\n');
            }
        }
    }

    // Check for indent constraint; otherwise use depth-based indentation.
    let indent = schema
        .constraints
        .get(vertex_id)
        .and_then(|cs| {
            cs.iter()
                .find(|c| c.sort.as_ref() == "indent")
                .map(|c| c.value.clone())
        });

    // Emit the vertex content. Leaf nodes (with literal-value) emit their text.
    let has_literal = schema
        .constraints
        .get(vertex_id)
        .map_or(false, |cs| cs.iter().any(|c| c.sort.as_ref() == "literal-value"));

    if has_literal {
        // Use the captured indent if available, otherwise depth-based.
        if let Some(ref indent_str) = indent {
            output.extend_from_slice(indent_str.as_bytes());
        }
        if let Some(constraints) = schema.constraints.get(vertex_id) {
            for c in constraints {
                if c.sort.as_ref() == "literal-value" {
                    output.extend_from_slice(c.value.as_bytes());
                }
            }
        }
    }

    // Recurse into children via outgoing edges.
    if let Some(edges) = schema.outgoing.get(vertex_id) {
        for edge in edges {
            // Find the target vertex for this edge.
            if let Some(target) = schema.edges.get(edge) {
                emit_vertex(schema, target, output, visited, depth + 1);
            }
        }
    }
}

/// Emit indentation at the given depth.
fn emit_indent(output: &mut Vec<u8>, depth: usize) {
    for _ in 0..depth {
        output.extend_from_slice(b"    ");
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
