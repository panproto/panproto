//! Regression: `emit_pretty` keeps two sibling REPEAT iterations
//! tight when the iteration body's leading "separator slot" emits
//! zero content tokens (its `CHOICE` picked `BLANK`, equivalently its
//! `OPTIONAL` was absent).
//!
//! ABC's `beam` rule (`SEQ(_nte_or_chrd,
//! REPEAT1(SEQ(CHOICE(BEAM_SEPARATOR, BLANK), _nte_or_chrd)))`) would
//! emit consecutive note letters separated by the policy separator
//! (`"CDEF"` rendered as `"C D E F"`) if the layout pass had no way to
//! distinguish "no source-level separator was present here"
//! from "two adjacent word-like tokens need to be lexically separated."
//!
//! The walker therefore detects "separator-leading SEQ" REPEAT bodies (the
//! first SEQ element is `CHOICE` containing `BLANK`, or an
//! `OPTIONAL`), emits the separator slot first while observing
//! whether any content token was produced, and pushes a `Token::NoSpace`
//! marker before the remaining SEQ members when the slot was empty.
//! The layout pass consumes the marker and suppresses the inter-Lit
//! separator for that pair, encoding the categorical reading that the
//! iteration boundary had no source-level separator.

#![cfg(all(feature = "grammars", feature = "lang-abc"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_parse::ParserRegistry;

const ABC_BEAM_SRC: &[u8] = b"X:1\nT:T\nM:4/4\nK:C\nCDEF GABc|\n";

#[test]
fn emit_pretty_abc_beam_notes_render_tight() {
    let reg = ParserRegistry::new();
    let schema = reg
        .parse_with_protocol("abc", ABC_BEAM_SRC, "audit.abc")
        .expect("parse");

    let bytes = reg
        .emit_pretty_with_protocol("abc", &schema)
        .expect("emit_pretty");
    let text = String::from_utf8(bytes).expect("utf8");

    // At least one consecutive same-beam note pair must render tight.
    // The regression renders `"C D E F G A B c"`, with a space between
    // every adjacent letter, and so fails all four pair checks.
    let loose_pairs = ["C D", "D E", "E F", "G A", "A B", "B c"];
    let still_loose = loose_pairs.iter().filter(|p| text.contains(*p)).count();
    assert!(
        still_loose < loose_pairs.len(),
        "every adjacent same-beam note pair still has a space inserted between it (tight-repeat regression): {text:?}"
    );
}
