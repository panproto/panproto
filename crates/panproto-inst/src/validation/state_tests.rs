//! Unit tests of the incremental validation state that observe how many
//! check instances are evaluated, which only the crate's own counter can.

use std::sync::Arc;

use panproto_gat::CheckStatus;
use panproto_schema::{
    Cardinality, Edge, InstanceShapeKind, Key, KeyPart, KeyRole, KeyScope, OnUnresolved, Protocol,
    RefSource, Reference, ResolveMode, Schema, Vertex,
};

use super::checks::{reset_semantic_evaluations, semantic_evaluations};
use super::{
    AdmissionPolicy, CheckKind, EditRejection, ExternalVerdicts, IncompleteReason, ReportScope,
    ValidatedGraph, ValidationBudget, ValidationOptions, ValidationPlan,
};
use crate::ginstance::GInstance;
use crate::graph_edit::{GraphDelta, GraphEdit};
use crate::metadata::Node;
use crate::value::{FieldPresence, Value};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

fn declare(schema: &mut Schema, e: &Edge) {
    schema.edges.insert(e.clone(), e.kind.clone());
    schema
        .outgoing
        .entry(e.src.clone())
        .or_default()
        .push(e.clone());
    schema
        .incoming
        .entry(e.tgt.clone())
        .or_default()
        .push(e.clone());
    schema
        .between
        .entry((e.src.clone(), e.tgt.clone()))
        .or_default()
        .push(e.clone());
}

/// Items named through `name`, keyed by that name, and holders whose `tag`
/// names an item by its key.
fn reference_plan() -> Arc<ValidationPlan> {
    let mut schema = Schema::default();
    for (id, kind) in [
        ("item", "object"),
        ("holder", "object"),
        ("name", "string"),
        ("tag", "string"),
    ] {
        schema.vertices.insert(
            id.into(),
            Vertex {
                id: id.into(),
                kind: kind.into(),
                nsid: None,
            },
        );
    }
    let name = edge("item", "name", "name");
    let tag = edge("holder", "tag", "tag");
    let link = Edge {
        kind: "ref".into(),
        ..edge("holder", "item", "link")
    };
    for e in [&name, &tag, &link] {
        declare(&mut schema, e);
    }
    schema.declarations.insert_key(
        "item".into(),
        Key {
            id: "pk".into(),
            parts: vec![KeyPart::Path(vec![name])],
            role: KeyRole::Unique,
            scope: KeyScope::Global,
        },
    );
    schema.declarations.insert_reference(Reference {
        id: "holder_ref".into(),
        holder: "holder".into(),
        source: RefSource::KeyValue {
            parts: vec![vec![tag]],
        },
        target_vertex: "item".into(),
        target_key: Some("pk".into()),
        edge: Some(link),
        cardinality: Cardinality::One,
        nullable: true,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
    });
    Arc::new(ValidationPlan::compile(
        &schema,
        &Protocol {
            syntactic_shape: InstanceShapeKind::Graph,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        },
        &ValidationOptions::default(),
    ))
}

/// `items` items named `0..items` and `holders` holders naming item 0.
fn reference_graph(items: u32, holders: u32) -> GInstance {
    let mut graph = GInstance::new();
    let mut next = 0_u32;
    let mut fresh = || {
        next += 1;
        next - 1
    };
    for i in 0..items {
        let (item, value) = (fresh(), fresh());
        graph = graph
            .with_node(Node::new(item, "item"))
            .with_node(
                Node::new(value, "name")
                    .with_value(FieldPresence::Present(Value::Str(i.to_string()))),
            )
            .with_edge(item, value, edge("item", "name", "name"));
    }
    for _ in 0..holders {
        let (holder, value) = (fresh(), fresh());
        graph = graph
            .with_node(Node::new(holder, "holder"))
            .with_node(
                Node::new(value, "tag").with_value(FieldPresence::Present(Value::Str("0".into()))),
            )
            .with_edge(holder, value, edge("holder", "tag", "tag"));
    }
    graph
}

fn zero() -> ValidationBudget {
    ValidationBudget { max_reads: Some(0) }
}

/// Initialization evaluates exactly the instances whole-instance validation
/// evaluates under the same budget: under a zero budget, only instances
/// with an empty trace, and never a budget-skipped one.
#[test]
fn a_zero_budget_initialization_evaluates_no_check_instance() {
    let plan = reference_plan();
    let graph = reference_graph(4, 4);
    reset_semantic_evaluations();
    let whole = plan.validate_graph(&graph, &zero(), &ExternalVerdicts::default());
    let by_validation = semantic_evaluations();
    reset_semantic_evaluations();
    let (validated, report) =
        ValidatedGraph::new(plan, graph, &zero(), &ExternalVerdicts::default());
    assert_eq!(semantic_evaluations(), by_validation);
    assert_eq!(report.canonical_bytes(), whole.canonical_bytes());
    assert_eq!(report.status, CheckStatus::Incomplete);
    let cut = validated
        .state()
        .budget_cuts
        .values()
        .next()
        .cloned()
        .unwrap_or_else(|| panic!("a zero budget cuts the schedule"));
    assert_eq!(by_validation + cut.skipped.len(), cut.scheduled.len());
    assert!(cut.skipped.len() > by_validation);
}

#[test]
fn a_zero_budget_edit_evaluates_no_check_instance() -> TestResult {
    let plan = reference_plan();
    let graph = reference_graph(4, 4);
    let (mut validated, _) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let edit = GraphEdit::AddNode {
        node: Node::new(1000, "item"),
    };
    reset_semantic_evaluations();
    let checked = validated.check_edit(&edit, &zero(), &ExternalVerdicts::default())?;
    assert_eq!(semantic_evaluations(), 0, "check_edit evaluated");
    assert_eq!(
        checked.report.status,
        CheckStatus::Incomplete,
        "{:#?}",
        checked.report.diagnostics
    );
    reset_semantic_evaluations();
    let result = validated.apply_edit(
        &edit,
        AdmissionPolicy::NoNewFailures,
        &zero(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(semantic_evaluations(), 0, "apply_edit evaluated");
    assert!(matches!(result, Err(EditRejection::NotAdmitted { .. })));
    Ok(())
}

#[test]
fn reattachment_replays_a_budget_cut_without_evaluating_skipped_instances() -> TestResult {
    let plan = reference_plan();
    let graph = reference_graph(4, 4);
    let (validated, _) = ValidatedGraph::new(
        plan.clone(),
        graph,
        &ValidationBudget { max_reads: Some(5) },
        &ExternalVerdicts::default(),
    );
    let skipped = validated
        .state()
        .budget_cuts
        .values()
        .next()
        .map_or(0, |cut| cut.skipped.len());
    let scheduled = validated
        .state()
        .budget_cuts
        .values()
        .next()
        .map_or(0, |cut| cut.scheduled.len());
    assert!(skipped > 0 && skipped < scheduled);
    let (graph, state) = validated.into_parts();
    reset_semantic_evaluations();
    ValidatedGraph::from_parts(plan, graph, state)?;
    // Only the evaluated prefix is evaluated again, to check the stored
    // outcome bundles; the replayed schedule and its skipped suffix are not.
    assert_eq!(semantic_evaluations(), scheduled - skipped);
    Ok(())
}

#[test]
fn the_admitted_sequence_number_refuses_to_wrap() -> TestResult {
    let plan = reference_plan();
    let (validated, _) = ValidatedGraph::new(
        plan.clone(),
        reference_graph(1, 0),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let (graph, mut state) = validated.into_parts();
    state.seq = u64::MAX - 1;
    let mut validated = ValidatedGraph::from_parts(plan, graph, state)?;
    let add = |id: u32| GraphEdit::AddNode {
        node: Node::new(id, "item"),
    };
    let first = validated.apply_edit(
        &add(100),
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(first.baseline_seq, u64::MAX - 1);
    assert_eq!(validated.state().seq, u64::MAX);
    let before = validated.state().canonical_bytes();
    let second = validated.apply_edit(
        &add(101),
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert!(
        matches!(
            second,
            Err(EditRejection::SequenceExhausted { seq: u64::MAX })
        ),
        "{second:?}"
    );
    assert_eq!(validated.state().canonical_bytes(), before);
    assert!(validated.instance().node(101).is_none());
    Ok(())
}

#[test]
fn delta_local_validation_evaluates_global_kinds_in_full() -> TestResult {
    let plan = reference_plan();
    let graph = reference_graph(2, 2);
    let full = plan.validate_graph(
        &graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(full.status, CheckStatus::Passed);
    let identity = plan.check_graph_delta(
        &graph,
        &GraphDelta::identity(),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(identity.scope, ReportScope::DeltaLocal);
    assert_eq!(
        identity.status,
        CheckStatus::Passed,
        "{:#?}",
        identity.diagnostics
    );
    for kind in [CheckKind::KeyUnique, CheckKind::ReferenceResolves] {
        let (key, coverage) = identity
            .coverage
            .iter()
            .find(|(key, _)| key.kind == kind)
            .ok_or("the plan compiles the global kind")?;
        assert_eq!(coverage, &full.coverage[key], "{kind:?}");
    }
    assert!(
        identity
            .incomplete
            .iter()
            .all(|entry| entry.reason != IncompleteReason::NoValidatedBaseline)
    );
    // Under a budget too small for the global instances, each global key
    // that was not evaluated in full says so.
    let cut = plan.check_graph_delta(
        &graph,
        &GraphDelta::identity(),
        &zero(),
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(cut.status, CheckStatus::Incomplete);
    for kind in [CheckKind::KeyUnique, CheckKind::ReferenceResolves] {
        assert!(cut.incomplete.iter().any(|entry| entry.check.kind == kind
            && entry.reason == IncompleteReason::NoValidatedBaseline));
    }
    Ok(())
}

#[test]
fn an_inapplicable_delta_is_refused_by_the_delta_local_checkers() {
    let mut schema = Schema::default();
    schema.vertices.insert(
        "item".into(),
        Vertex {
            id: "item".into(),
            kind: "object".into(),
            nsid: None,
        },
    );
    let plan = ValidationPlan::compile(
        &schema,
        &Protocol {
            syntactic_shape: InstanceShapeKind::Graph,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        },
        &ValidationOptions::default(),
    );
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    // A delta that adds a node at an unknown anchor and deletes a node the
    // graph does not have: it does not apply.
    let ghost = GraphEdit::Batch(vec![
        GraphEdit::AddNode {
            node: Node::new(1, "ghost"),
        },
        GraphEdit::RemoveNode {
            node: Node::new(99, "item"),
            incident: Vec::new(),
        },
    ])
    .to_delta();
    assert!(ghost.is_ok(), "{ghost:?}");
    let Ok(ghost) = ghost else { return };
    assert!(ghost.apply(&mut graph.clone()).is_err());
    let result = plan.check_graph_delta(
        &graph,
        &ghost,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert!(
        matches!(result, Err(crate::edit_error::EditError::NodeNotFound(99))),
        "{result:?}"
    );
}

// ---------------------------------------------------------------------------
// The admission policies against their definitions. The predicates below
// are written from the policies' definitions over abstract bundle vectors
// and call nothing in the crate; the production side is `admitted` over
// states that realize the same vectors.
// ---------------------------------------------------------------------------

/// The abstract outcome of one bundle: passed, failed or incomplete with
/// one of two canonical byte values, or budget-skipped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Passed,
    Failed(u8),
    Incomplete(u8),
    Skipped,
}

/// One abstract state: the outcomes of the three required bundles, whether
/// the first required bundle also carries an advisory diagnostic, the
/// outcome of the advisory check's one bundle, and the budget cut (the
/// position in the canonical schedule from which every instance is
/// skipped, and the reads spent), if any.
#[derive(Clone, Copy, Debug)]
struct Abstract {
    required: [Outcome; 3],
    advisory_note: bool,
    advisory: Outcome,
    cut: Option<(usize, u64)>,
}

/// What admission compares for one required bundle: nothing for a passed
/// bundle, the outcome with its byte value for an evaluated failure or
/// incompleteness, and for a skipped one the virtual incompleteness of the
/// cut, identified by the reads spent and the first skipped subject of the
/// required check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Compared {
    None,
    Failed(u8),
    Incomplete(u8),
    Virtual { spent: u64, from: usize },
}

fn compared(state: &Abstract) -> [Compared; 3] {
    let from = state
        .required
        .iter()
        .position(|o| *o == Outcome::Skipped)
        .unwrap_or(usize::MAX);
    let spent = state.cut.map_or(0, |(_, spent)| spent);
    state.required.map(|o| match o {
        Outcome::Passed => Compared::None,
        Outcome::Failed(b) => Compared::Failed(b),
        Outcome::Incomplete(b) => Compared::Incomplete(b),
        Outcome::Skipped => Compared::Virtual { spent, from },
    })
}

/// `RequirePassed`, from its definition: the result is `Passed`, that is
/// (V.5) no required bundle failed, is incomplete or was skipped by a cut
/// (whose key-level budget entry is required).
fn require_passed_predicate(result: &Abstract) -> bool {
    compared(result).iter().all(|c| *c == Compared::None)
}

/// `NoNewFailures`, from its definition: every required failed or
/// incomplete bundle of the result, a budget-skipped one counting as its
/// virtual incompleteness, was already so in the baseline with the same
/// canonical bytes. Advisory outcomes decide nothing.
fn no_new_failures_predicate(baseline: &Abstract, result: &Abstract) -> bool {
    compared(baseline)
        .iter()
        .zip(compared(result).iter())
        .all(|(before, after)| *after == Compared::None || before == after)
}

/// The external plan of the admission scope: a required check `audit` and
/// an advisory check `notes`, both about `item` elements; the required
/// check's instances come first in the canonical schedule, so a cut can
/// skip the advisory instance alone.
fn admission_plan() -> Arc<ValidationPlan> {
    let mut schema = Schema::default();
    schema.vertices.insert(
        "item".into(),
        Vertex {
            id: "item".into(),
            kind: "object".into(),
            nsid: None,
        },
    );
    let decl = |name: &str, requirement| super::ExternalCheckDecl {
        name: name.into(),
        anchors: std::collections::BTreeSet::from(["item".into()]),
        requirement,
    };
    let options = ValidationOptions {
        external: std::collections::BTreeMap::from([
            ("audit".into(), decl("audit", super::Requirement::Required)),
            ("notes".into(), decl("notes", super::Requirement::Advisory)),
        ]),
        ..ValidationOptions::default()
    };
    Arc::new(ValidationPlan::compile(
        &schema,
        &Protocol {
            syntactic_shape: InstanceShapeKind::Graph,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        },
        &options,
    ))
}

/// The open bundles realizing `state` over `plan`: the entries an
/// evaluation of the external checks reports (a rejection with witness
/// digest `b` for `Failed(b)`, a missing witness of the name `w{b}` for
/// `Incomplete(b)`), and for the advisory note an advisory diagnostic of the
/// first required bundle; a skipped instance has no bundle.
fn realize_open(
    plan: &ValidationPlan,
    schedule: &[super::CheckInstance],
    skipped: &[super::CheckInstance],
    state: &Abstract,
) -> std::collections::BTreeMap<super::CheckInstance, super::OpenVerdict> {
    use super::{CheckInstance, Diagnostic, IncompleteCheck, OpenVerdict, Requirement, Violation};
    use crate::identity::Digest;
    use crate::view_ref::ViewRef;
    let spec_of = |instance: &CheckInstance| {
        plan.checks()
            .get(&instance.check)
            .unwrap_or_else(|| panic!("{instance:?} is compiled"))
    };
    let diagnostic = |instance: &CheckInstance, requirement, witness: u8| Diagnostic {
        check: instance.check.clone(),
        class: spec_of(instance).class,
        requirement,
        subject: instance.subject.clone(),
        elements: instance.subject.iter().cloned().collect(),
        violation: Violation::ExternalRejected {
            witness: Digest([witness; 32]),
        },
        source: None,
    };
    let mut open = std::collections::BTreeMap::new();
    for instance in schedule {
        let Some(ViewRef::Node(n)) = instance.subject else {
            panic!("{instance:?} is about a node")
        };
        let required = spec_of(instance).requirement == Requirement::Required;
        let index = usize::try_from(n - 1).unwrap_or(usize::MAX);
        let outcome = if required {
            state.required[index]
        } else {
            state.advisory
        };
        assert_eq!(
            outcome == Outcome::Skipped,
            skipped.contains(instance),
            "{state:?}"
        );
        let mut verdict = OpenVerdict::default();
        match outcome {
            Outcome::Passed | Outcome::Skipped => {}
            Outcome::Failed(b) => {
                verdict
                    .diagnostics
                    .push(diagnostic(instance, spec_of(instance).requirement, b));
            }
            Outcome::Incomplete(b) => verdict.incomplete.push(IncompleteCheck {
                check: instance.check.clone(),
                class: spec_of(instance).class,
                requirement: spec_of(instance).requirement,
                subject: instance.subject.clone(),
                reason: IncompleteReason::MissingExternalWitness {
                    name: format!("w{b}").into(),
                },
            }),
        }
        if required && index == 0 && state.advisory_note && outcome != Outcome::Skipped {
            verdict
                .diagnostics
                .push(diagnostic(instance, Requirement::Advisory, 9));
        }
        if !verdict.diagnostics.is_empty() || !verdict.incomplete.is_empty() {
            open.insert(instance.clone(), verdict);
        }
    }
    open
}

/// The production state realizing `state` over `plan`: the open bundles of
/// [`realize_open`], one `BudgetCut` per check key with a skipped instance
/// (limit 7, the run's schedule and skipped suffix), and the status the
/// aggregate of the entries the state's report holds (every open entry,
/// one key-level budget entry per cut and the plan's static findings).
fn realize(
    plan: &ValidationPlan,
    schedule: &[super::CheckInstance],
    state: &Abstract,
) -> super::ValidationState {
    use super::{BudgetCut, CheckInstance, Diagnostic, IncompleteCheck, aggregate_status};
    let skipped: Vec<CheckInstance> = state
        .cut
        .map_or(Vec::new(), |(from, _)| schedule[from..].to_vec());
    let open = realize_open(plan, schedule, &skipped, state);
    let mut budget_cuts = std::collections::BTreeMap::new();
    if let Some((_, spent)) = state.cut {
        for instance in &skipped {
            budget_cuts
                .entry(instance.check.clone())
                .or_insert_with(|| BudgetCut {
                    limit: 7,
                    spent,
                    from: instance.subject.clone(),
                    scheduled: schedule.to_vec(),
                    skipped: skipped.clone(),
                });
        }
    }
    let diagnostics: Vec<Diagnostic> = open
        .values()
        .flat_map(|v| v.diagnostics.iter().cloned())
        .collect();
    let mut incomplete: Vec<IncompleteCheck> = open
        .values()
        .flat_map(|v| v.incomplete.iter().cloned())
        .collect();
    for (key, cut) in &budget_cuts {
        let spec = plan
            .checks()
            .get(key)
            .unwrap_or_else(|| panic!("{key:?} is compiled"));
        incomplete.push(IncompleteCheck {
            check: key.clone(),
            class: spec.class,
            requirement: spec.requirement,
            subject: None,
            reason: IncompleteReason::Budget {
                limit: cut.limit,
                spent: cut.spent,
                from: cut.from.clone(),
            },
        });
    }
    incomplete.extend(plan.static_findings().iter().cloned());
    super::ValidationState {
        plan: *plan.digest(),
        seq: 0,
        status: aggregate_status(&diagnostics, &incomplete),
        open,
        budget_cuts,
        coverage: std::collections::BTreeMap::new(),
        external: ExternalVerdicts::default(),
        key_indexes: std::collections::BTreeMap::new(),
        reference_indexes: std::collections::BTreeMap::new(),
    }
}

/// Every abstract state of the scope: with no cut, each required bundle
/// passed, failed or incomplete with either byte value, the advisory note
/// present or not, and the advisory bundle passed or failed; with a cut
/// from each position of the four-instance schedule and either of two
/// spent counts, the same choices for the instances before the cut.
fn every_abstract_state(schedule: &[super::CheckInstance], plan: &ValidationPlan) -> Vec<Abstract> {
    let evaluated = [
        Outcome::Passed,
        Outcome::Failed(0),
        Outcome::Failed(1),
        Outcome::Incomplete(0),
        Outcome::Incomplete(1),
    ];
    let is_required = |instance: &super::CheckInstance| {
        plan.checks()
            .get(&instance.check)
            .is_some_and(|spec| spec.requirement == super::Requirement::Required)
    };
    let mut cuts = vec![None];
    for from in 0..schedule.len() {
        for spent in [0, 1] {
            cuts.push(Some((from, spent)));
        }
    }
    let mut out = Vec::new();
    for cut in cuts {
        let skipped = |position: usize| cut.is_some_and(|(from, _)| position >= from);
        // Each schedule position's choices, in schedule order.
        let choices: Vec<Vec<Outcome>> = schedule
            .iter()
            .enumerate()
            .map(|(position, instance)| {
                if skipped(position) {
                    vec![Outcome::Skipped]
                } else if is_required(instance) {
                    evaluated.to_vec()
                } else {
                    vec![Outcome::Passed, Outcome::Failed(0)]
                }
            })
            .collect();
        let mut combos: Vec<Vec<Outcome>> = vec![Vec::new()];
        for options in &choices {
            combos = combos
                .iter()
                .flat_map(|prefix| {
                    options.iter().map(move |o| {
                        let mut next = prefix.clone();
                        next.push(*o);
                        next
                    })
                })
                .collect();
        }
        for combo in combos {
            let mut required = [Outcome::Passed; 3];
            let mut advisory = Outcome::Passed;
            let mut next = 0;
            for (instance, outcome) in schedule.iter().zip(&combo) {
                if is_required(instance) {
                    required[next] = *outcome;
                    next += 1;
                } else {
                    advisory = *outcome;
                }
            }
            let notes: &[bool] = if required[0] == Outcome::Skipped {
                &[false]
            } else {
                &[false, true]
            };
            for &advisory_note in notes {
                out.push(Abstract {
                    required,
                    advisory_note,
                    advisory,
                    cut,
                });
            }
        }
    }
    out
}

/// The production admission (`admitted` under each `AdmissionPolicy`)
/// agrees with the two predicates on every baseline and every result of
/// the scope: three required bundles, each passed, failed or incomplete
/// with one of two canonical byte values, with and without a budget cut
/// (from each of four schedule positions, with two spent counts), with an
/// advisory diagnostic inside a required bundle or not and an advisory
/// bundle passed or failed. On every `Passed` baseline the two policies
/// admit the same results.
#[test]
fn admission_policies_agree_with_their_predicates_on_three_bundles() {
    use super::state::admitted;
    use super::{CheckInstance, CheckKind, Requirement};
    use crate::view_ref::ViewRef;
    let plan = admission_plan();
    let key = |requirement| {
        plan.checks()
            .iter()
            .find(|(key, spec)| key.kind == CheckKind::External && spec.requirement == requirement)
            .map_or_else(
                || panic!("an external check of requirement {requirement:?}"),
                |(key, _)| key.clone(),
            )
    };
    let (required, advisory) = (key(Requirement::Required), key(Requirement::Advisory));
    let advisory_key = advisory.clone();
    let schedule: Vec<CheckInstance> = [
        (required.clone(), 1),
        (required.clone(), 2),
        (required, 3),
        (advisory, 1),
    ]
    .into_iter()
    .map(|(check, n)| CheckInstance {
        check,
        subject: Some(ViewRef::Node(n)),
    })
    .collect::<std::collections::BTreeSet<_>>()
    .into_iter()
    .collect();
    assert_eq!(
        schedule.last().map(|instance| &instance.check),
        Some(&advisory_key),
        "the advisory instance is scheduled last"
    );
    let states = every_abstract_state(&schedule, &plan);
    let realized: Vec<super::ValidationState> = states
        .iter()
        .map(|s| realize(&plan, &schedule, s))
        .collect();
    for (state, real) in states.iter().zip(&realized) {
        assert_eq!(
            real.status == CheckStatus::Passed,
            require_passed_predicate(state),
            "{state:?}"
        );
    }
    let (mut pairs, mut passed_baselines, mut admitted_counts) = (0_usize, 0_usize, [0_usize; 2]);
    for (baseline, real_baseline) in states.iter().zip(&realized) {
        let passed = require_passed_predicate(baseline);
        passed_baselines += usize::from(passed);
        for (result, real_result) in states.iter().zip(&realized) {
            let require = admitted(
                &plan,
                real_baseline,
                real_result,
                AdmissionPolicy::RequirePassed,
            );
            let tolerate = admitted(
                &plan,
                real_baseline,
                real_result,
                AdmissionPolicy::NoNewFailures,
            );
            assert_eq!(
                require,
                require_passed_predicate(result),
                "{baseline:?} -> {result:?}"
            );
            assert_eq!(
                tolerate,
                no_new_failures_predicate(baseline, result),
                "{baseline:?} -> {result:?}"
            );
            if passed {
                assert_eq!(require, tolerate, "{baseline:?} -> {result:?}");
            }
            admitted_counts[0] += usize::from(require);
            admitted_counts[1] += usize::from(tolerate);
            pairs += 1;
        }
    }
    assert_eq!(pairs, states.len() * states.len());
    eprintln!(
        "admission scope: {} states ({passed_baselines} passed), {pairs} ordered pairs, \
         admitted {} under RequirePassed and {} under NoNewFailures",
        states.len(),
        admitted_counts[0],
        admitted_counts[1]
    );
}
