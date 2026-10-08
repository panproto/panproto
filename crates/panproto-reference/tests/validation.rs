#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The probe fixture of validation: a ghost anchor, an undeclared label, an
//! incoherent endpoint, a missing required edge and a length bound.

use std::collections::BTreeMap;

use panproto_reference::model::{GEdge, Graph, Node, SEdge};
use panproto_reference::schema::{
    Cardinality, Key, KeyPart, KeyRole, KeyScope, OnUnresolved, PathSyntax, RefSource, Reference,
    ResolveMode, Schema,
};
use panproto_reference::validation::{self, CheckKind, CheckStatus, Semantics};
use panproto_reference::value::Value;

#[test]
fn the_probe_instance_fails_five_ways() {
    let works = SEdge::new("person", "org", "prop", Some("worksAt"));
    let mut s = Schema::default()
        .vertex("person", "obj")
        .vertex("org", "obj")
        .vertex("name", "str")
        .edge("person", "org", "prop", Some("worksAt"))
        .edge("person", "name", "prop", Some("name"));
    s.required.insert("person".into(), vec![works.clone()]);
    s.constraints
        .insert("name".into(), vec![("maxLength".into(), "3".into())]);
    let sem = BTreeMap::from([("maxLength".to_owned(), Semantics::MaxLength(true))]);
    let mut g = Graph::default();
    g.nodes.insert(
        0,
        Node::new(0, "person").with_field("name", Value::Str("Alice".into())),
    );
    g.nodes.insert(1, Node::new(1, "ghost"));
    g.nodes.insert(
        2,
        Node::new(2, "person").with_field("name", Value::Str("Bob".into())),
    );
    g.edges.insert(
        0,
        GEdge {
            src: 0,
            tgt: 2,
            label: works,
            position: None,
        },
    );
    g.edges.insert(
        1,
        GEdge {
            src: 2,
            tgt: 0,
            label: SEdge::new("person", "person", "prop", Some("knows")),
            position: None,
        },
    );
    let r = validation::validate(&s, &sem, &g).unwrap();
    let kinds: Vec<CheckKind> = r.diagnostics.iter().map(|d| d.check.kind).collect();
    assert_eq!(
        kinds,
        vec![
            CheckKind::NodeAnchor,
            CheckKind::EdgeDeclared,
            CheckKind::EndpointCoherence,
            CheckKind::RequiredEdge,
            CheckKind::ValueConstraint
        ]
    );
    assert_eq!(r.status, CheckStatus::Failed);
    assert!(validation::validate(&s, &BTreeMap::new(), &g).is_none());
}

#[test]
fn the_status_is_kleene_conjunction() {
    use CheckStatus::{Failed, Incomplete, Passed};
    for a in [Passed, Failed, Incomplete] {
        for b in [Passed, Failed, Incomplete] {
            let expected = if a == Failed || b == Failed {
                Failed
            } else if a == Incomplete || b == Incomplete {
                Incomplete
            } else {
                Passed
            };
            assert_eq!(a.and(b), expected);
        }
    }
    assert_eq!(CheckStatus::all([]), Passed);
}

#[test]
fn an_edge_to_a_missing_node_is_an_endpoint_finding_not_a_panic() {
    let works = SEdge::new("person", "org", "prop", Some("worksAt"));
    let s = Schema::default()
        .vertex("person", "obj")
        .vertex("org", "obj")
        .edge("person", "org", "prop", Some("worksAt"));
    let mut g = Graph::default();
    g.nodes.insert(0, Node::new(0, "person"));
    g.edges.insert(
        0,
        GEdge {
            src: 0,
            tgt: 9,
            label: works,
            position: None,
        },
    );
    let r = validation::validate(&s, &BTreeMap::new(), &g).unwrap();
    assert_eq!(r.diagnostics.len(), 1);
    assert_eq!(r.diagnostics[0].check.kind, CheckKind::EndpointCoherence);
    assert_eq!(
        format!("{:?}", r.diagnostics[0].violation),
        "DanglingEndpoint(false, 9)"
    );
    assert_eq!(r.status, CheckStatus::Failed);
}

#[test]
fn path_and_many_references_are_executable_or_explicitly_incomplete() {
    let (schema, graph) = path_and_many_fixture();
    let report = validation::validate(&schema, &BTreeMap::new(), &graph).unwrap();
    assert!(report.diagnostics.iter().any(|diagnostic| {
        matches!(
            &diagnostic.violation,
            panproto_reference::validation::Violation::DanglingReference(reference, values)
                if reference == "path" && values == &[Value::Str("/missing".into())]
        )
    }));
    assert!(report.incomplete.iter().any(|(check, _, subject, detail)| {
        check.kind == CheckKind::ReferenceResolves
            && subject == &Some(panproto_reference::lens::ViewRef::Node(0))
            && detail.contains("ReferenceCardinalityMismatch")
    }));
    assert_eq!(report.status, CheckStatus::Failed);

    let mut invalid = schema;
    invalid.references.get_mut("path").unwrap().target_key = Some("absent".into());
    assert!(validation::validate(&invalid, &BTreeMap::new(), &graph).is_none());
}

fn path_and_many_fixture() -> (Schema, Graph) {
    let path = SEdge::new("holder", "scalar", "prop", Some("path"));
    let left = SEdge::new("holder", "scalar", "prop", Some("left"));
    let right = SEdge::new("holder", "scalar", "prop", Some("right"));
    let id = SEdge::new("target", "scalar", "prop", Some("id"));
    let x = SEdge::new("target", "scalar", "prop", Some("x"));
    let y = SEdge::new("target", "scalar", "prop", Some("y"));
    let path_link = SEdge::new("holder", "target", "ref", Some("path_target"));
    let many_link = SEdge::new("holder", "target", "ref", Some("many_target"));
    let mut schema = Schema::default()
        .vertex("holder", "object")
        .vertex("target", "object")
        .vertex("scalar", "value");
    schema.edges.extend([
        path.clone(),
        left.clone(),
        right.clone(),
        id.clone(),
        x.clone(),
        y.clone(),
        path_link.clone(),
        many_link.clone(),
    ]);
    schema.keys.insert(
        "target".into(),
        vec![
            Key {
                id: "id".into(),
                parts: vec![KeyPart::Path(vec![id])],
                role: KeyRole::Identifier,
                scope: KeyScope::Document,
                equality: Vec::new(),
            },
            Key {
                id: "pair".into(),
                parts: vec![KeyPart::Path(vec![x]), KeyPart::Path(vec![y])],
                role: KeyRole::Unique,
                scope: KeyScope::Document,
                equality: Vec::new(),
            },
        ],
    );
    let reference = |id: &str, source: RefSource, key: &str, edge: SEdge| Reference {
        id: id.into(),
        holder: "holder".into(),
        source,
        target_vertex: "target".into(),
        target_key: Some(key.into()),
        edge: Some(edge),
        cardinality: if id == "many" {
            Cardinality::Many
        } else {
            Cardinality::One
        },
        nullable: false,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
        guard: None,
        null_match: panproto_reference::schema::NullMatch::Simple,
        null_vertex: None,
        scope_via: None,
    };
    schema.references.insert(
        "path".into(),
        reference(
            "path",
            RefSource::Path {
                field: vec![path],
                syntax: PathSyntax::JsonPointer,
            },
            "id",
            path_link,
        ),
    );
    schema.references.insert(
        "many".into(),
        reference(
            "many",
            RefSource::KeyValue {
                parts: vec![vec![left], vec![right]],
            },
            "pair",
            many_link,
        ),
    );
    (schema, path_and_many_graph())
}

fn path_and_many_graph() -> Graph {
    Graph {
        nodes: BTreeMap::from([
            (
                0,
                Node::new(0, "holder")
                    .with_field("path", Value::Str("/missing".into()))
                    .with_field("left", Value::List(vec![Value::Int(1), Value::Int(2)]))
                    .with_field("right", Value::List(vec![Value::Int(1)])),
            ),
            (
                1,
                Node::new(1, "target")
                    .with_field("id", Value::Str("/present".into()))
                    .with_field("x", Value::Int(1))
                    .with_field("y", Value::Int(1)),
            ),
        ]),
        edges: BTreeMap::new(),
    }
}
