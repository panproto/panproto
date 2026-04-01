//! Instance codecs for web and document protocols.
//!
//! - `ATProto` records use JSON encoding
//! - DOCX, ODF use XML-based parsing

use crate::registry::ProtocolRegistry;

/// Register all web/document protocol codecs with the registry.
#[allow(deprecated)]
pub fn register_all(registry: &mut ProtocolRegistry) {
    #[cfg(feature = "tree-sitter")]
    {
        use crate::unified_codec::UnifiedCodec;
        registry.register(UnifiedCodec::json("atproto"));
        registry.register(UnifiedCodec::xml("docx"));
        registry.register(UnifiedCodec::xml("odf"));
    }
    #[cfg(not(feature = "tree-sitter"))]
    {
        use crate::json_codec::JsonCodec;
        use crate::xml_codec::XmlCodec;
        registry.register(JsonCodec::new("atproto"));
        registry.register(XmlCodec::new("docx"));
        registry.register(XmlCodec::new("odf"));
    }
}
