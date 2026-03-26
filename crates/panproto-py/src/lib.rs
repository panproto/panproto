//! # panproto-py
//!
//! Native Python bindings for panproto via `PyO3`.
//!
//! This crate compiles to a `cdylib` that maturin packages as
//! `panproto._native`. The pure-Python layer in `sdk/python/src/panproto/`
//! re-exports these types under the `panproto` namespace.
//!
//! ## Architecture
//!
//! Unlike the WASM bindings (`panproto-wasm`), which use a thread-local
//! slab allocator with opaque `u32` handles and `MessagePack` IPC, the `PyO3`
//! bindings use `#[pyclass]` structs that own (or `Arc`-share) the
//! underlying Rust data directly. Python's garbage collector manages
//! lifetimes; no explicit `free_handle()` is needed.
//!
//! Data crosses the boundary via `pythonize` (serde ↔ Python dicts)
//! instead of `MessagePack`, eliminating the encode/decode overhead.

mod check;
mod convert;
mod error;
mod expr;
mod gat;
mod git;
mod inst;
mod io;
mod lens;
mod mig;
mod parse;
mod project;
mod protocols;
mod schema;
mod vcs;

use pyo3::prelude::*;

/// The native extension module for panproto.
///
/// Registered as `panproto._native` by maturin.
#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Exception hierarchy
    error::register(m)?;

    // Schema types: Protocol, Schema, SchemaBuilder, Vertex, Edge, etc.
    schema::register(m)?;

    // Protocol registry: list_builtin_protocols, get_builtin_protocol, define_protocol
    protocols::register(m)?;

    // Migration: Migration, MigrationBuilder, CompiledMigration, compile, check_existence
    mig::register(m)?;

    // Check: SchemaDiff, CompatReport, diff_schemas, diff_and_classify
    check::register(m)?;

    // Instance: Instance (W-type)
    inst::register(m)?;

    // I/O: IoRegistry (77 protocol codecs)
    io::register(m)?;

    // Lens: Lens, auto_generate_lens, classify_transform
    lens::register(m)?;

    // GAT: Theory, Model, create_theory, colimit, check_morphism, migrate_model
    gat::register(m)?;

    // Expressions: Expr, parse_expr, eval_with_instance
    expr::register(m)?;

    // VCS: VcsRepository
    vcs::register(m)?;

    // Parse: AstParserRegistry, parse_source_file
    parse::register(m)?;

    // Project: ProjectBuilder, ProjectSchema, build_project, parse_project
    project::register(m)?;

    // Git: GitImportResult, git_import
    git::register(m)?;

    Ok(())
}
