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
    ("properties", "properties", "key=value\n"),
    // Alternation/character-class newline terminator (`\r|\r\n|\n`, `[\r\n]`):
    // recognised as a structural line break instead of a `_` placeholder that
    // re-parses into a phantom field and grows.
    ("csv", "csv", "a,b,c\n1,2,3\n"),
    ("tsv", "tsv", "a\tb\tc\n1\t2\t3\n"),
    // Expanded-sweep clean passers (faithful, structure-preserving on the
    // stronger oracle; only cosmetic whitespace differs from the input).
    ("actionscript", "as", "package {\n  class C {\n  }\n}\n"),
    ("agda", "agda", "module M where\n"),
    ("al", "al", "table 1 \"T\"\n{\n}\n"),
    ("asciidoc", "adoc", "= Title\n"),
    ("bass", "bass", "(def x 1)\n"),
    ("beancount", "beancount", "2020-01-01 open Assets:Cash\n"),
    ("desktop", "desktop", "[Desktop Entry]\nName=X\n"),
    ("devicetree", "dts", "/dts-v1/;\n"),
    ("elisp", "el", "(defun f (x) x)\n"),
    ("firrtl", "fir", "circuit C :\n"),
    ("func", "fc", "int f() {\n  return 0;\n}\n"),
    ("gleam", "gleam", "fn f() {\n  1\n}\n"),
    ("groovy", "groovy", "def x = 1\n"),
    ("heex", "heex", "<div>hi</div>\n"),
    ("idris", "idr", "module M\n"),
    ("ispc", "ispc", "void f() {\n}\n"),
    ("janet", "janet", "(def x 1)\n"),
    ("jq", "jq", ".a\n"),
    ("lua", "lua", "local x = 1\n"),
    ("luau", "luau", "local x = 1\n"),
    ("matlab", "m", "x = 1;\n"),
    ("netlinx", "axs", "PROGRAM_NAME='x'\n"),
    ("objc", "m", "int f() {\n  return 0;\n}\n"),
    ("pony", "pony", "actor Main\n"),
    ("postscript", "ps", "/x 1 def\n"),
    ("powershell", "ps1", "$x = 1\n"),
    ("qmldir", "qmldir", "module M\n"),
    ("r", "r", "x <- 1\n"),
    ("racket", "rkt", "(define x 1)\n"),
    ("rego", "rego", "package p\n"),
    ("rescript", "res", "let x = 1\n"),
    ("scala", "scala", "val x = 1\n"),
    ("sparql", "rq", "SELECT ?x WHERE {\n  ?x ?y ?z\n}\n"),
    ("squirrel", "nut", "local x = 1\n"),
    ("tablegen", "td", "def X;\n"),
    ("tcl", "tcl", "set x 1\n"),
    ("templ", "templ", "package main\n"),
    ("tmux", "tmux", "set -g x 1\n"),
    ("twig", "twig", "{{ x }}\n"),
    ("v", "v", "fn f() {\n}\n"),
    // Third sweep (unsampled tail + borderline re-confirms).
    ("apex", "cls", "public class C {\n}\n"),
    ("arduino", "ino", "void setup() {\n}\n"),
    ("blade", "blade.php", "<div>hi</div>\n"),
    ("caddy", "caddy", "example.com {\n}\n"),
    ("chatito", "chatito", "%[greet]\n    hi\n"),
    ("cooklang", "cook", "Add salt.\n"),
    ("corn", "corn", "{\n}\n"),
    ("cpon", "cpon", "{\n}\n"),
    ("cylc", "cylc", "[scheduling]\n"),
    ("dtd", "dtd", "<!ELEMENT note (to)>\n"),
    ("earthfile", "earth", "VERSION 0.7\n"),
    ("elixir", "ex", "defmodule M do\nend\n"),
    ("enforce", "c", "class C {\n}\n"),
    ("faust", "dsp", "process = _;\n"),
    ("gdscript", "gd", "func f():\n\tpass\n"),
    ("gstlaunch", "gst", "videotestsrc ! autovideosink\n"),
    ("hack", "hack", "function f(): void {\n}\n"),
    ("hlsl", "hlsl", "float4 f() : SV_TARGET {\n  return 0;\n}\n"),
    ("lilypond", "ly", "{ c d e f }\n"),
    ("linkerscript", "ld", "SECTIONS {\n}\n"),
    ("luadoc", "lua", "--- @param x\n"),
    ("mojo", "mojo", "fn f():\n    pass\n"),
    ("move", "move", "module 0x1::m {\n}\n"),
    ("nqc", "nqc", "task main() {\n}\n"),
    ("puppet", "pp", "class c {\n}\n"),
    ("qml", "qml", "Item {\n}\n"),
    ("query", "scm", "(call) @c\n"),
    ("re2c", "re2c", "/*!re2c */\n"),
    ("solidity", "sol", "contract C {\n}\n"),
    ("verilog", "v", "module m;\nendmodule\n"),
    // Dispatch fixes (keyword-CHOICE-led SEQ skippable in accepts_first_edge).
    ("kotlin", "kt", "fun f(): Int {\n  return 1\n}\n"),
    // Fourth sweep (remaining tail).
    ("angular", "html", "<div>{{ x }}</div>\n"),
    ("batch", "bat", "@echo off\n"),
    ("chuck", "ck", "1 => int x;\n"),
    ("foam", "foam", "key value;\n"),
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
