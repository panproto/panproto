//! Schema enrichments.
//!
//! An *enrichment* of a schema is a family of constraint sorts that lives
//! over the abstract schema as a Grothendieck fibration: the base is the
//! abstract schema (vertex kinds, edges, content-level constraints) and
//! the fibre over each vertex is the data the enrichment carries for
//! that vertex.
//!
//! Stripping an enrichment is the forgetful functor down to the base.
//! Adding an enrichment is its section; well-formed sections require an
//! enrichment-specific synthesis procedure (for `Layout`, a grammar walk
//! driven by a [`LayoutPolicySpec`]).
//!
//! This module names the enrichments and gives the layout-kind predicate
//! that classifies which constraint sorts belong to the layout fibre.

/// The classifying tag for a schema enrichment.
///
/// Enrichments are not theory-level structure: they extend the *schema*
/// (constraint witnesses at vertices) rather than the underlying
/// algebraic theory. A protolens whose source transform is
/// [`StripEnrichment`](crate::TheoryTransform::StripEnrichment) and
/// target is [`AddEnrichment`](crate::TheoryTransform::AddEnrichment)
/// realises the section of the corresponding forgetful functor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum EnrichmentKind {
    /// Source-layout enrichment for grammar-driven parsers.
    ///
    /// Attaches the constraint sorts emitted by the tree-sitter walker
    /// (byte spans, interstitial text, CHOICE-alternative discriminators)
    /// to vertices of an abstract schema, producing a decorated schema
    /// that the emitter can render back to source bytes.
    Layout,
}

impl EnrichmentKind {
    /// Returns `true` when `sort` belongs to this enrichment's fibre.
    ///
    /// For `Layout`, this matches the constraint sorts written by the
    /// parse-side walker: byte spans, `chose-alt-*` discriminators, and
    /// every `interstitial-*` variant (including the `-start-byte`
    /// sibling sort).
    #[must_use]
    pub fn is_member_sort(self, sort: &str) -> bool {
        match self {
            Self::Layout => is_layout_sort(sort),
        }
    }
}

/// Predicate identifying the constraint sorts that make up the
/// layout enrichment fibre.
#[must_use]
pub fn is_layout_sort(sort: &str) -> bool {
    matches!(sort, "start-byte" | "end-byte")
        || sort.starts_with("chose-alt-")
        || sort.starts_with("interstitial-")
}

/// The layout role of a token, derived from the grammar's structure.
///
/// A role is a *fact about the grammar*, not a guess about the token's
/// text: a `(` that the grammar marks as the open of a matched pair is
/// [`BracketOpen`](LayoutRole::BracketOpen) regardless of which character
/// it is, and a token the grammar wraps in `IMMEDIATE_TOKEN` is
/// [`Immediate`](LayoutRole::Immediate) regardless of where it sits. The
/// emitter assigns a role to every token during derivation, then renders
/// spacing by consulting the pure [`Adjacency`] relation over role pairs.
///
/// This is the theory-level vocabulary the parse-side derivation targets;
/// `panproto_parse`'s `TokenRole` is the historical, per-emit-call
/// equivalent that this supersedes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum LayoutRole {
    /// Open of a matched delimiter pair (`(`, `[`, `{`, `<`, `begin`,
    /// `string_start`). Tight on its inner side.
    BracketOpen,
    /// Close of a matched delimiter pair (`)`, `]`, `}`, `end`,
    /// `string_end`). Tight on its inner side.
    BracketClose,
    /// First token of a `REPEAT` body's separator slot (`,` in
    /// `REPEAT(SEQ[",", item])`). Tight before, space after.
    Separator,
    /// Language keyword (`if`, `model`, `and`). Space on both sides.
    Keyword,
    /// Infix operator between content members of a CHOICE alternative
    /// (`+`, `=`, `~`, `<-`). Space on both sides.
    Operator,
    /// Structural connector between content members of a standalone SEQ
    /// (`.`, `::`, `->`). Tight on both sides.
    Connector,
    /// Text from a leaf vertex's `literal-value` constraint.
    Terminal,
    /// A token the grammar wraps in `IMMEDIATE_TOKEN`: the lexer would
    /// glue it to its neighbour with no intervening whitespace (`.` in a
    /// float literal, an immediate string delimiter). Tight on both
    /// sides, unconditionally.
    Immediate,
}

/// The spacing decision between two adjacent tokens.
///
/// The emitter's layout pass walks the token stream and, for each
/// adjacent pair, consults [`Adjacency::between`] to decide what to
/// emit between them. This is the total, declared relation that
/// replaces an ad-hoc role-pair lookup table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Adjacency {
    /// No separator: the tokens abut directly (`f(`, `0.5`, `a.b`).
    Tight,
    /// A single separator (normally one space).
    Space,
    /// A line break.
    Break,
}

impl Adjacency {
    /// The spacing between a token of role `prev` and a following token
    /// of role `next`.
    ///
    /// Pure over roles: word-likeness and token text do not enter here.
    /// Any text-dependent decision (a word-like bracket that should space
    /// like a keyword) is resolved during *role assignment*, so this
    /// relation stays a fact about roles alone. [`Immediate`](LayoutRole::Immediate)
    /// is glued on both sides unconditionally; the remaining arms encode
    /// the historical role-pair spacing table.
    #[must_use]
    pub const fn between(prev: LayoutRole, next: LayoutRole) -> Self {
        use LayoutRole::{BracketClose, BracketOpen, Connector, Immediate, Separator, Terminal};
        // The relation is most cleanly stated as the set of tight pairs;
        // everything else takes a separator. Ordering of the inner
        // alternatives is immaterial because they are disjoint after the
        // earlier ones are excluded.
        match (prev, next) {
            // Immediate tokens are lexically glued to their neighbours;
            // brackets are tight on the inside; a separator hugs the token
            // before it; connectors (`.`, `::`, `->`) are tight both ways;
            // a callee/close abuts a following open (`f(`, `}{`).
            (Immediate | BracketOpen | Connector, _)
            | (_, Immediate | BracketClose | Separator | Connector)
            | (Terminal | BracketClose, BracketOpen) => Self::Tight,
            // Every other adjacency (separator-then-token, keyword either
            // side, terminal/operator runs, close-then-non-bracket,
            // operator-then-open) takes a single separator.
            _ => Self::Space,
        }
    }
}

/// A grammar-derived layout specification: the theory-level data the
/// emitter interprets to render a schema back to source bytes.
///
/// This is the payload of the [`Layout`](EnrichmentKind::Layout)
/// enrichment in its *derived* form. It carries the policy knobs
/// ([`policy`](LayoutSpec::policy)) plus a per-rule role assignment
/// computed once from the grammar's facts (`IMMEDIATE_TOKEN`, matched
/// delimiters, external `_indent`/`_dedent` references). The emitter is
/// a model-interpreter over this spec: it looks up each rule's roles,
/// indent markers, and separator policy here rather than re-deriving
/// them per emit call.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct LayoutSpec {
    /// Whitespace / indentation knobs (indent width, newline, separator).
    pub policy: LayoutPolicySpec,
    /// Per-rule layout facts, keyed by grammar rule name. Rules absent
    /// here fall back to structural defaults during interpretation.
    pub rules: std::collections::BTreeMap<String, RuleLayout>,
}

impl LayoutSpec {
    /// A spec carrying only policy knobs, with no per-rule facts yet.
    #[must_use]
    pub const fn from_policy(policy: LayoutPolicySpec) -> Self {
        Self {
            policy,
            rules: std::collections::BTreeMap::new(),
        }
    }
}

/// The layout facts derived for a single grammar rule.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct RuleLayout {
    /// Role assigned to each STRING token in the rule body, keyed by the
    /// token's literal text (mirrors the parse-side `token_roles` map).
    pub token_roles: std::collections::BTreeMap<String, LayoutRole>,
    /// Whether this rule opens an indentation scope (a suite/block whose
    /// body the emitter must indent and line-break), derived from the
    /// rule referencing an external indent token.
    pub opens_indent: bool,
    /// The spacing the emitter inserts at this rule's `REPEAT` separator
    /// slot, when one is present (e.g. [`Break`](Adjacency::Break) for a
    /// statement list whose separator is a layout newline).
    pub separator: Option<Adjacency>,
}

/// Wire-serialisable layout policy carried inside
/// [`TheoryTransform::AddEnrichment`](crate::TheoryTransform::AddEnrichment).
///
/// Mirrors the field set of `panproto_parse::emit_pretty::FormatPolicy`
/// — the runtime policy actually consumed by the de-novo emitter.
/// Every field here is honoured end-to-end by `emit_pretty`'s
/// rendering pipeline; there are no stub fields whose values do not
/// affect output.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LayoutPolicySpec {
    /// Number of spaces per indent level.
    pub indent_width: usize,
    /// Separator inserted between adjacent terminals that the lexer
    /// would otherwise glue together. Default: a single space.
    pub separator: String,
    /// Newline byte sequence. Default: `"\n"`.
    pub newline: String,
    /// Tokens after which the emitter breaks to a new line.
    pub line_break_after: Vec<String>,
    /// Tokens that increase indent on emission.
    pub indent_open: Vec<String>,
    /// Tokens that decrease indent on emission.
    pub indent_close: Vec<String>,
}

impl Default for LayoutPolicySpec {
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn layout_sort_predicate_covers_walker_constraints() {
        assert!(is_layout_sort("start-byte"));
        assert!(is_layout_sort("end-byte"));
        assert!(is_layout_sort("chose-alt-fingerprint"));
        assert!(is_layout_sort("chose-alt-child-kinds"));
        assert!(is_layout_sort("interstitial-0"));
        assert!(is_layout_sort("interstitial-12-start-byte"));
        assert!(!is_layout_sort("literal-value"));
        assert!(!is_layout_sort("field:op"));
    }

    #[test]
    fn enrichment_kind_membership_matches_predicate() {
        assert!(EnrichmentKind::Layout.is_member_sort("start-byte"));
        assert!(EnrichmentKind::Layout.is_member_sort("interstitial-3"));
        assert!(!EnrichmentKind::Layout.is_member_sort("literal-value"));
    }

    /// `Adjacency::between` must reproduce the historical role-pair
    /// spacing table for the seven non-`Immediate` roles: `Space` where
    /// the old `needs_space_by_role` returned `true`, `Tight` otherwise.
    /// This is the regression anchor for the calculus subsuming the
    /// table without behaviour drift.
    #[test]
    fn adjacency_matches_historical_spacing_table() {
        use Adjacency::{Space, Tight};
        use LayoutRole::{
            BracketClose, BracketOpen, Connector, Keyword, Operator, Separator, Terminal,
        };
        // Brackets tight on the inside.
        assert_eq!(Adjacency::between(BracketOpen, Terminal), Tight);
        assert_eq!(Adjacency::between(Terminal, BracketClose), Tight);
        // Separator: tight before, space after.
        assert_eq!(Adjacency::between(Terminal, Separator), Tight);
        assert_eq!(Adjacency::between(Separator, Terminal), Space);
        // Connectors always tight.
        assert_eq!(Adjacency::between(Connector, Terminal), Tight);
        assert_eq!(Adjacency::between(Terminal, Connector), Tight);
        // Call / adjacency cases.
        assert_eq!(Adjacency::between(Terminal, BracketOpen), Tight);
        assert_eq!(Adjacency::between(BracketClose, BracketOpen), Tight);
        // Keywords always spaced.
        assert_eq!(Adjacency::between(Keyword, Terminal), Space);
        assert_eq!(Adjacency::between(Terminal, Keyword), Space);
        // Terminals / operators spaced.
        assert_eq!(Adjacency::between(Terminal, Terminal), Space);
        assert_eq!(Adjacency::between(Terminal, Operator), Space);
        assert_eq!(Adjacency::between(Operator, Terminal), Space);
        assert_eq!(Adjacency::between(Operator, Operator), Space);
        // Close before non-bracket, operator before open: spaced.
        assert_eq!(Adjacency::between(BracketClose, Terminal), Space);
        assert_eq!(Adjacency::between(Operator, BracketOpen), Space);
    }

    /// `Immediate` is glued on both sides regardless of the other role.
    #[test]
    fn immediate_role_is_always_tight() {
        for role in [
            LayoutRole::BracketOpen,
            LayoutRole::BracketClose,
            LayoutRole::Separator,
            LayoutRole::Keyword,
            LayoutRole::Operator,
            LayoutRole::Connector,
            LayoutRole::Terminal,
            LayoutRole::Immediate,
        ] {
            assert_eq!(
                Adjacency::between(LayoutRole::Immediate, role),
                Adjacency::Tight
            );
            assert_eq!(
                Adjacency::between(role, LayoutRole::Immediate),
                Adjacency::Tight
            );
        }
    }

    #[test]
    fn layout_spec_round_trips_through_json() {
        let mut spec = LayoutSpec::from_policy(LayoutPolicySpec::default());
        let mut roles = std::collections::BTreeMap::new();
        roles.insert(".".to_owned(), LayoutRole::Immediate);
        roles.insert("(".to_owned(), LayoutRole::BracketOpen);
        spec.rules.insert(
            "real_literal".to_owned(),
            RuleLayout {
                token_roles: roles,
                opens_indent: false,
                separator: Some(Adjacency::Break),
            },
        );
        let json = serde_json::to_string(&spec).unwrap();
        let back: LayoutSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, back);
    }
}
