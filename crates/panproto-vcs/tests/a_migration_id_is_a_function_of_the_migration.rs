//! A migration's object ID must name that migration and nothing else.
//!
//! `hash_migration` builds a canonical form in which every `HashMap` becomes a
//! `BTreeMap`, which is what makes the digest a function of the migration
//! rather than of the process's hash seed. `Migration::hyper_resolver` is keyed
//! by `(hyper_edge_id, labels)`, so canonicalising it under the hyper-edge ID
//! alone breaks the property twice over: entries that share an ID collapse onto
//! whichever one the bucket order happens to deliver last, and two migrations
//! that differ only in their label sets canonicalise to the same bytes.
//!
//! This drives both halves. The first half hashes a two-entry migration in many
//! separate processes and demands one answer; the second constructs a pair of
//! migrations that a label-erasing canonical form cannot tell apart.
//!
//! Run the child directly with `PP_MIGRATION_ID_DUMP=1` to see one process's
//! answer.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::{BTreeMap, HashMap};
use std::process::Command;

use panproto_gat::Name;
use panproto_mig::Migration;
use panproto_vcs::hash::{ObjectId, hash_migration};

/// How many separate processes the answer is compared across.
const PROCESSES: usize = 64;

const fn endpoints() -> (ObjectId, ObjectId) {
    (
        ObjectId::from_bytes([1u8; 32]),
        ObjectId::from_bytes([2u8; 32]),
    )
}

/// A migration whose sole content is two resolver entries on one hyper-edge,
/// distinguished only by the label set each governs.
fn two_entry_migration() -> Migration {
    let mut mig = Migration::empty();
    mig.hyper_resolver.insert(
        (
            Name::from("fan"),
            vec![Name::from("left"), Name::from("right")],
        ),
        (
            Name::from("fan_pair"),
            HashMap::from([
                (Name::from("left"), Name::from("a")),
                (Name::from("right"), Name::from("b")),
            ]),
        ),
    );
    mig.hyper_resolver.insert(
        (Name::from("fan"), vec![Name::from("left")]),
        (
            Name::from("fan_left_only"),
            HashMap::from([(Name::from("left"), Name::from("only"))]),
        ),
    );
    mig
}

/// A migration with one resolver entry on `fan`, governing the given labels.
fn one_entry_migration(labels: &[&str]) -> Migration {
    let mut mig = Migration::empty();
    mig.hyper_resolver.insert(
        (
            Name::from("fan"),
            labels.iter().map(|l| Name::from(*l)).collect(),
        ),
        (Name::from("target"), HashMap::new()),
    );
    mig
}

/// The child half: one process's digest for the fixture, printed.
#[test]
fn one_process_answer() {
    if std::env::var_os("PP_MIGRATION_ID_DUMP").is_none() {
        // Nothing to do: the parent below is what runs this with the variable
        // set. Left as a normal test so it type-checks in every run.
        return;
    }
    let (src, tgt) = endpoints();
    let id = hash_migration(src, tgt, &two_entry_migration()).expect("the fixture hashes");
    print!("<<<{id}>>>");
}

/// Extract what the child printed between the markers.
fn child_answer(exe: &std::path::Path) -> String {
    let output = Command::new(exe)
        .args(["one_process_answer", "--exact", "--nocapture"])
        .env("PP_MIGRATION_ID_DUMP", "1")
        .output()
        .expect("the test binary re-runs");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let start = text.find("<<<").expect("the child printed its answer");
    let end = text.find(">>>").expect("the child closed its answer");
    text[start + 3..end].to_owned()
}

#[test]
fn the_migration_id_does_not_depend_on_the_hash_seed() {
    if std::env::var_os("PP_MIGRATION_ID_DUMP").is_some() {
        return;
    }
    let exe = std::env::current_exe().expect("the test binary knows its own path");
    let mut distinct: BTreeMap<String, usize> = BTreeMap::new();
    for _ in 0..PROCESSES {
        *distinct.entry(child_answer(&exe)).or_insert(0) += 1;
    }

    assert_eq!(
        distinct.len(),
        1,
        "{PROCESSES} processes gave {} different object IDs for one migration:\n{}",
        distinct.len(),
        distinct
            .iter()
            .map(|(answer, count)| format!("  {answer} (seen {count} times)"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn migrations_that_differ_only_in_their_label_sets_get_different_ids() {
    let (src, tgt) = endpoints();
    let left = hash_migration(src, tgt, &one_entry_migration(&["left"])).expect("left hashes");
    let right = hash_migration(src, tgt, &one_entry_migration(&["right"])).expect("right hashes");
    assert_ne!(
        left, right,
        "the label set a resolver entry governs is part of the migration"
    );
}

#[test]
fn a_label_set_permutation_is_the_same_migration() {
    // The key's label component denotes a set, so listing it in another order
    // names the same migration and must not move the digest.
    let (src, tgt) = endpoints();
    let forward =
        hash_migration(src, tgt, &one_entry_migration(&["left", "right"])).expect("forward hashes");
    let reversed =
        hash_migration(src, tgt, &one_entry_migration(&["right", "left"])).expect("reverse hashes");
    assert_eq!(forward, reversed);
}
