#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The probe fixture of validation: a ghost anchor, an undeclared label, an
//! incoherent endpoint, a missing required edge and a length bound.

use std::collections::BTreeMap;

use panproto_reference::model::{GEdge, Graph, Node, SEdge};
use panproto_reference::schema::Schema;
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
