//! Enriched theories: schema coercions, defaults, mergers, conflict
//! policies, and refinement subsorting.
//!
//! Frozen-signature scaffold. Every entry point currently returns
//! [`PpStatus::Operation`]; the engine-wiring pass fills in the bodies
//! against `panproto_core::{schema, lens}`.

use safer_ffi::prelude::*;

use crate::error::FfiError;
use crate::panic::guard;

/// Add a coercion between two vertex kinds to a schema.
///
/// `schema_handle` is a [`Resource::Schema`](crate::handle::Resource)
/// handle; `from_kind` and `to_kind` are the UTF-8 source/target vertex
/// kind names; `expr` is a CBOR-encoded `panproto_expr::Expr` coercion
/// expression. On success, `out_handle` receives a fresh
/// [`Resource::Schema`](crate::handle::Resource) handle with the
/// coercion installed.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_add_coercion(
    schema_handle: u32,
    from_kind: c_slice::Ref<'_, u8>,
    to_kind: c_slice::Ref<'_, u8>,
    expr: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    let _ = (schema_handle, from_kind, to_kind, expr, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_schema_add_coercion".into(),
        ))
    })
}

/// Add a default value to a schema vertex.
///
/// `schema_handle` is a schema handle; `vertex_name` is the UTF-8 vertex
/// name; `expr` is a CBOR-encoded `panproto_core::inst::value::Value`.
/// On success, `out_handle` receives a fresh
/// [`Resource::Schema`](crate::handle::Resource) handle.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_add_default(
    schema_handle: u32,
    vertex_name: c_slice::Ref<'_, u8>,
    expr: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    let _ = (schema_handle, vertex_name, expr, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_schema_add_default".into(),
        ))
    })
}

/// Add a merger annotation to a schema vertex.
///
/// `schema_handle` is a schema handle; `vertex_name` is the UTF-8 vertex
/// name; `spec` is a CBOR-encoded `{ strategy, args }` record. On
/// success, `out_handle` receives a fresh
/// [`Resource::Schema`](crate::handle::Resource) handle.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_add_merger(
    schema_handle: u32,
    vertex_name: c_slice::Ref<'_, u8>,
    spec: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    let _ = (schema_handle, vertex_name, spec, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_schema_add_merger".into(),
        ))
    })
}

/// Add a conflict policy annotation to a schema vertex.
///
/// `schema_handle` is a schema handle; `vertex_name` is the UTF-8 vertex
/// name; `spec` is a CBOR-encoded `{ policy }` record. On success,
/// `out_handle` receives a fresh
/// [`Resource::Schema`](crate::handle::Resource) handle.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_add_policy(
    schema_handle: u32,
    vertex_name: c_slice::Ref<'_, u8>,
    spec: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    let _ = (schema_handle, vertex_name, spec, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_schema_add_policy".into(),
        ))
    })
}

/// Decide a refinement subsort relationship between two constraint sets.
///
/// `base_sort` is the UTF-8 shared base sort name; `sub_constraints`
/// and `super_constraints` are CBOR-encoded `Vec<(String, String)>`
/// of `(sort, value)` pairs. On success, `out_is_subsort` receives `1`
/// when the sub-refinement refines at least as much as the
/// super-refinement, else `0`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_enriched_refinement_subsort(
    base_sort: c_slice::Ref<'_, u8>,
    sub_constraints: c_slice::Ref<'_, u8>,
    super_constraints: c_slice::Ref<'_, u8>,
    out_is_subsort: &mut u32,
) -> i32 {
    let _ = (
        base_sort,
        sub_constraints,
        super_constraints,
        out_is_subsort,
    );
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_enriched_refinement_subsort".into(),
        ))
    })
}
