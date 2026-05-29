//! Regression: a Python function body (suite) must emit as a fixed point
//! with its statements indented inside the function.
//!
//! Python suites carry no bracket delimiter — indentation is the only
//! block marker, delivered by external `_indent`/`_dedent` tokens. If the
//! emitter neither selects the indent-bearing alternative nor synthesizes
//! an indent scope, body statements land at column 0 (structurally
//! outside the function), and a body separator emitted as `;` on the
//! first pass becomes `\n` on the re-parse — so emit is not a fixed point.

#![cfg(all(feature = "grammars", feature = "lang-python"))]
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use panproto_parse::ParserRegistry;

fn registry() -> ParserRegistry {
    ParserRegistry::new()
}

fn with_big_stack<F: FnOnce() + Send + 'static>(inner: F) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(inner)
        .expect("spawn")
        .join()
        .expect("worker panicked");
}

#[test]
fn python_function_body_is_fixed_point_and_indented() {
    with_big_stack(|| {
        let reg = registry();
        let src = b"def f():\n    x = 1\n    return x\n";
        let s1 = reg
            .parse_with_protocol("python", src, "x.py")
            .expect("parse");
        let e1 = reg.emit_pretty_with_protocol("python", &s1).expect("emit1");
        let s2 = reg
            .parse_with_protocol("python", &e1, "x.py")
            .expect("reparse");
        let e2 = reg.emit_pretty_with_protocol("python", &s2).expect("emit2");
        let e1s = String::from_utf8_lossy(&e1).into_owned();
        let e2s = String::from_utf8_lossy(&e2).into_owned();

        assert_eq!(
            e1, e2,
            "Python suite emit must be a fixed point.\ne1:\n{e1s}\ne2:\n{e2s}"
        );

        // No semicolon-joined statements.
        assert!(
            !e1s.contains(';'),
            "function body statements must not be joined with ';': {e1s:?}"
        );

        // The empty parameter list hugs the function name (`f()`, not
        // `f ()`).
        assert!(
            e1s.contains("f()"),
            "empty parameter list must stay tight against the name: {e1s:?}"
        );

        // The body must be indented: the `return` line sits inside the
        // function, not at column 0.
        let return_line = e1s
            .lines()
            .find(|l| l.contains("return"))
            .expect("emit must contain the return statement");
        assert!(
            return_line.starts_with(char::is_whitespace),
            "return statement must be indented inside the function body: {e1s:?}"
        );

        // Both body statements survive structurally on re-parse.
        let return_count = s2
            .vertices
            .values()
            .filter(|v| v.kind.as_ref() == "return_statement")
            .count();
        assert_eq!(
            return_count, 1,
            "re-parsed schema must keep one return_statement:\n{e1s}"
        );
    });
}
