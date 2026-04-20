//! Benchmarks for expression evaluation on realistic migration field-transform workloads.
//!
//! Expressions here model transforms that occur in real AT Protocol Lexicon
//! migrations: filtering a feed of real Bluesky post records by language,
//! projecting a record field, computing text length for constraint checks,
//! and concatenating display strings. Strings come from real posts in
//! `fixtures/atproto/records/`.

#![allow(clippy::expect_used)]

use std::sync::Arc;

use panproto_expr::{BuiltinOp, Env, EvalConfig, Expr, Literal, eval};

fn main() {
    divan::main();
}

// ---------------------------------------------------------------------------
// Realistic fixtures: strings lifted from real Bluesky post records
// ---------------------------------------------------------------------------

fn real_post_texts() -> Vec<Literal> {
    // Exact strings drawn from fixtures/atproto/records/post-{0..4}.json
    // (see fixtures/FIXTURES.md for source).
    vec![
        Literal::Str("Bluesky is looking for a design research contractor".into()),
        Literal::Str("en".into()),
        Literal::Str(
            "who's the best design researcher you know? nominate them (or yourself)".into(),
        ),
        Literal::Str("big plus if you know Bluesky well".into()),
        Literal::Str("2026-04-19T01:05:29.436Z".into()),
    ]
}

fn post_record() -> Literal {
    Literal::Record(vec![
        (
            Arc::from("text"),
            Literal::Str("Bluesky is looking for a design research contractor".into()),
        ),
        (
            Arc::from("createdAt"),
            Literal::Str("2026-04-19T01:05:29.436Z".into()),
        ),
        (Arc::from("langs"), Literal::List(vec![Literal::Str("en".into())])),
    ])
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

/// Count graphemes in a real post's text. Models the `maxGraphemes: 300`
/// constraint check from `app.bsky.feed.post`.
#[divan::bench]
fn len_real_post_text(bencher: divan::Bencher) {
    let text = Literal::Str(
        "Bluesky is looking for a design research contractor\n\nbig plus if you know Bluesky well\n\nwho's the best design researcher you know? nominate them (or yourself)"
            .into(),
    );
    let expr = Expr::builtin(BuiltinOp::Len, vec![Expr::Lit(text)]);
    let env = Env::new();
    let config = EvalConfig::default();
    bencher.bench(|| eval(&expr, &env, &config));
}

/// Filter a list of post-text strings for ones containing "Bluesky" —
/// a realistic content-moderation-style projection.
#[divan::bench]
fn filter_posts_by_substring(bencher: divan::Bencher) {
    let items: Vec<Expr> = real_post_texts().into_iter().map(Expr::Lit).collect();
    let expr = Expr::builtin(
        BuiltinOp::Filter,
        vec![
            Expr::List(items),
            Expr::lam(
                "s",
                Expr::builtin(
                    BuiltinOp::Contains,
                    vec![Expr::var("s"), Expr::Lit(Literal::Str("Bluesky".into()))],
                ),
            ),
        ],
    );
    let env = Env::new();
    let config = EvalConfig::default();
    bencher.bench(|| eval(&expr, &env, &config));
}

/// Map over a list of real post texts, prefixing each with a quote
/// attribution. Models a display-projection migration (e.g., feed view).
#[divan::bench]
fn map_prefix_posts(bencher: divan::Bencher) {
    let items: Vec<Expr> = real_post_texts().into_iter().map(Expr::Lit).collect();
    let expr = Expr::builtin(
        BuiltinOp::Map,
        vec![
            Expr::List(items),
            Expr::lam(
                "s",
                Expr::builtin(
                    BuiltinOp::Concat,
                    vec![Expr::Lit(Literal::Str("> ".into())), Expr::var("s")],
                ),
            ),
        ],
    );
    let env = Env::new();
    let config = EvalConfig::default();
    bencher.bench(|| eval(&expr, &env, &config));
}

/// Project `record.text` from a real post-shaped record literal.
/// Models a lens `get` projecting a single field.
#[divan::bench]
fn project_record_text_field(bencher: divan::Bencher) {
    let expr = Expr::Field(Box::new(Expr::Lit(post_record())), Arc::from("text"));
    let env = Env::new();
    let config = EvalConfig::default();
    bencher.bench(|| eval(&expr, &env, &config));
}
