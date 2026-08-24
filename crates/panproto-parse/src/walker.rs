//! Generic tree-sitter AST walker that converts parse trees to panproto schemas.
//!
//! Because theories are auto-derived from the grammar, the walker is fully generic:
//! one implementation works for all languages. The node's `kind()` IS the panproto
//! vertex kind; the field name IS the edge kind.
//!
//! Named-scope detection (functions, classes, methods, modules, types) is driven
//! by the grammar's `queries/tags.scm` file via [`ScopeDetector`], not by a
//! hardcoded node-kind list. This makes scope detection uniformly correct across
//! every tree-sitter grammar that ships a tags query. See the [`scope_detector`]
//! module for the full rationale.
//!
//! [`scope_detector`]: crate::scope_detector
//! [`ScopeDetector`]: crate::scope_detector::ScopeDetector

use std::collections::BTreeMap;

use panproto_schema::{Protocol, Schema, SchemaBuilder};
use rustc_hash::FxHashSet;

use crate::error::ParseError;
use crate::id_scheme::IdGenerator;
use crate::scope_detector::{NamedScope, ScopeDetector};
use crate::theory_extract::ExtractedTheoryMeta;

/// Nodes whose kind names suggest they contain ordered statement sequences.
///
/// Unlike scope detection (which is grammar-driven via `tags.scm`), block
/// grouping is a structural concern: we want sibling statements inside a
/// block to get positional IDs (`$0`, `$1`, ...) so insertions don't
/// cascade. Per-language [`WalkerConfig`] overrides extend this list.
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
#[derive(Debug, Clone, Default)]
pub struct WalkerConfig {
    /// Additional node kinds that contain ordered statement sequences.
    ///
    /// Named-scope detection is handled by [`ScopeDetector`] from the
    /// grammar's `tags.scm`; no per-language scope configuration is
    /// required here.
    ///
    /// [`ScopeDetector`]: crate::scope_detector::ScopeDetector
    pub extra_block_kinds: Vec<String>,
    /// Whether to capture comment nodes as constraints on the following sibling.
    pub capture_comments: bool,
    /// Whether to capture whitespace/formatting as constraints.
    pub capture_formatting: bool,
}

impl WalkerConfig {
    /// Construct a config with formatting and comment capture enabled (the
    /// common default; [`WalkerConfig::default`] returns all-false).
    #[must_use]
    pub const fn standard() -> Self {
        Self {
            extra_block_kinds: Vec::new(),
            capture_comments: true,
            capture_formatting: true,
        }
    }
}

/// Generic AST walker that converts a tree-sitter parse tree to a panproto [`Schema`].
///
/// The walker uses the auto-derived theory to determine vertex and edge kinds directly
/// from the tree-sitter AST, requiring no manual mapping table. Named-scope identity
/// (the part of the vertex ID that survives insertions) is driven by [`ScopeDetector`]
/// from the grammar's `tags.scm` query.
///
/// [`ScopeDetector`]: crate::scope_detector::ScopeDetector
pub struct AstWalker<'a> {
    /// The source code bytes (needed for extracting text of leaf nodes).
    source: &'a [u8],
    /// The auto-derived theory metadata. The `vertex_kinds` set is used to
    /// filter anonymous/internal tree-sitter nodes that are not part of the
    /// language's public grammar.
    theory_meta: &'a ExtractedTheoryMeta,
    /// The protocol definition (for `SchemaBuilder` validation).
    protocol: &'a Protocol,
    /// Per-language configuration.
    config: WalkerConfig,
    /// Known block kinds (merged from defaults + config).
    block_kinds: FxHashSet<String>,
    /// Named scopes indexed by `(start_byte, end_byte)` for O(log n) lookup
    /// during the tree walk. Derived from a [`ScopeDetector`] run over the
    /// full source before the walk begins.
    scope_map: BTreeMap<(usize, usize), NamedScope>,
}

impl<'a> AstWalker<'a> {
    /// Create a new walker for the given source, theory, and protocol.
    ///
    /// Runs an optional [`ScopeDetector`] over the source to build a
    /// per-file scope map. Pass `None` to disable named-scope detection
    /// (every non-root vertex gets a positional ID). Pass `Some(detector)`
    /// whose [`has_query`] is `false` for the same effect; the detector
    /// short-circuits to an empty scope list.
    ///
    /// [`ScopeDetector`]: crate::scope_detector::ScopeDetector
    /// [`has_query`]: crate::scope_detector::ScopeDetector::has_query
    #[must_use]
    pub fn new(
        source: &'a [u8],
        theory_meta: &'a ExtractedTheoryMeta,
        protocol: &'a Protocol,
        config: WalkerConfig,
        scope_detector: Option<&mut ScopeDetector>,
    ) -> Self {
        let mut block_kinds: FxHashSet<String> =
            BLOCK_KINDS.iter().map(|s| (*s).to_owned()).collect();
        for kind in &config.extra_block_kinds {
            block_kinds.insert(kind.clone());
        }

        let mut scope_map: BTreeMap<(usize, usize), NamedScope> = BTreeMap::new();
        if let Some(det) = scope_detector {
            for scope in det.scopes(source) {
                scope_map.insert((scope.node_range.start, scope.node_range.end), scope);
            }
        }

        Self {
            source,
            theory_meta,
            protocol,
            config,
            block_kinds,
            scope_map,
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

        builder.build().map_err(|e| ParseError::SchemaConstruction {
            reason: e.to_string(),
        })
    }

    /// Look up a node's named-scope entry, if any.
    fn scope_for(&self, node: tree_sitter::Node<'_>) -> Option<&NamedScope> {
        self.scope_map.get(&(node.start_byte(), node.end_byte()))
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
        // Error-recovery MISSING anonymous tokens (a zero-width `}`, `)`, `,`,
        // or keyword tree-sitter *inserts* to recover) are anonymous too, so
        // they would be dropped here; they are instead surfaced in
        // `walk_children_with_interstitials`, which scans every node's children
        // for them before the named-child walk (which also skips them).
        if !node.is_named() {
            return Ok(builder);
        }

        let kind = node.kind();

        // Skip the root "program"/"source_file"/"module" wrapper if it just wraps children.
        // We still process it to emit its children, but do so by iterating directly.
        let is_root_wrapper = parent_vertex_id.is_none()
            && (kind == "program"
                || kind == "source_file"
                || kind == "module"
                || kind == "translation_unit");

        let named_scope = if is_root_wrapper {
            None
        } else {
            self.scope_for(node)
        };

        // Determine vertex ID.
        //
        // For a node that is *both* named-scope and scope-introducing
        // (the common case: a `function_definition`, `class_definition`,
        // `module`, etc.) we must disambiguate the name exactly once
        // and reuse the same disambiguated leaf for both the vertex ID
        // here and the scope-stack frame pushed below. The
        // `record_name` / `push_recorded_scope` split on `IdGenerator`
        // makes that explicit: `record_name` is the side-effecting
        // step that bumps the parent frame's `seen` counter, the leaf
        // it returns becomes the trailing component of the vertex ID,
        // and `push_recorded_scope` later takes the same leaf without
        // re-recording.
        let (vertex_id, recorded_named_leaf) = if is_root_wrapper {
            // Root wrappers get the file path as their ID.
            (id_gen.current_prefix(), None)
        } else if let Some(scope) = named_scope {
            let leaf = id_gen.record_name(&scope.name);
            let prefix = id_gen.current_prefix();
            (format!("{prefix}::{leaf}"), Some(leaf))
        } else {
            // All other nodes get positional IDs.
            (id_gen.anonymous_id(), None)
        };

        // Determine the effective vertex kind. If the theory has extracted vertex kinds,
        // use those for validation. If the kind is unknown to the theory AND the protocol
        // has a closed obj_kinds list, fall back to "node".
        let effective_kind = if self.protocol.obj_kinds.is_empty() {
            // Open protocol: accept all kinds.
            kind
        } else if self.protocol.obj_kinds.iter().any(|k| k == kind) {
            kind
        } else if !self.theory_meta.vertex_kinds.is_empty()
            && self.theory_meta.vertex_kinds.iter().any(|k| k == kind)
        {
            // Known in the auto-derived theory even if not in the protocol's obj_kinds.
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
                        if let Some(child) = p.child(u32::try_from(i).unwrap_or(0)) {
                            if child.id() == node.id() {
                                return u32::try_from(i)
                                    .ok()
                                    .and_then(|idx| p.field_name_for_child(idx));
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

        // Store byte range for position-aware emission.
        builder = builder.constraint(&vertex_id, "start-byte", &node.start_byte().to_string());
        builder = builder.constraint(&vertex_id, "end-byte", &node.end_byte().to_string());

        // Capture any leading bytes that precede the document root. Tree-sitter
        // excludes leading `extra` tokens (a UTF-8 BOM, an awk `\<newline>`
        // line-continuation, a leading comment) from the root node's span, so
        // those bytes belong to no vertex's interstitial run and are dropped on
        // replay. Record them as a `doc-prefix` constraint on the root vertex so
        // the byte-faithful emit path can reproduce them verbatim. Only the
        // document root (no parent) with a non-zero start byte carries this;
        // every other vertex's leading gap is already a sibling interstitial.
        if parent_vertex_id.is_none() && node.start_byte() > 0 {
            if let Ok(prefix) = std::str::from_utf8(&self.source[..node.start_byte()]) {
                if !prefix.is_empty() {
                    builder = builder.constraint(&vertex_id, "doc-prefix", prefix);
                }
            }
        }

        // Record the pre-alias grammar symbol name when it differs from
        // the post-alias kind. Tree-sitter 0.25 exposes `grammar_name`
        // (the SYMBOL name as it appears in the rule body, before
        // `ALIAS { value: V }` rewriting). This is the signal that
        // disambiguates which production reached this child: when
        // emit's CHOICE dispatcher sees two alternatives both yielding
        // a child of kind `K`, one through `SYMBOL K` and one through
        // `ALIAS { ..., value: K }`, the recorded `pre-alias-symbol`
        // identifies which.
        let grammar_name = node.grammar_name();
        if grammar_name != kind {
            builder = builder.constraint(&vertex_id, "pre-alias-symbol", grammar_name);
        }

        // Emit constraints for leaf nodes (literals, identifiers, operators).
        if node.named_child_count() == 0 {
            if let Ok(text) = node.utf8_text(self.source) {
                builder = builder.constraint(&vertex_id, "literal-value", text);
            }
        }

        // Capture field-keyed anonymous-token children as `field:<name>`
        // constraints on this vertex. Tree-sitter rules of the form
        // `field('op', choice('+', '-', '*', '/'))` produce children
        // that are field-named but not themselves named nodes, so they
        // are skipped by the named-child walk above and would otherwise
        // be invisible to downstream consumers. Emitting the value here
        // lets consumers read `schema.field_text(vid, name)` directly
        // rather than reconstructing the text via start-byte / end-byte
        // arithmetic against the source buffer.
        builder = self.capture_anonymous_field_constraints(node, &vertex_id, builder);

        // Capture the faithful production trace: the ordered linearization
        // of this node's children as the parser tokenized them. Each slot
        // is either `C<kind>` for a named child (a schema edge — the
        // variant tag the emit review consumes) or `T<text>` for an
        // anonymous grammar token (a layout literal). This is the
        // variant-tag fibre of the layout complement; replaying it in the
        // put direction is byte-faithful by construction (it is the Prism
        // review *consuming* the parser's CHOICE decision rather than
        // re-deriving it). Hidden rules are inlined by tree-sitter and
        // never surface as their own child, so the trace aligns 1:1 with
        // the emitter's cursor edges and grammar string literals.
        builder = self.capture_production_trace(node, &vertex_id, builder);

        // Emit formatting constraints if enabled.
        if self.config.capture_formatting {
            builder = self.emit_formatting_constraints(node, &vertex_id, builder);
        }

        // Enter scope if this is a scope-introducing node. For a
        // named scope we reuse the disambiguated leaf computed above
        // (so the frame name matches the trailing component of
        // `vertex_id`); for an anonymous block we push a fresh
        // positional frame.
        let entered_scope = if let Some(leaf) = recorded_named_leaf {
            id_gen.push_recorded_scope(leaf);
            true
        } else if !is_root_wrapper && self.block_kinds.contains(kind) {
            id_gen.push_anonymous_scope();
            true
        } else {
            false
        };

        builder = self.walk_children_with_interstitials(node, builder, id_gen, &vertex_id)?;

        if entered_scope {
            id_gen.pop_scope();
        }

        Ok(builder)
    }

    /// Emit a zero-width, `ERROR`-kinded marker vertex for a tree-sitter
    /// error-recovery MISSING anonymous token, attached to `parent_vertex_id`.
    ///
    /// The marker is kinded like a genuine `ERROR` node where the protocol or
    /// theory admits that kind, else the closed-protocol `node` fallback. Its
    /// zero-width span (`start == end`, as for any inserted token) and its
    /// `missing` constraint (recording the elided token) let a downstream
    /// schema walker distinguish a recovered-incomplete parse from a complete
    /// one; a missing *named* token is already surfaced as a zero-width vertex
    /// by the normal walk, so this only closes the gap for anonymous ones.
    fn emit_missing_marker(
        &self,
        missing: tree_sitter::Node<'_>,
        mut builder: SchemaBuilder,
        id_gen: &mut IdGenerator,
        parent_vertex_id: &str,
    ) -> Result<SchemaBuilder, ParseError> {
        let admits_error = self.protocol.obj_kinds.is_empty()
            || self.protocol.obj_kinds.iter().any(|k| k == "ERROR")
            || self.theory_meta.vertex_kinds.iter().any(|k| k == "ERROR");
        let marker_kind = if admits_error { "ERROR" } else { "node" };
        let vertex_id = id_gen.anonymous_id();
        builder = builder.vertex(&vertex_id, marker_kind, None).map_err(|e| {
            ParseError::SchemaConstruction {
                reason: format!("missing-token marker '{vertex_id}': {e}"),
            }
        })?;
        builder = builder
            .edge(parent_vertex_id, &vertex_id, "child_of", None)
            .map_err(|e| ParseError::SchemaConstruction {
                reason: format!("missing-token edge {parent_vertex_id} -> {vertex_id}: {e}"),
            })?;
        builder = builder.constraint(&vertex_id, "start-byte", &missing.start_byte().to_string());
        builder = builder.constraint(&vertex_id, "end-byte", &missing.end_byte().to_string());
        builder = builder.constraint(&vertex_id, "missing", missing.kind());
        Ok(builder)
    }

    /// Walk named children, capturing interstitial text between them.
    ///
    /// Also computes a `chose-alt-fingerprint` constraint by trimming
    /// and joining every non-empty interstitial run. This is the
    /// categorical discriminator for the parent vertex's CHOICE alt:
    /// it survives the byte-position-stripping that
    /// `emit_pretty_roundtrip`'s by-construction simulation applies,
    /// so the CHOICE picker can dispatch deterministically against
    /// the recorded alternative even after interstitials are removed.
    /// A by-construction schema can populate this constraint directly
    /// to control which alternative the emitter picks.
    fn walk_children_with_interstitials(
        &self,
        node: tree_sitter::Node<'_>,
        mut builder: SchemaBuilder,
        id_gen: &mut IdGenerator,
        vertex_id: &str,
    ) -> Result<SchemaBuilder, ParseError> {
        let cursor = &mut node.walk();
        let children: Vec<_> = node.named_children(cursor).collect();
        let mut interstitial_idx = 0;
        let mut prev_end = node.start_byte();
        let mut fingerprint_parts: Vec<String> = Vec::new();
        let mut child_kinds: Vec<String> = Vec::new();

        for child in &children {
            let gap_start = prev_end;
            let gap_end = child.start_byte();
            builder = self.capture_interstitial(
                builder,
                vertex_id,
                gap_start,
                gap_end,
                &mut interstitial_idx,
                &mut fingerprint_parts,
            );
            // Record the named child's kind separately from the
            // literal-token fingerprint. The CHOICE picker reads the
            // kind sequence as a secondary, tiebreaker witness when
            // literal tokens alone don't discriminate alternatives.
            // Hidden-rule kinds (`_`-prefixed) are tree-sitter
            // implementation detail and never authored by humans;
            // omitting them keeps the witness language-clean and
            // matches the convention that hidden rules are inlined
            // by the emitter rather than dispatched against.
            let child_kind = child.kind();
            if !child_kind.starts_with('_') {
                child_kinds.push(child_kind.to_owned());
            }
            builder = self.walk_node(*child, builder, id_gen, Some(vertex_id))?;
            prev_end = child.end_byte();
        }

        // Surface error-recovery MISSING anonymous tokens among this node's
        // direct children. Tree-sitter inserts a zero-width MISSING token (a
        // `}`, `)`, `,`, or keyword) to recover from an incomplete construct;
        // because it is anonymous it is skipped by the named-children walk
        // above, so a recovered-incomplete parse would otherwise carry no ERROR
        // vertex and no zero-width vertex — indistinguishable from a complete
        // parse. Emit a marker for each so schema walkers that reject ERROR /
        // zero-width vertices detect the recovery.
        let missing_cursor = &mut node.walk();
        for child in node.children(missing_cursor) {
            if child.is_missing() && !child.is_named() {
                builder = self.emit_missing_marker(child, builder, id_gen, vertex_id)?;
            }
        }

        // Trailing interstitial after the last child.
        builder = self.capture_interstitial(
            builder,
            vertex_id,
            prev_end,
            node.end_byte(),
            &mut interstitial_idx,
            &mut fingerprint_parts,
        );

        if !fingerprint_parts.is_empty() {
            builder = builder.constraint(
                vertex_id,
                "chose-alt-fingerprint",
                &fingerprint_parts.join(" "),
            );
        }
        if !child_kinds.is_empty() {
            builder =
                builder.constraint(vertex_id, "chose-alt-child-kinds", &child_kinds.join(" "));
        }

        Ok(builder)
    }

    /// Capture interstitial text between `gap_start` and `gap_end` as a constraint.
    /// Walk all children of `node` (including anonymous tokens), and for
    /// each anonymous-token child that was reached through a tree-sitter
    /// `field('<name>', ...)` accessor, emit a `field:<name>` constraint
    /// on the parent vertex carrying the token's text.
    ///
    /// Tree-sitter rules like `field('direction', choice('/', '\\'))` or
    /// `field('func', choice('sigmoid','exp','log','abs'))` attach a
    /// field name to an unnamed token alternative. The named-children
    /// walk in [`walk_children_with_interstitials`] omits these (they
    /// are not named nodes), and downstream consumers previously had
    /// to recover the value by reading the source buffer between
    /// recorded byte offsets. This emits the value as a structural
    /// constraint so [`Schema::field_text`] can return it directly.
    fn capture_anonymous_field_constraints(
        &self,
        node: tree_sitter::Node<'_>,
        vertex_id: &str,
        mut builder: SchemaBuilder,
    ) -> SchemaBuilder {
        let child_count = node.child_count();
        for i in 0..child_count {
            let Some(child) = node.child(u32::try_from(i).unwrap_or(0)) else {
                continue;
            };
            // Named children carry their own vertex (and surface as edges
            // keyed by the field name in walk_node). We only need to
            // handle the unnamed tokens here.
            if child.is_named() {
                continue;
            }
            let Some(field_name) = u32::try_from(i)
                .ok()
                .and_then(|idx| node.field_name_for_child(idx))
            else {
                continue;
            };
            let Ok(text) = child.utf8_text(self.source) else {
                continue;
            };
            let sort = format!("field:{field_name}");
            builder = builder.constraint(vertex_id, &sort, text);
        }
        builder
    }

    /// Record the faithful production trace `ptrace-<slot>` for `node`.
    ///
    /// Walks every child of `node` (named children and anonymous tokens,
    /// in source order) and records each slot as `C<kind>` (a named
    /// child) or `T<text>` (an anonymous token's exact text). This is the
    /// variant-tag fibre of the layout complement
    /// ([`panproto_lens::layout_complement::TraceSlot`]). See the call
    /// site for why it is byte-faithful under replay.
    fn capture_production_trace(
        &self,
        node: tree_sitter::Node<'_>,
        vertex_id: &str,
        mut builder: SchemaBuilder,
    ) -> SchemaBuilder {
        let count = node.child_count();
        let mut slot = 0usize;
        for i in 0..count {
            let Some(child) = u32::try_from(i).ok().and_then(|i| node.child(i)) else {
                continue;
            };
            let value = if child.is_named() {
                format!("C{}", child.kind())
            } else if let Ok(text) = child.utf8_text(self.source) {
                format!("T{text}")
            } else {
                continue;
            };
            builder = builder.constraint(vertex_id, &format!("ptrace-{slot}"), &value);
            slot += 1;
        }
        builder
    }

    fn capture_interstitial(
        &self,
        mut builder: SchemaBuilder,
        vertex_id: &str,
        gap_start: usize,
        gap_end: usize,
        idx: &mut usize,
        fingerprint: &mut Vec<String>,
    ) -> SchemaBuilder {
        if gap_end > gap_start && gap_end <= self.source.len() {
            if let Ok(gap_text) = std::str::from_utf8(&self.source[gap_start..gap_end]) {
                if !gap_text.is_empty() {
                    let sort = format!("interstitial-{}", *idx);
                    builder = builder.constraint(vertex_id, &sort, gap_text);
                    builder = builder.constraint(
                        vertex_id,
                        &format!("{sort}-start-byte"),
                        &gap_start.to_string(),
                    );
                    // The run's end byte in the *original* source. Replay
                    // decides which fragments a rewritten one supersedes by
                    // comparing source spans, so the span has to be recorded
                    // rather than recovered from the text's current length:
                    // an edited interstitial no longer has its source length.
                    builder = builder.constraint(
                        vertex_id,
                        &format!("{sort}-end-byte"),
                        &gap_end.to_string(),
                    );
                    *idx += 1;
                    let trimmed = gap_text.trim();
                    if !trimmed.is_empty() {
                        fingerprint.push(trimmed.to_owned());
                    }
                }
            }
        }
        builder
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
                let blank_lines = memchr::memchr_iter(b'\n', gap).count().saturating_sub(1);
                if blank_lines > 0 {
                    builder = builder.constraint(
                        vertex_id,
                        "blank-lines-before",
                        &blank_lines.to_string(),
                    );
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
            schema_composition: None,
            instance_composition: None,
            obj_kinds: vec![], // Empty = open protocol, accepts all kinds.
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

    /// Helper to get a grammar by name from panproto-grammars.
    #[cfg(feature = "grammars")]
    fn get_grammar(name: &str) -> panproto_grammars::Grammar {
        panproto_grammars::grammars()
            .into_iter()
            .find(|g| g.name == name)
            .unwrap_or_else(|| panic!("grammar '{name}' not enabled in features"))
    }

    #[test]
    #[cfg(feature = "grammars")]
    fn walk_simple_typescript() {
        let source = b"function greet(name: string): string { return name; }";
        let grammar = get_grammar("typescript");

        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&grammar.language).unwrap();
        let tree = parser.parse(source, None).unwrap();

        let protocol = make_test_protocol();
        let meta = make_test_meta();
        let mut detector =
            crate::scope_detector::ScopeDetector::new(&grammar.language, grammar.tags_query, None)
                .unwrap();
        let walker = AstWalker::new(
            source,
            &meta,
            &protocol,
            WalkerConfig::standard(),
            Some(&mut detector),
        );

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

        // When tags.scm is present, the function name should appear in a vertex ID.
        if detector.has_query() {
            let has_greet = schema
                .vertices
                .keys()
                .any(|n| n.to_string().ends_with("::greet"));
            assert!(
                has_greet,
                "expected a vertex ID ending in ::greet, got: {:?}",
                schema
                    .vertices
                    .keys()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    #[cfg(feature = "grammars")]
    fn walk_simple_python() {
        let source = b"def add(a, b):\n    return a + b\n";
        let grammar = get_grammar("python");

        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&grammar.language).unwrap();
        let tree = parser.parse(source, None).unwrap();

        let protocol = make_test_protocol();
        let meta = make_test_meta();
        let mut detector =
            crate::scope_detector::ScopeDetector::new(&grammar.language, grammar.tags_query, None)
                .unwrap();
        let walker = AstWalker::new(
            source,
            &meta,
            &protocol,
            WalkerConfig::standard(),
            Some(&mut detector),
        );

        let schema = walker.walk(&tree, "test.py").unwrap();

        assert!(
            schema.vertices.len() > 1,
            "expected multiple vertices, got {}",
            schema.vertices.len()
        );

        if detector.has_query() {
            let has_add = schema
                .vertices
                .keys()
                .any(|n| n.to_string().ends_with("::add"));
            assert!(has_add, "expected ::add vertex");
        }
    }

    #[test]
    #[cfg(feature = "grammars")]
    fn walk_simple_rust() {
        let source = b"fn verify_push() {}\nstruct Foo;\nimpl Foo { fn bar(&self) {} }\n";
        let grammar = get_grammar("rust");

        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&grammar.language).unwrap();
        let tree = parser.parse(source, None).unwrap();

        let protocol = make_test_protocol();
        let meta = make_test_meta();
        let mut detector =
            crate::scope_detector::ScopeDetector::new(&grammar.language, grammar.tags_query, None)
                .unwrap();
        let walker = AstWalker::new(
            source,
            &meta,
            &protocol,
            WalkerConfig::standard(),
            Some(&mut detector),
        );

        let schema = walker.walk(&tree, "test.rs").unwrap();

        assert!(
            schema.vertices.len() > 1,
            "expected multiple vertices, got {}",
            schema.vertices.len()
        );

        if detector.has_query() {
            let vertex_ids: Vec<String> = schema.vertices.keys().map(ToString::to_string).collect();

            // Rust's function_item — the regression from issue #34 — must be
            // detected as a named scope now.
            assert!(
                vertex_ids.iter().any(|id| id.ends_with("::verify_push")),
                "expected ::verify_push named scope, got: {vertex_ids:?}"
            );
            assert!(
                vertex_ids.iter().any(|id| id.ends_with("::Foo")),
                "expected ::Foo named scope, got: {vertex_ids:?}"
            );
        }
    }

    /// Helper: parse source with a grammar, walk to Schema, emit back, compare.
    #[cfg(feature = "group-data")]
    fn assert_roundtrip(grammar_name: &str, source: &[u8], file_path: &str) {
        use crate::registry::AstParser;
        let grammar = panproto_grammars::grammars()
            .into_iter()
            .find(|g| g.name == grammar_name)
            .unwrap_or_else(|| panic!("grammar '{grammar_name}' not enabled"));

        let config = crate::languages::walker_configs::walker_config_for(grammar_name);
        let lang_parser = crate::languages::common::LanguageParser::from_language(
            grammar_name,
            grammar.extensions.to_vec(),
            grammar.language,
            grammar.node_types,
            grammar.tags_query,
            config,
        )
        .unwrap();

        let schema = lang_parser.parse(source, file_path).unwrap();
        let emitted = lang_parser.emit(&schema).unwrap();

        assert_eq!(
            std::str::from_utf8(source).unwrap(),
            std::str::from_utf8(&emitted).unwrap(),
            "round-trip failed for {grammar_name}: emitted bytes differ from source"
        );
    }

    #[test]
    #[cfg(feature = "group-data")]
    fn roundtrip_json_simple() {
        assert_roundtrip("json", br#"{"name": "test", "value": 42}"#, "test.json");
    }

    #[test]
    #[cfg(feature = "group-data")]
    fn roundtrip_json_formatted() {
        let source =
            b"{\n  \"name\": \"test\",\n  \"value\": 42,\n  \"nested\": {\n    \"a\": true\n  }\n}";
        assert_roundtrip("json", source, "test.json");
    }

    #[test]
    #[cfg(feature = "group-data")]
    fn roundtrip_json_array() {
        let source = b"[\n  1,\n  2,\n  3\n]";
        assert_roundtrip("json", source, "test.json");
    }

    #[test]
    #[cfg(feature = "group-data")]
    fn roundtrip_xml_simple() {
        let source = b"<root>\n  <child attr=\"val\">text</child>\n</root>";
        assert_roundtrip("xml", source, "test.xml");
    }

    #[test]
    #[cfg(feature = "group-data")]
    fn roundtrip_yaml_simple() {
        let source = b"name: test\nvalue: 42\nnested:\n  a: true\n";
        assert_roundtrip("yaml", source, "test.yaml");
    }

    #[test]
    #[cfg(feature = "group-data")]
    fn roundtrip_toml_simple() {
        let source = b"[package]\nname = \"test\"\nversion = \"0.1.0\"\n";
        assert_roundtrip("toml", source, "test.toml");
    }

    #[cfg(feature = "group-data")]
    fn parse_with(grammar_name: &str, source: &[u8], file_path: &str) -> panproto_schema::Schema {
        use crate::registry::AstParser;
        let grammar = panproto_grammars::grammars()
            .into_iter()
            .find(|g| g.name == grammar_name)
            .unwrap_or_else(|| panic!("grammar '{grammar_name}' not enabled"));
        let config = crate::languages::walker_configs::walker_config_for(grammar_name);
        let lang_parser = crate::languages::common::LanguageParser::from_language(
            grammar_name,
            grammar.extensions.to_vec(),
            grammar.language,
            grammar.node_types,
            grammar.tags_query,
            config,
        )
        .unwrap();
        lang_parser.parse(source, file_path).unwrap()
    }

    #[test]
    #[cfg(feature = "group-data")]
    fn fingerprint_and_child_kinds_emitted_separately() {
        // The walker must emit `chose-alt-fingerprint` and
        // `chose-alt-child-kinds` as TWO distinct constraints. The
        // CHOICE picker reads them independently: literal-token
        // matches drive primary scoring, child-kind matches act as a
        // tiebreaker. Mixing them would let punctuation in kind names
        // contaminate the literal score.
        let schema = parse_with("json", br#"{"a": 1}"#, "test.json");

        let saw_fingerprint = schema.constraints.values().any(|cs| {
            cs.iter()
                .any(|c| c.sort.as_ref() == "chose-alt-fingerprint")
        });
        let saw_child_kinds = schema.constraints.values().any(|cs| {
            cs.iter()
                .any(|c| c.sort.as_ref() == "chose-alt-child-kinds")
        });

        assert!(
            saw_fingerprint,
            "walker must emit chose-alt-fingerprint (literal-token witness)"
        );
        assert!(
            saw_child_kinds,
            "walker must emit chose-alt-child-kinds (named-kind witness)"
        );
    }

    #[test]
    #[cfg(feature = "group-data")]
    fn child_kinds_excludes_hidden_rules() {
        // Hidden rules (`_`-prefixed) are tree-sitter implementation
        // detail and must not appear in the kind witness.
        let schema = parse_with("json", br#"{"k": "v"}"#, "test.json");

        for cs in schema.constraints.values() {
            for c in cs {
                if c.sort.as_ref() == "chose-alt-child-kinds" {
                    for kind in c.value.split_whitespace() {
                        assert!(
                            !kind.starts_with('_'),
                            "hidden-rule kind '{kind}' must not appear in chose-alt-child-kinds"
                        );
                    }
                }
            }
        }
    }
}
