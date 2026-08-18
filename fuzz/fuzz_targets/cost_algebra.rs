#![no_main]
//! Fuzz target for the migration cost algebra.
//!
//! [`Cost`] is a valuation structure: a saturating `⊕` with an explicit
//! top element, a guarded `⊖` that never wraps and never clamps, and a
//! packed lexicographic encoding of the `(quality, drops)` pair. Those
//! are exactly the operations where a fuzzer beats a property test,
//! because the interesting inputs cluster near `u64::MAX / 2` and near
//! `⊤` rather than anywhere a uniform generator would land, and because
//! [`CostWeights::new`] has to be total over raw `f64` bit patterns —
//! subnormals and non-number payloads included — which a rational-grid
//! generator can never reach.
//!
//! Every assertion below is a law the solver's soundness argument rests
//! on, stated over the real types rather than over reference arithmetic.
//!
//! Run with:
//!
//! ```text
//! cargo fuzz run cost_algebra -- -max_total_time=300
//! ```

use libfuzzer_sys::fuzz_target;
use panproto_mig::{COST_SCALE, Cost, CostWeights, coverage_radix, quality_units};

/// Read a `u64` out of the front of `data`, consuming it.
fn take_u64(data: &mut &[u8]) -> Option<u64> {
    let (head, rest) = data.split_at_checked(8)?;
    *data = rest;
    let bytes: [u8; 8] = head.try_into().ok()?;
    Some(u64::from_le_bytes(bytes))
}

/// Read a `u32` out of the front of `data`, consuming it.
fn take_u32(data: &mut &[u8]) -> Option<u32> {
    let (head, rest) = data.split_at_checked(4)?;
    *data = rest;
    let bytes: [u8; 4] = head.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

/// Read an `f64` out of the front of `data` by reinterpreting its bits,
/// which is how the subnormals and non-number payloads get in.
fn take_f64(data: &mut &[u8]) -> Option<f64> {
    take_u64(data).map(f64::from_bits)
}

/// The `S(k)` axioms, over three costs and a top.
fn check_valuation_axioms(top: Cost, a: Cost, b: Cost, c: Cost) {
    assert_eq!(a.combine(b, top), b.combine(a, top), "⊕ is commutative");
    assert_eq!(
        a.combine(b.combine(c, top), top),
        a.combine(b, top).combine(c, top),
        "⊕ is associative"
    );
    assert_eq!(a.combine(Cost::BOT, top), a, "⊥ is the identity of ⊕");
    assert_eq!(a.combine(top, top), top, "⊤ annihilates");
    assert!(a.combine(b, top) <= top, "⊕ never exceeds ⊤");
    assert!(a.combine(b, top) >= a, "⊕ never improves a cost");

    // Monotonicity, in the one-argument form the axiom is stated in.
    let (worse, better) = if a >= b { (a, b) } else { (b, a) };
    assert!(worse.combine(c, top) >= better.combine(c, top));
}

/// Fairness, which is Lemma 7 of Cooper and Schiex and the identity every
/// equivalence preserving transformation is an instance of. Stated for
/// `w ⪯ v`, which is `⊖`'s precondition.
fn check_fairness(top: Cost, u: Cost, v: Cost, w: Cost) {
    assert!(w <= v, "the caller must establish ⊖'s precondition");
    let moved = v.diff(w, top);
    assert!(moved <= v.combine(Cost::BOT, top), "⊖ never worsens");
    assert_eq!(moved.combine(w, top), v.combine(Cost::BOT, top));
    assert_eq!(
        u.combine(w, top).combine(moved, top),
        u.combine(v, top),
        "cost moves without changing the sum"
    );

    // `sat_diff` agrees with `diff` wherever `diff` is defined and above
    // its own truncation point.
    if v > w {
        assert_eq!(v.sat_diff(w, top), moved);
    } else {
        assert_eq!(v.sat_diff(w, top), Cost::BOT);
    }
}

/// The packed encoding, over the domain its preconditions describe.
fn check_packed_encoding(radix: u64, q: u64, drops: u32) {
    let cost = Cost::packed(q, drops, radix);
    assert_eq!(cost.quality_part(radix), q, "the quality field round-trips");
    assert_eq!(
        cost.drop_part(radix),
        u64::from(drops),
        "the drop field round-trips"
    );
    assert!(
        cost < Cost::TOP_SENTINEL,
        "no quality term may reach ⊤: reaching it would declare an assignment infeasible"
    );
    // `Ord` on the packed integer is the lexicographic order on the pair.
    if u64::from(drops) + 1 < radix {
        assert!(cost < Cost::packed(q, drops + 1, radix));
    }
    if q < COST_SCALE {
        assert!(cost < Cost::packed(q + 1, drops, radix));
    }
}

fuzz_target!(|data: &[u8]| {
    let mut cursor = data;
    let (Some(raw_a), Some(raw_b), Some(raw_c), Some(raw_top)) = (
        take_u64(&mut cursor),
        take_u64(&mut cursor),
        take_u64(&mut cursor),
        take_u64(&mut cursor),
    ) else {
        return;
    };

    // The axioms are stated over `[⊥, ⊤]`, so the operands are placed in
    // the structure rather than rejected: a draw above `⊤` would be out
    // of the domain and would say nothing.
    let top = Cost::from_raw(raw_top);
    let a = Cost::from_raw(raw_a.min(raw_top));
    let b = Cost::from_raw(raw_b.min(raw_top));
    let c = Cost::from_raw(raw_c.min(raw_top));

    check_valuation_axioms(top, a, b, c);

    let (v, w) = if a >= b { (a, b) } else { (b, a) };
    check_fairness(top, c, v, w);

    // A cost recorded under an earlier, larger bound is `⊤` under the
    // current one and must stay there.
    assert_eq!(Cost::TOP_SENTINEL.diff(a, top), top);

    if let (Some(vertices), Some(drops), Some(q)) = (
        take_u32(&mut cursor),
        take_u32(&mut cursor),
        take_u64(&mut cursor),
    ) {
        // `coverage_radix` is total on `u32`, so no input is rejected
        // here; the operands are only placed inside `packed`'s stated
        // preconditions, which it enforces itself.
        let radix = coverage_radix(vertices);
        let bounded_drops = u32::try_from(u64::from(drops) % radix).unwrap_or(0);
        check_packed_encoding(radix, q % (COST_SCALE + 1), bounded_drops);
    }

    // `quality_units` is total over `f64`: the fuzzer reaches subnormals
    // and every non-number payload, which is where a missing guard would
    // read as a perfect match rather than as the worst one.
    if let Some(x) = take_f64(&mut cursor) {
        let units = quality_units(x);
        assert!(units <= COST_SCALE);
        if x.is_nan() {
            assert_eq!(units, COST_SCALE, "a non-number is the worst quality");
        }
        if x <= 0.0 {
            assert_eq!(units, 0);
        }
    }

    // `CostWeights::new` either rejects or returns a probability vector.
    // Nothing in between is representable, and in particular five finite
    // weights whose sum overflows must not normalise to all zeroes.
    let (Some(name), Some(edge), Some(prop), Some(degree), Some(anchor)) = (
        take_f64(&mut cursor),
        take_f64(&mut cursor),
        take_f64(&mut cursor),
        take_f64(&mut cursor),
        take_f64(&mut cursor),
    ) else {
        return;
    };
    if let Ok(w) = CostWeights::new(name, edge, prop, degree, anchor) {
        let sum = w.name() + w.edge() + w.prop() + w.degree() + w.anchor();
        assert!((sum - 1.0).abs() < 1e-9, "weights sum to {sum}");
        for value in [w.name(), w.edge(), w.prop(), w.degree(), w.anchor()] {
            assert!(value.is_finite() && (0.0..=1.0).contains(&value));
        }
    }
});
