//! Schema ingest, serialization, and validation.
//!
//! The schema module exposes three entry points beyond the
//! lifecycle helpers in [`crate::api`]:
//!
//! - [`pp_schema_from_cbor`]: deserialize a CBOR-encoded `Schema`
//!   into a fresh handle. Used to round-trip a schema previously
//!   emitted by [`pp_schema_to_cbor`], or to load a schema saved
//!   from another panproto consumer.
//! - [`pp_schema_to_cbor`]: emit the CBOR bytes of a schema handle.
//! - [`pp_schema_validate`]: run `panproto_schema::validate` against
//!   a `(schema, protocol)` pair and return a CBOR-encoded
//!   `Vec<String>` of human-readable error messages. An empty list
//!   means the schema is valid.

use std::sync::Arc;

use panproto_core::schema::{Schema, validate};
use safer_ffi::prelude::*;

use crate::error::{FfiError, PpStatus};
use crate::handle::{self, Resource};
use crate::panic::guard;

/// Deserialize a CBOR-encoded `Schema` into the slab.
///
/// On success, `out_handle` is set to a fresh handle and
/// [`PpStatus::Ok`] is returned. On CBOR decode failure,
/// [`PpStatus::Serialization`] is returned and `out_handle` is left
/// untouched.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_from_cbor(spec: c_slice::Ref<'_, u8>, out_handle: &mut u32) -> i32 {
    guard(|| {
        let schema: Schema = crate::canonical::decode(spec.as_slice())?;
        *out_handle = handle::alloc(Resource::Schema(Arc::new(schema)));
        Ok(PpStatus::Ok)
    })
}

/// Serialize the schema referenced by `schema_handle` to CBOR.
///
/// On success, `out` is populated with freshly allocated CBOR bytes;
/// the host must release them via `pp_buf_free`. Common failure modes
/// are [`PpStatus::InvalidHandle`] and [`PpStatus::TypeMismatch`]
/// (when `schema_handle` does not point at a `Schema` resource).
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_to_cbor(schema_handle: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let bytes = handle::with_resource(schema_handle, |r| {
            let s = r.as_schema()?;
            crate::canonical::encode(s)
        })?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Validate the schema referenced by `schema_handle` against the
/// protocol referenced by `proto_handle`.
///
/// `out_messages` is populated with a CBOR-encoded `Vec<String>` of
/// human-readable validation messages. An empty list means the schema
/// is valid against the protocol. The status is always [`PpStatus::Ok`]
/// on a successful validation pass (regardless of whether the schema
/// passed validation); validation failures appear in the message list.
/// Status [`PpStatus::Err`] is reserved for cases where validation
/// could not run at all (e.g. a typed-mismatch handle).
///
/// Both handles must outlive the call. Common failure modes are
/// [`PpStatus::InvalidHandle`] and [`PpStatus::TypeMismatch`].
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_validate(
    schema_handle: u32,
    proto_handle: u32,
    out_messages: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let messages = handle::with_two_resources(schema_handle, proto_handle, |r1, r2| {
            let schema = r1.as_schema()?;
            let protocol = r2.as_protocol()?;
            let errors = validate(schema, protocol);
            // Use Display formatting (panproto-schema's ValidationError
            // has a hand-written Display impl with human-readable
            // messages); Debug would otherwise print constructor names.
            let messages: Vec<String> = errors.iter().map(ToString::to_string).collect();
            Ok(messages)
        })?;
        let bytes = crate::canonical::encode(&messages)?;
        *out_messages = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Build a schema from a protocol handle and a CBOR-encoded list of
/// builder operations.
///
/// `proto` is a [`Resource::Protocol`] handle. `ops` is a CBOR-encoded
/// `Vec<BuildOp>` (see [`crate::api::helpers::BuildOp`]). On success,
/// `out_handle` receives a fresh [`Resource::Schema`] handle. The
/// eventual implementation will run the ops through
/// [`crate::api::helpers::build_schema_from_ops`].
///
/// Stub: returns [`PpStatus::Operation`] until implemented in the
/// engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_build(proto: u32, ops: c_slice::Ref<'_, u8>, out_handle: &mut u32) -> i32 {
    let _ = (proto, ops, out_handle);
    guard(|| Err(FfiError::Operation("unimplemented: pp_schema_build".into())))
}

/// Extract schema metadata (protocol name, vertices, edges) as CBOR.
///
/// `schema_handle` is a [`Resource::Schema`] handle. On success, `out`
/// receives a CBOR-encoded metadata record mirroring the WASM
/// `schema_metadata` payload (`{ protocol, vertices, edges }`).
///
/// Stub: returns [`PpStatus::Operation`] until implemented in the
/// engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_metadata(schema_handle: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (schema_handle, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_schema_metadata".into(),
        ))
    })
}

/// Normalize a schema by collapsing reference chains.
///
/// `schema_handle` is a [`Resource::Schema`] handle. On success,
/// `out_handle` receives a fresh [`Resource::Schema`] handle for the
/// normalized schema. The eventual implementation will call
/// `panproto_core::schema::normalize`.
///
/// Stub: returns [`PpStatus::Operation`] until implemented in the
/// engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_normalize(schema_handle: u32, out_handle: &mut u32) -> i32 {
    let _ = (schema_handle, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_schema_normalize".into(),
        ))
    })
}

/// Parse an `ATProto` lexicon JSON document into a schema.
///
/// `json` is raw JSON bytes (decoded with `serde_json`, not CBOR). On
/// success, `out_handle` receives a fresh [`Resource::Schema`] handle.
/// The eventual implementation will call
/// `panproto_core::protocols::atproto::parse_lexicon`.
///
/// Stub: returns [`PpStatus::Operation`] until implemented in the
/// engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_parse_atproto_lexicon(json: c_slice::Ref<'_, u8>, out_handle: &mut u32) -> i32 {
    let _ = (json, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_schema_parse_atproto_lexicon".into(),
        ))
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;

    use panproto_core::schema::{Protocol, Schema};

    use super::*;
    use crate::api::{pp_buf_free, pp_handle_free, pp_last_error_take, pp_protocol_define};
    use crate::canonical::{decode, encode};
    use crate::error::ErrorEnvelope;

    fn protocol_fixture() -> Protocol {
        Protocol {
            name: "schema-test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn schema_fixture() -> Schema {
        Schema {
            protocol: "schema-test".into(),
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

    fn define_protocol_handle(p: &Protocol) -> u32 {
        let bytes = encode(p).unwrap();
        let slice: c_slice::Box<u8> = bytes.into_boxed_slice().into();
        let mut handle: u32 = u32::MAX;
        let status = pp_protocol_define(slice.as_ref(), &mut handle);
        assert_eq!(status, PpStatus::Ok as i32);
        handle
    }

    fn allocate_schema_handle(s: &Schema) -> u32 {
        let bytes = encode(s).unwrap();
        let slice: c_slice::Box<u8> = bytes.into_boxed_slice().into();
        let mut handle: u32 = u32::MAX;
        let status = pp_schema_from_cbor(slice.as_ref(), &mut handle);
        assert_eq!(status, PpStatus::Ok as i32);
        handle
    }

    #[test]
    fn schema_round_trip_through_ffi() {
        let h = allocate_schema_handle(&schema_fixture());

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_schema_to_cbor(h, &mut out), PpStatus::Ok as i32);

        let restored: Schema = decode(&out).unwrap();
        assert_eq!(restored.protocol, "schema-test");

        pp_buf_free(out);
        assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
    }

    #[test]
    fn validate_empty_schema_returns_no_messages() {
        let proto_h = define_protocol_handle(&protocol_fixture());
        let schema_h = allocate_schema_handle(&schema_fixture());

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_schema_validate(schema_h, proto_h, &mut out);
        assert_eq!(status, PpStatus::Ok as i32);

        let messages: Vec<String> = decode(&out).unwrap();
        assert!(messages.is_empty(), "got messages: {messages:?}");

        pp_buf_free(out);
        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(proto_h), PpStatus::Ok as i32);
    }

    #[test]
    fn schema_to_cbor_on_protocol_handle_yields_type_mismatch() {
        // Drain prior state.
        let mut sink: repr_c::Vec<u8> = Vec::new().into();
        let _ = pp_last_error_take(&mut sink);
        pp_buf_free(sink);

        let proto_h = define_protocol_handle(&protocol_fixture());

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_schema_to_cbor(proto_h, &mut out);
        assert_eq!(status, PpStatus::TypeMismatch as i32);
        pp_buf_free(out);

        let mut env_buf: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_last_error_take(&mut env_buf), PpStatus::Ok as i32);
        let env: ErrorEnvelope = decode(&env_buf).unwrap();
        assert_eq!(env.tag, "type_mismatch");
        assert!(env.message.contains("Schema"));
        assert!(env.message.contains("Protocol"));
        pp_buf_free(env_buf);

        assert_eq!(pp_handle_free(proto_h), PpStatus::Ok as i32);
    }

    #[test]
    fn schema_from_cbor_rejects_garbage() {
        let bad: Box<[u8]> = vec![0xFFu8, 0xFE, 0xFD].into_boxed_slice();
        let slice: c_slice::Box<u8> = bad.into();
        let mut handle: u32 = u32::MAX;
        let status = pp_schema_from_cbor(slice.as_ref(), &mut handle);
        assert_eq!(status, PpStatus::Serialization as i32);
        assert_eq!(handle, u32::MAX);
    }

    #[test]
    fn validate_reports_unknown_vertex_kind() {
        use panproto_core::schema::Vertex;

        // Protocol that recognizes only `record` as an obj kind.
        let strict_proto = Protocol {
            name: "strict".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["record".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        };

        // Schema with a vertex of unrecognized kind.
        let mut schema = schema_fixture();
        schema.protocol = "strict".into();
        schema.vertices.insert(
            "post".into(),
            Vertex {
                id: "post".into(),
                kind: "ZZZ".into(),
                nsid: None,
            },
        );

        let proto_h = define_protocol_handle(&strict_proto);
        let schema_h = allocate_schema_handle(&schema);

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_schema_validate(schema_h, proto_h, &mut out);
        assert_eq!(status, PpStatus::Ok as i32);

        let messages: Vec<String> = decode(&out).unwrap();
        assert!(
            !messages.is_empty(),
            "expected at least one validation message"
        );
        // Display is human-readable; should mention the offending
        // vertex id and kind.
        let joined = messages.join("\n");
        assert!(joined.contains("post"), "messages: {joined}");
        assert!(joined.contains("ZZZ"), "messages: {joined}");
        // Display format contains spaces (Debug would print
        // CamelCase identifiers); spot-check that "vertex" or
        // "kind" appears (Display impls usually use these words).
        let lower = joined.to_lowercase();
        assert!(
            lower.contains("vertex") || lower.contains("kind"),
            "messages should be human-readable: {joined}"
        );

        pp_buf_free(out);
        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(proto_h), PpStatus::Ok as i32);
    }

    #[test]
    fn validate_with_invalid_handle_yields_invalid_handle() {
        let proto_h = define_protocol_handle(&protocol_fixture());
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_schema_validate(u32::MAX - 1, proto_h, &mut out);
        assert_eq!(status, PpStatus::InvalidHandle as i32);
        pp_buf_free(out);
        assert_eq!(pp_handle_free(proto_h), PpStatus::Ok as i32);
    }
}
