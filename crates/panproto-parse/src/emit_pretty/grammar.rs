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

//! `emit_pretty::grammar` (Phase A decomposition).

use super::{Deserialize, BTreeMap, ParseError, kind_satisfies_symbol, unwrap_to_string, is_word_like, has_repeat_recursive, leading_optional_sign, is_prefix_sigil, matching_close_bracket, is_immediate_token, extract_line_comment_prefix, collect_all_symbol_refs, unwrap_to_seq, external_symbol_name, referenced_symbols, terminal_pattern_of, pattern_absorbs_leading_space};


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
    /// Named terminal kinds whose underlying `PATTERN` can match a leading
    /// space (e.g. INI's `setting_value = PATTERN ".+"`). A layout space
    /// emitted *before* such a terminal would fold into its captured text
    /// on re-parse and accrete one space per emit, so the emitter hugs them
    /// to their predecessor. See [`pattern_absorbs_leading_space`].
    #[serde(skip)]
    pub leading_space_terminals: std::collections::HashSet<String>,
}


pub(crate) fn deserialize_supertypes<'de, D>(
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


pub(crate) fn deserialize_extras<'de, D>(
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
        grammar.leading_space_terminals = classify_leading_space_terminals(&grammar);
        grammar.yield_sets = compute_yield_sets(&grammar);
        Ok(grammar)
    }
}


/// Compute the subtyping relation as a forward-indexed map from a
/// SYMBOL name to the set of vertex kinds that satisfy that SYMBOL.
pub(crate) fn compute_subtype_closure(
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
            Production::Choice { members } => {
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
pub(crate) fn compute_yield_sets(
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
pub(crate) fn yield_of_production(
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
pub(crate) type NodeTypeResult = (
    std::collections::HashMap<String, std::collections::HashSet<String>>,
    std::collections::HashMap<
        String,
        std::collections::HashMap<String, std::collections::HashSet<String>>,
    >,
    std::collections::HashMap<String, std::collections::HashSet<String>>,
);


pub(crate) fn build_node_type_children(nt_bytes: &[u8]) -> NodeTypeResult {
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
pub(crate) fn augment_subtypes_from_node_types(grammar: &mut Grammar) {
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
pub(crate) fn collect_field_symbols(
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


pub(crate) fn collect_symbols_flat(prod: &Production, out: &mut Vec<String>) {
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
pub(crate) fn build_external_alias_map(grammar: &Grammar) -> std::collections::HashMap<String, String> {
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
pub(crate) fn build_named_alias_map(grammar: &Grammar) -> std::collections::HashMap<String, String> {
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
pub(crate) type RoleMap = std::collections::HashMap<String, std::collections::HashMap<String, TokenRole>>;

pub(crate) type IndentSet = std::collections::HashSet<(String, String)>;


pub(crate) fn compute_token_roles(grammar: &Grammar) -> (RoleMap, IndentSet) {
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
pub(crate) fn classify_production(
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
pub(crate) fn classify_seq(
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
pub(crate) fn classify_repeat_body(
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
pub(crate) fn classify_seq_positions(members: &[Production], in_choice: bool) -> Vec<Option<TokenRole>> {
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


/// Extract line-comment prefixes from the grammar's extras rules.
///
/// A line comment is identified by: the rule name is in
/// `grammar.extras` AND the rule body structurally matches
/// `TOKEN(SEQ [STRING prefix, PATTERN ...])` where the PATTERN
/// matches to end-of-line.
pub(crate) fn extract_line_comment_prefixes(grammar: &Grammar) -> Vec<String> {
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


// ═══════════════════════════════════════════════════════════════════
// Format policy
/// Classify external scanner tokens as layout actions based on
/// tree-sitter naming conventions. These conventions are ecosystem-
/// wide, not language-specific: every indent-based grammar uses
/// `_indent`/`_dedent`, every ASI grammar uses `_automatic_semicolon`.
pub(crate) fn classify_external_layout_tokens(grammar: &mut Grammar) {
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
pub(crate) fn classify_external_bracket_delimiters(grammar: &mut Grammar) {
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
pub(crate) fn classify_synthetic_indent_rules(grammar: &mut Grammar) {
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


/// Collect named terminal kinds whose underlying `PATTERN` can match a
/// leading space (see [`pattern_absorbs_leading_space`]). Two shapes
/// produce such a kind on the schema:
///
/// - `ALIAS { content: PATTERN p, named: true, value: K }` — the pattern is
///   renamed to kind `K` (INI's `setting_value`, `setting_name`, …).
/// - a named rule `K = PATTERN p` — the rule itself is a bare terminal.
///
/// In both cases the captured text round-trips through `K`, so a layout
/// space emitted before it would be absorbed and grow. The pattern is read
/// through token/precedence wrappers via [`terminal_pattern_of`].
pub(crate) fn classify_leading_space_terminals(grammar: &Grammar) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();

    // Named rules that are themselves a bare terminal pattern.
    for (name, rule) in &grammar.rules {
        if let Some(p) = terminal_pattern_of(rule) {
            if pattern_absorbs_leading_space(p) {
                out.insert(name.clone());
            }
        }
    }

    // Named aliases wrapping a terminal pattern.
    fn walk(prod: &Production, out: &mut std::collections::HashSet<String>) {
        match prod {
            Production::Alias {
                content,
                named: true,
                value,
            } => {
                if let Some(p) = terminal_pattern_of(content) {
                    if pattern_absorbs_leading_space(p) {
                        out.insert(value.clone());
                    }
                }
                walk(content, out);
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
            | Production::Reserved { content, .. } => walk(content, out),
            Production::Seq { members } | Production::Choice { members } => {
                for m in members {
                    walk(m, out);
                }
            }
            _ => {}
        }
    }
    for rule in grammar.rules.values() {
        walk(rule, &mut out);
    }
    out
}
