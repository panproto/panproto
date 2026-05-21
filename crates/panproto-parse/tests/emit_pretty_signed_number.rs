//! Regression: `emit_pretty` does not insert whitespace inside a
//! `signed_number` (or any other tight unary-prefix) production.
//!
//! Pre-fix, `f(-1.0)` rendered as `f(- 1.0)` because the layout pass
//! treated `-` and `1.0` as adjacent operator-run-then-operand tokens
//! and inserted the default separator between them. The result parsed
//! as a different AST: unary-minus applied to a positive literal,
//! rather than a single negative literal.
//!
//! The fix tracks an `expecting_operand` flag through the layout pass.
//! When the previous token was emitted while the cursor expected an
//! operand (start of stream / line, after `(` / `[` / `{` / `,` / `;`,
//! or after another binary or assignment operator), and that previous
//! token is `-` / `+` / `!` / `~`, it is treated as a tight unary
//! prefix and glued to the following operand.

#![cfg(all(feature = "grammars", feature = "lang-qvr"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_parse::ParserRegistry;

const QVR_NEGATIVE_ARG: &[u8] = b"\
program p : X -> X:
    sample a <- f(-1.0)
    return a
";

#[test]
fn emit_pretty_keeps_signed_number_tight_inside_call() {
    let reg = ParserRegistry::new();
    let schema = reg
        .parse_with_protocol("qvr", QVR_NEGATIVE_ARG, "neg.qvr")
        .expect("parse");

    let bytes = reg
        .emit_pretty_with_protocol("qvr", &schema)
        .expect("emit_pretty");
    let text = String::from_utf8(bytes).expect("utf8");

    assert!(
        text.contains("-1.0") || text.contains("-1."),
        "rendered output split the signed literal: {text:?}"
    );
    assert!(
        !text.contains("- 1.0"),
        "whitespace inserted between '-' and '1.0' (pre-fix bug): {text:?}"
    );
}
