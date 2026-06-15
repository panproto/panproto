//! Data versioning: dataset storage and schema-aware migration.
//!
//! Ported from `panproto_wasm::api::data` (see
//! `crates/panproto-wasm/src/api/data.rs`), narrowed to the six entry
//! points the C ABI exposes. The WASM `WasmError`/`JsError` pair
//! becomes [`FfiError`], `rmp_serde` becomes [`crate::canonical`] (CBOR
//! via ciborium), and the WASM slab becomes [`crate::handle`]. A data
//! set lives in the slab as a
//! [`Resource::DataSet`](crate::handle::Resource) carrying a
//! `vcs::DataSetObject`; its `data` field holds the CBOR-encoded
//! `Vec<WInstance>` (the on-slab payload is panproto-c-internal, so the
//! encoding matches what these entry points read back). The complement
//! carrier produced by a forward migration is also stored in a
//! `DataSet` resource whose `data` field is the CBOR-encoded
//! `Vec<Complement>`.

use panproto_core::{
    inst::{self, WInstance},
    lens::{self, Complement},
    vcs::{self, DataSetObject},
};
use safer_ffi::prelude::*;

use crate::api::helpers::{infer_root_vertex, protocol_for_schema};
use crate::error::{FfiError, PpStatus};
use crate::handle::{self, Resource};
use crate::panic::guard;

/// Store a data set from JSON, binding it to a schema.
///
/// `schema_handle` is a [`Resource::Schema`](crate::handle::Resource)
/// handle; `data_json` is raw JSON bytes (an array of records, decoded
/// with `serde_json`; a bare object is treated as a one-element array).
/// Each record is parsed via `inst::parse_json` against the schema's
/// inferred root vertex, the instances are CBOR-encoded into a fresh
/// `vcs::DataSetObject`, and the schema is hashed via
/// `vcs::hash::hash_schema`. On success, `out_handle` receives a fresh
/// [`Resource::DataSet`](crate::handle::Resource) handle.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_data_store_dataset(
    schema_handle: u32,
    data_json: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    guard(|| {
        let schema = handle::with_resource(schema_handle, |r| Ok(r.as_schema()?.clone()))?;

        // The store input is raw JSON, not CBOR: decode with serde_json.
        let json_value: serde_json::Value = serde_json::from_slice(data_json.as_slice())
            .map_err(|e| FfiError::Serialization(format!("JSON parse: {e}")))?;

        // Treat the input as an array of records; a bare object becomes a
        // one-element array.
        let records: Vec<serde_json::Value> = match json_value {
            serde_json::Value::Array(arr) => arr,
            other => vec![other],
        };

        let root = infer_root_vertex(&schema);
        let mut instances = Vec::with_capacity(records.len());
        for record in &records {
            let instance = inst::parse_json(&schema, &root, record)
                .map_err(|e| FfiError::Operation(format!("parse instance: {e}")))?;
            instances.push(instance);
        }

        let data_bytes = crate::canonical::encode(&instances)?;
        let schema_id = vcs::hash::hash_schema(&schema)
            .map_err(|e| FfiError::Operation(format!("hash schema: {e}")))?;

        #[allow(clippy::cast_possible_truncation)]
        let record_count = instances.len() as u64;
        let ds = DataSetObject {
            schema_id,
            data: data_bytes,
            record_count,
        };

        *out_handle = handle::alloc(Resource::DataSet(Box::new(ds)));
        Ok(PpStatus::Ok)
    })
}

/// Retrieve a data set as CBOR-encoded instances.
///
/// `dataset_handle` is a [`Resource::DataSet`](crate::handle::Resource)
/// handle. On success, `out` receives the CBOR-encoded
/// `Vec<WInstance>`. The stored payload is decoded and re-encoded so a
/// corrupt carrier surfaces as a serialization error rather than
/// returning opaque bytes.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_data_get_dataset(dataset_handle: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let data = handle::with_resource(dataset_handle, |r| Ok(r.as_dataset()?.data.clone()))?;

        let instances: Vec<WInstance> = crate::canonical::decode(&data)?;
        let bytes = crate::canonical::encode(&instances)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Migrate a data set forward between two schemas.
///
/// `dataset_handle` is a data set handle; `src_schema` and `tgt_schema`
/// are [`Resource::Schema`](crate::handle::Resource) handles.
/// Auto-generates a lens via `lens::auto_generate`, applies `lens::get`
/// per record, and stores both the migrated data set and the
/// complement carrier as new
/// [`Resource::DataSet`](crate::handle::Resource) handles, returned via
/// `out_data_handle` and `out_complement_handle`. The migrated data
/// set is hashed against the target schema; the complement carrier
/// keeps the source schema id and holds the CBOR-encoded
/// `Vec<Complement>` in its `data` field.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_data_migrate_forward(
    dataset_handle: u32,
    src_schema: u32,
    tgt_schema: u32,
    out_data_handle: &mut u32,
    out_complement_handle: &mut u32,
) -> i32 {
    guard(|| {
        let ds = handle::with_resource(dataset_handle, |r| Ok(r.as_dataset()?.clone()))?;
        let (src, tgt) = handle::with_two_resources(src_schema, tgt_schema, |r1, r2| {
            Ok((r1.as_schema()?.clone(), r2.as_schema()?.clone()))
        })?;

        let protocol = protocol_for_schema(&src);
        let instances: Vec<WInstance> = crate::canonical::decode(&ds.data)?;

        let config = lens::AutoLensConfig::default();
        let result = lens::auto_generate(&src, &tgt, &protocol, &config)
            .map_err(|e| FfiError::Operation(format!("auto_generate: {e}")))?;

        let mut migrated = Vec::with_capacity(instances.len());
        let mut complements = Vec::with_capacity(instances.len());
        for instance in &instances {
            let (view, complement) = lens::get(&result.lens, instance)
                .map_err(|e| FfiError::Operation(format!("lens get: {e}")))?;
            migrated.push(view);
            complements.push(complement);
        }

        let tgt_schema_id = vcs::hash::hash_schema(&tgt)
            .map_err(|e| FfiError::Operation(format!("hash schema: {e}")))?;

        #[allow(clippy::cast_possible_truncation)]
        let migrated_count = migrated.len() as u64;
        let new_ds = DataSetObject {
            schema_id: tgt_schema_id,
            data: crate::canonical::encode(&migrated)?,
            record_count: migrated_count,
        };
        let data_handle = handle::alloc(Resource::DataSet(Box::new(new_ds)));

        // The complement carrier rides in a DataSet resource whose `data`
        // field is the CBOR-encoded `Vec<Complement>`. It keeps the source
        // schema id so a later backward migration can be sanity-checked.
        #[allow(clippy::cast_possible_truncation)]
        let complement_count = complements.len() as u64;
        let comp_ds = DataSetObject {
            schema_id: ds.schema_id,
            data: crate::canonical::encode(&complements)?,
            record_count: complement_count,
        };
        let complement_handle = handle::alloc(Resource::DataSet(Box::new(comp_ds)));

        *out_data_handle = data_handle;
        *out_complement_handle = complement_handle;
        Ok(PpStatus::Ok)
    })
}

/// Migrate a data set backward using a stored complement.
///
/// `dataset_handle` is the migrated (forward) data set handle;
/// `complement` is the CBOR-encoded `Vec<Complement>` produced by the
/// forward migration; `src_schema` and `tgt_schema` are
/// [`Resource::Schema`](crate::handle::Resource) handles (the same
/// pair, in the same order, as the forward migration). Auto-generates
/// the lens and applies `lens::put` per record, pairing each migrated
/// view with its complement. On success, `out_handle` receives a fresh
/// [`Resource::DataSet`](crate::handle::Resource) handle re-anchored to
/// the source schema.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_data_migrate_backward(
    dataset_handle: u32,
    complement: c_slice::Ref<'_, u8>,
    src_schema: u32,
    tgt_schema: u32,
    out_handle: &mut u32,
) -> i32 {
    guard(|| {
        let ds = handle::with_resource(dataset_handle, |r| Ok(r.as_dataset()?.clone()))?;
        let (src, tgt) = handle::with_two_resources(src_schema, tgt_schema, |r1, r2| {
            Ok((r1.as_schema()?.clone(), r2.as_schema()?.clone()))
        })?;

        let protocol = protocol_for_schema(&src);
        let views: Vec<WInstance> = crate::canonical::decode(&ds.data)?;
        let complements: Vec<Complement> = crate::canonical::decode(complement.as_slice())?;

        let config = lens::AutoLensConfig::default();
        let result = lens::auto_generate(&src, &tgt, &protocol, &config)
            .map_err(|e| FfiError::Operation(format!("auto_generate: {e}")))?;

        let mut restored = Vec::with_capacity(views.len());
        for (view, comp) in views.iter().zip(complements.iter()) {
            let r = lens::put(&result.lens, view, comp)
                .map_err(|e| FfiError::Operation(format!("lens put: {e}")))?;
            restored.push(r);
        }

        let src_schema_id = vcs::hash::hash_schema(&src)
            .map_err(|e| FfiError::Operation(format!("hash schema: {e}")))?;

        #[allow(clippy::cast_possible_truncation)]
        let restored_count = restored.len() as u64;
        let restored_ds = DataSetObject {
            schema_id: src_schema_id,
            data: crate::canonical::encode(&restored)?,
            record_count: restored_count,
        };

        *out_handle = handle::alloc(Resource::DataSet(Box::new(restored_ds)));
        Ok(PpStatus::Ok)
    })
}

/// Check whether a data set's schema matches a given schema.
///
/// `dataset_handle` is a data set handle; `schema_handle` is a
/// [`Resource::Schema`](crate::handle::Resource) handle. On success,
/// `out` receives a CBOR-encoded record (`stale`, `data_schema_id`,
/// `target_schema_id`) by comparing `vcs::hash::hash_schema` outputs:
/// the data set is stale when its stored schema id differs from the
/// hash of the supplied schema.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_data_check_staleness(
    dataset_handle: u32,
    schema_handle: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let data_schema_id =
            handle::with_resource(dataset_handle, |r| Ok(r.as_dataset()?.schema_id))?;
        let schema = handle::with_resource(schema_handle, |r| Ok(r.as_schema()?.clone()))?;

        let target_schema_id = vcs::hash::hash_schema(&schema)
            .map_err(|e| FfiError::Operation(format!("hash schema: {e}")))?;

        let report = StalenessReport {
            stale: data_schema_id != target_schema_id,
            data_schema_id: data_schema_id.to_string(),
            target_schema_id: target_schema_id.to_string(),
        };

        let bytes = crate::canonical::encode(&report)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Round-trip and return a forward-migration complement carrier.
///
/// `complement` is the CBOR-encoded `Vec<Complement>` produced by a
/// forward migration. The payload is decoded and re-encoded so a
/// malformed carrier surfaces as a serialization error; on success,
/// `out` receives the re-encoded complement bytes.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_data_get_migration_complement(
    complement: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let complements: Vec<Complement> = crate::canonical::decode(complement.as_slice())?;
        let bytes = crate::canonical::encode(&complements)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// The staleness report returned by [`pp_data_check_staleness`].
///
/// Mirrors the JSON shape `panproto_wasm::api::data::check_dataset_staleness`
/// produces (`stale`, `data_schema_id`, `target_schema_id`); the C ABI
/// returns it CBOR-encoded. The schema ids are the hex strings of the
/// `vcs::hash::ObjectId` values.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct StalenessReport {
    /// Whether the data set's schema differs from the supplied schema.
    stale: bool,
    /// Hex-encoded id of the schema the data set was stored against.
    data_schema_id: String,
    /// Hex-encoded id of the supplied (target) schema.
    target_schema_id: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use panproto_core::schema::{Schema, SchemaBuilder};

    use super::*;
    use crate::api::{pp_buf_free, pp_handle_free};

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

    fn alloc_schema(s: &Schema) -> u32 {
        handle::alloc(Resource::Schema(Arc::new(s.clone())))
    }

    fn slice(bytes: &[u8]) -> c_slice::Box<u8> {
        bytes.to_vec().into_boxed_slice().into()
    }

    /// Store a small JSON-array data set against `schema_h`, returning
    /// the fresh data set handle.
    fn store(schema_h: u32, json: &[u8]) -> u32 {
        let json_slice = slice(json);
        let mut ds_h: u32 = u32::MAX;
        let status = pp_data_store_dataset(schema_h, json_slice.as_ref(), &mut ds_h);
        assert_eq!(status, PpStatus::Ok as i32, "store_dataset failed");
        assert_ne!(ds_h, u32::MAX);
        ds_h
    }

    #[test]
    fn store_then_get_round_trips() {
        let src = source_schema();
        let src_h = alloc_schema(&src);
        let ds_h = store(src_h, br#"[{"text": "hello"}, {"text": "world"}]"#);

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_data_get_dataset(ds_h, &mut out);
        assert_eq!(status, PpStatus::Ok as i32);
        let instances: Vec<WInstance> = crate::canonical::decode(&out).unwrap();
        assert_eq!(instances.len(), 2);
        pp_buf_free(out);

        assert_eq!(pp_handle_free(ds_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
    }

    #[test]
    fn store_accepts_bare_object() {
        let src = source_schema();
        let src_h = alloc_schema(&src);
        let ds_h = store(src_h, br#"{"text": "lonely"}"#);

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_data_get_dataset(ds_h, &mut out), PpStatus::Ok as i32);
        let instances: Vec<WInstance> = crate::canonical::decode(&out).unwrap();
        assert_eq!(instances.len(), 1);
        pp_buf_free(out);

        assert_eq!(pp_handle_free(ds_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
    }

    #[test]
    fn check_staleness_matches_and_mismatches() {
        let src = source_schema();
        let tgt = target_schema();
        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);
        let ds_h = store(src_h, br#"[{"text": "a"}]"#);

        // Same schema: not stale.
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_data_check_staleness(ds_h, src_h, &mut out),
            PpStatus::Ok as i32
        );
        let report: StalenessReport = crate::canonical::decode(&out).unwrap();
        assert!(!report.stale);
        assert_eq!(report.data_schema_id, report.target_schema_id);
        pp_buf_free(out);

        // Different schema: stale.
        let mut out2: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_data_check_staleness(ds_h, tgt_h, &mut out2),
            PpStatus::Ok as i32
        );
        let report2: StalenessReport = crate::canonical::decode(&out2).unwrap();
        assert!(report2.stale);
        assert_ne!(report2.data_schema_id, report2.target_schema_id);
        pp_buf_free(out2);

        assert_eq!(pp_handle_free(ds_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn migrate_forward_then_backward_round_trips() {
        let src = source_schema();
        let tgt = target_schema();
        let src_h = alloc_schema(&src);
        let tgt_h = alloc_schema(&tgt);
        let ds_h = store(src_h, br#"[{"text": "hi"}, {"text": "there"}]"#);

        // Forward: produces a migrated data set plus a complement carrier.
        let mut data_h: u32 = u32::MAX;
        let mut comp_h: u32 = u32::MAX;
        let status = pp_data_migrate_forward(ds_h, src_h, tgt_h, &mut data_h, &mut comp_h);
        assert_eq!(status, PpStatus::Ok as i32);
        assert_ne!(data_h, u32::MAX);
        assert_ne!(comp_h, u32::MAX);

        // The migrated data set is no longer stale against the target.
        let mut sout: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_data_check_staleness(data_h, tgt_h, &mut sout),
            PpStatus::Ok as i32
        );
        let sreport: StalenessReport = crate::canonical::decode(&sout).unwrap();
        assert!(!sreport.stale, "migrated data should match target schema");
        pp_buf_free(sout);

        // Pull the complement bytes out of the complement carrier's `data`
        // field. The carrier holds `Vec<Complement>`, so `pp_data_get_dataset`
        // (which expects `Vec<WInstance>`) would mis-decode it; read the slab
        // directly instead.
        let comp_bytes =
            handle::with_resource(comp_h, |r| Ok(r.as_dataset()?.data.clone())).unwrap();

        // get_migration_complement validates the carrier round-trips.
        let comp_slice = slice(&comp_bytes);
        let mut comp_validated: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_data_get_migration_complement(comp_slice.as_ref(), &mut comp_validated),
            PpStatus::Ok as i32
        );
        let comp_validated_bytes = comp_validated.to_vec();
        pp_buf_free(comp_validated);

        // Backward: restore the source-shaped data set.
        let comp_slice2 = slice(&comp_validated_bytes);
        let mut restored_h: u32 = u32::MAX;
        let status =
            pp_data_migrate_backward(data_h, comp_slice2.as_ref(), src_h, tgt_h, &mut restored_h);
        assert_eq!(status, PpStatus::Ok as i32);
        assert_ne!(restored_h, u32::MAX);

        // The restored data set matches the source schema again.
        let mut rout: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_data_check_staleness(restored_h, src_h, &mut rout),
            PpStatus::Ok as i32
        );
        let rreport: StalenessReport = crate::canonical::decode(&rout).unwrap();
        assert!(!rreport.stale, "restored data should match source schema");
        pp_buf_free(rout);

        // Round-trip preserves record count.
        let mut got: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_data_get_dataset(restored_h, &mut got),
            PpStatus::Ok as i32
        );
        let restored: Vec<WInstance> = crate::canonical::decode(&got).unwrap();
        assert_eq!(restored.len(), 2);
        pp_buf_free(got);

        assert_eq!(pp_handle_free(ds_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(data_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(comp_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(restored_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn store_rejects_invalid_schema_handle() {
        let json = slice(br#"[{"text": "x"}]"#);
        let mut out_handle: u32 = u32::MAX;
        let status = pp_data_store_dataset(u32::MAX - 1, json.as_ref(), &mut out_handle);
        assert_eq!(status, PpStatus::InvalidHandle as i32);
    }

    #[test]
    fn store_rejects_garbage_json() {
        let src = source_schema();
        let src_h = alloc_schema(&src);
        let bad = slice(&[0xFFu8, 0xFE, 0xFD]);
        let mut out_handle: u32 = u32::MAX;
        let status = pp_data_store_dataset(src_h, bad.as_ref(), &mut out_handle);
        assert_eq!(status, PpStatus::Serialization as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
    }

    #[test]
    fn get_dataset_rejects_schema_handle() {
        let src = source_schema();
        let src_h = alloc_schema(&src);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_data_get_dataset(src_h, &mut out);
        assert_eq!(status, PpStatus::TypeMismatch as i32);
        pp_buf_free(out);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
    }

    #[test]
    fn get_migration_complement_rejects_garbage() {
        let bad = slice(&[0xFFu8, 0xFE, 0xFD]);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_data_get_migration_complement(bad.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Serialization as i32);
        pp_buf_free(out);
    }
}
