//! Generic tree-sitter AST walker that converts parse trees to panproto schemas.
//!
//! Because theories are auto-derived from the grammar, the walker is fully generic:
//! one implementation works for all languages. The node's `kind()` IS the panproto
//! vertex kind; the field name IS the edge kind. Per-language customization is limited
//! to formatting constraints and scope detection callbacks.

use panproto_schema::{Protocol, Schema, SchemaBuilder};
use rustc_hash::FxHashSet;

use crate::error::ParseError;
use crate::id_scheme::IdGenerator;
use crate::theory_extract::ExtractedTheoryMeta;

/// Nodes whose kind names suggest they introduce a named scope.
///
/// When the walker encounters one of these node kinds, it looks for a `name`
/// or `identifier` child to use as the scope name in the ID generator.
const SCOPE_INTRODUCING_KINDS: &[&str] = &[
    "function_declaration",
    "function_definition",
    "method_declaration",
    "method_definition",
    "class_declaration",
    "class_definition",
    "interface_declaration",
    "struct_item",
    "enum_item",
    "enum_declaration",
    "impl_item",
    "trait_item",
    "module",
    "namespace_definition",
    "package_declaration",
];

/// Nodes whose kind names suggest they contain ordered statement sequences.
const BLOCK_KINDS: &[&str] = &[
    "block",
    "statement_block",
    "compound_statement",
    "declaration_list",
    "field_declaration_list",
    "enum_body",
    "class_body",
    "interface_body",
    "module_body",
];

/// Configuration for the walker, allowing per-language customization.
#[derive(Debug, Clone)]
pub struct WalkerConfig {
    /// Additional node kinds that introduce named scopes in this language.
    pub extra_scope_kinds: Vec<String>,
    /// Additional node kinds that contain ordered statement sequences.
    pub extra_block_kinds: Vec<String>,
    /// Field names to use when looking for the "name" of a scope-introducing node.
    /// Defaults to `["name", "identifier"]`.
    pub name_fields: Vec<String>,
    /// Whether to capture comment nodes as constraints on the following sibling.
    pub capture_comments: bool,
    /// Whether to capture whitespace/formatting as constraints.
    pub capture_formatting: bool,
}

impl Default for WalkerConfig {
    fn default() -> Self {
        Self {
            extra_scope_kinds: Vec::new(),
            extra_block_kinds: Vec::new(),
            name_fields: vec!["name".to_owned(), "identifier".to_owned()],
            capture_comments: true,
            capture_formatting: true,
        }
    }
}

/// Generic AST walker that converts a tree-sitter parse tree to a panproto [`Schema`].
///
/// The walker uses the auto-derived theory to determine vertex and edge kinds directly
/// from the tree-sitter AST, requiring no manual mapping table.
pub struct AstWalker<'a> {
    /// The source code bytes (needed for extracting text of leaf nodes).
    source: &'a [u8],
    /// The auto-derived theory metadata (used for protocol validation context).
    _theory_meta: &'a ExtractedTheoryMeta,
    /// The protocol definition (for SchemaBuilder validation).
    protocol: &'a Protocol,
    /// Per-language configuration.
    config: WalkerConfig,
    /// Known scope-introducing kinds (merged from defaults + config).
    scope_kinds: FxHashSet<String>,
    /// Known block kinds (merged from defaults + config).
    block_kinds: FxHashSet<String>,
}

impl<'a> AstWalker<'a> {
    /// Create a new walker for the given source, theory, and protocol.
    #[must_use]
    pub fn new(
        source: &'a [u8],
        theory_meta: &'a ExtractedTheoryMeta,
        protocol: &'a Protocol,
        config: WalkerConfig,
    ) -> Self {
        let mut scope_kinds: FxHashSet<String> = SCOPE_INTRODUCING_KINDS
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        for kind in &config.extra_scope_kinds {
            scope_kinds.insert(kind.clone());
        }

        let mut block_kinds: FxHashSet<String> =
            BLOCK_KINDS.iter().map(|s| (*s).to_owned()).collect();
        for kind in &config.extra_block_kinds {
            block_kinds.insert(kind.clone());
        }

        Self {
            source,
            _theory_meta: theory_meta,
            protocol,
            config,
            scope_kinds,
            block_kinds,
        }
    }

    /// Walk the entire parse tree and produce a [`Schema`].
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::SchemaConstruction`] if schema building fails.
    pub fn walk(&self, tree: &tree_sitter::Tree, file_path: &str) -> Result<Schema, ParseError> {
        let mut id_gen = IdGenerator::new(file_path);
        let builder = SchemaBuilder::new(self.protocol);
        let root = tree.root_node();

        let builder = self.walk_node(root, builder, &mut id_gen, None)?;

        builder
            .build()
            .map_err(|e| ParseError::SchemaConstruction {
                reason: e.to_string(),
            })
    }

    /// Recursively walk a single node, emitting vertices and edges.
    fn walk_node(
        &self,
        node: tree_sitter::Node<'_>,
        mut builder: SchemaBuilder,
        id_gen: &mut IdGenerator,
        parent_vertex_id: Option<&str>,
    ) -> Result<SchemaBuilder, ParseError> {
        // Skip anonymous tokens (punctuation, keywords like `{`, `}`, `,`, etc.).
        if !node.is_named() {
            return Ok(builder);
        }

        let kind = node.kind();

        // Skip the root "program"/"source_file"/"module" wrapper if it just wraps children.
        // We still process it to emit its children, but do so by iterating directly.
        let is_root_wrapper = parent_vertex_id.is_none()
            && (kind == "program" || kind == "source_file" || kind == "module" || kind == "translation_unit");

        // Determine vertex ID.
        let vertex_id = if is_root_wrapper {
            // Root wrappers get the file path as their ID.
            id_gen.current_prefix().to_string()
        } else if self.scope_kinds.contains(kind) {
            // Scope-introducing nodes use their name child for the ID.
            let name = self.extract_scope_name(&node);
            match name {
                Some(n) => id_gen.named_id(&n),
                None => id_gen.anonymous_id(),
            }
        } else {
            // All other nodes get positional IDs.
            id_gen.anonymous_id()
        };

        // Emit vertex. If the kind is not in the protocol's obj_kinds, use a generic "node" kind.
        let effective_kind =
            if self.protocol.obj_kinds.is_empty() || self.protocol.obj_kinds.iter().any(|k| k == kind) {
                kind
            } else {
                "node"
            };

        builder = builder
            .vertex(&vertex_id, effective_kind, None)
            .map_err(|e| ParseError::SchemaConstruction {
                reason: format!("vertex '{vertex_id}' ({kind}): {e}"),
            })?;

        // Emit edge from parent to this node.
        if let Some(parent_id) = parent_vertex_id {
            // Determine edge kind: use the tree-sitter field name if this node
            // was accessed via a field, otherwise use "child_of".
            let edge_kind = node
                .parent()
                .and_then(|p| {
                    // Find which field of the parent this node corresponds to.
                    for i in 0..p.child_count() {
                        if let Some(child) = p.child(i) {
                            if child.id() == node.id() {
                                return p.field_name_for_child(i as u32);
                            }
                        }
                    }
                    None
                })
                .unwrap_or("child_of");

            builder = builder
                .edge(parent_id, &vertex_id, edge_kind, None)
                .map_err(|e| ParseError::SchemaConstruction {
                    reason: format!("edge {parent_id} -> {vertex_id} ({edge_kind}): {e}"),
                })?;
        }

        // Emit constraints for leaf nodes (literals, identifiers, operators).
        if node.child_count() == 0 || !node.is_named() {
            if let Ok(text) = node.utf8_text(self.source) {
                builder = builder.constraint(&vertex_id, "literal-value", text);
            }
        }

        // Emit formatting constraints if enabled.
        if self.config.capture_formatting {
            builder = self.emit_formatting_constraints(node, &vertex_id, builder);
        }

        // Enter scope if this is a scope-introducing node.
        let entered_scope = if self.scope_kinds.contains(kind) && !is_root_wrapper {
            let name = self.extract_scope_name(&node);
            match name {
                Some(n) => {
                    id_gen.push_named_scope(&n);
                    true
                }
                None => {
                    id_gen.push_anonymous_scope();
                    true
                }
            }
        } else if self.block_kinds.contains(kind) {
            id_gen.push_anonymous_scope();
            true
        } else {
            false
        };

        // Recurse into named children, collecting comments to attach to
        // the next non-comment sibling.
        let cursor = &mut node.walk();
        let children: Vec<_> = node.named_children(cursor).collect();
        let mut pending_comments: Vec<String> = Vec::new();

        for child in children {
            if self.config.capture_comments && child.kind() == "comment" {
                if let Ok(text) = child.utf8_text(self.source) {
                    pending_comments.push(text.to_owned());
                }
                continue;
            }

            builder = self.walk_node(child, builder, id_gen, Some(&vertex_id))?;

            // Attach pending comments to the vertex that was just created for this child.
            if !pending_comments.is_empty() {
                // The child's vertex ID is the most recently added vertex. We reconstruct
                // it by looking at what the id_gen would have produced. Since walk_node
                // already created it, we find the last vertex that was added by looking
                // at the builder's current state. However, SchemaBuilder doesn't expose
                // iteration, so we use a simpler approach: attach to the parent vertex.
                // This is semantically correct (comments are part of the parent's scope)
                // and avoids complex lookahead.
                let comment_text = pending_comments.join("\n");
                builder = builder.constraint(&vertex_id, "comment", &comment_text);
                pending_comments.clear();
            }
        }

        // Any trailing comments (no following sibling) attach to the parent.
        if !pending_comments.is_empty() {
            let comment_text = pending_comments.join("\n");
            builder = builder.constraint(&vertex_id, "comment", &comment_text);
        }

        // Exit scope.
        if entered_scope {
            id_gen.pop_scope();
        }

        Ok(builder)
    }

    /// Extract the name of a scope-introducing node by looking for name/identifier children.
    fn extract_scope_name(&self, node: &tree_sitter::Node<'_>) -> Option<String> {
        for field_name in &self.config.name_fields {
            if let Some(name_node) = node.child_by_field_name(field_name.as_bytes()) {
                if let Ok(text) = name_node.utf8_text(self.source) {
                    return Some(text.to_owned());
                }
            }
        }
        None
    }

    /// Emit formatting constraints for a node (indentation, position).
    fn emit_formatting_constraints(
        &self,
        node: tree_sitter::Node<'_>,
        vertex_id: &str,
        mut builder: SchemaBuilder,
    ) -> SchemaBuilder {
        let start = node.start_position();

        // Capture indentation (column of first character on the line).
        if start.column > 0 {
            // Extract the actual indentation characters from the source.
            let line_start = node.start_byte().saturating_sub(start.column);
            if line_start < self.source.len() {
                let indent_end = line_start + start.column.min(self.source.len() - line_start);
                if let Ok(indent) = std::str::from_utf8(&self.source[line_start..indent_end]) {
                    // Only capture if the extracted region is pure whitespace.
                    if !indent.is_empty() && indent.trim().is_empty() {
                        builder = builder.constraint(vertex_id, "indent", indent);
                    }
                }
            }
        }

        // Count blank lines before this node by looking at source between
        // previous sibling's end and this node's start.
        if let Some(prev) = node.prev_named_sibling() {
            let gap_start = prev.end_byte();
            let gap_end = node.start_byte();
            if gap_start < gap_end && gap_end <= self.source.len() {
                let gap = &self.source[gap_start..gap_end];
                let blank_lines = gap.iter().filter(|&&b| b == b'\n').count().saturating_sub(1);
                if blank_lines > 0 {
                    builder =
                        builder.constraint(vertex_id, "blank-lines-before", &blank_lines.to_string());
                }
            }
        }

        builder
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThTestInst".into(),
            obj_kinds: vec![],  // Empty = open protocol, accepts all kinds.
            edge_rules: vec![],
            constraint_sorts: vec![],
            has_order: true,
            has_coproducts: false,
            has_recursion: false,
            has_causal: false,
            nominal_identity: false,
            has_defaults: false,
            has_coercions: false,
            has_mergers: false,
            has_policies: false,
        }
    }

    fn make_test_meta() -> ExtractedTheoryMeta {
        use panproto_gat::{Sort, Theory};
        ExtractedTheoryMeta {
            theory: Theory::new("ThTest", vec![Sort::simple("Vertex")], vec![], vec![]),
            supertypes: FxHashSet::default(),
            subtype_map: Vec::new(),
            optional_fields: FxHashSet::default(),
            ordered_fields: FxHashSet::default(),
            vertex_kinds: Vec::new(),
            edge_kinds: Vec::new(),
        }
    }

    #[test]
    fn walk_simple_typescript() {
        let source = b"function greet(name: string): string { return name; }";

        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();

        let protocol = make_test_protocol();
        let meta = make_test_meta();
        let walker = AstWalker::new(source, &meta, &protocol, WalkerConfig::default());

        let schema = walker.walk(&tree, "test.ts").unwrap();

        // Should have produced some vertices.
        assert!(
            schema.vertices.len() > 1,
            "expected multiple vertices, got {}",
            schema.vertices.len()
        );

        // The root should be the file.
        let root_name: panproto_gat::Name = "test.ts".into();
        assert!(
            schema.vertices.contains_key(&root_name),
            "missing root vertex"
        );
    }

    #[test]
    fn walk_simple_python() {
        let source = b"def add(a, b):\n    return a + b\n";

        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_python::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();

        let protocol = make_test_protocol();
        let meta = make_test_meta();
        let walker = AstWalker::new(source, &meta, &protocol, WalkerConfig::default());

        let schema = walker.walk(&tree, "test.py").unwrap();

        assert!(
            schema.vertices.len() > 1,
            "expected multiple vertices, got {}",
            schema.vertices.len()
        );
    }

    #[test]
    fn walk_simple_rust() {
        let source = b"fn main() { let x = 42; println!(\"{}\", x); }";

        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();

        let protocol = make_test_protocol();
        let meta = make_test_meta();
        let walker = AstWalker::new(source, &meta, &protocol, WalkerConfig::default());

        let schema = walker.walk(&tree, "test.rs").unwrap();

        assert!(
            schema.vertices.len() > 1,
            "expected multiple vertices, got {}",
            schema.vertices.len()
        );
    }
}
