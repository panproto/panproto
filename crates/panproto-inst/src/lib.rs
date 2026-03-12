//! # panproto-inst
//!
//! Instance representation for panproto.
//!
//! This crate provides two models for concrete data instances that
//! conform to schemas defined via `panproto-schema`:
//!
//! - **[`WInstance`]**: Tree-shaped (W-type) instances with nodes,
//!   arcs, and optional hyper-edge fans.
//! - **[`FInstance`]**: Relational (set-valued functor) instances
//!   with tables and foreign keys.
//!
//! Key operations:
//! - **[`wtype_restrict`]**: 5-step pipeline for restricting W-type
//!   instances along a migration mapping.
//! - **[`functor_restrict`]**: Precomposition (`Delta_F`) for functor
//!   instances.
//! - **[`parse_json`]** / **[`to_json`]**: Schema-guided JSON
//!   serialization round-trip.
//! - **[`validate_wtype`]**: Axiom checking (I1-I7) for W-type
//!   instances.

// Allow concrete HashMap/HashSet in public API signatures per ENGINEERING.md spec.
#![allow(clippy::implicit_hasher)]
/// Error types for instance operations.
pub mod error;
/// Hyperedge fan representation.
pub mod fan;
/// Set-valued functor instance representation.
pub mod functor;
/// Metadata types for W-type instance nodes.
pub mod metadata;
/// JSON parsing for W-type instances.
pub mod parse;
/// Validation of W-type instances against schemas.
pub mod validate;
/// Value types and field presence for W-type instances.
pub mod value;
/// W-type instance representation and the `wtype_restrict` pipeline.
pub mod wtype;

// Re-exports for convenience.
pub use error::{InstError, ParseError, RestrictError, ValidationError};
pub use fan::Fan;
pub use functor::{FInstance, functor_extend, functor_restrict};
pub use metadata::Node;
pub use parse::{parse_json, to_json};
pub use validate::validate_wtype;
pub use value::{FieldPresence, Value};
pub use wtype::{
    CompiledMigration, WInstance, ancestor_contraction, anchor_surviving, reachable_from_root,
    reconstruct_fans, resolve_edge, wtype_restrict,
};
