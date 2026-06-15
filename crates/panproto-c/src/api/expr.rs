//! Expression language: parsing, functional evaluation, GAT-term
//! evaluation, type checking, and declarative queries.
//!
//! Frozen-signature scaffold. Every entry point currently returns
//! [`PpStatus::Operation`]; the engine-wiring pass fills in the bodies
//! against `panproto-expr`, `panproto-expr-parser`, and
//! `panproto_core::{gat, inst}`.

use safer_ffi::prelude::*;

use crate::error::FfiError;
use crate::panic::guard;

/// Parse expression source text into a `panproto-expr` AST.
///
/// `source` is the UTF-8 source bytes. On success, `out` receives the
/// CBOR-encoded `panproto_expr::Expr`. Will call
/// `panproto_expr_parser::{tokenize, parse}`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_expr_parse(source: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (source, out);
    guard(|| Err(FfiError::Operation("unimplemented: pp_expr_parse".into())))
}

/// Evaluate a functional expression against an environment.
///
/// `expr` is a CBOR-encoded `panproto_expr::Expr`; `env` is a
/// CBOR-encoded `Vec<(String, panproto_expr::Literal)>`. On success,
/// `out` receives the CBOR-encoded `panproto_expr::Literal` result.
/// Will call `panproto_expr::eval`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_expr_eval_func(
    expr: c_slice::Ref<'_, u8>,
    env: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (expr, env, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_expr_eval_func".into(),
        ))
    })
}

/// Evaluate a GAT term against a theory and a variable environment.
///
/// `expr` is a CBOR-encoded `panproto_core::gat::Term`; `env` is a
/// CBOR-encoded `Vec<(String, gat::ModelValue)>`; `theory` is a
/// [`Resource::Theory`](crate::handle::Resource) handle. On success,
/// `out` receives the CBOR-encoded `gat::ModelValue` result.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_expr_eval_gat(
    expr: c_slice::Ref<'_, u8>,
    env: c_slice::Ref<'_, u8>,
    theory: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (expr, env, theory, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_expr_eval_gat".into(),
        ))
    })
}

/// Type-check a GAT term against a theory and a typing context.
///
/// `expr` is a CBOR-encoded `gat::Term`; `theory` is a
/// [`Resource::Theory`](crate::handle::Resource) handle; `context` is a
/// CBOR-encoded `Vec<(String, String)>` mapping variable names to sort
/// names. On success, `out` receives a CBOR-encoded record
/// (`well_formed`, `output_sort`, `error`). Will call
/// `gat::typecheck_term`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_expr_check(
    expr: c_slice::Ref<'_, u8>,
    theory: u32,
    context: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (expr, theory, context, out);
    guard(|| Err(FfiError::Operation("unimplemented: pp_expr_check".into())))
}

/// Execute a declarative query against a W-type instance.
///
/// `query` is a CBOR-encoded `panproto_core::inst::InstanceQuery`;
/// `instance` is a CBOR-encoded `WInstance`; `schema_handle` is a
/// [`Resource::Schema`](crate::handle::Resource) handle (a minimal
/// placeholder schema is used when invalid). On success, `out` receives
/// a CBOR-encoded list of match records. Will call `inst::execute_query`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_query_execute(
    query: c_slice::Ref<'_, u8>,
    instance: c_slice::Ref<'_, u8>,
    schema_handle: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (query, instance, schema_handle, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_query_execute".into(),
        ))
    })
}
