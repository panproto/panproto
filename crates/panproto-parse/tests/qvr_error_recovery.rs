//! Error-recovery MISSING anonymous tokens surface as schema markers.
//!
//! When tree-sitter recovers from an incomplete construct by *inserting* a
//! zero-width MISSING token, and that token is anonymous (a `]`, `}`, `)`,
//! `,`, or keyword), the walker used to drop it silently: the recovered parse
//! carried no `ERROR` vertex and no zero-width vertex, so it was
//! indistinguishable from a complete parse and a downstream walker validating
//! input could not reject it. The walker now surfaces each such token as a
//! zero-width, `ERROR`-kinded marker vertex carrying a `missing` constraint.

#![cfg(all(feature = "grammars", feature = "lang-qvr"))]
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use panproto_parse::ParserRegistry;

#[test]
fn missing_anonymous_token_surfaces_zero_width_marker() {
    // The dropped `]` after `[role=latent` makes tree-sitter recover by
    // inserting a zero-width MISSING `]` (an anonymous token).
    let registry = ParserRegistry::new();
    let schema = registry
        .parse_with_protocol(
            "qvr",
            b"object State : FinSet 8\nmorphism t : State -> State [role=latent\n",
            "recovered.qvr",
        )
        .expect("qvr parser recovers from the dropped `]`");

    // The recovery is legible: a vertex carrying a `missing` constraint that
    // records the elided token.
    let marker = schema
        .vertices
        .keys()
        .find(|vid| {
            schema
                .constraints
                .get(*vid)
                .is_some_and(|cs| cs.iter().any(|c| &*c.sort == "missing"))
        })
        .expect("a dropped anonymous token must surface a `missing` marker vertex");

    let cs = schema
        .constraints
        .get(marker)
        .expect("marker has constraints");
    let get = |sort: &str| {
        cs.iter()
            .find(|c| &*c.sort == sort)
            .map(|c| c.value.clone())
    };
    // Zero-width span (start == end) is the signal downstream walkers key on;
    // the recorded token makes it distinguishable from a genuine ERROR subtree.
    assert_eq!(get("start-byte"), get("end-byte"), "marker is zero-width");
    assert_eq!(
        get("missing").as_deref(),
        Some("]"),
        "records the elided token"
    );
    // The qvr protocol admits the ERROR kind, so the marker is also caught by a
    // downstream `kind == "ERROR"` rejection, not only a zero-width check.
    assert_eq!(&*schema.vertices[marker].kind, "ERROR");
}

#[test]
fn complete_parse_has_no_missing_marker() {
    // The well-formed counterpart must NOT carry a recovery marker, so the
    // marker is a true positive rather than always-on noise.
    let registry = ParserRegistry::new();
    let schema = registry
        .parse_with_protocol(
            "qvr",
            b"object State : FinSet 8\nmorphism t : State -> State [role=latent]\n",
            "complete.qvr",
        )
        .expect("well-formed qvr parses");
    let has_marker = schema
        .constraints
        .values()
        .any(|cs| cs.iter().any(|c| &*c.sort == "missing"));
    assert!(!has_marker, "a complete parse carries no `missing` marker");
}
