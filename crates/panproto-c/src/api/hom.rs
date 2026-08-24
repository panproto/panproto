//! Homomorphism search and the theory -> schema -> data cascade.
//!
//! These entry points mirror the Python-only `panproto_py::hom` surface
//! (morphism search has no WASM analogue). The `PyO3` wrapper passes
//! `SearchOptions` and `FoundMorphism` across the boundary as `PyO3`
//! classes; the C ABI passes them as CBOR. Because
//! `panproto_core::mig::hom_search::{SearchOptions, DomainConstraints,
//! FoundMorphism}`, `panproto_core::mig::span::SchemaSpan`,
//! `panproto_core::mig::CostWeights` and
//! `panproto_core::schema::SchemaOverlap` do not derive `serde`, the CBOR
//! payload types are the serializable shadow structs `SearchOptionsWire`,
//! `DomainConstraintsWire`, `CostWeightsWire`, `FoundMorphismWire`,
//! `SchemaSpanWire` and `SchemaOverlapWire` defined here (mirroring the
//! shadow-struct idiom in [`crate::api::helpers`]), converted to and from
//! the engine types at the boundary. `SchemaMorphism`, `TheoryMorphism`,
//! `Schema`, and `Migration` already derive `serde` and cross as
//! themselves.
//!
//! # The span is the primary search result
//!
//! [`pp_hom_find_span`] answers with a span `src ← apex → tgt`, and it is
//! total: a pair with nothing in common gets an empty apex rather than a
//! refusal. [`pp_hom_find_morphisms`] and [`pp_hom_find_best_morphism`] are
//! the total-morphism restriction of that same search and are empty exactly
//! when no total morphism exists, which on real schema pairs is the common
//! case. A host that wants to know what two schemas share asks for the span.
//!
//! The WASM `WasmError`/`JsError` pair becomes [`FfiError`], `rmp_serde`
//! becomes [`crate::canonical`] (CBOR via ciborium), and handle outputs
//! land in the slab as [`Resource::Migration`](crate::handle::Resource)
//! (for `morphism_to_migration`) or
//! [`Resource::MigrationWithSchemas`](crate::handle::Resource) (for the
//! cascade's `induce_migration_from_theory`).

use std::collections::HashMap;

use panproto_core::gat::{Name, TheoryMorphism};
use panproto_core::mig::{
    self, CostWeights, Migration, SchemaSpan, cascade,
    hom_search::{self, DomainConstraints, FoundMorphism, SearchOptions},
};
use panproto_core::schema::{Edge, Schema};
use safer_ffi::prelude::*;
use serde::{Deserialize, Serialize};

use crate::error::{FfiError, PpStatus};
use crate::handle::{self, Resource};
use crate::panic::guard;

// ---------------------------------------------------------------------------
// Serializable shadow structs (CBOR payload types)
// ---------------------------------------------------------------------------

/// Serializable mirror of
/// `panproto_core::mig::hom_search::SearchOptions`.
///
/// The engine type derives only `Clone`/`Debug`/`Default`, so it cannot
/// cross the FFI boundary directly. This shadow struct carries the same
/// fields with the same `snake_case` names the Haskell encoder pins
/// (`monic`, `epic`, `iso`, `max_results`, `hard_pins`). `serde(default)`
/// lets a producer omit any field, matching the engine's `Default`.
///
/// The five fields here are every field the engine's options type has, and
/// every one of them is honoured or refused rather than dropped: see `epic`,
/// which the span entry point rejects instead of ignoring. The
/// node budget is not among them: it lives on the engine's `SearchBudget`,
/// which the span search takes and the total-morphism entry points do not,
/// so carrying it here would give a host a knob two of the three entry
/// points that read this struct could not honour.
///
/// Hard *domain* restrictions are a separate struct on both sides:
/// [`DomainConstraintsWire`], which only [`pp_hom_find_span`] accepts.
#[derive(Debug, Default, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
struct SearchOptionsWire {
    /// Require an injective vertex map.
    #[serde(default)]
    monic: bool,
    /// Require a surjective vertex map.
    ///
    /// A property of a **total morphism**, so it is honoured by
    /// [`pp_hom_find_morphisms`] and [`pp_hom_find_best_morphism`] and refused
    /// by [`pp_hom_find_span`]: a span's right leg is deliberately partial and
    /// the span search never refuses for want of a match, so requiring the map
    /// to be onto would contradict that contract. Setting it on a span request
    /// returns `PpStatus::Operation` rather than an answer to a different
    /// question.
    #[serde(default)]
    epic: bool,
    /// Require a bijective vertex map (an isomorphism).
    #[serde(default)]
    iso: bool,
    /// Stop after this many morphisms, up to the engine's own cap of 1024.
    ///
    /// `0` is not unlimited: it asks for everything the search enumerates,
    /// which the same cap bounds. No value passed here is unbounded, because
    /// the engine materialises one morphism per optimum and the count of
    /// optima is not bounded by the schema pair's size.
    #[serde(default)]
    max_results: usize,
    /// Vertex mappings the caller knows and the search may not reconsider
    /// (the Python `anchors`).
    #[serde(default)]
    hard_pins: HashMap<String, String>,
}

impl SearchOptionsWire {
    /// Build the engine [`SearchOptions`], lifting the string-keyed
    /// `hard_pins` map into the `Name`-keyed form the search expects.
    fn into_engine(self) -> SearchOptions {
        let hard_pins = self
            .hard_pins
            .into_iter()
            .map(|(k, v)| (Name::from(k.as_str()), Name::from(v.as_str())))
            .collect();
        SearchOptions {
            monic: self.monic,
            epic: self.epic,
            iso: self.iso,
            max_results: self.max_results,
            hard_pins,
        }
    }
}

/// Serializable mirror of
/// `panproto_core::mig::hom_search::FoundMorphism`.
///
/// The engine type derives only `Clone`/`Debug`, so the C ABI exchanges
/// this shadow struct instead. The wire shape matches the Haskell
/// `FoundMorphism` codec exactly: a `vertex_map` (string-keyed, since a
/// `Name` is transparent over a string), an `edge_map` in the
/// `map_as_vec` array-of-pairs shape (the `Edge` key cannot be a CBOR
/// map key), and a `quality` double. `edge_map` is carried (unlike the
/// `PyO3` `to_dict`, which drops it) because `morphism_to_migration`
/// reconstructs the migration's edge mapping from it.
#[derive(Debug, Serialize, Deserialize)]
struct FoundMorphismWire {
    /// Vertex mapping: source vertex ID to target vertex ID.
    vertex_map: HashMap<Name, Name>,
    /// Edge mapping: source edge to target edge, as an array of pairs.
    #[serde(with = "panproto_core::schema::serde_helpers::map_as_vec")]
    edge_map: HashMap<Edge, Edge>,
    /// Quality score in `[0, 1]`.
    quality: f64,
}

impl From<FoundMorphism> for FoundMorphismWire {
    fn from(m: FoundMorphism) -> Self {
        Self {
            vertex_map: m.vertex_map,
            edge_map: m.edge_map,
            quality: m.quality,
        }
    }
}

impl From<FoundMorphismWire> for FoundMorphism {
    fn from(w: FoundMorphismWire) -> Self {
        Self {
            vertex_map: w.vertex_map,
            edge_map: w.edge_map,
            quality: w.quality,
        }
    }
}

/// Serializable mirror of `panproto_core::mig::CostWeights`.
///
/// The engine type keeps its five components private because it normalises
/// them at construction and rejects a vector that is negative, non-finite or
/// all zero. The wire therefore carries the raw components and
/// [`Self::into_engine`] runs that constructor, so a host cannot smuggle a
/// weight vector past the check.
#[derive(Debug, Serialize, Deserialize)]
struct CostWeightsWire {
    /// Weight on vertex-name agreement.
    name: f64,
    /// Weight on edge structure agreement.
    edge: f64,
    /// Weight on property-set agreement.
    prop: f64,
    /// Weight on degree agreement.
    degree: f64,
    /// Weight on anchor evidence.
    anchor: f64,
}

impl CostWeightsWire {
    /// Build the engine [`CostWeights`], which normalises the five
    /// components and checks their range.
    fn into_engine(self) -> Result<CostWeights, FfiError> {
        CostWeights::new(self.name, self.edge, self.prop, self.degree, self.anchor)
            .map_err(|e| FfiError::Operation(format!("scoring weights: {e}")))
    }
}

/// Serializable mirror of
/// `panproto_core::mig::hom_search::DomainConstraints`.
///
/// Every field is the caller stating which assignments are admissible, not a
/// heuristic filter. The two exclusion sets cross as CBOR arrays rather than
/// as sets, because CBOR has no set type; duplicates collapse on the way in.
/// `serde(default)` lets a producer omit any field, matching the engine's
/// `Default`, so an empty CBOR map is a valid payload meaning "no
/// restrictions".
#[derive(Debug, Default, Serialize, Deserialize)]
struct DomainConstraintsWire {
    /// For each source vertex, the only targets it may take.
    #[serde(default)]
    restricted_domains: HashMap<Name, Vec<Name>>,
    /// Target vertices no source vertex may map to.
    #[serde(default)]
    excluded_targets: Vec<Name>,
    /// Source vertices that must be left out of the apex.
    #[serde(default)]
    excluded_sources: Vec<Name>,
    /// Override the objective's component weights.
    #[serde(default)]
    scoring_weights: Option<CostWeightsWire>,
}

impl DomainConstraintsWire {
    /// Build the engine [`DomainConstraints`].
    ///
    /// # Errors
    ///
    /// [`FfiError::Operation`] when the scoring weights are present and the
    /// engine's constructor rejects them.
    fn into_engine(self) -> Result<DomainConstraints, FfiError> {
        let scoring_weights = self
            .scoring_weights
            .map(CostWeightsWire::into_engine)
            .transpose()?;
        Ok(DomainConstraints {
            restricted_domains: self.restricted_domains,
            excluded_targets: self.excluded_targets.into_iter().collect(),
            excluded_sources: self.excluded_sources.into_iter().collect(),
            scoring_weights,
        })
    }
}

/// Serializable mirror of `panproto_core::mig::span::SchemaSpan`.
///
/// The span is `src ←left─ apex ─right→ tgt`. The apex is a `Schema` and the
/// two legs are `Migration`s, all three of which derive `serde` and cross as
/// themselves; the rest of the wire is a selection of the span's measurements,
/// flattened so that a host reads doubles, booleans and a hex string rather
/// than a nested record it has no other use for.
///
/// It is a selection rather than the whole certificate. Four of the
/// certificate's fields cross: `proven_optimal`, `is_total` (which reads the
/// leg shape), `apex_digest` and `legs_are_functorial`. The other six do not:
/// `left_existence`, `right_existence`, `apex_pointed`, `path`,
/// `tie_break_order` and `limit_hit` have no wire form here, so a host cannot
/// tell a search the budget cut from one that finished.
///
/// `quality` is a ranking signal among spans over one source schema and
/// nothing else: every denominator of the objective is fixed by `src`, so
/// comparing it across source schemas compares two different scales. An empty
/// apex charges the full penalty on each component the source gives mass to,
/// and a source gives a component mass only when it has something for that
/// component to measure, so the floor moves with the source's shape: `0.0` over
/// a source with at least one named edge, `0.30` over a source whose edges are
/// all unnamed, `0.55` over an edgeless source, and `1.0` over an empty source.
/// Each is the worst reading on its own source's scale rather than a verdict,
/// which is why a host ranking pairs must read `apex_coverage` alongside it.
#[derive(Debug, Serialize, Deserialize)]
struct SchemaSpanWire {
    /// The apex: the sub-schema of `src` the search gave targets to.
    apex: Schema,
    /// `left : apex -> src`, an inclusion.
    left: Migration,
    /// `right : apex -> tgt`.
    right: Migration,
    /// `1 - quality_cost / COST_SCALE`, excluding the drop count.
    quality: f64,
    /// Lower end of the interval bracketing `quality`.
    quality_lo: f64,
    /// Upper end of the interval bracketing `quality`. Equal to
    /// `quality_lo` exactly when `proven_optimal` holds.
    quality_hi: f64,
    /// `|apex.vertices| / |src.vertices|`, or one when the source is empty.
    apex_coverage: f64,
    /// Whether the search proved its answer optimal.
    proven_optimal: bool,
    /// Whether the left leg is onto, which makes the span a total morphism.
    is_total: bool,
    /// The apex's content digest, lower-case hex.
    ///
    /// Together with the two leg maps this is the span's identity, which is
    /// what a host needs to dedupe or cache one. There is no schema-digest
    /// entry point on this surface and the CBOR `pp_schema_to_cbor` hands out is
    /// not the digest's pre-image, so a host cannot recompute it: without this
    /// field it is unreachable rather than merely inconvenient.
    ///
    /// Defaulted on decode, because this struct is bidirectional.
    /// `pp_hom_span_to_overlap` takes a span a host encoded, and hosts written
    /// against the nine-field form must keep working.
    #[serde(default)]
    apex_digest: String,
    /// Whether both legs passed the schema-morphism check.
    ///
    /// Defaulted on decode, for the same reason as `apex_digest`.
    #[serde(default)]
    legs_are_functorial: bool,
}

impl From<SchemaSpan> for SchemaSpanWire {
    fn from(span: SchemaSpan) -> Self {
        let (quality_lo, quality_hi) = span.quality_bounds;
        Self {
            is_total: span.is_total(),
            proven_optimal: span.certificate.proven_optimal,
            apex_digest: span.apex_digest_hex(),
            legs_are_functorial: span.certificate.legs_are_functorial,
            apex: span.apex,
            left: span.left,
            right: span.right,
            quality: span.quality,
            quality_lo,
            quality_hi,
            apex_coverage: span.apex_coverage,
        }
    }
}

/// Serializable mirror of `panproto_core::schema::SchemaOverlap`.
///
/// The pair lists are what `schema::schema_pushout` takes, each pair being
/// `(source element, target element)`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct SchemaOverlapWire {
    /// Vertex pairs identified by the pushout.
    vertex_pairs: Vec<(Name, Name)>,
    /// Edge pairs identified by the pushout.
    edge_pairs: Vec<(Edge, Edge)>,
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// Find structure-preserving morphisms between two schemas.
///
/// `src` and `tgt` are [`Resource::Schema`](crate::handle::Resource)
/// handles. `opts` is a CBOR-encoded `SearchOptionsWire` mirroring
/// `panproto_core::mig::hom_search::SearchOptions`. On success, `out`
/// receives a CBOR-encoded `Vec<FoundMorphismWire>` (each with
/// `vertex_map`, `edge_map`, and `quality`). Calls
/// `hom_search::find_morphisms`.
///
/// # This returns the optima, not the whole hom-set
///
/// It returns the morphisms **attaining the optimum**, capped by
/// `max_results`, and nothing else. Every element therefore carries the same
/// quality, which is the maximum over all total morphisms, so the list is in
/// non-increasing quality order trivially and a host reading element zero gets
/// what it always got. A host that walked the list for a suboptimal
/// alternative will not find one: there is no k-best over distinct quality
/// levels. Empty means no total morphism exists, and only that: a search that
/// could not be posed returns `PpStatus::Operation` with the reason, rather
/// than an empty list under a success status.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_hom_find_morphisms(
    src: u32,
    tgt: u32,
    opts: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let wire: SearchOptionsWire = crate::canonical::decode(opts.as_slice())?;
        let options = wire.into_engine();

        let (src_schema, tgt_schema) = handle::with_two_resources(src, tgt, |r1, r2| {
            Ok((r1.as_schema_arc()?, r2.as_schema_arc()?))
        })?;

        let found = hom_search::find_morphisms(&src_schema, &tgt_schema, &options)
            .map_err(|e| FfiError::Operation(format!("find_morphisms: {e}")))?;
        let wire_results: Vec<FoundMorphismWire> = found
            .morphisms
            .into_iter()
            .map(FoundMorphismWire::from)
            .collect();

        let bytes = crate::canonical::encode(&wire_results)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Find the single best-quality morphism between two schemas.
///
/// Arguments match [`pp_hom_find_morphisms`]. On success, `out`
/// receives a CBOR-encoded `Option<FoundMorphismWire>`. Calls
/// `hom_search::find_best_morphism`.
///
/// A CBOR `null` means no total morphism exists. A search that could not be
/// posed returns `PpStatus::Operation` instead, so a host is never told "no
/// morphism exists" about a pair the engine could not search.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_hom_find_best_morphism(
    src: u32,
    tgt: u32,
    opts: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let wire: SearchOptionsWire = crate::canonical::decode(opts.as_slice())?;
        let options = wire.into_engine();

        let (src_schema, tgt_schema) = handle::with_two_resources(src, tgt, |r1, r2| {
            Ok((r1.as_schema_arc()?, r2.as_schema_arc()?))
        })?;

        let best = hom_search::find_best_morphism(&src_schema, &tgt_schema, &options)
            .map_err(|e| FfiError::Operation(format!("find_best_morphism: {e}")))?
            .map(FoundMorphismWire::from);

        let bytes = crate::canonical::encode(&best)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Find the maximum span between two schemas.
///
/// `src` and `tgt` are [`Resource::Schema`](crate::handle::Resource) handles
/// and `protocol` is a [`Resource::Protocol`](crate::handle::Resource)
/// handle: the apex is a schema, a schema is well formed only against a
/// protocol, and inducing the apex re-validates it rather than assuming it,
/// so the protocol is an argument rather than something read off the source
/// (a schema stores only its protocol's name). `opts` is a CBOR-encoded
/// `SearchOptionsWire` and `constraints` a CBOR-encoded
/// `DomainConstraintsWire`; an empty CBOR map is a valid payload for either.
/// On success, `out` receives a CBOR-encoded `SchemaSpanWire`. Calls
/// `hom_search::find_span_constrained`.
///
/// # This never refuses for want of a match
///
/// Leaving every source vertex out of the apex is always feasible, so two
/// schemas with nothing in common get an empty apex, not an error. A non-`Ok`
/// status here means the search could not be posed or the induced apex is not
/// a schema, both of which are defects rather than answers.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_hom_find_span(
    src: u32,
    tgt: u32,
    protocol: u32,
    opts: c_slice::Ref<'_, u8>,
    constraints: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let opts_wire: SearchOptionsWire = crate::canonical::decode(opts.as_slice())?;
        let options = opts_wire.into_engine();

        let constraints_wire: DomainConstraintsWire =
            crate::canonical::decode(constraints.as_slice())?;
        let domain = constraints_wire.into_engine()?;

        let (src_schema, tgt_schema, proto) =
            handle::with_three_resources(src, tgt, protocol, |r1, r2, r3| {
                Ok((
                    r1.as_schema_arc()?,
                    r2.as_schema_arc()?,
                    r3.as_protocol()?.clone(),
                ))
            })?;

        let span =
            hom_search::find_span_constrained(&src_schema, &tgt_schema, &proto, &options, &domain)
                .map_err(|e| FfiError::Operation(format!("find_span: {e}")))?;

        let bytes = crate::canonical::encode(&SchemaSpanWire::from(span))?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Read a span's apex as the identification list a pushout takes.
///
/// `span` is a CBOR-encoded `SchemaSpanWire`, as [`pp_hom_find_span`] wrote
/// it. On success, `out` receives a CBOR-encoded `SchemaOverlapWire`: the
/// right leg's two maps as `(source element, target element)` pairs, sorted
/// by key so that one span always yields the same bytes. Feeding those pairs
/// to a pushout merges `src` and `tgt` along the apex.
///
/// The left leg is an inclusion, so the apex's own identifiers *are* source
/// identifiers and the right leg alone carries the identification.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_hom_span_to_overlap(span: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let wire: SchemaSpanWire = crate::canonical::decode(span.as_slice())?;

        let mut vertex_pairs: Vec<(Name, Name)> = wire.right.vertex_map.into_iter().collect();
        vertex_pairs.sort_unstable();

        let mut edge_pairs: Vec<(Edge, Edge)> = wire.right.edge_map.into_iter().collect();
        edge_pairs.sort_unstable();

        let overlap = SchemaOverlapWire {
            vertex_pairs,
            edge_pairs,
        };

        let bytes = crate::canonical::encode(&overlap)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Convert a found morphism into a compiled migration.
///
/// `morphism` is a CBOR-encoded `FoundMorphismWire`. The found
/// morphism is lowered to a `mig::Migration` via
/// `hom_search::morphism_to_migration`, then compiled against the
/// minimal schemas implied by its surviving vertex and edge sets. On
/// success, `out_handle` receives a fresh
/// [`Resource::Migration`](crate::handle::Resource) handle.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_hom_morphism_to_migration(morphism: c_slice::Ref<'_, u8>, out_handle: &mut u32) -> i32 {
    guard(|| {
        let wire: FoundMorphismWire = crate::canonical::decode(morphism.as_slice())?;
        let found: FoundMorphism = wire.into();

        let migration: Migration = hom_search::morphism_to_migration(&found);

        // The found morphism is total on the matched sub-schema, so the
        // surviving vertex/edge sets recover the source and target.
        // Build minimal anchoring schemas from those sets, then compile
        // (the same `mig::compile` the `mig` domain uses) so the handle
        // is an applyable `CompiledMigration`.
        let (src_schema, tgt_schema) = minimal_schemas_for_migration(&migration);

        let compiled = mig::compile(&src_schema, &tgt_schema, &migration)
            .map_err(|e| FfiError::Operation(format!("compile: {e}")))?;

        *out_handle = handle::alloc(Resource::Migration(Box::new(compiled)));
        Ok(PpStatus::Ok)
    })
}

/// Induce a schema morphism from a theory morphism and a source schema.
///
/// `theory_morphism` is a CBOR-encoded
/// `panproto_core::gat::TheoryMorphism`. `src` is a
/// [`Resource::Schema`](crate::handle::Resource) handle. On success,
/// `out` receives a CBOR-encoded `panproto_core::schema::SchemaMorphism`.
/// Calls `mig::cascade::induce_schema_morphism`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_hom_induce_schema_morphism(
    theory_morphism: c_slice::Ref<'_, u8>,
    src: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let theory_morph: TheoryMorphism = crate::canonical::decode(theory_morphism.as_slice())?;

        let src_schema = handle::with_resource(src, Resource::as_schema_arc)?;

        let schema_morph = cascade::induce_schema_morphism(&theory_morph, &src_schema);

        let bytes = crate::canonical::encode(&schema_morph)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Induce a migration from a theory morphism and source/target schemas.
///
/// `theory_morphism` is a CBOR-encoded `gat::TheoryMorphism`; `src` and
/// `tgt` are [`Resource::Schema`](crate::handle::Resource) handles. On
/// success, `out` receives the CBOR-encoded induced `SchemaMorphism`
/// and `out_handle` receives a fresh
/// [`Resource::MigrationWithSchemas`](crate::handle::Resource) handle
/// (the compiled `Delta_F` pullback bundled with its anchoring schemas).
/// Calls `mig::cascade::induce_migration_from_theory`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_hom_induce_migration_from_theory(
    theory_morphism: c_slice::Ref<'_, u8>,
    src: u32,
    tgt: u32,
    out: &mut repr_c::Vec<u8>,
    out_handle: &mut u32,
) -> i32 {
    guard(|| {
        let theory_morph: TheoryMorphism = crate::canonical::decode(theory_morphism.as_slice())?;

        let (src_schema, tgt_schema) = handle::with_two_resources(src, tgt, |r1, r2| {
            Ok((r1.as_schema_arc()?, r2.as_schema_arc()?))
        })?;

        let (schema_morph, compiled) =
            cascade::induce_migration_from_theory(&theory_morph, &src_schema, &tgt_schema);

        // Encode the schema morphism (the only fallible step) before
        // allocating the handle, so a serialization failure cannot leak a
        // slab slot the caller never learns about.
        let bytes = crate::canonical::encode(&schema_morph)?;

        *out_handle = handle::alloc(Resource::MigrationWithSchemas {
            compiled: Box::new(compiled),
            src_schema,
            tgt_schema,
        });

        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build minimal source and target schemas that anchor `migration`.
///
/// A migration produced by `morphism_to_migration` carries no resolver
/// tables, only the vertex and edge maps. Its domain keys describe the
/// source structure and its range values the target structure; compiling
/// against schemas reconstructed from those keys (source) and values
/// (target) yields an applyable `CompiledMigration` without the caller
/// having to supply the original schemas (which the C ABI does not
/// receive for this entry point). Vertex kinds are unknown here, so a
/// uniform `unknown` kind is used; the compile path keys off the vertex
/// and edge maps, not the kinds.
fn minimal_schemas_for_migration(
    migration: &Migration,
) -> (panproto_core::schema::Schema, panproto_core::schema::Schema) {
    use panproto_core::schema::{Schema, Vertex};
    use smallvec::SmallVec;

    fn schema_from(
        vertices: impl Iterator<Item = Name>,
        edges: impl Iterator<Item = Edge>,
    ) -> Schema {
        let mut vmap = HashMap::new();
        for v in vertices {
            vmap.entry(v.clone()).or_insert_with(|| Vertex {
                id: v.clone(),
                kind: "unknown".into(),
                nsid: None,
            });
        }

        let mut emap = HashMap::new();
        let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

        for e in edges {
            // Endpoints of every edge must exist as vertices.
            vmap.entry(e.src.clone()).or_insert_with(|| Vertex {
                id: e.src.clone(),
                kind: "unknown".into(),
                nsid: None,
            });
            vmap.entry(e.tgt.clone()).or_insert_with(|| Vertex {
                id: e.tgt.clone(),
                kind: "unknown".into(),
                nsid: None,
            });
            emap.insert(e.clone(), e.kind.clone());
            outgoing.entry(e.src.clone()).or_default().push(e.clone());
            incoming.entry(e.tgt.clone()).or_default().push(e.clone());
            between
                .entry((e.src.clone(), e.tgt.clone()))
                .or_default()
                .push(e.clone());
        }

        Schema {
            protocol: String::new(),
            vertices: vmap,
            edges: emap,
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing,
            incoming,
            between,
        }
    }

    let src = schema_from(
        migration.vertex_map.keys().cloned(),
        migration.edge_map.keys().cloned(),
    );
    let tgt = schema_from(
        migration.vertex_map.values().cloned(),
        migration.edge_map.values().cloned(),
    );
    (src, tgt)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use panproto_core::gat::{Name, TheoryMorphism};
    use panproto_core::mig::Migration;
    use panproto_core::schema::{Schema, SchemaBuilder, SchemaMorphism};

    use super::*;
    use crate::api::{pp_buf_free, pp_handle_free};
    use crate::canonical::{decode, encode};
    use crate::handle::Resource;

    /// A two-vertex source schema: a `post` record with a `text` string
    /// property.
    fn source_schema() -> Schema {
        let proto = crate::api::helpers::default_protocol("test");
        SchemaBuilder::new(&proto)
            .vertex("post", "record", None::<&str>)
            .unwrap()
            .vertex("text", "string", None::<&str>)
            .unwrap()
            .edge("post", "text", "prop", Some("text"))
            .unwrap()
            .build()
            .unwrap()
    }

    /// A target schema isomorphic to the source but with the record
    /// vertex renamed to `note` (the property keeps its `text` label).
    fn target_schema() -> Schema {
        let proto = crate::api::helpers::default_protocol("test");
        SchemaBuilder::new(&proto)
            .vertex("note", "record", None::<&str>)
            .unwrap()
            .vertex("text", "string", None::<&str>)
            .unwrap()
            .edge("note", "text", "prop", Some("text"))
            .unwrap()
            .build()
            .unwrap()
    }

    /// A target schema sharing only the record vertex: the `text` property
    /// has nowhere to go, so no total morphism exists and the span is the
    /// only answer.
    fn lossy_target_schema() -> Schema {
        let proto = crate::api::helpers::default_protocol("test");
        SchemaBuilder::new(&proto)
            .vertex("note", "record", None::<&str>)
            .unwrap()
            .build()
            .unwrap()
    }

    fn alloc_schema(s: &Schema) -> u32 {
        handle::alloc(Resource::Schema(Arc::new(s.clone())))
    }

    fn alloc_test_protocol() -> u32 {
        handle::alloc(Resource::Protocol(Box::new(
            crate::api::helpers::default_protocol("test"),
        )))
    }

    /// The all-default domain constraints, CBOR-encoded.
    fn default_constraints_bytes() -> Vec<u8> {
        encode(&DomainConstraintsWire::default()).unwrap()
    }

    /// Run `pp_hom_find_span` over the two schemas under the given options
    /// and constraints, answering with the decoded span.
    fn find_span_wire(
        src: &Schema,
        tgt: &Schema,
        opts: &[u8],
        constraints: &[u8],
    ) -> SchemaSpanWire {
        let src_h = alloc_schema(src);
        let tgt_h = alloc_schema(tgt);
        let proto_h = alloc_test_protocol();

        let opts = slice(opts);
        let constraints = slice(constraints);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_find_span(
            src_h,
            tgt_h,
            proto_h,
            opts.as_ref(),
            constraints.as_ref(),
            &mut out,
        );
        assert_eq!(status, PpStatus::Ok as i32);
        let span: SchemaSpanWire = decode(&out).unwrap();
        pp_buf_free(out);

        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(proto_h), PpStatus::Ok as i32);
        span
    }

    fn slice(bytes: &[u8]) -> c_slice::Box<u8> {
        bytes.to_vec().into_boxed_slice().into()
    }

    /// The all-default search options, CBOR-encoded.
    fn default_opts_bytes() -> Vec<u8> {
        encode(&SearchOptionsWire::default()).unwrap()
    }

    /// A theory morphism that renames the `prop` operation to `field`.
    fn rename_prop_morphism() -> TheoryMorphism {
        TheoryMorphism::new(
            "rename_prop",
            "test",
            "test",
            HashMap::new(),
            HashMap::from([(std::sync::Arc::from("prop"), std::sync::Arc::from("field"))]),
        )
    }

    #[test]
    fn search_options_wire_round_trips() {
        let wire = SearchOptionsWire {
            monic: true,
            epic: false,
            iso: true,
            max_results: 3,
            hard_pins: HashMap::from([("a".to_string(), "b".to_string())]),
        };
        let bytes = encode(&wire).unwrap();
        let back: SearchOptionsWire = decode(&bytes).unwrap();
        let engine = back.into_engine();
        assert!(engine.monic);
        assert!(engine.iso);
        assert_eq!(engine.max_results, 3);
        assert_eq!(
            engine.hard_pins.get(&Name::from("a")),
            Some(&Name::from("b"))
        );
    }

    #[test]
    fn search_options_wire_tolerates_missing_fields() {
        // An empty CBOR map decodes to all-default options.
        let bytes = encode(&HashMap::<String, bool>::new()).unwrap();
        let wire: SearchOptionsWire = decode(&bytes).unwrap();
        let engine = wire.into_engine();
        assert!(!engine.monic);
        assert_eq!(engine.max_results, 0);
        assert!(engine.hard_pins.is_empty());
    }

    #[test]
    fn find_morphisms_finds_the_rename() {
        let src = source_schema();
        let tgt = target_schema();
        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);

        let opts = slice(&default_opts_bytes());
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_find_morphisms(src_h, tgt_h, opts.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Ok as i32);

        let results: Vec<FoundMorphismWire> = decode(&out).unwrap();
        assert!(!results.is_empty(), "expected at least one morphism");
        // The post -> note rename should appear in some result's vertex map.
        let maps_post_to_note = results
            .iter()
            .any(|m| m.vertex_map.get(&Name::from("post")) == Some(&Name::from("note")));
        assert!(maps_post_to_note, "expected a post -> note mapping");
        // Every result attains the optimum, so they all carry one
        // quality. `find_morphisms_max_results_zero_returns_one` is what
        // pins the count; this walk passes vacuously on one element.
        for pair in results.windows(2) {
            assert!((pair[0].quality - pair[1].quality).abs() < f64::EPSILON);
        }
        pp_buf_free(out);

        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    /// `max_results = 0` no longer means "the whole hom-set".
    ///
    /// This pair admits several total morphisms and exactly one optimum, so
    /// an unlimited request answers with one element. The
    /// `find_morphisms_finds_the_rename` monotone-quality walk cannot catch
    /// the difference, because `windows(2)` over a one-element list is empty.
    #[test]
    fn find_morphisms_max_results_zero_returns_one() {
        let src = source_schema();
        let tgt = target_schema();
        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);

        let unlimited = encode(&SearchOptionsWire {
            max_results: 0,
            ..SearchOptionsWire::default()
        })
        .unwrap();
        let opts = slice(&unlimited);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_find_morphisms(src_h, tgt_h, opts.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Ok as i32);

        let results: Vec<FoundMorphismWire> = decode(&out).unwrap();
        assert_eq!(
            results.len(),
            1,
            "an unlimited request answers with the optima, not the hom-set"
        );
        pp_buf_free(out);

        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    /// The head of the list and the best-morphism answer are the same
    /// morphism, not merely the same quality.
    #[test]
    fn find_morphisms_top_matches_find_best() {
        let src = source_schema();
        let tgt = target_schema();
        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);

        let opts = slice(&default_opts_bytes());
        let mut list_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_hom_find_morphisms(src_h, tgt_h, opts.as_ref(), &mut list_out),
            PpStatus::Ok as i32
        );
        let results: Vec<FoundMorphismWire> = decode(&list_out).unwrap();
        pp_buf_free(list_out);

        let opts = slice(&default_opts_bytes());
        let mut best_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_hom_find_best_morphism(src_h, tgt_h, opts.as_ref(), &mut best_out),
            PpStatus::Ok as i32
        );
        let best: Option<FoundMorphismWire> = decode(&best_out).unwrap();
        pp_buf_free(best_out);

        let best = best.expect("the pair admits a total morphism");
        let top = results.first().expect("so the list is not empty");
        assert!((top.quality - best.quality).abs() < f64::EPSILON);
        assert_eq!(top.vertex_map, best.vertex_map);
        assert_eq!(top.edge_map, best.edge_map);

        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn find_best_morphism_returns_some() {
        let src = source_schema();
        let tgt = target_schema();
        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);

        let opts = slice(&default_opts_bytes());
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_find_best_morphism(src_h, tgt_h, opts.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Ok as i32);

        let best: Option<FoundMorphismWire> = decode(&out).unwrap();
        let best = best.expect("expected a best morphism");
        assert!((0.0..=1.0).contains(&best.quality));
        pp_buf_free(out);

        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn find_span_over_a_rename_is_total() {
        let span = find_span_wire(
            &source_schema(),
            &target_schema(),
            &default_opts_bytes(),
            &default_constraints_bytes(),
        );

        assert!(span.is_total, "every source vertex has an image");
        assert!((span.apex_coverage - 1.0).abs() < f64::EPSILON);
        assert_eq!(span.apex.vertices.len(), 2);
        assert_eq!(
            span.right.vertex_map.get(&Name::from("post")),
            Some(&Name::from("note"))
        );
        assert!(span.proven_optimal);
        assert!(span.quality_lo <= span.quality && span.quality <= span.quality_hi);
    }

    #[test]
    fn find_span_answers_where_no_total_morphism_exists() {
        let src = source_schema();
        let tgt = lossy_target_schema();

        // The total-morphism entry point has nothing to say here.
        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);
        let opts = slice(&default_opts_bytes());
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_hom_find_best_morphism(src_h, tgt_h, opts.as_ref(), &mut out),
            PpStatus::Ok as i32
        );
        let best: Option<FoundMorphismWire> = decode(&out).unwrap();
        assert!(best.is_none(), "the text property has nowhere to go");
        pp_buf_free(out);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);

        // The span answers with what the two schemas do share.
        let span = find_span_wire(
            &src,
            &tgt,
            &default_opts_bytes(),
            &default_constraints_bytes(),
        );
        assert!(!span.is_total);
        assert_eq!(span.apex.vertices.len(), 1);
        assert!((span.apex_coverage - 0.5).abs() < f64::EPSILON);
        assert_eq!(
            span.right.vertex_map.get(&Name::from("post")),
            Some(&Name::from("note"))
        );
    }

    #[test]
    fn find_span_honours_excluded_sources() {
        let constraints = encode(&DomainConstraintsWire {
            excluded_sources: vec![Name::from("text")],
            ..DomainConstraintsWire::default()
        })
        .unwrap();

        let span = find_span_wire(
            &source_schema(),
            &target_schema(),
            &default_opts_bytes(),
            &constraints,
        );

        assert!(!span.is_total, "an excluded source cannot be in the apex");
        assert!(!span.right.vertex_map.contains_key(&Name::from("text")));
    }

    #[test]
    fn find_span_rejects_unusable_scoring_weights() {
        let constraints = encode(&DomainConstraintsWire {
            scoring_weights: Some(CostWeightsWire {
                name: 0.0,
                edge: 0.0,
                prop: 0.0,
                degree: 0.0,
                anchor: 0.0,
            }),
            ..DomainConstraintsWire::default()
        })
        .unwrap();

        let src_h = alloc_schema(&source_schema());
        let tgt_h = alloc_schema(&target_schema());
        let proto_h = alloc_test_protocol();
        let opts = slice(&default_opts_bytes());
        let constraints = slice(&constraints);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_find_span(
            src_h,
            tgt_h,
            proto_h,
            opts.as_ref(),
            constraints.as_ref(),
            &mut out,
        );
        assert_eq!(status, PpStatus::Operation as i32);
        pp_buf_free(out);

        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(proto_h), PpStatus::Ok as i32);
    }

    #[test]
    fn find_span_rejects_a_non_protocol_third_handle() {
        let src = source_schema();
        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&target_schema());
        // A schema handle where a protocol handle belongs.
        let opts = slice(&default_opts_bytes());
        let constraints = slice(&default_constraints_bytes());
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_find_span(
            src_h,
            tgt_h,
            src_h,
            opts.as_ref(),
            constraints.as_ref(),
            &mut out,
        );
        assert_eq!(status, PpStatus::TypeMismatch as i32);
        pp_buf_free(out);

        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    /// The overlap the C ABI projects out of a span's wire form is the one
    /// the engine's own `SchemaSpan::to_overlap` computes, pair for pair and
    /// in the same order.
    #[test]
    fn span_to_overlap_matches_the_engine() {
        let src = source_schema();
        let tgt = target_schema();
        let proto = crate::api::helpers::default_protocol("test");

        let engine_span = hom_search::find_span(&src, &tgt, &proto, &SearchOptions::default())
            .expect("the span search is total");
        let engine_overlap = engine_span.to_overlap();

        let span_bytes = encode(&SchemaSpanWire::from(engine_span)).unwrap();
        let span = slice(&span_bytes);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_span_to_overlap(span.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Ok as i32);
        let wire: SchemaOverlapWire = decode(&out).unwrap();
        pp_buf_free(out);

        assert_eq!(wire.vertex_pairs, engine_overlap.vertex_pairs);
        assert_eq!(wire.edge_pairs, engine_overlap.edge_pairs);
        assert!(!wire.vertex_pairs.is_empty());
    }

    #[test]
    fn span_to_overlap_rejects_garbage() {
        let bad = slice(&[0xFFu8, 0xFE, 0xFD]);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_span_to_overlap(bad.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Serialization as i32);
        pp_buf_free(out);
    }

    #[test]
    fn morphism_to_migration_yields_a_migration_handle() {
        let src = source_schema();
        let tgt = target_schema();
        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);

        // First find a best morphism, then convert it to a migration handle.
        let opts = slice(&default_opts_bytes());
        let mut best_out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_find_best_morphism(src_h, tgt_h, opts.as_ref(), &mut best_out);
        assert_eq!(status, PpStatus::Ok as i32);
        let best_bytes = best_out.to_vec();
        pp_buf_free(best_out);

        // `best_bytes` encodes Option<FoundMorphismWire>; unwrap to the
        // FoundMorphismWire payload for the migration call.
        let best: Option<FoundMorphismWire> = decode(&best_bytes).unwrap();
        let best = best.expect("expected a morphism");
        let morphism_bytes = encode(&best).unwrap();

        let morphism = slice(&morphism_bytes);
        let mut mig_h: u32 = u32::MAX;
        let status = pp_hom_morphism_to_migration(morphism.as_ref(), &mut mig_h);
        assert_eq!(status, PpStatus::Ok as i32);
        assert_ne!(mig_h, u32::MAX);

        // The handle is an applyable Migration resource.
        let is_migration = handle::with_resource(mig_h, |r| Ok(r.as_migration().is_ok())).unwrap();
        assert!(is_migration);

        assert_eq!(pp_handle_free(mig_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn morphism_to_migration_handles_empty_morphism() {
        // A FoundMorphism with empty maps lowers to an empty migration;
        // compiling against empty schemas must still succeed.
        let wire = FoundMorphismWire {
            vertex_map: HashMap::new(),
            edge_map: HashMap::new(),
            quality: 1.0,
        };
        let bytes = encode(&wire).unwrap();
        let morphism = slice(&bytes);
        let mut mig_h: u32 = u32::MAX;
        let status = pp_hom_morphism_to_migration(morphism.as_ref(), &mut mig_h);
        assert_eq!(status, PpStatus::Ok as i32);
        assert_ne!(mig_h, u32::MAX);
        assert_eq!(pp_handle_free(mig_h), PpStatus::Ok as i32);
    }

    #[test]
    fn induce_schema_morphism_round_trips() {
        let src = source_schema();
        let src_h = alloc_schema(&src);

        let theory = rename_prop_morphism();
        let theory_bytes = encode(&theory).unwrap();
        let theory_slice = slice(&theory_bytes);

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_induce_schema_morphism(theory_slice.as_ref(), src_h, &mut out);
        assert_eq!(status, PpStatus::Ok as i32);

        let schema_morph: SchemaMorphism = decode(&out).unwrap();
        // Vertices are preserved with identical IDs.
        for (s, t) in &schema_morph.vertex_map {
            assert_eq!(s, t);
        }
        // The `prop` edge kind is renamed to `field`.
        let renamed = schema_morph
            .edge_map
            .iter()
            .any(|(s, t)| s.kind.as_ref() == "prop" && t.kind.as_ref() == "field");
        assert!(renamed, "expected prop -> field edge kind rename");
        pp_buf_free(out);

        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
    }

    #[test]
    fn induce_migration_from_theory_yields_morphism_and_handle() {
        let src = source_schema();
        let tgt = target_schema();
        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);

        let theory = rename_prop_morphism();
        let theory_bytes = encode(&theory).unwrap();
        let theory_slice = slice(&theory_bytes);

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let mut mig_h: u32 = u32::MAX;
        let status = pp_hom_induce_migration_from_theory(
            theory_slice.as_ref(),
            src_h,
            tgt_h,
            &mut out,
            &mut mig_h,
        );
        assert_eq!(status, PpStatus::Ok as i32);
        assert_ne!(mig_h, u32::MAX);

        let schema_morph: SchemaMorphism = decode(&out).unwrap();
        assert_eq!(schema_morph.vertex_map.len(), src.vertices.len());
        pp_buf_free(out);

        // The handle is a MigrationWithSchemas resource (accepts both
        // migration projection and carries the bundled schemas).
        let kind = handle::with_resource(mig_h, |r| Ok(r.type_name())).unwrap();
        assert_eq!(kind, "MigrationWithSchemas");
        assert!(handle::with_resource(mig_h, |r| Ok(r.as_migration().is_ok())).unwrap());

        assert_eq!(pp_handle_free(mig_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn find_morphisms_rejects_non_schema_handle() {
        let proto = crate::api::helpers::default_protocol("p");
        let proto_h = handle::alloc(Resource::Protocol(Box::new(proto)));
        let src = source_schema();
        let src_h = alloc_schema(&src);

        let opts = slice(&default_opts_bytes());
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_find_morphisms(proto_h, src_h, opts.as_ref(), &mut out);
        assert_eq!(status, PpStatus::TypeMismatch as i32);
        pp_buf_free(out);

        assert_eq!(pp_handle_free(proto_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
    }

    #[test]
    fn find_morphisms_rejects_invalid_handle() {
        let opts = slice(&default_opts_bytes());
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_find_morphisms(u32::MAX - 1, u32::MAX - 2, opts.as_ref(), &mut out);
        assert_eq!(status, PpStatus::InvalidHandle as i32);
        pp_buf_free(out);
    }

    #[test]
    fn find_morphisms_rejects_garbage_opts() {
        let src = source_schema();
        let tgt = target_schema();
        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);

        let bad = slice(&[0xFFu8, 0xFE, 0xFD]);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_hom_find_morphisms(src_h, tgt_h, bad.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Serialization as i32);
        pp_buf_free(out);

        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn morphism_to_migration_lowers_maps() {
        // Sanity-check the lowering: morphism_to_migration carries the
        // vertex/edge maps straight across, which our minimal schema
        // builder then anchors.
        let mut vertex_map = HashMap::new();
        vertex_map.insert(Name::from("post"), Name::from("note"));
        let found = FoundMorphism {
            vertex_map,
            edge_map: HashMap::new(),
            quality: 0.9,
        };
        let migration: Migration = hom_search::morphism_to_migration(&found);
        let (src, tgt) = minimal_schemas_for_migration(&migration);
        assert!(src.vertices.contains_key(&Name::from("post")));
        assert!(tgt.vertices.contains_key(&Name::from("note")));
    }
}
