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
use panproto_schema::{Schema, primary_entry};

use crate::error::{EmitInstanceError, ParseInstanceError};
use crate::json_pathway;
use crate::traits::{InstanceEmitter, InstanceParser, NativeRepr};

/// A generic codec for protocols whose instance data is JSON.
///
/// Delegates parsing to [`json_pathway::parse_json_bytes`] and emission
/// to [`json_pathway::emit_json_bytes`]. Parsing anchors the instance at the
/// schema's declared primary entry, with [`primary_entry`] providing a
/// deterministic fallback for legacy schemas without a declared entry.
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
            native_repr: NativeRepr::Tree,
        }
    }

    /// Create a new JSON codec that supports functor instances.
    #[must_use]
    pub const fn with_repr(mut self, repr: NativeRepr) -> Self {
        self.native_repr = repr;
        self
    }
}

/// Select the explicit entry when supplied, then the schema's primary entry.
///
/// Either way the selected name must be a vertex of `schema`; a configured or
/// declared entry that the schema does not contain selects nothing, and the
/// caller reports `SchemaMismatch` rather than anchoring an instance outside
/// the schema.
pub(crate) fn resolve_entry_vertex<'a>(
    explicit_entry: Option<&'a str>,
    schema: &'a Schema,
) -> Option<&'a str> {
    explicit_entry
        .or_else(|| primary_entry(schema).map(AsRef::as_ref))
        .filter(|entry| schema.has_vertex(entry))
}

/// Explain why [`resolve_entry_vertex`] returned `None` for the same
/// arguments.
pub(crate) fn missing_entry_message(explicit_entry: Option<&str>, schema: &Schema) -> String {
    if let Some(entry) = explicit_entry {
        return format!("configured entry vertex '{entry}' does not exist in schema");
    }
    schema.entries.first().map_or_else(
        || "schema has no entry vertex".to_owned(),
        |entry| format!("declared entry vertex '{entry}' is not a vertex of the schema"),
    )
}

impl InstanceParser for JsonCodec {
    fn protocol_name(&self) -> &str {
        &self.protocol
    }

    fn native_repr(&self) -> NativeRepr {
        self.native_repr
    }

    fn parse_wtype(&self, schema: &Schema, input: &[u8]) -> Result<WInstance, ParseInstanceError> {
        let root = resolve_entry_vertex(None, schema).ok_or_else(|| {
            ParseInstanceError::SchemaMismatch {
                protocol: self.protocol.clone(),
                message: missing_entry_message(None, schema),
            }
        })?;
        json_pathway::parse_json_bytes(schema, root, input, &self.protocol)
    }

    fn parse_functor(
        &self,
        _schema: &Schema,
        _input: &[u8],
    ) -> Result<FInstance, ParseInstanceError> {
        Err(ParseInstanceError::UnsupportedRepresentation {
            protocol: self.protocol.clone(),
            requested: NativeRepr::Table,
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
            requested: NativeRepr::Table,
            native: self.native_repr,
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use panproto_schema::SchemaBuilder;

    fn test_protocol() -> panproto_schema::Protocol {
        panproto_schema::Protocol {
            name: "test".into(),
            schema_theory: "ThTestSchema".into(),
            instance_theory: "ThTestInstance".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into()],
            constraint_sorts: vec![],
            ..Default::default()
        }
    }

    fn test_schema() -> Schema {
        SchemaBuilder::new(&test_protocol())
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

    #[test]
    fn json_codec_prefers_the_declared_entry() {
        let schema = SchemaBuilder::new(&test_protocol())
            .vertex("a", "object", None)
            .expect("a")
            .vertex("z", "object", None)
            .expect("z")
            .entry("z")
            .build()
            .expect("schema");

        let instance = JsonCodec::new("test")
            .parse_wtype(&schema, b"{}")
            .expect("parse declared entry");

        assert_eq!(instance.schema_root.as_ref(), "z");
    }

    #[test]
    fn json_codec_fallback_is_independent_of_vertex_insertion_order() {
        let first = SchemaBuilder::new(&test_protocol())
            .vertex("b", "object", None)
            .expect("b")
            .vertex("a", "object", None)
            .expect("a")
            .build()
            .expect("first schema");
        let second = SchemaBuilder::new(&test_protocol())
            .vertex("a", "object", None)
            .expect("a")
            .vertex("b", "object", None)
            .expect("b")
            .build()
            .expect("second schema");
        let codec = JsonCodec::new("test");

        let first_instance = codec
            .parse_wtype(&first, b"{}")
            .expect("parse first schema");
        let second_instance = codec
            .parse_wtype(&second, b"{}")
            .expect("parse second schema");

        assert_eq!(first_instance.schema_root.as_ref(), "a");
        assert_eq!(second_instance.schema_root.as_ref(), "a");
    }

    #[test]
    fn json_codec_cyclic_schema_uses_deterministic_primary_fallback() {
        let schema = SchemaBuilder::new(&test_protocol())
            .vertex("b", "object", None)
            .expect("b")
            .vertex("a", "object", None)
            .expect("a")
            .edge("b", "a", "prop", Some("a"))
            .expect("b to a")
            .edge("a", "b", "prop", Some("b"))
            .expect("a to b")
            .build()
            .expect("cyclic schema");

        let instance = JsonCodec::new("test")
            .parse_wtype(&schema, b"{}")
            .expect("parse cyclic schema");

        assert_eq!(instance.schema_root.as_ref(), "a");
    }

    #[test]
    fn json_codec_empty_schema_is_a_typed_mismatch() {
        let result = JsonCodec::new("test").parse_wtype(&Schema::default(), b"{}");

        assert!(matches!(
            result,
            Err(ParseInstanceError::SchemaMismatch { protocol, message })
                if protocol == "test" && message == "schema has no entry vertex"
        ));
    }

    #[test]
    fn explicit_entry_resolution_requires_a_schema_vertex() {
        let schema = SchemaBuilder::new(&test_protocol())
            .vertex("declared", "object", None)
            .expect("declared")
            .entry("declared")
            .build()
            .expect("schema");

        assert_eq!(resolve_entry_vertex(Some("missing"), &schema), None);
        assert_eq!(
            resolve_entry_vertex(Some("declared"), &schema),
            Some("declared")
        );
        assert_eq!(
            resolve_entry_vertex(Some("missing"), &Schema::default()),
            None
        );
    }

    #[test]
    fn a_declared_entry_outside_the_schema_is_refused() {
        let mut stale = SchemaBuilder::new(&test_protocol())
            .vertex("a", "object", None)
            .expect("a")
            .build()
            .expect("schema");
        stale.entries = vec!["ghost".into()];

        assert_eq!(resolve_entry_vertex(None, &stale), None);
        let result = JsonCodec::new("test").parse_wtype(&stale, b"{}");
        assert!(
            matches!(
                result,
                Err(ParseInstanceError::SchemaMismatch { ref protocol, ref message })
                    if protocol == "test"
                        && message == "declared entry vertex 'ghost' is not a vertex of the schema"
            ),
            "{result:?}"
        );
    }
}
