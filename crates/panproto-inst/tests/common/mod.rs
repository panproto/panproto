//! Growth measurement for the scaling tests.
//!
//! A scaling test runs a workload at a small and a large size and checks
//! how its time grows against the growth of its input. A raw ratio of two
//! times is exposed to the machine's load: test binaries running in
//! parallel preempt the workload, move it between fast and slow cores, and
//! contend for the caches and memory bandwidth, and a longer, larger run is
//! slowed more than a shorter, smaller one, so a linear workload can show a
//! superlinear ratio. A baseline of another operation over the same input
//! (a clone of it, say) does not cancel the load either, since the load
//! slows an allocation-bound clone and a hash-bound workload by different
//! factors.
//!
//! So the large run is measured against the small workload itself: each
//! round times one large run and, right after it, a window of repeated
//! small runs that lasts about as long. The two windows run the same code
//! over about the same number of elements, for the same length of time, at
//! the same moment, so the load slows both by about the same factor, and
//! their quotient, the large run in small runs, keeps its value under load.
//! The measure is the median of that quotient over the rounds, divided by
//! the factor by which the input grows. A workload linear in its input (up
//! to a logarithmic factor) then measures about 1, while a workload that
//! does `s` times more work per element at the large size measures about
//! `s`.

use std::time::Duration;

/// The growth of a workload from its small to its large size.
pub struct Growth {
    /// The median over the rounds of the large run's time in small runs.
    pub small_runs: f64,
    /// `small_runs` over the factor by which the input grows: about 1 for
    /// a workload linear in its input.
    pub growth: f64,
}

/// The growth of a workload whose input grows `ratio`-fold from `small` to
/// `large`, each of which runs the workload once and returns its time, as
/// the median over `rounds` rounds of the large run's time in small runs,
/// over `ratio`.
pub fn growth(
    rounds: usize,
    ratio: u32,
    mut small: impl FnMut() -> Duration,
    mut large: impl FnMut() -> Duration,
) -> Growth {
    assert!(rounds > 0, "a growth is measured over at least one round");
    // One warm-up run of each, then the repeat count that makes the window
    // of small runs about as long as one large run.
    let (one_small, one_large) = (small().max(Duration::from_micros(1)), large());
    let (s, l) = (one_small.as_nanos(), one_large.as_nanos());
    let repeats = u32::try_from((l + s / 2) / s)
        .unwrap_or(u32::MAX)
        .clamp(1, 1_000_000);
    let mut quotients: Vec<f64> = (0..rounds)
        .map(|_| {
            let large_run = large().as_secs_f64();
            let window: f64 = (0..repeats).map(|_| small().as_secs_f64()).sum();
            large_run * f64::from(repeats) / window
        })
        .collect();
    quotients.sort_by(f64::total_cmp);
    let small_runs = quotients[quotients.len() / 2];
    Growth {
        small_runs,
        growth: small_runs / f64::from(ratio),
    }
}
