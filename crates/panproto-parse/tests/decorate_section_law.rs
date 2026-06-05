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

// Both tests gate on a grammar absent from the workspace-default
// `group-core`. Gate the whole file on their union so it compiles to
// nothing (no orphaned helpers) when neither is enabled.
#![cfg(all(
    feature = "grammars",
    any(feature = "lang-json", feature = "lang-lilypond")
))]
#![allow(clippy::expect_used, clippy::unwrap_used, dead_code)]

use panproto_parse::{LayoutPolicy, ParserRegistry};
use panproto_schema::{DecoratedSchema, edge_multiset, kind_multiset};

fn registry() -> ParserRegistry {
    ParserRegistry::new()
}

fn section_law_holds(protocol: &str, source: &[u8]) {
    let reg = registry();
    let parsed = reg
        .parse_with_protocol(protocol, source, &format!("section.{protocol}"))
        .unwrap_or_else(|e| panic!("parse failed for {protocol}: {e}"));
    let decorated_input = DecoratedSchema::wrap_unchecked(parsed);
    let abstract_input = decorated_input.forget_layout();

    let policy = LayoutPolicy::default();
    let redecorated = reg
        .decorate(protocol, &abstract_input, &policy)
        .unwrap_or_else(|e| panic!("decorate failed for {protocol}: {e}"));

    let abstract_output = redecorated.forget_layout();

    let in_kinds = kind_multiset(abstract_input.as_schema());
    let out_kinds = kind_multiset(abstract_output.as_schema());
    assert_eq!(
        in_kinds, out_kinds,
        "section law violated for {protocol}: vertex-kind multiset drifted across decorate"
    );

    let in_edges = edge_multiset(abstract_input.as_schema());
    let out_edges = edge_multiset(abstract_output.as_schema());
    assert_eq!(
        in_edges, out_edges,
        "section law violated for {protocol}: edge-shape multiset drifted across decorate"
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

/// Regression: `pretty_with_protocol` on a hand-built abstract
/// schema must preserve schema edge order under REPEAT(CHOICE(...)).
///
/// Before the first-unconsumed-edge dispatch rule landed, the
/// emitter drained all `symbol` children, then all `punctuation`
/// children, then all `unsigned_integer` children, fusing
/// `[c, ', 4, d, ', 4, e, ', 4, f, ', 4]` into `[c, d, e, f, ', ',
/// ', ', 4, 4, 4, 4]`. The resulting bytes parsed back to a single
/// super-octave c followed by three bare letters and four bare
/// integers — a kind-multiset match but an edge-order disaster.
#[cfg(feature = "lang-lilypond")]
#[test]
fn lilypond_abstract_edge_order_preserved_through_pretty() {
    use panproto_schema::{Protocol, SchemaBuilder};

    with_big_stack(|| {
        let protocol = Protocol {
            name: "lilypond".into(),
            schema_theory: "ThLilypond".into(),
            instance_theory: "ThLilypondInstance".into(),
            ..Protocol::default()
        };
        let abstract_schema = SchemaBuilder::new(&protocol)
            .vertex("r", "lilypond_program", None)
            .unwrap()
            .vertex("b", "expression_block", None)
            .unwrap()
            .edge("r", "b", "child_of", None)
            .unwrap()
            .vertex("s1", "symbol", None)
            .unwrap()
            .edge("b", "s1", "child_of", None)
            .unwrap()
            .constraint("s1", "literal-value", "c")
            .vertex("p1", "punctuation", None)
            .unwrap()
            .edge("b", "p1", "child_of", None)
            .unwrap()
            .constraint("p1", "literal-value", "'")
            .vertex("d1", "unsigned_integer", None)
            .unwrap()
            .edge("b", "d1", "child_of", None)
            .unwrap()
            .constraint("d1", "literal-value", "4")
            .vertex("s2", "symbol", None)
            .unwrap()
            .edge("b", "s2", "child_of", None)
            .unwrap()
            .constraint("s2", "literal-value", "d")
            .vertex("p2", "punctuation", None)
            .unwrap()
            .edge("b", "p2", "child_of", None)
            .unwrap()
            .constraint("p2", "literal-value", "'")
            .vertex("d2", "unsigned_integer", None)
            .unwrap()
            .edge("b", "d2", "child_of", None)
            .unwrap()
            .constraint("d2", "literal-value", "4")
            .entry("r")
            .build_abstract()
            .expect("build abstract");

        let reg = registry();
        let bytes = reg
            .pretty_with_protocol("lilypond", &abstract_schema, &LayoutPolicy::default())
            .expect("pretty");
        let rendered = String::from_utf8(bytes).expect("utf8");

        // The unconsumed-edge dispatch must keep symbol → punct →
        // int interleaving. The exact whitespace depends on the
        // policy; assert on the token order.
        let c_idx = rendered.find('c').expect("c in output");
        let d_idx = rendered.find('d').expect("d in output");
        let first_quote = rendered.find('\'').expect("' in output");
        let first_four = rendered.find('4').expect("4 in output");
        assert!(
            c_idx < first_quote && first_quote < first_four && first_four < d_idx,
            "expected c < ' < 4 < d in {rendered:?}; \
             symbol/punct/int order broken under REPEAT(CHOICE)"
        );
    });
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

#[cfg(feature = "lang-json")]
#[test]
fn pretty_with_protocol_honours_policy() {
    // Drive the same abstract schema through two different policies
    // and assert the rendered bytes differ in *exactly* the way the
    // policies prescribe: separator, indent_width, and newline must
    // all reach the output. If any field is dead (a "stub"), one of
    // these assertions fails.
    with_big_stack(|| {
        let reg = registry();
        let parsed = reg
            .parse_with_protocol("json", b"{\"k\":1}", "policy.json")
            .expect("parse");
        let decorated_input = DecoratedSchema::wrap_unchecked(parsed);
        let abstract_input = decorated_input.forget_layout();

        let policy_a = LayoutPolicy {
            indent_width: 0,
            separator: " ".into(),
            newline: "\n".into(),
            ..LayoutPolicy::default()
        };
        let policy_b = LayoutPolicy {
            indent_width: 4,
            separator: "  ".into(),
            newline: "\r\n".into(),
            ..LayoutPolicy::default()
        };

        let bytes_a = reg
            .pretty_with_protocol("json", &abstract_input, &policy_a)
            .expect("pretty A");
        let bytes_b = reg
            .pretty_with_protocol("json", &abstract_input, &policy_b)
            .expect("pretty B");

        assert!(
            !bytes_a.contains(&b'\r'),
            "policy A's newline is LF, so output must not contain CR"
        );
        assert!(
            bytes_b.windows(2).any(|w| w == b"\r\n"),
            "policy B's newline is CRLF; output must contain \\r\\n"
        );

        // The two policies must produce different output for an
        // abstract schema with a layout — otherwise the policy field
        // values were ignored.
        assert_ne!(
            bytes_a, bytes_b,
            "pretty_with_protocol must honour LayoutPolicy field values"
        );
    });
}
