//! Homomorphism search and the theory -> schema -> data cascade.
//!
//! These entry points mirror the Python-only `panproto_py::hom` surface
//! (morphism search has no WASM analogue). Frozen-signature scaffold;
//! every entry point currently returns [`PpStatus::Operation`].

use safer_ffi::prelude::*;

use crate::error::FfiError;
use crate::panic::guard;

/// Find structure-preserving morphisms between two schemas.
///
/// `src` and `tgt` are [`Resource::Schema`](crate::handle::Resource)
/// handles. `opts` is a CBOR-encoded search-options record (anchors,
/// `monic`/`epic`/`iso` flags, `max_results`,
/// `relax_edge_name_pruning`) mirroring
/// `panproto_core::mig::hom_search::SearchOptions`. On success, `out`
/// receives a CBOR-encoded `Vec<FoundMorphism>` (each with `vertex_map`
/// and `quality`). Will call `hom_search::find_morphisms`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_hom_find_morphisms(
    src: u32,
    tgt: u32,
    opts: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (src, tgt, opts, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_hom_find_morphisms".into(),
        ))
    })
}

/// Find the single best-quality morphism between two schemas.
///
/// Arguments match [`pp_hom_find_morphisms`] except the search is capped
/// at one result. On success, `out` receives a CBOR-encoded
/// `Option<FoundMorphism>`. Will call `hom_search::find_best_morphism`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_hom_find_best_morphism(
    src: u32,
    tgt: u32,
    opts: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (src, tgt, opts, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_hom_find_best_morphism".into(),
        ))
    })
}

/// Convert a found morphism into a compiled migration.
///
/// `morphism` is a CBOR-encoded `FoundMorphism`. On success,
/// `out_handle` receives a fresh
/// [`Resource::Migration`](crate::handle::Resource) handle. Will call
/// `hom_search::morphism_to_migration`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_hom_morphism_to_migration(morphism: c_slice::Ref<'_, u8>, out_handle: &mut u32) -> i32 {
    let _ = (morphism, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_hom_morphism_to_migration".into(),
        ))
    })
}

/// Induce a schema morphism from a theory morphism and a source schema.
///
/// `theory_morphism` is a CBOR-encoded
/// `panproto_core::gat::TheoryMorphism`. `src` is a
/// [`Resource::Schema`](crate::handle::Resource) handle. On success,
/// `out` receives a CBOR-encoded `panproto_core::schema::SchemaMorphism`.
/// Will call `mig::cascade::induce_schema_morphism`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_hom_induce_schema_morphism(
    theory_morphism: c_slice::Ref<'_, u8>,
    src: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (theory_morphism, src, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_hom_induce_schema_morphism".into(),
        ))
    })
}

/// Induce a migration from a theory morphism and source/target schemas.
///
/// `theory_morphism` is a CBOR-encoded `gat::TheoryMorphism`; `src` and
/// `tgt` are [`Resource::Schema`](crate::handle::Resource) handles. On
/// success, `out` receives the CBOR-encoded induced `SchemaMorphism`
/// and `out_handle` receives a fresh
/// [`Resource::MigrationWithSchemas`](crate::handle::Resource) handle.
/// Will call `mig::cascade::induce_migration_from_theory`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_hom_induce_migration_from_theory(
    theory_morphism: c_slice::Ref<'_, u8>,
    src: u32,
    tgt: u32,
    out: &mut repr_c::Vec<u8>,
    out_handle: &mut u32,
) -> i32 {
    let _ = (theory_morphism, src, tgt, out, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_hom_induce_migration_from_theory".into(),
        ))
    })
}
