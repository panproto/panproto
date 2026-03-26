# panproto-jit

LLVM JIT compilation of panproto expressions for accelerated data migration.

## Overview

The `panproto-expr` evaluator is a tree-walking interpreter. For large data migrations (millions of records), each `FieldTransform::ApplyExpr(expr)` evaluates per record. This crate compiles expressions to native code via LLVM for acceleration.

## JIT-compiled operations

| Category | Operations | LLVM IR |
|---|---|---|
| Arithmetic | add, sub, mul, div, mod, neg, abs | Direct instructions |
| Comparison | eq, neq, lt, lte, gt, gte | `icmp` instructions |
| Boolean | and, or, not | Bitwise instructions |
| Coercions | int_to_float, float_to_int | `sitofp`/`fptosi` |
| Rounding | floor, ceil | Comparison + adjust |
| Control | let bindings, pattern match | Alloca + phi nodes |

Operations requiring heap allocation (strings, lists, records, graph traversal) return `JitError::Unsupported` and should use the interpreter.

## Compilation mapping

The `mapping` module classifies all 50 builtins into `ExprMapping` variants: `ArithmeticOp` (direct LLVM instruction), `ArrayLoop` (map/filter/fold compiled as loops), or `RuntimeCall` (requires runtime support functions).

## Features

- `inkwell-jit` (default): enables LLVM JIT compilation via inkwell. Requires LLVM 20 installed.

## License

MIT
