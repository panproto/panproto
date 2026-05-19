//! Regression: `emit_pretty` renders every iteration of repetition
//! inside a `FIELD(...)` body, not just the first.
//!
//! Two correlated shapes are exercised:
//!
//! - `FIELD(REPEAT(SYMBOL))` directly: `program_decl`'s
//!   `field('steps', repeat($._program_step))`.
//! - `FIELD(SEQ(SYMBOL, REPEAT(SEQ(STRING, SYMBOL))))`: the
//!   `commaSep1` shape that `field('args', commaSep1($._draw_arg))`
//!   expands to. The previous fix only peeled top-level
//!   `REPEAT`/`REPEAT1` off the field content; the SEQ wrapper in
//!   `commaSep1` defeated the peel and the inner REPEAT still walked
//!   inside the first consumed child's cursor, dropping every
//!   sibling arg beyond the first.
//!
//! `program_decl` in the QVR grammar is `seq('program', ..., field('steps',
//! repeat($._program_step)), $.return_step, ...)`. Before the
//! `Field { content: Repeat(...) }` carve-out in `emit_pretty.rs`,
//! the emitter consumed exactly one `steps`-named edge and walked the
//! REPEAT against that one child's cursor, silently dropping every
//! other step on the parent vertex.
//!
//! This test parses a multi-step program, walks `emit_pretty`, and
//! checks every step survives.

#![cfg(all(feature = "grammars", feature = "lang-qvr"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_parse::ParserRegistry;

const QVR_MULTI_STEP: &[u8] = b"\
program p : X -> X:
    sample a <- f()
    sample b <- g()
    return a
";

#[test]
fn emit_pretty_preserves_every_step_in_field_repeat() {
    let reg = ParserRegistry::new();
    let schema = reg
        .parse_with_protocol("qvr", QVR_MULTI_STEP, "multi.qvr")
        .expect("parse");

    let rendered = reg
        .emit_pretty_with_protocol("qvr", &schema)
        .expect("emit_pretty");
    let text = String::from_utf8(rendered).expect("utf8");

    // Every step's terminal tokens must survive. The bug being
    // closed here is the FIELD(REPEAT) drop: pre-fix output was
    // `program p: X -> X:\nsample a <- f()\nreturn a\n` (one sample,
    // not two). Token-order assertions for any one step body are out
    // of scope — that's the cursor-dispatch problem from #104 surfaced
    // at a deeper rule than expression_block. Here we only assert the
    // step *count* is preserved across `FIELD(REPEAT(...))`.
    let f_calls = text.matches("f(").count();
    let g_calls = text.matches("g(").count();
    assert_eq!(
        f_calls, 1,
        "first step's function call f() missing from {text:?}"
    );
    assert_eq!(
        g_calls, 1,
        "second step's function call g() missing from {text:?} \
         (FIELD(REPEAT) regression: second step was dropped before this fix)"
    );
    assert!(text.contains("return"), "return step missing from {text:?}");
}

/// Multiple args in a `commaSep1`-shaped field must all survive
/// `emit_pretty`. `field('args', commaSep1($._draw_arg))` expands to
/// `FIELD(SEQ(SYMBOL X, REPEAT(SEQ(',', SYMBOL X))))`. Pre-fix, the
/// inner REPEAT searched the first consumed child's cursor for more
/// args, found none, and broke after one iteration. With the
/// field-context override, the SYMBOLs under the FIELD body pull
/// successive `take_field("args")` edges from the outer cursor.
const QVR_MULTI_ARG: &[u8] = b"\
program p : X -> X:
    sample a <- f(1.0, 2.0, 3.0)
    return a
";

#[test]
fn emit_pretty_preserves_every_arg_in_field_commasep1() {
    let reg = ParserRegistry::new();
    let schema = reg
        .parse_with_protocol("qvr", QVR_MULTI_ARG, "args.qvr")
        .expect("parse");

    let rendered = reg
        .emit_pretty_with_protocol("qvr", &schema)
        .expect("emit_pretty");
    let text = String::from_utf8(rendered).expect("utf8");

    for arg in ["1.0", "2.0", "3.0"] {
        assert!(
            text.contains(arg),
            "rendered output is missing arg {arg:?}; commaSep1 regression: {text:?}",
        );
    }
}
