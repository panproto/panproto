//! The N-Triples protocol.
//!
//! An N-Triples document (W3C Recommendation RDF 1.1 N-Triples, 25 February
//! 2014) is a line-based list of triples, and it denotes the RDF graph that
//! is the set of those triples, so its instances are graphs both as parsed
//! and once resolved. The schema theory is the constrained multigraph and
//! the instance theory `ThGraphInstance`; the instance schema is the RDF
//! graph encoding of [`graph_schema`].

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{Protocol, Schema, SchemaError};

use crate::theories;

/// Returns the N-Triples protocol definition.
///
/// Its vertex kinds and edge rule are those of the RDF graph encoding.
#[must_use]
pub fn protocol() -> Protocol {
    let mut protocol = Protocol {
        name: "ntriples".into(),
        schema_theory: "ThNTriplesSchema".into(),
        instance_theory: "ThNTriplesInstance".into(),
        edge_rules: super::edge_rules(),
        obj_kinds: super::obj_kinds(),
        ..Default::default()
    };
    crate::semantics::attach(&mut protocol);
    protocol
}

/// The schema every N-Triples document is read under: the RDF graph
/// encoding.
///
/// # Errors
///
/// Returns [`SchemaError`] only if the protocol definition and the encoding
/// disagree, which the tests rule out.
pub fn graph_schema() -> Result<Schema, SchemaError> {
    super::graph_schema(&protocol())
}

/// Register the component GATs for N-Triples with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_graph_instance(
        registry,
        "ThNTriplesSchema",
        "ThNTriplesInstance",
    );
}
