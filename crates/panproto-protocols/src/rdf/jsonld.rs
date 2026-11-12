//! The JSON-LD protocol.
//!
//! A JSON-LD document (W3C Recommendation JSON-LD 1.1, 16 July 2020)
//! is a JSON document whose context maps its terms to IRIs, and it
//! denotes the RDF graph its expansion and RDF deserialization give
//! (JSON-LD 1.1 Processing Algorithms and API Sec 5.1 and 8.1), so its
//! instances are graphs both as parsed and once resolved. The schema theory is the constrained multigraph and
//! the instance theory `ThGraphInstance`; the instance schema is the RDF
//! graph encoding of [`graph_schema`].

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{Protocol, Schema, SchemaError};

use crate::theories;

/// Returns the JSON-LD protocol definition.
///
/// Its vertex kinds and edge rule are those of the RDF graph encoding.
#[must_use]
pub fn protocol() -> Protocol {
    let mut protocol = Protocol {
        name: "jsonld".into(),
        schema_theory: "ThJsonLdSchema".into(),
        instance_theory: "ThJsonLdInstance".into(),
        edge_rules: super::edge_rules(),
        obj_kinds: super::obj_kinds(),
        ..Default::default()
    };
    crate::semantics::attach(&mut protocol);
    protocol
}

/// The schema every JSON-LD document is read under: the RDF graph
/// encoding.
///
/// # Errors
///
/// Returns [`SchemaError`] only if the protocol definition and the encoding
/// disagree, which the tests rule out.
pub fn graph_schema() -> Result<Schema, SchemaError> {
    super::graph_schema(&protocol())
}

/// Register the component GATs for JSON-LD with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_graph_instance(registry, "ThJsonLdSchema", "ThJsonLdInstance");
}
