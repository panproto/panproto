//! Constraint-semantics rows for the domain protocols (`fhir`, `geojson`,
//! `vcard-ical`, `rss-atom`, `edi-x12`, `swift-mt`).
//!
//! An X12 data element's minimum and maximum lengths count characters. A
//! FHIR element's `min` and `max` and a vCard property's `cardinality` are
//! cardinalities of the element under its parent (`max` may be `*`), and
//! `required` is a presence flag; all are about how often the parent holds
//! the element, not about the element's own value, and describe here, as do
//! FHIR's terminology `binding` and X12's `usage` and `repeat`. The
//! `swift-mt` parser writes a field's tag under `format`, an identifier
//! rather than a value format, so it describes; RSS and Atom
//! `format` names a value format of the protocol's family, and SWIFT's
//! `length` is a field length written in SWIFT's own notation (`16x`,
//! `6!n`), which describes.

use panproto_gat::Name;
use panproto_schema::{ConstraintSemantics, LengthUnit};

use super::ConstraintRow;

/// The row giving sort `sort` of protocol `protocol` the meaning `semantics`
/// builds.
const fn row(
    protocol: &'static str,
    sort: &'static str,
    semantics: fn() -> ConstraintSemantics,
) -> ConstraintRow {
    ConstraintRow {
        protocol,
        sort,
        semantics,
    }
}

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ConstraintRow] = &[
    row("fhir", "min", || ConstraintSemantics::Annotation),
    row("fhir", "max", || ConstraintSemantics::Annotation),
    row("fhir", "binding", || ConstraintSemantics::Annotation),
    row("geojson", "required", || ConstraintSemantics::Annotation),
    row("vcard-ical", "required", || ConstraintSemantics::Annotation),
    row("vcard-ical", "cardinality", || {
        ConstraintSemantics::Annotation
    }),
    row("rss-atom", "required", || ConstraintSemantics::Annotation),
    row("rss-atom", "format", || {
        ConstraintSemantics::Format(Name::from("rss-atom"))
    }),
    row("edi-x12", "min-length", || {
        ConstraintSemantics::MinLength(LengthUnit::Chars)
    }),
    row("edi-x12", "max-length", || {
        ConstraintSemantics::MaxLength(LengthUnit::Chars)
    }),
    row("edi-x12", "required", || ConstraintSemantics::Annotation),
    row("edi-x12", "usage", || ConstraintSemantics::Annotation),
    row("edi-x12", "repeat", || ConstraintSemantics::Annotation),
    row("swift-mt", "required", || ConstraintSemantics::Annotation),
    row("swift-mt", "format", || ConstraintSemantics::Annotation),
    row("swift-mt", "length", || ConstraintSemantics::Annotation),
];
