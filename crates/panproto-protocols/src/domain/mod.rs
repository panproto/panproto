//! Domain-specific protocol definitions.

/// EDI X12 schema protocol definition and parser/emitter.
pub mod edi_x12;
/// FHIR StructureDefinition protocol definition and parser/emitter.
pub mod fhir;
/// GeoJSON schema protocol definition and parser/emitter.
pub mod geojson;
/// RSS/Atom feed schema protocol definition and parser/emitter.
pub mod rss_atom;
/// SWIFT MT financial messaging protocol definition and parser/emitter.
pub mod swift_mt;
/// vCard/iCalendar schema protocol definition and parser/emitter.
pub mod vcard_ical;
