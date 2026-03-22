//! # panproto-inst
//!
//! Instance representation for panproto (attributed C-sets).
//!
//! This crate provides three instance shapes, unified under [`Instance`]:
//!
//! - **[`WInstance`]**: Tree-shaped (W-type) instances with nodes,
//!   arcs, and optional hyper-edge fans.
//! - **[`FInstance`]**: Relational (set-valued functor) instances
//!   with tables and foreign keys.
//! - **[`GInstance`]**: Graph-shaped instances with nodes and edges
//!   (most general form, no root, cycles allowed).
//!
//! All three are attributed C-sets over different shape categories.
//! The [`Instance`] enum provides a unified interface.

// Allow concrete HashMap/HashSet in public API signatures per ENGINEERING.md spec.
#![allow(clippy::implicit_hasher)]

/// Generic attributed C-set trait unifying all instance shapes.
pub mod acset;
/// Error types for instance operations.
pub mod error;
/// Hyperedge fan representation.
pub mod fan;
/// Set-valued functor instance representation.
pub mod functor;
/// Graph-shaped instance representation.
pub mod ginstance;
/// Unified instance enum (attributed C-set).
pub mod instance;
/// Instance-aware expression evaluation (graph traversal builtins).
pub mod instance_env;
/// Metadata types for instance nodes.
pub mod metadata;
/// JSON parsing for W-type instances.
pub mod parse;
/// Right Kan extension (Pi_F) for instances.
pub mod pi;
/// Polynomial functor operations (fiber, group-by, join, section).
pub mod poly;
/// W-type instance representation and the `wtype_restrict` pipeline.
pub mod provenance;
/// Declarative query engine for W-type instances.
pub mod query;
/// Validation of W-type instances against schemas.
pub mod validate;
/// Value types and field presence.
pub mod value;
pub mod wtype;

// Re-exports for convenience.
pub use acset::AcsetOps;
pub use error::{InstError, ParseError, RestrictError, ValidationError};
pub use fan::Fan;
pub use functor::{FInstance, functor_extend, functor_restrict};
pub use ginstance::{GInstance, graph_extend, graph_restrict};
pub use instance::Instance;
pub use instance_env::eval_with_instance;
pub use metadata::Node;
pub use parse::{parse_json, to_json};
pub use pi::{functor_pi, wtype_pi};
pub use poly::{fiber_at_anchor, fiber_decomposition, fiber_with_predicate, group_by, join};
pub use provenance::{Provenance, ProvenanceMap, SourceField, TransformStep, compute_provenance};
pub use query::{InstanceQuery, QueryMatch, build_node_env, execute as execute_query};
pub use validate::validate_wtype;
pub use value::{FieldPresence, Value};
pub use wtype::{
    CaseBranch, CompiledMigration, FieldTransform, WInstance, ancestor_contraction,
    anchor_surviving, build_env_from_extra_fields, reachable_from_root, reconstruct_fans,
    resolve_edge, value_to_expr_literal, wtype_extend, wtype_restrict,
};
