//! The `#[wasm_bindgen]` entry points for panproto-wasm.
//!
//! Each public function takes handles (`u32`) and/or `MessagePack` byte
//! slices, performs the requested operation, and returns either a handle
//! or serialized bytes. All errors are converted to `JsError`.
//!
//! The entry points are grouped into domain submodules; this module is a
//! facade that re-exports their public surface and owns the shared
//! [`BuildOp`] type used for schema construction.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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

mod data;
mod enriched;
mod gat;
mod graph;
mod helpers;
mod instance;
mod lens;
mod registry;
mod schema;
mod vcs;

pub use data::*;
pub use enriched::*;
pub use gat::*;
pub use graph::*;
pub use instance::*;
pub use lens::*;
pub use registry::*;
pub use schema::*;
pub use vcs::*;
// A few `#[wasm_bindgen]` entry points (expression parser, query engine)
// live in the internal helpers module; re-export them explicitly rather
// than leaking every `pub(super)` helper.
pub use helpers::{eval_func_expr, execute_query, parse_expr};

/// Shared fixtures for the per-module native smoke tests.
///
/// The `#[wasm_bindgen]` entry points are plain Rust behind the
/// attribute, so they run on the host under `cargo test`. On the host,
/// constructing a `JsError` aborts (wasm-bindgen has no host runtime),
/// so these fixtures build inputs that drive the *happy path* — every
/// smoke test asserts an `Ok` / decodable result rather than exercising
/// an error branch (error branches are covered under
/// `#[cfg(target_arch = "wasm32")]` where a real JS runtime exists).
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub mod test_support {
    use std::sync::Arc;

    use panproto_core::schema::{Protocol, Schema, SchemaBuilder};

    use super::BuildOp;
    use crate::slab::{self, Resource};

    /// An open protocol carrying the object kinds the fixtures use.
    pub fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            obj_kinds: vec!["object".into(), "string".into(), "record".into()],
            ..Protocol::default()
        }
    }

    /// `post` record with `text` and `subtitle` string fields.
    pub fn source_schema() -> Schema {
        let proto = test_protocol();
        SchemaBuilder::new(&proto)
            .entry("post")
            .vertex("post", "record", Some("app.test.post"))
            .unwrap()
            .vertex("post.text", "string", None)
            .unwrap()
            .vertex("post.subtitle", "string", None)
            .unwrap()
            .edge("post", "post.text", "prop", Some("text"))
            .unwrap()
            .edge("post", "post.subtitle", "prop", Some("subtitle"))
            .unwrap()
            .build()
            .unwrap()
    }

    /// `post` record with only the `text` field.
    pub fn target_schema() -> Schema {
        let proto = test_protocol();
        SchemaBuilder::new(&proto)
            .entry("post")
            .vertex("post", "record", Some("app.test.post"))
            .unwrap()
            .vertex("post.text", "string", None)
            .unwrap()
            .edge("post", "post.text", "prop", Some("text"))
            .unwrap()
            .build()
            .unwrap()
    }

    /// MessagePack-encoded `Protocol` for [`super::define_protocol`].
    pub fn protocol_msgpack() -> Vec<u8> {
        rmp_serde::to_vec_named(&test_protocol()).unwrap()
    }

    /// MessagePack-encoded `Vec<BuildOp>` that builds [`target_schema`]
    /// for [`super::build_schema`].
    pub fn build_ops_msgpack() -> Vec<u8> {
        let ops = vec![
            BuildOp::Vertex {
                id: "post".to_owned(),
                kind: "record".to_owned(),
                nsid: Some("app.test.post".to_owned()),
            },
            BuildOp::Vertex {
                id: "post.text".to_owned(),
                kind: "string".to_owned(),
                nsid: None,
            },
            BuildOp::Edge {
                src: "post".to_owned(),
                tgt: "post.text".to_owned(),
                kind: "prop".to_owned(),
                name: Some("text".to_owned()),
            },
        ];
        rmp_serde::to_vec_named(&ops).unwrap()
    }

    /// Allocate a slab handle for a schema.
    pub fn schema_handle(schema: &Schema) -> u32 {
        slab::alloc(Resource::Schema(Arc::new(schema.clone())))
    }

    /// Allocate a slab handle for the test protocol.
    pub fn protocol_handle() -> u32 {
        slab::alloc(Resource::Protocol(test_protocol()))
    }
}

/// Linkage/coverage guard for the `#[wasm_bindgen]` entry points.
///
/// Each exported entry point is named here as a function pointer. The
/// module fails to compile if an export is renamed or removed, and the
/// grouping documents which module owns each smoke test. When adding a
/// new `#[wasm_bindgen] pub fn`, add it to the matching group and give it
/// a happy-path smoke test in that module's `#[cfg(test)] mod tests`.
#[cfg(test)]
#[allow(clippy::type_complexity)]
mod export_guard {
    /// Reference every exported entry point so a removed/renamed export
    /// is a compile error. `_ = f as usize` coerces each to a function
    /// pointer without calling it.
    #[test]
    fn every_export_is_linked() {
        use super::*;

        // schema.rs
        let _ = define_protocol as *const () as usize;
        let _ = build_schema as *const () as usize;
        let _ = parse_atproto_lexicon as *const () as usize;
        let _ = parse_schema_bundle as *const () as usize;
        let _ = list_bundle_parser_protocols as *const () as usize;
        let _ = schema_metadata as *const () as usize;
        let _ = check_existence as *const () as usize;
        let _ = compile_migration as *const () as usize;
        let _ = lift_record as *const () as usize;
        let _ = get_record as *const () as usize;
        let _ = put_record as *const () as usize;
        let _ = lift_json as *const () as usize;
        let _ = get_json as *const () as usize;
        let _ = put_json as *const () as usize;
        let _ = compose_migrations as *const () as usize;
        let _ = diff_schemas as *const () as usize;
        let _ = diff_schemas_full as *const () as usize;
        let _ = classify_diff as *const () as usize;
        let _ = report_text as *const () as usize;
        let _ = report_json as *const () as usize;
        let _ = normalize_schema as *const () as usize;
        let _ = validate_schema as *const () as usize;

        // instance.rs
        let _ = register_io_protocols as *const () as usize;
        let _ = list_io_protocols as *const () as usize;
        let _ = parse_instance as *const () as usize;
        let _ = emit_instance as *const () as usize;
        #[cfg(feature = "format-preserving")]
        {
            let _ = parse_instance_preserving as *const () as usize;
            let _ = emit_instance_preserving as *const () as usize;
        }
        let _ = validate_instance as *const () as usize;
        let _ = instance_to_json as *const () as usize;
        let _ = json_to_instance as *const () as usize;
        let _ = json_to_instance_with_root as *const () as usize;
        let _ = instance_element_count as *const () as usize;

        // data.rs
        let _ = store_dataset as *const () as usize;
        let _ = get_dataset as *const () as usize;
        let _ = migrate_dataset_forward as *const () as usize;
        let _ = migrate_dataset_backward as *const () as usize;
        let _ = check_dataset_staleness as *const () as usize;
        let _ = store_protocol_definition as *const () as usize;
        let _ = get_protocol_definition as *const () as usize;
        let _ = get_migration_complement as *const () as usize;
        let _ = free_handle as *const () as usize;

        // gat.rs
        let _ = create_theory as *const () as usize;
        let _ = colimit_theories as *const () as usize;
        let _ = check_morphism as *const () as usize;
        let _ = migrate_model as *const () as usize;

        // graph.rs
        let _ = fiber_at as *const () as usize;
        let _ = fiber_decomposition_wasm as *const () as usize;
        let _ = poly_hom as *const () as usize;
        let _ = preferred_conversion_path as *const () as usize;
        let _ = conversion_distance as *const () as usize;

        // enriched.rs
        let _ = eval_expr as *const () as usize;
        let _ = check_expr as *const () as usize;
        let _ = substitute_expr as *const () as usize;
        let _ = schema_add_coercion as *const () as usize;
        let _ = schema_add_default as *const () as usize;
        let _ = schema_add_merger as *const () as usize;
        let _ = schema_add_policy as *const () as usize;
        let _ = migration_coverage as *const () as usize;
        let _ = protolens_optic_kind as *const () as usize;
        let _ = protolens_simplify as *const () as usize;
        let _ = refinement_subsort as *const () as usize;

        // registry.rs
        let _ = list_builtin_protocols as *const () as usize;
        let _ = get_builtin_protocol as *const () as usize;

        // lens.rs (spot-check representative exports)
        let _ = auto_generate_protolens as *const () as usize;
        let _ = compile_lens_document as *const () as usize;
        let _ = compile_lens_document_with_refs as *const () as usize;
        let _ = protolens_field_transforms as *const () as usize;
        let _ = protolens_from_json as *const () as usize;

        // helpers (re-exported)
        let _ = parse_expr as *const () as usize;
        let _ = eval_func_expr as *const () as usize;
        let _ = execute_query as *const () as usize;
    }
}
