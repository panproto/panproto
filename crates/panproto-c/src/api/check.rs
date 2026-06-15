//! Schema diff and compatibility classification.
//!
//! Five entry points wire the C ABI to `panproto_core::check`:
//!
//! - [`pp_check_diff_simple`]: lightweight vertex/edge structural diff
//!   via [`crate::api::helpers::compute_diff`].
//! - [`pp_check_diff_full`]: the full `panproto-check` diff engine
//!   (20+ change categories) via `check::diff`.
//! - [`pp_check_classify`]: classify a full diff against a protocol via
//!   `check::classify`.
//! - [`pp_check_report_text`]: render a [`CompatReport`] as UTF-8 text.
//! - [`pp_check_report_json`]: render a [`CompatReport`] as JSON.
//!
//! Ported from `panproto_wasm::api::schema` (the `diff_schemas`,
//! `diff_schemas_full`, `classify_diff`, `report_text`, and
//! `report_json` functions): `rmp_serde` is replaced by
//! [`crate::canonical`] (CBOR/ciborium), `WasmError` by
//! [`FfiError`](crate::error::FfiError), and the WASM slab by
//! [`crate::handle`].
//!
//! [`CompatReport`]: panproto_core::check::CompatReport

use panproto_core::check;
use safer_ffi::prelude::*;

use crate::canonical;
use crate::error::PpStatus;
use crate::handle;
use crate::panic::guard;

/// Compute a lightweight structural diff between two schemas.
///
/// `s1` and `s2` are [`Resource::Schema`](crate::handle::Resource)
/// handles. On success, `out` receives a CBOR-encoded
/// [`SchemaDiff`](crate::api::helpers::SchemaDiff) (vertex/edge level)
/// computed by [`crate::api::helpers::compute_diff`].
///
/// Common failure modes are [`PpStatus::InvalidHandle`] and
/// [`PpStatus::TypeMismatch`] (when either handle does not point at a
/// `Schema`).
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_check_diff_simple(s1: u32, s2: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let bytes = handle::with_two_resources(s1, s2, |r1, r2| {
            let schema1 = r1.as_schema()?;
            let schema2 = r2.as_schema()?;
            let diff = crate::api::helpers::compute_diff(schema1, schema2);
            canonical::encode(&diff)
        })?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Compute a full diff between two schemas via the `panproto-check`
/// engine (20+ change categories).
///
/// `s1` and `s2` are [`Resource::Schema`](crate::handle::Resource)
/// handles. On success, `out` receives a CBOR-encoded
/// `panproto_core::check::SchemaDiff` produced by `check::diff`, with
/// constraints, hyper-edges, variants, recursion points, usage modes,
/// spans, and nominal-identity changes.
///
/// Common failure modes are [`PpStatus::InvalidHandle`] and
/// [`PpStatus::TypeMismatch`].
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_check_diff_full(s1: u32, s2: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let bytes = handle::with_two_resources(s1, s2, |r1, r2| {
            let schema1 = r1.as_schema()?;
            let schema2 = r2.as_schema()?;
            let diff = check::diff(schema1, schema2);
            canonical::encode(&diff)
        })?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Classify a full schema diff against a protocol, producing a
/// compatibility report.
///
/// `proto` is a [`Resource::Protocol`](crate::handle::Resource) handle.
/// `diff` is a CBOR-encoded `panproto_core::check::SchemaDiff` (as
/// emitted by [`pp_check_diff_full`]). On success, `out` receives a
/// CBOR-encoded `check::CompatReport` from `check::classify`.
///
/// Common failure modes are [`PpStatus::InvalidHandle`],
/// [`PpStatus::TypeMismatch`] (when `proto` does not point at a
/// `Protocol`), and [`PpStatus::Serialization`] (when `diff` is not a
/// valid CBOR `check::SchemaDiff`).
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_check_classify(proto: u32, diff: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let parsed: check::SchemaDiff = canonical::decode(diff.as_slice())?;
        let bytes = handle::with_resource(proto, |r| {
            let protocol = r.as_protocol()?;
            let report = check::classify(&parsed, protocol);
            canonical::encode(&report)
        })?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Render a compatibility report as human-readable text.
///
/// `report` is a CBOR-encoded `check::CompatReport`. On success, `out`
/// receives the UTF-8 text bytes produced by `check::report_text`.
///
/// Returns [`PpStatus::Serialization`] when `report` is not a valid
/// CBOR `check::CompatReport`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_check_report_text(report: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let parsed: check::CompatReport = canonical::decode(report.as_slice())?;
        let text = check::report_text(&parsed);
        *out = text.into_bytes().into();
        Ok(PpStatus::Ok)
    })
}

/// Render a compatibility report as a JSON document.
///
/// `report` is a CBOR-encoded `check::CompatReport`. On success, `out`
/// receives the UTF-8 JSON bytes serialized from the
/// [`serde_json::Value`] produced by `check::report_json`.
///
/// Returns [`PpStatus::Serialization`] when `report` is not a valid
/// CBOR `check::CompatReport` or when the rendered JSON value cannot be
/// serialized.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_check_report_json(report: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let parsed: check::CompatReport = canonical::decode(report.as_slice())?;
        let json = check::report_json(&parsed);
        let bytes = serde_json::to_vec(&json)
            .map_err(|e| crate::error::FfiError::Serialization(e.to_string()))?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use panproto_core::schema::{Protocol, Schema, Vertex};

    use super::*;
    use crate::api::pp_buf_free;
    use crate::canonical::decode;
    use crate::handle::{self, Resource};

    fn empty_schema() -> Schema {
        Schema {
            protocol: "check-test".into(),
            vertices: HashMap::new(),
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: vec![],
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
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        }
    }

    fn schema_with_vertex(id: &str, kind: &str) -> Schema {
        let mut s = empty_schema();
        s.vertices.insert(
            id.into(),
            Vertex {
                id: id.into(),
                kind: kind.into(),
                nsid: None,
            },
        );
        s
    }

    fn protocol_fixture() -> Protocol {
        Protocol {
            name: "check-test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["record".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn schema_handle(s: Schema) -> u32 {
        handle::alloc(Resource::Schema(Arc::new(s)))
    }

    fn protocol_handle(p: Protocol) -> u32 {
        handle::alloc(Resource::Protocol(Box::new(p)))
    }

    #[test]
    fn diff_simple_reports_added_vertex() {
        let h1 = schema_handle(empty_schema());
        let h2 = schema_handle(schema_with_vertex("post", "record"));

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_check_diff_simple(h1, h2, &mut out), PpStatus::Ok as i32);

        let diff: crate::api::helpers::SchemaDiff = decode(&out).unwrap();
        assert_eq!(diff.added_vertices, vec!["post".to_string()]);
        assert!(diff.removed_vertices.is_empty());

        pp_buf_free(out);
        handle::free(h1);
        handle::free(h2);
    }

    #[test]
    fn diff_full_reports_removed_vertex() {
        let h1 = schema_handle(schema_with_vertex("post", "record"));
        let h2 = schema_handle(empty_schema());

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_check_diff_full(h1, h2, &mut out), PpStatus::Ok as i32);

        let diff: check::SchemaDiff = decode(&out).unwrap();
        assert_eq!(diff.removed_vertices, vec!["post".to_string()]);

        pp_buf_free(out);
        handle::free(h1);
        handle::free(h2);
    }

    #[test]
    fn classify_then_report_round_trip() {
        // Removing a vertex is a breaking change.
        let h1 = schema_handle(schema_with_vertex("post", "record"));
        let h2 = schema_handle(empty_schema());
        let proto_h = protocol_handle(protocol_fixture());

        let mut diff_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_check_diff_full(h1, h2, &mut diff_out),
            PpStatus::Ok as i32
        );

        let diff_slice: c_slice::Box<u8> = diff_out.to_vec().into_boxed_slice().into();
        let mut report_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_check_classify(proto_h, diff_slice.as_ref(), &mut report_out),
            PpStatus::Ok as i32
        );

        let report: check::CompatReport = decode(&report_out).unwrap();
        assert!(!report.compatible, "removing a vertex must be breaking");
        assert!(!report.breaking.is_empty());

        // Text rendering.
        let report_slice: c_slice::Box<u8> = report_out.to_vec().into_boxed_slice().into();
        let mut text_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_check_report_text(report_slice.as_ref(), &mut text_out),
            PpStatus::Ok as i32
        );
        let text = String::from_utf8(text_out.to_vec()).unwrap();
        assert!(!text.is_empty(), "report text should be non-empty");

        // JSON rendering.
        let report_slice2: c_slice::Box<u8> = report_out.to_vec().into_boxed_slice().into();
        let mut json_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_check_report_json(report_slice2.as_ref(), &mut json_out),
            PpStatus::Ok as i32
        );
        let json: serde_json::Value = serde_json::from_slice(&json_out).unwrap();
        assert!(json.is_object() || json.is_array(), "json: {json}");

        pp_buf_free(diff_out);
        pp_buf_free(report_out);
        pp_buf_free(text_out);
        pp_buf_free(json_out);
        handle::free(h1);
        handle::free(h2);
        handle::free(proto_h);
    }

    #[test]
    fn diff_simple_on_protocol_handle_yields_type_mismatch() {
        let proto_h = protocol_handle(protocol_fixture());
        let schema_h = schema_handle(empty_schema());

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_check_diff_simple(proto_h, schema_h, &mut out);
        assert_eq!(status, PpStatus::TypeMismatch as i32);
        pp_buf_free(out);

        handle::free(proto_h);
        handle::free(schema_h);
    }

    #[test]
    fn diff_full_with_invalid_handle_yields_invalid_handle() {
        let schema_h = schema_handle(empty_schema());
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_check_diff_full(u32::MAX - 1, schema_h, &mut out);
        assert_eq!(status, PpStatus::InvalidHandle as i32);
        pp_buf_free(out);
        handle::free(schema_h);
    }

    #[test]
    fn classify_rejects_garbage_diff() {
        let proto_h = protocol_handle(protocol_fixture());
        let bad: Box<[u8]> = vec![0xFFu8, 0xFE, 0xFD].into_boxed_slice();
        let slice: c_slice::Box<u8> = bad.into();
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_check_classify(proto_h, slice.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Serialization as i32);
        pp_buf_free(out);
        handle::free(proto_h);
    }

    #[test]
    fn report_text_rejects_garbage() {
        let bad: Box<[u8]> = vec![0xFFu8, 0xFE, 0xFD].into_boxed_slice();
        let slice: c_slice::Box<u8> = bad.into();
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_check_report_text(slice.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Serialization as i32);
        pp_buf_free(out);
    }
}
