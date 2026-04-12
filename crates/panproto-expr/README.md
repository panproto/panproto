# panproto-expr

[![crates.io](https://img.shields.io/crates/v/panproto-expr.svg)](https://crates.io/crates/panproto-expr)
[![docs.rs](https://docs.rs/panproto-expr/badge.svg)](https://docs.rs/panproto-expr)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

A small functional expression language for writing data transforms in migrations.

## What it does

When a schema changes, field values sometimes need to change too: a single `name` field gets split into `firstName` and `lastName`, a numeric `status` code gets converted to a string enum, a price stored in cents needs to be divided by 100. This crate provides the language for writing those transforms: lambdas, pattern matching, let bindings, and about 50 built-in functions covering arithmetic, string manipulation, list operations, and record access.

Expressions are pure (no IO, no mutable state) and deterministic (same input always produces the same output). They are also bounded: a configurable step counter stops runaway loops before they can hang a migration pipeline. The AST serializes to JSON via serde, so expressions stored in the schema VCS travel alongside the schema versions they belong to.

The expression language is also the computational layer for everything beyond simple field remapping: coercion functions, merge logic for combining conflicting values, default value computation, and conflict resolution policies all use `Expr` values under the hood.

## Quick example

```rust,ignore
use panproto_expr::{Expr, Literal, BuiltinOp, Env, EvalConfig, eval};

// \s -> concat(s, "_v2")
let add_suffix = Expr::lam(
    "s",
    Expr::builtin(BuiltinOp::Concat, vec![
        Expr::var("s"),
        Expr::Lit(Literal::Str("_v2".into())),
    ]),
);

let applied = Expr::app(add_suffix, Expr::Lit(Literal::Str("widget".into())));
let result = eval(&applied, &Env::new(), &EvalConfig::default()).unwrap();
// result == Literal::Str("widget_v2")
```

## API overview

| Item | What it does |
|------|-------------|
| `Expr` | Expression AST: `Var`, `Lam`, `App`, `Lit`, `Record`, `List`, `Field`, `Index`, `Match`, `Let`, `Builtin` |
| `Literal` | Leaf values: `Bool`, `Int`, `Float`, `Str`, `Bytes`, `Null`, `Record`, `List`, `Closure` |
| `Pattern` | Destructuring patterns: `Wildcard`, `Var`, `Lit`, `Record`, `List`, `Constructor` |
| `BuiltinOp` | ~50 built-in operations across arithmetic, comparison, strings, lists, records, type coercion, and graph traversal |
| `eval` | Call-by-value evaluator with a step counter, depth limit, and list length limit |
| `EvalConfig` | Evaluation bounds: `max_steps` (default 100,000), `max_depth` (default 256), `max_list_len` (default 10,000) |
| `Env` | Immutable variable environment with lexical scoping |
| `substitute` | Capture-avoiding substitution |
| `free_vars` | Set of free variable names in an expression |
| `apply_builtin` | Apply a built-in operation directly to pre-evaluated arguments |
| `ExprError` | Error type: `StepLimitExceeded`, `DepthExceeded`, `UnboundVariable`, `TypeError`, `ArityMismatch`, and others |

## License

[MIT](../../LICENSE)
