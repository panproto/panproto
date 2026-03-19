# panproto-expr

[![crates.io](https://img.shields.io/crates/v/panproto-expr.svg)](https://crates.io/crates/panproto-expr)
[![docs.rs](https://docs.rs/panproto-expr/badge.svg)](https://docs.rs/panproto-expr)

Pure functional expression language for panproto enriched theories.

A small lambda calculus with pattern matching, records, lists, and ~50 built-in operations on strings, numbers, and collections. Expressions are the computational substrate for schema transforms: coercion functions, merge/split logic, default value computation, and conflict resolution policies. Evaluation is deterministic on native and WASM with configurable step and depth limits.

## API

| Item | Description |
|------|-------------|
| `Expr` | Expression AST: Var, Lam, App, Lit, Record, List, Field, Index, Match, Let, Builtin |
| `Pattern` | Destructuring patterns: Wildcard, Var, Lit, Record, List, Constructor |
| `BuiltinOp` | ~50 built-in operations across arithmetic, comparison, boolean, string, list, record, type coercion, and inspection |
| `Literal` | Values: Bool, Int, Float, Str, Bytes, Null, Record, List, Closure |
| `eval` | Call-by-value evaluator with step counter, depth limit, and list length limit |
| `EvalConfig` | Evaluation bounds: `max_steps` (default 100,000), `max_depth` (default 256), `max_list_len` (default 10,000) |
| `Env` | Immutable variable environment with lexical scoping |
| `substitute` | Capture-avoiding substitution |
| `free_vars` | Free variable analysis |
| `pattern_vars` | Variables bound by a pattern |
| `apply_builtin` | Direct application of a built-in operation to evaluated arguments |
| `ExprError` | Error type: StepLimitExceeded, DepthExceeded, UnboundVariable, TypeError, ArityMismatch, etc. |

## Example

```rust,ignore
use panproto_expr::{Expr, Literal, BuiltinOp, Env, EvalConfig, eval};

// λfirst. λlast. concat(first, concat(" ", last))
let merge_fn = Expr::lam("first", Expr::lam("last",
    Expr::builtin(BuiltinOp::Concat, vec![
        Expr::var("first"),
        Expr::builtin(BuiltinOp::Concat, vec![
            Expr::Lit(Literal::Str(" ".into())),
            Expr::var("last"),
        ]),
    ]),
));

let expr = Expr::app(Expr::app(merge_fn, Expr::Lit(Literal::Str("Alice".into()))),
                      Expr::Lit(Literal::Str("Smith".into())));
let result = eval(&expr, &Env::new(), &EvalConfig::default()).unwrap();
assert_eq!(result, Literal::Str("Alice Smith".into()));
```

## Design

- **Pure**: no IO, no mutable state, no randomness. Same inputs always produce the same output.
- **Deterministic**: IEEE 754 floats with `f64::total_cmp` for comparisons.
- **Serializable**: `Expr` derives `Serialize`/`Deserialize` for storage in the VCS alongside schemas.
- **Bounded**: step counter prevents runaway computation; depth counter prevents stack overflow.
- **WASM-safe**: no threads, no OS deps, no `std::fs`.
- **Closures**: lambdas evaluate to `Literal::Closure` values capturing the lexical environment.

## License

[MIT](../../LICENSE)
