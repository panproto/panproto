//! Growth measurement for the scaling tests.
//!
//! A scaling test runs a workload at a small and a large size and checks
//! how its time grows. A raw ratio of times is exposed to the machine's
//! load: test binaries running in parallel preempt the workload and contend
//! for the caches and memory bandwidth, and a longer, larger run is slowed
//! more than a shorter, smaller one, so a linear workload can show a
//! superlinear ratio. So each workload time here is divided by the time of
//! a linear baseline over the same input (clones of it), repeated until it
//! lasts about as long as the workload and taken right after it: both
//! windows are then about equally exposed to the load. The workload's
//! size in baseline units is the median of that quotient over the rounds.
//! A workload linear in its input (up to a logarithmic factor) then keeps
//! its size in baseline units from the small size to the large one, while
//! a workload that does `s` times more work per element at the large size
//! shows a growth of about `s`.

use std::time::{Duration, Instant};

/// The time of one call of `f`.
pub fn time(f: impl FnOnce()) -> Duration {
    let start = Instant::now();
    f();
    start.elapsed()
}

/// A workload at one size and its linear baseline: one baseline unit.
pub struct Probe<W, U> {
    /// The workload; returns its own time.
    pub work: W,
    /// One unit of the baseline over the same input.
    pub unit: U,
}

/// A probe calibrated to repeat its baseline unit for about as long as its
/// workload runs, with the quotients measured so far.
struct Calibrated<W, U> {
    probe: Probe<W, U>,
    repeats: u32,
    quotients: Vec<f64>,
}

impl<W: FnMut() -> Duration, U: FnMut()> Calibrated<W, U> {
    fn new(mut probe: Probe<W, U>) -> Self {
        // One warm-up run of each, then the repeat count that makes the
        // baseline window about as long as the workload's.
        let work = (probe.work)();
        let unit = (0..3)
            .map(|_| time(&mut probe.unit))
            .min()
            .unwrap_or(Duration::ZERO)
            .max(Duration::from_micros(1));
        let repeats = (work.as_secs_f64() / unit.as_secs_f64()).round();
        let repeats = if repeats < 1.0 { 1 } else { repeats.min(1e6) as u32 };
        Self {
            probe,
            repeats,
            quotients: Vec::new(),
        }
    }

    fn round(&mut self) {
        let work = (self.probe.work)();
        let unit = &mut self.probe.unit;
        let baseline = time(|| {
            for _ in 0..self.repeats {
                unit();
            }
        });
        self.quotients
            .push(work.as_secs_f64() * f64::from(self.repeats) / baseline.as_secs_f64());
    }

    fn median(mut self) -> f64 {
        self.quotients.sort_by(f64::total_cmp);
        self.quotients[self.quotients.len() / 2]
    }
}

/// The sizes of the small and the large workload in baseline units, each the
/// median over `rounds` rounds; a round measures the small probe and then the
/// large one.
pub fn sizes_in_units<W1, U1, W2, U2>(
    rounds: usize,
    small: Probe<W1, U1>,
    large: Probe<W2, U2>,
) -> (f64, f64)
where
    W1: FnMut() -> Duration,
    U1: FnMut(),
    W2: FnMut() -> Duration,
    U2: FnMut(),
{
    let (mut small, mut large) = (Calibrated::new(small), Calibrated::new(large));
    for _ in 0..rounds {
        small.round();
        large.round();
    }
    (small.median(), large.median())
}
