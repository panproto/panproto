//! The TSV protocol.
//!
//! A TSV file (IANA `text/tab-separated-values`) is a sequence of records
//! of tab-separated fields with no quoting, so its instances are tables
//! both as parsed and once resolved. The schema theory is the constrained
//! hypergraph and the instance theory `ThFunctor`, as for CSV.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::Protocol;

use crate::theories;

/// Returns the TSV protocol definition.
///
/// It constrains no vertex or edge kind and declares no constraint sort:
/// a TSV schema is built in code with the table and columns the data uses.
#[must_use]
pub fn protocol() -> Protocol {
    let mut protocol = Protocol {
        name: "tsv".into(),
        schema_theory: "ThTsvSchema".into(),
        instance_theory: "ThTsvInstance".into(),
        has_order: true,
        ..Default::default()
    };
    crate::semantics::attach(&mut protocol);
    protocol
}

/// Register the component GATs for TSV with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_hypergraph_functor(registry, "ThTsvSchema", "ThTsvInstance");
}
