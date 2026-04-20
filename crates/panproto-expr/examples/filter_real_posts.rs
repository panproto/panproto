//! Evaluate a filter expression over real Bluesky post texts.

use panproto_expr::{BuiltinOp, Env, EvalConfig, Expr, Literal, eval};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Post texts taken verbatim from fixtures/atproto/records/post-{0..4}.json.
    let posts = vec![
        Literal::Str("Bluesky is looking for a design research contractor".into()),
        Literal::Str("big plus if you know Bluesky well".into()),
        Literal::Str(
            "who's the best design researcher you know? nominate them (or yourself)".into(),
        ),
    ];

    let expr = Expr::builtin(
        BuiltinOp::Filter,
        vec![
            Expr::List(posts.into_iter().map(Expr::Lit).collect()),
            Expr::lam(
                "s",
                Expr::builtin(
                    BuiltinOp::Contains,
                    vec![Expr::var("s"), Expr::Lit(Literal::Str("Bluesky".into()))],
                ),
            ),
        ],
    );

    let result = eval(&expr, &Env::new(), &EvalConfig::default())?;
    match result {
        Literal::List(items) => {
            println!("{} posts matched \"Bluesky\":", items.len());
            for it in items {
                println!("  - {it:?}");
            }
        }
        other => println!("unexpected: {other:?}"),
    }
    Ok(())
}
