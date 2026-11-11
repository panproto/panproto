//! The RDF graph formats.
//!
//! An RDF graph is a set of triples, each a subject, a predicate and an
//! object, over three kinds of term: IRIs, blank nodes and literals (RDF 1.1
//! Concepts and Abstract Syntax, Sec 3). Every RDF protocol of this module
//! reads its documents into one graph encoding, the *RDF graph encoding*,
//! fixed by [`graph_schema`](crate::rdf::graph_schema):
//!
//! - one `iri` node per distinct IRI, holding the IRI as a string;
//! - one `blank` node per distinct blank node of the document, holding the
//!   label the document gives it;
//! - one `literal` node per distinct literal, holding its lexical form as a
//!   string, with a `datatype` edge to the `iri` node of its datatype and,
//!   for a language-tagged string, a `language` edge to a `language` node
//!   holding the tag;
//! - one `triple` node per distinct triple, with a `subject` edge to an
//!   `iri` or `blank` node, a `predicate` edge to an `iri` node and an
//!   `object` edge to an `iri`, `blank` or `literal` node.
//!
//! A triple is a node rather than an edge so that a predicate is a term like
//! any other: the IRI that names a predicate in one triple is the same node
//! as the IRI that is the subject or object of another.

use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder, SchemaError};

/// The N-Triples protocol.
pub mod ntriples;

/// The vertex of triple nodes.
pub const TRIPLE: &str = "triple";
/// The vertex of IRI nodes.
pub const IRI: &str = "iri";
/// The vertex of blank nodes.
pub const BLANK: &str = "blank";
/// The vertex of literal nodes.
pub const LITERAL: &str = "literal";
/// The vertex of language-tag nodes.
pub const LANGUAGE: &str = "language";

/// The edge from a triple to its subject.
pub const SUBJECT: &str = "subject";
/// The edge from a triple to its predicate.
pub const PREDICATE: &str = "predicate";
/// The edge from a triple to its object.
pub const OBJECT: &str = "object";
/// The edge from a literal to its datatype IRI.
pub const DATATYPE: &str = "datatype";
/// The edge from a language-tagged literal to its tag.
pub const LANGUAGE_TAG: &str = "language";

/// The kind of every edge of the RDF graph encoding.
pub const EDGE_KIND: &str = "prop";

/// The datatype of a literal written with no datatype and no language tag.
pub const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
/// The datatype of every language-tagged literal.
pub const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

/// The schema edges of the RDF graph encoding, as `(source, target, name)`.
pub const EDGES: &[(&str, &str, &str)] = &[
    (TRIPLE, IRI, SUBJECT),
    (TRIPLE, BLANK, SUBJECT),
    (TRIPLE, IRI, PREDICATE),
    (TRIPLE, IRI, OBJECT),
    (TRIPLE, BLANK, OBJECT),
    (TRIPLE, LITERAL, OBJECT),
    (LITERAL, IRI, DATATYPE),
    (LITERAL, LANGUAGE, LANGUAGE_TAG),
];

/// The vertex kinds of the RDF graph encoding.
#[must_use]
pub fn obj_kinds() -> Vec<String> {
    [TRIPLE, IRI, BLANK, LITERAL, LANGUAGE]
        .iter()
        .map(|kind| (*kind).to_owned())
        .collect()
}

/// The edge rule of the RDF graph encoding: a `prop` edge leaves a triple
/// or a literal.
#[must_use]
pub fn edge_rules() -> Vec<EdgeRule> {
    vec![EdgeRule {
        edge_kind: EDGE_KIND.into(),
        src_kinds: vec![TRIPLE.into(), LITERAL.into()],
        tgt_kinds: vec![IRI.into(), BLANK.into(), LITERAL.into(), LANGUAGE.into()],
    }]
}

/// The schema of the RDF graph encoding under `protocol`: the five vertices,
/// each named for its kind, and the edges of [`EDGES`].
///
/// # Errors
///
/// Returns [`SchemaError`] when `protocol` does not admit the encoding's
/// vertex kinds and edges.
pub fn graph_schema(protocol: &Protocol) -> Result<Schema, SchemaError> {
    let mut builder = SchemaBuilder::new(protocol);
    for kind in [TRIPLE, IRI, BLANK, LITERAL, LANGUAGE] {
        builder = builder.vertex(kind, kind, None)?;
    }
    for (src, tgt, name) in EDGES {
        builder = builder.edge(src, tgt, EDGE_KIND, Some(name))?;
    }
    builder.build()
}
