//! # panproto-core
//!
//! Core re-export facade for panproto.
//!
//! This crate provides a single, convenient entry point for consumers
//! by re-exporting the public APIs of all panproto sub-crates.

/// Re-export of `panproto-check` for validation and axiom checking.
pub use panproto_check as check;
/// Re-export of `panproto-gat` for GAT (generalized algebraic theory) types.
pub use panproto_gat as gat;
/// Re-export of `panproto-inst` for instance representations.
pub use panproto_inst as inst;
/// Re-export of `panproto-io` for instance-level parse/emit across all protocols.
pub use panproto_io as io;
/// Re-export of `panproto-lens` for bidirectional lenses and protolenses.
pub use panproto_lens as lens;
/// Re-export of `panproto-mig` for migration and lifting operations.
pub use panproto_mig as mig;
/// Re-export of `panproto-protocols` for built-in protocol definitions.
pub use panproto_protocols as protocols;
/// Re-export of `panproto-schema` for schema types and builders.
pub use panproto_schema as schema;
/// Re-export of `panproto-vcs` for schematic version control.
pub use panproto_vcs as vcs;

// -- Feature-gated cospan-support crates --

/// Re-export of `panproto-parse` for full-AST tree-sitter parsing (10 languages).
#[cfg(feature = "full-parse")]
pub use panproto_parse as parse;

/// Re-export of `panproto-project` for multi-file project assembly via coproduct.
#[cfg(feature = "project")]
pub use panproto_project as project;

/// Re-export of `panproto-git` for bidirectional git ↔ panproto-vcs translation.
#[cfg(feature = "git")]
pub use panproto_git as git;

/// Re-export of `panproto-llvm` for LLVM IR protocol and lowering morphisms.
#[cfg(feature = "llvm")]
pub use panproto_llvm as llvm;

/// Re-export of `panproto-jit` for LLVM JIT compilation of expressions.
#[cfg(feature = "jit")]
pub use panproto_jit as jit;
