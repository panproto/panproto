//! Data schema protocol definitions.

/// BSON Schema protocol definition and parser/emitter.
pub mod bson;
/// CDDL (RFC 8610) protocol definition and parser/emitter.
pub mod cddl;
/// CSV/Table Schema (Frictionless Data) protocol definition and parser/emitter.
pub mod csv_table;
/// INI Schema protocol definition and parser/emitter.
pub mod ini_schema;
/// JSON Schema protocol definition and parser/emitter.
pub mod json_schema;
/// TOML Schema protocol definition and parser/emitter.
pub mod toml_schema;
/// YAML Schema protocol definition and parser/emitter.
pub mod yaml_schema;
