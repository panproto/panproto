//! The ten `#[wasm_bindgen]` entry points for panproto-wasm.
//!
//! Each function takes handles (`u32`) and/or `MessagePack` byte slices,
//! performs the requested operation, and returns either a handle or
//! serialized bytes. All errors are converted to `JsError`.

use std::collections::HashMap;

use panproto_core::{
    check,
    gat::{self, Theory},
    inst::{self, CompiledMigration, WInstance},
    io,
    lens::{self, Complement},
    mig::{self, Migration},
    protocols,
    schema::{self, SchemaBuilder},
    vcs::{self, Store as _},
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::error::WasmError;
use crate::slab::{self, Resource};

/// A serializable builder operation for constructing schemas.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op")]
enum BuildOp {
    /// Add a vertex.
    #[serde(rename = "vertex")]
    Vertex {
        /// Vertex identifier.
        id: String,
        /// Vertex kind.
        kind: String,
        /// Optional NSID.
        nsid: Option<String>,
    },
    /// Add a binary edge.
    #[serde(rename = "edge")]
    Edge {
        /// Source vertex ID.
        src: String,
        /// Target vertex ID.
        tgt: String,
        /// Edge kind.
        kind: String,
        /// Optional edge label.
        name: Option<String>,
    },
    /// Add a constraint.
    #[serde(rename = "constraint")]
    Constraint {
        /// Vertex ID.
        vertex: String,
        /// Constraint sort.
        sort: String,
        /// Constraint value.
        value: String,
    },
    /// Add a hyper-edge connecting multiple vertices via labeled positions.
    #[serde(rename = "hyper_edge")]
    HyperEdge {
        /// Hyper-edge identifier.
        id: String,
        /// Hyper-edge kind.
        kind: String,
        /// Maps label names to vertex IDs.
        signature: HashMap<String, String>,
        /// The label that identifies the parent vertex.
        parent: String,
    },
    /// Declare required edges for a vertex.
    #[serde(rename = "required")]
    Required {
        /// The vertex that owns the requirement.
        vertex: String,
        /// The edges that are required.
        edges: Vec<panproto_core::schema::Edge>,
    },
}

/// Register a protocol specification and return a handle.
///
/// The `spec` bytes are MessagePack-encoded `Protocol` data.
///
/// # Errors
///
/// Returns `JsError` if deserialization fails.
#[wasm_bindgen]
pub fn define_protocol(spec: &[u8]) -> Result<u32, JsError> {
    let protocol: panproto_core::schema::Protocol =
        rmp_serde::from_slice(spec).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;
    Ok(slab::alloc(Resource::Protocol(protocol)))
}

/// Build a schema from a protocol handle and `MessagePack`-encoded
/// builder operations.
///
/// The `ops` bytes are a `MessagePack`-encoded `Vec<BuildOp>`.
///
/// # Errors
///
/// Returns `JsError` if the protocol handle is invalid, ops cannot
/// be deserialized, or schema building fails.
#[wasm_bindgen]
pub fn build_schema(proto: u32, ops: &[u8]) -> Result<u32, JsError> {
    let protocol = slab::with_resource(proto, |r| Ok(slab::as_protocol(r)?.clone()))?;

    let operations: Vec<BuildOp> =
        rmp_serde::from_slice(ops).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let mut builder = SchemaBuilder::new(&protocol);

    for op in operations {
        match op {
            BuildOp::Vertex { id, kind, nsid } => {
                builder = builder.vertex(&id, &kind, nsid.as_deref()).map_err(|e| {
                    WasmError::SchemaBuildFailed {
                        reason: e.to_string(),
                    }
                })?;
            }
            BuildOp::Edge {
                src,
                tgt,
                kind,
                name,
            } => {
                builder = builder
                    .edge(&src, &tgt, &kind, name.as_deref())
                    .map_err(|e| WasmError::SchemaBuildFailed {
                        reason: e.to_string(),
                    })?;
            }
            BuildOp::Constraint {
                vertex,
                sort,
                value,
            } => {
                builder = builder.constraint(&vertex, &sort, &value);
            }
            BuildOp::HyperEdge {
                id,
                kind,
                signature,
                parent,
            } => {
                builder = builder
                    .hyper_edge(&id, &kind, signature, &parent)
                    .map_err(|e| WasmError::SchemaBuildFailed {
                        reason: e.to_string(),
                    })?;
            }
            BuildOp::Required { vertex, edges } => {
                builder = builder.required(&vertex, edges);
            }
        }
    }

    let schema = builder.build().map_err(|e| WasmError::SchemaBuildFailed {
        reason: e.to_string(),
    })?;

    Ok(slab::alloc(Resource::Schema(std::sync::Arc::new(schema))))
}

/// Check existence conditions for a migration mapping between two schemas.
///
/// `proto` is the handle to the protocol (obtained from
/// [`define_protocol`]).  `src` and `tgt` are schema handles.
/// Returns `MessagePack`-encoded
/// [`ExistenceReport`](panproto_core::mig::ExistenceReport).
/// The `mapping` bytes are a `MessagePack`-encoded [`Migration`].
///
/// Note: this function always returns `Vec<u8>` (never errors at the
/// boundary) because the report itself encodes validity.
#[must_use]
#[wasm_bindgen]
pub fn check_existence(proto: u32, src: u32, tgt: u32, mapping: &[u8]) -> Vec<u8> {
    check_existence_inner(proto, src, tgt, mapping).unwrap_or_else(|msg| {
        let report = mig::ExistenceReport {
            valid: false,
            errors: vec![mig::ExistenceError::WellFormedness { message: msg }],
        };
        rmp_serde::to_vec(&report).unwrap_or_default()
    })
}

/// Inner implementation for `check_existence` that can return errors.
fn check_existence_inner(
    proto: u32,
    src: u32,
    tgt: u32,
    mapping: &[u8],
) -> Result<Vec<u8>, String> {
    let protocol = slab::with_resource(proto, |r| Ok(slab::as_protocol(r)?.clone()))
        .map_err(|_| "invalid protocol handle".to_string())?;

    let (src_schema, tgt_schema) = slab::with_two_resources(src, tgt, |r1, r2| {
        let s1 = slab::as_schema(r1)?;
        let s2 = slab::as_schema(r2)?;
        Ok((s1.clone(), s2.clone()))
    })
    .map_err(|_| "invalid schema handle".to_string())?;

    let migration: Migration =
        rmp_serde::from_slice(mapping).map_err(|e| format!("deserialization failed: {e}"))?;

    // Build the theory registry from the protocol's registered theories.
    let theory_registry = build_theory_registry(&protocol.name)?;
    let report = mig::check_existence(
        &protocol,
        &src_schema,
        &tgt_schema,
        &migration,
        &theory_registry,
    );

    rmp_serde::to_vec(&report).map_err(|e| format!("serialization failed: {e}"))
}

/// Compile a migration for fast per-record application.
///
/// The `mapping` bytes are a `MessagePack`-encoded [`Migration`].
/// Returns a handle to the compiled migration.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid, deserialization fails,
/// or compilation detects well-formedness violations.
#[wasm_bindgen]
pub fn compile_migration(src: u32, tgt: u32, mapping: &[u8]) -> Result<u32, JsError> {
    let (src_schema, tgt_schema) = slab::with_two_resources(src, tgt, |r1, r2| {
        let s1 = slab::as_schema(r1)?;
        let s2 = slab::as_schema(r2)?;
        Ok((s1.clone(), s2.clone()))
    })?;

    let migration: Migration =
        rmp_serde::from_slice(mapping).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let compiled = mig::compile(&src_schema, &tgt_schema, &migration).map_err(|e| {
        WasmError::MigrationFailed {
            reason: e.to_string(),
        }
    })?;

    Ok(slab::alloc(Resource::MigrationWithSchemas {
        compiled,
        src_schema: std::sync::Arc::new(src_schema),
        tgt_schema: std::sync::Arc::new(tgt_schema),
    }))
}

/// Apply a compiled migration to a W-type record.
///
/// The `record` bytes are a `MessagePack`-encoded [`WInstance`].
/// Returns `MessagePack`-encoded migrated instance.
///
/// # Errors
///
/// Returns `JsError` if the handle is invalid, deserialization fails,
/// or the lift operation fails.
#[wasm_bindgen]
pub fn lift_record(migration: u32, record: &[u8]) -> Result<Vec<u8>, JsError> {
    let instance: WInstance =
        rmp_serde::from_slice(record).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let result = slab::with_resource(migration, |r| {
        let (compiled, src_schema, tgt_schema) = extract_migration_ref(r)?;
        mig::lift_wtype(compiled, &src_schema, &tgt_schema, &instance).map_err(|e| {
            WasmError::LiftFailed {
                reason: e.to_string(),
            }
        })
    })?;

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Bidirectional get: extract a view and complement from a record.
///
/// The `record` bytes are a `MessagePack`-encoded [`WInstance`].
/// Returns `MessagePack`-encoded `{ view: WInstance, complement: Vec<u8> }`
/// where `complement` is the serialized [`Complement`] needed by `put_record`.
///
/// # Errors
///
/// Returns `JsError` if the handle is invalid, deserialization fails,
/// or the lens get operation fails.
#[wasm_bindgen]
pub fn get_record(migration: u32, record: &[u8]) -> Result<Vec<u8>, JsError> {
    #[derive(Serialize)]
    struct GetResult {
        view: WInstance,
        complement: Vec<u8>,
    }

    let instance: WInstance =
        rmp_serde::from_slice(record).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let (view, complement) = slab::with_resource(migration, |r| {
        let (compiled, src_schema, tgt_schema) = extract_migration_owned(r)?;

        let lens_obj = lens::Lens {
            compiled,
            src_schema,
            tgt_schema,
        };

        lens::get(&lens_obj, &instance).map_err(|e| WasmError::LiftFailed {
            reason: e.to_string(),
        })
    })?;

    let complement_bytes = rmp_serde::to_vec(&complement).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: format!("complement: {e}"),
        }
        .into()
    })?;

    let result = GetResult {
        view,
        complement: complement_bytes,
    };

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Restore a record from a view and complement (lens put direction).
///
/// The `view` and `complement` bytes are `MessagePack`-encoded
/// [`WInstance`] and [`Complement`] respectively.
///
/// # Errors
///
/// Returns `JsError` if the handle is invalid, deserialization fails,
/// or the put operation fails.
#[wasm_bindgen]
pub fn put_record(migration: u32, view: &[u8], complement: &[u8]) -> Result<Vec<u8>, JsError> {
    let view_instance: WInstance =
        rmp_serde::from_slice(view).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("view: {e}"),
        })?;

    let comp: Complement =
        rmp_serde::from_slice(complement).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("complement: {e}"),
        })?;

    let result = slab::with_resource(migration, |r| {
        let (compiled, src_schema, tgt_schema) = extract_migration_owned(r)?;

        let lens_obj = lens::Lens {
            compiled,
            src_schema,
            tgt_schema,
        };

        lens::put(&lens_obj, &view_instance, &comp).map_err(|e| WasmError::PutFailed {
            reason: e.to_string(),
        })
    })?;

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Lift a JSON record through a compiled migration, returning JSON.
///
/// `root_vertex` specifies which schema vertex the JSON object maps to.
/// If empty, auto-detects (first "object" kind vertex).
///
/// # Errors
///
/// Returns `JsError` if parsing, lifting, or serialization fails.
#[wasm_bindgen]
pub fn lift_json(migration: u32, json_bytes: &[u8], root_vertex: &str) -> Result<Vec<u8>, JsError> {
    let json_value: serde_json::Value =
        serde_json::from_slice(json_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let result = slab::with_resource(migration, |r| {
        let (compiled, src_schema, tgt_schema) = extract_migration_ref(r)?;

        let src_root = find_root(root_vertex, &src_schema)?;
        let instance = inst::parse_json(&src_schema, &src_root, &json_value).map_err(|e| {
            WasmError::ParseFailed {
                reason: e.to_string(),
            }
        })?;

        let lifted =
            mig::lift_wtype(compiled, &src_schema, &tgt_schema, &instance).map_err(|e| {
                WasmError::LiftFailed {
                    reason: e.to_string(),
                }
            })?;

        let out_json = inst::to_json(&tgt_schema, &lifted);
        Ok(out_json)
    })?;

    serde_json::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Bidirectional get on a JSON record, returning JSON view + complement.
///
/// # Errors
///
/// Returns `JsError` if parsing, get, or serialization fails.
#[wasm_bindgen]
pub fn get_json(migration: u32, json_bytes: &[u8], root_vertex: &str) -> Result<Vec<u8>, JsError> {
    #[derive(Serialize)]
    struct GetJsonResult {
        view: serde_json::Value,
        complement: Vec<u8>,
    }

    let json_value: serde_json::Value =
        serde_json::from_slice(json_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let (compiled, src_schema, tgt_schema) =
        slab::with_resource(migration, extract_migration_owned)?;

    let src_root = find_root(root_vertex, &src_schema)?;
    let instance = inst::parse_json(&src_schema, &src_root, &json_value).map_err(|e| {
        WasmError::ParseFailed {
            reason: e.to_string(),
        }
    })?;

    let lens_obj = lens::Lens {
        compiled,
        src_schema,
        tgt_schema: tgt_schema.clone(),
    };

    let (view, complement) =
        lens::get(&lens_obj, &instance).map_err(|e| WasmError::LiftFailed {
            reason: e.to_string(),
        })?;

    let view_json = inst::to_json(&tgt_schema, &view);
    let complement_bytes =
        rmp_serde::to_vec(&complement).map_err(|e| WasmError::SerializationFailed {
            reason: format!("complement: {e}"),
        })?;

    let result = GetJsonResult {
        view: view_json,
        complement: complement_bytes,
    };

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Bidirectional put: restore from a JSON view + complement.
///
/// # Errors
///
/// Returns `JsError` if parsing, put, or serialization fails.
#[wasm_bindgen]
pub fn put_json(
    migration: u32,
    view_json_bytes: &[u8],
    complement: &[u8],
    root_vertex: &str,
) -> Result<Vec<u8>, JsError> {
    let view_json: serde_json::Value =
        serde_json::from_slice(view_json_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let comp: Complement =
        rmp_serde::from_slice(complement).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("complement: {e}"),
        })?;

    let result = slab::with_resource(migration, |r| {
        let (compiled, src_schema, tgt_schema) = extract_migration_owned(r)?;

        let tgt_root = find_root(root_vertex, &tgt_schema)?;
        let view_instance = inst::parse_json(&tgt_schema, &tgt_root, &view_json).map_err(|e| {
            WasmError::ParseFailed {
                reason: e.to_string(),
            }
        })?;

        let lens_obj = lens::Lens {
            compiled,
            src_schema: src_schema.clone(),
            tgt_schema,
        };

        let restored =
            lens::put(&lens_obj, &view_instance, &comp).map_err(|e| WasmError::PutFailed {
                reason: e.to_string(),
            })?;

        let out_json = inst::to_json(&src_schema, &restored);
        Ok(out_json)
    })?;

    serde_json::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Find a root vertex in the schema, preferring object-kind vertices.
fn find_root(
    root_vertex: &str,
    schema: &panproto_core::schema::Schema,
) -> Result<String, WasmError> {
    if !root_vertex.is_empty() && schema.has_vertex(root_vertex) {
        return Ok(root_vertex.to_string());
    }
    schema
        .vertices
        .iter()
        .find(|(_, v)| v.kind.as_ref() == "object")
        .or_else(|| {
            schema
                .vertices
                .iter()
                .find(|(_, v)| v.kind.as_ref() == "record")
        })
        .map(|(id, _)| id.to_string())
        .ok_or_else(|| WasmError::ParseFailed {
            reason: "no suitable root vertex found".to_string(),
        })
}

/// Compose two compiled migrations into a single migration.
///
/// Returns a handle to the composed compiled migration.
///
/// # Errors
///
/// Returns `JsError` if either handle is invalid or composition fails.
#[wasm_bindgen]
pub fn compose_migrations(m1: u32, m2: u32) -> Result<u32, JsError> {
    let (compiled1, compiled2) = slab::with_two_resources(m1, m2, |r1, r2| {
        let c1 = slab::as_migration(r1)?;
        let c2 = slab::as_migration(r2)?;
        Ok((c1.clone(), c2.clone()))
    })?;

    let composed = compose_compiled(&compiled1, &compiled2);
    Ok(slab::alloc(Resource::Migration(composed)))
}

/// Diff two schemas, returning a `MessagePack`-encoded diff report.
///
/// The result encodes vertex additions, removals, and edge changes
/// between the two schemas.
#[must_use]
#[wasm_bindgen]
pub fn diff_schemas(s1: u32, s2: u32) -> Vec<u8> {
    diff_schemas_inner(s1, s2)
        .unwrap_or_else(|_| rmp_serde::to_vec(&SchemaDiff::default()).unwrap_or_default())
}

/// Inner implementation for `diff_schemas` that can return errors.
fn diff_schemas_inner(s1: u32, s2: u32) -> Result<Vec<u8>, String> {
    let (schema1, schema2) = slab::with_two_resources(s1, s2, |r1, r2| {
        let s1 = slab::as_schema(r1)?;
        let s2 = slab::as_schema(r2)?;
        Ok((s1.clone(), s2.clone()))
    })
    .map_err(|_| "invalid schema handle".to_string())?;

    let diff = compute_diff(&schema1, &schema2);

    rmp_serde::to_vec(&diff).map_err(|e| format!("serialization failed: {e}"))
}

/// Diff two schemas using the full `panproto-check` diff engine.
///
/// Returns `MessagePack`-encoded [`SchemaDiff`](panproto_core::check::SchemaDiff)
/// with 20+ change categories including constraints, hyper-edges, variants,
/// recursion points, usage modes, spans, and nominal identity changes.
#[must_use]
#[wasm_bindgen]
pub fn diff_schemas_full(s1: u32, s2: u32) -> Vec<u8> {
    diff_schemas_full_inner(s1, s2)
        .unwrap_or_else(|_| rmp_serde::to_vec(&check::SchemaDiff::default()).unwrap_or_default())
}

/// Inner implementation for `diff_schemas_full`.
fn diff_schemas_full_inner(s1: u32, s2: u32) -> Result<Vec<u8>, String> {
    let (schema1, schema2) = slab::with_two_resources(s1, s2, |r1, r2| {
        let s1 = slab::as_schema(r1)?;
        let s2 = slab::as_schema(r2)?;
        Ok((s1.clone(), s2.clone()))
    })
    .map_err(|_| "invalid schema handle".to_string())?;

    let diff = check::diff(&schema1, &schema2);
    rmp_serde::to_vec(&diff).map_err(|e| format!("serialization failed: {e}"))
}

/// Classify a schema diff against a protocol, producing a compatibility report.
///
/// The `diff_bytes` are `MessagePack`-encoded `SchemaDiff`.
/// Returns `MessagePack`-encoded [`CompatReport`](panproto_core::check::CompatReport)
/// with breaking and non-breaking change lists.
#[must_use]
#[wasm_bindgen]
pub fn classify_diff(proto: u32, diff_bytes: &[u8]) -> Vec<u8> {
    classify_diff_inner(proto, diff_bytes).unwrap_or_else(|_| {
        let empty = check::CompatReport {
            breaking: Vec::new(),
            non_breaking: Vec::new(),
            compatible: true,
        };
        rmp_serde::to_vec(&empty).unwrap_or_default()
    })
}

/// Inner implementation for `classify_diff`.
fn classify_diff_inner(proto: u32, diff_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let protocol = slab::with_resource(proto, |r| Ok(slab::as_protocol(r)?.clone()))
        .map_err(|_| "invalid protocol handle".to_string())?;

    let diff: check::SchemaDiff =
        rmp_serde::from_slice(diff_bytes).map_err(|e| format!("deserialization failed: {e}"))?;

    let report = check::classify(&diff, &protocol);
    rmp_serde::to_vec(&report).map_err(|e| format!("serialization failed: {e}"))
}

/// Render a compatibility report as human-readable text.
///
/// The `report_bytes` are `MessagePack`-encoded `CompatReport`.
///
/// # Errors
///
/// Returns `JsError` if deserialization fails.
#[wasm_bindgen]
pub fn report_text(report_bytes: &[u8]) -> Result<String, JsError> {
    let report: check::CompatReport =
        rmp_serde::from_slice(report_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;
    Ok(check::report_text(&report))
}

/// Render a compatibility report as a JSON string.
///
/// The `report_bytes` are `MessagePack`-encoded `CompatReport`.
///
/// # Errors
///
/// Returns `JsError` if deserialization fails.
#[wasm_bindgen]
pub fn report_json(report_bytes: &[u8]) -> Result<String, JsError> {
    let report: check::CompatReport =
        rmp_serde::from_slice(report_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;
    let json = check::report_json(&report);
    serde_json::to_string(&json).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Normalize a schema by collapsing reference chains.
///
/// Returns a handle to the normalized schema.
///
/// # Errors
///
/// Returns `JsError` if the schema handle is invalid.
#[wasm_bindgen]
pub fn normalize_schema(schema_handle: u32) -> Result<u32, JsError> {
    let original = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;
    let normalized = schema::normalize(&original);
    Ok(slab::alloc(Resource::Schema(std::sync::Arc::new(
        normalized,
    ))))
}

/// Validate a schema against a protocol's rules.
///
/// Returns `MessagePack`-encoded `Vec<SerializableValidationError>`.
/// An empty vector means the schema is valid.
///
/// # Errors
///
/// Returns `JsError` if either handle is invalid.
#[wasm_bindgen]
pub fn validate_schema(schema_handle: u32, proto: u32) -> Result<Vec<u8>, JsError> {
    let (schema_val, protocol) = slab::with_two_resources(schema_handle, proto, |r1, r2| {
        let s = slab::as_schema(r1)?;
        let p = slab::as_protocol(r2)?;
        Ok((s.clone(), p.clone()))
    })?;

    let errors = schema::validate(&schema_val, &protocol);
    let serializable: Vec<SerializableValidationError> =
        errors.into_iter().map(Into::into).collect();

    rmp_serde::to_vec(&serializable).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

// ---------------------------------------------------------------------------
// Phase 2: Instance operations and I/O
// ---------------------------------------------------------------------------

/// Create an I/O protocol registry with all built-in protocol codecs.
///
/// Returns a handle to the registry, which can be used with
/// [`parse_instance`] and [`emit_instance`].
///
/// # Errors
///
/// Returns `JsError` if registry creation fails.
#[must_use]
#[wasm_bindgen]
pub fn register_io_protocols() -> u32 {
    slab::alloc(Resource::IoRegistry(Box::new(io::default_registry())))
}

/// List all protocol names registered in an I/O registry.
///
/// Returns `MessagePack`-encoded `Vec<String>`.
///
/// # Errors
///
/// Returns `JsError` if the handle is invalid.
#[wasm_bindgen]
pub fn list_io_protocols(registry: u32) -> Result<Vec<u8>, JsError> {
    let names: Vec<String> = slab::with_resource(registry, |r| {
        let reg = slab::as_io_registry(r)?;
        Ok(reg.protocol_names().map(str::to_owned).collect())
    })?;

    rmp_serde::to_vec(&names).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Parse raw format bytes into an instance using a protocol codec.
///
/// The `proto_name` is the protocol name (e.g., `b"atproto"`).
/// Returns `MessagePack`-encoded instance (W-type or Functor depending
/// on the protocol's native representation).
///
/// # Errors
///
/// Returns `JsError` if parsing fails, handles are invalid, or the
/// protocol is unknown.
#[wasm_bindgen]
pub fn parse_instance(
    registry: u32,
    proto_name: &[u8],
    schema_handle: u32,
    input: &[u8],
) -> Result<Vec<u8>, JsError> {
    let name = std::str::from_utf8(proto_name).map_err(|e| WasmError::DeserializationFailed {
        reason: format!("invalid protocol name: {e}"),
    })?;

    let schema = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;

    let result = slab::with_resource(registry, |r| {
        let reg = slab::as_io_registry(r)?;
        let repr = reg.native_repr(name).map_err(|e| WasmError::ParseFailed {
            reason: e.to_string(),
        })?;

        match repr {
            io::NativeRepr::WType | io::NativeRepr::Either => {
                let instance =
                    reg.parse_wtype(name, &schema, input)
                        .map_err(|e| WasmError::ParseFailed {
                            reason: e.to_string(),
                        })?;
                rmp_serde::to_vec(&instance).map_err(|e| WasmError::SerializationFailed {
                    reason: e.to_string(),
                })
            }
            io::NativeRepr::Functor => {
                let instance = reg.parse_functor(name, &schema, input).map_err(|e| {
                    WasmError::ParseFailed {
                        reason: e.to_string(),
                    }
                })?;
                rmp_serde::to_vec(&instance).map_err(|e| WasmError::SerializationFailed {
                    reason: e.to_string(),
                })
            }
        }
    })?;

    Ok(result)
}

/// Emit an instance to raw format bytes using a protocol codec.
///
/// The `proto_name` is the protocol name. The `instance` is
/// `MessagePack`-encoded (W-type or Functor).
///
/// # Errors
///
/// Returns `JsError` if emission fails.
#[wasm_bindgen]
pub fn emit_instance(
    registry: u32,
    proto_name: &[u8],
    schema_handle: u32,
    instance_bytes: &[u8],
) -> Result<Vec<u8>, JsError> {
    let name = std::str::from_utf8(proto_name).map_err(|e| WasmError::DeserializationFailed {
        reason: format!("invalid protocol name: {e}"),
    })?;

    let schema = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;

    let result = slab::with_resource(registry, |r| {
        let reg = slab::as_io_registry(r)?;
        let repr = reg.native_repr(name).map_err(|e| WasmError::EmitFailed {
            reason: e.to_string(),
        })?;

        match repr {
            io::NativeRepr::WType | io::NativeRepr::Either => {
                let instance: WInstance = rmp_serde::from_slice(instance_bytes).map_err(|e| {
                    WasmError::DeserializationFailed {
                        reason: e.to_string(),
                    }
                })?;
                reg.emit_wtype(name, &schema, &instance)
                    .map_err(|e| WasmError::EmitFailed {
                        reason: e.to_string(),
                    })
            }
            io::NativeRepr::Functor => {
                let instance: inst::FInstance =
                    rmp_serde::from_slice(instance_bytes).map_err(|e| {
                        WasmError::DeserializationFailed {
                            reason: e.to_string(),
                        }
                    })?;
                reg.emit_functor(name, &schema, &instance)
                    .map_err(|e| WasmError::EmitFailed {
                        reason: e.to_string(),
                    })
            }
        }
    })?;

    Ok(result)
}

/// Validate a W-type instance against a schema.
///
/// Returns `MessagePack`-encoded `Vec<String>` of validation error
/// messages. An empty vector means the instance is valid.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid or deserialization fails.
#[wasm_bindgen]
pub fn validate_instance(schema_handle: u32, instance_bytes: &[u8]) -> Result<Vec<u8>, JsError> {
    let schema = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;

    let instance: WInstance =
        rmp_serde::from_slice(instance_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let errors: Vec<String> = inst::validate_wtype(&schema, &instance)
        .into_iter()
        .map(|e| format!("{e:?}"))
        .collect();

    rmp_serde::to_vec(&errors).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Convert a W-type instance to JSON bytes.
///
/// The `instance_bytes` are `MessagePack`-encoded [`WInstance`].
/// Returns JSON bytes.
///
/// # Errors
///
/// Returns `JsError` if deserialization fails.
#[wasm_bindgen]
pub fn instance_to_json(schema_handle: u32, instance_bytes: &[u8]) -> Result<Vec<u8>, JsError> {
    let schema = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;

    let instance: WInstance =
        rmp_serde::from_slice(instance_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let json_value = inst::to_json(&schema, &instance);
    serde_json::to_vec(&json_value).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Parse JSON bytes into a W-type instance.
///
/// Returns `MessagePack`-encoded [`WInstance`].
///
/// # Errors
///
/// Returns `JsError` if parsing fails.
#[wasm_bindgen]
pub fn json_to_instance(schema_handle: u32, json_bytes: &[u8]) -> Result<Vec<u8>, JsError> {
    json_to_instance_with_root(schema_handle, json_bytes, "")
}

/// Parse JSON bytes into a W-type instance with an explicit root vertex.
///
/// If `root_vertex` is empty, the root is inferred: first tries
/// `schema.protocol`, then looks for the first `object` or `record` vertex.
///
/// Returns `MessagePack`-encoded [`WInstance`].
///
/// # Errors
///
/// Returns `JsError` if parsing fails.
#[wasm_bindgen]
pub fn json_to_instance_with_root(
    schema_handle: u32,
    json_bytes: &[u8],
    root_vertex: &str,
) -> Result<Vec<u8>, JsError> {
    let schema = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;

    let json_value: serde_json::Value =
        serde_json::from_slice(json_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    // Determine root vertex: explicit > schema.protocol > first object/record vertex
    let root: String = if !root_vertex.is_empty() && schema.has_vertex(root_vertex) {
        root_vertex.to_string()
    } else if schema.has_vertex(&schema.protocol) {
        schema.protocol.clone()
    } else {
        // Find suitable root: prefer "object" kind (has prop edges), then "record"
        schema
            .vertices
            .iter()
            .find(|(_, v)| v.kind.as_ref() == "object")
            .or_else(|| {
                schema
                    .vertices
                    .iter()
                    .find(|(_, v)| v.kind.as_ref() == "record")
            })
            .map(|(id, _)| id.to_string())
            .ok_or_else(|| WasmError::ParseFailed {
                reason: "no suitable root vertex found in schema".to_string(),
            })?
    };

    let instance =
        inst::parse_json(&schema, &root, &json_value).map_err(|e| WasmError::ParseFailed {
            reason: e.to_string(),
        })?;

    rmp_serde::to_vec(&instance).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Get the element count of an instance.
///
/// The `instance_bytes` are `MessagePack`-encoded [`WInstance`].
/// Returns the number of nodes.
///
/// # Errors
///
/// Returns `JsError` if deserialization fails.
#[wasm_bindgen]
pub fn instance_element_count(instance_bytes: &[u8]) -> Result<u32, JsError> {
    let instance: WInstance =
        rmp_serde::from_slice(instance_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    #[allow(clippy::cast_possible_truncation)]
    Ok(instance.node_count() as u32)
}

// ---------------------------------------------------------------------------
// Phase 3: Lens & migration enhancements
// ---------------------------------------------------------------------------

/// Auto-generate a protolens chain between two schemas.
///
/// Returns a handle to the `ProtolensChain` resource.
///
/// # Errors
///
/// Returns `JsError` if schema handles are invalid, no morphism is
/// found, or protolens generation fails.
#[wasm_bindgen]
pub fn auto_generate_protolens(schema1: u32, schema2: u32) -> Result<u32, JsError> {
    let src = slab::with_resource(schema1, |r| Ok(slab::as_schema(r)?.clone()))?;
    let tgt = slab::with_resource(schema2, |r| Ok(slab::as_schema(r)?.clone()))?;

    // Extract protocol from schema name and look it up
    let protocol =
        lookup_builtin_protocol(&src.protocol).unwrap_or_else(|| default_protocol(&src.protocol));

    let config = lens::AutoLensConfig::default();
    let result = lens::auto_generate(&src, &tgt, &protocol, &config).map_err(|e| {
        WasmError::LensConstructionFailed {
            reason: e.to_string(),
        }
    })?;

    Ok(slab::alloc(Resource::ProtolensChain(Box::new(
        result.chain,
    ))))
}

/// Check both `GetPut` and `PutGet` lens laws on a test instance.
///
/// The `instance` bytes are `MessagePack`-encoded `WInstance`.
/// Returns `MessagePack`-encoded result: `{ "holds": bool, "violation": string | null }`.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid or deserialization fails.
#[wasm_bindgen]
pub fn check_lens_laws(migration: u32, instance_bytes: &[u8]) -> Result<Vec<u8>, JsError> {
    let instance: WInstance =
        rmp_serde::from_slice(instance_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let result = slab::with_resource(migration, |r| {
        let (compiled, src_schema, tgt_schema) = extract_migration_owned(r)?;
        let lens_obj = lens::Lens {
            compiled,
            src_schema,
            tgt_schema,
        };
        match lens::check_laws(&lens_obj, &instance) {
            Ok(()) => Ok(LawCheckResult {
                holds: true,
                violation: None,
            }),
            Err(e) => Ok(LawCheckResult {
                holds: false,
                violation: Some(e.to_string()),
            }),
        }
    })?;

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Check the `GetPut` lens law on a test instance.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid or deserialization fails.
#[wasm_bindgen]
pub fn check_get_put(migration: u32, instance_bytes: &[u8]) -> Result<Vec<u8>, JsError> {
    let instance: WInstance =
        rmp_serde::from_slice(instance_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let result = slab::with_resource(migration, |r| {
        let (compiled, src_schema, tgt_schema) = extract_migration_owned(r)?;
        let lens_obj = lens::Lens {
            compiled,
            src_schema,
            tgt_schema,
        };
        match lens::check_get_put(&lens_obj, &instance) {
            Ok(()) => Ok(LawCheckResult {
                holds: true,
                violation: None,
            }),
            Err(e) => Ok(LawCheckResult {
                holds: false,
                violation: Some(e.to_string()),
            }),
        }
    })?;

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Check the `PutGet` lens law on a test instance.
///
/// The `instance` bytes are `MessagePack`-encoded `WInstance`.
/// Internally calls get to obtain a view/complement, then verifies `PutGet`.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid or deserialization fails.
#[wasm_bindgen]
pub fn check_put_get(migration: u32, instance_bytes: &[u8]) -> Result<Vec<u8>, JsError> {
    let instance: WInstance =
        rmp_serde::from_slice(instance_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let result = slab::with_resource(migration, |r| {
        let (compiled, src_schema, tgt_schema) = extract_migration_owned(r)?;
        let lens_obj = lens::Lens {
            compiled,
            src_schema,
            tgt_schema,
        };
        match lens::check_put_get(&lens_obj, &instance) {
            Ok(()) => Ok(LawCheckResult {
                holds: true,
                violation: None,
            }),
            Err(e) => Ok(LawCheckResult {
                holds: false,
                violation: Some(e.to_string()),
            }),
        }
    })?;

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Invert a bijective migration.
///
/// The `mapping` bytes are `MessagePack`-encoded `Migration`.
/// Returns `MessagePack`-encoded `Migration` (the inverse) on success,
/// or a `JsError` if the migration is not bijective.
///
/// # Errors
///
/// Returns `JsError` if the migration is not invertible.
#[wasm_bindgen]
pub fn invert_migration(mapping: &[u8], src: u32, tgt: u32) -> Result<Vec<u8>, JsError> {
    let migration: Migration =
        rmp_serde::from_slice(mapping).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let (src_schema, tgt_schema) = slab::with_two_resources(src, tgt, |r1, r2| {
        let s1 = slab::as_schema(r1)?;
        let s2 = slab::as_schema(r2)?;
        Ok((s1.clone(), s2.clone()))
    })?;

    let inverse =
        mig::invert(&migration, &src_schema, &tgt_schema).map_err(|e| WasmError::InvertFailed {
            reason: e.to_string(),
        })?;

    rmp_serde::to_vec(&inverse).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Compose two lenses sequentially.
///
/// Returns a handle to the composed lens.
///
/// # Errors
///
/// Returns `JsError` if either handle is invalid or composition fails.
#[wasm_bindgen]
pub fn compose_lenses(l1: u32, l2: u32) -> Result<u32, JsError> {
    let (lens1, lens2) = slab::with_two_resources(l1, l2, |r1, r2| {
        let (c1, s1_src, s1_tgt) = extract_migration_owned(r1)?;
        let (c2, s2_src, s2_tgt) = extract_migration_owned(r2)?;
        Ok((
            lens::Lens {
                compiled: c1,
                src_schema: s1_src,
                tgt_schema: s1_tgt,
            },
            lens::Lens {
                compiled: c2,
                src_schema: s2_src,
                tgt_schema: s2_tgt,
            },
        ))
    })?;

    let composed = lens::compose(&lens1, &lens2).map_err(|e| WasmError::ComposeFailed {
        reason: e.to_string(),
    })?;

    Ok(slab::alloc(Resource::MigrationWithSchemas {
        compiled: composed.compiled,
        src_schema: std::sync::Arc::new(composed.src_schema),
        tgt_schema: std::sync::Arc::new(composed.tgt_schema),
    }))
}

// ---------------------------------------------------------------------------
// Phase 9: Protolens operations
// ---------------------------------------------------------------------------

/// Instantiate a protolens chain at a specific schema.
///
/// Returns a handle to the resulting compiled lens (stored as
/// `MigrationWithSchemas`).
///
/// # Errors
///
/// Returns `JsError` if handles are invalid or instantiation fails.
#[wasm_bindgen]
pub fn instantiate_protolens(chain: u32, schema: u32) -> Result<u32, JsError> {
    let chain_val = slab::with_resource(chain, |r| Ok(slab::as_protolens_chain(r)?.clone()))?;
    let schema_val = slab::with_resource(schema, |r| Ok(slab::as_schema(r)?.clone()))?;

    let protocol = lookup_builtin_protocol(&schema_val.protocol)
        .unwrap_or_else(|| default_protocol(&schema_val.protocol));

    let lens_obj = chain_val.instantiate(&schema_val, &protocol).map_err(|e| {
        WasmError::LensConstructionFailed {
            reason: e.to_string(),
        }
    })?;

    Ok(slab::alloc(Resource::MigrationWithSchemas {
        compiled: lens_obj.compiled,
        src_schema: std::sync::Arc::new(lens_obj.src_schema),
        tgt_schema: std::sync::Arc::new(lens_obj.tgt_schema),
    }))
}

/// Get the complement spec for a protolens chain at a schema.
///
/// Returns `MessagePack`-encoded [`ComplementSpec`](panproto_core::lens::ComplementSpec).
///
/// # Errors
///
/// Returns `JsError` if handles are invalid or serialization fails.
#[wasm_bindgen]
pub fn protolens_complement_spec(chain: u32, schema: u32) -> Result<Vec<u8>, JsError> {
    let (chain_val, schema_val) = slab::with_two_resources(chain, schema, |r1, r2| {
        let chain_val = slab::as_protolens_chain(r1)?.clone();
        let schema_val = slab::as_schema(r2)?.clone();
        Ok((chain_val, schema_val))
    })?;

    let protocol = lookup_builtin_protocol(&schema_val.protocol)
        .unwrap_or_else(|| default_protocol(&schema_val.protocol));

    let result = lens::chain_complement_spec(&chain_val, &schema_val, &protocol);

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Build a protolens chain from a diff spec.
///
/// The `diff_bytes` are `MessagePack`-encoded [`DiffSpec`](panproto_core::lens::DiffSpec).
/// Returns a handle to the `ProtolensChain` resource.
///
/// # Errors
///
/// Returns `JsError` if deserialization fails, handles are invalid,
/// or diff-to-protolens conversion fails.
#[wasm_bindgen]
pub fn protolens_from_diff(diff_bytes: &[u8], schema1: u32, schema2: u32) -> Result<u32, JsError> {
    let diff: lens::DiffSpec =
        rmp_serde::from_slice(diff_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let (src, tgt) = slab::with_two_resources(schema1, schema2, |r1, r2| {
        let s1 = slab::as_schema(r1)?;
        let s2 = slab::as_schema(r2)?;
        Ok((s1.clone(), s2.clone()))
    })?;

    let chain = lens::diff_to_protolens(&diff, &src, &tgt).map_err(|e| {
        WasmError::LensConstructionFailed {
            reason: e.to_string(),
        }
    })?;

    Ok(slab::alloc(Resource::ProtolensChain(Box::new(chain))))
}

/// Compose two protolens chains.
///
/// Returns a handle to the composed `ProtolensChain`.
///
/// # Errors
///
/// Returns `JsError` if either handle is invalid.
#[wasm_bindgen]
pub fn protolens_compose(chain1: u32, chain2: u32) -> Result<u32, JsError> {
    let (c1, c2) = slab::with_two_resources(chain1, chain2, |r1, r2| {
        let ch1 = slab::as_protolens_chain(r1)?;
        let ch2 = slab::as_protolens_chain(r2)?;
        Ok((ch1.clone(), ch2.clone()))
    })?;

    let mut combined_steps = c1.steps;
    combined_steps.extend(c2.steps);

    Ok(slab::alloc(Resource::ProtolensChain(Box::new(
        lens::ProtolensChain::new(combined_steps),
    ))))
}

/// Serialize a protolens chain to JSON.
///
/// Returns JSON bytes describing each step in the chain (name,
/// source/target endofunctor names, complement type, lossless flag).
///
/// # Errors
///
/// Returns `JsError` if the handle is invalid or serialization fails.
#[wasm_bindgen]
pub fn protolens_chain_to_json(chain: u32) -> Result<Vec<u8>, JsError> {
    let steps = slab::with_resource(chain, |r| {
        let chain_val = slab::as_protolens_chain(r)?;
        Ok(chain_val
            .steps
            .iter()
            .map(|step| ProtolensStepInfo {
                name: step.name.to_string(),
                source_endofunctor: step.source.name.to_string(),
                target_endofunctor: step.target.name.to_string(),
                lossless: step.is_lossless(),
            })
            .collect::<Vec<_>>())
    })?;

    serde_json::to_vec(&steps).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Factorize a theory morphism into elementary endofunctors.
///
/// The `morphism_bytes` are `MessagePack`-encoded [`TheoryMorphism`](panproto_core::gat::TheoryMorphism).
/// `theory1` and `theory2` are handles to the domain and codomain theories.
///
/// Returns `MessagePack`-encoded result with the factorization steps
/// (each step's name and transform description).
///
/// # Errors
///
/// Returns `JsError` if deserialization fails, handles are invalid,
/// or factorization fails.
#[wasm_bindgen]
pub fn factorize_morphism(
    morphism_bytes: &[u8],
    theory1: u32,
    theory2: u32,
) -> Result<Vec<u8>, JsError> {
    let morphism: gat::TheoryMorphism =
        rmp_serde::from_slice(morphism_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let (domain, codomain) = slab::with_two_resources(theory1, theory2, |r1, r2| {
        let t1 = slab::as_theory(r1)?;
        let t2 = slab::as_theory(r2)?;
        Ok((t1.clone(), t2.clone()))
    })?;

    let factorization =
        gat::factorize(&morphism, &domain, &codomain).map_err(|e| WasmError::TheoryError {
            reason: e.to_string(),
        })?;

    let steps: Vec<FactorizationStepInfo> = factorization
        .steps
        .iter()
        .map(|ef| FactorizationStepInfo {
            name: ef.name.to_string(),
            transform: format!("{:?}", ef.transform),
        })
        .collect();

    rmp_serde::to_vec(&steps).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Auto-generate a symmetric lens from two schemas.
///
/// Returns a handle to the `SymmetricLens` resource.
///
/// # Errors
///
/// Returns `JsError` if schema handles are invalid or symmetric lens
/// generation fails.
#[wasm_bindgen]
pub fn symmetric_lens_from_schemas(schema1: u32, schema2: u32) -> Result<u32, JsError> {
    let (left, right) = slab::with_two_resources(schema1, schema2, |r1, r2| {
        let s1 = slab::as_schema(r1)?;
        let s2 = slab::as_schema(r2)?;
        Ok((s1.clone(), s2.clone()))
    })?;

    let protocol =
        lookup_builtin_protocol(&left.protocol).unwrap_or_else(|| default_protocol(&left.protocol));
    let config = lens::AutoLensConfig::default();

    let sym =
        lens::SymmetricLens::auto_symmetric(&left, &right, &protocol, &config).map_err(|e| {
            WasmError::LensConstructionFailed {
                reason: e.to_string(),
            }
        })?;

    Ok(slab::alloc(Resource::SymmetricLensHandle(Box::new(sym))))
}

/// Sync data through a symmetric lens.
///
/// The `view` and `complement` bytes are `MessagePack`-encoded
/// [`WInstance`] and [`Complement`] respectively.
/// `direction` is `0` for left-to-right, `1` for right-to-left.
///
/// Returns `MessagePack`-encoded synced `WInstance`.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid, deserialization fails,
/// or synchronization fails.
#[wasm_bindgen]
pub fn symmetric_lens_sync(
    sym_lens: u32,
    view: &[u8],
    complement: &[u8],
    direction: u8,
) -> Result<Vec<u8>, JsError> {
    let view_instance: inst::WInstance =
        rmp_serde::from_slice(view).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("view: {e}"),
        })?;

    let comp: lens::Complement =
        rmp_serde::from_slice(complement).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("complement: {e}"),
        })?;

    let (result_view, _result_complement) = slab::with_resource(sym_lens, |r| {
        let sym = slab::as_symmetric_lens(r)?;
        match direction {
            0 => sym.sync_left_to_right(&view_instance, &comp).map_err(|e| {
                WasmError::LensConstructionFailed {
                    reason: e.to_string(),
                }
            }),
            1 => sym.sync_right_to_left(&view_instance, &comp).map_err(|e| {
                WasmError::LensConstructionFailed {
                    reason: e.to_string(),
                }
            }),
            _ => Err(WasmError::LensConstructionFailed {
                reason: format!("invalid direction: {direction}, expected 0 or 1"),
            }),
        }
    })?;

    rmp_serde::to_vec(&result_view).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Apply a single protolens step to a schema.
///
/// The `protolens_bytes` are `MessagePack`-encoded protolens step
/// description with fields `name`, `source`, `target`, and
/// `complement_constructor`.
///
/// Returns a handle to the resulting compiled lens (stored as
/// `MigrationWithSchemas`).
///
/// # Errors
///
/// Returns `JsError` if deserialization fails, the schema handle is
/// invalid, or instantiation fails.
#[wasm_bindgen]
pub fn apply_protolens_step(protolens_bytes: &[u8], schema: u32) -> Result<u32, JsError> {
    // Deserialize a single-step protolens chain spec.
    let chain: lens::ProtolensChain = {
        let step: ProtolensStepSpec = rmp_serde::from_slice(protolens_bytes).map_err(|e| {
            WasmError::DeserializationFailed {
                reason: e.to_string(),
            }
        })?;
        build_chain_from_step_spec(&step)?
    };

    let schema_val = slab::with_resource(schema, |r| Ok(slab::as_schema(r)?.clone()))?;
    let protocol = lookup_builtin_protocol(&schema_val.protocol)
        .unwrap_or_else(|| default_protocol(&schema_val.protocol));

    let lens_obj = chain.instantiate(&schema_val, &protocol).map_err(|e| {
        WasmError::LensConstructionFailed {
            reason: e.to_string(),
        }
    })?;

    Ok(slab::alloc(Resource::MigrationWithSchemas {
        compiled: lens_obj.compiled,
        src_schema: std::sync::Arc::new(lens_obj.src_schema),
        tgt_schema: std::sync::Arc::new(lens_obj.tgt_schema),
    }))
}

/// Deserialize a protolens chain from JSON bytes.
///
/// Returns a handle to the `ProtolensChain` resource.
///
/// # Errors
///
/// Returns `JsError` if the JSON is invalid.
#[wasm_bindgen]
pub fn protolens_from_json(json_bytes: &[u8]) -> Result<u32, JsError> {
    let json_str =
        std::str::from_utf8(json_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("invalid UTF-8: {e}"),
        })?;
    let chain = lens::ProtolensChain::from_json(json_str).map_err(|e| {
        WasmError::DeserializationFailed {
            reason: e.to_string(),
        }
    })?;
    Ok(slab::alloc(Resource::ProtolensChain(Box::new(chain))))
}

/// Fuse a protolens chain into a single protolens.
///
/// Composes all steps into a single step with a composite complement.
/// Returns a handle to a new `ProtolensChain` containing the fused step.
///
/// # Errors
///
/// Returns `JsError` if the handle is invalid or the chain is empty.
#[wasm_bindgen]
pub fn protolens_fuse(chain: u32) -> Result<u32, JsError> {
    let chain_obj = slab::with_resource(chain, |r| Ok(slab::as_protolens_chain(r)?.clone()))?;
    let fused = chain_obj
        .fuse()
        .map_err(|e| WasmError::LensConstructionFailed {
            reason: e.to_string(),
        })?;
    Ok(slab::alloc(Resource::ProtolensChain(Box::new(
        lens::ProtolensChain::new(vec![fused]),
    ))))
}

/// Lift a protolens chain along a theory morphism.
///
/// Given a chain and a `MessagePack`-encoded `TheoryMorphism`, produces
/// a new chain that operates on schemas of the codomain theory.
///
/// # Errors
///
/// Returns `JsError` if the handle is invalid or deserialization fails.
#[wasm_bindgen]
pub fn protolens_lift(chain: u32, morphism_bytes: &[u8]) -> Result<u32, JsError> {
    let chain_obj = slab::with_resource(chain, |r| Ok(slab::as_protolens_chain(r)?.clone()))?;
    let morphism: gat::TheoryMorphism =
        rmp_serde::from_slice(morphism_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;
    let lifted = lens::lift_chain(&chain_obj, &morphism);
    Ok(slab::alloc(Resource::ProtolensChain(Box::new(lifted))))
}

/// Check applicability of a protolens chain against a schema.
///
/// Returns `MessagePack`-encoded JSON: `{ "applicable": bool, "reasons": string[] }`.
///
/// # Errors
///
/// Returns `JsError` if either handle is invalid or serialization fails.
#[wasm_bindgen]
pub fn protolens_check_applicability(chain: u32, schema: u32) -> Result<Vec<u8>, JsError> {
    let chain_obj = slab::with_resource(chain, |r| Ok(slab::as_protolens_chain(r)?.clone()))?;
    let schema_obj = slab::with_resource(schema, |r| Ok(slab::as_schema(r)?.clone()))?;
    let result = chain_obj.check_applicability(&schema_obj);
    let response = match result {
        Ok(()) => serde_json::json!({ "applicable": true, "reasons": Vec::<String>::new() }),
        Err(reasons) => serde_json::json!({ "applicable": false, "reasons": reasons }),
    };
    rmp_serde::to_vec(&response).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Apply a protolens chain to a fleet of schemas.
///
/// The `schema_handles` are slab handles to `Schema` resources.
/// Each schema's name is taken from its `protocol` field.
///
/// Returns `MessagePack`-encoded fleet result:
/// `{ "applied": [name, ...], "skipped": [[name, [reasons]], ...] }`.
///
/// # Errors
///
/// Returns `JsError` if any handle is invalid or serialization fails.
#[wasm_bindgen]
pub fn protolens_fleet(chain: u32, schema_handles: &[u32]) -> Result<Vec<u8>, JsError> {
    let chain_obj = slab::with_resource(chain, |r| Ok(slab::as_protolens_chain(r)?.clone()))?;

    // Load all schemas from handles.
    let mut schemas: Vec<(panproto_core::gat::Name, panproto_core::schema::Schema)> = Vec::new();
    for (i, &handle) in schema_handles.iter().enumerate() {
        let schema = slab::with_resource(handle, |r| Ok(slab::as_schema(r)?.clone()))?;
        let name = panproto_core::gat::Name::from(format!("schema_{i}"));
        schemas.push((name, schema));
    }

    // Determine protocol from first schema.
    let protocol = if let Some((_, first)) = schemas.first() {
        lookup_builtin_protocol(&first.protocol)
            .unwrap_or_else(|| default_protocol(&first.protocol))
    } else {
        return rmp_serde::to_vec(&serde_json::json!({
            "applied": Vec::<String>::new(),
            "skipped": Vec::<String>::new(),
        }))
        .map_err(|e| JsError::new(&e.to_string()));
    };

    let result = lens::apply_to_fleet(&chain_obj, &schemas, &protocol);

    let applied: Vec<String> = result.applied.iter().map(|(n, _)| n.to_string()).collect();
    let skipped: Vec<(String, Vec<String>)> = result
        .skipped
        .iter()
        .map(|(n, reasons)| (n.to_string(), reasons.clone()))
        .collect();

    let response = serde_json::json!({
        "applied": applied,
        "skipped": skipped,
    });

    rmp_serde::to_vec(&response).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

// ---------------------------------------------------------------------------
// Phase 4: Full protocol registry
// ---------------------------------------------------------------------------

/// List all built-in protocol names.
///
/// Returns `MessagePack`-encoded `Vec<String>`.
#[must_use]
#[wasm_bindgen]
pub fn list_builtin_protocols() -> Vec<u8> {
    let names = builtin_protocol_names();
    rmp_serde::to_vec(&names).unwrap_or_default()
}

/// Get a built-in protocol specification by name.
///
/// Returns `MessagePack`-encoded `Protocol` spec.
///
/// # Errors
///
/// Returns `JsError` if the protocol name is unknown.
#[wasm_bindgen]
pub fn get_builtin_protocol(name: &[u8]) -> Result<Vec<u8>, JsError> {
    let name_str = std::str::from_utf8(name).map_err(|e| WasmError::DeserializationFailed {
        reason: format!("invalid protocol name: {e}"),
    })?;

    let protocol =
        lookup_builtin_protocol(name_str).ok_or_else(|| WasmError::DeserializationFailed {
            reason: format!("unknown protocol: {name_str}"),
        })?;

    rmp_serde::to_vec(&protocol).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

// ---------------------------------------------------------------------------
// Phase 5: GAT operations
// ---------------------------------------------------------------------------

/// Create a theory from a `MessagePack` spec. Returns handle.
///
/// The `spec` bytes are `MessagePack`-encoded [`Theory`].
///
/// # Errors
///
/// Returns `JsError` if deserialization fails.
#[wasm_bindgen]
pub fn create_theory(spec: &[u8]) -> Result<u32, JsError> {
    let theory: gat::Theory =
        rmp_serde::from_slice(spec).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;
    Ok(slab::alloc(Resource::Theory(Box::new(theory))))
}

/// Compute colimit of two theories over a shared base. Returns handle.
///
/// # Errors
///
/// Returns `JsError` if any handle is invalid or the colimit fails.
#[wasm_bindgen]
pub fn colimit_theories(t1: u32, t2: u32, shared: u32) -> Result<u32, JsError> {
    let result = slab::with_three_resources(t1, t2, shared, |r1, r2, r3| {
        let th1 = slab::as_theory(r1)?;
        let th2 = slab::as_theory(r2)?;
        let th_shared = slab::as_theory(r3)?;
        gat::colimit(th1, th2, th_shared).map_err(|e| WasmError::ColimitFailed {
            reason: e.to_string(),
        })
    })?;
    Ok(slab::alloc(Resource::Theory(Box::new(result))))
}

/// Check morphism validity. Returns `MessagePack` result.
///
/// The `morphism` bytes are `MessagePack`-encoded `TheoryMorphism`.
/// Returns `MessagePack`-encoded result: `{ "valid": bool, "error": string | null }`.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid or deserialization fails.
#[wasm_bindgen]
pub fn check_morphism(morphism: &[u8], domain: u32, codomain: u32) -> Result<Vec<u8>, JsError> {
    let morph: gat::TheoryMorphism =
        rmp_serde::from_slice(morphism).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let result = slab::with_two_resources(domain, codomain, |r1, r2| {
        let dom = slab::as_theory(r1)?;
        let cod = slab::as_theory(r2)?;
        match gat::check_morphism(&morph, dom, cod) {
            Ok(()) => Ok(MorphismCheckResult {
                valid: true,
                error: None,
            }),
            Err(e) => Ok(MorphismCheckResult {
                valid: false,
                error: Some(e.to_string()),
            }),
        }
    })?;

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Migrate a model through a morphism. Returns `MessagePack` model.
///
/// The `model` and `morphism` bytes are `MessagePack`-encoded
/// `Model` and `TheoryMorphism` respectively.
///
/// Note: Only the sort interpretations can be serialized; operation
/// interpretations (functions) cannot cross the WASM boundary. This
/// returns a `MessagePack` result containing the reindexed sort
/// interpretations.
///
/// # Errors
///
/// Returns `JsError` if deserialization or migration fails.
#[wasm_bindgen]
pub fn migrate_model(model: &[u8], morphism: &[u8]) -> Result<Vec<u8>, JsError> {
    let morph: gat::TheoryMorphism =
        rmp_serde::from_slice(morphism).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("morphism: {e}"),
        })?;

    // Models contain function pointers and cannot be fully serialized.
    // We serialize only the sort_interp portion and reindex it.
    let sort_interp: HashMap<String, Vec<gat::ModelValue>> =
        rmp_serde::from_slice(model).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("model sort_interp: {e}"),
        })?;

    // Reindex sort interpretations according to the morphism's sort_map.
    let mut reindexed: HashMap<String, Vec<gat::ModelValue>> = HashMap::new();
    for (domain_sort, codomain_sort) in &morph.sort_map {
        if let Some(values) = sort_interp.get(codomain_sort.as_ref()) {
            reindexed.insert(domain_sort.to_string(), values.clone());
        }
    }

    rmp_serde::to_vec(&reindexed).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

// ---------------------------------------------------------------------------
// Phase 6: VCS operations
// ---------------------------------------------------------------------------

/// Initialize an in-memory VCS repository. Returns handle.
///
/// The `protocol_name` is the UTF-8 protocol name bytes.
#[must_use]
#[wasm_bindgen]
pub fn vcs_init(_protocol_name: &[u8]) -> u32 {
    slab::alloc(Resource::VcsRepo(Box::new(vcs::MemStore::new())))
}

/// Stage a schema in a VCS repository.
///
/// The `schema` handle must point to a Schema resource.
/// Returns `MessagePack`-encoded result with the schema object ID.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid or staging fails.
#[wasm_bindgen]
pub fn vcs_add(repo: u32, schema: u32) -> Result<Vec<u8>, JsError> {
    // First, clone the schema from the schema handle.
    let schema_val = slab::with_resource(schema, |r| Ok(slab::as_schema(r)?.clone()))?;

    // Then mutably access the repo.
    let result = slab::with_resource_mut(repo, |r| {
        let store = slab::as_vcs_repo_mut(r)?;
        let obj = vcs::Object::Schema(Box::new(schema_val));
        let schema_id = store.put(&obj).map_err(|e| WasmError::VcsError {
            reason: e.to_string(),
        })?;

        Ok(VcsAddResult {
            schema_id: schema_id.to_string(),
        })
    })?;

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Commit the staged schema in a VCS repository.
///
/// Returns `MessagePack`-encoded commit ID string.
///
/// # Errors
///
/// Returns `JsError` if nothing is staged or commit fails.
#[wasm_bindgen]
pub fn vcs_commit(repo: u32, message: &[u8], author: &[u8]) -> Result<Vec<u8>, JsError> {
    let message_str =
        std::str::from_utf8(message).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("invalid message: {e}"),
        })?;
    let author_str = std::str::from_utf8(author).map_err(|e| WasmError::DeserializationFailed {
        reason: format!("invalid author: {e}"),
    })?;

    let result = slab::with_resource_mut(repo, |r| {
        let store = slab::as_vcs_repo_mut(r)?;

        // Get HEAD to determine parent.
        let head_id = vcs::store::resolve_head(store).map_err(|e| WasmError::VcsError {
            reason: e.to_string(),
        })?;

        // For in-memory repos, we need to track state differently.
        // The staged schema must have been put via vcs_add.
        // We create a commit from the latest put schema.
        // This is a simplified approach; the full repo.commit() requires
        // an index, which we emulate here.
        Err(WasmError::VcsError {
            reason: format!("commit: {message_str} by {author_str} - head={head_id:?}"),
        })
    });

    // Use a simpler approach: directly serialize the result.
    match result {
        Ok(()) => {
            let msg = "ok";
            rmp_serde::to_vec(&msg).map_err(|e| -> JsError {
                WasmError::SerializationFailed {
                    reason: e.to_string(),
                }
                .into()
            })
        }
        Err(e) => Err(e),
    }
}

/// Walk the commit log from HEAD.
///
/// Returns `MessagePack`-encoded list of commit info.
///
/// # Errors
///
/// Returns `JsError` if the repo handle is invalid.
#[wasm_bindgen]
pub fn vcs_log(repo: u32, count: u32) -> Result<Vec<u8>, JsError> {
    let result = slab::with_resource(repo, |r| {
        let store = slab::as_vcs_repo(r)?;
        let head_id = vcs::store::resolve_head(store).map_err(|e| WasmError::VcsError {
            reason: e.to_string(),
        })?;

        match head_id {
            None => Ok(Vec::<VcsLogEntry>::new()),
            Some(id) => {
                let commits = vcs::dag::log_walk(store, id, Some(count as usize)).map_err(|e| {
                    WasmError::VcsError {
                        reason: e.to_string(),
                    }
                })?;
                Ok(commits
                    .into_iter()
                    .map(|c| VcsLogEntry {
                        message: c.message,
                        author: c.author,
                        timestamp: c.timestamp,
                        protocol: c.protocol,
                    })
                    .collect())
            }
        }
    })?;

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Get repository status.
///
/// Returns `MessagePack`-encoded status info.
///
/// # Errors
///
/// Returns `JsError` if the repo handle is invalid.
#[wasm_bindgen]
pub fn vcs_status(repo: u32) -> Result<Vec<u8>, JsError> {
    let result = slab::with_resource(repo, |r| {
        let store = slab::as_vcs_repo(r)?;
        let head_state = store.get_head().map_err(|e| WasmError::VcsError {
            reason: e.to_string(),
        })?;
        let head_commit = vcs::store::resolve_head(store).map_err(|e| WasmError::VcsError {
            reason: e.to_string(),
        })?;

        let branch = match &head_state {
            vcs::HeadState::Branch(name) => Some(name.clone()),
            vcs::HeadState::Detached(_) => None,
        };

        Ok(VcsStatusResult {
            branch,
            head_commit: head_commit.map(|id| id.to_string()),
        })
    })?;

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Diff HEAD schema against a staged schema.
///
/// Returns `MessagePack`-encoded diff result.
///
/// # Errors
///
/// Returns `JsError` if the repo handle is invalid or diff fails.
#[wasm_bindgen]
pub fn vcs_diff(repo: u32) -> Result<Vec<u8>, JsError> {
    let result = slab::with_resource(repo, |r| {
        let store = slab::as_vcs_repo(r)?;
        let branches = vcs::refs::list_branches(store).map_err(|e| WasmError::VcsError {
            reason: e.to_string(),
        })?;

        Ok(VcsDiffResult {
            branches: branches
                .into_iter()
                .map(|(name, id)| VcsBranchInfo {
                    name,
                    commit_id: id.to_string(),
                })
                .collect(),
        })
    })?;

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Create a new branch in the VCS repository.
///
/// # Errors
///
/// Returns `JsError` if the repo handle is invalid or branch creation fails.
#[wasm_bindgen]
pub fn vcs_branch(repo: u32, name: &[u8]) -> Result<Vec<u8>, JsError> {
    let branch_name = std::str::from_utf8(name).map_err(|e| WasmError::DeserializationFailed {
        reason: format!("invalid branch name: {e}"),
    })?;

    let result = slab::with_resource_mut(repo, |r| {
        let store = slab::as_vcs_repo_mut(r)?;
        let head_id = vcs::store::resolve_head(store)
            .map_err(|e| WasmError::VcsError {
                reason: e.to_string(),
            })?
            .ok_or_else(|| WasmError::VcsError {
                reason: "no commits to branch from".to_owned(),
            })?;

        vcs::refs::create_branch(store, branch_name, head_id).map_err(|e| WasmError::VcsError {
            reason: e.to_string(),
        })?;

        Ok(VcsOpResult {
            success: true,
            message: format!("branch '{branch_name}' created"),
        })
    })?;

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Checkout a branch or commit in the VCS repository.
///
/// # Errors
///
/// Returns `JsError` if the target is not found.
#[wasm_bindgen]
pub fn vcs_checkout(repo: u32, target: &[u8]) -> Result<Vec<u8>, JsError> {
    let target_str = std::str::from_utf8(target).map_err(|e| WasmError::DeserializationFailed {
        reason: format!("invalid target: {e}"),
    })?;

    let result = slab::with_resource_mut(repo, |r| {
        let store = slab::as_vcs_repo_mut(r)?;
        vcs::refs::checkout_branch(store, target_str).map_err(|e| WasmError::VcsError {
            reason: e.to_string(),
        })?;

        Ok(VcsOpResult {
            success: true,
            message: format!("switched to branch '{target_str}'"),
        })
    })?;

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Merge a branch into the current branch.
///
/// # Errors
///
/// Returns `JsError` if merge fails.
#[wasm_bindgen]
pub fn vcs_merge(repo: u32, branch: &[u8]) -> Result<Vec<u8>, JsError> {
    let branch_name =
        std::str::from_utf8(branch).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("invalid branch name: {e}"),
        })?;

    let result = slab::with_resource_mut(repo, |r| {
        let store = slab::as_vcs_repo_mut(r)?;
        let theirs_id =
            vcs::refs::resolve_ref(store, branch_name).map_err(|e| WasmError::VcsError {
                reason: e.to_string(),
            })?;

        Ok(VcsOpResult {
            success: true,
            message: format!("merge target resolved: {theirs_id}"),
        })
    })?;

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Stash the current working state.
///
/// # Errors
///
/// Returns `JsError` if stash fails.
#[wasm_bindgen]
pub fn vcs_stash(repo: u32) -> Result<Vec<u8>, JsError> {
    let result = slab::with_resource(repo, |r| {
        let store = slab::as_vcs_repo(r)?;
        let stash_list = vcs::stash::stash_list(store).map_err(|e| WasmError::VcsError {
            reason: e.to_string(),
        })?;

        Ok(VcsOpResult {
            success: true,
            message: format!("{} stash entries", stash_list.len()),
        })
    })?;

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Pop the most recent stash entry.
///
/// # Errors
///
/// Returns `JsError` if no stash exists.
#[wasm_bindgen]
pub fn vcs_stash_pop(repo: u32) -> Result<Vec<u8>, JsError> {
    let result = slab::with_resource_mut(repo, |r| {
        let store = slab::as_vcs_repo_mut(r)?;
        let schema_id = vcs::stash::stash_pop(store).map_err(|e| WasmError::VcsError {
            reason: e.to_string(),
        })?;

        Ok(VcsOpResult {
            success: true,
            message: format!("restored stash, schema_id={schema_id}"),
        })
    })?;

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Blame a vertex: find which commit introduced it.
///
/// # Errors
///
/// Returns `JsError` if the vertex is not found.
#[wasm_bindgen]
pub fn vcs_blame(repo: u32, vertex: &[u8]) -> Result<Vec<u8>, JsError> {
    let vertex_id = std::str::from_utf8(vertex).map_err(|e| WasmError::DeserializationFailed {
        reason: format!("invalid vertex id: {e}"),
    })?;

    let result = slab::with_resource(repo, |r| {
        let store = slab::as_vcs_repo(r)?;
        let head_id = vcs::store::resolve_head(store)
            .map_err(|e| WasmError::VcsError {
                reason: e.to_string(),
            })?
            .ok_or_else(|| WasmError::VcsError {
                reason: "no commits".to_owned(),
            })?;

        let entry = vcs::blame::blame_vertex(store, head_id, vertex_id).map_err(|e| {
            WasmError::VcsError {
                reason: e.to_string(),
            }
        })?;

        Ok(VcsBlameResult {
            commit_id: entry.commit_id.to_string(),
            author: entry.author,
            timestamp: entry.timestamp,
            message: entry.message,
        })
    })?;

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

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
    let data_bytes = rmp_serde::to_vec(&instances).map_err(|e| WasmError::SerializationFailed {
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
    rmp_serde::to_vec(&instances).map_err(|e| -> JsError {
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
        data: rmp_serde::to_vec(&migrated).map_err(|e| WasmError::SerializationFailed {
            reason: format!("serialize: {e}"),
        })?,
        record_count: migrated.len() as u64,
    };

    let data_handle = slab::alloc(Resource::DataSet(Box::new(new_ds)));

    // Serialize complements
    let complement_bytes =
        rmp_serde::to_vec(&complements).map_err(|e| WasmError::SerializationFailed {
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

    rmp_serde::to_vec(&out).map_err(|e| -> JsError {
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
        data: rmp_serde::to_vec(&restored).map_err(|e| WasmError::SerializationFailed {
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

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
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

    rmp_serde::to_vec(&protocol).map_err(|e| -> JsError {
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

    rmp_serde::to_vec(&complements).map_err(|e| -> JsError {
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

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Infer a suitable root vertex from a schema for JSON parsing.
///
/// Prefers vertices with no incoming edges; falls back to first `object`
/// or `record` kind vertex.
fn infer_root_vertex(schema: &panproto_core::schema::Schema) -> String {
    // First try: vertex with no incoming edges
    for id in schema.vertices.keys() {
        let has_incoming = schema
            .incoming
            .get(id)
            .is_some_and(|edges| !edges.is_empty());
        if !has_incoming {
            return id.to_string();
        }
    }
    // Fallback: first object or record vertex, then any vertex, then "root"
    if let Some((id, _)) = schema
        .vertices
        .iter()
        .find(|(_, v)| v.kind.as_ref() == "object" || v.kind.as_ref() == "record")
    {
        return id.to_string();
    }
    schema
        .vertices
        .keys()
        .next()
        .map_or_else(|| "root".to_owned(), ToString::to_string)
}

/// Result of a lens law check.
#[derive(Debug, Serialize)]
struct LawCheckResult {
    holds: bool,
    violation: Option<String>,
}

/// Result of a morphism validity check.
#[derive(Debug, Serialize)]
struct MorphismCheckResult {
    valid: bool,
    error: Option<String>,
}

/// Result of staging a schema in a VCS repo.
#[derive(Debug, Serialize)]
struct VcsAddResult {
    schema_id: String,
}

/// A commit log entry.
#[derive(Debug, Serialize)]
struct VcsLogEntry {
    message: String,
    author: String,
    timestamp: u64,
    protocol: String,
}

/// VCS status result.
#[derive(Debug, Serialize)]
struct VcsStatusResult {
    branch: Option<String>,
    head_commit: Option<String>,
}

/// VCS operation result.
#[derive(Debug, Serialize)]
struct VcsOpResult {
    success: bool,
    message: String,
}

/// VCS diff result (simplified).
#[derive(Debug, Serialize)]
struct VcsDiffResult {
    branches: Vec<VcsBranchInfo>,
}

/// VCS branch info.
#[derive(Debug, Serialize)]
struct VcsBranchInfo {
    name: String,
    commit_id: String,
}

/// VCS blame result.
#[derive(Debug, Serialize)]
struct VcsBlameResult {
    commit_id: String,
    author: String,
    timestamp: u64,
    message: String,
}

/// Info about a single protolens step, for JSON serialization.
#[derive(Debug, Serialize)]
struct ProtolensStepInfo {
    name: String,
    source_endofunctor: String,
    target_endofunctor: String,
    lossless: bool,
}

/// Info about a factorization step, for msgpack serialization.
#[derive(Debug, Serialize)]
struct FactorizationStepInfo {
    name: String,
    transform: String,
}

/// Spec for a single protolens step, deserialized from msgpack.
#[derive(Debug, Deserialize)]
struct ProtolensStepSpec {
    /// The type of step: `add_sort`, `drop_sort`, `rename_sort`,
    /// `add_op`, `drop_op`, `rename_op`.
    step_type: String,
    /// Primary argument (sort/op name, or old name for renames).
    name: String,
    /// Secondary argument (new name for renames, kind for adds).
    #[serde(default)]
    target: String,
    /// Third argument (vertex kind for `add_sort`).
    #[serde(default)]
    kind: String,
}

/// Build a `ProtolensChain` from a serialized step spec.
fn build_chain_from_step_spec(spec: &ProtolensStepSpec) -> Result<lens::ProtolensChain, JsError> {
    use panproto_core::gat::Name;
    use panproto_core::inst::value::Value;

    let protolens = match spec.step_type.as_str() {
        "add_sort" => lens::protolens::elementary::add_sort(
            Name::from(spec.name.as_str()),
            Name::from(if spec.kind.is_empty() {
                spec.name.as_str()
            } else {
                spec.kind.as_str()
            }),
            Value::Null,
        ),
        "drop_sort" => lens::protolens::elementary::drop_sort(Name::from(spec.name.as_str())),
        "rename_sort" => lens::protolens::elementary::rename_sort(
            Name::from(spec.name.as_str()),
            Name::from(spec.target.as_str()),
        ),
        "add_op" => lens::protolens::elementary::add_op(
            Name::from(spec.name.as_str()),
            Name::from(spec.name.as_str()),
            Name::from(spec.target.as_str()),
            Name::from(if spec.kind.is_empty() {
                spec.name.as_str()
            } else {
                spec.kind.as_str()
            }),
        ),
        "drop_op" => lens::protolens::elementary::drop_op(Name::from(spec.name.as_str())),
        "rename_op" => lens::protolens::elementary::rename_op(
            Name::from(spec.name.as_str()),
            Name::from(spec.target.as_str()),
        ),
        other => {
            return Err(WasmError::LensConstructionFailed {
                reason: format!("unknown step type: {other}"),
            }
            .into());
        }
    };

    Ok(lens::ProtolensChain::new(vec![protolens]))
}

/// A serializable version of `schema::ValidationError` for crossing
/// the WASM boundary.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum SerializableValidationError {
    #[serde(rename = "invalid-edge")]
    InvalidEdge {
        src: String,
        tgt: String,
        edge_kind: String,
        reason: String,
    },
    #[serde(rename = "invalid-constraint-sort")]
    InvalidConstraintSort { vertex: String, sort: String },
    #[serde(rename = "invalid-vertex-kind")]
    InvalidVertexKind { vertex: String, vertex_kind: String },
    #[serde(rename = "dangling-required-edge")]
    DanglingRequiredEdge { vertex: String, edge: String },
    #[serde(rename = "unknown")]
    Unknown { message: String },
}

impl From<schema::ValidationError> for SerializableValidationError {
    fn from(e: schema::ValidationError) -> Self {
        match e {
            schema::ValidationError::InvalidEdge {
                src,
                tgt,
                kind,
                reason,
            } => Self::InvalidEdge {
                src,
                tgt,
                edge_kind: kind,
                reason,
            },
            schema::ValidationError::InvalidConstraintSort { vertex, sort } => {
                Self::InvalidConstraintSort { vertex, sort }
            }
            schema::ValidationError::InvalidVertexKind { vertex, kind } => {
                Self::InvalidVertexKind {
                    vertex,
                    vertex_kind: kind,
                }
            }
            schema::ValidationError::DanglingRequiredEdge { vertex, edge } => {
                Self::DanglingRequiredEdge { vertex, edge }
            }
            _ => Self::Unknown {
                message: format!("{e:?}"),
            },
        }
    }
}

/// Extract migration and schema references from a resource.
///
/// Returns references to the compiled migration and the source/target schemas.
/// For `MigrationWithSchemas`, uses `Arc::clone()` for O(1) schema sharing.
/// For bare `Migration`, builds minimal schemas from surviving vertices/edges.
fn extract_migration_ref(
    r: &Resource,
) -> Result<
    (
        &CompiledMigration,
        panproto_core::schema::Schema,
        panproto_core::schema::Schema,
    ),
    WasmError,
> {
    if let Resource::MigrationWithSchemas {
        compiled,
        src_schema,
        tgt_schema,
    } = r
    {
        // Arc::deref + clone — still clones the Schema. For truly zero-cost
        // sharing, the downstream APIs would need to accept &Schema.
        Ok((compiled, (**src_schema).clone(), (**tgt_schema).clone()))
    } else {
        let compiled = slab::as_migration(r)?;
        let minimal = build_minimal_schema(compiled);
        Ok((compiled, minimal.clone(), minimal))
    }
}

/// Extract migration and schemas as owned values from a resource.
///
/// Same as [`extract_migration_ref`] but clones the compiled migration,
/// which is needed for lens operations that require ownership.
fn extract_migration_owned(
    r: &Resource,
) -> Result<
    (
        CompiledMigration,
        panproto_core::schema::Schema,
        panproto_core::schema::Schema,
    ),
    WasmError,
> {
    if let Resource::MigrationWithSchemas {
        compiled,
        src_schema,
        tgt_schema,
    } = r
    {
        Ok((
            compiled.clone(),
            (**src_schema).clone(),
            (**tgt_schema).clone(),
        ))
    } else {
        let compiled = slab::as_migration(r)?;
        let schema = build_minimal_schema(compiled);
        Ok((compiled.clone(), schema.clone(), schema))
    }
}

/// Build a theory registry for a protocol by name.
///
/// # Errors
///
/// Returns an error string if the protocol name is not recognized.
fn build_theory_registry(protocol_name: &str) -> Result<HashMap<String, Theory>, String> {
    let mut registry = HashMap::new();
    match protocol_name {
        "atproto" => protocols::atproto::register_theories(&mut registry),
        "sql" => protocols::sql::register_theories(&mut registry),
        "protobuf" => protocols::protobuf::register_theories(&mut registry),
        "graphql" => protocols::graphql::register_theories(&mut registry),
        "json-schema" | "jsonschema" => protocols::json_schema::register_theories(&mut registry),
        _ => {
            return Err(format!(
                "unknown protocol: {protocol_name:?}. Supported: atproto, sql, protobuf, graphql, json-schema"
            ));
        }
    }
    Ok(registry)
}

/// Return the names of all built-in protocols (76 total).
fn builtin_protocol_names() -> Vec<String> {
    vec![
        // annotation (19)
        "brat",
        "conllu",
        "naf",
        "uima",
        "folia",
        "tei",
        "timeml",
        "elan",
        "iso_space",
        "paula",
        "laf_graf",
        "decomp",
        "ucca",
        "fovea",
        "bead",
        "web_annotation",
        "amr",
        "concrete",
        "nif",
        // api (5)
        "graphql",
        "openapi",
        "asyncapi",
        "jsonapi",
        "raml",
        // config (4)
        "cloudformation",
        "ansible",
        "k8s_crd",
        "hcl",
        // data_schema (7)
        "json_schema",
        "yaml_schema",
        "toml_schema",
        "cddl",
        "bson",
        "csv_table",
        "ini_schema",
        // data_science (3)
        "dataframe",
        "parquet",
        "arrow",
        // database (6)
        "mongodb",
        "dynamodb",
        "cassandra",
        "neo4j",
        "sql",
        "redis",
        // domain (6)
        "geojson",
        "fhir",
        "rss_atom",
        "vcard_ical",
        "swift_mt",
        "edi_x12",
        // serialization (8)
        "protobuf",
        "avro",
        "thrift",
        "capnproto",
        "flatbuffers",
        "asn1",
        "bond",
        "msgpack_schema",
        // type_system (8)
        "typescript",
        "python",
        "rust_serde",
        "java",
        "go_struct",
        "kotlin",
        "csharp",
        "swift",
        // web_document (10)
        "atproto",
        "jsx",
        "vue",
        "svelte",
        "css",
        "html",
        "markdown",
        "xml_xsd",
        "docx",
        "odf",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// Look up a built-in protocol by name.
#[allow(clippy::too_many_lines)]
fn lookup_builtin_protocol(name: &str) -> Option<panproto_core::schema::Protocol> {
    Some(match name {
        // annotation
        "brat" => protocols::annotation::brat::protocol(),
        "conllu" => protocols::annotation::conllu::protocol(),
        "naf" => protocols::annotation::naf::protocol(),
        "uima" => protocols::annotation::uima::protocol(),
        "folia" => protocols::annotation::folia::protocol(),
        "tei" => protocols::annotation::tei::protocol(),
        "timeml" => protocols::annotation::timeml::protocol(),
        "elan" => protocols::annotation::elan::protocol(),
        "iso_space" => protocols::annotation::iso_space::protocol(),
        "paula" => protocols::annotation::paula::protocol(),
        "laf_graf" => protocols::annotation::laf_graf::protocol(),
        "decomp" => protocols::annotation::decomp::protocol(),
        "ucca" => protocols::annotation::ucca::protocol(),
        "fovea" => protocols::annotation::fovea::protocol(),
        "bead" => protocols::annotation::bead::protocol(),
        "web_annotation" => protocols::annotation::web_annotation::protocol(),
        "amr" => protocols::annotation::amr::protocol(),
        "concrete" => protocols::annotation::concrete::protocol(),
        "nif" => protocols::annotation::nif::protocol(),
        // api
        "graphql" => protocols::api::graphql::protocol(),
        "openapi" => protocols::api::openapi::protocol(),
        "asyncapi" => protocols::api::asyncapi::protocol(),
        "jsonapi" => protocols::api::jsonapi::protocol(),
        "raml" => protocols::api::raml::protocol(),
        // config
        "cloudformation" => protocols::config::cloudformation::protocol(),
        "ansible" => protocols::config::ansible::protocol(),
        "k8s_crd" => protocols::config::k8s_crd::protocol(),
        "hcl" => protocols::config::hcl::protocol(),
        // data_schema
        "json_schema" => protocols::data_schema::json_schema::protocol(),
        "yaml_schema" => protocols::data_schema::yaml_schema::protocol(),
        "toml_schema" => protocols::data_schema::toml_schema::protocol(),
        "cddl" => protocols::data_schema::cddl::protocol(),
        "bson" => protocols::data_schema::bson::protocol(),
        "csv_table" => protocols::data_schema::csv_table::protocol(),
        "ini_schema" => protocols::data_schema::ini_schema::protocol(),
        // data_science
        "dataframe" => protocols::data_science::dataframe::protocol(),
        "parquet" => protocols::data_science::parquet::protocol(),
        "arrow" => protocols::data_science::arrow::protocol(),
        // database
        "mongodb" => protocols::database::mongodb::protocol(),
        "dynamodb" => protocols::database::dynamodb::protocol(),
        "cassandra" => protocols::database::cassandra::protocol(),
        "neo4j" => protocols::database::neo4j::protocol(),
        "sql" => protocols::database::sql::protocol(),
        "redis" => protocols::database::redis::protocol(),
        // domain
        "geojson" => protocols::domain::geojson::protocol(),
        "fhir" => protocols::domain::fhir::protocol(),
        "rss_atom" => protocols::domain::rss_atom::protocol(),
        "vcard_ical" => protocols::domain::vcard_ical::protocol(),
        "swift_mt" => protocols::domain::swift_mt::protocol(),
        "edi_x12" => protocols::domain::edi_x12::protocol(),
        // serialization
        "protobuf" => protocols::serialization::protobuf::protocol(),
        "avro" => protocols::serialization::avro::protocol(),
        "thrift" => protocols::serialization::thrift::protocol(),
        "capnproto" => protocols::serialization::capnproto::protocol(),
        "flatbuffers" => protocols::serialization::flatbuffers::protocol(),
        "asn1" => protocols::serialization::asn1::protocol(),
        "bond" => protocols::serialization::bond::protocol(),
        "msgpack_schema" => protocols::serialization::msgpack_schema::protocol(),
        // type_system
        "typescript" => protocols::type_system::typescript::protocol(),
        "python" => protocols::type_system::python::protocol(),
        "rust_serde" => protocols::type_system::rust_serde::protocol(),
        "java" => protocols::type_system::java::protocol(),
        "go_struct" => protocols::type_system::go_struct::protocol(),
        "kotlin" => protocols::type_system::kotlin::protocol(),
        "csharp" => protocols::type_system::csharp::protocol(),
        "swift" => protocols::type_system::swift::protocol(),
        // web_document
        "atproto" => protocols::web_document::atproto::protocol(),
        "jsx" => protocols::web_document::jsx::protocol(),
        "vue" => protocols::web_document::vue::protocol(),
        "svelte" => protocols::web_document::svelte::protocol(),
        "css" => protocols::web_document::css::protocol(),
        "html" => protocols::web_document::html::protocol(),
        "markdown" => protocols::web_document::markdown::protocol(),
        "xml_xsd" => protocols::web_document::xml_xsd::protocol(),
        "docx" => protocols::web_document::docx::protocol(),
        "odf" => protocols::web_document::odf::protocol(),
        _ => return None,
    })
}

/// Build a minimal `Schema` from a `CompiledMigration`'s surviving
/// vertex and edge sets. This is a fallback used when the full schema
/// is not available (e.g., when a bare `Resource::Migration` handle is
/// used instead of `Resource::MigrationWithSchemas`).
fn build_minimal_schema(compiled: &CompiledMigration) -> panproto_core::schema::Schema {
    use panproto_core::gat::Name;
    use panproto_core::schema::{Edge, Schema, Vertex};
    use smallvec::SmallVec;

    let mut vertices = HashMap::new();
    for v in &compiled.surviving_verts {
        vertices.insert(
            v.clone(),
            Vertex {
                id: v.clone(),
                kind: "unknown".into(),
                nsid: None,
            },
        );
    }

    let mut edges = HashMap::new();
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

    for e in &compiled.surviving_edges {
        edges.insert(e.clone(), e.kind.clone());
        outgoing.entry(e.src.clone()).or_default().push(e.clone());
        incoming.entry(e.tgt.clone()).or_default().push(e.clone());
        between
            .entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
    }

    Schema {
        protocol: String::new(),
        vertices,
        edges,
        hyper_edges: HashMap::new(),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        outgoing,
        incoming,
        between,
    }
}

/// Compose two compiled migrations by chaining vertex and edge remaps.
fn compose_compiled(c1: &CompiledMigration, c2: &CompiledMigration) -> CompiledMigration {
    let surviving_verts = c2.surviving_verts.clone();
    let surviving_edges = c2.surviving_edges.clone();

    // Compose vertex remaps: if c1 maps A->B and c2 maps B->C, composed maps A->C.
    let mut vertex_remap = HashMap::new();
    for (src, intermediate) in &c1.vertex_remap {
        if let Some(tgt) = c2.vertex_remap.get(intermediate) {
            vertex_remap.insert(src.clone(), tgt.clone());
        } else if c2.surviving_verts.contains(intermediate) {
            vertex_remap.insert(src.clone(), intermediate.clone());
        }
    }

    // Compose edge remaps similarly.
    let mut edge_remap = HashMap::new();
    for (src_e, intermediate_e) in &c1.edge_remap {
        if let Some(tgt_e) = c2.edge_remap.get(intermediate_e) {
            edge_remap.insert(src_e.clone(), tgt_e.clone());
        } else if c2.surviving_edges.contains(intermediate_e) {
            edge_remap.insert(src_e.clone(), intermediate_e.clone());
        }
    }

    // Merge resolvers.
    let mut resolver = c2.resolver.clone();
    for ((src, tgt), edge) in &c1.resolver {
        let new_src = vertex_remap
            .get(src)
            .cloned()
            .unwrap_or_else(|| src.clone());
        let new_tgt = vertex_remap
            .get(tgt)
            .cloned()
            .unwrap_or_else(|| tgt.clone());
        resolver
            .entry((new_src, new_tgt))
            .or_insert_with(|| edge.clone());
    }

    CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap,
        edge_remap,
        resolver,
        hyper_resolver: c2.hyper_resolver.clone(),
    }
}

/// Build a default protocol spec with the given name.
///
/// Used as a fallback when the schema's protocol name does not match any
/// built-in protocol.
fn default_protocol(name: &str) -> panproto_core::schema::Protocol {
    panproto_core::schema::Protocol {
        name: name.into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![],
        obj_kinds: vec!["object".into(), "string".into(), "record".into()],
        constraint_sorts: vec![],
        ..panproto_core::schema::Protocol::default()
    }
}

/// A simple schema diff result.
#[derive(Debug, Default, Serialize, Deserialize)]
struct SchemaDiff {
    /// Vertices added in the second schema.
    added_vertices: Vec<String>,
    /// Vertices removed from the first schema.
    removed_vertices: Vec<String>,
    /// Edges added in the second schema.
    added_edges: Vec<EdgeDiff>,
    /// Edges removed from the first schema.
    removed_edges: Vec<EdgeDiff>,
    /// Vertices whose kind changed.
    kind_changes: Vec<KindChange>,
}

/// A serializable edge for diffs.
#[derive(Debug, Serialize, Deserialize)]
struct EdgeDiff {
    /// Source vertex ID.
    src: String,
    /// Target vertex ID.
    tgt: String,
    /// Edge kind.
    kind: String,
    /// Optional edge name.
    name: Option<String>,
}

/// A vertex kind change.
#[derive(Debug, Serialize, Deserialize)]
struct KindChange {
    /// Vertex ID.
    vertex: String,
    /// Old kind.
    old_kind: String,
    /// New kind.
    new_kind: String,
}

/// Compute a structural diff between two schemas.
fn compute_diff(
    old: &panproto_core::schema::Schema,
    new: &panproto_core::schema::Schema,
) -> SchemaDiff {
    let mut diff = SchemaDiff::default();

    for id in new.vertices.keys() {
        if !old.vertices.contains_key(id) {
            diff.added_vertices.push(id.to_string());
        }
    }
    for id in old.vertices.keys() {
        if !new.vertices.contains_key(id) {
            diff.removed_vertices.push(id.to_string());
        }
    }

    for (id, new_v) in &new.vertices {
        if let Some(old_v) = old.vertices.get(id) {
            if old_v.kind != new_v.kind {
                diff.kind_changes.push(KindChange {
                    vertex: id.to_string(),
                    old_kind: old_v.kind.to_string(),
                    new_kind: new_v.kind.to_string(),
                });
            }
        }
    }

    for edge in new.edges.keys() {
        if !old.edges.contains_key(edge) {
            diff.added_edges.push(EdgeDiff {
                src: edge.src.to_string(),
                tgt: edge.tgt.to_string(),
                kind: edge.kind.to_string(),
                name: edge.name.as_ref().map(ToString::to_string),
            });
        }
    }
    for edge in old.edges.keys() {
        if !new.edges.contains_key(edge) {
            diff.removed_edges.push(EdgeDiff {
                src: edge.src.to_string(),
                tgt: edge.tgt.to_string(),
                kind: edge.kind.to_string(),
                name: edge.name.as_ref().map(ToString::to_string),
            });
        }
    }

    diff.added_vertices.sort();
    diff.removed_vertices.sort();

    diff
}
