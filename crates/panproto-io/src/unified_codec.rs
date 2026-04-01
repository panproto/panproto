//! Unified tree-sitter-based codec for all protocols.
//!
//! The `UnifiedCodec` replaces format-specific codecs (`JsonCodec`, `XmlCodec`,
//! `TabularCodec`) with a single implementation that:
//!
//! 1. Parses raw bytes via tree-sitter to a lossless CST Schema
//! 2. Extracts a domain-level WInstance/FInstance via the CST extraction lens
//! 3. Emits bytes by injecting instance data back into the CST Schema
//!
//! Format preservation comes for free: the CST Schema stores formatting
//! as constraints (interstitials, byte positions, indentation), and
//! `emit_from_schema` reconstructs the original bytes exactly.
//!
//! ## Feature gate
//!
//! This module requires the `tree-sitter` feature flag, which brings in
//! `panproto-parse` and `panproto-grammars` as dependencies.

use panproto_inst::{FInstance, WInstance};
use panproto_parse::languages::common::LanguageParser;
use panproto_parse::registry::AstParser;
use panproto_schema::Schema;

use crate::cst_extract::{
    CstComplement, FormatKind, extract_json_cst, extract_tabular_cst, extract_xml_cst,
    extract_yaml_cst, inject_json_cst, inject_tabular_cst, inject_xml_cst, inject_yaml_cst,
};
use crate::error::{EmitInstanceError, ParseInstanceError};
use crate::traits::{InstanceEmitter, InstanceParser, NativeRepr};

/// A unified codec backed by tree-sitter parsing and CST extraction.
///
/// Supports format-preserving round-trips: `parse → modify → emit` produces
/// output with the original formatting intact.
pub struct UnifiedCodec {
    protocol: String,
    format: FormatKind,
    native_repr: NativeRepr,
    /// The tree-sitter language parser for the underlying format.
    lang_parser: LanguageParser,
    /// For tabular codecs: the table vertex name in the `FInstance`.
    table_vertex: Option<String>,
}

impl UnifiedCodec {
    /// Create a new unified codec for a JSON-based protocol.
    ///
    /// # Panics
    ///
    /// Panics if the tree-sitter JSON grammar is not available (ensure the
    /// `tree-sitter` feature is enabled with `panproto-grammars/lang-json`).
    #[must_use]
    pub fn json(protocol: impl Into<String>) -> Self {
        Self::new(protocol, FormatKind::Json, NativeRepr::WType)
    }

    /// Create a new unified codec for an XML-based protocol.
    ///
    /// # Panics
    ///
    /// Panics if the tree-sitter XML grammar is not available.
    #[must_use]
    pub fn xml(protocol: impl Into<String>) -> Self {
        Self::new(protocol, FormatKind::Xml, NativeRepr::WType)
    }

    /// Create a new unified codec for a YAML-based protocol.
    ///
    /// # Panics
    ///
    /// Panics if the tree-sitter YAML grammar is not available.
    #[must_use]
    pub fn yaml(protocol: impl Into<String>) -> Self {
        Self::new(protocol, FormatKind::Yaml, NativeRepr::WType)
    }

    /// Create a new unified codec for a TOML-based protocol.
    ///
    /// # Panics
    ///
    /// Panics if the tree-sitter TOML grammar is not available.
    #[must_use]
    pub fn toml(protocol: impl Into<String>) -> Self {
        Self::new(protocol, FormatKind::Toml, NativeRepr::WType)
    }

    /// Create a new unified codec for a CSV-based protocol.
    ///
    /// # Panics
    ///
    /// Panics if the tree-sitter CSV grammar is not available.
    #[must_use]
    pub fn csv(protocol: impl Into<String>) -> Self {
        Self::new(protocol, FormatKind::Csv, NativeRepr::Functor)
    }

    /// Create a new unified codec for a TSV-based protocol.
    ///
    /// # Panics
    ///
    /// Panics if the tree-sitter TSV grammar is not available.
    #[must_use]
    pub fn tsv(protocol: impl Into<String>, table_vertex: impl Into<String>) -> Self {
        let mut codec = Self::new(protocol, FormatKind::Tsv, NativeRepr::Functor);
        codec.table_vertex = Some(table_vertex.into());
        codec
    }

    /// Create a new unified codec with explicit format and representation.
    ///
    /// # Panics
    ///
    /// Panics if the required tree-sitter grammar is not available.
    #[must_use]
    pub fn new(protocol: impl Into<String>, format: FormatKind, native_repr: NativeRepr) -> Self {
        let protocol = protocol.into();
        let grammar_name = format.grammar_name();

        let grammar = panproto_grammars::grammars()
            .into_iter()
            .find(|g| g.name == grammar_name)
            .unwrap_or_else(|| {
                panic!(
                    "tree-sitter grammar '{grammar_name}' not available; \
                     enable panproto-grammars/lang-{grammar_name}"
                )
            });

        let config = panproto_parse::languages::walker_configs::walker_config_for(grammar_name);
        let lang_parser = LanguageParser::from_language(
            grammar_name,
            grammar.extensions.to_vec(),
            grammar.language,
            grammar.node_types,
            config,
        )
        .unwrap_or_else(|e| panic!("failed to initialize grammar '{grammar_name}': {e}"));

        Self {
            protocol,
            format,
            native_repr,
            lang_parser,
            table_vertex: None,
        }
    }

    /// Parse raw bytes and return both the instance and the CST complement.
    ///
    /// The complement enables format-preserving emission via
    /// [`emit_wtype_preserving`](Self::emit_wtype_preserving).
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError`] if parsing or extraction fails.
    pub fn parse_wtype_preserving(
        &self,
        schema: &Schema,
        input: &[u8],
    ) -> Result<(WInstance, CstComplement), ParseInstanceError> {
        let cst_schema = self.parse_to_cst(input)?;
        let root = find_root_vertex(schema).ok_or_else(|| ParseInstanceError::SchemaMismatch {
            protocol: self.protocol.clone(),
            message: "schema has no root vertex".into(),
        })?;

        let (instance, complement) = match self.format {
            FormatKind::Json | FormatKind::Toml => extract_json_cst(&cst_schema, schema, &root)
                .map_err(|e| ParseInstanceError::Parse {
                    protocol: self.protocol.clone(),
                    message: e.to_string(),
                })?,
            FormatKind::Xml => extract_xml_cst(&cst_schema, schema, &root).map_err(|e| {
                ParseInstanceError::Parse {
                    protocol: self.protocol.clone(),
                    message: e.to_string(),
                }
            })?,
            FormatKind::Yaml => extract_yaml_cst(&cst_schema, schema, &root).map_err(|e| {
                ParseInstanceError::Parse {
                    protocol: self.protocol.clone(),
                    message: e.to_string(),
                }
            })?,
            FormatKind::Csv | FormatKind::Tsv => {
                return Err(ParseInstanceError::UnsupportedRepresentation {
                    protocol: self.protocol.clone(),
                    requested: NativeRepr::WType,
                    native: NativeRepr::Functor,
                });
            }
        };

        Ok((instance, complement))
    }

    /// Emit bytes from a WInstance using the CST complement for format preservation.
    ///
    /// If the complement is available, the instance data is injected back into
    /// the CST Schema and emitted with original formatting. Otherwise, falls
    /// back to canonical emission.
    ///
    /// # Errors
    ///
    /// Returns [`EmitInstanceError`] if emission fails.
    pub fn emit_wtype_preserving(
        &self,
        schema: &Schema,
        instance: &WInstance,
        complement: &CstComplement,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        let updated_cst = match self.format {
            FormatKind::Json | FormatKind::Toml => inject_json_cst(instance, complement, schema),
            FormatKind::Xml => inject_xml_cst(instance, complement, schema),
            FormatKind::Yaml => inject_yaml_cst(instance, complement, schema),
            FormatKind::Csv | FormatKind::Tsv => {
                return Err(EmitInstanceError::UnsupportedRepresentation {
                    protocol: self.protocol.clone(),
                    requested: NativeRepr::WType,
                    native: NativeRepr::Functor,
                });
            }
        }
        .map_err(|e| EmitInstanceError::Emit {
            protocol: self.protocol.clone(),
            message: e.to_string(),
        })?;

        self.lang_parser
            .emit(&updated_cst)
            .map_err(|e| EmitInstanceError::Emit {
                protocol: self.protocol.clone(),
                message: e.to_string(),
            })
    }

    /// Parse raw bytes to a CST Schema via tree-sitter.
    fn parse_to_cst(&self, input: &[u8]) -> Result<Schema, ParseInstanceError> {
        let file_name = format!("input.{}", self.format.extensions()[0]);
        self.lang_parser
            .parse(input, &file_name)
            .map_err(|e| ParseInstanceError::Parse {
                protocol: self.protocol.clone(),
                message: e.to_string(),
            })
    }

    /// Parse raw bytes to an FInstance for tabular formats.
    fn parse_functor_from_cst(
        &self,
        schema: &Schema,
        input: &[u8],
    ) -> Result<FInstance, ParseInstanceError> {
        let cst_schema = self.parse_to_cst(input)?;
        let table_vertex = self
            .table_vertex
            .clone()
            .or_else(|| find_root_vertex(schema))
            .unwrap_or_else(|| "rows".to_string());

        let (instance, _complement) = extract_tabular_cst(&cst_schema, schema, &table_vertex)
            .map_err(|e| ParseInstanceError::Parse {
                protocol: self.protocol.clone(),
                message: e.to_string(),
            })?;

        Ok(instance)
    }
}

/// Find the first root vertex in a schema (a vertex with no incoming edges).
fn find_root_vertex(schema: &Schema) -> Option<String> {
    schema
        .vertices
        .values()
        .find(|v| schema.incoming_edges(&v.id).is_empty())
        .map(|v| v.id.to_string())
}

impl InstanceParser for UnifiedCodec {
    fn protocol_name(&self) -> &str {
        &self.protocol
    }

    fn native_repr(&self) -> NativeRepr {
        self.native_repr
    }

    fn parse_wtype(&self, schema: &Schema, input: &[u8]) -> Result<WInstance, ParseInstanceError> {
        let (instance, _complement) = self.parse_wtype_preserving(schema, input)?;
        Ok(instance)
    }

    fn parse_functor(
        &self,
        schema: &Schema,
        input: &[u8],
    ) -> Result<FInstance, ParseInstanceError> {
        match self.format {
            FormatKind::Csv | FormatKind::Tsv => self.parse_functor_from_cst(schema, input),
            _ => Err(ParseInstanceError::UnsupportedRepresentation {
                protocol: self.protocol.clone(),
                requested: NativeRepr::Functor,
                native: self.native_repr,
            }),
        }
    }
}

impl InstanceEmitter for UnifiedCodec {
    fn protocol_name(&self) -> &str {
        &self.protocol
    }

    fn emit_wtype(
        &self,
        schema: &Schema,
        instance: &WInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        // Without a complement, fall back to canonical emission via legacy pathways.
        // For format-preserving emission, use `ProtocolCodec::emit_wtype_preserving`
        // which takes an explicit complement argument.
        match self.format {
            FormatKind::Json | FormatKind::Toml | FormatKind::Yaml => {
                crate::json_pathway::emit_json_bytes(schema, instance, &self.protocol)
            }
            FormatKind::Xml => crate::xml_pathway::emit_xml_bytes(schema, instance, &self.protocol),
            FormatKind::Csv | FormatKind::Tsv => {
                Err(EmitInstanceError::UnsupportedRepresentation {
                    protocol: self.protocol.clone(),
                    requested: NativeRepr::WType,
                    native: NativeRepr::Functor,
                })
            }
        }
    }

    fn emit_functor(
        &self,
        schema: &Schema,
        instance: &FInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        match self.format {
            FormatKind::Csv | FormatKind::Tsv => {
                // Without a complement, fall back to legacy tabular emission.
                let delimiter = match self.format {
                    FormatKind::Tsv => b'\t',
                    _ => b',',
                };
                let table_vertex = self
                    .table_vertex
                    .clone()
                    .or_else(|| find_root_vertex(schema))
                    .unwrap_or_else(|| "rows".to_string());
                crate::tabular_pathway::emit_tsv(instance, &self.protocol, &table_vertex, delimiter)
            }
            _ => Err(EmitInstanceError::UnsupportedRepresentation {
                protocol: self.protocol.clone(),
                requested: NativeRepr::Functor,
                native: self.native_repr,
            }),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_schema::SchemaBuilder;

    fn test_schema() -> Schema {
        let proto = panproto_schema::Protocol {
            name: "test".into(),
            schema_theory: "ThTestSchema".into(),
            instance_theory: "ThTestInstance".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into(), "number".into()],
            constraint_sorts: vec![],
            ..panproto_schema::Protocol::default()
        };
        SchemaBuilder::new(&proto)
            .vertex("root", "object", None)
            .unwrap()
            .vertex("root:name", "string", None)
            .unwrap()
            .vertex("root:value", "number", None)
            .unwrap()
            .edge("root", "root:name", "prop", Some("name"))
            .unwrap()
            .edge("root", "root:value", "prop", Some("value"))
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    fn unified_json_parse_and_emit() {
        let codec = UnifiedCodec::json("test");
        let schema = test_schema();
        let input = br#"{"name": "Alice", "value": 42}"#;

        let (instance, complement) = codec.parse_wtype_preserving(&schema, input).unwrap();
        assert_eq!(instance.node_count(), 3, "expected root + 2 children");

        let emitted = codec
            .emit_wtype_preserving(&schema, &instance, &complement)
            .unwrap();
        assert_eq!(
            std::str::from_utf8(input).unwrap(),
            std::str::from_utf8(&emitted).unwrap(),
            "format-preserving round-trip should produce identical bytes"
        );
    }

    #[test]
    fn unified_json_preserves_formatting() {
        let codec = UnifiedCodec::json("test");
        let schema = test_schema();
        let input = b"{\n  \"name\": \"Alice\",\n  \"value\": 42\n}";

        let (instance, complement) = codec.parse_wtype_preserving(&schema, input).unwrap();
        assert_eq!(instance.node_count(), 3);

        let emitted = codec
            .emit_wtype_preserving(&schema, &instance, &complement)
            .unwrap();
        assert_eq!(
            std::str::from_utf8(input).unwrap(),
            std::str::from_utf8(&emitted).unwrap(),
            "format-preserving emit should reconstruct original formatting"
        );
    }
}
