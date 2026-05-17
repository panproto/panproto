//! Cross-crate extension point for schema-enrichment synthesis.
//!
//! [`TheoryTransform::AddEnrichment`](panproto_gat::TheoryTransform::AddEnrichment)
//! needs to materialise an enrichment fibre on a schema, but the
//! synthesis procedure is enrichment-specific and grammar-aware: for
//! [`EnrichmentKind::Layout`](panproto_gat::EnrichmentKind::Layout) it
//! is the inverse of the parse-side grammar walker, which lives in
//! `panproto-parse` and depends on tree-sitter grammars.
//!
//! `panproto-lens` cannot directly depend on `panproto-parse` (the
//! dependency arrow would invert). This module is the slim trait /
//! registry pair `panproto-parse` populates at `ParserRegistry::new()`
//! time, so that `apply_theory_transform_to_schema` can dispatch the
//! `AddEnrichment` arm to a concrete driver.

use std::sync::{Arc, OnceLock, RwLock};

use panproto_gat::{EnrichmentKind, LayoutPolicySpec};
use panproto_schema::Schema;
use rustc_hash::FxHashMap;

use crate::error::LensError;

/// The synthesis driver for one enrichment family.
///
/// Implementations sit in `panproto-parse` (one per parser flavour)
/// and are registered by enricher name. The trait is intentionally
/// narrow: the lens crate only needs to invoke synthesis with a
/// schema and a policy spec.
pub trait LayoutEnricher: Send + Sync + 'static {
    /// Drive layout synthesis: read the abstract content of `schema`
    /// and return a new schema with the enrichment fibre attached.
    ///
    /// # Errors
    ///
    /// Returns [`LensError::EnrichmentSynthesisFailed`] if the input
    /// schema cannot be enriched (e.g. an unknown vertex kind, or an
    /// ambiguous CHOICE alternative that the policy does not resolve).
    fn enrich(
        &self,
        schema: &Schema,
        policy: &LayoutPolicySpec,
    ) -> Result<Schema, LensError>;
}

#[derive(Default)]
struct Registry {
    enrichers: FxHashMap<(EnrichmentKind, Arc<str>), Arc<dyn LayoutEnricher>>,
}

static REGISTRY: OnceLock<RwLock<Registry>> = OnceLock::new();

fn registry() -> &'static RwLock<Registry> {
    REGISTRY.get_or_init(|| RwLock::new(Registry::default()))
}

/// Register a synthesis driver for `(kind, enricher_name)`.
///
/// The convention for layout: `enricher_name` is the grammar name
/// (e.g. `"lilypond"`, `"json"`). Re-registering an existing key
/// replaces the prior driver — this is intentional so test code can
/// install fakes.
pub fn register_enricher(
    kind: EnrichmentKind,
    enricher_name: impl Into<Arc<str>>,
    driver: Arc<dyn LayoutEnricher>,
) {
    let mut g = registry().write().expect("enrichment registry poisoned");
    g.enrichers.insert((kind, enricher_name.into()), driver);
}

/// Look up a synthesis driver.
///
/// # Errors
///
/// Returns [`LensError::UnknownEnricher`] if no driver is registered
/// for `(kind, enricher_name)`.
pub fn lookup_enricher(
    kind: EnrichmentKind,
    enricher_name: &str,
) -> Result<Arc<dyn LayoutEnricher>, LensError> {
    let g = registry().read().expect("enrichment registry poisoned");
    let key: Arc<str> = Arc::from(enricher_name);
    g.enrichers
        .get(&(kind, key))
        .cloned()
        .ok_or_else(|| LensError::UnknownEnricher {
            kind,
            enricher: enricher_name.to_owned(),
        })
}

/// Returns `true` if a driver is registered for `(kind, enricher_name)`.
#[must_use]
pub fn has_enricher(kind: EnrichmentKind, enricher_name: &str) -> bool {
    let g = registry().read().expect("enrichment registry poisoned");
    let key: Arc<str> = Arc::from(enricher_name);
    g.enrichers.contains_key(&(kind, key))
}
