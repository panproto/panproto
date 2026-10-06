//! The validation corpus: its cases, and writing them out.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::path::{Path, PathBuf};

use panproto_gat::Name;
use panproto_inst::validation::{
    AdmissionPolicy, EditValidation, EditWitness, ExternalCheckDecl, ExternalVerdict,
    ExternalVerdicts, Requirement, SchemaRef, SourceLocation, ValidationBudget, ValidationOptions,
    ValidationPlan, ValidationReport, Violation, WitnessOptions,
};
use panproto_inst::{
    BoundaryProtocols, BoundarySide, CompiledMigration, Digest, EdgeId, EditRejection, FInstance,
    Fan, FieldPresence, GEdge, GInstance, GraphEdit, HYPER_LABEL_KIND, HYPER_PARENT_KIND, Node,
    NodeShape, RestrictError, RowId, TableEdit, TreeEdit, ValidatedGraph, ValidatedShape, Value,
    ViewRef, WInstance, from_node_link_json, functor_extend, functor_restrict, graph_extend,
    graph_restrict, to_node_link_json, wtype_extend, wtype_restrict,
};
use panproto_schema::{
    Cardinality, Constraint, ConstraintSemantics, Edge, HyperEdge, InstanceShapeKind, Key, KeyPart,
    KeyRole, KeyScope, LengthUnit, OnUnresolved, Protocol, RefSource, Reference, ResolveMode,
    Schema, Vertex,
};
use serde::Serialize;

/// What building a fixture may fail with.
type R<T> = Result<T, Box<dyn Error>>;

/// The instance of a fixture, in its shape.
#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum Instance {
    /// A graph.
    Graph(GInstance),
    /// A tree.
    Tree(WInstance),
    /// A table instance.
    Table(FInstance),
}

/// The edits applied to a fixture's instance after it is validated, in
/// order, as the edits of the instance's shape.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum EditTrace {
    /// Edits of a graph.
    Graph(Vec<GraphEdit>),
    /// Edits of a tree.
    Tree(Vec<TreeEdit>),
    /// Edits of a table instance.
    Table(Vec<TableEdit>),
}

impl EditTrace {
    /// The trace of no edits, in the shape of `instance`.
    const fn none_for(instance: &Instance) -> Self {
        match instance {
            Instance::Graph(_) => Self::Graph(Vec::new()),
            Instance::Tree(_) => Self::Tree(Vec::new()),
            Instance::Table(_) => Self::Table(Vec::new()),
        }
    }

    /// The number of edits.
    fn len(&self) -> usize {
        match self {
            Self::Graph(edits) => edits.len(),
            Self::Tree(edits) => edits.len(),
            Self::Table(edits) => edits.len(),
        }
    }

    /// Apply edit `k` to `instance`, an instance of the trace's shape.
    fn apply(&self, k: usize, instance: &mut Instance) -> R<Result<(), String>> {
        let applied = match (self, instance) {
            (Self::Graph(edits), Instance::Graph(g)) => edits[k].apply(g),
            (Self::Tree(edits), Instance::Tree(t)) => edits[k].apply(t),
            (Self::Table(edits), Instance::Table(f)) => edits[k].apply(f),
            _ => return Err("an edit trace of another shape than its instance".into()),
        };
        Ok(applied.map_err(|e| e.to_string()))
    }
}

/// What one edit of a trace did.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum EditOutcome {
    /// The edit applied, and validating the instance after it gives this
    /// report.
    Applied(ValidationReport),
    /// Applying the edit failed with this error, and the instance is left
    /// as it was.
    Rejected(String),
}

/// A restrict or extend operation represented by a boundary fixture.
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum BoundaryOperation {
    GraphRestrict,
    GraphExtend,
    TreeRestrict,
    TreeExtend,
    TableRestrict,
    TableExtend,
}

/// The expected result of one boundary operation.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum BoundaryOutcome {
    Passed(Instance),
    Validation {
        side: BoundarySide,
        report: ValidationReport,
    },
    Rejected(String),
}

/// The target side and expected result of one boundary operation. The
/// fixture's top-level schema and instance are its source side and input.
#[derive(Serialize)]
struct BoundaryTransition {
    operation: BoundaryOperation,
    target_schema: Schema,
    migration: BoundaryMigration,
    outcome: BoundaryOutcome,
}

/// The canonical subset of a migration used by the boundary corpus.
#[derive(Serialize)]
struct BoundaryMigration {
    surviving_verts: BTreeSet<Name>,
    surviving_edges: BTreeSet<Edge>,
    vertex_remap: BTreeMap<Name, Name>,
    edge_remap: BTreeMap<Edge, Edge>,
}

impl BoundaryMigration {
    fn from_compiled(migration: &CompiledMigration) -> Self {
        Self {
            surviving_verts: migration.surviving_verts.iter().cloned().collect(),
            surviving_edges: migration.surviving_edges.iter().cloned().collect(),
            vertex_remap: migration
                .vertex_remap
                .iter()
                .map(|(source, target)| (source.clone(), target.clone()))
                .collect(),
            edge_remap: migration
                .edge_remap
                .iter()
                .map(|(source, target)| (source.clone(), target.clone()))
                .collect(),
        }
    }
}

/// The expected incremental result of one Train-Benchmark repair.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum IncrementalOutcome {
    Applied {
        validation: EditValidation,
        witness: EditWitness,
    },
    NotAdmitted(EditValidation),
    ApplyError(String),
}

/// A fully replayable incremental probe stored beside a raw edit trace.
#[derive(Serialize)]
struct IncrementalProbe {
    edit: GraphEdit,
    policy: AdmissionPolicy,
    budget: ValidationBudget,
    external: ExternalVerdicts,
    options: WitnessOptions,
    outcome: IncrementalOutcome,
}

/// One expected violation tuple. A vector of these retains multiplicity.
#[derive(Serialize)]
struct ViolationTuple {
    check: panproto_inst::validation::CheckKind,
    subject: Option<ViewRef>,
    violation: Violation,
}

/// Initial and per-edit violation tuples for one generated sequence.
#[derive(Serialize)]
struct ViolationSequence {
    initial: Vec<ViolationTuple>,
    after_edits: Vec<Vec<ViolationTuple>>,
}

/// One validation case as it is written to disk.
#[derive(Serialize)]
struct Fixture {
    /// The case's path in the corpus, without extension.
    name: String,
    /// The protocol's name.
    protocol: String,
    /// The protocol, when its name is not a registered protocol's.
    protocol_definition: Option<Protocol>,
    /// The schema.
    schema: Schema,
    /// The validation options.
    options: ValidationOptions,
    /// The budget.
    budget: ValidationBudget,
    /// The external verdicts.
    external: ExternalVerdicts,
    /// The instance.
    instance: Instance,
    /// The edits applied to the instance after it is validated, in order;
    /// empty in a case that validates the instance alone.
    edit_trace: EditTrace,
    /// The report validation gives the instance.
    report: ValidationReport,
    /// What each edit of the trace did, in order: the report validating the
    /// instance after it gives, or the error applying it gives.
    edit_outcomes: Vec<EditOutcome>,
    /// Restrict or extend boundary operation, only for boundary fixtures.
    #[serde(skip_serializing_if = "Option::is_none")]
    boundary: Option<BoundaryTransition>,
    /// Incremental repair and witness, only for Train-Benchmark fixtures.
    #[serde(skip_serializing_if = "Option::is_none")]
    incremental: Option<IncrementalProbe>,
    /// Expected diagnostic tuples with multiplicity, only for the train corpus.
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_violations: Option<ViolationSequence>,
}

/// One instance that decoding rejects, as one encoding's file holds it.
#[derive(Serialize)]
struct Rejection<'a> {
    /// The case's path in the corpus, without extension.
    name: &'a str,
    /// The shape the instance would have.
    shape: InstanceShapeKind,
    /// The instance's wire form in the file's encoding.
    instance: &'a ciborium::Value,
    /// The error decoding it gives, in every encoding.
    error: &'a str,
}

/// One instance that decoding rejects, in every form it is written in.
struct RejectionCase {
    /// The case's path in the corpus, without extension.
    name: String,
    /// The shape the instance would have.
    shape: InstanceShapeKind,
    /// The instance's wire form in each encoding.
    forms: WireForms,
    /// The error decoding it gives, in every encoding.
    error: String,
    /// The instance's node-link JSON form, for a graph, which
    /// `from_node_link_json` refuses with the same error.
    node_link: Option<serde_json::Value>,
}

impl RejectionCase {
    /// Write the case to `<out>/<name>.json`, `.named.msgpack` and `.cbor`,
    /// each holding the instance's wire form in its own encoding, and, for
    /// a graph, its node-link form to `<out>/<name>.nodelink.json`.
    fn write(&self, out: &Path) -> R<()> {
        let path = |ext: &str| out.join(format!("{}.{ext}", self.name));
        if let Some(dir) = path("json").parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file = |instance| Rejection {
            name: &self.name,
            shape: self.shape,
            instance,
            error: &self.error,
        };
        let mut json = serde_json::to_string_pretty(&file(&self.forms.json))?;
        json.push('\n');
        std::fs::write(path("json"), json)?;
        std::fs::write(
            path("named.msgpack"),
            rmp_serde::to_vec_named(&file(&self.forms.msgpack))?,
        )?;
        let mut cbor = Vec::new();
        ciborium::into_writer(&file(&self.forms.cbor), &mut cbor)?;
        std::fs::write(path("cbor"), cbor)?;
        if let Some(node_link) = &self.node_link {
            write_node_link(out, &self.name, node_link)?;
        }
        Ok(())
    }
}

/// The name of the corpus's own protocols, by the shape their instances are
/// written in.
fn protocol_name(syntactic: InstanceShapeKind) -> String {
    let shape = match syntactic {
        InstanceShapeKind::Tree => "tree",
        InstanceShapeKind::Table => "table",
        InstanceShapeKind::Graph => "graph",
    };
    format!("validation-fixtures-{shape}")
}

/// The corpus's protocol for instances written in shape `syntactic`: a
/// length bound in characters, inclusive value bounds, out-degree bounds, a
/// declared acyclic self-edge, a pattern and a required value; resolved
/// instances are graphs.
fn fixture_protocol(syntactic: InstanceShapeKind) -> Protocol {
    let semantics = [
        (
            "maxLength",
            ConstraintSemantics::MaxLength(LengthUnit::Chars),
        ),
        ("minimum", ConstraintSemantics::Minimum),
        ("maximum", ConstraintSemantics::Maximum),
        ("maxOut", ConstraintSemantics::MaxOutDegree),
        ("minOut", ConstraintSemantics::MinOutDegree),
        ("acyclic", ConstraintSemantics::Acyclic),
        ("pattern", ConstraintSemantics::Pattern),
        ("required", ConstraintSemantics::RequiredValue),
    ];
    Protocol {
        name: protocol_name(syntactic),
        constraint_sorts: semantics.iter().map(|(s, _)| (*s).to_owned()).collect(),
        constraint_semantics: semantics
            .into_iter()
            .map(|(s, m)| (s.to_owned(), m))
            .collect(),
        syntactic_shape: syntactic,
        semantic_shape: InstanceShapeKind::Graph,
        ..Protocol::default()
    }
}

/// An edge of kind `kind` named `name`.
fn edge(src: &str, tgt: &str, kind: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: kind.into(),
        name: Some(name.into()),
    }
}

/// A `prop` edge named `name`.
fn prop(src: &str, tgt: &str, name: &str) -> Edge {
    edge(src, tgt, "prop", name)
}

/// A schema under construction, its adjacency indices kept up to date.
#[derive(Default)]
struct S(Schema);

impl S {
    fn vertex(mut self, id: &str, kind: &str) -> Self {
        self.0.vertices.insert(
            id.into(),
            Vertex {
                id: id.into(),
                kind: kind.into(),
                nsid: None,
            },
        );
        self
    }

    fn edge(mut self, e: &Edge) -> Self {
        self.0.edges.insert(e.clone(), e.kind.clone());
        self.0
            .outgoing
            .entry(e.src.clone())
            .or_default()
            .push(e.clone());
        self.0
            .incoming
            .entry(e.tgt.clone())
            .or_default()
            .push(e.clone());
        self.0
            .between
            .entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
        self
    }

    fn constraint(mut self, vertex: &str, sort: &str, value: &str) -> Self {
        self.0
            .constraints
            .entry(vertex.into())
            .or_default()
            .push(Constraint {
                sort: sort.into(),
                value: value.into(),
            });
        self
    }

    fn required(mut self, vertex: &str, e: &Edge) -> Self {
        self.0
            .required
            .entry(vertex.into())
            .or_default()
            .push(e.clone());
        self
    }

    fn ordered(mut self, e: &Edge) -> Self {
        self.0.orderings.insert(e.clone(), 0);
        self
    }

    fn hyper_edge(mut self, id: &str, signature: &[(&str, &str)], parent: &str) -> Self {
        self.0.hyper_edges.insert(
            id.into(),
            HyperEdge {
                id: id.into(),
                kind: "relation".into(),
                signature: signature
                    .iter()
                    .map(|(l, v)| (Name::from(*l), Name::from(*v)))
                    .collect(),
                parent_label: parent.into(),
            },
        );
        self
    }

    fn key(mut self, vertex: &str, id: &str, parts: &[Edge], role: KeyRole) -> Self {
        self.0.declarations.insert_key(
            vertex.into(),
            Key {
                id: id.into(),
                parts: parts
                    .iter()
                    .map(|e| KeyPart::Path(vec![e.clone()]))
                    .collect(),
                role,
                scope: KeyScope::Global,
            },
        );
        self
    }

    fn build(self) -> Schema {
        self.0
    }
}

/// A node anchored at `anchor` with the string value `value`.
fn valued(id: u32, anchor: &str, value: Value) -> Node {
    Node::new(id, anchor).with_value(FieldPresence::Present(value))
}

/// A table row of the given cells.
fn row(cells: &[(&str, Value)]) -> HashMap<String, Value> {
    cells
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect()
}

/// One validation case before it is validated.
struct Case {
    name: String,
    syntactic: InstanceShapeKind,
    schema: Schema,
    options: ValidationOptions,
    budget: ValidationBudget,
    external: ExternalVerdicts,
    instance: Instance,
    edits: Option<EditTrace>,
    boundary: Option<BoundaryTransition>,
    incremental: Option<IncrementalRequest>,
    record_violation_tuples: bool,
    protocol: Option<Protocol>,
}

/// Inputs from which the generator computes an incremental probe.
struct IncrementalRequest {
    edit: GraphEdit,
    policy: AdmissionPolicy,
    budget: ValidationBudget,
    external: ExternalVerdicts,
    options: WitnessOptions,
}

fn make_incremental_probe(
    plan: &ValidationPlan,
    instance: &Instance,
    initial_budget: &ValidationBudget,
    initial_external: &ExternalVerdicts,
    request: Option<&IncrementalRequest>,
) -> R<Option<IncrementalProbe>> {
    let Some(request) = request else {
        return Ok(None);
    };
    let Instance::Graph(graph) = instance else {
        return Err("incremental fixture probe is not a graph".into());
    };
    let (mut validated, _) = ValidatedGraph::new(
        std::sync::Arc::new(plan.clone()),
        graph.clone(),
        initial_budget,
        initial_external,
    );
    let validation = validated.check_edit(&request.edit, &request.budget, &request.external);
    let outcome = match validated.apply_edit_with_options(
        &request.edit,
        request.policy,
        &request.budget,
        &request.external,
        &request.options,
    ) {
        Ok(witness) => IncrementalOutcome::Applied {
            validation: validation?,
            witness,
        },
        Err(EditRejection::NotAdmitted { validation }) => {
            IncrementalOutcome::NotAdmitted(*validation)
        }
        Err(EditRejection::Apply(error)) => IncrementalOutcome::ApplyError(error.to_string()),
        Err(error) => IncrementalOutcome::ApplyError(error.to_string()),
    };
    Ok(Some(IncrementalProbe {
        edit: request.edit.clone(),
        policy: request.policy,
        budget: request.budget,
        external: request.external.clone(),
        options: request.options,
        outcome,
    }))
}

impl Case {
    /// A case with no options, an unlimited budget and no verdicts, read
    /// under the corpus protocol of its instance's shape.
    fn new(name: impl Into<String>, schema: Schema, instance: Instance) -> Self {
        let syntactic = match &instance {
            Instance::Graph(_) => InstanceShapeKind::Graph,
            Instance::Tree(_) => InstanceShapeKind::Tree,
            Instance::Table(_) => InstanceShapeKind::Table,
        };
        Self {
            name: name.into(),
            syntactic,
            schema,
            options: ValidationOptions::default(),
            budget: ValidationBudget::default(),
            external: ExternalVerdicts::default(),
            instance,
            edits: None,
            boundary: None,
            incremental: None,
            record_violation_tuples: false,
            protocol: None,
        }
    }

    /// The case with `edits` applied to its instance after it is validated.
    fn with_edits(mut self, edits: EditTrace) -> Self {
        self.edits = Some(edits);
        self
    }

    /// Attach a generated restrict or extend boundary result.
    fn with_boundary(mut self, boundary: BoundaryTransition) -> Self {
        self.boundary = Some(boundary);
        self
    }

    /// Attach a generated incremental probe and explicit violation tuples.
    fn with_incremental(mut self, incremental: IncrementalRequest) -> Self {
        self.incremental = Some(incremental);
        self.record_violation_tuples = true;
        self
    }

    /// Use a case-specific protocol instead of the ordinary fixture protocol.
    fn with_protocol(mut self, protocol: Protocol) -> Self {
        self.protocol = Some(protocol);
        self
    }

    /// The fixture: the case, the report validation gives, and what each
    /// edit of its trace does. An edit that fails leaves the instance as it
    /// was for the edits after it.
    fn validate(self) -> R<Fixture> {
        let protocol = self
            .protocol
            .unwrap_or_else(|| fixture_protocol(self.syntactic));
        let plan = ValidationPlan::compile(&self.schema, &protocol, &self.options);
        let validate = |instance: &Instance| match instance {
            Instance::Graph(g) => plan.validate_graph(g, &self.budget, &self.external),
            Instance::Tree(t) => plan.validate_tree(t, &self.budget, &self.external),
            Instance::Table(f) => plan.validate_table(f, &self.budget, &self.external),
        };
        let report = validate(&self.instance);
        let incremental = make_incremental_probe(
            &plan,
            &self.instance,
            &self.budget,
            &self.external,
            self.incremental.as_ref(),
        )?;
        let edit_trace = self
            .edits
            .unwrap_or_else(|| EditTrace::none_for(&self.instance));
        let mut current = self.instance.clone();
        let mut edit_outcomes = Vec::new();
        for k in 0..edit_trace.len() {
            let mut next = current.clone();
            edit_outcomes.push(match edit_trace.apply(k, &mut next)? {
                Ok(()) => {
                    current = next;
                    EditOutcome::Applied(validate(&current))
                }
                Err(error) => EditOutcome::Rejected(error),
            });
        }
        let tuples = |report: &ValidationReport| {
            report
                .diagnostics
                .iter()
                .map(|diagnostic| ViolationTuple {
                    check: diagnostic.check.kind,
                    subject: diagnostic.subject.clone(),
                    violation: diagnostic.violation.clone(),
                })
                .collect()
        };
        let expected_violations = self.record_violation_tuples.then(|| ViolationSequence {
            initial: tuples(&report),
            after_edits: edit_outcomes
                .iter()
                .map(|outcome| match outcome {
                    EditOutcome::Applied(report) => tuples(report),
                    EditOutcome::Rejected(_) => Vec::new(),
                })
                .collect(),
        });
        Ok(Fixture {
            name: self.name,
            protocol: protocol.name.clone(),
            protocol_definition: Some(protocol),
            schema: self.schema,
            options: self.options,
            budget: self.budget,
            external: self.external,
            edit_trace,
            instance: self.instance,
            report,
            edit_outcomes,
            boundary: self.boundary,
            incremental,
            expected_violations,
        })
    }
}

/// A graph of the given nodes and edges, edges numbered in order.
fn graph(nodes: Vec<Node>, edges: &[(u32, u32, &Edge)]) -> GInstance {
    let mut g = GInstance::new();
    for n in nodes {
        g = g.with_node(n);
    }
    for (s, t, e) in edges {
        g = g.with_edge(*s, *t, (*e).clone());
    }
    g
}

/// A tree of the given nodes and arcs, rooted at node 0, with `fans`.
fn tree(nodes: Vec<Node>, arcs: &[(u32, u32, &Edge)], fans: Vec<Fan>) -> WInstance {
    let root_anchor = nodes
        .first()
        .map_or_else(|| Name::from("doc"), |n| n.anchor.clone());
    WInstance::new(
        nodes.into_iter().map(|n| (n.id, n)).collect(),
        arcs.iter()
            .map(|(p, c, e)| (*p, *c, (*e).clone()))
            .collect(),
        fans,
        0,
        root_anchor,
    )
}

fn boundary_outcome<T>(
    result: Result<T, RestrictError>,
    wrap: impl FnOnce(T) -> Instance,
) -> BoundaryOutcome {
    match result {
        Ok(output) => BoundaryOutcome::Passed(wrap(output)),
        Err(RestrictError::Validation { side, report }) => BoundaryOutcome::Validation {
            side,
            report: *report,
        },
        Err(error) => BoundaryOutcome::Rejected(error.to_string()),
    }
}

fn boundary_transition(
    operation: BoundaryOperation,
    source: &Schema,
    target: &Schema,
    migration: &CompiledMigration,
    input: &Instance,
    protocol: &Protocol,
) -> R<BoundaryTransition> {
    let protocols = BoundaryProtocols {
        src: protocol,
        tgt: protocol,
    };
    let outcome = match (operation, input) {
        (BoundaryOperation::GraphRestrict, Instance::Graph(graph)) => boundary_outcome(
            graph_restrict(graph, source, target, migration, protocols),
            Instance::Graph,
        ),
        (BoundaryOperation::GraphExtend, Instance::Graph(graph)) => boundary_outcome(
            graph_extend(graph, source, target, migration, protocols),
            Instance::Graph,
        ),
        (BoundaryOperation::TreeRestrict, Instance::Tree(tree)) => boundary_outcome(
            wtype_restrict(tree, source, target, migration, protocols),
            Instance::Tree,
        ),
        (BoundaryOperation::TreeExtend, Instance::Tree(tree)) => boundary_outcome(
            wtype_extend(tree, source, target, migration, protocols),
            Instance::Tree,
        ),
        (BoundaryOperation::TableRestrict, Instance::Table(table)) => boundary_outcome(
            functor_restrict(table, source, target, migration, protocols),
            Instance::Table,
        ),
        (BoundaryOperation::TableExtend, Instance::Table(table)) => boundary_outcome(
            functor_extend(table, source, target, migration, protocols),
            Instance::Table,
        ),
        _ => return Err("boundary operation and input shape disagree".into()),
    };
    Ok(BoundaryTransition {
        operation,
        target_schema: target.clone(),
        migration: BoundaryMigration::from_compiled(migration),
        outcome,
    })
}

fn structural_boundary_cases(
    child: &Edge,
    source: &Schema,
    target: &Schema,
    drop_child: CompiledMigration,
    keep_all: CompiledMigration,
) -> R<Vec<Case>> {
    let graph_with_child = Instance::Graph(graph(
        vec![Node::new(0, "root"), Node::new(1, "child")],
        &[(0, 1, child)],
    ));
    let graph_lone = Instance::Graph(GInstance::new().with_node(Node::new(0, "root")));
    let tree_with_child = Instance::Tree(tree(
        vec![Node::new(0, "root"), Node::new(1, "child")],
        &[(0, 1, child)],
        Vec::new(),
    ));
    let tree_lone = Instance::Tree(tree(vec![Node::new(0, "root")], &[], Vec::new()));
    let mut cases = Vec::new();
    for (name, operation, input, migration) in [
        (
            "boundary/graph_restrict_output",
            BoundaryOperation::GraphRestrict,
            graph_with_child,
            drop_child.clone(),
        ),
        (
            "boundary/graph_extend_output",
            BoundaryOperation::GraphExtend,
            graph_lone,
            keep_all.clone(),
        ),
    ] {
        let protocol = fixture_protocol(InstanceShapeKind::Graph);
        let transition =
            boundary_transition(operation, source, target, &migration, &input, &protocol)?;
        cases.push(Case::new(name, source.clone(), input).with_boundary(transition));
    }
    for (name, operation, input, migration) in [
        (
            "boundary/tree_restrict_output",
            BoundaryOperation::TreeRestrict,
            tree_with_child,
            drop_child,
        ),
        (
            "boundary/tree_extend_output",
            BoundaryOperation::TreeExtend,
            tree_lone,
            keep_all,
        ),
    ] {
        let protocol = fixture_protocol(InstanceShapeKind::Tree);
        let transition =
            boundary_transition(operation, source, target, &migration, &input, &protocol)?;
        cases.push(Case::new(name, source.clone(), input).with_boundary(transition));
    }
    Ok(cases)
}

fn table_boundary_cases() -> R<Vec<Case>> {
    let table_relation = edge("record", "child", "fk", "child");
    let table_source = S::default()
        .vertex("record", "table")
        .vertex("child", "table")
        .edge(&table_relation)
        .build();
    let table_target = S::default()
        .vertex("record", "table")
        .vertex("child", "table")
        .edge(&table_relation)
        .required("record", &table_relation)
        .build();
    let table = Instance::Table(
        FInstance::new()
            .with_rows("record", [(RowId(0), HashMap::new())])?
            .with_rows("child", [(RowId(1), HashMap::new())])?,
    );
    let mut keep_table = CompiledMigration::default();
    keep_table
        .surviving_verts
        .extend([Name::from("record"), Name::from("child")]);
    keep_table.surviving_edges.insert(table_relation);
    let table_protocol = Protocol {
        name: "validation-fixtures-table-boundary".into(),
        syntactic_shape: InstanceShapeKind::Table,
        semantic_shape: InstanceShapeKind::Graph,
        non_value_kinds: BTreeSet::from(["table".into()]),
        ..Protocol::default()
    };

    let mut cases = Vec::new();
    for (name, operation) in [
        (
            "boundary/table_restrict_output",
            BoundaryOperation::TableRestrict,
        ),
        (
            "boundary/table_extend_output",
            BoundaryOperation::TableExtend,
        ),
    ] {
        let transition = boundary_transition(
            operation,
            &table_source,
            &table_target,
            &keep_table,
            &table,
            &table_protocol,
        )?;
        cases.push(
            Case::new(name, table_source.clone(), table.clone())
                .with_boundary(transition)
                .with_protocol(table_protocol.clone()),
        );
    }
    Ok(cases)
}

/// Restrict and extend boundary failures in every instance shape. Each input
/// passes its source plan and the generated output fails the target plan.
fn boundary() -> R<Vec<Case>> {
    let child = prop("root", "child", "child");
    let source = S::default()
        .vertex("root", "object")
        .vertex("child", "object")
        .edge(&child)
        .build();
    let target = S::default()
        .vertex("root", "object")
        .vertex("child", "object")
        .edge(&child)
        .required("root", &child)
        .build();
    let mut drop_child = CompiledMigration::default();
    drop_child.surviving_verts.insert("root".into());
    let mut keep_all = CompiledMigration::default();
    keep_all
        .surviving_verts
        .extend([Name::from("root"), Name::from("child")]);
    keep_all.surviving_edges.insert(child.clone());

    let mut cases = structural_boundary_cases(&child, &source, &target, drop_child, keep_all)?;
    cases.extend(table_boundary_cases()?);
    Ok(cases)
}

/// Train-Benchmark-shaped fault injection and repair sequences. The repair
/// additionally records an admitted finite-budget witness with evaluated
/// and skipped external runs.
fn witness_cut_budget(
    plan: &ValidationPlan,
    invalid: &GInstance,
    repair: &GraphEdit,
    external: &ExternalVerdicts,
) -> R<ValidationBudget> {
    let (validated, _) = ValidatedGraph::new(
        std::sync::Arc::new(plan.clone()),
        invalid.clone(),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let validation = validated.check_edit(repair, &ValidationBudget::default(), external)?;
    let delta = repair.to_delta()?;
    let mut post = invalid.clone();
    delta.apply(&mut post)?;
    let scheduled: Vec<_> = validation
        .footprint
        .rerun
        .union(&validation.footprint.created)
        .collect();
    let Some(last) = scheduled.last() else {
        return Err("the Train-Benchmark repair schedules no checks".into());
    };
    if last.check.kind != panproto_inst::validation::CheckKind::External {
        return Err("the Train-Benchmark cut must end in an external check".into());
    }
    let max_reads = scheduled[..scheduled.len() - 1]
        .iter()
        .map(|instance| post.reads(plan, external, instance).len() as u64)
        .sum();
    Ok(ValidationBudget {
        max_reads: Some(max_reads),
    })
}

fn train_graph(size: u32, child: &Edge, next: &Edge) -> (GInstance, GEdge, EdgeId, GEdge) {
    let present = |text: &str| FieldPresence::Present(Value::Str(text.into()));
    let mut valid = GInstance::new();
    for id in 0..size {
        valid = valid
            .with_node(Node::new(id, "item").with_value(present("ok")))
            .with_node(Node::new(size + id, "child"));
    }
    for id in 0..size {
        valid = valid.with_edge_id(EdgeId(id), id, size + id, child.clone());
        if id + 1 < size {
            valid = valid.with_edge_id(EdgeId(size + id), id, id + 1, next.clone());
        }
    }
    (
        valid,
        GEdge::new(0, size, child.clone()),
        EdgeId(size.saturating_mul(2)),
        GEdge::new(size - 1, 0, next.clone()),
    )
}

fn train_edits(
    required_edge: &GEdge,
    closing_id: EdgeId,
    closing_edge: &GEdge,
) -> (GraphEdit, GraphEdit) {
    let present = |text: &str| FieldPresence::Present(Value::Str(text.into()));
    (
        GraphEdit::Batch(vec![
            GraphEdit::SetValue {
                node: 0,
                old: Some(present("ok")),
                new: Some(present("broken")),
            },
            GraphEdit::SetValue {
                node: 1,
                old: Some(present("ok")),
                new: Some(present("broken")),
            },
            GraphEdit::RemoveEdge {
                id: EdgeId(0),
                edge: required_edge.clone(),
            },
            GraphEdit::AddEdge {
                id: closing_id,
                edge: closing_edge.clone(),
            },
        ]),
        GraphEdit::Batch(vec![
            GraphEdit::SetValue {
                node: 0,
                old: Some(present("broken")),
                new: Some(present("ok")),
            },
            GraphEdit::SetValue {
                node: 1,
                old: Some(present("broken")),
                new: Some(present("ok")),
            },
            GraphEdit::AddEdge {
                id: EdgeId(0),
                edge: required_edge.clone(),
            },
            GraphEdit::RemoveEdge {
                id: closing_id,
                edge: closing_edge.clone(),
            },
        ]),
    )
}

fn train_external() -> ExternalVerdicts {
    let mut external = ExternalVerdicts::default();
    for (name, passed, byte) in [("audit", false, 7), ("tail", true, 8)] {
        external.insert(
            name.into(),
            ViewRef::Node(0),
            ExternalVerdict {
                passed,
                witness: Digest([byte; 32]),
            },
        );
    }
    external
}

fn train_benchmark() -> R<Vec<Case>> {
    let child = prop("item", "child", "child");
    let next = prop("item", "item", "next");
    let schema = S::default()
        .vertex("item", "object")
        .vertex("child", "object")
        .edge(&child)
        .edge(&next)
        .required("item", &child)
        .constraint("item", "maxLength", "3")
        .constraint("item", "acyclic", "next")
        .build();
    let options = ValidationOptions {
        external: ["audit", "tail"]
            .into_iter()
            .map(|name| {
                (
                    name.into(),
                    ExternalCheckDecl {
                        name: name.into(),
                        anchors: BTreeSet::from(["item".into()]),
                        requirement: Requirement::Advisory,
                    },
                )
            })
            .collect(),
        ..ValidationOptions::default()
    };
    let mut cases = Vec::new();
    for size in [4_u32, 32, 256] {
        let (valid, required_edge, closing_id, closing_edge) = train_graph(size, &child, &next);
        let (inject, repair) = train_edits(&required_edge, closing_id, &closing_edge);
        let mut inject_case = Case::new(
            format!("train_benchmark/{size}_inject"),
            schema.clone(),
            Instance::Graph(valid.clone()),
        )
        .with_edits(EditTrace::Graph(vec![inject.clone()]));
        inject_case.options = options.clone();
        inject_case.record_violation_tuples = true;
        cases.push(inject_case);

        let mut invalid = valid.clone();
        inject.apply(&mut invalid)?;
        let external = train_external();
        let protocol = fixture_protocol(InstanceShapeKind::Graph);
        let plan = ValidationPlan::compile(&schema, &protocol, &options);
        let budget = witness_cut_budget(&plan, &invalid, &repair, &external)?;
        let mut repair_case = Case::new(
            format!("train_benchmark/{size}_repair"),
            schema.clone(),
            Instance::Graph(invalid),
        )
        .with_edits(EditTrace::Graph(vec![repair.clone()]))
        .with_incremental(IncrementalRequest {
            edit: repair,
            policy: AdmissionPolicy::RequirePassed,
            budget,
            external,
            options: WitnessOptions { digests: true },
        });
        repair_case.options = options.clone();
        cases.push(repair_case);
    }
    Ok(cases)
}

/// The probe schema: a person works at an organization, must, and has a
/// name of at most three characters.
fn probe_schema() -> Schema {
    let works_at = prop("person", "org", "worksAt");
    S::default()
        .vertex("person", "object")
        .vertex("org", "object")
        .edge(&works_at)
        .required("person", &works_at)
        .constraint("person", "maxLength", "3")
        .build()
}

/// Missing and unknown anchors, undeclared labels, in every shape.
fn anchors() -> R<Vec<Case>> {
    let works_at = prop("person", "org", "worksAt");
    let undeclared = prop("person", "org", "undeclared");
    let s = |v: &str| Value::Str(v.into());
    // Five faults (a ghost anchor, a person working at a person, an
    // undeclared label, a person working nowhere, a name too long) beside
    // two legal parallel `worksAt` edges.
    let probe = graph(
        vec![
            valued(0, "person", s("ann")),
            valued(1, "person", s("abcdefgh")),
            Node::new(2, "org"),
            valued(3, "person", s("bob")),
            Node::new(4, "ghost"),
        ],
        &[
            (0, 2, &works_at),
            (0, 2, &works_at),
            (1, 2, &works_at),
            (1, 0, &works_at),
            (0, 2, &undeclared),
        ],
    );
    let tables = S::default()
        .vertex("film", "table")
        .vertex("actor", "table")
        .vertex("title", "string")
        .edge(&prop("film", "title", "title"))
        .edge(&edge("film", "actor", "fk", "stars"))
        .build();
    let unknown_table = FInstance::new()
        .with_rows("film", [(RowId(0), row(&[("title", s("Alien"))]))])
        .and_then(|f| f.with_rows("ghost_table", [(RowId(1), row(&[]))]))?;
    let unknown_fk_label = FInstance::new()
        .with_rows("film", [(RowId(0), row(&[("title", s("Alien"))]))])
        .and_then(|f| f.with_rows("actor", [(RowId(1), row(&[]))]))
        .and_then(|f| {
            f.with_foreign_key(
                edge("film", "actor", "fk", "directs"),
                [(RowId(0), RowId(1))],
            )
        })?;
    let ghost_root = tree(vec![Node::new(0, "ghost")], &[], vec![]);
    Ok(vec![
        Case::new("anchors/probe", probe_schema(), Instance::Graph(probe)),
        Case::new(
            "anchors/unknown_table",
            tables.clone(),
            Instance::Table(unknown_table),
        ),
        Case::new(
            "anchors/unknown_fk_label",
            tables,
            Instance::Table(unknown_fk_label),
        ),
        Case::new(
            "anchors/tree_unknown_root_anchor",
            probe_schema(),
            Instance::Tree(ghost_root),
        ),
    ])
}

/// The incremental probe of `edit` under `RequirePassed`, an unlimited
/// budget, no external verdicts and both instance digests.
fn passing_digest_probe(edit: GraphEdit) -> IncrementalRequest {
    IncrementalRequest {
        edit,
        policy: AdmissionPolicy::RequirePassed,
        budget: ValidationBudget::default(),
        external: ExternalVerdicts::default(),
        options: WitnessOptions { digests: true },
    }
}

/// Edits that break a kept edge or arc, by changing its target's anchor or
/// its label, in graphs and trees.
fn endpoint_edits() -> R<Vec<Case>> {
    let ab = prop("a", "b", "ab");
    let ba = prop("b", "a", "ba");
    let schema = S::default()
        .vertex("a", "object")
        .vertex("b", "object")
        .edge(&ab)
        .build();
    let both_ways = S::default()
        .vertex("a", "object")
        .vertex("b", "object")
        .edge(&ab)
        .edge(&ba)
        .build();
    let coherent = graph(vec![Node::new(0, "a"), Node::new(1, "b")], &[(0, 1, &ab)]);
    let (kept, _) = coherent
        .edges_sorted()
        .next()
        .ok_or("the coherent pair has no edge")?;
    let tree_pair = || {
        tree(
            vec![Node::new(0, "a"), Node::new(1, "b")],
            &[(0, 1, &ab)],
            vec![],
        )
    };
    let endpoint_repair = GraphEdit::RelabelNode {
        node: 1,
        old_anchor: "a".into(),
        new_anchor: "b".into(),
    };
    Ok(vec![
        Case::new(
            "endpoints/incremental_repair_kept_edge",
            schema.clone(),
            Instance::Graph(graph(
                vec![Node::new(0, "a"), Node::new(1, "a")],
                &[(0, 1, &ab)],
            )),
        )
        .with_edits(EditTrace::Graph(vec![endpoint_repair.clone()]))
        .with_incremental(passing_digest_probe(endpoint_repair)),
        Case::new(
            "endpoints/edit_anchor_change_breaks_kept_edge",
            schema.clone(),
            Instance::Graph(coherent.clone()),
        )
        .with_edits(EditTrace::Graph(vec![GraphEdit::RelabelNode {
            node: 1,
            old_anchor: "b".into(),
            new_anchor: "a".into(),
        }]))
        .with_incremental(passing_digest_probe(GraphEdit::RelabelNode {
            node: 1,
            old_anchor: "b".into(),
            new_anchor: "a".into(),
        })),
        Case::new(
            "endpoints/edit_relabel_breaks_kept_edge",
            both_ways.clone(),
            Instance::Graph(coherent),
        )
        .with_edits(EditTrace::Graph(vec![GraphEdit::RelabelEdge {
            id: kept,
            old: ab.clone(),
            new: ba.clone(),
        }]))
        .with_incremental(passing_digest_probe(GraphEdit::RelabelEdge {
            id: kept,
            old: ab.clone(),
            new: ba.clone(),
        })),
        Case::new(
            "endpoints/tree_edit_anchor_change_breaks_kept_arc",
            schema,
            Instance::Tree(tree_pair()),
        )
        .with_edits(EditTrace::Tree(vec![TreeEdit::RelabelNode {
            id: 1,
            new_anchor: "a".into(),
        }])),
        // A tree arc changes its label by moving its child under the same
        // parent along another edge. The arc is kept by its identity in the
        // tree, the pair (parent, child); the graph lowering of the move
        // deletes the edge and adds one under a fresh edge id.
        Case::new(
            "endpoints/tree_edit_relabel_breaks_kept_arc",
            both_ways,
            Instance::Tree(tree_pair()),
        )
        .with_edits(EditTrace::Tree(vec![TreeEdit::MoveSubtree {
            node_id: 1,
            new_parent: 0,
            edge: ba,
        }])),
    ])
}

/// Incoherent endpoints: the source side, the target side, both, and a
/// self-loop on an edge that is not a self-edge; a coherent edge; a tree
/// arc.
fn endpoints() -> Vec<Case> {
    let ab = prop("a", "b", "ab");
    let schema = S::default()
        .vertex("a", "object")
        .vertex("b", "object")
        .edge(&ab)
        .build();
    let pair = |x: &str, y: &str| graph(vec![Node::new(0, x), Node::new(1, y)], &[(0, 1, &ab)]);
    vec![
        Case::new(
            "endpoints/coherent",
            schema.clone(),
            Instance::Graph(pair("a", "b")),
        ),
        Case::new(
            "endpoints/src_side",
            schema.clone(),
            Instance::Graph(pair("b", "b")),
        ),
        Case::new(
            "endpoints/tgt_side",
            schema.clone(),
            Instance::Graph(pair("a", "a")),
        ),
        Case::new(
            "endpoints/both_sides",
            schema.clone(),
            Instance::Graph(pair("b", "a")),
        ),
        Case::new(
            "endpoints/self_loop_on_non_self_edge",
            schema.clone(),
            Instance::Graph(graph(vec![Node::new(0, "a")], &[(0, 0, &ab)])),
        ),
        Case::new(
            "endpoints/tree_tgt_side",
            schema,
            Instance::Tree(tree(
                vec![Node::new(0, "a"), Node::new(1, "a")],
                &[(0, 1, &ab)],
                vec![],
            )),
        ),
    ]
}

/// Values and attributes: a value breaking its bound, an undeclared
/// attribute and a mistyped one, in each shape.
fn values() -> R<Vec<Case>> {
    let name = prop("item", "name", "name");
    let schema = S::default()
        .vertex("item", "object")
        .vertex("name", "string")
        .edge(&name)
        .constraint("name", "maxLength", "3")
        .build();
    let s = |v: &str| Value::Str(v.into());
    let mut item = Node::new(0, "item");
    item.extra_fields.insert("name".into(), Value::Int(5));
    item.extra_fields.insert("colour".into(), s("red"));
    let graph_case = graph(
        vec![item.clone(), valued(1, "name", s("toolong"))],
        &[(0, 1, &name)],
    );
    let tree_case = tree(
        vec![item, valued(1, "name", s("toolong"))],
        &[(0, 1, &name)],
        vec![],
    );
    let table_schema = S::default()
        .vertex("item", "table")
        .vertex("name", "string")
        .edge(&name)
        .constraint("name", "maxLength", "3")
        .build();
    let table_case = FInstance::new().with_rows(
        "item",
        [
            (
                RowId(0),
                row(&[("name", s("toolong")), ("colour", s("red"))]),
            ),
            (RowId(1), row(&[("name", Value::Int(5))])),
        ],
    )?;
    Ok(vec![
        Case::new(
            "values/graph_values_and_attributes",
            schema.clone(),
            Instance::Graph(graph_case),
        ),
        Case::new(
            "values/tree_values_and_attributes",
            schema,
            Instance::Tree(tree_case),
        ),
        Case::new(
            "values/table_values_and_attributes",
            table_schema,
            Instance::Table(table_case),
        ),
    ])
}

/// Required edges present and absent, out-degree bounds at and past the
/// boundary, parallel edges counted, and a required foreign key.
fn cardinality() -> R<Vec<Case>> {
    let child = prop("parent", "child", "child");
    let base = || {
        S::default()
            .vertex("parent", "object")
            .vertex("child", "object")
            .edge(&child)
    };
    let required = base().required("parent", &child).build();
    let at_most = |n: &str| base().constraint("parent", "maxOut", n).build();
    let at_least = base().constraint("parent", "minOut", "1").build();
    let fan_out = |k: u32| {
        let mut nodes = vec![Node::new(0, "parent")];
        nodes.extend((1..=k).map(|i| Node::new(i, "child")));
        let edges: Vec<(u32, u32, &Edge)> = (1..=k).map(|i| (0, i, &child)).collect();
        graph(nodes, &edges)
    };
    let parallel = graph(
        vec![Node::new(0, "parent"), Node::new(1, "child")],
        &[(0, 1, &child), (0, 1, &child)],
    );
    let stars = edge("film", "actor", "fk", "stars");
    let films = S::default()
        .vertex("film", "table")
        .vertex("actor", "table")
        .edge(&stars)
        .required("film", &stars)
        .build();
    let unstarred = FInstance::new()
        .with_rows("film", [(RowId(0), row(&[])), (RowId(1), row(&[]))])
        .and_then(|f| f.with_rows("actor", [(RowId(2), row(&[]))]))
        .and_then(|f| f.with_foreign_key(stars.clone(), [(RowId(0), RowId(2))]))?;
    Ok(vec![
        Case::new(
            "cardinality/required_edge_present",
            required.clone(),
            Instance::Graph(fan_out(1)),
        ),
        Case::new(
            "cardinality/required_edge_absent",
            required,
            Instance::Graph(fan_out(0)),
        ),
        Case::new(
            "cardinality/degree_at_upper_bound",
            at_most("2"),
            Instance::Graph(fan_out(2)),
        ),
        Case::new(
            "cardinality/degree_above_upper_bound",
            at_most("2"),
            Instance::Graph(fan_out(3)),
        ),
        Case::new(
            "cardinality/degree_at_lower_bound",
            at_least.clone(),
            Instance::Graph(fan_out(1)),
        ),
        Case::new(
            "cardinality/degree_below_lower_bound",
            at_least,
            Instance::Graph(fan_out(0)),
        ),
        Case::new(
            "cardinality/parallel_edges_counted",
            at_most("1"),
            Instance::Graph(parallel),
        ),
        Case::new(
            "cardinality/table_required_foreign_key_absent",
            films,
            Instance::Table(unstarred),
        ),
    ])
}

/// Positions of an ordered edge: two out-edges sharing one, absent
/// positions, distinct positions, and a reorder that creates a duplicate
/// position and then repairs it.
fn ordering() -> R<Vec<Case>> {
    let items = prop("list", "item", "items");
    let schema = S::default()
        .vertex("list", "object")
        .vertex("item", "object")
        .edge(&items)
        .ordered(&items)
        .build();
    let positioned = |p: [Option<u32>; 2]| -> R<GInstance> {
        let mut g = graph(
            vec![
                Node::new(0, "list"),
                Node::new(1, "item"),
                Node::new(2, "item"),
            ],
            &[(0, 1, &items), (0, 2, &items)],
        );
        let ids: Vec<_> = g.edges_sorted().map(|(id, _)| id).collect();
        for (id, position) in ids.into_iter().zip(p) {
            g.set_edge_position(id, position)?;
        }
        Ok(g)
    };
    let distinct = positioned([Some(0), Some(1)])?;
    let ids: Vec<EdgeId> = distinct.edges_sorted().map(|(id, _)| id).collect();
    let [first, second] = ids[..] else {
        return Err("the ordered list has not two edges".into());
    };
    let reorder = vec![
        GraphEdit::SetEdgePosition {
            id: second,
            old: Some(1),
            new: Some(0),
        },
        GraphEdit::SetEdgePosition {
            id: first,
            old: Some(0),
            new: Some(1),
        },
    ];
    let repair = GraphEdit::SetEdgePosition {
        id: second,
        old: Some(0),
        new: Some(1),
    };
    Ok(vec![
        Case::new(
            "ordering/incremental_repair_duplicate",
            schema.clone(),
            Instance::Graph(positioned([Some(0), Some(0)])?),
        )
        .with_edits(EditTrace::Graph(vec![repair.clone()]))
        .with_incremental(IncrementalRequest {
            edit: repair,
            policy: AdmissionPolicy::RequirePassed,
            budget: ValidationBudget::default(),
            external: ExternalVerdicts::default(),
            options: WitnessOptions { digests: true },
        }),
        Case::new(
            "ordering/edit_reorder_creates_and_repairs_duplicate",
            schema.clone(),
            Instance::Graph(distinct),
        )
        .with_edits(EditTrace::Graph(reorder.clone()))
        .with_incremental(IncrementalRequest {
            edit: GraphEdit::Batch(reorder),
            policy: AdmissionPolicy::RequirePassed,
            budget: ValidationBudget::default(),
            external: ExternalVerdicts::default(),
            options: WitnessOptions { digests: true },
        }),
        Case::new(
            "ordering/duplicate_positions",
            schema.clone(),
            Instance::Graph(positioned([Some(0), Some(0)])?),
        ),
        Case::new(
            "ordering/absent_positions_allowed",
            schema.clone(),
            Instance::Graph(positioned([None, None])?),
        ),
        Case::new(
            "ordering/distinct_positions",
            schema,
            Instance::Graph(positioned([Some(0), Some(1)])?),
        ),
    ])
}

/// Edits of a self-edge declared acyclic: one that closes a cycle and one
/// that breaks it; and a tree edit moving a subtree under its own
/// descendant, which applying refuses.
fn cycle_edits() -> R<Vec<Case>> {
    let next = prop("item", "item", "next");
    let acyclic = S::default()
        .vertex("item", "object")
        .edge(&next)
        .constraint("item", "acyclic", "next")
        .build();
    let chain = graph(
        (0..3).map(|i| Node::new(i, "item")).collect(),
        &[(0, 1, &next), (1, 2, &next)],
    );
    let closed = graph(
        (0..3).map(|i| Node::new(i, "item")).collect(),
        &[(0, 1, &next), (1, 2, &next), (2, 0, &next)],
    );
    let (closing, closing_edge) = closed
        .edges_sorted()
        .find(|(_, e)| e.src == 2 && e.tgt == 0)
        .map(|(id, e)| (id, e.clone()))
        .ok_or("the ring has no closing edge")?;
    let free_id = chain
        .edges_sorted()
        .map(|(id, _)| id.0 + 1)
        .max()
        .unwrap_or(0);
    Ok(vec![
        Case::new(
            "cycles/edit_closes_cycle",
            acyclic.clone(),
            Instance::Graph(chain),
        )
        .with_edits(EditTrace::Graph(vec![GraphEdit::AddEdge {
            id: EdgeId(free_id),
            edge: GEdge::new(2, 0, next.clone()),
        }]))
        .with_incremental(IncrementalRequest {
            edit: GraphEdit::AddEdge {
                id: EdgeId(free_id),
                edge: GEdge::new(2, 0, next.clone()),
            },
            policy: AdmissionPolicy::RequirePassed,
            budget: ValidationBudget::default(),
            external: ExternalVerdicts::default(),
            options: WitnessOptions { digests: true },
        }),
        Case::new("cycles/edit_breaks_cycle", acyclic, Instance::Graph(closed))
            .with_edits(EditTrace::Graph(vec![GraphEdit::RemoveEdge {
                id: closing,
                edge: closing_edge.clone(),
            }]))
            .with_incremental(IncrementalRequest {
                edit: GraphEdit::RemoveEdge {
                    id: closing,
                    edge: closing_edge,
                },
                policy: AdmissionPolicy::RequirePassed,
                budget: ValidationBudget::default(),
                external: ExternalVerdicts::default(),
                options: WitnessOptions { digests: true },
            }),
        Case::new(
            "cycles/tree_edit_moves_subtree_under_its_own_descendant",
            free_tree_schema(),
            Instance::Tree(tree(
                vec![
                    Node::new(0, "item"),
                    Node::new(1, "item"),
                    Node::new(2, "item"),
                ],
                &[(0, 1, &next), (1, 2, &next)],
                vec![],
            )),
        )
        .with_edits(EditTrace::Tree(vec![TreeEdit::MoveSubtree {
            node_id: 1,
            new_parent: 2,
            edge: next.clone(),
        }])),
    ])
}

/// Cycles of a self-edge: legal without a declaration; with `Acyclic`, a
/// two-cycle, a longer cycle and a self-loop fail and a chain passes. Tree
/// arcs that close a cycle away from the root leave every node of the
/// cycle unreachable; such a tree is built directly, since a tree edit
/// that would close the cycle is refused.
fn cycles() -> Vec<Case> {
    let next = prop("item", "item", "next");
    let base = S::default().vertex("item", "object").edge(&next);
    let free = base.build();
    let acyclic = S::default()
        .vertex("item", "object")
        .edge(&next)
        .constraint("item", "acyclic", "next")
        .build();
    let ring = |n: u32, closed: bool| {
        let nodes = (0..n).map(|i| Node::new(i, "item")).collect();
        let mut edges: Vec<(u32, u32, &Edge)> = (0..n.saturating_sub(1))
            .map(|i| (i, i + 1, &next))
            .collect();
        if closed {
            edges.push((n - 1, 0, &next));
        }
        graph(nodes, &edges)
    };
    vec![
        Case::new(
            "cycles/legal_without_declaration",
            free,
            Instance::Graph(ring(2, true)),
        ),
        Case::new(
            "cycles/two_cycle",
            acyclic.clone(),
            Instance::Graph(ring(2, true)),
        ),
        Case::new(
            "cycles/long_cycle",
            acyclic.clone(),
            Instance::Graph(ring(4, true)),
        ),
        Case::new(
            "cycles/self_loop",
            acyclic.clone(),
            Instance::Graph(ring(1, true)),
        ),
        Case::new(
            "cycles/acyclic_chain",
            acyclic,
            Instance::Graph(ring(3, false)),
        ),
        // The tree a move of a subtree under its own descendant would leave,
        // built directly: nodes 1 and 2 hang under each other, cut off from
        // the root.
        Case::new(
            "cycles/tree_subtree_under_its_own_descendant",
            free_tree_schema(),
            Instance::Tree(tree(
                vec![
                    Node::new(0, "item"),
                    Node::new(1, "item"),
                    Node::new(2, "item"),
                ],
                &[(2, 1, &next), (1, 2, &next)],
                vec![],
            )),
        ),
        // Arcs closing a three-node cycle beside a reachable child of the
        // root: nodes 1, 2 and 3 are unreachable, node 4 is not.
        Case::new(
            "cycles/tree_arcs_close_a_cycle_off_the_root",
            free_tree_schema(),
            Instance::Tree(tree(
                (0..5).map(|i| Node::new(i, "item")).collect(),
                &[(0, 4, &next), (1, 2, &next), (2, 3, &next), (3, 1, &next)],
                vec![],
            )),
        ),
    ]
}

/// An item whose `next` edge is free to cycle.
fn free_tree_schema() -> Schema {
    S::default()
        .vertex("item", "object")
        .edge(&prop("item", "item", "next"))
        .build()
}

/// The fan schema: a document of words, and the relation `rel` between two
/// words of a document.
fn fan_schema() -> Schema {
    S::default()
        .vertex("doc", "object")
        .vertex("word", "object")
        .edge(&prop("doc", "word", "words"))
        .hyper_edge("rel", &[("arg0", "word"), ("arg1", "word")], "about")
        .build()
}

/// A document of two words, related by `fan`.
fn fan_tree(fan: Fan) -> WInstance {
    let words = prop("doc", "word", "words");
    tree(
        vec![
            Node::new(0, "doc"),
            Node::new(1, "word"),
            Node::new(2, "word"),
        ],
        &[(0, 1, &words), (0, 2, &words)],
        vec![fan],
    )
}

/// The well-formed fan.
fn rel() -> Fan {
    Fan::new("rel", 0)
        .with_child("arg0", 1)
        .with_child("arg1", 2)
}

/// A label of the fan vocabulary: of kind `kind`, from the hyper-edge `rel`
/// to `tgt`, named `name`.
fn fan_label(kind: &str, tgt: &str, name: &str) -> Edge {
    edge("rel", tgt, kind, name)
}

/// Every fan violation, in graphs and trees, and well-formed fans.
fn fans() -> R<Vec<Case>> {
    let schema = fan_schema();
    let reified = |fan: Fan| GInstance::from_wtype_with_schema(&fan_tree(fan), &schema);
    let well_formed = reified(rel())?;
    let fan_node = well_formed
        .nodes()
        .values()
        .find(|n| n.shape == NodeShape::HyperEdge)
        .map(|n| n.id)
        .ok_or("the well-formed fan has no fan node")?;
    let with = |e: Edge, src: u32, tgt: u32| well_formed.clone().with_edge(src, tgt, e);
    let mut missing_parent = well_formed.clone();
    let parent_edge = missing_parent
        .out_edges(fan_node)
        .iter()
        .copied()
        .find(|e| {
            missing_parent
                .edge(*e)
                .is_some_and(|g| g.edge.kind == HYPER_PARENT_KIND)
        })
        .ok_or("the fan node has no parent edge")?;
    missing_parent.remove_edge(parent_edge)?;
    let graphs = vec![
        ("fans/well_formed", well_formed.clone()),
        ("fans/missing_parent", missing_parent),
        (
            "fans/multiple_parents",
            with(fan_label(HYPER_PARENT_KIND, "word", "about"), fan_node, 1),
        ),
        (
            "fans/duplicate_label",
            with(fan_label(HYPER_LABEL_KIND, "word", "arg0"), fan_node, 2),
        ),
        (
            "fans/unknown_label",
            with(fan_label(HYPER_LABEL_KIND, "word", "arg9"), fan_node, 1),
        ),
        (
            "fans/reserved_kind_outside_fan",
            with(edge("word", "word", HYPER_LABEL_KIND, "arg0"), 1, 2),
        ),
        (
            "fans/foreign_incident_edge",
            with(prop("doc", "word", "words"), 0, fan_node),
        ),
        (
            "fans/unknown_hyper_edge",
            reified(Fan::new("nope", 0).with_child("arg0", 1))?,
        ),
    ];
    let mut out: Vec<Case> = graphs
        .into_iter()
        .map(|(name, g)| Case::new(name, schema.clone(), Instance::Graph(g)))
        .collect();
    for (name, fan) in [
        ("fans/tree_well_formed", rel()),
        (
            "fans/tree_missing_parent",
            Fan::new("rel", 9).with_child("arg0", 1),
        ),
        ("fans/tree_missing_child", rel().with_child("arg1", 9)),
        ("fans/tree_unknown_label", rel().with_child("arg9", 2)),
        (
            "fans/tree_unknown_hyper_edge",
            Fan::new("nope", 0).with_child("arg0", 1),
        ),
    ] {
        out.push(Case::new(
            name,
            schema.clone(),
            Instance::Tree(fan_tree(fan)),
        ));
    }
    Ok(out)
}

/// A table whose rows hold `columns`, each an integer or string column
/// vertex named `<table>.<column>`.
fn table(s: S, name: &str, columns: &[(&str, &str)]) -> S {
    let mut s = s.vertex(name, "table");
    for (column, kind) in columns {
        let vertex = format!("{name}.{column}");
        s = s.vertex(&vertex, kind).edge(&prop(name, &vertex, column));
    }
    s
}

/// The reference from `holder`'s `column` to `target`'s key `key`, which
/// becomes the foreign key `fk`.
fn reference(
    id: &str,
    holder: &str,
    column: &str,
    target: &str,
    key: &str,
    nullable: bool,
    on_unresolved: OnUnresolved,
) -> Reference {
    Reference {
        id: id.into(),
        holder: holder.into(),
        source: RefSource::KeyValue {
            parts: vec![vec![prop(holder, &format!("{holder}.{column}"), column)]],
        },
        target_vertex: target.into(),
        target_key: Some(key.into()),
        edge: Some(edge(holder, target, "fk", id)),
        cardinality: Cardinality::One,
        nullable,
        on_unresolved,
        mode: ResolveMode::AddEdge,
    }
}

/// The Sakila-style schema: actors and films keyed by id, the film-actor
/// table keyed by the pair, and staff reporting to staff; the film-actor
/// references resolve on `on_unresolved`.
fn sakila(on_unresolved: OnUnresolved) -> R<Schema> {
    let int = "integer";
    let mut s = table(
        S::default(),
        "actor",
        &[("actor_id", int), ("name", "string")],
    );
    s = table(s, "film", &[("film_id", int), ("title", "string")]);
    s = table(s, "film_actor", &[("actor_id", int), ("film_id", int)]);
    s = table(s, "staff", &[("staff_id", int), ("reports_to", int)]);
    let col = |t: &str, c: &str| prop(t, &format!("{t}.{c}"), c);
    s = s
        .key(
            "actor",
            "actor_pk",
            &[col("actor", "actor_id")],
            KeyRole::Primary,
        )
        .key(
            "film",
            "film_pk",
            &[col("film", "film_id")],
            KeyRole::Primary,
        )
        .key(
            "film_actor",
            "film_actor_pk",
            &[col("film_actor", "actor_id"), col("film_actor", "film_id")],
            KeyRole::Primary,
        )
        .key(
            "staff",
            "staff_pk",
            &[col("staff", "staff_id")],
            KeyRole::Primary,
        );
    for r in [
        reference(
            "film_actor_actor",
            "film_actor",
            "actor_id",
            "actor",
            "actor_pk",
            false,
            on_unresolved.clone(),
        ),
        reference(
            "film_actor_film",
            "film_actor",
            "film_id",
            "film",
            "film_pk",
            false,
            on_unresolved,
        ),
        reference(
            "staff_reports_to",
            "staff",
            "reports_to",
            "staff",
            "staff_pk",
            true,
            OnUnresolved::Error,
        ),
    ] {
        let fk = r.edge.clone().ok_or("a reference without an edge")?;
        s = s.edge(&fk);
        s.0.declarations.insert_reference(r);
    }
    Ok(s.build())
}

/// The cells of one row, by column, in column order.
type Cells = Vec<(&'static str, Value)>;

/// The rows of a table instance, table by table, each with its id and
/// cells.
type Rows = Vec<(&'static str, Vec<(RowId, Cells)>)>;

/// Sakila rows: two actors, two films, the given film-actor pairs, and a
/// staff chain.
fn sakila_data(film_actor: &[(i64, i64)]) -> Rows {
    let i = Value::Int;
    let s = |v: &str| Value::Str(v.into());
    let mut next = 0_u32;
    let mut id = || {
        next += 1;
        RowId(next)
    };
    let actors = [(1, "Ann"), (2, "Bo")]
        .map(|(a, n)| (id(), vec![("actor_id", i(a)), ("name", s(n))]))
        .to_vec();
    let films = [(10, "Alien"), (11, "Heat")]
        .map(|(f, t)| (id(), vec![("film_id", i(f)), ("title", s(t))]))
        .to_vec();
    let pairs = film_actor
        .iter()
        .map(|(a, f)| (id(), vec![("actor_id", i(*a)), ("film_id", i(*f))]))
        .collect();
    let staff = [
        vec![("staff_id", i(1))],
        vec![("staff_id", i(2)), ("reports_to", i(1))],
        vec![("staff_id", i(3)), ("reports_to", i(2))],
    ]
    .map(|r| (id(), r))
    .to_vec();
    vec![
        ("actor", actors),
        ("film", films),
        ("film_actor", pairs),
        ("staff", staff),
    ]
}

/// The table instance of `data`.
fn table_of(data: &Rows) -> R<FInstance> {
    let mut f = FInstance::new();
    for (table, rows) in data {
        f = f.with_rows(*table, rows.iter().map(|(id, cells)| (*id, row(cells))))?;
    }
    Ok(f)
}

/// The tree of `data` under a root `db` node: a node per row, under the
/// root through the table's edge from `db`, and a valued node per cell,
/// anchored at the column vertex `<table>.<column>`, under its row through
/// the column's edge.
fn tree_of(data: &Rows) -> WInstance {
    let mut nodes = vec![Node::new(0, "db")];
    let mut arcs: Vec<(u32, u32, Edge)> = Vec::new();
    let mut next = 0_u32;
    for (table, rows) in data {
        for (_, cells) in rows {
            next += 1;
            let row_node = next;
            nodes.push(Node::new(row_node, *table));
            arcs.push((0, row_node, prop("db", table, table)));
            for (column, value) in cells {
                next += 1;
                let vertex = format!("{table}.{column}");
                nodes.push(valued(next, &vertex, value.clone()));
                arcs.push((row_node, next, prop(table, &vertex, column)));
            }
        }
    }
    let arcs: Vec<(u32, u32, &Edge)> = arcs.iter().map(|(p, c, e)| (*p, *c, e)).collect();
    tree(nodes, &arcs, vec![])
}

/// A schema of tables written as trees: `schema` with a root vertex `db`
/// and an edge from it to each of `tables`, named for the table.
fn under_db(schema: Schema, tables: &[&str]) -> Schema {
    let mut s = S(schema).vertex("db", "object");
    for t in tables {
        s = s.edge(&prop("db", t, t));
    }
    s.build()
}

/// The tables of the Sakila schema.
const SAKILA_TABLES: [&str; 4] = ["actor", "film", "film_actor", "staff"];

/// The pairs of the foreign key that resolves `holder`'s `column` against
/// `target`'s `target_column`: a pair from each holder row to the target
/// row whose `target_column` cell equals the holder row's `column` cell.
/// A holder row with no such cell, or with no matching target row, gets no
/// pair.
fn resolve(
    data: &Rows,
    holder: &str,
    column: &str,
    target: &str,
    target_column: &str,
) -> Vec<(RowId, RowId)> {
    fn rows<'a>(data: &'a Rows, table: &'a str) -> impl Iterator<Item = &'a (RowId, Cells)> {
        data.iter()
            .filter(move |(t, _)| *t == table)
            .flat_map(|(_, rows)| rows.iter())
    }
    let cell = |cells: &Cells, column: &str| {
        cells
            .iter()
            .find(|(c, _)| *c == column)
            .map(|(_, v)| v.clone())
    };
    rows(data, holder)
        .filter_map(|(id, cells)| {
            let value = cell(cells, column)?;
            rows(data, target)
                .find(|(_, target_cells)| {
                    cell(target_cells, target_column)
                        .is_some_and(|v| v.canonical_cmp(&value).is_eq())
                })
                .map(|(target_id, _)| (*id, *target_id))
        })
        .collect()
}

/// The Sakila table instance of `data` with every reference it can
/// resolve resolved into its foreign key: the semantic form of the rows,
/// which a graph-written protocol checks as edges.
fn resolved_sakila(data: &Rows) -> R<FInstance> {
    let mut f = table_of(data)?;
    for (id, holder, column, target, target_column) in [
        (
            "film_actor_actor",
            "film_actor",
            "actor_id",
            "actor",
            "actor_id",
        ),
        (
            "film_actor_film",
            "film_actor",
            "film_id",
            "film",
            "film_id",
        ),
        (
            "staff_reports_to",
            "staff",
            "reports_to",
            "staff",
            "staff_id",
        ),
    ] {
        let pairs = resolve(data, holder, column, target, target_column);
        if !pairs.is_empty() {
            f = f.with_foreign_key(edge(holder, target, "fk", id), pairs)?;
        }
    }
    Ok(f)
}

/// A key case: its name, how its film-actor references resolve, and its
/// film-actor pairs of actor and film ids.
type KeyCase = (&'static str, OnUnresolved, &'static [(i64, i64)]);

/// Keys and references: a Sakila composite key held and broken, a
/// self-referencing foreign key, and a dangling foreign key, strict and
/// lenient, each as a table instance, as a tree of rows and cells under a
/// root, and as a graph. The graph is written by a graph protocol, whose
/// semantic shape is its syntactic one, so it is the resolved form of the
/// rows: the graph form of the table instance with every reference that
/// resolves added as a foreign-key pair. A reference that does not resolve
/// leaves its holder without the edge, which a non-nullable reference
/// requires on the semantic shape whether it is strict or lenient.
fn keys() -> R<Vec<Case>> {
    let cases: [KeyCase; 5] = [
        (
            "sakila_composite_key",
            OnUnresolved::Error,
            &[(1, 10), (1, 11), (2, 10)],
        ),
        (
            "sakila_composite_key_duplicate",
            OnUnresolved::Error,
            &[(1, 10), (2, 11), (1, 10)],
        ),
        ("self_referencing_foreign_key", OnUnresolved::Error, &[]),
        (
            "dangling_foreign_key_strict",
            OnUnresolved::Error,
            &[(9, 10)],
        ),
        (
            "dangling_foreign_key_lenient",
            OnUnresolved::Complement,
            &[(9, 10)],
        ),
    ];
    let mut out = Vec::new();
    for (stem, on_unresolved, pairs) in cases {
        let schema = sakila(on_unresolved)?;
        let data = sakila_data(pairs);
        let table = table_of(&data)?;
        let (graph, _) = GInstance::from_finstance(&resolved_sakila(&data)?)?;
        let name = |prefix: &str| format!("keys/{prefix}{stem}");
        out.push(Case::new(
            name("graph_"),
            schema.clone(),
            Instance::Graph(graph),
        ));
        out.push(Case::new(
            name("tree_"),
            under_db(schema.clone(), &SAKILA_TABLES),
            Instance::Tree(tree_of(&data)),
        ));
        out.push(Case::new(name(""), schema, Instance::Table(table)));
    }
    out.extend(within_map_keys());
    Ok(out)
}

/// A self-embedding map-in-map schema whose entry key is unique within the
/// nearest enclosing map: maps hold entries, and an entry holds its key and
/// may hold a nested map as its value.
fn map_in_map() -> (Schema, Edge, Edge, Edge) {
    let entries = prop("map", "entry", "entries");
    let value = prop("entry", "map", "value");
    let key = prop("entry", "key", "key");
    let mut schema = S::default()
        .vertex("map", "object")
        .vertex("entry", "object")
        .vertex("key", "string")
        .edge(&entries)
        .edge(&value)
        .edge(&key)
        .build();
    schema.declarations.insert_key(
        "entry".into(),
        Key {
            id: "entry_key".into(),
            parts: vec![KeyPart::Path(vec![key.clone()])],
            role: KeyRole::Unique,
            scope: KeyScope::Within("map".into()),
        },
    );
    (schema, entries, value, key)
}

/// Keys scoped within the nearest enclosing map, as trees and graphs: an
/// equal key in an inner and an outer map (accepted), the same key twice in
/// one map (rejected), and entries with no enclosing map, which are outside
/// the key and reported by an advisory observation each (accepted).
fn within_map_keys() -> Vec<Case> {
    let (schema, entries, value, key) = map_in_map();
    let text = |id: u32| valued(id, "key", Value::Str("a".into()));
    let nested_nodes = || {
        vec![
            Node::new(0, "map"),
            Node::new(1, "entry"),
            text(2),
            Node::new(3, "map"),
            Node::new(4, "entry"),
            text(5),
        ]
    };
    let nested_arcs = [
        (0, 1, &entries),
        (1, 2, &key),
        (1, 3, &value),
        (3, 4, &entries),
        (4, 5, &key),
    ];
    let twice_nodes = || {
        vec![
            Node::new(0, "map"),
            Node::new(1, "entry"),
            text(2),
            Node::new(3, "entry"),
            text(4),
        ]
    };
    let twice_arcs = [
        (0, 1, &entries),
        (1, 2, &key),
        (0, 3, &entries),
        (3, 4, &key),
    ];
    // The root entry has no enclosing map; its nested map holds an entry
    // with the same key, which is in scope.
    let outside_nodes = || {
        vec![
            Node::new(0, "entry"),
            text(1),
            Node::new(2, "map"),
            Node::new(3, "entry"),
            text(4),
        ]
    };
    let outside_arcs = [(0, 1, &key), (0, 2, &value), (2, 3, &entries), (3, 4, &key)];
    let mut outside_graph = graph(outside_nodes(), &outside_arcs);
    // A second entry outside every map, with the same key as the root.
    outside_graph = outside_graph
        .with_node(Node::new(5, "entry"))
        .with_node(text(6))
        .with_edge(5, 6, key.clone());
    vec![
        Case::new(
            "keys/within_nested_maps_accept_an_equal_key",
            schema.clone(),
            Instance::Tree(tree(nested_nodes(), &nested_arcs, vec![])),
        ),
        Case::new(
            "keys/graph_within_nested_maps_accept_an_equal_key",
            schema.clone(),
            Instance::Graph(graph(nested_nodes(), &nested_arcs)),
        ),
        Case::new(
            "keys/within_one_map_rejects_a_repeated_key",
            schema.clone(),
            Instance::Tree(tree(twice_nodes(), &twice_arcs, vec![])),
        ),
        Case::new(
            "keys/graph_within_one_map_rejects_a_repeated_key",
            schema.clone(),
            Instance::Graph(graph(twice_nodes(), &twice_arcs)),
        ),
        Case::new(
            "keys/within_no_enclosing_map_is_outside_the_key",
            schema.clone(),
            Instance::Tree(tree(outside_nodes(), &outside_arcs, vec![])),
        ),
        Case::new(
            "keys/graph_within_no_enclosing_map_is_outside_the_key",
            schema,
            Instance::Graph(outside_graph),
        ),
    ]
}

/// The application check `audit`, about every item.
fn audit_options() -> ValidationOptions {
    let mut options = ValidationOptions::default();
    options.external.insert(
        "audit".into(),
        ExternalCheckDecl {
            name: "audit".into(),
            anchors: BTreeSet::from([Name::from("item")]),
            requirement: Requirement::Required,
        },
    );
    options
}

/// Reports that are not `Passed` because something was left undecided: a
/// budget too small for an acyclicity check, a sort the protocol does not
/// know, a pattern the regular-expression engine cannot compile, an
/// external check with no verdict and one with a rejecting verdict, a
/// failure beside an undecided check, and a source map the diagnostics
/// carry.
fn incomplete() -> Vec<Case> {
    let next = prop("item", "item", "next");
    let chain_schema = S::default()
        .vertex("item", "object")
        .edge(&next)
        .constraint("item", "acyclic", "next")
        .build();
    let chain = graph(
        (0..6).map(|i| Node::new(i, "item")).collect(),
        &(0..5).map(|i| (i, i + 1, &next)).collect::<Vec<_>>(),
    );
    let mut budget = Case::new(
        "incomplete/acyclic_budget",
        chain_schema,
        Instance::Graph(chain),
    );
    budget.budget = ValidationBudget { max_reads: Some(3) };

    let item = || graph(vec![Node::new(0, "item")], &[]);
    let colour = S::default()
        .vertex("item", "object")
        .constraint("item", "colour", "red")
        .build();
    let name = S::default()
        .vertex("name", "string")
        .constraint("name", "pattern", "(?=a)a")
        .build();
    let items = S::default().vertex("item", "object").build();
    let mut missing = Case::new(
        "incomplete/external_missing",
        items.clone(),
        Instance::Graph(item()),
    );
    missing.options = audit_options();
    let mut rejected = Case::new(
        "incomplete/external_rejected",
        items.clone(),
        Instance::Graph(item()),
    );
    rejected.options = audit_options();
    rejected.external.insert(
        "audit".into(),
        ViewRef::Node(0),
        ExternalVerdict {
            passed: false,
            witness: Digest([7; 32]),
        },
    );
    let mut mixed = Case::new(
        "incomplete/mixed_failure_and_incomplete",
        items,
        Instance::Graph(item().with_node(Node::new(1, "ghost"))),
    );
    mixed.options = audit_options();

    let bounded = S::default()
        .vertex("name", "string")
        .constraint("name", "maxLength", "3")
        .build();
    let sourced = Case::new(
        "incomplete/source_map",
        bounded,
        Instance::Graph(graph(
            vec![valued(0, "name", Value::Str("toolong".into()))],
            &[],
        )),
    );
    let sourced = with_source(sourced, "name");
    vec![
        budget,
        Case::new(
            "incomplete/unregistered_sort",
            colour,
            Instance::Graph(item()),
        ),
        Case::new(
            "incomplete/pattern_lookahead",
            name,
            Instance::Graph(graph(vec![valued(0, "name", Value::Str("a".into()))], &[])),
        ),
        missing,
        rejected,
        mixed,
        sourced,
    ]
}

/// The `a -> b` schema of the endpoint cases.
fn ab_schema() -> Schema {
    S::default()
        .vertex("a", "object")
        .vertex("b", "object")
        .edge(&prop("a", "b", "ab"))
        .build()
}

/// A tree of a root anchored at `root` and one child anchored at `child`,
/// under it through `label`.
fn two_node_tree(root: &str, child: &str, label: &Edge) -> WInstance {
    tree(
        vec![Node::new(0, root), Node::new(1, child)],
        &[(0, 1, label)],
        vec![],
    )
}

/// The anchor and endpoint cases of trees and tables beside the cases
/// above: an undeclared arc label, a coherent arc, and arcs incoherent on
/// the source side and on both sides; a coherent foreign-key pair. A table
/// instance cannot hold a pair between rows of the wrong tables
/// (construction refuses it), and a tree arc cannot be a self-loop.
fn tree_and_table_anchors_and_endpoints() -> R<Vec<Case>> {
    let ab = prop("a", "b", "ab");
    let tree_case = |name: &str, root: &str, child: &str, label: &Edge| {
        Case::new(
            name,
            ab_schema(),
            Instance::Tree(two_node_tree(root, child, label)),
        )
    };
    Ok(vec![
        tree_case(
            "anchors/tree_undeclared_label",
            "a",
            "b",
            &prop("a", "b", "undeclared"),
        ),
        tree_case("endpoints/tree_coherent", "a", "b", &ab),
        tree_case("endpoints/tree_src_side", "b", "b", &ab),
        tree_case("endpoints/tree_both_sides", "b", "a", &ab),
        table_coherent()?,
    ])
}

/// A foreign-key pair from a row of table `a` to a row of table `b` under
/// the foreign key `ab: a -> b`.
fn table_coherent() -> R<Case> {
    let ab = edge("a", "b", "fk", "ab");
    let schema = S::default()
        .vertex("a", "table")
        .vertex("b", "table")
        .edge(&ab)
        .build();
    let pair = FInstance::new()
        .with_rows("a", [(RowId(0), row(&[]))])
        .and_then(|f| f.with_rows("b", [(RowId(1), row(&[]))]))
        .and_then(|f| f.with_foreign_key(ab, [(RowId(0), RowId(1))]))?;
    Ok(Case::new(
        "endpoints/table_coherent",
        schema,
        Instance::Table(pair),
    ))
}

/// Required edges present and absent, and out-degree bounds at and past the
/// boundary, in trees (a parent's arcs) and table instances (a row's
/// foreign-key pairs). Parallel edges are graph cases only: a tree child
/// has one arc into it, and a foreign key holds a pair of rows once.
fn tree_and_table_cardinality() -> R<Vec<Case>> {
    let child = prop("parent", "child", "child");
    let base = || {
        S::default()
            .vertex("parent", "object")
            .vertex("child", "object")
            .edge(&child)
    };
    let required = base().required("parent", &child).build();
    let at_most = base().constraint("parent", "maxOut", "2").build();
    let at_least = base().constraint("parent", "minOut", "1").build();
    let fan_out = |k: u32| {
        let mut nodes = vec![Node::new(0, "parent")];
        nodes.extend((1..=k).map(|i| Node::new(i, "child")));
        let arcs: Vec<(u32, u32, &Edge)> = (1..=k).map(|i| (0, i, &child)).collect();
        tree(nodes, &arcs, vec![])
    };
    let stars = edge("film", "actor", "fk", "stars");
    let films = || {
        S::default()
            .vertex("film", "table")
            .vertex("actor", "table")
            .edge(&stars)
    };
    let starred = |k: u32| -> R<FInstance> {
        let actors: Vec<_> = (1..=3).map(|i| (RowId(i), row(&[]))).collect();
        let pairs: Vec<_> = (1..=k).map(|i| (RowId(0), RowId(i))).collect();
        let f = FInstance::new()
            .with_rows("film", [(RowId(0), row(&[]))])
            .and_then(|f| f.with_rows("actor", actors))?;
        Ok(if pairs.is_empty() {
            f
        } else {
            f.with_foreign_key(stars.clone(), pairs)?
        })
    };
    let table_required = films().required("film", &stars).build();
    let table_at_most = films().constraint("film", "maxOut", "2").build();
    let table_at_least = films().constraint("film", "minOut", "1").build();
    let tree_case = |name: &str, schema: &Schema, k: u32| {
        Case::new(name, schema.clone(), Instance::Tree(fan_out(k)))
    };
    let table_case = |name: &str, schema: &Schema, k: u32| -> R<Case> {
        Ok(Case::new(
            name,
            schema.clone(),
            Instance::Table(starred(k)?),
        ))
    };
    Ok(vec![
        tree_case("cardinality/tree_required_edge_present", &required, 1),
        tree_case("cardinality/tree_required_edge_absent", &required, 0),
        tree_case("cardinality/tree_degree_at_upper_bound", &at_most, 2),
        tree_case("cardinality/tree_degree_above_upper_bound", &at_most, 3),
        tree_case("cardinality/tree_degree_at_lower_bound", &at_least, 1),
        tree_case("cardinality/tree_degree_below_lower_bound", &at_least, 0),
        table_case(
            "cardinality/table_required_foreign_key_present",
            &table_required,
            1,
        )?,
        table_case("cardinality/table_degree_at_upper_bound", &table_at_most, 2)?,
        table_case(
            "cardinality/table_degree_above_upper_bound",
            &table_at_most,
            3,
        )?,
        table_case(
            "cardinality/table_degree_at_lower_bound",
            &table_at_least,
            1,
        )?,
        table_case(
            "cardinality/table_degree_below_lower_bound",
            &table_at_least,
            0,
        )?,
    ])
}

/// Record that the `maxLength 3` constraint on `vertex` was read from bytes
/// 12 to 27 of `schemas/name.json`.
fn with_source(mut case: Case, vertex: &str) -> Case {
    case.options.sources.0.insert(
        SchemaRef::Constraint {
            vertex: vertex.into(),
            constraint: Constraint {
                sort: "maxLength".into(),
                value: "3".into(),
            },
        },
        SourceLocation {
            source: "schemas/name.json".into(),
            start_byte: 12,
            end_byte: 27,
        },
    );
    case
}

/// A case checked by the application check `audit`.
fn audited(mut case: Case) -> Case {
    case.options = audit_options();
    case
}

/// A case checked by `audit`, whose verdict on `at` rejects it.
fn rejecting(case: Case, at: ViewRef) -> Case {
    let mut case = audited(case);
    case.external.insert(
        "audit".into(),
        at,
        ExternalVerdict {
            passed: false,
            witness: Digest([7; 32]),
        },
    );
    case
}

/// A case validated with a budget of three reads.
const fn budgeted(mut case: Case) -> Case {
    case.budget = ValidationBudget { max_reads: Some(3) };
    case
}

/// An `item` vertex of kind `kind` with a `name` it reaches through the
/// edge `name`, the name carrying the constraint `sort = value` if any.
fn named_schema(kind: &str, constraint: Option<(&str, &str)>) -> Schema {
    let mut b = S::default()
        .vertex("item", kind)
        .vertex("name", "string")
        .edge(&prop("item", "name", "name"));
    if let Some((sort, value)) = constraint {
        b = b.constraint("name", sort, value);
    }
    b.build()
}

/// An `item` vertex of kind `kind` carrying the unregistered sort
/// `colour`.
fn colour_schema(kind: &str) -> Schema {
    S::default()
        .vertex("item", kind)
        .constraint("item", "colour", "red")
        .build()
}

/// An `item` vertex of kind `kind`.
fn item_schema(kind: &str) -> Schema {
    S::default().vertex("item", kind).build()
}

/// A length bound of three on a name.
const BOUNDED: Option<(&str, &str)> = Some(("maxLength", "3"));

/// A pattern with a lookahead, which the regular-expression engine cannot
/// compile.
const LOOKAHEAD: Option<(&str, &str)> = Some(("pattern", "(?=a)a"));

/// The incompleteness cases of trees: a budget too small for the checks of
/// a six-node chain, a sort the protocol does not know, a pattern the
/// regular-expression engine cannot compile, an external check with no
/// verdict and one with a rejecting verdict, a failure beside an undecided
/// check, and a source map the diagnostics carry. Acyclicity is a graph
/// check, so the budget runs out on the checks trees have.
fn tree_incomplete() -> Vec<Case> {
    let next = prop("item", "item", "next");
    let name = prop("item", "name", "name");
    let item = || tree(vec![Node::new(0, "item")], &[], vec![]);
    let named = |value: &str| {
        tree(
            vec![
                Node::new(0, "item"),
                valued(1, "name", Value::Str(value.into())),
            ],
            &[(0, 1, &name)],
            vec![],
        )
    };
    let chain = tree(
        (0..6).map(|i| Node::new(i, "item")).collect(),
        &(0..5).map(|i| (i, i + 1, &next)).collect::<Vec<_>>(),
        vec![],
    );
    let chain_schema = S::default().vertex("item", "object").edge(&next).build();
    let case =
        |name: &str, schema: Schema, t: WInstance| Case::new(name, schema, Instance::Tree(t));
    vec![
        budgeted(case("incomplete/tree_budget", chain_schema, chain)),
        case(
            "incomplete/tree_unregistered_sort",
            colour_schema("object"),
            item(),
        ),
        case(
            "incomplete/tree_pattern_lookahead",
            named_schema("object", LOOKAHEAD),
            named("a"),
        ),
        audited(case(
            "incomplete/tree_external_missing",
            item_schema("object"),
            item(),
        )),
        rejecting(
            case(
                "incomplete/tree_external_rejected",
                item_schema("object"),
                item(),
            ),
            ViewRef::Node(0),
        ),
        audited(case(
            "incomplete/tree_mixed_failure_and_incomplete",
            named_schema("object", BOUNDED),
            named("toolong"),
        )),
        with_source(
            case(
                "incomplete/tree_source_map",
                named_schema("object", BOUNDED),
                named("toolong"),
            ),
            "name",
        ),
    ]
}

/// The incompleteness cases of [`tree_incomplete`], as table instances:
/// the budget runs out on six rows of `item`, and the name is a column.
fn table_incomplete() -> R<Vec<Case>> {
    let item = |cells: &[(&str, Value)]| -> R<Instance> {
        Ok(Instance::Table(
            FInstance::new().with_rows("item", [(RowId(0), row(cells))])?,
        ))
    };
    let named = |value: &str| item(&[("name", Value::Str(value.into()))]);
    let six_rows = FInstance::new().with_rows("item", (0..6).map(|i| (RowId(i), row(&[]))))?;
    Ok(vec![
        budgeted(Case::new(
            "incomplete/table_budget",
            item_schema("table"),
            Instance::Table(six_rows),
        )),
        Case::new(
            "incomplete/table_unregistered_sort",
            colour_schema("table"),
            item(&[])?,
        ),
        Case::new(
            "incomplete/table_pattern_lookahead",
            named_schema("table", LOOKAHEAD),
            named("a")?,
        ),
        audited(Case::new(
            "incomplete/table_external_missing",
            item_schema("table"),
            item(&[])?,
        )),
        rejecting(
            Case::new(
                "incomplete/table_external_rejected",
                item_schema("table"),
                item(&[])?,
            ),
            ViewRef::Row(RowId(0)),
        ),
        audited(Case::new(
            "incomplete/table_mixed_failure_and_incomplete",
            named_schema("table", BOUNDED),
            named("toolong")?,
        )),
        with_source(
            Case::new(
                "incomplete/table_source_map",
                named_schema("table", BOUNDED),
                named("toolong")?,
            ),
            "name",
        ),
    ])
}

/// Replace, anywhere in `value`, the number `from` under the key `key` with
/// `to`.
fn patch(value: &mut serde_json::Value, key: &str, from: u64, to: u64) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if k == key && v.as_u64() == Some(from) {
                    *v = serde_json::Value::from(to);
                } else {
                    patch(v, key, from, to);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                patch(v, key, from, to);
            }
        }
        _ => {}
    }
}

/// Instances decoding refuses, with the error each refusal gives: an edge
/// to a node the graph does not hold, and a foreign-key pair to a row the
/// instance does not hold.
fn rejections() -> R<Vec<RejectionCase>> {
    let ab = prop("a", "b", "ab");
    let well_formed = graph(vec![Node::new(0, "a"), Node::new(1, "b")], &[(0, 1, &ab)]);
    let dangling_edge = WireForms::of(&well_formed, redirect_edge_targets)?;
    let edge_error = dangling_edge.refusal::<GInstance>()?;
    let mut dangling_node_link = to_node_link_json(&well_formed);
    let before = dangling_node_link.clone();
    patch(&mut dangling_node_link, "tgt", 1, 99);
    if dangling_node_link == before {
        return Err("the edge's target was not found in the node-link form".into());
    }
    let node_link_error = from_node_link_json(&dangling_node_link)
        .err()
        .ok_or("a node-link graph with a dangling edge decodes")?;
    if node_link_error.to_string() != edge_error {
        return Err(format!(
            "the node-link decoder refuses the dangling edge with `{node_link_error}`, \
             the instance decoder with `{edge_error}`"
        )
        .into());
    }
    let stars = edge("film", "actor", "fk", "stars");
    let films = FInstance::new()
        .with_rows("film", [(RowId(0), row(&[]))])
        .and_then(|f| f.with_rows("actor", [(RowId(1), row(&[]))]))
        .and_then(|f| f.with_foreign_key(stars, [(RowId(0), RowId(1))]))?;
    let dangling_pair = WireForms::of(&films, redirect_pair_targets)?;
    let pair_error = dangling_pair.refusal::<FInstance>()?;
    Ok(vec![
        RejectionCase {
            name: "anchors/dangling_edge.rejection".into(),
            shape: InstanceShapeKind::Graph,
            forms: dangling_edge,
            error: edge_error,
            node_link: Some(dangling_node_link),
        },
        RejectionCase {
            name: "anchors/dangling_foreign_key.rejection".into(),
            shape: InstanceShapeKind::Table,
            forms: dangling_pair,
            error: pair_error,
            node_link: None,
        },
    ])
}

/// The three stored forms of a rejected instance: its JSON, named
/// `MessagePack` and CBOR wire forms, each read as a generic CBOR value so
/// that a map keeps the key type its own encoding gives it (a string in
/// JSON, an integer in `MessagePack` and CBOR).
struct WireForms {
    json: ciborium::Value,
    msgpack: ciborium::Value,
    cbor: ciborium::Value,
}

impl WireForms {
    /// The wire forms of `instance`, each with `redirect` applied, which
    /// must change it.
    fn of<T: Serialize>(instance: &T, redirect: fn(&mut ciborium::Value) -> bool) -> R<Self> {
        let mut cbor_bytes = Vec::new();
        ciborium::into_writer(instance, &mut cbor_bytes)?;
        let mut forms = Self {
            json: serde_json::from_slice(&serde_json::to_vec(instance)?)?,
            msgpack: rmp_serde::from_slice(&rmp_serde::to_vec_named(instance)?)?,
            cbor: ciborium::from_reader(cbor_bytes.as_slice())?,
        };
        for (encoding, form) in [
            ("JSON", &mut forms.json),
            ("MessagePack", &mut forms.msgpack),
            ("CBOR", &mut forms.cbor),
        ] {
            sort_maps(form);
            if !redirect(form) {
                return Err(format!("nothing to redirect in the {encoding} wire form").into());
            }
        }
        Ok(forms)
    }

    /// The error decoding each form as a `T` in its own encoding gives,
    /// requiring every form to be refused with the same error text as the
    /// JSON form, which is returned.
    fn refusal<T: serde::de::DeserializeOwned>(&self) -> R<String> {
        let json = serde_json::from_slice::<T>(&serde_json::to_vec(&self.json)?)
            .err()
            .ok_or("the JSON wire form decodes")?;
        let position = format!(" at line {} column {}", json.line(), json.column());
        let json = json.to_string();
        let json = json.strip_suffix(&position).unwrap_or(&json).to_owned();
        let msgpack = rmp_serde::from_slice::<T>(&rmp_serde::to_vec_named(&self.msgpack)?)
            .err()
            .ok_or("the MessagePack wire form decodes")?
            .to_string();
        let mut cbor_bytes = Vec::new();
        ciborium::into_writer(&self.cbor, &mut cbor_bytes)?;
        let cbor = match ciborium::from_reader::<T, _>(cbor_bytes.as_slice()) {
            Ok(_) => return Err("the CBOR wire form decodes".into()),
            Err(ciborium::de::Error::Semantic(_, message)) => message,
            Err(e) => return Err(format!("the CBOR wire form is malformed: {e}").into()),
        };
        if msgpack != json || cbor != json {
            return Err(format!(
                "the encodings refuse the instance differently: JSON `{json}`, MessagePack \
                 `{msgpack}`, CBOR `{cbor}`"
            )
            .into());
        }
        Ok(json)
    }
}

/// Sort the entries of every map in `value` by key, strings before
/// integers and each in ascending order, so that a wire form read from a
/// hash map is written the same way every time.
fn sort_maps(value: &mut ciborium::Value) {
    match value {
        ciborium::Value::Map(entries) => {
            entries.sort_by(|(a, _), (b, _)| {
                let key = |k: &ciborium::Value| {
                    (
                        k.as_integer().map(i128::from),
                        k.as_text().map(str::to_owned),
                    )
                };
                key(a).cmp(&key(b))
            });
            for (_, v) in entries {
                sort_maps(v);
            }
        }
        ciborium::Value::Array(items) => items.iter_mut().for_each(sort_maps),
        _ => {}
    }
}

/// Whether `value` is the integer `n`.
fn is_integer(value: &ciborium::Value, n: u64) -> bool {
    value
        .as_integer()
        .is_some_and(|i| i128::from(i) == i128::from(n))
}

/// Point every edge of a graph's wire form whose target is node 1 at node
/// 99, which the graph does not hold; whether any edge was redirected.
fn redirect_edge_targets(value: &mut ciborium::Value) -> bool {
    match value {
        ciborium::Value::Map(entries) => {
            let mut changed = false;
            for (k, v) in entries.iter_mut() {
                if k.as_text() == Some("tgt") && is_integer(v, 1) {
                    *v = ciborium::Value::from(99u64);
                    changed = true;
                } else {
                    changed |= redirect_edge_targets(v);
                }
            }
            changed
        }
        ciborium::Value::Array(items) => items
            .iter_mut()
            .fold(false, |changed, v| redirect_edge_targets(v) | changed),
        _ => false,
    }
}

/// Point the target of every foreign-key pair of a table instance's wire
/// form (`foreign_keys`: a list of `[edge, [[src, tgt], ...]]`) at row 99,
/// which no table holds; whether any pair was redirected.
fn redirect_pair_targets(value: &mut ciborium::Value) -> bool {
    let Some(entries) = value.as_map_mut() else {
        return false;
    };
    let mut changed = false;
    for (k, v) in entries.iter_mut() {
        if k.as_text() != Some("foreign_keys") {
            continue;
        }
        for entry in v.as_array_mut().into_iter().flatten() {
            let Some(pairs) = entry.as_array_mut().and_then(|e| e.get_mut(1)) else {
                continue;
            };
            for pair in pairs.as_array_mut().into_iter().flatten() {
                if let Some(tgt) = pair.as_array_mut().and_then(|p| p.get_mut(1)) {
                    *tgt = ciborium::Value::from(99u64);
                    changed = true;
                }
            }
        }
    }
    changed
}

/// Write `value` to `<out>/<name>.named.msgpack`, `.cbor` and `.json`.
fn write_three<T: Serialize>(out: &Path, name: &str, value: &T) -> Result<(), Box<dyn Error>> {
    let path = |ext: &str| out.join(format!("{name}.{ext}"));
    if let Some(dir) = path("json").parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path("named.msgpack"), rmp_serde::to_vec_named(value)?)?;
    let mut cbor = Vec::new();
    ciborium::into_writer(value, &mut cbor)?;
    std::fs::write(path("cbor"), cbor)?;
    let mut json = serde_json::to_string_pretty(value)?;
    json.push('\n');
    std::fs::write(path("json"), json)?;
    Ok(())
}

/// Write a node-link JSON document to `<out>/<name>.nodelink.json`.
fn write_node_link(out: &Path, name: &str, document: &serde_json::Value) -> R<()> {
    let mut json = serde_json::to_string_pretty(document)?;
    json.push('\n');
    std::fs::write(out.join(format!("{name}.nodelink.json")), json)?;
    Ok(())
}

/// Write the corpus to the directory the first argument names, or to the
/// repository's `tests/fixtures/validation`.
pub fn run() -> Result<(), Box<dyn Error>> {
    let out = std::env::args_os().nth(1).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/validation"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&out)?;
    let mut names = BTreeSet::new();
    let cases = [
        anchors()?,
        endpoints(),
        endpoint_edits()?,
        values()?,
        cardinality()?,
        ordering()?,
        cycles(),
        cycle_edits()?,
        fans()?,
        keys()?,
        incomplete(),
        tree_and_table_anchors_and_endpoints()?,
        tree_and_table_cardinality()?,
        tree_incomplete(),
        table_incomplete()?,
        boundary()?,
        train_benchmark()?,
    ];
    for case in cases.into_iter().flatten() {
        let fixture = case.validate()?;
        if !names.insert(fixture.name.clone()) {
            return Err(format!("fixture {} is listed twice", fixture.name).into());
        }
        write_three(&out, &fixture.name, &fixture)?;
        if let Instance::Graph(g) = &fixture.instance {
            write_node_link(&out, &fixture.name, &to_node_link_json(g))?;
        }
    }
    for rejection in rejections()? {
        rejection.write(&out)?;
    }
    Ok(())
}
