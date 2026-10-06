//! Validation transition and retained-heap benchmarks.
//!
//! Each `retained_*_heap_bytes` benchmark constructs its source state outside
//! the measured closure, clones one owned state component inside it, and returns
//! the clone. Divan records allocation-layout bytes before deferring the returned
//! value's drop until after the sample. The retained heap per iteration is
//! therefore `alloc + grow - dealloc - shrink` in Divan's byte rows. Building
//! and validating the fixture is excluded, so transient validation allocations
//! cannot obscure the much smaller retained balance through rounded output.
//! The balance includes collection nodes and spare capacity, plus allocations
//! copied by `Clone`. It excludes top-level inline bytes stored in Divan's
//! preallocated output slot and allocator-internal metadata. The complete-state
//! measurement already contains the index and external-projection components,
//! so the three measurements overlap and must not be added.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use divan::counter::ItemsCount;
use divan::{AllocProfiler, Bencher};
use panproto_inst::validation::{
    AdmissionPolicy, ExternalCheckDecl, ExternalVerdict, ExternalVerdicts, Requirement,
    ValidatedGraph, ValidationBudget, ValidationOptions, ValidationPlan,
};
use panproto_inst::{Digest, EdgeId, FieldPresence, GInstance, GraphEdit, Node, Value, ViewRef};
use panproto_schema::{
    Cardinality, Edge, InstanceShapeKind, Key, KeyPart, KeyRole, KeyScope, OnUnresolved, Protocol,
    RefSource, Reference, ResolveMode, Schema, Vertex,
};

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

fn fixture(size: u32) -> (Arc<ValidationPlan>, GInstance, GraphEdit) {
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
    let protocol = Protocol {
        syntactic_shape: InstanceShapeKind::Graph,
        semantic_shape: InstanceShapeKind::Graph,
        ..Protocol::default()
    };
    let plan = Arc::new(ValidationPlan::compile(
        &schema,
        &protocol,
        &ValidationOptions::default(),
    ));
    let mut graph = GInstance::new();
    for id in 0..size {
        graph = graph.with_node(Node::new(id, "item"));
    }
    let edit = GraphEdit::RelabelNode {
        node: 0,
        old_anchor: "item".into(),
        new_anchor: "renamed".into(),
    };
    (plan, graph, edit)
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

fn indexed_fixture(size: u32) -> (Arc<ValidationPlan>, GInstance, GraphEdit, ExternalVerdicts) {
    let id = Edge {
        src: "record".into(),
        tgt: "record.id".into(),
        kind: "prop".into(),
        name: Some("id".into()),
    };
    let resolves = Edge {
        src: "record".into(),
        tgt: "record".into(),
        kind: "ref".into(),
        name: Some("self".into()),
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
    declare_edge(&mut schema, id.clone());
    declare_edge(&mut schema, resolves.clone());
    schema.declarations.insert_key(
        "record".into(),
        Key {
            id: "primary".into(),
            parts: vec![KeyPart::Path(vec![id.clone()])],
            role: KeyRole::Primary,
            scope: KeyScope::Document,
        },
    );
    schema.declarations.insert_reference(Reference {
        id: "self".into(),
        holder: "record".into(),
        source: RefSource::KeyValue {
            parts: vec![vec![id.clone()]],
        },
        target_vertex: "record".into(),
        target_key: Some("primary".into()),
        edge: Some(resolves.clone()),
        cardinality: Cardinality::One,
        nullable: false,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
    });
    let options = ValidationOptions {
        external: BTreeMap::from([(
            "audit".into(),
            ExternalCheckDecl {
                name: "audit".into(),
                anchors: BTreeSet::from(["record".into()]),
                requirement: Requirement::Required,
            },
        )]),
        ..ValidationOptions::default()
    };
    let plan = Arc::new(ValidationPlan::compile(
        &schema,
        &Protocol {
            syntactic_shape: InstanceShapeKind::Graph,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        },
        &options,
    ));
    let mut graph = GInstance::new();
    let mut external = ExternalVerdicts::default();
    for record in 0..size {
        let holder = record * 2;
        let leaf = holder + 1;
        graph = graph
            .with_node(Node::new(holder, "record"))
            .with_node(
                Node::new(leaf, "record.id")
                    .with_value(FieldPresence::Present(Value::Int(i64::from(record)))),
            )
            .with_edge_id(EdgeId(record), holder, leaf, id.clone())
            .with_edge_id(EdgeId(size + record), holder, holder, resolves.clone());
        external.insert(
            "audit".into(),
            ViewRef::Node(holder),
            ExternalVerdict {
                passed: true,
                witness: Digest([record.to_le_bytes()[0]; 32]),
            },
        );
    }
    let edit = GraphEdit::SetValue {
        node: 1,
        old: Some(FieldPresence::Present(Value::Int(0))),
        new: Some(FieldPresence::Present(Value::Int(i64::from(size)))),
    };
    (plan, graph, edit, external)
}

#[divan::bench(args = [100, 500, 1_000])]
fn full_graph_local_recheck(bencher: Bencher, size: u32) {
    let (plan, graph, edit) = fixture(size);
    let budget = ValidationBudget::default();
    let external = ExternalVerdicts::default();
    let mut target = graph;
    edit.apply(&mut target)
        .unwrap_or_else(|error| panic!("the fixture edit applies: {error}"));
    bencher.bench(|| plan.validate_graph(&target, &budget, &external));
}

#[divan::bench(name = "incremental_graph_local_recheck_scheduled_runs_1", args = [100, 500, 1_000])]
fn incremental_graph_local_recheck(bencher: Bencher, size: u32) {
    let (plan, graph, edit) = fixture(size);
    let budget = ValidationBudget::default();
    let external = ExternalVerdicts::default();
    let (validated, _) = ValidatedGraph::new(plan, graph, &budget, &external);
    let footprint = validated
        .footprint(&edit, &external)
        .unwrap_or_else(|error| panic!("the fixture edit applies: {error}"));
    let scheduled = footprint.rerun.len() + footprint.created.len();
    assert_eq!(
        scheduled, 1,
        "the GraphLocal fixture must schedule exactly one run at every size"
    );
    bencher
        .counter(ItemsCount::from(scheduled))
        .with_inputs(|| validated.clone())
        .bench_values(|mut value| {
            let witness = value
                .apply_edit(&edit, AdmissionPolicy::RequirePassed, &budget, &external)
                .unwrap_or_else(|error| panic!("the fixture edit is admitted: {error}"));
            (value, witness)
        });
}

#[divan::bench(args = [100, 500, 1_000])]
fn full_maintained_global_recheck(bencher: Bencher, size: u32) {
    let (plan, graph, edit, external) = indexed_fixture(size);
    let budget = ValidationBudget::default();
    let mut target = graph;
    edit.apply(&mut target)
        .unwrap_or_else(|error| panic!("the fixture edit applies: {error}"));
    bencher.bench(|| plan.validate_graph(&target, &budget, &external));
}

#[divan::bench(name = "incremental_maintained_global_apply_scheduled_runs_size_plus_2", args = [100, 500, 1_000])]
fn incremental_maintained_global_apply(bencher: Bencher, size: u32) {
    let (plan, graph, edit, external) = indexed_fixture(size);
    let budget = ValidationBudget::default();
    let (validated, _) = ValidatedGraph::new(plan, graph, &budget, &external);
    let footprint = validated
        .footprint(&edit, &external)
        .unwrap_or_else(|error| panic!("the fixture edit applies: {error}"));
    let scheduled = footprint.rerun.len() + footprint.created.len();
    let expected = usize::try_from(size)
        .unwrap_or_else(|_| panic!("the benchmark size {size} fits usize"))
        + 2;
    assert_eq!(
        scheduled, expected,
        "the maintained-global fixture schedules one reference run per record plus its key and acyclic runs"
    );
    bencher
        .counter(ItemsCount::from(scheduled))
        .with_inputs(|| validated.clone())
        .bench_values(|mut value| {
            let witness = value
                .apply_edit(&edit, AdmissionPolicy::RequirePassed, &budget, &external)
                .unwrap_or_else(|error| panic!("the fixture edit is admitted: {error}"));
            (value, witness)
        });
}

fn retained_state(size: u32) -> panproto_inst::validation::ValidationState {
    let (plan, graph, _, external) = indexed_fixture(size);
    let (validated, _) = ValidatedGraph::new(plan, graph, &ValidationBudget::default(), &external);
    let (_, state) = validated.into_parts();
    state
}

#[divan::bench(args = [100, 1_000, 2_500], sample_size = 1)]
fn retained_state_heap_bytes(bencher: Bencher, size: u32) {
    let state = retained_state(size);
    bencher.bench(|| state.clone());
}

#[divan::bench(args = [100, 1_000, 2_500], sample_size = 1)]
fn retained_indexes_heap_bytes(bencher: Bencher, size: u32) {
    let probe = retained_state(size);
    assert!(
        probe
            .key_indexes
            .values()
            .any(|index| !index.values.is_empty()),
        "the retained-index fixture must build a nonempty key index"
    );
    assert!(
        probe
            .reference_indexes
            .values()
            .any(|index| !index.values.is_empty()),
        "the retained-index fixture must build a nonempty reference index"
    );
    bencher.bench(|| (probe.key_indexes.clone(), probe.reference_indexes.clone()));
}

#[divan::bench(args = [100, 1_000, 2_500], sample_size = 1)]
fn retained_external_projection_heap_bytes(bencher: Bencher, size: u32) {
    let state = retained_state(size);
    bencher.bench(|| state.external.clone());
}
