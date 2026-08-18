//! A fixpoint marker is named once, by the key it is filed under.
//!
//! [`RecursionPoint`] used to repeat that name in a `mu_id` field, and the two
//! copies were read by different call sites: inducing, diffing, hashing and the
//! AT Protocol emitter keyed on the map, while the search network and the
//! span's right leg read the field. The invariant that tied them together lived in a
//! doc comment and a `debug_assert`, and neither is on the deserialisation
//! path, so a schema keying a marker by anything else was accepted by
//! [`validate`] and then behaved differently in each build profile: a panic in
//! debug, and in release a silent drop of the marker from the induced apex
//! while both of its vertices survived.
//!
//! The field is gone, so the disagreement has no room to exist and the first
//! test here is a compile-time claim as much as a run-time one. What remains
//! reachable is a marker naming a vertex that is not in the schema, which is a
//! dangling reference rather than a contradiction, and [`validate`] now reports
//! it rather than leaving inducing to drop it quietly.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::HashSet;

use panproto_gat::Name;
use panproto_schema::{
    EdgeRule, Protocol, RecursionPoint, Schema, SchemaBuilder, ValidationError, induce, validate,
};

fn protocol() -> Protocol {
    Protocol {
        name: "recursive".to_owned(),
        schema_theory: "ThTest".to_owned(),
        instance_theory: "ThWType".to_owned(),
        edge_rules: vec![EdgeRule {
            edge_kind: "prop".to_owned(),
            src_kinds: vec!["object".to_owned()],
            tgt_kinds: vec!["object".to_owned()],
        }],
        obj_kinds: vec!["object".to_owned()],
        ..Protocol::default()
    }
}

fn two_vertices() -> Schema {
    SchemaBuilder::new(&protocol())
        .vertex("mu", "object", None::<&str>)
        .expect("mu")
        .vertex("body", "object", None::<&str>)
        .expect("body")
        .edge("mu", "body", "prop", Some("f"))
        .expect("edge")
        .entry("mu")
        .build()
        .expect("schema")
}

fn induce_all(schema: &Schema) -> Result<Schema, panproto_schema::SchemaError> {
    let keep_v: HashSet<Name> = schema.vertices.keys().cloned().collect();
    let keep_e: HashSet<_> = schema.edges.keys().cloned().collect();
    induce(schema, &protocol(), &keep_v, &keep_e)
}

/// A marker survives a serde round trip and an induction that keeps everything.
///
/// The round trip is the part that matters: deserialisation is the one path
/// that used to be able to introduce a marker whose two names disagreed, since
/// no builder could write one and `validate` did not look.
#[test]
fn a_marker_survives_the_round_trip_that_used_to_break_it() {
    let mut schema = two_vertices();
    schema.recursion_points.insert(
        Name::from("mu"),
        RecursionPoint {
            target_vertex: Name::from("body"),
        },
    );

    let json = serde_json::to_string(&schema).expect("serialise");
    let back: Schema = serde_json::from_str(&json).expect("deserialise");

    assert!(
        validate(&back, &protocol()).is_empty(),
        "a well-formed marker must validate clean: {:?}",
        validate(&back, &protocol())
    );

    let apex = induce_all(&back).expect("inducing everything cannot refuse");
    assert_eq!(
        apex.recursion_points.len(),
        1,
        "the marker must reach the apex, not be dropped on the way"
    );
    assert_eq!(
        apex.recursion_points
            .get(&Name::from("mu"))
            .expect("filed under its marker vertex")
            .target_vertex,
        Name::from("body")
    );
}

/// A marker keyed by a vertex the schema does not have is reported, not dropped.
///
/// This is the residue of the original defect. It can no longer contradict
/// itself, but it can still name something absent, and the failure mode was the
/// same: inducing keeps a marker only when both ends survive, so it vanished
/// from the apex while the apex's certificate still claimed full coverage.
#[test]
fn a_marker_keyed_by_a_missing_vertex_is_reported() {
    let mut schema = two_vertices();
    schema.recursion_points.insert(
        Name::from("not-a-vertex"),
        RecursionPoint {
            target_vertex: Name::from("body"),
        },
    );

    let findings = validate(&schema, &protocol());
    assert!(
        findings.iter().any(|finding| matches!(
            finding,
            ValidationError::DanglingRecursionPoint { mu, .. } if mu == "not-a-vertex"
        )),
        "a marker on a vertex that is not there has to be reported: {findings:?}"
    );
}

/// And the same when the vertex it unfolds to is the missing one.
#[test]
fn a_marker_unfolding_to_a_missing_vertex_is_reported() {
    let mut schema = two_vertices();
    schema.recursion_points.insert(
        Name::from("mu"),
        RecursionPoint {
            target_vertex: Name::from("gone"),
        },
    );

    let findings = validate(&schema, &protocol());
    assert!(
        findings.iter().any(|finding| matches!(
            finding,
            ValidationError::DanglingRecursionPoint { missing, .. } if missing == "gone"
        )),
        "a marker unfolding to a vertex that is not there has to be reported: {findings:?}"
    );
}

/// Inducing behaves identically in both profiles.
///
/// The old `debug_assert` made this an assertion in debug and a silent filter
/// in release, so a developer saw a crash where a shipped build lost data. No
/// assertion is left to differ, and this pins the behaviour rather than the
/// absence of the assertion: a marker whose ends are cut is dropped, and one
/// whose ends survive is kept, whatever the build.
#[test]
fn inducing_drops_a_marker_only_when_an_end_is_cut() {
    let mut schema = two_vertices();
    schema.recursion_points.insert(
        Name::from("mu"),
        RecursionPoint {
            target_vertex: Name::from("body"),
        },
    );

    let kept = induce_all(&schema).expect("keeping everything");
    assert_eq!(kept.recursion_points.len(), 1);

    let keep_v: HashSet<Name> = std::iter::once(Name::from("mu")).collect();
    let cut = induce(&schema, &protocol(), &keep_v, &HashSet::new()).expect("cutting the target");
    assert!(
        cut.recursion_points.is_empty(),
        "a marker whose target was cut cannot survive: {:?}",
        cut.recursion_points
    );
}
