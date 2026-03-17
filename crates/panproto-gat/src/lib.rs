//! # panproto-gat
//!
//! The GAT (Generalized Algebraic Theory) engine for panproto.
//!
//! This is Level 0 of the panproto architecture — the only component
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

mod check_model;
mod colimit;
mod eq;
mod error;
mod free_model;
mod ident;
mod model;
mod morphism;
mod nat_transform;
mod op;
mod pullback;
mod quotient;
mod sort;
mod theory;
mod typecheck;

pub use check_model::{
    CheckModelOptions, EquationViolation, check_model, check_model_with_options,
};
pub use colimit::colimit;
pub use eq::{Equation, Term};
pub use error::GatError;
pub use free_model::{FreeModelConfig, free_model};
pub use ident::{Ident, Name, NameSite, ScopeTag, SiteRename};
pub use model::{Model, ModelValue, migrate_model};
pub use morphism::{TheoryMorphism, check_morphism};
pub use nat_transform::{
    NaturalTransformation, check_natural_transformation, horizontal_compose, vertical_compose,
};
pub use op::Operation;
pub use pullback::{PullbackResult, pullback};
pub use quotient::quotient;
pub use sort::{Sort, SortParam};
pub use theory::{Theory, resolve_theory};
pub use typecheck::{
    VarContext, infer_var_sorts, typecheck_equation, typecheck_term, typecheck_theory,
};
