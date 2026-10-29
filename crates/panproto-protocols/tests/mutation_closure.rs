//! Every diagram the theory registration composes has a sound directed
//! rewrite system, so the registration gate admits each of them: the
//! soundness analysis completes and reports no non-joining critical pair
//! and no termination violation.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_gat::validate_rewrite_system;
use panproto_protocols::theories::{
    RegistrationDiagram, constrained_hypergraph_diagram, constrained_multigraph_diagram,
    constrained_simple_graph_diagram, th_flat, th_functor, th_graph_instance, th_wtype,
    typed_graph_diagram, wtype_meta_diagram,
};

/// The diagrams the registration helpers compose: the four schema
/// diagrams, the instance diagram of the multigraph-with-meta group, and
/// the four single-block instance diagrams.
fn registration_diagrams() -> Vec<(&'static str, RegistrationDiagram)> {
    vec![
        ("constrained_multigraph", constrained_multigraph_diagram()),
        ("constrained_hypergraph", constrained_hypergraph_diagram()),
        (
            "constrained_simple_graph",
            constrained_simple_graph_diagram(),
        ),
        ("typed_graph", typed_graph_diagram()),
        ("wtype_meta", wtype_meta_diagram("ThNodeValue")),
        ("wtype", RegistrationDiagram::single(th_wtype())),
        ("functor", RegistrationDiagram::single(th_functor())),
        ("flat", RegistrationDiagram::single(th_flat())),
        (
            "graph_instance",
            RegistrationDiagram::single(th_graph_instance()),
        ),
    ]
}

#[test]
fn every_registration_diagram_composes_to_a_sound_rewrite_system() {
    for (label, diagram) in registration_diagrams() {
        for result_name in ["ThClosureSchema", "ThClosureInstance"] {
            let composition = diagram
                .compose(result_name)
                .unwrap_or_else(|e| panic!("{label}: {e}"));
            let report = validate_rewrite_system(&composition.theory)
                .unwrap_or_else(|e| panic!("{label}: {e}"));
            assert_eq!(report.warnings(), Vec::<String>::new(), "{label}");
        }
    }
}
