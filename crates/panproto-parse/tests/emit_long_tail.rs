//! Coverage (M7, long tail): assert the emit fixed-point law on idiomatic
//! source across the long tail of vendored grammars, with a stronger oracle
//! than the byte fixed point alone.
//!
//! A protocol is admitted here only when, on its sample:
//!   1. `emit(parse(emit(s))) == emit(s)` (the byte fixed point / section law), and
//!   2. the parse round-trip preserves the **kind multiset** and **edge-shape
//!      multiset** of the schema — so emit dropped, reordered, or mangled no
//!      structural content.
//!
//! Criterion (2) is what distinguishes a genuine fixed point from a *degenerate*
//! one: an emitter that drops everything to `""` (or collapses content) still
//! satisfies (1) trivially, but fails (2). Protocols that pass both with
//! eyeball-clean output are listed in `VERIFIED_EMIT_PROTOCOLS`.
//!
//! Each case skips automatically when its grammar is not compiled into the
//! current build (so the file is exercised in full only under `--all-features`).

#![cfg(feature = "grammars")]
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use panproto_parse::{ParseError, ParserRegistry};
use panproto_schema::{edge_multiset, kind_multiset};

/// (protocol, extension, minimal idiomatic source).
///
/// Conservatively curated: every entry emits faithful, structure-preserving
/// output (only cosmetic whitespace may differ from the input). Protocols whose
/// emit drops/reorders/mangles tokens are deliberately excluded pending fixes.
const VERIFIED_SAMPLES: &[(&str, &str, &str)] = &[
    (
        "go",
        "go",
        "package main\n\nfunc f(x int) int {\n\treturn x + 1\n}\n",
    ),
    (
        "glsl",
        "glsl",
        "void main() {\n  gl_Position = vec4(0.0);\n}\n",
    ),
    ("starlark", "bzl", "x = 1\n"),
    ("pkl", "pkl", "x = 1\n"),
    ("editorconfig", "editorconfig", "root = true\n"),
    ("vim", "vim", "let x = 1\n"),
    ("git_config", "gitconfig", "[user]\n\tname = x\n"),
    ("dockerfile", "dockerfile", "FROM alpine\nRUN echo hi\n"),
    ("cmake", "cmake", "project(p)\n"),
    ("nginx", "conf", "server {\n}\n"),
    ("nickel", "ncl", "{ x = 1 }\n"),
    ("thrift", "thrift", "struct S {\n  1: i32 x\n}\n"),
    ("llvm", "ll", "define i32 @f() {\n  ret i32 0\n}\n"),
    ("gitcommit", "gitcommit", "subject line\n"),
    ("git_rebase", "git-rebase", "pick abc123 msg\n"),
    ("forth", "fth", ": square dup * ;\n"),
    ("wat", "wat", "(module)\n"),
    ("wast", "wast", "(module)\n"),
    ("bicep", "bicep", "param x int\n"),
    ("requirements", "txt", "flask==1.0\n"),
    ("ebnf", "ebnf", "rule = \"a\" ;\n"),
    ("ungrammar", "ungram", "Foo = 'a'\n"),
    ("org", "org", "* Heading\n\nText.\n"),
    ("asm", "asm", "mov eax, 1\n"),
    ("supercollider", "sc", "x = 1;\n"),
    ("capnp", "capnp", "struct S {\n  x @0 :Int32;\n}\n"),
    ("fidl", "fidl", "library l;\n"),
    ("smithy", "smithy", "namespace n\n"),
    ("graphql", "graphql", "type Query {\n  x: Int\n}\n"),
    ("textproto", "textproto", "name: \"x\"\n"),
    ("just", "just", "build:\n    echo hi\n"),
    // Leading-space-terminal class: a content terminal whose PATTERN absorbs
    // leading whitespace captures its own separator, so the emitter suppresses
    // the redundant layout space instead of accreting one per emit.
    ("ini", "ini", "[section]\nkey = value\n"),
    ("abc", "abc", "X:1\nT:Tune\nK:C\nCDEF|\n"),
];

fn with_big_stack<F: FnOnce() + Send + 'static>(inner: F) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(inner)
        .expect("spawn")
        .join()
        .expect("worker panicked");
}

/// Assert the strengthened emit law for one protocol. Skips silently if the
/// grammar is not compiled into this build.
fn assert_verified(protocol: &'static str, ext: &'static str, src: &'static [u8]) {
    with_big_stack(move || {
        let reg = ParserRegistry::new();
        let file = format!("sample.{ext}");
        let s1 = match reg.parse_with_protocol(protocol, src, &file) {
            Ok(s) => s,
            Err(ParseError::UnknownLanguage { .. }) => return, // grammar not compiled
            Err(e) => panic!("{protocol} parse failed: {e}"),
        };
        let e1 = reg
            .emit_pretty_with_protocol(protocol, &s1)
            .unwrap_or_else(|e| panic!("{protocol} emit1 failed: {e}"));
        let s2 = reg
            .parse_with_protocol(protocol, &e1, &file)
            .unwrap_or_else(|e| panic!("{protocol} reparse failed: {e}"));
        let e2 = reg
            .emit_pretty_with_protocol(protocol, &s2)
            .unwrap_or_else(|e| panic!("{protocol} emit2 failed: {e}"));

        let e1s = String::from_utf8_lossy(&e1).into_owned();
        let e2s = String::from_utf8_lossy(&e2).into_owned();
        assert_eq!(
            e1, e2,
            "{protocol} emit must be a fixed point.\ne1:\n{e1s}\ne2:\n{e2s}"
        );
        assert!(
            !s1.vertices.is_empty(),
            "{protocol} parsed to an empty schema (sample not exercising the grammar)"
        );
        assert_eq!(
            kind_multiset(&s1),
            kind_multiset(&s2),
            "{protocol} emit must preserve the vertex-kind multiset (no content dropped/mangled).\nemit:\n{e1s}"
        );
        assert_eq!(
            edge_multiset(&s1),
            edge_multiset(&s2),
            "{protocol} emit must preserve the edge-shape multiset (no structure dropped/reordered).\nemit:\n{e1s}"
        );
    });
}

#[test]
fn long_tail_emit_is_verified() {
    for (protocol, ext, src) in VERIFIED_SAMPLES {
        assert_verified(protocol, ext, src.as_bytes());
    }
}
