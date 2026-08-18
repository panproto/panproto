//! The superset premise `beats_on_objective` rests on, stated as a test.
//!
//! `panproto_lens::auto_lens::best_of_pinned_and_released` runs the search
//! twice, once with the strategy anchors pinned and once with them released,
//! and keeps whichever is better on the objective. Its doc justifies the
//! comparison with an argument rather than a test:
//!
//! > Releasing pins only ever *adds* values back to domains, so the released
//! > search optimises over a superset of the pinned search's feasible set and
//! > its optimum is therefore never worse on the objective.
//!
//! Two things have to hold for that to be true, and neither is checked
//! anywhere:
//!
//! 1. **The objective is the same function in both runs.** A pin restricts a
//!    domain; if it also moved a denominator (`|C_src|`, the coverage radix,
//!    the per-vertex or per-edge scale) or entered the cost tables through the
//!    evidence term, the two searches would be minimising different things and
//!    the two numbers would not be comparable at all.
//! 2. **The feasible set really does only grow.** Releasing must not remove a
//!    value from any domain.
//!
//! What is asserted below is the consequence of both: over every ordered pair
//! of the measured schema corpus, under randomly drawn kind-compatible pin
//! sets, the released span is never worse than the pinned one on
//! `(quality, mapped vertices)` read lexicographically, and the released total
//! morphism is never worse in quality than the pinned one. A violation is
//! exactly the case `beats_on_objective` would keep the wrong answer on.
//!
//! The pins are drawn rather than taken from the alignment strategies on
//! purpose. A strategy anchor is a plausible pin, so a corpus of them samples
//! the easy part of the space; the premise is stated for *every* pin set, and
//! an implausible pin is the one that collapses a domain hardest.

#![allow(
    clippy::expect_used,
    reason = "a malformed committed fixture should fail the test loudly"
)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_mig::hom_search::{SearchOptions, find_best_morphism, find_span};
use panproto_schema::Schema;

#[path = "support/lexicons.rs"]
mod lexicons;

/// xorshift64\*, so a failure reproduces from the seed alone.
struct Rng(u64);

impl Rng {
    const fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            usize::try_from(self.next() % n as u64).unwrap_or(0)
        }
    }
}

fn sorted_names(schema: &Schema) -> Vec<Name> {
    let mut names: Vec<Name> = schema.vertices.keys().cloned().collect();
    names.sort_unstable();
    names
}

/// A kind-compatible pin set, which is what a strategy anchor always is.
///
/// An incompatible pin leaves its vertex with `⊥` alone, which is a domain
/// collapse the premise also has to survive but a weaker one: the objective
/// cannot see it at all.
fn draw_pins(src: &Schema, tgt: &Schema, rng: &mut Rng, count: usize) -> HashMap<Name, Name> {
    let sources = sorted_names(src);
    let targets = sorted_names(tgt);
    let mut pins = HashMap::new();
    for _ in 0..count {
        if sources.is_empty() {
            break;
        }
        let index = rng.below(sources.len());
        let source = sources[index].clone();
        let kind = src.vertices[&source].kind.clone();
        let compatible: Vec<&Name> = targets
            .iter()
            .filter(|name| tgt.vertices[*name].kind == kind)
            .collect();
        if compatible.is_empty() {
            continue;
        }
        let pick = rng.below(compatible.len());
        pins.insert(source, compatible[pick].clone());
    }
    pins
}

/// `(quality, mapped)` is the objective, read so that larger is better.
///
/// `total_cmp` rather than `<`, for the same reason `beats_on_objective` uses
/// it: the tie-break has to stay transitive whatever arrives.
fn worse_on_objective(free: (f64, usize), pinned: (f64, usize)) -> bool {
    match free.0.total_cmp(&pinned.0) {
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Greater => false,
        std::cmp::Ordering::Equal => free.1 < pinned.1,
    }
}

/// Over the whole corpus, releasing a pin set never costs objective value.
#[test]
fn releasing_pins_never_lowers_the_span_objective() {
    let corpus = lexicons::corpus();
    let protocol = panproto_protocols::atproto::protocol();
    let mut rng = Rng(0x243F_6A88_85A3_08D3);
    let mut pairs = 0usize;
    let mut violations: Vec<String> = Vec::new();

    for (i, left) in corpus.iter().enumerate() {
        for (j, right) in corpus.iter().enumerate() {
            if i == j {
                continue;
            }
            let count = 1 + rng.below(4);
            let pins = draw_pins(&left.schema, &right.schema, &mut rng, count);
            if pins.is_empty() {
                continue;
            }

            let free = find_span(
                &left.schema,
                &right.schema,
                &protocol,
                &SearchOptions::default(),
            )
            .expect("the released network poses");
            let pinned = find_span(
                &left.schema,
                &right.schema,
                &protocol,
                &SearchOptions {
                    hard_pins: pins.clone(),
                    ..SearchOptions::default()
                },
            )
            .expect("the pinned network poses");
            pairs += 1;

            let free_key = (free.quality, free.apex.vertices.len());
            let pin_key = (pinned.quality, pinned.apex.vertices.len());
            if worse_on_objective(free_key, pin_key) {
                violations.push(format!(
                    "{} -> {}: released {free_key:?} (proven {}), pinned {pin_key:?} (proven \
                     {}), pins {pins:?}",
                    left.nsid,
                    right.nsid,
                    free.certificate.proven_optimal,
                    pinned.certificate.proven_optimal,
                ));
            }
        }
    }

    println!("released_search_is_never_worse: {pairs} pinned/released span pairs compared");
    assert!(pairs > 5_000, "the sweep covered only {pairs} pairs");
    assert!(
        violations.is_empty(),
        "the released span was worse on the objective on {} of {pairs} pairs, so \
         `beats_on_objective` can keep the pinned answer over a better one:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// The same premise on the total-morphism path, where `without_bottom` rebuilds
/// the network before the search runs.
///
/// The rebuild is the step that could break the premise without touching a
/// domain: it copies every unary table and rewrites the `⊥` slot, and a copy
/// that silently fell back to the original network would leave the released run
/// minimising over a different feasible set than the pinned one.
#[test]
fn releasing_pins_never_lowers_the_total_morphism_quality() {
    let corpus = lexicons::corpus();
    let mut rng = Rng(0x0BAD_C0DE_D15E_A5E1);
    let mut compared = 0usize;
    let mut violations: Vec<String> = Vec::new();

    for (i, left) in corpus.iter().enumerate() {
        for (j, right) in corpus.iter().enumerate() {
            if i == j {
                continue;
            }
            let count = 1 + rng.below(3);
            let pins = draw_pins(&left.schema, &right.schema, &mut rng, count);
            if pins.is_empty() {
                continue;
            }
            let free = find_best_morphism(&left.schema, &right.schema, &SearchOptions::default())
                .expect("the released network poses");
            let pinned = find_best_morphism(
                &left.schema,
                &right.schema,
                &SearchOptions {
                    hard_pins: pins.clone(),
                    ..SearchOptions::default()
                },
            )
            .expect("the pinned network poses");

            let Some(pinned) = pinned else {
                continue;
            };
            compared += 1;
            match free {
                None => violations.push(format!(
                    "{} -> {}: pinned found a total morphism and released found none, so the \
                     released feasible set is not a superset; pins {pins:?}",
                    left.nsid, right.nsid
                )),
                Some(free) if free.quality < pinned.quality => violations.push(format!(
                    "{} -> {}: released quality {} below pinned {}; pins {pins:?}",
                    left.nsid, right.nsid, free.quality, pinned.quality
                )),
                Some(_) => {}
            }
        }
    }

    println!(
        "released_search_is_never_worse: {compared} pairs admit a pinned total morphism, of \
         {} ordered pairs",
        corpus.len() * (corpus.len() - 1)
    );
    assert!(
        violations.is_empty(),
        "the released total search was worse on {} of {compared} pairs that admit a pinned total \
         morphism:\n{}",
        violations.len(),
        violations.join("\n")
    );
}
