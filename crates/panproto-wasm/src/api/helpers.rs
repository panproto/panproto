//! Internal helpers shared by api submodules.
//!
//! Split out of the monolithic api.rs into a domain module.

use panproto_core::{
    gat::{self, Theory},
    inst::{self, CompiledMigration, WInstance},
    lens::{self},
    protocols,
    schema::{self, Schema, SchemaBuilder},
};
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::error::WasmError;
use crate::slab::{self, Resource};

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Infer a suitable root vertex from a schema for JSON parsing.
///
/// Consults the schema's declared entry vertices via
/// [`schema::primary_entry`]. Falls back to `"root"` only if the schema
/// has no vertices (pathological case).
pub(super) fn infer_root_vertex(schema: &panproto_core::schema::Schema) -> String {
    schema::primary_entry(schema).map_or_else(|| "root".to_owned(), ToString::to_string)
}

/// Result of a lens law check.
#[derive(Debug, Serialize)]
pub(super) struct LawCheckResult {
    pub(super) holds: bool,
    pub(super) violation: Option<String>,
}

/// Result of a morphism validity check.
#[derive(Debug, Serialize)]
pub(super) struct MorphismCheckResult {
    pub(super) valid: bool,
    pub(super) error: Option<String>,
}

/// Result of staging a schema in a VCS repo.
#[derive(Debug, Serialize)]
pub(super) struct VcsAddResult {
    pub(super) schema_id: String,
}

/// A commit log entry.
#[derive(Debug, Serialize)]
pub(super) struct VcsLogEntry {
    pub(super) message: String,
    pub(super) author: String,
    pub(super) timestamp: u64,
    pub(super) protocol: String,
}

/// VCS status result.
#[derive(Debug, Serialize)]
pub(super) struct VcsStatusResult {
    pub(super) branch: Option<String>,
    pub(super) head_commit: Option<String>,
}

/// VCS operation result.
#[derive(Debug, Serialize)]
pub(super) struct VcsOpResult {
    pub(super) success: bool,
    pub(super) message: String,
}

/// VCS diff result (simplified).
#[derive(Debug, Serialize)]
pub(super) struct VcsDiffResult {
    pub(super) branches: Vec<VcsBranchInfo>,
}

/// VCS branch info.
#[derive(Debug, Serialize)]
pub(super) struct VcsBranchInfo {
    pub(super) name: String,
    pub(super) commit_id: String,
}

/// VCS blame result.
#[derive(Debug, Serialize)]
pub(super) struct VcsBlameResult {
    pub(super) commit_id: String,
    pub(super) author: String,
    pub(super) timestamp: u64,
    pub(super) message: String,
}

/// Info about a single protolens step, for JSON serialization.
#[derive(Debug, Serialize)]
pub(super) struct ProtolensStepInfo {
    pub(super) name: String,
    pub(super) source_endofunctor: String,
    pub(super) target_endofunctor: String,
    pub(super) lossless: bool,
}

/// Info about a factorization step, for msgpack serialization.
#[derive(Debug, Serialize)]
pub(super) struct FactorizationStepInfo {
    pub(super) name: String,
    pub(super) transform: String,
}

/// Spec for a single protolens step, deserialized from msgpack.
#[derive(Debug, Deserialize)]
pub(super) struct ProtolensStepSpec {
    /// The type of step: `add_sort`, `drop_sort`, `rename_sort`,
    /// `add_op`, `drop_op`, `rename_op`, `rename_edge_name`, `scoped`,
    /// `rename_field`, `hoist_field`, `remove_field`, `add_field`,
    /// `nest_field`, `map_items`.
    pub(super) step_type: String,
    /// Primary argument (sort/op name, or old name for renames).
    pub(super) name: String,
    /// Secondary argument (new name for renames, kind for adds).
    #[serde(default)]
    pub(super) target: String,
    /// Third argument (vertex kind for `add_sort`).
    #[serde(default)]
    pub(super) kind: String,
    /// Source sort for `rename_edge_name`.
    #[serde(default)]
    pub(super) src_sort: String,
    /// Target sort for `rename_edge_name`.
    #[serde(default)]
    pub(super) tgt_sort: String,
    /// Parent vertex for combinators (`rename_field`, `add_field`, etc.).
    #[serde(default)]
    pub(super) parent: String,
    /// Intermediate vertex for `hoist_field` / `nest_field`.
    #[serde(default)]
    pub(super) intermediate: String,
    /// Original edge label of the `parent → child` edge, for `nest_field`.
    /// Empty string means the edge had no label.
    #[serde(default)]
    pub(super) old_edge_name: String,
    /// Label for the new `parent → intermediate` edge in `nest_field`.
    #[serde(default)]
    pub(super) parent_to_intermediate: String,
    /// Label for the new `intermediate → child` edge in `nest_field`.
    #[serde(default)]
    pub(super) intermediate_to_child: String,
    /// Inner step spec for `scoped` / `map_items`.
    #[serde(default)]
    pub(super) inner: Option<Box<Self>>,
}

/// Build a `ProtolensChain` from a serialized step spec.
///
/// The match dispatches over ~14 elementary and derived step kinds; each
/// arm wires different fields of [`ProtolensStepSpec`] into the correct
/// combinator call. The length reflects the combinator surface, not
/// algorithmic complexity.
#[allow(clippy::too_many_lines)]
pub(super) fn build_chain_from_step_spec(
    spec: &ProtolensStepSpec,
) -> Result<lens::ProtolensChain, JsError> {
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
            let inner_spec =
                spec.inner
                    .as_ref()
                    .ok_or_else(|| WasmError::LensConstructionFailed {
                        reason: format!("'{0}' requires an 'inner' step spec", spec.step_type),
                    })?;
            let inner_chain = build_chain_from_step_spec(inner_spec)?;
            // If the inner chain has one step, use it directly;
            // otherwise fuse into a single step for scoping.
            let inner_step = match <[_; 1]>::try_from(inner_chain.steps) {
                Ok([only]) => only,
                Err(multi) => lens::ProtolensChain::new(multi).fuse().map_err(|e| {
                    WasmError::LensConstructionFailed {
                        reason: format!("failed to fuse inner chain: {e}"),
                    }
                })?,
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
            // old_edge_name: label of the original parent → child edge.
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
pub(super) enum SerializableValidationError {
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
pub(super) fn extract_migration_ref(
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
        // Arc::deref + clone; still clones the Schema. For truly zero-cost
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
pub(super) fn extract_migration_owned(
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
pub(super) fn build_theory_registry(
    protocol_name: &str,
) -> Result<HashMap<String, Theory>, String> {
    let mut registry = HashMap::new();
    match protocol_name {
        "atproto" => protocols::atproto::register_theories(&mut registry),
        _ => {
            return Err(format!(
                "unknown protocol: {protocol_name:?}. Supported: atproto"
            ));
        }
    }
    Ok(registry)
}

/// Return the names of all built-in semantic protocols.
///
/// Programming language and data format protocols are handled by tree-sitter
/// grammars via `panproto-grammars`. This list covers only the domain-specific
/// semantic protocols that require custom theory composition.
pub(super) fn builtin_protocol_names() -> Vec<String> {
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
pub(super) fn lookup_builtin_protocol(name: &str) -> Option<panproto_core::schema::Protocol> {
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

/// Build a minimal `Schema` from a `CompiledMigration`'s surviving
/// vertex and edge sets. This is a fallback used when the full schema
/// is not available (e.g., when a bare `Resource::Migration` handle is
/// used instead of `Resource::MigrationWithSchemas`).
pub(super) fn build_minimal_schema(compiled: &CompiledMigration) -> panproto_core::schema::Schema {
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
pub(super) fn compose_compiled(
    c1: &CompiledMigration,
    c2: &CompiledMigration,
) -> CompiledMigration {
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

/// Build a default protocol spec with the given name.
///
/// Used as a fallback when the schema's protocol name does not match any
/// built-in protocol.
pub(super) fn default_protocol(name: &str) -> panproto_core::schema::Protocol {
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
pub(super) struct SchemaDiff {
    /// Vertices added in the second schema.
    pub(super) added_vertices: Vec<String>,
    /// Vertices removed from the first schema.
    pub(super) removed_vertices: Vec<String>,
    /// Edges added in the second schema.
    pub(super) added_edges: Vec<EdgeDiff>,
    /// Edges removed from the first schema.
    pub(super) removed_edges: Vec<EdgeDiff>,
    /// Vertices whose kind changed.
    pub(super) kind_changes: Vec<KindChange>,
}

/// A serializable edge for diffs.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct EdgeDiff {
    /// Source vertex ID.
    pub(super) src: String,
    /// Target vertex ID.
    pub(super) tgt: String,
    /// Edge kind.
    pub(super) kind: String,
    /// Optional edge name.
    pub(super) name: Option<String>,
}

/// A vertex kind change.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct KindChange {
    /// Vertex ID.
    pub(super) vertex: String,
    /// Old kind.
    pub(super) old_kind: String,
    /// New kind.
    pub(super) new_kind: String,
}

/// Compute a structural diff between two schemas.
pub(super) fn compute_diff(
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

/// Evaluate a GAT term recursively using a variable environment and theory.
///
/// Variables are looked up in the environment. Operations are evaluated
/// by recursively evaluating arguments, then looking up the operation in
/// the theory and applying it via a model.
pub(super) fn eval_term_recursive(
    term: &gat::Term,
    env: &[(String, gat::ModelValue)],
    theory: &gat::Theory,
) -> Result<gat::ModelValue, String> {
    match term {
        gat::Term::Var(name) => env
            .iter()
            .find(|(k, _)| k.as_str() == name.as_ref())
            .map(|(_, v)| v.clone())
            .ok_or_else(|| format!("unbound variable: {name}")),
        gat::Term::App { op, args } => {
            // Evaluate all arguments first.
            let mut evaluated_args = Vec::with_capacity(args.len());
            for arg in args {
                evaluated_args.push(eval_term_recursive(arg, env, theory)?);
            }

            // Look up the operation in the theory. For nullary constants,
            // return a string representation of the constant name.
            let operation = theory
                .find_op(op)
                .ok_or_else(|| format!("unknown operation: {op}"))?;

            // For nullary operations (constants), return the op name as a string value.
            if operation.inputs.is_empty() && args.is_empty() {
                return Ok(gat::ModelValue::Str(op.to_string()));
            }

            // For operations with arguments, build a structured result.
            // Without a concrete model, we represent the result as a map
            // containing the operation name and evaluated arguments.
            Ok(gat::ModelValue::Map({
                let mut map = rustc_hash::FxHashMap::default();
                map.insert("op".to_string(), gat::ModelValue::Str(op.to_string()));
                map.insert("args".to_string(), gat::ModelValue::List(evaluated_args));
                map.insert(
                    "output_sort".to_string(),
                    gat::ModelValue::Str(operation.output.to_string()),
                );
                map
            }))
        }
    }
}

/// Classify a protolens chain's optic kind based on complement constructors.
pub(super) fn classify_optic_kind(chain: &lens::ProtolensChain) -> &'static str {
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
        (true, true, _) => "affine",
        (_, _, true) if !has_added && !has_dropped => "traversal",
        _ => "affine",
    }
}

/// Recursively classify complement constructors.
pub(super) fn classify_complement(
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

// ── Expression parser and query engine (70-72) ─────────────────────

/// Parse source text into a panproto expression.
///
/// Tokenizes the input using the surface syntax lexer, then parses
/// the token stream into an `Expr` AST. Returns the expression as
/// `MessagePack` bytes.
///
/// # Errors
///
/// Returns `JsError` if tokenization or parsing fails.
#[wasm_bindgen]
pub fn parse_expr(source: &str) -> Result<Vec<u8>, JsError> {
    let tokens = panproto_expr_parser::tokenize(source).map_err(|e| WasmError::ParseFailed {
        reason: e.to_string(),
    })?;

    let expr = panproto_expr_parser::parse(&tokens).map_err(|errs| WasmError::ParseFailed {
        reason: errs
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; "),
    })?;

    rmp_serde::to_vec_named(&expr).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Evaluate a functional expression with a given environment.
///
/// The `expr_bytes` are `MessagePack`-encoded [`panproto_expr::Expr`].
/// The `env_bytes` are `MessagePack`-encoded `Vec<(String, panproto_expr::Literal)>`.
/// Returns the result as `MessagePack`-encoded [`panproto_expr::Literal`].
///
/// This evaluates expressions from the pure functional language (lambda
/// calculus with builtins), as opposed to `eval_expr` which evaluates
/// GAT terms against a theory.
///
/// # Errors
///
/// Returns `JsError` if deserialization fails or evaluation errors.
#[wasm_bindgen]
pub fn eval_func_expr(expr_bytes: &[u8], env_bytes: &[u8]) -> Result<Vec<u8>, JsError> {
    let expr: panproto_expr::Expr =
        rmp_serde::from_slice(expr_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("expr: {e}"),
        })?;

    let bindings: Vec<(String, panproto_expr::Literal)> = rmp_serde::from_slice(env_bytes)
        .map_err(|e| WasmError::DeserializationFailed {
            reason: format!("env: {e}"),
        })?;

    let env: panproto_expr::Env = bindings
        .into_iter()
        .map(|(k, v)| (std::sync::Arc::from(k.as_str()), v))
        .collect();

    let config = panproto_expr::EvalConfig::default();
    let result =
        panproto_expr::eval(&expr, &env, &config).map_err(|e| WasmError::ExprEvalFailed {
            reason: e.to_string(),
        })?;

    rmp_serde::to_vec_named(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Execute a declarative query against a W-type instance.
///
/// The `query_bytes` are `MessagePack`-encoded [`inst::InstanceQuery`].
/// The `instance_bytes` are `MessagePack`-encoded [`WInstance`].
/// The `schema_bytes` are `MessagePack`-encoded [`Schema`]. If empty,
/// a minimal placeholder schema is used (sufficient for queries that
/// do not require schema-aware operations).
/// Returns `MessagePack`-encoded query results as a list of match objects,
/// each containing `node_id`, `anchor`, `value`, and `fields`.
///
/// # Errors
///
/// Returns `JsError` if deserialization fails.
#[wasm_bindgen]
pub fn execute_query(
    query_bytes: &[u8],
    instance_bytes: &[u8],
    schema_bytes: &[u8],
) -> Result<Vec<u8>, JsError> {
    let query: inst::InstanceQuery =
        rmp_serde::from_slice(query_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("query: {e}"),
        })?;

    let instance: WInstance =
        rmp_serde::from_slice(instance_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("instance: {e}"),
        })?;

    let schema: Schema = if schema_bytes.is_empty() {
        SchemaBuilder::new(&schema::Protocol::default())
            .vertex("_", "record", None)
            .map_err(|e| WasmError::DeserializationFailed {
                reason: format!("placeholder schema: {e}"),
            })?
            .build()
            .map_err(|e| WasmError::DeserializationFailed {
                reason: format!("placeholder schema: {e}"),
            })?
    } else {
        rmp_serde::from_slice(schema_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("schema: {e}"),
        })?
    };

    let matches = inst::execute_query(&query, &instance, &schema);

    // Convert QueryMatch results to a serializable form.
    let results: Vec<serde_json::Value> = matches
        .into_iter()
        .map(|m| {
            let fields: serde_json::Map<String, serde_json::Value> = m
                .fields
                .into_iter()
                .map(|(k, v)| {
                    let json_v = serde_json::to_value(&v).unwrap_or(serde_json::Value::Null);
                    (k, json_v)
                })
                .collect();
            serde_json::json!({
                "node_id": m.node_id,
                "anchor": m.anchor.as_ref(),
                "value": serde_json::to_value(&m.value).unwrap_or(serde_json::Value::Null),
                "fields": fields,
            })
        })
        .collect();

    rmp_serde::to_vec_named(&results).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}
