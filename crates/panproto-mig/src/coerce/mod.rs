//! Sort coercion via witness lenses.
//!
//! A [`SortLensWitness`] describes how to convert values of one sort
//! into another and (when invertible) back. Every witness carries a
//! [`panproto_gat::CoercionClass`] that classifies its round-trip
//! behaviour:
//!
//! * **Iso** — `inverse(forward(v)) = v` *and* `forward(inverse(w)) = w`
//!   on every value of the two carriers. Complement is empty.
//! * **Retraction** — `inverse(forward(v)) = v` only. The reverse
//!   direction may lose information. Complement captures the residual.
//! * **Projection** — neither direction round-trips without external
//!   data. Complement stores the original value verbatim.
//!
//! The built-in [`WitnessLibrary`] ships a small set of generic
//! (protocol-agnostic) witnesses. Most built-ins are classified
//! [`CoercionClass::Retraction`] because the reverse direction is
//! only defined on a sub-carrier (e.g. `StrToInt` requires canonical
//! decimal strings). Protocol cartridges may extend the library at
//! runtime.
//!
//! # Categorical integration
//!
//! Emitting a [`SortLensWitness`] as a `TheoryTransform::CoerceSort`
//! endofunctor is only sound when the naturality check passes: every
//! operation whose signature mentions the source sort must commute
//! with `ℓ` under sort substitution. This module provides
//! [`witness_satisfies_lens_laws`] to verify the witness alone; the
//! CSP in `hom_search` then enforces the per-op commutation when the
//! candidate is considered.

use std::sync::Arc;

use panproto_expr::{Env, Literal, eval};
use panproto_gat::{CoercionClass, ValueKind};

pub mod witness;

pub use witness::{SortLensWitness, WitnessLibrary, default_witness_library};

/// A single sample value used to verify a witness's lens laws.
pub type WitnessSample = Literal;

/// Check that `witness` satisfies the lens laws on every supplied sample.
///
/// Returns `Ok(())` when all laws hold on the stated domain, otherwise
/// an error describing the first violation. Short-circuits on the
/// first failure.
///
/// # Laws
///
/// * **`GetPut`** — for every `s` in `source_samples`,
///   `inverse(forward(s)) = s`. Required for all classifications.
/// * **`PutGet`** — for every `t` in `target_samples`,
///   `forward(inverse(t)) = t`. Required for [`CoercionClass::Iso`]
///   only. For `Retraction` / `Projection`, `target_samples` is
///   ignored by this checker; use [`witness_forward_fails_on`] to
///   positively demonstrate that the reverse direction does NOT
///   round-trip on a specific target sample.
///
/// For `Iso` witnesses the function REQUIRES at least one target
/// sample. An empty slice would vacuously pass the `PutGet` loop and
/// hide real bugs in the inverse direction.
///
/// Lambda parameter names are drawn from `witness.forward_param` and
/// `witness.inverse_param`. The evaluator runs under the default
/// `EvalConfig`; callers wanting deeper semantic checks can repeat the
/// test with a custom config.
///
/// # Purity
///
/// The function evaluates `forward` and `inverse` purely for their
/// returned values and discards any intermediate expressions. The
/// panproto-expr evaluator is side-effect-free by construction, so no
/// witness sample can mutate external state. Should a future builtin
/// grow side effects, this checker would execute those effects once
/// per sample — a semantics change that deserves a deliberate review.
///
/// # Errors
///
/// Returns an error describing the first failing sample, or, for
/// `Iso` witnesses, an error when `target_samples` is empty.
pub fn witness_satisfies_lens_laws(
    witness: &SortLensWitness,
    source_samples: &[WitnessSample],
    target_samples: &[WitnessSample],
) -> Result<(), String> {
    let config = panproto_expr::EvalConfig::default();

    for s in source_samples {
        let forward_env = Env::new().extend(Arc::clone(&witness.forward_param), s.clone());
        let forward_result = eval(&witness.forward, &forward_env, &config)
            .map_err(|e| format!("forward eval on {s:?} failed: {e}"))?;

        let inverse = witness
            .inverse
            .as_ref()
            .ok_or_else(|| "witness has no inverse; cannot verify GetPut".to_owned())?;
        let inverse_param = witness
            .inverse_param
            .as_ref()
            .ok_or_else(|| "witness inverse_param is missing".to_owned())?;
        let inverse_env = Env::new().extend(Arc::clone(inverse_param), forward_result);
        let round_trip = eval(inverse, &inverse_env, &config)
            .map_err(|e| format!("inverse eval round-trip on {s:?} failed: {e}"))?;

        if !literal_equal(&round_trip, s) {
            return Err(format!(
                "GetPut violation for sample {s:?}: inverse(forward(s)) = {round_trip:?}"
            ));
        }
    }

    if witness.class == CoercionClass::Iso {
        let inverse = witness
            .inverse
            .as_ref()
            .ok_or_else(|| "Iso witness must have an inverse".to_owned())?;
        let inverse_param = witness
            .inverse_param
            .as_ref()
            .ok_or_else(|| "Iso witness inverse_param is missing".to_owned())?;

        if target_samples.is_empty() {
            return Err(
                "Iso witness requires at least one target sample to verify PutGet; \
                 an empty target slice would vacuously pass and hide bugs in the \
                 inverse direction"
                    .to_owned(),
            );
        }

        for t in target_samples {
            let inverse_env = Env::new().extend(Arc::clone(inverse_param), t.clone());
            let back = eval(inverse, &inverse_env, &config)
                .map_err(|e| format!("inverse eval on {t:?} failed: {e}"))?;
            let forward_env = Env::new().extend(Arc::clone(&witness.forward_param), back);
            let round_trip = eval(&witness.forward, &forward_env, &config)
                .map_err(|e| format!("forward eval round-trip on {t:?} failed: {e}"))?;
            if !literal_equal(&round_trip, t) {
                return Err(format!(
                    "PutGet violation for target sample {t:?}: forward(inverse(t)) = {round_trip:?}"
                ));
            }
        }
    }

    Ok(())
}

/// Positively assert that `forward(inverse(t)) ≠ t` (or that the
/// round-trip fails to evaluate) for an off-domain target sample.
///
/// For `Retraction` and `Projection` witnesses, callers should use this
/// to demonstrate the witness is NOT an iso on the full target carrier.
/// This closes a common correctness gap: by default
/// [`witness_satisfies_lens_laws`] only checks `GetPut`, so a buggy
/// `Retraction` that secretly is an `Iso` (or vice versa) would not be
/// caught. Use this to exhibit an explicit target that the forward
/// direction does not recover.
///
/// Returns `Ok(())` when the round-trip either fails to evaluate or
/// evaluates to a value not equal to `t`. Returns `Err` when the
/// round-trip unexpectedly succeeds (indicating the witness may be
/// stronger than its declared class).
///
/// # Errors
///
/// Returns an error message when `forward(inverse(t)) == t` (the
/// witness round-tripped an off-domain target, contradicting its
/// non-Iso classification) or when the witness has no inverse.
pub fn witness_forward_fails_on(
    witness: &SortLensWitness,
    off_domain_target: &WitnessSample,
) -> Result<(), String> {
    let config = panproto_expr::EvalConfig::default();
    let inverse = witness
        .inverse
        .as_ref()
        .ok_or_else(|| "witness has no inverse".to_owned())?;
    let inverse_param = witness
        .inverse_param
        .as_ref()
        .ok_or_else(|| "witness inverse_param is missing".to_owned())?;

    let inverse_env = Env::new().extend(Arc::clone(inverse_param), off_domain_target.clone());
    let Ok(back) = eval(inverse, &inverse_env, &config) else {
        return Ok(()); // inverse failed ⇒ forward cannot round-trip
    };
    let forward_env = Env::new().extend(Arc::clone(&witness.forward_param), back);
    eval(&witness.forward, &forward_env, &config).map_or(Ok(()), |round_trip| {
        if literal_equal(&round_trip, off_domain_target) {
            Err(format!(
                "expected off-domain target {off_domain_target:?} to NOT round-trip, \
                 but forward(inverse(t)) = {round_trip:?} matched"
            ))
        } else {
            Ok(())
        }
    })
}

/// Structural equality on [`Literal`] values, tolerating float rounding
/// down to `1e-12` relative tolerance. `Literal` itself does not
/// implement `PartialEq` over floats in an IEEE-safe way.
///
/// Edge cases:
/// - `NaN == NaN` under this predicate (standard `f64` equality does
///   not satisfy this).
/// - `+0.0 == -0.0` (their subtraction is `0.0`, so they compare equal).
/// - `±inf` compares equal to itself and unequal to finite values; the
///   subtraction `inf - inf = NaN` makes two infinities of the same
///   sign fall into the NaN branch, so we special-case direct equality
///   first.
/// - Subnormals compare correctly because the tolerance floor (scale
///   clamped to `>= 1.0`) does not collapse subnormal-range comparisons
///   to zero.
/// - [`Literal`] variants such as `Closure` (when present in the
///   underlying `panproto_expr::Literal` enum) fall through to the
///   catch-all `a == b` branch and therefore compare by whatever
///   `PartialEq` is derived for them. Closures are equatable by
///   identity at best: two distinct closure values with the same
///   semantics will compare unequal, so witnesses whose forward /
///   inverse return closures cannot be lens-law-checked by this
///   predicate. Restrict witness samples to first-order carriers
///   (ints, floats, strings, bools, records of those, lists of
///   those) - which is exactly the set the built-in library
///   exercises.
fn literal_equal(a: &Literal, b: &Literal) -> bool {
    match (a, b) {
        (Literal::Float(x), Literal::Float(y)) => {
            if x.is_nan() && y.is_nan() {
                return true;
            }
            // Handle ±inf explicitly: `inf − inf = NaN` would otherwise
            // sink two equal infinities into the relative-tolerance
            // branch and report them as unequal.
            if x.is_infinite() || y.is_infinite() {
                // Equal magnitude *and* equal sign means the same
                // infinity; everything else (finite vs inf, opposite
                // infinities) is unequal. Using bit comparison
                // sidesteps the Clippy float-equality lint while
                // preserving IEEE semantics for +inf vs -inf.
                return x.to_bits() == y.to_bits();
            }
            // ±0 equivalence: `-0.0 - 0.0 = 0.0`, which is `<= 0`, so
            // the relative-tolerance branch handles this correctly.
            let scale = x.abs().max(y.abs()).max(1.0);
            (x - y).abs() <= 1e-12 * scale
        }
        (Literal::Record(ra), Literal::Record(rb)) => {
            ra.len() == rb.len()
                && ra
                    .iter()
                    .zip(rb.iter())
                    .all(|((ka, va), (kb, vb))| ka == kb && literal_equal(va, vb))
        }
        (Literal::List(la), Literal::List(lb)) => {
            la.len() == lb.len() && la.iter().zip(lb.iter()).all(|(x, y)| literal_equal(x, y))
        }
        _ => a == b,
    }
}

/// Resolve a `ValueKind` to a human-readable label suitable for
/// explanations and candidate step descriptions.
#[must_use]
pub const fn value_kind_label(kind: ValueKind) -> &'static str {
    match kind {
        ValueKind::Bool => "bool",
        ValueKind::Int => "int",
        ValueKind::Float => "float",
        ValueKind::Str => "str",
        ValueKind::Bytes => "bytes",
        ValueKind::Token => "token",
        ValueKind::Null => "null",
        ValueKind::Any => "any",
    }
}

/// Signature alias for callers that want to reason about the expression
/// type pair witnessed by a coercion without pulling panproto-gat.
pub use panproto_gat::ValueKind as CarrierKind;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]
mod edge_case_tests {
    use super::*;

    #[test]
    fn literal_equal_signed_zero() {
        assert!(literal_equal(&Literal::Float(0.0), &Literal::Float(-0.0)));
        assert!(literal_equal(&Literal::Float(-0.0), &Literal::Float(0.0)));
    }

    #[test]
    fn literal_equal_nan_is_reflexive() {
        assert!(literal_equal(
            &Literal::Float(f64::NAN),
            &Literal::Float(f64::NAN)
        ));
    }

    #[test]
    fn literal_equal_distinct_infinities() {
        assert!(literal_equal(
            &Literal::Float(f64::INFINITY),
            &Literal::Float(f64::INFINITY)
        ));
        assert!(literal_equal(
            &Literal::Float(f64::NEG_INFINITY),
            &Literal::Float(f64::NEG_INFINITY)
        ));
        assert!(!literal_equal(
            &Literal::Float(f64::INFINITY),
            &Literal::Float(f64::NEG_INFINITY)
        ));
        assert!(!literal_equal(
            &Literal::Float(f64::INFINITY),
            &Literal::Float(f64::MAX)
        ));
    }

    #[test]
    fn literal_equal_subnormals() {
        // Subnormals near-equal under the relative tolerance (scale
        // floor of 1.0 keeps the absolute tolerance at 1e-12, so any
        // two subnormals are closer than that).
        let a = f64::MIN_POSITIVE / 2.0; // subnormal
        let b = f64::MIN_POSITIVE / 4.0;
        assert!(literal_equal(&Literal::Float(a), &Literal::Float(b)));
    }

    #[test]
    fn literal_equal_near_f64_max() {
        // At extreme magnitudes the relative tolerance is 1e-12 * |x|,
        // which is still ~1e296 near f64::MAX. The documented
        // behaviour is explicit: "1e-12 relative tolerance".
        let x = f64::MAX;
        let y = f64::MAX - 1.0; // identical at f64 precision
        assert!(literal_equal(&Literal::Float(x), &Literal::Float(y)));
        // But a value off by a factor of 2 is rejected.
        assert!(!literal_equal(
            &Literal::Float(f64::MAX),
            &Literal::Float(f64::MAX / 2.0)
        ));
    }

    #[test]
    fn iso_witness_requires_target_samples() {
        // Construct a minimal Iso witness and confirm the checker
        // refuses to vacuously pass with an empty target slice.
        let v: std::sync::Arc<str> = std::sync::Arc::from("v");
        let iso = SortLensWitness {
            name: "id_int".to_owned(),
            source_kind: ValueKind::Int,
            target_kind: ValueKind::Int,
            class: CoercionClass::Iso,
            forward_param: std::sync::Arc::clone(&v),
            forward: panproto_expr::Expr::Var(std::sync::Arc::clone(&v)),
            inverse_param: Some(std::sync::Arc::clone(&v)),
            inverse: Some(panproto_expr::Expr::Var(v)),
            description: "identity on Int".to_owned(),
        };
        let err = witness_satisfies_lens_laws(&iso, &[Literal::Int(1), Literal::Int(2)], &[])
            .unwrap_err();
        assert!(
            err.contains("requires at least one target sample"),
            "expected empty-target rejection; got: {err}"
        );
        // With a target sample, the identity iso is accepted.
        witness_satisfies_lens_laws(&iso, &[Literal::Int(1)], &[Literal::Int(7)])
            .expect("identity iso must pass with a target sample");
    }

    // The retraction witnesses shipped in `default_witness_library` all
    // claim `GetPut` holds and `PutGet` fails on an off-domain target.
    // Pin that failure positively so a future refactor that accidentally
    // widens the inverse's domain (or loses the class distinction)
    // surfaces here. See `witness.rs` docstrings for the off-domain
    // witness values expected to fail.
    mod witness_forward_fails_on_off_domain {
        use super::{
            Literal,
            witness::{
                bool_to_int_witness, int_to_bool_witness, int_to_str_witness, str_to_int_witness,
            },
            witness_forward_fails_on,
        };

        #[test]
        fn str_to_int_rejects_non_numeric_strings() {
            let w = str_to_int_witness();
            // The inverse is `IntToStr`; we exhibit a target string the
            // forward map (`StrToInt`) rejects → round-trip must fail.
            for bad in ["abc", "", " 3", "1.5"] {
                witness_forward_fails_on(&w, &Literal::Str(bad.to_owned()))
                    .expect("str_to_int forward must fail on non-numeric string");
            }
        }

        #[test]
        fn int_to_str_fails_when_inverse_domain_gaps_leak() {
            // `int_to_str`'s inverse is `str_to_int`; the *forward*
            // direction is total, so `witness_forward_fails_on` applied
            // to the canonical integer string "3" must reject (round-trip
            // succeeds), not accept.
            let w = int_to_str_witness();
            let result = witness_forward_fails_on(&w, &Literal::Str("3".to_owned()));
            assert!(
                result.is_err(),
                "canonical decimal string must round-trip, but witness_forward_fails_on accepted it"
            );
        }

        #[test]
        fn bool_to_int_fails_on_out_of_range_ints() {
            // `bool_to_int` inverse is `int_to_bool`, which maps `0 → false`,
            // `1 → true`, and any other int to an error. A target value
            // of `2` must therefore positively fail the round-trip.
            let w = bool_to_int_witness();
            witness_forward_fails_on(&w, &Literal::Int(2))
                .expect("bool_to_int forward must fail on int outside {0,1}");
        }

        #[test]
        fn int_to_bool_fails_on_non_boolean_ints() {
            // `int_to_bool`'s inverse `bool_to_int` only produces `0`/`1`,
            // so any source `Int` outside that range breaks `GetPut`.
            let w = int_to_bool_witness();
            witness_forward_fails_on(&w, &Literal::Int(7))
                .expect("int_to_bool forward must fail on int outside {0,1}");
        }
    }
}
