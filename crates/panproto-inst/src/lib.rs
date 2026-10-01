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
/// The Sigma/Delta migration adjunction (unit, counit, transposes).
pub mod adjunction;
/// ACSet attribute discipline: derive and validate per-vertex attributes.
pub mod attributes;
/// The complement: data discarded by a forward projection.
pub mod complement;
/// Incremental contraction tracker for ancestor contraction.
pub mod contraction;
pub mod contraction_keys;
/// Edit errors for tree and table edits.
pub mod edit_error;
/// Operations on the category of elements `∫F` (Grothendieck construction).
pub mod element_ops;
/// Error types for instance operations.
pub mod error;
pub mod error_detail;
/// Hyperedge fan representation.
pub mod fan;
pub mod fan_violation;
/// Set-valued functor instance representation.
pub mod functor;
/// Graph-shaped instance representation.
pub mod ginstance;
pub mod graph_edit;
/// Internal hom schema construction and evaluation.
pub mod hom;
pub mod identity;
/// Unified instance enum (attributed C-set).
pub mod instance;
/// Instance-aware expression evaluation (graph traversal builtins).
pub mod instance_env;
/// First-class instance homomorphisms (morphisms of instances over a schema).
pub mod instance_hom;
pub mod locality;
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
/// Declarative query engine for instance presheaves.
pub mod query;
/// Incremental reachability index for W-type instances.
pub mod reachability;
pub mod segment;
/// Edit algebra for functor (table-shaped) instances.
pub mod table_edit;
/// Edit algebra for W-type (tree-shaped) instances.
pub mod tree_edit;
/// Validation of instances against schemas, re-exported from `validation`.
pub mod validate;
pub mod validation;
/// Value types and field presence.
pub mod value;
pub mod view_ref;
pub mod wtype;
pub mod wtype_check;

// Re-exports for convenience.
pub use acset::AcsetOps;
pub use adjunction::{
    AdjunctionError, f_counit, f_delta, f_delta_with_hom, f_sigma, f_transpose_left,
    f_transpose_right, f_unit, g_delta, w_counit, w_delta, w_sigma, w_transpose_left,
    w_transpose_right, w_unit,
};
pub use attributes::{AttributeDecl, AttributeSchema, AttributeViolation};
pub use complement::Complement;
pub use contraction::{ContractionRecord, ContractionTracker};
pub use edit_error::EditError;
pub use element_ops::ElementOps;
pub use error::{InstError, ParseError, RestrictError};
pub use error_detail::{ErrorDetail, PutDomainError};
pub use fan::Fan;
pub use fan_violation::FanViolation;
pub use functor::{FInstance, functor_extend, functor_restrict};
pub use functor::{
    ForeignKeyPair, KeyLookupError, Row, Table, canonical_finstance_bytes, write_finstance,
    write_pair_record, write_row_record,
};
pub use ginstance::{
    EdgeId, GEdge, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND, from_node_link_json,
    graph_extend, graph_restrict, reify_fans, to_node_link_json, unreify_fans,
};
pub use ginstance::{
    FanDomain, FanEmbedding, NotTreeReason, TreeEmbedding, TreeShapeReport,
    canonical_ginstance_bytes, reify_fans_owned, unreify_fans_owned, unreify_fans_with_schema,
    write_edge_record, write_ginstance, write_node_record,
};
pub use ginstance::{NotTableReason, TableEmbedding, TableShapeReport};
pub use graph_edit::{GraphDelta, GraphEdit};
pub use hom::{curry_migration, eval_hom, hom_schema};
pub use identity::{CanonicalSink, DigestHasher};
pub use identity::{
    ComplementShape, Digest, IdentityEnvelope, IdentityError, IdentityPolicy, IdentityStatus,
};
pub use instance::Instance;
pub use instance_env::{eval_with_element_ops, eval_with_instance};
pub use instance_hom::{FInstanceHom, HomError, WInstanceHom};
pub use metadata::{Node, NodeShape};
pub use panproto_schema::InstanceShapeKind;
pub use parse::{parse_json, to_json};
pub use pi::{functor_pi, functor_pi_with_hom, wtype_pi};
pub use poly::{
    SectionEnrichment, fiber_at_anchor, fiber_at_node, fiber_decomposition, fiber_with_predicate,
    group_by, join, restrict_with_complement, section,
};
pub use provenance::{Provenance, ProvenanceMap, SourceField, TransformStep, compute_provenance};
pub use query::{
    InstanceQuery, QueryMatch, build_node_env, execute as execute_query, execute_any,
    execute_elements, execute_functor, execute_graph,
};
pub use reachability::ReachabilityIndex;
pub use segment::{IdAllocator, IdRange, SegmentId};
pub use table_edit::TableEdit;
pub use tree_edit::TreeEdit;
pub use validation::{
    CheckClass, CheckCoverage, CheckInstance, CheckKey, CheckKind, CheckSpec, DeltaElementKind,
    Diagnostic, EndpointSide, ExternalCheckDecl, ExternalVerdict, ExternalVerdicts,
    IncompleteCheck, IncompleteReason, ReportScope, Requirement, SchemaRef, SchemaSourceMap,
    SourceLocation, TriggerTable, ValidationBudget, ValidationOptions, ValidationPlan,
    ValidationReport, Violation, validate_graph, validate_table, validate_wtype,
};
pub use value::{FidelityReason, ValueFidelityError, ValueSite};
pub use value::{FieldPresence, Value};
pub use view_ref::{IdFloor, RowId, SynthIndex, SynthKey, ViewRef};
pub use wtype::{
    CaseBranch, CompiledMigration, FanResolver, FanShape, FieldTransform, HyperResolverEntry,
    HyperResolverTable, TermAssignment, TermBranch, TermScope, TransformContext, WInstance,
    ancestor_contraction, anchor_surviving, apply_term_assignments_to_row,
    build_env_from_extra_fields, build_env_with_children, canonical_label_shape,
    collect_child_values, collect_scalar_child_values, expr_literal_to_value,
    extend_env_from_extra_fields, node_to_value, reconstruct_fans, referenced_names, resolve_edge,
    value_to_expr_literal, wtype_extend, wtype_extend_partial, wtype_restrict,
};
