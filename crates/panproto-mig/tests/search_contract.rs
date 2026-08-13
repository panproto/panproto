//! What the total-morphism entry points promise, and what they must never say
//! instead.
//!
//! Three contracts live here, each of which was broken by a returned value
//! rather than by a crash, so each is pinned against the observable answer:
//!
//! 1. **A search that could not run is not a search that found nothing.** The
//!    domain ceiling is reachable on ordinary input, and reporting it as "no
//!    total morphism exists" is a wrong answer about a pair whose identity
//!    morphism is perfect.
//! 2. **`epic` is a constraint the search enforces, not a filter over its
//!    answer.** A surjective total morphism need not be an argmin of an
//!    objective that says nothing about surjectivity, so filtering the argmins
//!    reports "none exists" whenever the optimum happens not to be onto.
//! 3. **`epic` is not a span property.** A span's right leg is deliberately
//!    partial, so the span search rejects the flag rather than dropping it.

#![allow(clippy::expect_used)]

use panproto_mig::hom_search::{SearchOptions, find_best_morphism, find_morphisms, find_span};
use panproto_mig::{BuildError, CfnError, SpanError, ValId};
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

fn protocol() -> Protocol {
    Protocol {
        name: "contract".to_owned(),
        schema_theory: "ThTest".to_owned(),
        instance_theory: "ThWType".to_owned(),
        edge_rules: vec![EdgeRule {
            edge_kind: "prop".to_owned(),
            src_kinds: vec!["object".to_owned()],
            tgt_kinds: vec!["string".to_owned()],
        }],
        obj_kinds: vec!["object".to_owned(), "string".to_owned()],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

/// One object vertex with `fields` string properties, so every string vertex
/// sees all `fields` string targets and they share one domain.
fn record(fields: usize) -> Schema {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto)
        .vertex("body", "object", None::<&str>)
        .expect("body");
    for index in 0..fields {
        let id = format!("f{index:03}");
        builder = builder
            .vertex(&id, "string", None::<&str>)
            .expect("field vertex")
            .edge("body", &id, "prop", Some(id.as_str()))
            .expect("field edge");
    }
    builder.entry("body").build().expect("build")
}

fn schema(ids: &[&str]) -> Schema {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    for id in ids {
        builder = builder.vertex(id, "object", None::<&str>).expect("vertex");
    }
    builder.build().expect("build")
}

// ---------------------------------------------------------------------------
// 1. A refused network is reported, never spelled "nothing found"
// ---------------------------------------------------------------------------

/// At the ceiling the identity is found; one past it the search reports why it
/// could not run.
///
/// The two halves have to be one test. The first pins that the ceiling really
/// is where `ValId::MAX_REAL_VALUES` says it is, so the second is testing the
/// refusal rather than some unrelated failure, and together they say the only
/// thing that changes across the boundary is `Ok` against `Err` — never `Ok`
/// against `Ok(empty)`.
#[test]
fn a_domain_ceiling_is_reported_rather_than_spelled_no_morphism() {
    let at = ValId::MAX_REAL_VALUES as usize;
    let fits = record(at);
    let best = find_best_morphism(&fits, &fits, &SearchOptions::default())
        .expect("a domain of exactly the ceiling poses")
        .expect("a schema maps onto itself");
    assert!(
        (best.quality - 1.0).abs() < 1e-9,
        "the identity of a schema against itself is a perfect match: got {}",
        best.quality
    );

    let over = record(at + 1);
    let refused = find_morphisms(&over, &over, &SearchOptions::default());
    assert!(
        matches!(
            refused,
            Err(SpanError::Build {
                source: BuildError::Network {
                    source: CfnError::DomainTooLarge { .. }
                }
            })
        ),
        "one target past the ceiling must report the ceiling, not an empty \
         hom-set: got {refused:?}"
    );

    let refused_best = find_best_morphism(&over, &over, &SearchOptions::default());
    assert!(
        refused_best.is_err(),
        "`find_best_morphism` must not launder the same refusal into `None`, \
         which its own contract defines as `no total morphism exists`"
    );

    // The identity of a schema against itself is total, perfect, and exists
    // whatever the network can represent, so `Ok(None)` here would be false.
    assert!(
        !matches!(refused_best, Ok(None)),
        "a pair whose identity morphism is perfect was told no morphism exists"
    );
}

// ---------------------------------------------------------------------------
// 2. `epic` is searched for, not filtered out
// ---------------------------------------------------------------------------

/// The unconstrained optimum is not onto, and a surjective total morphism
/// exists. A filter over the argmins returns nothing here; a search over the
/// surjective assignments returns the surjection.
#[test]
fn epic_finds_a_surjection_the_unconstrained_optimum_is_not() {
    let src = schema(&["alpha", "beta"]);
    let tgt = schema(&["alpha", "zzzzzz"]);

    let plain = find_morphisms(&src, &tgt, &SearchOptions::default()).expect("the network poses");
    assert_eq!(plain.len(), 1, "the unconstrained optimum is unique");
    let images: std::collections::BTreeSet<&str> = plain[0]
        .vertex_map
        .values()
        .map(panproto_gat::Name::as_str)
        .collect();
    assert_eq!(
        images.len(),
        1,
        "the premise of this test is that the optimum collapses both sources \
         onto one target"
    );

    let opts = SearchOptions {
        epic: true,
        ..SearchOptions::default()
    };
    let onto = find_morphisms(&src, &tgt, &opts).expect("the network poses");
    assert_eq!(
        onto.len(),
        1,
        "alpha->alpha, beta->zzzzzz is a surjective total morphism and must be \
         found even though it is not the unconstrained optimum"
    );
    let covered: std::collections::BTreeSet<&str> = onto[0]
        .vertex_map
        .values()
        .map(panproto_gat::Name::as_str)
        .collect();
    assert_eq!(covered.len(), tgt.vertices.len(), "the answer is onto");
}

/// Capping the result count must not change whether a surjective morphism is
/// found at all, which is what `find_best_morphism` does by forcing
/// `max_results = 1`.
#[test]
fn epic_agrees_between_find_best_and_find_morphisms() {
    let proto = protocol();
    let mk = |left: &str, right: &str| {
        SchemaBuilder::new(&proto)
            .vertex("root", "object", None::<&str>)
            .expect("root")
            .vertex(&format!("root.{left}"), "string", None::<&str>)
            .expect("left")
            .vertex(&format!("root.{right}"), "string", None::<&str>)
            .expect("right")
            .edge("root", &format!("root.{left}"), "prop", Some(left))
            .expect("left edge")
            .edge("root", &format!("root.{right}"), "prop", Some(right))
            .expect("right edge")
            .entry("root")
            .build()
            .expect("build")
    };
    let src = mk("a", "b");
    let tgt = mk("a", "z");

    let opts = SearchOptions {
        epic: true,
        ..SearchOptions::default()
    };
    let all = find_morphisms(&src, &tgt, &opts).expect("the network poses");
    let best = find_best_morphism(&src, &tgt, &opts).expect("the network poses");
    assert_eq!(
        all.is_empty(),
        best.is_none(),
        "the two entry points must agree on whether a surjective total \
         morphism exists: find_morphisms returned {} results, find_best \
         returned {:?}",
        all.len(),
        best.as_ref().map(|m| m.quality)
    );
    assert!(
        best.is_some(),
        "root.a->root.a, root.b->root.z is onto and total"
    );

    let capped = find_morphisms(
        &src,
        &tgt,
        &SearchOptions {
            epic: true,
            max_results: 1,
            ..SearchOptions::default()
        },
    )
    .expect("the network poses");
    assert_eq!(
        capped.len(),
        1,
        "a result cap bounds how many answers are returned, never whether one \
         is found"
    );
}

/// The cardinality test is exact, so it stands in for the search rather than
/// merely shortcutting it.
#[test]
fn epic_is_empty_when_the_source_is_smaller_than_the_target() {
    let src = schema(&["a"]);
    let tgt = schema(&["a", "b"]);
    let opts = SearchOptions {
        epic: true,
        ..SearchOptions::default()
    };
    assert!(
        find_morphisms(&src, &tgt, &opts)
            .expect("the network poses")
            .is_empty(),
        "one source vertex cannot cover two targets"
    );
}

// ---------------------------------------------------------------------------
// 3. `epic` is not a span property
// ---------------------------------------------------------------------------

/// A span cannot promise surjectivity, so asking for it is refused rather than
/// silently answered with a possibly non-surjective span.
#[test]
fn a_span_search_rejects_epic_rather_than_ignoring_it() {
    let src = schema(&["root"]);
    let tgt = schema(&["root", "extra"]);
    let opts = SearchOptions {
        epic: true,
        ..SearchOptions::default()
    };

    let refused = find_span(&src, &tgt, &protocol(), &opts);
    assert!(
        matches!(refused, Err(SpanError::EpicIsNotASpanProperty)),
        "a dropped option answers a different question than the one asked: \
         got {refused:?}"
    );

    // Without the flag the same pair still spans, so the refusal is about the
    // request rather than about the pair.
    assert!(find_span(&src, &tgt, &protocol(), &SearchOptions::default()).is_ok());
}
