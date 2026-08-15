//! # panproto-mig
//!
//! Migration engine for panproto.
//!
//! This crate computes and applies schema migrations, transforming
//! instances from one schema version to another while preserving
//! data integrity through theory morphisms.
//!
//! The migration pipeline consists of:
//!
//! 1. **Existence checking** ([`check_existence`]): Theory-derived
//!    validation that a migration is well-formed. The conditions
//!    checked depend on the protocol's schema and instance theories.
//!
//! 2. **Compilation** ([`compile()`]): Pre-computes surviving vertex/edge
//!    sets and remapping tables for fast per-record application.
//!
//! 3. **Lifting** ([`lift_wtype`], [`lift_functor`]): Applies a compiled
//!    migration to concrete instances, delegating to `panproto-inst`'s
//!    restrict operations.
//!
//! 4. **Composition** ([`compose()`]): Composes two sequential migrations
//!    into a single equivalent migration.
//!
//! 5. **Inversion** ([`invert()`]): Checks if a migration is invertible
//!    (bijective) and constructs the inverse if so.

// Allow concrete HashMap/HashSet in public API signatures per ENGINEERING.md spec.
#![allow(clippy::implicit_hasher)]

pub mod align;
pub mod cascade;
pub mod chase;
pub mod coerce;
pub mod compile;
pub mod compose;
pub mod coverage;
pub mod error;
pub mod existence;
pub mod hom_search;
pub mod invert;
pub mod lift;
pub mod migration;
pub mod overlap;
pub mod quality;
pub mod schema_theory;
pub mod solve;
pub mod span;

pub use cascade::{induce_data_migration, induce_migration_from_theory, induce_schema_morphism};
pub use chase::{
    Atom, AtomTerm, ChaseBudget, ChaseError, ChaseOutcome, Dependency, EmbeddedDependency, chase,
    dependencies_from_schema, dependencies_from_theory, saturate_row_existence,
    term_dependencies_from_theory,
};
pub use compile::compile;
pub use compose::compose;
pub use compose::{
    ComposeReport, OnMissing, RelabelComposition, compose_relabeling, compose_with_report,
};
pub use coverage::{CoverageReport, PartialFailure, PartialReason, check_coverage};
pub use error::{ComposeError, ExistenceError, InvertError, LiftError, MigError, SpanError};
pub use existence::{ExistenceReport, check_existence};
pub use hom_search::{
    DomainConstraints, FoundMorphism, SearchOptions, find_best_morphism,
    find_best_morphism_constrained, find_morphisms, find_morphisms_constrained, find_span,
    find_span_constrained, without_bottom,
};
pub use invert::invert;
pub use lift::{
    lift_functor, lift_functor_pi, lift_functor_sigma, lift_wtype, lift_wtype_pi, lift_wtype_sigma,
};
pub use migration::Migration;
pub use overlap::discover_overlap;
pub use schema_theory::{check_migration_morphism, induced_theory_morphism, schema_to_theory};

pub use align::{
    AliasDict, Anchor, StrategyTag, alias_anchors, default_alias_dict, exact_anchors,
    token_anchors, token_similarity,
};

pub use quality::reference_quality;
pub use span::{DEFAULT_OPTIMA_CAP, EdgeImages, LegShape, SchemaSpan, SpanCertificate, SpanSearch};

pub use solve::build::{BuildError, Evidence, NoEvidence};
pub use solve::cfn::{
    Cfn, CfnBuilder, CfnError, CostFunction, Domain, DomainIter, Domains, Variable,
};
pub use solve::cost::{
    COST_SCALE, Cost, CostWeights, CostWeightsError, DEFAULT_WEIGHTS, DROP_UNIT,
    MAX_COVERAGE_RADIX, coverage_radix, quality_units,
};
pub use solve::{
    Assignment, IsoError, LimitKind, SearchBudget, SearchWarning, SolveOutcome, SolverPath, ValId,
    VarId,
};

pub use coerce::{
    SortLensWitness, WitnessLibrary, default_witness_library, witness_forward_fails_on,
    witness_satisfies_lens_laws,
};
