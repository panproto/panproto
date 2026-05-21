//! Regression: `emit_pretty` includes children produced by named
//! `ALIAS { content: STRING, value: K, named: true }` productions when
//! picking a CHOICE alternative.
//!
//! Pre-fix, `referenced_symbols` walked into an `ALIAS`'s content and
//! ignored the alias `value`. Cursor-driven dispatch then matched
//! alternatives only on inner SYMBOLs, so the lilypond `named_context`
//! third CHOICE arm (`SEQ(ALIAS_punctuation, CHOICE(symbol, string))`,
//! introducing `punctuation` and `string` children) was invisible: the
//! emitter fell through to the BLANK alternative and dropped both
//! children. `\new Voice = "kick"` rendered as `\new Voice`, losing the
//! voice label entirely.
//!
//! The fix yields the alias `value` from `referenced_symbols` when the
//! alias is named, so cursor-driven dispatch can recognise the alt that
//! introduces a `punctuation` child.

#![cfg(all(feature = "grammars", feature = "lang-lilypond"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_parse::ParserRegistry;

const LILY_NAMED_CONTEXT: &[u8] =
    b"\\version \"2.24.0\"\n\\new Voice = \"kick\" { c,4 r4 }\n";

#[test]
fn emit_pretty_preserves_named_context_punctuation_and_string() {
    let reg = ParserRegistry::new();
    let schema = reg
        .parse_with_protocol("lilypond", LILY_NAMED_CONTEXT, "audit.ly")
        .expect("parse");

    let bytes = reg
        .emit_pretty_with_protocol("lilypond", &schema)
        .expect("emit_pretty");
    let text = String::from_utf8(bytes).expect("utf8");

    assert!(
        text.contains('='),
        "named_context `=` punctuation dropped from {text:?}"
    );
    assert!(
        text.contains("kick"),
        "named_context `string` child dropped from {text:?}"
    );
    assert!(
        text.contains("Voice"),
        "named_context `symbol` child dropped from {text:?}"
    );
}
