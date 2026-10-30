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
pub mod contraction_plan;
pub mod delta_convert;
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
pub mod graph_complement;
pub mod graph_edit;
/// Relational Sigma, Delta, and Pi for graph-shaped instances.
pub mod graph_kan;
pub mod graph_restrict;
/// Internal hom schema construction and evaluation.
pub mod hom;
pub mod identity;
/// Unified instance enum (attributed C-set).
pub mod instance;
/// Instance-aware expression evaluation (graph traversal builtins).
pub mod instance_env;
/// First-class instance homomorphisms (morphisms of instances over a schema).
pub mod instance_hom;
/// Declared key equality, ordering, and declaration predicates.
pub mod key;
pub mod locality;
/// Metadata types for instance nodes.
pub mod metadata;
/// The functor a compiled migration states between relation-ified schemas.
pub mod migration_functor;
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
/// The relation-ified schema of a table schema.
pub mod rel_schema;
/// Serializable carriers for reference-resolution lenses.
pub mod resolution_complement;
pub mod restrict_boundary;
pub mod restrict_plan;
pub mod segment;
pub mod table_complement;
pub mod table_delta;
/// Edit algebra for functor (table-shaped) instances.
pub mod table_edit;
pub mod table_restrict;
/// Edit algebra for W-type (tree-shaped) instances.
pub mod tree_edit;
/// Validation of W-type instances against schemas.
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
    AdjunctionError, f_counit, f_delta, f_delta_with_provenance, f_sigma, f_transpose_left,
    f_transpose_right, f_unit, w_counit, w_delta, w_sigma, w_transpose_left, w_transpose_right,
    w_unit,
};
pub use adjunction::{
    DeltaProvenance, NonConformance, SigmaProvenance, f_restrict_to_domain, f_sigma_restricted,
};
pub use adjunction::{
    f_delta_on_hom, f_pi_counit, f_pi_on_hom, f_pi_transpose_left, f_pi_transpose_right, f_pi_unit,
    f_sigma_on_hom, f_sigma_with_provenance,
};
pub use attributes::{AttributeDecl, AttributeSchema, AttributeViolation};
pub use complement::Complement;
pub use contraction::{ContractionRecord, ContractionTracker};
pub use contraction_plan::{
    ContractionMode, ContractionPlan, PairResolution, PlannedPair, UnresolvedReason, resolve_pair,
};
pub use edit_error::EditError;
pub use element_ops::ElementOps;
pub use error::{InstError, ParseError, RestrictError};
pub use error_detail::{ErrorDetail, KeyRefusal, PredicateRefusal, PutDomainError};
pub use fan::Fan;
pub use fan_violation::FanViolation;
pub use functor::FInstance;
pub use functor::{
    ForeignKeyPair, KeyLookupError, Row, Table, canonical_finstance_bytes, write_finstance,
    write_pair_record, write_row_record,
};
pub use ginstance::{
    EdgeId, GEdge, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND, from_node_link_json, reify_fans,
    to_node_link_json, unreify_fans,
};
pub use ginstance::{
    FanDomain, FanEmbedding, NotTreeReason, TreeEmbedding, TreeShapeReport,
    canonical_ginstance_bytes, reify_fans_owned, unreify_fans_owned, unreify_fans_with_schema,
    write_edge_record, write_ginstance, write_node_record,
};
pub use ginstance::{NotTableReason, TableEmbedding, TableShapeReport};
pub use graph_complement::GraphComplement;
pub use graph_edit::{GraphDelta, GraphEdit};
pub use graph_kan::{
    CellLayout, EdgeLayout, EncodingSide, GraphDeltaProvenance, GraphKanError, GraphKanHom,
    GraphLayout, GraphPiProvenance, GraphSigmaProvenance, NotEncodableReason, decode_graph,
    encode_graph, g_counit, g_delta, g_delta_along, g_delta_along_with_provenance,
    g_delta_with_provenance, g_pi, g_pi_along, g_pi_along_with_provenance, g_pi_counit,
    g_pi_restricted, g_pi_restricted_with_provenance, g_pi_transpose_left, g_pi_transpose_right,
    g_pi_unit, g_pi_with_provenance, g_sigma, g_sigma_along, g_sigma_along_with_provenance,
    g_sigma_restricted, g_sigma_restricted_with_provenance, g_sigma_with_provenance,
    g_transpose_left, g_transpose_right, g_unit,
};
pub use graph_restrict::{
    graph_extend, graph_extend_engine, graph_extend_partial, graph_extend_validated,
    graph_restrict, graph_restrict_validated, graph_restrict_with_complement,
    graph_restrict_with_plan,
};
pub use hom::{curry_migration, eval_hom, hom_schema};
pub use identity::{
    CanonicalSink, DigestHasher, RESOLUTION_CONTEXT, RESOLUTION_EXTERNAL_TEXT_CONTEXT,
    RESOLUTION_OCCURRENCE_CONTEXT, RESOLUTION_SPEC_CONTEXT, RESOLUTION_TOKEN_BOUNDARIES_CONTEXT,
    resolution_context_digest, resolution_digest, resolution_external_text_digest,
    resolution_identity, resolution_spec_digest, resolution_token_boundaries_digest,
};
pub use identity::{
    ComplementShape, Digest, IdentityEnvelope, IdentityError, IdentityPolicy, IdentityStatus,
};
pub use instance::Instance;
pub use instance_env::{eval_with_element_ops, eval_with_instance};
pub use instance_hom::{FInstanceHom, GInstanceHom, HomError, WInstanceHom};
pub use key::KeyingError;
pub use metadata::{Node, NodeShape};
pub use migration_functor::{EntityFunctor, MigrationFunctor};
pub use panproto_schema::InstanceShapeKind;
pub use parse::{parse_json, to_json};
pub use pi::{PiProvenance, functor_pi, functor_pi_restricted, wtype_pi};
pub use pi::{functor_pi_along, functor_pi_along_with_provenance};
pub use poly::{
    SectionEnrichment, fiber_at_anchor, fiber_at_node, fiber_decomposition, fiber_with_predicate,
    group_by, join, restrict_with_complement, section,
};
pub use provenance::{
    Provenance, ProvenanceMap, SourceField, TransformStep, compute_provenance, graph_fiber_at_node,
    graph_group_by, graph_provenance,
};
pub use query::{
    InstanceQuery, QueryError, QueryMatch, build_node_env, execute as execute_query, execute_any,
    execute_elements, execute_functor, execute_graph,
};
pub use reachability::{
    ReachabilityIndex, graph_components, reachable_from, strongly_connected_components,
};
pub use rel_schema::{RelSchema, RelSchemaError};
pub use resolution_complement::{
    AnyInstance, AssemblyPlanId, AssemblyResidue, AssemblyTrace, DefinitionKey, RedirectedArc,
    RedirectedAttachment, RedirectedFragment, RefOccurrence, ResolutionComplement,
    ResolutionContextKind, ResolutionKind, ResolutionProviderId, ResolutionSpan, ResolutionSpec,
    ResolutionTextId, ResolutionValueSite, UnresolvedReason as ResolutionUnresolvedReason,
    UnresolvedRef,
};
pub use restrict_boundary::{BoundaryProtocols, BoundarySide};
pub use restrict_plan::{
    BoundaryPlan, CellFate, CellPlan, EdgeFate, EdgeImage, RestrictPlan, SurvivalElement,
};
pub use segment::{IdAllocator, IdRange, SegmentId};
pub use table_complement::{DroppedRow, TableComplement};
pub use table_delta::{TableDelta, canonical_table_delta_bytes};
pub use table_edit::TableEdit;
pub use table_restrict::{
    functor_extend, functor_extend_engine, functor_extend_partial, functor_extend_validated,
    functor_restrict, functor_restrict_validated, functor_restrict_with_complement,
    functor_restrict_with_plan,
};
pub use tree_edit::{TreeEdit, canonical_tree_edit_bytes};
pub use validate::{validate_attributes, validate_wtype};
pub use validation::{
    AdmissionPolicy, BudgetCut, CheckClass, CheckCoverage, CheckInstance, CheckKey, CheckKind,
    CheckRun, CheckSpec, DeltaElementKind, Diagnostic, EditRejection, EditValidation, EditWitness,
    EndpointSide, ExternalCheckDecl, ExternalVerdict, ExternalVerdicts, FanRecord, Footprint,
    IncompleteCheck, IncompleteReason, KeyIndexState, OpenVerdict, ReportScope, Requirement,
    RunFinding, RunVerdict, SchemaRef, SchemaSourceMap, SourceLocation, StateField, StateMismatch,
    TriggerTable, Validated, ValidatedGraph, ValidatedShape, ValidatedTable, ValidatedTree,
    ValidationBudget, ValidationOptions, ValidationPlan, ValidationReport, ValidationState,
    Violation, WitnessMismatch, WitnessOptions, WitnessReplay, validate_graph, validate_table,
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
pub use wtype::{
    InstanceRef, NotExpressible, TreeSynthesis, tree_survival_verdicts, wtype_extend_engine,
    wtype_extend_validated, wtype_restrict_validated, wtype_restrict_with_plan,
    wtype_restrict_with_synthesis,
};
