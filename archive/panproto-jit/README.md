# panproto-jit

[![crates.io](https://img.shields.io/crates/v/panproto-jit.svg)](https://crates.io/crates/panproto-jit)
[![docs.rs](https://docs.rs/panproto-jit/badge.svg)](https://docs.rs/panproto-jit)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Compiles panproto expressions to native machine code via LLVM for faster data migration.

## What it does

When you migrate data, field transforms run once per record. For a table with a million rows, even a simple expression like `if x > threshold then x * rate else x` gets evaluated a million times by the default tree-walking interpreter. The interpreter is convenient but carries overhead on every step: dispatch on expression variants, bounds checks, environment lookups.

This crate compiles panproto expressions to native code using LLVM JIT before the migration starts, then hands back a function pointer that runs at full CPU speed. Arithmetic operations lower to `build_int_add` and friends; conditionals lower to `br i1`; lambdas become closure structs with a function pointer; higher-order builtins like `map` and `filter` become loops over an array. The compilation mapping for every `Expr` variant is documented in the `mapping` module.

The actual JIT engine requires the `inkwell-jit` feature flag and a local LLVM installation. Without the feature, the crate still compiles cleanly and exports the mapping specification and error types, so you can depend on it unconditionally and gate the fast path with `#[cfg(feature = "inkwell-jit")]`.

## Quick example

```rust,ignore
#[cfg(feature = "inkwell-jit")]
{
    use panproto_jit::{JitCompiler, CompiledExpr};
    use panproto_expr::Expr;

    let compiler = JitCompiler::new()?;

    // Compile an expression once.
    let expr: Expr = parse_expr("x -> if x > 0.0 then x * 1.1 else x")?;
    let compiled: CompiledExpr = compiler.compile_expr(&expr)?;

    // Execute it per record (no interpreter overhead).
    let result = compiled.execute(&[record_value])?;
}
```

## API overview

| Export | What it does |
|--------|-------------|
| `JitCompiler` | Owns the LLVM context; create once and reuse (`inkwell-jit` feature) |
| `CompiledExpr` | A compiled, callable function pointer for one expression (`inkwell-jit` feature) |
| `ExprMapping` | Documents how each `Expr` variant maps to LLVM IR constructs |
| `JitError` | Error variants: compilation failures, type mismatches, execution errors |

## License

[MIT](../../LICENSE)
