//! The scalar kind of each protocol's scalar vertex kinds.
//!
//! One [`ValueKindRow`] per scalar vertex kind of a protocol, kept in one
//! file per protocol family under `value_kinds/` and listed in `FAMILIES`
//! below. [`attach`](super::attach) collects a protocol's rows into its
//! [`value_kinds`](panproto_schema::Protocol::value_kinds). The same files
//! list, as [`NonScalarRow`]s, the vertex kinds that hold no scalar (records,
//! lists, references, declarations), so every vertex kind a parser emits is
//! accounted for one way or the other.

use std::collections::{BTreeMap, BTreeSet};

use panproto_schema::ScalarKind;

mod annotation_other;
mod annotation_xml;
mod api;
mod code;
mod config;
mod data_schema;
mod data_science;
mod database;
mod domain;
mod rdf;
mod serialization;
mod sql;
mod text;
mod web_document;

/// The scalar kind of one vertex kind of one protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueKindRow {
    /// The protocol's canonical name.
    pub protocol: &'static str,
    /// The vertex kind.
    pub vertex_kind: &'static str,
    /// The scalar kind its vertices hold.
    pub kind: ScalarKind,
}

/// A vertex kind of one protocol whose vertices hold no scalar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NonScalarRow {
    /// The protocol's canonical name.
    pub protocol: &'static str,
    /// The vertex kind.
    pub vertex_kind: &'static str,
}

/// The row tables of every family.
const FAMILIES: &[&[ValueKindRow]] = &[
    annotation_other::ROWS,
    annotation_xml::ROWS,
    api::ROWS,
    code::ROWS,
    config::ROWS,
    data_schema::ROWS,
    data_science::ROWS,
    database::ROWS,
    domain::ROWS,
    rdf::ROWS,
    serialization::ROWS,
    sql::ROWS,
    text::ROWS,
    web_document::ROWS,
];

/// The non-scalar tables of every family.
const NON_SCALAR_FAMILIES: &[&[NonScalarRow]] = &[
    annotation_other::NON_SCALAR,
    annotation_xml::NON_SCALAR,
    api::NON_SCALAR,
    code::NON_SCALAR,
    config::NON_SCALAR,
    data_schema::NON_SCALAR,
    data_science::NON_SCALAR,
    database::NON_SCALAR,
    domain::NON_SCALAR,
    rdf::NON_SCALAR,
    serialization::NON_SCALAR,
    sql::NON_SCALAR,
    text::NON_SCALAR,
    web_document::NON_SCALAR,
];

/// Every value-kind row, family by family.
pub fn rows() -> impl Iterator<Item = &'static ValueKindRow> {
    FAMILIES.iter().flat_map(|family| family.iter())
}

/// The scalar kind of each vertex kind the protocol whose canonical name is
/// `protocol` classifies, keyed by vertex kind.
#[must_use]
pub fn declared(protocol: &str) -> BTreeMap<String, ScalarKind> {
    rows()
        .filter(|row| row.protocol == protocol)
        .map(|row| (row.vertex_kind.to_owned(), row.kind.clone()))
        .collect()
}

/// Every non-scalar row, family by family.
pub fn non_scalar_rows() -> impl Iterator<Item = &'static NonScalarRow> {
    NON_SCALAR_FAMILIES.iter().flat_map(|family| family.iter())
}

/// The vertex kinds the protocol whose canonical name is `protocol`
/// declares non-scalar.
#[must_use]
pub fn non_scalar(protocol: &str) -> BTreeSet<&'static str> {
    non_scalar_rows()
        .filter(|row| row.protocol == protocol)
        .map(|row| row.vertex_kind)
        .collect()
}
