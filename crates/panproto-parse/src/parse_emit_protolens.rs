//! The parse / decorate / emit lens packaged as a first-class
//! [`Protolens`].
//!
//! For every registered grammar `G`, [`parse_emit_protolens`] returns
//! a [`Protolens`] whose source endofunctor strips the layout
//! enrichment fibre (yielding an abstract schema) and whose target
//! endofunctor adds it back (yielding a decorated schema) via the
//! registered [`LayoutEnricher`](panproto_lens::enrichment_registry::LayoutEnricher).
//! The result composes with every other protolens in
//! `panproto-lens::combinators` and `panproto-lens::elementary` and
//! gets free law-checking through
//! [`panproto_lens::optic::check_optic_laws`].
//!
//! ```text
//! Source theory  F(S) = strip_layout(S)   -- forgetful U
//! Target theory  G(S) = decorate(S, π)    -- section of U at policy π
//! Complement      = layout fibre / synthesis driver name
//! ```

use std::sync::Arc;

use panproto_gat::{
    EnrichmentKind, TheoryConstraint, TheoryEndofunctor, TheoryTransform,
};
use panproto_lens::protolens::{ComplementConstructor, Protolens};
use panproto_lens::{Lens, error::LensError};
use panproto_schema::Schema;

use crate::layout_policy::LayoutPolicy;

/// Build a [`Protolens`] for the parse / decorate / emit lens at
/// `grammar` under the given `policy`.
///
/// The result is suitable for [`Protolens::instantiate`] against any
/// schema, and for composition via
/// [`panproto_lens::protolens::vertical_compose`] /
/// [`panproto_lens::protolens::horizontal_compose`].
///
/// A precondition check on instantiation verifies that an enrichment
/// driver is registered for `(EnrichmentKind::Layout, grammar)` — this
/// is normally satisfied automatically by
/// [`ParserRegistry::register`](crate::ParserRegistry::register), which
/// installs a driver for every protocol it accepts.
#[must_use]
pub fn parse_emit_protolens(grammar: &str, policy: &LayoutPolicy) -> Protolens {
    let enricher: Arc<str> = Arc::from(grammar);
    Protolens {
        name: panproto_gat::Name::from(format!("parse_emit/{grammar}")),
        source: TheoryEndofunctor {
            name: Arc::from("strip_layout"),
            precondition: TheoryConstraint::Unconstrained,
            transform: TheoryTransform::StripEnrichment(EnrichmentKind::Layout),
        },
        target: TheoryEndofunctor {
            name: Arc::from(format!("decorate/{grammar}")),
            precondition: TheoryConstraint::Unconstrained,
            transform: TheoryTransform::AddEnrichment {
                kind: EnrichmentKind::Layout,
                enricher: Arc::clone(&enricher),
                policy: policy.to_spec(),
            },
        },
        complement_constructor: ComplementConstructor::Enrichment {
            kind: EnrichmentKind::Layout,
            enricher,
        },
    }
}

/// Instantiate the parse/emit protolens at a specific schema.
///
/// # Errors
///
/// Returns [`LensError`] if no enrichment driver is registered for
/// `grammar`, or if the instantiation path fails (theory transform
/// application, migration compilation, etc.).
pub fn instantiate_parse_emit_lens(
    grammar: &str,
    policy: &LayoutPolicy,
    schema: &Schema,
    protocol: &panproto_schema::Protocol,
) -> Result<Lens, LensError> {
    if !panproto_lens::enrichment_registry::has_enricher(EnrichmentKind::Layout, grammar)
    {
        return Err(LensError::UnknownEnricher {
            kind: EnrichmentKind::Layout,
            enricher: grammar.to_owned(),
        });
    }
    parse_emit_protolens(grammar, policy).instantiate(schema, protocol)
}
