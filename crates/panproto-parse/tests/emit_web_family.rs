//! Coverage (M2, web family): assert the emit fixed-point law
//! `emit(parse(emit(s))) == emit(s)` on idiomatic source for the
//! web-frontend grammars. Passing protocols are promoted to
//! `VERIFIED_EMIT_PROTOCOLS` in the registry.

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

/// Parse → emit → reparse → emit; assert the two emissions are
/// byte-identical (the `emit_pretty` image is a fixed point).
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

#[test]
#[cfg(feature = "lang-html")]
fn html_emit_is_fixed_point() {
    assert_emit_fixed_point(
        "html",
        "html",
        b"<!DOCTYPE html>\n<html>\n<body>\n<p>Hi</p>\n</body>\n</html>\n",
    );
}

#[test]
#[cfg(feature = "lang-css")]
fn css_emit_is_fixed_point() {
    assert_emit_fixed_point("css", "css", b"body {\n  color: red;\n  margin: 0;\n}\n");
}

#[test]
#[cfg(feature = "lang-scss")]
#[ignore = "class-selector `.box` re-emits reordered as `box .`; selector dispatch defect"]
fn scss_emit_is_fixed_point() {
    assert_emit_fixed_point("scss", "scss", b"$c: red;\n.box {\n  color: $c;\n}\n");
}

#[test]
#[cfg(feature = "lang-less")]
#[ignore = "sample/parse defect under current less grammar"]
fn less_emit_is_fixed_point() {
    assert_emit_fixed_point("less", "less", b"@c: red;\n.box {\n  color: @c;\n}\n");
}

#[test]
#[cfg(feature = "lang-vue")]
fn vue_emit_is_fixed_point() {
    assert_emit_fixed_point("vue", "vue", b"<template>\n  <p>Hi</p>\n</template>\n");
}

#[test]
#[cfg(feature = "lang-svelte")]
fn svelte_emit_is_fixed_point() {
    assert_emit_fixed_point(
        "svelte",
        "svelte",
        b"<script>\n  let x = 1;\n</script>\n<p>{x}</p>\n",
    );
}

#[test]
#[cfg(feature = "lang-astro")]
#[ignore = "frontmatter `---` fence spacing defect"]
fn astro_emit_is_fixed_point() {
    assert_emit_fixed_point("astro", "astro", b"---\nconst x = 1;\n---\n<p>Hi</p>\n");
}

#[test]
#[cfg(feature = "lang-tsx")]
#[ignore = "JSX element re-emits with broken spacing (< div className = ...); JSX spacing defect"]
fn tsx_emit_is_fixed_point() {
    assert_emit_fixed_point(
        "tsx",
        "tsx",
        b"const App = () => <div className=\"a\">Hi</div>;\n",
    );
}
