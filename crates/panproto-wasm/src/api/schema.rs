//! Schema construction, migration compilation, classification, and validation.
//!
//! Split out of the monolithic api.rs into a domain module.

use panproto_core::{
    check,
    inst::{self, WInstance},
    lens::{self, Complement},
    mig::{self, Migration},
    protocols,
    schema::{self, SchemaBuilder},
};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::error::WasmError;
use crate::slab::{self, Resource};

use super::BuildOp;
use super::helpers::{
    SchemaDiff, SerializableValidationError, build_theory_registry, compose_compiled, compute_diff,
    extract_migration_owned, extract_migration_ref,
};

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

/// Parse an `ATProto` lexicon JSON document into a schema.
///
/// Takes the raw JSON bytes of a lexicon file (e.g., `app.bsky.feed.post`
/// or `pub.layers.annotation.annotationLayer`) and returns a schema handle.
/// This is the generic entry point for any `ATProto`-compatible lexicon;
/// works for `Bluesky`, `RelationalText`, Layers, and any custom lexicon.
///
/// # Errors
///
/// Returns `JsError` if the JSON cannot be parsed or the lexicon is
/// not a valid `ATProto` Lexicon document.
#[wasm_bindgen]
pub fn parse_atproto_lexicon(json_bytes: &[u8]) -> Result<u32, JsError> {
    let json: serde_json::Value =
        serde_json::from_slice(json_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;
    let schema =
        protocols::atproto::parse_lexicon(&json).map_err(|e| WasmError::SchemaBuildFailed {
            reason: e.to_string(),
        })?;
    Ok(slab::alloc(Resource::Schema(std::sync::Arc::new(schema))))
}

#[derive(serde::Serialize)]
struct SchemaMeta {
    protocol: String,
    vertices: Vec<VertexMeta>,
    edges: Vec<EdgeMeta>,
}
#[derive(serde::Serialize)]
struct VertexMeta {
    id: String,
    kind: String,
    nsid: Option<String>,
}
#[derive(serde::Serialize)]
struct EdgeMeta {
    src: String,
    tgt: String,
    kind: String,
    name: Option<String>,
}

/// Extract schema metadata from a schema handle.
///
/// Returns `MessagePack`-encoded schema data including protocol name,
/// vertex IDs and kinds, edge sources/targets/kinds/names, and
/// constraint information. Used by the TypeScript SDK to populate
/// `SchemaData` for schemas built on the Rust side (e.g., via
/// [`parse_atproto_lexicon`]).
///
/// # Errors
///
/// Returns `JsError` if the handle is invalid.
#[wasm_bindgen]
pub fn schema_metadata(schema_handle: u32) -> Result<Vec<u8>, JsError> {
    slab::with_resource(schema_handle, |r| {
        let schema = slab::as_schema(r)?;

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

        rmp_serde::to_vec(&meta).map_err(|e| WasmError::SerializationFailed {
            reason: e.to_string(),
        })
    })
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
