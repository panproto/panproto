//! Generic tabular codec for protocols whose instance data is delimited text.
//!
//! Used for CSV, INI, and similar tab/comma/pipe-delimited formats.

use panproto_inst::{FInstance, WInstance};
use panproto_schema::Schema;

use crate::error::{EmitInstanceError, ParseInstanceError};
use crate::tabular_pathway;
use crate::traits::{InstanceEmitter, InstanceParser, NativeRepr};

/// A generic codec for tab/comma-separated protocols.
pub struct TabularCodec {
    protocol: String,
    table_vertex: String,
    delimiter: u8,
    comment_prefix: Option<u8>,
}

impl TabularCodec {
    /// Create a new tabular codec for TSV data (tab-delimited, `#` comments).
    #[must_use]
    pub fn tsv(protocol: impl Into<String>, table_vertex: impl Into<String>) -> Self {
        Self {
            protocol: protocol.into(),
            table_vertex: table_vertex.into(),
            delimiter: b'\t',
            comment_prefix: Some(b'#'),
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
        NativeRepr::Functor
    }

    fn parse_wtype(
        &self,
        _schema: &Schema,
        _input: &[u8],
    ) -> Result<WInstance, ParseInstanceError> {
        Err(ParseInstanceError::UnsupportedRepresentation {
            protocol: self.protocol.clone(),
            requested: NativeRepr::WType,
            native: NativeRepr::Functor,
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
            requested: NativeRepr::WType,
            native: NativeRepr::Functor,
        })
    }

    fn emit_functor(
        &self,
        _schema: &Schema,
        instance: &FInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        tabular_pathway::emit_tsv(instance, &self.protocol, &self.table_vertex, self.delimiter)
    }
}
