//! The TOML protocol.
//!
//! A TOML document is a tree of tables, arrays and scalar values, and its
//! instances are trees both as parsed and once resolved. The schema theory
//! is the constrained multigraph and the instance theory `ThWType`.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::Protocol;

use crate::theories;

/// Returns the TOML protocol definition.
///
/// It constrains no vertex or edge kind and declares no constraint sort:
/// a TOML schema is built in code with the kinds the data uses.
#[must_use]
pub fn protocol() -> Protocol {
    let mut protocol = Protocol {
        name: "toml".into(),
        schema_theory: "ThTomlSchema".into(),
        instance_theory: "ThTomlInstance".into(),
        has_order: true,
        ..Default::default()
    };
    crate::semantics::attach(&mut protocol);
    protocol
}

/// Register the component GATs for TOML with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(registry, "ThTomlSchema", "ThTomlInstance");
}
