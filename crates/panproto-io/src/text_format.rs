//! Instance codecs for generic text serialization formats.
//!
//! YAML, TOML, and CSV are format families in their own right: they carry
//! structured instance data yet, unlike the domain protocols, name no
//! single application. Registering them here gives the format-preserving
//! [`UnifiedCodec`](crate::unified_codec::UnifiedCodec) a home in
//! [`default_registry`](crate::default_registry), so a lossless,
//! byte-preserving round-trip is reachable by protocol name (`yaml`,
//! `toml`, `csv`).
//!
//! This module exists only under the `tree-sitter` feature: format
//! preservation is a property of the CST-backed `UnifiedCodec`, and the
//! formats have no canonical (non-tree-sitter) codec of their own. The
//! default build therefore omits these protocols rather than registering
//! a JSON-shaped stand-in that could not parse YAML, TOML, or CSV.

use crate::registry::ProtocolRegistry;
use crate::unified_codec::UnifiedCodec;

/// Register the format-preserving generic text-format codecs (`yaml`,
/// `toml`, `csv`) with the registry.
///
/// Each routes through the CST-backed [`UnifiedCodec`], so a
/// `parse → emit` round-trip through the registry reproduces the input
/// byte for byte. Registration is skipped for any format whose
/// tree-sitter grammar is not compiled in (see
/// [`ProtocolRegistry::register_optional`]).
pub fn register_all(registry: &mut ProtocolRegistry) {
    registry.register_optional(UnifiedCodec::yaml("yaml"));
    registry.register_optional(UnifiedCodec::toml("toml"));
    registry.register_optional(UnifiedCodec::csv("csv"));
}
