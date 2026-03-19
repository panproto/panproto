//! Integration test 9: Breaking change detection.
//!
//! Verifies that known `ATProto` lexicon breaking changes are correctly
//! detected by the diff/classify pipeline.

use std::collections::HashMap;

use panproto_check::{BreakingChange, classify, diff, report_text};
use panproto_gat::Name;
use panproto_protocols::atproto;
use panproto_schema::{Constraint, Edge, Schema, Vertex};
use smallvec::SmallVec;

/// Build a schema from vertices, edges, and optional constraints.
fn make_schema(
    verts: &[(&str, &str)],
    edge_list: &[Edge],
    constraints: HashMap<Name, Vec<Constraint>>,
) -> Schema {
    let mut vertices = HashMap::new();
    let mut edges = HashMap::new();
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

    for (id, kind) in verts {
        vertices.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
                nsid: None,
            },
        );
    }
    for e in edge_list {
        edges.insert(e.clone(), e.kind.clone());
        outgoing.entry(e.src.clone()).or_default().push(e.clone());
        incoming.entry(e.tgt.clone()).or_default().push(e.clone());
        between
            .entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
    }

    Schema {
        protocol: "atproto".into(),
        vertices,
        edges,
        hyper_edges: HashMap::new(),
        constraints,
        required: HashMap::new(),
        nsids: HashMap::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        coercions: HashMap::new(),
        mergers: HashMap::new(),
        defaults: HashMap::new(),
        policies: HashMap::new(),
        outgoing,
        incoming,
        between,
    }
}

#[test]
fn maxlength_tightening_detected() {
    // ATProto breaking change: maxLength 3000 -> 300 on text field.
    let protocol = atproto::protocol();

    let edge = Edge {
        src: "body".into(),
        tgt: "body.text".into(),
        kind: "prop".into(),
        name: Some("text".into()),
    };

    let old = make_schema(
        &[("body", "object"), ("body.text", "string")],
        std::slice::from_ref(&edge),
        HashMap::from([(
            Name::from("body.text"),
            vec![Constraint {
                sort: "maxLength".into(),
                value: "3000".into(),
            }],
        )]),
    );

    let new = make_schema(
        &[("body", "object"), ("body.text", "string")],
        std::slice::from_ref(&edge),
        HashMap::from([(
            Name::from("body.text"),
            vec![Constraint {
                sort: "maxLength".into(),
                value: "300".into(),
            }],
        )]),
    );

    let d = diff(&old, &new);
    let report = classify(&d, &protocol);

    assert!(
        !report.compatible,
        "tightening maxLength should be breaking"
    );
    assert!(
        report.breaking.iter().any(
            |b| matches!(b, BreakingChange::ConstraintTightened { sort, .. } if sort == "maxLength")
        ),
        "should detect maxLength tightening"
    );
}

#[test]
fn required_field_removal_detected() {
    let protocol = atproto::protocol();

    let edge_text = Edge {
        src: "body".into(),
        tgt: "body.text".into(),
        kind: "prop".into(),
        name: Some("text".into()),
    };
    let edge_time = Edge {
        src: "body".into(),
        tgt: "body.createdAt".into(),
        kind: "prop".into(),
        name: Some("createdAt".into()),
    };

    let old = make_schema(
        &[
            ("body", "object"),
            ("body.text", "string"),
            ("body.createdAt", "string"),
        ],
        &[edge_text.clone(), edge_time],
        HashMap::new(),
    );

    // New schema removes createdAt.
    let new = make_schema(
        &[("body", "object"), ("body.text", "string")],
        &[edge_text],
        HashMap::new(),
    );

    let d = diff(&old, &new);
    let report = classify(&d, &protocol);

    assert!(!report.compatible, "removing a field should be breaking");
    assert!(
        report.breaking.iter().any(|b| matches!(b, BreakingChange::RemovedVertex { vertex_id } if vertex_id == "body.createdAt")),
        "should detect removed vertex"
    );
    assert!(
        report.breaking.iter().any(|b| matches!(b, BreakingChange::RemovedEdge { name, .. } if name.as_deref() == Some("createdAt"))),
        "should detect removed edge"
    );
}

#[test]
fn kind_change_detected() {
    let protocol = atproto::protocol();

    let edge = Edge {
        src: "body".into(),
        tgt: "body.count".into(),
        kind: "prop".into(),
        name: Some("count".into()),
    };

    let old = make_schema(
        &[("body", "object"), ("body.count", "string")],
        std::slice::from_ref(&edge),
        HashMap::new(),
    );

    let new = make_schema(
        &[("body", "object"), ("body.count", "integer")],
        std::slice::from_ref(&edge),
        HashMap::new(),
    );

    let d = diff(&old, &new);
    let report = classify(&d, &protocol);

    assert!(!report.compatible, "changing kind should be breaking");
    assert!(
        report.breaking.iter().any(|b| matches!(b, BreakingChange::KindChanged { vertex_id, .. } if vertex_id == "body.count")),
        "should detect kind change"
    );
}

#[test]
fn adding_optional_field_is_non_breaking() {
    let protocol = atproto::protocol();

    let edge_text = Edge {
        src: "body".into(),
        tgt: "body.text".into(),
        kind: "prop".into(),
        name: Some("text".into()),
    };

    let old = make_schema(
        &[("body", "object"), ("body.text", "string")],
        std::slice::from_ref(&edge_text),
        HashMap::new(),
    );

    let edge_new = Edge {
        src: "body".into(),
        tgt: "body.langs".into(),
        kind: "prop".into(),
        name: Some("langs".into()),
    };

    let new = make_schema(
        &[
            ("body", "object"),
            ("body.text", "string"),
            ("body.langs", "array"),
        ],
        &[edge_text, edge_new],
        HashMap::new(),
    );

    let d = diff(&old, &new);
    let report = classify(&d, &protocol);

    assert!(
        report.compatible,
        "adding optional field should be non-breaking"
    );
}

#[test]
fn report_text_output_is_informative() {
    let protocol = atproto::protocol();

    let edge = Edge {
        src: "body".into(),
        tgt: "body.text".into(),
        kind: "prop".into(),
        name: Some("text".into()),
    };

    let old = make_schema(
        &[("body", "object"), ("body.text", "string")],
        std::slice::from_ref(&edge),
        HashMap::from([(
            Name::from("body.text"),
            vec![Constraint {
                sort: "maxLength".into(),
                value: "3000".into(),
            }],
        )]),
    );

    let new = make_schema(
        &[("body", "object"), ("body.text", "string")],
        std::slice::from_ref(&edge),
        HashMap::from([(
            Name::from("body.text"),
            vec![Constraint {
                sort: "maxLength".into(),
                value: "300".into(),
            }],
        )]),
    );

    let d = diff(&old, &new);
    let report = classify(&d, &protocol);
    let text = report_text(&report);

    assert!(
        text.contains("INCOMPATIBLE"),
        "report should say incompatible"
    );
    assert!(
        text.contains("maxLength"),
        "report should mention maxLength"
    );
    assert!(text.contains("3000"), "report should mention old value");
    assert!(text.contains("300"), "report should mention new value");
}
