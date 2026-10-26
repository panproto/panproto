//! Incremental validation state, admission, and replay tests.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::sync::Arc;

use panproto_gat::CheckStatus;
use panproto_inst::graph_edit::{GraphDelta, NodeMeta};
use panproto_inst::locality::{Locality, SummaryKind};
use panproto_inst::validation::{
    AdmissionPolicy, BudgetCut, CheckClass, CheckInstance, CheckKey, CheckKind, Diagnostic,
    EditRejection, EditWitness, ExternalCheckDecl, ExternalVerdict, ExternalVerdicts,
    IncompleteCheck, IncompleteReason, KeyIndexState, OpenVerdict, ReportScope, Requirement,
    RunFinding, RunVerdict, SchemaRef, StateField, Validated, ValidatedGraph, ValidatedShape,
    ValidationBudget, ValidationOptions, ValidationPlan, ValidationReport, ValidationState,
    Violation, WitnessOptions, WitnessReplay, aggregate_status,
};
use panproto_inst::{
    Digest, EdgeId, EditError, FInstance, FieldPresence, GEdge, GInstance, GraphEdit, Node,
    NodeShape, RowId, TableDelta, TableEdit, TreeEdit, ValidatedTable, ValidatedTree, Value,
    ViewRef, WInstance,
};
use panproto_schema::{
    Cardinality, Constraint, ConstraintSemantics, Edge, InstanceShapeKind, Key, KeyPart, KeyRole,
    KeyScope, OnUnresolved, Protocol, RefSource, Reference, ResolveMode, Schema, Vertex,
};
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const WITNESS_PROCESSES: usize = 32;

struct ExternalWitnessFixture {
    plan: Arc<ValidationPlan>,
    baseline: ValidationState,
    prior: GInstance,
    delta: GraphDelta,
    rejected: ExternalVerdicts,
    options: WitnessOptions,
    witness: EditWitness,
}

impl ExternalWitnessFixture {
    fn verifies(&self, candidate: &EditWitness) -> bool {
        candidate
            .verify(&WitnessReplay {
                plan: &self.plan,
                baseline: &self.baseline,
                prior: &self.prior,
                delta: &self.delta,
                policy: AdmissionPolicy::RequirePassed,
                budget: &ValidationBudget::default(),
                external: &self.rejected,
                expected_options: self.options,
            })
            .is_ok()
    }
}

fn plan(shape: InstanceShapeKind) -> Arc<ValidationPlan> {
    let mut schema = Schema::default();
    schema.vertices.insert(
        "item".into(),
        Vertex {
            id: "item".into(),
            kind: "object".into(),
            nsid: None,
        },
    );
    Arc::new(ValidationPlan::compile(
        &schema,
        &Protocol {
            syntactic_shape: shape,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        },
        &ValidationOptions::default(),
    ))
}

fn two_anchor_plan() -> Arc<ValidationPlan> {
    two_anchor_plan_with_options(&ValidationOptions::default())
}

fn two_anchor_plan_with_options(options: &ValidationOptions) -> Arc<ValidationPlan> {
    let mut schema = Schema::default();
    for id in ["item", "renamed"] {
        schema.vertices.insert(
            id.into(),
            Vertex {
                id: id.into(),
                kind: "object".into(),
                nsid: None,
            },
        );
    }
    Arc::new(ValidationPlan::compile(
        &schema,
        &Protocol {
            syntactic_shape: InstanceShapeKind::Graph,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        },
        options,
    ))
}

fn external_plan(requirement: Requirement) -> Arc<ValidationPlan> {
    external_plan_for(InstanceShapeKind::Graph, requirement)
}

fn external_plan_for(shape: InstanceShapeKind, requirement: Requirement) -> Arc<ValidationPlan> {
    let mut schema = Schema::default();
    schema.vertices.insert(
        "item".into(),
        Vertex {
            id: "item".into(),
            kind: "object".into(),
            nsid: None,
        },
    );
    let options = ValidationOptions {
        external: BTreeMap::from([(
            "audit".into(),
            ExternalCheckDecl {
                name: "audit".into(),
                anchors: BTreeSet::from(["item".into()]),
                requirement,
            },
        )]),
        ..ValidationOptions::default()
    };
    Arc::new(ValidationPlan::compile(
        &schema,
        &Protocol {
            syntactic_shape: shape,
            semantic_shape: shape,
            ..Protocol::default()
        },
        &options,
    ))
}

fn declare_edge(schema: &mut Schema, edge: Edge) {
    schema.edges.insert(edge.clone(), edge.kind.clone());
    schema
        .outgoing
        .entry(edge.src.clone())
        .or_default()
        .push(edge.clone());
    schema
        .incoming
        .entry(edge.tgt.clone())
        .or_default()
        .push(edge.clone());
    schema
        .between
        .entry((edge.src.clone(), edge.tgt.clone()))
        .or_default()
        .push(edge);
}

fn endpoint_plan() -> (Arc<ValidationPlan>, Edge, Edge, Edge) {
    let ab = Edge {
        src: "a".into(),
        tgt: "b".into(),
        kind: "prop".into(),
        name: Some("ab".into()),
    };
    let ba = Edge {
        src: "b".into(),
        tgt: "a".into(),
        kind: "prop".into(),
        name: Some("ba".into()),
    };
    let ac = Edge {
        src: "a".into(),
        tgt: "c".into(),
        kind: "prop".into(),
        name: Some("ac".into()),
    };
    let mut schema = Schema::default();
    for id in ["a", "b", "c"] {
        schema.vertices.insert(
            id.into(),
            Vertex {
                id: id.into(),
                kind: "object".into(),
                nsid: None,
            },
        );
    }
    for edge in [&ab, &ba, &ac] {
        declare_edge(&mut schema, edge.clone());
    }
    let plan = Arc::new(ValidationPlan::compile(
        &schema,
        &Protocol {
            syntactic_shape: InstanceShapeKind::Graph,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        },
        &ValidationOptions::default(),
    ));
    (plan, ab, ba, ac)
}

fn mixed_table_plan() -> Arc<ValidationPlan> {
    let collection = Edge {
        src: "doc".into(),
        tgt: "record".into(),
        kind: "array".into(),
        name: Some("records".into()),
    };
    let id = Edge {
        src: "record".into(),
        tgt: "record.id".into(),
        kind: "prop".into(),
        name: Some("id".into()),
    };
    let code = Edge {
        src: "record".into(),
        tgt: "record.code".into(),
        kind: "prop".into(),
        name: Some("code".into()),
    };
    let extra = Edge {
        src: "record".into(),
        tgt: "record.extra".into(),
        kind: "prop".into(),
        name: Some("extra".into()),
    };
    let mut schema = Schema::default();
    for id in ["doc", "record", "record.id", "record.code", "record.extra"] {
        schema.vertices.insert(
            id.into(),
            Vertex {
                id: id.into(),
                kind: "object".into(),
                nsid: None,
            },
        );
    }
    for edge in [&collection, &id, &code, &extra] {
        declare_edge(&mut schema, edge.clone());
    }
    schema.orderings.insert(collection.clone(), 0);
    schema.declarations.insert_key(
        "record".into(),
        Key {
            id: "primary".into(),
            parts: vec![
                KeyPart::Position { within: collection },
                KeyPart::Path(vec![id]),
                KeyPart::Path(vec![code]),
            ],
            role: KeyRole::Primary,
            scope: KeyScope::Within("doc".into()),
        },
    );
    let options = ValidationOptions {
        external: BTreeMap::from([(
            "audit".into(),
            ExternalCheckDecl {
                name: "audit".into(),
                anchors: BTreeSet::from(["record".into()]),
                requirement: Requirement::Advisory,
            },
        )]),
        ..ValidationOptions::default()
    };
    Arc::new(ValidationPlan::compile(
        &schema,
        &Protocol {
            syntactic_shape: InstanceShapeKind::Table,
            semantic_shape: InstanceShapeKind::Table,
            ..Protocol::default()
        },
        &options,
    ))
}

fn mixed_table(rows: u32) -> FInstance {
    FInstance::new()
        .with_rows(
            "record",
            (0..rows).map(|id| {
                (
                    RowId(id),
                    HashMap::from([("extra".to_owned(), Value::Str("old".into()))]),
                )
            }),
        )
        .expect("the record rows are well formed")
}

fn nan_key_plan() -> Arc<ValidationPlan> {
    let mut schema = Schema::default();
    schema.vertices.insert(
        "item".into(),
        Vertex {
            id: "item".into(),
            kind: "object".into(),
            nsid: None,
        },
    );
    schema.declarations.insert_key(
        "item".into(),
        Key {
            id: "nan".into(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::Unique,
            scope: KeyScope::Document,
        },
    );
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

fn deterministic_witness_bytes() -> Result<String, Box<dyn std::error::Error>> {
    let plan = two_anchor_plan();
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let external = ExternalVerdicts::default();
    let (mut validated, _) =
        ValidatedGraph::new(plan, graph, &ValidationBudget::default(), &external);
    let witness = validated.apply_edit_with_options(
        &GraphEdit::RelabelNode {
            node: 0,
            old_anchor: "item".into(),
            new_anchor: "renamed".into(),
        },
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &external,
        &WitnessOptions { digests: true },
    )?;
    let bytes = witness.canonical_bytes();
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(hex, "{byte:02x}").expect("writing to a string cannot fail");
    }
    Ok(hex)
}

fn assert_duplicate_key_is_rejected(
    state: &panproto_inst::ValidationState,
    pointer: &str,
) -> TestResult {
    let mut duplicate = serde_json::to_value(state)?;
    let entries = duplicate
        .pointer_mut(pointer)
        .and_then(serde_json::Value::as_array_mut)
        .expect("the map uses btree_map_as_vec and is nonempty");
    entries.push(entries[0].clone());
    assert!(serde_json::from_value::<panproto_inst::ValidationState>(duplicate).is_err());
    Ok(())
}

fn assert_state_duplicate_keys_are_rejected(state: &ValidationState) -> TestResult {
    for pointer in [
        "/open",
        "/budget_cuts",
        "/coverage",
        "/external",
        "/key_indexes",
        "/reference_indexes",
        "/key_indexes/0/1/buckets",
        "/key_indexes/0/1/by_value",
        "/key_indexes/0/1/values",
        "/reference_indexes/0/1/buckets",
        "/reference_indexes/0/1/by_value",
        "/reference_indexes/0/1/values",
    ] {
        assert_duplicate_key_is_rejected(state, pointer)?;
    }
    Ok(())
}

#[test]
fn one_process_witness_dump() -> TestResult {
    if std::env::var_os("PP_VALIDATION_WITNESS_DUMP").is_some() {
        print!("<<<{}>>>", deterministic_witness_bytes()?);
    }
    Ok(())
}

#[test]
fn validation_witnesses_are_identical_across_processes() -> TestResult {
    if std::env::var_os("PP_VALIDATION_WITNESS_DUMP").is_some() {
        return Ok(());
    }
    let exe = std::env::current_exe()?;
    let expected = deterministic_witness_bytes()?;
    for _ in 0..WITNESS_PROCESSES {
        let output = std::process::Command::new(&exe)
            .args(["one_process_witness_dump", "--exact", "--nocapture"])
            .env("PP_VALIDATION_WITNESS_DUMP", "1")
            .output()?;
        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout)?;
        let start = stdout.find("<<<").expect("the child printed its dump") + 3;
        let end = stdout.rfind(">>>").expect("the child closed its dump");
        assert_eq!(&stdout[start..end], expected);
    }
    Ok(())
}

#[test]
fn validated_graph_applies_atomically_and_its_witness_replays() -> TestResult {
    let plan = plan(InstanceShapeKind::Graph);
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let (mut validated, report) = ValidatedGraph::new(
        plan.clone(),
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(report.status, CheckStatus::Passed);
    assert_eq!(
        validated.report().canonical_bytes(),
        report.canonical_bytes()
    );

    let baseline = validated.state().clone();
    let prior = validated.instance().clone();
    let edit = GraphEdit::Identity;
    let delta = edit.to_delta()?;
    let witness = validated.apply_edit_with_options(
        &edit,
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
        &WitnessOptions { digests: true },
    )?;
    assert!(witness.prior.is_some() && witness.result.is_some());
    witness.verify(&WitnessReplay {
        plan: &plan,
        baseline: &baseline,
        prior: &prior,
        delta: &delta,
        policy: AdmissionPolicy::RequirePassed,
        budget: &ValidationBudget::default(),
        external: &ExternalVerdicts::default(),
        expected_options: WitnessOptions { digests: true },
    })?;
    assert_eq!(validated.state().seq, 1);
    Ok(())
}

#[test]
fn successful_witnesses_bind_graph_table_and_tree_contexts_and_digests() -> TestResult {
    let budget = ValidationBudget::default();
    let external = ExternalVerdicts::default();
    let options = WitnessOptions { digests: true };

    check_successful_graph_witness(&budget, &external, options)?;
    check_successful_table_witness(&budget, &external, options)?;
    check_successful_tree_witness(&budget, &external, options)
}

fn check_successful_graph_witness(
    budget: &ValidationBudget,
    external: &ExternalVerdicts,
    options: WitnessOptions,
) -> TestResult {
    let graph_plan = plan(InstanceShapeKind::Graph);
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let (mut graph_validated, _) = ValidatedGraph::new(graph_plan.clone(), graph, budget, external);
    let graph_baseline = graph_validated.state().clone();
    let graph_prior = graph_validated.instance().clone();
    let graph_edit = GraphEdit::AddNode {
        node: Node::new(1, "item"),
    };
    let graph_delta = graph_edit.to_delta()?;
    let graph_witness = graph_validated.apply_edit_with_options(
        &graph_edit,
        AdmissionPolicy::RequirePassed,
        budget,
        external,
        &options,
    )?;
    assert_eq!(graph_witness.prior, Some(graph_prior.digest()));
    assert_eq!(
        graph_witness.result,
        Some(graph_validated.instance().digest())
    );
    graph_witness.verify(&WitnessReplay {
        plan: &graph_plan,
        baseline: &graph_baseline,
        prior: &graph_prior,
        delta: &graph_delta,
        policy: AdmissionPolicy::RequirePassed,
        budget,
        external,
        expected_options: options,
    })?;
    Ok(())
}

fn check_successful_table_witness(
    budget: &ValidationBudget,
    external: &ExternalVerdicts,
    options: WitnessOptions,
) -> TestResult {
    let table_plan = plan(InstanceShapeKind::Table);
    let table = FInstance::new();
    let (mut table_validated, _) = ValidatedTable::new(table_plan.clone(), table, budget, external);
    let table_baseline = table_validated.state().clone();
    let table_prior = table_validated.instance().clone();
    let table_edit = TableEdit::InsertRow {
        table: "item".into(),
        id: RowId(0),
        row: HashMap::new(),
    };
    let table_delta = table_edit.to_delta()?;
    let table_witness = table_validated.apply_edit_with_options(
        &table_edit,
        AdmissionPolicy::RequirePassed,
        budget,
        external,
        &options,
    )?;
    assert_eq!(table_witness.prior, Some(table_prior.digest()));
    assert_eq!(
        table_witness.result,
        Some(table_validated.instance().digest())
    );
    table_witness.verify(&WitnessReplay {
        plan: &table_plan,
        baseline: &table_baseline,
        prior: &table_prior,
        delta: &table_delta,
        policy: AdmissionPolicy::RequirePassed,
        budget,
        external,
        expected_options: options,
    })?;
    Ok(())
}

fn check_successful_tree_witness(
    budget: &ValidationBudget,
    external: &ExternalVerdicts,
    options: WitnessOptions,
) -> TestResult {
    let next = Edge {
        src: "item".into(),
        tgt: "item".into(),
        kind: "prop".into(),
        name: Some("next".into()),
    };
    let mut tree_schema = Schema::default();
    tree_schema.vertices.insert(
        "item".into(),
        Vertex {
            id: "item".into(),
            kind: "object".into(),
            nsid: None,
        },
    );
    declare_edge(&mut tree_schema, next.clone());
    let tree_plan = Arc::new(ValidationPlan::compile(
        &tree_schema,
        &Protocol {
            syntactic_shape: InstanceShapeKind::Tree,
            semantic_shape: InstanceShapeKind::Tree,
            ..Protocol::default()
        },
        &ValidationOptions::default(),
    ));
    let tree = WInstance::new(
        HashMap::from([(0, Node::new(0, "item"))]),
        Vec::new(),
        Vec::new(),
        0,
        "item".into(),
    );
    let (mut tree_validated, report) =
        ValidatedTree::new(tree_plan.clone(), tree, budget, external);
    assert_eq!(report.status, CheckStatus::Passed);
    let tree_baseline = tree_validated.state().clone();
    let tree_prior = tree_validated.instance().clone();
    let tree_edit = TreeEdit::InsertNode {
        parent: 0,
        child_id: 1,
        node: Node::new(1, "item"),
        edge: next,
    };
    let tree_delta = <WInstance as ValidatedShape>::lower(&tree_edit)?;
    let tree_witness = tree_validated.apply_edit_with_options(
        &tree_edit,
        AdmissionPolicy::RequirePassed,
        budget,
        external,
        &options,
    )?;
    assert_eq!(tree_witness.prior, Some(tree_prior.digest()));
    assert_eq!(
        tree_witness.result,
        Some(tree_validated.instance().digest())
    );
    tree_witness.verify(&WitnessReplay {
        plan: &tree_plan,
        baseline: &tree_baseline,
        prior: &tree_prior,
        delta: &tree_delta,
        policy: AdmissionPolicy::RequirePassed,
        budget,
        external,
        expected_options: options,
    })?;
    Ok(())
}

#[test]
fn a_rejected_edit_leaves_state_and_instance_byte_identical() {
    let plan = plan(InstanceShapeKind::Graph);
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let (mut validated, _) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let before_instance = validated.instance().digest();
    let before_state = validated.state().canonical_bytes();
    let result = validated.apply_edit(
        &GraphEdit::AddNode {
            node: Node::new(1, "ghost"),
        },
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert!(matches!(result, Err(EditRejection::NotAdmitted { .. })));
    assert_eq!(validated.instance().digest(), before_instance);
    assert_eq!(validated.state().canonical_bytes(), before_state);
}

#[test]
fn report_is_read_from_state_without_recomputation() {
    let plan = external_plan(Requirement::Required);
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let mut external = ExternalVerdicts::default();
    external.insert(
        "audit".into(),
        ViewRef::Node(0),
        ExternalVerdict {
            passed: true,
            witness: Digest([1; 32]),
        },
    );
    let (validated, initial) =
        ValidatedGraph::new(plan, graph, &ValidationBudget::default(), &external);
    external.insert(
        "audit".into(),
        ViewRef::Node(0),
        ExternalVerdict {
            passed: false,
            witness: Digest([2; 32]),
        },
    );
    assert_eq!(
        validated.report().canonical_bytes(),
        initial.canonical_bytes()
    );
    assert_eq!(validated.report().status, CheckStatus::Passed);
}

#[test]
fn validated_new_retains_mixed_outcomes_and_external_projection() {
    let plan = mixed_table_plan();
    let table = mixed_table(1);
    let mut external = ExternalVerdicts::default();
    external.insert(
        "audit".into(),
        ViewRef::Row(RowId(0)),
        ExternalVerdict {
            passed: false,
            witness: Digest([3; 32]),
        },
    );
    let (validated, report) =
        Validated::<FInstance>::new(plan, table, &ValidationBudget::default(), &external);
    assert_eq!(validated.state().external, external);
    assert_eq!(
        validated.report().canonical_bytes(),
        report.canonical_bytes()
    );
    assert!(
        validated
            .state()
            .open
            .values()
            .any(|verdict| { !verdict.diagnostics.is_empty() && !verdict.incomplete.is_empty() })
    );
}

#[test]
fn state_report_reconstructs_multiple_diagnostics_and_incompletes() {
    let plan = mixed_table_plan();
    let (validated, initial) = Validated::<FInstance>::new(
        plan,
        mixed_table(2),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let reconstructed = validated.report();
    assert_eq!(reconstructed.canonical_bytes(), initial.canonical_bytes());
    assert!(reconstructed.diagnostics.len() >= 4);
    assert!(reconstructed.incomplete.len() >= 2);
    assert_eq!(
        validated
            .state()
            .open
            .values()
            .filter(|verdict| { !verdict.diagnostics.is_empty() && !verdict.incomplete.is_empty() })
            .count(),
        2
    );
}

#[test]
fn apply_edit_runs_exactly_the_footprint() -> TestResult {
    let plan = two_anchor_plan();
    let graph = GInstance::new()
        .with_node(Node::new(0, "item"))
        .with_node(Node::new(1, "item"));
    let (mut validated, _) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let edit = GraphEdit::RelabelNode {
        node: 0,
        old_anchor: "item".into(),
        new_anchor: "renamed".into(),
    };
    let footprint = validated.footprint(&edit, &ExternalVerdicts::default())?;
    let expected: BTreeSet<CheckInstance> =
        footprint.rerun.union(&footprint.created).cloned().collect();
    let witness = validated.apply_edit(
        &edit,
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(
        witness
            .runs
            .iter()
            .map(|run| run.instance.clone())
            .collect::<BTreeSet<_>>(),
        expected
    );
    Ok(())
}

#[test]
fn apply_edit_commits_external_projection_and_complete_bundles() -> TestResult {
    let plan = external_plan(Requirement::Advisory);
    let graph = GInstance::new()
        .with_node(Node::new(0, "item"))
        .with_node(Node::new(1, "item"));
    let mut baseline_external = ExternalVerdicts::default();
    for subject in [ViewRef::Node(0), ViewRef::Node(1)] {
        baseline_external.insert(
            "audit".into(),
            subject,
            ExternalVerdict {
                passed: true,
                witness: Digest([1; 32]),
            },
        );
    }
    let (mut validated, _) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &baseline_external,
    );
    let mut target_external = ExternalVerdicts::default();
    target_external.insert(
        "audit".into(),
        ViewRef::Node(0),
        ExternalVerdict {
            passed: false,
            witness: Digest([2; 32]),
        },
    );
    validated.apply_edit(
        &GraphEdit::Identity,
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &target_external,
    )?;
    assert_eq!(validated.state().external, target_external);
    let external_bundles: Vec<&OpenVerdict> = validated
        .state()
        .open
        .iter()
        .filter(|(instance, _)| instance.check.kind == CheckKind::External)
        .map(|(_, verdict)| verdict)
        .collect();
    assert_eq!(external_bundles.len(), 2);
    assert!(
        external_bundles
            .iter()
            .any(|bundle| { !bundle.diagnostics.is_empty() && bundle.incomplete.is_empty() })
    );
    assert!(
        external_bundles
            .iter()
            .any(|bundle| { bundle.diagnostics.is_empty() && !bundle.incomplete.is_empty() })
    );
    Ok(())
}

#[test]
fn no_new_failures_compares_complete_bundles_by_canonical_bytes_including_nan() -> TestResult {
    let plan = nan_key_plan();
    let value = FieldPresence::Present(Value::Float(f64::NAN));
    let graph = GInstance::new()
        .with_node(Node::new(0, "item").with_value(value.clone()))
        .with_node(Node::new(1, "item").with_value(value));
    let (mut validated, report) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(report.status, CheckStatus::Failed);
    validated.apply_edit(
        &GraphEdit::Identity,
        AdmissionPolicy::NoNewFailures,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(validated.state().status, CheckStatus::Failed);
    Ok(())
}

#[test]
fn no_policy_admits_a_new_required_failure_or_incomplete() {
    for policy in [
        AdmissionPolicy::RequirePassed,
        AdmissionPolicy::NoNewFailures,
    ] {
        let (mut failed, _) = ValidatedGraph::new(
            plan(InstanceShapeKind::Graph),
            GInstance::new().with_node(Node::new(0, "item")),
            &ValidationBudget::default(),
            &ExternalVerdicts::default(),
        );
        assert!(matches!(
            failed.apply_edit(
                &GraphEdit::AddNode {
                    node: Node::new(1, "ghost")
                },
                policy,
                &ValidationBudget::default(),
                &ExternalVerdicts::default(),
            ),
            Err(EditRejection::NotAdmitted { .. })
        ));

        let plan = external_plan(Requirement::Required);
        let mut baseline_external = ExternalVerdicts::default();
        baseline_external.insert(
            "audit".into(),
            ViewRef::Node(0),
            ExternalVerdict {
                passed: true,
                witness: Digest([1; 32]),
            },
        );
        let (mut incomplete, _) = ValidatedGraph::new(
            plan,
            GInstance::new().with_node(Node::new(0, "item")),
            &ValidationBudget::default(),
            &baseline_external,
        );
        assert!(matches!(
            incomplete.apply_edit(
                &GraphEdit::Identity,
                policy,
                &ValidationBudget::default(),
                &ExternalVerdicts::default(),
            ),
            Err(EditRejection::NotAdmitted { .. })
        ));
    }
}

#[test]
fn advisory_outcomes_do_not_control_admission() -> TestResult {
    for policy in [
        AdmissionPolicy::RequirePassed,
        AdmissionPolicy::NoNewFailures,
    ] {
        let plan = external_plan(Requirement::Advisory);
        let graph = GInstance::new().with_node(Node::new(0, "item"));
        let mut baseline_external = ExternalVerdicts::default();
        baseline_external.insert(
            "audit".into(),
            ViewRef::Node(0),
            ExternalVerdict {
                passed: true,
                witness: Digest([1; 32]),
            },
        );
        let (mut validated, _) = ValidatedGraph::new(
            plan,
            graph,
            &ValidationBudget::default(),
            &baseline_external,
        );
        let mut target_external = ExternalVerdicts::default();
        target_external.insert(
            "audit".into(),
            ViewRef::Node(0),
            ExternalVerdict {
                passed: false,
                witness: Digest([2; 32]),
            },
        );
        validated.apply_edit(
            &GraphEdit::Identity,
            policy,
            &ValidationBudget::default(),
            &target_external,
        )?;
        assert_eq!(validated.state().status, CheckStatus::Passed);
        assert!(!validated.report().diagnostics.is_empty());
    }
    Ok(())
}

#[test]
fn an_invalid_baseline_passes_only_when_every_required_open_instance_closes() -> TestResult {
    let plan = plan(InstanceShapeKind::Graph);
    let graph = GInstance::new()
        .with_node(Node::new(0, "ghost"))
        .with_node(Node::new(1, "ghost"));
    let (mut validated, report) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(report.status, CheckStatus::Failed);
    validated.apply_edit(
        &GraphEdit::RelabelNode {
            node: 0,
            old_anchor: "ghost".into(),
            new_anchor: "item".into(),
        },
        AdmissionPolicy::NoNewFailures,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(validated.state().status, CheckStatus::Failed);
    validated.apply_edit(
        &GraphEdit::RelabelNode {
            node: 1,
            old_anchor: "ghost".into(),
            new_anchor: "item".into(),
        },
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(validated.state().status, CheckStatus::Passed);
    Ok(())
}

#[test]
fn no_new_failures_admits_an_unchanged_invalid_bundle() -> TestResult {
    let plan = plan(InstanceShapeKind::Graph);
    let graph = GInstance::new().with_node(Node::new(0, "ghost"));
    let (mut validated, report) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(report.status, CheckStatus::Failed);
    validated.apply_edit(
        &GraphEdit::Identity,
        AdmissionPolicy::NoNewFailures,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(validated.state().status, CheckStatus::Failed);
    Ok(())
}

#[test]
fn from_parts_rejects_each_inconsistent_state_field() {
    let plan = plan(InstanceShapeKind::Graph);
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let (validated, _) = ValidatedGraph::new(
        plan.clone(),
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let (instance, state) = validated.into_parts();

    let mut changed = state.clone();
    changed.plan = Digest([0; 32]);
    assert_eq!(
        ValidatedGraph::from_parts(plan.clone(), instance.clone(), changed)
            .unwrap_err()
            .field,
        StateField::Plan
    );

    let mut changed = state.clone();
    changed.status = CheckStatus::Failed;
    assert_eq!(
        ValidatedGraph::from_parts(plan.clone(), instance.clone(), changed)
            .unwrap_err()
            .field,
        StateField::Status
    );

    let mut changed = state.clone();
    changed.coverage.clear();
    assert_eq!(
        ValidatedGraph::from_parts(plan.clone(), instance.clone(), changed)
            .unwrap_err()
            .field,
        StateField::Coverage
    );

    let key = state
        .coverage
        .keys()
        .next()
        .cloned()
        .expect("compiled check");
    let mut changed = state.clone();
    changed
        .key_indexes
        .insert(key.clone(), KeyIndexState::default());
    assert_eq!(
        ValidatedGraph::from_parts(plan.clone(), instance.clone(), changed)
            .unwrap_err()
            .field,
        StateField::KeyIndexes
    );

    let mut changed = state;
    changed
        .reference_indexes
        .insert(key, KeyIndexState::default());
    assert_eq!(
        ValidatedGraph::from_parts(plan, instance, changed)
            .unwrap_err()
            .field,
        StateField::ReferenceIndexes
    );

    assert_from_parts_rejects_open_budget_and_external();
}

fn assert_from_parts_rejects_open_budget_and_external() {
    let open_plan = two_anchor_plan();
    let (open_validated, _) = ValidatedGraph::new(
        open_plan.clone(),
        GInstance::new().with_node(Node::new(0, "ghost")),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let (open_instance, mut open_state) = open_validated.into_parts();
    assert!(!open_state.open.is_empty());
    let expected_instance = open_state
        .open
        .keys()
        .next()
        .cloned()
        .expect("the invalid anchor has an open outcome");
    open_state.open.clear();
    let mismatch = ValidatedGraph::from_parts(open_plan, open_instance, open_state).unwrap_err();
    assert_eq!(mismatch.field, StateField::Open);
    assert_eq!(mismatch.instance.as_deref(), Some(&expected_instance));

    let cut_plan = two_anchor_plan();
    let (cut_validated, _) = ValidatedGraph::new(
        cut_plan.clone(),
        GInstance::new().with_node(Node::new(0, "item")),
        &ValidationBudget { max_reads: Some(0) },
        &ExternalVerdicts::default(),
    );
    let (cut_instance, mut cut_state) = cut_validated.into_parts();
    let cut = cut_state
        .budget_cuts
        .values_mut()
        .next()
        .expect("zero reads leaves a cut");
    cut.spent = cut.spent.saturating_add(1);
    assert_eq!(
        ValidatedGraph::from_parts(cut_plan, cut_instance, cut_state)
            .unwrap_err()
            .field,
        StateField::BudgetCuts
    );

    let external_plan = external_plan(Requirement::Required);
    let mut active = ExternalVerdicts::default();
    active.insert(
        "audit".into(),
        ViewRef::Node(0),
        ExternalVerdict {
            passed: true,
            witness: Digest([1; 32]),
        },
    );
    let (external_validated, _) = ValidatedGraph::new(
        external_plan.clone(),
        GInstance::new().with_node(Node::new(0, "item")),
        &ValidationBudget::default(),
        &active,
    );
    let (external_instance, mut external_state) = external_validated.into_parts();
    external_state.external.insert(
        "audit".into(),
        ViewRef::Node(99),
        ExternalVerdict {
            passed: true,
            witness: Digest([2; 32]),
        },
    );
    assert_eq!(
        ValidatedGraph::from_parts(external_plan, external_instance, external_state)
            .unwrap_err()
            .field,
        StateField::External
    );
}

#[test]
fn validation_state_bytes_observe_nested_keys_and_payloads() {
    let plan = two_anchor_plan();
    let (validated, _) = ValidatedGraph::new(
        plan.clone(),
        GInstance::new().with_node(Node::new(0, "item")),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let state = validated.state().clone();
    let key = plan
        .checks()
        .keys()
        .next()
        .cloned()
        .expect("the structural plan has a compiled check");

    let with_open_subject = |subject| {
        let mut candidate = state.clone();
        candidate.open = BTreeMap::from([(
            CheckInstance {
                check: key.clone(),
                subject: Some(ViewRef::Node(subject)),
            },
            OpenVerdict::default(),
        )]);
        candidate
    };
    assert_ne!(
        with_open_subject(0).canonical_bytes(),
        with_open_subject(1).canonical_bytes(),
        "an open outcome's instance and optional subject are encoded"
    );
    let mut open_payload_changed = with_open_subject(0);
    open_payload_changed
        .open
        .values_mut()
        .next()
        .expect("the candidate has one open verdict")
        .incomplete
        .push(IncompleteCheck {
            check: key.clone(),
            class: CheckClass::GraphLocal,
            requirement: Requirement::Required,
            subject: Some(ViewRef::Node(0)),
            reason: IncompleteReason::MissingExternalWitness {
                name: "audit".into(),
            },
        });
    assert_ne!(
        with_open_subject(0).canonical_bytes(),
        open_payload_changed.canonical_bytes(),
        "an open verdict's nested payload is encoded"
    );

    let mut coverage_changed = state.clone();
    coverage_changed
        .coverage
        .get_mut(&key)
        .expect("the compiled check has coverage")
        .instances += 1;
    assert_ne!(state.canonical_bytes(), coverage_changed.canonical_bytes());

    let mut external_changed = state.clone();
    external_changed.external.insert(
        "audit".into(),
        ViewRef::Node(0),
        ExternalVerdict {
            passed: true,
            witness: Digest([7; 32]),
        },
    );
    assert_ne!(state.canonical_bytes(), external_changed.canonical_bytes());

    let with_index_element = |element| {
        let mut candidate = state.clone();
        candidate.key_indexes = BTreeMap::from([(
            key.clone(),
            KeyIndexState {
                buckets: BTreeMap::from([((None, vec![7]), vec![ViewRef::Node(element)])]),
                by_value: BTreeMap::new(),
                values: BTreeMap::new(),
            },
        )]);
        candidate
    };
    assert_ne!(
        with_index_element(0).canonical_bytes(),
        with_index_element(1).canonical_bytes(),
        "an index's complete nested view sequence is encoded"
    );
}

#[test]
fn sparse_budget_cuts_commit_exact_scheduled_and_skipped_instances() -> TestResult {
    let options = ValidationOptions {
        requirements: BTreeMap::from([(SchemaRef::Schema, Requirement::Advisory)]),
        ..ValidationOptions::default()
    };
    let plan = two_anchor_plan_with_options(&options);
    let graph = GInstance::new()
        .with_node(Node::new(0, "item"))
        .with_node(Node::new(1, "item"));
    let (mut validated, baseline) = ValidatedGraph::new(
        plan.clone(),
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(baseline.status, CheckStatus::Passed);
    let witness = validated.apply_edit(
        &GraphEdit::RelabelNode {
            node: 0,
            old_anchor: "item".into(),
            new_anchor: "renamed".into(),
        },
        AdmissionPolicy::RequirePassed,
        &ValidationBudget { max_reads: Some(0) },
        &ExternalVerdicts::default(),
    )?;
    let node_anchor = plan
        .checks()
        .keys()
        .find(|key| key.kind == CheckKind::NodeAnchor)
        .cloned()
        .expect("structural plans contain NodeAnchor");
    let carried = CheckInstance {
        check: node_anchor.clone(),
        subject: Some(ViewRef::Node(1)),
    };
    assert!(witness.runs.iter().all(|run| run.instance != carried));
    assert_eq!(validated.state().coverage[&node_anchor].passed, 1);
    assert_eq!(validated.state().coverage[&node_anchor].incomplete, 1);
    assert!(
        validated
            .state()
            .budget_cuts
            .values()
            .all(|cut| !cut.scheduled.contains(&carried) && !cut.skipped.contains(&carried))
    );

    let (instance, state) = validated.into_parts();
    ValidatedGraph::from_parts(plan.clone(), instance.clone(), state.clone())?;
    let mut forged = state.clone();
    forged
        .budget_cuts
        .values_mut()
        .next()
        .expect("zero budget records a cut")
        .spent = 1;
    assert_eq!(
        ValidatedGraph::from_parts(plan.clone(), instance.clone(), forged)
            .unwrap_err()
            .field,
        StateField::BudgetCuts
    );

    let mut forged = state.clone();
    forged
        .budget_cuts
        .values_mut()
        .next()
        .expect("zero budget records a cut")
        .scheduled
        .pop();
    assert_eq!(
        ValidatedGraph::from_parts(plan.clone(), instance.clone(), forged)
            .unwrap_err()
            .field,
        StateField::BudgetCuts
    );

    let mut forged = state;
    let cut = forged
        .budget_cuts
        .values_mut()
        .next()
        .expect("zero budget cuts the scheduled local checks");
    cut.skipped.push(carried);
    cut.skipped.sort();
    assert_eq!(
        ValidatedGraph::from_parts(plan, instance, forged)
            .unwrap_err()
            .field,
        StateField::BudgetCuts
    );
    Ok(())
}

#[test]
fn reattachment_rejects_each_budget_cut_order_and_cross_cut_mismatch() -> TestResult {
    let plan = external_plan(Requirement::Required);
    let graph = GInstance::new()
        .with_node(Node::new(0, "item"))
        .with_node(Node::new(1, "item"));
    let (validated, _) = ValidatedGraph::new(
        plan.clone(),
        graph,
        &ValidationBudget { max_reads: Some(0) },
        &ExternalVerdicts::default(),
    );
    let (instance, state) = validated.into_parts();
    let cut_keys: Vec<_> = state.budget_cuts.keys().cloned().collect();
    assert!(cut_keys.len() >= 2, "the plan has multiple budget-cut keys");
    let first_key = &cut_keys[0];
    let second_key = &cut_keys[1];
    let first = &state.budget_cuts[first_key];
    assert!(first.scheduled.len() >= 2);
    assert!(first.skipped.len() >= 2);
    let duplicate = first.skipped[0].clone();
    let duplicate_at = first
        .scheduled
        .iter()
        .position(|instance| instance == &duplicate)
        .expect("the skipped suffix belongs to the schedule");
    assert!(
        duplicate_at >= 2,
        "the schedule has two evaluated instances before its skipped suffix"
    );
    ValidatedGraph::from_parts(plan.clone(), instance.clone(), state.clone())?;

    let rejects = |forged| {
        assert_eq!(
            ValidatedGraph::from_parts(plan.clone(), instance.clone(), forged)
                .unwrap_err()
                .field,
            StateField::BudgetCuts
        );
    };

    let mut forged = state.clone();
    for cut in forged.budget_cuts.values_mut() {
        cut.scheduled.insert(duplicate_at + 1, duplicate.clone());
        cut.skipped.insert(1, duplicate.clone());
    }
    rejects(forged);

    let mut forged = state.clone();
    for cut in forged.budget_cuts.values_mut() {
        cut.scheduled.swap(0, 1);
    }
    rejects(forged);

    let mut forged = state.clone();
    for cut in forged.budget_cuts.values_mut() {
        cut.skipped.swap(0, 1);
    }
    rejects(forged);

    let mut forged = state.clone();
    forged
        .budget_cuts
        .get_mut(second_key)
        .expect("second cut")
        .limit += 1;
    rejects(forged);

    let mut forged = state.clone();
    forged
        .budget_cuts
        .get_mut(second_key)
        .expect("second cut")
        .spent += 1;
    rejects(forged);

    let mut forged = state.clone();
    forged
        .budget_cuts
        .get_mut(second_key)
        .expect("second cut")
        .scheduled
        .pop();
    rejects(forged);

    let mut forged = state;
    forged
        .budget_cuts
        .get_mut(second_key)
        .expect("second cut")
        .skipped
        .pop();
    rejects(forged);
    Ok(())
}

#[test]
fn graph_local_rerun_count_is_independent_of_unrelated_graph_size() -> TestResult {
    let mut schema = Schema::default();
    for id in ["item", "renamed"] {
        schema.vertices.insert(
            id.into(),
            Vertex {
                id: id.into(),
                kind: "object".into(),
                nsid: None,
            },
        );
    }
    let plan = Arc::new(ValidationPlan::compile(
        &schema,
        &Protocol {
            syntactic_shape: InstanceShapeKind::Graph,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        },
        &ValidationOptions::default(),
    ));
    let edit = GraphEdit::RelabelNode {
        node: 0,
        old_anchor: "item".into(),
        new_anchor: "renamed".into(),
    };
    let scheduled = |size| -> Result<BTreeSet<CheckInstance>, Box<dyn std::error::Error>> {
        let mut graph = GInstance::new();
        for id in 0..size {
            graph = graph.with_node(Node::new(id, "item"));
        }
        let (validated, _) = ValidatedGraph::new(
            plan.clone(),
            graph,
            &ValidationBudget::default(),
            &ExternalVerdicts::default(),
        );
        let footprint = validated.footprint(&edit, &ExternalVerdicts::default())?;
        Ok(footprint.rerun.union(&footprint.created).cloned().collect())
    };

    let small = scheduled(2)?;
    let large = scheduled(10_000)?;
    assert_eq!(small, large);
    assert!(!small.is_empty());
    assert!(
        small
            .iter()
            .all(|instance| instance.subject == Some(ViewRef::Node(0)))
    );
    Ok(())
}

fn distant_reference_plan() -> (Arc<ValidationPlan>, Edge, Edge) {
    let edge = |src: &str, tgt: &str, kind: &str, name: &str| Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: kind.into(),
        name: Some(name.into()),
    };
    let id = edge("person", "person.id", "prop", "id");
    let boss = edge("person", "person.boss", "prop", "boss");
    let manages = edge("person", "person", "ref", "boss");
    let mut schema = Schema::default();
    for vertex in ["person", "person.id", "person.boss"] {
        schema.vertices.insert(
            vertex.into(),
            Vertex {
                id: vertex.into(),
                kind: "object".into(),
                nsid: None,
            },
        );
    }
    for declared in [&id, &boss, &manages] {
        schema.edges.insert(declared.clone(), declared.kind.clone());
        schema
            .outgoing
            .entry(declared.src.clone())
            .or_default()
            .push(declared.clone());
        schema
            .incoming
            .entry(declared.tgt.clone())
            .or_default()
            .push(declared.clone());
        schema
            .between
            .entry((declared.src.clone(), declared.tgt.clone()))
            .or_default()
            .push(declared.clone());
    }
    schema.declarations.insert_key(
        "person".into(),
        Key {
            id: "pk".into(),
            parts: vec![KeyPart::Path(vec![id.clone()])],
            role: KeyRole::Identifier,
            scope: KeyScope::Document,
        },
    );
    schema.declarations.insert_reference(Reference {
        id: "boss".into(),
        holder: "person".into(),
        source: RefSource::KeyValue {
            parts: vec![vec![boss.clone()]],
        },
        target_vertex: "person".into(),
        target_key: Some("pk".into()),
        edge: Some(manages),
        cardinality: Cardinality::One,
        nullable: false,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
    });
    let plan = Arc::new(ValidationPlan::compile(
        &schema,
        &Protocol {
            syntactic_shape: InstanceShapeKind::Graph,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        },
        &ValidationOptions::default(),
    ));
    (plan, id, boss)
}

#[test]
fn a_new_key_target_reruns_a_distant_reference_holder() -> TestResult {
    let (plan, id, boss) = distant_reference_plan();
    let graph = GInstance::new()
        .with_node(Node::new(0, "person"))
        .with_node(Node::new(1, "person.boss").with_value(FieldPresence::Present(Value::Int(99))))
        .with_edge(0, 1, boss)
        .with_node(Node::new(4, "person"))
        .with_node(Node::new(5, "person.id").with_value(FieldPresence::Present(Value::Int(10))))
        .with_edge(4, 5, id.clone());
    let (validated, _) = ValidatedGraph::new(
        plan.clone(),
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let edit = GraphEdit::Batch(vec![
        GraphEdit::AddNode {
            node: Node::new(2, "person"),
        },
        GraphEdit::AddNode {
            node: Node::new(3, "person.id").with_value(FieldPresence::Present(Value::Int(99))),
        },
        GraphEdit::AddEdge {
            id: EdgeId(7),
            edge: GEdge::new(2, 3, id),
        },
    ]);
    let footprint = validated.footprint(&edit, &ExternalVerdicts::default())?;
    let reference = plan
        .checks()
        .keys()
        .find(|key| key.kind == CheckKind::ReferenceResolves)
        .cloned()
        .expect("the reference compiles");
    assert!(
        footprint.rerun.contains(&CheckInstance {
            check: reference,
            subject: Some(ViewRef::Node(0)),
        }),
        "rerun={:?} created={:?} retired={:?} touched={:?} reads={:?}",
        footprint.rerun,
        footprint.created,
        footprint.retired,
        footprint.touched,
        footprint.reads
    );
    Ok(())
}

/// Items ordered within a group and keyed by their position, and holders
/// whose `wanted` field names an item by that position: the plan and the
/// group's ordered edge and the holder's resolved edge.
fn position_reference_plan() -> (Arc<ValidationPlan>, Edge, Edge) {
    let contains = Edge {
        src: "group".into(),
        tgt: "item".into(),
        kind: "ordered".into(),
        name: Some("items".into()),
    };
    let wanted = Edge {
        src: "holder".into(),
        tgt: "number".into(),
        kind: "prop".into(),
        name: Some("wanted".into()),
    };
    let resolves = Edge {
        src: "holder".into(),
        tgt: "item".into(),
        kind: "ref".into(),
        name: Some("resolves".into()),
    };
    let mut schema = Schema::default();
    for vertex in ["group", "item", "holder", "number"] {
        schema.vertices.insert(
            vertex.into(),
            Vertex {
                id: vertex.into(),
                kind: if vertex == "number" {
                    "int".into()
                } else {
                    "object".into()
                },
                nsid: None,
            },
        );
    }
    for edge in [&contains, &wanted, &resolves] {
        declare_edge(&mut schema, edge.clone());
    }
    schema.orderings.insert(contains.clone(), 0);
    schema.declarations.insert_key(
        "item".into(),
        Key {
            id: "position".into(),
            parts: vec![KeyPart::Position {
                within: contains.clone(),
            }],
            role: KeyRole::Primary,
            scope: KeyScope::Document,
        },
    );
    schema.declarations.insert_reference(Reference {
        id: "by-position".into(),
        holder: "holder".into(),
        source: RefSource::KeyValue {
            parts: vec![vec![wanted]],
        },
        target_vertex: "item".into(),
        target_key: Some("position".into()),
        edge: Some(resolves.clone()),
        cardinality: Cardinality::One,
        nullable: false,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
    });
    let plan = Arc::new(ValidationPlan::compile(
        &schema,
        &Protocol {
            syntactic_shape: InstanceShapeKind::Graph,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        },
        &ValidationOptions::default(),
    ));
    (plan, contains, resolves)
}

#[test]
fn changing_an_edge_position_reruns_position_requirements_and_references() -> TestResult {
    let (plan, contains, resolves) = position_reference_plan();
    let mut graph = GInstance::new()
        .with_node(Node::new(0, "group"))
        .with_node(Node::new(1, "item"))
        .with_node(Node::new(2, "holder").with_extra_field("wanted", Value::Int(0)));
    graph.insert_edge(EdgeId(0), GEdge::new(0, 1, contains).with_position(Some(0)))?;
    graph.insert_edge(EdgeId(1), GEdge::new(2, 1, resolves))?;
    let (validated, baseline) = ValidatedGraph::new(
        plan.clone(),
        graph.clone(),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(
        baseline.status,
        CheckStatus::Passed,
        "baseline diagnostics: {:?}; incomplete: {:?}",
        baseline.diagnostics,
        baseline.incomplete
    );
    let edit = GraphEdit::SetEdgePosition {
        id: EdgeId(0),
        old: Some(0),
        new: Some(1),
    };
    let footprint = validated.footprint(&edit, &ExternalVerdicts::default())?;
    assert!(footprint.rerun.iter().any(|instance| {
        instance.check.kind == CheckKind::RequiredValue
            && instance.subject == Some(ViewRef::Node(1))
    }));
    assert!(footprint.rerun.iter().any(|instance| {
        instance.check.kind == CheckKind::ReferenceResolves
            && instance.subject == Some(ViewRef::Node(2))
    }));
    let checked = validated.check_edit(
        &edit,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert!(checked.report.diagnostics.iter().any(|diagnostic| {
        diagnostic.check.kind == CheckKind::ReferenceResolves
            && diagnostic.subject == Some(ViewRef::Node(2))
            && matches!(diagnostic.violation, Violation::DanglingReference { .. })
    }));
    let none = ExternalVerdicts::default();
    assert_v3_v4(
        &plan,
        &graph,
        &edit,
        &ValidationBudget::default(),
        &none,
        &none,
    )
}

#[test]
fn changing_node_shape_reruns_references_to_the_changed_target() -> TestResult {
    let mut schema = Schema::default();
    for vertex in ["holder", "target"] {
        schema.vertices.insert(
            vertex.into(),
            Vertex {
                id: vertex.into(),
                kind: "object".into(),
                nsid: None,
            },
        );
    }
    schema.declarations.insert_key(
        "target".into(),
        Key {
            id: "own-value".into(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::Identifier,
            scope: KeyScope::Document,
        },
    );
    schema.declarations.insert_reference(Reference {
        id: "target".into(),
        holder: "holder".into(),
        source: RefSource::KeyValue {
            parts: vec![Vec::new()],
        },
        target_vertex: "target".into(),
        target_key: Some("own-value".into()),
        edge: None,
        cardinality: Cardinality::One,
        nullable: false,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
    });
    let plan = Arc::new(ValidationPlan::compile(
        &schema,
        &Protocol {
            syntactic_shape: InstanceShapeKind::Graph,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        },
        &ValidationOptions::default(),
    ));
    let graph = GInstance::new()
        .with_node(Node::new(0, "holder").with_value(FieldPresence::Present(Value::Int(10))))
        .with_node(
            Node::new(4, "target")
                .with_value(FieldPresence::Present(Value::Int(10)))
                .with_shape(NodeShape::HyperEdge),
        );
    let (validated, baseline) = ValidatedGraph::new(
        plan.clone(),
        graph.clone(),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert!(baseline.diagnostics.iter().any(|diagnostic| {
        diagnostic.check.kind == CheckKind::ReferenceResolves
            && diagnostic.subject == Some(ViewRef::Node(0))
            && matches!(diagnostic.violation, Violation::DanglingReference { .. })
    }));
    let edit = GraphEdit::SetNodeMeta {
        node: 4,
        old: NodeMeta {
            shape: NodeShape::HyperEdge,
            ..NodeMeta::default()
        },
        new: NodeMeta::default(),
    };
    let footprint = validated.footprint(&edit, &ExternalVerdicts::default())?;
    assert!(footprint.rerun.iter().any(|instance| {
        instance.check.kind == CheckKind::ReferenceResolves
            && instance.subject == Some(ViewRef::Node(0))
    }));
    let checked = validated.check_edit(
        &edit,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert!(!checked.report.diagnostics.iter().any(|diagnostic| {
        diagnostic.check.kind == CheckKind::ReferenceResolves
            && diagnostic.subject == Some(ViewRef::Node(0))
    }));
    let none = ExternalVerdicts::default();
    assert_v3_v4(
        &plan,
        &graph,
        &edit,
        &ValidationBudget::default(),
        &none,
        &none,
    )
}

/// Removing an item's edge position leaves its primary key, its position
/// among the group's ordered items, to be counted among its siblings: the
/// key requirement's verdict is unchanged but its trace now reads the group,
/// so the instance is rerun and its read set replaced.
#[test]
fn removing_an_edge_position_reruns_the_positional_key_requirement() -> TestResult {
    let (plan, contains, _) = position_reference_plan();
    let mut graph = GInstance::new()
        .with_node(Node::new(0, "group"))
        .with_node(Node::new(1, "item"));
    graph.insert_edge(EdgeId(0), GEdge::new(0, 1, contains).with_position(Some(0)))?;
    let (validated, _) = ValidatedGraph::new(
        plan.clone(),
        graph.clone(),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let edit = GraphEdit::SetEdgePosition {
        id: EdgeId(0),
        old: Some(0),
        new: None,
    };
    let requirement = validated
        .footprint(&edit, &ExternalVerdicts::default())?
        .rerun
        .into_iter()
        .find(|instance| {
            instance.check.kind == CheckKind::RequiredValue
                && instance.subject == Some(ViewRef::Node(1))
        })
        .ok_or("the positional key requirement is rerun")?;
    let none = ExternalVerdicts::default();
    let mut post = graph.clone();
    edit.to_delta()?.apply(&mut post)?;
    assert_eq!(
        graph.reads(&plan, &none, &requirement),
        BTreeSet::from([ViewRef::Node(1), ViewRef::Edge(EdgeId(0))])
    );
    assert_eq!(
        post.reads(&plan, &none, &requirement),
        BTreeSet::from([ViewRef::Node(0), ViewRef::Node(1), ViewRef::Edge(EdgeId(0))])
    );
    assert_v3_v4(
        &plan,
        &graph,
        &edit,
        &ValidationBudget::default(),
        &none,
        &none,
    )
}

#[test]
fn key_required_value_locality_distinguishes_own_and_remote_parts() {
    let path = Edge {
        src: "remote".into(),
        tgt: "scalar".into(),
        kind: "prop".into(),
        name: Some("value".into()),
    };
    let within = Edge {
        src: "root".into(),
        tgt: "positioned".into(),
        kind: "ordered".into(),
        name: Some("items".into()),
    };
    let mut schema = Schema::default();
    for vertex in ["own", "remote", "scalar", "root", "positioned"] {
        schema.vertices.insert(
            vertex.into(),
            Vertex {
                id: vertex.into(),
                kind: "object".into(),
                nsid: None,
            },
        );
    }
    for edge in [&path, &within] {
        declare_edge(&mut schema, edge.clone());
    }
    schema.orderings.insert(within.clone(), 0);
    for (vertex, id, part) in [
        ("own", "own", KeyPart::Path(Vec::new())),
        ("remote", "remote", KeyPart::Path(vec![path])),
        ("positioned", "position", KeyPart::Position { within }),
    ] {
        schema.declarations.insert_key(
            vertex.into(),
            Key {
                id: id.into(),
                parts: vec![part],
                role: KeyRole::Primary,
                scope: KeyScope::Document,
            },
        );
    }
    let plan =
        ValidationPlan::compile(&schema, &Protocol::default(), &ValidationOptions::default());
    let required = |id: &str| {
        plan.checks()
            .iter()
            .find(|(key, _)| {
                key.kind == CheckKind::RequiredValue
                    && matches!(&key.decl, SchemaRef::Key { key, .. } if key.as_ref() == id)
            })
            .map(|(_, spec)| spec)
            .expect("the primary key compiles RequiredValue")
    };
    let own = required("own");
    assert_eq!(own.class, CheckClass::GraphLocal);
    assert!(
        own.locality
            .values()
            .all(|locality| *locality == Locality::Local)
    );
    for id in ["remote", "position"] {
        let spec = required(id);
        assert_eq!(spec.class, CheckClass::MaintainedGlobal);
        assert!(
            spec.locality
                .values()
                .all(|locality| { *locality == Locality::Summary(SummaryKind::SortedMerge) })
        );
        assert_eq!(
            spec.locality.len(),
            3,
            "RequiredValue applies to all shapes"
        );
    }
}

#[test]
fn changing_a_primary_key_leaf_reruns_the_required_holder() -> TestResult {
    let key_edge = Edge {
        src: "record".into(),
        tgt: "record.id".into(),
        kind: "prop".into(),
        name: Some("id".into()),
    };
    let mut schema = Schema::default();
    for vertex in ["record", "record.id"] {
        schema.vertices.insert(
            vertex.into(),
            Vertex {
                id: vertex.into(),
                kind: "object".into(),
                nsid: None,
            },
        );
    }
    declare_edge(&mut schema, key_edge.clone());
    schema.declarations.insert_key(
        "record".into(),
        Key {
            id: "primary".into(),
            parts: vec![KeyPart::Path(vec![key_edge.clone()])],
            role: KeyRole::Primary,
            scope: KeyScope::Document,
        },
    );
    let plan = Arc::new(ValidationPlan::compile(
        &schema,
        &Protocol {
            syntactic_shape: InstanceShapeKind::Graph,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        },
        &ValidationOptions::default(),
    ));
    let present = FieldPresence::Present(Value::Int(7));
    let graph = GInstance::new()
        .with_node(Node::new(0, "record"))
        .with_node(Node::new(1, "record.id").with_value(present.clone()))
        .with_edge_id(EdgeId(0), 0, 1, key_edge);
    let external = ExternalVerdicts::default();
    let (validated, baseline) = ValidatedGraph::new(
        plan.clone(),
        graph.clone(),
        &ValidationBudget::default(),
        &external,
    );
    assert_eq!(baseline.status, CheckStatus::Passed);

    let edit = GraphEdit::SetValue {
        node: 1,
        old: Some(present),
        new: Some(FieldPresence::Absent),
    };
    let required = plan
        .checks()
        .keys()
        .find(|key| key.kind == CheckKind::RequiredValue)
        .cloned()
        .expect("a primary key compiles a required-value check");
    let holder = CheckInstance {
        check: required,
        subject: Some(ViewRef::Node(0)),
    };
    let footprint = validated.footprint(&edit, &external)?;
    assert!(footprint.rerun.contains(&holder));
    assert!(footprint.reads.contains(&ViewRef::Node(1)));

    let checked = validated.check_edit(&edit, &ValidationBudget::default(), &external)?;
    assert_eq!(checked.report.status, CheckStatus::Failed);
    assert!(checked.report.diagnostics.iter().any(|diagnostic| {
        diagnostic.check == holder.check && diagnostic.subject == holder.subject
    }));
    let mut target = graph;
    edit.apply(&mut target)?;
    let mut full = plan.validate_graph(&target, &ValidationBudget::default(), &external);
    full.scope = checked.report.scope.clone();
    assert_eq!(checked.report.canonical_bytes(), full.canonical_bytes());
    Ok(())
}

#[test]
fn an_anchor_change_breaking_a_kept_edge_is_incoherent_endpoint() -> TestResult {
    let (plan, ab, _, _) = endpoint_plan();
    let graph = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "b"))
        .with_edge_id(EdgeId(0), 0, 1, ab);
    let (validated, baseline) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(baseline.status, CheckStatus::Passed);
    let checked = validated.check_edit(
        &GraphEdit::RelabelNode {
            node: 1,
            old_anchor: "b".into(),
            new_anchor: "c".into(),
        },
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert!(checked.report.diagnostics.iter().any(|diagnostic| {
        diagnostic.check.kind == CheckKind::EndpointCoherence
            && diagnostic.subject == Some(ViewRef::Edge(EdgeId(0)))
            && matches!(diagnostic.violation, Violation::IncoherentEndpoint { .. })
    }));
    Ok(())
}

#[test]
fn anchor_and_label_changes_trace_both_incident_neighborhoods() -> TestResult {
    let (plan, ab, ba, ac) = endpoint_plan();
    let graph = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "b"))
        .with_node(Node::new(2, "a"))
        .with_edge_id(EdgeId(0), 0, 1, ab.clone())
        .with_edge_id(EdgeId(1), 1, 2, ba);
    let (validated, _) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let anchor = validated.footprint(
        &GraphEdit::RelabelNode {
            node: 1,
            old_anchor: "b".into(),
            new_anchor: "c".into(),
        },
        &ExternalVerdicts::default(),
    )?;
    let endpoint_subjects = |footprint: &panproto_inst::validation::Footprint| {
        footprint
            .rerun
            .iter()
            .filter(|instance| instance.check.kind == CheckKind::EndpointCoherence)
            .filter_map(|instance| instance.subject.clone())
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(
        endpoint_subjects(&anchor),
        BTreeSet::from([ViewRef::Edge(EdgeId(0)), ViewRef::Edge(EdgeId(1))])
    );
    for read in [
        ViewRef::Node(0),
        ViewRef::Node(1),
        ViewRef::Node(2),
        ViewRef::Edge(EdgeId(0)),
        ViewRef::Edge(EdgeId(1)),
    ] {
        assert!(anchor.reads.contains(&read), "missing anchor read {read:?}");
    }

    let label = validated.footprint(
        &GraphEdit::RelabelEdge {
            id: EdgeId(0),
            old: ab,
            new: ac,
        },
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(
        endpoint_subjects(&label),
        BTreeSet::from([ViewRef::Edge(EdgeId(0))])
    );
    for read in [ViewRef::Node(0), ViewRef::Node(1), ViewRef::Edge(EdgeId(0))] {
        assert!(label.reads.contains(&read), "missing label read {read:?}");
    }
    Ok(())
}

fn assert_endpoint_bundle_run(
    witness: &EditWitness,
    instance: &CheckInstance,
    before: &OpenVerdict,
) {
    let endpoint_runs = witness
        .runs
        .iter()
        .filter(|run| run.instance.check.kind == CheckKind::EndpointCoherence)
        .collect::<Vec<_>>();
    let [run] = endpoint_runs.as_slice() else {
        panic!("the relabel must schedule exactly one endpoint run");
    };
    assert_eq!(
        &run.instance, instance,
        "the witness run must retain the exact endpoint check instance"
    );
    assert_eq!(
        run.reads,
        vec![
            ViewRef::Node(1),
            ViewRef::Node(10),
            ViewRef::Edge(EdgeId(0)),
        ],
        "the endpoint run must retain its canonical dependency order"
    );
    let expected_findings = before
        .diagnostics
        .iter()
        .map(|diagnostic| RunFinding {
            elements: diagnostic.elements.clone(),
            violation: diagnostic.violation.clone(),
        })
        .collect::<Vec<_>>();
    let RunVerdict::Evaluated {
        findings,
        incomplete,
    } = &run.verdict
    else {
        panic!("the endpoint run must be evaluated");
    };
    assert_eq!(
        findings, &expected_findings,
        "one run must retain both endpoint findings in state-bundle order"
    );
    assert!(
        incomplete.is_empty(),
        "the complete endpoint run has no incomplete reasons"
    );
}

#[test]
fn rerun_bundles_keep_the_report_canonical_order() -> TestResult {
    let first = Edge {
        src: "source".into(),
        tgt: "target".into(),
        kind: "first".into(),
        name: Some("link".into()),
    };
    let second = Edge {
        kind: "second".into(),
        ..first.clone()
    };
    let mut schema = Schema::default();
    for vertex in ["source", "target"] {
        schema.vertices.insert(
            vertex.into(),
            Vertex {
                id: vertex.into(),
                kind: "object".into(),
                nsid: None,
            },
        );
    }
    declare_edge(&mut schema, first.clone());
    declare_edge(&mut schema, second.clone());
    let plan = Arc::new(ValidationPlan::compile(
        &schema,
        &Protocol {
            syntactic_shape: InstanceShapeKind::Graph,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        },
        &ValidationOptions::default(),
    ));
    let graph = GInstance::new()
        .with_node(Node::new(10, "bad-source"))
        .with_node(Node::new(1, "bad-target"))
        .with_edge_id(EdgeId(0), 10, 1, first.clone());
    let (mut validated, baseline) = ValidatedGraph::new(
        plan.clone(),
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(baseline.status, CheckStatus::Failed);
    let endpoint = plan
        .checks()
        .keys()
        .find(|key| key.kind == CheckKind::EndpointCoherence)
        .cloned()
        .expect("structural plans contain endpoint coherence");
    let instance = CheckInstance {
        check: endpoint,
        subject: Some(ViewRef::Edge(EdgeId(0))),
    };
    let before = validated
        .state()
        .open
        .get(&instance)
        .cloned()
        .expect("both endpoint defects are open");
    assert_eq!(before.diagnostics.len(), 2);
    assert!(before.diagnostics[0].elements < before.diagnostics[1].elements);

    let witness = validated.apply_edit(
        &GraphEdit::RelabelEdge {
            id: EdgeId(0),
            old: first,
            new: second,
        },
        AdmissionPolicy::NoNewFailures,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert_endpoint_bundle_run(&witness, &instance, &before);
    let after = validated
        .state()
        .open
        .get(&instance)
        .expect("the endpoint defects remain open");
    assert_eq!(before, *after);
    Ok(())
}

#[test]
fn check_graph_delta_uses_the_exact_local_footprint() -> TestResult {
    let plan = plan(InstanceShapeKind::Graph);
    let graph = GInstance::new().with_node(Node::new(0, "ghost"));
    let delta = GraphEdit::AddNode {
        node: Node::new(1, "item"),
    }
    .to_delta()?;
    let report = plan.check_graph_delta(
        &graph,
        &delta,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(report.scope, ReportScope::DeltaLocal);
    assert_eq!(report.status, CheckStatus::Passed);
    assert!(report.diagnostics.is_empty());
    Ok(())
}

/// A global kind is incomplete with `NoValidatedBaseline` exactly when the
/// budget does not let the delta-local check evaluate it in full; within
/// budget it is evaluated in full, with the coverage of whole validation.
#[test]
fn delta_local_checks_leave_global_kinds_incomplete() -> TestResult {
    let plan = plan(InstanceShapeKind::Tree);
    let tree = WInstance::new(
        HashMap::from([(0, Node::new(0, "item"))]),
        Vec::new(),
        Vec::new(),
        0,
        "item".into(),
    );
    let reachable = |report: &ValidationReport| {
        report
            .coverage
            .iter()
            .find(|(key, _)| key.kind == CheckKind::TreeReachable)
            .map(|(_, coverage)| *coverage)
            .expect("tree plans compile reachability")
    };
    let no_baseline = |report: &ValidationReport| {
        report.incomplete.iter().any(|entry| {
            entry.check.kind == CheckKind::TreeReachable
                && entry.reason == IncompleteReason::NoValidatedBaseline
        })
    };
    let cut = plan.check_tree_edit(
        &tree,
        &TreeEdit::Identity,
        &ValidationBudget { max_reads: Some(0) },
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(cut.scope, ReportScope::DeltaLocal);
    assert_eq!(cut.status, CheckStatus::Incomplete);
    assert!(no_baseline(&cut));
    let coverage = reachable(&cut);
    assert_eq!((coverage.instances, coverage.incomplete), (1, 1));

    let full = plan.check_tree_edit(
        &tree,
        &TreeEdit::Identity,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(full.status, CheckStatus::Passed);
    assert!(!no_baseline(&full));
    let whole = plan.validate_tree(
        &tree,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(reachable(&full), reachable(&whole));
    Ok(())
}

#[test]
fn check_graph_delta_uses_the_supplied_external_projection() -> TestResult {
    let plan = external_plan(Requirement::Required);
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let delta = GraphEdit::AddNode {
        node: Node::new(1, "item"),
    }
    .to_delta()?;
    let missing = plan.check_graph_delta(
        &graph,
        &delta,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert!(missing.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::External
            && entry.subject == Some(ViewRef::Node(1))
            && matches!(
                entry.reason,
                IncompleteReason::MissingExternalWitness { .. }
            )
    }));

    let mut external = ExternalVerdicts::default();
    external.insert(
        "audit".into(),
        ViewRef::Node(1),
        ExternalVerdict {
            passed: true,
            witness: Digest([9; 32]),
        },
    );
    let accepted =
        plan.check_graph_delta(&graph, &delta, &ValidationBudget::default(), &external)?;
    assert_eq!(accepted.status, CheckStatus::Passed);
    assert!(accepted.incomplete.is_empty());
    Ok(())
}

#[test]
fn delta_local_external_checks_use_the_supplied_projection() -> TestResult {
    check_graph_delta_uses_the_supplied_external_projection()
}

#[test]
fn check_table_delta_uses_the_supplied_external_projection() -> TestResult {
    let plan = external_plan_for(InstanceShapeKind::Table, Requirement::Required);
    let table = FInstance::new();
    let delta = TableEdit::InsertRow {
        table: "item".into(),
        id: RowId(0),
        row: HashMap::new(),
    }
    .to_delta()?;
    let missing = plan.check_table_delta(
        &table,
        &delta,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert!(missing.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::External
            && entry.subject == Some(ViewRef::Row(RowId(0)))
            && matches!(
                entry.reason,
                IncompleteReason::MissingExternalWitness { .. }
            )
    }));
    let mut external = ExternalVerdicts::default();
    external.insert(
        "audit".into(),
        ViewRef::Row(RowId(0)),
        ExternalVerdict {
            passed: true,
            witness: Digest([10; 32]),
        },
    );
    let accepted =
        plan.check_table_delta(&table, &delta, &ValidationBudget::default(), &external)?;
    assert_eq!(accepted.status, CheckStatus::Passed);
    assert!(accepted.incomplete.is_empty());
    Ok(())
}

#[test]
fn incremental_validation_types_round_trip_in_four_encodings() -> TestResult {
    fn round_trip<T>(value: &T) -> TestResult
    where
        T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_vec(value)?;
        assert_eq!(value, &serde_json::from_slice::<T>(&json)?);
        let python = serde_json::to_value(value)?;
        assert_eq!(value, &serde_json::from_value::<T>(python)?);
        let message_pack = rmp_serde::to_vec_named(value)?;
        assert_eq!(value, &rmp_serde::from_slice::<T>(&message_pack)?);
        let mut cbor = Vec::new();
        ciborium::into_writer(value, &mut cbor)?;
        assert_eq!(value, &ciborium::from_reader::<T, _>(cbor.as_slice())?);
        Ok(())
    }

    let plan = external_plan(Requirement::Required);
    let graph = GInstance::new()
        .with_node(Node::new(0, "item"))
        .with_node(Node::new(1, "item"));
    let mut external = ExternalVerdicts::default();
    external.insert(
        "audit".into(),
        ViewRef::Node(0),
        ExternalVerdict {
            passed: true,
            witness: Digest([7; 32]),
        },
    );
    let budget = ValidationBudget { max_reads: Some(1) };
    let (mut validated, _) = ValidatedGraph::new(plan.clone(), graph, &budget, &external);
    let validation = validated.check_edit(&GraphEdit::Identity, &budget, &external)?;
    let witness = validated.apply_edit(
        &GraphEdit::Identity,
        AdmissionPolicy::NoNewFailures,
        &budget,
        &external,
    )?;
    assert!(
        witness
            .runs
            .iter()
            .any(|run| matches!(run.verdict, RunVerdict::Evaluated { .. }))
    );
    assert!(
        witness
            .runs
            .iter()
            .any(|run| matches!(run.verdict, RunVerdict::Skipped { .. }))
    );

    let diagnostic = plan
        .validate_graph(
            &GInstance::new().with_node(Node::new(7, "ghost")),
            &ValidationBudget::default(),
            &external,
        )
        .diagnostics
        .into_iter()
        .next()
        .expect("the ghost node is an evaluated failure");
    let at = panproto_inst::validation::CheckInstance {
        check: diagnostic.check.clone(),
        subject: diagnostic.subject.clone(),
    };
    let mut state = validated.state().clone();
    state.open.insert(
        at,
        OpenVerdict {
            diagnostics: vec![diagnostic.clone()],
            incomplete: vec![IncompleteCheck {
                check: diagnostic.check,
                class: diagnostic.class,
                requirement: diagnostic.requirement,
                subject: diagnostic.subject,
                reason: IncompleteReason::MissingExternalWitness {
                    name: "audit".into(),
                },
            }],
        },
    );
    assert!(!state.external.0.is_empty());
    assert!(!state.budget_cuts.is_empty());
    let index_key = state
        .coverage
        .keys()
        .next()
        .cloned()
        .expect("compiled check");
    let index = KeyIndexState {
        buckets: BTreeMap::from([((None, vec![1]), vec![ViewRef::Node(0)])]),
        by_value: BTreeMap::from([(vec![1], vec![ViewRef::Node(0)])]),
        values: BTreeMap::from([(ViewRef::Node(0), vec![Value::Int(1)])]),
    };
    state.key_indexes.insert(index_key.clone(), index.clone());
    state.reference_indexes.insert(index_key, index);

    round_trip(&state)?;
    round_trip(&validation.footprint)?;
    round_trip(&validation)?;
    round_trip(&witness)?;

    assert_state_duplicate_keys_are_rejected(&state)
}

#[test]
fn all_three_shapes_have_the_exact_surface_and_delta_pairs() -> TestResult {
    let graph_delta = <GInstance as ValidatedShape>::lower(&GraphEdit::Identity)?;
    assert!(graph_delta.is_identity());
    let table_delta = <FInstance as ValidatedShape>::lower(&TableEdit::Identity)?;
    assert!(table_delta.is_identity());
    let tree_delta = <WInstance as ValidatedShape>::lower(&TreeEdit::Sequence(vec![
        TreeEdit::Identity,
        TreeEdit::DeleteNode { id: 1 },
    ]))?;
    assert_eq!(tree_delta.steps().len(), 1);

    let tree = WInstance::new(
        HashMap::from([(0, Node::new(0, "item"))]),
        Vec::new(),
        Vec::new(),
        0,
        "item".into(),
    );
    let (mut validated, _) = ValidatedTree::new(
        plan(InstanceShapeKind::Tree),
        tree,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let result = validated.apply_edit(
        &TreeEdit::DeleteNode { id: 7 },
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert!(matches!(result, Err(EditRejection::Apply(_))));

    let bad_table = TableEdit::Batch(vec![
        TableEdit::InsertRow {
            table: "item".into(),
            id: RowId(0),
            row: HashMap::new(),
        },
        TableEdit::InsertRow {
            table: "item".into(),
            id: RowId(0),
            row: HashMap::new(),
        },
    ]);
    assert!(<FInstance as ValidatedShape>::lower(&bad_table).is_err());
    Ok(())
}

#[test]
fn an_identity_edit_can_close_external_or_budget_incompleteness() -> TestResult {
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let external_validation_plan = external_plan(Requirement::Required);
    let (mut external_baseline, report) = ValidatedGraph::new(
        external_validation_plan,
        graph.clone(),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(report.status, CheckStatus::Incomplete);
    let mut verdicts = ExternalVerdicts::default();
    verdicts.insert(
        "audit".into(),
        ViewRef::Node(0),
        ExternalVerdict {
            passed: true,
            witness: Digest([7; 32]),
        },
    );
    verdicts.insert(
        "audit".into(),
        ViewRef::Node(99),
        ExternalVerdict {
            passed: false,
            witness: Digest([8; 32]),
        },
    );
    external_baseline.apply_edit(
        &GraphEdit::Identity,
        AdmissionPolicy::NoNewFailures,
        &ValidationBudget::default(),
        &verdicts,
    )?;
    assert_eq!(external_baseline.state().status, CheckStatus::Passed);
    assert!(
        external_baseline
            .state()
            .external
            .get(&"audit".into(), &ViewRef::Node(0))
            .is_some()
    );
    assert!(
        external_baseline
            .state()
            .external
            .get(&"audit".into(), &ViewRef::Node(99))
            .is_none()
    );

    let (mut budget_baseline, report) = ValidatedGraph::new(
        plan(InstanceShapeKind::Graph),
        graph,
        &ValidationBudget { max_reads: Some(0) },
        &ExternalVerdicts::default(),
    );
    assert_eq!(report.status, CheckStatus::Incomplete);
    budget_baseline.apply_edit(
        &GraphEdit::Identity,
        AdmissionPolicy::NoNewFailures,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(budget_baseline.state().status, CheckStatus::Passed);
    Ok(())
}

#[test]
fn a_failure_discovered_under_a_baseline_budget_cut_is_new() {
    let plan = plan(InstanceShapeKind::Graph);
    let graph = GInstance::new().with_node(Node::new(0, "ghost"));
    let (mut validated, report) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget { max_reads: Some(0) },
        &ExternalVerdicts::default(),
    );
    assert_eq!(report.status, CheckStatus::Incomplete);
    let result = validated.apply_edit(
        &GraphEdit::Identity,
        AdmissionPolicy::NoNewFailures,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert!(matches!(result, Err(EditRejection::NotAdmitted { .. })));
}

#[test]
fn new_advisory_outcomes_are_reported_but_do_not_reject() -> TestResult {
    let plan = external_plan(Requirement::Advisory);
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let mut verdicts = ExternalVerdicts::default();
    verdicts.insert(
        "audit".into(),
        ViewRef::Node(0),
        ExternalVerdict {
            passed: false,
            witness: Digest([9; 32]),
        },
    );
    let (mut validated, report) =
        ValidatedGraph::new(plan, graph, &ValidationBudget::default(), &verdicts);
    assert_eq!(report.status, CheckStatus::Passed);
    assert_eq!(report.diagnostics.len(), 1);
    validated.apply_edit(
        &GraphEdit::Identity,
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &verdicts,
    )?;
    Ok(())
}

#[test]
fn one_run_retains_every_finding_and_incomplete_reason() -> TestResult {
    let plan = mixed_table_plan();
    let (mut validated, baseline) = Validated::<FInstance>::new(
        plan,
        mixed_table(1),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(baseline.status, CheckStatus::Failed);
    let witness = validated.apply_edit(
        &TableEdit::UpdateCell {
            table: "record".into(),
            id: RowId(0),
            column: "extra".into(),
            old: Some(Value::Str("old".into())),
            new: Some(Value::Str("new".into())),
        },
        AdmissionPolicy::NoNewFailures,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    let run = witness
        .runs
        .iter()
        .find(|run| {
            run.instance.check.kind == CheckKind::RequiredValue
                && run.instance.subject == Some(ViewRef::Row(RowId(0)))
        })
        .expect("the touched primary key is rerun");
    let RunVerdict::Evaluated {
        findings,
        incomplete,
    } = &run.verdict
    else {
        panic!("the primary-key check was evaluated")
    };
    assert_eq!(findings.len(), 2);
    assert_eq!(incomplete.len(), 1);
    assert_eq!(
        findings
            .iter()
            .map(|finding| &finding.violation)
            .collect::<Vec<_>>(),
        validated
            .state()
            .open
            .get(&run.instance)
            .expect("the mixed outcome remains open")
            .diagnostics
            .iter()
            .map(|diagnostic| &diagnostic.violation)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        incomplete,
        &validated
            .state()
            .open
            .get(&run.instance)
            .expect("the mixed outcome remains open")
            .incomplete
            .iter()
            .map(|check| check.reason.clone())
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn a_batch_is_validated_on_its_net_delta() -> TestResult {
    let plan = plan(InstanceShapeKind::Graph);
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let (mut validated, _) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let ghost = Node::new(1, "ghost");
    let batch = GraphEdit::Batch(vec![
        GraphEdit::AddNode {
            node: ghost.clone(),
        },
        GraphEdit::RemoveNode {
            node: ghost,
            incident: Vec::new(),
        },
    ]);
    assert!(batch.to_delta()?.is_identity());
    validated.apply_edit(
        &batch,
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert!(validated.instance().node(1).is_none());
    Ok(())
}

#[test]
fn removing_digest_commitments_is_a_mismatch() -> TestResult {
    let plan = plan(InstanceShapeKind::Graph);
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let (mut validated, _) = ValidatedGraph::new(
        plan.clone(),
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let baseline = validated.state().clone();
    let prior = validated.instance().clone();
    let edit = GraphEdit::AddNode {
        node: Node::new(1, "item"),
    };
    let delta = edit.to_delta()?;
    let witness = validated.apply_edit_with_options(
        &edit,
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
        &WitnessOptions { digests: true },
    )?;
    let mut missing = witness.clone();
    missing.prior = None;
    assert!(
        missing
            .verify(&WitnessReplay {
                plan: &plan,
                baseline: &baseline,
                prior: &prior,
                delta: &delta,
                policy: AdmissionPolicy::RequirePassed,
                budget: &ValidationBudget::default(),
                external: &ExternalVerdicts::default(),
                expected_options: WitnessOptions { digests: true },
            })
            .is_err()
    );
    let mut stripped = witness.clone();
    stripped.options = WitnessOptions::default();
    stripped.prior = None;
    stripped.result = None;
    assert_eq!(
        stripped
            .verify(&WitnessReplay {
                plan: &plan,
                baseline: &baseline,
                prior: &prior,
                delta: &delta,
                policy: AdmissionPolicy::RequirePassed,
                budget: &ValidationBudget::default(),
                external: &ExternalVerdicts::default(),
                expected_options: WitnessOptions { digests: true },
            })
            .unwrap_err()
            .field,
        "options"
    );
    let mut changed = witness;
    changed.runs[0].reads.push(ViewRef::Node(u32::MAX));
    assert!(
        changed
            .verify(&WitnessReplay {
                plan: &plan,
                baseline: &baseline,
                prior: &prior,
                delta: &delta,
                policy: AdmissionPolicy::RequirePassed,
                budget: &ValidationBudget::default(),
                external: &ExternalVerdicts::default(),
                expected_options: WitnessOptions { digests: true },
            })
            .is_err()
    );
    Ok(())
}

fn external_witness_fixture() -> Result<ExternalWitnessFixture, Box<dyn std::error::Error>> {
    let plan = external_plan(Requirement::Advisory);
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let baseline_external = ExternalVerdicts::default();
    let (mut validated, _) = ValidatedGraph::new(
        plan.clone(),
        graph,
        &ValidationBudget::default(),
        &baseline_external,
    );
    let baseline = validated.state().clone();
    let prior = validated.instance().clone();
    let edit = GraphEdit::Identity;
    let delta = edit.to_delta()?;
    let mut rejected = ExternalVerdicts::default();
    rejected.insert(
        "audit".into(),
        ViewRef::Node(0),
        ExternalVerdict {
            passed: false,
            witness: Digest([11; 32]),
        },
    );
    let options = WitnessOptions { digests: true };
    let witness = validated.apply_edit_with_options(
        &edit,
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &rejected,
        &options,
    )?;
    assert_eq!(witness.runs.len(), 1);
    assert!(matches!(
        &witness.runs[0].verdict,
        RunVerdict::Evaluated { findings, incomplete }
            if findings.len() == 1 && incomplete.is_empty()
    ));
    Ok(ExternalWitnessFixture {
        plan,
        baseline,
        prior,
        delta,
        rejected,
        options,
        witness,
    })
}

#[test]
fn v6_rejects_run_collection_and_key_mutations() -> TestResult {
    let fixture = external_witness_fixture()?;
    assert!(fixture.verifies(&fixture.witness));

    let mut removed_run = fixture.witness.clone();
    removed_run.runs.clear();
    assert!(!fixture.verifies(&removed_run));
    let mut duplicated_run = fixture.witness.clone();
    duplicated_run.runs.push(duplicated_run.runs[0].clone());
    assert!(!fixture.verifies(&duplicated_run));
    let mut changed_read = fixture.witness.clone();
    changed_read.runs[0].reads.push(ViewRef::Node(9));
    assert!(!fixture.verifies(&changed_read));
    let mut removed_read = fixture.witness.clone();
    removed_read.runs[0].reads.clear();
    assert!(!fixture.verifies(&removed_read));
    let mut changed_run_key = fixture.witness.clone();
    changed_run_key.runs[0].instance.check =
        CheckKey::new(CheckKind::NodeAnchor, SchemaRef::Schema);
    assert!(!fixture.verifies(&changed_run_key));
    Ok(())
}

#[test]
fn v6_rejects_run_verdict_and_finding_mutations() -> TestResult {
    let fixture = external_witness_fixture()?;
    let mut changed_disposition = fixture.witness.clone();
    changed_disposition.runs[0].verdict = RunVerdict::Skipped {
        reason: IncompleteReason::Budget {
            limit: 0,
            spent: 0,
            from: Some(ViewRef::Node(0)),
        },
    };
    assert!(!fixture.verifies(&changed_disposition));
    let mut removed_finding = fixture.witness.clone();
    if let RunVerdict::Evaluated { findings, .. } = &mut removed_finding.runs[0].verdict {
        findings.clear();
    }
    assert!(!fixture.verifies(&removed_finding));
    let mut changed_elements = fixture.witness.clone();
    if let RunVerdict::Evaluated { findings, .. } = &mut changed_elements.runs[0].verdict {
        findings[0].elements.clear();
    }
    assert!(!fixture.verifies(&changed_elements));
    Ok(())
}

#[test]
fn v6_rejects_replay_external_budget_delta_and_prior_mutations() -> TestResult {
    let fixture = external_witness_fixture()?;
    let mut accepted = ExternalVerdicts::default();
    accepted.insert(
        "audit".into(),
        ViewRef::Node(0),
        ExternalVerdict {
            passed: true,
            witness: Digest([11; 32]),
        },
    );
    let verify = |prior: &GInstance,
                  delta: &GraphDelta,
                  budget: &ValidationBudget,
                  external: &ExternalVerdicts| {
        fixture
            .witness
            .verify(&WitnessReplay {
                plan: &fixture.plan,
                baseline: &fixture.baseline,
                prior,
                delta,
                policy: AdmissionPolicy::RequirePassed,
                budget,
                external,
                expected_options: fixture.options,
            })
            .is_ok()
    };
    assert!(!verify(
        &fixture.prior,
        &fixture.delta,
        &ValidationBudget::default(),
        &accepted
    ));
    assert!(!verify(
        &fixture.prior,
        &fixture.delta,
        &ValidationBudget { max_reads: Some(0) },
        &fixture.rejected
    ));
    let changed_delta = GraphEdit::AddNode {
        node: Node::new(1, "item"),
    }
    .to_delta()?;
    assert!(!verify(
        &fixture.prior,
        &changed_delta,
        &ValidationBudget::default(),
        &fixture.rejected,
    ));
    let changed_prior = fixture.prior.clone().with_node(Node::new(1, "item"));
    assert!(!verify(
        &changed_prior,
        &fixture.delta,
        &ValidationBudget::default(),
        &fixture.rejected,
    ));
    Ok(())
}

#[test]
fn v6_rejects_changed_policy() -> TestResult {
    let budget = ValidationBudget::default();
    let external = ExternalVerdicts::default();
    let options = WitnessOptions { digests: true };

    let failed_plan = plan(InstanceShapeKind::Graph);
    let failed = GInstance::new().with_node(Node::new(0, "ghost"));
    let (mut failed_validated, baseline_report) =
        ValidatedGraph::new(failed_plan.clone(), failed, &budget, &external);
    assert_eq!(baseline_report.status, CheckStatus::Failed);
    let failed_baseline = failed_validated.state().clone();
    let failed_prior = failed_validated.instance().clone();
    let identity = GraphEdit::Identity.to_delta()?;
    let policy_witness = failed_validated.apply_edit_with_options(
        &GraphEdit::Identity,
        AdmissionPolicy::NoNewFailures,
        &budget,
        &external,
        &options,
    )?;
    policy_witness.verify(&WitnessReplay {
        plan: &failed_plan,
        baseline: &failed_baseline,
        prior: &failed_prior,
        delta: &identity,
        policy: AdmissionPolicy::NoNewFailures,
        budget: &budget,
        external: &external,
        expected_options: options,
    })?;
    assert_eq!(
        policy_witness
            .verify(&WitnessReplay {
                plan: &failed_plan,
                baseline: &failed_baseline,
                prior: &failed_prior,
                delta: &identity,
                policy: AdmissionPolicy::RequirePassed,
                budget: &budget,
                external: &external,
                expected_options: options,
            })
            .unwrap_err()
            .field,
        "admission"
    );
    Ok(())
}

fn acyclic_witness_plan() -> (Arc<ValidationPlan>, Edge) {
    let next = Edge {
        src: "item".into(),
        tgt: "item".into(),
        kind: "prop".into(),
        name: Some("next".into()),
    };
    let mut schema = Schema::default();
    schema.vertices.insert(
        "item".into(),
        Vertex {
            id: "item".into(),
            kind: "object".into(),
            nsid: None,
        },
    );
    declare_edge(&mut schema, next.clone());
    schema.constraints.insert(
        "item".into(),
        vec![Constraint {
            sort: "acyclic".into(),
            value: "next".into(),
        }],
    );
    let plan = Arc::new(ValidationPlan::compile(
        &schema,
        &Protocol {
            constraint_sorts: vec!["acyclic".into()],
            constraint_semantics: BTreeMap::from([(
                "acyclic".into(),
                ConstraintSemantics::Acyclic,
            )]),
            syntactic_shape: InstanceShapeKind::Graph,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        },
        &ValidationOptions::default(),
    ));
    (plan, next)
}

#[test]
fn v6_rejects_removed_and_reordered_multi_element_reads() -> TestResult {
    let budget = ValidationBudget::default();
    let external = ExternalVerdicts::default();
    let options = WitnessOptions { digests: true };
    let (acyclic_plan, next) = acyclic_witness_plan();
    let chain = GInstance::new()
        .with_node(Node::new(0, "item"))
        .with_node(Node::new(1, "item"))
        .with_node(Node::new(2, "item"))
        .with_edge_id(EdgeId(0), 0, 1, next.clone());
    let (mut chain_validated, _) =
        ValidatedGraph::new(acyclic_plan.clone(), chain, &budget, &external);
    let chain_baseline = chain_validated.state().clone();
    let chain_prior = chain_validated.instance().clone();
    let chain_edit = GraphEdit::AddEdge {
        id: EdgeId(1),
        edge: GEdge::new(1, 2, next),
    };
    let chain_delta = chain_edit.to_delta()?;
    let chain_witness = chain_validated.apply_edit_with_options(
        &chain_edit,
        AdmissionPolicy::RequirePassed,
        &budget,
        &external,
        &options,
    )?;
    let run_index = chain_witness
        .runs
        .iter()
        .position(|run| run.instance.check.kind == CheckKind::Acyclic && run.reads.len() > 1)
        .ok_or("the acyclic witness has no multi-read run")?;
    chain_witness.verify(&WitnessReplay {
        plan: &acyclic_plan,
        baseline: &chain_baseline,
        prior: &chain_prior,
        delta: &chain_delta,
        policy: AdmissionPolicy::RequirePassed,
        budget: &budget,
        external: &external,
        expected_options: options,
    })?;

    let mut removed = chain_witness.clone();
    removed.runs[run_index].reads.remove(0);
    assert!(
        removed
            .verify(&WitnessReplay {
                plan: &acyclic_plan,
                baseline: &chain_baseline,
                prior: &chain_prior,
                delta: &chain_delta,
                policy: AdmissionPolicy::RequirePassed,
                budget: &budget,
                external: &external,
                expected_options: options,
            })
            .is_err()
    );
    let mut reordered = chain_witness;
    reordered.runs[run_index].reads.reverse();
    assert!(
        reordered
            .verify(&WitnessReplay {
                plan: &acyclic_plan,
                baseline: &chain_baseline,
                prior: &chain_prior,
                delta: &chain_delta,
                policy: AdmissionPolicy::RequirePassed,
                budget: &budget,
                external: &external,
                expected_options: options,
            })
            .is_err()
    );
    Ok(())
}

#[test]
fn validated_new_report_equals_full_validation() {
    let plan = plan(InstanceShapeKind::Graph);
    let graph = GInstance::new()
        .with_node(Node::new(0, "item"))
        .with_node(Node::new(1, "ghost"));
    let external = ExternalVerdicts::default();
    let (validated, initial) = ValidatedGraph::new(
        plan.clone(),
        graph.clone(),
        &ValidationBudget::default(),
        &external,
    );
    let full = plan.validate_graph(&graph, &ValidationBudget::default(), &external);
    assert_eq!(initial.canonical_bytes(), full.canonical_bytes());
    assert_eq!(validated.report().canonical_bytes(), full.canonical_bytes());
}

#[test]
fn validation_state_stores_complete_open_bundles() {
    let ab = Edge {
        src: "a".into(),
        tgt: "b".into(),
        kind: "prop".into(),
        name: Some("ab".into()),
    };
    let mut schema = Schema::default();
    for id in ["a", "b"] {
        schema.vertices.insert(
            id.into(),
            Vertex {
                id: id.into(),
                kind: "object".into(),
                nsid: None,
            },
        );
    }
    declare_edge(&mut schema, ab.clone());
    let plan = Arc::new(ValidationPlan::compile(
        &schema,
        &Protocol::default(),
        &ValidationOptions::default(),
    ));
    let graph = GInstance::new()
        .with_node(Node::new(0, "b"))
        .with_node(Node::new(1, "a"))
        .with_edge_id(EdgeId(0), 0, 1, ab);
    let (validated, report) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(report.diagnostics.len(), 2);
    let bundle = validated
        .state()
        .open
        .values()
        .find(|bundle| bundle.diagnostics.len() == 2)
        .expect("both endpoint failures remain in one check-instance bundle");
    assert!(bundle.incomplete.is_empty());
}

#[test]
fn validation_state_stores_budget_cuts_separately() {
    let plan = plan(InstanceShapeKind::Graph);
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let (validated, report) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget { max_reads: Some(0) },
        &ExternalVerdicts::default(),
    );
    assert!(!validated.state().budget_cuts.is_empty());
    assert!(validated.state().open.values().all(|bundle| {
        bundle
            .incomplete
            .iter()
            .all(|entry| !matches!(entry.reason, IncompleteReason::Budget { .. }))
    }));
    assert!(
        report
            .incomplete
            .iter()
            .any(|entry| { matches!(entry.reason, IncompleteReason::Budget { .. }) })
    );
}

#[test]
fn validation_state_projects_active_external_verdicts() {
    let plan = external_plan(Requirement::Required);
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let mut supplied = ExternalVerdicts::default();
    for (name, subject) in [
        ("audit", ViewRef::Node(0)),
        ("audit", ViewRef::Node(99)),
        ("unused", ViewRef::Node(0)),
    ] {
        supplied.insert(
            name.into(),
            subject,
            ExternalVerdict {
                passed: true,
                witness: Digest([1; 32]),
            },
        );
    }
    let (validated, _) = ValidatedGraph::new(plan, graph, &ValidationBudget::default(), &supplied);
    let mut expected = ExternalVerdicts::default();
    expected.insert(
        "audit".into(),
        ViewRef::Node(0),
        ExternalVerdict {
            passed: true,
            witness: Digest([1; 32]),
        },
    );
    assert_eq!(validated.state().external, expected);
}

#[test]
fn check_edit_leaves_state_and_instance_byte_identical() -> TestResult {
    let plan = two_anchor_plan();
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let (validated, _) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let before_instance = validated.instance().digest();
    let before_state = validated.state().canonical_bytes();
    validated.check_edit(
        &GraphEdit::RelabelNode {
            node: 0,
            old_anchor: "item".into(),
            new_anchor: "renamed".into(),
        },
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert!(
        validated
            .check_edit(
                &GraphEdit::RelabelNode {
                    node: 0,
                    old_anchor: "stale".into(),
                    new_anchor: "renamed".into(),
                },
                &ValidationBudget::default(),
                &ExternalVerdicts::default(),
            )
            .is_err()
    );
    assert_eq!(validated.instance().digest(), before_instance);
    assert_eq!(validated.state().canonical_bytes(), before_state);
    Ok(())
}

#[test]
fn check_edit_and_apply_edit_report_the_same_typed_apply_error() {
    let plan = two_anchor_plan();
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let (mut validated, _) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let edit = GraphEdit::RelabelNode {
        node: 0,
        old_anchor: "stale".into(),
        new_anchor: "renamed".into(),
    };
    let dry = validated
        .check_edit(
            &edit,
            &ValidationBudget::default(),
            &ExternalVerdicts::default(),
        )
        .unwrap_err();
    let EditRejection::Apply(applied) = validated
        .apply_edit(
            &edit,
            AdmissionPolicy::RequirePassed,
            &ValidationBudget::default(),
            &ExternalVerdicts::default(),
        )
        .unwrap_err()
    else {
        panic!("a stale relabel is an apply error");
    };
    assert_eq!(
        std::mem::discriminant::<EditError>(&dry),
        std::mem::discriminant(&applied)
    );
    assert_eq!(dry.to_string(), applied.to_string());
}

#[test]
fn admission_policy_defaults_to_require_passed() {
    assert_eq!(AdmissionPolicy::default(), AdmissionPolicy::RequirePassed);
}

#[test]
fn witness_digests_are_off_by_default() -> TestResult {
    let plan = plan(InstanceShapeKind::Graph);
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let (mut validated, _) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let witness = validated.apply_edit(
        &GraphEdit::Identity,
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(witness.options, WitnessOptions::default());
    assert!(witness.prior.is_none() && witness.result.is_none());
    Ok(())
}

#[test]
fn apply_edit_with_options_records_both_instance_digests() -> TestResult {
    let plan = plan(InstanceShapeKind::Graph);
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let prior = graph.digest();
    let (mut validated, _) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let witness = validated.apply_edit_with_options(
        &GraphEdit::Identity,
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
        &WitnessOptions { digests: true },
    )?;
    assert_eq!(witness.prior, Some(prior));
    assert_eq!(witness.result, Some(validated.instance().digest()));
    Ok(())
}

#[test]
fn witness_delta_digest_uses_the_shape_context_and_normal_form() -> TestResult {
    let plan = plan(InstanceShapeKind::Graph);
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let (mut validated, _) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let edit = GraphEdit::Batch(vec![GraphEdit::Identity, GraphEdit::Identity]);
    let delta = edit.to_delta()?;
    let witness = validated.apply_edit(
        &edit,
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert!(delta.is_identity());
    assert_eq!(witness.delta, delta.digest());
    Ok(())
}

#[test]
fn external_rerun_is_not_confused_with_an_edit_trigger() -> TestResult {
    let plan = external_plan(Requirement::Advisory);
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let (mut validated, _) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let mut target = ExternalVerdicts::default();
    target.insert(
        "audit".into(),
        ViewRef::Node(0),
        ExternalVerdict {
            passed: true,
            witness: Digest([4; 32]),
        },
    );
    let witness = validated.apply_edit(
        &GraphEdit::Identity,
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &target,
    )?;
    assert!(witness.not_triggered.contains(&CheckKind::External));
    assert!(
        witness
            .runs
            .iter()
            .any(|run| run.instance.check.kind == CheckKind::External)
    );
    Ok(())
}

#[test]
fn not_triggered_is_the_complement_of_the_met_trigger_rows() -> TestResult {
    let plan = plan(InstanceShapeKind::Graph);
    let compiled: BTreeSet<CheckKind> = plan.checks().keys().map(|key| key.kind).collect();
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let (mut validated, _) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let witness = validated.apply_edit(
        &GraphEdit::Identity,
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(witness.not_triggered, compiled);
    Ok(())
}

fn assert_v3_v4<I: ValidatedShape>(
    plan: &Arc<ValidationPlan>,
    pre: &I,
    edit: &I::Edit,
    baseline_budget: &ValidationBudget,
    baseline_external: &ExternalVerdicts,
    target_external: &ExternalVerdicts,
) -> TestResult {
    let (validated, _) = Validated::<I>::new(
        plan.clone(),
        pre.clone(),
        baseline_budget,
        baseline_external,
    );
    let footprint = validated.footprint(edit, target_external)?;
    let delta = I::lower(edit)?;
    let mut post = pre.clone();
    I::apply_delta(&delta, &mut post)?;
    let before = pre.check_instances(plan);
    let after = post.check_instances(plan);
    assert_eq!(
        footprint.created,
        after.difference(&before).cloned().collect()
    );
    assert_eq!(
        footprint.retired,
        before.difference(&after).cloned().collect()
    );
    for instance in before.intersection(&after) {
        if !footprint.rerun.contains(instance) {
            let (old, new) = (
                pre.evaluate(plan, baseline_external, instance),
                post.evaluate(plan, target_external, instance),
            );
            assert_eq!(
                rmp_serde::to_vec_named(&old)?,
                rmp_serde::to_vec_named(&new)?,
                "carried {instance:?} changed: before {old:?}, after {new:?}; touched {:?}",
                footprint.touched
            );
        }
    }
    let mut reads = BTreeSet::new();
    for instance in &footprint.rerun {
        reads.extend(pre.reads(plan, baseline_external, instance));
        reads.extend(post.reads(plan, target_external, instance));
    }
    for instance in &footprint.created {
        reads.extend(post.reads(plan, target_external, instance));
    }
    for instance in &footprint.retired {
        reads.extend(pre.reads(plan, baseline_external, instance));
    }
    assert_eq!(footprint.reads, reads);

    let checked = validated.check_edit(edit, &ValidationBudget::default(), target_external)?;
    let mut full = post.validate(plan, &ValidationBudget::default(), target_external);
    full.scope = ReportScope::Edit {
        baseline_seq: validated.state().seq,
        baseline_status: validated.state().status,
    };
    assert_eq!(checked.report.canonical_bytes(), full.canonical_bytes());
    Ok(())
}

#[test]
fn v3_endpoint_counterexample_has_the_independently_expected_footprint() -> TestResult {
    let ab = Edge {
        src: "a".into(),
        tgt: "b".into(),
        kind: "prop".into(),
        name: Some("ab".into()),
    };
    let mut schema = Schema::default();
    for anchor in ["a", "b"] {
        schema.vertices.insert(
            anchor.into(),
            Vertex {
                id: anchor.into(),
                kind: "object".into(),
                nsid: None,
            },
        );
    }
    declare_edge(&mut schema, ab.clone());
    let plan = Arc::new(ValidationPlan::compile(
        &schema,
        &Protocol {
            syntactic_shape: InstanceShapeKind::Graph,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        },
        &ValidationOptions::default(),
    ));
    let graph = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "b"))
        .with_edge_id(EdgeId(0), 0, 1, ab);
    let (validated, report) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(report.status, CheckStatus::Passed);
    let footprint = validated.footprint(
        &GraphEdit::RelabelNode {
            node: 1,
            old_anchor: "b".into(),
            new_anchor: "a".into(),
        },
        &ExternalVerdicts::default(),
    )?;
    let endpoint = CheckInstance {
        check: CheckKey::new(CheckKind::EndpointCoherence, SchemaRef::Schema),
        subject: Some(ViewRef::Edge(EdgeId(0))),
    };
    let anchor = CheckInstance {
        check: CheckKey::new(CheckKind::NodeAnchor, SchemaRef::Schema),
        subject: Some(ViewRef::Node(1)),
    };
    let attribute = |vertex: &str| CheckInstance {
        check: CheckKey::new(CheckKind::Attribute, SchemaRef::Vertex(vertex.into())),
        subject: Some(ViewRef::Node(1)),
    };
    assert_eq!(footprint.created, BTreeSet::from([attribute("a")]));
    assert_eq!(footprint.retired, BTreeSet::new());
    assert_eq!(footprint.rerun, BTreeSet::from([endpoint, anchor]));
    assert_eq!(
        footprint.reads,
        BTreeSet::from([ViewRef::Node(0), ViewRef::Node(1), ViewRef::Edge(EdgeId(0)),])
    );
    assert_eq!(footprint.touched, BTreeSet::from([ViewRef::Node(1)]));
    Ok(())
}

#[test]
fn v4_unknown_anchor_counterexample_has_the_independently_expected_report() -> TestResult {
    let plan = plan(InstanceShapeKind::Graph);
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    let (validated, baseline) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(baseline.status, CheckStatus::Passed);
    let checked = validated.check_edit(
        &GraphEdit::RelabelNode {
            node: 0,
            old_anchor: "item".into(),
            new_anchor: "ghost".into(),
        },
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(checked.report.status, CheckStatus::Failed);
    assert_eq!(
        checked.report.scope,
        ReportScope::Edit {
            baseline_seq: 0,
            baseline_status: CheckStatus::Passed,
        }
    );
    assert!(checked.report.incomplete.is_empty());
    assert_eq!(checked.report.diagnostics.len(), 1);
    let diagnostic = &checked.report.diagnostics[0];
    assert_eq!(diagnostic.check.kind, CheckKind::NodeAnchor);
    assert_eq!(diagnostic.subject, Some(ViewRef::Node(0)));
    assert_eq!(diagnostic.elements, vec![ViewRef::Node(0)]);
    assert_eq!(
        diagnostic.violation,
        Violation::UnknownAnchor {
            anchor: "ghost".into(),
        }
    );
    let anchor_coverage = checked
        .report
        .coverage
        .iter()
        .find(|(key, _)| key.kind == CheckKind::NodeAnchor)
        .map(|(_, coverage)| coverage)
        .ok_or("the structural plan has NodeAnchor coverage")?;
    assert_eq!(anchor_coverage.instances, 1);
    assert_eq!(anchor_coverage.failed, 1);
    assert_eq!(anchor_coverage.passed, 0);
    assert_eq!(anchor_coverage.incomplete, 0);
    assert!(checked.report.coverage.values().all(|coverage| {
        coverage.instances == coverage.passed + coverage.failed + coverage.incomplete
    }));
    Ok(())
}

// ---------------------------------------------------------------------------
// The footprint, trigger, edit, sequence and witness laws over the shared
// generators.
//
// Instances are drawn by the shared generators of `validation_support`
// (schemas with required edges, keys, references, degree bounds, an acyclic
// self-edge and fans, and trees, graphs and tables over them); edits are
// drawn as operations on a copy of the instance and normalized by the
// shape's `diff` (graphs, tables) or kept step by step while they apply
// (trees). External projections are drawn per element. Each law is also
// checked on every instance of an exhaustive bounded scope, enumerated
// below without calling the validator.
// ---------------------------------------------------------------------------

mod validation_support;

use validation_support::{
    Case, STRINGS, TREE_ANCHORS, arb_graph_spec, arb_table_spec, arb_tree_spec,
    arb_validation_schema, graph_of, protocol, table_of, tree_labels, tree_of,
};

/// A generated schema's plan in `shape`, with the application check `audit`
/// on items and films.
fn law_plan(case: &Case, shape: InstanceShapeKind) -> Arc<ValidationPlan> {
    let mut options = ValidationOptions::default();
    options.external.insert(
        "audit".into(),
        ExternalCheckDecl {
            name: "audit".into(),
            anchors: BTreeSet::from(["item".into(), "film".into()]),
            requirement: Requirement::Required,
        },
    );
    Arc::new(ValidationPlan::compile(
        &case.schema,
        &protocol(shape),
        &options,
    ))
}

/// An external projection: for each element id (node or row), by the id
/// modulo the choice list, no verdict (0), a passing one (1) or a rejecting
/// one (2), for both external names the generated plans use.
fn projection(choices: &[u8], elements: impl IntoIterator<Item = ViewRef>) -> ExternalVerdicts {
    let mut verdicts = ExternalVerdicts::default();
    if choices.is_empty() {
        return verdicts;
    }
    for element in elements {
        let (ViewRef::Node(id) | ViewRef::Row(RowId(id))) = element else {
            continue;
        };
        let choice = choices[usize::try_from(id).unwrap_or(0) % choices.len()];
        if choice == 0 {
            continue;
        }
        for name in ["audit", "external"] {
            verdicts.insert(
                name.into(),
                element.clone(),
                ExternalVerdict {
                    passed: choice == 1,
                    witness: Digest([u8::try_from(id % 256).unwrap_or(0); 32]),
                },
            );
        }
    }
    verdicts
}

fn graph_elements(graph: &GInstance) -> Vec<ViewRef> {
    graph.nodes().keys().map(|id| ViewRef::Node(*id)).collect()
}

fn tree_elements(tree: &WInstance) -> Vec<ViewRef> {
    tree.nodes.keys().map(|id| ViewRef::Node(*id)).collect()
}

fn table_elements(table: &FInstance) -> Vec<ViewRef> {
    table.row_ids().into_iter().map(ViewRef::Row).collect()
}

/// Every external verdict either projection could need, for the elements of
/// both the instance before and after an edit.
fn projections(
    before: &[u8],
    after: &[u8],
    elements: &[ViewRef],
) -> (ExternalVerdicts, ExternalVerdicts) {
    (
        projection(before, elements.iter().cloned()),
        projection(after, elements.iter().cloned()),
    )
}

/// A drawn value: absent, null, one of the generator's strings, or an
/// integer.
fn drawn_value(code: u8) -> Option<Value> {
    match code % 8 {
        0 => None,
        1 => Some(Value::Null),
        2..=5 => Some(Value::Str(STRINGS[usize::from(code % 8) - 2].into())),
        _ => Some(Value::Int(i64::from(code % 3))),
    }
}

/// The field names the generated schemas read as value sites.
const FIELDS: [&str; 6] = ["name", "num", "tag", "cast", "title", "x"];

/// The kind of element a drawn graph operation changes: one family per
/// graph `DeltaElementKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum GraphFamily {
    AddNode,
    DelNode,
    AddEdge,
    DelEdge,
    Value,
    Field,
    Meta,
    Anchor,
    EdgeLabel,
    EdgePosition,
}

const GRAPH_FAMILIES: [GraphFamily; 10] = [
    GraphFamily::AddNode,
    GraphFamily::DelNode,
    GraphFamily::AddEdge,
    GraphFamily::DelEdge,
    GraphFamily::Value,
    GraphFamily::Field,
    GraphFamily::Meta,
    GraphFamily::Anchor,
    GraphFamily::EdgeLabel,
    GraphFamily::EdgePosition,
];

/// One drawn graph operation: its family and three choices it reads modulo
/// what the graph holds.
#[derive(Clone, Copy, Debug)]
struct GraphOp {
    family: GraphFamily,
    a: usize,
    b: usize,
    c: u8,
}

fn arb_graph_op(families: &'static [GraphFamily]) -> impl Strategy<Value = GraphOp> {
    (
        proptest::sample::select(families),
        any::<usize>(),
        any::<usize>(),
        any::<u8>(),
    )
        .prop_map(|(family, a, b, c)| GraphOp { family, a, b, c })
}

/// Apply `op` to `graph` where it applies, as a direct change of the
/// representation (not through the edit algebra under test).
fn apply_graph_op(graph: &mut GInstance, op: GraphOp) {
    let nodes: Vec<u32> = {
        let mut ids: Vec<u32> = graph.nodes().keys().copied().collect();
        ids.sort_unstable();
        ids
    };
    let edges: Vec<EdgeId> = graph.edges_sorted().map(|(id, _)| id).collect();
    let node = |i: usize| nodes.get(i % nodes.len().max(1)).copied();
    let edge = |i: usize| edges.get(i % edges.len().max(1)).copied();
    let labels = tree_labels();
    match op.family {
        GraphFamily::AddNode => {
            let id = nodes.last().map_or(0, |last| last + 1);
            let mut fresh = Node::new(id, TREE_ANCHORS[op.a % TREE_ANCHORS.len()]);
            if let Some(v) = drawn_value(op.c) {
                fresh = fresh.with_value(FieldPresence::Present(v));
            }
            let _ = graph.insert_node(fresh);
        }
        GraphFamily::DelNode => {
            if let Some(n) = node(op.a) {
                while let Some(&e) = graph
                    .out_edges(n)
                    .first()
                    .or_else(|| graph.in_edges(n).first())
                {
                    if graph.remove_edge(e).is_err() {
                        break;
                    }
                }
                let _ = graph.remove_node(n);
            }
        }
        GraphFamily::AddEdge => {
            if let (Some(s), Some(t)) = (node(op.a), node(op.b)) {
                if let Ok(id) =
                    graph.add_edge(s, t, labels[usize::from(op.c) % labels.len()].clone())
                {
                    let _ =
                        graph.set_edge_position(id, (op.c % 3 == 0).then_some(u32::from(op.c % 2)));
                }
            }
        }
        GraphFamily::DelEdge => {
            if let Some(e) = edge(op.a) {
                let _ = graph.remove_edge(e);
            }
        }
        GraphFamily::Value => {
            if let Some(n) = node(op.a)
                && let Some(record) = graph.node_mut(n)
            {
                record.value = drawn_value(op.c).map(FieldPresence::Present);
            }
        }
        GraphFamily::Field => {
            if let Some(n) = node(op.a)
                && let Some(record) = graph.node_mut(n)
            {
                let field = FIELDS[op.b % FIELDS.len()].to_owned();
                match drawn_value(op.c) {
                    Some(v) => {
                        record.extra_fields.insert(field, v);
                    }
                    None => {
                        record.extra_fields.remove(&field);
                    }
                }
            }
        }
        GraphFamily::Meta => {
            if let Some(n) = node(op.a)
                && let Some(record) = graph.node_mut(n)
            {
                record.shape = if record.shape == NodeShape::HyperEdge {
                    NodeShape::default()
                } else {
                    NodeShape::HyperEdge
                };
            }
        }
        GraphFamily::Anchor => {
            if let Some(n) = node(op.a)
                && let Some(record) = graph.node_mut(n)
            {
                record.anchor = TREE_ANCHORS[op.b % TREE_ANCHORS.len()].into();
            }
        }
        GraphFamily::EdgeLabel => {
            if let Some(e) = edge(op.a) {
                let _ = graph.relabel_edge(e, labels[op.b % labels.len()].clone());
            }
        }
        GraphFamily::EdgePosition => {
            if let Some(e) = edge(op.a) {
                let _ = graph.set_edge_position(e, (op.c % 3 != 0).then_some(u32::from(op.c % 3)));
            }
        }
    }
}

/// The normalized delta from `pre` to `pre` with `ops` applied.
fn graph_delta_of(pre: &GInstance, ops: &[GraphOp]) -> GraphDelta {
    let mut post = pre.clone();
    for op in ops {
        apply_graph_op(&mut post, *op);
    }
    GraphDelta::diff(pre, &post)
}

/// A generated graph: the graph form of a generated tree with extra edges,
/// positions and stray fan vocabulary.
fn arb_ginstance() -> impl Strategy<Value = GInstance> {
    arb_graph_spec(8).prop_map(|spec| graph_of(&spec))
}

/// A generated graph and a normalized delta of up to four operations of the
/// given families. The delta need not apply (a diff that leaves the fan
/// domain is refused by apply).
fn arb_graph_delta(
    families: &'static [GraphFamily],
) -> impl Strategy<Value = (GInstance, GraphDelta)> {
    (
        arb_ginstance(),
        proptest::collection::vec(arb_graph_op(families), 1..5),
    )
        .prop_map(|(graph, ops)| {
            let delta = graph_delta_of(&graph, &ops);
            (graph, delta)
        })
}

/// The kind of element a drawn table operation changes: one family per
/// table `DeltaElementKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TableFamily {
    AddRow,
    DelRow,
    AddPair,
    DelPair,
    Cell,
    Table,
}

const TABLE_FAMILIES: [TableFamily; 6] = [
    TableFamily::AddRow,
    TableFamily::DelRow,
    TableFamily::AddPair,
    TableFamily::DelPair,
    TableFamily::Cell,
    TableFamily::Table,
];

#[derive(Clone, Copy, Debug)]
struct TableOp {
    family: TableFamily,
    a: usize,
    b: usize,
    c: u8,
}

fn arb_table_op(families: &'static [TableFamily]) -> impl Strategy<Value = TableOp> {
    (
        proptest::sample::select(families),
        any::<usize>(),
        any::<usize>(),
        any::<u8>(),
    )
        .prop_map(|(family, a, b, c)| TableOp { family, a, b, c })
}

/// The tables and foreign-key labels table operations draw from.
const TABLES: [&str; 3] = ["film", "actor", "ghost_t"];

fn pair_labels() -> Vec<Edge> {
    vec![
        validation_support::e_stars(),
        validation_support::e_cast(),
        validation_support::e_ghost_fk(),
        validation_support::edge("actor", "actor", "fk", "knows"),
    ]
}

/// The columns the generated table schemas read.
const COLUMNS: [&str; 7] = ["title", "year", "lead", "aname", "cast", "note", "x"];

fn apply_table_op(table: &mut FInstance, op: TableOp) {
    let rows = table.row_ids();
    let row = |i: usize| rows.get(i % rows.len().max(1)).copied();
    match op.family {
        TableFamily::AddRow => {
            let id = rows.last().map_or(RowId(0), |last| RowId(last.0 + 1));
            let mut cells = HashMap::new();
            if let Some(v) = drawn_value(op.c) {
                cells.insert(COLUMNS[op.b % COLUMNS.len()].to_owned(), v);
            }
            let _ = table.insert_row(TABLES[op.a % TABLES.len()], id, cells);
        }
        TableFamily::DelRow => {
            if let Some(r) = row(op.a) {
                let _ = table.remove_row(r);
            }
        }
        TableFamily::AddPair => {
            if let (Some(s), Some(t)) = (row(op.a), row(op.b)) {
                let labels = pair_labels();
                let label = labels[usize::from(op.c) % labels.len()].clone();
                if !table.contains_pair(&label, s, t) {
                    let _ = table.insert_foreign_key(label, s, t);
                }
            }
        }
        TableFamily::DelPair => {
            let pairs = table.all_pairs();
            if let Some((edge, src, tgt)) = pairs.get(op.a % pairs.len().max(1)) {
                let _ = table.remove_foreign_key(edge, *src, *tgt);
            }
        }
        TableFamily::Cell => {
            if let Some(r) = row(op.a)
                && let Some(cells) = table.row_mut(r)
            {
                let column = COLUMNS[op.b % COLUMNS.len()].to_owned();
                match drawn_value(op.c) {
                    Some(v) => {
                        cells.insert(column, v);
                    }
                    None => {
                        cells.remove(&column);
                    }
                }
            }
        }
        TableFamily::Table => {
            if let Some(r) = row(op.a) {
                let _ = table.move_row(r, TABLES[op.b % TABLES.len()]);
            }
        }
    }
}

fn arb_finstance() -> impl Strategy<Value = FInstance> {
    arb_table_spec().prop_map(|spec| table_of(&spec))
}

/// A generated table instance and a normalized delta of up to four
/// operations of the given families.
fn arb_table_delta(
    families: &'static [TableFamily],
) -> impl Strategy<Value = (FInstance, TableDelta)> {
    (
        arb_finstance(),
        proptest::collection::vec(arb_table_op(families), 1..5),
    )
        .prop_map(|(table, ops)| {
            let mut post = table.clone();
            for op in ops {
                apply_table_op(&mut post, op);
            }
            let delta = TableDelta::diff(&table, &post);
            (table, delta)
        })
}

/// The kind of tree edit a drawn tree step is: one family per `TreeEdit`
/// step variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TreeFamily {
    InsertNode,
    DeleteNode,
    ContractNode,
    RelabelNode,
    SetField,
    RemoveField,
    MoveSubtree,
    InsertFan,
    DeleteFan,
    JoinFeatures,
}

const TREE_FAMILIES: [TreeFamily; 10] = [
    TreeFamily::InsertNode,
    TreeFamily::DeleteNode,
    TreeFamily::ContractNode,
    TreeFamily::RelabelNode,
    TreeFamily::SetField,
    TreeFamily::RemoveField,
    TreeFamily::MoveSubtree,
    TreeFamily::InsertFan,
    TreeFamily::DeleteFan,
    TreeFamily::JoinFeatures,
];

#[derive(Clone, Copy, Debug)]
struct TreeOp {
    family: TreeFamily,
    a: usize,
    b: usize,
    c: u8,
}

fn arb_tree_op(families: &'static [TreeFamily]) -> impl Strategy<Value = TreeOp> {
    (
        proptest::sample::select(families),
        any::<usize>(),
        any::<usize>(),
        any::<u8>(),
    )
        .prop_map(|(family, a, b, c)| TreeOp { family, a, b, c })
}

/// The tree step `op` draws against `tree`.
fn tree_step(tree: &WInstance, op: TreeOp) -> Option<TreeEdit> {
    let mut ids: Vec<u32> = tree.nodes.keys().copied().collect();
    ids.sort_unstable();
    let pick = |i: usize| ids.get(i % ids.len().max(1)).copied();
    let labels = tree_labels();
    let label = |i: usize| labels[i % labels.len()].clone();
    let anchor = |i: usize| -> panproto_gat::Name { TREE_ANCHORS[i % TREE_ANCHORS.len()].into() };
    Some(match op.family {
        TreeFamily::InsertNode => {
            let child_id = ids.last().map_or(0, |last| last + 1);
            let mut node = Node::new(child_id, anchor(op.b));
            if let Some(v) = drawn_value(op.c) {
                node = node.with_value(FieldPresence::Present(v));
            }
            TreeEdit::InsertNode {
                parent: pick(op.a)?,
                child_id,
                node,
                edge: label(usize::from(op.c)),
            }
        }
        TreeFamily::DeleteNode => TreeEdit::DeleteNode { id: pick(op.a)? },
        TreeFamily::ContractNode => TreeEdit::ContractNode { id: pick(op.a)? },
        TreeFamily::RelabelNode => TreeEdit::RelabelNode {
            id: pick(op.a)?,
            new_anchor: anchor(op.b),
        },
        TreeFamily::SetField => TreeEdit::SetField {
            node_id: pick(op.a)?,
            field: FIELDS[op.b % FIELDS.len()].into(),
            value: drawn_value(op.c).unwrap_or(Value::Null),
        },
        TreeFamily::RemoveField => TreeEdit::RemoveField {
            node_id: pick(op.a)?,
            field: FIELDS[op.b % FIELDS.len()].into(),
        },
        TreeFamily::MoveSubtree => {
            let node_id = pick(op.a)?;
            // One draw in three moves a node to the end of its own parent's
            // children along its own label: a pure reorder.
            let own = tree
                .arcs
                .iter()
                .find(|(_, child, _)| *child == node_id)
                .filter(|_| op.c % 3 == 0);
            match own {
                Some((parent, _, edge)) => TreeEdit::MoveSubtree {
                    node_id,
                    new_parent: *parent,
                    edge: edge.clone(),
                },
                None => TreeEdit::MoveSubtree {
                    node_id,
                    new_parent: pick(op.b)?,
                    edge: label(usize::from(op.c)),
                },
            }
        }
        TreeFamily::InsertFan => {
            let mut fan =
                panproto_inst::Fan::new(if op.c % 4 == 0 { "nope" } else { "rel" }, pick(op.a)?)
                    .with_child("a", pick(op.b)?);
            if op.c % 3 == 0 {
                fan = fan.with_child("b", pick(op.b.wrapping_add(1))?);
            }
            TreeEdit::InsertFan { fan }
        }
        TreeFamily::DeleteFan => TreeEdit::DeleteFan {
            hyper_edge_id: if op.c % 2 == 0 { "rel" } else { "nope" }.into(),
        },
        TreeFamily::JoinFeatures => {
            let primary = pick(op.a)?;
            let joined = pick(op.b)?;
            let produce = tree.nodes.get(&primary)?.clone();
            TreeEdit::JoinFeatures {
                primary,
                joined: vec![joined],
                produce,
            }
        }
    })
}

/// The steps of `ops` that apply in turn, starting from `tree`, as one
/// flattened tree edit.
fn tree_edit_of(tree: &WInstance, ops: &[TreeOp]) -> TreeEdit {
    let mut current = tree.clone();
    let mut steps = Vec::new();
    for op in ops {
        let Some(step) = tree_step(&current, *op) else {
            continue;
        };
        let mut next = current.clone();
        if step.apply(&mut next).is_ok() {
            current = next;
            steps.push(step);
        }
    }
    match steps.len() {
        0 => TreeEdit::Identity,
        1 => steps.remove(0),
        _ => TreeEdit::Sequence(steps),
    }
}

fn arb_winstance() -> impl Strategy<Value = WInstance> {
    arb_tree_spec(10).prop_map(|spec| tree_of(&spec))
}

/// A generated tree and a tree edit of up to four applicable steps of the
/// given families.
fn arb_tree_edit(families: &'static [TreeFamily]) -> impl Strategy<Value = (WInstance, TreeEdit)> {
    (
        arb_winstance(),
        proptest::collection::vec(arb_tree_op(families), 1..5),
    )
        .prop_map(|(tree, ops)| {
            let edit = tree_edit_of(&tree, &ops);
            (tree, edit)
        })
}

/// The surface edit that lowers to a canonical transition, so that a law
/// can apply through `apply_edit` exactly the delta a witness replays.
trait Surface: ValidatedShape {
    fn surface(delta: &Self::Delta) -> Self::Edit;
    fn elements(&self) -> Vec<ViewRef>;
}

impl Surface for GInstance {
    fn surface(delta: &GraphDelta) -> GraphEdit {
        GraphEdit::Delta(delta.clone())
    }
    fn elements(&self) -> Vec<ViewRef> {
        graph_elements(self)
    }
}

impl Surface for FInstance {
    fn surface(delta: &TableDelta) -> TableEdit {
        TableEdit::Delta(delta.clone())
    }
    fn elements(&self) -> Vec<ViewRef> {
        table_elements(self)
    }
}

impl Surface for WInstance {
    fn surface(delta: &TreeEdit) -> TreeEdit {
        delta.clone()
    }
    fn elements(&self) -> Vec<ViewRef> {
        tree_elements(self)
    }
}

/// Footprint soundness and edit agreement for one generated transition, or, when the edit does not
/// apply, its typed refusal by every surface with nothing changed.
fn v3_v4_case<I: Surface>(
    plan: &Arc<ValidationPlan>,
    pre: &I,
    delta: &I::Delta,
    baseline_budget: &ValidationBudget,
    before: &ExternalVerdicts,
    after: &ExternalVerdicts,
) -> TestResult {
    let mut post = pre.clone();
    if I::apply_delta(delta, &mut post).is_err() {
        let (validated, _) =
            Validated::<I>::new(plan.clone(), pre.clone(), baseline_budget, before);
        let edit = I::surface(delta);
        assert!(validated.footprint(&edit, after).is_err());
        assert!(
            validated
                .check_edit(&edit, &ValidationBudget::default(), after)
                .is_err()
        );
        return Ok(());
    }
    assert_v3_v4(
        plan,
        pre,
        &I::surface(delta),
        baseline_budget,
        before,
        after,
    )
}

/// Sequence agreement for one step: apply `delta` under `policy`, an unlimited budget
/// and `external`. An admitted step leaves a state whose report is full
/// validation of the new instance and which reattaches; a rejected one
/// leaves the canonical instance and state bytes unchanged. Returns the
/// baseline, the prior instance and the witness of an admitted step.
fn v5_step<I: Surface>(
    validated: &mut Validated<I>,
    plan: &Arc<ValidationPlan>,
    delta: &I::Delta,
    policy: AdmissionPolicy,
    external: &ExternalVerdicts,
    options: WitnessOptions,
) -> Result<Option<(ValidationState, I, EditWitness)>, TestCaseError> {
    let baseline = validated.state().clone();
    let prior = validated.instance().clone();
    let state_bytes = baseline.canonical_bytes();
    let instance_digest = prior.instance_digest();
    match validated.apply_edit_with_options(
        &I::surface(delta),
        policy,
        &ValidationBudget::default(),
        external,
        &options,
    ) {
        Ok(witness) => {
            let full = validated
                .instance()
                .validate(plan, &ValidationBudget::default(), external);
            prop_assert_eq!(validated.report().canonical_bytes(), full.canonical_bytes());
            prop_assert_eq!(validated.state().seq, baseline.seq + 1);
            let reattached = Validated::<I>::from_parts(
                plan.clone(),
                validated.instance().clone(),
                validated.state().clone(),
            );
            prop_assert!(reattached.is_ok(), "{:?}", reattached.err());
            Ok(Some((baseline, prior, witness)))
        }
        Err(rejection) => {
            prop_assert_eq!(
                validated.state().canonical_bytes(),
                state_bytes,
                "{:?}",
                rejection
            );
            prop_assert_eq!(validated.instance().instance_digest(), instance_digest);
            // A target whose status is `Passed` has no required failed or
            // incomplete outcome, so either policy admits it.
            if let EditRejection::NotAdmitted { validation } = &rejection {
                prop_assert_ne!(validation.report.status, CheckStatus::Passed);
            }
            Ok(None)
        }
    }
}

/// The canonical bytes of the witness admission would produce from the
/// replay context's baseline, paired with its prior instance, for its delta
/// under its policy, budget, external projection and expected options, or
/// `None` when the baseline does not reattach or the edit is refused.
fn replayed<I: Surface>(
    plan: &Arc<ValidationPlan>,
    replay: &WitnessReplay<'_, I>,
) -> Option<Vec<u8>> {
    let mut validated =
        Validated::<I>::from_parts(plan.clone(), replay.prior.clone(), replay.baseline.clone())
            .ok()?;
    validated
        .apply_edit_with_options(
            &I::surface(replay.delta),
            replay.policy,
            replay.budget,
            replay.external,
            &replay.expected_options,
        )
        .ok()
        .map(|witness| witness.canonical_bytes())
}

/// Every alteration of a witness this law tries: each scalar, the run list
/// (a run dropped, repeated or reordered), each of the first runs' reads,
/// disposition and outcome entries, `not_triggered`, the embedded options
/// and both instance digests.
fn witness_mutants(witness: &EditWitness) -> Vec<(String, EditWitness)> {
    let mut out = Vec::new();
    let mut push = |what: &str, w: EditWitness| out.push((what.to_owned(), w));
    let mut w = witness.clone();
    w.plan.0[0] ^= 1;
    push("plan", w);
    let mut w = witness.clone();
    w.baseline_seq = w.baseline_seq.wrapping_add(1);
    push("baseline_seq", w);
    for status in [
        CheckStatus::Passed,
        CheckStatus::Failed,
        CheckStatus::Incomplete,
    ] {
        let mut w = witness.clone();
        w.baseline_status = status;
        push("baseline_status", w);
        let mut w = witness.clone();
        w.result_status = status;
        push("result_status", w);
    }
    let mut w = witness.clone();
    w.delta.0[0] ^= 1;
    push("delta", w);
    if !witness.runs.is_empty() {
        let mut w = witness.clone();
        w.runs.remove(0);
        push("runs dropped", w);
        let mut w = witness.clone();
        w.runs.push(w.runs[0].clone());
        push("runs repeated", w);
    }
    if witness.runs.len() > 1 {
        let mut w = witness.clone();
        w.runs.swap(0, 1);
        push("runs reordered", w);
    }
    let mut w = witness.clone();
    if !w.not_triggered.insert(CheckKind::External) {
        w.not_triggered.remove(&CheckKind::External);
    }
    push("not_triggered", w);
    let mut w = witness.clone();
    w.options.digests = !w.options.digests;
    push("options", w);
    let mut w = witness.clone();
    w.prior = Some(w.prior.map_or(Digest([0; 32]), |mut d| {
        d.0[0] ^= 1;
        d
    }));
    push("prior", w);
    let mut w = witness.clone();
    w.result = if w.result.is_some() {
        None
    } else {
        Some(Digest([0; 32]))
    };
    push("result", w);
    out.extend(run_mutants(witness));
    out
}

/// The alterations of each of a witness's first three runs: its reads
/// extended and shortened, its disposition flipped, and its outcome entries
/// extended, repeated and dropped.
fn run_mutants(witness: &EditWitness) -> Vec<(String, EditWitness)> {
    let mut out = Vec::new();
    let mut push = |what: &str, w: EditWitness| out.push((what.to_owned(), w));
    for i in 0..witness.runs.len().min(3) {
        let mut w = witness.clone();
        w.runs[i].reads.push(ViewRef::Node(u32::MAX));
        push("reads extended", w);
        if !witness.runs[i].reads.is_empty() {
            let mut w = witness.clone();
            w.runs[i].reads.remove(0);
            push("reads dropped", w);
        }
        let mut w = witness.clone();
        w.runs[i].verdict = match &witness.runs[i].verdict {
            RunVerdict::Evaluated { .. } => RunVerdict::Skipped {
                reason: IncompleteReason::NoValidatedBaseline,
            },
            RunVerdict::Skipped { .. } => RunVerdict::Evaluated {
                findings: Vec::new(),
                incomplete: Vec::new(),
            },
        };
        push("disposition", w);
        if let RunVerdict::Evaluated {
            findings,
            incomplete,
        } = &witness.runs[i].verdict
        {
            let mut w = witness.clone();
            w.runs[i].verdict = RunVerdict::Evaluated {
                findings: findings.clone(),
                incomplete: incomplete
                    .iter()
                    .cloned()
                    .chain([IncompleteReason::NoValidatedBaseline])
                    .collect(),
            };
            push("incomplete extended", w);
            if let Some(first) = findings.first() {
                let mut w = witness.clone();
                w.runs[i].verdict = RunVerdict::Evaluated {
                    findings: findings.iter().cloned().chain([first.clone()]).collect(),
                    incomplete: incomplete.clone(),
                };
                push("finding repeated", w);
                let mut w = witness.clone();
                w.runs[i].verdict = RunVerdict::Evaluated {
                    findings: findings[1..].to_vec(),
                    incomplete: incomplete.clone(),
                };
                push("finding dropped", w);
            }
        }
    }
    out
}

/// Witness verification for one admitted transition: the witness verifies against its
/// exact replay context; an altered witness verifies iff it has the
/// witness's canonical bytes; and under an altered replay context (policy,
/// budget, external projection, expected options or baseline) it verifies
/// iff admission under that context produces a witness of the same bytes,
/// so that only observationally equivalent changes are accepted.
fn v6_case<I: Surface>(
    plan: &Arc<ValidationPlan>,
    replay: &WitnessReplay<'_, I>,
    witness: &EditWitness,
) -> Result<(), TestCaseError> {
    prop_assert!(
        witness.verify(replay).is_ok(),
        "{:?}",
        witness.verify(replay)
    );
    let bytes = witness.canonical_bytes();
    prop_assert_eq!(replayed(plan, replay), Some(bytes.clone()));
    for (what, mutant) in witness_mutants(witness) {
        let accepted = mutant.verify(replay).is_ok();
        prop_assert_eq!(accepted, mutant.canonical_bytes() == bytes, "{}", what);
    }
    let other_policy = match replay.policy {
        AdmissionPolicy::RequirePassed => AdmissionPolicy::NoNewFailures,
        AdmissionPolicy::NoNewFailures => AdmissionPolicy::RequirePassed,
    };
    let budgets = [
        ValidationBudget::default(),
        ValidationBudget { max_reads: Some(0) },
        ValidationBudget { max_reads: Some(3) },
    ];
    let mut externals = vec![ExternalVerdicts::default()];
    let mut flipped = replay.external.clone();
    for element in replay.prior.elements() {
        flipped.insert(
            "audit".into(),
            element,
            ExternalVerdict {
                passed: false,
                witness: Digest([1; 32]),
            },
        );
    }
    externals.push(flipped);
    let mut later = replay.baseline.clone();
    later.seq = later.seq.wrapping_add(1);
    let mut contexts: Vec<(&str, WitnessReplay<'_, I>)> = vec![(
        "policy",
        WitnessReplay {
            policy: other_policy,
            ..*replay
        },
    )];
    for budget in &budgets {
        contexts.push(("budget", WitnessReplay { budget, ..*replay }));
    }
    for external in &externals {
        contexts.push((
            "external",
            WitnessReplay {
                external,
                ..*replay
            },
        ));
    }
    contexts.push((
        "expected options",
        WitnessReplay {
            expected_options: WitnessOptions {
                digests: !replay.expected_options.digests,
            },
            ..*replay
        },
    ));
    contexts.push((
        "baseline",
        WitnessReplay {
            baseline: &later,
            ..*replay
        },
    ));
    for (what, context) in &contexts {
        let accepted = witness.verify(context).is_ok();
        prop_assert_eq!(
            accepted,
            replayed(plan, context).as_ref() == Some(&bytes),
            "{}",
            what
        );
    }
    Ok(())
}

fn arb_choices() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(0_u8..3, 0..4)
}

fn arb_baseline_budget() -> impl Strategy<Value = ValidationBudget> {
    proptest::option::of(0_u64..60).prop_map(|max_reads| ValidationBudget { max_reads })
}

/// One generated transition in each shape over one generated schema, with
/// a baseline budget and the baseline and target projections.
#[derive(Clone, Debug)]
struct Draw {
    case: Case,
    graph: (GInstance, GraphDelta),
    table: (FInstance, TableDelta),
    tree: (WInstance, TreeEdit),
    baseline_budget: ValidationBudget,
    before: Vec<u8>,
    after: Vec<u8>,
}

fn arb_draw() -> impl Strategy<Value = Draw> {
    (
        arb_validation_schema(),
        arb_graph_delta(&GRAPH_FAMILIES),
        arb_table_delta(&TABLE_FAMILIES),
        arb_tree_edit(&TREE_FAMILIES),
        arb_baseline_budget(),
        arb_choices(),
        arb_choices(),
    )
        .prop_map(
            |(case, graph, table, tree, baseline_budget, before, after)| Draw {
                case,
                graph,
                table,
                tree,
                baseline_budget,
                before,
                after,
            },
        )
}

/// Footprint soundness and edit agreement over the three transitions of
/// one draw.
fn v3_v4_draw(draw: &Draw) -> TestResult {
    let (pre, delta) = &draw.graph;
    let mut elements = graph_elements(pre);
    let mut post = pre.clone();
    if delta.apply(&mut post).is_ok() {
        elements.extend(graph_elements(&post));
    }
    let (before, after) = projections(&draw.before, &draw.after, &elements);
    let plan = law_plan(&draw.case, InstanceShapeKind::Graph);
    v3_v4_case(&plan, pre, delta, &draw.baseline_budget, &before, &after)?;

    let (pre, delta) = &draw.table;
    let mut elements = table_elements(pre);
    let mut post = pre.clone();
    if delta.apply(&mut post).is_ok() {
        elements.extend(table_elements(&post));
    }
    let (before, after) = projections(&draw.before, &draw.after, &elements);
    let plan = law_plan(&draw.case, InstanceShapeKind::Table);
    v3_v4_case(&plan, pre, delta, &draw.baseline_budget, &before, &after)?;

    let (pre, edit) = &draw.tree;
    let mut elements = tree_elements(pre);
    let mut post = pre.clone();
    if edit.apply(&mut post).is_ok() {
        elements.extend(tree_elements(&post));
    }
    let (before, after) = projections(&draw.before, &draw.after, &elements);
    let plan = law_plan(&draw.case, InstanceShapeKind::Tree);
    v3_v4_case(&plan, pre, edit, &draw.baseline_budget, &before, &after)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(160))]

    /// Footprint soundness over generated schemas, every shape, every
    /// element kind, replacements, path keys, `Within` scopes, moved
    /// subtrees, external changes and finite-budget baselines.
    #[test]
    fn v3_footprint_soundness(draw in arb_draw()) {
        v3_v4_draw(&draw).map_err(|error| TestCaseError::fail(error.to_string()))?;
    }

    /// Edit agreement with a finite-budget baseline and target external
    /// evidence that differs from the baseline's.
    #[test]
    fn v4_edit_agreement(
        draw in arb_draw(),
        limit in 0_u64..20,
        after in proptest::collection::vec(0_u8..3, 1..4),
    ) {
        let mut draw = draw;
        draw.baseline_budget = ValidationBudget { max_reads: Some(limit) };
        if draw.after == draw.before {
            draw.after = after;
        }
        v3_v4_draw(&draw).map_err(|error| TestCaseError::fail(error.to_string()))?;
    }
}

/// Run `check` on `cases` draws of `strategy`, failing with the first
/// failing draw.
fn run_cases<S: Strategy>(cases: u32, strategy: &S, check: impl Fn(S::Value) -> TestResult)
where
    S::Value: std::fmt::Debug,
{
    let mut runner = proptest::test_runner::TestRunner::new(ProptestConfig::with_cases(cases));
    let result = runner.run(strategy, |value| {
        check(value).map_err(|error| TestCaseError::fail(error.to_string()))
    });
    if let Err(error) = result {
        panic!("{}", error.to_string().lines().next().unwrap_or_default());
    }
}

/// Trigger completeness: the footprint soundness oracle over generator families that
/// change one element kind only, for every graph `DeltaElementKind`, every
/// table element kind and every tree edit kind, so that a missing trigger
/// row fails a law.
#[test]
fn trigger_table_is_complete_for_every_element_kind() {
    for family in GRAPH_FAMILIES {
        let strategy = (
            arb_validation_schema(),
            arb_ginstance(),
            proptest::collection::vec(arb_graph_op(&GRAPH_FAMILIES), 1..4),
            arb_choices(),
        );
        eprintln!("graph family {family:?}");
        run_cases(48, &strategy, |(case, pre, ops, choices)| {
            let ops: Vec<GraphOp> = ops.into_iter().map(|op| GraphOp { family, ..op }).collect();
            let delta = graph_delta_of(&pre, &ops);
            let external = projection(&choices, graph_elements(&pre));
            let plan = law_plan(&case, InstanceShapeKind::Graph);
            v3_v4_case(
                &plan,
                &pre,
                &delta,
                &ValidationBudget::default(),
                &external,
                &external,
            )
        });
    }
    for family in TABLE_FAMILIES {
        let strategy = (
            arb_validation_schema(),
            arb_finstance(),
            proptest::collection::vec(arb_table_op(&TABLE_FAMILIES), 1..4),
            arb_choices(),
        );
        eprintln!("table family {family:?}");
        run_cases(48, &strategy, |(case, pre, ops, choices)| {
            let mut post = pre.clone();
            for op in ops {
                apply_table_op(&mut post, TableOp { family, ..op });
            }
            let delta = TableDelta::diff(&pre, &post);
            let external = projection(&choices, table_elements(&pre));
            let plan = law_plan(&case, InstanceShapeKind::Table);
            v3_v4_case(
                &plan,
                &pre,
                &delta,
                &ValidationBudget::default(),
                &external,
                &external,
            )
        });
    }
    for family in TREE_FAMILIES {
        let strategy = (
            arb_validation_schema(),
            arb_winstance(),
            proptest::collection::vec(arb_tree_op(&TREE_FAMILIES), 1..4),
            arb_choices(),
        );
        eprintln!("tree family {family:?}");
        run_cases(48, &strategy, |(case, pre, ops, choices)| {
            let ops: Vec<TreeOp> = ops.into_iter().map(|op| TreeOp { family, ..op }).collect();
            let edit = tree_edit_of(&pre, &ops);
            let external = projection(&choices, tree_elements(&pre));
            let plan = law_plan(&case, InstanceShapeKind::Tree);
            v3_v4_case(
                &plan,
                &pre,
                &edit,
                &ValidationBudget::default(),
                &external,
                &external,
            )
        });
    }
}

/// One drawn sequence step: the operations of its edit, read against the
/// instance the earlier steps left, whether it asks `RequirePassed`, and
/// its external projection.
#[derive(Clone, Debug)]
struct Step<Op> {
    ops: Vec<Op>,
    require_passed: bool,
    choices: Vec<u8>,
}

fn arb_steps<Op: std::fmt::Debug + Clone>(
    op: impl Strategy<Value = Op>,
) -> impl Strategy<Value = Vec<Step<Op>>> {
    proptest::collection::vec(
        (
            proptest::collection::vec(op, 0..4),
            proptest::bool::weighted(0.3),
            arb_choices(),
        )
            .prop_map(|(ops, require_passed, choices)| Step {
                ops,
                require_passed,
                choices,
            }),
        0..8,
    )
}

const fn policy(require_passed: bool) -> AdmissionPolicy {
    if require_passed {
        AdmissionPolicy::RequirePassed
    } else {
        AdmissionPolicy::NoNewFailures
    }
}

/// Sequence agreement over one shape: from a trusted initialization under `initial`
/// (finite or not), apply every step through `next`, which draws the step's
/// delta against the current instance, and check each step with
/// [`v5_step`]; every admitted step's witness also passes [`v6_case`].
fn v5_sequence<I: Surface, Op: Clone>(
    plan: &Arc<ValidationPlan>,
    start: I,
    initial: &ValidationBudget,
    steps: &[Step<Op>],
    next: impl Fn(&I, &[Op]) -> I::Delta,
    witnesses: bool,
) -> Result<(), TestCaseError> {
    let (mut validated, report) =
        Validated::<I>::new(plan.clone(), start, initial, &projection(&[1], Vec::new()));
    prop_assert_eq!(
        validated.report().canonical_bytes(),
        report.canonical_bytes()
    );
    for (i, step) in steps.iter().enumerate() {
        let delta = next(validated.instance(), &step.ops);
        let mut elements = validated.instance().elements();
        let mut post = validated.instance().clone();
        if I::apply_delta(&delta, &mut post).is_ok() {
            elements.extend(post.elements());
        }
        let external = projection(&step.choices, elements);
        let options = WitnessOptions {
            digests: i % 2 == 0,
        };
        let admitted = v5_step(
            &mut validated,
            plan,
            &delta,
            policy(step.require_passed),
            &external,
            options,
        )?;
        if witnesses && let Some((baseline, prior, witness)) = admitted {
            v6_case(
                plan,
                &WitnessReplay {
                    plan,
                    baseline: &baseline,
                    prior: &prior,
                    delta: &delta,
                    policy: policy(step.require_passed),
                    budget: &ValidationBudget::default(),
                    external: &external,
                    expected_options: options,
                },
                &witness,
            )?;
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Sequence agreement with faults, repairs, identity edits, net
    /// batches of several operations, external changes and invalid and
    /// finite-budget baselines, in every shape.
    #[test]
    fn v5_sequence_agreement(
        case in arb_validation_schema(),
        graph in arb_ginstance(),
        graph_steps in arb_steps(arb_graph_op(&GRAPH_FAMILIES)),
        table in arb_finstance(),
        table_steps in arb_steps(arb_table_op(&TABLE_FAMILIES)),
        tree in arb_winstance(),
        tree_steps in arb_steps(arb_tree_op(&TREE_FAMILIES)),
        initial in arb_baseline_budget(),
    ) {
        v5_sequence(
            &law_plan(&case, InstanceShapeKind::Graph),
            graph,
            &initial,
            &graph_steps,
            graph_delta_of,
            false,
        )?;
        v5_sequence(
            &law_plan(&case, InstanceShapeKind::Table),
            table,
            &initial,
            &table_steps,
            |pre: &FInstance, ops: &[TableOp]| {
                let mut post = pre.clone();
                for op in ops {
                    apply_table_op(&mut post, *op);
                }
                TableDelta::diff(pre, &post)
            },
            false,
        )?;
        v5_sequence(
            &law_plan(&case, InstanceShapeKind::Tree),
            tree,
            &initial,
            &tree_steps,
            tree_edit_of,
            false,
        )?;
    }

    /// Witness verification: every witness admitted along a generated
    /// sequence verifies against its exact replay context, and every
    /// alteration of the witness or of the context is accepted exactly when
    /// it is observationally equivalent.
    #[test]
    fn v6_every_witness_verifies_and_mutations_fail(
        case in arb_validation_schema(),
        graph in arb_ginstance(),
        graph_steps in arb_steps(arb_graph_op(&GRAPH_FAMILIES)),
        table in arb_finstance(),
        table_steps in arb_steps(arb_table_op(&TABLE_FAMILIES)),
        tree in arb_winstance(),
        tree_steps in arb_steps(arb_tree_op(&TREE_FAMILIES)),
        initial in arb_baseline_budget(),
    ) {
        v5_sequence(
            &law_plan(&case, InstanceShapeKind::Graph),
            graph,
            &initial,
            &graph_steps,
            graph_delta_of,
            true,
        )?;
        v5_sequence(
            &law_plan(&case, InstanceShapeKind::Table),
            table,
            &initial,
            &table_steps,
            |pre: &FInstance, ops: &[TableOp]| {
                let mut post = pre.clone();
                for op in ops {
                    apply_table_op(&mut post, *op);
                }
                TableDelta::diff(pre, &post)
            },
            true,
        )?;
        v5_sequence(
            &law_plan(&case, InstanceShapeKind::Tree),
            tree,
            &initial,
            &tree_steps,
            tree_edit_of,
            true,
        )?;
    }
}

// ---------------------------------------------------------------------------
// Exhaustive bounded scopes. Each enumerator lists every instance of a fixed
// small scope by direct construction, without calling the validator, and
// every law above is checked on every ordered pair of them (graphs and
// tables, related by their normalized `diff`) or on every instance with
// every step of a fixed step menu (trees).
// ---------------------------------------------------------------------------

/// The schema of the bounded graph and tree scopes: documents hold items, an
/// item requires its `name` edge, items link to items through `next`, which
/// is acyclic and of out-degree at most one, and an item's name is a
/// document-wide primary key.
fn bounded_schema() -> Case {
    use validation_support::{Builder, Features, Flag, e_items, e_name, e_next};
    let mut schema = Builder::default()
        .vertex("doc", "object")
        .vertex("item", "object")
        .vertex("name", "string")
        .edge(&e_items())
        .edge(&e_next())
        .edge(&e_name())
        .constraint("item", "maxOut", "1")
        .constraint("item", "acyclic", "next")
        .build();
    schema.required.insert("item".into(), vec![e_name()]);
    schema.declarations.insert_key(
        "item".into(),
        Key {
            id: "pk".into(),
            parts: vec![KeyPart::Path(vec![e_name()])],
            role: KeyRole::Primary,
            scope: KeyScope::Global,
        },
    );
    Case {
        schema,
        features: Features {
            max_len: None,
            num_bounds: None,
            max_out: Some(1),
            min_out: None,
            flags: BTreeSet::from([Flag::Acyclic, Flag::RequiredName]),
            item_key: Some(KeyScope::Global),
            item_ref: None,
            film_ref: None,
            title_max: None,
        },
    }
}

/// The schema of the bounded table scope: the generated schema with a strict
/// film reference to actors by name and a one-character title bound.
fn bounded_table_schema() -> Case {
    use validation_support::{Features, schema_of};
    let features = Features {
        max_len: None,
        num_bounds: None,
        max_out: None,
        min_out: None,
        flags: BTreeSet::new(),
        item_key: None,
        item_ref: None,
        film_ref: Some((false, OnUnresolved::Error)),
        title_max: Some(1),
    };
    Case {
        schema: schema_of(&features),
        features,
    }
}

/// The law generators draw every declaration the shared schema generator
/// can carry, so the laws range over each.
#[test]
fn the_law_generators_draw_every_declaration() {
    use proptest::strategy::ValueTree as _;
    use validation_support::Flag;
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let mut flags = BTreeSet::new();
    let (mut keys, mut within, mut item_refs, mut film_refs) = (false, false, false, false);
    for _ in 0..400 {
        let case = arb_validation_schema()
            .new_tree(&mut runner)
            .unwrap()
            .current();
        flags.extend(case.features.flags.iter().copied());
        keys |= case.features.item_key.is_some();
        within |= matches!(case.features.item_key, Some(KeyScope::Within(_)));
        item_refs |= case.features.item_ref.is_some();
        film_refs |= case.features.film_ref.is_some();
    }
    assert_eq!(flags.len(), 10, "{flags:?}");
    assert!(flags.contains(&Flag::Acyclic));
    assert!(keys && within && item_refs && film_refs);
}

/// Every graph of the bounded scope: a document 0, an item 1, a node 2
/// anchored at `item` or the undeclared `ghost`, a name 3 with the value
/// `a` or none, and any subset of the edges `0 -items-> 1`, `1 -next-> 2`,
/// `2 -next-> 1`, `1 -name-> 3` and `2 -name-> 3`, each under a fixed id.
/// 128 graphs.
fn every_bounded_graph() -> Vec<GInstance> {
    use validation_support::{e_items, e_name, e_next};
    let candidates = [
        (0, 1, e_items()),
        (1, 2, e_next()),
        (2, 1, e_next()),
        (1, 3, e_name()),
        (2, 3, e_name()),
    ];
    let mut out = Vec::new();
    for ghost in [false, true] {
        for named in [false, true] {
            for bits in 0_u32..32 {
                let mut graph = GInstance::new()
                    .with_node(Node::new(0, "doc"))
                    .with_node(Node::new(1, "item"))
                    .with_node(Node::new(2, if ghost { "ghost" } else { "item" }));
                let mut name = Node::new(3, "name");
                if named {
                    name = name.with_value(FieldPresence::Present(Value::Str("a".into())));
                }
                graph = graph.with_node(name);
                for (i, (s, t, label)) in candidates.iter().enumerate() {
                    if bits & (1 << i) != 0 {
                        graph = graph.with_edge_id(
                            EdgeId(u32::try_from(i).unwrap()),
                            *s,
                            *t,
                            label.clone(),
                        );
                    }
                }
                out.push(graph);
            }
        }
    }
    out
}

/// Every table instance of the bounded scope: a film 0 titled `a` or not,
/// actor 10 named `a` or not, actor 11 named `b` or not, the film's lead
/// naming `a`, and any
/// subset of the pairs `0 -stars-> 10` and `0 -stars-> 11`. 32 instances.
fn every_bounded_table() -> Vec<FInstance> {
    let mut out = Vec::new();
    for bits in 0_u32..32 {
        let cell = |on: bool, column: &str, text: &str| {
            if on {
                HashMap::from([(column.to_owned(), Value::Str(text.into()))])
            } else {
                HashMap::new()
            }
        };
        let mut film = cell(bits & 1 != 0, "title", "a");
        film.insert("year".into(), Value::Int(2000));
        film.insert("lead".into(), Value::Str("a".into()));
        let mut table = FInstance::new()
            .with_rows("film", [(RowId(0), film)])
            .unwrap()
            .with_rows(
                "actor",
                [
                    (RowId(10), cell(bits & 2 != 0, "aname", "a")),
                    (RowId(11), cell(bits & 4 != 0, "aname", "b")),
                ],
            )
            .unwrap();
        for (bit, actor) in [(8, 10), (16, 11)] {
            if bits & bit != 0 {
                table
                    .insert_foreign_key(validation_support::e_stars(), RowId(0), RowId(actor))
                    .unwrap();
            }
        }
        out.push(table);
    }
    out
}

/// Every tree of the bounded scope: a root document and up to four more
/// nodes, each under any earlier node, anchored at `item` or `name` where
/// the schema declares an edge from its parent's anchor, a name valued `a`
/// at an even id and `b` at an odd one.
fn every_bounded_tree() -> Vec<WInstance> {
    use validation_support::{e_items, e_name, e_next};
    let declared = |parent: &str, child: &str| match (parent, child) {
        ("doc", "item") => Some(e_items()),
        ("item", "item") => Some(e_next()),
        ("item", "name") => Some(e_name()),
        _ => None,
    };
    let mut shapes: Vec<Vec<(usize, &str)>> = vec![Vec::new()];
    for _ in 0..4 {
        let mut grown = shapes.clone();
        for shape in &shapes {
            let anchors: Vec<&str> = std::iter::once("doc")
                .chain(shape.iter().map(|(_, a)| *a))
                .collect();
            for (parent, anchor) in anchors.iter().enumerate() {
                for child in ["item", "name"] {
                    if declared(anchor, child).is_some() {
                        let mut next = shape.clone();
                        next.push((parent, child));
                        if !grown.contains(&next) {
                            grown.push(next);
                        }
                    }
                }
            }
        }
        shapes = grown;
    }
    shapes
        .into_iter()
        .map(|shape| {
            let anchors: Vec<&str> = std::iter::once("doc")
                .chain(shape.iter().map(|(_, a)| *a))
                .collect();
            let mut nodes = HashMap::from([(0, Node::new(0, "doc"))]);
            let mut arcs = Vec::new();
            for (i, (parent, anchor)) in shape.iter().enumerate() {
                let id = u32::try_from(i + 1).unwrap();
                let mut node = Node::new(id, *anchor);
                if *anchor == "name" {
                    let text = if id % 2 == 0 { "a" } else { "b" };
                    node = node.with_value(FieldPresence::Present(Value::Str(text.into())));
                }
                nodes.insert(id, node);
                arcs.push((
                    u32::try_from(*parent).unwrap(),
                    id,
                    declared(anchors[*parent], anchor).unwrap(),
                ));
            }
            WInstance::new(nodes, arcs, Vec::new(), 0, "doc".into())
        })
        .collect()
}

/// The fixed step menu of a bounded tree: for every node, delete, contract,
/// relabel to each scope anchor, set and remove the field `name`, move under
/// every node along `items` and `next`, insert a child of each anchor, and
/// insert a `rel` fan with the node as parent; and delete the `rel` fan.
fn tree_step_menu(tree: &WInstance) -> Vec<TreeEdit> {
    use validation_support::{e_items, e_next};
    let mut ids: Vec<u32> = tree.nodes.keys().copied().collect();
    ids.sort_unstable();
    let fresh = ids.last().map_or(0, |last| last + 1);
    let mut menu = vec![TreeEdit::DeleteFan {
        hyper_edge_id: "rel".into(),
    }];
    for &n in &ids {
        menu.push(TreeEdit::DeleteNode { id: n });
        menu.push(TreeEdit::ContractNode { id: n });
        for anchor in ["doc", "item", "name", "ghost"] {
            menu.push(TreeEdit::RelabelNode {
                id: n,
                new_anchor: anchor.into(),
            });
        }
        menu.push(TreeEdit::SetField {
            node_id: n,
            field: "name".into(),
            value: Value::Str("a".into()),
        });
        menu.push(TreeEdit::RemoveField {
            node_id: n,
            field: "name".into(),
        });
        for &p in &ids {
            for edge in [e_items(), e_next(), validation_support::e_name()] {
                menu.push(TreeEdit::MoveSubtree {
                    node_id: n,
                    new_parent: p,
                    edge,
                });
            }
        }
        for (anchor, edge) in [("item", e_items()), ("name", validation_support::e_name())] {
            menu.push(TreeEdit::InsertNode {
                parent: n,
                child_id: fresh,
                node: Node::new(fresh, anchor),
                edge,
            });
        }
        if let Some(&other) = ids.iter().find(|&&m| m != n) {
            menu.push(TreeEdit::InsertFan {
                fan: panproto_inst::Fan::new("rel", n).with_child("a", other),
            });
        }
    }
    menu
}

/// The external projections of the bounded scopes: none, every element
/// passing, every element rejected.
const BOUNDED_PROJECTIONS: [&[u8]; 3] = [&[], &[1], &[2]];

/// Footprint soundness and edit agreement on every ordered pair of bounded graphs (16384 transitions),
/// from an unlimited and a four-read baseline, under unchanged and changed
/// external projections on the diagonal.
#[test]
fn v3_v4_hold_on_every_bounded_graph_transition() {
    let case = bounded_schema();
    let plan = law_plan(&case, InstanceShapeKind::Graph);
    let graphs = every_bounded_graph();
    assert_eq!(graphs.len(), 128);
    let mut checked = 0_usize;
    for pre in &graphs {
        for post in &graphs {
            let delta = GraphDelta::diff(pre, post);
            let budget = if checked % 2 == 0 {
                ValidationBudget::default()
            } else {
                ValidationBudget { max_reads: Some(4) }
            };
            let pass = projection(&[1], graph_elements(pre));
            v3_v4_case(&plan, pre, &delta, &budget, &pass, &pass).unwrap();
            checked += 1;
        }
        for before in BOUNDED_PROJECTIONS {
            for after in BOUNDED_PROJECTIONS {
                let (x, y) = projections(before, after, &graph_elements(pre));
                v3_v4_case(
                    &plan,
                    pre,
                    &GraphDelta::identity(),
                    &ValidationBudget::default(),
                    &x,
                    &y,
                )
                .unwrap();
            }
        }
    }
    assert_eq!(checked, 16384);
}

/// Footprint soundness and edit agreement on every ordered pair of bounded tables (1024 transitions),
/// under every pair of bounded projections.
#[test]
fn v3_v4_hold_on_every_bounded_table_transition() {
    let case = bounded_table_schema();
    let plan = law_plan(&case, InstanceShapeKind::Table);
    let tables = every_bounded_table();
    assert_eq!(tables.len(), 32);
    for pre in &tables {
        for post in &tables {
            let delta = TableDelta::diff(pre, post);
            for (i, before) in BOUNDED_PROJECTIONS.iter().enumerate() {
                let after = BOUNDED_PROJECTIONS[(i + 1) % 3];
                let (x, y) = projections(before, after, &table_elements(pre));
                v3_v4_case(&plan, pre, &delta, &ValidationBudget::default(), &x, &y).unwrap();
            }
        }
    }
}

/// Footprint soundness and edit agreement on every bounded tree with every step of its step menu,
/// from an unlimited and a four-read baseline.
#[test]
fn v3_v4_hold_on_every_bounded_tree_step() {
    let case = bounded_schema();
    let plan = law_plan(&case, InstanceShapeKind::Tree);
    let trees = every_bounded_tree();
    let mut checked = 0_usize;
    for pre in &trees {
        for step in tree_step_menu(pre) {
            let budget = if checked % 2 == 0 {
                ValidationBudget::default()
            } else {
                ValidationBudget { max_reads: Some(4) }
            };
            let pass = projection(&[1], tree_elements(pre));
            v3_v4_case(&plan, pre, &step, &budget, &pass, &pass).unwrap();
            checked += 1;
        }
    }
    eprintln!("bounded trees {} steps {checked}", trees.len());
}

/// The steps a bounded scope applies to an instance.
type Steps<'a, I> = &'a dyn Fn(&I) -> Vec<<I as ValidatedShape>::Delta>;

/// One bounded scope of the sequence and witness laws: the starting
/// instances, the steps applied to each start, the steps applied after
/// them (none for sequences of one step), and whether witnesses are
/// checked.
struct SequenceScope<'a, I: Surface> {
    starts: &'a [I],
    first: Steps<'a, I>,
    then: Option<Steps<'a, I>>,
    witnesses: bool,
}

/// The steps applied and the witnesses checked by [`bounded_sequences`].
#[derive(Default)]
struct SequenceCounts {
    steps: usize,
    witnesses: usize,
    passed_baselines: usize,
}

/// Sequence agreement on every sequence of a bounded scope, and, when the
/// scope asks for it, witness verification of every admitted step. From
/// each start, initialized under an unlimited and a four-read budget with
/// every element passing its external check, every first step is applied
/// under each policy, and every later step under each policy to every
/// state so reached. Each step is checked by the sequence law's step check
/// (an admitted step's state reports full validation and reattaches, a
/// rejected step changes nothing), each admitted step's witness by the
/// witness law's check, and on every `Passed` baseline the two policies
/// admit or reject the step together.
fn bounded_sequences<I: Surface>(
    plan: &Arc<ValidationPlan>,
    scope: &SequenceScope<'_, I>,
) -> SequenceCounts {
    let mut counts = SequenceCounts::default();
    for start in scope.starts {
        for initial in [
            ValidationBudget::default(),
            ValidationBudget { max_reads: Some(4) },
        ] {
            let (validated, _) = Validated::<I>::new(
                plan.clone(),
                start.clone(),
                &initial,
                &projection(&[1], start.elements()),
            );
            for first in (scope.first)(start) {
                let reached = bounded_step(plan, &validated, &first, scope.witnesses, &mut counts);
                let Some(then) = scope.then else {
                    continue;
                };
                for next in reached {
                    for step in then(next.instance()) {
                        bounded_step(plan, &next, &step, scope.witnesses, &mut counts);
                    }
                }
            }
        }
    }
    counts
}

/// One step of [`bounded_sequences`] under each policy, with every element
/// of the instance before and after it passing its external check; returns
/// the state after it under each policy.
fn bounded_step<I: Surface>(
    plan: &Arc<ValidationPlan>,
    validated: &Validated<I>,
    delta: &I::Delta,
    witnesses: bool,
    counts: &mut SequenceCounts,
) -> Vec<Validated<I>> {
    let mut elements = validated.instance().elements();
    let mut post = validated.instance().clone();
    if I::apply_delta(delta, &mut post).is_ok() {
        elements.extend(post.elements());
    }
    let external = projection(&[1], elements);
    let mut reached = Vec::new();
    let mut admissions = Vec::new();
    for require_passed in [true, false] {
        let mut next = validated.clone();
        let options = WitnessOptions {
            digests: counts.steps % 2 == 0,
        };
        let admitted = v5_step(
            &mut next,
            plan,
            delta,
            policy(require_passed),
            &external,
            options,
        )
        .unwrap_or_else(|e| panic!("sequence law: {e}"));
        counts.steps += 1;
        admissions.push(admitted.is_some());
        if witnesses && let Some((baseline, prior, witness)) = admitted {
            v6_case(
                plan,
                &WitnessReplay {
                    plan,
                    baseline: &baseline,
                    prior: &prior,
                    delta,
                    policy: policy(require_passed),
                    budget: &ValidationBudget::default(),
                    external: &external,
                    expected_options: options,
                },
                &witness,
            )
            .unwrap_or_else(|e| panic!("witness law: {e}"));
            counts.witnesses += 1;
        }
        reached.push(next);
    }
    if validated.state().status == CheckStatus::Passed {
        assert_eq!(
            admissions[0], admissions[1],
            "policies on a passed baseline"
        );
        counts.passed_baselines += 1;
    }
    reached
}

/// The graphs of the sequence scope: a document 0, items 1 and 2, names 3
/// valued `a` and 4 valued `b`, and any subset of the edges `0 -items-> 1`,
/// `0 -items-> 2`, `1 -name-> 3`, `2 -name-> 4`, `1 -next-> 2` and
/// `2 -next-> 1` (bits 0 to 5, each under its bit's edge id) that holds
/// every edge of `fixed`: `2^(6 - |fixed|)` graphs. Unlike the scope of
/// [`every_bounded_graph`], whose items share one name, some of these
/// graphs pass.
fn sequence_graphs(fixed: u32) -> Vec<GInstance> {
    use validation_support::{e_items, e_name, e_next};
    let candidates = [
        (0, 1, e_items()),
        (0, 2, e_items()),
        (1, 3, e_name()),
        (2, 4, e_name()),
        (1, 2, e_next()),
        (2, 1, e_next()),
    ];
    let mut out = Vec::new();
    for bits in 0_u32..64 {
        if bits & fixed != fixed {
            continue;
        }
        let mut graph = GInstance::new()
            .with_node(Node::new(0, "doc"))
            .with_node(Node::new(1, "item"))
            .with_node(Node::new(2, "item"))
            .with_node(
                Node::new(3, "name").with_value(FieldPresence::Present(Value::Str("a".into()))),
            )
            .with_node(
                Node::new(4, "name").with_value(FieldPresence::Present(Value::Str("b".into()))),
            );
        for (i, (s, t, label)) in candidates.iter().enumerate() {
            if bits & (1 << i) != 0 {
                graph =
                    graph.with_edge_id(EdgeId(u32::try_from(i).unwrap()), *s, *t, label.clone());
            }
        }
        out.push(graph);
    }
    out
}

/// The bounded tables whose enumeration bits include every bit of `fixed`
/// (bit 0 the film's title, bits 1 and 2 the actors' names, bits 3 and 4
/// the stars pairs): `2^(5 - |fixed|)` tables.
fn bounded_tables_holding(fixed: u32) -> Vec<FInstance> {
    every_bounded_table()
        .into_iter()
        .zip(0_u32..)
        .filter(|(_, bits)| bits & fixed == fixed)
        .map(|(table, _)| table)
        .collect()
}

/// The bounded trees of at most `nodes` nodes, the root included.
fn bounded_trees_up_to(nodes: usize) -> Vec<WInstance> {
    every_bounded_tree()
        .into_iter()
        .filter(|tree| tree.nodes.len() <= nodes)
        .collect()
}

/// The sequence law on every bounded table sequence: every sequence of one
/// step between the 32 bounded tables (4,096 steps), and every sequence of
/// two steps between the 8 tables with a titled film and a named actor 10
/// (4,352 steps), each step under both policies and from both initial
/// budgets.
#[test]
fn sequence_law_holds_on_every_bounded_table_sequence() {
    let plan = law_plan(&bounded_table_schema(), InstanceShapeKind::Table);
    let all = bounded_tables_holding(0);
    assert_eq!(all.len(), 32);
    let to_all = |pre: &FInstance| -> Vec<TableDelta> {
        all.iter().map(|post| TableDelta::diff(pre, post)).collect()
    };
    let one = bounded_sequences(
        &plan,
        &SequenceScope {
            starts: &all,
            first: &to_all,
            then: None,
            witnesses: false,
        },
    );
    let small = bounded_tables_holding(0b0_0011);
    assert_eq!(small.len(), 8);
    let to_small = |pre: &FInstance| -> Vec<TableDelta> {
        small
            .iter()
            .map(|post| TableDelta::diff(pre, post))
            .collect()
    };
    let two = bounded_sequences(
        &plan,
        &SequenceScope {
            starts: &small,
            first: &to_small,
            then: Some(&to_small),
            witnesses: false,
        },
    );
    assert_eq!(one.steps, 4096);
    assert_eq!(two.steps, 4352);
    assert!(one.passed_baselines > 0 && two.passed_baselines > 0);
    eprintln!(
        "bounded table sequences: {} one-step and {} two-step steps, {} passed baselines",
        one.steps,
        two.steps,
        one.passed_baselines + two.passed_baselines
    );
}

/// The sequence law on every bounded tree sequence: every step of the step
/// menu from every bounded tree of at most four nodes, and every sequence
/// of two menu steps from every bounded tree of at most two nodes, each
/// step under both policies and from both initial budgets.
#[test]
fn sequence_law_holds_on_every_bounded_tree_sequence() {
    let plan = law_plan(&bounded_schema(), InstanceShapeKind::Tree);
    let menu = |pre: &WInstance| -> Vec<TreeEdit> { tree_step_menu(pre) };
    let four = bounded_trees_up_to(4);
    let one = bounded_sequences(
        &plan,
        &SequenceScope {
            starts: &four,
            first: &menu,
            then: None,
            witnesses: false,
        },
    );
    let two_nodes = bounded_trees_up_to(2);
    let two = bounded_sequences(
        &plan,
        &SequenceScope {
            starts: &two_nodes,
            first: &menu,
            then: Some(&menu),
            witnesses: false,
        },
    );
    assert!(one.passed_baselines > 0 && two.passed_baselines > 0);
    eprintln!(
        "bounded tree sequences: {} trees of at most 4 nodes, {} one-step steps; {} trees of at \
         most 2 nodes, {} two-step steps; {} passed baselines",
        four.len(),
        one.steps,
        two_nodes.len(),
        two.steps,
        one.passed_baselines + two.passed_baselines
    );
}

/// The witness law on every admitted bounded step: every step between the
/// 16 sequence graphs holding both `items` edges, between the 8 tables with a
/// titled film and a named actor 10, and of the step menu from every
/// bounded tree of at most three nodes, under both policies and from both
/// initial budgets. Each witness verifies against its exact replay
/// context, every alteration of the witness is accepted exactly when its
/// canonical bytes are the witness's, and every altered context exactly
/// when admission under it reproduces the witness.
#[test]
fn witness_law_holds_on_every_bounded_step() {
    let graphs = sequence_graphs(0b00_0011);
    assert_eq!(graphs.len(), 16);
    let to_graphs = |pre: &GInstance| -> Vec<GraphDelta> {
        graphs
            .iter()
            .map(|post| GraphDelta::diff(pre, post))
            .collect()
    };
    let graph = bounded_sequences(
        &law_plan(&bounded_schema(), InstanceShapeKind::Graph),
        &SequenceScope {
            starts: &graphs,
            first: &to_graphs,
            then: None,
            witnesses: true,
        },
    );
    let tables = bounded_tables_holding(0b0_0011);
    let to_tables = |pre: &FInstance| -> Vec<TableDelta> {
        tables
            .iter()
            .map(|post| TableDelta::diff(pre, post))
            .collect()
    };
    let table = bounded_sequences(
        &law_plan(&bounded_table_schema(), InstanceShapeKind::Table),
        &SequenceScope {
            starts: &tables,
            first: &to_tables,
            then: None,
            witnesses: true,
        },
    );
    let menu = |pre: &WInstance| -> Vec<TreeEdit> { tree_step_menu(pre) };
    let trees = bounded_trees_up_to(3);
    let tree = bounded_sequences(
        &law_plan(&bounded_schema(), InstanceShapeKind::Tree),
        &SequenceScope {
            starts: &trees,
            first: &menu,
            then: None,
            witnesses: true,
        },
    );
    assert_eq!(graph.steps, 1024);
    assert_eq!(table.steps, 256);
    for (shape, counts) in [("graph", &graph), ("table", &table), ("tree", &tree)] {
        assert!(counts.witnesses > 0, "{shape}: no admitted step");
        eprintln!(
            "bounded {shape} witnesses: {} steps, {} witnesses checked",
            counts.steps, counts.witnesses
        );
    }
}

/// The sequence law on every bounded graph sequence: every sequence of one
/// step between the 32 sequence graphs holding `0 -items-> 1` (4,096
/// steps), and every sequence of two steps between the 8 holding both
/// `items` edges and `1 -name-> 3` (4,352 steps), each step under both
/// policies and from both initial budgets; the policies agree on every
/// passed baseline.
#[test]
fn sequence_law_holds_on_every_bounded_graph_sequence() {
    let plan = law_plan(&bounded_schema(), InstanceShapeKind::Graph);
    let core = sequence_graphs(0b00_0001);
    assert_eq!(core.len(), 32);
    let to_core = |pre: &GInstance| -> Vec<GraphDelta> {
        core.iter()
            .map(|post| GraphDelta::diff(pre, post))
            .collect()
    };
    let one = bounded_sequences(
        &plan,
        &SequenceScope {
            starts: &core,
            first: &to_core,
            then: None,
            witnesses: false,
        },
    );
    let small = sequence_graphs(0b00_0111);
    assert_eq!(small.len(), 8);
    let to_small = |pre: &GInstance| -> Vec<GraphDelta> {
        small
            .iter()
            .map(|post| GraphDelta::diff(pre, post))
            .collect()
    };
    let two = bounded_sequences(
        &plan,
        &SequenceScope {
            starts: &small,
            first: &to_small,
            then: Some(&to_small),
            witnesses: false,
        },
    );
    assert_eq!(one.steps, 4096);
    assert_eq!(two.steps, 8 * 2 * 8 * 2 + 8 * 2 * 8 * 2 * 8 * 2);
    assert!(one.passed_baselines > 0 && two.passed_baselines > 0);
    eprintln!(
        "bounded graph sequences: {} one-step and {} two-step steps, {} passed baselines",
        one.steps,
        two.steps,
        one.passed_baselines + two.passed_baselines
    );
}

// ---------------------------------------------------------------------------
// The baseline-relative recheck of a merged delta. Independence excludes
// representation conflicts, not jointly introduced violations, so the
// authoritative answer for `merge(a, b)` is `check_graph_delta(pre,
// merge(a, b))`. Its oracle is whole validation of the merged result: every
// diagnostic it reports is one whole validation reports, every diagnostic of
// whole validation it omits was already reported of the baseline with the
// same bytes, and from a passing baseline the two reports agree exactly.
// ---------------------------------------------------------------------------

/// Check the merged recheck of the independent pair `(left, right)` from
/// `pre` against whole validation of the merged result.
fn merged_recheck_case(
    plan: &ValidationPlan,
    pre: &GInstance,
    left: &GraphDelta,
    right: &GraphDelta,
    external: &ExternalVerdicts,
) -> TestResult {
    let unlimited = ValidationBudget::default();
    let merged = left.merge(right)?;
    prop_assert_eq_ok(
        merged == right.merge(left)?,
        "merge commutes on an independent pair",
    )?;
    let mut post = pre.clone();
    if merged.apply(&mut post).is_err() {
        assert!(
            plan.check_graph_delta(pre, &merged, &unlimited, external)
                .is_err()
        );
        return Ok(());
    }
    let local = plan.check_graph_delta(pre, &merged, &unlimited, external)?;
    let before = plan.validate_graph(pre, &unlimited, external);
    let after = plan.validate_graph(&post, &unlimited, external);
    let bytes = |d: &panproto_inst::validation::Diagnostic| rmp_serde::to_vec_named(d);
    let after_bytes: Vec<Vec<u8>> = after
        .diagnostics
        .iter()
        .map(bytes)
        .collect::<Result<_, _>>()?;
    let before_bytes: Vec<Vec<u8>> = before
        .diagnostics
        .iter()
        .map(bytes)
        .collect::<Result<_, _>>()?;
    let local_bytes: Vec<Vec<u8>> = local
        .diagnostics
        .iter()
        .map(bytes)
        .collect::<Result<_, _>>()?;
    for d in &local_bytes {
        assert!(
            after_bytes.contains(d),
            "a merged recheck diagnostic whole validation lacks"
        );
    }
    for d in &after_bytes {
        assert!(
            local_bytes.contains(d) || before_bytes.contains(d),
            "a new diagnostic of the merged result the recheck omits"
        );
    }
    let incomplete = |i: &panproto_inst::validation::IncompleteCheck| rmp_serde::to_vec_named(i);
    let after_incomplete: Vec<Vec<u8>> = after
        .incomplete
        .iter()
        .map(incomplete)
        .collect::<Result<_, _>>()?;
    let before_incomplete: Vec<Vec<u8>> = before
        .incomplete
        .iter()
        .map(incomplete)
        .collect::<Result<_, _>>()?;
    let local_incomplete: Vec<Vec<u8>> = local
        .incomplete
        .iter()
        .map(incomplete)
        .collect::<Result<_, _>>()?;
    for i in &local_incomplete {
        assert!(
            after_incomplete.contains(i),
            "a merged recheck incompleteness whole validation lacks"
        );
    }
    for i in &after_incomplete {
        assert!(
            local_incomplete.contains(i) || before_incomplete.contains(i),
            "a new incompleteness of the merged result the recheck omits"
        );
    }
    // When every local or external instance of the baseline passes, the
    // recheck is whole validation of the merged result.
    let is_local = |check: &CheckKey| {
        plan.checks().get(check).is_some_and(|spec| {
            matches!(
                spec.class,
                CheckClass::GraphLocal | CheckClass::BoundedExternal
            )
        })
    };
    if !before.diagnostics.iter().any(|d| is_local(&d.check))
        && !before.incomplete.iter().any(|i| is_local(&i.check))
    {
        assert_eq!(local.status, after.status);
        assert_eq!(local.diagnostics, after.diagnostics);
        assert_eq!(local.incomplete, after.incomplete);
    }
    Ok(())
}

fn prop_assert_eq_ok(holds: bool, what: &str) -> TestResult {
    if holds { Ok(()) } else { Err(what.into()) }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 192,
        max_global_rejects: 100_000,
        ..ProptestConfig::default()
    })]

    /// The merged recheck agrees with its oracle over generated schemas,
    /// graphs and pairs of deltas, kept exactly when the pair is independent
    /// (the independence domain, not narrowed by any validation property).
    #[test]
    fn merged_independent_delta_recheck_matches_oracle(
        case in arb_validation_schema(),
        pre in arb_ginstance(),
        left_ops in proptest::collection::vec(arb_graph_op(&GRAPH_FAMILIES), 1..3),
        right_ops in proptest::collection::vec(arb_graph_op(&GRAPH_FAMILIES), 1..3),
        choices in arb_choices(),
    ) {
        let left = graph_delta_of(&pre, &left_ops);
        let right = graph_delta_of(&pre, &right_ops);
        prop_assume!(left.independence(&right).is_empty());
        let plan = law_plan(&case, InstanceShapeKind::Graph);
        let external = projection(&choices, graph_elements(&pre));
        merged_recheck_case(&plan, &pre, &left, &right, &external)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
    }
}

/// The merged recheck agrees with its oracle on every independent pair of
/// one-element changes (an edge added or removed, the anchor of node 2, the
/// value of node 3) from every bounded graph.
#[test]
fn merged_recheck_matches_its_oracle_on_every_bounded_pair() {
    let case = bounded_schema();
    let plan = law_plan(&case, InstanceShapeKind::Graph);
    let graphs = every_bounded_graph();
    let mut pairs = 0_usize;
    for (i, pre) in graphs.iter().enumerate() {
        // The bounded graphs are indexed by their seven bits (five edges,
        // then the value of node 3, then the anchor of node 2), so a
        // one-element change flips one bit of the index.
        let changes: Vec<GraphDelta> = (0..7)
            .map(|bit| GraphDelta::diff(pre, &graphs[i ^ (1 << bit)]))
            .collect();
        for left in &changes {
            for right in &changes {
                if left.independence(right).is_empty() {
                    let external = projection(&[1], graph_elements(pre));
                    merged_recheck_case(&plan, pre, left, right, &external).unwrap();
                    pairs += 1;
                }
            }
        }
    }
    assert!(pairs > 1000, "{pairs}");
}

/// A tree whose arcs close a cycle off the root: a changed arc of the cycle
/// makes the cycle's nodes candidates, and the subtree walk that finds them
/// visits each node once and ends.
#[test]
fn a_tree_edit_on_an_arc_cycle_off_the_root_terminates() {
    use validation_support::e_next;
    let tree = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "doc")),
            (1, Node::new(1, "item")),
            (2, Node::new(2, "item")),
        ]),
        vec![(1, 2, e_next()), (2, 1, e_next())],
        Vec::new(),
        0,
        "doc".into(),
    );
    let touched = BTreeSet::from([ViewRef::Arc {
        parent: 1,
        child: 2,
    }]);
    let candidates = WInstance::candidate_subjects(&TreeEdit::Identity, &tree, &tree, &touched);
    assert_eq!(
        candidates,
        BTreeSet::from([
            ViewRef::Node(1),
            ViewRef::Node(2),
            ViewRef::Arc {
                parent: 1,
                child: 2
            },
            ViewRef::Arc {
                parent: 2,
                child: 1
            },
        ])
    );
}

#[test]
fn tree_shape_lowering_is_structurally_canonical() -> TestResult {
    assert_eq!(
        <WInstance as ValidatedShape>::shape(),
        InstanceShapeKind::Tree
    );

    let empty = <WInstance as ValidatedShape>::lower(&TreeEdit::Sequence(Vec::new()))?;
    assert!(matches!(empty, TreeEdit::Identity));

    let singleton =
        <WInstance as ValidatedShape>::lower(&TreeEdit::Sequence(vec![TreeEdit::DeleteNode {
            id: 7,
        }]))?;
    assert!(matches!(singleton, TreeEdit::DeleteNode { id: 7 }));
    Ok(())
}

#[test]
fn tree_touched_support_distinguishes_incidence_and_root_changes() {
    use validation_support::e_next;

    let nodes = HashMap::from([
        (0, Node::new(0, "doc")),
        (1, Node::new(1, "item")),
        (2, Node::new(2, "item")),
    ]);
    let before = WInstance::new(
        nodes.clone(),
        vec![(0, 1, e_next())],
        Vec::new(),
        0,
        "doc".into(),
    );
    assert!(
        WInstance::touched(&TreeEdit::Identity, &before, &before).is_empty(),
        "identical incidence sequences have empty support"
    );

    let after = WInstance::new(
        nodes.clone(),
        vec![(0, 1, e_next()), (0, 2, e_next())],
        Vec::new(),
        0,
        "doc".into(),
    );
    assert_eq!(
        WInstance::touched(&TreeEdit::Identity, &before, &after),
        BTreeSet::from([
            ViewRef::Node(0),
            ViewRef::Node(2),
            ViewRef::Arc {
                parent: 0,
                child: 2,
            },
        ])
    );

    let root_zero = WInstance::new(nodes.clone(), Vec::new(), Vec::new(), 0, "doc".into());
    let root_one = WInstance::new(nodes, Vec::new(), Vec::new(), 1, "doc".into());
    assert_eq!(
        WInstance::touched(&TreeEdit::Identity, &root_zero, &root_one),
        BTreeSet::from([ViewRef::Node(0), ViewRef::Node(1)])
    );
}

#[test]
fn table_candidates_close_rows_and_pairs_bidirectionally() {
    let table = every_bounded_table()
        .into_iter()
        .find(|table| !table.all_pairs().is_empty())
        .expect("the bounded tables include a foreign-key pair");
    let (edge, src, tgt) = table
        .all_pairs()
        .into_iter()
        .next()
        .expect("the selected table has a foreign-key pair");
    let pair = ViewRef::Pair { edge, src, tgt };

    let from_row = FInstance::candidate_subjects(
        &TableDelta::identity(),
        &table,
        &table,
        &BTreeSet::from([ViewRef::Row(src)]),
    );
    assert!(from_row.contains(&pair));

    let from_target = FInstance::candidate_subjects(
        &TableDelta::identity(),
        &table,
        &table,
        &BTreeSet::from([ViewRef::Row(tgt)]),
    );
    assert!(from_target.contains(&pair));

    let from_pair = FInstance::candidate_subjects(
        &TableDelta::identity(),
        &table,
        &table,
        &BTreeSet::from([pair.clone()]),
    );
    assert_eq!(
        from_pair,
        BTreeSet::from([pair, ViewRef::Row(src), ViewRef::Row(tgt)])
    );
}

/// Every element of a graph: its nodes and edges.
fn graph_carrier(graph: &GInstance) -> BTreeSet<ViewRef> {
    let mut out: BTreeSet<ViewRef> = graph_elements(graph).into_iter().collect();
    out.extend(graph.edges_sorted().map(|(id, _)| ViewRef::Edge(id)));
    out
}

/// Every element of a table instance: its rows and foreign-key pairs.
fn table_carrier(table: &FInstance) -> BTreeSet<ViewRef> {
    let mut out: BTreeSet<ViewRef> = table_elements(table).into_iter().collect();
    out.extend(
        table
            .all_pairs()
            .into_iter()
            .map(|(edge, src, tgt)| ViewRef::Pair { edge, src, tgt }),
    );
    out
}

/// Every element of a tree: its nodes, its arcs, and every node a fan
/// names, present or not.
fn tree_carrier(tree: &WInstance) -> BTreeSet<ViewRef> {
    let mut out: BTreeSet<ViewRef> = tree_elements(tree).into_iter().collect();
    out.extend(tree.arcs.iter().map(|(parent, child, _)| ViewRef::Arc {
        parent: *parent,
        child: *child,
    }));
    for fan in &tree.fans {
        out.insert(ViewRef::Node(fan.parent));
        out.extend(fan.children.values().map(|id| ViewRef::Node(*id)));
    }
    out
}

/// The subject-keyed check instances at `carrier` are exactly the
/// enumerated check instances with a subject: deciding whether an element is
/// an instance subject agrees with enumerating the instances, which is what
/// lets a transition compare instance sets at its candidate subjects only.
fn instance_membership_agrees<I: Surface>(
    plan: &ValidationPlan,
    instance: &I,
    carrier: &BTreeSet<ViewRef>,
) {
    let enumerated: BTreeSet<CheckInstance> = instance
        .check_instances(plan)
        .into_iter()
        .filter(|i| i.subject.is_some())
        .collect();
    assert_eq!(instance.check_instances_at(plan, carrier), enumerated);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn instance_subjects_agree_with_enumerated_instances(
        case in arb_validation_schema(),
        graph in arb_ginstance(),
        table in arb_finstance(),
        tree in arb_winstance(),
    ) {
        instance_membership_agrees(
            &law_plan(&case, InstanceShapeKind::Graph),
            &graph,
            &graph_carrier(&graph),
        );
        instance_membership_agrees(
            &law_plan(&case, InstanceShapeKind::Table),
            &table,
            &table_carrier(&table),
        );
        instance_membership_agrees(
            &law_plan(&case, InstanceShapeKind::Tree),
            &tree,
            &tree_carrier(&tree),
        );
    }
}

/// The same agreement on every instance of the bounded scopes.
#[test]
fn instance_subjects_agree_with_enumerated_instances_on_every_bounded_instance() {
    let case = bounded_schema();
    let graph_plan = law_plan(&case, InstanceShapeKind::Graph);
    for graph in every_bounded_graph() {
        instance_membership_agrees(&graph_plan, &graph, &graph_carrier(&graph));
    }
    let tree_plan = law_plan(&case, InstanceShapeKind::Tree);
    for tree in every_bounded_tree() {
        instance_membership_agrees(&tree_plan, &tree, &tree_carrier(&tree));
    }
    let table_plan = law_plan(&bounded_table_schema(), InstanceShapeKind::Table);
    for table in every_bounded_table() {
        instance_membership_agrees(&table_plan, &table, &table_carrier(&table));
    }
}

/// `graph` with every element a required diagnostic names removed (a node
/// with its incident edges), repeatedly, until it passes: a passing graph
/// drawn from the generator.
fn passing_core_graph(
    plan: &ValidationPlan,
    graph: &GInstance,
    external: &ExternalVerdicts,
) -> GInstance {
    let mut graph = graph.clone();
    loop {
        let report = plan.validate_graph(&graph, &ValidationBudget::default(), external);
        if report.status == CheckStatus::Passed {
            return graph;
        }
        let mut named: BTreeSet<ViewRef> = report
            .diagnostics
            .iter()
            .filter(|d| d.requirement == Requirement::Required)
            .flat_map(|d| d.subject.iter().chain(&d.elements).cloned())
            .collect();
        named.extend(
            report
                .incomplete
                .iter()
                .filter(|i| i.requirement == Requirement::Required)
                .filter_map(|i| i.subject.clone()),
        );
        if named.is_empty() {
            // Required whole-instance incompleteness with no element to
            // remove: the empty graph.
            return GInstance::new();
        }
        for element in named {
            match element {
                ViewRef::Edge(id) => {
                    let _ = graph.remove_edge(id);
                }
                ViewRef::Node(n) => {
                    while let Some(&e) = graph
                        .out_edges(n)
                        .first()
                        .or_else(|| graph.in_edges(n).first())
                    {
                        if graph.remove_edge(e).is_err() {
                            break;
                        }
                    }
                    let _ = graph.remove_node(n);
                }
                _ => {}
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// On a `Passed` baseline the two admission policies admit exactly the
    /// same edits.
    #[test]
    fn admission_policies_agree_on_a_passed_baseline(
        case in arb_validation_schema(),
        graph in arb_ginstance(),
        ops in proptest::collection::vec(arb_graph_op(&GRAPH_FAMILIES), 1..5),
        choices in arb_choices(),
        budget in arb_baseline_budget(),
    ) {
        let plan = law_plan(&case, InstanceShapeKind::Graph);
        let external = projection(&choices, graph_elements(&graph));
        let base = passing_core_graph(&plan, &graph, &external);
        let delta = graph_delta_of(&base, &ops);
        let (validated, report) =
            ValidatedGraph::new(plan, base, &ValidationBudget::default(), &external);
        prop_assert_eq!(report.status, CheckStatus::Passed);
        let mut admitted = Vec::new();
        for policy in [AdmissionPolicy::RequirePassed, AdmissionPolicy::NoNewFailures] {
            let mut copy = validated.clone();
            let result = copy.apply_edit(&GraphEdit::Delta(delta.clone()), policy, &budget, &external);
            admitted.push(match result {
                Ok(witness) => Some(witness.canonical_bytes()),
                Err(EditRejection::NotAdmitted { .. }) => None,
                Err(EditRejection::Apply(_)) => return Ok(()),
                Err(other) => return Err(TestCaseError::fail(other.to_string())),
            });
        }
        prop_assert_eq!(admitted[0].is_some(), admitted[1].is_some());
    }
}

/// Moving a child to the end of its own parent's children changes no arc's
/// parent, child or label, only the order of the parent's children; a key
/// whose value is the first value along a path reads that order, so the
/// edit reruns it and exposes the duplicate the new order makes.
#[test]
fn reordering_a_parents_children_reruns_what_reads_their_order() -> TestResult {
    use validation_support::{e_items, e_name};
    let case = bounded_schema();
    let plan = law_plan(&case, InstanceShapeKind::Tree);
    let name = |id: u32, text: &str| {
        Node::new(id, "name").with_value(FieldPresence::Present(Value::Str(text.into())))
    };
    let tree = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "doc")),
            (1, Node::new(1, "item")),
            (2, name(2, "a")),
            (3, name(3, "b")),
            (4, Node::new(4, "item")),
            (5, name(5, "b")),
        ]),
        vec![
            (0, 1, e_items()),
            (1, 2, e_name()),
            (1, 3, e_name()),
            (0, 4, e_items()),
            (4, 5, e_name()),
        ],
        Vec::new(),
        0,
        "doc".into(),
    );
    let edit = TreeEdit::MoveSubtree {
        node_id: 2,
        new_parent: 1,
        edge: e_name(),
    };
    let none = ExternalVerdicts::default();
    let (validated, baseline) = ValidatedTree::new(
        plan.clone(),
        tree.clone(),
        &ValidationBudget::default(),
        &none,
    );
    assert!(
        !baseline
            .diagnostics
            .iter()
            .any(|d| matches!(d.violation, Violation::DuplicateKey { .. })),
        "{:#?}",
        baseline.diagnostics
    );
    let checked = validated.check_edit(&edit, &ValidationBudget::default(), &none)?;
    assert!(
        checked
            .report
            .diagnostics
            .iter()
            .any(|d| matches!(d.violation, Violation::DuplicateKey { .. })),
        "{:#?}",
        checked.report.diagnostics
    );
    assert_v3_v4(
        &plan,
        &tree,
        &edit,
        &ValidationBudget::default(),
        &none,
        &none,
    )
}

/// Initialization is whole-instance validation: the report `Validated::new`
/// returns, and the one its state reconstructs, have the bytes of the shape's
/// validator under the same budget and verdicts, and the state reattaches.
fn initialization_is_whole_validation<I: Surface>(
    plan: &Arc<ValidationPlan>,
    instance: &I,
    budget: &ValidationBudget,
    external: &ExternalVerdicts,
) -> Result<(), TestCaseError> {
    let whole = instance.validate(plan, budget, external);
    let (validated, report) = Validated::<I>::new(plan.clone(), instance.clone(), budget, external);
    prop_assert_eq!(report.canonical_bytes(), whole.canonical_bytes());
    prop_assert_eq!(
        validated.report().canonical_bytes(),
        whole.canonical_bytes()
    );
    let (instance, state) = validated.into_parts();
    let reattached = Validated::<I>::from_parts(plan.clone(), instance, state);
    prop_assert!(reattached.is_ok(), "{:?}", reattached.err());
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn initialization_agrees_with_whole_validation(
        case in arb_validation_schema(),
        graph in arb_ginstance(),
        table in arb_finstance(),
        tree in arb_winstance(),
        budget in arb_baseline_budget(),
        choices in arb_choices(),
    ) {
        initialization_is_whole_validation(
            &law_plan(&case, InstanceShapeKind::Graph),
            &graph,
            &budget,
            &projection(&choices, graph_elements(&graph)),
        )?;
        initialization_is_whole_validation(
            &law_plan(&case, InstanceShapeKind::Table),
            &table,
            &budget,
            &projection(&choices, table_elements(&table)),
        )?;
        initialization_is_whole_validation(
            &law_plan(&case, InstanceShapeKind::Tree),
            &tree,
            &budget,
            &projection(&choices, tree_elements(&tree)),
        )?;
    }
}

/// The same agreement on every bounded instance, under every budget from
/// zero reads to one past the unlimited run's charge, and every bounded
/// projection.
#[test]
fn initialization_agrees_with_whole_validation_on_every_bounded_instance() {
    fn check<I: Surface>(plan: &Arc<ValidationPlan>, instance: &I) {
        for choices in BOUNDED_PROJECTIONS {
            let external = projection(choices, instance.elements());
            for limit in (0..12).map(Some).chain([None]) {
                initialization_is_whole_validation(
                    plan,
                    instance,
                    &ValidationBudget { max_reads: limit },
                    &external,
                )
                .unwrap();
            }
        }
    }
    let case = bounded_schema();
    let graph_plan = law_plan(&case, InstanceShapeKind::Graph);
    for graph in every_bounded_graph() {
        check(&graph_plan, &graph);
    }
    let tree_plan = law_plan(&case, InstanceShapeKind::Tree);
    for tree in every_bounded_tree() {
        check(&tree_plan, &tree);
    }
    let table_plan = law_plan(&bounded_table_schema(), InstanceShapeKind::Table);
    for table in every_bounded_table() {
        check(&table_plan, &table);
    }
}

/// A component of an element's value that a change can alter and a check can
/// depend on (the components of the trigger completeness lemma).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Component {
    /// A node or row is present.
    NodeExists,
    /// A node's anchor or a row's table.
    Anchor,
    /// A node's shape, discriminator, position or annotations.
    Meta,
    /// A node's own value.
    Value,
    /// A node's fields or a row's cells.
    Fields,
    /// An edge, arc or pair is present (with its endpoints).
    EdgeExists,
    /// An edge's or arc's label.
    Label,
    /// An edge's position.
    Position,
    /// The order of a tree node's children or parents.
    Order,
    /// A tree's recorded parent of a node.
    ParentMap,
    /// A tree's fans.
    Fans,
}

/// The components each element kind of a delta can change.
fn changed_components(kind: panproto_inst::validation::DeltaElementKind) -> BTreeSet<Component> {
    use Component as C;
    use panproto_inst::validation::DeltaElementKind as D;
    BTreeSet::from_iter(match kind {
        D::AddNode | D::DelNode | D::AddRow | D::DelRow => vec![C::NodeExists],
        D::AddEdge | D::DelEdge | D::AddPair | D::DelPair => vec![C::EdgeExists],
        D::AttrValue => vec![C::Value],
        D::AttrField | D::Cell | D::SetField | D::RemoveField => vec![C::Fields],
        D::AttrMeta => vec![C::Meta],
        D::AttrAnchor | D::Table | D::RelabelNode => vec![C::Anchor],
        D::AttrEdgeLabel => vec![C::Label],
        D::AttrEdgePosition => vec![C::Position],
        D::InsertNode | D::DeleteNode => {
            vec![C::NodeExists, C::EdgeExists, C::Order, C::ParentMap]
        }
        D::ContractNode | D::JoinFeatures => vec![
            C::NodeExists,
            C::EdgeExists,
            C::Order,
            C::ParentMap,
            C::Label,
            C::Fans,
            C::Anchor,
            C::Value,
            C::Fields,
            C::Meta,
        ],
        D::MoveSubtree => vec![C::EdgeExists, C::Label, C::Order, C::ParentMap],
        D::InsertFan | D::DeleteFan => vec![C::Fans],
    })
}

/// The components the outcome and the read sequence of a persistent
/// instance of each kind depend on, read off its evaluator: a component that
/// only decides whether the instance exists (which makes it created or
/// retired, not rerun) is not listed, and neither is the external verdict,
/// whose change forces a rerun of its own.
fn read_components(kind: CheckKind, shape: InstanceShapeKind) -> BTreeSet<Component> {
    use CheckKind as K;
    use Component as C;
    BTreeSet::from_iter(match kind {
        // A tree fan's reasons depend on which of its parent and children
        // are nodes and on the fan records; a graph fan node's on its shape,
        // its anchor (the hyper-edge) and its incident edges.
        K::FanWellFormed if shape == InstanceShapeKind::Tree => vec![C::NodeExists, C::Fans],
        K::FanWellFormed => vec![C::EdgeExists, C::Label, C::Meta, C::Anchor],
        K::SchemaWellFormed | K::DeclarationInterpretable | K::External => vec![],
        K::NodeAnchor => vec![C::Anchor, C::Meta],
        K::ReservedVocabulary => vec![C::Meta, C::Label],
        K::EdgeDeclared => vec![C::Label],
        K::EndpointCoherence => vec![C::Label, C::Anchor],
        K::RequiredEdge | K::OutDegree | K::Acyclic => vec![C::EdgeExists, C::Label],
        K::PositionDistinct => vec![C::EdgeExists, C::Label, C::Position],
        // Which fields of a node are value sites depends on its anchor and
        // on the labels of its out-edges; a key part's value also on
        // positions and the order of children.
        K::ValueConstraint => vec![C::Anchor, C::Value, C::Fields, C::EdgeExists, C::Label],
        K::RequiredValue => vec![
            C::Anchor,
            C::Value,
            C::Fields,
            C::EdgeExists,
            C::Label,
            C::Position,
            C::Order,
        ],
        // The declarations an element's fields are checked against are its
        // anchor's.
        K::Attribute => vec![C::Anchor, C::Fields],
        K::TreeRoot => vec![C::NodeExists],
        K::TreeParentMap => vec![C::ParentMap, C::EdgeExists, C::Order],
        K::TreeReachable => vec![C::EdgeExists, C::Order],
        K::KeyUnique | K::ReferenceResolves => vec![
            C::NodeExists,
            C::Anchor,
            C::Meta,
            C::Value,
            C::Fields,
            C::EdgeExists,
            C::Label,
            C::Position,
            C::Order,
        ],
    })
}

/// The trigger completeness lemma's inclusion: every check kind that reads a
/// component an element kind can change is in that element kind's trigger
/// row, for the element kinds of the shapes the kind applies to.
#[test]
fn trigger_rows_cover_every_kind_that_reads_a_changed_component() {
    use panproto_inst::validation::DeltaElementKind as D;
    let kinds = [
        CheckKind::SchemaWellFormed,
        CheckKind::DeclarationInterpretable,
        CheckKind::NodeAnchor,
        CheckKind::ReservedVocabulary,
        CheckKind::EdgeDeclared,
        CheckKind::EndpointCoherence,
        CheckKind::FanWellFormed,
        CheckKind::RequiredEdge,
        CheckKind::OutDegree,
        CheckKind::PositionDistinct,
        CheckKind::ValueConstraint,
        CheckKind::RequiredValue,
        CheckKind::Attribute,
        CheckKind::TreeRoot,
        CheckKind::TreeParentMap,
        CheckKind::KeyUnique,
        CheckKind::ReferenceResolves,
        CheckKind::Acyclic,
        CheckKind::TreeReachable,
        CheckKind::External,
    ];
    let applies = |k: CheckKind, shape: InstanceShapeKind| match k {
        CheckKind::TreeRoot
        | CheckKind::TreeParentMap
        | CheckKind::TreeReachable
        | CheckKind::ReservedVocabulary => shape == InstanceShapeKind::Tree,
        CheckKind::PositionDistinct | CheckKind::Acyclic => shape == InstanceShapeKind::Graph,
        CheckKind::FanWellFormed => shape != InstanceShapeKind::Table,
        _ => true,
    };
    let mut missing = Vec::new();
    for change in D::ALL {
        let shape = if is_tree_change(change) {
            InstanceShapeKind::Tree
        } else if matches!(
            change,
            D::AddRow | D::DelRow | D::AddPair | D::DelPair | D::Cell | D::Table
        ) {
            InstanceShapeKind::Table
        } else {
            InstanceShapeKind::Graph
        };
        for kind in kinds {
            if applies(kind, shape)
                && !read_components(kind, shape).is_disjoint(&changed_components(change))
                && !change.triggers().contains(&kind)
            {
                missing.push((change, kind));
            }
        }
    }
    assert!(missing.is_empty(), "{missing:?}");
}

const fn is_tree_change(change: panproto_inst::validation::DeltaElementKind) -> bool {
    use panproto_inst::validation::DeltaElementKind as D;
    matches!(
        change,
        D::InsertNode
            | D::DeleteNode
            | D::ContractNode
            | D::RelabelNode
            | D::SetField
            | D::RemoveField
            | D::MoveSubtree
            | D::InsertFan
            | D::DeleteFan
            | D::JoinFeatures
    )
}

// ---------------------------------------------------------------------------
// The admission policies against their definitions. The predicates below
// are written from the policies' definitions over abstract bundle vectors
// and call nothing in the crate; the production side is
// `AdmissionPolicy::admits` over
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
    let decl = |name: &str, requirement| ExternalCheckDecl {
        name: name.into(),
        anchors: BTreeSet::from(["item".into()]),
        requirement,
    };
    let options = ValidationOptions {
        external: BTreeMap::from([
            ("audit".into(), decl("audit", Requirement::Required)),
            ("notes".into(), decl("notes", Requirement::Advisory)),
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
    schedule: &[CheckInstance],
    skipped: &[CheckInstance],
    state: &Abstract,
) -> BTreeMap<CheckInstance, OpenVerdict> {
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
    let mut open = BTreeMap::new();
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
fn realize(plan: &ValidationPlan, schedule: &[CheckInstance], state: &Abstract) -> ValidationState {
    let skipped: Vec<CheckInstance> = state
        .cut
        .map_or(Vec::new(), |(from, _)| schedule[from..].to_vec());
    let open = realize_open(plan, schedule, &skipped, state);
    let mut budget_cuts = BTreeMap::new();
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
    ValidationState {
        plan: *plan.digest(),
        seq: 0,
        status: aggregate_status(&diagnostics, &incomplete),
        open,
        budget_cuts,
        coverage: BTreeMap::new(),
        external: ExternalVerdicts::default(),
        key_indexes: BTreeMap::new(),
        reference_indexes: BTreeMap::new(),
    }
}

/// Every abstract state of the scope: with no cut, each required bundle
/// passed, failed or incomplete with either byte value, the advisory note
/// present or not, and the advisory bundle passed or failed; with a cut
/// from each position of the four-instance schedule and either of two
/// spent counts, the same choices for the instances before the cut.
fn every_abstract_state(schedule: &[CheckInstance], plan: &ValidationPlan) -> Vec<Abstract> {
    let evaluated = [
        Outcome::Passed,
        Outcome::Failed(0),
        Outcome::Failed(1),
        Outcome::Incomplete(0),
        Outcome::Incomplete(1),
    ];
    let is_required = |instance: &CheckInstance| {
        plan.checks()
            .get(&instance.check)
            .is_some_and(|spec| spec.requirement == Requirement::Required)
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

/// The production admission (`AdmissionPolicy::admits` under each policy)
/// agrees with the two predicates on every baseline and every result of
/// the scope: three required bundles, each passed, failed or incomplete
/// with one of two canonical byte values, with and without a budget cut
/// (from each of four schedule positions, with two spent counts), with an
/// advisory diagnostic inside a required bundle or not and an advisory
/// bundle passed or failed. On every `Passed` baseline the two policies
/// admit the same results.
#[test]
fn admission_policies_agree_with_their_predicates_on_three_bundles() {
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
    .collect::<BTreeSet<_>>()
    .into_iter()
    .collect();
    assert_eq!(
        schedule.last().map(|instance| &instance.check),
        Some(&advisory_key),
        "the advisory instance is scheduled last"
    );
    let states = every_abstract_state(&schedule, &plan);
    let realized: Vec<ValidationState> = states
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
            let require = AdmissionPolicy::RequirePassed.admits(&plan, real_baseline, real_result);
            let tolerate = AdmissionPolicy::NoNewFailures.admits(&plan, real_baseline, real_result);
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
