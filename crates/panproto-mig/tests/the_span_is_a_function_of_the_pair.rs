//! Whether the span depends on anything but the two schemas it is given.
//!
//! [`SchemaSpan::right`]'s doc says the edge map "sends each apex edge to the
//! target edge [`edge_image`] picks between the images of its endpoints". That
//! makes the span a function of the pair only if `edge_image` is itself a
//! function of the target's edge set, and among parallel target edges of one
//! kind it has to break a tie. Taking the first the slice happens to hold does
//! not qualify, because [`Schema::edges_between`] returns its index in whatever
//! order built the schema.
//!
//! Two things build that order badly. A schema assembled by iterating a
//! `HashMap`, which is what a three-way merge produces, carries that map's
//! per-process bucket order, so the answer would move between runs of one
//! unchanged program. And a schema assembled by a builder carries insertion
//! order, so the answer would move between two schemas that are equal as
//! values. The second is the sharper statement, because no hashing is involved
//! in it at all, and it is what this file pins: the same edge set, added in two
//! different orders, has to give the same span.
//!
//! The apex hides all of this. It is canonical and sorts everything it reads,
//! so it stays byte-identical however the right leg moves underneath it, and a
//! test comparing apexes would pass either way.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use panproto_mig::hom_search::{SearchOptions, find_span};
use panproto_mig::solve::build::edge_image;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

fn protocol() -> Protocol {
    Protocol {
        name: "parallel".to_owned(),
        schema_theory: "ThTest".to_owned(),
        instance_theory: "ThWType".to_owned(),
        edge_rules: vec![EdgeRule {
            edge_kind: "prop".to_owned(),
            src_kinds: vec!["object".to_owned()],
            tgt_kinds: vec!["string".to_owned()],
        }],
        obj_kinds: vec!["object".to_owned(), "string".to_owned()],
        ..Protocol::default()
    }
}

/// A source with one named edge, so the span has exactly one edge to place.
fn source() -> Schema {
    SchemaBuilder::new(&protocol())
        .vertex("rec", "object", None::<&str>)
        .expect("rec")
        .vertex("rec.value", "string", None::<&str>)
        .expect("value")
        .edge("rec", "rec.value", "prop", Some("omega"))
        .expect("omega")
        .entry("rec")
        .build()
        .expect("source")
}

/// A target holding four parallel edges of one kind, added in `order`.
///
/// None of the four shares the source edge's name, so `edge_image`'s first
/// stage misses on all of them and the kind-only fallback is what decides.
fn target(order: &[&str]) -> Schema {
    let mut builder = SchemaBuilder::new(&protocol())
        .vertex("rec", "object", None::<&str>)
        .expect("rec")
        .vertex("rec.value", "string", None::<&str>)
        .expect("value");
    for name in order {
        builder = builder
            .edge("rec", "rec.value", "prop", Some(*name))
            .expect("parallel edge");
    }
    builder.entry("rec").build().expect("target")
}

const FORWARD: [&str; 4] = ["alpha", "beta", "gamma", "delta"];
const REVERSED: [&str; 4] = ["delta", "gamma", "beta", "alpha"];
const SHUFFLED: [&str; 4] = ["gamma", "alpha", "delta", "beta"];

/// The two targets really are the same schema, so any difference downstream is
/// a difference the schemas do not license.
#[test]
fn the_orders_build_equal_edge_sets() {
    let forward = target(&FORWARD);
    let reversed = target(&REVERSED);
    assert_eq!(
        forward.edges, reversed.edges,
        "the fixture is meant to vary insertion order and nothing else"
    );
    assert_eq!(forward.vertices, reversed.vertices);

    // And the index that carries the order really does differ, so the test is
    // not passing because the difference was normalised away before it counted.
    let names = |schema: &Schema| -> Vec<Option<String>> {
        schema
            .edges_between("rec", "rec.value")
            .iter()
            .map(|edge| edge.name.as_ref().map(ToString::to_string))
            .collect()
    };
    assert_ne!(
        names(&forward),
        names(&reversed),
        "the fixture is meant to leave the two indices in different orders"
    );
}

/// `edge_image` itself, with no search around it.
#[test]
fn edge_image_picks_by_value_not_by_position() {
    let src = source();
    let edge = src
        .edges
        .keys()
        .find(|edge| edge.name.as_deref() == Some("omega"))
        .expect("the omega edge")
        .clone();

    let chosen = |order: &[&str]| -> String {
        let tgt = target(order);
        edge_image(&tgt, &edge, &"rec".into(), &"rec.value".into())
            .expect("a parallel edge of the right kind is always available")
            .name
            .as_ref()
            .map(ToString::to_string)
            .expect("the fixture names every target edge")
    };

    let forward = chosen(&FORWARD);
    assert_eq!(
        forward,
        chosen(&REVERSED),
        "reversing the order the target was built in changed which parallel \
         edge the source edge maps to"
    );
    assert_eq!(forward, chosen(&SHUFFLED));

    // Least by `Edge`'s own ordering, which is the only choice available that
    // does not read a position. Pinned by value so that a change of rule has to
    // be deliberate rather than incidental.
    assert_eq!(
        forward, "alpha",
        "the fallback should take the least parallel edge of the kind"
    );
}

/// And the same property through the public entry point, on the whole span.
#[test]
fn the_span_is_the_same_whatever_order_the_target_was_built_in() {
    let proto = protocol();
    let opts = SearchOptions::default();
    let src = source();

    let span_of = |order: &[&str]| find_span(&src, &target(order), &proto, &opts).expect("span");

    let forward = span_of(&FORWARD);
    let reversed = span_of(&REVERSED);
    let shuffled = span_of(&SHUFFLED);

    assert_eq!(
        forward.right.edge_map, reversed.right.edge_map,
        "the right leg's edge map moved when only the target's build order did"
    );
    assert_eq!(forward.right.edge_map, shuffled.right.edge_map);
    assert_eq!(forward.right.vertex_map, reversed.right.vertex_map);
    assert_eq!(forward.left.edge_map, reversed.left.edge_map);

    // The apex is canonical, so it agreed even while the leg did not. Asserting
    // it here records that it is not evidence either way.
    assert_eq!(
        forward.apex.vertices.len(),
        reversed.apex.vertices.len(),
        "the apex never varied, which is why this went unnoticed"
    );
    assert_eq!(forward.apex.edges, reversed.apex.edges);
    assert!((forward.quality - reversed.quality).abs() < 1e-12);
}
