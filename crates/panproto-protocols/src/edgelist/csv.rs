//! The CSV edge-list protocol.
//!
//! An `edgelist-csv` document is an edge list written as RFC 4180 CSV with
//! a header record, and it denotes a directed multigraph, so its instances
//! are graphs both as parsed and once resolved. The schema theory is the
//! constrained multigraph and the instance theory `ThGraphInstance`; the
//! instance schema is the edge-list graph encoding of [`graph_schema`].

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{Protocol, Schema, SchemaError};

use crate::theories;

/// Returns the CSV edge-list protocol definition.
///
/// Its vertex kinds and edge rule are those of the edge-list graph
/// encoding.
#[must_use]
pub fn protocol() -> Protocol {
    let mut protocol = Protocol {
        name: "edgelist-csv".into(),
        schema_theory: "ThEdgeListCsvSchema".into(),
        instance_theory: "ThEdgeListCsvInstance".into(),
        edge_rules: super::edge_rules(),
        obj_kinds: super::obj_kinds(),
        ..Default::default()
    };
    crate::semantics::attach(&mut protocol);
    protocol
}

/// The schema every CSV edge list is read under: the edge-list graph
/// encoding.
///
/// # Errors
///
/// Returns [`SchemaError`] only if the protocol definition and the encoding
/// disagree, which the tests rule out.
pub fn graph_schema() -> Result<Schema, SchemaError> {
    super::graph_schema(&protocol())
}

/// Register the component GATs for CSV edge lists with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_graph_instance(
        registry,
        "ThEdgeListCsvSchema",
        "ThEdgeListCsvInstance",
    );
}
