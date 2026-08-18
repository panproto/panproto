//! What the total-morphism entry points promise, and what they must never say
//! instead.
//!
//! Three contracts live here, each of which was broken by a returned value
//! rather than by a crash, so each is pinned against the observable answer:
//!
//! 1. **A search that could not run is not a search that found nothing.** No
//!    domain size is refused any more, so what is left to refuse is a measured
//!    memory cost, and reporting that as "no total morphism exists" would be a
//!    wrong answer about a pair whose identity morphism is perfect.
//! 2. **`epic` is a constraint the search enforces, not a filter over its
//!    answer.** A surjective total morphism need not be an argmin of an
//!    objective that says nothing about surjectivity, so filtering the argmins
//!    reports "none exists" whenever the optimum happens not to be onto.
//! 3. **`epic` is not a span property.** A span's right leg is deliberately
//!    partial, so the span search rejects the flag rather than dropping it.

#![allow(clippy::expect_used)]

use panproto_mig::hom_search::{SearchOptions, find_best_morphism, find_morphisms, find_span};
use panproto_mig::solve::SearchBudget;
use panproto_mig::{BuildError, CfnError, SpanError, SpanSearch};
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

/// A record with two hundred fields of one type is an ordinary schema, and the
/// search finds its identity.
///
/// Every one of the two hundred string vertices sees all two hundred string
/// targets, so this is the shape that used to be refused: a single-word domain
/// held sixty-three real values, and the sixty-fourth field made the network
/// unbuildable. Nothing about the width shows through now.
#[test]
fn a_record_of_two_hundred_same_kind_fields_finds_its_identity() {
    let wide = record(200);
    let best = find_best_morphism(&wide, &wide, &SearchOptions::default())
        .expect("a two hundred value domain poses")
        .expect("a schema maps onto itself");
    assert!(
        (best.quality - 1.0).abs() < 1e-9,
        "the identity of a schema against itself is a perfect match: got {}",
        best.quality
    );
    assert_eq!(
        best.vertex_map.len(),
        201,
        "the identity maps the body and every field"
    );
    for index in 0..200usize {
        let id = format!("f{index:03}");
        assert_eq!(
            best.vertex_map.get(&panproto_gat::Name::from(id.as_str())),
            Some(&panproto_gat::Name::from(id.as_str())),
            "field {id} is not mapped to itself"
        );
    }
}

/// A pair too large to hold is refused by the memory budget, and the refusal
/// names the bytes.
///
/// This is what replaced the domain ceiling. The ceiling was a word size and
/// said nothing about the machine; this is a measurement of the cost tables the
/// network would allocate, checked against the same figure the dispatcher
/// budgets exact inference with. Two things are pinned: the number in the error
/// is a measured cost, and the refusal is not laundered into "no morphism
/// exists" about a pair whose identity morphism is perfect.
#[test]
fn a_network_over_the_memory_budget_reports_the_budget() {
    // Unary tables alone come to `fields · (fields + 1) + 2` entries, which at
    // eight bytes each passes the default sixty-four megabyte budget. That is
    // roughly forty-six times the widest domain the old ceiling allowed, and it
    // is the first thing that refuses anything now.
    let over = record(2_900);
    let refused = find_best_morphism(&over, &over, &SearchOptions::default());
    let Err(SpanError::Build {
        source:
            BuildError::Network {
                source:
                    CfnError::OverMemoryBudget {
                        entries,
                        bytes,
                        budget,
                    },
            },
    }) = refused
    else {
        panic!(
            "a network past the budget must report the budget, and \
             `find_best_morphism` must not launder it into `Ok(None)`, which \
             its own contract defines as `no total morphism exists`: {refused:?}"
        );
    };
    assert_eq!(entries, 2_900 * 2_901 + 2);
    assert_eq!(bytes, entries * 8);
    assert!(
        bytes > budget,
        "the refusal must report a cost above the budget it was checked against"
    );
}

/// The budget the refusal is measured against is the caller's, not a constant.
///
/// A word size could not be moved. This can: the same pair poses or is refused
/// according to what the caller says the machine has, and both answers name the
/// same measurement.
#[test]
fn the_memory_budget_is_the_callers() {
    let pair = record(200);
    let entries = 200 * 201 + 2;

    let refused = SpanSearch::new(&protocol())
        .with_budget(SearchBudget::default().with_mem_bytes(1024))
        .run(&pair, &pair);
    assert!(
        matches!(
            refused,
            Err(SpanError::Build {
                source: BuildError::Network {
                    source: CfnError::OverMemoryBudget {
                        entries: measured,
                        budget: 1024,
                        ..
                    }
                }
            }) if measured == entries
        ),
        "{refused:?}"
    );

    let span = SpanSearch::new(&protocol())
        .with_budget(SearchBudget::default())
        .run(&pair, &pair)
        .expect("the same pair poses against the default budget");
    assert_eq!(span.apex.vertices.len(), 201);
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

    let plain = find_morphisms(&src, &tgt, &SearchOptions::default())
        .expect("the network poses")
        .morphisms;
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
    let onto = find_morphisms(&src, &tgt, &opts)
        .expect("the network poses")
        .morphisms;
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
    let all = find_morphisms(&src, &tgt, &opts)
        .expect("the network poses")
        .morphisms;
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
    .expect("the network poses")
    .morphisms;
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
            .morphisms
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
