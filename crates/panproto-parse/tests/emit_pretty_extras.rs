//! Regression: `emit_pretty` drains tree-sitter `extras` children
//! (typically `line_comment` / `block_comment`) as a side channel,
//! rather than letting them suppress emission of every structural
//! sibling.
//!
//! Pre-fix, extras like supercollider's `line_comment` were recorded
//! as children of the surrounding vertex but were not referenced
//! anywhere in the production grammar. Cursor-driven CHOICE dispatch
//! found no alternative whose body could satisfy a `line_comment`
//! child kind, returned `None`, and the surrounding `REPEAT` loop
//! terminated after one iteration with `consumed == 0`. The result on
//! supercollider was an empty byte output for any non-trivial source.
//!
//! `Grammar::extras` now records the set of named-symbol / aliased
//! kinds declared under the grammar's `extras` array. `emit_production`
//! drains leading extras-kind edges from the cursor at every entry,
//! and `emit_vertex` drains trailing extras after the rule walk
//! completes. Each drained extra is emitted via `emit_vertex` to
//! preserve its content.

#![cfg(all(feature = "grammars", feature = "lang-supercollider"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_parse::ParserRegistry;

const SUPERCOLLIDER_SRC: &[u8] = b"// hello\nPdef(\\kick, Pbind(\\bus, \\drums, \\dur, 0.25));\n";

#[test]
fn emit_pretty_supercollider_preserves_comment_and_structural_siblings() {
    let reg = ParserRegistry::new();
    let schema = reg
        .parse_with_protocol("supercollider", SUPERCOLLIDER_SRC, "audit.scd")
        .expect("parse");

    let bytes = reg
        .emit_pretty_with_protocol("supercollider", &schema)
        .expect("emit_pretty");
    let text = String::from_utf8(bytes).expect("utf8");

    assert!(
        !text.is_empty(),
        "emit_pretty returned an empty byte string (pre-fix regression)"
    );
    assert!(
        text.contains("Pdef") && text.contains("Pbind"),
        "structural function calls missing from output: {text:?}"
    );
    assert!(
        text.contains("// hello"),
        "leading extras (line_comment) dropped from output: {text:?}"
    );
}
