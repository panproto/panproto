//! Migration existence checking, compilation, lifting, and composition.
//!
//! Ported from the migration entry points of
//! `panproto_wasm::api::schema` (and the coverage op in
//! `panproto_wasm::api::enriched`), narrowed to the seven entry points
//! the C ABI exposes. The WASM `WasmError`/`JsError` pair becomes
//! [`FfiError`], `rmp_serde` becomes [`crate::canonical`] (CBOR via
//! ciborium), and the WASM slab becomes [`crate::handle`]. Migration
//! specs and W-type instances cross the boundary as CBOR values; the
//! compiled migration and its anchoring schemas live in the slab as
//! [`Resource::MigrationWithSchemas`](crate::handle::Resource) (or a
//! bare [`Resource::Migration`](crate::handle::Resource) for a composed
//! migration).

use panproto_core::{
    inst::{self, CompiledMigration, WInstance},
    mig::{self, Migration},
    schema,
};
use safer_ffi::prelude::*;
use serde::{Deserialize, Serialize};

use crate::api::helpers::{build_theory_registry, compose_compiled, extract_migration_ref};
use crate::error::{FfiError, PpStatus};
use crate::handle::{self, Resource};
use crate::panic::guard;

/// Coverage report for a dry-run migration over a batch of records.
///
/// Mirrors the JSON shape `panproto_wasm::api::enriched::migration_coverage`
/// produces (`total`, `succeeded`, `failed`, `coverage_percent`,
/// `errors`), plus the source and target vertex counts. The C ABI
/// returns it CBOR-encoded.
#[derive(Debug, Serialize, Deserialize)]
struct CoverageReport {
    /// Total number of instances examined.
    total: u64,
    /// Number of instances that lifted successfully.
    succeeded: u64,
    /// Number of instances that failed to lift.
    failed: u64,
    /// Percentage of instances that lifted successfully (0..=100).
    coverage_percent: f64,
    /// Up to the first 20 per-record failure messages.
    errors: Vec<String>,
    /// Vertex count of the source schema.
    src_vertices: u64,
    /// Vertex count of the target schema.
    tgt_vertices: u64,
}

/// Check the existence conditions for a migration mapping between two
/// schemas.
///
/// `proto` is a [`Resource::Protocol`](crate::handle::Resource) handle;
/// `src` and `tgt` are [`Resource::Schema`](crate::handle::Resource)
/// handles. `mapping` is a CBOR-encoded `panproto_core::mig::Migration`.
/// On success, `out` receives a CBOR-encoded `mig::ExistenceReport`
/// (the report itself encodes validity). Calls `mig::check_existence`
/// with a theory registry from
/// [`crate::api::helpers::build_theory_registry`].
///
/// Like `pp_inst_validate`, the check returns [`PpStatus::Ok`] whenever
/// it can run to completion: a migration that fails the existence
/// conditions is reported as `valid: false` inside the report, not as a
/// failing status. A non-`Ok` status is reserved for inputs that
/// prevent the check from running (an invalid handle, a non-`Protocol`
/// or non-`Schema` resource, undecodable mapping bytes, or an
/// unrecognized protocol whose theory registry cannot be built).
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_mig_check_existence(
    proto: u32,
    src: u32,
    tgt: u32,
    mapping: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let migration: Migration = crate::canonical::decode(mapping.as_slice())?;

        let protocol = handle::with_resource(proto, |r| Ok(r.as_protocol()?.clone()))?;
        let (src_schema, tgt_schema) = handle::with_two_resources(src, tgt, |r1, r2| {
            Ok((r1.as_schema_arc()?, r2.as_schema_arc()?))
        })?;

        let theory_registry = build_theory_registry(&protocol.name)?;
        let report = mig::check_existence(
            &protocol,
            &src_schema,
            &tgt_schema,
            &migration,
            &theory_registry,
        );

        let bytes = crate::canonical::encode(&report)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Compile a migration for fast per-record application.
///
/// `src` and `tgt` are [`Resource::Schema`](crate::handle::Resource)
/// handles. `mapping` is a CBOR-encoded `mig::Migration`. On success,
/// `out_handle` receives a fresh
/// [`Resource::MigrationWithSchemas`](crate::handle::Resource) handle.
/// Calls `mig::compile`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_mig_compile(
    src: u32,
    tgt: u32,
    mapping: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    guard(|| {
        let migration: Migration = crate::canonical::decode(mapping.as_slice())?;

        let (src_schema, tgt_schema) = handle::with_two_resources(src, tgt, |r1, r2| {
            Ok((r1.as_schema_arc()?, r2.as_schema_arc()?))
        })?;

        let compiled = mig::compile(&src_schema, &tgt_schema, &migration)
            .map_err(|e| FfiError::Operation(format!("compile: {e}")))?;

        *out_handle = handle::alloc(Resource::MigrationWithSchemas {
            compiled: Box::new(compiled),
            src_schema,
            tgt_schema,
        });
        Ok(PpStatus::Ok)
    })
}

/// Serialize a compiled migration to CBOR.
///
/// `mig_handle` is a [`Resource::Migration`](crate::handle::Resource)
/// or [`Resource::MigrationWithSchemas`](crate::handle::Resource)
/// handle. On success, `out` receives the CBOR-encoded
/// `panproto_core::inst::CompiledMigration` (the `compiled` payload, not
/// the anchoring schemas). This is the byte form the graph domain
/// consumes: `pp_graph_fiber_at` and `pp_graph_fiber_decomposition`
/// take their `migration` argument as exactly these bytes.
///
/// Fails with [`PpStatus::InvalidHandle`] or [`PpStatus::TypeMismatch`]
/// when the handle does not resolve to a migration.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_mig_serialize_compiled(mig_handle: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let bytes = handle::with_resource(mig_handle, |r| {
            let compiled: &CompiledMigration = r.as_migration()?;
            crate::canonical::encode(compiled)
        })?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Apply a compiled migration to a single W-type record.
///
/// `migration` is a [`Resource::Migration`](crate::handle::Resource)
/// (or `MigrationWithSchemas`) handle. `record` is a CBOR-encoded
/// `panproto_core::inst::WInstance`. On success, `out` receives the
/// CBOR-encoded migrated instance. Calls `mig::lift_wtype` via
/// [`crate::api::helpers::extract_migration_ref`].
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_mig_lift_record(
    migration: u32,
    record: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let instance: WInstance = crate::canonical::decode(record.as_slice())?;

        let result = handle::with_resource(migration, |r| {
            let (compiled, src_schema, tgt_schema) = extract_migration_ref(r)?;
            mig::lift_wtype(compiled, &src_schema, &tgt_schema, &instance)
                .map_err(|e| FfiError::Operation(format!("lift_wtype: {e}")))
        })?;

        let bytes = crate::canonical::encode(&result)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Compose two compiled migrations into a single migration.
///
/// `m1` and `m2` are migration handles (either
/// [`Resource::Migration`](crate::handle::Resource) or
/// `MigrationWithSchemas`). On success, `out_handle` receives a fresh
/// [`Resource::Migration`](crate::handle::Resource) handle. Calls
/// [`crate::api::helpers::compose_compiled`].
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_mig_compose(m1: u32, m2: u32, out_handle: &mut u32) -> i32 {
    guard(|| {
        let composed = handle::with_two_resources(m1, m2, |r1, r2| {
            let c1 = r1.as_migration()?;
            let c2 = r2.as_migration()?;
            Ok(compose_compiled(c1, c2))
        })?;

        *out_handle = handle::alloc(Resource::Migration(Box::new(composed)));
        Ok(PpStatus::Ok)
    })
}

/// Invert a bijective migration.
///
/// `mapping` is a CBOR-encoded `mig::Migration`; `src` and `tgt` are
/// [`Resource::Schema`](crate::handle::Resource) handles. On success,
/// `out` receives the CBOR-encoded inverse `mig::Migration`. Calls
/// `mig::invert` and fails with [`PpStatus::Operation`] when the
/// migration is not invertible.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_mig_invert(
    mapping: c_slice::Ref<'_, u8>,
    src: u32,
    tgt: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let migration: Migration = crate::canonical::decode(mapping.as_slice())?;

        let (src_schema, tgt_schema) = handle::with_two_resources(src, tgt, |r1, r2| {
            Ok((r1.as_schema_arc()?, r2.as_schema_arc()?))
        })?;

        let inverse = mig::invert(&migration, &src_schema, &tgt_schema)
            .map_err(|e| FfiError::Operation(format!("invert: {e}")))?;

        let bytes = crate::canonical::encode(&inverse)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Run coverage analysis (dry-run migration) over a batch of instances.
///
/// `migration` is a migration handle; `src` and `tgt` are
/// [`Resource::Schema`](crate::handle::Resource) handles. `instances`
/// is a CBOR-encoded `Vec<WInstance>`. On success, `out` receives a
/// CBOR-encoded coverage report (`total`, `succeeded`, `failed`,
/// `coverage_percent`, `errors`, plus source and target vertex counts).
/// Calls `mig::lift_wtype` per record.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_mig_coverage(
    migration: u32,
    src: u32,
    tgt: u32,
    instances: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let instances: Vec<WInstance> = crate::canonical::decode(instances.as_slice())?;

        let compiled = handle::with_resource(migration, |r| Ok(r.as_migration()?.clone()))?;
        let (src_schema, tgt_schema) = handle::with_two_resources(src, tgt, |r1, r2| {
            Ok((r1.as_schema_arc()?, r2.as_schema_arc()?))
        })?;

        #[allow(clippy::cast_possible_truncation)]
        let total = instances.len() as u64;
        let mut succeeded = 0u64;
        let mut failed = 0u64;
        let mut errors: Vec<String> = Vec::new();

        for (i, instance) in instances.iter().enumerate() {
            match mig::lift_wtype(&compiled, &src_schema, &tgt_schema, instance) {
                Ok(_) => succeeded += 1,
                Err(e) => {
                    failed += 1;
                    if errors.len() < 20 {
                        errors.push(format!("record {i}: {e}"));
                    }
                }
            }
        }

        #[allow(clippy::cast_precision_loss)]
        let coverage_percent = if total > 0 {
            (succeeded as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        #[allow(clippy::cast_possible_truncation)]
        let report = CoverageReport {
            total,
            succeeded,
            failed,
            coverage_percent,
            errors,
            src_vertices: src_schema.vertex_count() as u64,
            tgt_vertices: tgt_schema.vertex_count() as u64,
        };

        let bytes = crate::canonical::encode(&report)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Lift a JSON record through a compiled migration, returning JSON.
///
/// `migration` is a migration handle. `json` is raw JSON bytes (decoded
/// with `serde_json`, not CBOR). `root_vertex` is the source schema
/// vertex the JSON object maps to (empty auto-detects). On success,
/// `out` receives the migrated record as JSON bytes. Calls
/// `inst::parse_json`, `mig::lift_wtype`, then `inst::to_json`.
///
/// Root-vertex resolution mirrors `pp_inst_json_to_instance`: the
/// explicit vertex when present, then the source schema's protocol name
/// when it names a vertex, then the schema's declared primary entry.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_mig_lift_json(
    migration: u32,
    json: c_slice::Ref<'_, u8>,
    root_vertex: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let root_vertex = std::str::from_utf8(root_vertex.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid root_vertex UTF-8: {e}")))?;

        let json_value: serde_json::Value = serde_json::from_slice(json.as_slice())
            .map_err(|e| FfiError::Serialization(e.to_string()))?;

        let result = handle::with_resource(migration, |r| {
            let (compiled, src_schema, tgt_schema) = extract_migration_ref(r)?;

            let root = resolve_root(root_vertex, &src_schema)?;
            let instance = inst::parse_json(&src_schema, &root, &json_value)
                .map_err(|e| FfiError::Operation(format!("parse_json: {e}")))?;

            let lifted = mig::lift_wtype(compiled, &src_schema, &tgt_schema, &instance)
                .map_err(|e| FfiError::Operation(format!("lift_wtype: {e}")))?;

            Ok(inst::to_json(&tgt_schema, &lifted))
        })?;

        let bytes =
            serde_json::to_vec(&result).map_err(|e| FfiError::Serialization(e.to_string()))?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Resolve a root vertex for JSON parsing against `schema`.
///
/// Precedence matches `pp_inst_json_to_instance`:
///   1. the explicit caller-supplied vertex if it exists in the schema;
///   2. `schema.protocol` (some builders name the top-level vertex after
///      the protocol);
///   3. the schema's declared primary entry.
///
/// When the schema is the minimal one synthesized from a bare
/// [`Resource::Migration`](crate::handle::Resource) (no declared entry),
/// the explicit vertex is the only resolvable option.
fn resolve_root(root_vertex: &str, schema: &schema::Schema) -> Result<String, FfiError> {
    if !root_vertex.is_empty() && schema.has_vertex(root_vertex) {
        Ok(root_vertex.to_string())
    } else if schema.has_vertex(&schema.protocol) {
        Ok(schema.protocol.clone())
    } else {
        schema::primary_entry(schema)
            .map(ToString::to_string)
            .ok_or_else(|| {
                FfiError::Operation("no suitable root vertex found in schema".to_string())
            })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use panproto_core::gat::Name;
    use panproto_core::mig::ExistenceReport;
    use panproto_core::schema::{Schema, SchemaBuilder};

    use super::*;
    use crate::api::{pp_buf_free, pp_handle_free, pp_inst_json_to_instance};
    use crate::canonical::{decode, encode};
    use crate::handle::Resource;

    /// A two-vertex source schema: a `post` record with a `text` string
    /// property.
    fn source_schema() -> Schema {
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

    /// A target schema isomorphic to the source but with the record
    /// vertex renamed to `note` (the property keeps its `text` label).
    fn target_schema() -> Schema {
        let proto = crate::api::helpers::default_protocol("test");
        SchemaBuilder::new(&proto)
            .vertex("note", "record", None)
            .unwrap()
            .vertex("text", "string", None)
            .unwrap()
            .edge("note", "text", "prop", Some("text"))
            .unwrap()
            .build()
            .unwrap()
    }

    /// The migration mapping the source `post`/`text` onto the target
    /// `note`/`text`: a vertex rename plus the matching edge remap.
    fn rename_migration(src: &Schema, tgt: &Schema) -> Migration {
        let mut vertex_map = HashMap::new();
        vertex_map.insert(Name::from("post"), Name::from("note"));
        vertex_map.insert(Name::from("text"), Name::from("text"));

        let src_edge = src.edges.keys().next().unwrap().clone();
        let tgt_edge = tgt.edges.keys().next().unwrap().clone();
        let mut edge_map = HashMap::new();
        edge_map.insert(src_edge, tgt_edge);

        Migration {
            vertex_map,
            edge_map,
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
            coercions: HashMap::new(),
            domain: None,
            codomain: None,
        }
    }

    fn alloc_schema(s: &Schema) -> u32 {
        handle::alloc(Resource::Schema(Arc::new(s.clone())))
    }

    fn slice(bytes: &[u8]) -> c_slice::Box<u8> {
        bytes.to_vec().into_boxed_slice().into()
    }

    /// Parse a JSON document against `schema_h`/`root` into CBOR
    /// instance bytes via the instance entry point.
    fn json_to_cbor(schema_h: u32, json: &[u8], root: &str) -> Vec<u8> {
        let json_slice = slice(json);
        let root_slice = slice(root.as_bytes());
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status =
            pp_inst_json_to_instance(schema_h, json_slice.as_ref(), root_slice.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Ok as i32, "json_to_instance failed");
        let bytes = out.to_vec();
        pp_buf_free(out);
        bytes
    }

    /// Compile the rename migration over freshly-allocated schema
    /// handles, returning `(migration_handle, src_handle, tgt_handle)`.
    fn compile_rename() -> (u32, u32, u32) {
        let src = source_schema();
        let tgt = target_schema();
        let migration = rename_migration(&src, &tgt);

        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);

        let mapping = encode(&migration).unwrap();
        let mapping_slice = slice(&mapping);
        let mut mig_h: u32 = u32::MAX;
        let status = pp_mig_compile(src_h, tgt_h, mapping_slice.as_ref(), &mut mig_h);
        assert_eq!(status, PpStatus::Ok as i32, "compile failed");
        (mig_h, src_h, tgt_h)
    }

    #[test]
    fn check_existence_accepts_valid_rename() {
        let src = source_schema();
        let tgt = target_schema();
        let migration = rename_migration(&src, &tgt);

        let proto = crate::api::helpers::lookup_builtin_protocol("atproto").unwrap();
        let proto_h = handle::alloc(Resource::Protocol(Box::new(proto)));
        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);

        let mapping = encode(&migration).unwrap();
        let mapping_slice = slice(&mapping);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status =
            pp_mig_check_existence(proto_h, src_h, tgt_h, mapping_slice.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Ok as i32);
        let report: ExistenceReport = decode(&out).unwrap();
        assert!(report.valid, "report errors: {:?}", report.errors);
        pp_buf_free(out);

        assert_eq!(pp_handle_free(proto_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn check_existence_unknown_protocol_errors() {
        let src = source_schema();
        let tgt = target_schema();
        let migration = rename_migration(&src, &tgt);

        // A bare default protocol named "test" has no registered theory
        // registry, so the check cannot run and returns a non-Ok status.
        let proto = crate::api::helpers::default_protocol("test");
        let proto_h = handle::alloc(Resource::Protocol(Box::new(proto)));
        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);

        let mapping = encode(&migration).unwrap();
        let mapping_slice = slice(&mapping);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status =
            pp_mig_check_existence(proto_h, src_h, tgt_h, mapping_slice.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Operation as i32);
        pp_buf_free(out);

        assert_eq!(pp_handle_free(proto_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn compile_then_lift_record_round_trips() {
        let (mig_h, src_h, tgt_h) = compile_rename();

        // Parse a record against the source schema, then lift it.
        let cbor = json_to_cbor(src_h, br#"{"text": "hello"}"#, "post");
        let rec_slice = slice(&cbor);
        let mut lifted: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_mig_lift_record(mig_h, rec_slice.as_ref(), &mut lifted);
        assert_eq!(status, PpStatus::Ok as i32);

        // The lifted instance re-anchors to the target `note` vertex.
        let instance: WInstance = decode(&lifted).unwrap();
        assert!(instance.node_count() >= 1);
        pp_buf_free(lifted);

        assert_eq!(pp_handle_free(mig_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn serialize_compiled_decodes_as_compiled_migration() {
        let (mig_h, src_h, tgt_h) = compile_rename();

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_mig_serialize_compiled(mig_h, &mut out);
        assert_eq!(status, PpStatus::Ok as i32);

        // The bytes decode as the same `CompiledMigration` the graph
        // domain consumes; the rename remaps `post` onto `note`.
        let compiled: CompiledMigration = decode(&out).unwrap();
        assert_eq!(
            compiled.vertex_remap.get(&Name::from("post")),
            Some(&Name::from("note"))
        );
        pp_buf_free(out);

        assert_eq!(pp_handle_free(mig_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn serialize_compiled_rejects_non_migration_handle() {
        let src_h = alloc_schema(&source_schema());

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_mig_serialize_compiled(src_h, &mut out);
        assert_eq!(status, PpStatus::TypeMismatch as i32);

        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
    }

    #[test]
    fn lift_json_round_trips() {
        let (mig_h, src_h, tgt_h) = compile_rename();

        let json = slice(br#"{"text": "hi there"}"#);
        let root = slice(b"post");
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_mig_lift_json(mig_h, json.as_ref(), root.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Ok as i32);
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert!(value.is_object(), "expected JSON object, got {value:?}");
        pp_buf_free(out);

        assert_eq!(pp_handle_free(mig_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn coverage_reports_all_succeeded() {
        let (mig_h, src_h, tgt_h) = compile_rename();

        // Two parseable records should both lift cleanly.
        let cbor_a = json_to_cbor(src_h, br#"{"text": "a"}"#, "post");
        let cbor_b = json_to_cbor(src_h, br#"{"text": "b"}"#, "post");
        let inst_a: WInstance = decode(&cbor_a).unwrap();
        let inst_b: WInstance = decode(&cbor_b).unwrap();
        let batch = encode(&vec![inst_a, inst_b]).unwrap();

        let batch_slice = slice(&batch);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_mig_coverage(mig_h, src_h, tgt_h, batch_slice.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Ok as i32);
        let report: CoverageReport = decode(&out).unwrap();
        assert_eq!(report.total, 2);
        assert_eq!(report.succeeded, 2);
        assert_eq!(report.failed, 0);
        assert!((report.coverage_percent - 100.0).abs() < 1e-9);
        pp_buf_free(out);

        assert_eq!(pp_handle_free(mig_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn compose_yields_a_liftable_migration_handle() {
        // Compose the rename migration with itself; the composed handle
        // is a bare Migration that lift_record still accepts.
        let (mig_h, src_h, tgt_h) = compile_rename();
        let (mig_h2, src_h2, tgt_h2) = compile_rename();

        let mut composed: u32 = u32::MAX;
        let status = pp_mig_compose(mig_h, mig_h2, &mut composed);
        assert_eq!(status, PpStatus::Ok as i32);
        assert_ne!(composed, u32::MAX);

        // The composed resource accepts a record (as a bare Migration,
        // which exercises helpers::build_minimal_schema).
        let cbor = json_to_cbor(src_h, br#"{"text": "x"}"#, "post");
        let rec_slice = slice(&cbor);
        let mut lifted: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_mig_lift_record(composed, rec_slice.as_ref(), &mut lifted);
        assert_eq!(status, PpStatus::Ok as i32);
        pp_buf_free(lifted);

        assert_eq!(pp_handle_free(composed), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(mig_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(mig_h2), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h2), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h2), PpStatus::Ok as i32);
    }

    #[test]
    fn invert_bijective_migration_round_trips() {
        let src = source_schema();
        let tgt = target_schema();
        let migration = rename_migration(&src, &tgt);

        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);

        let mapping = encode(&migration).unwrap();
        let mapping_slice = slice(&mapping);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_mig_invert(mapping_slice.as_ref(), src_h, tgt_h, &mut out);
        assert_eq!(status, PpStatus::Ok as i32);
        let inverse: Migration = decode(&out).unwrap();
        // The inverse maps the target `note` back to the source `post`.
        assert_eq!(
            inverse.vertex_map.get(&Name::from("note")),
            Some(&Name::from("post"))
        );
        pp_buf_free(out);

        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn compile_with_invalid_schema_handle_errors() {
        let src = source_schema();
        let tgt = target_schema();
        let migration = rename_migration(&src, &tgt);

        let mapping = encode(&migration).unwrap();
        let mapping_slice = slice(&mapping);
        let mut mig_h: u32 = u32::MAX;
        let status = pp_mig_compile(
            u32::MAX - 1,
            u32::MAX - 2,
            mapping_slice.as_ref(),
            &mut mig_h,
        );
        assert_eq!(status, PpStatus::InvalidHandle as i32);
    }

    #[test]
    fn lift_record_rejects_garbage_record() {
        let (mig_h, src_h, tgt_h) = compile_rename();
        let bad = slice(&[0xFFu8, 0xFE, 0xFD]);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_mig_lift_record(mig_h, bad.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Serialization as i32);
        pp_buf_free(out);

        assert_eq!(pp_handle_free(mig_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn compose_rejects_non_migration_handle() {
        let proto = crate::api::helpers::default_protocol("p");
        let proto_h = handle::alloc(Resource::Protocol(Box::new(proto)));
        let (mig_h, src_h, tgt_h) = compile_rename();

        let mut out_handle: u32 = u32::MAX;
        let status = pp_mig_compose(proto_h, mig_h, &mut out_handle);
        assert_eq!(status, PpStatus::TypeMismatch as i32);

        assert_eq!(pp_handle_free(proto_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(mig_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }
}
