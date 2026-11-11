//! The generic text formats: YAML, TOML, CSV and TSV.
//!
//! These protocols name no application. Their files carry structured data
//! and no schema of their own, so each reads no schema documents
//! ([`Parser::Programmatic`](crate::registry::Parser::Programmatic)): a
//! schema for one is built in code, and the schema decides which vertex
//! and edge kinds the data uses, so the protocols constrain neither. What
//! each declares is its theories and its shapes.

/// The CSV protocol.
pub mod csv;
/// The TOML protocol.
pub mod toml;
/// The TSV protocol.
pub mod tsv;
/// The YAML protocol.
pub mod yaml;
