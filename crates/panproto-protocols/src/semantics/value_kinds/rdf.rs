//! Value-kind rows for the RDF and edge-list protocols (`ntriples`, `nquads`, `turtle`, `jsonld`, `edgelist`).

use panproto_schema::ScalarKind;

use super::{NonScalarRow, ValueKindRow};

/// The rows of this family, one per declaration.
///
/// An IRI, a blank node label, a literal's lexical form and a language tag
/// are each held as the exact string the document denotes.
pub(super) const ROWS: &[ValueKindRow] = &[
    ValueKindRow {
        protocol: "ntriples",
        vertex_kind: "iri",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "ntriples",
        vertex_kind: "blank",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "ntriples",
        vertex_kind: "literal",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "ntriples",
        vertex_kind: "language",
        kind: ScalarKind::Str,
    },
];

/// The vertex kinds of this family that hold no scalar.
pub(super) const NON_SCALAR: &[NonScalarRow] = &[NonScalarRow {
    protocol: "ntriples",
    vertex_kind: "triple",
}];
