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
/// Re-export of `panproto-lens` for bidirectional lens combinators.
pub use panproto_lens as lens;
/// Re-export of `panproto-mig` for migration and lifting operations.
pub use panproto_mig as mig;
/// Re-export of `panproto-io` for instance-level parse/emit across all protocols.
pub use panproto_io as io;
/// Re-export of `panproto-protocols` for built-in protocol definitions.
pub use panproto_protocols as protocols;
/// Re-export of `panproto-schema` for schema types and builders.
pub use panproto_schema as schema;
/// Re-export of `panproto-vcs` for schematic version control.
pub use panproto_vcs as vcs;
