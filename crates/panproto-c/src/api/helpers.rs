//! Cold-path adapters and serializable shadow structs shared across
//! domain modules.
//!
//! Ported from `panproto_wasm::api::helpers` (see
//! `crates/panproto-wasm/src/api/helpers.rs`), with the WASM error type
//! `WasmError` replaced by [`FfiError`] and all `JsError` usage removed.
//! The serializable shadow structs are the CBOR payload types the C ABI
//! exchanges with the host; the adapter functions assemble engine inputs
//! from handle-resident resources.
//!
//! Everything in this module is a real implementation: it is reused
//! across the api modules and so must compile and behave correctly.

use std::collections::HashMap;

use panproto_core::{
    gat::Theory,
    inst::CompiledMigration,
    lens::{self},
    protocols,
    schema::{self, Schema, SchemaBuilder},
};
use serde::{Deserialize, Serialize};

use crate::error::FfiError;
use crate::handle::Resource;

// ---------------------------------------------------------------------------
// Builder operations (shared schema-construction payload)
// ---------------------------------------------------------------------------

/// A serializable builder operation for constructing schemas.
///
/// The CBOR payload for `pp_schema_build` is a `Vec<BuildOp>`. Mirrors
/// the WASM `BuildOp` enum used by `build_schema`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum BuildOp {
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

/// Apply a sequence of [`BuildOp`]s to a [`SchemaBuilder`] over `protocol`.
///
/// # Errors
///
/// Returns [`FfiError::Operation`] if any builder step fails (an unknown
/// edge kind, a dangling hyper-edge signature, and so on).
pub fn build_schema_from_ops(
    protocol: &schema::Protocol,
    ops: Vec<BuildOp>,
) -> Result<Schema, FfiError> {
    let mut builder = SchemaBuilder::new(protocol);
    for op in ops {
        match op {
            BuildOp::Vertex { id, kind, nsid } => {
                builder = builder
                    .vertex(&id, &kind, nsid.as_deref())
                    .map_err(|e| FfiError::Operation(format!("vertex {id:?}: {e}")))?;
            }
            BuildOp::Edge {
                src,
                tgt,
                kind,
                name,
            } => {
                builder = builder
                    .edge(&src, &tgt, &kind, name.as_deref())
                    .map_err(|e| FfiError::Operation(format!("edge {src}->{tgt}: {e}")))?;
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
                    .map_err(|e| FfiError::Operation(format!("hyper_edge {id:?}: {e}")))?;
            }
            BuildOp::Required { vertex, edges } => {
                builder = builder.required(&vertex, edges);
            }
        }
    }
    builder
        .build()
        .map_err(|e| FfiError::Operation(format!("schema build: {e}")))
}

// ---------------------------------------------------------------------------
// Migration extraction
// ---------------------------------------------------------------------------

/// Extract migration and schema references from a resource.
///
/// For [`Resource::MigrationWithSchemas`], clones the bundled schemas
/// (O(1) `Arc` deref plus a `Schema` clone). For a bare
/// [`Resource::Migration`], synthesizes minimal source/target schemas
/// from the surviving vertex and edge sets.
///
/// # Errors
///
/// Returns [`FfiError::TypeMismatch`] when the resource is not a
/// migration.
pub fn extract_migration_ref(
    r: &Resource,
) -> Result<(&CompiledMigration, Schema, Schema), FfiError> {
    if let Resource::MigrationWithSchemas {
        compiled,
        src_schema,
        tgt_schema,
    } = r
    {
        Ok((compiled, (**src_schema).clone(), (**tgt_schema).clone()))
    } else {
        let compiled = r.as_migration()?;
        let minimal = build_minimal_schema(compiled);
        Ok((compiled, minimal.clone(), minimal))
    }
}

/// Extract migration and schemas as owned values from a resource.
///
/// Same as [`extract_migration_ref`] but clones the compiled migration,
/// which is needed for lens operations that require ownership.
///
/// # Errors
///
/// Returns [`FfiError::TypeMismatch`] when the resource is not a
/// migration.
pub fn extract_migration_owned(
    r: &Resource,
) -> Result<(CompiledMigration, Schema, Schema), FfiError> {
    if let Resource::MigrationWithSchemas {
        compiled,
        src_schema,
        tgt_schema,
    } = r
    {
        Ok((
            (**compiled).clone(),
            (**src_schema).clone(),
            (**tgt_schema).clone(),
        ))
    } else {
        let compiled = r.as_migration()?;
        let schema = build_minimal_schema(compiled);
        Ok((compiled.clone(), schema.clone(), schema))
    }
}

/// Build a minimal [`Schema`] from a [`CompiledMigration`]'s surviving
/// vertex and edge sets.
///
/// Fallback used when the full schema is unavailable (a bare
/// [`Resource::Migration`] handle rather than
/// [`Resource::MigrationWithSchemas`]).
#[must_use]
pub fn build_minimal_schema(compiled: &CompiledMigration) -> Schema {
    use panproto_core::gat::Name;
    use panproto_core::schema::{Edge, Vertex};
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
        entries: Vec::new(),
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
        outgoing,
        incoming,
        between,
    }
}

/// Compose two compiled migrations by chaining vertex and edge remaps.
#[must_use]
pub fn compose_compiled(c1: &CompiledMigration, c2: &CompiledMigration) -> CompiledMigration {
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
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
        expansion_path: HashMap::new(),
    }
}

// ---------------------------------------------------------------------------
// Protocol registry helpers
// ---------------------------------------------------------------------------

/// Build a theory registry for a protocol by name.
///
/// # Errors
///
/// Returns [`FfiError::Operation`] if the protocol name is not
/// recognized.
pub fn build_theory_registry(protocol_name: &str) -> Result<HashMap<String, Theory>, FfiError> {
    let mut registry = HashMap::new();
    match protocol_name {
        "atproto" => protocols::atproto::register_theories(&mut registry),
        _ => {
            return Err(FfiError::Operation(format!(
                "unknown protocol: {protocol_name:?}. Supported: atproto"
            )));
        }
    }
    Ok(registry)
}

/// Return the names of all built-in semantic protocols.
///
/// Programming-language and data-format protocols are handled by
/// tree-sitter grammars via `panproto-grammars`. This list covers only
/// the domain-specific semantic protocols that require custom theory
/// composition.
#[must_use]
pub fn builtin_protocol_names() -> Vec<String> {
    [
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
        // api (4)
        "openapi",
        "asyncapi",
        "jsonapi",
        "raml",
        // config (3)
        "cloudformation",
        "ansible",
        "k8s_crd",
        // data_schema (2)
        "cddl",
        "bson",
        // data_science (3)
        "dataframe",
        "parquet",
        "arrow",
        // database (5)
        "mongodb",
        "dynamodb",
        "cassandra",
        "neo4j",
        "redis",
        // domain (6)
        "geojson",
        "fhir",
        "rss_atom",
        "vcard_ical",
        "swift_mt",
        "edi_x12",
        // serialization (5)
        "avro",
        "flatbuffers",
        "asn1",
        "bond",
        "msgpack_schema",
        // web_document (3)
        "atproto",
        "docx",
        "odf",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// Look up a built-in protocol by name.
#[must_use]
pub fn lookup_builtin_protocol(name: &str) -> Option<schema::Protocol> {
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
        "openapi" => protocols::api::openapi::protocol(),
        "asyncapi" => protocols::api::asyncapi::protocol(),
        "jsonapi" => protocols::api::jsonapi::protocol(),
        "raml" => protocols::api::raml::protocol(),
        // config
        "cloudformation" => protocols::config::cloudformation::protocol(),
        "ansible" => protocols::config::ansible::protocol(),
        "k8s_crd" => protocols::config::k8s_crd::protocol(),
        // data_schema
        "cddl" => protocols::data_schema::cddl::protocol(),
        "bson" => protocols::data_schema::bson::protocol(),
        // data_science
        "dataframe" => protocols::data_science::dataframe::protocol(),
        "parquet" => protocols::data_science::parquet::protocol(),
        "arrow" => protocols::data_science::arrow::protocol(),
        // database
        "mongodb" => protocols::database::mongodb::protocol(),
        "dynamodb" => protocols::database::dynamodb::protocol(),
        "cassandra" => protocols::database::cassandra::protocol(),
        "neo4j" => protocols::database::neo4j::protocol(),
        "redis" => protocols::database::redis::protocol(),
        // domain
        "geojson" => protocols::domain::geojson::protocol(),
        "fhir" => protocols::domain::fhir::protocol(),
        "rss_atom" => protocols::domain::rss_atom::protocol(),
        "vcard_ical" => protocols::domain::vcard_ical::protocol(),
        "swift_mt" => protocols::domain::swift_mt::protocol(),
        "edi_x12" => protocols::domain::edi_x12::protocol(),
        // serialization
        "avro" => protocols::serialization::avro::protocol(),
        "flatbuffers" => protocols::serialization::flatbuffers::protocol(),
        "asn1" => protocols::serialization::asn1::protocol(),
        "bond" => protocols::serialization::bond::protocol(),
        "msgpack_schema" => protocols::serialization::msgpack_schema::protocol(),
        // web_document
        "atproto" => protocols::web_document::atproto::protocol(),
        "docx" => protocols::web_document::docx::protocol(),
        "odf" => protocols::web_document::odf::protocol(),
        _ => return None,
    })
}

/// Build a default protocol spec with the given name.
///
/// Used as a fallback when the schema's protocol name does not match any
/// built-in protocol.
#[must_use]
pub fn default_protocol(name: &str) -> schema::Protocol {
    schema::Protocol {
        name: name.into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![],
        obj_kinds: vec!["object".into(), "string".into(), "record".into()],
        constraint_sorts: vec![],
        ..schema::Protocol::default()
    }
}

/// Resolve the protocol for a schema: a built-in if recognized,
/// otherwise a default spec carrying the schema's protocol name.
#[must_use]
pub fn protocol_for_schema(schema: &Schema) -> schema::Protocol {
    lookup_builtin_protocol(&schema.protocol).unwrap_or_else(|| default_protocol(&schema.protocol))
}

/// Infer a suitable root vertex from a schema for JSON parsing.
///
/// Consults the schema's declared entry vertices via
/// [`schema::primary_entry`]. Falls back to `"root"` only if the schema
/// has no declared entry.
#[must_use]
pub fn infer_root_vertex(schema: &Schema) -> String {
    schema::primary_entry(schema).map_or_else(|| "root".to_owned(), ToString::to_string)
}

// ---------------------------------------------------------------------------
// Serializable shadow structs (CBOR payload types)
// ---------------------------------------------------------------------------

/// Result of a lens law check.
#[derive(Debug, Serialize, Deserialize)]
pub struct LawCheckResult {
    /// Whether the law holds on the tested instance.
    pub holds: bool,
    /// Human-readable description of the violation, if any.
    pub violation: Option<String>,
}

/// Result of a morphism validity check.
#[derive(Debug, Serialize, Deserialize)]
pub struct MorphismCheckResult {
    /// Whether the morphism is valid.
    pub valid: bool,
    /// Human-readable description of the failure, if any.
    pub error: Option<String>,
}

/// Result of staging a schema in a VCS repo.
#[derive(Debug, Serialize, Deserialize)]
pub struct VcsAddResult {
    /// The hex-encoded object ID of the staged schema.
    pub schema_id: String,
}

/// A commit log entry.
#[derive(Debug, Serialize, Deserialize)]
pub struct VcsLogEntry {
    /// Commit message.
    pub message: String,
    /// Commit author.
    pub author: String,
    /// Unix timestamp.
    pub timestamp: u64,
    /// Protocol the committed schema targets.
    pub protocol: String,
}

/// VCS status result.
#[derive(Debug, Serialize, Deserialize)]
pub struct VcsStatusResult {
    /// Current branch, or `None` when HEAD is detached.
    pub branch: Option<String>,
    /// Hex-encoded HEAD commit ID, if any.
    pub head_commit: Option<String>,
}

/// VCS operation result.
#[derive(Debug, Serialize, Deserialize)]
pub struct VcsOpResult {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Human-readable description of the outcome.
    pub message: String,
}

/// VCS diff result (branch listing).
#[derive(Debug, Serialize, Deserialize)]
pub struct VcsDiffResult {
    /// All branches and the commit each points at.
    pub branches: Vec<VcsBranchInfo>,
}

/// VCS branch info.
#[derive(Debug, Serialize, Deserialize)]
pub struct VcsBranchInfo {
    /// Branch name.
    pub name: String,
    /// Hex-encoded commit ID the branch points at.
    pub commit_id: String,
}

/// VCS blame result.
#[derive(Debug, Serialize, Deserialize)]
pub struct VcsBlameResult {
    /// Hex-encoded commit ID that introduced the vertex.
    pub commit_id: String,
    /// Commit author.
    pub author: String,
    /// Unix timestamp.
    pub timestamp: u64,
    /// Commit message.
    pub message: String,
}

/// Info about a single protolens step, for JSON/CBOR serialization.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProtolensStepInfo {
    /// Step name.
    pub name: String,
    /// Source endofunctor name.
    pub source_endofunctor: String,
    /// Target endofunctor name.
    pub target_endofunctor: String,
    /// Whether the step is lossless.
    pub lossless: bool,
}

/// A structural schema diff result (lightweight, vertex/edge level).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SchemaDiff {
    /// Vertices added in the second schema.
    pub added_vertices: Vec<String>,
    /// Vertices removed from the first schema.
    pub removed_vertices: Vec<String>,
    /// Edges added in the second schema.
    pub added_edges: Vec<EdgeDiff>,
    /// Edges removed from the first schema.
    pub removed_edges: Vec<EdgeDiff>,
    /// Vertices whose kind changed.
    pub kind_changes: Vec<KindChange>,
}

/// A serializable edge for diffs.
#[derive(Debug, Serialize, Deserialize)]
pub struct EdgeDiff {
    /// Source vertex ID.
    pub src: String,
    /// Target vertex ID.
    pub tgt: String,
    /// Edge kind.
    pub kind: String,
    /// Optional edge name.
    pub name: Option<String>,
}

/// A vertex kind change.
#[derive(Debug, Serialize, Deserialize)]
pub struct KindChange {
    /// Vertex ID.
    pub vertex: String,
    /// Old kind.
    pub old_kind: String,
    /// New kind.
    pub new_kind: String,
}

/// Compute a lightweight structural diff between two schemas.
#[must_use]
pub fn compute_diff(old: &Schema, new: &Schema) -> SchemaDiff {
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

// ---------------------------------------------------------------------------
// Protolens step specs (CBOR payload + chain construction)
// ---------------------------------------------------------------------------

/// Spec for a single protolens step, deserialized from a CBOR payload.
#[derive(Debug, Deserialize)]
pub struct ProtolensStepSpec {
    /// The type of step: `add_sort`, `drop_sort`, `rename_sort`,
    /// `add_op`, `drop_op`, `rename_op`, `rename_edge_name`, `scoped`,
    /// `rename_field`, `hoist_field`, `remove_field`, `add_field`,
    /// `nest_field`, `map_items`.
    pub step_type: String,
    /// Primary argument (sort/op name, or old name for renames).
    pub name: String,
    /// Secondary argument (new name for renames, kind for adds).
    #[serde(default)]
    pub target: String,
    /// Third argument (vertex kind for `add_sort`).
    #[serde(default)]
    pub kind: String,
    /// Source sort for `rename_edge_name`.
    #[serde(default)]
    pub src_sort: String,
    /// Target sort for `rename_edge_name`.
    #[serde(default)]
    pub tgt_sort: String,
    /// Parent vertex for combinators (`rename_field`, `add_field`, etc.).
    #[serde(default)]
    pub parent: String,
    /// Intermediate vertex for `hoist_field` / `nest_field`.
    #[serde(default)]
    pub intermediate: String,
    /// Original edge label of the `parent -> child` edge, for `nest_field`.
    /// Empty string means the edge had no label.
    #[serde(default)]
    pub old_edge_name: String,
    /// Label for the new `parent -> intermediate` edge in `nest_field`.
    #[serde(default)]
    pub parent_to_intermediate: String,
    /// Label for the new `intermediate -> child` edge in `nest_field`.
    #[serde(default)]
    pub intermediate_to_child: String,
    /// Inner step spec for `scoped` / `map_items`.
    #[serde(default)]
    pub inner: Option<Box<Self>>,
}

/// Build a [`lens::ProtolensChain`] from a serialized step spec.
///
/// The match dispatches over the elementary and derived step kinds; each
/// arm wires different fields of [`ProtolensStepSpec`] into the correct
/// combinator call.
///
/// # Errors
///
/// Returns [`FfiError::Operation`] for an unknown step type, a missing
/// inner spec, or a chain-fusion failure.
#[allow(clippy::too_many_lines)]
pub fn build_chain_from_step_spec(
    spec: &ProtolensStepSpec,
) -> Result<lens::ProtolensChain, FfiError> {
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
        "rename_edge_name" => lens::protolens::elementary::rename_edge_name(
            Name::from(spec.src_sort.as_str()),
            Name::from(spec.tgt_sort.as_str()),
            Name::from(spec.name.as_str()),
            Name::from(spec.target.as_str()),
        ),
        "scoped" | "map_items" => {
            let inner_spec = spec.inner.as_ref().ok_or_else(|| {
                FfiError::Operation(format!(
                    "'{0}' requires an 'inner' step spec",
                    spec.step_type
                ))
            })?;
            let inner_chain = build_chain_from_step_spec(inner_spec)?;
            // If the inner chain has one step, use it directly; otherwise
            // fuse into a single step for scoping.
            let inner_step = match <[_; 1]>::try_from(inner_chain.steps) {
                Ok([only]) => only,
                Err(multi) => lens::ProtolensChain::new(multi)
                    .fuse()
                    .map_err(|e| FfiError::Operation(format!("failed to fuse inner chain: {e}")))?,
            };
            lens::protolens::elementary::scoped(Name::from(spec.name.as_str()), inner_step)
        }
        // Derived combinators return ProtolensChains directly.
        "rename_field" => {
            // name = old edge label (also used as field vertex ID),
            // target = new edge label
            return Ok(lens::combinators::rename_field(
                Name::from(spec.parent.as_str()),
                Name::from(spec.name.as_str()),
                Name::from(spec.name.as_str()),
                Name::from(spec.target.as_str()),
            ));
        }
        "remove_field" => {
            return Ok(lens::combinators::remove_field(Name::from(
                spec.name.as_str(),
            )));
        }
        "add_field" => {
            return Ok(lens::combinators::add_field(
                Name::from(spec.parent.as_str()),
                Name::from(spec.name.as_str()),
                Name::from(if spec.kind.is_empty() {
                    spec.name.as_str()
                } else {
                    spec.kind.as_str()
                }),
                Value::Null,
            ));
        }
        "hoist_field" => {
            return Ok(lens::combinators::hoist_field(
                Name::from(spec.parent.as_str()),
                Name::from(spec.intermediate.as_str()),
                Name::from(spec.name.as_str()),
            ));
        }
        "nest_field" => {
            // intermediate_kind: kind of the new intermediate vertex.
            let intermediate_kind = if spec.kind.is_empty() {
                spec.intermediate.as_str()
            } else {
                spec.kind.as_str()
            };
            // edge_kind: kind stamped on the two new edges (default "prop").
            let edge_kind = if spec.target.is_empty() {
                "prop"
            } else {
                spec.target.as_str()
            };
            // old_edge_name: label of the original parent -> child edge.
            let old_edge_name = if spec.old_edge_name.is_empty() {
                None
            } else {
                Some(Name::from(spec.old_edge_name.as_str()))
            };
            // Labels for the two new edges. Default to the intermediate
            // and child vertex ids respectively, preserving the historical
            // "label == vertex id" convention for callers that don't
            // distinguish the two.
            let parent_to_intermediate = if spec.parent_to_intermediate.is_empty() {
                spec.intermediate.as_str()
            } else {
                spec.parent_to_intermediate.as_str()
            };
            let intermediate_to_child = if spec.intermediate_to_child.is_empty() {
                spec.name.as_str()
            } else {
                spec.intermediate_to_child.as_str()
            };
            return Ok(lens::combinators::nest_field(
                Name::from(spec.parent.as_str()),
                Name::from(spec.name.as_str()),
                Name::from(spec.intermediate.as_str()),
                Name::from(intermediate_kind),
                Name::from(edge_kind),
                old_edge_name,
                Name::from(parent_to_intermediate),
                Name::from(intermediate_to_child),
            ));
        }
        other => {
            return Err(FfiError::Operation(format!("unknown step type: {other}")));
        }
    };

    Ok(lens::ProtolensChain::new(vec![protolens]))
}

/// Classify a protolens chain's optic kind from its complement
/// constructors.
#[must_use]
pub fn classify_optic_kind(chain: &lens::ProtolensChain) -> &'static str {
    if chain.steps.is_empty() {
        return "iso";
    }

    let mut has_added = false;
    let mut has_dropped = false;
    let mut has_composite = false;

    for step in &chain.steps {
        classify_complement(
            &step.complement_constructor,
            &mut has_added,
            &mut has_dropped,
            &mut has_composite,
        );
    }

    match (has_added, has_dropped, has_composite) {
        (false, false, false) => "iso",
        (true, false, _) => "lens",
        (false, true, _) => "prism",
        (_, _, true) if !has_added && !has_dropped => "traversal",
        _ => "affine",
    }
}

/// Recursively classify complement constructors.
fn classify_complement(
    cc: &lens::protolens::ComplementConstructor,
    has_added: &mut bool,
    has_dropped: &mut bool,
    has_composite: &mut bool,
) {
    match cc {
        lens::protolens::ComplementConstructor::Empty => {}
        lens::protolens::ComplementConstructor::AddedElement { .. } => {
            *has_added = true;
        }
        lens::protolens::ComplementConstructor::Composite(subs) => {
            *has_composite = true;
            for sub in subs {
                classify_complement(sub, has_added, has_dropped, has_composite);
            }
        }
        _ => {
            *has_dropped = true;
        }
    }
}
