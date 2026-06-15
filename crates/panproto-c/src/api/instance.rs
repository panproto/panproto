//! W-type instance validation and JSON conversion.
//!
//! Ported from `panproto_wasm::api::instance` (see
//! `crates/panproto-wasm/src/api/instance.rs`), narrowed to the four
//! instance entry points the C ABI exposes (the registry-backed
//! parse/emit codec surface lives in [`crate::api::registry`]). The WASM
//! `WasmError`/`JsError` pair becomes [`FfiError`], `rmp_serde` becomes
//! [`crate::canonical`], and the WASM slab becomes
//! [`crate::handle`]. Instances cross the boundary as CBOR-encoded
//! `WInstance` values (not slab handles); only the anchoring schema is a
//! handle.

use panproto_core::{inst, schema};
use safer_ffi::prelude::*;

use crate::error::{FfiError, PpStatus};
use crate::handle;
use crate::panic::guard;

/// Validate a W-type instance against a schema.
///
/// `schema_handle` is a [`Resource::Schema`](crate::handle::Resource)
/// handle. `instance` is a CBOR-encoded
/// `panproto_core::inst::WInstance`. On success, `out` receives a
/// CBOR-encoded `Vec<String>` of validation messages (empty means
/// valid). Calls `inst::validate_wtype`.
///
/// The validation pass always returns [`PpStatus::Ok`] when it can run
/// to completion: individual violations are reported as message
/// strings, not as a failing status. A non-`Ok` status is reserved for
/// inputs that prevent validation from running at all (an invalid
/// handle, a non-`Schema` resource, or undecodable instance bytes).
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_inst_validate(
    schema_handle: u32,
    instance: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let inst_value: inst::WInstance = crate::canonical::decode(instance.as_slice())?;
        let messages = handle::with_resource(schema_handle, |r| {
            let schema = r.as_schema()?;
            let errors: Vec<String> = inst::validate_wtype(schema, &inst_value)
                .into_iter()
                .map(|e| format!("{e:?}"))
                .collect();
            Ok(errors)
        })?;
        let bytes = crate::canonical::encode(&messages)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Convert a W-type instance to JSON bytes.
///
/// `schema_handle` is a [`Resource::Schema`](crate::handle::Resource)
/// handle. `instance` is a CBOR-encoded `WInstance`. On success, `out`
/// receives the JSON bytes (not CBOR). Calls `inst::to_json`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_inst_to_json(
    schema_handle: u32,
    instance: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let inst_value: inst::WInstance = crate::canonical::decode(instance.as_slice())?;
        let json_value = handle::with_resource(schema_handle, |r| {
            let schema = r.as_schema()?;
            Ok(inst::to_json(schema, &inst_value))
        })?;
        let bytes =
            serde_json::to_vec(&json_value).map_err(|e| FfiError::Serialization(e.to_string()))?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Parse JSON bytes into a W-type instance.
///
/// `schema_handle` is a [`Resource::Schema`](crate::handle::Resource)
/// handle. `json` is raw JSON bytes (decoded with `serde_json`, not
/// CBOR). `root_vertex` is a UTF-8 string selecting the root vertex
/// (empty infers it). On success, `out` receives a CBOR-encoded
/// `WInstance`. Calls `inst::parse_json`.
///
/// Root-vertex precedence mirrors the WASM boundary's
/// `json_to_instance_with_root`:
///   1. the explicit caller-supplied vertex if it exists in the schema;
///   2. `schema.protocol` (some builders use the protocol name as the
///      top-level vertex id);
///   3. the schema's declared primary entry (the pointed-schema
///      basepoint).
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_inst_json_to_instance(
    schema_handle: u32,
    json: c_slice::Ref<'_, u8>,
    root_vertex: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let root_vertex = std::str::from_utf8(root_vertex.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid root_vertex UTF-8: {e}")))?;

        let json_value: serde_json::Value = serde_json::from_slice(json.as_slice())
            .map_err(|e| FfiError::Serialization(e.to_string()))?;

        let inst_value = handle::with_resource(schema_handle, |r| {
            let schema = r.as_schema()?;

            let root: String = if !root_vertex.is_empty() && schema.has_vertex(root_vertex) {
                root_vertex.to_string()
            } else if schema.has_vertex(&schema.protocol) {
                schema.protocol.clone()
            } else {
                schema::primary_entry(schema)
                    .map(ToString::to_string)
                    .ok_or_else(|| {
                        FfiError::Operation("no suitable root vertex found in schema".to_string())
                    })?
            };

            inst::parse_json(schema, &root, &json_value)
                .map_err(|e| FfiError::Operation(format!("parse_json: {e}")))
        })?;

        let bytes = crate::canonical::encode(&inst_value)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Count the nodes in a W-type instance.
///
/// `instance` is a CBOR-encoded `WInstance`. On success, `out_count`
/// receives the node count. Calls `WInstance::node_count`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_inst_element_count(instance: c_slice::Ref<'_, u8>, out_count: &mut u32) -> i32 {
    guard(|| {
        let inst_value: inst::WInstance = crate::canonical::decode(instance.as_slice())?;
        #[allow(clippy::cast_possible_truncation)]
        {
            *out_count = inst_value.node_count() as u32;
        }
        Ok(PpStatus::Ok)
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use panproto_core::inst::WInstance;
    use panproto_core::schema::{Schema, SchemaBuilder};

    use super::*;
    use crate::api::{pp_buf_free, pp_handle_free};
    use crate::canonical::{decode, encode};
    use crate::handle::Resource;

    /// A schema with a `post` record carrying a string `text` property.
    fn post_schema() -> Schema {
        let proto = crate::api::helpers::default_protocol("test");
        SchemaBuilder::new(&proto)
            .vertex("post", "record", None)
            .unwrap()
            .vertex("text", "string", None)
            .unwrap()
            .edge("post", "text", "prop", Some("text"))
            .unwrap()
            .build()
            .unwrap()
    }

    fn allocate_schema_handle(s: &Schema) -> u32 {
        handle::alloc(Resource::Schema(Arc::new(s.clone())))
    }

    fn instance_slice(bytes: &[u8]) -> c_slice::Box<u8> {
        bytes.to_vec().into_boxed_slice().into()
    }

    /// Parse a small JSON document into a CBOR instance and return the
    /// CBOR bytes for downstream assertions.
    fn json_to_cbor_instance(schema_h: u32, json: &[u8], root: &str) -> Vec<u8> {
        let json_slice = instance_slice(json);
        let root_slice = instance_slice(root.as_bytes());
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status =
            pp_inst_json_to_instance(schema_h, json_slice.as_ref(), root_slice.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Ok as i32, "json_to_instance failed");
        let bytes = out.to_vec();
        pp_buf_free(out);
        bytes
    }

    #[test]
    fn json_round_trip_through_ffi() {
        let schema_h = allocate_schema_handle(&post_schema());
        let json = br#"{"text": "hello"}"#;

        let cbor = json_to_cbor_instance(schema_h, json, "post");

        // The decoded instance should have a root anchored to `post`
        // plus a child for the `text` property.
        let instance: WInstance = decode(&cbor).unwrap();
        assert!(instance.node_count() >= 2, "expected at least 2 nodes");

        // to_json on the same instance should yield a JSON object.
        let inst_slice = instance_slice(&cbor);
        let mut json_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_inst_to_json(schema_h, inst_slice.as_ref(), &mut json_out),
            PpStatus::Ok as i32
        );
        let value: serde_json::Value = serde_json::from_slice(&json_out).unwrap();
        assert!(value.is_object(), "expected JSON object, got {value:?}");
        pp_buf_free(json_out);

        // element_count should match node_count.
        let inst_slice = instance_slice(&cbor);
        let mut count: u32 = u32::MAX;
        assert_eq!(
            pp_inst_element_count(inst_slice.as_ref(), &mut count),
            PpStatus::Ok as i32
        );
        assert_eq!(count as usize, instance.node_count());

        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
    }

    #[test]
    fn validate_well_formed_instance_returns_no_messages() {
        let schema_h = allocate_schema_handle(&post_schema());
        let cbor = json_to_cbor_instance(schema_h, br#"{"text": "hi"}"#, "post");

        let inst_slice = instance_slice(&cbor);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_inst_validate(schema_h, inst_slice.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Ok as i32);
        let messages: Vec<String> = decode(&out).unwrap();
        assert!(messages.is_empty(), "got messages: {messages:?}");
        pp_buf_free(out);

        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
    }

    #[test]
    fn validate_reports_unknown_anchor() {
        let schema_h = allocate_schema_handle(&post_schema());

        // Hand-build an instance whose root anchors to a vertex absent
        // from the schema. validate_wtype's I1 check should flag it.
        let mut nodes = HashMap::new();
        let bad_anchor = panproto_core::gat::Name::from("ghost");
        nodes.insert(
            0u32,
            panproto_core::inst::metadata::Node::new(0, bad_anchor.clone()),
        );
        let instance = WInstance::new(nodes, Vec::new(), Vec::new(), 0, bad_anchor);
        let cbor = encode(&instance).unwrap();

        let inst_slice = instance_slice(&cbor);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_inst_validate(schema_h, inst_slice.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Ok as i32);
        let messages: Vec<String> = decode(&out).unwrap();
        assert!(
            !messages.is_empty(),
            "expected a validation message for the unknown anchor"
        );
        pp_buf_free(out);

        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
    }

    #[test]
    fn validate_with_invalid_handle_yields_invalid_handle() {
        let schema_h = allocate_schema_handle(&post_schema());
        let cbor = json_to_cbor_instance(schema_h, br#"{"text": "x"}"#, "post");
        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);

        let inst_slice = instance_slice(&cbor);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_inst_validate(u32::MAX - 1, inst_slice.as_ref(), &mut out);
        assert_eq!(status, PpStatus::InvalidHandle as i32);
        pp_buf_free(out);
    }

    #[test]
    fn validate_rejects_garbage_instance() {
        let schema_h = allocate_schema_handle(&post_schema());
        let bad = instance_slice(&[0xFFu8, 0xFE, 0xFD]);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_inst_validate(schema_h, bad.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Serialization as i32);
        pp_buf_free(out);
        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
    }

    #[test]
    fn element_count_rejects_garbage_instance() {
        let bad = instance_slice(&[0x00u8, 0x01, 0x02, 0x03]);
        let mut count: u32 = 7;
        let status = pp_inst_element_count(bad.as_ref(), &mut count);
        assert_eq!(status, PpStatus::Serialization as i32);
    }

    /// A schema with no vertices and no declared entries. `SchemaBuilder`
    /// rejects this (`EmptySchema`), so it is built directly. Used to
    /// exercise the "no suitable root vertex" path.
    fn vertexless_schema() -> Schema {
        Schema {
            protocol: "empty".into(),
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

    #[test]
    fn json_to_instance_unknown_root_with_no_entry_fails() {
        // A schema with no vertices and no entries cannot resolve a root
        // vertex, so parse must fail with an Operation status.
        let schema_h = allocate_schema_handle(&vertexless_schema());

        let json = instance_slice(br#"{"a": 1}"#);
        let root = instance_slice(b"");
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_inst_json_to_instance(schema_h, json.as_ref(), root.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Operation as i32);
        pp_buf_free(out);

        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
    }

    #[test]
    fn to_json_on_non_schema_handle_yields_type_mismatch() {
        // Allocate a protocol handle and try to use it as a schema.
        let proto = crate::api::helpers::default_protocol("p");
        let proto_h = handle::alloc(Resource::Protocol(Box::new(proto)));

        // Build any valid CBOR WInstance to get past the decode step.
        let instance = WInstance::new(
            HashMap::new(),
            Vec::new(),
            Vec::new(),
            0,
            panproto_core::gat::Name::from("x"),
        );
        let cbor = encode(&instance).unwrap();
        let inst_slice = instance_slice(&cbor);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_inst_to_json(proto_h, inst_slice.as_ref(), &mut out);
        assert_eq!(status, PpStatus::TypeMismatch as i32);
        pp_buf_free(out);

        assert_eq!(pp_handle_free(proto_h), PpStatus::Ok as i32);
    }
}
