//! What a caller can ask [`find_morphisms`] to materialise.
//!
//! The enumeration builds one [`FoundMorphism`] per optimum, and the count of
//! optima is a property of the pair rather than of its size: two schemas with no
//! edges and no shared name characters tie the entire hom-set at the optimum, so
//! `n` source vertices against `n` targets is `n^n` answers. At `n = 8` that is
//! 16,777,216 morphisms, 4.6 GB resident and about three minutes, from a pair
//! short enough to write out by hand.
//!
//! [`DEFAULT_OPTIMA_CAP`] therefore binds every request, not only the
//! unbounded-sounding `max_results = 0`. A cap applied to `0` alone would invert
//! the intent, leaving the one request that names no figure as the only bounded
//! one, so these tests pin both sides. And because the cap binds everywhere, a
//! caller has to be able to tell a list the cap cut from a list the pair
//! exhausted; [`MorphismList::truncated`] is that fact, asserted here rather
//! than left to the doc.
//!
//! `n = 6` is the size used below. It is `46_656` morphisms uncapped, which runs
//! in under a second and still exceeds the 1024-entry cap by a factor of forty
//! five, so the assertions distinguish the two behaviours without a test that
//! costs gigabytes to run.

#![allow(clippy::expect_used)]

use panproto_mig::hom_search::{SearchOptions, find_morphisms};
use panproto_mig::span::DEFAULT_OPTIMA_CAP;
use panproto_schema::{Protocol, Schema, SchemaBuilder};

/// One object kind and no edge rules, so every vertex is kind compatible with
/// every other and nothing constrains the assignment.
fn protocol() -> Protocol {
    Protocol {
        name: "test-cap".to_owned(),
        schema_theory: "ThTest".to_owned(),
        instance_theory: "ThWType".to_owned(),
        edge_rules: vec![],
        obj_kinds: vec!["object".to_owned()],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

/// `n` edgeless vertices whose names are built from one repeated letter.
///
/// The two schemas a test builds must share no name character, or the name
/// component of the objective would prefer some assignments to others and break
/// the tie the whole hom-set depends on.
fn edgeless(letters: &str, n: usize) -> Schema {
    let protocol = protocol();
    let mut builder = SchemaBuilder::new(&protocol);
    for index in 0..n {
        let letter = letters
            .chars()
            .nth(index)
            .expect("the letter pool covers the requested size");
        let name: String = std::iter::repeat_n(letter, 2).collect();
        builder = builder
            .vertex(&name, "object", None::<&str>)
            .expect("vertex");
    }
    builder.build().expect("build")
}

/// Every assignment ties, so the pair's optima are exactly `n^n`.
fn tied_pair(n: usize) -> (Schema, Schema) {
    (edgeless("abcdefgh", n), edgeless("mnopqrst", n))
}

/// An explicit request larger than the cap is answered with the cap.
///
/// Before the repair this returned `n^n`. The assertion is stated against
/// `DEFAULT_OPTIMA_CAP` rather than against `1024`, so it follows the constant
/// rather than pinning a number beside it.
#[test]
fn an_explicit_request_above_the_cap_is_answered_with_the_cap() {
    let (src, tgt) = tied_pair(6);
    let uncapped = SearchOptions {
        max_results: usize::MAX,
        ..SearchOptions::default()
    };

    let found = find_morphisms(&src, &tgt, &uncapped).expect("the network poses");

    assert_eq!(
        found.morphisms.len(),
        DEFAULT_OPTIMA_CAP,
        "a request for `usize::MAX` results must be bounded by the cap, not honoured verbatim"
    );
    assert!(
        found.truncated,
        "the pair has 6^6 = 46656 optima and 1024 were returned, so the list was cut"
    );
}

/// The request for everything reads the same cap, which is what it always did.
#[test]
fn asking_for_everything_is_the_same_bound_as_asking_for_too_much() {
    let (src, tgt) = tied_pair(6);
    let everything = SearchOptions {
        max_results: 0,
        ..SearchOptions::default()
    };
    let too_much = SearchOptions {
        max_results: 100_000,
        ..SearchOptions::default()
    };

    let all = find_morphisms(&src, &tgt, &everything).expect("the network poses");
    let over = find_morphisms(&src, &tgt, &too_much).expect("the network poses");

    assert_eq!(all.morphisms.len(), DEFAULT_OPTIMA_CAP);
    assert_eq!(over.morphisms.len(), DEFAULT_OPTIMA_CAP);
    assert!(all.truncated && over.truncated);
    let all_maps: Vec<_> = all.morphisms.iter().map(|m| &m.vertex_map).collect();
    let over_maps: Vec<_> = over.morphisms.iter().map(|m| &m.vertex_map).collect();
    assert_eq!(
        all_maps, over_maps,
        "the two requests are now the same request, so they must name the same morphisms"
    );
}

/// A request the cap does not bind is honoured, and reported as cut only when
/// the pair really has more.
#[test]
fn a_request_below_the_cap_is_honoured_and_says_whether_more_exist() {
    let (src, tgt) = tied_pair(6);
    let few = SearchOptions {
        max_results: 5,
        ..SearchOptions::default()
    };

    let found = find_morphisms(&src, &tgt, &few).expect("the network poses");

    assert_eq!(found.morphisms.len(), 5, "a figure under the cap stands");
    assert!(
        found.truncated,
        "five of 46656 optima were returned, and a caller cannot tell that from the list alone"
    );
}

/// A pair whose optimum really is unique reports an exhausted list, so the flag
/// is a fact about the search rather than a constant.
#[test]
fn an_exhausted_enumeration_is_not_reported_as_cut() {
    // Distinct names on both sides make one assignment strictly best, so the
    // whole optimum is a single morphism and the cap never binds.
    let src = edgeless("abcd", 3);
    let tgt = edgeless("abcd", 3);

    let found = find_morphisms(
        &src,
        &tgt,
        &SearchOptions {
            max_results: 0,
            ..SearchOptions::default()
        },
    )
    .expect("the network poses");

    assert_eq!(
        found.morphisms.len(),
        1,
        "identical schemas over distinct names admit one optimal morphism"
    );
    assert!(
        !found.truncated,
        "the walk reached the end of the optimum, so nothing was cut"
    );
}
