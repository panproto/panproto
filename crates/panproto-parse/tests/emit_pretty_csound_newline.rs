//! Regression: `emit_pretty` renders newline-shaped PATTERN terminals
//! as newlines rather than the bare `_` placeholder.
//!
//! csound's `_new_line` is `TOKEN(PATTERN "\r?\n")`. Pre-fix, the
//! pattern fell through `placeholder_for_pattern`'s final `else` arm
//! (no `[0-9]` / `[a-zA-Z_]` / `"` / `'` markers) and returned the bare
//! `"_"` sentinel. csound's `instrument_definition` SEQ includes a
//! REPEAT of `_statement`, each of which expects a `_new_line` between
//! statements; the placeholder dropped `_` characters between every
//! pair of structural siblings, producing unparseable output like
//! `endin _ </CsInstruments> _ </CsoundSynthesizer>`.
//!
//! The PATTERN handler now recognises `\r?\n`-shaped patterns and
//! emits them through `Output::newline()` (`Token::LineBreak`), and
//! recognises generic whitespace patterns (`\s+`, `[ \t]+`) and drops
//! them so the layout pass's policy separator inserts the actual
//! spacing. Anything else still falls through to the heuristic
//! placeholder.

#![cfg(all(feature = "grammars", feature = "lang-csound"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_parse::ParserRegistry;

const CSOUND_SRC: &[u8] = b"\
<CsoundSynthesizer>
<CsInstruments>
instr 1
out 0.5 * oscili(0.5, 440, 1)
endin
</CsInstruments>
</CsoundSynthesizer>
";

#[test]
fn emit_pretty_csound_does_not_emit_underscore_placeholder_for_newline() {
    let reg = ParserRegistry::new();
    let schema = reg
        .parse_with_protocol("csound", CSOUND_SRC, "audit.csd")
        .expect("parse");

    let bytes = reg
        .emit_pretty_with_protocol("csound", &schema)
        .expect("emit_pretty");
    let text = String::from_utf8(bytes).expect("utf8");

    assert!(
        !text.contains(" _ ") && !text.contains("\n_\n"),
        "newline-shaped PATTERN emitted as `_` placeholder: {text:?}"
    );
    assert!(
        text.contains("endin"),
        "instrument body lost endin keyword: {text:?}"
    );
    assert!(
        text.contains("</CsoundSynthesizer>"),
        "structural closer dropped: {text:?}"
    );
}
