//! # panproto-core
//!
//! Core re-export facade for panproto.
//!
//! This crate is the single dependency a downstream Rust user needs. It
//! re-exports the public APIs of every panproto library crate, so consumers
//! reach the full stack through `panproto_core` alone rather than depending on
//! member crates individually.
//!
//! The always-on library crates re-exported here are `panproto-check`,
//! `panproto-gat`, `panproto-inst`, `panproto-io`, `panproto-lens`,
//! `panproto-mig`, `panproto-protocols`, `panproto-schema`, and
//! `panproto-vcs`, together with the expression and DSL crates
//! `panproto-expr`, `panproto-expr-parser`, `panproto-lens-dsl`, and
//! `panproto-theory-dsl`. The support crates `panproto-parse`,
//! `panproto-project`, and `panproto-git` are re-exported behind cargo
//! features. The binding crates `panproto-cli`, `panproto-c`,
//! `panproto-wasm`, and `panproto-py` depend on `panproto-core` alone for
//! this surface.

/// Re-export of `panproto-check` for validation and axiom checking.
pub use panproto_check as check;
/// Re-export of `panproto-expr` for the expression-language values and evaluator.
pub use panproto_expr as expr;
/// Re-export of `panproto-expr-parser` for tokenizing, parsing, and pretty-printing expressions.
pub use panproto_expr_parser as expr_parser;
/// Re-export of `panproto-gat` for GAT (generalized algebraic theory) types.
pub use panproto_gat as gat;
/// Re-export of `panproto-inst` for instance representations.
pub use panproto_inst as inst;
/// Re-export of `panproto-io` for instance-level parse/emit across all protocols.
pub use panproto_io as io;
/// Re-export of `panproto-lens` for bidirectional lenses and protolenses.
pub use panproto_lens as lens;
/// Re-export of `panproto-lens-dsl` for the declarative lens specification front-end.
pub use panproto_lens_dsl as lens_dsl;
/// Re-export of `panproto-mig` for migration and lifting operations.
pub use panproto_mig as mig;
/// Re-export of `panproto-protocols` for built-in protocol definitions.
pub use panproto_protocols as protocols;
/// Re-export of `panproto-schema` for schema types and builders.
pub use panproto_schema as schema;
/// Re-export of `panproto-theory-dsl` for the declarative theory and protocol specification front-end.
pub use panproto_theory_dsl as theory_dsl;
/// Re-export of `panproto-vcs` for schematic version control.
pub use panproto_vcs as vcs;

// -- Feature-gated support crates --

/// Re-export of `panproto-parse` for full-AST tree-sitter parsing (10 languages).
#[cfg(feature = "full-parse")]
pub use panproto_parse as parse;

/// Re-export of `panproto-project` for multi-file project assembly via coproduct.
#[cfg(feature = "project")]
pub use panproto_project as project;

/// Re-export of `panproto-git` for bidirectional git ↔ panproto-vcs translation.
#[cfg(feature = "git")]
pub use panproto_git as git;
