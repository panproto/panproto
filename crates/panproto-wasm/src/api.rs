//! The ten `#[wasm_bindgen]` entry points for panproto-wasm.
//!
//! Each function takes handles (`u32`) and/or `MessagePack` byte slices,
//! performs the requested operation, and returns either a handle or
//! serialized bytes. All errors are converted to `JsError`.

use std::collections::HashMap;

use panproto_core::{
    gat::Theory,
    inst::{CompiledMigration, WInstance},
    lens::{self, Complement},
    mig::{self, Migration},
    protocols,
    schema::SchemaBuilder,
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

    Ok(slab::alloc(Resource::Schema(schema)))
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
        src_schema: Box::new(src_schema),
        tgt_schema: Box::new(tgt_schema),
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

/// Release a resource handle, making it available for reuse.
#[wasm_bindgen]
pub fn free_handle(handle: u32) {
    slab::free(handle);
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Extract migration and schema references from a resource.
///
/// Returns references to the compiled migration and the source/target schemas.
/// For `MigrationWithSchemas`, uses the stored schemas directly.
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

/// Build a minimal `Schema` from a `CompiledMigration`'s surviving
/// vertex and edge sets. This is a fallback used when the full schema
/// is not available (e.g., when a bare `Resource::Migration` handle is
/// used instead of `Resource::MigrationWithSchemas`).
fn build_minimal_schema(compiled: &CompiledMigration) -> panproto_core::schema::Schema {
    use panproto_core::schema::{Edge, Schema, Vertex};
    use smallvec::SmallVec;

    let mut vertices = HashMap::new();
    for v in &compiled.surviving_verts {
        vertices.insert(
            v.clone(),
            Vertex {
                id: v.clone(),
                kind: "unknown".to_string(),
                nsid: None,
            },
        );
    }

    let mut edges = HashMap::new();
    let mut outgoing: HashMap<String, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<String, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(String, String), SmallVec<Edge, 2>> = HashMap::new();

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
            diff.added_vertices.push(id.clone());
        }
    }
    for id in old.vertices.keys() {
        if !new.vertices.contains_key(id) {
            diff.removed_vertices.push(id.clone());
        }
    }

    for (id, new_v) in &new.vertices {
        if let Some(old_v) = old.vertices.get(id) {
            if old_v.kind != new_v.kind {
                diff.kind_changes.push(KindChange {
                    vertex: id.clone(),
                    old_kind: old_v.kind.clone(),
                    new_kind: new_v.kind.clone(),
                });
            }
        }
    }

    for edge in new.edges.keys() {
        if !old.edges.contains_key(edge) {
            diff.added_edges.push(EdgeDiff {
                src: edge.src.clone(),
                tgt: edge.tgt.clone(),
                kind: edge.kind.clone(),
                name: edge.name.clone(),
            });
        }
    }
    for edge in old.edges.keys() {
        if !new.edges.contains_key(edge) {
            diff.removed_edges.push(EdgeDiff {
                src: edge.src.clone(),
                tgt: edge.tgt.clone(),
                kind: edge.kind.clone(),
                name: edge.name.clone(),
            });
        }
    }

    diff.added_vertices.sort();
    diff.removed_vertices.sort();

    diff
}
