//! Data versioning: dataset storage and migration.
//!
//! Split out of the monolithic api.rs into a domain module.

use panproto_core::{
    inst::{self, WInstance},
    lens::{self},
    vcs::{self},
};
use wasm_bindgen::prelude::*;

use crate::error::WasmError;
use crate::slab::{self, Resource};

use super::helpers::{default_protocol, infer_root_vertex, lookup_builtin_protocol};

// ---------------------------------------------------------------------------
// Phase 7: Data versioning operations
// ---------------------------------------------------------------------------

/// Store a data set from JSON bytes, binding it to a schema.
///
/// The `data_json` bytes are a JSON-encoded array of records. The schema
/// handle identifies which schema this data conforms to.
///
/// Returns a handle to the stored `DataSet` resource.
///
/// # Errors
///
/// Returns `JsError` if the schema handle is invalid or JSON parsing fails.
#[wasm_bindgen]
pub fn store_dataset(schema_handle: u32, data_json: &[u8]) -> Result<u32, JsError> {
    let schema = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;

    // Parse JSON into instances
    let json_value: serde_json::Value =
        serde_json::from_slice(data_json).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("JSON parse: {e}"),
        })?;

    // Treat the input as an array of records; single objects become a one-element array
    let records: Vec<serde_json::Value> = match json_value {
        serde_json::Value::Array(arr) => arr,
        other => vec![other],
    };

    // Parse each record into a WInstance
    let root = infer_root_vertex(&schema);
    let mut instances = Vec::new();
    for record in &records {
        let instance =
            inst::parse_json(&schema, &root, record).map_err(|e| WasmError::ParseFailed {
                reason: format!("parse instance: {e}"),
            })?;
        instances.push(instance);
    }

    // Serialize instances as msgpack and compute a schema id
    let data_bytes =
        rmp_serde::to_vec_named(&instances).map_err(|e| WasmError::SerializationFailed {
            reason: format!("serialize instances: {e}"),
        })?;

    let schema_id = vcs::hash::hash_schema(&schema).map_err(|e| WasmError::VcsError {
        reason: format!("hash schema: {e}"),
    })?;

    let ds = vcs::DataSetObject {
        schema_id,
        data: data_bytes,
        record_count: instances.len() as u64,
        // This FFI stages raw bytes with no caller path or key.
        key: None,
    };

    Ok(slab::alloc(Resource::DataSet(Box::new(ds))))
}

/// Retrieve a data set as JSON bytes.
///
/// Returns a JSON-encoded array of records.
///
/// # Errors
///
/// Returns `JsError` if the handle is invalid or deserialization fails.
#[wasm_bindgen]
pub fn get_dataset(dataset_handle: u32) -> Result<Vec<u8>, JsError> {
    let ds = slab::with_resource(dataset_handle, |r| Ok(slab::as_dataset(r)?.clone()))?;

    let instances: Vec<WInstance> =
        rmp_serde::from_slice(&ds.data).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("deserialize instances: {e}"),
        })?;

    // Convert each instance to JSON using a minimal schema lookup
    // Return the raw msgpack-encoded instances for interop
    rmp_serde::to_vec_named(&instances).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: format!("serialize: {e}"),
        }
        .into()
    })
}

/// Migrate a data set forward between two schemas.
///
/// Auto-generates a lens between the source and target schemas,
/// then applies it to each record in the data set. Returns
/// `MessagePack`-encoded `{ data_handle: u32, complement_handle: u32 }`.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid, lens generation fails,
/// or migration fails.
#[wasm_bindgen]
pub fn migrate_dataset_forward(
    dataset_handle: u32,
    src_schema: u32,
    tgt_schema: u32,
) -> Result<Vec<u8>, JsError> {
    Ok(migrate_dataset_forward_inner(
        dataset_handle,
        src_schema,
        tgt_schema,
    )?)
}

/// The body of [`migrate_dataset_forward`], in [`WasmError`] terms.
///
/// Splitting the body out keeps the failure branches reachable from a
/// host `cargo test`: constructing a `JsError` needs a JS runtime, so an
/// entry point that returns one can only be driven down its happy path
/// off wasm32.
fn migrate_dataset_forward_inner(
    dataset_handle: u32,
    src_schema: u32,
    tgt_schema: u32,
) -> Result<Vec<u8>, WasmError> {
    // Clone the dataset
    let ds = slab::try_get(dataset_handle, |r| Ok(slab::as_dataset(r)?.clone()))?;

    // Clone both schemas
    let (src, tgt) = slab::try_get_two(src_schema, tgt_schema, |r1, r2| {
        let s1 = slab::as_schema(r1)?;
        let s2 = slab::as_schema(r2)?;
        Ok((s1.clone(), s2.clone()))
    })?;

    // Look up or construct protocol
    let protocol =
        lookup_builtin_protocol(&src.protocol).unwrap_or_else(|| default_protocol(&src.protocol));

    // Deserialize instances
    let instances: Vec<WInstance> =
        rmp_serde::from_slice(&ds.data).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("deserialize: {e}"),
        })?;

    // Generate lens
    let config = lens::AutoLensConfig::default();
    let result = lens::auto_generate(&src, &tgt, &protocol, &config).map_err(|e| {
        WasmError::LensConstructionFailed {
            reason: format!("auto_generate: {e}"),
        }
    })?;

    // Apply get to each instance
    let mut migrated = Vec::new();
    let mut complements = Vec::new();
    for instance in &instances {
        let (view, complement) =
            lens::get(&result.lens, instance).map_err(|e| WasmError::LiftFailed {
                reason: format!("lens get: {e}"),
            })?;
        migrated.push(view);
        complements.push(complement);
    }

    // Build new DataSetObject
    let tgt_schema_id = vcs::hash::hash_schema(&tgt).map_err(|e| WasmError::VcsError {
        reason: format!("hash schema: {e}"),
    })?;

    // Build both carrier objects (the fallible encodes) before
    // allocating either handle, so a serialization failure on the second
    // carrier cannot leak the slab slot of the first: the caller only
    // learns the handle through a successful return, so a slot allocated
    // on the way to an error is unreachable and never freed.
    let new_ds = vcs::DataSetObject {
        schema_id: tgt_schema_id,
        data: rmp_serde::to_vec_named(&migrated).map_err(|e| WasmError::SerializationFailed {
            reason: format!("serialize: {e}"),
        })?,
        record_count: migrated.len() as u64,
        // The key identifies the record, so it survives migration.
        key: ds.key.clone(),
    };

    // The complement carrier rides in a DataSet resource whose `data`
    // field holds the MessagePack-encoded `Vec<Complement>`.
    let comp_ds = vcs::DataSetObject {
        schema_id: ds.schema_id,
        data: rmp_serde::to_vec_named(&complements).map_err(|e| {
            WasmError::SerializationFailed {
                reason: format!("serialize complement: {e}"),
            }
        })?,
        record_count: complements.len() as u64,
        // A complement carrier holds no record, so it carries no key.
        key: None,
    };

    let data_handle = slab::alloc(Resource::DataSet(Box::new(new_ds)));
    let complement_handle = slab::alloc(Resource::DataSet(Box::new(comp_ds)));

    let out = serde_json::json!({
        "data_handle": data_handle,
        "complement_handle": complement_handle,
    });

    // Encoding the envelope is the last fallible step, and it is the one
    // step that happens with handles already allocated. Release them if
    // it fails: the caller never learns the handles, so nothing else can.
    match rmp_serde::to_vec_named(&out) {
        Ok(bytes) => Ok(bytes),
        Err(e) => {
            slab::free(data_handle);
            slab::free(complement_handle);
            Err(WasmError::SerializationFailed {
                reason: e.to_string(),
            })
        }
    }
}

/// Migrate a data set backward using a stored complement.
///
/// The complement list must hold exactly one entry per record in the
/// data set. A mismatch is an error naming both lengths, rather than a
/// restore of the shorter of the two whose `record_count` reports the
/// truncated set as if it were complete.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid, lens generation fails,
/// the complement count does not match the record count, or migration
/// fails.
#[wasm_bindgen]
pub fn migrate_dataset_backward(
    dataset_handle: u32,
    complement_bytes: &[u8],
    src_schema: u32,
    tgt_schema: u32,
) -> Result<u32, JsError> {
    Ok(migrate_dataset_backward_inner(
        dataset_handle,
        complement_bytes,
        src_schema,
        tgt_schema,
    )?)
}

/// The body of [`migrate_dataset_backward`], in [`WasmError`] terms.
///
/// Split out for the same reason as
/// [`migrate_dataset_forward_inner`]: the failure branches are only
/// reachable from a host test when the error type is not `JsError`.
fn migrate_dataset_backward_inner(
    dataset_handle: u32,
    complement_bytes: &[u8],
    src_schema: u32,
    tgt_schema: u32,
) -> Result<u32, WasmError> {
    let ds = slab::try_get(dataset_handle, |r| Ok(slab::as_dataset(r)?.clone()))?;

    let (src, tgt) = slab::try_get_two(src_schema, tgt_schema, |r1, r2| {
        let s1 = slab::as_schema(r1)?;
        let s2 = slab::as_schema(r2)?;
        Ok((s1.clone(), s2.clone()))
    })?;

    let protocol =
        lookup_builtin_protocol(&src.protocol).unwrap_or_else(|| default_protocol(&src.protocol));

    let instances: Vec<WInstance> =
        rmp_serde::from_slice(&ds.data).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("deserialize data: {e}"),
        })?;

    let complements: Vec<lens::Complement> =
        rmp_serde::from_slice(complement_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("deserialize complement: {e}"),
        })?;

    let config = lens::AutoLensConfig::default();
    let result = lens::auto_generate(&src, &tgt, &protocol, &config).map_err(|e| {
        WasmError::LensConstructionFailed {
            reason: format!("auto_generate: {e}"),
        }
    })?;

    // Each record is restored from the complement recorded for it, so
    // the two lists must line up exactly. Zipping mismatched lists would
    // drop the unpaired tail and report a `record_count` matching the
    // truncated set, leaving the loss undetectable.
    if instances.len() != complements.len() {
        return Err(WasmError::PutFailed {
            reason: format!(
                "migrate backward: {} record(s) but {} complement(s); \
                 every record needs the complement recorded for it",
                instances.len(),
                complements.len()
            ),
        });
    }

    let mut restored = Vec::with_capacity(instances.len());
    for (inst, comp) in instances.iter().zip(complements.iter()) {
        let r = lens::put(&result.lens, inst, comp).map_err(|e| WasmError::PutFailed {
            reason: format!("lens put: {e}"),
        })?;
        restored.push(r);
    }

    let src_schema_id = vcs::hash::hash_schema(&src).map_err(|e| WasmError::VcsError {
        reason: format!("hash schema: {e}"),
    })?;

    let restored_ds = vcs::DataSetObject {
        schema_id: src_schema_id,
        data: rmp_serde::to_vec_named(&restored).map_err(|e| WasmError::SerializationFailed {
            reason: format!("serialize: {e}"),
        })?,
        record_count: restored.len() as u64,
        key: ds.key,
    };

    Ok(slab::alloc(Resource::DataSet(Box::new(restored_ds))))
}

/// Check staleness: does this data set's schema match the given schema?
///
/// Returns `MessagePack`-encoded `{ stale: bool, data_schema_id: String, target_schema_id: String }`.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid.
#[wasm_bindgen]
pub fn check_dataset_staleness(
    dataset_handle: u32,
    schema_handle: u32,
) -> Result<Vec<u8>, JsError> {
    let ds = slab::with_resource(dataset_handle, |r| Ok(slab::as_dataset(r)?.clone()))?;
    let schema = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;

    let target_schema_id = vcs::hash::hash_schema(&schema).map_err(|e| WasmError::VcsError {
        reason: format!("hash schema: {e}"),
    })?;

    let result = serde_json::json!({
        "stale": ds.schema_id != target_schema_id,
        "data_schema_id": ds.schema_id.to_string(),
        "target_schema_id": target_schema_id.to_string(),
    });

    rmp_serde::to_vec_named(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Store a protocol definition in the slab and return a handle.
///
/// The `protocol_bytes` are `MessagePack`-encoded `Protocol` data.
///
/// # Errors
///
/// Returns `JsError` if deserialization fails.
#[wasm_bindgen]
pub fn store_protocol_definition(protocol_bytes: &[u8]) -> Result<u32, JsError> {
    let protocol: panproto_core::schema::Protocol =
        rmp_serde::from_slice(protocol_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;
    Ok(slab::alloc(Resource::Protocol(protocol)))
}

/// Get the protocol definition from a handle as `MessagePack` bytes.
///
/// # Errors
///
/// Returns `JsError` if the handle is invalid or the resource is not a protocol.
#[wasm_bindgen]
pub fn get_protocol_definition(handle: u32) -> Result<Vec<u8>, JsError> {
    let protocol = slab::with_resource(handle, |r| Ok(slab::as_protocol(r)?.clone()))?;

    rmp_serde::to_vec_named(&protocol).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Get the complement from a forward migration result.
///
/// The `complement_bytes` are the raw complement data stored during
/// forward migration. Returns `MessagePack`-encoded complement data.
///
/// # Errors
///
/// Returns `JsError` if deserialization fails.
#[wasm_bindgen]
pub fn get_migration_complement(complement_bytes: &[u8]) -> Result<Vec<u8>, JsError> {
    // Validate the bytes are valid msgpack by round-tripping
    let complements: Vec<lens::Complement> =
        rmp_serde::from_slice(complement_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("complement: {e}"),
        })?;

    rmp_serde::to_vec_named(&complements).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Release a resource handle, making it available for reuse.
#[wasm_bindgen]
pub fn free_handle(handle: u32) {
    slab::free(handle);
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::api::test_support;

    /// Store three records, migrate forward, drop one complement, and
    /// migrate back. The short list must be refused: pairing records with
    /// complements positionally drops the unpaired record, and the
    /// restored data set's `record_count` matches the truncated set, so
    /// nothing in the result tells the caller a record went missing.
    #[test]
    fn migrate_backward_refuses_a_short_complement_list() {
        let src_h = test_support::schema_handle(&test_support::source_schema());
        let tgt_h = test_support::schema_handle(&test_support::target_schema());

        let records = serde_json::json!([
            {"text": "a", "subtitle": "one"},
            {"text": "b", "subtitle": "two"},
            {"text": "c", "subtitle": "three"},
        ]);
        let data_h = store_dataset(src_h, &serde_json::to_vec(&records).unwrap()).unwrap();

        let forward = migrate_dataset_forward_inner(data_h, src_h, tgt_h).unwrap();
        let handles: serde_json::Value = rmp_serde::from_slice(&forward).unwrap();
        let migrated_h = u32::try_from(handles["data_handle"].as_u64().unwrap()).unwrap();
        let comp_h = u32::try_from(handles["complement_handle"].as_u64().unwrap()).unwrap();

        let comp_bytes = slab::try_get(comp_h, |r| Ok(slab::as_dataset(r)?.data.clone())).unwrap();
        let mut complements: Vec<lens::Complement> = rmp_serde::from_slice(&comp_bytes).unwrap();
        assert_eq!(complements.len(), 3);
        complements.pop();
        let short = rmp_serde::to_vec_named(&complements).unwrap();

        match migrate_dataset_backward_inner(migrated_h, &short, src_h, tgt_h) {
            Err(err) => {
                let message = err.to_string();
                assert!(
                    message.contains('3') && message.contains('2'),
                    "the error must name both lengths, got: {message}"
                );
            }
            Ok(handle) => panic!(
                "a short complement list must fail, not truncate the restore \
                 into handle {handle}"
            ),
        }

        free_handle(data_h);
        free_handle(migrated_h);
        free_handle(comp_h);
        free_handle(src_h);
        free_handle(tgt_h);
    }

    #[test]
    fn store_get_and_free_protocol_definition() {
        let handle = store_protocol_definition(&test_support::protocol_msgpack()).unwrap();
        let round_trip = get_protocol_definition(handle).unwrap();
        assert!(
            !round_trip.is_empty(),
            "protocol should round-trip to bytes"
        );
        free_handle(handle);
    }
}
