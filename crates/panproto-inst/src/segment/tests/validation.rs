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
use crate::metadata::{Node, NodeShape};
use crate::segment::{
    AnySegment, Boundary, Ghost, GraphSegment, InStub, Partition, SegmentError, SegmentId,
    TableSegment, TreeSegment, validate_segment,
};
use crate::validation::{
    CheckKind, EndpointSide, ExternalCheckDecl, IncompleteReason, ReportScope, Requirement,
    ValidationBudget, ValidationOptions, ValidationPlan, ValidationReport, Violation,
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
    ) -> Result<ValidationReport, SegmentError> = validate_segment;
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
