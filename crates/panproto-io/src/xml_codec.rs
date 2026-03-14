//! Generic XML-based codec for protocols whose instance data is XML.
//!
//! Used by NAF, FoLiA, TEI, TimeML, ELAN, ISO-Space, XSD instances,
//! RSS/Atom, PAULA, LAF/GrAF, DOCX (inner XML), ODF (inner XML).

use panproto_inst::{FInstance, WInstance};
use panproto_schema::Schema;

use crate::error::{EmitInstanceError, ParseInstanceError};
use crate::traits::{InstanceEmitter, InstanceParser, NativeRepr};
use crate::xml_pathway;

/// A generic codec for protocols whose instance data is XML.
///
/// Delegates to [`xml_pathway::parse_xml_bytes`] and
/// [`xml_pathway::emit_xml_bytes`].
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
        NativeRepr::WType
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
            requested: NativeRepr::Functor,
            native: NativeRepr::WType,
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
            requested: NativeRepr::Functor,
            native: NativeRepr::WType,
        })
    }
}
