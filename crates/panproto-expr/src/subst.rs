//! Substitution and free-variable analysis for expressions.

use std::sync::Arc;

use rustc_hash::FxHashSet;

use crate::{Expr, Pattern};

/// Collect all free variables in an expression.
#[must_use]
pub fn free_vars(expr: &Expr) -> FxHashSet<Arc<str>> {
    let mut vars = FxHashSet::default();
    collect_free(expr, &mut FxHashSet::default(), &mut vars);
    vars
}

fn collect_free(expr: &Expr, bound: &mut FxHashSet<Arc<str>>, free: &mut FxHashSet<Arc<str>>) {
    match expr {
        Expr::Var(name) => {
            if !bound.contains(name) {
                free.insert(Arc::clone(name));
            }
        }
        Expr::Lam(param, body) => {
            let was_bound = bound.insert(Arc::clone(param));
            collect_free(body, bound, free);
            if !was_bound {
                bound.remove(param);
            }
        }
        Expr::App(func, arg) => {
            collect_free(func, bound, free);
            collect_free(arg, bound, free);
        }
        Expr::Lit(_) => {}
        Expr::Record(fields) => {
            for (_, v) in fields {
                collect_free(v, bound, free);
            }
        }
        Expr::List(items) => {
            for item in items {
                collect_free(item, bound, free);
            }
        }
        Expr::Field(expr, _) => collect_free(expr, bound, free),
        Expr::Index(expr, idx) => {
            collect_free(expr, bound, free);
            collect_free(idx, bound, free);
        }
        Expr::Match { scrutinee, arms } => {
            collect_free(scrutinee, bound, free);
            for (pat, body) in arms {
                let pat_vars = pattern_vars(pat);
                let mut inserted = Vec::new();
                for v in &pat_vars {
                    if bound.insert(Arc::clone(v)) {
                        inserted.push(Arc::clone(v));
                    }
                }
                collect_free(body, bound, free);
                for v in &inserted {
                    bound.remove(v);
                }
            }
        }
        Expr::Let { name, value, body } => {
            collect_free(value, bound, free);
            let was_bound = bound.insert(Arc::clone(name));
            collect_free(body, bound, free);
            if !was_bound {
                bound.remove(name);
            }
        }
        Expr::Builtin(_, args) => {
            for arg in args {
                collect_free(arg, bound, free);
            }
        }
    }
}

/// Collect all variable names bound by a pattern.
#[must_use]
pub fn pattern_vars(pat: &Pattern) -> Vec<Arc<str>> {
    let mut vars = Vec::new();
    collect_pattern_vars(pat, &mut vars);
    vars
}

fn collect_pattern_vars(pat: &Pattern, vars: &mut Vec<Arc<str>>) {
    match pat {
        Pattern::Wildcard | Pattern::Lit(_) => {}
        Pattern::Var(name) => vars.push(Arc::clone(name)),
        Pattern::Record(fields) => {
            for (_, p) in fields {
                collect_pattern_vars(p, vars);
            }
        }
        Pattern::List(items) => {
            for p in items {
                collect_pattern_vars(p, vars);
            }
        }
        Pattern::Constructor(_, args) => {
            for p in args {
                collect_pattern_vars(p, vars);
            }
        }
    }
}

/// Apply capture-avoiding substitution: replace `name` with `replacement` in `expr`.
#[must_use]
pub fn substitute(expr: &Expr, name: &str, replacement: &Expr) -> Expr {
    match expr {
        Expr::Var(v) => {
            if &**v == name {
                replacement.clone()
            } else {
                expr.clone()
            }
        }
        Expr::Lam(param, body) => {
            if &**param == name {
                // param shadows the substitution target — no change
                expr.clone()
            } else if free_vars(replacement).contains(param) {
                // Would capture — alpha-rename the param first
                let fresh = fresh_name(param, &free_vars(replacement));
                let renamed_body = substitute(body, param, &Expr::Var(Arc::clone(&fresh)));
                Expr::Lam(
                    fresh,
                    Box::new(substitute(&renamed_body, name, replacement)),
                )
            } else {
                Expr::Lam(
                    Arc::clone(param),
                    Box::new(substitute(body, name, replacement)),
                )
            }
        }
        Expr::App(func, arg) => Expr::App(
            Box::new(substitute(func, name, replacement)),
            Box::new(substitute(arg, name, replacement)),
        ),
        Expr::Lit(_) => expr.clone(),
        Expr::Record(fields) => Expr::Record(
            fields
                .iter()
                .map(|(k, v)| (Arc::clone(k), substitute(v, name, replacement)))
                .collect(),
        ),
        Expr::List(items) => Expr::List(
            items
                .iter()
                .map(|i| substitute(i, name, replacement))
                .collect(),
        ),
        Expr::Field(e, f) => Expr::Field(Box::new(substitute(e, name, replacement)), Arc::clone(f)),
        Expr::Index(e, idx) => Expr::Index(
            Box::new(substitute(e, name, replacement)),
            Box::new(substitute(idx, name, replacement)),
        ),
        Expr::Match { scrutinee, arms } => Expr::Match {
            scrutinee: Box::new(substitute(scrutinee, name, replacement)),
            arms: arms
                .iter()
                .map(|(pat, body)| {
                    let pvars = pattern_vars(pat);
                    if pvars.iter().any(|v| &**v == name) {
                        // pattern binds the substitution target — no change in body
                        (pat.clone(), body.clone())
                    } else {
                        (pat.clone(), substitute(body, name, replacement))
                    }
                })
                .collect(),
        },
        Expr::Let {
            name: let_name,
            value,
            body,
        } => {
            let new_value = substitute(value, name, replacement);
            if &**let_name == name {
                // let shadows the substitution target
                Expr::Let {
                    name: Arc::clone(let_name),
                    value: Box::new(new_value),
                    body: body.clone(),
                }
            } else {
                Expr::Let {
                    name: Arc::clone(let_name),
                    value: Box::new(new_value),
                    body: Box::new(substitute(body, name, replacement)),
                }
            }
        }
        Expr::Builtin(op, args) => Expr::Builtin(
            *op,
            args.iter()
                .map(|a| substitute(a, name, replacement))
                .collect(),
        ),
    }
}

/// Generate a fresh variable name by appending primes until it's not in `avoid`.
fn fresh_name(base: &str, avoid: &FxHashSet<Arc<str>>) -> Arc<str> {
    let mut candidate = format!("{base}'");
    while avoid.contains(candidate.as_str()) {
        candidate.push('\'');
    }
    Arc::from(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Literal;

    #[test]
    fn free_vars_simple() {
        // λx. add(x, y) — y is free, x is bound
        let expr = Expr::lam(
            "x",
            Expr::builtin(crate::BuiltinOp::Add, vec![Expr::var("x"), Expr::var("y")]),
        );
        let fv = free_vars(&expr);
        assert!(fv.contains("y"));
        assert!(!fv.contains("x"));
    }

    #[test]
    fn substitute_simple() {
        // add(x, 1) with x → 42 becomes add(42, 1)
        let expr = Expr::builtin(
            crate::BuiltinOp::Add,
            vec![Expr::var("x"), Expr::Lit(Literal::Int(1))],
        );
        let result = substitute(&expr, "x", &Expr::Lit(Literal::Int(42)));
        assert_eq!(
            result,
            Expr::builtin(
                crate::BuiltinOp::Add,
                vec![Expr::Lit(Literal::Int(42)), Expr::Lit(Literal::Int(1))],
            )
        );
    }

    #[test]
    fn substitute_avoids_capture() {
        // λy. add(x, y) with x → y should alpha-rename:
        // λy'. add(y, y')
        let expr = Expr::lam(
            "y",
            Expr::builtin(crate::BuiltinOp::Add, vec![Expr::var("x"), Expr::var("y")]),
        );
        let result = substitute(&expr, "x", &Expr::var("y"));
        // The lambda param should be renamed to avoid capture
        match &result {
            Expr::Lam(param, _) => assert_ne!(&**param, "y"),
            _ => panic!("expected Lam"),
        }
    }

    #[test]
    fn substitute_shadowed_by_let() {
        // let x = 1 in add(x, y) with x → 99
        // The value (1) contains no free occurrence of x, so it stays 1.
        // The body is shadowed by the let binding, so x stays as x.
        let expr = Expr::let_in(
            "x",
            Expr::Lit(Literal::Int(1)),
            Expr::builtin(crate::BuiltinOp::Add, vec![Expr::var("x"), Expr::var("y")]),
        );
        let result = substitute(&expr, "x", &Expr::Lit(Literal::Int(99)));
        match &result {
            Expr::Let { value, body, .. } => {
                // value is a literal 1, not a reference to x, so unchanged
                assert_eq!(**value, Expr::Lit(Literal::Int(1)));
                // body should still reference x (shadowed by let)
                assert!(
                    matches!(body.as_ref(), Expr::Builtin(_, args) if matches!(&args[0], Expr::Var(v) if &**v == "x"))
                );
            }
            _ => panic!("expected Let"),
        }
    }
}
