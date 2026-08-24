//! What a three-way merge does with two branches that add the same name.
//!
//! Neither branch inherited the addition, so the pushout of the two over
//! their base keeps a copy for each and the merge keeps one. That quotient is
//! what a name-addressed schema wants -- two branches that add `x: string`
//! have added the same field, and conflicting on it would leave nothing to
//! resolve -- but it is a quotient, and the merged schema holds one element
//! where the free pushout holds two.
//!
//! This file pins that semantics and the report that makes it visible: the
//! merge names every addition it identified, so a caller who wants both
//! copies can rename one side and merge again.

#![allow(clippy::expect_used)]

use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};
use panproto_vcs::merge::{IdentifiedAddition, MergeConflict, three_way_merge};

fn test_protocol() -> Protocol {
    Protocol {
        name: "test".into(),
        schema_theory: "ThTest".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![],
        obj_kinds: vec!["object".into(), "string".into(), "integer".into()],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

fn build(vertices: &[(&str, &str)], edges: &[(&str, &str, &str, &str)]) -> Schema {
    let proto = test_protocol();
    let mut builder = SchemaBuilder::new(&proto);
    for (id, kind) in vertices {
        builder = builder.vertex(id, kind, None::<&str>).expect("vertex");
    }
    for (src, tgt, kind, name) in edges {
        builder = builder.edge(src, tgt, kind, Some(*name)).expect("edge");
    }
    builder.build().expect("schema")
}

#[test]
fn two_branches_adding_the_same_field_get_one_field_and_a_report() {
    let base = build(&[("root", "object")], &[]);
    let ours = build(
        &[("root", "object"), ("x", "string")],
        &[("root", "x", "prop", "x")],
    );
    let theirs = ours.clone();

    let merged = three_way_merge(&base, &ours, &theirs);

    assert!(
        merged.conflicts.is_empty(),
        "two additions that agree are not a conflict: {:?}",
        merged.conflicts,
    );
    assert_eq!(
        merged.merged_schema.vertices.len(),
        2,
        "the merge keeps one `x`, not one per branch",
    );

    let expected_edge = Edge {
        src: "root".into(),
        tgt: "x".into(),
        kind: "prop".into(),
        name: Some("x".into()),
    };
    assert_eq!(
        merged.identified_additions,
        vec![
            IdentifiedAddition::Vertex {
                vertex_id: "x".to_owned(),
            },
            IdentifiedAddition::Edge {
                edge: expected_edge,
            },
        ],
        "the merge names the vertex and the edge it identified",
    );
}

#[test]
fn an_addition_only_one_branch_made_is_not_an_identification() {
    let base = build(&[("root", "object")], &[]);
    let ours = build(
        &[("root", "object"), ("x", "string")],
        &[("root", "x", "prop", "x")],
    );
    let theirs = base.clone();

    let merged = three_way_merge(&base, &ours, &theirs);

    assert!(
        merged.identified_additions.is_empty(),
        "nothing was identified: {:?}",
        merged.identified_additions,
    );
    assert_eq!(
        merged.merged_schema.vertices.len(),
        2,
        "the one-sided addition is accepted",
    );
}

#[test]
fn two_branches_adding_the_same_name_differently_still_conflict() {
    let base = build(&[("root", "object")], &[]);
    let ours = build(&[("root", "object"), ("x", "string")], &[]);
    let theirs = build(&[("root", "object"), ("x", "integer")], &[]);

    let merged = three_way_merge(&base, &ours, &theirs);

    assert!(
        merged.identified_additions.is_empty(),
        "additions that disagree are not identified: {:?}",
        merged.identified_additions,
    );
    assert!(
        merged.conflicts.iter().any(|c| matches!(
            c,
            MergeConflict::BothAddedVertexDifferently { vertex_id, .. } if vertex_id == "x"
        )),
        "they conflict instead: {:?}",
        merged.conflicts,
    );
}

#[test]
fn the_report_is_the_same_in_every_process() {
    let base = build(&[("root", "object")], &[]);
    let ours = build(
        &[
            ("root", "object"),
            ("x", "string"),
            ("y", "integer"),
            ("z", "string"),
        ],
        &[
            ("root", "x", "prop", "x"),
            ("root", "y", "prop", "y"),
            ("root", "z", "prop", "z"),
        ],
    );
    let theirs = ours.clone();

    let first = three_way_merge(&base, &ours, &theirs).identified_additions;
    assert_eq!(first.len(), 6, "three vertices and three edges: {first:?}");
    for _ in 0..16 {
        assert_eq!(
            three_way_merge(&base, &ours, &theirs).identified_additions,
            first,
            "the reported order must not depend on hash iteration order",
        );
    }
}
