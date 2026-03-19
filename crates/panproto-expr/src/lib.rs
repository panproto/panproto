//! # panproto-expr
//!
//! A pure functional expression language for panproto enriched theories.
//!
//! This crate provides the computational substrate for schema transforms:
//! coercion functions, merge/split logic, default value computation,
//! and conflict resolution policies. Expressions are:
//!
//! - **Pure**: no IO, no mutable state, no randomness
//! - **Deterministic**: same inputs always produce the same output
//! - **Serializable**: the [`Expr`] enum derives `Serialize`/`Deserialize`
//! - **Platform-independent**: evaluates identically on native and WASM
//! - **Bounded**: step and depth limits prevent runaway computation
//!
//! The language is lambda calculus with pattern matching, records, lists,
//! and ~50 built-in operations on strings, numbers, and collections.

mod builtin;
mod env;
mod error;
mod eval;
mod expr;
mod literal;
mod subst;

pub use builtin::apply_builtin;
pub use env::Env;
pub use error::ExprError;
pub use eval::{EvalConfig, eval};
pub use expr::{BuiltinOp, Expr, Pattern};
pub use literal::Literal;
pub use subst::{free_vars, pattern_vars, substitute};
