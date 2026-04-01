//! Generic JSON-based codec for protocols whose instance data is JSON.
//!
//! Many protocols (GraphQL responses, OpenAPI payloads, JSON Schema instances,
//! ATProto records, etc.) use JSON as their instance encoding. This module
//! provides a reusable `JsonCodec` that implements `InstanceParser` and
//! `InstanceEmitter` by delegating to the `json_pathway` module.
//!
//! Protocol-specific modules create a `JsonCodec` with their protocol name
//! and register it in the `ProtocolRegistry`.

use panproto_inst::{FInstance, WInstance};
use panproto_schema::Schema;

use crate::error::{EmitInstanceError, ParseInstanceError};
use crate::json_pathway;
use crate::traits::{InstanceEmitter, InstanceParser, NativeRepr};

/// A generic codec for protocols whose instance data is JSON.
///
/// Delegates parsing to [`json_pathway::parse_json_bytes`] and emission
/// to [`json_pathway::emit_json_bytes`]. The `root_vertex` field determines
/// which schema vertex anchors the root of the instance tree; if `None`,
/// the first root vertex in the schema is used.
///
/// # Deprecation
///
/// This codec discards formatting (whitespace, key ordering, indentation)
/// during parsing. Enable the `tree-sitter` feature and use `UnifiedCodec::json`
/// for format-preserving round-trips.
#[deprecated(
    since = "0.24.0",
    note = "use UnifiedCodec::json (tree-sitter feature) for format-preserving round-trips"
)]
pub struct JsonCodec {
    protocol: String,
    native_repr: NativeRepr,
}

impl JsonCodec {
    /// Create a new JSON codec for the given protocol.
    #[must_use]
    pub fn new(protocol: impl Into<String>) -> Self {
        Self {
            protocol: protocol.into(),
            native_repr: NativeRepr::WType,
        }
    }

    /// Create a new JSON codec that supports functor instances.
    #[must_use]
    pub const fn with_repr(mut self, repr: NativeRepr) -> Self {
        self.native_repr = repr;
        self
    }
}

/// Find the first root vertex in a schema (a vertex with no incoming structural edges).
fn find_root_vertex(schema: &Schema) -> Option<String> {
    // A root vertex has no incoming edges.
    schema
        .vertices
        .values()
        .find(|v| schema.incoming_edges(&v.id).is_empty())
        .map(|v| v.id.to_string())
}

impl InstanceParser for JsonCodec {
    fn protocol_name(&self) -> &str {
        &self.protocol
    }

    fn native_repr(&self) -> NativeRepr {
        self.native_repr
    }

    fn parse_wtype(&self, schema: &Schema, input: &[u8]) -> Result<WInstance, ParseInstanceError> {
        let root = find_root_vertex(schema).ok_or_else(|| ParseInstanceError::SchemaMismatch {
            protocol: self.protocol.clone(),
            message: "schema has no root vertex".into(),
        })?;
        json_pathway::parse_json_bytes(schema, &root, input, &self.protocol)
    }

    fn parse_functor(
        &self,
        _schema: &Schema,
        _input: &[u8],
    ) -> Result<FInstance, ParseInstanceError> {
        Err(ParseInstanceError::UnsupportedRepresentation {
            protocol: self.protocol.clone(),
            requested: NativeRepr::Functor,
            native: self.native_repr,
        })
    }
}

impl InstanceEmitter for JsonCodec {
    fn protocol_name(&self) -> &str {
        &self.protocol
    }

    fn emit_wtype(
        &self,
        schema: &Schema,
        instance: &WInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        json_pathway::emit_json_bytes(schema, instance, &self.protocol)
    }

    fn emit_functor(
        &self,
        _schema: &Schema,
        _instance: &FInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        Err(EmitInstanceError::UnsupportedRepresentation {
            protocol: self.protocol.clone(),
            requested: NativeRepr::Functor,
            native: self.native_repr,
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use panproto_schema::SchemaBuilder;

    fn test_schema() -> Schema {
        let proto = panproto_schema::Protocol {
            name: "test".into(),
            schema_theory: "ThTestSchema".into(),
            instance_theory: "ThTestInstance".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into()],
            constraint_sorts: vec![],
            ..panproto_schema::Protocol::default()
        };
        SchemaBuilder::new(&proto)
            .vertex("root", "object", None)
            .expect("v")
            .vertex("root:name", "string", None)
            .expect("v")
            .edge("root", "root:name", "prop", Some("name"))
            .expect("e")
            .build()
            .expect("build")
    }

    #[test]
    fn json_codec_roundtrip() {
        let codec = JsonCodec::new("test");
        let schema = test_schema();
        let input = br#"{"name": "Alice"}"#;

        let instance = codec.parse_wtype(&schema, input).expect("parse");
        let emitted = codec.emit_wtype(&schema, &instance).expect("emit");
        let instance2 = codec.parse_wtype(&schema, &emitted).expect("re-parse");

        assert_eq!(instance.node_count(), instance2.node_count());
    }

    #[test]
    fn json_codec_functor_unsupported() {
        let codec = JsonCodec::new("test");
        let schema = test_schema();
        let result = codec.parse_functor(&schema, b"{}");
        assert!(result.is_err());
    }
}
