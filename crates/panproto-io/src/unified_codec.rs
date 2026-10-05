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
    CstCodecError, CstComplement, FormatKind, extract_json_cst_with_protocol,
    extract_tabular_cst_with_protocol, extract_toml_cst_with_protocol,
    extract_xml_cst_with_protocol, extract_yaml_cst_with_protocol, inject_json_cst_with_protocol,
    inject_tabular_cst_with_protocol, inject_toml_cst_with_protocol, inject_xml_cst_with_protocol,
    inject_yaml_cst_with_protocol,
};
use crate::error::{EmitInstanceError, ParseInstanceError, UnifiedCodecError};
use crate::json_codec::{missing_entry_message, resolve_entry_vertex};
use crate::json_pathway::protocol_definition;
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
    /// # Errors
    ///
    /// Returns [`UnifiedCodecError::MissingGrammar`] if the tree-sitter
    /// JSON grammar is not compiled in (enable
    /// `panproto-grammars/lang-json`), or [`UnifiedCodecError::ParserInit`]
    /// if the grammar is present but parser initialization fails.
    pub fn json(protocol: impl Into<String>) -> Result<Self, UnifiedCodecError> {
        Self::new(protocol, FormatKind::Json, NativeRepr::Tree)
    }

    /// Create a new unified codec for an XML-based protocol.
    ///
    /// # Errors
    ///
    /// Returns [`UnifiedCodecError`] if the tree-sitter XML grammar is
    /// unavailable or parser initialization fails.
    pub fn xml(protocol: impl Into<String>) -> Result<Self, UnifiedCodecError> {
        Self::new(protocol, FormatKind::Xml, NativeRepr::Tree)
    }

    /// Create a new unified codec for a YAML-based protocol.
    ///
    /// # Errors
    ///
    /// Returns [`UnifiedCodecError`] if the tree-sitter YAML grammar is
    /// unavailable or parser initialization fails.
    pub fn yaml(protocol: impl Into<String>) -> Result<Self, UnifiedCodecError> {
        Self::new(protocol, FormatKind::Yaml, NativeRepr::Tree)
    }

    /// Create a new unified codec for a TOML-based protocol.
    ///
    /// # Errors
    ///
    /// Returns [`UnifiedCodecError`] if the tree-sitter TOML grammar is
    /// unavailable or parser initialization fails.
    pub fn toml(protocol: impl Into<String>) -> Result<Self, UnifiedCodecError> {
        Self::new(protocol, FormatKind::Toml, NativeRepr::Tree)
    }

    /// Create a new unified codec for a CSV-based protocol.
    ///
    /// # Errors
    ///
    /// Returns [`UnifiedCodecError`] if the tree-sitter CSV grammar is
    /// unavailable or parser initialization fails.
    pub fn csv(protocol: impl Into<String>) -> Result<Self, UnifiedCodecError> {
        Self::new(protocol, FormatKind::Csv, NativeRepr::Table)
    }

    /// Create a new unified codec for a TSV-based protocol.
    ///
    /// # Errors
    ///
    /// Returns [`UnifiedCodecError`] if the tree-sitter TSV grammar is
    /// unavailable or parser initialization fails.
    pub fn tsv(
        protocol: impl Into<String>,
        table_vertex: impl Into<String>,
    ) -> Result<Self, UnifiedCodecError> {
        let mut codec = Self::new(protocol, FormatKind::Tsv, NativeRepr::Table)?;
        codec.table_vertex = Some(table_vertex.into());
        Ok(codec)
    }

    /// Create a new unified codec with explicit format and representation.
    ///
    /// # Errors
    ///
    /// Returns [`UnifiedCodecError::MissingGrammar`] if the required
    /// tree-sitter grammar is not compiled in, or
    /// [`UnifiedCodecError::ParserInit`] if grammar initialization fails.
    pub fn new(
        protocol: impl Into<String>,
        format: FormatKind,
        native_repr: NativeRepr,
    ) -> Result<Self, UnifiedCodecError> {
        let protocol = protocol.into();
        let grammar_name = format.grammar_name();

        let grammar = panproto_grammars::grammars()
            .into_iter()
            .find(|g| g.name == grammar_name)
            .ok_or_else(|| UnifiedCodecError::MissingGrammar {
                format: grammar_name.to_owned(),
            })?;

        let config = panproto_parse::languages::walker_configs::walker_config_for(grammar_name);
        let lang_parser = LanguageParser::from_language_with_grammar_json(
            grammar_name,
            grammar.extensions.to_vec(),
            grammar.language,
            grammar.node_types,
            grammar.tags_query,
            config,
            grammar.grammar_json,
        )
        .map_err(|e| UnifiedCodecError::ParserInit {
            format: grammar_name.to_owned(),
            source: Box::new(e),
        })?;

        Ok(Self {
            protocol,
            format,
            native_repr,
            lang_parser,
            table_vertex: None,
        })
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
        let root = resolve_entry_vertex(None, schema).ok_or_else(|| {
            ParseInstanceError::SchemaMismatch {
                protocol: self.protocol.clone(),
                message: missing_entry_message(None, schema),
            }
        })?;
        let cst_schema = self.parse_to_cst(input)?;
        let protocol = protocol_definition(&self.protocol);

        let (instance, complement) = match self.format {
            FormatKind::Json => {
                extract_json_cst_with_protocol(&cst_schema, schema, &protocol, root)
                    .map_err(|error| map_cst_parse_error(error, &self.protocol))?
            }
            FormatKind::Toml => {
                extract_toml_cst_with_protocol(&cst_schema, schema, &protocol, root)
                    .map_err(|error| map_cst_parse_error(error, &self.protocol))?
            }
            FormatKind::Xml => extract_xml_cst_with_protocol(&cst_schema, schema, &protocol, root)
                .map_err(|error| map_cst_parse_error(error, &self.protocol))?,
            FormatKind::Yaml => {
                extract_yaml_cst_with_protocol(&cst_schema, schema, &protocol, root)
                    .map_err(|error| map_cst_parse_error(error, &self.protocol))?
            }
            FormatKind::Csv | FormatKind::Tsv => {
                return Err(ParseInstanceError::UnsupportedRepresentation {
                    protocol: self.protocol.clone(),
                    requested: NativeRepr::Tree,
                    native: NativeRepr::Table,
                });
            }
        };

        Ok((instance, complement))
    }

    /// Emit bytes from a `WInstance` using the CST complement for format preservation.
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
        let protocol = protocol_definition(&self.protocol);
        let updated_cst = match self.format {
            FormatKind::Json => {
                inject_json_cst_with_protocol(instance, complement, schema, &protocol)
            }
            FormatKind::Toml => {
                inject_toml_cst_with_protocol(instance, complement, schema, &protocol)
            }
            FormatKind::Xml => {
                inject_xml_cst_with_protocol(instance, complement, schema, &protocol)
            }
            FormatKind::Yaml => {
                inject_yaml_cst_with_protocol(instance, complement, schema, &protocol)
            }
            FormatKind::Csv | FormatKind::Tsv => {
                return Err(EmitInstanceError::UnsupportedRepresentation {
                    protocol: self.protocol.clone(),
                    requested: NativeRepr::Tree,
                    native: NativeRepr::Table,
                });
            }
        }
        .map_err(|error| map_cst_emit_error(error, &self.protocol))?;

        self.lang_parser
            .emit(&updated_cst)
            .map_err(|e| EmitInstanceError::Emit {
                protocol: self.protocol.clone(),
                message: e.to_string(),
            })
    }

    /// Parse raw bytes for a tabular (CSV/TSV) format, returning both the
    /// `FInstance` and the CST complement needed for byte-preserving emission.
    ///
    /// Unlike [`InstanceParser::parse_functor`], which discards formatting
    /// (and therefore loses column order on re-emit), this retains the CST
    /// complement so [`emit_functor_preserving`](Self::emit_functor_preserving)
    /// can reproduce the original bytes exactly.
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError`] if the format is not tabular or parsing
    /// fails.
    pub fn parse_functor_preserving(
        &self,
        schema: &Schema,
        input: &[u8],
    ) -> Result<(FInstance, CstComplement), ParseInstanceError> {
        let complement_format = match self.format {
            FormatKind::Csv => "csv",
            FormatKind::Tsv => "tsv",
            _ => {
                return Err(ParseInstanceError::UnsupportedRepresentation {
                    protocol: self.protocol.clone(),
                    requested: NativeRepr::Table,
                    native: self.native_repr,
                });
            }
        };
        let table_vertex =
            resolve_entry_vertex(self.table_vertex.as_deref(), schema).ok_or_else(|| {
                ParseInstanceError::SchemaMismatch {
                    protocol: self.protocol.clone(),
                    message: missing_entry_message(self.table_vertex.as_deref(), schema),
                }
            })?;
        let cst_schema = self.parse_to_cst(input)?;
        let protocol = protocol_definition(&self.protocol);
        let (instance, mut complement) =
            extract_tabular_cst_with_protocol(&cst_schema, schema, &protocol, table_vertex)
                .map_err(|error| map_cst_parse_error(error, &self.protocol))?;
        complement.format = complement_format.into();
        Ok((instance, complement))
    }

    /// Emit bytes from a tabular (CSV/TSV) `FInstance` using the CST complement
    /// for byte-faithful format preservation.
    ///
    /// The instance's cell values are spliced back into the preserved CST
    /// (retaining column order, quoting, and line endings), then emitted via
    /// the unified replay path.
    ///
    /// # Errors
    ///
    /// Returns [`EmitInstanceError`] if the format is not tabular or emission
    /// fails.
    pub fn emit_functor_preserving(
        &self,
        schema: &Schema,
        instance: &FInstance,
        complement: &CstComplement,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        let expected_complement_format = match self.format {
            FormatKind::Csv => "csv",
            FormatKind::Tsv => "tsv",
            _ => {
                return Err(EmitInstanceError::UnsupportedRepresentation {
                    protocol: self.protocol.clone(),
                    requested: NativeRepr::Table,
                    native: self.native_repr,
                });
            }
        };
        if complement.format != expected_complement_format {
            return Err(EmitInstanceError::UnsupportedStructuralEdit {
                format: expected_complement_format.to_owned(),
                detail: format!(
                    "complement format is '{}' rather than '{expected_complement_format}'",
                    complement.format
                ),
            });
        }
        let expected_table = resolve_entry_vertex(self.table_vertex.as_deref(), schema)
            .ok_or_else(|| EmitInstanceError::Emit {
                protocol: self.protocol.clone(),
                message: missing_entry_message(self.table_vertex.as_deref(), schema),
            })?;
        if complement.source_vertex().map(AsRef::as_ref) != Some(expected_table) {
            return Err(EmitInstanceError::UnsupportedStructuralEdit {
                format: expected_complement_format.to_owned(),
                detail: format!(
                    "complement source table is {:?} rather than codec table '{expected_table}'",
                    complement.source_vertex().map(AsRef::as_ref)
                ),
            });
        }
        let protocol = protocol_definition(&self.protocol);
        let updated_cst = inject_tabular_cst_with_protocol(instance, complement, schema, &protocol)
            .map_err(|error| map_cst_emit_error(error, &self.protocol))?;
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

    /// Parse raw bytes to an `FInstance` for tabular formats.
    fn parse_functor_from_cst(
        &self,
        schema: &Schema,
        input: &[u8],
    ) -> Result<FInstance, ParseInstanceError> {
        let table_vertex =
            resolve_entry_vertex(self.table_vertex.as_deref(), schema).ok_or_else(|| {
                ParseInstanceError::SchemaMismatch {
                    protocol: self.protocol.clone(),
                    message: missing_entry_message(self.table_vertex.as_deref(), schema),
                }
            })?;
        let cst_schema = self.parse_to_cst(input)?;

        let protocol = protocol_definition(&self.protocol);
        let (instance, _complement) =
            extract_tabular_cst_with_protocol(&cst_schema, schema, &protocol, table_vertex)
                .map_err(|error| map_cst_parse_error(error, &self.protocol))?;

        Ok(instance)
    }
}

fn map_cst_parse_error(error: CstCodecError, protocol: &str) -> ParseInstanceError {
    match error {
        CstCodecError::Fidelity(errors) => ParseInstanceError::Fidelity(errors),
        CstCodecError::UnsupportedTaggedContainer {
            tag,
            container_kind,
            byte_offset,
        } => ParseInstanceError::UnsupportedTaggedContainer {
            protocol: protocol.to_owned(),
            tag,
            container_kind,
            byte_offset,
        },
        other => ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: other.to_string(),
        },
    }
}

fn map_cst_emit_error(error: CstCodecError, protocol: &str) -> EmitInstanceError {
    match error {
        CstCodecError::Value(error) => EmitInstanceError::Value(error),
        CstCodecError::UnsupportedStructuralEdit { format, detail } => {
            EmitInstanceError::UnsupportedStructuralEdit { format, detail }
        }
        other => EmitInstanceError::Emit {
            protocol: protocol.to_owned(),
            message: other.to_string(),
        },
    }
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
                requested: NativeRepr::Table,
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
            FormatKind::Json | FormatKind::Yaml => {
                crate::json_pathway::emit_json_bytes(schema, instance, &self.protocol)
            }
            FormatKind::Toml => {
                crate::toml_pathway::emit_toml_bytes(schema, instance, &self.protocol)
            }
            FormatKind::Xml => crate::xml_pathway::emit_xml_bytes(schema, instance, &self.protocol),
            FormatKind::Csv | FormatKind::Tsv => {
                Err(EmitInstanceError::UnsupportedRepresentation {
                    protocol: self.protocol.clone(),
                    requested: NativeRepr::Tree,
                    native: NativeRepr::Table,
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
                let table_vertex = resolve_entry_vertex(self.table_vertex.as_deref(), schema)
                    .ok_or_else(|| EmitInstanceError::Emit {
                        protocol: self.protocol.clone(),
                        message: missing_entry_message(self.table_vertex.as_deref(), schema),
                    })?;
                crate::tabular_pathway::validate_canonical_tabular_structure(
                    instance,
                    table_vertex,
                    if delimiter == b',' { "csv" } else { "tsv" },
                )?;
                crate::tabular_pathway::emit_tsv_with_schema(
                    schema,
                    instance,
                    &self.protocol,
                    table_vertex,
                    delimiter,
                )
            }
            _ => Err(EmitInstanceError::UnsupportedRepresentation {
                protocol: self.protocol.clone(),
                requested: NativeRepr::Table,
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

    fn test_protocol() -> panproto_schema::Protocol {
        panproto_schema::Protocol {
            name: "test".into(),
            schema_theory: "ThTestSchema".into(),
            instance_theory: "ThTestInstance".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into(), "number".into()],
            constraint_sorts: vec![],
            ..Default::default()
        }
    }

    fn test_schema() -> Schema {
        SchemaBuilder::new(&test_protocol())
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
        let codec = UnifiedCodec::json("test").unwrap();
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
        let codec = UnifiedCodec::json("test").unwrap();
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

    #[test]
    fn unified_root_selection_is_deterministic() {
        let schema = SchemaBuilder::new(&test_protocol())
            .vertex("declared", "object", None)
            .unwrap()
            .vertex("configured", "object", None)
            .unwrap()
            .entry("declared")
            .build()
            .unwrap();
        let codec = UnifiedCodec::tsv("test", "configured").unwrap();
        let (instance, complement) = codec
            .parse_functor_preserving(&schema, b"value\ncell\n")
            .unwrap();

        assert_eq!(resolve_entry_vertex(None, &schema), Some("declared"));
        assert_eq!(
            complement.source_vertex().map(AsRef::as_ref),
            Some("configured")
        );
        assert_eq!(instance.rows("configured").len(), 1);
        assert!(instance.rows("declared").is_empty());

        let insertion_order_a = SchemaBuilder::new(&test_protocol())
            .vertex("b", "object", None)
            .unwrap()
            .vertex("a", "object", None)
            .unwrap()
            .build()
            .unwrap();
        let insertion_order_b = SchemaBuilder::new(&test_protocol())
            .vertex("a", "object", None)
            .unwrap()
            .vertex("b", "object", None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(resolve_entry_vertex(None, &insertion_order_a), Some("a"));
        assert_eq!(resolve_entry_vertex(None, &insertion_order_b), Some("a"));

        let cyclic = SchemaBuilder::new(&test_protocol())
            .vertex("b", "object", None)
            .unwrap()
            .vertex("a", "object", None)
            .unwrap()
            .edge("b", "a", "prop", Some("a"))
            .unwrap()
            .edge("a", "b", "prop", Some("b"))
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(resolve_entry_vertex(None, &cyclic), Some("a"));

        let mut stale = SchemaBuilder::new(&test_protocol())
            .vertex("a", "object", None)
            .unwrap()
            .build()
            .unwrap();
        stale.entries = vec!["ghost".into()];
        assert_eq!(resolve_entry_vertex(None, &stale), None);
        let refused = UnifiedCodec::json("test")
            .unwrap()
            .parse_wtype_preserving(&stale, b"{}");
        assert!(matches!(
            refused,
            Err(ParseInstanceError::SchemaMismatch { ref message, .. })
                if message == "declared entry vertex 'ghost' is not a vertex of the schema"
        ));
    }

    #[test]
    fn explicit_tsv_rejects_an_empty_schema_before_cst_parsing() {
        let codec = UnifiedCodec::tsv("test", "configured").unwrap();
        let result = codec.parse_functor_preserving(&Schema::default(), b"\0\"unterminated");

        assert!(matches!(
            result,
            Err(ParseInstanceError::SchemaMismatch { ref message, .. })
                if message.contains("configured") && message.contains("does not exist")
        ));
    }

    #[test]
    fn explicit_tsv_rejects_a_missing_configured_vertex_before_cst_parsing() {
        let schema = SchemaBuilder::new(&test_protocol())
            .vertex("declared", "object", None)
            .unwrap()
            .entry("declared")
            .build()
            .unwrap();
        let codec = UnifiedCodec::tsv("test", "missing").unwrap();
        let result = codec.parse_functor_preserving(&schema, b"\0\"unterminated");

        assert!(matches!(
            result,
            Err(ParseInstanceError::SchemaMismatch { ref message, .. })
                if message.contains("missing") && message.contains("does not exist")
        ));
    }
}
