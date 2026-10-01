//! Validation of instances against schemas, re-exported from
//! [`validation`](crate::validation).
//!
//! Every validator returns a [`ValidationReport`] with a three-valued
//! status. A tree's structural invariants (anchors declared, arcs declared
//! with coherent endpoints, the root present, every node reachable from it,
//! required edges present, the parent map agreeing with the arcs, fans well
//! formed) are the `NodeAnchor`, `EdgeDeclared`, `EndpointCoherence`,
//! `TreeRoot`, `TreeReachable`, `RequiredEdge`, `TreeParentMap` and
//! `FanWellFormed` checks of [`validate_wtype`].

pub use crate::validation::{
    ValidationPlan, ValidationReport, validate_graph, validate_table, validate_wtype,
};
