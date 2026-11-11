//! The CSV protocol.
//!
//! A CSV file is a sequence of records of fields, so its instances are
//! tables both as parsed and once resolved. The schema theory is the
//! constrained hypergraph and the instance theory `ThFunctor`.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::Protocol;

use crate::theories;

/// Returns the CSV protocol definition.
///
/// It constrains no vertex or edge kind and declares no constraint sort:
/// a CSV schema is built in code with the table and columns the data uses.
#[must_use]
pub fn protocol() -> Protocol {
    let mut protocol = Protocol {
        name: "csv".into(),
        schema_theory: "ThCsvSchema".into(),
        instance_theory: "ThCsvInstance".into(),
        has_order: true,
        ..Default::default()
    };
    crate::semantics::attach(&mut protocol);
    protocol
}

/// Register the component GATs for CSV with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_hypergraph_functor(registry, "ThCsvSchema", "ThCsvInstance");
}
