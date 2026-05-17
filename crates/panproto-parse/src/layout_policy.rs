//! Runtime `LayoutPolicy` for the parse/decorate/emit lens.
//!
//! The put-direction of the parse/emit lens (`decorate`) needs to fill
//! in whitespace and choose CHOICE alternatives that parsing erases.
//! `LayoutPolicy` is that put-direction complement: it carries the
//! whitespace conventions (separator, indent, newline) plus optional
//! per-rule disambiguators when a grammar's CHOICE structure cannot be
//! uniquely identified from the abstract child-kind sequence alone.
//!
//! Its [`LayoutPolicySpec`](panproto_gat::LayoutPolicySpec) projection
//! is the wire-serialisable form embedded in
//! [`TheoryTransform::AddEnrichment`](panproto_gat::TheoryTransform::AddEnrichment).

use std::borrow::Cow;
use std::sync::Arc;

use panproto_gat::LayoutPolicySpec;
use rustc_hash::FxHashMap;

/// Whitespace and disambiguation conventions for `decorate`.
///
/// All fields default to standard ASCII conventions: single-space
/// separator, two-space indent, LF newline, no disambiguators
/// (ambiguous alternatives are a hard error rather than silently
/// resolved). Per-rule disambiguators name the index of the CHOICE
/// alternative to take when the grammar can produce multiple matching
/// alternatives for the same abstract child-kind sequence.
#[derive(Debug, Clone)]
pub struct LayoutPolicy {
    /// Whitespace inserted between adjacent terminals in a production.
    pub separator: Cow<'static, str>,
    /// One indentation level.
    pub indent: Cow<'static, str>,
    /// Newline sequence.
    pub newline: Cow<'static, str>,
    /// Per-rule disambiguators: a rule name mapped to the alternative
    /// index that this policy chooses when the abstract child-kind
    /// sequence is ambiguous. Empty by default; an ambiguity with no
    /// entry here is a hard error.
    pub disambiguators: FxHashMap<Arc<str>, usize>,
}

impl Default for LayoutPolicy {
    fn default() -> Self {
        Self {
            separator: Cow::Borrowed(" "),
            indent: Cow::Borrowed("  "),
            newline: Cow::Borrowed("\n"),
            disambiguators: FxHashMap::default(),
        }
    }
}

impl LayoutPolicy {
    /// Project to the wire-serialisable spec embedded in
    /// `TheoryTransform::AddEnrichment`.
    #[must_use]
    pub fn to_spec(&self) -> LayoutPolicySpec {
        LayoutPolicySpec {
            separator: self.separator.clone().into_owned(),
            indent: self.indent.clone().into_owned(),
            newline: self.newline.clone().into_owned(),
            disambiguators: self.disambiguators.clone(),
        }
    }

    /// Recover a runtime policy from its wire-serialisable spec.
    #[must_use]
    pub fn from_spec(spec: &LayoutPolicySpec) -> Self {
        Self {
            separator: Cow::Owned(spec.separator.clone()),
            indent: Cow::Owned(spec.indent.clone()),
            newline: Cow::Owned(spec.newline.clone()),
            disambiguators: spec.disambiguators.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_round_trips_through_spec() {
        let p = LayoutPolicy::default();
        let q = LayoutPolicy::from_spec(&p.to_spec());
        assert_eq!(p.separator, q.separator);
        assert_eq!(p.indent, q.indent);
        assert_eq!(p.newline, q.newline);
        assert_eq!(p.disambiguators.len(), q.disambiguators.len());
    }
}
