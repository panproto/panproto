//! Runtime `LayoutPolicy` for the parse / decorate / emit lens.
//!
//! `LayoutPolicy` is a renamed re-export of
//! [`FormatPolicy`](crate::emit_pretty::FormatPolicy) — the same
//! configuration that the de-novo emitter consumes. Naming aligns
//! with the put-direction terminology of the parse / decorate / emit
//! lens: parsing erases layout; `decorate` puts it back; the policy
//! is the put-direction complement that pins down what whitespace,
//! indentation, and CHOICE disambiguators the put step chooses when
//! the parse-side fingerprint is absent.
//!
//! Its [`LayoutPolicySpec`](panproto_gat::LayoutPolicySpec) projection
//! is the wire-serialisable form embedded in
//! [`TheoryTransform::AddEnrichment`](panproto_gat::TheoryTransform::AddEnrichment).

use panproto_gat::LayoutPolicySpec;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use crate::emit_pretty::FormatPolicy;

/// Runtime layout policy for `decorate` and `pretty_with_protocol`.
///
/// Aliased to [`FormatPolicy`](crate::emit_pretty::FormatPolicy): the
/// emitter's own policy struct is exactly the put-direction
/// complement of the parse/emit lens, so the two are one type.
pub type LayoutPolicy = FormatPolicy;

/// Project a [`LayoutPolicy`] to its wire-serialisable form for
/// embedding in [`TheoryTransform::AddEnrichment`](panproto_gat::TheoryTransform::AddEnrichment).
#[must_use]
pub fn policy_to_spec(policy: &LayoutPolicy) -> LayoutPolicySpec {
    LayoutPolicySpec {
        indent_width: policy.indent_width,
        separator: policy.separator.clone(),
        newline: policy.newline.clone(),
        line_break_after: policy.line_break_after.clone(),
        indent_open: policy.indent_open.clone(),
        indent_close: policy.indent_close.clone(),
        disambiguators: FxHashMap::default(),
    }
}

/// Recover a runtime [`LayoutPolicy`] from its wire-serialisable
/// [`LayoutPolicySpec`].
#[must_use]
pub fn policy_from_spec(spec: &LayoutPolicySpec) -> LayoutPolicy {
    FormatPolicy {
        indent_width: spec.indent_width,
        separator: spec.separator.clone(),
        newline: spec.newline.clone(),
        line_break_after: spec.line_break_after.clone(),
        indent_open: spec.indent_open.clone(),
        indent_close: spec.indent_close.clone(),
    }
}

/// CHOICE disambiguator map embedded in the protolens definition.
///
/// Mapped from a grammar production rule name to the index of the
/// alternative that the policy selects when child-kind matching
/// alone cannot uniquely pick an alternative. Carried alongside the
/// runtime policy on the put-direction side of the lens.
pub type DisambiguatorMap = FxHashMap<Arc<str>, usize>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_through_spec() {
        let p = LayoutPolicy::default();
        let q = policy_from_spec(&policy_to_spec(&p));
        assert_eq!(p.indent_width, q.indent_width);
        assert_eq!(p.separator, q.separator);
        assert_eq!(p.newline, q.newline);
        assert_eq!(p.line_break_after, q.line_break_after);
    }

    #[test]
    fn non_default_separator_round_trips() {
        let p = LayoutPolicy {
            separator: "\t".to_owned(),
            newline: "\r\n".to_owned(),
            indent_width: 4,
            ..LayoutPolicy::default()
        };
        let q = policy_from_spec(&policy_to_spec(&p));
        assert_eq!(q.separator, "\t");
        assert_eq!(q.newline, "\r\n");
        assert_eq!(q.indent_width, 4);
    }
}
