//! Instance codecs for domain-specific protocols.
//!
//! - JSON-based: geojson, fhir, vcard_ical (via jCard/jCal JSON encoding)
//! - XML-based: rss_atom
//! - Delimited: swift_mt (colon-delimited), edi_x12 (asterisk-delimited)

use crate::registry::ProtocolRegistry;
#[allow(deprecated)]
use crate::tabular_codec::TabularCodec;

/// Register all domain protocol codecs with the registry.
#[allow(deprecated)]
pub fn register_all(registry: &mut ProtocolRegistry) {
    // Colon-delimited and asterisk-delimited formats have no tree-sitter
    // grammars, so they always use the legacy tabular codec.
    registry.register(TabularCodec::with_delimiter("swift_mt", "fields", b':'));
    registry.register(TabularCodec::with_delimiter("edi_x12", "segments", b'*'));

    #[cfg(feature = "tree-sitter")]
    {
        use crate::unified_codec::UnifiedCodec;
        registry.register(UnifiedCodec::json("geojson"));
        registry.register(UnifiedCodec::json("fhir"));
        registry.register(UnifiedCodec::json("vcard_ical"));
        registry.register(UnifiedCodec::xml("rss_atom"));
    }
    #[cfg(not(feature = "tree-sitter"))]
    {
        use crate::json_codec::JsonCodec;
        use crate::xml_codec::XmlCodec;
        registry.register(JsonCodec::new("geojson"));
        registry.register(JsonCodec::new("fhir"));
        registry.register(JsonCodec::new("vcard_ical"));
        registry.register(XmlCodec::new("rss_atom"));
    }
}
