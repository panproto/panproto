//! Schema diff and compatibility classification.
//!
//! Frozen-signature scaffold. Every entry point currently returns
//! [`PpStatus::Operation`]; the engine-wiring pass fills in the bodies
//! against `panproto_core::check`.

use safer_ffi::prelude::*;

use crate::error::FfiError;
use crate::panic::guard;

/// Compute a lightweight structural diff between two schemas.
///
/// `s1` and `s2` are [`Resource::Schema`](crate::handle::Resource)
/// handles. On success, `out` receives a CBOR-encoded
/// [`SchemaDiff`](crate::api::helpers::SchemaDiff) (vertex/edge level).
/// Will call [`crate::api::helpers::compute_diff`].
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_check_diff_simple(s1: u32, s2: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (s1, s2, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_check_diff_simple".into(),
        ))
    })
}

/// Compute a full diff between two schemas via the `panproto-check`
/// engine (20+ change categories).
///
/// `s1` and `s2` are [`Resource::Schema`](crate::handle::Resource)
/// handles. On success, `out` receives a CBOR-encoded
/// `panproto_core::check::SchemaDiff`. Will call `check::diff`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_check_diff_full(s1: u32, s2: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (s1, s2, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_check_diff_full".into(),
        ))
    })
}

/// Classify a full schema diff against a protocol, producing a
/// compatibility report.
///
/// `proto` is a [`Resource::Protocol`](crate::handle::Resource) handle.
/// `diff` is a CBOR-encoded `panproto_core::check::SchemaDiff`. On
/// success, `out` receives a CBOR-encoded `check::CompatReport`. Will
/// call `check::classify`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_check_classify(proto: u32, diff: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (proto, diff, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_check_classify".into(),
        ))
    })
}

/// Render a compatibility report as human-readable text.
///
/// `report` is a CBOR-encoded `check::CompatReport`. On success, `out`
/// receives the rendered UTF-8 text bytes. Will call
/// `check::report_text`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_check_report_text(report: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (report, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_check_report_text".into(),
        ))
    })
}

/// Render a compatibility report as a JSON document.
///
/// `report` is a CBOR-encoded `check::CompatReport`. On success, `out`
/// receives the rendered JSON bytes. Will call `check::report_json`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_check_report_json(report: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (report, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_check_report_json".into(),
        ))
    })
}
