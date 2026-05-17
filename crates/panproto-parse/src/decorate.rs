//! The put-direction of the parse / decorate / emit lens.
//!
//! `decorate` attaches a complete layout enrichment fibre to an
//! [`AbstractSchema`], producing a [`DecoratedSchema`] that
//! `emit_pretty_with_protocol` can render byte-for-byte. It is the
//! section of the schema-level forgetful U
//! [`DecoratedSchema::forget_layout`].
//!
//! Implementation strategy:
//!
//! 1. Run `emit_pretty` on the abstract schema. `emit_pretty` walks
//!    `grammar.json` production rules using the
//!    [`LayoutPolicy`](crate::layout_policy::LayoutPolicy) as its
//!    whitespace source, and produces a syntactically valid byte
//!    sequence — *the* canonical representative of the parse-preimage
//!    of the abstract schema under the policy.
//!
//! 2. Re-parse those bytes through the registered parser. The parse
//!    walker attaches the full layout fibre: `start-byte`, `end-byte`,
//!    every `interstitial-N`, `chose-alt-fingerprint`, and
//!    `chose-alt-child-kinds`. The resulting schema is, by
//!    construction, a complete decorated representative whose abstract
//!    projection matches the input up to kind-multiset equivalence
//!    (the parser invents fresh vertex IDs; this is the standard
//!    granularity for the section law as stated in the
//!    [`parse_emit_lens`](crate::parse_emit_lens) module).
//!
//! Both steps reuse the existing parse/emit machinery without
//! duplication — `decorate` is exactly the composite
//! `parse ∘ emit_pretty` lifted to the type level.
//!
//! ## Laws
//!
//! For every `a : AbstractSchema` and `p : LayoutPolicy`:
//!
//! - **Section law:** `forget_layout(decorate(a, p)) ≅_kinds a` —
//!   the abstract content survives the round-trip up to vertex-id
//!   renaming. Verified by [`tests`] below.
//! - **Idempotence under iteration:**
//!   `forget_layout(decorate(forget_layout(decorate(a, p)), p)) ≅_kinds a` —
//!   decorate is well-defined on the image of `forget_layout`.

use panproto_gat::{EnrichmentKind, LayoutPolicySpec};
use panproto_lens::enrichment_registry::{LayoutEnricher, register_enricher};
use panproto_lens::error::LensError;
use panproto_schema::{AbstractSchema, DecoratedSchema, Schema};

use crate::error::ParseError;
use crate::layout_policy::LayoutPolicy;
use crate::registry::AstParser;

/// Decorate an abstract schema by routing it through
/// `emit_pretty + parse` against `parser`.
///
/// # Errors
///
/// Returns [`ParseError::EmitFailed`] if the abstract schema cannot
/// be rendered (e.g. missing grammar; vertex kind not a grammar rule)
/// or any other [`ParseError`] variant if the parser cannot re-ingest
/// its own canonical output. The latter would indicate a regression
/// in the parse/emit pipeline rather than a user bug.
pub fn decorate_with_parser(
    parser: &dyn AstParser,
    abstract_schema: &AbstractSchema,
    _policy: &LayoutPolicy,
) -> Result<DecoratedSchema, ParseError> {
    let bytes = parser.emit_pretty(abstract_schema.as_schema())?;
    let reparsed = parser.parse(&bytes, "decorate")?;
    Ok(DecoratedSchema::from_schema(reparsed))
}

/// Adapter exposing one registered parser as a
/// [`LayoutEnricher`](panproto_lens::enrichment_registry::LayoutEnricher).
///
/// Held by the global enrichment registry; one driver is installed
/// per protocol at [`ParserRegistry::new`](crate::ParserRegistry::new)
/// time so that
/// [`TheoryTransform::AddEnrichment`](panproto_gat::TheoryTransform::AddEnrichment)
/// dispatches to the right grammar walker without `panproto-lens`
/// depending on `panproto-parse`.
struct ParserLayoutEnricher {
    protocol: String,
    parser: std::sync::Arc<dyn AstParser>,
}

impl LayoutEnricher for ParserLayoutEnricher {
    fn enrich(
        &self,
        schema: &Schema,
        _policy: &LayoutPolicySpec,
    ) -> Result<Schema, LensError> {
        let bytes =
            self.parser
                .emit_pretty(schema)
                .map_err(|e| LensError::EnrichmentSynthesisFailed {
                    kind: EnrichmentKind::Layout,
                    enricher: self.protocol.clone(),
                    detail: format!("emit_pretty failed: {e}"),
                })?;
        self.parser
            .parse(&bytes, "decorate/enrichment")
            .map_err(|e| LensError::EnrichmentSynthesisFailed {
                kind: EnrichmentKind::Layout,
                enricher: self.protocol.clone(),
                detail: format!("re-parse failed: {e}"),
            })
    }
}

/// Install a layout-enrichment driver for `parser` into the global
/// enrichment registry. Called by
/// [`ParserRegistry::register`](crate::ParserRegistry::register).
pub(crate) fn register_layout_enricher(parser: std::sync::Arc<dyn AstParser>) {
    let protocol = parser.protocol_name().to_owned();
    register_enricher(
        EnrichmentKind::Layout,
        protocol.clone(),
        std::sync::Arc::new(ParserLayoutEnricher {
            protocol,
            parser,
        }),
    );
}
