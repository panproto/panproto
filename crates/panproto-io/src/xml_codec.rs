//! Generic XML-based codec for protocols whose instance data is XML.
//!
//! Used by NAF, FoLiA, TEI, TimeML, ELAN, ISO-Space, XSD instances,
//! RSS/Atom, PAULA, LAF/GrAF, DOCX (inner XML), ODF (inner XML).
//!
//! ## Prop1 waiver list
//!
//! - A scalar whose written text would exceed the default 64 MiB output-byte
//!   allowance is refused with a typed limit error.
//!
//! ## Departures from the format standard
//!
//! - The shared XML pathway writes a negative canonical NaN as `-NaN` to
//!   retain its sign. This is a deliberate departure from XSD 1.1 Part 2,
//!   Sections 3.3.4.1-3.3.4.2.

use panproto_inst::{FInstance, WInstance};
use panproto_schema::Schema;

use crate::error::{EmitInstanceError, ParseInstanceError};
use crate::traits::{InstanceEmitter, InstanceParser, NativeRepr};
use crate::xml_pathway;

/// A generic codec for protocols whose instance data is XML.
///
/// Delegates to [`xml_pathway::parse_xml_bytes`] and
/// [`xml_pathway::emit_xml_bytes`].
///
/// # Deprecation
///
/// This codec discards formatting (whitespace, attribute ordering,
/// comments) during parsing. Enable the `tree-sitter` feature and use
/// `UnifiedCodec::xml` for format-preserving round-trips.
#[deprecated(
    since = "0.24.0",
    note = "use UnifiedCodec::xml (tree-sitter feature) for format-preserving round-trips"
)]
pub struct XmlCodec {
    protocol: String,
}

impl XmlCodec {
    /// Create a new XML codec for the given protocol.
    #[must_use]
    pub fn new(protocol: impl Into<String>) -> Self {
        Self {
            protocol: protocol.into(),
        }
    }
}

impl InstanceParser for XmlCodec {
    fn protocol_name(&self) -> &str {
        &self.protocol
    }

    fn native_repr(&self) -> NativeRepr {
        NativeRepr::Tree
    }

    fn parse_wtype(&self, schema: &Schema, input: &[u8]) -> Result<WInstance, ParseInstanceError> {
        xml_pathway::parse_xml_bytes(schema, input, &self.protocol)
    }

    fn parse_functor(
        &self,
        _schema: &Schema,
        _input: &[u8],
    ) -> Result<FInstance, ParseInstanceError> {
        Err(ParseInstanceError::UnsupportedRepresentation {
            protocol: self.protocol.clone(),
            requested: NativeRepr::Table,
            native: NativeRepr::Tree,
        })
    }
}

impl InstanceEmitter for XmlCodec {
    fn protocol_name(&self) -> &str {
        &self.protocol
    }

    fn emit_wtype(
        &self,
        schema: &Schema,
        instance: &WInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        xml_pathway::emit_xml_bytes(schema, instance, &self.protocol)
    }

    fn emit_functor(
        &self,
        _schema: &Schema,
        _instance: &FInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        Err(EmitInstanceError::UnsupportedRepresentation {
            protocol: self.protocol.clone(),
            requested: NativeRepr::Table,
            native: NativeRepr::Tree,
        })
    }
}
