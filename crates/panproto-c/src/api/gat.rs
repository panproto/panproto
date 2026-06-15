//! GAT operations: theory construction, colimit, morphism checking,
//! model migration.
//!
//! Frozen-signature scaffold. Every entry point currently returns
//! [`PpStatus::Operation`]; the engine-wiring pass fills in the bodies
//! against `panproto_core::gat`.

use safer_ffi::prelude::*;

use crate::error::FfiError;
use crate::panic::guard;

/// Create a GAT theory from a CBOR spec.
///
/// `spec` is a CBOR-encoded `panproto_core::gat::Theory`. On success,
/// `out_handle` receives a fresh
/// [`Resource::Theory`](crate::handle::Resource) handle.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_gat_create_theory(spec: c_slice::Ref<'_, u8>, out_handle: &mut u32) -> i32 {
    let _ = (spec, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_gat_create_theory".into(),
        ))
    })
}

/// Compute the colimit of two theories over a shared base.
///
/// `t1`, `t2`, and `shared` are
/// [`Resource::Theory`](crate::handle::Resource) handles. On success,
/// `out_handle` receives a fresh
/// [`Resource::Theory`](crate::handle::Resource) handle. Will call
/// `gat::colimit_by_name`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_gat_colimit(t1: u32, t2: u32, shared: u32, out_handle: &mut u32) -> i32 {
    let _ = (t1, t2, shared, out_handle);
    guard(|| Err(FfiError::Operation("unimplemented: pp_gat_colimit".into())))
}

/// Check the validity of a theory morphism.
///
/// `morphism` is a CBOR-encoded `panproto_core::gat::TheoryMorphism`;
/// `domain` and `codomain` are
/// [`Resource::Theory`](crate::handle::Resource) handles. On success,
/// `out` receives a CBOR-encoded
/// [`MorphismCheckResult`](crate::api::helpers::MorphismCheckResult).
/// Will call `gat::check_morphism`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_gat_check_morphism(
    morphism: c_slice::Ref<'_, u8>,
    domain: u32,
    codomain: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (morphism, domain, codomain, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_gat_check_morphism".into(),
        ))
    })
}

/// Migrate a model through a theory morphism.
///
/// `model` is a CBOR-encoded sort-interpretation map
/// (`HashMap<String, Vec<ModelValue>>`; operation interpretations
/// cannot cross the boundary); `morphism` is a CBOR-encoded
/// `gat::TheoryMorphism`. On success, `out` receives the CBOR-encoded
/// reindexed sort interpretations.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_gat_migrate_model(
    model: c_slice::Ref<'_, u8>,
    morphism: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (model, morphism, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_gat_migrate_model".into(),
        ))
    })
}
