//! Regression: a literal-value Lit that ends in trailing newlines
//! pushes a `LineBreak` token for the newline tail rather than
//! keeping the newline characters inside the Lit.
//!
//! Pre-fix, ABC's `reference_number_line` matched `"X:1\n"` and
//! recorded that whole span as the vertex's `literal-value`. The
//! emitter's leaf shortcut pushed it as `Lit("X:1\n")`. The layout
//! pass then ran `needs_space_between("X:1\n", "T:")` for the
//! following tune-header line; neither token was punctuation in the
//! recognised classes, so the fall-through "keep operator runs apart"
//! rule kicked in and inserted a separator at column 0 of the next
//! line: `"X:1\n T: Test\n..."`. Only the first inter-line gap was
//! affected, because every other line's terminator went through the
//! `STRING "\n"` `_NL` rule which my newline-as-LineBreak fix had
//! already routed correctly.
//!
//! The fix: `Output::token` now splits a trailing-newline tail off
//! any Lit value, emits the trimmed prefix as a `Lit`, and pushes a
//! `LineBreak` for the newline tail. Layout treats `LineBreak` as a
//! line-state reset, so the next Lit starts at column 0 with no
//! intervening policy separator.

#![cfg(all(feature = "grammars", feature = "lang-abc"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_parse::ParserRegistry;

const ABC_SRC: &[u8] = b"X:1\nT:Test\nM:4/4\nK:C\nCDEF\n";

#[test]
fn emit_pretty_strips_trailing_newline_from_literal_lit() {
    let reg = ParserRegistry::new();
    let schema = reg
        .parse_with_protocol("abc", ABC_SRC, "audit.abc")
        .expect("parse");

    let bytes = reg
        .emit_pretty_with_protocol("abc", &schema)
        .expect("emit_pretty");
    let text = String::from_utf8(bytes).expect("utf8");

    // The pre-fix output was `"X:1\n T: Test\n..."`. After the fix
    // every newline-prefixed line starts at column 0.
    assert!(
        !text.contains("\n T:") && !text.contains("\n M:") && !text.contains("\n K:"),
        "leading space after a line break (literal Lit with embedded newline regression): {text:?}"
    );
    // Sanity: the header content still survives.
    for keyword in ["X:", "T:", "M:", "K:"] {
        assert!(
            text.contains(keyword),
            "header keyword {keyword:?} dropped: {text:?}"
        );
    }
}
