//! Composing migrations must land the second one's resolver entries in the
//! first one's label space.
//!
//! `compose(m1, m2)` is a migration `G1 -> G3`, so every key it holds is read
//! against `G1` data. Adopting an entry from `m2` therefore means pulling both
//! halves of it back through `m1`: the label set that selects the entry, and
//! the label remapping's own keys, which name the children of a `G1` fan.
//! Leaving either in `G2` means the entry never matches anything the composite
//! is applied to, and the labels it renames are labels no source fan carries.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_mig::{Migration, compose};

/// `m1: G1 -> G2` renames the hyper-edge `fan` to `fan_mid` and its labels
/// `a`, `b` to `A`, `B`. It holds no resolver entries of its own, so
/// everything the composite carries comes from `m2`.
fn first() -> Migration {
    let mut m1 = Migration::empty();
    m1.hyper_edge_map
        .insert(Name::from("fan"), Name::from("fan_mid"));
    m1.label_map
        .insert((Name::from("fan"), Name::from("a")), Name::from("A"));
    m1.label_map
        .insert((Name::from("fan"), Name::from("b")), Name::from("B"));
    m1
}

/// `m2: G2 -> G3` resolves the mid-level fan carrying `A` and `B` onto
/// `fan_out`, renaming those children to `x` and `y`.
fn second() -> Migration {
    let mut m2 = Migration::empty();
    m2.hyper_resolver.insert(
        (
            Name::from("fan_mid"),
            vec![Name::from("A"), Name::from("B")],
        ),
        (
            Name::from("fan_out"),
            HashMap::from([
                (Name::from("A"), Name::from("x")),
                (Name::from("B"), Name::from("y")),
            ]),
        ),
    );
    m2
}

#[test]
fn the_selecting_label_set_is_pulled_back_through_the_first_migration() {
    let composed = compose(&first(), &second()).expect("the two migrations compose");
    let mut keys: Vec<(String, Vec<String>)> = composed
        .hyper_resolver
        .keys()
        .map(|(he, labels)| {
            (
                he.to_string(),
                labels.iter().map(ToString::to_string).collect(),
            )
        })
        .collect();
    keys.sort();

    assert_eq!(
        keys,
        vec![("fan".to_string(), vec!["a".to_string(), "b".to_string()])],
        "the composite selects on the source hyper-edge and its source labels"
    );
}

#[test]
fn the_label_remap_renames_source_labels() {
    let composed = compose(&first(), &second()).expect("the two migrations compose");
    let (target, remap) = composed
        .hyper_resolver
        .values()
        .next()
        .expect("the composite carries the adopted entry");
    assert_eq!(*target, Name::from("fan_out"));

    let mut pairs: Vec<(String, String)> = remap
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    pairs.sort();
    assert_eq!(
        pairs,
        vec![
            ("a".to_string(), "x".to_string()),
            ("b".to_string(), "y".to_string())
        ],
        "the remap renames the labels a source fan actually carries"
    );
}
