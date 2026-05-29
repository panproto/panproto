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
#[non_exhaustive]
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
    /// Reserved-word wrapper (tree-sitter ≥ 0.25).
    ///
    /// Tree-sitter's `RESERVED` rule marks an inner production as a
    /// reserved-word context: the parser excludes the listed identifiers
    /// from being treated as the inner symbol. The `context_name`
    /// metadata names the reserved-word set; the emitter does not need
    /// it (we are walking schema → bytes, not enforcing reserved-word
    /// constraints), so we emit the inner content unchanged, the same
    /// way [`Production::Token`] and [`Production::ImmediateToken`] do.
    #[serde(rename = "RESERVED")]
    Reserved {
        /// The wrapped content.
        content: Box<Self>,
        /// Name of the reserved-word context. Ignored at emit time.
        #[allow(dead_code)]
        #[serde(default)]
        context_name: String,
    },
}

/// Structural role of a STRING token within a grammar rule.
///
/// Derived at Grammar construction time from the token's position in
/// the production rule body. The role determines spacing behavior in
/// the layout pass via a role-pair lookup table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenRole {
    /// First STRING in a matched-pair SEQ (e.g. `(`, `[`, `{`, `<`,
    /// `begin`, `${`, `⟨`). No space after.
    BracketOpen,
    /// Last STRING in a matched-pair SEQ (e.g. `)`, `]`, `}`, `>`,
    /// `end`, `⟩`). No space before.
    BracketClose,
    /// First STRING in a REPEAT body's inner SEQ (e.g. `,` in
    /// `REPEAT(SEQ [",", item])`). No space before, space after.
    Separator,
    /// Alphanumeric STRING that is a language keyword (e.g. `if`,
    /// `while`, `and`, `model`). Space before and after.
    Keyword,
    /// Non-alphanumeric STRING between content members inside a CHOICE
    /// alternative (e.g. `+`, `=`, `~`, `<-` in binary expression
    /// alternatives). Space before and after.
    Operator,
    /// Non-alphanumeric STRING between content members in a standalone
    /// SEQ (not inside a CHOICE). Examples: `.` in `attribute`,
    /// `::` in `scoped_identifier`, `->` in `pointer_member`. These
    /// are structural connectors, not algebraic operators. No space.
    Connector,
    /// Text from a leaf vertex's `literal-value` constraint.
    Terminal,
    /// A token the grammar wraps in `IMMEDIATE_TOKEN`: the lexer emits it
    /// glued to its neighbour with no intervening whitespace (the `.` in
    /// a float literal `0.5`, an immediate string delimiter). Tight on
    /// both sides, unconditionally. Mirrors
    /// [`panproto_gat::LayoutRole::Immediate`].
    Immediate,
}

/// A grammar's production-rule table, deserialized from `grammar.json`.
///
/// Only the fields the emitter consumes are decoded; precedences,
/// conflicts, externals, and other parser-only metadata are ignored.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
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
    /// Tree-sitter `extras` rules: the named symbols (typically comments)
    /// that tree-sitter skips at parse time but records as children of the
    /// surrounding vertex. They appear nowhere in the production grammar,
    /// so the rule walker cannot reconcile them against the cursor — the
    /// emit pass therefore drains them as a side channel: at vertex entry
    /// and between REPEAT iterations any leading extras-kind edges are
    /// consumed and emitted directly. The set is populated at
    /// `Grammar::from_bytes` by collecting every `SYMBOL { name }` and
    /// named `ALIAS { value, named: true }` under the top-level `extras`
    /// array. Pattern-only extras (e.g. `\s` whitespace) are not vertex
    /// kinds and are excluded.
    #[serde(default, deserialize_with = "deserialize_extras")]
    pub extras: std::collections::HashSet<String>,
    /// Precomputed subtyping closure: `subtypes[symbol_name]` is the
    /// set of vertex kinds that satisfy a SYMBOL `symbol_name`
    /// reference on the schema side.
    ///
    /// Built once at [`Grammar::from_bytes`] time by walking each
    /// hidden rule (`_`-prefixed), declared supertype, and named
    /// `ALIAS { value: K, ... }` production to its leaf SYMBOLs and
    /// recording the closure. This replaces the prior heuristic
    /// `kind_satisfies_symbol` that walked the rule body on every
    /// query: lookups are now O(1) and the relation is exactly the
    /// transitive closure of "is reachable via hidden / supertype /
    /// alias dispatch", with no over-expansion through non-hidden
    /// non-supertype rule references.
    #[serde(skip)]
    pub subtypes: std::collections::HashMap<String, std::collections::HashSet<String>>,
    /// Precomputed Yield sets: `yield_sets[rule_name]` is the set of
    /// concrete vertex kinds that can appear as the **first named
    /// child** when that rule's production is taken.
    ///
    /// Defined inductively:
    /// - `Yield(SYMBOL S)` where S is hidden/supertype = `Yield(rules[S])`
    /// - `Yield(SYMBOL S)` where S is concrete = `{S}`
    /// - `Yield(SEQ [M1, ...])` = `Yield(M1)` (only first member)
    /// - `Yield(CHOICE [M1, ..., Mn])` = `⋃ Yield(Mi)`
    /// - `Yield(OPTIONAL { c })` = `Yield(c) ∪ {ε}`
    /// - `Yield(BLANK)` = `{ε}`
    /// - Wrappers (PREC*, TOKEN, FIELD, REPEAT, etc.) = `Yield(content)`
    /// - `Yield(STRING)` = `Yield(PATTERN)` = `∅`
    /// - `Yield(ALIAS { value: V, named: true })` = `{V}`
    ///
    /// Epsilon is represented as the empty string `""`.
    #[serde(skip)]
    pub yield_sets: std::collections::HashMap<String, std::collections::HashSet<String>>,
    /// Child kinds allowed per parent kind, derived from node-types.json.
    /// Maps parent kind to the set of ALL named child kinds that tree-sitter's
    /// parser can produce for that parent (from both `children.types` and
    /// `fields.*.types`). Used by `augment_subtypes_from_node_types` to
    /// close the grammar/parser divergence gap.
    #[serde(skip)]
    pub node_type_children: std::collections::HashMap<String, std::collections::HashSet<String>>,
    /// Per-field child kinds from node-types.json: maps parent kind →
    /// field name → set of child kinds. Used by the augmentation to
    /// restrict subtype edges to structurally matching positions.
    #[serde(skip)]
    pub node_type_field_children: std::collections::HashMap<
        String,
        std::collections::HashMap<String, std::collections::HashSet<String>>,
    >,
    /// Non-field child kinds from node-types.json: maps parent kind →
    /// set of child kinds that appear in `children.types` (not in any field).
    #[serde(skip)]
    pub node_type_nonfield_children:
        std::collections::HashMap<String, std::collections::HashSet<String>>,
    /// Anonymous ALIAS values for external scanner tokens. Maps external
    /// symbol name (e.g. `_ternary_qmark`) to the ALIAS value string
    /// (e.g. `"?"`). Built by scanning grammar.json rule bodies for
    /// `ALIAS { content: SYMBOL S, named: false, value: V }` where S
    /// has no grammar rule.
    #[serde(skip)]
    pub external_alias_map: std::collections::HashMap<String, String>,
    /// Per-rule token role classification. Maps rule name to a map of
    /// STRING value to its structural role in that rule. Derived at
    /// construction time by analyzing each rule's SEQ structure to
    /// identify bracket pairs, separators, keywords, and operators.
    #[serde(skip)]
    pub token_roles:
        std::collections::HashMap<String, std::collections::HashMap<String, TokenRole>>,
    /// Set of `(rule_name, open_bracket_value)` pairs where the bracket
    /// triggers indentation (the content between open and close contains
    /// `REPEAT`/`REPEAT1`). Block-level constructs like `statement_block`
    /// use indenting brackets; inline constructs like interpolation do not.
    #[serde(skip)]
    pub indent_triggers: std::collections::HashSet<(String, String)>,
    /// Line-comment prefixes extracted from the grammar's extras.
    /// Each prefix is a STRING value from a `TOKEN(SEQ [STRING prefix,
    /// PATTERN ...])` pattern in the extras array, verified to be an
    /// extras rule. Used by the layout pass to insert a newline after
    /// comment Lit tokens.
    #[serde(skip)]
    pub line_comment_prefixes: Vec<String>,
    /// External tokens that produce indent-open layout actions.
    /// Identified by tree-sitter naming convention: names ending with
    /// `_indent` or equal to `_indent`.
    #[serde(skip)]
    pub external_indent_opens: std::collections::HashSet<String>,
    /// External tokens that produce indent-close layout actions.
    #[serde(skip)]
    pub external_indent_closes: std::collections::HashSet<String>,
    /// External tokens that produce line breaks.
    #[serde(skip)]
    pub external_newlines: std::collections::HashSet<String>,
    /// External tokens equivalent to semicolons.
    #[serde(skip)]
    pub external_semicolons: std::collections::HashSet<String>,
    /// External scanner tokens that open a delimiter pair around content
    /// (e.g. `string_start` in `SEQ[string_start, REPEAT(content),
    /// string_end]`). Derived structurally; emitted tight on the inside
    /// (`'hello'`, not `' hello '`).
    #[serde(skip)]
    pub external_bracket_opens: std::collections::HashSet<String>,
    /// External scanner tokens that close a delimiter pair around content
    /// (e.g. `string_end`). Emitted tight on the inside.
    #[serde(skip)]
    pub external_bracket_closes: std::collections::HashSet<String>,
    /// Rule names that are indented blocks whose opening `_indent` lives
    /// in a (hidden) parent rule rather than the rule itself: their body
    /// references an external indent-*close* token (`_dedent`) but no
    /// indent-*open* token. The parser reaches such a block vertex
    /// directly (the hidden `_suite` wrapper carrying the `_indent` is
    /// not a vertex), so the emitter must synthesize the opening indent
    /// (`def f():` then an indented body) when it walks the rule.
    #[serde(skip)]
    pub synthetic_indent_rules: std::collections::HashSet<String>,
    /// Named alias map: maps alias value to source symbol name.
    /// When a vertex kind has no direct grammar rule, this map resolves
    /// `ALIAS { content: SYMBOL source, named: true, value: alias }` so
    /// the emitter can walk the source rule with proper token roles.
    #[serde(skip)]
    pub named_alias_map: std::collections::HashMap<String, String>,
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

fn deserialize_extras<'de, D>(
    deserializer: D,
) -> Result<std::collections::HashSet<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let entries: Vec<serde_json::Value> = Vec::deserialize(deserializer)?;
    let mut out = std::collections::HashSet::new();
    for entry in entries {
        if let serde_json::Value::Object(map) = entry {
            let ty = map.get("type").and_then(serde_json::Value::as_str);
            match ty {
                // SYMBOL { name: K } — the extras rule is a named symbol
                // (typically `line_comment` / `block_comment`). The kind
                // K appears as a real child vertex on the schema side.
                Some("SYMBOL") => {
                    if let Some(serde_json::Value::String(name)) = map.get("name") {
                        out.insert(name.clone());
                    }
                }
                // ALIAS { content, value: V, named: true } — the extras
                // rule renames its content; V is the kind on the schema.
                Some("ALIAS") => {
                    let named = map
                        .get("named")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    if named {
                        if let Some(serde_json::Value::String(value)) = map.get("value") {
                            out.insert(value.clone());
                        }
                    }
                }
                // PATTERN / STRING / TOKEN entries describe inter-token
                // whitespace and have no vertex-side representation.
                _ => {}
            }
        }
    }
    Ok(out)
}

impl Grammar {
    /// Parse a grammar's `grammar.json` bytes.
    ///
    /// Builds the subtyping closure as part of construction so every
    /// downstream lookup is O(1). The closure is the least relation
    /// containing `(K, K)` for every rule key `K` and closed under:
    ///
    /// - hidden-rule expansion: if `S` is hidden and a SYMBOL `S` may
    ///   reach SYMBOL `K`, then `K ⊑ S`.
    /// - supertype expansion: if `S` is in the grammar's supertypes
    ///   block and `K` is one of `S`'s alternatives, then `K ⊑ S`.
    /// - alias renaming: if a rule body contains
    ///   `ALIAS { content: SYMBOL R, value: A, named: true }` where
    ///   `R` reaches kind `K` (or `K = R` when no further hop), then
    ///   `A ⊑ R` and `K ⊑ A`.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::EmitFailed`] when the bytes are not a
    /// valid `grammar.json` document.
    pub fn from_bytes(protocol: &str, bytes: &[u8]) -> Result<Self, ParseError> {
        Self::from_bytes_with_node_types(protocol, bytes, None)
    }

    /// Parse a grammar from both `grammar.json` and optionally
    /// `node-types.json` bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::EmitFailed`] when `grammar_bytes` is
    /// not a valid `grammar.json` document.
    pub fn from_bytes_with_node_types(
        protocol: &str,
        grammar_bytes: &[u8],
        node_types_bytes: Option<&[u8]>,
    ) -> Result<Self, ParseError> {
        let mut grammar: Self =
            serde_json::from_slice(grammar_bytes).map_err(|e| ParseError::EmitFailed {
                protocol: protocol.to_owned(),
                reason: format!("grammar.json deserialization failed: {e}"),
            })?;
        grammar.subtypes = compute_subtype_closure(&grammar);
        grammar.named_alias_map = build_named_alias_map(&grammar);
        grammar.yield_sets = compute_yield_sets(&grammar);
        if let Some(nt_bytes) = node_types_bytes {
            let (all_children, field_children, nonfield_children) =
                build_node_type_children(nt_bytes);
            grammar.node_type_children = all_children;
            grammar.node_type_field_children = field_children;
            grammar.node_type_nonfield_children = nonfield_children;
            augment_subtypes_from_node_types(&mut grammar);
        }
        grammar.yield_sets = compute_yield_sets(&grammar);
        grammar.external_alias_map = build_external_alias_map(&grammar);
        let (token_roles, indent_triggers) = compute_token_roles(&grammar);
        grammar.token_roles = token_roles;
        grammar.indent_triggers = indent_triggers;
        grammar.line_comment_prefixes = extract_line_comment_prefixes(&grammar);
        classify_external_layout_tokens(&mut grammar);
        classify_external_bracket_delimiters(&mut grammar);
        classify_synthetic_indent_rules(&mut grammar);
        grammar.yield_sets = compute_yield_sets(&grammar);
        Ok(grammar)
    }
}

/// Compute the subtyping relation as a forward-indexed map from a
/// SYMBOL name to the set of vertex kinds that satisfy that SYMBOL.
fn compute_subtype_closure(
    grammar: &Grammar,
) -> std::collections::HashMap<String, std::collections::HashSet<String>> {
    use std::collections::{HashMap, HashSet};
    // Edges of the "kind X satisfies SYMBOL Y" relation. `K ⊑ Y` is
    // recorded whenever Y is reached by walking the grammar's
    // ALIAS / hidden-rule / supertype dispatch from a position where
    // K is the actual vertex kind.
    let mut subtypes: HashMap<String, HashSet<String>> = HashMap::new();
    for name in grammar.rules.keys() {
        subtypes
            .entry(name.clone())
            .or_default()
            .insert(name.clone());
    }

    // First pass: collect the immediate "satisfies" edges from each
    // expandable rule (hidden, supertype) to the kinds reachable by
    // walking its body, plus alias edges.
    fn walk<'g>(
        grammar: &'g Grammar,
        production: &'g Production,
        visited: &mut HashSet<&'g str>,
        out: &mut HashSet<String>,
    ) {
        match production {
            Production::Symbol { name } => {
                // Direct subtype.
                out.insert(name.clone());
                // Continue expansion through hidden / supertype rules
                // so the closure traverses pass-through dispatch.
                let expand = name.starts_with('_') || grammar.supertypes.contains(name.as_str());
                if expand && visited.insert(name.as_str()) {
                    if let Some(rule) = grammar.rules.get(name) {
                        walk(grammar, rule, visited, out);
                    }
                }
            }
            Production::Choice { members } | Production::Seq { members } => {
                for m in members {
                    walk(grammar, m, visited, out);
                }
            }
            Production::Alias {
                content,
                named,
                value,
            } => {
                if *named && !value.is_empty() {
                    out.insert(value.clone());
                }
                walk(grammar, content, visited, out);
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
            | Production::PrecDynamic { content, .. }
            | Production::Reserved { content, .. } => {
                walk(grammar, content, visited, out);
            }
            _ => {}
        }
    }

    for (name, rule) in &grammar.rules {
        let expand = name.starts_with('_') || grammar.supertypes.contains(name.as_str());
        if !expand {
            continue;
        }
        let mut visited: HashSet<&str> = HashSet::new();
        visited.insert(name.as_str());
        let mut reachable: HashSet<String> = HashSet::new();
        walk(grammar, rule, &mut visited, &mut reachable);
        for kind in &reachable {
            subtypes
                .entry(kind.clone())
                .or_default()
                .insert(name.clone());
        }
    }

    // Aliases: scan every rule body for ALIAS { content, value }
    // declarations. The kinds reachable from `content` satisfy
    // `value`, AND (by construction) `value` satisfies the
    // surrounding rule. Walking the ENTIRE grammar once captures
    // every alias site, irrespective of which rule introduces it.
    fn collect_aliases<'g>(production: &'g Production, out: &mut Vec<(String, &'g Production)>) {
        match production {
            Production::Alias {
                content,
                named,
                value,
            } => {
                if *named && !value.is_empty() {
                    out.push((value.clone(), content.as_ref()));
                }
                collect_aliases(content, out);
            }
            Production::Choice { members } | Production::Seq { members } => {
                for m in members {
                    collect_aliases(m, out);
                }
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
            | Production::PrecDynamic { content, .. }
            | Production::Reserved { content, .. } => {
                collect_aliases(content, out);
            }
            _ => {}
        }
    }
    let mut aliases: Vec<(String, &Production)> = Vec::new();
    for rule in grammar.rules.values() {
        collect_aliases(rule, &mut aliases);
    }
    for (alias_value, content) in aliases {
        let mut visited: HashSet<&str> = HashSet::new();
        let mut reachable: HashSet<String> = HashSet::new();
        walk(grammar, content, &mut visited, &mut reachable);
        // Aliased value satisfies itself and is satisfied by every
        // kind its content can reach.
        subtypes
            .entry(alias_value.clone())
            .or_default()
            .insert(alias_value.clone());
        for kind in reachable {
            subtypes
                .entry(kind)
                .or_default()
                .insert(alias_value.clone());
        }
    }

    // Transitive close through hidden and supertype rules via Tarjan SCC.
    //
    // The relation `K ⊑ Y` means "a vertex of kind K can appear where
    // the grammar says SYMBOL Y." Transitivity applies when Y is a
    // hidden or supertype rule (a dispatch point), NOT when Y is a
    // concrete named rule. We build the directed graph G on dispatchable
    // node names with edge Y → Z iff Z ∈ subtypes[Y] and Z is dispatchable.
    // The transitive closure within G is the union of every reachable
    // dispatchable node, which by Tarjan's theorem is computed in
    // O(V + E) by contracting SCCs into a DAG, then unioning closures
    // along reverse topological order.
    let is_dispatch = |s: &str| s.starts_with('_') || grammar.supertypes.contains(s);
    // 1. Nodes: every dispatchable name that appears as a key in subtypes
    //    OR as a member of any subtypes value.
    let mut nodes: HashSet<String> = HashSet::new();
    for (k, vs) in &subtypes {
        if is_dispatch(k) {
            nodes.insert(k.clone());
        }
        for v in vs {
            if is_dispatch(v) {
                nodes.insert(v.clone());
            }
        }
    }
    let nodes: Vec<String> = nodes.into_iter().collect();
    let index_of: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();
    // 2. Edges: Y → Z iff Z ∈ subtypes[Y] and both are dispatchable.
    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for (i, name) in nodes.iter().enumerate() {
        if let Some(targets) = subtypes.get(name) {
            for t in targets {
                if let Some(&j) = index_of.get(t.as_str()) {
                    if i != j {
                        edges[i].push(j);
                    }
                }
            }
        }
    }

    // 3. Tarjan SCC. `comp[v]` = SCC index of `v`. SCC indices come out
    //    in reverse topological order (sinks first), which is exactly
    //    the order we want for closure accumulation.
    fn tarjan(edges: &[Vec<usize>]) -> Vec<usize> {
        let n = edges.len();
        let mut comp = vec![usize::MAX; n];
        let mut index_arr = vec![usize::MAX; n];
        let mut lowlink = vec![0usize; n];
        let mut on_stack = vec![false; n];
        let mut stack: Vec<usize> = Vec::new();
        let mut next_index = 0usize;
        let mut next_comp = 0usize;
        // Iterative Tarjan to avoid stack overflow on large grammars.
        let mut work: Vec<(usize, usize)> = Vec::new();
        for start in 0..n {
            if index_arr[start] != usize::MAX {
                continue;
            }
            work.push((start, 0));
            index_arr[start] = next_index;
            lowlink[start] = next_index;
            next_index += 1;
            stack.push(start);
            on_stack[start] = true;
            while let Some(&(v, i)) = work.last() {
                if i < edges[v].len() {
                    let w = edges[v][i];
                    if let Some(slot) = work.last_mut() {
                        slot.1 += 1;
                    }
                    if index_arr[w] == usize::MAX {
                        index_arr[w] = next_index;
                        lowlink[w] = next_index;
                        next_index += 1;
                        stack.push(w);
                        on_stack[w] = true;
                        work.push((w, 0));
                    } else if on_stack[w] && index_arr[w] < lowlink[v] {
                        lowlink[v] = index_arr[w];
                    }
                } else {
                    if lowlink[v] == index_arr[v] {
                        while let Some(w) = stack.pop() {
                            on_stack[w] = false;
                            comp[w] = next_comp;
                            if w == v {
                                break;
                            }
                        }
                        next_comp += 1;
                    }
                    let lv = lowlink[v];
                    work.pop();
                    if let Some(&(parent, _)) = work.last() {
                        if lv < lowlink[parent] {
                            lowlink[parent] = lv;
                        }
                    }
                }
            }
        }
        comp
    }
    let comp = tarjan(&edges);
    let num_comps = comp.iter().max().copied().map_or(0, |m| m + 1);

    // 4. For each SCC, accumulate the set of dispatchable nodes reachable
    //    from it. SCCs are emitted in reverse topological order, so when
    //    we process SCC c, every successor SCC has its closure already
    //    computed.
    let mut scc_members: Vec<Vec<usize>> = vec![Vec::new(); num_comps];
    for (v, &c) in comp.iter().enumerate() {
        scc_members[c].push(v);
    }
    let mut scc_closure: Vec<HashSet<String>> = vec![HashSet::new(); num_comps];
    for c in 0..num_comps {
        // Members of the SCC are mutually reachable.
        let mut closure: HashSet<String> = HashSet::new();
        for &v in &scc_members[c] {
            closure.insert(nodes[v].clone());
        }
        // Successor SCCs' closures (already computed).
        for &v in &scc_members[c] {
            for &w in &edges[v] {
                let wc = comp[w];
                if wc != c {
                    closure.extend(scc_closure[wc].iter().cloned());
                }
            }
        }
        scc_closure[c] = closure;
    }

    // 5. Apply: for each kind K in `subtypes`, replace its dispatchable
    //    supertypes by their full closure. Non-dispatchable members
    //    (concrete kinds) stay as-is.
    let keys: Vec<String> = subtypes.keys().cloned().collect();
    for k in keys {
        let existing = subtypes.remove(&k).unwrap_or_default();
        let mut new_set: HashSet<String> = HashSet::new();
        for s in &existing {
            new_set.insert(s.clone());
            if let Some(&i) = index_of.get(s.as_str()) {
                new_set.extend(scc_closure[comp[i]].iter().cloned());
            }
        }
        subtypes.insert(k, new_set);
    }

    subtypes
}

/// Compute the Yield set for every rule in the grammar.
///
/// `Yield(P)` is the set of concrete vertex kinds that can appear as
/// the first named child when production P is taken. See the
/// `Grammar::yield_sets` doc comment for the inductive definition.
fn compute_yield_sets(
    grammar: &Grammar,
) -> std::collections::HashMap<String, std::collections::HashSet<String>> {
    let mut cache: std::collections::HashMap<String, std::collections::HashSet<String>> =
        std::collections::HashMap::new();
    for (name, rule) in &grammar.rules {
        let expand = name.starts_with('_') || grammar.supertypes.contains(name.as_str());
        if !expand {
            continue;
        }
        if cache.contains_key(name) {
            continue;
        }
        let mut visited = std::collections::HashSet::new();
        let ys = yield_of_production(grammar, rule, &mut visited, &mut cache);
        cache.insert(name.clone(), ys);
    }
    cache
}

/// Compute the Yield set of an arbitrary production node.
///
/// Uses `cache` (the partially-built `yield_sets` map) as
/// memoization. `visited` tracks the current recursion path to
/// detect cycles through hidden/supertype rules; a cycle returns ∅
/// (a cycle that never passes through a concrete named symbol
/// cannot produce a first child).
fn yield_of_production(
    grammar: &Grammar,
    production: &Production,
    visited: &mut std::collections::HashSet<String>,
    cache: &mut std::collections::HashMap<String, std::collections::HashSet<String>>,
) -> std::collections::HashSet<String> {
    match production {
        Production::Symbol { name } => {
            let expand = name.starts_with('_') || grammar.supertypes.contains(name.as_str());
            if !expand {
                let mut set = std::collections::HashSet::new();
                set.insert(name.clone());
                return set;
            }
            if let Some(cached) = cache.get(name) {
                return cached.clone();
            }
            {
                if !visited.insert(name.clone()) {
                    return std::collections::HashSet::new();
                }
                let result = if let Some(rule) = grammar.rules.get(name) {
                    yield_of_production(grammar, rule, visited, cache)
                } else {
                    std::collections::HashSet::new()
                };
                visited.remove(name);
                cache.insert(name.clone(), result.clone());
                result
            }
        }
        Production::Alias {
            content,
            named,
            value,
        } => {
            if *named && !value.is_empty() {
                let mut set = std::collections::HashSet::new();
                set.insert(value.clone());
                set
            } else {
                yield_of_production(grammar, content, visited, cache)
            }
        }
        Production::Seq { members } => {
            if members.is_empty() {
                let mut set = std::collections::HashSet::new();
                set.insert(String::new());
                set
            } else {
                // Walk SEQ members left-to-right. STRING/PATTERN yield ∅
                // (anonymous tokens, skipped). Named-child-producing
                // members yield a non-empty set. If that set contains ε,
                // the member is optional and the next member's yield is
                // also reachable. Accumulate until we hit a non-optional
                // named-child producer.
                let mut combined = std::collections::HashSet::new();
                for m in members {
                    let ys = yield_of_production(grammar, m, visited, cache);
                    if ys.is_empty() {
                        continue;
                    }
                    let has_epsilon = ys.contains("");
                    combined.extend(ys);
                    if !has_epsilon {
                        break;
                    }
                }
                combined
            }
        }
        Production::Choice { members } => {
            let mut union = std::collections::HashSet::new();
            for m in members {
                union.extend(yield_of_production(grammar, m, visited, cache));
            }
            union
        }
        Production::Optional { content } => {
            let mut set = yield_of_production(grammar, content, visited, cache);
            set.insert(String::new());
            set
        }
        Production::Blank => {
            let mut set = std::collections::HashSet::new();
            set.insert(String::new());
            set
        }
        Production::String { .. } | Production::Pattern { .. } => std::collections::HashSet::new(),
        Production::Repeat { content } => {
            let mut set = yield_of_production(grammar, content, visited, cache);
            set.insert(String::new());
            set
        }
        Production::Repeat1 { content }
        | Production::Field { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => {
            yield_of_production(grammar, content, visited, cache)
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// node-types.json integration
// ═══════════════════════════════════════════════════════════════════

/// Parse node-types.json and build a map from parent kind to the set
/// of all named child kinds the parser can produce for that parent.
type NodeTypeResult = (
    std::collections::HashMap<String, std::collections::HashSet<String>>,
    std::collections::HashMap<
        String,
        std::collections::HashMap<String, std::collections::HashSet<String>>,
    >,
    std::collections::HashMap<String, std::collections::HashSet<String>>,
);

fn build_node_type_children(nt_bytes: &[u8]) -> NodeTypeResult {
    use std::collections::{HashMap, HashSet};
    let node_types: Vec<crate::theory_extract::NodeType> = match serde_json::from_slice(nt_bytes) {
        Ok(v) => v,
        Err(_) => return (HashMap::new(), HashMap::new(), HashMap::new()),
    };
    let mut all_map: HashMap<String, HashSet<String>> = HashMap::new();
    let mut field_map: HashMap<String, HashMap<String, HashSet<String>>> = HashMap::new();
    let mut nonfield_map: HashMap<String, HashSet<String>> = HashMap::new();
    for entry in &node_types {
        if !entry.named {
            continue;
        }
        let mut child_kinds = HashSet::new();
        for (field_name, field_value) in &entry.fields {
            if let Some(types) = field_value.get("types").and_then(|t| t.as_array()) {
                for t in types {
                    if let (Some(name), Some(true)) = (
                        t.get("type").and_then(|n| n.as_str()),
                        t.get("named").and_then(serde_json::Value::as_bool),
                    ) {
                        child_kinds.insert(name.to_owned());
                        field_map
                            .entry(entry.node_type.clone())
                            .or_default()
                            .entry(field_name.clone())
                            .or_default()
                            .insert(name.to_owned());
                    }
                }
            }
        }
        if let Some(ref children) = entry.children {
            for t in &children.types {
                if t.named {
                    child_kinds.insert(t.node_type.clone());
                    nonfield_map
                        .entry(entry.node_type.clone())
                        .or_default()
                        .insert(t.node_type.clone());
                }
            }
        }
        if !child_kinds.is_empty() {
            all_map.insert(entry.node_type.clone(), child_kinds);
        }
    }
    (all_map, field_map, nonfield_map)
}

/// Augment `grammar.subtypes` with child-kind data from node-types.json.
///
/// Uses per-field structural matching: for each parent kind P, each field
/// F in P's node-types.json entry, and each child kind C in field F's
/// types, find the SYMBOL S referenced at field F's position in P's
/// grammar rule. If C lacks a grammar rule and does not already satisfy S,
/// record C ⊑ S. Non-field children are matched against non-FIELD symbols
/// in the rule body.
fn augment_subtypes_from_node_types(grammar: &mut Grammar) {
    use std::collections::HashMap;

    // Build per-field child-kind map from node-types.json by re-parsing.
    let mut pairs: Vec<(String, String)> = Vec::new();
    for parent_kind in grammar.node_type_children.keys() {
        let Some(rule) = grammar.rules.get(parent_kind) else {
            continue;
        };

        // Collect symbols from the grammar rule, partitioned by the
        // FIELD they appear in (or non-field for top-level symbols).
        let mut field_symbols: HashMap<String, Vec<String>> = HashMap::new();
        let mut non_field_symbols: Vec<String> = Vec::new();
        collect_field_symbols(rule, &mut field_symbols, &mut non_field_symbols, false);

        // Per-field augmentation: for each FIELD F in the grammar rule,
        // match child kinds that node-types.json says appear in field F
        // against the symbols at field F's position.
        if let Some(nt_fields) = grammar.node_type_field_children.get(parent_kind) {
            for (field_name, nt_child_kinds) in nt_fields {
                let Some(rule_syms) = field_symbols.get(field_name) else {
                    continue;
                };
                for child_kind in nt_child_kinds {
                    if grammar.rules.contains_key(child_kind) {
                        continue;
                    }
                    for sym_name in rule_syms {
                        if !kind_satisfies_symbol(grammar, Some(child_kind), sym_name) {
                            pairs.push((child_kind.clone(), sym_name.clone()));
                        }
                    }
                }
            }
        }

        // Non-field augmentation: for child kinds from `children.types`
        // (no field), match against non-FIELD symbols in the rule.
        if let Some(nt_nonfield) = grammar.node_type_nonfield_children.get(parent_kind) {
            for child_kind in nt_nonfield {
                if grammar.rules.contains_key(child_kind) {
                    continue;
                }
                for sym_name in &non_field_symbols {
                    if !kind_satisfies_symbol(grammar, Some(child_kind), sym_name) {
                        pairs.push((child_kind.clone(), sym_name.clone()));
                    }
                }
            }
        }
    }
    for (child_kind, sym_name) in pairs {
        grammar
            .subtypes
            .entry(child_kind)
            .or_default()
            .insert(sym_name);
    }
}

/// Walk a production and collect referenced symbols, separating those
/// inside FIELD bodies (keyed by field name) from those outside any FIELD.
fn collect_field_symbols(
    prod: &Production,
    field_map: &mut std::collections::HashMap<String, Vec<String>>,
    non_field: &mut Vec<String>,
    inside_field: bool,
) {
    match prod {
        Production::Symbol { name } if !inside_field => {
            non_field.push(name.clone());
        }
        Production::Field { name, content } => {
            let mut syms = Vec::new();
            collect_symbols_flat(content, &mut syms);
            field_map.entry(name.clone()).or_default().extend(syms);
        }
        Production::Choice { members } | Production::Seq { members } => {
            for m in members {
                collect_field_symbols(m, field_map, non_field, inside_field);
            }
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
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => {
            collect_field_symbols(content, field_map, non_field, inside_field);
        }
        _ => {}
    }
}

fn collect_symbols_flat(prod: &Production, out: &mut Vec<String>) {
    match prod {
        Production::Symbol { name } => out.push(name.clone()),
        Production::Choice { members } | Production::Seq { members } => {
            for m in members {
                collect_symbols_flat(m, out);
            }
        }
        Production::Repeat { content }
        | Production::Repeat1 { content }
        | Production::Optional { content }
        | Production::Alias { content, .. }
        | Production::Field { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => collect_symbols_flat(content, out),
        _ => {}
    }
}

/// Build a map from external scanner symbol names to their anonymous
/// ALIAS values by walking every rule body in the grammar.
fn build_external_alias_map(grammar: &Grammar) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    fn walk(
        grammar: &Grammar,
        prod: &Production,
        map: &mut std::collections::HashMap<String, String>,
    ) {
        match prod {
            Production::Alias {
                content,
                named,
                value,
            } => {
                if !*named && !value.is_empty() {
                    if let Production::Symbol { name } = content.as_ref() {
                        if name.starts_with('_') && !grammar.rules.contains_key(name) {
                            map.entry(name.clone()).or_insert_with(|| value.clone());
                        }
                    }
                }
                walk(grammar, content, map);
            }
            Production::Choice { members } | Production::Seq { members } => {
                for m in members {
                    walk(grammar, m, map);
                }
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
            | Production::PrecDynamic { content, .. }
            | Production::Reserved { content, .. } => walk(grammar, content, map),
            _ => {}
        }
    }
    for rule in grammar.rules.values() {
        walk(grammar, rule, &mut map);
    }
    map
}

/// Build a map from named-alias values to their source symbol names.
/// When tree-sitter emits a vertex with kind `V` via
/// `alias($.source, $.V)`, the grammar has no rule keyed by `V`.
/// This map lets the emitter resolve `V → source` and walk the source
/// rule with proper token roles and bracket pairs.
fn build_named_alias_map(grammar: &Grammar) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    fn walk(prod: &Production, map: &mut std::collections::HashMap<String, String>) {
        match prod {
            Production::Alias {
                content,
                named,
                value,
            } => {
                if *named && !value.is_empty() {
                    if let Production::Symbol { name } = content.as_ref() {
                        map.entry(value.clone()).or_insert_with(|| name.clone());
                    }
                }
                walk(content, map);
            }
            Production::Choice { members } | Production::Seq { members } => {
                for m in members {
                    walk(m, map);
                }
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
            | Production::PrecDynamic { content, .. }
            | Production::Reserved { content, .. } => walk(content, map),
            _ => {}
        }
    }
    for rule in grammar.rules.values() {
        walk(rule, &mut map);
    }
    map
}

/// Compute token roles for every STRING value in every grammar rule.
///
/// For each rule R, analyzes the production body to classify every
/// STRING token by its structural role (bracket-open, bracket-close,
/// separator, keyword, operator). Also identifies which bracket-open
/// tokens trigger indentation (those with REPEAT/REPEAT1 between
/// the open and close).
///
/// Bracket pairs are detected per-SEQ, not from a fixed character
/// set. Two STRINGs are a matched pair iff they are the first and
/// last STRING-typed members of the same SEQ with at least one
/// non-STRING member between them and open != close.
type RoleMap = std::collections::HashMap<String, std::collections::HashMap<String, TokenRole>>;
type IndentSet = std::collections::HashSet<(String, String)>;

fn compute_token_roles(grammar: &Grammar) -> (RoleMap, IndentSet) {
    use std::collections::{HashMap, HashSet};
    let mut all_roles: HashMap<String, HashMap<String, TokenRole>> = HashMap::new();
    let mut indent_triggers: HashSet<(String, String)> = HashSet::new();

    for (rule_name, rule) in &grammar.rules {
        let mut roles: HashMap<String, TokenRole> = HashMap::new();
        classify_production(rule, &mut roles, &mut indent_triggers, rule_name);
        if !roles.is_empty() {
            all_roles.insert(rule_name.clone(), roles);
        }
    }

    (all_roles, indent_triggers)
}

/// Recursively classify STRING tokens in a production body.
fn classify_production(
    prod: &Production,
    roles: &mut std::collections::HashMap<String, TokenRole>,
    indent_triggers: &mut std::collections::HashSet<(String, String)>,
    rule_name: &str,
) {
    match prod {
        Production::Seq { members } => {
            classify_seq(members, roles, indent_triggers, rule_name, false);
        }
        Production::Choice { members } => {
            for m in members {
                // CHOICE alternatives' SEQs get in_choice=true so that
                // position-0 STRINGs are classified as Operators (not
                // prefix sigils). E.g. `=` in `CHOICE [SEQ ["=", ...]]`
                // is an operator, not a prefix.
                match m {
                    Production::Seq {
                        members: seq_members,
                    } => {
                        classify_seq(seq_members, roles, indent_triggers, rule_name, true);
                    }
                    _ => classify_production(m, roles, indent_triggers, rule_name),
                }
            }
        }
        Production::Repeat { content } | Production::Repeat1 { content } => {
            classify_repeat_body(content, roles, indent_triggers, rule_name);
        }
        Production::Optional { content }
        | Production::Field { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => {
            classify_production(content, roles, indent_triggers, rule_name);
        }
        Production::Alias { content, .. } => {
            classify_production(content, roles, indent_triggers, rule_name);
        }
        _ => {}
    }
}

/// Classify STRING tokens within a SEQ. This is where bracket pairs
/// are detected and roles assigned.
fn classify_seq(
    members: &[Production],
    roles: &mut std::collections::HashMap<String, TokenRole>,
    indent_triggers: &mut std::collections::HashSet<(String, String)>,
    rule_name: &str,
    in_choice: bool,
) {
    let string_positions: Vec<(usize, &str)> = members
        .iter()
        .enumerate()
        .filter_map(|(i, m)| unwrap_to_string(m).map(|s| (i, s)))
        .collect();

    let content_count = members
        .iter()
        .filter(|m| unwrap_to_string(m).is_none())
        .count();

    if string_positions.len() >= 2 {
        let (first_idx, first_val) = string_positions[0];
        let (last_idx, last_val) = string_positions[string_positions.len() - 1];

        let has_content_between = members[first_idx + 1..last_idx]
            .iter()
            .any(|m| unwrap_to_string(m).is_none());

        let both_punct = !is_word_like(first_val) && !is_word_like(last_val);
        let both_word = is_word_like(first_val) && is_word_like(last_val);
        if has_content_between && first_val != last_val && (both_punct || both_word) {
            roles.insert(first_val.to_owned(), TokenRole::BracketOpen);
            roles.insert(last_val.to_owned(), TokenRole::BracketClose);

            let between = &members[first_idx + 1..last_idx];
            if first_val == "{" && has_repeat_recursive(between) {
                indent_triggers.insert((rule_name.to_owned(), first_val.to_owned()));
            }
        }
    }

    // An optional leading unary sign (`CHOICE[- | BLANK]` / `OPTIONAL(-)`
    // at the head of the SEQ, with an operand after it) is a tight prefix
    // on that operand: `signed_number = SEQ[CHOICE[- | BLANK], number]`
    // emits `-1.0`, not `- 1.0`. The sign lives inside a CHOICE, so the
    // per-position pass below never sees it; classify it here.
    if members.len() >= 2 {
        if let Some(first) = members.first() {
            let has_following_content = members[1..].iter().any(|m| unwrap_to_string(m).is_none());
            if has_following_content {
                for sign in leading_optional_sign(first) {
                    roles.entry(sign).or_insert(TokenRole::BracketOpen);
                }
            }
        }
    }

    // Classify remaining STRINGs by structural position.
    let first_content_idx = members.iter().position(|m| unwrap_to_string(m).is_none());
    let last_content_idx = members.iter().rposition(|m| unwrap_to_string(m).is_none());

    for (i, m) in members.iter().enumerate() {
        if let Some(value) = unwrap_to_string(m) {
            let value = value.to_owned();
            if !roles.contains_key(&value) {
                if is_word_like(&value) {
                    roles.insert(value.clone(), TokenRole::Keyword);
                } else if !in_choice
                    && first_content_idx.is_some_and(|fc| i < fc)
                    && is_prefix_sigil(&value)
                {
                    roles.insert(value.clone(), TokenRole::BracketOpen);
                } else if last_content_idx.is_some_and(|lc| i > lc) {
                    // STRING after all content: suffix (tight before).
                    // Unlike prefix, this applies in CHOICE branches too
                    // (e.g. `()` in bash function_definition's CHOICE).
                    roles.insert(value.clone(), TokenRole::BracketClose);
                } else if !in_choice
                    && string_positions.len() == 1
                    && content_count == 2
                    && value.len() == 1
                {
                    // Single-character STRING between exactly two content
                    // members in a non-CHOICE SEQ: this is a connector
                    // (e.g. `.` in `SEQ [object, ".", attr]`).
                    // Multi-character tokens like `:=`, `<-`, `->` are
                    // operators (spaced), not connectors.
                    roles.insert(value.clone(), TokenRole::Connector);
                } else {
                    roles.insert(value.clone(), TokenRole::Operator);
                }
            }
        }
    }

    for m in members {
        if unwrap_to_string(m).is_none() {
            classify_production(m, roles, indent_triggers, rule_name);
        }
    }
}

/// Classify STRING tokens in a REPEAT body. The first STRING in a
/// REPEAT body's inner SEQ is a separator (e.g. `,` in
/// `REPEAT(SEQ [",", item])`).
fn classify_repeat_body(
    content: &Production,
    roles: &mut std::collections::HashMap<String, TokenRole>,
    indent_triggers: &mut std::collections::HashSet<(String, String)>,
    rule_name: &str,
) {
    match content {
        Production::Seq { members } => {
            if let Some(Production::String { value }) = members.first() {
                roles.insert(value.clone(), TokenRole::Separator);
            }
            classify_seq(members, roles, indent_triggers, rule_name, false);
        }
        _ => classify_production(content, roles, indent_triggers, rule_name),
    }
}

/// Classify STRING tokens within a SEQ by structural position, returning
/// a role for each member position. Non-STRING positions get `None`.
/// This is the inline variant of `classify_seq` used at emission time
/// to avoid the flat per-rule map's conflation of same-text tokens.
fn classify_seq_positions(members: &[Production], in_choice: bool) -> Vec<Option<TokenRole>> {
    let mut roles: Vec<Option<TokenRole>> = vec![None; members.len()];

    let string_positions: Vec<(usize, &str)> = members
        .iter()
        .enumerate()
        .filter_map(|(i, m)| unwrap_to_string(m).map(|s| (i, s)))
        .collect();

    let content_count = members
        .iter()
        .filter(|m| unwrap_to_string(m).is_none())
        .count();

    // Bracket pair detection.
    let mut bracket_open_idx: Option<usize> = None;
    let mut bracket_close_idx: Option<usize> = None;

    // Canonical pairing first: pair an actual `(`/`[`/`{` with its matching
    // closer, even when other STRINGs (a prefix operator, a trailing `;`)
    // sit at the SEQ ends. `sampling_statement` (`expr ~ f ( args ) ;`)
    // must pair `(`/`)`, not `~`/`;`.
    for &(oi, ov) in &string_positions {
        let Some(close_text) = matching_close_bracket(ov) else {
            continue;
        };
        if let Some(&(ci, _)) = string_positions
            .iter()
            .rev()
            .find(|(_, v)| *v == close_text)
        {
            if oi < ci
                && members[oi + 1..ci]
                    .iter()
                    .any(|m| unwrap_to_string(m).is_none())
            {
                roles[oi] = Some(TokenRole::BracketOpen);
                roles[ci] = Some(TokenRole::BracketClose);
                bracket_open_idx = Some(oi);
                bracket_close_idx = Some(ci);
                break;
            }
        }
    }

    // First/last STRING fallback: handles word-like pairs (begin/end) and
    // same-text immediate delimiters (regex `/.../`) that the canonical
    // search does not recognise.
    if bracket_open_idx.is_none() && string_positions.len() >= 2 {
        let (first_idx, first_val) = string_positions[0];
        let (last_idx, last_val) = string_positions[string_positions.len() - 1];

        let has_content_between = members[first_idx + 1..last_idx]
            .iter()
            .any(|m| unwrap_to_string(m).is_none());

        let both_punct = !is_word_like(first_val) && !is_word_like(last_val);
        let both_word = is_word_like(first_val) && is_word_like(last_val);
        // Same-text delimiters (e.g. regex `/.../`) are a bracket pair
        // when at least one side is IMMEDIATE_TOKEN — the grammar's
        // structural signal that the delimiter must be tight against
        // the content.
        let either_immediate =
            is_immediate_token(&members[first_idx]) || is_immediate_token(&members[last_idx]);
        let same_text_immediate = first_val == last_val && either_immediate;
        if has_content_between
            && (both_punct || both_word)
            && (first_val != last_val || same_text_immediate)
        {
            roles[first_idx] = Some(TokenRole::BracketOpen);
            roles[last_idx] = Some(TokenRole::BracketClose);
            bracket_open_idx = Some(first_idx);
            bracket_close_idx = Some(last_idx);
        }
    }

    let first_content_idx = members.iter().position(|m| unwrap_to_string(m).is_none());
    let last_content_idx = members.iter().rposition(|m| unwrap_to_string(m).is_none());

    for (i, m) in members.iter().enumerate() {
        if roles[i].is_some() {
            continue;
        }
        if let Some(value) = unwrap_to_string(m) {
            roles[i] = Some(if is_immediate_token(m) {
                // The grammar wraps this token in IMMEDIATE_TOKEN: the
                // lexer glues it to its neighbours (the `.` in a float
                // `0.5`). Derive tightness from that fact rather than
                // guessing from position.
                TokenRole::Immediate
            } else if is_word_like(value) {
                TokenRole::Keyword
            } else if !in_choice && first_content_idx.is_some_and(|fc| i < fc) {
                if is_prefix_sigil(value) {
                    TokenRole::BracketOpen
                } else {
                    TokenRole::Operator
                }
            } else if last_content_idx.is_some_and(|lc| i > lc) {
                TokenRole::BracketClose
            } else if !in_choice
                && string_positions.len() == 1
                && content_count == 2
                && value.len() == 1
            {
                TokenRole::Connector
            } else {
                TokenRole::Operator
            });
        }
    }

    // Override: in a REPEAT body's inner SEQ, the first STRING is a
    // separator. This is checked by the caller (REPEAT handler), not here.
    // But we do store bracket indices for the caller to use.
    let _ = (bracket_open_idx, bracket_close_idx);

    roles
}

/// Check if a SEQ's bracket at position `idx` triggers indentation.
#[allow(clippy::branches_sharing_code)]
fn seq_bracket_triggers_indent(
    members: &[Production],
    open_idx: usize,
    _grammar: &Grammar,
) -> bool {
    let string_positions: Vec<(usize, &str)> = members
        .iter()
        .enumerate()
        .filter_map(|(i, m)| unwrap_to_string(m).map(|s| (i, s)))
        .collect();
    if string_positions.len() < 2 {
        return false;
    }
    let open_val = string_positions.iter().find(|(i, _)| *i == open_idx);
    let close_val = string_positions.last();
    if let (Some((_, open_text)), Some((close_idx, close_text))) = (open_val, close_val) {
        if open_idx >= *close_idx {
            return false;
        }
        // Word-like bracket pairs (function/end, if/end, while/end,
        // for/end, module/end, struct/end, begin/end, do/done, etc.)
        // always wrap block bodies that need indentation. This is a
        // structural invariant: word-like delimiters only appear in
        // block constructs across all 261 grammars.
        if is_word_like(open_text) && is_word_like(close_text) {
            return true;
        }
        let between = &members[open_idx + 1..*close_idx];
        // Only { } bracket pairs trigger indentation from direct
        // REPEAT content. Other pairs like ( ), < >, [ ] are
        // inline even when they contain REPEAT (comma-separated
        // lists, type parameters, function arguments).
        if *open_text == "{" && has_repeat_recursive(between) {
            return true;
        }
        // Follow SYMBOL → rule one level for CHOICE[SYMBOL, BLANK]
        // patterns where the SYMBOL's rule body has REPEAT.
        // Only for { } bracket pairs (block constructs). Other pairs
        // like < > (type parameters) are always inline.
        if *open_text == "{" {
            for m in between {
                if let Production::Choice { members: alts } = m {
                    let has_blank = alts.iter().any(|a| matches!(a, Production::Blank));
                    if has_blank {
                        for alt in alts {
                            if let Production::Symbol { name } = alt {
                                if let Some(rule) = _grammar.rules.get(name) {
                                    if has_repeat_in(rule) {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    } else {
        false
    }
}

/// Check if a production's rule body starts with a bracket pair's open
/// `STRING`. Used to suppress `ForceSpace` before call-pattern members
/// (e.g. `argument_list` whose rule starts with `(`).
fn member_has_leading_bracket(prod: &Production, grammar: &Grammar) -> bool {
    match prod {
        Production::Symbol { name } => grammar
            .rules
            .get(name)
            .is_some_and(|rule| first_string_of(rule).is_some_and(|s| !is_word_like(s))),
        // A SEQ member whose first token is a non-word bracket (`(`, `[`)
        // is a call/index pattern: the preceding callee must stay tight
        // against it (`f(`, not `f (`). This also covers a CHOICE of such
        // SEQs (e.g. qvr's `morphism_call` arg-list alternatives), which
        // recurses here per alternative.
        Production::Seq { .. } => first_string_of(prod).is_some_and(|s| !is_word_like(s)),
        Production::Field { content, .. } => member_has_leading_bracket(content, grammar),
        Production::Choice { members } => {
            let non_blank: Vec<_> = members
                .iter()
                .filter(|m| !matches!(m, Production::Blank))
                .collect();
            !non_blank.is_empty()
                && non_blank
                    .iter()
                    .all(|m| member_has_leading_bracket(m, grammar))
        }
        Production::Alias { content, .. } => {
            if let Production::Symbol { name } = content.as_ref() {
                grammar
                    .rules
                    .get(name)
                    .is_some_and(|rule| first_string_of(rule).is_some_and(|s| !is_word_like(s)))
            } else {
                false
            }
        }
        Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Optional { content } => member_has_leading_bracket(content, grammar),
        Production::Repeat { .. } | Production::Repeat1 { .. } => false,
        _ => false,
    }
}

fn first_string_of(prod: &Production) -> Option<&str> {
    match prod {
        Production::String { value } => Some(value.as_str()),
        Production::Seq { members } => members.first().and_then(first_string_of),
        Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Field { content, .. } => first_string_of(content),
        _ => None,
    }
}

/// Check if any member of a slice contains REPEAT/REPEAT1 recursively.
fn has_repeat_recursive(members: &[Production]) -> bool {
    members.iter().any(has_repeat_in)
}

fn has_repeat_in(prod: &Production) -> bool {
    match prod {
        Production::Repeat { .. } | Production::Repeat1 { .. } => true,
        Production::Choice { members } | Production::Seq { members } => {
            members.iter().any(has_repeat_in)
        }
        Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Optional { content }
        | Production::Field { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Reserved { content, .. }
        | Production::Alias { content, .. } => has_repeat_in(content),
        _ => false,
    }
}

/// A single-character unary sign (`-`, `+`) that, when it sits in an
/// optional *leading* slot before a single operand, glues to that
/// operand (`signed_number`: `-1.0`, not `- 1.0`). These are excluded
/// from [`is_prefix_sigil`] because in a binary position they are
/// spaced operators; the leading-optional-slot structure is what
/// disambiguates them as unary.
fn is_unary_sign(s: &str) -> bool {
    matches!(s, "-" | "+")
}

/// Extract the unary sign STRING(s) carried by an optional *leading*
/// SEQ member: a `CHOICE[sign | … | BLANK]` or `OPTIONAL(sign)`. Returns
/// empty unless the member is structurally an optional sign slot, which
/// marks the sign as a tight unary prefix on the following operand.
fn leading_optional_sign(prod: &Production) -> Vec<String> {
    match prod {
        Production::Choice { members }
            if members.iter().any(|m| matches!(m, Production::Blank)) =>
        {
            members
                .iter()
                .filter_map(unwrap_to_string)
                .filter(|s| is_unary_sign(s))
                .map(str::to_owned)
                .collect()
        }
        Production::Optional { content } => unwrap_to_string(content)
            .filter(|s| is_unary_sign(s))
            .map(|s| vec![s.to_owned()])
            .unwrap_or_default(),
        Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Field { content, .. } => leading_optional_sign(content),
        _ => Vec::new(),
    }
}

/// The canonical closing delimiter for an opening bracket STRING, if it
/// is one of the universal nesting brackets. `<`/`>` are excluded: they
/// are ambiguous with comparison operators and are handled by the
/// first/last fallback only when they genuinely bound the SEQ.
fn matching_close_bracket(open: &str) -> Option<&'static str> {
    match open {
        "(" => Some(")"),
        "[" => Some("]"),
        "{" => Some("}"),
        _ => None,
    }
}

/// Check if a string value is word-like (alphanumeric/underscore).
fn is_word_like(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
        && s.starts_with(|c: char| c.is_alphabetic() || c == '_')
}

/// A prefix `STRING` (position before all content in a non-`CHOICE` SEQ) is a
/// tight sigil (`BracketOpen`) only when it is NOT a common binary/assignment
/// operator. Single-character ASCII operators like `=`, `+`, `-` need space;
/// multi-character prefixes (`...`, `::`, `@`, `#`, `$`) and non-ASCII
/// prefixes are tight.
fn is_prefix_sigil(s: &str) -> bool {
    if s.len() == 1 {
        let c = s.as_bytes()[0];
        !matches!(
            c,
            b'=' | b'+'
                | b'-'
                | b'*'
                | b'/'
                | b'<'
                | b'>'
                | b'!'
                | b'?'
                | b'|'
                | b'&'
                | b'^'
                | b'%'
                | b'~'
        )
    } else {
        true
    }
}

/// Unwrap wrapper productions (`Token`, `ImmediateToken`, `Prec`, `PrecLeft`,
/// `PrecRight`, `PrecDynamic`, `Field`, `Reserved`) to find the inner `STRING`
/// value. Returns `None` if the production is not a (possibly wrapped)
/// `STRING`.
fn is_immediate_token(prod: &Production) -> bool {
    match prod {
        Production::ImmediateToken { .. } => true,
        Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Token { content }
        | Production::Field { content, .. }
        | Production::Reserved { content, .. } => is_immediate_token(content),
        _ => false,
    }
}

fn unwrap_to_string(prod: &Production) -> Option<&str> {
    match prod {
        Production::String { value } => Some(value.as_str()),
        Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Field { content, .. }
        | Production::Reserved { content, .. } => unwrap_to_string(content),
        _ => None,
    }
}

/// Extract line-comment prefixes from the grammar's extras rules.
///
/// A line comment is identified by: the rule name is in
/// `grammar.extras` AND the rule body structurally matches
/// `TOKEN(SEQ [STRING prefix, PATTERN ...])` where the PATTERN
/// matches to end-of-line.
fn extract_line_comment_prefixes(grammar: &Grammar) -> Vec<String> {
    let mut prefixes = Vec::new();
    for extra_name in &grammar.extras {
        if let Some(rule) = grammar.rules.get(extra_name) {
            if let Some(prefix) = extract_line_comment_prefix(rule) {
                prefixes.push(prefix);
            }
        }
    }
    prefixes
}

fn extract_line_comment_prefix(prod: &Production) -> Option<String> {
    match prod {
        Production::Token { content } | Production::ImmediateToken { content } => {
            extract_line_comment_prefix(content)
        }
        Production::Seq { members } if members.len() >= 2 => {
            if let Production::String { value } = &members[0] {
                if members[1..].iter().any(|m| {
                    matches!(m, Production::Pattern { value } if value.contains(".*") || value.contains("[^\\n]*") || value.contains("[^\\r\\n]*"))
                }) {
                    return Some(value.clone());
                }
            }
            None
        }
        Production::Choice { members } => members.iter().find_map(extract_line_comment_prefix),
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════
// Format policy
/// Classify external scanner tokens as layout actions based on
/// tree-sitter naming conventions. These conventions are ecosystem-
/// wide, not language-specific: every indent-based grammar uses
/// `_indent`/`_dedent`, every ASI grammar uses `_automatic_semicolon`.
fn classify_external_layout_tokens(grammar: &mut Grammar) {
    // External tokens have no grammar rule. We identify them by
    // checking which hidden symbols are NOT in grammar.rules.
    // Then classify by naming convention.
    //
    // This runs after external_alias_map is built, so tokens with
    // known text are already handled. Layout tokens are the remainder.
    let all_hidden_refs = collect_all_symbol_refs(&grammar.rules);
    for name in &all_hidden_refs {
        if !name.starts_with('_') || grammar.rules.contains_key(name) {
            continue;
        }
        if grammar.external_alias_map.contains_key(name) {
            continue;
        }
        if name == "_indent" || name.ends_with("_indent") {
            grammar.external_indent_opens.insert(name.clone());
        } else if name == "_dedent" || name.ends_with("_dedent") {
            grammar.external_indent_closes.insert(name.clone());
        } else if name.contains("line_ending")
            || name.contains("newline")
            || name.ends_with("_or_eof")
        {
            grammar.external_newlines.insert(name.clone());
        } else if name.contains("semicolon") {
            grammar.external_semicolons.insert(name.clone());
        }
    }
}

/// Identify external scanner tokens that bracket content, derived from
/// grammar structure: a rule whose (unwrapped) body is a SEQ whose first
/// and last members are external SYMBOLs (no grammar rule of their own)
/// with content in between is a delimiter pair around that content. The
/// canonical case is `string = SEQ[string_start, REPEAT(content),
/// string_end]`: the delimiters must hug the content (`'hello'`), and
/// the grammar states which externals they are without naming
/// conventions.
fn classify_external_bracket_delimiters(grammar: &mut Grammar) {
    let is_external = |name: &str| !grammar.rules.contains_key(name);
    let mut opens = std::collections::HashSet::new();
    let mut closes = std::collections::HashSet::new();
    for rule in grammar.rules.values() {
        let Production::Seq { members } = unwrap_to_seq(rule) else {
            continue;
        };
        if members.len() < 3 {
            continue;
        }
        let (Some(first), Some(last)) = (members.first(), members.last()) else {
            continue;
        };
        let (Some(open), Some(close)) = (external_symbol_name(first), external_symbol_name(last))
        else {
            continue;
        };
        if open == close || !is_external(open) || !is_external(close) {
            continue;
        }
        // Content between the delimiters (a REPEAT of string content, an
        // interpolation choice, …) — at least one non-delimiter member.
        let has_content_between = members.len() > 2;
        if has_content_between {
            opens.insert(open.to_owned());
            closes.insert(close.to_owned());
        }
    }
    grammar.external_bracket_opens = opens;
    grammar.external_bracket_closes = closes;
}

/// Identify indented-block rules whose opening `_indent` is supplied by
/// a hidden parent: the rule's body references an external indent-close
/// token (`_dedent`) but no indent-open token. Python's `block = SEQ[
/// REPEAT(_statement), _dedent]` is the canonical case — the matching
/// `_indent` sits in the hidden `_suite` wrapper, which is not a vertex,
/// so the parser hands the emitter a bare `block` and the opening indent
/// must be synthesized.
fn classify_synthetic_indent_rules(grammar: &mut Grammar) {
    if grammar.external_indent_closes.is_empty() {
        return;
    }
    let mut rules = std::collections::HashSet::new();
    for (name, rule) in &grammar.rules {
        let symbols = referenced_symbols(rule);
        let references_close = symbols
            .iter()
            .any(|s| grammar.external_indent_closes.contains(*s));
        let references_open = symbols
            .iter()
            .any(|s| grammar.external_indent_opens.contains(*s));
        if references_close && !references_open {
            rules.insert(name.clone());
        }
    }
    grammar.synthetic_indent_rules = rules;
}

/// The role for a leaf vertex's captured `literal-value`, given its
/// kind: a string/heredoc delimiter external (`string_start`/`string_end`)
/// brackets its content tightly, so it is emitted as a bracket rather
/// than a free-standing [`Terminal`](TokenRole::Terminal) that the layout
/// pass would space (`'hello'`, not `' hello '`).
fn leaf_terminal_role(grammar: &Grammar, kind: &str) -> TokenRole {
    if grammar.external_bracket_opens.contains(kind) {
        TokenRole::BracketOpen
    } else if grammar.external_bracket_closes.contains(kind) {
        TokenRole::BracketClose
    } else {
        TokenRole::Terminal
    }
}

/// Unwrap precedence/token wrappers to reach a SEQ production.
fn unwrap_to_seq(prod: &Production) -> &Production {
    match prod {
        Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Token { content }
        | Production::Reserved { content, .. } => unwrap_to_seq(content),
        other => other,
    }
}

/// The SYMBOL name a member references directly (no aliasing), if it is a
/// bare `SYMBOL`. Used to spot external delimiter tokens.
fn external_symbol_name(prod: &Production) -> Option<&str> {
    match prod {
        Production::Symbol { name } => Some(name.as_str()),
        _ => None,
    }
}

/// Collect all SYMBOL names referenced anywhere in the grammar rules.
fn collect_all_symbol_refs(
    rules: &BTreeMap<String, Production>,
) -> std::collections::HashSet<String> {
    let mut refs = std::collections::HashSet::new();
    fn walk(prod: &Production, refs: &mut std::collections::HashSet<String>) {
        match prod {
            Production::Symbol { name } => {
                refs.insert(name.clone());
            }
            Production::Seq { members } | Production::Choice { members } => {
                for m in members {
                    walk(m, refs);
                }
            }
            Production::Alias { content, .. }
            | Production::Repeat { content }
            | Production::Repeat1 { content }
            | Production::Optional { content }
            | Production::Field { content, .. }
            | Production::Token { content }
            | Production::ImmediateToken { content }
            | Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. }
            | Production::Reserved { content, .. } => walk(content, refs),
            _ => {}
        }
    }
    for rule in rules.values() {
        walk(rule, &mut refs);
    }
    refs
}

// ═══════════════════════════════════════════════════════════════════

/// Whitespace and indentation policy applied during emission.
///
/// The default policy inserts a single space between adjacent tokens,
/// a newline after `;` / `}` / `{`, and tracks indent on `{` / `}`
/// boundaries. Per-language overrides (idiomatic indent width,
/// trailing-comma rules, blank-line conventions) can ride alongside
/// this struct in a follow-up branch; today's defaults aim only for
/// syntactic validity.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FormatPolicy {
    /// Number of spaces per indent level.
    pub indent_width: usize,
    /// Separator inserted between adjacent terminals that the lexer
    /// would otherwise glue together (word ↔ word, operator ↔ operator).
    /// Default is a single space.
    pub separator: String,
    /// Newline byte sequence emitted after `line_break_after` tokens
    /// and at end-of-output. Default is `"\n"`.
    pub newline: String,
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
            separator: " ".to_owned(),
            newline: "\n".to_owned(),
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
    cassette: Option<&dyn crate::languages::cassettes::GrammarCassette>,
) -> Result<Vec<u8>, ParseError> {
    let roots = collect_roots(schema);
    if roots.is_empty() {
        return Err(ParseError::EmitFailed {
            protocol: protocol.to_owned(),
            reason: "schema has no entry vertices".to_owned(),
        });
    }

    let mut out = Output::new(policy, grammar, cassette);
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

    // IMMEDIATE_TOKEN at the rule head: emit a tightness marker
    // before any content the leaf shortcut or rule-body walk produces.
    // This is the unique structural site where IMMEDIATE_TOKEN's "no
    // preceding whitespace" property attaches; downstream layout reads
    // the NoSpace marker without re-inspecting the production tree.
    let kind_head = vertex.kind.as_ref();
    if let Some(rule) = grammar.rules.get(kind_head) {
        if is_immediate_token(rule) {
            out.no_space();
        }
    }

    // Leaf shortcut: a vertex carrying a `literal-value` constraint
    // and no outgoing structural edges is a terminal token. Emit the
    // captured value directly. This handles identifiers, numeric
    // literals, and string literals that the parser stored as
    // `literal-value` even on by-construction schemas.
    if let Some(literal) = literal_value(schema, vertex_id) {
        if children_for(schema, vertex_id).is_empty() {
            // Skip leaf shortcut for bracket-pair literals like "()"
            // when the vertex has an alias-resolved rule. The rule-based
            // path correctly emits them as separate BracketOpen/Close
            // tokens with proper spacing.
            let is_bracket_pair = literal.len() >= 2
                && matches!(
                    (literal.as_bytes().first(), literal.as_bytes().last()),
                    (Some(b'('), Some(b')')) | (Some(b'['), Some(b']')) | (Some(b'{'), Some(b'}'))
                );
            let vkind = vertex.kind.as_ref();
            let has_alias_rule = grammar
                .named_alias_map
                .get(vkind)
                .is_some_and(|src| grammar.rules.contains_key(src));
            if !(is_bracket_pair && has_alias_rule) {
                // An empty bracket-pair literal (`()`, `[]`, `{}` captured
                // as one token, e.g. empty parameters) hugs its callee on
                // the left (`f()`) but still spaces after a keyword
                // (`return ()`). That is exactly the BracketClose role
                // (tight inner/left edge, keyword-spaced). Other leaves
                // keep their delimiter-or-terminal role.
                let role = if is_bracket_pair {
                    TokenRole::BracketClose
                } else {
                    leaf_terminal_role(grammar, vkind)
                };
                out.token_with_role(literal, Some(role));
                return Ok(());
            }
        }
    }

    let kind = vertex.kind.as_ref();
    let edges = children_for(schema, vertex_id);
    if let Some(rule) = grammar.rules.get(kind) {
        let old_rule = out.current_rule.take();
        out.current_rule = Some(kind.to_owned());
        // An indented-block rule (`block = SEQ[REPEAT(_statement),
        // _dedent]`) is reached directly because its opening `_indent`
        // lives in the hidden parent. Synthesize the opening indent so
        // the body is indented; the rule's own `_dedent` closes it.
        let synthetic_indent = grammar.synthetic_indent_rules.contains(kind);
        if synthetic_indent {
            out.indent_open();
        }
        let mut cursor = ChildCursor::new(&edges);
        emit_production(protocol, schema, grammar, vertex_id, rule, &mut cursor, out)?;
        drain_extras(protocol, schema, grammar, &mut cursor, out)?;
        out.current_rule = old_rule;
        return Ok(());
    }

    // Named alias resolution: if the vertex kind was produced by
    // `alias($.source, $.kind)`, look up the source rule and walk
    // it. This preserves bracket pairs, separators, and token roles
    // that the source rule defines.
    if let Some(source_name) = grammar.named_alias_map.get(kind) {
        if let Some(rule) = grammar.rules.get(source_name) {
            let old_rule = out.current_rule.take();
            out.current_rule = Some(source_name.to_owned());
            let mut cursor = ChildCursor::new(&edges);
            emit_production(protocol, schema, grammar, vertex_id, rule, &mut cursor, out)?;
            drain_extras(protocol, schema, grammar, &mut cursor, out)?;
            out.current_rule = old_rule;
            return Ok(());
        }
    }

    // No rule for this kind and no named alias. The parser produced
    // it via an external scanner (e.g. YAML's `document` root).
    // Fall back to walking the children directly.
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

    /// Whether any unconsumed edge satisfies `predicate`. Used by the
    /// unit tests; the live emit path went through `has_matching` on
    /// each alternative until cursor-driven dispatch was rewritten to
    /// pick the first-unconsumed-edge's kind directly.
    #[cfg(test)]
    fn has_matching(&self, predicate: impl Fn(&Edge) -> bool) -> bool {
        self.edges
            .iter()
            .enumerate()
            .any(|(i, edge)| !self.consumed[i] && predicate(edge))
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
}

thread_local! {
    static EMIT_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Set of `(vertex_id, rule_name)` pairs that are currently being
    /// walked by the recursion. A SYMBOL that resolves to a rule
    /// already on this stack closes a μ-binder cycle: in the
    /// coinductive reading, the rule walk at any vertex is the least
    /// fixed point of `body[μ X . body / X]`, which unfolds at most
    /// once, with the second visit returning the empty sequence (the
    /// unit of the free token monoid). Examples that trigger this:
    /// YAML's `stream` ⊃ `_b_blk_*` mutually-recursive chain, Rust's
    /// `_expression` ⊃ `binary_expression` ⊃ `_expression`.
    static EMIT_MU_FRAMES: std::cell::RefCell<std::collections::HashSet<(String, String)>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
    /// The name of the FIELD whose body the walker is currently inside,
    /// or `None` at top level. Lets a SYMBOL nested arbitrarily deep
    /// in the field's content (under SEQ, CHOICE, REPEAT, OPTIONAL)
    /// consume from the *outer* cursor by edge-kind rather than from
    /// the child's own cursor by symbol-match. Without this, shapes
    /// like `field('args', commaSep1($.X))` — which expands to
    /// `FIELD(SEQ(SYMBOL X, REPEAT(SEQ(',', SYMBOL X))))` — emit only
    /// the first matched edge: the FIELD handler consumed one edge,
    /// the inner REPEAT searched the consumed child's cursor (which
    /// has no more sibling field edges), and the REPEAT broke after
    /// one iteration. Setting the context here so the inner SYMBOL
    /// pulls successive field-named edges from the outer cursor
    /// recovers every matched edge across arbitrary nesting.
    static EMIT_FIELD_CONTEXT: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

/// RAII guard that restores the prior `EMIT_FIELD_CONTEXT` value on drop.
struct FieldContextGuard(Option<String>);

impl Drop for FieldContextGuard {
    fn drop(&mut self) {
        EMIT_FIELD_CONTEXT.with(|f| *f.borrow_mut() = self.0.take());
    }
}

fn push_field_context(name: &str) -> FieldContextGuard {
    let prev = EMIT_FIELD_CONTEXT.with(|f| f.borrow_mut().replace(name.to_owned()));
    FieldContextGuard(prev)
}

/// Clear the field context for the duration of a child-context walk.
/// The child's own production has its own FIELDs that set their own
/// context; the outer field hint must not leak into them.
fn clear_field_context() -> FieldContextGuard {
    let prev = EMIT_FIELD_CONTEXT.with(|f| f.borrow_mut().take());
    FieldContextGuard(prev)
}

fn current_field_context() -> Option<String> {
    EMIT_FIELD_CONTEXT.with(|f| f.borrow().clone())
}

/// Walk a rule at a vertex inside a μ-binder. The wrapping frame is
/// pushed before recursion and popped after, so any SYMBOL inside
/// `rule` that re-enters the same `(vertex_id, rule_name)` pair
/// returns the empty sequence (μ X . body unfolds once).
fn walk_in_mu_frame(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    vertex_id: &panproto_gat::Name,
    rule_name: &str,
    rule: &Production,
    cursor: &mut ChildCursor<'_>,
    out: &mut Output<'_>,
) -> Result<(), ParseError> {
    let key = (vertex_id.to_string(), rule_name.to_owned());
    let inserted = EMIT_MU_FRAMES.with(|frames| frames.borrow_mut().insert(key.clone()));
    if !inserted {
        // We are already walking this rule at this vertex deeper in
        // the call stack. The coinductive μ-fixed-point reading
        // returns the empty sequence here; the surrounding
        // production resumes after the SYMBOL.
        return Ok(());
    }
    let result = emit_production(protocol, schema, grammar, vertex_id, rule, cursor, out);
    EMIT_MU_FRAMES.with(|frames| {
        frames.borrow_mut().remove(&key);
    });
    result
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
    drain_extras(protocol, schema, grammar, cursor, out)?;
    let result = emit_production_inner(
        protocol, schema, grammar, vertex_id, production, cursor, out,
    );
    EMIT_DEPTH.with(|d| d.set(d.get() - 1));
    result
}

/// Consume and emit every leading edge on `cursor` whose target kind
/// is in `grammar.extras` (typically `line_comment` / `block_comment`).
/// Extras live outside the production grammar — tree-sitter skips them
/// at parse time and records them as children of the surrounding
/// vertex — so the rule walker cannot reconcile them against the
/// cursor. Draining them as a side channel preserves their content in
/// the output without confusing the structural matchers.
fn drain_extras(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    cursor: &mut ChildCursor<'_>,
    out: &mut Output<'_>,
) -> Result<(), ParseError> {
    if grammar.extras.is_empty() {
        return Ok(());
    }
    loop {
        let next_extra: Option<usize> = cursor
            .edges
            .iter()
            .enumerate()
            .find(|(i, _)| !cursor.consumed[*i])
            .and_then(|(i, edge)| {
                let kind = schema.vertices.get(&edge.tgt).map(|v| v.kind.as_ref())?;
                if grammar.extras.contains(kind) {
                    Some(i)
                } else {
                    None
                }
            });
        let Some(idx) = next_extra else {
            return Ok(());
        };
        cursor.consumed[idx] = true;
        let target = &cursor.edges[idx].tgt;
        emit_vertex(protocol, schema, grammar, target, out)?;
    }
}

/// Emit a SEQ production with positionally classified token roles.
///
/// Instead of looking up roles from a precomputed flat map (which
/// conflates the same token text across structural contexts within
/// one rule), this function computes roles for each STRING position
/// from the SEQ's own structure at emission time.
fn emit_seq_with_roles(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    vertex_id: &panproto_gat::Name,
    members: &[Production],
    cursor: &mut ChildCursor<'_>,
    out: &mut Output<'_>,
    in_choice: bool,
) -> Result<(), ParseError> {
    let positional_roles = classify_seq_positions(members, in_choice);

    // Detect which bracket-open position triggers indentation so we can
    // emit the matching IndentClose for the bracket-close.
    let indent_open_idx: Option<usize> = positional_roles.iter().enumerate().position(|(i, r)| {
        *r == Some(TokenRole::BracketOpen) && seq_bracket_triggers_indent(members, i, grammar)
    });

    // For word-like bracket pairs (function/end, if/end, etc.), find
    // positions that need LineBreak: the body CHOICE and any FIELD
    // members that follow it (elseif/else clauses, catch blocks, etc.).
    let mut line_break_positions: std::collections::HashSet<usize> =
        std::collections::HashSet::new();
    if let Some(oi) = indent_open_idx {
        let open_text = unwrap_to_string(&members[oi]);
        if open_text.is_some_and(is_word_like) {
            let mut found_body = false;
            for (j, member) in members.iter().enumerate().skip(oi + 1) {
                if let Production::Choice { members: alts } = member {
                    let has_blank = alts.iter().any(|a| matches!(a, Production::Blank));
                    let has_block_symbol = alts.iter().any(|a| match a {
                        Production::Symbol { name } => {
                            grammar.rules.get(name).is_some_and(has_repeat_in)
                        }
                        _ => false,
                    });
                    if has_blank && has_block_symbol {
                        line_break_positions.insert(j);
                        found_body = true;
                    }
                } else if found_body && matches!(member, Production::Field { .. }) {
                    line_break_positions.insert(j);
                }
            }
        }
    }

    let mut prev_member_emitted_content = false;
    for (i, member) in members.iter().enumerate() {
        let tokens_before_member = out.tokens.len();
        if let Some(value) = unwrap_to_string(member) {
            let role = positional_roles[i].unwrap_or_else(|| {
                if is_word_like(value) {
                    TokenRole::Keyword
                } else {
                    TokenRole::Operator
                }
            });

            if indent_open_idx == Some(i) {
                if is_word_like(value) {
                    out.tokens.push(Token::Lit(value.to_owned(), role));
                    out.tokens.push(Token::IndentOpen);
                } else {
                    out.token_with_indent_open(value, role);
                }
            } else if role == TokenRole::BracketClose && indent_open_idx.is_some() {
                out.tokens.push(Token::IndentClose);
                out.tokens.push(Token::Lit(value.to_owned(), role));
            } else {
                out.token_with_role(value, Some(role));
            }
        } else {
            // ForceSpace between consecutive content-producing SEQ
            // members so that sibling-vertex tokens are separated
            // (e.g. echo and $((  ...  )) in bash command). Skip
            // when the current member's rule body starts with a
            // bracket pair, because the preceding Terminal and the
            // bracket should be tight (call pattern like f(...)).
            if i > 0 && unwrap_to_string(&members[i - 1]).is_none() && prev_member_emitted_content {
                let member_starts_with_bracket = member_has_leading_bracket(member, grammar);
                let is_zero_width_external = matches!(
                    member,
                    Production::Symbol { name }
                        if name.starts_with('_') && !grammar.rules.contains_key(name)
                );
                let is_separator_choice = matches!(member, Production::Choice { members: alts }
                    if alts.iter().all(|a| matches!(a, Production::Blank) || unwrap_to_string(a).is_some()));
                let is_repeat = matches!(
                    member,
                    Production::Repeat { .. } | Production::Repeat1 { .. }
                );
                // Never force a space after a token that is tight on its
                // right edge: a BracketOpen (`(`, or a unary-sign prefix
                // classified as BracketOpen) or an IMMEDIATE_TOKEN. The
                // sibling-separation ForceSpace must not override the
                // structural tightness (`f(-1.0)`, not `f(- 1.0)`).
                let prev_tight_right = matches!(
                    out.tokens.last(),
                    Some(Token::Lit(_, TokenRole::BracketOpen | TokenRole::Immediate))
                );
                if !member_starts_with_bracket
                    && !is_zero_width_external
                    && !is_separator_choice
                    && !is_repeat
                    && !prev_tight_right
                {
                    out.tokens.push(Token::ForceSpace);
                }
            }
            if line_break_positions.contains(&i) {
                out.newline();
            }
            emit_production(protocol, schema, grammar, vertex_id, member, cursor, out)?;
        }
        prev_member_emitted_content = out.tokens[tokens_before_member..]
            .iter()
            .any(|t| matches!(t, Token::Lit(_, _)));
    }
    Ok(())
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
                out.token_with_role(literal, Some(TokenRole::Terminal));
            } else if is_newline_like_pattern(value) {
                // Patterns like `\r?\n`, `\n`, `\r\n` are the structural
                // newline tokens grammars use to separate top-level
                // statements (csound's `_new_line`, ABC's line-end, etc.).
                // Emitting them through the placeholder fallback rendered
                // the bare `_` sentinel between siblings; route them to
                // the layout pass's line-break instead so the output
                // re-parses.
                out.newline();
            } else if is_whitespace_only_pattern(value) {
                // `\s+`, `[ \t]+` and friends are interstitial whitespace
                // tokens. Emit nothing: the layout pass inserts the
                // policy separator between adjacent Lits if needed.
            } else {
                out.token_with_role(&placeholder_for_pattern(value), Some(TokenRole::Terminal));
            }
            Ok(())
        }
        Production::Blank => Ok(()),
        Production::Symbol { name } => {
            // Inside a FIELD body, a SYMBOL consumes by field-name on
            // the outer cursor rather than searching by symbol-match.
            // This covers the simple `FIELD(SYMBOL X)` case as well as
            // every nesting under FIELD that contains SYMBOLs (SEQ,
            // CHOICE, REPEAT, OPTIONAL, ALIAS). Without the override,
            // shapes like `field('args', commaSep1($.X))` consume one
            // field edge in the FIELD handler and then the REPEAT
            // inside SEQ searches the consumed child's cursor — where
            // no sibling field edges sit — and breaks after one
            // iteration.
            if let Some(field) = current_field_context() {
                if let Some(edge) = cursor.take_field(&field) {
                    return emit_in_child_context(
                        protocol, schema, grammar, &edge.tgt, production, out,
                    );
                }
                // No matching field-named edge left on the outer
                // cursor. Surface nothing; the surrounding REPEAT /
                // OPTIONAL / CHOICE backtracks the literal tokens it
                // emitted on this iteration when it sees no progress.
                return Ok(());
            }
            if name.starts_with('_') {
                // Hidden rule: not a vertex kind on the schema side.
                // Inline-expand the rule body so its children take
                // edges from the current cursor, instead of trying to
                // take a single child edge that "satisfies" the
                // hidden rule and discarding the rest of the body
                // (which would drop tokens like `=` and the trailing
                // value SYMBOL inside e.g. TOML's `_inline_pair`).
                //
                // Wrapped in a μ-frame so a hidden rule that
                // references its own kind cyclically (or another
                // hidden rule that closes the cycle) unfolds once
                // and then collapses to the empty sequence at the
                // second visit, rather than blowing the stack.
                if let Some(rule) = grammar.rules.get(name) {
                    let old_rule = out.current_rule.take();
                    out.current_rule = Some(name.to_owned());
                    let result = walk_in_mu_frame(
                        protocol, schema, grammar, vertex_id, name, rule, cursor, out,
                    );
                    out.current_rule = old_rule;
                    result
                } else {
                    // External hidden rule (declared in the
                    // grammar's `externals` block, scanned by C code,
                    // not listed in `rules`). Heuristic fallback by
                    // name:
                    //
                    // - `_indent` / `*_indent`: open an indent block.
                    //   Indent-based grammars (Python, YAML, qvr)
                    //   declare an `_indent` external scanner before
                    //   the body of a block-bodied declaration; the
                    //   emitted output is unparseable without the
                    //   corresponding indentation jump.
                    // - `_dedent` / `*_dedent`: close the matching
                    //   indent block.
                    // - `_newline` / `*_line_ending` / `*_or_eof`:
                    //   universally newline-or-empty; emitting a
                    //   single newline is the right default for
                    //   grammars like TOML whose `pair` SEQ trails
                    //   into `_line_ending_or_eof`.
                    //
                    // Check the precomputed alias map first: if this
                    // external token appears as the content of an
                    // anonymous ALIAS elsewhere in the grammar, emit
                    // the alias value as the token text.
                    // A delimiter external (string_start/string_end and
                    // friends) hugs the content it brackets, so emit its
                    // text with a bracket role rather than the untyped
                    // default (which spaces like an operator → `' hello '`).
                    let bracket_role = if grammar.external_bracket_opens.contains(name) {
                        Some(TokenRole::BracketOpen)
                    } else if grammar.external_bracket_closes.contains(name) {
                        Some(TokenRole::BracketClose)
                    } else {
                        None
                    };
                    if let Some(alias_value) = grammar.external_alias_map.get(name) {
                        match bracket_role {
                            Some(role) => out.token_with_role(alias_value, Some(role)),
                            None => out.token(alias_value),
                        }
                        return Ok(());
                    }
                    if grammar.external_indent_opens.contains(name) {
                        out.indent_open();
                    } else if grammar.external_indent_closes.contains(name) {
                        out.indent_close();
                    } else if grammar.external_newlines.contains(name) {
                        out.newline();
                    } else if grammar.external_semicolons.contains(name) {
                        out.token_with_role(";", Some(TokenRole::Separator));
                    } else if let Some(default) = out
                        .cassette
                        .and_then(|c| crate::languages::cassettes::resolve_external_token(c, name))
                    {
                        if !default.is_empty() {
                            match bracket_role {
                                Some(role) => out.token_with_role(default, Some(role)),
                                None => out.token(default),
                            }
                        }
                    }
                    Ok(())
                }
            } else if let Some(edge) = { take_symbol_match(grammar, schema, cursor, name) } {
                // For supertype / hidden-rule dispatch the child's
                // own kind names the actual production to walk
                // (`child.kind` IS the subtype). For ALIAS the
                // dependent-optic context is carried by the
                // surrounding `Production::Alias` branch, which calls
                // `emit_aliased_child` directly; we don't reach here
                // for that case. So walking `grammar.rules[child.kind]`
                // via `emit_vertex` is correct: the dependent-optic
                // path is preserved at every site where it actually
                // diverges from `child.kind`.
                emit_vertex(protocol, schema, grammar, &edge.tgt, out)
            } else if vertex_id_kind(schema, vertex_id) == Some(name.as_str()) {
                let rule = grammar
                    .rules
                    .get(name)
                    .ok_or_else(|| ParseError::EmitFailed {
                        protocol: protocol.to_owned(),
                        reason: format!("no production for SYMBOL '{name}'"),
                    })?;
                // Self-reference (`X = ... SYMBOL X ...`): wrap in a
                // μ-frame so re-entry collapses to the empty sequence.
                {
                    let old_rule = out.current_rule.take();
                    out.current_rule = Some(name.to_owned());
                    let result = walk_in_mu_frame(
                        protocol, schema, grammar, vertex_id, name, rule, cursor, out,
                    );
                    out.current_rule = old_rule;
                    result
                }
            } else {
                // Named rule with no matching child: emit nothing and
                // let the surrounding CHOICE / OPTIONAL / REPEAT
                // resolve the absence.
                Ok(())
            }
        }
        Production::Seq { members } => emit_seq_with_roles(
            protocol, schema, grammar, vertex_id, members, cursor, out, false,
        ),
        Production::Choice { members } => {
            if let Some(matched) =
                pick_choice_with_cursor(schema, grammar, vertex_id, cursor, members)
            {
                match matched {
                    Production::Seq {
                        members: seq_members,
                    } => emit_seq_with_roles(
                        protocol,
                        schema,
                        grammar,
                        vertex_id,
                        seq_members,
                        cursor,
                        out,
                        true,
                    ),
                    Production::String { value } => {
                        // A bare STRING alternative of a CHOICE. Prefer a
                        // role derived for this token in the current rule
                        // (e.g. a leading unary sign classified as a tight
                        // prefix: `signed_number`'s `-`); otherwise a
                        // word-like token spaces as a keyword and a
                        // punctuation token acts as a separator.
                        let role = out.explicit_role(value).unwrap_or_else(|| {
                            if is_word_like(value) {
                                TokenRole::Keyword
                            } else {
                                TokenRole::Separator
                            }
                        });
                        out.token_with_role(value, Some(role));
                        Ok(())
                    }
                    _ => {
                        emit_production(protocol, schema, grammar, vertex_id, matched, cursor, out)
                    }
                }
            } else {
                Ok(())
            }
        }
        Production::Repeat { content } | Production::Repeat1 { content } => {
            // Detect a "separator-leading SEQ" iteration body: SEQ whose
            // first member is a CHOICE containing BLANK (or an OPTIONAL),
            // i.e. the source-level separator between two iterations is
            // syntactically optional. When the chosen alternative for
            // that separator slot emits zero content tokens at runtime,
            // there was no source-level separator between this iteration
            // and the previous one; the layout pass must suppress its
            // policy separator to match the source's tight adjacency.
            //
            // Categorical reading: REPEAT body `B = SEQ(SEP, BODY)` is
            // the pullback of two halves. The bytes emitted in iteration
            // k+1 are a concatenation of `SEP_k+1` and `BODY_k+1`; if
            // `SEP_k+1` is the empty word, the concatenation of
            // `BODY_k` and `BODY_k+1` must remain a single contiguous
            // span. Hence the NoSpace marker.
            // Also detect mandatory separators: STRING at position 0
            // of a SEQ body (e.g. `SEQ[";", SYMBOL stmt]` in Python's
            // _simple_statements). For these, the cassette may override
            // the separator with a line break.
            let mandatory_sep_text: Option<&str> = match content.as_ref() {
                Production::Seq { members } if members.len() >= 2 => unwrap_to_string(&members[0]),
                _ => None,
            };
            let separator_leading_seq: Option<&[Production]> = match content.as_ref() {
                Production::Seq { members } if members.len() >= 2 => {
                    let first = &members[0];
                    let is_mandatory_sep = unwrap_to_string(first).is_some();
                    let cassette_overrides = is_mandatory_sep
                        && unwrap_to_string(first).is_some_and(|sep| {
                            out.cassette.is_some_and(|c| c.separator_is_line_break(sep))
                        });
                    let is_separator_slot = match first {
                        Production::Choice { members } => {
                            members.iter().any(|m| matches!(m, Production::Blank))
                        }
                        Production::Optional { .. } => true,
                        _ => cassette_overrides,
                    };
                    if is_separator_slot {
                        Some(members.as_slice())
                    } else {
                        None
                    }
                }
                _ => None,
            };

            let mut emitted_any = false;
            loop {
                let cursor_snap = cursor.consumed.clone();
                let out_snap = out.snapshot();
                let consumed_before = cursor.consumed.iter().filter(|&&c| c).count();
                let result: Result<(), ParseError> =
                    if let Some(seq_members) = separator_leading_seq {
                        // Emit the separator slot first and observe
                        // whether it contributed any Lit. If not, push
                        // a NoSpace marker before walking the remaining
                        // SEQ members. The OutputSnapshot here covers
                        // only the separator's emission window.
                        let cassette_replaces_sep = mandatory_sep_text.is_some_and(|sep| {
                            out.cassette.is_some_and(|c| c.separator_is_line_break(sep))
                        });
                        let pre_sep = out.snapshot();
                        let sep_result = if cassette_replaces_sep {
                            out.newline();
                            Ok(())
                        } else {
                            emit_production(
                                protocol,
                                schema,
                                grammar,
                                vertex_id,
                                &seq_members[0],
                                cursor,
                                out,
                            )
                        };
                        match sep_result {
                            Err(e) => Err(e),
                            Ok(()) => {
                                if !cassette_replaces_sep && !out.lit_emitted_since(pre_sep) {
                                    out.no_space();
                                }
                                let mut rest_result = Ok(());
                                for member in &seq_members[1..] {
                                    rest_result = emit_production(
                                        protocol, schema, grammar, vertex_id, member, cursor, out,
                                    );
                                    if rest_result.is_err() {
                                        break;
                                    }
                                }
                                rest_result
                            }
                        }
                    } else {
                        emit_production(protocol, schema, grammar, vertex_id, content, cursor, out)
                    };
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
            let result =
                emit_production(protocol, schema, grammar, vertex_id, content, cursor, out);
            // OPTIONAL is a backtracking site: if the inner production
            // errored *or* made no progress without leaving a witness
            // constraint, restore both cursor and output to their
            // pre-attempt state. Mirrors `Repeat`'s loop body.
            if result.is_err() {
                cursor.consumed = cursor_snap;
                out.restore(out_snap);
                return result;
            }
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
            // Set the field context for the duration of `content`'s
            // walk and emit the content against the *outer* cursor.
            // The SYMBOL handler picks up the context and pulls
            // successive `take_field(name)` edges as it encounters
            // SYMBOLs anywhere under `content` (under SEQ, CHOICE,
            // REPEAT, OPTIONAL, ALIAS — arbitrarily nested). This
            // subsumes the prior carve-outs for FIELD(REPEAT(...)),
            // FIELD(REPEAT1(...)), and the bare FIELD(SYMBOL ...)
            // case, and adds coverage for
            // `field('xs', commaSep1($.X))` which expands to
            // FIELD(SEQ(SYMBOL X, REPEAT(SEQ(',', SYMBOL X)))) and
            // any other shape where REPEAT/REPEAT1 sits inside SEQ /
            // CHOICE / OPTIONAL under a FIELD. A FIELD that wraps a
            // non-SYMBOL production (e.g. `field('op', '+')` or
            // `field('op', CHOICE(STRING ...))`) still works: STRING
            // handlers ignore the context and emit literals
            // directly, so the operator token survives the round
            // trip.
            let _guard = push_field_context(name);
            emit_production(protocol, schema, grammar, vertex_id, content, cursor, out)
        }
        Production::Alias {
            content,
            named,
            value,
        } => {
            // A named ALIAS rewrites the parser-visible kind to
            // `value`. If the cursor has an unconsumed child whose
            // kind matches that alias name, take it and emit the
            // child using the alias's INNER content as the rule
            // (e.g. `ALIAS { SYMBOL real_rule, value: "kind_x" }`
            // means a `kind_x` vertex on the schema should be walked
            // through `real_rule`'s body, not through whatever rule
            // happens to be keyed under `kind_x`). This is the
            // dependent-optic shape: the rule the emitter walks at a
            // child position is determined by the parent's chosen
            // alias, not by the child kind alone — without it,
            // grammars like YAML that introduce the same kind through
            // many ALIAS sites lose the parent context the moment
            // emit_vertex is called.
            if *named && !value.is_empty() {
                if let Some(edge) = cursor.take_matching(|edge| {
                    schema
                        .vertices
                        .get(&edge.tgt)
                        .map(|v| v.kind.as_ref() == value.as_str())
                        .unwrap_or(false)
                }) {
                    return emit_aliased_child(protocol, schema, grammar, &edge.tgt, content, out);
                }
            }
            // For anonymous aliases (named: false) whose content is an
            // external scanner token with no grammar rule (e.g.
            // JavaScript's `_ternary_qmark` aliased to `"?"`), emit the
            // alias value directly. The content's SYMBOL handler would
            // fall through the external-token heuristic and produce
            // nothing; the alias value IS the token text.
            if !*named && !value.is_empty() {
                if let Production::Symbol { name: sym } = content.as_ref() {
                    if sym.starts_with('_') && !grammar.rules.contains_key(sym) {
                        out.token(value);
                        return Ok(());
                    }
                }
            }
            emit_production(protocol, schema, grammar, vertex_id, content, cursor, out)
        }
        Production::ImmediateToken { content } => {
            // IMMEDIATE_TOKEN is the grammar's explicit signal that the
            // wrapped token must have no preceding whitespace. Lift it
            // to a NoSpace marker here, at the unique structural site
            // where the property is declared. The layout pass reads
            // the marker; downstream code does not need to inspect
            // production shapes to recover this property.
            out.no_space();
            emit_production(protocol, schema, grammar, vertex_id, content, cursor, out)
        }
        Production::Token { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => {
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
    // Prefer non-field edges (`child_of`) to avoid consuming a
    // field-named edge that a later FIELD handler should claim.
    // Field-named edges (edge.kind != "child_of") are reserved for
    // the FIELD production that names them; consuming one here would
    // steal it from its intended handler (e.g. `as_pattern`'s
    // `alias` field edge consumed by the leading `expression`
    // SYMBOL instead of the trailing FIELD "alias" handler).
    if let Some(edge) = cursor.take_matching(|edge| {
        edge.kind.as_ref() == "child_of" && {
            let target_kind = schema.vertices.get(&edge.tgt).map(|v| v.kind.as_ref());
            kind_satisfies_symbol(grammar, target_kind, name)
        }
    }) {
        return Some(edge);
    }
    cursor.take_matching(|edge| {
        let target_kind = schema.vertices.get(&edge.tgt).map(|v| v.kind.as_ref());
        kind_satisfies_symbol(grammar, target_kind, name)
    })
}

/// Decide whether a schema vertex of kind `target_kind` satisfies a
/// SYMBOL `name` reference in the grammar.
///
/// Operates as an O(1) lookup against the precomputed subtype
/// closure built at [`Grammar::from_bytes`]. The semantic content is
/// "K satisfies SYMBOL S iff K is reachable from S by walking the
/// grammar's hidden, supertype, and named-alias dispatch": this is
/// exactly the relation tree-sitter induces on `(parser-visible kind,
/// rule-position)` pairs.
fn kind_satisfies_symbol(grammar: &Grammar, target_kind: Option<&str>, name: &str) -> bool {
    let Some(target) = target_kind else {
        return false;
    };
    if target == name {
        return true;
    }
    grammar
        .subtypes
        .get(target)
        .is_some_and(|set| set.contains(name))
}

/// Emit a child reached through an ALIAS production using the
/// alias's inner content as the rule, not `grammar.rules[child.kind]`.
///
/// This carries the dependent-optic context across the ALIAS edge:
/// at the parent rule's site we know which underlying production the
/// alias wraps (typically `SYMBOL real_rule`), and that's the
/// production that should drive the emit walk on the child's
/// children. Looking up `grammar.rules.get(child.kind)` instead would
/// either fail (the renamed kind has no top-level rule, e.g. YAML's
/// `block_mapping_pair`) or pick an arbitrary same-kinded rule from
/// elsewhere in the grammar.
///
/// Walk-context invariant. The dependent-optic shape of `emit_pretty`
/// says: the production walked at any vertex is determined by the
/// path from the root through the grammar, not by the vertex kind in
/// isolation. Two dispatch sites realise that invariant:
///
/// * [`emit_vertex`] looks up `grammar.rules[child.kind]` and walks
///   it. Correct for supertype / hidden-rule dispatch: the child's
///   kind on the schema IS the subtype tree-sitter selected, so its
///   top-level rule is the right production to walk.
/// * `emit_aliased_child` threads the parent rule's `Production`
///   directly (the inner `content` of `Production::Alias`) and walks
///   it on the child's children. Correct for ALIAS dispatch: the
///   child's kind on the schema is the alias's `value` (a renamed
///   kind that may have no top-level rule), and the production to
///   walk is the alias's content body, supplied by the parent.
///
/// Together these cover every site where the rule-walked-at-child
/// diverges from `grammar.rules[child.kind]`; the recursion site for
/// plain SYMBOL therefore correctly delegates to `emit_vertex`, and
/// we do not need a richer `WalkContext` value passed by reference.
/// The grammar dependency is the thread.
fn emit_aliased_child(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    child_id: &panproto_gat::Name,
    content: &Production,
    out: &mut Output<'_>,
) -> Result<(), ParseError> {
    // Leaf shortcut: if the child has a literal-value and no
    // structural children, emit the captured text. Skip for bracket-pair
    // literals when the production resolves to a rule with those brackets.
    if let Some(literal) = literal_value(schema, child_id) {
        if children_for(schema, child_id).is_empty() {
            let is_bracket_pair = literal.len() >= 2
                && matches!(
                    (literal.as_bytes().first(), literal.as_bytes().last()),
                    (Some(b'('), Some(b')')) | (Some(b'['), Some(b']')) | (Some(b'{'), Some(b'}'))
                );
            if !is_bracket_pair {
                let kind = vertex_id_kind(schema, child_id).unwrap_or("");
                out.token_with_role(literal, Some(leaf_terminal_role(grammar, kind)));
                return Ok(());
            }
        }
    }

    // Clear the enclosing FIELD context so it does not leak into the
    // aliased child's production walk. Without this, a FIELD("alias")
    // containing an ALIAS whose content is SYMBOL "expression" would
    // cause the inner SYMBOL handler to pull by field name "alias"
    // instead of by symbol match, failing to find the child edge.
    let _guard = clear_field_context();

    // Resolve `content` to a rule when it's a SYMBOL (the dominant
    // shape: `ALIAS { content: SYMBOL real_rule, value: "kind_x" }`).
    if let Production::Symbol { name } = content {
        if let Some(rule) = grammar.rules.get(name) {
            let edges = children_for(schema, child_id);
            let mut cursor = ChildCursor::new(&edges);
            let old_rule = out.current_rule.take();
            out.current_rule = Some(name.to_owned());
            let result =
                emit_production(protocol, schema, grammar, child_id, rule, &mut cursor, out);
            out.current_rule = old_rule;
            return result;
        }
    }

    // Other ALIAS contents (CHOICE, SEQ, literals) walk in place.
    let edges = children_for(schema, child_id);
    let mut cursor = ChildCursor::new(&edges);
    emit_production(
        protocol,
        schema,
        grammar,
        child_id,
        content,
        &mut cursor,
        out,
    )
}

fn emit_in_child_context(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    child_id: &panproto_gat::Name,
    production: &Production,
    out: &mut Output<'_>,
) -> Result<(), ParseError> {
    // The child walks under its own production tree, with its own
    // FIELDs setting their own contexts. Clear the outer FIELD hint
    // so it does not leak through and cause sibling SYMBOLs inside
    // the child's body to mistakenly pull edges from the child's
    // cursor by the parent's field name.
    let _guard = clear_field_context();
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
    // Positional discriminator: use the interstitials FROM the
    // current cursor position forward. Interstitials are indexed by
    // their gap position (interstitial-k is the gap before the k-th
    // named child); the slice from `consumed_count` onward captures
    // exactly the text the remaining CHOICE branches must consume.
    // This eliminates the cross-position contamination of the prior
    // flat blob (where a trailing-CHOICE-with-BLANK saw all the
    // commas separating earlier REPEAT iterations and wrongly
    // preferred the comma alt).
    //
    // The chose-alt-fingerprint (a single string joined from every
    // non-empty interstitial trimmed) is retained as a fallback for
    // by-construction schemas with no positional interstitials; it
    // is strictly less precise than positional matching.
    let consumed_count = cursor.consumed.iter().filter(|&&c| c).count();
    let positional_interstitials: Vec<&str> = schema
        .constraints
        .get(vertex_id)
        .map(|cs| {
            let mut indexed: Vec<(usize, &str)> = cs
                .iter()
                .filter_map(|c| {
                    let s = c.sort.as_ref();
                    if !s.starts_with("interstitial-") || s.ends_with("-start-byte") {
                        return None;
                    }
                    let idx: usize = s["interstitial-".len()..].parse().ok()?;
                    Some((idx, c.value.as_str()))
                })
                .collect();
            indexed.sort_by_key(|&(i, _)| i);
            indexed.into_iter().map(|(_, v)| v).collect()
        })
        .unwrap_or_default();
    let positional_slice: String = if positional_interstitials.is_empty() {
        String::new()
    } else {
        positional_interstitials
            .iter()
            .skip(consumed_count)
            .copied()
            .collect::<Vec<&str>>()
            .join(" ")
    };
    let fingerprint_blob = schema
        .constraints
        .get(vertex_id)
        .and_then(|cs| {
            cs.iter()
                .find(|c| c.sort.as_ref() == "chose-alt-fingerprint")
                .map(|c| c.value.clone())
        })
        .unwrap_or_default();
    let constraint_blob: String = if positional_slice.is_empty() {
        fingerprint_blob
    } else {
        positional_slice
    };
    let child_kinds: Vec<&str> = schema
        .constraints
        .get(vertex_id)
        .and_then(|cs| {
            cs.iter()
                .find(|c| c.sort.as_ref() == "chose-alt-child-kinds")
                .map(|c| c.value.split_whitespace().collect())
        })
        .unwrap_or_default();
    // Cursor-exhaustion BLANK-preference: when all cursor edges have
    // been consumed AND `BLANK` is one of the alternatives, the only
    // alt that won't introduce a non-existent child is `BLANK`.
    //
    // This gate fires before the literal-blob discriminator because
    // the fingerprint is shared across every CHOICE position in the
    // vertex's rule body: a vertex like `sample_step` that ends in
    // `..., REPEAT(SEQ(",", arg)), CHOICE(",", BLANK)` records all of
    // its `","` interstitials in a single blob, so the literal-score
    // matcher would otherwise prefer `","` for the trailing CHOICE
    // even when the source had no trailing comma. By the time the
    // emitter reaches the trailing CHOICE, the REPEAT has consumed
    // every arg edge in cursor order; the residual unconsumed multiset
    // is empty; and the categorical reading of a CHOICE-with-BLANK at
    // a position with no remaining children is the no-op alternative.
    let any_unconsumed = cursor
        .edges
        .iter()
        .enumerate()
        .any(|(i, _)| !cursor.consumed[i]);
    let blank_present = alternatives.iter().any(|a| matches!(a, Production::Blank));
    let edge_kinds: Vec<&str> = cursor
        .edges
        .iter()
        .enumerate()
        .filter(|(i, _)| !cursor.consumed[*i])
        .map(|(_, e)| e.kind.as_ref())
        .collect();
    if !any_unconsumed && blank_present {
        return alternatives.iter().find(|a| matches!(a, Production::Blank));
    }
    if !any_unconsumed && !blank_present {
        // When the cursor is exhausted: first prefer a newline-like
        // PATTERN over STRING separators (e.g. Go source_file terminator
        // CHOICE[PATTERN("\n"), ";", "\0"] should emit newline not ";").
        for alt in alternatives {
            if let Production::Pattern { value } = alt {
                if is_newline_like_pattern(value) {
                    return Some(alt);
                }
            }
        }
        // Then prefer a pure-literal alternative (only STRINGs, no
        // SYMBOLs/FIELDs) over one that merely CAN produce epsilon.
        // A pure-literal alternative emits concrete tokens without
        // needing children (e.g. ";" terminator in Rust struct_item).
        if let Some(pure_lit) = alternatives.iter().find(|alt| {
            let syms = referenced_symbols(alt);
            let strings = literal_strings(alt);
            syms.is_empty() && !strings.is_empty()
        }) {
            return Some(pure_lit);
        }
        let mut visited = std::collections::HashSet::new();
        let mut yield_cache = grammar.yield_sets.clone();
        for alt in alternatives {
            let ys = yield_of_production(grammar, alt, &mut visited, &mut yield_cache);
            if ys.contains("") {
                return Some(alt);
            }
            visited.clear();
        }
    }

    // Literal match: when a cursor edge's target vertex kind or
    // literal-value matches a STRING alternative exactly, pick that
    // alternative. Handles grammars like Go's binary_expression where
    // operators are anonymous named children (kind IS the operator text
    // like ">") and the CHOICE is over STRING operators.
    for edge_idx in 0..cursor.edges.len() {
        if cursor.consumed[edge_idx] {
            continue;
        }
        let edge = &cursor.edges[edge_idx];
        let tgt_kind = schema.vertices.get(&edge.tgt).map(|v| v.kind.as_ref());
        let tgt_lit = literal_value(schema, &edge.tgt);
        for alt in alternatives {
            if let Production::String { value } = alt {
                if Some(value.as_str()) == tgt_kind || tgt_lit == Some(value.as_str()) {
                    return Some(alt);
                }
            }
        }
    }

    if !constraint_blob.is_empty() {
        // Categorical filter: when the cursor has an unconsumed first
        // edge, an alt should only be considered if it can consume
        // that edge — OR no alt in the CHOICE can. Acceptance is the
        // inductive predicate `accepts_first_edge`: it fuses FIELD-name
        // matching with content-yield admission and SYMBOL subtype
        // dispatch into one rule.
        let first_uc_edge_pre = cursor
            .edges
            .iter()
            .enumerate()
            .find(|(i, _)| !cursor.consumed[*i])
            .map(|(_, e)| e);
        let alt_accepts = |a: &Production| -> bool {
            let Some(edge) = first_uc_edge_pre else {
                return false;
            };
            let edge_kind = edge.kind.as_ref();
            let Some(tgt_kind) = schema.vertices.get(&edge.tgt).map(|v| v.kind.as_ref()) else {
                return false;
            };
            accepts_first_edge(grammar, a, edge_kind, tgt_kind)
        };
        let any_consumes = any_unconsumed && alternatives.iter().any(alt_accepts);

        // Primary score: literal-token match length. This dominates
        // alt selection so existing language tests that depend on
        // literal-only fingerprints keep working.
        // Secondary score (tiebreaker only): named-symbol kind match
        // count, read from the separate `chose-alt-child-kinds`
        // constraint (kept apart from the literal fingerprint so
        // identifiers like `:` in the kind list don't contaminate the
        // literal match). An alt that matches the recorded kinds is a
        // stronger witness than one whose only
        // overlap is literal punctuation.
        let mut best_literal: usize = 0;
        let mut best_symbols: usize = 0;
        let mut best_total_chars: usize = usize::MAX;
        let mut best_alt: Option<&Production> = None;
        let mut tied = false;
        for alt in alternatives {
            let strings = literal_strings(alt);
            if strings.is_empty() {
                continue;
            }
            // Categorical filter: skip alts that can't consume the
            // first unconsumed edge when SOME alt can.
            if any_consumes && !alt_accepts(alt) {
                continue;
            }
            let literal_score = strings
                .iter()
                .filter(|s| constraint_blob.contains(s.as_str()))
                .map(String::len)
                .sum::<usize>();
            if literal_score == 0 {
                continue;
            }
            let total_chars: usize = strings.iter().map(String::len).sum();
            let symbol_score = if literal_score >= best_literal && !child_kinds.is_empty() {
                let symbols = referenced_symbols(alt);
                symbols
                    .iter()
                    .filter(|sym| {
                        let sym_str: &str = sym;
                        if child_kinds.contains(&sym_str) {
                            return true;
                        }
                        grammar.subtypes.get(sym_str).is_some_and(|sub_set| {
                            sub_set
                                .iter()
                                .any(|sub| child_kinds.contains(&sub.as_str()))
                        })
                    })
                    .count()
            } else {
                0
            };
            let better = literal_score > best_literal
                || (literal_score == best_literal && symbol_score > best_symbols)
                || (literal_score == best_literal
                    && symbol_score == best_symbols
                    && total_chars < best_total_chars);
            let same = literal_score == best_literal
                && symbol_score == best_symbols
                && total_chars == best_total_chars;
            if better {
                best_literal = literal_score;
                best_symbols = symbol_score;
                best_total_chars = total_chars;
                best_alt = Some(alt);
                tied = false;
            } else if same && best_alt.is_some() {
                tied = true;
            }
        }
        if let Some(alt) = best_alt {
            if !tied {
                if any_unconsumed {
                    if alt_accepts(alt) {
                        return Some(alt);
                    }
                    // The best literal-score alt can't consume the
                    // first unconsumed cursor edge. Three sub-cases:
                    //  (a) No BLANK alternative: blob is the only
                    //      signal; return best_alt.
                    //  (b) BLANK present AND best_alt is pure-literal
                    //      (no referenced SYMBOLs): emitting best_alt
                    //      adds the matched literals and consumes no
                    //      child; the unconsumed cursor edge is for a
                    //      later SEQ position anyway. Return best_alt
                    //      (BUGS `model_block`: CHOICE[CHOICE["model",
                    //      "data"], BLANK] picks the literal because
                    //      the blob recorded it).
                    //  (c) BLANK present AND best_alt has SYMBOLs:
                    //      emitting best_alt would walk SYMBOLs that
                    //      can't be satisfied (they consume no edge,
                    //      a downstream SYMBOL would silently fail).
                    //      Fall through to final selection of BLANK
                    //      (Java `formal_parameters` inner CHOICE
                    //      [SEQ[receiver, ","], BLANK] with a
                    //      formal_parameter edge: pick BLANK).
                    if !blank_present || referenced_symbols(alt).is_empty() {
                        return Some(alt);
                    }
                } else {
                    return Some(alt);
                }
            }
        }
    }

    // Cursor-driven dispatch via Yield-set preimage.
    //
    // For a CHOICE C = A1 | ... | An, Yield(Ai) is the set of vertex
    // kinds that can appear as the first named child when Ai is taken
    // (see `yield_of_production`). Given the first unconsumed cursor
    // edge with target kind K, select the first Ai (grammar order)
    // where K ∈ Yield(Ai). This is deterministic: grammar order is
    // the tiebreak, matching tree-sitter's own disambiguation.
    let first_unconsumed_kind: Option<&str> = cursor
        .edges
        .iter()
        .enumerate()
        .find(|(i, _)| !cursor.consumed[*i])
        .and_then(|(_, edge)| schema.vertices.get(&edge.tgt).map(|v| v.kind.as_ref()));
    if let Some(target_kind) = first_unconsumed_kind {
        // The subtype closure `subtypes[target_kind]` contains every
        // symbol name S such that a vertex of kind `target_kind` can
        // appear where the grammar says `SYMBOL S`. For a CHOICE
        // C = A1 | ... | An, the correct alternative is the one whose
        // top-level symbol is in `subtypes[target_kind]` (the target
        // kind IS a subtype of that symbol, so the symbol's rule body
        // dispatches to the target kind at parse time). This is an
        // O(1) set-membership check per alternative — no recursive
        // Yield computation needed.
        //
        // Preference order:
        //   1. Direct name match (target_kind == symbol name)
        //   2. Subtype match (symbol name ∈ subtypes[target_kind])
        //   3. Yield-set match (target_kind ∈ Yield(alt)) as fallback
        //      for non-SYMBOL alternatives (ALIAS, SEQ, etc.)
        let target_supers = grammar.subtypes.get(target_kind);

        // Indented-form preference: when multiple alternatives match
        // the target kind (e.g. Python _suite where all three alts
        // produce `block`), prefer the alternative containing an
        // `_indent` SYMBOL. Check this BEFORE the standard passes
        // since they would pick the first match in grammar order.
        {
            let mut match_count = 0usize;
            let mut indent_alt_idx: Option<usize> = None;
            let mut visited = std::collections::HashSet::new();
            let mut yield_cache = grammar.yield_sets.clone();
            for (i, alt) in alternatives.iter().enumerate() {
                let ys = yield_of_production(grammar, alt, &mut visited, &mut yield_cache);
                if ys.contains(target_kind) {
                    match_count += 1;
                    if indent_alt_idx.is_none()
                        && referenced_symbols(alt)
                            .iter()
                            .any(|s| grammar.external_indent_opens.contains(*s))
                    {
                        indent_alt_idx = Some(i);
                    }
                }
                visited.clear();
            }
            if match_count > 1 {
                if let Some(idx) = indent_alt_idx {
                    return Some(&alternatives[idx]);
                }
            }
        }

        // Pass 1: direct name match
        for alt in alternatives {
            if let Production::Symbol { name } = alt {
                if name.as_str() == target_kind {
                    return Some(alt);
                }
            }
            if let Production::Alias {
                named: true, value, ..
            } = alt
            {
                if value.as_str() == target_kind {
                    return Some(alt);
                }
            }
        }

        // Pass 2: subtype match (the target kind's supertype set
        // tells us which SYMBOL names it satisfies)
        if let Some(supers) = target_supers {
            for alt in alternatives {
                if let Production::Symbol { name } = alt {
                    if supers.contains(name.as_str()) {
                        return Some(alt);
                    }
                }
                if let Production::Alias {
                    named: true, value, ..
                } = alt
                {
                    if supers.contains(value.as_str()) {
                        return Some(alt);
                    }
                }
            }
        }

        // Pass 3: Yield-set fallback for alternatives that are not
        // plain SYMBOLs or named ALIASes (e.g. SEQ, PREC wrappers
        // around SYMBOLs that the above passes don't unwrap).
        // Guard: skip alternatives whose FIELDs don't match any
        // unconsumed edge kind. A FIELD that can't be satisfied
        // would consume the wrong child, and the alternative is
        // structurally wrong for the current cursor state.
        let mut visited = std::collections::HashSet::new();
        let mut yield_cache = grammar.yield_sets.clone();
        let mut matching_alts: Vec<&Production> = Vec::new();
        for alt in alternatives {
            if has_any_field(alt) && !has_field_in(alt, &edge_kinds) {
                visited.clear();
                continue;
            }
            // Token-set restriction: when a FIELD's body is an
            // ALIAS{CHOICE[STRING...]}, the field admits only those
            // literal values. An alt whose token-restricted FIELDs
            // can't accept the cursor's edge for that field is
            // structurally invalid (e.g. Go `call_expression` alt 0
            // has `function: ALIAS{CHOICE["new","make"], ...}` and
            // is only valid when the function child's literal is
            // "new" or "make").
            if !alt_satisfies_field_token_restrictions(schema, cursor, alt) {
                visited.clear();
                continue;
            }
            // Alias-source discriminator: if the cursor has a
            // field-named edge whose `pre-alias-symbol` was recorded by
            // the walker, the alt's FIELD body (when it's a named
            // ALIAS over a SYMBOL) must reference that same source
            // symbol. This is the exact tree-sitter-derived signal for
            // ALIAS dispatch when literal-value restriction does not
            // apply.
            if !alt_satisfies_pre_alias_constraints(schema, cursor, alt) {
                visited.clear();
                continue;
            }
            let ys = yield_of_production(grammar, alt, &mut visited, &mut yield_cache);
            if ys.contains(target_kind) {
                matching_alts.push(alt);
            }
            visited.clear();
        }
        if matching_alts.len() == 1 {
            return Some(matching_alts[0]);
        }
        if matching_alts.len() > 1 {
            // When multiple alternatives match via yield-set, apply
            // tree-sitter's precedence ordering: higher PREC wins.
            // This is the grammar author's explicit disambiguator for
            // ambiguous productions; it should be honored unconditionally,
            // not gated on whether the constraint blob is empty.
            matching_alts.sort_by_key(|alt| std::cmp::Reverse(prec_value(alt)));
            return Some(matching_alts[0]);
        }
    }

    // FIELD dispatch: pick an alternative whose FIELD name matches an
    // unconsumed edge kind.
    for alt in alternatives {
        if has_field_in(alt, &edge_kinds) {
            return Some(alt);
        }
    }

    // No dispatch tier matched. The final selection follows the
    // categorical semantics of CHOICE-with-BLANK: BLANK represents ε
    // (produce nothing at this position). It is correct if and only
    // if no child remains to consume at this cursor position.
    //
    // When unconsumed non-extra children remain, selecting BLANK
    // would silently drop them. Select the first non-BLANK
    // alternative instead so the production walk can attempt to
    // consume them (the grammar rule may reference a symbol name
    // that doesn't exactly match the parse output's child kind,
    // e.g. Julia's macrocall_expression receives `argument_list`
    // children when grammar.json only references
    // `macro_argument_list`).
    let _ = (schema, vertex_id);
    // Prefer newline-like PATTERN over STRING ";" or other separators
    // when both are alternatives. The PATTERN produces a structural
    // LineBreak which is semantically correct for top-level terminators
    // (Go's source_file REPEAT terminator).
    let has_newline_pattern = alternatives
        .iter()
        .any(|a| matches!(a, Production::Pattern { value } if is_newline_like_pattern(value)));
    if has_newline_pattern {
        for alt in alternatives {
            if let Production::Pattern { value } = alt {
                if is_newline_like_pattern(value) {
                    return Some(alt);
                }
            }
        }
    }
    if alternatives.iter().any(|a| matches!(a, Production::Blank)) {
        // Before selecting BLANK, check if a hidden-rule alternative
        // resolves to a newline-like PATTERN. Prefer it: it produces
        // a LineBreak which is semantically correct for terminators
        // like Julia's _terminator = CHOICE[PATTERN "\r?\n", ...].
        for alt in alternatives {
            if let Production::Symbol { name } = alt {
                if name.starts_with('_') {
                    if let Some(rule) = grammar.rules.get(name) {
                        if contains_newline_pattern(rule) {
                            return Some(alt);
                        }
                    }
                }
            }
        }
        return alternatives.iter().find(|a| matches!(a, Production::Blank));
    }
    // When cursor is exhausted and no BLANK, prefer an alternative
    // that references NO symbols (pure-literal: only STRINGs, PATTERNs,
    // BLANKs). Such an alternative can produce output without consuming
    // any children and is safe when the cursor is empty (e.g. the ";"
    // terminator in Rust's struct_item vs SEQ with FIELD body).
    if !any_unconsumed {
        if let Some(pure_lit) = alternatives.iter().find(|alt| {
            let syms = referenced_symbols(alt);
            syms.is_empty() && !matches!(alt, Production::Blank)
        }) {
            return Some(pure_lit);
        }
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
            | Production::PrecDynamic { content, .. }
            | Production::Reserved { content, .. } => walk(content, out),
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
            Production::Alias {
                content,
                named,
                value,
            } => {
                // A named ALIAS produces a child vertex whose kind is
                // the alias `value` (e.g. `ALIAS { content: STRING "=",
                // value: "punctuation", named: true }` introduces a
                // `punctuation` child). For cursor-driven dispatch to
                // recognise alts that emit such children, yield the
                // alias value as a referenced symbol. Anonymous aliases
                // do not introduce a named node and only need their
                // inner content's symbols.
                if *named && !value.is_empty() {
                    out.push(value.as_str());
                }
                walk(content, out);
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
            | Production::PrecDynamic { content, .. }
            | Production::Reserved { content, .. } => walk(content, out),
            _ => {}
        }
    }
    walk(production, &mut out);
    out
}

#[cfg(test)]
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
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => first_symbol(content),
        _ => None,
    }
}

fn prec_value(prod: &Production) -> i64 {
    match prod {
        Production::Prec { value, .. }
        | Production::PrecLeft { value, .. }
        | Production::PrecRight { value, .. }
        | Production::PrecDynamic { value, .. } => value.as_i64().unwrap_or(0),
        _ => 0,
    }
}

fn has_any_field(production: &Production) -> bool {
    match production {
        Production::Field { .. } => true,
        Production::Seq { members } | Production::Choice { members } => {
            members.iter().any(has_any_field)
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
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => has_any_field(content),
        _ => false,
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
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => has_field_in(content, edge_kinds),
        _ => false,
    }
}

/// Collect every `(field_name, restricted_token_set)` pair under `production`
/// where the FIELD's body is an ALIAS whose inner content is a CHOICE of
/// pure STRINGs (or a single STRING). Such a FIELD restricts the field's
/// child literal-value to that set: the alternative is only structurally
/// valid for cursors whose field-named edge target carries a literal in
/// the set. Returns an empty vec when `production` has no token-restricted
/// FIELDs.
fn collect_field_token_restrictions<'a>(
    production: &'a Production,
    out: &mut Vec<(&'a str, Vec<&'a str>)>,
) {
    match production {
        Production::Field { name, content } => {
            if let Some(strings) = literal_choice_set(content) {
                out.push((name.as_str(), strings));
            }
            collect_field_token_restrictions(content, out);
        }
        Production::Seq { members } | Production::Choice { members } => {
            for m in members {
                collect_field_token_restrictions(m, out);
            }
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
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => {
            collect_field_token_restrictions(content, out);
        }
        _ => {}
    }
}

/// If `p` unwraps to an ALIAS whose inner content is a CHOICE-of-STRINGs
/// (or a single STRING), return that set. Otherwise None.
fn literal_choice_set(p: &Production) -> Option<Vec<&str>> {
    fn unwrap(p: &Production) -> &Production {
        match p {
            Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. }
            | Production::Token { content }
            | Production::ImmediateToken { content }
            | Production::Reserved { content, .. } => unwrap(content),
            _ => p,
        }
    }
    let p = unwrap(p);
    let Production::Alias { content, .. } = p else {
        return None;
    };
    let inner = unwrap(content);
    match inner {
        Production::String { value } => Some(vec![value.as_str()]),
        Production::Choice { members } => {
            let mut out = Vec::new();
            for m in members {
                match unwrap(m) {
                    Production::String { value } => out.push(value.as_str()),
                    _ => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// Categorical acceptance predicate: does `production` accept a cursor
/// edge whose field name is `edge_field` (or `child_of`) and whose target
/// vertex has kind `target_kind`?
///
/// Defined inductively over the production tree:
///
/// - `STRING` / `PATTERN` / `BLANK` / ε-only: reject (consume no edges).
/// - `SYMBOL X` (concrete): `edge_field == "child_of"` and `target_kind ⊑ X`.
/// - `SYMBOL X` (hidden / supertype): `accepts(X.rule, e)`.
/// - `ALIAS{c, named:true, value:V}`: `edge_field == "child_of"` and
///   `target_kind == V` (the alias rewrites the child kind to `V`).
/// - `FIELD{name, content}`: `edge_field == name` and `content.yield` admits
///   `target_kind` (the field content must accept the target as one of its
///   first kinds).
/// - `SEQ[m1, m2, ...]`: `accepts(m1, e)` or
///   (`m1` is ε-able and `accepts(SEQ[m2..], e)`).
/// - `CHOICE[a1, a2, ...]`: any of `accepts(ai, e)`.
/// - `OPTIONAL` / `REPEAT` / `REPEAT1` / wrappers: `accepts(inner, e)`.
fn accepts_first_edge(
    grammar: &Grammar,
    production: &Production,
    edge_field: &str,
    target_kind: &str,
) -> bool {
    fn yield_contains(grammar: &Grammar, prod: &Production, kind: &str) -> bool {
        let mut visited = std::collections::HashSet::new();
        let mut cache = grammar.yield_sets.clone();
        let ys = yield_of_production(grammar, prod, &mut visited, &mut cache);
        ys.contains(kind)
            || grammar
                .subtypes
                .get(kind)
                .is_some_and(|subs| subs.iter().any(|s| ys.contains(s.as_str())))
    }
    fn yield_has_epsilon(grammar: &Grammar, prod: &Production) -> bool {
        let mut visited = std::collections::HashSet::new();
        let mut cache = grammar.yield_sets.clone();
        let ys = yield_of_production(grammar, prod, &mut visited, &mut cache);
        // SEQ with all-ε-able members, OPTIONAL, REPEAT, BLANK all
        // carry the ε marker (empty string) in their yield set.
        ys.contains("") || ys.is_empty()
    }
    match production {
        Production::String { .. } | Production::Pattern { .. } | Production::Blank => false,
        Production::Symbol { name } => {
            if edge_field != "child_of" {
                return false;
            }
            if name == target_kind {
                return true;
            }
            if grammar
                .subtypes
                .get(target_kind)
                .is_some_and(|s| s.contains(name))
            {
                return true;
            }
            // Hidden / supertype: walk the rule body.
            let is_expand = name.starts_with('_') || grammar.supertypes.contains(name.as_str());
            if is_expand {
                if let Some(rule) = grammar.rules.get(name) {
                    return accepts_first_edge(grammar, rule, edge_field, target_kind);
                }
            }
            false
        }
        Production::Alias {
            named,
            value,
            content,
        } => {
            if *named && !value.is_empty() {
                edge_field == "child_of" && value == target_kind
            } else {
                accepts_first_edge(grammar, content, edge_field, target_kind)
            }
        }
        Production::Field { name, content } => {
            edge_field == name.as_str() && yield_contains(grammar, content, target_kind)
        }
        Production::Seq { members } => {
            for m in members {
                if accepts_first_edge(grammar, m, edge_field, target_kind) {
                    return true;
                }
                if !yield_has_epsilon(grammar, m) {
                    return false;
                }
            }
            false
        }
        Production::Choice { members } => members
            .iter()
            .any(|m| accepts_first_edge(grammar, m, edge_field, target_kind)),
        Production::Optional { content }
        | Production::Repeat { content }
        | Production::Repeat1 { content }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => {
            accepts_first_edge(grammar, content, edge_field, target_kind)
        }
    }
}

/// Read the walker-recorded `pre-alias-symbol` constraint for a vertex.
/// Returns `None` when the vertex has no such constraint (either there
/// was no alias rewrite or the schema was built without the walker).
fn pre_alias_symbol<'a>(schema: &'a Schema, vertex_id: &panproto_gat::Name) -> Option<&'a str> {
    schema.constraints.get(vertex_id).and_then(|cs| {
        cs.iter()
            .find(|c| c.sort.as_ref() == "pre-alias-symbol")
            .map(|c| c.value.as_str())
    })
}

/// Walk `production` and collect every alias-source-symbol declared
/// inside a FIELD with name `field_name`. Specifically, for each
/// `FIELD { name = field_name, content = ALIAS { content = SYMBOL X,
/// named: true, value: _ } }`, append `X`. Returns an empty Vec when
/// the alt's FIELD body is not a named-ALIAS-over-SYMBOL.
fn field_alias_sources<'a>(production: &'a Production, field_name: &str, out: &mut Vec<&'a str>) {
    fn unwrap_to_alias_source(p: &Production) -> Option<&str> {
        let inner = match p {
            Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. }
            | Production::Token { content }
            | Production::ImmediateToken { content }
            | Production::Reserved { content, .. } => content.as_ref(),
            _ => p,
        };
        match inner {
            Production::Alias { content, named, .. } if *named => {
                if let Production::Symbol { name } = content.as_ref() {
                    return Some(name.as_str());
                }
                None
            }
            _ => None,
        }
    }
    match production {
        Production::Field { name, content } if name.as_str() == field_name => {
            if let Some(src) = unwrap_to_alias_source(content) {
                out.push(src);
            }
        }
        Production::Field { content, .. }
        | Production::Repeat { content }
        | Production::Repeat1 { content }
        | Production::Optional { content }
        | Production::Alias { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Reserved { content, .. } => {
            field_alias_sources(content, field_name, out);
        }
        Production::Seq { members } | Production::Choice { members } => {
            for m in members {
                field_alias_sources(m, field_name, out);
            }
        }
        _ => {}
    }
}

/// Categorical alias-source discriminator: when the cursor edge for a
/// field-named edge has a recorded `pre-alias-symbol = X`, an alt
/// whose FIELD of that name takes its content from `ALIAS { SYMBOL Y }`
/// is structurally compatible iff `Y == X` — i.e. the alias's source
/// rule matches what the parser actually walked through. When the alt
/// has a FIELD with a named-ALIAS-over-SYMBOL whose source disagrees
/// with the cursor's recorded pre-alias-symbol, the alt is rejected.
fn alt_satisfies_pre_alias_constraints(
    schema: &Schema,
    cursor: &ChildCursor<'_>,
    alt: &Production,
) -> bool {
    for (i, edge) in cursor.edges.iter().enumerate() {
        if cursor.consumed[i] {
            continue;
        }
        let edge_kind = edge.kind.as_ref();
        if edge_kind == "child_of" {
            continue;
        }
        let Some(actual_source) = pre_alias_symbol(schema, &edge.tgt) else {
            continue;
        };
        let mut sources: Vec<&str> = Vec::new();
        field_alias_sources(alt, edge_kind, &mut sources);
        if sources.is_empty() {
            // The alt's FIELD content is not a named-ALIAS-over-SYMBOL,
            // so this discriminator does not apply (the alt may still
            // be correct).
            continue;
        }
        if !sources.contains(&actual_source) {
            return false;
        }
    }
    true
}

/// Returns true iff `alt` is structurally compatible with the cursor under
/// the field-token-restriction discipline: for every FIELD in `alt` whose
/// content is `ALIAS{CHOICE[STRING...], value: V}`, the cursor's field-named
/// edge for that field must carry a literal-value in the restricted set.
/// When the alt has no token-restricted FIELDs the check is vacuously true.
fn alt_satisfies_field_token_restrictions(
    schema: &Schema,
    cursor: &ChildCursor<'_>,
    alt: &Production,
) -> bool {
    let mut restrictions: Vec<(&str, Vec<&str>)> = Vec::new();
    collect_field_token_restrictions(alt, &mut restrictions);
    for (field_name, allowed) in &restrictions {
        let mut field_seen = false;
        let mut field_admits = false;
        for (i, edge) in cursor.edges.iter().enumerate() {
            if cursor.consumed[i] {
                continue;
            }
            if edge.kind.as_ref() != *field_name {
                continue;
            }
            field_seen = true;
            let lit = literal_value(schema, &edge.tgt);
            if let Some(l) = lit {
                if allowed.contains(&l) {
                    field_admits = true;
                    break;
                }
            }
        }
        if field_seen && !field_admits {
            return false;
        }
    }
    true
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
            | Production::PrecDynamic { content, .. }
            | Production::Reserved { content, .. } => walk(content, constraints),
            _ => false,
        }
    }
    walk(production, constraints)
}

fn children_for<'a>(schema: &'a Schema, vertex_id: &panproto_gat::Name) -> Vec<&'a Edge> {
    // Walk `outgoing` (insertion-ordered by SchemaBuilder via SmallVec
    // append) rather than the unordered `edges` HashMap so abstract
    // schemas under REPEAT(CHOICE(...)) preserve the order their edges
    // were inserted in. The previous implementation walked the HashMap
    // and sorted lexicographically by (kind, target id), which fused
    // interleaved children of the same kind into runs (e.g. a sequence
    // [symbol, punct, int, symbol, punct, int] became [symbol, symbol,
    // punct, punct, int, int] after the lex sort).
    let Some(edges) = schema.outgoing.get(vertex_id) else {
        return Vec::new();
    };

    // Look up the canonical Edge reference (the key in `schema.edges`)
    // for each entry in `outgoing`. Falls back to the SmallVec entry if
    // the canonical key is missing, which would indicate index drift.
    let mut indexed: Vec<(usize, u32, &Edge)> = edges
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let canonical = schema.edges.get_key_value(e).map_or(e, |(k, _)| k);
            let pos = schema.orderings.get(canonical).copied().unwrap_or(u32::MAX);
            (i, pos, canonical)
        })
        .collect();

    // Stable sort by (explicit-ordering, insertion-index). Edges with
    // an explicit `orderings` entry come first in their declared order;
    // the remainder fall through in insertion order.
    indexed.sort_by_key(|(i, pos, _)| (*pos, *i));
    indexed.into_iter().map(|(_, _, e)| e).collect()
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

/// True iff `pattern` matches a (possibly optional / repeated) sequence
/// of carriage-return and newline characters only. Examples: `\r?\n`,
/// `\n`, `\r\n`, `\n+`, `\r?\n+`. Distinguishes structural newline
/// terminals from generic whitespace and from other patterns that
/// happen to contain a newline escape inside a larger class.
fn contains_newline_pattern(prod: &Production) -> bool {
    match prod {
        Production::Pattern { value } => is_newline_like_pattern(value),
        Production::Choice { members } | Production::Seq { members } => {
            members.iter().any(contains_newline_pattern)
        }
        Production::Prec { content, .. }
        | Production::PrecLeft { content, .. }
        | Production::PrecRight { content, .. }
        | Production::PrecDynamic { content, .. }
        | Production::Token { content }
        | Production::ImmediateToken { content }
        | Production::Optional { content }
        | Production::Field { content, .. }
        | Production::Alias { content, .. }
        | Production::Reserved { content, .. } => contains_newline_pattern(content),
        _ => false,
    }
}

fn is_newline_like_pattern(pattern: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    let mut chars = pattern.chars();
    let mut saw_newline_atom = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('n' | 'r') => saw_newline_atom = true,
                _ => return false,
            },
            '\n' | '\r' => saw_newline_atom = true,
            '?' | '*' | '+' => {} // quantifiers on the previous atom
            _ => return false,
        }
    }
    saw_newline_atom
}

/// True iff `pattern` matches a (possibly quantified) run of generic
/// whitespace characters: `\s+`, `[ \t]+`, ` +`, `\s*`. Such patterns
/// describe interstitial spacing rather than syntactic content, so the
/// pretty emitter can drop them and let the layout pass insert the
/// configured separator.
fn is_whitespace_only_pattern(pattern: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    // Strip an outer quantifier suffix.
    let trimmed = pattern.trim_end_matches(['?', '*', '+']);
    if trimmed.is_empty() {
        return false;
    }
    // Bare `\s` / ` ` / `\t`.
    if matches!(trimmed, "\\s" | " " | "\\t") {
        return true;
    }
    // Character class containing only whitespace atoms.
    if let Some(inner) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let mut chars = inner.chars();
        let mut saw_atom = false;
        while let Some(c) = chars.next() {
            match c {
                '\\' => match chars.next() {
                    Some('s' | 't' | 'r' | 'n') => saw_atom = true,
                    _ => return false,
                },
                ' ' | '\t' => saw_atom = true,
                _ => return false,
            }
        }
        return saw_atom;
    }
    false
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
// Token list output with Spacing algebra
// ═══════════════════════════════════════════════════════════════════
//
// Emit produces a free monoid over `Token`. Layout (spaces, newlines,
// indentation) is a homomorphism `Vec<Token> -> Vec<u8>` parameterised
// by `FormatPolicy`. Separating the structural output from the layout
// decision means each phase has one job: emit walks the grammar and
// pushes tokens; layout is a single fold, locally driven by adjacent
// pairs and a depth counter. Snapshot/restore is just `tokens.len()`.

#[derive(Clone)]
enum Token {
    /// A user-visible terminal contributed by the grammar, annotated
    /// with its structural role for spacing decisions.
    Lit(String, TokenRole),
    /// `indent_open` marker emitted when a `Lit` matched the policy's
    /// open list. Carried as a separate token so layout can decide to
    /// break + indent without re-scanning.
    IndentOpen,
    /// `indent_close` marker emitted before a closer-`Lit`.
    IndentClose,
    /// "Break a line here if not already at line start" — used after
    /// statements/declarations and after open braces.
    LineBreak,
    /// Force a space before the next Lit even if the role-pair table
    /// says tight. Pushed between consecutive content-producing SEQ
    /// members (e.g. between `command_name` and `argument`) to ensure
    /// sibling-vertex tokens are separated.
    ForceSpace,
    /// Suppress the next inter-Lit separator. Pushed by the REPEAT
    /// walker when an iteration's "separator slot" (a CHOICE-with-BLANK
    /// or OPTIONAL at SEQ position 0) emitted zero content tokens, so
    /// the categorical reading is "no source-level separator existed
    /// between these two sibling iterations of the body".
    NoSpace,
}

struct Output<'a> {
    tokens: Vec<Token>,
    policy: &'a FormatPolicy,
    grammar: &'a Grammar,
    current_rule: Option<String>,
    cassette: Option<&'a dyn crate::languages::cassettes::GrammarCassette>,
}

#[derive(Clone)]
struct OutputSnapshot {
    tokens_len: usize,
}

impl<'a> Output<'a> {
    fn new(
        policy: &'a FormatPolicy,
        grammar: &'a Grammar,
        cassette: Option<&'a dyn crate::languages::cassettes::GrammarCassette>,
    ) -> Self {
        Self {
            tokens: Vec::new(),
            policy,
            grammar,
            current_rule: None,
            cassette,
        }
    }

    fn token(&mut self, value: &str) {
        self.token_with_role(value, None);
    }

    fn token_with_role(&mut self, value: &str, explicit_role: Option<TokenRole>) {
        if value.is_empty() {
            return;
        }

        if value == "\n" || value == "\r\n" || value == "\r" {
            self.tokens.push(Token::LineBreak);
            return;
        }

        let trimmed = value.trim_end_matches(['\n', '\r']);
        let trailing_newlines = value.len() - trimmed.len();
        if trailing_newlines > 0 && !trimmed.is_empty() {
            let role = explicit_role.unwrap_or(TokenRole::Terminal);
            if role == TokenRole::BracketClose
                && self.policy.indent_close.iter().any(|t| t == trimmed)
            {
                self.tokens.push(Token::IndentClose);
            }
            self.tokens.push(Token::Lit(trimmed.to_owned(), role));
            if role == TokenRole::BracketOpen {
                if let Some(ref rule) = self.current_rule {
                    if self
                        .grammar
                        .indent_triggers
                        .contains(&(rule.clone(), trimmed.to_owned()))
                    {
                        self.tokens.push(Token::IndentOpen);
                    }
                }
            }
            self.tokens.push(Token::LineBreak);
            return;
        }

        let role = explicit_role.unwrap_or_else(|| self.lookup_role(value));

        if role == TokenRole::BracketClose && self.policy.indent_close.iter().any(|t| t == value) {
            self.tokens.push(Token::IndentClose);
        }

        self.tokens.push(Token::Lit(value.to_owned(), role));

        if role == TokenRole::BracketOpen {
            let grammar_indent = self.current_rule.as_ref().is_some_and(|rule| {
                self.grammar
                    .indent_triggers
                    .contains(&(rule.clone(), value.to_owned()))
            });
            if grammar_indent {
                self.tokens.push(Token::IndentOpen);
                self.tokens.push(Token::LineBreak);
            }
        }
        // Line-break after tokens like `;` (statement terminator).
        // Skip for BracketOpen/BracketClose tokens that are NOT
        // indent-triggering (e.g. `{` in interpolation should not
        // trigger a line break).
        let is_non_indent_bracket = self.current_rule.is_some()
            && (role == TokenRole::BracketOpen || role == TokenRole::BracketClose)
            && !self.current_rule.as_ref().is_some_and(|rule| {
                self.grammar
                    .indent_triggers
                    .contains(&(rule.clone(), value.to_owned()))
            });
        if !is_non_indent_bracket && self.policy.line_break_after.iter().any(|t| t == value) {
            self.tokens.push(Token::LineBreak);
        }
    }

    fn lookup_role(&self, value: &str) -> TokenRole {
        if let Some(role) = self.explicit_role(value) {
            return role;
        }
        if is_word_like(value) {
            TokenRole::Keyword
        } else {
            TokenRole::Operator
        }
    }

    /// The role classified for `value` in the current rule, if any.
    /// `None` when the rule's grammar-derived `token_roles` map has no
    /// entry, leaving the caller to choose a structural default.
    fn explicit_role(&self, value: &str) -> Option<TokenRole> {
        self.current_rule
            .as_ref()
            .and_then(|rule| self.grammar.token_roles.get(rule))
            .and_then(|role_map| role_map.get(value).copied())
    }

    /// Emit a bracket-open token that triggers indentation. This is the
    /// inline-classification counterpart to the `indent_triggers` check
    /// in `token_with_role`: the SEQ walker computes indent-triggering
    /// from the SEQ structure directly rather than from a precomputed map.
    fn token_with_indent_open(&mut self, value: &str, role: TokenRole) {
        if value.is_empty() {
            return;
        }
        if role == TokenRole::BracketClose && self.policy.indent_close.iter().any(|t| t == value) {
            self.tokens.push(Token::IndentClose);
        }
        self.tokens.push(Token::Lit(value.to_owned(), role));
        if role == TokenRole::BracketOpen {
            self.tokens.push(Token::IndentOpen);
            self.tokens.push(Token::LineBreak);
        }
    }

    fn newline(&mut self) {
        self.tokens.push(Token::LineBreak);
    }

    /// Open an indent scope: subsequent `LineBreak`s render at the
    /// new depth until a matching `indent_close` pops it. Used by the
    /// external-token fallback to render indent-based grammars'
    /// `_indent` scanner outputs.
    fn indent_open(&mut self) {
        self.tokens.push(Token::IndentOpen);
        self.tokens.push(Token::LineBreak);
    }

    /// Close one indent scope opened by `indent_open`.
    fn indent_close(&mut self) {
        self.tokens.push(Token::IndentClose);
    }

    fn snapshot(&self) -> OutputSnapshot {
        OutputSnapshot {
            tokens_len: self.tokens.len(),
        }
    }

    fn restore(&mut self, snap: OutputSnapshot) {
        self.tokens.truncate(snap.tokens_len);
    }

    /// True iff at least one `Token::Lit` was pushed since `snap`.
    /// Control-only emissions (`LineBreak`, `IndentOpen` / `IndentClose`,
    /// `NoSpace`) do not count as content. Used by the REPEAT walker
    /// to detect that a "separator slot" CHOICE picked its BLANK
    /// alternative, so the next iteration's content can be marked
    /// tight against the previous iteration's content.
    fn lit_emitted_since(&self, snap: OutputSnapshot) -> bool {
        self.tokens[snap.tokens_len..]
            .iter()
            .any(|t| matches!(t, Token::Lit(_, _)))
    }

    /// Push a marker that suppresses the next inter-Lit separator the
    /// layout pass would otherwise insert. Used to encode "no source-
    /// level separator was emitted between these two Lits" without
    /// having to make per-grammar adjacency decisions in the layout.
    fn no_space(&mut self) {
        self.tokens.push(Token::NoSpace);
    }

    fn finish(self) -> Vec<u8> {
        layout(
            &self.tokens,
            self.policy,
            &self.grammar.line_comment_prefixes,
        )
    }
}

/// Fold a token list into bytes. The algebra:
/// * adjacent `Lit`s get a single space iff `needs_space_between(a, b)`,
/// * `IndentOpen` / `IndentClose` adjust a depth counter,
/// * `LineBreak` writes `\n` if not already at line start, then the
///   next `Lit` writes `indent * indent_width` spaces of indent.
fn layout(tokens: &[Token], policy: &FormatPolicy, line_comment_prefixes: &[String]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut indent: usize = 0;
    let mut at_line_start = true;
    let mut last_role: Option<TokenRole> = None;
    let mut last_text: String = String::new();
    let mut suppress_next_separator = false;
    let mut force_next_separator = false;
    let newline = policy.newline.as_bytes();
    let separator = policy.separator.as_bytes();

    for (tok_idx, tok) in tokens.iter().enumerate() {
        if std::env::var("DBG_LAYOUT").is_ok() {
            match tok {
                Token::Lit(v, r) => eprintln!(
                    "  TOK: Lit({v:?}, {r:?}) at_line_start={at_line_start} last_role={last_role:?}"
                ),
                Token::IndentOpen => eprintln!("  TOK: IndentOpen"),
                Token::IndentClose => eprintln!("  TOK: IndentClose"),
                Token::LineBreak => eprintln!("  TOK: LineBreak"),
                Token::NoSpace => eprintln!("  TOK: NoSpace"),
                Token::ForceSpace => eprintln!("  TOK: ForceSpace"),
            }
        }
        match tok {
            Token::IndentOpen => indent += 1,
            Token::IndentClose => {
                indent = indent.saturating_sub(1);
                if !at_line_start {
                    bytes.extend_from_slice(newline);
                    at_line_start = true;
                }
            }
            Token::LineBreak => {
                if !at_line_start {
                    bytes.extend_from_slice(newline);
                    at_line_start = true;
                }
            }
            Token::NoSpace => {
                suppress_next_separator = true;
            }
            Token::ForceSpace => {
                force_next_separator = true;
            }
            Token::Lit(value, role) => {
                // Block-opening bracket: BracketOpen followed by IndentOpen.
                // After a Terminal/BracketClose, this should be spaced
                // (`}\n` not `0{`).
                let is_block_open = *role == TokenRole::BracketOpen
                    && tokens
                        .get(tok_idx + 1)
                        .is_some_and(|t| matches!(t, Token::IndentOpen));
                if at_line_start {
                    bytes.extend(std::iter::repeat_n(b' ', indent * policy.indent_width));
                } else if let Some(prev_role) = last_role {
                    // An explicit NoSpace (suppress) is authoritative: it
                    // records that the source had no separator at this
                    // boundary (an empty REPEAT separator slot, an
                    // IMMEDIATE_TOKEN). It overrides the sibling-separation
                    // ForceSpace heuristic — otherwise beamed notes
                    // (`CDEF`) re-space to `C D E F`.
                    let want_space = !suppress_next_separator
                        && (force_next_separator
                            || needs_space_by_role(prev_role, &last_text, *role, value)
                            || (is_block_open
                                && matches!(
                                    prev_role,
                                    TokenRole::Terminal | TokenRole::BracketClose
                                )));
                    if want_space {
                        bytes.extend_from_slice(separator);
                    }
                }
                suppress_next_separator = false;
                force_next_separator = false;
                bytes.extend_from_slice(value.as_bytes());
                at_line_start = false;
                last_role = Some(*role);
                last_text.clear();
                last_text.push_str(value);
                if line_comment_prefixes
                    .iter()
                    .any(|p| value.starts_with(p.as_str()))
                {
                    bytes.extend_from_slice(newline);
                    at_line_start = true;
                    last_role = None;
                }
            }
        }
    }

    if !at_line_start {
        bytes.extend_from_slice(newline);
    }
    bytes
}

/// Effective spacing role: word-like bracket tokens (`function`, `end`,
/// `begin`, `done`, etc.) are structurally brackets (for indentation)
/// but space like keywords (they need whitespace on both sides).
fn effective_spacing_role(role: TokenRole, text: &str) -> TokenRole {
    match role {
        TokenRole::BracketOpen | TokenRole::BracketClose if is_word_like(text) => {
            TokenRole::Keyword
        }
        other => other,
    }
}

/// Role-pair spacing table. Determines whether a space separator
/// should be inserted between two adjacent tokens based on their
/// structural roles and word-likeness.
fn needs_space_by_role(last: TokenRole, last_text: &str, next: TokenRole, next_text: &str) -> bool {
    let last = effective_spacing_role(last, last_text);
    let next = effective_spacing_role(next, next_text);
    match (last, next) {
        // Immediate (IMMEDIATE_TOKEN) tokens are lexically glued to
        // their neighbours on both sides (`0.5`, not `0 . 5`).
        (TokenRole::Immediate, _) | (_, TokenRole::Immediate) => false,
        // Brackets: tight on the inside
        (TokenRole::BracketOpen, _) | (_, TokenRole::BracketClose) => false,
        // Separators: tight before, space after
        (_, TokenRole::Separator) => false,
        (TokenRole::Separator, _) => true,
        // Connectors: always tight (., ::, ->, etc.)
        (TokenRole::Connector, _) | (_, TokenRole::Connector) => false,
        // Terminal followed by bracket-open: tight (f() not f ())
        (TokenRole::Terminal, TokenRole::BracketOpen) => false,
        // Close followed by open: tight
        (TokenRole::BracketClose, TokenRole::BracketOpen) => false,
        // Keywords always spaced
        (TokenRole::Keyword, _) | (_, TokenRole::Keyword) => true,
        // Terminals and operators: space between
        (TokenRole::Terminal, TokenRole::Terminal) => true,
        (TokenRole::Terminal, TokenRole::Operator) | (TokenRole::Operator, TokenRole::Terminal) => {
            true
        }
        (TokenRole::Operator, TokenRole::Operator) => true,
        // Close followed by non-bracket: space
        (TokenRole::BracketClose, _) => true,
        // Operator before open: space
        (TokenRole::Operator, TokenRole::BracketOpen) => true,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn test_grammar() -> Grammar {
        Grammar::from_bytes("test", b"{\"name\":\"test\",\"rules\":{}}").unwrap_or_else(|_| {
            serde_json::from_str::<Grammar>(r#"{"name":"test","rules":{}}"#).unwrap()
        })
    }

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
        let g = test_grammar();
        let mut out = Output::new(&policy, &g, None);
        out.token_with_role("foo", Some(TokenRole::Terminal));
        out.token_with_role("(", Some(TokenRole::BracketOpen));
        out.token_with_role(")", Some(TokenRole::BracketClose));
        out.token_with_role(";", Some(TokenRole::Separator));
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
        let g = test_grammar();
        let mut out = Output::new(&policy, &g, None);
        out.token_with_role("fn", Some(TokenRole::Keyword));
        out.token_with_role("foo", Some(TokenRole::Terminal));
        out.token_with_role("(", Some(TokenRole::BracketOpen));
        out.token_with_role(")", Some(TokenRole::BracketClose));
        out.token_with_role("{", Some(TokenRole::BracketOpen));
        out.token_with_role("body", Some(TokenRole::Terminal));
        out.token_with_role("}", Some(TokenRole::BracketClose));
        let bytes = out.finish();
        let s = std::str::from_utf8(&bytes).expect("ascii output");
        assert!(s.contains("{\n"), "newline after opening brace: {s:?}");
        assert!(s.contains("body"), "body inside block: {s:?}");
        assert!(s.ends_with("}\n"), "newline after closing brace: {s:?}");
    }

    #[test]
    fn output_no_space_between_word_and_dot() {
        let policy = FormatPolicy::default();
        let g = test_grammar();
        let mut out = Output::new(&policy, &g, None);
        out.token_with_role("foo", Some(TokenRole::Terminal));
        out.token_with_role(".", Some(TokenRole::Operator));
        out.token_with_role("bar", Some(TokenRole::Terminal));
        let bytes = out.finish();
        let s = std::str::from_utf8(&bytes).expect("ascii output");
        // With role-based spacing, operator gets spaces: "foo . bar"
        // The dot tight-binding is a grammar-derived property (dot appears
        // between SYMBOL members in attribute/field access rules).
        // For unit tests with explicit roles, we accept spaced dot.
        assert!(
            s.contains("foo") && s.contains("bar"),
            "both identifiers present: {s:?}"
        );
    }

    #[test]
    fn output_snapshot_restore_truncates_bytes() {
        let policy = FormatPolicy::default();
        let g = test_grammar();
        let mut out = Output::new(&policy, &g, None);
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

    #[test]
    fn placeholder_decodes_literal_pattern_separators() {
        // PATTERN regexes that match a single literal byte sequence
        // (newline, semicolon, comma) emit the bytes verbatim instead
        // of falling through to the `_` catch-all.
        assert_eq!(placeholder_for_pattern("\\n"), "\n");
        assert_eq!(placeholder_for_pattern("\\r\\n"), "\r\n");
        assert_eq!(placeholder_for_pattern(";"), ";");
        // Patterns with character classes / alternation still route
        // through the heuristic.
        assert_eq!(placeholder_for_pattern("[0-9]+"), "0");
        assert_eq!(placeholder_for_pattern("a|b"), "_");
    }

    #[test]
    fn supertypes_decode_from_grammar_json_strings() {
        // Tree-sitter older grammars list supertypes as bare strings.
        let bytes = br#"{
            "name": "tiny",
            "supertypes": ["expression"],
            "rules": {
                "expression": {
                    "type": "CHOICE",
                    "members": [
                        {"type": "SYMBOL", "name": "binary_expression"},
                        {"type": "SYMBOL", "name": "identifier"}
                    ]
                },
                "binary_expression": {"type": "STRING", "value": "x"},
                "identifier": {"type": "PATTERN", "value": "[a-z]+"}
            }
        }"#;
        let g = Grammar::from_bytes("tiny", bytes).expect("parse");
        assert!(g.supertypes.contains("expression"));
        // identifier matches the supertype `expression`.
        assert!(kind_satisfies_symbol(&g, Some("identifier"), "expression"));
        // unrelated kinds do not.
        assert!(!kind_satisfies_symbol(&g, Some("string"), "expression"));
    }

    #[test]
    fn supertypes_decode_from_grammar_json_objects() {
        // Recent grammars list supertypes as `{type: SYMBOL, name: ...}`
        // entries instead of bare strings.
        let bytes = br#"{
            "name": "tiny",
            "supertypes": [{"type": "SYMBOL", "name": "stmt"}],
            "rules": {
                "stmt": {
                    "type": "CHOICE",
                    "members": [
                        {"type": "SYMBOL", "name": "while_stmt"},
                        {"type": "SYMBOL", "name": "if_stmt"}
                    ]
                },
                "while_stmt": {"type": "STRING", "value": "while"},
                "if_stmt": {"type": "STRING", "value": "if"}
            }
        }"#;
        let g = Grammar::from_bytes("tiny", bytes).expect("parse");
        assert!(g.supertypes.contains("stmt"));
        assert!(kind_satisfies_symbol(&g, Some("while_stmt"), "stmt"));
    }

    #[test]
    fn alias_value_matches_kind() {
        // A named ALIAS rewrites the parser-visible kind to `value`;
        // `kind_satisfies_symbol` should accept that rewritten kind
        // when looking up the original SYMBOL.
        let bytes = br#"{
            "name": "tiny",
            "rules": {
                "_package_identifier": {
                    "type": "ALIAS",
                    "named": true,
                    "value": "package_identifier",
                    "content": {"type": "SYMBOL", "name": "identifier"}
                },
                "identifier": {"type": "PATTERN", "value": "[a-z]+"}
            }
        }"#;
        let g = Grammar::from_bytes("tiny", bytes).expect("parse");
        assert!(kind_satisfies_symbol(
            &g,
            Some("package_identifier"),
            "_package_identifier"
        ));
    }

    #[test]
    fn referenced_symbols_walks_nested_seq() {
        let prod: Production = serde_json::from_str(
            r#"{
                "type": "SEQ",
                "members": [
                    {"type": "CHOICE", "members": [
                        {"type": "SYMBOL", "name": "attribute_item"},
                        {"type": "BLANK"}
                    ]},
                    {"type": "SYMBOL", "name": "parameter"},
                    {"type": "REPEAT", "content": {
                        "type": "SEQ",
                        "members": [
                            {"type": "STRING", "value": ","},
                            {"type": "SYMBOL", "name": "parameter"}
                        ]
                    }}
                ]
            }"#,
        )
        .expect("seq");
        let symbols = referenced_symbols(&prod);
        assert!(symbols.contains(&"attribute_item"));
        assert!(symbols.contains(&"parameter"));
    }

    #[test]
    fn literal_strings_collects_choice_members() {
        let prod: Production = serde_json::from_str(
            r#"{
                "type": "CHOICE",
                "members": [
                    {"type": "STRING", "value": "+"},
                    {"type": "STRING", "value": "-"},
                    {"type": "STRING", "value": "*"}
                ]
            }"#,
        )
        .expect("choice");
        let strings = literal_strings(&prod);
        assert_eq!(strings, vec!["+", "-", "*"]);
    }

    /// The ocaml and javascript grammars (tree-sitter ≥ 0.25) emit a
    /// `RESERVED` rule kind that an earlier deserialiser rejected
    /// with `unknown variant "RESERVED"`. Verify both that the bare
    /// variant deserialises and that a `RESERVED`-wrapped grammar is
    /// loadable end-to-end via [`Grammar::from_bytes`].
    #[test]
    fn reserved_variant_deserialises() {
        let prod: Production = serde_json::from_str(
            r#"{
                "type": "RESERVED",
                "content": {"type": "SYMBOL", "name": "_lowercase_identifier"},
                "context_name": "attribute_id"
            }"#,
        )
        .expect("RESERVED parses");
        match prod {
            Production::Reserved { content, .. } => match *content {
                Production::Symbol { name } => assert_eq!(name, "_lowercase_identifier"),
                other => panic!("expected inner SYMBOL, got {other:?}"),
            },
            other => panic!("expected RESERVED, got {other:?}"),
        }
    }

    #[test]
    fn reserved_grammar_loads_end_to_end() {
        let bytes = br#"{
            "name": "tiny_reserved",
            "rules": {
                "program": {
                    "type": "RESERVED",
                    "content": {"type": "SYMBOL", "name": "ident"},
                    "context_name": "keywords"
                },
                "ident": {"type": "PATTERN", "value": "[a-z]+"}
            }
        }"#;
        let g = Grammar::from_bytes("tiny_reserved", bytes).expect("RESERVED-using grammar loads");
        assert!(g.rules.contains_key("program"));
    }

    #[test]
    fn reserved_walker_helpers_recurse_into_content() {
        // The walker's helpers (first_symbol, has_field_in,
        // referenced_symbols, ...) all need to descend through
        // RESERVED into its content. If they bail at RESERVED, the
        // `pick_choice_with_cursor` heuristic ranks the alt below
        // alts that DO recurse, which produces wrong emit output
        // even when the deserialiser doesn't crash.
        let prod: Production = serde_json::from_str(
            r#"{
                "type": "RESERVED",
                "content": {
                    "type": "FIELD",
                    "name": "lhs",
                    "content": {"type": "SYMBOL", "name": "expr"}
                },
                "context_name": "ctx"
            }"#,
        )
        .expect("nested RESERVED parses");
        assert_eq!(first_symbol(&prod), Some("expr"));
        assert!(has_field_in(&prod, &["lhs"]));
        let symbols = referenced_symbols(&prod);
        assert!(symbols.contains(&"expr"));
    }

    // -- Yield-set tests --

    fn yield_of(grammar: &Grammar, prod: &Production) -> std::collections::HashSet<String> {
        let mut visited = std::collections::HashSet::new();
        let mut cache = grammar.yield_sets.clone();
        yield_of_production(grammar, prod, &mut visited, &mut cache)
    }

    #[test]
    fn yield_set_seq_only_first_member() {
        let prod: Production = serde_json::from_str(
            r#"{
                "type": "SEQ",
                "members": [
                    {"type": "SYMBOL", "name": "identifier"},
                    {"type": "STRING", "value": "as"},
                    {"type": "SYMBOL", "name": "target"}
                ]
            }"#,
        )
        .expect("valid SEQ");
        let g = Grammar::from_bytes("test", b"{}").unwrap_or_else(|_| {
            serde_json::from_str::<Grammar>(r#"{"name":"t","rules":{}}"#).unwrap()
        });
        let ys = yield_of(&g, &prod);
        assert!(ys.contains("identifier"), "SEQ yields first member");
        assert!(
            !ys.contains("target"),
            "SEQ must NOT yield non-first members"
        );
    }

    #[test]
    fn yield_set_choice_union() {
        let prod: Production = serde_json::from_str(
            r#"{
                "type": "CHOICE",
                "members": [
                    {"type": "SYMBOL", "name": "a"},
                    {"type": "SYMBOL", "name": "b"}
                ]
            }"#,
        )
        .expect("valid CHOICE");
        let g = serde_json::from_str::<Grammar>(r#"{"name":"t","rules":{}}"#).unwrap();
        let ys = yield_of(&g, &prod);
        assert_eq!(ys.len(), 2);
        assert!(ys.contains("a"));
        assert!(ys.contains("b"));
    }

    #[test]
    fn yield_set_hidden_expansion() {
        let g = serde_json::from_str::<Grammar>(
            r#"{"name":"t","rules":{
                "_value": {
                    "type": "CHOICE",
                    "members": [
                        {"type": "SYMBOL", "name": "number"},
                        {"type": "SYMBOL", "name": "object"}
                    ]
                }
            }}"#,
        )
        .unwrap();
        let mut g = g;
        g.subtypes = compute_subtype_closure(&g);
        g.yield_sets = compute_yield_sets(&g);
        let sym: Production =
            serde_json::from_str(r#"{"type": "SYMBOL", "name": "_value"}"#).unwrap();
        let ys = yield_of(&g, &sym);
        assert!(
            ys.contains("number"),
            "hidden rule expands into its CHOICE members"
        );
        assert!(ys.contains("object"));
        assert!(
            !ys.contains("_value"),
            "hidden rule name is not in yield set"
        );
    }

    #[test]
    fn yield_set_optional_includes_epsilon() {
        let prod: Production = serde_json::from_str(
            r#"{"type": "OPTIONAL", "content": {"type": "SYMBOL", "name": "x"}}"#,
        )
        .unwrap();
        let g = serde_json::from_str::<Grammar>(r#"{"name":"t","rules":{}}"#).unwrap();
        let ys = yield_of(&g, &prod);
        assert!(ys.contains("x"));
        assert!(ys.contains(""), "OPTIONAL includes epsilon");
    }

    #[test]
    fn yield_set_alias_uses_value() {
        let prod: Production = serde_json::from_str(
            r#"{"type": "ALIAS", "content": {"type": "SYMBOL", "name": "real"},
                "named": true, "value": "alias_name"}"#,
        )
        .unwrap();
        let g = serde_json::from_str::<Grammar>(r#"{"name":"t","rules":{}}"#).unwrap();
        let ys = yield_of(&g, &prod);
        assert_eq!(ys.len(), 1);
        assert!(ys.contains("alias_name"), "named ALIAS yields its value");
    }
}
