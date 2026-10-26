//! Generic tabular codec for protocols whose instance data is delimited text.
//!
//! Used for CSV, INI, and similar tab/comma/pipe-delimited formats.

use panproto_inst::{FInstance, WInstance};
use panproto_schema::Schema;

use crate::error::{EmitInstanceError, ParseInstanceError};
use crate::tabular_pathway;
use crate::traits::{InstanceEmitter, InstanceParser, NativeRepr};

/// A generic codec for tab/comma-separated protocols.
///
/// # Deprecation
///
/// This codec discards formatting (line endings, comment lines, quoting
/// style) during parsing. Enable the `tree-sitter` feature and use
/// `UnifiedCodec::tsv` or `UnifiedCodec::csv` for format-preserving
/// round-trips.
#[deprecated(
    since = "0.24.0",
    note = "use UnifiedCodec::tsv/csv (tree-sitter feature) for format-preserving round-trips"
)]
pub struct TabularCodec {
    protocol: String,
    table_vertex: String,
    delimiter: u8,
    comment_prefix: Option<u8>,
}

impl TabularCodec {
    /// Create a new tabular codec for IANA TSV data.
    ///
    /// TSV has no comment syntax: a leading `#` is ordinary field data.
    #[must_use]
    pub fn tsv(protocol: impl Into<String>, table_vertex: impl Into<String>) -> Self {
        Self {
            protocol: protocol.into(),
            table_vertex: table_vertex.into(),
            delimiter: b'\t',
            comment_prefix: None,
        }
    }

    /// Create a new tabular codec for CSV data (comma-delimited, no comments).
    #[must_use]
    pub fn csv(protocol: impl Into<String>, table_vertex: impl Into<String>) -> Self {
        Self {
            protocol: protocol.into(),
            table_vertex: table_vertex.into(),
            delimiter: b',',
            comment_prefix: None,
        }
    }

    /// Create a codec with a custom delimiter.
    #[must_use]
    pub fn with_delimiter(
        protocol: impl Into<String>,
        table_vertex: impl Into<String>,
        delimiter: u8,
    ) -> Self {
        Self {
            protocol: protocol.into(),
            table_vertex: table_vertex.into(),
            delimiter,
            comment_prefix: None,
        }
    }
}

impl InstanceParser for TabularCodec {
    fn protocol_name(&self) -> &str {
        &self.protocol
    }

    fn native_repr(&self) -> NativeRepr {
        NativeRepr::Table
    }

    fn parse_wtype(
        &self,
        _schema: &Schema,
        _input: &[u8],
    ) -> Result<WInstance, ParseInstanceError> {
        Err(ParseInstanceError::UnsupportedRepresentation {
            protocol: self.protocol.clone(),
            requested: NativeRepr::Tree,
            native: NativeRepr::Table,
        })
    }

    fn parse_functor(
        &self,
        schema: &Schema,
        input: &[u8],
    ) -> Result<FInstance, ParseInstanceError> {
        tabular_pathway::parse_tsv(
            schema,
            input,
            &self.protocol,
            &self.table_vertex,
            self.delimiter,
            self.comment_prefix,
        )
    }
}

impl InstanceEmitter for TabularCodec {
    fn protocol_name(&self) -> &str {
        &self.protocol
    }

    fn emit_wtype(
        &self,
        _schema: &Schema,
        _instance: &WInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        Err(EmitInstanceError::UnsupportedRepresentation {
            protocol: self.protocol.clone(),
            requested: NativeRepr::Tree,
            native: NativeRepr::Table,
        })
    }

    fn emit_functor(
        &self,
        schema: &Schema,
        instance: &FInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        tabular_pathway::validate_canonical_tabular_structure(
            instance,
            &self.table_vertex,
            match self.delimiter {
                b',' => "csv",
                b'\t' => "tsv",
                _ => "tabular",
            },
        )?;
        tabular_pathway::emit_tsv_with_schema_and_comments(
            schema,
            instance,
            &self.protocol,
            &self.table_vertex,
            self.delimiter,
            self.comment_prefix,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fmt::Display;

    use panproto_inst::value::Value;
    use panproto_schema::SchemaBuilder;

    use super::*;
    use crate::error::EmitValueReason;

    fn success<T, E: Display>(result: Result<T, E>, context: &str) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error}"),
        }
    }

    fn schema() -> Schema {
        let protocol = crate::json_pathway::protocol_definition("cassandra");
        let builder = success(
            SchemaBuilder::new(&protocol).vertex("rows", "table", None),
            "table",
        );
        let builder = success(builder.vertex("value", "text", None), "value");
        let builder = success(builder.edge("rows", "value", "prop", Some("value")), "edge");
        success(builder.build(), "schema")
    }

    fn one_cell(column: &str, value: &str) -> FInstance {
        success(
            FInstance::new().with_table(
                "rows",
                vec![HashMap::from([(
                    column.to_owned(),
                    Value::Str(value.to_owned()),
                )])],
            ),
            "table",
        )
    }

    #[test]
    fn canonical_tsv_preserves_hashes_and_one_column_empty_rows() {
        let codec = TabularCodec::tsv("cassandra", "rows");
        for (column, value) in [("#header", "value"), ("value", "#data"), ("value", "")] {
            let instance = one_cell(column, value);
            let emitted = success(codec.emit_functor(&schema(), &instance), "TSV emit");
            let reparsed = success(codec.parse_functor(&schema(), &emitted), "TSV reparse");
            assert_eq!(
                reparsed.rows("rows")[0].1.get(column),
                Some(&Value::Str(value.into()))
            );
        }
    }

    #[test]
    fn canonical_tsv_refuses_lines_that_reparse_as_comments_or_blanks() {
        let mut codec = TabularCodec::tsv("cassandra", "rows");
        codec.comment_prefix = Some(b'#');
        assert!(matches!(
            codec.emit_functor(&schema(), &one_cell("#header", "value")),
            Err(EmitInstanceError::Emit { message, .. }) if message.contains("comment")
        ));
        assert!(matches!(
            codec.emit_functor(&schema(), &one_cell("value", "#comment")),
            Err(EmitInstanceError::Value(error))
                if error.reason == EmitValueReason::NotRepresentable
        ));
    }
}
