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
const SUPPLEMENTAL_OBSERVATION_COUNT: usize = 206;

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

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

/// The kind table of the lexical-space carrier: each XSD datatype the
/// reference covers, with the value kind a route would give it.
fn xsd_kinds() -> Vec<(&'static str, ScalarKind)> {
    vec![
        (
            "integer",
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        ),
        ("decimal", ScalarKind::Decimal),
        ("double", ScalarKind::Float { bits: 64 }),
        ("boolean", ScalarKind::Bool),
        ("date", ScalarKind::Date),
        ("dateTime", ScalarKind::DateTime),
        (
            "gYear",
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        ),
    ]
}

/// The reference kind of a production kind of the lexical-space carrier.
fn reference_kind(kind: &ScalarKind) -> panproto_reference::coerce::ScalarKind {
    use panproto_reference::coerce::ScalarKind as K;
    match kind {
        ScalarKind::Int { bits, signed } => K::Int {
            bits: *bits,
            signed: *signed,
        },
        ScalarKind::Decimal => K::Decimal,
        ScalarKind::Float { bits } => K::Float { bits: *bits },
        ScalarKind::Bool => K::Bool,
        ScalarKind::Date => K::Date,
        ScalarKind::DateTime => K::DateTime,
        other => panic!("kind outside the lexical carrier: {other:?}"),
    }
}

/// The protocol of the annotated-format carrier: `integer` and `number`
/// vertices of the kinds JSON Schema's `type` gives them, the sort `format`
/// meaning an annotated format and the sort `int_bits` an integer-width
/// value parameter (what the assertion vocabulary writes).
fn format_parse_protocol() -> Protocol {
    let mut protocol = Protocol {
        name: "reference-annotated-format".to_owned(),
        obj_kinds: vec!["integer".to_owned(), "number".to_owned()],
        constraint_sorts: vec!["format".to_owned(), "int_bits".to_owned()],
        constraint_semantics: BTreeMap::from([
            (
                "format".to_owned(),
                ConstraintSemantics::AnnotatedFormat("test".into()),
            ),
            ("int_bits".to_owned(), ConstraintSemantics::ValueParameter),
        ]),
        value_kinds: BTreeMap::from([
            (
                "integer".to_owned(),
                ScalarKind::Int {
                    bits: 64,
                    signed: true,
                },
            ),
            ("number".to_owned(), ScalarKind::Float { bits: 64 }),
        ]),
        ..Protocol::default()
    };
    protocol.syntactic_shape = InstanceShapeKind::Graph;
    protocol.semantic_shape = InstanceShapeKind::Graph;
    protocol
}

/// The value spec a parse reads for one `vertex_kind` vertex carrying the
/// annotated `format` and the integer width `bits`, each when present.
fn format_parse_spec(
    protocol: &Protocol,
    vertex_kind: &str,
    format: Option<&str>,
    bits: Option<&str>,
) -> panproto_schema::ValueSpec {
    let mut builder = SchemaBuilder::new(protocol)
        .vertex("value", vertex_kind, None)
        .expect("format carrier vertex");
    if let Some(format) = format {
        builder = builder.constraint("value", "format", format);
    }
    if let Some(bits) = bits {
        builder = builder.constraint("value", "int_bits", bits);
    }
    let schema = builder.build().expect("format carrier schema");
    panproto_schema::ValueSpec::for_vertex(&schema, protocol, &"value".into())
}

fn coerced_observation(value: Option<&panproto_inst::Value>) -> String {
    match value {
        None => "refused".to_owned(),
        Some(panproto_inst::Value::Int(value)) => format!("int:{value}"),
        Some(panproto_inst::Value::Float(value)) => format!("float:{:x}", value.to_bits()),
        Some(other) => format!("other:{}", variant_name(&format!("{other:?}"))),
    }
}

fn reference_coerced_observation(value: &panproto_reference::coerce::Coerced) -> String {
    match value {
        Some(Ok(ReferenceValue::Int(value))) => format!("int:{value}"),
        Some(Ok(ReferenceValue::Float(bits))) => format!("float:{bits:x}"),
        Some(Ok(other)) => format!("other:{}", variant_name(&format!("{other:?}"))),
        Some(Err(_)) | None => "refused".to_owned(),
    }
}

/// The variant name at the start of a `Debug` rendering.
fn variant_name(debug: &str) -> String {
    debug
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// Validate one node holding `value` under the annotated `format`: whether
/// the report passed, whether it holds a diagnostic, and whether the plan
/// compiled a value check for the format.
fn production_annotated_observation(value: &ReferenceValue, format: &str) -> (bool, bool, bool) {
    let mut protocol = format_parse_protocol();
    protocol.obj_kinds.push("any".to_owned());
    protocol.value_kinds.insert("any".to_owned(), ScalarKind::Any);
    let schema = SchemaBuilder::new(&protocol)
        .vertex("value", "any", None)
        .expect("annotated carrier vertex")
        .constraint("value", "format", format)
        .build()
        .expect("annotated carrier schema");
    let plan = ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let compiled = plan
        .checks()
        .keys()
        .any(|key| key.kind == CheckKind::ValueConstraint);
    let graph = GInstance::new().with_node(
        Node::new(0, "value").with_value(FieldPresence::Present(keyed_value(value))),
    );
    let report = plan.validate_graph(
        &graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    (report.passed(), !report.diagnostics.is_empty(), compiled)
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
    protocol.value_kinds.extend(
        xsd_kinds()
            .into_iter()
            .map(|(local, kind)| (format!("{XSD}{local}"), kind)),
    );
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
        // The reference names the rejecting check; production records the
        // witness digest. The check itself is compared through the key.
        reference::Violation::ExternalRejected(_) => "external-rejected".to_owned(),
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
        Violation::ExternalRejected { .. } => "external-rejected".to_owned(),
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

/// The `child` vertex declares no attributes, so a field on a `child` node
/// is undeclared. The attribute check refuses it at every typed node, not
/// only at nodes whose vertex declares some attribute.
#[test]
fn an_attribute_at_a_vertex_declaring_none_is_refused() {
    let (reference_schema, semantics) = fixture_schema();
    let (schema, protocol) = production_fixture_schema(&reference_schema);
    let plan = ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let edge = reference_schema.edges.iter().next().unwrap();
    let extra = |node: &mut reference_model::Node| {
        node.fields
            .insert("note".to_owned(), ReferenceValue::Str("x".to_owned()));
    };

    let mut graph = graph_cases(edge).swap_remove(0);
    extra(graph.nodes.get_mut(&1).unwrap());
    let expected = reference::validate(&reference_schema, &semantics, &graph);
    assert!(
        expected
            .diagnostics
            .iter()
            .any(|d| matches!(d.violation, reference::Violation::UndeclaredAttribute(_))),
        "{expected:?}"
    );
    let actual = plan.validate_graph(
        &production_graph(&graph),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    compare_reports("graph with an undeclared attribute", &expected, &actual);

    let mut tree = tree_cases(edge).swap_remove(0);
    extra(tree.nodes.get_mut(&1).unwrap());
    let expected = reference::validate_tree(&reference_schema, &semantics, &tree)
        .expect("compact reference tree presents to validation");
    let actual = plan.validate_tree(
        &production_tree(&tree),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    compare_reports("tree with an undeclared attribute", &expected, &actual);
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
    // The reference records one refusal per skipped instance; `from` names
    // the first skipped subject of the instance's check key.
    assert!(entry.subject.is_some(), "a reference refusal names its instance");
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

/// A reference check instance under production's identity: its kind, the
/// production declaration it refines, and its subject.
fn reference_instance(instance: &reference::CheckInstance) -> String {
    format!(
        "{:?}|{}|{:?}",
        instance.check.kind,
        reference_declaration(&instance.check),
        instance.subject.as_ref().map(reference_view)
    )
}

fn production_instance(instance: &panproto_inst::validation::CheckInstance) -> String {
    format!(
        "{:?}|{}|{:?}",
        instance.check.kind,
        production_schema_ref(&instance.check.decl),
        instance.subject.as_ref().map(production_view)
    )
}

/// The edits of the transition carrier applicable to `graph`, each with the
/// reference graph it produces.
fn carrier_edits(
    graph: &reference_model::Graph,
    edge: &reference_model::SEdge,
) -> Vec<(String, panproto_inst::GraphEdit, reference_model::Graph)> {
    use panproto_inst::GraphEdit;
    let mut edits = Vec::new();
    let node = &graph.nodes[&1];
    for text in ["ok", "long"] {
        let new = ReferencePresence::Present(ReferenceValue::Str(text.to_owned()));
        if node.value.as_ref() != Some(&new) {
            let mut target = graph.clone();
            target.nodes.get_mut(&1).unwrap().value = Some(new.clone());
            edits.push((
                format!("set value {text}"),
                GraphEdit::SetValue {
                    node: 1,
                    old: node.value.as_ref().map(production_presence),
                    new: Some(production_presence(&new)),
                },
                target,
            ));
        }
    }
    let other = if node.anchor == "child" { "ghost" } else { "child" };
    let mut relabelled = graph.clone();
    other.clone_into(&mut relabelled.nodes.get_mut(&1).unwrap().anchor);
    edits.push((
        format!("relabel to {other}"),
        GraphEdit::RelabelNode {
            node: 1,
            old_anchor: node.anchor.as_str().into(),
            new_anchor: other.into(),
        },
        relabelled,
    ));
    if let Some(present) = graph.edges.get(&0) {
        let mut target = graph.clone();
        target.edges.remove(&0);
        edits.push((
            "remove edge".to_owned(),
            GraphEdit::RemoveEdge {
                id: EdgeId(0),
                edge: GEdge {
                    src: present.src,
                    tgt: present.tgt,
                    edge: production_edge(&present.label),
                    position: present.position,
                },
            },
            target,
        ));
    } else {
        let mut target = graph.clone();
        target.edges.insert(
            0,
            reference_model::GEdge {
                src: 0,
                tgt: 1,
                label: edge.clone(),
                position: None,
            },
        );
        edits.push((
            "add edge".to_owned(),
            GraphEdit::AddEdge {
                id: EdgeId(0),
                edge: GEdge {
                    src: 0,
                    tgt: 1,
                    edge: production_edge(edge),
                    position: None,
                },
            },
            target,
        ));
    }
    edits
}

/// Footprint soundness and edit agreement against an independent model of
/// incremental validation. For every base graph of the compact carrier and
/// every applicable edit (value changes, a relabel, an edge insertion or
/// removal), the reference computes the two complete schedules, their
/// exact `created` and `retired` differences and the persistent instances
/// whose outcome or read set changed. Production's footprint must have the
/// same `created` and `retired` sets, a `rerun` set containing every changed
/// instance, and exactly the read union the reference assigns to its
/// partitions; its edit report must be the reference's spliced report for
/// that rerun set, which must in turn be the target's whole report.
#[test]
fn incremental_transitions_agree_with_the_independent_reference() {
    let (reference_schema, semantics) = fixture_schema();
    let (schema, protocol) = production_fixture_schema(&reference_schema);
    let plan = std::sync::Arc::new(ValidationPlan::compile(
        &schema,
        &protocol,
        &ValidationOptions::default(),
    ));
    let edge = reference_schema.edges.iter().next().unwrap().clone();
    let none = ExternalVerdicts::default();
    let input = |graph: &reference_model::Graph| reference::ValidationInput::Graph {
        schema: reference_schema.clone(),
        semantics: semantics.clone(),
        instance: graph.clone(),
    };
    let (mut transitions, mut rerun_beyond_changed, mut changed_total) = (0usize, 0usize, 0usize);
    let mut witnesses = 0usize;
    for (index, base) in graph_cases(&edge).iter().enumerate() {
        for (name, edit, target) in carrier_edits(base, &edge) {
            let label = format!("graph-{index} {name}");
            let trust = reference::ValidationTrust::default();
            let before = reference::trusted_schedule(&input(base), &trust);
            let after = reference::trusted_schedule(&input(&target), &trust);
            let model = reference::incremental_footprint(&before, &after);

            let (validated, _) = panproto_inst::validation::ValidatedGraph::new(
                plan.clone(),
                production_graph(base),
                &ValidationBudget::default(),
                &none,
            );
            let footprint = validated
                .footprint(&edit, &none)
                .unwrap_or_else(|error| panic!("{label}: {error}"));
            let names = |set: &BTreeSet<panproto_inst::validation::CheckInstance>| {
                set.iter().map(production_instance).collect::<BTreeSet<_>>()
            };
            let reference_names = |set: &BTreeSet<reference::CheckInstance>| {
                set.iter().map(reference_instance).collect::<BTreeSet<_>>()
            };
            assert_eq!(names(&footprint.created), reference_names(&model.created), "{label} created");
            assert_eq!(names(&footprint.retired), reference_names(&model.retired), "{label} retired");
            let rerun = names(&footprint.rerun);
            let changed = reference_names(&model.changed);
            assert!(
                changed.is_subset(&rerun),
                "{label}: changed {changed:?} not rerun in {rerun:?}"
            );
            changed_total += changed.len();
            rerun_beyond_changed += rerun.difference(&changed).count();

            // The reference instances production reruns.
            let reference_rerun: BTreeSet<reference::CheckInstance> = before
                .instances()
                .into_iter()
                .chain(after.instances())
                .filter(|instance| rerun.contains(&reference_instance(instance)))
                .collect();
            assert_eq!(
                footprint.reads.iter().map(production_view).collect::<BTreeSet<_>>(),
                reference::footprint_reads(&before, &after, &reference_rerun)
                    .iter()
                    .map(reference_view)
                    .collect::<BTreeSet<_>>(),
                "{label} reads"
            );

            let checked = validated
                .check_edit(&edit, &ValidationBudget::default(), &none)
                .unwrap_or_else(|error| panic!("{label}: {error}"));
            let spliced = reference::incremental_report(&before, &after, &reference_rerun);
            let whole = reference::incremental_report(&after, &after, &BTreeSet::new());
            let validated_whole = reference::validate(&reference_schema, &semantics, &target);
            assert_eq!(whole.diagnostics, validated_whole.diagnostics, "{label}");
            assert_eq!(whole.incomplete, validated_whole.incomplete, "{label}");
            assert_eq!(spliced.status, whole.status, "{label}");
            assert_eq!(spliced.diagnostics, whole.diagnostics, "{label}");
            assert_eq!(spliced.incomplete, whole.incomplete, "{label}");
            assert_eq!(
                production_status_observation(checked.report.status),
                status_observation(spliced.status),
                "{label}"
            );
            assert_eq!(
                checked
                    .report
                    .diagnostics
                    .iter()
                    .map(production_diagnostic)
                    .collect::<Vec<_>>(),
                spliced
                    .diagnostics
                    .iter()
                    .map(reference_diagnostic)
                    .collect::<Vec<_>>(),
                "{label}"
            );
            assert_eq!(
                checked
                    .report
                    .incomplete
                    .iter()
                    .map(general_production_incomplete)
                    .collect::<Vec<_>>(),
                spliced
                    .incomplete
                    .iter()
                    .map(general_reference_incomplete)
                    .collect::<Vec<_>>(),
                "{label}"
            );

            // The witness of the admitted transition, checked against the
            // independent model: its runs are exactly the rerun and created
            // instances, each with the target schedule's reads and outcome
            // multiplicities.
            let mut applied = validated.clone();
            if let Ok(witness) = applied.apply_edit(
                &edit,
                AdmissionPolicy::NoNewFailures,
                &ValidationBudget::default(),
                &none,
            ) {
                let scheduled: BTreeSet<String> = footprint
                    .rerun
                    .iter()
                    .chain(&footprint.created)
                    .map(production_instance)
                    .collect();
                assert_eq!(
                    witness
                        .runs
                        .iter()
                        .map(|run| production_instance(&run.instance))
                        .collect::<BTreeSet<_>>(),
                    scheduled,
                    "{label} witness runs"
                );
                let target_runs: BTreeMap<String, &reference::CheckRun> = after
                    .runs()
                    .iter()
                    .map(|run| (reference_instance(&run.instance), run))
                    .collect();
                for run in &witness.runs {
                    let key = production_instance(&run.instance);
                    let expected = target_runs
                        .get(&key)
                        .unwrap_or_else(|| panic!("{label}: {key} is in the target schedule"));
                    assert_eq!(
                        run.reads.iter().map(production_view).collect::<BTreeSet<_>>(),
                        expected.reads.iter().map(reference_view).collect::<BTreeSet<_>>(),
                        "{label} witness reads of {key}"
                    );
                    let panproto_inst::validation::RunVerdict::Evaluated {
                        findings,
                        incomplete,
                    } = &run.verdict
                    else {
                        panic!("{label}: an unlimited run is evaluated");
                    };
                    assert_eq!(
                        (findings.len(), incomplete.len()),
                        (expected.diagnostics.len(), expected.incomplete.len()),
                        "{label} witness outcome of {key}"
                    );
                }
                witnesses += 1;
            }
            transitions += 1;
        }
    }
    assert_eq!(transitions, 12);
    assert!(witnesses > 0);
    assert!(changed_total > 0, "some transition changes a persistent outcome");
    eprintln!(
        "incremental reference: {transitions} transitions, {changed_total} changed persistent \
         instances, {rerun_beyond_changed} rerun beyond the changed set"
    );
}

/// The lexical-space law as a same-input differential over the F45 cases
/// and their neighbours. Each literal is reported exactly when it lies
/// outside its datatype's XSD lexical space, as the independent reference
/// decides it, or when a lexeme inside that space does not coerce under the
/// datatype's kind, as the reference coercion decides it; a datatype the
/// kind table lacks is an advisory unsupported declaration; and no entry
/// changes the status.
#[test]
fn lexical_space_reports_exactly_the_ill_typed_literals() {
    use panproto_reference::coerce::{NumeralDialect, ValueSpec as ReferenceSpec};

    let kinds: BTreeMap<&str, ScalarKind> = xsd_kinds().into_iter().collect();
    let cases = [
        ("integer", "42"),
        ("integer", "-0"),
        ("integer", "abc"),
        ("integer", "1.0"),
        ("decimal", "1."),
        ("decimal", ".5"),
        ("decimal", "1e3"),
        ("double", "INF"),
        ("double", "-INF"),
        ("double", "inf"),
        ("double", "1.5e3"),
        ("boolean", "true"),
        ("boolean", "True"),
        ("date", "2026-02-28"),
        ("date", "2026-02-30"),
        ("date", "2024-02-29"),
        ("dateTime", "2026-01-01T23:59:59Z"),
        ("dateTime", "2026-01-01T23:59:60Z"),
        ("dateTime", "2026-01-01T24:00:00Z"),
        ("gYear", "1999"),
        ("gYear", "01999"),
        ("QName", "x"),
        ("unknownType", "x"),
    ];
    let (mut reported_count, mut observed) = (0usize, 0usize);
    for (local, text) in cases {
        let datatype = format!("{XSD}{local}");
        let (passed, reported, incomplete) = production_lexical_observation(text, &datatype);
        assert!(passed, "{local} {text}: lexical-space entries are advisory");
        match (
            reference::lexical_space(text, &datatype),
            kinds.get(local),
        ) {
            (Some(inside), Some(kind)) => {
                let spec = ReferenceSpec {
                    numeral: Some(NumeralDialect::Xsd),
                    ..ReferenceSpec::of(reference_kind(kind))
                };
                let coerces = matches!(
                    panproto_reference::coerce::coerce_value(
                        &ReferenceValue::Str(text.to_owned()),
                        &spec
                    ),
                    Some(Ok(_))
                );
                assert_eq!(
                    reported,
                    !inside || !coerces,
                    "{local} {text}: inside {inside}, coerces {coerces}"
                );
                assert!(!incomplete, "{local} {text}");
                reported_count += usize::from(reported);
            }
            (None, None) => {
                assert!(!reported && incomplete, "{local} {text}: unknown datatype");
            }
            other => panic!("{local}: the carrier covers each datatype on both sides: {other:?}"),
        }
        observed += 1;
    }
    assert_eq!(observed, cases.len());
    assert!(reported_count >= 7, "{reported_count} ill-typed literals reported");
}

/// One incomplete entry of either report, every field kept.
fn general_reference_incomplete(entry: &reference::IncompleteCheck) -> IncompleteObservation {
    let reason = match &entry.reason {
        reference::IncompleteReason::MissingExternalWitness { name } => {
            format!("missing-external:{name}")
        }
        reference::IncompleteReason::Budget { .. } => return reference_incomplete(entry),
        other => panic!("unexpected reference incomplete: {other:?}"),
    };
    IncompleteObservation {
        check_kind: format!("{:?}", entry.check.kind),
        declaration: reference_declaration(&entry.check),
        class: format!("{:?}", entry.class),
        requirement: format!("{:?}", entry.requirement),
        subject: entry.subject.as_ref().map(reference_view),
        reason,
    }
}

fn general_production_incomplete(entry: &IncompleteCheck) -> IncompleteObservation {
    let reason = match &entry.reason {
        panproto_inst::validation::IncompleteReason::MissingExternalWitness { name } => {
            format!("missing-external:{name}")
        }
        panproto_inst::validation::IncompleteReason::Budget { .. } => {
            return production_incomplete(entry);
        }
        other => panic!("unexpected production incomplete: {other:?}"),
    };
    IncompleteObservation {
        check_kind: format!("{:?}", entry.check.kind),
        declaration: production_schema_ref(&entry.check.decl),
        class: format!("{:?}", entry.class),
        requirement: format!("{:?}", entry.requirement),
        subject: entry.subject.as_ref().map(production_view),
        reason,
    }
}

/// The reference coverage summed under production's check keys (the
/// reference refines structural keys by declaration), as
/// `(class, requirement, instances, passed, failed, incomplete)`.
fn reference_coverage(
    coverage: &BTreeMap<reference::CheckKey, reference::CheckCoverage>,
) -> BTreeMap<(String, String), (String, String, u64, u64, u64, u64)> {
    let mut out: BTreeMap<(String, String), (String, String, u64, u64, u64, u64)> =
        BTreeMap::new();
    for (key, entry) in coverage {
        let slot = out
            .entry((format!("{:?}", key.kind), reference_declaration(key)))
            .or_insert_with(|| {
                (
                    format!("{:?}", entry.class),
                    format!("{:?}", entry.requirement),
                    0,
                    0,
                    0,
                    0,
                )
            });
        slot.2 += entry.instances;
        slot.3 += entry.passed;
        slot.4 += entry.failed;
        slot.5 += entry.incomplete;
    }
    out
}

/// The failed and incomplete coverage counts summed per check kind. The
/// two sides partition passing instances differently (the reference keys a
/// declaration check per declaration and gives every typed node an
/// attribute instance), but every failed or incomplete instance is one
/// instance of the same kind on both sides.
fn outcome_counts(
    coverage: &BTreeMap<(String, String), (String, String, u64, u64, u64, u64)>,
) -> BTreeMap<String, (u64, u64)> {
    let mut out: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for ((kind, _), (_, _, _, _, failed, incomplete)) in coverage {
        let slot = out.entry(kind.clone()).or_default();
        slot.0 += failed;
        slot.1 += incomplete;
    }
    out.retain(|_, (failed, incomplete)| *failed + *incomplete > 0);
    out
}

fn production_coverage(
    coverage: &BTreeMap<CheckKey, panproto_inst::validation::CheckCoverage>,
) -> BTreeMap<(String, String), (String, String, u64, u64, u64, u64)> {
    coverage
        .iter()
        .map(|(key, entry)| {
            (
                (
                    format!("{:?}", key.kind),
                    production_schema_ref(&key.decl),
                ),
                (
                    format!("{:?}", entry.class),
                    format!("{:?}", entry.requirement),
                    entry.instances,
                    entry.passed,
                    entry.failed,
                    entry.incomplete,
                ),
            )
        })
        .collect()
}

/// The status law on a carrier that reaches every status, including
/// `Incomplete`: the compact schema with an application check on the child
/// vertex, each child judged passed, rejected or left without a verdict.
/// The whole report is compared, incomplete entries field by field and
/// coverage per check key included.
#[test]
fn status_law_holds_with_external_verdicts_on_every_shape_status() {
    let (mut reference_schema, mut semantics) = fixture_schema();
    reference_schema
        .constraints
        .get_mut("child")
        .unwrap()
        .push(("audit".to_owned(), "1".to_owned()));
    semantics.insert("audit".to_owned(), reference::Semantics::External);
    let (schema, mut protocol) = production_fixture_schema(&reference_schema);
    protocol.constraint_sorts.push("audit".to_owned());
    protocol
        .constraint_semantics
        .insert("audit".to_owned(), ConstraintSemantics::External);
    let plan = ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    assert!(plan.static_findings().is_empty(), "{:?}", plan.static_findings());
    let edge = reference_schema.edges.iter().next().unwrap().clone();
    let mut statuses = BTreeSet::new();
    let mut cases = 0usize;
    for (index, graph) in graph_cases(&edge).iter().enumerate() {
        for verdict in [Some(true), Some(false), None] {
            let child = panproto_reference::lens::ViewRef::Node(1);
            let context = reference::ValidationContext {
                external: verdict
                    .map(|passed| (("audit".to_owned(), child.clone()), passed))
                    .into_iter()
                    .collect(),
                ..reference::ValidationContext::default()
            };
            let mut external = ExternalVerdicts::default();
            if let Some(passed) = verdict {
                external.insert(
                    "audit".into(),
                    panproto_inst::ViewRef::Node(1),
                    panproto_inst::validation::ExternalVerdict {
                        passed,
                        witness: panproto_inst::Digest([7; 32]),
                    },
                );
            }
            let label = format!("graph-{index} verdict {verdict:?}");
            let expected = reference::witness_with_trust(
                reference::ValidationInput::Graph {
                    schema: reference_schema.clone(),
                    semantics: semantics.clone(),
                    instance: graph.clone(),
                },
                reference::ValidationTrust {
                    context,
                    ..reference::ValidationTrust::default()
                },
            )
            .expect("reference witness");
            let report = &expected.evaluation.report;
            let actual =
                plan.validate_graph(&production_graph(graph), &ValidationBudget::default(), &external);
            assert_eq!(
                production_status_observation(actual.status),
                status_observation(report.status),
                "{label}"
            );
            assert_eq!(
                reference::status(&report.diagnostics, &report.incomplete),
                report.status,
                "{label}"
            );
            assert_eq!(
                actual
                    .diagnostics
                    .iter()
                    .map(production_diagnostic)
                    .collect::<Vec<_>>(),
                report
                    .diagnostics
                    .iter()
                    .map(reference_diagnostic)
                    .collect::<Vec<_>>(),
                "{label}"
            );
            assert_eq!(
                actual
                    .incomplete
                    .iter()
                    .map(general_production_incomplete)
                    .collect::<Vec<_>>(),
                report
                    .incomplete
                    .iter()
                    .map(general_reference_incomplete)
                    .collect::<Vec<_>>(),
                "{label}"
            );
            assert_eq!(
                outcome_counts(&production_coverage(&actual.coverage)),
                outcome_counts(&reference_coverage(&expected.evaluation.coverage)),
                "{label} failed and incomplete coverage per kind"
            );
            statuses.insert(production_status_observation(actual.status));
            cases += 1;
        }
    }
    assert_eq!(cases, 12);
    assert_eq!(statuses.len(), 3, "every status is reached: {statuses:?}");
}

/// The status fold itself over every sequence of at most three entries,
/// each a required or advisory diagnostic or incomplete entry: production's
/// aggregation, the reference fold and the statement's predicate agree.
#[test]
fn status_fold_agrees_with_the_reference_on_every_short_entry_sequence() {
    use panproto_inst::validation::aggregate_status;

    let production_requirement = |advisory: bool| {
        if advisory {
            Requirement::Advisory
        } else {
            Requirement::Required
        }
    };
    let reference_requirement = |advisory: bool| {
        if advisory {
            reference::Requirement::Advisory
        } else {
            reference::Requirement::Required
        }
    };
    let reference_key = reference::CheckKey {
        kind: reference::CheckKind::External,
        decl: reference::SchemaRef::Schema,
    };
    let mut sequences = 0usize;
    for length in 0..=3_u32 {
        for code in 0..4_usize.pow(length) {
            let entries: Vec<(bool, bool)> = (0..length)
                .scan(code, |rest, _| {
                    let entry = *rest % 4;
                    *rest /= 4;
                    Some((entry & 1 == 1, entry & 2 == 2))
                })
                .collect();
            let (mut diagnostics, mut incomplete) = (Vec::new(), Vec::new());
            let (mut reference_diagnostics, mut reference_incomplete) = (Vec::new(), Vec::new());
            for (index, (is_incomplete, advisory)) in entries.iter().enumerate() {
                let node = u32::try_from(index).unwrap();
                let subject = panproto_inst::ViewRef::Node(node);
                let reference_subject = panproto_reference::lens::ViewRef::Node(node);
                if *is_incomplete {
                    incomplete.push(IncompleteCheck {
                        check: CheckKey::new(CheckKind::External, ProductionSchemaRef::Schema),
                        class: CheckClass::BoundedExternal,
                        requirement: production_requirement(*advisory),
                        subject: Some(subject),
                        reason: panproto_inst::validation::IncompleteReason::MissingExternalWitness {
                            name: "audit".into(),
                        },
                    });
                    reference_incomplete.push(reference::IncompleteCheck {
                        check: reference_key.clone(),
                        class: reference::CheckClass::BoundedExternal,
                        requirement: reference_requirement(*advisory),
                        subject: Some(reference_subject),
                        reason: reference::IncompleteReason::MissingExternalWitness {
                            name: "audit".to_owned(),
                        },
                    });
                } else {
                    let mut diagnostic = production_open_diagnostic("ghost");
                    diagnostic.requirement = production_requirement(*advisory);
                    diagnostic.subject = Some(subject.clone());
                    diagnostic.elements = vec![subject];
                    diagnostics.push(diagnostic);
                    reference_diagnostics.push(reference::Diagnostic {
                        check: reference_key.clone(),
                        class: reference::CheckClass::BoundedExternal,
                        requirement: reference_requirement(*advisory),
                        subject: Some(reference_subject.clone()),
                        elements: vec![reference_subject],
                        violation: reference::Violation::ExternalRejected("audit".to_owned()),
                    });
                }
            }
            let required_diagnostic = entries.iter().any(|(i, a)| !i && !a);
            let required_incomplete = entries.iter().any(|(i, a)| *i && !a);
            let stated = if required_diagnostic {
                "failed"
            } else if required_incomplete {
                "incomplete"
            } else {
                "passed"
            };
            assert_eq!(
                production_status_observation(aggregate_status(&diagnostics, &incomplete)),
                stated,
                "{entries:?}"
            );
            assert_eq!(
                status_observation(reference::status(
                    &reference_diagnostics,
                    &reference_incomplete
                )),
                stated,
                "{entries:?}"
            );
            sequences += 1;
        }
    }
    assert_eq!(sequences, 1 + 4 + 16 + 64);
}

fn keyed_value(value: &ReferenceValue) -> panproto_inst::Value {
    match value {
        ReferenceValue::Null => panproto_inst::Value::Null,
        ReferenceValue::Float(bits) => panproto_inst::Value::Float(f64::from_bits(*bits)),
        other => production_value(other),
    }
}

fn production_equality(
    equality: &panproto_reference::keys::KeyEquality,
) -> panproto_schema::KeyEquality {
    use panproto_reference::keys::{KeyEquality, SqlCollation, SqlPad};
    match equality {
        KeyEquality::Representation => panproto_schema::KeyEquality::Representation,
        KeyEquality::NumericValue => panproto_schema::KeyEquality::NumericValue,
        KeyEquality::Sql { collation, pad } => panproto_schema::KeyEquality::Sql {
            collation: match collation {
                SqlCollation::Binary => panproto_schema::SqlCollation::Binary,
                SqlCollation::AsciiCaseInsensitive => {
                    panproto_schema::SqlCollation::AsciiCaseInsensitive
                }
                SqlCollation::Named(name) => {
                    panproto_schema::SqlCollation::Named(name.as_str().into())
                }
            },
            pad: match pad {
                SqlPad::NoPad => panproto_schema::SqlPad::NoPad,
                SqlPad::PadSpace => panproto_schema::SqlPad::PadSpace,
            },
        },
        other => panic!("equality outside the kernel carrier: {other:?}"),
    }
}

fn one_part_key(equality: panproto_schema::KeyEquality) -> panproto_schema::Key {
    panproto_schema::Key {
        id: "k".into(),
        parts: vec![panproto_schema::KeyPart::Path(Vec::new())],
        role: panproto_schema::KeyRole::Identifier,
        scope: panproto_schema::KeyScope::Document,
        equality: vec![equality],
    }
}

/// Resolution and validation key a part value by the key function of its
/// declared equality (`canonical_key_bytes`), and two values match exactly
/// when both keys are defined and equal. The reference relation is read as
/// a kernel: a value is keyable when the relation decides it against
/// itself, and a pair is decided only when both of its values are keyable.
/// Over six equalities and seven values (294 ordered pairs, a `Named`
/// collation and null among them) the production kernel and the reference
/// kernel agree on every pair, undecided pairs included.
#[test]
fn declared_key_equality_is_the_kernel_of_the_production_key_function() {
    use panproto_reference::keys::{KeyEquality, SqlCollation, SqlPad};
    use panproto_reference::resolution::declared_key_equality;

    let equalities = [
        KeyEquality::Representation,
        KeyEquality::NumericValue,
        KeyEquality::Sql {
            collation: SqlCollation::Binary,
            pad: SqlPad::NoPad,
        },
        KeyEquality::Sql {
            collation: SqlCollation::AsciiCaseInsensitive,
            pad: SqlPad::NoPad,
        },
        KeyEquality::Sql {
            collation: SqlCollation::Binary,
            pad: SqlPad::PadSpace,
        },
        KeyEquality::Sql {
            collation: SqlCollation::Named("utf8mb4_0900_ai_ci".to_owned()),
            pad: SqlPad::NoPad,
        },
    ];
    let values = [
        ReferenceValue::Int(1),
        ReferenceValue::Float(1.0_f64.to_bits()),
        ReferenceValue::Str("a".to_owned()),
        ReferenceValue::Str("A".to_owned()),
        ReferenceValue::Str("a ".to_owned()),
        ReferenceValue::Bool(true),
        ReferenceValue::Null,
    ];
    let (mut pairs, mut undecided, mut identified) = (0usize, 0usize, 0usize);
    for equality in &equalities {
        let key = one_part_key(production_equality(equality));
        let kappa = |value: &ReferenceValue| {
            production_key::canonical_key_bytes(&key, &[keyed_value(value)]).ok()
        };
        let keyable = |value: &ReferenceValue| declared_key_equality(equality, value, value);
        for left in &values {
            for right in &values {
                let production = match (kappa(left), kappa(right)) {
                    (Some(a), Some(b)) => Some(a == b),
                    _ => None,
                };
                let reference = match (keyable(left), keyable(right)) {
                    (Some(true), Some(true)) => declared_key_equality(equality, left, right),
                    _ => None,
                };
                assert_eq!(
                    production, reference,
                    "{equality:?}: {left:?} against {right:?}"
                );
                undecided += usize::from(production.is_none());
                identified += usize::from(production == Some(true) && left != right);
                pairs += 1;
            }
        }
    }
    assert_eq!(pairs, 294);
    assert!(undecided > 0, "the named collation leaves text undecided");
    assert!(identified > 0, "some equality identifies distinct values");
    eprintln!("key kernel: {pairs} pairs, {undecided} undecided, {identified} identifications");
}

/// Validation's use of a declared equality: `KeyUnique` buckets by the
/// key function, so under a `Named` collation two text values are an
/// undecided `UnsupportedCollation`, never a pass or a byte comparison,
/// while two equal integers on the same part are a decided duplicate.
#[test]
fn key_unique_under_a_named_collation_is_undecided_on_text() {
    use panproto_inst::validation::IncompleteReason;
    let named = panproto_schema::KeyEquality::Sql {
        collation: panproto_schema::SqlCollation::Named("utf8mb4_0900_ai_ci".into()),
        pad: panproto_schema::SqlPad::NoPad,
    };
    let mut schema = Schema::default();
    schema.vertices.insert(
        "item".into(),
        panproto_schema::Vertex {
            id: "item".into(),
            kind: "object".into(),
            nsid: None,
        },
    );
    schema
        .declarations
        .insert_key("item".into(), one_part_key(named));
    let mut protocol = Protocol::default();
    protocol.syntactic_shape = InstanceShapeKind::Graph;
    protocol.semantic_shape = InstanceShapeKind::Graph;
    let plan = ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let graph = |a: panproto_inst::Value, b: panproto_inst::Value| {
        GInstance::new()
            .with_node(Node::new(0, "item").with_value(FieldPresence::Present(a)))
            .with_node(Node::new(1, "item").with_value(FieldPresence::Present(b)))
    };
    let text = plan.validate_graph(
        &graph(
            panproto_inst::Value::Str("a".to_owned()),
            panproto_inst::Value::Str("A".to_owned()),
        ),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(text.status, panproto_gat::CheckStatus::Incomplete, "{text:?}");
    assert!(
        text.incomplete.iter().any(|entry| {
            entry.check.kind == CheckKind::KeyUnique
                && entry.requirement == Requirement::Required
                && matches!(entry.reason, IncompleteReason::UnsupportedCollation { .. })
        }),
        "{text:?}"
    );
    assert!(
        !text
            .diagnostics
            .iter()
            .any(|d| d.check.kind == CheckKind::KeyUnique),
        "{text:?}"
    );
    let numbers = plan.validate_graph(
        &graph(panproto_inst::Value::Int(1), panproto_inst::Value::Int(1)),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(numbers.status, panproto_gat::CheckStatus::Failed, "{numbers:?}");
    assert!(numbers.diagnostics.iter().any(|d| matches!(
        d.violation,
        ProductionViolation::DuplicateKey { .. }
    )));
}

/// The reference's budget refusals collapsed to production's records. The
/// reference records one refusal per skipped instance; production records
/// one per check key, whose `from` is the key's first skipped subject.
fn per_production_key(entries: &[reference::IncompleteCheck]) -> Vec<IncompleteObservation> {
    let mut grouped: Vec<(
        (String, String),
        Option<panproto_reference::lens::ViewRef>,
        IncompleteObservation,
    )> = Vec::new();
    for entry in entries {
        let observation = reference_incomplete(entry);
        let key = (
            observation.check_kind.clone(),
            observation.declaration.clone(),
        );
        let from = match &entry.reason {
            reference::IncompleteReason::Budget { from, .. } => from.clone(),
            other => panic!("unexpected reference refusal: {other:?}"),
        };
        match grouped.iter_mut().find(|(seen, _, _)| *seen == key) {
            Some((_, least, kept)) => {
                if from < *least {
                    *least = from;
                    *kept = observation;
                }
            }
            None => grouped.push((key, from, observation)),
        }
    }
    grouped.into_iter().map(|(_, _, kept)| kept).collect()
}

/// The three clauses of the budget law on carriers whose unlimited report is
/// not `Passed`: every limit from zero to one past the unlimited work count
/// (the sum of the read traces of every scheduled instance, which the
/// reference accumulates without a limit) agrees with the reference, never
/// passes, reproduces the unlimited report from the work count on, and cuts
/// below it.
#[test]
fn finite_budget_law_holds_against_the_reference_on_failing_carriers() {
    let (reference_schema, semantics) = fixture_schema();
    let (schema, protocol) = production_fixture_schema(&reference_schema);
    let plan = ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let edge = reference_schema.edges.iter().next().unwrap().clone();
    let mut observed = 0usize;
    let mut cut_observed = 0usize;
    for (index, graph) in graph_cases(&edge).iter().enumerate().skip(1) {
        let input = reference::ValidationInput::Graph {
            schema: reference_schema.clone(),
            semantics: semantics.clone(),
            instance: graph.clone(),
        };
        let unlimited_reference =
            reference::witness_with_trust(input.clone(), reference::ValidationTrust::default())
                .expect("unlimited reference witness");
        // Production and the reference schedule the same check instances,
        // an attribute check at every typed node included, so the
        // reference's complete schedule is compared directly.
        let schedule = reference::trusted_schedule(&input, &reference::ValidationTrust::default());
        let evaluate = |max_reads: Option<u64>| {
            reference::evaluate_runs(
                reference::InstanceShape::Graph,
                schedule.runs(),
                &[],
                reference::ValidationBudget { max_reads },
            )
            .expect("the reference schedule is canonical")
        };
        let work = evaluate(None).spent;
        assert!(work > 0, "graph-{index} reads something");
        assert_ne!(
            unlimited_reference.evaluation.report.status,
            reference::CheckStatus::Passed,
            "graph-{index} is a failing carrier"
        );
        let production_graph = production_graph(graph);
        let unlimited = plan.validate_graph(
            &production_graph,
            &ValidationBudget::default(),
            &ExternalVerdicts::default(),
        );
        assert_ne!(unlimited.status, panproto_gat::CheckStatus::Passed);
        for limit in 0..=work + 1 {
            let expected = evaluate(Some(limit));
            let actual = plan.validate_graph(
                &production_graph,
                &ValidationBudget {
                    max_reads: Some(limit),
                },
                &ExternalVerdicts::default(),
            );
            let label = format!("graph-{index} at limit {limit}");
            assert_eq!(
                production_status_observation(actual.status),
                status_observation(expected.report.status),
                "{label}"
            );
            assert_eq!(
                actual
                    .diagnostics
                    .iter()
                    .map(production_diagnostic)
                    .collect::<Vec<_>>(),
                expected
                    .report
                    .diagnostics
                    .iter()
                    .map(reference_diagnostic)
                    .collect::<Vec<_>>(),
                "{label}"
            );
            assert_eq!(
                actual
                    .incomplete
                    .iter()
                    .map(production_incomplete)
                    .collect::<Vec<_>>(),
                per_production_key(&expected.report.incomplete),
                "{label}"
            );
            assert_ne!(
                actual.status,
                panproto_gat::CheckStatus::Passed,
                "a finite budget never passes a failing carrier: {label}"
            );
            if limit >= work {
                assert_eq!(
                    actual.canonical_bytes(),
                    unlimited.canonical_bytes(),
                    "{label}"
                );
            } else {
                assert!(!actual.incomplete.is_empty(), "{label} cuts");
                cut_observed += 1;
            }
            observed += 1;
        }
    }
    assert!(cut_observed > 0);
    eprintln!("budget law: {observed} limits observed, {cut_observed} of them cut");
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
                // A direct restatement of the definition, not production
                // code: the runtime resolver is exercised in panproto-io.
                let candidates = definitions
                    .iter()
                    .filter(|(defined, candidate, _)| *defined < position && candidate == name);
                let direct = if nearest {
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
                    direct
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
        let datatype = format!("{XSD}integer");
        let reference = reference::lexical_space(text, &datatype);
        let (passed, reported, incomplete) = production_lexical_observation(text, &datatype);
        assert!(passed, "lexical-space diagnostics are advisory");
        assert!(!incomplete, "integer has a declared lexical kind");
        assert_eq!(Some(!reported), reference);
        lexical_count += 1;
    }
    assert_eq!(lexical_count, 4);

    // Annotated formats under H72(d): (1) without the assertion vocabulary
    // the parse coerces under the kind from `type` alone, so a schema with a
    // format parses exactly as the schema without it; (3) with it, the width
    // becomes the integer-width parameter and acceptance is coercion under
    // the kind with that width; a named format keeps its required check.
    let grid = [
        "0",
        "1.0",
        "1e1",
        "1.5",
        "2147483648",
        "1099511627776",
        "9223372036854775808",
        "0.1",
        "1e39",
    ];
    let kinds = [
        (
            "integer",
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
            vec![None, Some("int32"), Some("int64"), Some("x-unknown")],
        ),
        (
            "number",
            ScalarKind::Float { bits: 64 },
            vec![None, Some("float"), Some("double")],
        ),
    ];
    let format_protocol = format_parse_protocol();
    let mut parse_count = 0usize;
    let mut assertion_count = 0usize;
    for (vertex_kind, kind, formats) in &kinds {
        let bare = format_parse_spec(&format_protocol, vertex_kind, None, None);
        for format in formats {
            let annotated = format_parse_spec(&format_protocol, vertex_kind, *format, None);
            assert_eq!(annotated, bare, "{vertex_kind} {format:?}: a format never enters the spec");
            for lexeme in grid {
                let production = panproto_inst::value::coerce::coerce(
                    &panproto_inst::value::coerce::Lexeme::new(
                        lexeme,
                        panproto_inst::value::coerce::LexClass::Number,
                    ),
                    &annotated,
                );
                let expected = reference::annotated_format_parse(
                    lexeme,
                    &panproto_reference::coerce::LexClass::Number,
                    reference_kind(kind),
                    *format,
                );
                assert_eq!(
                    coerced_observation(production.as_ref().ok()),
                    reference_coerced_observation(&expected),
                    "{vertex_kind} {format:?} {lexeme}"
                );
                parse_count += 1;
            }
        }
        for format in ["int32", "int64"] {
            if !matches!(kind, ScalarKind::Int { .. }) {
                continue;
            }
            let bits = if format == "int32" { "32" } else { "64" };
            let asserted = format_parse_spec(&format_protocol, vertex_kind, None, Some(bits));
            for lexeme in grid {
                let accepted = panproto_inst::value::coerce::coerce(
                    &panproto_inst::value::coerce::Lexeme::new(
                        lexeme,
                        panproto_inst::value::coerce::LexClass::Number,
                    ),
                    &asserted,
                )
                .is_ok();
                assert_eq!(
                    Some(accepted),
                    reference::format_assertion_acceptance(
                        lexeme,
                        &panproto_reference::coerce::LexClass::Number,
                        reference_kind(kind),
                        format,
                    ),
                    "asserted {format} {lexeme}"
                );
                assertion_count += 1;
            }
        }
    }
    assert!(
        format_parse_spec(&format_protocol, "integer", None, Some("32")).int_bits == Some(32),
        "the asserted width reaches the spec"
    );
    // A named asserting format keeps its required check, passing exactly the
    // values the independent RFC 4122 recognizer accepts.
    for text in [
        "550e8400-e29b-41d4-a716-446655440000",
        "550E8400-E29B-41D4-A716-446655440000",
        "550e8400-e29b-41d4-a716-44665544000",
        "550e8400e29b41d4a716446655440000",
        "not-a-uuid",
    ] {
        let (passed, reported, incomplete, _) = production_format_observation(text, false);
        assert!(!incomplete, "uuid has a settled recognizer");
        assert_eq!(passed, reference::uuid_text(text), "asserted uuid {text}");
        assert_eq!(reported, !passed);
        assertion_count += 1;
    }

    // (2) An annotated format reports exactly the values whose coercion
    // under the width format's kind fails, or that a named format's
    // recognizer rejects; any other format compiles to no check.
    let values = [
        ReferenceValue::Int(0),
        ReferenceValue::Int(2_147_483_648),
        ReferenceValue::Int(1_099_511_627_776),
        ReferenceValue::Float(1.5_f64.to_bits()),
        ReferenceValue::Float(0.1_f64.to_bits()),
        ReferenceValue::Float(1e39_f64.to_bits()),
        ReferenceValue::Str("550e8400-e29b-41d4-a716-446655440000".to_owned()),
        ReferenceValue::Str("not-a-uuid".to_owned()),
    ];
    let mut advisory_count = 0usize;
    for format in ["int32", "int64", "float", "double", "uuid", "x-unknown"] {
        for value in &values {
            let (passed, reported, compiled) = production_annotated_observation(value, format);
            assert!(passed, "an annotated format never fails the status");
            match reference::annotated_format_advisory(value, format) {
                Some(expected) => {
                    assert!(compiled, "{format} compiles to a check");
                    assert_eq!(reported, expected, "{format} {value:?}");
                }
                None => assert!(!compiled && !reported, "{format} compiles to no check"),
            }
            advisory_count += 1;
        }
    }
    assert_eq!(parse_count, 63);
    assert_eq!(assertion_count, 23);
    assert_eq!(advisory_count, 48);
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
