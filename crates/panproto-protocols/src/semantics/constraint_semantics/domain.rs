//! Constraint-semantics rows for the domain protocols (`fhir`, `geojson`,
//! `vcard-ical`, `rss-atom`, `edi-x12`, `swift-mt`).
//!
//! An X12 data element's minimum and maximum lengths count characters. A
//! FHIR element's `min` and `max` and a vCard property's `cardinality` are
//! cardinalities of the element under its parent (`max` may be `*`), and
//! `required` is a presence flag; all are about the parent element and
//! describe here (see the presence reading of the [`common`](super::common)
//! module), as do FHIR's terminology `binding` and X12's `usage` and
//! `repeat`. The `swift-mt` parser writes a field's tag under `format`, an
//! identifier rather than a value format, so it describes; RSS and Atom
//! `format` names a value format of the protocol's family, and SWIFT's
//! `length` is a field length written in SWIFT's own notation (`16x`,
//! `6!n`), which describes.

use panproto_gat::Name;
use panproto_schema::ConstraintSemantics;

use super::ConstraintRow;
use super::common::{annotation, max_chars, min_chars, row};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ConstraintRow] = &[
    row("fhir", "min", annotation),
    row("fhir", "max", annotation),
    row("fhir", "binding", annotation),
    row("geojson", "required", annotation),
    row("vcard-ical", "required", annotation),
    row("vcard-ical", "cardinality", annotation),
    row("rss-atom", "required", annotation),
    row("rss-atom", "format", || {
        ConstraintSemantics::Format(Name::from("rss-atom"))
    }),
    row("edi-x12", "min-length", min_chars),
    row("edi-x12", "max-length", max_chars),
    row("edi-x12", "required", annotation),
    row("edi-x12", "usage", annotation),
    row("edi-x12", "repeat", annotation),
    row("swift-mt", "required", annotation),
    row("swift-mt", "format", annotation),
    row("swift-mt", "length", annotation),
];
