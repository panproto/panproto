//! The segment validation boundary: contract errors precede reports, local
//! checks see boundary anchors, and non-local checks remain typed unknowns.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_schema::{
    ConstraintSemantics, InstanceShapeKind, Key, KeyPart, KeyRole, KeyScope, LengthUnit, Protocol,
    SchemaBuilder, Variant,
};
use proptest::prelude::*;

use super::{
    TestResult, arb_finstance, arb_ginstance, arb_partition, arb_winstance, interleaved, label,
    next_free,
};
use crate::FanViolation;
use crate::fan::Fan;
use crate::functor::FInstance;
use crate::ginstance::{
    EdgeId, FanEmbedding, GEdge, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND,
};
use crate::identity::Digest;
use crate::locality::{Locality, SummaryKind};
use crate::metadata::{Node, NodeShape};
use crate::segment::validate::{
    check_validation_view_capacity, validate_segment_with_edge_capacity,
};
use crate::segment::{
    AnySegment, Boundary, Ghost, GraphSegment, InStub, Partition, SegmentError, SegmentId,
    TableSegment, TreeSegment, partition_graph, partition_table, partition_tree,
    validate_segment as validate_segment_with_external,
};
use crate::validation::{
    CheckClass, CheckCoverage, CheckKey, CheckKind, EndpointSide, ExternalCheckDecl,
    ExternalVerdict, ExternalVerdicts, IncompleteReason, ReportScope, Requirement, SchemaRef,
    SourceLocation, ValidationBudget, ValidationOptions, ValidationPlan, ValidationReport,
    Violation,
};
use crate::value::{FieldPresence, Value};
use crate::view_ref::{RowId, ViewRef};
use crate::wtype::WInstance;

fn coverage_where<'a>(
    report: &'a ValidationReport,
    predicate: impl Fn(&CheckKey) -> bool,
    context: &str,
) -> &'a CheckCoverage {
    let Some((_, coverage)) = report.coverage.iter().find(|(key, _)| predicate(key)) else {
        panic!("{context}");
    };
    coverage
}

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

    assert_eq!(
        check_validation_view_capacity(segment, capacity, 0, capacity),
        Ok(())
    );
    assert_eq!(
        check_validation_view_capacity(segment, capacity - 1, 1, capacity),
        Ok(())
    );
    assert_eq!(
        check_validation_view_capacity(segment, capacity, 1, capacity),
        Err(SegmentError::ValidationViewUnrepresentable {
            segment,
            edges: capacity + 1,
        })
    );
}

#[test]
fn validate_segment_runs_the_capacity_preflight_before_constructing_the_view() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[10], 0, 20)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "a"))]),
        edges: BTreeMap::from([(
            EdgeId(0),
            GEdge::new(0, 0, label("a", "a", "prop", Some("loop"))),
        )]),
        boundary: Boundary {
            ghosts: BTreeMap::new(),
            in_stubs: BTreeMap::from([(
                ViewRef::Edge(EdgeId(1)),
                InStub {
                    src: 10,
                    src_owner: SegmentId(1),
                    src_anchor: "a".into(),
                    tgt: 0,
                    label: label("a", "a", "prop", Some("loop")),
                },
            )]),
        },
    });

    assert_eq!(
        validate_segment_with_edge_capacity(
            &segment,
            &partition,
            &plan()?,
            &ValidationBudget::default(),
            &ExternalVerdicts::default(),
            1,
        ),
        Err(SegmentError::ValidationViewUnrepresentable {
            segment: SegmentId(0),
            edges: 2,
        })
    );
    Ok(())
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

    // NodeAnchor(0) consumes the only read, so the original EdgeId(5) is
    // the first unpaid subject. The graph carrier internally reindexes this
    // edge to zero; neither the per-key cut nor its reason may leak that id.
    let edge_cut = validate_segment(
        &segment,
        &partition,
        &plan()?,
        &ValidationBudget { max_reads: Some(1) },
    )?;
    assert!(edge_cut.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::EdgeDeclared
            && entry.subject.is_none()
            && entry.reason
                == IncompleteReason::Budget {
                    limit: 1,
                    spent: 1,
                    from: Some(ViewRef::Edge(EdgeId(5))),
                }
    }));
    assert!(edge_cut.incomplete.iter().all(|entry| {
        entry.subject != Some(ViewRef::Edge(EdgeId(0)))
            && !matches!(
                entry.reason,
                IncompleteReason::Budget {
                    from: Some(ViewRef::Edge(EdgeId(0))),
                    ..
                }
            )
    }));
    assert_coverage_partitions(&edge_cut);
    Ok(())
}

#[test]
fn graph_fan_violations_remap_an_edge_keyed_in_stub_id() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[10], 0, 11)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(1),
        nodes: BTreeMap::from([(10, Node::new(10, "rel").with_shape(NodeShape::HyperEdge))]),
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
                    label: label("a", "rel", "prop", Some("fan")),
                },
            )]),
        },
    });

    let report = validate_segment(&segment, &partition, &plan()?, &ValidationBudget::default())?;
    assert!(report.diagnostics.iter().any(|entry| {
        entry.check.kind == CheckKind::FanWellFormed
            && entry.subject == Some(ViewRef::Node(10))
            && entry.violation
                == Violation::MalformedFan {
                    reason: FanViolation::ForeignIncidentEdge { edge: EdgeId(7) },
                    fan: None,
                }
    }));
    assert!(report.diagnostics.iter().all(|entry| {
        entry.violation
            != Violation::MalformedFan {
                reason: FanViolation::ForeignIncidentEdge { edge: EdgeId(0) },
                fan: None,
            }
    }));
    assert_coverage_partitions(&report);
    Ok(())
}

#[test]
fn graph_reserved_edge_from_an_ordinary_node_is_a_fan_subject() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[], 0, 2)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "a")), (1, Node::new(1, "a"))]),
        edges: BTreeMap::from([(
            EdgeId(5),
            GEdge::new(0, 1, label("a", "a", HYPER_LABEL_KIND, Some("rogue"))),
        )]),
        boundary: Boundary::default(),
    });

    let report = validate_segment(&segment, &partition, &plan()?, &ValidationBudget::default())?;
    let fan_diagnostics: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|entry| entry.check.kind == CheckKind::FanWellFormed)
        .map(|entry| (entry.subject.clone(), entry.violation.clone()))
        .collect();
    assert_eq!(
        fan_diagnostics,
        vec![(
            Some(ViewRef::Node(0)),
            Violation::MalformedFan {
                reason: FanViolation::ReservedKindOutsideFan { edge: EdgeId(5) },
                fan: None,
            },
        )]
    );
    let fan_coverage = coverage_where(
        &report,
        |key| key.kind == CheckKind::FanWellFormed,
        "the graph plan compiles fan well-formedness",
    );
    assert_eq!(fan_coverage.instances, 1);
    assert_eq!(fan_coverage.passed, 0);
    assert_eq!(fan_coverage.failed, 1);
    assert_eq!(fan_coverage.incomplete, 0);
    assert_coverage_partitions(&report);
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

#[test]
fn table_endpoint_coherence_keeps_a_one_sided_mismatch_out_of_the_functor_view() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Table, &[5], 0, 10)?;
    let edge = label("a", "b", "prop", Some("b"));
    let pair = ViewRef::Pair {
        edge: edge.clone(),
        src: RowId(0),
        tgt: RowId(5),
    };
    let segment = AnySegment::Table(TableSegment {
        segment: SegmentId(0),
        rows: BTreeMap::from([(RowId(0), ("a".into(), HashMap::new()))]),
        pairs: BTreeSet::from([(edge, RowId(0), RowId(5))]),
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

    let report = validate_segment(&segment, &partition, &plan()?, &ValidationBudget::default())?;
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
        vec![Violation::IncoherentEndpoint {
            side: EndpointSide::Tgt,
            expected: "b".into(),
            found: "a".into(),
        }]
    );
    assert_coverage_partitions(&report);
    Ok(())
}

#[test]
fn a_malformed_table_pair_does_not_satisfy_a_required_edge() -> TestResult {
    let protocol = Protocol::default();
    let required = label("a", "b", "prop", Some("b"));
    let schema = SchemaBuilder::new(&protocol)
        .vertex("a", "object", None)?
        .vertex("b", "object", None)?
        .edge("a", "b", "prop", Some("b"))?
        .required("a", vec![required.clone()])
        .build()?;
    let validation_plan =
        ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let partition = Partition::by_node_ranges(InstanceShapeKind::Table, &[5], 0, 10)?;
    let pair = ViewRef::Pair {
        edge: required.clone(),
        src: RowId(0),
        tgt: RowId(5),
    };
    let segment = AnySegment::Table(TableSegment {
        segment: SegmentId(0),
        rows: BTreeMap::from([(RowId(0), ("a".into(), HashMap::new()))]),
        pairs: BTreeSet::from([(required.clone(), RowId(0), RowId(5))]),
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
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.check.kind == CheckKind::EndpointCoherence
            && diagnostic.subject == Some(pair.clone())
            && diagnostic.violation
                == Violation::IncoherentEndpoint {
                    side: EndpointSide::Tgt,
                    expected: "b".into(),
                    found: "a".into(),
                }
    }));
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.check.kind == CheckKind::RequiredEdge
            && diagnostic.subject == Some(ViewRef::Row(RowId(0)))
            && diagnostic.violation
                == Violation::MissingRequiredEdge {
                    edge: required.clone(),
                }
    }));
    let required_coverage = coverage_where(
        &report,
        |key| key.kind == CheckKind::RequiredEdge,
        "the required edge compiles one check",
    );
    assert_eq!(required_coverage.instances, 1);
    assert_eq!(required_coverage.passed, 0);
    assert_eq!(required_coverage.failed, 1);
    assert_eq!(required_coverage.incomplete, 0);
    assert_coverage_partitions(&report);
    Ok(())
}

#[test]
fn a_malformed_property_pair_does_not_materialize_a_required_value() -> TestResult {
    let mut protocol = Protocol::default();
    protocol
        .constraint_semantics
        .insert("required".into(), ConstraintSemantics::RequiredValue);
    let property = label("source", "value", "prop", Some("value"));
    let schema = SchemaBuilder::new(&protocol)
        .vertex("source", "object", None)?
        .vertex("value", "string", None)?
        .vertex("other", "string", None)?
        .edge("source", "value", "prop", Some("value"))?
        .constraint("value", "required", "true")
        .build()?;
    let validation_plan =
        ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let partition = Partition::by_node_ranges(InstanceShapeKind::Table, &[5], 0, 10)?;
    let pair = ViewRef::Pair {
        edge: property.clone(),
        src: RowId(0),
        tgt: RowId(5),
    };
    let segment = AnySegment::Table(TableSegment {
        segment: SegmentId(0),
        rows: BTreeMap::from([(RowId(0), ("source".into(), HashMap::new()))]),
        pairs: BTreeSet::from([(property, RowId(0), RowId(5))]),
        boundary: Boundary {
            ghosts: BTreeMap::from([(
                5,
                Ghost {
                    node: 5,
                    owner: SegmentId(1),
                    anchor: "other".into(),
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
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.check.kind == CheckKind::EndpointCoherence
            && diagnostic.subject == Some(pair.clone())
            && diagnostic.violation
                == Violation::IncoherentEndpoint {
                    side: EndpointSide::Tgt,
                    expected: "value".into(),
                    found: "other".into(),
                }
    }));
    let required_diagnostics: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.check.kind == CheckKind::RequiredValue)
        .map(|diagnostic| (diagnostic.subject.clone(), diagnostic.violation.clone()))
        .collect();
    assert_eq!(
        required_diagnostics,
        vec![(
            Some(ViewRef::Row(RowId(0))),
            Violation::MissingValue {
                field: Some("value".into()),
            },
        )]
    );
    let required_coverage = coverage_where(
        &report,
        |key| key.kind == CheckKind::RequiredValue,
        "the constraint compiles one required-value check",
    );
    assert_eq!(required_coverage.instances, 1);
    assert_eq!(required_coverage.passed, 0);
    assert_eq!(required_coverage.failed, 1);
    assert_eq!(required_coverage.incomplete, 0);
    assert_coverage_partitions(&report);
    Ok(())
}

#[test]
fn table_endpoint_coherence_accepts_a_one_way_union_family_member() -> TestResult {
    let protocol = Protocol::default();
    let mut schema = SchemaBuilder::new(&protocol)
        .vertex("union", "object", None)?
        .vertex("arm", "object", None)?
        .vertex("b", "object", None)?
        .edge("union", "b", "prop", Some("b"))?
        .build()?;
    schema.variants.insert(
        "union".into(),
        vec![Variant {
            id: "arm".into(),
            parent_vertex: "union".into(),
            tag: Some("arm".into()),
        }],
    );
    let validation_plan =
        ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let edge = label("union", "b", "prop", Some("b"));
    let pair = ViewRef::Pair {
        edge: edge.clone(),
        src: RowId(0),
        tgt: RowId(1),
    };
    let partition = Partition::by_node_ranges(InstanceShapeKind::Table, &[], 0, 2)?;
    let segment = AnySegment::Table(TableSegment {
        segment: SegmentId(0),
        rows: BTreeMap::from([
            (RowId(0), ("arm".into(), HashMap::new())),
            (RowId(1), ("b".into(), HashMap::new())),
        ]),
        pairs: BTreeSet::from([(edge, RowId(0), RowId(1))]),
        boundary: Boundary::default(),
    });

    let report = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget::default(),
    )?;
    assert!(report.diagnostics.iter().all(|diagnostic| {
        diagnostic.check.kind != CheckKind::EndpointCoherence
            || diagnostic.subject != Some(pair.clone())
    }));
    let endpoint_coverage = coverage_where(
        &report,
        |key| key.kind == CheckKind::EndpointCoherence,
        "endpoint coherence is structural",
    );
    assert_eq!(endpoint_coverage.instances, 1);
    assert_eq!(endpoint_coverage.passed, 1);
    assert_eq!(endpoint_coverage.failed, 0);
    assert_eq!(endpoint_coverage.incomplete, 0);
    assert_coverage_partitions(&report);
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
            (first_edge, RowId(0), RowId(10)),
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
    assert_eq!(endpoints, vec![Some(first.clone()), Some(first)]);
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
fn an_exhausted_table_prefix_records_later_local_instances() -> TestResult {
    let protocol = Protocol::default();
    let required = label("a", "b", "prop", Some("b"));
    let schema = SchemaBuilder::new(&protocol)
        .vertex("a", "object", None)?
        .vertex("b", "object", None)?
        .edge("a", "b", "prop", Some("b"))?
        .required("a", vec![required])
        .build()?;
    let validation_plan =
        ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let partition = Partition::by_node_ranges(InstanceShapeKind::Table, &[], 0, 1)?;
    let segment = AnySegment::Table(TableSegment {
        segment: SegmentId(0),
        rows: BTreeMap::from([(RowId(0), ("a".into(), HashMap::new()))]),
        pairs: BTreeSet::new(),
        boundary: Boundary::default(),
    });

    let report = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget { max_reads: Some(0) },
    )?;
    assert!(report.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::NodeAnchor
            && entry.subject.is_none()
            && entry.reason
                == IncompleteReason::Budget {
                    limit: 0,
                    spent: 0,
                    from: Some(ViewRef::Row(RowId(0))),
                }
    }));
    assert!(report.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::RequiredEdge
            && entry.subject.is_none()
            && entry.reason
                == IncompleteReason::Budget {
                    limit: 0,
                    spent: 0,
                    from: Some(ViewRef::Row(RowId(0))),
                }
    }));
    assert_coverage_partitions(&report);
    Ok(())
}

#[test]
fn a_representable_table_pair_satisfies_its_required_edge() -> TestResult {
    let protocol = Protocol::default();
    let required = label("a", "b", "prop", Some("b"));
    let schema = SchemaBuilder::new(&protocol)
        .vertex("a", "object", None)?
        .vertex("b", "object", None)?
        .edge("a", "b", "prop", Some("b"))?
        .required("a", vec![required.clone()])
        .build()?;
    let validation_plan =
        ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let partition = Partition::by_node_ranges(InstanceShapeKind::Table, &[], 0, 2)?;
    let segment = AnySegment::Table(TableSegment {
        segment: SegmentId(0),
        rows: BTreeMap::from([
            (RowId(0), ("a".into(), HashMap::new())),
            (RowId(1), ("b".into(), HashMap::new())),
        ]),
        pairs: BTreeSet::from([(required, RowId(0), RowId(1))]),
        boundary: Boundary::default(),
    });

    let report = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget::default(),
    )?;
    assert!(
        report
            .diagnostics
            .iter()
            .all(|entry| !matches!(entry.violation, Violation::MissingRequiredEdge { .. }))
    );
    let required_coverage = coverage_where(
        &report,
        |key| key.kind == CheckKind::RequiredEdge,
        "the required edge compiles one check",
    );
    assert_eq!(required_coverage.instances, 1);
    assert_eq!(required_coverage.passed, 1);
    assert_eq!(required_coverage.failed, 0);
    assert_eq!(required_coverage.incomplete, 0);
    assert_coverage_partitions(&report);
    Ok(())
}

#[test]
fn finite_segment_budget_evaluates_only_admitted_check_instances() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[], 0, 3)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([
            (0, Node::new(0, "unknown-0")),
            (1, Node::new(1, "unknown-1")),
            (2, Node::new(2, "unknown-2")),
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
    let node_coverage = coverage_where(
        &report,
        |key| key.kind == CheckKind::NodeAnchor,
        "node anchor is structural",
    );
    assert_eq!(node_coverage.instances, 3);
    assert_eq!(node_coverage.passed, 0);
    assert_eq!(node_coverage.failed, 1);
    assert_eq!(node_coverage.incomplete, 2);
    assert_coverage_partitions(&report);
    Ok(())
}

#[test]
fn required_value_includes_a_direct_value_vertex_subject() -> TestResult {
    let mut protocol = Protocol::default();
    protocol
        .constraint_semantics
        .insert("required".into(), ConstraintSemantics::RequiredValue);
    let schema = SchemaBuilder::new(&protocol)
        .vertex("value", "string", None)?
        .constraint("value", "required", "true")
        .build()?;
    let validation_plan =
        ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[], 0, 1)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "value"))]),
        edges: BTreeMap::new(),
        boundary: Boundary::default(),
    });

    let report = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget::default(),
    )?;
    let required_diagnostics: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|entry| entry.check.kind == CheckKind::RequiredValue)
        .map(|entry| (entry.subject.clone(), entry.violation.clone()))
        .collect();
    assert_eq!(
        required_diagnostics,
        vec![(
            Some(ViewRef::Node(0)),
            Violation::MissingValue { field: None },
        )]
    );
    let required_coverage = coverage_where(
        &report,
        |key| key.kind == CheckKind::RequiredValue,
        "the constraint compiles one required-value check",
    );
    assert_eq!(required_coverage.instances, 1);
    assert_eq!(required_coverage.passed, 0);
    assert_eq!(required_coverage.failed, 1);
    assert_eq!(required_coverage.incomplete, 0);
    assert_coverage_partitions(&report);
    Ok(())
}

#[test]
fn required_value_excludes_named_properties_to_other_vertices() -> TestResult {
    let mut protocol = Protocol::default();
    protocol
        .constraint_semantics
        .insert("required".into(), ConstraintSemantics::RequiredValue);
    let schema = SchemaBuilder::new(&protocol)
        .vertex("source", "object", None)?
        .vertex("value", "string", None)?
        .vertex("other", "string", None)?
        .edge("source", "other", "prop", Some("other"))?
        .constraint("value", "required", "true")
        .build()?;
    let validation_plan =
        ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[], 0, 2)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "source")), (1, Node::new(1, "value"))]),
        edges: BTreeMap::new(),
        boundary: Boundary::default(),
    });

    let report = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget::default(),
    )?;
    let required_subjects: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|entry| entry.check.kind == CheckKind::RequiredValue)
        .map(|entry| entry.subject.clone())
        .collect();
    assert_eq!(required_subjects, vec![Some(ViewRef::Node(1))]);
    let required_coverage = coverage_where(
        &report,
        |key| key.kind == CheckKind::RequiredValue,
        "the constraint compiles one required-value check",
    );
    assert_eq!(required_coverage.instances, 1);
    assert_eq!(required_coverage.passed, 0);
    assert_eq!(required_coverage.failed, 1);
    assert_eq!(required_coverage.incomplete, 0);
    assert_coverage_partitions(&report);
    Ok(())
}

#[test]
fn required_value_excludes_named_non_properties_to_the_value_vertex() -> TestResult {
    let mut protocol = Protocol::default();
    protocol
        .constraint_semantics
        .insert("required".into(), ConstraintSemantics::RequiredValue);
    let schema = SchemaBuilder::new(&protocol)
        .vertex("source", "object", None)?
        .vertex("value", "string", None)?
        .edge("source", "value", "link", Some("linked"))?
        .constraint("value", "required", "true")
        .build()?;
    let validation_plan =
        ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[], 0, 2)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "source")), (1, Node::new(1, "value"))]),
        edges: BTreeMap::new(),
        boundary: Boundary::default(),
    });

    let report = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget::default(),
    )?;
    let required_subjects: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|entry| entry.check.kind == CheckKind::RequiredValue)
        .map(|entry| entry.subject.clone())
        .collect();
    assert_eq!(required_subjects, vec![Some(ViewRef::Node(1))]);
    let required_coverage = coverage_where(
        &report,
        |key| key.kind == CheckKind::RequiredValue,
        "the constraint compiles one required-value check",
    );
    assert_eq!(required_coverage.instances, 1);
    assert_eq!(required_coverage.passed, 0);
    assert_eq!(required_coverage.failed, 1);
    assert_eq!(required_coverage.incomplete, 0);
    assert_coverage_partitions(&report);
    Ok(())
}

#[test]
fn required_value_excludes_a_materialized_property_source() -> TestResult {
    let mut protocol = Protocol::default();
    protocol
        .constraint_semantics
        .insert("required".into(), ConstraintSemantics::RequiredValue);
    let value_edge = label("source", "value", "prop", Some("value"));
    let schema = SchemaBuilder::new(&protocol)
        .vertex("source", "object", None)?
        .vertex("value", "string", None)?
        .edge("source", "value", "prop", Some("value"))?
        .constraint("value", "required", "true")
        .build()?;
    let validation_plan =
        ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[], 0, 2)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "source")), (1, Node::new(1, "value"))]),
        edges: BTreeMap::from([(EdgeId(0), GEdge::new(0, 1, value_edge))]),
        boundary: Boundary::default(),
    });

    let report = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget::default(),
    )?;
    let required_subjects: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|entry| entry.check.kind == CheckKind::RequiredValue)
        .map(|entry| entry.subject.clone())
        .collect();
    assert_eq!(required_subjects, vec![Some(ViewRef::Node(1))]);
    let required_coverage = coverage_where(
        &report,
        |key| key.kind == CheckKind::RequiredValue,
        "the constraint compiles one required-value check",
    );
    assert_eq!(required_coverage.instances, 1);
    assert_eq!(required_coverage.passed, 0);
    assert_eq!(required_coverage.failed, 1);
    assert_eq!(required_coverage.incomplete, 0);
    assert_coverage_partitions(&report);
    Ok(())
}

#[test]
fn required_value_includes_a_missing_property_site() -> TestResult {
    let mut protocol = Protocol::default();
    protocol
        .constraint_semantics
        .insert("required".into(), ConstraintSemantics::RequiredValue);
    let schema = SchemaBuilder::new(&protocol)
        .vertex("source", "object", None)?
        .vertex("value", "string", None)?
        .edge("source", "value", "prop", Some("value"))?
        .constraint("value", "required", "true")
        .build()?;
    let validation_plan =
        ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[], 0, 1)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "source"))]),
        edges: BTreeMap::new(),
        boundary: Boundary::default(),
    });

    let report = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget::default(),
    )?;
    let required_diagnostics: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|entry| entry.check.kind == CheckKind::RequiredValue)
        .map(|entry| (entry.subject.clone(), entry.violation.clone()))
        .collect();
    assert_eq!(
        required_diagnostics,
        vec![(
            Some(ViewRef::Node(0)),
            Violation::MissingValue {
                field: Some("value".into()),
            },
        )]
    );
    let required_coverage = coverage_where(
        &report,
        |key| key.kind == CheckKind::RequiredValue,
        "the constraint compiles one required-value check",
    );
    assert_eq!(required_coverage.instances, 1);
    assert_eq!(required_coverage.passed, 0);
    assert_eq!(required_coverage.failed, 1);
    assert_eq!(required_coverage.incomplete, 0);
    assert_coverage_partitions(&report);
    Ok(())
}

#[test]
fn primary_key_required_value_excludes_incoming_property_sources() -> TestResult {
    let protocol = Protocol::default();
    let schema = SchemaBuilder::new(&protocol)
        .vertex("source", "object", None)?
        .vertex("value", "string", None)?
        .edge("source", "value", "prop", Some("value"))?
        .key(
            "value",
            Key {
                id: "pk".into(),
                parts: vec![KeyPart::Path(Vec::new())],
                role: KeyRole::Primary,
                scope: KeyScope::Document,
            },
        )
        .build()?;
    let validation_plan =
        ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[], 0, 2)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "source")), (1, Node::new(1, "value"))]),
        edges: BTreeMap::new(),
        boundary: Boundary::default(),
    });

    let report = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget::default(),
    )?;
    let required_subjects: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|entry| {
            entry.check.kind == CheckKind::RequiredValue
                && matches!(entry.check.decl, SchemaRef::Key { .. })
        })
        .map(|entry| entry.subject.clone())
        .collect();
    assert_eq!(required_subjects, vec![Some(ViewRef::Node(1))]);
    let required_coverage = coverage_where(
        &report,
        |key| key.kind == CheckKind::RequiredValue && matches!(key.decl, SchemaRef::Key { .. }),
        "the primary key compiles one required-value check",
    );
    assert_eq!(required_coverage.instances, 1);
    assert_eq!(required_coverage.passed, 0);
    assert_eq!(required_coverage.failed, 1);
    assert_eq!(required_coverage.incomplete, 0);
    assert_coverage_partitions(&report);
    Ok(())
}

#[test]
fn application_external_excludes_value_site_sources() -> TestResult {
    let protocol = Protocol::default();
    let schema = SchemaBuilder::new(&protocol)
        .vertex("source", "object", None)?
        .vertex("value", "string", None)?
        .edge("source", "value", "prop", Some("value"))?
        .build()?;
    let mut options = ValidationOptions::default();
    options.external.insert(
        "application".into(),
        ExternalCheckDecl {
            name: "application".into(),
            anchors: BTreeSet::from(["value".into()]),
            requirement: Requirement::Required,
        },
    );
    let validation_plan = ValidationPlan::compile(&schema, &protocol, &options);
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[], 0, 2)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "source")), (1, Node::new(1, "value"))]),
        edges: BTreeMap::new(),
        boundary: Boundary::default(),
    });

    let report = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget::default(),
    )?;
    let external_subjects: Vec<_> = report
        .incomplete
        .iter()
        .filter(|entry| {
            entry.check.kind == CheckKind::External
                && !matches!(entry.check.decl, SchemaRef::Constraint { .. })
                && matches!(
                    entry.reason,
                    IncompleteReason::MissingExternalWitness { .. }
                )
        })
        .map(|entry| entry.subject.clone())
        .collect();
    assert_eq!(external_subjects, vec![Some(ViewRef::Node(1))]);
    let external_coverage = coverage_where(
        &report,
        |key| key.kind == CheckKind::External && !matches!(key.decl, SchemaRef::Constraint { .. }),
        "the application declaration compiles one external check",
    );
    assert_eq!(external_coverage.instances, 1);
    assert_eq!(external_coverage.passed, 0);
    assert_eq!(external_coverage.failed, 0);
    assert_eq!(external_coverage.incomplete, 1);
    assert_coverage_partitions(&report);
    Ok(())
}

#[test]
fn constraint_external_includes_a_missing_property_site() -> TestResult {
    let mut protocol = Protocol::default();
    protocol
        .constraint_semantics
        .insert("external".into(), ConstraintSemantics::External);
    let schema = SchemaBuilder::new(&protocol)
        .vertex("source", "object", None)?
        .vertex("value", "string", None)?
        .edge("source", "value", "prop", Some("value"))?
        .constraint("value", "external", "application")
        .build()?;
    let validation_plan =
        ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[], 0, 1)?;
    let segment = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "source"))]),
        edges: BTreeMap::new(),
        boundary: Boundary::default(),
    });

    let report = validate_segment(
        &segment,
        &partition,
        &validation_plan,
        &ValidationBudget::default(),
    )?;
    let external_subjects: Vec<_> = report
        .incomplete
        .iter()
        .filter(|entry| {
            entry.check.kind == CheckKind::External
                && matches!(entry.check.decl, SchemaRef::Constraint { .. })
                && matches!(
                    entry.reason,
                    IncompleteReason::MissingExternalWitness { .. }
                )
        })
        .map(|entry| entry.subject.clone())
        .collect();
    assert_eq!(external_subjects, vec![Some(ViewRef::Node(0))]);
    let external_coverage = coverage_where(
        &report,
        |key| key.kind == CheckKind::External && matches!(key.decl, SchemaRef::Constraint { .. }),
        "the constraint compiles one external check",
    );
    assert_eq!(external_coverage.instances, 1);
    assert_eq!(external_coverage.passed, 0);
    assert_eq!(external_coverage.failed, 0);
    assert_eq!(external_coverage.incomplete, 1);
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
    let Some(coverage) = passed
        .coverage
        .iter()
        .find(|(key, _)| key.kind == CheckKind::External)
        .map(|(_, coverage)| coverage)
    else {
        panic!("the application check is compiled");
    };
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
    assert!(
        report
            .diagnostics
            .iter()
            .all(|entry| { entry.check.kind != CheckKind::FanWellFormed })
    );
    assert!(report.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::FanWellFormed
            && entry.subject == Some(ViewRef::Node(10))
            && entry.reason
                == IncompleteReason::OutsideSegment {
                    locality: Locality::Summary(SummaryKind::SortedMerge),
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
fn tree_native_view_retains_an_owned_ordinary_node() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Tree, &[], 0, 1)?;
    let segment = AnySegment::Tree(TreeSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, Node::new(0, "unknown"))]),
        arcs: BTreeMap::new(),
        fans: Vec::new(),
        root: Some(0),
        next_node: Some(1),
        next_edge: Some(0),
        boundary: Boundary::default(),
    });

    let report = validate_segment(&segment, &partition, &plan()?, &ValidationBudget::default())?;
    assert!(report.diagnostics.iter().any(|entry| {
        entry.check.kind == CheckKind::NodeAnchor
            && entry.subject == Some(ViewRef::Node(0))
            && entry.violation
                == Violation::UnknownAnchor {
                    anchor: "unknown".into(),
                }
    }));
    let node_coverage = coverage_where(
        &report,
        |key| key.kind == CheckKind::NodeAnchor,
        "node anchor is structural",
    );
    assert_eq!(node_coverage.instances, 1);
    assert_eq!(node_coverage.passed, 0);
    assert_eq!(node_coverage.failed, 1);
    assert_eq!(node_coverage.incomplete, 0);
    assert_coverage_partitions(&report);
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
    let Some((key, spec)) = validation_plan.checks().iter().find(|(key, _)| {
        key.kind == CheckKind::RequiredValue
            && key.decl
                == SchemaRef::Key {
                    vertex: "a".into(),
                    key: "remote".into(),
                }
    }) else {
        panic!("the primary key compiles a required-value check");
    };
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
    let Some((key, spec)) = validation_plan.checks().iter().find(|(key, _)| {
        key.kind == CheckKind::RequiredValue
            && key.decl
                == SchemaRef::Key {
                    vertex: "item".into(),
                    key: "position".into(),
                }
    }) else {
        panic!("the primary position key compiles a required-value check");
    };
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

fn local_completeness_plan() -> Result<ValidationPlan, Box<dyn std::error::Error>> {
    let mut protocol = Protocol::default();
    protocol.constraint_semantics.extend([
        (
            "maxLength".into(),
            ConstraintSemantics::MaxLength(LengthUnit::Chars),
        ),
        ("minOut".into(), ConstraintSemantics::MinOutDegree),
        ("required".into(), ConstraintSemantics::RequiredValue),
    ]);
    let property = label("a", "value", "prop", Some("value"));
    let ordered = label("a", "a", "prop", Some("children"));
    let mut schema = SchemaBuilder::new(&protocol)
        .vertex("a", "object", None)?
        .vertex("b", "object", None)?
        .vertex("c", "object", None)?
        .vertex("doc", "object", None)?
        .vertex("para", "object", None)?
        .vertex("word", "object", None)?
        .vertex("value", "string", None)?
        .edge("a", "value", "prop", Some("value"))?
        .edge("a", "a", "prop", Some("children"))?
        .required("a", vec![property])
        .constraint("a", "minOut", "1")
        .constraint("value", "maxLength", "3")
        .constraint("value", "required", "true")
        .hyper_edge(
            "rel",
            "relation",
            HashMap::from([
                ("parent".to_owned(), "a".to_owned()),
                ("child".to_owned(), "a".to_owned()),
            ]),
            "parent",
        )?
        .build()?;
    schema.orderings.insert(ordered, 0);
    let mut options = ValidationOptions::default();
    options.external.insert(
        "application".into(),
        ExternalCheckDecl {
            name: "application".into(),
            anchors: BTreeSet::from([
                "a".into(),
                "b".into(),
                "c".into(),
                "doc".into(),
                "para".into(),
                "word".into(),
                "value".into(),
            ]),
            requirement: Requirement::Required,
        },
    );
    Ok(ValidationPlan::compile(&schema, &protocol, &options))
}

struct LocalLawCorpus {
    graph: GInstance,
    table: FInstance,
    tree: WInstance,
    embedding: crate::ginstance::TreeEmbedding,
}

fn local_law_graph() -> Result<GInstance, Box<dyn std::error::Error>> {
    let property = label("a", "value", "prop", Some("value"));
    let ordered = label("a", "a", "prop", Some("children"));
    let unknown = label("a", "value", "mystery", Some("unknown"));
    let mut graph = GInstance::new();
    for node in [
        Node::new(0, "a").with_extra_field("value", Value::Str("ok".into())),
        Node::new(1, "value").with_value(FieldPresence::Present(Value::Str("ok".into()))),
        Node::new(2, "a").with_extra_field("bogus", Value::Int(1)),
        Node::new(3, "value").with_value(FieldPresence::Present(Value::Str("too-long".into()))),
        Node::new(4, "value"),
        Node::new(5, "a"),
        Node::new(6, "unknown"),
        Node::new(8, "rel").with_shape(NodeShape::HyperEdge),
    ] {
        graph.insert_node(node)?;
    }
    for (id, edge) in [
        (EdgeId(0), GEdge::new(0, 1, property.clone())),
        (
            EdgeId(1),
            GEdge::new(0, 2, ordered.clone()).with_position(Some(0)),
        ),
        (
            EdgeId(2),
            GEdge::new(2, 0, ordered.clone()).with_position(Some(0)),
        ),
        (EdgeId(3), GEdge::new(2, 2, ordered).with_position(Some(0))),
        (EdgeId(4), GEdge::new(2, 3, property.clone())),
        (EdgeId(5), GEdge::new(0, 2, property)),
        (EdgeId(6), GEdge::new(0, 1, unknown)),
        (
            EdgeId(7),
            GEdge::new(8, 0, label("rel", "a", HYPER_PARENT_KIND, Some("parent"))),
        ),
        (
            EdgeId(8),
            GEdge::new(8, 2, label("rel", "a", HYPER_LABEL_KIND, Some("child"))),
        ),
        (
            EdgeId(9),
            GEdge::new(2, 0, label("a", "a", HYPER_LABEL_KIND, Some("rogue"))),
        ),
    ] {
        graph.insert_edge(id, edge)?;
    }
    Ok(graph)
}

fn local_law_table() -> Result<FInstance, Box<dyn std::error::Error>> {
    let property = label("a", "value", "prop", Some("value"));
    let unknown = label("a", "value", "mystery", Some("unknown"));
    Ok(FInstance::new()
        .with_rows(
            "a",
            [
                (
                    RowId(0),
                    HashMap::from([("value".into(), Value::Str("ok".into()))]),
                ),
                (
                    RowId(2),
                    HashMap::from([
                        ("value".into(), Value::Str("too-long".into())),
                        ("bogus".into(), Value::Int(1)),
                    ]),
                ),
                (RowId(3), HashMap::new()),
            ],
        )?
        .with_rows(
            "value",
            [(RowId(1), HashMap::new()), (RowId(4), HashMap::new())],
        )?
        .with_rows("unknown", [(RowId(5), HashMap::new())])?
        .with_foreign_key(property, [(RowId(3), RowId(1))])?
        .with_foreign_key(unknown, [(RowId(2), RowId(4))])?)
}

fn local_law_tree() -> WInstance {
    let property = label("a", "value", "prop", Some("value"));
    let ordered = label("a", "a", "prop", Some("children"));
    let unknown = label("a", "value", "mystery", Some("unknown"));
    let nodes = HashMap::from([
        (
            0,
            Node::new(0, "a").with_extra_field("value", Value::Str("ok".into())),
        ),
        (
            1,
            Node::new(1, "value").with_value(FieldPresence::Present(Value::Str("ok".into()))),
        ),
        (
            2,
            Node::new(2, "a").with_extra_field("bogus", Value::Int(1)),
        ),
        (
            3,
            Node::new(3, "value").with_value(FieldPresence::Present(Value::Str("too-long".into()))),
        ),
        (4, Node::new(4, "value")),
        (5, Node::new(5, "a")),
        (6, Node::new(6, "unknown")),
        (
            7,
            Node::new(7, "value").with_value(FieldPresence::Present(Value::Str("ok".into()))),
        ),
    ]);
    let arcs = vec![
        (0, 1, property.clone()),
        (0, 2, ordered.clone()),
        (2, 3, property),
        (2, 4, ordered.clone()),
        (0, 5, ordered),
        (0, 6, label("a", "value", "prop", Some("value"))),
        (0, 7, unknown),
    ];
    WInstance::new(
        nodes,
        arcs,
        vec![Fan::new("rel", 0).with_child("child", 2)],
        0,
        "a".into(),
    )
}

fn local_law_corpus() -> Result<LocalLawCorpus, Box<dyn std::error::Error>> {
    let graph = local_law_graph()?;
    let table = local_law_table()?;
    let tree = local_law_tree();
    let (_, embedding) = GInstance::from_wtype_embedded(&tree, None)?;

    Ok(LocalLawCorpus {
        graph,
        table,
        tree,
        embedding,
    })
}

#[test]
fn tree_fan_well_formed_is_a_sorted_merge_while_graph_fans_remain_local() -> TestResult {
    let plan = local_completeness_plan()?;
    let Some(spec) = plan
        .checks()
        .values()
        .find(|spec| spec.key.kind == CheckKind::FanWellFormed)
    else {
        panic!("the structural fan check is compiled");
    };
    assert_eq!(
        spec.locality.get(&InstanceShapeKind::Tree),
        Some(&Locality::Summary(SummaryKind::SortedMerge))
    );
    assert_eq!(
        spec.locality.get(&InstanceShapeKind::Graph),
        Some(&Locality::Local)
    );
    assert_eq!(
        spec.locality.get(&InstanceShapeKind::Table),
        Some(&Locality::Local),
        "tables have no fan subjects"
    );
    Ok(())
}

fn external_verdicts(subjects: impl IntoIterator<Item = ViewRef>) -> ExternalVerdicts {
    let mut verdicts = ExternalVerdicts::default();
    let mut subjects: Vec<_> = subjects.into_iter().collect();
    subjects.sort();
    for (index, subject) in subjects.into_iter().enumerate() {
        verdicts.insert(
            "application".into(),
            subject,
            ExternalVerdict {
                passed: index % 2 == 0,
                witness: Digest([7; 32]),
            },
        );
    }
    verdicts
}

fn project_external(full: &ExternalVerdicts, segment: &AnySegment) -> ExternalVerdicts {
    let owned: BTreeSet<ViewRef> = segment
        .node_anchors()
        .into_iter()
        .map(|(id, _)| segment.node_ref(id))
        .collect();
    ExternalVerdicts(
        full.0
            .iter()
            .filter(|((_, subject), _)| owned.contains(subject))
            .map(|(key, verdict)| (key.clone(), *verdict))
            .collect(),
    )
}

fn local_diagnostic_multisets<'a>(
    plan: &ValidationPlan,
    shape: InstanceShapeKind,
    reports: impl IntoIterator<Item = &'a ValidationReport>,
) -> BTreeMap<crate::validation::CheckKey, Vec<Vec<u8>>> {
    let mut by_check: BTreeMap<_, Vec<Vec<u8>>> = plan
        .checks()
        .iter()
        .filter(|(_, spec)| spec.locality.get(&shape) == Some(&Locality::Local))
        .map(|(key, _)| (key.clone(), Vec::new()))
        .collect();
    for report in reports {
        for diagnostic in report.diagnostics.iter().filter(|diagnostic| {
            plan.checks()
                .get(&diagnostic.check)
                .and_then(|spec| spec.locality.get(&shape))
                == Some(&Locality::Local)
        }) {
            let bytes = rmp_serde::to_vec_named(diagnostic)
                .unwrap_or_else(|error| panic!("a diagnostic is serializable: {error}"));
            let Some(diagnostics) = by_check.get_mut(&diagnostic.check) else {
                panic!("every local diagnostic has a compiled local key");
            };
            diagnostics.push(bytes);
        }
    }
    for diagnostics in by_check.values_mut() {
        diagnostics.sort();
    }
    by_check
}

fn assert_local_completeness(
    plan: &ValidationPlan,
    shape: InstanceShapeKind,
    whole: &ValidationReport,
    segments: &[ValidationReport],
) -> Result<(), TestCaseError> {
    let whole = local_diagnostic_multisets(plan, shape, [whole]);
    let segments = local_diagnostic_multisets(plan, shape, segments);
    prop_assert_eq!(segments, whole);
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CoverageLedger {
    instances: u64,
    passed: u64,
    failed: u64,
    incomplete: u64,
}

fn local_coverage_ledger(
    plan: &ValidationPlan,
    shape: InstanceShapeKind,
    reports: &[ValidationReport],
) -> BTreeMap<crate::validation::CheckKey, CoverageLedger> {
    plan.checks()
        .iter()
        .filter(|(_, spec)| spec.locality.get(&shape) == Some(&Locality::Local))
        .map(|(key, _)| {
            let mut total = CoverageLedger::default();
            for report in reports {
                if let Some(coverage) = report.coverage.get(key) {
                    total.instances += coverage.instances;
                    total.passed += coverage.passed;
                    total.failed += coverage.failed;
                    total.incomplete += coverage.incomplete;
                }
            }
            (key.clone(), total)
        })
        .collect()
}

fn failure_is_admission_invariant(shape: InstanceShapeKind, kind: CheckKind) -> bool {
    matches!(
        (shape, kind),
        (
            InstanceShapeKind::Table,
            CheckKind::EndpointCoherence | CheckKind::PositionDistinct
        ) | (
            InstanceShapeKind::Tree,
            CheckKind::ReservedVocabulary
                | CheckKind::TreeRoot
                | CheckKind::TreeParentMap
                | CheckKind::TreeReachable
        )
    )
}

fn assert_local_coverage(
    plan: &ValidationPlan,
    shape: InstanceShapeKind,
    reports: &[ValidationReport],
) {
    let ledger = local_coverage_ledger(plan, shape, reports);
    for (key, coverage) in &ledger {
        assert_eq!(
            coverage.instances,
            coverage.passed + coverage.failed + coverage.incomplete,
            "coverage partitions for {shape:?} {key:?}"
        );

        if matches!(
            key.kind,
            CheckKind::SchemaWellFormed | CheckKind::DeclarationInterpretable
        ) || plan.is_statically_incomplete(key)
        {
            continue;
        }
        if shape == InstanceShapeKind::Table && key.kind == CheckKind::FanWellFormed {
            assert_eq!(
                *coverage,
                CoverageLedger::default(),
                "tables have no fan subjects"
            );
            continue;
        }

        assert!(
            coverage.instances > 0,
            "the fixed corpus must exercise {shape:?} {key:?}"
        );
        assert_eq!(
            coverage.incomplete, 0,
            "the fixed corpus supplies every input for {shape:?} {key:?}"
        );
        assert!(
            coverage.passed > 0,
            "the fixed corpus must supply a passing witness for {shape:?} {key:?}"
        );
        if failure_is_admission_invariant(shape, key.kind) {
            assert_eq!(
                coverage.failed, 0,
                "the admitted {shape:?} carrier makes {key:?} pass-only"
            );
        } else {
            assert!(
                coverage.failed > 0,
                "the fixed corpus must supply a failing witness for {shape:?} {key:?}"
            );
        }
    }
}

#[test]
fn local_law_corpus_covers_every_compiled_local_family() -> TestResult {
    let plan = local_completeness_plan()?;
    let corpus = local_law_corpus()?;
    let graph_external = external_verdicts(corpus.graph.nodes().keys().copied().map(ViewRef::Node));
    let table_external = external_verdicts(corpus.table.row_ids().into_iter().map(ViewRef::Row));
    let tree_external = external_verdicts(corpus.tree.nodes.keys().copied().map(ViewRef::Node));
    let graph = plan.validate_graph(&corpus.graph, &ValidationBudget::default(), &graph_external);
    let table = plan.validate_table(&corpus.table, &ValidationBudget::default(), &table_external);
    let tree = plan.validate_tree(&corpus.tree, &ValidationBudget::default(), &tree_external);

    assert_local_coverage(&plan, InstanceShapeKind::Graph, &[graph]);
    assert_local_coverage(&plan, InstanceShapeKind::Table, &[table]);
    assert_local_coverage(&plan, InstanceShapeKind::Tree, &[tree]);

    let Some(fan_key) = plan
        .checks()
        .keys()
        .find(|key| key.kind == CheckKind::FanWellFormed)
    else {
        panic!("the structural fan check is compiled");
    };
    let graph_fan = local_coverage_ledger(
        &plan,
        InstanceShapeKind::Graph,
        &[plan.validate_graph(&corpus.graph, &ValidationBudget::default(), &graph_external)],
    )[fan_key];
    assert!(graph_fan.passed > 0 && graph_fan.failed > 0);
    assert_ne!(
        plan.checks()[fan_key]
            .locality
            .get(&InstanceShapeKind::Tree),
        Some(&Locality::Local)
    );
    Ok(())
}

fn graph_local_completeness(
    graph: &GInstance,
    partition: &Partition,
    plan: &ValidationPlan,
) -> Result<(), TestCaseError> {
    let external = external_verdicts(graph.nodes().keys().copied().map(ViewRef::Node));
    let whole = plan.validate_graph(graph, &ValidationBudget::default(), &external);
    let segments = partition_graph(graph, partition)
        .map_err(|error| TestCaseError::fail(error.to_string()))?
        .into_iter()
        .map(AnySegment::Graph)
        .map(|segment| {
            let projection = project_external(&external, &segment);
            validate_segment_with_external(
                &segment,
                partition,
                plan,
                &ValidationBudget::default(),
                &projection,
            )
            .map_err(|error| TestCaseError::fail(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    assert_local_completeness(plan, InstanceShapeKind::Graph, &whole, &segments)
}

fn table_local_completeness(
    table: &FInstance,
    partition: &Partition,
    plan: &ValidationPlan,
) -> Result<(), TestCaseError> {
    let external = external_verdicts(table.row_ids().into_iter().map(ViewRef::Row));
    let whole = plan.validate_table(table, &ValidationBudget::default(), &external);
    let segments = partition_table(table, partition)
        .map_err(|error| TestCaseError::fail(error.to_string()))?
        .into_iter()
        .map(AnySegment::Table)
        .map(|segment| {
            let projection = project_external(&external, &segment);
            validate_segment_with_external(
                &segment,
                partition,
                plan,
                &ValidationBudget::default(),
                &projection,
            )
            .map_err(|error| TestCaseError::fail(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    assert_local_completeness(plan, InstanceShapeKind::Table, &whole, &segments)
}

fn tree_local_completeness(
    tree: &WInstance,
    embedding: &crate::ginstance::TreeEmbedding,
    partition: &Partition,
    plan: &ValidationPlan,
) -> Result<(), TestCaseError> {
    let external = external_verdicts(tree.nodes.keys().copied().map(ViewRef::Node));
    let whole = plan.validate_tree(tree, &ValidationBudget::default(), &external);
    let segments = partition_tree(tree, embedding, partition)
        .map_err(|error| TestCaseError::fail(error.to_string()))?
        .into_iter()
        .map(AnySegment::Tree)
        .map(|segment| {
            let projection = project_external(&external, &segment);
            validate_segment_with_external(
                &segment,
                partition,
                plan,
                &ValidationBudget::default(),
                &projection,
            )
            .map_err(|error| TestCaseError::fail(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    assert_local_completeness(plan, InstanceShapeKind::Tree, &whole, &segments)
}

#[test]
fn duplicate_tree_fans_across_segments_are_excluded_from_the_local_key_law() -> TestResult {
    let nodes = HashMap::from([(0, Node::new(0, "doc")), (1, Node::new(1, "word"))]);
    let fan = Fan::new("missing", 0).with_child("child", 1);
    let tree = WInstance::new(
        nodes,
        vec![(0, 1, label("doc", "word", "prop", Some("child")))],
        vec![fan.clone(), fan],
        0,
        "doc".into(),
    );
    let (_, embedding) = GInstance::from_wtype_embedded(&tree, None)?;
    let free = next_free(
        embedding.next_node.checked_sub(1),
        embedding.next_edge.checked_sub(1),
    );
    let partition = interleaved(InstanceShapeKind::Tree, free, 2, 0);
    let plan = local_completeness_plan()?;
    let external = external_verdicts(tree.nodes.keys().copied().map(ViewRef::Node));
    let whole = plan.validate_tree(&tree, &ValidationBudget::default(), &external);
    assert_eq!(
        whole
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.check.kind == CheckKind::FanWellFormed)
            .count(),
        1,
        "equal native fans are one semantic fan"
    );

    let segment_reports: Vec<_> = partition_tree(&tree, &embedding, &partition)?
        .into_iter()
        .map(AnySegment::Tree)
        .map(|segment| {
            let projection = project_external(&external, &segment);
            validate_segment_with_external(
                &segment,
                &partition,
                &plan,
                &ValidationBudget::default(),
                &projection,
            )
        })
        .collect::<Result<_, _>>()?;
    assert!(segment_reports.iter().all(|report| {
        report
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.check.kind != CheckKind::FanWellFormed)
    }));
    assert!(segment_reports.iter().any(|report| {
        report.incomplete.iter().any(|entry| {
            entry.check.kind == CheckKind::FanWellFormed
                && entry.reason
                    == IncompleteReason::OutsideSegment {
                        locality: Locality::Summary(SummaryKind::SortedMerge),
                    }
        })
    }));

    assert_eq!(
        local_diagnostic_multisets(&plan, InstanceShapeKind::Tree, &segment_reports),
        local_diagnostic_multisets(&plan, InstanceShapeKind::Tree, [&whole])
    );
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn local_check_diagnostics_from_all_segments_equal_the_whole_state_diagnostics_for_every_partition(
        (graph, graph_partition) in arb_ginstance().prop_flat_map(|graph| {
            let free = next_free(graph.max_node_id(), graph.max_edge_id().map(|edge| edge.0));
            (Just(graph), arb_partition(InstanceShapeKind::Graph, free))
        }),
        (table, table_partition) in arb_finstance().prop_flat_map(|table| {
            let free = next_free(table.max_row_id().map(|row| row.0), None);
            (Just(table), arb_partition(InstanceShapeKind::Table, free))
        }),
        ((tree, embedding), tree_partition) in arb_winstance().prop_flat_map(|(tree, embedding)| {
            let free = next_free(
                embedding.next_node.checked_sub(1),
                embedding.next_edge.checked_sub(1),
            );
            (Just((tree, embedding)), arb_partition(InstanceShapeKind::Tree, free))
        }),
        graph_corpus_partition in arb_partition(InstanceShapeKind::Graph, 16),
        table_corpus_partition in arb_partition(InstanceShapeKind::Table, 16),
        tree_corpus_partition in arb_partition(InstanceShapeKind::Tree, 16),
    ) {
        let plan = local_completeness_plan()
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        graph_local_completeness(&graph, &graph_partition, &plan)?;
        table_local_completeness(&table, &table_partition, &plan)?;
        tree_local_completeness(&tree, &embedding, &tree_partition, &plan)?;

        let corpus = local_law_corpus()
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        graph_local_completeness(&corpus.graph, &graph_corpus_partition, &plan)?;
        table_local_completeness(&corpus.table, &table_corpus_partition, &plan)?;
        tree_local_completeness(
            &corpus.tree,
            &corpus.embedding,
            &tree_corpus_partition,
            &plan,
        )?;
    }
}
