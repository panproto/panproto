//! Reusable L.3 identity, isomorphism, plan-compilation and fresh-id assertions.

#![allow(clippy::expect_used)]

use std::collections::BTreeSet;

use panproto_inst::identity::IdentityCarrier;
use panproto_inst::{
    CompiledMigration, EdgeId, GEdge, GInstance, GraphComplement, IdFloor, IdentityError,
    InstError, Node, PutDomainError, SynthKey,
};
use panproto_lens::graph_lens::{
    check_put_domain, fresh_view_edge_id, fresh_view_node_id, get, is_isomorphism, put,
    put_without_complement,
};
use panproto_lens::{Lens, LensError};
use panproto_reference::enumerate::lenses::{GraphPutComplementIdKind, GraphPutContractCase};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

fn edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "rel".into(),
        name: Some(name.into()),
    }
}

fn schema() -> Schema {
    SchemaBuilder::new(&Protocol::default())
        .vertex("a", "record", None)
        .expect("a")
        .vertex("b", "record", None)
        .expect("b")
        .edge("a", "b", "rel", Some("b"))
        .expect("edge")
        .build()
        .expect("schema")
}

fn projected_schema() -> Schema {
    SchemaBuilder::new(&Protocol::default())
        .vertex("a", "record", None)
        .expect("a")
        .build()
        .expect("projected schema")
}

fn identity_lens(schema: &Schema) -> Lens {
    Lens {
        compiled: CompiledMigration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.keys().cloned().collect(),
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    }
}

fn projection_lens() -> Lens {
    Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into()].into(),
            ..CompiledMigration::default()
        },
        src_schema: schema(),
        tgt_schema: projected_schema(),
    }
}

fn noncompiling_lens(missing_target: &str) -> Lens {
    let schema = schema();
    let source_edge = edge("a", "b", "b");
    Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into(),
            surviving_edges: [source_edge.clone()].into(),
            edge_remap: [(source_edge, edge("b", missing_target, "wrong"))].into(),
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema,
    }
}

const fn complement(floor: u32) -> GraphComplement {
    let mut complement = GraphComplement::empty();
    complement.id_floor = IdFloor {
        node: floor,
        edge: floor,
    };
    complement
}

fn complement_with_held_id(
    kind: GraphPutComplementIdKind,
    floor: u32,
    taken: u32,
) -> GraphComplement {
    let mut complement = complement(floor);
    match kind {
        GraphPutComplementIdKind::DroppedNode => {
            complement
                .dropped_nodes
                .insert(taken, Node::new(taken, "a"));
        }
        GraphPutComplementIdKind::DroppedEdge => {
            complement.dropped_edges.insert(
                EdgeId(taken),
                GEdge::new(taken, taken, edge("a", "a", "loop")),
            );
        }
        GraphPutComplementIdKind::SynthesizedNode => {
            complement.synthesized_nodes.insert(
                taken,
                SynthKey::Expand {
                    source_edge: EdgeId(taken),
                    hop: 0,
                },
            );
        }
        GraphPutComplementIdKind::SynthesizedEdge => {
            complement.synthesized_edges.insert(
                EdgeId(taken),
                SynthKey::Expand {
                    source_edge: EdgeId(taken),
                    hop: 0,
                },
            );
        }
    }
    complement
}

fn view_with_taken(taken: impl IntoIterator<Item = u32>) -> GInstance {
    let label = edge("a", "a", "loop");
    let mut view = GInstance::new();
    for id in taken {
        view.insert_node(Node::new(id, "a"))
            .expect("the reference carrier deduplicates ids");
        view.insert_edge(EdgeId(id), GEdge::new(id, id, label.clone()))
            .expect("the self-loop endpoints are present");
    }
    view
}

fn assert_plan_compilation_precedence(missing_target: &str) {
    let lens = noncompiling_lens(missing_target);
    let view = GInstance::new();
    let mut complement = GraphComplement::empty();
    complement.stamp(
        lens.identity(panproto_inst::ComplementShape::Graph),
        &lens.src_schema,
    );

    let get_error = get(&lens, &view).expect_err("get must compile the plan first");
    assert!(
        get_error.to_string().contains(missing_target),
        "get's compilation refusal must name the absent target `{missing_target}`: {get_error}"
    );
    let report = check_put_domain(&lens, &view, &complement)
        .expect_err("the invalid plan is outside every put domain");
    assert_eq!(report.len(), 1);
    let PutDomainError::PlanCompilation { detail } = &report[0] else {
        panic!("the first clause must be PlanCompilation: {report:?}");
    };
    assert!(
        detail.contains(missing_target),
        "the typed payload must name the absent target `{missing_target}`: {detail}"
    );
    let put_error = put(&lens, &view, &complement)
        .expect_err("put must return the same typed plan-domain refusal");
    match put_error {
        LensError::PutDomain(error) => assert_eq!(*error, report[0]),
        error => panic!("put returned a non-domain error: {error:?}"),
    }
}

fn assert_envelope_mismatch_precedes_domain(missing_target: &str) {
    let receiver = noncompiling_lens(missing_target);
    let foreign = identity_lens(&receiver.src_schema);
    let mut complement = GraphComplement::empty();
    complement.stamp(
        foreign.identity(panproto_inst::ComplementShape::Graph),
        &foreign.src_schema,
    );
    let view = GInstance::new();

    assert!(matches!(
        check_put_domain(&receiver, &view, &complement),
        Err(report) if matches!(report.first(), Some(PutDomainError::PlanCompilation { .. }))
    ));
    assert!(matches!(
        put(&receiver, &view, &complement),
        Err(LensError::Identity(IdentityError::Mismatch { .. }))
    ));
}

fn assert_data_bearing_unbound_complement() {
    let schema = schema();
    let lens = identity_lens(&schema);
    let mut complement = GraphComplement::empty();
    complement.dropped_nodes.insert(7, Node::new(7, "a"));
    assert!(matches!(
        put(&lens, &GInstance::new(), &complement),
        Err(LensError::Identity(IdentityError::UnboundComplement))
    ));
}

fn assert_complement_free_isomorphism() {
    let schema = schema();
    let lens = identity_lens(&schema);
    let view = GInstance::new()
        .with_node(Node::new(3, "a"))
        .with_node(Node::new(5, "b"))
        .with_edge_id(EdgeId(8), 3, 5, edge("a", "b", "b"));

    assert!(is_isomorphism(&lens));
    assert!(
        put_without_complement(&lens, &view)
            .as_ref()
            .is_ok_and(|restored| GInstance::identical(restored, &view))
    );
}

fn assert_complement_free_non_isomorphism() {
    let lens = projection_lens();
    let view = GInstance::new().with_node(Node::new(3, "a"));

    assert!(!is_isomorphism(&lens));
    assert!(matches!(
        put_without_complement(&lens, &view),
        Err(LensError::NotAnIsomorphism { .. })
    ));
}

fn assert_complement_held_id(kind: GraphPutComplementIdKind, floor: u32, taken: u32) {
    let complement = complement_with_held_id(kind, floor, taken);
    let view = GInstance::new();
    let successor = taken.checked_add(1).filter(|id| *id < u32::MAX);
    let (expected_node, expected_edge) = match kind {
        GraphPutComplementIdKind::DroppedNode | GraphPutComplementIdKind::SynthesizedNode => {
            (successor, Some(floor))
        }
        GraphPutComplementIdKind::DroppedEdge | GraphPutComplementIdKind::SynthesizedEdge => {
            (Some(floor), successor)
        }
    };

    let node = fresh_view_node_id(&view, &complement);
    if let Some(id) = expected_node {
        assert_eq!(node.as_ref().ok().copied(), Some(id));
    } else {
        assert!(matches!(node, Err(InstError::NodeIdsExhausted)));
    }
    let edge = fresh_view_edge_id(&view, &complement);
    if let Some(id) = expected_edge {
        assert_eq!(edge.as_ref().ok().copied(), Some(EdgeId(id)));
    } else {
        assert!(matches!(edge, Err(InstError::EdgeIdsExhausted)));
    }
}

/// Assert one independently enumerated L.3 contract boundary.
pub fn assert_graph_put_contract_case(case: GraphPutContractCase) {
    let (floor, taken, expected) = match case {
        GraphPutContractCase::PlanCompilation => {
            assert_plan_compilation_precedence("wrong");
            return;
        }
        GraphPutContractCase::EnvelopeMismatchPrecedesDomain => {
            assert_envelope_mismatch_precedes_domain("wrong");
            return;
        }
        GraphPutContractCase::DataBearingUnboundComplement => {
            assert_data_bearing_unbound_complement();
            return;
        }
        GraphPutContractCase::ComplementFreeIsomorphism => {
            assert_complement_free_isomorphism();
            return;
        }
        GraphPutContractCase::ComplementFreeNonIsomorphism => {
            assert_complement_free_non_isomorphism();
            return;
        }
        GraphPutContractCase::EmptyAtFloor(floor) => (floor, None, Some(floor)),
        GraphPutContractCase::TakenBelowFloor { floor, taken } => (floor, Some(taken), Some(floor)),
        GraphPutContractCase::TakenAtOrAboveFloor { floor, taken } => (
            floor,
            Some(taken),
            taken.checked_add(1).filter(|id| *id < u32::MAX),
        ),
        GraphPutContractCase::ExhaustedByTaken(taken) => (0, Some(taken), None),
        GraphPutContractCase::ComplementHeldAtOrAboveFloor { kind, floor, taken } => {
            assert_complement_held_id(kind, floor, taken);
            return;
        }
        GraphPutContractCase::ComplementHeldExhausted { kind, taken } => {
            assert_complement_held_id(kind, 0, taken);
            return;
        }
    };
    let view = view_with_taken(taken);
    let complement = complement(floor);
    let node = fresh_view_node_id(&view, &complement);
    let edge = fresh_view_edge_id(&view, &complement);
    if let Some(id) = expected {
        assert_eq!(node.as_ref().ok().copied(), Some(id));
        assert_eq!(edge.as_ref().ok().copied(), Some(EdgeId(id)));
    } else {
        assert!(matches!(node, Err(InstError::NodeIdsExhausted)));
        assert!(matches!(edge, Err(InstError::EdgeIdsExhausted)));
    }
}

/// Assert a generated L.3 case together with every named contract family.
#[allow(clippy::too_many_arguments)]
pub fn assert_randomized_graph_put_contract(
    floor: u32,
    taken: BTreeSet<u32>,
    dropped_nodes: BTreeSet<u32>,
    dropped_edges: BTreeSet<u32>,
    synthesized_nodes: BTreeSet<u32>,
    synthesized_edges: BTreeSet<u32>,
    below_delta: u16,
    above_delta: u16,
    missing_suffix: &str,
) {
    let expected_node = taken
        .iter()
        .chain(&dropped_nodes)
        .chain(&synthesized_nodes)
        .max()
        .map_or(floor, |largest| floor.max(largest + 1));
    let expected_edge = taken
        .iter()
        .chain(&dropped_edges)
        .chain(&synthesized_edges)
        .max()
        .map_or(floor, |largest| floor.max(largest + 1));
    let view = view_with_taken(taken);
    let mut complement = complement(floor);
    for id in dropped_nodes {
        complement.dropped_nodes.insert(id, Node::new(id, "a"));
    }
    for id in dropped_edges {
        complement
            .dropped_edges
            .insert(EdgeId(id), GEdge::new(id, id, edge("a", "a", "loop")));
    }
    for id in synthesized_nodes {
        complement.synthesized_nodes.insert(
            id,
            SynthKey::Expand {
                source_edge: EdgeId(id),
                hop: 0,
            },
        );
    }
    for id in synthesized_edges {
        complement.synthesized_edges.insert(
            EdgeId(id),
            SynthKey::Expand {
                source_edge: EdgeId(id),
                hop: 0,
            },
        );
    }
    assert_eq!(
        fresh_view_node_id(&view, &complement)
            .as_ref()
            .ok()
            .copied(),
        Some(expected_node),
    );
    assert_eq!(
        fresh_view_edge_id(&view, &complement)
            .as_ref()
            .ok()
            .copied(),
        Some(EdgeId(expected_edge)),
    );

    let missing_target = format!("missing-{missing_suffix}");
    assert_plan_compilation_precedence(&missing_target);
    assert_envelope_mismatch_precedes_domain(&missing_target);
    assert_data_bearing_unbound_complement();
    assert_complement_free_isomorphism();
    assert_complement_free_non_isomorphism();
    for empty_floor in [0, 1, u32::MAX - 1] {
        assert_graph_put_contract_case(GraphPutContractCase::EmptyAtFloor(empty_floor));
    }

    let ordinary_floor = floor + 2;
    let below = u32::from(below_delta) % ordinary_floor;
    assert_graph_put_contract_case(GraphPutContractCase::TakenBelowFloor {
        floor: ordinary_floor,
        taken: below,
    });
    assert_graph_put_contract_case(GraphPutContractCase::TakenAtOrAboveFloor {
        floor: ordinary_floor,
        taken: ordinary_floor,
    });
    assert_graph_put_contract_case(GraphPutContractCase::TakenAtOrAboveFloor {
        floor: ordinary_floor,
        taken: ordinary_floor + u32::from(above_delta),
    });
    for exhausted in [u32::MAX - 1, u32::MAX] {
        assert_graph_put_contract_case(GraphPutContractCase::ExhaustedByTaken(exhausted));
    }
    for kind in [
        GraphPutComplementIdKind::DroppedNode,
        GraphPutComplementIdKind::DroppedEdge,
        GraphPutComplementIdKind::SynthesizedNode,
        GraphPutComplementIdKind::SynthesizedEdge,
    ] {
        assert_graph_put_contract_case(GraphPutContractCase::ComplementHeldAtOrAboveFloor {
            kind,
            floor: ordinary_floor,
            taken: ordinary_floor + u32::from(above_delta),
        });
        assert_graph_put_contract_case(GraphPutContractCase::ComplementHeldExhausted {
            kind,
            taken: u32::MAX - 1,
        });
    }
}
