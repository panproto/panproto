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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
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
}
