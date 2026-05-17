//! Section-law smoke tests for the parse / decorate / emit lens.
//!
//! For every grammar a sample source survives:
//!
//! ```text
//! parse  ────▶  DecoratedSchema
//!   │             │
//!   │             ▼ forget_layout
//!   │           AbstractSchema  ◀── input to `decorate`
//!   │             │
//!   │             ▼ decorate(policy)
//!   │           DecoratedSchema'
//!   │             │
//!   ▼             ▼ forget_layout
//! kind_multiset(parse_input.forget_layout())
//!   ==
//! kind_multiset(forget_layout(decorate(forget_layout(parse_input))))
//! ```
//!
//! This is the section law (`forget_layout ∘ decorate = id` on
//! abstract content) up to kind-multiset equivalence — the standard
//! granularity, since the parser invents fresh vertex IDs and we do
//! not preserve them through the round-trip.

#![cfg(feature = "grammars")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_parse::{LayoutPolicy, ParserRegistry};
use panproto_schema::{DecoratedSchema, kind_multiset};

fn registry() -> ParserRegistry {
    ParserRegistry::new()
}

fn section_law_holds(protocol: &str, source: &[u8]) {
    let reg = registry();
    let parsed = reg
        .parse_with_protocol(protocol, source, &format!("section.{protocol}"))
        .unwrap_or_else(|e| panic!("parse failed for {protocol}: {e}"));
    let decorated_input = DecoratedSchema::from_schema(parsed);
    let abstract_input = decorated_input.forget_layout();

    let policy = LayoutPolicy::default();
    let redecorated = reg
        .decorate(protocol, &abstract_input, &policy)
        .unwrap_or_else(|e| panic!("decorate failed for {protocol}: {e}"));

    let abstract_output = redecorated.forget_layout();

    let lhs = kind_multiset(abstract_input.as_schema());
    let rhs = kind_multiset(abstract_output.as_schema());
    assert_eq!(
        lhs, rhs,
        "section law violated for {protocol}: abstract content drifted across decorate"
    );
}

fn with_big_stack(inner: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(inner)
        .expect("spawn worker")
        .join()
        .expect("worker panicked");
}

#[cfg(feature = "lang-json")]
#[test]
fn json_section_law() {
    with_big_stack(|| section_law_holds("json", br#"{"k": [1, 2, 3]}"#));
}

#[cfg(feature = "lang-lilypond")]
#[test]
fn lilypond_section_law_on_issue_example() {
    with_big_stack(|| section_law_holds("lilypond", b"{ c'4 d'4 }"));
}

#[cfg(feature = "lang-json")]
#[test]
fn parse_emit_protolens_constructible_for_json() {
    let reg = registry();
    let policy = LayoutPolicy::default();
    let p = reg
        .parse_emit_protolens("json", &policy)
        .expect("protolens construction");
    // The protolens names what it does.
    assert!(p.name.as_ref().starts_with("parse_emit/"));
}
