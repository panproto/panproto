//! Call-by-value expression evaluator with step and depth limits.
//!
//! The evaluator is pure, deterministic, and WASM-safe. It tracks a step
//! counter (decremented on each reduction) and a depth counter (incremented
//! on each recursive call) to bound computation.

use std::sync::Arc;

use crate::builtin::apply_builtin;
use crate::env::Env;
use crate::error::ExprError;
use crate::expr::{BuiltinOp, Expr, Pattern};
use crate::literal::Literal;

/// Configuration for the expression evaluator.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EvalConfig {
    /// Maximum number of reduction steps before aborting.
    pub max_steps: u64,
    /// Maximum recursion depth before aborting.
    pub max_depth: u32,
    /// Maximum list length for operations that produce lists.
    pub max_list_len: usize,
}

impl Default for EvalConfig {
    fn default() -> Self {
        Self {
            max_steps: 100_000,
            max_depth: 256,
            max_list_len: 10_000,
        }
    }
}

/// Mutable evaluation state tracking resource consumption.
struct EvalState {
    steps_remaining: u64,
    max_steps: u64,
    max_depth: u32,
    max_list_len: usize,
}

impl EvalState {
    const fn new(config: &EvalConfig) -> Self {
        Self {
            steps_remaining: config.max_steps,
            max_steps: config.max_steps,
            max_depth: config.max_depth,
            max_list_len: config.max_list_len,
        }
    }

    const fn tick(&mut self) -> Result<(), ExprError> {
        if self.steps_remaining == 0 {
            return Err(ExprError::StepLimitExceeded(self.max_steps));
        }
        self.steps_remaining -= 1;
        Ok(())
    }
}

/// Evaluate an expression in the given environment.
///
/// # Errors
///
/// Returns [`ExprError`] on type mismatches, unbound variables,
/// step/depth limit exceeded, or runtime errors.
pub fn eval(expr: &Expr, env: &Env, config: &EvalConfig) -> Result<Literal, ExprError> {
    let mut state = EvalState::new(config);
    eval_inner(expr, env, 0, &mut state)
}

fn eval_inner(
    expr: &Expr,
    env: &Env,
    depth: u32,
    state: &mut EvalState,
) -> Result<Literal, ExprError> {
    if depth > state.max_depth {
        return Err(ExprError::DepthExceeded(state.max_depth));
    }
    state.tick()?;

    match expr {
        Expr::Var(name) => env
            .get(name)
            .cloned()
            .ok_or_else(|| ExprError::UnboundVariable(name.to_string())),

        Expr::Lit(lit) => Ok(lit.clone()),

        Expr::Lam(param, body) => {
            // Lambdas evaluate to closures, capturing the current environment.
            // This enables proper lexical scoping and first-class functions.
            let captured: Vec<(Arc<str>, Literal)> = env
                .iter()
                .map(|(k, v)| (Arc::clone(k), v.clone()))
                .collect();
            Ok(Literal::Closure {
                param: Arc::clone(param),
                body: body.clone(),
                env: captured,
            })
        }

        Expr::App(func, arg) => eval_app(func, arg, env, depth, state),

        Expr::Record(fields) => {
            let mut result = Vec::with_capacity(fields.len());
            for (name, expr) in fields {
                let val = eval_inner(expr, env, depth + 1, state)?;
                result.push((Arc::clone(name), val));
            }
            Ok(Literal::Record(result))
        }

        Expr::List(items) => {
            let mut result = Vec::with_capacity(items.len());
            for item in items {
                let val = eval_inner(item, env, depth + 1, state)?;
                result.push(val);
            }
            if result.len() > state.max_list_len {
                return Err(ExprError::ListLengthExceeded(result.len()));
            }
            Ok(Literal::List(result))
        }

        Expr::Field(expr, field) => {
            let val = eval_inner(expr, env, depth + 1, state)?;
            match &val {
                Literal::Record(fields) => fields
                    .iter()
                    .find(|(k, _)| k == field)
                    .map(|(_, v)| v.clone())
                    .ok_or_else(|| ExprError::FieldNotFound(field.to_string())),
                _ => Err(ExprError::TypeError {
                    expected: "record".into(),
                    got: val.type_name().into(),
                }),
            }
        }

        Expr::Index(expr, idx_expr) => eval_index(expr, idx_expr, env, depth, state),

        Expr::Match { scrutinee, arms } => eval_match(scrutinee, arms, env, depth, state),

        Expr::Let { name, value, body } => {
            let val = eval_inner(value, env, depth + 1, state)?;
            let new_env = env.extend(Arc::clone(name), val);
            eval_inner(body, &new_env, depth + 1, state)
        }

        Expr::Builtin(op, args) => {
            // Special handling for higher-order builtins (Map, Filter, Fold, FlatMap)
            match op {
                BuiltinOp::Map => eval_map(args, env, depth, state),
                BuiltinOp::Filter => eval_filter(args, env, depth, state),
                BuiltinOp::Fold => eval_fold(args, env, depth, state),
                BuiltinOp::FlatMap => eval_flat_map(args, env, depth, state),
                _ => {
                    let evaluated: Result<Vec<_>, _> = args
                        .iter()
                        .map(|a| eval_inner(a, env, depth + 1, state))
                        .collect();
                    apply_builtin(*op, &evaluated?)
                }
            }
        }
    }
}

/// Evaluate a function application.
///
/// Evaluates both the function and argument, then applies. The function
/// may be a structural `Lam` node (direct beta reduction) or may evaluate
/// to a `Literal::Closure` (proper closure application with captured env).
fn eval_app(
    func: &Expr,
    arg: &Expr,
    env: &Env,
    depth: u32,
    state: &mut EvalState,
) -> Result<Literal, ExprError> {
    // Evaluate the function expression to a value.
    let func_val = eval_inner(func, env, depth + 1, state)?;
    // Evaluate the argument.
    let arg_val = eval_inner(arg, env, depth + 1, state)?;
    // Apply the closure to the argument.
    apply_closure(&func_val, &arg_val, depth, state)
}

/// Apply a closure value to an argument value.
///
/// Reconstructs the captured environment, binds the parameter, and
/// evaluates the body. This is the formal beta-reduction step for
/// the call-by-value evaluation strategy.
fn apply_closure(
    func: &Literal,
    arg: &Literal,
    depth: u32,
    state: &mut EvalState,
) -> Result<Literal, ExprError> {
    match func {
        Literal::Closure { param, body, env } => {
            // Reconstruct the captured environment.
            let mut closure_env: Env = env
                .iter()
                .map(|(k, v)| (Arc::clone(k), v.clone()))
                .collect();
            // Bind the parameter to the argument.
            closure_env = closure_env.extend(Arc::clone(param), arg.clone());
            // Evaluate the body in the extended environment.
            eval_inner(body, &closure_env, depth + 1, state)
        }
        _ => Err(ExprError::NotAFunction),
    }
}

/// Evaluate an index expression: `expr[idx]`.
#[allow(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn eval_index(
    expr: &Expr,
    idx_expr: &Expr,
    env: &Env,
    depth: u32,
    state: &mut EvalState,
) -> Result<Literal, ExprError> {
    let val = eval_inner(expr, env, depth + 1, state)?;
    let idx = eval_inner(idx_expr, env, depth + 1, state)?;
    match (&val, &idx) {
        (Literal::List(items), Literal::Int(i)) => {
            let index = if *i < 0 {
                (items.len() as i64 + i) as usize
            } else {
                *i as usize
            };
            items
                .get(index)
                .cloned()
                .ok_or(ExprError::IndexOutOfBounds {
                    index: *i,
                    len: items.len(),
                })
        }
        _ => Err(ExprError::TypeError {
            expected: "(list, int)".into(),
            got: format!("({}, {})", val.type_name(), idx.type_name()),
        }),
    }
}

/// Evaluate a match expression.
fn eval_match(
    scrutinee: &Expr,
    arms: &[(Pattern, Expr)],
    env: &Env,
    depth: u32,
    state: &mut EvalState,
) -> Result<Literal, ExprError> {
    let val = eval_inner(scrutinee, env, depth + 1, state)?;
    for (pattern, body) in arms {
        if let Some(bindings) = match_pattern(pattern, &val) {
            let mut new_env = env.clone();
            for (name, bound_val) in bindings {
                new_env = new_env.extend(name, bound_val);
            }
            return eval_inner(body, &new_env, depth + 1, state);
        }
    }
    Err(ExprError::NonExhaustiveMatch)
}

/// Evaluate `map(list_expr, lambda_expr)`.
fn eval_map(
    args: &[Expr],
    env: &Env,
    depth: u32,
    state: &mut EvalState,
) -> Result<Literal, ExprError> {
    if args.len() != 2 {
        return Err(ExprError::ArityMismatch {
            op: "Map".into(),
            expected: 2,
            got: args.len(),
        });
    }
    let list_val = eval_inner(&args[0], env, depth + 1, state)?;
    let items = match list_val {
        Literal::List(items) => items,
        other => {
            return Err(ExprError::TypeError {
                expected: "list".into(),
                got: other.type_name().into(),
            });
        }
    };

    let func = &args[1];
    let mut result = Vec::with_capacity(items.len());
    for item in &items {
        let val = apply_lambda(func, item, env, depth + 1, state)?;
        result.push(val);
    }
    if result.len() > state.max_list_len {
        return Err(ExprError::ListLengthExceeded(result.len()));
    }
    Ok(Literal::List(result))
}

/// Evaluate `filter(list_expr, predicate_expr)`.
fn eval_filter(
    args: &[Expr],
    env: &Env,
    depth: u32,
    state: &mut EvalState,
) -> Result<Literal, ExprError> {
    if args.len() != 2 {
        return Err(ExprError::ArityMismatch {
            op: "Filter".into(),
            expected: 2,
            got: args.len(),
        });
    }
    let list_val = eval_inner(&args[0], env, depth + 1, state)?;
    let items = match list_val {
        Literal::List(items) => items,
        other => {
            return Err(ExprError::TypeError {
                expected: "list".into(),
                got: other.type_name().into(),
            });
        }
    };

    let pred = &args[1];
    let mut result = Vec::new();
    for item in &items {
        let keep = apply_lambda(pred, item, env, depth + 1, state)?;
        match keep {
            Literal::Bool(true) => result.push(item.clone()),
            Literal::Bool(false) => {}
            other => {
                return Err(ExprError::TypeError {
                    expected: "bool".into(),
                    got: other.type_name().into(),
                });
            }
        }
    }
    Ok(Literal::List(result))
}

/// Evaluate `fold(list_expr, init_expr, accumulator_expr)`.
fn eval_fold(
    args: &[Expr],
    env: &Env,
    depth: u32,
    state: &mut EvalState,
) -> Result<Literal, ExprError> {
    if args.len() != 3 {
        return Err(ExprError::ArityMismatch {
            op: "Fold".into(),
            expected: 3,
            got: args.len(),
        });
    }
    let list_val = eval_inner(&args[0], env, depth + 1, state)?;
    let items = match list_val {
        Literal::List(items) => items,
        other => {
            return Err(ExprError::TypeError {
                expected: "list".into(),
                got: other.type_name().into(),
            });
        }
    };

    let mut acc = eval_inner(&args[1], env, depth + 1, state)?;
    let func = &args[2];

    for item in &items {
        // func is a curried binary function: λacc. λitem. body
        // Apply it to acc, then to item.
        acc = apply_lambda_2(func, &acc, item, env, depth + 1, state)?;
    }
    Ok(acc)
}

/// Evaluate `flat_map(list_expr, lambda_expr)`.
fn eval_flat_map(
    args: &[Expr],
    env: &Env,
    depth: u32,
    state: &mut EvalState,
) -> Result<Literal, ExprError> {
    if args.len() != 2 {
        return Err(ExprError::ArityMismatch {
            op: "FlatMap".into(),
            expected: 2,
            got: args.len(),
        });
    }
    let list_val = eval_inner(&args[0], env, depth + 1, state)?;
    let items = match list_val {
        Literal::List(items) => items,
        other => {
            return Err(ExprError::TypeError {
                expected: "list".into(),
                got: other.type_name().into(),
            });
        }
    };

    let func = &args[1];
    let mut result = Vec::new();
    for item in &items {
        let sub_list = apply_lambda(func, item, env, depth + 1, state)?;
        match sub_list {
            Literal::List(sub_items) => result.extend(sub_items),
            other => {
                return Err(ExprError::TypeError {
                    expected: "list".into(),
                    got: other.type_name().into(),
                });
            }
        }
        if result.len() > state.max_list_len {
            return Err(ExprError::ListLengthExceeded(result.len()));
        }
    }
    Ok(Literal::List(result))
}

/// Evaluate a function expression and apply it to a single argument value.
///
/// The function expression is evaluated to produce a closure, then the
/// closure is applied to the argument via [`apply_closure`].
fn apply_lambda(
    func_expr: &Expr,
    arg: &Literal,
    env: &Env,
    depth: u32,
    state: &mut EvalState,
) -> Result<Literal, ExprError> {
    let func_val = eval_inner(func_expr, env, depth + 1, state)?;
    apply_closure(&func_val, arg, depth, state)
}

/// Evaluate a curried binary function and apply it to two arguments.
///
/// Evaluates `func_expr` to get a closure, applies to `arg1` to get
/// a second closure, then applies to `arg2`.
fn apply_lambda_2(
    func_expr: &Expr,
    arg1: &Literal,
    arg2: &Literal,
    env: &Env,
    depth: u32,
    state: &mut EvalState,
) -> Result<Literal, ExprError> {
    let func_val = eval_inner(func_expr, env, depth + 1, state)?;
    let partial = apply_closure(&func_val, arg1, depth, state)?;
    apply_closure(&partial, arg2, depth, state)
}

/// Try to match a value against a pattern, returning bindings on success.
fn match_pattern(pattern: &Pattern, value: &Literal) -> Option<Vec<(Arc<str>, Literal)>> {
    let mut bindings = Vec::new();
    if match_inner(pattern, value, &mut bindings) {
        Some(bindings)
    } else {
        None
    }
}

fn match_inner(
    pattern: &Pattern,
    value: &Literal,
    bindings: &mut Vec<(Arc<str>, Literal)>,
) -> bool {
    match pattern {
        Pattern::Wildcard => true,
        Pattern::Var(name) => {
            bindings.push((Arc::clone(name), value.clone()));
            true
        }
        Pattern::Lit(lit) => lit == value,
        Pattern::Record(field_pats) => {
            if let Literal::Record(fields) = value {
                for (pat_name, pat) in field_pats {
                    let field_val = fields.iter().find(|(k, _)| k == pat_name);
                    match field_val {
                        Some((_, v)) => {
                            if !match_inner(pat, v, bindings) {
                                return false;
                            }
                        }
                        None => return false,
                    }
                }
                true
            } else {
                false
            }
        }
        Pattern::List(item_pats) => {
            if let Literal::List(items) = value {
                if items.len() != item_pats.len() {
                    return false;
                }
                for (pat, val) in item_pats.iter().zip(items.iter()) {
                    if !match_inner(pat, val, bindings) {
                        return false;
                    }
                }
                true
            } else {
                false
            }
        }
        Pattern::Constructor(tag, arg_pats) => {
            // Constructors match against records with a "$tag" field
            if let Literal::Record(fields) = value {
                let tag_field = fields.iter().find(|(k, _)| &**k == "$tag");
                if let Some((_, Literal::Str(t))) = tag_field {
                    if t.as_str() != &**tag {
                        return false;
                    }
                    // Match remaining args against "$0", "$1", etc. fields
                    for (i, pat) in arg_pats.iter().enumerate() {
                        let key = format!("${i}");
                        let field_val = fields.iter().find(|(k, _)| k.as_ref() == key.as_str());
                        match field_val {
                            Some((_, v)) => {
                                if !match_inner(pat, v, bindings) {
                                    return false;
                                }
                            }
                            None => return false,
                        }
                    }
                    true
                } else {
                    false
                }
            } else {
                false
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn default_config() -> EvalConfig {
        EvalConfig::default()
    }

    #[test]
    fn eval_literal() {
        let result = eval(&Expr::Lit(Literal::Int(42)), &Env::new(), &default_config());
        assert_eq!(result.unwrap(), Literal::Int(42));
    }

    #[test]
    fn eval_variable() {
        let env = Env::new().extend(Arc::from("x"), Literal::Int(10));
        let result = eval(&Expr::var("x"), &env, &default_config());
        assert_eq!(result.unwrap(), Literal::Int(10));
    }

    #[test]
    fn eval_unbound_variable() {
        let result = eval(&Expr::var("x"), &Env::new(), &default_config());
        assert!(matches!(result, Err(ExprError::UnboundVariable(_))));
    }

    #[test]
    fn eval_lambda_application() {
        // (λx. add(x, 1))(41) = 42
        let expr = Expr::App(
            Box::new(Expr::lam(
                "x",
                Expr::builtin(
                    BuiltinOp::Add,
                    vec![Expr::var("x"), Expr::Lit(Literal::Int(1))],
                ),
            )),
            Box::new(Expr::Lit(Literal::Int(41))),
        );
        let result = eval(&expr, &Env::new(), &default_config());
        assert_eq!(result.unwrap(), Literal::Int(42));
    }

    #[test]
    fn eval_let_binding() {
        // let x = 10 in add(x, 5)
        let expr = Expr::let_in(
            "x",
            Expr::Lit(Literal::Int(10)),
            Expr::builtin(
                BuiltinOp::Add,
                vec![Expr::var("x"), Expr::Lit(Literal::Int(5))],
            ),
        );
        let result = eval(&expr, &Env::new(), &default_config());
        assert_eq!(result.unwrap(), Literal::Int(15));
    }

    #[test]
    fn eval_record_and_field() {
        let expr = Expr::field(
            Expr::Record(vec![
                (Arc::from("name"), Expr::Lit(Literal::Str("alice".into()))),
                (Arc::from("age"), Expr::Lit(Literal::Int(30))),
            ]),
            "age",
        );
        let result = eval(&expr, &Env::new(), &default_config());
        assert_eq!(result.unwrap(), Literal::Int(30));
    }

    #[test]
    fn eval_list_index() {
        let expr = Expr::Index(
            Box::new(Expr::List(vec![
                Expr::Lit(Literal::Int(10)),
                Expr::Lit(Literal::Int(20)),
                Expr::Lit(Literal::Int(30)),
            ])),
            Box::new(Expr::Lit(Literal::Int(1))),
        );
        let result = eval(&expr, &Env::new(), &default_config());
        assert_eq!(result.unwrap(), Literal::Int(20));
    }

    #[test]
    fn eval_pattern_match() {
        // match 42 { 0 => "zero", x => concat("num:", int_to_str(x)) }
        let expr = Expr::Match {
            scrutinee: Box::new(Expr::Lit(Literal::Int(42))),
            arms: vec![
                (
                    Pattern::Lit(Literal::Int(0)),
                    Expr::Lit(Literal::Str("zero".into())),
                ),
                (
                    Pattern::Var(Arc::from("x")),
                    Expr::builtin(
                        BuiltinOp::Concat,
                        vec![
                            Expr::Lit(Literal::Str("num:".into())),
                            Expr::builtin(BuiltinOp::IntToStr, vec![Expr::var("x")]),
                        ],
                    ),
                ),
            ],
        };
        let result = eval(&expr, &Env::new(), &default_config());
        assert_eq!(result.unwrap(), Literal::Str("num:42".into()));
    }

    #[test]
    fn eval_map() {
        // map([1, 2, 3], λx. mul(x, 2))
        let expr = Expr::builtin(
            BuiltinOp::Map,
            vec![
                Expr::List(vec![
                    Expr::Lit(Literal::Int(1)),
                    Expr::Lit(Literal::Int(2)),
                    Expr::Lit(Literal::Int(3)),
                ]),
                Expr::lam(
                    "x",
                    Expr::builtin(
                        BuiltinOp::Mul,
                        vec![Expr::var("x"), Expr::Lit(Literal::Int(2))],
                    ),
                ),
            ],
        );
        let result = eval(&expr, &Env::new(), &default_config());
        assert_eq!(
            result.unwrap(),
            Literal::List(vec![Literal::Int(2), Literal::Int(4), Literal::Int(6)])
        );
    }

    #[test]
    fn eval_filter() {
        // filter([1, 2, 3, 4], λx. gt(x, 2))
        let expr = Expr::builtin(
            BuiltinOp::Filter,
            vec![
                Expr::List(vec![
                    Expr::Lit(Literal::Int(1)),
                    Expr::Lit(Literal::Int(2)),
                    Expr::Lit(Literal::Int(3)),
                    Expr::Lit(Literal::Int(4)),
                ]),
                Expr::lam(
                    "x",
                    Expr::builtin(
                        BuiltinOp::Gt,
                        vec![Expr::var("x"), Expr::Lit(Literal::Int(2))],
                    ),
                ),
            ],
        );
        let result = eval(&expr, &Env::new(), &default_config());
        assert_eq!(
            result.unwrap(),
            Literal::List(vec![Literal::Int(3), Literal::Int(4)])
        );
    }

    #[test]
    fn eval_fold() {
        // fold([1, 2, 3], 0, λacc. λx. add(acc, x)) = 6
        let expr = Expr::builtin(
            BuiltinOp::Fold,
            vec![
                Expr::List(vec![
                    Expr::Lit(Literal::Int(1)),
                    Expr::Lit(Literal::Int(2)),
                    Expr::Lit(Literal::Int(3)),
                ]),
                Expr::Lit(Literal::Int(0)),
                Expr::lam(
                    "acc",
                    Expr::lam(
                        "x",
                        Expr::builtin(BuiltinOp::Add, vec![Expr::var("acc"), Expr::var("x")]),
                    ),
                ),
            ],
        );
        let result = eval(&expr, &Env::new(), &default_config());
        assert_eq!(result.unwrap(), Literal::Int(6));
    }

    #[test]
    fn eval_step_limit() {
        // A computation that exceeds the step limit
        let config = EvalConfig {
            max_steps: 5,
            ..EvalConfig::default()
        };
        // map([1,2,3,4,5,6,7,8,9,10], λx. add(x, 1)) — should exceed 5 steps
        let items: Vec<_> = (1..=10).map(|i| Expr::Lit(Literal::Int(i))).collect();
        let expr = Expr::builtin(
            BuiltinOp::Map,
            vec![
                Expr::List(items),
                Expr::lam(
                    "x",
                    Expr::builtin(
                        BuiltinOp::Add,
                        vec![Expr::var("x"), Expr::Lit(Literal::Int(1))],
                    ),
                ),
            ],
        );
        let result = eval(&expr, &Env::new(), &config);
        assert!(matches!(result, Err(ExprError::StepLimitExceeded(_))));
    }

    #[test]
    fn eval_merge_example() {
        // The merge example from the design doc:
        // λfirst. λlast. concat(first, concat(" ", last))
        let merge_fn = Expr::lam(
            "first",
            Expr::lam(
                "last",
                Expr::builtin(
                    BuiltinOp::Concat,
                    vec![
                        Expr::var("first"),
                        Expr::builtin(
                            BuiltinOp::Concat,
                            vec![Expr::Lit(Literal::Str(" ".into())), Expr::var("last")],
                        ),
                    ],
                ),
            ),
        );
        // Apply: merge_fn("Alice")("Smith")
        let expr = Expr::App(
            Box::new(Expr::App(
                Box::new(merge_fn),
                Box::new(Expr::Lit(Literal::Str("Alice".into()))),
            )),
            Box::new(Expr::Lit(Literal::Str("Smith".into()))),
        );
        let result = eval(&expr, &Env::new(), &default_config());
        assert_eq!(result.unwrap(), Literal::Str("Alice Smith".into()));
    }

    #[test]
    fn eval_split_example() {
        // The split example from the design doc:
        // λfull. let parts = split(full, " ") in
        //   { firstName: head(parts), lastName: join(tail(parts), " ") }
        let split_fn = Expr::lam(
            "full",
            Expr::let_in(
                "parts",
                Expr::builtin(
                    BuiltinOp::Split,
                    vec![Expr::var("full"), Expr::Lit(Literal::Str(" ".into()))],
                ),
                Expr::Record(vec![
                    (
                        Arc::from("firstName"),
                        Expr::builtin(BuiltinOp::Head, vec![Expr::var("parts")]),
                    ),
                    (
                        Arc::from("lastName"),
                        Expr::builtin(
                            BuiltinOp::Join,
                            vec![
                                Expr::builtin(BuiltinOp::Tail, vec![Expr::var("parts")]),
                                Expr::Lit(Literal::Str(" ".into())),
                            ],
                        ),
                    ),
                ]),
            ),
        );
        let expr = Expr::App(
            Box::new(split_fn),
            Box::new(Expr::Lit(Literal::Str("Alice B Smith".into()))),
        );
        let result = eval(&expr, &Env::new(), &default_config());
        let expected = Literal::Record(vec![
            (Arc::from("firstName"), Literal::Str("Alice".into())),
            (Arc::from("lastName"), Literal::Str("B Smith".into())),
        ]);
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn eval_coercion_example() {
        // λv. str_to_int(v)
        let coerce = Expr::lam(
            "v",
            Expr::builtin(BuiltinOp::StrToInt, vec![Expr::var("v")]),
        );
        let expr = Expr::App(
            Box::new(coerce),
            Box::new(Expr::Lit(Literal::Str("42".into()))),
        );
        let result = eval(&expr, &Env::new(), &default_config());
        assert_eq!(result.unwrap(), Literal::Int(42));
    }
}
