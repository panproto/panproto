//! The Turtle protocol.
//!
//! A Turtle document (W3C Recommendation RDF 1.1 Turtle, 25 February 2014)
//! abbreviates a list of triples with directives, prefixed names,
//! predicate and object lists, blank node property lists and collections,
//! and it denotes the RDF graph that is the set of those triples, so its
//! instances are graphs both as parsed and once resolved. The schema theory is the constrained multigraph and
//! the instance theory `ThGraphInstance`; the instance schema is the RDF
//! graph encoding of [`graph_schema`].

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{Protocol, Schema, SchemaError};

use crate::theories;

/// Returns the Turtle protocol definition.
///
/// Its vertex kinds and edge rule are those of the RDF graph encoding.
#[must_use]
pub fn protocol() -> Protocol {
    let mut protocol = Protocol {
        name: "turtle".into(),
        schema_theory: "ThTurtleSchema".into(),
        instance_theory: "ThTurtleInstance".into(),
        edge_rules: super::edge_rules(),
        obj_kinds: super::obj_kinds(),
        ..Default::default()
    };
    crate::semantics::attach(&mut protocol);
    protocol
}

/// The schema every Turtle document is read under: the RDF graph
/// encoding.
///
/// # Errors
///
/// Returns [`SchemaError`] only if the protocol definition and the encoding
/// disagree, which the tests rule out.
pub fn graph_schema() -> Result<Schema, SchemaError> {
    super::graph_schema(&protocol())
}

/// Register the component GATs for Turtle with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_graph_instance(registry, "ThTurtleSchema", "ThTurtleInstance");
}
