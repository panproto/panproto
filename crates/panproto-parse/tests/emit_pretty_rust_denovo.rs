//! Regression tests for rust de-novo (by-construction / `forget_layout`)
//! `emit_pretty`.
//!
//! These pin the three gaps where information that rides the layout fibre on
//! parsed schemas needs a grammar-derived reconstruction rule on the abstract
//! path. The bar is AST round-trip: emit's output must re-parse to the same
//! kind multiset, not match the original bytes.

#![cfg(all(feature = "grammars", feature = "lang-rust"))]
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use panproto_parse::ParserRegistry;
use panproto_schema::{Constraint, Schema};
use std::collections::BTreeMap;

fn registry() -> ParserRegistry {
    ParserRegistry::new()
}

fn kind_multiset(s: &Schema) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for v in s.vertices.values() {
        *m.entry(v.kind.to_string()).or_default() += 1;
    }
    m
}

fn with_big_stack<F: FnOnce() + Send + 'static>(inner: F) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(inner)
        .expect("spawn")
        .join()
        .expect("worker panicked");
}

/// Defect 1: a line comment must not absorb the items that follow it. Before
/// the fix the abstract emit placed every following item on the comment line,
/// so the whole file re-parsed as one `line_comment`. The fix registers
/// rust's `//` prefix (whose grammar body is `SEQ[STRING "//", CHOICE[…]]`),
/// so the layout pass breaks the line after the comment leaf.
#[test]
fn line_comment_does_not_absorb_following_item() {
    with_big_stack(|| {
        let reg = registry();
        let src = b"// doc\npub mod schema;\n";
        let parsed = reg.parse_with_protocol("rust", src, "a.rs").expect("parse");
        let original = kind_multiset(&parsed);
        let mut abs = parsed.clone();
        abs.forget_layout_in_place();
        let bytes = reg
            .emit_pretty_with_protocol("rust", &abs)
            .expect("emit_pretty");
        let reparsed = reg
            .parse_with_protocol("rust", &bytes, "out.rs")
            .expect("re-parse");
        assert_eq!(
            kind_multiset(&reparsed),
            original,
            "abstract emit did not round-trip to the same AST: {:?}",
            String::from_utf8_lossy(&bytes)
        );
    });
}

/// Defect 2: a childless `token_tree` vertex carrying the whole captured token
/// run as a `literal-value` (the supported by-construction encoding for an
/// opaque token tree, since tree-sitter leaves its anonymous `::` punctuation
/// with no CST vertex) must emit that literal verbatim, not rule-walk to a
/// bare `()`. Construct the encoding by collapsing a parsed token tree to an
/// opaque leaf, then check the captured text survives emit and re-parse.
#[test]
fn opaque_token_tree_leaf_emits_verbatim() {
    with_big_stack(|| {
        let reg = registry();
        let src = b"#[allow(clippy::module_inception)]\npub mod m;\n";
        let parsed = reg.parse_with_protocol("rust", src, "b.rs").expect("parse");

        // The lint paren group: a `token_tree` with `identifier` children.
        let tt_id = parsed
            .vertices
            .values()
            .find(|v| {
                v.kind.as_ref() == "token_tree"
                    && parsed.outgoing_edges(&v.id.to_string()).iter().any(|e| {
                        parsed
                            .vertices
                            .get(&e.tgt)
                            .is_some_and(|c| c.kind.as_ref() == "identifier")
                    })
            })
            .map(|v| v.id.clone())
            .expect("a token_tree with identifier children");

        // Collapse it to the opaque-leaf encoding: no children, one
        // literal-value carrying the whole run (including the `::`).
        let mut s = parsed.clone();
        let child_ids: Vec<_> = s
            .outgoing_edges(&tt_id.to_string())
            .iter()
            .map(|e| e.tgt.clone())
            .collect();
        s.outgoing.remove(&tt_id);
        s.edges.retain(|e, _| e.src != tt_id);
        for c in &child_ids {
            s.vertices.remove(c);
            s.outgoing.remove(c);
            s.incoming.remove(c);
            s.constraints.remove(c);
            s.edges.retain(|e, _| &e.src != c && &e.tgt != c);
        }
        s.constraints.insert(
            tt_id.clone(),
            vec![Constraint {
                sort: "literal-value".into(),
                value: "(clippy::module_inception)".into(),
            }],
        );
        s.forget_layout_in_place();

        let bytes = reg
            .emit_pretty_with_protocol("rust", &s)
            .expect("emit_pretty");
        let out = String::from_utf8_lossy(&bytes);
        assert!(
            out.contains("(clippy::module_inception)"),
            "opaque token tree leaf was not emitted verbatim, got: {out:?}"
        );
        // And the result re-parses without error (no stray `()` or dropped
        // `::` that would corrupt the attribute).
        let reparsed = reg
            .parse_with_protocol("rust", &bytes, "out.rs")
            .expect("re-parse");
        assert!(
            !reparsed
                .vertices
                .values()
                .any(|v| v.kind.as_ref().contains("ERROR")),
            "emitted attribute did not re-parse cleanly: {out:?}"
        );
        assert_eq!(
            reparsed
                .vertices
                .values()
                .filter(|v| v.kind.as_ref() == "token_tree")
                .count(),
            1,
            "the token tree did not survive emit + re-parse: {out:?}"
        );
    });
}

/// Defect 3: `blank-lines-before` is pure layout (blank lines carry no AST
/// structure), so `forget_layout` must strip it — the abstract surface may
/// not advertise a sort the emitter does not consume.
#[test]
fn forget_layout_strips_blank_lines_before() {
    with_big_stack(|| {
        let reg = registry();
        let src = b"pub mod a;\n\n\npub mod b;\n";
        let parsed = reg.parse_with_protocol("rust", src, "c.rs").expect("parse");
        let recorded = parsed
            .constraints
            .values()
            .flatten()
            .filter(|c| c.sort.as_ref() == "blank-lines-before")
            .count();
        assert!(
            recorded > 0,
            "expected the walker to record blank-lines-before on the spaced items"
        );
        let mut abs = parsed.clone();
        abs.forget_layout_in_place();
        let remaining = abs
            .constraints
            .values()
            .flatten()
            .filter(|c| c.sort.as_ref() == "blank-lines-before")
            .count();
        assert_eq!(
            remaining, 0,
            "forget_layout left {remaining} blank-lines-before constraint(s) on the abstract surface"
        );
    });
}
