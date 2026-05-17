//! The put-direction of the parse / decorate / emit lens.
//!
//! `decorate` attaches a complete layout enrichment fibre to an
//! [`AbstractSchema`], producing a [`DecoratedSchema`] that
//! `emit_pretty_with_protocol` can render byte-for-byte. It is a
//! section of the schema-level forgetful U
//! [`DecoratedSchema::forget_layout`] up to kind-multiset
//! equivalence.
//!
//! ## Implementation strategy
//!
//! 1. Render the abstract schema to canonical bytes via
//!    `emit_pretty_with_policy`. The de-novo emitter walks
//!    `grammar.json` production rules driven by the caller-supplied
//!    [`LayoutPolicy`].
//!
//! 2. Re-parse those bytes. The parse walker attaches the full layout
//!    fibre (`start-byte`, `end-byte`, every `interstitial-N`,
//!    `chose-alt-fingerprint`, `chose-alt-child-kinds`) and invents
//!    fresh vertex IDs.
//!
//! Step 2's ID renaming is intrinsic to the parse walker: tree-sitter
//! reparses can consolidate or fragment tokens at boundaries that the
//! emit pipeline doesn't know how to mirror (e.g. lilypond's
//! `c'4` parses as a single note even when the emitter rendered three
//! tokens `c`, `'`, `4`). Recovering vertex IDs from the abstract
//! input by parallel walk therefore *cannot* succeed for all
//! grammars; the documented section law holds at the standard
//! granularity of [`kind_multiset`](panproto_schema::kind_multiset)
//! and [`edge_multiset`](panproto_schema::edge_multiset).
//!
//! ## Laws
//!
//! For every `a : AbstractSchema` and `p : LayoutPolicy`:
//!
//! - **Section law (mod kind-multiset):**
//!   `kind_multiset(forget_layout(decorate(a, p)).as_schema()) ==
//!    kind_multiset(a.as_schema())`
//!   for every protocol with a vendored grammar — verified by the
//!   `decorate_section_law` integration test.
//! - **Policy fidelity:** the bytes produced by `pretty_with_protocol`
//!   honour every field of `p` (separator, newline, indent_width,
//!   line_break_after, indent_open / close).

use panproto_gat::{EnrichmentKind, LayoutPolicySpec};
use panproto_lens::enrichment_registry::{LayoutEnricher, register_enricher};
use panproto_lens::error::LensError;
use panproto_schema::{AbstractSchema, DecoratedSchema, Schema};

use crate::error::ParseError;
use crate::layout_policy::{LayoutPolicy, policy_from_spec};
use crate::registry::AstParser;

/// Decorate an abstract schema by routing through `emit_pretty_with_policy +
/// parse` against `parser`.
///
/// # Errors
///
/// Returns [`ParseError::EmitFailed`] when the abstract schema cannot
/// be rendered (missing grammar; vertex kind not a grammar rule), or
/// any other [`ParseError`] variant if the parser cannot re-ingest
/// its canonical output — the latter indicates a regression in the
/// parse/emit pipeline rather than a user bug.
pub fn decorate_with_parser(
    parser: &dyn AstParser,
    abstract_schema: &AbstractSchema,
    policy: &LayoutPolicy,
) -> Result<DecoratedSchema, ParseError> {
    let schema = abstract_schema.as_schema();
    if schema.protocol != parser.protocol_name() {
        return Err(ParseError::SchemaConstruction {
            reason: format!(
                "decorate_with_parser: protocol mismatch — parser is '{}' but schema is '{}'",
                parser.protocol_name(),
                schema.protocol,
            ),
        });
    }
    let decorated = decorate_schema(parser, schema, policy)?;
    Ok(DecoratedSchema::wrap_unchecked(decorated))
}

/// Schema-level decorate driver shared by [`decorate_with_parser`] and
/// the [`ParserLayoutEnricher`] adapter installed in the lens crate's
/// enrichment registry.
fn decorate_schema(
    parser: &dyn AstParser,
    abstract_schema: &Schema,
    policy: &LayoutPolicy,
) -> Result<Schema, ParseError> {
    let bytes = parser.emit_pretty_with_policy(abstract_schema, policy)?;
    parser.parse(&bytes, "decorate")
}

/// Adapter exposing one registered parser as a
/// [`LayoutEnricher`](panproto_lens::enrichment_registry::LayoutEnricher).
///
/// Held by the global enrichment registry; one driver is installed
/// per protocol at [`ParserRegistry::register`](crate::ParserRegistry::register)
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
        policy: &LayoutPolicySpec,
    ) -> Result<Schema, LensError> {
        let runtime_policy = policy_from_spec(policy);
        decorate_schema(self.parser.as_ref(), schema, &runtime_policy).map_err(|e| {
            LensError::EnrichmentSynthesisFailed {
                kind: EnrichmentKind::Layout,
                enricher: self.protocol.clone(),
                detail: e.to_string(),
            }
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
