//! # panproto-parse
//!
//! Tree-sitter full-AST parsers and emitters for panproto language protocols.
//!
//! This crate provides the schema-level presentation functors that map between
//! source code text and panproto [`Schema`] models. It operates at the schema
//! level of panproto's two-parameter architecture (complementing `panproto-io`
//! which operates at the instance level).
//!
//! ## Theory extraction
//!
//! Tree-sitter grammars are theory presentations. Each grammar's `node-types.json`
//! is structurally isomorphic to a GAT: named node types become sorts, fields
//! become operations. The [`theory_extract`] module auto-derives panproto theories
//! from grammar metadata, ensuring the theory is always in sync with the parser.
//!
//! ## Generic walker
//!
//! Because theories are auto-derived from the grammar, the AST walker is fully
//! generic: one [`walker::AstWalker`] implementation works for all languages.
//! The node's `kind()` IS the panproto vertex kind; the field name IS the edge
//! kind. Per-language customization is limited to formatting constraints and
//! import resolution hints.
//!
//! ## Pipeline
//!
//! ```text
//! source code ──parse──→ Schema ──merge/diff/check──→ Schema ──emit──→ source code
//! ```
//!
//! [`Schema`]: panproto_schema::Schema

/// Error types for full-AST parsing and emission.
pub mod error;

/// Scope-aware vertex ID generation for full-AST schemas.
pub mod id_scheme;

/// Automated theory extraction from tree-sitter grammar metadata.
pub mod theory_extract;

/// Generic tree-sitter AST walker.
pub mod walker;

/// Per-language parser and emitter implementations.
pub mod languages;

/// Parser registry mapping protocol names to implementations.
pub mod registry;

pub use error::ParseError;
pub use id_scheme::IdGenerator;
pub use registry::{AstParser, ParserRegistry};
pub use theory_extract::{ExtractedTheoryMeta, extract_theory_from_node_types};
pub use walker::{AstWalker, WalkerConfig};
