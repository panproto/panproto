//! Language parser infrastructure.
//!
//! The shared `LanguageParser` from `common` handles all tree-sitter grammars
//! uniformly: the node kind IS the vertex kind, the field name IS the edge kind.
//! Grammar sources and `Language` objects come from `panproto-grammars`.
//!
//! Per-language `WalkerConfig` overrides (extra scope/block kinds) are stored
//! in `walker_configs`. Languages without overrides use the default config.

/// Shared language parser implementation.
pub mod common;

/// Per-language `WalkerConfig` overrides.
pub mod walker_configs;
