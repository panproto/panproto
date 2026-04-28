#![allow(
    clippy::module_name_repetitions,
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::map_unwrap_or,
    clippy::option_if_let_else,
    clippy::elidable_lifetime_names,
    clippy::items_after_statements,
    clippy::needless_pass_by_value,
    clippy::single_match_else,
    clippy::manual_let_else,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::single_char_pattern,
    clippy::naive_bytecount,
    clippy::expect_used,
    clippy::redundant_pub_crate,
    clippy::used_underscore_binding,
    clippy::redundant_field_names,
    clippy::struct_field_names,
    clippy::redundant_else,
    clippy::similar_names
)]

//! De-novo source emission from a by-construction schema.
//!
//! [`AstParser::emit`] reconstructs source from byte-position fragments
//! that the parser stored on the schema during `parse`. That works for
//! edit pipelines (`parse → transform → emit`) but fails for schemas
//! built by hand (`SchemaBuilder` with no parse history): they carry
//! no `start-byte`, no `interstitial-N`, no `literal-value`, and the
//! reconstructor returns `Err(EmitFailed { reason: "schema has no
//! text fragments" })`.
//!
//! This module renders such schemas to source bytes by walking
//! tree-sitter's `grammar.json` production rules. For each schema
//! vertex of kind `K`, the walker looks up `K`'s production in the
//! grammar and emits its body in order:
//!
//! - `STRING` nodes contribute literal token bytes directly.
//! - `SYMBOL` and `FIELD` nodes recurse into the schema's children,
//!   matching by edge kind (which is the tree-sitter field name).
//! - `SEQ` emits its members in order.
//! - `CHOICE` picks the alternative whose head `SYMBOL` matches an
//!   actual child kind, or whose terminals appear in the rendered
//!   prefix; falls back to the first non-`BLANK` alternative when no
//!   alternative matches.
//! - `REPEAT` and `REPEAT1` emit their content once per matching
//!   child edge in declared order.
//! - `OPTIONAL` emits its content iff a corresponding child edge or
//!   constraint is populated.
//! - `PATTERN` is a regex placeholder for variable-text terminals
//!   (identifiers, numbers, quoted strings). The walker emits a
//!   `literal-value` constraint when present and otherwise falls
//!   back to a placeholder derived from the regex shape.
//! - `BLANK`, `TOKEN`, `IMMEDIATE_TOKEN`, `ALIAS`, `PREC*` are
//!   handled transparently (the inner content is emitted; the
//!   wrapper is dropped).
//!
//! Whitespace and indentation come from a `FormatPolicy` applied
//! during emission. The default policy inserts a single space between
//! adjacent tokens, a newline after `;` / `}` / `{`, and tracks an
//! indent counter on `{` / `}` boundaries.
//!
//! Output is *syntactically valid* for any grammar that ships
//! `grammar.json`. Idiomatic formatting (rustfmt-style spacing rules,
//! per-language conventions) is a polish layer that lives outside
//! this module.

use std::collections::BTreeMap;

use panproto_schema::{Edge, Schema};
use serde::Deserialize;

use crate::error::ParseError;

// ═══════════════════════════════════════════════════════════════════
// Grammar JSON model
// ═══════════════════════════════════════════════════════════════════

/// A single tree-sitter production rule.
///
/// Mirrors the shape emitted by `tree-sitter generate`: every node has
/// a `type` discriminator that selects a structural variant. The
/// untyped subset (`PATTERN`, `STRING`, `SYMBOL`, `BLANK`) handles
/// terminals; the structural subset (`SEQ`, `CHOICE`, `REPEAT`,
/// `REPEAT1`, `OPTIONAL`, `FIELD`, `ALIAS`, `TOKEN`,
/// `IMMEDIATE_TOKEN`, `PREC*`) builds composite productions.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum Production {
    /// Concatenation of productions.
    #[serde(rename = "SEQ")]
    Seq {
        /// Ordered members; each is emitted in turn.
        members: Vec<Self>,
    },
    /// Alternation between productions.
    #[serde(rename = "CHOICE")]
    Choice {
        /// Alternatives; the walker picks one based on the schema's
        /// children and constraints.
        members: Vec<Self>,
    },
    /// Zero-or-more repetition.
    #[serde(rename = "REPEAT")]
    Repeat {
        /// The repeated body.
        content: Box<Self>,
    },
    /// One-or-more repetition.
    #[serde(rename = "REPEAT1")]
    Repeat1 {
        /// The repeated body.
        content: Box<Self>,
    },
    /// Optional inclusion (zero or one).
    ///
    /// Tree-sitter usually emits `OPTIONAL` as `CHOICE { content,
    /// BLANK }`, but recent generator versions also emit explicit
    /// `OPTIONAL` nodes; both shapes are accepted.
    #[serde(rename = "OPTIONAL")]
    Optional {
        /// The optional body.
        content: Box<Self>,
    },
    /// Reference to another rule by name.
    #[serde(rename = "SYMBOL")]
    Symbol {
        /// Name of the referenced rule (matches a vertex kind on the
        /// schema side).
        name: String,
    },
    /// Literal token bytes.
    #[serde(rename = "STRING")]
    String {
        /// The literal token. Emitted verbatim.
        value: String,
    },
    /// Regex-matched terminal.
    ///
    /// At parse time this matches arbitrary bytes; at emit time the
    /// walker substitutes a `literal-value` constraint when present
    /// and falls back to a placeholder otherwise.
    #[serde(rename = "PATTERN")]
    Pattern {
        /// The original regex.
        value: String,
    },
    /// The empty production. Emits nothing.
    #[serde(rename = "BLANK")]
    Blank,
    /// Named field over a content production.
    ///
    /// The field `name` matches an edge kind on the schema side; the
    /// walker resolves the corresponding child vertex and recurses
    /// into `content` with that child as context.
    #[serde(rename = "FIELD")]
    Field {
        /// Field name (matches edge kind).
        name: String,
        /// The contents of the field.
        content: Box<Self>,
    },
    /// An aliased production.
    ///
    /// `value` records the parser-visible kind; the walker emits
    /// `content` and ignores the alias rename.
    #[serde(rename = "ALIAS")]
    Alias {
        /// The aliased content.
        content: Box<Self>,
        /// Whether the alias is a named node.
        #[serde(default)]
        named: bool,
        /// The alias's surface name.
        #[serde(default)]
        value: String,
    },
    /// A token wrapper.
    ///
    /// Tree-sitter uses `TOKEN` to mark a sub-rule as a single
    /// lexical token; the walker emits the inner content unchanged.
    #[serde(rename = "TOKEN")]
    Token {
        /// The wrapped content.
        content: Box<Self>,
    },
    /// An immediate-token wrapper (no preceding whitespace).
    ///
    /// Treated like [`Production::Token`] for emit purposes.
    #[serde(rename = "IMMEDIATE_TOKEN")]
    ImmediateToken {
        /// The wrapped content.
        content: Box<Self>,
    },
    /// Precedence wrapper.
    #[serde(rename = "PREC")]
    Prec {
        /// Precedence value (numeric or string). Ignored at emit time.
        #[allow(dead_code)]
        value: serde_json::Value,
        /// The wrapped content.
        content: Box<Self>,
    },
    /// Left-associative precedence wrapper.
    #[serde(rename = "PREC_LEFT")]
    PrecLeft {
        /// Precedence value. Ignored at emit time.
        #[allow(dead_code)]
        value: serde_json::Value,
        /// The wrapped content.
        content: Box<Self>,
    },
    /// Right-associative precedence wrapper.
    #[serde(rename = "PREC_RIGHT")]
    PrecRight {
        /// Precedence value. Ignored at emit time.
        #[allow(dead_code)]
        value: serde_json::Value,
        /// The wrapped content.
        content: Box<Self>,
    },
    /// Dynamic precedence wrapper.
    #[serde(rename = "PREC_DYNAMIC")]
    PrecDynamic {
        /// Precedence value. Ignored at emit time.
        #[allow(dead_code)]
        value: serde_json::Value,
        /// The wrapped content.
        content: Box<Self>,
    },
}

/// A grammar's production-rule table, deserialized from `grammar.json`.
///
/// Only the fields the emitter consumes are decoded; precedences,
/// conflicts, externals, and other parser-only metadata are ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct Grammar {
    /// Grammar name (e.g. `"rust"`, `"typescript"`).
    #[allow(dead_code)]
    pub name: String,
    /// Map from rule name (a vertex kind on the schema side) to
    /// production. Entries are kept in lexical order so iteration
    /// is deterministic.
    pub rules: BTreeMap<String, Production>,
    /// Supertypes declared in the grammar's `supertypes` field. A
    /// supertype is a rule whose body is a `CHOICE` of `SYMBOL`
    /// references; tree-sitter parsers report a node's kind as one
    /// of the subtypes (e.g. `identifier`, `typed_parameter`) rather
    /// than the supertype name (`parameter`), so the emitter needs to
    /// know that a child kind in a subtype set should match the
    /// supertype name when a SYMBOL references it.
    #[serde(default, deserialize_with = "deserialize_supertypes")]
    pub supertypes: std::collections::HashSet<String>,
}

fn deserialize_supertypes<'de, D>(
    deserializer: D,
) -> Result<std::collections::HashSet<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let entries: Vec<serde_json::Value> = Vec::deserialize(deserializer)?;
    let mut out = std::collections::HashSet::new();
    for entry in entries {
        match entry {
            serde_json::Value::String(s) => {
                out.insert(s);
            }
            serde_json::Value::Object(map) => {
                if let Some(serde_json::Value::String(name)) = map.get("name") {
                    out.insert(name.clone());
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

impl Grammar {
    /// Parse a grammar's `grammar.json` bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::EmitFailed`] when the bytes are not a
    /// valid `grammar.json` document.
    pub fn from_bytes(protocol: &str, bytes: &[u8]) -> Result<Self, ParseError> {
        serde_json::from_slice(bytes).map_err(|e| ParseError::EmitFailed {
            protocol: protocol.to_owned(),
            reason: format!("grammar.json deserialization failed: {e}"),
        })
    }
}

// ═══════════════════════════════════════════════════════════════════
// Format policy
// ═══════════════════════════════════════════════════════════════════

/// Whitespace and indentation policy applied during emission.
///
/// The default policy inserts a single space between adjacent tokens,
/// a newline after `;` / `}` / `{`, and tracks indent on `{` / `}`
/// boundaries. Per-language overrides (idiomatic indent width,
/// trailing-comma rules, blank-line conventions) can ride alongside
/// this struct in a follow-up branch; today's defaults aim only for
/// syntactic validity.
#[derive(Debug, Clone)]
pub struct FormatPolicy {
    /// Number of spaces per indent level.
    pub indent_width: usize,
    /// Tokens after which the walker breaks to a new line.
    pub line_break_after: Vec<String>,
    /// Tokens that increase indent on emission.
    pub indent_open: Vec<String>,
    /// Tokens that decrease indent on emission.
    pub indent_close: Vec<String>,
}

impl Default for FormatPolicy {
    fn default() -> Self {
        Self {
            indent_width: 2,
            line_break_after: vec![";".into(), "{".into(), "}".into()],
            indent_open: vec!["{".into()],
            indent_close: vec!["}".into()],
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Emitter
// ═══════════════════════════════════════════════════════════════════

/// Emit a by-construction schema to source bytes.
///
/// `protocol` is the grammar / language name (used in error messages
/// and to label the entry point).
///
/// The walker treats `schema.entries` as the ordered list of root
/// vertices, falling back to a deterministic by-id ordering when
/// `entries` is empty. Each root is emitted using the production
/// associated with its kind in `grammar.rules`.
///
/// # Errors
///
/// Returns [`ParseError::EmitFailed`] when:
///
/// - the schema has no vertices
/// - a root vertex's kind is not a grammar rule
/// - a `SYMBOL` reference points at a kind with no rule and no schema
///   child to resolve it to
/// - a required `FIELD` has no corresponding edge in the schema
pub fn emit_pretty(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    policy: &FormatPolicy,
) -> Result<Vec<u8>, ParseError> {
    let roots = collect_roots(schema);
    if roots.is_empty() {
        return Err(ParseError::EmitFailed {
            protocol: protocol.to_owned(),
            reason: "schema has no entry vertices".to_owned(),
        });
    }

    let mut out = Output::new(policy);
    for (i, root) in roots.iter().enumerate() {
        if i > 0 {
            out.newline();
        }
        emit_vertex(protocol, schema, grammar, root, &mut out)?;
    }
    Ok(out.finish())
}

fn collect_roots(schema: &Schema) -> Vec<&panproto_gat::Name> {
    if !schema.entries.is_empty() {
        return schema
            .entries
            .iter()
            .filter(|name| schema.vertices.contains_key(*name))
            .collect();
    }

    // Fallback: every vertex that is not the target of any structural edge
    // (sorted by id for determinism).
    let mut targets: std::collections::HashSet<&panproto_gat::Name> =
        std::collections::HashSet::new();
    for edge in schema.edges.keys() {
        targets.insert(&edge.tgt);
    }
    let mut roots: Vec<&panproto_gat::Name> = schema
        .vertices
        .keys()
        .filter(|name| !targets.contains(name))
        .collect();
    roots.sort();
    roots
}

fn emit_vertex(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    vertex_id: &panproto_gat::Name,
    out: &mut Output<'_>,
) -> Result<(), ParseError> {
    let vertex = schema
        .vertices
        .get(vertex_id)
        .ok_or_else(|| ParseError::EmitFailed {
            protocol: protocol.to_owned(),
            reason: format!("vertex '{vertex_id}' not found"),
        })?;

    // Leaf shortcut: a vertex carrying a `literal-value` constraint
    // and no outgoing structural edges is a terminal token. Emit the
    // captured value directly. This handles identifiers, numeric
    // literals, and string literals that the parser stored as
    // `literal-value` even on by-construction schemas.
    if let Some(literal) = literal_value(schema, vertex_id) {
        if children_for(schema, vertex_id).is_empty() {
            out.token(literal);
            return Ok(());
        }
    }

    let kind = vertex.kind.as_ref();
    let edges = children_for(schema, vertex_id);
    if let Some(rule) = grammar.rules.get(kind) {
        let mut cursor = ChildCursor::new(&edges);
        return emit_production(protocol, schema, grammar, vertex_id, rule, &mut cursor, out);
    }

    // No rule for this kind. The parser produced it via an ALIAS
    // (tree-sitter's `alias($.something, $.actual_kind)`) or via an
    // external scanner (e.g. YAML's `document` root). Fall back to
    // walking the children directly so the inner content survives;
    // surrounding tokens — whose only source is the missing rule —
    // are necessarily absent.
    for edge in &edges {
        emit_vertex(protocol, schema, grammar, &edge.tgt, out)?;
    }
    Ok(())
}

/// Linear cursor over a vertex's outgoing edges, used to thread
/// children through a production rule without double-consuming them.
struct ChildCursor<'a> {
    edges: &'a [&'a Edge],
    consumed: Vec<bool>,
}

impl<'a> ChildCursor<'a> {
    fn new(edges: &'a [&'a Edge]) -> Self {
        Self {
            edges,
            consumed: vec![false; edges.len()],
        }
    }

    /// Take the next unconsumed edge whose kind equals `field_name`.
    fn take_field(&mut self, field_name: &str) -> Option<&'a Edge> {
        for (i, edge) in self.edges.iter().enumerate() {
            if !self.consumed[i] && edge.kind.as_ref() == field_name {
                self.consumed[i] = true;
                return Some(edge);
            }
        }
        None
    }

    /// Take the next unconsumed edge whose target vertex satisfies
    /// `predicate`. Returns the edge and the underlying production
    /// resolution path is the caller's job.
    fn take_matching(&mut self, predicate: impl Fn(&Edge) -> bool) -> Option<&'a Edge> {
        for (i, edge) in self.edges.iter().enumerate() {
            if !self.consumed[i] && predicate(edge) {
                self.consumed[i] = true;
                return Some(edge);
            }
        }
        None
    }

    /// Whether any unconsumed edge satisfies `predicate`.
    fn has_matching(&self, predicate: impl Fn(&Edge) -> bool) -> bool {
        self.edges
            .iter()
            .enumerate()
            .any(|(i, edge)| !self.consumed[i] && predicate(edge))
    }
}

thread_local! {
    static EMIT_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn emit_production(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    vertex_id: &panproto_gat::Name,
    production: &Production,
    cursor: &mut ChildCursor<'_>,
    out: &mut Output<'_>,
) -> Result<(), ParseError> {
    let depth = EMIT_DEPTH.with(|d| {
        let v = d.get() + 1;
        d.set(v);
        v
    });
    if depth > 500 {
        EMIT_DEPTH.with(|d| d.set(d.get() - 1));
        return Err(ParseError::EmitFailed {
            protocol: protocol.to_owned(),
            reason: format!(
                "emit_production recursion >500 (likely a cyclic grammar; \
                     vertex='{vertex_id}')"
            ),
        });
    }
    let result = emit_production_inner(
        protocol, schema, grammar, vertex_id, production, cursor, out,
    );
    EMIT_DEPTH.with(|d| d.set(d.get() - 1));
    result
}

fn emit_production_inner(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    vertex_id: &panproto_gat::Name,
    production: &Production,
    cursor: &mut ChildCursor<'_>,
    out: &mut Output<'_>,
) -> Result<(), ParseError> {
    match production {
        Production::String { value } => {
            out.token(value);
            Ok(())
        }
        Production::Pattern { value } => {
            if let Some(literal) = literal_value(schema, vertex_id) {
                out.token(literal);
            } else {
                out.token(&placeholder_for_pattern(value));
            }
            Ok(())
        }
        Production::Blank => Ok(()),
        Production::Symbol { name } => {
            if name.starts_with('_') {
                // Hidden rule: not a vertex kind on the schema side.
                // Inline-expand the rule body so its children take
                // edges from the current cursor, instead of trying to
                // take a single child edge that "satisfies" the
                // hidden rule and discarding the rest of the body
                // (which would drop tokens like `=` and the trailing
                // value SYMBOL inside e.g. TOML's `_inline_pair`).
                if let Some(rule) = grammar.rules.get(name) {
                    emit_production(protocol, schema, grammar, vertex_id, rule, cursor, out)
                } else {
                    // External hidden rule (declared in the
                    // grammar's `externals` block, scanned by C code,
                    // not listed in `rules`). Heuristic fallback:
                    // line-ending / EOF externals are universally
                    // newline-or-empty, so emitting a single newline
                    // is the right default for grammars like TOML
                    // whose `pair` SEQ trails into
                    // `_line_ending_or_eof`. Anything else falls
                    // through silently.
                    if name.contains("line_ending")
                        || name.contains("newline")
                        || name.ends_with("_or_eof")
                    {
                        out.newline();
                    }
                    Ok(())
                }
            } else if let Some(edge) = take_symbol_match(grammar, schema, cursor, name) {
                emit_vertex(protocol, schema, grammar, &edge.tgt, out)
            } else if vertex_id_kind(schema, vertex_id) == Some(name.as_str()) {
                let rule = grammar
                    .rules
                    .get(name)
                    .ok_or_else(|| ParseError::EmitFailed {
                        protocol: protocol.to_owned(),
                        reason: format!("no production for SYMBOL '{name}'"),
                    })?;
                emit_production(protocol, schema, grammar, vertex_id, rule, cursor, out)
            } else {
                // Named rule with no matching child: emit nothing and
                // let the surrounding CHOICE / OPTIONAL / REPEAT
                // resolve the absence.
                Ok(())
            }
        }
        Production::Seq { members } => {
            for member in members {
                emit_production(protocol, schema, grammar, vertex_id, member, cursor, out)?;
            }
            Ok(())
        }
        Production::Choice { members } => {
            if let Some(matched) =
                pick_choice_with_cursor(schema, grammar, vertex_id, cursor, members)
            {
                emit_production(protocol, schema, grammar, vertex_id, matched, cursor, out)
            } else {
                Ok(())
            }
        }
        Production::Repeat { content } | Production::Repeat1 { content } => {
            let mut emitted_any = false;
            loop {
                let cursor_snap = cursor.consumed.clone();
                let out_snap = out.snapshot();
                let consumed_before = cursor.consumed.iter().filter(|&&c| c).count();
                let result =
                    emit_production(protocol, schema, grammar, vertex_id, content, cursor, out);
                let consumed_after = cursor.consumed.iter().filter(|&&c| c).count();
                if result.is_err() || consumed_after == consumed_before {
                    cursor.consumed = cursor_snap;
                    out.restore(out_snap);
                    break;
                }
                emitted_any = true;
            }
            if matches!(production, Production::Repeat1 { .. }) && !emitted_any {
                emit_production(protocol, schema, grammar, vertex_id, content, cursor, out)?;
            }
            Ok(())
        }
        Production::Optional { content } => {
            let cursor_snap = cursor.consumed.clone();
            let out_snap = out.snapshot();
            let consumed_before = cursor.consumed.iter().filter(|&&c| c).count();
            emit_production(protocol, schema, grammar, vertex_id, content, cursor, out)?;
            let consumed_after = cursor.consumed.iter().filter(|&&c| c).count();
            if consumed_after == consumed_before
                && !has_relevant_constraint(content, schema, vertex_id)
            {
                cursor.consumed = cursor_snap;
                out.restore(out_snap);
            }
            Ok(())
        }
        Production::Field { name, content } => {
            if let Some(edge) = cursor.take_field(name) {
                emit_in_child_context(protocol, schema, grammar, &edge.tgt, content, out)
            } else if first_symbol(content).is_none() {
                // FIELD wraps a non-child production (e.g. a literal
                // STRING operator like `+` in a binary_expression, or
                // a CHOICE of STRING tokens). The walker captures
                // these as interstitials rather than vertices, so the
                // schema has no field edge to consume; emit the
                // content in place so the operator / keyword survives
                // the round-trip.
                emit_production(protocol, schema, grammar, vertex_id, content, cursor, out)
            } else {
                Ok(())
            }
        }
        Production::Alias {
            content,
            named,
            value,
        } => {
            // A named ALIAS rewrites the parser-visible kind to
            // `value`. If the cursor has an unconsumed child whose
            // kind matches that alias name, take it and emit the
            // alias body in the child's context — without this branch
            // the inner `SYMBOL identifier` would walk on the parent
            // cursor and never see the renamed child.
            if *named && !value.is_empty() {
                if let Some(edge) = cursor.take_matching(|edge| {
                    schema
                        .vertices
                        .get(&edge.tgt)
                        .map(|v| v.kind.as_ref() == value.as_str())
                        .unwrap_or(false)
                }) {
                    return emit_vertex(protocol, schema, grammar, &edge.tgt, out);
                }
            }
            emit_production(protocol, schema, grammar, vertex_id, content, cursor, out)
        }
        Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. } => {
            emit_production(protocol, schema, grammar, vertex_id, content, cursor, out)
        }
    }
}

/// Take the next cursor edge whose target vertex's kind matches the
/// SYMBOL `name` directly or via inline expansion of a hidden rule.
fn take_symbol_match<'a>(
    grammar: &Grammar,
    schema: &Schema,
    cursor: &mut ChildCursor<'a>,
    name: &str,
) -> Option<&'a Edge> {
    cursor.take_matching(|edge| {
        let target_kind = schema.vertices.get(&edge.tgt).map(|v| v.kind.as_ref());
        kind_satisfies_symbol(grammar, target_kind, name)
    })
}

fn kind_satisfies_symbol(grammar: &Grammar, target_kind: Option<&str>, name: &str) -> bool {
    if target_kind == Some(name) {
        return true;
    }
    // Inline-expand:
    //
    // - Hidden rules (`_`-prefixed) — tree-sitter's pass-through
    //   rules whose alternatives become the actual node kinds.
    // - Declared supertypes — rules listed in the grammar's
    //   `supertypes` block, whose subtypes (the SYMBOL alternatives)
    //   are what the parser reports as the actual node kind. A
    //   reference to `parameter` should match an `identifier` on the
    //   schema side because identifier is one of parameter's
    //   subtypes.
    //
    // A regular non-hidden, non-supertype rule reference like
    // `dotted_key` still requires exact kind match on the schema
    // side; otherwise `kind_satisfies_symbol` becomes a transitive
    // closure that accepts every leaf the rule can produce, and
    // CHOICE dispatch picks the wrong alternative (e.g. choosing
    // `dotted_key` for a single bare key).
    let allow_expand = name.starts_with('_') || grammar.supertypes.contains(name);
    if !allow_expand {
        return false;
    }
    let mut visited: std::collections::HashSet<&str> = std::collections::HashSet::new();
    fn walk<'g>(
        grammar: &'g Grammar,
        production: &'g Production,
        target_kind: Option<&str>,
        visited: &mut std::collections::HashSet<&'g str>,
    ) -> bool {
        match production {
            Production::Symbol { name } => {
                if Some(name.as_str()) == target_kind {
                    return true;
                }
                // Continue expansion through hidden rules or declared
                // supertypes only; a regular rule reference must match
                // by kind on the schema side.
                if !name.starts_with('_') && !grammar.supertypes.contains(name.as_str()) {
                    return false;
                }
                if visited.insert(name.as_str()) {
                    if let Some(rule) = grammar.rules.get(name) {
                        return walk(grammar, rule, target_kind, visited);
                    }
                }
                false
            }
            Production::Choice { members } | Production::Seq { members } => members
                .iter()
                .any(|m| walk(grammar, m, target_kind, visited)),
            Production::Alias {
                content,
                named,
                value,
            } => {
                // ALIAS rewrites the parser-visible kind: a node that
                // structurally is the inner `content` is reported as
                // `value` when `named`. The schema's vertex kind is
                // therefore `value`, and the walker should treat that
                // as a match for the surrounding SYMBOL.
                if *named && Some(value.as_str()) == target_kind {
                    return true;
                }
                walk(grammar, content, target_kind, visited)
            }
            Production::Repeat { content }
            | Production::Repeat1 { content }
            | Production::Optional { content }
            | Production::Field { content, .. }
            | Production::Token { content }
            | Production::ImmediateToken { content }
            | Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. } => {
                walk(grammar, content, target_kind, visited)
            }
            _ => false,
        }
    }
    if let Some(rule) = grammar.rules.get(name) {
        return walk(grammar, rule, target_kind, &mut visited);
    }
    false
}

fn emit_in_child_context(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    child_id: &panproto_gat::Name,
    production: &Production,
    out: &mut Output<'_>,
) -> Result<(), ParseError> {
    // If `production` is a structural wrapper (CHOICE / SEQ /
    // OPTIONAL / ...) whose referenced symbols cover the child's own
    // kind, the child IS the production's target node and the right
    // emit path is `emit_vertex(child)` (which honours the
    // literal-value leaf shortcut). Without this guard, FIELD(pattern,
    // CHOICE { _pattern, self }) on an identifier child walks the
    // CHOICE on the identifier's empty cursor, falls through to the
    // first non-BLANK alt, and loses the captured identifier text.
    if !matches!(production, Production::Symbol { .. }) {
        let child_kind = schema.vertices.get(child_id).map(|v| v.kind.as_ref());
        let symbols = referenced_symbols(production);
        if symbols
            .iter()
            .any(|s| kind_satisfies_symbol(grammar, child_kind, s) || child_kind == Some(s))
        {
            return emit_vertex(protocol, schema, grammar, child_id, out);
        }
    }
    match production {
        Production::Symbol { .. } => emit_vertex(protocol, schema, grammar, child_id, out),
        _ => {
            let edges = children_for(schema, child_id);
            let mut cursor = ChildCursor::new(&edges);
            emit_production(
                protocol,
                schema,
                grammar,
                child_id,
                production,
                &mut cursor,
                out,
            )
        }
    }
}

fn pick_choice_with_cursor<'a>(
    schema: &Schema,
    grammar: &Grammar,
    vertex_id: &panproto_gat::Name,
    cursor: &ChildCursor<'_>,
    alternatives: &'a [Production],
) -> Option<&'a Production> {
    // Constraint-driven dispatch (highest priority): when the
    // alternatives differ by literal STRING tokens (the canonical
    // shape of binary_expression operator alternatives:
    // `CHOICE { SEQ { left, "+", right }, SEQ { left, "&&", right }, ... }`
    // the operator captured by the walker as an interstitial picks
    // out the right alt unambiguously. Without this, the fallback
    // takes the first non-BLANK alt and loses the operator entirely.
    let constraint_blob = schema
        .constraints
        .get(vertex_id)
        .map(|cs| {
            cs.iter()
                .filter(|c| {
                    let s = c.sort.as_ref();
                    s.starts_with("interstitial-") && !s.ends_with("-start-byte")
                })
                .map(|c| c.value.as_str())
                .collect::<Vec<&str>>()
                .join(" ")
        })
        .unwrap_or_default();
    if !constraint_blob.is_empty() {
        let mut best: Option<(usize, &Production)> = None;
        for alt in alternatives {
            let strings = literal_strings(alt);
            if strings.is_empty() {
                continue;
            }
            let score = strings
                .iter()
                .filter(|s| constraint_blob.contains(s.as_str()))
                .map(String::len)
                .sum::<usize>();
            if score > 0 && best.is_none_or(|(s, _)| score > s) {
                best = Some((score, alt));
            }
        }
        if let Some((_, alt)) = best {
            return Some(alt);
        }
    }

    // Cursor-driven dispatch: pick the alternative whose body
    // references at least one SYMBOL whose target kind is present in
    // the unconsumed cursor edges. `referenced_symbols` walks the
    // alternative recursively (across nested SEQs, REPEATs, OPTIONALs,
    // FIELDs, etc.) so a leading optional like `attribute_item` does
    // not block matching when only the trailing required symbol is
    // present on the schema.
    for alt in alternatives {
        let symbols = referenced_symbols(alt);
        if !symbols.is_empty()
            && cursor.has_matching(|edge| {
                let tk = schema.vertices.get(&edge.tgt).map(|v| v.kind.as_ref());
                symbols
                    .iter()
                    .any(|s| kind_satisfies_symbol(grammar, tk, s))
            })
        {
            return Some(alt);
        }
    }

    // FIELD dispatch: pick an alternative whose FIELD name matches an
    // unconsumed edge kind.
    let edge_kinds: Vec<&str> = cursor
        .edges
        .iter()
        .enumerate()
        .filter(|(i, _)| !cursor.consumed[*i])
        .map(|(_, e)| e.kind.as_ref())
        .collect();
    for alt in alternatives {
        if has_field_in(alt, &edge_kinds) {
            return Some(alt);
        }
    }

    // No cursor-driven match. Fall back to:
    //
    // - BLANK (the explicit empty alternative) when present, so an
    //   OPTIONAL-shaped CHOICE compiles to nothing.
    // - The first non-`BLANK` alternative as a last resort, used by
    //   STRING-only alternatives (keyword tokens) and other choices
    //   that don't reach the cursor.
    //
    // The previous "match own_kind" branch is intentionally absent:
    // when an alt's first SYMBOL equals the current vertex's kind, the
    // caller is already emitting that vertex's own rule. Recursing
    // into the alt would cause a self-loop in the rule walk.
    let _ = (schema, vertex_id);
    if alternatives.iter().any(|a| matches!(a, Production::Blank)) {
        return alternatives.iter().find(|a| matches!(a, Production::Blank));
    }
    alternatives
        .iter()
        .find(|alt| !matches!(alt, Production::Blank))
}

/// Collect every literal STRING token directly inside `production`
/// (without descending into SYMBOLs / hidden rules). Used to score
/// CHOICE alternatives against the parent vertex's interstitials so
/// the right operator / keyword form is picked when the schema
/// preserves interstitial fragments from a prior parse.
fn literal_strings(production: &Production) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(p: &Production, out: &mut Vec<String>) {
        match p {
            Production::String { value } if !value.is_empty() => {
                out.push(value.clone());
            }
            Production::Choice { members } | Production::Seq { members } => {
                for m in members {
                    walk(m, out);
                }
            }
            Production::Repeat { content }
            | Production::Repeat1 { content }
            | Production::Optional { content }
            | Production::Field { content, .. }
            | Production::Alias { content, .. }
            | Production::Token { content }
            | Production::ImmediateToken { content }
            | Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. } => walk(content, out),
            _ => {}
        }
    }
    walk(production, &mut out);
    out
}

/// Collect every SYMBOL name reachable from `production` without
/// crossing into nested rules. Used by `pick_choice_with_cursor` to
/// rank alternatives by "any SYMBOL inside this alt matches something
/// on the cursor", instead of just the first SYMBOL: a leading
/// optional like `attribute_item` then `parameter` is otherwise
/// rejected when only the parameter children are present.
fn referenced_symbols(production: &Production) -> Vec<&str> {
    let mut out = Vec::new();
    fn walk<'a>(p: &'a Production, out: &mut Vec<&'a str>) {
        match p {
            Production::Symbol { name } => out.push(name.as_str()),
            Production::Choice { members } | Production::Seq { members } => {
                for m in members {
                    walk(m, out);
                }
            }
            Production::Repeat { content }
            | Production::Repeat1 { content }
            | Production::Optional { content }
            | Production::Field { content, .. }
            | Production::Alias { content, .. }
            | Production::Token { content }
            | Production::ImmediateToken { content }
            | Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. } => walk(content, out),
            _ => {}
        }
    }
    walk(production, &mut out);
    out
}

fn first_symbol(production: &Production) -> Option<&str> {
    match production {
        Production::Symbol { name } => Some(name),
        Production::Seq { members } => members.iter().find_map(first_symbol),
        Production::Choice { members } => members.iter().find_map(first_symbol),
        Production::Repeat { content }
        | Production::Repeat1 { content }
        | Production::Optional { content }
        | Production::Field { content, .. }
        | Production::Alias { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. } => first_symbol(content),
        _ => None,
    }
}

fn has_field_in(production: &Production, edge_kinds: &[&str]) -> bool {
    match production {
        Production::Field { name, .. } => edge_kinds.contains(&name.as_str()),
        Production::Seq { members } | Production::Choice { members } => {
            members.iter().any(|m| has_field_in(m, edge_kinds))
        }
        Production::Repeat { content }
        | Production::Repeat1 { content }
        | Production::Optional { content }
        | Production::Alias { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. } => has_field_in(content, edge_kinds),
        _ => false,
    }
}

fn has_relevant_constraint(
    production: &Production,
    schema: &Schema,
    vertex_id: &panproto_gat::Name,
) -> bool {
    let constraints = match schema.constraints.get(vertex_id) {
        Some(c) => c,
        None => return false,
    };
    fn walk(production: &Production, constraints: &[panproto_schema::Constraint]) -> bool {
        match production {
            Production::String { value } => constraints
                .iter()
                .any(|c| c.value == *value || c.sort.as_ref() == value),
            Production::Field { name, content } => {
                constraints.iter().any(|c| c.sort.as_ref() == name) || walk(content, constraints)
            }
            Production::Seq { members } | Production::Choice { members } => {
                members.iter().any(|m| walk(m, constraints))
            }
            Production::Repeat { content }
            | Production::Repeat1 { content }
            | Production::Optional { content }
            | Production::Alias { content, .. }
            | Production::Token { content }
            | Production::ImmediateToken { content }
            | Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. } => walk(content, constraints),
            _ => false,
        }
    }
    walk(production, constraints)
}

fn children_for<'a>(schema: &'a Schema, vertex_id: &panproto_gat::Name) -> Vec<&'a Edge> {
    let mut edges: Vec<&Edge> = schema
        .edges
        .keys()
        .filter(|e| &e.src == vertex_id)
        .collect();
    edges.sort_by_key(|e| {
        // Edges with an explicit ordering position come first; remaining
        // edges sort lexicographically by (kind, target id) for
        // deterministic walks.
        let pos = schema.orderings.get(*e).copied().unwrap_or(u32::MAX);
        (pos, e.kind.clone(), e.tgt.clone())
    });
    edges
}

fn vertex_id_kind<'a>(schema: &'a Schema, vertex_id: &panproto_gat::Name) -> Option<&'a str> {
    schema.vertices.get(vertex_id).map(|v| v.kind.as_ref())
}

fn literal_value<'a>(schema: &'a Schema, vertex_id: &panproto_gat::Name) -> Option<&'a str> {
    schema
        .constraints
        .get(vertex_id)?
        .iter()
        .find(|c| c.sort.as_ref() == "literal-value")
        .map(|c| c.value.as_str())
}

fn placeholder_for_pattern(pattern: &str) -> String {
    // Heuristic placeholder for unconstrained PATTERN terminals.
    //
    // First handle the "the regex IS a literal escape" cases that
    // tree-sitter grammars use as separators (`\n`, `\r\n`, `;`,
    // etc.); emitting the matching character is always preferable
    // to a `_x` identifier-like placeholder when the surrounding
    // grammar expects a separator.
    let simple_lit = decode_simple_pattern_literal(pattern);
    if let Some(lit) = simple_lit {
        return lit;
    }

    if pattern.contains("[0-9]") || pattern.contains("\\d") {
        "0".into()
    } else if pattern.contains("[a-zA-Z_]") || pattern.contains("\\w") {
        "_x".into()
    } else if pattern.contains('"') || pattern.contains('\'') {
        "\"\"".into()
    } else {
        "_".into()
    }
}

/// Decode a tree-sitter PATTERN whose regex is a simple literal
/// (newline, semicolon, comma, etc.) to the byte sequence it matches.
/// Returns `None` for patterns with character classes, alternations,
/// or quantifiers; the caller falls back to the heuristic placeholder.
fn decode_simple_pattern_literal(pattern: &str) -> Option<String> {
    // Skip patterns containing regex metachars that would broaden the
    // match beyond a single literal byte sequence.
    if pattern
        .chars()
        .any(|c| matches!(c, '[' | ']' | '(' | ')' | '*' | '+' | '?' | '|' | '{' | '}'))
    {
        return None;
    }
    let mut out = String::new();
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some('/') => out.push('/'),
                Some(other) => out.push(other),
                None => return None,
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

// ═══════════════════════════════════════════════════════════════════
// Output buffer with whitespace policy
// ═══════════════════════════════════════════════════════════════════

struct Output<'a> {
    bytes: Vec<u8>,
    last_token: Option<String>,
    indent: usize,
    at_line_start: bool,
    policy: &'a FormatPolicy,
}

#[derive(Clone)]
struct OutputSnapshot {
    bytes_len: usize,
    last_token: Option<String>,
    indent: usize,
    at_line_start: bool,
}

impl<'a> Output<'a> {
    fn new(policy: &'a FormatPolicy) -> Self {
        Self {
            bytes: Vec::new(),
            last_token: None,
            indent: 0,
            at_line_start: true,
            policy,
        }
    }

    fn token(&mut self, value: &str) {
        if value.is_empty() {
            return;
        }

        let close_first = self.policy.indent_close.iter().any(|t| t == value);
        if close_first && self.indent > 0 {
            self.indent -= 1;
            if !self.at_line_start {
                self.newline();
            }
        }

        if self.at_line_start {
            self.write_indent();
        } else if self.needs_space_before(value) {
            self.bytes.push(b' ');
        }

        self.bytes.extend_from_slice(value.as_bytes());
        self.at_line_start = false;
        self.last_token = Some(value.to_owned());

        if self.policy.indent_open.iter().any(|t| t == value) {
            self.indent += 1;
            self.newline();
        } else if self.policy.line_break_after.iter().any(|t| t == value) {
            self.newline();
        }
    }

    fn newline(&mut self) {
        if !self.at_line_start {
            self.bytes.push(b'\n');
            self.at_line_start = true;
        }
    }

    fn write_indent(&mut self) {
        for _ in 0..(self.indent * self.policy.indent_width) {
            self.bytes.push(b' ');
        }
    }

    fn needs_space_before(&self, next: &str) -> bool {
        let last = match &self.last_token {
            Some(t) => t.as_str(),
            None => return false,
        };
        if last.is_empty() || next.is_empty() {
            return false;
        }
        if is_punct_open(last) || is_punct_open(next) {
            // No space inside `(`, `[`, `{` and no space before `(`/`[`
            // when calling/indexing.
            return false;
        }
        if is_punct_close(next) {
            return false;
        }
        if is_punct_close(last) && is_punct_punctuation(next) {
            return false;
        }
        if last == "." || next == "." {
            return false;
        }
        if last_is_word_like(last) && first_is_word_like(next) {
            return true;
        }
        if last_ends_with_alnum(last) && first_is_alnum_or_underscore(next) {
            return true;
        }
        // Adjacent operator runs: keep them apart so the lexer doesn't
        // glue `>` and `=` into `>=` unintentionally.
        true
    }

    fn snapshot(&self) -> OutputSnapshot {
        OutputSnapshot {
            bytes_len: self.bytes.len(),
            last_token: self.last_token.clone(),
            indent: self.indent,
            at_line_start: self.at_line_start,
        }
    }

    fn restore(&mut self, snap: OutputSnapshot) {
        self.bytes.truncate(snap.bytes_len);
        self.last_token = snap.last_token;
        self.indent = snap.indent;
        self.at_line_start = snap.at_line_start;
    }

    fn finish(mut self) -> Vec<u8> {
        if !self.at_line_start {
            self.bytes.push(b'\n');
        }
        self.bytes
    }
}

fn is_punct_open(s: &str) -> bool {
    matches!(s, "(" | "[" | "{" | "\"" | "'" | "`")
}

fn is_punct_close(s: &str) -> bool {
    matches!(s, ")" | "]" | "}" | "," | ";" | ":" | "\"" | "'" | "`")
}

fn is_punct_punctuation(s: &str) -> bool {
    matches!(s, "," | ";" | ":" | "." | ")" | "]" | "}")
}

fn last_is_word_like(s: &str) -> bool {
    s.chars()
        .next_back()
        .map(|c| c.is_alphanumeric() || c == '_')
        .unwrap_or(false)
}

fn first_is_word_like(s: &str) -> bool {
    s.chars()
        .next()
        .map(|c| c.is_alphanumeric() || c == '_')
        .unwrap_or(false)
}

fn last_ends_with_alnum(s: &str) -> bool {
    s.chars()
        .next_back()
        .map(char::is_alphanumeric)
        .unwrap_or(false)
}

fn first_is_alnum_or_underscore(s: &str) -> bool {
    s.chars()
        .next()
        .map(|c| c.is_alphanumeric() || c == '_')
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_grammar_json() {
        let bytes = br#"{
            "name": "tiny",
            "rules": {
                "program": {
                    "type": "SEQ",
                    "members": [
                        {"type": "STRING", "value": "hello"},
                        {"type": "STRING", "value": ";"}
                    ]
                }
            }
        }"#;
        let g = Grammar::from_bytes("tiny", bytes).expect("valid tiny grammar");
        assert!(g.rules.contains_key("program"));
    }

    #[test]
    fn output_emits_punctuation_without_leading_space() {
        let policy = FormatPolicy::default();
        let mut out = Output::new(&policy);
        out.token("foo");
        out.token("(");
        out.token(")");
        out.token(";");
        let bytes = out.finish();
        let s = std::str::from_utf8(&bytes).expect("ascii output");
        assert!(s.starts_with("foo();"), "got {s:?}");
    }

    #[test]
    fn grammar_from_bytes_rejects_malformed_input() {
        let result = Grammar::from_bytes("malformed", b"not json");
        let err = result.expect_err("malformed bytes must yield Err");
        let msg = err.to_string();
        assert!(
            msg.contains("malformed"),
            "error message should name the protocol: {msg:?}"
        );
    }

    #[test]
    fn output_indents_after_open_brace() {
        let policy = FormatPolicy::default();
        let mut out = Output::new(&policy);
        out.token("fn");
        out.token("foo");
        out.token("(");
        out.token(")");
        out.token("{");
        out.token("body");
        out.token("}");
        let bytes = out.finish();
        let s = std::str::from_utf8(&bytes).expect("ascii output");
        assert!(s.contains("{\n"), "newline after opening brace: {s:?}");
        assert!(s.contains("body"), "body inside block: {s:?}");
        assert!(s.ends_with("}\n"), "newline after closing brace: {s:?}");
    }

    #[test]
    fn output_no_space_between_word_and_dot() {
        let policy = FormatPolicy::default();
        let mut out = Output::new(&policy);
        out.token("foo");
        out.token(".");
        out.token("bar");
        let bytes = out.finish();
        let s = std::str::from_utf8(&bytes).expect("ascii output");
        assert!(s.starts_with("foo.bar"), "no space around dot: {s:?}");
    }

    #[test]
    fn output_snapshot_restore_truncates_bytes() {
        let policy = FormatPolicy::default();
        let mut out = Output::new(&policy);
        out.token("keep");
        let snap = out.snapshot();
        out.token("drop");
        out.token("more");
        out.restore(snap);
        out.token("after");
        let bytes = out.finish();
        let s = std::str::from_utf8(&bytes).expect("ascii output");
        assert!(s.contains("keep"), "kept token survives: {s:?}");
        assert!(s.contains("after"), "post-restore token visible: {s:?}");
        assert!(!s.contains("drop"), "rolled-back token removed: {s:?}");
        assert!(!s.contains("more"), "rolled-back token removed: {s:?}");
    }

    #[test]
    fn child_cursor_take_field_consumes_once() {
        let edges_owned: Vec<Edge> = vec![Edge {
            src: panproto_gat::Name::from("p"),
            tgt: panproto_gat::Name::from("c"),
            kind: panproto_gat::Name::from("name"),
            name: None,
        }];
        let edges: Vec<&Edge> = edges_owned.iter().collect();
        let mut cursor = ChildCursor::new(&edges);
        let first = cursor.take_field("name");
        let second = cursor.take_field("name");
        assert!(first.is_some(), "first take returns the edge");
        assert!(
            second.is_none(),
            "second take returns None (already consumed)"
        );
    }

    #[test]
    fn child_cursor_take_matching_predicate() {
        let edges_owned: Vec<Edge> = vec![
            Edge {
                src: "p".into(),
                tgt: "c1".into(),
                kind: "child_of".into(),
                name: None,
            },
            Edge {
                src: "p".into(),
                tgt: "c2".into(),
                kind: "key".into(),
                name: None,
            },
        ];
        let edges: Vec<&Edge> = edges_owned.iter().collect();
        let mut cursor = ChildCursor::new(&edges);
        assert!(cursor.has_matching(|e| e.kind.as_ref() == "key"));
        let taken = cursor.take_matching(|e| e.kind.as_ref() == "key");
        assert!(taken.is_some());
        assert!(
            !cursor.has_matching(|e| e.kind.as_ref() == "key"),
            "consumed edge no longer matches"
        );
        assert!(
            cursor.has_matching(|e| e.kind.as_ref() == "child_of"),
            "the other edge is still available"
        );
    }

    #[test]
    fn kind_satisfies_symbol_direct_match() {
        let bytes = br#"{
            "name": "tiny",
            "rules": {
                "x": {"type": "STRING", "value": "x"}
            }
        }"#;
        let g = Grammar::from_bytes("tiny", bytes).expect("valid grammar");
        assert!(kind_satisfies_symbol(&g, Some("x"), "x"));
        assert!(!kind_satisfies_symbol(&g, Some("y"), "x"));
        assert!(!kind_satisfies_symbol(&g, None, "x"));
    }

    #[test]
    fn kind_satisfies_symbol_through_hidden_rule() {
        let bytes = br#"{
            "name": "tiny",
            "rules": {
                "_value": {
                    "type": "CHOICE",
                    "members": [
                        {"type": "SYMBOL", "name": "object"},
                        {"type": "SYMBOL", "name": "number"}
                    ]
                },
                "object": {"type": "STRING", "value": "{}"},
                "number": {"type": "PATTERN", "value": "[0-9]+"}
            }
        }"#;
        let g = Grammar::from_bytes("tiny", bytes).expect("valid grammar");
        assert!(
            kind_satisfies_symbol(&g, Some("number"), "_value"),
            "number is reachable from _value via CHOICE"
        );
        assert!(
            kind_satisfies_symbol(&g, Some("object"), "_value"),
            "object is reachable from _value via CHOICE"
        );
        assert!(
            !kind_satisfies_symbol(&g, Some("string"), "_value"),
            "string is NOT among the alternatives"
        );
    }

    #[test]
    fn first_symbol_skips_string_terminals() {
        let prod: Production = serde_json::from_str(
            r#"{
                "type": "SEQ",
                "members": [
                    {"type": "STRING", "value": "{"},
                    {"type": "SYMBOL", "name": "body"},
                    {"type": "STRING", "value": "}"}
                ]
            }"#,
        )
        .expect("valid SEQ");
        assert_eq!(first_symbol(&prod), Some("body"));
    }

    #[test]
    fn placeholder_for_pattern_routes_by_regex_class() {
        assert_eq!(placeholder_for_pattern("[0-9]+"), "0");
        assert_eq!(placeholder_for_pattern("[a-zA-Z_]\\w*"), "_x");
        assert_eq!(placeholder_for_pattern("\"[^\"]*\""), "\"\"");
        assert_eq!(placeholder_for_pattern("\\d+\\.\\d+"), "0");
    }

    #[test]
    fn format_policy_default_breaks_after_semicolon() {
        let policy = FormatPolicy::default();
        assert!(policy.line_break_after.iter().any(|t| t == ";"));
        assert!(policy.indent_open.iter().any(|t| t == "{"));
        assert!(policy.indent_close.iter().any(|t| t == "}"));
        assert_eq!(policy.indent_width, 2);
    }
}
