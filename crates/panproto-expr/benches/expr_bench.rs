//! Benchmarks for expression evaluation.

use panproto_expr::{BuiltinOp, Env, EvalConfig, Expr, Literal, eval};

#[allow(clippy::cast_possible_wrap)]
fn make_sum_expr(n: usize) -> Expr {
    // fold([1..n], 0, λacc. λx. add(acc, x))
    let items: Vec<Expr> = (1..=n).map(|i| Expr::Lit(Literal::Int(i as i64))).collect();
    Expr::builtin(
        BuiltinOp::Fold,
        vec![
            Expr::List(items),
            Expr::Lit(Literal::Int(0)),
            Expr::lam(
                "acc",
                Expr::lam(
                    "x",
                    Expr::builtin(BuiltinOp::Add, vec![Expr::var("acc"), Expr::var("x")]),
                ),
            ),
        ],
    )
}

#[divan::bench(args = [10, 100, 1000])]
fn eval_fold_sum(bencher: divan::Bencher, n: usize) {
    let expr = make_sum_expr(n);
    let env = Env::new();
    let config = EvalConfig::default();
    bencher.bench(|| eval(&expr, &env, &config));
}

#[allow(clippy::cast_possible_wrap)]
fn make_map_expr(n: usize) -> Expr {
    let items: Vec<Expr> = (0..n).map(|i| Expr::Lit(Literal::Int(i as i64))).collect();
    Expr::builtin(
        BuiltinOp::Map,
        vec![
            Expr::List(items),
            Expr::lam(
                "x",
                Expr::builtin(
                    BuiltinOp::Mul,
                    vec![Expr::var("x"), Expr::Lit(Literal::Int(2))],
                ),
            ),
        ],
    )
}

#[divan::bench(args = [10, 100, 1000])]
fn eval_map(bencher: divan::Bencher, n: usize) {
    let expr = make_map_expr(n);
    let env = Env::new();
    let config = EvalConfig::default();
    bencher.bench(|| eval(&expr, &env, &config));
}

fn main() {
    divan::main();
}
