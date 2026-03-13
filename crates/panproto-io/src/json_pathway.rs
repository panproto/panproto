//! Shared SIMD-accelerated JSON pathway for schema-guided instance parsing.
//!
//! This module provides the core JSON → `WInstance` pipeline used by ~30
//! protocols whose instance data is JSON (or JSON-like: YAML, TOML, BSON).
//! It wraps [`panproto_inst::parse_json`] with SIMD-accelerated pre-parsing
//! via `simd-json` for throughput-critical paths, and a `serde_json` fallback
//! for correctness-critical paths.
//!
//! ## Performance
//!
//! The SIMD pathway uses `simd-json` to parse raw bytes into a
//! `serde_json::Value` (via the serde compatibility layer), then delegates
//! to the existing schema-guided walker in `panproto-inst`. This gives us
//! 2-4x speedup on the JSON parsing step while reusing the proven walker.

use panproto_inst::{WInstance, parse_json, to_json};
use panproto_schema::Schema;

use crate::error::{EmitInstanceError, ParseInstanceError};

/// Parse JSON bytes into a `WInstance` using SIMD-accelerated parsing.
///
/// The `root_vertex` identifies which schema vertex anchors the root of
/// the instance tree. The parser uses `simd-json` for structural scanning
/// of the raw bytes, then delegates to [`panproto_inst::parse_json`] for
/// schema-guided tree construction.
///
/// # Errors
///
/// Returns [`ParseInstanceError::Json`] if the bytes are not valid JSON,
/// or [`ParseInstanceError::SchemaMismatch`] if the JSON structure does
/// not match the schema.
pub fn parse_json_bytes(
    schema: &Schema,
    root_vertex: &str,
    input: &[u8],
    protocol: &str,
) -> Result<WInstance, ParseInstanceError> {
    // SIMD pathway: parse bytes into serde_json::Value via simd-json.
    // simd-json requires a mutable buffer for in-place parsing.
    let mut buf = input.to_vec();
    let json_val: serde_json::Value = simd_json::serde::from_slice(&mut buf).map_err(|e| {
        ParseInstanceError::Json(format!("{e}"))
    })?;

    parse_json(schema, root_vertex, &json_val).map_err(|e| ParseInstanceError::Parse {
        protocol: protocol.to_string(),
        message: e.to_string(),
    })
}

/// Parse a `serde_json::Value` into a `WInstance` (non-SIMD path).
///
/// Used when the JSON is already parsed (e.g., from YAML or TOML conversion).
///
/// # Errors
///
/// Returns [`ParseInstanceError::SchemaMismatch`] if the JSON structure
/// does not match the schema.
pub fn parse_json_value(
    schema: &Schema,
    root_vertex: &str,
    json_val: &serde_json::Value,
    protocol: &str,
) -> Result<WInstance, ParseInstanceError> {
    parse_json(schema, root_vertex, json_val).map_err(|e| ParseInstanceError::Parse {
        protocol: protocol.to_string(),
        message: e.to_string(),
    })
}

/// Emit a `WInstance` to JSON bytes.
///
/// Uses [`panproto_inst::to_json`] to convert the instance to a
/// `serde_json::Value`, then serializes to bytes.
///
/// # Errors
///
/// Returns [`EmitInstanceError::Emit`] if the instance cannot be serialized.
pub fn emit_json_bytes(
    schema: &Schema,
    instance: &WInstance,
    protocol: &str,
) -> Result<Vec<u8>, EmitInstanceError> {
    let json_val = to_json(schema, instance);
    serde_json::to_vec(&json_val).map_err(|e| EmitInstanceError::Emit {
        protocol: protocol.to_string(),
        message: e.to_string(),
    })
}

/// Emit a `WInstance` to a `serde_json::Value` (non-byte path).
///
/// Used when the caller needs the JSON value for further processing
/// (e.g., YAML or TOML conversion).
#[must_use]
pub fn emit_json_value(schema: &Schema, instance: &WInstance) -> serde_json::Value {
    to_json(schema, instance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use panproto_schema::SchemaBuilder;

    fn simple_schema() -> Schema {
        // A minimal schema with a root object and one string property.
        let proto = panproto_schema::Protocol {
            name: "test".into(),
            schema_theory: "ThTestSchema".into(),
            instance_theory: "ThTestInstance".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into()],
            constraint_sorts: vec![],
        };
        SchemaBuilder::new(&proto)
            .vertex("root", "object", None)
            .expect("vertex")
            .vertex("root:name", "string", None)
            .expect("vertex")
            .edge("root", "root:name", "prop", Some("name"))
            .expect("edge")
            .build()
            .expect("build")
    }

    #[test]
    fn roundtrip_json_bytes() {
        let schema = simple_schema();
        let input = br#"{"name": "Alice"}"#;

        let instance =
            parse_json_bytes(&schema, "root", input, "test").expect("parse should succeed");
        assert!(instance.node_count() >= 2, "should have at least root + name nodes");

        let emitted = emit_json_bytes(&schema, &instance, "test").expect("emit should succeed");
        let instance2 =
            parse_json_bytes(&schema, "root", &emitted, "test").expect("re-parse should succeed");

        assert_eq!(instance.node_count(), instance2.node_count());
        assert_eq!(instance.arc_count(), instance2.arc_count());
    }

    #[test]
    fn invalid_json_returns_error() {
        let schema = simple_schema();
        let input = b"not valid json {{{";
        let result = parse_json_bytes(&schema, "root", input, "test");
        assert!(result.is_err());
    }
}
