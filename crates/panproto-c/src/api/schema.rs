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

use panproto_core::protocols;
use panproto_core::schema::{self, Schema, validate};
use safer_ffi::prelude::*;
use serde::Serialize;

use crate::api::helpers::{self, BuildOp};
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
/// `out_handle` receives a fresh [`Resource::Schema`] handle. The ops
/// are run through [`crate::api::helpers::build_schema_from_ops`].
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_build(proto: u32, ops: c_slice::Ref<'_, u8>, out_handle: &mut u32) -> i32 {
    guard(|| {
        let protocol = handle::with_resource(proto, |r| Ok(r.as_protocol()?.clone()))?;
        let operations: Vec<BuildOp> = crate::canonical::decode(ops.as_slice())?;
        let built = helpers::build_schema_from_ops(&protocol, operations)?;
        *out_handle = handle::alloc(Resource::Schema(Arc::new(built)));
        Ok(PpStatus::Ok)
    })
}

/// Schema metadata payload (`{ protocol, vertices, edges }`).
///
/// Mirrors the WASM `schema_metadata` shape so the same Haskell decoder
/// works against either backend.
#[derive(Serialize)]
struct SchemaMeta {
    protocol: String,
    vertices: Vec<VertexMeta>,
    edges: Vec<EdgeMeta>,
}

/// A single vertex entry in [`SchemaMeta`].
#[derive(Serialize)]
struct VertexMeta {
    id: String,
    kind: String,
    nsid: Option<String>,
}

/// A single edge entry in [`SchemaMeta`].
#[derive(Serialize)]
struct EdgeMeta {
    src: String,
    tgt: String,
    kind: String,
    name: Option<String>,
}

/// Extract schema metadata (protocol name, vertices, edges) as CBOR.
///
/// `schema_handle` is a [`Resource::Schema`] handle. On success, `out`
/// receives a CBOR-encoded metadata record mirroring the WASM
/// `schema_metadata` payload (`{ protocol, vertices, edges }`).
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_metadata(schema_handle: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let bytes = handle::with_resource(schema_handle, |r| {
            let schema = r.as_schema()?;

            let vertices: Vec<VertexMeta> = schema
                .vertices
                .values()
                .map(|v| VertexMeta {
                    id: v.id.to_string(),
                    kind: v.kind.to_string(),
                    nsid: v.nsid.as_deref().map(str::to_owned),
                })
                .collect();

            let edges: Vec<EdgeMeta> = schema
                .edges
                .keys()
                .map(|e| EdgeMeta {
                    src: e.src.to_string(),
                    tgt: e.tgt.to_string(),
                    kind: e.kind.to_string(),
                    name: e.name.as_deref().map(str::to_owned),
                })
                .collect();

            let meta = SchemaMeta {
                protocol: schema.protocol.clone(),
                vertices,
                edges,
            };

            crate::canonical::encode(&meta)
        })?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Normalize a schema by collapsing reference chains.
///
/// `schema_handle` is a [`Resource::Schema`] handle. On success,
/// `out_handle` receives a fresh [`Resource::Schema`] handle for the
/// normalized schema. Calls `panproto_core::schema::normalize`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_normalize(schema_handle: u32, out_handle: &mut u32) -> i32 {
    guard(|| {
        let original = handle::with_resource(schema_handle, |r| Ok(r.as_schema()?.clone()))?;
        let normalized = schema::normalize(&original);
        *out_handle = handle::alloc(Resource::Schema(Arc::new(normalized)));
        Ok(PpStatus::Ok)
    })
}

/// Parse an `ATProto` lexicon JSON document into a schema.
///
/// `json` is raw JSON bytes (decoded with `serde_json`, not CBOR). On
/// success, `out_handle` receives a fresh [`Resource::Schema`] handle.
/// Calls `panproto_core::protocols::atproto::parse_lexicon`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_parse_atproto_lexicon(json: c_slice::Ref<'_, u8>, out_handle: &mut u32) -> i32 {
    guard(|| {
        let value: serde_json::Value = serde_json::from_slice(json.as_slice())
            .map_err(|e| FfiError::Serialization(e.to_string()))?;
        let schema = protocols::atproto::parse_lexicon(&value)
            .map_err(|e| FfiError::Operation(format!("parse_atproto_lexicon: {e}")))?;
        *out_handle = handle::alloc(Resource::Schema(Arc::new(schema)));
        Ok(PpStatus::Ok)
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

    /// Deserialize shadow for [`SchemaMeta`] (the struct itself is
    /// serialize-only).
    #[derive(serde::Deserialize)]
    struct SchemaMetaOut {
        protocol: String,
        vertices: Vec<VertexMetaOut>,
        edges: Vec<EdgeMetaOut>,
    }
    #[derive(serde::Deserialize)]
    struct VertexMetaOut {
        id: String,
        kind: String,
        #[allow(dead_code)]
        nsid: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct EdgeMetaOut {
        src: String,
        tgt: String,
        #[allow(dead_code)]
        kind: String,
        #[allow(dead_code)]
        name: Option<String>,
    }

    fn build_ops_handle(proto_h: u32, ops: &[BuildOp]) -> u32 {
        let bytes = encode(&ops).unwrap();
        let slice: c_slice::Box<u8> = bytes.into_boxed_slice().into();
        let mut handle: u32 = u32::MAX;
        let status = pp_schema_build(proto_h, slice.as_ref(), &mut handle);
        assert_eq!(status, PpStatus::Ok as i32);
        handle
    }

    #[test]
    fn build_then_metadata_round_trips_vertices_and_edges() {
        // A protocol that recognizes "object" vertices and a "prop" edge
        // between them.
        use panproto_core::schema::EdgeRule;
        let proto = Protocol {
            name: "build-test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![EdgeRule {
                edge_kind: "prop".into(),
                src_kinds: vec!["object".into()],
                tgt_kinds: vec!["object".into()],
            }],
            obj_kinds: vec!["object".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        };
        let proto_h = define_protocol_handle(&proto);

        let ops = vec![
            BuildOp::Vertex {
                id: "user".into(),
                kind: "object".into(),
                nsid: None,
            },
            BuildOp::Vertex {
                id: "post".into(),
                kind: "object".into(),
                nsid: None,
            },
            BuildOp::Edge {
                src: "user".into(),
                tgt: "post".into(),
                kind: "prop".into(),
                name: Some("authored".into()),
            },
        ];
        let schema_h = build_ops_handle(proto_h, &ops);

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_schema_metadata(schema_h, &mut out), PpStatus::Ok as i32);
        let meta: SchemaMetaOut = decode(&out).unwrap();
        pp_buf_free(out);

        assert_eq!(meta.protocol, "build-test");
        let mut kinds: Vec<(String, String)> = meta
            .vertices
            .iter()
            .map(|v| (v.id.clone(), v.kind.clone()))
            .collect();
        kinds.sort();
        assert_eq!(
            kinds,
            vec![
                ("post".to_string(), "object".to_string()),
                ("user".to_string(), "object".to_string()),
            ]
        );
        assert_eq!(meta.edges.len(), 1);
        assert_eq!(meta.edges[0].src, "user");
        assert_eq!(meta.edges[0].tgt, "post");

        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(proto_h), PpStatus::Ok as i32);
    }

    #[test]
    fn build_with_invalid_proto_handle_errors() {
        let ops: Vec<BuildOp> = vec![];
        let bytes = encode(&ops).unwrap();
        let slice: c_slice::Box<u8> = bytes.into_boxed_slice().into();
        let mut handle: u32 = u32::MAX;
        let status = pp_schema_build(u32::MAX - 1, slice.as_ref(), &mut handle);
        assert_eq!(status, PpStatus::InvalidHandle as i32);
    }

    #[test]
    fn metadata_on_protocol_handle_yields_type_mismatch() {
        let proto_h = define_protocol_handle(&protocol_fixture());
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_schema_metadata(proto_h, &mut out);
        assert_eq!(status, PpStatus::TypeMismatch as i32);
        pp_buf_free(out);
        assert_eq!(pp_handle_free(proto_h), PpStatus::Ok as i32);
    }

    #[test]
    fn normalize_yields_a_fresh_schema_handle() {
        let h = allocate_schema_handle(&schema_fixture());

        let mut norm_h: u32 = u32::MAX;
        assert_eq!(pp_schema_normalize(h, &mut norm_h), PpStatus::Ok as i32);
        assert_ne!(norm_h, u32::MAX);

        // The normalized handle is a real Schema (round-trips to CBOR).
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_schema_to_cbor(norm_h, &mut out), PpStatus::Ok as i32);
        let restored: Schema = decode(&out).unwrap();
        assert_eq!(restored.protocol, "schema-test");
        pp_buf_free(out);

        assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(norm_h), PpStatus::Ok as i32);
    }

    #[test]
    fn parse_atproto_lexicon_builds_schema() {
        let lexicon = serde_json::json!({
            "lexicon": 1,
            "id": "app.test.post",
            "defs": {
                "main": {
                    "type": "record",
                    "record": {
                        "type": "object",
                        "required": ["text"],
                        "properties": {
                            "text": { "type": "string" }
                        }
                    }
                }
            }
        });
        let json_bytes = serde_json::to_vec(&lexicon).unwrap();
        let slice: c_slice::Box<u8> = json_bytes.into_boxed_slice().into();

        let mut handle: u32 = u32::MAX;
        let status = pp_schema_parse_atproto_lexicon(slice.as_ref(), &mut handle);
        assert_eq!(status, PpStatus::Ok as i32);
        assert_ne!(handle, u32::MAX);

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_schema_metadata(handle, &mut out), PpStatus::Ok as i32);
        let meta: SchemaMetaOut = decode(&out).unwrap();
        pp_buf_free(out);
        assert_eq!(meta.protocol, "atproto");
        // The main record vertex (named for the lexicon id) must exist.
        assert!(
            meta.vertices.iter().any(|v| v.id == "app.test.post"),
            "vertices: {:?}",
            meta.vertices.iter().map(|v| &v.id).collect::<Vec<_>>()
        );

        assert_eq!(pp_handle_free(handle), PpStatus::Ok as i32);
    }

    #[test]
    fn parse_atproto_lexicon_rejects_non_json() {
        let bad: Box<[u8]> = vec![0xFFu8, 0x00, 0x01].into_boxed_slice();
        let slice: c_slice::Box<u8> = bad.into();
        let mut handle: u32 = u32::MAX;
        let status = pp_schema_parse_atproto_lexicon(slice.as_ref(), &mut handle);
        assert_eq!(status, PpStatus::Serialization as i32);
        assert_eq!(handle, u32::MAX);
    }
}
