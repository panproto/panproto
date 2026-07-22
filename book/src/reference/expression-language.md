# Expression-language reference

The `panproto-expr` language is pure, total, and bounded: every well-typed expression terminates within a fixed step budget, no IO is permitted, and evaluation is deterministic. It is used inside migrations to describe field-level transforms, and inside queries to describe predicates and projections.

For the model behind the language, see [Expression language: denotational semantics](../explanation/semantics/expression-language.md).

## Surface syntax

The Haskell-style surface syntax supports literals, variables, lambdas, application, `let`, pattern matching, and list comprehensions. The full grammar lives in [`panproto-expr-parser`](https://github.com/panproto/panproto/tree/main/crates/panproto-expr-parser). Inside JSON or YAML migration files, expressions appear as strings; the parser is invoked on read.

## Types

| Type | Description |
|---|---|
| `Int` | 64-bit signed integer with checked arithmetic. Overflow is an error, not a wrap. |
| `Float` | IEEE 754 64-bit float. |
| `Str` | UTF-8 string. |
| `Bool` | Boolean. |
| `List a` | Heterogeneous list (the type parameter is the element constraint when known). |
| `Record` | Map from string keys to values. |
| `Null` | Singleton type for the absence of a value. |
| `Any` | Top type used in builtins that accept any input. |

## Builtins

The 60 built-in operations are in `panproto_expr::BuiltinOp`, grouped by family in `crates/panproto-expr/src/builtin.rs`.

### Arithmetic
`Add`, `Sub`, `Mul`, `Div`, `Mod`, `Neg`, `Abs`, `Floor`, `Ceil`, `Round`. `Div` and `Mod` raise `DivisionByZero` on a zero divisor. Integer arithmetic uses checked operations; overflow raises `Overflow`.

### Comparison
`Eq`, `Neq`, `Lt`, `Lte`, `Gt`, `Gte`. Returns `Bool`.

### Boolean
`And`, `Or`, `Not`. Short-circuit evaluation.

### String
`Concat`, `Len`, `Slice`, `Upper`, `Lower`, `Trim`, `Split`, `Join`, `Replace`, `Contains`. `Contains` is overloaded on its first argument: substring containment on a string, element membership on a list.

### List
`Map`, `Filter`, `Fold`, `FlatMap`, `Append`, `Head`, `Tail`, `Reverse`, `Length`, `Range`. `Head` and `Tail` raise on the empty list.

`Map`, `Filter`, `Fold`, and `FlatMap` take the list first and the function last in the abstract syntax, while the surface syntax names the function first (`map f xs`, `fold f z xs`); the parser permutes between the two. `Expr` is serialized into stored lens documents, so the abstract order is the compatibility-bearing one.

`Range` constructs `[start, ..., stop]` with both bounds inclusive, yielding the empty list when `stop < start`. The surface syntax `[a..b]` lowers to it. Its length is checked against `max_list_len` before allocating, since it is the only builtin that turns a constant-size expression into an arbitrarily long list. Open-ended `[a..]` is rejected: there are no lazy lists to lower it to.

### Record
`MergeRecords`, `Keys`, `Values`, `HasField`.

### Utility
`DefaultVal`, `Clamp`, `TruncateStr`.

### Type coercion
`IntToFloat`, `FloatToInt`, `IntToStr`, `FloatToStr`, `StrToInt`, `StrToFloat`. The `*ToInt` and `*ToFloat` parses raise `ParseError` on invalid input.

### Type inspection
`TypeOf`, `IsNull`, `IsList`.

### Graph traversal (instance-aware)
`Edge`, `Children`, `HasEdge`, `EdgeCount`, `Anchor`. These return `Null` in the standard evaluator. To use them, evaluate in an instance environment via `panproto_inst::instance_env`.

## Errors

The full list of `ExprError` variants is in [`crates/panproto-expr/src/error.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/error.rs).

| Variant | Cause |
|---|---|
| `StepLimitExceeded(u64)` | Evaluation exceeded the configured step budget (`EvalConfig::max_steps`, default 100,000). |
| `DepthExceeded(u32)` | Evaluation exceeded the configured recursion depth. |
| `UnboundVariable(String)` | Referenced a variable not bound in the environment. |
| `TypeError { expected, got }` | Operand had an incompatible type. |
| `ArityMismatch { op, expected, got }` | Builtin called with the wrong number of arguments. |
| `IndexOutOfBounds { index, len }` | List index out of range. |
| `FieldNotFound(String)` | Record field missing. |
| `NonExhaustiveMatch` | No pattern arm matched the scrutinee. |
| `DivisionByZero` | `Div` or `Mod` with a zero divisor. |
| `ListLengthExceeded(usize)` | List operation exceeded the configured maximum length. |
| `ParseError { value, target_type }` | A `*ToInt` / `*ToFloat` coercion failed to parse its input. |
| `NotAFunction` | Application of a non-function value. |
| `Overflow` | Checked integer arithmetic overflowed. |
| `FloatNotRepresentable(String)` | Float value (NaN, infinity, out-of-range) cannot be represented as an integer. |

## Authoritative source

Builtin enum: [`crates/panproto-expr/src/expr.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/expr.rs). Implementation: [`crates/panproto-expr/src/builtin.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/builtin.rs). Evaluator: [`crates/panproto-expr/src/eval.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/eval.rs).

## See also

- [Apply field transforms](../how-to/field-transforms.md) for using the language inside migrations.
- [Query instances](../how-to/query-instances.md) for using it inside queries.
- [Expression language: denotational semantics](../explanation/semantics/expression-language.md) for the formal model.
