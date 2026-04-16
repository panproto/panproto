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
    // Clone the dataset
    let ds = slab::with_resource(dataset_handle, |r| Ok(slab::as_dataset(r)?.clone()))?;

    // Clone both schemas
    let (src, tgt) = slab::with_two_resources(src_schema, tgt_schema, |r1, r2| {
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

    let new_ds = vcs::DataSetObject {
        schema_id: tgt_schema_id,
        data: rmp_serde::to_vec_named(&migrated).map_err(|e| WasmError::SerializationFailed {
            reason: format!("serialize: {e}"),
        })?,
        record_count: migrated.len() as u64,
    };

    let data_handle = slab::alloc(Resource::DataSet(Box::new(new_ds)));

    // Serialize complements
    let complement_bytes =
        rmp_serde::to_vec_named(&complements).map_err(|e| WasmError::SerializationFailed {
            reason: format!("serialize complement: {e}"),
        })?;

    // Store complement bytes in a DataSet resource (as raw carrier)
    let comp_ds = vcs::DataSetObject {
        schema_id: ds.schema_id,
        data: complement_bytes,
        record_count: complements.len() as u64,
    };
    let complement_handle = slab::alloc(Resource::DataSet(Box::new(comp_ds)));

    let out = serde_json::json!({
        "data_handle": data_handle,
        "complement_handle": complement_handle,
    });

    rmp_serde::to_vec_named(&out).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Migrate a data set backward using a stored complement.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid, lens generation fails,
/// or migration fails.
#[wasm_bindgen]
pub fn migrate_dataset_backward(
    dataset_handle: u32,
    complement_bytes: &[u8],
    src_schema: u32,
    tgt_schema: u32,
) -> Result<u32, JsError> {
    let ds = slab::with_resource(dataset_handle, |r| Ok(slab::as_dataset(r)?.clone()))?;

    let (src, tgt) = slab::with_two_resources(src_schema, tgt_schema, |r1, r2| {
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

    let mut restored = Vec::new();
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
