//! The segment validation boundary: contract errors precede reports, local
//! checks see boundary anchors, and non-local checks remain typed unknowns.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_schema::{
    ConstraintSemantics, InstanceShapeKind, Key, KeyPart, KeyRole, KeyScope, Protocol,
    SchemaBuilder,
};

use super::{TestResult, label};
use crate::fan_violation::FanViolation;
use crate::ginstance::{EdgeId, FanEmbedding, GEdge, HYPER_LABEL_KIND, HYPER_PARENT_KIND};
use crate::identity::Digest;
use crate::locality::{Locality, SummaryKind};
use crate::metadata::{Node, NodeShape};
use crate::segment::validate::check_validation_view_capacity;
use crate::segment::{
    AnySegment, Boundary, Ghost, GraphSegment, InStub, Partition, SegmentError, SegmentId,
    TableSegment, TreeSegment, validate_segment as validate_segment_with_external,
};
use crate::validation::{
    CheckClass, CheckKind, EndpointSide, ExternalCheckDecl, ExternalVerdict, ExternalVerdicts,
    IncompleteReason, ReportScope, Requirement, SchemaRef, SourceLocation, ValidationBudget,
    ValidationOptions, ValidationPlan, ValidationReport, Violation,
};
use crate::view_ref::{RowId, ViewRef};

fn schema() -> Result<(panproto_schema::Schema, Protocol), Box<dyn std::error::Error>> {
    let mut protocol = Protocol::default();
    protocol
        .constraint_semantics
        .insert("acyclic".to_owned(), ConstraintSemantics::Acyclic);
    let schema = SchemaBuilder::new(&protocol)
        .vertex("a", "object", None)?
        .vertex("b", "object", None)?
        .edge("a", "b", "prop", Some("b"))?
        .edge("a", "a", "prop", Some("loop"))?
        .constraint("a", "acyclic", "loop")
        .key(
            "a",
            Key {
                id: "id".into(),
                parts: vec![KeyPart::Path(Vec::new())],
                role: KeyRole::Unique,
                scope: KeyScope::Document,
            },
        )
        .hyper_edge(
            "rel",
            "relation",
            HashMap::from([("parent".to_owned(), "a".to_owned())]),
            "parent",
        )?
        .build()?;
    Ok((schema, protocol))
}

fn plan() -> Result<ValidationPlan, Box<dyn std::error::Error>> {
    let (schema, protocol) = schema()?;
    Ok(ValidationPlan::compile(
        &schema,
        &protocol,
        &ValidationOptions::default(),
    ))
}

/// Most segment tests exercise validation without application verdicts.
fn validate_segment(
    segment: &AnySegment,
    partition: &Partition,
    plan: &ValidationPlan,
    budget: &ValidationBudget,
) -> Result<ValidationReport, SegmentError> {
    validate_segment_with_external(
        segment,
        partition,
        plan,
        budget,
        &ExternalVerdicts::default(),
    )
}

fn assert_coverage_partitions(report: &ValidationReport) {
    for (key, coverage) in &report.coverage {
        assert_eq!(
            coverage.passed + coverage.failed + coverage.incomplete,
            coverage.instances,
            "coverage for {key:?}"
        );
    }
}

fn assert_every_non_local_instance_is_outside(report: &ValidationReport, plan: &ValidationPlan) {
    for (key, spec) in plan.checks() {
        let Some(locality) = spec.locality.get(&report.shape) else {
            continue;
        };
        if *locality == crate::locality::Locality::Local {
            continue;
        }
        let coverage = &report.coverage[key];
        let entries = report
            .incomplete
            .iter()
            .filter(|entry| {
                entry.check == *key
                    && entry.reason
                        == IncompleteReason::OutsideSegment {
                            locality: locality.clone(),
                        }
            })
            .count() as u64;
        assert_eq!(
            entries, coverage.instances,
            "non-local instances of {key:?}"
        );
        assert!(
            report.diagnostics.iter().all(|entry| entry.check != *key),
            "non-local check {key:?} produced a diagnostic"
        );
    }
}

#[test]
fn validate_segment_has_the_pinned_public_signature() {
    let _: fn(
        &AnySegment,
        &Partition,
        &ValidationPlan,
        &ValidationBudget,
        &ExternalVerdicts,
    ) -> Result<ValidationReport, SegmentError> = validate_segment_with_external;
}

#[test]
fn validation_view_capacity_accepts_the_full_edge_id_domain_and_refuses_the_next_edge() {
    let segment = SegmentId(7);
    let capacity = 1_u64 << u32::BITS;

    assert_eq!(check_validation_view_capacity(segment, capacity, 0), Ok(()));
    assert_eq!(
        check_validation_view_capacity(segment, capacity - 1, 1),
        Ok(())
    );
    assert_eq!(
        check_validation_view_capacity(segment, capacity, 1),
        Err(SegmentError::ValidationViewUnrepresentable {
            segment,
            edges: capacity + 1,
        })
    );
}

#[test]
fn segment_restriction_does_not_create_an_instance_for_an_undeclared_anchor() -> TestResult {
    let (schema, protocol) = schema()?;
    let mut options = ValidationOptions::default();
    options.external.insert(
        "application".into(),
        ExternalCheckDecl {
            name: "application".into(),
            anchors: BTreeSet::from(["unknown".into()]),
            requirement: Requirement::Required,
        },
    );
    let validation_plan = ValidationPlan::compile(&schema, &protocol, &options);
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[], 0, 1)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "unknown"))]),
        edges: BTreeMap::new(),
        boundary: Boundary::default(),
    });

    let report = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget::default(),
    )?;
    let Some(external) = report
        .coverage
        .iter()
        .find(|(key, _)| key.kind == CheckKind::External)
        .map(|(_, coverage)| coverage)
    else {
        panic!("the application check is compiled");
    };
    assert_eq!(external.instances, 0);
    assert!(
        report
            .incomplete
            .iter()
            .all(|entry| entry.check.kind != CheckKind::External)
    );
    Ok(())
}

#[test]
fn validate_segment_returns_every_contract_violation_before_validation() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[5, 10], 0, 15)?;
    let mut segment = GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "a")), (10, Node::new(10, "a"))]),
        edges: BTreeMap::from([(
            EdgeId(9),
            GEdge::new(0, 10, label("a", "a", "prop", Some("x"))),
        )]),
        boundary: Boundary::default(),
    };
    segment.boundary.in_stubs.insert(
        ViewRef::Edge(EdgeId(7)),
        InStub {
            src: 5,
            src_owner: SegmentId(0),
            src_anchor: "a".into(),
            tgt: 0,
            label: label("a", "a", "prop", None),
        },
    );

    assert_eq!(
        validate_segment(
            &AnySegment::Graph(segment),
            &partition,
            &plan()?,
            &ValidationBudget::default(),
        ),
        Err(SegmentError::Invalid {
            violations: vec![
                SegmentError::WrongOwner {
                    elem: ViewRef::Node(10),
                    expected: SegmentId(2),
                    found: SegmentId(0),
                },
                SegmentError::GhostMismatch { node: 10 },
                SegmentError::InStubMismatch {
                    elem: ViewRef::Edge(EdgeId(7)),
                },
            ],
        })
    );
    Ok(())
}

#[test]
fn graph_node_map_keys_must_equal_embedded_ids_in_the_same_owner() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[], 0, 2)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(1, "a"))]),
        edges: BTreeMap::new(),
        boundary: Boundary::default(),
    });

    assert_eq!(
        validate_segment(&segment, &partition, &plan()?, &ValidationBudget::default()),
        Err(SegmentError::Invalid {
            violations: vec![SegmentError::NodeIdMismatch { key: 0, found: 1 }],
        })
    );
    Ok(())
}

#[test]
fn tree_node_map_keys_must_equal_embedded_ids() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Tree, &[], 0, 2)?;
    let segment = AnySegment::Tree(TreeSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(1, "a"))]),
        arcs: BTreeMap::new(),
        fans: Vec::new(),
        root: Some(0),
        next_node: Some(2),
        next_edge: Some(0),
        boundary: Boundary::default(),
    });

    assert_eq!(
        validate_segment(&segment, &partition, &plan()?, &ValidationBudget::default()),
        Err(SegmentError::Invalid {
            violations: vec![SegmentError::NodeIdMismatch { key: 0, found: 1 }],
        })
    );
    Ok(())
}

#[test]
fn graph_node_map_keys_cannot_hide_a_cross_owner_embedded_id() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[5], 0, 10)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(5, "a"))]),
        edges: BTreeMap::from([(
            EdgeId(0),
            GEdge::new(0, 0, label("a", "a", "prop", Some("loop"))),
        )]),
        boundary: Boundary::default(),
    });

    // Before the key/id preflight, constructing the local graph re-keyed
    // the node as 5 and panicked while inserting the edge 0 -> 0.
    assert_eq!(
        validate_segment(&segment, &partition, &plan()?, &ValidationBudget::default()),
        Err(SegmentError::Invalid {
            violations: vec![SegmentError::NodeIdMismatch { key: 0, found: 5 }],
        })
    );
    Ok(())
}

#[test]
fn duplicate_embedded_node_ids_are_refused_by_the_map_preflight() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[], 0, 2)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "a")), (1, Node::new(0, "a"))]),
        edges: BTreeMap::new(),
        boundary: Boundary::default(),
    });

    assert_eq!(
        validate_segment(&segment, &partition, &plan()?, &ValidationBudget::default()),
        Err(SegmentError::Invalid {
            violations: vec![SegmentError::NodeIdMismatch { key: 1, found: 0 }],
        })
    );
    Ok(())
}

#[test]
fn graph_local_checks_use_ghost_anchors_but_report_only_owned_subjects() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[10], 0, 20)?;
    let declared = label("a", "b", "prop", Some("b"));
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "a"))]),
        edges: BTreeMap::from([(EdgeId(5), GEdge::new(0, 10, declared))]),
        boundary: Boundary {
            ghosts: BTreeMap::from([(
                10,
                Ghost {
                    node: 10,
                    owner: SegmentId(1),
                    anchor: "wrong".into(),
                },
            )]),
            in_stubs: BTreeMap::new(),
        },
    });
    let report = validate_segment(&segment, &partition, &plan()?, &ValidationBudget::default())?;

    assert_eq!(
        report.scope,
        ReportScope::Segment {
            segment: SegmentId(0)
        }
    );
    assert!(report.diagnostics.iter().any(|entry| {
        entry.check.kind == CheckKind::EndpointCoherence
            && entry.subject == Some(ViewRef::Edge(EdgeId(5)))
            && entry.violation
                == Violation::IncoherentEndpoint {
                    side: EndpointSide::Tgt,
                    expected: "b".into(),
                    found: "wrong".into(),
                }
    }));
    assert!(
        report
            .diagnostics
            .iter()
            .all(|entry| entry.subject != Some(ViewRef::Node(10)))
    );
    let Some(node_coverage) = report
        .coverage
        .iter()
        .find(|(key, _)| key.kind == CheckKind::NodeAnchor)
        .map(|(_, coverage)| coverage)
    else {
        panic!("NodeAnchor is structural");
    };
    assert_eq!(node_coverage.instances, 1, "the ghost is context only");
    assert_coverage_partitions(&report);
    assert_every_non_local_instance_is_outside(&report, &plan()?);

    // NodeAnchor(0), EdgeDeclared(5) and EndpointCoherence(5) read exactly
    // five owned/context elements. The ghost supplies the endpoint anchor,
    // but it is not charged as a second NodeAnchor instance.
    let bounded = validate_segment(
        &segment,
        &partition,
        &plan()?,
        &ValidationBudget { max_reads: Some(5) },
    )?;
    assert!(bounded.diagnostics.iter().any(|entry| {
        entry.check.kind == CheckKind::EndpointCoherence
            && entry.subject == Some(ViewRef::Edge(EdgeId(5)))
    }));
    assert_coverage_partitions(&bounded);
    Ok(())
}

#[test]
fn table_endpoint_coherence_reports_malformed_pairs_without_constructing_them() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Table, &[5], 0, 10)?;
    let source = SourceLocation {
        source: "schema.test".to_owned(),
        start_byte: 7,
        end_byte: 11,
    };
    let (schema, protocol) = schema()?;
    let mut options = ValidationOptions::default();
    options.sources.0.insert(SchemaRef::Schema, source.clone());
    let validation_plan = ValidationPlan::compile(&schema, &protocol, &options);
    let edge = label("a", "b", "missing", Some("b"));
    let pair = ViewRef::Pair {
        edge: edge.clone(),
        src: RowId(0),
        tgt: RowId(5),
    };
    let segment = AnySegment::Table(TableSegment {
        segment: SegmentId(0),
        rows: BTreeMap::from([(RowId(0), ("b".into(), HashMap::new()))]),
        pairs: BTreeSet::from([(edge.clone(), RowId(0), RowId(5))]),
        boundary: Boundary {
            ghosts: BTreeMap::from([(
                5,
                Ghost {
                    node: 5,
                    owner: SegmentId(1),
                    anchor: "a".into(),
                },
            )]),
            in_stubs: BTreeMap::new(),
        },
    });

    let report = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget::default(),
    )?;
    let found: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.check.kind == CheckKind::EndpointCoherence
                && diagnostic.subject == Some(pair.clone())
        })
        .map(|diagnostic| diagnostic.violation.clone())
        .collect();
    assert_eq!(
        found,
        vec![
            Violation::IncoherentEndpoint {
                side: EndpointSide::Src,
                expected: "a".into(),
                found: "b".into(),
            },
            Violation::IncoherentEndpoint {
                side: EndpointSide::Tgt,
                expected: "b".into(),
                found: "a".into(),
            },
        ]
    );
    assert!(report.diagnostics.iter().all(|diagnostic| {
        diagnostic.subject != Some(pair.clone()) || diagnostic.source == Some(source.clone())
    }));
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.check.kind == CheckKind::EdgeDeclared
            && diagnostic.subject == Some(pair.clone())
            && diagnostic.violation
                == Violation::UndeclaredEdge {
                    label: edge.clone(),
                }
    }));
    assert_coverage_partitions(&report);

    let bounded = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget { max_reads: Some(0) },
    )?;
    assert!(
        bounded.diagnostics.is_empty(),
        "no check instance was admitted"
    );
    assert!(bounded.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::EndpointCoherence
            && matches!(entry.reason, IncompleteReason::Budget { limit: 0, .. })
    }));
    assert_coverage_partitions(&bounded);
    Ok(())
}

fn two_malformed_table_pairs() -> Result<(Partition, AnySegment, ViewRef, ViewRef), SegmentError> {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Table, &[10], 0, 12)?;
    let first_edge = label("a", "b", "missing-a", Some("b"));
    let second_edge = label("a", "b", "missing-b", Some("b"));
    let first = ViewRef::Pair {
        edge: first_edge.clone(),
        src: RowId(0),
        tgt: RowId(10),
    };
    let second = ViewRef::Pair {
        edge: second_edge.clone(),
        src: RowId(1),
        tgt: RowId(11),
    };
    let segment = AnySegment::Table(TableSegment {
        segment: SegmentId(0),
        rows: BTreeMap::from([
            (RowId(0), ("b".into(), HashMap::new())),
            (RowId(1), ("b".into(), HashMap::new())),
        ]),
        pairs: BTreeSet::from([
            (first_edge.clone(), RowId(0), RowId(10)),
            (second_edge, RowId(1), RowId(11)),
        ]),
        boundary: Boundary {
            ghosts: BTreeMap::from([
                (
                    10,
                    Ghost {
                        node: 10,
                        owner: SegmentId(1),
                        anchor: "a".into(),
                    },
                ),
                (
                    11,
                    Ghost {
                        node: 11,
                        owner: SegmentId(1),
                        anchor: "a".into(),
                    },
                ),
            ]),
            in_stubs: BTreeMap::new(),
        },
    });
    Ok((partition, segment, first, second))
}

#[test]
fn table_direct_diagnostics_stop_at_the_exact_budget_cut() -> TestResult {
    let (partition, segment, first, second) = two_malformed_table_pairs()?;

    // Two NodeAnchor instances consume the first two reads. The first
    // EdgeDeclared instance is admitted by the third; the second and every
    // later instance remain beyond the global cut.
    let report = validate_segment(
        &segment,
        &partition,
        &plan()?,
        &ValidationBudget { max_reads: Some(3) },
    )?;
    let undeclared: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|entry| entry.check.kind == CheckKind::EdgeDeclared)
        .map(|entry| entry.subject.clone())
        .collect();
    assert_eq!(undeclared, vec![Some(first.clone())]);
    assert!(report.diagnostics.iter().all(|entry| {
        entry.subject != Some(second.clone()) && entry.check.kind != CheckKind::EndpointCoherence
    }));
    assert!(report.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::EdgeDeclared
            && matches!(
                entry.reason,
                IncompleteReason::Budget {
                    limit: 3,
                    spent: 3,
                    from: Some(ref at),
                } if *at == second
            )
    }));
    assert_coverage_partitions(&report);

    // Both one-read declaration checks fit after the two node checks, but
    // the first three-read endpoint check does not fit under six reads.
    let endpoint_cut = validate_segment(
        &segment,
        &partition,
        &plan()?,
        &ValidationBudget { max_reads: Some(6) },
    )?;
    assert!(
        endpoint_cut
            .diagnostics
            .iter()
            .all(|entry| entry.check.kind != CheckKind::EndpointCoherence)
    );
    assert!(endpoint_cut.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::EndpointCoherence
            && matches!(
                entry.reason,
                IncompleteReason::Budget {
                    limit: 6,
                    spent: 4,
                    from: Some(ref at),
                } if *at == first
            )
    }));

    // One more read admits the whole first endpoint footprint atomically;
    // the second endpoint instance is then the canonical cut point.
    let endpoint_admitted = validate_segment(
        &segment,
        &partition,
        &plan()?,
        &ValidationBudget { max_reads: Some(7) },
    )?;
    let endpoints: Vec<_> = endpoint_admitted
        .diagnostics
        .iter()
        .filter(|entry| entry.check.kind == CheckKind::EndpointCoherence)
        .map(|entry| entry.subject.clone())
        .collect();
    assert_eq!(endpoints, vec![Some(first.clone()), Some(first.clone())]);
    assert!(endpoint_admitted.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::EndpointCoherence
            && matches!(
                entry.reason,
                IncompleteReason::Budget {
                    limit: 7,
                    spent: 7,
                    from: Some(ref at),
                } if *at == second
            )
    }));
    assert_coverage_partitions(&endpoint_cut);
    assert_coverage_partitions(&endpoint_admitted);
    Ok(())
}

#[test]
fn finite_segment_budget_evaluates_only_admitted_check_instances() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[], 0, 2)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([
            (0, Node::new(0, "unknown-0")),
            (1, Node::new(1, "unknown-1")),
        ]),
        edges: BTreeMap::new(),
        boundary: Boundary::default(),
    });

    let report = validate_segment(
        &segment,
        &partition,
        &plan()?,
        &ValidationBudget { max_reads: Some(1) },
    )?;
    let diagnostics: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|entry| entry.check.kind == CheckKind::NodeAnchor)
        .map(|entry| entry.subject.clone())
        .collect();
    assert_eq!(diagnostics, vec![Some(ViewRef::Node(0))]);
    assert!(report.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::NodeAnchor
            && entry.subject.is_none()
            && entry.reason
                == IncompleteReason::Budget {
                    limit: 1,
                    spent: 1,
                    from: Some(ViewRef::Node(1)),
                }
    }));
    assert_coverage_partitions(&report);
    Ok(())
}

type GraphExternalFixture = (ValidationPlan, Partition, AnySegment);

fn graph_external_fixture() -> Result<GraphExternalFixture, Box<dyn std::error::Error>> {
    let (schema, protocol) = schema()?;
    let mut options = ValidationOptions::default();
    options.external.insert(
        "application".into(),
        ExternalCheckDecl {
            name: "application".into(),
            anchors: BTreeSet::from(["a".into()]),
            requirement: Requirement::Required,
        },
    );
    let validation_plan = ValidationPlan::compile(&schema, &protocol, &options);
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[10], 0, 20)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(1),
        nodes: BTreeMap::from([(10, Node::new(10, "a"))]),
        edges: BTreeMap::new(),
        boundary: Boundary {
            ghosts: BTreeMap::new(),
            in_stubs: BTreeMap::from([(
                ViewRef::Edge(EdgeId(7)),
                InStub {
                    src: 0,
                    src_owner: SegmentId(0),
                    src_anchor: "a".into(),
                    tgt: 10,
                    label: label("a", "a", "prop", Some("loop")),
                },
            )]),
        },
    });
    Ok((validation_plan, partition, segment))
}

#[test]
fn graph_external_budget_counts_in_stubs_as_context() -> TestResult {
    let (validation_plan, partition, segment) = graph_external_fixture()?;

    let cut = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget { max_reads: Some(3) },
    )?;
    assert!(cut.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::External
            && matches!(entry.reason, IncompleteReason::Budget { limit: 3, .. })
    }));
    assert!(cut.incomplete.iter().all(|entry| {
        entry.check.kind != CheckKind::External
            || !matches!(
                entry.reason,
                IncompleteReason::MissingExternalWitness { .. }
            )
    }));

    let admitted = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget { max_reads: Some(4) },
    )?;
    assert!(admitted.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::External
            && matches!(
                entry.reason,
                IncompleteReason::MissingExternalWitness { .. }
            )
    }));
    assert_coverage_partitions(&cut);
    assert_coverage_partitions(&admitted);
    Ok(())
}

#[test]
fn segment_external_checks_use_the_supplied_verdict_projection() -> TestResult {
    let (validation_plan, partition, segment) = graph_external_fixture()?;
    let witness = Digest([7; 32]);
    let mut external = ExternalVerdicts::default();
    external.insert(
        "application".into(),
        ViewRef::Node(10),
        ExternalVerdict {
            passed: false,
            witness,
        },
    );
    let rejected = validate_segment_with_external(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget::default(),
        &external,
    )?;
    assert!(rejected.diagnostics.iter().any(|entry| {
        entry.check.kind == CheckKind::External
            && entry.subject == Some(ViewRef::Node(10))
            && entry.violation == Violation::ExternalRejected { witness }
    }));
    assert!(rejected.incomplete.iter().all(|entry| {
        entry.check.kind != CheckKind::External
            || !matches!(
                entry.reason,
                IncompleteReason::MissingExternalWitness { .. }
            )
    }));

    external.insert(
        "application".into(),
        ViewRef::Node(10),
        ExternalVerdict {
            passed: true,
            witness,
        },
    );
    let passed = validate_segment_with_external(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget::default(),
        &external,
    )?;
    assert!(
        passed
            .diagnostics
            .iter()
            .all(|entry| entry.check.kind != CheckKind::External)
    );
    assert!(
        passed
            .incomplete
            .iter()
            .all(|entry| entry.check.kind != CheckKind::External)
    );
    let coverage = passed
        .coverage
        .iter()
        .find(|(key, _)| key.kind == CheckKind::External)
        .map(|(_, coverage)| coverage)
        .expect("the application check is compiled");
    assert_eq!(coverage.passed, 1);
    assert_coverage_partitions(&rejected);
    assert_coverage_partitions(&passed);
    Ok(())
}

#[test]
fn table_external_budget_counts_in_stubs_that_are_context_only() -> TestResult {
    let (schema, protocol) = schema()?;
    let mut options = ValidationOptions::default();
    options.external.insert(
        "application".into(),
        ExternalCheckDecl {
            name: "application".into(),
            anchors: BTreeSet::from(["a".into()]),
            requirement: Requirement::Required,
        },
    );
    let validation_plan = ValidationPlan::compile(&schema, &protocol, &options);
    let partition = Partition::by_node_ranges(InstanceShapeKind::Table, &[10], 0, 20)?;
    let edge = label("a", "a", "prop", Some("loop"));
    let at = ViewRef::Pair {
        edge: edge.clone(),
        src: RowId(0),
        tgt: RowId(10),
    };
    let segment = AnySegment::Table(TableSegment {
        segment: SegmentId(1),
        rows: BTreeMap::from([(RowId(10), ("a".into(), HashMap::new()))]),
        pairs: BTreeSet::new(),
        boundary: Boundary {
            ghosts: BTreeMap::new(),
            in_stubs: BTreeMap::from([(
                at,
                InStub {
                    src: 0,
                    src_owner: SegmentId(0),
                    src_anchor: "a".into(),
                    tgt: 10,
                    label: edge,
                },
            )]),
        },
    });

    let cut = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget { max_reads: Some(3) },
    )?;
    assert!(cut.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::External
            && matches!(entry.reason, IncompleteReason::Budget { limit: 3, .. })
    }));
    assert!(cut.incomplete.iter().all(|entry| {
        entry.check.kind != CheckKind::External
            || !matches!(
                entry.reason,
                IncompleteReason::MissingExternalWitness { .. }
            )
    }));

    let admitted = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget { max_reads: Some(4) },
    )?;
    assert!(admitted.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::External
            && matches!(
                entry.reason,
                IncompleteReason::MissingExternalWitness { .. }
            )
    }));
    assert_coverage_partitions(&cut);
    assert_coverage_partitions(&admitted);
    Ok(())
}

fn tree_with_foreign_fan_edge() -> Result<(AnySegment, Partition), SegmentError> {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Tree, &[10], 0, 20)?;
    let parent = label("rel", "a", HYPER_PARENT_KIND, None);
    let reserved_arc = label("a", "a", HYPER_LABEL_KIND, Some("ordinary"));
    let mut boundary = Boundary::default();
    boundary.in_stubs.insert(
        ViewRef::Edge(EdgeId(7)),
        InStub {
            src: 0,
            src_owner: SegmentId(0),
            src_anchor: "a".into(),
            tgt: 10,
            label: label("a", "rel", HYPER_LABEL_KIND, Some("incoming")),
        },
    );
    let segment = TreeSegment {
        segment: SegmentId(1),
        nodes: BTreeMap::from([
            (10, Node::new(10, "rel").with_shape(NodeShape::HyperEdge)),
            (11, Node::new(11, "a")),
            (12, Node::new(12, "a")),
        ]),
        arcs: BTreeMap::from([
            (EdgeId(0), (10, 11, parent)),
            (EdgeId(1), (11, 12, reserved_arc)),
        ]),
        fans: vec![FanEmbedding {
            node: 10,
            parent_edge: EdgeId(0),
            child_edges: BTreeMap::new(),
        }],
        root: Some(11),
        next_node: Some(13),
        next_edge: Some(2),
        boundary,
    };
    Ok((AnySegment::Tree(segment), partition))
}

#[test]
fn validate_segment_refuses_an_owned_ordinary_arc_incident_to_a_local_fan() -> TestResult {
    let (mut segment, partition) = tree_with_foreign_fan_edge()?;
    let AnySegment::Tree(tree) = &mut segment else {
        panic!("the fixture is a tree segment");
    };
    assert!(
        tree.arcs
            .insert(
                EdgeId(42),
                (11, 10, label("a", "rel", "prop", Some("foreign"))),
            )
            .is_none()
    );
    tree.next_edge = Some(43);

    assert_eq!(
        validate_segment(&segment, &partition, &plan()?, &ValidationBudget::default()),
        Err(SegmentError::Invalid {
            violations: vec![SegmentError::NotEmbedded {
                elem: ViewRef::Arc {
                    parent: 11,
                    child: 10,
                },
            }],
        })
    );
    Ok(())
}

#[test]
fn validate_segment_refuses_an_arc_keyed_in_stub_targeting_a_local_fan() -> TestResult {
    let (mut segment, partition) = tree_with_foreign_fan_edge()?;
    let AnySegment::Tree(tree) = &mut segment else {
        panic!("the fixture is a tree segment");
    };
    tree.boundary.in_stubs.insert(
        ViewRef::Arc {
            parent: 0,
            child: 10,
        },
        InStub {
            src: 0,
            src_owner: SegmentId(0),
            src_anchor: "a".into(),
            tgt: 10,
            label: label("a", "rel", "prop", Some("foreign")),
        },
    );

    assert_eq!(
        validate_segment(&segment, &partition, &plan()?, &ValidationBudget::default()),
        Err(SegmentError::Invalid {
            violations: vec![SegmentError::NotEmbedded {
                elem: ViewRef::Arc {
                    parent: 0,
                    child: 10,
                },
            }],
        })
    );
    Ok(())
}

#[test]
fn tree_ordinary_checks_use_the_native_forest_and_fans_use_the_reified_graph() -> TestResult {
    let (segment, partition) = tree_with_foreign_fan_edge()?;
    let report = validate_segment(&segment, &partition, &plan()?, &ValidationBudget::default())?;

    assert_eq!(
        report.scope,
        ReportScope::Segment {
            segment: SegmentId(1)
        }
    );
    assert!(report.diagnostics.iter().any(|entry| {
        entry.check.kind == CheckKind::FanWellFormed
            && entry.subject == Some(ViewRef::Node(10))
            && entry.violation
                == Violation::MalformedFan {
                    reason: FanViolation::ForeignIncidentEdge { edge: EdgeId(7) },
                    fan: None,
                }
    }));
    assert!(report.diagnostics.iter().any(|entry| {
        entry.check.kind == CheckKind::ReservedVocabulary
            && entry.subject
                == Some(ViewRef::Arc {
                    parent: 11,
                    child: 12,
                })
            && entry.violation == Violation::ReservedVocabulary
    }));
    assert!(report.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::TreeRoot
            && entry.subject.is_none()
            && matches!(entry.reason, IncompleteReason::OutsideSegment { .. })
    }));
    let reachable: Vec<_> = report
        .incomplete
        .iter()
        .filter(|entry| entry.check.kind == CheckKind::TreeReachable)
        .map(|entry| entry.subject.clone())
        .collect();
    assert_eq!(
        reachable,
        vec![Some(ViewRef::Node(11)), Some(ViewRef::Node(12))],
        "fan nodes are not logical tree-reachability subjects"
    );
    assert!(report.diagnostics.iter().all(|entry| {
        report
            .coverage
            .get(&entry.check)
            .is_some_and(|coverage| coverage.instances > 0)
    }));
    assert_coverage_partitions(&report);
    assert_every_non_local_instance_is_outside(&report, &plan()?);
    Ok(())
}

#[test]
fn tree_segment_refuses_two_owned_ordinary_arcs_into_one_child() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Tree, &[], 0, 2)?;
    let segment = AnySegment::Tree(TreeSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "a")), (1, Node::new(1, "a"))]),
        arcs: BTreeMap::from([
            (EdgeId(0), (0, 1, label("a", "a", "prop", Some("loop")))),
            (EdgeId(1), (0, 1, label("a", "a", "prop", Some("loop")))),
        ]),
        fans: Vec::new(),
        root: Some(0),
        next_node: Some(2),
        next_edge: Some(2),
        boundary: Boundary::default(),
    });

    assert_eq!(
        validate_segment(&segment, &partition, &plan()?, &ValidationBudget::default()),
        Err(SegmentError::Invalid {
            violations: vec![SegmentError::NotEmbedded {
                elem: ViewRef::Node(1),
            }],
        })
    );
    Ok(())
}

#[test]
fn tree_parent_uniqueness_counts_arc_keyed_in_stubs() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Tree, &[10], 0, 12)?;
    let incoming = label("a", "a", "prop", Some("loop"));
    let segment = AnySegment::Tree(TreeSegment {
        segment: SegmentId(1),
        nodes: BTreeMap::from([(10, Node::new(10, "a")), (11, Node::new(11, "a"))]),
        arcs: BTreeMap::from([(EdgeId(0), (10, 11, incoming.clone()))]),
        fans: Vec::new(),
        root: Some(10),
        next_node: Some(12),
        next_edge: Some(1),
        boundary: Boundary {
            ghosts: BTreeMap::new(),
            in_stubs: BTreeMap::from([(
                ViewRef::Arc {
                    parent: 0,
                    child: 11,
                },
                InStub {
                    src: 0,
                    src_owner: SegmentId(0),
                    src_anchor: "a".into(),
                    tgt: 11,
                    label: incoming,
                },
            )]),
        },
    });

    assert_eq!(
        validate_segment(&segment, &partition, &plan()?, &ValidationBudget::default()),
        Err(SegmentError::Invalid {
            violations: vec![SegmentError::NotEmbedded {
                elem: ViewRef::Node(11),
            }],
        })
    );
    Ok(())
}

#[test]
fn segment_reports_mark_non_local_checks_outside_segment() -> TestResult {
    let validation_plan = plan()?;
    let budget = ValidationBudget::default();

    let graph_partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[], 0, 1)?;
    let graph = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "a"))]),
        edges: BTreeMap::new(),
        boundary: Boundary::default(),
    });
    let table_partition = Partition::by_node_ranges(InstanceShapeKind::Table, &[], 0, 1)?;
    let table = AnySegment::Table(TableSegment {
        segment: SegmentId(0),
        rows: BTreeMap::from([(RowId(0), ("a".into(), HashMap::new()))]),
        pairs: BTreeSet::new(),
        boundary: Boundary::default(),
    });
    let tree_partition = Partition::by_node_ranges(InstanceShapeKind::Tree, &[], 0, 1)?;
    let tree = AnySegment::Tree(TreeSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "a"))]),
        arcs: BTreeMap::new(),
        fans: Vec::new(),
        root: Some(0),
        next_node: Some(1),
        next_edge: Some(0),
        boundary: Boundary::default(),
    });

    for (segment, partition, shape) in [
        (graph, graph_partition, InstanceShapeKind::Graph),
        (table, table_partition, InstanceShapeKind::Table),
        (tree, tree_partition, InstanceShapeKind::Tree),
    ] {
        let report = validate_segment(&segment, &partition, &validation_plan, &budget)?;
        assert_eq!(report.shape, shape);
        assert_eq!(
            report.scope,
            ReportScope::Segment {
                segment: SegmentId(0)
            }
        );
        assert_coverage_partitions(&report);
        assert_every_non_local_instance_is_outside(&report, &validation_plan);
    }
    Ok(())
}

#[test]
fn path_primary_required_value_is_non_local_at_a_segment_boundary() -> TestResult {
    let protocol = Protocol::default();
    let remote_value = label("a", "b", "prop", Some("b"));
    let schema = SchemaBuilder::new(&protocol)
        .vertex("a", "object", None)?
        .vertex("b", "string", None)?
        .edge("a", "b", "prop", Some("b"))?
        .key(
            "a",
            Key {
                id: "remote".into(),
                parts: vec![KeyPart::Path(vec![remote_value.clone()])],
                role: KeyRole::Primary,
                scope: KeyScope::Document,
            },
        )
        .build()?;
    let validation_plan =
        ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let (key, spec) = validation_plan
        .checks()
        .iter()
        .find(|(key, _)| {
            key.kind == CheckKind::RequiredValue
                && key.decl
                    == SchemaRef::Key {
                        vertex: "a".into(),
                        key: "remote".into(),
                    }
        })
        .expect("the primary key compiles a required-value check");
    assert_eq!(spec.class, CheckClass::MaintainedGlobal);
    assert_eq!(
        spec.locality.get(&InstanceShapeKind::Graph),
        Some(&Locality::Summary(SummaryKind::SortedMerge))
    );

    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[10], 0, 20)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "a"))]),
        edges: BTreeMap::from([(EdgeId(0), GEdge::new(0, 10, remote_value))]),
        boundary: Boundary {
            ghosts: BTreeMap::from([(
                10,
                Ghost {
                    node: 10,
                    owner: SegmentId(1),
                    anchor: "b".into(),
                },
            )]),
            in_stubs: BTreeMap::new(),
        },
    });
    let report = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget::default(),
    )?;

    assert!(report.diagnostics.iter().all(|entry| entry.check != *key));
    assert!(report.incomplete.iter().any(|entry| {
        entry.check == *key
            && entry.subject == Some(ViewRef::Node(0))
            && entry.reason
                == IncompleteReason::OutsideSegment {
                    locality: Locality::Summary(SummaryKind::SortedMerge),
                }
    }));
    assert_eq!(report.coverage[key].instances, 1);
    assert_eq!(report.coverage[key].incomplete, 1);
    assert_coverage_partitions(&report);
    Ok(())
}

#[test]
fn static_key_errors_precede_non_local_position_unknowns() -> TestResult {
    let protocol = Protocol::default();
    let within = label("root", "item", "ordered", Some("items"));
    let schema = SchemaBuilder::new(&protocol)
        .vertex("root", "object", None)?
        .vertex("item", "object", None)?
        .edge("root", "item", "ordered", Some("items"))?
        .key(
            "item",
            Key {
                id: "position".into(),
                parts: vec![KeyPart::Position { within }],
                role: KeyRole::Primary,
                scope: KeyScope::Document,
            },
        )
        .build()?;
    let validation_plan =
        ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let (key, spec) = validation_plan
        .checks()
        .iter()
        .find(|(key, _)| {
            key.kind == CheckKind::RequiredValue
                && key.decl
                    == SchemaRef::Key {
                        vertex: "item".into(),
                        key: "position".into(),
                    }
        })
        .expect("the primary position key compiles a required-value check");
    assert_eq!(spec.class, CheckClass::MaintainedGlobal);
    assert_eq!(
        spec.locality.get(&InstanceShapeKind::Graph),
        Some(&Locality::Summary(SummaryKind::SortedMerge))
    );
    assert!(validation_plan.is_statically_incomplete(key));
    let expected_static: Vec<_> = validation_plan
        .static_findings()
        .iter()
        .filter(|entry| entry.check == *key)
        .cloned()
        .collect();
    assert!(matches!(
        expected_static.as_slice(),
        [entry] if matches!(entry.reason, IncompleteReason::IllFormedDeclaration { .. })
    ));

    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[], 0, 1)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "item"))]),
        edges: BTreeMap::new(),
        boundary: Boundary::default(),
    });
    let report = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget::default(),
    )?;

    let actual: Vec<_> = report
        .incomplete
        .iter()
        .filter(|entry| entry.check == *key)
        .cloned()
        .collect();
    assert_eq!(actual, expected_static);
    assert_eq!(report.coverage[key].instances, 1);
    assert_eq!(report.coverage[key].incomplete, 1);
    assert_coverage_partitions(&report);
    Ok(())
}
