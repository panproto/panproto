//! Regression: `emit_pretty` picks `BLANK` over a STRING-only
//! alternative when the cursor has no unconsumed children and the
//! CHOICE includes `BLANK`.
//!
//! Pre-fix, QVR's `sample_step` ended its argument list with a
//! `CHOICE(",", BLANK)` (optional trailing comma). The
//! `chose-alt-fingerprint` constraint that drives CHOICE dispatch is
//! built from the vertex's interstitial fragments joined into one
//! blob; an `f(1.0, 2.0, 3.0)` invocation deposited `","` three times
//! into that blob (once per arg-separator gap). The trailing CHOICE
//! then scored `","` higher than `BLANK` (3 vs 0 literal matches)
//! and emitted `f(1.0, 2.0, 3.0,)` with a phantom trailing comma.
//!
//! The fix gates CHOICE dispatch on the cursor's residual
//! unconsumed multiset: when no edges remain to discriminate the
//! alternatives AND `BLANK` is one of them, the only categorically
//! correct alt is `BLANK`. The literal-blob fingerprint cannot
//! distinguish multiple positional CHOICEs at the same vertex, so the
//! cursor-exhaustion gate is the structural invariant.

#![cfg(all(feature = "grammars", feature = "lang-qvr"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_parse::ParserRegistry;

const QVR_TRAILING_COMMA: &[u8] = b"\
program p : X -> X:
    sample a <- f(1.0, 2.0, 3.0)
    return a
";

#[test]
fn emit_pretty_does_not_emit_phantom_trailing_comma_in_commasep1() {
    let reg = ParserRegistry::new();
    let schema = reg
        .parse_with_protocol("qvr", QVR_TRAILING_COMMA, "trail.qvr")
        .expect("parse");

    let bytes = reg
        .emit_pretty_with_protocol("qvr", &schema)
        .expect("emit_pretty");
    let text = String::from_utf8(bytes).expect("utf8");

    // The phantom trailing comma rendered as `3.0,)`. With the fix the
    // closing paren immediately follows the last arg.
    assert!(
        !text.contains("3.0,)") && !text.contains("3.0 ,)"),
        "phantom trailing comma in commaSep1 output: {text:?}"
    );
    // Sanity: all three args still survive.
    for arg in ["1.0", "2.0", "3.0"] {
        assert!(text.contains(arg), "arg {arg:?} dropped: {text:?}");
    }
}
