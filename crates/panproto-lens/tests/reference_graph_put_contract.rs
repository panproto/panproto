//! Independent plan-compilation and fresh-id boundary evidence for L.3.

#![allow(clippy::expect_used)]

use std::collections::BTreeSet;

use panproto_inst::identity::IdentityCarrier;
use panproto_inst::{
    CompiledMigration, EdgeId, GEdge, GInstance, GraphComplement, IdFloor, InstError, Node,
    PutDomainError,
};
use panproto_lens::graph_lens::{
    check_put_domain, fresh_view_edge_id, fresh_view_node_id, get, put,
};
use panproto_lens::{Lens, LensError};
use panproto_reference::enumerate::lenses::{
    GraphPutContractCase, for_each_graph_put_contract_case,
    graph_put_contract_case_count_certificate,
};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};
use proptest::prelude::*;

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

fn noncompiling_lens() -> Lens {
    let schema = schema();
    let source_edge = edge("a", "b", "b");
    Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into(),
            surviving_edges: [source_edge.clone()].into(),
            edge_remap: [(source_edge, edge("b", "a", "wrong"))].into(),
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

fn assert_plan_compilation_precedence() {
    let lens = noncompiling_lens();
    let view = GInstance::new();
    let mut complement = GraphComplement::empty();
    complement.stamp(
        lens.identity(panproto_inst::ComplementShape::Graph),
        &lens.src_schema,
    );

    assert!(
        get(&lens, &view).is_err(),
        "get must compile the plan first"
    );
    let report = check_put_domain(&lens, &view, &complement)
        .expect_err("the invalid plan is outside every put domain");
    assert_eq!(report.len(), 1);
    assert!(matches!(
        report.first(),
        Some(PutDomainError::PlanCompilation { detail }) if !detail.is_empty()
    ));
    assert!(matches!(
        put(&lens, &view, &complement),
        Err(LensError::PutDomain(error))
            if matches!(*error, PutDomainError::PlanCompilation { ref detail } if !detail.is_empty())
    ));
}

fn assert_fresh_case(case: GraphPutContractCase) {
    let (floor, taken, expected) = match case {
        GraphPutContractCase::PlanCompilation => {
            assert_plan_compilation_precedence();
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

#[test]
fn graph_put_contract_certificate() {
    let mut count = 0;
    for_each_graph_put_contract_case(|case| {
        count += 1;
        assert_fresh_case(case);
    });
    assert_eq!(count, graph_put_contract_case_count_certificate());
    assert_eq!(count, 9);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_graph_put_contract(
        floor in 0_u16..=4_000,
        taken in proptest::collection::vec(0_u16..=4_000, 0..8),
    ) {
        let floor = u32::from(floor);
        let taken: BTreeSet<u32> = taken.into_iter().map(u32::from).collect();
        let expected = taken.iter().next_back().map_or(floor, |largest| {
            floor.max(largest + 1)
        });
        let view = view_with_taken(taken.iter().copied());
        let complement = complement(floor);
        prop_assert_eq!(
            fresh_view_node_id(&view, &complement).as_ref().ok().copied(),
            Some(expected),
        );
        prop_assert_eq!(
            fresh_view_edge_id(&view, &complement).as_ref().ok().copied(),
            Some(EdgeId(expected)),
        );
    }
}
