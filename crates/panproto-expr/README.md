# panproto-expr

[![crates.io](https://img.shields.io/crates/v/panproto-expr.svg)](https://crates.io/crates/panproto-expr)
[![docs.rs](https://docs.rs/panproto-expr/badge.svg)](https://docs.rs/panproto-expr)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Expression AST and bounded evaluator used by migrations and schema enrichments.

## Evaluation model

The language provides lambdas, application, lexical `let` bindings, pattern
matching, records, lists, field access, and built-in operations. `BuiltinOp` has
60 variants covering arithmetic, comparison, strings, collections, records,
conversion, hashing, and graph queries. Built-ins are first-class values and support
partial application.

The base evaluator performs no I/O and mutation is confined to its evaluation state.
Graph-query built-ins require an `InstanceResolver`; calling them through the base
`eval` or `apply_builtin` path returns `ExprError::NoInstanceContext`.

Evaluation is bounded by `EvalConfig`. Its defaults are 100,000 steps, depth 256,
and list length 10,000. Reaching a bound returns an error. These limits contain
runaway evaluation; they do not prove termination below the configured bound.

## Example

```rust,ignore
use panproto_expr::{Env, EvalConfig, Expr, Literal, eval};

let expr = Expr::app(
    Expr::lam("x", Expr::var("x")),
    Expr::Lit(Literal::Int(3)),
);
let value = eval(&expr, &Env::new(), &EvalConfig::default())?;
```

## Public API

| Item | Purpose |
|------|---------|
| `Expr`, `Literal`, `Pattern` | Serializable syntax and values |
| `BuiltinOp` | Built-in operation identifier |
| `eval`, `eval_with_resolver` | Evaluate without or with graph context |
| `EvalConfig`, `Env` | Bounds and lexical environment |
| `apply_builtin` | Apply a built-in to evaluated arguments |
| `substitute`, `free_vars` | AST analysis and capture-avoiding substitution |
| `ExprError` | Parse-independent evaluation errors |

## License

[MIT](../../LICENSE)
