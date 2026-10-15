//! The YAML protocol.
//!
//! YAML's syntax is a tree of mappings, sequences and scalars, and its
//! anchors and aliases let one node be reached from several places, so its
//! instances parse as trees and are graphs once aliases are resolved. The
//! schema theory is the constrained multigraph and the instance theory
//! `ThWType`.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::Protocol;

use crate::theories;

/// Returns the YAML protocol definition.
///
/// It constrains no vertex or edge kind and declares no constraint sort:
/// a YAML schema is built in code with the kinds the data uses.
#[must_use]
pub fn protocol() -> Protocol {
    let mut protocol = Protocol {
        name: "yaml".into(),
        schema_theory: "ThYamlSchema".into(),
        instance_theory: "ThYamlInstance".into(),
        has_order: true,
        ..Default::default()
    };
    crate::semantics::attach(&mut protocol);
    protocol
}

/// Register the component GATs for YAML with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(registry, "ThYamlSchema", "ThYamlInstance");
}
