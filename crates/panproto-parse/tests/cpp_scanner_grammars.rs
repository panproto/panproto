//! Grammars with a C++ external scanner (`scanner.cc`) must compile,
//! link, and parse. The scanner is compiled into a per-grammar namespace
//! wrapper; that wrapper must hoist the scanner's system includes to
//! global scope (so libc++ headers are not pulled into the namespace),
//! and the parser archive must be exempt from symbol localization (its
//! external-scanner references live in the separate scanner archive).
//! Both are build-script concerns; this test is the end-to-end witness
//! that the resulting grammars actually parse.

#![cfg(all(feature = "lang-mojo", feature = "lang-norg", feature = "lang-wolfram"))]
#![allow(clippy::expect_used)]

use panproto_parse::ParserRegistry;

#[test]
fn cpp_scanner_grammars_parse() {
    let reg = ParserRegistry::new();
    for (protocol, source, file) in [
        ("mojo", "fn main():\n    pass\n", "m.mojo"),
        ("norg", "* heading\n", "n.norg"),
        ("wolfram", "f[x_] := x^2\n", "w.wl"),
    ] {
        let schema = reg
            .parse_with_protocol(protocol, source.as_bytes(), file)
            .unwrap_or_else(|e| panic!("{protocol} parse failed: {e}"));
        assert!(
            !schema.vertices.is_empty(),
            "{protocol} produced an empty schema"
        );
    }
}
