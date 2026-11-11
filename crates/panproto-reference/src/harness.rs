//! The differential harness: run the production code and the reference on
//! every enumerated input and compare exactly.

use std::fmt::Debug;

/// The first disagreement: the input, both outputs and the bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disagreement {
    /// The bound the enumeration was run at.
    pub bound: String,
    /// The input, as debug text.
    pub input: String,
    /// The production output.
    pub production: String,
    /// The reference output.
    pub reference: String,
}

impl std::fmt::Display for Disagreement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "disagreement at bound {}\n  input: {}\n  production: {}\n  reference: {}",
            self.bound, self.input, self.production, self.reference
        )
    }
}

/// Run `production` and `reference` on every input, compare with `eq`, and
/// stop at the first disagreement, which is returned (and printed). Returns
/// the number of inputs compared otherwise.
///
/// `eq` compares exactly: for a construction defined only up to relabelling it
/// exhibits a bijection that commutes with the structure maps, and for one
/// with canonical bytes it compares the bytes; [`outcome_eq`] encodes the
/// rule for refusals.
///
/// # Errors
///
/// The first [`Disagreement`].
pub fn agree<I: Debug, P: Debug, R: Debug>(
    bound: &str,
    inputs: impl IntoIterator<Item = I>,
    production: impl Fn(&I) -> P,
    reference: impl Fn(&I) -> R,
    eq: impl Fn(&P, &R) -> bool,
) -> Result<u64, Disagreement> {
    let mut n = 0u64;
    for i in inputs {
        let (p, r) = (production(&i), reference(&i));
        if !eq(&p, &r) {
            let d = Disagreement {
                bound: bound.to_owned(),
                input: format!("{i:?}"),
                production: format!("{p:?}"),
                reference: format!("{r:?}"),
            };
            println!("{d}");
            return Err(d);
        }
        n += 1;
    }
    Ok(n)
}

/// Outcome equality: two values agree by `value_eq`, two refusals agree only
/// when `refusal_eq` maps both to the same reason, and a value against a
/// refusal is a disagreement.
pub fn outcome_eq<A, B, E1, E2>(
    p: &Result<A, E1>,
    r: &Result<B, E2>,
    value_eq: impl Fn(&A, &B) -> bool,
    refusal_eq: impl Fn(&E1, &E2) -> bool,
) -> bool {
    match (p, r) {
        (Ok(a), Ok(b)) => value_eq(a, b),
        (Err(x), Err(y)) => refusal_eq(x, y),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_disagreement_is_reported_with_its_input() {
        let r = agree(
            "n < 5",
            0..5u32,
            |i| i * 2,
            |i| i + i + u32::from(*i == 3),
            |a, b| a == b,
        );
        assert_eq!(r.as_ref().map_err(|d| d.input.clone()), Err("3".to_owned()));
        assert_eq!(
            r.as_ref().err().map(ToString::to_string),
            Some(
                "disagreement at bound n < 5\n  input: 3\n  production: 6\n  reference: 7"
                    .to_owned()
            )
        );
        assert_eq!(
            agree("n < 5", 0..5u32, |i| i * 2, |i| i + i, |a, b| a == b),
            Ok(5)
        );
        let ok: Result<u8, &str> = Ok(1);
        let other: Result<u8, &str> = Ok(2);
        let err: Result<u8, &str> = Err("x");
        assert!(outcome_eq(&ok, &ok, |a, b| a == b, |a, b| a == b));
        assert!(!outcome_eq(&ok, &other, |a, b| a == b, |a, b| a == b));
        assert!(!outcome_eq(&ok, &err, |a, b| a == b, |a, b| a == b));
        assert!(outcome_eq(&err, &err, |a, b| a == b, |a, b| a == b));
    }
}
