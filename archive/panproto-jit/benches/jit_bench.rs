//! JIT classification benchmarks over realistic migration field-transform expressions.

#![allow(clippy::expect_used)]

use std::sync::Arc;

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_jit::mapping::classify_expr;

fn main() {
    divan::main();
}

/// A realistic field-transform expression: project `record.text` from an
/// `app.bsky.feed.post` record, then concatenate with a prefix.
fn bsky_post_display_expr() -> Expr {
    Expr::lam(
        "record",
        Expr::builtin(
            BuiltinOp::Concat,
            vec![
                Expr::Lit(Literal::Str("[bsky] ".into())),
                Expr::Field(Box::new(Expr::var("record")), Arc::from("text")),
            ],
        ),
    )
}

/// A realistic filter predicate: posts whose `langs` list contains `"en"`.
fn bsky_lang_filter_expr() -> Expr {
    Expr::lam(
        "p",
        Expr::builtin(
            BuiltinOp::Contains,
            vec![
                Expr::Field(Box::new(Expr::var("p")), Arc::from("langs")),
                Expr::Lit(Literal::Str("en".into())),
            ],
        ),
    )
}

#[divan::bench]
fn classify_post_display_expr(bencher: divan::Bencher) {
    let e = bsky_post_display_expr();
    bencher.bench(|| classify_expr(&e));
}

#[divan::bench]
fn classify_lang_filter_expr(bencher: divan::Bencher) {
    let e = bsky_lang_filter_expr();
    bencher.bench(|| classify_expr(&e));
}
