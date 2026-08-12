//! Every ordered pair of the measured lexicon corpus, searched.
//!
//! This is the load-bearing claim about the span search stated as a test
//! rather than as a benchmark. `cargo bench` never runs in CI, so a claim that
//! lives only in `benches/span_bench.rs` is a claim nothing checks. The two
//! properties asserted here are the two the rewrite was for:
//!
//! 1. **Every pair is answered optimally.** [`SpanCertificate::proven_optimal`]
//!    is not "the search did its best"; it is a proof obligation the exact
//!    paths discharge and the fallback paths do not. A `false` anywhere in the
//!    corpus means the dispatcher left exact inference on a real schema pair,
//!    which is the case the width measurement exists to prevent.
//! 2. **No pair is slow.** The previous search took 24.1 seconds on
//!    `feed.post → verifyCoercionLaws` and never answered
//!    `verifyCoercionLaws → vcs.schemaTree` at all. A per-pair ceiling is what
//!    turns "it is fast now" into something that fails when it stops being
//!    true.
//!
//! Seventy-seven lexicons give 5852 ordered pairs. Both directions are searched
//! because the span is not symmetric: the apex is induced on the *source*, so
//! `(a, b)` and `(b, a)` are different networks with different answers.
//!
//! # The timing assertion is release-only
//!
//! A debug build of the solver runs an order of magnitude slower than a release
//! one, and a ceiling that held in both would have to be so loose it asserted
//! nothing. The correctness assertions run in both; the ceiling is skipped under
//! `debug_assertions`, following
//! `crates/panproto-core/tests/stringency_monotonicity.rs`. Run
//! `cargo nextest run --release -p panproto-mig -E 'test(lexicon_sweep)'` to
//! exercise it.
//!
//! Set `PP_DUMP_SWEEP=1` to print the slowest twenty pairs.

#![allow(
    clippy::expect_used,
    reason = "a malformed committed fixture should fail the test loudly"
)]

use std::time::{Duration, Instant};

use panproto_mig::SpanSearch;

#[path = "support/lexicons.rs"]
mod lexicons;

/// What one pair may cost, in a release build.
///
/// Fifty milliseconds is two orders of magnitude below the 13.2 second worst
/// case the previous search was measured at on this corpus, and roughly two
/// orders of magnitude *above* the measured median, so it reports a regression
/// rather than machine noise.
const PER_PAIR_CEILING: Duration = Duration::from_millis(50);

/// One measured pair.
struct Measured {
    src: String,
    tgt: String,
    elapsed: Duration,
    apex_vertices: usize,
    proven_optimal: bool,
}

/// The value at the given percentile of an ascending slice.
fn percentile(sorted: &[Duration], p: f64) -> Duration {
    assert!(!sorted.is_empty(), "no pairs were measured");
    let last = sorted.len() - 1;
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "an index into a slice of at most 5852 elements is exact in f64 and the product \
                  of two values in range is in range"
    )]
    let index = (p * last as f64).round() as usize;
    sorted[index]
}

#[test]
fn every_ordered_lexicon_pair_is_answered_optimally_and_quickly() {
    let protocol = panproto_protocols::atproto::protocol();
    let corpus = lexicons::corpus();
    let search = SpanSearch::new(&protocol);

    let mut measured: Vec<Measured> = Vec::with_capacity(corpus.len() * (corpus.len() - 1));
    for (i, src) in corpus.iter().enumerate() {
        for (j, tgt) in corpus.iter().enumerate() {
            if i == j {
                continue;
            }
            let started = Instant::now();
            let span = search.run(&src.schema, &tgt.schema).unwrap_or_else(|e| {
                panic!(
                    "{} -> {}: the span search refused, and it is documented never to refuse for \
                     want of a match: {e}",
                    src.nsid, tgt.nsid
                )
            });
            measured.push(Measured {
                src: src.nsid.clone(),
                tgt: tgt.nsid.clone(),
                elapsed: started.elapsed(),
                apex_vertices: span.apex.vertices.len(),
                proven_optimal: span.certificate.proven_optimal,
            });
        }
    }

    assert_eq!(
        measured.len(),
        5852,
        "the corpus no longer gives 5852 ordered pairs, so the counts this file states are stale"
    );

    let not_proven: Vec<&Measured> = measured.iter().filter(|m| !m.proven_optimal).collect();
    assert!(
        not_proven.is_empty(),
        "{} of {} pairs were not proven optimal, so the dispatcher left exact inference on a real \
         schema pair. First five: {:?}",
        not_proven.len(),
        measured.len(),
        not_proven
            .iter()
            .take(5)
            .map(|m| (m.src.as_str(), m.tgt.as_str()))
            .collect::<Vec<_>>(),
    );

    let mut times: Vec<Duration> = measured.iter().map(|m| m.elapsed).collect();
    times.sort_unstable();
    let p50 = percentile(&times, 0.50);
    let p95 = percentile(&times, 0.95);
    let max = *times.last().expect("5852 pairs were measured");
    let total: Duration = times.iter().sum();

    if std::env::var("PP_DUMP_SWEEP").is_ok() {
        let mut slowest: Vec<&Measured> = measured.iter().collect();
        slowest.sort_by_key(|m| std::cmp::Reverse(m.elapsed));
        for m in slowest.iter().take(20) {
            eprintln!(
                "{:?}  {} -> {}  apex={}",
                m.elapsed, m.src, m.tgt, m.apex_vertices
            );
        }
    }
    eprintln!(
        "lexicon sweep: {} pairs, total {total:?}, p50 {p50:?}, p95 {p95:?}, max {max:?}",
        measured.len()
    );

    if cfg!(debug_assertions) {
        return;
    }

    let over: Vec<&Measured> = measured
        .iter()
        .filter(|m| m.elapsed > PER_PAIR_CEILING)
        .collect();
    assert!(
        over.is_empty(),
        "{} of {} pairs took longer than {PER_PAIR_CEILING:?}. Slowest: {:?}",
        over.len(),
        measured.len(),
        over.iter()
            .max_by_key(|m| m.elapsed)
            .map(|m| (m.src.as_str(), m.tgt.as_str(), m.elapsed)),
    );
}
