//! Value-kind rows for the RDF and edge-list protocols (`ntriples`, `nquads`, `turtle`, `jsonld`, `edgelist-csv`, `edgelist-tsv`).

use panproto_schema::ScalarKind;

use super::{NonScalarRow, ValueKindRow};

/// The rows of this family, one per declaration.
///
/// An IRI, a blank node label, a literal's lexical form and a language tag
/// are each held as the exact string the document denotes, and so are an
/// edge list's node names and edge labels.
pub(super) const ROWS: &[ValueKindRow] = &[
    ValueKindRow {
        protocol: "ntriples",
        vertex_kind: "iri",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "turtle",
        vertex_kind: "iri",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "jsonld",
        vertex_kind: "iri",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "ntriples",
        vertex_kind: "blank",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "turtle",
        vertex_kind: "blank",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "jsonld",
        vertex_kind: "blank",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "ntriples",
        vertex_kind: "literal",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "turtle",
        vertex_kind: "literal",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "jsonld",
        vertex_kind: "literal",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "ntriples",
        vertex_kind: "language",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "turtle",
        vertex_kind: "language",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "jsonld",
        vertex_kind: "language",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "edgelist-csv",
        vertex_kind: "node",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "edgelist-csv",
        vertex_kind: "edge",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "edgelist-tsv",
        vertex_kind: "node",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "edgelist-tsv",
        vertex_kind: "edge",
        kind: ScalarKind::Str,
    },
];

/// The vertex kinds of this family that hold no scalar.
pub(super) const NON_SCALAR: &[NonScalarRow] = &[
    NonScalarRow {
        protocol: "ntriples",
        vertex_kind: "triple",
    },
    NonScalarRow {
        protocol: "turtle",
        vertex_kind: "triple",
    },
    NonScalarRow {
        protocol: "jsonld",
        vertex_kind: "triple",
    },
];
