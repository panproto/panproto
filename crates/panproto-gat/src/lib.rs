//! # panproto-gat
//!
//! The GAT (Generalized Algebraic Theory) engine for panproto.
//!
//! This is Level 0 of the panproto architecture, the only component
//! implemented directly in Rust rather than as data. It provides:
//!
//! - **Sorts**: Types that may depend on terms of other sorts
//! - **Operations**: Term constructors with typed inputs and outputs
//! - **Equations**: Judgemental equalities between terms
//! - **Theories**: Named collections of sorts, operations, and equations
//! - **Theory morphisms**: Structure-preserving maps between theories
//! - **Colimits**: Pushouts of theories for composing schema languages
//! - **Pullbacks**: Intersections of theories for overlap discovery
//! - **Models**: Interpretations of theories in Set
//! - **Type-checking**: Verification that terms and equations are well-typed
//! - **Natural transformations**: Morphisms between theory morphisms
//! - **Free models**: Initial model construction from theories
//! - **Quotient theories**: Theory simplification by merging identified elements
//!
//! The mathematical foundations are based on Cartmell (1986) and
//! the `GATlab` architecture (Lynch et al., 2024).

pub mod alg_struct;
mod check_model;
mod colimit;
pub mod composition;
mod enrichment;
mod eq;
mod error;
mod factorize;
mod free_model;
mod ident;
mod model;
mod morphism;
mod nat_transform;
mod op;
mod pullback;
mod quotient;
pub mod refinement;
pub mod rewriting;
mod schema_functor;
mod sort;
mod theory;
mod typecheck;
pub mod witness;

pub use check_model::{
    CheckModelOptions, EquationViolation, check_model, check_model_with_options,
};
pub use colimit::{ColimitResult, colimit, colimit_by_name, pushout_by_name};
pub use composition::{CompositionSpec, CompositionStep, recompose};
pub use factorize::{Factorization, factorize, validate_factorization};

pub use enrichment::{
    Adjacency, EnrichmentKind, LayoutPolicySpec, LayoutRole, LayoutSpec, RuleLayout,
    is_interstitial_text_sort, is_layout_sort,
};
pub use eq::{
    CaseBranch, DirectedEquation, Equation, Term, alpha_equivalent, alpha_equivalent_equation,
    compose_subst, match_pattern, normalize, normalize_with_status, normalize_with_witness,
};
pub use error::GatError;
pub use free_model::{FreeModelConfig, FreeModelResult, free_model};
pub use ident::{Ident, Name, NameSite, ScopeTag, SiteRename};
pub use model::{Model, ModelValue, migrate_model};
pub use morphism::{OpAssignment, TheoryMorphism, check_morphism, check_morphism_with_witnesses};
pub use nat_transform::{
    NaturalTransformation, check_interchange, check_natural_transformation, horizontal_compose,
    vertical_compose,
};
pub use op::{Implicit, Operation};
pub use pullback::{PullbackResult, pullback};
pub use quotient::quotient;
pub use schema_functor::{TheoryConstraint, TheoryEndofunctor, TheoryTransform};
pub use sort::{
    CoercionClass, Sort, SortClosure, SortExpr, SortKind, SortParam, ValueKind,
    classify_builtin_coercion, positional_param_rename, signatures_equivalent_modulo_param_rename,
    sort_params_equivalent_modulo_rename,
};
pub use theory::{ConflictPolicy, ConflictStrategy, Theory, resolve_theory, th_editable_structure};
pub use typecheck::{
    HoleReport, SortScheme, VarContext, infer_var_sorts, typecheck_equation,
    typecheck_equation_modulo_rewrites, typecheck_term, typecheck_term_with_holes,
    typecheck_theory,
};

pub use alg_struct::{AlgStruct, StructField, StructParam};
pub use refinement::{RefinedSort, RefinementConstraint, RefinementError};
pub use rewriting::{
    ConfluenceReport, CriticalPair, OpPrecedence, RewriteSystemReport, RuleViolation,
    TerminationReport, check_local_confluence, check_termination_via_lpo, lpo_greater,
    validate_rewrite_system,
};
pub use witness::{EqWitness, WitnessJustification};
