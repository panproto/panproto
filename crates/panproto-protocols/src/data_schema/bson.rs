//! The BSON protocol: BSON 1.1 documents and their tree encoding.
//!
//! A `bson` document is a [BSON 1.1](https://bsonspec.org/spec.html)
//! document, read from its binary form or from MongoDB Extended JSON by
//! `panproto-io`, and it is a tree both as parsed and once resolved: BSON
//! has no reference from one part of a document to another (a `DBPointer`
//! or a `DBRef` names a document of a collection, which is outside the
//! document). The schema theory is the constrained multigraph with W-type
//! instances, and every document is read under one schema, the *BSON tree
//! encoding* of [`tree_schema`](crate::data_schema::bson::tree_schema):
//!
//! - a [`DOCUMENT`](crate::data_schema::bson::DOCUMENT) node per document (the root, each embedded document and
//!   each code-with-scope's scope), with an [`ELEMENT_EDGE`](crate::data_schema::bson::ELEMENT_EDGE) edge to each of
//!   its elements in order;
//! - an [`ELEMENT`](crate::data_schema::bson::ELEMENT) node per element, holding the element's name as a
//!   string, with one [`VALUE`](crate::data_schema::bson::VALUE) edge to the node of its value;
//! - an [`ARRAY`](crate::data_schema::bson::ARRAY) node per array, with an [`ITEM`](crate::data_schema::bson::ITEM) edge to each of its
//!   values in order;
//! - a [`CODE_WITH_SCOPE`](crate::data_schema::bson::CODE_WITH_SCOPE) node, holding the code as a string, with one
//!   [`SCOPE`](crate::data_schema::bson::SCOPE) edge to its scope's [`DOCUMENT`](crate::data_schema::bson::DOCUMENT) node;
//! - a node per value of every other BSON type, anchored at the vertex of
//!   that type ([`VALUE_VERTICES`](crate::data_schema::bson::VALUE_VERTICES)) and holding the value as the
//!   `panproto-io` BSON module states.
//!
//! An element is a node rather than an edge name so that a document keeps
//! two elements of one name, which BSON allows, in their order.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder, SchemaError};

use crate::theories;

/// The vertex of document nodes.
pub const DOCUMENT: &str = "document";
/// The vertex of element nodes.
pub const ELEMENT: &str = "element";
/// The vertex of array nodes.
pub const ARRAY: &str = "array";
/// The vertex of code-with-scope nodes.
pub const CODE_WITH_SCOPE: &str = "code-with-scope";

/// The edge from a document node to each of its element nodes.
pub const ELEMENT_EDGE: &str = "element";
/// The edge from an element node to its value's node.
pub const VALUE: &str = "value";
/// The edge from an array node to each of its values' nodes.
pub const ITEM: &str = "item";
/// The edge from a code-with-scope node to its scope's document node.
pub const SCOPE: &str = "scope";

/// The vertices a value's node may be anchored at, one per BSON type, in
/// the order of their BSON type bytes (`0x01` to `0x13`, then `0xFF` and
/// `0x7F`).
pub const VALUE_VERTICES: &[&str] = &[
    "double",
    "string",
    DOCUMENT,
    ARRAY,
    "binary",
    "undefined",
    "object-id",
    "boolean",
    "datetime",
    "null",
    "regex",
    "db-pointer",
    "code",
    "symbol",
    CODE_WITH_SCOPE,
    "int32",
    "timestamp",
    "int64",
    "decimal128",
    "min-key",
    "max-key",
];

/// The vertex kinds of the BSON tree encoding: [`ELEMENT`] and every
/// vertex of [`VALUE_VERTICES`].
#[must_use]
pub fn obj_kinds() -> Vec<String> {
    std::iter::once(ELEMENT)
        .chain(VALUE_VERTICES.iter().copied())
        .map(str::to_owned)
        .collect()
}

/// The edges of the BSON tree encoding, as `(source, target, name)`; each
/// edge's kind is its name.
#[must_use]
pub fn edges() -> Vec<(&'static str, &'static str, &'static str)> {
    let mut edges = vec![(DOCUMENT, ELEMENT, ELEMENT_EDGE)];
    edges.extend(VALUE_VERTICES.iter().map(|tgt| (ELEMENT, *tgt, VALUE)));
    edges.extend(VALUE_VERTICES.iter().map(|tgt| (ARRAY, *tgt, ITEM)));
    edges.push((CODE_WITH_SCOPE, DOCUMENT, SCOPE));
    edges
}

/// The edge rules of the BSON tree encoding.
///
/// An `element` edge leaves a document and enters an element, a `value`
/// edge leaves an element and an `item` edge an array and each enters any
/// value vertex, and a `scope` edge leaves a code-with-scope and enters a
/// document.
#[must_use]
pub fn edge_rules() -> Vec<EdgeRule> {
    let values: Vec<String> = VALUE_VERTICES.iter().map(|v| (*v).to_owned()).collect();
    vec![
        EdgeRule {
            edge_kind: ELEMENT_EDGE.into(),
            src_kinds: vec![DOCUMENT.into()],
            tgt_kinds: vec![ELEMENT.into()],
        },
        EdgeRule {
            edge_kind: VALUE.into(),
            src_kinds: vec![ELEMENT.into()],
            tgt_kinds: values.clone(),
        },
        EdgeRule {
            edge_kind: ITEM.into(),
            src_kinds: vec![ARRAY.into()],
            tgt_kinds: values,
        },
        EdgeRule {
            edge_kind: SCOPE.into(),
            src_kinds: vec![CODE_WITH_SCOPE.into()],
            tgt_kinds: vec![DOCUMENT.into()],
        },
    ]
}

/// Returns the BSON protocol definition.
///
/// Its vertex kinds and edge rules are those of the BSON tree encoding.
#[must_use]
pub fn protocol() -> Protocol {
    let mut protocol = Protocol {
        name: "bson".into(),
        schema_theory: "ThBsonSchema".into(),
        instance_theory: "ThBsonInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: obj_kinds(),
        has_order: true,
        has_recursion: true,
        ..Default::default()
    };
    crate::semantics::attach(&mut protocol);
    protocol
}

/// The schema every BSON document is read under: the BSON tree encoding,
/// one vertex per kind named for its kind, the edges of [`edges`], and the
/// [`DOCUMENT`] vertex as its entry.
///
/// # Errors
///
/// Returns [`SchemaError`] only if the protocol definition and the encoding
/// disagree, which the tests rule out.
pub fn tree_schema() -> Result<Schema, SchemaError> {
    let protocol = protocol();
    let mut builder = SchemaBuilder::new(&protocol);
    for kind in obj_kinds() {
        builder = builder.vertex(&kind, &kind, None)?;
    }
    for (src, tgt, name) in edges() {
        builder = builder.edge(src, tgt, name, Some(name))?;
    }
    builder.entry(DOCUMENT).build()
}

/// Register the component GATs for BSON.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(registry, "ThBsonSchema", "ThBsonInstance");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tree_schema_holds_every_kind_and_edge_of_the_encoding() {
        let schema = tree_schema().unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(schema.vertices.len(), 22);
        assert_eq!(schema.edges.len(), 44);
        for kind in obj_kinds() {
            assert_eq!(
                schema
                    .vertices
                    .get(kind.as_str())
                    .map(|v| v.kind.to_string()),
                Some(kind)
            );
        }
        assert_eq!(schema.entries.len(), 1);
    }
}
