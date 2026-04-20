//! Classify the JIT compilation shape of a realistic AT Proto field transform.

use std::sync::Arc;

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_jit::mapping::classify_expr;

fn main() {
    let expr = Expr::lam(
        "record",
        Expr::builtin(
            BuiltinOp::Concat,
            vec![
                Expr::Lit(Literal::Str("[bsky] ".into())),
                Expr::Field(Box::new(Expr::var("record")), Arc::from("text")),
            ],
        ),
    );
    let mapping = classify_expr(&expr);
    println!("classified bsky post display-projection expr: {mapping:?}");
}
