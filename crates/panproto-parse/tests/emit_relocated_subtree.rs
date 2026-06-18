//! Regression (issue #202): a parsed subtree relocated into a new context
//! must not replay its stale byte span verbatim and concatenate onto its new
//! sibling.
//!
//! When a parsed `class_definition` is grafted onto a fresh schema beside a
//! sibling statement, the class keeps the `[start-byte, end-byte)` span it
//! had in its original source. The verbatim replay path tiled that stale span
//! perfectly and emitted the class as raw bytes, which bypasses the
//! inter-statement separator the relocated context needs: the class body's
//! last token ran straight into the next `def` (`…(1 + 2)def f():`), invalid
//! Python. The fix declines verbatim replay when a vertex's recorded span is
//! not contained by its parent's, falling back to the role-table walk that
//! inserts the grammar-default separator.
//!
//! This test reproduces the "relocated" condition minimally: it parses a real
//! module, then drops the module root's byte span so its children become
//! subtrees whose parent no longer contains them, exactly what a graft does.

#![cfg(all(feature = "grammars", feature = "lang-python"))]
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use panproto_parse::ParserRegistry;

fn with_big_stack<F: FnOnce() + Send + 'static>(inner: F) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(inner)
        .expect("spawn")
        .join()
        .expect("worker panicked");
}

#[test]
fn relocated_subtree_separates_from_sibling() {
    with_big_stack(|| {
        let reg = ParserRegistry::new();
        let src =
            b"class A:\n    def m(self):\n        return self.x.y(1 + 2)\n\ndef f():\n    return 2\n";
        let parsed = reg
            .parse_with_protocol("python", src, "src.py")
            .expect("parse");

        // Standalone round-trip stays byte-faithful (the consistent case the
        // fix must not disturb): every vertex's span nests in its parent's.
        let standalone = reg
            .emit_pretty_with_protocol("python", &parsed)
            .expect("emit standalone");
        let standalone = String::from_utf8_lossy(&standalone).into_owned();
        assert!(
            standalone.contains(")\n\ndef f") || standalone.contains(")\ndef f"),
            "standalone emit lost the class/def separator: {standalone:?}"
        );

        // Relocate: drop the root module's byte span. Its children keep their
        // own (now stale) spans, so the parent no longer contains them.
        let root = parsed
            .vertices
            .keys()
            .find(|id| parsed.incoming.get(*id).is_none_or(|e| e.is_empty()))
            .expect("a structural root")
            .clone();
        let mut relocated = parsed.clone();
        let cs = relocated
            .constraints
            .get_mut(&root)
            .expect("root constraints");
        cs.retain(|c| c.sort.as_ref() != "start-byte" && c.sort.as_ref() != "end-byte");

        let out = reg
            .emit_pretty_with_protocol("python", &relocated)
            .expect("emit relocated");
        let text = String::from_utf8_lossy(&out).into_owned();

        // The class must not run straight into the following def.
        assert!(
            !text.contains(")def"),
            "grafted class concatenated onto the next statement: {text:?}"
        );
        assert!(
            text.contains("def f"),
            "relocated def missing from output: {text:?}"
        );
        // A separator (newline) now precedes the top-level def.
        assert!(
            text.contains("\ndef f"),
            "no separator before the relocated def: {text:?}"
        );
    });
}
