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
    /// Anonymous ALIAS values for external scanner tokens. Maps external
    /// symbol name (e.g. `_ternary_qmark`) to the ALIAS value string
    /// (e.g. `"?"`). Built by scanning grammar.json rule bodies for
    /// `ALIAS { content: SYMBOL S, named: false, value: V }` where S
    /// has no grammar rule.
    #[serde(skip)]
    pub external_alias_map: std::collections::HashMap<String, String>,
    /// Rules whose `{`/`}` STRING tokens are inline delimiters (e.g.
    /// string interpolation) rather than block scopes. Identified
    /// structurally: a rule whose SEQ contains `{` and `}` but no
    /// REPEAT/REPEAT1 between them.
    #[serde(skip)]
    pub inline_brace_rules: std::collections::HashSet<String>,
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
        if let Some(nt_bytes) = node_types_bytes {
            grammar.node_type_children = build_node_type_children(nt_bytes);
            augment_subtypes_from_node_types(&mut grammar);
        }
        grammar.external_alias_map = build_external_alias_map(&grammar);
        grammar.inline_brace_rules = identify_inline_brace_rules(&grammar);
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

    // Transitive close: `K ⊑ A` and `A ⊑ B` implies `K ⊑ B`. Iterate
    // a few rounds; the relation is small so a quick fixed-point
    // suffices in practice.
    for _ in 0..8 {
        let snapshot = subtypes.clone();
        let mut changed = false;
        for (kind, supers) in &snapshot {
            let extra: HashSet<String> = supers
                .iter()
                .flat_map(|s| snapshot.get(s).cloned().unwrap_or_default())
                .collect();
            let entry = subtypes.entry(kind.clone()).or_default();
            for s in extra {
                if entry.insert(s) {
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
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
                // Walk the SEQ members left-to-right, returning the
                // Yield of the first member that can produce a named
                // child. STRING and PATTERN yield ∅ (anonymous
                // tokens); skip them to reach the first named-child-
                // producing position.  This handles hidden rules like
                // `_initializer = SEQ ["=", FIELD { value, ... }]`
                // where the leading "=" is a STRING.
                for m in members {
                    let ys = yield_of_production(grammar, m, visited, cache);
                    if !ys.is_empty() {
                        return ys;
                    }
                }
                std::collections::HashSet::new()
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
        Production::Repeat { content }
        | Production::Repeat1 { content }
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
fn build_node_type_children(
    nt_bytes: &[u8],
) -> std::collections::HashMap<String, std::collections::HashSet<String>> {
    let node_types: Vec<crate::theory_extract::NodeType> = match serde_json::from_slice(nt_bytes) {
        Ok(v) => v,
        Err(_) => return std::collections::HashMap::new(),
    };
    let mut map: std::collections::HashMap<String, std::collections::HashSet<String>> =
        std::collections::HashMap::new();
    for entry in &node_types {
        if !entry.named {
            continue;
        }
        let mut child_kinds = std::collections::HashSet::new();
        for field_value in entry.fields.values() {
            if let Some(types) = field_value.get("types").and_then(|t| t.as_array()) {
                for t in types {
                    if let (Some(name), Some(true)) = (
                        t.get("type").and_then(|n| n.as_str()),
                        t.get("named").and_then(serde_json::Value::as_bool),
                    ) {
                        child_kinds.insert(name.to_owned());
                    }
                }
            }
        }
        if let Some(ref children) = entry.children {
            for t in &children.types {
                if t.named {
                    child_kinds.insert(t.node_type.clone());
                }
            }
        }
        if !child_kinds.is_empty() {
            map.insert(entry.node_type.clone(), child_kinds);
        }
    }
    map
}

/// Augment `grammar.subtypes` with child-kind data from node-types.json.
///
/// For each parent kind P with node-type children, for each SYMBOL S
/// referenced in P's grammar rule, for each child kind C in
/// `node_type_children[P]`: if C does not already satisfy S, record
/// C satisfies S. This closes the grammar/parser divergence where
/// tree-sitter's parser produces child kinds not reachable from
/// grammar.json's production rules.
fn augment_subtypes_from_node_types(grammar: &mut Grammar) {
    let pairs: Vec<(String, String)> = grammar
        .node_type_children
        .iter()
        .flat_map(|(parent_kind, allowed_children)| {
            let symbols: Vec<&str> = grammar
                .rules
                .get(parent_kind)
                .map(|rule| referenced_symbols(rule))
                .unwrap_or_default();
            let mut out = Vec::new();
            for child_kind in allowed_children {
                let already_satisfies_some = symbols
                    .iter()
                    .any(|s| kind_satisfies_symbol(grammar, Some(child_kind), s));
                if already_satisfies_some {
                    continue;
                }
                for sym_name in &symbols {
                    out.push((child_kind.clone(), (*sym_name).to_owned()));
                }
            }
            out
        })
        .collect();
    for (child_kind, sym_name) in pairs {
        grammar
            .subtypes
            .entry(child_kind)
            .or_default()
            .insert(sym_name);
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

/// Identify rules whose `{`/`}` tokens are inline delimiters (e.g.
/// interpolation) rather than block scopes. A rule is inline-brace
/// iff its production SEQ contains both an opening brace token and
/// `}`, and the members between them contain no REPEAT/REPEAT1
/// (which would indicate a statement-list block).
fn identify_inline_brace_rules(grammar: &Grammar) -> std::collections::HashSet<String> {
    fn is_inline_brace_body(prod: &Production) -> bool {
        match prod {
            Production::Seq { members } => {
                let open_idx = members.iter().position(|m| match m {
                    Production::String { value } => value.contains('{'),
                    _ => false,
                });
                let close_idx = members
                    .iter()
                    .rposition(|m| matches!(m, Production::String { value } if value == "}"));
                if let (Some(open), Some(close)) = (open_idx, close_idx) {
                    if open < close {
                        let between = &members[open + 1..close];
                        return !between.iter().any(has_repeat);
                    }
                }
                false
            }
            Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. }
            | Production::Token { content }
            | Production::ImmediateToken { content }
            | Production::Reserved { content, .. } => is_inline_brace_body(content),
            _ => false,
        }
    }
    fn has_repeat(prod: &Production) -> bool {
        match prod {
            Production::Repeat { .. } | Production::Repeat1 { .. } => true,
            Production::Choice { members } | Production::Seq { members } => {
                members.iter().any(has_repeat)
            }
            Production::Prec { content, .. }
            | Production::PrecLeft { content, .. }
            | Production::PrecRight { content, .. }
            | Production::PrecDynamic { content, .. }
            | Production::Optional { content }
            | Production::Field { content, .. }
            | Production::Token { content }
            | Production::ImmediateToken { content }
            | Production::Reserved { content, .. } => has_repeat(content),
            _ => false,
        }
    }
    let mut result = std::collections::HashSet::new();
    for (name, rule) in &grammar.rules {
        if is_inline_brace_body(rule) {
            result.insert(name.clone());
        }
    }
    result
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
        let old_suppress = out.suppress_brace_indent;
        if grammar.inline_brace_rules.contains(kind) {
            out.suppress_brace_indent = true;
        }
        let mut cursor = ChildCursor::new(&edges);
        emit_production(protocol, schema, grammar, vertex_id, rule, &mut cursor, out)?;
        // Drain any extras left after the rule walk completed; tree-sitter
        // may record trailing comments as children of the surrounding
        // vertex (i.e. after the last structural child the rule matched).
        drain_extras(protocol, schema, grammar, &mut cursor, out)?;
        out.suppress_brace_indent = old_suppress;
        return Ok(());
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
                out.token(&placeholder_for_pattern(value));
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
                    walk_in_mu_frame(
                        protocol, schema, grammar, vertex_id, name, rule, cursor, out,
                    )
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
                    if let Some(alias_value) = grammar.external_alias_map.get(name) {
                        out.token(alias_value);
                        return Ok(());
                    }
                    if name == "_indent" || name.ends_with("_indent") {
                        out.indent_open();
                    } else if name == "_dedent" || name.ends_with("_dedent") {
                        out.indent_close();
                    } else if name.contains("line_ending")
                        || name.contains("newline")
                        || name.ends_with("_or_eof")
                    {
                        out.newline();
                    } else if name.contains("semicolon") {
                        out.token(";");
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
                walk_in_mu_frame(
                    protocol, schema, grammar, vertex_id, name, rule, cursor, out,
                )
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
            let separator_leading_seq: Option<&[Production]> = match content.as_ref() {
                Production::Seq { members } if members.len() >= 2 => {
                    let first = &members[0];
                    let is_separator_slot = match first {
                        Production::Choice { members } => {
                            members.iter().any(|m| matches!(m, Production::Blank))
                        }
                        Production::Optional { .. } => true,
                        _ => false,
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
                        let pre_sep = out.snapshot();
                        let sep_result = emit_production(
                            protocol,
                            schema,
                            grammar,
                            vertex_id,
                            &seq_members[0],
                            cursor,
                            out,
                        );
                        match sep_result {
                            Err(e) => Err(e),
                            Ok(()) => {
                                if !out.lit_emitted_since(pre_sep) {
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
        Production::Token { content }
        | Production::ImmediateToken { content }
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
    // structural children, emit the captured text. Identifiers and
    // similar terminals reach here when an ALIAS wraps a SYMBOL that
    // resolves to a PATTERN.
    if let Some(literal) = literal_value(schema, child_id) {
        if children_for(schema, child_id).is_empty() {
            out.token(literal);
            return Ok(());
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
            return emit_production(protocol, schema, grammar, child_id, rule, &mut cursor, out);
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
    // Discriminator-driven dispatch (highest priority): when the
    // walker recorded a `chose-alt-fingerprint` constraint at parse
    // time, dispatch directly against that. This is the categorical
    // discriminator: it survives stripping of byte-position
    // constraints (so by-construction round-trips work) and is the
    // explicit witness of which CHOICE alternative the parser took.
    //
    // Falls back to the live `interstitial-*` substring blob when no
    // fingerprint is present (e.g. instances built by callers that
    // bypass the AstWalker). Both blobs are scored by the longest
    // STRING-literal token in an alternative that matches; the
    // length tiebreak prefers `&&` over `&`, `==` over `=`, etc.
    let constraint_blob = schema
        .constraints
        .get(vertex_id)
        .map(|cs| {
            let fingerprint: Option<&str> = cs
                .iter()
                .find(|c| c.sort.as_ref() == "chose-alt-fingerprint")
                .map(|c| c.value.as_str());
            if let Some(fp) = fingerprint {
                fp.to_owned()
            } else {
                cs.iter()
                    .filter(|c| {
                        let s = c.sort.as_ref();
                        s.starts_with("interstitial-") && !s.ends_with("-start-byte")
                    })
                    .map(|c| c.value.as_str())
                    .collect::<Vec<&str>>()
                    .join(" ")
            }
        })
        .unwrap_or_default();
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
    if !any_unconsumed && blank_present {
        return alternatives.iter().find(|a| matches!(a, Production::Blank));
    }
    if !any_unconsumed && !blank_present {
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

    if !constraint_blob.is_empty() {
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
        let mut best_alt: Option<&Production> = None;
        let mut tied = false;
        for alt in alternatives {
            let strings = literal_strings(alt);
            if strings.is_empty() {
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
            // Symbol score is computed only as a tiebreaker among alts
            // whose literal-token coverage is the same; it never lifts
            // an alt above one with a strictly higher literal score.
            // Reads the `chose-alt-child-kinds` constraint (a separate
            // sequence the walker emits, kept apart from the literal
            // fingerprint to avoid cross-contamination).
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
                || (literal_score == best_literal && symbol_score > best_symbols);
            let same = literal_score == best_literal && symbol_score == best_symbols;
            if better {
                best_literal = literal_score;
                best_symbols = symbol_score;
                best_alt = Some(alt);
                tied = false;
            } else if same && best_alt.is_some() {
                tied = true;
            }
        }
        // Only commit to an alt when the fingerprint discriminates it
        // uniquely. A tie means the alts share the same literal token
        // set (e.g. JSON's `string = CHOICE { SEQ { '"', '"' }, SEQ {
        // '"', _string_content, '"' } }` — both alts contain just the
        // two `"` tokens). In that case fall through to cursor-based
        // dispatch, which uses the actual edge structure.
        if let Some(alt) = best_alt {
            if !tied {
                return Some(alt);
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
                            .any(|s| *s == "_indent" || s.ends_with("_indent"))
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
        let mut visited = std::collections::HashSet::new();
        let mut yield_cache = grammar.yield_sets.clone();
        for alt in alternatives {
            let ys = yield_of_production(grammar, alt, &mut visited, &mut yield_cache);
            if ys.contains(target_kind) {
                return Some(alt);
            }
            visited.clear();
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
    /// A user-visible terminal contributed by the grammar.
    Lit(String),
    /// `indent_open` marker emitted when a `Lit` matched the policy's
    /// open list. Carried as a separate token so layout can decide to
    /// break + indent without re-scanning.
    IndentOpen,
    /// `indent_close` marker emitted before a closer-`Lit`.
    IndentClose,
    /// "Break a line here if not already at line start" — used after
    /// statements/declarations and after open braces.
    LineBreak,
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
    suppress_brace_indent: bool,
}

#[derive(Clone)]
struct OutputSnapshot {
    tokens_len: usize,
}

impl<'a> Output<'a> {
    fn new(policy: &'a FormatPolicy) -> Self {
        Self {
            tokens: Vec::new(),
            policy,
            suppress_brace_indent: false,
        }
    }

    fn token(&mut self, value: &str) {
        if value.is_empty() {
            return;
        }

        // A grammar STRING whose value is a newline (e.g. abc's `_NL = "\n"`
        // or any rule that uses `"\n"` as a structural line terminator)
        // must route through the layout's `LineBreak` channel. Emitting it
        // as a `Lit` leaves the newline character in the byte stream but
        // also makes `needs_space_between` insert the configured separator
        // between the newline and the following token, producing leading
        // spaces on every line after the first.
        if value == "\n" || value == "\r\n" || value == "\r" {
            self.tokens.push(Token::LineBreak);
            return;
        }

        // A captured literal value (typically a vertex's `literal-value`
        // constraint covering the full source span of a terminal-like
        // rule, e.g. abc's `reference_number_line` matching `"X:1\n"`)
        // may contain trailing newlines. Splitting the trailing newlines
        // off as a `LineBreak` lets the layout pass treat the next Lit
        // as starting a new line; otherwise the next Lit pair would
        // trigger `needs_space_between` against the embedded `\n` and
        // insert the policy separator at column 0 of the new line.
        let trimmed = value.trim_end_matches(['\n', '\r']);
        let trailing_newlines = value.len() - trimmed.len();
        if trailing_newlines > 0 && !trimmed.is_empty() {
            if !self.suppress_brace_indent && self.policy.indent_close.iter().any(|t| t == trimmed)
            {
                self.tokens.push(Token::IndentClose);
            }
            self.tokens.push(Token::Lit(trimmed.to_owned()));
            if !self.suppress_brace_indent && self.policy.indent_open.iter().any(|t| t == trimmed) {
                self.tokens.push(Token::IndentOpen);
            } else if self.policy.line_break_after.iter().any(|t| t == trimmed) {
                // already emitting a LineBreak below for the trailing \n
            }
            self.tokens.push(Token::LineBreak);
            return;
        }

        if !self.suppress_brace_indent && self.policy.indent_close.iter().any(|t| t == value) {
            self.tokens.push(Token::IndentClose);
        }

        self.tokens.push(Token::Lit(value.to_owned()));

        if !self.suppress_brace_indent && self.policy.indent_open.iter().any(|t| t == value) {
            self.tokens.push(Token::IndentOpen);
            self.tokens.push(Token::LineBreak);
        } else if self.policy.line_break_after.iter().any(|t| t == value)
            && !(self.suppress_brace_indent && (value == "{" || value == "}"))
        {
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
            .any(|t| matches!(t, Token::Lit(_)))
    }

    /// Push a marker that suppresses the next inter-Lit separator the
    /// layout pass would otherwise insert. Used to encode "no source-
    /// level separator was emitted between these two Lits" without
    /// having to make per-grammar adjacency decisions in the layout.
    fn no_space(&mut self) {
        self.tokens.push(Token::NoSpace);
    }

    fn finish(self) -> Vec<u8> {
        layout(&self.tokens, self.policy)
    }
}

/// Fold a token list into bytes. The algebra:
/// * adjacent `Lit`s get a single space iff `needs_space_between(a, b)`,
/// * `IndentOpen` / `IndentClose` adjust a depth counter,
/// * `LineBreak` writes `\n` if not already at line start, then the
///   next `Lit` writes `indent * indent_width` spaces of indent.
fn layout(tokens: &[Token], policy: &FormatPolicy) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut indent: usize = 0;
    let mut at_line_start = true;
    let mut last_lit: Option<&str> = None;
    // True iff, at the moment `last_lit` was emitted, the cursor was at a
    // position where the grammar expects an operand: start of stream / line,
    // just after an open paren / bracket / brace, just after a separator like
    // `,` or `;`, or just after a binary / assignment operator. Used by
    // `needs_space_between` to recognise `last_lit` as a tight unary prefix
    // (`f(-1.0)`) rather than a spaced binary operator (`a - b`).
    let mut last_was_in_operand_position = true;
    let mut expecting_operand = true;
    // Set when a `Token::NoSpace` marker is seen; cleared when the next
    // Lit consumes it. While set, suppress the policy separator that
    // would otherwise be inserted before the next Lit.
    let mut suppress_next_separator = false;
    let newline = policy.newline.as_bytes();
    let separator = policy.separator.as_bytes();

    for tok in tokens {
        match tok {
            Token::IndentOpen => indent += 1,
            Token::IndentClose => {
                indent = indent.saturating_sub(1);
                if !at_line_start {
                    bytes.extend_from_slice(newline);
                    at_line_start = true;
                    expecting_operand = true;
                }
            }
            Token::LineBreak => {
                if !at_line_start {
                    bytes.extend_from_slice(newline);
                    at_line_start = true;
                    expecting_operand = true;
                }
            }
            Token::NoSpace => {
                suppress_next_separator = true;
            }
            Token::Lit(value) => {
                if at_line_start {
                    bytes.extend(std::iter::repeat_n(b' ', indent * policy.indent_width));
                } else if let Some(prev) = last_lit {
                    if !suppress_next_separator
                        && needs_space_between(prev, value, last_was_in_operand_position)
                    {
                        bytes.extend_from_slice(separator);
                    }
                }
                suppress_next_separator = false;
                bytes.extend_from_slice(value.as_bytes());
                at_line_start = false;
                last_was_in_operand_position = expecting_operand;
                expecting_operand = leaves_operand_position(value);
                last_lit = Some(value.as_str());
                // Line comments consume text to end-of-line but the
                // newline terminator is not part of their literal
                // value. Force a line break after any Lit that starts
                // with a line-comment prefix so subsequent tokens
                // don't appear on the comment line.
                if value.starts_with("//") || value.starts_with('#') {
                    bytes.extend_from_slice(newline);
                    at_line_start = true;
                    expecting_operand = true;
                }
            }
        }
    }

    if !at_line_start {
        bytes.extend_from_slice(newline);
    }
    bytes
}

/// True iff emitting `tok` leaves the cursor in a position where the
/// grammar expects an operand next. Operand-introducing tokens are open
/// punctuation, separators, and operator-like strings; operand-terminating
/// tokens are identifiers, literals, and closing punctuation.
fn leaves_operand_position(tok: &str) -> bool {
    if tok.is_empty() {
        return true;
    }
    if is_punct_open(tok) {
        return true;
    }
    if matches!(tok, "," | ";") {
        return true;
    }
    if is_punct_close(tok) {
        return false;
    }
    if first_is_alnum_or_underscore(tok) || last_ends_with_alnum(tok) {
        return false;
    }
    // Pure punctuation/operator runs (`=`, `+`, `-`, `<=`, `>>`, …) leave
    // the cursor expecting another operand.
    true
}

fn needs_space_between(last: &str, next: &str, expecting_operand: bool) -> bool {
    if last.is_empty() || next.is_empty() {
        return false;
    }
    if is_punct_open(last) || is_punct_open(next) {
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
    // Tight unary prefix: `last` is a sign/logical-not operator emitted
    // where the grammar expected an operand, so it glues to `next`.
    // `expecting_operand` here means: just before `last` was emitted,
    // the cursor expected an operand, which makes `last` a unary prefix.
    // Examples: `f(-1.0)`, `[ -2, 3 ]`, `return -x`, `a = !flag`.
    if expecting_operand && is_unary_prefix_operator(last) && first_is_operand_start(next) {
        return false;
    }
    if last_is_word_like(last) && first_is_word_like(next) {
        return true;
    }
    if last_ends_with_alnum(last) && first_is_alnum_or_underscore(next) {
        return true;
    }
    // Adjacent operator runs: keep them apart so the lexer doesn't glue
    // `>` and `=` into `>=` unintentionally.
    true
}

fn is_unary_prefix_operator(s: &str) -> bool {
    matches!(s, "-" | "+" | "!" | "~")
}

fn first_is_operand_start(s: &str) -> bool {
    s.chars()
        .next()
        .map(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '(')
        .unwrap_or(false)
}

fn is_punct_open(s: &str) -> bool {
    matches!(s, "(" | "[" | "{" | "\"" | "'" | "`" | "@" | "#")
        || s.ends_with('{')
        || s.ends_with('(')
        || s.ends_with('[')
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
#[allow(clippy::unwrap_used)]
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
