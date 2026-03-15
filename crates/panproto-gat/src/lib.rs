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
//! - **Models**: Interpretations of theories in Set
//!
//! The mathematical foundations are based on Cartmell (1986) and
//! the `GATlab` architecture (Lynch et al., 2024).

mod colimit;
mod eq;
mod error;
mod ident;
mod model;
mod morphism;
mod op;
mod sort;
mod theory;

pub use colimit::colimit;
pub use eq::{Equation, Term};
pub use error::GatError;
pub use ident::{Ident, Name, NameSite, ScopeTag, SiteRename};
pub use model::{Model, ModelValue, migrate_model};
pub use morphism::{TheoryMorphism, check_morphism};
pub use op::Operation;
pub use sort::{Sort, SortParam};
pub use theory::{Theory, resolve_theory};
