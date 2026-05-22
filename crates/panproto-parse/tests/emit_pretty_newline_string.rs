//! Regression: a grammar STRING whose value is a newline (e.g. abc's
//! `_NL = STRING "\n"`) routes through the layout pass's `LineBreak`
//! channel rather than through `Token::Lit`.
//!
//! Pre-fix, `Output::token` pushed every grammar STRING as a `Lit`. A
//! literal `"\n"` STRING left the newline character in the output but
//! the layout pass's `needs_space_between` then inserted the configured
//! separator between the newline and the following token, producing
//! leading spaces on every line after the first when the grammar used
//! `STRING "\n"` as a line terminator (abc's `_NL`, csound's
//! line-terminator alts, and any grammar with a similar shape). The
//! same path also produced trailing spaces before every newline
//! because the separator landed between the line's last content token
//! and the following `"\n"` Lit.
//!
//! `Output::token` now recognises `"\n"` / `"\r"` / `"\r\n"` and pushes
//! `Token::LineBreak` directly. The layout pass treats `LineBreak` as a
//! line-state reset, so neither leading nor trailing separators slip
//! in around the structural line break.

#![cfg(all(feature = "grammars", feature = "lang-abc"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_parse::ParserRegistry;

const ABC_HEADER_SRC: &[u8] = b"X:1\nT:Test\nM:4/4\nL:1/8\nK:C\nCDEF GABc|\n";

#[test]
fn emit_pretty_abc_does_not_leak_whitespace_around_string_newline() {
    let reg = ParserRegistry::new();
    let schema = reg
        .parse_with_protocol("abc", ABC_HEADER_SRC, "audit.abc")
        .expect("parse");

    let bytes = reg
        .emit_pretty_with_protocol("abc", &schema)
        .expect("emit_pretty");
    let text = String::from_utf8(bytes).expect("utf8");

    // No trailing space before a newline. Pre-fix every header line
    // ended with ` \n` because `needs_space_between(content, "\n")`
    // returned true.
    for line in text.lines() {
        assert!(
            !line.ends_with(' '),
            "trailing space before newline survived: {line:?} in {text:?}"
        );
    }

    // Header keywords still emit. Pre-fix this would have passed
    // (the bug was whitespace, not content), but it pins the keyword
    // dispatch alongside the layout fix.
    for keyword in ["X:", "M:", "L:", "K:"] {
        assert!(
            text.contains(keyword),
            "header keyword {keyword:?} dropped from output: {text:?}"
        );
    }
}
