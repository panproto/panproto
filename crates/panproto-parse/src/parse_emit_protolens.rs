//! The parse / decorate / emit lens packaged as a first-class
//! [`Protolens`](panproto_lens::Protolens).
//!
//! For every registered grammar `G`, [`parse_emit_protolens()`] returns
//! a [`Protolens`](panproto_lens::Protolens) whose source endofunctor strips the layout
//! enrichment fibre (yielding an abstract schema) and whose target
//! endofunctor adds it back via the registered
//! [`LayoutEnricher`](panproto_lens::enrichment_registry::LayoutEnricher).
//!
//! ```text
//! source theory  F(S) = StripEnrichment(Layout)(S)    -- the forgetful U
//! target theory  G(S) = AddEnrichment(Layout, π)(S)   -- a section of U
//! complement       = layout fibre (per-vertex witness)
//! ```
//!
//! ## What this protolens is, and is not
//!
//! The [`Protolens`](panproto_lens::Protolens) returned here is the **schema-level description**
//! of the parse/decorate/emit relationship: it documents which
//! constraint sorts belong to the layout fibre, which synthesis
//! driver populates them, and what policy the put-direction uses.
//! It composes with every other protolens in `panproto-lens` for
//! reasoning about chain laws.
//!
//! The operational entry points for actually rendering or
//! reconstructing source bytes are
//! [`ParserRegistry::decorate`](crate::ParserRegistry::decorate),
//! [`ParserRegistry::pretty_with_protocol`](crate::ParserRegistry::pretty_with_protocol),
//! and [`ParserRegistry::emit_pretty_with_protocol`](crate::ParserRegistry::emit_pretty_with_protocol)
//! — not [`Protolens::instantiate`](panproto_lens::Protolens::instantiate). The reason is that the parse-side
//! walker invents fresh vertex IDs that the lens framework's
//! `WInstance`-level get/put cannot align with the source schema's
//! IDs; the underlying mismatch is intrinsic to grammars that
//! consolidate tokens (e.g. lilypond's note pitches) at parse time.
//! The schema-level descriptive content of the protolens remains
//! useful for chain reasoning even when the lens framework's
//! instance-level migration semantics are not the right fit.

use std::sync::Arc;

use panproto_gat::{
    EnrichmentKind, TheoryConstraint, TheoryEndofunctor, TheoryTransform,
};
use panproto_lens::protolens::{ComplementConstructor, Protolens};

use crate::layout_policy::{LayoutPolicy, policy_to_spec};

/// Build a [`Protolens`] for the parse / decorate / emit relationship
/// at `grammar` under the given `policy`.
///
/// The protolens's *target* transform invokes the registered
/// [`LayoutEnricher`](panproto_lens::enrichment_registry::LayoutEnricher)
/// — installed automatically by
/// [`ParserRegistry::register`](crate::ParserRegistry::register) for
/// every protocol it accepts. The *complement constructor* records
/// that the discarded fibre is `EnrichmentKind::Layout` keyed by the
/// grammar name.
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
                policy: policy_to_spec(policy),
            },
        },
        complement_constructor: ComplementConstructor::Enrichment {
            kind: EnrichmentKind::Layout,
            enricher,
        },
    }
}
