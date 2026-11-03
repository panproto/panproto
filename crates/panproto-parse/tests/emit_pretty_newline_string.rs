//! Regression: a grammar STRING whose value is a newline (e.g. abc's
//! `_NL = STRING "\n"`) routes through the layout pass's `LineBreak`
//! channel rather than through `Token::Lit`.
//!
//! Pushed as a `Lit`, a literal `"\n"` STRING leaves the newline
//! character in the output, but the layout pass's `needs_space_between`
//! then inserts the configured separator between the newline and the
//! following token, producing leading spaces on every line after the
//! first when the grammar uses `STRING "\n"` as a line terminator (abc's
//! `_NL`, csound's line-terminator alts, and any grammar with a similar
//! shape). The same path also produces trailing spaces before every
//! newline, because the separator lands between the line's last content
//! token and the following `"\n"` Lit.
//!
//! `Output::token` therefore recognises `"\n"` / `"\r"` / `"\r\n"` and pushes
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

    // No trailing space before a newline. The regression ends every
    // header line with ` \n` because `needs_space_between(content, "\n")`
    // returns true.
    for line in text.lines() {
        assert!(
            !line.ends_with(' '),
            "trailing space before newline survived: {line:?} in {text:?}"
        );
    }

    // Header keywords still emit. The regression is whitespace, not
    // content, but this pins the keyword dispatch alongside the layout
    // rule.
    for keyword in ["X:", "M:", "L:", "K:"] {
        assert!(
            text.contains(keyword),
            "header keyword {keyword:?} dropped from output: {text:?}"
        );
    }
}
