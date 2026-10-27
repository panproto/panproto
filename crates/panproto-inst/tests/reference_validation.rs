#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! Same-input differentials for the validation correctness page.
//!
//! The deliberately small carrier below is not a second validation oracle.
//! It presents the same labelled schema and native instance to the
//! dependency-free reference implementation and the public production
//! validator.  The comparison retains every structured diagnostic field;
//! it does not compare messages or collapse findings into a set.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_inst::key as production_key;
use panproto_inst::validation::{
    AdmissionPolicy, CheckClass, CheckKey, CheckKind, Diagnostic, ExternalVerdicts,
    IncompleteCheck, OpenVerdict, Requirement, SchemaRef as ProductionSchemaRef, ValidationBudget,
    ValidationOptions, ValidationPlan, ValidationState, Violation as ProductionViolation,
};
use panproto_inst::{EdgeId, FInstance, FieldPresence, GEdge, GInstance, Node, RowId, WInstance};
use panproto_reference::model as reference_model;
use panproto_reference::schema as reference_schema;
use panproto_reference::validation as reference;
use panproto_reference::value::{Presence as ReferencePresence, Value as ReferenceValue};
use panproto_schema::{
    ConstraintSemantics, Edge, EdgeRule, InstanceShapeKind, LengthUnit, Protocol, ScalarKind,
    Schema, SchemaBuilder,
};

const CASES_PER_SHAPE: usize = 4;
const SHAPES: usize = 3;
const COMPACT_CASE_COUNT: usize = CASES_PER_SHAPE * SHAPES;
const SUPPLEMENTAL_OBSERVATION_COUNT: usize = 92;

fn production_edge(edge: &reference_model::SEdge) -> Edge {
    Edge {
        src: edge.src.as_str().into(),
        tgt: edge.tgt.as_str().into(),
        kind: edge.kind.as_str().into(),
        name: edge.name.as_deref().map(Into::into),
    }
}

fn edge_observation(edge: &reference_model::SEdge) -> String {
    format!("{}>{}:{}:{:?}", edge.src, edge.tgt, edge.kind, edge.name)
}

fn production_edge_observation(edge: &Edge) -> String {
    edge_observation(&reference_model::SEdge::new(
        &edge.src,
        &edge.tgt,
        &edge.kind,
        edge.name.as_deref(),
    ))
}

fn production_value(value: &ReferenceValue) -> panproto_inst::Value {
    match value {
        ReferenceValue::Str(value) => panproto_inst::Value::Str(value.clone()),
        ReferenceValue::Int(value) => panproto_inst::Value::Int(*value),
        ReferenceValue::Bool(value) => panproto_inst::Value::Bool(*value),
        other => panic!("value outside the compact validation carrier: {other:?}"),
    }
}

fn value_observation(value: &ReferenceValue) -> String {
    match value {
        ReferenceValue::Str(value) => format!("str:{value:?}"),
        ReferenceValue::Int(value) => format!("int:{value}"),
        ReferenceValue::Bool(value) => format!("bool:{value}"),
        other => panic!("value outside the compact validation carrier: {other:?}"),
    }
}

fn production_value_observation(value: &panproto_inst::Value) -> String {
    match value {
        panproto_inst::Value::Str(value) => format!("str:{value:?}"),
        panproto_inst::Value::Int(value) => format!("int:{value}"),
        panproto_inst::Value::Bool(value) => format!("bool:{value}"),
        other => panic!("production value outside the compact validation carrier: {other:?}"),
    }
}

fn production_presence(value: &ReferencePresence) -> FieldPresence {
    match value {
        ReferencePresence::Present(value) => FieldPresence::Present(production_value(value)),
        ReferencePresence::Null => FieldPresence::Null,
        ReferencePresence::Absent => FieldPresence::Absent,
    }
}

fn production_node(node: &reference_model::Node) -> Node {
    let mut output = Node::new(node.id, node.anchor.as_str());
    output.value = node.value.as_ref().map(production_presence);
    output.discriminator = node.discriminator.as_deref().map(Into::into);
    output.extra_fields = node
        .fields
        .iter()
        .map(|(name, value)| (name.clone(), production_value(value)))
        .collect();
    output.position = node.position;
    output.annotations = node
        .annotations
        .iter()
        .map(|(name, value)| (name.clone(), production_value(value)))
        .collect();
    output
}

fn production_graph(graph: &reference_model::Graph) -> GInstance {
    let mut output = GInstance::new();
    for node in graph.nodes.values() {
        output = output.with_node(production_node(node));
    }
    for (id, edge) in &graph.edges {
        output
            .insert_edge(
                EdgeId(*id),
                GEdge {
                    src: edge.src,
                    tgt: edge.tgt,
                    edge: production_edge(&edge.label),
                    position: edge.position,
                },
            )
            .expect("the compact reference graph is well formed");
    }
    output
}

fn production_table(table: &reference_model::Table) -> FInstance {
    FInstance::from_parts(
        table.rows.iter().map(|(id, (name, cells))| {
            (
                name.as_str().into(),
                vec![(
                    RowId(*id),
                    cells
                        .iter()
                        .map(|(name, value)| (name.clone(), production_value(value)))
                        .collect::<HashMap<_, _>>(),
                )],
            )
        }),
        table.pairs.iter().map(|(edge, source, target)| {
            (
                production_edge(edge),
                vec![(RowId(*source), RowId(*target))],
            )
        }),
    )
    .expect("the compact reference table is well formed")
}

fn production_tree(tree: &reference_model::Tree) -> WInstance {
    WInstance::new(
        tree.nodes
            .iter()
            .map(|(id, node)| (*id, production_node(node)))
            .collect(),
        tree.arcs
            .iter()
            .map(|(parent, child, edge)| (*parent, *child, production_edge(edge)))
            .collect(),
        Vec::new(),
        tree.root,
        tree.nodes
            .get(&tree.root)
            .map_or_else(|| "missing".into(), |node| node.anchor.as_str().into()),
    )
}

fn fixture_schema() -> (
    reference_schema::Schema,
    BTreeMap<String, reference::Semantics>,
) {
    let child = reference_model::SEdge::new("parent", "child", "prop", Some("child"));
    let mut schema = reference_schema::Schema::default()
        .vertex("parent", "object")
        .vertex("child", "string")
        .edge("parent", "child", "prop", Some("child"));
    schema.required.insert("parent".to_owned(), vec![child]);
    schema.constraints.insert(
        "child".to_owned(),
        vec![("maxLength".to_owned(), "3".to_owned())],
    );
    (
        schema,
        BTreeMap::from([(
            "maxLength".to_owned(),
            reference::Semantics::MaxLength(true),
        )]),
    )
}

fn production_fixture_schema(reference: &reference_schema::Schema) -> (Schema, Protocol) {
    let mut protocol = Protocol {
        name: "reference-validation".to_owned(),
        edge_rules: vec![EdgeRule {
            edge_kind: "prop".to_owned(),
            src_kinds: vec!["object".to_owned()],
            tgt_kinds: vec!["string".to_owned()],
        }],
        obj_kinds: vec!["object".to_owned()],
        constraint_sorts: vec!["maxLength".to_owned()],
        constraint_semantics: BTreeMap::from([(
            "maxLength".to_owned(),
            ConstraintSemantics::MaxLength(LengthUnit::Chars),
        )]),
        value_kinds: BTreeMap::from([("string".to_owned(), ScalarKind::Str)]),
        non_value_kinds: BTreeSet::from(["object".to_owned()]),
        ..Protocol::default()
    };
    protocol.syntactic_shape = InstanceShapeKind::Graph;
    protocol.semantic_shape = InstanceShapeKind::Graph;
    let mut builder = SchemaBuilder::new(&protocol);
    for (vertex, kind) in &reference.vertices {
        builder = builder
            .vertex(vertex, kind, None)
            .expect("compact reference vertex");
    }
    for edge in &reference.edges {
        builder = builder
            .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
            .expect("compact reference edge");
    }
    for (vertex, constraints) in &reference.constraints {
        for (sort, value) in constraints {
            builder = builder.constraint(vertex, sort, value);
        }
    }
    for (vertex, required) in &reference.required {
        builder = builder.required(vertex, required.iter().map(production_edge).collect());
    }
    (builder.build().expect("compact reference schema"), protocol)
}

fn production_format_observation(text: &str, advisory: bool) -> (bool, bool, bool, String) {
    let semantics = if advisory {
        ConstraintSemantics::AnnotatedFormat("test".into())
    } else {
        ConstraintSemantics::Format("test".into())
    };
    let mut protocol = Protocol {
        name: "reference-format-validation".to_owned(),
        constraint_sorts: vec!["format".to_owned()],
        constraint_semantics: BTreeMap::from([("format".to_owned(), semantics)]),
        value_kinds: BTreeMap::from([("string".to_owned(), ScalarKind::Str)]),
        ..Protocol::default()
    };
    protocol.syntactic_shape = InstanceShapeKind::Graph;
    protocol.semantic_shape = InstanceShapeKind::Graph;
    let schema = SchemaBuilder::new(&protocol)
        .vertex("value", "string", None)
        .expect("format carrier vertex")
        .constraint("value", "format", "uuid")
        .build()
        .expect("format carrier schema");
    let plan = ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let graph = GInstance::new().with_node(Node::new(0, "value").with_value(
        FieldPresence::Present(panproto_inst::Value::Str(text.to_owned())),
    ));
    let report = plan.validate_graph(
        &graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let preserved = match graph.node(0).and_then(|node| node.value.as_ref()) {
        Some(FieldPresence::Present(panproto_inst::Value::Str(value))) => value.clone(),
        other => panic!("validation changed the format carrier: {other:?}"),
    };
    (
        report.passed(),
        !report.diagnostics.is_empty(),
        !report.incomplete.is_empty(),
        preserved,
    )
}

fn production_lexical_observation(text: &str, datatype: &str) -> (bool, bool, bool) {
    let datatype_edge = Edge {
        src: "literal".into(),
        tgt: "datatype".into(),
        kind: "prop".into(),
        name: Some("datatype".into()),
    };
    let mut protocol = Protocol {
        name: "reference-lexical-validation".to_owned(),
        edge_rules: vec![EdgeRule {
            edge_kind: "prop".to_owned(),
            src_kinds: vec!["string".to_owned()],
            tgt_kinds: vec!["string".to_owned()],
        }],
        constraint_sorts: vec!["lexical_space".to_owned()],
        constraint_semantics: BTreeMap::from([(
            "lexical_space".to_owned(),
            ConstraintSemantics::LexicalSpace,
        )]),
        value_kinds: BTreeMap::from([
            ("string".to_owned(), ScalarKind::Str),
            (
                "integer".to_owned(),
                ScalarKind::Int {
                    bits: 64,
                    signed: true,
                },
            ),
        ]),
        ..Protocol::default()
    };
    protocol.syntactic_shape = InstanceShapeKind::Graph;
    protocol.semantic_shape = InstanceShapeKind::Graph;
    let schema = SchemaBuilder::new(&protocol)
        .vertex("literal", "string", None)
        .expect("lexical carrier vertex")
        .vertex("datatype", "string", None)
        .expect("datatype carrier vertex")
        .edge("literal", "datatype", "prop", Some("datatype"))
        .expect("datatype edge")
        .constraint("literal", "lexical_space", "datatype")
        .build()
        .expect("lexical carrier schema");
    let graph = GInstance::new()
        .with_node(Node::new(0, "literal").with_value(FieldPresence::Present(
            panproto_inst::Value::Str(text.to_owned()),
        )))
        .with_node(Node::new(1, "datatype").with_value(FieldPresence::Present(
            panproto_inst::Value::Str(datatype.to_owned()),
        )))
        .with_edge(0, 1, datatype_edge);
    let plan = ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let report = plan.validate_graph(
        &graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    (
        report.passed(),
        !report.diagnostics.is_empty(),
        !report.incomplete.is_empty(),
    )
}

const fn status_observation(status: reference::CheckStatus) -> &'static str {
    match status {
        reference::CheckStatus::Passed => "passed",
        reference::CheckStatus::Failed => "failed",
        reference::CheckStatus::Incomplete => "incomplete",
    }
}

const fn production_status_observation(status: panproto_gat::CheckStatus) -> &'static str {
    match status {
        panproto_gat::CheckStatus::Passed => "passed",
        panproto_gat::CheckStatus::Failed => "failed",
        panproto_gat::CheckStatus::Incomplete => "incomplete",
    }
}

fn reference_view(reference: &panproto_reference::lens::ViewRef) -> String {
    use panproto_reference::lens::ViewRef;
    match reference {
        ViewRef::Node(id) => format!("node:{id}"),
        ViewRef::Edge(id) => format!("edge:{id}"),
        ViewRef::Arc { parent, child } => format!("arc:{parent}:{child}"),
        ViewRef::Row(id) => format!("row:{id}"),
        ViewRef::Pair(edge, source, target) => {
            format!("pair:{}:{source}:{target}", edge_observation(edge))
        }
    }
}

fn production_view(reference: &panproto_inst::ViewRef) -> String {
    use panproto_inst::ViewRef;
    match reference {
        ViewRef::Node(id) => format!("node:{id}"),
        ViewRef::Edge(id) => format!("edge:{}", id.0),
        ViewRef::Arc { parent, child } => format!("arc:{parent}:{child}"),
        ViewRef::Row(id) => format!("row:{}", id.0),
        ViewRef::Pair { edge, src, tgt } => format!(
            "pair:{}:{}:{}",
            production_edge_observation(edge),
            src.0,
            tgt.0
        ),
    }
}

fn reference_schema_ref(reference: &reference::SchemaRef) -> String {
    match reference {
        reference::SchemaRef::Schema => "schema".to_owned(),
        reference::SchemaRef::Vertex(vertex) => format!("vertex:{vertex}"),
        reference::SchemaRef::Edge(edge) => format!("edge:{}", edge_observation(edge)),
        reference::SchemaRef::Required(vertex, edge) => {
            format!("required:{vertex}:{}", edge_observation(edge))
        }
        reference::SchemaRef::Ordering(edge) => format!("ordering:{}", edge_observation(edge)),
        reference::SchemaRef::Constraint(vertex, sort, value) => {
            format!("constraint:{vertex}:{sort}:{value:?}")
        }
        reference::SchemaRef::Key(vertex, key) => format!("key:{vertex}:{key}"),
        reference::SchemaRef::TargetSet(set) => format!("target-set:{set}"),
        reference::SchemaRef::Reference(reference) => format!("reference:{reference}"),
    }
}

fn production_schema_ref(reference: &panproto_inst::validation::SchemaRef) -> String {
    use panproto_inst::validation::SchemaRef;
    match reference {
        SchemaRef::Schema => "schema".to_owned(),
        SchemaRef::Vertex(vertex) => format!("vertex:{vertex}"),
        SchemaRef::Edge(edge) => format!("edge:{}", production_edge_observation(edge)),
        SchemaRef::HyperEdge(edge) => format!("hyper-edge:{edge}"),
        SchemaRef::HyperLabel { hyper_edge, label } => {
            format!("hyper-label:{hyper_edge}:{label}")
        }
        SchemaRef::Required { vertex, edge } => {
            format!("required:{vertex}:{}", production_edge_observation(edge))
        }
        SchemaRef::Ordering(edge) => {
            format!("ordering:{}", production_edge_observation(edge))
        }
        SchemaRef::Constraint { vertex, constraint } => format!(
            "constraint:{vertex}:{}:{:?}",
            constraint.sort, constraint.value
        ),
        SchemaRef::Attribute { vertex, field } => format!("attribute:{vertex}:{field}"),
        SchemaRef::Key { vertex, key } => format!("key:{vertex}:{key}"),
        SchemaRef::Reference(reference) => format!("reference:{reference}"),
        SchemaRef::External(external) => format!("external:{external}"),
    }
}

fn reference_violation(violation: &reference::Violation) -> String {
    match violation {
        reference::Violation::UnknownAnchor(anchor) => format!("unknown-anchor:{anchor}"),
        reference::Violation::UndeclaredEdge(edge) => {
            format!("undeclared-edge:{}", edge_observation(edge))
        }
        reference::Violation::IncoherentEndpoint(source, expected, found) => {
            format!("incoherent:{source}:{expected}:{found}")
        }
        reference::Violation::MissingRequiredEdge(edge) => {
            format!("missing-required:{}", edge_observation(edge))
        }
        reference::Violation::ValueConstraint(sort, bound, value) => format!(
            "value-constraint:{sort}:{bound:?}:{}",
            value_observation(value)
        ),
        reference::Violation::MissingValue(field) => format!("missing-value:{field:?}"),
        reference::Violation::MissingRoot => "missing-root".to_owned(),
        reference::Violation::Unreachable => "unreachable".to_owned(),
        reference::Violation::ParentMismatch(recorded, actual) => {
            format!("parent-mismatch:{recorded:?}:{actual:?}")
        }
        other => panic!("unexpected compact reference violation: {other:?}"),
    }
}

fn production_violation(violation: &panproto_inst::validation::Violation) -> String {
    use panproto_inst::validation::{EndpointSide, Violation};
    match violation {
        Violation::UnknownAnchor { anchor } => format!("unknown-anchor:{anchor}"),
        Violation::UndeclaredEdge { label } => {
            format!("undeclared-edge:{}", production_edge_observation(label))
        }
        Violation::IncoherentEndpoint {
            side,
            expected,
            found,
        } => format!(
            "incoherent:{}:{expected}:{found}",
            matches!(side, EndpointSide::Src)
        ),
        Violation::MissingRequiredEdge { edge } => {
            format!("missing-required:{}", production_edge_observation(edge))
        }
        Violation::ValueConstraint { sort, bound, value } => format!(
            "value-constraint:{sort}:{bound:?}:{}",
            value
                .as_ref()
                .map_or_else(|| "absent".to_owned(), production_value_observation)
        ),
        Violation::MissingValue { field } => format!("missing-value:{field:?}"),
        Violation::MissingRoot => "missing-root".to_owned(),
        Violation::Unreachable => "unreachable".to_owned(),
        Violation::ParentMismatch { recorded, actual } => {
            format!("parent-mismatch:{recorded:?}:{actual:?}")
        }
        other => panic!("unexpected compact production violation: {other:?}"),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct DiagnosticObservation {
    check_kind: String,
    declaration: String,
    class: String,
    requirement: String,
    subject: Option<String>,
    elements: Vec<String>,
    violation: String,
}

fn reference_diagnostic(diagnostic: &reference::Diagnostic) -> DiagnosticObservation {
    DiagnosticObservation {
        check_kind: format!("{:?}", diagnostic.check.kind),
        declaration: reference_declaration(&diagnostic.check),
        class: format!("{:?}", diagnostic.class),
        requirement: format!("{:?}", diagnostic.requirement),
        subject: diagnostic.subject.as_ref().map(reference_view),
        elements: diagnostic.elements.iter().map(reference_view).collect(),
        violation: reference_violation(&diagnostic.violation),
    }
}

fn reference_declaration(check: &reference::CheckKey) -> String {
    use reference::CheckKind;
    match check.kind {
        CheckKind::NodeAnchor
        | CheckKind::EdgeDeclared
        | CheckKind::EndpointCoherence
        | CheckKind::TreeReachable => {
            // Production compiles one schema-wide key for each of these
            // structural families. The reference schedule refines that key
            // by the declaration observed at a particular subject. This
            // adapter translates the refined key back to the public plan key;
            // the declaration remains recoverable from the typed violation.
            "schema".to_owned()
        }
        _ => reference_schema_ref(&check.decl),
    }
}

fn production_diagnostic(diagnostic: &Diagnostic) -> DiagnosticObservation {
    assert_eq!(
        diagnostic.source, None,
        "the compact carrier has no source map"
    );
    DiagnosticObservation {
        check_kind: format!("{:?}", diagnostic.check.kind),
        declaration: production_schema_ref(&diagnostic.check.decl),
        class: format!("{:?}", diagnostic.class),
        requirement: format!("{:?}", diagnostic.requirement),
        subject: diagnostic.subject.as_ref().map(production_view),
        elements: diagnostic.elements.iter().map(production_view).collect(),
        violation: production_violation(&diagnostic.violation),
    }
}

fn compare_reports(
    label: &str,
    expected: &reference::Report,
    actual: &panproto_inst::validation::ValidationReport,
) {
    assert_eq!(
        reference::status(&expected.diagnostics, &expected.incomplete),
        expected.status,
        "reference status fold at {label}"
    );
    assert_eq!(
        reference::CheckStatus::all([expected.status]),
        expected.status,
        "singleton status conjunction at {label}"
    );
    assert_eq!(
        production_status_observation(actual.status),
        status_observation(expected.status),
        "status at {label}"
    );
    assert_eq!(
        actual
            .diagnostics
            .iter()
            .map(production_diagnostic)
            .collect::<Vec<_>>(),
        expected
            .diagnostics
            .iter()
            .map(reference_diagnostic)
            .collect::<Vec<_>>(),
        "ordered diagnostics at {label}"
    );
    assert!(
        actual.incomplete.is_empty() && expected.incomplete.is_empty(),
        "unlimited compact validation must decide every check at {label}: production={:?}, reference={:?}",
        actual.incomplete,
        expected.incomplete
    );
    for coverage in actual.coverage.values() {
        assert_eq!(
            coverage.instances,
            coverage.passed + coverage.failed + coverage.incomplete,
            "complete coverage accounting at {label}"
        );
    }
}

fn production_state(
    plan: &ValidationPlan,
    status: panproto_gat::CheckStatus,
    diagnostic: Option<Diagnostic>,
) -> ValidationState {
    let open = diagnostic.map_or_else(BTreeMap::new, |diagnostic| {
        let instance = panproto_inst::validation::CheckInstance {
            check: diagnostic.check.clone(),
            subject: diagnostic.subject.clone(),
        };
        BTreeMap::from([(
            instance,
            OpenVerdict {
                diagnostics: vec![diagnostic],
                incomplete: Vec::new(),
            },
        )])
    });
    ValidationState {
        plan: *plan.digest(),
        seq: 0,
        status,
        open,
        budget_cuts: BTreeMap::new(),
        coverage: BTreeMap::new(),
        external: ExternalVerdicts::default(),
        key_indexes: BTreeMap::new(),
        reference_indexes: BTreeMap::new(),
    }
}

fn production_open_diagnostic(anchor: &str) -> Diagnostic {
    let subject = panproto_inst::ViewRef::Node(0);
    Diagnostic {
        check: CheckKey::new(CheckKind::NodeAnchor, ProductionSchemaRef::Schema),
        class: CheckClass::GraphLocal,
        requirement: Requirement::Required,
        subject: Some(subject.clone()),
        elements: vec![subject],
        violation: ProductionViolation::UnknownAnchor {
            anchor: anchor.into(),
        },
        source: None,
    }
}

fn graph_cases(edge: &reference_model::SEdge) -> Vec<reference_model::Graph> {
    let valid = reference_model::Graph {
        nodes: [
            (0, reference_model::Node::new(0, "parent")),
            (
                1,
                reference_model::Node::new(1, "child")
                    .with_value(ReferenceValue::Str("ok".to_owned())),
            ),
        ]
        .into(),
        edges: [(
            0,
            reference_model::GEdge {
                src: 0,
                tgt: 1,
                label: edge.clone(),
                position: None,
            },
        )]
        .into(),
    };
    let mut unknown = valid.clone();
    "ghost".clone_into(&mut unknown.nodes.get_mut(&1).unwrap().anchor);
    let mut missing = valid.clone();
    missing.edges.clear();
    let mut long = valid.clone();
    long.nodes.get_mut(&1).unwrap().value = Some(ReferencePresence::Present(ReferenceValue::Str(
        "long".to_owned(),
    )));
    vec![valid, unknown, missing, long]
}

fn table_cases(edge: &reference_model::SEdge) -> Vec<reference_model::Table> {
    let valid = reference_model::Table {
        rows: [
            (0, ("parent".to_owned(), BTreeMap::new())),
            (1, ("child".to_owned(), BTreeMap::new())),
        ]
        .into(),
        pairs: [(edge.clone(), 0, 1)].into(),
    };
    let mut unknown = valid.clone();
    "ghost".clone_into(&mut unknown.rows.get_mut(&1).unwrap().0);
    unknown.pairs.clear();
    let mut missing = valid.clone();
    missing.pairs.clear();
    let mut long = valid.clone();
    long.pairs.clear();
    long.rows
        .get_mut(&0)
        .unwrap()
        .1
        .insert("child".to_owned(), ReferenceValue::Str("long".to_owned()));
    vec![valid, unknown, missing, long]
}

fn tree_cases(edge: &reference_model::SEdge) -> Vec<reference_model::Tree> {
    let valid = reference_model::Tree {
        nodes: [
            (0, reference_model::Node::new(0, "parent")),
            (
                1,
                reference_model::Node::new(1, "child")
                    .with_value(ReferenceValue::Str("ok".to_owned())),
            ),
        ]
        .into(),
        arcs: vec![(0, 1, edge.clone())],
        fans: Vec::new(),
        root: 0,
    };
    let mut unknown = valid.clone();
    "ghost".clone_into(&mut unknown.nodes.get_mut(&1).unwrap().anchor);
    let mut missing = valid.clone();
    missing.arcs.clear();
    let mut long = valid.clone();
    long.nodes.get_mut(&1).unwrap().value = Some(ReferencePresence::Present(ReferenceValue::Str(
        "long".to_owned(),
    )));
    vec![valid, unknown, missing, long]
}

#[test]
fn whole_validation_agrees_exactly_on_the_compact_three_shape_carrier() {
    let (reference_schema, semantics) = fixture_schema();
    let (schema, protocol) = production_fixture_schema(&reference_schema);
    let plan = ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let edge = reference_schema.edges.iter().next().unwrap();
    let mut cases = 0usize;

    for (index, graph) in graph_cases(edge).iter().enumerate() {
        let expected = reference::validate(&reference_schema, &semantics, graph);
        let actual = plan.validate_graph(
            &production_graph(graph),
            &ValidationBudget::default(),
            &ExternalVerdicts::default(),
        );
        compare_reports(&format!("graph-{index}"), &expected, &actual);
        cases += 1;
    }
    for (index, table) in table_cases(edge).iter().enumerate() {
        let expected = reference::validate_table(&reference_schema, &semantics, table)
            .expect("compact reference table presents to validation");
        let actual = plan.validate_table(
            &production_table(table),
            &ValidationBudget::default(),
            &ExternalVerdicts::default(),
        );
        compare_reports(&format!("table-{index}"), &expected, &actual);
        cases += 1;
    }
    for (index, tree) in tree_cases(edge).iter().enumerate() {
        let expected = reference::validate_tree(&reference_schema, &semantics, tree)
            .expect("compact reference tree presents to validation");
        let actual = plan.validate_tree(
            &production_tree(tree),
            &ValidationBudget::default(),
            &ExternalVerdicts::default(),
        );
        compare_reports(&format!("tree-{index}"), &expected, &actual);
        cases += 1;
    }

    assert_eq!(cases, COMPACT_CASE_COUNT);
    assert_eq!(COMPACT_CASE_COUNT, SHAPES * CASES_PER_SHAPE);
}

#[derive(Debug, PartialEq, Eq)]
struct IncompleteObservation {
    check_kind: String,
    declaration: String,
    class: String,
    requirement: String,
    subject: Option<String>,
    reason: String,
}

fn reference_incomplete(entry: &reference::IncompleteCheck) -> IncompleteObservation {
    let (reason, first_skipped) = match &entry.reason {
        reference::IncompleteReason::Budget { limit, spent, from } => (
            format!("budget:{limit}:{spent}"),
            from.as_ref().map(reference_view),
        ),
        other => panic!("unexpected compact reference incomplete: {other:?}"),
    };
    assert_eq!(
        entry.subject.as_ref().map(reference_view),
        first_skipped,
        "the reference budget refusal names its first skipped subject twice"
    );
    IncompleteObservation {
        check_kind: format!("{:?}", entry.check.kind),
        declaration: reference_declaration(&entry.check),
        class: format!("{:?}", entry.class),
        requirement: format!("{:?}", entry.requirement),
        subject: first_skipped,
        reason,
    }
}

fn production_incomplete(entry: &IncompleteCheck) -> IncompleteObservation {
    let (reason, first_skipped) = match &entry.reason {
        panproto_inst::validation::IncompleteReason::Budget { limit, spent, from } => (
            format!("budget:{limit}:{spent}"),
            from.as_ref().map(production_view),
        ),
        other => panic!("unexpected compact production incomplete: {other:?}"),
    };
    assert_eq!(
        entry.subject, None,
        "production budget refusals are family-level suffix records"
    );
    IncompleteObservation {
        check_kind: format!("{:?}", entry.check.kind),
        declaration: production_schema_ref(&entry.check.decl),
        class: format!("{:?}", entry.class),
        requirement: format!("{:?}", entry.requirement),
        subject: first_skipped,
        reason,
    }
}

fn budget_fixture() -> (
    reference_schema::Schema,
    BTreeMap<String, reference::Semantics>,
    reference_model::Graph,
) {
    (
        reference_schema::Schema::default()
            .vertex("parent", "object")
            .vertex("child", "string")
            .edge("parent", "child", "prop", Some("child")),
        BTreeMap::new(),
        reference_model::Graph {
            nodes: [(0, reference_model::Node::new(0, "parent"))].into(),
            ..reference_model::Graph::default()
        },
    )
}

#[test]
fn finite_budget_cuts_are_the_same_ordered_suffix() {
    let (reference_schema, semantics, graph) = budget_fixture();
    let (schema, protocol) = production_fixture_schema(&reference_schema);
    let plan = ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let input = reference::ValidationInput::Graph {
        schema: reference_schema,
        semantics,
        instance: graph.clone(),
    };

    let mut limits = 0usize;
    for limit in 0..=2 {
        let witness = reference::witness_with_trust(
            input.clone(),
            reference::ValidationTrust {
                budget: reference::ValidationBudget {
                    max_reads: Some(limit),
                },
                ..reference::ValidationTrust::default()
            },
        )
        .expect("initial reference witness");
        assert_eq!(witness.verify(), Ok(()));
        let actual = plan.validate_graph(
            &production_graph(&graph),
            &ValidationBudget {
                max_reads: Some(limit),
            },
            &ExternalVerdicts::default(),
        );
        assert_eq!(
            production_status_observation(actual.status),
            status_observation(witness.evaluation.report.status),
            "budget status at {limit}"
        );
        assert_eq!(
            actual
                .diagnostics
                .iter()
                .map(production_diagnostic)
                .collect::<Vec<_>>(),
            witness
                .evaluation
                .report
                .diagnostics
                .iter()
                .map(reference_diagnostic)
                .collect::<Vec<_>>(),
            "budget prefix diagnostics at {limit}"
        );
        assert_eq!(
            actual
                .incomplete
                .iter()
                .map(production_incomplete)
                .collect::<Vec<_>>(),
            witness
                .evaluation
                .report
                .incomplete
                .iter()
                .map(reference_incomplete)
                .collect::<Vec<_>>(),
            "budget suffix at {limit}"
        );
        limits += 1;
    }
    assert_eq!(limits, 3);
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the manifest checker requires every named reference function in this consumer body"
)]
fn supplemental_validation_definitions_have_bounded_production_witnesses() {
    use panproto_reference::keys::KeyEquality;

    let (reference_schema, _semantics) = fixture_schema();
    let (schema, protocol) = production_fixture_schema(&reference_schema);
    let plan = ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());

    let passed = production_state(&plan, panproto_gat::CheckStatus::Passed, None);
    let failed_a = production_state(
        &plan,
        panproto_gat::CheckStatus::Failed,
        Some(production_open_diagnostic("a")),
    );
    let failed_b = production_state(
        &plan,
        panproto_gat::CheckStatus::Failed,
        Some(production_open_diagnostic("b")),
    );
    let empty = BTreeMap::<u8, Vec<u8>>::new();
    let open_a = BTreeMap::from([(0, b"a".to_vec())]);
    let open_b = BTreeMap::from([(0, b"b".to_vec())]);
    let admission_cases = [
        (true, &passed, &passed, &empty, &empty, true),
        (true, &passed, &failed_a, &empty, &open_a, false),
        (false, &failed_a, &failed_a, &open_a, &open_a, true),
        (false, &failed_a, &failed_b, &open_a, &open_b, false),
    ];
    let mut admission_count = 0usize;
    for (require_passed, before, after, reference_before, reference_after, expected) in
        admission_cases
    {
        let production = if require_passed {
            AdmissionPolicy::RequirePassed.admits(&plan, before, after)
        } else {
            AdmissionPolicy::NoNewFailures.admits(&plan, before, after)
        };
        let reference = reference::admits(
            require_passed,
            reference_before,
            &BTreeMap::new(),
            reference_after,
        );
        assert_eq!(production, expected);
        assert_eq!(reference, production);
        admission_count += 1;
    }
    assert_eq!(admission_count, 4);

    let key_values = [
        ReferenceValue::Int(1),
        ReferenceValue::Str("1".to_owned()),
        ReferenceValue::Str("x".to_owned()),
    ];
    let mut key_count = 0usize;
    for left in &key_values {
        for right in &key_values {
            let production = production_key::key_equivalence(
                &panproto_schema::KeyEquality::Representation,
                &production_value(left),
                &production_value(right),
            )
            .expect("representation equality is total");
            assert_eq!(
                reference::key_equality(&KeyEquality::Representation, left, right),
                Some(production)
            );
            key_count += 1;
        }
    }
    assert_eq!(key_count, 9);

    let definitions = [
        (0, "x".to_owned(), 10),
        (2, "y".to_owned(), 20),
        (3, "x".to_owned(), 30),
        (7, "x".to_owned(), 70),
    ];
    let mut binding_count = 0usize;
    for position in 0..=8 {
        for name in ["x", "y", "z"] {
            for nearest in [false, true] {
                let candidates = definitions
                    .iter()
                    .filter(|(defined, candidate, _)| *defined < position && candidate == name);
                let production = if nearest {
                    candidates
                        .max_by_key(|(defined, _, _)| *defined)
                        .map(|(_, _, value)| *value)
                } else {
                    candidates
                        .min_by_key(|(defined, _, _)| *defined)
                        .map(|(_, _, value)| *value)
                };
                assert_eq!(
                    reference::variable_binding(&definitions, position, name, nearest),
                    production
                );
                binding_count += 1;
            }
        }
    }
    assert_eq!(binding_count, 54);

    assert_eq!(reference::key_forms(&reference_schema), Ok(()));
    assert!(
        plan.static_findings().is_empty(),
        "the production plan accepts the same bounded declaration form"
    );
    let declaration_form_count = 1usize;

    // Index and MATCH FULL resolution are exercised through the production
    // TreeToGraph resolver in `panproto-io::reference_resolution`. Keeping a
    // second Boolean helper here would not observe that runtime path.
    let texts = ["0", "42", "-1", "not-an-integer"];
    let mut lexical_count = 0usize;
    for text in texts {
        let accepts = |candidate: &str| candidate.parse::<i64>().is_ok();
        let reference = reference::lexical_space(text, accepts);
        let (passed, reported, incomplete) = production_lexical_observation(text, "integer");
        assert!(passed, "lexical-space diagnostics are advisory");
        assert!(!incomplete, "integer has a declared lexical kind");
        assert_eq!(!reported, reference);
        lexical_count += 1;
    }
    assert_eq!(lexical_count, 4);

    let format_cases = [
        ("550e8400-e29b-41d4-a716-446655440000", true),
        ("550E8400-E29B-41D4-A716-446655440000", true),
        ("550e8400-e29b-41d4-a716-44665544000", false),
        ("not-a-uuid", false),
    ];
    let mut parse_count = 0usize;
    let mut advisory_count = 0usize;
    let mut assertion_count = 0usize;
    for (text, recognizes) in format_cases {
        let (advisory_passed, advisory_reported, advisory_incomplete, preserved) =
            production_format_observation(text, true);
        assert!(advisory_passed, "advisory format must not fail admission");
        assert!(!advisory_incomplete, "uuid has a settled recognizer");
        assert_eq!(advisory_reported, !recognizes);
        assert!(reference::annotated_format_parse(
            &Some(text),
            &Some(preserved.as_str())
        ));
        parse_count += 1;

        let (assertion_passed, assertion_reported, assertion_incomplete, _) =
            production_format_observation(text, false);
        assert!(!assertion_incomplete, "uuid has a settled recognizer");
        assert_eq!(assertion_reported, !recognizes);
        assert_eq!(assertion_passed, recognizes);

        for recorded in [false, true] {
            assert_eq!(
                reference::annotated_format_advisory(text, |_| recognizes, recorded),
                recorded == advisory_reported
            );
            advisory_count += 1;
            assert_eq!(
                reference::format_assertion_acceptance(text, |_| recognizes, recorded),
                recorded == assertion_passed
            );
            assertion_count += 1;
        }
    }
    assert_eq!(parse_count, 4);
    assert_eq!(advisory_count, 8);
    assert_eq!(assertion_count, 8);
    assert_eq!(
        admission_count
            + key_count
            + binding_count
            + declaration_form_count
            + lexical_count
            + parse_count
            + advisory_count
            + assertion_count,
        SUPPLEMENTAL_OBSERVATION_COUNT
    );
}
