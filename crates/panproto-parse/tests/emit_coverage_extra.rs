//! Coverage (M3 systems, M4 functional, M5 data, M6 markup): assert the
//! emit fixed-point law on idiomatic source. Passing protocols are
//! promoted to `VERIFIED_EMIT_PROTOCOLS`; protocols with a remaining
//! emit defect are marked `#[ignore]` with the observed symptom.

#![cfg(feature = "grammars")]
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

fn assert_emit_fixed_point(protocol: &'static str, ext: &'static str, src: &'static [u8]) {
    with_big_stack(move || {
        let reg = registry();
        let file = format!("sample.{ext}");
        let s1 = reg
            .parse_with_protocol(protocol, src, &file)
            .unwrap_or_else(|e| panic!("{protocol} parse failed: {e}"));
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
    });
}

// ── M5 data / config ────────────────────────────────────────────────
#[test]
#[cfg(feature = "lang-json")]
fn json_emit_is_fixed_point() {
    assert_emit_fixed_point("json", "json", b"{\n  \"a\": 1,\n  \"b\": [1, 2]\n}\n");
}

#[test]
#[cfg(feature = "lang-toml")]
fn toml_emit_is_fixed_point() {
    assert_emit_fixed_point("toml", "toml", b"key = \"value\"\n\n[section]\nn = 1\n");
}

#[test]
#[cfg(feature = "lang-yaml")]
fn yaml_emit_is_fixed_point() {
    assert_emit_fixed_point("yaml", "yaml", b"key: value\nlist:\n  - a\n  - b\n");
}

#[test]
#[cfg(feature = "lang-jsonnet")]
fn jsonnet_emit_is_fixed_point() {
    assert_emit_fixed_point("jsonnet", "jsonnet", b"{\n  a: 1,\n}\n");
}

#[test]
#[cfg(feature = "lang-nix")]
fn nix_emit_is_fixed_point() {
    assert_emit_fixed_point("nix", "nix", b"{\n  a = 1;\n}\n");
}

#[test]
#[cfg(feature = "lang-hcl")]
fn hcl_emit_is_fixed_point() {
    assert_emit_fixed_point("hcl", "hcl", b"a = 1\n\nblock {\n  b = 2\n}\n");
}

#[test]
#[cfg(feature = "lang-kdl")]
#[ignore = "slashdash /- comment marker leaks into node args"]
fn kdl_emit_is_fixed_point() {
    assert_emit_fixed_point("kdl", "kdl", b"node 1 2\n");
}

#[test]
#[cfg(feature = "lang-ron")]
#[ignore = "`(...)` struct wrapper lost; re-emit empty"]
fn ron_emit_is_fixed_point() {
    assert_emit_fixed_point("ron", "ron", b"(\n    a: 1,\n)\n");
}

// ── M3 systems ──────────────────────────────────────────────────────
#[test]
#[cfg(feature = "lang-zig")]
fn zig_emit_is_fixed_point() {
    assert_emit_fixed_point("zig", "zig", b"pub fn main() void {}\n");
}

#[test]
#[cfg(feature = "lang-nim")]
fn nim_emit_is_fixed_point() {
    assert_emit_fixed_point("nim", "nim", b"proc f(): int =\n  result = 1\n");
}

#[test]
#[cfg(feature = "lang-d")]
#[ignore = "CHOICE dispatch (pick_choice) greedily picks optional template_parameters over BLANK for the initializer's int_literal; needs tree-sitter production-ID witnesses (plan §6)"]
fn d_emit_is_fixed_point() {
    assert_emit_fixed_point("d", "d", b"void main() {\n  int x = 1;\n}\n");
}

#[test]
#[cfg(feature = "lang-ada")]
fn ada_emit_is_fixed_point() {
    assert_emit_fixed_point("ada", "adb", b"procedure P is\nbegin\n   null;\nend P;\n");
}

#[test]
#[cfg(feature = "lang-fortran")]
fn fortran_emit_is_fixed_point() {
    assert_emit_fixed_point(
        "fortran",
        "f90",
        b"program p\n  integer :: x\nend program p\n",
    );
}

#[test]
#[cfg(feature = "lang-hare")]
fn hare_emit_is_fixed_point() {
    assert_emit_fixed_point("hare", "ha", b"fn main() void = {\n};\n");
}

#[test]
#[cfg(feature = "lang-odin")]
#[ignore = "trailing `;` inserted after `package main` on re-emit"]
fn odin_emit_is_fixed_point() {
    assert_emit_fixed_point("odin", "odin", b"package main\n\nmain :: proc() {\n}\n");
}

// ── M4 functional ───────────────────────────────────────────────────
#[test]
#[cfg(feature = "lang-haskell")]
fn haskell_emit_is_fixed_point() {
    assert_emit_fixed_point("haskell", "hs", b"main :: IO ()\nmain = putStrLn \"hi\"\n");
}

#[test]
#[cfg(feature = "lang-ocaml")]
fn ocaml_emit_is_fixed_point() {
    assert_emit_fixed_point("ocaml", "ml", b"let x = 1\n");
}

#[test]
#[cfg(feature = "lang-fsharp")]
fn fsharp_emit_is_fixed_point() {
    assert_emit_fixed_point("fsharp", "fs", b"let x = 1\n");
}

#[test]
#[cfg(feature = "lang-elm")]
#[ignore = "indent-based module body collapses to one line; structure lost on re-emit"]
fn elm_emit_is_fixed_point() {
    assert_emit_fixed_point("elm", "elm", b"module M exposing (..)\n\n\nx =\n    1\n");
}

#[test]
#[cfg(feature = "lang-purescript")]
#[ignore = "type signature `x :: Int` re-emits as `x :: ;` (annotation dropped)"]
fn purescript_emit_is_fixed_point() {
    assert_emit_fixed_point("purescript", "purs", b"module M where\n\nx :: Int\nx = 1\n");
}

#[test]
#[cfg(feature = "lang-lean")]
#[ignore = "`def x : Nat := 1` re-emits as `def x Nat:1` (`:`/`:=` mangled)"]
fn lean_emit_is_fixed_point() {
    assert_emit_fixed_point("lean", "lean", b"def x : Nat := 1\n");
}

// ── M6 markup ───────────────────────────────────────────────────────
#[test]
#[cfg(feature = "lang-markdown")]
fn markdown_emit_is_fixed_point() {
    assert_emit_fixed_point("markdown", "md", b"# Title\n\nA paragraph.\n");
}

#[test]
#[cfg(feature = "lang-typst")]
#[ignore = "markup close marker `]` leaks into emitted content"]
fn typst_emit_is_fixed_point() {
    assert_emit_fixed_point("typst", "typ", b"= Heading\n\nText.\n");
}

#[test]
#[cfg(feature = "lang-latex")]
fn latex_emit_is_fixed_point() {
    assert_emit_fixed_point(
        "latex",
        "tex",
        b"\\documentclass{article}\n\\begin{document}\nHi\n\\end{document}\n",
    );
}
