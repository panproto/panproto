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

use std::sync::Arc;

use rustc_hash::FxHashMap;

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
/// The runtime `LayoutPolicy` (with its `Cow<'static, str>` fields and
/// resolver hooks) lives in `panproto-parse`. This struct is the
/// serialisable projection: enough state to round-trip a policy through
/// a stored protolens definition.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LayoutPolicySpec {
    /// Whitespace inserted between adjacent terminals.
    pub separator: String,
    /// One indentation level.
    pub indent: String,
    /// Newline sequence.
    pub newline: String,
    /// Per-rule disambiguators: maps a production-rule name to the
    /// index of the alternative the policy selects when child-kind
    /// matching is ambiguous. An empty map means "ambiguity is an
    /// error"; this is the strict default.
    pub disambiguators: FxHashMap<Arc<str>, usize>,
}

impl Default for LayoutPolicySpec {
    fn default() -> Self {
        Self {
            separator: " ".to_owned(),
            indent: "  ".to_owned(),
            newline: "\n".to_owned(),
            disambiguators: FxHashMap::default(),
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
