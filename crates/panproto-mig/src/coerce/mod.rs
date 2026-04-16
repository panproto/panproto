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
//! (protocol-agnostic) iso witnesses. Protocol cartridges may extend
//! the library at runtime.
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
/// Returns `Ok(())` when all laws hold, otherwise an error describing
/// the first violation.
///
/// # Laws
///
/// * **`GetPut`** — for every `s` in `source_samples`,
///   `inverse(forward(s)) = s`. Required for all classifications.
/// * **`PutGet`** — for every `t` in `target_samples`,
///   `forward(inverse(t)) = t`. Required for `Iso` only.
///
/// Lambda parameter names are drawn from `witness.forward_param` and
/// `witness.inverse_param`. The evaluator runs under the default
/// `EvalConfig`; callers wanting deeper semantic checks can repeat the
/// test with a custom config.
///
/// # Errors
///
/// Returns an error describing the first failing sample.
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

/// Structural equality on [`Literal`] values, tolerating float rounding
/// down to `1e-12` relative tolerance. `Literal` itself does not
/// implement `PartialEq` over floats in an IEEE-safe way.
fn literal_equal(a: &Literal, b: &Literal) -> bool {
    match (a, b) {
        (Literal::Float(x), Literal::Float(y)) => {
            if x.is_nan() && y.is_nan() {
                return true;
            }
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
