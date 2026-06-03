//! Instance codecs for domain-specific protocols.
//!
//! - JSON-based: geojson, fhir, vcard_ical (via jCard/jCal JSON encoding)
//! - XML-based: rss_atom
//! - Delimited: swift_mt (colon-delimited), edi_x12 (asterisk-delimited)

use crate::byte_tabular::ByteTabularCodec;
use crate::registry::ProtocolRegistry;

/// Register all domain protocol codecs with the registry.
#[allow(deprecated)]
pub fn register_all(registry: &mut ProtocolRegistry) {
    // Colon-delimited (SWIFT MT) and asterisk-delimited (EDI X12) formats have
    // no tree-sitter grammar. The byte-faithful tabular codec records the exact
    // original layout (field segments, line endings, blank lines) and replays
    // it, so a `parse → emit` round-trip is byte-identical and a single-cell
    // edit re-emits exactly that cell.
    registry.register(ByteTabularCodec::new("swift_mt", "fields", b':', None));
    registry.register(ByteTabularCodec::new("edi_x12", "segments", b'*', None));

    #[cfg(feature = "tree-sitter")]
    {
        use crate::unified_codec::UnifiedCodec;
        registry.register_optional(UnifiedCodec::json("geojson"));
        registry.register_optional(UnifiedCodec::json("fhir"));
        registry.register_optional(UnifiedCodec::json("vcard_ical"));
        registry.register_optional(UnifiedCodec::xml("rss_atom"));
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
