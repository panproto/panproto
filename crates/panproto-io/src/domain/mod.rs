//! Instance codecs for domain-specific protocols.
//!
//! - JSON-based: geojson, fhir, vcard_ical (via jCard/jCal JSON encoding)
//! - XML-based: rss_atom
//! - Delimited: swift_mt (colon-delimited), edi_x12 (asterisk-delimited)

use crate::json_codec::JsonCodec;
use crate::registry::ProtocolRegistry;
use crate::tabular_codec::TabularCodec;
use crate::xml_codec::XmlCodec;

/// Register all domain protocol codecs with the registry.
pub fn register_all(registry: &mut ProtocolRegistry) {
    registry.register(JsonCodec::new("geojson"));
    registry.register(JsonCodec::new("fhir"));
    registry.register(JsonCodec::new("vcard_ical"));
    registry.register(XmlCodec::new("rss_atom"));
    registry.register(TabularCodec::with_delimiter("swift_mt", "fields", b':'));
    registry.register(TabularCodec::with_delimiter("edi_x12", "segments", b'*'));
}
