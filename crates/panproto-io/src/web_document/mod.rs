//! Instance codecs for web and document protocols.
//!
//! - `ATProto` records use JSON encoding (via `JsonCodec`)
//! - DOCX, ODF use XML-based parsing

use crate::json_codec::JsonCodec;
use crate::registry::ProtocolRegistry;
use crate::xml_codec::XmlCodec;

/// Register all web/document protocol codecs with the registry.
pub fn register_all(registry: &mut ProtocolRegistry) {
    // JSON-based
    registry.register(JsonCodec::new("atproto"));

    // XML-based
    registry.register(XmlCodec::new("docx"));
    registry.register(XmlCodec::new("odf"));
}
