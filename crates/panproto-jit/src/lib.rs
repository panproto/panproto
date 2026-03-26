//! # panproto-jit
//!
//! LLVM JIT compilation of panproto expressions for accelerated data migration.
//!
//! The `panproto-expr` evaluator is a tree-walking interpreter with step/depth
//! limits. For large data migrations (millions of records), each
//! `FieldTransform::ApplyExpr(expr)` evaluates per record. LLVM JIT compilation
//! accelerates this by compiling expressions to native code.
//!
//! ## Compilation mapping
//!
//! | `Expr` variant | LLVM IR |
//! |---|---|
//! | `Lit(n)` | `i64` / `f64` / `ptr` constant |
//! | `Var(x)` | Load from environment struct |
//! | `App(f, arg)` | Function call (direct or indirect via closure) |
//! | `Lam(x, body)` | Closure struct (captures as fields) + function pointer |
//! | `If(c, t, e)` | Conditional branch (`br i1`) |
//! | `Let(x, v, body)` | `alloca` + `store` + evaluate body |
//! | `Match(s, arms)` | Switch/cascading `br` based on pattern discrimination |
//! | `Builtin(Add, args)` | `build_int_add()` / `build_float_add()` |
//! | `Builtin(StrConcat, args)` | Call to string runtime function |
//! | `Builtin(Map, [f, list])` | Loop over array, apply closure per element |
//! | `Builtin(Filter, [p, list])` | Conditional loop with output array |
//! | `Builtin(Fold, [f, init, list])` | Accumulator loop |
//! | `Record(fields)` | Struct construction |
//! | `List(elems)` | Array allocation + element stores |
//! | `FieldAccess(base, name)` | Struct GEP or tagged union field lookup |
//!
//! ## Feature flag
//!
//! The actual LLVM JIT compilation requires the `inkwell-jit` feature and LLVM
//! installed locally. Without the feature flag, this crate provides the
//! compilation mapping specification and error types but no runtime compilation.

/// Error types for JIT compilation.
pub mod error;

/// Compilation mapping from `Expr` to LLVM IR constructs.
pub mod mapping;

/// LLVM IR code generation and JIT compilation via inkwell.
#[cfg(feature = "inkwell-jit")]
pub mod codegen;

pub use error::JitError;
pub use mapping::ExprMapping;

#[cfg(feature = "inkwell-jit")]
pub use codegen::{CompiledExpr, JitCompiler};
